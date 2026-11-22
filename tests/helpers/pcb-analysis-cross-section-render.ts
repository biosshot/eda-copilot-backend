import { renderCrossSection as renderSectionHtml } from '../../src/analyze/render/cross-section-html.ts';
import { sectionResults } from './pcb-analysis-cross-section-audit.ts';

export function renderCrossSection(name: string, board: unknown, expectations: unknown, metadata: unknown, result: unknown) {
  return renderSectionHtml(name, board, expectations, metadata, result, sectionResults(result));
}
