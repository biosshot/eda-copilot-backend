import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { minimalProblem } from './helpers/native-board-problem.ts';
import { PCB_PLACEMENT_EPSILON } from '../src/pcb-layout/f32.ts';
import { boxInsideBoard, overlaps, PLACEMENT_EPSILON } from '../src/pcb-layout/pcb-auto-place/geometry.ts';

const native = createRequire(import.meta.url)('../native/node-addon/index.cjs');

test('clearance boundary candidates agree in actual CPU/GPU Board search', { skip: process.env.PCB_BOARD_GPU_TESTS !== '1' }, () => {
    const p = touchingPair();
    p.searchWidth = 4;
    const run = (backend: string) => {
        const child = spawnSync(process.execPath, ['-e',
            'const a=require("./native/node-addon/index.cjs");const p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));console.log(JSON.stringify(a.solveBoardPacked(p)));'], {
            input: JSON.stringify(p), encoding: 'utf8', windowsHide: true,
            env: { ...process.env, PCB_BOARD_BACKEND: backend, PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_PACKER_PROFILE: '1', PCB_BOARD_PACKER_THREADS: '1' },
        });
        assert.equal(child.status, 0, child.stderr);
        assert.doesNotMatch(child.stderr, /fallback|validation failed/);
        if (backend === 'cubecl') assert.match(child.stderr, /"backend":"cubecl"/);
        return JSON.parse(child.stdout);
    };
    const cpu = run('cpu'), gpu = run('cubecl');
    assert.equal(cpu.rank.hardCount, 0);
    assert.deepEqual(gpu, cpu);
});

function touchingPair(shortfall = 0) {
    const p = minimalProblem();
    p.bounds = p.fullBoardBounds = { left: -20, right: 20, top: -20, bottom: 20 };
    p.boardOutline = [{ x: -20, y: -20 }, { x: 20, y: -20 }, { x: 20, y: 20 }, { x: -20, y: 20 }];
    p.clearance = .35;
    // Exact boxes from the digital_input_1 pair which used to add 10M for
    // 5.96e-8 mm of clearance error. Bodies and primitive boxes both participate.
    const boxes = [
        { left: -.9039999842643738, right: 10.904000282287598, top: -2.575000047683716, bottom: 2.5250000953674316 },
        { left: -3.257999897003174 + shortfall, right: -1.253999948501587 + shortfall, top: -.5720000267028809, bottom: 4.156000137329102 },
    ];
    const source = p.primitives[0], component = p.components[0];
    p.primitives = boxes.map((bbox, i) => ({ ...structuredClone(source), id: `P${i}`, bbox,
        collisionBoxes: [bbox], width: bbox.right - bbox.left, height: bbox.bottom - bbox.top,
        placements: [{ ...source.placements[0], designator: `U${i}`, x: (bbox.left + bbox.right) / 2, y: (bbox.top + bbox.bottom) / 2 }],
        connectionPoints: [], pathPorts: [],
    }));
    p.components = boxes.map((bodyBox, i) => ({ ...structuredClone(component), primitiveId: `P${i}`, designator: `U${i}`, bodyBox }));
    p.componentConflict = [0, 1, 1, 0];
    p.componentPairClearance = [0, .35, .35, 0];
    return p;
}

test('placement distance tolerance agrees across native and TS contract', () => {
    assert.equal(native.placementGeometryEpsilon(), PCB_PLACEMENT_EPSILON);
    assert.equal(PLACEMENT_EPSILON, PCB_PLACEMENT_EPSILON);
});

test('USB tolerance boundary survives centering in native Block, Board and reports', () => {
    for (const dx of [0, -4.5]) for (const extraShortfall of [0, .001]) {
        const p = touchingPair();
        const boxes = [
            { left: -2.177999973 + dx, right: 2.180000067 + dx, top: -2.178999901, bottom: 2.178999901 },
            { left: 2.529000044 + dx - extraShortfall, right: 6.456999779 + dx - extraShortfall, top: -.679000020, bottom: 1.179000020 },
        ];
        boxes.forEach((box, i) => {
            p.primitives[i].bbox = p.components[i].bodyBox = box;
            p.primitives[i].collisionBoxes = [box];
        });
        const board = native.evaluateBoardQuality(p);
        const block = native.evaluateBlockQuality({ ...p, version: 4,
            collisionMode: 'components', hardCollisionMode: 'components', candidateBoxMode: 'bbox',
            components: p.components.map(c => ({ ...c, pinCount: 2, role: 'passive', powerComponent: false })),
            experiments: { routingMetric: 'off' },
        });
        assert.equal(board.rank.hardCount, extraShortfall > 0 ? 1 : 0);
        assert.equal(block.score > 1_000_000, extraShortfall > 0);
        assert.equal(overlaps(boxes[0], boxes[1], p.clearance), extraShortfall > 0);
    }
});

test('Block and Board ignore F32 boundary noise but preserve real clearance violations', () => {
    for (const shortfall of [0, .0005, .002]) {
        const p = touchingPair(shortfall);
        const board = native.evaluateBoardQuality(p);
        const block = native.evaluateBlockQuality({ ...p, version: 4,
            collisionMode: 'components', hardCollisionMode: 'components', candidateBoxMode: 'bbox',
            components: p.components.map(c => ({ ...c, pinCount: 2, role: 'passive', powerComponent: false })),
            experiments: { routingMetric: 'off' },
        });
        assert.equal(board.rank.hardCount, shortfall > .001 ? 1 : 0);
        assert.equal(block.score > 1_000_000, shortfall > .001);
        assert.equal(overlaps(p.primitives[0].bbox, p.primitives[1].bbox, p.clearance), shortfall > .001);
    }
});

test('polygon edge contact and epsilon-sized overflow stay legal; real overflow does not', () => {
    const outline = [{ x: -5, y: -5 }, { x: 5, y: -5 }, { x: 5, y: 5 }, { x: -5, y: 5 }];
    const board = { outline: { type: 'polygon' as const, width: 10, height: 10, points: outline } };
    for (const excess of [0, .0005, .002]) {
        const p = minimalProblem();
        const box = { left: 4, right: 5 + excess, top: -1, bottom: 1 };
        p.edgeClearance = 0;
        p.components[0].edgeClearance = 0;
        p.components[0].bodyBox = box;
        p.primitives[0].bbox = box;
        p.primitives[0].width = box.right - box.left;
        p.primitives[0].height = box.bottom - box.top;
        p.primitives[0].collisionBoxes = [box];
        assert.equal(native.evaluateBoardQuality(p).rank.hardCount, excess > .001 ? 1 : 0);
        assert.equal(boxInsideBoard(board as Parameters<typeof boxInsideBoard>[0], box), excess <= .001);
    }
});

test('permitted edge contact does not hide a concave notch whose mouth touches the box edge', () => {
    const p = minimalProblem();
    p.edgeClearance = p.components[0].edgeClearance = 0;
    p.boardOutline = [{x:-5,y:-5},{x:5,y:-5},{x:5,y:5},{x:1,y:5},{x:1,y:1},{x:0,y:0},{x:-1,y:1},{x:-1,y:5},{x:-5,y:5}];
    const box = {left:-2,right:2,top:-1,bottom:1};
    p.components[0].bodyBox = p.primitives[0].bbox = box;
    p.primitives[0].collisionBoxes = [box];
    assert.equal(native.evaluateBoardQuality(p).rank.hardCount, 1);
    const board = {outline:{type:'polygon' as const,width:10,height:10,points:p.boardOutline}};
    assert.equal(boxInsideBoard(board as Parameters<typeof boxInsideBoard>[0],box),false);
});
