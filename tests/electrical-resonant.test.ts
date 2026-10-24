import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {networkResource} from '../src/analyze/analysis-dsl/frequency-model.ts';
import {createAnalysisApi} from '../src/analyze/analysis-dsl/context.ts';
import {transientRequest} from '../src/analyze/analysis-dsl/transient-executor.ts';
import {createFrequencyExecutor} from '../src/analyze/analysis-dsl/frequency-executor.ts';
import {resonantResource} from './helpers/electrical-resonant-model.ts';
import {auditResonantRuntime} from './helpers/electrical-resonant-audit.ts';
test('authored resonant metadata uses existing builders and reaches native unchanged',async()=>{
 const model=resonantResource();let request:any;
 const api=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},()=>{},async task=>{
  request=transientRequest(task as any);return{id:'adapter-only',status:'complete',method:'capture',diagnostics:[],assumptions:[],metrics:{}};
 });
 const a=api.globals.analysis(),n=a.network({model:a.model(model),ports:{left:'p1',right:'p2'}});
 await a.si().network(n).initial('steady').drivers([{port:n.port('left'),model:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:25},stimulus:{kind:'waveform',voltageV:{kind:'step',from:0,to:1,atS:0,riseS:2e-7}}}]).time({stopS:1e-6,sampleStepS:1e-8}).run();
 assert.deepEqual(request.request.topology.instances[0].model.data,model.data);assert.deepEqual(request.sourceResources[0].resource,model);
 for(const change of [
  (r:any)=>r.data.temporal.dcSlopeF.pop(),(r:any)=>delete r.data.temporal.modeTransform,
  (r:any)=>r.data.temporal.normalizationOhm=NaN,(r:any)=>r.data.temporal.normalizationOhm={p1:50,p2:50},
  (r:any)=>r.data.temporal.method='guess',(r:any)=>r.data.representation='Z',
  (r:any)=>r.data.temporal.highFrequency.inverseInductancePerH[0]=0,
 ]){const bad=structuredClone(model);change(bad);assert.throws(()=>networkResource(bad));}
});
test('saved resonant fields and negative substitutions independently audit',async(t)=>{
 let result:any;try{result=JSON.parse(await readFile(process.env.P6_RESONANT_RESULT??'debugging/p6-resonant-runtime/direct/result.json','utf8')).result;}catch{t.skip('Generate current saved runtime evidence first');return;}
 assert.equal(auditResonantRuntime(result).status,'pass');
 const solve=(r:any)=>r.reports.flatMap((p:any)=>p.calculations).filter((c:any)=>c.phase==='completed')[1].result.solution;
 for(const corrupt of [
  (r:any)=>solve(r).driverCurrentsA[20][0]+=.001,(r:any)=>solve(r).portVoltages[20][1]+=.1,
  (r:any)=>solve(r).energyJ[20]+=1e-9,(r:any)=>solve(r).quality.energyBalanceJ=1,
  (r:any)=>solve(r).fit.sourceFits[0].coefficients[0].inductanceH*=2,(r:any)=>solve(r).bandHz[1]=2e8,
  (r:any)=>solve(r).sourceResources[0].sha256='0'.repeat(64),
  (r:any)=>r.reports.flatMap((p:any)=>p.plots)[0].data.validity.status='valid',
  (r:any)=>r.reports.flatMap((p:any)=>p.plots)[0].data.validity.domain.bandHz=[0,2e8],
  (r:any)=>r.reports.flatMap((p:any)=>p.plots)[0].data.values[20]+=.1,
 ]){const bad=structuredClone(result);corrupt(bad);assert.throws(()=>auditResonantRuntime(bad));}
});
test('ordinary AC derivative of resonant source stays conditional frequency-only data',async()=>{
 const execute=createFrequencyExecutor();let resource:any;
 const api=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},()=>{},async task=>{
  const result=await execute(task as any);resource=result.network;return result;
 });
 const a=api.globals.analysis(),n=a.network({model:a.model(resonantResource()),ports:{left:'p1',right:'p2'}});
 await a.networkParameters().network(n).frequencyHz(1e6).representation('Y').budget({maxSeconds:60,maxMemoryMiB:256}).run();
 assert.deepEqual(resource.analyses,['networkParameters']);assert.equal(resource.data.temporal,undefined);assert.equal(resource.validity.status,'conditional');
 assert.deepEqual(resource.validity.domain.bandHz,[1e6,1e6]);assert.deepEqual(resource.validity.domain.provenance[0].sourceValidity.domain.bandHz,[0,1e8]);
 assert.equal(networkResource(resource).data.format,'network-parameters');
});
