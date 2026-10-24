import type {AnalysisElectricalCircuit,AnalysisElectricalRequest} from '../../../native/analyze/index.js';
import type {AnalysisTask} from './executor.ts';
import type * as T from './spec-doc.js';
import {electricalModel} from './electrical-models.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';

function unsupported(message:string):never{throw new AnalysisDslError('UNSUPPORTED_NETWORK_MODEL',message);}
/** Only topology/resource adaptation. Assembly, complex arithmetic and integration stay Rust/F64. */
export function electricalCircuit(task:AnalysisTask<'dc'|'pi'>,circuit:T.CircuitInput,mode:string,frequencies?:number[]){
 const nodes=[...circuit.nodes],elements:AnalysisElectricalCircuit['elements']=[],acSources:NonNullable<AnalysisElectricalRequest['acSources']>=[],resolvedModels:T.Json[]=[];
 let conditional=false;
 const add=(input:Record<string,unknown>,pair:readonly string[])=>{
  const {ideal,model,referenceC,temperatureCoefficientPerK,...normalized}=input;
  if(referenceC!==undefined||temperatureCoefficientPerK!==undefined)unsupported('Temperature dependent circuit models are unsupported');
  if(normalized.kind==='voltage-source'&&model!=='ideal'){
   if(!object(model))unsupported('Voltage source needs ideal or linear-driver model');
   const resolved=electricalModel(model as unknown as T.Model,task,[task.kind]);conditional ||=resolved.conditional;
   const driver=resolved.model;if(driver.kind!=='linear-driver')unsupported('Voltage source needs ideal or linear-driver model');
   const node=`driver:${String(normalized.id)}`;
   if(nodes.includes(node))unsupported(`Generated driver node collision ${node}`);nodes.push(node);
   elements.push({...normalized,nodes:[node,pair[1]]} as AnalysisElectricalCircuit['elements'][number]);
   elements.push({id:`${String(normalized.id)}:output`,kind:'resistor',nodes:[node,pair[0]],resistanceOhm:driver.outputResistanceOhm});
   resolvedModels.push({id:String(normalized.id),...resolved.provenance,interpretation:'Authored absolute voltage behind finite output resistance; no clipping or inferred switching'});
   return;
  }
  if(normalized.kind==='capacitor'){normalized.esrOhm=ideal===true?0:normalized.esrOhm;normalized.eslH=ideal===true?0:normalized.eslH;}
  if(normalized.kind==='inductor')normalized.seriesResistanceOhm=ideal===true?0:normalized.seriesResistanceOhm;
  elements.push({...normalized,nodes:[...pair]} as AnalysisElectricalCircuit['elements'][number]);
 };
 const addModel=(id:string,model:T.Model|T.ComponentDescription,pair:readonly string[])=>{
  const resolved=electricalModel(model,task,[task.kind]);conditional ||= resolved.conditional;
  resolvedModels.push({id,...resolved.provenance});
  if(resolved.model.kind==='rc-input'){
   add({id:`${id}:C`,kind:'capacitor',capacitanceF:resolved.model.capacitanceF,ideal:true},pair);
   if(resolved.model.resistanceOhm!==undefined)add({id:`${id}:R`,kind:'resistor',resistanceOhm:resolved.model.resistanceOhm},pair);
  }else{
   if(!['resistor','capacitor','inductor'].includes(resolved.model.kind))unsupported('Circuit model requires local R/C/L or rc-input; driver activity belongs to an explicit source');
   add({...resolved.model,id},pair);
  }
 };
 for(const e of circuit.elements){
  if(e.kind==='model'){
   if(Object.keys(e.pins).sort().join(',')!=='n,p')unsupported('Local models require exactly p and n contact mappings');
   addModel(e.id,e.model,[e.pins.p,e.pins.n]);
  }else add({...e},e.nodes);
 }
 const portNodes=(port:T.Port):readonly[string,string]=>{
  const descriptor=task.descriptors.find(d=>object(d)&&d.id===port.id);
  if(!object(descriptor)||!object(descriptor.input)||descriptor.input.owner!==task.config.circuit?.id)unsupported('Source/load port must belong to the selected explicit circuit');
  const input=descriptor.input;
  const p=circuit.ports.find(p=>p.id===input.name);
  if(!p)unsupported('Unknown explicit circuit source/load port');return[p.positive,p.negative];
 };
 for(const [i,source]of(task.config.sources??[]).entries()){
  const pair=portNodes(source.port),id=`source:${i}`,spectrum=source.voltageSpectrum??source.currentSpectrum;
  if(mode==='frequency'){
   if(!spectrum||source.waveform||source.voltageV!==undefined||source.currentA!==undefined)unsupported('AC source requires an explicit voltageSpectrum/currentSpectrum, never a DC value or waveform');
   const unit=source.voltageSpectrum?'V':'A';
   if(spectrum.unit!==unit||!frequencies||spectrum.frequencyHz.length!==frequencies.length||spectrum.frequencyHz.some((f,j)=>f!==frequencies[j]))unsupported('Source spectrum unit and axis must match requested frequencies exactly');
   if(source.currentSpectrum&&source.model!==undefined&&source.model!=='ideal')unsupported('Current spectrum supports explicit ideal current source only');
   add({id,kind:source.voltageSpectrum?'voltage-source':'current-source',...(source.voltageSpectrum?{voltageV:0,model:source.model??'ideal'}:{currentA:0})},pair);
   acSources.push({elementId:id,values:spectrum.values.map(v=>({...v}))});
  }else{
   if(spectrum)unsupported('Frequency spectra cannot be used as DC/time sources');
   if(mode==='dc'&&source.waveform)unsupported('DC source cannot silently select a waveform operating point');
   if(source.voltageV!==undefined)add({id,kind:'voltage-source',voltageV:source.voltageV,model:source.model??'ideal',...(source.waveform?{waveform:source.waveform}:{})},pair);
   else if(source.currentA!==undefined){
    if(source.model!==undefined&&source.model!=='ideal')unsupported('Current source supports ideal model only');
    add({id,kind:'current-source',currentA:source.currentA,...(source.waveform?{waveform:source.waveform}:{})},pair);
   }else unsupported('Explicit voltageV/currentA required');
  }
 }
 const loads=task.config.loads;
 if(loads!==undefined&&!Array.isArray(loads))unsupported('Explicit circuit loads must be individual circuit PortLoad objects');
 for(const [i,input]of((loads??[]) as readonly T.Load[]).entries()){
  if(!('port'in input))unsupported('Component/pin loads belong to board mode; use an explicit circuit port');
  const load=input as T.PortLoad,pair=portNodes(load.port),id=`load:${i}`;
  const count=[load.currentA,load.resistanceOhm,load.model,load.powerW].filter(v=>v!==undefined).length;
  if(count!==1||load.powerW!==undefined)unsupported('Choose one explicit R/current/model load; constant power needs an unsupported nonlinear model');
  if(load.waveform&&(load.currentA===undefined||mode!=='time'))unsupported('Load waveform requires time-domain explicit currentA');
  if(load.resistanceOhm!==undefined)add({id,kind:'resistor',resistanceOhm:load.resistanceOhm},pair);
  else if(load.currentA!==undefined)add({id,kind:'current-source',currentA:mode==='frequency'?0:load.currentA,...(load.waveform?{waveform:load.waveform}:{})},pair);
  else addModel(id,load.model!,pair);
 }
 if(nodes.length>1024||elements.length>2048)throw new AnalysisDslError('NETWORK_BUDGET','Expanded circuit exceeds 1024 nodes/2048 elements');
 if(new Set(elements.map(e=>e.id)).size!==elements.length)unsupported('Expanded source/load/model element ID collision');
 return{circuit:{nodes,reference:circuit.reference??'gnd',ports:circuit.ports.map(p=>({...p})),elements},acSources,resolvedModels,conditional};
}
