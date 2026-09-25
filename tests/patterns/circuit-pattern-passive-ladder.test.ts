import test from 'node:test';
import assert from 'node:assert/strict';
import { createPatternContext } from '../../src/circuit-layout/patterns/helpers.ts';
import { detectPatternMacros } from '../../src/circuit-layout/patterns/collapse.ts';
import { refinedCircuitLayoutPatterns } from '../../src/circuit-layout/patterns/registry.ts';
import { component, createPatternFixtureCircuit, resistorSymbol } from './helpers.ts';

function fixture(extraTap = false) {
    const cs = [
        component('R1', [[1, 'A', 'IN'], [2, 'B', 'INNER']]),
        component('R2', [[1, 'A', 'INNER'], [2, 'B', 'MID']]),
        component('C1', [[1, 'A', 'IN'], [2, 'B', 'MID']]),
        component('R3', [[1, 'A', 'MID'], [2, 'B', 'LOW']]),
        component('C2', [[1, 'A', 'MID'], [2, 'B', 'LOW']]),
        component('R4', [[1, 'A', 'LOW'], [2, 'B', 'GND']]),
        component('C3', [[1, 'A', 'LOW'], [2, 'B', 'GND']]),
    ];
    if (extraTap) cs.push(component('U1', [[1, 'TAP', 'INNER']], 'other'));
    const circuit = createPatternFixtureCircuit('passive-ladder', 'passive-ladder', cs);
    const symbols = cs.map(resistorSymbol);
    return { circuit, symbols };
}

test('series and parallel arms form one tapped ladder without losing a net', () => {
    const { circuit, symbols } = fixture();
    const found = detectPatternMacros(circuit, symbols, refinedCircuitLayoutPatterns);
    const ladder = found.macros.find(m => m.patternId === 'passive-ladder' && m.absorbedDesignators.length === 7);
    assert(ladder);
    assert.deepEqual(new Set(ladder.absorbedDesignators), new Set(circuit.components.map(c => c.designator)));
    assert(ladder.placements.some(p => p.pins.some(pin => pin.signal_name === 'INNER')));
    assert(ladder.placements.some(p => p.pins.some(pin => pin.signal_name === 'GND')));
});

test('two dividers with shared high, tap and low rails align as one ladder', () => {
    const cs = [component('R1', [[1, 'A', 'VIN'], [2, 'B', 'MID']]),
        component('R2', [[1, 'A', 'VIN'], [2, 'B', 'MID']]),
        component('R3', [[1, 'A', 'VIN'], [2, 'B', 'MID']]),
        component('R4', [[1, 'A', 'MID'], [2, 'B', 'GND']]),
        component('R5', [[1, 'A', 'MID'], [2, 'B', 'GND']])];
    const circuit = createPatternFixtureCircuit('parallel-dividers', 'parallel-dividers', cs);
    const found = detectPatternMacros(circuit, cs.map(resistorSymbol), refinedCircuitLayoutPatterns);
    assert.equal(found.macros[0]?.patternId, 'passive-ladder');
    assert.deepEqual(new Set(found.macros[0].absorbedDesignators), new Set(cs.map(c => c.designator)));
});

test('unequal symbols align adjacent ladder stages by pins and retain body clearance', () => {
    const cs = [component('R1', [[1, 'A', 'VIN'], [2, 'B', 'MID']]),
        component('C1', [[1, 'A', 'VIN'], [2, 'B', 'MID']]),
        component('R2', [[1, 'A', 'MID'], [2, 'B', 'GND']]),
        component('C2', [[1, 'A', 'MID'], [2, 'B', 'GND']])];
    const sizes = new Map<string, [number, number, number]>([
        ['R1', [60, 28, 14]], ['C1', [55, 41, 20.5]],
        ['R2', [75, 42, 27]], ['C2', [90, 30, 9]],
    ]);
    const symbols = cs.map(c => {
        const symbol = resistorSymbol(c);
        const [width, height, pinY] = sizes.get(c.designator)!;
        symbol.symbol.width = width;
        symbol.symbol.height = height;
        symbol.symbol.center = { x: width / 2, y: height / 2 };
        symbol.symbol.pins[0].y = pinY;
        symbol.symbol.pins[1].x = width;
        symbol.symbol.pins[1].y = pinY;
        return symbol;
    });
    const circuit = createPatternFixtureCircuit('unequal-ladder', 'unequal-ladder', cs);
    const found = detectPatternMacros(circuit, symbols, refinedCircuitLayoutPatterns);
    const ladder = found.macros.find(m => m.patternId === 'passive-ladder');
    assert(ladder);
    const placements = new Map(ladder.placements.filter(p => sizes.has(p.designator))
        .map(p => [p.designator, p]));
    const pin = (id: string, number: number) => {
        const placement = placements.get(id)!;
        return { x: placement.x + placement.pins.find(p => p.num === number)!.x,
            y: placement.y + placement.pins.find(p => p.num === number)!.y };
    };
    assert.equal(pin('R1', 2).y, pin('R2', 1).y);
    assert.equal(pin('C1', 2).y, pin('C2', 1).y);
    assert.equal(pin('R1', 1).x, pin('C1', 1).x);
    assert.equal(pin('R2', 1).x, pin('C2', 1).x);
    for (const [first, second] of [['R1', 'C1'], ['R2', 'C2']]) {
        const pair = [placements.get(first)!, placements.get(second)!].sort((a, b) => a.y - b.y);
        const [upper, lower] = pair;
        assert(upper.y + upper.height + 20 <= lower.y);
    }
    assert([...placements.values()].every(p => p.x >= 0 && p.y >= 0));
});

test('an original cross-block tap prevents a false series chain', () => {
    const { circuit, symbols } = fixture(true);
    const local = { ...circuit, components: circuit.components.filter(c => c.block_name !== 'other') };
    const found = detectPatternMacros(local, symbols.filter(s => s.block_name !== 'other'),
        refinedCircuitLayoutPatterns, { circuit });
    assert(!found.macros.some(m => m.patternId === 'passive-ladder'
        && m.absorbedDesignators.includes('R1') && m.absorbedDesignators.includes('R2')));
});

test('capacitors in series keep their private midpoint inside one macro', () => {
    const cs = [component('C1', [[1, 'A', 'LEFT'], [2, 'B', 'PRIVATE']]),
        component('C2', [[1, 'A', 'PRIVATE'], [2, 'B', 'RIGHT']])];
    const circuit = createPatternFixtureCircuit('series-caps', 'series-caps', cs);
    const found = detectPatternMacros(circuit, cs.map(resistorSymbol), refinedCircuitLayoutPatterns);
    const chain = found.macros.find(m => m.patternId === 'passive-ladder');
    assert.deepEqual(new Set(chain?.absorbedDesignators), new Set(['C1', 'C2']));
    assert(chain?.placements.some(p => p.pins.some(pin => pin.signal_name === 'PRIVATE')));
});

test('supply rails limit parallel group turns; ordinary nets permit quarter turns', () => {
    const parallel = refinedCircuitLayoutPatterns.find(p => p.id === 'parallel-two-pin')!;
    for (const [upper, lower] of [['SIGNAL_A', 'GND'], ['VCC', 'SIGNAL_B'], ['SIGNAL_A', 'SIGNAL_B']]) {
        const cs = [component('R1', [[1, 'A', upper], [2, 'B', lower]]),
            component('C1', [[1, 'A', upper], [2, 'B', lower]])];
        const circuit = createPatternFixtureCircuit('parallel', 'parallel', cs);
        const context = createPatternContext(circuit, cs.map(resistorSymbol));
        const match = parallel.findMatches(context)[0];
        assert(match);
        const macro = parallel.instantiate(match, context);
        assert.deepEqual(macro?.refinementRotations,
            upper === 'VCC' || lower === 'GND' ? [180] : [90, 180, 270]);
    }
});
