import { createRequire } from 'node:module';
import workerpool from 'workerpool';
import { backendResource } from '../runtime/resources.ts';
import { isAnalysisProblem, type AnalysisResult } from './types.ts';
import { createProblemExecutor } from './execute-problem.ts';

const require = createRequire(import.meta.url);
const executeProblem = createProblemExecutor();

function importAnalysisInWorker(problem: unknown, jobId: string): AnalysisResult {
    if (!isAnalysisProblem(problem) || typeof jobId !== 'string') throw new TypeError('Invalid analysis worker request');
    if (problem.kind === 'dc'||problem.kind==='electrical-network'||problem.kind==='frequency-network'||problem.kind==='cross-section') { workerpool.workerEmit({ jobId, phase: 'started' }); return executeProblem(problem); }
    const addon = require(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    if (typeof addon.importAnalysisBoard !== 'function') throw new Error('Rebuild the shared addon: npm run native:build');
    if (problem.kind === 'prepare-geometry' && typeof addon.prepareAnalysisGeometry !== 'function') throw new Error('Rebuild the shared addon: npm run native:build');
    workerpool.workerEmit({ jobId, phase: 'started' });
    if (problem.kind === 'prepare-conduction') return addon.prepareAnalysisConduction(problem.boardJson, {
        ...(problem.geometry ? { geometry: problem.geometry } : {}),
        ...(problem.mesh ? { mesh: problem.mesh } : {}),
        ...(problem.profile ? { profile: problem.profile } : {}),
    });
    return problem.kind === 'prepare-geometry' ? addon.prepareAnalysisGeometry(problem.boardJson, problem.geometry) : addon.importAnalysisBoard(problem.boardJson);
}

workerpool.worker({ importAnalysisInWorker });
