import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../native/analyze/index.js';
import { cachedBoardFoilReport, HYBRID_PLANAR_OPERATOR_VERSION } from '../src/analyze/board-foil-cache.ts';
import { digest, LocalModelCache } from '../src/analyze/local-model-cache.ts';
import { isAnalysisProblem, type BoardFoilProblem } from '../src/analyze/types.ts';
import { auditBoardFoilFrequency } from '../src/analyze/board-foil-frequency-audit.ts';
import { runAutoFoil } from '../src/analyze/auto-foil-response.ts';
import { validateBoardFoilReport } from '../src/analyze/board-foil-validation.ts';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';

const problem: BoardFoilProblem = {
  kind: 'board-foil-checked', boardJson: '{"board":"captured"}', geometry: { ports: [['a','b']] },
  expected: {
    electrodePairs: [['a','b']], sourceCopper: [{regionId:'p0',sourcePath:'pads[0]',layer:'TOP'}],
    sourceBarrels: [{barrelId:'barrel-0',sourcePath:'vias[0]',layers:['TOP','BOTTOM']}],
  },
  request: { frequenciesHz: [1000, 2000], referenceOhm: [50], budget: { maxSeconds: 30, maxMemoryMiB: 128 } },
};

function mock(request: AnalysisBoardFoilCheckedRequest, input = problem): AnalysisBoardFoilCheckedReport {
  const { frequenciesHz, referenceOhm, budget } = request;
  return {
    status:'complete', coverage:'unverified', representation:'S', frequenciesHz, referenceOhm:referenceOhm!,
    ports:input.expected.electrodePairs, parameters:frequenciesHz.map(()=>[[{real:0,imaginary:0}]]), enrichment:[],
    samples:frequenciesHz.map(frequencyHz=>({frequencyHz,columns:[{input:0,kclRelative:0,kvlRelative:0,kvlUncertaintyRelative:0,
      copperLossW:referenceOhm![0]/4,powerBalanceErrorW:0}]})),
    quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0,maxFullKvlUncertaintyRelative:0},
    ...(request.partitionPolicy==='automatic-conductive-schur-v1'&&{conductiveCorrection:{
      method:'sparse owner interiors and shared conductive trace Schur' as const,
      ownerInteriorDofs:[3,3],freeSharedTraceDofs:2,traceUpperEntries:3,ownedPlannedBytes:1024,
      maxOriginalFreeResidualRelative:0,scope:'Original-coordinate conductive correction',
    }}),
    provenance:{
    magneticOperator:{policy:'exact-streamed-v1',method:'unchanged direct tetrahedral pairs',scope:'Exact numerical pair action; physical accuracy unverified',maxFullKvlUncertaintyRelative:0},
      modelKind:'mixed',method:'mock mixed',barrelIds:['barrel-0'],
      ...(request.partitionPolicy==='automatic-conductive-schur-v1'&&{partitionPolicy:request.partitionPolicy,automaticPartition:{
        policy:request.partitionPolicy,status:'selected' as const,reason:'Original mesh partition',
        algorithm:'longest-extent-midpoint-v1' as const,meshOwnershipSignature:'fnv1a64:0123456789abcdef',
        owners:2,originalCopperTetrahedra:18,crossOwnerFaces:1,freeSharedTraceDofs:2,ownerInteriorDofs:[3,3],
      }}),
      sourceMapping:input.expected.sourceCopper.map(({regionId,sourcePath,layer})=>[regionId,sourcePath,null,layer]),
      electrodeAreasM2:{a:1e-8,b:1e-8},maxCopperVolumeChange:0,prisms:3,retainedPotentials:4,currentModes:5,
      nativeCopperTetrahedra:18,volumeTetrahedra:9,fullPotentialCount:12,fullCurrentCount:54,
      plannedBytes:32*1048576,maxSeconds:budget.maxSeconds,maxMemoryMiB:budget.maxMemoryMiB,
      magneticQuadratureCoverage:'unverified',spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1 A RMS',
    },
  };
}

test('compact magnetic evidence is checked without bypassing ownership or power gates', () => {
  const compact=()=>{
    const value=mock(problem.request);
    delete value.provenance.magneticOperator;
    value.operator='compact-planar-homogeneous-prototype';
    delete value.quality.maxFullKvlUncertaintyRelative;
    Object.assign(value.quality,{scope:'compact-discrete',physicalError:'unverified'});
    value.samples.forEach(s=>s.columns.forEach(c=>delete c.kvlUncertaintyRelative));
    value.provenance.compactOperator={version:'compact-planar-v1',basis:'dc-harmonic-only',electricOperator:'homogeneous-charge-gram',approximationError:'unverified',scope:'compact-discrete',originalVolumeActions:0,
      foilSupports:3,localTetraSupports:9,potentialCoordinates:8,currentCoordinates:24,planarMaxOrder:8,
      planarMaxRelativeChange:1e-9,mixedMaxRelativeChange:1e-9,tetraSelfMaxRelativeChange:1e-9,
      tetraPairAdaptiveTolerance:1e-10,physicalAccuracy:'unverified'};
    return value;
  };
  const validate=(value:AnalysisBoardFoilCheckedReport)=>validateBoardFoilReport(value,problem,problem.request.budget);
  validate(compact());
  const layered=compact();layered.operator='compact-planar-layered-prototype';layered.provenance.compactOperator!.electricOperator='layered-spatial-field-mesh';
  validate(layered);
  layered.provenance.currentModes=0;layered.provenance.compactOperator!.planarMaxOrder=0;validate(layered);
  const adaptive=()=>{
    const value=compact(); value.operator='compact-planar-layered-prototype';
    Object.assign(value.provenance.compactOperator!,{basis:'adaptive-mixed-closure',electricOperator:'layered-spatial-field-mesh',nearQuadratureEstimatedTolerance:1e-8});
    value.quality.maxCompactKvlUncertaintyRelative=0;
    for(const sample of value.samples)for(const column of sample.columns){
      column.compactKvlUncertaintyRelative=0;
      column.compactMagneticCertificate={policy:'compact-mixed-near-far',nearQuadratureErrorCertified:false};
    }
    return value;
  };
  validate(adaptive());
  const controlled=adaptive();
  const timeQuality={relativeResidual:0,maximumLocalErrorRatio:.1,energyBalanceJ:0,energyNormalizationJ:1,maximumAcceptedStepS:.02/256};
  controlled.modelRegimeAudit={status:'pass',scope:'discrete-model-only',physicalCoverage:'unverified',probeHz:50,
    dcPortVoltagesV:[[25]],acPortVoltagesV:[{real:[25],imaginary:[0]}],timeDiagnostics:[{relativePeakErrors:[.001,.0005],timeQuality:[timeQuality,timeQuality]}]};
  const previousRegime=process.env.P7_HYBRID_REGIME_AUDIT;
  try{
    process.env.P7_HYBRID_REGIME_AUDIT='1';
    assert.throws(()=>validate(adaptive()),/requested model regime control/);
    validate(controlled);
    for(const change of [
      (v:AnalysisBoardFoilCheckedReport)=>{v.modelRegimeAudit!.probeHz=101;},
      (v:AnalysisBoardFoilCheckedReport)=>{v.modelRegimeAudit!.timeDiagnostics[0].relativePeakErrors[1]=.03;},
      (v:AnalysisBoardFoilCheckedReport)=>{v.modelRegimeAudit!.timeDiagnostics[0].timeQuality[1].energyBalanceJ=.001;},
      (v:AnalysisBoardFoilCheckedReport)=>{v.modelRegimeAudit!.physicalCoverage='accepted' as any;},
    ]){const value=structuredClone(controlled);change(value);assert.throws(()=>validate(value),/CHANNEL_FOIL_EVIDENCE/);}
  }finally{if(previousRegime===undefined)delete process.env.P7_HYBRID_REGIME_AUDIT;else process.env.P7_HYBRID_REGIME_AUDIT=previousRegime;}
  const hybridProblem:BoardFoilProblem={...problem,request:{...problem.request,partitionPolicy:'automatic-hybrid-v1'}};
  const hybrid=()=>{
    const value=adaptive();
    value.provenance.partitionPolicy='automatic-hybrid-v1';
    value.provenance.hybridAssembly={policy:'automatic-hybrid-v1',method:'global-compact-mixed',potentialCoordinates:8,currentCoordinates:24,physicalAccuracy:'unverified'};
    return value;
  };
  const validateHybrid=(v:AnalysisBoardFoilCheckedReport)=>validateBoardFoilReport(v,hybridProblem,problem.request.budget);
  validateHybrid(hybrid());
  const twoPortProblem:BoardFoilProblem={...hybridProblem,
    geometry:{ports:[['a','b'],['c','d']],returnBoundaryMm:[[-1,-1,-1],[1,1,1]]},
    expected:{...hybridProblem.expected,electrodePairs:[['a','b'],['c','d']]},
    request:{...hybridProblem.request,referenceOhm:[50,50]}};
  const coupled=hybrid();
  coupled.ports=twoPortProblem.expected.electrodePairs;coupled.referenceOhm=[50,50];
  Object.assign(coupled.provenance.electrodeAreasM2,{c:1e-8,d:1e-8});
  coupled.parameters=coupled.frequenciesHz.map(()=>[[{real:.9,imaginary:0},{real:.3,imaginary:0}],[{real:.3,imaginary:0},{real:0,imaginary:0}]]);
  for(const sample of coupled.samples){
    const original=sample.columns[0];
    sample.columns=[0,1].map(input=>({...structuredClone(original),input,copperLossW:input===0?1.25:11.375,
      returnBoundary:{policy:'internal-guard-flux-v1',boundsMm:[[-1,-1,-1],[1,1,1]],conductiveA:[.00075,0,0,0,0,0],
        displacementA:[0,0,0,0,0,0],portCurrentA:input===0?.05:.5,unresolvedA:.00075,
        relative:input===0?.015:.0015,absoluteFloorA:1e-12,threshold:.01,passed:input!==0}}));
  }
  validateBoardFoilReport(coupled,twoPortProblem,problem.request.budget);
  for(const sample of coupled.samples){
    Object.assign(sample.columns[0].returnBoundary!,{portCurrentA:.15,relative:.005,passed:true});
  }
  assert.throws(()=>validateBoardFoilReport(coupled,twoPortProblem,problem.request.budget),/normalization disagrees/,
    'larger induced output current must not turn failing input return into a pass');
  assert.throws(()=>validate(hybrid()),/CHANNEL_FOIL_EVIDENCE/);
  for(const change of [
    (v:AnalysisBoardFoilCheckedReport)=>{delete v.provenance.hybridAssembly;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.hybridAssembly!.potentialCoordinates++;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.hybridAssembly!.currentCoordinates++;},
    (v:AnalysisBoardFoilCheckedReport)=>{(v.provenance.hybridAssembly as any).method='schur';},
    (v:AnalysisBoardFoilCheckedReport)=>{(v.provenance.hybridAssembly as any).physicalAccuracy='verified';},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.compactOperator!.basis='full-mixed';},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.automaticPartition=mock({...problem.request,partitionPolicy:'automatic-conductive-schur-v1'}).provenance.automaticPartition;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.conductiveCorrection=mock({...problem.request,partitionPolicy:'automatic-conductive-schur-v1'}).conductiveCorrection;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.samples[0].columns[0].copperLossW=0;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.sourceMapping=[];},
  ]){const value=hybrid();change(value);assert.throws(()=>validateHybrid(value),/CHANNEL_FOIL_EVIDENCE/);}
  const dir=mkdtempSync(join(tmpdir(),'p7-hybrid-policy-'));
  const previous=process.env.P7_COMPACT_PLANAR;
  const previousWorkers=process.env.P7_COMPACT_PL_THREADS;
  const previousNearWorkers=process.env.P7_MAGNETIC_NEAR_THREADS;
  try {
    process.env.P7_COMPACT_PLANAR='1';
    delete process.env.P7_COMPACT_PL_THREADS;
    delete process.env.P7_MAGNETIC_NEAR_THREADS;
    const cache=new LocalModelCache(dir);
    const produceHybrid=(r:AnalysisBoardFoilCheckedRequest)=>{const value=hybrid();value.provenance.maxSeconds=r.budget.maxSeconds;value.provenance.maxMemoryMiB=r.budget.maxMemoryMiB;return value;};
    const cached=cachedBoardFoilReport(cache,'hybrid',hybridProblem,produceHybrid);
    const regimeBeforeCache=process.env.P7_HYBRID_REGIME_AUDIT;
    try{
      process.env.P7_HYBRID_REGIME_AUDIT='1';
      const audited=cachedBoardFoilReport(cache,'hybrid',hybridProblem,r=>({...produceHybrid(r),modelRegimeAudit:structuredClone(controlled.modelRegimeAudit)}));
      assert.notEqual(audited.cache?.key,cached.cache?.key);
      assert.equal(cachedBoardFoilReport(cache,'hybrid',hybridProblem,()=>{throw Error('unexpected audited solve');}).cache?.status,'memory');
    }finally{if(regimeBeforeCache===undefined)delete process.env.P7_HYBRID_REGIME_AUDIT;else process.env.P7_HYBRID_REGIME_AUDIT=regimeBeforeCache;}
    assert.equal(cachedBoardFoilReport(cache,'hybrid',hybridProblem,()=>{throw Error('unexpected solve');}).cache?.key,cached.cache?.key);
    process.env.P7_COMPACT_PL_THREADS='2';
    assert.notEqual(cachedBoardFoilReport(cache,'hybrid',hybridProblem,produceHybrid).cache?.key,cached.cache?.key);
    process.env.P7_COMPACT_PL_THREADS='5';
    assert.throws(()=>cachedBoardFoilReport(cache,'hybrid',hybridProblem,()=>hybrid()),/LOCAL_BUDGET/);
    delete process.env.P7_COMPACT_PL_THREADS;
    const schurProblem={...problem,request:{...problem.request,partitionPolicy:'automatic-conductive-schur-v1' as const}};
    process.env.P7_MAGNETIC_NEAR_THREADS='2';
    assert.notEqual(cachedBoardFoilReport(cache,'hybrid',hybridProblem,produceHybrid).cache?.key,cached.cache?.key);
    process.env.P7_MAGNETIC_NEAR_THREADS='5';
    assert.throws(()=>cachedBoardFoilReport(cache,'hybrid',hybridProblem,produceHybrid),/LOCAL_BUDGET/);
    delete process.env.P7_MAGNETIC_NEAR_THREADS;
    assert.notEqual(cachedBoardFoilReport(cache,'hybrid',schurProblem,r=>mock(r)).cache?.key,cached.cache?.key);
    const rfProblem:BoardFoilProblem={...hybridProblem,geometry:{...hybridProblem.geometry,
      stack:['TOP','BOTTOM'].map(name=>({name,copper:true,conductivitySM:58e6})),barrelConductivitySM:58e6},
      request:{...hybridProblem.request,frequenciesHz:[1e6,2e6],operatorPolicy:'hybrid-planar-v3',profileUpperHz:1e8}};
    const rfReport=(request:AnalysisBoardFoilCheckedRequest)=>{
      const value=produceHybrid(request),upper=request.profileUpperHz??request.frequenciesHz.at(-1)!;
      value.frequenciesHz=request.frequenciesHz;
      value.samples.forEach((sample,i)=>{sample.frequencyHz=request.frequenciesHz[i];});
      value.provenance.operatorPolicy='hybrid-planar-v3';
      const depth=1/Math.sqrt(Math.PI*upper*(4e-7*Math.PI)*58e6);
      value.provenance.copperSkinMesh={version:3,requestedUpperHz:request.frequenciesHz.at(-1)!,trainingUpperBoundHz:request.frequenciesHz.at(-1)!,effectiveUpperHz:upper,skinDepthDivisor:3,currentAndElectricShareMesh:true,scope:'Implementation mesh screening only',
        layers:['TOP','BOTTOM'].map(layer=>({layer,skinDepthM:depth,maximumStepM:depth/6,slabs:6})),roundBarrels:[{id:'barrel-0',skinDepthM:depth,maximumRadialStepM:depth/4,shells:4}]};
      return value;
    };
    const plan={...problem.expected,boardJson:problem.boardJson,geometry:rfProblem.geometry,ports:[{name:'port'}],attachments:[]} as unknown as AutoFoilPlan;
    runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>{
      assert.equal(input.request.partitionPolicy,'automatic-hybrid-v1');return rfReport(input.request);
    });
    const expansions:number[]=[];
    const searching={...plan,returnSearch:{initialMarginMm:1,expand:(scale:number)=>{
      expansions.push(scale);
      return {...plan,roiMm:[[-2*scale,-2*scale],[2*scale,2*scale]],
        geometry:{...(plan.geometry as object),returnBoundaryMm:[[-scale,-scale,-scale],[scale,scale,scale]]}} as AutoFoilPlan;
    }}};
    const boundaryReport=(input:BoardFoilProblem,relative:number)=>{
      const value=rfReport(input.request);
      for(const sample of value.samples)for(const column of sample.columns){
        column.returnBoundary={policy:'internal-guard-flux-v1',boundsMm:(input.geometry as any).returnBoundaryMm,
          conductiveA:[relative*.25,0,0,0,0,0],displacementA:[0,0,relative*.25,0,0,0],
          portCurrentA:.5,unresolvedA:relative*.5,relative,absoluteFloorA:1e-12,threshold:.01,passed:relative<.01};
      }
      return value;
    };
    const searched=runAutoFoil(searching,[1000,2000],problem.request.budget,undefined,input=>boundaryReport(input,expansions.length===1?.02:.005));
    assert.deepEqual(expansions,[1,2]);
    assert.equal(searched.returnSearch!.steps.length,2);
    assert.equal(searched.returnSearch!.steps[1].passed,true);
    assert.deepEqual(searched.resolvedPlan!.roiMm,[[-4,-4],[4,4]]);
    assert.ok(!Object.keys(searched).includes('resolvedPlan'));
    expansions.length=0;
    assert.throws(()=>runAutoFoil(searching,[1000,2000],problem.request.budget,undefined,input=>boundaryReport(input,.01)),/CHANNEL_RETURN_UNRESOLVED/);
    assert.equal(expansions.length,8,'the threshold is strictly below one percent and expansion is bounded');
    assert.throws(()=>runAutoFoil(searching,[1000,2000],problem.request.budget,undefined,input=>{
      const value=boundaryReport(input,.005);delete value.samples[0].columns[0].returnBoundary;return value;
    }),/CHANNEL_RETURN_EVIDENCE/);
    assert.throws(()=>runAutoFoil(searching,[1000,2000],problem.request.budget,undefined,input=>{
      const value=boundaryReport(input,.005);value.samples[0].columns[0].returnBoundary!.portCurrentA=50;return value;
    }),/CHANNEL_RETURN_EVIDENCE|CHANNEL_FOIL_EVIDENCE/);
    const rf=cachedBoardFoilReport(cache,'hybrid',rfProblem,rfReport);
    runAutoFoil({...plan,geometry:rfProblem.geometry} as AutoFoilPlan,[1e6,2e6],problem.request.budget,undefined,input=>{
      assert.equal(input.request.operatorPolicy,'hybrid-planar-v3');
      assert.equal(input.request.partitionPolicy,'automatic-hybrid-v1');
      return rfReport(input.request);
    });
    assert.notEqual(rf.cache?.key,cached.cache?.key);
    assert.equal(cachedBoardFoilReport(cache,'hybrid',rfProblem,()=>{throw Error('unexpected RF solve');}).cache?.status,'memory');
    assert.throws(()=>cachedBoardFoilReport(cache,'hybrid',{...rfProblem,request:{...rfProblem.request,partitionPolicy:'automatic-conductive-schur-v1'}},rfReport),/BOARD_FOIL_GRID/);
    assert.throws(()=>cachedBoardFoilReport(cache,'hybrid',{...rfProblem,request:{...rfProblem.request,profileUpperHz:1e10+1}},rfReport),/BOARD_FOIL_GRID/);
    const undersampled=rfReport(rfProblem.request);undersampled.provenance.copperSkinMesh!.layers[0].maximumStepM*=2;
    assert.throws(()=>validateBoardFoilReport(undersampled,rfProblem,rfProblem.request.budget),/CHANNEL_FOIL_EVIDENCE/);
  } finally {if(previous===undefined)delete process.env.P7_COMPACT_PLANAR;else process.env.P7_COMPACT_PLANAR=previous;if(previousWorkers===undefined)delete process.env.P7_COMPACT_PL_THREADS;else process.env.P7_COMPACT_PL_THREADS=previousWorkers;if(previousNearWorkers===undefined)delete process.env.P7_MAGNETIC_NEAR_THREADS;else process.env.P7_MAGNETIC_NEAR_THREADS=previousNearWorkers;rmSync(dir,{recursive:true,force:true});}
  for(const change of [
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.compactOperator!.nearQuadratureEstimatedTolerance=1e-7;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.quality.maxCompactKvlUncertaintyRelative=1e-9;},
    (v:AnalysisBoardFoilCheckedReport)=>{delete v.samples[0].columns[0].compactMagneticCertificate;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.samples[0].columns[0].compactKvlUncertaintyRelative=1;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.samples[0].columns[0].compactKvlUncertaintyRelative=NaN;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.compactOperator!.basis='full-mixed';},
    (v:AnalysisBoardFoilCheckedReport)=>{v.samples[0].columns[0].copperLossW=100;},
    (v:AnalysisBoardFoilCheckedReport)=>{v.provenance.sourceMapping=[];},
  ]){const value=adaptive();change(value);assert.throws(()=>validate(value),/CHANNEL_FOIL_EVIDENCE/);}
  const changes:((value:AnalysisBoardFoilCheckedReport)=>void)[]=[
    v=>{(v.provenance.compactOperator as any).version='other';},
    v=>{(v.provenance.compactOperator as any).scope='physical';},
    v=>{(v.provenance.compactOperator as any).basis='other';},
    v=>{(v.provenance.compactOperator as any).approximationError='verified';},
    v=>{v.provenance.compactOperator!.electricOperator='layered-spatial-field-mesh';},
    v=>{(v.quality as any).scope='physical';},
    v=>{(v.quality as any).physicalError='verified';},
    v=>{(v.provenance.compactOperator as any).originalVolumeActions=1;},
    v=>{v.provenance.compactOperator!.foilSupports++;},
    v=>{v.provenance.compactOperator!.localTetraSupports++;},
    v=>{v.provenance.compactOperator!.potentialCoordinates=0;},
    v=>{v.provenance.compactOperator!.currentCoordinates=55;},
    v=>{v.provenance.compactOperator!.planarMaxOrder=0;},
    v=>{v.provenance.compactOperator!.planarMaxOrder=68;},
    v=>{v.provenance.compactOperator!.planarMaxOrder=9;},
    ...(['planarMaxRelativeChange','mixedMaxRelativeChange','tetraSelfMaxRelativeChange','tetraPairAdaptiveTolerance'] as const).map(key=>(v:AnalysisBoardFoilCheckedReport)=>{v.provenance.compactOperator![key]=1e-7;}),
    v=>{v.provenance.compactOperator!.mixedMaxRelativeChange=NaN;},
    v=>{v.provenance.sourceMapping=[];},
    v=>{v.samples[0].columns[0].copperLossW=0;},
  ];
  for(const change of changes){const value=compact();change(value);assert.throws(()=>validate(value),/CHANNEL_FOIL_EVIDENCE/);}
});

test('section current hints have isolated response cache identity', () => {
  const dir=mkdtempSync(join(tmpdir(),'p7-section-hints-'));
  const before=process.env.P7_SECTION_HINTS;
  try {
    const cache=new LocalModelCache(dir);
    const run=()=>cachedBoardFoilReport(cache,'section-control',problem,r=>mock(r));
    delete process.env.P7_SECTION_HINTS;
    const baseline=run();
    process.env.P7_SECTION_HINTS='1';
    assert.notEqual(run().cache?.key,baseline.cache?.key);
    assert.equal(run().cache?.status,'memory');
    process.env.P7_SECTION_HINTS='0';
    assert.equal(run().cache?.key,baseline.cache?.key);
  } finally {
    if(before===undefined)delete process.env.P7_SECTION_HINTS;else process.env.P7_SECTION_HINTS=before;
    rmSync(dir,{recursive:true,force:true});
  }
});

test('experimental axial tetrahedra isolate response cache identity', () => {
  const dir=mkdtempSync(join(tmpdir(),'p7-axial-tetra-'));const before=process.env.P7_MAGNETIC_AXIAL_TETS;
  try {
    const cache=new LocalModelCache(dir);const run=()=>cachedBoardFoilReport(cache,'axial-control',problem,r=>mock(r));
    delete process.env.P7_MAGNETIC_AXIAL_TETS;const baseline=run();
    process.env.P7_MAGNETIC_AXIAL_TETS='1';assert.notEqual(run().cache?.key,baseline.cache?.key);assert.equal(run().cache?.status,'memory');
    process.env.P7_MAGNETIC_AXIAL_TETS='0';assert.equal(run().cache?.key,baseline.cache?.key);
    process.env.P7_MAGNETIC_AXIAL_TETS='auto';assert.throws(run,/unsupported P7_MAGNETIC_AXIAL_TETS/);
  } finally {if(before===undefined)delete process.env.P7_MAGNETIC_AXIAL_TETS;else process.env.P7_MAGNETIC_AXIAL_TETS=before;rmSync(dir,{recursive:true,force:true});}
});

test('bounded near omission has isolated response cache identity', () => {
  const dir=mkdtempSync(join(tmpdir(),'p7-near-bound-'));
  const before=process.env.P7_MAGNETIC_NEAR_BOUND;
  try {
    const cache=new LocalModelCache(dir);
    const run=()=>cachedBoardFoilReport(cache,'near-control',problem,r=>mock(r));
    delete process.env.P7_MAGNETIC_NEAR_BOUND;const baseline=run();
    process.env.P7_MAGNETIC_NEAR_BOUND='1';assert.notEqual(run().cache?.key,baseline.cache?.key);
    assert.equal(run().cache?.status,'memory');
    process.env.P7_MAGNETIC_NEAR_BOUND='0';assert.equal(run().cache?.key,baseline.cache?.key);
  } finally {
    if(before===undefined)delete process.env.P7_MAGNETIC_NEAR_BOUND;else process.env.P7_MAGNETIC_NEAR_BOUND=before;
    rmSync(dir,{recursive:true,force:true});
  }
});

test('magnetic batch partition policy separates cached numerical certificates', () => {
  const dir=mkdtempSync(join(tmpdir(),'p7-magnetic-batches-'));
  const before=process.env.P7_MAGNETIC_BATCH_COLUMNS;
  try {
    const cache=new LocalModelCache(dir);
    const run=()=>cachedBoardFoilReport(cache,'batch-control',problem,r=>mock(r));
    delete process.env.P7_MAGNETIC_BATCH_COLUMNS;
    const common=run();
    process.env.P7_MAGNETIC_BATCH_COLUMNS='1';
    const single=run();
    assert.notEqual(single.cache?.key,common.cache?.key);
    assert.equal(run().cache?.status,'memory');
    process.env.P7_MAGNETIC_BATCH_COLUMNS='0';
    assert.equal(run().cache?.key,common.cache?.key);
    for(const invalid of ['-1','65','1.5','NaN']) {
      process.env.P7_MAGNETIC_BATCH_COLUMNS=invalid;
      assert.throws(run,/LOCAL_BUDGET/);
    }
  } finally {
    if(before===undefined)delete process.env.P7_MAGNETIC_BATCH_COLUMNS;else process.env.P7_MAGNETIC_BATCH_COLUMNS=before;
    rmSync(dir,{recursive:true,force:true});
  }
});

test('automatic partition policy reaches native preparation and exact cache with checked evidence', () => {
  const dir=mkdtempSync(join(tmpdir(),'p7-automatic-partition-'));
  try {
    const cache=new LocalModelCache(dir);
    const plan={...problem.expected,boardJson:problem.boardJson,geometry:problem.geometry,
      ports:[{name:'original-port'}],attachments:[]} as unknown as AutoFoilPlan;
    // Exercise historical Schur evidence directly, independent of production routing.
    const runAutoFoil = (_plan:AutoFoilPlan, frequenciesHz:number[], budget:BoardFoilProblem['request']['budget'], interval:readonly [number,number]|undefined, execute:(input:BoardFoilProblem)=>AnalysisBoardFoilCheckedReport) => {
      const input:BoardFoilProblem={...problem,request:{...problem.request,frequenciesHz,budget,partitionPolicy:'automatic-conductive-schur-v1'}};
      const value=execute(input); validateBoardFoilReport(value,input,value.cache?.producerBudget??budget);
      const frequencyAudit=interval&&auditBoardFoilFrequency(input,value,interval,[1000,100000],performance.now(),execute);
      return {...value,frequencyAudit};
    };
    let calls=0;
    const execute=(input:BoardFoilProblem)=>cachedBoardFoilReport(cache,'core-partition',input,request=>{
      calls++;
      assert.equal(request.partitionPolicy,'automatic-conductive-schur-v1');
      return mock(request,input);
    });
    const run=()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,execute);
    const cold=run(),warm=run();
    assert.equal(calls,1);
    assert.equal(warm.cache?.status,'memory');
    assert.equal(warm.cache?.key,cold.cache?.key);
    assert.equal(warm.coverage,'unverified');
    assert.equal(warm.provenance.automaticPartition?.status,'selected');
    const tree=(input:BoardFoilProblem)=>{
      const value=mock(input.request,input);
      const certificate={
        coverSignature:'fnv1a64:1111111111111111',envelopeSignature:'fnv1a64:2222222222222222',
        absoluteActionGoals:[1e-12,1e-12],maximumAbsoluteErrorBounds:[1e-15,1e-15],
        chosenOrderHistogram:[1,...Array(12).fill(0)],farClusterPairs:1,exactPairs:18,
        zeroBlocks:0,activeEnvelopeTerminals:18,actionWallSeconds:0.01,
      };
      value.provenance.magneticOperator={
        policy:'certified-adaptive-harmonic-v1',method:'symmetric adaptive Newton expansion',
        scope:'Far truncation and arithmetic only; physical and near quadrature unverified',
        expansionOrder:12,minimumExpansionOrder:0,separationRatioMaximum:1,
        admissibility:'strict-disjoint-balls-certified-tail',treeNodes:3,
        representationVersion:3,harmonicBasis:'laplace-quotient-z01',momentCoefficientCount:169,
        m2lCouplings:5551,shiftStencilTerms:3619,axisSerial:true,leafTetrahedra:1,
        interactionStorage:'immutable-adaptive-dual-tree-v1',interactionLevels:6,interactionStackSlots:24,
        interactionReplaysPerAction:3,traversalStorageBytes:1184,
        geometrySignature:'fnv1a64:0123456789abcdef',
        basisCover:{...certificate,modelRevision:1,rhsColumns:2,visitedNodePairs:4,nearTiles:2,
          maximumMomentDegreeHistogram:Array(13).fill(0),maximumLocalDegreeHistogram:Array(13).fill(0),
          m2lScalarContractionsPerAction:3,shiftStencilApplicationsPerAction:0},
        plannedBytes:2048,treeSignature:'fnv1a64:0123456789abcdef',maxFullKvlUncertaintyRelative:5e-10,
      };
      for(const sample of value.samples)for(const column of sample.columns){
        column.kvlRelative=1e-9;column.kvlUncertaintyRelative=5e-10;
        column.magneticCertificate={...certificate,policy:'certified-adaptive-harmonic-v1'};
      }
      value.quality.maxKvlRelative=1e-9;
      value.quality.maxFullKvlUncertaintyRelative=5e-10;
      return value;
    };
    assert.equal(runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,tree)
      .provenance.magneticOperator?.policy,'certified-adaptive-harmonic-v1');
    const chunked=(input:BoardFoilProblem,corrupt=false)=>{
      const value=tree(input),operator=value.provenance.magneticOperator!;
      if(operator.policy==='certified-adaptive-harmonic-v1') {
        const cover=operator.basisCover;
        const chunks=[0,1].map(i=>({...structuredClone(cover),rhsColumns:1,
          absoluteActionGoals:[cover.absoluteActionGoals[i]],maximumAbsoluteErrorBounds:[cover.maximumAbsoluteErrorBounds[i]]}));
        Object.assign(cover,{batchColumns:1,chunkCount:2,coverAggregation:'sum-of-sequential-chunk-covers',chunkCertificates:chunks,
          chosenOrderHistogram:cover.chosenOrderHistogram.map(n=>2*n),farClusterPairs:2,exactPairs:36});
        if(corrupt)chunks[1].maximumAbsoluteErrorBounds[0]=2e-12;
      }
      return value;
    };
    assert.doesNotThrow(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>chunked(input)));
    assert.throws(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>chunked(input,true)),/CHANNEL_FOIL_EVIDENCE/);
    const bounded=(input:BoardFoilProblem,bad?:'disabled'|'sum'|'policy')=>{
      const value=chunked(input);const cover=(value.provenance.magneticOperator as any).basisCover;
      Object.assign(cover,{nearBoundEnabled:true,boundedOmittedBlocks:3,nearBoundPolicy:'summed-per-tetra-volume-v1'});
      cover.chunkCertificates.forEach((c:any,i:number)=>Object.assign(c,{nearBoundEnabled:true,boundedOmittedBlocks:i+1,nearBoundPolicy:'summed-per-tetra-volume-v1'}));
      if(bad==='disabled')cover.nearBoundEnabled=false;
      if(bad==='sum')cover.boundedOmittedBlocks=4;
      if(bad==='policy')cover.nearBoundPolicy='unknown';
      return value;
    };
    assert.doesNotThrow(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,bounded));
    for(const bad of ['disabled','sum','policy'] as const) assert.throws(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>bounded(input,bad)),/CHANNEL_FOIL_EVIDENCE/);
    const estimated=(input:BoardFoilProblem,bad?:'certified'|'policy'|'chunk'|'column')=>{
      const value=chunked(input);const cover=(value.provenance.magneticOperator as any).basisCover;
      const tag={nearQuadraturePolicy:'axial-tetra-estimated-v1',nearQuadratureErrorCertified:false};
      Object.assign(cover,tag);cover.chunkCertificates.forEach((c:any)=>Object.assign(c,tag));
      value.samples.forEach(s=>s.columns.forEach(c=>Object.assign(c.magneticCertificate!,tag)));
      if(bad==='certified')cover.nearQuadratureErrorCertified=true;
      if(bad==='policy')cover.nearQuadraturePolicy='unknown';
      if(bad==='chunk')delete cover.chunkCertificates[0].nearQuadraturePolicy;
      if(bad==='column')delete value.samples[0].columns[0].magneticCertificate!.nearQuadraturePolicy;
      return value;
    };
    assert.doesNotThrow(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,estimated));
    for(const bad of ['certified','policy','chunk','column'] as const) assert.throws(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>estimated(input,bad)),/CHANNEL_FOIL_EVIDENCE/);
    assert.doesNotThrow(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>{
      const value=tree(input),operator=value.provenance.magneticOperator!;
      if(operator.policy==='certified-adaptive-harmonic-v1')Object.assign(operator.basisCover,{
        rhsColumns:0,absoluteActionGoals:[],maximumAbsoluteErrorBounds:[],activeEnvelopeTerminals:0,
        chosenOrderHistogram:Array(13).fill(0),farClusterPairs:0,nearTiles:0,exactPairs:0,zeroBlocks:1,
        m2lScalarContractionsPerAction:0,shiftStencilApplicationsPerAction:0,
      });
      return value;
    }));
    const changedTree=runAutoFoil(plan,[1000,2000],problem.request.budget,[1000,2000],input=>{
      const value=tree(input);
      if(input.request.frequenciesHz.length===1&&value.provenance.magneticOperator?.policy==='certified-adaptive-harmonic-v1'){
        value.provenance.magneticOperator.treeSignature='fnv1a64:fedcba9876543210';
        value.provenance.magneticOperator.geometrySignature='fnv1a64:fedcba9876543210';
      }
      return value;
    });
    assert.ok(changedTree.frequencyAudit?.batch.status==='refused');
    assert.match(changedTree.frequencyAudit.batch.message??'',/CHANNEL_FOIL_FREQUENCY_MESH/);
    const changedCover=runAutoFoil(plan,[1000,2000],problem.request.budget,[1000,2000],input=>{
      const value=tree(input),operator=value.provenance.magneticOperator!;
      if(input.request.frequenciesHz.length===1&&operator.policy==='certified-adaptive-harmonic-v1')
        operator.basisCover.coverSignature='fnv1a64:3333333333333333';
      return value;
    });
    assert.equal(changedCover.frequencyAudit?.batch.status,'checked');
    for(const corrupt of [
      (value:AnalysisBoardFoilCheckedReport)=>{delete value.provenance.magneticOperator;},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.provenance.magneticOperator!,{representationVersion:1});},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.provenance.magneticOperator!,{momentCoefficientCount:286});},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.provenance.magneticOperator!,{interactionStorage:'stored-pairs'});},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.provenance.magneticOperator!,{traversalStorageBytes:0});},
      (value:AnalysisBoardFoilCheckedReport)=>{delete value.samples[0].columns[0].magneticCertificate;},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.samples[0].columns[0].magneticCertificate!,{absoluteActionGoals:[],maximumAbsoluteErrorBounds:[]});},
      (value:AnalysisBoardFoilCheckedReport)=>{value.samples[0].columns[0].magneticCertificate!.maximumAbsoluteErrorBounds[0]=1;},
      (value:AnalysisBoardFoilCheckedReport)=>{value.samples[0].columns[0].magneticCertificate!.chosenOrderHistogram[0]=2;},
      (value:AnalysisBoardFoilCheckedReport)=>{value.quality.maxFullKvlUncertaintyRelative=0;},
      (value:AnalysisBoardFoilCheckedReport)=>{value.samples[0].columns[0].kvlUncertaintyRelative=2e-9;},
      (value:AnalysisBoardFoilCheckedReport)=>{value.samples[0].columns[0].kvlRelative=2e-8;},
    ]){
      assert.throws(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>{
        const value=tree(input);corrupt(value);return value;
      }),/CHANNEL_FOIL_EVIDENCE/);
    }
    const audited=runAutoFoil(plan,[1000,2000],problem.request.budget,[1000,2000],input=>{
      const report=mock(input.request,input);
      if(input.request.frequenciesHz.length===1)
        report.provenance.automaticPartition!.meshOwnershipSignature='fnv1a64:fedcba9876543210';
      return report;
    });
    assert.ok(audited.frequencyAudit?.batch.status==='refused');
    assert.match(audited.frequencyAudit.batch.message??'',/changed the original physical mesh or ownership/);
    for(const corrupt of [
      (report:AnalysisBoardFoilCheckedReport)=>{delete report.provenance.automaticPartition;},
      (report:AnalysisBoardFoilCheckedReport)=>{report.conductiveCorrection!.maxOriginalFreeResidualRelative=2e-8;},
      (report:AnalysisBoardFoilCheckedReport)=>{report.conductiveCorrection!.ownerInteriorDofs=[0,6];},
      (report:AnalysisBoardFoilCheckedReport)=>{report.provenance.automaticPartition!.status='not-applicable';delete report.conductiveCorrection;},
    ]){
      assert.throws(()=>runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,input=>{
        const report=mock(input.request,input);corrupt(report);return report;
      }),/CHANNEL_FOIL_EVIDENCE/);
    }
  } finally {rmSync(dir,{recursive:true,force:true});}
});

test('exact grid cache preserves immutable producer evidence through warm and process restart', () => {
  const dir = mkdtempSync(join(tmpdir(),'p7-foil-cache-'));
  try {
    assert.ok(isAnalysisProblem(problem));
    const cache = new LocalModelCache(dir);
    let calls = 0;
    const produce = (request: AnalysisBoardFoilCheckedRequest) => { calls++; return mock(request); };
    const cold = cachedBoardFoilReport(cache,'core-1',problem,produce);
    assert.equal(cold.cache?.status,'cold');
    assert.equal(cold.cache?.assembly,1);
    cold.parameters[0][0][0].real = 0.1;
    const replay = {...problem,request:{...problem.request,budget:{maxSeconds:10,maxMemoryMiB:1}}};
    const warm = cachedBoardFoilReport(cache,'core-1',replay,produce);
    assert.equal(warm.cache?.status,'memory');
    assert.equal(warm.cache?.assembly,0);
    assert.equal(warm.cache?.training,0);
    assert.equal(warm.cache?.replayBudget.maxMemoryMiB,1);
    assert.deepEqual(warm.cache?.producerBudget,cold.cache?.producerBudget);
    assert.equal(warm.provenance.plannedBytes,32*1048576);
    assert.equal(warm.parameters[0][0][0].real,0);
    assert.equal(warm.coverage,'unverified');
    assert.equal(calls,1);

    const code = [
      "import { LocalModelCache } from './src/analyze/local-model-cache.ts';",
      "import { cachedBoardFoilReport } from './src/analyze/board-foil-cache.ts';",
      'const report=cachedBoardFoilReport(new LocalModelCache('+JSON.stringify(dir)+'),"core-1",'+JSON.stringify(replay)+',()=>{throw Error("unexpected native preparation")});',
      'console.log(JSON.stringify(report.cache));',
    ].join('\n');
    const restarted = JSON.parse(execFileSync(process.execPath,[...process.execArgv,'--input-type=module','-e',code],{encoding:'utf8'}));
    assert.equal(restarted.status,'disk');
    assert.equal(restarted.key,cold.cache!.key);
    assert.equal(restarted.assembly,0);
    assert.deepEqual(restarted.producerBudget,cold.cache!.producerBudget);

    const path = join(dir,cold.cache!.key+'.json');
    const original = readFileSync(path,'utf8');
    for (const mode of ['checksum','residual','source']) {
      cache.clear();
      const envelope = JSON.parse(original);
      const entry = JSON.parse(envelope.payload);
      if (mode==='source') entry.report.provenance.sourceMapping[0][1] = 'pads[1]';
      else entry.report.samples[0].columns[0].kclRelative = 2e-8;
      envelope.payload = JSON.stringify(entry);
      if (mode!=='checksum') envelope.checksum = digest(envelope.payload);
      writeFileSync(path,JSON.stringify(envelope));
      assert.equal(cachedBoardFoilReport(cache,'core-1',problem,produce).cache?.status,'cold');
    }
    assert.equal(calls,4);
    assert.throws(()=>cachedBoardFoilReport(cache,'core-1',replay,produce,performance.now()-20_000),/CHANNEL_FOIL_DEADLINE/);
    assert.equal(calls,4);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('exact foil cache keys the physical request and bounds replay size', () => {
  assert.equal(HYBRID_PLANAR_OPERATOR_VERSION, 'hybrid-planar-v11');
  const priorCompactPlanar = process.env.P7_COMPACT_PLANAR;
  const priorFullMixed = process.env.P7_COMPACT_FULL_MIXED;
  const dir = mkdtempSync(join(tmpdir(),'p7-foil-cache-'));
  try {
    delete process.env.P7_COMPACT_PLANAR;
    delete process.env.P7_COMPACT_FULL_MIXED;
    const cache = new LocalModelCache(dir);
    let capturedIdentity: any;
    const originalKey = cache.key.bind(cache);
    cache.key = (identity: unknown) => { capturedIdentity = identity; return originalKey(identity); };
    const run = (core: string, input: BoardFoilProblem) => cachedBoardFoilReport(cache,core,input,request=>mock(request,input));
    const cold = run('core-1',problem);
    assert.equal(capturedIdentity.hybridPlanarOperatorVersion, HYBRID_PLANAR_OPERATOR_VERSION);
    assert.notEqual(originalKey({...capturedIdentity,hybridPlanarOperatorVersion:'hybrid-planar-v0'}),cold.cache?.key);
    assert.equal(capturedIdentity.compactPlanarPolicy,'default-v1');
    process.env.P7_COMPACT_PLANAR='1';
    const compact = run('core-1',problem);
    assert.equal(capturedIdentity.compactPlanarPolicy,'compact-v1');
    assert.notEqual(compact.cache?.key,cold.cache?.key);
    assert.equal(capturedIdentity.compactBasisPolicy,'dc-harmonic-only');
    process.env.P7_COMPACT_FULL_MIXED='1';
    const fullMixed=run('core-1',problem);
    assert.equal(capturedIdentity.compactBasisPolicy,'adaptive-mixed-closure-v1');
    assert.notEqual(fullMixed.cache?.key,cache.key({...capturedIdentity,compactBasisPolicy:'full-mixed'}));
    assert.notEqual(fullMixed.cache?.key,compact.cache?.key);
    process.env.P7_COMPACT_FULL_MIXED='0';
    assert.equal(run('core-1',problem).cache?.key,compact.cache?.key);
    delete process.env.P7_COMPACT_PLANAR;
    const variants = [
      { ...problem, boardJson: '{"board":"different"}' },
      { ...problem, geometry: {...problem.geometry as object, airMarginM:0.002} },
      { ...problem, request:{...problem.request,frequenciesHz:[1000,3000]} },
      { ...problem, request:{...problem.request,referenceOhm:[75]} },
      { ...problem, request:{...problem.request,maxIterations:4} },
      { ...problem, request:{...problem.request,partitionPolicy:'automatic-conductive-schur-v1' as const} },
      { ...problem, expected:{...problem.expected,sourceBarrels:[{...problem.expected.sourceBarrels[0],layers:['TOP']}]} },
    ];
    for (const changed of variants) assert.notEqual(run('core-1',changed).cache?.key,cold.cache?.key);
    assert.notEqual(run('core-2',problem).cache?.key,cold.cache?.key);
    const verbose = {...problem,boardJson:'{"board":"verbose-report"}'};
    const large = cachedBoardFoilReport(cache,'core-1',verbose,request=>{
      const report = mock(request);
      report.provenance.method = 'x'.repeat(70_000);
      return report;
    });
    const replay = {...verbose,request:{...verbose.request,budget:{maxSeconds:10,maxMemoryMiB:1}}};
    const noSolve = () => { throw Error('unexpected cold preparation'); };
    assert.throws(()=>cachedBoardFoilReport(cache,'core-1',replay,noSolve),/LOCAL_CACHE_MEMORY_LIMIT/);
    cache.clear();
    assert.throws(()=>cachedBoardFoilReport(cache,'core-1',replay,noSolve),/LOCAL_CACHE_MEMORY_LIMIT/);
    assert.equal(cachedBoardFoilReport(cache,'core-1',verbose,noSolve).cache?.key,large.cache?.key);
  } finally {
    if(priorCompactPlanar===undefined)delete process.env.P7_COMPACT_PLANAR;else process.env.P7_COMPACT_PLANAR=priorCompactPlanar;
    if(priorFullMixed===undefined)delete process.env.P7_COMPACT_FULL_MIXED;else process.env.P7_COMPACT_FULL_MIXED=priorFullMixed;
    rmSync(dir,{recursive:true,force:true});
  }
});

test('skin-volume cache isolates frozen profiles and revalidates their mesh evidence', () => {
  const dir = mkdtempSync(join(tmpdir(),'p7-foil-skin-cache-'));
  try {
    const conductivitySM = 58e6;
    const input: BoardFoilProblem = {
      ...problem,
      geometry: { stack: ['TOP','BOTTOM'].map(name=>({name,copper:true,conductivitySM})), barrelConductivitySM:conductivitySM },
      request: { ...problem.request, frequenciesHz:[1000,20_000], operatorPolicy:'skin-volume-v3', profileUpperHz:100_000 },
    };
    const cache = new LocalModelCache(dir);
    let calls = 0;
    const produce = (request: AnalysisBoardFoilCheckedRequest) => {
      calls++;
      const report = mock(request,input);
      const requestedUpperHz = request.frequenciesHz.at(-1)!;
      const effectiveUpperHz = request.profileUpperHz ?? requestedUpperHz;
      const skinDepthM = 1/Math.sqrt(Math.PI*(4e-7*Math.PI)*effectiveUpperHz*conductivitySM);
      report.provenance.operatorPolicy = request.operatorPolicy;
      report.provenance.copperSkinMesh = {
        version:3, requestedUpperHz, trainingUpperBoundHz:requestedUpperHz, effectiveUpperHz, skinDepthDivisor:3,
        currentAndElectricShareMesh:true, scope:'Copper thickness and round barrel radial resolution',
        layers:['TOP','BOTTOM'].map(layer=>({layer,skinDepthM,maximumStepM:skinDepthM/4,slabs:2})),
        roundBarrels:[{id:'barrel-0',skinDepthM,maximumRadialStepM:skinDepthM/4,shells:2}],
      };
      return report;
    };
    const run = (request = input.request) => cachedBoardFoilReport(cache,'core-1',{...input,request},produce);
    const frozen = run();
    assert.equal(run().cache?.status,'memory');
    assert.notEqual(run({...input.request,profileUpperHz:undefined}).cache?.key,frozen.cache?.key);
    assert.equal(frozen.coverage,'unverified');
    for (const request of [
      {...input.request,operatorPolicy:undefined,profileUpperHz:undefined},
      {...input.request,profileUpperHz:19_999},
      {...input.request,profileUpperHz:100_001},
    ]) assert.throws(()=>run(request),/BOARD_FOIL_GRID/);

    cache.clear();
    const path = join(dir,frozen.cache!.key+'.json');
    const envelope = JSON.parse(readFileSync(path,'utf8'));
    const entry = JSON.parse(envelope.payload);
    entry.report.provenance.copperSkinMesh.roundBarrels[0].maximumRadialStepM *= 2;
    envelope.payload = JSON.stringify(entry);
    envelope.checksum = digest(envelope.payload);
    writeFileSync(path,JSON.stringify(envelope));
    assert.equal(run().cache?.status,'cold');
    assert.equal(calls,3);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('explicit high-memory board foil budget reaches producer without raising defaults',()=>{
 const dir=mkdtempSync(join(tmpdir(),'p7-high-memory-'));
 const before=process.env.P7_VERIFY_MEMORY_MIB;
 const stop=new Error('captured-budget');
 const input=structuredClone(problem);input.request.budget.maxMemoryMiB=16384;
 try{
  const cache=new LocalModelCache(dir);
  process.env.P7_VERIFY_MEMORY_MIB='20480';
  assert.throws(()=>cachedBoardFoilReport(cache,'high-budget',input,r=>{
   assert.equal(r.budget.maxMemoryMiB,16384);throw stop;
  }),e=>e===stop);
  delete process.env.P7_VERIFY_MEMORY_MIB;
  assert.throws(()=>cachedBoardFoilReport(cache,'default-budget',input,r=>{
   assert.equal(r.budget.maxMemoryMiB,2048);throw stop;
  }),e=>e===stop);
  for(const value of [NaN,Infinity,0,65537]){
   input.request.budget.maxMemoryMiB=value;
   assert.throws(()=>cachedBoardFoilReport(cache,'bad-budget',input,()=>assert.fail('producer')),/BOARD_FOIL_BUDGET/);
  }
  input.request.budget.maxMemoryMiB=16384;process.env.P7_VERIFY_MEMORY_MIB='65537';
  assert.throws(()=>cachedBoardFoilReport(cache,'bad-ceiling',input,()=>assert.fail('producer')),/LOCAL_BUDGET/);
 }finally{
  if(before===undefined)delete process.env.P7_VERIFY_MEMORY_MIB;else process.env.P7_VERIFY_MEMORY_MIB=before;
  rmSync(dir,{recursive:true,force:true});
 }
});

test('spatial recovery validates geometry, drive and loss before publication', () => {
 const good=()=>{
  const value=mock(problem.request);
  for(const sample of value.samples) sample.columns[0].spatialRecovery={status:'recovered-discrete',physicalAccuracy:'unverified',driveConvention:'1 A RMS matched Norton',copperLossW:12.5,supports:[{kind:'foil',layer:'TOP',triangle:0,zM:[0,1],xyM:[[0,0],[1,0],[0,1]],volumeM3:1,currentDensityRealAPerM2:[1,0,0],currentDensityImaginaryAPerM2:[0,1,0],lossW:12.5}]};
  return value;
 };
 const validate=(v:AnalysisBoardFoilCheckedReport)=>validateBoardFoilReport(v,problem,problem.request.budget);
 validate(good());
 for(const mutate of [
  (v:any)=>{v.samples[0].columns[0].spatialRecovery=null},
  (v:any)=>{v.samples[0].columns[0].spatialRecovery=false},
  (v:any)=>{v.samples[0].columns[0].spatialRecovery.supports=[]},
  (v:any)=>{v.samples[0].columns[0].spatialRecovery.driveConvention='unknown'},
  (v:any)=>{v.samples[0].columns[0].spatialRecovery.supports[0].xyM=[[0,0]]},
  (v:any)=>{v.samples[0].columns[0].spatialRecovery.supports[0].currentDensityRealAPerM2=null},
  (v:any)=>{v.samples[0].columns[0].spatialRecovery.supports[0].lossW=1},
 ]) {const bad=good();mutate(bad);assert.throws(()=>validate(bad),/CHANNEL_FOIL_EVIDENCE/);}
});
