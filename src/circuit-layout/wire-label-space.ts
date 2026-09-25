import type { ElkNode } from 'elkjs';
import type { SymbolWithMeta } from '#types/symbol.ts';

type Side = 'left' | 'right' | 'top' | 'bottom';
type Endpoint = { portId: string };

/** Reserve client-created wire labels before layout and routing. Enlarging the
 * owner box keeps unrelated routes outside the label strip. Virtual terminals
 * remain on its boundary; the editor extends them to the physical library pins. */
export function reserveWireLabelSpace(root: ElkNode, symbols: SymbolWithMeta[],
    signalMap: Record<string, Endpoint[] | undefined>,
    clientLabels: readonly { pinId: string; signalName: string }[]) {
    const labels = new Map(clientLabels.map(p => [p.pinId, p.signalName]));
    for (const [signal, endpoints] of Object.entries(signalMap)) {
        if (endpoints?.length === 1) labels.set(endpoints[0].portId, signal);
    }
    const leaves = new Map<string, ElkNode>();
    const visit = (node: ElkNode) => {
        if (node.children) node.children.forEach(visit);
        else leaves.set(node.id, node);
    };
    visit(root);
    for (const owner of symbols) {
        const node = leaves.get(owner.designator), symbol = owner.symbol;
        // Expanded pattern members have their own precomputed geometry.
        if (!node || symbol.padding === undefined || symbol.pins.length <= 3) continue;
        const sideOf = (p: { x: number; y: number }): Side =>
            ([['left', p.x], ['right', symbol.width - p.x], ['top', p.y], ['bottom', symbol.height - p.y]] as [Side, number][])
                .sort((a, b) => a[1] - b[1])[0][0];
        const extra = { left: 0, right: 0, top: 0, bottom: 0 };
        for (const pin of symbol.pins) {
            const label = labels.get(`${owner.designator}_pin_${pin.num}`);
            if (!label || label !== pin.signal_name || !label.trim() || /^nc$/i.test(label.trim())) continue;
            // Same character estimate and 10-unit rounding as EXT wirePortMinLength.
            const required = Math.max(20, Math.ceil((Array.from(label).length * 5 + 10) / 10) * 10);
            const side = sideOf(pin);
            extra[side] = Math.max(extra[side], required - symbol.padding);
        }
        if (!Object.values(extra).some(Boolean)) continue;
        const pins = symbol.pins.map(p => {
            const side = sideOf(p);
            return { ...p,
                x: p.x + extra.left + (side === 'left' ? -extra.left : side === 'right' ? extra.right : 0),
                y: p.y + extra.top + (side === 'top' ? -extra.top : side === 'bottom' ? extra.bottom : 0),
            };
        });
        symbol.width += extra.left + extra.right;
        symbol.height += extra.top + extra.bottom;
        symbol.center = { x: symbol.center.x + extra.left, y: symbol.center.y + extra.top };
        symbol.pins = pins;
        node.width = symbol.width;
        node.height = symbol.height;
        Object.assign(node, { center: symbol.center });
        const byId = new Map(pins.map(p => [`${owner.designator}_pin_${p.num}`, p]));
        for (const port of node.ports ?? []) {
            const pin = byId.get(port.id);
            if (pin) { port.x = pin.x; port.y = pin.y; }
        }
    }
}
