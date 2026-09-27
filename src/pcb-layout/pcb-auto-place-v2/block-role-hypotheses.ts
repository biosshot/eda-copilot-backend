import type { PcbComponent } from '#types/pcb/layout-model.ts';
import { isGroundSignalName, isPowerSignalName } from '#utils/signals.ts';
import type { BlockSolveParams } from './block-solver.ts';

export interface RoleHypothesis { designator: string; from: string; to: PcbComponent['pcb']['role']; reason: string }

/** Conservative search seeds, not edits to the input or inferred circuit truth.
 * Extend by component type when a concrete contradiction has been observed. */
export function suspiciousBlockRoles(params: BlockSolveParams): RoleHypothesis[] {
    return params.primitives.flatMap(p => {
        if (p.locked || p.placements.length !== 1) return [];
        const c = params.options.componentByDesignator?.get(p.placements[0].designator);
        if (!c || c.pcb.role !== 'decoupling_cap') return [];
        const nets = c.pins.map(pin => pin.signal_name);
        if (nets.length !== 2 || nets.some(n => !n || isGroundSignalName(n) || isPowerSignalName(n))) return [];
        return [{ designator: c.designator, from: c.pcb.role, to: 'passive' as const,
            reason: 'decoupling_cap has two connected signal pins and no recognised supply/ground pin' }];
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
