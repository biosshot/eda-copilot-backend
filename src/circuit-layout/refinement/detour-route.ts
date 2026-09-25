import type { ElkExtendedEdge } from 'elkjs';
import { type Placed, type Point, EPS, edgeSegments, exitsAlong, normal, path, routeLength, simplifyRoute, withPath } from './geometry.ts';
import { measureRouteShape, segmentLength, straightRuns } from './net-routes.ts';
import { RouteEnvironment, clearPath, localCrossings, reconnect } from './router.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';

const MIN_LENGTH = 480;
const MIN_EXCESS = 180;
const MAX_EDGES = 12;
const MAX_TRACKS = 80;
const MAX_TRIALS = 320;

const crossingCount = (edges: ElkExtendedEdge[], env: RouteEnvironment) =>
    [...localCrossings(edges, env).values()].reduce((sum, count) => sum + count, 0);
const physicalLength = (edges: ElkExtendedEdge[]) =>
    straightRuns(edges.flatMap(edgeSegments)).reduce((sum, segment) => sum + segmentLength(segment), 0);
const same = (a: number, b: number) => Math.abs(a - b) < EPS;

/** Revisit long detours after all bodies and flags have stopped moving. A
 * horizontal pair of IC terminals may require both an outer pin corridor and
 * a separate channel above/below the intervening bodies. The ordinary router
 * samples one channel at a time, so enumerate these bounded combinations. */
export function rerouteFixedDetours(nodes: Placed[], input: ElkExtendedEdge[], nets: ReadonlyMap<string, string>) {
    let edges = input, changed = 0;
    const candidates = input.map(edge => {
        const p = path(edge), length = routeLength(p);
        const direct = p.length >= 2 ? Math.abs(p[0].x - p.at(-1)!.x) + Math.abs(p[0].y - p.at(-1)!.y) : length;
        return { edge, length, direct, excess: length - direct };
    }).filter(item => item.length >= MIN_LENGTH && item.excess >= MIN_EXCESS
        && item.length >= Math.max(1, item.direct) * 1.6)
        .sort((a, b) => b.excess - a.excess || a.edge.id.localeCompare(b.edge.id)).slice(0, MAX_EDGES);
    for (const item of candidates) {
        const edge = edges.find(e => e.id === item.edge.id)!;
        const original = path(edge), a = original[0], b = original.at(-1)!;
        if (!a || !b || edge.sources.length !== 1 || edge.targets.length !== 1 || !nets.get(edge.sources[0])
            || nets.get(edge.sources[0]) !== nets.get(edge.targets[0])) continue;
        const owner = nodes.find(n => n.ports?.some(p => p.id === edge.sources[0]));
        const target = nodes.find(n => n.ports?.some(p => p.id === edge.targets[0]));
        if (!owner || !target) continue;
        const an = normal(owner, edge.sources[0]), bn = normal(target, edge.targets[0]);
        const vertical = !!an.y && !!bn.y;
        if (!vertical && (!an.x || !bn.x)) continue;
        const project = (p: Point) => vertical ? { x: p.y, y: p.x } : p;
        const old = original.map(project), start = project(a), end = project(b);
        const outwardA = vertical ? an.y : an.x, outwardB = vertical ? bn.y : bn.x;
        const startEscape = Math.min(gap.pinEscape, Math.max(gap.wire, routeLength(original.slice(0, 2))));
        const endEscape = Math.min(gap.pinEscape, Math.max(gap.wire, routeLength(original.slice(-2))));
        const outer = [...new Set([start.x + outwardA * startEscape, old[1].x])]
            .filter(x => (x - start.x) * outwardA >= startEscape - EPS);
        const inner = [...new Set([end.x + outwardB * endEscape, old.at(-2)!.x])]
            .filter(x => (x - end.x) * outwardB >= endEscape - EPS);
        if (!outer.length || !inner.length) continue;
        const spanLow = Math.min(start.x, end.x), spanHigh = Math.max(start.x, end.x);
        const tracks = [...new Set([
            ...[owner, target].flatMap(n => { const top = vertical ? n.x : n.y, size = vertical ? n.width : n.height;
                return [top - gap.pinEscape, top + size + gap.pinEscape]; }),
            ...nodes.filter(n => { const left = vertical ? n.y : n.x, width = vertical ? n.height : n.width;
                return left < spanHigh + 180 && left + width > spanLow - 180;
            }).flatMap(n => { const top = vertical ? n.x : n.y, size = vertical ? n.width : n.height;
                return [top - gap.pinEscape, top + size + gap.pinEscape]; }),
            ...old.map(p => p.y),
        ])].filter(y => Number.isFinite(y))
            .sort((x, y) => Math.abs(start.y - x) + Math.abs(end.y - x)
                - Math.abs(start.y - y) - Math.abs(end.y - y) || x - y).slice(0, MAX_TRACKS);
        const rest = edges.filter(e => e.id !== edge.id), env = new RouteEnvironment(nodes, rest, nets);
        const net = nets.get(edge.sources[0])!, sameNet = rest.filter(e => nets.get(e.sources[0]) === net);
        const oldCrossings = crossingCount([edge], env), oldPhysical = physicalLength([...sameNet, edge]);
        const oldJogs = measureRouteShape([...sameNet, edge]).shortJogs;
        const owners = new Set([owner.id, target.id]);
        let best: { edge: ElkExtendedEdge; length: number; physical: number; crossings: number } | undefined;
        let attempts = 0;
        const consider = (points: Point[]) => {
            const p = simplifyRoute(points), length = routeLength(p);
            if (length > item.length - Math.max(80, item.length * 0.15) + EPS
                || !exitsAlong(p, an, startEscape) || !exitsAlong(p.toReversed(), bn, endEscape)
                || !clearPath(p, net, env, [], [], owners)) return;
            const next = withPath(edge, p), crossings = crossingCount([next], env);
            if (crossings > oldCrossings) return;
            const group = [...sameNet, next], physical = physicalLength(group);
            if (physical >= oldPhysical - gap.branch || measureRouteShape(group).shortJogs > oldJogs) return;
            if (!best || physical < best.physical - EPS || (same(physical, best.physical)
                && (crossings < best.crossings || (crossings === best.crossings && length < best.length - EPS))))
                best = { edge: next, length, physical, crossings };
        };
        // A direct reroute may already succeed on a simple two-bend connection.
        const direct = reconnect(edge, [], env, [], true);
        if (direct) consider(path(direct));
        outerLoop: for (const y of tracks) for (const x1 of outer) for (const x2 of inner) {
            if (++attempts > MAX_TRIALS) break outerLoop;
            const projected = [start, { x: x1, y: start.y }, { x: x1, y }, { x: x2, y },
                { x: x2, y: end.y }, end];
            consider(projected.map(project));
        }
        if (best) { edges = edges.map(e => e.id === edge.id ? best!.edge : e); changed++; }
    }
    return { edges, changed };
}
