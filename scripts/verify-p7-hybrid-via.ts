import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {createAnalysisExecutor} from '../src/analyze/analysis-dsl/executor.ts';
import {terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {segmentedCouponDslFor} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';

const folder=process.env.P7_OUTPUT_DIR;
assert.ok(folder&&process.env.PCB_ANALYSIS_CACHE_DIR,'external supervision and isolated cache required');
process.env.P7_COMPACT_PLANAR='1';
const seconds=Number(process.env.P7_VERIFY_SECONDS??25);
assert.ok(Number.isSafeInteger(seconds)&&seconds>=1&&seconds<=600,'P7_VERIFY_SECONDS must be an integer in [1,600]');
process.env.P7_VERIFY_SECONDS=String(seconds);
process.env.P7_VERIFY_MEMORY_MIB='512';
const boardPath='tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json';
const board=JSON.parse(readFileSync(boardPath,'utf8'));
const code=segmentedCouponDslFor('via')
 .replace('startHz:1000,stopHz:10000,points:3','startHz:1000000,stopHz:1000000,points:1')
 .replace('maxSeconds:300,maxMemoryMiB:2048',`maxSeconds:${seconds},maxMemoryMiB:512`);
const execute=createAnalysisExecutor();
let solution:any;
try{
 const response=await executeScenario({board,code},{filename:'hybrid-via.js',maxEntries:1000,maxOutputBytes:16*1024*1024},()=>{},async task=>{
  const result=await execute(task);if(task.kind==='impedance')solution=result.solution;return result;
 });
 assert.equal(response.status,'complete',JSON.stringify(response.error));
 assert.equal(solution.status,'complete');
 assert.equal(solution.provenance.operatorPolicy,'hybrid-planar-v3');
 assert.equal(solution.provenance.partitionPolicy,'automatic-hybrid-v1');
 assert.equal(solution.coverage,'unverified');
 assert.ok(solution.foilPlan.sourceBarrels.length>0,'real barrel must be retained');
 assert.equal(solution.samples.length,1);
 assert.equal(solution.samples[0].columns.length,2);
 for(const [index,drive] of solution.samples[0].columns.entries()){
  assert.ok(drive.kclRelative>=0&&drive.kclRelative<=1e-8);
  assert.ok(drive.kvlRelative>=0&&drive.kvlRelative<=1e-8);
  const outgoing=solution.parameters[0].reduce((sum:number,row:any[])=>sum+row[index].real**2+row[index].imaginary**2,0);
  const net=50/4*(1-outgoing);
  const limit=1e-12+1e-8*(Math.abs(net)+drive.copperLossW);
  assert.ok(drive.copperLossW>0&&drive.powerBalanceErrorW>=0&&drive.powerBalanceErrorW<=limit);
  assert.ok(Math.abs(net-drive.copperLossW)<=limit,'wave/copper power balance');
 }
 assert.equal(solution.samples[0].lossGrams.copperWPerA2.length,2);
 assert.equal(solution.portObservations.spatialRecovery.status,'recovered-discrete');
 writeFileSync(`${folder}/comparison.json`,JSON.stringify({status:'pass',scope:'unchanged via coupon through automatic hybrid DSL/addon; physical accuracy unverified',boardPath,solution},null,2));
}catch(error){
 writeFileSync(`${folder}/comparison.json`,JSON.stringify({status:'validation-failure',reason:String(error),boardPath,solution},null,2));
 throw error;
}finally{await terminateAnalysisPool();}
