import assert from 'node:assert/strict';
import test from 'node:test';
import { resistorPullBankPattern } from '../../src/circuit-layout/patterns/catalog/resistor-pull-bank.ts';
import { detectPatternMacros, routeMacroInternals } from '../../src/circuit-layout/patterns/index.ts';
import { refinedCircuitLayoutPatterns } from '../../src/circuit-layout/patterns/registry.ts';
import { autoPlaceCircuitWithHierarchy } from '../../src/circuit-layout/index.ts';
import { localGroups, orientations } from '../../src/circuit-layout/refinement/groups.ts';
import { rotateSymbolGeometry } from '../../src/circuit-layout/patterns/helpers.ts';
import type { SymbolWithMeta } from '../../src/types/symbol.ts';
import { assertExpandedLayout, assertMacroRoutes, assertPatternRoutesAvoidBodies,
    component, createPatternFixtureCircuit, resistorSymbol, writePatternArtifacts, type PatternFixture } from './helpers.ts';

function fixture(count = 4, common = 'GND', rotation = 0): PatternFixture {
    const resistors = Array.from({ length: count }, (_, i) => component(`R${i + 1}`,
        i % 2 ? [[2, '2', common], [1, '1', `IO_${i}`]] : [[1, '1', `IO_${i}`], [2, '2', common]]));
    resistors.forEach((r, i) => { r.value = i % 2 ? '10k' : '47k'; });
    // Deliberately scramble connector order relative to resistor designators.
    const connector = component('J1', Array.from({ length: count }, (_, i) => [i + 1, `${i + 1}`, `IO_${count - 1 - i}`]));
    const connectorSymbol: SymbolWithMeta = { designator: 'J1', block_name: connector.block_name,
        symbol: { width: 80, height: count * 48 + 40, center: { x: 40, y: 40 },
            pins: connector.pins.map((p, i) => ({ num: p.pin_number, name: p.name, signal_name: p.signal_name,
                part: '', x: 80, y: 20 + i * 48 })) } };
    connectorSymbol.symbol = rotateSymbolGeometry(connectorSymbol.symbol, rotation);
    return { circuit: createPatternFixtureCircuit('pull-bank', 'connector pull resistors', [...resistors, connector]),
        symbols: [...resistors.map(resistorSymbol), connectorSymbol] };
}

test('requires four resistors, excludes capacitors, NC, duplicate branches and other blocks', () => {
    assert.equal(detectPatternMacros(fixture(3).circuit, fixture(3).symbols, [resistorPullBankPattern]).macros.length, 0);
    for (const change of ['capacitor', 'nc', 'duplicate', 'block', 'oversized']) {
        const f = fixture(), c = f.circuit.components[0], s = f.symbols[0];
        if (change === 'capacitor') c.designator = s.designator = 'C1';
        if (change === 'nc') c.pins[0].signal_name = s.symbol.pins[0].signal_name = ' nc ';
        if (change === 'duplicate') c.pins[0].signal_name = s.symbol.pins[0].signal_name = 'IO_1';
        if (change === 'block') c.block_name = s.block_name = 'other';
        if (change === 'oversized') { s.symbol.width *= 3; s.symbol.pins[1].x *= 3; }
        assert.equal(detectPatternMacros(f.circuit, f.symbols, [resistorPullBankPattern]).macros.length, 0, change);
    }
});

test('functional divider takes priority and a bypass capacitor stays outside the pull bank', () => {
    const f = fixture(4, '3V3');
    const extras = [component('R10', [[1, '1', '3V3'], [2, '2', 'FB']]),
        component('R11', [[1, '1', 'FB'], [2, '2', 'GND']]),
        component('C1', [[1, '1', '3V3'], [2, '2', 'GND']])];
    f.circuit.components.push(...extras); f.symbols.push(...extras.map(resistorSymbol));
    const macros = detectPatternMacros(f.circuit, f.symbols, refinedCircuitLayoutPatterns).macros;
    assert(macros.some(m => m.patternId === 'voltage-divider'
        && m.absorbedDesignators.includes('R10') && m.absorbedDesignators.includes('R11')));
    assert.deepEqual(macros.find(m => m.patternId === resistorPullBankPattern.id)?.absorbedDesignators, ['R1', 'R2', 'R3', 'R4']);
});

test('a pull bank does not force boundary ports at a dense multipart neighbour in another block', async () => {
    const f = fixture(5, 'DDR_VTT');
    const neighbour = f.circuit.components.at(-1)!;
    neighbour.designator = 'U3.5'; neighbour.block_name = 'FPGA';
    neighbour.pins.forEach(p => { p.port_style = 'out'; });
    const symbol = f.symbols.at(-1)!;
    symbol.designator = neighbour.designator; symbol.block_name = neighbour.block_name;
    symbol.symbol.pins.forEach(p => { p.port_style = 'out'; });
    f.circuit.blocks.push({ name: 'FPGA', description: '', next_block_names: [] });
    const result = await autoPlaceCircuitWithHierarchy(f.circuit, f.symbols, undefined, { layoutRefinement: true });
    assert(!result.addedSymbol.some(c => c.pins.some(p => p.signal_name.startsWith('IO_'))));
    const expected = f.circuit.components.flatMap(c => c.pins.filter(p => p.signal_name.startsWith('IO_'))
        .map(p => `${c.designator}_pin_${p.pin_number}`));
    assert.deepEqual(new Set(result.clientManagedLabels?.map(label => label.pinId)), new Set(expected));
});

for (const common of ['GND', '3V3']) for (const rotation of [0, 90, 180, 270]) {
    test(`four connector pulls to ${common}, connector orientation ${rotation}: direct wires and rotatable bank`, async () => {
        const f = fixture(4, common, rotation), snapshot = structuredClone(f);
        const macro = detectPatternMacros(f.circuit, f.symbols, refinedCircuitLayoutPatterns).macros.find(m => m.patternId === resistorPullBankPattern.id)!;
        assert(macro);
        assert.equal(macro.layoutChildBlock, undefined);
        assert.equal(macro.forceBoundaryPorts, false);
        assert.deepEqual(macro.refinementRotations, [90, 180, 270]);
        assert(macro.placements.every(p => !p.generatedComponent));
        // Pin order along the branch face follows the connector, not R numbering.
        const connector = f.symbols.at(-1)!.symbol;
        const coordinate = rotation % 180 ? 'x' : 'y';
        const branchPorts = macro.ports.filter(p => p.key !== 'COMMON').sort((a, b) => a[coordinate] - b[coordinate]);
        assert.deepEqual(branchPorts.map(p => p.signalName), [...connector.pins].sort((a, b) => a[coordinate] - b[coordinate]).map(p => p.signal_name));
        macro.routedPaths = await routeMacroInternals(macro);
        assertMacroRoutes(macro);
        const nodes = macro.placements.map(p => ({ id: p.designator, x: p.x, y: p.y, width: p.width, height: p.height,
            center: p.center, rotation: p.rotate, ports: p.pins.map(pin => ({ id: pin.id, x: pin.x, y: pin.y })) }));
        const groups = localGroups(nodes, [], f.circuit.components, [], [macro]);
        const group = groups.find(g => g.ids.includes('R1'))!;
        assert(!group.ids.includes('J1'));
        assert.equal(orientations(group, nodes, f.symbols).length, 4);
        const result = await autoPlaceCircuitWithHierarchy(f.circuit, f.symbols, undefined, { layoutRefinement: true });
        assertExpandedLayout(f, result);
        assertPatternRoutesAvoidBodies(result, resistorPullBankPattern.id);
        assert(!result.addedSymbol.some(c => c.pins.some(p => p.signal_name.startsWith('IO_'))), 'branches stay wired');
        for (let i = 0; i < 4; i++) {
            const resistor = `R${i + 1}_pin_1`, terminal = `J1_pin_${4 - i}`;
            assert(result.edges.some(e => [...e.sources, ...e.targets].includes(resistor)
                && [...e.sources, ...e.targets].includes(terminal)), `${resistor} connects directly to ${terminal}`);
        }
        assert.deepEqual(f, snapshot);
    });
}

function ddrFixture(): PatternFixture {
    // Portable scope: R47/R48/R49/R50/R51 and the corresponding W631GG6MB pins.
    // Keep the DDR memory and termination bank in one functional block to exercise direct routing.
    const signals = ['DDR_A11', 'DDR_A12', 'DDR_BA0', 'DDR_BA1', 'DDR_BA2'];
    const refs = ['R47', 'R48', 'R49', 'R50', 'R51'];
    const pins = ['R7', 'N7', 'M2', 'N8', 'M3'];
    const rs = refs.map((ref, i) => ({ ...component(ref, [[1, '1', signals[i]], [2, '2', 'DDR_VTT']]), value: '49.9R' }));
    const memory = component('U17', []);
    memory.pins = signals.map((signal_name, i) => ({ pin_number: pins[i], name: signal_name.slice(4), signal_name }));
    const supply = component('J2', [[1, 'VTT', 'DDR_VTT']]);
    const cs = [...rs, memory, supply];
    const symbols: SymbolWithMeta[] = [...rs.map(resistorSymbol), ...[memory, supply].map(c => ({ designator: c.designator,
        block_name: c.block_name, symbol: { width: 100, height: 320, center: { x: 50, y: 160 }, pins: c.pins.map((p, i) => ({
            num: p.pin_number, name: p.name, signal_name: p.signal_name, part: '', x: 0, y: 30 + i * 48 })) } }))];
    return { circuit: createPatternFixtureCircuit('ddr-termination', 'DDR3 subset', cs), symbols };
}

test('DDR3 termination excerpt preserves real signal names and pin numbers with inline wires', async () => {
    const f = ddrFixture();
    const signals = ['DDR_A11', 'DDR_A12', 'DDR_BA0', 'DDR_BA1', 'DDR_BA2'];
    const refs = ['R47', 'R48', 'R49', 'R50', 'R51'];
    const pins = ['R7', 'N7', 'M2', 'N8', 'M3'];
    const result = await autoPlaceCircuitWithHierarchy(f.circuit, f.symbols, undefined, { layoutRefinement: true });
    assertExpandedLayout(f, result);
    assertPatternRoutesAvoidBodies(result, resistorPullBankPattern.id);
    assert(!result.addedSymbol.some(c => c.pins.some(p => signals.includes(p.signal_name))));
    for (let i = 0; i < refs.length; i++) assert(result.edges.some(e =>
        [...e.sources, ...e.targets].includes(`${refs[i]}_pin_1`)
        && [...e.sources, ...e.targets].includes(`U17_pin_${pins[i]}`)));
});

test('saves viewable schematics for the connector and DDR3 cases', async () => {
    for (const [name, f] of [
        ['resistor-pull-bank-connector', fixture(4, '3V3', 270)],
        ['resistor-pull-bank-ddr3', ddrFixture()],
    ] as const) {
        const { withPattern } = await writePatternArtifacts(name, f,
            { patternCatalog: [resistorPullBankPattern] });
        assertExpandedLayout(f, withPattern);
    }
});
