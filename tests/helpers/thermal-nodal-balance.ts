import assert from 'node:assert/strict';
import {sampleWaveform} from '../../src/analyze/analysis-dsl/descriptions.ts';
/** Independent original steady nodal conservation from saved signed path flows.
 * Uses manifests/contact weights, never correction-CG quality or global balance. */
export function auditThermalSteadyNodalBalance(s:any){
 const network=s.request?.network;assert.ok(network&&Array.isArray(network.nodes)&&Array.isArray(network.links)&&Array.isArray(network.sources),'Authored compact network required for independent nodal audit');
 const environment=new Set<string>(['pcb:ambient',...network.nodes.filter((n:any)=>n.boundary==='environment').map((n:any)=>n.id)]);
 const residual=new Map<string,number>(s.system.nodeIds.filter((id:string)=>!environment.has(id)).map((id:string)=>[id,0]));const powers=new Map<string,number>();
 const add=(id:string,q:number)=>{assert.ok(Number.isFinite(q));if(!environment.has(id)){assert.ok(residual.has(id),`Unmapped physical node ${id}`);residual.set(id,residual.get(id)!+q);}};
 const source=(id:string,q:number)=>{add(id,-q);powers.set(id,(powers.get(id)??0)+q);};const at=s.axis[0];
 for(const q of network.sources)source(q.node,sampleWaveform(q.powerW,at)*(q.weight??1));
 for(const q of s.spatial.sources??[])for(const w of q.weights)source(w.node,sampleWaveform(q.powerW,at)*w.weight);
 const planar=s.spatial.mesh.pointsMm.length,layers=s.spatial.layers.map((l:any)=>l.name),node=(i:number)=>{assert.ok(Number.isInteger(i)&&i>=0&&i<planar*layers.length);return`pcb:${layers[Math.floor(i/planar)]}:${i%planar}`;};
 for(const q of s.spatial.barrelSources??[])for(const side of [q.fromWeights,q.toWeights])for(const[i,w]of side)source(node(i),q.powerW*w/2);
 const compact=new Map<string,any>(network.links.map((l:any)=>[l.id,l])),contacts=new Map<string,any>(s.spatial.contacts.map((c:any)=>[c.id,c])),barrels=new Map<string,any>(),ordinal=new Map<string,number>();for(const b of s.spatial.barrels??[]){const i=ordinal.get(b.id)??0;ordinal.set(b.id,i+1);barrels.set(`barrel:${b.id}:${i}`,b);}
 for(const id of s.system.linkIds){const q=s.heatFlowW[id][0];if(compact.has(id)){const l=compact.get(id);add(l.from,q);add(l.to,-q);}else if(id.startsWith('fem:')){const[a,b]=id.slice(4).split(':').map(Number);add(node(a),q);add(node(b),-q);}else if(id.startsWith('surface:')){const[a,b]=id.slice(8).split(':').map(Number);add(node(a*planar+b),q);}else if(id.startsWith('contact:')){const split=id.lastIndexOf(':'),c=contacts.get(id.slice(8,split));assert.ok(c,`Unknown contact ${id}`);add(c.node,q);add(node(Number(id.slice(split+1))),-q);}else if(barrels.has(id)){const b=barrels.get(id);for(const[i,w]of b.fromWeights)add(node(i),q*w);for(const[i,w]of b.toWeights)add(node(i),-q*w);}else assert.fail(`Unmapped thermal path ${id}`);}
 const values=[...residual.values()],powerValues=[...powers.values()];const norm=Math.sqrt(values.reduce((v,r)=>v+r*r,0)),normalization=1+Math.sqrt(powerValues.reduce((v,p)=>v+p*p,0));const normalized=norm/normalization;assert.ok(Number.isFinite(normalized)&&normalized<=1e-8,`Original nodal physical residual ${normalized} exceeds 1e-8`);
 return{normalizedOriginalNodalResidual:normalized,residualL2W:norm,normalizationW:normalization,worst:[...residual.entries()].sort((a,b)=>Math.abs(b[1])-Math.abs(a[1])).slice(0,8),scope:'Independent steady nodal balance from complete signed flow/source arrays and exact source/contact/barrel mappings'};
}
