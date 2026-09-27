import { readFileSync, writeFileSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { createCanvas, loadImage } from 'canvas';
import { metrics } from './experiment-block-replay.mjs';

const root = '.test-output/architecture';
const out = 'docs/experiments/placement-architecture-2026-09-27';
mkdirSync(out, { recursive: true });
const rows = JSON.parse(readFileSync(`${root}/measurements.json`));
copyFileSync(`${root}/measurements.json`, `${out}/measurements.json`);
const r = n => Number(n.toFixed(2));
let table = '| Board | Mode | Valid | Pair sum, mm | Worst pair, mm | HPWL, mm | Block time, s | Total time, s | Accepted blocks |\n|---|---|---|---:|---:|---:|---:|---:|---:|\n';
const fixtures = ['Telemetry', 'ESPower', 'esp32c3'];
const modes = ['legacy', 'full-micro-single', 'full-micro', 'full-micro-repack', 'full-off', 'full-geometric-single', 'full-geometric', 'full-geometric-repack'];
for (const row of [...rows].sort((a, b) => fixtures.indexOf(a.fixture) - fixtures.indexOf(b.fixture) || modes.indexOf(a.mode) - modes.indexOf(b.mode))) {
    const last = row.stageMetrics.at(-1);
    table += `| ${row.fixture} | ${row.mode} | ${row.ok} | ${r(last.pairSum)} | ${r(last.pairMax)} | ${r(last.hpwl)} | ${r(row.blockMs / 1000)} | ${r(row.ms / 1000)} | ${row.diagnostics.filter(d => d.message.startsWith('Block portfolio selected')).length} |\n`;
}
writeFileSync(`${out}/results.md`, `# Whole-board experiments\n\nAll modes use the ordinary tree solver. No injected native options. Subtree workers and native solve cache are disabled; postrefine uses one thread. Timings are single samples, not a statistical benchmark. Valid placement does not establish routability.\n\n${table}\n`);

const usbRows = ['legacy', 'full-micro', 'full-off', 'full-geometric'].map(mode => {
    const capture = JSON.parse(readFileSync(`${root}/Telemetry/${mode}/captures.json`))
        .find(r => r.problem.components.length && r.problem.components.every(c => c.blockName === 'usb_charge'));
    return { mode, ms: capture.ms, ...metrics(capture.problem, capture.solution) };
});
writeFileSync(`${out}/usb-local.json`, JSON.stringify(usbRows, null, 2));
writeFileSync(`${out}/usb-local.md`, '# USB Charge before board packing\n\nThese are the primary local block solutions, before portfolio selection, board packing and postrefine. Whole-board SVG crops may therefore show a different variant.\n\n| Mode | Aspect | Area, mm² | Internal pair sum, mm | Worst pair, mm | Time, ms |\n|---|---:|---:|---:|---:|---:|\n' + usbRows.map(row => `| ${row.mode} | ${r(row.aspect)} | ${r(row.area)} | ${r(row.pairSum)} | ${r(row.pairMax)} | ${r(row.ms)} |`).join('\n') + '\n');

let routeTable = '| Board | Mode | Found / jobs | Budget cutoffs | No path | Weighted penalty |\n|---|---|---:|---:|---:|---:|\n';
for (const fixture of fixtures) {
    const path = `${root}/${fixture}/route-summary.json`;
    if (existsSync(path)) {
        copyFileSync(path, `${out}/${fixture}-route-summary.json`);
        for (const row of JSON.parse(readFileSync(path)).sort((a, b) => modes.indexOf(a.variant) - modes.indexOf(b.variant))) {
            routeTable += `| ${fixture} | ${row.variant} | ${row.found} / ${row.jobs} | ${row.budgetExhaustedAfter} | ${row.noPath} | ${r(row.afterPenalty)} |\n`;
        }
    }
}
writeFileSync(`${out}/routing-probe.md`, '# Independent bounded routing probe\n\nEvery board uses a job plan frozen on its legacy placement. A cutoff is an inconclusive bounded search, not proof that a connection is impossible. These are sampled jobs, not a routed PCB. Weights include net priority and fallback estimates, so compare penalties only within one board and its common job plan.\n\n' + routeTable);
const input = JSON.parse(readFileSync('tests/fixtures/block-placement/Telemetry/input.json'));
const usb = new Set(input.components.filter(c => c.block_name === 'usb_charge').map(c => c.designator));
if (!usb.size) throw Error('USB block not found');
const cards = [];
for (const mode of ['legacy', 'full-micro', 'full-geometric', 'full-off', 'full-micro-repack', 'full-geometric-repack']) {
    if (!existsSync(`${root}/Telemetry/${mode}/placement.json`)) continue;
    const placements = JSON.parse(readFileSync(`${root}/Telemetry/${mode}/placement.json`)).placements;
    const svg = renderPlacementSubsetSvg(input, placements.filter(p => usb.has(p.designator)), { padding: 2 });
    writeFileSync(`${out}/usb-${mode}.svg`, svg);
    const rasterSvg = svg.replace(/^(<svg[^>]*width=")([\d.]+)(" height=")([\d.]+)/, (_, a, w, b, h) => `${a}${Number(w) * 3}${b}${Number(h) * 3}`);
    const image = await loadImage(Buffer.from(rasterSvg));
    const canvas = createCanvas(image.width, image.height);
    canvas.getContext('2d').drawImage(image, 0, 0, canvas.width, canvas.height);
    writeFileSync(`${out}/usb-${mode}.png`, canvas.toBuffer('image/png'));
    cards.push(`<figure><figcaption>${mode}</figcaption><img src="usb-${mode}.svg"></figure>`);
}
writeFileSync(`${out}/usb-comparison.html`, `<!doctype html><meta charset="utf-8"><title>USB Charge — board context</title><style>body{font:16px system-ui;background:#eef2f6}main{display:flex;flex-wrap:wrap;gap:12px}figure{background:white;margin:0;padding:16px;max-width:45%}img{max-width:100%}</style><h1>USB Charge — from complete Telemetry runs</h1><p>All images are final board placements; common renderer scale. Placement validity does not prove routing.</p><main>${cards.join('')}</main>`);
console.log(out);
