import { createRequire } from 'node:module';
import workerpool from 'workerpool';
import { backendResource } from '../runtime/resources.ts';
import { isAnalysisProblem, type AnalysisResult } from './types.ts';

const require = createRequire(import.meta.url);

function importAnalysisInWorker(problem: unknown, jobId: string): AnalysisResult {
    if (!isAnalysisProblem(problem) || typeof jobId !== 'string') throw new TypeError('Invalid analysis worker request');
    const addon = require(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    if (typeof addon.importAnalysisBoard !== 'function') throw new Error('Rebuild the shared addon: npm run native:build');
    if (problem.kind === 'prepare-geometry' && typeof addon.prepareAnalysisGeometry !== 'function') throw new Error('Rebuild the shared addon: npm run native:build');
    workerpool.workerEmit({ jobId, phase: 'started' });
    return problem.kind === 'prepare-geometry' ? addon.prepareAnalysisGeometry(problem.boardJson, problem.geometry) : addon.importAnalysisBoard(problem.boardJson);
}

workerpool.worker({ importAnalysisInWorker });
