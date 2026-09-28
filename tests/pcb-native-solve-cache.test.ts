import assert from 'node:assert/strict';
import test from 'node:test';
import { NativeSolveCache, cachedNativeSolve } from '../src/pcb-layout/pcb-auto-place-v2/native/solve-cache.ts';

const solution = () => ({ version: 3, states: [], rank: { hardCount: 0, hardSeverity: 0, score: 123 } });

test('native batch cache solves only unique misses and preserves order and result isolation', () => {
    const cache = new NativeSolveCache(3);
    const batches: number[][] = [];
    const run = (problems: Array<{ id: number }>) => {
        batches.push(problems.map(p => p.id));
        return problems.map(p => ({ ...solution(), rank: { ...solution().rank, score: p.id } }));
    };
    cache.solveMany('block', [{ id: 1 }], run);
    const values = cache.solveMany('block', [3, 1, 2, 3].map(id => ({ id })), run);
    assert.deepEqual(batches, [[1], [3, 2]]);
    assert.deepEqual(values.map(v => v.rank.score), [3, 1, 2, 3]);
    values[0].rank.score = -1;
    assert.equal(values[3].rank.score, 3);
    assert.equal(cache.solve('block', { id: 3 }, () => { throw Error('cache miss'); }).rank.score, 3);
    assert.throws(() => cache.solveMany('block', [{ id: 4 }], () => []), /number of results/);
    cache.solveMany('block', [{ id: 4 }], run);
    assert.deepEqual(batches.at(-1), [4]);
});

test('native solve cache reuses the complete problem and isolates mutable results', () => {
    const cache = new NativeSolveCache();
    let calls = 0;
    const solve = () => { calls++; return solution(); };
    const problem = { geometry: [1, 2], nets: ['A'], constraints: { clearance: 0.25 } };
    cache.solve('board', problem, solve).rank.score = -99;
    const hit = cache.solve('board', structuredClone(problem), solve);
    assert.equal(hit.rank.score, 123);
    hit.rank.score = -88;
    assert.equal(cache.solve('board', problem, solve).rank.score, 123);
    assert.equal(calls, 1);
    for (const changed of [{ ...problem, geometry: [1, 3] }, { ...problem, nets: ['B'] },
        { ...problem, constraints: { clearance: 0.251 } }]) cache.solve('board', changed, solve);
    cache.solve('block', problem, solve);
    assert.equal(calls, 5);
});

test('native solve cache evicts least recently used entries and bypasses oversized inputs/results', () => {
    const cache = new NativeSolveCache(2);
    let calls = 0;
    const solve = () => { calls++; return solution(); };
    for (const id of [1, 2, 1, 3, 1, 2]) cache.solve('board', { id }, solve);
    assert.equal(calls, 4);
    for (const bounded of [new NativeSolveCache(0), new NativeSolveCache(2, 1), new NativeSolveCache(2, 60)]) {
        const before = calls;
        for (let i = 0; i < 2; i++) bounded.solve('board', { id: 1 }, solve);
        assert.equal(calls - before, 2);
    }
});

test('native solve cache never conflates signed zero, nonfinite inputs, or failed solves', () => {
    const cache = new NativeSolveCache();
    let calls = 0;
    const solve = () => { calls++; return solution(); };
    for (const x of [0, -0, 0, -0]) cache.solve('board', { x }, solve);
    assert.equal(calls, 2);
    for (const x of [NaN, Infinity, -Infinity]) {
        cache.solve('board', { x }, solve);
        cache.solve('board', { x }, solve);
    }
    assert.equal(calls, 8);
    for (let i = 0; i < 2; i++) assert.throws(() => cache.solve('board', { x: 9 }, () => {
        calls++; throw new Error('native validation');
    }), /native validation/);
    assert.equal(calls, 10);
});

test('native solve cache is scoped to the loaded addon', () => {
    let calls = 0;
    const solve = () => { calls++; return solution(); };
    cachedNativeSolve({}, 'board', {}, solve);
    cachedNativeSolve({}, 'board', {}, solve);
    assert.equal(calls, 2);
});
