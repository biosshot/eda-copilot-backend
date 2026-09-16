import type {
    PcbComponent,
    Placement,
    PlacementInput,
    PlacementRelation,
    PlacementTreeNode,
} from '#types/pcb/layout-model.ts';
import { isConnectedSignalName } from '#utils/signals.ts';
import {
    boardBox,
    componentBox,
    componentPadBox,
    getPadWorld,
    isThroughHolePad,
} from '../pcb-auto-place/geometry.ts';
import { createClearanceResolver, type ClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { buildPlacementGraph } from '../pcb-auto-place/placement-graph.ts';
import type { PlacementPrimitive } from './primitives.ts';
import { encodeNativeBoardPackProblem } from './native/encode-board-problem.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';

export type PostPlaceRouteScoreContext = {
    relations: PlacementRelation[];
    componentByDesignator: Map<string, PcbComponent>;
    clearanceResolver: ClearanceResolver;
};

export function createPostPlaceRouteScoreContext(input: PlacementInput): PostPlaceRouteScoreContext {
    const graph = buildPlacementGraph(input);
    return {
        relations: graph.relations,
        componentByDesignator: new Map(input.components.map((component) => [component.designator, component])),
        clearanceResolver: createClearanceResolver(input),
    };
}

/**
 * Route-aware score used only for local post-place variants. The native
 * Micro-A* receives component-level primitives so endpoint carving never makes
 * unrelated siblings in a larger block/macro transparent.
 */
export function postPlaceRoutePenalty(
    input: PlacementInput,
    placements: Placement[],
    changedDesignators: Set<string>,
    context: PostPlaceRouteScoreContext,
) {
    if (changedDesignators.size === 0) return 0;
    const placementByDesignator = new Map(placements.map((placement) => [placement.designator, placement]));
    const primitives = input.components.flatMap((component) => {
        const placement = placementByDesignator.get(component.designator);
        return placement ? [componentPrimitive(component, placement)] : [];
    });
    if (primitives.length < 2) return 0;

    const node: PlacementTreeNode = {
        id: 'tree:board:post-place-route-score',
        kind: 'board',
        label: 'post-place-route-score',
        children: [],
    };
    const problem = encodeNativeBoardPackProblem({
        node,
        primitives,
        relations: context.relations,
        options: {
            grid: input.solverOptions.placementGridStep ?? 0.5,
            clearance: input.board.clearances.component,
            componentByDesignator: context.componentByDesignator,
            clearanceResolver: context.clearanceResolver,
            board: input.board,
            edgeClearance: input.board.clearances.edge,
            bounds: boardBox(input.board),
            obstacles: boardHoleBoxes(input),
            // constraintRegion is a placement rule, not a routing keepout.
            constraintRegions: [],
            compactness: input.solverOptions.compactness ?? 'normal',
            searchWidth: 32,
        },
    });
    const changedPrimitiveIds = [...changedDesignators].map((designator) => `post:${designator}`);
    const routingObstacles = input.components.flatMap((component) => {
        const placement = placementByDesignator.get(component.designator);
        if (!placement) return [];
        return component.footprint.pads.map((pad) => {
            const pin = component.pins.find((candidate) => String(candidate.pin_number) === String(pad.pin_number));
            const net = pin && isConnectedSignalName(pin.signal_name) ? pin.signal_name : undefined;
            return {
                box: componentPadBox(placement, pad),
                layer: isThroughHolePad(pad) ? undefined : placement.layer,
                ref: `${component.designator}.${String(pad.pin_number)}`,
                net,
                primitiveId: `post:${component.designator}`,
            };
        });
    });
    return loadNativeBoardPacker().scoreRouteLayoutWithObstacles(problem, changedPrimitiveIds, routingObstacles);
}

function componentPrimitive(component: PcbComponent, placement: Placement): PlacementPrimitive {
    const bbox = componentBox(component, placement);
    const connectionPoints = component.pins.flatMap((pin) => {
        if (!isConnectedSignalName(pin.signal_name)) return [];
        const point = getPadWorld(component, placement, pin.pin_number);
        return point ? [{
            ref: `${component.designator}.${String(pin.pin_number)}`,
            net: pin.signal_name,
            x: point.x,
            y: point.y,
        }] : [];
    });
    return {
        id: `post:${component.designator}`,
        kind: 'component',
        label: component.designator,
        sourceNodeId: `tree:component:${component.designator}`,
        locked: true,
        canRotate: false,
        allowedOrientations: [0],
        bbox,
        collisionBoxes: [bbox],
        width: bbox.right - bbox.left,
        height: bbox.bottom - bbox.top,
        placements: [{ ...placement }],
        connectionPoints,
        pathPorts: [],
        children: [],
    };
}

function boardHoleBoxes(input: PlacementInput) {
    return (input.boardHoles ?? []).map((hole) => {
        const radius = Math.max(hole.keepout, hole.diameter / 2, hole.drill / 2);
        return {
            left: hole.x - radius,
            right: hole.x + radius,
            top: hole.y - radius,
            bottom: hole.y + radius,
        };
    });
}
