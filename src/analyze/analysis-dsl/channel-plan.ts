import type {AnalysisGeometryReport} from '../../../native/analyze/index.js';
import {AnalysisDslError} from './schema.ts';

type Contact = {sourcePath:string;layer:string};
type Box = [[number,number],[number,number]];
const fail=(message:string):never=>{throw new AnalysisDslError('CHANNEL_PLAN',message);};
/** Existing native physical connectivity seeds finite numerical domains. The
 * connected-island envelope is audit metadata, not a required volume solve.
 * Returning a model requires separate complex-port/loss domain verification. */
export function channelPlan(g:AnalysisGeometryReport,contacts:Contact[]) {
 if(contacts.length<2)fail('Two physical signal endpoints are required');
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
 const bounds=(id:string):Box=>{const box:Box=[[Infinity,Infinity],[-Infinity,-Infinity]];for(const p of regions.get(id)!.polygon.outer){for(let k=0;k<2;k++){box[0][k]=Math.min(box[0][k],p[k]);box[1][k]=Math.max(box[1][k],p[k]);}}return box;};
 const merge=(boxes:Iterable<Box>):Box=>{const result:Box=[[Infinity,Infinity],[-Infinity,-Infinity]];for(const box of boxes)for(let k=0;k<2;k++){result[0][k]=Math.min(result[0][k],box[0][k]);result[1][k]=Math.max(result[1][k],box[1][k]);}return result;};
 const boxes=new Map(g.regions.map(r=>[r.id,bounds(r.id)]));
 const roi=merge([...chosen].map(id=>boxes.get(id)!));
 const confirmedLocalBarrel=contacts[0].sourcePath===contacts[1].sourcePath&&g.holes.some(h=>h.source.path===contacts[0].sourcePath&&h.plated&&h.span_confirmed);
 const signalSeed=confirmedLocalBarrel?selected.slice(0,2).flat():islandFor(selected[0][0]).region_ids;
 const portBounds=[...new Set([...selected.flat(),...signalSeed])].map(id=>boxes.get(id)!);
 const portBox=merge(portBounds);
 const baseMargin=Math.max(0.05,(portBox[1][0]-portBox[0][0])/4,(portBox[1][1]-portBox[0][1])/4);
 const candidateDomainsMm=Array.from({length:3},(_,i):Box=>{const m=baseMargin*2**i;return [[portBox[0][0]-m,portBox[0][1]-m],[portBox[1][0]+m,portBox[1][1]+m]];});
 const retainedCandidate=candidateDomainsMm.at(-1)!;
 const inBox=(b:Box,box:Box)=>b[0][0]<=box[1][0]&&b[1][0]>=box[0][0]&&b[0][1]<=box[1][1]&&b[1][1]>=box[0][1];
 const selectedIds=g.regions.filter(r=>inBox(boxes.get(r.id)!,candidateDomainsMm[0])).map(r=>r.id);
 const selectedPaths=new Set(g.regions.filter(r=>inBox(boxes.get(r.id)!,retainedCandidate)).map(r=>r.source.path));
 const contactPaths=new Set(contacts.map(c=>c.sourcePath));
 const signalIslandId=islandFor(selected[0][0]).id;
 const copperDecisions=g.regions.map(r=>{
  const island=regionIslands.get(r.id)?.id;
  const insideCandidate=inBox(boxes.get(r.id)!,retainedCandidate);
  const role=island===signalIslandId?'signal':chosen.has(r.id)?'return':'environment';
  return {regionId:r.id,sourcePath:r.source.path,layer:r.layer,islandId:island,role,
   numericalDomain:insideCandidate?'candidate':'outside-candidates',
   reason:insideCandidate?'Captured physical copper intersects the final candidate; retained without net-name joins':
    chosen.has(r.id)?'Connected branch/return copper retained in the source audit, outside finite numerical candidates; truncation requires observable sensitivity verification':
    'Outside all candidate domains; excluded from numerical extraction, preserved in the board snapshot'};
 });
 const parents=new Set<string>();for(const path of selectedPaths){let p=path;while(p.includes('.')){p=p.slice(0,p.lastIndexOf('.'));parents.add(p);}}
 const relevant=(path:string)=>{if(parents.has(path))return true;let p=path;for(;;){if(selectedPaths.has(p))return true;const dot=p.lastIndexOf('.');if(dot<0)return false;p=p.slice(0,dot);}};
 const diagnosticDecisions=g.diagnostics.map(d=>({diagnostic:d,
  inSelectedDomain:relevant(d.path),atPort:contactPaths.has(d.path),
  decision:(!relevant(d.path)?'ignored':'assumed') as 'ignored'|'assumed',
  reason:!relevant(d.path)?'Outside bounded candidate domains and not used to assert port connectivity; preserve global source geometry and check finite-domain error separately':
   d.code==='contact_uncertain'||d.code==='point_contact'?'Approximate contact was not used to join native islands; endpoints have a separate confirmed physical path. Preserve copper and gaps; do not assert this extra connection':
   d.code==='drill_topology_uncertain'?'Native geometry retains enclosing drill contours and does not assert uncertain fragments as a confirmed open. Preserve this conservative geometry; observable sensitivity remains unverified':
   'Retain native captured geometry and its reported limitation; no topology repair or ideal net-name connection',
  validityImpact:relevant(d.path)?'conditional; no 5% spatial/topology certification':'conditional on domain sensitivity; no topology repair or net-name joins'}));
 return {roiMm:candidateDomainsMm[0],conservativeConnectedEnvelopeMm:roi,candidateDomainsMm,automaticConvergenceRequired:true,selectedRegionIds:selectedIds.sort(),connectedIslandRegionIds:[...chosen].sort(),copperDecisions,contacts,signalIsland:signalIslandId,
  policy:confirmedLocalBarrel?'confirmed plated-barrel seed; captured intersecting branches and surrounding copper retained; external branch cuts require observable domain sensitivity; no net-name joins':'full confirmed signal-island envelope seeds paths/branches; surrounding domains require observable convergence; no shortest-path substitution or user-authored cuts',
  spatialVerification:{operator:'same direct-FEM polynomial magnetic operator; two bounded local patch levels',levels:[{traceDegree:4,bodyDegree:6,stars:1,exteriorOrder:16},{traceDegree:6,bodyDegree:8,stars:1,exteriorOrder:16}],gate:'2% canonical50Ohm power-wave change with0.01 amplitude floor and intrinsic Cu total loss change; unloaded reactive Z and regional losses unverified'},geometryDiagnostics:g.diagnostics,diagnosticDecisions,spatialValidation:'unverified; environment expansion/observable accuracy not certified',confirmedLocalBarrel};
}
