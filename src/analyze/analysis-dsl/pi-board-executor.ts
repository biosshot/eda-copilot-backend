import type {AnalysisLocalDcReport} from '../../../native/analyze/index.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {withBudgetHint} from './budget-hint.ts';
import {json} from './data.ts';
import {object} from './descriptions.ts';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import {localDcEndpoint} from './local-dc-endpoint.ts';
import {prepareLocalRequest,type LocalExtractionPlan} from './local-request.ts';
import {portDcImpedanceMatrix} from './pi-board-dc.ts';
import {runPiFrequency} from './pi-frequency-executor.ts';
import {assemblePiBoard,type PiBoardAssembly} from './pi-board-request.ts';
import {AnalysisDslError} from './schema.ts';
import type * as T from './spec-doc.js';

/** Same default as the local/group executors; used only when the request carries no `.budget()`. */
export const piDefaultBudget={maxSeconds:1800,maxMemoryMiB:4096};
const NOT_IMPLEMENTED='PI_NOT_IMPLEMENTED_STAGE';

/** Every PI port is one physical pad pair. The channel machinery plans the common domain, so the ports are
 * presented as synthetic channels (port 2j -> port 2j+1, an odd last port closes onto port 0); no channel semantics are used. */
export function syntheticPiPorts(task:AnalysisTask<'pi'>,assembly:PiBoardAssembly){
 const descriptors:T.Json[]=[];
 const handle=(id:string,kind:string)=>({kind,id,contextId:task.contextId});
 assembly.ports.forEach((port,k)=>descriptors.push({...handle('pi:port:'+k,'port'),input:json({positive:port.positive,negative:port.negative,
  ...(port.positiveLayer&&{positiveLayer:port.positiveLayer}),...(port.negativeLayer&&{negativeLayer:port.negativeLayer})})}));
 const targets=Array.from({length:Math.ceil(assembly.ports.length/2)},(_,j)=>{
  const channel=handle('pi:channel:'+j,'channel');
  descriptors.push({...channel,input:{from:{id:'pi:port:'+2*j},to:{id:'pi:port:'+(2*j+1<assembly.ports.length?2*j+1:0)}}});
  return channel as unknown as T.Channel;
 });
 const synthetic={...task,descriptors:[...task.descriptors,...descriptors],config:{...task.config,components:undefined}} as unknown as AnalysisTask<'pi'>;
 return {synthetic,targets};
}
/** Plans the physical domain of the synthetic ports and maps PI ports to the native port order (`order[k]` = native index of PI port k). */
export function planPiPorts(synthetic:AnalysisTask<'pi'>,targets:T.Channel[],assembly:PiBoardAssembly,dcEndpoint:boolean){
 let plan:LocalExtractionPlan|undefined;
 const geometry=prepareLocalRequest(synthetic,targets,'group',p=>{plan=p;},dcEndpoint);
 const names=[...new Set(plan!.portHandles!.map(h=>h.portName))];
 if(names.length!==assembly.ports.length)throw new AnalysisDslError('PI_PORT','Physical port planning merged PI ports; each PI port needs its own pad pair');
 const index=plan!.portHandles!.map(h=>names.indexOf(h.portName));
 const order=assembly.ports.map((_,k)=>index[plan!.portHandles!.findIndex(h=>h.handleId==='pi:port:'+k)]);
 const ids=names.map((_,g)=>assembly.ports[order.indexOf(g)].id);
 return {geometry,plan:plan!,order,ids,names};
}
export type PiProblemExecutor=(problem:never)=>unknown;

/** DC point of the board PI (`frequencyHz(0)`); also the DC end of the AC stitching report. */
export function solvePiDc(task:AnalysisTask<'pi'>,assembly:PiBoardAssembly,budget:{maxSeconds:number;maxMemoryMiB:number},start:number,execute:PiProblemExecutor):ExecutionResult{
 const {synthetic,targets}=syntheticPiPorts(task,assembly);
 const {geometry,order,ids}=planPiPorts(synthetic,targets,assembly,true);
 const left=budget.maxSeconds-(performance.now()-start)/1000;
 let dc:ExecutionResult;
 try{dc=localDcEndpoint(synthetic as unknown as AnalysisTask<'impedance'>,geometry,[],left,execute as never,undefined,ids);}
 catch(error){
  if(!(error instanceof AnalysisDslError))throw error;
  throw new AnalysisDslError(error.code,withBudgetHint(error.code,error.message,budget),error.diagnostics);
 }
 return finish(task,assembly,order,dc,geometry.ports as [string,string][]);
}

/** Board `pi()` (P8). `frequencyHz(0)` is the DC point (stage 2); a positive frequency or an axis is the AC path
 * (stage 3, `pi-frequency-executor.ts`); the time domain is refused with `PI_NOT_IMPLEMENTED_STAGE` until stage 4. */
export function createPiBoardExecutor(execute:PiProblemExecutor=createProblemExecutor(true) as never){
 return async(task:AnalysisTask<'pi'>):Promise<ExecutionResult>=>{
  const start=performance.now(),c=task.config;
  const assembly=assemblePiBoard(task);
  if(c.domain==='time'||c.time!==undefined)
   throw new AnalysisDslError(NOT_IMPLEMENTED,'Board PI time domain is not implemented yet (P8 stage 4)');
  const budget={maxSeconds:c.budget?.maxSeconds??piDefaultBudget.maxSeconds,maxMemoryMiB:c.budget?.maxMemoryMiB??piDefaultBudget.maxMemoryMiB};
  if(c.frequencyHz===0)return solvePiDc(task,assembly,budget,start,execute);
  if(c.frequencyHz===undefined&&c.frequency===undefined)
   throw new AnalysisDslError('MISSING_FIELD','Board PI frequency domain needs frequencyHz(0) for the DC point, frequencyHz(f) or frequency({startHz,stopHz,points,scale})');
  return runPiFrequency(task,assembly,budget,start,execute);
 };
}

function finish(task:AnalysisTask<'pi'>,assembly:PiBoardAssembly,order:number[],dc:ExecutionResult,geometryPorts:[string,string][]):ExecutionResult{
 const report=dc.solution as unknown as AnalysisLocalDcReport;
 const at=(name:string)=>{const i=report.electrodeNames.indexOf(name);if(i<0)throw new AnalysisDslError('LOCAL_DC_EVIDENCE','Missing electrode '+name);return i;};
 const pairs=geometryPorts.map(([p,n])=>[at(p),at(n)] as const);
 const zNative=portDcImpedanceMatrix(report.dcConductanceS,report.electrodeGalvanicComponents,pairs,pairs.map((_,g)=>assembly.ports[order.indexOf(g)].conductanceS));
 // Report in PI port order; floating ports stay null.
 const z=assembly.ports.map((_,i)=>assembly.ports.map((_,j)=>zNative[order[i]][order[j]]));
 for(const [i,port] of assembly.ports.entries())
  if(z[i][i]===null&&port.roles.includes('load'))
   throw new AnalysisDslError('PI_PORT_FLOATING','Port '+port.origin[0]+' is not connected to its return at DC (no source or load path closes rail and return)');
 const seen=z.map((row,i)=>row[i]);
 const validity:T.Validity={status:'conditional',reason:'DC endpoint of an extracted copper model; physical accuracy unverified (P8 approximation tier).',domain:{frequencyHz:0,continuousFrequencyCoverage:false}};
 const metrics:Record<string,T.DataDescription>={...dc.metrics};
 for(const [i,port] of assembly.ports.entries())for(const origin of port.origin)
  if(task.descriptors.some(d=>object(d)&&d.id===origin&&d.kind==='port'))
   metrics['impedance:'+origin]=seen[i]===null?{kind:'scalar',unit:'Ohm',validity:{status:'unavailable',reason:'Open circuit at DC once the own termination is removed'},diagnostics:[]}:
    {kind:'scalar',unit:'Ohm',value:seen[i]!,validity,diagnostics:[]};
 const portMap=assembly.ports.map((p,i)=>({id:p.id,roles:p.roles,origin:p.origin,positive:p.positive,negative:p.negative,
  conductanceS:Number.isFinite(p.conductanceS)?p.conductanceS:'short',floating:z[i][i]===null,dcImpedanceOhm:seen[i]}));
 return {...dc,metrics,diagnostics:[...dc.diagnostics,...assembly.warnings,{code:'PI_PHYSICAL_UNVERIFIED',severity:'warning',message:'Board PI results are an unverified approximation'}],
  assumptions:[...dc.assumptions,'Board PI DC point: capacitors open, small-signal current loads open, sources shorted or their output resistance; zMatrixDc[i][j] is the voltage at port i for 1 A at port j with the other ports terminated, the diagonal excludes its own termination.'],
  method:'Board PI DC endpoint: N-port copper conduction (P8 stage 2)',
  solution:json({...(dc.solution as object),portMap,zMatrixDc:z,resolvedModels:assembly.resolvedModels})};
}
