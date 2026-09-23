import test from 'node:test';
import assert from 'node:assert/strict';
import { circuitToSymbols, getSymbol } from '../src/devices/symbols/symbol-parser.ts';
import { rotateSymbolGeometry } from '../src/circuit-layout/patterns/helpers.ts';
import { autoPlaceCircuitWithHierarchy } from '../src/circuit-layout/index.ts';
import { edgeSegments, segmentThroughBox } from '../src/circuit-layout/refinement/geometry.ts';
import { assertExpandedLayout, createPatternFixtureCircuit } from './patterns/helpers.ts';

function fixture(counts: number[]) {
    const components = counts.map((count, section) => ({
        designator: `U1.${section + 1}`, value: 'test IC', part_uuid: String(count),
        search_query: 'test IC', block_name: '__v_root__',
        pins: Array.from({ length: count }, (_, i) => ({ pin_number: String(i + 1), name: `IO${i}`, signal_name: `NET${i}` })),
    }));
    const loadSymbol: typeof getSymbol = async (partUuid, partId) => {
        const count = Number(partUuid);
        const height = Math.ceil(count / 2) * 20;
        return {
            dataStr: '', rect: [-50, -height / 2, 50, height / 2], partIds: ['U.1', 'U.2'],
            pins: Array.from({ length: count }, (_, i) => ({
                num: String(i + 1), name: `IO${i}`, signal_name: '', part: `U.${(partId ?? 0) + 1}`,
                x: i % 2 ? 50 : -50, y: height / 2 - 10 - Math.floor(i / 2) * 20,
            })),
        };
    };
    return { circuit: createPatternFixtureCircuit('padding', 'IC pair', components), loadSymbol };
}

test('padding thresholds and cap preserve physical terminals at every rotation', async () => {
    for (const [count, extra] of [[2, 0], [24, 0], [25, 20], [40, 20], [41, 30], [57, 40], [73, 50], [89, 60], [381, 60]]) {
        const { circuit, loadSymbol } = fixture([count]);
        const { nodes } = await circuitToSymbols(circuit, loadSymbol);
        const actual = nodes[0].symbol;
        assert.equal(actual.width, 120 + extra * 2);
        // ELK requires virtual terminals on the reserved box boundary.
        // Their lead allowance grows with padding; library insertion stays centered.
        assert.equal(actual.center!.x, actual.width / 2);
        assert.ok(actual.pins.every(p => p.x === 0 || p.x === actual.width));
        for (const angle of [0, 90, 180, 270]) {
            const a = rotateSymbolGeometry(actual, angle);
            for (const [i, pin] of a.pins.entries()) {
                let x = i % 2 ? 60 + extra : -60 - extra;
                let y = 10 + Math.floor(i / 2) * 20 - Math.ceil(count / 2) * 10;
                for (let turn = 0; turn < angle; turn += 90) [x, y] = [y, -x];
                assert.deepEqual([pin.x - a.center!.x, pin.y - a.center!.y], [x || 0, y || 0]);
            }
        }
    }
});

test('multipart padding is calculated independently for each section', async () => {
    const { circuit, loadSymbol } = fixture([24, 64]);
    const { nodes, subParts } = await circuitToSymbols(circuit, loadSymbol);
    assert.deepEqual(nodes.map(n => n.symbol.width), [120, 200]);
    assert.deepEqual(subParts, { 'U1.1': 'U.1', 'U1.2': 'U.2' });
});

for (const count of [2, 32, 96]) for (const layoutRefinement of [false, true]) {
    test(`layout and routing preserve connections between ${count}-pin sections (refinement=${layoutRefinement})`, async () => {
        const { circuit, loadSymbol } = fixture([count, count]);
        const { nodes: symbols } = await circuitToSymbols(circuit, loadSymbol);
        const result = await autoPlaceCircuitWithHierarchy(circuit, symbols, {}, { layoutMode: 'legacy', layoutPatterns: true, layoutRefinement });
        assertExpandedLayout({ circuit, symbols }, result);
        assert.ok(result.positioned.every(p => [p.x, p.y, p.width, p.height].every(Number.isFinite)));
    });
}

test('client wire labels enlarge a section while preserving its routed connections', async () => {
    const { circuit, loadSymbol } = fixture([32, 32, 2, 2]);
    for (const c of circuit.components.slice(2)) for (const p of c.pins) p.signal_name = `UNRELATED_${p.pin_number}`;
    for (const c of circuit.components.slice(0, 2)) for (const p of c.pins.slice(0, 6)) {
        p.signal_name = `LONG_EXTERNAL_SIGNAL_${c.designator}_${p.pin_number}`;
    }
    const { nodes: symbols } = await circuitToSymbols(circuit, loadSymbol);
    const originalWidth = symbols[0].symbol.width;
    const result = await autoPlaceCircuitWithHierarchy(circuit, symbols, {}, { layoutMode: 'legacy', layoutPatterns: true, layoutRefinement: true });
    assertExpandedLayout({ circuit, symbols }, result);
    for (const node of result.positioned.filter(p => ['U1.1', 'U1.2'].includes(p.designator))) {
        assert.ok(node.width > originalWidth, 'Label reserve must survive layout and refinement');
        const unrelated = result.edges.filter(e => [...e.sources, ...e.targets].every(id => !id.startsWith(`${node.designator}_pin_`)));
        assert.ok(unrelated.length > 0);
        for (const edge of unrelated) for (const segment of edgeSegments(edge)) {
            assert.ok(!segmentThroughBox(segment, node), 'Unrelated routing must avoid the reserved label strip');
        }
    }
});


