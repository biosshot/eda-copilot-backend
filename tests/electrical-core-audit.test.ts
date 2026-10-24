import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync}from'node:fs';
import {runAnalysisDsl}from'../src/analyze/run-analysis-dsl.ts';
import {auditElectricalCore,coreCases}from'./helpers/electrical-core-audit.ts';
test('direct production DSL completed fields pass independent audit; missing/corrupt/conditional data never pass',async()=>{
 const results=process.env.P6_SAVED_RESULTS?process.env.P6_SAVED_RESULTS.split(';').map(path=>{const saved=JSON.parse(readFileSync(path,'utf8'));return saved.result??saved;}):[await runAnalysisDsl({code:readFileSync('tests/fixtures/analysis-electrical-core/scenario.js','utf8')},{timeoutMs:120000,maxOutputBytes:32*1024*1024})];
 for(const result of results){
 assert.equal(auditElectricalCore(result).status,'pass');
 const changes:((r:any)=>void)[]=[
  r=>coreCases(r).get('ac').solution.nodeVoltages.out.pop(),
  r=>coreCases(r).get('ac').solution.nodeVoltages.out[5].imaginary+=.1,
  r=>coreCases(r).get('sampled').solution.fit.validity.status='valid',
  r=>coreCases(r).get('sampled').solution.conditional=false,
  r=>coreCases(r).get('sampled').solution.portVoltages.pop(),
  r=>coreCases(r).get('sampled').solution.sourceResources[0].sha256='0'.repeat(64),
  r=>coreCases(r).get('sampled').solution.fit.refinement.at(-1).holdoutMaximum=1,
  r=>coreCases(r).get('sampled').solution.driverCurrentsA[3][0]=null,
  r=>coreCases(r).get('matched').solution.portVoltages[5][1]=.1,
  r=>coreCases(r).get('reflected').solution.portVoltages[5].pop(),
  r=>coreCases(r).get('cascade frequency').solution.parameters[2][0][1].real=.9,
  r=>{const p=r.reports.flatMap((r:any)=>r.plots).find((p:any)=>p.title==='sampled:output');p.data.validity.status='valid';},
  r=>{const p=r.reports.flatMap((r:any)=>r.plots).find((p:any)=>p.title==='ac:phase');p.data.unit='1';},
  r=>{const p=r.reports.flatMap((r:any)=>r.plots).find((p:any)=>p.title==='matched:output');p.data.values=[];},
 ];
 for(const change of changes){const copy=structuredClone(result);change(copy);assert.throws(()=>auditElectricalCore(copy));}
 }
});
