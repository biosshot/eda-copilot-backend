import {readFileSync,writeFileSync,readdirSync,mkdirSync} from 'node:fs';
import {gunzipSync,gzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {applyNativeBoardPackSolution} from '../src/pcb-layout/pcb-auto-place-v2/native/apply-board-solution.ts';
import {blockQuality,legalBlockCandidate,selectBlockCandidates} from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import {selectPairSeeds} from '../src/pcb-layout/pcb-auto-place-v2/block-search-stages.ts';
import {createClearanceResolver} from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import {renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

// Frozen native inputs isolate the search metric from child solving, board
// packing, native post-refine, and JS/native result-cache hits.
const root='docs/experiments/placement-performance-2026-09-28/Telemetry';
const source=`${root}/staged-final`;
const out=`${root}/isolated-block-metric`;mkdirSync(out,{recursive:true});
const inputBytes=readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json');
const input=JSON.parse(inputBytes),clearance=createClearanceResolver(input);
const hash=b=>createHash('sha256').update(b).digest('hex');
const addon=loadNativeBoardPacker();
for(const method of ['solveBoardPacked','refinePostPlacement','scoreRouteLayout','scoreRouteLayoutWithObstacles','prepareRouteLayoutComparison','compareRouteLayoutCandidate']) {
    Object.defineProperty(addon,method,{configurable:true,value:()=>{throw Error(`Excluded from isolated benchmark: ${method}`);}});
}
const captured=readdirSync(source).filter(f=>/^block-.*\.json\.gz$/.test(f)).sort((a,b)=>a.localeCompare(b,undefined,{numeric:true})).map(file=>({file,...JSON.parse(gunzipSync(readFileSync(`${source}/${file}`)))}))
    .filter(d=>d.problem.deferPairs&&!d.problem.pairSeed);
const names=process.argv.slice(2).length?process.argv.slice(2):['usb_input','usb_charge','lte_power','voltage_iso','current_iso','adc','low_charge_pos','low_charge_neg'];
const rounds=Number(process.env.BLOCK_METRIC_ROUNDS??2),threads=6;
const manifest={nativeHash:hash(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')),inputHash:hash(inputBytes),
    startedAt:new Date().toISOString(),threads,rounds,names,scope:'Frozen native block inputs, beam + singles, up to two selected pair continuations. No board pack or post-refine. Common role-independent checkpoint selection.',cases:{}};
const cases=names.map(name=>{
    const matches=captured.filter(d=>d.problem.components.some(c=>c.blockName===name));
    const size=Math.max(...matches.map(d=>d.problem.components.length));
    const selected=matches.filter(d=>d.problem.components.length===size);
    assert.ok(selected.length,`Missing block ${name}`);
    const inventory=selected[0].problem.components.map(c=>c.designator).sort();
    for(const d of selected)assert.deepEqual(d.problem.components.map(c=>c.designator).sort(),inventory);
    manifest.cases[name]=selected.map(d=>({file:d.file,hash:hash(JSON.stringify(d.problem)),components:size,primitives:d.problem.primitives.length}));
    writeFileSync(`${out}/${name}-inputs.json.gz`,gzipSync(JSON.stringify(selected.map(d=>d.problem))));
    return {name,problems:selected.map(d=>d.problem)};
});
writeFileSync(`${out}/manifest.json`,JSON.stringify(manifest,null,2));
const runs=[];
function run({name,problems},metric,round){
    const modified=problems.map(p=>({...structuredClone(p),experiments:{...p.experiments,routingMetric:metric}}));
    const pool=[],seeds=[],all=[];
    const add=(problem,snapshot,index)=>{
        const primitives=applyNativeBoardPackSolution(problem.primitives.map(p=>({...p,children:[]})),snapshot,4);
        assert.deepEqual(primitives.flatMap(p=>p.placements).map(p=>p.designator).sort(),problem.components.map(c=>c.designator).sort());
        const candidate={hypothesis:String(index),stage:snapshot.stage,primitives,quality:blockQuality(input,primitives),index,
            nativeHard:snapshot.rank.hardCount,legal:legalBlockCandidate(input,primitives,clearance,primitives.some(p=>p.locked))};
        all.push(candidate);
        if(candidate.nativeHard===0&&candidate.legal)pool.push(candidate);
        if(snapshot.stage==='singles'&&candidate.nativeHard===0)seeds.push(candidate);
        return candidate;
    };
    const invoke=ps=>ps.length===1?[addon.solveBlockPrimitives(ps[0])]:addon.solveBlockPrimitivesBatch(ps,threads);
    const t0=performance.now();const initial=invoke(modified);const initialMs=performance.now()-t0;
    initial.forEach((solution,i)=>solution.checkpoints.filter(c=>c.stage!=='pairs').forEach(c=>add(modified[i],c,i)));
    let choices=selectPairSeeds(seeds,pool);
    if(!choices.length&&!pool.length)choices=seeds.filter(c=>c.primitives.length<=12).slice(0,2);
    const pairProblems=choices.map(c=>{
        const p={...modified[c.index],pairSeed:initial[c.index].pairSeed};delete p.deferPairs;assert.ok(p.pairSeed);return p;
    });
    const t1=performance.now();const pairs=pairProblems.length?invoke(pairProblems):[];const pairsMs=performance.now()-t1;
    pairs.forEach((solution,i)=>solution.checkpoints.forEach(c=>add(pairProblems[i],c,choices[i].index)));
    // An invalid result is shown explicitly, never silently called accepted.
    const selected=selectBlockCandidates(pool);
    const best=selected[0]??all.filter(c=>c.nativeHard===0).sort((a,b)=>a.quality.score-b.quality.score)[0]??all[0];
    assert.ok(best);
    const placements=best.primitives.flatMap(p=>p.placements);
    const result={name,metric,round,initialMs,pairsMs,ms:initialMs+pairsMs,hypotheses:problems.length,pairHypotheses:choices.map(c=>c.index),
        legalCandidates:pool.length,retained:selected.length,selected:{hypothesis:best.hypothesis,stage:best.stage,quality:best.quality,legal:best.legal,nativeHard:best.nativeHard},
        metrics:placementMetrics(input,placements),placements};
    runs.push(result);
    writeFileSync(`${out}/${name}-${metric}-${round}.json.gz`,gzipSync(JSON.stringify({initial,pairs,candidates:all,result})));
    writeFileSync(`${out}/runs.json`,JSON.stringify(runs,null,2));
    console.log(JSON.stringify({...result,placements:undefined,selected:{stage:best.stage,score:best.quality.score,legal:best.legal,nativeHard:best.nativeHard}}));
    if(round===0)writeFileSync(`${out}/${name}-${metric}.svg`,renderPlacementSubsetSvg(input,placements,{ratsnestTopology:'mst',signalPaths:true,padding:2}).replace(/[ \t]+$/gm,''));
}
for(let round=0;round<rounds;round++)for(const [i,c]of (round%2?[...cases].reverse():cases).entries()){
    for(const metric of (i+round)%2?['geometric','micro']:['micro','geometric'])run(c,metric,round);
}
const median=a=>{a=[...a].sort((a,b)=>a-b);return(a[Math.floor((a.length-1)/2)]+a[Math.floor(a.length/2)])/2;};
const rows=names.map(name=>({name,...Object.fromEntries(['micro','geometric'].map(metric=>{
    const rs=runs.filter(r=>r.name===name&&r.metric===metric);
    for(const r of rs)assert.deepEqual(r.placements,rs[0].placements,`${name}/${metric}: nondeterministic placements`);
    return [metric,{...rs[0],medianMs:median(rs.map(r=>r.ms)),times:rs.map(r=>r.ms)}];
}))}));
writeFileSync(`${out}/summary.json`,JSON.stringify({manifest,rows},null,2));
const n=x=>Number(x).toFixed(2),esc=x=>String(x).replaceAll('&','&amp;').replaceAll('<','&lt;');
const table=r=>`<table><tr><th>Показатель</th><th>Micro</th><th>Geometry</th></tr>${[
    ['Время, с',r.micro.medianMs/1000,r.geometric.medianMs/1000],['Общая оценка',r.micro.selected.quality.score,r.geometric.selected.quality.score],
    ...[['MST, мм','wireLength'],['Пересечения линий','crossings'],['Пересечения чужих падов','foreignPadHits'],['Площадь, мм²','area']].map(([label,key])=>[label,r.micro.metrics[key],r.geometric.metrics[key]])
].map(([label,a,b])=>`<tr><td>${label}</td><td>${n(a)}</td><td>${n(b)}</td></tr>`).join('')}<tr><td>Допустимый результат</td><td>${r.micro.selected.legal&&r.micro.selected.nativeHard===0}</td><td>${r.geometric.selected.legal&&r.geometric.selected.nativeHard===0}</td></tr></table>`;
writeFileSync(`${out}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Telemetry: isolated block solver</title><style>body{font:16px system-ui;background:#edf2f7;color:#172033;max-width:1500px;margin:24px auto;padding:0 20px}section{background:white;margin:20px 0;padding:20px;border-radius:12px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0}img{width:100%;max-height:780px;object-fit:contain}table{border-collapse:collapse;width:100%}td,th{padding:7px;border-bottom:1px solid #ddd;text-align:left}p{line-height:1.5}</style><h1>Telemetry: блоки без микророутера</h1><p>Слева micro, справа geometric. ${rounds} повтора, время — медиана, шесть потоков на независимые гипотезы. Порядок режимов чередуется. Фоновая нагрузка не контролируется.</p><p>Изолированный native block solver: одинаковые сохранённые входы и дочерние группы, beam → одиночные → до двух парных продолжений. Общая оценка выбирает допустимый результат из промежуточных состояний. Post-refine и упаковка платы отключены в обоих режимах. Время включает только native-вызовы; SVG, проверка качества и сериализация отчёта не входят. Это не прогноз времени полной платы. Отсутствие допустимого кандидата отмечено явно.</p><p><a href="summary.json">Все метрики и provenance</a></p>${rows.map(r=>`<section><h2>${esc(r.name)}</h2><p>Гипотез: ${r.micro.hypotheses}. Парных продолжений: ${r.micro.pairHypotheses.length} / ${r.geometric.pairHypotheses.length}. Выбран этап: ${r.micro.selected.stage} / ${r.geometric.selected.stage}.</p>${table(r)}<div class="pair">${['micro','geometric'].map(m=>`<figure><figcaption>${m}</figcaption><a href="${r.name}-${m}.svg"><img src="${r.name}-${m}.svg"></a></figure>`).join('')}</div></section>`).join('')}</html>`);
console.log(JSON.stringify({done:true,out,rows:rows.map(r=>({name:r.name,microMs:r.micro.medianMs,geometryMs:r.geometric.medianMs,score:[r.micro.selected.quality.score,r.geometric.selected.quality.score],legal:[r.micro.selected.legal,r.geometric.selected.legal]}))}));
