import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisBoardFoilCheckedReport } from '../native/analyze/index.js';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createFrequencyExecutor } from '../src/analyze/analysis-dsl/frequency-executor.ts';
import { createTransientExecutor } from '../src/analyze/analysis-dsl/transient-executor.ts';
import { createLocalExecutor } from '../src/analyze/analysis-dsl/local-executor.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { capturedBoardGeometry } from '../src/analyze/geometry-snapshot.ts';
import { isAnalysisProblem, type BoardFoilProblem } from '../src/analyze/types.ts';
import { finitePlaneBend, finitePlaneBendDsl as source } from './helpers/pcb-analysis-finite-plane-bend.ts';
import { createHash } from 'node:crypto';

type Geometry={allowInsulatedCopperCuts:boolean;roiMm:[[number,number],[number,number]];ports:[string,string][];cutPlanes:unknown;electrodes:{name:string;sourcePath:string}[]};

function checkedMock(problem:BoardFoilProblem, sourceMapping:AnalysisBoardFoilCheckedReport['provenance']['sourceMapping'],
 barrelIds:string[]=[]):AnalysisBoardFoilCheckedReport{
 const g=problem.geometry as Geometry;
 const {frequenciesHz,referenceOhm,budget}=problem.request;
 const s=[[{real:.2,imaginary:0},{real:.5,imaginary:0}],[{real:.5,imaginary:0},{real:.2,imaginary:0}]];
 const provenanceModel=barrelIds.length?{
  modelKind:'mixed' as const,nativeCopperTetrahedra:28,volumeTetrahedra:28,
  fullPotentialCount:4,fullCurrentCount:84,magneticQuadratureCoverage:'unverified' as const,
 }:{modelKind:'foil' as const,maxMagneticReciprocity:0,maxMagneticRefinement:0,maxElectricReciprocity:0};
 return {status:'complete',coverage:'unverified',representation:'S',frequenciesHz,referenceOhm:referenceOhm!,ports:g.ports,
   parameters:frequenciesHz.map(()=>s),enrichment:[],
   samples:frequenciesHz.map(frequencyHz=>({frequencyHz,columns:[0,1].map(input=>({input,kclRelative:0,kvlRelative:0,copperLossW:8.875,powerBalanceErrorW:0}))})),
   quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0},
   provenance:{method:barrelIds.length?'Mock mixed foil and volume':'Mock checked foil',...provenanceModel,barrelIds,sourceMapping,
    electrodeAreasM2:Object.fromEntries(g.electrodes.map(e=>[e.name,1e-8])),maxCopperVolumeChange:0,prisms:barrelIds.length?0:28,
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
  assert.ok(isAnalysisProblem(problem));
  foilCalls.push(problem);
  const g=problem.geometry as Geometry;
  assert.equal(g.allowInsulatedCopperCuts,false);
  assert.deepEqual(g.cutPlanes,{});
  assert.deepEqual(g.ports.map(pair=>pair.map(name=>g.electrodes.find(e=>e.name===name)?.sourcePath)),[['pads[0]','pads[2]'],['pads[1]','pads[3]']]);
  const {frequenciesHz,referenceOhm}=problem.request;
  assert.deepEqual(frequenciesHz,[1000,3000]);
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
 assert.equal(foilCalls.length,1);
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
 assert.equal(calls,3);
 board.holes.push({id:'unconfirmed-span',at:{x:.8,y:.8},hole:{shape:'round',diameterMm:.15,plated:true}});
 const captured=capturedBoardGeometry(JSON.stringify(board));
 assert.ok(captured.holes.some(hole=>hole.source.source_id==='unconfirmed-span'&&hole.plated&&!hole.span_confirmed));
 assert.equal((await run()).error?.code,'CHANNEL_FOIL_PADSTACK');
 assert.equal(calls,3);
});
