import test from 'node:test';
import assert from 'node:assert/strict';
import { boardFoilResult } from '../src/analyze/analysis-dsl/board-foil-result.ts';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';
import { createBoardChannelExecutor } from '../src/analyze/board-channel-executor.ts';
import type { createBoardChannelPlanner } from '../src/analyze/board-channel-plan.ts';
import { runAutoFoil } from '../src/analyze/auto-foil-response.ts';
import { channelPassiveElement } from '../src/analyze/channel-components.ts';
import type { BoardFoilProblem, BoardFoilReport } from '../src/analyze/types.ts';

const budget={maxSeconds:30,maxMemoryMiB:256};
const plan:AutoFoilPlan={
 boardJson:'{}',roiMm:[[0,0],[1,1]],geometry:{electrodes:[
  {name:'a',sourcePath:'pads[0]',layer:'TOP',upper:false},
  {name:'b',sourcePath:'pads[1]',layer:'BOTTOM',upper:true},
 ]},
 terminals:[{id:'a',sourcePath:'pads[0]',layer:'TOP'},{id:'b',sourcePath:'pads[1]',layer:'BOTTOM'}],
 ports:[{name:'input',positive:'a',negative:'b'}],electrodePairs:[['a','b']],
 sourceCopper:[{regionId:'top',sourcePath:'pads[0]',layer:'TOP'}],sourceBarrels:[],
 attachments:[{id:'load',electrodes:['a','b'],model:channelPassiveElement('load',{kind:'resistor',resistanceOhm:10},[1500,7500])}],
};

function report(problem:BoardFoilProblem):BoardFoilReport{
 const {frequenciesHz,referenceOhm,budget}=problem.request;
 const partitionPolicy=problem.request.partitionPolicy;
 return {
  status:'complete',coverage:'unverified',representation:'S',frequenciesHz,referenceOhm:referenceOhm!,
  ports:problem.expected.electrodePairs,
  parameters:frequenciesHz.map(f=>{
   const phase=f===2250?.025:0;
   return [[{real:.5*Math.cos(phase),imaginary:.5*Math.sin(phase)}]];
  }),enrichment:[],
  samples:frequenciesHz.map(frequencyHz=>({frequencyHz,columns:[{input:0,kclRelative:0,kvlRelative:0,kvlUncertaintyRelative:0,
   copperLossW:8.875,powerBalanceErrorW:0,packages:[{id:'load',currentA:{real:Math.sqrt(.05),imaginary:0},lossW:.5}]}]})),
  quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0,maxFullKvlUncertaintyRelative:0},
  provenance:{
   ...(partitionPolicy&&{partitionPolicy,automaticPartition:{policy:partitionPolicy,status:'not-applicable' as const,
    reason:'Tiny mock has no private owner interiors',algorithm:'longest-extent-midpoint-v1' as const,
    meshOwnershipSignature:'fnv1a64:0123456789abcdef',owners:2,originalCopperTetrahedra:3,
    crossOwnerFaces:1,freeSharedTraceDofs:0,ownerInteriorDofs:[0,0]}}),
    magneticOperator:{policy:'exact-streamed-v1',method:'unchanged direct tetrahedral pairs',scope:'Exact numerical pair action; physical accuracy unverified',maxFullKvlUncertaintyRelative:0},
   method:'mock',modelKind:'mixed',barrelIds:[],sourceMapping:[['top','pads[0]',null,'TOP']],electrodeAreasM2:{a:1e-8,b:1e-8},
   maxCopperVolumeChange:0,prisms:0,nativeCopperTetrahedra:3,volumeTetrahedra:3,fullPotentialCount:4,fullCurrentCount:9,
   retainedPotentials:2,currentModes:2,plannedBytes:1024,...budget,magneticQuadratureCoverage:'unverified',
   spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1 A RMS Norton',
   attachments:[{electrodes:['a','b'],id:'load'}],
  },
 };
}

test('joint range runs one bounded same-geometry batch and preserves exact replay, package scope and full demand',()=>{
 const calls:BoardFoilProblem[]=[];
 let failBatch=false;
 const execute=(problem:BoardFoilProblem)=>{
  calls.push(problem);
  if(failBatch&&calls.length>1)throw Error('MEMORY_LIMIT: supplemental batch refused');
  return report(problem);
 };
 const exact=runAutoFoil(plan,[1500],budget,[1500,1500],execute);
 assert.equal(calls.length,1);
 assert.equal(exact.frequencyAudit?.batch.status,'not-attempted');
 assert.deepEqual(exact.frequencyAudit?.interpolation.validatedIntervalsHz,[[1500,1500]]);
 calls.length=0;
 const demand=[0,12e9] as const;
 const range=runAutoFoil(plan,[1500,7500],budget,demand,execute);
 assert.equal(calls.length,2);
 const [base,batch]=calls;
 assert.deepEqual(batch.request.frequenciesHz,[2250,3000,3750,4500,5250,6000,6750]);
 assert.deepEqual(batch.expected,base.expected);
 assert.deepEqual(batch.geometry,base.geometry);
 assert.equal(batch.boardJson,base.boardJson);
 assert.deepEqual(batch.request.attachments,base.request.attachments);
 assert.deepEqual(batch.request.referenceOhm,base.request.referenceOhm);
 assert.equal(batch.request.budget.maxMemoryMiB,budget.maxMemoryMiB);
 assert.ok(batch.request.budget.maxSeconds<budget.maxSeconds);
 assert.deepEqual(range.frequenciesHz,[1500,7500]);
 const audit=range.frequencyAudit!;
 assert.deepEqual(audit.interpolation.requestedIntervalHz,demand);
 assert.deepEqual(audit.applicabilityBandHz,[1500,7500]);
 assert.equal(audit.physicalCoverage.status,'unverified');
 assert.equal(audit.batch.status,'checked');
 assert.ok(audit.interpolation.unverified.some(gap=>gap.intervalHz[0]===0&&gap.reason==='unsupported'));
 assert.ok(audit.interpolation.unverified.some(gap=>gap.intervalHz[1]===12e9&&gap.reason==='unsupported'));
 const held=audit.interpolation.holdouts.find(h=>h.frequencyHz===2250)!;
 assert.equal(held.status,'requires-review');
 assert.equal(held.accepted,false);
 assert.equal(held.promoted,true);
 assert.ok(audit.interpolation.unverified.some(gap=>gap.reason==='sample-budget'));
 assert.deepEqual(audit.interpolation.samples.map(s=>s.frequencyHz),batch.request.frequenciesHz);
 const result=boardFoilResult('ctx',plan,range,[]);
 assert.deepEqual((result.network?.data as {frequenciesHz:number[]}).frequenciesHz,[1500,7500]);
 assert.equal((result.network?.validity?.domain as {bandCoverage:string}).bandCoverage,'unverified');
 assert.ok(result.diagnostics.some(d=>d.code==='CHANNEL_FOIL_FREQUENCY_REVIEW'));
 assert.ok(result.diagnostics.some(d=>d.code==='CHANNEL_FOIL_FREQUENCY_UNVERIFIED'));

 calls.length=0;
 failBatch=true;
 const refused=runAutoFoil(plan,[1500,7500],budget,demand,execute);
 assert.equal(calls.length,2);
 assert.equal(refused.frequencyAudit?.batch.status,'refused');
 assert.ok(refused.frequencyAudit?.interpolation.unverified.some(gap=>gap.reason==='physical-evaluation'));
 assert.deepEqual(refused.frequenciesHz,range.frequenciesHz);
});


test('high board demand requires closed joint geometry and freezes the admitted profile across the audit batch',()=>{
 const highPlan:AutoFoilPlan={...plan,geometry:{...plan.geometry,stack:[
  {name:'TOP',copper:true,conductivitySM:58e6},{name:'BOTTOM',copper:true,conductivitySM:58e6},
 ]},attachments:[{...plan.attachments[0],model:channelPassiveElement('load',{kind:'resistor',resistanceOhm:10},[1500,75000])}]};
 const calls:BoardFoilProblem[]=[];
 let changedMesh=false, refuseClosure=false, closedCalls=0;
 const execute=(problem:BoardFoilProblem)=>{
  calls.push(problem);
  const baseline=report(problem);
  const {operatorPolicy,profileUpperHz,frequenciesHz}=problem.request;
  assert.equal(operatorPolicy,'skin-volume-v3');
  assert.equal(profileUpperHz,75000);
  const maximum=frequenciesHz.at(-1)!;
  const skinDepthM=1/(Math.sqrt(Math.PI*(4e-7*Math.PI)*profileUpperHz!)*Math.sqrt(58e6));
  assert.equal(baseline.provenance.modelKind,'mixed');
  Object.assign(baseline.provenance,{
   operatorPolicy,copperSkinMesh:{
    version:3,requestedUpperHz:maximum,trainingUpperBoundHz:maximum,effectiveUpperHz:profileUpperHz,
    skinDepthDivisor:3,currentAndElectricShareMesh:true,scope:'shared mesh',
    layers:['TOP','BOTTOM'].map(layer=>({layer,skinDepthM,maximumStepM:skinDepthM/4,slabs:1})),roundBarrels:[],
   },
   ...(changedMesh&&calls.length>1&&{nativeCopperTetrahedra:4,volumeTetrahedra:4,fullCurrentCount:12}),
  });
  if(changedMesh&&calls.length>1)baseline.provenance.automaticPartition!.originalCopperTetrahedra=4;
  return baseline;
 };
 const createPlanner=(()=>(options:unknown)=>{
  assert.ok(options&&typeof options==='object'&&'closedJointDeadlineMs' in options);
  closedCalls++;
  if(refuseClosure)throw Error('CHANNEL_JOINT_CLOSURE: unsupported physical closure');
  return highPlan;
 }) as unknown as typeof createBoardChannelPlanner;
 const run=createBoardChannelExecutor(createPlanner,()=>assert.fail('unexpected segmented solve'),
  (foil,axis,remaining,interval)=>runAutoFoil(foil,axis,remaining,interval,execute));
 const demand=[0,12e9] as const;
 const attachment=highPlan.attachments[0];
 const contacts={signals:[],references:[],ports:[],components:[{
  id:attachment.id,designator:'R1',pins:[highPlan.terminals[0],highPlan.terminals[1]] as const,model:attachment.model,
 }]};
 const result=run({}, {}, contacts, [20000,60000], budget, demand);
 assert.ok('coverage' in result&&result.frequencyAudit?.batch.status==='checked');
 assert.equal(closedCalls,1);
 assert.equal(calls.length,2);
 const [base,batch]=calls;
 assert.equal(base.request.profileUpperHz,batch.request.profileUpperHz);
 assert.deepEqual(base.request.frequenciesHz,[20000,60000]);
 assert.ok(batch.request.frequenciesHz.length<=8);
 assert.deepEqual(batch.geometry,base.geometry);
 assert.deepEqual(batch.expected,base.expected);
 assert.deepEqual(result.frequencyAudit?.applicabilityBandHz,[1500,75000]);
 assert.deepEqual(result.frequencyAudit?.interpolation.requestedIntervalHz,demand);
 assert.ok(result.frequencyAudit?.interpolation.unverified.some(gap=>gap.intervalHz[0]===0));
 assert.ok(result.frequencyAudit?.interpolation.unverified.some(gap=>gap.intervalHz[1]===12e9));
 assert.equal(result.coverage,'unverified');

 calls.length=0;
 changedMesh=true;
 const changed=run({}, {}, contacts, [20000,60000], budget, demand);
 assert.ok('coverage' in changed&&changed.frequencyAudit?.batch.status==='refused');
 assert.match(changed.frequencyAudit!.batch.message??'',/CHANNEL_FOIL_FREQUENCY_MESH/);
 assert.deepEqual(changed.frequenciesHz,[20000,60000]);
 assert.equal(calls.length,2);
 refuseClosure=true;
 assert.throws(()=>run({}, {}, contacts, [20000,60000], budget, demand),/CHANNEL_JOINT_CLOSURE/);
 assert.equal(calls.length,2);
});
