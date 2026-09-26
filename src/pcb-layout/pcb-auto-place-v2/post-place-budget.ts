import type { PlacementInput } from '#types/pcb/layout-model.ts';

export const POST_PLACE_TIMEOUT_MS = 30_000;

/** Workload proxy, not physical occupancy: fixed parts/pads also affect routing. */
export function postPlaceBudget(input: PlacementInput) {
    const componentCount = input.components.length;
    const pinCount = input.components.reduce((sum, c) => sum + c.footprint.pads.length, 0);
    const tiers = [
        { components: 50, pins: 250, iterations: 16 },
        { components: 100, pins: 500, iterations: 12 },
        { components: 150, pins: 1000, iterations: 8 },
        { components: 250, pins: 1500, iterations: 5 },
    ];
    const adaptiveIterationLimit = tiers.find(t => componentCount <= t.components && pinCount <= t.pins)?.iterations ?? 3;
    const requestedIterations = Math.max(0, Math.floor(input.solverOptions.localImproveIterations));
    return { componentCount, pinCount, adaptiveIterationLimit, requestedIterations,
        iterations: Math.min(requestedIterations, adaptiveIterationLimit), timeoutMs: POST_PLACE_TIMEOUT_MS };
}
