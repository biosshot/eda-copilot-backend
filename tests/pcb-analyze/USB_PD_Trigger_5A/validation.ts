import {runSavedValidation} from '../../helpers/pcb-analysis-validation.ts';
await runSavedValidation({script:new URL('./validation.js',import.meta.url),expectations:new URL('./validation.expectations.json',import.meta.url)});
