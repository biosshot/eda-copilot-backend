/** Selection rule and partition for the product channel cascade (P7-completion 4.4, switch P7_CHANNEL_CASCADE).
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
