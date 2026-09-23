import test from 'node:test';
import assert from 'node:assert/strict';
import type { ElkNode } from 'elkjs';
import type { CircuitComponent } from '../src/types/circuit.ts';
import type { SymbolWithMeta } from '../src/types/symbol.ts';
import { autoPlaceCircuitWithHierarchy } from '../src/circuit-layout/index.ts';
import { sharedSupplyNets } from '../src/circuit-layout/graph-order.ts';
import { component, createPatternFixtureCircuit } from './patterns/helpers.ts';

function geometry(c: CircuitComponent): SymbolWithMeta {
    const chip = /^U/.test(c.designator);
    return { designator: c.designator, block_name: c.block_name, symbol: {
        width: 100, height: 100, center: { x: 50, y: 50 },
        pins: c.pins.map((p, i) => ({ num: p.pin_number, name: p.name, signal_name: p.signal_name, port_style: p.port_style, part: '',
            x: chip ? (i % 2) * 100 : 50, y: chip ? 20 + Math.floor(i / 2) * 20 : i * 100 })),
    } };
}
const leaves = (n: ElkNode): ElkNode[] => n.children ? n.children.flatMap(leaves) : [n];

for (const mode of ['cross-block', 'external'] as const) for (const count of [4, 5, 24]) {
    test(`port styles respect the dense-pin threshold on a multipart unit: ${mode}, ${count} pins`, async () => {
        const owner = component('U3.5', Array.from({ length: count }, (_, i) =>
            [i + 1, `IO${i}`, `DATA_${i}`] as [number, string, string]), 'FPGA');
        const styles = ['in', 'out', 'bi'] as const;
        owner.pins.forEach((pin, i) => { pin.port_style = styles[i % styles.length]; });
        const components = [owner];
        if (mode === 'cross-block') components.push(component('U17', owner.pins.map(pin =>
            [Number(pin.pin_number), pin.name, pin.signal_name]), 'Memory'));
        const circuit = createPatternFixtureCircuit('styled-dense', 'styled dense connections', components);
        circuit.blocks = components.map(c => ({ name: c.block_name, description: '', next_block_names: [] }));
        const symbols = components.map(c => {
            const symbol = geometry(c);
            symbol.symbol.height = count * 20 + 40;
            symbol.symbol.center.y = symbol.symbol.height / 2;
            symbol.symbol.pins.forEach((pin, i) => { pin.x = 0; pin.y = 20 + i * 20; });
            return symbol;
        });
        const result = await autoPlaceCircuitWithHierarchy(circuit, symbols, undefined, {
            layoutRefinement: true, layoutPatterns: false,
            externalSignals: mode === 'external' ? owner.pins.map(pin => pin.signal_name) : undefined,
        });
        const ports = result.addedSymbol.filter(c => c.block_name === 'block_FPGA');
        assert.equal(ports.length, count < 5 ? count : 0);
        for (const pin of owner.pins) {
            const pinId = `${owner.designator}_pin_${pin.pin_number}`;
            const edges = result.edges.filter(edge => [...edge.sources, ...edge.targets].includes(pinId));
            if (count < 5) {
                const port = ports.find(c => c.pins[0].signal_name === pin.signal_name);
                assert.equal(port?.pins[0].port_style, pin.port_style);
                assert(edges.some(edge => [...edge.sources, ...edge.targets].includes(`${port!.designator}_pin_1`)));
            } else {
                assert.equal(edges.length, 0, 'leave the named pin to the client wire label');
                if (mode === 'cross-block') assert(result.clientManagedLabels?.some(label =>
                    label.pinId === pinId && label.signalName === pin.signal_name));
            }
        }
    });
}

test('required other-page signals get ports even in a dense block, without duplicating automatic ports', async () => {
    const owner = component('U3', Array.from({ length: 5 }, (_, i) =>
        [i + 1, `IO${i}`, `DATA_${i}`] as [number, string, string]), 'FPGA');
    const circuit = createPatternFixtureCircuit('other-page', 'other-page', [owner]);
    circuit.blocks = [{ name: 'FPGA', description: '', next_block_names: [] }];
    const result = await autoPlaceCircuitWithHierarchy(circuit, [geometry(owner)], undefined, {
        layoutRefinement: true, layoutPatterns: false,
        externalSignals: owner.pins.map(pin => pin.signal_name), requiredExternalSignals: ['DATA_0'],
    });
    const ports = result.addedSymbol.filter(c => c.block_name === 'block_FPGA');
    assert.deepEqual(ports.map(c => c.pins[0].signal_name), ['DATA_0']);
});

test('required other-page signal already connected between blocks keeps its automatic ports', async () => {
    const components = [component('U1', [[1, 'IO', 'DATA']], 'A'),
        component('U2', [[1, 'IO', 'DATA']], 'B')];
    const circuit = createPatternFixtureCircuit('other-page-shared', 'other-page-shared', components);
    circuit.blocks = ['A', 'B'].map(name => ({ name, description: '', next_block_names: [] }));
    const result = await autoPlaceCircuitWithHierarchy(circuit, components.map(geometry), undefined, {
        layoutRefinement: true, layoutPatterns: false,
        externalSignals: ['DATA'], requiredExternalSignals: ['DATA'],
    });
    const ports = result.addedSymbol.filter(c => c.pins[0].signal_name === 'DATA');
    assert.equal(ports.length, 2);
});

test('required signal gets a port in a dense block even when another block already has one', async () => {
    const dense = component('U3', Array.from({ length: 5 }, (_, i) =>
        [i + 1, `IO${i}`, `DATA_${i}`] as [number, string, string]), 'FPGA');
    const other = component('U2', dense.pins.map((pin, i) =>
        [i + 1, pin.name, pin.signal_name] as [number, string, string]), 'IO');
    const circuit = createPatternFixtureCircuit('cross-page-dense', 'cross-page-dense', [dense, other]);
    circuit.blocks = ['FPGA', 'IO'].map(name => ({ name, description: '', next_block_names: [] }));
    const result = await autoPlaceCircuitWithHierarchy(circuit, [geometry(dense), geometry(other)], undefined, {
        layoutRefinement: true, layoutPatterns: false,
        externalSignals: ['DATA_0'], requiredExternalSignals: ['DATA_0'],
    });
    assert.deepEqual(result.addedSymbol.filter(c => c.pins[0].signal_name === 'DATA_0')
        .map(c => c.block_name).sort(), ['block_FPGA', 'block_IO']);
});

test('similar signal names do not suppress each other’s forced ports', async () => {
    const owner = component('U1', [[1, 'IO', 'X_DATA'], [2, 'IO', 'DATA']], 'IO');
    const circuit = createPatternFixtureCircuit('similar-names', 'similar-names', [owner]);
    circuit.blocks = [{ name: 'IO', description: '', next_block_names: [] }];
    const result = await autoPlaceCircuitWithHierarchy(circuit, [geometry(owner)], undefined, {
        layoutRefinement: true, layoutPatterns: false,
        externalSignals: ['X_DATA', 'DATA'], requiredExternalSignals: ['X_DATA', 'DATA'],
    });
    assert.deepEqual(result.addedSymbol.map(c => c.pins[0].signal_name).sort(), ['DATA', 'X_DATA']);
});

test('explicit styles create separate ports for one net in one block', async () => {
    const cs = [component('U1', [[1, 'A', 'DATA'], [2, 'B', 'DATA']], 'A'),
        component('U2', [[1, 'A', 'DATA']], 'B')];
    cs[0].pins[0].port_style = 'in';
    cs[0].pins[1].port_style = 'out';
    const circuit = createPatternFixtureCircuit('styled-port', 'styled-port', cs);
    circuit.blocks = ['A', 'B'].map(name => ({ name, description: name, next_block_names: [] }));
    const result = await autoPlaceCircuitWithHierarchy(circuit, cs.map(geometry), undefined,
        { layoutRefinement: false, layoutPatterns: false });
    const styled = result.addedSymbol.filter(c => c.block_name === 'block_A'
        && c.pins[0].signal_name === 'DATA');
    assert.deepEqual(styled.map(c => c.pins[0].port_style).sort(), ['in', 'out']);
    assert(styled.every(c => c.part_uuid === 'b4dd4008fe1a4942b81a1cc59f3de199'));
});

test('power and ground symbols take priority over a port style hint', async () => {
    const cs = [component('U1', [[1, 'VDD', 'VCC'], [2, 'GND', 'GND']], 'A')];
    cs[0].pins[0].port_style = 'in';
    cs[0].pins[1].port_style = 'out';
    const circuit = createPatternFixtureCircuit('supply-style', 'supply-style', cs);
    circuit.blocks = [{ name: 'A', description: 'A', next_block_names: [] }];
    const result = await autoPlaceCircuitWithHierarchy(circuit, cs.map(geometry), undefined,
        { layoutRefinement: false, layoutPatterns: false });
    assert(result.addedSymbol.every(c => c.pins[0].port_style === undefined));
});

test('NC never creates external aliases, pattern connections or ports; physical pins remain', async () => {
    const cs = [component('U1', [[1, 'NC', 'NC'], [2, 'IO', ' nc '], [3, 'IO', 'NC_VALID']], 'A'),
        component('U2', [[1, 'NC', 'NC'], [2, 'IO', ' nc '], [3, 'IO', 'NC_VALID']], 'B')];
    const circuit = createPatternFixtureCircuit('nc', 'nc', cs);
    circuit.blocks = ['A', 'B'].map(name => ({ name, description: name, next_block_names: [] }));
    const symbols = cs.map(geometry), snapshot = structuredClone({ circuit, symbols });
    let checked = false;
    const result = await autoPlaceCircuitWithHierarchy(circuit, symbols, { elkLayoutFinish(graph) {
        checked = true;
        for (const c of cs) for (const pin of [1, 2]) {
            const id = `${c.designator}_pin_${pin}`;
            assert(leaves(graph).some(n => n.ports?.some(p => p.id === id)), 'keep the unused physical pin');
            assert(!graph.edges?.some(e => [...e.sources, ...e.targets].includes(id)), 'NC has no wire');
        }
        return graph;
    } }, { layoutRefinement: true, externalSignals: ['NC', ' nc ', 'NC_VALID'] });
    assert(checked);
    assert(result.addedSymbol.some(c => c.pins[0].signal_name === 'NC_VALID'), 'exact NC only, not a substring');
    assert(result.addedSymbol.every(c => !/^nc$/i.test(c.pins[0].signal_name.trim())));
    assert.deepEqual({ circuit, symbols }, snapshot);
});

test('small LDO and protection chain stay physically connected, including single decoupling capacitors', async () => {
    const cs = [component('J1', [[1, '1', 'VIN_RAW'], [2, '2', 'GND']]),
        component('F1', [[1, '1', 'VIN_RAW'], [2, '2', 'VIN_FUSED']]),
        component('D1', [[1, 'A', 'VIN_FUSED'], [2, 'K', 'VIN_PROT']]),
        component('D2', [[1, 'A', 'GND'], [2, 'K', 'VIN_PROT']]),
        component('U1', [[1, 'IN', 'VIN_PROT'], [2, 'OUT', '3V3'], [3, 'GND', 'GND'], [4, 'NC', 'NC'],
            [5, 'NC', 'NC'], [6, 'NC', 'NC'], [7, 'NC', 'NC']]),
        component('C1', [[1, '1', 'VIN_PROT'], [2, '2', 'GND']]), component('C2', [[1, '1', '3V3'], [2, '2', 'GND']])];
    for (const layoutPatterns of [false, true]) {
        // Inspect physical wires after pattern expansion, without joining
        // disconnected markers by name. Seven IC pins trigger old bank nesting.
        const r = await autoPlaceCircuitWithHierarchy(createPatternFixtureCircuit('ldo', 'ldo', cs), cs.map(geometry), undefined,
            { layoutRefinement: true, layoutPatterns });
        const owner = new Map(r.renderGraph!.children!.flatMap(n => (n.ports ?? []).map(p => [p.id, n.id])));
        const reached = new Set(['U1']);
        for (let pass = 0; pass < r.positioned.length; pass++) for (const e of r.edges) {
            const ids = [...e.sources, ...e.targets].map(p => owner.get(p)!);
            if (ids.some(id => reached.has(id))) ids.forEach(id => reached.add(id));
        }
        for (const c of cs) assert(reached.has(c.designator), `${c.designator}, patterns=${layoutPatterns}`);
    }
});

test('supply localization is per block, preserves series protection and permits large-block decoupling', () => {
    const cs = [component('U1', [[1, 'VIN', 'SYS'], [2, 'VDD', '3V3']]),
        component('F1', [[1, '1', 'VIN_RAW'], [2, '2', 'VIN_FUSED']]),
        component('D1', [[1, 'A', 'VIN_FUSED'], [2, 'K', 'SYS']]),
        component('C1', [[1, '1', '3V3'], [2, '2', 'GND']])];
    const symbols = cs.map(geometry), shared = () => sharedSupplyNets(cs, symbols).get('__v_root__')!;
    assert(shared().has('3V3'));
    symbols[0].symbol.pins.push(...Array.from({ length: 14 }, (_, i) => ({ num: i + 3, name: 'NC', signal_name: '', part: '', x: 0, y: i })));
    assert(!shared().has('3V3'), 'large IC allows local decoupling flags');
    for (const net of ['VIN_RAW', 'VIN_FUSED', 'SYS']) assert(shared().has(net), `keep ${net} series path`);
    assert(!shared().has('GND'), 'ground remains local without aliasing separate ground domains');
    const small = component('C9', [[1, '1', '3V3'], [2, '2', 'GND']], 'small');
    cs.push(small); symbols.push(geometry(small));
    assert(sharedSupplyNets(cs, symbols).get('small')!.has('3V3'), 'large neighbour block does not detach this capacitor');
});

test('sixteen real components permit supply localization while fifteen stay wired', () => {
    const cs = [component('U1', [[1, 'VDD', '3V3']]), component('C1', [[1, '1', '3V3'], [2, '2', 'GND']]),
        ...Array.from({ length: 13 }, (_, i) => component(`R${i + 1}`, [[1, '1', `N${i}`], [2, '2', 'GND']]))];
    const shared = () => sharedSupplyNets(cs, cs.map(geometry)).get('__v_root__')!;
    assert(shared().has('3V3'));
    cs.push(component('R14', [[1, '1', 'N14'], [2, '2', 'GND']]));
    assert(!shared().has('3V3'));
});
