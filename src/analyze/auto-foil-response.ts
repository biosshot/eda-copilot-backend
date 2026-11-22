import type { AutoFoilPlan } from './auto-foil-plan.ts';
import { auditBoardFoilFrequency, plannedFrequencyAuditAxis, subsetBoardFoilReport } from './board-foil-frequency-audit.ts';
import { auditBoardFoilGrid } from './board-foil-sensitivity.ts';
import { validateBoardFoilReport } from './board-foil-validation.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';
import { requestedFrequencyInterval, type FrequencyInterval } from './p7-frequency-planning.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';
import { boardChannelHybridBandHz } from './board-channel-plan.ts';
import { summarizeReturnBoundary, RETURN_BOUNDARY_THRESHOLD, RETURN_CURRENT_FLOOR_A } from './return-boundary.ts';

type ReturnSearchStep = ReturnType<typeof summarizeReturnBoundary> & {
 domainScale:number; observationBoundsMm:unknown; solvedRoiMm:AutoFoilPlan['roiMm'];
};

export type TruncatedDomainPolicy={tier:'truncated';unresolvedReturnFraction:number;limit:number;roiMode:string;marginMm:number|null;gate:number};
export type BoardFoilRunReport = BoardFoilReport & {
 /** Present only on a truncated-domain model (budget-shrunken domain, return current not resolved to the 1% gate). */
 quality:BoardFoilReport['quality']&{domainPolicy?:TruncatedDomainPolicy};
 jointSensitivity?: ReturnType<typeof auditBoardFoilGrid>;
 frequencyAudit?: ReturnType<typeof auditBoardFoilFrequency>;
 returnSearch?: {policy:'internal-guard-flux-v1';status:'passed'|'truncated';threshold:number;absoluteFloorA:number;steps:ReturnSearchStep[]};
 /** Non-enumerable caller context. It never enters serialized reports. */
 resolvedPlan?:AutoFoilPlan;
};

export const DEFAULT_MAX_UNRESOLVED_RETURN=0.10;

/** Return-gate decision. A full-domain solve keeps the 1% gate (expand while it fails). A budget-shrunken domain cannot
 * be enlarged, so it is flagged "truncated" up to `limit` of unresolved return current and refused above it. */
export function returnGateDecision(passed:boolean,maximumRelative:number,shrunk:boolean,limit:number):'passed'|'expand'|'truncated'|'refuse' {
 if(passed)return 'passed';
 if(!shrunk)return 'expand';
 return maximumRelative<=limit?'truncated':'refuse';
}

export function runAutoFoil(
 plan:AutoFoilPlan, frequenciesHz:number[], budgetIn:{maxSeconds:number;maxMemoryMiB:number;maxUnresolvedReturn?:number;enforceBudget?:boolean;exportReducedModel?:boolean},
 interval:FrequencyInterval|undefined, execute:(problem:BoardFoilProblem)=>BoardFoilReport,
): BoardFoilRunReport {
 const {maxUnresolvedReturn=DEFAULT_MAX_UNRESOLVED_RETURN,enforceBudget,exportReducedModel,...budget}=budgetIn;
 if(!(maxUnresolvedReturn>0&&maxUnresolvedReturn<=1))throw new Error('CHANNEL_FOIL_GRID: budget.maxUnresolvedReturn must lie in (0,1]');
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
 const referenceOhm=plan.ports.map(()=>50);
 const search=plan.returnSearch;
 const steps:ReturnSearchStep[]=[];
 let truncated:TruncatedDomainPolicy|undefined;
 for(let iteration=0;iteration<(search?8:1);iteration++) {
  const domainScale=2**iteration;
  const candidate=search?search.expand(domainScale,Infinity):plan;
  const attemptStarted=performance.now();
  const problem:BoardFoilProblem={kind:'board-foil-checked',boardJson:candidate.boardJson,geometry:candidate.geometry,
   expected:{electrodePairs:candidate.electrodePairs,sourceCopper:candidate.sourceCopper,sourceBarrels:candidate.sourceBarrels},
   request:{frequenciesHz,referenceOhm,budget,...(enforceBudget&&{enforceBudget}),...(exportReducedModel&&{exportReducedModel}),
    partitionPolicy:'automatic-hybrid-v1',
    ...(candidate.attachments.length&&{attachments:candidate.attachments}),
    operatorPolicy:'hybrid-planar-v3' as const,profileUpperHz}};
  const observations:ReturnType<typeof summarizeReturnBoundary>[]=[];
  let boundaryError:unknown;
  // The audit's producer batch is known before the baseline is observed, so the
  // mesh/operator is built once for baseline plus audit frequencies and both
  // consumers read their own subset. Any failure falls back to separate solves.
  const solved=new Map<string,BoardFoilReport>();
  const gridKey=(f:readonly number[])=>f.join(',');
  if(requested&&process.env.P7_AUDIT_SEPARATE_SOLVE!=='1') {
   const axis=plannedFrequencyAuditAxis(frequenciesHz,n,requested,[low,high]);
   const union=[...new Set([...frequenciesHz,...axis])].sort((a,b)=>a-b);
   if(axis.length&&union.length*n<=256) {
    const unionProblem:BoardFoilProblem={...problem,request:{...problem.request,frequenciesHz:union}};
    // A failing joint solve is a real failure of the same operator and is not retried.
    const unionReport=execute(unionProblem);
    validateBoardFoilReport(unionReport,unionProblem,unionReport.cache?.producerBudget??unionProblem.request.budget);
    try {
     for(const grid of [frequenciesHz,axis]) {
      const subset=subsetBoardFoilReport(unionReport,grid);
      validateBoardFoilReport(subset,{...problem,request:{...problem.request,frequenciesHz:grid}},subset.cache?.producerBudget??problem.request.budget);
      solved.set(gridKey(grid),subset);
     }
    } catch {
     solved.clear();
    }
   }
  }
  const checkedExecute=(input:BoardFoilProblem)=>{
   const value=solved.get(gridKey(input.request.frequenciesHz))??execute(input);
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
   if(process.env.P7_PHASE_TRACE==='1')console.error('P7_PHASE '+JSON.stringify({phase:'return-expansion',domainScale,elapsedS:(performance.now()-attemptStarted)/1000,fluxGatePassed:steps.at(-1)!.passed}));
   const last=steps.at(-1)!;
   const prov=report.provenance as {roiFallback?:{mode?:string;marginMm?:number};electricalRoi?:{marginMm?:number}}|undefined;
   const shrunk=!!(prov?.roiFallback||prov?.electricalRoi);
   // A budget-shrunken solve lands on the same shrunken mesh at every larger domain scale, so another expansion would
   // repeat the identical solve.
   const decision=returnGateDecision(last.passed,last.maximumRelative,shrunk,maxUnresolvedReturn);
   if(decision==='refuse')
    throw new Error('CHANNEL_RETURN_UNRESOLVED: boundary current '+last.maximumRelative+' above the truncated-domain limit '+maxUnresolvedReturn+' on a budget-shrunken domain; larger domain scales resolve to the same shrunken mesh (not retried) '+JSON.stringify(steps));
   if(decision==='expand')continue;
   if(decision==='truncated')truncated={tier:'truncated' as const,unresolvedReturnFraction:last.maximumRelative,limit:maxUnresolvedReturn,
    roiMode:prov?.roiFallback?.mode??'electrical-size',marginMm:prov?.roiFallback?.marginMm??prov?.electricalRoi?.marginMm??null,gate:RETURN_BOUNDARY_THRESHOLD};
  }
  const jointSensitivity=process.env.P7_AUTO_FOIL_AUDIT==='1'&&auditBoardFoilGrid(problem,report,attemptStarted,execute);
  const result:BoardFoilRunReport={...report,...(truncated&&{quality:{...report.quality,domainPolicy:truncated}}),...(frequencyAudit&&{frequencyAudit}),...(jointSensitivity&&{jointSensitivity}),
   ...(search&&{returnSearch:{policy:'internal-guard-flux-v1' as const,status:(truncated?'truncated':'passed') as 'passed'|'truncated',
    threshold:RETURN_BOUNDARY_THRESHOLD,absoluteFloorA:RETURN_CURRENT_FLOOR_A,steps}})};
  Object.defineProperty(result,'resolvedPlan',{enumerable:false,value:candidate});
  return result;
 }
 throw new Error('CHANNEL_RETURN_UNRESOLVED: boundary current did not fall below 1% '+JSON.stringify(steps));
}
