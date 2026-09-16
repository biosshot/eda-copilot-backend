import assert from 'node:assert/strict';
import test from 'node:test';
import { runPcbLayoutDsl } from '../src/pcb-layout/pcb-layout-dsl/spec.ts';
import {
    attachLocalLayoutSeeds,
    extractLocalLayouts,
    instrumentLocalLayoutDsl,
    legalizeLocalLayout,
    prepareLocalLayoutPrimitives,
    stripLocalLayoutCarriers,
    validateLocalLayouts,
} from '../src/pcb-layout/pcb-auto-place-v2/local-layout.ts';
import type { PcbComponent, PlacementInput } from '../src/types/pcb/layout-model.ts';
import type { PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';

function primitive(id: string, x: number, y: number, width = 2, height = 2, rotate = 0): PlacementPrimitive {
    return {
        id,
        kind: 'component',
        label: id,
        sourceNodeId: `component:${id}`,
        locked: false,
        canRotate: true,
        allowedOrientations: [0, 90, 180, 270],
        bbox: { left: x - width / 2, right: x + width / 2, top: y - height / 2, bottom: y + height / 2 },
        collisionBoxes: [{ left: x - width / 2, right: x + width / 2, top: y - height / 2, bottom: y + height / 2 }],
        width,
        height,
        placements: [{ designator: id, x, y, rotate, layer: 'top', score: 0 }],
        connectionPoints: [{ ref: `${id}.1`, net: 'SIG', x, y }],
        children: [],
    };
}

function component(designator: string, blockName = 'filter'): PcbComponent {
    return {
        designator,
        value: 'TEST',
        pins: [{ pin_number: '1', name: '1', signal_name: 'SIG' }],
        block_name: blockName,
        search_query: '',
        part_uuid: null,
        footprint: {
            name: 'TEST',
            width: 2,
            height: 2,
            pads: [{ pin_number: '1', name: '1', x: 0, y: 0, width: 0.4, height: 0.4 }],
        },
        pcb: {
            role: 'passive',
            allowedLayers: ['top'],
            allowedRotations: [0, 90, 180, 270],
        },
    };
}

test('localLayout is sparse and survives DSL capture without changing placement rule schema', () => {
    const raw = runPcbLayoutDsl(instrumentLocalLayoutDsl(`
        board.rect(30, 20);
        block("filter", ["C1", "L1", "C2", "R1"], "power", {
            localLayout: {
                C1: { x: -2, y: 0, rotate: 90 },
                L1: { x: 0, y: 0 },
                C2: { x: 2, y: 0, rotate: 90 },
            },
        });
    `));
    const layouts = extractLocalLayouts(raw);
    assert.deepEqual(layouts.get('filter'), {
        C1: { x: -2, y: 0, rotate: 90 },
        L1: { x: 0, y: 0 },
        C2: { x: 2, y: 0, rotate: 90 },
    });
    assert.equal(layouts.get('filter')?.R1, undefined);
    assert.equal(stripLocalLayoutCarriers(raw).modules.some((module) => module.name.startsWith('__eda_local_layout__:')), false);
});

test('local legalizer moves only components involved in a violation', () => {
    const a = primitive('A', 0, 0, 2, 2);
    const c = primitive('C', 0.5, 0, 2, 2);
    const b = primitive('B', 20, 0, 2, 2);
    const result = legalizeLocalLayout([a, c, b], 0.5);
    const bResult = result.find((item) => item.id === 'B')!;
    assert.equal(bResult.placements[0].x, 20);
    assert.equal(bResult.placements[0].y, 0);
    const aResult = result.find((item) => item.id === 'A')!;
    const cResult = result.find((item) => item.id === 'C')!;
    assert.ok(cResult.bbox.left - aResult.bbox.right >= 0.5 - 1e-6);
});

test('local legalizer expands approximate distances and preserves requested ordering', () => {
    const left = primitive('C1', -2, 0, 6, 2);
    const center = primitive('L1', 0, 0, 6, 2);
    const right = primitive('C2', 2, 0, 6, 2);
    const result = legalizeLocalLayout([left, center, right], 0.5);
    const x = new Map(result.map((item) => [item.id, item.placements[0].x]));
    assert.ok(x.get('C1')! < x.get('L1')!);
    assert.ok(x.get('L1')! < x.get('C2')!);
    assert.ok(x.get('L1')! - x.get('C1')! > 2);
});

test('seeded components become one rigid macro while omitted members stay free', () => {
    const components = [component('C1'), component('L1'), component('R1')];
    const input = {
        components,
    } as unknown as PlacementInput;
    attachLocalLayoutSeeds(input, new Map([['filter', {
        C1: { x: -3, y: 0, rotate: 90 },
        L1: { x: 3, y: 0 },
    }]]));
    const map = new Map(components.map((item) => [item.designator, item]));
    const result = prepareLocalLayoutPrimitives(
        'filter',
        [primitive('C1', 0, 0), primitive('L1', 0, 0), primitive('R1', 0, 0)],
        map,
        0.5,
    );
    assert.equal(result.length, 2);
    const macro = result.find((item) => item.id === 'local-layout:filter')!;
    const free = result.find((item) => item.label === 'R1')!;
    assert.deepEqual(new Set(macro.placements.map((placement) => placement.designator)), new Set(['C1', 'L1']));
    assert.equal(free.placements[0].designator, 'R1');
    assert.equal(macro.placements.find((placement) => placement.designator === 'C1')?.rotate, 90);
});

test('localLayout rejects unknown designators and absolute-placement conflicts', () => {
    const rules = runPcbLayoutDsl(`
        board.rect(30, 20);
        block("filter", ["C1"], "power");
        component("C1").fixed({ x: 1, y: 1 });
    `);
    assert.throws(() => validateLocalLayouts(
        { components: [{ designator: 'C1' }] },
        rules,
        new Map([['filter', { C1: { x: 0, y: 0 }, X1: { x: 1, y: 0 } }]]),
    ), /cannot also use fixed|unknown component/);
});
