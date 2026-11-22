// Independent output checks; never uses triangulator counters as the oracle.
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
const root=resolve('debugging/conduction-mesh');
const read=p=>JSON.parse(readFileSync(p,'utf8'));
const hash=p=>createHash('sha256').update(readFileSync(p,'utf8').replace(/\r\n/g,'\n')).digest('hex');
const semantic=({timings_ms,serialized_bytes,...r})=>r;
const names=['BandAmpV5','ESPower','NanoScope','NanoSpec','STM32G0_CAN_RS485','TPS63060DSCR','ThunderF722','USB_PD_Trigger_5A'];
function inside(p,ring){let yes=false;for(let i=0,j=ring.length-1;i<ring.length;j=i++){const a=ring[i],b=ring[j];if((a[1]>p[1])!==(b[1]>p[1])&&p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0])yes=!yes;}return yes;}
const inPoly=(p,poly)=>inside(p,poly.outer)&&!poly.holes.some(h=>inside(p,h));
function boundaryDistance(p,polygons){let best=Infinity;for(const poly of polygons)for(const ring of [poly.outer,...poly.holes])for(let i=0;i<ring.length;i++){const a=ring[i],b=ring[(i+1)%ring.length],dx=b[0]-a[0],dy=b[1]-a[1],l=dx*dx+dy*dy;if(l){const t=Math.max(0,Math.min(1,((p[0]-a[0])*dx+(p[1]-a[1])*dy)/l));best=Math.min(best,Math.hypot(p[0]-a[0]-t*dx,p[1]-a[1]-t*dy));}}return best;}
const triangleArea=(a,b,c)=>((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]))/2;
function pointInTriangle(p,a,b,c){return triangleArea(a,b,p)>0&&triangleArea(b,c,p)>0&&triangleArea(c,a,p)>0;}
function circleDistance(p,a,b,c){
 if(pointInTriangle(p,a,b,c))return 0;
 const distance=(a,b)=>{const dx=b[0]-a[0],dy=b[1]-a[1],t=Math.max(0,Math.min(1,((p[0]-a[0])*dx+(p[1]-a[1])*dy)/(dx*dx+dy*dy)));return Math.hypot(p[0]-a[0]-t*dx,p[1]-a[1]-t*dy);};
 return Math.min(distance(a,b),distance(b,c),distance(c,a));
}
const results=[];
for(const name of names){
 const fixture=join('tests/pcb-analyze',name),runs=join('debugging/pcb-analyze',name,'runs');
 const run=readdirSync(runs).filter(s=>s.includes('-mesh-')).sort().at(-1);assert.ok(run,name);const dir=join(runs,run);
 assert.equal(read(join(dir,'run.json')).status,'completed');
 const report=read(join(dir,'mesh.json')),summary=read(join(dir,'report.json'));
 assert.deepEqual(semantic(report),semantic(read(join(root,'rust',name+'.json'))),name+': full Rust/addon-worker semantic parity');
 assert.equal(report.status,'partial');assert.equal(report.counts.ready_domains,0);assert.ok(report.counts.built_domains>0);
 const geometry=read(join(root,'geometry-rust',name+'.json'));
 assert.deepEqual(geometry,read(join('debugging/copper-geometry/rust',name+'.json')),name+': previous full geometry report unchanged');
 const input=read(join(fixture,name+'.json')),manifest=read(join(fixture,'fixture.json'));
 // Use the existing recorded inventory; no fixture values are regenerated.
 for(const [file,expected] of Object.entries(manifest.files)){
  const bytes=readFileSync(join(fixture,file));const normalized=expected.normalization==='binary'?bytes:Buffer.from(bytes.toString('utf8').replace(/\r\n/g,'\n'));
  assert.equal(normalized.length,expected.bytes);assert.equal(createHash('sha256').update(normalized).digest('hex'),expected.sha256,`${name}/${file} unchanged`);
 }
 let nodes=0,triangles=0,terminals=0,area=0;const layerSummary=[],rayBoundaryCases=[];
 for(const d of report.domains){
  const copper=geometry.layers.find(l=>l.layer===d.layer).copper;
  const edgeCounts=new Map();let actualArea=0;
  for(const t of d.triangles){assert.ok(t.every(i=>Number.isSafeInteger(i)&&i>=0&&i<d.nodes.length));const p=t.map(i=>d.nodes[i]);assert.ok(p.flat().every(Number.isFinite));const a=triangleArea(...p);assert.ok(a>0);actualArea+=a;
   for(let i=0;i<3;i++){const key=[t[i],t[(i+1)%3]].sort((a,b)=>a-b).join(':');edgeCounts.set(key,(edgeCounts.get(key)??0)+1);}
  }
  assert.ok(Math.abs(actualArea-d.mesh_area_mm2)<1e-6*(1+actualArea));
  if(d.triangles.length){assert.ok(Math.abs(actualArea-d.area_mm2)<1e-6*(1+d.area_mm2));
   for(let i=0;i<d.triangles.length;i+=Math.max(1,Math.floor(d.triangles.length/32))){const p=d.triangles[i].map(n=>d.nodes[n]);const centre=[p.reduce((n,v)=>n+v[0],0)/3,p.reduce((n,v)=>n+v[1],0)/3];if(!copper.some(c=>inPoly(centre,c))){const distance=boundaryDistance(centre,copper);const arithmeticTolerance=(1+Math.max(...centre.map(Math.abs)))*1e-12;assert.ok(distance<=arithmeticTolerance,`${name}/${d.id}: independent ray test outside arithmetic boundary tolerance (${distance} mm)`);rayBoundaryCases.push({domain:d.id,triangle:i,distanceMm:distance,arithmeticToleranceMm:arithmeticTolerance,areaMm2:triangleArea(...p)});}}
  }
  const bounds=new Set(d.boundaries.map(e=>e.nodes.slice().sort((a,b)=>a-b).join(':')));
  for(const [key,count] of edgeCounts){assert.ok(count===1||count===2);if(count===1)assert.ok(bounds.has(key),'Every exposed mesh edge has boundary provenance');}
  for(const t of d.terminals){assert.ok(t.triangles.length>0);assert.ok(t.boundary_edges.length>0);assert.ok(t.triangles.every(i=>i<d.triangles.length));assert.ok(t.boundary_edges.every(i=>d.boundaries[i].source_paths.includes(t.source.path)));}
  if(!d.readiness.topology_confirmed||!d.readiness.copper_complete||!d.readiness.physics_complete||!d.readiness.mesh_complete)assert.equal(d.readiness.dc_ready,false);
  nodes+=d.nodes.length;triangles+=d.triangles.length;terminals+=d.terminals.length;area+=actualArea;
 }
 assert.equal(nodes,report.counts.nodes);assert.equal(triangles,report.counts.triangles);assert.equal(terminals,report.counts.terminals);
 for(const layer of input.layers){const domains=report.domains.filter(d=>d.layer===layer.name);layerSummary.push({layer:layer.name,built:domains.filter(d=>d.triangles.length).length,unbuilt:domains.filter(d=>!d.triangles.length).length,areaMm2:domains.reduce((a,d)=>a+d.mesh_area_mm2,0)});}
 const independent=[];
 if(['BandAmpV5','ESPower'].includes(name)){
  const raw=read(join(fixture,'export-evidence.json')).capture.pcb;
  const pair=name==='BandAmpV5'?['1ef61450f2f63c6ee14','73d6db5f0c9d2233']:['e536e19','581f781a134fbbbf'];
  const pad=input.pads.find(p=>p.id===pair[0]),track=input.copper.fixed.tracks.find(t=>t.id===pair[1]);
  const nativePad=raw.pads.find(p=>p.id===pad.id),nativeTrack=raw.tracks.find(t=>t.id===track.id);
  assert.ok(Math.abs(pad.at.x-nativePad.x)<1e-8&&Math.abs(pad.at.y+nativePad.y)<1e-8);
  assert.ok(Math.abs(track.widthMm-nativeTrack.width)<1e-8);
  const endpoint=track.points.find(p=>{const a=-pad.rotationDeg*Math.PI/180,dx=p.x-pad.at.x,dy=p.y-pad.at.y;return Math.abs(dx*Math.cos(a)-dy*Math.sin(a))<pad.shape.widthMm/2-0.01&&Math.abs(dx*Math.sin(a)+dy*Math.cos(a))<pad.shape.heightMm/2-0.01;});assert.ok(endpoint);
  const padRegions=geometry.regions.filter(r=>r.source.source_id===pad.id).map(r=>r.id),trackRegions=geometry.regions.filter(r=>r.source.source_id===track.id).map(r=>r.id);
  const domain=report.domains.find(d=>d.triangles.length&&padRegions.some(id=>d.region_ids.includes(id))&&trackRegions.some(id=>d.region_ids.includes(id)));
  if(name==='BandAmpV5')assert.ok(domain,'Native narrow track and pad share a conforming mesh domain');
  if(domain){const terminal=domain.terminals.find(t=>t.source.source_id===pad.id);assert.ok(terminal);const point=[endpoint.x,endpoint.y];assert.ok(terminal.triangles.some(i=>{const [a,b,c]=domain.triangles[i].map(j=>domain.nodes[j]);return triangleArea(a,b,point)>=-1e-9&&triangleArea(b,c,point)>=-1e-9&&triangleArea(c,a,point)>=-1e-9;}),'Native endpoint belongs to actual pad terminal elements');}
  const via=raw.vias[0],at=[via.x,-via.y],radius=via.drill/2;
  const contacts=report.barrels.find(b=>b.source.source_id===via.id);
  assert.ok(contacts&&contacts.contacts.length>=2,`${name}: native via boundary has contacts on distinct layers`);
  const contactDomains=contacts.contacts.map(c=>report.domains.find(d=>d.id===c.domain));
  for(const d of contactDomains){for(const t of d.triangles){const p=t.map(i=>d.nodes[i]);assert.ok(circleDistance(at,...p)>=radius-0.0001,'Full native via bore, including triangle edges, is outside the mesh');}
   for(const e of contacts.contacts.find(c=>c.domain===d.id).boundary_edges){for(const i of d.boundaries[e].nodes)assert.ok(Math.abs(Math.hypot(d.nodes[i][0]-at[0],d.nodes[i][1]-at[1])-radius)<0.003);}
  }
  const fill=raw.polygons.find(p=>p.sources.some(s=>s.length===10&&s[2]==='ARC'&&s[6]==='ARC'&&s[3]===180&&s[7]===180));
  const circle=fill.sources.find(s=>s.length===10&&s[2]==='ARC'&&s[6]==='ARC'&&s[3]===180&&s[7]===180);
  const centre=[(circle[0]+circle[4])/2,-(circle[1]+circle[5])/2];
  const fillDomains=report.domains.filter(d=>d.triangles.length&&geometry.regions.some(r=>d.region_ids.includes(r.id)&&(r.source.native_fill_ids.includes(fill.id)||r.source.source_id===fill.pourId)));
  if(name==='BandAmpV5')assert.ok(fillDomains.length>0);
  for(const d of fillDomains)for(const t of d.triangles)assert.equal(pointInTriangle(centre,...t.map(i=>d.nodes[i])),false,'Native fill void absent from mesh');
  independent.push({padId:pad.id,trackId:track.id,trackWidthMm:track.widthMm,nativeTrackEndpoint:endpoint,padTrackMeshDomain:domain?.id??null,viaId:via.id,viaHoleYDown:at,viaDiameter:via.diameter,viaDrill:via.drill,contactDomains:contactDomains.map(d=>d.id),fillId:fill.id,nativeFillCircle:circle,fillVoidYDown:centre,fillVoidMeshed:fillDomains.length>0,limitation:'Unbuilt domains do not certify void preservation in a mesh. Existing native source vs capture dimensions/provenance differences remain; see copper geometry verification.'});
 }
 results.push({name,run:dir,status:report.status,counts:report.counts,elapsedMs:summary.elapsedMs,reportBytes:summary.reportBytes,nativeTimingsMs:report.timings_ms,serializedRustBytes:report.serialized_bytes,fixtureJsonSha256:hash(join(fixture,name+'.json')),layers:layerSummary,diagnostics:Object.fromEntries([...new Set(report.diagnostics.map(d=>d.code))].sort().map(code=>[code,report.diagnostics.filter(d=>d.code===code).length])),unbuilt:report.diagnostics.filter(d=>d.code==='mesh_unbuilt'),rayBoundaryCases,independent});
}
const binary=createRequire(import.meta.url)('../native/node-addon/platform.cjs').nativeFilename();
const nativeSha256=createHash('sha256').update(readFileSync(join('native/node-addon',binary))).digest('hex');
writeFileSync(join(root,'evidence.json'),JSON.stringify({platform:process.platform,arch:process.arch,node:process.version,nativeSha256,fullRustNodeSemanticParity:8,previousGeometryReportsEqual:8,results},null,2)+'\n');
console.log(JSON.stringify({fullRustNodeSemanticParity:8,previousGeometryReportsEqual:8,results:results.map(({name,counts,elapsedMs,reportBytes})=>({name,counts,elapsedMs,reportBytes}))},null,2));
