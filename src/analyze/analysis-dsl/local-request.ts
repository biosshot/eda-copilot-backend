import type { BoardChannelRequest, PhysicalModePair } from '../channel-components.ts';
import { physicalChannelComponents, type PhysicalDcComponentRequest } from './channel-component-request.ts';
import { normalizePlannerTerminals, type PhysicalTerminal } from '../channel-terminals.ts';
import {channelPlan} from './channel-plan.ts';
import {capturedBoardGeometry} from '../geometry-snapshot.ts';
import type {AnalysisTask} from './executor.ts';import type * as T from './spec-doc.js';import {object} from './descriptions.ts';import {AnalysisDslError} from './schema.ts';import {dcMaterial} from './profile-adapter.ts';
const decimalScale=(v:number,power:number)=>{const [m,e='0']=String(v).split('e');return Number(m+'e'+(Number(e)+power));};
const fail=(s:string):never=>{throw new AnalysisDslError('LOCAL_INPUT',s);};
/** Infers a missing contact layer from the single captured pad a selector resolves to: one pad layer, else the component side when it is a pad layer. */
export function inferContactLayer(b:any,selector:any):{layer:string;pad:any}|undefined{
 if(!selector||Array.isArray(selector)||selector.kind==='viaId')return undefined;
 const pads=b.pads.filter((p:any)=>selector.kind==='padId'?p.id===selector.id:selector.kind==='pad'&&p.component===selector.designator&&p.number===selector.number);
 if(pads.length!==1)return undefined;
 const pad=pads[0],layers:string[]=pad.layers??[];
 if(layers.length===1)return {layer:layers[0],pad};
 const side=b.components?.find((c:any)=>c.designator===pad.component)?.side?.toUpperCase();
 return side&&layers.includes(side)?{layer:side,pad}:undefined;
}
export type LocalExtractionPlan = ReturnType<typeof channelPlan> & {
 terminalRequest?: BoardChannelRequest;
 dcComponents?: PhysicalDcComponentRequest[];
 portHandles?: { handleId: string; portName: string }[];
};
type PlanSink = (plan: LocalExtractionPlan) => void;
type BoardTarget = T.Channel | T.Differential;

function checkAcMaterial(material:T.Material,kind:'Layer'|'Plating'):void {
 if(material.validity?.status==='unavailable'||material.analyses&&!material.analyses.includes('impedance'))
  fail(kind+' material is unavailable for impedance extraction');
 const domain=material.validity?.domain;
 if(object(domain)&&domain.bandHz!==undefined){
  const band=domain.bandHz;
  if(!Array.isArray(band)||band.length!==2||typeof band[0]!=='number'||typeof band[1]!=='number'||
     !Number.isFinite(band[0])||!Number.isFinite(band[1])||band[0]>1000||band[1]<100000)
   fail(kind+' material band does not cover local v1 training/applicability band');
 }
}

/** Existing layered impedance target -> internal volume transport. */
export function localRequest(task:AnalysisTask<'impedance'>,onPlan?:PlanSink){
 const target=task.config.target;
 if(!target||!('contextId' in target)||(target.kind!=='channel'&&target.kind!=='differential'))return fail('Layered local extraction needs a board channel or differential pair');
 return prepareLocalRequest(task,[target],target.kind==='differential'?'differential':'legacy',onPlan,task.config.frequencyHz===0);
}

/** Existing board SI channel selection -> one joint P7 request. The caller
 * supplies the physical frequency axis and consumes the handle bindings. */
export function localChannelGroupRequest(task:AnalysisTask<'si'>,onPlan:PlanSink){
 const selected=task.config.channels;
 if(!selected?.length)return fail('Joint board extraction requires a nonempty SI channel list');
 const targets=selected.map(target=>target.kind==='line'?fail('Explicit lines are not board channels for joint extraction'):target);
 return prepareLocalRequest(task,targets,'group',onPlan);
}

export function prepareLocalRequest(task:AnalysisTask<'impedance'|'si'|'pi'>,targets:readonly BoardTarget[],mode:'legacy'|'differential'|'group',onPlan?:PlanSink,dcEndpoint=false){
 const c=task.config,b=task.board??fail('Captured board required');
 const descriptions=new Map(task.descriptors.filter((d):d is Record<string,T.Json>=>object(d)).map(d=>[String(d.id),d]));
 const input=(h:unknown):any=>{if(!object(h))return fail('Missing handle');const d=descriptions.get(String(h.id));if(!d||!object(d.input))return fail('Unknown physical descriptor');return d.input;};
 const legs=targets.flatMap((target,i)=>{
  const prefix=mode==='group'?'channel-'+(i+1)+'-':'';
  const description=input(target);
  return target.kind==='differential'
   ?[{ channel:input(description.positive), prefix:prefix+'positive-' },{ channel:input(description.negative), prefix:prefix+'negative-' }]
   :[{ channel:description, prefix }];
 });
 const channels=legs.map(({channel})=>channel);
 for(const channel of channels){
  if(channel.via?.length)fail('Authored channel path constraints are not supported by finite ROI extraction');
  if(channel.scope?.length>1)fail('Multiple explicit scopes are not supported by local extraction');
 }
 if(mode!=='legacy'&&channels.some(channel=>channel.scope?.length))fail('Joint board extraction requires an automatic common domain');
 const scope=channels[0].scope?.[0];
 if(scope&&c.components?.length)fail('Component attachments require an automatic channel domain');
 let roi:number[][]|undefined;
 if(scope){
 if(scope.kind!=='region'||scope.polygon?.holes?.length||scope.polygon?.outer?.length!==4)fail('Local v1 requires rectangular scope without holes');
 const xs=scope.polygon.outer.map((p:any)=>p.x),ys=scope.polygon.outer.map((p:any)=>p.y);roi=[[Math.min(...xs),Math.min(...ys)],[Math.max(...xs),Math.max(...ys)]];
 if(scope.polygon.outer.some((p:any)=>!([roi![0][0],roi![1][0]].includes(p.x)&&[roi![0][1],roi![1][1]].includes(p.y))))fail('Scope must be axis-aligned rectangle');}

 if(!c.stackup||!c.materials)fail('Explicit stackup/materials/plating required');const stackup=c.stackup!;
 if(stackup.input.solderMask)fail('Local v1 does not support solder-mask material extraction');
 const checkDcMaterial=(m:T.Material)=>{
  if(m.validity?.status==='unavailable'||m.analyses&&!m.analyses.includes('dc')&&!m.analyses.includes('impedance'))
   fail('Material is unavailable for exact DC');
  const domain=m.validity?.domain;
  if(object(domain)&&domain.bandHz!==undefined){
   const band=domain.bandHz;
   if(!Array.isArray(band)||band.length!==2||band[0]!==0||typeof band[1]!=='number'||!Number.isFinite(band[1])||band[1]<0)
    fail('Material band does not include exact DC');
  }
 };
 const stack=stackup.input.layers.map((layer,i)=>{const routing=stackup.routing.layers[i];if(!layer.material||!c.materials?.[layer.material])fail('Every layer needs explicit material');const m=c.materials![layer.material!];if(dcEndpoint)checkDcMaterial(m);else checkAcMaterial(m,'Layer');if(layer.kind==='dielectric'&&(m.conductivitySM!==undefined||layer.lossTangent!==undefined&&layer.lossTangent!==0))fail('Local v1 dielectric conductivity/loss is unsupported');if(layer.kind==='dielectric'&&layer.relativePermittivity!==undefined&&layer.relativePermittivity!==m.relativePermittivity)fail('Layer and material epsilon disagree');if(m.lossTangent!==undefined&&m.lossTangent!==0)fail('Local v1 dielectric loss tangent is unsupported; it must not be ignored');return layer.kind==='copper'?{name:layer.name,copper:true,thicknessM:decimalScale(routing.thicknessMm,-3),conductivitySM:dcMaterial(m,c.temperatureC).conductivity_s_m,epsilonR:null}:{name:layer.name,copper:false,thicknessM:decimalScale(routing.thicknessMm,-3),conductivitySM:null,epsilonR:m.relativePermittivity??fail('Explicit dielectric epsilon required')};});
 if(scope&&JSON.stringify([...scope.layers].sort())!==JSON.stringify(stack.filter(l=>l.copper).map(l=>l.name).sort()))fail('3D ROI scope must include every copper stack layer; partial layer scopes are unsupported');
 if(stackup.input.diameterConvention!=='finished_hole'||stackup.routing.viaPlatingThicknessUm===undefined||!stackup.input.platingMaterial)fail('Explicit finished-hole plating convention/material required');
 const plating=c.materials![stackup.input.platingMaterial!];if(!plating)fail('Plating material unavailable');
 if(dcEndpoint)checkDcMaterial(plating);
 else {
  checkAcMaterial(plating,'Plating');
  if(plating.lossTangent!==undefined&&plating.lossTangent!==0)fail('Local v1 plating loss tangent is unsupported');
 }
 const portsByLeg=legs.flatMap(({channel,prefix})=>[
  { handle:channel.from, name:prefix+'input' },
  { handle:channel.to, name:prefix+'output' },
 ]);
 const endpoints=portsByLeg.map(({handle})=>input(handle));
 const source=(selector:any)=>{
  if(Array.isArray(selector))return fail('One finite contact per electrode supported');
  if(selector.kind==='viaId'){
   const vias=b.copper?.fixed?.vias??[];
   const matches=vias.flatMap((via,index)=>via.id===selector.id?[index]:[]);
   if(matches.length!==1)fail('Unknown or ambiguous captured via');
   return `copper.fixed.vias[${matches[0]}]`;
  }
  const match=(p:typeof b.pads[number])=>selector.kind==='padId'?p.id===selector.id:selector.kind==='pad'&&p.component===selector.designator&&p.number===selector.number;
  const matches=b.pads.flatMap((pad,index)=>match(pad)?[index]:[]);
  if(matches.length!==1)fail('Unknown/ambiguous captured pad');
  return `pads[${matches[0]}]`;
 };
 const planningContacts:{sourcePath:string;layer:string}[]=[];const electrodes:any[]=[],pairs:string[][]=endpoints.map(()=>[]),seen=new Map<string,string>();
 // Signal contacts first, then returns, preserving the original STM32 ordering.
 for(const role of ['positive','negative'] as const)for(let p=0;p<endpoints.length;p++){const e=endpoints[p],layer=e[role+'Layer']??e.layer??inferContactLayer(b,e[role])?.layer;if(!layer)fail('Contact layer required');const sourcePath=source(e[role]),key=sourcePath+':'+layer;planningContacts.push({sourcePath,layer});
 let name=seen.get(key);if(!name){name=(role==='positive'?'signal':'return')+'-'+layer;if(electrodes.some(x=>x.name===name))name+='-'+p;seen.set(key,name);const index=stack.findIndex(l=>l.copper&&l.name===layer);if(index<0)fail('Contact layer absent in stack');const copperIndices=stack.flatMap((l,i)=>l.copper?[i]:[]);// Physical terminal convention is stable across channel direction: bottom uses its exterior upper face; top/interior use the first Cu face in stack order.
 electrodes.push({name,sourcePath,layer,upper:index===copperIndices.at(-1)});}pairs[p][role==='positive'?0:1]=name;}
 let geometryPairs=pairs;
 if(!roi){
  const prepared=capturedBoardGeometry(JSON.stringify(b));
  let plan: LocalExtractionPlan;
  const components=!dcEndpoint&&c.components?.length?physicalChannelComponents(c.components,task,prepared):[];
  const dcComponents=dcEndpoint&&c.components?.length?physicalChannelComponents(c.components,task,prepared,'dc'):[];
  const geometryComponents=dcEndpoint?dcComponents:components;
  if(mode!=='legacy'||components.length){
   const byId=new Map<string,PhysicalTerminal>(electrodes.map(({name:id,sourcePath,layer})=>[id,{id,sourcePath,layer}]));
   const signalIds=new Set(pairs.map(([positive])=>positive));
   const signals=[...byId.values()].filter(t=>signalIds.has(t.id));
   const references=[...byId.values()].filter(t=>!signalIds.has(t.id));
   const authored=pairs.map(([positive,negative],i)=>({name:portsByLeg[i].name,positive,negative}));
   const physicalPorts=new Map<string,typeof authored[number]>();
   const portHandles=authored.map((port,i)=>{
    const key=JSON.stringify([port.positive,port.negative]);
    const existing=physicalPorts.get(key);
    if(existing&&mode!=='group')fail('Layered differential requires four distinct oriented leg ports');
    if(!existing)physicalPorts.set(key,port);
    return {handleId:String(portsByLeg[i].handle.id),portName:(existing??port).name};
   });
   const ports=[...physicalPorts.values()];
   geometryPairs=ports.map(({positive,negative})=>[positive,negative]);
   const modePairs:PhysicalModePair[]=[];
   let legOffset=0;
   for(const [i,target] of targets.entries()){
    if(target.kind==='differential'){
     const prefix=mode==='group'?'channel-'+(i+1)+'-':'';
     for(const [end,label] of ['input','output'].entries())modePairs.push({
      positive:portHandles[2*legOffset+end].portName,negative:portHandles[2*(legOffset+1)+end].portName,
      differential:prefix+'differential-'+label,common:prefix+'common-'+label,
     });
     legOffset+=2;
    }else legOffset++;
   }
   const terminalRequest:BoardChannelRequest={signals,references,ports,...(modePairs.length&&{modePairs}),...(components.length&&{components})};
   normalizePlannerTerminals(terminalRequest);
   const signalLegs=channels.map((_,i)=>[byId.get(pairs[2*i][0])!,byId.get(pairs[2*i+1][0])!] as const);
   plan={...channelPlan(prepared,planningContacts,signalLegs,geometryComponents,dcEndpoint),terminalRequest,portHandles};
  }else plan=channelPlan(prepared,planningContacts,undefined,geometryComponents,dcEndpoint);
  roi=plan.candidateDomainsMm[0];
  onPlan?.(dcComponents.length?{...plan,dcComponents}:plan);
 }

 return {roiMm:roi,stack,finishedHolePlatingM:decimalScale(stackup.routing.viaPlatingThicknessUm!,-6),barrelConductivitySM:dcMaterial(plating,c.temperatureC).conductivity_s_m,airMarginM:.002,maxTriangleAreaM2:1e-6,maxVerticalStepM:.002,curveDeviationMm:.001,electrodes,ports:geometryPairs,acceptUnknownFillFreshness:true,allowInsulatedCopperCuts:true,minTriangleAngleDeg:0,airEdgeDivisions:1};
}
