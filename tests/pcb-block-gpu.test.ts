import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { addOrdinaryBus, minimalProblem } from './helpers/native-board-problem.ts';
import type { NativeBlockSolveProblemV4, NativeBlockSolveSolutionV4 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';

const require = createRequire(import.meta.url);
const addon = resolve('native/node-addon', require('../native/node-addon/platform.cjs').nativeFilename());
const gpuEnabled = process.env.PCB_BLOCK_GPU_TESTS === '1';

test('USB half-grid bounds retain actual placement origins in CPU/GPU search', { skip: !gpuEnabled }, () => {
    const captured = JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/block-23.json', import.meta.url), 'utf8'));
    const p = { ...captured.problem, version: 4, searchWidth: 1, experiments: {} } as NativeBlockSolveProblemV4;
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BLOCK_GPU_VERIFY: '1' });
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /block-gpu-fallback|validation failed/);
    checkGeometry(p, cpu.solution);
    checkGeometry(p, gpu.solution);
    assert.deepEqual(gpu.solution.checkpoints.map(c => c.stage), ['beam', 'singles', 'pairs']);
    assert.ok(gpu.solution.rank.score <= cpu.solution.rank.score * 1.01 + .01);
});

test('GPU hull buffers cover more than 64 points with BBOX collision aliases', { skip: !gpuEnabled }, () => {
    const source = minimalProblem().primitives[0];
    const primitives = Array.from({ length: 65 }, (_, i) => {
        const x = i < 64 ? (i - 32) * 3 : 0;
        const y = i < 64 ? (i - 32) ** 2 : -5;
        const bbox = { left: x - .5, right: x + .5, top: y - .5, bottom: y + .5 };
        return { ...structuredClone(source), id: `primitive:C${i}`, label: `C${i}`,
            sourceNodeId: `tree:component:C${i}`, sourceNodeIds: [`tree:component:C${i}`],
            locked: i < 64, canRotate: false, allowedOrientations: [0],
            bbox, collisionBoxes: [bbox], width: 1, height: 1, connectionPoints: [],
            placements: [{ designator: `C${i}`, x, y, rotate: 0, layer: 'top' as const, score: 0 }] };
    });
    const p: NativeBlockSolveProblemV4 = { ...problem(), primitives,
        components: primitives.map(v => ({ designator: v.label, primitiveId: v.id, blockName: v.label,
            layer: 'top', bodyBox: v.bbox, throughHoleBoxes: [], pinCount: 0, powerComponent: false, role: 'passive' })),
        relations: [], experiments: { routingMetric: 'geometric' },
        componentConflict: Array.from({ length: 65 * 65 }, (_, i) => Number(Math.floor(i / 65) !== i % 65)),
        componentPairClearance: Array.from({ length: 65 * 65 }, (_, i) => Math.floor(i / 65) === i % 65 ? 0 : .25) };
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BLOCK_GPU_VERIFY_PRUNE: '1' });
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /block-gpu-fallback|panicked/);
    checkGeometry(p, gpu.solution);
    assert.deepEqual(gpu.solution, cpu.solution);
});

test('geometric route GPU is used by block search and failures replay the full CPU call', { skip: !gpuEnabled }, () => {
    const p = problem();
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_ROUTE_GPU_VERIFY: '1', PCB_ROUTE_GPU_PROFILE: '1', PCB_ROUTE_GPU_FORCE: '1' });
    assert.match(gpu.log, /\[geometric-route-gpu\]/);
    assert.doesNotMatch(gpu.log, /block-gpu-fallback/);
    assert.deepEqual(gpu.solution, cpu.solution);
    const small = run(p, 'cubecl', { PCB_ROUTE_GPU_PROFILE: '1' });
    assert.match(small.log, /\[geometric-route-cpu\]/);
    assert.doesNotMatch(small.log, /block-gpu-fallback/);
    assert.deepEqual(small.solution, cpu.solution);
    const failed = run(p, 'cubecl', { PCB_ROUTE_GPU_FAIL: '1' });
    assert.match(failed.log, /injected geometric route failure/);
    assert.match(failed.log, /block-gpu-fallback/);
    assert.deepEqual(failed.solution, cpu.solution);
});

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
        experiments: { routingMetric: 'geometric', localAccess: true, stableNetWeight: true,
            netCandidates: false, longNets: true, pairSwaps: true, reinsertPair: true } };
}

function run(p: NativeBlockSolveProblemV4, backend: string, env: Record<string, string> = {}) {
    const child = spawnSync(process.execPath, ['--input-type=commonjs', '-e',
        'const fs=require("fs"),addon=require(process.argv[1]);console.log(JSON.stringify(addon.solveBlockPrimitives(JSON.parse(fs.readFileSync(0,"utf8")))));', addon], {
        input: JSON.stringify(p), encoding: 'utf8', windowsHide: true, maxBuffer: 4 * 1024 * 1024, timeout: 60_000,
        env: { ...process.env, PCB_BLOCK_BACKEND: backend, PCB_BLOCK_SOLVER_PROFILE: '1', ...env },
    });
    assert.equal(child.status, 0, child.stderr || String(child.error));
    return { solution: JSON.parse(child.stdout) as NativeBlockSolveSolutionV4, log: child.stderr };
}

test('block completes on CPU with both GPU drivers unavailable or GPU disabled', () => {
    const p = problem();
    const cpu = run(p, 'cpu');
    checkGeometry(p, cpu.solution);
    for (const backend of ['auto', 'cubecl']) for (const options of [
        { PCB_GPU_RUNTIME: 'auto', PCB_GPU_CUDA_DISABLED: '1', VK_DRIVER_FILES: resolve('debugging/nonexistent-vulkan-driver.json') },
        { PCB_BLOCK_GPU_DISABLED: '1' },
    ]) {
        const result = run(p, backend, options);
        assert.deepEqual(result.solution, cpu.solution);
        assert.doesNotMatch(result.log, /\[block-gpu-runtime\]/);
    }
});

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

test('ordinary bus preserves every net and long distances on block CPU/GPU', { skip: !gpuEnabled }, () => {
    const p = problem();
    addOrdinaryBus(p);
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BLOCK_GPU_VERIFY: '1' });
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /block-gpu-fallback|validation failed/);
    assert.deepEqual(gpu.solution, cpu.solution);
});

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

test('GPU external relations select the present endpoint in both directions', {skip:!gpuEnabled},()=>{
    for (const reverse of [false,true]) {
        const p=problem();
        p.relations.push({...p.relations[0],id:'external',
            from:reverse?'pad:OUTSIDE.1':'pad:C0.1',to:reverse?'pad:C0.1':'pad:OUTSIDE.1',
            effect:'move_from',weight:12,preferFacingPads:false});
        const cpu=run(p,'cpu');
        const gpu=run(p,'cubecl',{PCB_BLOCK_GPU_VERIFY:'1'});
        assert.match(gpu.log,/"backend":"cubecl"/);
        assert.doesNotMatch(gpu.log,/block-gpu-fallback|validation failed/);
        checkGeometry(p,gpu.solution);
        assert.ok(gpu.solution.rank.score<=cpu.solution.rank.score*1.01+.01);
    }
});


test('folded block relation matches an explicit moving point on CPU and GPU', () => {
    const p = problem();
    p.components[0].blockName = 'folded';
    p.components[0].bodyBox = {left: .5, right: 1, top: -.25, bottom: .25};
    p.primitives[0].connectionPoints.push({ref: 'group-oracle', x: .75, y: 0});
    p.relations = [{id: 'group-near', kind: 'hint', from: 'block:folded', to: 'component:C1',
        relation: 'near', priority: 'high', hard: false, effect: 'move_from', satelliteAnchor: false}];
    const explicit = structuredClone(p);explicit.relations[0].from = 'pad:group-oracle';
    assert.deepEqual(require(addon).evaluateBlockQuality(p), require(addon).evaluateBlockQuality(explicit));
    const cpu = run(p, 'cpu');
    assert.deepEqual(cpu.solution, run(explicit, 'cpu').solution);
    if (gpuEnabled) {
        const gpu = run(p, 'cubecl', {PCB_BLOCK_GPU_VERIFY: '1'});
        assert.doesNotMatch(gpu.log, /block-gpu-fallback/);
        assert.deepEqual(gpu.solution, cpu.solution);
    }
});

for (const backend of ['cpu', 'cubecl']) test(`group points survive deferred pairs through single and batch ${backend}`, {skip:backend === 'cubecl' && !gpuEnabled}, () => {
    const p = problem();
    p.components[0].blockName = 'folded';
    p.primitives[1].locked = true;
    p.relations = [{id:'group', kind:'hint', from:'block:folded', to:'component:C1',
        relation:'near', hard:false, effect:'move_from', satelliteAnchor:false},
        {id:'locked-group',kind:'hint',from:'block:P1',to:'component:C0',relation:'near',hard:false,effect:'move_from',satelliteAnchor:false}];
    const child = spawnSync(process.execPath, ['-e', `
        const assert=require('assert/strict'),a=require(process.argv[1]);
        const p=JSON.parse(require('fs').readFileSync(0,'utf8'));
        const initial=a.solveBlockPrimitivesBatch([{...p,deferPairs:true}],1)[0];
        assert(initial.pairSeed.some(s=>s.primitive.connectionPoints.some(p=>p.ref==='block:folded')));
        const resume={...p,pairSeed:initial.pairSeed};
        const single=a.solveBlockPrimitives(resume);
        const batch=a.solveBlockPrimitivesBatch([resume],1)[0];
        assert.deepEqual(batch,single);
        const full=a.solveBlockPrimitives(p);
        assert.deepEqual(single.rank,full.rank);assert.deepEqual(single.states,full.states);
        const invalid=structuredClone(resume);invalid.pairSeed.find(s=>!s.primitive.locked).primitive.connectionPoints.push({ref:'forged',x:0,y:0});
        assert.throws(()=>a.solveBlockPrimitivesBatch([invalid],1),/invalid pairSeed geometry or inventory/);
        console.log('ok');
    `, addon], {input:JSON.stringify(p),encoding:'utf8',windowsHide:true,
        env:{...process.env,PCB_BLOCK_BACKEND:backend,PCB_BOARD_PACKER_THREADS:'1'},maxBuffer:4*1024*1024});
    assert.equal(child.status,0,child.stderr || String(child.error));
    assert.doesNotMatch(child.stderr,/block-gpu-fallback/);
});


test('semantic groups and clearance minima keep CPU/GPU block ranks including pair resume', {skip:!gpuEnabled},()=>{
    const p=problem();p.components[0].role='main_ic';p.components[0].blockName='pair';p.components[1].blockName='pair';
    const relation=(name:string,from:string,to:string,condition={},hard=false)=>({id:name,kind:'hint',relation:name,from,to,condition,hard,effect:'move_from',priority:'normal',weight:70,minDistance:1,satelliteAnchor:false});
    p.relations.push(relation('clearance','block:pair','component:C2',{},true),relation('away_from','component:C0','component:C1'),
        relation('same_side','block:pair','component:C2'),relation('prefer_layer','block:pair','board',{layer:'bottom'}));
    const cpu=run(p,'cpu'),gpu=run(p,'cubecl',{PCB_BLOCK_GPU_VERIFY:'1',PCB_BLOCK_GPU_VERIFY_PRUNE:'1',PCB_BLOCK_GPU_VERIFY_FRONTIER:'1'});
    assert.doesNotMatch(gpu.log,/fallback/);assert.match(gpu.log,/\[block-gpu/);assert.deepEqual(gpu.solution,cpu.solution);
    const deferred=run({...p,deferPairs:true},'cubecl',{PCB_BLOCK_GPU_VERIFY:'1'});
    assert.ok(deferred.solution.pairSeed?.length);
    const resumed={...p,pairSeed:deferred.solution.pairSeed};
    assert.deepEqual(run(resumed,'cubecl',{PCB_BLOCK_GPU_VERIFY:'1'}).solution,run(resumed,'cpu').solution);
});

test('GPU long-net scales use the CPU nominal primitive size', { skip: !gpuEnabled }, () => {
    // STEPPER_CONTROLLER pd_controller: off-grid input boxes whose rounded bbox
    // extent (1.9393 -> 1.939 or 1.94) differs from the rotated width the CPU uses.
    const captured = JSON.parse(readFileSync(new URL('./fixtures/block-placement/STEPPER_CONTROLLER/pd-controller.json', import.meta.url), 'utf8'));
    const p = captured.problem as NativeBlockSolveProblemV4;
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BLOCK_GPU_VERIFY: '1' });
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /block-gpu-fallback|panicked/);
    assert.deepEqual(gpu.solution, cpu.solution);
});
