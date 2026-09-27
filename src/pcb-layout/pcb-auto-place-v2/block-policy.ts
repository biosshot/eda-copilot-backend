import type { NativeBlockSolveProblemV2 } from './native/contract.ts';

export type BlockExperiments = NonNullable<NativeBlockSolveProblemV2['experiments']>;

/** Research-branch defaults, also used by the ordinary tree solver (no test hooks).
 * Explicit input parameters keep native caching valid. Environment switches are
 * a process-level A/B control and are inherited by subtree workers.
 */
export function blockPolicy(ignoredNets: string[] = []) {
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
        candidateClearance: candidates !== '0', candidateRings: candidates === '2',
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
