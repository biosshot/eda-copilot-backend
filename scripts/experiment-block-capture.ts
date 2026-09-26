// Capture resolved native problems and baseline solutions without applying to an editor.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { runPcbLayout } from '../src/pcb-layout/run-pcb-layout.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';

process.env.PCB_LAYOUT_SUBTREE_WORKERS = '0';
process.env.PCB_NATIVE_SOLVE_CACHE = '0';
const [fixture = 'ESpower', output = '.test-output/block-experiments'] = process.argv.slice(2);
const dir = resolve(output, fixture);
mkdirSync(dir, { recursive: true });
const root = fixture === 'Telemetry' ? '../telemetry-design/placement-experiments/2026-09-26' : `tests/pcb-layout/${fixture}`;
const circuit = JSON.parse(readFileSync(`${root}/${fixture === 'Telemetry' ? 'schematic-current' : fixture}.json`, 'utf8'));
const code = readFileSync(`${root}/${fixture === 'Telemetry' ? 'intent-current' : fixture}.js`, 'utf8');
// Only the board is preserved by this DSL. Normalize native y-up coordinates
// exactly like the extension's getPcbExistingPlacement; component poses come from DSL.
const pcb = fixture === 'Telemetry' ? JSON.parse(readFileSync(`${root}/pcb-current-overview.json`, 'utf8')) : undefined;
const points = pcb?.board.polygon;
const cx = points ? (Math.min(...points.map((p: any) => p.x)) + Math.max(...points.map((p: any) => p.x))) / 2 : 0;
const cy = points ? (Math.min(...points.map((p: any) => p.y)) + Math.max(...points.map((p: any) => p.y))) / 2 : 0;
const existingPlacement = points ? { board: { polygon: points.map((p: any) => ({ x: p.x - cx, y: cy - p.y })) }, components: [] } : undefined;
const addon = loadNativeBoardPacker();
const original = addon.solveBlockPrimitives;
let index = 0;
Object.defineProperty(addon, 'solveBlockPrimitives', { configurable: true, value: (problem: object) => {
    const start = performance.now();
    const solution = original(problem);
    writeFileSync(`${dir}/block-${index++}.json`, JSON.stringify({ problem, solution, ms: performance.now() - start }));
    return solution;
} });
const start = performance.now();
const result = await runPcbLayout({ circuit, code, existingPlacement, outputDir: dir });
writeFileSync(`${dir}/input.json`, JSON.stringify(result.placementInput));
writeFileSync(`${dir}/summary.json`, JSON.stringify({ fixture, ms: performance.now() - start, blocks: index,
    components: circuit.components.length, inputHash: createHash('sha256').update(JSON.stringify(result.placementInput)).digest('hex'),
    report: result.placementReport, digest: result.digest }, null, 2));
console.log(JSON.stringify({ fixture, blocks: index, ms: performance.now() - start, ok: result.placementReport.ok }));
