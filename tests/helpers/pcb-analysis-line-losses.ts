import {runSavedCrossSection} from './pcb-analysis-cross-section.ts';
import {auditLineLosses} from './pcb-analysis-line-losses-audit.ts';
import {renderLineLosses} from './pcb-analysis-line-losses-render.ts';
export const runSavedLineLosses=(input:{script:URL;expectations:URL})=>runSavedCrossSection({...input,name:'impedance-losses',audit:auditLineLosses,render:renderLineLosses,evidenceFiles:['tests/helpers/pcb-analysis-line-losses.ts','tests/helpers/pcb-analysis-line-losses-audit.ts','tests/helpers/pcb-analysis-line-losses-render.ts','tests/fixtures/analysis-line-losses/references.json']});
