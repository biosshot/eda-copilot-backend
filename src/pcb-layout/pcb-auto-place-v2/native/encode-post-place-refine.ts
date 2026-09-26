import type { PlacementInput, Placement, TargetRef, Point } from '#types/pcb/layout-model.ts';
import { normalizeRotation } from '#utils/math.ts';
import { samePartUuid } from '#types/lcsc.ts';
import { isConnectedSignalName } from '#utils/signals.ts';
import { boardBox, boardAnchorPoint, boardHoleKeepoutRadius, getBox, componentBox, componentCollisionBoxes, getLocalPointWorld, componentPadBox, isThroughHolePad } from '../../pcb-auto-place/geometry.ts';
import { componentPairClearance, familyBlockDesignators, canonicalModuleDesignators, blockBboxLimit, familyBboxLimit, moduleBboxLimit } from '../../pcb-auto-place/report-helpers.ts';
import { expandHints } from '../../pcb-auto-place/hints.ts';
import { createPostPlaceRouteScoreContext, routeLayoutProblem } from '../post-place-route-score.ts';

export type RefineTarget = { kind: 'missing' | 'component' | 'pin' | 'group' | 'point'; component?: number; pad?: number; members?: number[]; point?: Point };

/** Compile invariant semantics once. Rust owns every iteration and candidate. */
export function encodeNativePostPlaceRefineProblem(input: PlacementInput, placements: Placement[], threads: number) {
    const componentsByName = new Map(input.components.map((c, i) => [c.designator, i]));
    const componentMap = new Map(input.components.map(c => [c.designator, c]));
    const poseIndex = new Map(placements.map((p, i) => [p.designator, i]));
    for (const c of input.components) if (!poseIndex.has(c.designator)) throw new Error(`Post-place refine: missing placement for ${c.designator}`);
    const members = (names: Iterable<string>) => [...names].flatMap(name => componentsByName.has(name) ? [componentsByName.get(name)!] : []);
    const target = (ref?: TargetRef | 'all'): RefineTarget => {
        if (!ref || ref === 'all') return { kind: 'missing' };
        if (ref.type === 'board_anchor') return { kind: 'point', point: boardAnchorPoint(input.board, ref.anchor) };
        if (ref.type === 'block') return { kind: 'group', members: members(input.blocks.find(b => b.name === ref.block_name)?.component_designators ?? []) };
        const component = componentsByName.get(ref.designator);
        if (component === undefined) return { kind: 'missing' };
        if (ref.type === 'component') return { kind: 'component', component };
        const pad = input.components[component].footprint.pads.findIndex(p => String(p.pin_number) === String(ref.pin_number));
        return pad < 0 ? { kind: 'missing' } : { kind: 'pin', component, pad };
    };
    const explicitMembers = new Set((input.refineGroups ?? []).flatMap(g => g.componentDesignators));
    const explicitlyRefinable = input.components.map(c => !c.pcb.edgeMount && !c.pcb.edgePlace && !c.pcb.syntheticBoardPad);
    const fixed = input.components.map(c => Boolean(c.pcb.fixedPlacement));
    const automatic = input.components.map((c, i) => explicitlyRefinable[i] && !fixed[i] && !explicitMembers.has(c.designator));
    const groups = [
        ...input.blocks.map(b => ({ name: '', members: members(b.component_designators).filter(i => automatic[i]), rotate: false, swap: true, deltas: [0, 180] })),
        ...(input.refineGroups ?? []).map(g => ({ name: g.name, members: members(g.componentDesignators).filter(i => explicitlyRefinable[i]), rotate: g.rotateBy.includes(180), swap: g.swap, deltas: g.rotateBy.includes(180) ? [0, 180] : [0] })),
    ];
    const geometrySignature = (i: number, angle: number) => {
        const c = input.components[i];
        const rotate = (p: Point) => getLocalPointWorld({ designator: '', x: 0, y: 0, rotate: angle, layer: 'top', score: 0 }, p);
        const round = (n: number) => Math.round(n * 1000) / 1000;
        const size = angle % 180 === 0 ? [c.footprint.width, c.footprint.height] : [c.footprint.height, c.footprint.width];
        const pads = c.footprint.pads.map(p => { const q = rotate(p); return [round(q.x), round(q.y), round(angle % 180 ? p.height : p.width), round(angle % 180 ? p.width : p.height), p.shape ?? '', p.mount ?? '', round(p.drillDiameter ?? 0)].join(':'); }).sort();
        return `${round(size[0])}x${round(size[1])}|${pads.join('|')}`;
    };
    const signatures = input.components.map((_, i) => [0, 90, 180, 270].map(a => geometrySignature(i, a)));
    const compatibility = input.components.map((a, i) => input.components.map((b, j) => {
        const offset = signatures[i].findIndex(s => s === signatures[j][0]);
        return samePartUuid(a.part_uuid, b.part_uuid) || Boolean(a.footprint_uuid && a.footprint_uuid === b.footprint_uuid) || offset >= 0 ? Math.max(0, offset) * 90 : -1;
    }));
    // Coordinates are never invented by refine. Compile exact JS collation ranks
    // for reachable pose keys so Rust preserves localeCompare ties without callbacks.
    const reachable = input.components.map((_, i) => new Set([i]));
    const connect = (a: number, b: number) => { if (compatibility[a][b] >= 0) { reachable[a].add(b); reachable[b].add(a); } };
    for (const g of groups) if (g.swap) for (const a of g.members) for (const b of g.members) connect(a, b);
    const fixedMembers = members(input.components.filter((c, i) => fixed[i] && explicitlyRefinable[i] && !explicitMembers.has(c.designator)).map(c => c.designator));
    for (const a of fixedMembers) for (const b of fixedMembers) connect(a, b);
    for (let k = 0; k < reachable.length; k++) for (let i = 0; i < reachable.length; i++) if (reachable[i].has(k)) for (const j of reachable[k]) reachable[i].add(j);
    const angles = [...new Set([...placements.flatMap(p => [0, 90, 180, 270].map(a => normalizeRotation(p.rotate + a))), ...input.components.flatMap(c => c.pcb.allowedRotations.map(normalizeRotation))])];
    const keys = input.components.map((c, i) => [...reachable[i]].flatMap(j => {
        const p = placements[poseIndex.get(input.components[j].designator)!];
        return angles.flatMap(rotate => (['top', 'bottom'] as const).map(layer => ({ x: p.x, y: p.y, rotate, layer, key: `${c.designator}:${p.x}:${p.y}:${rotate}:${layer}`, rank: 0 })));
    }));
    const keyRank = new Map([...new Set(keys.flat().map(k => k.key))].sort((a, b) => a.localeCompare(b)).map((key, rank) => [key, rank]));
    for (const entries of keys) for (const key of entries) key.rank = keyRank.get(key.key)!;
    const lexRank = new Map(input.components.map(c => c.designator).sort().map((name, i) => [name, i]));
    let obstacleOffset = 0;
    const components = input.components.map((c, i) => ({
        obstacleOffset: (() => { const start = obstacleOffset; obstacleOffset += c.footprint.pads.length; return start; })(),
        designator: c.designator, poseIndex: poseIndex.get(c.designator)!, lexRank: lexRank.get(c.designator)!,
        automatic: automatic[i], fixed: fixed[i], diagnostic: fixedMembers.includes(i),
        allowedRotations: (c.pcb.allowedRotations.length ? c.pcb.allowedRotations : [0, 90, 180, 270]).map(normalizeRotation),
        allowedLayers: c.pcb.allowedLayers.filter(l => input.board.allowedLayers.includes(l)),
        overflow: { left: 0, right: 0, top: 0, bottom: 0, ...c.pcb.boardOverflow }, through: c.footprint.pads.some(isThroughHolePad), keys: keys[i],
        pads: c.footprint.pads.map(p => ({ ref: `${c.designator}.${String(p.pin_number)}`, through: isThroughHolePad(p), net: (() => { const pin = c.pins.find(pin => String(pin.pin_number) === String(p.pin_number)); return pin && isConnectedSignalName(pin.signal_name) ? pin.signal_name : undefined; })() })),
        pins: c.pins.flatMap(pin => { const pad = c.footprint.pads.findIndex(p => String(p.pin_number) === String(pin.pin_number)); return pad >= 0 && isConnectedSignalName(pin.signal_name) ? [{ pad, net: pin.signal_name, ref: `${c.designator}.${String(pin.pin_number)}` }] : []; }),
        orientations: angles.flatMap(rotate => (['top', 'bottom'] as const).map(layer => {
            const pose: Placement = { designator: c.designator, x: 0, y: 0, rotate, layer, score: 0 };
            return { rotate, layer, box: getBox(c, pose), body: componentBox(c, pose), opposite: componentCollisionBoxes(c, pose, layer === 'top' ? 'bottom' : 'top'),
                points: c.footprint.pads.map(p => getLocalPointWorld(pose, p)), padBoxes: c.footprint.pads.map(p => componentPadBox(pose, p)) };
        })),
    }));
    const ignored = new Set(input.solverOptions.ignoredRatsnestSignals.map(s => s.toUpperCase()));
    const netPoints = new Map<string, Array<{ component: number; pad: number }>>();
    for (const [i, c] of components.entries()) for (const pin of c.pins) if (!ignored.has(pin.net.toUpperCase())) {
        const list = netPoints.get(pin.net) ?? []; list.push({ component: i, pad: pin.pad }); netPoints.set(pin.net, list);
    }
    const nets = [...netPoints].map(([name, points]) => ({ name, points, weight: /^(?:VBUS|VCC|VDD|VIN|BAT|AVDD|DVDD|IOVDD|ADC_AVDD|VREG|[+]\w+)/i.test(name) ? 0.25 : 1 }));
    const hints = expandHints(input).map(h => ({ ...h, source: target(h.source), target: target(h.target), all: h.target === 'all', weight: Math.max(1, h.weight) }));
    const hierarchy: Array<{ key: string; source: RefineTarget; maxWidth?: number | null; maxHeight?: number | null; anchor?: RefineTarget; offset?: Point; maxGap?: number }> = [];
    for (const b of input.blocks) {
        const source = target({ type: 'block', block_name: b.name });
        if (b.hardBbox) hierarchy.push({ key: `block:${b.name}:bbox`, source, ...blockBboxLimit(input, b, componentMap) });
        if (b.familyHard) hierarchy.push({ key: `block:${b.name}:family-bbox`, source: { kind: 'group', members: members(familyBlockDesignators(input, b)) }, ...familyBboxLimit(input, b, componentMap) });
        if (b.hardAnchor && b.anchor && b.maxAnchorGap !== undefined) hierarchy.push({ key: `block:${b.name}:anchor`, source, anchor: target(b.anchor), offset: { x: b.anchorOffset?.x ?? 0, y: b.anchorOffset?.y ?? 0 }, maxGap: b.maxAnchorGap });
    }
    for (const m of input.modules) if (m.hardBbox) hierarchy.push({ key: `module:${m.name}:bbox`, source: { kind: 'group', members: members(canonicalModuleDesignators(input, m)) }, ...moduleBboxLimit(input, m, componentMap) });
    const route = routeLayoutProblem(input, placements, createPostPlaceRouteScoreContext(input));
    return { version: 1, threads, iterations: Math.max(0, Math.floor(input.solverOptions.localImproveIterations)), minDelta: Math.max(0, input.solverOptions.localImproveMinDelta),
        placements, components, groups, compatibility, nets, hints, hierarchy, routeProblem: route.problem,
        board: boardBox(input.board), polygon: input.board.outline.type === 'polygon' ? input.board.outline.points : [], edgeClearance: input.board.clearances.edge,
        holes: (input.boardHoles ?? []).map(h => ({ x: h.x, y: h.y, radius: boardHoleKeepoutRadius(h) + input.board.clearances.component })),
        regions: (input.constraintRegions ?? []).map(r => ({ name: r.name, box: r.box, layers: r.layers, allowed: members(input.components.filter(c => r.allowBlocks.includes(c.block_name)).map(c => c.designator)) })),
        pairClearances: input.components.map(a => input.components.map(b => componentPairClearance(input, a, b))),
        paths: (input.paths ?? []).map(p => ({ id: p.id, shape: p.shape, priority: p.priority, preferFacingPads: p.preferFacingPads,
            ports: p.segments.flatMap(s => [{ target: target(s.source), order: s.index * 2, ref: `${s.source.designator}.${String(s.source.pin_number)}`, role: s.index === 0 ? 'source' : 'exit' },
                { target: target(s.target), order: s.index * 2 + 1, ref: `${s.target.designator}.${String(s.target.pin_number)}`, role: s.index === p.segments.length - 1 ? 'target' : 'entry' }]) })),
    };
}
export type NativePostPlaceRefineProblem = ReturnType<typeof encodeNativePostPlaceRefineProblem>;
