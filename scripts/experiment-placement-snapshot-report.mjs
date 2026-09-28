import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { renderPlacementSvg, renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { placementMetrics } from './experiment-placement-metrics.mjs';

// Usage: node --import tsx scripts/experiment-placement-snapshot-report.mjs OUT NAME INPUT [NAME INPUT ...]
const [out, ...args] = process.argv.slice(2);
if (!out || args.length < 2 || args.length % 2) {
  throw Error('Usage: experiment-placement-snapshot-report.mjs OUT NAME INPUT [NAME INPUT ...]');
}
const esc = value => String(value ?? '').replaceAll('&', '&amp;').replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const fmt = value => Number(value).toFixed(2);
const rel = (...parts) => parts.join('/');
const boards = [];

for (let i = 0; i < args.length; i += 2) {
  const [name, inputPath] = [args[i], args[i + 1]];
  if (!/^[A-Za-z0-9_-]+$/.test(name)) throw Error(`Invalid board name: ${name}`);
  const raw = readFileSync(inputPath);
  const input = JSON.parse(raw);
  const dir = join(out, name);
  const summary = JSON.parse(readFileSync(join(dir, 'summary.json')));
  const result = JSON.parse(gunzipSync(readFileSync(join(dir, 'result.json.gz'))));
  const sha = createHash('sha256').update(raw).digest('hex');
  if (summary.inputSha256 !== sha) throw Error(`${name}: result was produced from a different input`);

  writeFileSync(join(dir, 'board.svg'), renderPlacementSvg(input, result.placements,
    { ratsnest: true, ratsnestTopology: 'mst', signalPaths: true, constraintRegions: true }));
  const blocks = [];
  for (const [index, block] of input.blocks.entries()) {
    const refs = new Set(block.component_designators ?? []);
    if (!refs.size) continue;
    const boardPlacements = result.placements.filter(p => refs.has(p.designator));
    const localPlacements = result.localBlocks.find(p => p.label === block.name)?.placements;
    const id = `block-${index}`;
    const boardSvg = `${id}-board.svg`;
    writeFileSync(join(dir, boardSvg), renderPlacementSubsetSvg(input, boardPlacements,
      { padding: 2, ratsnest: true, ratsnestTopology: 'mst', signalPaths: true, constraintRegions: true }));
    let localSvg = null;
    if (localPlacements?.length) {
      localSvg = `${id}-local.svg`;
      writeFileSync(join(dir, localSvg), renderPlacementSubsetSvg(input, localPlacements,
        { padding: 2, ratsnest: true, ratsnestTopology: 'mst', signalPaths: true, constraintRegions: true }));
    }
    blocks.push({ name: block.name, refs: [...refs], boardSvg, localSvg,
      boardMetrics: placementMetrics(input, boardPlacements) });
  }
  boards.push({ name, inputPath, summary, blocks,
    signalPathCount: input.paths?.length ?? 0, regionCount: input.constraintRegions?.length ?? 0 });
}

const cards = boards.map(board => {
  const { name, summary, blocks } = board;
  const status = summary.reportOk && summary.inventoryOk && !summary.fixedChanges.length ? 'OK' : 'Есть нарушения';
  const blockCards = blocks.map(block => `<details class="block"><summary>${esc(block.name)} · ${block.refs.length} компонентов · MST ${fmt(block.boardMetrics.wireLength)} мм</summary>
    <p>${esc(block.refs.join(', '))}</p><div class="images">
    ${block.localSvg ? `<figure><figcaption>После сборки блока</figcaption><a href="${esc(rel(name, block.localSvg))}"><img loading="lazy" src="${esc(rel(name, block.localSvg))}"></a></figure>` : ''}
    <figure><figcaption>На итоговой плате</figcaption><a href="${esc(rel(name, block.boardSvg))}"><img loading="lazy" src="${esc(rel(name, block.boardSvg))}"></a></figure>
    </div></details>`).join('');
  return `<section id="${esc(name)}"><h2>${esc(name)}</h2>
    <p>${summary.componentCount} компонентов, ${blocks.length} блоков. Полный проход: ${fmt(summary.ms / 1000)} с. Проверка: <strong>${status}</strong>.</p>
    <p>Длина MST: ${fmt(summary.metrics.wireLength)} мм; пересечения связей: ${summary.metrics.crossings}; чужие пады: ${summary.metrics.foreignPadHits}.
    Signal paths во входе: ${board.signalPathCount}; constraint regions: ${board.regionCount}.</p>
    <p><a href="${esc(rel(name, 'assembly.json'))}">Assembly JSON</a> · <a href="${esc(rel(name, 'summary.json'))}">Диагностика</a> · <a href="${esc(rel(name, 'board.svg'))}">SVG платы</a></p>
    <a href="${esc(rel(name, 'board.svg'))}"><img class="board" src="${esc(rel(name, 'board.svg'))}" alt="Плата ${esc(name)}"></a>
    <h3>Блоки</h3>${blockCards}</section>`;
}).join('');

mkdirSync(out, { recursive: true });
writeFileSync(join(out, 'comparison.html'), `<!doctype html><html lang="ru"><meta charset="utf-8"><title>Полное размещение плат</title>
<style>body{font:16px system-ui;background:#f1f5f9;color:#172033;max-width:1700px;margin:24px auto;padding:0 24px}a{color:#0369a1}section{background:white;margin:24px 0;padding:20px;border:1px solid #cbd5e1;border-radius:12px}p{line-height:1.5}.board{display:block;width:100%;max-height:1100px;object-fit:contain;border:1px solid #cbd5e1}.block{border-top:1px solid #cbd5e1;padding:12px 0}.block summary{cursor:pointer;font-weight:600}.images{display:grid;grid-template-columns:repeat(auto-fit,minmax(400px,1fr));gap:12px}.images figure{margin:0;padding:10px;border:1px solid #cbd5e1;border-radius:8px}.images img{width:100%;max-height:700px;object-fit:contain}figcaption{font-weight:600;margin-bottom:8px}</style>
<h1>Полное размещение плат</h1><p>Красный пунктир показывает связи; цветные линии — заданные signal paths. Это результат размещения, а не трассировка.</p>
${cards}</html>`);
writeFileSync(join(out, 'report.json'), JSON.stringify(boards, null, 2));
console.log(JSON.stringify(boards.map(b => ({ name: b.name, blocks: b.blocks.length, ms: b.summary.ms,
  reportOk: b.summary.reportOk, inventoryOk: b.summary.inventoryOk }))));
