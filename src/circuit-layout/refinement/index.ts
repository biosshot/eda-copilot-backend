import type { ElkNode, ElkExtendedEdge } from 'elkjs';
import type { CircuitComponent } from '#types/circuit.ts';
import type { SymbolWithMeta } from '#types/symbol.ts';
import type { MacroInstance } from '../patterns/types.ts';
import { localGroups, orientations, translations } from './groups.ts';
import { RouteEnvironment, reconnect, localCrossings, clearPath } from './router.ts';
import { type Placed, type Point, EPS, pinPositions, path, normal, orthogonal, exitsAlong, overlaps, expand,
    edgeSegments, shift, withPath, boundsOf, routeLength } from './geometry.ts';
import { coalesceNetRoutes, straightRuns, segmentLength, measureRouteShape, connectedNetEdges } from './net-routes.ts';
import { acceptsFlagOrientation, flagReadabilityCost } from './flag-policy.ts';
import { SCHEMATIC_CLEARANCE as gap, REFINEMENT_LIMITS as limit, componentClearance } from './policy.ts';
import { resolveSceneBlocks } from './scope.ts';
import { mergeLocalFlags } from './flags.ts';
import { centerBankFlags, lowerGroundFlags, alignLeafFlags } from './flag-position.ts';
import { labelLongLinks, LONG_LINK_POLICY } from './long-links.ts';
import { compactEmptyBands } from './compact.ts';
import { packDrawingIslands } from './pack-islands.ts';
import { hasConnection } from '../signals.ts';
import { placeNearbyFlags } from './nearby-flags.ts';
import { packSchematicRectangles, SCHEMATIC_SHEET, PAGE_SOFT_GRID } from '#utils/schematic-packing.ts';
import { effectiveLayoutArea, evaluateLayoutQuality, safelyImprovesLayout } from '../quality.ts';
import { removeNetCycles } from './net-cycles.ts';
import { rerouteFixedDetours } from './detour-route.ts';
import { softlyAlignMajorComponents } from './soft-align.ts';
import { type ConnectorRole, CONNECTOR_OVERRIDE_RATIO, connectorLeadLength, connectorOrientationSeverity,
    connectorOverrideWorthwhile, inferConnectorRoles } from './connector-policy.ts';

export function sceneNets(components: readonly CircuitComponent[]) {
    return new Map<string, string>(components.flatMap(c => c.pins.map(p => [`${c.designator}_pin_${p.pin_number}`,
        hasConnection(p.signal_name) ? p.signal_name : `unconnected:${c.designator}:${p.pin_number}`] as const)));
}

function physicalLength(edges: ElkExtendedEdge[], nets: ReadonlyMap<string, string>) {
    const byNet = new Map<string, ElkExtendedEdge[]>();
    for (const e of edges) { const key = nets.get(e.sources[0])!; const list = byNet.get(key) ?? []; list.push(e); byNet.set(key, list); }
    return [...byNet.values()].reduce((sum, list) => sum + straightRuns(list.flatMap(edgeSegments)).reduce((n, s) => n + segmentLength(s), 0), 0);
}
function cost(nodes: Placed[], edges: ElkExtendedEdge[], nets: ReadonlyMap<string, string>, originals: ReadonlySet<string>) {
    const bounds = boundsOf(nodes);
    let result = physicalLength(edges, nets) + Math.sqrt(effectiveLayoutArea(bounds.width, bounds.height, 3));
    for (const e of edges) {
        const p = path(e);
        result += Math.max(0, p.length - 2) * gap.pinEscape;
        const owners = [e.sources[0], e.targets[0]].map(id => nodes.find(n => n.ports?.some(port => port.id === id)));
        // Give the lead between real parts priority over the small bend needed
        // to connect an upright supply flag to a horizontal series component.
        if (owners.every(n => n && originals.has(n.id))) {
            const anchored = owners.some(n => /^U/i.test(n!.id) || (n!.ports?.length ?? 0) > 4);
            result += Math.max(0, p.length - 2) * gap.branch + routeLength(p) * (anchored ? 3 : 1);
        }
        for (const [id, points] of [[e.sources[0], p], [e.targets[0], p.toReversed()]] as const) {
            const owner = nodes.find(n => n.ports?.some(port => port.id === id));
            if (owner && !exitsAlong([...points], normal(owner, id), gap.pinEscape)) result += gap.branch * 4;
        }
    }
    return result;
}

/** Bounded local placement/routing. Complete electrical and export checks live
 * in the bank runner, never in this production candidate loop. */
export function refineSchematicScene(input: ElkNode, components: readonly CircuitComponent[], added: readonly CircuitComponent[],
    symbols: readonly SymbolWithMeta[], macros: readonly MacroInstance[] = [],
    connectorRoles: ReadonlyMap<string, ConnectorRole> = new Map(), orderSeeds: readonly number[] = [1, 2, 4]): ReturnType<typeof refineLocalScene> {
    const scopes = resolveSceneBlocks(components, added, input.edges ?? []);
    const names = [...new Set((input.children ?? []).map(n => scopes.get(n.id)))];
    const owners = new Map((input.children ?? []).flatMap(n => (n.ports ?? []).map(p => [p.id, n.id])));
    if (names.length < 2 || names.includes(undefined) || (input.edges ?? []).some(e =>
        new Set([...e.sources, ...e.targets].map(p => scopes.get(owners.get(p)!))).size !== 1)) {
        return refineBestLocalScene(input, components, added, symbols, macros, connectorRoles, orderSeeds);
    }
    const started = performance.now();
    const results = names.sort().map(name => {
        const children = (input.children ?? []).filter(n => scopes.get(n.id) === name) as Placed[];
        const ids = new Set(children.map(n => n.id));
        const edges = (input.edges ?? []).filter(e => ids.has(owners.get(e.sources[0])!));
        const points = [...children, ...edges.flatMap(path)];
        const d = { x: gap.component - Math.min(...points.map(p => p.x)), y: gap.component - Math.min(...points.map(p => p.y)) };
        const local = { ...input, children: children.map(n => ({ ...n, ...shift(n, d) })),
            edges: edges.map(e => withPath(e, path(e).map(p => shift(p, d)))) };
        return refineBestLocalScene(local, components.filter(c => ids.has(c.designator)), added.filter(c => ids.has(c.designator)),
            symbols.filter(s => ids.has(s.designator)), macros.filter(m => m.absorbedDesignators.some(id => ids.has(id))), connectorRoles, orderSeeds);
    });
    const first = results[0], stats = { ...first.stats };
    for (const key of ['groupsMoved', 'componentsRotated', 'candidates', 'netsCoalesced', 'detoursRerouted', 'flagsRemoved', 'flagsCentered',
        'flagsLowered', 'flagsAligned', 'chipsAligned', 'longLinksLabeled', 'relayouts', 'islandsPacked', 'emptySpaceRemoved'] as const) {
        stats[key] = results.reduce((sum, r) => sum + r.stats[key], 0);
    }
    const removedSymbolIds = new Set(results.flatMap(r => [...r.removedSymbolIds]));
    const addedSymbols = results.flatMap(r => r.addedSymbols);
    const boxes = results.map((r, i) => ({ id: String(i), ...boundsOf([...(r.scene.children ?? []) as Placed[],
        ...(r.scene.edges ?? []).flatMap(path).map(p => ({ ...p, width: 0, height: 0 }))]) }));
    const page = packSchematicRectangles(boxes, SCHEMATIC_SHEET.blockPadding * 2 + SCHEMATIC_SHEET.extraBlockGap,
        [], undefined, PAGE_SOFT_GRID);
    const packed = { nodes: [] as Placed[], edges: [] as ElkExtendedEdge[] };
    results.forEach((r, i) => {
        const at = page.positions.get(String(i))!, box = boxes[i];
        const d = { x: at.x - box.x + gap.component, y: at.y - box.y + gap.component };
        packed.nodes.push(...(r.scene.children ?? []).map(n => ({ ...n, ...shift(n as Placed, d) }) as Placed));
        packed.edges.push(...(r.scene.edges ?? []).map(e => withPath(e, path(e).map(p => shift(p, d)))));
    });
    const bounds = boundsOf([...packed.nodes, ...packed.edges.flatMap(path).map(p => ({ ...p, width: 0, height: 0 }))]);
    stats.elapsedMs = performance.now() - started;
    stats.sceneTranslation = { x: 0, y: 0 };
    stats.localizedNets = results.flatMap(r => r.stats.localizedNets);
    stats.skipped = results.map(r => r.stats.skipped).filter(Boolean).join('; ');
    return { scene: { ...input, children: packed.nodes, edges: packed.edges,
        width: bounds.x + bounds.width + gap.component, height: bounds.y + bounds.height + gap.component },
        rotations: new Map(results.flatMap(r => [...r.rotations])), addedSymbols, removedSymbolIds, stats };
}

function refineBestLocalScene(input: ElkNode, components: readonly CircuitComponent[], added: readonly CircuitComponent[],
    symbols: readonly SymbolWithMeta[], macros: readonly MacroInstance[], connectorRoles: ReadonlyMap<string, ConnectorRole>,
    orderSeeds: readonly number[]) {
    const started = performance.now();
    const seeds = [...new Set(orderSeeds.length ? orderSeeds : [1])];
    let best = refineLocalScene(input, components, added, symbols, macros, connectorRoles, seeds[0]);
    const baselineQuality = evaluateLayoutQuality(best.scene);
    let bestScore = baselineQuality.score;
    let candidates = best.stats.candidates;
    for (const seed of seeds.slice(1)) {
        const candidate = refineLocalScene(input, components, added, symbols, macros, connectorRoles, seed);
        candidates += candidate.stats.candidates;
        const quality = evaluateLayoutQuality(candidate.scene);
        if (safelyImprovesLayout(quality, baselineQuality) && quality.score < bestScore) {
            best = candidate;
            bestScore = quality.score;
        }
    }
    best.stats.candidates = candidates;
    best.stats.elapsedMs = performance.now() - started;
    return best;
}

function seededGroupOrder<T>(groups: T[], seed: number): T[] {
    if (seed === 1) return groups;
    const shuffled = [...groups];
    let state = seed >>> 0;
    for (let i = shuffled.length - 1; i > 0; i--) {
        state ^= state << 13; state ^= state >>> 17; state ^= state << 5;
        const j = (state >>> 0) % (i + 1);
        [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
    }
    return shuffled;
}

function refineLocalScene(input: ElkNode, components: readonly CircuitComponent[], added: readonly CircuitComponent[],
    symbols: readonly SymbolWithMeta[], macros: readonly MacroInstance[] = [],
    firstPassConnectorRoles: ReadonlyMap<string, ConnectorRole> = new Map(), orderSeed = 1) {
    const started = performance.now(), scene = structuredClone(input);
    let nodes = (scene.children ?? []) as Placed[], edges = scene.edges ?? [];
    const nets = sceneNets([...components, ...added]), blocks = resolveSceneBlocks(components, added, edges);
    const originalIds = new Set(components.map(c => c.designator));
    const connectorRoles = new Map([...inferConnectorRoles(nodes, edges, components), ...firstPassConnectorRoles]);
    const patternByMember = new Map(macros.flatMap(m => m.absorbedDesignators.map(id => [id, m.id] as const)));
    const rotations = new Map<string, { rotate: number; center: Point }>();
    const stats = { groupsMoved: 0, componentsRotated: 0, candidates: 0, netsCoalesced: 0, detoursRerouted: 0, flagsRemoved: 0, flagsCentered: 0, flagsLowered: 0, flagsAligned: 0, chipsAligned: 0, longLinksLabeled: 0,
        relayouts: 0, localizedNets: [] as string[], islandsPacked: 0, emptySpaceRemoved: 0, sceneTranslation: { x: 0, y: 0 }, elapsedMs: 0, skipped: '' };
    if (edges.some(e => !orthogonal(path(e)) || e.sources.length !== 1 || e.targets.length !== 1)) {
        stats.skipped = 'Unsupported compound or non-orthogonal routes'; stats.elapsedMs = performance.now() - started;
        return { scene, rotations, addedSymbols: [] as CircuitComponent[], removedSymbolIds: new Set<string>(), stats };
    }
    const newSymbols: CircuitComponent[] = [];
    for (let pass = 0; pass < limit.passes; pass++) {
        const portStyles = new Map(components.flatMap(component => component.pins
            .filter(pin => pin.port_style)
            .map(pin => [`${component.designator}_pin_${pin.pin_number}`, pin.port_style!] as const)));
        const labeled = labelLongLinks(nodes, edges, nets, blocks, originalIds,
            Math.max(0, LONG_LINK_POLICY.maximumLinks - stats.longLinksLabeled), patternByMember, portStyles);
        nodes = labeled.nodes; edges = labeled.edges; added = [...added, ...labeled.added]; stats.longLinksLabeled += labeled.links;
        newSymbols.push(...labeled.added);
        const flagKinds = new Map(added.map(c => [c.designator, c]));
        const groups = seededGroupOrder(localGroups(nodes, edges, components, added, macros), orderSeed === 1 ? 1 : orderSeed + pass);
        for (const group of groups) {
            if (!group.ids.length || group.ids.length > limit.members) continue;
            const ids = new Set(group.ids), moving = nodes.filter(n => ids.has(n.id)), fixed = nodes.filter(n => !ids.has(n.id));
            const pins = new Set(moving.flatMap(n => (n.ports ?? []).map(p => p.id)));
            const incident = edges.filter(e => [...e.sources, ...e.targets].some(p => pins.has(p)));
            const boundary = incident.filter(e => [...e.sources, ...e.targets].some(p => !pins.has(p)));
            if (!incident.length || boundary.length > limit.boundaryEdges || incident.some(e => !orthogonal(path(e)) || e.sources.length !== 1 || e.targets.length !== 1)) continue;
            const owner = new Map(nodes.flatMap(n => (n.ports ?? []).map(p => [p.id, n.id])));
            const scopes = new Set(incident.flatMap(e => [...e.sources, ...e.targets]).map(p => blocks.get(owner.get(p)!)));
            if (scopes.size !== 1 || scopes.has(undefined)) continue;
            const incidentIds = new Set(incident.map(e => e.id)), env = new RouteEnvironment(fixed, edges.filter(e => !incidentIds.has(e.id)), nets);
            const affectedNets = new Set(incident.map(e => nets.get(e.sources[0])));
            const rails = edges.filter(e => affectedNets.has(nets.get(e.sources[0])) && !incidentIds.has(e.id));
            if ([...incident, ...rails].reduce((n, e) => n + edgeSegments(e).length, 0) > limit.netSegments) continue;
            // Include retained portions of these nets: moving one duplicate route
            // must not keep its old crossing AND introduce a second crossing nearby.
            const priorCrossings = localCrossings([...incident, ...rails], env);
            const oldJogs = new Map([...affectedNets].map(net => [net,
                measureRouteShape([...incident, ...rails].filter(e => nets.get(e.sources[0]) === net)).shortJogs]));
            const crowding = (moving: Placed[]) => moving.reduce((sum, n) => sum + fixed.filter(b => originalIds.has(n.id) && originalIds.has(b.id)
                && componentClearance(n, b) > gap.component && overlaps(n, b, componentClearance(n, b))).length * gap.largeIC * 8, 0);
            const privatePins = new Set(connectedNetEdges([...incident, ...rails], nets).filter(g => g.length === 1)
                .flatMap(g => [...g[0].sources, ...g[0].targets]));
            for (const n of moving.filter(n => flagKinds.has(n.id) && n.ports?.length === 1)) {
                const id = n.ports![0].id;
                if (incident.filter(e => [...e.sources, ...e.targets].includes(id)).length === 1) privatePins.add(id);
            }
            const poses = orientations(group, nodes, symbols).map(pose => ({ pose, shifts: translations(pose, boundary, fixed, rails, nets, env.edges) }));
            const connector = components.find(c => ids.has(c.designator) && connectorRoles.has(c.designator));
            const role = connector && connectorRoles.get(connector.designator);
            const preferredSeverity = connector && role ? Math.min(...poses.map(({ pose }) =>
                connectorOrientationSeverity(pose.nodes.find(n => n.id === connector.designator)!, connector, role))) : 0;
            const preference = (items: Placed[], routes: ElkExtendedEdge[]) => {
                const node = connector && items.find(n => n.id === connector.designator);
                if (!node || !role) return { preferred: true, length: 0, penalty: 0 };
                const length = connectorLeadLength(node, routes);
                const preferred = connectorOrientationSeverity(node, connector, role) <= preferredSeverity + EPS;
                return { preferred, length, penalty: preferred ? 0 : length * (1 / CONNECTOR_OVERRIDE_RATIO - 1) };
            };
            const initialPreference = preference(moving, [...incident, ...rails]);
            const initialCost = cost(nodes, [...incident, ...rails], nets, originalIds) + crowding(moving)
                + flagReadabilityCost(nodes, [...incident, ...rails], flagKinds) + initialPreference.penalty;
            const initialLength = physicalLength([...incident, ...rails], nets);
            let best: { nodes: Placed[]; edges: ElkExtendedEdge[]; value: number; preferred: boolean; leadLength: number } | undefined;
            let preferredBest: typeof best = initialPreference.preferred
                ? { nodes: moving, edges: incident, value: initialCost, preferred: true, leadLength: initialPreference.length } : undefined;
            let attempted = 0, screened = 0;
            // Round-robin orientations: a busy first pose must not exhaust the
            // candidate budget before the useful 90-degree pose gets a turn.
            candidateSearch: for (let index = 0; index < Math.max(...poses.map(p => p.shifts.length)); index++) {
                for (const { pose, shifts } of poses) {
                    const d = shifts[index]; if (!d) continue;
                    if (++screened > limit.candidates * 40 || attempted >= limit.candidates) break candidateSearch;
                    stats.candidates++;
                    const candidate = pose.nodes.map(n => ({ ...n, x: n.x + d.x, y: n.y + d.y }));
                    if (candidate.some(n => env.bodies.query(expand(n, gap.largeIC)).some(b => overlaps(n, b, originalIds.has(n.id) && originalIds.has(b.id) ? componentClearance(n, b) : gap.port)))) continue;
                    // Rigid groups retain the spacing defined by their pattern.
                    // Rechecking it against a different padding froze whole groups
                    // (notably the existing pi filter, whose parts share a rail).
                    if (group.flexible && candidate.some((n, i) => candidate.slice(i + 1).some(b => overlaps(n, b, gap.port)))) continue;
                    // Bodies cannot move across an unchanged wire, even on their own net.
                    if (candidate.some(n => env.wires.query(expand(n, gap.wire)).some(s => {
                        const box = expand(n, gap.wire);
                        return Math.max(s.a.x, s.b.x) > box.x + EPS && Math.min(s.a.x, s.b.x) < box.x + box.width - EPS
                            && Math.max(s.a.y, s.b.y) > box.y + EPS && Math.min(s.a.y, s.b.y) < box.y + box.height - EPS;
                    }))) continue;
                    attempted++;
                    const routed: ElkExtendedEdge[] = [];
                    let valid = true;
                    for (const edge of incident) {
                        const internal = edge.sources.every(p => pins.has(p)) && edge.targets.every(p => pins.has(p));
                        let next: ElkExtendedEdge | null;
                        if (internal && !group.flexible) {
                            next = withPath(edge, path(edge).map(p => shift(pose.transform?.(p) ?? p, d)));
                            const owners = new Set([...edge.sources, ...edge.targets].map(p => owner.get(p)!));
                            if (!clearPath(path(next), nets.get(edge.sources[0])!, env, candidate, routed, owners, false)) next = null;
                        } else {
                            next = reconnect(edge, candidate, env, routed);
                            if (!next && !group.loneFlag && routeLength(path(edge)) >= LONG_LINK_POLICY.minimumLength) {
                                const positions = pinPositions([...fixed, ...candidate]);
                                const a = positions.get(edge.sources[0])!, b = positions.get(edge.targets[0])!;
                                if (Math.abs(a.x - b.x) + Math.abs(a.y - b.y) < routeLength(path(edge)) * 0.6)
                                    next = reconnect(edge, candidate, env, routed, true);
                            }
                        }
                        if (!next) { valid = false; break; }
                        // Preserve an established straight IC attachment. An old
                        // passive-to-passive axis must not lock an inductor far from
                        // its IC: that straightness is a cost preference, not a veto.
                        if (!internal && path(edge).length === 2 && [...edge.sources, ...edge.targets].every(p => originalIds.has(owner.get(p)!))) {
                            const oldOwners = [...edge.sources, ...edge.targets].map(p => nodes.find(n => n.id === owner.get(p))!);
                            if (oldOwners.some(n => /^U/i.test(n.id) || (n.ports?.length ?? 0) > 4)
                                && exitsAlong(path(edge), normal(oldOwners[0], edge.sources[0]), gap.pinEscape)
                                && exitsAlong(path(edge).toReversed(), normal(oldOwners[1], edge.targets[0]), gap.pinEscape)
                                && path(next).length !== 2) { valid = false; break; }
                        }
                        routed.push(next);
                    }
                    if (!valid) continue;
                    if (candidate.some(n => {
                        const flag = flagKinds.get(n.id), before = moving.find(b => b.id === n.id);
                        return flag && before && !acceptsFlagOrientation(before, n, flag, incident, routed,
                            privatePins.has(n.ports![0].id), group.ids.length > 1);
                    })) continue;
                    if (pose.requiresShorter && physicalLength([...routed, ...rails], nets) >= initialLength - 1) continue;
                    const crossings = localCrossings([...routed, ...rails], env);
                    // Moving a group may replace a GND/output crossing with one
                    // GND/input crossing. Compare total physical crossings, not
                    // their old pair names; the count must still not grow.
                    if ([...crossings.values()].reduce((n, count) => n + count, 0)
                        > [...priorCrossings.values()].reduce((n, count) => n + count, 0)) continue;
                    const readable = preference(candidate, [...routed, ...rails]);
                    const value = cost([...fixed, ...candidate], [...routed, ...rails], nets, originalIds)
                        + flagReadabilityCost([...fixed, ...candidate], [...routed, ...rails], flagKinds) + readable.penalty;
                    if ([...affectedNets].some(net => measureRouteShape([...routed, ...rails].filter(e => nets.get(e.sources[0]) === net)).shortJogs > oldJogs.get(net)!)) continue;
                    if (value < (best?.value ?? initialCost) - 1) {
                        best = { nodes: candidate, edges: routed, value, preferred: readable.preferred, leadLength: readable.length };
                    }
                    if (readable.preferred && value < (preferredBest?.value ?? initialCost) - 1)
                        preferredBest = { nodes: candidate, edges: routed, value, preferred: true, leadLength: readable.length };
                }
            }
            if (best && !best.preferred && preferredBest && !connectorOverrideWorthwhile(best.leadLength, preferredBest.leadLength))
                best = preferredBest.value >= initialCost - 1 ? undefined : preferredBest;
            if (!best) continue;
            const replacement = new Map(best.nodes.map(n => [n.id, n])), routes = new Map(best.edges.map(e => [e.id, e]));
            nodes = nodes.map(n => replacement.get(n.id) ?? n); edges = edges.map(e => routes.get(e.id) ?? e);
            stats.groupsMoved++;
            for (const n of best.nodes) if (n.rotation !== undefined && n.center) rotations.set(n.id, { rotate: n.rotation, center: n.center });
        }
    }
    const centered = centerBankFlags(nodes, edges, added, macros, nets);
    nodes = centered.nodes; edges = centered.edges; stats.flagsCentered = centered.centered;
    const coalesced = coalesceNetRoutes(edges, nets, nodes, [...pinPositions(nodes).values()], { maxBridgeDistance: gap.bridge });
    edges = coalesced.edges; stats.netsCoalesced = coalesced.groupsChanged;
    const merged = mergeLocalFlags(nodes, edges, added, nets, blocks);
    nodes = merged.nodes; edges = merged.edges; stats.flagsRemoved = merged.removed.size;
    const lowered = lowerGroundFlags(nodes, edges, added, nets);
    nodes = lowered.nodes; edges = lowered.edges; stats.flagsLowered = lowered.lowered;
    const compacted = compactEmptyBands(nodes, edges);
    nodes = compacted.nodes; edges = compacted.edges; stats.emptySpaceRemoved = compacted.removed;
    const packed = packDrawingIslands(nodes, edges, nets, blocks, originalIds);
    nodes = packed.nodes; edges = packed.edges; stats.islandsPacked = packed.moved;
    const aligned = alignLeafFlags(nodes, edges, added, nets);
    nodes = aligned.nodes; edges = aligned.edges; stats.flagsAligned += aligned.aligned;
    for (const n of aligned.rotated) rotations.set(n.id, { rotate: n.rotation!, center: n.center! });
    // Packing and final flag alignment can create new adjacent stems. Finish
    // with the same net-aware cleanup so those moves do not leave twin rails.
    const finalRoutes = coalesceNetRoutes(edges, nets, nodes, [...pinPositions(nodes).values()], { maxBridgeDistance: gap.bridge });
    edges = finalRoutes.edges; stats.netsCoalesced += finalRoutes.groupsChanged;
    const finalFlags = mergeLocalFlags(nodes, edges, added, nets, blocks);
    nodes = finalFlags.nodes; edges = finalFlags.edges;
    for (const id of finalFlags.removed) merged.removed.add(id);
    stats.flagsRemoved = merged.removed.size;
    const nearby = placeNearbyFlags(nodes, edges, added, nets);
    nodes = nearby.nodes; edges = nearby.edges; stats.flagsAligned += nearby.moved;
    const alignedChips = softlyAlignMajorComponents(nodes, edges, nets, originalIds, new Set(patternByMember.keys()), blocks);
    nodes = alignedChips.nodes; edges = alignedChips.edges; stats.chipsAligned = alignedChips.moved;
    edges = removeNetCycles(edges, nets).edges;
    const detours = rerouteFixedDetours(nodes, edges, nets);
    edges = detours.edges; stats.detoursRerouted = detours.changed;
    for (const n of nearby.rotated) rotations.set(n.id, { rotate: n.rotation!, center: n.center! });
    stats.componentsRotated = [...rotations.keys()].filter(id => {
        const before = input.children!.find(n => n.id === id), after = nodes.find(n => n.id === id);
        if (!before || !after) return false;
        return Math.abs(before.width! - after.width) > EPS || Math.abs(before.height! - after.height) > EPS
            || before.ports!.some(p => { const q = after.ports!.find(q => q.id === p.id)!; return Math.abs(p.x! - q.x!) > EPS || Math.abs(p.y! - q.y!) > EPS; });
    }).length;
    stats.elapsedMs = performance.now() - started;
    // The canvas origin is not a placement obstacle. Allow a local group to
    // use free space left/above the current drawing, then translate the whole
    // scene once. Relative pin/wire geometry is unchanged by this translation.
    const minX = Math.min(...nodes.map(n => n.x), ...edges.flatMap(e => path(e).map(p => p.x)));
    const minY = Math.min(...nodes.map(n => n.y), ...edges.flatMap(e => path(e).map(p => p.y)));
    const originalMinX = Math.min(...(input.children ?? []).map(n => n.x!), ...(input.edges ?? []).flatMap(e => path(e).map(p => p.x)));
    const originalMinY = Math.min(...(input.children ?? []).map(n => n.y!), ...(input.edges ?? []).flatMap(e => path(e).map(p => p.y)));
    // Remove blank margins left behind when the formerly topmost group moves
    // down. Preserve the original margin rather than preserving empty canvas.
    if (nodes.length && (minX < 0 || minY < 0 || minX > originalMinX || minY > originalMinY)) {
        const d = { x: minX < 0 ? gap.component - minX : Math.min(0, originalMinX - minX),
            y: minY < 0 ? gap.component - minY : Math.min(0, originalMinY - minY) };
        stats.sceneTranslation = d;
        nodes = nodes.map(n => ({ ...n, x: n.x + d.x, y: n.y + d.y }));
        edges = edges.map(e => withPath(e, path(e).map(p => shift(p, d))));
    }
    const bounds = nodes.length ? boundsOf(nodes) : { x: 0, y: 0, width: 0, height: 0 };
    scene.children = nodes; scene.edges = edges;
    scene.width = Math.max(bounds.x + bounds.width, ...edges.flatMap(e => path(e).map(p => p.x)), 0) + gap.component;
    scene.height = Math.max(bounds.y + bounds.height, ...edges.flatMap(e => path(e).map(p => p.y)), 0) + gap.component;
    return { scene, rotations, addedSymbols: newSymbols.filter(c => !merged.removed.has(c.designator)), removedSymbolIds: merged.removed, stats };
}
