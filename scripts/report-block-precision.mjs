// Read-only report for experiment-block-precision.mjs. Run with node --import tsx.
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createCanvas, loadImage } from 'canvas';
import { metrics } from './experiment-block-replay.mjs';
import { renderPlacementSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { getBox, unionBoxes } from '../src/pcb-layout/pcb-auto-place/geometry.ts';

const root = resolve(process.argv[2] ?? 'debugging/block-precision-2026-09-30');
const read = file => JSON.parse(readFileSync(join(root, file), 'utf8'));
const problem = read('problem.json');
const runs = read('runs.json');
const round = x => Math.round(x * 1000) / 1000;
const placements = solution => solution.states.flatMap(s => s.placements);
const poses = ps => ps.map(p => ({ designator: p.designator, x: round(p.x), y: round(p.y), rotate: p.rotate, layer: p.layer }));

function differences(before, after) {
  const old = new Map(placements(before).map(p => [p.designator, p]));
  const next = placements(after);
  const missing = [...old.keys()].filter(id => !next.some(p => p.designator === id));
  const added = next.filter(p => !old.has(p.designator)).map(p => p.designator);
  const rows = next.filter(p => old.has(p.designator)).map(p => {
    const a = old.get(p.designator);
    return { designator: p.designator, before: poses([a])[0], after: poses([p])[0],
      rawDistanceMm: Math.hypot(p.x - a.x, p.y - a.y),
      distanceMm: Math.hypot(round(p.x) - round(a.x), round(p.y) - round(a.y)),
      rotationChanged: a.rotate !== p.rotate, layerChanged: a.layer !== p.layer };
  });
  return { missing, added, duplicates: next.length - new Set(next.map(p => p.designator)).size,
    changedAtPlacementResolution: rows.filter(p => p.distanceMm > 0 || p.rotationChanged || p.layerChanged),
    rotated: rows.filter(p => p.rotationChanged).length, maxRawDistanceMm: Math.max(0, ...rows.map(p => p.rawDistanceMm)),
    maxDistanceMm: Math.max(0, ...rows.map(p => p.distanceMm)) };
}

const stages = runs.f64.solution.checkpoints.map(before => {
  const after = runs.f32.solution.checkpoints.find(c => c.stage === before.stage);
  const evaluated64 = read(`f64-${before.stage}-f64-evaluation.json`);
  const evaluated32 = read(`f32-${before.stage}-f64-evaluation.json`);
  if (evaluated64.rank.score !== before.rank.score || evaluated64.rank.hardCount !== before.rank.hardCount) {
    throw Error(`F64 offline scorer must reproduce original ${before.stage} score exactly`);
  }
  const intended64 = evaluated64.diagnostics.reconstructedFromRoundedPlacements;
  const intended32 = evaluated32.diagnostics.reconstructedFromRoundedPlacements;
  if (intended64.rank.score !== before.rank.score || intended64.rank.hardCount !== before.rank.hardCount) {
    throw Error(`F64 pose reconstruction must reproduce original ${before.stage} score exactly`);
  }
  if (JSON.stringify(poses(placements(intended32))) !== JSON.stringify(poses(placements(after)))) {
    throw Error(`F64 reconstruction changed intended ${before.stage} F32 placements`);
  }
  return { stage: before.stage, f64Score: before.rank.score, f32ReportedScore: after.rank.score,
    f32RawGeometryF64Score: evaluated32.rank.score,
    rawF32GeometryOverlapPairs: evaluated32.diagnostics.rawOverlapPairs,
    f32PosesF64Score: intended32.rank.score,
    scoreChangePercent: (intended32.rank.score - before.rank.score) / before.rank.score * 100,
    hardF64: before.rank.hardCount, hardF32Reported: after.rank.hardCount, hardF32PosesF64: intended32.rank.hardCount,
    difference: differences(before, after) };
});
const layers = runs.f64.layers.map((before, i) => {
  const after = runs.f32.layers[i];
  const signature = layer => JSON.stringify(layer?.states.map(s => ({ poses: poses(s.poses), hard: s.hard })));
  return { depth: before.depth, keptF64: before.states.length, keptF32: after?.states.length,
    sameRetainedPosesAndOrder: signature(before) === signature(after) };
});
const cross = read('f64-pairs-f32-evaluation.json');
const summary = { method: read('manifest.json').method, exactF64Reference: true,
  firstDifferentBeamDepth: layers.find(l => !l.sameRetainedPosesAndOrder)?.depth ?? null,
  beamLayers: layers, stages,
  f64Metrics: metrics(problem, runs.f64.solution),
  f32Metrics: metrics(problem, read('f32-pairs-f64-evaluation.json').diagnostics.reconstructedFromRoundedPlacements),
  sameLayoutControl: { f64Score: runs.f64.solution.rank.score, f32Score: cross.rank.score,
    f32HardCount: cross.rank.hardCount, f32OverlapPairs: cross.diagnostics?.rawOverlapPairs },
  finalDifference: differences(runs.f64.solution, runs.f32.solution) };
writeFileSync(join(root, 'comparison.json'), JSON.stringify(summary, null, 2));

// Reuse the project renderer and captured physical geometry at one common scale.
const input = JSON.parse(readFileSync(resolve('debugging/pcb-layout/runs/PortableScope/2026-09-29T12-02-18-665Z/placement/input.json'), 'utf8'));
const componentMap = new Map(input.components.map(c => [c.designator, c]));
const panelRows = ['f64', 'f32'].map(precision => ({ precision, solution: runs[precision].solution,
  metric: summary[`${precision}Metrics`], rank: stages.at(-1)[precision === 'f64' ? 'f64Score' : 'f32PosesF64Score'] }));
const bounds = unionBoxes(panelRows.flatMap(row => placements(row.solution).map(p => getBox(componentMap.get(p.designator), p))));
bounds.left -= 2; bounds.right += 2; bounds.top -= 2; bounds.bottom += 2;
const canvas = createCanvas(1800, 820), ctx = canvas.getContext('2d');
ctx.fillStyle = '#eef2f6'; ctx.fillRect(0, 0, canvas.width, canvas.height);
ctx.fillStyle = '#152334'; ctx.font = 'bold 27px Arial';
ctx.fillText('PortableScope FPGA · F64 и F32 · полный поиск', 28, 42);
ctx.fillStyle = '#536477'; ctx.font = '18px Arial';
ctx.fillText('Одинаковый вход и параметры. Позы округлены до 0,001 мм и оценены в F64. Пунктир — связи, не трассы.', 28, 74);
for (let i = 0; i < panelRows.length; i++) {
  const row = panelRows[i], x = 20 + i * 900;
  ctx.fillStyle = 'white'; ctx.fillRect(x, 100, 860, 650);
  ctx.fillStyle = '#152334'; ctx.font = 'bold 24px Arial'; ctx.fillText(row.precision.toUpperCase(), x + 20, 138);
  ctx.font = '19px Arial'; ctx.fillText(`F64 score: ${row.rank.toFixed(6)}`, x + 20, 169);
  ctx.fillStyle = '#536477'; ctx.font = '17px Arial';
  ctx.fillText(`Площадь ${row.metric.area.toFixed(2)} мм² · HPWL ${row.metric.hpwl.toFixed(2)} мм`, x + 20, 198);
  const svg = renderPlacementSvg(input, placements(row.solution), { bounds, labels: true, ratsnest: true,
    ratsnestTopology: 'mst', signalPaths: false, constraintRegions: false });
  writeFileSync(join(root, `${row.precision}.svg`), svg);
  const enlarged = svg.replace(/^(<svg[^>]*width=")([\d.]+)(" height=")([\d.]+)/,
    (_, a, w, b, h) => `${a}${Number(w) * 4}${b}${Number(h) * 4}`);
  const img = await loadImage(Buffer.from(enlarged));
  const scale = Math.min(830 / img.width, 510 / img.height);
  ctx.drawImage(img, x + (860 - img.width * scale) / 2, 215 + (510 - img.height * scale) / 2, img.width * scale, img.height * scale);
}
ctx.fillStyle = '#152334'; ctx.font = '18px Arial';
ctx.fillText(`Изменились ${summary.finalDifference.changedAtPlacementResolution.length}/19 компонентов; повороты: ${summary.finalDifference.rotated}; `
  + `score: ${stages.at(-1).scoreChangePercent.toFixed(4)}%`, 28, 790);
writeFileSync(join(root, 'comparison.png'), canvas.toBuffer('image/png'));
console.log(JSON.stringify({ firstDifferentBeamDepth: summary.firstDifferentBeamDepth,
  stages: stages.map(s => ({ ...s, difference: { changed: s.difference.changedAtPlacementResolution.length,
    rotated: s.difference.rotated, maxDistanceMm: s.difference.maxDistanceMm } })),
  f64Metrics: { area: summary.f64Metrics.area, hpwl: summary.f64Metrics.hpwl },
  f32Metrics: { area: summary.f32Metrics.area, hpwl: summary.f32Metrics.hpwl },
  sameLayoutControl: summary.sameLayoutControl, image: join(root, 'comparison.png') }, null, 2));
