// Exact board inputs and independent full-board evidence. Never edit references.
import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { closeSync, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

const require = createRequire(import.meta.url);
const args = Object.fromEntries(process.argv.slice(3).map(arg => {
    const i = arg.indexOf('=');
    if (i < 1) throw Error(`Expected name=value: ${arg}`);
    return [arg.slice(0, i), arg.slice(i + 1)];
}));
const mode = process.argv[2];
const root = resolve('debugging/board-gpu-2026-10-01');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const json = path => JSON.parse(readFileSync(path, 'utf8'), (_, value) => {
    if (value && typeof value === 'object' && Object.keys(value).length === 1 && '$nativeNumber' in value) {
        return { '-0': -0, NaN, Infinity, '-Infinity': -Infinity }[value.$nativeNumber];
    }
    return value;
});
const write = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2));
const addonPath = resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH ??
    join('native/pcb-board-packer', require('../native/pcb-board-packer/platform.cjs').nativeFilename()));
function sources() {
    const paths = execFileSync('rg', ['--files', 'native/pcb-board-packer/src',
        'src/pcb-layout/pcb-auto-place-v2', 'scripts'], { encoding: 'utf8' }).trim().split(/\r?\n/);
    paths.push('native/pcb-board-packer/Cargo.toml', 'native/pcb-board-packer/Cargo.lock', 'package.json');
    return Object.fromEntries(paths.sort().map(path => [path, hash(readFileSync(path))]));
}
function evidence() {
    const addonSha256 = hash(readFileSync(addonPath));
    const sourceReport = args.sourceReport ? json(resolve(args.sourceReport)) : null;
    if (sourceReport && sourceReport.addonSha256 !== addonSha256) throw Error('Source evidence belongs to another addon');
    return { revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
        addonPath, addonSha256, sourceHashes: sourceReport?.sourceHashes ?? sources(),
        sourceReport: args.sourceReport ?? null,
        sourceEvidenceKind: sourceReport ? 'manifest verified against addon hash' : 'working tree; build correspondence unverified',
        node: process.version, harnessSha256: hash(readFileSync(new URL(import.meta.url))),
        settings: Object.fromEntries(Object.entries(process.env).filter(([key]) => key.startsWith('PCB_'))) };
}
function output(label) {
    const path = resolve(args.out ?? join(root, `${label}-${Date.now()}`));
    // Explicit outputs are immutable evidence too; do not silently overwrite them.
    if (existsSync(path) && process.env.PCB_BOARD_GPU_EXPERIMENT_CHILD !== path) throw Error(`Output already exists: ${path}`);
    mkdirSync(path, { recursive: true });
    return path;
}
process.env.PCB_NATIVE_SOLVE_CACHE = '0';
process.env.PCB_BOARD_BACKEND = args.backend ?? 'cpu';
process.env.PCB_BOARD_PACKER_THREADS = args.workers ?? '1';
if (!['cpu', 'cubecl', 'auto'].includes(process.env.PCB_BOARD_BACKEND)) throw Error('Invalid board backend');
if (args.detail === '1') process.env.PCB_BOARD_PACKER_DETAIL = '1';
process.env.PCB_BOARD_PACKER_PROFILE = '1';

if (['capture', 'replay'].includes(mode) && !process.env.PCB_BOARD_GPU_EXPERIMENT_CHILD) {
    const out = output(`${mode}-${args.fixture ?? process.env.PCB_BOARD_BACKEND}`);
    const stdout = openSync(join(out, 'stdout.log'), 'wx'), stderr = openSync(join(out, 'stderr.log'), 'wx');
    const cli = process.argv.slice(2).filter(arg => !arg.startsWith('out='));
    const child = spawnSync(process.execPath, [...process.execArgv, process.argv[1], ...cli, `out=${out}`], {
        env: { ...process.env, PCB_BOARD_GPU_EXPERIMENT_CHILD: out },
        stdio: ['ignore', stdout, stderr], windowsHide: true,
    });
    closeSync(stdout); closeSync(stderr);
    const log = readFileSync(join(out, 'stderr.log'), 'utf8');
    const parse = (tag) => log.split(/\r?\n/).filter(line => line.startsWith(`${tag} `)).map(line => JSON.parse(line.slice(tag.length + 1)));
    const reportPath = join(out, 'results.json');
    if (existsSync(reportPath)) {
        const report = json(reportPath);
        report.backendCalls = parse('[board-backend]');
        report.gpuStages = parse('[board-gpu-stage]');
        report.fallbacks = parse('[board-gpu-fallback]');
        report.cpuDetails = parse('[board-detail]');
        report.stageCheckpoints = parse('[board-stage-checkpoint]');
        report.exitCode = child.status;
        report.actualBackends = [...new Set(report.backendCalls.map(call => call.backend))];
        write(reportPath, report);
        if (mode === 'replay' && args.backend === 'cubecl' && args.requireGpu !== '0'
            && (report.actualBackends.length !== 1 || report.actualBackends[0] !== 'cubecl')) {
            throw Error('GPU replay used a fallback or performed no GPU work; inspect results.json');
        }
    }
    process.stdout.write(readFileSync(join(out, 'stdout.log'), 'utf8'));
    if (child.status !== 0) process.stderr.write(log.slice(-12000));
    if (child.error) console.error(child.error);
    process.exit(child.status ?? 1);
} else if (mode === 'build') {
    if (process.env.PCB_BOARD_PACKER_NATIVE_PATH) throw Error('build requires the production addon path');
    const out = output('build');
    const before = sources();
    const revisionBefore = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
    for (const path of Object.keys(before)) {
        const dest = join(out, 'source', path);
        mkdirSync(dirname(dest), { recursive: true }); copyFileSync(path, dest);
    }
    write(join(out, 'source-hashes-before.json'), before);
    execFileSync(process.execPath, ['scripts/build-native.mjs'], { stdio: 'inherit', windowsHide: true });
    if (!isDeepStrictEqual(before, sources())) throw Error('Source files changed during build; addon provenance is unverified');
    copyFileSync(addonPath, join(out, 'solver.node'));
    write(join(out, 'manifest.json'), { ...evidence(), sourceEvidenceKind: 'sources frozen and verified across native build', revisionBefore });
    console.log(`Verified build: ${out}`);
} else if (mode === 'snapshot') {
    const out = output('baseline');
    const manifest = evidence();
    copyFileSync(addonPath, join(out, 'solver.node'));
    for (const path of Object.keys(manifest.sourceHashes)) {
        const dest = join(out, 'source', path);
        mkdirSync(dirname(dest), { recursive: true });
        copyFileSync(path, dest);
    }
    writeFileSync(join(out, 'working-tree.patch'), execFileSync('git', ['diff', '--binary']));
    writeFileSync(join(out, 'git-status.txt'), execFileSync('git', ['status', '--short']));
    write(join(out, 'manifest.json'), manifest);
    console.log(`Saved baseline: ${out}`);
} else if (mode === 'capture') {
    const fixture = args.fixture ?? 'Telemetry';
    const out = output(`capture-${fixture}`);
    // All non-board backends and worker budgets are fixed for comparisons.
    process.env.PCB_BLOCK_BACKEND = args.block ?? 'auto';
    process.env.PCB_LAYOUT_WORKERS = '1';
    process.env.PCB_LAYOUT_SUBTREE_WORKERS = '0';
    process.env.PCB_POST_PLACE_THREADS = '1';
    process.env.PCB_LAYOUT_DEBUG_DIR = out;
    const inputPath = resolve(`tests/fixtures/block-placement/${fixture}/input.json`);
    const inputBytes = readFileSync(inputPath);
    const input = JSON.parse(inputBytes);
    const { autoPlacePcbWithReportAsync, renderPlacementSvg } =
        await import('../src/pcb-layout/pcb-auto-place/auto-place.ts');
    const { createPlacementDebugArtifacts, writePlacementArtifacts } = await import('../src/pcb-layout/artifacts.ts');
    const before = evidence();
    const start = performance.now();
    const result = await autoPlacePcbWithReportAsync(input);
    const wallMs = performance.now() - start;
    writePlacementArtifacts(join(out, 'placement'), input, result.placements, result.report, result.layout,
        result.stages, renderPlacementSvg(input, result.placements),
        createPlacementDebugArtifacts(input, result.placements));
    write(join(out, 'results.json'), { ...before, fixture, inputPath, inputSha256: hash(inputBytes), wallMs,
        placementOk: result.report.ok, report: result.report, placements: result.placements,
        scope: 'complete resolved board, including ordinary/aligned, portfolio and refiner' });
    console.log(JSON.stringify({ out, fixture, wallMs, placementOk: result.report.ok }));
} else if (mode === 'replay') {
    if (!args.inputs) throw Error('replay requires inputs=comma-separated-native-capture-directories');
    const out = output(`replay-${process.env.PCB_BOARD_BACKEND}`);
    const runs = Number(args.runs ?? 4);
    if (!Number.isSafeInteger(runs) || runs < 1) throw Error('Invalid runs');
    const inputs = args.inputs.split(',').map(path => {
        const source = resolve(path), bytes = readFileSync(join(source, 'problem.json'));
        const meta = existsSync(join(source, 'meta.json')) ? json(join(source, 'meta.json')) : null;
        if (meta && (meta.kind !== 'board' || meta.inputSha256 !== hash(bytes))) throw Error(`Invalid capture: ${source}`);
        return { source, sha256: hash(bytes), problem: json(join(source, 'problem.json')) };
    });
    const reference = args.reference ? json(resolve(args.reference)) : null;
    const captured = inputs.every(input => existsSync(join(input.source, 'solution.json')))
        ? inputs.map(input => json(join(input.source, 'solution.json'))) : null;
    if (reference && !isDeepStrictEqual(reference.inputs, inputs.map(({ problem, ...input }) => input)))
        throw Error('Reference input identities differ');
    const before = evidence();
    const addon = require(addonPath), rows = [];
    for (let run = 0; run < runs; run++) {
        const callMs = [], solutions = [];
        const started = performance.now();
        for (const input of inputs) {
            const start = performance.now();
            solutions.push(addon.solveBoardPacked(input.problem));
            callMs.push(performance.now() - start);
        }
        const wallMs = performance.now() - started;
        const exactReferenceMatch = reference ? isDeepStrictEqual(solutions, reference.rows[0].solutions) : null;
        const exactCaptureMatch = captured ? isDeepStrictEqual(solutions, captured) : null;
        rows.push({ run, first: run === 0, callMs, wallMs, exactReferenceMatch, exactCaptureMatch, solutions });
        write(join(out, 'results.json'), { ...before, inputs: inputs.map(({ problem, ...input }) => input), rows,
            scope: 'exact native board calls; excludes block/refiner and TypeScript portfolio',
            comparisonMode: args.comparison === 'quality' ? 'quality; requires separate geometry/metric review' : 'exact',
            validation: process.env.PCB_BOARD_GPU_VERIFY === '1',
            shortlistValidation: process.env.PCB_BOARD_GPU_VERIFY_SHORTLIST === '1',
            stageValidation: process.env.PCB_BOARD_GPU_VERIFY_STAGES === '1' });
        console.log(JSON.stringify({ run, wallMs, callMs, exactReferenceMatch, out }));
        if (args.comparison !== 'quality' && (exactReferenceMatch === false || exactCaptureMatch === false))
            throw Error('Board result differs from CPU reference/captured output');
    }
} else throw Error('Use build, snapshot, capture or replay');
