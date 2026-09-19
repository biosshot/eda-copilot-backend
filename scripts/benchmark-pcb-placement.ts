// node --import tsx scripts/benchmark-pcb-placement.ts ESpower [runs] [report.json] [variant.js]
// Set PCB_BOARD_PACKER_NATIVE_PATH to compare another release addon.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { runPcbLayout } from '../src/pcb-layout/run-pcb-layout.ts';
import type { ExplainCircuit } from '../src/types/circuit.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';

const fixture = process.argv[2] ?? 'ESpower';
if (!/^[a-zA-Z0-9_-]+$/.test(fixture)) throw new Error('Expected a fixture directory name');
const runs = Number(process.argv[3] ?? 2);
if (!Number.isInteger(runs) || runs < 1) throw new Error('Expected a positive run count');
const root = new URL(`../tests/pcb-layout/${fixture}/`, import.meta.url);
const circuit = JSON.parse(readFileSync(new URL(`${fixture}.json`, root), 'utf8')) as ExplainCircuit;
const code = readFileSync(new URL(`${fixture}.js`, root), 'utf8');
const variantCode = process.argv[5] ? readFileSync(process.argv[5], 'utf8') : code;
let native: Record<string, { calls: number; ms: number }> = {};
const addon = loadNativeBoardPacker();
for (const name of ['solveBlockPrimitives', 'solveBoardPacked', 'prepareRouteLayoutComparison', 'compareRouteLayoutCandidate'] as const) {
    const original = addon[name] as (...args: unknown[]) => unknown;
    Object.defineProperty(addon, name, { configurable: true, writable: true, value: (...args: unknown[]) => {
        const start = performance.now();
        try { return original.apply(addon, args); }
        finally { const entry = native[name] ??= { calls: 0, ms: 0 }; entry.calls++; entry.ms += performance.now() - start; }
    } });
}
const samples = [];
for (let i = 0; i < runs; i++) {
    native = {};
    const start = performance.now();
    const result = await runPcbLayout({ circuit: structuredClone(circuit), code: i === 0 ? code : variantCode });
    const ms = performance.now() - start;
    const sample = { run: i + 1, ms, ok: result.placementReport.ok,
        components: result.placements.length, native,
        checksum: createHash('sha256').update(JSON.stringify({ placements: result.placements,
            report: result.placementReport, layout: result.layout })).digest('hex') };
    samples.push(sample);
    console.log(JSON.stringify(sample));
}
if (process.argv[4]) writeFileSync(process.argv[4], JSON.stringify({ fixture, samples }, null, 2));
