import test from 'node:test';
import assert from 'node:assert/strict';
import {electricalNominal,electricalModel} from '../src/analyze/analysis-dsl/electrical-models.ts';
import {electricalCircuit} from '../src/analyze/analysis-dsl/electrical-circuit.ts';
import {circuitObservedVoltage,circuitObservedTransfer} from '../src/analyze/analysis-dsl/electrical-executor.ts';
import {createAnalysisApi} from '../src/analyze/analysis-dsl/context.ts';

const circuit={nodes:['in','out','gnd'],reference:'gnd',ports:[{id:'in',positive:'in',negative:'gnd'},{id:'out',positive:'out',negative:'gnd'}],elements:[{id:'r',kind:'resistor',nodes:['in','out'],resistanceOhm:1000}]} as any;
const p=(name:string)=>({kind:'port',id:name,contextId:'a'});
const task=(config:any={})=>({kind:'pi',contextId:'a',config:{circuit:{kind:'circuit',id:'c',contextId:'a'},...config},variantChanges:[],descriptors:[{id:'c',input:circuit},...['in','out'].map(id=>({id,input:{owner:'c',name:id}}))]}) as any;

test('nominal units, explicit precedence, missing parasitics and exact PartUuid resources',()=>{
 for(const [text,kind,value]of [['4k7','resistor',4700],['2R2','resistor',2.2],['100 nF','capacitor',1e-7],['2.2uH','inductor',2.2e-6],['1e3 Ohm','resistor',1000]] as const)assert.ok(Math.abs(electricalNominal(text,kind)-value)<=Math.abs(value)*1e-14);
 for(const text of ['10%','10k 0603','100V','0','-1','1e999','10mA'])assert.throws(()=>electricalNominal(text,'resistor'));
 assert.throws(()=>electricalNominal('104','capacitor'),/Ambiguous/);
 const context={board:{components:[{designator:'R1',value:'4k7',partUuid:'abc'}]},electricalModels:{'lcsc:abc':{kind:'resistor',resistanceOhm:999}}} as any;
 assert.equal((electricalModel({kind:'resistor',designator:'R1',resistanceOhm:100},context).model as any).resistanceOhm,100);
 assert.equal((electricalModel({kind:'resistor',designator:'R1'},context).model as any).resistanceOhm,4700);
 delete context.board.components[0].value;
 assert.equal((electricalModel({kind:'resistor',designator:'R1'},context).model as any).resistanceOhm,999);
 delete context.electricalModels['lcsc:abc'];
 assert.throws(()=>electricalModel({kind:'resistor',designator:'R1'},context),/Missing/);
 assert.equal((electricalModel({kind:'capacitor',designator:'C1',capacitanceF:1e-9,ideal:true},{}).model as any).ideal,true);
 assert.throws(()=>electricalModel({kind:'capacitor',designator:'C1',capacitanceF:1e-9},{}),/parasitics/);
 assert.throws(()=>electricalModel({kind:'resistor',designator:'R1',resistanceOhm:1,model:{kind:'resistor',resistanceOhm:2}},{}),/Conflicting/);
});

test('circuit spectra are explicit, source/load ports remain owned, finite driver and RC input expand',()=>{
 for(const[kind,denied]of [['dc','pi'],['pi','dc']]as const){
  const current=task();current.kind=kind;
  const c={...circuit,elements:[{id:'r',kind:'model',model:{id:'restricted',kind:'electrical',analyses:[denied],data:{kind:'resistor',resistanceOhm:1}},pins:{p:'in',n:'out'}}]};
  assert.throws(()=>electricalCircuit(current,c as any,kind==='dc'?'dc':'frequency',[1000]),/does not support/);
  const driven={...circuit,elements:[{id:'v',kind:'voltage-source',nodes:['in','gnd'],voltageV:0,model:{id:'restricted-driver',kind:'electrical',analyses:[denied],data:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50}}}]};
  assert.throws(()=>electricalCircuit(current,driven as any,kind==='dc'?'dc':'frequency',[1000]),/does not support/);
 }
 const source={port:p('in'),model:{kind:'linear-driver',lowV:0,highV:3.3,outputResistanceOhm:50},voltageSpectrum:{frequencyHz:[1000,2000],values:[{real:1,imaginary:.5},{real:2,imaginary:-.5}],unit:'V'}};
 const load={port:p('out'),model:{kind:'rc-input',capacitanceF:1e-12,resistanceOhm:10000}};
 const r=electricalCircuit(task({sources:[source],loads:[load]}),circuit,'frequency',[1000,2000]);
 assert.equal(r.circuit.nodes.length,4);assert.equal(r.circuit.elements.length,5);
 assert.deepEqual(r.acSources,[{elementId:'source:0',values:source.voltageSpectrum.values}]);
 assert.equal(r.circuit.elements.find(e=>e.id==='source:0:output')?.resistanceOhm,50);
 assert.equal(r.circuit.elements.find(e=>e.id==='load:0:R')?.resistanceOhm,10000);
 assert.throws(()=>electricalCircuit(task({sources:[{port:p('in'),model:'ideal',voltageV:1}]}),circuit,'frequency',[1000]),/Spectrum/);
 assert.throws(()=>electricalCircuit(task({sources:[source]}),circuit,'frequency',[1000,3000]),/axis/);
 assert.throws(()=>electricalCircuit(task({sources:[{...source,voltageSpectrum:{...source.voltageSpectrum,unit:'A'}}]}),circuit,'frequency',[1000,2000]),/unit/);
 assert.throws(()=>electricalCircuit(task({sources:[{...source,port:p('foreign')}]}),circuit,'frequency',[1000,2000]),/port/);
 assert.throws(()=>electricalCircuit(task({loads:[{port:p('out'),powerW:1}]}),circuit,'time'),/constant power/);
});

test('approved existing sources/loads and component descriptors survive prepare without native work',async()=>{
 let seen:any;
 const api=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},()=>{},async t=>{seen=t;return{id:'accepted',status:'complete',method:'test transport only',diagnostics:[],assumptions:[],metrics:{}};});
 const a=api.globals.analysis();
 const c=a.circuit({...circuit,elements:[{id:'r',kind:'model',model:api.globals.resistor({designator:'R1',resistanceOhm:1e3}),pins:{p:'in',n:'out'}}]});
 const builder=a.pi().circuit(c).domain('frequency').frequencyHz(1000).sources([{port:c.port('in'),model:'ideal',voltageSpectrum:{frequencyHz:[1000],values:[{real:1,imaginary:0}],unit:'V'}}]).loads([{port:c.port('out'),resistanceOhm:1000}]);
 assert.equal((await builder.prepare()).structurallyValid,true);
 await builder.run();assert.equal(seen.config.sources[0].voltageSpectrum.values[0].real,1);
});

test('complex observed voltage/transfer preserve values, null denominators and source ambiguity',()=>{
 const t=task(),axis={name:'frequencyHz',unit:'Hz',values:[1,2]},data=(values:any)=>({kind:'series',unit:'V',validity:{status:'valid'},diagnostics:[],axis,values});
 const metrics:any={
  'circuit-node:in':data([{real:1,imaginary:1},{real:0,imaginary:0}]),
  'circuit-node:out':data([{real:1,imaginary:0},{real:1,imaginary:0}]),
  'circuit-node:gnd':data([{real:0,imaginary:0},{real:0,imaginary:0}]),
  'circuit-ac-source-count':{kind:'scalar',unit:'1',value:1,validity:{status:'valid'},diagnostics:[]},
 };
 assert.deepEqual(circuitObservedVoltage(t,t.descriptors[1],metrics)?.values,[{real:1,imaginary:1},{real:0,imaginary:0}]);
 const transfer=circuitObservedTransfer(t,{from:p('in'),to:p('out')},metrics);
 assert.deepEqual(transfer?.values,[{real:.5,imaginary:-.5},null]);assert.equal(transfer?.validity.status,'conditional');
 metrics['circuit-ac-source-count'].value=2;assert.equal(circuitObservedTransfer(t,{from:p('in'),to:p('out')},metrics),undefined);
});
