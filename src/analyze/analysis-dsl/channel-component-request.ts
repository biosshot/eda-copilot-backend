import type { AnalysisFrequencyResource, AnalysisGeometryReport } from '../../../native/analyze/index.js';
import type { PhysicalTerminal } from '../channel-terminals.ts';
import { channelPassiveElement, MAX_CHANNEL_COMPONENTS, type PhysicalComponentRequest } from '../channel-components.ts';
import { electricalModel } from './electrical-models.ts';
import type { AnalysisTask } from './executor.ts';
import { declaredNetworkBand, networkResource } from './frequency-model.ts';
import { AnalysisDslError } from './schema.ts';
import type { ComponentDescription } from './spec-doc.js';

export interface PhysicalDcComponentRequest {
 id:string;
 designator:string;
 kind:'resistor'|'capacitor'|'inductor';
 pins:readonly [PhysicalTerminal,PhysicalTerminal];
 resistanceOhm:number|null;
}

const fail=(message:string):never=>{throw new AnalysisDslError('CHANNEL_COMPONENT',message);};

/** Resolve existing .components descriptions against original captured pad
 * sources. The planner still owns exposure and bounded local domain selection. */
export function physicalChannelComponents(
 entries:readonly ComponentDescription[],
 task:Pick<AnalysisTask,'board'|'electricalModels'>,
 geometry:AnalysisGeometryReport,
):PhysicalComponentRequest[];
export function physicalChannelComponents(
 entries:readonly ComponentDescription[],
 task:Pick<AnalysisTask,'board'|'electricalModels'>,
 geometry:AnalysisGeometryReport,
 mode:'dc',
):PhysicalDcComponentRequest[];
export function physicalChannelComponents(
 entries:readonly ComponentDescription[],
 task:Pick<AnalysisTask,'board'|'electricalModels'>,
 geometry:AnalysisGeometryReport,
 mode:'ac'|'dc'='ac',
):(PhysicalComponentRequest|PhysicalDcComponentRequest)[]{
 const dcFail=(message:string):never=>{throw new AnalysisDslError('LOCAL_DC_COMPONENT_UNSUPPORTED',message);};
 if(entries.length>MAX_CHANNEL_COMPONENTS)fail('Component bridge count exceeds '+MAX_CHANNEL_COMPONENTS);
 const board=task.board??fail('Component attachment requires the captured board');
 const seen=new Set<string>();
 return entries.map(entry=>{
  const designator=entry.designator;
  if(seen.has(designator)||board.components.filter(c=>c.designator===designator).length!==1)
   fail('Missing or ambiguous component '+designator);
  seen.add(designator);
  const id='component:'+designator;
  const pinNumbers=Object.values(entry.pins??{});
  const [pinA,pinB]=mode==='dc'&&entry.pins?.positive&&entry.pins?.negative?
   [entry.pins.positive,entry.pins.negative]:pinNumbers;
  if(pinNumbers.length!==2||typeof pinA!=='string'||typeof pinB!=='string'||pinA===pinB)
   return fail('Exactly two explicitly mapped single pins required on '+designator);
  const terminal=(pin:string):PhysicalTerminal=>{
   const pads=board.pads.flatMap((pad,index)=>pad.component===designator&&pad.number===pin?[index]:[]);
   if(pads.length!==1)fail('Missing or ambiguous physical component pad '+designator+'.'+pin);
   const sourcePath='pads['+pads[0]+']';
   const regions=geometry.regions.filter(region=>region.source.path===sourcePath);
   if(regions.length!==1||!geometry.islands.some(island=>island.region_ids.includes(regions[0].id)))
    fail('Component pad requires one confirmed copper region on one layer: '+designator+'.'+pin);
   return {id:'component-pin:'+JSON.stringify([designator,pin]),sourcePath,layer:regions[0].layer};
  };
  let model:AnalysisFrequencyResource;
  if(mode==='dc'&&(entry.kind==='device'||entry.kind==='ic'))
   return dcFail('Exact DC requires an explicit R/C/L component equivalent: '+designator);
  if(entry.kind==='device'){
   const supplied=entry.model;
   if(!supplied||supplied.kind!=='network')return fail('Device bridge requires an explicitly supplied floating one-port network: '+designator);
   const resource=networkResource(supplied);
   if(!declaredNetworkBand(resource))fail('Device bridge requires an explicit model applicability band: '+designator);
   if(resource.ports.length!==1||resource.data.format!=='coupled-rlc'||resource.data.referenceTopology!=='infinity')
    fail('Device bridge requires a floating one-port coupled RLC model: '+designator);
   const byRole=entry.pins;
   if(typeof byRole?.positive!=='string'||typeof byRole?.negative!=='string')
    return fail('Authored device port polarity requires positive and negative pin keys: '+designator);
   model={id:resource.id,kind:'network',ports:[...resource.ports],data:resource.data,source:resource.source,validity:resource.validity,
    analyses:resource.analyses&&[...resource.analyses]};
   return {id,designator,pins:[terminal(byRole.positive),terminal(byRole.negative)],model};
  }
  if(entry.kind==='ic')return fail('IC models are outside the bounded two-pin bridge adapter: '+designator);
  const nominal={resistor:'resistanceOhm',capacitor:'capacitanceF',inductor:'inductanceH'} as const;
  if(entry[nominal[entry.kind]]===undefined&&entry.model===undefined)
   fail('Missing explicit component equivalent: '+designator);
  const resolved=electricalModel(entry,task,mode==='dc'?['dc','impedance']:['impedance','networkParameters']);
  const equivalent=resolved.model;
  if(equivalent.kind!=='resistor'&&equivalent.kind!=='capacitor'&&equivalent.kind!=='inductor')
   return fail('Unsupported component equivalent: '+designator);
  const band=entry.model&&'id' in entry.model?declaredNetworkBand(entry.model):undefined;
  if(mode==='dc'){
   if(band&&band[0]!==0)return dcFail('Component model band does not include exact DC: '+designator);
   if(equivalent.kind==='resistor'&&(equivalent.referenceC!==undefined||equivalent.temperatureCoefficientPerK!==undefined))
    return dcFail('Temperature-dependent package resistance is unsupported: '+designator);
   const resistanceOhm=equivalent.kind==='capacitor'?null:
    equivalent.kind==='resistor'?equivalent.resistanceOhm:equivalent.seriesResistanceOhm??0;
   if(resistanceOhm!==null&&(!(resistanceOhm>0)||!Number.isFinite(resistanceOhm)||!Number.isFinite(1/resistanceOhm)))
    return dcFail('Exact DC requires positive finite resistance or inductor DCR; ideal shorts require a different solver: '+designator);
   return {id,designator,kind:equivalent.kind,pins:[terminal(pinA),terminal(pinB)],resistanceOhm};
  }
  model=channelPassiveElement(id,equivalent,band);
  return {id,designator,pins:[terminal(pinA),terminal(pinB)],equivalentKind:equivalent.kind,model};
 });
}
