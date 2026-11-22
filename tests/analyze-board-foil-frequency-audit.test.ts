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
 boardJson:'{}',roiMm:[[0,0],[1,1]],geometry:{stack:[{name:'TOP',copper:true,conductivitySM:58e6},{name:'BOTTOM',copper:true,conductivitySM:58e6}],electrodes:[
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
 const result:BoardFoilReport = {
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
 Object.assign(result,{operator:'compact-planar-layered-prototype'});
 delete result.quality.maxFullKvlUncertaintyRelative;
 Object.assign(result.quality,{scope:'compact-discrete',physicalError:'unverified',maxCompactKvlUncertaintyRelative:0});
 for(const sample of result.samples as unknown as Array<{columns:Array<Record<string,unknown>>}>) for(const column of sample.columns){
  delete column.kvlUncertaintyRelative; column.compactKvlUncertaintyRelative=0;
  column.compactMagneticCertificate={policy:'compact-mixed-near-far',nearQuadratureErrorCertified:false};
 }
 Object.assign(result.provenance,{automaticPartition:undefined,magneticOperator:undefined,
  compactOperator:{version:'compact-planar-v1',basis:'adaptive-mixed-closure',electricOperator:'layered-spatial-field-mesh',foilSupports:0,localTetraSupports:3,potentialCoordinates:4,currentCoordinates:9,approximationError:'unverified',scope:'compact-discrete',originalVolumeActions:0,physicalAccuracy:'unverified',planarMaxOrder:0,planarMaxRelativeChange:0,mixedMaxRelativeChange:0,tetraSelfMaxRelativeChange:0,tetraPairAdaptiveTolerance:0,nearQuadratureEstimatedTolerance:1e-8},
  hybridAssembly:{policy:'automatic-hybrid-v1',method:'global-compact-mixed',potentialCoordinates:4,currentCoordinates:9,physicalAccuracy:'unverified'}});
 if(problem.request.operatorPolicy==='hybrid-planar-v3'){
  // The auto-foil request always selects the hybrid planar policy, so the validator requires the shared skin-mesh record.
  const top=frequenciesHz.at(-1)!,upper=problem.request.profileUpperHz??top;
  const stack=((problem.geometry as {stack?:{name:string;copper:boolean;conductivitySM?:number}[]}).stack??[]).filter(l=>l.copper);
  const depth=(c:number)=>1/Math.sqrt(Math.PI*upper*(4e-7*Math.PI)*c);
  Object.assign(result.provenance,{operatorPolicy:'hybrid-planar-v3',copperSkinMesh:{version:3,requestedUpperHz:top,trainingUpperBoundHz:top,effectiveUpperHz:upper,
   skinDepthDivisor:3,currentAndElectricShareMesh:true,scope:'shared mesh',
   layers:stack.map(l=>({layer:l.name,skinDepthM:depth(l.conductivitySM!),maximumStepM:depth(l.conductivitySM!)/6,slabs:1})),roundBarrels:[]}});
 }
 return result;
}

test('joint range runs one bounded same-geometry batch and preserves exact replay, package scope and full demand',()=>{
 // This test checks the separately solved supplemental batch; the default one-pass union solve is covered by the high-board test.
 const previousSeparate=process.env.P7_AUDIT_SEPARATE_SOLVE;
 process.env.P7_AUDIT_SEPARATE_SOLVE='1';
 try{
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
 assert.equal(batch.request.budget.maxSeconds,budget.maxSeconds);
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
 assert.deepEqual(audit.adaptiveResponse.points.map(p=>p.frequencyHz),[1500,...batch.request.frequenciesHz,7500]);
 assert.equal(audit.adaptiveResponse.physicalCoverage.status,'unverified');
 assert.ok(audit.adaptiveResponse.points.find(p=>p.frequencyHz===2250)?.samples.columns[0].copperLossW===8.875);
 assert.ok(audit.adaptiveResponse.independentHoldoutFrequenciesHz.some(h=>h.frequencyHz===2250&&h.status==='requires-review'));
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
 assert.deepEqual(refused.frequencyAudit?.adaptiveResponse.points.map(p=>p.frequencyHz),[1500,7500]);
 }finally{
  if(previousSeparate===undefined)delete process.env.P7_AUDIT_SEPARATE_SOLVE;else process.env.P7_AUDIT_SEPARATE_SOLVE=previousSeparate;
 }
});

test('frequency audit rejects valid compact basis changes but permits quadrature statistics changes',()=>{
 const previous=process.env.P7_COMPACT_PLANAR;
 process.env.P7_COMPACT_PLANAR='1';
 const previousSeparate=process.env.P7_AUDIT_SEPARATE_SOLVE;
 process.env.P7_AUDIT_SEPARATE_SOLVE='1'; // basis changes are compared between separately solved batches
 const calls:BoardFoilProblem[]=[];
 const run=(change:'basis'|'quadrature')=>{
  calls.length=0;
  const execute=(problem:BoardFoilProblem)=>{
   calls.push(problem);const value=report(problem) as BoardFoilReport & {operator?:string};
   const changed=calls.length>1;
   Object.assign(value,{operator:'compact-planar-layered-prototype'});
   delete value.quality.maxFullKvlUncertaintyRelative;
   Object.assign(value.quality,{scope:'compact-discrete',physicalError:'unverified',maxCompactKvlUncertaintyRelative:0});
   for(const sample of value.samples as unknown as Array<{columns:Array<Record<string,unknown>>}>) for(const column of sample.columns){
    delete column.kvlUncertaintyRelative;
    column.compactKvlUncertaintyRelative=0;
    column.compactMagneticCertificate={policy:'compact-mixed-near-far',nearQuadratureErrorCertified:false};
   }
   const potentialCoordinates=changed&&change==='basis'?3:4;
   Object.assign(value.provenance,{
    partitionPolicy:'automatic-hybrid-v1',automaticPartition:undefined,magneticOperator:undefined,
    compactOperator:{version:'compact-planar-v1',basis:'adaptive-mixed-closure',electricOperator:'layered-spatial-field-mesh',
     foilSupports:0,localTetraSupports:3,potentialCoordinates,currentCoordinates:9,approximationError:'unverified',scope:'compact-discrete',
     originalVolumeActions:0,physicalAccuracy:'unverified',planarMaxOrder:0,planarMaxRelativeChange:0,
     mixedMaxRelativeChange:0,tetraSelfMaxRelativeChange:0,tetraPairAdaptiveTolerance:0,nearQuadratureEstimatedTolerance:1e-8,
     skinHints:{added:changed&&change==='quadrature'?7:1}},
    hybridAssembly:{policy:'automatic-hybrid-v1',method:'global-compact-mixed',potentialCoordinates,currentCoordinates:9,physicalAccuracy:'unverified'},
   });
   return value;
  };
  return runAutoFoil(plan,[1500,7500],budget,[1500,7500],execute);
 };
 try{
  const changed=run('basis');
  assert.equal(changed.frequencyAudit?.batch.status,'refused');
  assert.deepEqual(changed.frequencyAudit?.adaptiveResponse.points.map(p=>p.frequencyHz),[1500,7500]);
  const quadrature=run('quadrature');
  assert.equal(quadrature.frequencyAudit?.batch.status,'checked');
  assert.ok(quadrature.frequencyAudit?.adaptiveResponse.points.length>2);
 }finally{if(previous===undefined)delete process.env.P7_COMPACT_PLANAR;else process.env.P7_COMPACT_PLANAR=previous;
  if(previousSeparate===undefined)delete process.env.P7_AUDIT_SEPARATE_SOLVE;else process.env.P7_AUDIT_SEPARATE_SOLVE=previousSeparate;}
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
  assert.equal(operatorPolicy,'hybrid-planar-v3');
  assert.equal(profileUpperHz,75000);
  const maximum=frequenciesHz.at(-1)!;
  const skinDepthM=1/(Math.sqrt(Math.PI*(4e-7*Math.PI)*profileUpperHz!)*Math.sqrt(58e6));
  assert.equal(baseline.provenance.modelKind,'mixed');
  Object.assign(baseline.provenance,{
   operatorPolicy,copperSkinMesh:{
    version:3,requestedUpperHz:maximum,trainingUpperBoundHz:maximum,effectiveUpperHz:profileUpperHz,
    skinDepthDivisor:3,currentAndElectricShareMesh:true,scope:'shared mesh',
    layers:['TOP','BOTTOM'].map(layer=>({layer,skinDepthM,maximumStepM:skinDepthM/6,slabs:1})),roundBarrels:[],
   },
   ...(changedMesh&&calls.length>1&&{nativeCopperTetrahedra:4,volumeTetrahedra:4,fullCurrentCount:12}),
  });

  return baseline;
 };
 const createPlanner=(()=>(options:unknown)=>{
  if(options&&typeof options==='object'&&!('closedJointDeadlineMs' in options))return {blocks:[{bandHz:[1000,10000]}]};
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
 assert.equal(calls.length,1);
 const [base]=calls;
 assert.deepEqual(base.request.frequenciesHz.filter(f=>f===20000||f===60000),[20000,60000]);
 assert.ok(base.request.frequenciesHz.length<=2+8);
 assert.deepEqual(result.frequencyAudit?.applicabilityBandHz,[1500,75000]);
 assert.deepEqual(result.frequencyAudit?.interpolation.requestedIntervalHz,demand);
 assert.ok(result.frequencyAudit?.interpolation.unverified.some(gap=>gap.intervalHz[0]===0));
 assert.ok(result.frequencyAudit?.interpolation.unverified.some(gap=>gap.intervalHz[1]===12e9));
 assert.equal(result.coverage,'unverified');

 calls.length=0;
 changedMesh=true;
 // The mesh-identity guard only applies to separately solved supplemental batches.
 process.env.P7_AUDIT_SEPARATE_SOLVE='1';
 const changed=run({}, {}, contacts, [20000,60000], budget, demand);
 assert.ok('coverage' in changed&&changed.frequencyAudit?.batch.status==='refused');
 assert.match(changed.frequencyAudit!.batch.message??'',/CHANNEL_FOIL_EVIDENCE/);
 assert.deepEqual(changed.frequenciesHz,[20000,60000]);
 assert.equal(calls.length,2);
 delete process.env.P7_AUDIT_SEPARATE_SOLVE;
 refuseClosure=true;
 assert.throws(()=>run({}, {}, contacts, [20000,60000], budget, demand),/CHANNEL_JOINT_CLOSURE/);
 assert.equal(calls.length,2);
});
