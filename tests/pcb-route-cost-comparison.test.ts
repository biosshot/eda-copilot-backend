import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { NativeBoardPackProblemV3, NativePrimitive, NativeRelation, NativeRoutingObstacle } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';

import { refinePostPlacement } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { defaultSolverOptions } from '../src/pcb-layout/pcb-auto-place/utils.ts';
import type { PcbComponent, Placement, PlacementInput } from '../src/types/pcb/layout-model.ts';

const addon = loadNativeBoardPacker();
const bounds = { left: -10, right: 10, top: -10, bottom: 10 };

function primitive(id: string, x: number, y: number, net: string): NativePrimitive {
    const bbox = { left: x - 0.05, right: x + 0.05, top: y - 0.05, bottom: y + 0.05 };
    return { id, kind: 'component', label: id, sourceNodeId: id, sourceNodeIds: [id],
        locked: true, canRotate: false, allowedOrientations: [0], bbox, collisionBoxes: [bbox],
        width: 0.1, height: 0.1, placements: [{ designator: id, x, y, rotate: 0, layer: 'top', score: 0 }],
        connectionPoints: [{ ref: `${id}.1`, x, y, net }], pathPorts: [], edgePlace: null };
}
function relation(from: string, to: string): NativeRelation {
    return { id: `${from}->${to}`, kind: 'component', from: `pad:${from}`, to: `pad:${to}`,
        relation: 'critical_pair', priority: 'critical', weight: 70, hard: false, effect: 'score_only',
        satelliteAnchor: false, preferFacingPads: false } as NativeRelation;
}
function problem(primitives: NativePrimitive[], relations: NativeRelation[] = []): NativeBoardPackProblemV3 {
    return { version: 3, grid: 0.25, clearance: 0.25, searchWidth: 32, compactness: 'normal',
        bounds, fullBoardBounds: bounds, boardOutline: [], edgeClearance: 0,
        primitives, relations, obstacles: [], constraintRegions: [], components: [],
        componentPairClearance: [], componentConflict: [] };
}

test('temporary routes leave a physically legal 0.5 mm adjacent endpoint accessible', () => {
    const p = problem([primitive('N', -2, 0, 'N'), primitive('TN', 2, 0, 'N'),
        primitive('P', -2, 0.5, 'P'), primitive('TP', 2, 0.5, 'P')],
    [relation('N.1', 'TN.1'), relation('P.1', 'TP.1')]);
    const baseline = addon.prepareRouteLayoutComparison(p, ['N', 'P'], []);
    assert.equal(baseline.jobs.length, 2);
    assert.ok(baseline.jobs.every(job => job.status === 'found' && job.vias === 0), JSON.stringify(baseline));
});

test('bounded fallback explores vias but reports their real critical cost', () => {
    const p = problem([primitive('A', -2, 0, 'SIG'), primitive('B', 2, 0, 'SIG')], [relation('A.1', 'B.1')]);
    const topWall: NativeRoutingObstacle = { box: { left: -0.5, right: 0.5, top: -11, bottom: 11 }, layer: 'top' };
    const baseline = addon.prepareRouteLayoutComparison(p, ['A'], [topWall]);
    const job = baseline.jobs[0];
    assert.equal(job.status, 'found', JSON.stringify(job));
    assert.equal(job.vias, 2);
    assert.equal(job.usedFallback, true);
    assert.ok(job.physicalCost! >= 2 * job.job.viaCost + job.planarLength! - 1e-6);
    assert.ok(job.expanded <= 4500);
    // Blocking both layers must not become cheaper than the known via route.
    const comparison = addon.compareRouteLayoutCandidate(p, [{ box: topWall.box }], baseline);
    assert.notEqual(comparison.jobs[0].status, 'found');
    assert.ok(comparison.afterPenalty > comparison.beforePenalty, JSON.stringify(comparison));
    assert.equal(comparison.feasibilityOrder, 1);
});

test('frozen route plans survive changed nearest pads without adding obligations', () => {
    const p = problem([primitive('R', -2, 0, 'USB'), primitive('J', 2, 0, 'USB')], [relation('R.1', 'J.1')]);
    p.primitives[1].connectionPoints.push({ ref: 'J.2', net: 'USB', x: -3, y: 0 });
    const baseline = addon.prepareRouteLayoutComparison(p, ['R'], []);
    assert.equal(baseline.jobs.length, 1, 'explicit pair covers this primitive pair/net');
    const moved = structuredClone(p);
    moved.primitives[0].connectionPoints[0].x = -3.5;
    const after = addon.compareRouteLayoutCandidate(moved, [], baseline);
    assert.deepEqual(after.jobs.map(j => j.job), baseline.jobs.map(j => j.job));
    assert.equal(after.jobs[0].job.targetRef, 'J.1');
    moved.primitives[1].connectionPoints = [];
    assert.throws(() => addon.compareRouteLayoutCandidate(moved, [], baseline), /route plan endpoint.*missing/);
});

test('ordinary three-terminal net changes tree while retaining every terminal', () => {
    const p = problem([primitive('J', -4, 0, 'SIG'), primitive('U5', 0, 0, 'SIG'), primitive('D5', 4, 0, 'SIG')]);
    const baseline = addon.prepareRouteLayoutComparison(p, ['J'], []);
    assert.deepEqual(baseline.topologyNets, ['SIG']);
    const pairs = (jobs: typeof baseline.jobs) => jobs.map(({ job }) =>
        [job.sourceRef, job.targetRef].sort().join('-')).sort();
    assert.deepEqual(pairs(baseline.jobs), ['D5.1-U5.1', 'J.1-U5.1']);
    const moved = problem([primitive('J', 6, 0, 'SIG'), primitive('U5', 0, 0, 'SIG'), primitive('D5', 4, 0, 'SIG')]);
    const after = addon.compareRouteLayoutCandidate(moved, [], baseline);
    assert.deepEqual(pairs(after.jobs), ['D5.1-J.1', 'D5.1-U5.1']);
    assert.equal(after.unresolvedAfter, 0);
    assert.equal(after.feasibilityOrder, 0);
    assert.ok(after.beforePenalty - after.afterPenalty <= baseline.maximumImprovement! + 1e-9);
    const same = addon.compareRouteLayoutCandidate(p, [], baseline);
    assert.equal(same.beforePenalty, same.afterPenalty);
    const legacy = { ...baseline, version: 1 as const, topologyNets: undefined };
    assert.deepEqual(addon.compareRouteLayoutCandidate(moved, [], legacy).jobs.map(j => j.job),
        baseline.jobs.map(j => j.job), 'legacy baselines keep frozen pairs');
    moved.primitives[1].connectionPoints = [];
    assert.throws(() => addon.compareRouteLayoutCandidate(moved, [], baseline), /endpoint.*missing/);
});

test('ordinary tree includes all terminals rather than the old two-job sample', () => {
    const p = problem(Array.from({ length: 5 }, (_, i) => primitive(`P${i}`, -8 + i * 3, 0, 'SIG')));
    const baseline = addon.prepareRouteLayoutComparison(p, ['P0'], []);
    assert.deepEqual(baseline.topologyNets, ['SIG']);
    assert.equal(baseline.jobs.length, 4);
    const terminals = new Set(baseline.jobs.flatMap(({ job }) => [job.sourceRef, job.targetRef]));
    assert.equal(terminals.size, 5);
    assert.equal(addon.compareRouteLayoutCandidate(p, [], baseline).unresolvedAfter, 0);
    const blocked = addon.compareRouteLayoutCandidate(p, [{ box: { left: -1.5, right: -0.5, top: -11, bottom: 11 } }], baseline);
    assert.equal(blocked.feasibilityOrder, 1);
    assert.ok(blocked.afterPenalty > blocked.beforePenalty);
});

test('complete net plans respect the shared budget without truncating admitted trees', () => {
    const primitives = ['A', 'B', 'C'].flatMap((net, row) => Array.from({ length: 8 }, (_, i) =>
        primitive(`${net}${i}`, -8 + i * 2, row * 2, net)));
    const baseline = addon.prepareRouteLayoutComparison(problem(primitives), ['A0', 'B0', 'C0'], []);
    assert.equal(baseline.topologyNets?.length, 2);
    assert.ok(baseline.jobs.length <= 16);
    for (const net of baseline.topologyNets!) {
        const jobs = baseline.jobs.filter(j => j.job.net === net);
        assert.equal(jobs.length, 7);
        assert.equal(new Set(jobs.flatMap(j => [j.job.sourceRef, j.job.targetRef])).size, 8);
    }
});

test('ESPower frozen USB snapshot: swap lowers cost on the same four obligations', () => {
    const { current, obstacles } = espowerSnapshot();
    const changed = ['post:R7', 'post:R8'];
    const before = addon.prepareRouteLayoutComparison(current, changed, obstacles);
    assert.equal(before.jobs.length, 4, JSON.stringify(before.jobs.map(j => j.job)));
    const swapped = structuredClone(current);
    const swappedObstacles = structuredClone(obstacles);
    const a = current.primitives.find(p => p.id === 'post:R7')!;
    const b = current.primitives.find(p => p.id === 'post:R8')!;
    for (const [from, to] of [[a, b], [b, a]]) {
        const moved = swapped.primitives.find(p => p.id === from.id)!;
        const dx = to.placements[0].x - from.placements[0].x;
        const dy = to.placements[0].y - from.placements[0].y;
        assert.equal(from.placements[0].rotate, to.placements[0].rotate);
        const translate = (box: typeof bounds) => { box.left += dx; box.right += dx; box.top += dy; box.bottom += dy; };
        translate(moved.bbox);
        moved.collisionBoxes.forEach(translate);
        moved.connectionPoints.forEach(pt => { pt.x += dx; pt.y += dy; });
        moved.placements.forEach(pt => { pt.x += dx; pt.y += dy; });
        swappedObstacles.filter(o => o.primitiveId === from.id).forEach(o => translate(o.box));
    }
    const after = addon.compareRouteLayoutCandidate(swapped, swappedObstacles, before);
    assert.deepEqual(after.jobs.map(j => j.job), before.jobs.map(j => j.job));
    assert.ok(after.afterPenalty < after.beforePenalty, JSON.stringify({ before, after }));
    assert.equal(after.unresolvedBefore, 0, JSON.stringify(before));
    assert.equal(after.unresolvedAfter, 0, JSON.stringify(after));
    const mcuBefore = before.jobs.filter(j => j.job.targetPrimitive === 'post:U1');
    const mcuAfter = after.jobs.filter(j => j.job.targetPrimitive === 'post:U1');
    assert.equal(mcuBefore.reduce((sum, j) => sum + j.vias, 0), 2);
    assert.equal(mcuAfter.reduce((sum, j) => sum + j.vias, 0), 0);
    assert.ok(after.jobs.every(j => j.expanded <= 4500));
    // Missing and budget-limited routes have no fabricated physical length.
    for (const job of [...before.jobs, ...after.jobs]) {
        assert.equal(job.physicalCost === null, job.status !== 'found');
    }
    console.log('ESPower USB comparison', JSON.stringify({ before: after.beforePenalty, after: after.afterPenalty,
        unresolvedBefore: after.unresolvedBefore, unresolvedAfter: after.unresolvedAfter,
        beforeJobs: before.jobs.map(j => [j.job.net, j.status, j.vias]),
        afterJobs: after.jobs.map(j => [j.job.net, j.status, j.vias]) }));
});

function espowerSnapshot() {
    const data = JSON.parse(readFileSync(new URL('./fixtures/pcb-route-cost/espower-usb.json', import.meta.url), 'utf8')) as {
        bounds: typeof bounds;
        bodies: Array<[string, 'top' | 'bottom', number, number, number, number, number]>;
        points: Array<[string, string, number, number]>;
        pads: Array<[string, string | null, 'top' | 'bottom' | null, number, number, number, number]>;
    };
    const primitives = data.bodies.map(([id, layer, left, right, top, bottom, rotate]) => {
        const p = primitive(`post:${id}`, (left + right) / 2, (top + bottom) / 2, '');
        p.bbox = { left, right, top, bottom }; p.collisionBoxes = [{ ...p.bbox }];
        p.width = right - left; p.height = bottom - top;
        p.placements[0].layer = layer; p.placements[0].rotate = rotate;
        p.placements[0].designator = id;
        p.connectionPoints = data.points.filter(([ref]) => ref.startsWith(`${id}.`)).map(([ref, net, x, y]) => ({ ref, net, x, y }));
        return p;
    });
    const current = problem(primitives, [['R8.1', 'U12.A6'], ['R7.1', 'U12.A7'], ['R7.2', 'U1.25'], ['R8.2', 'U1.26']]
        .map(([from, to]) => ({ ...relation(from, to), weight: 280 })));
    current.bounds = data.bounds; current.fullBoardBounds = data.bounds;
    const obstacles: NativeRoutingObstacle[] = data.pads.map(([ref, net, layer, left, right, top, bottom]) => ({
        ref, net: net ?? undefined, layer: layer ?? undefined,
        box: { left, right, top, bottom }, primitiveId: `post:${ref.split('.')[0]}`,
    }));
    return { current, obstacles };
}


test('ESPower post-processor accepts the R7/R8 swap despite a worse geometric score', () => {
    const { input, placements } = espowerRefinementInput();
    const result = refinePostPlacement(input, placements);
    assert.equal(result.moves.length, 1);
    const move = result.moves[0];
    assert.equal(move.kind, 'swap');
    assert.deepEqual(move.designators, ['R7', 'R8']);
    assert.equal(move.routeJobCount, 4);
    assert.equal(move.routeUnresolvedBefore, 0);
    assert.equal(move.routeUnresolvedAfter, 0);
    assert.ok(move.scoreAfter > move.scoreBefore, 'the route correction, not ordinary geometry, selects this swap');
    assert.ok(move.routePenaltyAfter < move.routePenaltyBefore);
    assert.ok(move.effectiveImprovement > 0);
    const pose = (items: Placement[], id: string) => items.find(p => p.designator === id)!;
    assert.equal(pose(result.placements, 'R7').x, pose(placements, 'R8').x);
    assert.equal(pose(result.placements, 'R8').x, pose(placements, 'R7').x);
    for (const fixed of placements.filter(p => p.designator !== 'R7' && p.designator !== 'R8')) {
        assert.deepEqual(pose(result.placements, fixed.designator), fixed);
    }
});

test('route improvement bounds preserve the eager refiner result', () => {
    const prepare = addon.prepareRouteLayoutComparison;
    const compare = addon.compareRouteLayoutCandidate;
    let calls = 0;
    addon.compareRouteLayoutCandidate = (...args) => { calls++; return compare(...args); };
    try {
        const { input, placements } = espowerRefinementInput();
        const bounded = refinePostPlacement(input, placements);
        const boundedCalls = calls;
        calls = 0;
        addon.prepareRouteLayoutComparison = (...args) => {
            const baseline = prepare(...args);
            delete baseline.maximumImprovement;
            return baseline;
        };
        const eager = refinePostPlacement(input, placements);
        assert.deepEqual(bounded, eager);
        assert.ok(boundedCalls <= calls);
    } finally {
        addon.prepareRouteLayoutComparison = prepare;
        addon.compareRouteLayoutCandidate = compare;
    }
});

function espowerRefinementInput(): { input: PlacementInput; placements: Placement[] } {
    const { current, obstacles } = espowerSnapshot();
    // Rebase the captured world-space boxes to zero-rotation footprints. This
    // keeps the complete routing geometry without needing a network/library lookup.
    const placements: Placement[] = current.primitives.map(p => ({ ...p.placements[0], rotate: 0 }));
    const components: PcbComponent[] = current.primitives.map(p => {
        const pose = placements.find(item => item.designator === p.placements[0].designator)!;
        const movable = pose.designator === 'R7' || pose.designator === 'R8';
        const pads = obstacles.filter(o => o.primitiveId === p.id);
        return {
            designator: pose.designator, value: pose.designator,
            block_name: movable ? 'usb_data' : pose.designator,
            part_uuid: movable ? 'same-22-ohm-part' : null, footprint_uuid: null, search_query: '',
            pins: pads.map(pad => ({ pin_number: pad.ref!.split('.')[1], name: pad.ref!.split('.')[1], signal_name: pad.net ?? 'NC' })),
            footprint: { name: pose.designator, width: p.width, height: p.height,
                pads: pads.map(pad => ({
                    pin_number: pad.ref!.split('.')[1], name: pad.ref!.split('.')[1],
                    x: ((pad.box.left + pad.box.right) / 2 - pose.x) * (pose.layer === 'bottom' ? -1 : 1),
                    y: (pad.box.top + pad.box.bottom) / 2 - pose.y,
                    width: pad.box.right - pad.box.left, height: pad.box.bottom - pad.box.top,
                    mount: pad.layer === undefined ? 'through_hole' : 'smd',
                })),
            },
            pcb: { role: 'passive', allowedLayers: [pose.layer], allowedRotations: [0],
                ...(!movable ? { fixedPlacement: {} } : {}) },
        };
    });
    const others = components.filter(c => c.designator !== 'R7' && c.designator !== 'R8').map(c => c.designator);
    const target = (ref: string) => ({ type: 'pin' as const, designator: ref.split('.')[0], pin_number: ref.split('.')[1] });
    const input: PlacementInput = {
        board: { coordinateSystem: 'centered', outline: { type: 'rect', width: 48, height: 32 },
            allowedLayers: ['top', 'bottom'], defaultLayer: 'top', clearances: { component: 0.25, edge: 0.25 } },
        boardHoles: [], constraintRegions: [], components, modules: [], paths: [],
        blocks: [{ name: 'usb_data', description: 'USB series pair', role: 'generic', component_designators: ['R7', 'R8'] },
            ...others.map(id => ({ name: id, description: id, role: 'generic' as const, component_designators: [id] }))],
        hints: current.relations.map(r => ({ relation: 'critical_pair', source: target(r.from.slice(4)),
            target: target(r.to.slice(4)), priority: 'critical', hard: false, weightMultiplier: 1 })),
        refineGroups: [
            { name: 'usb-swap', componentDesignators: ['R7', 'R8'], swap: true, rotateBy: [] },
            { name: 'fixed-context', componentDesignators: others, swap: false, rotateBy: [] },
        ],
        solverOptions: { ...defaultSolverOptions, localImproveIterations: 2, localImproveMinDelta: 0.001 },
    };
    return { input, placements };
}
