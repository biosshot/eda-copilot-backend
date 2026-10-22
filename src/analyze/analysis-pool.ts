import { randomUUID } from 'node:crypto';
import { availableParallelism } from 'node:os';
import { performance } from 'node:perf_hooks';
import workerpool, { type Pool } from 'workerpool';
import { placementWorkerForkOptions } from '../pcb-layout/worker-process-options.ts';
import env from '../utils/env.ts';
import {
    AnalysisExecutionError, isAnalysisProblem,
    type AnalysisExecutionCode, type AnalysisResult, type AnalysisProblem,
    type AnalysisProgress, type RunAnalysisOptions,
} from './types.ts';

type Config = { maxWorkers: number; maxQueueSize: number; timeoutMs: number; terminateTimeoutMs: number };
type Job = {
    id: string; problem: AnalysisProblem; options: RunAnalysisOptions; deadline: number;
    resolve: (report: AnalysisResult) => void; reject: (error: AnalysisExecutionError) => void;
    timer?: ReturnType<typeof setTimeout>; abort: () => void; settled: boolean; started: boolean; slot?: Slot;
};
type Slot = { pool: Pool; job?: Job; stopping?: Promise<void> };
const DEFAULT_TIMEOUT_MS = 30 * 60 * 1000;

function envInt(name: string, fallback: number, min: number, max: number) {
    if (!env[name]?.trim()) return fallback;
    const value = Number(env[name]);
    return Number.isSafeInteger(value) && value >= min && value <= max ? value : fallback;
}

export function analysisPoolConfig(): Config {
    return {
        maxWorkers: envInt('PCB_ANALYZE_WORKERS', 1, 1, availableParallelism()),
        maxQueueSize: envInt('PCB_ANALYZE_WORKER_QUEUE_SIZE', 4, 0, 10_000),
        timeoutMs: envInt('PCB_ANALYZE_WORKER_TIMEOUT_MS', DEFAULT_TIMEOUT_MS, 1, 2_147_483_647),
        terminateTimeoutMs: envInt('PCB_ANALYZE_WORKER_TERMINATE_TIMEOUT_MS', 1_000, 1, 60_000),
    };
}

// Analysis-specific FIFO. Each execution slot owns one reusable workerpool process.
// Public terminate() can then stop precisely the occupied process and await its exit,
// without reaching into workerpool internals or terminating a neighbouring job.
export class AnalysisPool {
    private readonly queue: Job[] = [];
    private readonly slots = new Set<Slot>();
    private disposing?: Promise<void>;
    private closed = false;
    private draining = false;

    constructor(private readonly workerScript: string, private readonly config = analysisPoolConfig()) {}

    run<P extends AnalysisProblem>(problem: P, options?: RunAnalysisOptions): Promise<AnalysisResult<P>>;
    run(problem: AnalysisProblem, options: RunAnalysisOptions = {}): Promise<AnalysisResult> {
        if (this.closed) return Promise.reject(new AnalysisExecutionError('DISPOSED', 'Analysis pool is disposing or disposed'));
        if (!isAnalysisProblem(problem) || !validOptions(options)) {
            return Promise.reject(new AnalysisExecutionError('INVALID_REQUEST', 'Invalid analysis problem or execution options'));
        }
        if (options.signal?.aborted) return Promise.reject(new AnalysisExecutionError('ABORTED', 'Analysis task aborted'));
        const freeSlots = [...this.slots].filter(slot => !slot.job && !slot.stopping).length
            + this.config.maxWorkers - this.slots.size;
        if (this.queue.length >= this.config.maxQueueSize + freeSlots) {
            return Promise.reject(new AnalysisExecutionError('QUEUE_FULL', 'Analysis waiting queue is full'));
        }
        const acceptedAt = performance.now();
        const snapshot = { ...options };
        return new Promise((resolve, reject) => {
            const job: Job = {
                id: randomUUID(), problem: problem.kind === 'prepare-geometry'
                    ? { kind: problem.kind, boardJson: problem.boardJson, ...(problem.geometry ? { geometry: { ...problem.geometry } } : {}) }
                    : { kind: problem.kind, boardJson: problem.boardJson }, options: snapshot,
                deadline: acceptedAt + (snapshot.timeoutMs ?? this.config.timeoutMs),
                resolve, reject, settled: false, started: false, abort: () => this.fail(job, 'ABORTED'),
            };
            this.queue.push(job);
            snapshot.signal?.addEventListener('abort', job.abort, { once: true });
            this.emit(job, 'queued');
            // A progress callback may synchronously abort or dispose.
            if (snapshot.signal?.aborted) job.abort();
            if (!job.settled) this.armDeadline(job);
            this.drain();
        });
    }

    dispose(): Promise<void> {
        if (this.disposing) return this.disposing;
        this.closed = true;
        // Install the shared promise before progress callbacks can reenter dispose.
        this.disposing = Promise.resolve().then(async () => {
            for (const job of [...this.queue, ...[...this.slots].flatMap(slot => slot.job ? [slot.job] : [])]) {
                this.fail(job, 'DISPOSED');
            }
            const results = await Promise.allSettled([...this.slots].map(slot => this.stop(slot)));
            const errors = results.flatMap(result => result.status === 'rejected' ? [result.reason] : []);
            if (errors.length) throw new AggregateError(errors, 'Analysis worker shutdown failed');
        });
        return this.disposing;
    }

    private emit(job: Job, phase: AnalysisProgress['phase']) {
        try { void Promise.resolve(job.options.onProgress?.({ jobId: job.id, phase })).catch(() => undefined); }
        catch { /* Progress observers cannot affect execution. */ }
    }

    private armDeadline(job: Job) {
        const remaining = job.deadline - performance.now();
        if (remaining <= 0) { this.fail(job, 'TIMEOUT'); return; }
        // Node timers have an int32 limit. Long finite deadlines are rearmed.
        job.timer = setTimeout(() => this.armDeadline(job), Math.min(remaining, 2_147_483_647));
    }

    private finish(job: Job, report?: AnalysisResult, error?: AnalysisExecutionError) {
        if (job.settled) return;
        job.settled = true;
        clearTimeout(job.timer);
        job.options.signal?.removeEventListener('abort', job.abort);
        const index = this.queue.indexOf(job);
        if (index >= 0) this.queue.splice(index, 1);
        if (error) job.reject(error); else job.resolve(report!);
        this.emit(job, error ? (error.code === 'ABORTED' ? 'cancelled' : 'failed') : 'completed');
    }

    private fail(job: Job, code: AnalysisExecutionCode, cause?: unknown) {
        if (job.settled) return;
        // Mark the slot as stopping before callbacks can submit another task.
        if (job.slot) void this.stop(job.slot).catch(() => undefined);
        this.finish(job, undefined, new AnalysisExecutionError(code, `Analysis execution: ${code}`, { cause }));
        this.drain();
    }

    private stop(slot: Slot): Promise<void> {
        if (slot.stopping) return slot.stopping;
        // workerpool first requests clean exit, then kills a blocked process at
        // terminateTimeoutMs. The outer deadline includes time to observe exit.
        slot.stopping = Promise.resolve().then(() => slot.pool.terminate(true, this.config.terminateTimeoutMs + 5_000))
            .then(() => { this.slots.delete(slot); this.drain(); });
        // If shutdown fails, retain the occupied slot: never oversubscribe by
        // replacing a process whose exit has not been confirmed.
        return slot.stopping;
    }

    private drain() {
        if (this.closed || this.draining) return;
        this.draining = true;
        try {
            while (!this.closed && this.queue.length) {
                const job = this.queue[0];
                if (performance.now() >= job.deadline) { this.fail(job, 'TIMEOUT'); continue; }
                let slot = [...this.slots].find(candidate => !candidate.job && !candidate.stopping);
                if (!slot) {
                    if (this.slots.size >= this.config.maxWorkers) return;
                    slot = { pool: workerpool.pool(this.workerScript, {
                        workerType: 'process', forkOpts: placementWorkerForkOptions,
                        maxWorkers: 1, maxQueueSize: 1, workerTerminateTimeout: this.config.terminateTimeoutMs,
                    }) };
                    this.slots.add(slot);
                }
                this.queue.shift();
                slot.job = job;
                job.slot = slot;
                this.execute(slot, job);
            }
        } finally { this.draining = false; }
    }

    private execute(slot: Slot, job: Job) {
        let result: PromiseLike<AnalysisResult>;
        try {
            result = slot.pool.exec('importAnalysisInWorker', [job.problem, job.id], { on: event => {
                if (job.settled || job.started || event?.jobId !== job.id || event.phase !== 'started') return;
                if (performance.now() >= job.deadline) { this.fail(job, 'TIMEOUT'); return; }
                job.started = true;
                this.emit(job, 'started');
            } });
        } catch (error) { this.fail(job, 'WORKER_FAILED', error); return; }
        // Assimilate workerpool's custom thenable; observe every rejection.
        void Promise.resolve(result).then(report => {
            if (job.settled) return;
            if (performance.now() >= job.deadline) { this.fail(job, 'TIMEOUT'); return; }
            slot.job = undefined;
            this.finish(job, report);
            this.drain();
        }, error => this.fail(job, 'WORKER_FAILED', error)).catch(() => undefined);
    }
}

function validOptions(options: RunAnalysisOptions): boolean {
    return options !== null && typeof options === 'object' && !Array.isArray(options)
        && Object.keys(options).every(key => key === 'signal' || key === 'timeoutMs' || key === 'onProgress')
        && (options.timeoutMs === undefined || (typeof options.timeoutMs === 'number' && Number.isFinite(options.timeoutMs) && options.timeoutMs > 0))
        && (options.signal === undefined || options.signal instanceof AbortSignal)
        && (options.onProgress === undefined || typeof options.onProgress === 'function');
}
