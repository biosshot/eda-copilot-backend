import {randomUUID,createHash} from 'node:crypto';
import type {AnalysisTransientRequest} from '../../../native/analyze/index.js';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {AnalysisDslError} from './schema.ts';
import {object} from './descriptions.ts';
import {electricalModel} from './electrical-models.ts';
import {createProblemExecutor} from '../execute-problem.ts';
import {json} from './data.ts';
import {frequencyTopology} from './frequency-executor.ts';
import {declaredNetworkBand} from './frequency-model.ts';
function fail(message:string):never{throw new AnalysisDslError('UNSUPPORTED_TEMPORAL_MODEL',message);}
/** Digital stimulus adapter only; differential equations and fitting stay Rust.
 * State emits its MSB before left shift; feedback is MSB XOR tap bit. No inversion.
 * Polynomial references: TI SerDes SPRUHO3B table19-1; AMD AM017 pattern table. */
export function transientStimulus(stimulus:T.Stimulus,low:number,high:number):T.Waveform{
 if(stimulus.kind==='waveform')return stimulus.voltageV;
 let bits:string;
 if(stimulus.kind==='bits'){
  if(stimulus.bits.length*stimulus.repeatCount>20000)fail('Stimulus exceeds 20000 explicit bits');
  bits=stimulus.bits.repeat(stimulus.repeatCount);
 }else{
  const taps:Record<number,number>={7:6,9:5,15:14,23:18,31:28},tap=taps[stimulus.order];
  if(!tap)fail('PRBS orders supported: 7,9,15,23,31; polynomial/bit convention is explicit');
  if(stimulus.bits>20000)fail('Stimulus exceeds 20000 explicit bits');
  let state=stimulus.seed;bits='';
  for(let i=0;i<stimulus.bits;i++){
   const output=(state>>>(stimulus.order-1))&1;
   bits+=String(output);const feedback=output^((state>>>(tap-1))&1);
   state=(state*2+feedback)%2**stimulus.order;
  }
 }
 const ui=1/stimulus.bitRateBps;
 if(stimulus.riseS<=0||stimulus.fallS<=0||stimulus.riseS>ui||stimulus.fallS>ui)fail('Bits/PRBS require finite rise/fall within one UI');
 const points:[number,number][]=[[0,bits[0]==='1'?high:low]];
 const push=(t:number,v:number)=>{const last=points.at(-1)!;if(t===last[0]){if(v!==last[1])fail('Unresolvable stimulus front');}else points.push([t,v]);};
 for(let i=1;i<bits.length;i++)if(bits[i]!==bits[i-1]){
  const old=bits[i-1]==='1'?high:low,to=bits[i]==='1'?high:low;
  push(i*ui,old);push(i*ui+(bits[i]==='1'?stimulus.riseS:stimulus.fallS),to);
 }
 push(bits.length*ui,bits.at(-1)==='1'?high:low);
 return{kind:'pwl',points,interpolation:'linear'};
}
export function transientRequest(task:AnalysisTask<'si'>){
 const c=task.config;
 if(!c.network||c.channels||task.variantChanges.length||c.temperatureC!==undefined)fail('Time currently requires an explicit network Resource with fixed coefficients');
 const descriptor=task.descriptors.find(d=>object(d)&&d.id===c.network!.id);
 if(!object(descriptor)||!['network','connect'].includes(String(descriptor.kind))||!object(descriptor.input))fail('PCB/line time models require an explicit supported model');
 const graph=frequencyTopology(task);
 for(const instance of graph.instances)if(instance.model.validity&&object(instance.model.validity)&&instance.model.validity.status==='unavailable'||instance.model.analyses&&!instance.model.analyses.includes('si'))fail('Resource does not authorize SI time analysis');
 const names=graph.ports.map(p=>p.name);
 const resolve=(binding:T.Binding)=>{
  if(!('port'in binding)||!binding.port)fail('Finite linear scalar drivers/receivers require one port binding');
  const d=task.descriptors.find(d=>object(d)&&d.id===binding.port!.id);
  if(!object(d)||!object(d.input)||d.input.owner!==c.network!.id)fail('Binding port must belong to the selected network');
  const port=names.indexOf(String(d.input.name));if(port<0)fail('Unknown time port');return port;
 };
 let conditional=graph.instances.some(i=>object(i.model.validity)&&i.model.validity.status==='conditional');const resolvedModels:T.Json[]=[];
 const drivers=(c.drivers??[]).map(d=>{
  const resolved=electricalModel(d.model,task,['si']);conditional ||=resolved.conditional;resolvedModels.push(json(resolved.provenance));
  if(resolved.model.kind!=='linear-driver')fail('SI driver requires an explicit finite linear-driver');
  return{port:resolve(d),resistanceOhm:resolved.model.outputResistanceOhm,waveform:transientStimulus(d.stimulus,resolved.model.lowV,resolved.model.highV)};
 });
 const receivers=(c.receivers??[]).map(r=>{
  const resolved=electricalModel(r.model,task,['si']);conditional ||=resolved.conditional;resolvedModels.push(json(resolved.provenance));
  if(resolved.model.kind!=='rc-input')fail('SI receiver requires an explicit passive rc-input');
  return{port:resolve(r),resistanceOhm:resolved.model.resistanceOhm,capacitanceF:resolved.model.capacitanceF};
 });
 const request:AnalysisTransientRequest={...(descriptor.kind==='network'&&c.initial===undefined?{resource:graph.instances[0].model}:{topology:graph}),drivers,receivers,time:c.time!,initial:c.initial,numerics:c.numerics,budget:c.budget};
 const sourceResources=graph.instances.map(i=>({instanceId:i.id,resource:i.model,sourceValidity:i.model.validity??null,allowedAnalyses:i.model.analyses??null,sha256:createHash('sha256').update(JSON.stringify(json(i.model))).digest('hex')}));
 const portMapping={outputs:graph.ports,connections:graph.connections,connectionContacts:graph.connectionContacts??graph.connections.map(()=>'both'),instances:graph.instances.map(i=>({instanceId:i.id,sourcePorts:i.model.ports,logicalPorts:i.ports}))};
 return{request,names,conditional,resolvedModels,sourceResources,portMapping};
}
export function createTransientExecutor(){const execute=createProblemExecutor(true);return async(task:AnalysisTask<'si'>):Promise<ExecutionResult>=>{
 const adapted=transientRequest(task),report=execute({kind:'transient-network',request:adapted.request});
 if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'NETWORK_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
 report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS process high-water RSS; includes Node and earlier scenario calculations'};
 const conditional=adapted.conditional||report.conditional;
 const resonant=adapted.sourceResources.some(s=>object(s.resource.data)&&object(s.resource.data.temporal)&&s.resource.data.temporal.method==='passive-resonant-y');
 const fitReports=Array.isArray(report.fit)?report.fit:[report.fit];
 const fitUpper=Math.min(...fitReports.map(fit=>object(fit)&&Array.isArray(fit.frequenciesHz)?Number(fit.frequenciesHz.at(-1)):Number.MAX_VALUE));
 const bandHz=resonant?[0,Math.min(fitUpper,...adapted.sourceResources.map(s=>declaredNetworkBand(s.resource)?.[1]??Number.MAX_VALUE))]:undefined;
 const applicability=bandHz?{domain:{bandHz,sourceResourceHashes:adapted.sourceResources.map(s=>s.sha256),temporalClass:'passive-resonant-y',assumption:'Authored modal RLC class and closure; unknown intersample/extraband resonances remain unexcluded'}}:{};
 const metrics:Record<string,T.DataDescription>={};
 for(let i=0;i<adapted.names.length;i++)metrics[`network-voltage:${adapted.names[i]}`]={kind:'series',unit:'V',values:report.portVoltages!.map(v=>v[i]),axis:{name:'timeS',unit:'s',values:report.axis!},validity:{status:conditional?'conditional':'valid',...applicability,...(conditional?{reason:'Conditional source/model band/finite-sample closure; see solution provenance. A finite-front screen does not prove a bandlimited source.'}:{})},diagnostics:[]};
 return{id:`${task.contextId}:time-${randomUUID()}`,status:'complete',method:'Passive supplied temporal model CPU/F64, finite drivers, adaptive backward Euler',diagnostics:report.diagnostics,assumptions:report.assumptions??[],metrics,solution:json({...report,...(bandHz?{bandHz}:{}),physicalInitial:task.config.initial??null,resolvedModels:adapted.resolvedModels,sourceResources:adapted.sourceResources,portMapping:adapted.portMapping,conditional})};
 };}
export function transientObservedVoltage(task:AnalysisTask,descriptor:unknown,metrics:Readonly<Record<string,T.DataDescription>>):T.DataDescription|undefined{
 if(task.kind!=='si'||!object(descriptor)||!object(descriptor.input)||descriptor.input.owner!==(task.config as T.Configurations['si']).network?.id)return;
 return metrics[`network-voltage:${descriptor.input.name}`];
}
