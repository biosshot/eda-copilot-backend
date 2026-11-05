import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../../native/analyze/index.js';
import { digest, type LocalModelCache } from './local-model-cache.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';

type Budget = AnalysisBoardFoilCheckedRequest['budget'];
type Entry = { report: AnalysisBoardFoilCheckedReport; producerBudget: Budget };

function validBudget(budget: Budget) {
  return Number.isFinite(budget?.maxSeconds) && budget.maxSeconds > 0 && budget.maxSeconds <= 3600 &&
    Number.isFinite(budget?.maxMemoryMiB) && budget.maxMemoryMiB >= 1 && budget.maxMemoryMiB <= 4096;
}

function verificationLimit(name: string, fallback: number, min: number, max: number) {
  const env = process.env[name];
  const value = env === undefined ? fallback : /^\+?\d+$/.test(env) ? Number(env) : NaN;
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw new Error('LOCAL_BUDGET: invalid ' + name);
  }
  return value;
}

/** Reuse checked response snapshots only. The producing native solver is
 * neither required nor invoked when reading an immutable exact-grid result. */
export function cachedBoardFoilReport(
  cache: LocalModelCache, core: string, problem: BoardFoilProblem,
  prepare: (request: AnalysisBoardFoilCheckedRequest) => AnalysisBoardFoilCheckedReport,
  started = performance.now(),
): BoardFoilReport {
  const { request, expected } = problem;
  const { frequenciesHz, maxIterations = 16, operatorPolicy, profileUpperHz, partitionPolicy } = request;
  const profiled = operatorPolicy === 'skin-volume-v3';
  const limitHz = profiled ? 100_000 : 10_000;
  const upperHz = profileUpperHz ?? frequenciesHz.at(-1);
  const portCount = expected.electrodePairs.length;
  const referenceOhm = request.referenceOhm ?? expected.electrodePairs.map(() => 50);
  if (!validBudget(request.budget)) throw new Error('BOARD_FOIL_BUDGET: finite positive time and 1..4096 MiB required');
  if ((partitionPolicy !== undefined && partitionPolicy !== 'automatic-conductive-schur-v1') ||
      (operatorPolicy !== undefined && !profiled) ||
      (profileUpperHz !== undefined && (!profiled || !Number.isFinite(profileUpperHz) ||
        profileUpperHz < frequenciesHz[frequenciesHz.length - 1] || profileUpperHz > limitHz)) ||
      portCount < 1 || portCount > 16 || !frequenciesHz.length || frequenciesHz.length * portCount > 256 ||
      frequenciesHz.some((f, i) => !Number.isFinite(f) || f < 1000 || f > limitHz || i > 0 && f <= frequenciesHz[i - 1]) ||
      referenceOhm.length !== portCount || referenceOhm.some(r => !Number.isFinite(r) || r <= 0 || !Number.isFinite(1 / r)) ||
      !Number.isSafeInteger(maxIterations) || maxIterations < 1 || maxIterations > 16) {
    throw new Error(`BOARD_FOIL_GRID: bounded ordered 1..${limitHz / 1000} kHz exact grid, valid policy/profile and positive references required`);
  }
  const deadlineSeconds = Math.min(request.budget.maxSeconds, verificationLimit('P7_VERIFY_SECONDS', 300, 1, 3600));
  const maxMemoryMiB = Math.min(Math.floor(request.budget.maxMemoryMiB), verificationLimit('P7_VERIFY_MEMORY_MIB', 2048, 256, 8192));
  const remaining = () => {
    const seconds = deadlineSeconds - (performance.now() - started) / 1000;
    if (seconds <= 0) throw new Error('CHANNEL_FOIL_DEADLINE: shared cache/solve budget exhausted');
    return seconds;
  };
  // Bound the encoded output before reading/parsing it and allow headroom for
  // UTF-16 strings, parsed copies and result transport. This is a size
  // admission rule, independent of the recorded native plan and process RSS.
  const maxEncodedBytes = Math.floor(maxMemoryMiB * 1048576 / 16);
  remaining();
  const key = cache.key({
    core, producer: 'automatic-board-foil-or-mixed-exact-grid-v1', board: digest(problem.boardJson),
    magneticProducerPolicy:'exact-or-certified-cartesian-tree-v1',
    magneticRepresentationVersion:2,
    magneticInteractionStorage:'immutable-dual-tree-v1',
    geometry: problem.geometry, expected, frequenciesHz, referenceOhm, maxIterations,
    ...(profiled && { operatorPolicy, profileUpperHz: upperHz }),
    ...(partitionPolicy && { partitionPolicy }),
    ...(request.attachments?.length&&{attachments:request.attachments}),
  });
  remaining();
  const cached = cache.obtain<Entry>(key, () => {
    const producerBudget = { maxSeconds: remaining(), maxMemoryMiB };
    const report = prepare({ ...request, referenceOhm, maxIterations, budget: producerBudget });
    remaining();
    return { report, producerBudget };
  }, value => {
    const entry = value as Entry | null;
    if (!entry || !validBudget(entry.producerBudget)) throw new Error('CHANNEL_FOIL_CACHE: invalid original producer budget');
    validateBoardFoilReport(entry.report, problem, entry.producerBudget);
  }, maxEncodedBytes);
  remaining();
  return { ...cached.value.report, cache: {
    ...cached.evidence, policy: 'exact-grid-result', producerBudget: cached.value.producerBudget,
    elapsedSeconds: (performance.now() - started) / 1000,
    replayBudget: { maxSeconds: deadlineSeconds, maxMemoryMiB },
  } };
}
