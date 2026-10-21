import * as fp from '../../f32.ts';
import type { PcbComponent, Placement, PlacementInput, TargetRef } from '#types/pcb/layout-model.ts';
import { isConnectedSignalName } from '#utils/signals.ts';
import {
    boardBox, boardHoleKeepoutRadius, boardOutlinePolygon, componentBodyBox, componentBox, componentCollisionBoxes,
    componentPadBox, getBox, getLocalPointWorld, GEOMETRY_EPSILON, padPlacementLayer, boardAnchorPoint, unionBoxes,
} from '../../pcb-auto-place/geometry.ts';
import { componentPairClearance, targetDesignators } from '../../pcb-auto-place/report-helpers.ts';
import { placementsCanConflict } from '../../pcb-auto-place/utils.ts';
import { expandHints, pathRelationWeight } from '../../pcb-auto-place/hints.ts';
import { createPlacementReport } from '../../pcb-auto-place/placement-report.ts';
import { boardSpacingExemptPairs } from '../board-spacing.ts';
import type { PlacementPrimitive } from '../primitives.ts';
import { componentAccessRows } from './pad-access.ts';
import { encodeNativePostPlaceRefineProblem } from './encode-post-place-refine.ts';
import { encodeNativePostPlaceScoreProblem, postPlaceLocalScopes } from './encode-post-place-score.ts';
import { loadNativeBoardPacker } from './load-native-board-packer.ts';

export const NATIVE_BOARD_ALIGNMENT_CONTRACT_VERSION = 1;

/** A coordinate source the native pass can re-evaluate after a translation. */
type AlignmentTarget =
    | { kind: 'missing' }
    | { kind: 'point'; x: number; y: number }
    | { kind: 'component'; component: number }
    | { kind: 'pin'; component: number; pad: number }
    | { kind: 'block'; members: number[] };

export interface BoardAlignmentPairInput {
    a: string; b: string; similarity: number; anchorA?: string; anchorB?: string;
    orientation?: { a: string; b: string; offset: number };
}

/**
 * Compile the finishing pass once. This pass only translates rigid blocks, so
 * every rotation, layer and target resolution is fixed: each component carries
 * its geometry at its own pose with x = y = 0, and Rust applies the same
 * coordinate transport as `sourceCoordinateAdd` for every candidate. Per-call
 * native frames are recomputed in Rust exactly as the individual addon calls do.
 */
export function encodeNativeBoardAlignmentProblem(input: PlacementInput, roots: PlacementPrimitive[], initial: Placement[],
    pairs: BoardAlignmentPairInput[], options: { threads: number; spacingGap: number; passes: number; maxShift: number }) {
    const index = new Map(input.components.map((c, i) => [c.designator, i]));
    const placementByDesignator = new Map(initial.map(p => [p.designator, p]));
    const componentByDesignator = new Map(input.components.map(c => [c.designator, c]));
    const poseIndex = new Map(initial.map((p, i) => [p.designator, i]));
    for (const c of input.components) if (!placementByDesignator.has(c.designator)) throw new Error(`Board alignment: missing placement for ${c.designator}`);
    const placementOf = (c: PcbComponent) => placementByDesignator.get(c.designator)!;
    const padIndex = (c: PcbComponent, pin: string | number) => c.footprint.pads.findIndex(p => String(p.pin_number) === String(pin));

    const components = input.components.map(c => {
        const p = placementOf(c);
        const at: Placement = { designator: c.designator, x: 0, y: 0, rotate: p.rotate, layer: p.layer, score: 0 };
        return {
            designator: c.designator, poseIndex: poseIndex.get(c.designator)!, x: p.x, y: p.y, rotate: p.rotate, layer: p.layer,
            body: componentBodyBox(c, at), hintBox: getBox(c, at), packBox: componentBox(c, at),
            rows: componentAccessRows(c, at),
            pads: c.footprint.pads.map(pad => ({ point: getLocalPointWorld(at, pad), box: componentPadBox(at, pad) })),
            collision: { top: componentCollisionBoxes(c, at, 'top'), bottom: componentCollisionBoxes(c, at, 'bottom') },
            ...reportComponent(input, c),
        };
    });

    const resolve = (ref: TargetRef | 'all' | undefined): AlignmentTarget => {
        if (!ref || ref === 'all') return { kind: 'missing' };
        if (ref.type === 'board_anchor') { const p = boardAnchorPoint(input.board, ref.anchor); return { kind: 'point', x: p.x, y: p.y }; }
        if (ref.type === 'block') {
            const members = (input.blocks.find(b => b.name === ref.block_name)?.component_designators ?? [])
                .flatMap(name => index.has(name) ? [index.get(name)!] : []);
            return members.length ? { kind: 'block', members } : { kind: 'missing' };
        }
        const component = index.get(ref.designator);
        if (component === undefined) return { kind: 'missing' };
        if (ref.type === 'component') return { kind: 'component', component };
        if (ref.type === 'pin') {
            const pad = padIndex(input.components[component], ref.pin_number);
            return pad < 0 ? { kind: 'missing' } : { kind: 'pin', component, pad };
        }
        return { kind: 'missing' };
    };

    return {
        version: NATIVE_BOARD_ALIGNMENT_CONTRACT_VERSION,
        threads: options.threads,
        passes: options.passes,
        maxShift: options.maxShift,
        grid: fp.max(.5, input.solverOptions.placementGridStep ?? .5),
        placements: initial.map(p => ({ designator: p.designator, x: p.x, y: p.y })),
        components,
        roots: roots.map(root => {
            const members = root.placements.flatMap(p => index.has(p.designator) && placementByDesignator.has(p.designator) ? [index.get(p.designator)!] : []);
            return {
                id: root.id, label: root.label, members,
                movable: !root.locked && !root.anchored && root.placements.every(p => {
                    const c = componentByDesignator.get(p.designator)!;
                    return !c.pcb.fixedPlacement && !c.pcb.edgeMount && !c.pcb.edgePlace;
                }),
                // `encodePrimitive` keeps these stale tree coordinates; they only enter the alignment-score frame.
                bboxCollision: !root.collisionBoxes?.length,
                staticPoints: [...(root.collisionBoxes?.length ? root.collisionBoxes.flatMap(b => [{ x: b.left, y: b.top }, { x: b.right, y: b.bottom }]) : []),
                    ...root.connectionPoints.map(p => ({ x: p.x, y: p.y })), ...(root.pathPorts ?? []).map(p => ({ x: p.x, y: p.y }))],
            };
        }),
        pairs: pairs.map(pair => ({
            a: roots.findIndex(r => r.id === pair.a), b: roots.findIndex(r => r.id === pair.b), similarity: pair.similarity,
            anchorA: pair.anchorA ?? null, anchorB: pair.anchorB ?? null, orientation: pair.orientation ?? null,
        })),
        alignmentPolicy: loadNativeBoardPacker().placementAlignmentPolicy(pairs.length, false),
        score: scoreTemplate(input, initial, placementByDesignator, index, resolve, padIndex),
        spacing: spacingTemplate(input, roots, placementByDesignator, componentByDesignator, options.spacingGap),
        report: reportTemplate(input, initial, resolve),
        constraints: encodeNativePostPlaceRefineProblem(input, initial, 1),
    };
}

function reportComponent(input: PlacementInput, c: PcbComponent) {
    const board = boardBox(input.board);
    const edge = input.board.clearances.edge;
    const overflow = c.pcb.boardOverflow ?? {};
    const hasOverflow = (overflow.left ?? 0) > 0 || (overflow.right ?? 0) > 0 || (overflow.top ?? 0) > 0 || (overflow.bottom ?? 0) > 0;
    const tolerance = fp.add(0.01, GEOMETRY_EPSILON);
    const polygon = input.board.outline.type === 'polygon' && !hasOverflow;
    const leftLimit = (overflow.left ?? 0) > 0 ? fp.sub(board.left, (overflow.left ?? 0)) : fp.add(board.left, edge);
    const rightLimit = (overflow.right ?? 0) > 0 ? fp.add(board.right, (overflow.right ?? 0)) : fp.sub(board.right, edge);
    const topLimit = (overflow.top ?? 0) > 0 ? fp.sub(board.top, (overflow.top ?? 0)) : fp.add(board.top, edge);
    const bottomLimit = (overflow.bottom ?? 0) > 0 ? fp.add(board.bottom, (overflow.bottom ?? 0)) : fp.sub(board.bottom, edge);
    return {
        // Same exemption as `createPlacementReport`; collisions still apply.
        boardExempt: c.pcb.fixedPlacement !== undefined && !c.pcb.fixedBlock,
        outside: polygon ? { polygon: true, edge: fp.max(0, fp.sub(edge, tolerance)), limits: [0, 0, 0, 0] }
            : { polygon: false, edge: 0, limits: [fp.sub(leftLimit, tolerance), fp.add(rightLimit, tolerance), fp.sub(topLimit, tolerance), fp.add(bottomLimit, tolerance)] },
        regions: (input.constraintRegions ?? []).flatMap((r, i) => r.allowBlocks.includes(c.block_name) ? [] : [i]),
    };
}

/** Same entries, order and resolution as `encodeNativePostPlaceScoreProblem`. */
function scoreTemplate(input: PlacementInput, initial: Placement[], placements: Map<string, Placement>, index: Map<string, number>,
    resolve: (ref: TargetRef | 'all' | undefined) => AlignmentTarget, padIndex: (c: PcbComponent, pin: string | number) => number) {
    const ignored = new Set(input.solverOptions.ignoredRatsnestSignals.map(s => s.toUpperCase()));
    const localScopes = postPlaceLocalScopes(input);
    const nets = new Map<string, { points: Array<[number, number]>; layers: Array<string | null>; owners: Array<string | null>; scopes: Array<string | null> }>();
    const obstacles: Array<{ component: number; pad: number; ref: string; net?: string; layer?: string }> = [];
    for (const [ci, c] of input.components.entries()) {
        const p = placements.get(c.designator);
        if (!p) continue;
        for (const [pi, pad] of c.footprint.pads.entries()) obstacles.push({ component: ci, pad: pi, ref: `${c.designator}.${String(pad.pin_number)}`,
            net: c.pins.find(q => String(q.pin_number) === String(pad.pin_number))?.signal_name ?? undefined, layer: padPlacementLayer(pad, p) });
        for (const pin of c.pins) {
            if (!isConnectedSignalName(pin.signal_name) || ignored.has(pin.signal_name.toUpperCase())) continue;
            const pad = padIndex(c, pin.pin_number);
            if (pad < 0) continue;
            const net = nets.get(pin.signal_name) ?? { points: [], layers: [], owners: [], scopes: [] };
            net.points.push([ci, pad]);
            net.layers.push(padPlacementLayer(c.footprint.pads[pad], p) ?? null);
            net.owners.push(c.pcb.role === 'main_ic' ? c.designator : null);
            net.scopes.push(localScopes.get(c.designator) ?? null);
            nets.set(pin.signal_name, net);
        }
    }
    const weights = loadNativeBoardPacker().placementNetWeights([...nets.keys()], false);
    const explicit = input.hints.flatMap(h => {
        if (h.relation !== 'critical_pair' || h.source.type !== 'pin' || h.target.type !== 'pin') return [];
        const ai = index.get(h.source.designator), bi = index.get(h.target.designator);
        if (ai === undefined || bi === undefined) return [];
        const a = input.components[ai], b = input.components[bi];
        const ap = placements.get(a.designator), bp = placements.get(b.designator);
        if (!ap || !bp) return [];
        const pa = padIndex(a, h.source.pin_number), pb = padIndex(b, h.target.pin_number);
        const net = a.pins.find(p => String(p.pin_number) === String(h.source.type === 'pin' ? h.source.pin_number : ''))?.signal_name;
        const other = b.pins.find(p => String(p.pin_number) === String(h.target.type === 'pin' ? h.target.pin_number : ''))?.signal_name;
        if (pa < 0 || pb < 0 || !net || net !== other) return [];
        const la = padPlacementLayer(a.footprint.pads[pa], ap), lb = padPlacementLayer(b.footprint.pads[pb], bp);
        return [{ source: [ai, pa], target: [bi, pb], net, layer: (la === lb ? la : !la ? lb : !lb ? la : undefined) ?? null,
            kind: h.path ? 'signal_path' : 'critical_pair' }];
    });
    const distances: unknown[] = [], clearances: unknown[] = [], edges: unknown[] = [];
    for (const rule of expandHints(input)) {
        const weight = rule.weight;
        if (rule.kind === 'distance' && rule.target && rule.target !== 'all') {
            const source = resolve(rule.source), target = resolve(rule.target);
            if (source.kind !== 'missing' && target.kind !== 'missing')
                distances.push({ source, target, weight, inverseTauSquared: rule.inverseTauSquared ?? null, min: rule.min ?? null, max: rule.max ?? null });
        }
        if (rule.kind === 'clearance' && rule.target && rule.min !== undefined) {
            const source = resolve(rule.source);
            if (source.kind === 'missing') continue;
            if (rule.target === 'all') {
                for (const [ci, c] of input.components.entries()) {
                    if (targetDesignators(input, rule.source).includes(c.designator)) continue;
                    if (placements.has(c.designator)) clearances.push({ source, target: { kind: 'component', component: ci }, minimum: rule.min, weight });
                }
            } else {
                const target = resolve(rule.target);
                if (target.kind !== 'missing') clearances.push({ source, target, minimum: rule.min, weight });
            }
        }
        if (rule.kind === 'edge' && rule.edge) {
            const source = resolve(rule.source);
            if (source.kind !== 'missing') edges.push({ source, board: boardBox(input.board), edge: rule.edge, weight });
        }
    }
    const paths = (input.paths ?? []).map(path => {
        const ports: unknown[] = [];
        const port = (order: number, target: { designator: string; pin_number: string | number }, role: string) => {
            const ci = index.get(target.designator);
            if (ci === undefined || !placements.has(target.designator)) return;
            const pad = padIndex(input.components[ci], target.pin_number);
            if (pad >= 0) ports.push({ component: ci, pad, order, ref: `${target.designator}.${String(target.pin_number)}`, role });
        };
        for (const segment of path.segments) {
            port(segment.index * 2, segment.source, segment.index === 0 ? 'source' : 'exit');
            port(segment.index * 2 + 1, segment.target, segment.index === path.segments.length - 1 ? 'target' : 'entry');
        }
        return { pathId: path.id, ports, shape: path.shape, priority: path.priority, weight: pathRelationWeight(input, path.id), preferFacingPads: path.preferFacingPads };
    });
    const reference = encodeNativePostPlaceScoreProblem(input, initial);
    return {
        routeClearance: input.board.clearances.component,
        access: input.components.flatMap((c, ci) => placements.has(c.designator) ? [ci] : []),
        explicit, obstacles,
        nets: [...nets.entries()].map(([name, net], i) => ({ name, weight: weights[i], ...net })),
        distances, clearances, edges, paths,
        // Layers and rotations do not change in this pass.
        fixedPenalties: reference.fixedPenalties,
    };
}

function spacingTemplate(input: PlacementInput, roots: PlacementPrimitive[], placements: Map<string, Placement>,
    components: Map<string, PcbComponent>, gap: number) {
    // Board-pack encoding: one owner per component, first primitive wins.
    const seen = new Set<string>();
    const members = roots.map(root => root.placements.flatMap(p => {
        if (seen.has(p.designator) || !components.has(p.designator)) return [];
        seen.add(p.designator);
        return [input.components.findIndex(c => c.designator === p.designator)];
    }));
    const exempt = new Set(boardSpacingExemptPairs(input, roots).map(([a, b]) => `${a}\n${b}`));
    const active: Array<[number, number]> = [];
    for (let i = 0; i < roots.length; i++) for (let j = i + 1; j < roots.length; j++) {
        if (roots[i].locked === true && roots[j].locked === true) continue;
        const conflict = !members[i].length || !members[j].length || members[i].some(a => members[j].some(b => {
            const ca = input.components[a], cb = input.components[b];
            return placementsCanConflict(ca, placements.get(ca.designator)!, cb, placements.get(cb.designator)!);
        }));
        if (!conflict || exempt.has(`${roots[i].id}\n${roots[j].id}`) || exempt.has(`${roots[j].id}\n${roots[i].id}`)) continue;
        active.push([i, j]);
    }
    return {
        gap, clearance: input.board.clearances.component, members, pairs: active,
        bounds: { left: fp.div(-input.board.outline.width, 2), right: fp.div(input.board.outline.width, 2),
            top: fp.div(-input.board.outline.height, 2), bottom: fp.div(input.board.outline.height, 2) },
        // Component-less primitives keep their encoded collision boxes in every layer.
        fixedBoxes: roots.map((root, i) => members[i].length ? null : (root.collisionBoxes?.length ? root.collisionBoxes
            : [unionBoxes(root.placements.flatMap(p => components.has(p.designator) && placements.has(p.designator)
                ? [componentBox(components.get(p.designator)!, placements.get(p.designator)!)] : []))])),
    };
}

/** Geometry and mandatory-hint checks of `createPlacementReport` and `alignmentHardHintsNoWorse`. */
function reportTemplate(input: PlacementInput, initial: Placement[], resolve: (ref: TargetRef | 'all' | undefined) => AlignmentTarget) {
    const baseline = createPlacementReport(input, initial);
    const hintKeys = new Map<string, number>();
    const hintClass = (hint: unknown) => { const key = JSON.stringify(hint); if (!hintKeys.has(key)) hintKeys.set(key, hintKeys.size); return hintKeys.get(key)!; };
    const expected = new Map<string, number>();
    const expectedId = (text: string) => { if (!expected.has(text)) expected.set(text, expected.size); return expected.get(text)!; };
    const rules: unknown[] = [];
    for (const hint of input.hints) {
        if (!(hint.priority === 'critical' || ('hard' in hint && hint.hard))) continue;
        const cls = hintClass(hint);
        for (const rule of expandHints({ ...input, hints: [hint] })) {
            // Unresolved, layer and preference violations cannot change under translation.
            if (rule.kind === 'distance' && rule.target && rule.target !== 'all') {
                const source = resolve(rule.source), target = resolve(rule.target);
                if (source.kind === 'missing' || target.kind === 'missing') continue;
                rules.push({ kind: 'distance', cls, source, target, min: rule.min ?? null, max: rule.max ?? null,
                    minExpected: rule.min === undefined ? null : expectedId(`>= ${rule.min}mm`), maxExpected: rule.max === undefined ? null : expectedId(`<= ${rule.max}mm`) });
            }
            if (rule.kind === 'clearance' && rule.target && rule.target !== 'all') {
                const source = resolve(rule.source), target = resolve(rule.target);
                if (source.kind === 'missing' || target.kind === 'missing' || rule.min === undefined) continue;
                rules.push({ kind: 'clearance', cls, source, target, min: rule.min, max: null, minExpected: expectedId(`>= ${rule.min}mm clearance`), maxExpected: null });
            }
            if (rule.kind === 'edge' && rule.edge && rule.source.type === 'component') {
                const source = resolve(rule.source);
                if (source.kind !== 'component') continue;
                rules.push({ kind: 'edge', cls, source, target: { kind: 'missing' }, edge: rule.edge, min: rule.min ?? null, max: rule.max ?? null,
                    minExpected: rule.min === undefined ? null : expectedId(`>= ${rule.min}mm from ${rule.edge} edge`),
                    maxExpected: rule.max === undefined ? null : expectedId(`<= ${rule.max}mm from ${rule.edge} edge`) });
            }
        }
    }
    return {
        board: boardBox(input.board),
        polygon: input.board.outline.type === 'polygon' ? boardOutlinePolygon(input.board) : [],
        holes: (input.boardHoles ?? []).map(h => ({ x: h.x, y: h.y, required: fp.add(boardHoleKeepoutRadius(h), input.board.clearances.component) })),
        regions: (input.constraintRegions ?? []).map(r => ({ box: r.box, layers: r.layers })),
        // Layer violations and unplaced components cannot change under translation.
        staticFailure: baseline.layerViolations.length > 0 || baseline.unplaced.length > 0,
        rules,
        pairClearance: input.components.map(a => input.components.map(b => componentPairClearance(input, a, b))),
        baseline: baseline.hintViolations.flatMap(v => typeof v.actual === 'number' && hintKeys.has(JSON.stringify(v.hint)) && expected.has(v.expected)
            ? [{ cls: hintKeys.get(JSON.stringify(v.hint))!, expected: expected.get(v.expected)!, actual: v.actual, atLeast: v.expected.startsWith('>=') }] : []),
    };
}
export type NativeBoardAlignmentProblem = ReturnType<typeof encodeNativeBoardAlignmentProblem>;
