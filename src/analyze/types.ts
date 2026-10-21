import type { AnalysisImportReport } from '../../native/analyze/index.js';

export type { AnalysisImportReport };
export type AnalysisProblem = Readonly<{ kind: 'import-board'; boardJson: string }>;
export type AnalysisProgress = {
    jobId: string;
    phase: 'queued' | 'started' | 'completed' | 'failed' | 'cancelled';
};
export type RunAnalysisOptions = {
    signal?: AbortSignal;
    timeoutMs?: number;
    onProgress?: (event: AnalysisProgress) => void | Promise<void>;
};
export type AnalysisExecutionCode = 'ABORTED' | 'TIMEOUT' | 'QUEUE_FULL' | 'WORKER_FAILED' | 'DISPOSED' | 'INVALID_REQUEST';
export class AnalysisExecutionError extends Error {
    constructor(readonly code: AnalysisExecutionCode, message: string, options?: ErrorOptions) {
        super(message, options);
        this.name = 'AnalysisExecutionError';
    }
}

// Validate both the parent boundary and the worker boundary before native calls.
export function isAnalysisProblem(value: unknown): value is AnalysisProblem {
    return value !== null && typeof value === 'object'
        && Object.keys(value).length === 2
        && 'kind' in value && value.kind === 'import-board'
        && 'boardJson' in value && typeof value.boardJson === 'string';
}
