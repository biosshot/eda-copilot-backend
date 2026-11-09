import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../../native/analyze/index.js';
import { digest, magneticAxialTetPolicy, type LocalModelCache } from './local-model-cache.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';

type Budget = AnalysisBoardFoilCheckedRequest['budget'];
type Entry = { report: AnalysisBoardFoilCheckedReport; producerBudget: Budget };

/** Bump whenever the shared planar/hybrid operator changes its numerical
 * meaning. Kept in the existing exact-grid identity so old snapshots miss. */
export const HYBRID_PLANAR_OPERATOR_VERSION = 'hybrid-planar-v18';

function validBudget(budget: Budget) {
  return Number.isFinite(budget?.maxSeconds) && budget.maxSeconds > 0 && budget.maxSeconds <= 3600 &&
    Number.isFinite(budget?.maxMemoryMiB) && budget.maxMemoryMiB >= 1 && budget.maxMemoryMiB <= 65536;
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
  const hybridProfile = operatorPolicy === 'hybrid-planar-v3';
  const profiled = operatorPolicy === 'skin-volume-v3' || hybridProfile;
  const limitHz = hybridProfile ? 1e10 : profiled ? 100_000 : 10_000;
  const upperHz = profileUpperHz ?? frequenciesHz.at(-1);
  const portCount = expected.electrodePairs.length;
  const referenceOhm = request.referenceOhm ?? expected.electrodePairs.map(() => 50);
  const externalResources = partitionPolicy === 'automatic-hybrid-v1';
  if (!externalResources && !validBudget(request.budget)) throw new Error('BOARD_FOIL_BUDGET: finite positive time and 1..65536 MiB required');
  if ((partitionPolicy !== undefined && partitionPolicy !== 'automatic-conductive-schur-v1' && partitionPolicy !== 'automatic-hybrid-v1') ||
      (operatorPolicy !== undefined && !profiled) ||
      (hybridProfile && partitionPolicy !== 'automatic-hybrid-v1') ||
      (profileUpperHz !== undefined && (!profiled || !Number.isFinite(profileUpperHz) ||
        profileUpperHz < frequenciesHz[frequenciesHz.length - 1] || profileUpperHz > limitHz)) ||
      portCount < 1 || portCount > 16 || !frequenciesHz.length || frequenciesHz.length * portCount > 256 ||
      frequenciesHz.some((f, i) => !Number.isFinite(f) || f < 1000 || f > limitHz || i > 0 && f <= frequenciesHz[i - 1]) ||
      referenceOhm.length !== portCount || referenceOhm.some(r => !Number.isFinite(r) || r <= 0 || !Number.isFinite(1 / r)) ||
      !Number.isSafeInteger(maxIterations) || maxIterations < 1 || maxIterations > 16) {
    throw new Error(`BOARD_FOIL_GRID: bounded ordered 1..${limitHz / 1000} kHz exact grid, valid policy/profile and positive references required`);
  }
  const deadlineSeconds = externalResources ? request.budget.maxSeconds : Math.min(request.budget.maxSeconds, verificationLimit('P7_VERIFY_SECONDS', 300, 1, 3600));
  const maxMemoryMiB = externalResources ? request.budget.maxMemoryMiB : Math.min(Math.floor(request.budget.maxMemoryMiB), verificationLimit('P7_VERIFY_MEMORY_MIB', 2048, 256, 65536));
  const magneticBatchColumns = verificationLimit('P7_MAGNETIC_BATCH_COLUMNS', 0, 0, 64);
  const sectionHints = process.env.P7_SECTION_HINTS === '1';
  const nearBound = process.env.P7_MAGNETIC_NEAR_BOUND === '1';
  const compactPlanar = process.env.P7_COMPACT_PLANAR === '1';
  const compactWorkers = compactPlanar || partitionPolicy==='automatic-hybrid-v1' ? verificationLimit('P7_COMPACT_PL_THREADS',1,1,4) : 1;
  const localNearWorkers = compactPlanar || partitionPolicy==='automatic-hybrid-v1' ? verificationLimit('P7_MAGNETIC_NEAR_THREADS',1,1,4) : 1;
  const compactFullMixed = process.env.P7_COMPACT_FULL_MIXED === '1';
  const axialTetPolicy = magneticAxialTetPolicy();
  const remaining = () => {
    if (externalResources) return deadlineSeconds;
    const seconds = deadlineSeconds - (performance.now() - started) / 1000;
    if (seconds <= 0) throw new Error('CHANNEL_FOIL_DEADLINE: shared cache/solve budget exhausted');
    return seconds;
  };
  // Bound the encoded output before reading/parsing it and allow headroom for
  // UTF-16 strings, parsed copies and result transport. This is a size
  // admission rule, independent of the recorded native plan and process RSS.
  const maxEncodedBytes = externalResources ? Infinity : Math.floor(maxMemoryMiB * 1048576 / 16);
  remaining();
  const key = cache.key({
    core, producer: 'automatic-board-foil-or-mixed-exact-grid-v1',
    hybridPlanarOperatorVersion: HYBRID_PLANAR_OPERATOR_VERSION,
    ...(process.env.P7_HYBRID_REGIME_AUDIT==='1'&&{regimeControlPolicy:'discrete-dc-ac-time-v1'}),
    compactPlanarPolicy: compactPlanar ? 'compact-v1' : 'default-v1',
    ...((compactPlanar || partitionPolicy==='automatic-hybrid-v1') && { compactWorkers, localNearWorkers }),
    ...(compactPlanar && { compactBasisPolicy: compactFullMixed ? 'adaptive-mixed-closure-v1' : 'dc-harmonic-only' }),
    board: digest(problem.boardJson),
    magneticProducerPolicy:'exact-or-certified-adaptive-harmonic-v1',
    magneticRepresentationVersion:3,
    magneticInteractionStorage:'immutable-adaptive-dual-tree-v1',
    ...(magneticBatchColumns > 0 && { magneticBatchColumns }),
    ...(sectionHints && { sectionHintPolicy: 'uniform-axial-current-seeds-v1' }),
    ...(nearBound && { nearBoundPolicy: 'summed-per-tetra-volume-v1' }),
    ...(axialTetPolicy && { nearQuadraturePolicy: axialTetPolicy }),
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
    if (!entry || (!externalResources && !validBudget(entry.producerBudget))) throw new Error('CHANNEL_FOIL_CACHE: invalid original producer budget');
    validateBoardFoilReport(entry.report, problem, entry.producerBudget);
  }, maxEncodedBytes);
  remaining();
  return { ...cached.value.report, cache: {
    ...cached.evidence, policy: 'exact-grid-result',
    ...(externalResources ? { resourcePolicy: 'external' as const } : {
      producerBudget: cached.value.producerBudget, replayBudget: { maxSeconds: deadlineSeconds, maxMemoryMiB },
    }),
    elapsedSeconds: (performance.now() - started) / 1000,
  } };
}
