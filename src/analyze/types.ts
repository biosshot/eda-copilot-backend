import type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions } from '../../native/analyze/index.js';

export type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions };
export type AnalysisProblem = Readonly<{ kind: 'import-board'; boardJson: string }>
    | Readonly<{ kind: 'prepare-geometry'; boardJson: string; geometry?: Readonly<AnalysisGeometryOptions> }>;
export type AnalysisResult<P extends AnalysisProblem = AnalysisProblem> = P extends { kind: 'import-board' } ? AnalysisImportReport : AnalysisGeometryReport;
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
    if (value === null || typeof value !== 'object' || !('boardJson' in value) || typeof value.boardJson !== 'string' || !('kind' in value)) return false;
    if (value.kind === 'import-board') return Object.keys(value).length === 2;
    if (value.kind !== 'prepare-geometry' || !Object.keys(value).every(k => ['kind', 'boardJson', 'geometry'].includes(k))) return false;
    return !('geometry' in value) || value.geometry === undefined || validGeometryOptions(value.geometry);
}

function validGeometryOptions(value: unknown): boolean {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
    return Object.entries(value).every(([key, v]) => typeof v === 'number' && Number.isFinite(v)
        && (key === 'max_deviation_mm' ? v >= 0.000001 && v <= 0.01
            : key === 'max_vertices_per_curve' ? Number.isInteger(v) && v >= 8 && v <= 65536
                : key === 'max_generated_vertices' && Number.isInteger(v) && v >= 8 && v <= 10_000_000));
}
