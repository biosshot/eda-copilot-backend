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
 const frequencies=c.frequencyHz!==undefined?[c.frequencyHz]:Array.from({length:points},(_,i)=>{const f=c.frequency!,t=points===1?0:i/(points-1);return f.scale==='log'?f.startHz*(f.stopHz/f.startHz)**t:f.startHz+(f.stopHz-f.startHz)*t;});
 const report=execute({kind:'cross-section',request:{line,frequencies,mesh:c.mesh,budget:c.budget}});
 if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'SECTION_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
 report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS process high-water RSS including Node and earlier solves'};
 const p=report.parameters!,modes=p.modes;
 const validity:T.Validity={status:'conditional',reason:task.board?'Board-derived idealized geometry/stackup hypothesis; PEC/lossless quasi-TEM heuristic, not a measured PCB channel':'Explicit ideal PEC/lossless quasi-TEM model; applicability screening is a geometric heuristic'};
 const metric=(unit:string,data:unknown,kind:'table'|'scalar'='table'):T.DataDescription=>kind==='scalar'?{kind,unit,value:data as number,validity,diagnostics:[]}:{kind,unit,data:json(data),validity,diagnostics:[]};
 const metrics:Record<string,T.DataDescription>={
  characteristicImpedance:n===1?metric('Ohm',modes[0].impedanceOhm,'scalar'):metric('Ohm',{rows:[...modes.map(m=>({name:m.name,value:m.impedanceOhm})),{name:'differential',value:p.differentialOhm},{name:'common',value:p.commonOhm}],convention:'Vdiff=V+-V-, Idiff=(I+-I-)/2; Vcommon=(V++V-)/2, Icommon=I++I-'}),
  rlgc:metric('1',{...p,units:{capacitanceFPerM:'F/m',vacuumCapacitanceFPerM:'F/m',inductanceHPerM:'H/m',resistanceOhmPerM:'Ohm/m',conductanceSPerM:'S/m'}}),
  delay:line.lengthMm===undefined?{kind:'scalar',unit:'s',validity:{status:'unavailable',reason:'lengthMm absent; only modal delaySPerM is calculated'},diagnostics:[]}:
   n===1?metric('s',modes[0].delaySPerM*Number(line.lengthMm)*1e-3,'scalar'):metric('s',{rows:modes.map(m=>({name:m.name,value:m.delaySPerM*Number(line.lengthMm)*1e-3}))}),
  segments:metric('1',{kind:'one homogeneous idealized cross section',lineId:c.target.id,geometry:line,lengthMm:line.lengthMm??null,boardIdentity:task.boardIdentity??null,conditional:true}),
 };
 const hash=createHash('sha256').update(JSON.stringify(json(line))).digest('hex');
 const network:T.NetworkResource|undefined=report.network?{id:`section-${randomUUID()}`,kind:'network',ports:report.network.ports,source:'P7a FEM C/C0, external L and analytic modal PEC lossless propagation',analyses:['networkParameters'],validity:{...validity,domain:json({geometrySha256:hash,lineId:c.target.id,frequenciesHz:frequencies,applicability:report.applicability,phaseConvention:report.network.phaseConvention,interpolation:'P6b complex coefficient interpolation is conditional, not a fresh line solve; no extrapolation',assumptions:report.assumptions})},data:{format:'network-parameters',representation:'S',referenceTopology:'common',nature:'passive',referenceOhm:50,frequenciesHz:frequencies,parameters:report.network.parameters}}:undefined;
 return{id:`${task.contextId}:section-${randomUUID()}`,status:'complete',method:'Conforming triangle P1 electrostatic FEM C/C0; sparse faer LLT CPU/F64; modal distributed PEC lossless line',diagnostics:report.diagnostics,assumptions:report.assumptions!,metrics,network,solution:json({...report,lineId:c.target.id,geometrySha256:hash,boardIdentity:task.boardIdentity??null,validity})};
};}
