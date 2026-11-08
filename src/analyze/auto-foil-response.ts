import type { AutoFoilPlan } from './auto-foil-plan.ts';
import { auditBoardFoilFrequency } from './board-foil-frequency-audit.ts';
import { auditBoardFoilGrid } from './board-foil-sensitivity.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';
import { requestedFrequencyInterval, type FrequencyInterval } from './p7-frequency-planning.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';
import { boardChannelBlockBandHz, boardChannelJointSkinBandHz } from './board-channel-plan.ts';

export type BoardFoilRunReport = BoardFoilReport & {
 jointSensitivity?: ReturnType<typeof auditBoardFoilGrid>;
 frequencyAudit?: ReturnType<typeof auditBoardFoilFrequency>;
};

export function runAutoFoil(
 plan:AutoFoilPlan, frequenciesHz:number[], budget:{maxSeconds:number;maxMemoryMiB:number},
 interval:FrequencyInterval|undefined, execute:(problem:BoardFoilProblem)=>BoardFoilReport,
): BoardFoilRunReport {
 const started=performance.now();
 const n=plan.ports.length;
 const requested=interval&&requestedFrequencyInterval({spectralBandHz:interval});
 const highestSampleHz=frequenciesHz.at(-1)??0;
 const demandUpperHz=Math.max(requested?.[1]??0,highestSampleHz);
 const extendedDemand=demandUpperHz>boardChannelBlockBandHz[1];
 const foilBandHz:FrequencyInterval=extendedDemand?boardChannelJointSkinBandHz:boardChannelBlockBandHz;
 if(!Number.isFinite(budget.maxSeconds)||budget.maxSeconds<=0)
  throw new Error('CHANNEL_FOIL_DEADLINE: shared budget exhausted before the foil solve');
 if(!n||n>16||!frequenciesHz.length||frequenciesHz.length*n>256||
    frequenciesHz.some(f=>!Number.isFinite(f)||f<foilBandHz[0]||f>foilBandHz[1])||
    frequenciesHz.some((f,i)=>i>0&&f<=frequenciesHz[i-1]))
  throw new Error('CHANNEL_FOIL_GRID: bounded ordered '+foilBandHz[0]+'..'+foilBandHz[1]+' Hz grid with at most 256 frequency/port columns required');
 let [low,high]=foilBandHz;
 for(const attachment of plan.attachments){
  const band=declaredNetworkBand(attachment.model);
  if(!band)continue;
  if(frequenciesHz.some(f=>f<band[0]||f>band[1]))
   throw new Error('CHANNEL_FOIL_COMPONENT_BAND: requested sample lies outside package applicability for '+attachment.id);
  low=Math.max(low,band[0]);
  high=Math.min(high,band[1]);
 }
 // Geometry and exact frequency batches share one mesh profile derived from
 // admitted demand, including points that may only be evaluated in the audit.
 const profileUpperHz=Math.min(demandUpperHz,high);
 const skin=profileUpperHz>boardChannelBlockBandHz[1];
 const referenceOhm=plan.ports.map(()=>50);
 const problem: BoardFoilProblem = {kind:'board-foil-checked',boardJson:plan.boardJson,geometry:plan.geometry,
  expected:{electrodePairs:plan.electrodePairs,sourceCopper:plan.sourceCopper,sourceBarrels:plan.sourceBarrels},
  request:{frequenciesHz,referenceOhm,budget,partitionPolicy:process.env.P7_COMPACT_PLANAR==='1'?'automatic-hybrid-v1':'automatic-conductive-schur-v1',
   ...(plan.attachments.length&&{attachments:plan.attachments}),
   ...(skin&&{operatorPolicy:'skin-volume-v3' as const,profileUpperHz})}};
 const report=execute(problem);
 validateBoardFoilReport(report, problem, report.cache?.producerBudget ?? budget);
 const frequencyAudit=requested&&auditBoardFoilFrequency(problem,report,requested,[low,high],started,execute);
 const jointSensitivity=process.env.P7_AUTO_FOIL_AUDIT==='1'&&auditBoardFoilGrid(problem,report,started,execute);
 return frequencyAudit||jointSensitivity?{...report,...(frequencyAudit&&{frequencyAudit}),...(jointSensitivity&&{jointSensitivity})}:report;
}
