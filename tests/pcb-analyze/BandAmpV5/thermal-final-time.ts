import {runSavedThermal} from '../../helpers/pcb-analysis-thermal.ts';
await runSavedThermal({script:new URL('./thermal-final-time.js',import.meta.url),expectations:new URL('./thermal-final-time.expectations.json',import.meta.url),timeoutMs:900000});
