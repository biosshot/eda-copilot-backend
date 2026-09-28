import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { NATIVE_BLOCK_SOLVE_CONTRACT_VERSION, type NativeBlockSolveProblemV4 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';
import { blockPolicy } from '../src/pcb-layout/pcb-auto-place-v2/block-policy.ts';

const read = (fixture: string, file: number) => JSON.parse(readFileSync(new URL(`./fixtures/block-placement/${fixture}/block-${file}.json`, import.meta.url), 'utf8'));
const addon = loadNativeBoardPacker();
const usb = read('Telemetry', 23);
usb.problem.version = NATIVE_BLOCK_SOLVE_CONTRACT_VERSION;
const solve = (experiments = {}, width = 1) => addon.solveBlockPrimitives({ ...structuredClone(usb.problem), experiments, searchWidth: width } as NativeBlockSolveProblemV4);

test('parallel block hypotheses preserve ordered solutions and all checkpoints', () => {
    const problems = [{}, { netCandidates: true }, { pairSwaps: true }].map(experiments =>
        ({ ...structuredClone(usb.problem), experiments, searchWidth: 1 } as NativeBlockSolveProblemV4));
    const expected = problems.map(p => addon.solveBlockPrimitives(p));
    assert.equal(typeof addon.solveBlockPrimitivesBatch, 'function');
    for (const threads of [1, 2, 4]) {
        assert.deepEqual(addon.solveBlockPrimitivesBatch!(problems, threads), expected);
    }
    assert.deepEqual(addon.solveBlockPrimitivesBatch!([], 4), []);
    assert.throws(() => addon.solveBlockPrimitivesBatch!([problems[0], { ...problems[1], version: -1 }], 4), /Invalid/);
});

test('pair continuation resumes singles without repeating beam and preserves the solution', () => {
    const problem = { ...structuredClone(usb.problem), searchWidth: 1, experiments: { pairSwaps: true, reinsertPair: true } };
    const full = addon.solveBlockPrimitives(problem);
    const initial = addon.solveBlockPrimitives({ ...problem, deferPairs: true });
    const resumed = addon.solveBlockPrimitives({ ...problem, pairSeed: initial.pairSeed });
    assert.deepEqual(resumed.checkpoints.map(c => c.stage), ['pairs']);
    assert.equal(resumed.rank.hardCount, full.rank.hardCount);
    assert.ok(Math.abs(resumed.rank.score - full.rank.score) < .001);
    const poses = (r: typeof full) => r.states.flatMap(s => s.placements).sort((a, b) => a.designator.localeCompare(b.designator));
    assert.deepEqual(poses(resumed), poses(full));
    assert.throws(() => addon.solveBlockPrimitives({ ...problem, pairSeed: initial.pairSeed!.slice(1) }), /inventory/);
    const duplicate = [...initial.pairSeed!]; duplicate[1] = duplicate[0];
    assert.throws(() => addon.solveBlockPrimitives({ ...problem, pairSeed: duplicate }), /duplicate/);
    const locked = structuredClone(problem); locked.primitives[0].locked = true;
    const seed = structuredClone(initial.pairSeed!); const p = seed.find(s => s.primitive.id === locked.primitives[0].id)!; p.primitive.placements[0].x += 123;
    assert.throws(() => addon.solveBlockPrimitives({ ...locked, pairSeed: seed }), /locked/);
});

test('experiments are opt-in: captured USB baseline remains identical', () => {
    const { checkpoints, ...result } = solve();
    assert.deepEqual(result, { ...usb.solution, version: NATIVE_BLOCK_SOLVE_CONTRACT_VERSION });
    assert.equal(checkpoints.length, 3);
});

test('ignored ordinary nets produce no extra candidates (case insensitive)', () => {
    const ignoredNets = usb.problem.primitives.flatMap((p: any) => p.connectionPoints.map((cp: any) => cp.net?.toLowerCase())).filter(Boolean);
    assert.deepEqual(solve({ netCandidates: true, ignoredNets }), solve());
});

test('bounded block neighbourhoods preserve hard legality', () => {
    for (const experiments of [{ extraPasses: true }, { pairSwaps: true }, { reinsertPair: true }]) {
        assert.ok(solve(experiments).rank.hardCount <= usb.solution.rank.hardCount);
    }
});

test('new candidates, beam and pair moves preserve a locked mechanical primitive', () => {
    const problem = structuredClone(usb.problem);
    const fixed = problem.primitives.find((p: any) => p.placements.some((c: any) => c.designator === 'U11'));
    fixed.locked = true;
    const solution = addon.solveBlockPrimitives({ ...problem, searchWidth: 4,
        experiments: { netCandidates: true, pairSwaps: true, reinsertPair: true, keepDenseAccess: true } });
    const state = solution.states.find(s => s.primitiveId === fixed.id)!;
    assert.equal(state.rotation, 0);
    assert.equal(state.translationX, 0);
    assert.equal(state.translationY, 0);
    assert.equal(solution.rank.hardCount, 0);
});

test('USB combined experiment shortens local two-terminal connections without overlaps', async () => {
    // Independent geometry measurements, not the score being optimized.
    // @ts-expect-error research harness is a plain ES module
    const { metrics } = await import('../scripts/experiment-block-replay.mjs');
    const candidate = solve({ netCandidates: true, stableNetWeight: true, reducedHull: true,
        smoothAspect: true, longNets: true, pairSwaps: true }, 4);
    const before = metrics(usb.problem, usb.solution), after = metrics(usb.problem, candidate);
    assert.equal(after.hard, 0);
    assert.ok(after.pairSum < before.pairSum * 0.75);
    assert.ok(after.pairMax < 6);
});

test('unknown experiment flags fail instead of silently running the wrong variant', () => {
    assert.throws(() => solve({ netCandidateTypo: true }), /unknown field/);
    assert.throws(() => solve({ routingMetric: 'typo' }), /unknown variant/);
});

test('full branch profile preserves USB hard legality with either alternative route term', () => {
    for (const routingMetric of ['off', 'geometric']) {
        assert.equal(solve({ ...blockPolicy().experiments, routingMetric }, 4).rank.hardCount, 0);
    }
});
