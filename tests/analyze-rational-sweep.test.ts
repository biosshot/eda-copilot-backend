import test from 'node:test';
import assert from 'node:assert/strict';
import {checkError,nativeRationalApi,planRationalSweep,RATIONAL_MIN_POINTS,solveFrequencySweep,type Matrix} from '../src/analyze/rational-sweep.ts';

const axis=(count:number)=>Array.from({length:count},(_,i)=>1e4*(1e3)**(i/(count-1)));
const cx=(re:number,im:number)=>({real:re,imaginary:im});
const mul=(a:{real:number;imaginary:number},b:{real:number;imaginary:number})=>cx(a.real*b.real-a.imaginary*b.imaginary,a.real*b.imaginary+a.imaginary*b.real);
const div=(a:{real:number;imaginary:number},b:{real:number;imaginary:number})=>{const d=b.real**2+b.imaginary**2;return cx((a.real*b.real+a.imaginary*b.imaginary)/d,(a.imaginary*b.real-a.real*b.imaginary)/d);};
/** Lossy passive two-port: low-pass S21 = 0.8/(1+jf/f0), reflection S11 = 0.3 (jf/f0)/(1+jf/f0); optional narrow resonance on S21. */
function s2(f:number,resonanceHz?:number):Matrix{
 const x=cx(0,f/2e6),pole=cx(1,f/2e6);
 let s21=div(cx(0.8,0),pole);
 if(resonanceHz){const q=300,u=f/resonanceHz-1;s21=mul(s21,cx(1-0.2/(1+(q*u)**2),0.2*q*u/(1+(q*u)**2)*0.5));}
 const s11=mul(cx(0.3,0),div(x,pole));
 return [[s11,s21],[s21,s11]];
}
const reference=[50,50];
const direct=(resonanceHz?:number)=>{
 const calls:{frequenciesHz:number[];interval:readonly [number,number]|undefined}[]=[];
 const solve=(frequenciesHz:number[],interval:readonly [number,number]|undefined)=>{
  calls.push({frequenciesHz,interval});
  return {report:{frequenciesHz,parameters:frequenciesHz.map(f=>s2(f,resonanceHz)),referenceOhm:reference}};
 };
 return {calls,solve};
};

test('rational sweep plan: short axes stay direct, long axes get grid, held-out checks between grid points and a rest',()=>{
 assert.equal(planRationalSweep(RATIONAL_MIN_POINTS-1),null);
 for(const count of [RATIONAL_MIN_POINTS,25,40,128]){
  const plan=planRationalSweep(count)!;
  assert.equal(plan.grid.length,12);assert.equal(plan.checks.length,3);
  assert.equal(plan.grid[0],0);assert.equal(plan.grid.at(-1),count-1);
  assert.equal(new Set([...plan.grid,...plan.checks,...plan.rest]).size,count);
  assert.ok(plan.grid.length+plan.checks.length<count);
  for(const check of plan.checks)assert.ok(plan.grid.some(g=>g<check)&&plan.grid.some(g=>g>check),'check is strictly between grid points');
 }
});

test('rational sweep: axis shorter than the minimum is the unchanged direct call and has no record',()=>{
 const {calls,solve}=direct();
 const frequencies=axis(RATIONAL_MIN_POINTS-1);
 const out=solveFrequencySweep({frequenciesHz:frequencies,interval:[frequencies[0],frequencies.at(-1)!],solve});
 assert.equal(calls.length,1);assert.deepEqual(calls[0].frequenciesHz,frequencies);
 assert.equal(out.record,undefined);assert.equal(out.rational,undefined);
});

test('rational sweep: a rational channel passes the held-out checks, fewer solves, all points returned and recorded',()=>{
 const {calls,solve}=direct();
 const frequencies=axis(40);
 const out=solveFrequencySweep({frequenciesHz:frequencies,interval:[frequencies[0],frequencies.at(-1)!],solve});
 assert.equal(calls.length,1);
 assert.equal(calls[0].frequenciesHz.length,15);assert.equal(calls[0].interval,undefined);
 const record=out.record!;
 assert.equal(record.method,'rational',String(record.reason));
 assert.equal(record.reason,null);
 assert.equal(record.directSolves.total,15);assert.ok(record.directSolves.total<frequencies.length);
 assert.equal(record.fitGridHz.length,12);assert.equal(record.checkPointsHz.length,3);assert.equal(record.checks.length,3);
 assert.ok(record.worstCheckRelative!<1e-3);assert.equal(record.passive,true);assert.ok(record.poleCount!>=1);
 assert.equal(record.rationalPoints,40-15);
 const rational=out.rational!;
 assert.deepEqual(rational.frequenciesHz,frequencies);assert.equal(rational.parameters.length,40);
 // Direct values are kept at the solved frequencies, the model supplies the others within the tolerance.
 rational.frequenciesHz.forEach((f,i)=>{
  const exact=s2(f),got=rational.parameters[i];
  const error=checkError(f,got,exact,reference);
  if(rational.directFrequenciesHz.includes(f))assert.equal(error.worst,0);else assert.ok(error.worst<5e-3,'model point at '+f+': '+error.worst);
 });
});

test('rational sweep: a resonance the grid misses fails the held-out check and the whole axis is solved as without the model',()=>{
 const frequencies=axis(40),plan=planRationalSweep(40)!;
 const resonanceHz=frequencies[plan.checks[1]];
 const {calls,solve}=direct(resonanceHz);
 const interval=[frequencies[0],frequencies.at(-1)!] as const;
 const out=solveFrequencySweep({frequenciesHz:frequencies,interval,solve});
 assert.equal(calls.length,2);
 assert.equal(calls[0].frequenciesHz.length,15);
 // The fallback call is exactly the call a caller without the model makes.
 assert.deepEqual(calls[1].frequenciesHz,frequencies);assert.deepEqual(calls[1].interval,interval);
 assert.equal(out.rational,undefined);
 assert.deepEqual(out.result.report.parameters,frequencies.map(f=>s2(f,resonanceHz)));
 const record=out.record!;
 assert.equal(record.method,'direct-fallback');
 assert.match(record.reason!,/held-out error|not passive|refused/);
 assert.ok(record.checks.some(c=>!c.passed)||record.passive===false);
 assert.deepEqual(record.directSolves,{first:15,second:40,total:55,ifAllDirect:40});
});

test('rational sweep: a refused fit and a non-passive model fall back with the reason',()=>{
 const frequencies=axis(30);
 const refuse=direct();
 const refused=solveFrequencySweep({frequenciesHz:frequencies,interval:[frequencies[0],frequencies.at(-1)!],solve:refuse.solve,
  api:()=>({fit:()=>{throw new Error('RATIONAL_FIT_NOT_CONVERGED: test');},evaluate:nativeRationalApi().evaluate})});
 assert.equal(refuse.calls.length,2);assert.equal(refused.record!.method,'direct-fallback');assert.match(refused.record!.reason!,/rational fit refused: RATIONAL_FIT_NOT_CONVERGED/);
 const real=nativeRationalApi();
 const passive=direct();
 const notPassive=solveFrequencySweep({frequenciesHz:frequencies,interval:[frequencies[0],frequencies.at(-1)!],solve:passive.solve,
  api:()=>({evaluate:real.evaluate,fit:request=>({...real.fit(request),passive:false,passivityReason:'forced for the test'})})});
 assert.equal(passive.calls.length,2);assert.match(notPassive.record!.reason!,/not passive \(forced for the test\)/);
});

test('rational sweep: check error is per entry (small entry), plus port voltage/current and series impedance',()=>{
 const base:Matrix=[[cx(3e-5,0),cx(1,0)],[cx(1,0),cx(3e-5,0)]];
 // A 1e-4 absolute shift of the tiny S11 is invisible to a Frobenius norm but is 3.3x its own magnitude.
 const shifted:Matrix=[[cx(3e-5+1e-4,0),cx(1,0)],[cx(1,0),cx(3e-5,0)]];
 const small=checkError(1e5,shifted,base,reference);
 assert.ok(small.entryRelative>3&&!small.passed);assert.deepEqual(small.worstEntry,[0,0]);
 // A 4e-4 shift of S21 ~ 1 passes the entry test but series Z = 2 Z0 (1/S21 - 1) is milliohms.
 const s21:Matrix=[[cx(3e-5,0),cx(1.0004,0)],[cx(1.0004,0),cx(3e-5,0)]];
 const series=checkError(1e5,s21,[[cx(3e-5,0),cx(0.9999,0)],[cx(0.9999,0),cx(3e-5,0)]],reference);
 assert.ok(series.entryRelative<1e-3);assert.ok(series.seriesImpedanceRelative!>1);assert.equal(series.passed,false);
 const same=checkError(1e5,base,base,reference);
 assert.equal(same.worst,0);assert.equal(same.passed,true);
 assert.equal(checkError(1e5,[[cx(0.5,0)]],[[cx(0.5,0)]],[50]).seriesImpedanceRelative,null);
});
