import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {existsSync,readFileSync} from 'node:fs';
import {lfSha256,planScenario,scenarioSource,stage5Manifest} from '../scripts/verify-p7-stage5-scenario.ts';

const manifest = stage5Manifest();
const board = JSON.parse(readFileSync(manifest.board.path, 'utf8'));
const scenario = (id: string) => manifest.scenarios.find(s => s.id === id)!;

test('stage 5 manifest binds the TPS board, the DSL files, the band token and the closure rows', () => {
  assert.equal(lfSha256(manifest.board.path), manifest.board.sha256);
  const closure = JSON.parse(readFileSync('tests/fixtures/analysis-p7-closure/manifest.json', 'utf8'));
  const rows = new Set<string>(closure.caseTraceability.rows.map((row: { id: string }) => row.id));
  for (const item of manifest.scenarios) {
    assert.ok(existsSync(item.dsl), item.dsl);
    for (const row of (item as any).matrixRows) assert.ok(rows.has(row), row);
    for (const band of item.bandsHz) assert.match(scenarioSource(item, band), item.bandToken ? new RegExp('/\\*BAND_HZ\\*/' + band + '(?![0-9])') : /analysis/);
  }
  assert.deepEqual(scenario('band-step').bandsHz, (manifest as any).stepsHz);
  assert.deepEqual((manifest as any).stepsHz, [1e4, 1e6, 1e8, 1e9]);
});

test('PDN multiport: three distinct physical ports on one rail are planned jointly', async () => {
  const {planned} = await planScenario(board, scenario('pdn-multiport'), 1e6);
  assert.ok(planned, 'production hybrid planner accepts the request');
  assert.equal(planned.ports.length, 3);
  assert.equal(new Set(planned.ports).size, 3);
  assert.equal(planned.attachments, 0);
});

test('coupled pair: two adjacent signal nets give four oriented ports with distinct signal legs', async () => {
  const {planned} = await planScenario(board, scenario('coupled-pair'), 1e6);
  assert.ok(planned);
  assert.deepEqual(planned.ports, ['positive-input', 'positive-output', 'negative-input', 'negative-output']);
});

test('band step: the same two-port channel is planned at every step of the stage 5 ladder', async () => {
  for (const band of scenario('band-step').bandsHz) {
    const {planned} = await planScenario(board, scenario('band-step'), band);
    assert.deepEqual(planned?.ports, ['input', 'output'], String(band));
  }
});

test('stage 6 runner lists the 39 matrix rows and the stage 5 items without running anything', () => {
  const run = (...args: string[]) => execFileSync('python', ['scripts/run-p7-matrix.py', '--dry-run', ...args], {encoding: 'utf8', env: {...process.env, PYTHONUTF8: '1'}});
  const rows = run().split('\n').filter(line => /^(primary|supplemental|return-topology)-/.test(line));
  assert.equal(rows.length, 39);
  const attached = run('--rows', 'supplemental-21');
  assert.match(attached, /verify-p7-stage5-scenario\.ts pdn-multiport/);
  const stage5 = run('--rows', 's5/');
  for (const step of [10000, 1000000, 100000000, 1000000000]) assert.match(stage5, new RegExp('s5/band-step@' + step + '\\b'));
});
