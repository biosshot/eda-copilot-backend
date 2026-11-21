import {createRequire} from 'node:module';
import {backendResource} from '../runtime/resources.ts';
import type {RationalFitRequest,RationalFitResult,RationalMatrix,RationalModel} from '../../native/analyze/index.js';
import type {FrequencyInterval} from './p7-frequency-planning.ts';

/** P7-completion stage 7.5: a long frequency axis of one board channel is solved directly on a fit grid plus a few held-out
 * check frequencies; a rational model (vector fitting, stage 7.3) of the grid gives the other points only when its error
 * at the held-out points is within `RATIONAL_SWEEP_TOLERANCE` for every S entry and for the quantities derived from S
 * (port voltage and current, series impedance of a port pair). Otherwise the whole axis is solved directly, exactly as
 * without this module. Axes shorter than `RATIONAL_MIN_POINTS` never come here. */
export const RATIONAL_SWEEP_TOLERANCE=1e-3;
export const RATIONAL_FIT_POINTS=12;
export const RATIONAL_CHECK_POINTS=3;
/** Smallest axis for which grid + checks (15) is clearly fewer than the requested points. */
export const RATIONAL_MIN_POINTS=24;
const MAX_POLES=8;
const MAX_PORTS=8;

export type Matrix=RationalMatrix;
type Solved={report:{frequenciesHz:number[];parameters:Matrix[];referenceOhm:number[]}};
export type RationalApi={
 fit:(request:RationalFitRequest)=>RationalFitResult;
 evaluate:(request:{model:RationalModel;frequenciesHz:number[]})=>{samples:RationalMatrix[]};
};
let addon:RationalApi|undefined;
export const nativeRationalApi=():RationalApi=>{
 if(!addon){
  const native=createRequire(import.meta.url)(backendResource('native','node-addon','index.cjs')) as {fitRationalModel:RationalApi['fit'];evaluateRationalModel:RationalApi['evaluate']};
  addon={fit:native.fitRationalModel,evaluate:native.evaluateRationalModel};
 }
 return addon;
};

/** Indices (into the requested axis) of the fit grid and the held-out check points; null when the axis is too short. */
export function planRationalSweep(count:number):{grid:number[];checks:number[];rest:number[]}|null{
 if(!(count>=RATIONAL_MIN_POINTS))return null;
 const grid=Array.from({length:RATIONAL_FIT_POINTS},(_,k)=>Math.round(k*(count-1)/(RATIONAL_FIT_POINTS-1)));
 // One check in the middle of each of three gaps spread over the axis (gaps 2, 5 and 8 of 11).
 const checks=[2,5,8].map(k=>Math.floor((grid[k]+grid[k+1])/2));
 const taken=new Set([...grid,...checks]);
 return {grid,checks,rest:Array.from({length:count},(_,i)=>i).filter(i=>!taken.has(i))};
}

const abs=(z:{real:number;imaginary:number})=>Math.hypot(z.real,z.imaginary);
const sub=(a:{real:number;imaginary:number},b:{real:number;imaginary:number})=>({real:a.real-b.real,imaginary:a.imaginary-b.imaginary});
/** |a - b| / |b|; a zero reference matched exactly counts as no error, a nonzero value against it as infinite. */
const relative=(a:{real:number;imaginary:number},b:{real:number;imaginary:number})=>{
 const d=abs(sub(a,b)),m=abs(b);
 return m>0?d/m:d>0?Infinity:0;
};
const recip=(z:{real:number;imaginary:number})=>{const d=z.real*z.real+z.imaginary*z.imaginary;return {real:z.real/d,imaginary:-z.imaginary/d};};

export interface CheckError{
 frequencyHz:number;
 /** max over entries of |S_model - S_direct| / |S_direct| */
 entryRelative:number;worstEntry:[number,number];
 portVoltageRelative:number;portCurrentRelative:number;
 /** Series impedance 2 Z0 (1/S_ij - 1) of each port pair (i < j), worst relative error; null with one port. */
 seriesImpedanceRelative:number|null;
 worst:number;passed:boolean;
}
/** Strict relative errors of one held-out matrix: every entry against its own magnitude plus the derived quantities. */
export function checkError(frequencyHz:number,model:Matrix,direct:Matrix,referenceOhm:readonly number[],tolerance=RATIONAL_SWEEP_TOLERANCE):CheckError{
 const n=direct.length;
 let entry=0,worstEntry:[number,number]=[0,0],voltage=0,current=0,series:number|null=null;
 for(let i=0;i<n;i++)for(let j=0;j<n;j++){
  const e=relative(model[i][j],direct[i][j]);
  if(e>entry||Number.isNaN(e)){entry=Number.isNaN(e)?Infinity:e;worstEntry=[i,j];}
  // Port voltage and current of the unit-current drive of port j, as boardFoilPortObservations derives them.
  const k=Math.sqrt(referenceOhm[i]*referenceOhm[j])/2,delta=i===j?1:0;
  const volts=(s:{real:number;imaginary:number})=>({real:k*(delta+s.real),imaginary:k*s.imaginary});
  const amps=(v:{real:number;imaginary:number})=>({real:delta-v.real/referenceOhm[i],imaginary:-v.imaginary/referenceOhm[i]});
  const vm=volts(model[i][j]),vd=volts(direct[i][j]);
  voltage=Math.max(voltage,relative(vm,vd));
  current=Math.max(current,relative(amps(vm),amps(vd)));
 }
 for(let i=0;i<n;i++)for(let j=i+1;j<n;j++){
  const z0=Math.sqrt(referenceOhm[i]*referenceOhm[j]);
  const zs=(s:{real:number;imaginary:number})=>{const t=recip(s);return {real:2*z0*(t.real-1),imaginary:2*z0*t.imaginary};};
  const e=relative(zs(model[i][j]),zs(direct[i][j]));
  series=Math.max(series??0,Number.isNaN(e)?Infinity:e);
 }
 const finite=(v:number)=>Number.isNaN(v)?Infinity:v;
 const worst=Math.max(entry,finite(voltage),finite(current),series??0);
 return {frequencyHz,entryRelative:entry,worstEntry,portVoltageRelative:finite(voltage),portCurrentRelative:finite(current),seriesImpedanceRelative:series,worst,passed:worst<=tolerance};
}

export interface RationalSweepRecord{
 method:'rational'|'direct-fallback';
 /** Why the axis was solved directly; null when the rational model was used. */
 reason:string|null;
 toleranceRelative:number;
 requestedPoints:number;
 fitGridHz:number[];checkPointsHz:number[];
 checks:CheckError[];worstCheckRelative:number|null;
 poleCount:number|null;passive:boolean|null;
 fit:null|{rmsRelativeError:number;maxRelativeError:number;iterations:number;passivity:{passive:boolean;worstValue:number;worstFrequencyHz:number;violations:number};
  enforcement:RationalFitResult['enforcement'];poleSelection:RationalFitResult['poleSelection']};
 /** Direct frequency solves: the first call (grid + checks) and, after a fallback, the whole axis again. */
 directSolves:{first:number;second:number;total:number;ifAllDirect:number};
 model:RationalModel|null;
 rationalPoints:number;
}
export interface SweepOutcome<R extends Solved>{
 result:R;
 /** Present only when the rational model supplied points: all requested frequencies, direct values at the solved ones. */
 rational?:{frequenciesHz:number[];parameters:Matrix[];directFrequenciesHz:number[]};
 /** Present whenever the axis was long enough to try the rational path. */
 record?:RationalSweepRecord;
}

/** Solve `frequenciesHz` (strictly increasing). `solve(frequencies, interval)` is the existing direct path; the first call
 * omits the requested interval (so no between-point audit is spent on a grid that the held-out checks already audit),
 * the fallback call is the unchanged call of the whole axis. */
export function solveFrequencySweep<R extends Solved>(options:{
 frequenciesHz:number[];interval:FrequencyInterval;
 solve:(frequenciesHz:number[],interval:FrequencyInterval|undefined)=>R;
 api?:()=>RationalApi;tolerance?:number;
}):SweepOutcome<R>{
 const {frequenciesHz,interval,solve,tolerance=RATIONAL_SWEEP_TOLERANCE}=options;
 const plan=planRationalSweep(frequenciesHz.length);
 if(!plan)return {result:solve(frequenciesHz,interval)};
 const count=frequenciesHz.length;
 const gridHz=plan.grid.map(i=>frequenciesHz[i]),checkHz=plan.checks.map(i=>frequenciesHz[i]);
 const directHz=[...gridHz,...checkHz].sort((a,b)=>a-b);
 const base={toleranceRelative:tolerance,requestedPoints:count,fitGridHz:gridHz,checkPointsHz:checkHz};
 const first=solve(directHz,undefined);
 const fallback=(reason:string,part:Partial<RationalSweepRecord>={}):SweepOutcome<R>=>{
  const result=solve(frequenciesHz,interval);
  return {result,record:{...base,method:'direct-fallback',reason,checks:[],worstCheckRelative:null,poleCount:null,passive:null,fit:null,model:null,rationalPoints:0,
   ...part,directSolves:{first:directHz.length,second:count,total:directHz.length+count,ifAllDirect:count}}};
 };
 const report=first.report,n=report.referenceOhm.length;
 const at=(frequencyHz:number)=>report.frequenciesHz.indexOf(frequencyHz);
 if(report.frequenciesHz.length!==directHz.length||directHz.some(f=>at(f)<0)||report.parameters.length!==directHz.length)
  return fallback('direct solve did not return the requested fit and check frequencies');
 if(n<1||n>MAX_PORTS)return fallback('the rational fit supports 1 to '+MAX_PORTS+' ports, the channel has '+n);
 const api=(options.api??nativeRationalApi)();
 let fit:RationalFitResult;
 try{
  fit=api.fit({kind:'S',frequenciesHz:gridHz,samples:gridHz.map(f=>report.parameters[at(f)]),poles:'auto',maxPoles:MAX_POLES,iterations:30,fitD:true,enforcePassivity:true});
 }catch(error){
  return fallback('rational fit refused: '+(error instanceof Error?error.message:String(error)));
 }
 const passivity=fit.passivity;
 const summary={rmsRelativeError:fit.rmsRelativeError,maxRelativeError:fit.maxRelativeError,iterations:fit.iterations,
  passivity:{passive:passivity.passive,worstValue:passivity.worstValue,worstFrequencyHz:passivity.worstFrequencyHz,violations:passivity.violations.length},
  enforcement:fit.enforcement,poleSelection:fit.poleSelection};
 const common={poleCount:fit.model.poles.length,passive:fit.passive,fit:summary};
 const checkSamples=api.evaluate({model:fit.model,frequenciesHz:checkHz}).samples;
 const checks=checkHz.map((f,k)=>checkError(f,checkSamples[k],report.parameters[at(f)],report.referenceOhm,tolerance));
 const worst=Math.max(...checks.map(c=>c.worst));
 if(!fit.passive)return fallback('rational model is not passive'+(fit.passivityReason?' ('+fit.passivityReason+')':''),{...common,checks,worstCheckRelative:worst});
 const failed=checks.filter(c=>!c.passed);
 if(failed.length)return fallback('held-out error '+worst.toExponential(2)+' above '+tolerance+' at '+failed.map(c=>c.frequencyHz).join(', ')+' Hz',{...common,checks,worstCheckRelative:worst});
 const restHz=plan.rest.map(i=>frequenciesHz[i]);
 const restSamples=api.evaluate({model:fit.model,frequenciesHz:restHz}).samples;
 const rest=new Map(restHz.map((f,k)=>[f,restSamples[k]]));
 const parameters=frequenciesHz.map(f=>rest.get(f)??report.parameters[at(f)]);
 return {result:first,rational:{frequenciesHz,parameters,directFrequenciesHz:directHz},
  record:{...base,method:'rational',reason:null,checks,worstCheckRelative:worst,...common,
   directSolves:{first:directHz.length,second:0,total:directHz.length,ifAllDirect:count},model:fit.model,rationalPoints:restHz.length}};
}
