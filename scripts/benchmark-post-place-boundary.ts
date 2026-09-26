import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { encodeNativePostPlaceScoreProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-score.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { createPostPlaceRouteScoreContext, preparePostPlaceRouteComparison } from '../src/pcb-layout/pcb-auto-place-v2/post-place-route-score.ts';
import type { PlacementInput, Placement } from '../src/types/pcb/layout-model.ts';

// This probe never starts the placement solver or creates workers.
const dir = resolve('../pcb/portablescope-placement');
const { input, placements, provenance }: { input: PlacementInput; placements: Placement[]; provenance: string } =
    JSON.parse(readFileSync(resolve(dir, 'post-place-probe-input.json'), 'utf8'));
const addon = loadNativeBoardPacker();
const probe = createRequire(import.meta.url)(resolve(dir, 'post-place-probe.node'));
const context = createPostPlaceRouteScoreContext(input);

function encodeRoute(poses: Placement[], changed: Set<string>) {
    let captured: { problem: any; obstacles: any; changedIds: string[] } | undefined;
    const original = addon.prepareRouteLayoutComparison;
    addon.prepareRouteLayoutComparison = (problem, ids, obstacles) => {
        captured = { problem, obstacles, changedIds: ids };
        return { version: 2, jobs: [], topologyNets: [], maximumImprovement: 0 };
    };
    try { preparePostPlaceRouteComparison(input, poses, changed, context); }
    finally { addon.prepareRouteLayoutComparison = original; }
    assert.ok(captured);
    return captured;
}

function average(count: number, fn: () => unknown) {
    const start = performance.now();
    for (let i = 0; i < count; i++) fn();
    return (performance.now() - start) / count;
}
function median(values: number[]) {
    const sorted = values.toSorted((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
}

const choices = [['R60', 'R61'], ['C70', 'C71'], ['R20', 'R21'], ['U1'], ['U2'], ['U3']];
const samples = [];
const started = performance.now();
for (const ids of choices) {
    if (process.argv.includes('--short') && !['R60', 'U1'].includes(ids[0])) continue;
    if (ids.some(id => !input.components.some(c => c.designator === id))) continue;
    const changed = new Set(ids);
    const variant = placements.map(p => {
        if (!changed.has(p.designator)) return p;
        if (ids.length === 1) return { ...p, rotate: (p.rotate + 180) % 360 };
        const other = placements.find(q => q.designator === ids.find(id => id !== p.designator))!;
        return { ...p, x: other.x, y: other.y, rotate: other.rotate, layer: other.layer };
    });
    const base = encodeRoute(placements, changed);
    const candidate = encodeRoute(variant, changed);
    const score = encodeNativePostPlaceScoreProblem(input, variant);
    const baseline = addon.prepareRouteLayoutComparison(base.problem, base.changedIds, base.obstacles);
    const expected = addon.compareRouteLayoutCandidate(candidate.problem, candidate.obstacles, baseline);
    const expectedScore = addon.scorePostPlace(score);
    const baseContext = new probe.PreparedPostPlaceProbe(base.problem, base.obstacles, score, baseline);
    const candidateContext = new probe.PreparedPostPlaceProbe(candidate.problem, candidate.obstacles, score, baseline);
    assert.deepEqual(baseContext.prepare(base.changedIds, 1).result, baseline);
    assert.deepEqual(candidateContext.compare(1).result, expected);
    assert.equal(candidateContext.score(1).result, expectedScore);
    probe.probeDecodeScore(score);
    probe.probeDecodeRoute(candidate.problem, candidate.obstacles, baseline);

    const rounds = [];
    for (let round = 0; round < 3; round++) {
        // Reverse measurement order to reduce warmup/clock bias.
        const operations: Array<[string, () => number]> = [
            ['scoreCurrentMs', () => average(8, () => addon.scorePostPlace(score))],
            ['scorePreparedMs', () => candidateContext.score(8).computeMs / 8],
            ['scoreDecodeOnlyMs', () => average(8, () => probe.probeDecodeScore(score))],
            ['scoreSerdeInsideMs', () => median(Array.from({ length: 8 }, () => probe.probeDecodeScore(score).insideMs))],
            ['prepareCurrentMs', () => average(3, () => addon.prepareRouteLayoutComparison(base.problem, base.changedIds, base.obstacles))],
            ['preparePreparedMs', () => baseContext.prepare(base.changedIds, 3).computeMs / 3],
            ['compareCurrentMs', () => average(3, () => addon.compareRouteLayoutCandidate(candidate.problem, candidate.obstacles, baseline))],
            ['comparePreparedMs', () => candidateContext.compare(3).computeMs / 3],
            ['compareDecodeOnlyMs', () => average(8, () => probe.probeDecodeRoute(candidate.problem, candidate.obstacles, baseline))],
            ['compareSerdeInsideMs', () => median(Array.from({ length: 8 }, () => probe.probeDecodeRoute(candidate.problem, candidate.obstacles, baseline).insideMs))],
            ['scoreTsEncodingMs', () => average(8, () => encodeNativePostPlaceScoreProblem(input, variant))],
            ['routeTsEncodingMs', () => average(8, () => encodeRoute(variant, changed))],
        ];
        if (round % 2) operations.reverse();
        rounds.push(Object.fromEntries(operations.map(([key, fn]) => [key, fn()])));
    }
    const timings = Object.fromEntries(Object.keys(rounds[0]).map(key => [key, median(rounds.map(r => r[key]))]));
    const sample = { ids, jobs: baseline.jobs.length,
        expandedBefore: baseline.jobs.reduce((sum, job) => sum + job.expanded, 0),
        expandedAfter: expected.jobs.reduce((sum, job) => sum + job.expanded, 0),
        scoreTerms: { nets: score.nets.length, distances: score.distances.length, clearances: score.clearances.length, edges: score.edges.length, paths: score.paths.length },
        routeObjects: { primitives: candidate.problem.primitives.length, obstacles: candidate.obstacles.length },
        scorePayloadBytes: Buffer.byteLength(JSON.stringify(score)),
        routePayloadBytes: Buffer.byteLength(JSON.stringify([candidate.problem, candidate.obstacles, baseline])),
        timings, rounds, identical: true };
    samples.push(sample);
    console.log(JSON.stringify({ ids, jobs: sample.jobs, timings }));
}
assert.ok(samples.length >= (process.argv.includes('--short') ? 2 : 4));
const report = { provenance, components: input.components.length, samples,
    elapsedMs: performance.now() - started, processCpuUsage: process.cpuUsage(),
    limitations: ['Single process, small sample; not a full-refine benchmark.',
        'Prepared computation excludes one-time context construction, pose-update cost and per-call output conversion.',
        'Decode-only includes the N-API input conversion, serde decoding, validation (route) and object destruction.',
        'No process IPC, geometry hard checks or candidate generation measured.'] };
writeFileSync(resolve(dir, process.argv.includes('--short') ? 'post-place-boundary-decode-benchmark.json' : 'post-place-boundary-benchmark.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify({ elapsedMs: report.elapsedMs, samples: samples.length, identical: true }));
