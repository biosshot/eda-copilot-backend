import * as fp from '../f32.ts';
import { AsyncLocalStorage } from 'node:async_hooks';
import type { PlacementInput } from '#types/pcb/layout-model.ts';
import { isGroundSignalName, isPowerSignalName } from '#utils/signals.ts';
import { getPadWorld, unionBoxes } from '../pcb-auto-place/geometry.ts';
import { createFixedPlacement } from '../pcb-auto-place/fixed.ts';
import { minimumSpanningEdges } from '../pcb-auto-place/ratsnest.ts';
import { encodeNativePostPlaceScoreProblem } from './native/encode-post-place-score.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import type { ClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { blockScopedInput, scopedPlacements } from './block-post-refiner.ts';
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

export function legalBlockCandidate(input: PlacementInput, primitives: PlacementPrimitive[], clearance: ClearanceResolver, world = false) {
    if (world && primitives.flatMap(p=>p.placements).some(p=> {
        const c=input.components.find(c=>c.designator===p.designator)!;
        const fixed=createFixedPlacement(input,c);
        return fixed && (p.x!==fixed.x || p.y!==fixed.y || p.rotate!==fixed.rotate || p.layer!==fixed.layer);
    })) return false;
    const scoped = blockScopedInput(input, primitives, world);
    const placements = scopedPlacements(input, primitives, world);
    const problem = encodeNativePostPlaceRefineProblem(scoped, placements, 1);
    problem.pairClearances = scoped.components.map(a => scoped.components.map(b => clearance(a.designator, b.designator)));
    const scope = world ? primitives.flatMap(p=>p.placements).map(p=>p.designator)
        .filter(d=>!input.components.find(c=>c.designator===d)?.pcb.fixedPlacement) : undefined;
    return loadNativeBoardPacker().validatePlacement(problem, scope);
}

/** One role-independent acceptance objective. Search heuristics (including role
 * guesses and micro-route corrections) never enter the final comparison. */
export function blockQuality(input: PlacementInput, primitives: PlacementPrimitive[]): BlockQuality {
    const placements = primitives.flatMap(p => p.placements);
    const local = blockScopedInput(input, primitives);
    const problem = encodeNativePostPlaceScoreProblem(local, placements);
    const box = unionBoxes(primitives.map(p => p.bbox));
    const width = fp.sub(box.right, box.left), height = fp.sub(box.bottom, box.top), area = fp.mul(width, height);
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
            const a = points[i], b = points[j], d = fp.hypot(fp.sub(a.x, b.x), fp.sub(a.y, b.y));
            wire = fp.add(wire, fp.mul(d, weight));
            // Even multi-terminal nets pay for an individually stretched edge.
            if (a.owner !== b.owner) localStretch = fp.add(localStretch, fp.mul(fp.mul(weight, 2), fp.pow(fp.max(0, fp.sub(d, 4)), 2)));
        }
        // MST can hide a distant passive behind another passive. Measure each
        // pin's access to each connected IC as well, using its nearest same-net pad.
        const cores = [...new Set(points.filter(p => p.core).map(p => p.owner))];
        for (const p of points.filter(p => !p.core)) for (const core of cores) {
            const d = fp.min(...points.filter(q => q.owner === core).map(q => fp.hypot(fp.sub(p.x, q.x), fp.sub(p.y, q.y))));
            links[`${p.ref}->${core}`] = d;
            localStretch = fp.add(localStretch, fp.mul(weight, (fp.add(fp.mul(6, d), fp.mul(32, fp.pow(fp.max(0, fp.sub(d, 3)), 2))))));
        }
        if (external.has(net)) {
            exposure = fp.add(exposure, fp.mul(weight, fp.min(...points.map(p =>
                fp.max(0, fp.min(fp.sub(p.x, box.left), fp.sub(box.right, p.x), fp.sub(p.y, box.top), fp.sub(box.bottom, p.y)))))));
            for (const p of points) ports[p.ref] = { x: fp.sub(p.x, fp.div((fp.add(box.left, box.right)), 2)), y: fp.sub(p.y, fp.div((fp.add(box.top, box.bottom)), 2)) };
        }
    }
    const electrical = fp.add(loadNativeBoardPacker().scorePostPlace(problem), localStretch);
    // mm² has a deliberately modest price compared with mm of local connection.
    return { score: fp.add(fp.add(fp.add(electrical, fp.mul(.35, area)), fp.mul(.5, (fp.add(width, height)))), fp.mul(2, exposure)),
        electrical, localStretch, wire, maxLocal: fp.max(0, ...Object.values(links)), area, width, height, exposure, links, ports };
}

export function comparableBlockQuality(candidate: BlockQuality, best: BlockQuality) {
    return candidate.electrical <= fp.add(fp.mul(best.electrical, 1.12), 20)
        && candidate.wire <= fp.add(fp.mul(best.wire, 1.15), 1)
        && Object.entries(best.links).every(([key, d]) => (candidate.links[key] ?? Infinity) <= fp.add(d, fp.max(1.5, fp.mul(d, .35))));
}

function signature(primitives: PlacementPrimitive[]) {
    const ps = primitives.flatMap(p => p.placements).sort((a, b) => a.designator.localeCompare(b.designator));
    const origin = ps[0];
    return JSON.stringify(ps.map(p => [p.designator, Math.round(fp.mul((fp.sub(p.x, origin.x)), 1000)),
        Math.round(fp.mul((fp.sub(p.y, origin.y)), 1000)), p.rotate, p.layer]));
}

/** Quality gate first, then retain useful geometry/port diversity; never pad to three. */
export function selectBlockCandidates(candidates: BlockCandidate[]): BlockCandidate[] {
    const seen = new Set<string>();
    const ranked = candidates.filter(c => Number.isFinite(c.quality.score)).sort((a, b) => fp.sub(a.quality.score, b.quality.score))
        .filter(c => { const key = signature(c.primitives); if (seen.has(key)) return false; seen.add(key); return true; });
    if (!ranked.length) return [];
    const best = ranked[0];
    const comparable = ranked.filter(c => comparableBlockQuality(c.quality, best.quality));
    const selected = [best];
    for (const c of comparable.slice(1)) {
        // A lower-quality candidate needs a useful improvement in shape or access.
        const q = c.quality;
        if (selected.some(s => s.quality.electrical <= fp.add(q.electrical, 1) && s.quality.wire <= fp.add(q.wire, .05)
            && s.quality.width <= fp.add(q.width, .1) && s.quality.height <= fp.add(q.height, .1) && s.quality.exposure <= fp.add(q.exposure, .1)
            && Object.entries(q.ports).every(([key, p]) => {
                const other = s.quality.ports[key];
                return other && fp.hypot(fp.sub(p.x, other.x), fp.sub(p.y, other.y)) < .5;
            }))) continue;
        selected.push(c);
        if (selected.length === 3) break;
    }
    return selected;
}

/** Re-score retained layouts in world coordinates with the very same objective. */
export function blockPortfolioInternalScore(input: PlacementInput, roots: PlacementPrimitive[]): number {
    return roots.reduce((sum, p) => fp.add(sum, (p.blockQuality ? blockQuality(input, p.children).score
        : blockPortfolioInternalScore(input, p.children))), 0);
}
