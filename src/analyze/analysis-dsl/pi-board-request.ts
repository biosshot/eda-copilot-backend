import type {AnalysisTask} from './executor.ts';
import type * as T from './spec-doc.js';
import {electricalModel} from './electrical-models.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';

/** P8 board PI task assembly (contract: docs/analyze/reference/p8-pi/pi-board-semantics.md, section 3).
 * One port = one pair of original physical pads. Nothing is merged, grouped or guessed. */
export const PI_PORT_LIMIT=16;
export type PiPortRole='source'|'load'|'decap'|'component';
export interface PiBoardPort{
 id:string;
 roles:PiPortRole[];
 /** Authored handle id, component designator or `<designator>:<rail>:<n>` for realised component loads. */
 origin:string[];
 positive:T.Selector;negative:T.Selector;
 positiveLayer?:string;negativeLayer?:string;
 /** Small-signal DC termination of the port seen by the others: conductance in S, 0 = open, Infinity = short. */
 conductanceS:number;
}
export interface PiBoardAssembly{
 ports:PiBoardPort[];
 /** Two-pin R/C/L descriptions that become ports (`legs`) and DC packages. */
 legs:T.ComponentDescription[];
 resolvedModels:T.Json[];
 warnings:T.Diagnostic[];
}
/** DC conductance of a local model: capacitors open, inductors their DCR (zero = short), R and rc-input as authored. */
function dcConductance(m:T.LocalModel):number{
 const r=m.kind==='resistor'?m.resistanceOhm:m.kind==='inductor'?m.seriesResistanceOhm??0:m.kind==='rc-input'?m.resistanceOhm:undefined;
 return m.kind==='capacitor'||r===undefined?0:r>0?1/r:Infinity;
}
const fail=(code:string,message:string,diagnostics:T.Diagnostic[]=[]):never=>{throw new AnalysisDslError(code,message,diagnostics);};
const padOf=(task:Pick<AnalysisTask,'board'>,selector:T.Selector)=>task.board!.pads.filter(p=>selector.kind==='padId'?p.id===selector.id:selector.kind==='pad'&&p.component===selector.designator&&p.number===selector.number);

export function assemblePiBoard(task:AnalysisTask<'pi'>):PiBoardAssembly{
 const c=task.config,board=task.board;
 if(!board)return fail('MISSING_MODEL','Board PI requires an explicit board');
 if(task.variantChanges.length)return fail('VARIANT_REBUILD_REQUIRED','Board PI cannot rebuild geometry for variants');
 if(c.numerics)return fail('PI_INPUT','Board PI does not accept custom numerics');
 const ports:PiBoardPort[]=[],warnings:T.Diagnostic[]=[],resolvedModels:T.Json[]=[];
 const description=(id:string)=>task.descriptors.find(d=>object(d)&&d.id===id) as {input?:Record<string,unknown>}|undefined;
 const single=(selector:unknown):T.Selector=>{
  if(!object(selector)||Array.isArray(selector))return fail('PI_PORT','PI port side must be one physical pad');
  const s=selector as unknown as T.Selector;
  if(s.kind!=='pad'&&s.kind!=='padId')return fail('PI_PORT','PI port side must be a pad selector');
  if(padOf(task,s).length!==1)return fail('PI_PORT','PI port side must resolve to one captured pad');
  return s;
 };
 const key=(s:T.Selector)=>'pad:'+padOf(task,s)[0].id;
 const add=(role:PiPortRole,origin:string,positive:T.Selector,negative:T.Selector,conductanceS=0,layers:{positiveLayer?:string;negativeLayer?:string}={}):PiBoardPort=>{
  if(key(positive)===key(negative))fail('INVALID_REFERENCE','PI port needs distinct positive and return pads: '+origin);
  const same=ports.find(p=>key(p.positive)===key(positive)&&key(p.negative)===key(negative));
  if(same){if(!same.roles.includes(role))same.roles.push(role);same.origin.push(origin);same.conductanceS+=conductanceS;return same;}
  const port:PiBoardPort={id:'pi-port:'+ports.length,roles:[role],origin:[origin],positive,negative,conductanceS,...layers};
  ports.push(port);return port;
 };
 const fromHandle=(role:PiPortRole,handle:T.Port,conductanceS=0)=>{
  const input=description(handle.id)?.input;
  if(!object(input)||!input.positive||!input.negative)return fail('PI_PORT','PI source/load port is not a board pad pair');
  return add(role,handle.id,single(input.positive),single(input.negative),conductanceS,{
   ...(typeof input.positiveLayer==='string'&&{positiveLayer:input.positiveLayer}),...(typeof input.negativeLayer==='string'&&{negativeLayer:input.negativeLayer})});
 };
 const local=(model:T.Model,what:string,kinds:readonly string[])=>{
  const resolved=electricalModel(model,task,['pi']);
  if(!kinds.includes(resolved.model.kind))fail('PI_RESOURCE_UNSUPPORTED',what+': only local R/C/L, rc-input and linear-driver models are supported on a board in v1');
  resolvedModels.push({id:what,...resolved.provenance});
  return resolved.model;
 };
 // VRM / sources.
 const sources=c.sources??[];
 if(!sources.length)fail('MISSING_FIELD','Board PI requires at least one source');
 for(const [i,source] of sources.entries()){
  if(source.model===undefined)fail('PI_COMPONENT_MODEL','Source '+i+' has no VRM model; use model:"ideal" explicitly or a linear-driver');
  let g=Infinity;
  if(source.model==='ideal')warnings.push({code:'PI_IDEAL_SOURCE',severity:'warning',message:'Source '+i+' is an explicit ideal VRM (zero output impedance)'});
  else{const m=local(source.model as T.Model,'source:'+i,['linear-driver']);if(m.kind==='linear-driver'&&m.outputResistanceOhm>0)g=1/m.outputResistanceOhm;}
  fromHandle('source',source.port,g);
 }
 // Loads: explicit ports first, then the existing prepare resolver realised pair by pair.
 for(const [i,load] of (Array.isArray(c.loads)?c.loads:[]).entries()){
  if('designator' in load)continue;
  if(load.powerW!==undefined)fail('UNSUPPORTED_NETWORK_MODEL','Constant-power load is nonlinear and unsupported');
  if([load.currentA,load.resistanceOhm,load.model].filter(v=>v!==undefined).length!==1)fail('UNSUPPORTED_NETWORK_MODEL','Choose one of currentA/resistanceOhm/model for load '+i);
  const m=load.model?local(load.model,'load:'+i,['resistor','capacitor','inductor','rc-input']):undefined;
  fromHandle('load',load.port,load.resistanceOhm!==undefined?1/load.resistanceOhm:m?dcConductance(m):0);
 }
 for(const l of task.resolvedLoads??[])if(object(l)&&Array.isArray(l.distributions)){
  type Pin={padId:string;currentA:number};
  const groups=l.distributions as unknown as {side:string;unverified?:boolean;pins:Pin[]}[];
  const supply=groups.find(g=>g.side==='supply')?.pins.map(p=>({...p}))??[],returns=groups.find(g=>g.side==='return')?.pins.map(p=>({...p}))??[];
  let j=0,k=0,n=0;
  while(j<supply.length&&k<returns.length){
   const s=supply[j],r=returns[k],current=Math.min(s.currentA,r.currentA);
   add('load',`${String(l.designator)}:${String(l.rail)}:${++n}`,{kind:'padId',id:s.padId},{kind:'padId',id:r.padId});
   s.currentA-=current;r.currentA-=current;if(s.currentA===0)j++;if(r.currentA===0)k++;
  }
 }
 // Rail components: every R/C/L needs an explicit model; nothing is ideal unless the author says so.
 const legs:T.ComponentDescription[]=[];
 for(const entry of c.components??[]){
  if(entry.kind==='ic')continue;
  if(entry.kind==='device')fail('PI_RESOURCE_UNSUPPORTED','Component '+entry.designator+': multiport/device resources on a board are outside PI v1');
  let model:T.LocalModel;
  try{model=electricalModel(entry,task,['pi']).model;}
  catch(error){throw new AnalysisDslError('PI_COMPONENT_MODEL',error instanceof Error?error.message:String(error));}
  if(model.kind==='capacitor'){
   if(model.ideal===true)warnings.push({code:'PI_IDEAL_DECOUPLING',severity:'warning',message:'Capacitor '+entry.designator+' is explicitly ideal (no ESR/ESL)'});
   else if(model.esrOhm===undefined||model.eslH===undefined)fail('PI_COMPONENT_MODEL','Capacitor '+entry.designator+' needs esrOhm and eslH or explicit ideal:true; the nominal value never implies parasitics');
  }
  resolvedModels.push({id:'component:'+entry.designator,model:model as unknown as T.Json});
  const pins=entry.pins??{},pn=Object.values(pins);
  const [a,b]=typeof pins.positive==='string'&&typeof pins.negative==='string'?[pins.positive,pins.negative]:pn;
  if(pn.length!==2||typeof a!=='string'||typeof b!=='string'||a===b)fail('PI_COMPONENT_MODEL','Component '+entry.designator+' needs exactly two explicitly mapped pins');
  add('decap','component:'+entry.designator,single({kind:'pad',designator:entry.designator,number:a}),single({kind:'pad',designator:entry.designator,number:b}),dcConductance(model));
  legs.push(entry);
 }
 // A two-pad board part touching a PI net must be described, never silently open or shorted.
 const railNets=new Set(ports.flatMap(p=>[p.positive,p.negative]).flatMap(s=>padOf(task,s).map(pad=>pad.net).filter(Boolean)));
 const known=new Set([...legs.map(l=>l.designator),...(c.components??[]).map(l=>l.designator),
  ...ports.flatMap(p=>[p.positive,p.negative]).flatMap(s=>padOf(task,s).map(pad=>pad.component)),
  ...(Array.isArray(c.loads)?c.loads:[]).flatMap(l=>'designator' in l?[l.designator]:[])]);
 const byPart=new Map<string,string[]>();
 for(const pad of board.pads)if(pad.component)byPart.set(pad.component,[...(byPart.get(pad.component)??[]),pad.net??'']);
 const missing=[...byPart].filter(([designator,nets])=>!known.has(designator)&&nets.length===2&&nets.some(n=>n&&railNets.has(n))).map(([d])=>d);
 if(missing.length)fail('PI_COMPONENT_MODEL','Rail components without a model (declare them with .components, never implicitly open/ideal): '+missing.join(', '));
 if(ports.length>PI_PORT_LIMIT)fail('PI_PORT_LIMIT','Board PI needs '+ports.length+' ports (limit '+PI_PORT_LIMIT+'); decoupling is never grouped implicitly: '+ports.map(p=>p.origin[0]).join(', '));
 if(ports.length<2)fail('PI_PORT','Board PI needs at least a source port and a load/component port');
 return {ports,legs,resolvedModels,warnings};
}
