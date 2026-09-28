import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { resolve, relative, join } from 'node:path';
import { createHash } from 'node:crypto';
import { renderPlacementSvg, renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';

const [inputPath, beforeDir, afterDir, output] = process.argv.slice(2);
if (!output) throw Error('Usage: experiment-placement-compare-report.mjs INPUT BEFORE_DIR AFTER_DIR OUTPUT_DIR');
const out = resolve(output), raw = readFileSync(inputPath), input = JSON.parse(raw);
const sha = createHash('sha256').update(raw).digest('hex');
const esc = s => String(s ?? '').replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const fmt = n => Number(n).toFixed(2);
const link = path => relative(out, resolve(path)).replaceAll('\\', '/');
const entries = [beforeDir, afterDir].map((dir, i) => {
    const result = JSON.parse(gunzipSync(readFileSync(join(dir, 'result.json.gz'))));
    const summary = JSON.parse(readFileSync(join(dir, 'summary.json')));
    if (summary.inputSha256 !== sha) throw Error(`${dir}: input hash differs`);
    return { dir, tag: i ? 'after' : 'before', title: i ? 'После' : 'До', result, summary };
});
mkdirSync(out, { recursive: true });
const options = { ratsnest: true, ratsnestTopology: 'mst', signalPaths: true, constraintRegions: true };
const figures = (paths, captions = ['До', 'После']) => `<div class="figures">${paths.map((path, i) =>
    `<figure><figcaption>${esc(captions[i])}</figcaption><a href="${esc(path)}"><img loading="lazy" src="${esc(path)}"></a></figure>`).join('')}</div>`;
for (const e of entries) writeFileSync(join(out, `${e.tag}-board.svg`), renderPlacementSvg(input, e.result.placements, options));
const blocks = input.blocks.map((block, index) => {
    const refs = new Set(block.component_designators);
    const paths = [], local = [];
    for (const e of entries) {
        const path = `${e.tag}-block-${index}.svg`;
        writeFileSync(join(out, path), renderPlacementSubsetSvg(input, e.result.placements.filter(p => refs.has(p.designator)), { ...options, padding: 2 }));
        paths.push(path);
        const saved = e.result.localBlocks.find(b => b.label === block.name)?.placements;
        if (saved?.length) {
            const path = `${e.tag}-block-${index}-local.svg`;
            writeFileSync(join(out, path), renderPlacementSubsetSvg(input, saved, { ...options, padding: 2 }));
            local.push(path);
        }
    }
    return `<details><summary>${esc(block.name)} · ${refs.size} компонентов</summary>${figures(paths)}
        ${local.length === 2 ? `<details><summary>Сразу после сборки блока</summary>${figures(local)}</details>` : ''}</details>`;
}).join('');
const rows = [
    ['Полный проход, с', e => e.summary.ms / 1000],
    ['Длина MST, мм', e => e.summary.metrics.wireLength],
    ['Пересечения связей', e => e.summary.metrics.crossings],
    ['Пересечения чужих падов', e => e.summary.metrics.foreignPadHits],
].map(([label, value]) => `<tr><td>${label}</td>${entries.map(e => `<td>${fmt(value(e))}</td>`).join('')}</tr>`).join('');
writeFileSync(join(out, 'comparison.html'), `<!doctype html><html lang="ru"><meta charset="utf-8"><title>Размещение: до и после</title>
<style>body{font:16px system-ui;background:#f1f5f9;color:#172033;max-width:1800px;margin:24px auto;padding:0 24px}a{color:#0369a1}.figures{display:grid;grid-template-columns:1fr 1fr;gap:14px}figure{background:white;margin:10px 0;padding:12px;border:1px solid #cbd5e1;border-radius:8px}img{width:100%;max-height:950px;object-fit:contain}figcaption,summary{font-weight:600}details{padding:12px 0;border-top:1px solid #cbd5e1}summary{cursor:pointer}table{border-collapse:collapse;background:white}td,th{padding:10px 25px;border:1px solid #cbd5e1}p{line-height:1.5}</style>
<h1>Размещение: до и после</h1><p>Один и тот же сохранённый вход. Красный пунктир — связи, цветные линии — signal paths. Трассировка не выполнялась.</p>
<table><tr><th>Показатель</th><th>До</th><th>После</th></tr>${rows}</table>
<p>${entries.map(e => `${e.title}: <a href="${esc(link(join(e.dir, 'assembly.json')))}">Assembly JSON</a> · <a href="${esc(link(join(e.dir, 'summary.json')))}">Диагностика</a> (report OK: ${e.summary.reportOk}, состав сохранён: ${e.summary.inventoryOk}, изменений фиксированных: ${e.summary.fixedChanges.length})`).join('<br>')}</p>
<h2>Полная плата</h2>${figures(['before-board.svg', 'after-board.svg'])}<h2>Все блоки</h2>${blocks}</html>`);
console.log(join(out, 'comparison.html'));
