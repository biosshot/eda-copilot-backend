import type { AnalysisDslOutput } from '../../src/analyze/run-analysis-dsl.ts';
import { renderFrequencyReport as renderFrequencyHtml } from '../../src/analyze/render/frequency-html.ts';
import { frequencyCases } from './pcb-analysis-frequency-audit.ts';

export function renderFrequencyReport(name: string, metadata: unknown, result: AnalysisDslOutput) {
  return renderFrequencyHtml(name, metadata, result, frequencyCases(result));
}
