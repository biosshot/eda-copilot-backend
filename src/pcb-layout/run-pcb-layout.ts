import { autoPlacePcbWithReportAsync, renderPlacementSvg } from "#pcb-layout/pcb-auto-place/auto-place.ts";
import { runPcbLayoutDsl } from "#pcb-layout/pcb-layout-dsl/spec.ts";
import type { ExplainCircuit } from "#types/circuit.ts";
import type { ExistingPlacement, FootprintSpec } from "#types/pcb/layout-model.ts";
import { LayoutRulesSchema } from "#types/pcb/layout-rules.ts";
import { mkdir, writeFile } from "node:fs/promises";
import { createPlacementDebugArtifacts, writePlacementArtifacts } from "./artifacts.ts";
import { createBoardAssemble } from "./board-assemble.ts";
import { applyPlacementPreviewFilter, buildPlacementInput } from "./placement-input.ts";
import { validatePlacementRulesForCircuit } from "./placement-validation.ts";
import { buildPlacementGraph } from "./pcb-auto-place/placement-graph.ts";
import { createPcbLayoutDigest, createPcbToolReport } from "./report.ts";
import path from "node:path";
import { terminatePcbSubtreeWorkerPool } from "./pcb-auto-place-v2/tree-subtree-pool.ts";
import {
    attachLocalLayoutSeeds,
    extractLocalLayouts,
    instrumentLocalLayoutDsl,
    stripLocalLayoutCarriers,
    validateLocalLayouts,
} from "./pcb-auto-place-v2/local-layout.ts";
import {
    emitPcbLayoutProgress,
    pcbLayoutProgress,
    PCB_LAYOUT_PROGRESS,
    type PcbLayoutProgressReporter,
} from "./progress.ts";
import { applyExistingBoard, applyExistingComponentPlacements, ensurePreservedComponentBlocks, resolvePreservedComponentDesignators } from "./existing-placement.ts";

export type RunPcbLayoutOptions = {
    code: string;
    circuit: ExplainCircuit;
    existingPlacement?: ExistingPlacement;
    footprints?: Record<string, FootprintSpec>;
    outputDir?: string | null;
    onProgress?: PcbLayoutProgressReporter;
};

export async function runPcbLayout(options: RunPcbLayoutOptions) {
    const artifactDir = process.env.PCB_LAYOUT_DEBUG_DIR
        ? path.join(process.env.PCB_LAYOUT_DEBUG_DIR, 'placement')
        : options.outputDir;
    const timings: Record<string, number> = {};
    const runStarted = performance.now();
    let previousStage = runStarted;
    let previousName = 'parse_dsl';
    const stageDone = (nextName: string) => {
        if (!process.env.PCB_LAYOUT_DEBUG_DIR) return;
        const now = performance.now();
        timings[previousName] = now - previousStage;
        previousName = nextName;
        previousStage = now;
    };
    const emitProgress = (stage: Parameters<typeof pcbLayoutProgress>[0], progress: number, content: string) => {
        const event = pcbLayoutProgress(stage, progress, content);
        if (artifactDir) console.error(`[pcb-layout] ${event.progress}% ${event.stage}: ${event.content}`);
        emitPcbLayoutProgress(options.onProgress, event);
    };

    try {
        emitProgress('parse_dsl', PCB_LAYOUT_PROGRESS.parseDsl, 'Parsing PCB layout DSL.');
        const rawRules = runPcbLayoutDsl(instrumentLocalLayoutDsl(options.code));
        const localLayouts = extractLocalLayouts(rawRules);
        const parsedRules = ensurePreservedComponentBlocks(
            options.circuit,
            applyExistingBoard(
                LayoutRulesSchema().parse(stripLocalLayoutCarriers(rawRules)),
                options.existingPlacement,
            ),
            options.existingPlacement,
        );
        const { circuit, rules, preview } = applyPlacementPreviewFilter(options.circuit, parsedRules);
        stageDone('validate_dsl');

        emitProgress('validate_dsl', PCB_LAYOUT_PROGRESS.validateDsl, 'Validating PCB layout DSL against the circuit.');
        validatePlacementRulesForCircuit(circuit, rules);
        validateLocalLayouts(circuit, rules, localLayouts);
        stageDone('resolve_footprints');

        emitProgress('resolve_footprints', PCB_LAYOUT_PROGRESS.resolveFootprints, 'Resolving PCB footprints and component geometry.');
        const placementInput = attachLocalLayoutSeeds(
            applyExistingComponentPlacements(
                await buildPlacementInput(circuit, rules, options.footprints),
                rules.preserve,
                options.existingPlacement,
            ),
            localLayouts,
        );
        stageDone('build_graph');

        emitProgress('build_graph', PCB_LAYOUT_PROGRESS.buildGraph, 'Building placement ownership graph.');
        const placementGraph = buildPlacementGraph(placementInput);
        stageDone('auto_place');

        const { placements, report, layout, stages } = await autoPlacePcbWithReportAsync(placementInput, {
            onProgress: options.onProgress,
            logProgress: Boolean(artifactDir),
        });
        stageDone('render_and_artifacts');

        emitProgress('render', PCB_LAYOUT_PROGRESS.render, 'Rendering PCB placement preview and debug artifacts.');
        const placementSvg = renderPlacementSvg(placementInput, placements);
        const placementDebugArtifacts = createPlacementDebugArtifacts(placementInput, placements);
        const placementArtifacts = artifactDir
            ? writePlacementArtifacts(artifactDir, placementInput, placements, report, layout, stages, placementSvg, placementDebugArtifacts)
            : {};
        stageDone('assemble_and_report');

        emitProgress('assemble', PCB_LAYOUT_PROGRESS.assemble, 'Creating PCB board assembly payload.');
        const imageSvg = placementSvg;
        const boardAssemble = createBoardAssemble(layout, {
            preserveBoard: rules.preserve?.board === true,
            preservedComponents: resolvePreservedComponentDesignators(rules.preserve, options.existingPlacement),
        });
        const digest = createPcbLayoutDigest({
            placementInput,
            placementReport: report,
            layout,
            placementArtifacts,
        });
        const toolReport = createPcbToolReport({
            placementReport: report,
            placementInput,
            layout,
            preview,
        });

        if (artifactDir)
            await writeFile(path.join(artifactDir, 'board.assemble.json'), JSON.stringify(boardAssemble, null, 2)).catch(_ => _)
        if (artifactDir)
            await writeFile(path.join(artifactDir, 'placement.graph.json'), JSON.stringify(placementGraph, null, 2)).catch(_ => _)
        stageDone('done');
        if (process.env.PCB_LAYOUT_DEBUG_DIR) {
            const directory = process.env.PCB_LAYOUT_DEBUG_DIR;
            await mkdir(directory, { recursive: true });
            await mkdir(path.join(directory, 'placement'), { recursive: true });
            await writeFile(path.join(directory, 'placement', 'input.json'), JSON.stringify(placementInput));
            await writeFile(path.join(directory, 'stages.json'), JSON.stringify({
                version: 1, wallMs: performance.now() - runStarted, stagesMs: timings,
                placementOk: report.ok, components: placementInput.components.length,
            }, null, 2));
        }

        emitProgress('done', PCB_LAYOUT_PROGRESS.done, 'PCB layout finished.');

        return {
            rules,
            placementInput,
            placementGraph,
            placements,
            placementReport: report,
            placementStatus: report.ok ? "ok" as const : "failed_with_layout" as const,
            failedWithLayout: !report.ok,
            layout,
            stages,
            placementSvg,
            placementArtifacts,
            placementDebugArtifacts,
            boardAssemble,
            imageSvg,
            digest,
            toolReport,
            artifactsSaved: Boolean(artifactDir),
        };
    } finally {
        await terminatePcbSubtreeWorkerPool(false).catch(() => undefined);
    }
}

export type PcbLayoutRunResult = Awaited<ReturnType<typeof runPcbLayout>>;
