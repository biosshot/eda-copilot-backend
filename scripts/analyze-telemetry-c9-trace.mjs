import {readFileSync,writeFileSync} from 'node:fs';
import {gzipSync} from 'node:zlib';
import {getPadWorld} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
const out='docs/experiments/telemetry-c9-trace-2026-09-27';
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const distance=ps=>[['C9','1','U2','6'],['C9','2','U2','7']].map(([a,ap,b,bp])=>{
    const point=(d,pin)=>{const p=ps.find(p=>p.designator===d);return p&&getPadWorld(input.components.find(c=>c.designator===d),p,pin);};
    const p=point(a,ap),q=point(b,bp);return p&&q?Math.hypot(p.x-q.x,p.y-q.y):null;
});
const good=r=>r.hard===0&&r.distances.every(d=>d!=null&&d<=3);
const closest=rs=>rs.filter(r=>r.hard===0&&r.distances.every(d=>d!=null)).sort((a,b)=>Math.max(...a.distances)-Math.max(...b.distances))[0];
const normalize=ps=>{const u=ps.find(p=>p.designator==='U2'),degrees=90-u.rotate,a=degrees*Math.PI/180;
    return ps.map(p=>({...p,x:(p.x-u.x)*Math.cos(a)-(p.y-u.y)*Math.sin(a),y:(p.x-u.x)*Math.sin(a)+(p.y-u.y)*Math.cos(a),rotate:(p.rotate+degrees+360)%360}));};
const signature=ps=>normalize(ps).sort((a,b)=>a.designator.localeCompare(b.designator)).map(p=>[p.designator,+p.x.toFixed(5),+p.y.toFixed(5),p.rotate,p.layer]);
const data=[];
for(const tag of ['base','combined','combined-c9-signal']){
    const log=readFileSync(`.test-output/telemetry-c9-trace/current_iso-${tag}.log`,'utf8');
    const events=log.split(/\r?\n/).filter(s=>s.startsWith('PCB_TRACE ')).map(s=>JSON.parse(s.slice(10)));
    writeFileSync(`${out}/current_iso/${tag}-trace.jsonl.gz`,gzipSync(events.map(e=>JSON.stringify(e)).join('\n')+'\n'));
    const final=JSON.parse(readFileSync(`${out}/current_iso/${tag}.json`));
    const old=JSON.parse(readFileSync(`docs/experiments/telemetry-ordering-2026-09-27/current_iso/${tag==='combined-c9-signal'?'combined':tag}.json`));
    const unchanged=JSON.stringify(signature(final.placements))===JSON.stringify(signature(old.placements));
    if(tag!=='combined-c9-signal'&&!unchanged)throw Error(`Tracing changed ${tag} geometry`);
    const stages=events.filter(e=>['beam_complete','local_complete','pair_complete','native_final'].includes(e.event)).map(e=>({stage:e.event,placements:e.data.poses,parts:e.data.parts}));
    stages.push({stage:'postrefine_final',placements:final.placements});
    for(const s of stages){s.distances=distance(s.placements);s.metrics=placementMetrics(input,s.placements);
        s.svg=`current_iso/${tag}-${s.stage}.svg`;
        writeFileSync(`${out}/${s.svg}`,renderPlacementSubsetSvg(input,normalize(s.placements),{ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:false,padding:2}).replace(/[ \t]+$/gm,''));}
    const search=[];
    for(let i=0;i<events.length;i++){
        const e=events[i];if(e.phase!=='search'||e.event!=='generated')continue;
        const short=events[i+1],returned=events[i+2];
        if(short.event!=='shortlist'||returned.event!=='ranked_returned')throw Error('Unexpected candidate trace sequence');
        search.push({placed:e.data.placed.map(p=>p.designator),placedPoses:e.data.placed,
            generated:e.data.candidates.length,legal:e.data.candidates.filter(c=>c.hard===0).length,
            goodGenerated:e.data.candidates.filter(good).length,goodShortlist:short.data.candidates.filter(good).length,goodReturned:returned.data.candidates.filter(good).length,
            closestGenerated:closest(e.data.candidates),closestReturned:closest(returned.data.candidates),
            bestGenerated:e.data.candidates[0],returned:returned.data.candidates,details:e.data.details});
    }
    const beam=events.filter(e=>e.event==='beam_kept').map(e=>({step:e.data.states[0]?.poses.length,states:e.data.states.map(s=>({...s,distances:distance(s.poses)}))}));
    const local=events.filter(e=>e.event==='local_candidates').map(e=>({placed:e.data.placed,current:e.data.current,generated:e.data.candidates.length,
        legal:e.data.candidates.filter(c=>c.hard===0).length,goodGenerated:e.data.candidates.filter(good).length,
        closestGenerated:closest(e.data.candidates),closestTop16:closest(e.data.candidates.slice(0,16)),top16:e.data.candidates.slice(0,16)}));
    const changes=events.filter(e=>['local_chosen','pair_accepted'].includes(e.event));
    const postrefine=JSON.parse(readFileSync(`${out}/current_iso/${tag}-postrefine.json`));
    data.push({tag,unchanged,stages,search,beam,local,changes,postrefine});
}
writeFileSync(`${out}/analysis.json`,JSON.stringify(data,null,2));
writeFileSync(`${out}/comparison.html`, `<!doctype html><html lang="ru"><meta charset="utf-8"><title>C9: путь кандидата</title>
<style>body{font:16px system-ui;margin:24px;background:#f1f5f8;color:#203344}p{max-width:1200px;line-height:1.5}select{font:inherit;padding:8px;margin:8px}section{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;padding:12px;background:white}img{width:100%;height:470px;object-fit:contain}table{border-collapse:collapse;background:white;width:100%;font-size:14px}td,th{padding:8px;border:1px solid #ccd6df;text-align:left}details{margin:20px 0}.scroll{overflow:auto}@media(max-width:850px){section{grid-template-columns:1fr}}</style>
<h1>C9: от кандидатов до финальной компоновки</h1>
<p>Два воспроизведения current_iso: прежняя очередность и все четыре экспериментальных изменения. В них поиск и оценка не изменены; итоговые позиции совпали с предыдущими прогонами с точностью 0,00001 мм в системе координат U2. Третий прогон — контроль: только внутренний признак powerComponent у C9 снят, остальной вход и настройки сохранены. Остальные блоки не запускались.</p>
<p>«Близкий» здесь означает: оба расстояния C9.1 → U2.6 и C9.2 → U2.7 не больше 3 мм, геометрических нарушений нет. Это диагностический порог, не новое ограничение. Красные линии — MST, расстояния в таблице измеряются непосредственно до U2.</p>
<label>Очередность <select id="run"><option value="base">Прежняя</option><option value="combined">Все четыре изменения</option><option value="combined-c9-signal">Контроль: C9 не компонент питания</option></select></label>
<section>${[0,1].map(i=>`<figure><select id="stage${i}"></select><a id="link${i}" target="_blank"><img id="img${i}"></a><p id="caption${i}"></p></figure>`).join('')}</section>
<h2>Позиция C9 по этапам</h2><table id="stages"></table>
<details open><summary>Генерация и отсечение кандидатов C9 во время beam search</summary><p>Каждая строка — отдельное состояние родителей. «До / после shortlist / после micro» — число близких кандидатов. Последний столбец ещё не означает выживание в глобальном beam.</p><div class="scroll"><table id="search"></table></div></details>
<details><summary>Выжившие состояния beam</summary><table id="beam"></table></details>
<details><summary>Одиночные локальные перемещения C9</summary><table id="local"></table></details>
<p>Подробности, состав оценок и позы сохранены в <a href="analysis.json">analysis.json</a>; исходные трассы лежат рядом с результатами. Никакое улучшение по умолчанию не включалось и не отключалось.</p>
<script>const data=${JSON.stringify(data).replace(/</g,'\\u003c')};const el=id=>document.getElementById(id);
const labels={beam_complete:'После beam search',local_complete:'После одиночных перемещений',pair_complete:'После парных перестановок',native_final:'После центрирования',postrefine_final:'После block postrefine'};
const fmt=ds=>ds?.map(d=>d==null?'—':d.toFixed(2)).join(' / ')??'—';let current;
function choose(){current=data.find(d=>d.tag===el('run').value);for(let i=0;i<2;i++){const s=el('stage'+i);s.replaceChildren();for(const r of current.stages)s.add(new Option(labels[r.stage],r.stage));s.value=i?'postrefine_final':'beam_complete';}draw();}
function table(id,heads,rows){el(id).innerHTML='<tr>'+heads.map(h=>'<th>'+h+'</th>').join('')+'</tr>'+rows.map(r=>'<tr>'+r.map(c=>'<td>'+c+'</td>').join('')+'</tr>').join('');}
function draw(){for(let i=0;i<2;i++){const s=current.stages.find(s=>s.stage===el('stage'+i).value);el('img'+i).src=s.svg;el('link'+i).href=s.svg;el('caption'+i).textContent='C9 → U2: '+fmt(s.distances)+' мм; MST '+s.metrics.wireLength+' мм; линия–линия '+s.metrics.crossings+'; линия–пад '+s.metrics.foreignPadHits;}
table('stages',['Этап','C9 → U2, мм','MST, мм','Линия–линия','Линия–пад'],current.stages.map(s=>[labels[s.stage],fmt(s.distances),s.metrics.wireLength,s.metrics.crossings,s.metrics.foreignPadHits]));
table('search',['Уже размещены','Всего / допустимых','Близких: до / shortlist / micro','Ближайший допустимый, мм','Ближайший после micro, мм'],current.search.map(s=>[s.placed.join(', '),s.generated+' / '+s.legal,[s.goodGenerated,s.goodShortlist,s.goodReturned].join(' / '),fmt(s.closestGenerated?.distances),fmt(s.closestReturned?.distances)]));
table('beam',['Шаг','Состояний','Расстояния C9 → U2 во всех состояниях, мм'],current.beam.map(s=>[s.step,s.states.length,s.states.map(r=>fmt(r.distances)).join('; ')]));
table('local',['До прохода, мм','Близких кандидатов','Ближайший допустимый, мм','Ближайший в shortlist 16, мм'],current.local.map(s=>[fmt(s.current.distances),s.goodGenerated,fmt(s.closestGenerated?.distances),fmt(s.closestTop16?.distances)]));}
el('run').onchange=choose;for(let i=0;i<2;i++)el('stage'+i).onchange=draw;el('run').value='combined';choose();</script></html>`);
console.log(JSON.stringify(data.map(d=>({tag:d.tag,unchanged:d.unchanged,stages:d.stages.map(s=>({stage:s.stage,d:s.distances,m:s.metrics})),search:d.search.map(s=>({placed:s.placed,n:s.generated,legal:s.legal,good:[s.goodGenerated,s.goodShortlist,s.goodReturned],near:s.closestGenerated?.distances,returned:s.closestReturned?.distances})),local:d.local.map(s=>({current:s.current.distances,good:s.goodGenerated,closest:s.closestGenerated?.distances,top16:s.closestTop16?.distances}))})),null,2));
