import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {renderPlacementSvg,renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
import {blockQuality} from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

const root='docs/experiments/placement-performance-2026-09-28/Telemetry';
const tag=process.argv[2]??'staged-final';
const out=`${root}/staged-comparison`;mkdirSync(out,{recursive:true});
const read=p=>JSON.parse(readFileSync(p));
const unpack=p=>JSON.parse(gunzipSync(readFileSync(p)));
const input=read('docs/experiments/global-placement-2026-09-27/Telemetry/input.json');
const before=unpack(`${root}/current-profile/result.json.gz`),after=unpack(`${root}/${tag}/result.json.gz`);
const bs=read(`${root}/current-profile/summary.json`),as=read(`${root}/${tag}/summary.json`);
const stages=r=>r.stages.find(s=>s.name==='01-v2-tree').data.primitives;
const save=(file,s)=>writeFileSync(`${out}/${file}`,s.replace(/[ \t]+$/gm,''));
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const rows=[];
assert.equal(after.report.ok,true);
assert.deepEqual(after.placements.map(p=>p.designator).sort(),input.components.map(c=>c.designator).sort());
for(const c of input.components.filter(c=>c.pcb.fixedPlacement)){
    const a=after.placements.find(p=>p.designator===c.designator),b=before.placements.find(p=>p.designator===c.designator);
    for(const key of ['x','y','rotate','layer'])assert.equal(a[key],b[key],`fixed ${c.designator}.${key}`);
}
for(const name of ['voltage_iso','current_iso','adc','usb_charge','lte_power','low_charge_pos','low_charge_neg']){
    const pair=[before,after].map(r=>stages(r).find(p=>p.label===name));
    const quality=pair.map(p=>blockQuality(input,p.children));
    const metrics=pair.map(p=>placementMetrics(input,p.placements));
    pair.forEach((p,i)=>save(`${name}-${i}.svg`,renderPlacementSubsetSvg(input,p.placements,{ratsnestTopology:'mst',signalPaths:true,padding:2})));
    rows.push({name,quality,metrics});
}
for(const [name,r]of [['before',before],['after',after]])save(`${name}.svg`,renderPlacementSvg(input,r.placements,{ratsnestTopology:'mst',signalPaths:true}));
const fixed=new Set(input.components.filter(c=>c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace).map(c=>c.designator));
const asm=createBoardAssemble(after.layout,{preserveBoard:true,preservedComponents:fixed});
save('assembly.json',JSON.stringify(BoardAssembleSchema().parse({components:asm.components}),null,2));
const changed=after.placements.filter(a=>{const b=before.placements.find(b=>b.designator===a.designator);return ['x','y','rotate','layer'].some(k=>a[k]!==b[k]);}).length;
const diagnostics=after.stages.find(s=>s.name==='01-v2-tree').data.diagnostics.filter(d=>d.message.startsWith('Staged block search:'));
const summary={beforeMs:bs.ms,afterMs:as.ms,changed,qualityBefore:bs.metrics,qualityAfter:as.metrics,
    methodsBefore:bs.byMethod,methodsAfter:as.byMethod,blocks:rows,diagnostics,reportOk:after.report.ok};
save('summary.json',JSON.stringify(summary,null,2));
const images=(paths)=>`<div class="pair">${paths.map((p,i)=>`<figure><figcaption>${i?'После':'До'}</figcaption><a href="${p}"><img src="${p}"></a></figure>`).join('')}</div>`;
const qualityTable=(a,b)=>`<table><tr><th>Показатель</th><th>До</th><th>После</th></tr>${[['MST, мм','wireLength'],['Пересечения линий','crossings'],['Чужие пады','foreignPadHits'],['Площадь, мм²','area']].map(([label,key])=>`<tr><td>${label}</td><td>${a[key]}</td><td>${b[key]}</td></tr>`).join('')}</table>`;
save('comparison.html',`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Telemetry: выборочный парный поиск</title><style>body{font:16px system-ui;max-width:1550px;margin:24px auto;padding:0 24px;background:#f1f5f9;color:#172033}p{line-height:1.5}section{background:white;padding:20px;margin:20px 0;border-radius:10px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;min-width:0}img{width:100%;max-height:900px;object-fit:contain}td,th{padding:8px;border-bottom:1px solid #ddd;text-align:left}table{width:100%;border-collapse:collapse}a{color:#0369a1}</style>
<h1>Telemetry: дорогой поиск после общего отбора</h1><p>До: ${(bs.ms/1000).toFixed(1)} с. После: ${(as.ms/1000).toFixed(1)} с. Изменено позиций: ${changed}. Проверка размещения: ${after.report.ok}.</p>
<p>Все гипотезы проходят beam и одиночные перемещения. Парный поиск продолжают не более двух электрически сопоставимых вариантов; исходные checkpoints сохраняются. Близкие состояния пропускают повторный post-refine при совпадающих поворотах, группах, смещении до 0,15 мм и малой разнице электрических показателей. Их точные исходные оценки не заменяются приблизительным кешем.</p>
<p>Оба замера — полные расчёты на одной машине с шестью native-потоками. Фоновая нагрузка не контролировалась; прирост основан на одном запуске каждого варианта. Время включает захват диагностики, но исключает SVG/HTML.</p>
<section><h2>Полная плата</h2>${qualityTable(bs.metrics,as.metrics)}${images(['before.svg','after.svg'])}<p><a href="assembly.json">Assembly JSON после</a> · <a href="summary.json">Метрики и диагностика</a></p></section>
${rows.map(r=>`<section><h2>${esc(r.name)}</h2><p>Общая оценка блока: ${r.quality[0].score.toFixed(2)} → ${r.quality[1].score.toFixed(2)}. Электрическая: ${r.quality[0].electrical.toFixed(2)} → ${r.quality[1].electrical.toFixed(2)}.</p>${qualityTable(...r.metrics)}${images([`${r.name}-0.svg`,`${r.name}-1.svg`])}</section>`).join('')}
<section><h2>Отбор дорогих операций</h2>${diagnostics.map(d=>`<p>${esc(d.nodeId)}: ${esc(d.message)}</p>`).join('')}</section></html>`);
console.log(JSON.stringify({beforeMs:bs.ms,afterMs:as.ms,changed,qualityBefore:bs.metrics,qualityAfter:as.metrics,blocks:rows.map(r=>({name:r.name,score:r.quality.map(q=>q.score)}))}));
