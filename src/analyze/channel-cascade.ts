/** Product channel cascade (P7-completion 4.4; on by default, P7_CHANNEL_CASCADE=0 disables): a chain of 2D wave sections
 * stitched by S-parameters, each section sampled and stitched at every requested frequency separately
 * (so frequency-dependent R(f), L(f) need no causal model; that is stage 7). Physical accuracy is `unverified`. */
import type {AnalysisProblem} from './types.ts';
import {sectionTransitionFrequencyRequest} from './segmented-channel.ts';
import {capturedBoardGeometry} from './geometry-snapshot.ts';
import {cascadeTwoPorts,cx,maxSingularValue,phaseDelayS,reciprocityError,terminatePorts,type CMatrix} from './channel-cascade-stitch.ts';
import type {CascadeSection,ChannelModelChoice} from './channel-cascade-policy.ts';

// On by default (user decision after P7-completion 10.5); P7_CHANNEL_CASCADE=0 or false restores the one-domain path.
export const cascadeSwitchOn=()=>{const v=process.env.P7_CHANNEL_CASCADE;return !(v==='0'||v?.toLowerCase()==='false');};
const EPS0=8.8541878128e-12,C0=299792458;
/** New internal consistency checks of the stitched result; not acceptance thresholds of the extractor. */
const PASSIVITY_TOL=1e-8,RECIPROCITY_TOL=1e-8;
/** Joint without a block is accepted only while the estimated reflection of the omitted discontinuity stays below this at the top frequency (4.1 memo). */
export const JOINT_S11_LIMIT=0.01;
/** Slice half widths tried in order; the transverse width of the domain is limited by the section applicability screen at 1 GHz. */
const SLICE_HALF_WIDTHS_MM=[2.0,1.8,1.5],SLICE_AIR_MM=1.5;
const fail=(code:string,message:string):never=>{throw Error(code+': '+message);};
type Execute=(problem:AnalysisProblem)=>any;
type Cascade=Extract<ChannelModelChoice,{mode:'cascade'}>;

export interface CascadeSectionReport {
 id:string;element:string;sourcePath:string;lengthMm:number;widthMm:number;conductors:number;
 modes:{delaySPerM:number;impedanceOhm:number}[];minGapMm?:number;wallSeconds:number;
 maxSingularValue:number;reciprocityError:number;sliceHalfWidthMm:number;
}
export interface CascadeJointReport {
 afterSection:string;beforeSection:string;type:string;bendDeg:number;
 estimatedS11AtTopFrequency:number;estimatedDelayErrorPs:number;neighbourLengthsMm:[number,number];shortNeighbour:boolean;
}
export interface CascadeSolved {
 frequenciesHz:number[];parameters:{real:number;imaginary:number}[][][];
 sections:CascadeSectionReport[];joints:CascadeJointReport[];
 maxSingularValue:number;maxReciprocityError:number;
 delayPs:number[];totalLengthMm:number;wallSeconds:number;warnings:string[];
}
const pointInRing=(ring:number[][],p:[number,number])=>{let inside=false;for(let i=0,j=ring.length-1;i<ring.length;j=i++){const [xi,yi]=ring[i],[xj,yj]=ring[j];if((yi>p[1])!==(yj>p[1])&&p[0]<(xj-xi)*(p[1]-yi)/(yj-yi)+xi)inside=!inside;}return inside;};

/** The cascade needs one reference copper layer: the stack's only other copper layer, and every
 * return contact must be on the same physical copper island as the reference region under the channel. */
function referenceFor(g:any,stack:any[],choice:Cascade,returns:{sourcePath:string;layer:string}[],at:[number,number]){
 const copper:string[]=stack.filter(l=>l.copper).map(l=>l.name);
 if(copper.length!==2||!copper.includes(choice.layer))fail('CHANNEL_CASCADE_REFERENCE','cascade needs a two-copper-layer stack with the channel on one of them');
 const layer=copper.find(n=>n!==choice.layer)!;
 const region=g.regions.find((r:any)=>r.layer===layer&&pointInRing(r.polygon.outer,at)&&!r.polygon.holes.some((h:number[][])=>pointInRing(h,at)));
 if(!region)fail('CHANNEL_CASCADE_REFERENCE','no reference copper under the channel at '+at.join(','));
 const islandOf=(regionId:string)=>g.islands.find((i:any)=>i.region_ids.includes(regionId))?.id;
 const island=islandOf(region.id);
 for(const c of returns){
  const matching=g.regions.filter((r:any)=>r.source.path===c.sourcePath&&r.layer===c.layer);
  if(matching.length!==1||islandOf(matching[0].id)!==island)fail('CHANNEL_CASCADE_REFERENCE','return contact '+c.sourcePath+' is not on the reference copper island');
 }
 return {layer,regionPath:region.source.path as string};
}

export function solveChannelCascade(execute:Execute,input:{
 boardJson:string;stack:any[];choice:Cascade;returns:{sourcePath:string;layer:string}[];frequencies:number[];
 budget:{maxSeconds:number;maxMemoryMiB:number};
}):CascadeSolved{
 const start=performance.now(),{stack,choice,frequencies:FS}=input;
 const warnings:string[]=[];
 const g:any=capturedBoardGeometry(input.boardJson);
 const dielectricHeightM=stack.filter(l=>!l.copper).reduce((s,l)=>s+l.thicknessM,0);
 const memory=Math.min(2048,input.budget.maxMemoryMiB);
 const twoPorts:CMatrix[][]=[];const sections:CascadeSectionReport[]=[];const topEps:number[]=[],topZ:number[]=[];
 for(const sec of choice.sections){
  const t0=performance.now(),remaining=input.budget.maxSeconds-(t0-start)/1000;
  if(remaining<=0)fail('CHANNEL_CASCADE_BUDGET','time budget exhausted before section '+sec.id);
  const mid:[number,number]=[(sec.startMm[0]+sec.endMm[0])/2,(sec.startMm[1]+sec.endMm[1])/2];
  const ref=referenceFor(g,stack,choice,input.returns,mid);
  let attempt:any;const errors:string[]=[];
  for(const hw of SLICE_HALF_WIDTHS_MM){
  const board:any=execute({kind:'board-section',boardJson:input.boardJson,stack,slice:{atMm:mid,transverseUnit:[-sec.directionUnit[1],sec.directionUnit[0]],halfWidthMm:hw,airMarginMm:SLICE_AIR_MM,referenceLayer:ref.layer,referenceSourcePath:ref.regionPath,referenceXMm:0}} as any);
  const geo=board.geometry,map:any[]=board.sourceMapping,nc=geo.conductors.length;
  const candidates=map.map((m,i)=>({m,i})).filter(({m})=>m.role!=='reference'&&m.layer===choice.layer&&m.intervalMm[0]<0&&m.intervalMm[1]>0);
  if(candidates.length!==1)fail('CHANNEL_CASCADE_SECTION','section '+sec.id+': signal conductor not found at the slice');
  const {m:strip,i:si}=candidates[0],width=strip.intervalMm[1]-strip.intervalMm[0];
  if(!strip.sourcePaths.includes(sec.sourcePath)||Math.abs(width-sec.widthMm)>0.05*sec.widthMm+0.01)
   fail('CHANNEL_CASCADE_SECTION','section '+sec.id+': copper at the slice ('+width.toFixed(3)+' mm) is not the planned track/pad ('+sec.widthMm.toFixed(3)+' mm); neighbouring copper merges with the signal');
  const gaps=map.filter((m,i)=>i!==si&&m.layer===choice.layer).flatMap(m=>[Math.abs(m.intervalMm[0]-strip.intervalMm[1]),Math.abs(strip.intervalMm[0]-m.intervalMm[1])]).filter(v=>v>=0);
  const layerOf=(name:string)=>stack.find(l=>l.name===name);
  const loss={frequenciesHz:FS,conductivitySM:map.map(m=>layerOf(m.layer).conductivitySM),minimumSkinDimensionsM:map.map(m=>layerOf(m.layer).thicknessM),
   dielectricLossTangents:geo.dielectrics.map(()=>0),bandHz:[FS[0],FS.at(-1)!]};
  const line={kind:'polygon-section' as const,lengthMm:sec.lengthMm,geometry:{...geo,maxNodes:Math.max(geo.maxNodes??0,200000),maxMemoryMib:memory,loss}};
  const cs:any=execute({kind:'cross-section',request:{line,frequencies:FS,mesh:{maxRefinements:5,maxNodes:200000,maxTriangles:400000},budget:{maxSeconds:Math.min(600,remaining),maxMemoryMiB:memory}}} as any);
  if(cs.status==='complete'){attempt={board,geo,map,nc,strip,si,width,gaps,cs,hw,loss};break;}
  errors.push('half width '+hw+' mm: '+cs.status+' '+(cs.diagnostics??[]).map((d:any)=>d.code).slice(0,2).join(', '));
  }
  if(!attempt)fail('CHANNEL_CASCADE_SECTION','section '+sec.id+' cross-section did not converge: '+errors.join('; '));
  const {geo,map,nc,strip,si,gaps,cs,hw}=attempt;
  const ids:string[]=geo.conductors.map((c:any)=>c.id),signalIds:string[]=cs.conductorOrder,n=signalIds.length;
  const key=(e:number,id:string)=>sec.id+':e'+e+':'+id;
  const ends=[0,1].map(e=>Object.fromEntries(ids.map(id=>[id,key(e,id)])));
  const cuts=[0,1].flatMap(e=>signalIds.map(id=>({positive:key(e,id),negative:key(e,cs.reference)})));
  const stripPosition=signalIds.indexOf(strip.conductorId);
  if(stripPosition<0)fail('CHANNEL_CASCADE_SECTION','section '+sec.id+': signal conductor missing from the producer order');
  const electric={conductorOrder:cs.conductorOrder,reference:cs.reference,capacitanceFPerM:cs.parameters.capacitanceFPerM,modes:cs.parameters.modes};
  const perFrequency:CMatrix[]=[];let sigma=0,recip=0;
  for(const [k,f] of FS.entries()){
   const smp=cs.frequencyDependent.samples[k];
   const sample={frequencyHz:f,conductorOrder:ids,signalIds,referenceId:cs.reference,resistanceOhmPerM:smp.resistanceOhmPerM,totalInductanceHPerM:smp.totalInductanceHPerM};
   const request=sectionTransitionFrequencyRequest({id:sec.id+':'+k,line:{kind:'polygon-section',lengthMm:sec.lengthMm,geometry:{conductors:geo.conductors}},electric,sample,terminals:{ends:ends as any,cuts},budget:{maxSeconds:Math.min(600,input.budget.maxSeconds-(performance.now()-start)/1000),maxMemoryMiB:memory}});
   const network:any=execute({kind:'frequency-network',request} as any);
   if(network.status!=='complete')fail('CHANNEL_CASCADE_SECTION','section '+sec.id+' network at '+f+' Hz '+network.status+': '+(network.diagnostics??[]).map((d:any)=>d.code).slice(0,2).join(', '));
   const S:CMatrix=network.parameters[0].map((row:any[])=>row.map(v=>cx(v.real,v.imaginary)));
   const keep=[stripPosition,n+stripPosition],term=[...Array(2*n).keys()].filter(i=>!keep.includes(i));
   // Pour halves and other section conductors are shorted to the reference at both section ends.
   const S2=terminatePorts(S,keep,term,cx(-1,0));
   sigma=Math.max(sigma,maxSingularValue(S2));recip=Math.max(recip,reciprocityError(S2));
   perFrequency.push(S2);
  }
  if(sigma>1+PASSIVITY_TOL||recip>RECIPROCITY_TOL)fail('CHANNEL_CASCADE_PASSIVITY','section '+sec.id+' two-port is not passive/reciprocal: max singular value '+sigma+', reciprocity '+recip);
  const modes=cs.parameters.modes.map((m:any)=>({delaySPerM:m.delaySPerM,impedanceOhm:m.impedanceOhm}));
  // Main mode = the lowest-impedance mode (strip against the pour/reference); the pour-half modes are slower and higher-Z.
  const main=modes.reduce((a:any,m:any)=>m.impedanceOhm<a.impedanceOhm?m:a);
  topEps.push((main.delaySPerM*C0)**2);topZ.push(main.impedanceOhm);
  twoPorts.push(perFrequency);
  sections.push({id:sec.id,element:sec.kind,sourcePath:sec.sourcePath,lengthMm:sec.lengthMm,widthMm:sec.widthMm,conductors:nc,modes,
   minGapMm:gaps.length?Math.min(...gaps):undefined,wallSeconds:(performance.now()-t0)/1000,maxSingularValue:sigma,reciprocityError:recip,sliceHalfWidthMm:hw});
 }
 // Joints without a block: shunt capacitance C = eps0 eps_eff w (k = 1, upper estimate), |dS11| = w C Z0 / 2, dt = C Z0 / 2.
 const fTop=FS.at(-1)!;
 const joints:CascadeJointReport[]=choice.joints.map((j,i)=>{
  const w=Math.max(j.widthAMm,j.widthBMm)*1e-3,Z=Math.max(topZ[i],topZ[i+1]),eps=Math.max(topEps[i],topEps[i+1]);
  const C=EPS0*eps*w,s11=2*Math.PI*fTop*C*Z/2;
  const A=choice.sections[i],B=choice.sections[i+1],hMm=dielectricHeightM*1e3;
  const need=(s:CascadeSection,r:CascadeSectionReport)=>2*Math.max(hMm,s.widthMm,r.minGapMm??0);
  const short=A.lengthMm<need(A,sections[i])||B.lengthMm<need(B,sections[i+1]);
  if(s11>JOINT_S11_LIMIT)fail('CHANNEL_CASCADE_JOINT','joint '+j.afterSection+' -> '+j.beforeSection+' ('+j.kind+') omitted-discontinuity estimate |dS11| '+s11.toExponential(2)+' at '+fTop+' Hz exceeds '+JOINT_S11_LIMIT+'; a 3D block is required');
  if(short)warnings.push('Joint '+j.afterSection+' -> '+j.beforeSection+': a neighbouring section is shorter than 2 max(h, w, gap); fields of adjacent discontinuities overlap and the joint error estimate is not valid there.');
  return {afterSection:j.afterSection,beforeSection:j.beforeSection,type:j.kind,bendDeg:j.bendDeg,estimatedS11AtTopFrequency:s11,estimatedDelayErrorPs:C*Z/2*1e12,neighbourLengthsMm:[A.lengthMm,B.lengthMm],shortNeighbour:short};
 });
 const stitched=FS.map((_,k)=>cascadeTwoPorts(...twoPorts.map(s=>s[k])));
 const sigma=Math.max(...stitched.map(maxSingularValue)),recip=Math.max(...stitched.map(reciprocityError));
 if(sigma>1+PASSIVITY_TOL||recip>RECIPROCITY_TOL)fail('CHANNEL_CASCADE_PASSIVITY','stitched channel is not passive/reciprocal: max singular value '+sigma+', reciprocity '+recip);
 if(stitched.some(S=>S.some(row=>row.some(v=>!Number.isFinite(v.re)||!Number.isFinite(v.im)))))fail('CHANNEL_CASCADE_PASSIVITY','stitched channel has nonfinite entries');
 return {frequenciesHz:FS,parameters:stitched.map(S=>S.map(row=>row.map(v=>({real:v.re,imaginary:v.im})))),sections,joints,maxSingularValue:sigma,maxReciprocityError:recip,
  delayPs:stitched.map((S,k)=>phaseDelayS(S[1][0],FS[k])*1e12),totalLengthMm:choice.sections.reduce((s,x)=>s+x.lengthMm,0),wallSeconds:(performance.now()-start)/1000,warnings};
}
