import type { Placement, PlacementGraphDiagnostic } from '#types/pcb/layout-model.ts';

export interface PostPlaceMove {
    kind: 'rotate_180' | 'swap';
    designators: string[];
    description: string;
    scoreBefore: number;
    scoreAfter: number;
    routePenaltyBefore: number;
    routePenaltyAfter: number;
    effectiveImprovement: number;
    routeJobCount: number;
    routeUnresolvedBefore: number;
    routeUnresolvedAfter: number;
    routeBudgetExhaustedBefore: number;
    routeBudgetExhaustedAfter: number;
}

export interface PostPlaceRefineResult {
    placements: Placement[];
    diagnostics: PlacementGraphDiagnostic[];
    moves: PostPlaceMove[];
    scoreBefore: number;
    scoreAfter: number;
    profile: PostPlaceProfile;
}

type BatchProfile = {
    candidates: number; hardRejected: number; boundRejected: number; feasibilityRejected: number;
    insufficientImprovement: number; baselineEvaluations: number; baselineCacheHits: number; routeEvaluations: number;
    routeEncodingMs: number; routeNativeMs: number; scoreEncodingMs: number; scoreNativeMs: number;
    geometryMs: number; globalScoreMs: number; baselineMs: number; routeMs: number;
};

export type PostPlaceProfile = {
    encodingMs?: number;
    componentCount?: number; pinCount?: number; adaptiveIterationLimit?: number; requestedIterations?: number;
    iterationLimit?: number; timeoutMs?: number; timedOut?: boolean;
    stopReason?: 'disabled' | 'iteration_limit' | 'no_improvement' | 'timeout';
    workers: number; initialScoreMs: number; fixedDiagnosticsMs: number; totalMs: number;
    iterations: Array<BatchProfile & { generationMs: number; evaluationWallMs: number; accepted: boolean; generatedCandidates?: number; timedOut?: boolean }>;
};
