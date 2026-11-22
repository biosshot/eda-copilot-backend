import type {AnalysisThermalNetwork} from '../../../native/analyze/index.js';
import type {AnalysisTask} from './executor.ts';
import type * as T from './spec-doc.js';
import {AnalysisDslError} from './schema.ts';
import {thermalCooling} from './thermal-executor.ts';
import {json,} from './data.ts';
import {thermalLossTransfer,type ThermalSolutionResolver} from './thermal-loss-transfer.ts';
function reject(message:string):never{throw new AnalysisDslError('THERMAL_BOARD',message);}
function local(model:T.Model|undefined,kinds:string[]):T.LocalModel{if(!model||'id'in model||!kinds.includes(model.kind))reject(`Explicit local ${kinds.join('/')} model required`);return model;}
/** Geometry and coefficients are resolved explicitly; FEM assembly remains Rust. */
export function thermalBoard(task:AnalysisTask<'thermal'>,resolve?:ThermalSolutionResolver):{network:AnalysisThermalNetwork;board:unknown;conditional:boolean;conditionalReasons:string[];componentNodes:Map<string,string>;lossTransfer?:T.Json} {
 const c=task.config;if(!task.board||!task.boardIdentity||!c.stackup)reject('Board mode needs board identity and explicit stackup');
 if(c.mesh?.maxNodes!==undefined&&c.mesh.maxNodes<4)reject('Thermal maxNodes must be >=4');
 if(c.stackup.input.solderMask)reject('Solder mask thermal physics requires an explicit separate layer model');
 const network:AnalysisThermalNetwork={nodes:[],links:[],sources:[]};const contacts:unknown[]=[];const heat:unknown[]=[];const componentNodes=new Map<string,string>();
 let conditional=c.inputPolicy?.cadFill==='snapshot';const conditionalReasons:string[]=[];
 const material=(name:string|undefined)=>{const m=name?c.materials?.[name]:undefined;if(!m||m.thermalConductivityWPerMK===undefined||!m.source?.trim()||!m.state?.trim())reject(`Material ${name??'(missing)'} requires k, source and state`);if(m.validity?.status==='unavailable'||m.temperatureCoefficientPerK!==undefined||m.referenceC!==undefined)reject('Unavailable/temperature-dependent thermal properties are unsupported');if(m.validity?.status==='conditional')conditional=true;if((c.domain??'time')==='time'&&(m.densityKgM3===undefined||m.specificHeatJPerKgK===undefined))reject(`Material ${name} requires explicit density/specific heat in time`);return{thermalConductivityWPerMK:m.thermalConductivityWPerMK,densityKgM3:m.densityKgM3,specificHeatJPerKgK:m.specificHeatJPerKgK,source:m.source,state:m.state};};
 const layers=c.stackup.input.layers.map((l,i)=>({kind:l.kind,name:l.name,thicknessMm:c.stackup!.routing.layers[i].thicknessMm,material:material(l.material),...(l.kind==='copper'?{background:material(l.backgroundMaterial)}:{})}));
 const designators=new Set<string>();
 for(const component of c.components??[]){
  if(designators.has(component.designator)||task.board.components.filter(x=>x.designator===component.designator).length!==1)reject(`Duplicate/unknown component ${component.designator}`);designators.add(component.designator);
  if(component.pins||component.ground||component.supplies)reject('Electrical contact mappings cannot supply thermal paths');
  const model=local(component.model,['thermal-path','thermal-network']) as Extract<T.LocalModel,{kind:'thermal-path'|'thermal-network'}>;
  const id=(node:string)=>`${component.designator}:${node}`;
  for(const n of model.nodes){const name=id(n.id);componentNodes.set(JSON.stringify({target:{kind:'component',designator:component.designator},node:n.id}),name);network.nodes.push({...n,id:name});}
  for(const l of model.links)network.links.push({id:`${component.designator}:${l.id??`${l.from}->${l.to}`}`,from:id(l.from),to:id(l.to),cooling:thermalCooling(l.model,l.resistanceKPerW)});
  for(const [i,contact]of(model.contacts??[]).entries()){
   if(!model.nodes.some(n=>n.id===contact.node))reject(`Unknown compact contact node ${contact.node}`);
   contacts.push({id:`${component.designator}:${i}`,node:id(contact.node),target:contact.target,resistanceKPerW:contact.resistanceKPerW});
  }
 }
 if(c.heat&&!Array.isArray(c.heat))reject('Thermal category defaults require explicit board classification; use authored heat array');
 const assigned=new Set<string>();
 for(const [i,h]of(Array.isArray(c.heat)?c.heat:[]).entries()){
  const key=JSON.stringify([h.target,h.node]);if(assigned.has(key))reject('Duplicate heat assignment');assigned.add(key);
  const compact=componentNodes.get(JSON.stringify({target:h.target,node:h.node}));
  if(compact)network.sources.push({node:compact,powerW:h.powerW});
  else{if(h.node!=='board')reject(`Unknown thermal node ${h.node}; direct physical heat requires node:"board"`);heat.push({id:`heat-${i}`,target:h.target,node:h.node,powerW:h.powerW});}
 }
 const transfer=thermalLossTransfer(task,resolve);if(transfer){heat.push(...transfer.heat);conditional ||= transfer.conditional;if(transfer.conditionalReason)conditionalReasons.push(transfer.conditionalReason);}
 for(const [i,h]of(c.cooling?.heatsinks??[]).entries()){
  if(h.target.kind!=='component')reject('Heatsink target requires explicit compact component node');
  const compact=componentNodes.get(JSON.stringify({target:h.target,node:h.thermalNode}));if(!compact)reject('Unknown heatsink contact thermal node');
  const model=local(h.model,['heatsink']) as Extract<T.LocalModel,{kind:'heatsink'}>;
  const face=local(h.interface,['thermal-interface']) as Extract<T.LocalModel,{kind:'thermal-interface'}>;
  const sink=`heatsink:${i}`;network.nodes.push({id:sink,heatCapacityJPerK:model.heatCapacityJPerK});
  network.links.push({id:`tim:${i}`,from:compact,to:sink,cooling:thermalCooling(face)});
  // The board preparation creates the named environment node.
  network.links.push({id:`heatsink-cooling:${i}`,from:sink,to:'pcb:ambient',cooling:thermalCooling(h.cooling)});
 }
 const surfaceCooling=c.cooling?.boardSurfaces?thermalCooling(c.cooling.boardSurfaces):undefined;
 const stack=c.stackup.input;const plated=task.board.copper.fixed.vias.length+task.board.copper.editable.vias.length+task.board.pads.filter(p=>p.hole?.plated).length>0;
 const barrel=plated?{thicknessMm:(c.stackup.routing.viaPlatingThicknessUm??reject('Explicit viaPlatingThicknessUm required'))/1000,diameterConvention:stack.diameterConvention??reject('Explicit plating diameterConvention required'),material:material(stack.platingMaterial)}:undefined;
 return{network,board:json({boardJson:JSON.stringify(task.board),boardIdentity:task.boardIdentity,layers,maxEdgeMm:c.mesh?.maxEdgeMm??2,geometryDeviationMm:c.mesh?.geometryDeviationMm??.001,maxNodes:c.mesh?.maxNodes??50000,refinement:c.mesh?.refinement,snapshot:c.inputPolicy?.cadFill==='snapshot',heat,contacts,surfaceCooling,barrel,barrelHeat:transfer?.barrelHeat??[]}),conditional,conditionalReasons,componentNodes,...(transfer?{lossTransfer:transfer.provenance}:{})};
}
