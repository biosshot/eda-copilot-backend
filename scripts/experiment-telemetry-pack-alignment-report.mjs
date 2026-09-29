import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {boardAlignmentPolicy,boardAlignmentScore,alignmentErrors,alignmentHardHintsNoWorse} from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import {renderPlacementSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {componentBox,unionBoxes} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {createPcbLayout} from '../src/pcb-layout/pcb-auto-place/layout.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
import {encodeNativePostPlaceRefineProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

const dir=process.argv[2]??'docs/experimental/pcb/telemetry-pack-alignment-2026-09-27';
const orientationRun=dir.includes('orientation');
const inputRaw=readFileSync('docs/experimental/pcb/global-placement-2026-09-27/Telemetry/input.json');
const input=JSON.parse(inputRaw);
const source=process.argv[3]??'docs/experimental/pcb/telemetry-anchored-2026-09-27/after-final.json.gz';
const results=[source,`${dir}/after.json.gz`].map(path=>JSON.parse(gunzipSync(readFileSync(path))));
const roots=results[0].stages[0].data.root.children;
const policy=boardAlignmentPolicy(input,roots);
const currentRoots=ps=>roots.map(r=>({...r,placements:r.placements.map(p=>ps.find(q=>q.designator===p.designator)),
    bbox:unionBoxes(ps.filter(p=>r.placements.some(q=>q.designator===p.designator)).map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p)))}));
const addon=loadNativeBoardPacker();
const n=v=>Number(v).toFixed(2);
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const fixed=new Set(input.components.filter(c=>c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace).map(c=>c.designator));
const summary=results.map((r,i)=>{
    const tag=i?'after':'before',ps=r.placements,rs=currentRoots(ps),report=createPlacementReport(input,ps);
    for(const clean of [false,true])writeFileSync(`${dir}/${tag}-board${clean?'-clean':''}.svg`,renderPlacementSvg(input,ps,{ratsnest:!clean,ratsnestTopology:'mst',signalPaths:false}));
    const asm=createBoardAssemble(createPcbLayout(input,ps),{preserveBoard:true,preservedComponents:fixed});
    writeFileSync(`${dir}/${tag}.assemble.json`,JSON.stringify(BoardAssembleSchema().parse({components:asm.components}),null,2));
    const packed=r.stages[0].placements;
    return {tag,metrics:placementMetrics(input,ps),alignment:alignmentErrors(rs,policy.pairs),alignmentScore:boardAlignmentScore(rs,policy),
        packedAlignment:alignmentErrors(currentRoots(packed),policy.pairs),packedAlignmentScore:boardAlignmentScore(currentRoots(packed),policy),
        reportOk:report.ok,strictValid:addon.validatePlacement(encodeNativePostPlaceRefineProblem(input,ps,1)),
        criticalClearances:report.hintViolations.filter(v=>v.hint.relation==='clearance'&&v.hint.priority==='critical'),
        fixedChanges:ps.filter(p=>fixed.has(p.designator)&&['x','y','rotate','layer'].some(k=>p[k]!==results[0].placements.find(q=>q.designator===p.designator)[k])).map(p=>p.designator)};
});
const crops=[['iso',['U1','U2']],['inductors',['L3','L4']],['charge',['U16','U20']],['usb',['J5']]];
for(const [name,refs] of crops){
    const members=new Set(roots.filter(r=>r.placements.some(p=>refs.includes(p.designator))).flatMap(r=>r.placements.map(p=>p.designator)));
    const bounds=unionBoxes(results.flatMap(r=>r.placements.filter(p=>members.has(p.designator)).map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p))));
    bounds.left-=2;bounds.top-=2;bounds.right+=2;bounds.bottom+=2;
    results.forEach((r,i)=>writeFileSync(`${dir}/${i?'after':'before'}-${name}.svg`,renderPlacementSvg(input,r.placements.filter(p=>members.has(p.designator)),{bounds,ratsnestTopology:'mst',signalPaths:false})));
}
const diagnostics=(results[1].report.graphReport?.diagnostics??[]).filter(d=>d.code==='v2_solver');
let probe=existsSync(`${dir}/probe-summary.json`)?JSON.parse(readFileSync(`${dir}/probe-summary.json`)):null;
if(results[1].hypotheses?.length){
    const cases=results[1].hypotheses.slice(0,2).map((h,i)=>({tag:i?'aligned':'ordinary',rank:h.rank,metrics:placementMetrics(input,h.roots.flatMap(p=>p.placements)),alignmentScore:boardAlignmentScore(h.roots,policy),alignment:alignmentErrors(h.roots,policy.pairs)}));
    results[1].hypotheses.slice(0,2).forEach((h,i)=>{for(const clean of [false,true])writeFileSync(`${dir}/probe-${i?'aligned':'ordinary'}${clean?'-clean':''}.svg`,renderPlacementSvg(input,h.roots.flatMap(p=>p.placements),{ratsnest:!clean,ratsnestTopology:'mst',signalPaths:false}));});
    probe={cases,selected:'см. диагностику полного отбора',hardHintsNoWorse:alignmentHardHintsNoWorse(...results[1].hypotheses.slice(0,2).map(h=>createPlacementReport(input,h.roots.flatMap(p=>p.placements)))),diagnostics:diagnostics.filter(d=>d.nodeId==='block-portfolio-repack')};
}
const baselinePackingReport=results[1].hypotheses?.length?createPlacementReport(input,results[1].hypotheses[0].roots.flatMap(p=>p.placements)):null;
const hypothesisSummary=(results[1].hypotheses??[]).map((h,i)=>{
    const ps=h.roots.flatMap(p=>p.placements),report=createPlacementReport(input,ps);
    for(const clean of [false,true])writeFileSync(`${dir}/hypothesis-${i}${clean?'-clean':''}.svg`,renderPlacementSvg(input,ps,{ratsnest:!clean,ratsnestTopology:'mst',signalPaths:false}));
    const critical=report.hintViolations.filter(v=>v.hint.priority==='critical'||v.hint.hard);
    const worsened=critical.flatMap(v=>{const b=baselinePackingReport.hintViolations.find(b=>JSON.stringify(b.hint)===JSON.stringify(v.hint));
        return !b || (typeof v.actual==='number'&&typeof b.actual==='number'&&(v.expected.startsWith('>=')?v.actual<b.actual:v.actual>b.actual))
            ?[{hint:v.hint,before:b?.actual??'satisfied',after:v.actual,expected:v.expected}]:[];});
    return {index:i,alignment:h.alignment,angles:alignmentErrors(h.roots,policy.pairs).map(p=>p.orientationError),metrics:placementMetrics(input,ps),worsened,criticalCount:critical.length};
});
const changed=results[1].placements.filter(p=>['x','y','rotate','layer'].some(k=>p[k]!==results[0].placements.find(q=>q.designator===p.designator)[k])).length;
const validation={validChange:addon.validatePlacementChange(encodeNativePostPlaceRefineProblem(input,results[0].placements,1),results[1].placements),
    hardHintsNoWorse:alignmentHardHintsNoWorse(createPlacementReport(input,results[0].placements),createPlacementReport(input,results[1].placements))};
const output={baseline:source,inputSha256:createHash('sha256').update(inputRaw).digest('hex'),nativeSha256:createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex'),
    policy,ms:results[1].ms,results:summary,diagnostics,validation,changed,probe,hypothesisSummary};
writeFileSync(`${dir}/summary.json`,JSON.stringify(output,null,2));
const pair=scope=>`<div class="pair">${['before','after'].map(tag=>`<figure><figcaption>${tag==='before'?'До':'После'}</figcaption><a href="${tag}-${scope}.svg"><img class="${scope}" data-tag="${tag}" src="${tag}-${scope}.svg"></a></figure>`).join('')}</div>`;
const row=(label,values)=>`<tr><td>${label}</td>${values.map(v=>`<td>${esc(typeof v==='number'?n(v):v)}</td>`).join('')}</tr>`;
writeFileSync(`${dir}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Telemetry — выравнивание в основном поиске</title><style>body{font:16px system-ui;background:#f1f5f9;color:#172033;max-width:1600px;margin:24px auto;padding:0 24px}p{max-width:1150px;line-height:1.5}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;background:white;border:1px solid #cbd5e1;border-radius:10px;padding:12px}img{width:100%;max-height:850px;object-fit:contain}figcaption{font-weight:bold}table{border-collapse:collapse;width:100%;background:white;margin:20px 0}td,th{padding:10px;border-bottom:1px solid #ddd;text-align:left}.notice{background:#fff7ed;border-left:4px solid #ea580c;padding:14px}a{color:#0369a1}</style>
<h1>Telemetry: ${orientationRun?"согласование поворотов основных микросхем":"мягкое выравнивание в основном поиске"}</h1>
<p>«До» — последняя полная плата с фиксированным USB и мягкими зазорами. «После» — повторный board packaging с теми же сохранёнными локальными блоками и альтернативами, затем обычный postrefine и завершающее выравнивание. Локальная сборка блоков не повторялась.</p>
<p><strong>Изменено компонентов в принятом результате: ${changed}. ${changed?"Смотрите сравнение ниже.":"Отобрана исходная гипотеза; итоговые изображения совпадают. Это отрицательный результат текущей настройки, а не подтверждение улучшения."}</strong></p>
<p>${orientationRun?"Новая часть эксперимента — бонус за одинаковое направление основных микросхем. Максимум 120 единиц на пару с поправкой на похожесть и расстояние. Направление сравнивается по нумерованным площадкам, с учётом исходного поворота футпринта. Центр позиции и компонент направления могут различаться. Итоговый postrefine по-прежнему имеет собственную оценку; таблица показывает состояние до и после него.":""}</p>
<p>Похожие пары определяются до размещения, порог 0,78. В native score добавлен ограниченный бонус за общую ось центров основных компонентов, с допуском 0,15 мм. Бонус плавно исчезает при зазоре между блоками от 8 до 16 мм. Генерируются позиции на общих осях с несколькими промежутками и проекции свободных мест. Прежние кандидаты остаются. Проверяются пять полных гипотез: обычная, с выравниванием, плотная и две альтернативные сборки с выравниванием. Совместная атомарная постановка пары пока не добавлена.</p>
<p>Время board packaging и последующих этапов: ${n(results[1].ms/1000)} с. Фиксированных компонентов сдвинуто: ${summary[1].fixedChanges.length}. Это сравнение размещения; трассировка не запускалась.</p>
<p class="notice">Проверка изменения native: ${validation.validChange}; обязательные ограничения не ухудшены: ${validation.hardHintsNoWorse}. Строгая абсолютная проверка: ${summary.map(s=>s.strictValid).join(' → ')}. Число нарушений обязательного зазора: ${summary.map(s=>s.criticalClearances.length).join(' → ')}. Исходная плата уже содержала нарушения; файлы ASM — для просмотра, не подтверждение готовности к изготовлению.</p>
<table><tr><th>Метрика</th><th>До</th><th>После</th></tr>${[['Длина MST, мм','wireLength'],['Пересечения линий','crossings'],['Пересечения чужих падов','foreignPadHits']].map(([l,k])=>row(l,summary.map(s=>s.metrics[k]))).join('')}${row('Бонус выравнивания после упаковки',summary.map(s=>s.packedAlignmentScore))}${row('Бонус выравнивания в конце',summary.map(s=>s.alignmentScore))}</table>
<h2>Оси похожих блоков</h2><p>Меньшая разность X или Y центров, мм. Малое значение означает общую строку или колонку; это не расстояние между блоками.</p><table><tr><th>Центры</th><th>После упаковки: до / после</th><th>Итог: до / после</th></tr>${policy.pairs.map((p,i)=>row(`${p.anchorA??p.a} / ${p.anchorB??p.b}`,[summary.map(s=>n(s.packedAlignment[i].error)).join(' / '),summary.map(s=>n(s.alignment[i].error)).join(' / ')])).join('')}</table>
${orientationRun?`<h2>Направление основных компонентов</h2><table><tr><th>Компоненты</th><th>Разница после упаковки: до / после, °</th><th>Разница в итоге: до / после, °</th></tr>${policy.pairs.map((p,i)=>row(p.orientation?`${p.orientation.a} / ${p.orientation.b}`:'не сопоставлены',[summary.map(s=>s.packedAlignment[i].orientationError??'—').join(' / '),summary.map(s=>s.alignment[i].orientationError??'—').join(' / ')])).join('')}</table>`:''}
<h2>Полная плата</h2><label><input id="clean" type="checkbox"> Скрыть линии связей</label>${pair('board')}
${crops.map(([name,refs])=>`<h2>${refs.join(' / ')}</h2>${pair(name)}`).join('')}
${probe?`<h2>Отдельная проверка: что предложила новая упаковка</h2><p>Две первичные гипотезы до выбора альтернатив и postrefine. Слева обычная упаковка, справа — с новой метрикой и кандидатами. Это предложенный вариант, не принятый итог. Отбор: ${esc(probe.selected)}. Обязательные ограничения не ухудшены: ${probe.hardHintsNoWorse}.</p><table><tr><th>Метрика</th><th>Обычная</th><th>С выравниванием</th></tr>${row('Бонус выравнивания и направления',probe.cases.map(c=>c.alignmentScore))}${orientationRun?policy.pairs.map((p,i)=>row(p.orientation?`Угол ${p.orientation.a}/${p.orientation.b}, °`:"Нет сравнимого направления",probe.cases.map(c=>c.alignment[i].orientationError??"—"))).join(''):''}${['wireLength','crossings','foreignPadHits'].map(k=>row(k,probe.cases.map(c=>c.metrics[k]))).join('')}</table><div class="pair">${probe.cases.map(c=>`<figure><figcaption>${c.tag}</figcaption><a href="probe-${c.tag}.svg"><img src="probe-${c.tag}-clean.svg"></a></figure>`).join('')}</div><pre>${esc(probe.diagnostics.map(d=>d.message).join('\n'))}</pre>`:''}
${orientationRun?`<h2>Все гипотезы до итогового отбора</h2><p>Углы в порядке ${policy.pairs.map(p=>p.orientation?`${p.orientation.a}/${p.orientation.b}`:'нет сопоставления').join(', ')}. 0 — обычная упаковка; 1 — с выравниванием и направлением; 2 — плотная; 3–4 — альтернативные локальные сборки.</p><table><tr><th>Гипотеза</th><th>Разницы направления, °</th><th>MST, мм</th><th>Пересечения линий / падов</th></tr>${hypothesisSummary.map(h=>row(String(h.index),[h.angles.map(a=>a??'—').join(' / '),h.metrics.wireLength,`${h.metrics.crossings} / ${h.metrics.foreignPadHits}`])).join('')}</table><p>${hypothesisSummary.map(h=>`<a href="hypothesis-${h.index}.svg">Плата ${h.index}</a>`).join(' · ')}</p>
${hypothesisSummary.filter(h=>h.index===3).map(h=>`<h2>Отклонённая гипотеза 3: согласованные U16/U20 и U13/U17</h2><p>Это предложенная упаковка до postrefine, не принятый результат. Ухудшенные обязательные ограничения показаны ниже.</p><div class="pair"><figure><figcaption>Обычная упаковка 0</figcaption><img src="hypothesis-0-clean.svg"></figure><figure><figcaption>Альтернативная упаковка 3 — отклонена</figcaption><img src="hypothesis-3-clean.svg"></figure></div><table><tr><th>Правило</th><th>До</th><th>В гипотезе 3</th><th>Требование</th></tr>${h.worsened.map(v=>row(`${v.hint.source.block_name??v.hint.source.designator} → ${v.hint.target?.block_name??v.hint.target?.designator??'all'}`,[v.before,v.after,v.expected])).join('')}</table>`).join('')}`:''}
<details><summary>Диагностика выбора</summary><pre>${esc(JSON.stringify(diagnostics,null,2))}</pre></details>
<p><a href="after.assemble.json">ASM после</a> · <a href="before.assemble.json">ASM до</a> · <a href="summary.json">Численные результаты</a>. Фиксированная механика, контур, отверстия и медь исключены из ASM. В EasyEDA ничего не применялось.</p>
<script>document.querySelector('#clean').onchange=e=>document.querySelectorAll('img.board').forEach(img=>img.src=img.dataset.tag+'-board'+(e.target.checked?'-clean':'')+'.svg');</script></html>`);
console.log(JSON.stringify({results:summary,validation,diagnostics},null,2));
