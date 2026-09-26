import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { refinePostPlacement, refinePostPlacementAsync } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { terminatePcbSubtreeWorkerPool } from '../src/pcb-layout/pcb-auto-place-v2/tree-subtree-pool.ts';
import { defaultSolverOptions } from '../src/pcb-layout/pcb-auto-place/utils.ts';
import type { PlacementInput, Placement, PcbComponent, FootprintSpec } from '../src/types/pcb/layout-model.ts';
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

const input = routeAwareSwapInput();
input.components = []; input.blocks = []; input.boardHoles = [];
input.board.outline = { type: 'rect', width: 140, height: 110 };
input.solverOptions.localImproveIterations = 2;
const placements: Placement[] = [];
for (let i = 0; i < 64; i++) {
    const offsetX = ((i % 8) - 3.5) * 16, offsetY = (Math.floor(i / 8) - 3.5) * 12;
    const tile = routeAwareSwapInput();
    for (const c of tile.components) {
        c.designator += `_${i}`; c.block_name += `_${i}`;
        c.pins.forEach(p => { p.signal_name += `_${i}`; });
        if (c.pcb.fixedPlacement) c.pcb.edgeMount = { edge: 'left' };
        input.components.push(c);
    }
    for (const b of tile.blocks) {
        b.name += `_${i}`; b.component_designators = b.component_designators.map(d => `${d}_${i}`);
        input.blocks.push(b);
    }
    input.boardHoles!.push(...tile.boardHoles!.map(h => ({ ...h, name: `${h.name}_${i}`, x: h.x + offsetX, y: h.y + offsetY })));
    placements.push(...routeAwareSwapPlacements().map(p => ({ ...p, designator: `${p.designator}_${i}`, x: p.x + offsetX, y: p.y + offsetY })));
}
process.env.PCB_POST_PLACE_THREADS = '2';
try {
    const { profile: serialProfile, ...serial } = refinePostPlacement(input, placements);
    const { profile: parallelProfile, ...parallel } = await refinePostPlacementAsync(input, placements);
    assert.deepEqual(parallel, serial);
    const report = { components: input.components.length, serial: serialProfile, parallel: parallelProfile, identical: true };
    if (process.argv[2]) writeFileSync(process.argv[2], JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report));
} finally { await terminatePcbSubtreeWorkerPool(); }
