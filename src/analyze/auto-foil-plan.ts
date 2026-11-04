import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
import type { PhysicalPort, PhysicalTerminal } from './channel-terminals.ts';

type Bounds = [[number,number],[number,number]];
type Source = {regionId:string;sourcePath:string;layer:string};
type BarrelSource = {barrelId:string;sourcePath:string;layers:string[]};
export interface AutoFoilPlan {
 boardJson:string;
 geometry:Record<string,unknown>;
 ports:readonly PhysicalPort[];
 terminals:readonly PhysicalTerminal[];
 electrodePairs:[string,string][];
 sourceCopper:Source[];
 sourceBarrels:BarrelSource[];
 roiMm:Bounds;
}

type Options = {
 boardJson:string;
 geometry:Record<string,unknown>&{stack:{name:string;copper:boolean}[];maxTriangleAreaM2:number};
 captured:AnalysisGeometryReport;
 terminals:PhysicalTerminal[];
 ports:PhysicalPort[];
 seedRegions:Set<string>;
 marginMm:number;
 deadlineMs:number;
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
export function autoFoilPlan({boardJson,geometry,captured,terminals,ports,seedRegions,marginMm,deadlineMs}:Options):AutoFoilPlan|undefined {
 const stack=geometry.stack,copper=stack.filter(layer=>layer.copper);
 if(copper.length<2||copper.length>8||stack.length!==2*copper.length-1||stack.some((layer,i)=>layer.copper!==(i%2===0))||
    captured.status==='invalid'||captured.excluded.length||captured.fills.some(fill=>fill.status!=='available'))return;
 if(!(geometry.maxTriangleAreaM2>0)||!Number.isFinite(geometry.maxTriangleAreaM2))return;
 const groups=new Map(captured.islands.flatMap(island=>island.region_ids.map(id=>[id,island.region_ids] as const)));
 const regions=captured.regions.map(region=>({region,box:bounds(region.polygon.outer)}));
 const platingMm=Number(geometry.finishedHolePlatingM)*1000;
 const platingKnown=Number.isFinite(platingMm)&&platingMm>0;
 const holes=captured.holes.map(hole=>({
  hole,
  box:expand(bounds(hole.polygons.flatMap(polygon=>polygon.outer)),
   hole.plated?(platingKnown?platingMm:0)+2*captured.boolean_error_bound_mm:0),
 }));
 const selected=new Set(seedRegions);
 const barrels=new Map<string,typeof holes[number]>();
 const intersects=(box:Bounds,roi:Bounds)=>[0,1].every(k=>box[0][k]<=roi[1][k]&&box[1][k]>=roi[0][k]);
 for(;;){
  if(performance.now()>=deadlineMs)
   throw new Error('CHANNEL_FOIL_DEADLINE: shared geometry closure deadline exhausted');
  const owned=regions.filter(({region})=>selected.has(region.id));
  if(!owned.length)return;
  const roiMm=expand(bounds([...owned,...barrels.values()].flatMap(({box})=>box)),marginMm);
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
   continue;
  }
  const neighbours=regions.filter(({region,box})=>!selected.has(region.id)&&intersects(box,roiMm));
  if(neighbours.length){
   for(const {region} of neighbours){
    const island=groups.get(region.id);
    if(!island)return;
    for(const id of island)selected.add(id);
   }
   continue;
  }
  const names=new Map(terminals.map((terminal,i)=>[terminal.id,'foil-terminal-'+i]));
  const electrodePairs=ports.map(({positive,negative}):[string,string]=>[names.get(positive)!,names.get(negative)!]);
  const electrodes=terminals.map(({id,sourcePath,layer})=>({name:names.get(id)!,sourcePath,layer,upper:layer===copper.at(-1)!.name}));
  if(electrodes.some(electrode=>!copper.some(layer=>layer.name===electrode.layer)))return;
  const sourceCopper=owned.map(({region})=>({regionId:region.id,sourcePath:region.source.path,layer:region.layer}));
  const sourceBarrels=[...barrels.values()].map(({hole})=>({barrelId:hole.id,sourcePath:hole.source.path,layers:hole.layers}));
  return {boardJson,ports,terminals,electrodePairs,sourceCopper,sourceBarrels,roiMm,
   geometry:{...geometry,roiMm,domainPolygonMm:undefined,cutPlanes:{},allowInsulatedCopperCuts:false,electrodes,ports:electrodePairs}};
 }
}