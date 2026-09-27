import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { renderPlacementSubsetSvg, renderPlacementSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { canonicalModuleDesignators } from '../src/pcb-layout/pcb-auto-place/report-helpers.ts';
import { placementMetrics } from './experiment-placement-metrics.mjs';
const out = 'docs/experiments/global-placement-2026-09-27';
mkdirSync(out, { recursive: true });
const read = p => JSON.parse(readFileSync(p, 'utf8'));
const manifest = read('.test-output/global-placement/manifest.json');
const runs = read('.test-output/global-placement/runs.json');
const tags = ['before', 'refined', 'pads'];
const data = [];
const seenInputs = new Map();
function normalize(placements, before, input) {
    const anchor = [...input.components].filter(c => placements.some(p => p.designator === c.designator))
        .sort((a, b) => b.footprint.pads.length - a.footprint.pads.length || a.designator.localeCompare(b.designator))[0];
    const base = before.find(p => p.designator === anchor?.designator);
    const current = placements.find(p => p.designator === anchor?.designator);
    if (!base || !current) return placements;
    const angle = base.rotate - current.rotate, a = angle * Math.PI / 180;
    return placements.map(p => ({ ...p, x: (p.x - current.x) * Math.cos(a) - (p.y - current.y) * Math.sin(a),
        y: (p.x - current.x) * Math.sin(a) + (p.y - current.y) * Math.cos(a), rotate: ((p.rotate + angle) % 360 + 360) % 360 }));
}
for (const f of manifest) {
    if (!f.input) { data.push({ ...f, variants: {}, entities: [] }); continue; }
    const inputRaw = readFileSync(f.input), input = JSON.parse(inputRaw);
    // Preserve the resolved geometry so reruns do not depend on remote footprint changes.
    const dir = `${out}/${f.name}`; mkdirSync(dir, { recursive: true });
    writeFileSync(`${dir}/input.json`, inputRaw);
    const entities = [{ id: 'board', label: 'Полная плата', kind: 'board', names: input.components.map(c => c.designator) },
        ...input.blocks.filter(b => b.component_designators.length >= 2).map((b, i) => ({ id: `block-${i}`, label: `Блок: ${b.name}`, name: b.name, kind: 'block', names: b.component_designators })),
        ...(input.modules ?? []).map((m, i) => ({ id: `module-${i}`, label: `Модуль: ${m.name}`, kind: 'module', names: [...canonicalModuleDesignators(input, m)] }))];
    const semanticHash = createHash('sha256').update(JSON.stringify(input)).digest('hex');
    const duplicateOf = seenInputs.get(semanticHash);
    seenInputs.set(semanticHash, duplicateOf ?? f.name);
    const board = { ...f, components: input.components.length, blocks: input.blocks.length, inputHash: createHash('sha256').update(inputRaw).digest('hex'), semanticHash, duplicateOf, entities, variants: {} };
    const solutions = Object.fromEntries(tags.map(tag => {
        const root = `.test-output/architecture/${f.name}/global-${tag}`;
        if (!existsSync(`${root}/summary.json`)) return [tag, { error: existsSync(`${root}/run.log`) ? readFileSync(`${root}/run.log`, 'utf8').slice(-4000) : 'Запуск не завершён.', run: runs.find(r => r.fixture === f.name && r.tag === tag) }];
        const solution = read(`${root}/placement.json`);
        const summary = read(`${root}/summary.json`);
        const treeFile = readdirSync(`${root}/stages`).find(p => p.includes('v2-tree') && p.endsWith('.json'));
        return [tag, { ...solution, summary, tree: treeFile ? read(`${root}/stages/${treeFile}`).data : null }];
    }));
    for (const tag of tags) {
        const s = solutions[tag];
        if (s.error) { board.variants[tag] = s; continue; }
        const summary = s.summary;
        writeFileSync(`${dir}/${tag}-placement.json`, JSON.stringify({ placements: s.placements, report: s.report }));
        writeFileSync(`${dir}/${tag}-summary.json`, JSON.stringify(summary, null, 2));
        const actual = new Set(s.placements.map(p => p.designator)), expected = new Set(input.components.map(c => c.designator));
        const inventoryOk = actual.size === expected.size && s.placements.length === expected.size && [...actual].every(n => expected.has(n));
        const fixedChanges = input.components.filter(c => c.pcb.fixedPlacement).flatMap(c => {
            const p = s.placements.find(p => p.designator === c.designator), f = c.pcb.fixedPlacement;
            return !p || Math.abs(p.x - f.x) > .005 || Math.abs(p.y - f.y) > .005
                || (f.rotate != null && Math.abs(((p.rotate - f.rotate + 540) % 360) - 180) > .005)
                || (f.layer != null && p.layer !== f.layer) ? [c.designator] : [];
        });
        const variant = { ok: summary.ok, inventoryOk, fixedChanges, ms: summary.ms, nativeHash: summary.nativeHash,
            diagnostics: summary.diagnostics, errors: Object.fromEntries(['unplaced', 'outsideBoard', 'overlaps', 'boardHoleViolations', 'constraintRegionViolations', 'layerViolations', 'hintViolations'].map(key => [key, s.report[key]])), metrics: {}, files: {} };
        for (const e of entities) {
            const names = new Set(e.names), ps = s.placements.filter(p => names.has(p.designator));
            const before = (solutions.before.placements ?? s.placements).filter(p => names.has(p.designator));
            const file = `${f.name}/${tag}-${e.id}.svg`;
            const options = { ratsnestTopology: 'mst', signalPaths: false };
            writeFileSync(`${out}/${file}`, e.kind === 'board' ? renderPlacementSvg(input, ps, options)
                : renderPlacementSubsetSvg(input, normalize(ps, before, input), { ...options, padding: 2 }));
            variant.files[e.id] = file; variant.metrics[e.id] = placementMetrics(input, ps);
            if (e.kind === 'block') {
                const local = s.tree?.primitives?.find(p => p.kind === 'block' && p.label === e.name);
                const localBefore = solutions.before.tree?.primitives?.find(p => p.kind === 'block' && p.label === e.name)?.placements ?? local?.placements;
                if (local?.placements?.length) {
                    const localFile = `${f.name}/${tag}-${e.id}-local.svg`;
                    writeFileSync(`${out}/${localFile}`, renderPlacementSubsetSvg(input, normalize(local.placements, localBefore, input), { ...options, padding: 2 }));
                    variant.files[`${e.id}-local`] = localFile; variant.metrics[`${e.id}-local`] = placementMetrics(input, local.placements);
                }
            }
        }
        board.variants[tag] = variant;
    }
    data.push(board);
}
writeFileSync(`${out}/measurements.json`, JSON.stringify(data, null, 2));
writeFileSync(`${out}/manifest.json`, JSON.stringify(manifest.map(f => f.input ? { ...f, input: `${out}/${f.name}/input.json` } : f), null, 2));
writeFileSync(`${out}/runs.json`, JSON.stringify(runs, null, 2));
const unique = data.filter(b => b.entities.length && !b.duplicateOf);
const stats = { inputs: data.filter(b => b.entities.length).length, uniqueBoards: unique.length, runs: runs.length,
    blocks: unique.reduce((n,b) => n + b.entities.filter(e => e.kind === 'block').length, 0),
    modules: unique.reduce((n,b) => n + b.entities.filter(e => e.kind === 'module').length, 0),
    skipped: data.filter(b => b.error).map(b => ({ name: b.name, error: b.error })), comparisons: {} };
for (const base of ['before', 'refined']) {
    const compare = key => {
        const deltas = unique.flatMap(b => b.entities.filter(e => e.kind === key).flatMap(e => {
            const a = b.variants[base]?.metrics?.[e.id], z = b.variants.pads?.metrics?.[e.id];
            return a && z ? [{ fixture: b.name, entity: e.label, pad: z.foreignPadHits - a.foreignPadHits,
                line: z.crossings - a.crossings, length: +(z.wireLength - a.wireLength).toFixed(2) }] : [];
        }));
        const count = field => ({ improved: deltas.filter(d => d[field] < 0).length, unchanged: deltas.filter(d => d[field] === 0).length, worsened: deltas.filter(d => d[field] > 0).length });
        return { pad: count('pad'), line: count('line'), length: count('length'), deltas };
    };
    stats.comparisons[base] = { board: compare('board'), block: compare('block'), module: compare('module') };
}
stats.cleanBoards = Object.fromEntries(tags.map(tag => [tag, unique.filter(b => b.variants[tag]?.ok && b.variants[tag]?.inventoryOk && !b.variants[tag]?.fixedChanges.length).length]));
stats.inventoryPreserved = data.filter(b => b.entities.length).every(b => tags.every(tag => b.variants[tag]?.inventoryOk && !b.variants[tag]?.fixedChanges.length));
writeFileSync(`${out}/aggregate.json`, JSON.stringify(stats, null, 2));
const safeData = JSON.stringify(data).replace(/</g, '\\u003c');
writeFileSync(`${out}/comparison.html`, `<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Глобальное сравнение размещения PCB</title>
<style>body{font:16px system-ui;margin:24px;background:#f3f5f7;color:#172d40}h1{font-size:26px}header{position:sticky;top:0;background:#f3f5f7;padding:12px 0;z-index:2;border-bottom:1px solid #aaa}select,button{font:inherit;padding:8px;margin:4px;max-width:100%}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}figure{margin:0;background:white;padding:12px}img{width:100%;height:650px;object-fit:contain}figcaption{padding:12px 0;line-height:1.6}table{border-collapse:collapse;width:100%;font-size:14px}td,th{padding:8px;border:1px solid #ccc;text-align:left}tr[data-board]{cursor:pointer}tr[data-board]:hover{background:#dce8ff}.bad{color:#a21d25}.good{color:#147244}pre{white-space:pre-wrap;max-height:350px;overflow:auto}.gallery{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:12px}.gallery img{height:220px}.gallery button{background:white;border:1px solid #ccc;text-align:left}a{color:#245bb2}@media(max-width:850px){.pair{grid-template-columns:1fr}img{height:450px}body{margin:10px}}</style>
<h1>Глобальное сравнение блоков и полных плат</h1><p id="totals"></p><p>Три независимых запуска каждого входа: <b>до</b> последних изменений; <b>кандидаты + локальный postrefine</b>; тот же вариант <b>со штрафами за чужие пады</b>. Остальные настройки одинаковы: полный профиль, микророутер и портфель блоков включены. «До» уже содержит предыдущие улучшения ветки; это не исторический Legacy.</p><p>Линии — MST электрических связей, а не проложенные дорожки. Каждый чужой пад на каждом отрезке считается отдельно; исключены заданные игнорируемые цепи, учитываются слои. Для блоков оцениваются связи между показанными компонентами. Ориентация блоков выровнена вокруг компонента с наибольшим числом площадок; полные платы показаны без преобразований. Одиночные блоки видны на полной плате, отдельные карточки начинаются с двух компонентов. Время — один запуск при трёх параллельных процессах, не сравнительный бенчмарк скорости.</p>
<p><b>Отдельный вклад штрафов за пады</b> относительно кандидатов + postrefine: меньше пересечений с чужими падами на ${stats.comparisons.refined.board.pad.improved} платах, без изменений на ${stats.comparisons.refined.board.pad.unchanged}, больше на ${stats.comparisons.refined.board.pad.worsened}. Для блоков: ${stats.comparisons.refined.block.pad.improved} улучшились, ${stats.comparisons.refined.block.pad.unchanged} без изменений, ${stats.comparisons.refined.block.pad.worsened} ухудшились. Это не оценка победы по всем метрикам: длина и пересечения линий могут ухудшиться. STEPPER_CONTROLLER и ThunderF722 имеют ошибки во всех вариантах и включены в сравнение как проблемные примеры. Чистых итоговых плат: ${stats.cleanBoards.before} до, ${stats.cleanBoards.refined} с кандидатами/postrefine, ${stats.cleanBoards.pads} со штрафами за пады. Состав и заданные фиксированные позиции сохранены во всех запусках.</p>
<details><summary>Сводка полных плат — нажмите строку для просмотра</summary><div style="overflow:auto"><table id="overview"></table></div></details>
<header><select id="board"></select><select id="entity"></select><label><input id="local" type="checkbox"> Блок сразу после сборки</label><br><select id="left"></select><select id="right"></select></header>
<h2 id="title"></h2><div class="pair"><figure><a id="a0" target="_blank"><img id="img0"></a><figcaption id="caption0"></figcaption></figure><figure><a id="a1" target="_blank"><img id="img1"></a><figcaption id="caption1"></figcaption></figure></div>
<details><summary>Ошибки и диагностика выбранной платы</summary><pre id="diagnostics"></pre></details><h2>Все блоки и модули выбранной платы</h2><div class="gallery" id="gallery"></div>
<details><summary>Пропущенные исходные примеры и причины</summary><pre id="skips"></pre></details>
<script>
const data=${safeData};const tags=['before','refined','pads'],titles=['До последних изменений','Кандидаты + локальный postrefine','Со штрафами за чужие пады'];const el=id=>document.getElementById(id);const available=data.filter(b=>b.entities.length);let current;
const unique=available.filter(b=>!b.duplicateOf);el('totals').textContent=unique.length+' разных полных плат; '+unique.reduce((n,b)=>n+b.entities.filter(e=>e.kind==='block').length,0)+' блоков; '+unique.reduce((n,b)=>n+b.entities.filter(e=>e.kind==='module').length,0)+' модулей; '+available.length*3+' запусков. ESPower и ESP32C3 дополнительно повторены через банк тестов с идентичными входами.';
el('skips').textContent=data.filter(b=>b.error).map(b=>b.name+'\\n'+b.error).join('\\n\\n');
for(const b of available)el('board').add(new Option(b.name+' ('+b.components+' компонентов)',b.name));
for(const id of ['left','right'])for(let i=0;i<3;i++)el(id).add(new Option(titles[i],tags[i]));el('right').value='pads';
function metric(m){return m?'MST: '+m.wireLength+' мм; линия–линия: '+m.crossings+'; линия–пад: '+m.foreignPadHits+'; площадь: '+m.area+' мм²':'Нет данных';}
function cell(v){return v?.metrics?.board?metric(v.metrics.board)+'<br><b class="'+(v.ok?'good':'bad')+'">'+(v.ok?'Чистое размещение':'Есть ошибки размещения')+'</b><br>'+Math.round(v.ms/1000)+' с; состав: '+(v.inventoryOk?'OK':'ОШИБКА')+'; фиксированные: '+v.fixedChanges.length:'<span class="bad">Запуск не завершён</span>';}
el('overview').innerHTML='<tr><th>Плата</th>'+titles.map(t=>'<th>'+t+'</th>').join('')+'</tr>'+available.map(b=>'<tr data-board="'+b.name+'"><td>'+b.name+'<br>'+b.components+' компонентов</td>'+tags.map(t=>'<td>'+cell(b.variants[t])+'</td>').join('')+'</tr>').join('');
el('overview').onclick=e=>{const r=e.target.closest('[data-board]');if(r){el('board').value=r.dataset.board;selectBoard();el('title').scrollIntoView({block:'center'});}};
function selectBoard(){current=available.find(b=>b.name===el('board').value);el('entity').replaceChildren();for(const e of current.entities)el('entity').add(new Option(e.label+' ('+e.names.length+')',e.id));gallery();update();}
function update(){const entity=current.entities.find(e=>e.id===el('entity').value);let key=entity.id;if(el('local').checked&&entity.kind==='block')key+='-local';el('title').textContent=current.name+' / '+entity.label;
for(let side=0;side<2;side++){const tag=el(side?'right':'left').value,v=current.variants[tag],file=v?.files?.[key];el('img'+side).hidden=!file;if(file){el('img'+side).src=file;el('a'+side).href=file;}else el('img'+side).removeAttribute('src');el('caption'+side).textContent=titles[tags.indexOf(tag)]+' — '+(file?metric(v.metrics[key]):'Нет результата для выбранной стадии');}el('diagnostics').textContent=JSON.stringify(Object.fromEntries(tags.map(t=>[t,{ok:current.variants[t]?.ok,errors:current.variants[t]?.errors,diagnostics:current.variants[t]?.diagnostics,error:current.variants[t]?.error}])),null,2);}
function gallery(){el('gallery').replaceChildren();for(const e of current.entities.filter(e=>e.kind!=='board')){const button=document.createElement('button');const label=document.createElement('div');label.textContent=e.label;button.append(label);const img=document.createElement('img');img.loading='lazy';const file=current.variants.pads?.files?.[e.id];if(file)img.src=file;button.append(img);const caption=document.createElement('div');caption.textContent=metric(current.variants.pads?.metrics?.[e.id]);button.append(caption);button.onclick=()=>{el('entity').value=e.id;update();el('title').scrollIntoView({block:'center'});};el('gallery').append(button);}}
el('board').onchange=selectBoard;for(const id of ['entity','local','left','right'])el(id).onchange=update;const query=new URLSearchParams(location.search);if(available.some(b=>b.name===query.get('board')))el('board').value=query.get('board');selectBoard();if(current.entities.some(e=>e.id===query.get('entity')))el('entity').value=query.get('entity');if(query.get('local')==='1')el('local').checked=true;update();
</script></html>`);
console.log(JSON.stringify({ boards: data.filter(b => b.entities.length).length, blocks: data.reduce((n, b) => n + b.entities.filter(e => e.kind === 'block').length, 0), modules: data.reduce((n, b) => n + b.entities.filter(e => e.kind === 'module').length, 0), runs: runs.length, out }));
