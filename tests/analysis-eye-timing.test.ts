import { renderAnalysisReport } from '../src/analyze.ts';
import assert from 'node:assert/strict';
import test from 'node:test';
import { eyeFromSignal, timingFromSignals } from '../src/analyze/analysis-dsl/timing-eye.ts';
import { resultFor, createAnalysisExecutor, type AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { AnalysisDslError } from '../src/analyze/analysis-dsl/schema.ts';
import type { DataDescription, TimingOptions } from '../src/analyze/analysis-dsl/spec-doc.js';

const NS = 1e-9;
const series = (t: number[], v: (number | null)[]): DataDescription => ({
  kind: 'series', unit: 'V', axis: { name: 'timeS', unit: 's', values: t }, values: v, validity: { status: 'valid' }, diagnostics: [],
});
const port = (id: string) => ({ kind: 'port', id, contextId: 'ctx' });
const opts = (o: object) => o as unknown as TimingOptions;

/** Piecewise-linear waveform on a 0.1 ns grid; breakpoints in ns, values in V. */
function pwl(points: [number, number][], stopNs = 50): DataDescription {
  const interp = (x: number) => {
    for (let i = 1; i < points.length; i++) {
      const [x0, y0] = points[i - 1]!, [x1, y1] = points[i]!;
      if (x <= x1) return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
    }
    return points.at(-1)![1];
  };
  const t: number[] = [], v: number[] = [];
  for (let i = 0; i <= stopNs * 10; i++) { t.push(i / 10 * NS); v.push(interp(i / 10)); }
  return series(t, v);
}

/** NRZ with linear edges of rampFrac UI, both edge directions at every UI boundary. */
function nrz(bits: string, bitRateBps: number, samplesPerUi: number, rampFrac: number, uis: number): DataDescription {
  const ui = 1 / bitRateBps, ramp = rampFrac * samplesPerUi, t: number[] = [], v: number[] = [];
  const level = (k: number) => (bits[((k % bits.length) + bits.length) % bits.length] === '1' ? 1 : 0);
  for (let i = 0; i < uis * samplesPerUi; i++) {
    const k = Math.floor(i / samplesPerUi), p = i % samplesPerUi;
    const cur = level(k), prev = k === 0 ? cur : level(k - 1);
    t.push(i * ui / samplesPerUi);
    v.push(prev + (cur - prev) * Math.min(1, p / ramp));
  }
  return series(t, v);
}

const clockPts: [number, number][] = [[0, 0], [9.9, 0], [10.1, 1], [50, 1]];
const dataPts: [number, number][] = [[0, 0], [7.0, 0], [7.2, 1], [12.0, 1], [12.2, 0], [50, 0]];
const window = { startS: 0, stopS: 50 * NS };
const samplingOpts = (dataPort = 'data', offset = 0) => opts({
  kind: 'sampling', data: [{ port: port(dataPort), lowV: 0.2, highV: 0.8 }],
  clock: { port: port('clk'), thresholdV: 0.5, edges: 'rising', sampleOffsetS: offset }, window,
});
const bySeries = (m: Record<string, DataDescription>) => (p: unknown) => m[(p as { id: string }).id];

test('sampling timing reports setup and hold from interpolated threshold crossings', () => {
  const seriesOf = bySeries({ clk: pwl(clockPts), data: pwl(dataPts) });
  const r = timingFromSignals(samplingOpts(), seriesOf);
  // Clock rises through 0.5 V at 10.0 ns. Data reaches 0.8 V at 7.16 ns and falls back through 0.8 V at 12.04 ns.
  assert.ok(Math.abs(r.events.values![0] as number - 10 * NS) < 1e-13);
  assert.ok(Math.abs((r.setup.values![0] as number) - 2.84 * NS) < 1e-13);
  assert.ok(Math.abs((r.hold.values![0] as number) - 2.04 * NS) < 1e-13);
  assert.equal((r.samples.data as { rows: { level: string }[] }).rows[0]!.level, 'high');
  assert.equal(r.diagnostics.length, 0);
});

test('sampling offset moves the sample instant and therefore the margins', () => {
  const seriesOf = bySeries({ clk: pwl(clockPts), data: pwl(dataPts) });
  const r = timingFromSignals(samplingOpts('data', 0.5 * NS), seriesOf);
  assert.ok(Math.abs((r.setup.values![0] as number) - 3.34 * NS) < 1e-13);
  assert.ok(Math.abs((r.hold.values![0] as number) - 1.54 * NS) < 1e-13);
});

test('sampling between the thresholds is undefined, not a level', () => {
  const seriesOf = bySeries({ clk: pwl(clockPts), data: pwl([[0, 0.5], [50, 0.5]]) });
  const r = timingFromSignals(samplingOpts(), seriesOf);
  assert.equal(r.setup.values![0], null);
  assert.equal(r.setup.validity.status, 'conditional');
  assert.ok(r.diagnostics.some(d => d.code === 'TIMING_UNDEFINED_LEVEL'));
});

test('missing hold transition inside the window stays unavailable', () => {
  const seriesOf = bySeries({ clk: pwl(clockPts), data: pwl([[0, 0], [7, 0], [7.2, 1], [50, 1]]) });
  const r = timingFromSignals(samplingOpts(), seriesOf);
  assert.ok(Math.abs((r.setup.values![0] as number) - 2.84 * NS) < 1e-13);
  assert.equal(r.hold.values![0], null);
  assert.ok(r.diagnostics.some(d => d.code === 'TIMING_NO_TRANSITION'));
});

test('sampling without a clock edge reports a missing-event diagnostic and no number', () => {
  const seriesOf = bySeries({ clk: pwl([[0, 0], [50, 0]]), data: pwl(dataPts) });
  const r = timingFromSignals(samplingOpts(), seriesOf);
  assert.equal(r.setup.validity.status, 'unavailable');
  assert.equal(r.diagnostics[0]!.code, 'TIMING_MISSING_EVENT');
});

const propOpts = (dsts: string[], maxDelayS = 10 * NS) => opts({
  kind: 'propagation', edges: 'rising', maxDelayS, window,
  paths: dsts.map(d => ({ from: port('src'), to: port(d), sourceThresholdV: 0.5, destinationThresholdV: 0.5 })),
});

test('propagation delay, skew and spread from same-edge crossings', () => {
  const seriesOf = bySeries({
    src: pwl(clockPts),
    d1: pwl([[0, 0], [13.4, 0], [13.6, 1], [50, 1]]),
    d2: pwl([[0, 0], [13.9, 0], [14.1, 1], [50, 1]]),
  });
  const r = timingFromSignals(propOpts(['d1', 'd2']), seriesOf);
  assert.ok(Math.abs((r.delay.values![0] as number) - 3.5 * NS) < 1e-13);
  assert.ok(Math.abs((r.delay.values![1] as number) - 4.0 * NS) < 1e-13);
  assert.ok(Math.abs((r.skew.values![0] as number) - 0.5 * NS) < 1e-13);
  assert.ok(Math.abs((r.delaySpread.value as number) - 0.5 * NS) < 1e-13);
  assert.equal(r.setup.validity.status, 'unavailable');
});

test('ambiguous propagation match is unavailable with a diagnostic', () => {
  const seriesOf = bySeries({
    src: pwl(clockPts),
    d1: pwl([[0, 0], [12.9, 0], [13.1, 1], [13.3, 0], [13.5, 0], [13.7, 1], [50, 1]]),
  });
  const r = timingFromSignals(propOpts(['d1']), seriesOf);
  assert.equal(r.delay.values![0], null);
  assert.equal(r.delaySpread.validity.status, 'unavailable');
  assert.ok(r.diagnostics.some(d => d.code === 'TIMING_AMBIGUOUS_MATCH'));
});

test('missing destination edge is a diagnostic, never a zero delay', () => {
  const seriesOf = bySeries({ src: pwl(clockPts), d1: pwl([[0, 0], [50, 0]]) });
  const r = timingFromSignals(propOpts(['d1']), seriesOf);
  assert.equal(r.delay.values![0], null);
  assert.equal(r.delaySpread.validity.status, 'unavailable');
  assert.ok(r.diagnostics.some(d => d.code === 'TIMING_MISSING_EVENT'));
});

test('SiResult.timing reads voltage waveforms through the executor accessor', () => {
  const metrics = { 'voltage:clk': pwl(clockPts), 'voltage:data': pwl(dataPts) } as Record<string, DataDescription>;
  const task = { kind: 'si', contextId: 'ctx', config: {}, descriptors: [], variantChanges: [] } as unknown as AnalysisTask<'si'>;
  const result = resultFor(task, { id: 'x', status: 'complete', method: 'test', diagnostics: [], assumptions: [], metrics }, () => {}, () => { throw new Error('unused'); });
  const timing = result.timing(samplingOpts());
  assert.ok(Math.abs((timing.setup.values![0] as number) - 2.84 * NS) < 1e-13);
  assert.equal(timing.diagnostics.length, 0);
});

const UI = 1 / 1e9;
const eyeInput = (signal: DataDescription, extra: Partial<{ discardS: number; phaseS: number; thresholdV: number }> = {}) => ({
  signal, bitRateBps: 1e9, discardS: 0, phaseS: 0, thresholdV: 0.5, ...extra,
});
const eyeBits = '0011010110';

test('ideal NRZ with 0.2 UI linear edges gives the analytic eye', () => {
  const r = eyeFromSignal(eyeInput(nrz(eyeBits, 1e9, 400, 0.2, 40)));
  assert.equal(r.status, 'complete');
  assert.ok(Math.abs((r.metrics.height.value as number) - 1) < 1e-9, 'rails are 0 V and 1 V in the centre');
  assert.ok(Math.abs((r.metrics.voltageMargin.value as number) - 0.5) < 1e-9);
  // Edge region closes where the opening drops below 50% of height: phase (0.05 UI, 0.15 UI) for a 0.2 UI ramp.
  assert.ok(Math.abs((r.metrics.width.value as number) - 0.9 * UI) < 0.05 * UI, `width ${r.metrics.width.value}`);
  assert.ok(Math.abs((r.metrics.timingMargin.value as number) - 0.35 * UI) < 0.05 * UI, `timingMargin ${r.metrics.timingMargin.value}`);
  assert.equal(r.metrics.diagram.kind, 'eye');
  const report = renderAnalysisReport({ status: 'complete', reports: [{
    contextId: 'eye-fixture', descriptors: [], checks: [], logs: [], plots: [], diagnostics: [],
    summary: { pass: 0, fail: 0, unknown: 0 },
    calculations: [{ phase: 'completed', result: { kind: 'eye', status: r.status,
      diagram: { kind: 'eye', data: r.metrics.diagram.data ?? null } } }],
  }] });
  const image = report.images.find(image => image.kind === 'eye');
  assert.match(image?.svg ?? '', /High maximum/);
  assert.match(image?.svg ?? '', /Phase within the unit interval/);
  assert.equal(r.assumptions.length, 6);
});

test('signal shorter than 2 UI after discardS is refused with a coded error', () => {
  const short = nrz(eyeBits, 1e9, 400, 0.2, 1.5);
  assert.throws(() => eyeFromSignal(eyeInput(short)), (e: unknown) => e instanceof AnalysisDslError && e.code === 'EYE_SIGNAL_TOO_SHORT');
  assert.throws(() => eyeFromSignal(eyeInput(nrz(eyeBits, 1e9, 400, 0.2, 10), { discardS: 9 * UI })), (e: unknown) => e instanceof AnalysisDslError && e.code === 'EYE_SIGNAL_TOO_SHORT');
});

test('missing samples after discardS give unavailable eye metrics', () => {
  const s = nrz(eyeBits, 1e9, 400, 0.2, 10);
  s.values = s.values!.map((v, i) => (i === 500 ? null : v));
  const r = eyeFromSignal(eyeInput(s));
  assert.equal(r.status, 'partial');
  assert.equal(r.metrics.height.validity.status, 'unavailable');
  assert.ok(r.diagnostics.some(d => d.code === 'EYE_MISSING_SAMPLES'));
});

test('eye task is dispatched by the production executor', async () => {
  const executor = createAnalysisExecutor();
  const task = { kind: 'eye', contextId: 'ctx', config: eyeInput(nrz(eyeBits, 1e9, 400, 0.2, 10)), descriptors: [], variantChanges: [] } as unknown as AnalysisTask<'eye'>;
  const payload = await executor(task);
  assert.equal(payload.status, 'complete');
  assert.ok(Math.abs((payload.metrics.height!.value as number) - 1) < 1e-9);
});
