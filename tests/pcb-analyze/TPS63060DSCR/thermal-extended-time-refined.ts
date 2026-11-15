import {runExtendedThermal} from '../../../debugging/thermal-verification/extended-convergence/run-extended.mjs';
await runExtendedThermal({script:new URL('./thermal-extended-time-refined.js',import.meta.url),expectations:new URL('./thermal-extended-time-refined.expectations.json',import.meta.url),timeoutMs:2100000});
