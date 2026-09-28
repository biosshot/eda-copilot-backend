import {readFileSync,writeFileSync} from 'node:fs';
import {createCanvas,loadImage} from 'canvas';
import assert from 'node:assert/strict';
const out='docs/experiments/placement-performance-2026-09-28/Telemetry/isolated-block-metric';
const {rows,manifest}=JSON.parse(readFileSync(`${out}/summary.json`));
const runs=JSON.parse(readFileSync(`${out}/runs.json`));
const median=a=>{a=[...a].sort((a,b)=>a-b);return(a[Math.floor((a.length-1)/2)]+a[Math.floor(a.length/2)])/2;};
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
for(const r of rows)for(const metric of ['micro','geometric']){
    const result=r[metric];
    for(const p of result.placements){
        const c=input.components.find(c=>c.designator===p.designator);
        assert.ok(c.pcb.allowedRotations.includes(p.rotate),`${r.name}/${metric}/${p.designator}: rotation`);
        assert.ok(c.pcb.allowedLayers.includes(p.layer),`${r.name}/${metric}/${p.designator}: layer`);
    }
}
const sum=(metric,fn)=>rows.reduce((s,r)=>s+fn(r[metric]),0);
const aggregate=Object.fromEntries(['micro','geometric'].map(m=>[m,{
    nativeMs:sum(m,r=>r.medianMs),wire:sum(m,r=>r.metrics.wireLength),crossings:sum(m,r=>r.metrics.crossings),
    foreignPadHits:sum(m,r=>r.metrics.foreignPadHits),score:sum(m,r=>r.selected.quality.score),
    legal:rows.filter(r=>r[m].selected.legal&&r[m].selected.nativeHard===0).length,
}]));
const differences=rows.map(r=>({name:r.name,microSeconds:r.micro.medianMs/1000,geometrySeconds:r.geometric.medianMs/1000,
    speedup:r.micro.medianMs/r.geometric.medianMs,scoreChangePercent:100*(r.geometric.selected.quality.score/r.micro.selected.quality.score-1),
    wire:[r.micro.metrics.wireLength,r.geometric.metrics.wireLength],crossings:[r.micro.metrics.crossings,r.geometric.metrics.crossings],
    pads:[r.micro.metrics.foreignPadHits,r.geometric.metrics.foreignPadHits],legal:[r.micro.selected.legal,r.geometric.selected.legal],
    timing:Object.fromEntries(['micro','geometric'].map(m=>{const rs=runs.filter(x=>x.name===r.name&&x.metric===m);return [m,{
        initialSeconds:median(rs.map(x=>x.initialMs))/1000,pairSeconds:median(rs.map(x=>x.pairsMs))/1000,
        totalRangeSeconds:[Math.min(...rs.map(x=>x.ms))/1000,Math.max(...rs.map(x=>x.ms))/1000],selectedStage:r[m].selected.stage,
    }];})),
    c9Links:Object.fromEntries(['micro','geometric'].map(m=>[m,Object.fromEntries(Object.entries(r[m].selected.quality.links).filter(([k])=>k.startsWith('C9.')))]))}));
writeFileSync(`${out}/analysis.json`,JSON.stringify({aggregate,differences},null,2));
let html=readFileSync(`${out}/comparison.html`,'utf8');
const overview=`<section id="overview"><h2>Итог по блокам (${rows.length})</h2><p>Сумма медиан: ${(aggregate.micro.nativeMs/1000).toFixed(1)} → ${(aggregate.geometric.nativeMs/1000).toFixed(1)} с. Это время выбранных блоков, а не полной платы. Меньшая оценка лучше.</p><table><tr><th>Блок</th><th>Micro, с</th><th>Geometry, с</th><th>Изменение общей оценки</th></tr>${differences.map(r=>`<tr><td>${r.name}</td><td>${r.microSeconds.toFixed(2)}</td><td>${r.geometrySeconds.toFixed(2)}</td><td>${r.scoreChangePercent.toFixed(1)}%</td></tr>`).join('')}</table><h3>Основной поиск / парные продолжения, с</h3><table><tr><th>Блок</th><th>Micro: beam + singles / pairs</th><th>Geometry: beam + singles / pairs</th></tr>${differences.map(r=>`<tr><td>${r.name}</td>${['micro','geometric'].map(m=>`<td>${r.timing[m].initialSeconds.toFixed(2)} / ${r.timing[m].pairSeconds.toFixed(2)}</td>`).join('')}</tr>`).join('')}</table><p>Метрика меняет траекторию поиска и отбор парных продолжений. Разницу полного времени нельзя целиком приписать стоимости A*. Исключение post-refine тоже меняет допуск гипотез к парному поиску: эти суммы нельзя напрямую сопоставлять с прошлым полным прогоном платы. <a href="analysis.json">Диапазоны замеров и связи C9</a>.</p></section>`;
html=html.replace(/<section id="overview">[\s\S]*?<\/section>/,'').replace('<section>',overview+'<section>');
const observations=`<p id="quality-observations">Допустимые результаты: ${aggregate.micro.legal}/${rows.length} и ${aggregate.geometric.legal}/${rows.length}. Всего пересечений линий: ${aggregate.micro.crossings} → ${aggregate.geometric.crossings}; чужих падов: ${aggregate.micro.foreignPadHits} → ${aggregate.geometric.foreignPadHits}. Суммарные показатели не заменяют проверку каждого блока.</p>`;
html=html.replace(/<p id="quality-observations">.*?<\/p>/,'').replace('<h3>Основной поиск',observations+'<h3>Основной поиск');
const c9=differences.find(r=>r.name==='current_iso');
if(c9){
    const links=m=>Object.values(c9.c9Links[m]).map(n=>n.toFixed(2)).join(' / ');
    html=html.replace(/<p id="c9-links">.*?<\/p>/,'').replace('<h2>current_iso</h2>',`<h2>current_iso</h2><p id="c9-links">Связи C9 → U2: ${links('micro')} мм с micro; ${links('geometric')} мм с geometry. Эти локальные изменения следует оценивать отдельно от суммарного score.</p>`);
}
writeFileSync(`${out}/comparison.html`,html);
// Keep millimetres at the same visual scale within each before/after pair.
// The renderer uses a fixed px/mm scale, but independent CSS fitting would
// otherwise magnify a landscape layout more than a portrait layout.
for(const r of rows){
    const pair=['micro','geometric'].map(m=>{
        const path=`${out}/${r.name}-${m}.svg`,svg=readFileSync(path,'utf8');
        const match=svg.match(/^<svg[^>]*width="([\d.]+)" height="([\d.]+)"[^>]*>([\s\S]*)<\/svg>\s*$/);
        assert.ok(match,path);return {path,svg,width:Number(match[1]),height:Number(match[2]),body:match[3]};
    });
    const width=Math.max(...pair.map(s=>s.width)),height=Math.max(...pair.map(s=>s.height));
    for(const s of pair)if(s.width!==width||s.height!==height)writeFileSync(s.path,
        `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><g transform="translate(${(width-s.width)/2} ${(height-s.height)/2})">${s.body}</g></svg>`);
}
for(let page=0;page<Math.ceil(rows.length/4);page++){
    const subset=rows.slice(page*4,page*4+4),w=760,h=480;
    const canvas=createCanvas(w*2,h*subset.length),ctx=canvas.getContext('2d');
    ctx.fillStyle='#edf2f7';ctx.fillRect(0,0,canvas.width,canvas.height);
    for(const [i,r]of subset.entries())for(const [col,m]of ['micro','geometric'].entries()){
        let svg=readFileSync(`${out}/${r.name}-${m}.svg`,'utf8');
        svg=svg.replace(/^(<svg[^>]*width=")([\d.]+)(" height=")([\d.]+)/,(_,a,w,b,h)=>`${a}${Number(w)*3}${b}${Number(h)*3}`);
        const image=await loadImage(Buffer.from(svg)),scale=Math.min((w-30)/image.width,(h-92)/image.height);
        const v=r[m];ctx.fillStyle='#172033';ctx.font='bold 20px Arial';ctx.fillText(`${r.name} — ${m}`,col*w+16,i*h+28);
        ctx.font='16px Arial';ctx.fillText(`${(v.medianMs/1000).toFixed(2)} s | MST ${v.metrics.wireLength} mm | crossings ${v.metrics.crossings} | pads ${v.metrics.foreignPadHits}`,col*w+16,i*h+54);
        ctx.drawImage(image,col*w+(w-image.width*scale)/2,i*h+78,image.width*scale,image.height*scale);
    }
    writeFileSync(`${out}/comparison-${page+1}.png`,canvas.toBuffer('image/png'));
}
writeFileSync(`${out}/README.md`,`# Isolated block solver: micro vs geometric\n\n${manifest.scope}\n\nTwo freshly computed runs per mode (no solve-cache or saved-result replay). Six native workers; no concurrent compiler, placement process or tests. Ordinary host background load is uncontrolled. Report generation is outside timed regions. Both rounds must produce identical poses; the harness asserts this. Input and binary hashes and per-hypothesis input hashes are in manifest.json. Frozen native inputs include child geometry from the earlier micro run, deliberately identical in A/B; this does not measure recursive all-geometry block solving or a full PCB. No post-refine or board packaging is called.\n\nNative timings include beam/singles plus at most two separately selected pair continuations. Both modes keep checkpoints and use the same role-independent blockQuality and legality gate. A missing legal candidate is explicitly reported, not treated as an acceptable layout. Every selected component rotation and layer is checked against input. Connection metrics use the same MST renderer inputs; intersections are geometric proxies, not proof of routing.\n\nRun: node --import tsx scripts/experiment-isolated-block-metric.mjs\n\nReport: node scripts/report-isolated-block-metric.mjs\n\nSummary: micro ${(aggregate.micro.nativeMs/1000).toFixed(2)} s, geometry ${(aggregate.geometric.nativeMs/1000).toFixed(2)} s (sum of per-block medians). These sums must not be compared directly with full-board elapsed time.\n`);
console.log(JSON.stringify({aggregate,differences}));
