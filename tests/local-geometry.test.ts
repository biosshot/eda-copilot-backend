import assert from 'node:assert/strict';
import test from 'node:test';
import { createSymbolResolver, circuitToSymbols } from '../src/devices/symbols/symbol-parser.ts';
import { requireResolvedComponentFootprint } from '../src/pcb-layout/footprints.ts';
import { extractCircuit } from '../src/schematic.ts';
import { installEasyEdaFixture, PART_UUID, schematicInput } from './fixtures/api-fixtures.mjs';

const localId = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const unknownId = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const symbol = { width: 40, height: 20, center: { x: 20, y: 10 }, pins: [
  { num: '1', name: 'A', x: 0, y: 10, signal_name: '', part: '1' },
  { num: '2', name: 'B', x: 40, y: 10, signal_name: '', part: '1' },
] };
const localFootprint = { name: 'LOCAL', width: 2, height: 1, pads: [] };

test('catalogue wins, local geometry fills misses, and inputs remain unchanged', async () => {
  const fixture = installEasyEdaFixture();
  const original = globalThis.fetch;
  globalThis.fetch = async (input, init) => String(input).includes(localId) || String(input).includes(unknownId)
    ? Response.json({ success: false }, { status: 404 }) : original(input, init);
  try {
    const symbols = { [PART_UUID]: symbol, [localId]: symbol };
    const before = structuredClone(symbols);
    const resolve = createSymbolResolver(symbols);
    assert.notEqual((await resolve(PART_UUID))!.pins[0].name, 'A');
    assert.equal((await resolve(localId))!.pins[0].name, 'A');
    const component = { ...schematicInput.circuit.add_components[0], part_uuid: localId };
    await circuitToSymbols({ components: [component] }, resolve);
    assert.deepEqual(symbols, before);
    await assert.rejects(circuitToSymbols({ components: [{ ...component, part_uuid: unknownId }] }, resolve), /Missing symbol/);
    const input = structuredClone(schematicInput);
    input.circuit.add_components = [{ ...component }];
    const extraction = { ...input, symbols };
    const snapshot = structuredClone(extraction);
    assert.equal((await extractCircuit(extraction)).circuit.components[0].designator, 'R1');
    assert.deepEqual(extraction, snapshot);
    assert.notEqual((await requireResolvedComponentFootprint({ ...component, part_uuid: PART_UUID }, new Map(), { [PART_UUID]: localFootprint })).name, 'LOCAL');
    assert.equal((await requireResolvedComponentFootprint(component, new Map(), { [localId]: localFootprint })).name, 'LOCAL');
    await assert.rejects(requireResolvedComponentFootprint({ ...component, part_uuid: unknownId }), /Missing real footprint/);
  } finally { globalThis.fetch = original; fixture.restore(); }
});
