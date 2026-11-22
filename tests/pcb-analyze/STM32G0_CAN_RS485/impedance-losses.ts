import {runSavedLineLosses} from '../../helpers/pcb-analysis-line-losses.ts';
await runSavedLineLosses({script:new URL('./impedance-losses.js',import.meta.url),expectations:new URL('./impedance-losses.expectations.json',import.meta.url)});
