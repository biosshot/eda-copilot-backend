import type { BoardFoilReport } from './types.ts';
import {projectBoardFoilLossGrams} from './board-foil-loss-grams.ts';

type Complex=Readonly<{real:number;imaginary:number}>;
export type MixedModePair=Readonly<{positive:string;negative:string;differential:string;common:string}>;
const sqrt2=Math.sqrt(2);

/** Power-wave basis transform over explicitly paired physical port names. */
export function boardFoilMixedMode(report:BoardFoilReport,portNames:readonly string[],pairs:readonly MixedModePair[]) {
 const n=portNames.length,nf=report.frequenciesHz.length;
 if(!n||n>16||nf===0||nf*n>256||report.parameters.length!==nf||report.referenceOhm.length!==n||
    new Set(portNames).size!==n||portNames.some(name=>!name.trim()))
  throw new Error('CHANNEL_FOIL_MIXED_MODE: invalid or oversized port/frequency dimensions');
 const index=new Map(portNames.map((name,i)=>[name,i]));
 const used=new Set<string>(),outNames:string[]=[],modes:Array<{inputIndices:number[];weights:number[];referenceOhm:number}>=[];
 for(const pair of pairs){
  if(!pair.positive.trim()||!pair.negative.trim()||!pair.differential.trim()||!pair.common.trim()||
     pair.positive===pair.negative||pair.differential===pair.common||
    !index.has(pair.positive)||!index.has(pair.negative)||used.has(pair.positive)||used.has(pair.negative)||
     outNames.includes(pair.differential)||outNames.includes(pair.common))
   throw new Error('CHANNEL_FOIL_MIXED_MODE: pairs must name disjoint physical ports and unique output modes');
  const pi=index.get(pair.positive)!,ni=index.get(pair.negative)!;
  const rp=report.referenceOhm[pi],rn=report.referenceOhm[ni];
  if(!Number.isFinite(rp)||rp<=0||!Number.isFinite(rn)||rn<=0||rp!==rn)
   throw new Error('CHANNEL_FOIL_MIXED_MODE: each pair requires equal positive finite wave references');
  if(!Number.isFinite(2*rp)||!Number.isFinite(rp/2))throw new Error('CHANNEL_FOIL_MIXED_MODE: modal reference overflow');
  used.add(pair.positive);used.add(pair.negative);
  outNames.push(pair.differential,pair.common);
  modes.push({inputIndices:[pi,ni],weights:[1/sqrt2,-1/sqrt2],referenceOhm:2*rp},
   {inputIndices:[pi,ni],weights:[1/sqrt2,1/sqrt2],referenceOhm:rp/2});
 }
 for(let i=0;i<n;i++)if(!used.has(portNames[i])){const referenceOhm=report.referenceOhm[i];if(!Number.isFinite(referenceOhm)||referenceOhm<=0)throw new Error('CHANNEL_FOIL_MIXED_MODE: positive finite references required');outNames.push(portNames[i]);modes.push({inputIndices:[i],weights:[1],referenceOhm});}
 if(new Set(outNames).size!==outNames.length||outNames.some(name=>!name.trim()))
  throw new Error('CHANNEL_FOIL_MIXED_MODE: output mode names must be globally unique');
 const parameters=report.frequenciesHz.map((frequencyHz,f)=>{
  const s=report.parameters[f];
  if(!Number.isFinite(frequencyHz)||frequencyHz<=0||s.length!==n||s.some(row=>row.length!==n||row.some(z=>!Number.isFinite(z.real)||!Number.isFinite(z.imaginary))))
   throw new Error('CHANNEL_FOIL_MIXED_MODE: non-finite or non-square checked S matrix');
  const transformed=modes.map((outMode,i)=>modes.map((inMode,j)=>{
   let real=0,imaginary=0;
   for(let a=0;a<outMode.inputIndices.length;a++)for(let b=0;b<inMode.inputIndices.length;b++){
    const value=s[outMode.inputIndices[a]][inMode.inputIndices[b]],weight=outMode.weights[a]*inMode.weights[b];
    real+=weight*value.real;imaginary+=weight*value.imaginary;
   }
   return {real,imaginary};
  }));
  if(transformed.some(row=>row.some(z=>!Number.isFinite(z.real)||!Number.isFinite(z.imaginary))))
   throw new Error('CHANNEL_FOIL_MIXED_MODE: transformed S overflow');
  const accepted=transformed.map((_,j)=>{
   const reflected=transformed.reduce((sum,outputRow)=>sum+outputRow[j].real*outputRow[j].real+outputRow[j].imaginary*outputRow[j].imaginary,0);
   const incoming=modes[j].referenceOhm/4;
   const value=incoming*(1-reflected);
   const roundingBoundW=64*Number.EPSILON*(n+1)*incoming;
   if(!Number.isFinite(value)||value < -roundingBoundW)
    throw new Error('CHANNEL_FOIL_MIXED_MODE: materially negative accepted power indicates a non-passive transformed model');
   return {powerW:value<0?0:value,roundingBoundW};
  });
  const grams=report.samples[f]?.lossGrams;
  const losses=grams&&modes.map((mode,j)=>{
   const drive=Array.from({length:n},()=>({real:0,imaginary:0}));
   mode.inputIndices.forEach((i,k)=>{drive[i].real=mode.weights[k]*Math.sqrt(mode.referenceOhm)/Math.sqrt(report.referenceOhm[i]);});
   const value=projectBoardFoilLossGrams(grams,drive),power=accepted[j].powerW;
   if(Math.abs(value.totalLossW-power)>1e-12+1e-8*(value.totalLossW+power))throw Error('CHANNEL_FOIL_MIXED_MODE: modal copper/package power mismatch');
   return value;
  });
  return {frequencyHz,parameters:transformed,acceptedPowerW:accepted.map(x=>x.powerW),acceptedPowerRoundingBoundW:accepted.map(x=>x.roundingBoundW),...(losses&&{losses})};
 });
 return {sourcePortNames:[...portNames],modePairs:pairs.map(pair=>({...pair})),portNames:outNames,referenceOhm:modes.map(mode=>mode.referenceOhm),parameters,
  lossRecovery:{scope:report.samples.every(s=>s.lossGrams)?'matched drive loss Grams':'total accepted boundary power only',separateCopperAndPackages:report.samples.every(s=>s.lossGrams)?'computed' as const:'unverified' as const},
  physicalCoverage:{status:'unverified' as const,reason:'Mixed-mode S is a mathematical power-wave transform of checked physical-port samples; it adds no physical acceptance or spatial recovery.'}};
}
