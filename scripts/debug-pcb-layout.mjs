import { createHash } from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import { copyFileSync, createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';

const backend = fileURLToPath(new URL('../', import.meta.url));
const debugRoot = join(backend, 'debugging', 'pcb-layout');
const isoId = () => new Date().toISOString().replace(/[:.]/g, '-');
const json = file => JSON.parse(readFileSync(file, 'utf8'));
const writeJson = (file, value) => writeFileSync(file, JSON.stringify(value, null, 2));
const sha256 = value => createHash('sha256').update(value).digest('hex');
const safe = value => value.replace(/[^a-zA-Z0-9_-]+/g, '_');

function captures(directory) {
    const base = join(directory, 'native');
    const rows = [];
    if (!existsSync(base)) return rows;
    for (const kind of readdirSync(base)) for (const processName of readdirSync(join(base, kind)))
        for (const item of readdirSync(join(base, kind, processName))) {
            const path = join(base, kind, processName, item);
            const meta = join(path, 'meta.json');
            if (existsSync(meta)) rows.push({ ...json(meta), path: path.slice(directory.length + 1).replaceAll('\\', '/') });
        }
    return rows.sort((a, b) => a.capturedAt.localeCompare(b.capturedAt));
}

function nativeProfile(logFile) {
    const lines = readFileSync(logFile, 'utf8').split(/\r?\n/);
    const blocks = [];
    const board = [];
    const detail = [];
    for (const line of lines) {
        const block = line.match(/\[pcb-block-solver\] components=(\d+) beam_ms=([\d.]+) singles_ms=([\d.]+) pairs_ms=([\d.]+)/);
        if (block) blocks.push({ components: +block[1], beamMs: +block[2], singlesMs: +block[3], pairsMs: +block[4] });
        const boardLine = line.match(/\[pcb-board-packer\] (.*)/);
        if (boardLine) board.push(boardLine[1]);
        const detailLine = line.match(/\[block-detail\] (\{.*\})/);
        if (detailLine) {
            try { detail.push(JSON.parse(detailLine[1])); } catch { /* Keep the raw log for partial/interleaved lines. */ }
        }
    }
    const totals = {};
    for (const item of detail) for (const [name, pair] of Object.entries(item.totals ?? {})) {
        const sum = totals[name] ?? [0, 0];
        sum[0] += pair[0];
        sum[1] += pair[1];
        totals[name] = sum;
    }
    return { blockStageSamples: blocks, boardStageLines: board, blockDetailSamples: detail.length,
        detailTotals: Object.fromEntries(Object.entries(totals).map(([name, [calls, nanoseconds]]) =>
            [name, { calls, milliseconds: nanoseconds / 1e6 }])) };
}

async function capture(fixture) {
    const directory = join(backend, 'tests', 'pcb-layout', fixture);
    if (!existsSync(directory)) throw Error(`Unknown PCB layout fixture: ${fixture}`);
    const runners = readdirSync(directory).filter(name => name.endsWith('.ts'));
    if (runners.length !== 1) throw Error(`Expected one runner in ${directory}; found ${runners.length}`);
    const output = join(debugRoot, 'runs', safe(fixture), isoId());
    mkdirSync(output, { recursive: true });
    const sourceDir = join(output, 'source');
    mkdirSync(sourceDir);
    const sourceFiles = readdirSync(directory).filter(name =>
        name.endsWith('.js') || name.endsWith('.json') || name === runners[0]);
    for (const file of sourceFiles) copyFileSync(join(directory, file), join(sourceDir, file));
    const log = createWriteStream(join(output, 'run.log'));
    const start = performance.now();
    const child = spawn(process.execPath, ['--import', 'tsx', join(directory, runners[0])], {
        cwd: backend,
        env: { ...process.env, PCB_LAYOUT_DEBUG_DIR: output, PCB_BLOCK_SOLVER_PROFILE: '1',
            PCB_BLOCK_SOLVER_DETAIL: '1', PCB_BOARD_PACKER_PROFILE: '1' },
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => { log.write(chunk); });
    const code = await new Promise((done, reject) => { child.on('error', reject); child.on('close', done); });
    await new Promise(done => log.end(done));
    const rows = captures(output);
    const phases = existsSync(join(output, 'stages.json')) ? json(join(output, 'stages.json')) : null;
    const profile = nativeProfile(join(output, 'run.log'));
    const revision = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: backend, encoding: 'utf8' }).trim();
    const require = createRequire(import.meta.url);
    const nativeFile = join(backend, 'native', 'pcb-board-packer',
        require('../native/pcb-board-packer/platform.cjs').nativeFilename());
    const nativeSha256 = existsSync(nativeFile) ? sha256(readFileSync(nativeFile)) : null;
    const summary = { version: 1, fixture, status: code === 0 ? 'completed' : 'failed', exitCode: code,
        revision, nativeSha256, sourceFiles: Object.fromEntries(sourceFiles.map(file =>
            [file, sha256(readFileSync(join(sourceDir, file)))])),
        wallMs: performance.now() - start, phases, profile, nativeRequests: rows.length,
        blockCalls: rows.filter(row => row.kind === 'block').length,
        boardCalls: rows.filter(row => row.kind === 'board').length,
        note: 'batchWallMs is the whole batch duration, not individual hypothesis CPU time.', captures: rows };
    writeJson(join(output, 'summary.json'), summary);
    writeFileSync(join(output, 'summary.md'), [
        `# PCB layout: ${fixture}`, '', `Status: ${summary.status} (exit ${code})`,
        `Wall time: ${(summary.wallMs / 1000).toFixed(2)} s`,
        `Native requests: ${rows.length} (${summary.blockCalls} block, ${summary.boardCalls} board; cache hits may be included)`,
        `Placement valid: ${phases?.placementOk ?? 'unknown'}`, '',
        '## Stage wall times', '',
        ...Object.entries(phases?.stagesMs ?? {}).map(([name, ms]) => `- ${name}: ${(ms / 1000).toFixed(2)} s`),
        '', '## Native detail totals', '',
        ...Object.entries(profile.detailTotals).map(([name, item]) => `- ${name}: ${item.calls} calls, ${item.milliseconds.toFixed(1)} ms accumulated`),
        '',
        '## Files', '', '- [Run log](run.log)',
        ...(existsSync(join(output, 'placement', 'placement.svg')) ? ['- [Placement preview](placement/placement.svg)'] : []),
        ...(existsSync(join(output, 'placement', 'board.assemble.json')) ? ['- [Assembly JSON](placement/board.assemble.json)'] : []),
        ...(existsSync(join(output, 'placement', 'input.json')) ? ['- [Resolved placement input](placement/input.json)'] : []),
        '- Original fixture files: `source/`', '- [Machine summary](summary.json)',
        '- Exact native inputs and outputs: `native/`', '',
        'Batch wall time belongs to the complete batch and must not be summed across its hypotheses.',
        'Compare result quality and write the experiment verdict in `docs/experimental/pcb/`.', '',
    ].join('\n'));
    console.log(`PCB capture ${fixture}: ${summary.status} in ${(summary.wallMs / 1000).toFixed(1)} s; ${rows.length} native requests`);
    console.log(`Summary: ${join(output, 'summary.md')}`);
    if (code !== 0) console.error(`Details: ${join(output, 'run.log')}`);
    if (code !== 0) process.exitCode = code || 1;
}

function replay(source, count) {
    const inputDir = existsSync(join(source, 'meta.json')) ? source : dirname(source);
    const meta = json(join(inputDir, 'meta.json'));
    if (!['block', 'board'].includes(meta.kind)) throw Error('Expected block or board capture');
    const raw = readFileSync(join(inputDir, 'problem.json'), 'utf8');
    if (sha256(raw) !== meta.inputSha256) throw Error('Captured native input checksum mismatch');
    const problem = JSON.parse(raw, (_key, value) => {
        if (value && typeof value === 'object' && Object.keys(value).length === 1 && '$nativeNumber' in value) {
            if (value.$nativeNumber === '-0') return -0;
            if (value.$nativeNumber === 'NaN') return NaN;
            if (value.$nativeNumber === 'Infinity') return Infinity;
            if (value.$nativeNumber === '-Infinity') return -Infinity;
        }
        return value;
    });
    const baseline = existsSync(join(inputDir, 'solution.json')) ? json(join(inputDir, 'solution.json')) : null;
    const output = join(debugRoot, 'replays', meta.kind, `${isoId()}-${safe(meta.label || 'unnamed')}`);
    mkdirSync(output, { recursive: true });
    const addon = loadNativeBoardPacker();
    const times = [];
    let solution;
    for (let i = 0; i < count; i++) {
        const start = performance.now();
        solution = meta.kind === 'block' ? addon.solveBlockPrimitives(problem) : addon.solveBoardPacked(problem);
        times.push(performance.now() - start);
    }
    const exact = baseline ? JSON.stringify(solution) === JSON.stringify(baseline) : null;
    const summary = { version: 1, source: resolve(inputDir), kind: meta.kind, label: meta.label,
        inputSha256: meta.inputSha256, repeats: count, timesMs: times,
        minMs: Math.min(...times), medianMs: [...times].sort((a, b) => a - b)[Math.floor(times.length / 2)],
        exactBaselineMatch: exact, baselineRank: baseline?.rank ?? null, resultRank: solution.rank };
    writeJson(join(output, 'summary.json'), summary);
    writeJson(join(output, 'solution.json'), solution);
    writeFileSync(join(output, 'summary.md'), `# Native ${meta.kind} replay: ${meta.label}\n\n` +
        `Input SHA-256: ${meta.inputSha256}\n\n` +
        `Times: ${times.map(time => time.toFixed(2)).join(', ')} ms\n\n` +
        `Exact baseline match: ${exact}\n\n` +
        'Compare score, validity and placement preview before judging an experiment.\n');
    console.log(output);
}

const [command, first, ...rest] = process.argv.slice(2);
if (command === 'capture' && first) await capture(first);
else if (command === 'replay' && first) {
    const count = Number(rest[0] ?? 1);
    if (!Number.isSafeInteger(count) || count < 1 || count > 100) throw Error('Repeat count must be 1..100');
    replay(resolve(first), count);
} else {
    console.log('Usage: node --import tsx scripts/debug-pcb-layout.mjs capture <fixture>');
    console.log('       node --import tsx scripts/debug-pcb-layout.mjs replay <capture-dir-or-meta.json> [repeats]');
    process.exitCode = 1;
}
