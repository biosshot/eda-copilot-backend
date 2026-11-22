import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisDslOutput } from '../src/analyze/run-analysis-dsl.ts';
import { auditDcReport, type DcExpectations } from './helpers/pcb-analysis-dc-audit.ts';

// A small serialized-data fixture tests the validator, not FEM accuracy.
function fixture() {
  const expectations: DcExpectations = { cases: [], measurements: ['USB output'], checks: [],
    convergence: { relative: .02, voltageFloorV: 1e-9, lossFloorW: 1e-9 }, requireBarrelCurrent: true };
  const scalar = (value: number) => ({ kind: 'scalar', unit: 'V', value, validity: { status: 'conditional' }, diagnostics: [] });
  const calculations = [], logs = [], checks = [], plots = [];
  for (const area of [1, .5]) for (const scale of [1, 2]) {
    const id = `result-${area}-${scale}`, caseId = `mesh-${area}/load-${scale}`;
    expectations.cases.push({ id: caseId, maxAreaMm2: area, loadScale: scale });
    const field = (name: string, values: number[]) => ({ kind: 'map', unit: name === 'voltage_v' ? 'V' : name === 'loss_w' ? 'W' : 'A/mm2', values,
      validity: { status: 'conditional' }, diagnostics: [], data: { solutionId: id, field: name, slices: [{ domainId: 'domain', start: 0, count: values.length }] } });
    const map = { voltageMap: field('voltage_v', [1, .9, .9]), currentDensityMap: field('current_density_a_mm2', [2]), lossMap: field('loss_w', [.01 * scale * scale]) };
    calculations.push({ phase: 'completed', kind: 'dc', result: { id, contextId: 'context', kind: 'dc', ...map, solution: {
      id, boardIdentity: 'board', request: { context_id: 'context', conduction: { area } }, status: 'complete', conditional: true,
      mesh: { domains: [{ id: 'domain', layer: 'TOP', nodes: [[0, 0], [1, 0], [0, 1]], triangles: [[0, 1, 2]] }] },
      domains: [{ domain_id: 'domain', layer: 'TOP', voltage_v: map.voltageMap.values, current_density_a_mm2: [2], current_density_vector_a_mm2: [[2, 0]], loss_w: map.lossMap.values }],
      included_domains: ['domain'], quality: { relative_residual: 1e-12, current_balance_a: 1e-14, normalized_current_balance: 1e-12, power_balance_w: 1e-17, normalized_power_balance: 1e-12 },
      copper_loss_w: .01 * scale * scale, barrel_loss_w: .001 * scale * scale, barrels: [{ current_a: .1 * scale, drop_v: .01 * scale, loss_w: .001 * scale * scale }],
    } } });
    const drop = .06 * scale;
    logs.push({ message: 'dc.case', data: { caseId, resultId: id, maxAreaMm2: area, loadScale: scale,
      measurements: [{ label: 'USB output', portId: 'port', referenceV: 1, voltage: scalar(1 - drop), drop: scalar(Math.abs(1 - (1 - drop))) }] } });
    for (const [name, status, max] of [[`${caseId}/USB output/drop`, scale === 1 ? 'pass' : 'fail', .1], [`${caseId}/copper-loss`, 'pass', undefined]] as const) {
      checks.push({ name, status, expected: { max } }); expectations.checks.push({ name, status, ...(max === undefined ? {} : { max }) });
    }
    for (const data of Object.values(map)) plots.push({ title: caseId, data });
  }
  return { report: { status: 'complete', reports: [{ calculations, logs, checks, plots }] }, expectations };
}
test('DC audit accepts a conditional snapshot and expected fixed-threshold USB fail followed by maps/check', () => {
  const { report, expectations } = fixture();
  const roundTrip = JSON.parse(JSON.stringify(report));
  assert.equal(auditDcReport(roundTrip, expectations).complete, 4);
});
const corruptions: Record<string, (r: ReturnType<typeof fixture>['report']) => void> = {
  'missing case': r => { r.reports[0].logs.pop(); },
  'duplicate case': r => { r.reports[0].logs[1].data.caseId = r.reports[0].logs[0].data.caseId; },
  'failed attempt': r => { (r.reports[0].calculations as unknown[]).push({ kind: 'sweep-attempt', status: 'failed' }); },
  'missing consumer': r => { r.reports[0].logs[0].data.measurements = []; },
  'wrong consumer': r => { r.reports[0].logs[0].data.measurements[0].label = 'other'; },
  'null measurement': r => { r.reports[0].logs[0].data.measurements[0].voltage.value = null as unknown as number; },
  'unavailable measurement': r => { r.reports[0].logs[0].data.measurements[0].voltage.validity.status = 'unavailable'; },
  'nonfinite measurement': r => { r.reports[0].logs[0].data.measurements[0].voltage.value = NaN; },
  'dangling mesh': r => { Object.assign(r.reports[0].calculations[0].result.solution, { mesh: undefined, meshReference: 'absent' }); },
  'cyclic mesh': r => { const c = r.reports[0].calculations[0].result; Object.assign(c.solution, { mesh: undefined, meshReference: c.id }); },
  'incompatible mesh context': r => { const [a, b] = r.reports[0].calculations.map(c => c.result); b.contextId = 'other'; Object.assign(a.solution, { mesh: undefined, meshReference: b.id }); },
  'missing domain': r => { r.reports[0].calculations[0].result.solution.mesh.domains = []; },
  'field cardinality': r => { r.reports[0].calculations[0].result.solution.domains[0].voltage_v.pop(); },
  'nonfinite field': r => { r.reports[0].calculations[0].result.solution.domains[0].loss_w[0] = Infinity; },
  'triangle index': r => { r.reports[0].calculations[0].result.solution.mesh.domains[0].triangles[0][2] = 999; },
  'drop convergence': r => { const m = r.reports[0].logs[2].data.measurements[0]; m.voltage.value = 1 - .066; m.drop.value = Math.abs(1 - m.voltage.value); },
  'loss convergence': r => { r.reports[0].calculations[2].result.solution.copper_loss_w *= 1.11; },
  'residual': r => { r.reports[0].calculations[0].result.solution.quality.relative_residual = 1e-7; },
  'KCL': r => { Object.assign(r.reports[0].calculations[0].result.solution.quality, { current_balance_a: 1e-10, normalized_current_balance: 1e-7 }); },
  'power': r => { Object.assign(r.reports[0].calculations[0].result.solution.quality, { power_balance_w: 1e-14, normalized_power_balance: 1e-7 }); },
  'nonfinite quality': r => { r.reports[0].calculations[0].result.solution.quality.normalized_power_balance = NaN; },
  'lost raised USB fail': r => { r.reports[0].checks[2].status = 'pass'; },
  'changed USB threshold': r => { r.reports[0].checks[2].expected.max = .2; },
  'lost following loss check': r => { r.reports[0].checks.splice(3, 1); },
  'reordered continuation check': r => { [r.reports[0].checks[2], r.reports[0].checks[3]] = [r.reports[0].checks[3], r.reports[0].checks[2]]; },
  'lost following map': r => { r.reports[0].plots.splice(5, 1); },
  'dangling plot': r => { r.reports[0].plots[0].data.data.solutionId = 'absent'; },
  'wrong map slices': r => { r.reports[0].plots[0].data.data.slices[0].count = 999; },
  'lost barrel currents': r => { r.reports[0].calculations[0].result.solution.barrels = []; },
};
for (const [name, corrupt] of Object.entries(corruptions)) test(`DC audit rejects ${name}`, () => {
  const { report, expectations } = fixture(); corrupt(report);
  assert.throws(() => auditDcReport(report as unknown as AnalysisDslOutput, expectations));
});
test('DC audit uses separate absolute and normalized guards in physical units', () => {
  const { report, expectations } = fixture(), q = report.reports[0].calculations[0].result.solution.quality;
  Object.assign(q, { current_balance_a: 1e-13, normalized_current_balance: 1e-6, power_balance_w: 1e-16, normalized_power_balance: 1e-6 });
  assert.doesNotThrow(() => auditDcReport(report as unknown as AnalysisDslOutput, expectations));
});
