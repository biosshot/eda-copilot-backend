import { backendResource } from '../runtime/resources.ts';
import { AnalysisPool } from './analysis-pool.ts';
import type { AnalysisProblem, AnalysisResult, RunAnalysisOptions } from './types.ts';
export { AnalysisExecutionError } from './types.ts';
export type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions, AnalysisResult, AnalysisProblem, AnalysisProgress, RunAnalysisOptions } from './types.ts';
export type { AnalysisPolygon } from '../../native/analyze/index.js';

let pool: AnalysisPool | undefined;
let disposing: Promise<void> | undefined;

export function runAnalysisProblem<P extends AnalysisProblem>(problem: P, options?: RunAnalysisOptions): Promise<AnalysisResult<P>> {
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
