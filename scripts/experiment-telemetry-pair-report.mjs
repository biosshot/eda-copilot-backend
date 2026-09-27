import {readFileSync,writeFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {boardAlignmentPolicy,boardAlignmentScore,alignmentErrors,alignmentHardHintsNoWorse,boardElectricalQuality,boardElectricalRegression} from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import {renderPlacementSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {componentBox,unionBoxes} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {createPcbLayout} from '../src/pcb-layout/pcb-auto-place/layout.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
import {encodeNativePostPlaceRefineProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

const dir=process.argv[2]??'docs/experiments/telemetry-pair-placement-2026-09-27';
const source='docs/experiments/telemetry-orientation-2026-09-27/after.json.gz';
const load=p=>JSON.parse(gunzipSync(readFileSync(p)));
const before=load(source),after=load(`${dir}/after.json.gz`);
const raw=readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'),input=JSON.parse(raw);
const addon=loadNativeBoardPacker(),roots=after.hypotheses[0].roots;
const policy=boardAlignmentPolicy(input,roots);
const withPoses=ps=>roots.map(r=>({...r,placements:r.placements.map(p=>ps.find(q=>q.designator===p.designator)),
    bbox:unionBoxes(r.placements.map(p=>componentBox(input.components.find(c=>c.designator===p.designator),ps.find(q=>q.designator===p.designator))))}));
const fixed=new Set(input.components.filter(c=>c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace).map(c=>c.designator));
const capture=(tag,ps)=>{
    const rs=withPoses(ps),report=createPlacementReport(input,ps);
    for(const clean of [true,false])writeFileSync(`${dir}/${tag}-board${clean?'-clean':''}.svg`,renderPlacementSvg(input,ps,{ratsnest:!clean,ratsnestTopology:'mst',signalPaths:false}));
    return {tag,metrics:placementMetrics(input,ps),alignment:alignmentErrors(rs,policy.pairs),penalty:boardAlignmentScore(rs,policy),
        geometryOk:report.ok,absoluteValid:addon.validatePlacement(encodeNativePostPlaceRefineProblem(input,ps,1)),
        mandatory:report.hintViolations.filter(v=>v.hint.priority==='critical'||v.hint.hard),
        fixedChanges:ps.filter(p=>fixed.has(p.designator)&&['x','y','rotate','layer'].some(k=>p[k]!==before.placements.find(q=>q.designator===p.designator)[k])).map(p=>p.designator)};
};
const final=[capture('before',before.placements),capture('after',after.placements)];
const packed=after.hypotheses.map((h,i)=>capture(i?'proposal':'baseline',h.roots.flatMap(r=>r.placements)));
const pps=after.hypotheses.map(h=>h.roots.flatMap(r=>r.placements));
const diagnostics=(after.report.graphReport?.diagnostics??[]).filter(d=>d.code==='v2_solver');
const checks={
    finalHardHintsNoWorse:alignmentHardHintsNoWorse(createPlacementReport(input,before.placements),createPlacementReport(input,after.placements)),
    finalValidChange:addon.validatePlacementChange(encodeNativePostPlaceRefineProblem(input,before.placements,1),after.placements),
    finalElectricalRegression:boardElectricalRegression(boardElectricalQuality(input,before.placements),boardElectricalQuality(input,after.placements))??null,
    proposalHardHintsNoWorse:alignmentHardHintsNoWorse(...pps.map(ps=>createPlacementReport(input,ps))),
    proposalElectricalRegression:boardElectricalRegression(...pps.map(ps=>boardElectricalQuality(input,ps)))??null,
};
const baselineReport=createPlacementReport(input,pps[0]);
const worsened=createPlacementReport(input,pps[1]).hintViolations.filter(v=>v.hint.priority==='critical'||v.hint.hard).flatMap(v=>{
    const b=baselineReport.hintViolations.find(b=>JSON.stringify(b.hint)===JSON.stringify(v.hint)&&b.expected===v.expected);
    return !b||(typeof b.actual==='number'&&typeof v.actual==='number'&&(v.expected.startsWith('>=')?v.actual<b.actual:v.actual>b.actual))
        ?[{hint:v.hint,before:b?.actual??'satisfied',after:v.actual,expected:v.expected}]:[];
});
const changed=after.placements.filter(p=>['x','y','rotate','layer'].some(k=>p[k]!==before.placements.find(q=>q.designator===p.designator)[k])).length;
const crops=[['iso',['U1','U2']],['inductors',['L3','L4']],['charge',['R38','R44']],['usb',['J5']]];
for(const [tag,result] of [['before',before],['after',after]]){
    const asm=createBoardAssemble(createPcbLayout(input,result.placements),{preserveBoard:true,preservedComponents:fixed});
    writeFileSync(`${dir}/${tag}.assemble.json`,JSON.stringify(BoardAssembleSchema().parse({components:asm.components}),null,2));
}
for(const [name,refs] of crops){
    const members=new Set(roots.filter(r=>r.placements.some(p=>refs.includes(p.designator))).flatMap(r=>r.placements.map(p=>p.designator)));
    const bounds=unionBoxes([before,after].flatMap(r=>r.placements.filter(p=>members.has(p.designator)).map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p))));
    bounds.left-=2;bounds.right+=2;bounds.top-=2;bounds.bottom+=2;
    for(const [tag,r] of [['before',before],['after',after]])writeFileSync(`${dir}/${tag}-${name}.svg`,renderPlacementSvg(input,r.placements.filter(p=>members.has(p.designator)),{bounds,ratsnestTopology:'mst',signalPaths:false}));
}
const oldPack=before.hypotheses[0].roots.flatMap(r=>r.placements);
const poseKey=ps=>JSON.stringify(ps.map(p=>[p.designator,p.x,p.y,p.rotate,p.layer]).sort((a,b)=>a[0].localeCompare(b[0])));
const summary={source,inputSha256:createHash('sha256').update(raw).digest('hex'),
    nativeSha256:createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex'),
    ms:after.ms,changed,policy,checks,final,packed,diagnostics,worsened,ordinaryPackingPreserved:poseKey(oldPack)===poseKey(pps[0])};
writeFileSync(`${dir}/summary.json`,JSON.stringify(summary,null,2));
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;'),n=x=>Number(x).toFixed(2);
const pair=(a,b,scope,labels=['До','После'])=>`<div class="pair">${[a,b].map((tag,i)=>`<figure><figcaption>${labels[i]}</figcaption><a href="${tag}-${scope}.svg"><img src="${tag}-${scope}${scope==='board'?'-clean':''}.svg" data-tag="${tag}" class="${scope}"></a></figure>`).join('')}</div>`;
const row=(label,values)=>`<tr><td>${esc(label)}</td>${values.map(v=>`<td>${esc(v)}</td>`).join('')}</tr>`;
const metrics=items=>`<table><tr><th>Показатель</th><th>До</th><th>После</th></tr>${[['Длина MST, мм','wireLength'],['Пересечения линий','crossings'],['Пересечения чужих падов','foreignPadHits']].map(([label,key])=>row(label,items.map(r=>r.metrics[key]))).join('')}${row('Штраф выравнивания',items.map(r=>n(r.penalty)))}</table>`;
const axes=items=>`<table><tr><th>Опорные центры</th><th>Отклонение до → после, мм</th><th>Компоненты направления</th><th>Разница угла до → после</th></tr>${policy.pairs.map((p,i)=>row(`${p.anchorA??p.a} / ${p.anchorB??p.b}`,[items.map(s=>n(s.alignment[i].error)).join(' → '),p.orientation?`${p.orientation.a} / ${p.orientation.b}`:'—',items.map(s=>s.alignment[i].orientationError??'—').join(' → ')])).join('')}</table>`;
writeFileSync(`${dir}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Telemetry — совместное размещение похожих блоков</title>
<style>body{font:16px system-ui;background:#f1f5f9;color:#172033;max-width:1600px;margin:24px auto;padding:0 24px}p{max-width:1150px;line-height:1.5}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;background:white;border:1px solid #cbd5e1;border-radius:10px;padding:12px}img{width:100%;max-height:850px;object-fit:contain}figcaption{font-weight:bold}table{border-collapse:collapse;width:100%;background:white;margin:20px 0}td,th{padding:10px;border-bottom:1px solid #ddd;text-align:left}.notice{background:#fff7ed;border-left:4px solid #ea580c;padding:14px}a{color:#0369a1}pre{white-space:pre-wrap}</style>
<h1>Telemetry: совместное размещение похожих блоков</h1>
<p>Один поиск с мягкими зазорами, штрафами за несовпадение осей и поворотов и дополнительными кандидатами постановки пары. Обычная упаковка сохранена как страховочный результат. Внутренние компоновки блоков одинаковы в двух попытках; альтернативные пересборки не подставлялись.</p>
<p>После выбора упаковки сохранены прежний локальный выбор вариантов блоков, общий postrefine и небольшое финальное выравнивание. Они не исключены из рабочего алгоритма. Контрольная пара индуктивностей — L3/L4 (hv_pos/hv_neg); L2 относится к logic_power.</p>
<p><strong>Изменено компонентов в итоговой плате: ${changed}. ${changed?'Результат и проверки ниже.':'Итоговая плата осталась прежней; улучшение на Telemetry не подтверждено.'}</strong></p>
<p>Позиционный штраф: допуск 0,15 мм, плавный переход к линейному росту после 1 мм, вес 8. Поворот: от 0 при совпадении до 24 при разнице 180°. Оба взвешиваются похожестью; бонуса за сближение нет. Порог похожести 0,78, для совместной постановки 0,85. Центр позиции и компонент направления могут различаться.</p>
<p>Обычные позиции не убраны: 32 места в предварительном списке плюс до 16 дополнительных позиций на осях. Совместная постановка добавляет переходы на два блока; состояния сравниваются на равной глубине. Перебираются допустимые повороты, четыре стороны и несколько зазоров. Фиксированная механика не участвует в перемещении пары.</p>
<p>Время: ${n(after.ms/1000)} с. Обычная упаковка воспроизвела прежние позиции: ${summary.ordinaryPackingPreserved}. Фиксированных компонентов сдвинуто: ${final[1].fixedChanges.length}.</p>
<p class="notice">Обязательные ограничения не ухудшены: ${checks.finalHardHintsNoWorse}. Проверка изменения native: ${checks.finalValidChange}. Абсолютная проверка: ${final.map(s=>s.absoluteValid).join(' → ')}; исходные нарушения не скрыты. Электрическая регрессия по защитной проверке: ${checks.finalElectricalRegression??'нет'}. Трассировка не выполнялась.</p>
<h2>Принятая полная плата: до и после</h2><label><input id="lines" type="checkbox"> Показать линии связей</label>${metrics(final)}${axes(final)}${pair('before','after','board')}
${crops.map(([name,refs])=>`<h2>${refs.join(' / ')}</h2>${pair('before','after',name)}`).join('')}
<details><summary>Что предложил новый поиск до итогового отбора</summary><p>Слева — обычная упаковка тех же блоков, справа — новое предложение. Правая картинка не означает принятия результата. Обязательные ограничения не ухудшены: ${checks.proposalHardHintsNoWorse}. Электрическая проверка: ${checks.proposalElectricalRegression??'без регрессии'}.</p>${metrics(packed)}${axes(packed)}${pair('baseline','proposal','board',['Обычная упаковка — страховка','Предложение нового поиска'])}<table><tr><th>Ухудшенное правило</th><th>До</th><th>В предложении</th><th>Требование</th></tr>${worsened.map(v=>row(`${v.hint.source.block_name??v.hint.source.designator} → ${v.hint.target?.block_name??v.hint.target?.designator??'all'}`,[v.before,v.after,v.expected])).join('')}</table><pre>${esc(diagnostics.map(d=>d.message).join('\n'))}</pre></details>
<p><a href="after.assemble.json">ASM принятого результата</a> · <a href="before.assemble.json">ASM до</a> · <a href="summary.json">Численные результаты</a>. В EasyEDA ничего не применялось.</p>
<script>document.querySelector('#lines').onchange=e=>document.querySelectorAll('img.board').forEach(img=>img.src=img.dataset.tag+'-board'+(e.target.checked?'':'-clean')+'.svg');</script></html>`);
console.log(JSON.stringify({changed,checks,ordinaryPackingPreserved:summary.ordinaryPackingPreserved,final:final.map(s=>({metrics:s.metrics,alignment:s.alignment})),packed:packed.map(s=>({metrics:s.metrics,alignment:s.alignment})),diagnostics},null,2));
