import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { minimalProblem } from './helpers/native-board-problem.ts';
import type { NativeBlockSolveProblemV4, NativeBlockSolveSolutionV4 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';

const require = createRequire(import.meta.url);
const addon = resolve('native/pcb-board-packer', require('../native/pcb-board-packer/platform.cjs').nativeFilename());
const gpuEnabled = process.env.PCB_BLOCK_GPU_TESTS === '1';

test('GPU batch lends one CPU slot while retaining ordered results and full-call recovery', { skip: !gpuEnabled }, () => {
    const first=problem();const second=problem();second.primitives[0].locked=true;
    const problems=[first,second];
    const invoke=(backend:string,extra:Record<string,string>={})=>{
        const child=spawnSync(process.execPath,['--input-type=commonjs','-e',
            'const a=require(process.argv[1]);const p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));console.log(JSON.stringify(a.solveBlockPrimitivesBatch(p,1)));',addon],{
            input:JSON.stringify(problems),encoding:'utf8',windowsHide:true,maxBuffer:8*1024*1024,timeout:180_000,
            env:{...process.env,PCB_BLOCK_BACKEND:backend,PCB_BLOCK_SOLVER_PROFILE:'1',...extra},
        });
        assert.equal(child.status,0,child.stderr||String(child.error));
        const schedulerLine=child.stderr.split('\n').find(line=>line.startsWith('[block-cpu-scheduler] '));
        assert.ok(schedulerLine,'batch scheduler telemetry');
        return {values:JSON.parse(child.stdout) as NativeBlockSolveSolutionV4[],log:child.stderr,
            scheduler:JSON.parse(schedulerLine.slice('[block-cpu-scheduler] '.length))};
    };
    const cpu=invoke('cpu');const gpu=invoke('cubecl');
    assert.equal(gpu.scheduler.threads,2);assert.equal(gpu.scheduler.cpu.activeLimit,1);
    assert.equal(gpu.scheduler.cpu.peakActive,1);assert.equal(gpu.scheduler.cpu.active,0);
    assert.ok(gpu.scheduler.cpu.suspensions>0);
    assert.doesNotMatch(gpu.log,/block-gpu-fallback/);
    for(let i=0;i<problems.length;i++) {
        checkGeometry(problems[i],gpu.values[i]);
        assert.deepEqual(gpu.values[i].checkpoints.map(c=>c.stage),cpu.values[i].checkpoints.map(c=>c.stage));
        assert.ok(gpu.values[i].rank.score<=cpu.values[i].rank.score*1.01+.01);
    }
    const locked=second.primitives[0].id;
    assert.deepEqual(gpu.values[1].states.find(s=>s.primitiveId===locked)?.placements,
        cpu.values[1].states.find(s=>s.primitiveId===locked)?.placements);
    const failed=invoke('cubecl',{PCB_BLOCK_GPU_FAIL_AT:'singles'});
    assert.deepEqual(failed.values,cpu.values);
    assert.match(failed.log,/block-gpu-fallback/);
    assert.equal(failed.scheduler.cpu.peakActive,1);assert.equal(failed.scheduler.cpu.active,0);
});

function problem(): NativeBlockSolveProblemV4 {
    const source = minimalProblem();
    const primitives = [0, 1, 2].map(i => {
        const p = structuredClone(source.primitives[0]);
        p.id = `primitive:P${i}`; p.label = `P${i}`;
        p.sourceNodeId = `tree:block:P${i}`; p.sourceNodeIds = [p.sourceNodeId];
        p.allowedOrientations = [0, 90];
        p.placements[0].designator = `C${i}`;
        p.connectionPoints = [{ ref: `C${i}.1`, net: 'SIG', x: .75, y: 0 }];
        p.pathPorts = [{ pathId: 'path', order: i, ref: `C${i}.1`, role: i === 0 ? 'source' : 'target', x: .75, y: 0, normal: { x: 1, y: 0 } }];
        return p;
    });
    const components = primitives.map((p, i) => ({ designator: `C${i}`, primitiveId: p.id, blockName: p.label,
        layer: 'top' as const, bodyBox: { left: -.5, right: .5, top: -.25, bottom: .25 },
        throughHoleBoxes: i === 2 ? [{ left: -.25, right: .25, top: -.25, bottom: .25 }] : [],
        pinCount: 2, powerComponent: i === 1, role: 'passive' }));
    primitives[0].placements.push({ designator: 'C3', x: 0, y: 1, rotate: 0, layer: 'top', score: 0 });
    primitives[0].bbox.bottom = 1.5; primitives[0].height = 2;
    primitives[0].collisionBoxes.push({ left: -.5, right: .5, top: .75, bottom: 1.25 });
    components.push({ ...structuredClone(components[0]), designator: 'C3', bodyBox: { left: -.5, right: .5, top: .75, bottom: 1.25 } });
    const n = components.length;
    return { version: 4, grid: .5, clearance: .25, searchWidth: 1, compactness: 'normal',
        collisionMode: 'components', hardCollisionMode: 'components', candidateBoxMode: 'collision',
        primitives, components, obstacles: [], routingObstacles: [],
        componentConflict: Array.from({ length: n * n }, (_, i) => Number(Math.floor(i / n) !== i % n)),
        componentPairClearance: Array.from({ length: n * n }, (_, i) => Math.floor(i / n) === i % n ? 0 : .25),
        relations: [{ id: 'facing', kind: 'critical_pair', from: 'C0.1', to: 'C1.1', hard: false,
            effect: 'move_both', satelliteAnchor: false, preferFacingPads: true, pathId: 'path', pathShape: 'straight', weight: 2, maxDistance: 4 }],
        experiments: { routingMetric: 'geometric', localAccess: true, stableNetWeight: true, padCrossings: true,
            netCandidates: false, longNets: true, pairSwaps: true, reinsertPair: true } };
}

function run(p: NativeBlockSolveProblemV4, backend: string, env: Record<string, string> = {}) {
    const child = spawnSync(process.execPath, ['--input-type=commonjs', '-e',
        'const fs=require("fs"),addon=require(process.argv[1]);console.log(JSON.stringify(addon.solveBlockPrimitives(JSON.parse(fs.readFileSync(0,"utf8")))));', addon], {
        input: JSON.stringify(p), encoding: 'utf8', windowsHide: true, maxBuffer: 4 * 1024 * 1024,
        env: { ...process.env, PCB_BLOCK_BACKEND: backend, PCB_BLOCK_SOLVER_PROFILE: '1', ...env },
    });
    assert.equal(child.status, 0, child.stderr || String(child.error));
    return { solution: JSON.parse(child.stdout) as NativeBlockSolveSolutionV4, log: child.stderr };
}

function checkGeometry(p: NativeBlockSolveProblemV4, s: NativeBlockSolveSolutionV4) {
    assert.equal(s.rank.hardCount, 0);
    const placements = s.states.flatMap(s => s.placements ?? []);
    const boxes = p.components.map(c => {
        const source = p.primitives.flatMap(p => p.placements).find(p => p.designator === c.designator)!;
        const target = placements.find(p => p.designator === c.designator)!;
        const angle = ((target.rotate - source.rotate) % 360 + 360) % 360;
        const sin = angle === 90 ? 1 : angle === 270 ? -1 : 0;
        const cos = angle === 0 ? 1 : angle === 180 ? -1 : 0;
        const corners = [c.bodyBox.left, c.bodyBox.right].flatMap(x => [c.bodyBox.top, c.bodyBox.bottom].map(y => ({
            x: target.x + (x - source.x) * cos - (y - source.y) * sin,
            y: target.y + (x - source.x) * sin + (y - source.y) * cos,
        })));
        return { left: Math.min(...corners.map(p => p.x)), right: Math.max(...corners.map(p => p.x)), top: Math.min(...corners.map(p => p.y)), bottom: Math.max(...corners.map(p => p.y)) };
    });
    for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i], b = boxes[j];
        const gap = Math.max(b.left - a.right, a.left - b.right, b.top - a.bottom, a.top - b.bottom);
        assert.ok(gap + .002 >= p.componentPairClearance[i * boxes.length + j], `${i}/${j}: ${gap}`);
    }
    if (p.world) for (let i = 0; i < boxes.length; i++) {
        if (p.primitives.find(primitive => primitive.id === p.components[i].primitiveId)?.locked) continue;
        const b = boxes[i], w = p.world.bounds, c = p.world.edgeClearance;
        assert.ok(b.left >= w.left + c - .002 && b.right <= w.right - c + .002
            && b.top >= w.top + c - .002 && b.bottom <= w.bottom - c + .002);
        for (const obstacle of p.world.obstacles) {
            if (obstacle.designator !== p.components[i].designator) continue;
            const a = obstacle.box;
            const gap = Math.max(b.left - a.right, a.left - b.right, b.top - a.bottom, a.top - b.bottom);
            assert.ok(gap + .002 >= obstacle.clearance);
        }
    }
}

test('block GPU unavailable preserves the complete CPU result for a composite block', () => {
    const p = problem();
    const cpu = run(p, 'cpu');
    const disabled = run(p, 'cubecl', { PCB_BLOCK_GPU_DISABLED: '1' });
    assert.deepEqual(disabled.solution, cpu.solution);
    assert.match(disabled.log, /GPU disabled/);
    assert.doesNotMatch(disabled.log, /\[block-gpu-runtime\]/);
    checkGeometry(p, cpu.solution);
});

test('GPU composite, through-hole, path and facing scores preserve valid full-cycle quality', { skip: !gpuEnabled }, () => {
    const p = problem();
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BLOCK_GPU_BATCH_SIZE: '32' });
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /block-gpu-fallback/);
    for (const result of [cpu, gpu]) checkGeometry(p, result.solution);
    assert.ok(gpu.solution.rank.score <= cpu.solution.rank.score * 1.01 + .01);
    assert.deepEqual(gpu.solution.checkpoints.map(c => c.stage), cpu.solution.checkpoints.map(c => c.stage));
});

test('GPU block failure after singles discards partial checkpoints and repeats on CPU', { skip: !gpuEnabled }, () => {
    const p = problem();
    const cpu = run(p, 'cpu');
    const recovered = run(p, 'cubecl', { PCB_BLOCK_GPU_FAIL_AT: 'singles' });
    assert.match(recovered.log, /injected GPU failure after singles/);
    assert.deepEqual(recovered.solution, cpu.solution);
});

test('GPU bounded block retains locked poses and world constraints', { skip: !gpuEnabled }, () => {
    const p = problem();
    p.primitives[0].locked = true;
    p.bounds = { left: -8, right: 8, top: -8, bottom: 8 };
    p.world = { bounds: p.bounds, outline: [{ x: -8, y: -8 }, { x: 8, y: -8 }, { x: 8, y: 8 }, { x: -8, y: 8 }], edgeClearance: .25,
        obstacles: [{ designator: 'C1', box: { left: 4, right: 6, top: 4, bottom: 6 }, clearance: .25 }] };
    p.targetWidth = 8; p.targetHeight = 8;
    p.relations.push({ ...p.relations[0], id: 'anchored', from: 'C2.1', to: 'anchor:board.right', weight: .2 });
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl');
    assert.doesNotMatch(gpu.log, /block-gpu-fallback/);
    assert.match(gpu.log, /"backend":"cubecl"/);
    checkGeometry(p, gpu.solution);
    const fixed = gpu.solution.states.find(s => s.primitiveId === p.primitives[0].id)!;
    assert.deepEqual(fixed.placements, cpu.solution.states.find(s => s.primitiveId === fixed.primitiveId)!.placements);
    assert.ok(gpu.solution.rank.score <= cpu.solution.rank.score * 1.01 + .01);
});
