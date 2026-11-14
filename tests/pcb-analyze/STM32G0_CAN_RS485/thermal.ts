import {runSavedThermal} from '../../helpers/pcb-analysis-thermal.ts';
await runSavedThermal({timeoutMs:1200000,script:new URL('./thermal.js',import.meta.url),expectations:new URL('./thermal.expectations.json',import.meta.url)});
