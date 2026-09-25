import type { ElkExtendedEdge } from 'elkjs';
import type { CircuitComponent } from '#types/circuit.ts';
import { type Placed, normal, pinPositions, overlaps, expand, edgeSegments, segmentThroughBox, pointOnSegment, path, routeLength, boundsOf, withPath, simplifyRoute } from './geometry.ts';
import { RouteEnvironment, reconnect, localCrossings, clearPath } from './router.ts';
import { connectedNetEdges, coalesceNetRoutes } from './net-routes.ts';
import { turnNode } from './groups.ts';
import { acceptsFlagOrientation, flagReadabilityCost } from './flag-policy.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';
import { effectiveLayoutArea } from '../quality.ts';
import { removeNetCycles } from './net-cycles.ts';

/** Rebuild private flag leads together: the old fan-out must not obstruct
 * the slots needed by neighbouring ports. A shared lead is movable only when
 * removing it leaves the complete remaining tree and its anchor connected. */
export function placeNearbyFlags(nodes: Placed[], edges: ElkExtendedEdge[], added: readonly CircuitComponent[], nets: ReadonlyMap<string, string>) {
    const flags = new Map(added.map(c => [c.designator, c]));
    const owners = new Map(nodes.flatMap(n => (n.ports ?? []).map(p => [p.id, n])));
    const groups = new Map<string, Array<{ flag: Placed; edge: ElkExtendedEdge; anchor: string }>>();
    const buses = connectedNetEdges(edges, nets);
    const positions = pinPositions(nodes);
    const sharedNets = new Set<string>();
    for (const flag of nodes) {
        if (!flags.has(flag.id) || flag.ports?.length !== 1) continue;
        const id = flag.ports[0].id, incident = edges.filter(e => [...e.sources, ...e.targets].includes(id));
        if (incident.length !== 1) continue;
        const edge = incident[0], anchor = edge.sources[0] === id ? edge.targets[0] : edge.sources[0];
        const owner = owners.get(anchor);
        if (!owner || flags.has(owner.id)) continue;
        const bus = buses.find(g => g.includes(edge));
        if (!bus) continue;
        if (bus.length > 1) {
            const remaining = bus.filter(e => e !== edge), net = nets.get(id)!;
            if (sharedNets.has(net) || connectedNetEdges(remaining, nets).length !== 1
                || !remaining.some(e => edgeSegments(e).some(s => pointOnSegment(positions.get(anchor)!, s)))) continue;
            sharedNets.add(net);
        }
        // A busy supply tap on the opposite IC face must not veto a complete
        // row of private signals. Solve each face/bus class independently.
        const face = normal(owner, anchor);
        const kind = flags.get(flag.id)!.part_uuid === 'GND' ? 'ground' : flags.get(flag.id)!.part_uuid === 'VCC' ? 'supply' : 'signal';
        const key = `${owner.id}:${face.x},${face.y}:${kind}`;
        const list = groups.get(key) ?? [];
        list.push({ flag, edge, anchor }); groups.set(key, list);
    }
    let moved = 0;
    const rotated: Placed[] = [];
    const batches = [...groups].flatMap(([id, group]) => {
        const ordered = [...group].sort((a, b) => positions.get(a.anchor)!.y - positions.get(b.anchor)!.y
            || positions.get(a.anchor)!.x - positions.get(b.anchor)!.x || a.flag.id.localeCompare(b.flag.id));
        return Array.from({ length: Math.ceil(group.length / 8) }, (_, i) => [id, ordered.slice(i * 8, (i + 1) * 8)] as const);
    });
    for (const [ownerId, group] of batches) {
        if (group.length > 48) continue;
        const ids = new Set(group.map(g => g.flag.id));
        const anchorPins = new Set(owners.get(group[0].anchor)!.ports?.map(p => p.id));
        const incident = edges.filter(e => [...e.sources, ...e.targets].some(p => anchorPins.has(p)));
        const oldEdges = incident.length <= 192 ? incident : group.map(g => g.edge);
        const leads = new Set(group.map(g => g.edge.id));
        const neighbours = oldEdges.filter(e => !leads.has(e.id));
        const fixed = nodes.filter(n => !ids.has(n.id)), retained = edges.filter(e => !oldEdges.includes(e));
        const env = new RouteEnvironment(fixed, retained, nets), pins = pinPositions(fixed);
        const crossingEnv = new RouteEnvironment(fixed, [], nets);
        const crossings = (routes: ElkExtendedEdge[]) => [...localCrossings(routes, crossingEnv).values()].reduce((a, b) => a + b, 0);
        const initialCrossings = crossings(edges);
        const cost = (ns: Placed[], es: ElkExtendedEdge[]) => {
            const complete = es.length === edges.length ? es : [...retained, ...es];
            const b = boundsOf([...fixed, ...ns, ...complete.flatMap(path).map(p => ({ ...p, width: 0, height: 0 }))]);
            return complete.reduce((sum, e) => sum + routeLength(path(e))
                + Math.max(0, path(e).length - 2) * gap.pinEscape, 0) + flagReadabilityCost([...fixed, ...ns], complete, flags)
                + (ns.length === group.length ? Math.sqrt(effectiveLayoutArea(b.width, b.height, 3)) * 2 : 0);
        };
        let best: { nodes: Placed[]; edges: ElkExtendedEdge[]; cost: number } | undefined;
        const ordered = [...group].sort((a, b) => pins.get(a.anchor)!.y - pins.get(b.anchor)!.y
            || pins.get(a.anchor)!.x - pins.get(b.anchor)!.x || a.flag.id.localeCompare(b.flag.id));
        const consider = (placed: Placed[], routes: ElkExtendedEdge[]) => {
            for (const edge of neighbours) {
                const endpointOwners = new Set([...edge.sources, ...edge.targets].map(p => owners.get(p)!.id));
                const next = clearPath(path(edge), nets.get(edge.sources[0])!, env, placed, routes, endpointOwners, false)
                    ? edge : reconnect(edge, placed, env, routes, true);
                if (!next) return;
                routes.push(next);
            }
            const complete = [...retained, ...routes];
            const normalized = group.length > 1 ? coalesceNetRoutes(complete, nets, [...fixed, ...placed], [...pinPositions([...fixed, ...placed]).values()], { maxBridgeDistance: gap.bridge }) : { edges: complete };
            const nextEdges = removeNetCycles(normalized.edges, nets).edges;
            if (crossings(nextEdges) > initialCrossings) return;
            // Moving shared geometry must not separate an existing physical tap.
            for (const bus of buses) {
                const ids = new Set(bus.map(e => e.id));
                if (connectedNetEdges(nextEdges.filter(e => ids.has(e.id)), nets).length !== 1) return;
            }
            const value = cost(placed, nextEdges);
            if (value < (best?.cost ?? cost(group.map(g => g.flag), edges)) - 1) best = { nodes: placed, edges: nextEdges, cost: value };
        };
        // Build whole horizontal rows before routing. All row bodies are visible
        // to the router, so an early lead cannot occupy a later port's slot.
        const owner = owners.get(group[0].anchor)!;
        const neighbourIds = new Set(incident.flatMap(e => [...e.sources, ...e.targets]).map(p => owners.get(p)!.id));
        const bodies = fixed.filter(n => neighbourIds.has(n.id) && !flags.has(n.id));
        const reach = Math.max(gap.branch, ...bodies.map(n => Math.max(n.x + n.width - owner.x - owner.width, owner.x - n.x))) + gap.branch;
        const fanoutDistance = Math.max(gap.branch, ...bodies.map(n => Math.max(owner.y - n.y, n.y + n.height - owner.y - owner.height)))
            + group.length * gap.wire + gap.branch;
        if (group.length > 1) for (const rows of [1]) for (const below of [false, true])
            for (const distance of [gap.branch, gap.branch * 4, gap.branch * 8, fanoutDistance])
                for (const offset of [gap.branch, Math.max(reach, group.length * gap.wire + gap.branch)]) {
            const placed: Placed[] = [];
            for (const side of [-1, 1]) {
                const items = (below ? ordered.toReversed() : ordered).filter(g => (normal(owner, g.anchor).x < 0 ? -1 : 1) === side);
                const columns = Math.ceil(items.length / rows);
                const width = Math.max(0, ...items.map(g => g.flag.width)) + gap.port;
                const height = Math.max(0, ...items.map(g => g.flag.height)) + gap.port;
                items.forEach((item, i) => {
                    const column = i % columns, row = Math.floor(i / columns);
                    const pose = below ? turnNode(item.flag, 180) : item.flag;
                    placed.push({ ...pose, x: side > 0 ? owner.x + owner.width + offset + column * width
                        : owner.x - offset - pose.width - column * width,
                        y: below ? owner.y + owner.height + distance + row * height
                            : owner.y - distance - pose.height - row * height });
                });
            }
            if (placed.some((n, i) => [...fixed, ...placed.slice(i + 1)].some(b => overlaps(n, b, gap.port))
                || retained.some(e => edgeSegments(e).some(s => segmentThroughBox(s, expand(n, gap.wire)))))) continue;
            const routes: ElkExtendedEdge[] = [];
            for (const [index, item] of ordered.entries()) {
                const next = placed.find(n => n.id === item.flag.id)!;
                const pin = pins.get(item.anchor)!, out = normal(owner, item.anchor), q = pinPositions([next]).get(next.ports![0].id)!;
                const rank = below ? ordered.length - 1 - index : index;
                const x = pin.x + out.x * (gap.pinEscape + rank * gap.wire);
                const y = q.y + (below ? -1 : 1) * (gap.pinEscape + rank * gap.wire);
                const points = simplifyRoute([pin, { x, y: pin.y }, { x, y }, { x: q.x, y }, q]);
                const fanout = out.x && rows === 1 && clearPath(points, nets.get(item.anchor)!, env, placed, routes, new Set([owner.id, next.id]))
                    ? withPath(item.edge, item.edge.sources[0] === item.anchor ? points : points.toReversed()) : null;
                const route = reconnect(item.edge, placed, env, routes, true, oldEdges) ?? fanout;
                if (!route || !acceptsFlagOrientation(item.flag, next, flags.get(item.flag.id)!, [item.edge], [route], true)) break;
                routes.push(route);
            }
            if (routes.length === group.length) consider(placed, routes);
        }
        for (const order of [ordered, ordered.toReversed()]) {
            const placed: Placed[] = [], routes: ElkExtendedEdge[] = [];
            for (const item of order) {
                const pin = pins.get(item.anchor)!, out = normal(owners.get(item.anchor)!, item.anchor);
                const id = item.flag.ports![0].id;
                const candidates: Placed[] = [item.flag];
                for (const pose of [item.flag, turnNode(item.flag, 180)]) {
                    const inward = normal(pose, id), port = pose.ports![0];
                    for (let column = 0; column < Math.min(20, Math.max(5, group.length + 1)); column++) for (let row = -2; row <= 2; row++) {
                        const depth = gap.port + column * ((out.x ? pose.width : pose.height) + gap.port);
                        const lateral = row * ((out.x ? pose.height : pose.width) + gap.port);
                        candidates.push({ ...pose,
                            x: pin.x + out.x * depth + (out.y ? lateral : 0) - inward.x * gap.pinEscape - port.x!,
                            y: pin.y + out.y * depth + (out.x ? lateral : 0) - inward.y * gap.pinEscape - port.y! });
                    }
                }
                let selected: { node: Placed; edge: ElkExtendedEdge; cost: number } | undefined;
                for (const next of candidates) {
                    if ([...fixed, ...placed].some(n => overlaps(next, n, gap.port))
                        || [...retained, ...routes].some(e => edgeSegments(e).some(s => segmentThroughBox(s, expand(next, gap.wire))))) continue;
                    const route = reconnect(item.edge, [...placed, next], env, routes);
                    if (!route || !acceptsFlagOrientation(item.flag, next, flags.get(item.flag.id)!, [item.edge], [route], true)) continue;
                    const value = cost([next], [route]);
                    if (!selected || value < selected.cost) selected = { node: next, edge: route, cost: value };
                }
                if (!selected) break;
                placed.push(selected.node); routes.push(selected.edge);
            }
            if (placed.length !== group.length) continue;
            consider(placed, routes);
        }
        if (!best) continue;
        const replacements = new Map(best.nodes.map(n => [n.id, n])), replacementsEdges = new Map(best.edges.map(e => [e.id, e]));
        nodes = nodes.map(n => replacements.get(n.id) ?? n); edges = edges.map(e => replacementsEdges.get(e.id) ?? e);
        moved += best.nodes.length;
        for (const n of best.nodes) if (n.rotation !== undefined) rotated.push(n);
    }
    return { nodes, edges, moved, rotated };
}
