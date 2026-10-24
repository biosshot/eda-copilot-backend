import {randomUUID} from 'node:crypto';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {json} from './data.ts';
import {electricalCircuit} from './electrical-circuit.ts';

function unsupported(message:string):never{throw new AnalysisDslError('UNSUPPORTED_NETWORK_MODEL',message);}
/** Adapt existing circuit vocabulary; all numerical assembly/solve remains Rust. */
export function createElectricalExecutor(){
 const execute=createProblemExecutor(true);
 return async(task:AnalysisTask<'dc'|'pi'>):Promise<ExecutionResult>=>{
  const c=task.config;if(!c.circuit)unsupported('Explicit circuit required; PCB frequency extraction is a separate stage');
  if(task.variantChanges.length||c.temperatureC!==undefined)unsupported('Circuit variants/temperature laws are not supported');
  if('mesh'in c&&c.mesh||'inputPolicy'in c&&c.inputPolicy)unsupported('PCB mesh/CAD input policy cannot be applied to an explicit circuit');
  const d=task.descriptors.find(d=>object(d)&&d.id===c.circuit!.id);
  if(!object(d)||!object(d.input))unsupported('Unknown circuit');
  const circuit=d.input as unknown as T.CircuitInput;
  const mode=task.kind==='dc'?'dc':(c as T.Configurations['pi']).domain!;
  const pc=c as T.Configurations['pi'];
  // Match native admission before materializing/serializing the frequency axis.
  // Division keeps this safe even for integers beyond JS's exact range.
  if(mode==='frequency'){
   const points=pc.frequencyHz!==undefined?1:pc.frequency?.points??0;
   const response=Number(!!c.sources?.length);
   if(!Number.isSafeInteger(points)||points<1||points>100_000/(Math.max(1,circuit.ports.length)+response)||points>100_000/Math.max(1,c.sources?.length??0))
    throw new AnalysisDslError('NETWORK_BUDGET','Frequency/port solutions exceed the 100000 sample limit');
  }
  const frequencies=mode!=='frequency'?undefined:pc.frequencyHz!==undefined?[pc.frequencyHz]:pc.frequency?Array.from({length:pc.frequency.points},(_,i)=>{const f=pc.frequency!;const t=f.points===1?0:i/(f.points-1);return f.scale==='log'?f.startHz*Math.pow(f.stopHz/f.startHz,t):f.startHz+(f.stopHz-f.startHz)*t;}):undefined;
  const expanded=electricalCircuit(task,circuit,mode,frequencies);
  const initial=pc.initial==='steady'?'steady':pc.initial?.data;
  const report=execute({kind:'electrical-network',request:{contextId:task.contextId,circuit:expanded.circuit,mode,...(mode==='frequency'?{frequencies,...(expanded.acSources.length?{acSources:expanded.acSources}:{})}:{}),...(mode==='time'?{time:pc.time,initial}:{}),numerics:c.numerics,budget:c.budget}});
  if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'NETWORK_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
  report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS process high-water RSS through this native call; includes worker/Node and earlier calculations, not per-solve allocation'};
  const metrics:Record<string,T.DataDescription>={};
  const axis=report.axis?{name:mode==='frequency'?'frequencyHz' as const:'timeS' as const,unit:mode==='frequency'?'Hz' as const:'s' as const,values:report.axis}:undefined;
  const data=(v:number|readonly(number|T.Complex)[],unit:string):T.DataDescription=>({kind:Array.isArray(v)?'series':'scalar',unit,validity:{status:expanded.conditional?'conditional':'valid'},diagnostics:[],...(Array.isArray(v)?{values:v,axis}:{value:v as number})});
  if(expanded.acSources.length)metrics['circuit-ac-source-count']=data(expanded.acSources.filter(s=>s.values.some(v=>v.real!==0||v.imaginary!==0)).length,'1');
  if(report.losses!==undefined)metrics.losses=data(report.losses,'W');
  for(const[id,v]of Object.entries(report.currents??{}))metrics[`current:${JSON.stringify(id)}`]=data(v,'A');
  // Save immutable node fields for ports declared after run; observations never stamp.
  for(const[id,v]of Object.entries(report.nodeVoltages??{}))metrics[`circuit-node:${id}`]=data(v,'V');
  for(const port of circuit.ports){
   if(report.impedances?.[port.id])metrics[`impedance:${c.circuit.id}:port:${port.id}`]=data(report.impedances[port.id],'Ohm');
  }
  return{id:`${task.contextId}:network-${randomUUID()}`,status:'complete',method:'explicit sparse MNA CPU/F64',diagnostics:report.diagnostics,assumptions:['Explicit lumped RLC network; no PCB frequency extraction or automatic board connections.','V=V(first)-V(second); I first to second; passive power sign. Waveforms are absolute, not added to DC constants.','Passive circuit ports do not load the network. Impedance uses separate +1 A tests with independent sources zeroed; explicit spectra separately produce peak-phasor V/I.',...(mode==='time'?['Adaptive backward Euler with step doubling; output interpolation only. Sampled extrema can miss peaks.']:[])],metrics,solution:json({...report,resolvedModels:expanded.resolvedModels,conditional:expanded.conditional}),...(report.finalState?{finalState:json(report.finalState)}:{})};
 };
}
export function circuitObservedVoltage(task:AnalysisTask,descriptor:unknown,metrics:Readonly<Record<string,T.DataDescription>>):T.DataDescription|undefined{
 if(!object(descriptor)||!object(descriptor.input)||!('circuit'in task.config)||descriptor.input.owner!==task.config.circuit?.id)return;
 const input=descriptor.input;
 const owner=task.descriptors.find(d=>object(d)&&d.id===input.owner);
 if(!object(owner)||!object(owner.input)||!Array.isArray(owner.input.ports))return;
 const p=owner.input.ports.find(p=>object(p)&&p.id===input.name);if(!object(p))return;
 const a=metrics[`circuit-node:${p.positive}`],b=metrics[`circuit-node:${p.negative}`];if(!a||!b)return;
 const subtract=(v:number|T.Complex|null,w:number|T.Complex|null)=>typeof v==='number'&&typeof w==='number'?v-w:object(v)&&object(w)&&typeof v.real==='number'&&typeof v.imaginary==='number'&&typeof w.real==='number'&&typeof w.imaginary==='number'?{real:v.real-w.real,imaginary:v.imaginary-w.imaginary}:null;
 return {...a,...(a.values&&b.values?{values:a.values.map((v,i)=>subtract(v,b.values![i]))}:{value:(a.value as number)-(b.value as number)})};
}
export function circuitObservedTransfer(task:AnalysisTask,argument:unknown,metrics:Readonly<Record<string,T.DataDescription>>,describe?:(id:string)=>unknown):T.DataDescription|undefined{
 if(!object(argument)||!object(argument.from)||!object(argument.to)||metrics['circuit-ac-source-count']?.value!==1)return;
 const get=(p:Record<string,unknown>)=>circuitObservedVoltage(task,describe?.(String(p.id))??task.descriptors.find(d=>object(d)&&d.id===p.id),metrics);
 const from=get(argument.from),to=get(argument.to);
 if(!from?.values||!to?.values||from.axis?.name!=='frequencyHz')return;
 const values=from.values.map((v,i)=>{
  const w=to.values![i];if(!object(v)||!object(w)||typeof v.real!=='number'||typeof v.imaginary!=='number'||typeof w.real!=='number'||typeof w.imaginary!=='number')return null;
  const scale=Math.max(Math.abs(v.real),Math.abs(v.imaginary));if(scale===0)return null;
  const re=v.real/scale,im=v.imaginary/scale,den=re*re+im*im;
  const real=(w.real/scale*re+w.imaginary/scale*im)/den,imaginary=(w.imaginary/scale*re-w.real/scale*im)/den;
  return Number.isFinite(real)&&Number.isFinite(imaginary)?{real,imaginary}:null;
 });
 const missing=values.some(v=>v===null);
 return{...to,unit:'1',values,validity:{status:missing?(values.every(v=>v===null)?'unavailable':'conditional'):from.validity.status==='conditional'||to.validity.status==='conditional'?'conditional':'valid',...(missing?{reason:'Transfer denominator zero/unresolvable at some saved frequencies'}:{})}};
}
