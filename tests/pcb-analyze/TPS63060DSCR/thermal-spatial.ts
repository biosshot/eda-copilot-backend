import {runSavedThermal} from '../../helpers/pcb-analysis-thermal.ts';
await runSavedThermal({script:new URL('./thermal-spatial.js',import.meta.url),expectations:new URL('./thermal-spatial.expectations.json',import.meta.url)});
