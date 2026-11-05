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

function physicalMesh(report: BoardFoilReport) {
  const p=report.provenance,skin=p.copperSkinMesh;
  const magnetic=p.magneticOperator;
  return canonical({
    modelKind:p.modelKind,operatorPolicy:p.operatorPolicy,prisms:p.prisms,
    partitionPolicy:p.partitionPolicy,automaticPartition:p.automaticPartition,
    ...(magnetic&&{magneticOperator:magnetic.policy==='exact-streamed-v1'
      ?{policy:magnetic.policy,method:magnetic.method}
      :{policy:magnetic.policy,method:magnetic.method,expansionOrder:magnetic.expansionOrder,
        representationVersion:magnetic.representationVersion,harmonicBasis:magnetic.harmonicBasis,
        interactionStorage:magnetic.interactionStorage,visitedNodePairs:magnetic.visitedNodePairs,
        interactionStackSlots:magnetic.interactionStackSlots,interactionLevels:magnetic.interactionLevels,
        interactionReplaysPerAction:magnetic.interactionReplaysPerAction,
        momentCoefficientCount:magnetic.momentCoefficientCount,m2lCouplings:magnetic.m2lCouplings,
        shiftStencilTerms:magnetic.shiftStencilTerms,axisSerial:magnetic.axisSerial,leafTetrahedra:magnetic.leafTetrahedra,
        separationRatioMaximum:magnetic.separationRatioMaximum,treeNodes:magnetic.treeNodes,
        exactPairs:magnetic.exactPairs,farClusterPairs:magnetic.farClusterPairs,treeSignature:magnetic.treeSignature}}),
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
  const deadlineMs=started+1000*maxSeconds;
  const seeded=new Map(baseline.frequenciesHz.flatMap((f,i)=>
    requestedIntervalHz[0]<=f&&f<=requestedIntervalHz[1] ? [[f,observations(baseline,i)] as const] : []));
  const gates=Object.fromEntries(Object.keys(observations(baseline,0)).map(name=>[name,{
    absoluteFloor:name.startsWith('S:')?p7LowBandAuditPolicy.waveFloor:p7LowBandAuditPolicy.lossFloorW,
    relativeTolerance:p7LowBandAuditPolicy.relativeTolerance,
  }]));
  let batch: {status:'checked'|'refused'; frequenciesHz:number[]; cache?:BoardFoilReport['cache']; message?:string} | undefined;
  const interpolation=auditFrequencyCoverage({
    demand:{spectralBandHz:requestedIntervalHz},supportedIntervalsHz:[applicabilityBandHz],
    transitionFrequenciesHz:[...seeded.keys()],initialSamples:seeded,deadlineMs,
    policy:{maxSamples:seeded.size+maxFreshSamples,maxStepHz:p7LowBandAuditPolicy.maxStepHz,gates,
      modelVersion:'joint-foil-checked-frequency-audit-v1'},
    evaluatePhysicalBatch(frequenciesHz) {
      batch={status:'refused',frequenciesHz};
      try{
        const maxSeconds=(deadlineMs-performance.now())/1000;
        if(maxSeconds<=0)throw new Error('CHANNEL_FOIL_DEADLINE: shared frequency audit budget exhausted');
        const changed:BoardFoilProblem={...problem,request:{...problem.request,frequenciesHz,
          budget:{...problem.request.budget,maxSeconds}}};
        const report=execute(changed);
        validateBoardFoilReport(report,changed,report.cache?.producerBudget??changed.request.budget);
        if(physicalMesh(report)!==physicalMesh(baseline))
          throw new Error('CHANNEL_FOIL_FREQUENCY_MESH: supplemental solve changed the original physical mesh or ownership');
        if(performance.now()>=deadlineMs)throw new Error('CHANNEL_FOIL_DEADLINE: batch exceeded shared frequency audit budget');
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
  return {
    applicabilityBandHz, maxFreshSamples,
    batch:batch??{status:'not-attempted' as const},
    interpolation:{...interpolation,holdouts,samples:interpolation.samples.filter(s=>!seeded.has(s.frequencyHz))},
    physicalCoverage:{status:'unverified' as const,reason:'Independent joint board frequency reference unavailable'},
    scope:'Supplemental exact physical points are audit evidence only. The network replays its original exact grid. Interpolation sensitivity does not admit continuous replay or physical-band accuracy.',
  };
}
