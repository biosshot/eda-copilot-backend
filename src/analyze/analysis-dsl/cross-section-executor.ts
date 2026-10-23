import {randomUUID,createHash} from 'node:crypto';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {json} from './data.ts';

/** Transport/description adaptation only. Geometry, FEM and propagation are Rust. */
export function createCrossSectionExecutor(){const execute=createProblemExecutor(true);return async(task:AnalysisTask<'impedance'>):Promise<ExecutionResult>=>{
 const c=task.config;
 if(c.method!=='cross-section'||c.target?.kind!=='line')throw new AnalysisDslError('SECTION_UNSUPPORTED','Cross-section supports explicit a.line only; formula/layered/channel/differential/Resource extraction is not implemented');
 if(task.variantChanges.length||c.numerics||c.stackup||c.materials||c.components||c.temperatureC!==undefined)throw new AnalysisDslError('SECTION_UNSUPPORTED','Supply dimensions/materials on a.line; variants, overrides, temperature and numerics are unsupported; mesh/physics guards remain fixed');
 const d=task.descriptors.find(x=>object(x)&&x.id===c.target!.id);
 if(!object(d)||!object(d.input))throw new AnalysisDslError('SECTION_INPUT','Missing immutable line description');
 const line=d.input,n=String(line.kind).startsWith('coupled-')?2:1,points=c.frequencyHz===undefined?c.frequency?.points??0:1;
 if(!Number.isSafeInteger(points)||points<1||points>8192||points*(2*n)**2>131072)throw new AnalysisDslError('SECTION_BUDGET','Frequency output cap before axis allocation');
 const hasLoss=line.conductors!==undefined||['material','topMaterial','bottomMaterial'].some(k=>object(line[k])&&Number(line[k].lossTangent)>0);
 const refs=String(line.kind).includes('microstrip')?1:2;
 if(hasLoss&&points*((2*n)**2+n*n*(6+n+refs+4)+n*32)>131072)throw new AnalysisDslError('SECTION_BUDGET','Loss coefficient/mode/contribution output cap before frequency axis allocation');
 const frequencies=c.frequencyHz!==undefined?[c.frequencyHz]:Array.from({length:points},(_,i)=>{const f=c.frequency!,t=points===1?0:i/(points-1);return f.scale==='log'?f.startHz*(f.stopHz/f.startHz)**t:f.startHz+(f.stopHz-f.startHz)*t;});
 const report=execute({kind:'cross-section',request:{line,frequencies,mesh:c.mesh,budget:c.budget}});
 if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'SECTION_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
 report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS process high-water RSS including Node and earlier solves'};
 const p=report.parameters!,modes=p.modes;
 type LossMode={name:string;characteristicImpedanceOhm:{real:number;imaginary:number};phaseDelayS:number|null};
 const loss=p.frequencyDependent as undefined|{frequenciesHz:number[];samples:{frequencyHz:number;modes:LossMode[]}[]};
 const validity:T.Validity={status:'conditional',reason:loss?`${task.board?'Board-derived geometry/reference/material hypotheses; ':''}Weak-loss quasi-TEM surface impedance/regional energy perturbation; fixed material band, small skin depth, no roughness/radiation; numerical convergence is not an applicability proof`:task.board?'Board-derived idealized geometry/stackup hypothesis; PEC/lossless quasi-TEM heuristic, not a measured PCB channel':'Explicit ideal PEC/lossless quasi-TEM model; applicability screening is a geometric heuristic'};
 const metric=(unit:string,data:unknown,kind:'table'|'scalar'='table'):T.DataDescription=>kind==='scalar'?{kind,unit,value:data as number,validity,diagnostics:[]}:{kind,unit,data:json(data),validity,diagnostics:[]};
 const metrics:Record<string,T.DataDescription>={
  characteristicImpedance:n===1?metric('Ohm',modes[0].impedanceOhm,'scalar'):metric('Ohm',{rows:[...modes.map(m=>({name:m.name,value:m.impedanceOhm})),{name:'differential',value:p.differentialOhm},{name:'common',value:p.commonOhm}],convention:'Vdiff=V+-V-, Idiff=(I+-I-)/2; Vcommon=(V++V-)/2, Icommon=I++I-'}),
  rlgc:metric('1',{...p,units:{capacitanceFPerM:'F/m',vacuumCapacitanceFPerM:'F/m',inductanceHPerM:'H/m',resistanceOhmPerM:'Ohm/m',conductanceSPerM:'S/m'}}),
  delay:line.lengthMm===undefined?{kind:'scalar',unit:'s',validity:{status:'unavailable',reason:'lengthMm absent; only modal delaySPerM is calculated'},diagnostics:[]}:
   n===1?metric('s',modes[0].delaySPerM*Number(line.lengthMm)*1e-3,'scalar'):metric('s',{rows:modes.map(m=>({name:m.name,value:m.delaySPerM*Number(line.lengthMm)*1e-3}))}),
  segments:metric('1',{kind:'one homogeneous idealized cross section',lineId:c.target.id,geometry:line,lengthMm:line.lengthMm??null,boardIdentity:task.boardIdentity??null,conditional:true}),
 };
 if(loss){
  const series=(unit:string,values:(number|{real:number;imaginary:number})[]):T.DataDescription=>({kind:'series',unit,values,axis:{name:'frequencyHz',unit:'Hz',values:frequencies},validity,diagnostics:[]});
  const convention='Vdiff=V+-V-, Idiff=(I+-I-)/2; Vcommon=(V++V-)/2, Icommon=I++I-';
  metrics.characteristicImpedance=n===1?series('Ohm',loss.samples.map(s=>s.modes[0].characteristicImpedanceOhm)):metric('Ohm',{frequenciesHz:frequencies,rows:loss.samples.flatMap(s=>[...s.modes.map(m=>({frequencyHz:s.frequencyHz,name:m.name,...m.characteristicImpedanceOhm})),{frequencyHz:s.frequencyHz,name:'differential',real:2*s.modes[1].characteristicImpedanceOhm.real,imaginary:2*s.modes[1].characteristicImpedanceOhm.imaginary},{frequencyHz:s.frequencyHz,name:'common',real:s.modes[0].characteristicImpedanceOhm.real/2,imaginary:s.modes[0].characteristicImpedanceOhm.imaginary/2}]),convention});
  if(line.lengthMm!==undefined)metrics.delay=n===1?{...series('s',loss.samples.map(s=>s.modes[0].phaseDelayS!)),data:{definition:'phase delay beta*length/omega; not group delay'}}:metric('s',{frequenciesHz:frequencies,definition:'phase delay beta*length/omega; not group delay',rows:loss.samples.flatMap(s=>s.modes.map(m=>({frequencyHz:s.frequencyHz,name:m.name,value:m.phaseDelayS})))});
  else metrics.delay={kind:'scalar',unit:'s',validity:{status:'unavailable',reason:'lengthMm absent; frequency-dependent modal phaseDelaySPerM remains available in rlgc'},diagnostics:[]};
  metrics.rlgc=metric('1',{conductorOrder:p.conductorOrder,...loss,units:{frequencyHz:'Hz',resistanceOhmPerM:'Ohm/m',externalInductanceHPerM:'H/m',internalInductanceHPerM:'H/m',totalInductanceHPerM:'H/m',conductanceSPerM:'S/m',capacitanceFPerM:'F/m',alphaNpPerM:'Np/m',betaRadPerM:'rad/m',characteristicImpedanceOhm:'Ohm',phaseDelayS:'s',phaseDelaySPerM:'s/m'}});
 }
 const hash=createHash('sha256').update(JSON.stringify(json(line))).digest('hex');
 const network:T.NetworkResource|undefined=report.network?{id:`section-${randomUUID()}`,kind:'network',ports:report.network.ports,source:loss?'P7b FEM C/C0, vacuum unit-current surface impedance and regional energy loss perturbation; stable complex modal propagation':'P7a FEM C/C0, external L and analytic modal PEC lossless propagation',analyses:['networkParameters'],validity:{...validity,domain:json({geometrySha256:hash,lineId:c.target.id,frequenciesHz:frequencies,applicability:report.applicability,...(loss?{lossModel:{lineInput:line,method:"vacuum unit-current surface impedance and regional dielectric energy participation perturbation",frequencyBandHz:[frequencies[0],frequencies.at(-1)],convergence:"independent mesh/domain loss power <=2%; legacy C/C0/L/Z/delay <=1%"}}:{}),phaseConvention:report.network.phaseConvention,interpolation:'P6b complex coefficient interpolation is conditional, not a fresh line solve; no extrapolation',assumptions:report.assumptions})},data:{format:'network-parameters',representation:'S',referenceTopology:'common',nature:'passive',referenceOhm:50,frequenciesHz:frequencies,parameters:report.network.parameters}}:undefined;
 return{id:`${task.contextId}:section-${randomUUID()}`,status:'complete',method:loss?'Conforming P1 electrostatic/vacuum FEM; unit-current surface impedance and regional energy perturbation; stable lossy modal waves CPU/F64':'Conforming triangle P1 electrostatic FEM C/C0; sparse faer LLT CPU/F64; modal distributed PEC lossless line',diagnostics:report.diagnostics,assumptions:report.assumptions!,metrics,network,solution:json({...report,lineId:c.target.id,geometrySha256:hash,boardIdentity:task.boardIdentity??null,validity})};
};}
