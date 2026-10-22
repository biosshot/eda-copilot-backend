import type { CheckInput, CheckRecord, DataDescription, DataValue, Json, Report } from './spec-doc.js';
import { z } from 'zod';
import { createHash } from 'node:crypto';
import { AnalysisDslError, data, invalid, parse } from './schema.ts';

/** Strict finite JSON snapshot: callbacks and Promises are never native task data. */
export function json(value: unknown): Json {
  const active = new Set<object>();
  function copy(v: unknown): Json {
    if (v === null || typeof v === 'string' || typeof v === 'boolean') return v;
    if (typeof v === 'number' && Number.isFinite(v)) return v;
    if (!v || typeof v !== 'object' || 'then' in v) invalid('Expected finite serializable data, not function/Promise/undefined');
    if (active.has(v)) invalid('Cyclic data is not supported');
    active.add(v);
    const out: Json = Array.isArray(v) ? v.map(copy) : Object.fromEntries(Object.entries(v).filter(([, x]) => x !== undefined).map(([k, x]) => [k, copy(x)]));
    active.delete(v); return out;
  }
  return copy(value);
}
export function freeze<T>(v: T): T {
  if (v && typeof v === 'object' && !Object.isFrozen(v)) { Object.values(v).forEach(freeze); Object.freeze(v); }
  return v;
}
export function unavailable(unit: string, reason: string, contextId?: string): DataValue {
  return value({ kind: 'scalar', unit, validity: { status: 'unavailable', reason }, diagnostics: [{ code: 'DATA_UNAVAILABLE', severity: 'warning', message: reason }], ...(contextId ? { contextId } : {}) });
}
type ReportSection = 'descriptors' | 'calculations' | 'checks' | 'logs' | 'plots' | 'diagnostics';
export type ReportUpdate = { kind: 'context'; report: Report } | { kind: 'entry'; contextId: string; section: ReportSection; record: Json };
/** Append-only progress preserves partial reports without repeatedly copying
 * the entire accumulated report across IPC on every log/check/plot. */
export function applyReportUpdate(reports: Report[], update: ReportUpdate): void {
  if (update.kind === 'context') { reports.push(update.report); return; }
  const report = reports.find(r => r.contextId === update.contextId);
  if (!report) throw new AnalysisDslError('WORKER_FAILED', 'Report update references an unknown context');
  (report[update.section] as Json[]).push(update.record);
  if (update.section === 'checks') report.summary[(update.record as unknown as CheckRecord).status]++;
}
/** Real scalar transforms and supplied finite-value reducers. No missing value
 * becomes zero; partial reductions retain an explicit conditional coverage. */
export function value(input: DataDescription): DataValue {
  const description = parse(data, json(input)) as DataDescription;
  const result = { ...description };
  const unsupported = (operation: string) => unavailable(operation === 'phaseDeg' ? 'deg' : description.unit, `${operation} numerical processing is not implemented`, description.contextId);
  for (const name of ['min', 'max', 'abs', 'real', 'imaginary', 'magnitude', 'phaseDeg', 'at']) {
    Object.defineProperty(result, name, { value: (position?: unknown) => {
      if (name === 'at') parse(z.union([z.strictObject({ timeS: z.number().finite().nonnegative() }), z.strictObject({ frequencyHz: z.number().finite().positive() })]), position);
      if (description.validity.status !== 'unavailable') {
        if (['min','max'].includes(name) && description.values) {
          const references = description.data && typeof description.data === 'object' && 'independentReferences' in description.data ? description.data.independentReferences : undefined;
          if (description.kind === 'map' && description.unit === 'V' && Array.isArray(references) && references.length > 1) return unavailable('V','Cannot reduce potentials across independent mathematical references',description.contextId);
          const numbers = description.values.filter((v): v is number => typeof v === 'number' && Number.isFinite(v));
          if (numbers.length) {
            const partial = numbers.length !== description.values.length;
            return value({kind:'scalar',unit:description.unit,value:numbers.reduce((a,b)=>name==='min'?Math.min(a,b):Math.max(a,b)),validity:partial?{status:'conditional',reason:'Reducer covers available real values only; missing/complex values excluded',domain:description.data}:description.validity,diagnostics:description.diagnostics,...(description.contextId?{contextId:description.contextId}:{})});
          }
        }
        if (description.kind === 'scalar' && typeof description.value === 'number' && ['abs','real','imaginary','magnitude'].includes(name)) return value({...description,value:name==='imaginary'?0:name==='real'?description.value:Math.abs(description.value)});
      }
      return (name === 'min' || name === 'max') && description.kind === 'scalar' && typeof description.value === 'number' ? value(description) : unsupported(name);
    } });
  }
  return freeze(result) as DataValue;
}

export class ReportStore {
  readonly report: Report;
  constructor(contextId: string, boardIdentity: string | undefined, private readonly limits: { maxEntries: number; maxOutputBytes: number }, private readonly changed: (update: ReportUpdate) => void, private readonly budget = { entries: 0, bytes: 0 }) {
    this.report = { contextId, ...(boardIdentity ? { boardIdentity } : {}), descriptors: [], calculations: [], checks: [], logs: [], plots: [], summary: { pass: 0, fail: 0, unknown: 0 }, diagnostics: [] };
    const size = Buffer.byteLength(JSON.stringify(this.report));
    if (budget.bytes + size > limits.maxOutputBytes) throw new AnalysisDslError('OUTPUT_LIMIT', 'Scenario report context byte limit exceeded');
    budget.bytes += size;
  }
  add(section: ReportSection, record: unknown) {
    const safe = section === 'descriptors' || section === 'calculations' ? reportReferences(json(record)) : json(record);
    const size = Buffer.byteLength(JSON.stringify(safe)) + 1;
    if (this.budget.entries >= this.limits.maxEntries || this.budget.bytes + size > this.limits.maxOutputBytes) throw new AnalysisDslError('OUTPUT_LIMIT', 'Analysis report entry/byte limit exceeded');
    this.budget.entries++; this.budget.bytes += size;
    (this.report[section] as Json[]).push(safe);
    if (section === 'checks') this.report.summary[(record as CheckRecord).status]++;
    this.changed({ kind: 'entry', contextId: this.report.contextId, section, record: safe });
  }
  check(input: CheckInput): CheckRecord {
    let status: CheckRecord['status'] = 'unknown', message = 'Invalid or unavailable check value/condition';
    const actual = input?.actual;
    const wrapped = !!actual && typeof actual === 'object';
    const actualValue = wrapped ? actual.value : actual;
    const finite = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);
    const range = input?.min !== undefined || input?.max !== undefined;
    const targeted = input?.target !== undefined || input?.tolerance !== undefined;
    const equals = input?.equals !== undefined;
    const keys = new Set(['name', 'actual', 'min', 'max', 'target', 'tolerance', 'unit', 'equals']);
    const correctInput = input && Object.keys(input).every(k => keys.has(k)) && typeof input.name === 'string' && input.name.length > 0;
    const known = !wrapped || actual.kind === 'scalar' && actual.validity && ['valid', 'conditional'].includes(actual.validity.status);
    const compatibleUnit = !wrapped || input?.unit === undefined || input.unit === actual.unit;
    if (wrapped && actual.validity?.status === 'unavailable') message = actual.validity.reason ?? 'Value is unavailable';
    else if (wrapped && input?.unit && input.unit !== actual.unit) message = `Expected unit ${input.unit}; actual unit ${actual.unit}`;
    // Conditional values are usable only in the stated validity domain. An
    // unresolved/unavailable value is never a passing numeric test.
    if (correctInput && known && compatibleUnit && Number(range) + Number(targeted) + Number(equals) === 1) {
      let pass: boolean | undefined;
      if (equals && !wrapped && (typeof actualValue === 'string' || typeof actualValue === 'boolean' || Number.isInteger(actualValue)) && (typeof input.equals === 'string' || typeof input.equals === 'boolean' || Number.isInteger(input.equals))) pass = actualValue === input.equals;
      if (equals && wrapped && actual.unit === '1' && (typeof actualValue === 'string' || typeof actualValue === 'boolean' || Number.isInteger(actualValue)) && (typeof input.equals === 'string' || typeof input.equals === 'boolean' || Number.isInteger(input.equals))) pass = actualValue === input.equals;
      if (!equals && finite(actualValue) && typeof input.unit === 'string' && (!wrapped || actual.unit === input.unit)) {
        if (range && (input.min === undefined || finite(input.min)) && (input.max === undefined || finite(input.max)) && (input.min === undefined || input.max === undefined || input.min <= input.max)) pass = (input.min === undefined || actualValue >= input.min) && (input.max === undefined || actualValue <= input.max);
        if (targeted && finite(input.target) && finite(input.tolerance) && input.tolerance >= 0) pass = Math.abs(actualValue - input.target) <= input.tolerance;
      }
      if (pass !== undefined) { status = pass ? 'pass' : 'fail'; message = pass ? 'Condition satisfied within supplied validity' : 'Condition not satisfied'; }
    }
    const safeActual = (() => { try { return json(actual); } catch { return null; } })();
    const expected = Object.fromEntries(Object.entries(input ?? {}).filter(([k]) => !['name', 'actual'].includes(k)));
    let safeExpected: Json = null; try { safeExpected = json(expected); } catch { /* Retain unknown instead of nonfinite JSON. */ }
    const record: CheckRecord = { id: `check-${this.report.checks.length + 1}`, name: typeof input?.name === 'string' ? input.name : 'Invalid check', status, actual: safeActual, expected: safeExpected, message };
    this.add('checks', record); return freeze(record);
  }
  snapshot(): Report { return json(this.report) as unknown as Report; }
}

/** Automatic report entries retain reproducible identities, not repeated model
 * payloads. Executor tasks keep the complete immutable resources separately.
 * Explicit plot/log data is retained as supplied, subject to the scenario cap. */
function reportReferences(v: Json): Json {
  if (Array.isArray(v)) return v.map(reportReferences);
  if (!v || typeof v !== 'object') return v;
  const record = v as Record<string, Json>;
  const resourceFields = ['id', 'kind', 'source', 'analyses', 'ports', 'parameters', 'validity', 'data'];
  if (record.kind === 'material') resourceFields.push('conductivitySM', 'relativePermittivity', 'lossTangent', 'thermalConductivityWPerMK', 'densityKgM3', 'specificHeatJPerKgK', 'referenceC', 'temperatureCoefficientPerK', 'state');
  if (typeof record.id === 'string' && typeof record.kind === 'string' && !record.contextId && !['padId', 'finalState'].includes(record.kind) && Object.keys(record).every(k => resourceFields.includes(k))) {
    return { resourceId: record.id, kind: record.kind, sha256: createHash('sha256').update(JSON.stringify(record)).digest('hex'), ...(record.source ? { source: record.source } : {}) };
  }
  return Object.fromEntries(Object.entries(record).map(([k, x]) => [k, reportReferences(x)]));
}
