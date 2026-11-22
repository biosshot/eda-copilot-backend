import { readFile, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { runAnalysisProblem, terminateAnalysisPool } from '../../src/analyze/run-analysis.ts';

export async function importSavedAnalysisBoard() {
    try {
        const fixtureDir = process.env.PCB_ANALYZE_FIXTURE_DIR;
        const runDir = process.env.PCB_ANALYZE_RUN_DIR;
        if (!fixtureDir || !runDir) throw new Error('Run with npm run test:pcb-analyze -- <fixture> --name import');
        const boardJson = await readFile(join(fixtureDir, `${basename(fixtureDir)}.json`), 'utf8');
        const report = await runAnalysisProblem({ kind: 'import-board', boardJson });
        await writeFile(join(runDir, 'report.json'), JSON.stringify(report, null, 2) + '\n');
        console.log(JSON.stringify({ imported: report.imported, accepted: report.accepted, diagnostics: report.diagnostics.length }));
        if (!report.imported) process.exitCode = 1;
    } catch (error) {
        console.error(error);
        process.exitCode = 1;
    } finally { await terminateAnalysisPool(); }
}
