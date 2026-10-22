import type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions } from '../../native/analyze/index.js';
import type { AnalysisConductionOptions, AnalysisConductionReport } from '../../native/analyze/index.js';

export type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions };
export type { AnalysisConductionOptions, AnalysisConductionReport };
export type AnalysisProblem = Readonly<{ kind: 'import-board'; boardJson: string }>
    | Readonly<{ kind: 'prepare-geometry'; boardJson: string; geometry?: Readonly<AnalysisGeometryOptions> }>
    | Readonly<{ kind: 'prepare-conduction'; boardJson: string } & AnalysisConductionOptions>;
export type AnalysisResult<P extends AnalysisProblem = AnalysisProblem> = P extends { kind: 'import-board' } ? AnalysisImportReport : P extends { kind: 'prepare-conduction' } ? AnalysisConductionReport : AnalysisGeometryReport;
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
    if (value.kind === 'prepare-conduction') {
        if (!Object.keys(value).every(k => ['kind', 'boardJson', 'geometry', 'mesh', 'profile'].includes(k))) return false;
        return (!('geometry' in value) || value.geometry === undefined || validGeometryOptions(value.geometry))
            && (!('mesh' in value) || value.mesh === undefined || validMesh(value.mesh))
            && (!('profile' in value) || value.profile === undefined || validProfile(value.profile));
    }
    if (value.kind !== 'prepare-geometry' || !Object.keys(value).every(k => ['kind', 'boardJson', 'geometry'].includes(k))) return false;
    return !('geometry' in value) || value.geometry === undefined || validGeometryOptions(value.geometry);
}

function record(v: unknown): v is Record<string, unknown> { return !!v && typeof v === 'object' && !Array.isArray(v); }
function keys(v: Record<string, unknown>, names: string[]) { return Object.keys(v).every(k => names.includes(k)); }
function positive(v: unknown, max = 1e12): v is number { return typeof v === 'number' && Number.isFinite(v) && v > 0 && v <= max; }
function material(v: unknown): boolean { return record(v) && keys(v, ['conductivity_s_m', 'state']) && positive(v.conductivity_s_m) && typeof v.state === 'string' && !!v.state.trim(); }
function validMesh(v: unknown): boolean {
    return record(v) && Object.entries(v).every(([k, n]) => typeof n === 'number' && Number.isFinite(n) && (
        k === 'max_area_mm2' ? n >= 1e-8 && n <= 1e6 : k === 'min_angle_deg' ? n >= 0 && n <= 30
            : Number.isInteger(n) && (k === 'max_nodes' ? n >= 3 && n <= 2_000_000 : k === 'max_triangles' ? n >= 1 && n <= 4_000_000 : k === 'max_refinement_steps' && n >= 0 && n <= 64)));
}
function validProfile(v: unknown): boolean {
    if (!record(v) || !keys(v, ['stackup', 'materials', 'barrel'])) return false;
    if (v.stackup !== undefined && (!Array.isArray(v.stackup) || !v.stackup.every(e => record(e) && positive(e.thickness_mm, 100)
        && (e.kind === 'copper' ? keys(e, ['kind', 'layer', 'thickness_mm']) && typeof e.layer === 'string'
            : e.kind === 'dielectric' && keys(e, ['kind', 'name', 'thickness_mm']) && typeof e.name === 'string')))) return false;
    if (v.materials !== undefined && (!record(v.materials) || !Object.values(v.materials).every(material))) return false;
    const b = v.barrel;
    return b === undefined || record(b) && keys(b, ['plating_thickness_mm', 'material', 'diameter_convention'])
        && (b.plating_thickness_mm === undefined || positive(b.plating_thickness_mm, 10))
        && (b.material === undefined || material(b.material))
        && (b.diameter_convention === undefined || b.diameter_convention === 'finished_hole' || b.diameter_convention === 'before_plating');
}

function validGeometryOptions(value: unknown): boolean {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
    return Object.entries(value).every(([key, v]) => typeof v === 'number' && Number.isFinite(v)
        && (key === 'max_deviation_mm' ? v >= 0.000001 && v <= 0.01
            : key === 'max_vertices_per_curve' ? Number.isInteger(v) && v >= 8 && v <= 65536
                : key === 'max_generated_vertices' && Number.isInteger(v) && v >= 8 && v <= 10_000_000));
}
