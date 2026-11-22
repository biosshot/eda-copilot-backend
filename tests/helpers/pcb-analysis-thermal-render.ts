import type { AnalysisDslOutput } from '../../src/analyze/run-analysis-dsl.ts';
import { thermalSvg as renderThermalSvg } from '../../src/analyze/render/thermal-html.ts';
import { thermalResults, type ThermalExpectations } from './pcb-analysis-thermal-audit.ts';
import { compareThermalPhysicalFields } from './thermal-field-comparison.ts';

export { thermalHtml } from '../../src/analyze/render/thermal-html.ts';
export function thermalSvg(result: AnalysisDslOutput, expectations: ThermalExpectations, metadata: unknown) {
  return renderThermalSvg(result, expectations, metadata, thermalResults(result), compareThermalPhysicalFields);
}
