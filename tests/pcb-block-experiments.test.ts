import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { NativeBlockSolveProblemV2 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';
import { blockPolicy } from '../src/pcb-layout/pcb-auto-place-v2/block-policy.ts';

const read = (fixture: string, file: number) => JSON.parse(readFileSync(new URL(`./fixtures/block-placement/${fixture}/block-${file}.json`, import.meta.url), 'utf8'));
const addon = loadNativeBoardPacker();
const usb = read('Telemetry', 23);
const solve = (experiments = {}, width = 1) => addon.solveBlockPrimitives({ ...structuredClone(usb.problem), experiments, searchWidth: width } as NativeBlockSolveProblemV2);

test('experiments are opt-in: captured USB baseline remains identical', () => {
    assert.deepEqual(solve(), usb.solution);
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
