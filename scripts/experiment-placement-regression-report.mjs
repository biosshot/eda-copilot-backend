import {readFileSync,writeFileSync,existsSync,mkdirSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {renderPlacementSvg,renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {canonicalModuleDesignators} from '../src/pcb-layout/pcb-auto-place/report-helpers.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {boardElectricalQuality,boardElectricalRegression,alignmentHardHintsNoWorse} from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
import {minimumSpanningEdges} from '../src/pcb-layout/pcb-auto-place/ratsnest.ts';
import {encodeNativePostPlaceScoreProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-score.ts';

const out=resolve('docs/experimental/pcb/placement-regression-2026-09-28');
const bank=resolve('docs/experimental/pcb/global-placement-2026-09-27');
const measured=JSON.parse(readFileSync(`${bank}/measurements.json`));
const manifest=JSON.parse(readFileSync(`${bank}/manifest.json`));
const boards=measured.filter(b=>b.entities?.length&&!b.duplicateOf);
const skipped=manifest.filter(b=>!b.input).map(b=>({name:b.name,reason:b.error}));
const esc=s=>String(s??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
const n=x=>Number(x).toFixed(2);
const duration=ms=>ms>=60_000?`${Math.floor(ms/60_000)} мин ${n(ms%60_000/1000)} с`:`${n(ms/1000)} с`;
const sha=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
// These saved results predate later performance work. Do not relabel them with
// the current checkout/binary when only rebuilding their SVG/HTML presentation.
const versions={before:'b8fe403',after:'8311f7b'};
const binaries={before:null,after:null}; // The original runner did not persist binary hashes.
const grid=[];
function note(html){return `<p class="note">${html}</p>`;}
function summaryRow(label,a,b){return `<tr><td>${esc(label)}</td><td>${a==null?'—':esc(a)}</td><td>${b==null?'—':esc(b)}</td></tr>`;}
function figures(paths,names=['До','После']){return `<div class="figures">${paths.map((path,i)=>`<figure><figcaption>${names[i]}</figcaption><a href="${esc(path)}"><img loading="lazy" src="${esc(path)}"></a></figure>`).join('')}</div>`;}
function metricsTable(a,b){return `<table><tr><th>Показатель</th><th>До</th><th>После</th></tr>${[
    ['Длина MST, мм','wireLength'],['Пересечения линий','crossings'],['Пересечения чужих падов','foreignPadHits'],
    ['HPWL, мм','hpwl'],['Площадь охвата, мм²','area']].map(([label,key])=>summaryRow(label,a?.[key],b?.[key])).join('')}</table>`;}
for(const board of boards){
    const name=board.name,dir=`${out}/${name}`;
    const raw=readFileSync(`${bank}/${name}/input.json`),input=JSON.parse(raw);
    const statuses={},results={},summaries={};
    for(const tag of ['before','after']){
        const base=`${dir}/${tag}`;
        if(!existsSync(`${base}/result.json.gz`)||!existsSync(`${base}/summary.json`)){
            statuses[tag]='Не завершено';continue;
        }
        try{
            results[tag]=JSON.parse(gunzipSync(readFileSync(`${base}/result.json.gz`)));
            summaries[tag]=JSON.parse(readFileSync(`${base}/summary.json`));
            if(summaries[tag].inputSha256!==sha(`${bank}/${name}/input.json`))throw Error('Вход отличается от сохранённого');
            statuses[tag]=summaries[tag].reportOk?'OK':'С нарушениями';
        }catch(e){statuses[tag]=`Ошибка чтения: ${e.message}`;}
    }
    if(!results.before||!results.after){grid.push({name,statuses,components:input.components.length,error:'Один из прогонов не завершён'});continue;}
    const a=results.before,b=results.after,sa=summaries.before,sb=summaries.after;
    const pa=createPlacementReport(input,a.placements),pb=createPlacementReport(input,b.placements);
    const badHint=!alignmentHardHintsNoWorse(pa,pb);
    const wiring=boardElectricalRegression(boardElectricalQuality(input,a.placements),boardElectricalQuality(input,b.placements))??null;
    const q={inventory:sb.inventoryOk,fixedChanges:sb.fixedChanges,hintRegression:badHint,
        declaredPaths:input.paths?.length??0,declaredRegions:input.constraintRegions?.length??0,
        pathsBefore:pa.signalPaths,pathsAfter:pb.signalPaths,
        reportBefore:sa.reportOk,reportAfter:sb.reportOk,wiringRegression:wiring,
        violationsBefore:sa.violations,violationsAfter:sb.violations};
    const netLengths=ps=>new Map(encodeNativePostPlaceScoreProblem(input,ps).nets.map(net=>
        [net.name,minimumSpanningEdges(net.points).reduce((s,[i,j])=>s+Math.hypot(net.points[i].x-net.points[j].x,net.points[i].y-net.points[j].y),0)]));
    const baseNets=netLengths(a.placements),newNets=netLengths(b.placements);
    q.longerNets=[...baseNets].flatMap(([net,length])=>{
        const current=newNets.get(net);return current!==undefined&&current>length+Math.max(.5,.02*length)
            ?[{net,before:length,after:current,delta:current-length}]:[];
    }).sort((a,b)=>b.delta-a.delta);
    const boardFiles=[];
    for(const tag of ['before','after']){
        const p=results[tag].placements,rel=`${name}/${tag}`;
        saveText(`${out}/${rel}/board.svg`,renderPlacementSvg(input,p,{ratsnest:true,ratsnestTopology:'mst',signalPaths:true}));
        boardFiles.push(`${rel}/board.svg`);
    }
    const entities=[...input.blocks.filter(b=>b.component_designators?.length).map((x,i)=>({kind:'block',name:x.name,refs:x.component_designators,id:`block-${i}`})),
        ...(input.modules??[]).map((x,i)=>({kind:'module',name:x.name,refs:[...canonicalModuleDesignators(input,x)],id:`module-${i}`}))];
    const entries=[];
    for(const e of entities){
        const names=new Set(e.refs),ps=[a,b].map(r=>r.placements.filter(p=>names.has(p.designator)));
        const paths=[];
        for(const [i,tag]of ['before','after'].entries()){
            const path=`${name}/${tag}/${e.id}.svg`;
            saveText(`${out}/${path}`,renderPlacementSubsetSvg(input,ps[i],{padding:2,ratsnest:true,ratsnestTopology:'mst',signalPaths:true}));
            paths.push(path);
        }
        const local=[];
        if(e.kind==='block')for(const [i,tag]of ['before','after'].entries()){
            const p=results[tag].localBlocks.find(p=>p.label===e.name)?.placements;
            if(p?.length){const path=`${name}/${tag}/${e.id}-local.svg`;
                saveText(`${out}/${path}`,renderPlacementSubsetSvg(input,p,{padding:2,ratsnest:true,ratsnestTopology:'mst',signalPaths:true}));
                local[i]=path;}
        }
        entries.push({...e,paths,local,metrics:ps.map(p=>placementMetrics(input,p))});
    }
    grid.push({name,components:input.components.length,blocks:entries.filter(e=>e.kind==='block').length,
        modules:entries.filter(e=>e.kind==='module').length,statuses,summaryBefore:sa,summaryAfter:sb,
        boardFiles,entries,validation:q,changed:b.placements.filter(p=>['x','y','rotate','layer'].some(k=>p[k]!==a.placements.find(t=>t.designator===p.designator)?.[k])).length});
}
const regression={versions,binaries,boards:grid,skipped,
    completedTimingMs:{before:grid.reduce((s,b)=>s+(b.summaryBefore&&b.summaryAfter?b.summaryBefore.ms:0),0),
        after:grid.reduce((s,b)=>s+(b.summaryBefore&&b.summaryAfter?b.summaryAfter.ms:0),0)}};
writeFileSync(`${out}/summary.json`,JSON.stringify(regression,null,2));
const complete=grid.filter(b=>b.summaryAfter),invalid=complete.filter(b=>!b.summaryAfter.reportOk),
    newlyInvalid=invalid.filter(b=>b.summaryBefore.reportOk),
    missing=grid.filter(b=>b.error),changed=complete.filter(b=>b.changed),
    loss=complete.filter(b=>b.validation.hintRegression||b.validation.wiringRegression||b.summaryAfter.metrics.wireLength>b.summaryBefore.metrics.wireLength+.01
        ||b.summaryAfter.metrics.crossings>b.summaryBefore.metrics.crossings||b.summaryAfter.metrics.foreignPadHits>b.summaryBefore.metrics.foreignPadHits);
const totalMs={before:complete.reduce((s,b)=>s+b.summaryBefore.ms,0),after:complete.reduce((s,b)=>s+b.summaryAfter.ms,0)};
const resultTable=`<table><tr><th>Плата</th><th>Компонентов</th><th>Блоков</th><th>Изменено</th><th>Время до / после</th><th>Длина MST, мм</th><th>Линии</th><th>Чужие пады</th><th>Проверка</th></tr>${grid.map(b=>{
    if(b.error)return `<tr><td>${esc(b.name)}</td><td>${b.components}</td><td colspan="7">${esc(b.error)}</td></tr>`;
    const before=b.summaryBefore,after=b.summaryAfter;
    return `<tr><td><a href="#${esc(b.name)}">${esc(b.name)}</a></td><td>${b.components}</td><td>${b.blocks}</td><td>${b.changed}</td>
        <td>${duration(before.ms)} / ${duration(after.ms)}</td><td>${n(before.metrics.wireLength)} → ${n(after.metrics.wireLength)}</td><td>${before.metrics.crossings} → ${after.metrics.crossings}</td>
        <td>${before.metrics.foreignPadHits} → ${after.metrics.foreignPadHits}</td><td>${after.reportOk?'OK':before.reportOk?'Новые нарушения':'Исходные нарушения'}${b.validation.hintRegression?' · зазоры хуже':''}${b.validation.wiringRegression?' · сети длиннее':''}</td></tr>`;}).join('')}</table>`;
const cards=grid.map(b=>{
    if(b.error)return `<section id="${esc(b.name)}"><h2>${esc(b.name)}</h2>${note(esc(b.error))}</section>`;
    const qa=b.validation,entities=b.entries,blockCount=entities.filter(e=>e.kind==='block').length;
    const moduleCount=entities.length-blockCount;
    const totalRegressions=[qa.hintRegression?'обязательные зазоры':'',qa.wiringRegression?'длина отдельных сетей':'',
        b.summaryAfter.metrics.wireLength>b.summaryBefore.metrics.wireLength+.01?'общая длина':'',
        b.summaryAfter.metrics.crossings>b.summaryBefore.metrics.crossings?'пересечения линий':'',
        b.summaryAfter.metrics.foreignPadHits>b.summaryBefore.metrics.foreignPadHits?'пересечения чужих падов':''].filter(Boolean);
    const item=e=>`<details class="entity"><summary>${esc(e.kind==='block'?'Блок':'Модуль')}: ${esc(e.name)} (${e.refs.length}) · MST ${n(e.metrics[0].wireLength)} → ${n(e.metrics[1].wireLength)} мм · пады ${e.metrics[0].foreignPadHits} → ${e.metrics[1].foreignPadHits}</summary>
        ${metricsTable(...e.metrics)}${figures(e.paths)}${e.local.length===2?`<details><summary>Сразу после сборки блока</summary>${figures(e.local)}</details>`:''}</details>`;
    return `<section id="${esc(b.name)}"><h2>${esc(b.name)}</h2><p>${b.components} компонентов, ${blockCount} блоков, ${moduleCount} модулей; изменено ${b.changed}. Время: ${duration(b.summaryBefore.ms)} до, ${duration(b.summaryAfter.ms)} после. ${totalRegressions.length?`Ухудшения: ${esc(totalRegressions.join(', '))}.`:'По отмеченным метрикам ухудшений нет.'}</p>
        ${note(`Инвентарь: ${qa.inventory}; фиксированные позиции изменены: ${qa.fixedChanges.length}; отчёт размещения: ${qa.reportBefore} → ${qa.reportAfter}; обязательные зазоры ухудшены: ${qa.hintRegression}; проверка отдельных сетей: ${qa.wiringRegression??'без регрессии'}.`)}
        ${qa.longerNets.length?`<details><summary>Сети, превысившие допуск удлинения (${qa.longerNets.length})</summary><table><tr><th>Сеть</th><th>До, мм</th><th>После, мм</th><th>Изменение, мм</th></tr>${qa.longerNets.map(x=>`<tr><td>${esc(x.net)}</td><td>${n(x.before)}</td><td>${n(x.after)}</td><td>+${n(x.delta)}</td></tr>`).join('')}</table></details>`:''}
        ${metricsTable(b.summaryBefore.metrics,b.summaryAfter.metrics)}
        <p>Во входе: signal paths — ${qa.declaredPaths}, constraint regions — ${qa.declaredRegions}. Цветные линии показывают заданные signal paths; красный пунктир — обычные связи. Это ориентиры размещения, не дорожки.</p>
        ${qa.declaredPaths?`<details><summary>Выполнение ограничений signal paths</summary><table><tr><th>Путь</th><th>До</th><th>После</th><th>Сегменты после, мм / предел</th></tr>${qa.pathsAfter.map(p=>`<tr><td>${esc(p.id)}</td><td>${qa.pathsBefore.find(a=>a.id===p.id)?.withinConstraints?'В пределах':'Есть превышения'}</td><td>${p.withinConstraints?'В пределах':'Есть превышения'}</td><td>${p.segments.map(s=>`${esc(s.source)} → ${esc(s.target)}: ${s.distance??'—'} / ${s.maxDistance??'не задан'}`).join('<br>')}</td></tr>`).join('')}</table></details>`:''}
        <h3>Полная плата</h3>${figures(b.boardFiles)}
        <p><a href="${esc(b.name)}/before/assembly.json">Assembly до</a> · <a href="${esc(b.name)}/after/assembly.json">Assembly после</a> · <a href="${esc(b.name)}/before/summary.json">Диагностика до</a> · <a href="${esc(b.name)}/after/summary.json">Диагностика после</a></p>
        <h3>Блоки и модули</h3>${entities.map(item).join('')}</section>`;
}).join('');
saveText(`${out}/comparison.html`,`<!doctype html><html lang="ru"><meta charset="utf-8"><title>Глобальная проверка размещения плат</title>
<style>body{font:16px system-ui;background:#f1f5f9;color:#172033;max-width:1700px;margin:24px auto;padding:0 24px}p{line-height:1.5}a{color:#0369a1}section{background:#fff;margin:26px 0;padding:18px;border:1px solid #cbd5e1;border-radius:12px}table{border-collapse:collapse;width:100%;margin:12px 0}td,th{border-bottom:1px solid #ddd;padding:9px;text-align:left}tr:hover{background:#f8fafc}.figures{display:grid;grid-template-columns:1fr 1fr;gap:15px;margin:14px 0}figure{margin:0;min-width:0;padding:10px;border:1px solid #cbd5e1;border-radius:8px}img{width:100%;max-height:850px;object-fit:contain}figcaption{font-weight:bold;margin-bottom:6px}.entity{border-top:1px solid #cbd5e1;padding:12px 0}.entity summary{cursor:pointer;font-weight:600}.note{padding:10px;background:#eef6ff;border-left:4px solid #0ea5e9}.bad{color:#a11616}</style>
<h1>Размещение PCB: сравнение всех сохранённых стендов</h1>
<p>Одинаковые сохранённые входы и геометрия. «До» — предыдущий коммит ${esc(versions.before)}; «после» — ${esc(versions.after)}. Каждый вариант рассчитан полным плейсером, включая сборку блоков, упаковку и финальную доработку. Результаты могут иметь собственные исходные нарушения; это отчёт о размещении, не трассировка.</p>
<p><strong>${complete.length}/${boards.length} полных пар запусков.</strong> Изменения на ${changed.length} платах; ${loss.length} плат имеют ухудшение хотя бы одного указанного показателя; ${newlyInvalid.length} плат с новыми нарушениями, ${invalid.length-newlyInvalid.length} с прежними нарушениями; ${missing.length} неполных пар. ${skipped.length} исходных примеров без пригодного сохранённого входа.</p>
<p>Сумма времени по завершённым платам: <strong>${duration(totalMs.before)} до → ${duration(totalMs.after)} после</strong>. Прогоны выполнялись на одной машине с одинаковыми лимитами потоков и частью времени параллельно; это оценка затрат, а не изолированный замер скорости.</p>
${resultTable}
<details><summary>Примеры без полного входа</summary><table><tr><th>Стенд</th><th>Причина</th></tr>${skipped.map(s=>`<tr><td>${esc(s.name)}</td><td>${esc(s.reason)}</td></tr>`).join('')}</table></details>
<p><a href="summary.json">Все численные результаты</a>. Assembly JSON каждого варианта доступен в разделе платы. Установка в EasyEDA и трассировка не выполнялись.</p>
${cards}</html>`);
console.log(JSON.stringify({boards:boards.length,complete:complete.length,changed:changed.length,withRegression:loss.length,invalid:invalid.length,missing:missing.length,skipped:skipped.length,output:`${out}/comparison.html`}));

function saveText(path,svg){writeFileSync(path,svg.replace(/[ \t]+$/gm,''));}
