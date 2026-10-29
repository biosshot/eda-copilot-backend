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
 const start=performance.now();const c=task.config;let extractionPlan:unknown;const geometry=localRequest(task,plan=>{extractionPlan=plan;});
 if(task.variantChanges.length||c.numerics||c.mesh||c.components?.length)throw new AnalysisDslError('LOCAL_INPUT','Variants/numerics require explicit supported geometry reconstruction');
 const band:[number,number]=[1000,100000];
 if(c.budget&&(c.budget.maxSeconds!==undefined&&c.budget.maxSeconds!==300||c.budget.maxMemoryMiB!==undefined&&c.budget.maxMemoryMiB!==2048))throw new AnalysisDslError('LOCAL_BUDGET','Local v1 currently supports the explicit 300s/2048MiB cap; other limits are not silently ignored');
 const points=c.frequencyHz===undefined?c.frequency?.points??0:1;if(points<1||points>8192)throw new AnalysisDslError('LOCAL_BUDGET','1..8192 frequencies required');
 const frequencies=c.frequencyHz!==undefined?[c.frequencyHz]:Array.from({length:points},(_,i)=>{const f=c.frequency!,t=points===1?0:i/(points-1);return f.scale==='log'?f.startHz*(f.stopHz/f.startHz)**t:f.startHz+(f.stopHz-f.startHz)*t;});
 const report=execute({kind:'local-extraction',request:{boardJson:JSON.stringify(task.board),geometry,bandHz:band,frequenciesHz:frequencies}});
 const ports=report.prepared.request.ports.map((p:string[],i:number)=>i===0?'input':'output');
 const validity:T.Validity=report.validity;
 const resource:T.NetworkResource={id:`local-${report.cache.key}`,kind:'network',ports,analyses:['networkParameters','si'],source:'Extracted finite Cu/foil/barrel local FEM, congruent passive P6; finite physical boundaries',validity:{...validity,domain:json({bandHz:band,request:report.prepared.request,units:{C:'F',L:'H',R:'Ohm',position:'m'},sourceMapping:report.prepared.mapping,fullVerification:'incomplete; see developer ledger'})},data:report.model};
 const metrics:Record<string,T.DataDescription>={segments:{kind:'table',unit:'1',data:json({ports,extractionPlan,request:report.prepared.request,originMm:report.prepared.originMm,nodesM:report.prepared.nodesM,copperCells:report.prepared.copperCells,electrodeNodes:report.prepared.electrodeNodes,sourceIds:report.prepared.sourceIds,samples:report.samples,cache:report.cache,solveStageCounts:report.stageCounts,stageSeconds:report.prepared.stageSeconds,wallSeconds:(performance.now()-start)/1000}),validity,diagnostics:[]}};
 return{id:`${task.contextId}:local-${randomUUID()}`,status:'complete',method:'Local direct sparse FEM CPU/F64 -> passive P6; conditional finite ROI v1',diagnostics:[],assumptions:['1 kHz..100 kHz local model; authored materials/plating and finite contacts; neighbouring captured copper retained.',geometry.allowInsulatedCopperCuts?'Authored ROI uses insulated copper cuts. Accepted electric v1 does not certify all PCB spatial directions or arbitrary RF.':'Automatic domain preserves complete physical islands with no insulated copper cuts. Spatial/domain convergence is unverified; large return planes can exceed the native resource cap.','Local recovery drive uses RMS phasors: Re(I)^2+Im(I)^2 times cell R is mean Joule W.',
'Current and cell Joule losses recovered from retained current congruence, no invented field map.','Complete real-board full/reduced developer verification remains separately reported as incomplete.'],metrics,network:resource,solution:json({...report,extractionPlan,wallSeconds:(performance.now()-start)/1000})};
};}
