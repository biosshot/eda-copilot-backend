/** Planning only; never invokes a field solver. Run from backend root. */
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {createBoardChannelPlanner} from '../src/analyze/board-channel-plan.ts';
import type {FoilClosureStep} from '../src/analyze/auto-foil-plan.ts';
const root='tests/pcb-analyze/STM32G0_CAN_RS485/';
const boardPath=process.env.P7_BOARD_PATH??root+'STM32G0_CAN_RS485.json';
const sourcePath=process.env.P7_SOURCE_PATH??root+'local-extraction-complete-channel.js';
const out=process.env.P7_OUTPUT_DIR;
assert.ok(out,'Set P7_OUTPUT_DIR to a dedicated diagnostic directory');
const boardText=readFileSync(boardPath,'utf8'),code=readFileSync(sourcePath,'utf8');
const hash=(value:string)=>createHash('sha256').update(value).digest('hex');
const runs:any[]=[];
const scenario=await executeScenario({board:JSON.parse(boardText),code},
 {filename:sourcePath,maxEntries:1000,maxOutputBytes:64*1024*1024},()=>{},async(task:any)=>{
 assert.equal(task.kind,'impedance');
 let extraction:any;
 const geometry=localRequest(task,(plan:any)=>{extraction=plan;});
 const planner=createBoardChannelPlanner(task.board,geometry,extraction.terminalRequest??extraction.contacts);
 const closure:FoilClosureStep[]=[];
 const report:any={requestedFrequencyHz:task.config.frequencyHz,numericalSolves:0,
  terminals:extraction.terminalRequest??extraction.contacts,closure};
 const start=performance.now();
 try {
  const plan=planner({autoFoilDeadlineMs:start+10000,onClosureStep:step=>closure.push(step)});
  report.route='segmented';report.blocks=plan.blocks.map(b=>({id:b.id,kind:b.kind}));
 }catch(error:any){
  if(error.code!=='CHANNEL_CORRIDOR_UNSUPPORTED')throw error;
  report.route=error.foilCandidate?'joint-candidate':'refused';
  report.reason=error.message;report.geometryReason=error.geometryReason;
  report.returnClosure=error.corridorAudit.returnClosure;
  report.corridors=error.corridorAudit.corridors.map((c:any)=>({runId:c.runId,
   geometricIntervalsMm:c.geometricIntervalsMm,admissibleIntervalsMm:c.admissibleIntervalsMm,
   lateralContinuations:c.lateralContinuations,geometricEvents:c.geometricEvents}));
  report.candidate=error.foilCandidate&&{roiMm:error.foilCandidate.roiMm,
   sourceCopper:error.foilCandidate.sourceCopper,sourceBarrels:error.foilCandidate.sourceBarrels};
 }
 report.seconds=(performance.now()-start)/1000;runs.push(report);
 return {id:'planning-only',status:'partial',diagnostics:[],assumptions:[],metrics:{},solution:{planningOnly:true}};
 });
mkdirSync(out,{recursive:true});
const report={schemaVersion:1,numericalSolves:0,boardPath,sourcePath,
 boardSha256:hash(boardText),sourceSha256:hash(code),
 implementationHashes:Object.fromEntries(['src/analyze/auto-foil-plan.ts','src/analyze/board-channel-plan.ts',
  'src/analyze/run-corridors.ts'].map(path=>[path,hash(readFileSync(path,'utf8'))])),
 scenarioStatus:scenario.status,error:scenario.error,runs};
writeFileSync(join(out,'route-diagnostics.json'),JSON.stringify(report,null,2)+'\n');
assert.equal(scenario.status,'complete',JSON.stringify(scenario.error));assert.ok(runs.length);
console.log(JSON.stringify({output:join(out,'route-diagnostics.json'),numericalSolves:0,
 runs:runs.map(r=>({route:r.route,reason:r.geometryReason,seconds:r.seconds,
  closure:r.closure.map((s:FoilClosureStep)=>({iteration:s.iteration,reason:s.reason,regions:s.selectedRegions,barrels:s.selectedBarrels}))}))}));
