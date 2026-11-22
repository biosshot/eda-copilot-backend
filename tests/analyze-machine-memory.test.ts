import test from 'node:test';
import assert from 'node:assert/strict';
import {machineMemoryCapMiB,resolveMemoryMiB} from '../src/analyze/analysis-dsl/machine-memory.ts';
import {withBudgetHint} from '../src/analyze/analysis-dsl/budget-hint.ts';

test('request budget.maxMemoryMiB is used as given',()=>{
 assert.deepEqual(resolveMemoryMiB(6144,1<<30),{maxMemoryMiB:6144,machineLimited:false});
});
test('no budget: machine cap = available RAM minus 15% (injected figure)',()=>{
 const total=16*1024*1048576;
 assert.equal(machineMemoryCapMiB(total),Math.floor(16384*0.85));
 assert.deepEqual(resolveMemoryMiB(undefined,total),{maxMemoryMiB:13926,machineLimited:true});
 assert.equal(machineMemoryCapMiB(1024),256);
});
test('a machine-derived refusal says it is a machine-memory limit outside the extractor',()=>{
 const text=withBudgetHint('LOCAL_MEMORY_LIMIT','x',{maxSeconds:1,maxMemoryMiB:13926},true);
 assert.match(text,/machine-memory limit outside the extractor/);
 assert.doesNotMatch(withBudgetHint('LOCAL_MEMORY_LIMIT','x',{maxSeconds:1,maxMemoryMiB:6144}),/machine-memory/);
});
