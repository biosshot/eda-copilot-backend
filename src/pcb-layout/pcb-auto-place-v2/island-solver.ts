import * as fp from '../f32.ts';
import type {
    Box,
    Layer,
    PcbComponent,
    Placement,
    PlacementGraph,
    PlacementInput,
    PlacementTreeNode,
    Point,
} from '#types/pcb/layout-model.ts';
import {
    componentBox,
    componentPadBox,
    componentPairCollisionBoxPairs,
    dist,
    getPadOffset,
    isThroughHolePad,
    overlaps,
    pointsBox,
    roundPlacement,
    rotatedSize,
    unionBoxes,
} from '../pcb-auto-place/geometry.ts';
import type { ClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { createClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { placementsCanConflict, segmentIntersectsBox } from '../pcb-auto-place/utils.ts';
import { placementPadCrossingWeight } from './block-policy.ts';

type IslandKind = 'line' | 'bypass' | 'cap_cluster' | 'core_pairs';

export interface IslandSolverOptions {
    grid?: number;
    clearance?: number;
}

export interface IslandSolveDiagnostic {
    severity: 'warning' | 'error';
    message: string;
}

export interface IslandSolveResult {
    islandId: string;
    label: string;
    kind: IslandKind;
    scope: string;
    placements: Placement[];
    bbox: Box;
    width: number;
    height: number;
    area: number;
    score: number;
    diagnostics: IslandSolveDiagnostic[];
}

type CandidatePlacement = Placement & { box: Box; component: PcbComponent };

type IslandNode = PlacementTreeNode & {
    kind: 'island';
    data: Record<string, unknown> & {
        kind: IslandKind;
        components?: string[];
    };
};

export function solvePlacementIslands(
    input: PlacementInput,
    graph: PlacementGraph,
    options: IslandSolverOptions = {},
): IslandSolveResult[] {
    const componentByDesignator = new Map(input.components.map((component) => [component.designator, component]));
    const grid = options.grid ?? input.solverOptions.placementGridStep ?? 0.5;
    const clearance = options.clearance ?? input.board.clearances.component ?? 0.8;
    const clearanceResolver = createClearanceResolver(input);
    return collectIslandNodes(graph.root)
        .map((island) => solveIsland(island, input, componentByDesignator, grid, clearance, clearanceResolver))
        .filter((result): result is IslandSolveResult => Boolean(result));
}

function solveIsland(
    island: IslandNode,
    input: PlacementInput,
    componentByDesignator: Map<string, PcbComponent>,
    grid: number,
    clearance: number,
    clearanceResolver: ClearanceResolver,
): IslandSolveResult | null {
    const components = island.data.components
        ?.map((designator) => componentByDesignator.get(designator))
        .filter((component): component is PcbComponent => Boolean(component)) ?? [];
    if (components.length === 0) return null;

    const diagnostics: IslandSolveDiagnostic[] = [];
    const kind = island.data.kind;
    const placements = kind === 'cap_cluster'
        ? solveCapCluster(island, components, input, grid, clearance, clearanceResolver)
        : kind === 'core_pairs' && components.length === 2
            ? solveTwoComponentCorePairs(island, components, grid, clearance, clearanceResolver)
        : kind === 'bypass'
            ? solveBypass(island, input, components, grid, clearance, clearanceResolver)
        : kind === 'line'
            ? solveOrderedIsland(island, input, components, grid, clearance, clearanceResolver)
            : solveCompactIsland(island, components, grid, clearance, clearanceResolver);

    const bbox = placementsBox(components, placements);
    const score = scorePlacements(island, components, placements, clearanceResolver);
    if (hasOverlap(components, placements, clearanceResolver)) {
        diagnostics.push({ severity: 'error', message: 'Island placements overlap after compact solve' });
    }

    return {
        islandId: island.id,
        label: island.label,
        kind,
        scope: island.ref ?? island.id,
        placements: placements.map(({ designator, x, y, rotate, layer, score }) => ({ designator, x, y, rotate, layer, score })),
        bbox,
        width: roundPlacement(fp.sub(bbox.right, bbox.left)),
        height: roundPlacement(fp.sub(bbox.bottom, bbox.top)),
        area: roundPlacement(fp.mul((fp.sub(bbox.right, bbox.left)), (fp.sub(bbox.bottom, bbox.top)))),
        score: roundPlacement(score),
        diagnostics,
    };
}

function solveCapCluster(island: IslandNode, components: PcbComponent[], input: PlacementInput, grid: number, clearance: number, clearanceResolver: ClearanceResolver): CandidatePlacement[] {
    const axis = island.data.axis === 'x' || island.data.axis === 'y' ? island.data.axis : null;
    const maxRows = island.data.maxRows === 1 || island.data.maxRows === 2 ? island.data.maxRows : 2;
    const topology = island.data.topology === 'center_power_bus' ? 'center_power_bus' : 'edge_bus';
    const gap = typeof island.data.gap === 'number' ? fp.max(island.data.gap, clearance) : clearance;
    const rowGap = typeof island.data.rowGap === 'number' ? fp.max(island.data.rowGap, clearance) : gap;
    const axes: Array<'x' | 'y'> = axis ? [axis] : ['x', 'y'];
    const rotations = normalizedRotations(components[0]);
    const variants: CandidatePlacement[][] = [];

    for (const candidateAxis of axes) {
        const powerSide = targetPinPowerSide(island, input, candidateAxis);
        for (let rows = 1; rows <= maxRows; rows += 1) {
            const maxPerRow = typeof island.data.maxPerRow === 'number'
                ? Math.max(1, Math.floor(island.data.maxPerRow))
                : Math.ceil(fp.div(components.length, rows));
            const actualRows = Math.ceil(fp.div(components.length, maxPerRow));
            if (actualRows > maxRows) continue;
            if (topology === 'center_power_bus' && rows === 1 && maxRows >= 2 && Math.ceil(fp.div(components.length, maxPerRow)) >= 2) {
                // a two-row center_power_bus variant will be generated at rows === 2
                continue;
            }
            if (topology === 'center_power_bus' && rows === 2 && actualRows === 2) {
                variants.push(placeCenterPowerBus(components, candidateAxis, maxPerRow, grid, gap, rowGap, island, powerSide));
            } else if (actualRows === 1) {
                variants.push(placeRowWithPowerSide(components, candidateAxis, powerSide, gap, grid, island));
            } else {
                for (const rotate of rotationsForAxis(rotations, components, island, candidateAxis)) {
                    variants.push(placeGrid(components, candidateAxis, maxPerRow, rotate, grid, clearance, gap, rowGap));
                }
            }
        }
    }

    return bestVariant(island, components, variants, clearance, clearanceResolver);
}

function solveOrderedIsland(island: IslandNode, input: PlacementInput, components: PcbComponent[], grid: number, clearance: number, clearanceResolver: ClearanceResolver): CandidatePlacement[] {
    const axis = island.data.axis === 'x' || island.data.axis === 'y' ? island.data.axis : chooseCompactAxis(components);
    const rotate = bestLineRotation(components, axis, island, input, clearance, clearanceResolver);
    return placeGrid(components, axis, components.length, rotate, grid, clearance);
}

function solveBypass(island: IslandNode, input: PlacementInput, components: PcbComponent[], grid: number, clearance: number, _clearanceResolver: ClearanceResolver): CandidatePlacement[] {
    if (components.length === 0) return [];
    const promoted = promotedBypassClusterData(island, input, components);
    if (promoted) {
        return solveCapCluster({ ...island, data: { ...island.data, ...promoted } }, components, input, grid, clearance, _clearanceResolver);
    }
    const axis = island.data.axis === 'x' || island.data.axis === 'y' ? island.data.axis : chooseCompactAxis(components);
    const gap = typeof island.data.gap === 'number' ? fp.max(island.data.gap, clearance) : clearance;
    const targetNet = resolveTargetNet(island, input);
    const explicitRotate = typeof island.data.rotate === 'number' ? normalizeRotation(island.data.rotate) : null;
    const rotate = explicitRotate ?? (targetNet ? bestBypassRotation(components, axis, targetNet) : bestLineRotation(components, axis, island, input, clearance, _clearanceResolver));
    return placeGrid(components, axis, components.length, rotate, grid, clearance, gap);
}

function promotedBypassClusterData(island: IslandNode, input: PlacementInput, components: PcbComponent[]) {
    if (island.data.kind !== 'bypass' || components.length < 5) return null;
    const powerNet = resolveTargetNet(island, input);
    if (!powerNet) return null;
    const returnNet = commonBypassReturnNet(components, powerNet);
    if (!returnNet) return null;
    return {
        kind: 'cap_cluster' as const,
        powerNet,
        returnNet,
        maxRows: 2,
        maxPerRow: Math.ceil(fp.div(components.length, 2)),
        topology: 'center_power_bus',
        axis: island.data.axis === 'x' || island.data.axis === 'y' ? island.data.axis : null,
        rowGap: typeof island.data.rowGap === 'number' ? island.data.rowGap : island.data.gap,
    };
}

function commonBypassReturnNet(components: PcbComponent[], powerNet: string) {
    const counts = new Map<string, number>();
    for (const component of components) {
        const nets = new Set(component.pins
            .map((pin) => pin.signal_name)
            .filter((net): net is string => Boolean(net) && net !== powerNet));
        for (const net of nets) counts.set(net, fp.add((counts.get(net) ?? 0), 1));
    }
    return [...counts.entries()]
        .filter(([, count]) => count >= Math.max(2, Math.floor(fp.mul(components.length, 0.5))))
        .sort((a, b) => fp.sub(b[1], a[1]) || fp.sub(Number(isGroundNet(b[0])), Number(isGroundNet(a[0]))) || a[0].localeCompare(b[0]))[0]?.[0] ?? null;
}

function bestBypassRotation(components: PcbComponent[], axis: 'x' | 'y', targetNet: string): number {
    const rotations = normalizedRotations(components[0]);
    const scored = rotations.map((rotate) => {
        let sumAbs = 0;
        let positive = 0;
        let negative = 0;
        for (const component of components) {
            const pin = component.pins.find((item) => item.signal_name === targetNet);
            if (!pin) continue;
            const offset = getPadOffset(component, pin.pin_number, rotate);
            if (!offset) continue;
            const cross = axis === 'x' ? offset.y : offset.x;
            sumAbs = fp.add(sumAbs, fp.abs(cross));
            if (cross > 0.0001) positive = fp.add(positive, 1);
            else if (cross < -0.0001) negative = fp.add(negative, 1);
        }
        const sameSide = positive === 0 || negative === 0;
        return { rotate, score: sameSide ? sumAbs : fp.mul(sumAbs, 0.2) };
    });
    scored.sort((a, b) => fp.sub(b.score, a.score));
    return scored[0]?.rotate ?? 0;
}

function solveCompactIsland(island: IslandNode, components: PcbComponent[], grid: number, clearance: number, clearanceResolver: ClearanceResolver): CandidatePlacement[] {
    const ordered = components
        .slice()
        .sort((a, b) => fp.sub(pairDegree(island, b.designator), pairDegree(island, a.designator)) || fp.sub(footprintArea(b), footprintArea(a)));
    const placed: CandidatePlacement[] = [];

    for (const component of ordered) {
        const candidates = compactCandidates(component, placed, grid, clearance);
        let best = candidates[0];
        let bestScore = Infinity;
        for (const candidate of candidates) {
            const variant = [...placed, candidate];
            if (hasOverlap(ordered, variant, clearanceResolver)) continue;
            const score = scorePlacements(island, ordered, variant, clearanceResolver);
            if (score < bestScore) {
                best = candidate;
                bestScore = score;
            }
        }
        placed.push(best);
    }

    return centerPlacements(placed, grid);
}

function solveTwoComponentCorePairs(island: IslandNode, components: PcbComponent[], grid: number, clearance: number, clearanceResolver: ClearanceResolver): CandidatePlacement[] {
    const anchor = selectCoreAnchor(island, components);
    const moving = components.find((component) => component.designator !== anchor.designator) ?? components[1];
    const variants: CandidatePlacement[][] = [];

    for (const anchorRotate of coreAnchorRotations(anchor)) {
        const anchorPlacement = candidatePlacement(anchor, 0, 0, anchorRotate);
        for (const movingRotate of normalizedRotations(moving)) {
            const pairClearance = fp.add(fp.max(clearance, clearanceResolver(anchor.designator, moving.designator)), .001);
            const centers = movingCentersAround(anchorPlacement.box, rotatedSize(moving.footprint, movingRotate), grid, pairClearance);
            // Add exact pad alignments and the midpoint balancing both links;
            // bbox/grid offsets alone miss these tangential positions.
            const movingOrigin = candidatePlacement(moving, 0, 0, movingRotate);
            const alignments = parseIslandPairs(island).flatMap(([a, b]) => {
                const [fixedRef, movingRef] = a.startsWith(`${anchor.designator}.`) ? [a, b] : [b, a];
                const fixedPad = padWorld(components, [anchorPlacement, movingOrigin], fixedRef);
                const movingPad = padWorld(components, [anchorPlacement, movingOrigin], movingRef);
                return fixedPad && movingPad ? [{ x: fp.sub(fixedPad.x, movingPad.x), y: fp.sub(fixedPad.y, movingPad.y) }] : [];
            });
            if (alignments.length > 1) alignments.push({ x: fp.div(alignments.reduce((n,p)=>fp.add(n, p.x),0), alignments.length), y: fp.div(alignments.reduce((n,p)=>fp.add(n, p.y),0), alignments.length) });
            const size = rotatedSize(moving.footprint, movingRotate), box = anchorPlacement.box;
            for (const p of alignments) centers.push(
                { x: fp.sub(fp.sub(box.left, pairClearance), fp.div(size.width, 2)), y: p.y },
                { x: fp.add(fp.add(box.right, pairClearance), fp.div(size.width, 2)), y: p.y },
                { x: p.x, y: fp.sub(fp.sub(box.top, pairClearance), fp.div(size.height, 2)) },
                { x: p.x, y: fp.add(fp.add(box.bottom, pairClearance), fp.div(size.height, 2)) },
            );
            for (const center of dedupePoints(centers)) {
                variants.push([
                    anchorPlacement,
                    candidatePlacement(moving, center.x, center.y, movingRotate),
                ]);
            }
        }
    }

    return centerPlacements(bestCorePairVariant(island, components, variants, clearanceResolver), grid);
}

function coreAnchorRotations(anchor: PcbComponent) {
    const rotations = normalizedRotations(anchor);
    if (anchor.pcb.role !== 'main_ic') return rotations;
    const scored = rotations.map((rotate) => ({ rotate, score: pinOneUpperLeftRotationPenalty(anchor, rotate) }));
    const best = fp.min(...scored.map((item) => item.score));
    return scored
        .filter((item) => item.score <= fp.add(best, 0.001))
        .map((item) => item.rotate);
}

function selectCoreAnchor(island: IslandNode, components: PcbComponent[]) {
    return components.slice().sort((a, b) =>
        fp.sub(Number(b.pcb.role === 'main_ic'), Number(a.pcb.role === 'main_ic'))
        || fp.sub(pairDegree(island, b.designator), pairDegree(island, a.designator))
        || b.footprint.pads.length - a.footprint.pads.length
        || fp.sub(footprintArea(b), footprintArea(a)))[0];
}

function movingCentersAround(anchorBox: Box, movingSize: { width: number; height: number }, _grid: number, clearance: number) {
    const anchorCenter = {
        x: fp.div((fp.add(anchorBox.left, anchorBox.right)), 2),
        y: fp.div((fp.add(anchorBox.top, anchorBox.bottom)), 2),
    };
    const offsets = uniqueNumbers([
        0,
        clearance,
        -clearance,
        fp.mul(clearance, 2),
        fp.mul(-clearance, 2),
        fp.div(movingSize.width, 2),
        fp.div(-movingSize.width, 2),
        fp.div(movingSize.height, 2),
        fp.div(-movingSize.height, 2),
    ]);
    const baseCenters = [
        { x: fp.sub(fp.sub(anchorBox.left, clearance), fp.div(movingSize.width, 2)), y: anchorCenter.y, axis: 'y' as const },
        { x: fp.add(fp.add(anchorBox.right, clearance), fp.div(movingSize.width, 2)), y: anchorCenter.y, axis: 'y' as const },
        { x: anchorCenter.x, y: fp.sub(fp.sub(anchorBox.top, clearance), fp.div(movingSize.height, 2)), axis: 'x' as const },
        { x: anchorCenter.x, y: fp.add(fp.add(anchorBox.bottom, clearance), fp.div(movingSize.height, 2)), axis: 'x' as const },
    ];
    const centers: Point[] = [];
    for (const base of baseCenters) {
        for (const offset of offsets) {
            centers.push({
                x: roundPlacement(fp.add(base.x, (base.axis === 'x' ? offset : 0))),
                y: roundPlacement(fp.add(base.y, (base.axis === 'y' ? offset : 0))),
            });
        }
    }
    return dedupePoints(centers);
}

function bestCorePairVariant(island: IslandNode, components: PcbComponent[], variants: CandidatePlacement[][], clearanceResolver: ClearanceResolver) {
    return variants
        .filter((variant) => !hasOverlap(components, variant, clearanceResolver))
        .sort((a, b) => fp.sub(scoreCorePairVariant(island, components, a, clearanceResolver), scoreCorePairVariant(island, components, b, clearanceResolver)))[0]
        ?? variants.sort((a, b) => fp.sub(scoreCorePairVariant(island, components, a, clearanceResolver), scoreCorePairVariant(island, components, b, clearanceResolver)))[0]
        ?? [];
}

function scoreCorePairVariant(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[], clearanceResolver: ClearanceResolver) {
    const pairs = parseIslandPairs(island);
    const bbox = placementsBox(components, placements);
    let score = hasOverlap(components, placements, clearanceResolver) ? 1_000_000 : 0;

    let pairScore = 0;
    let sumDistance = 0;
    let maxDistance = 0;
    const expectedMax = typeof island.data.maxDistance === 'number' ? island.data.maxDistance : null;
    for (const [a, b] of pairs) {
        const first = padWorld(components, placements, a);
        const second = padWorld(components, placements, b);
        if (!first || !second) continue;
        const value = dist(first, second);
        const excess = expectedMax !== null ? fp.max(0, fp.sub(value, expectedMax)) : 0;
        pairScore = fp.add(pairScore, fp.add(value, fp.mul(fp.mul(excess, excess), 100)));
        sumDistance = fp.add(sumDistance, value);
        maxDistance = fp.max(maxDistance, value);
    }

    score = fp.add(score, (pairs.length > 0 ? fp.mul(pairScore, 10_000) : 0));
    // This solver precedes the native block solver. Its internal geometry is
    // subsequently rigid, so it must charge foreign-pad hits here as well.
    // 100 scales the shared weight to this solver's 10,000-per-mm pair term.
    score = fp.add(score, fp.mul(fp.mul(corePairPadHits(island, components, placements), placementPadCrossingWeight()), 100));
    score = fp.add(score, fp.mul(pairBalancePenalty(island, components, placements), 25));
    score = fp.add(score, fp.mul(pairSegmentCrossingPenalty(island, components, placements), 600));
    score = fp.add(score, fp.mul(facingPadsPenalty(island, components, placements), 15));
    score = fp.add(score, fp.mul(fp.mul((fp.sub(bbox.right, bbox.left)), (fp.sub(bbox.bottom, bbox.top))), 0.4));
    score = fp.add(score, fp.add(sumDistance, maxDistance));
    score = fp.add(score, fp.mul(anchorPinOneUpperLeftPenalty(island, components, placements), 5));
    return score;
}

function corePairPadHits(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    let hits = 0;
    for (const [a, b] of parseIslandPairs(island)) {
        const first = padWorld(components, placements, a), second = padWorld(components, placements, b);
        if (!first || !second) continue;
        const [designator, pin] = a.split('.');
        const net = components.find(c => c.designator === designator)?.pins.find(p => String(p.pin_number) === pin)?.signal_name;
        const endpoints = [a, b].map(ref => placements.find(p => p.designator === ref.split('.')[0])!);
        const layer = endpoints[0].layer === endpoints[1].layer ? endpoints[0].layer : null;
        const crossed = new Set<string>();
        for (const component of components) {
            const placement = placements.find(p => p.designator === component.designator)!;
            for (const pad of component.footprint.pads) {
                const ref = `${component.designator}.${pad.pin_number}`;
                if (ref === a || ref === b) continue;
                if (net && component.pins.some(p => String(p.pin_number) === String(pad.pin_number) && p.signal_name === net)) continue;
                if (layer && placement.layer !== layer && !isThroughHolePad(pad)) continue;
                if (segmentIntersectsBox(first, second, componentPadBox(placement, pad))) crossed.add(ref);
            }
        }
        hits = fp.add(hits, crossed.size);
    }
    return hits;
}

function pairBalancePenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    const distances = parseIslandPairs(island).map(([a, b]) => {
        const first = padWorld(components, placements, a);
        const second = padWorld(components, placements, b);
        return first && second ? dist(first, second) : 0;
    }).filter((value) => value > 0);
    if (distances.length < 2) return 0;
    return fp.sub(fp.max(...distances), fp.min(...distances));
}

function pairSegmentCrossingPenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    const segments = parseIslandPairs(island).map(([a, b]) => {
        const first = padWorld(components, placements, a);
        const second = padWorld(components, placements, b);
        return first && second ? [first, second] as const : null;
    }).filter((segment): segment is readonly [Point, Point] => Boolean(segment));
    let penalty = 0;
    for (let i = 0; i < segments.length; i += 1) {
        for (let j = i + 1; j < segments.length; j += 1) {
            if (segmentsCross(segments[i][0], segments[i][1], segments[j][0], segments[j][1])) penalty = fp.add(penalty, 1);
        }
    }
    return penalty;
}

function facingPadsPenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    let penalty = 0;
    for (const [a, b] of parseIslandPairs(island)) {
        const [aDesignator, aPin] = a.split('.');
        const [bDesignator, bPin] = b.split('.');
        const aComponent = components.find((component) => component.designator === aDesignator);
        const bComponent = components.find((component) => component.designator === bDesignator);
        const aPlacement = placements.find((placement) => placement.designator === aDesignator);
        const bPlacement = placements.find((placement) => placement.designator === bDesignator);
        if (!aComponent || !bComponent || !aPlacement || !bPlacement) continue;
        const aPad = padPoint(aComponent, aPlacement, aPin);
        const bPad = padPoint(bComponent, bPlacement, bPin);
        if (!aPad || !bPad) continue;
        const aOut = normalizeVector({ x: fp.sub(aPad.x, aPlacement.x), y: fp.sub(aPad.y, aPlacement.y) });
        const bOut = normalizeVector({ x: fp.sub(bPad.x, bPlacement.x), y: fp.sub(bPad.y, bPlacement.y) });
        const link = normalizeVector({ x: fp.sub(bPad.x, aPad.x), y: fp.sub(bPad.y, aPad.y) });
        penalty = fp.add(penalty, fp.max(0, fp.sub(1, dot(aOut, link))));
        penalty = fp.add(penalty, fp.max(0, fp.add(1, dot(bOut, link))));
    }
    return penalty;
}

function anchorPinOneUpperLeftPenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    const anchor = selectCoreAnchor(island, components);
    const placement = placements.find((item) => item.designator === anchor.designator);
    if (!placement) return 0;
    const pinOne = getPadOffset(anchor, '1', placement.rotate, placement.layer);
    if (!pinOne) return 0;
    return pinOneUpperLeftPenaltyForOffset(anchor, pinOne, placement.rotate);
}

function pinOneUpperLeftRotationPenalty(component: PcbComponent, rotate: number) {
    const pinOne = getPadOffset(component, '1', rotate, component.pcb.allowedLayers[0] ?? 'top');
    return pinOne ? pinOneUpperLeftPenaltyForOffset(component, pinOne, rotate) : 0;
}

function pinOneUpperLeftPenaltyForOffset(component: PcbComponent, pinOne: Point, rotate: number) {
    const size = rotatedSize(component.footprint, rotate);
    const normalizedX = fp.div(pinOne.x, fp.max(fp.div(size.width, 2), 0.001));
    const normalizedY = fp.div(pinOne.y, fp.max(fp.div(size.height, 2), 0.001));
    return fp.add(fp.add(fp.pow(fp.max(0, fp.add(normalizedX, 0.25)), 2), fp.pow(fp.max(0, fp.add(normalizedY, 0.25)), 2)), fp.mul(fp.max(0, fp.sub(normalizedX, normalizedY)), 0.15));
}

function compactCandidates(component: PcbComponent, placed: CandidatePlacement[], grid: number, clearance: number) {
    const candidates: CandidatePlacement[] = [];
    const rotations = normalizedRotations(component);
    if (placed.length === 0) {
        for (const rotate of rotations) candidates.push(candidatePlacement(component, 0, 0, rotate));
        return candidates;
    }

    const bbox = unionBoxes(placed.map((placement) => placement.box));
    for (const rotate of rotations) {
        const size = rotatedSize(component.footprint, rotate);
        const offsets = [
            { x: fp.add(fp.add(bbox.right, clearance), fp.div(size.width, 2)), y: 0 },
            { x: fp.sub(fp.sub(bbox.left, clearance), fp.div(size.width, 2)), y: 0 },
            { x: 0, y: fp.add(fp.add(bbox.bottom, clearance), fp.div(size.height, 2)) },
            { x: 0, y: fp.sub(fp.sub(bbox.top, clearance), fp.div(size.height, 2)) },
            { x: fp.add(fp.add(bbox.right, clearance), fp.div(size.width, 2)), y: fp.add(fp.add(bbox.bottom, clearance), fp.div(size.height, 2)) },
            { x: fp.add(fp.add(bbox.right, clearance), fp.div(size.width, 2)), y: fp.sub(fp.sub(bbox.top, clearance), fp.div(size.height, 2)) },
            { x: fp.sub(fp.sub(bbox.left, clearance), fp.div(size.width, 2)), y: fp.add(fp.add(bbox.bottom, clearance), fp.div(size.height, 2)) },
            { x: fp.sub(fp.sub(bbox.left, clearance), fp.div(size.width, 2)), y: fp.sub(fp.sub(bbox.top, clearance), fp.div(size.height, 2)) },
        ];
        for (const point of offsets) candidates.push(candidatePlacement(component, snap(point.x, grid), snap(point.y, grid), rotate));
    }
    return candidates;
}

function placeGrid(
    components: PcbComponent[],
    axis: 'x' | 'y',
    maxPerRow: number,
    rotate: number,
    grid: number,
    clearance: number,
    gap?: number,
    rowGap?: number,
): CandidatePlacement[] {
    const componentGap = fp.max(gap ?? clearance, clearance);
    const rowSpacing = fp.max(rowGap ?? gap ?? clearance, clearance);
    const sizes = components.map((component) => rotatedSize(component.footprint, rotate));
    const rowCount = Math.ceil(fp.div(components.length, maxPerRow));
    const rowSizes: Array<{ width: number; height: number }> = [];
    for (let row = 0; row < rowCount; row += 1) {
        const rowComponents = components.slice(fp.mul(row, maxPerRow), fp.mul((row + 1), maxPerRow));
        const rowComponentSizes = sizes.slice(fp.mul(row, maxPerRow), fp.mul((row + 1), maxPerRow));
        const width = axis === 'x'
            ? fp.add(rowComponentSizes.reduce((sum, size) => fp.add(sum, size.width), 0), fp.mul(componentGap, Math.max(0, rowComponents.length - 1)))
            : fp.max(...rowComponentSizes.map((size) => size.width));
        const height = axis === 'x'
            ? fp.max(...rowComponentSizes.map((size) => size.height))
            : fp.add(rowComponentSizes.reduce((sum, size) => fp.add(sum, size.height), 0), fp.mul(componentGap, Math.max(0, rowComponents.length - 1)));
        rowSizes.push({ width, height });
    }

    const placements: CandidatePlacement[] = [];
    let crossCursor = fp.sub(fp.div(-rowSizes.reduce((sum, size) => fp.add(sum, (axis === 'x' ? size.height : size.width)), 0), 2), fp.div(fp.mul(rowSpacing, Math.max(0, rowCount - 1)), 2));
    for (let row = 0; row < rowCount; row += 1) {
        const start = fp.mul(row, maxPerRow);
        const end = fp.min(components.length, fp.add(start, maxPerRow));
        const rowSize = rowSizes[row];
        let mainCursor = fp.div(-(axis === 'x' ? rowSize.width : rowSize.height), 2);
        const crossCenter = fp.add(crossCursor, fp.div((axis === 'x' ? rowSize.height : rowSize.width), 2));
        for (let index = start; index < end; index += 1) {
            const component = components[index];
            const size = sizes[index];
            const mainCenter = fp.add(mainCursor, fp.div((axis === 'x' ? size.width : size.height), 2));
            const x = axis === 'x' ? mainCenter : crossCenter;
            const y = axis === 'x' ? crossCenter : mainCenter;
            placements.push(candidatePlacement(component, roundPlacement(x), roundPlacement(y), rotate));
            mainCursor = fp.add(mainCursor, fp.add((axis === 'x' ? size.width : size.height), componentGap));
        }
        crossCursor = fp.add(crossCursor, fp.add((axis === 'x' ? rowSize.height : rowSize.width), rowSpacing));
    }
    return centerPlacements(placements, grid);
}

function targetPinPowerSide(island: IslandNode, input: PlacementInput, axis: 'x' | 'y'): 1 | -1 {
    const target = island.data.target;
    if (!target || typeof target !== 'object' || (target as { type?: string }).type !== 'pin') return 1;
    const pinTarget = target as { type: 'pin'; designator: string; pin_number: string | number };
    const component = input.components.find((item) => item.designator === pinTarget.designator);
    if (!component) return 1;
    const pin = component.pins.find((item) => String(item.pin_number) === String(pinTarget.pin_number));
    if (!pin) return 1;
    const pad = component.footprint.pads.find((item) => String(item.pin_number) === String(pin.pin_number));
    if (!pad) return 1;
    return axis === 'x' ? (pad.y >= 0 ? 1 : -1) : (pad.x >= 0 ? 1 : -1);
}

function placeCenterPowerBus(
    components: PcbComponent[],
    axis: 'x' | 'y',
    maxPerRow: number,
    grid: number,
    gap: number,
    rowGap: number,
    island: IslandNode,
    powerSide: 1 | -1,
): CandidatePlacement[] {
    const rowCount = Math.ceil(fp.div(components.length, maxPerRow));
    if (rowCount < 2) {
        return placeRowWithPowerSide(components, axis, powerSide, gap, grid, island);
    }

    const rows: CandidatePlacement[][] = [];
    for (let row = 0; row < rowCount; row += 1) {
        const start = fp.mul(row, maxPerRow);
        const rowComponents = components.slice(start, fp.add(start, maxPerRow));
        const desiredSide = row === 0 ? powerSide : (-powerSide as 1 | -1);
        rows.push(placeRowWithPowerSide(rowComponents, axis, desiredSide, gap, grid, island));
    }

    const rowCrossSizes = rows.map((rowPlacements) => {
        const box = unionBoxes(rowPlacements.map((placement) => placement.box));
        return axis === 'x' ? fp.sub(box.bottom, box.top) : fp.sub(box.right, box.left);
    });
    const totalCross = fp.add(rowCrossSizes.reduce((sum, size) => fp.add(sum, size), 0), fp.mul(rowGap, (rows.length - 1)));
    let crossCursor = fp.div(-totalCross, 2);

    const result: CandidatePlacement[] = [];
    for (let row = 0; row < rows.length; row += 1) {
        const rowBox = unionBoxes(rows[row].map((placement) => placement.box));
        const rowCrossCenter = fp.add(crossCursor, fp.div(rowCrossSizes[row], 2));
        const dx = axis === 'x' ? fp.div(-(fp.add(rowBox.left, rowBox.right)), 2) : rowCrossCenter;
        const dy = axis === 'x' ? rowCrossCenter : fp.div(-(fp.add(rowBox.top, rowBox.bottom)), 2);
        for (const placement of rows[row]) {
            result.push(candidatePlacement(placement.component, roundPlacement(fp.add(placement.x, dx)), roundPlacement(fp.add(placement.y, dy)), placement.rotate));
        }
        crossCursor = fp.add(crossCursor, fp.add(rowCrossSizes[row], rowGap));
    }
    return result;
}

function placeRowWithPowerSide(
    components: PcbComponent[],
    axis: 'x' | 'y',
    desiredSide: 1 | -1,
    gap: number,
    grid: number,
    island: IslandNode,
): CandidatePlacement[] {
    const placements: CandidatePlacement[] = [];
    const sizes: Array<{ width: number; height: number }> = [];
    for (const component of components) {
        const rotations = normalizedRotations(component);
        const { powerPin, returnPin } = powerReturnPins(component, island);
        let bestRotate = rotations[0] ?? 0;
        let bestScore = -Infinity;
        for (const rotate of rotations) {
            const powerOffset = powerPin ? getPadOffset(component, powerPin.pin_number, rotate) : null;
            const returnOffset = returnPin ? getPadOffset(component, returnPin.pin_number, rotate) : null;
            if (!powerOffset || !returnOffset) continue;
            const powerCross = axis === 'x' ? powerOffset.y : powerOffset.x;
            const returnCross = axis === 'x' ? returnOffset.y : returnOffset.x;
            const axisAlign = fp.abs(axis === 'x' ? fp.sub(powerOffset.x, returnOffset.x) : fp.sub(powerOffset.y, returnOffset.y));
            const score = fp.sub(fp.add(fp.mul(powerCross, desiredSide), fp.mul(returnCross, (-desiredSide))), axisAlign);
            if (score > bestScore) {
                bestScore = score;
                bestRotate = rotate;
            }
        }
        sizes.push(rotatedSize(component.footprint, bestRotate));
        placements.push(candidatePlacement(component, 0, 0, bestRotate));
    }

    const mainSize = fp.add(sizes.reduce((sum, size) => fp.add(sum, (axis === 'x' ? size.width : size.height)), 0), fp.mul(gap, Math.max(0, sizes.length - 1)));
    let mainCursor = fp.div(-mainSize, 2);
    const result: CandidatePlacement[] = [];
    for (let index = 0; index < components.length; index += 1) {
        const size = sizes[index];
        const mainCenter = fp.add(mainCursor, fp.div((axis === 'x' ? size.width : size.height), 2));
        const x = axis === 'x' ? mainCenter : 0;
        const y = axis === 'x' ? 0 : mainCenter;
        result.push(candidatePlacement(components[index], roundPlacement(x), roundPlacement(y), placements[index].rotate));
        mainCursor = fp.add(mainCursor, fp.add((axis === 'x' ? size.width : size.height), gap));
    }
    return centerPlacements(result, grid);
}

function powerReturnPins(component: PcbComponent, island: IslandNode) {
    const powerNet = typeof island.data.powerNet === 'string' ? island.data.powerNet : null;
    const returnNet = typeof island.data.returnNet === 'string' ? island.data.returnNet : null;
    const powerPin = powerNet ? component.pins.find((pin) => pin.signal_name === powerNet) ?? null : null;
    const returnPin = returnNet ? component.pins.find((pin) => pin.signal_name === returnNet) ?? null : null;
    return { powerPin, returnPin };
}

function bestVariant(island: IslandNode, components: PcbComponent[], variants: CandidatePlacement[][], clearance: number, clearanceResolver: ClearanceResolver) {
    return variants.reduce((best, variant) =>
        scorePlacements(island, components, variant, clearanceResolver) < scorePlacements(island, components, best, clearanceResolver)
            ? variant
            : best,
    variants[0] ?? []);
}

function scorePlacements(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[], clearanceResolver: ClearanceResolver) {
    if (placements.length === 0) return Infinity;
    const bbox = placementsBox(components, placements);
    const width = fp.sub(bbox.right, bbox.left);
    const height = fp.sub(bbox.bottom, bbox.top);
    let score = fp.add(fp.mul(fp.mul(width, height), 2), (fp.add(width, height)));
    if (hasOverlap(components, placements, clearanceResolver)) score = fp.add(score, 1_000_000);
    score = fp.add(score, fp.mul(pairDistancePenalty(island, components, placements), 8));
    score = fp.add(score, fp.mul(sameNetPadSpreadPenalty(components, placements), 5));
    score = fp.add(score, fp.mul(capClusterPadAlignmentPenalty(island, components, placements), 6));
    return score;
}

function pairDistancePenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    const pairs = parseIslandPairs(island);
    let penalty = 0;
    for (const [a, b] of pairs) {
        const first = padWorld(components, placements, a);
        const second = padWorld(components, placements, b);
        if (!first || !second) continue;
        penalty = fp.add(penalty, dist(first, second));
    }
    return penalty;
}

const GROUND_NET_SPREAD_WEIGHT = 0.2;

function isGroundNet(net: string) {
    return net.toUpperCase().includes('GND');
}

function sameNetPadSpreadPenalty(components: PcbComponent[], placements: CandidatePlacement[]) {
    const padsByNet = new Map<string, Point[]>();
    for (const component of components) {
        const placement = placements.find((item) => item.designator === component.designator);
        if (!placement) continue;
        for (const pin of component.pins) {
            if (!pin.signal_name) continue;
            const point = padPoint(component, placement, pin.pin_number);
            if (!point) continue;
            const points = padsByNet.get(pin.signal_name) ?? [];
            points.push(point);
            padsByNet.set(pin.signal_name, points);
        }
    }
    let penalty = 0;
    for (const [net, points] of padsByNet.entries()) {
        if (points.length < 2) continue;
        const box = pointsBox(points);
        const spread = fp.add((fp.sub(box.right, box.left)), (fp.sub(box.bottom, box.top)));
        penalty = fp.add(penalty, isGroundNet(net) ? fp.mul(spread, GROUND_NET_SPREAD_WEIGHT) : spread);
    }
    return penalty;
}

function capClusterPadAlignmentPenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    if (island.data.kind !== 'cap_cluster') return 0;
    const powerNet = typeof island.data.powerNet === 'string' ? island.data.powerNet : null;
    const returnNet = typeof island.data.returnNet === 'string' ? island.data.returnNet : null;
    if (!powerNet || !returnNet) return 0;
    let penalty = fp.add(netAlignmentPenalty(components, placements, powerNet), netAlignmentPenalty(components, placements, returnNet));
    if (island.data.topology === 'center_power_bus') {
        penalty = fp.add(penalty, centerPowerBusCentroidPenalty(island, components, placements));
    }
    return penalty;
}

function centerPowerBusCentroidPenalty(island: IslandNode, components: PcbComponent[], placements: CandidatePlacement[]) {
    const axis = island.data.axis === 'x' || island.data.axis === 'y' ? island.data.axis : null;
    const powerNet = typeof island.data.powerNet === 'string' ? island.data.powerNet : null;
    const returnNet = typeof island.data.returnNet === 'string' ? island.data.returnNet : null;
    if (!powerNet || !returnNet || !axis) return 0;
    const powerCentroid = netCentroidCross(components, placements, powerNet, axis);
    const returnCentroid = netCentroidCross(components, placements, returnNet, axis);
    if (powerCentroid === null || returnCentroid === null) return 0;
    // power pads should be closer to island center than return pads
    const outward = fp.max(0, fp.sub(fp.abs(powerCentroid), fp.abs(returnCentroid)));
    return fp.mul(outward, 100);
}

function netCentroidCross(components: PcbComponent[], placements: CandidatePlacement[], net: string, axis: 'x' | 'y') {
    let sum = 0;
    let count = 0;
    for (const component of components) {
        const pin = component.pins.find((item) => item.signal_name === net);
        const placement = placements.find((item) => item.designator === component.designator);
        if (!pin || !placement) continue;
        const point = padPoint(component, placement, pin.pin_number);
        if (!point) continue;
        sum = fp.add(sum, axis === 'x' ? point.y : point.x);
        count += 1;
    }
    return count > 0 ? fp.div(sum, count) : null;
}

function netAlignmentPenalty(components: PcbComponent[], placements: CandidatePlacement[], net: string) {
    const points: Point[] = [];
    for (const component of components) {
        const pin = component.pins.find((item) => item.signal_name === net);
        const placement = placements.find((item) => item.designator === component.designator);
        if (!pin || !placement) continue;
        const point = padPoint(component, placement, pin.pin_number);
        if (point) points.push(point);
    }
    if (points.length < 2) return 0;
    const box = pointsBox(points);
    return fp.min(fp.sub(box.right, box.left), fp.sub(box.bottom, box.top));
}

function rotationsForAxis(rotations: number[], components: PcbComponent[], island: IslandNode, axis: 'x' | 'y') {
    const powerNet = typeof island.data.powerNet === 'string' ? island.data.powerNet : null;
    const returnNet = typeof island.data.returnNet === 'string' ? island.data.returnNet : null;
    if (!powerNet || !returnNet) return rotations;
    const scored = rotations.map((rotate) => ({
        rotate,
        score: components.reduce((sum, component) => fp.add(sum, padVectorAxisScore(component, powerNet, returnNet, rotate, axis)), 0),
    }));
    const bestScore = fp.min(...scored.map((item) => item.score));
    return scored.filter((item) => item.score <= fp.add(bestScore, 0.001)).map((item) => item.rotate);
}

function padVectorAxisScore(component: PcbComponent, powerNet: string, returnNet: string, rotate: number, axis: 'x' | 'y') {
    const powerPin = component.pins.find((pin) => pin.signal_name === powerNet);
    const returnPin = component.pins.find((pin) => pin.signal_name === returnNet);
    if (!powerPin || !returnPin) return 0;
    const power = getPadOffset(component, powerPin.pin_number, rotate);
    const ret = getPadOffset(component, returnPin.pin_number, rotate);
    if (!power || !ret) return 0;
    const dx = fp.abs(fp.sub(power.x, ret.x));
    const dy = fp.abs(fp.sub(power.y, ret.y));
    return axis === 'x' ? fp.sub(dx, dy) : fp.sub(dy, dx);
}

function bestLineRotation(components: PcbComponent[], axis: 'x' | 'y', island: IslandNode, input: PlacementInput, clearance: number, clearanceResolver: ClearanceResolver) {
    const targetNet = resolveTargetNet(island, input);
    const rotations = normalizedRotations(components[0]);
    if (targetNet) {
        const scored = rotations.map((rotate) => ({
            rotate,
            topology: targetPadPerpendicularScore(components, axis, rotate, targetNet),
            compact: scorePlacements(island, components, placeGrid(components, axis, components.length, rotate, 0.5, clearance), clearanceResolver),
        }));
        scored.sort((a, b) => {
            if (b.topology !== a.topology) return fp.sub(b.topology, a.topology);
            return fp.sub(a.compact, b.compact);
        });
        return scored[0]?.rotate ?? 0;
    }
    return bestSharedRotation(components, axis, island, clearanceResolver);
}

function bestSharedRotation(components: PcbComponent[], axis: 'x' | 'y', island: IslandNode, clearanceResolver: ClearanceResolver) {
    const rotations = normalizedRotations(components[0]);
    return rotations
        .map((rotate) => ({
            rotate,
            score: scorePlacements(island, components, placeGrid(components, axis, components.length, rotate, 0.5, 0.8), clearanceResolver),
        }))
        .sort((a, b) => fp.sub(a.score, b.score))[0]?.rotate ?? 0;
}

function resolveTargetNet(island: IslandNode, input: PlacementInput): string | null {
    const target = island.data.target;
    if (!target || typeof target !== 'object' || (target as { type?: string }).type !== 'pin') return null;
    const pinTarget = target as { type: 'pin'; designator: string; pin_number: string | number };
    const component = input.components.find((item) => item.designator === pinTarget.designator);
    if (!component) return null;
    const pin = component.pins.find((item) => String(item.pin_number) === String(pinTarget.pin_number));
    return pin?.signal_name ?? null;
}

function padNet(component: PcbComponent, pinNumber: string | number): string | null {
    const pin = component.pins.find((item) => String(item.pin_number) === String(pinNumber));
    return pin?.signal_name ?? null;
}

function targetPadPerpendicularScore(components: PcbComponent[], axis: 'x' | 'y', rotate: number, targetNet: string): number {
    let total = 0;
    let count = 0;
    for (const component of components) {
        const pad = component.footprint.pads.find((item) => padNet(component, item.pin_number) === targetNet);
        if (!pad) continue;
        const layer = component.pcb.allowedLayers[0] ?? 'top';
        const offset = getPadOffset(component, pad.pin_number, rotate, layer);
        if (!offset) continue;
        total = fp.add(total, axis === 'x' ? fp.abs(offset.y) : fp.abs(offset.x));
        count += 1;
    }
    return count > 0 ? fp.div(total, count) : 0;
}

function chooseCompactAxis(components: PcbComponent[]) {
    const totalWidth = components.reduce((sum, component) => fp.add(sum, component.footprint.width), 0);
    const totalHeight = components.reduce((sum, component) => fp.add(sum, component.footprint.height), 0);
    return totalWidth <= totalHeight ? 'x' : 'y';
}

function parseIslandPairs(island: IslandNode): Array<[string, string]> {
    const pairs = island.data.pairs;
    if (!Array.isArray(pairs)) return [];
    return pairs
        .filter((pair): pair is [string, string] => Array.isArray(pair) && typeof pair[0] === 'string' && typeof pair[1] === 'string');
}

function pairDegree(island: IslandNode, designator: string) {
    return parseIslandPairs(island)
        .filter(([a, b]) => a.startsWith(`${designator}.`) || b.startsWith(`${designator}.`))
        .length;
}

function collectIslandNodes(root: PlacementTreeNode): IslandNode[] {
    const nodes: IslandNode[] = [];
    const visit = (node: PlacementTreeNode) => {
        if (node.kind === 'island' && isIslandKind(node.data?.kind)) nodes.push(node as IslandNode);
        for (const child of node.children) visit(child);
    };
    visit(root);
    return nodes;
}

function isIslandKind(kind: unknown): kind is IslandKind {
    return kind === 'line' || kind === 'bypass' || kind === 'cap_cluster' || kind === 'core_pairs';
}

function normalizedRotations(component: PcbComponent) {
    const rotations = component.pcb.allowedRotations.length ? component.pcb.allowedRotations : [0, 90, 180, 270];
    return [...new Set(rotations.map((rotation) => normalizeRotation(rotation)))];
}

function normalizeRotation(value: number) {
    return ((Math.round(value) % 360) + 360) % 360;
}

function candidatePlacement(component: PcbComponent, x: number, y: number, rotate: number): CandidatePlacement {
    const layer = component.pcb.allowedLayers[0] ?? 'top';
    const placement = { designator: component.designator, x, y, rotate, layer, score: 0 };
    return { ...placement, component, box: componentBox(component, placement) };
}

function centerPlacements(placements: CandidatePlacement[], grid: number) {
    if (placements.length === 0) return [];
    const bbox = unionBoxes(placements.map((placement) => placement.box));
    const dx = fp.div((fp.add(bbox.left, bbox.right)), 2);
    const dy = fp.div((fp.add(bbox.top, bbox.bottom)), 2);
    return placements.map((placement) => candidatePlacement(
        placement.component,
        roundPlacement(fp.sub(placement.x, dx)),
        roundPlacement(fp.sub(placement.y, dy)),
        placement.rotate,
    ));
}

function padWorld(components: PcbComponent[], placements: CandidatePlacement[], ref: string) {
    const [designator, pin] = ref.split('.');
    const component = components.find((item) => item.designator === designator);
    const placement = placements.find((item) => item.designator === designator);
    if (!component || !placement) return null;
    return padPoint(component, placement, pin);
}

function padPoint(component: PcbComponent, placement: Placement, pin: string | number) {
    const offset = getPadOffset(component, pin, placement.rotate, placement.layer);
    return offset ? { x: fp.add(placement.x, offset.x), y: fp.add(placement.y, offset.y) } : null;
}







function placementsBox(components: PcbComponent[], placements: Placement[]) {
    return unionBoxes(placements.map((placement) => {
        const component = components.find((item) => item.designator === placement.designator);
        return component ? componentBox(component, placement) : null;
    }).filter((box): box is Box => Boolean(box)));
}



function hasOverlap(components: PcbComponent[], placements: Placement[], clearanceResolver: ClearanceResolver) {
    for (let i = 0; i < placements.length; i += 1) {
        for (let j = i + 1; j < placements.length; j += 1) {
            const aComponent = components.find((component) => component.designator === placements[i].designator);
            const bComponent = components.find((component) => component.designator === placements[j].designator);
            if (!aComponent || !bComponent) continue;
            if (!placementsCanConflict(aComponent, placements[i], bComponent, placements[j])) continue;
            const clearance = clearanceResolver(aComponent.designator, bComponent.designator);
            const boxPairs = componentPairCollisionBoxPairs(aComponent, placements[i], bComponent, placements[j]);
            if (boxPairs.some((pair) => overlaps(pair.a, pair.b, clearance))) return true;
        }
    }
    return false;
}







function segmentsCross(a: Point, b: Point, c: Point, d: Point) {
    return fp.mul(orientation(a, b, c), orientation(a, b, d)) < 0
        && fp.mul(orientation(c, d, a), orientation(c, d, b)) < 0;
}

function orientation(a: Point, b: Point, c: Point) {
    const value = fp.sub(fp.mul((fp.sub(b.x, a.x)), (fp.sub(c.y, a.y))), fp.mul((fp.sub(b.y, a.y)), (fp.sub(c.x, a.x))));
    if (fp.abs(value) < 0.000001) return 0;
    return value > 0 ? 1 : -1;
}

function normalizeVector(vector: Point) {
    const length = fp.hypot(vector.x, vector.y);
    if (length <= 0.000001) return { x: 0, y: 0 };
    return { x: fp.div(vector.x, length), y: fp.div(vector.y, length) };
}

function dot(a: Point, b: Point) {
    return fp.add(fp.mul(a.x, b.x), fp.mul(a.y, b.y));
}

function uniqueNumbers(values: number[]) {
    return [...new Set(values.filter((value) => Number.isFinite(value)).map((value) => roundPlacement(value)))];
}

function dedupePoints(points: Point[]) {
    const seen = new Set<string>();
    return points.filter((point) => {
        const key = `${roundPlacement(point.x)}:${roundPlacement(point.y)}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
    });
}



function footprintArea(component: PcbComponent) {
    return fp.mul(component.footprint.width, component.footprint.height);
}

function snap(value: number, grid: number) {
    if (grid <= 0) return value;
    return fp.mul(Math.round(fp.div(value, grid)), grid);
}
