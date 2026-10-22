import { runSavedAnalysisDc } from '../../helpers/pcb-analysis-dc.ts';

await runSavedAnalysisDc({
  script: new URL('./dc.js', import.meta.url),
  expectations: new URL('./dc.expectations.json', import.meta.url),
  runner: { timeoutMs: 540_000, maxOutputBytes: 128 * 1024 * 1024 },
});
