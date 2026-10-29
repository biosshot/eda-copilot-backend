/** Measure the existing direct BandAmp section DSL in a fresh compiled worker.
 * Only an elapsed-time table is added; require the exact existing disk entry. */
import{readFileSync,writeFileSync,mkdirSync,existsSync}from'node:fs';import{resolve,join}from'node:path';import assert from'node:assert/strict';import{createHash}from'node:crypto';import{runAnalysisDsl}from'../../../../src/analyze/run-analysis-dsl.ts';import{localCoreIdentity}from'../../../../src/analyze/local-model-cache.ts';
const packet=process.argv[2],base=resolve('debugging/p6-p7-integration',packet);process.env.PCB_ANALYSIS_CACHE_DIR=join(base,'cache');
const original=JSON.parse(readFileSync(join(base,'BandAmpV5/cold/result.json'),'utf8'));
const key=original.reports.flatMap((r:any)=>r.plots).find((p:any)=>p.title.startsWith('Supported section warm')).data.cache.key;
assert.ok(existsSync(join(base,'cache',key+'.json')),'Exact final section cache required; no free preparation');
const boardJson=readFileSync('tests/pcb-analyze/BandAmpV5/BandAmpV5.json','utf8');
let code=readFileSync('tests/pcb-analyze/BandAmpV5/section-cache.js','utf8');assert.ok(code.includes('const warmSection=await'));
code=code.replace('const warmSection=await','const warmStarted=Date.now();const warmSection=await');
code+="\na.plot({title:'Measured same-worker complete warm run',kind:'table',data:{wallSeconds:(Date.now()-warmStarted)/1000,workerResources:warmSection.solution.resources,cache:warmSection.solution.cache}});\n";
const result=await runAnalysisDsl({code,board:JSON.parse(boardJson)},{timeoutMs:30000,maxOutputBytes:32*1024*1024});assert.equal(result.status,'complete',JSON.stringify(result.error));
const plots=result.reports.flatMap((r:any)=>r.plots);assert.equal(plots.find((p:any)=>p.title.startsWith('Supported section cold')).data.cache.status,'disk');
const measurement=plots.find((p:any)=>p.title==='Measured same-worker complete warm run').data;
assert.equal(measurement.cache.status,'memory');assert.equal(measurement.cache.key,key);assert.equal(measurement.cache.assembly,0);assert.equal(measurement.cache.training,0);
const out=join(base,'BandAmpV5/warm-measurement');mkdirSync(out,{recursive:true});writeFileSync(join(out,'code.js'),code);writeFileSync(join(out,'result.json'),JSON.stringify(result));
const evidence={...measurement,scope:'Complete same-worker section run and existing warm plot; OS worker high-water RSS, not process-family peak',core:localCoreIdentity(),boardSha256:createHash('sha256').update(boardJson).digest('hex'),codeSha256:createHash('sha256').update(code).digest('hex')};writeFileSync(join(out,'measurement.json'),JSON.stringify(evidence,null,2));console.log(JSON.stringify(evidence));
