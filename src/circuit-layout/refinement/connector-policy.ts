import type { ElkExtendedEdge, ElkNode } from 'elkjs';
import type { CircuitComponent } from '#types/circuit.ts';
import { getDesignatorLabel } from '#utils/component.ts';
import { isGroundSignal } from '../ground.ts';
import { isPowerSignal } from '../power.ts';
import { type Placed, EPS, normal, path, routeLength } from './geometry.ts';

export type ConnectorRole = 'input' | 'output';
export const CONNECTOR_OVERRIDE_RATIO = 0.7;

export const connectorOverrideWorthwhile = (otherLength: number, preferredLength: number) =>
    otherLength <= preferredLength * CONNECTOR_OVERRIDE_RATIO + EPS;

const isConnector = (component: CircuitComponent) => getDesignatorLabel(component.designator) === 'Разъемы'
    && component.pins.length >= 2 && component.pins.length <= 4;
const signalPins = (component: CircuitComponent) => component.pins.filter(p => p.signal_name
    && !isGroundSignal(p.signal_name) && !isPowerSignal(p.signal_name) && !/^NC$/i.test(p.signal_name));
const leadPins = (component: CircuitComponent) => {
    const signals = signalPins(component);
    return signals.length ? signals : component.pins.filter(p => p.signal_name
        && !isGroundSignal(p.signal_name) && !/^NC$/i.test(p.signal_name));
};
const namedRole = (component: CircuitComponent): ConnectorRole | undefined => {
    const pins = leadPins(component);
    const styles = new Set(pins.map(p => p.port_style).filter(style => style === 'in' || style === 'out'));
    if (styles.size > 1) return undefined;
    if (styles.size === 1) return styles.has('in') ? 'input' : 'output';
    const names = new Set(pins.flatMap(p => {
        const value = p.signal_name.toUpperCase();
        return [/(?:^|[_\-/])(?:IN|INPUT)(?:$|[_\-/])/.test(value) ? 'input' : undefined,
            /(?:^|[_\-/])(?:OUT|OUTPUT)(?:$|[_\-/])/.test(value) ? 'output' : undefined]
            .filter((role): role is ConnectorRole => !!role);
    }));
    return names.size === 1 ? [...names][0] : undefined;
};

/** Infer once from the first laid-out scene. Positions are a visual hint, not
 * electrical direction: ambiguous middle/mixed connectors remain unrestricted. */
export function inferConnectorRoles(nodes: readonly Placed[], edges: readonly ElkExtendedEdge[],
    components: readonly CircuitComponent[]) {
    const byId = new Map(nodes.map(n => [n.id, n]));
    const known = new Map(components.map(c => [c.designator, c]));
    const owner = new Map(nodes.flatMap(n => (n.ports ?? []).map(p => [p.id, n.id])));
    const result = new Map<string, ConnectorRole>();
    for (const component of components.filter(isConnector)) {
        const node = byId.get(component.designator), pins = leadPins(component);
        if (!node || !pins.length) continue;
        const explicit = namedRole(component);
        if (explicit) { result.set(component.designator, explicit); continue; }
        const ownPins = new Set(pins.map(p => `${component.designator}_pin_${p.pin_number}`));
        const neighbours = [...new Set(edges.filter(e => [...e.sources, ...e.targets].some(id => ownPins.has(id)))
            .flatMap(e => [...e.sources, ...e.targets].map(id => owner.get(id)))
            .filter((id): id is string => !!id && id !== component.designator && known.get(id)?.block_name === component.block_name))]
            .map(id => byId.get(id)!).filter(Boolean);
        if (!neighbours.length) continue;
        const center = node.x + node.width / 2;
        const votes = neighbours.map(n => n.x + n.width / 2 - center).filter(dx => Math.abs(dx) >= 20);
        if (!votes.length || votes.some(dx => Math.sign(dx) !== Math.sign(votes[0]))) continue;
        const peers = components.filter(c => c.block_name === component.block_name).map(c => byId.get(c.designator)).filter((n): n is Placed => !!n);
        if (peers.length < 2) continue;
        const left = Math.min(...peers.map(n => n.x)), right = Math.max(...peers.map(n => n.x + n.width));
        const relative = (center - left) / Math.max(1, right - left);
        if (votes[0] > 0 && relative <= 0.45) result.set(component.designator, 'input');
        if (votes[0] < 0 && relative >= 0.55) result.set(component.designator, 'output');
    }
    return result;
}

/** ELK nests block coordinates; terminal IDs remain stable across the flatten. */
export function inferConnectorRolesFromLayout(graph: ElkNode, components: readonly CircuitComponent[]) {
    const nodes: Placed[] = [];
    const visit = (parent: ElkNode, x: number, y: number) => {
        for (const child of parent.children ?? []) {
            const nx = x + (child.x ?? 0), ny = y + (child.y ?? 0);
            if (child.ports?.length) nodes.push({ ...child, x: nx, y: ny, width: child.width ?? 0, height: child.height ?? 0 });
            visit(child, nx, ny);
        }
    };
    visit(graph, graph.x ?? 0, graph.y ?? 0);
    return inferConnectorRoles(nodes, graph.edges ?? [], components);
}

/** Compare pin *order* as well as their facing direction. A two-pin connector
 * with both pins on one face can still have GND below the signal. */
export function connectorOrientationSeverity(node: Placed, component: CircuitComponent, role: ConnectorRole) {
    const signals = leadPins(component);
    if (!signals.length) return 0;
    const wanted = role === 'input' ? 1 : -1;
    const signalIds = signals.map(p => `${component.designator}_pin_${p.pin_number}`)
        .filter(id => node.ports?.some(p => p.id === id));
    if (!signalIds.length) return 0;
    const facing = signalIds.filter(id => normal(node, id).x !== wanted).length / signalIds.length;
    const signalY = signalIds.reduce((sum, id) => sum + node.ports!.find(p => p.id === id)!.y!, 0) / signalIds.length;
    const badGround = component.pins.some(p => isGroundSignal(p.signal_name)
        && (node.ports?.find(q => q.id === `${component.designator}_pin_${p.pin_number}`)?.y ?? Infinity) <= signalY + EPS);
    const badPower = signalPins(component).length > 0 && component.pins.some(p => isPowerSignal(p.signal_name)
        && (node.ports?.find(q => q.id === `${component.designator}_pin_${p.pin_number}`)?.y ?? -Infinity) >= signalY - EPS);
    return facing + (badGround || badPower ? 0.7 : 0);
}

export function connectorLeadLength(node: Placed, edges: readonly ElkExtendedEdge[]) {
    const pins = new Set((node.ports ?? []).map(p => p.id));
    return edges.filter(e => [...e.sources, ...e.targets].some(id => pins.has(id)))
        .reduce((sum, e) => sum + routeLength(path(e)), 0);
}
