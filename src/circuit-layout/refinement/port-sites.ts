import type { ElkExtendedEdge } from 'elkjs';
import { type Placed, type Point, pinPositions, normal, path, edgeSegments, routeLength,
    expand, overlaps, segmentThroughBox, boundsOf } from './geometry.ts';
import { RouteEnvironment, reconnect } from './router.ts';
import { translations } from './groups.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';

export type PortSite = { node: Placed; edge: ElkExtendedEdge; rerouted: ElkExtendedEdge[]; length: number };
export function replaceRoutes(edges: ElkExtendedEdge[], replacements: ElkExtendedEdge[]) {
    const byId = new Map(replacements.map(edge => [edge.id, edge]));
    return edges.map(edge => byId.get(edge.id) ?? edge);
}

/** Keep the symbol upright, and sample its contact relative to the real pin.
 * Body extents determine lateral offsets; neither contact is assumed centered. */
function nearPin(origin: Placed, lead: ElkExtendedEdge, nodes: Placed[]): Point[] {
    const owner = nodes.find(n => n.ports?.some(p => p.id === lead.sources[0]));
    if (!owner) return [];
    const pin = pinPositions([owner]).get(lead.sources[0])!;
    const contact = origin.ports![0], direction = normal(owner, lead.sources[0]);
    const lateral = direction.x ? origin.height + gap.port : origin.width / 2 + gap.port;
    const positions: Point[] = [];
    for (const distance of [gap.pinEscape, 30, 45, 60, 90, 120]) {
        for (const offset of [0, lateral, -lateral, 30, -30, lateral * 2, -lateral * 2]) {
            positions.push({ x: pin.x + direction.x * distance - direction.y * offset - contact.x!,
                y: pin.y + direction.y * distance + direction.x * offset - contact.y! });
        }
    }
    return positions;
}

/** The second pass may change a few obstructing routes, but never component
 * positions or already selected port leads. All edits remain speculative. */
export function findPortSites(origin: Placed, leads: ElkExtendedEdge[], environment: RouteEnvironment,
    sameNet: ElkExtendedEdge[], accepted: ElkExtendedEdge[], count: number, allowReroute = false): PortSite[] {
    const pins = pinPositions(environment.nodes);
    const contact = origin.ports![0];
    const candidates = leads.flatMap(lead => {
        const positions = [...nearPin(origin, lead, environment.nodes),
            ...translations({ nodes: [origin] }, [lead], environment.nodes, sameNet, environment.nets).slice(1)];
        const unique = new Map(positions.map(p => [`${p.x},${p.y}`, p]));
        return [...unique.values()].map(p => ({ lead, node: { ...origin, ...p } }));
    }).filter(({ node }) => !environment.nodes.some(n => overlaps(node, n, gap.port)))
        .sort((a, b) => {
            const distance = (c: typeof a) => {
                const p = pins.get(c.lead.sources[0])!;
                const q = { x: c.node.x + contact.x!, y: c.node.y + contact.y! };
                return Math.abs(p.x - q.x) + Math.abs(p.y - q.y);
            };
            return distance(a) - distance(b) || a.node.x - b.node.x || a.node.y - b.node.y;
        }).slice(0, 144);
    const options: PortSite[] = [];
    const bounds = boundsOf(environment.nodes);
    let repairs = 0;
    const routeCost = (edge: ElkExtendedEdge) => routeLength(path(edge)) + Math.max(0, path(edge).length - 2) * gap.pinEscape;
    for (const { lead, node } of candidates) {
        const obstructs = (edge: ElkExtendedEdge) => edgeSegments(edge).some(s => segmentThroughBox(s, expand(node, gap.wire)));
        if (accepted.some(obstructs)) continue;
        const blockers = environment.edges.filter(obstructs);
        if (blockers.length && (!allowReroute || blockers.length > 4)) continue;
        if (allowReroute && repairs++ >= 16) break;
        const removed = new Set(blockers);
        const env = blockers.length ? new RouteEnvironment(environment.nodes,
            environment.edges.filter(e => !removed.has(e)), environment.nets) : environment;
        const route = reconnect(lead, [node], env, accepted, allowReroute);
        if (!route) continue;
        const rerouted: ElkExtendedEdge[] = [];
        const repairEnv = blockers.length ? new RouteEnvironment([...env.nodes, node], env.edges, env.nets) : env;
        for (const edge of blockers) {
            const repaired = reconnect(edge, [], repairEnv, [...accepted, route, ...rerouted], true);
            if (!repaired) break;
            rerouted.push(repaired);
        }
        if (rerouted.length !== blockers.length) continue;
        const growth = Math.max(0, bounds.x - node.x) + Math.max(0, bounds.y - node.y)
            + Math.max(0, node.x + node.width - bounds.x - bounds.width)
            + Math.max(0, node.y + node.height - bounds.y - bounds.height);
        const length = routeCost(route) + Math.max(0, rerouted.reduce((n, e) => n + routeCost(e), 0)
            - blockers.reduce((n, e) => n + routeCost(e), 0)) + growth;
        options.push({ node, edge: route, rerouted, length });
    }
    return options.sort((a, b) => a.length - b.length || a.node.x - b.node.x || a.node.y - b.node.y).slice(0, count);
}
