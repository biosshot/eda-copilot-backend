import type { AutoFoilPlan } from './auto-foil-plan.ts';
import { auditBoardFoilFrequency } from './board-foil-frequency-audit.ts';
import { auditBoardFoilGrid } from './board-foil-sensitivity.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';
import { requestedFrequencyInterval, type FrequencyInterval } from './p7-frequency-planning.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';
import { boardChannelBlockBandHz, boardChannelHybridBandHz } from './board-channel-plan.ts';
import { summarizeReturnBoundary, RETURN_BOUNDARY_THRESHOLD, RETURN_CURRENT_FLOOR_A } from './return-boundary.ts';

type ReturnSearchStep = ReturnType<typeof summarizeReturnBoundary> & {
 domainScale:number; observationBoundsMm:unknown; solvedRoiMm:AutoFoilPlan['roiMm'];
};

export type BoardFoilRunReport = BoardFoilReport & {
 jointSensitivity?: ReturnType<typeof auditBoardFoilGrid>;
 frequencyAudit?: ReturnType<typeof auditBoardFoilFrequency>;
 returnSearch?: {policy:'internal-guard-flux-v1';status:'passed';threshold:number;absoluteFloorA:number;steps:ReturnSearchStep[]};
 /** Non-enumerable caller context. It never enters serialized reports. */
 resolvedPlan?:AutoFoilPlan;
};

export function runAutoFoil(
 plan:AutoFoilPlan, frequenciesHz:number[], budget:{maxSeconds:number;maxMemoryMiB:number},
 interval:FrequencyInterval|undefined, execute:(problem:BoardFoilProblem)=>BoardFoilReport,
): BoardFoilRunReport {
 const n=plan.ports.length;
 const requested=interval&&requestedFrequencyInterval({spectralBandHz:interval});
 const highestSampleHz=frequenciesHz.at(-1)??0;
 const demandUpperHz=Math.max(requested?.[1]??0,highestSampleHz);
 const foilBandHz:FrequencyInterval=boardChannelHybridBandHz;
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
 const search=plan.returnSearch;
 const steps:ReturnSearchStep[]=[];
 for(let iteration=0;iteration<(search?8:1);iteration++) {
  const domainScale=2**iteration;
  const candidate=search?search.expand(domainScale,Infinity):plan;
  const attemptStarted=performance.now();
  const problem:BoardFoilProblem={kind:'board-foil-checked',boardJson:candidate.boardJson,geometry:candidate.geometry,
   expected:{electrodePairs:candidate.electrodePairs,sourceCopper:candidate.sourceCopper,sourceBarrels:candidate.sourceBarrels},
   request:{frequenciesHz,referenceOhm,budget,
    partitionPolicy:'automatic-hybrid-v1',
    ...(candidate.attachments.length&&{attachments:candidate.attachments}),
    ...(skin&&{operatorPolicy:'hybrid-planar-v3' as const,profileUpperHz})}};
  const observations:ReturnType<typeof summarizeReturnBoundary>[]=[];
  let boundaryError:unknown;
  const checkedExecute=(input:BoardFoilProblem)=>{
   const value=execute(input);
   try {
    validateBoardFoilReport(value,input,value.cache?.producerBudget??input.request.budget);
    if(search)observations.push(summarizeReturnBoundary(value,input));
   } catch(error) {
    if(search)boundaryError=error;
    throw error;
   }
   return value;
  };
  const report=checkedExecute(problem);
  // A failing requested point already requires expansion. Holdouts are only
  // useful after every requested drive satisfies the boundary-current gate.
  const baselinePass=!search||observations.every(value=>value.passed);
  const frequencyAudit=baselinePass&&requested&&auditBoardFoilFrequency(problem,report,requested,[low,high],attemptStarted,checkedExecute);
  if(boundaryError)throw boundaryError;
  if(search) {
   steps.push({domainScale,observationBoundsMm:candidate.geometry.returnBoundaryMm,solvedRoiMm:candidate.roiMm,
    passed:observations.every(value=>value.passed),
    maximumRelative:Math.max(...observations.map(value=>value.maximumRelative)),
    maximumUnresolvedA:Math.max(...observations.map(value=>value.maximumUnresolvedA)),
    minimumPortCurrentA:Math.min(...observations.map(value=>value.minimumPortCurrentA)),
    frequenciesHz:[...new Set(observations.flatMap(value=>value.frequenciesHz))].sort((a,b)=>a-b)});
   if(!steps.at(-1)!.passed)continue;
  }
  const jointSensitivity=process.env.P7_AUTO_FOIL_AUDIT==='1'&&auditBoardFoilGrid(problem,report,attemptStarted,execute);
  const result:BoardFoilRunReport={...report,...(frequencyAudit&&{frequencyAudit}),...(jointSensitivity&&{jointSensitivity}),
   ...(search&&{returnSearch:{policy:'internal-guard-flux-v1' as const,status:'passed' as const,
    threshold:RETURN_BOUNDARY_THRESHOLD,absoluteFloorA:RETURN_CURRENT_FLOOR_A,steps}})};
  Object.defineProperty(result,'resolvedPlan',{enumerable:false,value:candidate});
  return result;
 }
 throw new Error('CHANNEL_RETURN_UNRESOLVED: boundary current did not fall below 1% '+JSON.stringify(steps));
}
