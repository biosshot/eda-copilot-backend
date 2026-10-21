import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, existsSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { getEventListeners } from 'node:events';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import nodeTest, { type TestContext } from 'node:test';
import workerpool from 'workerpool';
import { AnalysisPool, analysisPoolConfig } from '../src/analyze/analysis-pool.ts';
import { runAnalysisProblem, terminateAnalysisPool } from '../src/analyze/run-analysis.ts';
import { disposeBackend, makePcbLayout } from '../src/pcb.ts';
import { pcbInput } from './fixtures/api-fixtures.mjs';
import type { AnalysisProblem, AnalysisProgress } from '../src/analyze/types.ts';

const worker = resolve('tests/fixtures/analysis-worker.cjs');
const test = (name: string, fn: (t: TestContext) => void | Promise<void>) => nodeTest(name, { timeout: 30_000 }, fn);
const code = (value: string) => ({ code: value });
const problem = (command: object = {}) => ({ kind: 'import-board', boardJson: JSON.stringify(command) } as const);
const alive = (pid: number) => { try { process.kill(pid, 0); return true; } catch { return false; } };

async function until(condition: () => boolean, message: string, timeout = 10_000) {
    const deadline = performance.now() + timeout;
    while (!condition()) {
        assert.ok(performance.now() < deadline, message);
        await delay(10);
    }
}

function harness(t: TestContext, maxWorkers = 1, maxQueueSize = 4) {
    const dir = mkdtempSync(join(tmpdir(), 'analysis-runtime-'));
    const pool = new AnalysisPool(worker, { maxWorkers, maxQueueSize, timeoutMs: 15_000, terminateTimeoutMs: 50 });
    const localPids = new Set<number>();
    let sequence = 0;
    const task = (mode = 'block') => {
        const ready = join(dir, `ready-${sequence++}`), release = ready + '-release';
        return { problem: problem({ mode, ready, release }), ready, release,
            async started() {
                await until(() => existsSync(ready), 'worker did not report readiness');
                const pid = Number(readFileSync(ready, 'utf8'));
                localPids.add(pid);
                assert.notEqual(pid, process.pid);
                return pid;
            },
            unblock() { writeFileSync(release, 'go'); },
        };
    };
    t.after(async () => {
        await pool.dispose();
        for (const pid of localPids) assert.equal(alive(pid), false, `Leaked worker ${pid}`);
        assert.ok(resolve(dir).startsWith(resolve(tmpdir(), 'analysis-runtime-')));
        rmSync(dir, { recursive: true, force: true });
    });
    return { pool, task, dir };
}

test('runtime rejects invalid requests and pre-aborted signals without starting a process', async t => {
    const { pool, task, dir } = harness(t);
    for (const invalid of [null, {}, { kind: 'future', boardJson: '{}' }, { kind: 'import-board', boardJson: 4 },
        { ...problem(), extra: true }]) {
        await assert.rejects(pool.run(invalid as AnalysisProblem), code('INVALID_REQUEST'));
    }
    for (const options of [null, [], { timeoutMs: 0 }, { timeoutMs: Infinity }, { timeoutMs: NaN }, { timeoutMs: -1 },
        { onProgress: 2 }, { signal: {} }, { extra: true }]) {
        await assert.rejects(pool.run(problem(), options as never), code('INVALID_REQUEST'));
    }
    const job = task('return');
    await assert.rejects(pool.run(job.problem, { signal: AbortSignal.abort() }), code('ABORTED'));
    assert.deepEqual(readdirSync(dir), []);
    assert.equal((await pool.run(job.problem)).imported, true);
    await job.started();
});

test('environment configuration never creates unlimited workers/queue and supports zero waiting slots', () => {
    const keys = ['PCB_ANALYZE_WORKERS', 'PCB_ANALYZE_WORKER_QUEUE_SIZE', 'PCB_ANALYZE_WORKER_TIMEOUT_MS', 'PCB_ANALYZE_WORKER_TERMINATE_TIMEOUT_MS'];
    const saved = keys.map(key => process.env[key]);
    try {
        for (const value of ['Infinity', 'NaN', '-1', '1.5', '99999999999999999999', 'bad']) {
            keys.forEach(key => { process.env[key] = value; });
            assert.deepEqual(analysisPoolConfig(), { maxWorkers: 1, maxQueueSize: 4, timeoutMs: 1_800_000, terminateTimeoutMs: 1_000 });
        }
        process.env.PCB_ANALYZE_WORKERS = '1'; process.env.PCB_ANALYZE_WORKER_QUEUE_SIZE = '0';
        assert.equal(analysisPoolConfig().maxQueueSize, 0);
    } finally { keys.forEach((key, i) => { if (saved[i] === undefined) delete process.env[key]; else process.env[key] = saved[i]; }); }
});

test('four waiting jobs exclude the active job; queued cancellation frees capacity', async t => {
    const { pool, task } = harness(t);
    const active = task(), activeResult = pool.run(active.problem);
    await active.started();
    const abort = new AbortController(), events: AnalysisProgress[] = [];
    const cancelled = pool.run(problem(), { signal: abort.signal, onProgress: e => { events.push(e); } });
    const cancelledCheck = assert.rejects(cancelled, code('ABORTED'));
    const queued = Array.from({ length: 3 }, () => pool.run(problem()));
    await assert.rejects(pool.run(problem()), code('QUEUE_FULL'));
    abort.abort(); await cancelledCheck;
    assert.equal(getEventListeners(abort.signal, 'abort').length, 0);
    queued.push(pool.run(problem()));
    active.unblock();
    await Promise.all([activeResult, ...queued]);
    assert.deepEqual(events.map(e => e.phase), ['queued', 'cancelled']);
});

test('zero queue accepts an idle slot and rejects only waiting work', async t => {
    const { pool, task } = harness(t, 1, 0);
    const active = task(), result = pool.run(active.problem);
    await active.started();
    await assert.rejects(pool.run(problem()), code('QUEUE_FULL'));
    active.unblock(); await result;
    assert.equal((await pool.run(problem())).imported, true);
});

test('cancelling synchronous work kills its process and subsequent work succeeds', async t => {
    const { pool, task } = harness(t);
    const active = task(), abort = new AbortController(), events: AnalysisProgress[] = [];
    const result = pool.run(active.problem, { signal: abort.signal, onProgress: e => { events.push(e); } });
    const rejected = assert.rejects(result, code('ABORTED'));
    const pid = await active.started();
    abort.abort(); await rejected;
    assert.equal(getEventListeners(abort.signal, 'abort').length, 0);
    assert.equal((await pool.run(problem())).imported, true);
    assert.equal(alive(pid), false);
    active.unblock();
    assert.deepEqual(events.map(e => e.phase), ['queued', 'started', 'cancelled']);
});

test('queued deadline expires without executing and releases its waiting slot', async t => {
    const { pool, task } = harness(t, 1, 1);
    const active = task(), result = pool.run(active.problem);
    await active.started();
    const queued = task('return'), events: AnalysisProgress[] = [];
    await assert.rejects(pool.run(queued.problem, { timeoutMs: 100, onProgress: e => { events.push(e); } }), code('TIMEOUT'));
    assert.equal(existsSync(queued.ready), false);
    assert.deepEqual(events.map(e => e.phase), ['queued', 'failed']);
    const next = pool.run(problem());
    active.unblock(); await result; await next;
});

test('deadline includes queue time and is not restarted at worker dispatch', async t => {
    const { pool, task } = harness(t);
    const active = task(), first = pool.run(active.problem);
    await active.started();
    const second = task(), events: AnalysisProgress[] = [], accepted = performance.now();
    const result = pool.run(second.problem, { timeoutMs: 1_500, onProgress: e => { events.push(e); } });
    const check = assert.rejects(result, code('TIMEOUT'));
    // Deliberate elapsed queue budget, rather than a race with process startup.
    await delay(900); active.unblock(); await first; await second.started();
    await check;
    assert.ok(performance.now() - accepted < 2_100, 'queue budget was incorrectly reset');
    assert.deepEqual(events.map(e => e.phase), ['queued', 'started', 'failed']);
    await pool.run(problem());
});

test('timeout stops synchronous work, rejects late results, and recovers', async t => {
    const { pool, task } = harness(t);
    const active = task(), events: AnalysisProgress[] = [];
    const result = pool.run(active.problem, { timeoutMs: 2_000, onProgress: e => { events.push(e); } });
    const rejected = assert.rejects(result, code('TIMEOUT'));
    const pid = await active.started(); await rejected;
    await pool.run(problem());
    assert.equal(alive(pid), false);
    active.unblock();
    assert.deepEqual(events.map(e => e.phase), ['queued', 'started', 'failed']);
});

test('unexpected worker exit is typed, never retried, and the next task runs', async t => {
    const { pool, task } = harness(t);
    const active = task('exit'), events: AnalysisProgress[] = [];
    await assert.rejects(pool.run(active.problem, { onProgress: e => { events.push(e); } }), code('WORKER_FAILED'));
    const pid = await active.started();
    await pool.run(problem());
    assert.equal(alive(pid), false);
    assert.deepEqual(events.map(e => e.phase), ['queued', 'started', 'failed']);
});

test('two workers isolate cancellation from a neighbouring synchronous task', async t => {
    const { pool, task } = harness(t, 2);
    const first = task(), second = task(), abort = new AbortController();
    const a = pool.run(first.problem, { signal: abort.signal }), b = pool.run(second.problem);
    const check = assert.rejects(a, code('ABORTED'));
    const [pidA, pidB] = await Promise.all([first.started(), second.started()]);
    assert.notEqual(pidA, pidB);
    abort.abort(); await check;
    await pool.run(problem());
    assert.equal(alive(pidA), false); assert.equal(alive(pidB), true);
    second.unblock(); assert.equal((await b).imported, true);
});

test('two workers isolate timeout from a neighbouring synchronous task', async t => {
    const { pool, task } = harness(t, 2);
    const first = task(), second = task();
    const a = pool.run(first.problem, { timeoutMs: 2_000 }), b = pool.run(second.problem);
    const check = assert.rejects(a, code('TIMEOUT'));
    const [pidA, pidB] = await Promise.all([first.started(), second.started()]);
    await check;
    await pool.run(problem());
    assert.equal(alive(pidA), false); assert.equal(alive(pidB), true);
    second.unblock(); assert.equal((await b).imported, true);
});

test('waiting requests keep string snapshots and run in FIFO order', async t => {
    const { pool, task } = harness(t);
    const active = task(), first = pool.run(active.problem);
    await active.started();
    const next = task('return'), mutable = { ...next.problem }, order: number[] = [];
    const queued = [pool.run(mutable, { onProgress: event => { if (event.phase === 'started') order.push(1); } }),
        pool.run(problem(), { onProgress: event => { if (event.phase === 'started') order.push(2); } })];
    mutable.boardJson = '{';
    active.unblock(); await first; await Promise.all(queued);
    await next.started();
    assert.deepEqual(order, [1, 2]);
});

test('dispose settles active/queued jobs once, waits for exit, is idempotent and rejects reentry', async t => {
    const { pool, task } = harness(t);
    const active = task(), events: AnalysisProgress[] = [];
    const first = pool.run(active.problem, { onProgress: e => { events.push(e); } });
    const a = assert.rejects(first, code('DISPOSED'));
    const pid = await active.started();
    const second = pool.run(problem()), b = assert.rejects(second, code('DISPOSED'));
    const disposal = pool.dispose(); assert.equal(pool.dispose(), disposal);
    await assert.rejects(pool.run(problem()), code('DISPOSED'));
    await Promise.all([a, b, disposal]);
    assert.equal(alive(pid), false);
    assert.deepEqual(events.map(e => e.phase), ['queued', 'started', 'failed']);
});

test('completion/cancel races and throwing/rejecting observers do not corrupt queue or leak rejections', async t => {
    const { pool, task } = harness(t);
    const unhandled: unknown[] = [], listener = (error: unknown) => { unhandled.push(error); };
    process.on('unhandledRejection', listener);
    try {
        for (let i = 0; i < 8; i++) {
            const active = task(), abort = new AbortController(), phases: string[] = [];
            const result = pool.run(active.problem, { signal: abort.signal, onProgress: e => {
                phases.push(e.phase);
                if (e.phase === 'completed') abort.abort();
                if (i % 2) return Promise.reject(new Error('observer rejection'));
                throw new Error('observer exception');
            } });
            const outcome = result.then(() => 'completed', e => { assert.equal(e.code, 'ABORTED'); return 'cancelled'; });
            await active.started(); active.unblock();
            if (i % 2) abort.abort();
            assert.equal(await outcome, phases.at(-1));
            assert.equal(phases.filter(p => ['completed', 'cancelled', 'failed'].includes(p)).length, 1);
        }
        await pool.dispose(); await delay(0);
        assert.deepEqual(unhandled, []);
    } finally { process.off('unhandledRejection', listener); }
});

test('completion/dispose races settle once without leaking processes or late events', async t => {
    for (let i = 0; i < 4; i++) {
        const { pool, task } = harness(t);
        const active = task(), phases: string[] = [];
        let disposal: Promise<void> | undefined;
        const result = pool.run(active.problem, { onProgress: event => {
            phases.push(event.phase);
            if (event.phase === 'completed') disposal = pool.dispose();
        } });
        const outcome = result.then(() => 'completed', error => { assert.equal(error.code, 'DISPOSED'); return 'failed'; });
        const pid = await active.started(); active.unblock();
        if (i % 2) disposal = pool.dispose();
        assert.equal(await outcome, phases.at(-1));
        await disposal;
        assert.equal(alive(pid), false);
        assert.equal(phases.filter(p => p === 'completed' || p === 'failed').length, 1);
    }
});

test('reentrant progress cancellation/dispose and nested submission remain bounded', async t => {
    const { pool } = harness(t, 1, 0);
    const abort = new AbortController(), events: string[] = [];
    await assert.rejects(pool.run(problem(), { signal: abort.signal, onProgress: e => {
        events.push(e.phase); if (e.phase === 'queued') abort.abort();
    } }), code('ABORTED'));
    assert.deepEqual(events, ['queued', 'cancelled']);
    let nested: Promise<unknown> | undefined;
    await pool.run(problem(), { onProgress: e => {
        if (e.phase === 'queued') nested = assert.rejects(pool.run(problem()), code('QUEUE_FULL'));
    } });
    await nested;
    let disposed: Promise<void> | undefined;
    await assert.rejects(pool.run(problem(), { onProgress: e => {
        if (e.phase === 'queued') disposed = pool.dispose();
    } }), code('DISPOSED'));
    await disposed;
});

test('eight actual addon reports and invalid-board diagnostics match the direct importer completely', async t => {
    t.after(terminateAnalysisPool);
    let addon: typeof import('../native/node-addon/index.cjs');
    try { addon = createRequire(import.meta.url)('../native/node-addon/index.cjs'); }
    catch (cause) { throw new Error('Build the shared addon with npm run native:build before native integration tests', { cause }); }
    assert.equal(typeof addon.importAnalysisBoard, 'function', 'Run npm run native:build (stale shared addon)');
    const root = resolve('tests/pcb-analyze');
    const names = readdirSync(root).filter(name => existsSync(join(root, name, 'fixture.json')));
    assert.equal(names.length, 8);
    for (const name of names) {
        const json = readFileSync(join(root, name, `${name}.json`), 'utf8'), events: AnalysisProgress[] = [];
        assert.deepEqual(await runAnalysisProblem({ kind: 'import-board', boardJson: json }, { onProgress: e => { events.push(e); } }), addon.importAnalysisBoard(json), name);
        assert.deepEqual(events.map(e => e.phase), ['queued', 'started', 'completed']);
        assert.equal(new Set(events.map(e => e.jobId)).size, 1);
    }
    for (const json of ['{', '{}']) {
        const events: AnalysisProgress[] = [];
        const report = await runAnalysisProblem({ kind: 'import-board', boardJson: json }, { onProgress: e => { events.push(e); } });
        assert.equal(report.imported, false); assert.deepEqual(report, addon.importAnalysisBoard(json));
        assert.equal(events.at(-1)?.phase, 'completed');
    }
    await Promise.all([disposeBackend(), disposeBackend()]);
    // A successfully disposed generation can be created lazily again.
    assert.equal((await runAnalysisProblem({ kind: 'import-board', boardJson: '{}' })).imported, false);
    await terminateAnalysisPool();
});

test('real addon loads only in a different PID while the parent event loop stays responsive', async t => {
    const dir = mkdtempSync(join(tmpdir(), 'analysis-pid-')), log = join(dir, 'dlopen.jsonl');
    t.after(() => { assert.ok(resolve(dir).startsWith(resolve(tmpdir(), 'analysis-pid-'))); rmSync(dir, { recursive: true, force: true }); });
    const source = `
        import assert from 'node:assert/strict';
        import { readFileSync, existsSync } from 'node:fs';
        import { runAnalysisProblem, terminateAnalysisPool } from './dist/analysis-runtime.js';
        assert.equal(existsSync(process.env.ANALYSIS_TEST_DLOPEN_LOG), false);
        let ticks = 0;
        const timer = setInterval(() => ticks++, 1);
        try {
            const boardJson = readFileSync('tests/pcb-analyze/NanoScope/NanoScope.json', 'utf8');
            const report = await runAnalysisProblem({ kind: 'import-board', boardJson });
            assert.equal(report.imported, true);
            assert.ok(ticks > 0, 'parent event loop was blocked');
            const records = readFileSync(process.env.ANALYSIS_TEST_DLOPEN_LOG, 'utf8').trim().split(/\\r?\\n/).map(JSON.parse);
            assert.ok(records.length > 0);
            assert.ok(records.every(r => r.pid !== process.pid));
            console.log(JSON.stringify(records));
        } finally { clearInterval(timer); await terminateAnalysisPool(); }
    `;
    const child = spawn(process.execPath, ['--input-type=module', '-e', source], {
        cwd: resolve('.'), windowsHide: true,
        env: { ...process.env, PCB_BLOCK_GPU_DISABLED: '1', ANALYSIS_TEST_DLOPEN_LOG: log,
            NODE_OPTIONS: `--require="${resolve('tests/fixtures/analysis-addon-pid.cjs').replaceAll('\\', '/')}"` },
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    const watchdog = setTimeout(() => child.kill(), 20_000);
    let stdout = '', stderr = '';
    child.stdout.on('data', data => { stdout += data; }); child.stderr.on('data', data => { stderr += data; });
    const exitCode = await new Promise<number | null>((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
    clearTimeout(watchdog);
    assert.equal(exitCode, 0, stderr + stdout);
    const records = JSON.parse(stdout.trim());
    for (const record of records) assert.equal(alive(record.pid), false);
    assert.doesNotMatch(stderr, /CubeCL|CUDA|Vulkan/i);
});

test('disposeBackend stops analysis even when placement shutdown rejects', async t => {
    const directory = mkdtempSync(join(tmpdir(), 'analysis-dispose-'));
    const ready = join(directory, 'ready'), release = join(directory, 'release');
    const originalPool = workerpool.pool.bind(workerpool);
    const injected = new Error('Test placement shutdown failure');
    t.mock.method(workerpool, 'pool', (script, options) => {
        const pool = originalPool(script.endsWith('run-analysis.worker.js') ? worker : script, options);
        if (script.endsWith('run-pcb-layout.worker.js')) {
            const terminate = pool.terminate.bind(pool);
            t.mock.method(pool, 'terminate', async (...args) => { await terminate(...args); throw injected; });
        }
        return pool;
    });
    t.after(async () => {
        t.mock.restoreAll(); await disposeBackend();
        assert.ok(resolve(directory).startsWith(resolve(tmpdir(), 'analysis-dispose-')));
        rmSync(directory, { recursive: true, force: true });
    });
    const board = await makePcbLayout(pcbInput);
    assert.ok(board.pcb, board.content);
    const result = runAnalysisProblem(problem({ mode: 'block', ready, release }));
    const rejected = assert.rejects(result, code('DISPOSED'));
    await until(() => existsSync(ready), 'analysis worker not ready');
    const pid = Number(readFileSync(ready, 'utf8'));
    await assert.rejects(disposeBackend(), (error: unknown) => error instanceof AggregateError && error.errors.includes(injected));
    await rejected;
    assert.equal(alive(pid), false);
});
