import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { refineBlockPrimitives } from '../src/pcb-layout/pcb-auto-place-v2/block-post-refiner.ts';
import { createClearanceResolver } from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import { applyNativeBoardPackSolution } from '../src/pcb-layout/pcb-auto-place-v2/native/apply-board-solution.ts';
import { getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import type { PlacementInput } from '../src/types/pcb/layout-model.ts';
import type { PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';

function fixture() {
    const input: PlacementInput = JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/input.json', import.meta.url), 'utf8'));
    const captured = JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/block-23.json', import.meta.url), 'utf8'));
    const source: PlacementPrimitive[] = captured.problem.primitives.map((p: PlacementPrimitive) => ({ ...p,
        kind: 'component', sourceNodeId: p.id, children: [],
    }));
    return { input, primitives: applyNativeBoardPackSolution(source, captured.solution, 2) };
}

test('block postrefine improves captured USB locally and transforms pad metadata consistently', () => {
    const { input, primitives } = fixture();
    const before = structuredClone(primitives);
    const result = refineBlockPrimitives(input, primitives, createClearanceResolver(input));
    assert.ok(result.moves > 0, 'captured USB must have a local swap opportunity');
    assert.deepEqual(primitives, before, 'input primitives are immutable');
    assert.deepEqual(result.primitives.map(p => p.id).sort(), primitives.map(p => p.id).sort());
    for (const p of result.primitives) for (const cp of p.connectionPoints) {
        const [name, pin] = cp.ref.split('.');
        const component = input.components.find(c => c.designator === name)!;
        const point = getPadWorld(component, p.placements[0], pin)!;
        assert.ok(Math.abs(point.x - cp.x) <= .002 && Math.abs(point.y - cp.y) <= .002, cp.ref);
    }
});

test('block postrefine preserves compound primitives even with an explicit refine group', () => {
    const { input, primitives } = fixture();
    const frozen = primitives.filter(p => ['R32', 'R34'].includes(p.placements[0].designator));
    for (const p of frozen) p.kind = 'island';
    input.refineGroups = [{ name: 'all-usb', componentDesignators: primitives.flatMap(p => p.placements.map(q => q.designator)), swap: true, rotateBy: [180] }];
    const result = refineBlockPrimitives(input, primitives, createClearanceResolver(input));
    for (const p of frozen) assert.deepEqual(result.primitives.find(q => q.id === p.id), p);
});

test('block postrefine preserves fixed and orientation-restricted primitives', () => {
    const { input, primitives } = fixture();
    const fixed = primitives.find(p => p.placements[0].designator === 'R32')!;
    input.components.find(c => c.designator === 'R32')!.pcb.fixedPlacement = fixed.placements[0];
    for (const p of primitives) { p.allowedOrientations = [0]; p.canRotate = false; }
    const result = refineBlockPrimitives(input, primitives, createClearanceResolver(input));
    assert.deepEqual(result.primitives.find(p => p.id === fixed.id), fixed);
    for (const p of result.primitives) assert.equal(p.placements[0].rotate, primitives.find(q => q.id === p.id)!.placements[0].rotate);
});
