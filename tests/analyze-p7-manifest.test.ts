import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {manifest,validateManifest,classifyReview} from '../scripts/p7/p7-coverage.mjs';
test('coverage requires all known classes and handles malformed records',()=>{
 assert.deepEqual(validateManifest(manifest),[]);
 for(const cases of [[],[null],[{id:'unknown',test:4}], [...manifest.cases,manifest.cases[0]]])assert.ok(validateManifest({schemaVersion:1,cases}).length);
 const bad=structuredClone(manifest);bad.cases[0].test='../outside';assert.ok(validateManifest(bad).length);
});
test('coverage rejects contradictory preparation, missing metadata and claimed acceptance', () => {
 const bad = structuredClone(manifest);
 const first = bad.cases[0];
 first.status = 'ready';
 first.acceptance = 'accepted';
 first.observables = [];
 first.proposedReference = ' ';
 const errors = validateManifest(bad);
 for (const message of ['ready has missing prerequisites', 'acceptance requires a separate reviewed evidence workflow',
   'invalid observables', 'invalid proposedReference']) {
   assert.ok(errors.includes(first.id + ': ' + message));
 }
});
test('coverage rejects linked files outside the repository', (t) => {
 const temporary = mkdtempSync(join(tmpdir(), 'p7-manifest-'));
 try {
   const root = join(temporary, 'repository');
   const outside = join(temporary, 'outside');
   mkdirSync(root);
   mkdirSync(outside);
   writeFileSync(join(outside, 'test.ts'), '');
   try {
     symlinkSync(outside, join(root, 'linked'), process.platform === 'win32' ? 'junction' : 'dir');
   } catch (error) {
     if (process.platform !== 'win32' || (error as NodeJS.ErrnoException).code !== 'EPERM') throw error;
     t.skip('Windows environment does not permit creating a junction');
     return;
   }
   const first = { ...manifest.cases[0], test: 'linked/test.ts' };
   assert.ok(validateManifest({ schemaVersion: 1, cases: [first] }, root)
     .includes(first.id + ': nonlocal test path'));
 } finally {
   rmSync(temporary, { recursive: true, force: true });
 }
});
test('review band is 1.5 percentage points, with strict invariants and finite inputs',()=>{
 for(const [errorPercent,expected] of [[2,'pass'],[3.1,'requires-review'],[3.5,'requires-review'],[3.5001,'fail'],[NaN,'fail'],[Infinity,'fail'],[-1,'fail']] as const)
 assert.equal(classifyReview({errorPercent,upperThresholdPercent:2}),expected);
 assert.equal(classifyReview({errorPercent:2.1,upperThresholdPercent:2,kind:'invariant'}),'fail');
 assert.equal(classifyReview({errorPercent:0,upperThresholdPercent:2,failure:'unknown'}),'fail');
 assert.equal(classifyReview({errorPercent:0,upperThresholdPercent:NaN}),'fail');
 assert.equal(classifyReview({errorPercent:9.5,upperThresholdPercent:8,kind:'physical-error'}),'requires-review');
 assert.equal(classifyReview(null),'fail');
});
