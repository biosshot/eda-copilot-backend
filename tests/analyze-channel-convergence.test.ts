import test from'node:test';import assert from'node:assert/strict';import {channelResponseChange}from'../src/analyze/board-channel-executor.ts';
const response=(x:number)=>({ports:[{name:'in',index:0}],frequenciesHz:[1000],parameters:[[[{real:x,imaginary:0}]]]});
test('assembled channel convergence preserves original power-wave and Cu loss2% screens',()=>{
 const a=response(.9),b=response(.91),r=channelResponseChange(a,b);assert.equal(r.waveAbsoluteAmplitudeFloor,.01);assert.equal(r.lossAbsoluteFloorW,1e-18);assert.ok(r.copperLossRelativeChange>.02);assert.ok(r.maxRelativeChange>.02);
 const c=response(.9);c.frequenciesHz=[2000];assert.throws(()=>channelResponseChange(a,c),/axis changed/);
});
test('whole-channel delay gate detects errors invisible in low-band complex S',()=>{
 const r=(delay:number)=>({ports:[{name:'in',index:0},{name:'out',index:1}],frequenciesHz:[1000,10000],parameters:[1000,10000].map(f=>{const v={real:Math.cos(2*Math.PI*f*delay),imaginary:-Math.sin(2*Math.PI*f*delay)},z={real:0,imaginary:0};return [[z,v],[v,z]];})});
 const change=channelResponseChange(r(80e-12),r(70e-12));assert.ok(change.waveRelativeChange<1e-5);assert.ok(change.groupDelayRelativeChange>.14);assert.ok(change.maxRelativeChange>.02);
});
