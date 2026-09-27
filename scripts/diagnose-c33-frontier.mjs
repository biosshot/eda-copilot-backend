// Historical A/B harness: its environment modes were retired by the unified policy.
if (process.argv[1]?.endsWith('diagnose-c33-frontier.mjs')) throw new Error('Archived placement experiment: replay on commit 90a77cf; use experiment-telemetry-unified.mjs on this branch.');
import {readFileSync,writeFileSync} from 'node:fs';
import {buildPlacementGraph} from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import {solvePlacementSubtreeSync} from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import {withBlockSolverCapture,solveBlockPrimitivesRust} from '../src/pcb-layout/pcb-auto-place-v2/block-solver-engine.ts';
import {translatePrimitive,rotatePrimitive,unionPrimitive} from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import {getPadWorld} from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import {validatePrimitive} from '../src/pcb-layout/pcb-auto-place/primitive-validation.ts';
import {renderPlacementSubsetSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
const out='docs/experiments/telemetry-frontier-2026-09-27';
const input=JSON.parse(readFileSync('tests/fixtures/block-placement/Telemetry/input.json'));
const layout=JSON.parse(readFileSync(`${out}/lte_power/both.json`));
Object.assign(process.env,{PCB_BLOCK_PROFILE:'full',PCB_BLOCK_FRONTIER:'1',PCB_BLOCK_PAD_OWNER:'1',PCB_BLOCK_LOCAL_ACCESS:'0',PCB_BLOCK_RELAX_GROUPS:'off',PCB_NATIVE_SOLVE_CACHE:'0'});
const graph=buildPlacementGraph(input),find=n=>n.kind==='block'&&n.label==='lte_power'?n:n.children.map(find).find(Boolean);
let params;const stop=new Error('capture');try{withBlockSolverCapture(p=>{if(p.node.label==='lte_power'){params=p;throw stop}},()=>solvePlacementSubtreeSync({input,graph,node:find(graph.root)}));}catch(e){if(e!==stop)throw e}
const moved=(p,target)=>{const r=rotatePrimitive(p,target.rotate-p.placements[0].rotate);return translatePrimitive(r,target.x-r.placements[0].x,target.y-r.placements[0].y)};
const fixed=params.primitives.map(p=>({...moved(p,layout.placements.find(q=>q.designator===p.placements[0].designator)),locked:true,canRotate:false}));
const index=fixed.findIndex(p=>p.label==='C33'),original=fixed[index];
const component=input.components.find(c=>c.designator==='C33'),ic=input.components.find(c=>c.designator==='U10');
const target=getPadWorld(ic,layout.placements.find(p=>p.designator==='U10'),'3');
const length=p=>{const a=getPadWorld(component,p.placements[0],'1');return Math.hypot(a.x-target.x,a.y-target.y)};
const evaluate=ps=>{const root=unionPrimitive('probe','block','probe','probe',ps);if(!validatePrimitive(input,root).ok)return null;
    const result=solveBlockPrimitivesRust({...params,primitives:ps,options:{...params.options,searchWidth:1}});
    return {baseScore:result.rank.score,length:length(ps[index]),pose:ps[index].placements[0],metrics:placementMetrics(input,root.placements)};};
const baseline=evaluate(fixed);let nearest=baseline,best=baseline;let legal=0;
for(const rotate of component.pcb.allowedRotations)for(let x=-5;x<=5;x+=.5)for(let y=-5;y<=5;y+=.5){
    const p=moved(original,{x:target.x+x,y:target.y+y,rotate}),ps=fixed.map((v,i)=>i===index?p:v),r=evaluate(ps);if(!r)continue;legal++;
    if(r.length<nearest.length)nearest=r;if(r.baseScore<best.baseScore)best=r;
}
const local={};for(const metric of ['micro','off','geometric']){const ps=fixed.map((p,i)=>i===index?{...p,locked:false,canRotate:true}:p);
    const r=solveBlockPrimitivesRust({...params,primitives:ps,options:{...params.options,experiments:{...params.options.experiments,routingMetric:metric},searchWidth:1}});
    const candidate=r.result.find(p=>p.label==='C33');local[metric]={length:length(candidate),pose:candidate.placements[0],baseScore:r.rank.score};}
const nearPrimitives=fixed.map((p,i)=>i===index?moved(original,nearest.pose):p);
const withoutRelation=params.relations.map(relation=>{
    const score=ps=>solveBlockPrimitivesRust({...params,primitives:ps,relations:params.relations.filter(r=>r!==relation),options:{...params.options,searchWidth:1}}).rank.score;
    return {id:relation.id,from:relation.from,to:relation.to,baseline:baseline.baseScore-score(fixed),nearest:nearest.baseScore-score(nearPrimitives)};
}).filter(r=>Math.abs(r.nearest-r.baseline)>1).sort((a,b)=>(b.nearest-b.baseline)-(a.nearest-a.baseline));
const result={note:'Grid probe keeps every other component fixed. baseScore excludes the micro-router correction; locked state is identical across grid samples. This is a diagnostic, not a production layout.',legal,baseline,nearest,best,local,relationContributions:withoutRelation};
writeFileSync(`${out}/c33-probe.json`,JSON.stringify(result,null,2));
for(const [name,r] of Object.entries({baseline,nearest,best})){
    const ps=fixed.map((p,i)=>i===index?moved(original,r.pose):p).flatMap(p=>p.placements);
    writeFileSync(`${out}/c33-${name}.svg`,renderPlacementSubsetSvg(input,ps,{ratsnestTopology:'mst',signalPaths:false,padding:2}).replace(/[ \t]+$/gm,''));
}
console.log(JSON.stringify(result));
