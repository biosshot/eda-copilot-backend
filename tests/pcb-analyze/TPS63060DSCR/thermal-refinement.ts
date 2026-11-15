import {runSavedThermal} from '../../helpers/pcb-analysis-thermal.ts';
await runSavedThermal({script:new URL('./thermal-refinement.js',import.meta.url),expectations:new URL('./thermal-refinement.expectations.json',import.meta.url)});
