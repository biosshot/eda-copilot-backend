// Exercise the ordinary production entry point: no replacement of solve inputs.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { autoPlacePcbWithReportAsync, renderPlacementSvg } from '../src/pcb-layout/pcb-auto-place/auto-place.ts';
import { createPlacementDebugArtifacts, writePlacementArtifacts } from '../src/pcb-layout/artifacts.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { globalPostPlaceScore } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { createRequire } from 'node:module';

const [fixture = 'Telemetry', mode = 'full-micro'] = process.argv.slice(2);
const settings = {
    legacy: ['legacy', 'micro', '0'],
    'full-micro': ['full', 'micro', '1'],
    'full-micro-single': ['full', 'micro', '0'],
    'full-off': ['full', 'off', '1'],
    'full-geometric': ['full', 'geometric', '1'],
    'full-geometric-single': ['full', 'geometric', '0'],
    'full-micro-repack': ['full', 'micro', '2'],
    'full-geometric-repack': ['full', 'geometric', '2'],
}[mode];
if (!settings) throw Error(`Unknown mode: ${mode}`);
[process.env.PCB_BLOCK_PROFILE, process.env.PCB_BLOCK_ROUTING, process.env.PCB_BLOCK_PORTFOLIO] = settings;
process.env.PCB_LAYOUT_SUBTREE_WORKERS = '0';
process.env.PCB_POST_PLACE_THREADS = '1';
process.env.PCB_NATIVE_SOLVE_CACHE = '0';
const raw = readFileSync(`tests/fixtures/block-placement/${fixture}/input.json`);
const input = JSON.parse(raw);
const addon = loadNativeBoardPacker();
const solve = addon.solveBlockPrimitives;
const captures = [];
Object.defineProperty(addon, 'solveBlockPrimitives', { configurable: true, value: problem => {
    const start = performance.now();
    const solution = solve(problem);
    captures.push({ problem, solution, ms: performance.now() - start });
    console.error(`block ${captures.length}: ${problem.primitives.map(p => p.label).join(',')} ${captures.at(-1).ms.toFixed(0)}ms`);
    return solution;
} });
const start = performance.now();
const result = await autoPlacePcbWithReportAsync(input);
const elapsed = performance.now() - start;
const tag = process.env.PCB_EXPERIMENT_TAG ?? mode;
const dir = `.test-output/architecture/${fixture}/${tag}`;
mkdirSync(dir, { recursive: true });
writePlacementArtifacts(dir, input, result.placements, result.report, result.layout, result.stages,
    renderPlacementSvg(input, result.placements), createPlacementDebugArtifacts(input, result.placements));
const stageMetrics = result.stages.map(stage => {
    const nets = new Map();
    for (const c of input.components) {
        const p = stage.placements.find(p => p.designator === c.designator);
        for (const pin of c.pins) {
            if (!p || !pin.signal_name || input.solverOptions.ignoredRatsnestSignals.some(s => s.toUpperCase() === pin.signal_name.toUpperCase())) continue;
            const point = getPadWorld(c, p, pin.pin_number);
            if (point) { const pts = nets.get(pin.signal_name) ?? []; pts.push(point); nets.set(pin.signal_name, pts); }
        }
    }
    let hpwl = 0; const pairs = [];
    for (const pts of nets.values()) {
        hpwl += Math.max(...pts.map(p => p.x)) - Math.min(...pts.map(p => p.x)) + Math.max(...pts.map(p => p.y)) - Math.min(...pts.map(p => p.y));
        if (pts.length === 2) pairs.push(Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y));
    }
    return { stage: stage.name, hpwl, pairSum: pairs.reduce((a, b) => a + b, 0), pairMax: Math.max(0, ...pairs), score: globalPostPlaceScore(input, stage.placements) };
});
const fixedChanges = input.components.filter(c => c.pcb.fixedPlacement).flatMap(c => {
    const p = result.placements.find(p => p.designator === c.designator), f = c.pcb.fixedPlacement;
    return !p || Math.abs(p.x - f.x) > .005 || Math.abs(p.y - f.y) > .005 || p.rotate !== f.rotate || p.layer !== f.layer ? [c.designator] : [];
});
const require = createRequire(import.meta.url);
const native = readFileSync(`native/pcb-board-packer/${require('../native/pcb-board-packer/platform.cjs').nativeFilename()}`);
const summary = { fixture, mode, tag, candidates: process.env.PCB_BLOCK_CANDIDATES ?? '2', blockPostRefine: process.env.PCB_BLOCK_POST_REFINE ?? '1', padCrossings: process.env.PCB_PLACEMENT_PAD_CROSSINGS ?? '1', settings, ms: elapsed, blockMs: captures.reduce((a, c) => a + c.ms, 0),
    blockSolves: captures.length, ok: result.report.ok, fixedChanges, stageMetrics,
    nativeHash: createHash('sha256').update(native).digest('hex'), inputHash: createHash('sha256').update(raw).digest('hex'),
    diagnostics: result.report.graphReport?.diagnostics, node: process.version };
writeFileSync(`${dir}/summary.json`, JSON.stringify(summary, null, 2));
writeFileSync(`${dir}/captures.json`, JSON.stringify(captures));
console.log(JSON.stringify(summary));
if (!result.report.ok || fixedChanges.length || new Set(result.placements.map(p => p.designator)).size !== input.components.length) process.exitCode = 1;
