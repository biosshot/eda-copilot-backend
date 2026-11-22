import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {createRequire} from 'node:module';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {createAnalysisExecutor} from '../src/analyze/analysis-dsl/executor.ts';
import {terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {segmentedCouponDsl} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';

// Bounded positive discrete-model control, not a physical accuracy acceptance.
const folder=process.env.P7_OUTPUT_DIR;
assert.ok(folder&&process.env.PCB_ANALYSIS_CACHE_DIR,'Use the process-tree supervisor with isolated cache');
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
const original=addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked;
const geometryCalls:any[]=[];
addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked=function(...args:Parameters<typeof original>){
 geometryCalls.push(structuredClone(args[1]));return original.apply(this,args);
};
const base=JSON.parse(readFileSync('tests/fixtures/analysis-segmented-coupon.json','utf8'));
base.outline=[{x:-1.5,y:-1},{x:1.5,y:-1},{x:1.5,y:1.65},{x:-1.5,y:1.65}];
base.pads=base.pads.slice(0,4);
for(const [i,pad] of base.pads.entries()){
 pad.at={x:i%2===0?-.6:.6,y:0};pad.shape={kind:'rect',widthMm:.4,heightMm:.4};
}
const track=(id:string,layer:string,net:string,points:number[][],widthMm:number)=>({id,layer,net,widthMm,points:points.map(([x,y])=>({x,y}))});
const signal=track('signal','TOP','SIGNAL',[[-.6,0],[.6,0]],.4);
const left=track('return-left','BOTTOM','RETURN',[[-.6,0],[-.6,1.2]],.2);
const bridge=track('return-remote-bridge','BOTTOM','RETURN',[[-.6,1.2],[.6,1.2]],.2);
const right=track('return-right','BOTTOM','RETURN',[[.6,1.2],[.6,0]],.2);
base.copper.fixed.vias=[];base.copper.fixed.zones=[];
const code=segmentedCouponDsl
 .replace('boardThicknessMm:.12','boardThicknessMm:.17')
 .replaceAll('thicknessMm:.01','thicknessMm:.035')
 .replace('startHz:1000,stopHz:10000,points:3','startHz:1000000,stopHz:1000000,points:1');
const execute=createAnalysisExecutor();
async function run(closed:boolean){
 const board=structuredClone(base);
 board.copper.fixed.tracks=closed?[signal,left,bridge,right]:[signal,left,right];
 const first=geometryCalls.length;
 let solution:any;
 const response=await executeScenario({board,code},{filename:closed?'remote-return-closed.js':'remote-return-open.js',maxEntries:1000,maxOutputBytes:8*1024*1024},()=>{},async task=>{
  const result=await execute(task);if(task.kind==='impedance')solution=result.solution;return result;
 });
 return {response,solution,geometry:geometryCalls.slice(first)};
}
try {
 const closed=await run(true);
 assert.equal(closed.response.status,'complete',JSON.stringify(closed.response.error));
 const solution=closed.solution;
 assert.equal(solution.status,'complete');assert.equal(solution.coverage,'unverified');
 assert.equal(solution.provenance.operatorPolicy,'hybrid-planar-v3');
 assert.equal(solution.provenance.partitionPolicy,'automatic-hybrid-v1');
 // Endpoint-only local window: pad half-width .2 mm plus the planner's
 // 2*stack-height margin (.34 mm). The bridge is centred at y=1.2 mm.
 const initialLocalUpperMm=.2+2*.17;
 assert.ok(1.2-.1>initialLocalUpperMm);
 assert.ok(solution.foilPlan.roiMm[1][1]>1.2);
 assert.ok(solution.foilPlan.sourceCopper.some((s:any)=>s.sourcePath==='copper.fixed.tracks[2]'&&s.layer==='BOTTOM'),'remote bridge absent from selected copper');
 const geometry=closed.geometry.find((g:any)=>g.roiMm&&g.electrodes);
 assert.ok(geometry,'native geometry not captured');
 const origin=geometry.coordinateOriginMm??geometry.roiMm[0];
 const sample=solution.samples[0];const column=sample.columns[0];
 for(const [index,drive] of solution.samples[0].columns.entries()){
  assert.ok(drive.kclRelative>=0&&drive.kclRelative<=1e-8);
  assert.ok(drive.kvlRelative>=0&&drive.kvlRelative<=1e-8);
  const outgoing=solution.parameters[0].reduce((sum:number,row:any[])=>sum+row[index].real**2+row[index].imaginary**2,0);
  const net=50/4*(1-outgoing);
  const limit=1e-12+1e-8*(Math.abs(net)+drive.copperLossW);
  assert.ok(drive.copperLossW>0&&drive.powerBalanceErrorW>=0&&drive.powerBalanceErrorW<=limit);
  assert.ok(Math.abs(net-drive.copperLossW)<=limit,'wave/copper power balance');
 }
 assert.equal(column.spatialRecovery.status,'recovered-discrete');
 const bridgeSupports=column.spatialRecovery.supports.filter((s:any)=>{
  if(s.kind!=='foil'||s.layer!=='BOTTOM')return false;
  const x=origin[0]+1000*s.xyM.reduce((v:number,p:number[])=>v+p[0],0)/3;
  const y=origin[1]+1000*s.xyM.reduce((v:number,p:number[])=>v+p[1],0)/3;
  return Math.abs(x)<.35&&y>1.05&&y<1.35;
 });
 const remoteLoss=bridgeSupports.reduce((v:number,s:any)=>v+s.lossW,0);
 const remoteCurrentDensity=Math.max(0,...bridgeSupports.map((s:any)=>Math.hypot(...s.currentDensityRealAPerM2,...s.currentDensityImaginaryAPerM2)));
 assert.ok(bridgeSupports.length>0&&remoteLoss>0&&Number.isFinite(remoteLoss)&&remoteCurrentDensity>0,'no nonzero recovered current and power in remote copper');
 assert.ok(column.copperLossW>0&&Number.isFinite(column.powerBalanceErrorW));
 const opened=await run(false);
 let negative:any;
 if(opened.response.status==='complete'){
  assert.equal(opened.solution.status,'complete');
  const a=solution.parameters[0],b=opened.solution.parameters[0];
  let difference=0;
  for(let i=0;i<a.length;i++)for(let j=0;j<a[i].length;j++)difference=Math.max(difference,Math.hypot(a[i][j].real-b[i][j].real,a[i][j].imaginary-b[i][j].imaginary));
  assert.ok(Number.isFinite(difference)&&difference>1e-7,`open bridge did not change response: ${difference}`);
  negative={status:'complete',maxSChange:difference};
 }else{
  const error=opened.response.error;
  assert.match(JSON.stringify(error),/CHANNEL_FOIL_CONTACT|CHANNEL_SEGMENTATION|COUPLED_DC_INCOMPATIBLE|LOCAL_BOARD_ELECTRODE|CHANNEL_FOIL_UNSUPPORTED/,'open case must have an explicit physical/topology refusal');
  negative={status:'refused',error};
 }
 writeFileSync(join(folder,'comparison.json'),JSON.stringify({status:'pass',scope:'bounded discrete model; physical accuracy unverified',initialLocalUpperMm,roiMm:solution.foilPlan.roiMm,remoteBridgeSource:'copper.fixed.tracks[2]',remoteSupports:bridgeSupports.length,remoteLossW:remoteLoss,remoteCurrentDensityAPerM2:remoteCurrentDensity,copperLossW:column.copperLossW,powerBalanceErrorW:column.powerBalanceErrorW,negative},null,2));
}catch(error){
 writeFileSync(join(folder,'remote-return-error.json'),JSON.stringify({error:String(error),geometryCalls},null,2));
 writeFileSync(join(folder,'comparison.json'),JSON.stringify({status:'validation-failure',reason:String(error),scope:'bounded discrete model; physical accuracy unverified'},null,2));
 throw error;
}finally{
 addon.AnalysisElectricalSession.prototype.solveBoardFoilChecked=original;
 await terminateAnalysisPool();
}
