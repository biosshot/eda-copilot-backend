import { AsyncLocalStorage } from 'node:async_hooks';
import type { BlockSolveParams } from './block-solver.ts';
import { applyNativeBoardPackSolution } from './native/apply-board-solution.ts';
import { NATIVE_BLOCK_SOLVE_CONTRACT_VERSION } from './native/contract.ts';
import { encodeNativeBlockSolveProblem } from './native/encode-block-problem.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { cachedNativeSolve } from './native/solve-cache.ts';
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
    blockSolverCapture.getStore()?.(params);
    const primitives = prepareLocalLayoutPrimitives(
        params.node.label,
        params.primitives,
        params.options.componentByDesignator,
        params.options.clearance,
        params.options.clearanceResolver,
    );
    const prepared = primitives === params.primitives ? params : { ...params, primitives };
    const addon = loadNativeBoardPacker();
    if (params.options.experiments && typeof addon.validatePlacementChange !== 'function') {
        throw new Error('Block experiments require the research native addon; run npm run native:build before using this branch');
    }
    const nativeVersion = addon.blockContractVersion();
    if (nativeVersion !== NATIVE_BLOCK_SOLVE_CONTRACT_VERSION) {
        throw new Error(`Rust PCB block solver contract ${nativeVersion} does not match TypeScript contract ${NATIVE_BLOCK_SOLVE_CONTRACT_VERSION}`);
    }
    const problem = encodeNativeBlockSolveProblem(prepared);
    const solution = cachedNativeSolve(addon, 'block', problem, () => addon.solveBlockPrimitives(problem));
    const checkpoints = solution.checkpoints.map(snapshot => ({ stage: snapshot.stage, rank: snapshot.rank,
        primitives: applyNativeBoardPackSolution(primitives, snapshot, NATIVE_BLOCK_SOLVE_CONTRACT_VERSION) }));
    return { checkpoints, result: applyNativeBoardPackSolution(primitives, solution, NATIVE_BLOCK_SOLVE_CONTRACT_VERSION), rank: solution.rank };
}
