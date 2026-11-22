import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisLocalDcReport } from '../native/analyze/index.js';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createLocalExecutor } from '../src/analyze/analysis-dsl/local-executor.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import type { LocalDcExtractionProblem } from '../src/analyze/types.ts';
import { segmentedCoupon, segmentedCouponDsl } from './helpers/pcb-analysis-segmented-coupon.ts';

function componentCoupon() {
 const board=segmentedCoupon(),track=board.copper.fixed.tracks[0];
 board.copper.fixed.tracks.splice(0,1,...[[0,2.75],[3.25,6.75],[7.25,10]].map(([a,b],i)=>
  ({...track,id:'signal-'+i,points:[{x:a,y:0},{x:b,y:0}]})));
 for(const [name,a,b] of [['R1',2.75,3.25],['L1',6.75,7.25],['C1',2.25,3.75]] as const){
  board.components.push({...board.components[0],designator:name,at:{x:(a+b)/2,y:0}});
  for(const [i,x] of [a,b].entries())board.pads.push({...structuredClone(board.pads[0]),
   component:name,number:String(i+1),id:name+'-'+(i+1),at:{x,y:0}});
 }
 return board;
}

const resistor="resistor({designator:'R1',pins:{negative:'2',positive:'1'},resistanceOhm:2})";
const inductor="inductor({designator:'L1',pins:{positive:'1',negative:'2'},inductanceH:1e-6,seriesResistanceOhm:3})";
const capacitor="capacitor({designator:'C1',pins:{positive:'1',negative:'2'},capacitanceF:1e-6,esrOhm:.1,eslH:1e-9})";

test('native DC preserves a series R/DCR chain and an open capacitive bridge',async()=>{
 const execute=createLocalExecutor();
 const base=segmentedCouponDsl.split('a.plot(')[0].replace(".frequency({startHz:1000,stopHz:10000,points:3,scale:'log'})",'.frequencyHz(0)');
 for(const withResistor of [true,false]){
  let response:Awaited<ReturnType<typeof execute>>|undefined;
  const models=withResistor?[resistor,inductor,capacitor]:[inductor,capacitor];
  const code=base.replace('.frequencyHz(0)','.components(['+models.join(',')+']).frequencyHz(0)');
  const result=await executeScenario({board:componentCoupon(),code},
   {filename:'native-dc-components.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
   async task=>response=await execute(task as AnalysisTask<'impedance'>));
  assert.equal(result.status,'complete',JSON.stringify(result.error));
  const solution=response!.solution as any;
  assert.equal(solution.physicalPorts.length,2);
  assert.deepEqual(solution.portRealV,[null,null]);
  const cap=solution.componentDc.find((entry:any)=>entry.designator==='C1');
  assert.equal(cap.dc,'open');
  assert.equal(cap.currentA,0);
  assert.equal(cap.lossW,0);
  if(withResistor){
   const modal=solution.balancedPortResponse;
   assert.deepEqual(modal.basisDrivesA,[[-1,1]]);
   assert.equal(modal.powerByDrive[0].externalLoadLossW,0);
   assert.ok(Math.abs(modal.powerByDrive[0].packageLossW-5)<5e-8);
   assert.ok(Math.abs(modal.modalImpedanceOhm[0][0]/solution.drivePowerW-1)<1e-8);
   assert.ok(Math.abs(solution.packageLossW-5)<5e-8);
   assert.ok(solution.copperLossW>0);
   assert.ok(Math.abs((solution.packageLossW+solution.copperLossW)/solution.drivePowerW-1)<1e-8);
   assert.equal(new Set(solution.electrodeGalvanicComponents).size,4);
   assert.equal(new Set(solution.electrodeLoadedComponents).size,2);
  }else{
   assert.deepEqual(solution.balancedPortResponse.basisDrivesA,[]);
   assert.equal(solution.loopProbe,undefined);
   assert.equal(solution.packageLossW,0);
   assert.equal(solution.copperLossW,0);
   assert.equal(new Set(solution.electrodeLoadedComponents).size,3);
  }
 }
});

test('layered DC separates component load stamps and losses from external ports and copper',async()=>{
 const calls:LocalDcExtractionProblem[]=[];
 const execute=createLocalExecutor(((problem:LocalDcExtractionProblem):AnalysisLocalDcReport=>{
  calls.push(problem);
  assert.equal(problem.kind,'local-dc-extraction');
  const {drive,loadsOhm,geometry}=problem.request;
  assert.ok(loadsOhm);
  const g=geometry as {ports:[string,string][];electrodes:{name:string;sourcePath:string}[];allowInsulatedCopperCuts:boolean};
  assert.equal(g.allowInsulatedCopperCuts,false);
  assert.deepEqual(loadsOhm,drive?[null,null,2,3]:[null,null,3]);
  if(drive)assert.deepEqual(drive,[1,-1,0,0]);
  const privateNames=g.ports.slice(2).flat();
  assert.ok(privateNames.every(name=>!name.includes('"C1"')));
  assert.equal(g.electrodes.filter(e=>e.name.includes('"C1"')).length,2);
  if(drive)assert.deepEqual(g.ports[2].map(name=>g.electrodes.find(e=>e.name===name)?.sourcePath),['pads[4]','pads[5]']);
  const electrodeNames=g.electrodes.map(e=>e.name), count=electrodeNames.length;
  const copperIslandByPad=[0,2,3,3,0,1,1,2,0,1];
  const electrodeGalvanicComponents=g.electrodes.map(e=>copperIslandByPad[Number(e.sourcePath.slice(5,-1))]);
  const electrodeLoadedComponents=electrodeGalvanicComponents.map(i=>i===3?3:drive?0:Math.min(i,1));
  return {status:'complete',frequencyHz:0,ports:g.ports,driveA:drive??g.ports.map(()=>0),loadsOhm,
   portRealV:drive?[null,null,2,3]:[null,null,0],floatingPortIndices:[0,1],
   electrodeNames,electrodeVoltagesV:electrodeNames.map(()=>0),
   electrodeGalvanicComponents,electrodeLoadedComponents,
   dcConductanceS:Array.from({length:count},()=>Array(count).fill(0)),totalLossW:drive?1.5:0,loadLossW:drive?5:0,
   drivePowerW:drive?6.5:0,normalizedElectrodeResidual:0,roiMm:[[0,0],[10,1]],domainPolygonMm:null,
   allowInsulatedCopperCuts:false,sourceMapping:[],validity:{status:'conditional',domain:{frequencyHz:0},reason:'Finite copper'},
  };
 }) as NonNullable<Parameters<typeof createLocalExecutor>[0]>);
 const base=segmentedCouponDsl.split('a.plot(')[0].replace(".frequency({startHz:1000,stopHz:10000,points:3,scale:'log'})",".frequencyHz(0)");
 const run=async(models:string[])=>executeScenario(
  {board:componentCoupon(),code:base.replace('.frequencyHz(0)','.components(['+models.join(',')+']).frequencyHz(0)')},
  {filename:'layered-dc-components.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
  async task=>{
   const result=await execute(task as AnalysisTask<'impedance'>);
   const solution=result.solution as any;
   assert.equal(solution.physicalPorts.length,2);
   assert.equal(solution.nativePortMapping.externalPortCount,2);
   assert.deepEqual(solution.portRealV,[null,null]);
   assert.deepEqual(solution.loadsOhm,[null,null]);
   assert.equal(solution.dcConductanceScope,'Copper electrode Schur matrix before package load stamps');
   assert.equal(solution.componentDc.find((c:any)=>c.designator==='C1').dc,'open');
   assert.equal(solution.componentDc.find((c:any)=>c.designator==='C1').lossW,0);
   if(models.includes(resistor)){
    assert.equal(solution.loopProbe.resistanceOhm,6.5);
    assert.equal(solution.copperLossW,1.5);
    assert.equal(solution.packageLossW,5);
    const packageLosses=solution.componentDc.filter((c:any)=>c.dc==='resistive').map((c:any)=>c.lossW);
    assert.ok(Math.abs(packageLosses[0]-2)<1e-12&&Math.abs(packageLosses[1]-3)<1e-12);
    assert.equal(new Set(solution.electrodeGalvanicComponents).size,4);
    assert.equal(new Set(solution.electrodeLoadedComponents).size,2);
   }else{
    assert.equal(solution.loopProbe,undefined);
    assert.equal(solution.packageLossW,0);
   }
   return result;
  });
 const loaded=await run([resistor,inductor,capacitor]);
 assert.equal(loaded.status,'complete',JSON.stringify(loaded.error));
 const open=await run([inductor,capacitor]);
 assert.equal(open.status,'complete',JSON.stringify(open.error));
 assert.equal(calls[1].request.drive,undefined);
 const ideal=await run([inductor.replace('seriesResistanceOhm:3','ideal:true'),capacitor]);
 assert.equal(ideal.error?.code,'LOCAL_DC_COMPONENT_UNSUPPORTED');
 assert.equal(calls.length,2);
});
