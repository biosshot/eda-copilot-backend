import test from 'node:test';
import assert from 'node:assert/strict';
import { splitMultiPartComponent } from '../src/circuit-layout/search-many-part-comp.ts';
import type { Circuit } from '../src/types/circuit.ts';
import { getSymbol } from '../src/devices/symbols/symbol-parser.ts';

const symbol = {
    dataStr: '', rect: [0, 0, 10, 10], partIds: ['U.1', 'U.2', 'U.3'],
    pins: [
        { num: 'A1', name: 'A', part: 'U.1', x: 0, y: 0, signal_name: '' },
        { num: 'B1', name: 'B', part: 'U.2', x: 0, y: 0, signal_name: '' },
        { num: 'C1', name: 'C', part: 'U.3', x: 0, y: 0, signal_name: '' },
    ],
};
const loadSymbol = (async () => symbol) as typeof getSymbol;
const makeCircuit = (pinNumber = 'C1') => ({ components: [{
    designator: 'U1', value: 'U', part_uuid: 'test', search_query: 'U', block_name: 'Test',
    pins: [
        { pin_number: 'A1', name: 'A', signal_name: 'A_NET' },
        { pin_number: 'B1', name: 'B', signal_name: '' },
        { pin_number: pinNumber, name: 'C', signal_name: 'C_NET' },
    ],
}] }) as Circuit;

test('multipart sections retain their library indices when an intermediate section is unused', async () => {
    const result = await splitMultiPartComponent(makeCircuit(), loadSymbol);
    assert.deepEqual(result.components.map(component => component.designator), ['U1.1', 'U1.3']);
    assert.deepEqual(result.components.map(component => component.pins.map(pin => pin.pin_number)), [['A1'], ['C1']]);
});

test('multipart split rejects a pin absent from the library symbol', async () => {
    await assert.rejects(splitMultiPartComponent(makeCircuit('H99'), loadSymbol), /U1: pin number "H99" not found/);
});
