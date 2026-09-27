// Reuse unchanged local block portfolios; rerun the complete board stage and
// global post-refinement after board-only scoring adjustments.
import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {gunzipSync,gzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {buildPlacementGraph} from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import {createClearanceResolver} from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import {solveBoardPrimitives} from '../src/pcb-layout/pcb-auto-place-v2/board-solver.ts';
import {unionPrimitive} from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import {refinePostPlacementAsync} from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import {createFixedPlacement} from '../src/pcb-layout/pcb-auto-place/fixed.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {createPcbLayout} from '../src/pcb-layout/pcb-auto-place/layout.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
const dir='docs/experiments/telemetry-anchored-2026-09-27';
const source=process.argv[2] ?? 'after', tag=process.argv[3] ?? 'after-final';
const raw=existsSync(`${dir}/${source}.json`)?readFileSync(`${dir}/${source}.json`):gunzipSync(readFileSync(`${dir}/${source}.json.gz`)), prior=JSON.parse(raw);
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const graph=buildPlacementGraph(input), tree=prior.stages.find(s=>s.name==='01-v2-tree').data;
const childPrimitives=graph.root.children.filter(n=>n.kind!=='pad').map(n=>{
    const p=tree.primitives.find(p=>p.sourceNodeId===n.id);
    if(!p)throw Error(`Missing saved child ${n.id}`);
    return p;
});
const diagnostics=[];
process.env.PCB_BOARD_PACKER_PROFILE='1';
const start=performance.now();
const packed=solveBoardPrimitives({input,graph,node:graph.root,childPrimitives,
    grid:input.solverOptions.placementGridStep??.5,clearance:input.board.clearances.component,
    componentByDesignator:new Map(input.components.map(c=>[c.designator,c])),blockRoleByName:new Map(input.blocks.map(b=>[b.name,b.role])),
    clearanceResolver:createClearanceResolver(input),compactness:input.solverOptions.compactness??'normal',diagnostics});
tree.root=unionPrimitive('board','board','board',graph.root.id,packed);
const fixed=new Map(input.components.flatMap(c=>{const p=createFixedPlacement(input,c);return p?[[c.designator,p]]:[];}));
const legalized=tree.root.placements.map(p=>fixed.get(p.designator)??p);
const refined=await refinePostPlacementAsync(input,legalized);
const placements=refined.placements;
const report=createPlacementReport(input,placements,[...diagnostics,...refined.diagnostics].map(d=>({...d,code:'v2_solver'})));
const layout=createPcbLayout(input,placements);
const result={placements,layout,report,ms:performance.now()-start,metrics:placementMetrics(input,placements),
    reusedLocalPortfolios:{source,sha256:createHash('sha256').update(raw).digest('hex')},
    stages:[{name:'01-v2-tree',placements:tree.root.placements,data:tree},{name:'03-v2-post-place',placements,data:refined}]};
writeFileSync(`${dir}/${tag}.json`,JSON.stringify(result,null,2));
writeFileSync(`${dir}/${tag}.json.gz`,gzipSync(JSON.stringify(result)));
const asm=createBoardAssemble(layout,{preserveBoard:true,preservedComponents:new Set(fixed.keys())});
writeFileSync(`${dir}/${tag}.assemble.json`,JSON.stringify({components:asm.components},null,2));
console.log(JSON.stringify({tag,ok:report.ok,ms:result.ms,metrics:result.metrics}));
