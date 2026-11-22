import assert from 'node:assert/strict';
import test from 'node:test';
import { createAnalysisApi } from '../src/analyze/analysis-dsl/context.ts';
import { value } from '../src/analyze/analysis-dsl/data.ts';

const limits = { maxEntries: 1000, maxOutputBytes: 1e6 };
const analysis = () => createAnalysisApi({}, limits, () => {}).globals.analysis();
const scalar = (v: number, unit = 'Ohm') => value({ kind: 'scalar', unit, value: v, validity: { status: 'valid' }, diagnostics: [] });
type Best = { value: number; metric: number; residual: number };
type OptBest = { parameters: Record<string, number>; objective: number; feasible: boolean };
const statuses = (history: readonly unknown[]) => history.map(h => (h as { status: string }).status);

test('solve finds a linear root within tolerance', async () => {
  const r = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 10 }, evaluate: x => scalar(3 * x + 1), target: 10, tolerance: 1e-9, unit: 'Ohm', maxEvaluations: 200 });
  assert.equal(r.status, 'found');
  assert.ok(Math.abs((r.best as Best).value - 3) < 1e-6);
});

test('solve finds the nonlinear root of x^2 = 2 on [0,2]', async () => {
  const r = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 2 }, evaluate: x => scalar(x * x), target: 2, tolerance: 1e-6, unit: 'Ohm', maxEvaluations: 100 });
  assert.equal(r.status, 'found');
  assert.ok(Math.abs((r.best as Best).value - Math.SQRT2) < 1e-5);
  assert.ok(r.history.length <= 100);
});

test('solve with explicit values picks the smallest residual and reports not-found otherwise', async () => {
  const found = await analysis().solve({ parameter: { name: 'x', unit: 'mm', values: [1, 2, 3] }, evaluate: x => scalar(10 * x), target: 21, tolerance: 1.5, unit: 'Ohm', maxEvaluations: 10 });
  assert.equal(found.status, 'found');
  assert.equal((found.best as Best).value, 2);
  assert.equal(found.history.length, 3);
  const missed = await analysis().solve({ parameter: { name: 'x', unit: 'mm', values: [1, 2, 3] }, evaluate: x => scalar(10 * x), target: 100, tolerance: 1, unit: 'Ohm', maxEvaluations: 10 });
  assert.equal(missed.status, 'not-found');
  assert.equal((missed.best as Best).value, 3);
  const partial = await analysis().solve({ parameter: { name: 'x', unit: 'mm', values: [1, 2, 3] }, evaluate: x => scalar(10 * x), target: 21, tolerance: 1, unit: 'Ohm', maxEvaluations: 1 });
  assert.equal(partial.status, 'budget-exhausted');
});

test('solve reports not-found with the closest point and a note when no root is bracketed', async () => {
  const r = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 2 }, evaluate: x => scalar(x * x), target: 10, tolerance: 1e-6, unit: 'Ohm', maxEvaluations: 50 });
  assert.equal(r.status, 'not-found');
  assert.equal((r.best as Best).value, 2);
  assert.match(String(r.note), /not guaranteed/);
});

test('solve with a tiny budget stops at budget-exhausted without exceeding it', async () => {
  let calls = 0;
  const r = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 2 }, evaluate: x => { calls++; return scalar(x * x); }, target: 2, tolerance: 1e-9, unit: 'Ohm', maxEvaluations: 2 });
  assert.equal(r.status, 'budget-exhausted');
  assert.equal(calls, 2);
  assert.ok(r.best !== undefined);
});

test('solve records one throw as failed and keeps searching; all throws give failed', async () => {
  let n = 0;
  const once = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 2 }, evaluate: x => { n++; if (n === 1) throw new Error('boom'); return scalar(x); }, target: 1, tolerance: 1e-6, unit: 'Ohm', maxEvaluations: 30 });
  assert.equal(once.status, 'found');
  assert.equal(statuses(once.history).filter(s => s === 'failed').length, 1);
  assert.match(JSON.stringify(once.history), /boom/);
  const all = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 2 }, evaluate: () => { throw new Error('always'); }, target: 1, tolerance: 1e-6, unit: 'Ohm', maxEvaluations: 20 });
  assert.equal(all.status, 'failed');
  assert.ok(all.history.length > 0 && statuses(all.history).every(s => s === 'failed'));
});

test('solve refuses a unit mismatch as a failed evaluation', async () => {
  const r = await analysis().solve({ parameter: { name: 'x', unit: 'mm', min: 0, max: 2 }, evaluate: x => scalar(x, 'mm'), target: 1, tolerance: 1e-6, unit: 'Ohm', maxEvaluations: 20 });
  assert.equal(r.status, 'failed');
  assert.match(JSON.stringify(r.history), /unit mismatch: expected Ohm, got mm/);
});

test('solve still rejects a parameter that is both interval and explicit values', async () => {
  await assert.rejects(analysis().solve({ parameter: { name: 'x', min: 0, max: 2, values: [1] } as never, evaluate: x => scalar(x), target: 1, tolerance: 1e-6, unit: 'Ohm', maxEvaluations: 5 }));
});

// Quadratic bowl minimum at (3,1); constraint x <= 1 moves the constrained optimum to the boundary x = 1.
const bowl = ({ x, y }: Readonly<Record<string, number>>) => ({ loss: scalar((x - 3) ** 2 + (y - 1) ** 2, 'dB'), xv: scalar(x, 'mm') });
const bowlOptions = (seed = 7, maxEvaluations = 400) => ({
  parameters: { x: { min: -5, max: 5 }, y: { min: -5, max: 5 } },
  evaluate: bowl,
  objective: { metric: 'loss', direction: 'minimize' as const, unit: 'dB' },
  constraints: [{ metric: 'xv', max: 1, unit: 'mm' }],
  maxEvaluations, seed,
});

test('optimize finds the constrained boundary optimum of a quadratic bowl', async () => {
  const r = await analysis().optimize(bowlOptions());
  assert.equal(r.status, 'found');
  const best = r.best as OptBest;
  assert.ok(Math.abs(best.parameters.x! - 1) < 0.05, JSON.stringify(best));
  assert.ok(Math.abs(best.parameters.y! - 1) < 0.05, JSON.stringify(best));
  assert.ok(best.parameters.x! <= 1 + 1e-12);
  assert.equal(best.feasible, true);
  assert.ok(r.violations.length > 0);
  assert.match(String(r.note), /not a global-optimum claim/);
});

test('optimize maximize direction finds the peak of a concave objective', async () => {
  const r = await analysis().optimize({
    parameters: { x: { min: 0, max: 10 } },
    evaluate: ({ x }) => ({ gain: scalar(10 - (x - 6) ** 2, 'dB') }),
    objective: { metric: 'gain', direction: 'maximize', unit: 'dB' },
    constraints: [], maxEvaluations: 200, seed: 3,
  });
  assert.equal(r.status, 'found');
  const best = r.best as OptBest;
  assert.ok(Math.abs(best.parameters.x! - 6) < 0.05, JSON.stringify(best));
  assert.ok(Math.abs(best.objective - 10) < 0.01);
});

test('optimize reports not-found with violations when no point is feasible', async () => {
  const r = await analysis().optimize({ ...bowlOptions(1, 60), constraints: [{ metric: 'xv', min: 100, unit: 'mm' }] });
  assert.equal(r.status, 'not-found');
  assert.equal(r.best, undefined);
  assert.ok(r.violations.length > 0);
});

test('optimize is deterministic for a given seed', async () => {
  const first = await analysis().optimize(bowlOptions(42, 150));
  const second = await analysis().optimize(bowlOptions(42, 150));
  assert.equal(JSON.stringify(first.history), JSON.stringify(second.history));
  assert.equal(JSON.stringify(first.best), JSON.stringify(second.best));
});

test('optimize with discrete values only evaluates listed levels', async () => {
  const r = await analysis().optimize({
    parameters: { n: { values: [1, 2, 4, 8] } },
    evaluate: ({ n }) => ({ loss: scalar((n - 5) ** 2, 'dB') }),
    objective: { metric: 'loss', direction: 'minimize', unit: 'dB' },
    constraints: [], maxEvaluations: 20, seed: 9,
  });
  assert.equal(r.status, 'found');
  assert.equal((r.best as OptBest).parameters.n, 4);
  for (const h of r.history) assert.ok([1, 2, 4, 8].includes((h as { parameters: { n: number } }).parameters.n));
});

test('optimize treats missing or unit-mismatched objective as failed evaluations', async () => {
  const missing = await analysis().optimize({ ...bowlOptions(1, 10), evaluate: () => ({ xv: scalar(0, 'mm') }) });
  assert.equal(missing.status, 'failed');
  const mismatch = await analysis().optimize({ ...bowlOptions(1, 10), evaluate: ({ x }) => ({ loss: scalar(x, 'V'), xv: scalar(x, 'mm') }) });
  assert.equal(mismatch.status, 'failed');
  assert.match(JSON.stringify(mismatch.history), /unit mismatch: expected dB, got V/);
});
