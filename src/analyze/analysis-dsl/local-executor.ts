import {localDcEndpoint} from './local-dc-endpoint.ts';
import {solveHybridBoardChannel} from '../hybrid-board-channel.ts';
import {boardFoilResult} from './board-foil-result.ts';
import {localRequest, type LocalExtractionPlan} from './local-request.ts';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {requestedFrequencyInterval} from '../p7-frequency-planning.ts';
import {AnalysisDslError} from './schema.ts';
/** Existing channel/port/scope target carries physical inputs, never a circuit.
 * No diagnostic process, new builder, SavedScenario wrapper or net inference. */
export function isLocalTarget(task:AnalysisTask){const c=task.config as T.Configurations['impedance'];return task.kind==='impedance'&&c.method==='layered'&&(c.target?.kind==='channel'||c.target?.kind==='differential');}
export function createLocalExecutor(execute=createProblemExecutor(true)){return async(task:AnalysisTask<'impedance'>):Promise<ExecutionResult>=>{
 const start=performance.now();const c=task.config;let extractionPlan:LocalExtractionPlan|undefined;const geometry=localRequest(task,plan=>{extractionPlan=plan;});
 if(task.variantChanges.length||c.numerics||c.mesh)throw new AnalysisDslError('LOCAL_INPUT','Variants/numerics require explicit supported geometry reconstruction');
 const dcSeconds=()=>Number(process.env.P7_VERIFY_SECONDS??300)-(performance.now()-start)/1000;
 const points=c.frequencyHz===undefined?c.frequency?.points??0:1;if(points<1)throw new AnalysisDslError('LOCAL_INPUT','At least one frequency is required');
 const [startHz,stopHz]=requestedFrequencyInterval(c);
 const frequencies=c.frequencyHz!==undefined?[startHz]:Array.from({length:points},(_,i)=>{const t=points===1?0:i/(points-1);return c.frequency!.scale==='log'?startHz*(stopHz/startHz)**t:startHz+(stopHz-startHz)*t;});
 const warningDiagnostics=()=> (extractionPlan?.diagnosticDecisions??[]).map(d=>({code:d.diagnostic.code,severity:'warning' as const,message:(d.diagnostic.message??d.diagnostic.code)+'; '+d.decision+': '+d.reason,path:d.diagnostic.path}));
 if(startHz===0&&stopHz===0)
  return localDcEndpoint(task,geometry,warningDiagnostics(),dcSeconds(),execute,extractionPlan?.dcComponents);
 if(!extractionPlan) throw new AnalysisDslError('CHANNEL_FOIL_SCOPE','Hybrid AC extraction requires an automatic domain around the physical contacts',warningDiagnostics());
 try {
  const solved=solveHybridBoardChannel(execute,task.board,geometry,extractionPlan.terminalRequest??extractionPlan.contacts,frequencies,{maxSeconds:300,maxMemoryMiB:2048},[startHz,stopHz]);
  return boardFoilResult(task.contextId,solved.foil,solved.report,warningDiagnostics());
 } catch(error) { if(error instanceof AnalysisDslError)throw error;const message=error instanceof Error?error.message:String(error);throw new AnalysisDslError(message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'CHANNEL_REJECTED',message,warningDiagnostics()); }
};}
