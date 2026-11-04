import test from 'node:test';
import assert from 'node:assert/strict';
import { channelLossGrams } from '../src/analyze/channel-loss.ts';
import { channelResponseChange } from '../src/analyze/board-channel-executor.ts';

const report=()=>({
 instanceIds:['package','left','right'],
 convention:'Native matched intrinsic R',
 samples:[{frequencyHz:3000,oppositeDriveW:[4,8,12],jouleWPerA2:[
  [[1,-1],[-1,1]],
  [[2,-2],[-2,2]],
  [[3,-3],[-3,3]],
 ]}],
});

test('owner mapping separates package resistance from Cu for arbitrary drive superposition',()=>{
 const [group]=channelLossGrams(report(),[3000],2,['left','right'],['package']);
 assert.deepEqual(group.copperWPerA2,[[5,-5],[-5,5]]);
 assert.deepEqual(group.componentWPerA2,[[1,-1],[-1,1]]);
 assert.equal(group.oppositeCopperW,20);
 assert.equal(group.oppositeComponentW,4);
 assert.throws(()=>channelLossGrams(report(),[4000],2,['left','right'],['package']),/physical frequency axis/);
 assert.throws(()=>channelLossGrams(report(),[3000],2,['left','missing'],['package']),/instance ownership/);
 const asymmetric=report();asymmetric.samples[0].jouleWPerA2[1][0][1]=7;
 assert.throws(()=>channelLossGrams(asymmetric,[3000],2,['left','right'],['package']),/finite and symmetric/);
});

test('Cu-only convergence can refuse equal total passive loss after package substitution',()=>{
 const response=(copper:number,packageLoss:number)=>({
  ports:[{name:'in',index:0}], frequenciesHz:[3000], referenceOhm:[50],
  parameters:[[[{real:.5,imaginary:0}]]],
  channelLossGrams:[{frequencyHz:3000,copperWPerA2:[[copper]],componentWPerA2:[[packageLoss]],oppositeCopperW:copper,oppositeComponentW:packageLoss}],
 });
 const a=response(2,7.375), b=response(3,6.375);
 const change=channelResponseChange(a,b,true);
 assert.equal(change.totalPassiveLossRelativeChange,0);
 assert.equal(change.copperLossRelativeChange,1/3);
 assert.equal(change.maxRelativeChange,1/3);
 assert.throws(()=>channelResponseChange(a,{...b,channelLossGrams:undefined},true),/incomplete matched loss axis/);
});

test('opposite-drive gate uses recovered loss even if rounded Gram entries cancel',()=>{
 const zero={real:0,imaginary:0};
 const response=(oppositeCopperW:number)=>({
  ports:[{name:'left',index:0},{name:'right',index:1}],frequenciesHz:[3000],referenceOhm:[50,50],
  parameters:[[[zero,zero],[zero,zero]]],
  channelLossGrams:[{frequencyHz:3000,copperWPerA2:[[1,1],[1,1]],
   componentWPerA2:[[0,0],[0,0]],oppositeCopperW,oppositeComponentW:0}],
 });
 const change=channelResponseChange(response(2e-20),response(2e-18),true);
 assert.ok(Math.abs(change.copperLossRelativeChange!-.99)<1e-14);
 assert.ok(Math.abs(change.maxRelativeChange-.99)<1e-14);
});
