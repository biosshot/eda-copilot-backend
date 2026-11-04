import {localDcEndpoint} from './local-dc-endpoint.ts';
import {solveBoardChannel} from '../board-channel-executor.ts';
import {referenceDomainChange} from './local-domain.ts';
import {localRequest, type LocalExtractionPlan} from './local-request.ts';
import {randomUUID} from 'node:crypto';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {requestedFrequencyInterval} from '../p7-frequency-planning.ts';
import {localFrequencyAuditRequest, type LocalFrequencyAuditRequest} from '../local-frequency-audit.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {json} from './data.ts';
/** Existing channel/port/scope target carries physical inputs, never a circuit.
 * No diagnostic process, new builder, SavedScenario wrapper or net inference. */
export function isLocalTarget(task:AnalysisTask){const c=task.config as T.Configurations['impedance'];return task.kind==='impedance'&&c.method==='layered'&&(c.target?.kind==='channel'||c.target?.kind==='differential');}
export function createLocalExecutor(execute=createProblemExecutor(true)){return async(task:AnalysisTask<'impedance'>):Promise<ExecutionResult>=>{
 const start=performance.now();const c=task.config;let extractionPlan:LocalExtractionPlan|undefined;const geometry=localRequest(task,plan=>{extractionPlan=plan;});
 if(task.variantChanges.length||c.numerics||c.mesh)throw new AnalysisDslError('LOCAL_INPUT','Variants/numerics require explicit supported geometry reconstruction');
 const band:[number,number]=[1000,100000];
 const maxSeconds=Number(process.env.P7_VERIFY_SECONDS??300),maxMemoryMiB=Number(process.env.P7_VERIFY_MEMORY_MIB??2048);
 if(!Number.isSafeInteger(maxSeconds)||maxSeconds<=0||maxSeconds>3600||!Number.isSafeInteger(maxMemoryMiB)||maxMemoryMiB<256||maxMemoryMiB>8192)throw new AnalysisDslError('LOCAL_BUDGET','Invalid bounded developer verification limits');
 if(c.budget&&(c.budget.maxSeconds!==undefined&&c.budget.maxSeconds!==maxSeconds||c.budget.maxMemoryMiB!==undefined&&c.budget.maxMemoryMiB!==maxMemoryMiB))throw new AnalysisDslError('LOCAL_BUDGET','Budget must match the active default or explicit developer verification limits');
 const points=c.frequencyHz===undefined?c.frequency?.points??0:1;if(points<1||points>8192)throw new AnalysisDslError('LOCAL_BUDGET','1..8192 frequencies required');
 const [startHz,stopHz]=requestedFrequencyInterval(c);
 const frequencies=c.frequencyHz!==undefined?[startHz]:Array.from({length:points},(_,i)=>{const t=points===1?0:i/(points-1);return c.frequency!.scale==='log'?startHz*(stopHz/startHz)**t:startHz+(stopHz-startHz)*t;});
 const warningDiagnostics=()=> (extractionPlan?.diagnosticDecisions??[]).map(d=>({code:d.diagnostic.code,severity:'warning' as const,message:(d.diagnostic.message??d.diagnostic.code)+'; '+d.decision+': '+d.reason,path:d.diagnostic.path}));
 if(startHz===0&&stopHz===0)
  return localDcEndpoint(task,geometry,warningDiagnostics(),maxSeconds-(performance.now()-start)/1000,execute,extractionPlan?.dcComponents);
 if(extractionPlan&&!extractionPlan.confirmedLocalBarrel){
  const solved=(()=>{try{return solveBoardChannel(task.board,geometry,extractionPlan!.terminalRequest??extractionPlan!.contacts,frequencies,{maxSeconds:maxSeconds-(performance.now()-start)/1000,maxMemoryMiB},[startHz,stopHz]);}catch(error){if(error instanceof AnalysisDslError)throw error;const message=error instanceof Error?error.message:String(error);throw new AnalysisDslError(message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'CHANNEL_REJECTED',message,warningDiagnostics());}})();
  const componentLoss=solved.lossRecovery;
  const frequencyAudit=solved.frequencyAudit;
  const interpolation=frequencyAudit?.interpolation;
  const frequencyDiagnostics=frequencyAudit?[{
   code:'CHANNEL_PHYSICAL_FREQUENCY_UNVERIFIED',severity:'warning' as const,message:frequencyAudit.physicalCoverage.reason
  },...(interpolation&&interpolation.status!=='validated'?[{
   code:'CHANNEL_INTERPOLATION_UNVERIFIED',severity:'warning' as const,
   message:'Adaptive replay left gaps in the requested assembled response: '+JSON.stringify(interpolation.unverified)
  }]:[])]:[];
  const validity:T.Validity={status:'conditional',reason:'Automatic finite-copper slab composition; DC-uniform thin-ribbon section losses/internal L. Spatial interaction, proximity/transition and complete5% error remain unverified.',domain:json({bandHz:solved.bandHz,...(frequencyAudit&&{frequencyCoverage:{physical:frequencyAudit.physicalCoverage,interpolation:interpolation&&{status:interpolation.status,requestedIntervalsHz:interpolation.requestedIntervalsHz,validatedIntervalsHz:interpolation.validatedIntervalsHz,unverified:interpolation.unverified}}}),accuracy:'not certified5percent',temporal:'not admitted',...(componentLoss&&{lossRecovery:componentLoss})})};
  const data=solved.result.assembledModel;if(!data)throw new AnalysisDslError('CHANNEL_COMPOSITION','Physical assembled descriptor absent');
  const network:T.NetworkResource={id:'segmented-'+randomUUID(),kind:'network',ports:solved.result.ports!.map((p:{name:string})=>p.name),analyses:['networkParameters'],source:'Actual polygon FEM sections and joint direct-FEM local slabs; physical P6 terminal composition',validity,data:data as T.NetworkResource['data']};
  return {id:task.contextId+':segmented-'+randomUUID(),status:componentLoss?'partial':'complete',method:'Automatic physical slab segmentation -> numerical section/local -> P6',diagnostics:[...warningDiagnostics(),...frequencyDiagnostics,...(componentLoss?[{code:'CHANNEL_COMPONENT_REGIONAL_LOSS_UNVERIFIED',severity:'warning' as const,message:'Cu and supplied package resistance losses are recovered separately. Regional Cu recovery and absolute accuracy remain unverified.'}]:[])],assumptions:['Whole confirmed signal islands and their branches retained; physical source/island identities, not net-name joins.',...(c.target?.kind==='differential'?['Four oriented leg ports share one extracted model. The reported network uses the authored leg-to-return pairs with S50 power-wave references. Differential/common-mode characteristic impedance is unavailable.']:[]),'Each slab owns disjoint captured copper; all in-slab neighbours are solved jointly.',...(componentLoss?['Explicit component pin dependencies use the supplied package models. The AC resource establishes no additional DC copper connectivity.']:[]),'Numerical domain truncation and inter-slab approximation require independent refinement; no complete-channel5% certificate or HF/time admission.'],metrics:{segments:{kind:'table',unit:'1',data:json({audit:solved.audit,ownership:solved.ownership,blocks:solved.blocks,evidence:solved.evidence}),validity,diagnostics:[]}},network,solution:json({...solved,extractionPlan:solved.audit,validity})};
 }
 const boardJson=JSON.stringify(task.board);const domainEvidence:any[]=[];let report:any;
 const solve=(roiMm:number[][],axis:number[],drive?:number[],observablesOnly=false,magneticPatchLevel?:1|2,frequencyAudit?:LocalFrequencyAuditRequest)=>{try{return execute({kind:'local-extraction',request:{boardJson,geometry:{...geometry,roiMm},remainingSeconds:maxSeconds-(performance.now()-start)/1000,bandHz:band,frequenciesHz:axis,drive,loadsOhm:extractionPlan?geometry.ports.map(()=>50):undefined,observablesOnly,magneticPatchLevel,frequencyAudit}});}catch(error){if(error instanceof AnalysisDslError)throw error;const message=error instanceof Error?error.message:String(error);throw new AnalysisDslError(message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'LOCAL_NATIVE_REJECTED',message+'; native resources: '+JSON.stringify((error as {localResources?:unknown})?.localResources??null)+'; extraction evidence: '+JSON.stringify({roiMm,domainEvidence,extractionPlan,wallSeconds:(performance.now()-start)/1000}),warningDiagnostics());}};
 if(extractionPlan){
  const axis=[...new Set([...frequencies,1000,10000,Math.sqrt(1e9),100000])].sort((a,b)=>a-b);let previous:any;let accepted=false;
  const drives=[...geometry.ports.map((_,i)=>geometry.ports.map((_,j)=>i===j?1:0)),geometry.ports.map((_,i)=>i===0?1:-1)];
  for(const roi of extractionPlan.candidateDomainsMm){
   let samples:any[]=[];const runs:any[]=[];let spatialPrevious:any;
   for(const level of [1,2] as const){
    samples=[];
    for(const drive of drives){report=solve(roi,axis,drive,true,level);runs.push({magneticPatchLevel:level,drive,cache:report.cache,resources:report.resources,stageCounts:report.stageCounts});samples.push(...report.samples.map((s:any)=>({frequencyHz:s.frequencyHz,drive,portRealV:s.portRealV,portImaginaryV:s.portImaginaryV,totalLossW:s.totalLossW})));}
    if(spatialPrevious){const spatialChange=referenceDomainChange(spatialPrevious,{samples});runs.push({spatialChange});if(spatialChange.maxRelativeChange>0.02)throw new AnalysisDslError('LOCAL_SPATIAL_CONVERGENCE','Bounded direct-FEM polynomial patch refinement did not meet 2% canonical S50/Cu-total-loss change: '+JSON.stringify({roi,runs,spatialChange}),warningDiagnostics());}
    spatialPrevious={samples};
   }
   const change=previous?referenceDomainChange(previous,{samples}):undefined;
   domainEvidence.push({roiMm:roi,runs,change,wallSeconds:(performance.now()-start)/1000});
   if(change&&change.maxRelativeChange<=0.02){accepted=true;break;}
   previous={samples};
  }
  if(!accepted)throw new AnalysisDslError('LOCAL_DOMAIN_CONVERGENCE','Automatic physical environment did not meet the declared 2% canonical S50/Cu-total-loss change gate; no channel result admitted: '+JSON.stringify(domainEvidence));
  report=solve(domainEvidence.at(-1).roiMm,frequencies,undefined,false,2,localFrequencyAuditRequest([startHz,stopHz]));
 }else report=solve(geometry.roiMm,frequencies,undefined,false,undefined,localFrequencyAuditRequest([startHz,stopHz]));
 report.domainVerification=domainEvidence;report.observableVerification=extractionPlan?{status:'conditional',observable:'canonical50Ohm power waves and intrinsic Cu total-loss sensitivity only',referenceOhm:50,waveAbsoluteAmplitudeFloor:.01,unloadedReactiveImpedance:'unverified',regionalRecovery:'unverified',totalFivePercentAccuracy:'unverified'}:{status:'conditional',observable:'authored finite ROI; see source-bound developer evidence'};

 const ports=report.prepared.request.ports.map((p:string[],i:number)=>i===0?'input':'output');
 const audit=report.frequencyAudit;
 const coverage=audit&&{
  status:audit.status,requestedIntervalHz:audit.requestedIntervalHz,
  validatedIntervalsHz:audit.validatedIntervalsHz,unverified:audit.unverified,key:audit.key
 };
 const auditWarning=audit?.status==='unverified'?{
  code:'LOCAL_FREQUENCY_UNVERIFIED',severity:'warning' as const,
  message:'Independent FEM frequency audit left gaps in the requested intrinsic Z/Cu-loss interval: '+JSON.stringify(audit.unverified)
 }:undefined;
 const validity:T.Validity={
  ...report.validity,
  ...(auditWarning?{reason:report.validity.reason+' '+auditWarning.message}:{}),
  ...(coverage?{domain:json({frequencyAudit:coverage})}:{})
 };
 const resource:T.NetworkResource={id:`local-${report.cache.key}`,kind:'network',ports,analyses:['networkParameters','si'],source:'Extracted finite Cu/foil/barrel local FEM, congruent passive P6; finite numerical extraction domain',validity:{...validity,domain:json({bandHz:band,bandHzScope:'solver applicability; independently validated intervals are listed in frequencyAudit',request:report.prepared.request,units:{C:'F',L:'H',R:'Ohm',position:'m'},sourceMapping:report.prepared.mapping,observableVerification:report.observableVerification,frequencyAudit:coverage,fullVerification:'incomplete; see developer ledger'})},data:report.model};
 const metrics:Record<string,T.DataDescription>={segments:{kind:'table',unit:'1',data:json({ports,extractionPlan,request:report.prepared.request,originMm:report.prepared.originMm,nodesM:report.prepared.nodesM,copperCells:report.prepared.copperCells,electrodeNodes:report.prepared.electrodeNodes,sourceIds:report.prepared.sourceIds,loadsOhm:report.loadsOhm,sampleBoundaryCondition:extractionPlan?'Norton unit-current excitation and50Ohm mathematical reference shunts; not inferred PCB receivers':'unloaded physical port-current excitation',samples:report.samples,cache:report.cache,solveStageCounts:report.stageCounts,stageSeconds:report.prepared.stageSeconds,wallSeconds:(performance.now()-start)/1000}),validity,diagnostics:[]}};
 return{id:`${task.contextId}:local-${randomUUID()}`,status:'complete',method:'Local direct sparse FEM CPU/F64 -> passive P6; conditional finite ROI v1',diagnostics:[...warningDiagnostics(),...(auditWarning?[auditWarning]:[]),...(report.prepared.arrangement?.sourceCopperRestorations??[]).map((d:any)=>({code:'SOURCE_COPPER_CONSISTENCY',severity:'warning' as const,message:d.decision+': '+d.reason,path:d.sourcePath}))],assumptions:['1 kHz..100 kHz local model; authored materials/plating and finite contacts; neighbouring captured copper retained.',!extractionPlan?'Authored ROI uses insulated copper cuts. Accepted electric v1 does not certify all PCB spatial directions or arbitrary RF.':'Automatic finite copper domain and two bounded direct-FEM polynomial patch levels passed 2% canonical S50 and Cu-total-loss sensitivity at requested and verification frequencies; natural truncated-current boundaries are numerical approximations. The 50 Ohm loads define mathematical power-wave references, not PCB receivers. Unloaded reactive impedance, total 5% accuracy and regional loss/branch response remain unverified.','Local recovery drive uses RMS phasors: Re(I)^2+Im(I)^2 times cell R is mean Joule W.',
'Finite terminal convention: lower Cu face for TOP/interior; upper exterior face for BOTTOM. Metal continuing across that face is excluded from the exposed contact, without cutting the conductor.',
'Current and cell Joule losses recovered from retained current congruence, no invented field map.','Complete real-board full/reduced developer verification remains separately reported as incomplete.'],metrics,network:resource,solution:json({...report,extractionPlan,wallSeconds:(performance.now()-start)/1000})};
};}
