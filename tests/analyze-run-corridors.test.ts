import test from 'node:test';
import assert from 'node:assert/strict';
import { proposeCorridors } from '../src/analyze/run-corridors.ts';
import { canonicalLocalGeometry } from '../src/analyze/local-model-cache.ts';
const polygon = (outer: number[][], holes: number[][][] = []) => ({ outer, holes });
const run: any = { id: 'r', direction: [1, 0], from: [0, 0], to: [10, 0], lengthMm: 10, widthMm: .1, layer: 'TOP', sourcePath: 'track', regionIds: ['signal'] };
function geometry() { return { boolean_error_bound_mm: 1e-9, regions: [{ id: 'signal', source: { path: 'track' }, layer: 'TOP', polygon: polygon([[0, -.05], [10, -.05], [10, .05], [0, .05]]) }, { id: 'return', source: { path: 'finite-return' }, layer: 'BOTTOM', polygon: polygon([[0, -.25], [10, -.25], [10, .25], [0, .25]]) }], layers: [{ layer: 'TOP', copper: [polygon([[0, -.05], [10, -.05], [10, .05], [0, .05]])] }, { layer: 'BOTTOM', copper: [polygon([[0, -.25], [10, -.25], [10, .25], [0, .25]])] }], holes: [], outline: polygon([[-1, -1], [11, -1], [11, 1], [-1, 1]]) } as any; }
test('material edge variation blocks interval even with unchanged copper', () => {
    const g = geometry();
    g.outline = polygon([[-1, -1], [11, -1], [11, 1], [6, 1], [4, .1], [-1, 1]]);
    const c = proposeCorridors(g, [run], .3)[0];
    assert.ok(c.events.some(e => e.kind === 'material' && e.startMm < 4.2 && e.stopMm > 4.2));
    assert.ok(!c.uniformIntervalsMm.some(([a, b]) => a < 4 && b > 6));
});
test('internal cutout between three convenient sample locations remains material event', () => {
    const g = geometry();
    g.outline.holes = [[[2.1, -.1], [2.2, -.1], [2.2, .1], [2.1, .1]]];
    const c = proposeCorridors(g, [run], .3)[0];
    assert.ok(c.events.some(e => e.kind === 'material' && e.startMm < 2.1 && e.stopMm > 2.1));
    assert.ok(!c.uniformIntervalsMm.some(([a, b]) => a < 2.1 && b > 2.2));
});
test('source subdivision does not create a physical layer boundary event', () => {
    const g = geometry(), before = proposeCorridors(g, [run], .3)[0];
    g.regions[0].polygon = polygon([[0, -.05], [5, -.05], [5, .05], [0, .05]]);
    g.regions.push({ ...g.regions[0], id: 'signal-b', source: { path: 'track-b' }, polygon: polygon([[5, -.05], [10, -.05], [10, .05], [5, .05]]) });
    const after = proposeCorridors(g, [run], .3)[0];
    assert.deepEqual(after.events, before.events);
    assert.deepEqual(after.uniformIntervalsMm, before.uniformIntervalsMm);
});
test('continuous plane crossing a corridor side is diagnosed as distributed physical interface', () => {
    const g = geometry();
    g.regions[1].polygon = polygon([[-1, -2], [11, -2], [11, 2], [-1, 2]]);
    const c = proposeCorridors(g, [run], .3)[0];
    assert.equal(c.lateralContinuations.length, 2);
    assert.ok(c.lateralContinuations.every(v => v.sourcePath === 'finite-return' && Math.abs(v.lengthMm - 10) < 1e-9));
});
test('local cache uses canonical physical electrode source union identity', () => {
    const input = (e: any) => ({ electrodes: [{ name: 'contact', layer: 'TOP', upper: false, ...e }] });
    assert.deepEqual(canonicalLocalGeometry(input({ sourcePath: 'track' })), canonicalLocalGeometry(input({ sourcePaths: ['track'] })));
    assert.deepEqual(canonicalLocalGeometry(input({ sourcePaths: ['b', 'a', 'b'] })), canonicalLocalGeometry(input({ sourcePaths: ['a', 'b'] })));
    assert.notDeepEqual(canonicalLocalGeometry(input({ sourcePath: 'a', sourcePaths: ['b'] })), canonicalLocalGeometry(input({ sourcePaths: ['a', 'b'] })));
});
