import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync}from'node:fs';
import{runAnalysisDsl}from'../src/analyze/run-analysis-dsl.ts';import{auditFrequencyReport,frequencyCases}from'./helpers/pcb-analysis-frequency-audit.ts';
test('production frequency audit catches swapped coefficients, references, order, missing axes/plots, nature and extrapolation',async()=>{
 const result=await runAnalysisDsl({code:readFileSync('tests/fixtures/analysis-frequency/network.js','utf8'),models:JSON.parse(readFileSync('tests/fixtures/analysis-frequency/resources.json','utf8'))},{maxOutputBytes:16*1024*1024});assert.equal(auditFrequencyReport(result).status,'pass');
 const changes=[
 (r:any)=>{frequencyCases(r)[0].solution.sourceHashes[0].sha256='0'.repeat(64);},
 (r:any)=>{const s=frequencyCases(r)[3].solution;[s.parameters[0][0][1],s.parameters[0][1][0]]=[s.parameters[0][1][0],s.parameters[0][0][1]];},
 (r:any)=>{frequencyCases(r)[0].solution.referenceOhm=[75,50];},
 (r:any)=>{frequencyCases(r)[0].solution.ports.reverse();},
 (r:any)=>{frequencyCases(r)[0].solution.frequenciesHz.pop();},
 (r:any)=>{r.reports[0].plots.pop();},
 (r:any)=>{frequencyCases(r)[3].solution.nature='passive';},
 (r:any)=>{frequencyCases(r)[0].solution.frequenciesHz[0]=.5e9;},
 (r:any)=>{frequencyCases(r)[0].solution.provenance[0].sourceReferenceOhm=[75,75];},
 (r:any)=>{frequencyCases(r)[0].solution.provenance[0].sourcePorts.reverse();},
 (r:any)=>{frequencyCases(r)[0].solution.provenance[0].normalized.parameters[0][0][1].real=.4;},
 (r:any)=>{frequencyCases(r)[0].solution.interpolation.pop();},
 (r:any)=>{frequencyCases(r)[0].solution.conditional=false;},
 (r:any)=>{r.reports[0].plots[0].data.values[0]=999;},
 (r:any)=>{r.reports[0].checks[1].status='unknown';},
 ];
 for(const change of changes){const copy=structuredClone(result);change(copy);assert.throws(()=>auditFrequencyReport(copy));}
});
