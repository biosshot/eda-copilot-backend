import * as fp from '../f32.ts';
import type { BlockCandidate } from './block-quality.ts';
import { comparableBlockQuality } from './block-quality.ts';

/** Group membership and mobility affect what local refinement is allowed to do. */
function structure(c: BlockCandidate) {
    return JSON.stringify(c.primitives.map(p => [p.locked, p.kind, p.canRotate,
        [...(p.allowedOrientations ?? [])], p.placements.map(q => q.designator).sort()])
        .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))));
}

/** Search-work equivalence, never a rounded native-cache key. Originals remain
 * in the final pool with their own exact scores and positions. */
export function nearBlockLayout(a: BlockCandidate, b: BlockCandidate, tolerance = .15): boolean {
    if (structure(a) !== structure(b)) return false;
    const qa = a.quality, qb = b.quality;
    // Tight absolute gates prevent large scores from hiding a pad-access change.
    if (fp.abs(fp.sub(qa.electrical, qb.electrical)) > .5 || fp.abs(fp.sub(qa.score, qb.score)) > 1
        || fp.abs(fp.sub(qa.wire, qb.wire)) > .1 || fp.abs(fp.sub(qa.maxLocal, qb.maxLocal)) > .1) return false;
    const links = new Set([...Object.keys(qa.links), ...Object.keys(qb.links)]);
    if ([...links].some(k => qa.links[k] === undefined || qb.links[k] === undefined || fp.abs(fp.sub(qa.links[k], qb.links[k])) > .1)) return false;
    const poses = (c: BlockCandidate) => c.primitives.flatMap(p => p.placements).sort((x, y) => x.designator.localeCompare(y.designator));
    const pa = poses(a), pb = poses(b);
    if (!pa.length || pa.length !== pb.length) return false;
    const absolute = a.primitives.some(p => p.locked) || b.primitives.some(p => p.locked);
    const dx = absolute ? 0 : fp.sub(pa[0].x, pb[0].x), dy = absolute ? 0 : fp.sub(pa[0].y, pb[0].y);
    return pa.every((p, i) => p.designator === pb[i].designator && p.rotate === pb[i].rotate && p.layer === pb[i].layer
        && fp.hypot(fp.sub(fp.sub(p.x, pb[i].x), dx), fp.sub(fp.sub(p.y, pb[i].y), dy)) <= tolerance);
}

/** At most two distinct, electrically competitive single-move checkpoints.
 * Every hypothesis has already completed beam and singles before this gate. */
export function selectPairSeeds<T extends BlockCandidate>(seeds: T[], pool: BlockCandidate[], limit = 2): T[] {
    const best = [...pool].sort((a, b) => fp.sub(a.quality.score, b.quality.score))[0];
    // Judge the hypothesis by its best saved checkpoint, not just its singles
    // state. Otherwise its own successful postrefine can exclude its pair stage.
    const bestFor = (c: T) => pool.filter(p => p.hypothesis === c.hypothesis)
        .reduce((best, p) => p.quality.score < best.quality.score ? p : best, c);
    const selected: T[] = [];
    for (const candidate of [...seeds].sort((a, b) => fp.sub(bestFor(a).quality.score, bestFor(b).quality.score))) {
        if (candidate.primitives.length > 12 || candidate.primitives.filter(p => !p.locked).length < 2) continue;
        if (best && !comparableBlockQuality(bestFor(candidate).quality, best.quality)) continue;
        if (selected.some(other => nearBlockLayout(candidate, other))) continue;
        selected.push(candidate);
        if (selected.length >= limit) break;
    }
    return selected;
}
