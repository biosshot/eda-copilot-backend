import assert from 'node:assert/strict';
import test from 'node:test';
import { boardSpacingPenalty, boardSpacingPolicy } from '../src/pcb-layout/pcb-auto-place-v2/board-spacing.ts';
import { routeLayoutProblem } from '../src/pcb-layout/pcb-auto-place-v2/post-place-route-score.ts';
import { encodeNativePostPlaceScoreProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-score.ts';
import type { PlacementInput, PcbComponent } from '../src/types/pcb/layout-model.ts';
import type { PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';

test('board comfort and final score use each side of a top-mounted display', () => {
    const components = [
        { designator: 'U6', footprint: { name: 'DISPLAY', width: 60, height: 60,
            bodyBox: { left: -30, right: 30, top: -30, bottom: 30 },
            pads: [{ pin_number: '1', x: -28, y: -28, width: 1, height: 1, mount: 'through_hole' }] },
            pcb: { allowedLayers: ['top'], allowedRotations: [0] } },
        { designator: 'R1', footprint: { name: 'R', width: 2, height: 1,
            pads: [{ pin_number: '1', x: 0, y: 0, width: 0.5, height: 0.5 }] },
            pcb: { allowedLayers: ['bottom'], allowedRotations: [0] } },
    ] as PcbComponent[];
    const input = { board: { coordinateSystem: 'centered', outline: { width: 100, height: 100 },
        defaultLayer: 'bottom', allowedLayers: ['top', 'bottom'], clearances: { component: 0.35, edge: 0.5 } },
        components, blocks: [], modules: [], hints: [], constraintRegions: [] } as PlacementInput;
    const roots = components.map((component) => ({ id: component.designator, locked: false,
        placements: [{ designator: component.designator, x: 0, y: 0, rotate: 0,
            layer: component.pcb.allowedLayers[0], score: 0 }] })) as PlacementPrimitive[];
    assert.ok(boardSpacingPolicy(input).density < 0.25);
    assert.equal(boardSpacingPenalty(input, roots, 3), 0);
});

test('board route input preserves pad sides inside one mixed component', () => {
    const component = { designator: 'RF1', footprint: { name: 'RF', width: 5, height: 4, pads: [
        { pin_number: '1', x: -1, y: 0, width: 1, height: 1, layer: 'top' },
        { pin_number: '2', x: 1, y: 0, width: 1, height: 1, layer: 'bottom' },
    ] }, pins: [{ pin_number: '1', signal_name: 'A' }, { pin_number: '2', signal_name: 'B' }],
    pcb: { allowedLayers: ['top'], allowedRotations: [0] } } as PcbComponent;
    const input = { board: { coordinateSystem: 'centered', outline: { width: 20, height: 20 },
        defaultLayer: 'top', allowedLayers: ['top', 'bottom'], clearances: { component: 0.35, edge: 0.5 } },
        components: [component], blocks: [], modules: [], hints: [], constraintRegions: [],
        solverOptions: { placementGridStep: 0.5, compactness: 'normal', ignoredRatsnestSignals: [] } } as PlacementInput;
    const pose = { designator: 'RF1', x: 0, y: 0, rotate: 0, layer: 'top' as const, score: 0 };
    const { problem, routingObstacles } = routeLayoutProblem(input, [pose], {
        relations: [], componentByDesignator: new Map([['RF1', component]]), clearanceResolver: () => 0.35,
    });
    assert.deepEqual(problem.primitives[0].connectionPointLayers, ['top', 'bottom']);
    assert.ok(problem.primitives[0].collisionBoxLayers?.includes('bottom'));
    assert.deepEqual(routingObstacles.map((obstacle) => obstacle.layer), ['top', 'bottom']);
    for (const layer of ['top', 'bottom'] as const) {
        const score = encodeNativePostPlaceScoreProblem(input, [{ ...pose, layer }]);
        const expected = layer === 'top' ? ['top', 'bottom'] : ['bottom', 'top'];
        assert.deepEqual(score.routingObstacles?.map(o => o.layer), expected);
        assert.deepEqual(score.nets.map(n => n.layers?.[0]), expected);
    }
});
