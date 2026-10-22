import assert from 'node:assert/strict';
import type { AnalysisDcReport, AnalysisConductionReport } from '../../native/analyze/index.js';
import type { AnalysisDslOutput } from '../../src/analyze/run-analysis-dsl.ts';
import type { DataDescription } from '../../src/analyze/analysis-dsl/spec-doc.js';

export interface DcExpectations {
  cases: { id: string; maxAreaMm2: number; loadScale: number }[];
  measurements: string[];
  convergence: { relative: number; voltageFloorV: number; lossFloorW: number };
  checks: { name: string; status: 'pass' | 'fail'; max?: number }[];
  requireBarrelCurrent: boolean;
}
export interface DcCase {
  caseId: string; resultId: string; maxAreaMm2: number; loadScale: number;
  measurements: { label: string; portId: string; referenceV: number; voltage: DataDescription; drop: DataDescription }[];
}
export interface DcCompleted {
  id: string; contextId: string; kind: string; solution: AnalysisDcReport & {
    id: string; boardIdentity: string; meshReference?: string;
    request: { context_id: string; conduction: unknown };
  };
  voltageMap: DataDescription; currentDensityMap: DataDescription; lossMap: DataDescription;
}
export function completedDc(report: AnalysisDslOutput): DcCompleted[] {
  return report.reports.flatMap(r => r.calculations)
    .filter(c => c && typeof c === 'object' && !Array.isArray(c) && c.phase === 'completed' && c.kind === 'dc')
    .map(c => (c as { result: unknown }).result as DcCompleted);
}
export function dcCases(report: AnalysisDslOutput): DcCase[] {
  return report.reports.flatMap(r => r.logs).filter(l => l.message === 'dc.case').map(l => l.data as unknown as DcCase);
}
/** Resolve only compatible immutable report geometry, independently of native reuse. */
export function resolveDcMesh(all: DcCompleted[], result: DcCompleted): AnalysisConductionReport {
  const seen = new Set<string>();
  let current = result;
  while (true) {
    assert.ok(!seen.has(current.id), `cyclic mesh reference: ${current.id}`); seen.add(current.id);
    assert.equal(current.contextId, result.contextId, 'mesh reference context');
    assert.equal(current.solution.boardIdentity, result.solution.boardIdentity, 'mesh reference board');
    assert.deepEqual(current.solution.request.conduction, result.solution.request.conduction, 'mesh reference profile/options');
    if (!current.solution.meshReference) {
      assert.ok(current.solution.mesh, `missing mesh: ${current.id}`);
      return current.solution.mesh;
    }
    const target = all.find(r => r.id === current.solution.meshReference);
    assert.ok(target, `dangling mesh reference: ${current.solution.meshReference}`); current = target;
  }
}
function finite(value: unknown, label: string): asserts value is number {
  assert.equal(typeof value, 'number', `${label}: numeric`); assert.ok(Number.isFinite(value), `${label}: finite`);
}
function numeric(value: DataDescription, unit: string, label: string): number {
  assert.ok(value, `${label}: missing measurement`); assert.equal(value.unit, unit, `${label}: unit`);
  assert.ok(['valid', 'conditional'].includes(value.validity.status), `${label}: unavailable`);
  finite(value.value, label); return value.value;
}
function unique(actual: string[], expected: string[], label: string) {
  assert.equal(new Set(actual).size, actual.length, `${label}: duplicate ID`);
  assert.deepEqual([...actual].sort(), [...expected].sort(), label);
}
function converged(coarse: number, fine: number, floor: number, relative: number, label: string) {
  finite(coarse, label); finite(fine, label);
  const change = Math.abs(fine - coarse) / Math.max(Math.abs(coarse), floor);
  assert.ok(change <= relative, `${label}: change=${change} > ${relative}`); return change;
}
/** Serialized-data assertions only. Never invokes a solver or searches old outputs. */
export function auditDcReport(report: AnalysisDslOutput, expected: DcExpectations) {
  assert.equal(report.status, 'complete', `runner: ${JSON.stringify(report.error)}`);
  const all = completedDc(report), cases = dcCases(report);
  unique(all.map(r => r.id), cases.map(c => c.resultId), 'completed results/cases');
  unique(cases.map(c => c.caseId), expected.cases.map(c => c.id), 'case IDs');
  assert.ok(expected.cases.length > 0 && expected.measurements.length > 0, 'nonempty expectations');
  const checks = report.reports.flatMap(r => r.checks);
  assert.ok(checks.every(c => c.status !== 'unknown'), 'unknown check');
  unique(checks.map(c => c.name), expected.checks.map(c => c.name), 'check IDs');
  assert.deepEqual(checks.map(c => c.name), expected.checks.map(c => c.name), 'check sequence (loss check follows consumer checks)');
  for (const e of expected.checks) {
    const c = checks.find(c => c.name === e.name)!;
    assert.equal(c.status, e.status, `${e.name}: check status`);
    if (e.max !== undefined) assert.equal((c.expected as { max?: number }).max, e.max, `${e.name}: fixed threshold`);
  }
  for (const c of report.reports.flatMap(r => r.calculations)) {
    if (c && typeof c === 'object' && !Array.isArray(c) && c.kind === 'sweep-attempt') assert.notEqual(c.status, 'failed', 'failed sweep attempt');
  }
  const plots = report.reports.flatMap(r => r.plots);
  function fieldReference(value: DataDescription) {
    const data = value.data as { solutionId?: string; field?: string; slices?: { domainId: string; start: number; count: number }[] } | undefined;
    assert.ok(data?.solutionId, 'missing plot/map solution reference');
    const result = all.find(r => r.id === data.solutionId); assert.ok(result, 'dangling plot/map solution reference');
    const field = data.field as 'voltage_v' | 'current_density_a_mm2' | 'loss_w';
    assert.ok(['voltage_v', 'current_density_a_mm2', 'loss_w'].includes(field), 'unknown plot/map field');
    if (value.contextId !== undefined) assert.equal(value.contextId, result.contextId, 'plot/map context');
    const domains = result.solution.domains!;
    let offset = 0;
    const slices = domains.map(d => { const start = offset; offset += d[field].length; return { domainId: d.domain_id, start, count: d[field].length }; });
    assert.deepEqual(data.slices, slices, 'map slices');
    if (value.values) assert.deepEqual(value.values, domains.flatMap(d => d[field]), 'map samples');
    assert.ok(['valid', 'conditional'].includes(value.validity.status), 'unavailable map');
  }
  for (const p of plots) fieldReference(p.data as unknown as DataDescription);
  for (const c of cases) {
    const e = expected.cases.find(e => e.id === c.caseId)!;
    assert.equal(c.maxAreaMm2, e.maxAreaMm2, 'case mesh'); assert.equal(c.loadScale, e.loadScale, 'case load');
    unique(c.measurements.map(m => m.label), expected.measurements, `${c.caseId}: measurements`);
    for (const m of c.measurements) {
      finite(m.referenceV, 'reference voltage'); assert.ok(m.portId, 'measurement port ID');
      const v = numeric(m.voltage, 'V', `${c.caseId}/${m.label}/voltage`);
      const drop = numeric(m.drop, 'V', `${c.caseId}/${m.label}/drop`);
      assert.equal(drop, Math.abs(m.referenceV - v), 'drop agrees with measured voltage');
    }
    const r = all.find(r => r.id === c.resultId)!, s = r.solution, mesh = resolveDcMesh(all, r);
    assert.equal(s.status, 'complete', `${c.caseId}: native result`);
    assert.ok(s.domains?.length, 'missing computed domains');
    unique(s.domains!.map(d => d.domain_id), s.included_domains!, 'included domains');
    for (const d of s.domains!) {
      const m = mesh.domains.find(m => m.id === d.domain_id); assert.ok(m, `missing domain geometry: ${d.domain_id}`);
      assert.equal(m.layer, d.layer, 'domain layer');
      assert.ok(m.nodes.length > 0 && m.triangles.length > 0, 'empty computed mesh');
      for (const point of m.nodes) { assert.equal(point.length, 2); point.forEach(v => finite(v, 'mesh coordinate')); }
      for (const t of m.triangles) { assert.equal(t.length, 3); assert.equal(new Set(t).size, 3); assert.ok(t.every(i => Number.isInteger(i) && i >= 0 && i < m.nodes.length), 'triangle index'); }
      for (const [field, count] of [['voltage_v', m.nodes.length], ['current_density_a_mm2', m.triangles.length], ['loss_w', m.triangles.length]] as const) {
        assert.equal(d[field].length, count, `${field}: cardinality`); d[field].forEach(v => finite(v, field));
      }
      assert.equal(d.current_density_vector_a_mm2.length, m.triangles.length, 'J vector cardinality');
      for (const v of d.current_density_vector_a_mm2) { assert.equal(v.length, 2); v.forEach(x => finite(x, 'J vector')); }
    }
    const q = s.quality; assert.ok(q, 'missing quality');
    for (const v of [q.relative_residual, q.current_balance_a, q.normalized_current_balance, q.power_balance_w, q.normalized_power_balance]) finite(v, 'quality');
    assert.ok(q.relative_residual >= 0 && q.relative_residual <= 1e-8, 'residual guard');
    assert.ok(Math.abs(q.current_balance_a) <= 1e-12 || Math.abs(q.normalized_current_balance!) <= 1e-8, 'KCL guard');
    assert.ok(Math.abs(q.power_balance_w) <= 1e-15 || Math.abs(q.normalized_power_balance!) <= 1e-8, 'power guard');
    finite(s.copper_loss_w, 'copper loss'); finite(s.barrel_loss_w, 'barrel loss');
    for (const b of s.barrels ?? []) for (const v of [b.current_a, b.drop_v, b.loss_w]) finite(v, 'barrel field');
    if (expected.requireBarrelCurrent) assert.ok(s.barrels?.some(b => Math.abs(b.current_a) > 1e-12), 'required nonzero barrel current');
    for (const field of [r.voltageMap, r.currentDensityMap, r.lossMap]) fieldReference(field);
    for (const field of ['voltage_v', 'current_density_a_mm2', 'loss_w']) assert.ok(plots.some(p => {
      const data = (p.data as unknown as DataDescription)?.data as { solutionId?: string; field?: string } | undefined;
      return data?.solutionId === r.id && data.field === field;
    }), `${c.caseId}: missing ${field} plot after checks`);
  }
  const convergence = [];
  for (const scale of new Set(expected.cases.map(c => c.loadScale))) {
    const pair = cases.filter(c => c.loadScale === scale).sort((a, b) => b.maxAreaMm2 - a.maxAreaMm2);
    assert.equal(pair.length, 2, 'exactly two meshes per load'); const [a, b] = pair;
    const drops = a.measurements.map(m => ({ label: m.label, relativeChange: converged(m.drop.value!, b.measurements.find(n => n.label === m.label)!.drop.value!, expected.convergence.voltageFloorV, expected.convergence.relative, `${scale}/${m.label}: drop convergence`) }));
    const sa = all.find(r => r.id === a.resultId)!.solution, sb = all.find(r => r.id === b.resultId)!.solution;
    const loss = converged(sa.copper_loss_w! + sa.barrel_loss_w!, sb.copper_loss_w! + sb.barrel_loss_w!, expected.convergence.lossFloorW, expected.convergence.relative, `${scale}: conductor loss convergence`);
    convergence.push({ loadScale: scale, drops, conductorLossRelativeChange: loss });
  }
  return { cases: cases.length, complete: all.length, convergence };
}
