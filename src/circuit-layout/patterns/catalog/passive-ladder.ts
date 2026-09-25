import { getDesignatorLabel } from '#utils/component.ts';
import type { CircuitComponent } from '#types/circuit.ts';
import { chooseRotation, createMacroInstance, createPlacement, createShortSymbolPlacement, macroId,
    pinForSignal, placementPin, setPinRoutingSignal, shortSymbolKindForSignal } from '../helpers.ts';
import type { CircuitLayoutPattern, MacroComponentPlacement, PatternContext, PatternMatch } from '../types.ts';
import { hasOpposedAxialPins } from './parallel-two-pin.ts';

type Branch = { parts: string[]; nets: string[]; kind: string };
type Bundle = { left: string; right: string; branches: Branch[] };
type Shape = { kind: 'series' | 'parallel' | 'ladder'; nets: string[]; bundles: Bundle[] };
const ID = 'passive-ladder';
const PART_GAP = 15;
const ARM_GAP = 20;
const STAGE_GAP = 30;
const PAD = 25;
const compare = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });
const key = (a: string, b: string) => [a, b].sort(compare).join('\u0000');

function passiveKind(c: CircuitComponent) {
    const label = getDesignatorLabel(c.designator);
    if (label === 'Резисторы') return 'R';
    if (label === 'Конденсаторы') return 'C';
    if (/^L\d/i.test(c.designator)) return 'L';
    if (/^D\d/i.test(c.designator)) return 'D';
    return '';
}

function eligible(c: CircuitComponent, context: PatternContext) {
    const symbol = context.symbolsByDesignator.get(c.designator)?.symbol;
    return c.pins.length === 2 && !!passiveKind(c) && !!symbol && hasOpposedAxialPins(symbol)
        && c.pins.every(p => p.signal_name && !/^NC$/i.test(p.signal_name))
        && c.pins[0].signal_name !== c.pins[1].signal_name;
}

/** The original graph decides whether a series junction is really private. */
function privateJunction(net: string, a: CircuitComponent, b: CircuitComponent, context: PatternContext) {
    if (shortSymbolKindForSignal(net) || context.externalSignals?.has(net)) return false;
    const pins = (context.originalSignalEndpoints ?? context.signalEndpoints).get(net) ?? [];
    return pins.length === 2 && new Set(pins.map(p => p.designator)).size === 2
        && pins.every(p => (p.designator === a.designator || p.designator === b.designator)
            && p.blockName === a.block_name);
}

function buildShapes(context: PatternContext, components: CircuitComponent[]) {
    const parts = components.filter(c => eligible(c, context)).sort((a, b) => compare(a.designator, b.designator));
    const byId = new Map(parts.map(c => [c.designator, c]));
    const adjacent = new Map(parts.map(c => [c.designator, [] as { id: string; net: string }[]]));
    const byNet = new Map<string, CircuitComponent[]>();
    for (const c of parts) for (const p of c.pins) {
        const list = byNet.get(p.signal_name) ?? [];
        list.push(c); byNet.set(p.signal_name, list);
    }
    for (const [net, pair] of byNet) {
        if (pair.length !== 2 || passiveKind(pair[0]) !== passiveKind(pair[1])
            || !['R', 'C'].includes(passiveKind(pair[0]))
            || !privateJunction(net, pair[0], pair[1], context)) continue;
        adjacent.get(pair[0].designator)!.push({ id: pair[1].designator, net });
        adjacent.get(pair[1].designator)!.push({ id: pair[0].designator, net });
    }
    // Branching junctions cannot be reduced to a series component.
    for (const [id, links] of adjacent) if (links.length > 2) {
        for (const link of links) adjacent.set(link.id, adjacent.get(link.id)!.filter(other => other.id !== id));
        adjacent.set(id, []);
    }
    const visited = new Set<string>(), branches: Branch[] = [];
    for (const start of parts.filter(c => adjacent.get(c.designator)!.length <= 1)) {
        if (visited.has(start.designator)) continue;
        const ordered: string[] = [], innerNets: string[] = [];
        let current = start.designator, previous = '';
        while (!visited.has(current)) {
            visited.add(current); ordered.push(current);
            const next = adjacent.get(current)!.find(link => link.id !== previous && !visited.has(link.id));
            if (!next) break;
            innerNets.push(next.net); previous = current; current = next.id;
        }
        const first = byId.get(ordered[0])!, last = byId.get(ordered.at(-1)!)!;
        const left = first.pins.find(p => p.signal_name !== innerNets[0])?.signal_name;
        const right = ordered.length === 1 ? first.pins.find(p => p.signal_name !== left)?.signal_name
            : last.pins.find(p => p.signal_name !== innerNets.at(-1))?.signal_name;
        if (!left || !right || left === right) continue;
        branches.push({ parts: ordered, nets: [left, ...innerNets, right], kind: passiveKind(first) });
    }
    // A cyclic network has no unambiguous ends; leave its members to ordinary layout.
    const bundlesByKey = new Map<string, Bundle>();
    for (const branch of branches) {
        const left = branch.nets[0], right = branch.nets.at(-1)!;
        const pair = [left, right].sort(compare);
        const k = key(left, right), bundle = bundlesByKey.get(k) ?? { left: pair[0], right: pair[1], branches: [] };
        bundle.branches.push(branch); bundlesByKey.set(k, bundle);
    }
    const bundles = [...bundlesByKey.values()].sort((a, b) => compare(key(a.left, a.right), key(b.left, b.right)));
    const shapes: Shape[] = [];
    const links = new Map<string, Bundle[]>();
    for (const bundle of bundles) for (const net of [bundle.left, bundle.right]) {
        const list = links.get(net) ?? []; list.push(bundle); links.set(net, list);
    }
    // Walk from each grounded leaf, through unbranched tapped stages.
    const consumed = new Set<Bundle>();
    for (const ground of [...links.keys()].filter(n => shortSymbolKindForSignal(n) === 'GND' && links.get(n)!.length === 1)) {
        const backwards: Bundle[] = [], nets = [ground];
        let at = ground, previous: Bundle | undefined;
        while (links.get(at)?.length === (previous ? 2 : 1)) {
            const next = links.get(at)!.find(b => b !== previous);
            if (!next || consumed.has(next)) break;
            backwards.push(next);
            at = next.left === at ? next.right : next.left;
            nets.push(at); previous = next;
            if (shortSymbolKindForSignal(at)) break;
        }
        if (backwards.length < 2 || !backwards.some(b => b.branches.length > 1 || b.branches[0].parts.length > 1)) continue;
        backwards.forEach(b => consumed.add(b));
        shapes.push({ kind: 'ladder', nets: nets.reverse(), bundles: backwards.reverse() });
    }
    for (const bundle of bundles) {
        if (consumed.has(bundle)) continue;
        // Ordinary parallel parts already have a dedicated pattern. Only a
        // series arm needs this composite representation.
        if (bundle.branches.length > 1 && bundle.branches.some(branch => branch.parts.length > 1))
            shapes.push({ kind: 'parallel', nets: [bundle.left, bundle.right], bundles: [bundle] });
        else if (bundle.branches[0].parts.length > 1) shapes.push({ kind: 'series', nets: [bundle.left, bundle.right], bundles: [bundle] });
    }
    return shapes;
}

function members(shape: Shape) { return shape.bundles.flatMap(b => b.branches.flatMap(branch => branch.parts)); }

export const passiveLadderPattern: CircuitLayoutPattern = {
    id: ID, priority: 50,
    findMatches(context) {
        return [...context.componentsByBlock].flatMap(([blockName, components]) =>
            buildShapes(context, components).map(shape => ({
                patternId: ID, priority: shape.kind === 'ladder' ? 50 : shape.kind === 'parallel' ? 35 : 25,
                blockName, designators: members(shape), roles: { shape: JSON.stringify(shape) },
            })));
    },
    instantiate(match: PatternMatch, context) {
        const shape = JSON.parse(match.roles.shape) as Shape;
        const ids = new Set(members(shape));
        if (ids.size !== match.designators.length || [...ids].some(id =>
            !context.componentsByDesignator.has(id) || !context.symbolsByDesignator.has(id))) return null;
        // Recheck every private junction against the full page after another macro was accepted.
        for (const bundle of shape.bundles) for (const branch of bundle.branches)
            for (let i = 1; i < branch.parts.length; i++) {
                const a = context.componentsByDesignator.get(branch.parts[i - 1])!;
                const b = context.componentsByDesignator.get(branch.parts[i])!;
                if (!privateJunction(branch.nets[i], a, b, context)) return null;
            }
        const placements: MacroComponentPlacement[] = [];
        const stagePins = new Map<string, string>();
        const stages = shape.bundles.map((bundle, s) => bundle.branches.map(branch => {
            const from = shape.nets[s];
            const parts = branch.nets[0] === from ? branch.parts : [...branch.parts].reverse();
            const nets = branch.nets[0] === from ? branch.nets : [...branch.nets].reverse();
            const items = parts.map((id, index) => {
                const component = context.componentsByDesignator.get(id)!;
                const symbol = context.symbolsByDesignator.get(id)!;
                const incoming = pinForSignal(component, nets[index]);
                const outgoing = pinForSignal(component, nets[index + 1]);
                if (!incoming || !outgoing) return null;
                const geometry = chooseRotation(symbol.symbol, new Map([
                    [String(incoming.pin_number), 'WEST'], [String(outgoing.pin_number), 'EAST'],
                ]));
                const incomingPin = geometry.pins.find(pin => pin.num == incoming.pin_number);
                const outgoingPin = geometry.pins.find(pin => pin.num == outgoing.pin_number);
                if (!incomingPin || !outgoingPin) return null;
                // Axial pins normally share a Y coordinate. Use their midpoint
                // so a small library-symbol offset is distributed evenly.
                const pinAxisY = (incomingPin.y + outgoingPin.y) / 2;
                return { symbol, geometry, incoming, outgoing, incomingPin, pinAxisY };
            });
            return items.some(item => !item) ? null : { nets, items };
        }));
        if (stages.some(stage => stage.some(branch => !branch))) return null;

        // All stages share row pin axes. Size each row from the largest body
        // extent above and below its pins, including symbols of unequal size.
        const rowCount = Math.max(...stages.map(stage => stage.length));
        const rowAxes: number[] = [];
        let contentBottom = PAD;
        for (let row = 0; row < rowCount; row++) {
            const items = stages.flatMap(stage => stage[row]?.items ?? []).map(item => item!);
            const above = Math.max(...items.map(item => item.pinAxisY));
            const below = Math.max(...items.map(item => item.geometry.height - item.pinAxisY));
            const axis = contentBottom + above;
            rowAxes.push(axis);
            contentBottom = axis + below + ARM_GAP;
        }
        contentBottom -= ARM_GAP;

        let stageX = PAD;
        for (const stage of stages) {
            let widest = 0;
            const firstPinX = Math.max(...stage.map(branch => branch!.items[0]!.incomingPin.x));
            for (let row = 0; row < stage.length; row++) {
                const branch = stage[row]!;
                const { nets, items } = branch;
                let x = stageX + firstPinX - items[0]!.incomingPin.x;
                for (let i = 0; i < items.length; i++) {
                    const item = items[i]!;
                    const placed = createPlacement(item.symbol, item.geometry, item.geometry.rotation,
                        x, rowAxes[row] - item.pinAxisY);
                    placements.push(placed);
                    if (!stagePins.has(nets[i])) stagePins.set(nets[i], placementPin(placed, item.incoming.pin_number)!.id);
                    stagePins.set(nets[i + 1], placementPin(placed, item.outgoing.pin_number)!.id);
                    x += item.geometry.width + PART_GAP;
                }
                widest = Math.max(widest, x - stageX - PART_GAP);
            }
            stageX += widest + STAGE_GAP;
        }
        const width = stageX - STAGE_GAP + PAD;
        const height = contentBottom + PAD;
        const id = macroId(ID, match.designators);
        const ports = [] as Parameters<typeof createMacroInstance>[0]['ports'];
        const original = context.originalSignalEndpoints ?? context.signalEndpoints;
        let shortOrdinal = 0;
        for (let i = 0; i < shape.nets.length; i++) {
            const net = shape.nets[i];
            const pinId = stagePins.get(net);
            const pin = placements.flatMap(p => p.pins.map(q => ({ ...q, x: p.x + q.x, y: p.y + q.y }))).find(p => p.id === pinId);
            if (!pin) return null;
            const supply = shortSymbolKindForSignal(net);
            if (supply) {
                const flag = createShortSymbolPlacement({ kind: supply, signalName: net, blockName: match.blockName,
                    scope: id, ordinal: shortOrdinal++, x: pin.x, y: pin.y });
                flag.x = pin.x - flag.width / 2;
                flag.y = supply === 'GND' ? height + 10 : -flag.height - 10;
                placements.push(flag);
                // One short symbol terminates this entire local supply rail.
                for (const placement of placements) for (const terminal of placement.pins) {
                    if (placement === flag || terminal.signal_name !== net) continue;
                    setPinRoutingSignal(placement, terminal.num, flag.designator);
                }
                continue;
            }
            const external = (original.get(net) ?? []).some(p => !ids.has(p.designator))
                || context.externalSignals?.has(net);
            if (!external) continue;
            const side = i === 0 ? 'WEST' : i === shape.nets.length - 1 ? 'EAST' : 'NORTH';
            ports.push({ key: `TAP_${i}`, pinNumber: `tap_${i}`, signalName: net,
                x: side === 'WEST' ? 0 : side === 'EAST' ? width : pin.x,
                y: side === 'NORTH' ? 0 : pin.y, side, terminalSide: side, primaryPinId: pin.id });
        }
        const macro = createMacroInstance({ id, patternId: ID, blockName: match.blockName,
            absorbedDesignators: match.designators, width, height: Math.max(height, ...placements.map(p => p.y + p.height)) + PAD,
            placements, ports });
        macro.forceRoutedSignals = shape.nets;
        macro.routingClearance = 10;
        macro.forceBoundaryPorts = false;
        return macro;
    },
};
