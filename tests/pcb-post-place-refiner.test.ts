import { encodeNativePostPlaceScoreProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-score.ts';
import { refinePostPlacement as refinePostPlacementSerial } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { terminatePcbSubtreeWorkerPool } from '../src/pcb-layout/pcb-auto-place-v2/tree-subtree-pool.ts';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import test from 'node:test';
import { availableParallelism } from 'node:os';
import { refinePostPlacementAsync, refinePostPlacement } from '../src/pcb-layout/pcb-auto-place-v2/post-place-refiner.ts';
import { runPcbLayoutDsl } from '../src/pcb-layout/pcb-layout-dsl/spec.ts';
import { defaultSolverOptions } from '../src/pcb-layout/pcb-auto-place/utils.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { encodeNativePostPlaceRefineProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import { postPlaceBudget } from '../src/pcb-layout/pcb-auto-place-v2/post-place-budget.ts';
import type { FootprintSpec, PcbComponent, Placement, PlacementInput } from '../src/types/pcb/layout-model.ts';

test.describe('post-place refinement', () => {
    test('compiles explicit refine groups and keeps rotations relative', () => {
        const rules = runPcbLayoutDsl(`
            refineGroup("headers", ["H1", "H2"], { swap: true, rotateBy: [180] });
        `);

        assert.deepEqual(rules.refineGroups, [{
            name: 'headers',
            component_designators: ['H1', 'H2'],
            swap: true,
            rotateBy: [180],
        }]);
        assert.throws(
            () => runPcbLayoutDsl('refineGroup("bad", ["H1", "H2"], { rotateBy: [90] });'),
            /only the relative 180 degree post-placement flip/,
        );
    });

    test('automatically accepts an atomic swap plus 180-degree rotation inside one block', () => {
        const input = pairInput();
        const before = pairPlacements();
        const result = refinePostPlacement(input, before);
        const a = placement(result.placements, 'A');
        const b = placement(result.placements, 'B');
        const move = result.moves[0];

        assert.equal(a.x, 5);
        assert.equal(b.x, -5);
        assert.ok(move?.description.includes('A<->B'));
        assert.ok(move?.description.includes('+=180'), move?.description);
        assert.ok(result.scoreAfter < result.scoreBefore);
        assert.ok(result.moves.every((candidate) => candidate.scoreAfter < candidate.scoreBefore));
        assert.ok(move);
        assert.ok(Number.isFinite(move.routePenaltyBefore));
        assert.ok(Number.isFinite(move.routePenaltyAfter));
        assert.ok(move.effectiveImprovement > 0);
    });

    test('does not mutate fixed poses without permission and reports the opportunity', () => {
        const input = pairInput({ fixed: true });
        const before = pairPlacements();
        const result = refinePostPlacement(input, before);

        assert.deepEqual(result.placements, before);
        assert.equal(result.moves.length, 0);
        assert.equal(result.diagnostics.length, 1);
        assert.match(result.diagnostics[0].message, /safe post-place improvement/);
        assert.match(result.diagnostics[0].message, /fixed placement must be preserved/);
        assert.match(result.diagnostics[0].message, /refineGroup\("post_A_B"/);
    });

    test('reports a fixed single-component rotation despite an unrelated existing hard violation', () => {
        const input = pairInput({ fixed: true });
        for (const designator of ['B', 'LEFT', 'RIGHT']) {
            componentByDesignator(input, designator).pcb.edgeMount = { edge: 'left' };
        }
        input.hints.push({
            relation: 'critical_pair',
            source: { type: 'pin', designator: 'LEFT', pin_number: '1' },
            target: { type: 'pin', designator: 'RIGHT', pin_number: '1' },
            priority: 'critical',
            maxDistance: 1,
            hard: true,
        });

        const result = refinePostPlacement(input, pairPlacements());

        assert.deepEqual(result.placements, pairPlacements());
        assert.equal(result.moves.length, 0);
        assert.equal(result.diagnostics.length, 1);
        assert.match(result.diagnostics[0].message, /A rotate \+= 180/);
        assert.match(result.diagnostics[0].message, /refineGroup\("post_A", \["A"\], \{ rotateBy: \[180\] \}\)/);
    });

    test('allows a fixed pose exchange only through an explicit refine group', () => {
        const input = pairInput({ fixed: true, refineGroup: true });
        const result = refinePostPlacement(input, pairPlacements());

        assert.equal(placement(result.placements, 'A').x, 5);
        assert.equal(placement(result.placements, 'B').x, -5);
        assert.equal(result.diagnostics.length, 0);
        assert.ok(result.moves.some((move) => move.description.startsWith('headers: A<->B')));
    });

    test('rejects a score improvement that introduces a hard collision', () => {
        const input = pairInput();
        input.components[0].footprint = footprint('large', 3, 1, -0.35);
        input.components[0].pcb.allowedRotations = [0];
        input.components[1].pcb.allowedRotations = [0];
        input.components.push(component('OBS', 'obstacle', 'OBS', footprint('obstacle', 1, 1, 0)));
        input.blocks.push(block('obstacle', ['OBS']));
        const before = [...pairPlacements(), pose('OBS', 6.2, 0)];

        const result = refinePostPlacement(input, before);

        assert.deepEqual(result.placements, before);
        assert.equal(result.moves.length, 0);
    });

    test('recognizes equivalent footprint geometry with a different library base angle', () => {
        const input = pairInput();
        input.components[0].part_uuid = null;
        input.components[1].part_uuid = null;
        input.components[0].footprint_uuid = null;
        input.components[1].footprint_uuid = null;
        input.components[0].pcb.allowedRotations = [0, 90, 180, 270];
        input.components[1].pcb.allowedRotations = [0, 90, 180, 270];
        input.components[0].footprint = {
            name: 'horizontal', width: 2, height: 1,
            pads: [{ pin_number: '1', x: 0.6, y: 0, width: 0.4, height: 0.2 }],
        };
        input.components[1].footprint = {
            name: 'vertical', width: 1, height: 2,
            pads: [{ pin_number: '1', x: 0, y: 0.6, width: 0.2, height: 0.4 }],
        };

        const result = refinePostPlacement(input, pairPlacements());

        assert.equal(placement(result.placements, 'A').x, 5);
        assert.equal(placement(result.placements, 'B').x, -5);
        assert.ok(result.moves.some((move) => move.kind === 'swap'));
    });
});

function pairInput(options: { fixed?: boolean; refineGroup?: boolean } = {}): PlacementInput {
    const pairFootprint = footprint('pair', 1, 1, -0.35);
    const components = [
        component('A', 'pair', 'RIGHT', pairFootprint, options.fixed),
        component('B', 'pair', 'LEFT', pairFootprint, options.fixed),
        component('LEFT', 'left-end', 'LEFT', footprint('left-end', 1.2, 1, 0), true),
        component('RIGHT', 'right-end', 'RIGHT', footprint('right-end', 1.4, 1, 0), true),
    ];
    return {
        board: {
            coordinateSystem: 'centered',
            outline: { type: 'rect', width: 40, height: 20 },
            defaultLayer: 'top',
            allowedLayers: ['top'],
            clearances: { component: 0.1, edge: 0.1 },
        },
        boardHoles: [],
        constraintRegions: [],
        components,
        blocks: [
            block('pair', ['A', 'B']),
            block('left-end', ['LEFT']),
            block('right-end', ['RIGHT']),
        ],
        modules: [],
        hints: [],
        paths: [],
        refineGroups: options.refineGroup ? [{
            name: 'headers',
            componentDesignators: ['A', 'B'],
            swap: true,
            rotateBy: [180],
        }] : [],
        solverOptions: {
            ...defaultSolverOptions,
            ignoredRatsnestSignals: [],
            localImproveIterations: 8,
            localImproveMinDelta: 0.001,
        },
    };
}

function component(
    designator: string,
    blockName: string,
    signal: string,
    componentFootprint: FootprintSpec,
    fixed = false,
): PcbComponent {
    return {
        designator,
        value: designator,
        pins: [{ pin_number: '1', name: '1', signal_name: signal }],
        block_name: blockName,
        search_query: '',
        part_uuid: blockName === 'pair' ? 'same-part' : null,
        footprint_uuid: null,
        footprint: componentFootprint,
        pcb: {
            role: 'passive',
            allowedLayers: ['top'],
            allowedRotations: [0, 180],
            ...(fixed ? { fixedPlacement: {} } : {}),
        },
    };
}

function footprint(name: string, width: number, height: number, padX: number): FootprintSpec {
    return {
        name,
        width,
        height,
        pads: [{ pin_number: '1', name: '1', x: padX, y: 0, width: 0.25, height: 0.25 }],
    };
}

function block(name: string, componentDesignators: string[]): PlacementInput['blocks'][number] {
    return { name, description: name, component_designators: componentDesignators, role: 'generic' };
}

function pairPlacements(): Placement[] {
    return [pose('A', -5, 0), pose('B', 5, 0), pose('LEFT', -9, 0), pose('RIGHT', 9, 0)];
}

function gpuRefine(problem: object, backend: string, options: Record<string,string> = {}, boundaryError?: RegExp) {
    const require = createRequire(import.meta.url);
    const addon = resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH ?? resolve('native/pcb-board-packer', require('../native/pcb-board-packer/platform.cjs').nativeFilename()));
    const child = spawnSync(process.execPath, ['-e',
        'const a=require(process.argv[1]),p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));const r=a.refinePostPlacement(p);console.log(JSON.stringify({result:r,valid:a.validatePlacementChange(p,r.placements)}));', addon], {
        input: JSON.stringify(problem), encoding: 'utf8', windowsHide: true,
        env: { ...process.env, PCB_POST_PLACE_BACKEND: backend, ...options },
    });
    if (boundaryError) {
        assert.notEqual(child.status,0);
        assert.match(child.stderr,boundaryError);
        assert.doesNotMatch(child.stderr,/\[post-place-gpu\]/);
        return {value:null,profile:null,log:child.stderr};
    }
    assert.equal(child.status, 0, child.stderr || String(child.error));
    const {result,valid} = JSON.parse(child.stdout);
    assert.equal(valid, true);
    const {profile,...value}=result;
    return {value,profile,log:child.stderr};
}

test('post-place GPU preserves full scores, paths, layers, clearance and stable winners', {skip:process.env.PCB_POST_PLACE_GPU_TESTS!=='1'}, () => {
    const input=pairInput();
    const problem=encodeNativePostPlaceRefineProblem(input,pairPlacements(),1);
    problem.padCrossingWeight=13;
    const target=(component:number)=>({kind:'component' as const,component});
    problem.hints.push(
        {kind:'distance',source:target(0),target:target(2),all:false,weight:1.7,min:1,max:6},
        {kind:'clearance',source:target(0),target:{kind:'missing'},all:true,weight:2,min:3},
        {kind:'same_side',source:target(0),target:target(1),all:false,weight:4},
        {kind:'prefer_layer',source:target(0),target:{kind:'missing'},all:false,weight:7,layer:'bottom'},
        {kind:'edge',source:{kind:'group',members:[0,1]},target:{kind:'missing'},all:false,weight:3,edge:'right'},
    );
    problem.paths=[{id:'probe',shape:'straight',priority:'high',preferFacingPads:true,
        ports:[0,2,1,3].map((component,order)=>({target:{kind:'pin',component,pad:0},order,ref:`p${order}`,role:'endpoint'}))}];
    problem.nets.push({name:'mst-ties',weight:0.65,points:[0,1,2,3].map(component=>({component,pad:0}))});
    const cpu=gpuRefine(problem,'cpu');
    const gpu=gpuRefine(problem,'cubecl',{PCB_POST_PLACE_GPU_VERIFY:'1'});
    assert.deepEqual(gpu.value,cpu.value);
    assert.match(gpu.log,/\[post-place-gpu\]/);
    assert.doesNotMatch(gpu.log,/fallback|validation failed/);
});

test('post-place GPU failure discards moves and restores the full CPU budget', {skip:process.env.PCB_POST_PLACE_GPU_TESTS!=='1'}, () => {
    const problem=encodeNativePostPlaceRefineProblem(pairInput(),pairPlacements(),1);
    const cpu=gpuRefine(problem,'cpu');
    for(const stage of ['batch','after_move','diagnostics']) {
        const replay=gpuRefine(problem,'cubecl',{PCB_POST_PLACE_GPU_FAIL:stage});
        assert.deepEqual(replay.value,cpu.value,stage);
        assert.match(replay.log,/\[post-place-gpu-fallback\]/);
        assert.match(replay.log,new RegExp(`"cpuBudgetMs":${problem.timeoutMs}`));
    }
});

test('post-place GPU chunk boundaries and worker budgets preserve the complete selection', {skip:process.env.PCB_POST_PLACE_GPU_TESTS!=='1'}, () => {
    const problem=encodeNativePostPlaceRefineProblem(pairInput(),pairPlacements(),1);
    const cpu=gpuRefine(problem,'cpu');
    for(const [chunk,workers] of [[1,1],[17,2],[128,4]]) {
        const gpu=gpuRefine({...problem,threads:workers},'cubecl',{PCB_POST_PLACE_GPU_CHUNK_SIZE:String(chunk),PCB_POST_PLACE_GPU_VERIFY:'1'});
        assert.deepEqual(gpu.value,cpu.value);
        assert.doesNotMatch(gpu.log,/fallback|validation failed/);
    }
});

test('post-place auto and zero timeout stay CPU; disabled GPU preserves the original result', {skip:process.env.PCB_POST_PLACE_GPU_TESTS!=='1'}, () => {
    const problem=encodeNativePostPlaceRefineProblem(pairInput(),pairPlacements(),1);
    const cpu=gpuRefine(problem,'cpu');
    const auto=gpuRefine(problem,'auto');
    assert.deepEqual(auto.value,cpu.value);
    assert.doesNotMatch(auto.log,/\[post-place-gpu\]/);
    const disabled=gpuRefine(problem,'cubecl',{PCB_BLOCK_GPU_DISABLED:'1'});
    assert.deepEqual(disabled.value,cpu.value);
    assert.match(disabled.log,/GPU disabled/);
    const zero=gpuRefine({...problem,timeoutMs:0},'cubecl',{PCB_BLOCK_SOLVER_PROFILE:'1'});
    assert.doesNotMatch(zero.log,/\[block-gpu-runtime\]/);
    assert.deepEqual(zero.value.placements,problem.placements);
    assert.equal(zero.profile.timedOut,true);
    assert.match(zero.log,/"batches":0/);
});

test('post-place idle and failed owners release the GPU while their process stays alive', {skip:process.env.PCB_POST_PLACE_GPU_TESTS!=='1',timeout:30000}, async () => {
    const p=encodeNativePostPlaceRefineProblem(pairInput(),pairPlacements(),1);
    const cpu=gpuRefine(p,'cpu');
    const require=createRequire(import.meta.url);
    const addon=resolve('native/pcb-board-packer',require('../native/pcb-board-packer/platform.cjs').nativeFilename());
    const owner=spawn(process.execPath,['-e',
        'const a=require(process.argv[1]),p=JSON.parse(require("node:fs").readFileSync(0,"utf8"));a.refinePostPlacement(p);process.send("ready");process.on("message",()=>{process.env.PCB_POST_PLACE_GPU_FAIL="after_move";a.refinePostPlacement(p);process.send("failed");});',addon],{
        windowsHide:true,stdio:['pipe','ignore','pipe','ipc'],env:{...process.env,PCB_POST_PLACE_BACKEND:'cubecl'},
    });
    let log='';owner.stderr!.on('data',data=>{log+=data;});
    const next=()=>new Promise<void>((ok,fail)=>{owner.once('message',()=>ok());owner.once('error',fail);});
    try {
        const ready=next();owner.stdin!.end(JSON.stringify(p));await ready;
        const nextCall=gpuRefine(p,'cubecl');assert.deepEqual(nextCall.value,cpu.value);assert.match(nextCall.log,/\[post-place-gpu\]/);assert.doesNotMatch(nextCall.log,/GPU owned by another process|fallback/);
        const failed=next();owner.send('fail');await failed;
        const recovered=gpuRefine(p,'cubecl');assert.deepEqual(recovered.value,cpu.value);assert.doesNotMatch(recovered.log,/fallback/);
        assert.equal(owner.exitCode,null);assert.match(log,/post-place-gpu-fallback/);
    }finally{owner.kill();}
});

test('post-place GPU guards and absent Vulkan driver replay the original CPU input', {skip:process.env.PCB_POST_PLACE_GPU_TESTS!=='1'}, () => {
    const original=encodeNativePostPlaceRefineProblem(pairInput(),pairPlacements(),1);
    const cases: [string, typeof original, Record<string,string>][]=[];
    const negative=structuredClone(original);negative.nets[0].weight=-1;
    cases.push(['negative MST crossing weight',negative,{}]);
    const duplicate=structuredClone(original);duplicate.nets.push(structuredClone(duplicate.nets[0]));
    cases.push(['duplicate post-place net names',duplicate,{}]);
    const unsafe=structuredClone(original);
    unsafe.hints.push({kind:'distance',source:{kind:'component',component:0},target:{kind:'point',point:{x:1e8,y:0}},all:false,weight:0});
    // Keep wider-frame rejection explicit; its support is not accepted yet.
    for (const backend of ['cpu','cubecl']) {
        gpuRefine(unsafe,backend,{},/coordinate exceeds the documented absolute\/local frame/);
    }
    const layer=structuredClone(original);
    layer.hints.push({kind:'prefer_layer',source:{kind:'component',component:0},target:{kind:'missing'},all:false,weight:1,layer:'inner'});
    cases.push(['unsupported post-place layer',layer,{}]);
    cases.push(['no compatible F32 Vulkan GPU',original,{PCB_GPU_RUNTIME:'vulkan',VK_DRIVER_FILES:resolve('debugging/nonexistent-vulkan-driver.json')}]);
    for(const [reason,problem,environment] of cases) {
        const cpu=gpuRefine(problem,'cpu');
        const gpu=gpuRefine(problem,'cubecl',environment);
        assert.deepEqual(gpu.value,cpu.value,reason);
        assert.ok(gpu.log.includes(reason),gpu.log);
        assert.doesNotMatch(gpu.log,/"backend":"cubecl"/);
    }
});

function pose(designator: string, x: number, y: number): Placement {
    return { designator, x, y, rotate: 0, layer: 'top', score: 0 };
}

function placement(placements: Placement[], designator: string) {
    const result = placements.find((item) => item.designator === designator);
    assert.ok(result, `${designator} must be placed`);
    return result;
}

function componentByDesignator(input: PlacementInput, designator: string) {
    const result = input.components.find((component) => component.designator === designator);
    assert.ok(result, `${designator} must exist`);
    return result;
}

// Compares serial and parallel Rust evaluation.
test('Rust threads match serial Rust moves and scores', async () => {
    process.env.PCB_POST_PLACE_THREADS = '3';
    try {
        for (const input of [pairInput(), pairInput({ fixed: true }), pairInput({ fixed: true, refineGroup: true })]) {
            const before = pairPlacements();
            const { profile: serialProfile, ...serial } = refinePostPlacementSerial(input, before);
            for (let repeat = 0; repeat < 2; repeat++) {
                const { profile, ...parallel } = await refinePostPlacementAsync(input, before);
                assert.deepEqual(parallel, serial);
                assert.equal(profile.workers, Math.max(1, Math.min(3, Math.floor(availableParallelism() / 2))));
                assert.deepEqual(profile.iterations.map(i => i.candidates), serialProfile.iterations.map(i => i.candidates));
            }
            input.solverOptions.localImproveIterations = 0;
            const { profile, ...disabled } = await refinePostPlacementAsync(input, before);
            const { profile: ignored, ...disabledSerial } = refinePostPlacement(input, before);
            assert.deepEqual(disabled, disabledSerial);
            assert.equal(profile.iterations.length, 0);
        }
    } finally {
        await terminatePcbSubtreeWorkerPool();
        delete process.env.PCB_POST_PLACE_THREADS;
    }
});

test('native threads match across board, side and hard-constraint cases', async () => {
    process.env.PCB_POST_PLACE_THREADS = '3';
    try {
        for (let scenario = 0; scenario < 12; scenario++) {
            const input = pairInput();
            const before = pairPlacements();
            input.solverOptions.localImproveIterations = 3;
            if (scenario === 1) input.board.outline = { type: 'polygon', width: 40, height: 20,
                points: [{ x: -20, y: -10 }, { x: 20, y: -10 }, { x: 20, y: 10 }, { x: 2, y: 10 }, { x: 2, y: 2 }, { x: -2, y: 2 }, { x: -2, y: 10 }, { x: -20, y: 10 }] };
            if (scenario === 2) {
                input.board.allowedLayers = ['top', 'bottom'];
                input.components[0].pcb.allowedLayers = ['top', 'bottom'];
                input.components[1].pcb.allowedLayers = ['top', 'bottom'];
                before[1].layer = 'bottom';
                input.components[0].footprint.pads[0].mount = 'through_hole';
                input.components[0].footprint.pads[0].drillDiameter = 0.3;
            }
            if (scenario === 3) input.constraintRegions = [{ name: 'ban', box: { left: 4, right: 6, top: -1, bottom: 1 }, layers: ['top'], allowBlocks: [] }];
            if (scenario === 4) input.boardHoles = [{ name: 'hole', x: 5, y: 0, drill: 0.6, diameter: 1, keepout: 1 }];
            if (scenario === 5) Object.assign(input.blocks[0], { hardAnchor: true, anchor: { type: 'component', designator: 'LEFT' }, maxAnchorGap: 2, anchorOffset: { x: 1, y: 1 } });
            if (scenario === 6) Object.assign(input.blocks[0], { hardBbox: true, maxBboxWidth: 5, maxBboxHeight: 3, familyHard: true, familyMaxWidth: 7 });
            if (scenario === 7) input.hints = [{ relation: 'critical_pair', source: { type: 'block', block_name: 'pair' }, target: { type: 'board_anchor', anchor: 'board.right' }, hard: true, maxDistance: 3, priority: 'critical' }];
            if (scenario === 8) input.hints = [
                { relation: 'clearance', source: { type: 'pin', designator: 'A', pin_number: '1' }, target: 'all', min: 1, priority: 'critical' },
                { relation: 'edge', source: { type: 'block', block_name: 'pair' }, edge: 'left', priority: 'high' },
                { relation: 'same_side', source: { type: 'component', designator: 'A' }, target: { type: 'component', designator: 'B' }, priority: 'high' },
            ];
            if (scenario === 9) input.paths = [{ id: 'path', priority: 'critical', shape: 'straight', preferFacingPads: true,
                stages: [], terminals: { first: { type: 'pin', designator: 'A', pin_number: '1' }, last: { type: 'pin', designator: 'RIGHT', pin_number: '1' } },
                segments: [{ index: 0, source: { type: 'pin', designator: 'A', pin_number: '1' }, target: { type: 'pin', designator: 'RIGHT', pin_number: '1' }, priority: 'critical' }] }];
            if (scenario === 10) input.modules = [{ name: 'm', description: '', block_names: ['pair'], hardBbox: true, maxWidth: 8, maxHeight: 4 }];
            if (scenario === 11) { input.components.reverse(); before.reverse(); }
            const { profile: ignored, ...expected } = refinePostPlacementSerial(input, before);
            const { profile: serialProfile, ...serial } = refinePostPlacement(input, before);
            const { profile: parallelProfile, ...parallel } = await refinePostPlacementAsync(input, before);
            assert.deepEqual(serial, expected, `serial scenario ${scenario}`);
            assert.deepEqual(parallel, expected, `parallel scenario ${scenario}`);
        }
    } finally { delete process.env.PCB_POST_PLACE_THREADS; }
});

test('whole refinement crosses the native boundary once and preserves caller data', () => {
    const input = pairInput();
    const poses = pairPlacements();
    const snapshot = structuredClone({ input, poses });
    const { profile: ignored, ...expected } = refinePostPlacementSerial(input, poses);
    const addon = loadNativeBoardPacker();
    const original = { refinePostPlacement: addon.refinePostPlacement, scorePostPlace: addon.scorePostPlace,
        prepareRouteLayoutComparison: addon.prepareRouteLayoutComparison, compareRouteLayoutCandidate: addon.compareRouteLayoutCandidate };
    let calls = 0;
    try {
        addon.refinePostPlacement = problem => { calls++; return original.refinePostPlacement(problem); };
        const unexpected = () => { throw new Error('Per-candidate native boundary must not be used'); };
        addon.scorePostPlace = unexpected;
        addon.prepareRouteLayoutComparison = unexpected;
        addon.compareRouteLayoutCandidate = unexpected;
        const { profile, ...actual } = refinePostPlacement(input, poses);
        assert.deepEqual(actual, expected);
        assert.equal(calls, 1);
        assert.deepEqual({ input, poses }, snapshot);
    } finally { Object.assign(addon, original); }
});

test('refinement caps oversized thread configuration at half CPUs and eight', async () => {
    const previous = process.env.PCB_POST_PLACE_THREADS;
    process.env.PCB_POST_PLACE_THREADS = '1000';
    try {
        const input = pairInput();
        input.solverOptions.localImproveIterations = 0;
        const result = await refinePostPlacementAsync(input, pairPlacements());
        assert.equal(result.profile.workers, Math.max(1, Math.min(8, Math.floor(availableParallelism() / 2))));
    } finally {
        if (previous === undefined) delete process.env.PCB_POST_PLACE_THREADS;
        else process.env.PCB_POST_PLACE_THREADS = previous;
    }
});

test('refine budget decreases with either component or pad complexity and respects smaller requested limits', () => {
    const input = pairInput();
    input.solverOptions.localImproveIterations = 16;
    const seed = input.components[0];
    for (const [components, pads, expected] of [[50, 250, 16], [51, 250, 12], [50, 251, 12],
        [100, 500, 12], [101, 500, 8], [100, 501, 8], [150, 1000, 8],
        [151, 1000, 5], [150, 1001, 5], [245, 1333, 5], [250, 1500, 5],
        [251, 1500, 3], [250, 1501, 3]]) {
        input.components = Array.from({ length: components }, (_, i) => ({ ...seed,
            footprint: { ...seed.footprint, pads: Array.from({ length: Math.floor(pads / components) + (i < pads % components ? 1 : 0) }, () => seed.footprint.pads[0]) } }));
        const budget = postPlaceBudget(input);
        assert.equal(budget.iterations, expected, `${components} components / ${pads} pads`);
        assert.equal(budget.pinCount, pads);
        assert.equal(budget.timeoutMs, 30_000);
    }
    input.solverOptions.localImproveIterations = 2;
    assert.equal(postPlaceBudget(input).iterations, 2);
    input.solverOptions.localImproveIterations = 0;
    assert.equal(postPlaceBudget(input).iterations, 0);
});

test('expired native budget returns the unchanged placement without evaluating candidates', () => {
    const input = pairInput({ fixed: true });
    const poses = pairPlacements();
    for (const threads of [1, 2]) {
        const problem = encodeNativePostPlaceRefineProblem(input, poses, threads);
        problem.timeoutMs = 0;
        const result = loadNativeBoardPacker().refinePostPlacement(problem);
        assert.equal(result.profile.stopReason, 'timeout');
        assert.equal(result.profile.timedOut, true);
        assert.equal(result.profile.iterations.length, 0);
        assert.deepEqual(result.placements, poses);
        assert.deepEqual(result.moves, []);
        assert.deepEqual(result.diagnostics, []);
    }
});

test('native timeout stops serial and parallel search and leaves a reusable valid result', () => {
    const input = pairInput();
    input.components = []; input.blocks = [];
    input.board.outline = { type: 'rect', width: 400, height: 400 };
    const poses: Placement[] = [];
    for (let tile = 0; tile < 32; tile++) {
        const part = pairInput();
        for (const c of part.components) {
            c.designator += `_${tile}`; c.block_name += `_${tile}`;
            c.pins.forEach(pin => { pin.signal_name += `_${tile}`; });
            if (c.pcb.fixedPlacement) c.pcb.edgeMount = { edge: 'left' };
            input.components.push(c);
        }
        input.blocks.push(...part.blocks.map(b => ({ ...b, name: `${b.name}_${tile}`, component_designators: b.component_designators.map(d => `${d}_${tile}`) })));
        poses.push(...pairPlacements().map(p => ({ ...p, designator: `${p.designator}_${tile}`, x: p.x + (tile % 8 - 3.5) * 30, y: p.y + (Math.floor(tile / 8) - 1.5) * 30 })));
    }
    for (const threads of [1, 2]) {
        const problem = encodeNativePostPlaceRefineProblem(input, poses, threads);
        problem.timeoutMs = 20;
        const result = loadNativeBoardPacker().refinePostPlacement(problem);
        assert.equal(result.profile.stopReason, 'timeout');
        assert.ok(result.profile.totalMs < 2000, 'cooperative timeout should not run all passes');
        assert.equal(result.placements.length, poses.length);
        assert.ok(result.profile.iterations.length > 0, 'exercise timeout inside the search');
        for (const c of input.components.filter(c => c.pcb.fixedPlacement)) {
            assert.deepEqual(result.placements.find(p => p.designator === c.designator), poses.find(p => p.designator === c.designator));
        }
        const restart = refinePostPlacement({ ...input, solverOptions: { ...input.solverOptions, localImproveIterations: 0 } }, result.placements);
        assert.equal(restart.scoreBefore, result.scoreAfter, 'partial search must roll back uncommitted scratch poses');
    }
});

 test('direct Rust search respects CPU caps and matches serial results', () => {
    const input = pairInput();
    const poses = pairPlacements();
    const addon = loadNativeBoardPacker();
    const { profile: serialProfile, ...serial } = addon.refinePostPlacement(encodeNativePostPlaceRefineProblem(input, poses, 1));
    const { profile: parallelProfile, ...parallel } = addon.refinePostPlacement(encodeNativePostPlaceRefineProblem(input, poses, 3));
    assert.equal(parallelProfile.workers, Math.max(1, Math.min(3, Math.floor(availableParallelism() / 2))));
    assert.ok(parallelProfile.iterations.length > 0);
    assert.deepEqual(parallel, serial);
});

test('geometric postrefine evaluates swaps without any micro-router jobs; board default retains routing', () => {
    const input=pairInput(), poses=pairPlacements(), addon=loadNativeBoardPacker();
    const problem=encodeNativePostPlaceRefineProblem(input,poses,1);
    assert.equal(problem.routingMetric,'micro');
    const board=addon.refinePostPlacement(problem);
    assert.ok(board.profile.iterations.some(p=>p.routeEvaluations>0));
    const block=addon.refinePostPlacement({...problem,routingMetric:'geometric'});
    assert.ok(block.moves.length>0);
    assert.ok(block.scoreAfter<block.scoreBefore);
    assert.ok(block.profile.iterations.every(p=>p.routeEvaluations===0));
    assert.ok(block.moves.every(m=>m.routePenaltyBefore===0&&m.routePenaltyAfter===0));
});


test('post-refine retains opposite SMD pad sides when rebuilding the world', () => {
    const input = pairInput();
    input.hints = [];
    input.paths = [];
    for (const component of input.components) {
        component.footprint = structuredClone(component.footprint);
        for (const pad of component.footprint.pads) pad.layer = component.designator === 'A' ? 'bottom' : 'top';
    }
    const poses = pairPlacements();
    const problem = encodeNativePostPlaceRefineProblem(input, poses, 1);
    assert.ok(problem.components.find(c => c.designator === 'A')?.pads.every(p => p.opposite));
    const addon = loadNativeBoardPacker();
    const result = addon.refinePostPlacement({ ...problem, routingMetric: 'geometric', iterations: 0 });
    const expected = addon.scorePostPlace(encodeNativePostPlaceScoreProblem(input, poses));
    assert.ok(Math.abs(result.scoreBefore - expected) < 0.002, `${result.scoreBefore} != ${expected}`);
    assert.deepEqual(result.placements, poses);
});


test('native refinement validates constraint regions against opposite-side pads', () => {
    const input = pairInput();
    input.hints = [];
    input.paths = [];
    const poses = pairPlacements();
    const component = componentByDesignator(input, 'A');
    component.footprint = structuredClone(component.footprint);
    const pose = poses.find(p => p.designator === 'A')!;
    input.constraintRegions = [{ name: 'bottom-keepout', layers: ['bottom'], allowBlocks: [],
        box: { left: pose.x - 2, right: pose.x + 2, top: pose.y - 2, bottom: pose.y + 2 } }];
    const addon = loadNativeBoardPacker();
    assert.equal(addon.validatePlacement(encodeNativePostPlaceRefineProblem(input, poses, 1)), true);
    component.footprint.pads[0].layer = 'bottom';
    assert.equal(addon.validatePlacement(encodeNativePostPlaceRefineProblem(input, poses, 1)), false);
});
