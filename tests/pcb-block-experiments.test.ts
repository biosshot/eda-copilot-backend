import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { NativeBlockSolveProblemV2 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';

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
    const captured = read('ESPower', 0);
    const solution = addon.solveBlockPrimitives({ ...captured.problem, searchWidth: 4,
        experiments: { netCandidates: true, pairSwaps: true, reinsertPair: true, keepDenseAccess: true } });
    assert.deepEqual(solution.states, captured.solution.states);
});

test('unknown experiment flags fail instead of silently running the wrong variant', () => {
    assert.throws(() => solve({ netCandidateTypo: true }), /unknown field/);
});
