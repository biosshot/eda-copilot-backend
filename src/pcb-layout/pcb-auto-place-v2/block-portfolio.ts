import type { PlacementInput, PlacementReport } from '#types/pcb/layout-model.ts';
import { createPlacementReport } from '../pcb-auto-place/placement-report.ts';
import { blockPortfolioInternalScore } from './block-quality.ts';
import { globalPostPlaceScore } from './post-place-refiner.ts';
import { boardAlignmentPolicy, boardAlignmentScore, alignmentHardHintsNoWorse, boardElectricalQuality, boardElectricalRegression } from './board-alignment.ts';
import { boardSpacingPenalty, boardSpacingPolicy } from './board-spacing.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { translatePrimitive, unionPrimitive, type PlacementPrimitive, type PrimitiveSolveDiagnostic } from './primitives.ts';

/** Two bounded board-wide hypotheses let the packer make room for different
 * block shapes before neighbourhoods are frozen. This is not a Cartesian search. */
export function blockPortfolioSeed(roots: PlacementPrimitive[], index: number): PlacementPrimitive[] {
    return roots.map(p => {
        const alternative = !p.locked || p.anchored ? p.layoutAlternatives?.[index] : undefined;
        if (alternative) return { ...alternative,
            layoutAlternatives: [{ ...p, layoutAlternatives: undefined }, ...(p.layoutAlternatives ?? []).filter(q => q !== alternative)] };
        if (!p.children.length) return p;
        const children = blockPortfolioSeed(p.children, index);
        if (children.every((child, i) => child === p.children[i])) return p;
        return { ...p, ...unionPrimitive(p.id, p.kind, p.label, p.sourceNodeId, children, p.deferredRelations) };
    });
}

export function choosePackedPortfolio(input: PlacementInput, candidates: PlacementPrimitive[][],
    diagnostics: PrimitiveSolveDiagnostic[]): PlacementPrimitive[] {
    const poses = (p: PlacementPrimitive[]) => p.flatMap(q => q.placements);
    const gap = boardSpacingPolicy(input).gap;
    let best = candidates[0];
    const alignment = boardAlignmentPolicy(input,best);
    const baselineReport = createPlacementReport(input,poses(best));
    const baselineElectrical = boardElectricalQuality(input,poses(best));
    let score = globalPostPlaceScore(input, poses(best)) + blockPortfolioInternalScore(input, best) + boardSpacingPenalty(input,best,gap) + boardAlignmentScore(best,alignment);
    const constraints = encodeNativePostPlaceRefineProblem(input, poses(best), 1);
    const fixed = new Set(input.components.filter(c => c.pcb.fixedPlacement || c.pcb.edgeMount || c.pcb.edgePlace).map(c => c.designator));
    const baseline = new Map(poses(best).map(p => [p.designator, p]));
    let selected = 0;
    for (let index = 1; index < candidates.length; index++) {
        const candidate = poses(candidates[index]);
        if (candidate.some(p => fixed.has(p.designator) && ['x', 'y', 'rotate', 'layer'].some(k => p[k as keyof typeof p] !== baseline.get(p.designator)?.[k as keyof typeof p]))) continue;
        const next = globalPostPlaceScore(input, candidate) + blockPortfolioInternalScore(input, candidates[index]) + boardSpacingPenalty(input,candidates[index],gap) + boardAlignmentScore(candidates[index],alignment);
        const report = createPlacementReport(input,candidate);
        const electricalRegression = boardElectricalRegression(baselineElectrical,boardElectricalQuality(input,candidate));
        const reason = next >= score-1e-6 ? 'score' : !report.ok ? 'geometry'
            : !alignmentHardHintsNoWorse(baselineReport,report) ? 'mandatory hint magnitude'
            : electricalRegression ? electricalRegression
            : !loadNativeBoardPacker().validatePlacementChange(constraints,candidate) ? 'native constraints' : 'accepted';
        diagnostics.push({severity:'warning',nodeId:'block-portfolio-repack',message:
            `Board alignment proposal: ${reason}, score ${next.toFixed(2)}, alignment penalty ${boardAlignmentScore(candidates[index],alignment).toFixed(2)}`});
        if(reason !== 'accepted')continue;
        best = candidates[index]; score = next; selected = index;
    }
    diagnostics.push({ severity: 'warning', nodeId: 'block-portfolio-repack',
        message: `Board packaging: ${selected ? 'alignment proposal' : 'ordinary fallback'} selected, score ${score.toFixed(2)}` });
    return best;
}

/** Select internal block layouts in their actual board neighbourhood. This is a
 * bounded coordinate descent, not a second postrefine or a global repacking.
 * A candidate retains ownership and may slide by at most two placement steps.
 */
export function selectBlockPortfolio(input: PlacementInput, roots: PlacementPrimitive[], grid: number,
    diagnostics: PrimitiveSolveDiagnostic[] = []): PlacementPrimitive[] {
    const ids: string[] = [];
    const visit = (p: PlacementPrimitive) => {
        if ((!p.locked || p.anchored) && p.layoutAlternatives?.length) ids.push(p.id);
        if (p.anchored) return;
        p.children.forEach(visit);
    };
    roots.forEach(visit);
    if (!ids.length) return roots;
    const placements = (ps: PlacementPrimitive[]) => ps.flatMap(p => p.placements);
    let current = roots;
    const alignment = boardAlignmentPolicy(input,roots);
    const gap = boardSpacingPolicy(input).gap;
    let score = globalPostPlaceScore(input, placements(current)) + blockPortfolioInternalScore(input, current) + boardSpacingPenalty(input,current,gap) + boardAlignmentScore(current,alignment);
    let report = createPlacementReport(input, placements(current));
    let evaluated = 0, accepted = 0;
    for (const id of ids) {
        const owner = find(current, id);
        if (!owner) continue;
        const constraints = encodeNativePostPlaceRefineProblem(input, placements(current), 1);
        let best = current, bestScore = score, bestReport = report;
        let bestVariant = 0, bestOffset = [0, 0];
        const variants = [owner, ...(owner.layoutAlternatives ?? [])];
        const offsets = owner.anchored ? [[0, 0]] : [[0, 0], [-grid, 0], [grid, 0], [0, -grid], [0, grid],
            [-2 * grid, 0], [2 * grid, 0], [0, -2 * grid], [0, 2 * grid]];
        const inventory = owner.placements.map(p => p.designator).sort().join('|');
        for (const variant of variants) for (const [dx, dy] of offsets) {
            if (variant === owner && dx === 0 && dy === 0) continue;
            if (variant.placements.map(p => p.designator).sort().join('|') !== inventory || (variant.locked && !owner.anchored)) continue;
            const replacement = translatePrimitive({ ...variant, layoutAlternatives: undefined }, dx, dy);
            const candidate = replace(current, id, replacement);
            const proposed = placements(candidate);
            const nextScore = globalPostPlaceScore(input, proposed) + blockPortfolioInternalScore(input, candidate) + boardSpacingPenalty(input,candidate,gap) + boardAlignmentScore(candidate,alignment);
            evaluated++;
            if (nextScore >= bestScore - 1e-6) continue;
            // Check the complete board, including polygon, holes and opposite-side
            // through-hole collisions. Never trade legality for a lower wire score.
            const nextReport = createPlacementReport(input, proposed);
            if (!nextReport.ok || !hintsNoWorse(report, nextReport)) continue;
            if (!loadNativeBoardPacker().validatePlacementChange(constraints, proposed)) continue;
            if (proposed.some(p => {
                const c = input.components.find(c => c.designator === p.designator)!;
                const original = placements(current).find(q => q.designator === p.designator)!;
                return (c.pcb.fixedPlacement && JSON.stringify(p) !== JSON.stringify(original))
                    || !c.pcb.allowedRotations.includes(p.rotate);
            })) continue;
            best = candidate; bestScore = nextScore; bestReport = nextReport;
            bestVariant = variants.indexOf(variant); bestOffset = [dx, dy];
        }
        if (best !== current) {
            accepted++;
            diagnostics.push({ severity: 'warning', nodeId: owner.sourceNodeId,
                message: `Block portfolio selected ${owner.label} in board context: variant ${bestVariant}, shift ${bestOffset.join(',')}mm; score ${score.toFixed(2)} -> ${bestScore.toFixed(2)}` });
            current = best; score = bestScore; report = bestReport;
        }
    }
    diagnostics.push({ severity: 'warning', nodeId: 'block-portfolio',
        message: `Block portfolio: ${ids.length} blocks, ${evaluated} candidates, ${accepted} accepted` });
    return current;
}

function hintsNoWorse(before: PlacementReport, after: PlacementReport) {
    return after.hintViolations.every(v => before.hintViolations.some(b => {
        if (JSON.stringify(b.hint) !== JSON.stringify(v.hint) || b.expected !== v.expected) return false;
        if (typeof b.actual !== 'number' || typeof v.actual !== 'number') return b.actual === v.actual;
        return v.expected.startsWith('>=') ? v.actual >= b.actual : v.actual <= b.actual;
    }));
}

function find(roots: PlacementPrimitive[], id: string): PlacementPrimitive | undefined {
    for (const p of roots) { if (p.id === id) return p; const child = find(p.children, id); if (child) return child; }
    return undefined;
}

function replace(roots: PlacementPrimitive[], id: string, replacement: PlacementPrimitive): PlacementPrimitive[] {
    return roots.map(p => {
        if (p.id === id) return replacement;
        if (!find(p.children, id)) return p;
        const children = replace(p.children, id, replacement);
        return { ...p, ...unionPrimitive(p.id, p.kind, p.label, p.sourceNodeId, children, p.deferredRelations), layoutAlternatives: undefined };
    });
}
