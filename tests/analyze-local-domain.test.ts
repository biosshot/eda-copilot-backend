import test from 'node:test';
import assert from 'node:assert/strict';
import {domainChange} from '../src/analyze/analysis-dsl/local-domain.ts';
const response=(real:number[],imaginary:number[],loss=1e-3)=>({samples:[{frequencyHz:1e4,drive:[1,0],portRealV:real,portImaginaryV:imaginary,totalLossW:loss}]});
test('environment gate detects phase changes even when port magnitudes match',()=>{
 const change=domainChange(response([Math.cos(.03)],[Math.sin(.03)]),response([1],[0]));
 assert.ok(change.voltageRelativeChange>.02);assert.equal(change.totalLossRelativeChange,0);
});
test('declared weak-transfer floor handles a response zero without hiding significant loss changes',()=>{
 const change=domainChange(response([1,1e-10],[0,0],1.03e-3),response([1,0],[0,0]));
 assert.ok(change.voltageRelativeChange<1e-6);assert.ok(change.totalLossRelativeChange>.02);
});
test('environment comparisons refuse changed physical drives or verification axes',()=>{
 const a=response([1],[0]),b=response([1],[0]);b.samples[0].drive=[0,1];assert.throws(()=>domainChange(a,b),/mapping changed/);
 b.samples[0].drive=[1,0];b.samples[0].frequencyHz=2e4;assert.throws(()=>domainChange(a,b),/mapping changed/);
});
