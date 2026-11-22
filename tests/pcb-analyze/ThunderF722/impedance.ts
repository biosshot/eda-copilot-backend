import {runSavedCrossSection} from '../../helpers/pcb-analysis-cross-section.ts';
await runSavedCrossSection({script:new URL('./impedance.js',import.meta.url),expectations:new URL('./impedance.expectations.json',import.meta.url)});
