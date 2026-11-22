import test from 'node:test';
import assert from 'node:assert/strict';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';
import { boardFoilResult } from '../src/analyze/analysis-dsl/board-foil-result.ts';
import { runAutoFoil } from '../src/analyze/auto-foil-response.ts';
import { auditBoardFoilGrid } from '../src/analyze/board-foil-sensitivity.ts';
import type { BoardFoilProblem, BoardFoilReport } from '../src/analyze/types.ts';

const plan: AutoFoilPlan = {
  boardJson:'{}', geometry:{ stack:[{name:'TOP',copper:true,conductivitySM:58e6},{name:'BOTTOM',copper:true,conductivitySM:58e6}], maxTriangleAreaM2:1e-8, airMarginM:.002, electrodes:[], allowInsulatedCopperCuts:false },
  roiMm:[[0,0],[1,1]], terminals:[
    {id:'a',sourcePath:'pads[0]',layer:'TOP'}, {id:'b',sourcePath:'pads[1]',layer:'BOTTOM'},
  ],
  ports:[{name:'input',positive:'a',negative:'b'}], electrodePairs:[['a','b']],
  sourceCopper:[{regionId:'top',sourcePath:'pads[0]',layer:'TOP'}], sourceBarrels:[], attachments:[],
};
const budget = {maxSeconds:20,maxMemoryMiB:256};

function skinMesh(problem: BoardFoilProblem) {
  // Auto-foil requests always select the hybrid planar policy, whose report must carry the shared skin-mesh record.
  const top=problem.request.frequenciesHz.at(-1)!, upper=problem.request.profileUpperHz??top;
  const depth=1/Math.sqrt(Math.PI*upper*(4e-7*Math.PI)*58e6);
  return {operatorPolicy:'hybrid-planar-v3' as const,copperSkinMesh:{version:3 as const,requestedUpperHz:top,trainingUpperBoundHz:top,effectiveUpperHz:upper,
    skinDepthDivisor:3 as const,currentAndElectricShareMesh:true as const,scope:'shared mesh',
    layers:['TOP','BOTTOM'].map(layer=>({layer,skinDepthM:depth,maximumStepM:depth/6,slabs:1})),roundBarrels:[]}};
}

/** Auto-foil's automatic-hybrid policy is validated against compact-operator evidence, not the legacy exact-streamed record. */
function compactEvidence(result: BoardFoilReport): BoardFoilReport {
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
  return result;
}

function report(problem: BoardFoilProblem, phase = 0, mixed = false): BoardFoilReport {
  const result=rawReport(problem,phase,mixed);
  return problem.request.partitionPolicy==='automatic-hybrid-v1'?compactEvidence(result):result;
}

function rawReport(problem: BoardFoilProblem, phase = 0, mixed = false): BoardFoilReport {
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
      ...(problem.request.operatorPolicy==='hybrid-planar-v3'&&skinMesh(problem)),
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
      // The automatic-hybrid policy runs under external resource supervision: variants keep the full request budget.
      assert.deepEqual(variant.request,base.request);
      assert.equal(variant.request.budget.maxMemoryMiB,budget.maxMemoryMiB);
      assert.equal(variant.request.budget.maxSeconds,budget.maxSeconds);
    }
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
    // The wall-clock deadline is enforced only for in-process (non-hybrid) solves; hybrid ones are supervised externally.
    const inProcess:BoardFoilProblem={...calls[0],request:{...calls[0].request,partitionPolicy:undefined,operatorPolicy:undefined}};
    const timed=auditBoardFoilGrid(inProcess,wide,performance.now()-30_000,()=>assert.fail('unexpected retry'));
    assert.deepEqual(timed.checks.map(c=>'failure' in c ? c.failure.code : undefined),['CHANNEL_FOIL_DEADLINE','CHANNEL_FOIL_DEADLINE']);
  } finally {
    if(previous===undefined)delete process.env.P7_AUTO_FOIL_AUDIT;
    else process.env.P7_AUTO_FOIL_AUDIT=previous;
  }
});
