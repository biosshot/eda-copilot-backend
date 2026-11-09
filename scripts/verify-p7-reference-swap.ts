import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {createRequire} from 'node:module';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {createAnalysisExecutor} from '../src/analyze/analysis-dsl/executor.ts';
import {terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {segmentedCouponDslFor} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';

// One discrete-model reference swap; this is not physical accuracy acceptance.
const folder=process.env.P7_OUTPUT_DIR;
assert.ok(folder&&process.env.PCB_ANALYSIS_CACHE_DIR,'External supervision and isolated cache required');
const seconds=Number(process.env.P7_VERIFY_SECONDS??25),memory=512;
assert.ok(Number.isSafeInteger(seconds)&&seconds>=1&&seconds<=600,'P7_VERIFY_SECONDS must be an integer in [1,600]');
process.env.P7_COMPACT_PLANAR='1';
process.env.P7_VERIFY_SECONDS=String(seconds);process.env.P7_VERIFY_MEMORY_MIB=String(memory);
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
const original=addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked;
const calls:any[]=[];
addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked=function(...args:Parameters<typeof original>){
 calls.push(structuredClone(args[1]));return original.apply(this,args);
};
const boardPath='tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json';
const base=JSON.parse(readFileSync(boardPath,'utf8'));
// Existing signal via remains: input TOP -> output BOTTOM. Move output return
// to TOP, and connect its TOP/BOTTOM copper through a distinct return via.
base.outline=[{x:-.3,y:-1.1},{x:10.3,y:-1.1},{x:10.3,y:.6},{x:-.3,y:.6}];
for(const pad of base.pads.filter((p:any)=>p.net==='RETURN'))pad.at.y=-.6;
base.pads[3].layers=['TOP'];
const tracks=base.copper.fixed.tracks;
tracks[1].points=[{x:0,y:-.6},{x:5,y:-.6}];
tracks.push({id:'top-return',net:'RETURN',layer:'TOP',widthMm:.1,points:[{x:5,y:-.6},{x:10,y:-.6}]});
const returnVia={id:'return-via',net:'RETURN',at:{x:5,y:-.6},diameterMm:.3,drillMm:.15,type:'through',fromLayer:'TOP',toLayer:'BOTTOM'};
const code=segmentedCouponDslFor('via')
 .replace("negative:pad('J1','2'),positiveLayer:'BOTTOM',negativeLayer:'BOTTOM'","negative:pad('J1','2'),positiveLayer:'BOTTOM',negativeLayer:'TOP'")
 .replace('startHz:1000,stopHz:10000,points:3','startHz:1000000,stopHz:1000000,points:1')
 .replace('maxSeconds:300,maxMemoryMiB:2048',`maxSeconds:${seconds},maxMemoryMiB:512`);
assert.match(code,/positiveLayer:'BOTTOM',negativeLayer:'TOP'/,'output physical reference must change layers');
const execute=createAnalysisExecutor();
async function run(intact:boolean){
 const board=structuredClone(base);
 if(intact)board.copper.fixed.vias.push(returnVia);
 const first=calls.length;let solution:any;
 const response=await executeScenario({board,code},{filename:intact?'reference-swap-intact.js':'reference-swap-open.js',maxEntries:1000,maxOutputBytes:16*1024*1024},()=>{},async task=>{
  const result=await execute(task);if(task.kind==='impedance')solution=result.solution;return result;
 });
 return {response,solution,geometry:calls.slice(first)};
}
try {
 const intact=await run(true);
 assert.equal(intact.response.status,'complete',JSON.stringify(intact.response.error));
 const solution=intact.solution;
 assert.equal(solution.status,'complete');assert.equal(solution.coverage,'unverified');
 assert.equal(solution.provenance.operatorPolicy,'hybrid-planar-v3');
 assert.equal(solution.provenance.partitionPolicy,'automatic-hybrid-v1');
 assert.deepEqual(solution.physicalPorts.map((p:any)=>[p.positive.layer,p.negative.layer]),[['TOP','BOTTOM'],['BOTTOM','TOP']]);
 const paths=solution.foilPlan.sourceBarrels.map((s:any)=>s.sourcePath);
 assert.ok(paths.includes('copper.fixed.vias[0]')&&paths.includes('copper.fixed.vias[1]'),'signal and return barrels must both be retained');
 const geometry=intact.geometry.find((g:any)=>g.roiMm&&g.electrodes);
 assert.ok(geometry,'missing native geometry');
 const origin=geometry.coordinateOriginMm??geometry.roiMm[0];
 const s=solution.parameters[0];
 for(const [j,c] of solution.samples[0].columns.entries()) {
  assert.ok(Number.isFinite(c.kclRelative)&&c.kclRelative<=1e-8&&Number.isFinite(c.kvlRelative)&&c.kvlRelative<=1e-8,'original physical KCL/KVL gate');
  const reflected=s.reduce((sum:number,row:any[])=>sum+row[j].real**2+row[j].imaginary**2,0);
  const wavePower=50/4*(1-reflected),loss=c.copperLossW;
  assert.ok(Number.isFinite(wavePower)&&Number.isFinite(loss)&&loss>0);
  assert.ok(Math.abs(wavePower-loss)<=1e-12+1e-8*(Math.abs(wavePower)+loss),'matched-wave/copper power balance');
 }
 const supports=solution.samples[0].columns.flatMap((c:any)=>{
  assert.equal(c.spatialRecovery.status,'recovered-discrete');
  assert.ok(Number.isFinite(c.kclRelative)&&Number.isFinite(c.kvlRelative)&&Number.isFinite(c.powerBalanceErrorW)&&c.copperLossW>0);
  return c.spatialRecovery.supports.filter((s:any)=>{
   if(s.kind!=='tetrahedron')return false;
   const center=[0,1,2].map(k=>s.verticesM.reduce((v:number,p:number[])=>v+p[k],0)/4);
   const x=origin[0]+1000*center[0],y=origin[1]+1000*center[1];
   return Math.abs(x-5)<.16&&Math.abs(y+.6)<.16&&center[2]>1e-5&&center[2]<.00021;
  });
 });
 const returnLoss=supports.reduce((v:number,s:any)=>v+s.lossW,0);
 const returnCurrentDensity=Math.max(0,...supports.map((s:any)=>Math.hypot(...s.currentDensityRealAPerM2,...s.currentDensityImaginaryAPerM2)));
 assert.ok(supports.length>0&&returnLoss>0&&Number.isFinite(returnLoss)&&returnCurrentDensity>0,'no recovered current and loss in return barrel');
 const open=await run(false);let negative:any;
 if(open.response.status==='complete'){
  assert.equal(open.solution.status,'complete');
  const a=solution.parameters[0],b=open.solution.parameters[0];let difference=0;
  for(let i=0;i<a.length;i++)for(let j=0;j<a[i].length;j++)difference=Math.max(difference,Math.hypot(a[i][j].real-b[i][j].real,a[i][j].imaginary-b[i][j].imaginary));
  assert.ok(Number.isFinite(difference)&&difference>1e-7,`open return via did not change response: ${difference}`);
  negative={status:'complete',maxSChange:difference};
 }else{
  const error=open.response.error;
  assert.match(JSON.stringify(error),/CHANNEL_FOIL_CONTACT|CHANNEL_SEGMENTATION|COUPLED_DC_INCOMPATIBLE|LOCAL_BOARD_ELECTRODE|CHANNEL_FOIL_UNSUPPORTED/,'open case must have an explicit physical/topology refusal');
  negative={status:'refused',error};
 }
 writeFileSync(join(folder,'comparison.json'),JSON.stringify({status:'pass',scope:'single-frequency discrete model; physical accuracy unverified',boardPath,physicalPorts:solution.physicalPorts,barrelSources:paths,returnBarrelSupports:supports.length,returnBarrelLossW:returnLoss,returnCurrentDensityAPerM2:returnCurrentDensity,quality:solution.quality,negative},null,2));
}catch(error){
 writeFileSync(join(folder,'comparison.json'),JSON.stringify({status:'validation-failure',reason:String(error),scope:'single-frequency discrete model; physical accuracy unverified'},null,2));
 throw error;
}finally{
 addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked=original;
 await terminateAnalysisPool();
}
