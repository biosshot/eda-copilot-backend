import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {gzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {autoPlacePcbWithReportAsync} from '../src/pcb-layout/pcb-auto-place/auto-place.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

const [inputPath,out]=process.argv.slice(2);
if(!inputPath||!out)throw Error('Usage: experiment-placement-regression-run.mjs INPUT OUTPUT_DIR');
mkdirSync(out,{recursive:true});
const raw=readFileSync(inputPath),input=JSON.parse(raw);
const started=performance.now();
const result=await autoPlacePcbWithReportAsync(input);
const fixed=new Set(input.components.filter(c=>c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace).map(c=>c.designator));
const asm=createBoardAssemble(result.layout,{preserveBoard:true,preservedComponents:fixed});
writeFileSync(`${out}/assembly.json`,JSON.stringify(BoardAssembleSchema().parse({components:asm.components}),null,2));
const stage=result.stages.find(s=>s.name==='01-v2-tree');
if(!stage?.data?.primitives)throw Error('Missing saved block-stage primitives');
const expected=new Set(input.components.map(c=>c.designator)),actual=new Set(result.placements.map(p=>p.designator));
const inventoryOk=expected.size===actual.size&&result.placements.length===expected.size&&[...actual].every(ref=>expected.has(ref));
const fixedChanges=result.placements.filter(p=>{const c=input.components.find(c=>c.designator===p.designator);
    if(!c?.pcb.fixedPlacement)return false;const f=c.pcb.fixedPlacement;
    return Math.abs(p.x-f.x)>.005||Math.abs(p.y-f.y)>.005||f.rotate!=null&&p.rotate!==f.rotate||f.layer!=null&&p.layer!==f.layer;
}).map(p=>p.designator);
const summary={inputSha256:createHash('sha256').update(raw).digest('hex'),ms:performance.now()-started,
    componentCount:input.components.length,blockCount:input.blocks.length,reportOk:result.report.ok,inventoryOk,fixedChanges,
    metrics:placementMetrics(input,result.placements),
    violations:Object.fromEntries(['unplaced','outsideBoard','overlaps','boardHoleViolations','constraintRegionViolations','layerViolations','hintViolations'].map(k=>[k,result.report[k]?.length??0])),
    diagnostics:result.report.graphReport?.diagnostics??[]};
writeFileSync(`${out}/result.json.gz`,gzipSync(JSON.stringify({placements:result.placements,report:result.report,
    localBlocks:stage.data.primitives.filter(p=>p.kind==='block').map(p=>({id:p.id,label:p.label,sourceNodeId:p.sourceNodeId,placements:p.placements}))})));
writeFileSync(`${out}/summary.json`,JSON.stringify(summary,null,2));
console.log(JSON.stringify({ms:summary.ms,reportOk:summary.reportOk,inventoryOk,fixedChanges,metrics:summary.metrics}));
