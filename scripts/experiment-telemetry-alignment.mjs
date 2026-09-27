import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {refineBoardAlignment,blockSimilarity,alignmentAnchor,BOARD_ALIGNMENT_POLICY} from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import {renderPlacementSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {componentBox,unionBoxes,boxGap} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {createPcbLayout} from '../src/pcb-layout/pcb-auto-place/layout.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {encodeNativePostPlaceRefineProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

const dir='docs/experiments/telemetry-alignment-2026-09-27';
mkdirSync(dir,{recursive:true});
const inputRaw=readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json');
const baselineRaw=readFileSync('docs/experiments/telemetry-anchored-2026-09-27/after-final.json.gz');
const input=JSON.parse(inputRaw), baseline=JSON.parse(gunzipSync(baselineRaw));
const roots=baseline.stages.find(s=>s.name==='01-v2-tree').data.root.children;
const start=performance.now();
const result=refineBoardAlignment(input,roots,baseline.placements);
const ms=performance.now()-start;
const report=createPlacementReport(input,result.placements);
const problem=encodeNativePostPlaceRefineProblem(input,baseline.placements,1);
const addon=loadNativeBoardPacker();
const similarities=[];
for(let i=0;i<roots.length;i++)for(let j=i+1;j<roots.length;j++){
    const cs=r=>input.components.filter(c=>r.placements.some(p=>p.designator===c.designator));
    const a=cs(roots[i]),b=cs(roots[j]),similarity=blockSimilarity(a,b);
    const bounds=r=>unionBoxes(baseline.placements.filter(p=>r.placements.some(q=>q.designator===p.designator)).map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p)));
    if(similarity>=.65)similarities.push({a:roots[i].label,b:roots[j].label,similarity,anchorA:alignmentAnchor(a),anchorB:alignmentAnchor(b),gap:boxGap(bounds(roots[i]),bounds(roots[j]))});
}
const summaries=['before','after'].map(tag=>{
    const placements=tag==='before'?baseline.placements:result.placements;
    const r=createPlacementReport(input,placements);
    for(const clean of [false,true])writeFileSync(`${dir}/${tag}-board${clean?'-clean':''}.svg`,renderPlacementSvg(input,placements,{ratsnest:!clean,ratsnestTopology:'mst',signalPaths:false}));
    const fixed=new Set(input.components.filter(c=>c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace).map(c=>c.designator));
    const asm=createBoardAssemble(createPcbLayout(input,placements),{preserveBoard:true,preservedComponents:fixed});
    writeFileSync(`${dir}/${tag}.assemble.json`,JSON.stringify(BoardAssembleSchema().parse({components:asm.components}),null,2));
    return {tag,metrics:placementMetrics(input,placements),reportOk:r.ok,strictValid:addon.validatePlacement(encodeNativePostPlaceRefineProblem(input,placements,1)),
        criticalClearances:r.hintViolations.filter(v=>v.hint.relation==='clearance'&&v.hint.priority==='critical')};
});
const changes=result.placements.flatMap(p=>{const b=baseline.placements.find(q=>q.designator===p.designator);return ['x','y','rotate','layer'].some(k=>b[k]!==p[k])?[{designator:p.designator,before:b,after:p}]:[];});
const crops=[['iso',['U1','U2']],['inductors',['L3','L4']],['charge',['U16','U20']]];
for(const [name,refs] of crops){
    const members=new Set(roots.filter(r=>r.placements.some(p=>refs.includes(p.designator))).flatMap(r=>r.placements.map(p=>p.designator)));
    const bounds=unionBoxes([baseline.placements,result.placements].flatMap(ps=>ps.filter(p=>members.has(p.designator)).map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p))));
    bounds.left-=2;bounds.top-=2;bounds.right+=2;bounds.bottom+=2;
    for(const [tag,ps] of [['before',baseline.placements],['after',result.placements]])writeFileSync(`${dir}/${tag}-${name}.svg`,renderPlacementSvg(input,ps.filter(p=>members.has(p.designator)),{bounds,ratsnestTopology:'mst',signalPaths:false}));
}
const summary={policy:BOARD_ALIGNMENT_POLICY,ms,similarities:similarities.sort((a,b)=>b.similarity-a.similarity),pairs:result.pairs,before:result.before,after:result.after,moves:result.moves,evaluated:result.evaluated,rejected:result.rejected,changes,summaries,
    validChange:addon.validatePlacementChange(problem,result.placements),inputSha256:createHash('sha256').update(inputRaw).digest('hex'),baselineSha256:createHash('sha256').update(baselineRaw).digest('hex')};
writeFileSync(`${dir}/summary.json`,JSON.stringify(summary,null,2));
writeFileSync(`${dir}/after.json`,JSON.stringify({placements:result.placements,report,diagnostics:result.diagnostics},null,2));
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const names=new Map(roots.map(r=>[r.id,r.label]));
const n=v=>Number(v).toFixed(2);
const pair=(scope)=>`<div class="pair">${['before','after'].map(tag=>`<figure><figcaption>${tag==='before'?'До':'После'}</figcaption><a href="${tag}-${scope}.svg"><img class="${scope}" data-tag="${tag}" src="${tag}-${scope}.svg"></a></figure>`).join('')}</div>`;
writeFileSync(`${dir}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Telemetry: мягкое выравнивание</title><style>body{font:16px system-ui;color:#172033;background:#f1f5f9;max-width:1600px;margin:24px auto;padding:0 24px}p{max-width:1150px;line-height:1.5}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{background:white;border:1px solid #cbd5e1;border-radius:10px;margin:0;padding:12px}img{width:100%;max-height:850px;object-fit:contain}figcaption{font-weight:bold}table{border-collapse:collapse;width:100%;background:white;margin:20px 0}td,th{padding:10px;text-align:left;border-bottom:1px solid #ddd}.notice{background:#fff7ed;padding:14px;border-left:4px solid #ea580c}a{color:#0369a1}</style>
<h1>Telemetry: мягкое выравнивание похожих блоков</h1>
<p>«До» — последний результат с фиксированным USB и мягкими зазорами (after-final). «После» — только новый завершающий проход по той же плате, без повторной сборки блоков или перепаковки. Это изолированная проверка нового этапа, который теперь вызывается и в основном плейсере после postrefine.</p>
<p><strong>${result.moves.length ? "Приняты допустимые выравнивания." : "На этой компоновке безопасных улучшений среди проверенных кандидатов не найдено. До и после совпадают; исходная плата сохранена."}</strong></p>
<p>Порог похожести ${BOARD_ALIGNMENT_POLICY.similarity}; учитываются состав, типы локальных соединений и размеры основного корпуса. Дополнительный диод снижает сходство постепенно. Рассматриваются только соседние блоки, перенос каждого компонента ограничен ${BOARD_ALIGNMENT_POLICY.maxShift} мм от исходного состояния. Ориентации и внутренняя сборка сохраняются. Выравнивание — ограниченный мягкий штраф, а не требование.</p>
<p>Проверено ${result.evaluated} кандидатов за ${n(ms/1000)} с, принято ${result.moves.length} перемещений пар; изменено ${changes.length} компонентов. Проверка изменения native: ${summary.validChange}. Результат не трассировался; линии — MST.</p>
<p class="notice">Исходные нарушения обязательного межгруппового зазора не исправляются этим проходом. До: ${summaries[0].criticalClearances.length}, после: ${summaries[1].criticalClearances.length}. Абсолютная проверка платы: ${summaries.map(s=>s.strictValid).join(' → ')}. ASM предназначен для сравнения; в EasyEDA ничего не применялось.</p>
<table><tr><th>Метрика платы</th><th>До</th><th>После</th></tr>${[['Длина MST, мм','wireLength'],['Пересечения линий','crossings'],['Пересечения чужих падов','foreignPadHits']].map(([label,key])=>`<tr><td>${label}</td>${summaries.map(s=>`<td>${s.metrics[key]}</td>`).join('')}</tr>`).join('')}</table>
<p>Причины отклонения (первая сработавшая проверка): ${esc(JSON.stringify(result.rejected))}. Пары, у которых для выравнивания требуется слишком большой перенос, не порождают кандидатов.</p>
<h2>Пары, прошедшие порог и фильтр соседства</h2><table><tr><th>Блоки</th><th>Центры</th><th>Сходство</th><th>Смещение осей до, мм</th><th>После, мм</th></tr>${result.before.map((p,i)=>`<tr><td>${esc(names.get(p.a))} / ${esc(names.get(p.b))}</td><td>${p.anchorA??'центр блока'} / ${p.anchorB??'центр блока'}</td><td>${n(p.similarity)}</td><td>${n(p.error)}</td><td>${n(result.after[i].error)}</td></tr>`).join('')}</table>
<p>U1/U2 распознаны как похожие, но отфильтрованы по расстоянию: зазор между габаритами ${n(similarities.find(p=>[p.anchorA,p.anchorB].includes("U1")&&[p.anchorA,p.anchorB].includes("U2"))?.gap)} мм превышает локальный порог ${BOARD_ALIGNMENT_POLICY.neighbourGap} мм. Механизм не притягивает их через плату.</p>
<h2>Полная плата</h2><label><input id="clean" type="checkbox"> Скрыть линии связей</label>${pair('board')}
${crops.map(([name,refs])=>`<h2>${refs.join(' / ')}</h2>${pair(name)}`).join('')}
<p><a href="after.assemble.json">ASM после</a> · <a href="before.assemble.json">ASM до</a> · <a href="summary.json">Диагностика и все изменения</a>. ASM содержит только размещение подвижных компонентов.</p>
<script>document.querySelector('#clean').onchange=e=>document.querySelectorAll('img.board').forEach(img=>img.src=img.dataset.tag+'-board'+(e.target.checked?'-clean':'')+'.svg');</script></html>`);
console.log(JSON.stringify({ms,pairs:result.before,after:result.after,moves:result.moves,evaluated:result.evaluated,metrics:summaries.map(s=>s.metrics),validChange:summary.validChange,changes:changes.map(c=>c.designator)},null,2));
