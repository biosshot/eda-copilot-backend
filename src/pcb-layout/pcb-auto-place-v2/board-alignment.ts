import type { PcbComponent, Placement, PlacementInput, PlacementReport, Point } from '#types/pcb/layout-model.ts';
import { boxCenter, boxGap, componentBox, unionBoxes } from '../pcb-auto-place/geometry.ts';
import { minimumSpanningEdges } from '../pcb-auto-place/ratsnest.ts';
import { isConnectedSignalName } from '#utils/signals.ts';
import { createPlacementReport } from '../pcb-auto-place/placement-report.ts';
import { boardSpacingPenalty, boardSpacingPolicy } from './board-spacing.ts';
import { globalPostPlaceScore } from './post-place-refiner.ts';
import { encodeNativePostPlaceScoreProblem } from './native/encode-post-place-score.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import type { PlacementPrimitive, PrimitiveSolveDiagnostic } from './primitives.ts';

/** Conservative finishing pass, in mm. Similarity never attracts distant blocks. */
export const BOARD_ALIGNMENT_POLICY = Object.freeze({ similarity: .78, neighbourGap: 8, maxShift: 3, weight: 24, passes: 2 });

function family(c: PcbComponent): string {
    if (c.pcb.role === 'main_ic') return `IC${c.pins.length}`;
    return c.designator.match(/^[A-Za-z]+/)?.[0].toUpperCase() ?? c.pcb.role;
}
const area = (c: PcbComponent) => c.footprint.width * c.footprint.height;
const weight = (key: string) => key.startsWith('IC') ? 4 : key === 'L' ? 3 : 1;
function histogram(keys: string[]) {
    const result = new Map<string, number>();
    for (const key of keys) result.set(key, (result.get(key) ?? 0) + 1);
    return result;
}
function dice(a: Map<string, number>, b: Map<string, number>, weighted = false) {
    let shared = 0, total = 0;
    for (const key of new Set([...a.keys(), ...b.keys()])) {
        const w = weighted ? weight(key) : 1;
        shared += w * Math.min(a.get(key) ?? 0, b.get(key) ?? 0);
        total += w * ((a.get(key) ?? 0) + (b.get(key) ?? 0));
    }
    return total ? 2 * shared / total : 1;
}
function signature(components: PcbComponent[]) {
    const nets = new Map<string, Set<PcbComponent>>();
    for (const c of components) for (const pin of c.pins) {
        if (!isConnectedSignalName(pin.signal_name)) continue;
        const members = nets.get(pin.signal_name) ?? new Set<PcbComponent>();
        members.add(c); nets.set(pin.signal_name, members);
    }
    // Typed edges, including local supply connectivity, independent of net names
    // and pin numbering. Multiple pads on one component do not multiply edges.
    const edges: string[] = [];
    for (const members of nets.values()) {
        const cs = [...members];
        for (let i = 0; i < cs.length; i++) for (let j = i + 1; j < cs.length; j++)
            edges.push([family(cs[i]), family(cs[j])].sort().join(':'));
    }
    return { nodes: histogram(components.map(family)), edges: histogram(edges) };
}

export function alignmentAnchor(components: PcbComponent[]): string | undefined {
    const sorted = [...components].sort((a, b) => area(b) - area(a) || a.designator.localeCompare(b.designator));
    if (!sorted.length) return undefined;
    if (sorted.length === 1 || area(sorted[0]) >= 1.5 * area(sorted[1])) return sorted[0].designator;
    // When no footprint dominates, prefer a substantial IC; otherwise the block
    // bbox center is less arbitrary than choosing one of many equal passives.
    return sorted.find(c => c.pcb.role === 'main_ic' && area(c) >= .7 * area(sorted[0]))?.designator;
}

export function blockSimilarity(a: PcbComponent[], b: PcbComponent[]): number {
    if (!a.length || !b.length) return 0;
    const aa = alignmentAnchor(a), ba = alignmentAnchor(b);
    if (Boolean(aa) !== Boolean(ba)) return 0;
    let shape = 1;
    if (aa && ba) {
        const x = a.find(c => c.designator === aa)!, y = b.find(c => c.designator === ba)!;
        if (family(x) !== family(y)) return 0;
        const dims = (c: PcbComponent) => [c.footprint.width, c.footprint.height].sort((a,b) => a-b);
        const xd = dims(x), yd = dims(y);
        shape = Math.min(xd[0], yd[0]) / Math.max(xd[0], yd[0], .001)
            * Math.min(xd[1], yd[1]) / Math.max(xd[1], yd[1], .001);
        if (shape < .5) return 0;
    }
    const x = signature(a), y = signature(b);
    return .55 * dice(x.nodes, y.nodes, true) + .3 * dice(x.edges, y.edges) + .15 * shape;
}

/** Refresh bounds from postrefine poses without changing ownership or using stale
 * local alternatives. Rigid translations in this pass preserve internal layout. */
function currentRoots(input: PlacementInput, roots: PlacementPrimitive[], placements: Placement[]) {
    const cs = new Map(input.components.map(c => [c.designator, c]));
    const ps = new Map(placements.map(p => [p.designator, p]));
    return roots.map(root => {
        const poses = root.placements.map(p => ps.get(p.designator)!).filter(Boolean);
        const boxes = poses.map(p => componentBox(cs.get(p.designator)!, p));
        const bbox = unionBoxes(boxes);
        return { ...root, placements: poses, bbox, width: bbox.right-bbox.left, height: bbox.bottom-bbox.top };
    });
}

export interface AlignmentPair { a: string; b: string; similarity: number; anchorA?: string; anchorB?: string }
export function findAlignmentPairs(input: PlacementInput, roots: PlacementPrimitive[], placements: Placement[], local = true): AlignmentPair[] {
    const current = currentRoots(input, roots, placements), pairs: AlignmentPair[] = [];
    const components = (root: PlacementPrimitive) => input.components.filter(c => root.placements.some(p => p.designator === c.designator));
    for (let i = 0; i < current.length; i++) for (let j = i+1; j < current.length; j++) {
        const a = current[i], b = current[j];
        if (!a.placements.length || !b.placements.length || a.locked && b.locked) continue;
        if (local && boxGap(a.bbox, b.bbox) > BOARD_ALIGNMENT_POLICY.neighbourGap) continue;
        if (a.placements.some(p => p.layer !== a.placements[0].layer) || b.placements.some(p => p.layer !== a.placements[0].layer)) continue;
        const ac = components(a), bc = components(b), similarity = blockSimilarity(ac, bc);
        if (similarity < BOARD_ALIGNMENT_POLICY.similarity) continue;
        pairs.push({ a: a.id, b: b.id, similarity, anchorA: alignmentAnchor(ac), anchorB: alignmentAnchor(bc) });
    }
    return pairs.sort((a,b) => b.similarity-a.similarity || a.a.localeCompare(b.a) || a.b.localeCompare(b.b));
}
export interface BoardSoftAlignment { pairs: AlignmentPair[]; weight: number; tolerance: number; range: number; fade: number }
export function boardAlignmentPolicy(input: PlacementInput, roots: PlacementPrimitive[]): BoardSoftAlignment {
    return { pairs: findAlignmentPairs(input,roots,roots.flatMap(p=>p.placements),false), weight:24, tolerance:.15, range:8, fade:8 };
}
/** Bounded reward, rather than a fading positive penalty: moving apart must not
 * become a way to escape an alignment penalty. Incomplete pairs contribute zero. */
export function boardAlignmentScore(roots: PlacementPrimitive[], policy: BoardSoftAlignment) {
    let score=0;
    for(const pair of policy.pairs) {
        const a=roots.find(p=>p.id===pair.a), b=roots.find(p=>p.id===pair.b);
        if(!a||!b)continue;
        const ac=center(a,pair.anchorA), bc=center(b,pair.anchorB);
        const error=Math.min(3,Math.max(0,Math.min(Math.abs(ac.x-bc.x),Math.abs(ac.y-bc.y))-policy.tolerance));
        const proximity=Math.max(0,Math.min(1,1-(boxGap(a.bbox,b.bbox)-policy.range)/policy.fade));
        score-=policy.weight*pair.similarity*proximity*(9-error*error);
    }
    return score;
}
function center(root: PlacementPrimitive, anchor?: string): Point {
    return root.placements.find(p => p.designator === anchor) ?? boxCenter(root.bbox);
}
export function alignmentErrors(roots: PlacementPrimitive[], pairs: AlignmentPair[]) {
    return pairs.map(pair => {
        const a = center(roots.find(r => r.id === pair.a)!, pair.anchorA);
        const b = center(roots.find(r => r.id === pair.b)!, pair.anchorB);
        return { ...pair, error: Math.min(Math.abs(a.x-b.x), Math.abs(a.y-b.y)) };
    });
}
function penalty(roots: PlacementPrimitive[], pairs: AlignmentPair[]) {
    return alignmentErrors(roots, pairs).reduce((s,p) => s + BOARD_ALIGNMENT_POLICY.weight * p.similarity * Math.min(3, p.error) ** 2, 0);
}

/** Native relative validation compares violation identities, not their magnitude.
 * A finishing pass must not deepen an existing mandatory clearance violation. */
export function alignmentHardHintsNoWorse(before: PlacementReport, after: PlacementReport) {
    return after.hintViolations.filter(v => v.hint.priority === 'critical' || ('hard' in v.hint && v.hint.hard)).every(v =>
        before.hintViolations.some(b => JSON.stringify(b.hint) === JSON.stringify(v.hint) && b.expected === v.expected &&
            (typeof b.actual === 'number' && typeof v.actual === 'number'
                ? v.expected.startsWith('>=') ? v.actual >= b.actual : v.actual <= b.actual
                : b.actual === v.actual)));
}

export function refineBoardAlignment(input: PlacementInput, roots: PlacementPrimitive[], initial: Placement[]) {
    let placements = initial;
    let current = currentRoots(input, roots, placements);
    const pairs = findAlignmentPairs(input, roots, initial);
    const before = alignmentErrors(current, pairs);
    const diagnostics: PrimitiveSolveDiagnostic[] = [];
    const rejected = { objective: 0, electrical: 0, netLength: 0, geometry: 0, hardHint: 0, native: 0 };
    const moves: Array<{ blocks: string[]; axis: 'x' | 'y'; shifts: number[]; perpendicular: number; scoreBefore: number; scoreAfter: number }> = [];
    if (!pairs.length) return { placements, pairs, before, after: before, moves, diagnostics, evaluated: 0, rejected };
    const addon = loadNativeBoardPacker();
    const constraints = encodeNativePostPlaceRefineProblem(input, initial, 1);
    const spacing = boardSpacingPolicy(input).gap;
    const baseline = new Map(initial.map(p => [p.designator, p]));
    const baselineReport = createPlacementReport(input,initial);
    const movable = (root: PlacementPrimitive) => !root.locked && !root.anchored && root.placements.every(p => {
        const c = input.components.find(c => c.designator === p.designator)!;
        return !c.pcb.fixedPlacement && !c.pcb.edgeMount && !c.pcb.edgePlace;
    });
    const electrical = (ps: Placement[]) => {
        const problem = encodeNativePostPlaceScoreProblem(input, ps);
        // Separate actual wiring from large hint penalties: reducing an existing
        // clearance violation must not buy long wires for a cosmetic adjustment.
        const lengths = problem.nets.map(net => minimumSpanningEdges(net.points).reduce((s,[a,b]) =>
            s + Math.hypot(net.points[a].x-net.points[b].x, net.points[a].y-net.points[b].y), 0));
        return { score: addon.scorePostPlace({ ...problem, distances: [], clearances: [], fixedPenalties: [], edges: [], paths: [] }), lengths };
    };
    const baseElectrical = electrical(initial);
    const objective = (ps: Placement[], rs: PlacementPrimitive[]) => globalPostPlaceScore(input, ps)
        + boardSpacingPenalty(input, rs, spacing) + penalty(rs, pairs);
    let score = objective(placements, current), evaluated = 0;
    for (let pass = 0; pass < BOARD_ALIGNMENT_POLICY.passes; pass++) {
        let accepted = false;
        for (const pair of pairs) {
            const a = current.find(r => r.id === pair.a)!, b = current.find(r => r.id === pair.b)!;
            const ac = center(a,pair.anchorA), bc = center(b,pair.anchorB);
            let best: { ps: Placement[]; rs: PlacementPrimitive[]; score: number; axis: 'x'|'y'; shifts: number[]; perpendicular: number } | undefined;
            for (const axis of ['x','y'] as const) {
                const delta = bc[axis]-ac[axis];
                if (Math.abs(delta) < .001 || Math.abs(delta) > 2*BOARD_ALIGNMENT_POLICY.maxShift) continue;
                for (const fraction of [1,.5,.25]) for (const shares of [[1,0],[0,-1],[.5,-.5]]) for (const perpendicular of [0,-.5,.5,-1,1]) {
                    const shifts = shares.map(s => s*delta*fraction);
                    if (shifts.some((d,i) => Math.abs(d) > .00001 && !movable(i===0?a:b))) continue;
                    const offsets = new Map([...a.placements.map(p => [p.designator,shifts[0]] as const),
                        ...b.placements.map(p => [p.designator,shifts[1]] as const)]);
                    const other = axis === 'x' ? 'y' : 'x';
                    const ps = placements.map(p => offsets.get(p.designator) ? { ...p,
                        [axis]: Math.round((p[axis]+offsets.get(p.designator)!)*1000)/1000,
                        [other]: Math.round((p[other]+perpendicular)*1000)/1000 } : p);
                    if (ps.some(p => Math.hypot(p.x-baseline.get(p.designator)!.x,p.y-baseline.get(p.designator)!.y) > BOARD_ALIGNMENT_POLICY.maxShift+.001)) continue;
                    evaluated++;
                    const rs = currentRoots(input, roots, ps), next = objective(ps,rs);
                    if (next >= (best?.score ?? score)-1e-6 || penalty(rs,pairs) >= penalty(current,pairs)-1e-6) { rejected.objective++; continue; }
                    const e = electrical(ps);
                    if (e.score > baseElectrical.score + Math.min(40, .001 * Math.abs(baseElectrical.score))) { rejected.electrical++; continue; }
                    if (e.lengths.some((d,i) => d > baseElectrical.lengths[i] + Math.max(.5, .02*baseElectrical.lengths[i]))) { rejected.netLength++; continue; }
                    const report = createPlacementReport(input,ps);
                    if (!report.ok) { rejected.geometry++; continue; }
                    if (!alignmentHardHintsNoWorse(baselineReport,report)) { rejected.hardHint++; continue; }
                    if (!addon.validatePlacementChange(constraints,ps)) { rejected.native++; continue; }
                    best = { ps,rs,score: next,axis,shifts,perpendicular };
                }
            }
            if (!best) continue;
            moves.push({ blocks: [a.label,b.label], axis: best.axis, shifts: best.shifts, perpendicular: best.perpendicular, scoreBefore: score, scoreAfter: best.score });
            placements = best.ps; current = best.rs; score = best.score; accepted = true;
        }
        if (!accepted) break;
    }
    diagnostics.push({ severity: 'warning', nodeId: 'board-alignment', message:
        `Board soft alignment: similarity >= ${BOARD_ALIGNMENT_POLICY.similarity}, ${pairs.length} nearby pairs, ${evaluated} candidates, ${moves.length} accepted; fixed components and block interiors preserved` });
    return { placements, pairs, before, after: alignmentErrors(current,pairs), moves, diagnostics, evaluated, rejected };
}
