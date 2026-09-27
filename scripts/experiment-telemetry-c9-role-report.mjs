import {readFileSync,writeFileSync} from 'node:fs';
import {gzipSync} from 'node:zlib';
import {getPadWorld} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
import {out,variants} from './experiment-telemetry-c9-role.mjs';
const source=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const normalize=ps=>{const u=ps.find(p=>p.designator==='U2'),a=(90-u.rotate)*Math.PI/180;return ps.map(p=>({...p,x:(p.x-u.x)*Math.cos(a)-(p.y-u.y)*Math.sin(a),y:(p.x-u.x)*Math.sin(a)+(p.y-u.y)*Math.cos(a),rotate:(p.rotate+90-u.rotate+360)%360}));};
const signature=ps=>normalize(ps).sort((a,b)=>a.designator.localeCompare(b.designator)).map(p=>[p.designator,+p.x.toFixed(5),+p.y.toFixed(5),p.rotate,p.layer]);
const runs=[];
for(const v of variants){
    const r=JSON.parse(readFileSync(`${out}/current_iso/${v.id}.json`));
    const input=structuredClone(source);input.components.find(c=>c.designator==='C9').pcb.role=v.c9Role;
    if(!r.inventory||!r.orientation||!r.validation.ok||!r.hardPairsOk||!r.onlyRoleChanged)throw Error(`Invalid ${v.id}`);
    if(v.c9Role==='decoupling_cap'){
        const old=JSON.parse(readFileSync(`docs/experiments/telemetry-c9-trace-2026-09-27/current_iso/${v.family}.json`));
        r.matchesPriorRun=JSON.stringify(signature(r.placements))===JSON.stringify(signature(old.placements));
        if(!r.matchesPriorRun)throw Error(`Baseline drift: ${v.id}`);
    }
    const p=JSON.parse(readFileSync(`${out}/current_iso/${v.id}-problem.json`)),ci=p.components.findIndex(c=>c.designator==='C9'),ui=p.components.findIndex(c=>c.designator==='U2');
    r.nativeC9=p.components[ci];r.c9IcClearance=p.componentPairClearance[ci*p.components.length+ui];
    const events=readFileSync(`.test-output/telemetry-c9-role/current_iso-${v.id}.log`,'utf8').split(/\r?\n/).filter(s=>s.startsWith('PCB_TRACE ')).map(s=>JSON.parse(s.slice(10)));
    const selected=events.filter(e=>['beam_complete','local_complete','pair_complete','native_final','local_chosen','pair_accepted','beam_kept'].includes(e.event));
    writeFileSync(`${out}/current_iso/${v.id}-stages.jsonl.gz`,gzipSync(selected.map(e=>JSON.stringify(e)).join('\n')+'\n'));
    r.stages=events.filter(e=>['beam_complete','local_complete','pair_complete'].includes(e.event)).map(e=>({stage:e.event,placements:e.data.poses,parts:e.data.parts}));
    r.stages.push({stage:'postrefine_final',placements:r.placements});
    const distance=ps=>[['1','6'],['2','7']].map(([a,b])=>{
        const point=(d,pin)=>getPadWorld(input.components.find(c=>c.designator===d),ps.find(p=>p.designator===d),pin);
        const x=point('C9',a),y=point('U2',b);return Math.hypot(x.x-y.x,x.y-y.y);
    });
    for(const s of r.stages){s.distances=distance(s.placements);s.metrics=placementMetrics(input,s.placements);s.allMetrics=placementMetrics({...input,solverOptions:{...input.solverOptions,ignoredRatsnestSignals:[]}},s.placements);
        for(const all of [false,true]){const f=`current_iso/${v.id}-${s.stage}${all?'-with-ignored':''}.svg`;
            writeFileSync(`${out}/${f}`,renderPlacementSubsetSvg(input,normalize(s.placements),{ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:all,padding:2}).replace(/[ \t]+$/gm,''));}}
    r.order=r.captures.slice().sort((a,b)=>b.order.flat().length-a.order.flat().length)[0]?.order.map(a=>a.join('+')).join(' → ');
    r.postrefine=JSON.parse(readFileSync(`${out}/current_iso/${v.id}-postrefine.json`));
    runs.push(r);
}
const summary=runs.map(r=>({id:r.variant.id,label:r.variant.label,c9Role:r.c9Role,powerComponent:r.nativeC9.powerComponent,c9IcClearance:r.c9IcClearance,
    sourceInputHash:r.sourceInputHash,inputHash:r.inputHash,nativeHash:r.nativeHash,matchesPriorRun:r.matchesPriorRun??null,valid:true,ms:r.ms,metrics:r.metrics,allMetrics:r.allMetrics,
    order:r.order,stages:r.stages.map(s=>({stage:s.stage,distances:s.distances,metrics:s.metrics,allMetrics:s.allMetrics})),postMoves:r.postrefine.moves}));
writeFileSync(`${out}/summary.json`,JSON.stringify(summary,null,2));
writeFileSync(`${out}/measurements.json`,JSON.stringify(runs,null,2));
writeFileSync(`${out}/comparison.html`, `<!doctype html><html lang="ru"><meta charset="utf-8"><title>C9 — замена роли</title>
<style>body{font:16px system-ui;margin:24px;background:#f1f5f8;color:#203344}p{max-width:1200px;line-height:1.5}select{font:inherit;padding:8px;margin:8px}section{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;padding:12px;background:white}img{width:100%;height:490px;object-fit:contain}table{border-collapse:collapse;background:white;width:100%;font-size:14px}td,th{padding:8px;border:1px solid #ccd6df;text-align:left}.scroll{overflow:auto}details{margin:20px 0}@media(max-width:850px){section{grid-template-columns:1fr}}</style>
<h1>C9: decoupling_cap → passive</h1><p>Четыре локальных прогона current_iso. В каждом сравнении меняется только роль C9 во входе, до построения графа и вычисления зазоров. Кандидаты, микророутер, оценки и остальные компоненты сохранены. Два исходных варианта воспроизвели предыдущие позиции. Полная плата не запускалась.</p>
<p>Сравнение с прежним снятием только powerComponent: полная смена роли также увеличивает требуемый зазор C9 к U2. Автоматический подбор ролей пока не добавлен. Красные линии — MST, не трассировка.</p>
<p><b>Результат:</b> с прежней очередью passive улучшает расстояния C9 5,33 / 4,63 → 4,74 / 2,93 мм и MST 41,96 → 36,86 мм. С новой очередью итог ухудшается: 6,00 / 7,07 → 8,65 / 7,52 мм, MST 43,67 → 45,74 мм. Хорошая позиция 2,80 / 2,82 мм сохраняется после одиночных перемещений и теряется в парном проходе. Улучшение старого варианта относится к учитываемым цепям: при показе скрытых цепей пересечения с чужими падами возрастают 3 → 5.</p>
<label>Очередность <select id="family"><option value="base">Прежняя</option><option value="combined">Все четыре изменения</option></select></label>
<label>Этап <select id="stage"><option value="postrefine_final">Итог</option><option value="beam_complete">После beam</option><option value="local_complete">После одиночных перемещений</option><option value="pair_complete">После парных перемещений</option></select></label>
<label><input id="all" type="checkbox">Показать скрытые цепи</label>
<section>${[0,1].map(i=>`<figure><h2>${i?'passive':'decoupling_cap'}</h2><a id="link${i}" target="_blank"><img id="img${i}"></a><p id="caption${i}"></p></figure>`).join('')}</section>
<h2>C9 → U2 по этапам</h2><table id="stages"></table>
<details open><summary>Все четыре итога</summary><div class="scroll"><table id="runs"></table></div></details>
<p>Все локальные проверки состава, зазоров, углов и обязательных пар пройдены. <a href="summary.json">Численные результаты</a>. Исходный DSL и роли в рабочей схеме не редактировались.</p>
<script>const runs=${JSON.stringify(summary).replace(/</g,'\\u003c')};const el=id=>document.getElementById(id),fmt=ds=>ds.map(d=>d.toFixed(2)).join(' / ');
const names={beam_complete:'После beam',local_complete:'После одиночных перемещений',pair_complete:'После парных перемещений',postrefine_final:'Итог'};
const metric=m=>'MST '+m.wireLength+' мм · линия–линия '+m.crossings+' · линия–пад '+m.foreignPadHits+' · площадь '+m.area+' мм²';
function table(id,heads,rows){el(id).innerHTML='<tr>'+heads.map(h=>'<th>'+h+'</th>').join('')+'</tr>'+rows.map(r=>'<tr>'+r.map(c=>'<td>'+c+'</td>').join('')+'</tr>').join('');}
function draw(){const chosen=['original','passive'].map(t=>runs.find(r=>r.id===el('family').value+'-'+t));for(let i=0;i<2;i++){let r=chosen[i],s=r.stages.find(s=>s.stage===el('stage').value),f='current_iso/'+r.id+'-'+s.stage+(el('all').checked?'-with-ignored':'')+'.svg';el('img'+i).src=f;el('link'+i).href=f;el('caption'+i).textContent='C9 → U2: '+fmt(s.distances)+' мм. '+metric(el('all').checked?s.allMetrics:s.metrics)+'. Зазор C9–U2: '+r.c9IcClearance.toFixed(4)+' мм. Порядок: '+r.order;}
table('stages',['Этап','decoupling_cap, мм','passive, мм'],Object.keys(names).map(k=>[names[k],...chosen.map(r=>fmt(r.stages.find(s=>s.stage===k).distances))]));
table('runs',['Вариант','C9 → U2, мм','Метрики итога'],runs.map(r=>[r.label,fmt(r.stages.at(-1).distances),metric(el('all').checked?r.allMetrics:r.metrics)]));}
for(const id of ['family','stage','all'])el(id).onchange=draw;el('family').value='combined';draw();</script></html>`);
console.log(JSON.stringify(summary,null,2));
