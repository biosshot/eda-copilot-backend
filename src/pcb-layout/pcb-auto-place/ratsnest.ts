import * as fp from '../f32.ts';
import type { Point } from '#types/pcb/layout-model.ts';

/** Same deterministic Prim tree and 0.001 mm tie tolerance as native post-place scoring. */
export function minimumSpanningEdges(points: Point[]): Array<[number, number]> {
    const used = new Set<number>(points.length ? [0] : []);
    const edges: Array<[number, number]> = [];
    while (used.size < points.length) {
        let best: [number, number, number] | undefined;
        for (let a = 0; a < points.length; a++) if (used.has(a)) {
            for (let b = 0; b < points.length; b++) if (!used.has(b)) {
                const d = fp.hypot(fp.sub(points[a].x, points[b].x), fp.sub(points[a].y, points[b].y));
                if (!best || d < fp.sub(best[2], .001) || (fp.abs(fp.sub(d, best[2])) <= .001 && (a < best[0] || (a === best[0] && b < best[1])))) best = [a, b, d];
            }
        }
        if (!best) break;
        used.add(best[1]); edges.push([best[0], best[1]]);
    }
    return edges;
}
