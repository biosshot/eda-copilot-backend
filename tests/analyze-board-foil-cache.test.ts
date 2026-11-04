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
    samples:frequenciesHz.map(frequencyHz=>({frequencyHz,columns:[{input:0,kclRelative:0,kvlRelative:0,
      copperLossW:referenceOhm![0]/4,powerBalanceErrorW:0}]})),
    quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0},
    provenance:{
      modelKind:'mixed',method:'mock mixed',barrelIds:['barrel-0'],
      sourceMapping:input.expected.sourceCopper.map(({regionId,sourcePath,layer})=>[regionId,sourcePath,null,layer]),
      electrodeAreasM2:{a:1e-8,b:1e-8},maxCopperVolumeChange:0,prisms:3,retainedPotentials:4,currentModes:5,
      nativeCopperTetrahedra:18,volumeTetrahedra:9,fullPotentialCount:12,fullCurrentCount:54,
      plannedBytes:32*1048576,maxSeconds:budget.maxSeconds,maxMemoryMiB:budget.maxMemoryMiB,
      magneticQuadratureCoverage:'unverified',spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1 A RMS',
    },
  };
}

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
