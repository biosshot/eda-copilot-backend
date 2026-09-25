import type { ElkExtendedEdge } from 'elkjs';
import type { CircuitComponent } from '#types/circuit.ts';
import { shortSymbolsMap, stableShortSymbolId } from '../short-symbol.ts';
import { isPowerSignal } from '../power.ts';
import { isGroundSignal } from '../ground.ts';
import { type Placed, pinPositions, path, edgeSegments, routeLength, expand, overlaps, segmentThroughBox, EPS } from './geometry.ts';
import { straightRuns, segmentLength, connectedNetEdges } from './net-routes.ts';
import { RouteEnvironment, reconnect, localCrossings } from './router.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';
import { translations } from './groups.ts';

export const LONG_LINK_POLICY = Object.freeze({ fraction: 0.2, minimumLength: 480, directPinLength: 360,
    medianFactor: 3, maximumLinks: 16, candidatePairs: 6 });

/** Replace outlier links by explicit labels, retaining every other complete
 * terminal-to-terminal route. Shared portions therefore keep all their taps.
 * Local IC attachments are kept wired so a port cannot hide poor placement. */
export function labelLongLinks(nodes: Placed[], edges: ElkExtendedEdge[], nets: Map<string, string>, blocks: Map<string, string>, originalIds: ReadonlySet<string>, maximumLinks: number = LONG_LINK_POLICY.maximumLinks,
    patternByMember: ReadonlyMap<string, string> = new Map(),
    portStyles: ReadonlyMap<string, NonNullable<CircuitComponent['pins'][number]['port_style']>> = new Map()) {
    const added: CircuitComponent[] = [];
    const byNet = new Map<string, ElkExtendedEdge[]>();
    for (const e of edges) { const list = byNet.get(nets.get(e.sources[0])!) ?? []; list.push(e); byNet.set(nets.get(e.sources[0])!, list); }
    const total = [...byNet.values()].reduce((n, list) => n + straightRuns(list.flatMap(edgeSegments)).reduce((sum, s) => sum + segmentLength(s), 0), 0);
    const initial = [...edges].sort((a, b) => routeLength(path(b)) - routeLength(path(a)) || a.id.localeCompare(b.id));
    const lengths = initial.map(e => routeLength(path(e))).sort((a, b) => a - b);
    const typical = lengths[Math.floor(lengths.length / 2)] ?? 0;
    let links = 0;
    const ink = (es: readonly ElkExtendedEdge[]) => straightRuns(es.flatMap(edgeSegments)).reduce((sum, s) => sum + segmentLength(s), 0);
    const crossingCount = (routes: ElkExtendedEdge[], environment: RouteEnvironment) =>
        [...localCrossings(routes, environment).values()].reduce((sum, count) => sum + count, 0);
    // Forced boundary routing can contain two logical copies of the same drawn
    // segment. A cut must remove both, but only count the drawing once.
    const sameDrawing = (a: ElkExtendedEdge, b: ElkExtendedEdge) => {
        if (nets.get(a.sources[0]) !== nets.get(b.sources[0])) return false;
        const pa = path(a), pb = path(b);
        return pa.length === pb.length && (pa.every((p, i) => Math.abs(p.x - pb[i].x) < EPS && Math.abs(p.y - pb[i].y) < EPS)
            || pa.every((p, i) => Math.abs(p.x - pb[pb.length - 1 - i].x) < EPS
                && Math.abs(p.y - pb[pb.length - 1 - i].y) < EPS))
            && new Set([...a.sources, ...a.targets]).size === new Set([...b.sources, ...b.targets]).size
            && [...a.sources, ...a.targets].every(id => [...b.sources, ...b.targets].includes(id));
    };
    const internalPattern = (edge: ElkExtendedEdge) => {
        const ends = [edge.sources[0], edge.targets[0]].map(id => nodes.find(n => n.ports?.some(p => p.id === id)));
        const pattern = patternByMember.get(ends[0]?.id ?? '');
        return !!pattern && pattern === patternByMember.get(ends[1]?.id ?? '');
    };
    // A supply pull-up/down keeps its signal-side local attachment. Check
    // complete cuts in both paths; cutting the signal first can otherwise
    // leave the part visually isolated after a later supply cut.
    const keepsPassiveAttachment = (candidate: ElkExtendedEdge[]) => {
        const owner = new Map(nodes.flatMap(n => (n.ports ?? []).map(p => [p.id, n])));
        const attached = (routes: ElkExtendedEdge[], pinId: string, part: Placed) => routes.some(route => {
            const ends = [route.sources[0], route.targets[0]];
            return ends.includes(pinId) && ends.some(id => owner.get(id) !== part && originalIds.has(owner.get(id)?.id ?? ''));
        });
        return nodes.filter(n => originalIds.has(n.id) && n.ports?.length === 2).every(part => {
            const pins = part.ports!, signals = pins.map(p => nets.get(p.id) ?? '');
            const supply = signals.map(signal => isPowerSignal(signal) || isGroundSignal(signal)
                || /(?:^|_)\d+V\d+(?:_|$)/i.test(signal));
            if (supply[0] === supply[1]) return true;
            const pinId = pins[supply[0] ? 1 : 0].id;
            return !attached(edges, pinId, part) || attached(candidate, pinId, part);
        });
    };

    // A star of logical edges can draw one long shared rail. Cutting one edge
    // then saves only its final stub, so evaluate the shared rail as a unit.
    const cutSharedRail = (group: ElkExtendedEdge[], net: string) => {
        const removed = new Set(group), retained = edges.filter(e => !removed.has(e));
        const sameNet = retained.filter(e => nets.get(e.sources[0]) === net);
        const terminals = [...new Set(group.flatMap(e => [...e.sources, ...e.targets]))];
        const owner = new Map(nodes.flatMap(n => (n.ports ?? []).map(p => [p.id, n])));
        const realOwner = new Map(nodes.filter(n => originalIds.has(n.id)).flatMap(n => (n.ports ?? []).map(p => [p.id, n.id])));
        const retainedGroups = connectedNetEdges(sameNet, nets);
        // Cutting a duplicate edge cannot remove a drawing bridge.
        if (group.some(e => retainedGroups.some(g => [e.sources[0], e.targets[0]].every(id =>
            g.some(route => [...route.sources, ...route.targets].includes(id)))))) return false;
        const pendingNodes: Placed[] = [], pendingComponents: CircuitComponent[] = [], pendingEdges: ElkExtendedEdge[] = [];
        const trialNets = new Map(nets), scope = blocks.get(owner.get(terminals[0])!.id)!;
        const positions = pinPositions(nodes);
        for (const [ordinal, pinId] of terminals.entries()) {
            const local = retainedGroups.find(g => g.some(e => [...e.sources, ...e.targets].includes(pinId)));
            if (local?.some(e => [...e.sources, ...e.targets].some(p => nodes.some(n => !originalIds.has(n.id)
                && n.ports?.length === 1 && n.ports[0].id === p && blocks.get(n.id) === scope)))) continue;
            const kind = shortSymbolsMap.GND.is(net) ? shortSymbolsMap.GND : shortSymbolsMap.VCC.is(net) ? shortSymbolsMap.VCC : shortSymbolsMap.NETPORT;
            const id = stableShortSymbolId(kind.name, net, `${scope}:${group.map(e => e.id).sort().join(':')}:long`, ordinal);
            if (nodes.some(n => n.id === id)) return false;
            const created = kind.create(net, scope, id), flagId = `${id}_pin_1`;
            trialNets.set(flagId, net);
            const env = new RouteEnvironment([...nodes, ...pendingNodes], retained, trialNets);
            const origin = { ...created.node, x: 0, y: 0 } as Placed;
            const targets = [...new Set([pinId, ...(local ?? []).flatMap(e => [...e.sources, ...e.targets])
                .filter(p => realOwner.has(p))])].slice(0, 4);
            const sites = targets.map(target => {
                const lead: ElkExtendedEdge = { id: `${group[0].id}:shared:${ordinal}`, container: group[0].container,
                    sources: [target], targets: [flagId], sections: [{ id: `${group[0].id}:shared:${ordinal}:s`,
                        startPoint: positions.get(target)!, endPoint: pinPositions([origin]).get(flagId)! }] };
                return { lead, shifts: translations({ nodes: [origin] }, [lead], env.nodes, sameNet, trialNets).slice(1) };
            });
            const candidates = Array.from({ length: Math.max(0, ...sites.map(s => s.shifts.length)) }, (_, i) =>
                sites.filter(s => s.shifts[i]).map(s => ({ lead: s.lead, d: s.shifts[i] }))).flat().slice(0, 96);
            let best: { node: Placed; edge: ElkExtendedEdge; length: number } | undefined;
            for (const { lead, d } of candidates) {
                const node = { ...origin, x: d.x, y: d.y };
                if (env.nodes.some(n => overlaps(node, n, gap.port))
                    || [...retained, ...pendingEdges].some(e => edgeSegments(e).some(s => segmentThroughBox(s, expand(node, gap.wire))))) continue;
                const route = reconnect(lead, [node], env, pendingEdges);
                if (!route) continue;
                const value = routeLength(path(route)) + (path(route).length - 2) * gap.pinEscape;
                if (!best || value < best.length - EPS) best = { node, edge: route, length: value };
            }
            if (!best) return false;
            if (kind === shortSymbolsMap.NETPORT) {
                const style = portStyles.get(best.edge.sources[0]);
                if (style) created.component.pins[0].port_style = style;
            }
            pendingNodes.push(best.node); pendingComponents.push(created.component); pendingEdges.push(best.edge);
        }
        if (!pendingEdges.length || !keepsPassiveAttachment([...retained, ...pendingEdges])
            || ink([...sameNet, ...pendingEdges]) > ink([...sameNet, ...group]) - gap.branch) return false;
        const env = new RouteEnvironment(nodes, retained, trialNets);
        if (crossingCount([...pendingEdges, ...sameNet], env)
            > crossingCount([...group, ...sameNet], env)) return false;
        for (const c of pendingComponents) { nets.set(`${c.designator}_pin_1`, net); blocks.set(c.designator, c.block_name); }
        nodes = [...nodes, ...pendingNodes]; edges = [...retained, ...pendingEdges]; added.push(...pendingComponents);
        links += new Set(group.map(e => [...e.sources, ...e.targets].sort().join('|'))).size;
        return true;
    };

    for (const edge of initial) {
        if (links >= maximumLinks) break;
        if (!edges.includes(edge)) continue;
        const p = path(edge), length = routeLength(p), net = nets.get(edge.sources[0]);
        if (p.length < 2) continue;
        const distance = segmentLength({ a: p[0], b: p.at(-1)! });
        const detour = length / Math.max(gap.branch, distance);
        const owners = [edge.sources[0], edge.targets[0]].map(id => nodes.find(n => n.ports?.some(p => p.id === id)));
        const minimum = owners.every(n => n && (n.ports?.length ?? 0) > 2)
            ? LONG_LINK_POLICY.directPinLength : LONG_LINK_POLICY.minimumLength;
        if (!net || net.startsWith('unconnected:') || length < minimum
            || (length < total * LONG_LINK_POLICY.fraction && length < typical * LONG_LINK_POLICY.medianFactor && detour < 2.5)) continue;
        if (owners.some(n => !n || (!originalIds.has(n.id) && n.ports?.length !== 1))
            || owners.every(n => !originalIds.has(n!.id)) || blocks.get(owners[0]!.id) !== blocks.get(owners[1]!.id)) continue;
        // Internal pattern rails remain wired, however wide the bank becomes.
        // Only a connection across a pattern boundary may become a net port.
        const pattern = patternByMember.get(owners[0]!.id);
        if (pattern && pattern === patternByMember.get(owners[1]!.id)) continue;
        const members = nodes.filter(n => originalIds.has(n.id) && blocks.get(n.id) === blocks.get(owners[0]!.id));
        if (owners.some(n => n!.ports?.length === 2) && members.filter(n => (n.ports?.length ?? 0) > 2).length < 2) continue;
        if (distance < minimum / 2 && detour < 2.5) continue;
        const copies = edges.filter(other => sameDrawing(edge, other));
        if (copies.length > 1 && new Set(copies.map(other => [...other.sources, ...other.targets].sort().join('|'))).size === 1) {
            if (cutSharedRail(copies, net)) continue;
            // Keeping the other copy would keep the entire physical connection.
            continue;
        }
        if (owners.every(n => originalIds.has(n!.id) && (n!.ports?.length ?? 0) > 2)) {
            const partners = edges.filter(other => other !== edge && nets.get(other.sources[0]) === net
                && routeLength(path(other)) >= LONG_LINK_POLICY.minimumLength
                && [...other.sources, ...other.targets].some(id => [...edge.sources, ...edge.targets].includes(id))
                && [other.sources[0], other.targets[0]].every(id => {
                    const n = nodes.find(node => node.ports?.some(p => p.id === id));
                    return n && originalIds.has(n.id) && (n.ports?.length ?? 0) > 2 && blocks.get(n.id) === blocks.get(owners[0]!.id);
                }) && !internalPattern(other)
                && ink([edge]) + ink([other]) - ink([edge, other]) >= LONG_LINK_POLICY.minimumLength / 2)
                .sort((a, b) => routeLength(path(b)) - routeLength(path(a)) || a.id.localeCompare(b.id)).slice(0, 3);
            for (let count = Math.min(partners.length, maximumLinks - links - 1); count >= 1; count--) {
                if (cutSharedRail([edge, ...partners.slice(0, count)], net)) break;
            }
            if (!edges.includes(edge)) continue;
        }
        const retained = edges.filter(e => e !== edge), sameNet = retained.filter(e => nets.get(e.sources[0]) === net);
        // A remote shared supply flag may be split into local copies. A lone
        // flag is instead handled by placement; do not leave flag-to-flag wires.
        if (owners.some(n => !originalIds.has(n!.id) && !sameNet.some(e => [...e.sources, ...e.targets].includes(n!.ports![0].id)))) continue;
        // A lone IC's feedback/series network needs better placement, not labels.
        // Follow both sides of a series part: a USB resistor between a connector
        // and an MCU is an inter-group bridge, despite having two net names.
        const attachmentNets = new Set([net, ...owners.filter(n => n!.ports?.length === 2)
            .flatMap(n => n!.ports!.map(p => nets.get(p.id)!))].filter(n => n && !shortSymbolsMap.GND.is(n)));
        if (owners.some(n => n!.ports?.length === 2) && !shortSymbolsMap.GND.is(net) && !shortSymbolsMap.VCC.is(net)
            && members.filter(n => (n.ports?.length ?? 0) > 2 && n.ports?.some(p => attachmentNets.has(nets.get(p.id)!))).length < 2) continue;
        // Cut only this bridge, preserving the other branches of the named net.
        // A small part must retain a wired attachment to another real component;
        // otherwise a fuse/diode would become an isolated part between two flags.
        const realOwner = new Map(nodes.filter(n => originalIds.has(n.id)).flatMap(n => (n.ports ?? []).map(p => [p.id, n.id])));
        if (owners.some(n => n!.ports?.length === 2 && !retained.some(e => {
            const ids = [...e.sources, ...e.targets].map(p => realOwner.get(p));
            return ids.includes(n!.id) && ids.some(id => id && id !== n!.id);
        }))) continue;
        // Cutting a redundant logical edge would leave the same long connection
        // in place and add useless flags. Only split a bridge between terminals.
        if (connectedNetEdges(sameNet, nets).some(group => [edge.sources[0], edge.targets[0]].every(id =>
            group.some(e => [...e.sources, ...e.targets].includes(id))))) continue;
        const trialNets = new Map(nets);
        const retainedGroups = connectedNetEdges(sameNet, nets);
        type LabelSite = { node?: Placed; component?: CircuitComponent; edge?: ElkExtendedEdge; length: number };
        const sitesFor = (ordinal: number, chosen: LabelSite[], count: number): LabelSite[] => {
            const pinId = ordinal ? edge.targets[0] : edge.sources[0];
            const owner = owners[ordinal]!, block = blocks.get(owner.id)!;
            // A retained local port already names this side of the cut. Reuse
            // it instead of trying to fit a duplicate into the same pin fan-out.
            const local = retainedGroups.find(g => g.some(e => [...e.sources, ...e.targets].includes(pinId)));
            if (local?.some(e => [...e.sources, ...e.targets].some(p => nodes.some(n => !originalIds.has(n.id)
                && n.ports?.length === 1 && n.ports[0].id === p && blocks.get(n.id) === block)))) return [{ length: 0 }];
            const kind = shortSymbolsMap.GND.is(net) ? shortSymbolsMap.GND : shortSymbolsMap.VCC.is(net) ? shortSymbolsMap.VCC : shortSymbolsMap.NETPORT;
            const id = stableShortSymbolId(kind.name, net, `${block}:${edge.id}:long`, ordinal);
            if (nodes.some(n => n.id === id)) return [];
            const created = kind.create(net, block, id), flagId = `${id}_pin_1`;
            trialNets.set(flagId, net);
            const chosenNodes = chosen.flatMap(s => s.node ? [s.node] : []);
            const chosenEdges = chosen.flatMap(s => s.edge ? [s.edge] : []);
            const env = new RouteEnvironment([...nodes, ...chosenNodes], retained, trialNets);
            const origin = { ...created.node, x: 0, y: 0 } as Placed;
            const terminals = [...new Set([pinId, ...(local ?? []).flatMap(e => [...e.sources, ...e.targets])
                .filter(p => realOwner.has(p))])].slice(0, 4);
            const positions = pinPositions(nodes);
            const sites = terminals.map(terminal => {
                const lead: ElkExtendedEdge = { id: `${edge.id}:label:${ordinal}`, container: edge.container, sources: [terminal], targets: [flagId],
                    sections: [{ id: `${edge.id}:label:${ordinal}:s`, startPoint: positions.get(terminal)!, endPoint: pinPositions([origin]).get(flagId)! }] };
                return { lead, shifts: translations({ nodes: [origin] }, [lead], env.nodes, sameNet, trialNets).slice(1) };
            });
            // The local bus may have space near another terminal, even when
            // the connector pin itself is crowded. Share the same bounded search.
            const candidates = Array.from({ length: Math.max(...sites.map(s => s.shifts.length)) }, (_, i) =>
                sites.filter(s => s.shifts[i]).map(s => ({ lead: s.lead, d: s.shifts[i] }))).flat().slice(0, 96);
            const options: LabelSite[] = [];
            for (const { lead, d } of candidates) {
                const node = { ...origin, x: d.x, y: d.y };
                if (env.nodes.some(n => overlaps(node, n, gap.port))
                    || [...retained, ...chosenEdges].some(e => edgeSegments(e).some(s => segmentThroughBox(s, expand(node, gap.wire))))) continue;
                const route = reconnect(lead, [node], env, chosenEdges);
                if (!route) continue;
                const value = routeLength(path(route)) + (path(route).length - 2) * gap.pinEscape;
                const component = structuredClone(created.component);
                if (kind === shortSymbolsMap.NETPORT) {
                    const style = portStyles.get(route.sources[0]);
                    if (style) component.pins[0].port_style = style;
                }
                options.push({ node, edge: route, component, length: value });
            }
            return options.sort((a, b) => a.length - b.length || a.node!.x - b.node!.x || a.node!.y - b.node!.y).slice(0, count);
        };
        const priorInk = ink([...sameNet, edge]);
        const crossingEnv = new RouteEnvironment(nodes, retained, trialNets);
        const priorCrossings = crossingCount([edge, ...sameNet], crossingEnv);
        const validPair = (pair: LabelSite[]) => {
            const leads = pair.flatMap(s => s.edge ? [s.edge] : []);
            const symbolCost = pair.filter(s => s.component).length * gap.port;
            // Count shared physical ink once and include the visual cost of the
            // newly added labels. Pair-specific crossings may change, provided
            // the total number of physical crossings does not grow.
            return ink([...sameNet, ...leads]) + symbolCost <= priorInk - gap.branch
                && crossingCount([...leads, ...sameNet], crossingEnv) <= priorCrossings;
        };
        let selected: LabelSite[] | undefined;
        const first = sitesFor(0, [], LONG_LINK_POLICY.candidatePairs);
        for (const one of first) {
            const second = sitesFor(1, [one], LONG_LINK_POLICY.candidatePairs);
            for (const two of second) {
                const pair = [one, two];
                if (!validPair(pair)) continue;
                if (!selected || pair.reduce((n, s) => n + s.length, 0) < selected.reduce((n, s) => n + s.length, 0) - EPS)
                    selected = pair;
            }
            // Most edges already have a valid best pair. Only expand the search
            // when that pair fails the complete physical drawing check.
            if (selected && one === first[0]) break;
        }
        if (!selected) continue;
        const pendingNodes = selected.flatMap(s => s.node ? [s.node] : []);
        const pendingComponents = selected.flatMap(s => s.component ? [s.component] : []);
        const pendingEdges = selected.flatMap(s => s.edge ? [s.edge] : []);
        if (!keepsPassiveAttachment([...retained, ...pendingEdges])) continue;
        for (const c of pendingComponents) { nets.set(`${c.designator}_pin_1`, net); blocks.set(c.designator, c.block_name); }
        nodes = [...nodes, ...pendingNodes]; edges = [...retained, ...pendingEdges]; added.push(...pendingComponents); links++;
    }
    return { nodes, edges, added, links };
}
