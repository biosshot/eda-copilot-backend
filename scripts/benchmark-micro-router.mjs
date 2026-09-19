// node scripts/benchmark-micro-router.mjs [absolute/path/to/baseline.node]
// Build the current addon with npm run native:build first. No network/EDA required.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';

const require = createRequire(import.meta.url);
const current = require('../native/pcb-board-packer/index.cjs');
const baseline = process.argv[2] ? require(resolve(process.argv[2])) : undefined;
const data = JSON.parse(readFileSync(new URL('../tests/fixtures/pcb-route-cost/espower-usb.json', import.meta.url), 'utf8'));
const primitives = data.bodies.map(([id, layer, left, right, top, bottom, rotate]) => {
  const bbox = { left, right, top, bottom };
  return {
    id: `post:${id}`, kind: 'component', label: id, sourceNodeId: id, sourceNodeIds: [id],
    locked: true, canRotate: false, allowedOrientations: [0], bbox, collisionBoxes: [bbox],
    width: right - left, height: bottom - top, edgePlace: null, pathPorts: [],
    placements: [{ designator: id, x: (left + right) / 2, y: (top + bottom) / 2, rotate, layer, score: 0 }],
    connectionPoints: data.points.filter(([ref]) => ref.startsWith(`${id}.`)).map(([ref, net, x, y]) => ({ ref, net, x, y })),
  };
});
const relations = [['R8.1', 'U12.A6'], ['R7.1', 'U12.A7'], ['R7.2', 'U1.25'], ['R8.2', 'U1.26']]
  .map(([from, to]) => ({ id: `${from}->${to}`, kind: 'component', from: `pad:${from}`, to: `pad:${to}`,
    relation: 'critical_pair', priority: 'critical', weight: 280, hard: false, effect: 'score_only',
    satelliteAnchor: false, preferFacingPads: false }));
const problem = { version: 3, grid: 0.25, clearance: 0.25, searchWidth: 32, compactness: 'normal',
  bounds: data.bounds, fullBoardBounds: data.bounds, boardOutline: [], edgeClearance: 0,
  primitives, relations, obstacles: [], constraintRegions: [], components: [], componentPairClearance: [], componentConflict: [] };
const obstacles = data.pads.map(([ref, net, layer, left, right, top, bottom]) => ({
  ref, net, layer, box: { left, right, top, bottom }, primitiveId: `post:${ref.split('.')[0]}`,
}));

for (const [name, changed] of [['USB', ['post:R7', 'post:R8']], ['all components', primitives.map(p => p.id)]]) {
  const run = addon => addon.prepareRouteLayoutComparison(problem, changed, obstacles);
  const expected = run(current);
  if (baseline) {
    const previous = run(baseline);
    assert.equal(expected.version, previous.version);
    assert.deepEqual(expected.jobs, previous.jobs, `${name}: route samples changed`);
  }
  const versions = baseline ? [['baseline', baseline], ['current', current]] : [['current', current]];
  const timings = Object.fromEntries(versions.map(([label]) => [label, []]));
  for (const [, addon] of versions) for (let i = 0; i < 5; i++) run(addon);
  // Alternate order to reduce thermal/frequency bias; exclude warmup and parity checks.
  for (let round = 0; round < 6; round++) {
    for (const [label, addon] of round % 2 ? [...versions].reverse() : versions) {
      const start = performance.now();
      for (let i = 0; i < 20; i++) run(addon);
      timings[label].push((performance.now() - start) / 20);
    }
  }
  const median = values => { const sorted = [...values].sort((a, b) => a - b); return (sorted[2] + sorted[3]) / 2; };
  console.log(JSON.stringify({ name, jobs: expected.jobs.length, parity: baseline ? 'exact' : 'not checked',
    medianMs: Object.fromEntries(Object.entries(timings).map(([label, values]) => [label, median(values)])),
    speedup: baseline ? median(timings.baseline) / median(timings.current) : undefined, timings }, null, 2));
}
