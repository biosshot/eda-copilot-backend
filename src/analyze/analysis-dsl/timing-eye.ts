import type { DataDescription, Diagnostic, Json, TimingOptions, Validity } from './spec-doc.js';
import { AnalysisDslError } from './schema.ts';

/** Pure post-processing of already-produced time-voltage waveforms (P11 v1).
 * No solver, no native code. Missing events, ambiguous matches, window-limited
 * intervals and missing samples are reported as diagnostics and unavailable
 * values, never as zero or as a fabricated number. */

type Edge = 'rising' | 'falling';
interface Crossing { timeS: number; edge: Edge }
interface Signal { t: readonly number[]; v: readonly (number | null)[]; validity: Validity; unit: string }

export interface TimingSignals {
  samples: DataDescription; events: DataDescription; setup: DataDescription; hold: DataDescription;
  delay: DataDescription; skew: DataDescription; delaySpread: DataDescription; diagnostics: Diagnostic[];
}

const note = (code: string, message: string): Diagnostic => ({ code, severity: 'warning', message });

function unavailableSeries(unit: string, reason: string, code = 'DATA_UNAVAILABLE'): DataDescription {
  return { kind: 'series', unit, axis: { name: 'timeS', unit: 's', values: [] }, validity: { status: 'unavailable', reason }, diagnostics: [note(code, reason)] };
}
function unavailableScalar(unit: string, reason: string, code = 'DATA_UNAVAILABLE'): DataDescription {
  return { kind: 'scalar', unit, validity: { status: 'unavailable', reason }, diagnostics: [note(code, reason)] };
}

function signal(d: DataDescription | undefined, label: string): Signal | string {
  if (!d) return `${label} is missing`;
  if (d.validity.status === 'unavailable') return `${label} unavailable: ${d.validity.reason ?? 'no reason given'}`;
  if (d.kind !== 'series' || d.axis?.name !== 'timeS' || !d.values) return `${label} must be a timeS series`;
  const t = d.axis.values;
  if (t.length !== d.values.length || t.length < 2 || t.some((x, i) => i > 0 && !(x > t[i - 1]!))) return `${label} needs at least two strictly increasing samples`;
  return { t, v: d.values.map(x => (typeof x === 'number' ? x : null)), validity: d.validity, unit: d.unit };
}

function combineValidity(signals: readonly Signal[], partial: boolean): Validity {
  const conditional = signals.find(s => s.validity.status === 'conditional');
  if (conditional) return { status: 'conditional', reason: conditional.validity.reason ?? 'Conditional input signal' };
  return partial ? { status: 'conditional', reason: 'Some events are unavailable; see diagnostics' } : { status: 'valid' };
}

/** Threshold crossings in [start, stop], linearly interpolated. A null sample
 * touching the window is reported through `missing`; it is never bridged. */
function crossings(s: Signal, thr: number, start: number, stop: number): { list: Crossing[]; missing: boolean } {
  const list: Crossing[] = [];
  let missing = false;
  for (let i = 0; i + 1 < s.t.length; i++) {
    const t0 = s.t[i]!, t1 = s.t[i + 1]!;
    if (t1 < start || t0 > stop) continue;
    const a = s.v[i], b = s.v[i + 1];
    if (a === null || a === undefined || b === null || b === undefined) { missing = true; continue; }
    if (a < thr && b >= thr) list.push({ timeS: t0 + (thr - a) * (t1 - t0) / (b - a), edge: 'rising' });
    else if (a > thr && b <= thr) list.push({ timeS: t0 + (thr - a) * (t1 - t0) / (b - a), edge: 'falling' });
  }
  return { list: list.filter(c => c.timeS >= start && c.timeS <= stop), missing };
}

function valueAt(s: Signal, t: number): number | null {
  const n = s.t.length;
  if (t < s.t[0]! || t > s.t[n - 1]!) return null;
  let lo = 0, hi = n - 1;
  while (hi - lo > 1) { const m = (lo + hi) >> 1; if (s.t[m]! <= t) lo = m; else hi = m; }
  const a = s.v[lo], b = s.v[hi];
  if (a === null || a === undefined || b === null || b === undefined) return null;
  return a + (b - a) * (t - s.t[lo]!) / (s.t[hi]! - s.t[lo]!);
}

/** Index of the first crossing strictly after t (list is time-ordered). */
function indexAfter(list: readonly Crossing[], t: number): number {
  let lo = 0, hi = list.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (list[m]!.timeS <= t) lo = m + 1; else hi = m; }
  return lo;
}
function lastAtOrBefore(list: readonly Crossing[], t: number): Crossing | undefined {
  const i = indexAfter(list, t) - 1;
  return i >= 0 ? list[i] : undefined;
}
function firstAfter(list: readonly Crossing[], t: number): Crossing | undefined {
  return list[indexAfter(list, t)];
}

class Tally {
  private readonly entries = new Map<string, { count: number; message: string }>();
  add(code: string, message: string) {
    const e = this.entries.get(code);
    if (e) e.count++; else this.entries.set(code, { count: 1, message });
  }
  list(): Diagnostic[] {
    return [...this.entries].map(([code, e]) => note(code, `${e.count} occurrence(s): ${e.message}`));
  }
}

function failAll(reason: string, code: string): TimingSignals {
  const series = unavailableSeries('s', reason, code);
  return {
    samples: { ...unavailableSeries('1', reason, code), kind: 'table' }, events: series, setup: series, hold: series,
    delay: series, skew: series, delaySpread: unavailableScalar('s', reason, code), diagnostics: [note(code, reason)],
  };
}

/** Resolves timing(options) from waveform accessors. seriesOf returns the
 * already-validated voltage DataDescription of a port (or undefined). */
export function timingFromSignals(options: TimingOptions, seriesOf: (port: unknown) => DataDescription | undefined): TimingSignals {
  const { startS, stopS } = options.window;
  if (options.kind === 'sampling') return sampling(options, seriesOf, startS, stopS);
  return propagation(options, seriesOf, startS, stopS);
}

function sampling(o: Extract<TimingOptions, { kind: 'sampling' }>, seriesOf: (port: unknown) => DataDescription | undefined, start: number, stop: number): TimingSignals {
  const inputs = [signal(seriesOf(o.clock.port), 'Clock'), ...o.data.map(d => signal(seriesOf(d.port), 'Data'))];
  const bad = inputs.find((x): x is string => typeof x === 'string');
  if (bad) return failAll(bad, 'TIMING_SIGNAL_UNAVAILABLE');
  const signals = inputs as Signal[];
  const [clock, ...data] = signals;
  const ck = crossings(clock!, o.clock.thresholdV, start, stop);
  if (ck.missing) return failAll('Clock has missing samples inside the window', 'TIMING_MISSING_SAMPLES');
  const events = ck.list.filter(c => o.clock.edges === 'both' || c.edge === o.clock.edges);
  if (!events.length) return failAll('No selected clock threshold crossing inside the window', 'TIMING_MISSING_EVENT');
  const levels = o.data.map((d, j) => {
    const hi = crossings(data[j]!, d.highV, start, stop), lo = crossings(data[j]!, d.lowV, start, stop);
    return { missing: hi.missing || lo.missing, hiRise: hi.list.filter(c => c.edge === 'rising'), hiFall: hi.list.filter(c => c.edge === 'falling'), loRise: lo.list.filter(c => c.edge === 'rising'), loFall: lo.list.filter(c => c.edge === 'falling') };
  });
  if (levels.some(l => l.missing)) return failAll('Data has missing samples inside the window', 'TIMING_MISSING_SAMPLES');

  const tally = new Tally();
  const rows: Json[] = [];
  const setup: (number | null)[] = [], hold: (number | null)[] = [];
  let partial = false;
  for (const ev of events) {
    const ts = ev.timeS + o.clock.sampleOffsetS;
    let setupWorst = Infinity, holdWorst = Infinity, setupOk = true, holdOk = true;
    o.data.forEach((d, j) => {
      const L = levels[j]!;
      const v = ts >= start && ts <= stop ? valueAt(data[j]!, ts) : null;
      let level: 'high' | 'low' | 'undefined' | 'unavailable' = 'unavailable';
      let setupS: number | null = null, holdS: number | null = null;
      if (v === null) tally.add('TIMING_SAMPLE_OUTSIDE_WINDOW', 'sample instant outside the signal or window');
      else if (v >= d.highV) level = 'high';
      else if (v <= d.lowV) level = 'low';
      else { level = 'undefined'; tally.add('TIMING_UNDEFINED_LEVEL', 'sample between the low and high thresholds'); }
      if (level === 'high' || level === 'low') {
        const high = level === 'high';
        // Setup: last arrival through this level's threshold; hold: first departure through the same threshold.
        const arrival = high ? lastAtOrBefore(L.hiRise, ts) : lastAtOrBefore(L.loFall, ts);
        const departure = high ? firstAfter(L.hiFall, ts) : firstAfter(L.loRise, ts);
        if (arrival) setupS = ts - arrival.timeS; else tally.add('TIMING_NO_TRANSITION', 'no preceding level transition inside the window');
        if (departure) holdS = departure.timeS - ts; else tally.add('TIMING_NO_TRANSITION', 'no following level transition inside the window');
      }
      if (setupS === null) setupOk = false; else setupWorst = Math.min(setupWorst, setupS);
      if (holdS === null) holdOk = false; else holdWorst = Math.min(holdWorst, holdS);
      rows.push({ clockEventS: ev.timeS, sampleS: ts, port: d.port.id, valueV: v, level, setupS, holdS });
    });
    setup.push(setupOk ? setupWorst : null);
    hold.push(holdOk ? holdWorst : null);
    if (!setupOk || !holdOk) partial = true;
  }
  const times = events.map(e => e.timeS);
  const axis = { name: 'timeS' as const, unit: 's' as const, values: times };
  const validity = combineValidity(signals, partial);
  return {
    samples: { kind: 'table', unit: '1', validity, diagnostics: [], data: { rows } },
    events: { kind: 'series', unit: 's', axis, values: times, validity, diagnostics: [] },
    setup: { kind: 'series', unit: 's', axis, values: setup, validity, diagnostics: [] },
    hold: { kind: 'series', unit: 's', axis, values: hold, validity, diagnostics: [] },
    delay: unavailableSeries('s', 'Propagation delay applies to propagation timing only'),
    skew: unavailableSeries('s', 'Skew applies to propagation timing only'),
    delaySpread: unavailableScalar('s', 'Delay spread applies to propagation timing only'),
    diagnostics: tally.list(),
  };
}

function propagation(o: Extract<TimingOptions, { kind: 'propagation' }>, seriesOf: (port: unknown) => DataDescription | undefined, start: number, stop: number): TimingSignals {
  const pairs = o.paths.map(p => [signal(seriesOf(p.from), 'Source'), signal(seriesOf(p.to), 'Destination')] as const);
  const bad = pairs.flat().find((x): x is string => typeof x === 'string');
  if (bad) return failAll(bad, 'TIMING_SIGNAL_UNAVAILABLE');
  const signals = pairs.flat() as Signal[];
  const crossed = o.paths.map((p, k) => {
    const [src, dst] = pairs[k] as [Signal, Signal];
    return { sc: crossings(src, p.sourceThresholdV, start, stop), dc: crossings(dst, p.destinationThresholdV, start, stop) };
  });
  if (crossed.some(c => c.sc.missing || c.dc.missing)) return failAll('Path has missing samples inside the window', 'TIMING_MISSING_SAMPLES');

  const tally = new Tally();
  const rows: Json[] = [];
  const perPath: (number | null)[][] = [], sourceTimes: number[][] = [];
  o.paths.forEach((p, k) => {
    const { sc, dc } = crossed[k]!;
    const sources = sc.list.filter(c => o.edges === 'both' || c.edge === o.edges);
    // Non-inverting match: a destination crossing with the same edge as its source.
    const candidates: Record<Edge, Crossing[]> = { rising: dc.list.filter(c => c.edge === 'rising'), falling: dc.list.filter(c => c.edge === 'falling') };
    const delays: (number | null)[] = [];
    for (const e of sources) {
      const list = candidates[e.edge];
      const until = e.timeS + o.maxDelayS;
      const i = indexAfter(list, e.timeS);
      let n = 0;
      while (i + n < list.length && list[i + n]!.timeS <= until && n < 2) n++;
      let delayS: number | null = null, destinationS: number | null = null;
      if (n === 1) { destinationS = list[i]!.timeS; delayS = destinationS - e.timeS; }
      else if (n === 0) {
        if (until > stop) tally.add('TIMING_WINDOW_LIMITED', 'destination crossing not found before the window end');
        else tally.add('TIMING_MISSING_EVENT', 'no destination crossing within maxDelayS');
      } else tally.add('TIMING_AMBIGUOUS_MATCH', 'more than one destination crossing within maxDelayS');
      delays.push(delayS);
      rows.push({ path: k, from: o.paths[k]!.from.id, to: o.paths[k]!.to.id, sourceS: e.timeS, destinationS, delayS });
    }
    perPath.push(delays);
    sourceTimes.push(sources.map(e => e.timeS));
  });

  const flat = o.paths.flatMap((_, k) => sourceTimes[k]!.map((t, j) => ({ t, k, v: perPath[k]![j] ?? null }))).sort((a, b) => a.t - b.t);
  const validity = combineValidity(signals, perPath.some(ds => ds.some(v => v === null)));
  if (!flat.length) return failAll('No source threshold crossing inside the window', 'TIMING_MISSING_EVENT');
  const delayVals = flat.map(f => f.v);
  const delayPartial = delayVals.some(v => v === null);
  const delayValidity = combineValidity(signals, delayPartial);
  const delay: DataDescription = {
    kind: 'series', unit: 's', axis: { name: 'timeS', unit: 's', values: flat.map(f => f.t) }, values: delayVals,
    validity: delayValidity, diagnostics: [], data: { pathIndex: flat.map(f => f.k) },
  };

  let skew: DataDescription;
  if (o.paths.length < 2) skew = unavailableSeries('s', 'Skew needs at least two paths');
  else {
    const ref = sourceTimes[0]!;
    if (o.paths.some((_, k) => sourceTimes[k]!.length !== ref.length)) tally.add('TIMING_AMBIGUOUS_MATCH', 'source event counts differ between paths; unpaired ordinals are null');
    const values = ref.map((_, j) => {
      const col = o.paths.map((_, k) => perPath[k]?.[j] ?? null);
      if (col.some(v => v === null)) return null;
      const nums = col as number[];
      return Math.max(...nums) - Math.min(...nums);
    });
    skew = { kind: 'series', unit: 's', axis: { name: 'timeS', unit: 's', values: ref }, values, validity: combineValidity(signals, values.some(v => v === null)), diagnostics: [] };
  }

  const finite = delayVals.filter((v): v is number => v !== null);
  const delaySpread: DataDescription = finite.length
    ? { kind: 'scalar', unit: 's', value: Math.max(...finite) - Math.min(...finite), validity: delayValidity, diagnostics: [] }
    : unavailableScalar('s', 'No valid delay to spread; see diagnostics', 'TIMING_MISSING_EVENT');

  return {
    samples: { ...unavailableSeries('1', 'Sampled levels apply to sampling timing only'), kind: 'table' },
    events: { kind: 'table', unit: '1', validity, diagnostics: [], data: { rows } },
    setup: unavailableSeries('s', 'Setup applies to sampling timing only'),
    hold: unavailableSeries('s', 'Hold applies to sampling timing only'),
    delay, skew, delaySpread, diagnostics: tally.list(),
  };
}

export const EYE_ASSUMPTIONS: readonly string[] = [
  'Folds the supplied voltage samples with t >= discardS modulo UI = 1/bitRateBps, phase origin discardS + phaseS; no interpolation or resampling.',
  'Phase is resolved into 64 bins per UI; width and timingMargin are quantized to about one bin (UI/64).',
  'High and low samples are classified only by thresholdV; no bit recovery, pattern alignment, jitter, noise, equalization or BER model.',
  'Height is the minimum over bins within 0.1 UI of UI/2 of (lowest high - highest low); voltageMargin is the minimum distance from thresholdV to the worst high or low in those bins.',
  'Width is the circular span of bins around UI/2 whose opening is at least 50% of height (a convention, not a mask); timingMargin is the smaller distance from UI/2 to the span edges.',
  'Refused when the signal covers less than 2 UI after discardS; missing samples after discardS give unavailable results.',
];

export interface EyeInput { signal: DataDescription; bitRateBps: number; discardS: number; phaseS: number; thresholdV: number }
export interface EyeAnalysis {
  metrics: Record<'diagram' | 'height' | 'width' | 'timingMargin' | 'voltageMargin', DataDescription>;
  status: 'complete' | 'partial'; diagnostics: Diagnostic[]; assumptions: string[];
}

const PHASE_BINS = 64;

function unavailableEye(reason: string, code: string): EyeAnalysis {
  const metric = (unit: string) => unavailableScalar(unit, reason, code);
  return {
    metrics: { diagram: { ...unavailableScalar('V', reason, code), kind: 'eye' }, height: metric('V'), width: metric('s'), timingMargin: metric('s'), voltageMargin: metric('V') },
    status: 'partial', diagnostics: [note(code, reason)], assumptions: [...EYE_ASSUMPTIONS],
  };
}

/** Eye metrics from one supplied time-voltage series. Refuses (throws
 * AnalysisDslError) when the signal is structurally invalid, does not cover
 * discardS, or is shorter than 2 UI after discardS. */
export function eyeFromSignal(c: EyeInput): EyeAnalysis {
  const ui = 1 / c.bitRateBps;
  const s = signal(c.signal, 'Eye signal');
  if (typeof s === 'string') {
    if (c.signal.validity.status === 'unavailable') return unavailableEye(s, 'DATA_UNAVAILABLE');
    throw new AnalysisDslError('EYE_INVALID_SIGNAL', s);
  }
  if (s.t[0]! > c.discardS) throw new AnalysisDslError('EYE_SIGNAL_NOT_COVERING_DISCARD', 'Signal starts after discardS; nothing to fold');
  const available = s.t[s.t.length - 1]! - c.discardS;
  if (available < 2 * ui) throw new AnalysisDslError('EYE_SIGNAL_TOO_SHORT', `Eye needs at least 2 full UI (${2 * ui} s) after discardS; available ${available} s`);

  const B = PHASE_BINS, origin = c.discardS + c.phaseS, thr = c.thresholdV;
  const hiMin = Array<number>(B).fill(Infinity), hiMax = Array<number>(B).fill(-Infinity);
  const loMin = Array<number>(B).fill(Infinity), loMax = Array<number>(B).fill(-Infinity);
  const count = Array<number>(B).fill(0);
  let missing = 0;
  s.t.forEach((t, i) => {
    if (t < c.discardS) return;
    const v = s.v[i];
    if (v === null || v === undefined) { missing++; return; }
    const phase = (((t - origin) % ui) + ui) % ui;
    const b = Math.min(B - 1, Math.floor(phase / ui * B));
    count[b]!++;
    if (v >= thr) { hiMin[b] = Math.min(hiMin[b]!, v); hiMax[b] = Math.max(hiMax[b]!, v); }
    else { loMin[b] = Math.min(loMin[b]!, v); loMax[b] = Math.max(loMax[b]!, v); }
  });
  if (missing) return unavailableEye(`Eye signal has ${missing} missing sample(s) after discardS`, 'EYE_MISSING_SAMPLES');

  const has = (b: number) => hiMin[b] !== Infinity && loMin[b] !== Infinity;
  const openingOf = (b: number) => hiMin[b]! - loMax[b]!;
  const mod = (x: number) => ((x % B) + B) % B;
  const centre = Array.from({ length: B }, (_, b) => b).filter(b => Math.abs((b + 0.5) / B * ui - ui / 2) <= 0.1 * ui);
  const diagnostics: Diagnostic[] = [];
  const sparse = count.filter(n => n === 0).length;
  const validity: Validity = s.validity.status === 'conditional'
    ? { status: 'conditional', reason: s.validity.reason ?? 'Conditional input signal' }
    : sparse ? { status: 'conditional', reason: `${sparse} of ${B} phase bins have no samples; width and timing resolution are sample-density limited` } : { status: 'valid' };

  let heightV: number | null = null, voltageMarginV: number | null = null;
  if (centre.some(b => !has(b))) diagnostics.push(note('EYE_EMPTY_CENTRE', 'Centre window has phase bins without both high and low samples; height unavailable'));
  else {
    heightV = Math.min(...centre.map(openingOf));
    voltageMarginV = Math.min(...centre.map(b => Math.min(hiMin[b]! - thr, thr - loMax[b]!)));
  }

  let widthS: number | null = null, timingMarginS: number | null = null;
  if (heightV === null) diagnostics.push(note('EYE_EMPTY_CENTRE', 'Width needs the centre height; unavailable'));
  else if (heightV <= 0) { widthS = 0; timingMarginS = 0; }
  else {
    const c0 = B / 2;
    const open = (b: number) => has(b) && openingOf(b) >= 0.5 * heightV!;
    let L = 0, R = 0;
    while (L + 1 < B && open(mod(c0 - L - 1))) L++;
    while (L + R + 1 < B && open(mod(c0 + R + 1))) R++;
    widthS = (L + R + 1) * ui / B;
    timingMarginS = Math.min(L, R + 1) * ui / B;
  }

  const fmt = (v: number | null, unit: string, reason: string): DataDescription => v === null
    ? unavailableScalar(unit, reason, 'EYE_EMPTY_CENTRE')
    : { kind: 'scalar', unit, value: v, validity, diagnostics: [] };
  const numOrNull = (x: number) => (Number.isFinite(x) ? x : null);
  const diagram: DataDescription = {
    kind: 'eye', unit: 'V', validity, diagnostics: [],
    data: {
      uiS: ui, phaseBins: B, phaseS: Array.from({ length: B }, (_, b) => (b + 0.5) / B * ui),
      highMinV: hiMin.map(numOrNull), highMaxV: hiMax.map(numOrNull), lowMinV: loMin.map(numOrNull), lowMaxV: loMax.map(numOrNull), count,
    } as Json,
  };
  const metrics = {
    diagram,
    height: fmt(heightV, 'V', 'Height unavailable; see diagnostics'),
    width: fmt(widthS, 's', 'Width unavailable; see diagnostics'),
    timingMargin: fmt(timingMarginS, 's', 'Timing margin unavailable; see diagnostics'),
    voltageMargin: fmt(voltageMarginV, 'V', 'Voltage margin unavailable; see diagnostics'),
  };
  const complete = heightV !== null && widthS !== null;
  return { metrics, status: complete ? 'complete' : 'partial', diagnostics, assumptions: [...EYE_ASSUMPTIONS] };
}
