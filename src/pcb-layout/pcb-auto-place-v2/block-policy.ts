import type { NativeBlockSolveProblemV3 } from './native/contract.ts';

export type BlockExperiments = NonNullable<NativeBlockSolveProblemV3['experiments']>;

export function placementPadCrossingWeight() { return 180; }

/** One production policy on the research branch. Search parameters are explicit
 * in the native input/cache; obsolete A/B environment switches are not read. */
export function blockPolicy(ignoredNets: string[] = []) {
    const experiments: BlockExperiments = {
        netCandidates: true, stableNetWeight: true, reducedHull: true, smoothAspect: true,
        frontierOrder: true, padOwnerCandidates: true, localAccess: true,
        orderEqualCritical: true, orderCoreAffinity: true, orderBranching: true, orderScarcity: true,
        candidateClearance: true, candidateRings: true, padCrossings: true,
        longNets: true, extraPasses: true, pairSwaps: true, reinsertPair: true, keepDenseAccess: true,
        ignoredNets, routingMetric: 'micro',
    };
    return { searchWidth: 4, postRefine: true, experiments, portfolio: true };
}
