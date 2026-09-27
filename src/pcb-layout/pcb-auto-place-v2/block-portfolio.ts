import type { PlacementInput, PlacementReport } from '#types/pcb/layout-model.ts';
import { createPlacementReport } from '../pcb-auto-place/placement-report.ts';
import { globalPostPlaceScore } from './post-place-refiner.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { translatePrimitive, unionPrimitive, type PlacementPrimitive, type PrimitiveSolveDiagnostic } from './primitives.ts';

/** Select internal block layouts in their actual board neighbourhood. This is a
 * bounded coordinate descent, not a second postrefine or a global repacking.
 * A candidate retains ownership and may slide by at most two placement steps.
 */
export function selectBlockPortfolio(input: PlacementInput, roots: PlacementPrimitive[], grid: number,
    diagnostics: PrimitiveSolveDiagnostic[] = []): PlacementPrimitive[] {
    const ids: string[] = [];
    const visit = (p: PlacementPrimitive) => {
        if (!p.locked && p.layoutAlternatives?.length) ids.push(p.id);
        p.children.forEach(visit);
    };
    roots.forEach(visit);
    if (!ids.length) return roots;
    const placements = (ps: PlacementPrimitive[]) => ps.flatMap(p => p.placements);
    let current = roots;
    let score = globalPostPlaceScore(input, placements(current));
    let report = createPlacementReport(input, placements(current));
    let evaluated = 0, accepted = 0;
    for (const id of ids) {
        const owner = find(current, id)!;
        const constraints = encodeNativePostPlaceRefineProblem(input, placements(current), 1);
        let best = current, bestScore = score, bestReport = report;
        let bestVariant = 0, bestOffset = [0, 0];
        const variants = [owner, ...(owner.layoutAlternatives ?? [])];
        const offsets = [[0, 0], [-grid, 0], [grid, 0], [0, -grid], [0, grid],
            [-2 * grid, 0], [2 * grid, 0], [0, -2 * grid], [0, 2 * grid]];
        const inventory = owner.placements.map(p => p.designator).sort().join('|');
        for (const variant of variants) for (const [dx, dy] of offsets) {
            if (variant === owner && dx === 0 && dy === 0) continue;
            if (variant.placements.map(p => p.designator).sort().join('|') !== inventory || variant.locked) continue;
            const replacement = translatePrimitive({ ...variant, layoutAlternatives: undefined }, dx, dy);
            const candidate = replace(current, id, replacement);
            const proposed = placements(candidate);
            const nextScore = globalPostPlaceScore(input, proposed);
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
