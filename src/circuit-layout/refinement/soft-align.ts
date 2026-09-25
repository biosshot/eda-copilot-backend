import type { ElkExtendedEdge } from 'elkjs';
import { type Placed, boundsOf, expand, overlaps, path, routeLength } from './geometry.ts';
import { SCHEMATIC_CLEARANCE as gap, componentClearance } from './policy.ts';
import { RouteEnvironment, localCrossings, reconnect } from './router.ts';

/** Nearby large symbols form optional visual rows. This is deliberately based
 * on the existing layout: an IC with a separate functional circuit is not
 * pulled across the block merely because it has the same designator prefix. */
function rows(nodes: Placed[], originals: ReadonlySet<string>, protectedIds: ReadonlySet<string>,
    blocks?: ReadonlyMap<string, string>) {
    const chips = nodes.filter(n => originals.has(n.id) && !protectedIds.has(n.id)
        && /^U/i.test(n.id) && (n.ports?.length ?? 0) >= 16).sort((a, b) => a.x - b.x || a.id.localeCompare(b.id));
    const parent = chips.map((_, i) => i);
    const find = (i: number): number => parent[i] === i ? i : (parent[i] = find(parent[i]));
    for (let i = 0; i < chips.length; i++) for (let j = i + 1; j < chips.length; j++) {
        const a = chips[i], b = chips[j];
        if (blocks && blocks.get(a.id) !== blocks.get(b.id)) continue;
        const horizontalGap = Math.max(0, b.x - a.x - a.width);
        if (horizontalGap > 120 || Math.abs(a.y - b.y) > Math.max(30, Math.min(a.height, b.height) * 0.25)) continue;
        parent[find(j)] = find(i);
    }
    const groups = new Map<number, Placed[]>();
    chips.forEach((n, i) => { const root = find(i), group = groups.get(root) ?? []; group.push(n); groups.set(root, group); });
    return [...groups.values()].filter(group => group.length > 1);
}

const crossingCount = (edges: ElkExtendedEdge[], env: RouteEnvironment) =>
    [...localCrossings(edges, env).values()].reduce((total, count) => total + count, 0);

/** Move one IC toward the row established by its neighbours. All incident
 * routes are rebuilt; collision, crossing, wire length and footprint remain
 * vetoes or bounded costs. Unconnected IC sections can align without routing. */
export function softlyAlignMajorComponents(nodes: Placed[], edges: ElkExtendedEdge[], nets: ReadonlyMap<string, string>,
    originals: ReadonlySet<string>, protectedIds: ReadonlySet<string> = new Set(), blocks?: ReadonlyMap<string, string>) {
    let moved = 0;
    for (const row of rows(nodes, originals, protectedIds, blocks)) {
        const ordered = row.map(n => ({ n, count: edges.filter(e => [...e.sources, ...e.targets].some(p => n.ports?.some(q => q.id === p))).length }))
            .sort((a, b) => a.n.y - b.n.y || b.count - a.count || a.n.id.localeCompare(b.n.id));
        const total = ordered.reduce((sum, entry) => sum + 1 + Math.min(8, entry.count), 0);
        let weight = 0, target = ordered[0].n.y;
        for (const entry of ordered) {
            weight += 1 + Math.min(8, entry.count);
            if (weight >= total / 2) { target = entry.n.y; break; }
        }
        for (const { n: original } of ordered.toSorted((a, b) => a.count - b.count || a.n.id.localeCompare(b.n.id))) {
            const node = nodes.find(n => n.id === original.id)!;
            const delta = Math.max(-80, Math.min(80, target - node.y));
            if (Math.abs(delta) < 2) continue;
            const pins = new Set((node.ports ?? []).map(p => p.id));
            const incident = edges.filter(e => [...e.sources, ...e.targets].some(p => pins.has(p)));
            if (incident.length > 48) continue;
            const ids = new Set(incident.map(e => e.id)), fixed = nodes.filter(n => n.id !== node.id);
            const env = new RouteEnvironment(fixed, edges.filter(e => !ids.has(e.id)), nets);
            const before = crossingCount(incident, env);
            const oldLength = incident.reduce((sum, edge) => sum + routeLength(path(edge)), 0);
            const oldBox = boundsOf([...nodes, ...edges.flatMap(path).map(p => ({ ...p, width: 0, height: 0 }))]);
            let best: { node: Placed; routes: ElkExtendedEdge[]; gain: number } | undefined;
            for (const fraction of [1, 0.75, 0.5, 0.25]) {
                const candidate = { ...node, y: node.y + delta * fraction };
                if (fixed.some(other => overlaps(candidate, other, originals.has(other.id)
                    ? componentClearance(candidate, other) : gap.port))) continue;
                if (env.wires.query(expand(candidate, gap.wire)).some(s => {
                    const box = expand(candidate, gap.wire);
                    return Math.max(s.a.x, s.b.x) > box.x && Math.min(s.a.x, s.b.x) < box.x + box.width
                        && Math.max(s.a.y, s.b.y) > box.y && Math.min(s.a.y, s.b.y) < box.y + box.height;
                })) continue;
                const routes: ElkExtendedEdge[] = [];
                for (const edge of incident) {
                    const next = reconnect(edge, [candidate], env, routes) ?? reconnect(edge, [candidate], env, routes, true);
                    if (!next) break;
                    routes.push(next);
                }
                if (routes.length !== incident.length || crossingCount(routes, env) > before) continue;
                const newLength = routes.reduce((sum, edge) => sum + routeLength(path(edge)), 0);
                const gain = Math.abs(node.y - target) - Math.abs(candidate.y - target);
                if (newLength - oldLength > Math.max(20, gain * 1.5)) continue;
                const nextNodes = nodes.map(n => n.id === node.id ? candidate : n);
                const nextEdges = edges.map(e => routes.find(r => r.id === e.id) ?? e);
                const box = boundsOf([...nextNodes, ...nextEdges.flatMap(path).map(p => ({ ...p, width: 0, height: 0 }))]);
                if (box.width * box.height > oldBox.width * oldBox.height * 1.02) continue;
                if (!best || gain > best.gain) best = { node: candidate, routes, gain };
            }
            if (!best) continue;
            nodes = nodes.map(n => n.id === node.id ? best.node : n);
            const replacement = new Map(best.routes.map(e => [e.id, e]));
            edges = edges.map(e => replacement.get(e.id) ?? e);
            moved++;
        }
    }
    return { nodes, edges, moved };
}
