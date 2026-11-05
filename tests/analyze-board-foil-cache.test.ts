import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../native/analyze/index.js';
import { cachedBoardFoilReport } from '../src/analyze/board-foil-cache.ts';
import { digest, LocalModelCache } from '../src/analyze/local-model-cache.ts';
import { isAnalysisProblem, type BoardFoilProblem } from '../src/analyze/types.ts';
import { runAutoFoil } from '../src/analyze/auto-foil-response.ts';
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
    ...(request.partitionPolicy&&{conductiveCorrection:{
      method:'sparse owner interiors and shared conductive trace Schur' as const,
      ownerInteriorDofs:[3,3],freeSharedTraceDofs:2,traceUpperEntries:3,ownedPlannedBytes:1024,
      maxOriginalFreeResidualRelative:0,scope:'Original-coordinate conductive correction',
    }}),
    provenance:{
    magneticOperator:{policy:'exact-streamed-v1',method:'unchanged direct tetrahedral pairs',scope:'Exact numerical pair action; physical accuracy unverified',maxFullKvlUncertaintyRelative:0},
      modelKind:'mixed',method:'mock mixed',barrelIds:['barrel-0'],
      ...(request.partitionPolicy&&{partitionPolicy:request.partitionPolicy,automaticPartition:{
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

test('automatic partition policy reaches native preparation and exact cache with checked evidence', () => {
  const dir=mkdtempSync(join(tmpdir(),'p7-automatic-partition-'));
  try {
    const cache=new LocalModelCache(dir);
    const plan={...problem.expected,boardJson:problem.boardJson,geometry:problem.geometry,
      ports:[{name:'original-port'}],attachments:[]} as unknown as AutoFoilPlan;
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
      value.provenance.magneticOperator={
        policy:'certified-cartesian-tree-v1',method:'symmetric dual-tree Newton expansion',
        scope:'Far truncation and arithmetic only; physical and near quadrature unverified',
        expansionOrder:10,separationRatioMaximum:0.2,treeNodes:3,exactPairs:18,farClusterPairs:1,
        representationVersion:2,harmonicBasis:'laplace-quotient-z01',momentCoefficientCount:121,
        m2lCouplings:2926,shiftStencilTerms:2108,axisSerial:true,leafTetrahedra:1,
        plannedBytes:1024,treeSignature:'fnv1a64:0123456789abcdef',maxFullKvlUncertaintyRelative:5e-10,
      };
      for(const sample of value.samples)for(const column of sample.columns){
        column.kvlRelative=1e-9;column.kvlUncertaintyRelative=5e-10;
      }
      value.quality.maxKvlRelative=1e-9;
      value.quality.maxFullKvlUncertaintyRelative=5e-10;
      return value;
    };
    assert.equal(runAutoFoil(plan,[1000,2000],problem.request.budget,undefined,tree)
      .provenance.magneticOperator?.policy,'certified-cartesian-tree-v1');
    const changedTree=runAutoFoil(plan,[1000,2000],problem.request.budget,[1000,2000],input=>{
      const value=tree(input);
      if(input.request.frequenciesHz.length===1&&value.provenance.magneticOperator?.policy==='certified-cartesian-tree-v1')
        value.provenance.magneticOperator.treeSignature='fnv1a64:fedcba9876543210';
      return value;
    });
    assert.ok(changedTree.frequencyAudit?.batch.status==='refused');
    assert.match(changedTree.frequencyAudit.batch.message??'',/CHANNEL_FOIL_FREQUENCY_MESH/);
    for(const corrupt of [
      (value:AnalysisBoardFoilCheckedReport)=>{delete value.provenance.magneticOperator;},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.provenance.magneticOperator!,{representationVersion:1});},
      (value:AnalysisBoardFoilCheckedReport)=>{Object.assign(value.provenance.magneticOperator!,{momentCoefficientCount:286});},
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
  const dir = mkdtempSync(join(tmpdir(),'p7-foil-cache-'));
  try {
    const cache = new LocalModelCache(dir);
    const run = (core: string, input: BoardFoilProblem) => cachedBoardFoilReport(cache,core,input,request=>mock(request,input));
    const cold = run('core-1',problem);
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
  } finally { rmSync(dir,{recursive:true,force:true}); }
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
