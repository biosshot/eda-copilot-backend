import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
import type { PhysicalPort, PhysicalTerminal } from './channel-terminals.ts';

type Bounds = [[number,number],[number,number]];
type Source = {regionId:string;sourcePath:string;layer:string};
export interface AutoFoilPlan {
 boardJson:string;
 geometry:Record<string,unknown>;
 ports:readonly PhysicalPort[];
 terminals:readonly PhysicalTerminal[];
 electrodePairs:[string,string][];
 sourceCopper:Source[];
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
 hasVias:boolean;
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

/** Close a bounded candidate over physical islands and complete intersecting
 * neighbours. Rectangle tests are conservative. Native source checks decide
 * actual admission and guard against every clipped conductor. */
export function autoFoilPlan({boardJson,geometry,captured,terminals,ports,seedRegions,marginMm,hasVias,deadlineMs}:Options):AutoFoilPlan|undefined {
 const stack=geometry.stack,copper=stack.filter(layer=>layer.copper);
 if(copper.length<2||copper.length>8||stack.length!==2*copper.length-1||stack.some((layer,i)=>layer.copper!==(i%2===0))||hasVias||captured.holes.some(hole=>hole.plated)||
    captured.status==='invalid'||captured.excluded.length||captured.fills.some(fill=>fill.status!=='available'))return;
 if(!(geometry.maxTriangleAreaM2>0)||!Number.isFinite(geometry.maxTriangleAreaM2))return;
 const groups=new Map(captured.islands.flatMap(island=>island.region_ids.map(id=>[id,island.region_ids] as const)));
 const regions=captured.regions.map(region=>({region,box:bounds(region.polygon.outer)}));
 const selected=new Set(seedRegions);
 const intersects=(box:Bounds,roi:Bounds)=>[0,1].every(k=>box[0][k]<=roi[1][k]&&box[1][k]>=roi[0][k]);
 for(;;){
  if(performance.now()>=deadlineMs)
   throw new Error('CHANNEL_FOIL_DEADLINE: shared geometry closure deadline exhausted');
  const owned=regions.filter(({region})=>selected.has(region.id));
  if(!owned.length)return;
  const roiMm=bounds(owned.flatMap(({box})=>box));
  for(let k=0;k<2;k++){roiMm[0][k]-=marginMm;roiMm[1][k]+=marginMm;}
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
  return {boardJson,ports,terminals,electrodePairs,sourceCopper,roiMm,
   geometry:{...geometry,roiMm,domainPolygonMm:undefined,cutPlanes:{},allowInsulatedCopperCuts:false,electrodes,ports:electrodePairs}};
 }
}