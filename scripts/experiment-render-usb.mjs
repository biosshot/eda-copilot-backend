import { readFileSync,writeFileSync,mkdirSync } from 'node:fs';
import { renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
const input=JSON.parse(readFileSync('tests/fixtures/block-placement/Telemetry/input.json'));
const rows=JSON.parse(readFileSync('debugging/usb-experiments.json')).rows;
mkdirSync('debugging/usb-comparison',{recursive:true});
for(const row of rows) {
 const placements=row.solution.states.flatMap(s=>s.placements);
 writeFileSync(`debugging/usb-comparison/${row.variant}.svg`,renderPlacementSubsetSvg(input,placements,{padding:2}));
}
const cards=rows.filter(r=>['B0','N','W8','L','ALL','ALLX','ALLR'].includes(r.variant)).map(r=>
 `<figure><figcaption>${r.variant}: pair sum ${r.pairSum.toFixed(2)} mm, max ${r.pairMax.toFixed(2)} mm, area ${r.area.toFixed(1)} mm²</figcaption><img src="${r.variant}.svg"></figure>`).join('');
writeFileSync('debugging/usb-comparison/index.html',`<!doctype html><meta charset="utf-8"><title>USB block experiments</title><style>body{font:16px sans-serif;background:#e2e8f0}main{display:flex;flex-wrap:wrap}figure{background:white;padding:16px;margin:8px;width:350px}img{width:auto;height:auto;max-width:100%;image-rendering:auto}figcaption{margin-bottom:12px}</style><h1>USB Charge — local block results</h1><p>Same rendering scale. These are local solver outputs, not full-board placement results.</p><main>${cards}</main>`);
