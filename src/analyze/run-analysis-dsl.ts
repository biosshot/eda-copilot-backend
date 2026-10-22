import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import workerpool from 'workerpool';
import { backendResource } from '../runtime/resources.ts';
import { placementWorkerForkOptions } from '../pcb-layout/worker-process-options.ts';
import { applyReportUpdate, json, type ReportUpdate } from './analysis-dsl/data.ts';
import { object } from './analysis-dsl/descriptions.ts';
import type { Report } from './analysis-dsl/spec-doc.js';
import { AnalysisDslError } from './analysis-dsl/schema.ts';
import type { ScenarioInput, AnalysisDslOutput, ScenarioError } from './analysis-dsl/scenario.ts';
export type { AnalysisDslOutput, ScenarioError } from './analysis-dsl/scenario.ts';
export type RunAnalysisDslInput = ScenarioInput;
export interface RunAnalysisDslOptions { signal?: AbortSignal; timeoutMs?: number; filename?: string; maxEntries?: number; maxOutputBytes?: number }

/** Read the standalone, installed DSL reference. PCB Layout documentation is kept
 * at dist/spec-doc.d.ts; Analysis uses dist/analysis-spec-doc.d.ts. */
export function analysisDslDocumentation(): string { return readFileSync(backendResource('dist', 'analysis-spec-doc.d.ts'), 'utf8'); }

/** One process per scenario, using the project's workerpool process mechanism.
 * A parent monotonic deadline includes startup and terminates synchronous loops
 * before/after await. No Promise.race-only cancellation, no shared mutable globals.
 * Isolation is for lifecycle control; node:vm is not a hostile-code sandbox.
 */
export async function runAnalysisDsl(input: RunAnalysisDslInput, options: RunAnalysisDslOptions = {}): Promise<AnalysisDslOutput> {
  if (!object(input) || typeof input.code !== 'string' || !input.code.trim() || Object.keys(input).some(k => !['code', 'board', 'profiles', 'models'].includes(k))) throw new AnalysisDslError('INVALID_REQUEST', 'Expected {code, board?, profiles?, models?}');
  if (!options || typeof options !== 'object' || Object.keys(options).some(k => !['signal', 'timeoutMs', 'filename', 'maxEntries', 'maxOutputBytes'].includes(k))) throw new AnalysisDslError('INVALID_REQUEST', 'Unknown scenario option');
  const timeout = options.timeoutMs ?? 120_000, maxEntries = options.maxEntries ?? 1000, maxOutputBytes = options.maxOutputBytes ?? 4 * 1024 * 1024;
  if (!Number.isFinite(timeout) || timeout <= 0 || !Number.isSafeInteger(maxEntries) || maxEntries < 1 || maxEntries > 100_000 || !Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 4096 || maxOutputBytes > 128 * 1024 * 1024 || options.filename !== undefined && (typeof options.filename !== 'string' || !options.filename.trim() || Buffer.byteLength(options.filename) > 512 || /[\x00-\x1f]/.test(options.filename)) || options.signal !== undefined && !(options.signal instanceof AbortSignal)) throw new AnalysisDslError('INVALID_REQUEST', 'Invalid scenario limits, filename or signal');
  const filename = options.filename ?? 'analysis-dsl.js';
  const reports: Report[] = [];
  const failure = (code: string, message: string): AnalysisDslOutput => ({ status: 'failed', reports, error: { code, message, filename, diagnostics: [] } });
  if (options.signal?.aborted) return failure('ABORTED', 'Analysis scenario aborted');
  const deadline = performance.now() + timeout;
  const snapshot = json(input);
  const pool = workerpool.pool(backendResource('dist', 'run-analysis-dsl.worker.js'), { workerType: 'process', minWorkers: 0, maxWorkers: 1, workerTerminateTimeout: 100, forkOpts: { ...placementWorkerForkOptions, execArgv: [] } });
  let stopped: AnalysisDslOutput | undefined;
  let stopping: Promise<unknown> | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stop = (code: string) => {
    if (stopped) return;
    stopped = failure(code, `Analysis scenario ${code === 'ABORTED' ? 'aborted' : 'deadline exceeded'}`);
    stopping = Promise.resolve(pool.terminate(true, 5100));
    void stopping.catch(() => {});
  };
  const abort = () => stop('ABORTED');
  function arm() {
    const remaining = deadline - performance.now();
    if (remaining <= 0) stop('TIMEOUT');
    else timer = setTimeout(arm, Math.min(remaining, 2_147_483_647));
  }
  options.signal?.addEventListener('abort', abort, { once: true });
  arm();
  try {
    if (options.signal?.aborted) abort();
    if (stopped) return stopped;
    const result = await pool.exec('runScenario', [snapshot, { filename, maxEntries, maxOutputBytes }], { on: event => { if (!stopped && object(event) && object(event.reportUpdate)) applyReportUpdate(reports, event.reportUpdate as ReportUpdate); } }) as AnalysisDslOutput;
    if (!stopped && performance.now() >= deadline) stop('TIMEOUT');
    return stopped ?? result;
  } catch (error) {
    return stopped ?? failure('WORKER_FAILED', error instanceof Error ? error.message : String(error));
  } finally {
    clearTimeout(timer); options.signal?.removeEventListener('abort', abort);
    // Await exit before resolving; do not replace or reuse a possibly stuck process.
    await (stopping ?? pool.terminate(true, 5100));
  }
}
