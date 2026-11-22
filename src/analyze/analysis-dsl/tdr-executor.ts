import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {createTransientExecutor} from './transient-executor.ts';

/** Source amplitude of the excitation step. Reflection is ratio-based, so the value cancels. */
const SOURCE_V=1;
/** Negligible far-end storage that makes an rc-input receiver a resistor (tau = Zref*C ~ 5e-17 s). */
const MATCHED_CAPACITANCE_F=1e-18;
/** |1-rho| below this is treated as an open circuit: impedance is unbounded and is not reported. */
const OPEN_EPS=1e-6;
const VELOCITY_KEY='propagationVelocityMps';
const BOARD_KINDS=['line','channel','differential','board'];

type Series={kind:'series';unit:string;axis:{name:string;unit:string;values:number[]};values:(number|null)[];validity:Record<string,unknown>;diagnostics:unknown[]};

function refuse(code:string,message:string):never{throw new AnalysisDslError(code,message,[{code,severity:'error',message}]);}

/** P11 v1 TDR for a standalone network only. Excitation is a linear-driver step of SOURCE_V
 * through a series referenceOhm into the first declared port, which is reused through the
 * existing SI transient executor by rewriting the task. Post-processing only; no solver here. */
export function createTdrExecutor(transient=createTransientExecutor()){
 return async(task:AnalysisTask<'tdr'>):Promise<ExecutionResult>=>{
  const c=task.config;
  if(task.board||task.variantChanges.length)refuse('TDR_BOARD_NOT_IMPLEMENTED','TDR board/variant analysis is not implemented in v1; use a standalone network target');
  const target=c.target;
  if(!target)refuse('TDR_TARGET_REQUIRED','TDR requires an explicit standalone network target');
  if(BOARD_KINDS.includes(String(target.kind)))refuse('TDR_BOARD_NOT_IMPLEMENTED',`TDR on ${target.kind} is not implemented in v1: P7 extraction only covers 1-10 kHz, not the TDR time window`);
  if(!['network','connect'].includes(String(target.kind)))refuse('TDR_TARGET_NOT_IMPLEMENTED',`TDR does not accept ${target.kind} targets in v1`);
  if(typeof c.referenceOhm!=='number'){
   if(object(c.referenceOhm))refuse('TDR_PER_PORT_REFERENCE_NOT_IMPLEMENTED','Per-port referenceOhm semantics are not defined for TDR v1; supply one number');
   refuse('TDR_REFERENCE_REQUIRED','TDR requires referenceOhm');
  }
  const Z0=c.referenceOhm;
  if(!(Z0>0)||!Number.isFinite(Z0))refuse('TDR_REFERENCE_INVALID','referenceOhm must be a finite positive number');
  const riseS=c.riseS;
  if(typeof riseS!=='number'||!(riseS>0))refuse('TDR_RISE_INVALID','riseS must be a finite positive number');
  if(!c.time)refuse('TDR_TIME_REQUIRED','TDR requires an explicit time window');
  if(riseS>=c.time.stopS)refuse('TDR_RISE_INVALID','riseS must be shorter than the time window');

  const network=task.descriptors.find(d=>object(d)&&d.id===target.id);
  if(!object(network)||!object(network.input))refuse('TDR_TARGET_UNKNOWN','Unknown network target');
  const input=network.input as Record<string,unknown>;
  const declared:string[]=Array.isArray(network.ports)?(network.ports as unknown[]).map(String):Array.isArray(input.ports)?(input.ports as unknown[]).map(String):Object.keys((input.ports??{}) as object);
  if(declared.length<1||declared.length>2)refuse('TDR_PORT_COUNT_UNSUPPORTED',`TDR v1 supports one or two network ports; network has ${declared.length}`);
  /** Port descriptors exist only if the author called .port(); otherwise add the same id the DSL would make. */
  const added:Record<string,unknown>[]=[];
  const port=(name:string)=>{
   const found=task.descriptors.find(x=>object(x)&&x.kind==='port'&&object(x.input)&&x.input.owner===target.id&&x.input.name===name) as Record<string,unknown>|undefined;
   const d:Record<string,unknown>=found??{kind:'port',id:`${target.id}:port:${name}`,contextId:task.contextId,input:{owner:target.id,name}};
   if(!found)added.push(d);
   return{kind:'port',id:String(d.id),contextId:task.contextId} as T.Port;
  };

  const term=c.termination;
  let farModel:T.Model|undefined;
  if(declared.length===1&&term!=='open')refuse('TDR_TERMINATION_NOT_APPLICABLE','Single-port network: the DUT is its own load; termination must be open');
  if(declared.length===2){
   if(term==='short')refuse('TDR_SHORT_TERMINATION_NOT_IMPLEMENTED','A short cannot be expressed as an rc-input receiver; supply an explicit model or use matched/open');
   if(term==='matched')farModel={kind:'rc-input',resistanceOhm:Z0,capacitanceF:MATCHED_CAPACITANCE_F} as T.LocalModel;
   else if(object(term))farModel=term as T.Model;
  }
  const dutName=declared[0]!;
  const stimulus={kind:'waveform',voltageV:{kind:'step',from:0,to:SOURCE_V,atS:0,riseS}};
  const dut=port(dutName),far=farModel?port(declared[1]!):undefined;
  const siTask={...task,kind:'si',descriptors:[...task.descriptors,...added],config:{network:target,drivers:[{port:dut,model:{kind:'linear-driver',lowV:0,highV:SOURCE_V,outputResistanceOhm:Z0},stimulus}],receivers:far?[{port:far,model:farModel}]:[],time:c.time}} as unknown as AnalysisTask<'si'>;
  const run=await transient(siTask);
  const series=run.metrics[`network-voltage:${dutName}`] as unknown as Series|undefined;
  if(!series||series.kind!=='series'||!Array.isArray(series.values))refuse('TDR_RESULT_UNAVAILABLE','Transient executor returned no port voltage');

  const incident=SOURCE_V/2;
  const times=series.axis.values;
  const diagnostics:T.Diagnostic[]=[...run.diagnostics];
  let rampSamples=0,unboundedSamples=0;
  const reflection:(number|null)[]=[],impedance:(number|null)[]=[];
  times.forEach((time,i)=>{
   const v=series.values[i];
   if(time<riseS){rampSamples++;reflection.push(null);impedance.push(null);return;}
   if(typeof v!=='number'){reflection.push(null);impedance.push(null);return;}
   const rho=v/incident-1,denominator=1-rho;
   reflection.push(rho);
   if(Math.abs(denominator)<OPEN_EPS){unboundedSamples++;impedance.push(null);}
   else impedance.push(Z0*(1+rho)/denominator);
  });
  if(rampSamples)diagnostics.push({code:'TDR_RISE_SAMPLES_UNAVAILABLE',severity:'warning',message:`${rampSamples} samples before riseS are the source ramp, not an incident step; reported unavailable`});
  if(unboundedSamples)diagnostics.push({code:'TDR_IMPEDANCE_UNBOUNDED',severity:'warning',message:`${unboundedSamples} samples have |1-rho| < ${OPEN_EPS}; impedance is unbounded and unavailable`});

  const parameters=(input.parameters??{}) as Record<string,unknown>;
  const velocity=parameters[VELOCITY_KEY];
  const delayLine=object(input.data)&&input.data.format==='delayed-scattering';
  const distance:T.DataDescription=typeof velocity==='number'&&velocity>0&&delayLine?{kind:'series',unit:'mm',axis:series.axis,values:times.map((time,i)=>time<riseS?null:velocity*time/2*1000),validity:{status:'valid'},diagnostics:[]} as unknown as T.DataDescription:{kind:'scalar',unit:'mm',validity:{status:'unavailable',reason:`No propagation velocity declared (parameters.${VELOCITY_KEY}) on a delay line; distance is not inferred from geometry`},diagnostics:[{code:'TDR_DISTANCE_UNAVAILABLE',severity:'warning',message:`parameters.${VELOCITY_KEY} with a delayed-scattering network is required for distance`}]} as unknown as T.DataDescription;

  return {
   id:`${task.contextId}:tdr`,status:'complete',method:'Standalone network TDR: step through existing SI transient path, ratio post-processing only',
   diagnostics,
   assumptions:[
    `Excitation: linear-driver step ${SOURCE_V} V, rise ${riseS} s, output resistance referenceOhm ${Z0} Ohm (Thevenin source), into the first declared port '${dutName}'.`,
    declared.length===2?`Second declared port '${declared[1]}' is the far end.`:'Single-port network: the DUT is its own load.',
    'Incident wave = V/2 after the source rise; samples before riseS are unavailable. rho = v_port/(V/2) - 1; Z = Zref(1+rho)/(1-rho).',
    'Impedance is the instantaneous port-voltage ratio of the solved transient, not band-limited TDR; no de-embedding, fixture or launch correction.',
    term==='matched'&&declared.length===2?`Matched far end realised as rc-input R=${Z0} Ohm with C=${MATCHED_CAPACITANCE_F} F (tau ~ 5e-17 s).`:term==='open'?'Open far end: no far-port receiver.':'Explicit far-end model supplied by the user.',
    `Distance: only with a delayed-scattering network declaring parameters.${VELOCITY_KEY}; otherwise unavailable, never inferred from geometry.`,
    'Board, line, channel, differential and resource targets are refused (P7 extraction covers 1-10 kHz only).',
   ],
   metrics:{
    reflection:{kind:'series',unit:'1',axis:series.axis,values:reflection,validity:series.validity,diagnostics:[]} as unknown as T.DataDescription,
    impedance:{kind:'series',unit:'Ohm',axis:series.axis,values:impedance,validity:series.validity,diagnostics:[]} as unknown as T.DataDescription,
    distance,
   },
  };
 };
}
