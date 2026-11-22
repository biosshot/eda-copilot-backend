/** Selection rule and partition for the product channel cascade (P7-completion 4.4; on by default, P7_CHANNEL_CASCADE=0 disables).
 * Pure geometry, no solver. A channel is cascaded only when its signal copper is one chain
 * pad -> straight tracks -> pad on a single layer: no layer change, branch or extra component pad.
 * Anything else stays on the one-domain path (the caller keeps today's behaviour). */
export type Pt=[number,number];
export interface CascadeContact {sourcePath:string;layer:string}
export interface CascadeSection {
 id:string;kind:'pad'|'track';sourcePath:string;layer:string;
 /** Section ends along the signal axis, board millimetres. */
 startMm:Pt;endMm:Pt;lengthMm:number;widthMm:number;directionUnit:Pt;
}
export interface CascadeJoint {
 afterSection:string;beforeSection:string;atMm:Pt;kind:'pad-to-track'|'width-step'|'bend'|'straight';
 bendDeg:number;widthAMm:number;widthBMm:number;
}
export type ChannelModelChoice=
 |{mode:'cascade';layer:string;sections:CascadeSection[];joints:CascadeJoint[];portPlaneMm:[Pt,Pt];notModelled:string[]}
 |{mode:'one-domain';reasons:string[]};
const TOL_MM=0.01;
const ENDPOINT_TOL_MM=1e-3;
const dist=(a:Pt,b:Pt)=>Math.hypot(a[0]-b[0],a[1]-b[1]);
const dot=(a:Pt,b:Pt)=>a[0]*b[0]+a[1]*b[1];
const unit=(a:Pt,b:Pt):Pt=>{const l=dist(a,b);return [(b[0]-a[0])/l,(b[1]-a[1])/l];};
const indexOf=(path:string,prefix:string)=>{const m=path.match(new RegExp('^'+prefix.replace(/[.[\]]/g,'\\$&')+'\\[(\\d+)\\]$'));return m?Number(m[1]):undefined;};
interface Rect {centre:Pt;axes:[Pt,Pt];half:[number,number]}
const padRect=(pad:any):Rect|undefined=>{
 if(pad?.shape?.kind!=='rect'||!pad.at)return undefined;
 const a=(pad.rotationDeg??0)*Math.PI/180,u:Pt=[Math.cos(a),Math.sin(a)],v:Pt=[-Math.sin(a),Math.cos(a)];
 return {centre:[pad.at.x,pad.at.y],axes:[u,v],half:[pad.shape.widthMm/2,pad.shape.heightMm/2]};
};
const insidePad=(r:Rect,p:Pt,tol=TOL_MM)=>[0,1].every(k=>Math.abs(dot([p[0]-r.centre[0],p[1]-r.centre[1]],r.axes[k]))<=r.half[k]+tol);
/** Distance from the pad centre along `d` to the pad boundary, with the pad width across `d`. d must follow a pad axis. */
const padExit=(r:Rect,d:Pt)=>{
 const k=[0,1].find(i=>Math.abs(dot(d,r.axes[i]))>Math.cos(Math.PI/180));
 if(k===undefined)return undefined;
 return {lengthMm:r.half[k]/Math.abs(dot(d,r.axes[k])),widthMm:2*r.half[1-k]};
};

/** `signal` is the set of source paths on the signal island (contacts included). */
export function chooseChannelModel(board:any,contacts:CascadeContact[],signal:CascadeContact[]):ChannelModelChoice{
 const refuse=(...reasons:string[]):ChannelModelChoice=>({mode:'one-domain',reasons});
 if(contacts.length>4)return refuse('coupled multi-port set ('+contacts.length/2+' ports): the cascade handles one two-port channel, a coupled pair needs coupled sections');
 if(contacts.length!==4)return refuse('cascade needs one two-port channel with explicit return contacts');
 const [a,b]=contacts;
 if(a.layer!==b.layer)return refuse('signal contacts are on different layers: a layer transition needs a block');
 const layer=a.layer;
 const padA=board.pads?.[indexOf(a.sourcePath,'pads')??-1],padB=board.pads?.[indexOf(b.sourcePath,'pads')??-1];
 const rectA=padA&&padRect(padA),rectB=padB&&padRect(padB);
 if(!rectA||!rectB)return refuse('signal contacts must be rectangular pads');
 const tracks:{path:string;width:number;points:Pt[]}[]=[];
 const seen=new Set<string>();
 for(const s of signal){
  const key=s.sourcePath+'|'+s.layer;if(seen.has(key))continue;seen.add(key);
  if(s.sourcePath===a.sourcePath||s.sourcePath===b.sourcePath){
   if(s.layer!==layer)return refuse('signal pad also present on another layer: a layer transition needs a block');continue;
  }
  const t=indexOf(s.sourcePath,'copper.fixed.tracks');
  if(t===undefined)return refuse('signal copper '+s.sourcePath+' is not a track: via, extra pad or zone needs a block');
  if(s.layer!==layer)return refuse('signal track on another layer: a layer transition needs a block');
  const track=board.copper?.fixed?.tracks?.[t];
  if(!track||!Array.isArray(track.points)||track.points.length<2||!(track.widthMm>0))return refuse('malformed track '+s.sourcePath);
  tracks.push({path:s.sourcePath,width:track.widthMm,points:track.points.map((p:any)=>[p.x,p.y] as Pt)});
 }
 if(!tracks.length)return refuse('no signal track between the pads');
 const segments=tracks.flatMap(t=>t.points.slice(1).map((p,i)=>({path:t.path,width:t.width,a:t.points[i],b:p,used:false})));
 // Walk from pad A: pick the segment with an endpoint on the pad, then continue at coincident endpoints.
 let current:{path:string;width:number;a:Pt;b:Pt;used:boolean}|undefined;
 let at:Pt=rectA.centre;
 const startEnd=(seg:typeof segments[number]):Pt|undefined=>insidePad(rectA,seg.a)?seg.a:insidePad(rectA,seg.b)?seg.b:undefined;
 const starts=segments.filter(s=>startEnd(s));
 if(starts.length!==1)return refuse(starts.length?'signal copper branches at the input pad':'no track leaves the input pad');
 const chain:{path:string;width:number;from:Pt;to:Pt}[]=[];
 current=starts[0];{const s=startEnd(current)!;chain.push({path:current.path,width:current.width,from:s,to:s===current.a?current.b:current.a});current.used=true;}
 at=chain[0].to;
 while(!insidePad(rectB,at)){
  const next=segments.filter(s=>!s.used&&(dist(s.a,at)<ENDPOINT_TOL_MM||dist(s.b,at)<ENDPOINT_TOL_MM));
  if(next.length!==1)return refuse(next.length?'signal copper branches or is T-shaped near '+at.map(v=>v.toFixed(3)).join(','):'signal track chain is broken before the output pad');
  const s=next[0];s.used=true;const forward=dist(s.a,at)<ENDPOINT_TOL_MM;
  chain.push({path:s.path,width:s.width,from:forward?s.a:s.b,to:forward?s.b:s.a});at=chain.at(-1)!.to;
 }
 if(segments.some(s=>!s.used))return refuse('signal tracks beyond the input-output chain (stub or branch)');
 // Pads become a section (full pad width) only where the track meets them along a pad axis; otherwise the port plane is the track end inside the pad.
 const sections:CascadeSection[]=[];
 const last=chain.length-1,d0=unit(chain[0].from,chain[0].to),dN=unit(chain[last].from,chain[last].to);
 const exitA=padExit(rectA,d0),exitB=padExit(rectB,dN);
 const startA:Pt=exitA?[rectA.centre[0]+d0[0]*exitA.lengthMm,rectA.centre[1]+d0[1]*exitA.lengthMm]:chain[0].from;
 const endB:Pt=exitB?[rectB.centre[0]-dN[0]*exitB.lengthMm,rectB.centre[1]-dN[1]*exitB.lengthMm]:chain[last].to;
 const inputPort:Pt=exitA?rectA.centre:chain[0].from;
 if(exitA)sections.push({id:'pad-input',kind:'pad',sourcePath:a.sourcePath,layer,startMm:rectA.centre,endMm:startA,lengthMm:exitA.lengthMm,widthMm:exitA.widthMm,directionUnit:d0});
 for(const [i,seg] of chain.entries()){
  const d=unit(seg.from,seg.to),from=i===0?startA:seg.from,to=i===last?endB:seg.to;
  const length=dot([to[0]-from[0],to[1]-from[1]],d);
  if(length>TOL_MM)sections.push({id:'track-'+i,kind:'track',sourcePath:seg.path,layer,startMm:from,endMm:to,lengthMm:length,widthMm:seg.width,directionUnit:d});
  else if(chain.length===1)return refuse('the pads cover the whole track');
 }
 if(exitB)sections.push({id:'pad-output',kind:'pad',sourcePath:b.sourcePath,layer,startMm:endB,endMm:chain[last].to,lengthMm:dot([chain[last].to[0]-endB[0],chain[last].to[1]-endB[1]],dN),widthMm:exitB.widthMm,directionUnit:dN});
 if(sections.length<2||sections.some(x=>!(x.lengthMm>TOL_MM)))return refuse('degenerate section chain (a pad reaches past the track end)');
 const joints:CascadeJoint[]=sections.slice(1).map((s,i)=>{
  const p=sections[i],cos=Math.max(-1,Math.min(1,dot(p.directionUnit,s.directionUnit))),bendDeg=Math.acos(cos)*180/Math.PI;
  const kind=p.kind==='pad'||s.kind==='pad'?'pad-to-track':bendDeg>1?'bend':Math.abs(p.widthMm-s.widthMm)>TOL_MM?'width-step':'straight';
  return {afterSection:p.id,beforeSection:s.id,atMm:s.startMm,kind,bendDeg,widthAMm:p.widthMm,widthBMm:s.widthMm};
 });
 return {mode:'cascade',layer,sections,joints,portPlaneMm:[inputPort,chain[last].to],
  notModelled:['pad parts not aligned with the track (the port plane is the track end inside the pad, no pad shunt)','width steps and bends (ideal joint between sections, error estimated per joint)']};
}

export interface ChannelBlockSite {kind:'layer-transition'|'component-pad'|'branch'|'open-end';atMm:Pt;layers:string[];sourcePaths:string[]}
export interface ChannelStructure {
 sections:{count:number;totalLengthMm:number;byLayerMm:Record<string,number>};
 blocks:ChannelBlockSite[];/** Copper pour polygons on the signal island (not sections: a pour is not a track). */pours:number;summary:string;
}
/** Where a cascade would need a 3D block (P7-completion 4.5): layer transitions, component pads on the copper, branches and open ends;
 * the straight track runs between them are the sections. Pure geometry; used to name what the one-domain fallback is missing. */
export function describeChannelStructure(board:any,contacts:CascadeContact[],signal:CascadeContact[]):ChannelStructure{
 const TOL=0.01,items:{key:string;at:Pt;radius:number;kind:'pad'|'via'|'track-end';layers:string[];path:string;contact:boolean;seg?:number}[]=[];
 const seen=new Set<string>(),done=new Set<string>(),contactPaths=new Set(contacts.slice(0,contacts.length/2).map(c=>c.sourcePath));
 const segs:{path:string;layer:string;a:Pt;b:Pt}[]=[];
 for(const s of signal){
  if(seen.has(s.sourcePath+'|'+s.layer))continue;seen.add(s.sourcePath+'|'+s.layer);
  const pi=indexOf(s.sourcePath,'pads'),vi=indexOf(s.sourcePath,'copper.fixed.vias'),ti=indexOf(s.sourcePath,'copper.fixed.tracks');
  if(pi!==undefined){
   if(done.has(s.sourcePath))continue;done.add(s.sourcePath);
   const p=board.pads?.[pi];if(!p?.at)continue;
   const sh=p.shape??{},r=sh.diameterMm?sh.diameterMm/2:Math.max(sh.widthMm??0,sh.heightMm??0)/2;
   items.push({key:s.sourcePath,at:[p.at.x,p.at.y],radius:r,kind:'pad',layers:p.layers??[s.layer],path:s.sourcePath,contact:contactPaths.has(s.sourcePath)});
  }else if(vi!==undefined){
   if(done.has(s.sourcePath))continue;done.add(s.sourcePath);
   const v=board.copper?.fixed?.vias?.[vi];if(!v?.at)continue;
   items.push({key:s.sourcePath,at:[v.at.x,v.at.y],radius:(v.diameterMm??0)/2,kind:'via',layers:[v.fromLayer,v.toLayer],path:s.sourcePath,contact:false});
  }else if(ti!==undefined){
   const t=board.copper?.fixed?.tracks?.[ti];if(!t?.points)continue;
   for(let i=1;i<t.points.length;i++)segs.push({path:s.sourcePath,layer:s.layer,a:[t.points[i-1].x,t.points[i-1].y],b:[t.points[i].x,t.points[i].y]});
  }
 }
 segs.forEach((g,i)=>{items.push({key:'s'+i+'a',at:g.a,radius:0,kind:'track-end',layers:[g.layer],path:g.path,contact:false,seg:i},{key:'s'+i+'b',at:g.b,radius:0,kind:'track-end',layers:[g.layer],path:g.path,contact:false,seg:i});});
 const parent=items.map((_,i)=>i),find=(i:number):number=>parent[i]===i?i:(parent[i]=find(parent[i]));
 for(let i=0;i<items.length;i++)for(let j=i+1;j<items.length;j++){
  const a=items[i],b=items[j];if(a.kind==='track-end'&&b.kind==='track-end'?dist(a.at,b.at)<ENDPOINT_TOL_MM*2:(a.kind!=='track-end'&&b.kind!=='track-end')?false:dist(a.at,b.at)<=Math.max(a.radius,b.radius)+TOL)parent[find(i)]=find(j);
 }
 const clusters=new Map<number,number[]>();items.forEach((_,i)=>{const r=find(i);clusters.set(r,[...(clusters.get(r)??[]),i]);});
 const blocks:ChannelBlockSite[]=[],blockRoots=new Set<number>();
 for(const [root,ids] of clusters){
  const m=ids.map(i=>items[i]),ends=m.filter(x=>x.kind==='track-end').length,solid=m.filter(x=>x.kind!=='track-end');
  const contact=solid.some(x=>x.contact),via=solid.some(x=>x.kind==='via'||x.layers.length>1),other=solid.filter(x=>!x.contact);
  const degree=ends+(contact?1:0);
  const kind:ChannelBlockSite['kind']|undefined=via?'layer-transition':other.length?'component-pad':degree>=3?'branch':(ends===1&&!contact)?'open-end':contact&&ends>1?'branch':undefined;
  if(!kind)continue;
  blockRoots.add(root);
  const layers=[...new Set(m.flatMap(x=>x.layers))],c=(solid[0]??m[0]).at;
  blocks.push({kind,atMm:c,layers,sourcePaths:[...new Set(solid.map(x=>x.path))]});
 }
 // Sections: track segments joined through plain two-segment nodes (no block there).
 const sp=segs.map((_,i)=>i),sf=(i:number):number=>sp[i]===i?i:(sp[i]=sf(sp[i]));
 for(const [root,ids] of clusters){if(blockRoots.has(root))continue;const s=ids.map(i=>items[i].seg).filter((v):v is number=>v!==undefined);for(let k=1;k<s.length;k++)sp[sf(s[k])]=sf(s[0]);}
 const pours=new Set(signal.filter(s=>indexOf(s.sourcePath.replace(/\.filledPolygons.*$/,''),'copper.fixed.zones')!==undefined).map(s=>s.sourcePath.replace(/\.filledPolygons.*$/,''))).size;
 const byLayer:Record<string,number>={},groups=new Set<number>();let total=0;
 segs.forEach((g,i)=>{groups.add(sf(i));const l=dist(g.a,g.b);total+=l;byLayer[g.layer]=(byLayer[g.layer]??0)+l;});
 const count=blocks.reduce((m:Record<string,number>,b)=>({...m,[b.kind]:(m[b.kind]??0)+1}),{});
 const summary=groups.size+' track sections ('+total.toFixed(2)+' mm) and blocks needed at: '+(Object.entries(count).map(([k,n])=>n+' '+k).join(', ')||'none')+(pours?'; '+pours+' copper pour on the signal island':'');
 return {sections:{count:groups.size,totalLengthMm:total,byLayerMm:byLayer},blocks,pours,summary};
}
