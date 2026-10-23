import {randomUUID} from 'node:crypto';
import type {AnalysisElectricalCircuit} from '../../../native/analyze/index.js';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {json} from './data.ts';

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
  const elements:AnalysisElectricalCircuit['elements']=circuit.elements.map(e=>{
   let input:Record<string,unknown>,nodes:readonly string[];
   if(e.kind==='model'){
    if('id'in e.model)unsupported('Resource models are not supported; provide a local R/C/L model');
    if(!['resistor','capacitor','inductor'].includes(e.model.kind))unsupported('Only local R/C/L two-contact models are supported');
    if(Object.keys(e.pins).sort().join(',')!=='n,p')unsupported('Local R/C/L models require exactly p and n contact mappings');
    input={...e.model,id:e.id};nodes=[e.pins.p,e.pins.n];
   }else{input={...e};nodes=e.nodes;}
   const {ideal,model,referenceC,temperatureCoefficientPerK,...normalized}=input;
   if(referenceC!==undefined||temperatureCoefficientPerK!==undefined)unsupported('Temperature dependent circuit models are unsupported');
   if(normalized.kind==='voltage-source'&&model!=='ideal')unsupported('Voltage sources require model:"ideal"');
   if(normalized.kind==='capacitor'){normalized.esrOhm=ideal===true?0:normalized.esrOhm;normalized.eslH=ideal===true?0:normalized.eslH;}
   if(normalized.kind==='inductor')normalized.seriesResistanceOhm=ideal===true?0:normalized.seriesResistanceOhm;
   return {...normalized,nodes:[...nodes]} as AnalysisElectricalCircuit['elements'][number];
  });
  const mode=task.kind==='dc'?'dc':(c as T.Configurations['pi']).domain!;
  const pc=c as T.Configurations['pi'];
  // Match native admission before materializing/serializing the frequency axis.
  // Division keeps this safe even for integers beyond JS's exact range.
  if(mode==='frequency'){
   const points=pc.frequencyHz!==undefined?1:pc.frequency?.points??0;
   if(!Number.isSafeInteger(points)||points<1||points>100_000/Math.max(1,circuit.ports.length))
    throw new AnalysisDslError('NETWORK_BUDGET','Frequency/port solutions exceed the 100000 sample limit');
  }
  const frequencies=mode!=='frequency'?undefined:pc.frequencyHz!==undefined?[pc.frequencyHz]:pc.frequency?Array.from({length:pc.frequency.points},(_,i)=>{const f=pc.frequency!;const t=f.points===1?0:i/(f.points-1);return f.scale==='log'?f.startHz*Math.pow(f.stopHz/f.startHz,t):f.startHz+(f.stopHz-f.startHz)*t;}):undefined;
  const initial=pc.initial==='steady'?'steady':pc.initial?.data;
  const report=execute({kind:'electrical-network',request:{contextId:task.contextId,circuit:{nodes:[...circuit.nodes],reference:circuit.reference??'gnd',ports:circuit.ports.map(p=>({...p})),elements},mode,...(mode==='frequency'?{frequencies}:{}),...(mode==='time'?{time:pc.time,initial}:{}),numerics:c.numerics,budget:c.budget}});
  if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'NETWORK_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
  report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS process high-water RSS through this native call; includes worker/Node and earlier calculations, not per-solve allocation'};
  const metrics:Record<string,T.DataDescription>={};
  const axis=report.axis?{name:mode==='frequency'?'frequencyHz' as const:'timeS' as const,unit:mode==='frequency'?'Hz' as const:'s' as const,values:report.axis}:undefined;
  const data=(v:number|readonly(number|T.Complex)[],unit:string):T.DataDescription=>({kind:Array.isArray(v)?'series':'scalar',unit,validity:{status:'valid'},diagnostics:[],...(Array.isArray(v)?{values:v,axis}:{value:v as number})});
  if(report.losses!==undefined)metrics.losses=data(report.losses,'W');
  for(const[id,v]of Object.entries(report.currents??{}))metrics[`current:${JSON.stringify(id)}`]=data(v,'A');
  // Save immutable node fields for ports declared after run; observations never stamp.
  for(const[id,v]of Object.entries(report.nodeVoltages??{}))metrics[`circuit-node:${id}`]=data(v,'V');
  for(const port of circuit.ports){
   if(report.impedances?.[port.id])metrics[`impedance:${c.circuit.id}:port:${port.id}`]=data(report.impedances[port.id],'Ohm');
  }
  return{id:`${task.contextId}:network-${randomUUID()}`,status:'complete',method:'explicit sparse MNA CPU/F64',diagnostics:report.diagnostics,assumptions:['Explicit lumped RLC network; no PCB frequency extraction or automatic board connections.','V=V(first)-V(second); I first to second; passive power sign. Waveforms are absolute, not added to DC constants.','Passive circuit ports do not load the network. Independent AC sources zeroed; separate +1 A port test.',...(mode==='time'?['Adaptive backward Euler with step doubling; output interpolation only. Sampled extrema can miss peaks.']:[])],metrics,solution:json(report),...(report.finalState?{finalState:json(report.finalState)}:{})};
 };
}
export function circuitObservedVoltage(task:AnalysisTask,descriptor:unknown,metrics:Readonly<Record<string,T.DataDescription>>):T.DataDescription|undefined{
 if(!object(descriptor)||!object(descriptor.input)||!('circuit'in task.config)||descriptor.input.owner!==task.config.circuit?.id)return;
 const input=descriptor.input;
 const owner=task.descriptors.find(d=>object(d)&&d.id===input.owner);
 if(!object(owner)||!object(owner.input)||!Array.isArray(owner.input.ports))return;
 const p=owner.input.ports.find(p=>object(p)&&p.id===input.name);if(!object(p))return;
 const a=metrics[`circuit-node:${p.positive}`],b=metrics[`circuit-node:${p.negative}`];if(!a||!b)return;
 return {...a,...(a.values&&b.values?{values:a.values.map((v,i)=>typeof v==='number'&&typeof b.values![i]==='number'?v-(b.values![i]as number):null)}:{value:(a.value as number)-(b.value as number)})};
}
