import {runSavedThermal} from '../../helpers/pcb-analysis-thermal.ts';
await runSavedThermal({script:new URL('./thermal-final-time-refined.js',import.meta.url),expectations:new URL('./thermal-final-time-refined.expectations.json',import.meta.url),timeoutMs:900000});
