import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {gunzipSync,gzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {renderPlacementSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {componentBox,unionBoxes,boxGap,boardOutlinePolygon,getPadWorld} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {createFixedPlacement} from '../src/pcb-layout/pcb-auto-place/fixed.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
import {boardSpacingPolicy} from '../src/pcb-layout/pcb-auto-place-v2/board-spacing.ts';
const dir='docs/experiments/telemetry-anchored-2026-09-27';
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const names=new Set(['J5','R30','R31','F1','C41']);
const load=name=>JSON.parse(existsSync(`${dir}/${name}.json`)?readFileSync(`${dir}/${name}.json`):gunzipSync(readFileSync(`${dir}/${name}.json.gz`)));
const results=['before','after'].map(tag=>({tag,...load(tag==='after'&&(existsSync(`${dir}/after-final.json`)||existsSync(`${dir}/after-final.json.gz`))?'after-final':tag)}));
for(const r of results){
    const file=r.tag==='after'&&existsSync(`${dir}/after-final.assemble.json`)?'after-final':r.tag;
    const asm=JSON.parse(readFileSync(`${dir}/${file}.assemble.json`));
    // This comparison updates placement on the existing PCB; do not duplicate
    // mounting holes, generated copper or the already-present board outline.
    writeFileSync(`${dir}/${r.tag}.assemble.json`,JSON.stringify({components:asm.components},null,2));
}
const usbBounds=unionBoxes(results.flatMap(r=>r.placements.filter(p=>names.has(p.designator)).map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p))));
for(const side of ['left','top'])usbBounds[side]-=3;
for(const side of ['right','bottom'])usbBounds[side]+=3;
function usbSvg(ps,all=false){
    const outline=boardOutlinePolygon(input.board).map(p=>`${(p.x-usbBounds.left)*12},${(p.y-usbBounds.top)*12}`).join(' ');
    return renderPlacementSvg(input,ps,{bounds:usbBounds,ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:all})
        .replace('</svg>',`<polygon points="${outline}" fill="none" stroke="#14532d" stroke-width="3"/></svg>`);
}
function gaps(placements){
    const blocks=input.blocks.map(b=>({name:b.name,poses:placements.filter(p=>b.component_designators.includes(p.designator))}))
        .filter(b=>b.poses.length).map(b=>({...b,box:unionBoxes(b.poses.map(p=>componentBox(input.components.find(c=>c.designator===p.designator),p))),
            movable:b.poses.some(p=>!input.components.find(c=>c.designator===p.designator).pcb.fixedPlacement)}));
    const nearest=blocks.filter(b=>b.movable).map(b=>Math.min(...blocks.filter(o=>o!==b).map(o=>boxGap(b.box,o.box)))).sort((a,b)=>a-b);
    return {median:nearest[Math.floor(nearest.length/2)],below1:nearest.filter(d=>d<1).length,blocks:nearest.length};
}
function links(placements){
    const j=input.components.find(c=>c.designator==='J5'),jp=placements.find(p=>p.designator==='J5');
    return placements.filter(p=>names.has(p.designator)&&p.designator!=='J5').flatMap(p=>{
        const c=input.components.find(c=>c.designator===p.designator);
        return c.pins.filter(pin=>pin.signal_name&&pin.signal_name!=='GND').flatMap(pin=>{
            const ends=j.pins.filter(q=>q.signal_name===pin.signal_name).map(q=>getPadWorld(j,jp,q.pin_number)).filter(Boolean);
            const start=getPadWorld(c,p,pin.pin_number);
            return start&&ends.length?[{pin:`${p.designator}.${pin.pin_number}`,net:pin.signal_name,mm:Math.min(...ends.map(q=>Math.hypot(start.x-q.x,start.y-q.y)))}]:[];
        });
    });
}
const summary=results.map(r=>{
    const ps=r.placements.filter(p=>names.has(p.designator));
    for(const all of [false,true])writeFileSync(`${dir}/${r.tag}-usb${all?'-all':''}.svg`,usbSvg(ps,all));
    writeFileSync(`${dir}/${r.tag}-board.svg`,renderPlacementSvg(input,r.placements,{ratsnestTopology:'mst',signalPaths:false}));
    writeFileSync(`${dir}/${r.tag}-board-clean.svg`,renderPlacementSvg(input,r.placements,{ratsnest:false,signalPaths:false}));
    const fixedChanges=input.components.flatMap(c=>{const f=createFixedPlacement(input,c),p=r.placements.find(p=>p.designator===c.designator);
        return f&&(!p||['x','y','rotate','layer'].some(k=>p[k]!==f[k]))?[c.designator]:[];});
    return {tag:r.tag,ok:r.report.ok,count:r.placements.length,fixedChanges,
        criticalClearanceViolations:r.report.hintViolations.filter(v=>v.hint.relation==='clearance'&&v.hint.priority==='critical'),
        board:placementMetrics(input,r.placements),usb:placementMetrics(input,ps),gaps:gaps(r.placements),links:links(r.placements)};
});
const tree=results[1].stages.find(s=>s.name==='01-v2-tree').data;
const family=tree.primitives.find(p=>p.label==='mechanic_J5'&&p.anchored);
const alternatives=family?[family,...family.layoutAlternatives??[]]:[];
const control=existsSync(`${dir}/usb-control.json`)?JSON.parse(readFileSync(`${dir}/usb-control.json`)):null;
alternatives.forEach((p,i)=>writeFileSync(`${dir}/usb-variant-${i+1}.svg`,usbSvg(p.placements)));
writeFileSync(`${dir}/summary.json`,JSON.stringify({spacing:boardSpacingPolicy(input),results:summary,usbVariants:alternatives.length,
    control:control?{ok:control.report.ok,validChange:control.validChange,metrics:control.metrics}:undefined,
    inputSha256:createHash('sha256').update(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json')).digest('hex'),
    nativeSha256:createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex')},null,2));
for(const name of ['before','after','after-final'])if(existsSync(`${dir}/${name}.json`))
    writeFileSync(`${dir}/${name}.json.gz`,gzipSync(readFileSync(`${dir}/${name}.json`)));
const n=v=>typeof v==='number'?v.toFixed(2):v;
const row=(title,values)=>`<tr><td>${title}</td>${values.map(v=>`<td>${n(v)}</td>`).join('')}</tr>`;
const table=(scope)=>`<table><tr><th>${scope==='usb'?'USB: J5 + usb_input':'Вся Telemetry'}</th><th>До</th><th>После</th></tr>${[
    ['Длина MST, мм','wireLength'],['Пересечения линий','crossings'],['Пересечения чужих падов','foreignPadHits']
].map(([label,key])=>row(label,summary.map(r=>r[scope][key]))).join('')}</table>`;
const pair=(scope,extra='')=>`<div class="pair">${results.map(r=>`<figure><figcaption>${r.tag==='before'?'До':'После'}</figcaption><a href="${r.tag}-${scope}.svg" target="_blank"><img class="${scope}" data-tag="${r.tag}" src="${r.tag}-${scope}.svg" alt="${scope}"></a></figure>`).join('')}</div>${extra}`;
writeFileSync(`${dir}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Telemetry — фиксированный USB и мягкие зазоры</title><style>
body{font:16px system-ui;background:#f1f5f9;color:#172033;margin:24px auto;max-width:1500px;padding:0 24px}h1{font-size:28px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;background:white;border:1px solid #cbd5e1;border-radius:10px;padding:12px}img{width:100%;max-height:820px;object-fit:contain}figcaption{font-weight:700;margin:4px}table{border-collapse:collapse;margin:18px 0;width:100%;background:white}td,th{padding:10px;text-align:left;border-bottom:1px solid #dbe3ec}p{max-width:1050px;line-height:1.5}a{color:#0369a1}.variants{display:flex;gap:12px}.variants img{max-height:300px}button,label{margin:10px;padding:8px}code{background:#e2e8f0;padding:2px 5px}</style>
<h1>Telemetry: обвязка фиксированного USB и мягкие зазоры</h1>
<p>Одинаковая схема, контур и фиксированная механика. «До» — состояние ветки b747758. «После» — сборка обвязки в реальном окружении J5 и ограниченный мягкий отступ между блоками. Зелёная линия в USB — настоящий край платы. Нажмите изображение для открытия SVG.</p>
<p style="background:#fff7ed;padding:14px;border-left:4px solid #ea580c"><strong>Результат смешанный.</strong> В USB уменьшилось число пересечений чужих площадок, но выросло число пересечений линий. Полная перепаковка ухудшила электрические метрики платы. Проверены четыре гипотезы упаковки: с отступами, плотная и два набора альтернативных блоков; выбран первый вариант. Это экспериментальный результат, а не подтверждение общего улучшения платы.</p>
<p style="background:#fff1f2;padding:14px;border-left:4px solid #e11d48"><strong>Полная плата не прошла строгую проверку всех ограничений.</strong> И «До», и «После» имеют по семь нарушений заданного межгруппового зазора 3,2 мм. Обычный report.ok не считает эти замечания причиной отказа; абсолютная native-проверка их отклоняет. Перекрытий компонентов и перемещений фиксированной механики нет. ASM — для просмотра и сравнения, не готовый проверенный результат для изготовления.</p>
<p>Проверка отчёта: ${summary.map(r=>`${r.tag}: ${r.ok?'OK':'есть замечания'}, ${r.count} компонентов, сдвинуто фиксированных: ${r.fixedChanges.length}`).join('; ')}. Метрики прямых линий MST служат для сравнения размещения; это не результат трассировки.</p>
${table('usb')}<label><input type="checkbox" id="ignored">Показать также игнорируемые сети USB</label>${pair('usb')}
<h2>Полная плата</h2>${table('board')}
<p>Медианный ближайший зазор между исходными DSL-блоками: ${summary.map(r=>`${r.tag} ${n(r.gaps.median)} мм`).join(' → ')}. Блоков с соседом ближе 1 мм: ${summary.map(r=>`${r.tag} ${r.gaps.below1}/${r.gaps.blocks}`).join(' → ')}. Это описательная метрика: тесное соседство сильно связанных блоков допустимо.</p>
<label><input type="checkbox" id="clean">Скрыть линии связей полной платы</label>${pair('board')}
<h2>Сохранённые локальные варианты USB</h2><div class="variants">${alternatives.map((_,i)=>`<figure><figcaption>Вариант ${i+1}</figcaption><img src="usb-variant-${i+1}.svg"></figure>`).join('')}</div>
<p>Все варианты сохраняют положение J5. На плате выбирается допустимая компоновка обвязки, без перемещения якоря.</p>
${control?.report.ok&&control.validChange?`<h2>Контроль: заменить только USB на исходной плате</h2><p>Четыре компонента R30, R31, F1 и C41 взяты из новой сборки; все остальные позиции — из «До». Это отдельный контрольный эксперимент, не результат board packaging. Геометрия и ограничения проверены. Длина MST ${control.metrics.wireLength} мм, пересечений линий ${control.metrics.crossings}, чужих площадок ${control.metrics.foreignPadHits}.</p><a href="usb-control-board.svg"><img src="usb-control-board.svg" alt="Контрольная плата с заменой только USB"></a><p><a href="usb-control.assemble.json">ASM только четырёх компонентов USB</a>. Метрики контроля воспроизводятся при применении к исходной компоновке «До».</p>`:''}
<p><a href="after.assemble.json">ASM JSON после</a> · <a href="before.assemble.json">ASM JSON до</a> · <a href="summary.json">Численные результаты</a>. Контур и фиксированные компоненты исключены из команды перемещения; файл пока не применён в EasyEDA.</p>
<script>document.querySelector('#ignored').onchange=e=>document.querySelectorAll('img.usb').forEach(img=>img.src=img.dataset.tag+'-usb'+(e.target.checked?'-all':'')+'.svg');document.querySelector('#clean').onchange=e=>document.querySelectorAll('img.board').forEach(img=>img.src=img.dataset.tag+'-board'+(e.target.checked?'-clean':'')+'.svg');</script></html>`);
console.log(JSON.stringify(summary,null,2));
