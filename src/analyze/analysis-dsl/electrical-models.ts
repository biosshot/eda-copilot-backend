import type * as T from './spec-doc.js';
import type {AnalysisTask} from './executor.ts';
import {AnalysisDslError,localModel,parse} from './schema.ts';
import {object} from './descriptions.ts';
import {getPartUuidKey} from '../../types/lcsc.ts';

const fail=(message:string):never=>{throw new AnalysisDslError('UNSUPPORTED_NETWORK_MODEL',message);};
const nominal={resistor:'resistanceOhm',capacitor:'capacitanceF',inductor:'inductanceH'} as const;
/** Strict authored nominal notation; no tolerance, package or parasitic inference. */
export function electricalNominal(value:string,kind:keyof typeof nominal):number{
 const units={resistor:'(?:Ohm|ohm|Ω)',capacitor:'F',inductor:'H'};
 if(/\S\s+\d/.test(value.trim()))fail(`Ambiguous ${kind} nominal '${value}'`);
 const text=value.trim().replace(/\s+/g,'');
 const full=new RegExp(`^(\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)([pnumkKMGTµμ]?)(?:${units[kind]})?$`).exec(text);
 const embedded=kind==='resistor'?/^(\d+)([RkKM])(\d+)$/.exec(text):null;
 if(full&&kind!=='resistor'&&!full[2]&&!new RegExp(`${units[kind]}$`).test(text))fail(`Ambiguous bare ${kind} code '${value}'; use an explicit SI prefix/unit`);
 const powers:Record<string,number>={p:-12,n:-9,u:-6,'µ':-6,'μ':-6,m:-3,k:3,K:3,M:6,G:9,T:12,R:0,'':0};
 const number=full?Number(full[1])*10**powers[full[2]]:embedded?Number(`${embedded[1]}.${embedded[3]}`)*10**powers[embedded[2]]:NaN;
 if(!Number.isFinite(number)||number<=0)fail(`Unsupported or nonpositive ${kind} nominal '${value}'`);
 return number;
}
export function electricalModel(input:T.Model|T.ComponentDescription,task:Pick<AnalysisTask,'board'|'electricalModels'>,analyses:readonly string[]=['dc','pi']){
 const provenance:Record<string,T.Json>={authored:input as unknown as T.Json};let conditional=false;
 const unwrap=(m:T.Model):T.LocalModel=>{
  if('id'in m){
   if(m.validity?.status==='unavailable')fail(`Unavailable electrical resource ${m.id}`);
   if(m.analyses&&!m.analyses.some(a=>analyses.includes(a)))fail(`Electrical resource ${m.id} does not support ${analyses.join('/')} analysis`);
   if(!object(m.data))fail(`Resource ${m.id} needs explicit local electrical data`);
   conditional ||= m.validity?.status==='conditional';provenance.resource=m as unknown as T.Json;
   return parse(localModel,m.data) as T.LocalModel;
  }
  return m;
 };
 if(!('designator'in input))return{model:unwrap(input),provenance,conditional};
 if(!['resistor','capacitor','inductor'].includes(input.kind))fail(`Component ${input.designator} needs a supported local R/C/L kind`);
 const kind=input.kind as keyof typeof nominal,key=nominal[kind];
 const instance=task.board?.components.filter(c=>c.designator===input.designator);
 if(instance&&instance.length!==1)fail(`Unknown or ambiguous board component ${input.designator}`);
 const boardComponent=instance?.[0];
 if(boardComponent)provenance.boardComponent={designator:boardComponent.designator,...(boardComponent.partUuid?{partUuid:boardComponent.partUuid}:{}),...(boardComponent.value?{value:boardComponent.value}:{})} as T.Json;
 const fields=['resistanceOhm','capacitanceF','inductanceH','esrOhm','eslH','seriesResistanceOhm','ideal','referenceC','temperatureCoefficientPerK'];
 let resolved:Record<string,unknown>=input.model?{...unwrap(input.model)}:{kind};
 if(resolved.kind!==kind)fail(`Component ${input.designator} model kind mismatch`);
 for(const field of fields)if(field in input){
  const v=(input as unknown as Record<string,unknown>)[field];
  if(field in resolved&&resolved[field]!==v)fail(`Conflicting authored ${field} on ${input.designator}`);
  resolved[field]=v;
 }
 if(resolved[key]===undefined&&boardComponent?.value){resolved[key]=electricalNominal(boardComponent.value,kind);provenance.nominalSource='board-instance-value';}
 if(resolved[key]===undefined&&boardComponent?.partUuid&&object(task.electricalModels)){
  const partKey=getPartUuidKey(boardComponent.partUuid),catalog=task.electricalModels[partKey];
  if(object(catalog)&&typeof catalog.kind==='string'){
   const model=unwrap(catalog as unknown as T.Model);if(model.kind!==kind)fail(`Catalog kind mismatch for ${partKey}`);
   resolved={...model,...resolved};
   if(resolved[key]===undefined)resolved[key]=(model as unknown as Record<string,unknown>)[key];
   provenance.nominalSource='exact-PartUuid-catalog';provenance.partUuidKey=partKey;
  }
 }
 if(resolved[key]===undefined)fail(`Missing explicit/board/catalog ${key} for ${input.designator}`);
 provenance.resolved=resolved as T.Json;
 return{model:parse(localModel,resolved) as T.LocalModel,provenance,conditional};
}
