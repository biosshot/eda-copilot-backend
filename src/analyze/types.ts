import type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions } from '../../native/analyze/index.js';
import type { AnalysisConductionOptions, AnalysisConductionReport } from '../../native/analyze/index.js';
import type { AnalysisDcRequest, AnalysisDcReport } from '../../native/analyze/index.js';
import type { AnalysisElectricalRequest, AnalysisElectricalReport, AnalysisFrequencyRequest, AnalysisFrequencyReport } from '../../native/analyze/index.js';

export type { AnalysisImportReport, AnalysisGeometryReport, AnalysisGeometryOptions };
export type { AnalysisConductionOptions, AnalysisConductionReport };
export type AnalysisProblem = Readonly<{ kind: 'import-board'; boardJson: string }>
    | Readonly<{ kind: 'prepare-geometry'; boardJson: string; geometry?: Readonly<AnalysisGeometryOptions> }>
    | Readonly<{ kind: 'prepare-conduction'; boardJson: string } & AnalysisConductionOptions>
    | Readonly<{ kind: 'dc'; request: AnalysisDcRequest }>
    | Readonly<{ kind: 'electrical-network'; request: AnalysisElectricalRequest }>
    | Readonly<{ kind: 'frequency-network'; request: AnalysisFrequencyRequest }>;
export type AnalysisResult<P extends AnalysisProblem = AnalysisProblem> = P extends {kind:'frequency-network'} ? AnalysisFrequencyReport : P extends {kind:'electrical-network'} ? AnalysisElectricalReport : P extends { kind: 'dc' } ? AnalysisDcReport : P extends { kind: 'import-board' } ? AnalysisImportReport : P extends { kind: 'prepare-conduction' } ? AnalysisConductionReport : AnalysisGeometryReport;
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
    if (record(value) && value.kind === 'frequency-network') return keys(value,['kind','request']) && record(value.request) && Array.isArray(value.request.instances) && Array.isArray(value.request.frequencies) && finiteJson(value.request);
    if (record(value) && value.kind === 'electrical-network') return keys(value,['kind','request']) && record(value.request) && typeof value.request.contextId==='string' && ['dc','frequency','time'].includes(String(value.request.mode)) && record(value.request.circuit) && finiteJson(value.request);
    if (record(value) && value.kind === 'dc') return keys(value, ['kind','request']) && validDcRequest(value.request);
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

function finiteJson(v:unknown):boolean {return v===null||typeof v==='string'||typeof v==='boolean'||typeof v==='number'&&Number.isFinite(v)||Array.isArray(v)&&v.every(finiteJson)||record(v)&&Object.values(v).every(x=>x===undefined||finiteJson(x));}

function record(v: unknown): v is Record<string, unknown> { return !!v && typeof v === 'object' && !Array.isArray(v); }
function keys(v: Record<string, unknown>, names: string[]) { return Object.keys(v).every(k => names.includes(k)); }
function positive(v: unknown, max = 1e12): v is number { return typeof v === 'number' && Number.isFinite(v) && v > 0 && v <= max; }
function material(v: unknown): boolean { return record(v) && keys(v, ['conductivity_s_m', 'state']) && positive(v.conductivity_s_m) && typeof v.state === 'string' && !!v.state.trim(); }
function validDcRequest(v:unknown):boolean {
    if (!record(v) || !keys(v,['context_id','board_json','conductor','conduction','snapshot','terminals','elements','limits']) || typeof v.context_id!=='string' || !v.context_id.trim()
        || !(v.board_json===null || typeof v.board_json==='string') || (v.board_json===null)===(v.conductor===null)
        || v.snapshot!==undefined && typeof v.snapshot!=='boolean') return false;
    const conductor=v.conductor;
    if (conductor!==null && (!record(conductor) || !keys(conductor,['length_mm','width_mm','thickness_mm','conductivity_s_m','state'])
        || !['length_mm','width_mm','thickness_mm','conductivity_s_m'].every(k=>positive(conductor[k])) || typeof conductor.state!=='string' || !conductor.state.trim())) return false;
    if (!Array.isArray(v.terminals) || v.terminals.length>100_000 || !v.terminals.every(t=>record(t)&&keys(t,['id','pad_id'])&&typeof t.id==='string'&&!!t.id&&typeof t.pad_id==='string'&&!!t.pad_id)) return false;
    if (!Array.isArray(v.elements) || !v.elements.length || v.elements.length>100_000 || !v.elements.every(e=>record(e)&&keys(e,['id','kind','positive','negative','value','role'])
        && ['id','positive','negative'].every(k=>typeof e[k]==='string'&&!!e[k]) && ['voltage','current','resistance'].includes(String(e.kind)) && ['source','load'].includes(String(e.role))
        && typeof e.value==='number'&&Number.isFinite(e.value)&&(e.kind!=='resistance'||e.value>0))) return false;
    const c=v.conduction,l=v.limits;
    return (c===undefined || record(c)&&keys(c,['geometry','mesh','profile'])&&(c.geometry===undefined||validGeometryOptions(c.geometry))&&(c.mesh===undefined||validMesh(c.mesh))&&(c.profile===undefined||validProfile(c.profile)))
        && (l===undefined || record(l)&&keys(l,['max_memory_mib','max_unknowns','max_seconds'])&&positive(l.max_memory_mib,4096)&&l.max_memory_mib>=1&&positive(l.max_seconds)&&Number.isInteger(l.max_unknowns)&&Number(l.max_unknowns)>=2&&Number(l.max_unknowns)<=100_000);
}
function validMesh(v: unknown): boolean {
    return record(v) && Object.entries(v).every(([k, n]) => k === 'max_edge_mm' ? n === null || positive(n,1e6) && n >= 1e-6 : typeof n === 'number' && Number.isFinite(n) && (
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
