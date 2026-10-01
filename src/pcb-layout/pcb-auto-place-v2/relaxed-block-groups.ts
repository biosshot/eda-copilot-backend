import * as fp from '../f32.ts';
import type { PlacementInput, PlacementRelation, PlacementTreeNode } from '#types/pcb/layout-model.ts';
import type { PlacementPrimitive } from './primitives.ts';

export type GroupRelaxation = 'off' | 'satellites' | 'caps' | 'all';

/** Alternative representation, not an input/netlist rewrite. Core pairs,
 * lines, bypass islands, fixed geometry and explicit row topology stay rigid.
 * Removed group anchors are distributed over members. Explicit distance limits
 * on a whole group remain protected; a cap island's implicit target survives.
 */
export function relaxBlockGroups(input: PlacementInput, nodes: PlacementTreeNode[],
    primitives: PlacementPrimitive[], relations: PlacementRelation[], mode: GroupRelaxation,
    makeComponent: (designator: string) => PlacementPrimitive) {
    const replacements = new Map<string, string[]>();
    const released: string[] = [];
    const split = (p: PlacementPrimitive, insideSatellite = false): PlacementPrimitive[] => {
        const fixed = p.locked || p.placements.some(q => {
            const c = input.components.find(c => c.designator === q.designator);
            return c?.pcb.fixedPlacement || c?.pcb.edgeMount || c?.pcb.edgePlace || c?.pcb.syntheticBoardPad;
        });
        if (fixed || mode === 'off') return [p];
        const node = nodes.find(n => n.id === p.sourceNodeId);
        const block = input.blocks.find(b => b.name === p.label);
        const satellite = p.kind === 'block' && block?.placement === 'satellite'
            && (mode === 'satellites' || mode === 'all');
        const cap = p.kind === 'island' && node?.data?.kind === 'cap_cluster'
            && (mode === 'caps' || mode === 'all')
            && !['axis', 'maxRows', 'maxPerRow', 'topology'].some(k => node.data?.[k] != null);
        const passive = p.kind === 'island' && p.label.startsWith('passive_net:')
            && (insideSatellite || ((mode === 'caps' || mode === 'all') && p.placements.every(q => /^C\d/.test(q.designator))));
        if (!satellite && !cap && !passive) return [p];
        const endpoints = [p.sourceNodeId.replace(/^tree:/, ''), `${p.kind}:${p.label}`];
        // A hard whole-group constraint cannot be represented by independent
        // endpoint hints. Keep the group rather than silently weakening it.
        if (relations.some(r => r.hard && (r.kind !== 'island_target' || r.data?.maxDistance != null || r.data?.minDistance != null)
            && (endpoints.includes(r.from) || endpoints.includes(r.to)))) return [p];
        released.push(p.label);
        const members = p.placements.map(q => `component:${q.designator}`);
        for (const endpoint of endpoints) replacements.set(endpoint, members);
        if (satellite && p.children.length) return p.children.flatMap(child => split(child, true));
        return p.placements.map(q => makeComponent(q.designator));
    };
    const expanded = primitives.flatMap(p => split(p));
    const mapped = relations.flatMap(r => {
        const from = replacements.get(r.from) ?? [r.from], to = replacements.get(r.to) ?? [r.to];
        if (from[0] === r.from && to[0] === r.to) return [r];
        return from.flatMap(a => to.filter(b => b !== a).map(b => ({ ...r,
            id: `${r.id}:released:${a}:${b}`, from: a, to: b,
            weight: fp.div((r.weight ?? 70), (from.length * to.length)),
        })));
    });
    return { primitives: expanded, relations: mapped, released };
}
