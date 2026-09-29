import { readFileSync, writeFileSync } from 'node:fs';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { solvePlacementIslands } from '../src/pcb-layout/pcb-auto-place-v2/island-solver.ts';
import { createClearanceResolver } from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import { getPadWorld, componentPairCollisionBoxPairs, overlaps } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { createCanvas, loadImage } from 'canvas';
const input = JSON.parse(readFileSync('tests/fixtures/block-placement/Telemetry/input.json'));
const graph = buildPlacementGraph(input), islands = solvePlacementIslands(input, graph), clearance = createClearanceResolver(input);
for (const [name, refs] of [['lte_switch', [['U10.11','L1.1'], ['U10.9','L1.2']]], ['logic_switch', [['U12.4','L2.1'], ['U12.2','L2.2']]]]) {
    const original = islands.find(i => i.label === `core_pairs:${name}`).placements;
    const [a,b] = original.map(p => input.components.find(c => c.designator === p.designator));
    function length(ps) { return refs.reduce((n, pair) => { const q = pair.map(ref => { const [name,pin] = ref.split('.'); return getPadWorld(input.components.find(c=>c.designator===name), ps.find(p=>p.designator===name), pin); }); return n+Math.hypot(q[0].x-q[1].x,q[0].y-q[1].y); },0); }
    let best = { length: length(original), placements: original };
    for (const rotate of b.pcb.allowedRotations) for (let x=-7;x<=7;x+=.1) for(let y=-7;y<=7;y+=.1) {
        const ps = [original[0], {...original[1], rotate, x:original[0].x+x, y:original[0].y+y}];
        if(componentPairCollisionBoxPairs(a,ps[0],b,ps[1]).some(p=>overlaps(p.a,p.b,clearance(a.designator,b.designator)))) continue;
        const d=length(ps); if(d<best.length) best={length:d,placements:ps};
    }
    console.log(JSON.stringify({name,selectedLength:length(original),selected:original,shortestCenterDistanceIgnoringPadObstacles:best}));
}
const canvas=createCanvas(1600,1100),ctx=canvas.getContext('2d');ctx.fillStyle='white';ctx.fillRect(0,0,1600,1100);
for(const [k,id] of [8,11,16,27].entries()) {
    let svg=readFileSync(`docs/experimental/pcb/global-placement-2026-09-27/Telemetry/pads-block-${id}.svg`,'utf8');
    svg=svg.replace(/width="([0-9.]+)" height="([0-9.]+)"/,(_,w,h)=>`width="${w*3}" height="${h*3}"`);
    const img=await loadImage(Buffer.from(svg)),s=Math.min(780/img.width,530/img.height);
    ctx.drawImage(img,k%2*800,Math.floor(k/2)*550,img.width*s,img.height*s);
}
writeFileSync('debugging/diagnose-blocks.png',canvas.toBuffer('image/png'));
