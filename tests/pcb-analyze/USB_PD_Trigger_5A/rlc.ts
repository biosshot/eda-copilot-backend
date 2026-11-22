import{runSavedAnalysisRlc}from'../../helpers/pcb-analysis-rlc.ts';
await runSavedAnalysisRlc({script:new URL('./rlc.js',import.meta.url),expectations:new URL('./rlc.expectations.json',import.meta.url)});
