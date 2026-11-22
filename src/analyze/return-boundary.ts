import type { AnalysisReturnBoundary } from '../../native/analyze/index.js';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';

export const RETURN_BOUNDARY_THRESHOLD = 0.01;
export const RETURN_CURRENT_FLOOR_A = 1e-12;

/** The solver clamps the requested guard box strictly inside a shrunken (ROI/corridor) field domain; the audited box
 * is then a sub-box of the requested one. Anything outside the request, or unordered, is invalid. */
function boundsWithin(actual: unknown, requested: number[][]): boolean {
  if (!Array.isArray(actual) || actual.length !== 2 || actual.some(r => !Array.isArray(r) || r.length !== 3 || r.some(v => !Number.isFinite(v)))) return false;
  const a = actual as number[][];
  return [0, 1, 2].every(k => a[0][k] < a[1][k] && a[0][k] >= requested[0][k] && a[1][k] <= requested[1][k]);
}

/** Check independently recomputable fields before trusting native/cache evidence. */
export function validateReturnBoundary(value: AnalysisReturnBoundary | undefined, bounds: unknown): void {
  const fail = (): never => { throw new Error('CHANNEL_RETURN_EVIDENCE: invalid internal boundary-current evidence'); };
  if (bounds === undefined) {
    if (value !== undefined) fail();
    return;
  }
  if (!Array.isArray(bounds) || bounds.length !== 2 || bounds.some(row =>
    !Array.isArray(row) || row.length !== 3 || row.some(v => !Number.isFinite(v))) ||
    bounds[0].some((v: number, axis: number) => v >= bounds[1][axis])) fail();
  if (!value || value.policy !== 'internal-guard-flux-v1' ||
    !boundsWithin(value.boundsMm, bounds as number[][]) ||
    value.absoluteFloorA !== RETURN_CURRENT_FLOOR_A || value.threshold !== RETURN_BOUNDARY_THRESHOLD ||
    ![value.conductiveA, value.displacementA].every(face => Array.isArray(face) && face.length === 6 &&
      face.every(current => Number.isFinite(current) && current >= 0)) ||
    ![value.portCurrentA, value.unresolvedA, value.relative].every(v => Number.isFinite(v) && v >= 0)) return fail();
  const sum = [...value.conductiveA, ...value.displacementA].reduce((total, current) => total + current, 0);
  const relative = sum / Math.max(value.portCurrentA, RETURN_CURRENT_FLOOR_A);
  const matches = (a: number, b: number) => Math.abs(a - b) <= 64 * Number.EPSILON * Math.max(Math.abs(a), Math.abs(b), Number.MIN_VALUE);
  if (!Number.isFinite(sum) || !Number.isFinite(relative) || !matches(value.unresolvedA, sum) ||
    !matches(value.relative, relative) ||
    value.passed !== (value.portCurrentA > RETURN_CURRENT_FLOOR_A && relative < RETURN_BOUNDARY_THRESHOLD)) fail();
}

export function summarizeReturnBoundary(report: BoardFoilReport, problem: BoardFoilProblem) {
  const bounds = (problem.geometry as {returnBoundaryMm?:unknown})?.returnBoundaryMm;
  if (bounds === undefined) throw new Error('CHANNEL_RETURN_EVIDENCE: missing observation box');
  let passed = true;
  let maximumRelative = 0;
  let maximumUnresolvedA = 0;
  let minimumPortCurrentA = Infinity;
  for (const sample of report.samples) for (const column of sample.columns) {
    validateReturnBoundary(column.returnBoundary, bounds);
    const flux = column.returnBoundary!;
    passed &&= flux.passed;
    maximumRelative = Math.max(maximumRelative, flux.relative);
    maximumUnresolvedA = Math.max(maximumUnresolvedA, flux.unresolvedA);
    minimumPortCurrentA = Math.min(minimumPortCurrentA, flux.portCurrentA);
  }
  if (!Number.isFinite(minimumPortCurrentA)) throw new Error('CHANNEL_RETURN_EVIDENCE: no checked drives');
  return {passed, maximumRelative, maximumUnresolvedA, minimumPortCurrentA, frequenciesHz: report.frequenciesHz};
}
