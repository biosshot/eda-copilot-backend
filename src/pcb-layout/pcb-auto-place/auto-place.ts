import * as fp from '../f32.ts';
import { round } from './geometry.ts';
import type { Placement, PlacementGraphDiagnostic, PlacementStage, PlacementInput } from '#types/pcb/layout-model.ts';
import { PlacementError, PCB_PLACEMENT_ASSUMPTIONS } from '#types/pcb/layout-model.ts';
import { isFixedComponent } from './utils.ts';
import { DIAGNOSTIC_SCORE_SCALE } from './consts.ts';
import { createFixedPlacement } from './fixed.ts';
import { renderPlacementSvg } from './render.ts';
import { resolve } from 'node:path';
import { createPcbLayout } from './layout.ts';
import { createPlacementReport } from './placement-report.ts';
import { buildPlacementGraph } from './placement-graph.ts';
import { solvePlacementTreeBottomUp, solvePlacementTreeBottomUpAsync } from '../pcb-auto-place-v2/tree-solver.ts';
import { refinePostPlacement, refinePostPlacementAsync } from '../pcb-auto-place-v2/post-place-refiner.ts';
import { refineBoardAlignment } from '../pcb-auto-place-v2/board-alignment.ts';
import { emitPcbLayoutProgress, type PcbLayoutProgressReporter } from '../progress.ts';
export { PCB_PLACEMENT_ASSUMPTIONS, PlacementError } from '#types/pcb/layout-model.ts';
export { createPcbLayout } from './layout.ts';
export { createPlacementReport } from './placement-report.ts';
export { renderPlacementSubsetSvg, renderPlacementSvg } from './render.ts';
export type {
    BlockRole,
    BoardAnchor,
    BoardEdge,
    Box,
    CenteredRectBoard,
    ComponentRole,
    FootprintPad,
    FootprintSpec,
    HintPriority,
    Layer,
    PcbBlock,
    PcbComponent,
    PcbLayout,
    Placement,
    PlacementHint,
    PlacementInput,
    PlacementReport,
    PlacementStage,
    Point,
    TargetRef,
} from '#types/pcb/layout-model.ts';
export type { PcbRoutingRules } from '#types/pcb/routing-model.ts';

export function autoPlacePcb(input: PlacementInput): Placement[] {
    return autoPlacePcbInternal(input, false).placements;
}

function autoPlacePcbInternal(
    input: PlacementInput,
    collectStages: boolean,
    options: { allowInvalidGeometry?: boolean } = {},
): { placements: Placement[]; stages: PlacementStage[]; solverDiagnostics: PlacementGraphDiagnostic[] } {
    const stages: PlacementStage[] = [];
    const pushStage = (name: string, stagePlacements: Placement[], other?: unknown) => {
        if (!collectStages) return;
        stages.push({ name, data: other, placements: stagePlacements.map((placement) => ({ ...placement })) });
    };
    const graph = buildPlacementGraph(input);
    const tree = solvePlacementTreeBottomUp(input, graph, {
        grid: input.solverOptions.placementGridStep ?? 0.5,
        clearance: input.board.clearances.component,
    });
    const solverDiagnostics = treeDiagnosticsForReport(tree.diagnostics);
    pushStage('01-v2-tree', tree.root.placements, tree);

    const rawPlacements = input.components.map((component) => {
        const placement = tree.root.placements.find((item) => item.designator === component.designator);
        if (!placement) throw new Error(`Component ${component.designator} was not placed by v2 auto-place`);
        return placement;
    });
    const legalized = preserveFixedPlacements(input, rawPlacements);
    // const legalized = preserveFixedPlacements(input, legalizeGlobalPlacement(input, fixedPreserved));
    pushStage('02-v2-legalize', legalized);
    const refined = refinePostPlacement(input, legalized);
    solverDiagnostics.push(...refined.diagnostics);
    pushStage('03-v2-post-place', refined.placements, postPlaceStageData(refined));
    const aligned = refineBoardAlignment(input, tree.root.children, refined.placements);
    solverDiagnostics.push(...aligned.diagnostics.map(d => ({ ...d, code: 'v2_solver' })));
    pushStage('03b-v2-board-alignment', aligned.placements, aligned);
    const placements = normalizePlacementScores(aligned.placements);
    const report = createPlacementReport(input, placements, solverDiagnostics);
    if (hasHardGeometryViolations(report)) {
        pushStage('04-v2-invalid-final', placements, report);
        if (options.allowInvalidGeometry) return { placements, stages, solverDiagnostics };
        throw new PlacementError('PCB auto-placement v2 produced invalid geometry', report);
    }
    pushStage('04-v2-final', placements);
    return { placements, stages, solverDiagnostics };
}

async function autoPlacePcbInternalAsync(
    input: PlacementInput,
    collectStages: boolean,
    options: { allowInvalidGeometry?: boolean; onProgress?: PcbLayoutProgressReporter; logProgress?: boolean } = {},
): Promise<{ placements: Placement[]; stages: PlacementStage[]; solverDiagnostics: PlacementGraphDiagnostic[] }> {
    const stages: PlacementStage[] = [];
    const pushStage = (name: string, stagePlacements: Placement[], other?: unknown) => {
        if (!collectStages) return;
        stages.push({ name, data: other, placements: stagePlacements.map((placement) => ({ ...placement })) });
    };
    const graph = buildPlacementGraph(input);
    const tree = await solvePlacementTreeBottomUpAsync(input, graph, {
        grid: input.solverOptions.placementGridStep ?? 0.5,
        clearance: input.board.clearances.component,
        onProgress: options.onProgress,
        logProgress: options.logProgress,
    });
    const solverDiagnostics = treeDiagnosticsForReport(tree.diagnostics);
    pushStage('01-v2-tree', tree.root.placements, tree);

    const rawPlacements = input.components.map((component) => {
        const placement = tree.root.placements.find((item) => item.designator === component.designator);
        if (!placement) throw new Error(`Component ${component.designator} was not placed by v2 auto-place`);
        return placement;
    });
    const legalized = preserveFixedPlacements(input, rawPlacements);
    pushStage('02-v2-legalize', legalized);
    const refined = await refinePostPlacementAsync(input, legalized, (content) => {
        emitPcbLayoutProgress(options.onProgress, { stage: 'solve_board', progress: 90, content });
        if (options.logProgress) console.error(`[pcb-post-place] ${content}`);
    });
    solverDiagnostics.push(...refined.diagnostics);
    pushStage('03-v2-post-place', refined.placements, postPlaceStageData(refined));
    const aligned = refineBoardAlignment(input, tree.root.children, refined.placements);
    solverDiagnostics.push(...aligned.diagnostics.map(d => ({ ...d, code: 'v2_solver' })));
    pushStage('03b-v2-board-alignment', aligned.placements, aligned);
    const placements = normalizePlacementScores(aligned.placements);
    const report = createPlacementReport(input, placements, solverDiagnostics);
    if (hasHardGeometryViolations(report)) {
        pushStage('04-v2-invalid-final', placements, report);
        if (options.allowInvalidGeometry) return { placements, stages, solverDiagnostics };
        throw new PlacementError('PCB auto-placement v2 produced invalid geometry', report);
    }
    pushStage('04-v2-final', placements);
    return { placements, stages, solverDiagnostics };
}

function hasHardGeometryViolations(report: ReturnType<typeof createPlacementReport>) {
    return report.outsideBoard.length > 0
        || report.overlaps.length > 0
        || report.boardHoleViolations.length > 0
        || report.constraintRegionViolations.length > 0
        || report.layerViolations.length > 0
        || report.unplaced.length > 0;
}

function preserveFixedPlacements(input: PlacementInput, placements: Placement[]) {
    const fixedPlacements = new Map(input.components.flatMap((component) => {
        const fixedPlacement = createFixedPlacement(input, component);
        return fixedPlacement ? [[component.designator, fixedPlacement] as const] : [];
    }));
    if (fixedPlacements.size === 0) return placements;

    return placements.map((placement) => fixedPlacements.get(placement.designator) ?? placement);
}

function normalizePlacementScores(placements: Placement[]) {
    return placements.map((placement) => ({
        ...placement,
        score: diagnosticScore(placement.score),
    }));
}

function postPlaceStageData(result: ReturnType<typeof refinePostPlacement>) {
    return {
        scoreBefore: result.scoreBefore,
        scoreAfter: result.scoreAfter,
        moves: result.moves,
        profile: result.profile,
        diagnostics: result.diagnostics,
    };
}

function diagnosticScore(score: number) {
    if (score <= 0) return 0;
    return round(fp.div(score, (fp.add(1, fp.div(score, DIAGNOSTIC_SCORE_SCALE)))));
}

export function autoPlacePcbWithReport(input: PlacementInput) {
    const { placements, stages, solverDiagnostics } = autoPlacePcbInternal(input, true, { allowInvalidGeometry: true });
    const report = createPlacementReport(input, placements, solverDiagnostics);
    const layout = createPcbLayout(input, placements);
    return { placements, report, layout, stages };
}

export async function autoPlacePcbWithReportAsync(
    input: PlacementInput,
    options: { onProgress?: PcbLayoutProgressReporter; logProgress?: boolean } = {},
) {
    const { placements, stages, solverDiagnostics } = await autoPlacePcbInternalAsync(input, true, {
        allowInvalidGeometry: true,
        onProgress: options.onProgress,
        logProgress: options.logProgress,
    });
    const report = createPlacementReport(input, placements, solverDiagnostics);
    const layout = createPcbLayout(input, placements);
    return { placements, report, layout, stages };
}

function treeDiagnosticsForReport(diagnostics: Array<{ severity: 'warning' | 'error'; nodeId: string; message: string }>): PlacementGraphDiagnostic[] {
    return diagnostics.flatMap((diagnostic) => {
        if (diagnostic.message.startsWith('Dissolved sparse module ')) {
            return [{
                severity: diagnostic.severity,
                code: 'module_dissolved',
                message: diagnostic.message,
                nodeId: diagnostic.nodeId,
            }];
        }
        if (['Deferred ', 'Block portfolio', 'Block postrefine', 'Block checkpoint', 'Role hypothesis', 'Staged block search',
            'Independent placement', 'Suspicious placement role', 'Board soft spacing', 'Board search alignment',
            'Board alignment proposal', 'Board packaging'].some(prefix=>diagnostic.message.startsWith(prefix))) {
            return [{
                severity: diagnostic.severity,
                code: 'v2_solver',
                message: diagnostic.message,
                nodeId: diagnostic.nodeId,
            }];
        }
        return [];
    });
}
