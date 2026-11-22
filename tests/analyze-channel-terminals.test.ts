import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizePlannerTerminals, legacyChannelTerminalRequest, validateChannelTerminalRequest } from '../src/analyze/channel-terminals.ts';

const contact = (sourcePath: string) => ({ sourcePath, layer: 'top' });

test('legacy four contacts normalize to two explicit ports', () => {
  const request = legacyChannelTerminalRequest(['s-in', 's-out', 'r-in', 'r-out'].map(contact));
  assert.deepEqual(request.ports, [
    { name: 'input', positive: 'legacy-0', negative: 'legacy-2' },
    { name: 'output', positive: 'legacy-1', negative: 'legacy-3' },
  ]);
  assert.deepEqual(normalizePlannerTerminals(request).ports, request.ports);
});

test('legacy ports that share a physical reference reuse its terminal', () => {
  const request = legacyChannelTerminalRequest(['s-in', 's-out', 'r', 'r'].map(contact));
  assert.equal(request.references.length, 1);
  assert.equal(request.ports[0].negative, request.ports[1].negative);
});

test('explicit N-port incidence and polarity survive terminal order and caller edits', () => {
  const request = {
    signals: ['a', 'b', 'c'].map(id => ({ id, ...contact(id) })),
    references: [{ id: 'r', ...contact('ref') }],
    ports: [
      { name: 'input', positive: 'a', negative: 'r' },
      { name: 'reversed', positive: 'r', negative: 'b' },
      { name: 'differential', positive: 'b', negative: 'c' },
    ],
  };
  const expectedPorts = structuredClone(request.ports);
  request.signals.reverse();
  const normalized = normalizePlannerTerminals(request);
  request.signals[0].sourcePath = 'changed';
  request.ports[0].positive = 'changed';
  assert.deepEqual(normalized.ports, expectedPorts);
  assert.equal(normalized.contacts.find(t => t.id === 'c')?.sourcePath, 'c');
  assert.deepEqual(normalized.signalIds, new Set(['a', 'b', 'c']));
});

test('invalid, unused and aliased terminal requests fail precisely', () => {
  const signals = [{ id: 's', ...contact('a') }];
  const references = [{ id: 'r', ...contact('b') }];
  const ports = [{ name: 'p', positive: 's', negative: 'r' }];
  assert.throws(() => legacyChannelTerminalRequest([]), /exactly four contacts/);
  assert.throws(() => validateChannelTerminalRequest({ signals, references, ports: [{ ...ports[0], negative: 'x' }] }), /two distinct declared terminals/);
  assert.throws(() => normalizePlannerTerminals({ signals, references, ports: [...ports, { ...ports[0], name: 'alias' }] }), /Duplicate oriented/);
  assert.throws(() => normalizePlannerTerminals({ signals, references: [...references, { id: 'unused', ...contact('c') }], ports }), /unused by any port/);
  assert.throws(() => normalizePlannerTerminals({ signals, references: [{ id: 'r', ...contact('a') }], ports }), /same physical contact/);
});