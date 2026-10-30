import type {AnalysisGeometryReport} from '../../native/analyze/index.js';
import type {ChannelBlock,ChannelCut,CopperInterval,SegmentedChannel} from './segmented-channel.ts';
import {createProblemExecutor} from './execute-problem.ts';
const fail=(s:string):never=>{throw new Error('CHANNEL_SEGMENTATION: '+s);};
/** Slab decomposition retains the whole confirmed signal island, including
 * branches. All nonuniform/coupled copper in each local slab is solved jointly.
 * Numerical sections use captured finite copper, never a closest template. */
export function boardChannelPlan(board:any,base:any,contacts:{sourcePath:string;layer:string}[],options:{domainScale?:number;magneticPatchLevel?:1|2}={}):SegmentedChannel & {audit:unknown}{
 const execute=createProblemExecutor(true),boardJson=JSON.stringify(board);
 const g:AnalysisGeometryReport=execute({kind:'prepare-geometry',boardJson});
 const island=(c:{sourcePath:string;layer:string})=>{const region=g.regions.filter(r=>r.source.path===c.sourcePath&&r.layer===c.layer);if(region.length!==1)fail('Ambiguous physical endpoint');return g.islands.find(i=>i.region_ids.includes(region[0].id))??fail('Unconfirmed endpoint');};
 const signal=island(contacts[0]);if(signal.id!==island(contacts[1]).id)fail('Disconnected signal endpoints');
 const signalRegions=g.regions.filter(r=>signal.region_ids.includes(r.id));
 const range=(rs:typeof signalRegions,k:number)=>[Math.min(...rs.flatMap(r=>r.polygon.outer.map(p=>p[k]))),Math.max(...rs.flatMap(r=>r.polygon.outer.map(p=>p[k])))];
 const xr=range(signalRegions,0),yr=range(signalRegions,1),axis=xr[1]-xr[0]>=yr[1]-yr[0]?0:1,trans=1-axis;
 const portRegions=g.regions.filter(r=>contacts.some(c=>c.sourcePath===r.source.path&&c.layer===r.layer));
 const physicalRange=[...signalRegions,...portRegions],innerSpan=range(physicalRange,axis),cross=range(physicalRange,trans),height=base.stack.reduce((s:number,l:any)=>s+l.thicknessM*1000,0),margin=Math.max(.05,2*height),domainMargin=margin*(options.domainScale??1);
 const span=[innerSpan[0]-domainMargin,innerSpan[1]+domainMargin],transverse=[cross[0]-domainMargin,cross[1]+domainMargin];
 // Numerical truncation may cut an ordinary track, but never a plated wall.
 // Close over every intersecting physical barrel, including connector pads;
 // expansion can encounter another barrel, so iterate to a finite fixed point.
 const wall=base.finishedHolePlatingM*1000+(base.curveDeviationMm??.001)+2*g.boolean_error_bound_mm;
 for(let pass=0;pass<=g.holes.length;pass++){let changed=false;
  for(const h of g.holes.filter(h=>h.plated&&h.span_confirmed)){const points=h.polygons.flatMap(p=>p.outer);if(!points.length)continue;
   const a=[Math.min(...points.map(p=>p[axis]))-wall,Math.max(...points.map(p=>p[axis]))+wall],b=[Math.min(...points.map(p=>p[trans]))-wall,Math.max(...points.map(p=>p[trans]))+wall];
   if(a[0]>span[1]||a[1]<span[0]||b[0]>transverse[1]||b[1]<transverse[0])continue;
   for(const [bound,actual]of [[span,a],[transverse,b]]){if(actual[0]<bound[0]){bound[0]=actual[0];changed=true;}if(actual[1]>bound[1]){bound[1]=actual[1];changed=true;}}
  }
  if(!changed)break;if(pass===g.holes.length)fail('Physical barrel-domain closure did not stabilize');
 }
 const centre=(transverse[0]+transverse[1])/2;
 const relevant=g.regions.filter(r=>{const a=range([r],axis),b=range([r],trans);return a[0]<=span[1]&&a[1]>=span[0]&&b[0]<=transverse[1]&&b[1]>=transverse[0];});
 const rough:[number,number][]=[];
 const local=(a:number,b:number)=>rough.push([Math.max(span[0],a-margin),Math.min(span[1],b+margin)]);
 for(const r of relevant){
  for(const ring of [r.polygon.outer,...r.polygon.holes])for(let i=0;i<ring.length;i++){
   const a=ring[i],b=ring[(i+1)%ring.length];
   if(a[axis]===b[axis])local(a[axis],a[axis]);
   else if(a[trans]!==b[trans])local(Math.min(a[axis],b[axis]),Math.max(a[axis],b[axis]));
  }
 }
 // Nonuniform dielectric outline/bores require joint local fields as well.
 const materialRings=[...(g.outline?[g.outline.outer,...g.outline.holes]:[]),...g.holes.flatMap(h=>h.polygons.flatMap(p=>[p.outer,...p.holes]))];
 for(const ring of materialRings)for(let i=0;i<ring.length;i++){const a=ring[i],b=ring[(i+1)%ring.length];if(Math.min(a[trans],b[trans])>transverse[1]||Math.max(a[trans],b[trans])<transverse[0])continue;if(a[axis]===b[axis])local(a[axis],a[axis]);else if(a[trans]!==b[trans])local(Math.min(a[axis],b[axis]),Math.max(a[axis],b[axis]));}
 for(let p=0;p<2;p++){const pair=[contacts[p],contacts[p+2]],rs=relevant.filter(r=>pair.some(c=>r.source.path===c.sourcePath&&r.layer===c.layer)),a=range(rs,axis);local(a[0],a[1]);}
 // Environmental copper beyond the actual ports belongs to the end local
 // regions, never an invented extra length of transmission line.
 rough.push([span[0],Math.min(span[1],innerSpan[0]+margin)],[Math.max(span[0],innerSpan[1]-margin),span[1]]);
 rough.sort((a,b)=>a[0]-b[0]);const merged:[number,number][]=[];
 for(const a of rough){if(a[1]<=a[0])continue;const last=merged.at(-1);if(last&&a[0]<=last[1])last[1]=Math.max(last[1],a[1]);else merged.push([...a]);}
 const slabs:{kind:'local'|'section';a:number;b:number}[]=[];let cursor=span[0];for(const [a,b]of merged){if(a>cursor)slabs.push({kind:'section',a:cursor,b:a});slabs.push({kind:'local',a,b});cursor=b;}if(cursor<span[1])slabs.push({kind:'section',a:cursor,b:span[1]});
 if(slabs.length>32)fail('More than32 joint slabs; no branches discarded');
 const point=(x:number)=>axis===0?[x,centre]:[centre,x];
 const returnIslands=[...new Map([island(contacts[2]),island(contacts[3])].map(i=>[i.id,i])).values()];
 const returnRegionIds=new Set(returnIslands.flatMap(i=>i.region_ids));const returnRegions=g.regions.filter(r=>returnRegionIds.has(r.id));
 const inRing=(ring:number[][],p:number[])=>{let yes=false;for(let i=0,j=ring.length-1;i<ring.length;j=i++){const a=ring[i],b=ring[j];if((a[1]>p[1])!==(b[1]>p[1])&&p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0])yes=!yes;}return yes;};
 const inside=(r:typeof relevant[number],p:number[])=>inRing(r.polygon.outer,p)&&!r.polygon.holes.some(h=>inRing(h,p));
 const slices=new Map<number,any>();
 function slice(at:number){if(slices.has(at))return slices.get(at);
  const witness=(r:typeof relevant[number])=>{const cuts:number[]=[transverse[0],transverse[1]];for(const ring of [r.polygon.outer,...r.polygon.holes])for(let i=0;i<ring.length;i++){const a=ring[i],b=ring[(i+1)%ring.length];if((a[axis]<=at&&b[axis]>at)||(b[axis]<=at&&a[axis]>at)){const t=(at-a[axis])/(b[axis]-a[axis]);const y=a[trans]+t*(b[trans]-a[trans]);if(y>transverse[0]&&y<transverse[1])cuts.push(y);}}cuts.sort((a,b)=>a-b);for(let i=1;i<cuts.length;i++){const y=(cuts[i-1]+cuts[i])/2,p=point(at);p[trans]=y;if(inside(r,p))return y;}return undefined;};
  let ref=returnRegions[0],referenceY:number|undefined;for(const r of returnRegions){const y=witness(r);if(y!==undefined){ref=r;referenceY=y;break;}}const referenceOffset=(referenceY??fail('No physical continuous reference interval at cut '+at))-centre;
  const transverseUnit=axis===0?[0,1]:[1,0];
  const value=execute({kind:'board-section',boardJson,stack:base.stack,slice:{atMm:point(at),transverseUnit,halfWidthMm:(transverse[1]-transverse[0])/2,airMarginMm:Math.max(1,4*height)*(options.domainScale??1),referenceLayer:ref.layer,referenceSourcePath:ref.source.path,referenceXMm:referenceOffset}});
  const mapping=value.sourceMapping,reference=mapping.find((m:any)=>m.role==='reference');
  const key=(m:any)=>JSON.stringify({axis,at,layer:m.layer,interval:m.intervalMm});
  const cuts:ChannelCut[]=mapping.filter((m:any)=>m.role!=='reference').map((m:any)=>({positive:key(m),negative:key(reference)}));
  const result={...value,cuts,signalMapping:mapping.filter((m:any)=>m.role!=='reference'),reference};slices.set(at,result);return result;
 }
 const sources=new Map<string,{a:number;b:number}>();for(const r of relevant){const key=r.source.path+':'+r.layer,a=range([r],axis),old=sources.get(key);sources.set(key,{a:Math.max(span[0],Math.min(old?.a??Infinity,a[0])),b:Math.min(span[1],Math.max(old?.b??-Infinity,a[1]))});}
 for(const hole of g.holes.filter(h=>h.plated&&h.span_confirmed)){const points=hole.polygons.flatMap(p=>p.outer);if(!points.length)continue;const low=Math.min(...points.map(p=>p[trans])),high=Math.max(...points.map(p=>p[trans]));if(low>transverse[1]||high<transverse[0])continue;const a=Math.max(span[0],Math.min(...points.map(p=>p[axis]))),b=Math.min(span[1],Math.max(...points.map(p=>p[axis])));if(a<b)sources.set(hole.source.path+':barrel',{a,b});}
 const ownership=(a:number,b:number):CopperInterval[]=>[...sources].flatMap(([sourcePath,s])=>Math.max(a,s.a)<Math.min(b,s.b)?[{sourcePath,startMm:Math.max(a,s.a),stopMm:Math.min(b,s.b)}]:[]);
 const blocks:ChannelBlock[]=[],ports:SegmentedChannel['ports']=[];
 for(const [index,slab]of slabs.entries()){
  const id='slab-'+index,own=ownership(slab.a,slab.b);
  if(slab.kind==='section'){
   const mid=slice((slab.a+slab.b)/2),left=slice(slab.a),right=slice(slab.b);
   if(JSON.stringify(left.geometry)!==JSON.stringify(right.geometry)||JSON.stringify(left.geometry)!==JSON.stringify(mid.geometry))fail('Nonconstant physical section escaped change detection');
   const metal=[...mid.geometry.conductors.filter((m:any)=>m.role!=='reference'),mid.geometry.conductors.find((m:any)=>m.role==='reference')];
   const maps=[...mid.signalMapping,mid.reference];const resistance:number[]=[],internal:number[]=[];
   for(let i=0;i<metal.length;i++){const m=metal[i],p=m.contour.outerM??m.contour.outer_m??m.contour.outer;const xs=p.map((v:number[])=>v[0]),zs=p.map((v:number[])=>v[1]);const w=Math.max(...xs)-Math.min(...xs),t=Math.max(...zs)-Math.min(...zs),sigma=base.stack.find((l:any)=>l.name===maps[i].layer).conductivitySM;
    if(t/w>.25)fail('Low-band ribbon internal inductance requires thin conductor');
    if(Math.sqrt(1/(Math.PI*1e4*4e-7*Math.PI*sigma))<10*t)fail('Low-band volume skin overlap not admitted');
    resistance.push(1/(sigma*w*t));internal.push(4e-7*Math.PI*t/(12*w));
   }
   const n=resistance.length-1,dcConductors={resistanceOhmPerM:Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>resistance[n]+(i===j?resistance[i]:0))),internalInductanceHPerM:Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>internal[n]+(i===j?internal[i]:0)))};
   blocks.push({id,kind:'section',line:{kind:'polygon-section',lengthMm:slab.b-slab.a,geometry:mid.geometry},dcConductors,bandHz:[1000,10000],cuts:[...left.cuts,...right.cuts],ownership:own});
  }else{
   const roi=axis===0?[[slab.a,transverse[0]],[slab.b,transverse[1]]]:[[transverse[0],slab.a],[transverse[1],slab.b]];
   const electrodes:any[]=[],cutPlanes:Record<string,unknown>={},pairs:string[][]=[],cuts:ChannelCut[]=[],electrodeNames=new Map<string,string>();
   function electrode(key:string,sourcePath:string,layer:string,at?:number,spanMm?:number[]){let name=electrodeNames.get(key);if(name)return name;name='terminal-'+electrodes.length;electrodeNames.set(key,name);electrodes.push({name,sourcePath,layer,upper:layer===base.stack.filter((l:any)=>l.copper).at(-1).name});if(at!==undefined)cutPlanes[name]={axis,atMm:at,spanMm};return name;}
   for(const at of [slab.a,slab.b])if(at>span[0]&&at<span[1]){const s=slice(at);for(let k=0;k<s.cuts.length;k++){const c=s.cuts[k],m=s.signalMapping[k],ref=s.reference;pairs.push([electrode(c.positive,m.sourcePaths[0],m.layer,at,m.intervalMm.map((v:number)=>v+centre)),electrode(c.negative,ref.sourcePaths[0],ref.layer,at,ref.intervalMm.map((v:number)=>v+centre))]);cuts.push(c);}}
   for(let p=0;p<2;p++){const positive=contacts[p],negative=contacts[p+2],rs=relevant.filter(r=>r.source.path===positive.sourcePath&&r.layer===positive.layer),r=range(rs,axis);if(r[0]>=slab.a&&r[1]<=slab.b){const c={positive:'external-'+p+'-signal',negative:'external-'+p+'-return'};pairs.push([electrode(c.positive,positive.sourcePath,positive.layer),electrode(c.negative,negative.sourcePath,negative.layer)]);cuts.push(c);ports.push({name:p===0?'input':'output',cut:c});}}
   if(pairs.length===0)fail('Unconnected local slab');
   blocks.push({id,kind:'local',boardJson,geometry:{...base,roiMm:roi,electrodes,ports:pairs,cutPlanes},magneticPatchLevel:options.magneticPatchLevel??1,bandHz:[1000,10000],cuts,ownership:own});
  }
 }
 if(ports.length!==2)fail('Physical endpoints must belong to complete local source regions');
 ports.sort((a,b)=>a.name==='input'?-1:b.name==='input'?1:0);
 return {blocks,ports,coverage:ownership(span[0],span[1]),audit:{axis,slabs,ownershipConvention:'Original source polygon clipped by longitudinal coordinate interval, with layer/barrel ownership; not an arclength approximation',copperDecisions:g.regions.map(r=>({regionId:r.id,sourcePath:r.source.path,layer:r.layer,islandId:g.islands.find(i=>i.region_ids.includes(r.id))?.id,role:signal.region_ids.includes(r.id)?'signal':returnRegionIds.has(r.id)?'return':'neighbour',included:relevant.includes(r),reason:relevant.includes(r)?'Captured physical copper intersects the full signal/port environmental domain; disjoint slab ownership retains its branches and joint in-slab coupling':'Outside current numerical domain; snapshot retained and expansion checked'})),selectedRegionIds:relevant.map(r=>r.id),excludedRegionIds:g.regions.filter(r=>!relevant.includes(r)).map(r=>({id:r.id,reason:'Outside current numerical domain; source snapshot preserved and domain expansion required'})),geometryDiagnostics:g.diagnostics,geometryFindings:g.findings,transverseMm:transverse,signalIsland:signal.id,returnIslands:returnIslands.map(i=>i.id),policy:'whole confirmed signal island; all branches and neighbouring slab copper retained; no shortest path',lossApproximation:'Uniform-current DC ribbon R/internal L in sections; no verified proximity/skin-transition or complete5% certificate',accuracy:'conditional; joint-local/section interaction and domain refinement remain required'}};
}
