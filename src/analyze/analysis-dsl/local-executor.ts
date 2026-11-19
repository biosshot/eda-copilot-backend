import {localDcEndpoint} from './local-dc-endpoint.ts';
import {solveHybridBoardChannel} from '../hybrid-board-channel.ts';
import {boardFoilResult} from './board-foil-result.ts';
import {localRequest, type LocalExtractionPlan} from './local-request.ts';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {requestedFrequencyInterval} from '../p7-frequency-planning.ts';
import {AnalysisDslError} from './schema.ts';
import {withBudgetHint} from './budget-hint.ts';
import {resolveMemoryMiB} from './machine-memory.ts';
import {cascadeSwitchOn,solveChannelCascade} from '../channel-cascade.ts';
import {chooseChannelModel} from '../channel-cascade-policy.ts';
import {channelCascadeResult} from './channel-cascade-result.ts';
/** Defaults apply only when the request carries no `.budget()`. */
const defaultBudget={maxSeconds:1800};
/** Existing channel/port/scope target carries physical inputs, never a circuit.
 * No diagnostic process, new builder, SavedScenario wrapper or net inference. */
export function isLocalTarget(task:AnalysisTask){const c=task.config as T.Configurations['impedance'];return task.kind==='impedance'&&c.method==='layered'&&(c.target?.kind==='channel'||c.target?.kind==='differential');}
export function createLocalExecutor(execute=createProblemExecutor(true)){return async(task:AnalysisTask<'impedance'>):Promise<ExecutionResult>=>{
 const start=performance.now();const c=task.config;let extractionPlan:LocalExtractionPlan|undefined;const geometry=localRequest(task,plan=>{extractionPlan=plan;});
 if(task.variantChanges.length||c.numerics||c.mesh)throw new AnalysisDslError('LOCAL_INPUT','Variants/numerics require explicit supported geometry reconstruction');
 const dcSeconds=()=>Number(c.budget?.maxSeconds??process.env.P7_VERIFY_SECONDS??defaultBudget.maxSeconds)-(performance.now()-start)/1000;
 const points=c.frequencyHz===undefined?c.frequency?.points??0:1;if(points<1)throw new AnalysisDslError('LOCAL_INPUT','At least one frequency is required');
 const [startHz,stopHz]=requestedFrequencyInterval(c);
 const frequencies=c.frequencyHz!==undefined?[startHz]:Array.from({length:points},(_,i)=>{const t=points===1?0:i/(points-1);return c.frequency!.scale==='log'?startHz*(stopHz/startHz)**t:startHz+(stopHz-startHz)*t;});
 const warningDiagnostics=()=> (extractionPlan?.diagnosticDecisions??[]).map(d=>({code:d.diagnostic.code,severity:'warning' as const,message:(d.diagnostic.message??d.diagnostic.code)+'; '+d.decision+': '+d.reason,path:d.diagnostic.path}));
 if(startHz===0&&stopHz===0)
  return localDcEndpoint(task,geometry,warningDiagnostics(),dcSeconds(),execute,extractionPlan?.dcComponents,undefined,resolveMemoryMiB(c.budget?.maxMemoryMiB).maxMemoryMiB);
 if(!extractionPlan) throw new AnalysisDslError('CHANNEL_FOIL_SCOPE','Hybrid AC extraction requires an automatic domain around the physical contacts',warningDiagnostics());
 try {
  // P7_CHANNEL_CASCADE: a plain pad-track-pad channel is a chain of 2D sections; any other channel keeps the one-domain path below.
  if(cascadeSwitchOn()&&!extractionPlan.terminalRequest){
   const signal=extractionPlan.copperDecisions.filter(d=>d.role==='signal').map(({sourcePath,layer})=>({sourcePath,layer}));
   const choice=chooseChannelModel(task.board,extractionPlan.contacts,signal);
   if(choice.mode==='cascade'){
    const budget={maxSeconds:Number(c.budget?.maxSeconds??defaultBudget.maxSeconds)-(performance.now()-start)/1000,maxMemoryMiB:resolveMemoryMiB(c.budget?.maxMemoryMiB).maxMemoryMiB};
    const cascade=solveChannelCascade(execute,{boardJson:JSON.stringify(task.board),stack:(geometry as any).stack,choice,returns:extractionPlan.contacts.slice(2),frequencies,budget});
    return channelCascadeResult(task.contextId,cascade,choice,extractionPlan.contacts,warningDiagnostics());
   }
  }
  const solved=solveHybridBoardChannel(execute,task.board,geometry,extractionPlan.terminalRequest??extractionPlan.contacts,frequencies,{maxSeconds:c.budget?.maxSeconds??defaultBudget.maxSeconds,maxMemoryMiB:resolveMemoryMiB(c.budget?.maxMemoryMiB).maxMemoryMiB},[startHz,stopHz]);
  return boardFoilResult(task.contextId,solved.foil,solved.report,warningDiagnostics());
 } catch(error) { if(error instanceof AnalysisDslError)throw error;const message=error instanceof Error?error.message:String(error);{const code=message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'CHANNEL_REJECTED';throw new AnalysisDslError(code,withBudgetHint(code,message,{maxSeconds:c.budget?.maxSeconds??defaultBudget.maxSeconds,maxMemoryMiB:resolveMemoryMiB(c.budget?.maxMemoryMiB).maxMemoryMiB},c.budget?.maxMemoryMiB===undefined),warningDiagnostics());} }
};}
