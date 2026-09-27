import type { NativeBlockSolveProblemV2 } from './native/contract.ts';

export type BlockExperiments = NonNullable<NativeBlockSolveProblemV2['experiments']>;

export function placementPadCrossingWeight() {
    const flag = process.env.PCB_PLACEMENT_PAD_CROSSINGS ?? '1';
    if (!['0', '1'].includes(flag)) throw new Error(`Unknown PCB_PLACEMENT_PAD_CROSSINGS: ${flag}`);
    return process.env.PCB_BLOCK_PROFILE === 'legacy' || flag === '0' ? 0 : 180;
}

/** Research-branch defaults, also used by the ordinary tree solver (no test hooks).
 * Explicit input parameters keep native caching valid. Environment switches are
 * a process-level A/B control and are inherited by subtree workers.
 */
export function blockPolicy(ignoredNets: string[] = []) {
    const flag = (name: string, fallback = '1') => {
        const value = process.env[name] ?? fallback;
        if (!['0', '1'].includes(value)) throw new Error(`Unknown ${name}: ${value}`);
        return value === '1';
    };
    const profile = process.env.PCB_BLOCK_PROFILE ?? 'full';
    if (!['full', 'legacy'].includes(profile)) throw new Error(`Unknown PCB_BLOCK_PROFILE: ${profile}`);
    const candidates = process.env.PCB_BLOCK_CANDIDATES ?? '2';
    if (!['0', '1', '2'].includes(candidates)) throw new Error(`Unknown PCB_BLOCK_CANDIDATES: ${candidates}`);
    const postRefine = process.env.PCB_BLOCK_POST_REFINE ?? '1';
    if (!['0', '1'].includes(postRefine)) throw new Error(`Unknown PCB_BLOCK_POST_REFINE: ${postRefine}`);
    const routingMetric = process.env.PCB_BLOCK_ROUTING ?? 'micro';
    if (!['micro', 'off', 'geometric'].includes(routingMetric)) throw new Error(`Unknown PCB_BLOCK_ROUTING: ${routingMetric}`);
    const experiments: BlockExperiments = profile === 'legacy' ? {} : {
        netCandidates: true, stableNetWeight: true, reducedHull: true, smoothAspect: true,
        frontierOrder: flag('PCB_BLOCK_FRONTIER'), padOwnerCandidates: flag('PCB_BLOCK_PAD_OWNER'),
        localAccess: flag('PCB_BLOCK_LOCAL_ACCESS'),
        orderEqualCritical: flag('PCB_BLOCK_ORDER_EQUAL', '0'),
        orderCoreAffinity: flag('PCB_BLOCK_ORDER_CORE', '0'),
        orderBranching: flag('PCB_BLOCK_ORDER_BRANCH', '0'),
        orderScarcity: flag('PCB_BLOCK_ORDER_SCARCITY', '0'),
        candidateClearance: candidates !== '0', candidateRings: candidates === '2',
        padCrossings: placementPadCrossingWeight() > 0,
        longNets: true, extraPasses: true, pairSwaps: true, reinsertPair: true, keepDenseAccess: true,
    };
    const portfolio = process.env.PCB_BLOCK_PORTFOLIO ?? '1';
    if (!['0', '1', '2'].includes(portfolio)) throw new Error(`Unknown PCB_BLOCK_PORTFOLIO: ${portfolio}`);
    return { searchWidth: profile === 'legacy' ? 1 : 4,
        postRefine: profile !== 'legacy' && postRefine === '1',
        experiments: { ...experiments, ignoredNets, routingMetric: routingMetric as 'micro' | 'off' | 'geometric' },
        portfolio: profile !== 'legacy' && portfolio !== '0',
        repack: profile !== 'legacy' && portfolio === '2' };
}
