import test from 'node:test';import assert from 'node:assert/strict';import{readFileSync}from'node:fs';
import{runAnalysisDsl}from'../src/analyze/run-analysis-dsl.ts';
test('user documentation AC and SI-only sampled-to-time examples execute through production DSL',async()=>{
 const source=readFileSync('docs/analyze/reference/p5-p6-electrical/electrical-core-examples.md','utf8');
 const blocks=[...source.matchAll(/```js\r?\n([\s\S]*?)```/g)].map(m=>m[1]);assert.equal(blocks.length,2);
 for(const code of blocks){const result=await runAnalysisDsl({code},{timeoutMs:120000});assert.equal(result.status,'complete',JSON.stringify(result.error));const checks=result.reports.flatMap(r=>r.checks);assert.equal(checks.length,1);assert.equal(checks[0].status,'pass');}
});
