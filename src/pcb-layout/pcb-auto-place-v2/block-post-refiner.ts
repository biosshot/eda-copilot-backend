import type { PlacementInput, TargetRef } from '#types/pcb/layout-model.ts';
import type { ClearanceResolver } from '../pcb-auto-place/clearance-resolver.ts';
import { familyBlockDesignators } from '../pcb-auto-place/report-helpers.ts';
import { createFixedPlacement } from '../pcb-auto-place/fixed.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import { rotatePrimitive, translatePrimitive, type PlacementPrimitive } from './primitives.ts';

/** Local swap budget before a block becomes a rigid board-placement primitive.
 * Islands stay rigid. Board anchors and external nets are evaluated later in
 * board context, never against invented positions in this local frame.
 */
export function refineBlockPrimitives(input: PlacementInput, primitives: PlacementPrimitive[], clearance: ClearanceResolver, world = false) {
    const placements = scopedPlacements(input, primitives, world);
    const names = new Set(placements.map(p => p.designator));
    const movable = new Set(primitives.filter(p => p.kind === 'component' && !p.locked && p.placements.length === 1)
        .map(p => p.placements[0].designator).filter(d => {
            const pcb = input.components.find(c => c.designator === d)?.pcb;
            return pcb && !pcb.fixedPlacement && !pcb.edgeMount && !pcb.edgePlace && !pcb.syntheticBoardPad;
        }));
    if (movable.size < 2) return { primitives, moves: 0, ms: 0 };
    const start = performance.now();
    const scoped = blockScopedInput(input, primitives, world);
    const problem = encodeNativePostPlaceRefineProblem(scoped, placements, 1);
    problem.routingMetric = 'geometric';
    problem.iterations = 8;
    problem.timeoutMs = 2000;
    problem.pairClearances = scoped.components.map(a => scoped.components.map(b => clearance(a.designator, b.designator)));
    for (const c of problem.components) {
        c.diagnostic = false;
        // Explicit refine groups must not unlock members of an assembled island.
        if (!movable.has(c.designator)) c.automatic = false;
        const primitive = primitives.find(p => p.placements.length === 1 && p.placements[0].designator === c.designator);
        if (primitive) {
            const base = primitive.placements[0].rotate;
            const deltas = primitive.allowedOrientations?.length ? primitive.allowedOrientations : primitive.canRotate ? [0, 90, 180, 270] : [0];
            c.allowedRotations = c.allowedRotations.filter(a => deltas.some(d => ((base + d) % 360 + 360) % 360 === a));
        }
    }
    for (const g of problem.groups) g.members = g.members.filter(i => movable.has(problem.components[i].designator));
    const addon = loadNativeBoardPacker();
    if (addon.postPlaceRefineContractVersion() !== 3) throw new Error('Geometric block postrefine requires npm run native:build');
    const result = addon.refinePostPlacement(problem);
    if (!addon.validatePlacementChange(problem, result.placements)) throw new Error('Block postrefine violated placement constraints');
    const poses = new Map(result.placements.map(p => [p.designator, p]));
    const refined = primitives.map(p => {
        if (p.placements.length !== 1 || !movable.has(p.placements[0].designator)) return p;
        const before = p.placements[0], after = poses.get(before.designator)!;
        if (after.layer !== before.layer) throw new Error('Block postrefine changed component layer');
        const rotated = rotatePrimitive(p, after.rotate - before.rotate);
        return translatePrimitive(rotated, after.x - rotated.placements[0].x, after.y - rotated.placements[0].y);
    });
    return { primitives: refined, moves: result.moves.length, ms: performance.now() - start };
}

/** Scope electrical constraints to a complete local block, without invented board anchors. */
export function scopedPlacements(input: PlacementInput, primitives: PlacementPrimitive[], world = false) {
    const placements = primitives.flatMap(p=>p.placements);
    if (world) for (const c of input.components) {
        if (placements.some(p=>p.designator===c.designator)) continue;
        const p = createFixedPlacement(input,c);
        if (p) placements.push(p);
    }
    return placements;
}

export function blockScopedInput(input: PlacementInput, primitives: PlacementPrimitive[], world = false): PlacementInput {
    const placements = scopedPlacements(input, primitives, world);
    const names = new Set(placements.map(p => p.designator));
    const movable = new Set(primitives.filter(p => !p.locked && p.kind === 'component' && p.placements.length === 1)
        .map(p => p.placements[0].designator));
    const blocks = input.blocks.filter(b => b.component_designators.length > 0 && b.component_designators.every(d => names.has(d)));
    const blockNames = new Set(blocks.map(b => b.name));
    const local = (t: TargetRef | 'all' | undefined): boolean => !t || t === 'all' ||
        (t.type === 'block' ? blockNames.has(t.block_name) : t.type !== 'board_anchor' && names.has(t.designator));
    const extent = Math.max(10, ...primitives.flatMap(p => Object.values(p.bbox).map(Math.abs))) + 10;
    return {
        ...input,
        board: world ? input.board : { ...input.board, outline: { type: 'rect', width: extent * 2, height: extent * 2 } },
        boardHoles: world ? input.boardHoles : [], constraintRegions: world ? input.constraintRegions : [], modules: [],
        components: input.components.filter(c => names.has(c.designator)).map(c => ({ ...c, pcb: { ...c.pcb,
            fixedPlacement: movable.has(c.designator) ? c.pcb.fixedPlacement : placements.find(p => p.designator === c.designator)!,
        } })),
        blocks: blocks.map(b => ({ ...b, familyHard: Boolean(b.familyHard && familyBlockDesignators(input, b).every(d => names.has(d))), hardAnchor: Boolean(b.hardAnchor && local(b.anchor)),
            anchor: local(b.anchor) ? b.anchor : undefined })),
        hints: input.hints.filter(h => {
            if (h.relation === 'edge') return false;
            if (h.relation === 'line') return h.components.every(d => names.has(d));
            if (h.relation === 'bypass' || h.relation === 'cap_cluster') return h.capacitors.every(d => names.has(d)) && local(h.target);
            return local(h.source) && (!('target' in h) || local(h.target));
        }),
        paths: input.paths?.filter(p => p.segments.every(s => names.has(s.source.designator) && names.has(s.target.designator))),
        refineGroups: input.refineGroups?.filter(g => g.componentDesignators.every(d => names.has(d))),
    } satisfies PlacementInput;
}
