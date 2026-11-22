import test from 'node:test';
import assert from 'node:assert/strict';
import {transientStimulus,transientRequest,transientObservedVoltage} from '../src/analyze/analysis-dsl/transient-executor.ts';
import {networkResource} from '../src/analyze/analysis-dsl/frequency-model.ts';
import {createAnalysisApi} from '../src/analyze/analysis-dsl/context.ts';
import {sampleWaveform} from '../src/analyze/analysis-dsl/descriptions.ts';

const model={id:'c',kind:'network',ports:['p'],data:{format:'coupled-rlc',referenceTopology:'grounded',potentialNodes:['v'],currentDofs:[],incidence:[],portIncidence:[[0,0,1]],resistanceOhm:[],inductanceH:[],capacitanceF:[[0,0,1e-9]]}} as any;
test('bits encode absolute finite fronts; PRBS7 emits full period by declared convention',()=>{
 const waveform=transientStimulus({kind:'bits',bits:'010',repeatCount:2,bitRateBps:1e6,riseS:1e-7,fallS:2e-7},.2,3.2);
 assert.equal(sampleWaveform(waveform,0),.2);
 assert.ok(Math.abs(sampleWaveform(waveform,1.05e-6)-1.7)<1e-12);
 assert.equal(sampleWaveform(waveform,2.3e-6),.2);
 assert.equal(sampleWaveform(waveform,6e-6),.2);
 const stimulus={kind:'prbs',order:7,seed:127,bits:254,bitRateBps:1e6,riseS:1e-7,fallS:1e-7} as const;
 const w=transientStimulus(stimulus,0,1);
 const bits=Array.from({length:254},(_,i)=>sampleWaveform(w,(i+.5)*1e-6));
 assert.deepEqual(bits.slice(0,127),bits.slice(127));assert.equal(bits.slice(0,127).filter(Boolean).length,64);
 assert.throws(()=>transientStimulus({...stimulus,order:8},0,1),/orders/);
 assert.throws(()=>transientStimulus({...stimulus,riseS:0},0,1),/finite/);
});
test('public existing SI builder binds exact finite drivers/receivers and retains conditional fields',async()=>{
 let captured:any;
 const api=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},()=>{},async task=>{
  captured=task;const adapted=transientRequest(task as any);assert.deepEqual(adapted.names,['input']);
  assert.equal(adapted.request.drivers[0].resistanceOhm,50);assert.equal(adapted.request.receivers[0].capacitanceF,1e-12);
  return{id:'r',status:'complete',method:'injected adapter contract',diagnostics:[],assumptions:[],metrics:{'network-voltage:input':{kind:'series',unit:'V',axis:{name:'timeS',unit:'s',values:[0,1e-6]},values:[0,1],validity:{status:'conditional'},diagnostics:[]}}};
 });
 const a=api.globals.analysis(),n=a.network({model:networkResource(model),ports:{input:'p'}}),p=n.port('input');
 const result=await a.si().network(n).drivers([{port:p,model:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50},stimulus:{kind:'waveform',voltageV:{kind:'step',from:0,to:1,atS:0,riseS:1e-7}}}]).receivers([{port:p,model:{kind:'rc-input',capacitanceF:1e-12}}]).time({stopS:1e-6,sampleStepS:1e-8}).run();
 assert.equal(result.voltage(p).validity.status,'conditional');assert.deepEqual(result.voltage(p).values,[0,1]);
 assert.equal(result.differentialVoltage({positive:p,negative:p}).values?.[1],0);
 const wrong=structuredClone(captured);wrong.descriptors.find((d:any)=>d.id===p.id).input.owner='foreign';assert.throws(()=>transientRequest(wrong),/belong/);
 assert.equal(transientObservedVoltage(captured,{input:{owner:'foreign',name:'input'}},{}),undefined);
 const bad=structuredClone(model);bad.data.unknown=1;assert.throws(()=>networkResource(bad));
});
