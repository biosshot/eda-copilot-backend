// Frozen TypeScript board alignment pass, the reference for the native pass.
// Keep it identical to `refineBoardAlignment` before the native port.
import * as fp from '../../src/pcb-layout/f32.ts';
import type { Placement, PlacementInput, Point } from '../../src/types/pcb/layout-model.ts';
import { boxCenter, componentBox, unionBoxes } from '../../src/pcb-layout/pcb-auto-place/geometry.ts';
import { createPlacementReport } from '../../src/pcb-layout/pcb-auto-place/placement-report.ts';
import { boardSpacingPenalty, boardSpacingPolicy } from '../../src/pcb-layout/pcb-auto-place-v2/board-spacing.ts';
import { globalPostPlaceScore } from '../../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { encodeNativePostPlaceRefineProblem } from '../../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from '../../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { PlacementPrimitive, PrimitiveSolveDiagnostic } from '../../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import {
    BOARD_ALIGNMENT_POLICY, alignmentErrors, alignmentHardHintsNoWorse, boardAlignmentScore, boardElectricalQuality,
    boardElectricalRegression, findAlignmentPairs, type AlignmentPair,
} from '../../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';

function currentRoots(input: PlacementInput, roots: PlacementPrimitive[], placements: Placement[]) {
    const cs = new Map(input.components.map(c => [c.designator, c]));
    const ps = new Map(placements.map(p => [p.designator, p]));
    return roots.map(root => {
        const poses = root.placements.map(p => ps.get(p.designator)!).filter(Boolean);
        const boxes = poses.map(p => componentBox(cs.get(p.designator)!, p));
        const bbox = unionBoxes(boxes);
        return { ...root, placements: poses, bbox, width: fp.sub(bbox.right, bbox.left), height: fp.sub(bbox.bottom, bbox.top) };
    });
}
function center(root: PlacementPrimitive, anchor?: string): Point {
    return root.placements.find(p => p.designator === anchor) ?? boxCenter(root.bbox);
}
function penalty(roots: PlacementPrimitive[], pairs: AlignmentPair[]) {
    return boardAlignmentScore(roots, {pairs,...loadNativeBoardPacker().placementAlignmentPolicy(pairs.length, false)});
}

export function referenceBoardAlignment(input: PlacementInput, roots: PlacementPrimitive[], initial: Placement[]) {
    let placements = initial;
    let current = currentRoots(input, roots, placements);
    const pairs = findAlignmentPairs(input, roots, initial);
    const before = alignmentErrors(current, pairs);
    const diagnostics: PrimitiveSolveDiagnostic[] = [];
    const rejected = { objective: 0, electrical: 0, netLength: 0, geometry: 0, hardHint: 0, native: 0 };
    const moves: Array<{ blocks: string[]; axis: 'x' | 'y'; shifts: number[]; perpendicularShifts: number[]; scoreBefore: number; scoreAfter: number }> = [];
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
    const electrical = (ps: Placement[]) => boardElectricalQuality(input,ps);
    const baseElectrical = electrical(initial);
    const objective = (ps: Placement[], rs: PlacementPrimitive[]) => loadNativeBoardPacker().placementScoreSum([globalPostPlaceScore(input, ps), boardSpacingPenalty(input, rs, spacing), penalty(rs, pairs)]);
    let score = objective(placements, current), evaluated = 0;
    for (let pass = 0; pass < BOARD_ALIGNMENT_POLICY.passes; pass++) {
        let accepted = false;
        for (const pair of pairs) {
            const a = current.find(r => r.id === pair.a)!, b = current.find(r => r.id === pair.b)!;
            const ac = center(a,pair.anchorA), bc = center(b,pair.anchorB);
            let best: { ps: Placement[]; rs: PlacementPrimitive[]; score: number; axis: 'x'|'y'; shifts: number[]; perpendicularShifts: number[] } | undefined;
            for (const axis of ['x','y'] as const) {
                const delta = fp.sub(bc[axis], ac[axis]);
                if (fp.abs(delta) < .001 || fp.abs(delta) > fp.mul(2, BOARD_ALIGNMENT_POLICY.maxShift)) continue;
                const axisShifts = [1,.5,.25].flatMap(fraction => [[1,0],[0,-1],[.5,-.5]].map(shares=>shares.map(s=>fp.mul(fp.mul(s, delta), fraction))));
                // Alignment need not happen at either existing axis or exactly
                // halfway. Pad crossings and neighbouring blocks make small
                // changes of the shared axis meaningful. Keep the old moves and
                // add exact shared axes across the bounded interval.
                const grid=fp.max(.5, input.solverOptions.placementGridStep??.5);
                for(let target=fp.min(ac[axis], bc[axis]);target<=fp.max(ac[axis], bc[axis]);target = fp.add(target, grid))
                    axisShifts.push([fp.sub(target, ac[axis]),fp.sub(target, bc[axis])]);
                const seenShifts=new Set<string>();
                for (const shifts of axisShifts)
                for (const perpendiculars of [[0,0],[-.5,-.5],[.5,.5],[-1,-1],[1,1],[0,-.5],[0,.5],[-.5,0],[.5,0],[0,-1],[0,1],[-1,0],[1,0]]) {
                    const key=[...shifts,...perpendiculars].map(n=>n.toFixed(3)).join(',');
                    if(seenShifts.has(key))continue;
                    seenShifts.add(key);
                    if (shifts.some((d,i) => (fp.abs(d) > .00001 || perpendiculars[i] !== 0) && !movable(i===0?a:b))) continue;
                    const offsets = new Map([...a.placements.map(p => [p.designator,[shifts[0],perpendiculars[0]]] as const),
                        ...b.placements.map(p => [p.designator,[shifts[1],perpendiculars[1]]] as const)]);
                    const other = axis === 'x' ? 'y' : 'x';
                    const ps = placements.map(p => offsets.get(p.designator)?.some(d=>d!==0) ? { ...p,
                        [axis]: fp.div(Math.round(fp.mul((fp.add(p[axis], offsets.get(p.designator)![0])), 1000)), 1000),
                        [other]: fp.div(Math.round(fp.mul((fp.add(p[other], offsets.get(p.designator)![1])), 1000)), 1000) } : p);
                    if (ps.some(p => fp.hypot(fp.sub(p.x, baseline.get(p.designator)!.x), fp.sub(p.y, baseline.get(p.designator)!.y)) > fp.add(BOARD_ALIGNMENT_POLICY.maxShift, .001))) continue;
                    evaluated++;
                    const rs = currentRoots(input, roots, ps), next = objective(ps,rs);
                    if (next >= fp.sub((best?.score ?? score), 1e-6) || penalty(rs,pairs) >= fp.sub(penalty(current,pairs), 1e-6)) { rejected.objective++; continue; }
                    const e = electrical(ps);
                    const regression=boardElectricalRegression(baseElectrical,e);
                    if (regression==='wiring score') { rejected.electrical++; continue; }
                    if (regression) { rejected.netLength++; continue; }
                    const report = createPlacementReport(input,ps);
                    if (!report.ok) { rejected.geometry++; continue; }
                    if (!alignmentHardHintsNoWorse(baselineReport,report)) { rejected.hardHint++; continue; }
                    if (!addon.validatePlacementChange(constraints,ps)) { rejected.native++; continue; }
                    best = { ps,rs,score: next,axis,shifts,perpendicularShifts:perpendiculars };
                }
            }
            if (!best) continue;
            moves.push({ blocks: [a.label,b.label], axis: best.axis, shifts: best.shifts, perpendicularShifts: best.perpendicularShifts, scoreBefore: score, scoreAfter: best.score });
            placements = best.ps; current = best.rs; score = best.score; accepted = true;
        }
        if (!accepted) break;
    }
    diagnostics.push({ severity: 'warning', nodeId: 'board-alignment', message:
        `Board soft alignment: similarity >= ${BOARD_ALIGNMENT_POLICY.similarity}, ${pairs.length} nearby pairs, ${evaluated} candidates, ${moves.length} accepted; fixed components and block interiors preserved` });
    return { placements, pairs, before, after: alignmentErrors(current,pairs), moves, diagnostics, evaluated, rejected };
}
