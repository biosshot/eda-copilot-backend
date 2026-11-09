import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {createAnalysisExecutor} from '../src/analyze/analysis-dsl/executor.ts';
import {segmentedCouponDsl} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';

const folder=process.env.P7_OUTPUT_DIR!;
assert.ok(folder&&process.env.PCB_ANALYSIS_CACHE_DIR);
const limits={maxSeconds:Math.min(25,Number(process.env.P7_VERIFY_SECONDS??25)),maxMemoryMiB:Math.min(512,Number(process.env.P7_VERIFY_MEMORY_MIB??512))};
assert.ok(Number.isInteger(limits.maxSeconds)&&limits.maxSeconds>0&&Number.isInteger(limits.maxMemoryMiB)&&limits.maxMemoryMiB>0,'Finite positive verification limits required');
process.env.P7_VERIFY_SECONDS=String(limits.maxSeconds);process.env.P7_VERIFY_MEMORY_MIB=String(limits.maxMemoryMiB);
process.env.P7_COMPACT_PLANAR='1';
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
const nativeSolve=addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked;
let nativeCalls=0;
addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked=function(...args:Parameters<typeof nativeSolve>){
 assert.notEqual(process.env.P7_SMOKE_MODE,'restart','Restart must reuse every exact-grid cache entry');
 nativeCalls++;return nativeSolve.apply(this,args);
};
const board=JSON.parse(readFileSync('tests/fixtures/analysis-segmented-coupon.json','utf8'));
board.outline=[{x:-1.5,y:-1},{x:1.5,y:-1},{x:1.5,y:1},{x:-1.5,y:1}];
board.pads=board.pads.slice(0,4);
for(const [i,pad] of board.pads.entries()){
 pad.at={x:i%2===0?-.6:.6,y:0};
 pad.shape={kind:'rect',widthMm:.8,heightMm:.8};
}
board.copper.fixed.tracks=['TOP','BOTTOM'].map((layer,i)=>({id:'wide-'+i,net:i?'RETURN':'SIGNAL',layer,widthMm:.8,points:[{x:-.6,y:0},{x:.6,y:0}]}));
board.copper.fixed.vias=[];board.copper.fixed.zones=[];
const code=segmentedCouponDsl
 .replace('boardThicknessMm:.12','boardThicknessMm:.17')
 .replaceAll('thicknessMm:.01','thicknessMm:.035')
 .replace('startHz:1000,stopHz:10000,points:3','startHz:1000000,stopHz:1100000,points:2')
 .replace('maxSeconds:300,maxMemoryMiB:2048',`maxSeconds:${limits.maxSeconds},maxMemoryMiB:${limits.maxMemoryMiB}`);
const execute=createAnalysisExecutor();
async function run(){
 let solution:any;
 const response=await executeScenario({board,code},{filename:'hybrid-coupled-rf.js',maxEntries:1000,maxOutputBytes:8*1024*1024},()=>{},async task=>{
  const result=await execute(task);
  if(task.kind==='impedance')solution=result.solution;
  return result;
 });
 assert.equal(response.status,'complete',JSON.stringify(response.error));
 assert.equal(solution.status,'complete');assert.equal(solution.coverage,'unverified');
 assert.equal(solution.provenance.operatorPolicy,'hybrid-planar-v3');
 assert.equal(solution.provenance.partitionPolicy,'automatic-hybrid-v1');
 assert.equal(solution.returnSearch?.status,'passed');
 assert.ok(solution.returnSearch.steps.length>0);
 assert.ok(solution.provenance.compactOperator.skinHints.added>0);
 assert.ok(solution.provenance.compactOperator.sectionHints.submitted>0);
 assert.ok(solution.provenance.compactOperator.sectionHints.addedCurrentModes>0);
 assert.ok(solution.portObservations.frequencies.length===2);
 assert.ok(solution.samples.every((sample:any)=>sample.lossGrams?.copperWPerA2.length===2));
 assert.equal(solution.portObservations.spatialRecovery.status,'recovered-discrete');
 for(const [i,sample] of solution.samples.entries())for(const [j,column] of sample.columns.entries()){
  assert.equal(column.returnBoundary?.passed,true);
  assert.ok(column.returnBoundary.relative<.01);
  assert.ok(column.returnBoundary.portCurrentA>column.returnBoundary.absoluteFloorA);
  assert.equal(column.spatialRecovery.status,'recovered-discrete');
  assert.ok(column.spatialRecovery.supports.length>0);
  assert.deepEqual(solution.portObservations.frequencies[i].drives[j].spatialRecoveryRef,{sampleIndex:i,columnIndex:j});
 }
 if(process.env.P7_HYBRID_REGIME_AUDIT==='1')assert.equal(solution.modelRegimeAudit?.status,'pass');
 assert.equal(solution.frequencyAudit.batch.status,'checked');
 assert.ok(solution.frequencyAudit.adaptiveResponse.points.length>2);
 return {cache:solution.cache.status,key:solution.cache.key,frequenciesHz:solution.frequenciesHz,quality:solution.quality,returnSearch:solution.returnSearch,skinHints:solution.provenance.compactOperator.skinHints,sectionHints:solution.provenance.compactOperator.sectionHints,lossGrams:solution.samples.map((sample:any)=>({frequencyHz:sample.frequencyHz,...sample.lossGrams})),modelRegimeAudit:solution.modelRegimeAudit};
}
if(process.env.P7_SMOKE_MODE==='restart'){
 const restart=await run();assert.equal(restart.cache,'disk');
 writeFileSync(join(folder,'restart.json'),JSON.stringify(restart));
}else{
 const cold=await run();assert.equal(cold.cache,'cold');
 const coldNativeCalls=nativeCalls;assert.ok(coldNativeCalls>=2);
 const warm=await run();assert.equal(warm.cache,'memory');assert.equal(warm.key,cold.key);
 assert.equal(nativeCalls,coldNativeCalls,'Warm replay must not execute a native solve');
 execFileSync(process.execPath,['--import','tsx',import.meta.filename],{env:{...process.env,P7_SMOKE_MODE:'restart'},windowsHide:true,stdio:'inherit'});
 const restart=JSON.parse(readFileSync(join(folder,'restart.json'),'utf8'));
 assert.equal(restart.key,cold.key);
 writeFileSync(join(folder,'comparison.json'),JSON.stringify({status:'pass',scope:'small coupled hybrid DSL/cache control; physical accuracy unverified',coldNativeCalls,warmNativeCalls:nativeCalls-coldNativeCalls,restartNativeCalls:0,cold,warm,restart}));
}
