import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {runAnalysisProblem,terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs');
function board(){const b=JSON.parse(readFileSync('tests/fixtures/analysis-dc-coupon.json','utf8'));b.components=[];b.pads=[{id:'a',number:'1',net:'VCC',at:{x:0,y:0},rotationDeg:0,layers:['TOP'],shape:{kind:'rect',widthMm:1,heightMm:1}},{id:'b',number:'1',net:'GND',at:{x:1.2,y:0},rotationDeg:0,layers:['TOP'],shape:{kind:'rect',widthMm:1,heightMm:1}}];return b;}
const start='const a=analysis({board});';const options={timeoutMs:30000,maxOutputBytes:16*1024*1024};
const completed=(r:any)=>r.reports.flatMap((r:any)=>r.calculations).filter((c:any)=>c.phase==='completed').map((c:any)=>c.result);

test('production geometry DSL: complete pass/fail distances, scoped external neighbour and continuation',async()=>{
 const r=await runAnalysisDsl({board:board(),code:`${start}const base=a.geometry().scope([padId('a')]);const p=await base.rules([{kind:'clearance',minMm:.1}]).run();a.check({name:'controlled pass',actual:p.violations.count(),equals:0});const f=await base.rules([{kind:'clearance',minMm:.3}]).run();a.check({name:'expected fail',actual:f.violations.count(),equals:0});a.check({name:'continued',actual:f.violations.items.length,equals:1});a.plot({title:'witness',data:f.measurements});`},options);assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),['pass','fail','pass']);const f=completed(r)[1];assert.equal(f.violations.items.length,1);assert.deepEqual(f.violations.items[0].source_ids,['a','b']);assert.ok(Math.abs(f.violations.items[0].actual_mm-.2)<1e-9);
});
test('partial coverage never turns aggregate checks into clean pass, while confirmed findings survive',async()=>{
 const b=board();const r=await runAnalysisDsl({board:b,code:`${start}const g=await a.geometry().rules([{kind:'connectivity'},{kind:'clearance',minMm:.3},{kind:'edge-clearance',minMm:.3}]).run();a.check({name:'aggregate is unknown',actual:g.violations.count(),equals:0});a.check({name:'upper bound also unknown',actual:g.violations.count(),max:100,unit:'1'});a.check({name:'confirmed reports retained',actual:g.violations.items.length,min:1,unit:'1'});a.check({name:'continued',actual:1,equals:1});`},options);assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),['unknown','unknown','pass','pass']);assert.equal(completed(r)[0].status,'partial');
});
test('missing/stale copper and uncertain point contacts keep diagnostics and aggregate unavailable',async()=>{
 const b=board();b.pads[1].at={x:1,y:1};b.copper.fixed.zones=[{id:'missing-zone',kind:'fill',net:'VCC',layers:['TOP'],outline:{outer:[{x:3,y:0},{x:5,y:0},{x:5,y:2},{x:3,y:2}]},filledPolygons:[{layer:'TOP',source:'cad',status:'unavailable',reason:'test missing copper'}]}];
 // Use a valid captured unavailable fill status vocabulary.
 b.copper.fixed.zones[0].filledPolygons[0].status='missing';
 const native=addon.validateAnalysisGeometry({board_json:JSON.stringify(b),rules:[{kind:'connectivity'},{kind:'clearance',minMm:.2}]});assert.equal(native.status,'partial',JSON.stringify(native));assert.ok(native.diagnostics.some((d:any)=>d.code==='fill_unavailable'));assert.ok(native.measurements.some((m:any)=>m.code==='contact_uncertain'));
});
test('unsupported channel rules/selectors, invalid limits and native recovery are explicit',async()=>{
 const b=board(),request={board_json:JSON.stringify(b),rules:[{kind:'clearance',minMm:.3}]};for(const rule of [{kind:'length',maxMm:1},{kind:'skew',maxMm:1},{kind:'return-transition',maxDistanceMm:1},{kind:'reference-continuity'}])assert.equal(addon.validateAnalysisGeometry({...request,rules:[rule]}).diagnostics[0].code,'GEOMETRY_UNSUPPORTED_RULE');
 for(const delta of [{budget:{maxMemoryMiB:1}},{budget:{maxSeconds:1e-12}},{limits:{max_measurements:1},rules:[{kind:'edge-clearance',minMm:.3}]},{scope:[{kind:'via',id:'v'}]},{scope:[]}]){assert.equal(addon.validateAnalysisGeometry({...request,...delta}).status,'rejected');assert.equal(addon.validateAnalysisGeometry(request).status,'complete');}
 const unsupported=await runAnalysisDsl({board:b,code:`${start}const p=a.port({positive:padId('a'),negative:padId('b')});const c=a.channel({from:p,to:p});await a.geometry().rules([{kind:'length',channel:c,maxMm:1}]).run();`},options);assert.equal(unsupported.status,'failed');assert.equal(unsupported.error?.code,'GEOMETRY_UNSUPPORTED_RULE');
});
test('geometry runtime native parity, active cancellation/deadline/report cap and following successful jobs',async()=>{
 const request={board_json:JSON.stringify(board()),rules:[{kind:'clearance' as const,minMm:.3}]};try{const r=await runAnalysisProblem({kind:'geometry-validation',request});const n=addon.validateAnalysisGeometry(request);delete (r as any).elapsed_ms;delete n.elapsed_ms;assert.deepEqual(r,n);const abort=new AbortController();await assert.rejects(runAnalysisProblem({kind:'geometry-validation',request},{signal:abort.signal,onProgress:e=>{if(e.phase==='started')abort.abort();}}),{code:'ABORTED'});await assert.rejects(runAnalysisProblem({kind:'geometry-validation',request},{timeoutMs:1}),{code:'TIMEOUT'});assert.equal((await runAnalysisProblem({kind:'geometry-validation',request})).status,'complete');}finally{await terminateAnalysisPool();}
 const code=`${start}await a.geometry().rules([{kind:'clearance',minMm:.3}]).run();`;assert.equal((await runAnalysisDsl({board:board(),code},{timeoutMs:1})).error?.code,'TIMEOUT');const abort=new AbortController();abort.abort();assert.equal((await runAnalysisDsl({code},{signal:abort.signal})).error?.code,'ABORTED');assert.equal((await runAnalysisDsl({board:board(),code},{maxOutputBytes:4096})).status,'failed');assert.equal((await runAnalysisDsl({board:board(),code},options)).status,'complete');
});
test('Node shared addon agrees with standalone CPU/F64 geometry validation',()=>{
 const request={board_json:JSON.stringify(board()),rules:[{kind:'clearance',minMm:.3}]};const n=addon.validateAnalysisGeometry(request);const r=spawnSync('cargo',['run','--quiet','--locked','--manifest-path','native/analyze/Cargo.toml','--example','validation'],{input:JSON.stringify(request),encoding:'utf8',windowsHide:true,timeout:180000,maxBuffer:16*1024*1024});assert.ifError(r.error);assert.equal(r.status,0,r.stderr);const native=JSON.parse(r.stdout);delete n.elapsed_ms;delete native.elapsed_ms;assert.deepEqual(native,n);
});
