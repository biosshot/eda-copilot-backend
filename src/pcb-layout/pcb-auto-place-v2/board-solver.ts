import * as fp from '../f32.ts';
import type {
    BlockRole,
    BoardEdge,
    Box,
    PcbComponent,
    PlacementGraph,
    PlacementInput,
    PlacementRelation,
    PlacementTreeNode,
} from '#types/pcb/layout-model.ts';
import { isGroundSignal } from '#circuit-layout/ground.ts';
import { isPowerSignal } from '#circuit-layout/power.ts';
import { componentBox } from '../pcb-auto-place/geometry.ts';
import { priorityWeight } from '../pcb-auto-place/hints.ts';
import type { ClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { solveBoardPackedPrimitives } from './board-packer-engine.ts';
import type { PlacementPrimitive, PrimitiveSolveDiagnostic } from './primitives.ts';
import { selectBlockPortfolio } from './block-portfolio.ts';
import { boardAlignmentPolicy } from './board-alignment.ts';
import { boardSpacingPolicy, boardSpacingExemptPairs } from './board-spacing.ts';

const ORDINARY_NET_RELATION_PREFIX = '__ordinary_net__:';

export interface BoardSolveParams {
    input: PlacementInput;
    graph: PlacementGraph;
    node: PlacementTreeNode;
    childPrimitives: PlacementPrimitive[];
    grid: number;
    clearance: number;
    componentByDesignator: Map<string, PcbComponent>;
    blockRoleByName: Map<string, BlockRole>;
    clearanceResolver: ClearanceResolver;
    compactness: 'normal' | 'high';
    diagnostics: PrimitiveSolveDiagnostic[];
}

export function solveBoardPrimitives(params: BoardSolveParams) {
    const spacing = boardSpacingPolicy(params.input);
    params.diagnostics.push({severity:'warning',nodeId:params.node.id,
        message:`Board soft spacing: ${spacing.gap.toFixed(2)}mm extra, density ${spacing.density.toFixed(3)}, compactness scale ${spacing.compactnessScale.toFixed(3)}`});
    const boardPrimitives = boardPlacementPrimitives(params);
    validateDissolvedGroupReferences(params.graph.relations, boardPrimitives.dissolvedScopes);
    const primitives = boardPrimitives.primitives.map(boardPackingPrimitive);
    const alignment = boardAlignmentPolicy(params.input, primitives);
    const packed = solveBoardPackedPrimitives({
        node: params.node,
        primitives,
        relations: [
            ...params.graph.relations.filter((relation) => (
                relation.scope === params.node.id
                || boardPrimitives.dissolvedScopes.has(relation.scope)
            )),
            ...dissolvedSatelliteRelations(params, boardPrimitives.dissolvedScopes),
            ...ordinaryNetRelations(params, primitives),
        ],
        options: {
            softAlignment: alignment.pairs.length ? alignment : undefined,
            softSpacing: {...spacing,exemptPairs:boardSpacingExemptPairs(params.input,primitives)},
            grid: params.grid,
            clearance: params.clearance,
            componentByDesignator: params.componentByDesignator,
            clearanceResolver: params.clearanceResolver,
            board: params.input.board,
            edgeClearance: params.input.board.clearances.edge,
            bounds: boardBounds(params.input),
            obstacles: boardHoleBoxes(params.input),
            constraintRegions: params.input.constraintRegions,
            compactness: params.compactness,
            searchWidth: 32,
        },
    });
    params.diagnostics.push({severity:'warning',nodeId:params.node.id,
        message:`Board search: one complete packing with weak alignment for ${alignment.pairs.length} structural pairs`});
    return selectBlockPortfolio(params.input,packed,params.grid,params.diagnostics);
}

/**
 * Ordinary electrical connectivity is intentionally represented as zero-endpoint
 * marker relations. The native board scorer consumes these markers separately;
 * the normal strong relation scorer cannot resolve their endpoints and therefore
 * never turns ordinary nets into hard/strong placement constraints.
 */
function ordinaryNetRelations(params: BoardSolveParams, primitives: PlacementPrimitive[]): PlacementRelation[] {
    const ignored = new Set((params.input.solverOptions.ignoredRatsnestSignals ?? []).map((net) => net.toUpperCase()));
    const primitiveIdsByNet = new Map<string, Set<string>>();

    for (const primitive of primitives) {
        for (const point of primitive.connectionPoints) {
            const net = point.net?.trim();
            if (!net || ignored.has(net.toUpperCase()) || isGroundSignal(net)) continue;
            const ids = primitiveIdsByNet.get(net) ?? new Set<string>();
            ids.add(primitive.id);
            primitiveIdsByNet.set(net, ids);
        }
    }

    return [...primitiveIdsByNet.entries()]
        .filter(([, primitiveIds]) => primitiveIds.size >= 2 && primitiveIds.size <= 8)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([net]): PlacementRelation => ({
            id: `${ORDINARY_NET_RELATION_PREFIX}${net}`,
            kind: 'net',
            from: `${ORDINARY_NET_RELATION_PREFIX}${net}`,
            to: `${ORDINARY_NET_RELATION_PREFIX}${net}`,
            weight: isPowerSignal(net) ? 0.1 : 1,
            scope: params.node.id,
            effect: 'score_only',
        }));
}

function validateDissolvedGroupReferences(relations: PlacementRelation[], dissolvedScopes: Set<string>) {
    const errors = new Set<string>();
    for (const relation of relations) {
        for (const endpoint of [relation.from, relation.to]) {
            if (!endpoint.startsWith('block:') && !endpoint.startsWith('module:')) continue;
            if (!dissolvedScopes.has(`tree:${endpoint}`)) continue;
            errors.add(`- ${relation.relation ?? relation.kind} (${relation.id}) references dissolved ${endpoint}.`);
        }
    }
    if (errors.size === 0) return;
    throw new Error([
        'Invalid PCB placement DSL:',
        'Placement constraints cannot reference blocks or modules dissolved for board placement.',
        ...errors,
        'Use comp("designator") or pin("designator", "pin_number") to target a specific component or pad instead of the dissolved group.',
    ].join('\n'));
}

function dissolvedSatelliteRelations(params: BoardSolveParams, dissolvedScopes: Set<string>): PlacementRelation[] {
    return params.input.blocks.flatMap((block): PlacementRelation[] => {
        if (!block.attachTo || block.anchor || !dissolvedScopes.has(`tree:block:${block.attachTo}`)) return [];
        const parent = params.input.blocks.find((candidate) => candidate.name === block.attachTo);
        if (!parent) return [];
        // edgePlace must position the connector body, so its family is dissolved.
        // attached_to only builds the tree; restore proximity using component endpoints
        // because the dissolved parent block no longer exists in the board packer.
        return parent.component_designators.map((designator) => ({
            id: `satellite:${block.name}:parent:${designator}`,
            kind: 'hint',
            from: `block:${block.name}`,
            to: `component:${designator}`,
            relation: 'near',
            priority: 'high',
            weight: fp.div(fp.mul(priorityWeight('high'), 2), parent.component_designators.length),
            scope: params.node.id,
            effect: 'move_from',
        }));
    });
}

function boardPackingPrimitive(primitive: PlacementPrimitive): PlacementPrimitive {
    return {
        ...primitive,
        collisionBoxes: primitive.collisionBoxes?.length ? primitive.collisionBoxes : [primitive.bbox],
    };
}

function boardPlacementPrimitives(params: BoardSolveParams) {
    const bounds = boardBounds(params.input);
    const dissolvedScopes = new Set<string>();
    const primitives: PlacementPrimitive[] = [];

    for (const primitive of params.childPrimitives) {
        if (primitive.anchored) {
            primitives.push(primitive);
            continue;
        }
        if (shouldDissolveEdgePlacePrimitive(params, primitive)) {
            primitives.push(...expandDissolvedChildren(params, primitive.children, dissolvedScopes));
            dissolvedScopes.add(primitive.sourceNodeId);
            params.diagnostics.push({
                severity: 'warning',
                nodeId: primitive.sourceNodeId,
                message: `Dissolved edge-place mechanical group ${primitive.label} before board placement`,
            });
            continue;
        }

        const decision = moduleDissolveDecision(params, primitive, bounds);
        if (!decision.dissolve) {
            primitives.push(primitive);
            continue;
        }

        primitives.push(...expandDissolvedChildren(params, primitive.children, dissolvedScopes));
        dissolvedScopes.add(primitive.sourceNodeId);
        params.diagnostics.push({
            severity: 'warning',
            nodeId: primitive.sourceNodeId,
            message: `Dissolved sparse module ${primitive.label} before board placement: ${decision.reasons.join('; ')}`,
        });
    }

    return { primitives, dissolvedScopes };
}

function expandDissolvedChildren(
    params: BoardSolveParams,
    children: PlacementPrimitive[],
    dissolvedScopes: Set<string>,
): PlacementPrimitive[] {
    const result: PlacementPrimitive[] = [];
    for (const child of children) {
        if (!shouldDissolveEdgePlacePrimitive(params, child)) {
            result.push(child);
            continue;
        }
        dissolvedScopes.add(child.sourceNodeId);
        params.diagnostics.push({
            severity: 'warning',
            nodeId: child.sourceNodeId,
            message: `Dissolved edge-place mechanical group ${child.label} before board placement`,
        });
        result.push(...expandDissolvedChildren(params, child.children, dissolvedScopes));
    }
    return result;
}

function shouldDissolveEdgePlacePrimitive(params: BoardSolveParams, primitive: PlacementPrimitive): boolean {
    if (primitive.kind === 'component' || primitive.children.length === 0) return false;
    return primitive.placements.some((placement) => params.componentByDesignator.get(placement.designator)?.pcb.edgePlace)
        || primitive.children.some((child) => shouldDissolveEdgePlacePrimitive(params, child));
}

function moduleDissolveDecision(
    params: BoardSolveParams,
    primitive: PlacementPrimitive,
    bounds: Box,
): { dissolve: boolean; reasons: string[] } {
    if (primitive.kind !== 'module' || primitive.children.length <= 1) return { dissolve: false, reasons: [] };

    const usableWidth = fp.sub(bounds.right, bounds.left);
    const usableHeight = fp.sub(bounds.bottom, bounds.top);
    const boardArea = fp.max(1, fp.mul(usableWidth, usableHeight));
    const width = fp.max(0, primitive.width);
    const height = fp.max(0, primitive.height);
    const area = fp.max(1, fp.mul(width, height));
    const fillDensity = moduleFillDensity(primitive);
    const padDensity = modulePadDensity(params, primitive);
    const aspectRatio = height > 0 && width > 0 ? fp.max(fp.div(width, height), fp.div(height, width)) : Infinity;
    const fixedEdges = moduleFixedEdges(params, primitive, bounds);

    const reasons: string[] = [];
    if (width > fp.mul(usableWidth, 0.82)) reasons.push(`wide module ${round(width)}mm uses ${round(fp.div(width, usableWidth))} of usable board width`);
    if (height > fp.mul(usableHeight, 0.82)) reasons.push(`tall module ${round(height)}mm uses ${round(fp.div(height, usableHeight))} of usable board height`);
    if (width > fp.mul(usableWidth, 0.45) && fillDensity < 0.65) reasons.push(`wide sparse bbox ${round(width)}mm, fill ${round(fillDensity)}`);
    if (height > fp.mul(usableHeight, 0.45) && fillDensity < 0.65) reasons.push(`tall sparse bbox ${round(height)}mm, fill ${round(fillDensity)}`);
    if (area > fp.mul(boardArea, 0.28) && fillDensity < 0.7) reasons.push(`module area ${round(area)}mm2 is large and sparse, fill ${round(fillDensity)}`);
    if (aspectRatio > 1.9 && fillDensity < 0.72) reasons.push(`stretched aspect ratio ${round(aspectRatio)}`);
    if (primitive.locked && fixedEdges.size >= 1 && primitive.children.length >= 2) {
        reasons.push(`locked module has fixed/edge child plus movable siblings (${[...fixedEdges].join(', ')})`);
    }
    if (fixedEdges.size >= 1 && primitive.children.length >= 3) reasons.push(`fixed/edge-mounted children inside multi-family module (${[...fixedEdges].join(', ')})`);
    if (fixedEdges.size >= 2) reasons.push(`fixed/edge-mounted children target multiple board edges (${[...fixedEdges].join(', ')})`);
    if (padDensity < 0.09 && (width > fp.mul(usableWidth, 0.35) || height > fp.mul(usableHeight, 0.35))) {
        reasons.push(`low pad density ${round(padDensity)} pads/mm2`);
    }

    return { dissolve: reasons.length > 0, reasons };
}

function moduleFillDensity(primitive: PlacementPrimitive) {
    const moduleArea = fp.max(1, fp.mul(primitive.width, primitive.height));
    const boxes = primitive.children.length > 0 ? primitive.children.map((child) => child.bbox) : [primitive.bbox];
    const occupied = boxes.reduce((sum, box) => fp.add(sum, boxArea(box)), 0);
    return fp.min(1, fp.div(occupied, moduleArea));
}

function modulePadDensity(params: BoardSolveParams, primitive: PlacementPrimitive) {
    const moduleArea = fp.max(1, fp.mul(primitive.width, primitive.height));
    const padCount = primitive.placements.reduce((sum, placement) => {
        const component = params.componentByDesignator.get(placement.designator);
        return fp.add(sum, (component?.pins.length ?? 0));
    }, 0);
    return fp.div(padCount, moduleArea);
}

function moduleFixedEdges(params: BoardSolveParams, primitive: PlacementPrimitive, bounds: Box) {
    const edges = new Set<BoardEdge>();
    for (const placement of primitive.placements) {
        const component = params.componentByDesignator.get(placement.designator);
        if (!component) continue;
        const edge = component.pcb.edgeMount?.edge
            ?? component.pcb.edgePlace?.edges[0]
            ?? fixedPlacementEdge(component.pcb.fixedPlacement?.anchor?.anchor);
        if (edge) {
            edges.add(edge);
            continue;
        }
        if (!component.pcb.fixedPlacement) continue;
        const box = componentBox(component, placement);
        const nearest = nearestBoardEdge(box, bounds);
        if (nearest.distance <= fp.max(2, fp.add(params.input.board.clearances.edge, 1))) edges.add(nearest.edge);
    }
    return edges;
}

function fixedPlacementEdge(anchor: unknown): BoardEdge | null {
    if (anchor === 'board.left' || anchor === 'left') return 'left';
    if (anchor === 'board.right' || anchor === 'right') return 'right';
    if (anchor === 'board.top' || anchor === 'top') return 'top';
    if (anchor === 'board.bottom' || anchor === 'bottom') return 'bottom';
    if (anchor === 'board.top_left') return 'top';
    if (anchor === 'board.top_right') return 'top';
    if (anchor === 'board.bottom_left') return 'bottom';
    if (anchor === 'board.bottom_right') return 'bottom';
    return null;
}

function nearestBoardEdge(box: Box, bounds: Box) {
    const distances: Array<{ edge: BoardEdge; distance: number }> = [
        { edge: 'left', distance: fp.abs(fp.sub(box.left, bounds.left)) },
        { edge: 'right', distance: fp.abs(fp.sub(bounds.right, box.right)) },
        { edge: 'top', distance: fp.abs(fp.sub(box.top, bounds.top)) },
        { edge: 'bottom', distance: fp.abs(fp.sub(bounds.bottom, box.bottom)) },
    ];
    return distances.sort((a, b) => fp.sub(a.distance, b.distance))[0];
}

function boardBounds(input: PlacementInput): Box {
    const edge = input.board.clearances.edge ?? 0;
    return {
        left: fp.add(fp.div(-input.board.outline.width, 2), edge),
        right: fp.sub(fp.div(input.board.outline.width, 2), edge),
        top: fp.add(fp.div(-input.board.outline.height, 2), edge),
        bottom: fp.sub(fp.div(input.board.outline.height, 2), edge),
    };
}

function boardHoleBoxes(input: PlacementInput): Box[] {
    return (input.boardHoles ?? []).map((hole) => {
        const radius = fp.max(hole.keepout, fp.div(hole.diameter, 2), fp.div(hole.drill, 2));
        return {
            left: fp.sub(hole.x, radius),
            right: fp.add(hole.x, radius),
            top: fp.sub(hole.y, radius),
            bottom: fp.add(hole.y, radius),
        };
    });
}

function boxArea(box: Box) {
    return fp.mul(fp.max(0, fp.sub(box.right, box.left)), fp.max(0, fp.sub(box.bottom, box.top)));
}

function round(value: number) {
    return fp.div(Math.round(fp.mul(value, 1000)), 1000);
}
