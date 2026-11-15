import {randomUUID} from 'node:crypto';
import type {AnalysisFrequencyReport,AnalysisFrequencyRequest} from '../../../native/analyze/index.js';
import {runAutoFoil,type BoardFoilRunReport} from '../auto-foil-response.ts';
import {boardChannelHybridBandHz,createBoardChannelPlanner} from '../board-channel-plan.ts';
import type {BoardFoilProblem} from '../types.ts';
import {withBudgetHint} from './budget-hint.ts';
import {json} from './data.ts';
import {object} from './descriptions.ts';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import {admittanceFromS,magnitudeExtrema,portAdmittance,reflection,relativeDifference,terminatedZ,type Cx} from './pi-frequency-compose.ts';
import {fitTier,planPiFrequency,quasiStaticLimitHz,type PiBand} from './pi-frequency-plan.ts';
import {planPiPorts,solvePiDc,syntheticPiPorts,type PiProblemExecutor} from './pi-board-executor.ts';
import type {PiBoardAssembly} from './pi-board-request.ts';
import {AnalysisDslError} from './schema.ts';
import type * as T from './spec-doc.js';

type Matrix=Cx[][];
type Sample={real:number;imaginary:number};
const cx=(v:Sample):Cx=>({re:v.real,im:v.imaginary});
const out=(v:Cx|null):T.Complex|null=>v&&Number.isFinite(v.re)&&Number.isFinite(v.im)?{real:v.re,imaginary:v.im}:null;
const messageOf=(error:unknown)=>error instanceof Error?error.message:String(error);

/** Frequencies of the request: one point or the authored axis. */
export function piAxis(c:Readonly<{frequencyHz?:number;frequency?:{startHz:number;stopHz:number;points:number;scale:'log'|'linear'}}>):number[]{
 if(c.frequencyHz!==undefined)return [c.frequencyHz];
 const f=c.frequency!;
 if(!Number.isSafeInteger(f.points)||f.points<1||f.points>100000)throw new AnalysisDslError('NETWORK_BUDGET','Board PI axis needs 1..100000 points');
 return Array.from({length:f.points},(_,i)=>{const t=f.points===1?0:i/(f.points-1);return f.scale==='log'?f.startHz*(f.stopHz/f.startHz)**t:f.startHz+(f.stopHz-f.startHz)*t;});
}

/** Wave matrices S(f) (reference 50 Ohm) of an open-port reduced model through the existing P6 descriptor frequency path. */
export function reducedModelWaves(execute:PiProblemExecutor,report:Pick<BoardFoilRunReport,'reducedModel'>,names:readonly string[],frequenciesHz:readonly number[],budget:{maxSeconds:number;maxMemoryMiB:number}):Matrix[]{
 const model=report.reducedModel!;
 const n=names.length,chunk=Math.max(1,Math.min(Math.floor(131072/(n*n)),Math.floor(1e8/n**3)));
 const waves:Matrix[]=[];
 for(let from=0;from<frequenciesHz.length;from+=chunk){
  const request:AnalysisFrequencyRequest={
   instances:[{id:'board-reduced-model',ports:[...names],model:{id:'board-reduced-model',kind:'network',ports:[...names],data:model.model,
    validity:{status:'conditional',reason:'P8 reduced board model; physical accuracy unverified'},analyses:['networkParameters']}}],
   connections:[],ports:names.map((name,index)=>({name,index})),frequencies:frequenciesHz.slice(from,from+chunk),
   // The P6 network contract admits at most 4096 MiB; its own plan check still bounds the evaluation.
   representation:'S',referenceOhm:names.map(()=>50),budget:{...budget,maxMemoryMiB:Math.min(budget.maxMemoryMiB,4096)}};
  const result=execute({kind:'frequency-network',request} as never) as AnalysisFrequencyReport;
  if(result.status!=='complete'||!result.parameters)
   throw new AnalysisDslError(result.diagnostics[0]?.code??'PI_MODEL_EVALUATION',result.diagnostics.map(d=>d.message).join('; ')||'Reduced model evaluation failed',result.diagnostics);
  for(const matrix of result.parameters)waves.push(matrix.map(row=>row.map(cx)));
 }
 return waves;
}

/** Board `pi()` at one frequency or on a frequency axis (P8 stage 3). Bands are extracted once at anchor frequencies with the
 * hybrid path (enforced budget); every requested point is the evaluation of the reduced model of its band composed with the
 * decoupling legs, the VRM and the loads. Points outside [1 kHz, f_hi] are unavailable, never extrapolated. */
export function runPiFrequency(task:AnalysisTask<'pi'>,assembly:PiBoardAssembly,budget:{maxSeconds:number;maxMemoryMiB:number},start:number,execute:PiProblemExecutor):ExecutionResult{
 const c=task.config,frequencies=piAxis(c),n=assembly.ports.length;
 if(frequencies.some(f=>!(f>0)||!Number.isFinite(f)))throw new AnalysisDslError('PI_INPUT','Board PI AC frequencies must be finite and positive; the DC point is frequencyHz(0)');
 const remaining=()=>{
  const left=budget.maxSeconds-(performance.now()-start)/1000;
  if(left<=0)throw new AnalysisDslError('PI_BUDGET_DEADLINE','Board PI budget exhausted'+withBudgetHint('PI_BUDGET_DEADLINE','',budget));
  return {maxSeconds:left,maxMemoryMiB:budget.maxMemoryMiB};
 };
 const diagnostics:T.Diagnostic[]=[...assembly.warnings];
 // DC end first (it fixes the stitching reference). A failure is recorded, not hidden.
 let zDc:(number|null)[][]|undefined,dcNote:string|undefined;
 try{zDc=(solvePiDc(task,assembly,remaining(),performance.now(),execute).solution as unknown as {zMatrixDc:(number|null)[][]}).zMatrixDc;}
 catch(error){
  if(error instanceof AnalysisDslError&&/DEADLINE|MEMORY|BUDGET/.test(error.code))throw error;
  dcNote=messageOf(error);
  diagnostics.push({code:'PI_DC_STITCH_UNAVAILABLE',severity:'warning',message:'DC end of the AC stitching report is unavailable: '+dcNote});
 }
 const {synthetic,targets}=syntheticPiPorts(task,assembly);
 const {geometry,plan,order,names}=planPiPorts(synthetic,targets,assembly,false);
 if(!plan.terminalRequest)throw new AnalysisDslError('PI_PORT','Board PI AC extraction needs the physical terminal request');
 const foil=createBoardChannelPlanner(task.board,geometry,plan.terminalRequest)({closedJointDeadlineMs:Infinity,guardedReturn:true});
 const stack=(geometry as unknown as {stack:{thicknessM:number;epsilonR:number|null}[]}).stack;
 const frequencyPlan=planPiFrequency({frequenciesHz:frequencies,portCount:n,quasiStaticHz:quasiStaticLimitHz(foil.roiMm,stack)});
 const gamma=(f:number):Cx[]=>assembly.ports.map(p=>reflection(portAdmittance(p.terms,2*Math.PI*f)));
 // PI port i is native port order[i]; the native matrices are reordered into PI port order.
 const toPi=(m:Matrix):Matrix=>assembly.ports.map((_,i)=>assembly.ports.map((_,j)=>m[order[i]][order[j]]));
 const compose=(s:Matrix,f:number)=>terminatedZ(toPi(s),gamma(f));
 const z:((Cx|null)[][]|null)[]=frequencies.map(()=>null);
 const bandReports:Record<string,unknown>[]=[];
 let worstFit=0;
 let lowestAnchor:{frequencyHz:number;waves:Matrix}|undefined;
 const models=new Map<PiBand,BoardFoilRunReport>();
 const bandRefusals:{code:string;message:string}[]=[];
 for(const band of frequencyPlan.bands){
  const left=remaining();
  const frequenciesHz=band.anchorsHz;
  let report:BoardFoilRunReport;
  try{
   report=runAutoFoil(foil,frequenciesHz,{...left,enforceBudget:true,exportReducedModel:true},band.extractionHz,execute as never);
  }catch(error){
   if(error instanceof AnalysisDslError)throw error;
   const message=messageOf(error),code=message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'PI_EXTRACTION_REJECTED';
   // A budget refusal or a refusal of every band stops the run; a physical refusal of one band only makes its points unavailable.
   if(/DEADLINE|MEMORY|BUDGET/.test(code))throw new AnalysisDslError(code,withBudgetHint(code,message,budget),diagnostics);
   bandRefusals.push({code,message});
   for(const index of band.pointIndices)frequencyPlan.unavailable.push({index,frequencyHz:frequencies[index],reason:'band '+band.extractionHz+' Hz refused: '+code});
   diagnostics.push({code:'PI_BAND_REFUSED',severity:'warning',message:'Band at '+band.extractionHz+' Hz refused, its points are unavailable: '+message});
   continue;
  }
  if(!report.reducedModel||report.reducedModel.portCount!==n||report.ports.length!==n)
   throw new AnalysisDslError('PI_MODEL_EXPORT','The hybrid extraction did not return the reduced model for all '+n+' PI ports');
  models.set(band,report);
  const pointFrequencies=band.pointIndices.map(i=>frequencies[i]);
  // Evaluate the model at the requested points, the extraction frequencies (fit) and the audit points (checks) in one pass.
  const checked=report.frequenciesHz;
  const wanted=[...new Set([...pointFrequencies,...checked,...(report.frequencyAudit?.adaptiveResponse.points.map(p=>p.frequencyHz)??[])])].sort((a,b)=>a-b);
  const waves=reducedModelWaves(execute,report,names,wanted,remaining());
  const at=new Map(wanted.map((f,i)=>[f,waves[i]]));
  band.pointIndices.forEach(i=>{z[i]=compose(at.get(frequencies[i])!,frequencies[i]);});
  // Fit of the reduced model against the independently reported exact samples (Y, contract section 8 / 10).
  const compare=(frequencyHz:number,exact:Sample[][])=>{
   const model=at.get(frequencyHz)!,reference=exact.map(row=>row.map(cx));
   const ym=admittanceFromS(model),yr=admittanceFromS(reference);
   return ym&&yr?relativeDifference(ym,yr):relativeDifference(model,reference);
  };
  const anchors=report.frequenciesHz.map((frequencyHz,i)=>({frequencyHz,relative:compare(frequencyHz,report.parameters[i])}));
  const checks=(report.frequencyAudit?.adaptiveResponse.points??[]).filter(p=>!frequenciesHz.includes(p.frequencyHz))
   .map(p=>({frequencyHz:p.frequencyHz,relative:compare(p.frequencyHz,p.parameters)}));
  const maximum=Math.max(...anchors.map(a=>a.relative),...checks.map(a=>a.relative));
  worstFit=Math.max(worstFit,maximum);
  const tier=fitTier(maximum);
  if(tier==='refusal')throw new AnalysisDslError('PI_BAND_FIT','Reduced board model differs from its own extracted samples by '+(100*maximum).toFixed(2)+'% (limit 10%) in the band '+band.extractionHz.join('..')+' Hz; no result is returned',diagnostics);
  const lowest=anchors[0].frequencyHz;
  if(!lowestAnchor||lowest<lowestAnchor.frequencyHz)lowestAnchor={frequencyHz:lowest,waves:at.get(lowest)!};
  const provenance=report.provenance as unknown as {electricalRoi?:unknown;roiFallback?:unknown};
  bandReports.push({boundsHz:band.boundsHz,extractionHz:band.extractionHz,anchorsHz:band.anchorsHz,pointCount:band.pointIndices.length,
   profileUpperHz:report.reducedModel.profileUpperHz,trainedAtHz:report.reducedModel.trainedAtHz,
   modelSize:{potentials:report.reducedModel.model.potentialNodes.length,currents:report.reducedModel.model.currentDofs.length},
   bandFit:{metric:'relative Frobenius difference of Y(f) (reduced model vs extracted samples)',tier,maxRelative:maximum,
    meanRelative:anchors.reduce((s,a)=>s+a.relative,0)/anchors.length,anchors,checkPoints:checks,
    note:'Check points come from the extraction audit batch and share the training set of the model, so they are not independent of the model.'},
   frequencyAudit:report.frequencyAudit?{holdouts:report.frequencyAudit.interpolation.holdouts.map(({frequencyHz,status,maxRelativeError})=>({frequencyHz,status,maxRelativeError}))}:null,
   quality:report.quality,domain:{roiFallback:provenance.roiFallback??null,electricalRoi:provenance.electricalRoi??null}});
 }
 // Junction mismatch between neighbouring band models.
 const junctions=frequencyPlan.junctionsHz.flatMap(frequencyHz=>{
  const [lower,upper]=[...models].filter(([band])=>band.boundsHz[1]===frequencyHz||band.boundsHz[0]===frequencyHz).map(([,r])=>r);
  if(!lower||!upper)return [];// a neighbouring band was refused: no junction to compare
  const both=[lower,upper].map(r=>reducedModelWaves(execute,r,names,[frequencyHz],remaining())[0]);
  const zs=both.map(s=>compose(s,frequencyHz));
  const d=assembly.ports.map((_,i)=>zs[0][i][i]&&zs[1][i][i]?Math.hypot(zs[0][i][i]!.re-zs[1][i][i]!.re,zs[0][i][i]!.im-zs[1][i][i]!.im)/Math.max(Math.hypot(zs[0][i][i]!.re,zs[0][i][i]!.im),Number.MIN_VALUE):null);
  return {frequencyHz,relativeImpedanceDifference:d,max:Math.max(...d.filter((v):v is number=>v!==null))};
 });
 // DC/AC stitching: lowest evaluated anchor against the DC point.
 let dcStitch:Record<string,unknown>={status:'unavailable',reason:dcNote??'No extracted band'};
 if(zDc&&lowestAnchor){
  const zs=compose(lowestAnchor.waves,lowestAnchor.frequencyHz);
  const relative=assembly.ports.map((_,i)=>{const a=zs[i][i],b=zDc![i][i];return a&&b!==null&&b!==0?Math.hypot(a.re-b,a.im)/Math.abs(b):null;});
  const finite=relative.filter((v):v is number=>v!==null),maximum=finite.length?Math.max(...finite):null;
  dcStitch={status:'reported',acFrequencyHz:lowestAnchor.frequencyHz,relativeDifference:relative,max:maximum,
   tier:maximum===null?null:fitTier(maximum,0.01,0.05),dcImpedanceOhm:assembly.ports.map((_,i)=>zDc![i][i]),
   note:'|Z_ac - Z_dc|/|Z_dc| per port at the lowest extracted frequency; the inductive part is part of the difference, not subtracted.'};
  if(maximum!==null&&maximum>0.05)diagnostics.push({code:'PI_DC_STITCH',severity:'warning',message:'AC model at '+lowestAnchor.frequencyHz+' Hz differs from the DC point by '+(100*maximum).toFixed(1)+'% (approximation)'});
 }
 for(const u of frequencyPlan.unavailable.slice(0,1))diagnostics.push({code:'PI_BAND_UNAVAILABLE',severity:'warning',
  message:frequencyPlan.unavailable.length+' of '+frequencies.length+' requested points are unavailable; first: '+u.frequencyHz+' Hz '+u.reason});
 diagnostics.push({code:'PI_PHYSICAL_UNVERIFIED',severity:'warning',message:'Board PI results are an unverified approximation'},
  {code:'PI_BAND_APPROXIMATION',severity:'warning',message:'Frequency results come from a reduced coupled-RLC board model evaluated between extraction anchors; band fit '+(100*worstFit).toFixed(3)+'%'});

 if(bandRefusals.length&&!models.size){const {code,message}=bandRefusals[0];throw new AnalysisDslError(code,withBudgetHint(code,message,budget),diagnostics);}
 const available=frequencies.map((_,i)=>z[i]!==null);
 const anyAvailable=available.some(Boolean),allAvailable=available.every(Boolean);
 const validity:T.Validity=!anyAvailable?{status:'unavailable',reason:frequencyPlan.unavailable[0]?.reason??'No point in the verified band'}:{status:'conditional',
  reason:'Reduced model of an extracted copper board, unverified (P8 approximation tier)'+(allAvailable?'':'; points outside the verified band are null'),
  domain:{bandHz:[1e3,frequencyPlan.fHiHz],fHiReason:frequencyPlan.fHiReason,continuousFrequencyCoverage:false}};
 const axis={name:'frequencyHz' as const,unit:'Hz' as const,values:frequencies};
 const series=(pick:(i:number)=>Cx|null,unit:string):T.DataDescription=>({kind:'series',unit,validity,diagnostics:[],axis,values:frequencies.map((_,i)=>out(pick(i)))});
 const metrics:Record<string,T.DataDescription>={};
 const handles=assembly.ports.flatMap((port,i)=>port.origin.filter(o=>task.descriptors.some(d=>object(d)&&d.id===o&&d.kind==='port')).map(o=>[o,i] as const));
 const element=(i:number,j:number)=>(f:number)=>{const m=z[f];return m?m[i][j]:null;};
 for(const [id,i] of handles)metrics['impedance:'+id]=series(element(i,i),'Ohm');
 for(const [from,i] of handles)for(const [to,j] of handles)if(from!==to)metrics['pi-transfer:'+from+':'+to]=series(f=>{
  const m=z[f];const num=m?.[j][i],den=m?.[i][i];
  if(!num||!den)return null;
  const d=den.re*den.re+den.im*den.im;
  return d>0?{re:(num.re*den.re+num.im*den.im)/d,im:(num.im*den.re-num.re*den.im)/d}:null;
 },'1');
 const resonances=assembly.ports.map((port,i)=>({port,i})).filter(({i})=>handles.some(([,p])=>p===i)).flatMap(({port,i})=>
  magnitudeExtrema(frequencies,frequencies.map((_,f)=>{const v=z[f]?.[i][i];return v?Math.hypot(v.re,v.im):null;})).map(e=>({port:port.origin[0],...e})));
 metrics.resonances={kind:'table',unit:'Hz',validity:anyAvailable?{...validity,reason:'Local extrema of |Z(f)| on the requested axis; frequencies are grid points, refined values come from a parabola through three points in log-log scale and are not a new solve'}:validity,diagnostics:[],
  data:json({extrema:resonances,axisPoints:frequencies.length,pointsPerDecade:frequencies.length>1&&frequencies[0]<frequencies.at(-1)!?(frequencies.length-1)/Math.log10(frequencies.at(-1)!/frequencies[0]):null})};
 const solution=json({
  portMap:assembly.ports.map((p,i)=>({id:p.id,roles:p.roles,origin:p.origin,positive:p.positive,negative:p.negative,nativeIndex:order[i]})),
  frequenciesHz:frequencies,zMatrix:z.map(m=>m&&m.map(row=>row.map(out))),
  zMatrixDefinition:'zMatrix[f][i][j] is the voltage at port i for 1 A injected at port j with every other port terminated by its source, load or decoupling model; the diagonal excludes port j\'s own termination.',
  fHiHz:frequencyPlan.fHiHz,fHiReason:frequencyPlan.fHiReason,
  unavailable:frequencyPlan.unavailable,bands:bandReports,junctions,dcStitch,
  bandFit:{worstRelative:worstFit,tier:fitTier(worstFit),policy:'noise <= 1%, approximation <= 10%, refusal above'},
  dcEndpoint:zDc?{zMatrixDc:zDc}:{status:'unavailable',reason:dcNote},
  budget:{maxSeconds:budget.maxSeconds,maxMemoryMiB:budget.maxMemoryMiB,enforced:true,usedSeconds:(performance.now()-start)/1000},
  resolvedModels:assembly.resolvedModels,boardFoilBandHz:boardChannelHybridBandHz,
 });
 return {id:task.contextId+':pi-board-'+randomUUID(),status:allAvailable?'complete':'partial',
  method:'Board PI frequency domain: reduced coupled-RLC board model composed with decoupling legs, VRM and loads (P8 stage 3)',
  diagnostics,metrics,solution,
  assumptions:['Each occupied P7 band is extracted once at anchor frequencies with an enforced budget; requested points are evaluations of that band\'s reduced model, not extractions.',
   'Terminations are linear small-signal one-ports on the original pad pairs: ideal source = short, linear driver = output resistance, current loads open, resistive/rc loads as authored, decoupling = ESR+ESL+C as authored (never implied by the nominal value).',
   'impedance(port) is the voltage at the port per 1 A injected into it with every other port terminated and its own termination removed. transfer({from,to}) is V(to)/V(from) for that excitation.',
   'Points below 1 kHz or above f_hi are unavailable (null), never extrapolated.']};
}
