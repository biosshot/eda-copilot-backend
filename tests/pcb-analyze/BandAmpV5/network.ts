import {runSavedFrequencyNetwork} from '../../helpers/pcb-analysis-frequency.ts';
await runSavedFrequencyNetwork({script:new URL('./network.js',import.meta.url),expectations:new URL('./network.expectations.json',import.meta.url)});
