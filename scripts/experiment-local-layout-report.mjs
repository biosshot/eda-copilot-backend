import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { renderPlacementSubsetSvg, renderPlacementSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { placementMetrics } from './experiment-placement-metrics.mjs';
import { createCanvas, loadImage } from 'canvas';
const previous = 'docs/experiments/global-placement-2026-09-27';
const out = 'docs/experiments/local-layout-2026-09-27';
const read = path => JSON.parse(readFileSync(path, 'utf8'));
const boards = read(`${previous}/measurements.json`).filter(b => b.entities.length && !b.duplicateOf && (!process.argv[2] || process.argv.slice(2).includes(b.name)));
const data = [];
function normalized(ps, before, input) {
    const component = input.components.filter(c => ps.some(p => p.designator === c.designator))
        .sort((a,b)=>Number(b.pcb.role==='main_ic')-Number(a.pcb.role==='main_ic') || b.footprint.pads.length-a.footprint.pads.length || a.designator.localeCompare(b.designator))[0];
    const anchor = ps.find(p=>p.designator===component.designator), reference = before.find(p=>p.designator===component.designator);
    const degrees = reference.rotate-anchor.rotate, a=degrees*Math.PI/180;
    return ps.map(p=>({...p,x:(p.x-anchor.x)*Math.cos(a)-(p.y-anchor.y)*Math.sin(a),y:(p.x-anchor.x)*Math.sin(a)+(p.y-anchor.y)*Math.cos(a),rotate:((p.rotate+degrees)%360+360)%360}));
}
for(const board of boards) {
    const root = `.test-output/architecture/${board.name}/local-fixes`;
    if(!existsSync(`${root}/summary.json`)) throw Error(`Missing completed result: ${board.name}`);
    mkdirSync(`${out}/${board.name}`,{recursive:true});
    const input=read(`${previous}/${board.name}/input.json`), before=read(`${previous}/${board.name}/pads-placement.json`), after=read(`${root}/placement.json`), summary=read(`${root}/summary.json`);
    const entities = [...board.entities];
    for (const e of board.entities.filter(e => e.kind === 'block')) {
        const names = new Set(e.names), visited = new Set();
        const visit = name => { if(visited.has(name)) return; visited.add(name); for(const child of input.blocks.filter(b=>b.attachTo===name)){child.component_designators.forEach(n=>names.add(n));visit(child.name);} };
        visit(e.name);
        if(names.size>e.names.length) entities.push({id:`family-${e.id}`,name:e.name,label:`Блок с дочерними: ${e.name}`,kind:'family',names:[...names]});
    }
    const actual=new Set(after.placements.map(p=>p.designator));
    const b={name:board.name,entities,ignored:input.solverOptions.ignoredRatsnestSignals,
        beforeOk:before.report.ok,afterOk:after.report.ok,inventoryOk:actual.size===input.components.length&&after.placements.length===input.components.length&&input.components.every(c=>actual.has(c.designator)),
        fixedChanges:summary.fixedChanges,beforeMs:board.variants.pads.ms,ms:summary.ms,diagnostics:buildPlacementGraph(input).report.diagnostics,
        issues:Object.fromEntries(['unplaced','overlaps','outsideBoard','boardHoleViolations','constraintRegionViolations','layerViolations','hintViolations'].map(k=>[k,after.report[k]])),variants:{before:{},after:{}}};
    writeFileSync(`${out}/${board.name}/after-placement.json`,JSON.stringify(after));
    writeFileSync(`${out}/${board.name}/summary.json`,JSON.stringify(summary,null,2));
    for(const e of entities) {
        const names=new Set(e.names), old=before.placements.filter(p=>names.has(p.designator));
        for(const [tag,solution] of [['before',before],['after',after]]) {
            const ps=solution.placements.filter(p=>names.has(p.designator));
            for(const showAll of [false,true]) {
                const key=e.id+(showAll?'-all':''),file=`${board.name}/${tag}-${key}.svg`;
                const opts={ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:showAll};
                writeFileSync(`${out}/${file}`,e.kind==='board'?renderPlacementSvg(input,ps,opts):renderPlacementSubsetSvg(input,normalized(ps,old,input),{...opts,padding:2}));
                const metricInput=showAll?{...input,solverOptions:{...input.solverOptions,ignoredRatsnestSignals:[]}}:input;
                b.variants[tag][key]={file,...placementMetrics(metricInput,ps)};
            }
        }
    }
    data.push(b);
}
const deltas=data.map(b=>({name:b.name,ok:b.afterOk,beforeOk:b.beforeOk,wire:+(b.variants.after.board.wireLength-b.variants.before.board.wireLength).toFixed(2),pad:b.variants.after.board.foreignPadHits-b.variants.before.board.foreignPadHits,line:b.variants.after.board.crossings-b.variants.before.board.crossings}));
writeFileSync(`${out}/measurements.json`,JSON.stringify(data,null,2));
writeFileSync(`${out}/board-deltas.json`,JSON.stringify(deltas,null,2));
writeFileSync(`${out}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Локальная компоновка: острова и скрытые цепи</title>
<style>body{font:16px system-ui;color:#172d40;background:#f3f5f7;margin:24px}h1{font-size:27px}header{position:sticky;top:0;background:#f3f5f7;z-index:2;padding:12px 0;border-bottom:1px solid #bbb}select{font:inherit;padding:8px;max-width:100%}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;padding:14px;background:white}img{width:100%;height:610px;object-fit:contain}figcaption{padding:8px 0}pre{white-space:pre-wrap;max-height:400px;overflow:auto}table{border-collapse:collapse;width:100%;font-size:14px}td,th{border:1px solid #ccc;padding:8px;text-align:left}tr[data-board]{cursor:pointer}tr[data-board]:hover{background:#e0edff}.bad{color:#aa2030}.good{color:#147244}button{font:inherit;padding:7px;margin:4px;cursor:pointer}a{color:#245bb2}@media(max-width:850px){.pair{grid-template-columns:1fr}body{margin:10px}}</style>
<h1>Локальная компоновка: до и после исправлений</h1><p>${data.length} плат, все их блоки и модули из предыдущего теста. «До» уже включает штрафы за чужие пады и все предыдущие улучшения. Исходные цепи и правила плат одинаковы. Микророутер остаётся включён.</p>
<p><b>Что исправлено:</b> острова критических пар теперь штрафуют проход через чужие площадки и получают позиции, выровненные по падам. Автоматическая пассивная группа требует общей цепи у каждого участника — несвязанные детали больше не замораживаются вместе. Блоки с дочерними блоками получают новые кандидаты, beam и локальный postrefine. Предупреждения показывают скрытые локальные цепи, подключённые выводы без площадок и отсутствие явных внутренних правил.</p>
<p>R6.2 подключён к I_OUT; эта цепь явно находится в ignoredRatsnestSignals. Переключатель ниже показывает её вместе с остальными скрытыми цепями, включая земли, и пересчитывает отображаемые показатели. Он меняет только диагностический вид. Линии — MST связей между показанными компонентами, не трассировка.</p>
<p>Обычный вид блока показывает только его собственные компоненты. Для родительских блоков доступен также вид «Блок с дочерними»: например, у current_iso рядом размещаются ещё семь компонентов обвязки. В обычном SVG их нет, поэтому часть занятого ими места выглядит пустой.</p>
<div id="quick"></div><details><summary>Сводка полных плат — штатный список исключённых цепей</summary><div style="overflow:auto"><table id="overview"></table></div></details>
<header><select id="board"></select> <select id="entity"></select> <label><input id="all" type="checkbox">Показать скрытые цепи</label></header><h2 id="heading"></h2><p id="hidden"></p><div class="pair">${[0,1].map(i=>`<figure><b>${i?'После исправления':'До исправления'}</b><a id="link${i}" target="_blank"><img id="img${i}"></a><figcaption id="caption${i}"></figcaption></figure>`).join('')}</div>
<details><summary>Предупреждения по исходным цепям, правилам и ошибки итогового размещения</summary><pre id="diagnostics"></pre></details>
<script>const data=${JSON.stringify(data).replace(/</g,'\\u003c')};const el=id=>document.getElementById(id);for(const b of data)el('board').add(new Option(b.name,b.name));
const metric=m=>'MST '+m.wireLength+' мм; линия–линия '+m.crossings+'; линия–пад '+m.foreignPadHits+'; площадь '+m.area+' мм²';let current;
el('overview').innerHTML='<tr><th>Плата</th><th>До</th><th>После</th></tr>'+data.map(b=>'<tr data-board="'+b.name+'"><td>'+b.name+'</td>'+['before','after'].map(t=>'<td>'+metric(b.variants[t].board)+'<br><b class="'+(b[t+'Ok']?'good':'bad')+'">'+(b[t+'Ok']?'Чистое размещение':'Есть ошибки размещения')+'</b><br>Время прогона: '+Math.round((t==='before'?b.beforeMs:b.ms)/1000)+' с</td>').join('')+'</tr>').join('');
function board(){current=data.find(b=>b.name===el('board').value);el('entity').replaceChildren();for(const e of current.entities)el('entity').add(new Option(e.label+' ('+e.names.length+')',e.id));update();}
function update(){const entity=current.entities.find(e=>e.id===el('entity').value),key=entity.id+(el('all').checked?'-all':'');el('heading').textContent=current.name+' / '+entity.label;el('hidden').textContent=(el('all').checked?'Показаны также исключённые цепи: ':'Исключены согласно входным данным: ')+current.ignored.join(', ');for(let i=0;i<2;i++){const m=current.variants[i?'after':'before'][key];el('img'+i).src=m.file;el('link'+i).href=m.file;el('caption'+i).textContent=metric(m);}el('diagnostics').textContent=current.diagnostics.map(d=>d.code+': '+d.message).join('\\n\\n')+'\\n\\nПроверки итогового размещения:\\n'+JSON.stringify(current.issues,null,2);}
el('board').onchange=board;el('entity').onchange=el('all').onchange=update;el('overview').onclick=e=>{const r=e.target.closest('[data-board]');if(r){el('board').value=r.dataset.board;board();el('heading').scrollIntoView({block:'center'});}};
for(const [id,label] of [['block-8','L1 / LTE power'],['block-11','L2 / Logic power'],['block-16','R6 / Current isolation'],['family-block-16','R6 + дочерняя обвязка'],['block-27','D2 / HV negative']]){const b=document.createElement('button');b.textContent=label;b.onclick=()=>{el('board').value='Telemetry';board();el('entity').value=id;update();};el('quick').append(b);}board();const query=new URLSearchParams(location.search);if(data.some(b=>b.name===query.get('board'))){el('board').value=query.get('board');board();}if(current.entities.some(e=>e.id===query.get('entity')))el('entity').value=query.get('entity');el('all').checked=query.get('all')==='1';update();</script></html>`);
const telemetry=data.find(b=>b.name==='Telemetry');
const canvas=createCanvas(1500,1760),ctx=canvas.getContext('2d');ctx.fillStyle='#f3f5f7';ctx.fillRect(0,0,1500,1760);
for(const [row,id] of [8,11,16,27].entries())for(let side=0;side<2;side++){
    const e=telemetry.entities.find(e=>e.id===`block-${id}`),tag=side?'after':'before',m=telemetry.variants[tag][e.id];
    let svg=readFileSync(`${out}/${m.file}`,'utf8');svg=svg.replace(/width="([\d.]+)" height="([\d.]+)"/,(_,w,h)=>`width="${w*3}" height="${h*3}"`);
    const img=await loadImage(Buffer.from(svg)),s=Math.min(720/img.width,340/img.height);
    ctx.fillStyle='#172d40';ctx.font='bold 21px Arial';ctx.fillText(`${e.name} — ${side?'после':'до'}`,side*750+15,row*440+27);ctx.font='17px Arial';ctx.fillText(`MST ${m.wireLength} мм · линия–линия ${m.crossings} · линия–пад ${m.foreignPadHits}`,side*750+15,row*440+54);
    ctx.drawImage(img,side*750+(750-img.width*s)/2,row*440+80,img.width*s,img.height*s);
}
writeFileSync(`${out}/telemetry-blocks.png`,canvas.toBuffer('image/png'));
console.log(JSON.stringify({boards:data.length,inventory:data.every(b=>b.inventoryOk&&!b.fixedChanges.length),deltas},null,2));
