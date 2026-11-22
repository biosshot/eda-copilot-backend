import { z } from 'zod';
import type { EdaApiOptions } from './devices/eda-api.ts';
import { ExplainCircuitStruct } from './types/circuit.ts';
import { ExistingPlacementSchema, FootprintSpecSchema, PlacementError } from './types/pcb/layout-model.ts';
import { formatComponentSizeReport, getComponentSizeReport } from './pcb-layout/component-size-report.ts';
import { svgToDataUrl } from './pcb-layout/artifacts.ts';
import { createPcbDslErrorReport, createPcbToolReport, formatPcbToolReportForMessage, formatPlacementError } from './pcb-layout/report.ts';
import { runPcbLayoutQueued, terminatePcbLayoutWorkerPool, type RunPcbLayoutQueuedOptions } from './pcb-layout/run-pcb-layout-pool.ts';
import { terminateAnalysisPool } from './analyze/run-analysis.ts';

export const layoutSchema = z.object({
  code: z.string().min(1),
  circuit: ExplainCircuitStruct(),
  existingPlacement: ExistingPlacementSchema().optional(),
  footprints: z.record(z.string(), FootprintSpecSchema()).optional(),
});
const sizesSchema = z.object({
  circuit: ExplainCircuitStruct(),
  footprints: z.record(z.string(), FootprintSpecSchema()).optional(),
  designators: z.array(z.string()).nullish(),
  includeAll: z.boolean().nullish(),
});

export type MakePcbLayoutInput = z.input<typeof layoutSchema>;
export type GetPcbComponentSizesInput = z.input<typeof sizesSchema>;

export async function getPcbComponentSizes(input: GetPcbComponentSizesInput, options: EdaApiOptions = {}) {
  const data = sizesSchema.parse(input);
  try {
    const report = await getComponentSizeReport(data.circuit, data, data.footprints, options.edaEdition);
    return { content: formatComponentSizeReport(report), report };
  } catch (error) {
    return { error: `Error: ${(error as Error).message}` };
  }
}

export async function makePcbLayout(input: MakePcbLayoutInput, options: RunPcbLayoutQueuedOptions = {}) {
  options.signal?.throwIfAborted();
  const data = layoutSchema.parse(input);
  try {
    const run = await runPcbLayoutQueued({ ...data, edaEdition: options.edaEdition }, options);
    options.signal?.throwIfAborted();
    return {
      content: formatPcbToolReportForMessage(run.toolReport),
      toolReport: run.toolReport,
      pcb: run.boardAssemble,
      blockPreview: run.blockPreview,
      preview_image_url: svgToDataUrl(run.imageSvg),
      placement_debug_artifacts: {
        items: run.placementDebugArtifacts.items.map(item => ({
          type: item.type, name: item.name, fileName: item.fileName,
          components: item.components, svg_url: svgToDataUrl(item.svg),
        })),
      },
    };
  } catch (error) {
    options.signal?.throwIfAborted();
    if (error instanceof PlacementError) {
      const toolReport = createPcbToolReport({ placementReport: error.report });
      return { content: formatPcbToolReportForMessage(toolReport) + '\n\n' + formatPlacementError(error), toolReport };
    }
    const toolReport = createPcbDslErrorReport(error as Error);
    return { content: formatPcbToolReportForMessage(toolReport), toolReport };
  }
}

export async function disposeBackend() {
  const results = await Promise.allSettled([terminatePcbLayoutWorkerPool(true), terminateAnalysisPool()]);
  const errors = results.flatMap(result => result.status === 'rejected' ? [result.reason] : []);
  if (errors.length) throw new AggregateError(errors, 'Backend worker shutdown failed');
}
export { validatePcbLayoutIntent } from './pcb-layout/intent-validation/validate.ts';
export { pcbLayoutDslSkeleton } from './pcb-layout/intent-validation/schematic-declarations.ts';
export { detectCircuitPatterns } from './pcb-layout/intent-validation/recognition/detect.ts';
export type { ValidatePcbLayoutIntentInput, PcbIntentDiagnostic, PcbSchematicGroups, PcbSchematicBlockDeclarations } from './pcb-layout/intent-validation/types.ts';
export type { CircuitPatternMatch, CircuitPatternRequirement, CircuitPinRef, DetectCircuitPatternsInput } from './pcb-layout/intent-validation/recognition/types.ts';
export type { PcbLayoutProgress, PcbLayoutProgressReporter } from './pcb-layout/progress.ts';
export type { RunPcbLayoutQueuedOptions as PcbLayoutOptions } from './pcb-layout/run-pcb-layout-pool.ts';
