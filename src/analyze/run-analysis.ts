import { backendResource } from '../runtime/resources.ts';
import { AnalysisPool } from './analysis-pool.ts';
import type { AnalysisProblem, RunAnalysisOptions } from './types.ts';
export { AnalysisExecutionError } from './types.ts';
export type { AnalysisImportReport, AnalysisProblem, AnalysisProgress, RunAnalysisOptions } from './types.ts';

let pool: AnalysisPool | undefined;
let disposing: Promise<void> | undefined;

export function runAnalysisProblem(problem: AnalysisProblem, options?: RunAnalysisOptions) {
    // During disposal the closed pool rejects new requests. Only confirmed
    // successful shutdown permits a fresh lazy generation.
    pool ??= new AnalysisPool(backendResource('dist', 'run-analysis.worker.js'));
    return pool.run(problem, options);
}

export function terminateAnalysisPool(): Promise<void> {
    if (disposing) return disposing;
    if (!pool) return Promise.resolve();
    const current = pool;
    disposing = current.dispose().then(() => { pool = undefined; })
        .finally(() => { disposing = undefined; });
    return disposing;
}
