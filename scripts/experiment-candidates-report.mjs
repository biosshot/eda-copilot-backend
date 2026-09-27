import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createCanvas, loadImage } from 'canvas';
import { renderPlacementSubsetSvg, renderPlacementSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { minimumSpanningEdges } from '../src/pcb-layout/pcb-auto-place/ratsnest.ts';
import { getPadWorld, getBox, componentPadBox, isThroughHolePad } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { segmentIntersectsBox } from '../src/pcb-layout/pcb-auto-place/utils.ts';

const out = 'docs/experiments/block-candidates-2026-09-27';
mkdirSync(out, { recursive: true });
const tags = ['before', 'clearance', 'expanded', 'refined', 'pads'];
const titles = ['До изменений', 'Исправленные зазоры', 'Больше кандидатов', 'Кандидаты + локальный postrefine', 'Со штрафом за чужие пады'];
const fixtures = [['Telemetry', 'usb_charge', 'U11'], ['ESPower', 'charger', 'U7'], ['esp32c3', 'Power', 'U1']];
const rows = [];
const read = p => JSON.parse(readFileSync(p, 'utf8'));
const root = (f, t) => `.test-output/architecture/${f}/candidates-${t}`;
const round = n => Math.round(n * 100) / 100;
function metrics(input, placements) {
    const nets = new Map(), boxes = [], pads = [];
    for (const c of input.components) {
        const p = placements.find(p => p.designator === c.designator);
        if (!p) continue;
        boxes.push(getBox(c, p));
        for (const pad of c.footprint.pads) pads.push({ box: componentPadBox(p, pad), layer: isThroughHolePad(pad) ? null : p.layer,
            ref: `${c.designator}.${pad.pin_number}`,
            net: c.pins.find(pin => String(pin.pin_number) === String(pad.pin_number))?.signal_name });
        for (const pin of c.pins) {
            if (!pin.signal_name || input.solverOptions.ignoredRatsnestSignals.some(n => n.toUpperCase() === pin.signal_name.toUpperCase())) continue;
            const q = getPadWorld(c, p, pin.pin_number);
            if (!q) continue;
            q.layer = isThroughHolePad(c.footprint.pads.find(pad => String(pad.pin_number) === String(pin.pin_number))) ? null : p.layer;
            const points = nets.get(pin.signal_name) ?? []; points.push(q); nets.set(pin.signal_name, points);
        }
    }
    const pairs = [...nets.values()].filter(p => p.length === 2);
    const segments = [...nets].flatMap(([net, ps]) => minimumSpanningEdges(ps).map(([a, b]) => [net, [ps[a], ps[b]]]));
    const wireLength = segments.reduce((total, [, [a, b]]) => total + Math.hypot(a.x - b.x, a.y - b.y), 0);
    const foreignPadHits = segments.reduce((total, [net, [a, b]]) => {
        const layer = a.layer === b.layer ? a.layer : !a.layer ? b.layer : !b.layer ? a.layer : null;
        return total + new Set(pads.filter(p => p.net !== net && (!layer || !p.layer || layer === p.layer)
            && segmentIntersectsBox(a, b, p.box)).map(p => p.ref)).size;
    }, 0);
    const hpwl = [...nets.values()].reduce((total, ps) => total + Math.max(...ps.map(p => p.x)) - Math.min(...ps.map(p => p.x))
        + Math.max(...ps.map(p => p.y)) - Math.min(...ps.map(p => p.y)), 0);
    const lengths = pairs.map(([a, b]) => Math.hypot(a.x - b.x, a.y - b.y));
    const orient = (a, b, c) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let crossings = 0;
    for (let i = 0; i < segments.length; i++) for (let j = i + 1; j < segments.length; j++) {
        if (segments[i][0] === segments[j][0]) continue;
        const [a, b] = segments[i][1], [c, d] = segments[j][1];
        if (orient(a, b, c) * orient(a, b, d) < -1e-9 && orient(c, d, a) * orient(c, d, b) < -1e-9) crossings++;
    }
    const w = Math.max(...boxes.map(b => b.right)) - Math.min(...boxes.map(b => b.left));
    const h = Math.max(...boxes.map(b => b.bottom)) - Math.min(...boxes.map(b => b.top));
    return { pairCount: pairs.length, pairSum: round(lengths.reduce((a, b) => a + b, 0)), pairMax: round(Math.max(0, ...lengths)), crossings,
        foreignPadHits, wireLength: round(wireLength), hpwl: round(hpwl), area: round(w * h), aspect: round(Math.max(w, h) / Math.min(w, h)) };
}
for (const [fixture, block, preferredAnchor] of fixtures) {
    const input = read(`tests/fixtures/block-placement/${fixture}/input.json`);
    const names = new Set(input.blocks.find(b => b.name === block).component_designators);
    const before = read(`${root(fixture, 'before')}/placement.json`).placements.filter(p => names.has(p.designator));
    const anchorName = before.some(p => p.designator === preferredAnchor) ? preferredAnchor : before[0].designator;
    const targetRotation = before.find(p => p.designator === anchorName).rotate;
    for (const tag of tags) {
        const summary = read(`${root(fixture, tag)}/summary.json`);
        const placements = read(`${root(fixture, tag)}/placement.json`).placements.filter(p => names.has(p.designator));
        const anchor = placements.find(p => p.designator === anchorName);
        const angle = targetRotation - anchor.rotate, a = angle * Math.PI / 180;
        // Only normalize the view; stored complete-board poses are unmodified.
        const normalized = placements.map(p => ({ ...p,
            x: (p.x - anchor.x) * Math.cos(a) - (p.y - anchor.y) * Math.sin(a),
            y: (p.x - anchor.x) * Math.sin(a) + (p.y - anchor.y) * Math.cos(a),
            rotate: ((p.rotate + angle) % 360 + 360) % 360,
        }));
        writeFileSync(`${out}/${fixture}-${tag}-block.svg`, renderPlacementSubsetSvg(input, normalized, { padding: 2, ratsnestTopology: 'mst', signalPaths: false }));
        writeFileSync(`${out}/${fixture}-${tag}-board.svg`, renderPlacementSvg(input, read(`${root(fixture, tag)}/placement.json`).placements, { ratsnestTopology: 'mst', signalPaths: false }));
        const tree = read(`${root(fixture, tag)}/stages/01-v2-tree.json`).data;
        const local = tree.primitives.find(p => p.kind === 'block' && p.label === block);
        rows.push({ fixture, block, tag, ok: summary.ok, fixedChanges: summary.fixedChanges,
            board: summary.stageMetrics.at(-1), blockMetrics: metrics(input, placements),
            boardGeometry: metrics(input, read(`${root(fixture, tag)}/placement.json`).placements),
            localMetrics: local ? metrics(input, local.placements) : null,
            localRefine: summary.diagnostics.filter(d => d.message.startsWith('Block postrefine')),
            ms: summary.ms, blockMs: summary.blockMs, inputHash: summary.inputHash, nativeHash: summary.nativeHash });
    }
}
writeFileSync(`${out}/measurements.json`, JSON.stringify(rows, null, 2));
async function contactSheet(kind) {
    const width = kind === 'block' ? 700 : 800, height = kind === 'block' ? 500 : 820;
    const canvas = createCanvas(width * 2, height * 3), ctx = canvas.getContext('2d');
    ctx.fillStyle = '#f3f5f7'; ctx.fillRect(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < fixtures.length; i++) for (let j = 0; j < 2; j++) {
        const [fixture, block] = fixtures[i], tag = j ? 'pads' : 'before';
        let svg = readFileSync(`${out}/${fixture}-${tag}-${kind}.svg`, 'utf8');
        svg = svg.replace(/^(<svg[^>]*width=")([\d.]+)(" height=")([\d.]+)/, (_, a, w, b, h) => `${a}${Number(w) * 4}${b}${Number(h) * 4}`);
        const img = await loadImage(Buffer.from(svg));
        const s = Math.min((width - 30) / img.width, (height - 100) / img.height);
        ctx.fillStyle = '#152c40'; ctx.font = 'bold 22px Arial';
        ctx.fillText(`${fixture}${kind === 'block' ? ' / ' + block : ''} — ${j ? 'после' : 'до'}`, j * width + 18, i * height + 30);
        const m = rows.find(r => r.fixture === fixture && r.tag === tag);
        ctx.font = '17px Arial';
        ctx.fillText(kind === 'block' ? `Длина MST: ${m.blockMetrics.wireLength} мм · линия–линия: ${m.blockMetrics.crossings} · линия–пад: ${m.blockMetrics.foreignPadHits}` : `Двухточечные связи: ${round(m.board.pairSum)} мм`, j * width + 18, i * height + 59);
        ctx.drawImage(img, j * width + (width - img.width * s) / 2, i * height + 85, img.width * s, img.height * s);
    }
    writeFileSync(`${out}/${kind}-comparison.png`, canvas.toBuffer('image/png'));
}
await contactSheet('block'); await contactSheet('board');
const sections = fixtures.map(([f, b]) => `<h2>${f} / ${b}</h2><div class="pair">${[0, 1].map(side => `<figure><img data-fixture="${f}" data-kind="block" data-side="${side}"><figcaption data-fixture="${f}" data-side="${side}"></figcaption></figure>`).join('')}</div><details><summary>Полная плата ${f}</summary><div class="pair">${[0, 1].map(side => `<a data-fixture="${f}" data-kind="board" data-side="${side}"><img data-fixture="${f}" data-kind="board" data-side="${side}"></a>`).join('')}</div></details>`).join('');
writeFileSync(`${out}/comparison.html`, `<!doctype html><meta charset="utf-8"><title>Размещение: кандидаты и локальный postrefine</title>
<style>body{font:17px system-ui;margin:24px;background:#f3f5f7;color:#152c40}header{position:sticky;top:0;background:#f3f5f7;padding:12px;z-index:1}.pair{display:grid;grid-template-columns:1fr 1fr;gap:20px}figure{margin:0;padding:12px;background:white}img{width:100%;height:480px;object-fit:contain}details img{height:850px}select{font:inherit;padding:8px;max-width:46%}summary{cursor:pointer;margin:20px 0}</style>
<h1>Кандидаты и локальный postrefine</h1><p>Три платы, пять отдельных запусков каждой. Ориентация блоков выровнена для сравнения; полные платы показаны как получены. Линии — кратчайшее связующее дерево (MST), как в оценке postrefine. Пересечения учитывают также многоточечные цепи; исключены заданные ignoredRatsnestSignals. Каждая пересечённая чужая площадка считается отдельно. Это оценка размещения, не результат трассировки.</p>
<header>${[0, 1].map(side => `<select id="s${side}">${tags.map((t, i) => `<option value="${t}" ${i === (side ? 4 : 0) ? 'selected' : ''}>${titles[i]}</option>`).join('')}</select>`).join('')}</header>${sections}
<script>const rows=${JSON.stringify(rows)}; function update(){for(const e of document.querySelectorAll('[data-fixture]')){const tag=document.getElementById('s'+e.dataset.side).value;const file=e.dataset.fixture+'-'+tag+'-'+e.dataset.kind+'.svg';if(e.tagName==='IMG')e.src=file;else if(e.tagName==='A')e.href=file;else{const r=rows.find(r=>r.fixture===e.dataset.fixture&&r.tag===tag);e.textContent='Двухточечных связей: '+r.blockMetrics.pairCount+'; длина: '+r.blockMetrics.pairSum+' мм; максимум: '+r.blockMetrics.pairMax+' мм; линия–линия: '+r.blockMetrics.crossings+'; линия–пад: '+r.blockMetrics.foreignPadHits+'; площадь: '+r.blockMetrics.area+' мм²';}}}s0.onchange=s1.onchange=update;update();</script>`);
console.log(JSON.stringify(rows.map(r => ({ fixture: r.fixture, tag: r.tag, block: r.blockMetrics, local: r.localMetrics, boardPair: round(r.board.pairSum), seconds: round(r.ms / 1000) })), null, 2));
