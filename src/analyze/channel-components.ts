import type { AnalysisFrequencyResource } from '../../native/analyze/index.js';
import type { ChannelTerminalRequest, PhysicalTerminal } from './channel-terminals.ts';
import type { ChannelCut } from './segmented-channel.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';
import type { CoupledRlcData, LocalModel } from './analysis-dsl/spec-doc.js';

export interface PhysicalComponentRequest {
 id: string;
 designator: string;
 pins: readonly [PhysicalTerminal, PhysicalTerminal];
 equivalentKind?: PassiveElement["kind"];
 model: AnalysisFrequencyResource;
}

export type BoardChannelRequest = ChannelTerminalRequest & { components?: readonly PhysicalComponentRequest[] };

export const MAX_CHANNEL_COMPONENTS = 8;

export type ComponentIslandEdge = readonly [string,string];

/** Reachability for an authored model dependency graph. No copper or DC
 * island IDs are merged. Stop at barriers when seeding signal geometry. */
export function componentReachableIslands(seeds:Iterable<string>, edges:readonly ComponentIslandEdge[], barriers:ReadonlySet<string>=new Set()){
 const reachable=new Set(seeds);
 for(const island of reachable)for(const [a,b] of edges){
  if(a===island&&!barriers.has(b))reachable.add(b);
  if(b===island&&!barriers.has(a))reachable.add(a);
 }
 return reachable;
}

export interface ChannelAttachment {
 id: string;
 cut: ChannelCut;
 model: AnalysisFrequencyResource;
}

type PassiveElement = Extract<LocalModel, {kind: 'resistor' | 'capacitor' | 'inductor'}>;
type Entry = [number, number, number];
const fail=(message:string):never=>{throw new Error('CHANNEL_COMPONENT: '+message);};

/** The exact supplied constant series model, with no implicit ground. */
export function channelPassiveElement(id:string, element:PassiveElement, bandHz?:[number,number]):AnalysisFrequencyResource{
 let resistance=0, inductance=0, capacitance=0;
 if(element.kind==='resistor'){
  if(element.referenceC!==undefined||element.temperatureCoefficientPerK!==undefined)
   fail('Temperature-dependent component resistance is not supported: '+id);
  resistance=element.resistanceOhm;
  if(!(resistance>0))fail('Positive explicit component resistance required: '+id);
 }else if(element.kind==='inductor'){
  resistance=element.seriesResistanceOhm??0;
  inductance=element.inductanceH;
  if(!(inductance>0))fail('Positive explicit component inductance required: '+id);
 }else{
  resistance=element.esrOhm??0;
  inductance=element.eslH??0;
  capacitance=element.capacitanceF;
  if(!(capacitance>0))fail('Positive explicit component capacitance required: '+id);
 }
 if([resistance,inductance,capacitance].some(value=>!Number.isFinite(value)||value<0))
  fail('Nonnegative finite component coefficients required: '+id);

 const hasCurrent=resistance>0||inductance>0;
 const seriesNode=capacitance>0&&hasCurrent;
 const potentialNodes=seriesNode?['positive','negative','internal']:['positive','negative'];
 const portIncidence:Entry[]=[[0,0,1],[1,0,-1]];
 const incidence:Entry[]=hasCurrent?[[0,0,1],[seriesNode?2:1,0,-1]]:[];
 const a=seriesNode?2:0;
 const capacitanceF:Entry[]=capacitance>0?[[a,a,capacitance],[1,1,capacitance],[a,1,-capacitance],[1,a,-capacitance]]:[];
 const data:CoupledRlcData={
  format:'coupled-rlc',referenceTopology:'infinity',potentialNodes,currentDofs:hasCurrent?['series']:[],
  portIncidence,incidence,capacitanceF,resistanceOhm:resistance>0?[[0,0,resistance]]:[],
  inductanceH:inductance>0?[[0,0,inductance]]:[],
  structuralGaugeModes:[potentialNodes.map(()=>1)],
  ...(seriesNode&&{conservedPotentialModes:[[0,1,0]]}),
  portTerminals:[{positive:{node:'positive'},negative:{node:'negative'}}],
 };
 return {
  id,kind:'network',ports:['terminals'],analyses:['networkParameters'],data,
  source:'Authored constant passive component equivalent',
  validity:{status:'conditional',reason:'Component equivalent supplied by the caller',...(bandHz&&{domain:{bandHz}})},
 };
}

/** Check both pins are represented by copper terminals before starting FEM. */
export function validateChannelAttachments(blocks:readonly {id:string;cuts:readonly ChannelCut[]}[],attachments:readonly ChannelAttachment[]):[number,number][]{
 if(!attachments.length)return [];
 if(blocks.length+attachments.length>32)fail('Copper and component instances exceed the composition budget');
 const ids=new Set(blocks.map(block=>block.id));
 const terminals=new Set(blocks.flatMap(block=>block.cuts.flatMap(({positive,negative})=>[positive,negative])));
 const bands:[number,number][]=[];
 for(const {id,cut,model} of attachments){
  if(!id.trim()||ids.has(id))fail('Duplicate or empty attachment instance ID: '+id);
  ids.add(id);
  if(cut.positive===cut.negative||!terminals.has(cut.positive)||!terminals.has(cut.negative))
   fail('Both distinct component pins must resolve to exact exposed copper terminals: '+id);
  const descriptor=model.data as CoupledRlcData|undefined;
  const pair=descriptor?.portTerminals?.[0];
  if(model.kind!=='network'||model.ports.length!==1||descriptor?.format!=='coupled-rlc'||descriptor.referenceTopology!=='infinity'||
     descriptor.portTerminals?.length!==1||!pair||!('node' in pair.positive)||!('node' in pair.negative)||pair.positive.node===pair.negative.node)
   fail('Component requires one floating physical port in a passive P6 RLC descriptor: '+id);
  if(model.analyses&&!model.analyses.includes('networkParameters'))
   fail('Component Resource does not authorize networkParameters: '+id);
  const validity=model.validity as {status?:unknown}|undefined;
  if(validity?.status!==undefined&&validity.status!=='valid'&&validity.status!=='conditional')
   fail('Component Resource is unavailable: '+id);
  const band=declaredNetworkBand(model);
  if(band)bands.push(band);
 }
 return bands;
}
