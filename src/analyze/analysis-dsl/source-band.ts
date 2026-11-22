import type {Time,Waveform} from './spec-doc.js';
import {AnalysisDslError} from './schema.ts';
/** Fourier convention X(f)=integral x(t) exp(-i2pi ft)dt. Constant
 * prehistory and a constant continuation after the relevant finite event train.
 * For continuous PWL, two-sided L1 tail <= TV(x')/(2*pi²*F). This is an
 * input-voltage bound; a physical output error also needs an admitted causal
 * transfer bound and initial-state coverage. No sampled output cadence enters it. */
export function sourceBandPlan(w:Waveform,time:Time,tailFraction=0.01){
 const bad=(s:string):never=>{throw new AnalysisDslError('SOURCE_BAND_INPUT',s);};
 if(!Number.isFinite(time.stopS)||time.stopS<=0||!Number.isFinite(1/time.stopS)||!Number.isFinite(tailFraction)||tailFraction<=0||tailFraction>=1)bad('Positive finite duration and input-tail fraction in (0,1) required');
 const base={method:'analytic two-sided Fourier L1 tail',waveformKind:w.kind,durationS:time.stopS,dcRequired:true,positiveFrequencyResolutionHz:1/time.stopS,inputTailFraction:tailFraction,inputNormalization:'authored waveform excursion; an absolute input-voltage budget, not output relative error',outputAccuracy:'unverified; source bound requires a causal transfer/initial-state applicability bound',initialization:'constant source prehistory at its initial value; explicit stored-state transients need separate coverage'};
 const unavailable=(reason:string)=>({...base,status:'unavailable' as const,reason});
 let excursion=0,variation=0,events=0;const characteristicHz:number[]=[];
 if(w.kind==='step'){
  excursion=Math.abs(w.to-w.from);if(w.atS>=time.stopS)excursion=0;
  if(excursion){if(w.riseS<=0)return unavailable('Ideal voltage jump has no finite L1 spectral-tail bound');variation=2*excursion/w.riseS;events=2;characteristicHz.push(1/w.riseS);}
 }else if(w.kind==='sine'){
  excursion=2*w.amplitude;const f=1/w.periodS;
  if(!Number.isFinite(f)||f<=0)bad('Representable positive sine period required');
  const upper=excursion===0?0:f/Math.tanh(Math.PI*tailFraction*excursion/w.amplitude);
  if(!Number.isFinite(upper))bad('Required sine bandwidth exceeds finite numerical range');
  return{...base,status:'bounded-input' as const,excursionV:excursion,requiredUpperHz:upper,inputTailBoundV:tailFraction*excursion,characteristicHz:[f],derivation:'For unilateral sine with continuous constant prehistory, tail <= A/(2pi)*log((F+f0)/(F-f0)); F>f0. Includes onset, not only the carrier line.'};
 }else if(w.kind==='pulse'){
  excursion=Math.abs(w.high-w.low);const count=Math.max(0,Math.min(w.count,Math.ceil((time.stopS-w.delayS)/w.periodS)));
  if(count===0)excursion=0;
  if(excursion){if(w.riseS<=0||w.fallS<=0)return unavailable('Pulse jump has no finite L1 spectral-tail bound');variation=2*count*excursion*(1/w.riseS+1/w.fallS);events=4*count;characteristicHz.push(1/w.periodS,1/w.riseS,1/w.fallS);if(w.highS>0)characteristicHz.push(1/w.highS);}
 }else{
  if(w.points.length<2||w.points.length>100000)bad('Bounded PWL event description required');
  const values=w.points.map(p=>p[1]);let lo=Infinity,hi=-Infinity;for(const value of values){lo=Math.min(lo,value);hi=Math.max(hi,value);}excursion=hi-lo;
  const count=w.repeat?Math.min(w.repeat.count,Math.ceil(time.stopS/w.repeat.periodS)):1;
  if(w.interpolation==='hold'&&excursion>0)return unavailable('Held PWL jumps have no finite L1 spectral-tail bound');
  if(count>1&&values[0]!==values.at(-1))return unavailable('Repeated PWL has a discontinuous cycle boundary');
  const slopes:number[]=[];
  for(let i=1;i<w.points.length;i++){
   const [a,x]=w.points[i-1],[b,y]=w.points[i];if(!Number.isFinite(a)||!Number.isFinite(b)||!Number.isFinite(x)||!Number.isFinite(y)||b<=a)bad('Finite strictly ordered PWL points required');
   if(count===1&&a>=time.stopS)break;
   slopes.push((y-x)/(b-a));characteristicHz.push(1/(b-a));
  }
  if(slopes.length){const first=slopes[0],last=slopes.at(-1)!;variation=count*(Math.abs(first)+Math.abs(last));for(let i=1;i<slopes.length;i++)variation+=count*Math.abs(slopes[i]-slopes[i-1]);events=count*(slopes.length+1);}
  if(w.repeat)characteristicHz.push(1/w.repeat.periodS);
 }
 if(!Number.isFinite(excursion)||!Number.isFinite(variation)||characteristicHz.some(f=>!Number.isFinite(f)||f<=0))bad('Source spectral bound exceeds finite numerical range');
 const upper=excursion===0?0:variation/(2*Math.PI**2*tailFraction*excursion);
 if(!Number.isFinite(upper))bad('Required source bandwidth exceeds finite numerical range');
 const seeds=[...new Set(characteristicHz)].sort((a,b)=>a-b);const limited=seeds.length>64?Array.from({length:64},(_,i)=>seeds[Math.round(i*(seeds.length-1)/63)]):seeds;
 return{...base,status:'bounded-input' as const,characteristicSeedCount:seeds.length,omittedSeedCount:seeds.length-limited.length,excursionV:excursion,slopeTotalVariationVPerS:variation,relevantSlopeEvents:events,requiredUpperHz:upper,inputTailBoundV:tailFraction*excursion,characteristicHz:limited,derivation:'Continuous finite PWL: distributional second derivative is a finite measure; |X(f)|<=TV(x\u2032)/(4pi²f²), so integral(|f|>F)|X(f)|df<=TV(x\u2032)/(2pi²F). Future complete cycles can be omitted by causality.'};
}

/** Absolute omitted-HF voltage error between two passive LTI responses.
 * Each finite Thevenin source supplies at most |Vs|²/(4Rs); a finite shunt
 * receiver dissipates |Vo|²/Ro. Hence |H|<=sqrt(Ro/Rs)/2 per source, and the
 * difference of two passive responses is bounded by sqrt(Ro/Rs).
 * This supplies no in-band/model/spatial error certificate. */
export function passiveOutputTailBounds(input:{plans:ReturnType<typeof sourceBandPlan>[];sourceResistanceOhm:number[];outputs:{port:string;resistanceOhm?:number}[];bandHz:readonly number[]|null;zeroInitialState:boolean}){
 const unavailable=(reason:string)=>({status:'unavailable' as const,reason});
 if(input.outputs.length===0)return unavailable('No resistively loaded observed output was supplied');
 if(!input.zeroInitialState)return unavailable('Nonzero or unconfirmed source/state prehistory requires its own stored-energy coverage');
 if(!input.bandHz||input.bandHz[0]!==0||input.bandHz.length!==2||!Number.isFinite(input.bandHz[1])||input.bandHz[1]<=0)return unavailable('A declared common band including DC is required');
 if(input.plans.length!==input.sourceResistanceOhm.length||input.sourceResistanceOhm.some(r=>!Number.isFinite(r)||r<=0))return unavailable('Finite positive Thevenin resistance for every source is required');
 const upper=input.bandHz[1];const tails=input.plans.map(p=>{
  if(p.status!=='bounded-input')return undefined;
  if(p.excursionV===0)return 0;
  if('slopeTotalVariationVPerS'in p)return p.slopeTotalVariationVPerS/(2*Math.PI**2*upper);
  const carrier=p.characteristicHz[0];if(upper<=carrier)return undefined;
  return p.excursionV/(4*Math.PI)*Math.log((upper+carrier)/(upper-carrier));
 });
 if(tails.some(t=>t===undefined||!Number.isFinite(t)))return unavailable('Input spectral tail could not be bounded above the declared model band');
 const outputs=input.outputs.map(o=>{
  if(o.resistanceOhm===undefined||!Number.isFinite(o.resistanceOhm)||o.resistanceOhm<=0)return{port:o.port,status:'unavailable',reason:'A finite positive resistive receiver shunt is required for a voltage/power bound'};
  const bound=tails.reduce<number>((sum,t,i)=>t===0?sum:sum+Math.sqrt(o.resistanceOhm!)/Math.sqrt(input.sourceResistanceOhm[i])*t!,0);
  return Number.isFinite(bound)?{port:o.port,status:'bounded-absolute-HF-tail',absoluteDifferenceBoundV:bound,normalization:'absolute volts; no percentage of a weak/zero actual output is asserted'}:{port:o.port,status:'unavailable',reason:'Passive voltage-tail bound exceeds finite numerical range'};
 });
 return{status:'conditional-passivity-bound' as const,upperHz:upper,inputTailBoundsV:tails,outputs,assumptions:['Both actual and supplied linear networks are passive, with only the declared finite Thevenin sources and passive loads','Corresponding physical and model systems start unenergized with zero source values; stored-state mismatch is excluded','In-band extraction/interpolation and numerical time errors remain separate and must share the final observable error budget']};
}
