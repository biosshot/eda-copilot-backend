import {channelPassiveElement,type BoardChannelRequest} from '../src/analyze/channel-components.ts';
import test from'node:test';import assert from'node:assert/strict';import {channelResponseChange,createBoardChannelExecutor}from'../src/analyze/board-channel-executor.ts';
const response=(x:number)=>({ports:[{name:'in',index:0}],frequenciesHz:[1000],parameters:[[[{real:x,imaginary:0}]]]});
test('assembled channel convergence preserves original power-wave and Cu loss2% screens',()=>{
 const a=response(.9),b=response(.91),r=channelResponseChange(a,b);assert.equal(r.waveAbsoluteAmplitudeFloor,.01);assert.equal(r.lossAbsoluteFloorW,1e-18);assert.ok((r.copperLossRelativeChange??0)>.02);assert.ok(r.maxRelativeChange>.02);
 const c=response(.9);c.frequenciesHz=[2000];assert.throws(()=>channelResponseChange(a,c),/axis changed/);
});
test('whole-channel delay gate detects errors invisible in low-band complex S',()=>{
 const r=(delay:number)=>({ports:[{name:'in',index:0},{name:'out',index:1}],frequenciesHz:[1000,10000],parameters:[1000,10000].map(f=>{const v={real:Math.cos(2*Math.PI*f*delay),imaginary:-Math.sin(2*Math.PI*f*delay)},z={real:0,imaginary:0};return [[z,v],[v,z]];})});
 const change=channelResponseChange(r(80e-12),r(70e-12));assert.ok(change.waveRelativeChange<1e-5);assert.ok(change.groupDelayRelativeChange>.14);assert.ok(change.maxRelativeChange>.02);
});

const convergenceAxis = [1000, Math.sqrt(1e7), 10000];
const budget = { maxSeconds: 12, maxMemoryMiB: 256 };

function mockChannel(energy: Record<string, number>) {
 const captured:string[]=[];
 const replays:{id:string;frequencyHz:number}[]=[];
 const calls: { id: string; frequencies: number[]; remainingSeconds: number }[] = [];
 const solve = createBoardChannelExecutor(
  () => ({domainScale, magneticPatchLevel, boundaryFraction} = {}) => {
   const stage = magneticPatchLevel === 2 ? 'fine' : boundaryFraction === 1/4 ? 'shifted' : 'coarse';
   const id = domainScale + '-' + stage;
   const cut = {positive: id + '-positive', negative: id + '-negative'};
   return {
    blocks: [{id, kind: 'local', cuts: [cut], bandHz: [1000,10000], ownership: [], boardJson: '{}', geometry: {}}],
    ports: [{name: 'in', cut}], coverage: [], audit: {id, domainScale, magneticPatchLevel, boundaryFraction},
   };
  },
  (plan, frequenciesHz, remaining = budget, options) => {
   const { id } = plan.blocks[0];
   assert.ok(Object.hasOwn(energy,id), 'unexpected solve ' + id);
   assert.equal(remaining.maxMemoryMiB, budget.maxMemoryMiB);
   calls.push({id, frequencies: [...frequenciesHz], remainingSeconds: remaining.maxSeconds});
   const result = {
    ...response(.9), frequenciesHz, status: 'complete' as const, diagnostics: [], elapsedMs: 0, referenceOhm: [50],
    parameters: frequenciesHz.map(() => response(.9).parameters[0]),
    loadedEnergyGram: frequenciesHz.map(() => ({magneticJPerA2: [[energy[id] * 1e-10]]})),
    candidateId: id,
   };
   if(options?.captureFrequencyReplay){
    captured.push(id);
    options.captureFrequencyReplay((frequencyHz,maxSeconds)=>{
     assert.ok(maxSeconds>0&&maxSeconds<=remaining.maxSeconds);
     replays.push({id,frequencyHz});
     return {...result,frequenciesHz:[frequencyHz],parameters:response(.9).parameters};
    });
   }
   return { result, evidence: [id], bandHz: [1000,10000], magneticCoupling: null,
    ownership: [{id, ownership: []}], wallSeconds: 0 };
  },
 );
 return {calls,captured,replays, run: (frequencies: number[], request:BoardChannelRequest|[] = [], band?:readonly [number,number]) => solve({}, {}, request, frequencies, budget,band)};
}

const laterPass = {'1-coarse': 1, '2-coarse': 1.055, '4-coarse': 1.07, '4-fine': 1.08, '4-shifted': 1.073};

test('after the first domain comparison fails the returned fine solve and all evidence use domain 4', t => {
 let time = 0;
 t.mock.method(performance, 'now', () => time += 100);
 const {calls,run} = mockChannel(laterPass);
 const requested = [2000, 5000];
 const output = run(requested);
 assert.deepEqual(calls.map(call => call.id), ['1-coarse','2-coarse','4-coarse','4-fine','4-shifted','4-fine']);
 assert.ok(calls.every((call,i) => call.remainingSeconds > 0 && call.remainingSeconds < (calls[i-1]?.remainingSeconds ?? budget.maxSeconds)));
 assert.deepEqual(calls.at(-1)?.frequencies, requested);
 assert.deepEqual(output.result.frequenciesHz, requested);
 assert.deepEqual(output.evidence, ['4-fine']);
 assert.equal(output.audit.id, '4-fine');
 assert.deepEqual(output.blocks.map(block => block.id), ['4-fine']);
 assert.deepEqual(output.ownership.map(block => block.id), ['4-fine']);
 const [, smaller, accepted] = output.verification;
 assert.ok(smaller.domain!.maxRelativeChange > .02);
 assert.equal(smaller.spatial, undefined);
 assert.equal(smaller.boundary, undefined);
 assert.equal(accepted.domainScale, 4);
 assert.ok(accepted.domain!.maxRelativeChange <= .02);
 assert.ok(accepted.spatial!.maxRelativeChange <= .02);
 assert.deepEqual(accepted.runs.map(run => run.evidence), [['4-coarse'], ['4-fine']]);
 assert.deepEqual(accepted.boundary!.evidence, ['4-shifted']);
});

test('the first passing domain pair returns the larger fine result without a redundant solve', () => {
 const {calls,run} = mockChannel({'1-coarse': 1, '2-coarse': 1.01, '2-fine': 1.014, '2-shifted': 1.012});
 const output = run(convergenceAxis);
 assert.deepEqual(calls.map(call => call.id), ['1-coarse','2-coarse','2-fine','2-shifted']);
 assert.deepEqual(output.evidence, ['2-fine']);
 assert.equal(output.audit.domainScale, 2);
 assert.equal(output.verification.at(-1)?.domainScale, 2);
});

test('the larger domain must pass its own spatial and shifted-boundary checks', () => {
 const spatial = mockChannel({...laterPass,'4-fine': 1.14});
 assert.throws(() => spatial.run(convergenceAxis), /CHANNEL_SPATIAL_CONVERGENCE/);
 assert.deepEqual(spatial.calls.map(call => call.id), ['1-coarse','2-coarse','4-coarse','4-fine']);

 const boundary = mockChannel({...laterPass,'4-shifted': 1.14});
 assert.throws(() => boundary.run(convergenceAxis), /CHANNEL_BOUNDARY_CONVERGENCE/);
 assert.deepEqual(boundary.calls.map(call => call.id), ['1-coarse','2-coarse','4-coarse','4-fine','4-shifted']);
});

test('component verification uses only common-band frequencies and labels total passive loss',()=>{
 const request:BoardChannelRequest={signals:[],references:[],ports:[],components:[{
  id:'component:R1',designator:'R1',
  pins:[{id:'a',sourcePath:'pads[0]',layer:'TOP'},{id:'b',sourcePath:'pads[1]',layer:'TOP'}],
  model:channelPassiveElement('R1',{kind:'resistor',resistanceOhm:1},[2000,8000]),
 }]};
 const {calls,run}=mockChannel({'1-coarse':1,'2-coarse':1.01,'2-fine':1.014,'2-shifted':1.012});
 assert.throws(()=>run([1000,3000],request),/CHANNEL_COMPONENT_BAND/);
 assert.equal(calls.length,0);
 const result=run([3000,5000],request);
 assert.deepEqual(calls[0].frequencies,[2000,3000,4000,5000,8000]);
 assert.deepEqual(calls.at(-1)?.frequencies,[3000,5000]);
 const {domain}=result.verification.at(-1)!;
 assert.equal(domain?.copperLossRelativeChange,undefined);
 assert.equal(domain?.totalPassiveLossRelativeChange,0);
 assert.match(domain!.lossRecovery,/Cu-only and regional convergence remain unverified/);
});
test('audits only the returned fine composition and preserves the fixed output grid', () => {
 const {calls,captured,replays,run}=mockChannel(laterPass);
 const output=run([2000,5000],[],[2000,5000]);
 assert.deepEqual(captured,['4-fine']);
 assert.deepEqual(calls.at(-1)?.frequencies,[2000,5000]);
 assert.ok(replays.every(replay=>replay.id==='4-fine'));
 assert.deepEqual(replays.map(replay=>replay.frequencyHz),[2000,5000,3500,2750,4250]);
 assert.deepEqual(output.result.frequenciesHz,[2000,5000]);
 assert.equal(output.frequencyAudit?.interpolation.status,'validated');
 assert.equal(output.frequencyAudit?.physicalCoverage.status,'unverified');
});
