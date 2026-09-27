import { AsyncLocalStorage } from 'node:async_hooks';
import type { PlacementInput } from '#types/pcb/layout-model.ts';
import { isGroundSignalName, isPowerSignalName } from '#utils/signals.ts';
import { getPadWorld, unionBoxes } from '../pcb-auto-place/geometry.ts';
import { minimumSpanningEdges } from '../pcb-auto-place/ratsnest.ts';
import { encodeNativePostPlaceScoreProblem } from './native/encode-post-place-score.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import type { ClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { blockScopedInput } from './block-post-refiner.ts';
import type { PlacementPrimitive } from './primitives.ts';

export interface BlockQuality {
    score: number;
    electrical: number;
    localStretch: number;
    wire: number;
    maxLocal: number;
    area: number;
    width: number;
    height: number;
    exposure: number;
    links: Record<string, number>;
    ports: Record<string, { x: number; y: number }>;
}
export interface BlockCandidate {
    stage: string;
    hypothesis: string;
    primitives: PlacementPrimitive[];
    quality: BlockQuality;
}

const capture = new AsyncLocalStorage<(label: string, pool: BlockCandidate[], selected: BlockCandidate[]) => void>();
export function withBlockCandidateCapture<T>(callback: NonNullable<ReturnType<typeof capture.getStore>>, run: () => T): T {
    return capture.run(callback, run);
}
export function captureBlockCandidates(label: string, pool: BlockCandidate[], selected: BlockCandidate[]) {
    capture.getStore()?.(label, pool, selected);
}

export function legalBlockCandidate(input: PlacementInput, primitives: PlacementPrimitive[], clearance: ClearanceResolver) {
    const scoped = blockScopedInput(input, primitives);
    const placements = primitives.flatMap(p => p.placements);
    const problem = encodeNativePostPlaceRefineProblem(scoped, placements, 1);
    problem.pairClearances = scoped.components.map(a => scoped.components.map(b => clearance(a.designator, b.designator)));
    return loadNativeBoardPacker().validatePlacement(problem);
}

/** One role-independent acceptance objective. Search heuristics (including role
 * guesses and micro-route corrections) never enter the final comparison. */
export function blockQuality(input: PlacementInput, primitives: PlacementPrimitive[]): BlockQuality {
    const placements = primitives.flatMap(p => p.placements);
    const local = blockScopedInput(input, primitives);
    const problem = encodeNativePostPlaceScoreProblem(local, placements);
    const box = unionBoxes(primitives.map(p => p.bbox));
    const width = box.right - box.left, height = box.bottom - box.top, area = width * height;
    const poses = new Map(placements.map(p => [p.designator, p]));
    const ignored = new Set(input.solverOptions.ignoredRatsnestSignals.map(n => n.toUpperCase()));
    const nets = new Map<string, Array<{ x: number; y: number; ref: string; owner: string; core: boolean }>>();
    for (const c of local.components) for (const pin of c.pins) {
        const net = pin.signal_name;
        if (!net || ignored.has(net.toUpperCase()) || isGroundSignalName(net)) continue;
        const point = getPadWorld(c, poses.get(c.designator)!, pin.pin_number);
        if (!point) continue;
        const ps = nets.get(net) ?? [];
        ps.push({ ...point, ref: `${c.designator}.${pin.pin_number}`, owner: c.designator, core: c.pcb.role === 'main_ic' });
        nets.set(net, ps);
    }
    const links: Record<string, number> = {};
    const ports: BlockQuality['ports'] = {};
    let wire = 0, localStretch = 0, exposure = 0;
    const external = new Set(input.components.filter(c => !poses.has(c.designator)).flatMap(c => c.pins.map(p => p.signal_name)));
    for (const [net, points] of nets) {
        const weight = isPowerSignalName(net) ? .25 : 1;
        for (const [i, j] of minimumSpanningEdges(points)) {
            const a = points[i], b = points[j], d = Math.hypot(a.x - b.x, a.y - b.y);
            wire += d * weight;
            // Even multi-terminal nets pay for an individually stretched edge.
            if (a.owner !== b.owner) localStretch += weight * 2 * Math.max(0, d - 4) ** 2;
        }
        // MST can hide a distant passive behind another passive. Measure each
        // pin's access to each connected IC as well, using its nearest same-net pad.
        const cores = [...new Set(points.filter(p => p.core).map(p => p.owner))];
        for (const p of points.filter(p => !p.core)) for (const core of cores) {
            const d = Math.min(...points.filter(q => q.owner === core).map(q => Math.hypot(p.x - q.x, p.y - q.y)));
            links[`${p.ref}->${core}`] = d;
            localStretch += weight * (6 * d + 32 * Math.max(0, d - 3) ** 2);
        }
        if (external.has(net)) {
            exposure += weight * Math.min(...points.map(p =>
                Math.max(0, Math.min(p.x - box.left, box.right - p.x, p.y - box.top, box.bottom - p.y))));
            for (const p of points) ports[p.ref] = { x: p.x - (box.left + box.right) / 2, y: p.y - (box.top + box.bottom) / 2 };
        }
    }
    const electrical = loadNativeBoardPacker().scorePostPlace(problem) + localStretch;
    // mm² has a deliberately modest price compared with mm of local connection.
    return { score: electrical + .35 * area + .5 * (width + height) + 2 * exposure,
        electrical, localStretch, wire, maxLocal: Math.max(0, ...Object.values(links)), area, width, height, exposure, links, ports };
}

export function comparableBlockQuality(candidate: BlockQuality, best: BlockQuality) {
    return candidate.electrical <= best.electrical * 1.12 + 20
        && candidate.wire <= best.wire * 1.15 + 1
        && Object.entries(best.links).every(([key, d]) => (candidate.links[key] ?? Infinity) <= d + Math.max(1.5, d * .35));
}

function signature(primitives: PlacementPrimitive[]) {
    const ps = primitives.flatMap(p => p.placements).sort((a, b) => a.designator.localeCompare(b.designator));
    const origin = ps[0];
    return JSON.stringify(ps.map(p => [p.designator, Math.round((p.x - origin.x) * 1000),
        Math.round((p.y - origin.y) * 1000), p.rotate, p.layer]));
}

/** Quality gate first, then retain useful geometry/port diversity; never pad to three. */
export function selectBlockCandidates(candidates: BlockCandidate[]): BlockCandidate[] {
    const seen = new Set<string>();
    const ranked = candidates.filter(c => Number.isFinite(c.quality.score)).sort((a, b) => a.quality.score - b.quality.score)
        .filter(c => { const key = signature(c.primitives); if (seen.has(key)) return false; seen.add(key); return true; });
    if (!ranked.length) return [];
    const best = ranked[0];
    const comparable = ranked.filter(c => comparableBlockQuality(c.quality, best.quality));
    const selected = [best];
    for (const c of comparable.slice(1)) {
        // A lower-quality candidate needs a useful improvement in shape or access.
        const q = c.quality;
        if (selected.some(s => s.quality.electrical <= q.electrical + 1 && s.quality.wire <= q.wire + .05
            && s.quality.width <= q.width + .1 && s.quality.height <= q.height + .1 && s.quality.exposure <= q.exposure + .1
            && Object.entries(q.ports).every(([key, p]) => {
                const other = s.quality.ports[key];
                return other && Math.hypot(p.x - other.x, p.y - other.y) < .5;
            }))) continue;
        selected.push(c);
        if (selected.length === 3) break;
    }
    return selected;
}

/** Re-score retained layouts in world coordinates with the very same objective. */
export function blockPortfolioInternalScore(input: PlacementInput, roots: PlacementPrimitive[]): number {
    return roots.reduce((sum, p) => sum + (p.blockQuality ? blockQuality(input, p.children).score
        : blockPortfolioInternalScore(input, p.children)), 0);
}
