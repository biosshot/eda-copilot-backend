import {runSavedThermal} from '../../helpers/pcb-analysis-thermal.ts';
await runSavedThermal({script:new URL('./thermal.js',import.meta.url),expectations:new URL('./thermal.expectations.json',import.meta.url)});
