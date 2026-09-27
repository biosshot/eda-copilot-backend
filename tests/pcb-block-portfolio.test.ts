import test from 'node:test';
import assert from 'node:assert/strict';
import { blockPolicy } from '../src/pcb-layout/pcb-auto-place-v2/block-policy.ts';
import { selectBlockPortfolio } from '../src/pcb-layout/pcb-auto-place-v2/block-portfolio.ts';
import { rotatePrimitive, translatePrimitive, unionPrimitive, type PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import { defaultSolverOptions } from '../src/pcb-layout/pcb-auto-place/utils.ts';
import { createPlacementReport } from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import { encodeNativePostPlaceRefineProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { PlacementInput } from '../src/types/pcb/layout-model.ts';

test('branch policy enables previous improvements and has an explicit legacy fallback', () => {
    const saved = { ...process.env };
    try {
        delete process.env.PCB_BLOCK_PROFILE; delete process.env.PCB_BLOCK_ROUTING; delete process.env.PCB_BLOCK_PORTFOLIO;
        const p = blockPolicy(['GND']);
        assert.equal(p.searchWidth, 4); assert.equal(p.portfolio, true);
        for (const flag of ['netCandidates', 'stableNetWeight', 'reducedHull', 'smoothAspect', 'longNets', 'extraPasses', 'pairSwaps', 'reinsertPair', 'keepDenseAccess'] as const) assert.equal(p.experiments[flag], true);
        process.env.PCB_BLOCK_PROFILE = 'legacy';
        assert.equal(blockPolicy().searchWidth, 1); assert.equal(blockPolicy().portfolio, false);
        process.env.PCB_BLOCK_ROUTING = 'typo';
        assert.throws(() => blockPolicy(), /Unknown PCB_BLOCK_ROUTING/);
    } finally {
        for (const key of ['PCB_BLOCK_PROFILE', 'PCB_BLOCK_ROUTING', 'PCB_BLOCK_PORTFOLIO']) {
            if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
        }
    }
});

test('nested children and portfolio rotate about the parent origin and translate together', () => {
    const parent = unionPrimitive('pair', 'block', 'pair', 'pair', [part('A', -5), part('B', 5)]);
    parent.layoutAlternatives = [unionPrimitive('pair', 'block', 'pair', 'pair', [part('A', 5), part('B', -5)])];
    const moved = translatePrimitive(rotatePrimitive(parent, 90), 7, 8);
    assert.deepEqual(moved.placements, moved.children.flatMap(p => p.placements));
    assert.deepEqual(moved.placements.map(p => [p.x, p.y]), [[7, 3], [7, 13]]);
    assert.deepEqual(moved.layoutAlternatives![0].placements.map(p => [p.x, p.y]), [[7, 13], [7, 3]]);
    assert.deepEqual(parent.placements.map(p => [p.x, p.y]), [[-5, 0], [5, 0]]);
});

test('board context selects the internal variant that faces external neighbours and preserves fixed poses', () => {
    const { input, roots } = fixture();
    const before = structuredClone(roots);
    const result = selectBlockPortfolio(input, roots, .5);
    const poses = result.flatMap(p => p.placements);
    assert.equal(poses.find(p => p.designator === 'A')!.x, 5);
    assert.equal(poses.find(p => p.designator === 'B')!.x, -5);
    assert.equal(createPlacementReport(input, poses).ok, true);
    assert.deepEqual(result.slice(1), before.slice(1));
    assert.deepEqual(roots, before);
});

test('a better wire score cannot select a block variant intersecting a keepout', () => {
    const { input, roots } = fixture();
    // Only the alternative places A at this coordinate; restrict its block while
    // allowing B's original footprint here through independent component owners.
    const alternative = roots[0].layoutAlternatives![0];
    alternative.children[0] = part('A', 5, 2);
    roots[0].layoutAlternatives = [unionPrimitive('pair', 'block', 'pair', 'pair', alternative.children)];
    input.constraintRegions = [{ name: 'ban', box: { left: 3, right: 7, top: .6, bottom: 4 }, layers: ['top'], allowBlocks: [] }];
    const result = selectBlockPortfolio(input, roots, .25);
    assert.ok(result[0].placements.find(p => p.designator === 'A')!.x < 0);
    assert.equal(createPlacementReport(input, result.flatMap(p => p.placements)).ok, true);
});

test('portfolio preserves hard pin distance and block bounding constraints', () => {
    for (const anchor of [false, true]) {
        const { input, roots } = fixture();
        if (anchor) {
            Object.assign(input.blocks[0], { hardBbox: true, maxBboxWidth: 12, maxBboxHeight: 2 });
            roots[0].layoutAlternatives = [unionPrimitive('pair', 'block', 'pair', 'pair', [part('A', 5, 3), part('B', -5)])];
        } else input.hints = [{ relation: 'critical_pair', source: { type: 'pin', designator: 'A', pin_number: '1' },
            target: { type: 'pin', designator: 'LEFT', pin_number: '1' }, maxDistance: 5, hard: true, priority: 'critical' }];
        const result = selectBlockPortfolio(input, roots, .25);
        assert.ok(result[0].placements.find(p => p.designator === 'A')!.x < 0);
    }
});

test('native portfolio validator handles reordered poses and rejects invalid inventory', () => {
    const { input, roots } = fixture();
    const poses = roots.flatMap(p => p.placements);
    const problem = encodeNativePostPlaceRefineProblem(input, poses, 1);
    const validate = loadNativeBoardPacker().validatePlacementChange;
    assert.equal(validate(problem, poses.toReversed()), true);
    assert.equal(validate(problem, poses.slice(1)), false);
    assert.equal(validate(problem, [poses[0], poses[0], ...poses.slice(2)]), false);
    assert.equal(validate(problem, poses.map(p => ({ ...p, x: 100 }))), false);
});

function part(designator: string, x: number, y = 0, locked = false): PlacementPrimitive {
    const box = { left: x - .5, right: x + .5, top: y - .5, bottom: y + .5 };
    return { id: designator, kind: 'component', sourceNodeId: designator, label: designator, locked,
        allowedOrientations: [0, 90, 180, 270], bbox: box, collisionBoxes: [box], width: 1, height: 1,
        placements: [{ designator, x, y, rotate: 0, layer: 'top', score: 0 }],
        connectionPoints: [{ ref: `${designator}.1`, x, y }], children: [] };
}

function fixture() {
    const roots = [unionPrimitive('pair', 'block', 'pair', 'pair', [part('A', -5), part('B', 5)]), part('LEFT', -9, 0, true), part('RIGHT', 9, 0, true)];
    roots[0].layoutAlternatives = [unionPrimitive('pair', 'block', 'pair', 'pair', [part('A', 5), part('B', -5)])];
    const input: PlacementInput = {
        board: { coordinateSystem: 'centered', outline: { type: 'rect', width: 40, height: 20 }, defaultLayer: 'top', allowedLayers: ['top'], clearances: { component: .1, edge: .1 } },
        boardHoles: [], constraintRegions: [], modules: [], hints: [], paths: [],
        blocks: [{ name: 'pair', description: '', component_designators: ['A', 'B'], role: 'generic' }, ...['LEFT', 'RIGHT'].map(name => ({ name, description: '', component_designators: [name], role: 'generic' as const }))],
        components: ['A', 'B', 'LEFT', 'RIGHT'].map(designator => ({ designator, value: '', search_query: '', part_uuid: null, footprint_uuid: null,
            block_name: designator.length === 1 ? 'pair' : designator,
            pins: [{ pin_number: '1', name: '1', signal_name: designator === 'A' ? 'RIGHT' : designator === 'B' ? 'LEFT' : designator }],
            footprint: { name: 'test', width: 1, height: 1, pads: [{ pin_number: '1', x: 0, y: 0, width: .2, height: .2 }] },
            pcb: { role: 'passive', allowedLayers: ['top'], allowedRotations: [0, 90, 180, 270], ...(designator.length > 1 ? { fixedPlacement: {} } : {}) } })),
        solverOptions: { ...defaultSolverOptions, ignoredRatsnestSignals: [] },
    };
    return { input, roots };
}
