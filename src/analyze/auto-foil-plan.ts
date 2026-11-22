import type { AnalysisBoardFoilCheckedRequest, AnalysisGeometryReport } from '../../native/analyze/index.js';
import type { PhysicalPort, PhysicalTerminal } from './channel-terminals.ts';
import {MAX_CHANNEL_COMPONENTS,type PhysicalComponentRequest,type PhysicalModePair} from './channel-components.ts';

type Bounds = [[number,number],[number,number]];
export type FoilClosureStep = {
 iteration:number; reason:'seed'|'barrel-envelope'|'intersecting-islands'|'complete';
 roiMm?:Bounds; selectedRegions:number; selectedBarrels:number;
 addedRegionIds:string[]; addedBarrelIds:string[]; intersectingRegionIds?:string[];
};
type Source = {regionId:string;sourcePath:string;layer:string};
type BarrelSource = {barrelId:string;sourcePath:string;layers:string[]};
type Electrode = {name:string;sourcePath:string;layer:string;upper:boolean};
export interface AutoFoilPlan {
 boardJson:string;
 geometry:Record<string,unknown>&{electrodes:Electrode[]};
 ports:readonly PhysicalPort[];
 modePairs?:readonly PhysicalModePair[];
 terminals:readonly PhysicalTerminal[];
 electrodePairs:[string,string][];
 sourceCopper:Source[];
 sourceBarrels:BarrelSource[];
 attachments:NonNullable<AnalysisBoardFoilCheckedRequest["attachments"]>;
 roiMm:Bounds;
 /** Internal, axis-aligned boundary for the adaptive return-current audit. */
 observationRoiMm?:Bounds;
 /** In-memory planner context; never part of the native request. */
 returnSearch?:{initialMarginMm:number;expand:(domainScale:number,deadlineMs:number)=>AutoFoilPlan};
 domain:{policy:'oriented-run-v1'|'axis-aligned-fallback-v1';reason:string;polygonMm?:[number,number][]};
}

type Options = {
 boardJson:string;
 geometry:Record<string,unknown>&{stack:{name:string;copper:boolean}[];maxTriangleAreaM2:number};
 captured:AnalysisGeometryReport;
 terminals:PhysicalTerminal[];
 ports:PhysicalPort[];
 modePairs?:readonly PhysicalModePair[];
 components:readonly PhysicalComponentRequest[];
 seedRegions:Set<string>;
 marginMm:number;
 guardMarginMm?:number;
 deadlineMs:number;
 onClosureStep?:(step:FoilClosureStep)=>void;
 direction?:[number,number];
 directionReason?:string;
};
function bounds(points:readonly [number,number][]):Bounds {
 const box:Bounds=[[Infinity,Infinity],[-Infinity,-Infinity]];
 for(const [x,y] of points){
  box[0][0]=Math.min(box[0][0],x);box[0][1]=Math.min(box[0][1],y);
  box[1][0]=Math.max(box[1][0],x);box[1][1]=Math.max(box[1][1],y);
 }
 return box;
}

function expand(box:Bounds, distanceMm:number):Bounds {
 return [[box[0][0]-distanceMm,box[0][1]-distanceMm],[box[1][0]+distanceMm,box[1][1]+distanceMm]];
}

/** Close a bounded candidate over physical islands and complete intersecting
 * neighbours. Rectangle tests are conservative. Native source checks decide
 * actual admission and guard against every clipped conductor. */
export function autoFoilPlan({boardJson,geometry,captured,terminals,ports,modePairs,components,seedRegions,marginMm,guardMarginMm,deadlineMs,onClosureStep,direction,directionReason}:Options):AutoFoilPlan|undefined {
 // A caller-supplied polygon is an explicit physical scope; do not silently
 // replace it with an automatic closure.
 if(geometry.domainPolygonMm!==undefined)return;
 if(guardMarginMm!==undefined&&(!(guardMarginMm>0)||!Number.isFinite(guardMarginMm)))return;
 // The observation window is axis-aligned; keep it inside the solved domain.
 if(guardMarginMm!==undefined)direction=undefined;
 if(direction&&(!direction.every(Number.isFinite)||Math.abs(Math.hypot(...direction)-1)>1e-12))return;
 const stack=geometry.stack,copper=stack.filter(layer=>layer.copper);
 if(copper.length<2||copper.length>8||stack.length!==2*copper.length-1||stack.some((layer,i)=>layer.copper!==(i%2===0))||
    captured.status==='invalid'||captured.excluded.length||captured.fills.some(fill=>fill.status!=='available'))return;
 if(!(geometry.maxTriangleAreaM2>0)||!Number.isFinite(geometry.maxTriangleAreaM2))return;
 if(components.length>MAX_CHANNEL_COMPONENTS||ports.length+components.length>16||terminals.length>32)
  throw new Error('CHANNEL_FOIL_COMPONENT: at most 8 package branches, 16 total port pairs and 32 electrodes required');
 if(components.some(component=>!component.equivalentKind))
  throw new Error('CHANNEL_FOIL_COMPONENT: joint extraction requires an explicit two-terminal R/C/L equivalent for each package');
 const names=new Map(terminals.map((terminal,i)=>[terminal.id,'foil-terminal-'+i]));
 const attachments=components.map(({id,model,pins:[a,b]})=>{
  const positive=names.get(a.id),negative=names.get(b.id);
  if(!positive||!negative||positive===negative)
   throw new Error('CHANNEL_FOIL_COMPONENT: distinct retained physical pads required for '+id);
  return {id,model,electrodes:[positive,negative] as [string,string]};
 });
 const groups=new Map(captured.islands.flatMap(island=>island.region_ids.map(id=>[id,island.region_ids] as const)));
 // Only automatic hybrid candidates provide a physical run direction. All
 // bounds, including the closure search, use the same orthonormal frame.
 const d=direction??[1,0];const t:[number,number]=[-d[1],d[0]];
 const project=([x,y]:[number,number]):[number,number]=>[x*d[0]+y*d[1],x*t[0]+y*t[1]];
 const worldCorners=(box:Bounds):[number,number][]=>[[box[0][0],box[0][1]],[box[1][0],box[0][1]],[box[1][0],box[1][1]],[box[0][0],box[1][1]]]
  .map(([x,y]):[number,number]=>[x*d[0]+y*t[0],x*d[1]+y*t[1]]);
 const worldBounds=(box:Bounds):Bounds=>direction?bounds(worldCorners(box)):box;
 const regions=captured.regions.map(region=>({region,box:bounds(region.polygon.outer.map(project))}));
 const platingMm=Number(geometry.finishedHolePlatingM)*1000;
 const platingKnown=Number.isFinite(platingMm)&&platingMm>0;
 const holes=captured.holes.map(hole=>({
  hole,
  box:expand(bounds(hole.polygons.flatMap(polygon=>polygon.outer).map(project)),
   hole.plated?(platingKnown?platingMm:0)+2*captured.boolean_error_bound_mm:0),
 }));
 const selected=new Set(seedRegions);
 const seedBoxes=regions.filter(({region})=>selected.has(region.id)).map(({box})=>box);
 if(!seedBoxes.length)return;
 const observationRoiMm=guardMarginMm===undefined?undefined:expand(bounds(seedBoxes.flatMap(box=>[box[0],box[1]])),marginMm);
 const barrels=new Map<string,typeof holes[number]>();
 let iteration=0;
 onClosureStep?.({iteration,reason:'seed',selectedRegions:selected.size,selectedBarrels:0,addedRegionIds:[...selected],addedBarrelIds:[]});
 const intersects=(box:Bounds,roi:Bounds)=>[0,1].every(k=>box[0][k]<=roi[1][k]&&box[1][k]>=roi[0][k]);
 for(;;){
  iteration++;
  if(performance.now()>=deadlineMs)
   throw new Error('CHANNEL_FOIL_DEADLINE: shared geometry closure deadline exhausted');
  const owned=regions.filter(({region})=>selected.has(region.id));
  if(!owned.length)return;
  const roiMm=expand(bounds([...owned,...barrels.values()].flatMap(({box})=>box)),marginMm+(guardMarginMm??0));
  // A bore just outside the box may have a plating wall inside it. Close
  // against the entire conservative plating envelope before native meshing.
  const nearbyHoles=holes.filter(({hole,box})=>!barrels.has(hole.id)&&intersects(box,roiMm));
  if(nearbyHoles.some(({hole})=>!hole.span_confirmed))
   throw new Error('CHANNEL_FOIL_PADSTACK: a hole in the closed candidate has no confirmed physical span');
  const newBarrels=nearbyHoles.filter(({hole})=>hole.plated);
  if(newBarrels.length){
   if(!platingKnown||!Number.isFinite(geometry.barrelConductivitySM)||Number(geometry.barrelConductivitySM)<=0)
    throw new Error('CHANNEL_FOIL_PADSTACK: retained barrels require positive plating thickness and conductivity');
   for(const entry of newBarrels)barrels.set(entry.hole.id,entry);
   onClosureStep?.({iteration,reason:'barrel-envelope',roiMm:worldBounds(roiMm),selectedRegions:selected.size,selectedBarrels:barrels.size,addedRegionIds:[],addedBarrelIds:newBarrels.map(entry=>entry.hole.id)});
   continue;
  }
  const neighbours=regions.filter(({region,box})=>!selected.has(region.id)&&intersects(box,roiMm));
  if(neighbours.length){
   const addedRegionIds:string[]=[];
   for(const {region} of neighbours){
    const island=groups.get(region.id);
    if(!island)return;
    for(const id of island){if(onClosureStep&&!selected.has(id))addedRegionIds.push(id);selected.add(id);}
   }
   onClosureStep?.({iteration,reason:'intersecting-islands',roiMm:worldBounds(roiMm),selectedRegions:selected.size,selectedBarrels:barrels.size,addedRegionIds,addedBarrelIds:[],intersectingRegionIds:neighbours.map(entry=>entry.region.id)});
   continue;
  }
  const electrodePairs=ports.map(({positive,negative}):[string,string]=>[names.get(positive)!,names.get(negative)!]);
  const electrodes=terminals.map(({id,sourcePath,layer})=>({name:names.get(id)!,sourcePath,layer,upper:layer===copper.at(-1)!.name}));
  if(electrodes.some(electrode=>!copper.some(layer=>layer.name===electrode.layer)))return;
  const sourceCopper=owned.map(({region})=>({regionId:region.id,sourcePath:region.source.path,layer:region.layer}));
  const sourceBarrels=[...barrels.values()].map(({hole})=>({barrelId:hole.id,sourcePath:hole.source.path,layers:hole.layers}));
  const polygonMm=worldCorners(roiMm);
  const physicalRoi=direction?bounds(polygonMm):roiMm;
  const domain=direction?{policy:'oriented-run-v1' as const,reason:'unambiguous aligned physical runs',polygonMm}:
   {policy:'axis-aligned-fallback-v1' as const,reason:directionReason??'hybrid direction not requested'};
  onClosureStep?.({iteration,reason:'complete',roiMm:physicalRoi,selectedRegions:selected.size,selectedBarrels:barrels.size,addedRegionIds:[],addedBarrelIds:[]});
  return {boardJson,ports,...(modePairs?.length&&{modePairs}),terminals,electrodePairs,sourceCopper,sourceBarrels,attachments,roiMm:physicalRoi,...(observationRoiMm&&{observationRoiMm}),domain,
   geometry:{...geometry,roiMm:physicalRoi,domainPolygonMm:direction?polygonMm:undefined,cutPlanes:{},allowInsulatedCopperCuts:false,electrodes,ports:electrodePairs}};
 }
}
