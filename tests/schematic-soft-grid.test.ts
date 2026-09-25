import { test } from 'node:test';
import assert from 'node:assert/strict';
import { packSchematicRectangles, PAGE_SOFT_GRID } from '../src/utils/schematic-packing.ts';
import { type Placed, overlaps, path } from '../src/circuit-layout/refinement/geometry.ts';
import { softlyAlignMajorComponents } from '../src/circuit-layout/refinement/soft-align.ts';

test('large unconnected IC sections settle into a row while an obstacle retains a stagger', () => {
    const chip = (id: string, x: number, y: number): Placed => ({ id, x, y, width: 150, height: 300,
        ports: Array.from({ length: 16 }, (_, i) => ({ id: `${id}_pin_${i}`, x: 0, y: 20 + i * 10 })) });
    const nodes = [chip('U1', 0, 100), chip('U2', 200, 160), chip('U3', 400, 110)];
    const originals = new Set(nodes.map(n => n.id));
    const first = softlyAlignMajorComponents(nodes, [], new Map(), originals);
    assert.deepEqual(first.nodes.map(n => n.y), [110, 110, 110]);
    assert.deepEqual(nodes.map(n => n.y), [100, 160, 110]);
    const obstacle: Placed = { id: 'R1', x: 200, y: 80, width: 60, height: 40, ports: [] };
    const guarded = softlyAlignMajorComponents([...nodes, obstacle], [], new Map(), new Set([...originals, obstacle.id]));
    assert.equal(guarded.nodes.find(n => n.id === 'U2')!.y, 160);
    assert(guarded.nodes.every((n, i) => guarded.nodes.slice(i + 1).every(other => !overlaps(n, other, 0))));
    const separateBlocks = softlyAlignMajorComponents(nodes, [], new Map(), originals, new Set(),
        new Map(nodes.map(n => [n.id, n.id])));
    assert.equal(separateBlocks.moved, 0);
});

test('an aligned IC lead is rerouted to its exact pin while preserving the original input', () => {
    const chip = (id: string, x: number, y: number, right = false): Placed => ({ id, x, y, width: 150, height: 300,
        ports: Array.from({ length: 16 }, (_, i) => ({ id: `${id}_pin_${i}`, x: right && i === 0 ? 150 : 0, y: 20 + i * 10 })) });
    const nodes = [chip('U1', 0, 100, true), chip('U2', 200, 130), chip('U3', 400, 100)];
    const edge = { id: 'signal', sources: ['U1_pin_0'], targets: ['U2_pin_0'], sections: [{ id: 'segment',
        startPoint: { x: 150, y: 120 }, bendPoints: [{ x: 170, y: 120 }, { x: 170, y: 150 }], endPoint: { x: 200, y: 150 } }] };
    const nets = new Map([['U1_pin_0', 'SIGNAL'], ['U2_pin_0', 'SIGNAL']]);
    const result = softlyAlignMajorComponents(nodes, [edge], nets, new Set(nodes.map(n => n.id)));
    assert.equal(result.nodes.find(n => n.id === 'U2')!.y, 100);
    assert.deepEqual(path(result.edges[0]).at(-1), { x: 200, y: 120 });
    assert.deepEqual(path(edge).at(-1), { x: 200, y: 150 });
});

test('page grid attracts free blocks without reducing their padded clearance or enlarging bounds', () => {
    const items = [{ id: 'a', width: 273, height: 280 }, { id: 'b', width: 217, height: 230 },
        { id: 'c', width: 211, height: 200 }, { id: 'd', width: 130, height: 100 }];
    const baseline = packSchematicRectangles(items, 85);
    const packed = packSchematicRectangles(items, 85, [], undefined, PAGE_SOFT_GRID);
    assert.equal(packed.positions.get('d')!.y % PAGE_SOFT_GRID.step, 0);
    assert.equal(packed.width, baseline.width);
    assert.equal(packed.height, baseline.height);
    for (const [i, a] of items.entries()) for (const b of items.slice(i + 1)) {
        const pa = packed.positions.get(a.id)!, pb = packed.positions.get(b.id)!;
        assert(!overlaps({ ...pa, width: a.width, height: a.height }, { ...pb, width: b.width, height: b.height }, 84.999));
    }
});

test('a fragmented grid packing trial falls back to another block order', () => {
    const items = [{ id: 'flash', width: 454.5, height: 298 }, { id: 'memory', width: 2006.5, height: 1531 },
        { id: 'fpga', width: 1435, height: 1775 }];
    const result = packSchematicRectangles(items, 85, [], undefined, PAGE_SOFT_GRID);
    assert.equal(result.positions.size, items.length);
    for (const [i, a] of items.entries()) for (const b of items.slice(i + 1)) {
        const pa = result.positions.get(a.id)!, pb = result.positions.get(b.id)!;
        assert(!overlaps({ ...pa, width: a.width, height: a.height }, { ...pb, width: b.width, height: b.height }, 84.999));
    }
});
