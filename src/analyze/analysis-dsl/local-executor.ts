import {domainChange} from './local-domain.ts';
import {localRequest} from './local-request.ts';
import {randomUUID} from 'node:crypto';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {json} from './data.ts';
/** Existing channel/port/scope target carries physical inputs, never a circuit.
 * No diagnostic process, new builder, SavedScenario wrapper or net inference. */
export function isLocalTarget(task:AnalysisTask){const c=task.config as T.Configurations['impedance'];return task.kind==='impedance'&&c.method==='layered'&&c.target?.kind==='channel';}
export function createLocalExecutor(){const execute=createProblemExecutor(true);return async(task:AnalysisTask<'impedance'>):Promise<ExecutionResult>=>{
 const start=performance.now();const c=task.config;let extractionPlan:ReturnType<typeof import('./channel-plan.ts').channelPlan>|undefined;const geometry=localRequest(task,plan=>{extractionPlan=plan;});
 if(task.variantChanges.length||c.numerics||c.mesh||c.components?.length)throw new AnalysisDslError('LOCAL_INPUT','Variants/numerics require explicit supported geometry reconstruction');
 const band:[number,number]=[1000,100000];
 if(c.budget&&(c.budget.maxSeconds!==undefined&&c.budget.maxSeconds!==300||c.budget.maxMemoryMiB!==undefined&&c.budget.maxMemoryMiB!==2048))throw new AnalysisDslError('LOCAL_BUDGET','Local v1 currently supports the explicit 300s/2048MiB cap; other limits are not silently ignored');
 const points=c.frequencyHz===undefined?c.frequency?.points??0:1;if(points<1||points>8192)throw new AnalysisDslError('LOCAL_BUDGET','1..8192 frequencies required');
 const frequencies=c.frequencyHz!==undefined?[c.frequencyHz]:Array.from({length:points},(_,i)=>{const f=c.frequency!,t=points===1?0:i/(points-1);return f.scale==='log'?f.startHz*(f.stopHz/f.startHz)**t:f.startHz+(f.stopHz-f.startHz)*t;});
 const boardJson=JSON.stringify(task.board);const domainEvidence:any[]=[];let report:any;
 const warningDiagnostics=()=> (extractionPlan?.diagnosticDecisions??[]).map(d=>({code:d.diagnostic.code,severity:'warning' as const,message:(d.diagnostic.message??d.diagnostic.code)+'; '+d.decision+': '+d.reason,path:d.diagnostic.path}));
 const solve=(roiMm:number[][],axis:number[],drive?:number[],observablesOnly=false,magneticPatchLevel?:1|2)=>{try{return execute({kind:'local-extraction',request:{boardJson,geometry:{...geometry,roiMm},remainingSeconds:300-(performance.now()-start)/1000,bandHz:band,frequenciesHz:axis,drive,observablesOnly,magneticPatchLevel}});}catch(error){if(error instanceof AnalysisDslError)throw error;const message=error instanceof Error?error.message:String(error);throw new AnalysisDslError(message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'LOCAL_NATIVE_REJECTED',message+'; native resources: '+JSON.stringify((error as {localResources?:unknown})?.localResources??null)+'; extraction evidence: '+JSON.stringify({roiMm,domainEvidence,extractionPlan,wallSeconds:(performance.now()-start)/1000}),warningDiagnostics());}};
 if(extractionPlan){
  const axis=[...new Set([...frequencies,1000,10000,Math.sqrt(1e9),100000])].sort((a,b)=>a-b);let previous:any;let accepted=false;
  const drives=[...geometry.ports.map((_,i)=>geometry.ports.map((_,j)=>i===j?1:0)),geometry.ports.map((_,i)=>i===0?1:-1)];
  for(const roi of extractionPlan.candidateDomainsMm){
   let samples:any[]=[];const runs:any[]=[];let spatialPrevious:any;
   for(const level of [1,2] as const){
    samples=[];
    for(const drive of drives){report=solve(roi,axis,drive,true,level);runs.push({magneticPatchLevel:level,drive,cache:report.cache,resources:report.resources,stageCounts:report.stageCounts});samples.push(...report.samples.map((s:any)=>({frequencyHz:s.frequencyHz,drive,portRealV:s.portRealV,portImaginaryV:s.portImaginaryV,totalLossW:s.totalLossW})));}
    if(spatialPrevious){const spatialChange=domainChange(spatialPrevious,{samples});runs.push({spatialChange});if(spatialChange.maxRelativeChange>0.02)throw new AnalysisDslError('LOCAL_SPATIAL_CONVERGENCE','Bounded direct-FEM polynomial patch refinement did not meet 2% complex port/total-loss change: '+JSON.stringify({roi,runs,spatialChange}),warningDiagnostics());}
    spatialPrevious={samples};
   }
   const change=previous?domainChange(previous,{samples}):undefined;
   domainEvidence.push({roiMm:roi,runs,change,wallSeconds:(performance.now()-start)/1000});
   if(change&&change.maxRelativeChange<=0.02){accepted=true;break;}
   previous={samples};
  }
  if(!accepted)throw new AnalysisDslError('LOCAL_DOMAIN_CONVERGENCE','Automatic physical environment did not meet the declared 2% complex port/total-loss change gate; no channel result admitted: '+JSON.stringify(domainEvidence));
  report=solve(domainEvidence.at(-1).roiMm,frequencies,undefined,false,2);
 }else report=solve(geometry.roiMm,frequencies);
 report.domainVerification=domainEvidence;

 const ports=report.prepared.request.ports.map((p:string[],i:number)=>i===0?'input':'output');
 const validity:T.Validity=report.validity;
 const resource:T.NetworkResource={id:`local-${report.cache.key}`,kind:'network',ports,analyses:['networkParameters','si'],source:'Extracted finite Cu/foil/barrel local FEM, congruent passive P6; finite numerical extraction domain',validity:{...validity,domain:json({bandHz:band,request:report.prepared.request,units:{C:'F',L:'H',R:'Ohm',position:'m'},sourceMapping:report.prepared.mapping,fullVerification:'incomplete; see developer ledger'})},data:report.model};
 const metrics:Record<string,T.DataDescription>={segments:{kind:'table',unit:'1',data:json({ports,extractionPlan,request:report.prepared.request,originMm:report.prepared.originMm,nodesM:report.prepared.nodesM,copperCells:report.prepared.copperCells,electrodeNodes:report.prepared.electrodeNodes,sourceIds:report.prepared.sourceIds,samples:report.samples,cache:report.cache,solveStageCounts:report.stageCounts,stageSeconds:report.prepared.stageSeconds,wallSeconds:(performance.now()-start)/1000}),validity,diagnostics:[]}};
 return{id:`${task.contextId}:local-${randomUUID()}`,status:'complete',method:'Local direct sparse FEM CPU/F64 -> passive P6; conditional finite ROI v1',diagnostics:[...warningDiagnostics(),...(report.prepared.arrangement?.sourceCopperRestorations??[]).map((d:any)=>({code:'SOURCE_COPPER_CONSISTENCY',severity:'warning' as const,message:d.decision+': '+d.reason,path:d.sourcePath}))],assumptions:['1 kHz..100 kHz local model; authored materials/plating and finite contacts; neighbouring captured copper retained.',!extractionPlan?'Authored ROI uses insulated copper cuts. Accepted electric v1 does not certify all PCB spatial directions or arbitrary RF.':'Automatic finite copper domain and two bounded direct-FEM polynomial patch levels passed 2% complex port and total-loss sensitivity at requested and verification frequencies; natural truncated-current boundaries are numerical approximations. This does not certify total 5% accuracy or regional loss/branch response.','Local recovery drive uses RMS phasors: Re(I)^2+Im(I)^2 times cell R is mean Joule W.',
'Finite terminal convention: lower Cu face for TOP/interior; upper exterior face for BOTTOM. Metal continuing across that face is excluded from the exposed contact, without cutting the conductor.',
'Current and cell Joule losses recovered from retained current congruence, no invented field map.','Complete real-board full/reduced developer verification remains separately reported as incomplete.'],metrics,network:resource,solution:json({...report,extractionPlan,wallSeconds:(performance.now()-start)/1000})};
};}
