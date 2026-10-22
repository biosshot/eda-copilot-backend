import { createRequire } from 'node:module';
import { backendResource } from '../runtime/resources.ts';
import type { AnalysisProblem, AnalysisResult } from './types.ts';

/** Used only inside the existing process workers. DSL calls native in its own
 * disposable process, so cancellation cannot orphan a nested native worker. */
export function createProblemExecutor(retainDcForScenario = false) {
  let session: import('../../native/analyze/index.js').AnalysisDcSession | undefined;
  return <P extends AnalysisProblem>(problem: P): AnalysisResult<P> => {
    const addon = createRequire(import.meta.url)(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    let result: AnalysisResult;
    if (problem.kind === 'dc') {
      session ??= new addon.AnalysisDcSession();
      try { result = session.solve(problem.request); }
      finally { if (!retainDcForScenario) { session.clear(); session=undefined; } }
    }
    else if (problem.kind === 'prepare-conduction') result = addon.prepareAnalysisConduction(problem.boardJson, { geometry: problem.geometry, mesh: problem.mesh, profile: problem.profile });
    else if (problem.kind === 'prepare-geometry') result = addon.prepareAnalysisGeometry(problem.boardJson, problem.geometry);
    else result = addon.importAnalysisBoard(problem.boardJson);
    return result as AnalysisResult<P>;
  };
}
