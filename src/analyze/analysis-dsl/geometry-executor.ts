import {randomUUID} from 'node:crypto';
import type {AnalysisTask, ExecutionResult} from './executor.ts';
import type {DataDescription, Validity} from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {AnalysisDslError} from './schema.ts';
import {json} from './data.ts';

/** Only immutable DSL/native adaptation. All spatial/physical work is Rust. */
export function createGeometryExecutor() {
  const execute = createProblemExecutor();
  return async (task: AnalysisTask<'geometry'>): Promise<ExecutionResult> => {
    const c = task.config;
    if (!task.board) throw new AnalysisDslError('GEOMETRY_BOARD_REQUIRED', 'Geometry validation needs an explicit board');
    if (task.variantChanges.length || c.numerics) throw new AnalysisDslError('GEOMETRY_UNSUPPORTED', 'Geometry variants/custom numerics are unsupported; no implicit refill or threshold relaxation');
    const report = execute({kind: 'geometry-validation', request: {
      board_json: JSON.stringify(task.board), rules: c.rules ?? [], scope: c.scope,
      budget: c.budget,
    }});
    const diagnostics = report.diagnostics.map(({code,message,severity,path}) => ({code,message,severity,...(path ? {path} : {})}));
    if (report.status === 'rejected') throw new AnalysisDslError(report.diagnostics[0]?.code ?? 'GEOMETRY_REJECTED', diagnostics.map(d => d.message).join('; '), diagnostics);
    const validity: Validity = report.status === 'complete' ? {status: 'valid'} : {status: 'conditional', reason: 'Prepared snapshot and coverage limitations remain explicit; see coverage and diagnostics'};
    const count: DataDescription = {kind: 'scalar', unit: '1', value: report.violations.length,
      validity: report.status === 'complete' ? {status: 'valid'} : {status: 'unavailable', reason: 'Requested geometry coverage is incomplete; confirmed findings are retained separately'}, diagnostics};
    return {id: `${task.contextId}:geometry-${randomUUID()}`, status: report.status, method: report.method,
      diagnostics,
      assumptions: ['Captured physical copper; no CAD refill/repair', 'Connectivity opens require unavailable terminal-group intent', 'Same-net clearance exemption does not create electrical connectivity'],
      metrics: {violationCount: count, measurements: {kind: 'table', unit: 'mm', validity, diagnostics,
        data: json({rows: report.measurements, coverage: report.coverage, confirmedReportCount: report.violations.length, uncertainReportCount: report.uncertain_count, confirmedCountMeaning: 'Count of retained confirmed reports, not a complete defect count'})}},
      violations: json(report.violations) as readonly import('./spec-doc.js').Json[],
      solution: json({...report, resources: {peakRssBytes: process.resourceUsage().maxRSS * 1024, metric: 'OS process high-water RSS including Node and earlier native work'}}),
    };
  };
}
