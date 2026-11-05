import test from 'node:test';
import assert from 'node:assert/strict';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';
import { boardFoilResult } from '../src/analyze/analysis-dsl/board-foil-result.ts';
import { runAutoFoil } from '../src/analyze/auto-foil-response.ts';
import { auditBoardFoilGrid } from '../src/analyze/board-foil-sensitivity.ts';
import type { BoardFoilProblem, BoardFoilReport } from '../src/analyze/types.ts';

const plan: AutoFoilPlan = {
  boardJson:'{}', geometry:{ maxTriangleAreaM2:1e-8, airMarginM:.002, electrodes:[], allowInsulatedCopperCuts:false },
  roiMm:[[0,0],[1,1]], terminals:[
    {id:'a',sourcePath:'pads[0]',layer:'TOP'}, {id:'b',sourcePath:'pads[1]',layer:'BOTTOM'},
  ],
  ports:[{name:'input',positive:'a',negative:'b'}], electrodePairs:[['a','b']],
  sourceCopper:[{regionId:'top',sourcePath:'pads[0]',layer:'TOP'}], sourceBarrels:[], attachments:[],
};
const budget = {maxSeconds:20,maxMemoryMiB:256};

function report(problem: BoardFoilProblem, phase = 0, mixed = false): BoardFoilReport {
  const { frequenciesHz, referenceOhm, budget } = problem.request;
  const partitionPolicy=problem.request.partitionPolicy;
  mixed=mixed||Boolean(partitionPolicy);
  return {
    status:'complete', coverage:'unverified', representation:'S', frequenciesHz, referenceOhm:referenceOhm!,
    ports:problem.expected.electrodePairs, parameters:frequenciesHz.map(()=>[[{real:.5*Math.cos(phase),imaginary:.5*Math.sin(phase)}]]),
    enrichment:[],
    samples:frequenciesHz.map(frequencyHz=>({frequencyHz,columns:[{input:0,kclRelative:0,kvlRelative:0,...(mixed&&{kvlUncertaintyRelative:0}),copperLossW:9.375,powerBalanceErrorW:0}]})),
    quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0,...(mixed&&{maxFullKvlUncertaintyRelative:0})},
    provenance:{
      ...(partitionPolicy&&{partitionPolicy,automaticPartition:{policy:partitionPolicy,status:'not-applicable' as const,
        reason:'Tiny mock has no private owner interiors',algorithm:'longest-extent-midpoint-v1' as const,
        meshOwnershipSignature:'fnv1a64:0123456789abcdef',owners:2,originalCopperTetrahedra:3,
        crossOwnerFaces:1,freeSharedTraceDofs:0,ownerInteriorDofs:[0,0]}}),
    ...(mixed&&{magneticOperator:{policy:'exact-streamed-v1' as const,method:'unchanged direct tetrahedral pairs' as const,scope:'Exact numerical pair action; physical accuracy unverified',maxFullKvlUncertaintyRelative:0}}),
      method:'mock', barrelIds:[], sourceMapping:[['top','pads[0]',null,'TOP']], electrodeAreasM2:{a:1e-8,b:1e-8},
      maxCopperVolumeChange:0, retainedPotentials:2, currentModes:2, plannedBytes:1024, ...budget,
      spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1 A RMS Norton',
      ...(mixed ? {modelKind:'mixed' as const,prisms:0,nativeCopperTetrahedra:3,volumeTetrahedra:3,
        fullPotentialCount:4,fullCurrentCount:9,magneticQuadratureCoverage:'unverified' as const}
        :{modelKind:'foil' as const,prisms:1,maxMagneticReciprocity:0,maxMagneticRefinement:0,maxElectricReciprocity:0}),
    },
  };
}

test('automatic joint sensitivity is opt-in, reuses one physical request and retains the baseline', () => {
  const previous=process.env.P7_AUTO_FOIL_AUDIT;
  const calls:BoardFoilProblem[]=[];
  let failDomain = true;
  const execute = (problem:BoardFoilProblem) => {
    calls.push(problem);
    const {maxTriangleAreaM2,airMarginM} = problem.geometry as typeof plan.geometry;
    if (maxTriangleAreaM2===5e-9) return report(problem,0.025,true);
    if (airMarginM===.004) {
      if (failDomain) throw new Error('MEMORY_LIMIT: bounded exterior refusal');
      return report(problem,0.045);
    }
    return report(problem);
  };
  try {
    delete process.env.P7_AUTO_FOIL_AUDIT;
    const normal=runAutoFoil(plan,[1000,3000],budget,undefined,execute);
    assert.equal(calls.length,1);
    assert.equal(normal.jointSensitivity,undefined);
    calls.length=0;
    process.env.P7_AUTO_FOIL_AUDIT='1';
    const audited=runAutoFoil(plan,[1000,3000],budget,undefined,execute);
    assert.equal(calls.length,3);
    const [base,mesh,domain]=calls;
    assert.deepEqual(mesh.geometry,{...plan.geometry,maxTriangleAreaM2:5e-9});
    assert.deepEqual(domain.geometry,{...plan.geometry,airMarginM:.004});
    for(const variant of [mesh,domain]){
      assert.equal(variant.boardJson,base.boardJson);
      assert.deepEqual(variant.expected,base.expected);
      assert.deepEqual(variant.request,{...base.request,budget:variant.request.budget});
      assert.equal(variant.request.budget.maxMemoryMiB,budget.maxMemoryMiB);
      assert.ok(variant.request.budget.maxSeconds<budget.maxSeconds);
    }
    assert.ok(domain.request.budget.maxSeconds<=mesh.request.budget.maxSeconds);
    assert.deepEqual(audited.parameters,normal.parameters);
    assert.equal(audited.coverage,'unverified');
    const result=boardFoilResult('test',plan,audited,[]);
    assert.equal(result.status,'partial');
    assert.deepEqual(result.diagnostics.map(d=>d.code),[
      'CHANNEL_FOIL_SENSITIVITY_REVIEW','CHANNEL_FOIL_SENSITIVITY_UNVERIFIED','CHANNEL_FOIL_PHYSICAL_UNVERIFIED',
    ]);
    const validityDomain=result.network?.validity?.domain as Record<string,unknown>;
    assert.equal(validityDomain.bandCoverage,'unverified');
    assert.equal(validityDomain.spatialCoverage,'unverified');
    assert.deepEqual(validityDomain.jointSensitivity,[
      {kind:'mesh',status:'requires-review'},{kind:'exterior-domain',status:'unverified'},
    ]);
    const [meshCheck,domainCheck]=audited.jointSensitivity!.checks;
    assert.equal(meshCheck.status,'requires-review');
    assert.ok('producer'in meshCheck && meshCheck.producer?.modelKind==='mixed');
    assert.equal(domainCheck.status,'unverified');
    assert.ok('failure'in domainCheck && domainCheck.failure?.code==='MEMORY_LIMIT');
    assert.notEqual(audited.jointSensitivity!.baseline.geometrySha256,meshCheck.producer.geometrySha256);

    calls.length=0;
    failDomain=false;
    const wide=runAutoFoil(plan,[1000],budget,undefined,execute);
    assert.equal(calls.length,3);
    const wideDomain=wide.jointSensitivity!.checks[1];
    assert.equal(wideDomain.status,'unverified');
    assert.ok('reason' in wideDomain);
    assert.match(wideDomain.reason??'',/exceeds/);
    const timed=auditBoardFoilGrid(calls[0],wide,performance.now()-30_000,()=>assert.fail('unexpected retry'));
    assert.deepEqual(timed.checks.map(c=>'failure' in c ? c.failure.code : undefined),['CHANNEL_FOIL_DEADLINE','CHANNEL_FOIL_DEADLINE']);
  } finally {
    if(previous===undefined)delete process.env.P7_AUTO_FOIL_AUDIT;
    else process.env.P7_AUTO_FOIL_AUDIT=previous;
  }
});
