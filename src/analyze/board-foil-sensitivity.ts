import { digest } from './local-model-cache.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';

const policy = {
  passRelativeChange: 0.02,
  reviewRelativeChange: 0.035,
  waveAmplitudeFloor: 0.01,
  lossFloorW: 1e-18,
  normalization: 'Complex S50 column L2 change / max(variant column L2, 0.01); loss change / max(variant loss, 1e-18 W). Floors stabilize sensitivity ratios for weak outputs.',
} as const;
const variants = [
  { kind: 'mesh', parameter: 'maxTriangleAreaM2', factor: 0.5 },
  { kind: 'exterior-domain', parameter: 'airMarginM', factor: 2 },
] as const;
type Variant = typeof variants[number];
type Status = 'pass' | 'requires-review' | 'unverified';
type Budget = BoardFoilProblem['request']['budget'];
type Location = { frequencyHz: number; input: number; packageId?: string };
type Change = Location & { absoluteChange: number; denominator: number; relativeChange: number };

function sensitivity(baseline: BoardFoilReport, variant: BoardFoilReport) {
  const worst: Partial<Record<'s50Column' | 'copperLoss' | 'packageLoss', Change>> = {};
  const record = (key: keyof typeof worst, location: Location, absoluteChange: number, denominator: number) => {
    const relativeChange = absoluteChange / denominator;
    if (!Number.isFinite(relativeChange)) throw new Error('CHANNEL_FOIL_AUDIT_RANGE: nonfinite normalized change');
    if (worst[key] === undefined || relativeChange > worst[key]!.relativeChange) {
      worst[key] = { ...location, absoluteChange, denominator, relativeChange };
    }
  };
  for (const [f, sample] of variant.samples.entries()) for (const [input, column] of sample.columns.entries()) {
    const location = { frequencyHz: sample.frequencyHz, input };
    const matrix = variant.parameters[f];
    const before = baseline.parameters[f];
    const norm = Math.hypot(...matrix.flatMap(row => [row[input].real, row[input].imaginary]));
    const difference = Math.hypot(...matrix.flatMap((row, output) => [
      row[input].real - before[output][input].real, row[input].imaginary - before[output][input].imaginary,
    ]));
    record('s50Column', location, difference, Math.max(policy.waveAmplitudeFloor, norm));
    const original = baseline.samples[f].columns[input];
    record('copperLoss', location, Math.abs(column.copperLossW - original.copperLossW),
      Math.max(policy.lossFloorW, column.copperLossW));
    for (const [i, item] of (column.packages ?? []).entries()) {
      record('packageLoss', { ...location, packageId: item.id }, Math.abs(item.lossW - original.packages![i].lossW),
        Math.max(policy.lossFloorW, item.lossW));
    }
  }
  return { worst, maxRelativeChange: Math.max(...Object.values(worst).map(change => change.relativeChange)) };
}

function producer(geometry: unknown, report: BoardFoilReport) {
  const { modelKind, plannedBytes, prisms } = report.provenance;
  return { geometrySha256: digest(JSON.stringify(geometry)), modelKind, prisms, plannedBytes,
    ...(report.provenance.modelKind === 'mixed' && { volumeTetrahedra: report.provenance.volumeTetrahedra }),
    cache: report.cache, quality: report.quality };
}

function runVariant(
  variant: Variant, problem: BoardFoilProblem, baseline: BoardFoilReport, remainingBudget: () => Budget,
  execute: (problem: BoardFoilProblem) => BoardFoilReport,
) {
  const started = performance.now();
  const geometry = problem.geometry as Record<string, unknown>;
  const from = geometry[variant.parameter];
  const to = typeof from === 'number' ? from * variant.factor : undefined;
  const evidence = { kind: variant.kind, parameter: variant.parameter, from, to };
  const elapsed = () => (performance.now() - started) / 1000;
  try {
    if (typeof to !== 'number' || !Number.isFinite(to) || to <= 0 || to === from) {
      throw new Error('CHANNEL_FOIL_AUDIT_INPUT: positive finite geometry perturbation required');
    }
    const changed: BoardFoilProblem = { ...problem, geometry: { ...geometry, [variant.parameter]: to },
      request: { ...problem.request, budget: remainingBudget() } };
    const report = execute(changed);
    validateBoardFoilReport(report, changed, report.cache?.producerBudget ?? changed.request.budget);
    remainingBudget();
    const comparison = sensitivity(baseline, report);
    const { maxRelativeChange } = comparison;
    const status: Status = maxRelativeChange <= policy.passRelativeChange ? 'pass' :
      maxRelativeChange <= policy.reviewRelativeChange ? 'requires-review' : 'unverified';
    return { ...evidence, ...comparison, status, producer: producer(changed.geometry, report), elapsedSeconds: elapsed(),
      ...(status === 'unverified' && { reason: 'Change exceeds the sensitivity review limit' }) };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const code = message.match(/^([A-Z][A-Z0-9_]+):/)?.[1] ?? 'CHANNEL_FOIL_AUDIT_REFUSED';
    return { ...evidence, status: 'unverified' as const, elapsedSeconds: elapsed(), failure: { code, message } };
  }
}

/** Independent perturbations of the returned baseline under one request deadline.
 * Each exact response still passes the ordinary physical and numerical gates. */
export function auditBoardFoilGrid(
  problem: BoardFoilProblem, baseline: BoardFoilReport, started: number,
  execute: (problem: BoardFoilProblem) => BoardFoilReport,
) {
  const externalResources = problem.request.partitionPolicy === 'automatic-hybrid-v1';
  const limit = Math.min(problem.request.budget.maxSeconds, Number(process.env.P7_VERIFY_SECONDS ?? 300));
  const remainingBudget = () => {
    if (externalResources) return problem.request.budget;
    const maxSeconds = limit - (performance.now() - started) / 1000;
    if (!Number.isFinite(maxSeconds) || maxSeconds <= 0) {
      throw new Error('CHANNEL_FOIL_DEADLINE: shared extraction and sensitivity budget exhausted');
    }
    return { ...problem.request.budget, maxSeconds };
  };
  return {
    policy, frequenciesHz: problem.request.frequenciesHz, baseline: producer(problem.geometry, baseline),
    checks: variants.map(variant => runVariant(variant, problem, baseline, remainingBudget, execute)),
    cutSensitivity: 'not-applicable; joint closed copper has no internal cuts',
    scope: 'Exact-grid sensitivity of matched responses. Physical accuracy, energy and continuous-band coverage remain unverified.',
  };
}
