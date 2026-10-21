import { createRequire } from 'node:module';
import workerpool from 'workerpool';
import { backendResource } from '../runtime/resources.ts';
import { isAnalysisProblem, type AnalysisImportReport } from './types.ts';

const require = createRequire(import.meta.url);

function importAnalysisInWorker(problem: unknown, jobId: string): AnalysisImportReport {
    if (!isAnalysisProblem(problem) || typeof jobId !== 'string') throw new TypeError('Invalid analysis worker request');
    const addon = require(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    if (typeof addon.importAnalysisBoard !== 'function') throw new Error('Rebuild the shared addon: npm run native:build');
    workerpool.workerEmit({ jobId, phase: 'started' });
    return addon.importAnalysisBoard(problem.boardJson);
}

workerpool.worker({ importAnalysisInWorker });
