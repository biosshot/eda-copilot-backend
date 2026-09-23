import test from 'node:test';
import assert from 'node:assert/strict';
import { reserveWireLabelSpace } from '../src/circuit-layout/wire-label-space.ts';
import type { SymbolWithMeta } from '../src/types/symbol.ts';
import type { ElkNode } from 'elkjs';

function fixture(padding = 10) {
    const owner: SymbolWithMeta = { designator: 'U1.2', block_name: 'main', symbol: {
        width: 200, height: 200, center: { x: 100, y: 100 }, padding,
        pins: [[0, 80], [200, 80], [80, 0], [80, 200]].map(([x, y], i) => ({
            num: String(i), name: String(i), signal_name: 'ABCDEFGHIJKL', part: 'U.2', x, y,
        })),
    } };
    const leaf: ElkNode = { id: owner.designator, width: 200, height: 200,
        ports: owner.symbol.pins.map(p => ({ id: `U1.2_pin_${p.num}`, x: p.x, y: p.y })) };
    return { owner, leaf, root: { id: 'root', children: [leaf] } };
}

for (const side of [0, 1, 2, 3]) test(`reserves only label side ${side}, keeping terminals on the boundary`, () => {
    const { owner, leaf, root } = fixture();
    reserveWireLabelSpace(root, [owner], {}, [{ pinId: `U1.2_pin_${side}`, signalName: 'ABCDEFGHIJKL' }]);
    assert.equal(owner.symbol.width, side < 2 ? 260 : 200);
    assert.equal(owner.symbol.height, side < 2 ? 200 : 260);
    assert.deepEqual(owner.symbol.center, { x: side === 0 ? 160 : 100, y: side === 2 ? 160 : 100 });
    assert.ok(leaf.ports!.every(p => p.x === 0 || p.x === leaf.width || p.y === 0 || p.y === leaf.height));
    assert.deepEqual(leaf.ports!.map(p => [p.x, p.y]), owner.symbol.pins.map(p => [p.x, p.y]));
});

test('existing large-IC padding covers the label without adding twice', () => {
    const { owner, root } = fixture(70);
    const before = structuredClone(owner);
    reserveWireLabelSpace(root, [owner], { ABCDEFGHIJKL: [{ portId: 'U1.2_pin_0' }] }, []);
    assert.deepEqual(owner, before);
});

test('routed signals and generated short-symbol aliases do not reserve label space', () => {
    const { owner, root } = fixture();
    const before = structuredClone(owner);
    reserveWireLabelSpace(root, [owner], {
        ABCDEFGHIJKL: [{ portId: 'U1.2_pin_0' }, { portId: 'R1_pin_1' }],
        'ext_ABCDEFGHIJKL': [{ portId: 'U1.2_pin_1' }],
    }, []);
    assert.deepEqual(owner, before);
});
