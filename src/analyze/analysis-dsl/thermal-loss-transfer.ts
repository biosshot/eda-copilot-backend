import type {AnalysisTask} from './executor.ts';
import type * as T from './spec-doc.js';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
export type ThermalSolutionResolver=(contextId:string,id:string)=>T.Json|undefined;
function reject(message:string):never{throw new AnalysisDslError('THERMAL_LOSS_TRANSFER',message);}
/** Resolve immutable completed DC solutions in this context; supplied arrays must
 * exactly match their source, including zero entries. No foreign/trimmed maps. */
export function thermalLossTransfer(task:AnalysisTask<'thermal'>,resolve:ThermalSolutionResolver|undefined):{heat:unknown[];barrelHeat:unknown[];conditional:boolean;conditionalReason?:string;provenance:T.Json}|undefined{
 const map=task.config.losses;if(!map)return;
 if(!resolve||map.contextId!==task.contextId||map.kind!=='map'||map.unit!=='W'||map.validity.status==='unavailable'||!object(map.data)||typeof map.data.solutionId!=='string'||map.data.field!=='loss_w'||map.data.association!=='triangle'||!Array.isArray(map.values))reject('Expected same-context DC triangle loss map with explicit source identity, W and full values');
 const source=resolve(task.contextId,map.data.solutionId);
 if(!object(source)||source.status!=='complete'||source.id!==map.data.solutionId||source.boardIdentity!==task.boardIdentity||!Array.isArray(source.domains)||!source.domains.length)reject('Loss source is foreign, unavailable or does not match this board');
 const idleDomains=Array.isArray(source.idle_domains)?source.idle_domains:[];
 const idleIds=new Set<string>();
 for(const d of idleDomains){if(!object(d)||typeof d.domain_id!=='string'||typeof d.layer!=='string'||!Array.isArray(d.nets)||!d.nets.length||!Array.isArray(d.loss_w)||d.loss_w.some(v=>v!==0))reject('Malformed idle domain: only scenario-asserted zero loss is accepted');idleIds.add(d.domain_id);}
 if(Array.isArray(source.excluded_domains)&&source.excluded_domains.some(d=>!idleIds.has(d as string)))reject('Loss map has uncomputed physical domains; missing regions cannot become zero');
 const idleReason=idleDomains.length?`zero DC current asserted by the scenario for nets ${[...new Set(idleDomains.flatMap(d=>object(d)&&Array.isArray(d.nets)?d.nets.map(String):[]))].join(', ')}`:undefined;
 const canonicalConditional=source.conditional===true||idleDomains.length>0;
 if(canonicalConditional&&map.validity.status!=='conditional')reject('Conditional DC map cannot be relabelled valid');
 let mesh=source.mesh;if(!object(mesh)&&typeof source.meshReference==='string'){const original=resolve(task.contextId,source.meshReference);if(!object(original)||original.boardIdentity!==task.boardIdentity||!object(original.mesh))reject('Loss mesh reference does not resolve within this board/context');mesh=original.mesh;}
 if(!object(mesh)||!Array.isArray(mesh.domains))reject('Loss source has no authoritative computed mesh');
 const expected:number[]=[];const heat:unknown[]=[];let total=0.,zeroSkipped=0;const slices:{domainId:string;start:number;count:number}[]=[];const domainIds=new Set<string>();
 for(const domain of [...source.domains,...idleDomains]){
  if(!object(domain)||typeof domain.domain_id!=='string'||typeof domain.layer!=='string'||!Array.isArray(domain.loss_w))reject('Malformed authoritative loss domain');
  if(domainIds.has(domain.domain_id))reject('Duplicate authoritative loss domain');domainIds.add(domain.domain_id);slices.push({domainId:domain.domain_id,start:expected.length,count:domain.loss_w.length});
  const geometries=mesh.domains.filter(d=>object(d)&&d.id===domain.domain_id);if(geometries.length!==1||!object(geometries[0]))reject('Missing/duplicate loss domain geometry');const geometry=geometries[0];
  if(geometry.layer!==domain.layer||!Array.isArray(geometry.nodes)||!Array.isArray(geometry.triangles)||geometry.triangles.length!==domain.loss_w.length)reject('Loss domain axis/mesh mismatch');
  for(let i=0;i<domain.loss_w.length;i++){
   const power=domain.loss_w[i];if(typeof power!=='number'||!Number.isFinite(power)||power<0)reject('Missing/nonfinite/negative source power');expected.push(power);total+=power;
   const triangle=geometry.triangles[i];if(!Array.isArray(triangle)||triangle.length!==3)reject('Invalid source triangle');
   const outer=triangle.map(index=>{if(typeof index!=='number'||!Number.isSafeInteger(index)||index<0)reject('Invalid source node index');const p=(geometry.nodes as readonly T.Json[])[index];if(!Array.isArray(p)||p.length!==2||!p.every(v=>typeof v==='number'&&Number.isFinite(v)))reject('Invalid source coordinates');return{x:p[0],y:p[1]};});
   // Retain explicit zero entries in evidence; no substitute for missing values.
   // Exact-zero entries stay in the checked evidence above but add no heat; sliver triangles may miss the thermal mesh.
   if(power===0){zeroSkipped++;continue;}
   heat.push({id:`dc:${source.id}:${domain.domain_id}:${i}`,target:{kind:'region',layers:[domain.layer],polygon:{outer}},node:'board',powerW:power});
  }
 }
 if(map.values.length!==expected.length||map.values.some((v,i)=>v!==expected[i]))reject('Loss values are truncated, reordered or differ from their authoritative source');
 if(JSON.stringify(map.data.slices)!==JSON.stringify(slices))reject('Loss domain slices were trimmed, duplicated or relabelled');
 if(typeof source.copper_loss_w!=='number'||Math.abs(total-source.copper_loss_w)>1e-9*Math.max(1,total))reject('Loss source global power does not equal triangle sum');
 const barrelHeat:unknown[]=[];let barrelPower=0.;const barrelIds=new Set<string>();
 for(const segment of Array.isArray(source.barrels)?source.barrels:[]){
  if(!object(segment)||typeof segment.hole_id!=='string'||typeof segment.from_layer!=='string'||typeof segment.to_layer!=='string'||typeof segment.loss_w!=='number'||!Number.isFinite(segment.loss_w)||segment.loss_w<0||!Array.isArray(mesh.barrels))reject('Missing/nonfinite barrel loss evidence');
  const matches=mesh.barrels.filter(b=>object(b)&&b.hole_id===segment.hole_id);if(matches.length!==1||!object(matches[0])||!object(matches[0].source)||matches[0].ready!==true)reject('Unresolved barrel physical source/contact');const barrel=matches[0];const provenance=barrel.source as Record<string,T.Json>;
  if(!['via','pad'].includes(String(provenance.kind))||typeof provenance.path!=='string'||!Array.isArray(barrel.segments)||!barrel.segments.some(s=>object(s)&&s.from_layer===segment.from_layer&&s.to_layer===segment.to_layer))reject('Unsupported/missing barrel segment mapping');
  const id=`dc:${source.id}:barrel:${segment.hole_id}:${segment.from_layer}:${segment.to_layer}`;if(barrelIds.has(id))reject('Duplicate barrel loss segment');barrelIds.add(id);
  barrelHeat.push({id,barrel:`${provenance.kind}:${typeof provenance.source_id==='string'?provenance.source_id:provenance.path}`,fromLayer:segment.from_layer,toLayer:segment.to_layer,powerW:segment.loss_w});barrelPower+=segment.loss_w;
 }
 if(typeof source.barrel_loss_w!=='number'||Math.abs(barrelPower-source.barrel_loss_w)>1e-9*Math.max(1,barrelPower))reject('Missing/trimmed barrel loss power; full source contribution must be transferred');
 return{heat,barrelHeat,conditional:canonicalConditional||map.validity.status==='conditional',...(idleReason?{conditionalReason:idleReason}:{}),provenance:{solutionId:source.id,zeroPowerEntriesSkipped:zeroSkipped,boardIdentity:task.boardIdentity??null,copperPowerW:total,barrelPowerW:barrelPower,totalPowerW:total+barrelPower,triangles:expected.length,...(idleDomains.length?{idleDomains:idleDomains.map(d=>({domainId:(d as {domain_id:string}).domain_id,nets:(d as {nets:T.Json}).nets}))}:{}),barrelSegments:barrelHeat.length,coverage:'All computed triangles and finite barrel segments from authoritative same-context DC solution; no uncomputed domains'}};
}
