import type {AnalysisGeometryReport} from '../../../native/analyze/index.js';
import {AnalysisDslError} from './schema.ts';

type Contact = {sourcePath:string;layer:string};
type Box = [[number,number],[number,number]];
const fail=(message:string):never=>{throw new AnalysisDslError('CHANNEL_PLAN',message);};
/** Conservative first domain: retain complete physical islands and every copper
 * region intersecting their envelope. No net-name joins, shortest-path choice,
 * clipped plane or inferred ideal return. Native FEM still enforces its budget.
 * This is deliberately not a certificate of spatial observable convergence. */
export function channelPlan(g:AnalysisGeometryReport,contacts:Contact[]) {
 if(g.status==='invalid'||g.excluded.length||g.fills.some(f=>f.status!=='available'))fail('Complete captured copper geometry is required for automatic channel planning');
 if(g.regions.length>100000)fail('Automatic channel planning region budget exceeded');
 const regionIslands=new Map(g.islands.flatMap(i=>i.region_ids.map(id=>[id,i] as const)));
 const regions=new Map(g.regions.map(r=>[r.id,r]));
 const selected=contacts.map(c=>g.regions.filter(r=>r.source.path===c.sourcePath&&r.layer===c.layer).map(r=>r.id));
 if(selected.some(s=>s.length!==1))fail('Each physical contact must select exactly one captured copper region');
 const islandFor=(id:string)=>regionIslands.get(id)??fail('Contact has no confirmed physical island');
 // localRequest orders positive input/output, then their returns.
 if(islandFor(selected[0][0]).id!==islandFor(selected[1][0]).id)fail('Signal endpoints are physically disconnected; net names do not connect copper');
 const chosen=new Set(selected.flatMap(s=>islandFor(s[0]).region_ids));
 const bounds=(id:string):Box=>{const p=regions.get(id)!.polygon.outer;return [[Math.min(...p.map(v=>v[0])),Math.min(...p.map(v=>v[1]))],[Math.max(...p.map(v=>v[0])),Math.max(...p.map(v=>v[1]))]];};
 const boxes=new Map(g.regions.map(r=>[r.id,bounds(r.id)]));
 let roi:Box=[[0,0],[0,0]],changed=true;
 let iterations=0;
 while(changed){
  if(++iterations>Math.min(g.regions.length+1,256))fail('Automatic domain closure did not terminate');
  const b=[...chosen].map(id=>boxes.get(id)!);
  roi=[[Math.min(...b.map(v=>v[0][0])),Math.min(...b.map(v=>v[0][1]))],[Math.max(...b.map(v=>v[1][0])),Math.max(...b.map(v=>v[1][1]))]];
  changed=false;
  for(const [id,b]of boxes)if(!chosen.has(id)&&b[0][0]<=roi[1][0]&&b[1][0]>=roi[0][0]&&b[0][1]<=roi[1][1]&&b[1][1]>=roi[0][1]){
   // Keep the entire island: a return connection may leave this envelope.
   for(const member of islandFor(id).region_ids)if(!chosen.has(member)){chosen.add(member);changed=true;}
  }
 }
 const selectedPaths=new Set([...chosen].map(id=>regions.get(id)!.source.path));
 const uncertain=g.diagnostics.filter(d=>['contact_uncertain','drill_topology_uncertain'].includes(d.code)&&selectedPaths.has(d.path));
 if(uncertain.length)fail(`Automatic domain contains unresolved physical topology: ${uncertain.slice(0,6).map(d=>d.code+' at '+d.path).join('; ')+(uncertain.length>6?`; ${uncertain.length-6} more`: '')}`);
 const margin=Math.max(g.boolean_error_bound_mm*4,0.001);
 roi=[[roi[0][0]-margin,roi[0][1]-margin],[roi[1][0]+margin,roi[1][1]+margin]];
 // The margin must not introduce a fresh cut. Expand through the same closure
 // using the full envelope on the next request rather than accepting a cut.
 for(const [id,b]of boxes)if(!chosen.has(id)&&b[0][0]<roi[1][0]&&b[1][0]>roi[0][0]&&b[0][1]<roi[1][1]&&b[1][1]>roi[0][1])fail('Automatic boundary margin encounters additional copper; domain expansion required');
 return {roiMm:roi,selectedRegionIds:[...chosen].sort(),contacts,signalIsland:islandFor(selected[0][0]).id,
  policy:'complete physical islands; branches retained; no shortest path; no insulated copper cuts',
  geometryDiagnostics:g.diagnostics,spatialValidation:'unverified; environment expansion/observable accuracy not certified',iterations};
}
