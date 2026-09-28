import { AsyncLocalStorage } from 'node:async_hooks';
import { availableParallelism } from 'node:os';
import type { BlockSolveParams } from './block-solver.ts';
import { applyNativeBoardPackSolution } from './native/apply-board-solution.ts';
import { NATIVE_BLOCK_SOLVE_CONTRACT_VERSION } from './native/contract.ts';
import { encodeNativeBlockSolveProblem } from './native/encode-block-problem.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { cachedNativeSolveMany } from './native/solve-cache.ts';
import type { PlacementPrimitive } from './primitives.ts';
import { prepareLocalLayoutPrimitives } from './local-layout.ts';

const blockSolverCapture = new AsyncLocalStorage<(params: BlockSolveParams) => void>();

export function withBlockSolverCapture<T>(capture: (params: BlockSolveParams) => void, run: () => T): T {
    return blockSolverCapture.run(capture, run);
}

export function solveBlockPrimitives(params: BlockSolveParams): PlacementPrimitive[] {
    return solveBlockPrimitivesRust(params).result;
}

export function solveBlockPrimitivesRust(params: BlockSolveParams) {
    return solveBlockHypothesesRust([params])[0];
}

export function solveBlockHypothesesRust(hypotheses: BlockSolveParams[]) {
    if (!hypotheses.length) return [];
    const addon = loadNativeBoardPacker();
    const nativeVersion = addon.blockContractVersion();
    if (nativeVersion !== NATIVE_BLOCK_SOLVE_CONTRACT_VERSION) {
        throw new Error(`Rust PCB block solver contract ${nativeVersion} does not match TypeScript contract ${NATIVE_BLOCK_SOLVE_CONTRACT_VERSION}`);
    }
    const preparedHypotheses = hypotheses.map(params => {
        blockSolverCapture.getStore()?.(params);
        const primitives = prepareLocalLayoutPrimitives(
            params.node.label,
            params.primitives,
            params.options.componentByDesignator,
            params.options.clearance,
            params.options.clearanceResolver,
        );
        const prepared = primitives === params.primitives ? params : { ...params, primitives };
        if (params.options.experiments && typeof addon.validatePlacementChange !== 'function') {
            throw new Error('Block experiments require the research native addon; run npm run native:build before using this branch');
        }
        return { primitives, problem: encodeNativeBlockSolveProblem(prepared) };
    });
    const limit = Math.max(1, Math.min(8, Math.floor(availableParallelism() / 2)));
    const requested = Number(process.env.PCB_BOARD_PACKER_THREADS ?? limit);
    const budget = Number.isFinite(requested) && requested > 0 ? Math.min(limit, Math.floor(requested)) : limit;
    // Share the CPU budget with opt-in subtree processes instead of multiplying it.
    const subtreeWorkers = Math.max(1, Math.min(limit, Number(process.env.PCB_LAYOUT_SUBTREE_WORKERS) || 1));
    const threads = Math.max(1, Math.floor(budget / subtreeWorkers));
    const solutions = cachedNativeSolveMany(addon, 'block', preparedHypotheses.map(p => p.problem), misses =>
        threads > 1 && misses.length > 1 && addon.solveBlockPrimitivesBatch
            ? addon.solveBlockPrimitivesBatch(misses, threads)
            : misses.map(problem => addon.solveBlockPrimitives(problem)));
    return solutions.map((solution, index) => {
        const { primitives } = preparedHypotheses[index];
        const checkpoints = solution.checkpoints.map(snapshot => ({ stage: snapshot.stage, rank: snapshot.rank,
            primitives: applyNativeBoardPackSolution(primitives, snapshot, NATIVE_BLOCK_SOLVE_CONTRACT_VERSION) }));
        return { checkpoints, result: applyNativeBoardPackSolution(primitives, solution, NATIVE_BLOCK_SOLVE_CONTRACT_VERSION), rank: solution.rank };
    });
}
