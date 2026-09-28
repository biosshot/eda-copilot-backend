import type { PcbComponent } from '#types/pcb/layout-model.ts';
import { isGroundSignalName, isPowerSignalName } from '#utils/signals.ts';
import type { BlockSolveParams } from './block-solver.ts';

export interface RoleHypothesis { designator: string; from: string; to: PcbComponent['pcb']['role']; reason: string }

/** Conservative search seeds, not edits to the input or inferred circuit truth.
 * Extend by component type when a concrete contradiction has been observed. */
export function suspiciousBlockRoles(params: BlockSolveParams): RoleHypothesis[] {
    // Parallel capacitors on the same two nets support the declared decoupling
    // role even when a supply net has an unfamiliar name (for example I_OUT).
    const capPairs = new Map<string, number>();
    for (const primitive of params.primitives) {
        if (primitive.placements.length !== 1) continue;
        const c = params.options.componentByDesignator?.get(primitive.placements[0].designator);
        if (c?.pcb.role !== 'decoupling_cap') continue;
        const nets = c.pins.map(pin => pin.signal_name);
        if (nets.length !== 2 || nets.some(n => !n)) continue;
        const key = [...nets].sort().join('\u0000');
        capPairs.set(key, (capPairs.get(key) ?? 0) + 1);
    }
    return params.primitives.flatMap(p => {
        if (p.locked || p.placements.length !== 1) return [];
        const c = params.options.componentByDesignator?.get(p.placements[0].designator);
        if (!c || c.pcb.role !== 'decoupling_cap') return [];
        const nets = c.pins.map(pin => pin.signal_name);
        if (nets.length !== 2 || nets.some(n => !n || isGroundSignalName(n) || isPowerSignalName(n))) return [];
        if ((capPairs.get([...nets].sort().join('\u0000')) ?? 0) > 1) return [];
        return [{ designator: c.designator, from: c.pcb.role, to: 'passive' as const,
            reason: 'decoupling_cap has two connected signal pins, no recognised supply/ground pin, and no parallel capacitor on the same two nets' }];
    });
}

export function withRoleHypotheses(params: BlockSolveParams, hypotheses: RoleHypothesis[]): BlockSolveParams {
    const components = new Map(params.options.componentByDesignator);
    for (const h of hypotheses) {
        const c = components.get(h.designator)!;
        components.set(h.designator, { ...c, pcb: { ...c.pcb, role: h.to } });
    }
    // Clearance and explicit constraints are invariant across role hypotheses.
    return { ...params, options: { ...params.options, componentByDesignator: components } };
}
