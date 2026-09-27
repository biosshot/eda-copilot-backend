import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {createCanvas,loadImage} from 'canvas';
import {out,blocks,variants} from './experiment-telemetry-ordering.mjs';
import {getPadWorld} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const data=[];
for(const block of blocks){
    const runs=variants.filter(v=>existsSync(`${out}/${block}/${v.id}.json`)).map(v=>JSON.parse(readFileSync(`${out}/${block}/${v.id}.json`)));
    if(runs.length!==variants.length)throw Error(`Incomplete ${block}: ${runs.length}/${variants.length}`);
    const names=runs[0].placements.map(p=>p.designator).sort();
    const ic=input.components.filter(c=>c.pcb.role==='main_ic'&&names.includes(c.designator)).sort((a,b)=>b.pins.length-a.pins.length)[0];
    const reference=runs[0].placements.find(p=>p.designator===ic.designator);
    for(const r of runs){
        if(JSON.stringify(r.placements.map(p=>p.designator).sort())!==JSON.stringify(names))throw Error(`Inventory differs: ${block}/${r.variant.id}`);
        const anchor=r.placements.find(p=>p.designator===ic.designator),degrees=reference.rotate-anchor.rotate,a=degrees*Math.PI/180;
        const ps=r.placements.map(p=>({...p,x:(p.x-anchor.x)*Math.cos(a)-(p.y-anchor.y)*Math.sin(a),y:(p.x-anchor.x)*Math.sin(a)+(p.y-anchor.y)*Math.cos(a),rotate:(p.rotate+degrees+360)%360}));
        for(const all of [false,true])writeFileSync(`${out}/${block}/${r.variant.id}${all?'-with-ignored':''}.svg`,renderPlacementSubsetSvg(input,ps,{ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:all,padding:2}).replace(/[ \t]+$/gm,''));
        r.passiveAccess=[];
        for(const p of r.placements){const c=input.components.find(c=>c.designator===p.designator);if(c.pcb.role==='main_ic')continue;
            for(const pin of c.pins){const net=pin.signal_name;if(!net||input.solverOptions.ignoredRatsnestSignals.some(n=>n.toLowerCase()===net.toLowerCase()))continue;
                const q=getPadWorld(c,p,pin.pin_number);if(!q)continue;
                const targets=ic.pins.filter(i=>i.signal_name===net).map(i=>({pin:i.pin_number,point:getPadWorld(ic,anchor,i.pin_number)})).filter(i=>i.point);
                if(!targets.length)continue;targets.sort((a,b)=>Math.hypot(a.point.x-q.x,a.point.y-q.y)-Math.hypot(b.point.x-q.x,b.point.y-q.y));
                const t=targets[0];r.passiveAccess.push({from:`${c.designator}.${pin.pin_number}`,to:`${ic.designator}.${t.pin}`,net,mm:+Math.hypot(t.point.x-q.x,t.point.y-q.y).toFixed(2)});
            }
        }
        r.order=r.captures.slice().sort((a,b)=>b.order.flat().length-a.order.flat().length)[0]?.order.map(p=>p.join('+')).join(' → ')??'';
        r.ok=r.inventory&&r.orientation&&r.validation.ok&&r.hardPairsOk;
        r.released=r.diagnostics.filter(d=>d.message.startsWith('Experimental')).map(d=>d.message);
    }
    data.push({block,names,runs});
}
const summary=data.map(b=>({block:b.block,components:b.names.length,runs:b.runs.map(r=>({id:r.variant.id,ok:r.ok,metrics:r.metrics,allMetrics:r.allMetrics,pairs:r.pairs,order:r.order,ms:r.ms,released:r.released}))}));
writeFileSync(`${out}/measurements.json`,JSON.stringify(data,null,2));
writeFileSync(`${out}/summary.json`,JSON.stringify(summary,null,2));
writeFileSync(`${out}/runs.json`,JSON.stringify(data.flatMap(b=>b.runs.map(r=>({block:b.block,variant:r.variant.id,completed:true,ok:r.ok,inputHash:r.inputHash,nativeHash:r.nativeHash??null}))),null,2));
writeFileSync(`${out}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Telemetry — эксперименты с очередностью</title>
<style>body{font:16px system-ui;background:#f1f5f8;color:#203344;margin:22px}h1{font-size:27px;margin-bottom:8px}p{max-width:1150px;line-height:1.5}header{position:sticky;top:0;background:#f1f5f8;padding:12px 0;z-index:2;border-bottom:1px solid #ccd5df}select,button{font:inherit;padding:8px;margin:3px;max-width:100%}button{cursor:pointer}.pair{display:grid;grid-template-columns:1fr 1fr;gap:14px}figure{background:white;padding:15px;margin:0;border-radius:8px}figure img{width:100%;height:520px;object-fit:contain}figcaption{line-height:1.7;font-size:14px}table{border-collapse:collapse;width:100%;font-size:14px;background:white}td,th{border:1px solid #cfd9e1;padding:8px;text-align:left}.good{color:#18714f}.bad{color:#b13131}.scroll{overflow:auto}details{margin:18px 0}pre{white-space:pre-wrap}a{color:#245bb2}.note{border-left:4px solid #dbab38;padding:12px;background:#fffbef}@media(max-width:850px){.pair{grid-template-columns:1fr}body{margin:10px}}</style>
<h1>Telemetry: очередность размещения</h1><p>5 блоков, по 6 вариантов. Меняется только выбор следующей детали: равный бонус критических пар; усиление прямых связей с ядром; три продолжения очереди; приоритет двух выводов и дефицита близких мест; все изменения вместе. Во всех запусках одинаковы вход, разборка групп, кандидаты, микророутер, штрафы и локальные улучшения. Портфель отключён. Полные платы не запускались.</p>
<p class="note">Варианты с перебором очереди сохраняют до трёх разных наборов поставленных деталей, до четырёх компоновок каждого. Поэтому бюджет поиска больше. Дефицит мест — доля занятых кандидатов около лучшего расстояния к IC, а не доказательство отсутствия свободного места. Hard-ограничения продолжают проверяться независимо от очереди. Красные линии — MST, не трассировка.</p>
<div id="buttons"></div><header><select id="block"></select> <label><input id="all" type="checkbox">Показать скрытые цепи</label></header>
<h2 id="heading"></h2><div class="pair">${[0,1].map(i=>`<figure><select id="variant${i}"></select><a id="link${i}" target="_blank"><img id="image${i}"></a><figcaption id="caption${i}"></figcaption></figure>`).join('')}</div>
<details open><summary>Связи пассивов с главной микросхемой — ближайший пад той же цепи</summary><div class="scroll"><table id="access"></table></div></details>
<details><summary>Все эксперименты выбранного блока</summary><div class="scroll"><table id="runs"></table></div><pre id="released"></pre></details>
<p>Геометрическая проверка здесь включает внутренние зазоры, состав компонентов, разрешённые слои/углы и локальные явные hard-пары. Контур платы и внешние связи требуют последующего отдельного прогона полной платы.</p>
<script>const data=${JSON.stringify(data).replace(/</g,'\\u003c')};const el=id=>document.getElementById(id);let current;
for(const b of data){el('block').add(new Option(b.block+' · '+b.names.length+' компонентов',b.block));const bt=document.createElement('button');bt.textContent=b.block;bt.onclick=()=>{el('block').value=b.block;choose();};el('buttons').append(bt);}
const metric=m=>'MST '+m.wireLength+' мм · линия–линия '+m.crossings+' · линия–пад '+m.foreignPadHits+' · площадь '+m.area+' мм²';
function choose(){current=data.find(b=>b.block===el('block').value);for(let i=0;i<2;i++){const old=el('variant'+i).value;el('variant'+i).replaceChildren();for(const r of current.runs)el('variant'+i).add(new Option(r.variant.label,r.variant.id));el('variant'+i).value=old||(i?'combined':'base');}update();}
function update(){el('heading').textContent=current.block+' — включая дочерние блоки';const selected=[0,1].map(i=>current.runs.find(r=>r.variant.id===el('variant'+i).value));for(let i=0;i<2;i++){const r=selected[i],file=current.block+'/'+r.variant.id+(el('all').checked?'-with-ignored':'')+'.svg';el('image'+i).src=file;el('link'+i).href=file;el('caption'+i).innerHTML=metric(el('all').checked?r.allMetrics:r.metrics)+'<br><b class="'+(r.ok?'good':'bad')+'">'+(r.ok?'Локальные проверки пройдены':'Есть нарушения')+'</b> · '+(r.ms/1000).toFixed(1)+' с<br><b>Порядок:</b> '+r.order+'<br>'+r.released.join('<br>');}
const keys=[...new Set(selected.flatMap(r=>r.passiveAccess.map(p=>p.from)))];el('access').innerHTML='<tr><th>Пад пассива</th><th>Слева</th><th>Справа</th></tr>'+keys.map(key=>'<tr><td>'+key+'</td>'+selected.map(r=>{const p=r.passiveAccess.find(p=>p.from===key);return '<td>'+(p?p.mm+' мм → '+p.to+' · '+p.net:'—')+'</td>';}).join('')+'</tr>').join('');
el('runs').innerHTML='<tr><th>Вариант</th><th>Метрики</th><th>Проверки</th><th>Время</th></tr>'+current.runs.map(r=>'<tr><td>'+r.variant.label+'</td><td>'+metric(el('all').checked?r.allMetrics:r.metrics)+'</td><td>'+(r.ok?'OK':'Нарушения')+'</td><td>'+(r.ms/1000).toFixed(1)+' с</td></tr>').join('');el('released').textContent=selected.map(r=>r.variant.label+'\\n'+(r.released.join('\\n')||'Группы сохранены')).join('\\n\\n');}
el('block').onchange=choose;el('all').onchange=update;for(let i=0;i<2;i++)el('variant'+i).onchange=update;choose();const q=new URLSearchParams(location.search);if(data.some(b=>b.block===q.get('block'))){el('block').value=q.get('block');choose();}if(current.runs.some(r=>r.variant.id===q.get('right')))el('variant1').value=q.get('right');el('all').checked=q.get('all')==='1';update();</script></html>`);
const tags=['base','scarcity','combined'],titles=['До','Два вывода и дефицит мест','Все четыре изменения'];
const canvas=createCanvas(1800,2000),ctx=canvas.getContext('2d');ctx.fillStyle='#f1f5f8';ctx.fillRect(0,0,1800,2000);
for(let row=0;row<data.length;row++)for(let col=0;col<tags.length;col++){
    const b=data[row],r=b.runs.find(r=>r.variant.id===tags[col]);let svg=readFileSync(`${out}/${b.block}/${tags[col]}.svg`,'utf8');svg=svg.replace(/width="([\d.]+)" height="([\d.]+)"/,(_,w,h)=>`width="${w*3}" height="${h*3}"`);
    const img=await loadImage(Buffer.from(svg)),s=Math.min(570/img.width,290/img.height);
    ctx.fillStyle='#203344';ctx.font='bold 19px Arial';ctx.fillText(b.block+' — '+titles[col],col*600+12,row*400+27);ctx.font='16px Arial';ctx.fillText('MST '+r.metrics.wireLength+' · линии '+r.metrics.crossings+' · пады '+r.metrics.foreignPadHits,col*600+12,row*400+53);
    ctx.drawImage(img,col*600+(600-img.width*s)/2,row*400+82,img.width*s,img.height*s);
}writeFileSync(`${out}/blocks.png`,canvas.toBuffer('image/png'));
console.log(JSON.stringify({runs:data.reduce((n,b)=>n+b.runs.length,0),ok:data.every(b=>b.runs.every(r=>r.ok)),summary:summary.map(b=>({block:b.block,runs:b.runs.filter(r=>tags.includes(r.id))}))}));
