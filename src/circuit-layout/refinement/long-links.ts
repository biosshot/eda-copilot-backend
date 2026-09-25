import type { ElkExtendedEdge } from 'elkjs';
import type { CircuitComponent } from '#types/circuit.ts';
import { shortSymbolsMap, stableShortSymbolId } from '../short-symbol.ts';
import { isPowerSignal } from '../power.ts';
import { isGroundSignal } from '../ground.ts';
import { type Placed, pinPositions, path, edgeSegments, routeLength, EPS } from './geometry.ts';
import { straightRuns, segmentLength, connectedNetEdges } from './net-routes.ts';
import { RouteEnvironment, localCrossings } from './router.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';
import { findPortSites, replaceRoutes } from './port-sites.ts';

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
    const cutSharedRail = (group: ElkExtendedEdge[], net: string, repair = false): boolean => {
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
        const pendingRepairs: ElkExtendedEdge[] = [];
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
            const updated = replaceRoutes(retained, pendingRepairs);
            const env = new RouteEnvironment([...nodes, ...pendingNodes], updated, trialNets);
            const origin = { ...created.node, x: 0, y: 0 } as Placed;
            const targets = [...new Set([pinId, ...(local ?? []).flatMap(e => [...e.sources, ...e.targets])
                .filter(p => realOwner.has(p))])].slice(0, 4);
            const leads = targets.map(target => {
                const lead: ElkExtendedEdge = { id: `${group[0].id}:shared:${ordinal}`, container: group[0].container,
                    sources: [target], targets: [flagId], sections: [{ id: `${group[0].id}:shared:${ordinal}:s`,
                        startPoint: positions.get(target)!, endPoint: pinPositions([origin]).get(flagId)! }] };
                return lead;
            });
            const best = findPortSites(origin, leads, env, updated.filter(e => nets.get(e.sources[0]) === net), pendingEdges, 1, repair)[0];
            if (!best) return !repair && cutSharedRail(group, net, true);
            if (kind === shortSymbolsMap.NETPORT) {
                const style = portStyles.get(best.edge.sources[0]);
                if (style) created.component.pins[0].port_style = style;
            }
            pendingNodes.push(best.node); pendingComponents.push(created.component); pendingEdges.push(best.edge);
            pendingRepairs.push(...best.rerouted);
        }
        const updated = replaceRoutes(retained, pendingRepairs);
        const affectedNets = new Set([net, ...pendingRepairs.map(e => nets.get(e.sources[0]))]);
        const before = edges.filter(e => affectedNets.has(nets.get(e.sources[0])));
        const after = [...updated.filter(e => affectedNets.has(nets.get(e.sources[0]))), ...pendingEdges];
        if (!pendingEdges.length || !keepsPassiveAttachment([...updated, ...pendingEdges])
            || ink(after) + pendingNodes.length * gap.port > ink(before) - gap.branch) return false;
        const env = new RouteEnvironment(nodes, edges.filter(e => !affectedNets.has(nets.get(e.sources[0]))), trialNets);
        if (crossingCount(after, env) > crossingCount(before, env)) return !repair && cutSharedRail(group, net, true);
        for (const c of pendingComponents) { nets.set(`${c.designator}_pin_1`, net); blocks.set(c.designator, c.block_name); }
        nodes = [...nodes, ...pendingNodes]; edges = [...updated, ...pendingEdges]; added.push(...pendingComponents);
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
        // Named output rails may bridge a local load and a feedback divider
        // even with only one IC. Keep switching and feedback nets protected.
        const supplyRail = isPowerSignal(net) || /(?:^|_)\d+V\d+$/i.test(net);
        // Require a signal-side attachment; supply-to-supply protection parts
        // and supply-to-ground decoupling must keep their physical chain.
        const supplyAttachment = supplyRail && owners.some(n => n!.ports?.length === 2 && n!.ports!.some(p => {
            const other = nets.get(p.id);
            return other && other !== net && !/^nc$/i.test(other) && !other.startsWith('unconnected:')
                && !isGroundSignal(other) && !isPowerSignal(other) && !/(?:^|_)\d+V\d+$/i.test(other);
        }));
        if (!supplyAttachment && owners.some(n => n!.ports?.length === 2) && members.filter(n => (n.ports?.length ?? 0) > 2).length < 2) continue;
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
        if (owners.some(n => n!.ports?.length === 2) && !shortSymbolsMap.GND.is(net) && !supplyAttachment
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
        type LabelSite = { node?: Placed; component?: CircuitComponent; edge?: ElkExtendedEdge; rerouted?: ElkExtendedEdge[]; length: number };
        const sitesFor = (ordinal: number, chosen: LabelSite[], count: number, repair: boolean): LabelSite[] => {
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
            const updated = replaceRoutes(retained, chosen.flatMap(s => s.rerouted ?? []));
            const env = new RouteEnvironment([...nodes, ...chosenNodes], updated, trialNets);
            const origin = { ...created.node, x: 0, y: 0 } as Placed;
            const terminals = [...new Set([pinId, ...(local ?? []).flatMap(e => [...e.sources, ...e.targets])
                .filter(p => realOwner.has(p))])].slice(0, 4);
            const positions = pinPositions(nodes);
            const leads = terminals.map(terminal => {
                const lead: ElkExtendedEdge = { id: `${edge.id}:label:${ordinal}`, container: edge.container, sources: [terminal], targets: [flagId],
                    sections: [{ id: `${edge.id}:label:${ordinal}:s`, startPoint: positions.get(terminal)!, endPoint: pinPositions([origin]).get(flagId)! }] };
                return lead;
            });
            return findPortSites(origin, leads, env, updated.filter(e => nets.get(e.sources[0]) === net), chosenEdges, count, repair).map(site => {
                const component = structuredClone(created.component);
                if (kind === shortSymbolsMap.NETPORT) {
                    const style = portStyles.get(site.edge.sources[0]);
                    if (style) component.pins[0].port_style = style;
                }
                return { ...site, component };
            });
        };
        const validPair = (pair: LabelSite[]) => {
            const leads = pair.flatMap(s => s.edge ? [s.edge] : []);
            const repairs = pair.flatMap(s => s.rerouted ?? []);
            const affectedNets = new Set([net, ...repairs.map(e => nets.get(e.sources[0]))]);
            const before = edges.filter(e => affectedNets.has(nets.get(e.sources[0])));
            const after = [...replaceRoutes(retained, repairs).filter(e => affectedNets.has(nets.get(e.sources[0]))), ...leads];
            const crossingEnv = new RouteEnvironment(nodes, edges.filter(e => !affectedNets.has(nets.get(e.sources[0]))), trialNets);
            const symbolCost = pair.filter(s => s.component).length * gap.port;
            // Count shared physical ink once and include the visual cost of the
            // newly added labels. Pair-specific crossings may change, provided
            // the total number of physical crossings does not grow.
            return ink(after) + symbolCost <= ink(before) - gap.branch
                && crossingCount(after, crossingEnv) <= crossingCount(before, crossingEnv);
        };
        let selected: LabelSite[] | undefined;
        for (const repair of [false, true]) {
            const first = sitesFor(0, [], LONG_LINK_POLICY.candidatePairs, repair);
            for (const one of first) {
                const second = sitesFor(1, [one], LONG_LINK_POLICY.candidatePairs, repair);
                for (const two of second) {
                    const pair = [one, two];
                    if (!validPair(pair)) continue;
                    if (!selected || pair.reduce((n, s) => n + s.length, 0) < selected.reduce((n, s) => n + s.length, 0) - EPS)
                        selected = pair;
                }
                if (selected && one === first[0]) break;
            }
            if (selected) break;
        }
        if (!selected) continue;
        const pendingNodes = selected.flatMap(s => s.node ? [s.node] : []);
        const pendingComponents = selected.flatMap(s => s.component ? [s.component] : []);
        const pendingEdges = selected.flatMap(s => s.edge ? [s.edge] : []);
        const updated = replaceRoutes(retained, selected.flatMap(s => s.rerouted ?? []));
        if (!keepsPassiveAttachment([...updated, ...pendingEdges])) continue;
        for (const c of pendingComponents) { nets.set(`${c.designator}_pin_1`, net); blocks.set(c.designator, c.block_name); }
        nodes = [...nodes, ...pendingNodes]; edges = [...updated, ...pendingEdges]; added.push(...pendingComponents); links++;
    }
    return { nodes, edges, added, links };
}
