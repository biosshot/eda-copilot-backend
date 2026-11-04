import test from 'node:test';
import assert from 'node:assert/strict';
import {manifest,validateManifest,classifyReview} from '../scripts/p7/p7-coverage.mjs';
test('coverage requires all known classes and handles malformed records',()=>{
 assert.deepEqual(validateManifest(manifest),[]);
 for(const cases of [[],[null],[{id:'unknown',test:4}], [...manifest.cases,manifest.cases[0]]])assert.ok(validateManifest({schemaVersion:1,cases}).length);
 const bad=structuredClone(manifest);bad.cases[0].test='../outside';assert.ok(validateManifest(bad).length);
});
test('review band is 1.5 percentage points, with strict invariants and finite inputs',()=>{
 for(const [errorPercent,expected] of [[2,'pass'],[3.1,'requires-review'],[3.5,'requires-review'],[3.5001,'fail'],[NaN,'fail'],[Infinity,'fail'],[-1,'fail']] as const)
 assert.equal(classifyReview({errorPercent,upperThresholdPercent:2}),expected);
 assert.equal(classifyReview({errorPercent:2.1,upperThresholdPercent:2,kind:'invariant'}),'fail');
 assert.equal(classifyReview({errorPercent:0,upperThresholdPercent:2,failure:'unknown'}),'fail');
 assert.equal(classifyReview({errorPercent:0,upperThresholdPercent:NaN}),'fail');
});
