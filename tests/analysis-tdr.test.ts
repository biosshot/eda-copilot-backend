import assert from 'node:assert/strict';
import test from 'node:test';
import { createTdrExecutor } from '../src/analyze/analysis-dsl/tdr-executor.ts';
import { createAnalysisApi } from '../src/analyze/analysis-dsl/context.ts';
import { createAnalysisExecutor, type AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { networkResource } from '../src/analyze/analysis-dsl/frequency-model.ts';
import type { createTransientExecutor } from '../src/analyze/analysis-dsl/transient-executor.ts';

const NS = 1e-9;
const limits = { maxEntries: 100, maxOutputBytes: 1e6 };
type Transient = ReturnType<typeof createTransientExecutor>;

/** Fake SI transient: port voltage v(t) = (V/2)(1+rho(t)) on a uniform grid, rho supplied by caller. */
function fakeTransient(rho: (t: number) => number | null, stopS = 5 * NS, stepS = 0.1 * NS): Transient {
  return (async () => {
    const t: number[] = [], v: (number | null)[] = [];
    for (let x = 0; x <= stopS + stepS / 2; x += stepS) { t.push(x); const r = rho(x); v.push(r === null ? null : 0.5 * (1 + r)); }
    return { id: 'fake', status: 'complete', method: 'fake', diagnostics: [], assumptions: [], metrics: {
      'network-voltage:a': { kind: 'series', unit: 'V', axis: { name: 'timeS', unit: 's', values: t }, values: v, validity: { status: 'valid' }, diagnostics: [] },
    } } as never;
  }) as unknown as Transient;
}

const network = (data: object, parameters?: object) => ({ id: 'net', kind: 'network', ports: ['a'], input: { id: 'net', kind: 'network', ports: ['a'], data, ...(parameters ? { parameters } : {}) }, ports: ['a'] });
const portOf = (name: string) => ({ kind: 'port', id: `net:port:${name}`, contextId: 'ctx', input: { owner: 'net', name } });
const oneOrTwoPort = (ports: string[], parameters?: object) => ({ id: 'net', kind: 'network', ports, input: { id: 'net', kind: 'network', ports, data: { format: 'delayed-scattering', referenceTopology: 'common', nature: 'passive', referenceOhm: 50, scattering: [[0, 1], [1, 0]], delayS: 1e-9 }, ...(parameters ? { parameters } : {}) }, ports });
function task(overrides: Record<string, unknown>, descriptors: object[] = [network({ format: 'coupled-rlc', referenceTopology: 'grounded', potentialNodes: ['v'], currentDofs: [], incidence: [], portIncidence: [[0, 0, 1]], resistanceOhm: [], inductanceH: [], capacitanceF: [] }), portOf('a')]) {
  return {
    kind: 'tdr', contextId: 'ctx', config: { target: { kind: 'network', id: 'net', contextId: 'ctx' }, referenceOhm: 50, riseS: 0.5 * NS, termination: 'open', time: { stopS: 5 * NS, sampleStepS: 0.1 * NS }, ...overrides },
    descriptors, variantChanges: [],
  } as unknown as AnalysisTask<'tdr'>;
}
const series = (r: { values?: unknown[] }) => r.values as (number | null)[];

test('constant reflection maps to impedance by Z = Zref(1+rho)/(1-rho) and ramp samples are unavailable', async () => {
  const exec = createTdrExecutor(fakeTransient(() => 0.2));
  const r = await exec(task({}));
  const rho = r.metrics.reflection as unknown as { values: (number | null)[] }, z = r.metrics.impedance as unknown as { values: (number | null)[] };
  assert.equal(rho.values[0], null);
  assert.equal(rho.values[2], null);
  assert.ok(Math.abs(rho.values.at(-1)! - 0.2) < 1e-12);
  assert.ok(Math.abs(z.values.at(-1)! - 75) < 1e-9);
  assert.ok(r.diagnostics.some(d => d.code === 'TDR_RISE_SAMPLES_UNAVAILABLE'));
});

test('rho near +1 marks impedance unavailable instead of infinity', async () => {
  const exec = createTdrExecutor(fakeTransient(() => 1 - 1e-8));
  const r = await exec(task({}));
  assert.equal(series(r.metrics.impedance as never).at(-1), null);
  assert.ok(r.diagnostics.some(d => d.code === 'TDR_IMPEDANCE_UNBOUNDED'));
});

test('distance is unavailable without a declared propagation velocity and never inferred from geometry', async () => {
  const exec = createTdrExecutor(fakeTransient(() => 0));
  const r = await exec(task({}));
  const d = r.metrics.distance as unknown as { validity: { status: string } };
  assert.equal(d.validity.status, 'unavailable');
});

test('distance equals v*t/2 when a delay line declares propagation velocity', async () => {
  const exec = createTdrExecutor(fakeTransient(() => 0));
  const descriptors = [oneOrTwoPort(['a'], { propagationVelocityMps: 2e8 }), portOf('a')];
  const r = await exec(task({ termination: 'open' }, descriptors));
  const d = r.metrics.distance as unknown as { values: number[]; axis: { values: number[] } };
  const i = d.values.length - 1;
  assert.ok(Math.abs(d.values[i]! - 2e8 * d.axis.values[i]! / 2 * 1000) < 1e-6);
});

test('refusals: board/line targets, per-port reference, short far end, invalid target and port count', async () => {
  const exec = createTdrExecutor(fakeTransient(() => 0));
  await assert.rejects(exec(task({ target: { kind: 'line', id: 'l', contextId: 'ctx' } })), { code: 'TDR_BOARD_NOT_IMPLEMENTED' });
  await assert.rejects(exec(task({ target: { kind: 'channel', id: 'c', contextId: 'ctx' } })), { code: 'TDR_BOARD_NOT_IMPLEMENTED' });
  await assert.rejects(exec(task({ referenceOhm: { a: 50 } })), { code: 'TDR_PER_PORT_REFERENCE_NOT_IMPLEMENTED' });
  await assert.rejects(exec(task({ target: { kind: 'resource', id: 'r', contextId: 'ctx' } })), { code: 'TDR_TARGET_NOT_IMPLEMENTED' });
  await assert.rejects(exec(task({ termination: 'short' }, [oneOrTwoPort(['a', 'b']), portOf('a'), { ...portOf('b'), id: 'net:port:b', input: { owner: 'net', name: 'b' } }])), { code: 'TDR_SHORT_TERMINATION_NOT_IMPLEMENTED' });
  await assert.rejects(exec(task({}, [oneOrTwoPort(['a', 'b', 'c'])])), { code: 'TDR_PORT_COUNT_UNSUPPORTED' });
});

const resistorModel = (id: string, R: number) => ({ id, kind: 'network', ports: ['p'], analyses: ['si'], source: 'Authored one-port shunt resistor for TDR tests', data: { format: 'coupled-rlc', referenceTopology: 'grounded', potentialNodes: ['v'], currentDofs: ['r'], incidence: [[0, 0, 1]], portIncidence: [[0, 0, 1]], resistanceOhm: [[0, 0, R]], inductanceH: [], capacitanceF: [] } });
const lineModel = (delayS: number, parameters?: object) => ({ id: 'line', kind: 'network', ports: ['a', 'b'], analyses: ['si'], source: 'Authored ideal 50 Ohm delayed line for TDR tests', data: { format: 'delayed-scattering', referenceTopology: 'common', nature: 'passive', referenceOhm: 50, scattering: [[0, 1], [1, 0]], delayS }, ...(parameters ? { parameters } : {}) });

/** Real native transient path through the public DSL builder. */
async function tdrRun(model: object, termination: unknown, stopNs = 3) {
  const api = createAnalysisApi({}, limits, () => {}, createAnalysisExecutor());
  const a = api.globals.analysis();
  const ports = Object.fromEntries((model as { ports: string[] }).ports.map(p => [p, p]));
  const n = a.network({ model: networkResource(model as never), ports });
  return a.tdr().target(n).referenceOhm(50).riseS(0.1 * NS).termination(termination as never).time({ stopS: stopNs * NS, sampleStepS: 0.01 * NS }).run();
}
const last = (r: { values?: unknown[] }) => (r.values as number[]).at(-1)!;

test('physics: matched resistor gives rho=0, Z=Zref; open gives rho->+1; short gives rho->-1', async () => {
  const matched = await tdrRun(resistorModel('m', 50), 'open');
  assert.ok(Math.abs(last(matched.reflection as never)) < 1e-4);
  assert.ok(Math.abs(last(matched.impedance as never) - 50) < 0.05);
  const open = await tdrRun(resistorModel('o', 1e9), 'open');
  assert.ok(Math.abs(last(open.reflection as never) - 1) < 1e-4);
  assert.equal(last(open.impedance as never), null);
  const short = await tdrRun(resistorModel('s', 1e-6), 'open');
  assert.ok(Math.abs(last(short.reflection as never) + 1) < 1e-4);
});

test('physics: resistor R gives steady rho=(R-Z0)/(R+Z0) and Z=R', async () => {
  const r = await tdrRun(resistorModel('r150', 150), 'open');
  assert.ok(Math.abs(last(r.reflection as never) - 0.5) < 1e-4);
  assert.ok(Math.abs(last(r.impedance as never) - 150) < 0.2);
});

test('physics: matched 50 Ohm delay line keeps rho=0 after the source rise', async () => {
  const r = await tdrRun(lineModel(1 * NS), 'matched', 4);
  const rho = (r.reflection as never as { values: (number | null)[] }).values.filter((v): v is number => typeof v === 'number');
  assert.ok(rho.length > 100);
  assert.ok(Math.max(...rho.map(Math.abs)) < 1e-3);
});

test('physics: open far end of a 50 Ohm delay line shows rho=+1 after the round-trip delay', async () => {
  const r = await tdrRun(lineModel(1 * NS), 'open', 4);
  const t = (r.reflection as never as { axis: { values: number[] } }).axis.values, rho = (r.reflection as never as { values: (number | null)[] }).values;
  const before = rho.filter((_, i) => t[i]! > 0.2 * NS && t[i]! < 1.8 * NS) as number[];
  const after = rho.filter((_, i) => t[i]! > 2.3 * NS) as number[];
  assert.ok(before.length > 10 && after.length > 10);
  assert.ok(Math.max(...before.map(Math.abs)) < 1e-3);
  assert.ok(Math.min(...after) > 1 - 1e-2);
});
