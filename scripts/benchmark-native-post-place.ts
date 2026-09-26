import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { refinePostPlacement as reference } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.reference.ts';
import { refinePostPlacement, refinePostPlacementAsync } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import type { PlacementInput, Placement } from '../src/types/pcb/layout-model.ts';

// Keep the real board's geometry/nets/constraints, but bound the experiment's
// candidate set and iterations. This does not apply anything to EasyEDA.
const snapshot = JSON.parse(readFileSync(process.argv[2], 'utf8')) as { input: PlacementInput; placements: Placement[] };
const selected = ['R60', 'R61', 'C70', 'C71', 'R20', 'R21'];
assert(selected.every(name => snapshot.input.components.some(c => c.designator === name)));
snapshot.input.refineGroups = [
    { name: 'probe', componentDesignators: selected, swap: true, rotateBy: [180] },
    { name: 'excluded', componentDesignators: snapshot.input.components.map(c => c.designator).filter(name => !selected.includes(name)), swap: false, rotateBy: [] },
];
snapshot.input.solverOptions.localImproveIterations = 2;
const results: Record<string, unknown> = {};
let expected: unknown;
for (const [name, run] of [
    ['typescript', () => reference(snapshot.input, snapshot.placements)],
    ['rust1', () => refinePostPlacement(snapshot.input, snapshot.placements)],
    ['rust2', () => { process.env.PCB_POST_PLACE_THREADS = '2'; return refinePostPlacementAsync(snapshot.input, snapshot.placements); }],
] as const) {
    const samples = [];
    for (let repeat = 0; repeat < 3; repeat++) {
        const start = performance.now();
        const { profile, ...result } = await run();
        const wallMs = performance.now() - start;
        if (expected) assert.deepEqual(result, expected); else expected = result;
        samples.push({ wallMs, profile });
    }
    const medianWallMs = samples.map(s => s.wallMs).sort((a, b) => a - b)[1];
    results[name] = { medianWallMs, samples };
    console.log(`${name}: median ${medianWallMs.toFixed(1)} ms over 3 runs`);
}
const report = { components: snapshot.input.components.length, selected, maxIterations: 2, identical: true, results };
if (process.argv[3]) writeFileSync(process.argv[3], JSON.stringify(report, null, 2));
