import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createAnalysisApi} from '../src/analyze/analysis-dsl/context.ts';
import {transientRequest} from '../src/analyze/analysis-dsl/transient-executor.ts';
import {networkResource} from '../src/analyze/analysis-dsl/frequency-model.ts';
import {auditElectricalPhysical,physicalCases} from './helpers/electrical-physical-audit.ts';
const model:any={id:'full',kind:'network',source:'Author supplied full conductors',analyses:['si','networkParameters'],ports:['join','out'],data:{format:'coupled-rlc',referenceTopology:'infinity',potentialNodes:['p','n'],currentDofs:[],incidence:[],portIncidence:[[0,0,1],[1,0,-1],[0,1,1],[1,1,-1]],resistanceOhm:[],inductanceH:[],capacitanceF:[[0,0,3e-9],[0,1,-2e-9],[1,0,-2e-9],[1,1,3e-9]],portTerminals:[{positive:{node:'p'},negative:{node:'n'}},{positive:{node:'p'},negative:{node:'n'}}]}};
test('public physical contacts and bounded initial state reach existing native request with original sources',async()=>{
 let request:any,captured:any;
 const api=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},()=>{},async task=>{
  captured=task;request=transientRequest(task as any);
  return{id:'physical-result',status:'complete',method:'adapter contract only',diagnostics:[],assumptions:[],metrics:{'network-voltage:a':{kind:'series',unit:'V',values:[1,1],axis:{name:'timeS',unit:'s',values:[0,1e-6]},validity:{status:'valid'},diagnostics:[]}}};
 });
 const a=api.globals.analysis(),left=a.network({model:networkResource(model),ports:{join:'join',out:'out'}}),right=a.network({model:networkResource(model),ports:{join:'join',out:'out'}});
 const n=a.connect({networks:[left,right],connections:[{from:left.port('join'),to:right.port('join'),contacts:'positive'}],ports:{a:left.port('out'),b:right.port('out')}});
 const initial={kind:'physical-state' as const,steady:false,potentialsV:[left,right].flatMap(x=>[{instanceId:x.id,node:'p',value:2},{instanceId:x.id,node:'n',value:1}]),currentsA:[]};
 await a.si().network(n).initial(initial).time({stopS:1e-6,sampleStepS:1e-8}).numerics({maxStepS:1e-9}).run();
 assert.deepEqual(request.request.topology.connectionContacts,['positive']);assert.deepEqual(request.request.initial,initial);assert.equal(request.request.numerics.maxStepS,1e-9);
 assert.deepEqual(request.request.topology.instances[0].model.data,model.data);assert.equal(request.sourceResources.length,2);assert.deepEqual(request.portMapping.connectionContacts,['positive']);
 const denied=structuredClone(captured);denied.descriptors.find((d:any)=>d.id===left.id).input.model.analyses=['networkParameters'];assert.throws(()=>transientRequest(denied),/authorize SI/);
 const bad=structuredClone(model);bad.data.portTerminals[0].negative={reference:'infinity'};assert.throws(()=>networkResource(bad));
 bad.data.portTerminals[0].negative={node:'n',extra:true};assert.throws(()=>networkResource(bad));
});
test('saved physical audit independently rejects charge, physical ground, reference, provenance and energy corruption',async(t)=>{
 let result:any;try{result=JSON.parse(await readFile(process.env.P6_PHYSICAL_RESULT??'debugging/p6-physical-verification/direct-dsl/standalone/result.json','utf8')).result;}catch{t.skip('Generate physical saved evidence first');return;}
 assert.equal(auditElectricalPhysical(result).status,'pass');
 const solution=(r:any,name='both')=>physicalCases(r).get(name).solution;
 for(const corrupt of [
  (r:any)=>solution(r).conductorChargesC[5][0]+=1e-9,
  (r:any)=>solution(r).conductorPotentialsV[5][0]+=.1,
  (r:any)=>solution(r).physicalRecovery.potentials[0].index=null,
  (r:any)=>solution(r).axis[3]=solution(r).axis[2],
  (r:any)=>solution(r).sourceResources[0].resource.sha256='0'.repeat(64),
  (r:any)=>solution(r).sourceResources[0].allowedAnalyses=[],
  (r:any)=>solution(r).quality.normalizedEnergyBalance=.01,
  (r:any)=>solution(r).quality.beNumericalDissipationJ+=1e-9,
  (r:any)=>solution(r,'single frequency').parameters[2][1][0].imaginary+=1,
  (r:any)=>solution(r,'both common').energyJ[5]=0,
  (r:any)=>r.reports.flatMap((p:any)=>p.plots)[0].data.values[3]+=.1,
 ]){const copy=structuredClone(result);corrupt(copy);assert.throws(()=>auditElectricalPhysical(copy));}
});
