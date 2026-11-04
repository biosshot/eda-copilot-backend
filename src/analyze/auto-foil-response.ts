import type { AutoFoilPlan } from './auto-foil-plan.ts';
import { auditBoardFoilGrid } from './board-foil-sensitivity.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';
import type { FrequencyInterval } from './p7-frequency-planning.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';

export type BoardFoilRunReport = BoardFoilReport & { jointSensitivity?: ReturnType<typeof auditBoardFoilGrid> };

export function runAutoFoil(
 plan:AutoFoilPlan, frequenciesHz:number[], budget:{maxSeconds:number;maxMemoryMiB:number},
 interval:FrequencyInterval|undefined, execute:(problem:BoardFoilProblem)=>BoardFoilReport,
): BoardFoilRunReport {
 const started=performance.now();
 const n=plan.ports.length;
 if(!Number.isFinite(budget.maxSeconds)||budget.maxSeconds<=0)
  throw new Error('CHANNEL_FOIL_DEADLINE: shared budget exhausted before the foil solve');
 if(!n||n>16||!frequenciesHz.length||frequenciesHz.length*n>256||
    [...frequenciesHz,...(interval??[])].some(f=>!Number.isFinite(f)||f<1000||f>10000)||
    frequenciesHz.some((f,i)=>i>0&&f<=frequenciesHz[i-1]))
  throw new Error('CHANNEL_FOIL_GRID: bounded ordered 1-10 kHz grid with at most 256 frequency/port columns required');
 for(const attachment of plan.attachments){
  const band=declaredNetworkBand(attachment.model);
  if(band&&[...frequenciesHz,...(interval??[])].some(f=>f<band[0]||f>band[1]))
   throw new Error('CHANNEL_FOIL_COMPONENT_BAND: requested demand exceeds package applicability for '+attachment.id);
 }
 const referenceOhm=plan.ports.map(()=>50);
 const problem: BoardFoilProblem = {kind:'board-foil-checked',boardJson:plan.boardJson,geometry:plan.geometry,
  expected:{electrodePairs:plan.electrodePairs,sourceCopper:plan.sourceCopper,sourceBarrels:plan.sourceBarrels},
  request:{frequenciesHz,referenceOhm,budget,...(plan.attachments.length&&{attachments:plan.attachments})}};
 const report=execute(problem);
 validateBoardFoilReport(report, problem, report.cache?.producerBudget ?? budget);
 return process.env.P7_AUTO_FOIL_AUDIT==='1'
  ? {...report,jointSensitivity:auditBoardFoilGrid(problem,report,started,execute)}
  : report;
}
