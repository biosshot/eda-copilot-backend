import {readFileSync,writeFileSync,readdirSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {renderPlacementSvg,renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';

const root='docs/experiments/placement-performance-2026-09-28';
const board='esp32c3',dir=`${root}/${board}`;
const read=p=>JSON.parse(readFileSync(p));
const unzip=p=>JSON.parse(gunzipSync(readFileSync(p)));
const input=read(`docs/experiments/global-placement-2026-09-27/${board}/input.json`);
const tags=['before','serial-control','parallel-1','parallel-2'];
const runs=tags.map(tag=>({tag,summary:read(`${dir}/${tag}/summary.json`),result:unzip(`${dir}/${tag}/result.json.gz`)}));
const baseline=runs[0];
const blocks=tag=>readdirSync(`${dir}/${tag}`).filter(f=>/^block-.*\.json.gz$/.test(f)).map(f=>unzip(`${dir}/${tag}/${f}`));
const nativeBaseline=new Map(blocks('before').map(b=>[JSON.stringify(b.problem),b.solution]));
for(const run of runs){
    assert.deepEqual(run.result.placements,baseline.result.placements,`${run.tag}: changed placement`);
    assert.deepEqual(run.summary.metrics,baseline.summary.metrics,`${run.tag}: changed metrics`);
    assert.equal(run.result.report.ok,true);
    for(const block of blocks(run.tag))assert.deepEqual(block.solution,nativeBaseline.get(JSON.stringify(block.problem)),`${run.tag}: changed native checkpoint`);
}
const selected=runs.at(-1);
const fixed=new Set(input.components.filter(c=>c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace).map(c=>c.designator));
const asm=createBoardAssemble(selected.result.layout,{preserveBoard:true,preservedComponents:fixed});
const assembly=BoardAssembleSchema().parse({components:asm.components});
assert.equal(assembly.components.length,input.components.length-fixed.size);
writeFileSync(`${dir}/assembly.json`,JSON.stringify(assembly,null,2));
for(const [tag,result]of [['before',baseline.result],['parallel',selected.result]]){
    saveText(`${dir}/${tag}.svg`,renderPlacementSvg(input,result.placements,{ratsnest:true,ratsnestTopology:'mst',signalPaths:true}));
    const stage=result.stages.find(s=>s.name==='01-v2-tree');
    for(const p of stage.data.primitives.filter(p=>p.kind==='block'))
        saveText(`${dir}/${tag}-${p.label.replaceAll(/[^a-zA-Z0-9_-]/g,'_')}.svg`,renderPlacementSubsetSvg(input,p.placements,{ratsnest:true,ratsnestTopology:'mst',signalPaths:true,padding:2}));
}
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const seconds=ms=>(ms/1000).toFixed(2);
const pathChecks=[];
for(const name of ['ESPower','esp32c3']){
    const i=read(`docs/experiments/global-placement-2026-09-27/${name}/input.json`);
    const result=name===board?selected.result:unzip(`docs/experiments/placement-regression-2026-09-28/${name}/after/result.json.gz`);
    const report=createPlacementReport(i,result.placements);
    const svg=renderPlacementSvg(i,result.placements,{ratsnest:true,ratsnestTopology:'mst',signalPaths:true});
    assert.match(svg,/data-path-segment/);
    saveText(`${root}/${name}-paths.svg`,svg);
    pathChecks.push({name,declaredPaths:i.paths.length,declaredRegions:i.constraintRegions.length,paths:report.signalPaths});
}
const summary={board,runs:runs.map(r=>({tag:r.tag,ms:r.summary.ms,byMethod:r.summary.byMethod})),
    exactPlacements:true,exactNativeCheckpoints:true,metrics:selected.summary.metrics,pathChecks};
writeFileSync(`${root}/summary.json`,JSON.stringify(summary,null,2));
const image=(src,title)=>`<figure><figcaption>${esc(title)}</figcaption><a href="${src}"><img src="${src}"></a></figure>`;
const local=selected.result.stages.find(s=>s.name==='01-v2-tree').data.primitives.filter(p=>p.kind==='block');
saveText(`${root}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Скорость и ограничения размещения</title>
<style>body{font:16px system-ui;background:#f1f5f9;color:#172033;max-width:1500px;margin:24px auto;padding:0 24px}p{line-height:1.55}section{background:white;padding:20px;margin:20px 0;border-radius:12px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:20px}figure{margin:0;min-width:0}img{width:100%;max-height:850px;object-fit:contain}figcaption{font-weight:600}table{border-collapse:collapse;width:100%}td,th{text-align:left;padding:10px;border-bottom:1px solid #ddd}a{color:#0369a1}summary{cursor:pointer;padding:12px 0}</style>
<h1>Ускорение без изменения компоновки</h1><p>ESP32-C3: независимые гипотезы одного блока теперь рассчитываются параллельно. Набор кандидатов, все промежуточные результаты и итоговые положения совпадают точно. Микророутер и оценка качества сохранены.</p>
<section><h2>Полный расчёт платы</h2><table><tr><th>Запуск</th><th>Время, с</th><th>Результат</th></tr>${runs.map(r=>`<tr><td>${esc(r.tag)}</td><td>${seconds(r.summary.ms)}</td><td>Точное совпадение позиций и native checkpoints</td></tr>`).join('')}</table>
<p>before — исходный код до оптимизаций. serial-control — контроль с последовательным native API. parallel-1/2 — новый пакетный API. Запуски отдельными процессами, последовательно; сборка и тесты не выполнялись во время этих замеров. Время включает полный плейсер и диагностические захваты; SVG/HTML исключены. Фоновая нагрузка машины не контролировалась.</p>
<p>Две проверки: ${seconds(baseline.summary.ms)} → ${seconds(runs[2].summary.ms)} с и ${seconds(runs[1].summary.ms)} → ${seconds(runs[3].summary.ms)} с. Это измерение ESP32-C3; такой же коэффициент для Telemetry пока не подтверждён.</p>
<p>Кеширование route score и переиспользование памяти поиска не показали убедительного ускорения и исключены из кода.</p></section>
<section><h2>ESP32-C3: полная плата</h2><div class="pair">${image(`${board}/before.svg`,'До')}${image(`${board}/parallel.svg`,'После: параллельные гипотезы')}</div><p><a href="${board}/assembly.json">Assembly JSON для EasyEDA</a></p>
${local.map(p=>{const id=p.label.replaceAll(/[^a-zA-Z0-9_-]/g,'_');return `<details><summary>Блок ${esc(p.label)}</summary><div class="pair">${image(`${board}/before-${id}.svg`,'До')}${image(`${board}/parallel-${id}.svg`,'После')}</div></details>`;}).join('')}</section>
<section><h2>Signal paths и constraint regions</h2><p>В предыдущем HTML пути скрывались параметром signalPaths:false. Отображение исправлено, заданные пути не потеряны. Цветные линии — пути, красный пунктир — ratsnest; это не трассировка. Constraint regions теперь также отрисовываются, когда присутствуют во входе.</p>
${pathChecks.map(b=>`<h3>${b.name}</h3><p>Задано путей: ${b.declaredPaths}; constraint regions: ${b.declaredRegions}. ${b.name==='ESPower'?'Показан сохранённый результат предыдущего прогона, новый длительный расчёт ESPower не выполнялся.':''}</p><table><tr><th>Путь</th><th>Ограничения</th><th>Длина / лимит, мм</th></tr>${b.paths.map(p=>`<tr><td>${esc(p.id)}</td><td>${p.withinConstraints?'В пределах':'Превышение мягкого ограничения'}</td><td>${p.segments.map(s=>`${esc(s.source)} → ${esc(s.target)}: ${Number(s.distance).toFixed(2)} / ${s.maxDistance??'—'}`).join('<br>')}</td></tr>`).join('')}</table>${image(`${b.name}-paths.svg`,b.name)}`).join('')}
<p>В сохранённом входе и текущем DSL ESPower constraint regions отсутствуют. Поэтому нарушение конкретной области пока не воспроизведено: нужен вход с этой областью. Отдельная сквозная проверка заданной запрещённой области с разрешённым блоком проходит весь плейсер.</p></section></html>`);
console.log(JSON.stringify({exactPlacements:true,exactNativeCheckpoints:true,runs:summary.runs.map(({tag,ms})=>({tag,ms})),pathChecks:pathChecks.map(b=>({name:b.name,paths:b.declaredPaths,regions:b.declaredRegions}))}));

function saveText(path,svg){writeFileSync(path,svg.replace(/[ \t]+$/gm,''));}
