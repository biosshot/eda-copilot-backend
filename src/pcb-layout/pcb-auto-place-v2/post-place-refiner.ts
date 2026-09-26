import { availableParallelism } from 'node:os';
import { performance } from 'node:perf_hooks';
import type { PlacementInput, Placement } from '#types/pcb/layout-model.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import { encodeNativePostPlaceScoreProblem } from './native/encode-post-place-score.ts';
import { NATIVE_POST_PLACE_SCORE_CONTRACT_VERSION } from './native/contract.ts';
import type { PostPlaceRefineResult } from './post-place-refiner.reference.ts';
export type { PostPlaceMove, PostPlaceRefineResult, PostPlaceProfile } from './post-place-refiner.reference.ts';

function run(input: PlacementInput, placements: Placement[], threads: number): PostPlaceRefineResult {
    const addon = loadNativeBoardPacker();
    if (addon.postPlaceRefineContractVersion() !== 1) throw new Error('Rust post-place refine contract does not match TypeScript contract 1');
    const started = performance.now();
    const problem = encodeNativePostPlaceRefineProblem(input, placements, threads);
    const encodingMs = performance.now() - started;
    const result = addon.refinePostPlacement(problem);
    return { ...result, profile: { ...result.profile, encodingMs } };
}

/** One native call; the synchronous API intentionally evaluates on one Rust thread. */
export function refinePostPlacement(input: PlacementInput, placements: Placement[]): PostPlaceRefineResult {
    return run(input, placements, 1);
}

export async function refinePostPlacementAsync(input: PlacementInput, placements: Placement[], onIteration?: (message: string) => void): Promise<PostPlaceRefineResult> {
    const raw = process.env.PCB_POST_PLACE_THREADS ?? process.env.PCB_BOARD_PACKER_THREADS ?? process.env.PCB_LAYOUT_SUBTREE_WORKERS;
    const limit = Math.max(1, Math.min(8, Math.floor(availableParallelism() / 2)));
    const configured = raw === undefined ? limit : Number(raw);
    const threads = Math.min(limit, Number.isFinite(configured) ? Math.max(1, Math.floor(configured)) : limit);
    onIteration?.(`Post-placement refinement in Rust: up to ${threads} native threads.`);
    return run(input, placements, threads);
}

export function globalPostPlaceScore(input: PlacementInput, placements: Placement[]) {
    const addon = loadNativeBoardPacker();
    if (addon.postPlaceScoreContractVersion() !== NATIVE_POST_PLACE_SCORE_CONTRACT_VERSION) throw new Error('Rust post-place score contract mismatch');
    return addon.scorePostPlace(encodeNativePostPlaceScoreProblem(input, placements));
}
