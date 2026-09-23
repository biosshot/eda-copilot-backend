import { getPinDirection } from '#circuit-layout/improvement.ts';
import { hasConnection } from '#circuit-layout/signals.ts';
import { chooseRotation, createMacroInstance, createPlacement, fitPlacements, macroId,
    pinForSignal, placementPin, rotateSymbolGeometry } from '../helpers.ts';
import type { CircuitLayoutPattern, MacroPort, PatternContext, PatternMatch } from '../types.ts';
import { isTwoPinKind } from './common.ts';

const ID = 'resistor-pull-bank';
const MINIMUM = 4;
const GAP = 20;
const PADDING = 20;
const compare = (a: string, b: string) => a.localeCompare(b, 'en', { numeric: true });

function entriesFor(match: PatternMatch, context: PatternContext) {
    return match.designators.map(id => {
        const component = context.componentsByDesignator.get(id)!;
        const symbol = context.symbolsByDesignator.get(id)!;
        const common = pinForSignal(component, match.roles.common)!;
        const branch = component.pins.find(pin => pin !== common)!;
        const geometry = chooseRotation(symbol.symbol, new Map([
            [String(branch.pin_number), 'NORTH'], [String(common.pin_number), 'SOUTH'],
        ]));
        return { component, symbol, common, branch, geometry };
    });
}

/** One common net and independent branches; values need not be identical. */
export const resistorPullBankPattern: CircuitLayoutPattern = {
    id: ID,
    // Let functional circuits and true parallel pairs claim their members first.
    priority: 4,
    findMatches(context) {
        const matches: PatternMatch[] = [];
        for (const [blockName, components] of context.componentsByBlock) {
            const groups = new Map<string, typeof components>();
            for (const c of components) {
                const symbol = context.symbolsByDesignator.get(c.designator)?.symbol;
                if (!isTwoPinKind(c, 'resistor') || !symbol || symbol.pins.length !== 2) continue;
                const [a, b] = c.pins.map(p => p.signal_name);
                if (!hasConnection(a) || !hasConnection(b) || a === b) continue;
                if (c.pins.some(p => !symbol.pins.some(q => String(q.num) === String(p.pin_number)))) continue;
                for (const net of [a, b]) {
                    const group = groups.get(net) ?? [];
                    group.push(c); groups.set(net, group);
                }
            }
            for (const [common, group] of groups) {
                if (group.length < MINIMUM) continue;
                const branches = group.map(c => c.pins.find(p => p.signal_name !== common)!.signal_name);
                if (new Set(branches).size !== group.length) continue;
                const match: PatternMatch = { patternId: ID, priority: this.priority, blockName,
                    designators: group.map(c => c.designator).sort(compare), roles: { common } };
                const entries = entriesFor(match, context);
                if (entries.some(e => {
                    const a = e.geometry.pins.find(p => String(p.num) === String(e.branch.pin_number))!;
                    const b = e.geometry.pins.find(p => String(p.num) === String(e.common.pin_number))!;
                    return getPinDirection(e.geometry, a) !== 'TOP' || getPinDirection(e.geometry, b) !== 'BOTTOM'
                        || Math.abs(a.x - b.x) > 2;
                })) continue;
                if (['width', 'height'].some(dimension => {
                    const sizes = entries.map(e => e.geometry[dimension as 'width' | 'height']);
                    return Math.min(...sizes) <= 0 || Math.max(...sizes) > 2 * Math.min(...sizes);
                })) continue;
                matches.push(match);
            }
        }
        return matches;
    },
    instantiate(match, context) {
        const entries = entriesFor(match, context);
        const members = new Set(match.designators);
        // Prefer the pin order and face of a neighbour serving several branches.
        // Only symbol geometry is consulted; electrical pin identities never change.
        const anchors = new Map<string, { side: ReturnType<typeof getPinDirection>; coordinates: Map<string, number> }>();
        for (const e of entries) for (const endpoint of context.signalEndpoints.get(e.branch.signal_name) ?? []) {
            if (members.has(endpoint.designator) || endpoint.blockName !== match.blockName) continue;
            const symbol = context.symbolsByDesignator.get(endpoint.designator)?.symbol;
            const pin = symbol?.pins.find(p => String(p.num) === String(endpoint.pinNumber));
            if (!symbol || !pin) continue;
            const side = getPinDirection(symbol, pin);
            const key = `${endpoint.designator}/${side}`;
            const anchor = anchors.get(key) ?? { side, coordinates: new Map<string, number>() };
            anchor.coordinates.set(e.branch.signal_name, side === 'LEFT' || side === 'RIGHT' ? pin.y : pin.x);
            anchors.set(key, anchor);
        }
        const anchor = [...anchors].sort((a, b) => b[1].coordinates.size - a[1].coordinates.size || compare(a[0], b[0]))[0]?.[1];
        const aligned = anchor && anchor.coordinates.size === entries.length ? anchor : undefined;
        const rotation = aligned ? ({ LEFT: 270, RIGHT: 90, TOP: 180, BOTTOM: 0 } as const)[aligned.side] : 0;
        entries.sort((a, b) => {
            if (aligned) {
                const delta = aligned.coordinates.get(a.branch.signal_name)! - aligned.coordinates.get(b.branch.signal_name)!;
                if (delta) return rotation === 90 || rotation === 180 ? -delta : delta;
            }
            return compare(a.branch.signal_name, b.branch.signal_name) || compare(a.component.designator, b.component.designator);
        });
        const branchY = Math.max(...entries.map(e => e.geometry.pins.find(p => String(p.num) === String(e.branch.pin_number))!.y));
        let x = 0;
        const placements = entries.map(e => {
            const pin = e.geometry.pins.find(p => String(p.num) === String(e.branch.pin_number))!;
            const p = createPlacement(e.symbol, e.geometry, e.geometry.rotation, x, branchY - pin.y);
            x += e.geometry.width + GAP;
            return p;
        });
        const fitted = fitPlacements(placements, PADDING);
        const ports: Omit<MacroPort, 'elkPortId'>[] = entries.map((e, i) => {
            const pin = placementPin(placements[i], e.branch.pin_number)!;
            return { key: e.branch.signal_name, pinNumber: `branch_${i}`, signalName: e.branch.signal_name,
                x: pin.x, y: 0, side: 'NORTH', terminalSide: 'NORTH', primaryPinId: pin.id, tailMode: 'straight' };
        });
        const commonPin = placementPin(placements[0], entries[0].common.pin_number)!;
        ports.push({ key: 'COMMON', pinNumber: 'common', signalName: match.roles.common,
            x: commonPin.x, y: fitted.height, side: 'SOUTH', terminalSide: 'SOUTH',
            primaryPinId: commonPin.id, tailMode: 'straight' });
        // Turn the entire macro before ELK, including port tails and pin metadata.
        const frame = rotateSymbolGeometry({ width: fitted.width, height: fitted.height,
            center: { x: 0, y: 0 }, pins: ports.map((p, i) => ({ num: i, name: '', signal_name: '', part: '', x: p.x, y: p.y })) }, rotation);
        const transform = (px: number, py: number) => rotation === 90 ? { x: py, y: fitted.width - px }
            : rotation === 180 ? { x: fitted.width - px, y: fitted.height - py }
            : rotation === 270 ? { x: fitted.height - py, y: px } : { x: px, y: py };
        const sides = ['NORTH', 'WEST', 'SOUTH', 'EAST'] as const;
        for (const p of placements) {
            const center = transform(p.x + p.width / 2, p.y + p.height / 2);
            const geometry = rotateSymbolGeometry({ width: p.width, height: p.height, center: p.center, pins: p.pins }, rotation);
            p.x = center.x - geometry.width / 2; p.y = center.y - geometry.height / 2;
            p.width = geometry.width; p.height = geometry.height; p.center = geometry.center;
            p.rotate = (p.rotate + rotation) % 360;
            p.pins = p.pins.map((pin, i) => ({ ...pin, x: geometry.pins[i].x, y: geometry.pins[i].y,
                side: sides[(sides.indexOf(pin.side) + rotation / 90) % 4] }));
        }
        ports.forEach((port, i) => {
            port.x = frame.pins[i].x; port.y = frame.pins[i].y;
            port.side = sides[(sides.indexOf(port.side) + rotation / 90) % 4];
            port.terminalSide = port.side;
        });
        const macro = createMacroInstance({ id: macroId(ID, match.designators), patternId: ID,
            blockName: match.blockName, absorbedDesignators: match.designators,
            width: frame.width, height: frame.height, placements, ports });
        macro.forceRoutedSignals = [match.roles.common];
        macro.routingClearance = 10;
        macro.refinementRotations = [90, 180, 270];
        macro.forceBoundaryPorts = false;
        return macro;
    },
};
