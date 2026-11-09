import { canonical } from './local-model-cache.ts';
import { auditFrequencyCoverage, type FrequencyInterval } from './p7-frequency-planning.ts';
import { p7LowBandAuditPolicy } from './segmented-frequency-audit.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';

type Value = Readonly<Record<string, readonly [number, number]>>;
type Execute = (problem: BoardFoilProblem) => BoardFoilReport;

function observations(report: BoardFoilReport, index: number): Value {
  const values: Record<string, readonly [number, number]> = {};
  for (const [output, row] of report.parameters[index].entries()) for (const [input, wave] of row.entries()) {
    values['S:'+output+':'+input] = [wave.real,wave.imaginary];
  }
  for (const [input, column] of report.samples[index].columns.entries()) {
    values['copperLossW:'+input] = [column.copperLossW,0];
    for (const [i, item] of (column.packages ?? []).entries()) {
      values['packageLossW:'+input+':'+i] = [item.lossW,0];
    }
  }
  return values;
}

function physicalMeshIdentity(report: BoardFoilReport) {
  const p=report.provenance,skin=p.copperSkinMesh;
  const magnetic=p.magneticOperator;
  const extended=p as unknown as Record<string,unknown>;
  const compact=extended.compactOperator as Record<string,unknown>|undefined;
  const hybrid=extended.hybridAssembly as Record<string,unknown>|undefined;
  return canonical({
    modelKind:p.modelKind,operatorPolicy:p.operatorPolicy,prisms:p.prisms,
    // Include only stable representation identity; frequency-specific quadrature
    // measurements and hints are expected to vary between valid exact batches.
    compactOperator:compact&&{
      version:compact.version,basis:compact.basis,electricOperator:compact.electricOperator,
      foilSupports:compact.foilSupports,localTetraSupports:compact.localTetraSupports,
      potentialCoordinates:compact.potentialCoordinates,currentCoordinates:compact.currentCoordinates,
    },
    hybridAssembly:hybrid&&{
      policy:hybrid.policy,method:hybrid.method,potentialCoordinates:hybrid.potentialCoordinates,
      currentCoordinates:hybrid.currentCoordinates,
    },
    partitionPolicy:p.partitionPolicy,automaticPartition:p.automaticPartition,
    ...(magnetic&&{magneticOperator:magnetic.policy==='exact-streamed-v1'
      ?{policy:magnetic.policy,method:magnetic.method}
      :{policy:magnetic.policy,method:magnetic.method,expansionOrder:magnetic.expansionOrder,
        representationVersion:magnetic.representationVersion,harmonicBasis:magnetic.harmonicBasis,
        interactionStorage:magnetic.interactionStorage,
        interactionStackSlots:magnetic.interactionStackSlots,interactionLevels:magnetic.interactionLevels,
        interactionReplaysPerAction:magnetic.interactionReplaysPerAction,
        momentCoefficientCount:magnetic.momentCoefficientCount,m2lCouplings:magnetic.m2lCouplings,
        shiftStencilTerms:magnetic.shiftStencilTerms,axisSerial:magnetic.axisSerial,leafTetrahedra:magnetic.leafTetrahedra,
        minimumExpansionOrder:magnetic.minimumExpansionOrder,admissibility:magnetic.admissibility,
        separationRatioMaximum:magnetic.separationRatioMaximum,treeNodes:magnetic.treeNodes,
        geometrySignature:magnetic.geometrySignature,treeSignature:magnetic.treeSignature}}),
    sourceMapping:p.sourceMapping,barrelIds:p.barrelIds,electrodeAreasM2:p.electrodeAreasM2,
    ...(p.modelKind==='mixed'&&{volumeTetrahedra:p.volumeTetrahedra,nativeCopperTetrahedra:p.nativeCopperTetrahedra,
      fullPotentialCount:p.fullPotentialCount,fullCurrentCount:p.fullCurrentCount}),
    ...(skin&&{skin:{version:skin.version,effectiveUpperHz:skin.effectiveUpperHz,skinDepthDivisor:skin.skinDepthDivisor,
      currentAndElectricShareMesh:skin.currentAndElectricShareMesh,layers:skin.layers,roundBarrels:skin.roundBarrels}}),
  });
}

export function auditBoardFoilFrequency(
  problem: BoardFoilProblem, baseline: BoardFoilReport, requestedIntervalHz: FrequencyInterval,
  applicabilityBandHz: FrequencyInterval, started: number, execute: Execute,
) {
  const n=problem.expected.electrodePairs.length;
  const maxFreshSamples=Math.min(8,Math.floor(64/n));
  const maxSeconds=Math.min(problem.request.budget.maxSeconds,Number(process.env.P7_VERIFY_SECONDS??300));
  const externalResources=problem.request.partitionPolicy==='automatic-hybrid-v1';
  const deadlineMs=externalResources?undefined:started+1000*maxSeconds;
  const seeded=new Map(baseline.frequenciesHz.flatMap((f,i)=>
    requestedIntervalHz[0]<=f&&f<=requestedIntervalHz[1] ? [[f,observations(baseline,i)] as const] : []));
  const gates=Object.fromEntries(Object.keys(observations(baseline,0)).map(name=>[name,{
    absoluteFloor:name.startsWith('S:')?p7LowBandAuditPolicy.waveFloor:p7LowBandAuditPolicy.lossFloorW,
    relativeTolerance:p7LowBandAuditPolicy.relativeTolerance,
  }]));
  let batch: {status:'checked'|'refused'; frequenciesHz:number[]; cache?:BoardFoilReport['cache']; message?:string} | undefined;
  let supplementalResponses: Array<{frequencyHz:number;parameters:BoardFoilReport['parameters'][number];samples:BoardFoilReport['samples'][number];referenceOhm:number[]}> = [];
  const interpolation=auditFrequencyCoverage({
    demand:{spectralBandHz:requestedIntervalHz},supportedIntervalsHz:[applicabilityBandHz],
    transitionFrequenciesHz:[...seeded.keys()],initialSamples:seeded,deadlineMs,
    policy:{maxSamples:seeded.size+maxFreshSamples,maxStepHz:p7LowBandAuditPolicy.maxStepHz,gates,
      modelVersion:'joint-foil-checked-frequency-audit-v1'},
    evaluatePhysicalBatch(frequenciesHz) {
      batch={status:'refused',frequenciesHz};
      try{
        const maxSeconds=deadlineMs===undefined?problem.request.budget.maxSeconds:(deadlineMs-performance.now())/1000;
        if(deadlineMs!==undefined&&maxSeconds<=0)throw new Error('CHANNEL_FOIL_DEADLINE: shared frequency audit budget exhausted');
        const changed:BoardFoilProblem={...problem,request:{...problem.request,frequenciesHz,
          budget:{...problem.request.budget,maxSeconds}}};
        const report=execute(changed);
        validateBoardFoilReport(report,changed,report.cache?.producerBudget??changed.request.budget);
        if(physicalMeshIdentity(report)!==physicalMeshIdentity(baseline))
          throw new Error('CHANNEL_FOIL_FREQUENCY_MESH: supplemental solve changed the original physical mesh or ownership');
        if(deadlineMs!==undefined&&performance.now()>=deadlineMs)throw new Error('CHANNEL_FOIL_DEADLINE: batch exceeded shared frequency audit budget');
        // Retain only the requested exact response payload, not whole reports or provenance trees.
        supplementalResponses.push(...frequenciesHz.map((frequencyHz,i)=>({frequencyHz,
          parameters:report.parameters[i],samples:report.samples[i],referenceOhm:report.referenceOhm})));
        batch={status:'checked',frequenciesHz,cache:report.cache};
        return new Map(frequenciesHz.map((f,i)=>[f,observations(report,i)]));
      }catch(error){
        batch.message=error instanceof Error?error.message:String(error);
        throw error;
      }
    },
  });
  const holdouts=interpolation.holdouts.map(holdout=>{
    const maxRelativeError=Math.max(...Object.values(holdout.normalizedErrors));
    const status=maxRelativeError<=p7LowBandAuditPolicy.relativeTolerance
      ? holdout.accepted?'pass' as const:'unverified' as const
      : maxRelativeError<=0.035?'requires-review' as const:'unverified' as const;
    return {...holdout,maxRelativeError,status};
  });
  const baselineResponses=baseline.frequenciesHz.map((frequencyHz,i)=>({frequencyHz,
    parameters:baseline.parameters[i],samples:baseline.samples[i],referenceOhm:baseline.referenceOhm}));
  const adaptiveByFrequency=new Map<number,typeof baselineResponses[number]>();
  for(const point of [...baselineResponses,...supplementalResponses])adaptiveByFrequency.set(point.frequencyHz,point);
  const adaptiveResponse={
    points:[...adaptiveByFrequency.values()].sort((a,b)=>a.frequencyHz-b.frequencyHz),
    independentHoldoutFrequenciesHz:holdouts.map(({frequencyHz,status,maxRelativeError,accepted})=>({frequencyHz,status,maxRelativeError,accepted})),
    physicalCoverage:{status:'unverified' as const,reason:'Exact evaluated points and interpolation holdouts do not establish continuous-band or independent physical accuracy.'},
  };
  return {
    applicabilityBandHz, maxFreshSamples,
    batch:batch??{status:'not-attempted' as const},
    interpolation:{...interpolation,holdouts,samples:interpolation.samples.filter(s=>!seeded.has(s.frequencyHz))},
    adaptiveResponse,
    physicalCoverage:{status:'unverified' as const,reason:'Independent joint board frequency reference unavailable'},
    scope:'Supplemental exact physical points are audit evidence only. The network replays its original exact grid. Interpolation sensitivity does not admit continuous replay or physical-band accuracy.',
  };
}
