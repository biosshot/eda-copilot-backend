import { writeFile, rename, rm } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

/** Shared symbol-cache readers see either the old JSON or the complete new one.
 * A killed worker may leave a .tmp file; it never becomes a cache entry. */
export async function writeJsonAtomic(filename: string, value: unknown) {
    const temporary = `${filename}.${process.pid}.${randomUUID()}.tmp`;
    try {
        await writeFile(temporary, JSON.stringify(value, null, 2), { flag: 'wx' });
        for (let attempt = 0; ; attempt++) {
            try { await rename(temporary, filename); break; }
            catch (error) {
                // Windows can briefly lock the destination while another worker
                // reads it. Retry replacement without ever deleting the old JSON.
                if (process.platform !== 'win32' || !['EPERM', 'EACCES', 'EBUSY'].includes((error as NodeJS.ErrnoException).code ?? '') || attempt >= 7) throw error;
                await delay(Math.min(10 * 2 ** attempt, 160));
            }
        }
    } finally {
        await rm(temporary, { force: true });
    }
}

/** Fill a vacant slot immediately; a slow case does not hold up a whole batch.
 * Callback failures stop dispatch, but all active callbacks are awaited. */
export async function parallelJobs<T>(jobs: readonly T[], concurrency: number,
    run: (job: T, index: number) => Promise<void>, signal?: AbortSignal) {
    if (!Number.isSafeInteger(concurrency) || concurrency < 1) throw new Error('Concurrency must be a positive integer');
    let cursor = 0, stopped = false;
    const workers = Array.from({ length: Math.min(concurrency, jobs.length) }, async () => {
        while (!stopped && !signal?.aborted) {
            const index = cursor++;
            if (index >= jobs.length) return;
            try { await run(jobs[index], index); }
            catch (error) { stopped = true; throw error; }
        }
    });
    const settled = await Promise.allSettled(workers);
    const failure = settled.find(r => r.status === 'rejected');
    if (failure?.status === 'rejected') throw failure.reason;
}

/** One parent-owned writer; overlapping requests coalesce into the latest
 * snapshot instead of concurrently truncating the gallery/summary files. */
export function coalescedWriter(write: () => Promise<void>) {
    let pending = false, active: Promise<void> | undefined;
    return () => {
        pending = true;
        return active ??= Promise.resolve().then(async () => {
            try { while (pending) { pending = false; await write(); } }
            finally { active = undefined; }
        });
    };
}

export function runIsolated(args: string[], options: { cwd: string; timeoutMs: number; signal?: AbortSignal }) {
    return new Promise<{ code: number | null; timedOut: boolean; interrupted: boolean; log: string }>(done => {
        if (options.signal?.aborted) { done({ code: null, timedOut: false, interrupted: true, log: '' }); return; }
        const child = spawn(process.execPath, args, { cwd: options.cwd, windowsHide: true, env: { ...process.env, LOG_LEVEL: 'error' } });
        let log = '', timedOut = false, interrupted = false;
        const append = (s: unknown) => { log = (log + s).slice(-200000); };
        child.stdout.on('data', append); child.stderr.on('data', append);
        child.on('error', append);
        const stop = () => { interrupted = true; child.kill('SIGKILL'); };
        const timer = setTimeout(() => { timedOut = true; child.kill('SIGKILL'); }, options.timeoutMs);
        options.signal?.addEventListener('abort', stop, { once: true });
        // Reclaim the slot only after process exit, including spawn failures.
        child.on('close', code => {
            clearTimeout(timer); options.signal?.removeEventListener('abort', stop);
            done({ code, timedOut, interrupted, log });
        });
    });
}
