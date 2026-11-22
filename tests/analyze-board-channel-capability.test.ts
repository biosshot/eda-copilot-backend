import test from 'node:test';
import assert from 'node:assert/strict';
import {channelPassiveElement} from '../src/analyze/channel-components.ts';
import {createBoardChannelExecutor} from '../src/analyze/board-channel-executor.ts';
const budget={maxSeconds:5,maxMemoryMiB:256};
for(const frequency of [9000,10000,20000])test(`geometry capability admits segmented ${frequency}Hz without a fixed threshold`,()=>{
 const stop=new Error('segmented-selected');let planned=0;
 const planner:any=()=> (options:any)=>{assert.ok(!('closedJointDeadlineMs' in options));planned++;return {blocks:[{bandHz:[1000,100000]}]};};
 const run=createBoardChannelExecutor(planner,(_p,axis)=>{assert.ok(axis.includes(frequency));assert.ok(axis.includes(100000));throw stop;},()=>assert.fail('joint selected'));
 assert.throws(()=>run({}, {}, [],[frequency],budget),e=>e===stop);assert.equal(planned,1);
});
test('narrow block capability selects joint only after geometry planning',()=>{
 const order:string[]=[];const foil:any={marker:true};
 const planner:any=()=> (options:any)=>{if('closedJointDeadlineMs' in options){order.push('joint');return foil;}order.push('geometry');return {blocks:[{bandHz:[1000,10000]}]};};
 const run=createBoardChannelExecutor(planner,()=>assert.fail('segmented selected'),p=>{assert.equal(p,foil);return 'joint';});
 assert.equal(run({}, {}, [],[20000],budget),'joint');assert.deepEqual(order,['geometry','joint']);
});
test('unavailable segmented band refuses before solve without joint producer',()=>{
 const planner:any=()=>()=>({blocks:[{bandHz:[1000,10000]}]});
 const run=createBoardChannelExecutor(planner,()=>assert.fail('solve selected'));
 assert.throws(()=>run({}, {}, [],[20000],budget),/CHANNEL_MODEL_BAND/);
});

test('malformed frequency demand is rejected before creating a geometry planner',()=>{
 const run=createBoardChannelExecutor(()=>assert.fail('geometry created'),()=>assert.fail('solve'),()=>assert.fail('joint'));
 for(const axis of [[NaN],[Infinity],[-1]])assert.throws(()=>run({}, {}, [],axis,budget),/CHANNEL_FREQUENCY_INPUT/);
 for(const interval of [[NaN,1],[0,Infinity],[-1,1],[2,1],[1]] as any[])
  assert.throws(()=>run({}, {}, [],[1000],budget,interval),/CHANNEL_FREQUENCY_INPUT/);
});

test('component interval gaps reach joint audit without discarding the full demand',()=>{
 const interval=[0,1e6] as const;
 const contacts:any={components:[{model:channelPassiveElement('load',{kind:'resistor',resistanceOhm:1},[1000,30000])}]};
 const planner:any=()=> (options:any)=>'closedJointDeadlineMs' in options?{joint:true}:{blocks:[{bandHz:[1000,10000]}]};
 const run=createBoardChannelExecutor(planner,()=>assert.fail('segmented'),(_p,axis,_budget,demand)=>{
  assert.deepEqual(axis,[20000]);assert.deepEqual(demand,interval);return 'audited-joint';
 });
 assert.equal(run({}, {}, contacts,[20000],budget,interval),'audited-joint');
 const noPlan=createBoardChannelExecutor(()=>assert.fail('geometry'),()=>assert.fail('solve'),()=>assert.fail('joint'));
 assert.throws(()=>noPlan({}, {}, contacts,[40000],budget,interval),/CHANNEL_COMPONENT_BAND/);
});

test('producer envelope bypasses segmentation-specific refusals only outside its capability',()=>{
 let segmented=0,joint=0;
 const create:any=()=>Object.assign((options:any)=>{
  if('closedJointDeadlineMs' in options){joint++;return {joint:true};}
  segmented++;throw new Error('CHANNEL_SEGMENTATION: Low-band ribbon requires thin conductor');
 },{segmentedCapabilityBandHz:Object.freeze([1000,10000])});
 const run=createBoardChannelExecutor(create,()=>assert.fail('solve'),()=> 'joint');
 assert.equal(run({}, {}, [],[20000],budget),'joint');assert.equal(segmented,0);assert.equal(joint,1);
 assert.throws(()=>run({}, {}, [],[10000],budget),/Low-band ribbon/);assert.equal(segmented,1);assert.equal(joint,1);
});
test('invalid producer metadata refuses instead of hiding planner faults with joint fallback',()=>{
 for(const band of [[1000,NaN],[0,10000],[20000,1000],[1000]]){
  const create:any=()=>Object.assign(()=>assert.fail('planner called'),{segmentedCapabilityBandHz:band});
  const run=createBoardChannelExecutor(create,()=>assert.fail('solve'),()=>assert.fail('joint'));
  assert.throws(()=>run({}, {}, [],[20000],budget),/invalid segmented producer capability/);
 }
});
