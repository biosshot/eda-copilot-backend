// Compare the two primary packing hypotheses before portfolio/global refinement.
import {readFileSync,writeFileSync} from 'node:fs';
import {gunzipSync,gzipSync} from 'node:zlib';
import {buildPlacementGraph} from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import {createClearanceResolver} from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import {solveBoardPrimitives} from '../src/pcb-layout/pcb-auto-place-v2/board-solver.ts';
import {withBoardPackerCapture,solveBoardPackedPrimitivesRust} from '../src/pcb-layout/pcb-auto-place-v2/board-packer-engine.ts';
import {boardAlignmentPolicy,boardAlignmentScore,alignmentErrors,alignmentHardHintsNoWorse} from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import {choosePackedPortfolio} from '../src/pcb-layout/pcb-auto-place-v2/block-portfolio.ts';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {renderPlacementSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
const dir='docs/experiments/telemetry-pack-alignment-2026-09-27';
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const prior=JSON.parse(gunzipSync(readFileSync('docs/experiments/telemetry-anchored-2026-09-27/after-final.json.gz')));
const graph=buildPlacementGraph(input),tree=prior.stages[0].data;
const childPrimitives=graph.root.children.filter(n=>n.kind!=='pad').map(n=>tree.primitives.find(p=>p.sourceNodeId===n.id));
let params;const stop=new Error('capture only');
try {withBoardPackerCapture(p=>{params=p;throw stop;},()=>solveBoardPrimitives({input,graph,node:graph.root,childPrimitives,
    grid:input.solverOptions.placementGridStep??.5,clearance:input.board.clearances.component,
    componentByDesignator:new Map(input.components.map(c=>[c.designator,c])),blockRoleByName:new Map(input.blocks.map(b=>[b.name,b.role])),
    clearanceResolver:createClearanceResolver(input),compactness:input.solverOptions.compactness??'normal',diagnostics:[]}));}
catch(e){if(e!==stop)throw e;}
const policy=boardAlignmentPolicy(input,params.primitives),cases=[];
for(const enabled of [false,true]){
    const tag=enabled?'aligned':'ordinary',start=performance.now();
    const solved=solveBoardPackedPrimitivesRust({...params,options:{...params.options,softAlignment:enabled?policy:undefined}});
    const placements=solved.result.flatMap(p=>p.placements),report=createPlacementReport(input,placements);
    cases.push({tag,ms:performance.now()-start,rank:solved.rank,roots:solved.result,placements,report,
        alignmentScore:boardAlignmentScore(solved.result,policy),alignment:alignmentErrors(solved.result,policy.pairs),metrics:placementMetrics(input,placements)});
    writeFileSync(`${dir}/probe-${tag}.svg`,renderPlacementSvg(input,placements,{ratsnestTopology:'mst',signalPaths:false}));
    writeFileSync(`${dir}/probe-${tag}-clean.svg`,renderPlacementSvg(input,placements,{ratsnest:false,signalPaths:false}));
    console.log(JSON.stringify({tag,ms:cases.at(-1).ms,alignment:cases.at(-1).alignment,metrics:cases.at(-1).metrics}));
}
const diagnostics=[];
const selected=choosePackedPortfolio(input,cases.map(c=>c.roots),diagnostics);
const result={cases,selected:cases.find(c=>c.roots===selected)?.tag,diagnostics,hardHintsNoWorse:alignmentHardHintsNoWorse(cases[0].report,cases[1].report)};
writeFileSync(`${dir}/probe.json.gz`,gzipSync(JSON.stringify(result)));
writeFileSync(`${dir}/probe-summary.json`,JSON.stringify({...result,cases:cases.map(({roots,placements,report,...rest})=>({...rest,reportOk:report.ok,
    criticalClearances:report.hintViolations.filter(v=>v.hint.relation==='clearance'&&v.hint.priority==='critical')}))},null,2));
console.log(JSON.stringify({selected:result.selected,diagnostics,hardHintsNoWorse:result.hardHintsNoWorse}));
