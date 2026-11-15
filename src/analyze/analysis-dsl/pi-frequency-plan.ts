import {p7FrequencyBoundariesHz,type FrequencyInterval} from '../p7-frequency-planning.ts';

/** Frequency plan of the board PI (contract sections 7 and 9). The extraction cost is minutes per frequency, so a user
 * axis never goes to the extraction point by point: each occupied band is extracted once at a few anchors and the
 * reduced model of that band is evaluated at every requested point. */
export const PI_AC_FLOOR_HZ=1e3;
export const PI_AC_CEILING_HZ=40e6;
/** Quasi-static electrical-size factor of the native hybrid screen (`2 pi f/c D sqrt(er) <= 0.1`). */
export const PI_QUASI_STATIC_KD=0.1;
const C0=299792458;

export interface PiStackLayer{thicknessM:number;epsilonR:number|null}
/** Upper frequency of the quasi-static screen for a region of interest (mm bounds) and stack. */
export function quasiStaticLimitHz(roiMm:readonly (readonly [number,number])[],stack:readonly PiStackLayer[]):number{
 const dx=1e-3*(roiMm[1][0]-roiMm[0][0]),dy=1e-3*(roiMm[1][1]-roiMm[0][1]);
 const dz=stack.reduce((s,l)=>s+l.thicknessM,0);
 const epsilon=stack.reduce((m,l)=>Math.max(m,l.epsilonR??1),1);
 return PI_QUASI_STATIC_KD*C0/(2*Math.PI*Math.hypot(dx,dy,dz)*Math.sqrt(epsilon));
}

export interface PiBand{
 /** Band boundaries of the P7 planner that contain requested points. */
 boundsHz:FrequencyInterval;
 /** Extraction interval: lowest and highest requested point of the band (the profile is frozen at the top). */
 extractionHz:FrequencyInterval;
 pointIndices:number[];
 anchorsHz:number[];
}
export interface PiUnavailable{index:number;frequencyHz:number;reason:string}
export interface PiFrequencyPlan{
 fHiHz:number;
 fHiReason:string;
 bands:PiBand[];
 unavailable:PiUnavailable[];
 /** Frequencies of the junctions between occupied bands whose models are both evaluated. */
 junctionsHz:number[];
}

/** Anchor frequencies of one band: both extraction ends, a log-spaced interior (about one per decade, at most 6 in
 * total) and any characteristic frequencies, never more than `maxAnchors` (the 256 frequency x port column limit). */
export function anchorFrequencies(extractionHz:FrequencyInterval,maxAnchors:number,characteristicHz:readonly number[]=[]):number[]{
 const [lo,hi]=extractionHz;
 if(lo===hi)return [lo];
 const cap=Math.max(2,Math.min(6,maxAnchors));
 const decades=Math.log10(hi/lo);
 const count=Math.min(cap,Math.max(3,Math.ceil(decades)+1));
 const base=Array.from({length:count},(_,i)=>lo*(hi/lo)**(i/(count-1)));
 base[0]=lo;base[count-1]=hi;
 const extra=characteristicHz.filter(f=>f>lo&&f<hi).slice(0,Math.max(0,maxAnchors-count));
 return [...new Set([...base,...extra])].sort((a,b)=>a-b);
}

export function planPiFrequency(options:{
 frequenciesHz:readonly number[];portCount:number;quasiStaticHz:number;
 /** Highest frequency admitted by the declared models of the components (none: unrestricted). */
 modelCeilingHz?:number;characteristicHz?:readonly number[];
}):PiFrequencyPlan{
 const {frequenciesHz,portCount,quasiStaticHz,modelCeilingHz=Infinity,characteristicHz}=options;
 const candidates:[number,string][]=[[PI_AC_CEILING_HZ,'the P8 v1 ceiling of 40 MHz'],
  [quasiStaticHz,'the quasi-static electrical-size limit of the board region (2 pi f D sqrt(er)/c <= '+PI_QUASI_STATIC_KD+')'],
  [modelCeilingHz,'the declared band of a component model']];
 const [fHiHz,fHiReason]=candidates.reduce((a,b)=>b[0]<a[0]?b:a);
 const unavailable:PiUnavailable[]=[];
 const occupied=new Map<number,number[]>();
 const edges=p7FrequencyBoundariesHz.filter(f=>f>=PI_AC_FLOOR_HZ);
 frequenciesHz.forEach((f,index)=>{
  if(f<PI_AC_FLOOR_HZ){unavailable.push({index,frequencyHz:f,reason:'below the 1 kHz AC extraction floor; the DC point is frequencyHz(0)'});return;}
  if(f>fHiHz){unavailable.push({index,frequencyHz:f,reason:'above f_hi = '+fHiHz+' Hz, '+fHiReason+'; values are not extrapolated'});return;}
  // Band k covers (edges[k], edges[k+1]]; the lowest band also contains its lower edge.
  let band=0;
  while(band+1<edges.length&&f>edges[band+1])band++;
  occupied.set(band,[...(occupied.get(band)??[]),index]);
 });
 const maxAnchors=Math.max(2,Math.floor(256/portCount));
 const bands=[...occupied].sort((a,b)=>a[0]-b[0]).map(([band,pointIndices]):PiBand=>{
  const values=pointIndices.map(i=>frequenciesHz[i]);
  const extractionHz:FrequencyInterval=[Math.min(...values),Math.max(...values)];
  return {boundsHz:[edges[band],Math.min(edges[band+1]??fHiHz,fHiHz)],extractionHz,pointIndices,
   anchorsHz:anchorFrequencies(extractionHz,maxAnchors,characteristicHz)};
 });
 const junctionsHz=bands.slice(1).map((band,i)=>band.boundsHz[0]).filter((f,i)=>bands[i].boundsHz[1]===f);
 return {fHiHz,fHiReason,bands,unavailable,junctionsHz};
}

/** Three-tier policy of the contract (section 10) for a relative mismatch: noise, approximation, refusal. */
export type PiTier='noise'|'approximation'|'refusal';
export function fitTier(relative:number,noise=0.01,approximation=0.10):PiTier{
 return relative<=noise?'noise':relative<=approximation?'approximation':'refusal';
}
