import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../native/analyze/index.js';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import type { CoupledRlcData } from '../src/analyze/analysis-dsl/spec-doc.js';
import { createFrequencyExecutor } from '../src/analyze/analysis-dsl/frequency-executor.ts';
import { createTransientExecutor } from '../src/analyze/analysis-dsl/transient-executor.ts';
import { createLocalExecutor } from '../src/analyze/analysis-dsl/local-executor.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { capturedBoardGeometry } from '../src/analyze/geometry-snapshot.ts';
import { isAnalysisProblem, type BoardFoilProblem } from '../src/analyze/types.ts';
import { finitePlaneBend, finitePlaneBendDsl as source } from './helpers/pcb-analysis-finite-plane-bend.ts';
import { createHash } from 'node:crypto';
import { mkdtempSync,rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { cachedBoardFoilReport } from '../src/analyze/board-foil-cache.ts';
import { LocalModelCache } from '../src/analyze/local-model-cache.ts';

type Geometry={allowInsulatedCopperCuts:boolean;roiMm:[[number,number],[number,number]];ports:[string,string][];cutPlanes:unknown;electrodes:{name:string;sourcePath:string}[]};

function checkedMock(problem:BoardFoilProblem, sourceMapping:AnalysisBoardFoilCheckedReport['provenance']['sourceMapping'],
 barrelIds:string[]=[]):AnalysisBoardFoilCheckedReport{
 const g=problem.geometry as Geometry;
 const {frequenciesHz,referenceOhm,budget}=problem.request;
 const s=[[{real:.2,imaginary:0},{real:.5,imaginary:0}],[{real:.5,imaginary:0},{real:.2,imaginary:0}]];
 const mixed=Boolean(barrelIds.length||problem.request.attachments?.length||problem.request.partitionPolicy);
 const partitionPolicy=problem.request.partitionPolicy;
 const provenanceModel=mixed?{
  modelKind:'mixed' as const,nativeCopperTetrahedra:28,volumeTetrahedra:28,
  fullPotentialCount:4,fullCurrentCount:84,magneticQuadratureCoverage:'unverified' as const,
 }:{modelKind:'foil' as const,maxMagneticReciprocity:0,maxMagneticRefinement:0,maxElectricReciprocity:0};
 return {status:'complete',coverage:'unverified',representation:'S',frequenciesHz,referenceOhm:referenceOhm!,ports:g.ports,
   parameters:frequenciesHz.map(()=>s),enrichment:[],
   samples:frequenciesHz.map(frequencyHz=>({frequencyHz,columns:[0,1].map(input=>({input,kclRelative:0,kvlRelative:0,copperLossW:8.875,powerBalanceErrorW:0}))})),
   quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0},
   ...(partitionPolicy&&{conductiveCorrection:{
    method:'sparse owner interiors and shared conductive trace Schur' as const,
    ownerInteriorDofs:[1,1],freeSharedTraceDofs:1,traceUpperEntries:1,ownedPlannedBytes:128,
    maxOriginalFreeResidualRelative:0,scope:'Original-coordinate conductive correction',
   }}),
   provenance:{method:mixed?'Mock mixed foil and volume':'Mock checked foil',...provenanceModel,barrelIds,sourceMapping,
    ...(partitionPolicy&&{partitionPolicy,automaticPartition:{
     policy:partitionPolicy,status:'selected' as const,reason:'Original mesh partition',
     algorithm:'longest-extent-midpoint-v1' as const,meshOwnershipSignature:'fnv1a64:0123456789abcdef',
     owners:2,originalCopperTetrahedra:28,crossOwnerFaces:1,freeSharedTraceDofs:1,ownerInteriorDofs:[1,1],
    }}),
    electrodeAreasM2:Object.fromEntries(g.electrodes.map(e=>[e.name,1e-8])),maxCopperVolumeChange:0,prisms:mixed?0:28,
    retainedPotentials:4,currentModes:6,plannedBytes:1024,maxSeconds:budget.maxSeconds,maxMemoryMiB:budget.maxMemoryMiB,
    spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1A RMS Norton'},
  };
}

test('finite lateral return triggers the joint foil producer and exact physical S replay',async()=>{
 const board=finitePlaneBend();
 const foilCalls:BoardFoilProblem[]=[];
 let mutateReport:((report:AnalysisBoardFoilCheckedReport)=>void)|undefined;
 let producer:{boardSha256:string;geometrySha256:string}|undefined;
 const local=createLocalExecutor(((problem:BoardFoilProblem):AnalysisBoardFoilCheckedReport=>{
  assert.equal(problem.kind,'board-foil-checked');
  assert.equal(problem.request.partitionPolicy,'automatic-conductive-schur-v1');
  assert.ok(isAnalysisProblem(problem));
  foilCalls.push(problem);
  const g=problem.geometry as Geometry;
  assert.equal(g.allowInsulatedCopperCuts,false);
  assert.deepEqual(g.cutPlanes,{});
  assert.deepEqual(g.ports.map(pair=>pair.map(name=>g.electrodes.find(e=>e.name===name)?.sourcePath)),[['pads[0]','pads[2]'],['pads[1]','pads[3]']]);
  const {frequenciesHz,referenceOhm}=problem.request;
  assert.deepEqual(frequenciesHz,frequenciesHz.length===1?[2000]:[1000,3000]);
  assert.deepEqual(referenceOhm,[50,50]);
  const captured=capturedBoardGeometry(problem.boardJson);
  assert.ok(captured.regions.some(region=>region.source.path.includes('zones[0]')));
  const report=checkedMock(problem,captured.regions.map(region=>[region.id,region.source.path,region.source.net,region.layer]));
  mutateReport?.(report);
  return report;
 }) as NonNullable<Parameters<typeof createLocalExecutor>[0]>);
 const frequency=createFrequencyExecutor();
 const transient=createTransientExecutor();
 const run=async(code:string)=>executeScenario({board,code},
  {filename:'automatic-foil.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
  async task=>{
   if(task.kind==='impedance'){
    const result=await local(task as AnalysisTask<'impedance'>);
    assert.equal(result.status,'partial');
    const model=result.network as any;
    assert.equal(model.data.format,'physical-port-samples');
    assert.equal(model.data.referenceTopology,'physical-pairs');
    assert.deepEqual(model.ports,['input','output']);
    assert.equal(model.validity.domain.bandCoverage,'unverified');
    assert.equal(model.validity.domain.bandHz,undefined);
    const problem=foilCalls.at(-1)!;
    producer={
     boardSha256:createHash('sha256').update(problem.boardJson).digest('hex'),
     geometrySha256:createHash('sha256').update(JSON.stringify(problem.geometry)).digest('hex'),
    };
    assert.deepEqual(model.validity.domain.producer,producer);
    assert.deepEqual((result.solution as any).foilPlan.producer,producer);
    return result;
   }
   if(task.kind==='si')return transient(task as AnalysisTask<'si'>);
   const result=await frequency(task as AnalysisTask<'networkParameters'>);
   const resource=result.network as any;
   assert.equal(resource.data.format,'physical-port-samples');
   assert.equal(resource.data.referenceTopology,'physical-pairs');
   assert.deepEqual(resource.data.frequenciesHz,[1000]);
   assert.equal(resource.data.physicalPorts[0].negative.sourcePath,'pads[2]');
   assert.equal(resource.validity.domain.bandHz,undefined);
   assert.deepEqual(resource.validity.domain.producer,producer);
   assert.match(resource.validity.domain.sourceValiditySha256,/^[a-f0-9]{64}$/);
   assert.equal(resource.validity.domain.sourceValidity,undefined);
   const solution=result.solution as any;
   assert.deepEqual(solution.topology,{referenceTopology:'physical-pairs',connections:[]});
   assert.equal(solution.validityLimitations,undefined);
   return result;
  });
 const replay="await a.networkParameters().network(result.network).representation('S').referenceOhm(50).frequencyHz(1000).run();";
 const accepted=await run(source+replay);
 assert.equal(accepted.status,'complete',JSON.stringify(accepted.error));
 assert.deepEqual(foilCalls.map(call=>call.request.frequenciesHz),[[1000,3000],[2000]]);
 const offGrid=await run(source+replay.replace('frequencyHz(1000)','frequencyHz(2000)'));
 assert.equal(offGrid.error?.code,'NETWORK_EXACT_GRID');
 const joined="const joined=a.connect({networks:[result.network],connections:[],ports:{input:result.network.port('input'),output:result.network.port('output')}});";
 const joinedReplay=await run(source+joined+replay.replace('network(result.network)','network(joined)'));
 assert.equal(joinedReplay.error?.code,'NETWORK_PHYSICAL_SAMPLE_TOPOLOGY');
 const changedReference=await run(source+replay.replace('referenceOhm(50)','referenceOhm(75)'));
 assert.equal(changedReference.error?.code,'NETWORK_PHYSICAL_SAMPLE_REFERENCE');
 const temporal=await run(source+"await a.si().network(result.network).initial('steady').time({stopS:1e-3,sampleStepS:1e-4}).run();");
 assert.equal(temporal.error?.code,'NETWORK_PHYSICAL_SAMPLE_TOPOLOGY');
 const invalidEvidence:((report:AnalysisBoardFoilCheckedReport)=>void)[]=[
  report=>{delete report.provenance.automaticPartition;},
  report=>{report.provenance.automaticPartition!.originalCopperTetrahedra++;},
  report=>{report.conductiveCorrection!.maxOriginalFreeResidualRelative=2e-8;},
  report=>{report.conductiveCorrection!.ownerInteriorDofs=[0,0];},
  report=>{report.samples[0].columns[0].kclRelative=2e-8;},
  report=>{report.samples[0].columns[0].kvlRelative=2e-8;},
  report=>{report.samples[0].columns[0].powerBalanceErrorW=-1e-15;},
  report=>{report.samples[0].columns[0].powerBalanceErrorW=1e-6;},
  report=>{report.samples[0].columns[0].copperLossW=0;},
  report=>{report.quality.maxKclRelative=1e-9;},
 ];
 for(const invalidate of invalidEvidence){
  mutateReport=invalidate;
  const rejected=await run(source);
  assert.match(rejected.error?.message??'',/CHANNEL_FOIL_EVIDENCE/);
 }
});
test('closed lateral candidate retains local barrel provenance and ignores a distant via',async()=>{
 const board=finitePlaneBend();
 for(const point of board.outline)if(point.x>1)point.x=5;
 const via=(id:string,x:number)=>({id,net:'RETURN',at:{x,y:.55},diameterMm:.3,drillMm:.15,type:'through',fromLayer:'TOP',toLayer:'BOTTOM'});
 board.copper.fixed.vias=[via('local-return-via',.55),via('remote-return-via',4)];
 let calls=0,omitBarrel=false,mislabelModel=false;
 const local=createLocalExecutor(((problem:BoardFoilProblem):AnalysisBoardFoilCheckedReport=>{
  assert.equal(problem.kind,'board-foil-checked');
  calls++;
  const geometry=problem.geometry as Geometry;
  assert.equal(geometry.allowInsulatedCopperCuts,false);
  assert.deepEqual(geometry.cutPlanes,{});
  assert.ok(geometry.roiMm[1][0]<3);
  const captured=capturedBoardGeometry(problem.boardJson);
  const [near,far]=captured.holes;
  assert.deepEqual(captured.holes.map(hole=>hole.source.source_id),['local-return-via','remote-return-via']);
  assert.ok(near.plated&&near.span_confirmed&&far.plated&&far.span_confirmed);
  const sourceMapping:AnalysisBoardFoilCheckedReport['provenance']['sourceMapping']=captured.regions
   .filter(region=>region.source.source_id!=='remote-return-via')
   .map(region=>[region.id,region.source.path,region.source.net,region.layer]);
  const report=checkedMock(problem,sourceMapping,[near.id]);
  if(omitBarrel)report.provenance.barrelIds=[];
  if(mislabelModel)Object.assign(report.provenance,{modelKind:'foil',prisms:28});
  return report;
 }) as NonNullable<Parameters<typeof createLocalExecutor>[0]>);
 const run=()=>executeScenario({board,code:source},
  {filename:'automatic-mixed.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
  async task=>{
   const result=await local(task as AnalysisTask<'impedance'>);
   const solution=result.solution as any;
   const near=capturedBoardGeometry(JSON.stringify(board)).holes[0];
   assert.equal(result.status,'partial');
   assert.match(result.method,/mixed foil and volume/);
   assert.equal(result.network?.validity?.status,'conditional');
   assert.match(result.network?.validity?.reason??'',/volumetric copper/);
   assert.deepEqual(solution.foilPlan.sourceBarrels,[{barrelId:near.id,sourcePath:near.source.path,layers:near.layers}]);
   assert.equal(solution.provenance.modelKind,'mixed');
   return result;
  });
 const accepted=await run();
 assert.equal(accepted.status,'complete',JSON.stringify(accepted.error));
 omitBarrel=true;
 assert.equal((await run()).error?.code,'CHANNEL_FOIL_EVIDENCE');
 omitBarrel=false;
 mislabelModel=true;
 assert.equal((await run()).error?.code,'CHANNEL_FOIL_EVIDENCE');
 assert.equal(calls,4);
 board.holes.push({id:'unconfirmed-span',at:{x:.8,y:.8},hole:{shape:'round',diameterMm:.15,plated:true}});
 const captured=capturedBoardGeometry(JSON.stringify(board));
 assert.ok(captured.holes.some(hole=>hole.source.source_id==='unconfirmed-span'&&hole.plated&&!hole.span_confirmed));
 assert.equal((await run()).error?.code,'CHANNEL_FOIL_PADSTACK');
 assert.equal(calls,4);
});

test('joint mixed candidate welds an explicit two-pin package to its retained finite pads',async t=>{
 const board=finitePlaneBend();
 const track=board.copper.fixed.tracks[0];
 track.points=[{x:0,y:0},{x:.55,y:0}];
 board.copper.fixed.tracks.push({...track,id:'signal-after-gap',points:[{x:.95,y:0},{x:1.5,y:0},{x:1.5,y:1.5}]});
 board.components.push({...board.components[0],designator:'R1',at:{x:.75,y:0}});
 for(const [i,x] of [.55,.95].entries())board.pads.push({...board.pads[0],id:'R1-'+i,component:'R1',number:String(i+1),at:{x,y:0}});
 const folder=mkdtempSync(join(tmpdir(),'p7-package-cache-'));
 t.after(()=>rmSync(folder,{recursive:true,force:true}));
 const cache=new LocalModelCache(folder);
 let calls=0,omitPackageLoss=false,mismatchPackageLoss=false;
 const local=createLocalExecutor(((problem:BoardFoilProblem):AnalysisBoardFoilCheckedReport=>{
  assert.ok(isAnalysisProblem(problem));
  const geometry=problem.geometry as Geometry;
  const [attachment]=problem.request.attachments!;
  assert.equal(problem.request.attachments!.length,1);
  assert.equal(attachment.id,'component:R1');
  assert.deepEqual(attachment.electrodes.map(name=>geometry.electrodes.find(e=>e.name===name)?.sourcePath),['pads[4]','pads[5]']);
  assert.deepEqual(geometry.ports.map(pair=>pair.map(name=>geometry.electrodes.find(e=>e.name===name)?.sourcePath)),[['pads[0]','pads[2]'],['pads[1]','pads[3]']]);
  assert.equal(geometry.electrodes.length,6);
  assert.equal(geometry.allowInsulatedCopperCuts,false);
  const captured=capturedBoardGeometry(problem.boardJson);
  const pinIslands=attachment.electrodes.map(name=>{
   const path=geometry.electrodes.find(e=>e.name===name)!.sourcePath;
   const region=captured.regions.find(r=>r.source.path===path)!;
   return captured.islands.find(island=>island.region_ids.includes(region.id))!.id;
  });
  assert.notEqual(pinIslands[0],pinIslands[1]);
  const solve=(request:AnalysisBoardFoilCheckedRequest):AnalysisBoardFoilCheckedReport=>{
   calls++;
   const report=checkedMock({...problem,request},captured.regions.map(r=>[r.id,r.source.path,r.source.net,r.layer]));
   // Serde emits object fields in sorted order.
   report.provenance.attachments=[{electrodes:attachment.electrodes,id:attachment.id}];
   const resistance=(attachment.model.data as CoupledRlcData).resistanceOhm[0][2];
   for(const {columns} of report.samples)for(const column of columns){
    column.copperLossW=8.375;
    if(!omitPackageLoss)column.packages=[{id:attachment.id,
     currentA:{real:mismatchPackageLoss?0:Math.sqrt(.5/resistance),imaginary:0},lossW:.5}];
   }
   return report;
  };
  return omitPackageLoss||mismatchPackageLoss?solve(problem.request):cachedBoardFoilReport(cache,'mock-physical-packages',problem,solve);
 }) as NonNullable<Parameters<typeof createLocalExecutor>[0]>);
 const run=(resistanceOhm:number)=>executeScenario({board,code:source.replace('.frequency(',
  ".components([resistor({designator:'R1',pins:{positive:'1',negative:'2'},resistanceOhm:"+resistanceOhm+"})]).frequency(")},
  {filename:'automatic-package.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},
  async task=>{
   const result=await local(task as AnalysisTask<'impedance'>);
   assert.equal(result.status,'partial');
   assert.deepEqual(result.network?.ports,['input','output']);
   const solution=result.solution as any;
   assert.match(solution.foilPlan.producer.attachmentsSha256,/^[a-f0-9]{64}$/);
   assert.deepEqual(solution.foilPlan.packages[0].physicalContacts,
    [{sourcePath:'pads[4]',layer:'TOP'},{sourcePath:'pads[5]',layer:'TOP'}]);
   assert.equal(solution.samples[0].columns[0].copperLossW,8.375);
   assert.equal(solution.samples[0].columns[0].packages[0].lossW,.5);
   return result;
  });
 for(const resistance of [10,10,20]){
  const output=await run(resistance);
  assert.equal(output.status,'complete',JSON.stringify(output.error));
 }
 assert.equal(calls,4);
 omitPackageLoss=true;
 assert.equal((await run(10)).error?.code,'CHANNEL_FOIL_EVIDENCE');
 omitPackageLoss=false;
 mismatchPackageLoss=true;
 const mismatch=await run(10);
 assert.equal(mismatch.error?.code,'CHANNEL_FOIL_EVIDENCE');
 assert.match(mismatch.error?.message??'',/dissipation does not match/);
});
