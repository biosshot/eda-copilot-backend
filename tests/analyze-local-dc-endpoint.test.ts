import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisLocalDcReport } from '../native/analyze/index.js';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createLocalExecutor } from '../src/analyze/analysis-dsl/local-executor.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { isAnalysisProblem, type LocalDcExtractionProblem } from '../src/analyze/types.ts';
import { segmentedCoupon, segmentedCouponDsl } from './helpers/pcb-analysis-segmented-coupon.ts';

test('layered DC reaches the native copper endpoint without preparing an AC model', async()=>{
 const execute=createLocalExecutor();
 let response:Awaited<ReturnType<typeof execute>>|undefined;
 const code=segmentedCouponDsl.split('a.plot(')[0]
  .replace(".frequency({startHz:1000,stopHz:10000,points:3,scale:'log'})",'.frequencyHz(0)');
 const result=await executeScenario({board:segmentedCoupon(),code},
  {filename:'layered-native-dc.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
  async task=>response=await execute(task as AnalysisTask<'impedance'>));
 assert.equal(result.status,'complete',JSON.stringify(result.error));
 assert.ok(response);
 assert.equal(response.network,undefined);
 const solution=response.solution as any;
 assert.equal(solution.frequencyHz,0);
 assert.equal(solution.allowInsulatedCopperCuts,false);
 assert.deepEqual(solution.driveA,[1,-1]);
 assert.deepEqual(solution.portRealV,[null,null]);
 assert.deepEqual(solution.electrodeGalvanicComponents,solution.electrodeLoadedComponents);
 assert.ok(solution.loopProbe.resistanceOhm>0);
 assert.ok(Math.abs(solution.totalLossW/solution.drivePowerW-1)<1e-8);
 assert.equal(solution.validity.domain.continuousFrequencyCoverage,false);
});

test('layered DSL sends an isolated exact DC endpoint and retains unexcited galvanic islands',async()=>{
 const calls:LocalDcExtractionProblem[]=[];
 const execute=createLocalExecutor(((problem:LocalDcExtractionProblem):AnalysisLocalDcReport=>{
  assert.equal(problem.kind,'local-dc-extraction');
  assert.ok(isAnalysisProblem(problem));
  assert.ok(problem.remainingSeconds>0);
  const {geometry,drive}=problem.request;
  const model=geometry as {allowInsulatedCopperCuts:boolean;ports:[string,string][]};
  assert.equal(model.allowInsulatedCopperCuts,false);
  assert.equal(problem.request.loadsOhm,undefined);
  assert.equal(problem.request.observablesOnly,true);
  calls.push(problem);
  return {
   status:'complete',frequencyHz:0,ports:model.ports,driveA:drive??[0,0],loadsOhm:[null,null],
   portRealV:[null,null],floatingPortIndices:[0,1],drivePowerW:drive?.some(Boolean)?0.7:0,totalLossW:drive?.some(Boolean)?0.7:0,loadLossW:0,
   electrodeNames:['signal-TOP','signal-TOP-1','return-BOTTOM','return-BOTTOM-1'],
   electrodeGalvanicComponents:drive?[0,0,1,1]:[0,1,2,2],electrodeLoadedComponents:drive?[0,0,1,1]:[0,1,2,2],electrodeVoltagesV:[0,0,0,0],
   dcConductanceS:[[1,-1,0,0],[-1,1,0,0],[0,0,1,-1],[0,0,-1,1]],
   normalizedElectrodeResidual:0,roiMm:[[0,0],[10,1]],domainPolygonMm:null,allowInsulatedCopperCuts:false,
   sourceMapping:[],validity:{status:'conditional',domain:{frequencyHz:0},reason:'Finite copper'},
  };
 }) as NonNullable<Parameters<typeof createLocalExecutor>[0]>);
 const code=segmentedCouponDsl.split('a.plot(')[0]
  .replace(".frequency({startHz:1000,stopHz:10000,points:3,scale:'log'})",".frequencyHz(0)")
  .replace("conductivitySM:58e6,state:'Explicit uniform copper coupon'","conductivitySM:58e6,state:'Explicit uniform copper coupon',analyses:['dc'],validity:{status:'conditional',domain:{bandHz:[0,0]}}");
 const run=async(board:any,source=code)=>executeScenario(
  {board,code:source},{filename:'layered-dc.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
  async task=>{
   const result=await execute(task as AnalysisTask<'impedance'>);
   assert.equal(result.network,undefined);
   assert.equal(result.metrics.characteristicImpedance,undefined);
   const data=result.metrics.segments.data as any;
   assert.equal(data.frequencyHz,0);
   assert.deepEqual(data.ports.map((port:any)=>port.name),['input','output']);
   assert.deepEqual(data.portRealV,[null,null]);
   assert.deepEqual(result.metrics.segments.validity.domain,{frequencyHz:0,continuousFrequencyCoverage:false});
   if(calls.at(-1)?.request.drive)assert.equal(data.loopProbe.resistanceOhm,0.7);
   else assert.equal(data.loopProbe,undefined);
   return result;
  });
 const connected=await run(segmentedCoupon());
 assert.equal(connected.status,'complete',JSON.stringify(connected.error));
 assert.deepEqual(calls.at(-1)?.request.drive,[1,-1]);
 const reversed=code.replace("positive:pad('J1','1'),negative:pad('J1','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'",
  "positive:pad('J1','2'),negative:pad('J1','1'),positiveLayer:'BOTTOM',negativeLayer:'TOP'");
 const flipped=await run(segmentedCoupon(),reversed);
 assert.equal(flipped.status,'complete',JSON.stringify(flipped.error));
 assert.deepEqual(calls.at(-1)?.request.drive,[1,1]);
 const board=segmentedCoupon();
 const signal=board.copper.fixed.tracks[0];
 signal.points[1].x=4;
 board.copper.fixed.tracks.push({...signal,id:'detached-signal',points:[{x:6,y:0},{x:10,y:0}]});
 const disconnected=await run(board);
 assert.equal(disconnected.status,'complete',JSON.stringify(disconnected.error));
 assert.equal(calls.at(-1)?.request.drive,undefined);
 const badBand=await run(segmentedCoupon(),code.replace('bandHz:[0,0]','bandHz:[1000,100000]'));
 assert.match(JSON.stringify(badBand.error),/does not include exact DC/);
 const badPlating=await run(segmentedCoupon(),code.replace("platingMaterial:'cu'","platingMaterial:'plating'")
  .replace('.materials({cu:',".materials({plating:{conductivitySM:58e6,state:'Explicit plating',validity:{status:'conditional',domain:{bandHz:[1000,100000]}}},cu:"));
 assert.match(JSON.stringify(badPlating.error),/does not include exact DC/);
 const ambiguous=segmentedCoupon();
 ambiguous.pads.push({...ambiguous.pads[0],id:'duplicate-pad'});
 const badContact=await run(ambiguous);
 assert.match(JSON.stringify(badContact.error),/resolves to 2 physical pads/);
 const supplied=await run(segmentedCoupon(),code.replace('.frequencyHz(0)',
  ".components([inductor({designator:'J0',pins:{positive:'1',negative:'2'},inductanceH:1e-6,ideal:true})]).frequencyHz(0)"));
 assert.equal(supplied.error?.code,'LOCAL_DC_COMPONENT_UNSUPPORTED');
 const interval=await run(segmentedCoupon(),code.replace('.frequencyHz(0)',".frequency({startHz:0,stopHz:1000,points:2,scale:'linear'})"));
 assert.equal(interval.status,'failed');
 assert.equal(calls.length,3);
});
