import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
import { createProblemExecutor } from './execute-problem.ts';
import { freeze } from './analysis-dsl/data.ts';
let captured: {
    boardJson: string;
    report: AnalysisGeometryReport;
} | undefined;
/** One bounded immutable JS snapshot shared by DSL planning and all refinements.
 * Native section/volume sessions still own their separate internal preparations. */
export function capturedBoardGeometry(boardJson: string): AnalysisGeometryReport {
    if (captured?.boardJson === boardJson)
        return captured.report;
    const report = freeze(createProblemExecutor()({ kind: 'prepare-geometry', boardJson }));
    captured = { boardJson, report };
    return report;
}
