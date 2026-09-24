import type { ElkExtendedEdge } from 'elkjs';
import { connectedNetEdges, pointKey, pointOnSegment, straightRuns } from './net-routes.ts';
import { type Point, type Segment, EPS, path, edgeSegments, routeLength, withPath, simplifyRoute } from './geometry.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';

/** Remove redundant ink using only existing same-net segments. Unlike routing
 * coalescence this never adds a bridge. Terminal escapes are mandatory tree
 * edges, and every original logical connection is reconstructed on the tree. */
export function removeNetCycles(edges: ElkExtendedEdge[], nets: ReadonlyMap<string, string>) {
    const replacements = new Map<string, ElkExtendedEdge>();
    let removed = 0;
    groups: for (const group of connectedNetEdges(edges, nets)) {
        const input = group.flatMap(edgeSegments);
        if (input.length > 250 || group.some(e => path(e).length < 2)
            || input.some(s => Math.abs(s.a.x - s.b.x) > EPS && Math.abs(s.a.y - s.b.y) > EPS)) continue;
        const runs = straightRuns(input), points = new Map<string, Point>();
        const add = (p: Point) => points.set(pointKey(p), p);
        input.forEach(s => { add(s.a); add(s.b); });
        const protectedRuns: Segment[] = [];
        for (const e of group) for (const p of [path(e), path(e).toReversed()]) {
            const distance = routeLength(p.slice(0, 2)), escape = Math.min(distance, gap.pinEscape);
            if (distance < EPS) continue;
            const end = { x: p[0].x + (p[1].x - p[0].x) * escape / distance,
                y: p[0].y + (p[1].y - p[0].y) * escape / distance };
            add(end); protectedRuns.push({ a: p[0], b: end });
        }
        for (const a of runs) for (const b of runs) {
            if ((Math.abs(a.a.x - a.b.x) < EPS) === (Math.abs(b.a.x - b.b.x) < EPS)) continue;
            const p = Math.abs(a.a.x - a.b.x) < EPS ? { x: a.a.x, y: b.a.y } : { x: b.a.x, y: a.a.y };
            if (pointOnSegment(p, a) && pointOnSegment(p, b)) add(p);
        }
        const segments: { a: string; b: string; length: number; mandatory: boolean }[] = [];
        for (const run of runs) {
            const stops = [...points].filter(([, p]) => pointOnSegment(p, run))
                .sort((a, b) => a[1].x - b[1].x || a[1].y - b[1].y);
            for (let i = 1; i < stops.length; i++) {
                const [a, p] = stops[i - 1], [b, q] = stops[i];
                segments.push({ a, b, length: routeLength([p, q]), mandatory: protectedRuns.some(s => pointOnSegment(p, s) && pointOnSegment(q, s)) });
            }
        }
        // Equal-cost trees can use different terminal exits. Try both geometric
        // tie orders; string ordering of coordinates is not translation invariant.
        for (const order of [1, -1]) {
            const parent = new Map([...points.keys()].map(k => [k, k]));
            const root = (k: string): string => { const p = parent.get(k)!; if (p === k) return k; const r = root(p); parent.set(k, r); return r; };
            const tree = new Map<string, string[]>();
            let cycles = 0, protectedCycle = false;
            segments.sort((a, b) => Number(b.mandatory) - Number(a.mandatory) || a.length - b.length
                || order * (points.get(a.a)!.x - points.get(b.a)!.x || points.get(a.a)!.y - points.get(b.a)!.y
                    || points.get(a.b)!.x - points.get(b.b)!.x || points.get(a.b)!.y - points.get(b.b)!.y));
            for (const s of segments) {
                const a = root(s.a), b = root(s.b);
                if (a === b) { cycles++; if (s.mandatory) protectedCycle = true; continue; }
                parent.set(a, b);
                for (const [from, to] of [[s.a, s.b], [s.b, s.a]]) tree.set(from, [...tree.get(from) ?? [], to]);
            }
            if (!cycles || protectedCycle) continue groups;
            const net = nets.get(group[0].sources[0]);
            const foreign = edges.filter(e => nets.get(e.sources[0]) !== net).flatMap(edgeSegments);
            const trial: ElkExtendedEdge[] = [];
            for (const e of group) {
                const old = path(e), start = pointKey(old[0]), end = pointKey(old.at(-1)!);
                const previous = new Map<string, string>(), queue = [start], seen = new Set(queue);
                for (let i = 0; i < queue.length && !seen.has(end); i++) for (const next of tree.get(queue[i]) ?? []) {
                    if (seen.has(next)) continue;
                    seen.add(next); previous.set(next, queue[i]); queue.push(next);
                }
                if (!seen.has(end)) break;
                const route = [points.get(end)!]; let cursor = end;
                while (cursor !== start) { cursor = previous.get(cursor)!; route.push(points.get(cursor)!); }
                const next = simplifyRoute(route.reverse());
                // Preserve both terminal exits; never turn a plain foreign crossing
                // into a corner/contact in the serialized drawing.
                if (next.length < 2 || next.some(p => foreign.some(s => pointOnSegment(p, s)))
                    || [false, true].some(reverse => {
                        const p = reverse ? old.toReversed() : old, q = reverse ? next.toReversed() : next;
                        return (p[1].x - p[0].x) * (q[1].x - q[0].x) + (p[1].y - p[0].y) * (q[1].y - q[0].y) <= 0
                            || routeLength(q.slice(0, 2)) + EPS < Math.min(gap.pinEscape, routeLength(p.slice(0, 2)));
                    })) break;
                trial.push(withPath(e, next));
            }
            if (trial.length !== group.length) continue;
            trial.forEach(e => replacements.set(e.id, e)); removed += cycles;
            continue groups;
        }
    }
    return { edges: edges.map(e => replacements.get(e.id) ?? e), removed };
}
