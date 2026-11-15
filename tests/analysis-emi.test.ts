import assert from 'node:assert/strict';
import test from 'node:test';
import { createEmiExecutor } from '../src/analyze/analysis-dsl/emi-executor.ts';
import { sampleWaveform } from '../src/analyze/analysis-dsl/descriptions.ts';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createAnalysisExecutor } from '../src/analyze/analysis-dsl/executor.ts';
import { createAnalysisApi } from '../src/analyze/analysis-dsl/context.ts';
import { networkResource } from '../src/analyze/analysis-dsl/frequency-model.ts';
import type { createTransientExecutor } from '../src/analyze/analysis-dsl/transient-executor.ts';

const NS = 1e-9;
type Transient = ReturnType<typeof createTransientExecutor>;
type Gain = Record<string, Record<string, number>>; // output port -> driven port -> linear gain
type Spectrum = { frequencyHz: number[]; amplitudeV: number[] };

/** Fake SI transient: each output port is the linear superposition of driven waveforms times a gain.
 * It stands in for the native solver, so these tests cover the EMI pipeline (rewrite, resample, window, detector). */
function fakeTransient(gain: Gain, times: (n: number) => number = n => n * NS, count = 1000): Transient & { calls: unknown[] } {
  const calls: unknown[] = [];
  const run = (async (siTask: { config: { drivers: { port: { id: string }; stimulus: { voltageV: Parameters<typeof sampleWaveform>[0] } }[] } }) => {
    calls.push(siTask);
    const drivers = siTask.config.drivers;
    const t = Array.from({ length: count }, (_, n) => times(n));
    const metrics: Record<string, unknown> = {};
    for (const out of Object.keys(gain)) {
      const values = t.map(x => drivers.reduce((sum, d) => sum + (gain[out]?.[d.port.id.split(':').at(-1)!] ?? 0) * sampleWaveform(d.stimulus.voltageV, x), 0));
      metrics[`network-voltage:${out}`] = { kind: 'series', unit: 'V', axis: { name: 'timeS', unit: 's', values: t }, values, validity: { status: 'valid' }, diagnostics: [] };
    }
    return { id: 'fake', status: 'complete', method: 'fake', diagnostics: [], assumptions: [], metrics };
  }) as unknown as Transient & { calls: unknown[] };
  run.calls = calls;
  return run;
}

const netDescriptor = { id: 'net', kind: 'network', ports: ['a', 'b'], input: { id: 'net', kind: 'network', ports: ['a', 'b'], data: { format: 'coupled-rlc' } }, ports: ['a', 'b'] };
const portDescriptor = (name: string) => ({ kind: 'port', id: `net:port:${name}`, contextId: 'ctx', input: { owner: 'net', name } });
const handle = (name: string) => ({ kind: 'port', id: `net:port:${name}`, contextId: 'ctx' });
const sine = (f: number, amplitude: number, phaseDeg = 0) => ({ kind: 'sine', offset: 0, amplitude, periodS: 1 / f, phaseDeg });
const BASE_DESCRIPTORS = [netDescriptor, portDescriptor('a'), portDescriptor('b')];

/** Default: 50 MHz, 1 V sine on port a, 50 Ohm source, measured at b, 1000 samples at 1 ns, Hann, 1 MHz..200 MHz. */
function emiTask(config: Record<string, unknown> = {}, descriptors: object[] = BASE_DESCRIPTORS): AnalysisTask<'emi'> {
  return {
    kind: 'emi', contextId: 'ctx', variantChanges: [], descriptors,
    config: {
      kind: 'conducted',
      excitations: [{ port: handle('a'), voltageV: sine(50e6, 1), outputResistanceOhm: 50 }],
      measurement: { port: handle('b'), quantity: 'voltage', detector: 'peak' },
      time: { stopS: 999 * NS, sampleStepS: NS },
      spectrum: { discardS: 0, window: 'hann', startHz: 1e6, stopHz: 200e6 },
      ...config,
    },
  } as unknown as AnalysisTask<'emi'>;
}

const scalar = (r: { metrics: Record<string, unknown> }, key: string) => (r.metrics[key] as { value: number }).value;
const spectrumOf = (r: { metrics: Record<string, unknown> }, id: string) => (r.metrics.spectra as unknown as { data: Record<string, Spectrum> }).data[id]!;
const amp = (s: Spectrum, hz: number) => s.amplitudeV[s.frequencyHz.findIndex(f => Math.abs(f - hz) < 1)]!;

test('divider: a 50 MHz sine through a 50 Ohm source into a 50 Ohm shunt reads one line at f with amplitude A*k (Hann on-bin, 1%)', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 0.5 }, b: { a: 0.5 } }));
  const r = await exec(emiTask());
  assert.ok(Math.abs(scalar(r, 'measurement:"b"') - 0.5) < 0.005);
  const s = spectrumOf(r, 'b');
  assert.ok(Math.abs(amp(s, 50e6) - 0.5) / 0.5 < 0.01);
  // Hann main lobe spreads an on-bin tone into k+-1 at half amplitude; bins two or more away are empty.
  assert.ok(Math.abs(amp(s, 52e6)) < 1e-9);
  assert.ok(Math.abs(amp(s, 48e6)) < 1e-9);
});

test('rewrite: the excitation becomes a linear-driver with the explicit source resistance and waveform stimulus', async () => {
  const fake = fakeTransient({ a: { a: 1 }, b: { a: 1 } });
  await createEmiExecutor(fake)(emiTask());
  const sent = fake.calls[0] as { kind: string; config: { drivers: { port: { id: string }; model: { kind: string; outputResistanceOhm: number }; stimulus: { kind: string } }[]; network: { id: string } } };
  assert.equal(sent.kind, 'si');
  assert.equal(sent.config.network.id, 'net');
  assert.equal(sent.config.drivers[0]!.model.kind, 'linear-driver');
  assert.equal(sent.config.drivers[0]!.model.outputResistanceOhm, 50);
  assert.equal(sent.config.drivers[0]!.stimulus.kind, 'waveform');
});

test('two sources at different frequencies give two lines with their own amplitudes and no double counting', async () => {
  // Port a is driven at 50 MHz (1 V); port b at 100 MHz (0.5 V); measurement at a sees 0.5 times b.
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1, b: 0.5 }, b: { a: 0, b: 1 } }));
  const r = await exec(emiTask({
    excitations: [{ port: handle('a'), voltageV: sine(50e6, 1), outputResistanceOhm: 50 }, { port: handle('b'), voltageV: sine(100e6, 0.5), outputResistanceOhm: 50 }],
    measurement: { port: handle('a'), quantity: 'voltage', detector: 'peak' },
  }));
  const s = spectrumOf(r, 'a');
  assert.ok(Math.abs(amp(s, 50e6) - 1) < 0.01);
  assert.ok(Math.abs(amp(s, 100e6) - 0.25) < 0.0025);
  assert.ok(Math.abs(amp(s, 75e6)) < 1e-9);
});

test('a zero-amplitude source gives zero spectrum and zero peak', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 1 } }));
  const r = await exec(emiTask({ excitations: [{ port: handle('a'), voltageV: sine(50e6, 0), outputResistanceOhm: 50 }] }));
  assert.equal(scalar(r, 'measurement:"b"'), 0);
  assert.ok(spectrumOf(r, 'b').amplitudeV.every(a => a === 0));
});

test('phase change does not change the spectral amplitude', async () => {
  const at = async (phaseDeg: number) => {
    const r = await createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 0.5 } }))(emiTask({ excitations: [{ port: handle('a'), voltageV: sine(50e6, 1, phaseDeg), outputResistanceOhm: 50 }] }));
    return amp(spectrumOf(r, 'b'), 50e6);
  };
  const p0 = await at(0), p90 = await at(90), p37 = await at(37);
  assert.ok(Math.abs(p0 - p90) < 1e-9);
  assert.ok(Math.abs(p0 - p37) < 1e-9);
});

test('coupling kind uses the same network path and reproduces the conducted reading', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 0.5 } }));
  const conducted = await exec(emiTask());
  const coupling = await exec(emiTask({ kind: 'coupling' }));
  assert.equal(scalar(coupling, 'measurement:"b"'), scalar(conducted, 'measurement:"b"'));
});

test('non-uniform transient samples are resampled to uniform 1 ns by linear interpolation (10 MHz tone, 2% tolerance)', async () => {
  // Jittered native axis: odd samples shifted by 0.3 ns; the uniform grid is 1 ns.
  const jitter = (n: number) => n * NS + (n % 2) * 0.3 * NS;
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 1 } }, jitter, 1000));
  const r = await exec(emiTask({
    excitations: [{ port: handle('a'), voltageV: sine(10e6, 1), outputResistanceOhm: 50 }],
    time: { stopS: 999 * NS, sampleStepS: NS },
  }));
  assert.ok(Math.abs(amp(spectrumOf(r, 'b'), 10e6) - 1) < 0.02);
  assert.ok(r.assumptions.some(a => a.includes('resampled')));
});

test('peak detector is sqrt(2) times the rms of a single on-bin tone', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 1 } }));
  const peak = await exec(emiTask({ measurement: { port: handle('b'), quantity: 'voltage', detector: 'peak' } }));
  const rms = await exec(emiTask({ measurement: { port: handle('b'), quantity: 'voltage', detector: 'rms' } }));
  const p = scalar(peak, 'measurement:"b"'), q = scalar(rms, 'measurement:"b"');
  assert.ok(p >= q);
  assert.ok(Math.abs(p / q - Math.SQRT2) < 1e-6);
  assert.ok(Math.abs(p - 1) < 0.01);
});

test('bandwidth splits the range into bands: peak per band and no band is lower than its own lines', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 1 } }));
  const r = await exec(emiTask({ measurement: { port: handle('b'), quantity: 'voltage', detector: 'peak', bandwidthHz: 25e6 } }));
  const m = r.metrics['measurement:"b"'] as unknown as { kind: string; axis: { values: number[] }; values: (number | null)[] };
  assert.equal(m.kind, 'series');
  assert.equal(m.values.length, 8); // 1 MHz..200 MHz in 25 MHz bands
  assert.ok(Math.abs(Math.max(...m.values as number[]) - 1) < 0.01);
  assert.ok(Math.abs(m.values[0]!) < 1e-9); // 1..26 MHz holds no line
  assert.ok(Math.abs(m.values[1]! - 1) < 0.01); // 26..51 MHz holds the 50 MHz line
});

test('refusals: risk/board, empty excitations, current and spectrum excitations, missing source resistance, frequency-only, unknown port', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 1 } }));
  await assert.rejects(exec(emiTask({ kind: 'risk' })), { code: 'EMI_BOARD_NOT_IMPLEMENTED' });
  await assert.rejects(exec({ ...emiTask(), board: {} } as unknown as AnalysisTask<'emi'>), { code: 'EMI_BOARD_NOT_IMPLEMENTED' });
  await assert.rejects(exec(emiTask({ excitations: [] })), { code: 'EMI_EXCITATION_REQUIRED' });
  await assert.rejects(exec(emiTask({ excitations: [{ port: handle('a'), currentA: 1e-3, outputResistanceOhm: 50 }] })), { code: 'EMI_CURRENT_EXCITATION_NOT_IMPLEMENTED' });
  await assert.rejects(exec(emiTask({ excitations: [{ port: handle('a'), voltageSpectrum: { frequencyHz: [1e6], values: [{ real: 1, imaginary: 0 }], unit: 'V' } }] })), { code: 'EMI_SPECTRUM_EXCITATION_NOT_IMPLEMENTED' });
  await assert.rejects(exec(emiTask({ excitations: [{ port: handle('a'), voltageV: sine(50e6, 1) }] })), { code: 'EMI_SOURCE_RESISTANCE_REQUIRED' });
  await assert.rejects(exec(emiTask({ time: undefined })), { code: 'EMI_FREQUENCY_DOMAIN_NOT_IMPLEMENTED' });
  await assert.rejects(exec(emiTask({ measurement: { port: handle('b'), quantity: 'current' } })), { code: 'EMI_CURRENT_MEASUREMENT_NOT_IMPLEMENTED' });
  await assert.rejects(exec(emiTask({ excitations: [{ port: { kind: 'port', id: 'nope:port:x', contextId: 'ctx' }, voltageV: 1, outputResistanceOhm: 50 }] })), { code: 'EMI_PORT_UNKNOWN' });
});

test('refusals: spectrum above Nyquist and below 1/T', async () => {
  const exec = createEmiExecutor(fakeTransient({ a: { a: 1 }, b: { a: 1 } }));
  // Nyquist for 1 ns sampling is 500 MHz.
  await assert.rejects(exec(emiTask({ spectrum: { discardS: 0, window: 'hann', startHz: 1e6, stopHz: 600e6 } })), { code: 'EMI_SPECTRUM_ABOVE_NYQUIST' });
  // 1/T for 1000 samples at 1 ns is 1 MHz; a 100 kHz start is below resolution.
  await assert.rejects(exec(emiTask({ spectrum: { discardS: 0, window: 'hann', startHz: 1e5, stopHz: 200e6 } })), { code: 'EMI_SPECTRUM_BELOW_RESOLUTION' });
});

test('EMI without board, circuit or standalone network fails prepare with MISSING_MODEL', async () => {
  const api = createAnalysisApi({}, { maxEntries: 100, maxOutputBytes: 1e6 }, () => {}, createAnalysisExecutor());
  const a = api.globals.analysis();
  const p = await a.emi().kind('conducted').time({ stopS: 999 * NS, sampleStepS: NS }).spectrum({ discardS: 0, window: 'hann', startHz: 1e6, stopHz: 200e6 }).prepare();
  assert.equal(p.structurallyValid, false);
  assert.ok(p.diagnostics.some(d => d.code === 'MISSING_MODEL'));
});

/** Real native transient path through the public DSL builder: one shunt resistor R on port p (same model as the TDR tests). */
const shuntResistor = (R: number) => ({ id: 'r', kind: 'network', ports: ['p'], analyses: ['si'], source: 'Authored one-port shunt resistor for EMI tests', data: { format: 'coupled-rlc', referenceTopology: 'grounded', potentialNodes: ['v'], currentDofs: ['r'], incidence: [[0, 0, 1]], portIncidence: [[0, 0, 1]], resistanceOhm: [[0, 0, R]], inductanceH: [], capacitanceF: [] } });
async function emiReal(R: number, amplitudeV: number, f: number) {
  const api = createAnalysisApi({}, { maxEntries: 100, maxOutputBytes: 1e6 }, () => {}, createAnalysisExecutor());
  const a = api.globals.analysis();
  const n = a.network({ model: networkResource(shuntResistor(R) as never), ports: { p: 'p' } });
  return a.emi().kind('conducted')
    .excitations([{ port: n.port('p'), voltageV: sine(f, amplitudeV), outputResistanceOhm: 50 }])
    .measurement({ port: n.port('p'), quantity: 'voltage', detector: 'peak' })
    .time({ stopS: 999 * NS, sampleStepS: NS })
    .spectrum({ discardS: 0, window: 'hann', startHz: 1e6, stopHz: 200e6 })
    .run();
}

test('real solver: 50 MHz sine through 50 Ohm source into 50 Ohm shunt reads A*k = 0.5 V peak line (2%)', async () => {
  const r = await emiReal(50, 1, 50e6);
  const peak = (r.measurement('p') as unknown as { value: number }).value;
  assert.ok(Math.abs(peak - 0.5) / 0.5 < 0.02, `peak ${peak}`);
});

test('real solver: zero-amplitude source gives zero peak', async () => {
  const r = await emiReal(50, 0, 50e6);
  assert.equal((r.measurement('p') as unknown as { value: number }).value, 0);
});
