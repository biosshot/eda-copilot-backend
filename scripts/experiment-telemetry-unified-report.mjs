import vm from 'node:vm';
import {readFileSync,writeFileSync} from 'node:fs';
import {renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
import {getPadWorld} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {out,blocks} from './experiment-telemetry-unified.mjs';

const input=JSON.parse(readFileSync('tests/fixtures/block-placement/Telemetry/input.json'));
const runs=[];
for(const block of blocks){
    const r=JSON.parse(readFileSync(`${out}/${block}/unified.json`));
    if(!r.inventory||!r.orientation||!r.validation.ok||!r.hardPairsOk||!r.sourceUnchanged
        ||r.diagnostics.some(d=>d.severity==='error'))throw Error(`Invalid run: ${block}`);
    const pool=r.pools.find(p=>p.label===block)?.candidates;
    if(!pool?.length)throw Error(`Missing pool: ${block}`);
    const selected=pool.filter(c=>c.selected).sort((a,b)=>a.quality.score-b.quality.score);
    if(!selected.length||selected.length>3)throw Error(`Invalid portfolio: ${block}`);
    const baseline=pool.find(c=>c.stage==='beam'&&['grouped','original'].includes(c.hypothesis))??pool[0];
    const chosen=[baseline,...selected];
    const panels=chosen.map((c,index)=>{
        for(const all of [false,true])writeFileSync(`${out}/${block}/view-${index}${all?'-with-ignored':''}.svg`,
            renderPlacementSubsetSvg(input,c.placements,{ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:all,padding:2}).replace(/[ \t]+$/gm,''));
        const metrics=placementMetrics(input,c.placements);
        const allMetrics=placementMetrics({...input,solverOptions:{...input.solverOptions,ignoredRatsnestSignals:[]}},c.placements);
        const c9=[];
        if(c.placements.some(p=>p.designator==='C9'))for(const[a,b]of [['1','6'],['2','7']]){
            const point=(d,pin)=>getPadWorld(input.components.find(c=>c.designator===d),c.placements.find(p=>p.designator===d),pin);
            const x=point('C9',a),y=point('U2',b);c9.push(Math.hypot(x.x-y.x,x.y-y.y));
        }
        return {index,stage:c.stage,hypothesis:c.hypothesis,quality:c.quality,metrics,allMetrics,c9};
    });
    runs.push({block,ms:r.ms,panels,roles:r.diagnostics.filter(d=>d.message.startsWith('Role hypothesis')).map(d=>d.message),
        pool:pool.map(c=>({stage:c.stage,hypothesis:c.hypothesis,quality:c.quality,selected:c.selected})),
        valid:true,pairs:r.pairs,sourceInputHash:r.sourceInputHash,nativeHash:r.nativeHash});
}
writeFileSync(`${out}/summary.json`,JSON.stringify(runs,null,2));
writeFileSync(`${out}/runs.json`,JSON.stringify(runs.map(r=>({block:r.block,code:0,ms:r.ms})),null,2));
const data=JSON.stringify(runs).replace(/</g,'\\u003c');
writeFileSync(`${out}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Telemetry — единый отбор компоновок</title><style>
*{box-sizing:border-box}body{margin:0;background:#eef2f6;color:#183045;font:16px/1.5 system-ui}main{max-width:1500px;margin:auto;padding:26px}h1{font-size:30px;margin:0 0 10px}h2{font-size:21px}p{max-width:1120px}header{margin-bottom:24px}.controls{position:sticky;top:0;background:#eef2f6f5;padding:12px 0;z-index:1;display:flex;gap:22px;align-items:center;flex-wrap:wrap}select{padding:7px;font:inherit;border:1px solid #a9bcca;border-radius:6px}.views{display:grid;grid-template-columns:1fr 1fr;gap:18px}figure{background:white;border:1px solid #d5dee6;border-radius:10px;padding:18px;margin:0}figure h2{margin:0}img{width:100%;height:510px;object-fit:contain}figcaption{font-size:14px}table{border-collapse:collapse;width:100%;background:white;font-size:14px}th,td{border-bottom:1px solid #d8e2e9;padding:9px;text-align:left}th{background:#e1e9f1}.scroll{overflow:auto}.selected{background:#e6f5eb}details{margin-top:24px}summary{cursor:pointer;font-weight:600}.note{padding:12px 16px;border-left:4px solid #49806b;background:#e6f5eb}.muted{color:#576e81}pre{white-space:pre-wrap;font:13px/1.5 system-ui}@media(max-width:850px){.views{grid-template-columns:1fr}main{padding:16px}img{height:380px}}
</style><main><header><h1>Telemetry: единый отбор компоновок блока</h1>
<p>Пять блоков, без полной платы. Слева — начальная сборка beam с исходными ролями и сохранёнными группами; справа — результат общего отбора. Это этапы одного действующего механизма, а не переключаемые режимы плейсера.</p>
<p class="note">Сохраняем beam, одиночные, парные и результаты post-refine. Пробуем разборку групп и подозрительные роли, сравниваем одной оценкой и оставляем 1–3 сопоставимых варианта. Исходные роли и схема не изменены.</p></header>
<div class="controls"><label>Блок <select id="block"></select></label><label>Вариант справа <select id="variant"></select></label><label><input id="all" type="checkbox"> Показать скрытые цепи</label></div>
<div class="views">${[0,1].map(i=>`<figure><h2 id="title${i}"></h2><p class="muted" id="stage${i}"></p><a id="link${i}" target="_blank"><img id="image${i}"></a><figcaption id="caption${i}"></figcaption></figure>`).join('')}</div>
<h2>Пять выбранных блоков</h2><div class="scroll"><table id="summary"></table></div>
<p class="muted">Длины — прямые линии MST, не трассировка. Показ скрытых цепей меняет только визуализацию и диагностические числа, а не отбор. Внутренняя линия между падами одной микросхемы не штрафуется за пады этой же микросхемы; такие попадания указаны отдельно.</p>
<details><summary>Оценки всех допустимых этапов выбранного блока</summary><div class="scroll"><table id="pool"></table></div></details>
<details><summary>Попытки изменения роли</summary><pre id="roles"></pre></details>
<details><summary>Правила отбора и границы проверки</summary><p>Общая оценка включает ограничения, длины, пересечения линий и чужих падов, дополнительный штраф за растяжение отдельных связей, площадь и доступ к внешним цепям. Для связи пассива с микросхемой применяется 6·d + 32·max(0,d−3)²; питание имеет вес 0,25. Площадь стоит 0,35 на мм². Это начальный баланс, проверенный на Telemetry.</p><p>Дополнительный вариант проходит ограничения: электрическая оценка не хуже лучшей более чем на 12% + 20, взвешенная MST не хуже на 15% + 1 мм, отдельная связь с IC не длиннее лучшей более чем на max(1,5 мм, 35%). Дубликаты и варианты без полезного отличия формы или выходов отбрасываются. Смена роли не уменьшает итоговую оценку неподвижной компоновки.</p><p>Микророутер остаётся эвристикой поиска. Окончательный отбор готовых блоков использует общую геометрическую оценку. При размещении платы дополнительно учитывается внутреннее качество выбранных блоков; интеграция проверена тестами, полная Telemetry в этом запуске не собиралась.</p></details>
<script>const runs=${data},el=id=>document.getElementById(id),fmt=n=>n.toFixed(2),stage=s=>({beam:'Beam',singles:'Одиночные',pairs:'Парные'}[s]??s.replace('+postrefine',' + post-refine'));
function table(id,heads,rows){el(id).innerHTML='<tr>'+heads.map(h=>'<th>'+h+'</th>').join('')+'</tr>'+rows.map(r=>'<tr'+(r.kept?' class="selected"':'')+'>'+r.values.map(c=>'<td>'+c+'</td>').join('')+'</tr>').join('');}
el('block').innerHTML=runs.map(r=>'<option>'+r.block+'</option>').join('');
function options(){const r=runs.find(r=>r.block===el('block').value);el('variant').innerHTML=r.panels.slice(1).map((p,i)=>'<option value="'+p.index+'">'+(i?'Альтернатива '+i:'Основной')+' · '+stage(p.stage)+'</option>').join('');draw();}
function draw(){const r=runs.find(r=>r.block===el('block').value),all=el('all').checked;[r.panels[0],r.panels[+el('variant').value]].forEach((p,i)=>{const m=all?p.allMetrics:p.metrics,f=r.block+'/view-'+p.index+(all?'-with-ignored':'')+'.svg';el('title'+i).textContent=i?'После общего отбора':'Начальная сборка';el('stage'+i).textContent=p.hypothesis+' / '+stage(p.stage)+' · score '+fmt(p.quality.score);el('image'+i).src=f;el('link'+i).href=f;el('caption'+i).textContent='MST '+m.wireLength+' мм · линия–линия '+m.crossings+' · чужие пады '+m.foreignPadHits+' · неизменяемые внутри IC '+m.internalIcPadHits+' · площадь '+m.area+' мм²'+(p.c9.length?' · C9 → U2: '+p.c9.map(fmt).join(' / ')+' мм':'');});
table('summary',['Блок','MST до → выбрано, мм','Линии','Чужие пады','Площадь, мм²','Вариантов'],runs.map(r=>{const ms=r.panels.slice(0,2).map(p=>all?p.allMetrics:p.metrics),v=k=>ms.map(m=>m[k]).join(' → ');return {values:[r.block,v('wireLength'),v('crossings'),v('foreignPadHits'),v('area'),r.panels.length-1]};}));
table('pool',['Гипотеза','Этап','Score','Электрическая часть','Растяжение','Площадь','Сохранён'],r.pool.map(c=>({kept:c.selected,values:[c.hypothesis,stage(c.stage),fmt(c.quality.score),fmt(c.quality.electrical),fmt(c.quality.localStretch),fmt(c.quality.area),c.selected?'да':'—']})));
el('roles').textContent=r.roles.length?r.roles.join('\\n\\n'):'Подозрительные роли не найдены.';}
el('block').onchange=options;el('variant').onchange=draw;el('all').onchange=draw;options();</script></main></html>`);
console.log(JSON.stringify(runs.map(r=>({block:r.block,ms:r.ms,selected:r.panels[1],retained:r.panels.length-1})),null,2));

new vm.Script(readFileSync(`${out}/comparison.html`,'utf8').match(/<script>([\s\S]*)<\/script>/)[1]);
