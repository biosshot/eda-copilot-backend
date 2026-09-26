import { refinePostPlacement as refinePostPlacementSerial } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { terminatePcbSubtreeWorkerPool } from '../src/pcb-layout/pcb-auto-place-v2/tree-subtree-pool.ts';
import assert from 'node:assert/strict';
import test from 'node:test';
import { refinePostPlacementAsync, refinePostPlacement, globalPostPlaceScore } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { defaultSolverOptions } from '../src/pcb-layout/pcb-auto-place/utils.ts';
import type { FootprintSpec, PcbComponent, Placement, PlacementInput } from '../src/types/pcb/layout-model.ts';

test('post-place swap can be selected purely by Micro-A* routability', () => {
    const input = routeAwareSwapInput();
    const before = routeAwareSwapPlacements();
    const swapped = before.map((placement) => {
        if (placement.designator === 'A') return { ...placement, x: 2 };
        if (placement.designator === 'B') return { ...placement, x: -2 };
        return placement;
    });

    // Euclidean/global post-place scoring sees the assignments as symmetric.
    assert.ok(Math.abs(globalPostPlaceScore(input, before) - globalPostPlaceScore(input, swapped)) < 0.001);

    const result = refinePostPlacement(input, before);
    const move = result.moves.find((candidate) => candidate.kind === 'swap');
    assert.ok(move, 'route-aware refinement should choose the A/B swap');
    assert.equal(placement(result.placements, 'A').x, 2);
    assert.equal(placement(result.placements, 'B').x, -2);
    assert.ok(move.routePenaltyBefore > move.routePenaltyAfter, JSON.stringify(move));
    assert.ok(move.effectiveImprovement > 0);
});

function routeAwareSwapInput(): PlacementInput {
    const pairFootprint = footprint('pair', 0.8, 0.8);
    const endpointFootprint = footprint('endpoint', 0.8, 0.8);
    const components = [
        component('A', 'pair', 'NA', pairFootprint, false, 'same-part'),
        component('B', 'pair', 'NB', pairFootprint, false, 'same-part'),
        component('TA', 'top-end', 'NA', endpointFootprint, true),
        component('TB', 'bottom-end', 'NB', endpointFootprint, true),
    ];
    return {
        board: {
            coordinateSystem: 'centered',
            outline: { type: 'rect', width: 16, height: 12 },
            defaultLayer: 'top',
            allowedLayers: ['top'],
            clearances: { component: 0.1, edge: 0.1 },
        },
        boardHoles: [{
            name: 'route-obstacle',
            x: -1,
            y: 2,
            drill: 0.5,
            diameter: 0.8,
            keepout: 1.2,
        }],
        constraintRegions: [],
        components,
        blocks: [
            block('pair', ['A', 'B']),
            block('top-end', ['TA']),
            block('bottom-end', ['TB']),
        ],
        modules: [],
        hints: [],
        paths: [],
        refineGroups: [],
        solverOptions: {
            ...defaultSolverOptions,
            ignoredRatsnestSignals: [],
            placementGridStep: 0.25,
            localImproveIterations: 4,
            localImproveMinDelta: 0.001,
        },
    };
}

function routeAwareSwapPlacements(): Placement[] {
    return [
        pose('A', -2, 0),
        pose('B', 2, 0),
        pose('TA', 0, 4),
        pose('TB', 0, -4),
    ];
}

function component(
    designator: string,
    blockName: string,
    signal: string,
    componentFootprint: FootprintSpec,
    fixed: boolean,
    partUuid: string | null = null,
): PcbComponent {
    return {
        designator,
        value: designator,
        pins: [{ pin_number: '1', name: '1', signal_name: signal }],
        block_name: blockName,
        search_query: '',
        part_uuid: partUuid,
        footprint_uuid: null,
        footprint: componentFootprint,
        pcb: {
            role: 'passive',
            allowedLayers: ['top'],
            allowedRotations: [0],
            ...(fixed ? { fixedPlacement: {} } : {}),
        },
    };
}

function footprint(name: string, width: number, height: number): FootprintSpec {
    return {
        name,
        width,
        height,
        pads: [{ pin_number: '1', name: '1', x: 0, y: 0, width: 0.2, height: 0.2 }],
    };
}

function block(name: string, componentDesignators: string[]): PlacementInput['blocks'][number] {
    return { name, description: name, component_designators: componentDesignators, role: 'generic' };
}

function pose(designator: string, x: number, y: number): Placement {
    return { designator, x, y, rotate: 0, layer: 'top', score: 0 };
}

function placement(placements: Placement[], designator: string) {
    const found = placements.find((item) => item.designator === designator);
    assert.ok(found, `${designator} must be placed`);
    return found;
}

// Compares serial and parallel Rust evaluation.
test('Rust threads match serial Rust moves and scores', async () => {
    process.env.PCB_POST_PLACE_THREADS = '3';
    try {
        const input = routeAwareSwapInput();
        const before = routeAwareSwapPlacements();
        const { profile: serialProfile, ...serial } = refinePostPlacementSerial(input, before);
        for (let repeat = 0; repeat < 2; repeat++) {
            const { profile, ...parallel } = await refinePostPlacementAsync(input, before);
            assert.deepEqual(parallel, serial);
            const { profile: nativeProfile, ...nativeSerial } = refinePostPlacement(input, before);
            assert.deepEqual(nativeSerial, serial);
            assert.ok(profile.workers > 1);
            assert.deepEqual(profile.iterations.map(i => i.candidates), serialProfile.iterations.map(i => i.candidates));
        }
    } finally {
        await terminatePcbSubtreeWorkerPool();
        delete process.env.PCB_POST_PLACE_THREADS;
    }
});
