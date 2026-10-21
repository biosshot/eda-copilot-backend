import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { refineBoardAlignment } from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import { componentBox } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import type { Placement, PlacementInput } from '../src/types/pcb/layout-model.ts';
import type { PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import { referenceBoardAlignment } from './helpers/board-alignment-reference.ts';

type Case = { input: PlacementInput; roots: PlacementPrimitive[]; placements: Placement[] };
const gz = (file: string) => JSON.parse(gunzipSync(readFileSync(file)).toString());
const telemetryInput = () => JSON.parse(readFileSync('docs/experimental/pcb/global-placement-2026-09-27/Telemetry/input.json', 'utf8')) as PlacementInput;
const plain = (value: unknown) => JSON.parse(JSON.stringify(value));

/** Placements, moves, counters, errors and diagnostics, compared in full. */
function assertParity({ input, roots, placements }: Case, threads = [1, 4]) {
    const original = JSON.stringify({ input, roots, placements });
    const reference = plain(referenceBoardAlignment(input, roots, placements));
    for (const t of threads) assert.deepStrictEqual(plain(refineBoardAlignment(input, roots, placements, t)), reference, `${t} native thread(s)`);
    assert.equal(JSON.stringify({ input, roots, placements }), original, 'inputs must not be mutated');
    return reference as ReturnType<typeof referenceBoardAlignment>;
}

function simple(x = 10): Case {
    const input = telemetryInput();
    input.components = input.components.filter(c => ['L3', 'L4'].includes(c.designator));
    input.components.forEach(c => c.pins.forEach(p => p.signal_name = ''));
    input.blocks = input.components.map(c => ({ name: c.block_name!, role: 'power', description: '', component_designators: [c.designator] }));
    input.modules = []; input.hints = []; input.paths = []; input.refineGroups = []; input.boardHoles = []; input.constraintRegions = [];
    input.board.outline = { type: 'rect', width: 100, height: 100 };
    const placements: Placement[] = input.components.map((c, i) => ({ designator: c.designator, x: i ? x : -10, y: i ? 1 : 0, rotate: 0, layer: 'top', score: 0 }));
    const roots: PlacementPrimitive[] = placements.map((p, i) => {
        const bbox = componentBox(input.components[i], p);
        return { id: p.designator, label: p.designator, sourceNodeId: p.designator, kind: 'block', bbox, width: bbox.right - bbox.left, height: bbox.bottom - bbox.top,
            placements: [p], children: [], connectionPoints: [] };
    });
    return { input, roots, placements };
}
function anchored(x?: number): Case {
    const c = simple(x);
    c.roots[0].locked = true;
    c.input.components[0].pcb.fixedPlacement = { ...c.placements[0] };
    return c;
}

test('native pass matches the TypeScript pass on a free pair', () => {
    const result = assertParity(simple());
    assert.ok(result.moves.length > 0);
});

test('native pass matches the TypeScript pass when a keepout rejects the target axis', () => {
    const c = anchored();
    c.input.constraintRegions = [{ name: 'under-right', layers: ['top'], allowBlocks: [], box: { left: 0, right: 30, top: -20, bottom: -5.8 } }];
    assert.ok(assertParity(c).rejected.geometry > 0);
});

test('native pass matches the TypeScript pass when a mandatory hint would deepen its violation', () => {
    // Aligning L4 with L3 moves it away from a fixed U9 whose mandatory, nearly
    // weightless distance limit is already exceeded.
    const c = anchored();
    const u9 = { ...structuredClone(c.input.components[0]), designator: 'U9', block_name: 'u9' };
    const pose: Placement = { designator: 'U9', x: 10, y: 21, rotate: 0, layer: 'top', score: 0 };
    u9.pcb.fixedPlacement = { ...pose };
    c.input.components.push(u9);
    c.input.blocks.push({ name: 'u9', role: 'power', description: '', component_designators: ['U9'] });
    c.placements.push(pose);
    const bbox = componentBox(u9, pose);
    c.roots.push({ id: 'U9', label: 'U9', sourceNodeId: 'U9', kind: 'block', locked: true, bbox, width: bbox.right - bbox.left, height: bbox.bottom - bbox.top,
        placements: [pose], children: [], connectionPoints: [] });
    c.input.hints = [{ relation: 'critical_pair', source: { type: 'component', designator: 'L4' }, target: { type: 'component', designator: 'U9' },
        priority: 'critical', hard: true, maxDistance: 19, weightMultiplier: 1e-6 }];
    assert.ok(assertParity(c).rejected.hardHint > 0);
});

test('native pass matches the TypeScript pass when the native validator rejects a block anchor gap', () => {
    const c = anchored();
    // The anchor point lies 1 mm beyond L4; aligning L4 to L3 moves it away.
    const box = componentBox(c.input.components[1], c.placements[1]);
    const l4 = c.input.blocks.find(b => b.component_designators.includes('L4'))!;
    Object.assign(l4, { anchor: { type: 'component', designator: 'L3' }, anchorOffset: { x: 20, y: box.bottom + 1 }, hardAnchor: true, maxAnchorGap: 1.2 });
    assert.ok(assertParity(c).rejected.native > 0);
});

// Review counterexample: four identical parts, the first two in a module with a
// 13.5 mm hard height limit. A later move is valid only on top of an earlier
// accepted one, so validation must see the accumulated layout.
test('native validation keeps earlier accepted moves', () => {
    const result = assertParity(gz('tests/fixtures/board-alignment/consecutive-module-moves.json.gz'));
    assert.equal(result.moves.length, 5);
    assert.equal(result.rejected.native, 0);
    assert.deepEqual(result.placements.map(p => p.y), [1.5, 1.625, 1.875, 2]);
});

test('native pass matches the TypeScript pass on STM32G0, with accepted moves', () => {
    const result = assertParity(gz('tests/fixtures/board-alignment/STM32G0_CAN_RS485.json.gz'));
    assert.ok(result.moves.length > 0 && result.rejected.geometry > 0 && result.rejected.electrical > 0);
});

test('native pass matches the TypeScript pass on the historical Telemetry layout', () => {
    const saved = gz('tests/fixtures/block-placement/Telemetry/board-alignment-geometric.json.gz');
    const result = assertParity({ input: telemetryInput(), roots: saved.roots, placements: saved.placements }, [1, 3]);
    assert.ok(result.rejected.netLength > 0 && result.rejected.geometry > 0 && result.rejected.objective > 0);
});
