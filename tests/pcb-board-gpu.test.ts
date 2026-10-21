import test from 'node:test';
import { encodeNativeBoardPackProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-board-problem.ts';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { availableParallelism } from 'node:os';
import { blockPolicy } from '../src/pcb-layout/pcb-auto-place-v2/block-policy.ts';
import { addOrdinaryBus, minimalProblem } from './helpers/native-board-problem.ts';
import type { NativeBoardPackProblemV7 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';

const require = createRequire(import.meta.url);
const addonPath = resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH
    ?? resolve('native/pcb-board-packer', require('../native/pcb-board-packer/platform.cjs').nativeFilename()));
const gpuEnabled = process.env.PCB_BOARD_GPU_TESTS === '1';

for (const count of [4, 16]) test(`${count} repeated channels use joint CPU/GPU search even with zero orientation weight`, { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    const extent = Math.max(12, count * 2);
    const channels = Array.from({ length: count }, (_, i) => i);
    p.bounds = p.fullBoardBounds = { left: -extent, right: extent, top: -12, bottom: 12 };
    p.boardOutline = [{ x: -extent, y: -12 }, { x: extent, y: -12 }, { x: extent, y: 12 }, { x: -extent, y: 12 }];
    const template = p.primitives[0];
    p.primitives = [...channels, count].map(i => ({ ...structuredClone(template), id: `P${i}`, locked: i === count,
        placements: [{ ...template.placements[0], designator: `U${i}` }], connectionPoints: [],
        pathPorts: i < count
            ? [{ pathId: `ch${i}`, order: 1, ref: `U${i}.1`, role: 'target', x: 0, y: 0, normal: { x: 0, y: -1 } }]
            : channels.map(j => ({ pathId: `ch${j}`, order: 0, ref: `J1.${j + 1}`, role: 'source', x: (j - (count - 1) / 2) * 3, y: 0, normal: { x: 0, y: 1 } })),
    }));
    p.components = p.primitives.map((primitive, i) => ({ ...structuredClone(p.components[0]), primitiveId: primitive.id, designator: `U${i}` }));
    p.componentConflict = Array.from({ length: (count + 1) ** 2 }, (_, i) => Number(Math.floor(i / (count + 1)) !== i % (count + 1)));
    p.componentPairClearance = p.componentConflict.map(v => v * .2);
    p.softAlignment = { weight: 1, tolerance: .15, orientationWeight: 0, pairs: [
        ...channels.slice(1).map(i => ({ a: 'P0', b: `P${i}`, anchorA: 'U0', anchorB: `U${i}`, similarity: 1, orientation: { a: 'U0', b: `U${i}`, offset: 0 } })),
    ] };
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
    assert.match(cpu.log, /joint candidates=[1-9]/);
    assert.match(gpu.log, /joint candidates=[1-9]/);
    assert.doesNotMatch(gpu.log, /fallback|validation failed/);
    assert.deepEqual(gpu.solution, cpu.solution);
    assert.equal(cpu.solution.rank.hardCount, 0);
});

for (const kind of ['block', 'module']) test(`folded ${kind} relations reuse moving pad endpoints on CPU and GPU`, () => {
    const p = minimalProblem();
    p.components[0].blockName = 'folded';
    if (kind === 'module') p.relationGroups = { 'module:folded': ['U1'] };
    p.components[0].bodyBox = { left: -1, right: 0, top: -.5, bottom: .5 };
    p.primitives[0].connectionPoints = [{ ref: 'U1.1', x: -.5, y: 0 }];
    p.relations = [{ id: 'folded-near', kind: 'hint', from: `${kind}:folded`, to: 'anchor:board.right',
        relation: 'near', priority: 'high', hard: false, effect: 'move_from', satelliteAnchor: false }];
    const explicit = structuredClone(p);
    explicit.relations[0].from = 'pad:U1.1';
    assert.deepEqual(require(addonPath).evaluateBoardQuality(p), require(addonPath).evaluateBoardQuality(explicit));
    const cpu = run(p, 'cpu');
    assert.deepEqual(cpu.solution, run(explicit, 'cpu').solution);
    if (gpuEnabled) {
        const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1' });
        assert.doesNotMatch(gpu.log, /fallback/);
        assert.match(gpu.log, /\[board-backend\].*"backend":"cubecl"/);
        assert.deepEqual(gpu.solution, cpu.solution);
    }
});

test('geometric route GPU preserves owned pads and replays the original board on failure', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    p.searchWidth = 4;
    const source = structuredClone(p.primitives[0]);
    p.primitives = [0, 1, 2].map(i => ({ ...structuredClone(source), id: `P${i}`,
        placements: [{ ...source.placements[0], designator: `U${i}` }],
        connectionPoints: [{ ref: `U${i}.1`, net: i === 2 ? 'OTHER' : 'SIG', x: .8, y: 0 }],
    }));
    p.components = p.primitives.map((primitive, i) => ({ ...structuredClone(p.components[0]), primitiveId: primitive.id, designator: `U${i}` }));
    p.componentConflict = Array.from({ length: 9 }, (_, i) => Number(Math.floor(i / 3) !== i % 3));
    p.componentPairClearance = p.componentConflict.map(v => v * .2);
    p.routingObstacles = p.primitives.map((primitive, i) => ({ primitiveId: primitive.id, ref: `U${i}.1`,
        net: i === 2 ? 'OTHER' : 'SIG', layer: i === 1 ? 'bottom' : 'top',
        box: { left: .6, right: 1, top: -.3, bottom: .3 } }));
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_ROUTE_GPU_VERIFY: '1', PCB_ROUTE_GPU_PROFILE: '1', PCB_ROUTE_GPU_FORCE: '1' });
    assert.match(gpu.log, /\[geometric-route-gpu\]/);
    assert.doesNotMatch(gpu.log, /board-gpu-fallback/);
    assert.deepEqual(gpu.solution, cpu.solution);
    const failed = run(p, 'cubecl', { PCB_ROUTE_GPU_FAIL: '1' });
    assert.match(failed.log, /injected geometric route failure/);
    assert.match(failed.log, /board-gpu-fallback/);
    assert.deepEqual(failed.solution, cpu.solution);
});

function run(problem: NativeBoardPackProblemV7, backend: string | undefined, options: Record<string, string> = {}, boundaryError?: RegExp) {
    const child = spawnSync(process.execPath, ['--input-type=commonjs', '-e',
        'const fs=require("node:fs");const addon=require(process.argv[1]);const inputs=JSON.parse(fs.readFileSync(0,"utf8"));'
        + 'console.log(JSON.stringify(inputs.map(p=>addon.solveBoardPacked(p))));', addonPath], {
        input: JSON.stringify([problem]), encoding: 'utf8', windowsHide: true, timeout: 60_000,
        env: { ...process.env, PCB_BOARD_BACKEND: backend, PCB_BOARD_PACKER_THREADS: '1',
            PCB_BOARD_PACKER_PROFILE: '1', PCB_BLOCK_SOLVER_PROFILE: '1', ...options },
    });
    if (boundaryError) {
        assert.notEqual(child.status,0);
        assert.match(child.stderr,boundaryError);
        assert.doesNotMatch(child.stderr,/\[block-gpu-runtime\]/);
        return {solution:null,log:child.stderr};
    }
    assert.equal(child.status, 0, child.stderr || String(child.error));
    return { solution: JSON.parse(child.stdout)[0], log: child.stderr };
}

test('explicit CPU and conservative auto board calls do not initialize the GPU', () => {
    const p = minimalProblem();
    const cpu = run(p, 'cpu');
    const auto = run(p, 'auto');
    assert.deepEqual(auto.solution, cpu.solution);
    assert.doesNotMatch(cpu.log + auto.log, /\[block-gpu-runtime\]/);
    const defaults = run(p, undefined);
    assert.deepEqual(defaults.solution, cpu.solution);
    assert.match(defaults.log, /"requested":"auto"/);
    assert.doesNotMatch(defaults.log, /\[block-gpu-runtime\]/);
});

test('disabled board GPU replays the complete original CPU call', () => {
    const p = minimalProblem();
    const cpu = run(p, 'cpu');
    const disabled = run(p, 'cubecl', { PCB_BLOCK_GPU_DISABLED: '1' });
    assert.deepEqual(disabled.solution, cpu.solution);
    assert.doesNotMatch(disabled.log, /\[block-gpu-runtime\]/);
});

test('unsafe board coordinates are rejected by the domain before GPU initialization', () => {
    const p = minimalProblem();
    p.primitives[0].placements[0].x = 1e9;
    // A billion-mm local extent, rather than a translated small board.
    // Wider previously supported frames remain an open migration case.
    for (const backend of ['cpu','cubecl']) {
        run(p,backend,{},/coordinate exceeds the documented absolute\/local frame/);
    }
});

test('board GPU evaluates all candidates and preserves the complete CPU result', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    const cpu = run(p, 'cpu', { PCB_BOARD_GPU_VERIFY_STAGES: '1' });
    const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1', PCB_BOARD_GPU_VERIFY_STAGES: '1' });
    const checkpoints = (log: string) => log.split(/\r?\n/).filter(line => line.startsWith('[board-stage-checkpoint] '))
        .map(line => JSON.parse(line.slice('[board-stage-checkpoint] '.length)));
    assert.equal(checkpoints(cpu.log).length, 3);
    assert.deepEqual(checkpoints(gpu.log), checkpoints(cpu.log));
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /board-gpu-fallback|validation failed/);
    assert.deepEqual(gpu.solution, cpu.solution);
});

for (const backend of ['cpu', 'cubecl']) test(`board ${backend} finds a layer-specific corridor narrower than its grid`, { skip: backend === 'cubecl' && !gpuEnabled }, () => {
    const p = minimalProblem();
    p.grid = 1;
    p.clearance = 0.35;
    p.edgeClearance = 0;
    p.bounds = p.fullBoardBounds = { left: -12, right: 12, top: -12, bottom: 12 };
    p.boardOutline = [{ x: -12, y: -12 }, { x: 12, y: -12 }, { x: 12, y: 12 }, { x: -12, y: 12 }];
    const bodies = [
        { left: -2.54, right: 2.54, top: -2, bottom: 2 },
        { left: -11, right: -2.1241, top: -11, bottom: 11 },
        { left: 2.99, right: 11, top: -11, bottom: 11 },
    ];
    const pad = { left: -2.12, right: 2.12, top: -2, bottom: 2 };
    const template = p.primitives[0];
    p.primitives = bodies.map((box, i) => ({ ...structuredClone(template),
        id: `primitive:P${i}`, locked: i > 0, canRotate: false, allowedOrientations: [0],
        bbox: box, collisionBoxes: i === 0 ? [box, pad] : [box],
        collisionBoxLayers: i === 0 ? ['bottom', 'top'] : ['top'],
        width: box.right - box.left, height: box.bottom - box.top,
        placements: [{ designator: `U${i}`, x: 0, y: 0, rotate: 0, layer: i === 0 ? 'bottom' : 'top', score: 0 }],
        connectionPoints: [],
    }));
    p.components = bodies.map((bodyBox, i) => ({ ...structuredClone(p.components[0]),
        designator: `U${i}`, primitiveId: p.primitives[i].id, layer: i === 0 ? 'bottom' : 'top',
        bodyBox, throughHoleBoxes: i === 0 ? [pad] : [], edgeClearance: 0,
    }));
    p.componentConflict = Array.from({ length: 9 }, (_, i) => Math.floor(i / 3) === i % 3 ? 0 : 1);
    p.componentPairClearance = p.componentConflict.map(c => c ? 0.35 : 0);
    const { solution, log } = run(p, backend, { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
    assert.equal(solution.rank.hardCount, 0);
    const x = solution.states.find((s: { primitiveId: string }) => s.primitiveId === 'primitive:P0').placements[0].x;
    assert.ok(x >= 0.3458 && x <= 0.5201, `pad centre ${x} misses the legal corridor`);
    assert.doesNotMatch(log, /board-gpu-fallback|validation failed/);
    if (backend === 'cubecl') assert.match(log, /"backend":"cubecl"/);
});

test('GPU handles compound geometry, layer exclusions, obstacles, overflow and regions', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    p.primitives = Array.from({ length: 3 }, (_, i) => {
        const primitive = structuredClone(p.primitives[0]);
        primitive.id = `primitive:P${i}`;
        primitive.placements[0].designator = `U${i}`;
        primitive.connectionPoints[0].ref = `U${i}.1`;
        primitive.locked = i === 0;
        return primitive;
    });
    p.components = p.primitives.flatMap((primitive, i) => Array.from({ length: 2 }, (_, j) => ({
        ...structuredClone(p.components[0]), primitiveId: primitive.id,
        designator: `U${i}`, layer: i === 1 ? 'bottom' as const : 'top' as const,
        bodyBox: { left: -.8 + j * .5, right: -.2 + j * .5, top: -.4, bottom: .4 },
        throughHoleBoxes: [{ left: -.1, right: .1, top: -.3, bottom: .3 }],
        boardOverflow: { left: .4, right: 0, top: 0, bottom: 0 },
    })));
    const n = p.components.length;
    p.componentConflict = Array.from({ length: n * n }, (_, i) => Math.floor(i / n) === i % n ? 0 : 1);
    p.componentPairClearance = p.componentConflict.map(c => c ? .3 : 0);
    p.obstacles = [{ left: 2, right: 2.2, top: -3, bottom: 3 }];
    p.constraintRegions = [{ name: 'keepout', layers: ['bottom'], allowBlocks: [],
        box: { left: -3, right: -2, top: -1, bottom: 1 } }];
    p.softSpacing = { gap: .5, compactnessScale: .25, exemptPairs: [[p.primitives[0].id, p.primitives[1].id]] };
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /board-gpu-fallback|validation failed/);
    assert.deepEqual(gpu.solution, cpu.solution);
});

test('GPU preserves endpoint, topology, ordinary-net and alignment terms together', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    p.primitives = Array.from({ length: 3 }, (_, i) => {
        const primitive = structuredClone(p.primitives[0]);
        primitive.id = `primitive:P${i}`;
        primitive.placements[0].designator = `U${i}`;
        primitive.connectionPoints[0].ref = `U${i}.1`;
        primitive.pathPorts = [0, 1].map(j => ({ x: j ? .8 : -.8, y: 0, pathId: 'path-A', order: i * 2 + j,
            ref: `U${i}.${j + 1}`, role: j ? 'output' : 'input', normal: { x: j ? 1 : -1, y: 0 } }));
        return primitive;
    });
    p.components = p.primitives.map((primitive, i) => ({ ...structuredClone(p.components[0]),
        primitiveId: primitive.id, designator: `U${i}` }));
    p.componentConflict = Array.from({ length: 9 }, (_, i) => Math.floor(i / 3) === i % 3 ? 0 : 1);
    p.componentPairClearance = p.componentConflict.map(v => v ? .2 : 0);
    p.relations = [
        { id: 'path-rule', kind: 'critical_pair', from: 'pad:U0.1', to: 'component:U1', priority: 'high',
            hard: true, weight: 70, effect: 'move_both', maxDistance: 3, minDistance: .4,
            satelliteAnchor: true, anchorOffset: { x: .35, y: -.15 }, sidePreference: 'left',
            pathId: 'path-A', pathShape: 'straight', preferFacingPads: true },
        { id: '__ordinary_net__:SIG', kind: 'net', from: '__ordinary_net__:SIG', to: '__ordinary_net__:SIG',
            hard: false, weight: 1, effect: 'score_only', satelliteAnchor: false, preferFacingPads: false },
    ];
    p.softAlignment = { weight: 24, tolerance: .15, orientationWeight: 120,
        pairs: [{ a: p.primitives[1].id, b: p.primitives[2].id, similarity: .95, anchorA: 'U1', anchorB: 'U2',
            orientation: { a: 'U1', b: 'U2', offset: 90 } }] };
    for (const compactness of ['normal', 'high'] as const) {
        p.compactness = compactness;
        const cpu = run(p, 'cpu');
        const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
        assert.match(gpu.log, /"backend":"cubecl"/);
        assert.doesNotMatch(gpu.log, /board-gpu-fallback|validation failed/);
        assert.deepEqual(gpu.solution, cpu.solution);
    }
});

test('ordinary bus preserves every net and long distances on board CPU/GPU', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    p.searchWidth = 4;
    const second = structuredClone(p.primitives[0]);
    second.id = 'primitive:second'; second.placements[0].designator = 'U2';
    second.connectionPoints[0].ref = 'U2.1';
    p.primitives.push(second);
    p.components.push({ ...structuredClone(p.components[0]), primitiveId: second.id, designator: 'U2' });
    p.componentConflict = [0, 1, 1, 0]; p.componentPairClearance = [0, .2, .2, 0];
    addOrdinaryBus(p);
    for (const compactness of ['normal', 'high'] as const) {
        p.compactness = compactness;
        const cpu = run(p, 'cpu');
        const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
        assert.match(gpu.log, /"backend":"cubecl"/);
        assert.doesNotMatch(gpu.log, /board-gpu-fallback|validation failed/);
        assert.deepEqual(gpu.solution, cpu.solution);
    }
});

test('large primitives keep the CPU edge bias on board GPU', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    const large = structuredClone(p.primitives[0]);
    large.id = 'primitive:large'; large.placements[0].designator = 'U2';
    large.connectionPoints[0].ref = 'U2.1';
    // 8 % of the board area; the edge bias applies from 5.5 %.
    large.bbox = { left: -2, right: 2, top: -1, bottom: 1 }; large.width = 4; large.height = 2;
    large.collisionBoxes = [{ ...large.bbox }];
    p.primitives.push(large);
    p.components.push({ ...structuredClone(p.components[0]), primitiveId: large.id, designator: 'U2', bodyBox: large.bbox });
    p.componentConflict = [0, 1, 1, 0]; p.componentPairClearance = [0, .2, .2, 0];
    for (const compactness of ['normal', 'high'] as const) {
        p.compactness = compactness;
        const cpu = run(p, 'cpu');
        const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
        assert.match(gpu.log, /"backend":"cubecl"/);
        assert.doesNotMatch(gpu.log, /board-gpu-fallback|validation failed/);
        assert.deepEqual(gpu.solution, cpu.solution);
    }
});

test('GPU failure at every board stage restarts the original CPU input', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    const cpu = run(p, 'cpu');
    const failures = [
        { PCB_BOARD_GPU_FAIL_BATCH: '1' },
        { PCB_BOARD_GPU_FAIL_AT: 'after_beam' },
        { PCB_BOARD_GPU_FAIL_AT: 'after_local_improve' },
        { PCB_BOARD_GPU_FAIL_AT: 'after_repair' },
    ];
    for (const options of failures) {
        const gpu = run(p, 'cubecl', options);
        assert.match(gpu.log, /board-gpu-fallback/);
        assert.match(gpu.log, /runtime_failure/);
        assert.deepEqual(gpu.solution, cpu.solution);
        assert.equal((gpu.log.match(/\[block-gpu-runtime\]/g) ?? []).length, 1);
    }
});

test('auto keeps the CPU worker budget while using GPU', {
    skip: !gpuEnabled || Math.floor(availableParallelism() / 2) <= 4,
}, () => {
    const p = minimalProblem();
    const zero = { left: 0, right: 0, top: 0, bottom: 0 };
    p.bounds = zero;p.fullBoardBounds = zero;
    p.boardOutline = [{ x: 0, y: 0 }, { x: 0, y: 0 }, { x: 0, y: 0 }];
    p.primitives = Array.from({ length: 34 }, (_, i) => ({ ...structuredClone(p.primitives[0]),
        id: `p:${i}`, locked: i >= 24, canRotate: false, allowedOrientations: [0],
        bbox: zero, collisionBoxes: [zero], width: 0, height: 0, connectionPoints: [],
        placements: [{ designator: `U${i}`, x: 0, y: 0, rotate: 0, layer: 'top', score: 0 }] }));
    p.components = Array.from({ length: 154 }, (_, i) => ({ ...structuredClone(p.components[0]),
        designator: `U${i}`, primitiveId: p.primitives[i % 34].id, bodyBox: zero }));
    p.componentConflict = Array(154 * 154).fill(0);p.componentPairClearance = Array(154 * 154).fill(0);
    p.relations = Array.from({ length: 535 }, (_, i) => ({ id: `r:${i}`, kind: 'net', from: 'component:U0', to: 'component:U0',
        hard: false, weight: 1, effect: 'score_only', satelliteAnchor: false, preferFacingPads: false }));
    const cpu = run(p, 'cpu', { PCB_BOARD_PACKER_THREADS: '6' });
    const auto = run(p, 'auto', { PCB_BOARD_PACKER_THREADS: '6' });
    assert.deepEqual(auto.solution, cpu.solution);
    // Opt-in Metal keeps auto board packing on the CPU; other runtimes use the GPU.
    if (process.platform === 'darwin' && process.env.PCB_GPU_METAL_DISABLED === '0') {
        assert.doesNotMatch(auto.log, /"backend":"cubecl"/);
        return;
    }
    assert.match(auto.log, /"backend":"cubecl"/);
    assert.match(auto.log, /threads=6/);
    const schedulerLine=auto.log.split('\n').find(line=>line.startsWith('[board-cpu-scheduler] '));
    assert.ok(schedulerLine,'board beam scheduler telemetry');
    const scheduler=JSON.parse(schedulerLine.slice('[board-cpu-scheduler] '.length));
    assert.equal(scheduler.cpu.activeLimit,6);
    assert.ok(scheduler.cpu.peakActive<=6);
    assert.equal(scheduler.cpu.active,0);
    assert.doesNotMatch(auto.log, /board-gpu-fallback/);
});

test('failure of the second aligned call preserves the completed ordinary call', { skip: !gpuEnabled }, () => {
    const ordinary = minimalProblem();
    const second = structuredClone(ordinary.primitives[0]);
    second.id = 'primitive:second';second.placements[0].designator = 'U2';second.connectionPoints[0].ref = 'U2.1';
    ordinary.primitives.push(second);
    ordinary.components.push({ ...ordinary.components[0], primitiveId: second.id, designator: 'U2' });
    ordinary.componentConflict = [0, 1, 1, 0];ordinary.componentPairClearance = [0, .2, .2, 0];
    const aligned = structuredClone(ordinary);
    aligned.softAlignment = { weight: 24, tolerance: .15, orientationWeight: 120,
        pairs: [{ a: ordinary.primitives[0].id, b: second.id, similarity: .95, anchorA: 'U1', anchorB: 'U2' }] };
    const reference = [run(ordinary, 'cpu').solution, run(aligned, 'cpu').solution];
    const child = spawnSync(process.execPath, ['--input-type=commonjs', '-e',
        'const a=require(process.argv[1]);const p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));'
        + 'const ordinary=a.solveBoardPacked(p[0]);process.env.PCB_BOARD_GPU_FAIL_AT="after_local_improve";'
        + 'const aligned=a.solveBoardPacked(p[1]);console.log(JSON.stringify([ordinary,aligned]));', addonPath], {
        input: JSON.stringify([ordinary, aligned]), encoding: 'utf8', windowsHide: true,
        env: { ...process.env, PCB_BOARD_BACKEND: 'cubecl', PCB_BOARD_PACKER_THREADS: '1',
            PCB_BOARD_PACKER_PROFILE: '1', PCB_BLOCK_SOLVER_PROFILE: '1' },
    });
    assert.equal(child.status, 0, child.stderr || String(child.error));
    assert.deepEqual(JSON.parse(child.stdout), reference);
    assert.equal((child.stderr.match(/\[block-gpu-runtime\]/g) ?? []).length, 1);
    assert.equal((child.stderr.match(/\[board-gpu-fallback\]/g) ?? []).length, 1);
    assert.match(child.stderr, /"backend":"cubecl"/);
    assert.match(child.stderr, /"backend":"cpu"/);
});


test('production GPU shortlist preserves polygon/edge decisions, ties and chunk boundaries', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    p.boardOutline = [{ x: -5, y: -5 }, { x: 5, y: -5 }, { x: 5, y: 0 },
        { x: 1, y: 0 }, { x: 1, y: 5 }, { x: -5, y: 5 }];
    p.primitives[0].edgePlace = { edges: ['left', 'top'], inset: .2005, y: -.0005 };
    p.components[0].edgeClearance = .2005;
    p.obstacles = [{ left: -.0005, right: .2005, top: -2.0005, bottom: 2.0005 }];
    const cpu = run(p, 'cpu');
    for (const chunk of ['1', '17', '512']) {
        const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_CHUNK_SIZE: chunk,
            PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
        assert.match(gpu.log, /"backend":"cubecl"/);
        assert.doesNotMatch(gpu.log, /board-gpu-fallback|validation failed/);
        assert.deepEqual(gpu.solution, cpu.solution);
    }
    const production = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '0', PCB_BOARD_GPU_VERIFY_SHORTLIST: '0' });
    assert.match(production.log, /"backend":"cubecl"/);
    assert.doesNotMatch(production.log, /board-gpu-fallback/);
    assert.deepEqual(production.solution, cpu.solution);
});


test('board GPU has no-device recovery without changing the CPU result', () => {
    const p = minimalProblem();
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_GPU_RUNTIME: 'vulkan', VK_DRIVER_FILES: resolve('debugging/nonexistent-vulkan-driver.json') });
    assert.deepEqual(gpu.solution, cpu.solution);
    assert.match(gpu.log, /no compatible F32 Vulkan GPU/);
    assert.doesNotMatch(gpu.log, /"backend":"cubecl"/);
});

test('board completes on CPU with both GPU drivers unavailable or GPU disabled', () => {
    const p = minimalProblem();
    const cpu = run(p, 'cpu');
    for (const backend of ['auto', 'cubecl']) for (const options of [
        { PCB_GPU_RUNTIME: 'auto', PCB_GPU_CUDA_DISABLED: '1', PCB_GPU_METAL_DISABLED: '1', VK_DRIVER_FILES: resolve('debugging/nonexistent-vulkan-driver.json') },
        { PCB_BLOCK_GPU_DISABLED: '1' },
    ]) {
        const result = run(p, backend, options);
        assert.deepEqual(result.solution, cpu.solution);
        assert.doesNotMatch(result.log, /\[block-gpu-runtime\]/);
        assert.match(result.log, /"backend":"cpu"/);
    }
});

test('block -> board -> block share one runtime and keep their buffers independent', { skip: !gpuEnabled }, () => {
    const board = minimalProblem();
    const b = minimalProblem();
    const second = structuredClone(b.primitives[0]);second.id = 'primitive:second';
    second.placements[0].designator = 'U2';second.connectionPoints[0].ref = 'U2.1';
    b.primitives.push(second);b.components.push({ ...b.components[0], primitiveId: second.id, designator: 'U2' });
    const third = structuredClone(b.primitives[0]);third.id = 'primitive:third';
    third.placements[0].designator = 'U3';third.connectionPoints[0].ref = 'U3.1';
    b.primitives.push(third);b.components.push({ ...b.components[0], primitiveId: third.id, designator: 'U3' });
    const block = { version: 4, grid: .5, clearance: .2, searchWidth: 4, compactness: 'normal',
        primitives: b.primitives, components: b.components.map(c => ({ ...c, pinCount: 1, powerComponent: true })),
        componentConflict: [0, 1, 1, 1, 0, 1, 1, 1, 0], componentPairClearance: [0, .2, .2, .2, 0, .2, .2, .2, 0],
        collisionMode: 'components', hardCollisionMode: 'components', candidateBoxMode: 'bbox',
        relations: [], obstacles: [], experiments: { ...blockPolicy().experiments, longNets: false } };

    const invoke = (backend: string) => {
        const child = spawnSync(process.execPath, ['--input-type=commonjs', '-e',
            'const a=require(process.argv[1]);const [b,p]=JSON.parse(require("node:fs").readFileSync(0,"utf8"));'
            + 'const values=[a.solveBlockPrimitives(b),a.solveBoardPacked(p),a.solveBlockPrimitives(b)];'
            + 'console.log(JSON.stringify(values));', addonPath], {
            input: JSON.stringify([block, board]), encoding: 'utf8', windowsHide: true,
            env: { ...process.env, PCB_BLOCK_BACKEND: backend, PCB_BOARD_BACKEND: backend,
                PCB_BOARD_PACKER_THREADS: '1', PCB_BLOCK_SOLVER_THREADS: '1', PCB_BLOCK_SOLVER_PROFILE: '1' },
        });
        assert.equal(child.status, 0, child.stderr);
        return { values: JSON.parse(child.stdout), log: child.stderr };
    };
    const cpu = invoke('cpu'), gpu = invoke('cubecl');
    assert.deepEqual(gpu.values, cpu.values);
    assert.equal((gpu.log.match(/\[block-gpu-runtime\]/g) ?? []).length, 1);
    assert.ok((gpu.log.match(/\[block-gpu-stage\]/g) ?? []).length >= 2, gpu.log);
    assert.match(gpu.log, /\[board-backend\].*"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /fallback/);
});


test('board failure releases the process lease while its CPU replay process remains alive', { skip: !gpuEnabled, timeout: 60000 }, async () => {
    const p = minimalProblem(), cpu = run(p, 'cpu');
    const owner = spawn(process.execPath, ['--input-type=commonjs', '-e',
        'const a=require(process.argv[1]);const p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));'
        + 'process.send(a.solveBoardPacked(p));process.on("message",()=>process.exit(0));', addonPath], {
        windowsHide: true, stdio: ['pipe', 'ignore', 'pipe', 'ipc'],
        env: { ...process.env, PCB_BOARD_BACKEND: 'cubecl', PCB_BOARD_GPU_FAIL_BATCH: '1', PCB_BOARD_PACKER_THREADS: '1' },
    });
    let log = '';
    owner.stderr!.on('data', data => { log += data; });
    try {
        const solution = new Promise((ok, fail) => {
            owner.once('message', ok); owner.once('error', fail);
            owner.once('exit', code => fail(Error(`lease owner exited early: ${code}`)));
        });
        owner.stdin!.end(JSON.stringify(p));
        assert.deepEqual(await solution, cpu.solution);
        await new Promise<void>(done => setImmediate(done));
        assert.equal(owner.exitCode, null);
        assert.match(log, /board-gpu-fallback/);
        const contender = run(p, 'cubecl');
        assert.deepEqual(contender.solution, cpu.solution);
        assert.match(contender.log, /"backend":"cubecl"/);
        assert.doesNotMatch(contender.log, /fallback/);
        assert.equal(owner.exitCode, null);
    } finally { owner.kill(); }
});


test('1/2/4 board workers preserve the CPU result and one GPU initialization', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    const second = structuredClone(p.primitives[0]);
    second.id = 'primitive:second';second.placements[0].designator = 'U2';second.connectionPoints[0].ref = 'U2.1';
    p.primitives.push(second);
    p.components.push({ ...structuredClone(p.components[0]), primitiveId: second.id, designator: 'U2' });
    p.componentConflict = [0, 1, 1, 0];p.componentPairClearance = [0, .2, .2, 0];
    const cpu = run(p, 'cpu');
    for (const workers of ['1', '2', '4']) {
        const gpu = run(p, 'cubecl', { PCB_BOARD_PACKER_THREADS: workers });
        assert.deepEqual(gpu.solution, cpu.solution);
        assert.match(gpu.log, /"backend":"cubecl"/);
        assert.doesNotMatch(gpu.log, /fallback/);
        assert.equal((gpu.log.match(/\[block-gpu-runtime\]/g) ?? []).length, 1);
    }
});


test('idle GPU owner stays alive while four processes complete explicit GPU work', { skip: !gpuEnabled, timeout: 60000 }, async () => {
    const p = minimalProblem(), cpu = run(p, 'cpu');
    const code = 'const a=require(process.argv[1]);const p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));'
        + 'process.send(a.solveBoardPacked(p));process.on("message",()=>process.exit(0));';
    const owner = spawn(process.execPath, ['--input-type=commonjs', '-e', code, addonPath], {
        windowsHide: true, stdio: ['pipe', 'ignore', 'pipe', 'ipc'],
        env: { ...process.env, PCB_BOARD_BACKEND: 'cubecl', PCB_BOARD_PACKER_THREADS: '1' },
    });
    let ownerLog = '';owner.stderr!.on('data', data => { ownerLog += data; });
    try {
        const ready = new Promise((ok, fail) => { owner.once('message', ok);owner.once('error', fail); });
        owner.stdin!.end(JSON.stringify(p));assert.deepEqual(await ready, cpu.solution);
        await new Promise<void>(done => setImmediate(done));
        assert.match(ownerLog, /"backend":"cubecl"/);
        for (const processes of [4]) {
            const results = await Promise.all(Array.from({ length: processes - 1 }, () => new Promise<{ solution: unknown; log: string }>((ok, fail) => {
                const child = spawn(process.execPath, ['--input-type=commonjs', '-e',
                    'const a=require(process.argv[1]);const p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));console.log(JSON.stringify(a.solveBoardPacked(p)));', addonPath], {
                    windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
                    env: { ...process.env, PCB_BOARD_BACKEND: 'cubecl', PCB_BOARD_PACKER_THREADS: '1' },
                });
                let output = '', log = '';
                child.stdout!.on('data', data => { output += data; });child.stderr!.on('data', data => { log += data; });
                child.on('error', fail);child.on('close', code => {
                    if (code !== 0) { fail(Error(log));return; }
                    try { ok({ solution: JSON.parse(output), log }); } catch (error) { fail(error); }
                });
                child.stdin!.end(JSON.stringify(p));
            })));
            for (const row of results) {
                assert.deepEqual(row.solution, cpu.solution);
                assert.doesNotMatch(row.log, /GPU owned by another process|board-gpu-fallback/);
                assert.match(row.log, /"backend":"cubecl"/);
            }
            assert.equal(owner.exitCode, null);
        }
    } finally { owner.kill();await new Promise<void>(done => owner.once('close', () => done())); }
    const next = run(p, 'cubecl');
    assert.match(next.log, /"backend":"cubecl"/);
    assert.deepEqual(next.solution, cpu.solution);
});


test('empty and fully locked calls report CPU without GPU initialization', () => {
    const p = minimalProblem();p.primitives[0].locked = true;
    const empty = { ...p, primitives: [], components: [], componentConflict: [], componentPairClearance: [] };
    for (const input of [p, empty]) {
        const cpu = run(input, 'cpu'), gpu = run(input, 'cubecl');
        assert.deepEqual(gpu.solution, cpu.solution);
        assert.doesNotMatch(gpu.log, /\[block-gpu-runtime\]/);
        assert.match(gpu.log, /"backend":"cpu"/);
    }
});

test('empty and fully locked boards complete on CPU when both GPU drivers are unavailable', () => {
    const locked = minimalProblem();locked.primitives[0].locked = true;
    const empty = { ...locked, primitives: [], components: [], componentConflict: [], componentPairClearance: [] };
    for (const input of [locked, empty]) {
        const cpu = run(input, 'cpu');
        for (const backend of ['auto', 'cubecl']) {
            const fallback = run(input, backend, {
                PCB_GPU_RUNTIME: 'auto', PCB_GPU_CUDA_DISABLED: '1', PCB_GPU_METAL_DISABLED: '1',
                VK_DRIVER_FILES: resolve('debugging/nonexistent-vulkan-driver.json'),
            });
            assert.deepEqual(fallback.solution, cpu.solution);
            assert.doesNotMatch(fallback.log, /\[block-gpu-runtime\]/);
            assert.match(fallback.log, /"backend":"cpu"/);
        }
    }
});

test('unsupported layers and inverted boxes fall back before initialization', () => {
    const p = minimalProblem();p.components[0].bodyBox.right = -1.1;
    const layer = minimalProblem();(layer.components[0] as { layer: string }).layer = 'inner2';
    for (const input of [p, layer]) {
        const cpu = run(input, 'cpu'), gpu = run(input, 'cubecl');
        assert.deepEqual(gpu.solution, cpu.solution);
        assert.doesNotMatch(gpu.log, /\[block-gpu-runtime\]/);
        assert.match(gpu.log, /"backend":"cpu"/);
    }
});

test('negative score terms preserve shortlist selection without nonnegative bounds', { skip: !gpuEnabled }, () => {
    const p = minimalProblem();
    const second = structuredClone(p.primitives[0]);second.id = 'primitive:second';
    second.placements[0].designator = 'U2';second.connectionPoints[0].ref = 'U2.1';
    p.primitives.push(second);p.components.push({ ...p.components[0], primitiveId: second.id, designator: 'U2' });
    p.componentConflict = [0, 1, 1, 0];p.componentPairClearance = [0, .2, .2, 0];
    p.softSpacing = { gap: .35, compactnessScale: -.5, exemptPairs: [] };
    p.relations = [{ id: 'negative', kind: 'hint', from: 'component:U1', to: 'component:U2',
        hard: false, weight: -40, effect: 'score_only', satelliteAnchor: false, preferFacingPads: false }];
    const cpu = run(p, 'cpu');
    const gpu = run(p, 'cubecl', { PCB_BOARD_GPU_VERIFY: '1', PCB_BOARD_GPU_VERIFY_SHORTLIST: '1' });
    assert.deepEqual(gpu.solution, cpu.solution);
    assert.match(gpu.log, /"backend":"cubecl"/);
    assert.doesNotMatch(gpu.log, /fallback|validation failed/);
});


test('directional component matrices preserve the CPU cache semantics through an early guard', () => {
    const p = minimalProblem();
    p.components.push({ ...structuredClone(p.components[0]), designator: 'U2' });
    p.componentConflict = [0, 1, 1, 0];p.componentPairClearance = [0, .2, .3, 0];
    const cpu = run(p, 'cpu'), gpu = run(p, 'cubecl');
    assert.deepEqual(gpu.solution, cpu.solution);
    assert.doesNotMatch(gpu.log, /\[block-gpu-runtime\]/);
    assert.match(gpu.log, /asymmetric board GPU component rules|CPU-only build/);
});


test('unsafe edge generator offsets fall back before GPU initialization', () => {
    const p = minimalProblem();p.primitives[0].edgePlace = { edges: ['left'], offset: 1e9 };
    const cpu = run(p, 'cpu'), gpu = run(p, 'cubecl');
    assert.deepEqual(gpu.solution, cpu.solution);
    assert.doesNotMatch(gpu.log, /\[block-gpu-runtime\]/);
    assert.match(gpu.log, /unsafe board GPU number|CPU-only build/);
});


test('runtime selection uses CUDA without Vulkan and Vulkan without CUDA', {skip:process.env.PCB_CUDA_GPU_TESTS!=='1'},()=>{
    const p=minimalProblem();const cpu=run(p,'cpu');
    const cuda=run(p,'cubecl',{PCB_GPU_RUNTIME:'auto',VK_DRIVER_FILES:resolve('debugging/nonexistent-vulkan-driver.json')});
    assert.match(cuda.log,/"backend":"cuda"/);assert.doesNotMatch(cuda.log,/board-gpu-fallback/);
    assert.deepEqual(cuda.solution,cpu.solution);
    const vulkan=run(p,'cubecl',{PCB_GPU_RUNTIME:'auto',PCB_GPU_CUDA_DISABLED:'1'});
    assert.match(vulkan.log,/"backend":"vulkan"/);assert.doesNotMatch(vulkan.log,/board-gpu-fallback/);
    assert.deepEqual(vulkan.solution,cpu.solution);
});

test('GPU allocation failure discards the call and restores its CPU result', {skip:!gpuEnabled},()=>{
    const p=minimalProblem();const cpu=run(p,'cpu');
    const failed=run(p,'cubecl',{PCB_GPU_FAIL_ALLOCATION:'1'});
    assert.match(failed.log,/injected GPU out of memory/);
    assert.match(failed.log,/board-gpu-fallback/);assert.deepEqual(failed.solution,cpu.solution);
});

test('CUDA server allocation failure returns CPU recovery without invalid-handle panics', {skip:process.env.PCB_CUDA_GPU_TESTS!=='1'},()=>{
    const p=minimalProblem();
    const cpu=run(p,'cpu');
    const gpu=run(p,'cubecl',{PCB_GPU_RUNTIME:'cuda',PCB_CUDA_FAIL_ALLOCATION_AFTER:'8'});
    assert.deepEqual(gpu.solution,cpu.solution);
    assert.match(gpu.log,/"backend":"cuda"/);
    assert.match(gpu.log,/board-gpu-fallback/);
    assert.match(gpu.log,/CUDA out of memory/);
    assert.doesNotMatch(gpu.log,/Memory page .* doesn't exist|memory allocation of .* failed/);
});


test('module membership is encoded from the tree before rigid ownership is resolved', () => {
    const p = encodeNativeBoardPackProblem({
        node: {id: 'board', kind: 'board', label: 'board', children: [
            {id: 'tree:module:analog', kind: 'module', label: 'analog', children: [
                {id: 'tree:block:divider', kind: 'block', label: 'divider', children: [
                    {id: 'tree:component:R1', kind: 'component', label: 'R1', children: []},
                    {id: 'tree:component:R2', kind: 'component', label: 'R2', children: []},
                ]},
            ]},
            {id: 'tree:component:U1', kind: 'component', label: 'U1', children: []},
        ]},
        primitives: [], relations: [], options: {grid: .5, clearance: .2,
            bounds: {left:-10, right:10, top:-10, bottom:10}},
    });
    assert.deepEqual(p.relationGroups, {'module:analog': ['R1', 'R2']});
});

test('module membership spanning rigid owners is rejected instead of picking the first', () => {
    const p = minimalProblem();
    p.relationGroups = {'module:split': ['U1', 'U2']};
    const other = structuredClone(p.primitives[0]);other.id = 'other';other.placements[0].designator = 'U2';
    p.primitives.push(other);
    p.components.push({...p.components[0], designator: 'U2', primitiveId: 'other'});
    p.componentConflict = [0, 1, 1, 0];p.componentPairClearance = [0, .2, .2, 0];
    p.relations = [{id:'split', kind:'hint', from:'module:split', to:'anchor:board.right',
        hard:false, effect:'move_from', satelliteAnchor:false}];
    assert.throws(() => require(addonPath).evaluateBoardQuality(p), /spans multiple rigid primitives/);
});


test('semantic clearance, layer and edge hints keep CPU/GPU candidate ranks and legality', {skip:!gpuEnabled},()=>{
    const p=minimalProblem();p.searchWidth=4;
    const source=structuredClone(p.primitives[0]);
    p.primitives=[0,1,2].map(i=>({...structuredClone(source),id:`P${i}`,
        placements:[{...source.placements[0],designator:`U${i}`,layer:i===1?'bottom':'top'}],
        connectionPoints:[{ref:`U${i}.1`,net:'SIG',x:.8,y:0}],connectionPointLayers:[i===1?'bottom':'top']}));
    p.components=p.primitives.map((q,i)=>({...structuredClone(p.components[0]),primitiveId:q.id,designator:`U${i}`,blockName:i<2?'pair':'other',layer:i===1?'bottom':'top'}));
    p.componentConflict=Array.from({length:9},(_,i)=>Number(Math.floor(i/3)!==i%3));p.componentPairClearance=p.componentConflict.map(v=>v*.2);
    const relation=(name:string,from:string,to:string,condition={},hard=false)=>({id:name,kind:'hint',relation:name,from,to,condition,hard,effect:'move_from',priority:'normal',weight:70,minDistance:1,satelliteAnchor:false});
    p.relations=[relation('clearance','block:pair','component:U2',{},true),relation('away_from','component:U0','component:U1'),
        relation('same_side','block:pair','component:U2'),relation('prefer_layer','component:U0','board',{layer:'bottom'}),
        relation('edge','component:U2','board',{edge:'right',orientation:'any'})];
    const cpu=run(p,'cpu');const gpu=run(p,'cubecl',{PCB_BOARD_GPU_VERIFY:'1',PCB_BOARD_GPU_VERIFY_SHORTLIST:'1'});
    assert.doesNotMatch(gpu.log,/fallback/);assert.match(gpu.log,/"backend":"cubecl"/);assert.deepEqual(gpu.solution,cpu.solution);
});
