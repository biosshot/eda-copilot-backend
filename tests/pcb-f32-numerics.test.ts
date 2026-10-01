import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { add, f32, mul, roundPlacement, sinCosDegrees } from '../src/pcb-layout/f32.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { minimalProblem } from './helpers/native-board-problem.ts';
import { rotatePrimitive, type PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import { getLocalPointWorld, componentBodyBox, componentPadBox, dist } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { encodeNativeBoardPackProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-board-problem.ts';
import { applyNativeBoardPackSolution } from '../src/pcb-layout/pcb-auto-place-v2/native/apply-board-solution.ts';
import type { BoardPackParams } from '../src/pcb-layout/pcb-auto-place-v2/board-packer.ts';
import type { PlacementInput, Placement } from '../src/types/pcb/layout-model.ts';

test('F32 arithmetic rounds each operation, uses signed FTZ and canonical grid', () => {
    assert.equal(add(16_777_216, 1), 16_777_216);
    assert.equal(add(add(16_777_216, 1), -16_777_216), 0);
    assert.equal(mul(2 ** -149, 2 ** 126), 0);
    assert.ok(Object.is(f32(-(2 ** -149)), -0));
    assert.equal(mul(-(2 ** -126), 0.5), -0);
    assert.equal(roundPlacement(-0.0005), 0);
    for (let ticks = -1_024_000; ticks <= 1_024_000; ticks += 17) {
        const expected = Math.fround(ticks / 1000);
        assert.equal(roundPlacement(expected), expected === 0 ? 0 : expected);
    }
    assert.deepEqual(sinCosDegrees(0), [0, 1]);
    assert.deepEqual(sinCosDegrees(90), [1, 0]);
    assert.deepEqual(sinCosDegrees(180), [0, -1]);
    assert.deepEqual(sinCosDegrees(-90), [-1, 0]);
});

const fixture = JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/input.json', import.meta.url), 'utf8')) as PlacementInput;
function smallComponent() {
    const c = structuredClone(fixture.components[0]);
    c.designator = 'U1';c.block_name = 'core';
    c.footprint = { name:'small',width:0.01,height:0.002,bodyBox:{left:-0.005,right:0.005,top:-0.001,bottom:0.001},
        pads:[{pin_number:'1',name:'1',x:0.001,y:0,width:0.0002,height:0.0002}] };
    c.pcb = {...c.pcb,fixedPlacement:undefined,boardOverflow:undefined,occupiedAreas:undefined,generatedGeometry:[],edgePlace:undefined};
    return c;
}

test('real TS encoder localizes tiny bodies/pads before narrowing and applies returned locked poses', () => {
    const c = smallComponent(), addon=loadNativeBoardPacker();
    for (const shift of [0,1000,1e6,-1e6,1e9-100,-1e9+100]) {
        const pose:Placement={designator:'U1',x:shift+0.001234567,y:shift,rotate:90,layer:'top',score:0};
        const box=componentBodyBox(c,pose), pad=componentPadBox(pose,c.footprint.pads[0]);
        assert.ok(box.right-box.left>0.0019);
        assert.ok(pad.right-pad.left>0.00019);
        const a=getLocalPointWorld(pose,{x:0,y:0}),b=getLocalPointWorld(pose,{x:0.001,y:0});
        assert.ok(Math.abs(dist(a,b)-0.001)<1e-6);
        const p:PlacementPrimitive={...minimalProblem().primitives[0],sourceNodeId:'test',children:[],
            bbox:box,collisionBoxes:[box],width:0.002,height:0.01,placements:[pose],connectionPoints:[{...b,ref:'U1.1'}],locked:true};
        const input=encodeNativeBoardPackProblem({node:{} as BoardPackParams['node'],primitives:[p],relations:[],
            options:{grid:0.5,clearance:0,edgeClearance:0,bounds:{left:shift-1,right:shift+1,top:shift-1,bottom:shift+1},
                componentByDesignator:new Map([['U1',c]])}});
        assert.ok(input.components[0].bodyBox.right-input.components[0].bodyBox.left>0.0019);
        const result=addon.solveBoardPacked(input);
        assert.equal(result.rank.hardCount,0);
        assert.deepEqual(result.states[0].placements,[pose]);
        assert.deepEqual(applyNativeBoardPackSolution([p],result)[0].placements,[pose]);
    }
});

test('integer degree rotations retain low bits beyond the F32 integer range', () => {
    const angle=16_777_219;
    const source: PlacementPrimitive={...minimalProblem().primitives[0],kind:'component',
        sourceNodeId:'test',children:[],allowedOrientations:[angle],
        placements:[{designator:'A',x:0,y:0,rotate:angle,layer:'top',score:0}]};
    const rotated=rotatePrimitive(source,270);
    const modulo=(value:number)=>((value%360)+360)%360;
    assert.deepEqual(rotated.allowedOrientations,[modulo(angle-270)]);
    assert.equal(rotated.placements[0].rotate,modulo(angle+270));
});

// Translate transport coordinates without touching vectors, lengths or scores.
function translate(value: unknown, shift: number): void {
    if (Array.isArray(value)) { value.forEach(v => translate(v, shift)); return; }
    if (!value || typeof value !== 'object') return;
    const object = value as Record<string, unknown>;
    const box = ['left','right','top','bottom'].every(k => k in object);
    for (const [key, child] of Object.entries(object)) {
        if (typeof child === 'number' && (key === 'x' || key === 'y' || (box && ['left','right','top','bottom'].includes(key)))) object[key] = child + shift;
        else if (!['normal','anchorOffset','offset','overflow','boardOverflow','orientations'].includes(key)) translate(child, shift);
    }
}

test('native board uses F32 locally at large absolute offsets and preserves locked output', () => {
    const addon = loadNativeBoardPacker();
    assert.equal(addon.numericContract?.(), 'f32-rte-ftz-v1');
    const input = minimalProblem();
    input.primitives[0].locked = true;
    input.primitives[0].placements[0].x = 0.001234567;
    const base = addon.solveBoardPacked(structuredClone(input));
    for (const shift of [1e6, -1e6, 1e9-100, -1e9+100]) {
        const translated = structuredClone(input);
        // The shared source bbox is aliased by component geometry in this helper.
        translated.components[0].bodyBox = { ...translated.components[0].bodyBox };
        translate(translated, shift);
        const result = addon.solveBoardPacked(translated);
        assert.equal(result.rank.hardCount, base.rank.hardCount);
        assert.equal(result.states[0].placements[0].x, translated.primitives[0].placements[0].x);
        assert.equal(result.states[0].translationX, 0);
        assert.equal(result.states[0].translationY, 0);
    }
});
