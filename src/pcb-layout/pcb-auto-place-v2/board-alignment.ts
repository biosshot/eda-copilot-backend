import { sinCosDegrees } from '../f32.ts';
import * as fp from '../f32.ts';
import type { PcbComponent, Placement, PlacementInput, PlacementReport, Point } from '#types/pcb/layout-model.ts';
import { boxCenter, boxGap, componentBox, unionBoxes } from '../pcb-auto-place/geometry.ts';
import { encodePrimitive } from './native/encode-block-problem.ts';
import { isConnectedSignalName } from '#utils/signals.ts';
import { boardSpacingPolicy } from './board-spacing.ts';
import { encodeNativePostPlaceScoreProblem } from './native/encode-post-place-score.ts';
import { encodeNativeBoardAlignmentProblem, NATIVE_BOARD_ALIGNMENT_CONTRACT_VERSION } from './native/encode-board-alignment.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import type { PlacementPrimitive, PrimitiveSolveDiagnostic } from './primitives.ts';

/** Conservative finishing pass, in mm. Similarity never attracts distant blocks. */
export const BOARD_ALIGNMENT_POLICY = Object.freeze({ similarity: .78, neighbourGap: 8, maxShift: 4, passes: 2 });


function family(c: PcbComponent): string {
    if (c.pcb.role === 'main_ic') return `IC${c.pins.length}`;
    return c.designator.match(/^[A-Za-z]+/)?.[0].toUpperCase() ?? c.pcb.role;
}
const area = (c: PcbComponent) => fp.mul(c.footprint.width, c.footprint.height);

function histogram(keys: string[]) {
    const result = new Map<string, number>();
    for (const key of keys) result.set(key, fp.add((result.get(key) ?? 0), 1));
    return result;
}
function signature(components: PcbComponent[]) {
    const nets = new Map<string, Set<PcbComponent>>();
    for (const c of components) for (const pin of c.pins) {
        if (!isConnectedSignalName(pin.signal_name)) continue;
        const members = nets.get(pin.signal_name) ?? new Set<PcbComponent>();
        members.add(c); nets.set(pin.signal_name, members);
    }
    // Typed edges, including local supply connectivity, independent of net names
    // and pin numbering. Multiple pads on one component do not multiply edges.
    const edges: string[] = [];
    for (const members of nets.values()) {
        const cs = [...members];
        for (let i = 0; i < cs.length; i++) for (let j = i + 1; j < cs.length; j++)
            edges.push([family(cs[i]), family(cs[j])].sort().join(':'));
    }
    return { nodes: histogram(components.map(family)), edges: histogram(edges) };
}

export function alignmentAnchor(components: PcbComponent[]): string | undefined {
    const sorted = [...components].sort((a, b) => fp.sub(area(b), area(a)) || a.designator.localeCompare(b.designator));
    if (!sorted.length) return undefined;
    if (sorted.length === 1 || area(sorted[0]) >= fp.mul(1.5, area(sorted[1]))) return sorted[0].designator;
    // When no footprint dominates, prefer a substantial IC; otherwise the block
    // bbox center is less arbitrary than choosing one of many equal passives.
    return sorted.find(c => c.pcb.role === 'main_ic' && area(c) >= fp.mul(.7, area(sorted[0])))?.designator;
}

export function blockSimilarity(a: PcbComponent[], b: PcbComponent[]): number {
    if (!a.length || !b.length) return 0;
    const aa = alignmentAnchor(a), ba = alignmentAnchor(b);
    if (Boolean(aa) !== Boolean(ba)) return 0;
    let dimensions: [number, number, number, number] | undefined;
    if (aa && ba) {
        const x = a.find(c => c.designator === aa)!, y = b.find(c => c.designator === ba)!;
        if (family(x) !== family(y)) return 0;
        dimensions = [x.footprint.width, x.footprint.height, y.footprint.width, y.footprint.height];
    }
    const x = signature(a), y = signature(b);
    return loadNativeBoardPacker().placementBlockSimilarity({nodesA:[...x.nodes],nodesB:[...y.nodes],edgesA:[...x.edges],edgesB:[...y.edges],dimensions});
}

/** Refresh bounds from postrefine poses without changing ownership or using stale
 * local alternatives. Rigid translations in this pass preserve internal layout. */
function currentRoots(input: PlacementInput, roots: PlacementPrimitive[], placements: Placement[]) {
    const cs = new Map(input.components.map(c => [c.designator, c]));
    const ps = new Map(placements.map(p => [p.designator, p]));
    return roots.map(root => {
        const poses = root.placements.map(p => ps.get(p.designator)!).filter(Boolean);
        const boxes = poses.map(p => componentBox(cs.get(p.designator)!, p));
        const bbox = unionBoxes(boxes);
        return { ...root, placements: poses, bbox, width: fp.sub(bbox.right, bbox.left), height: fp.sub(bbox.bottom, bbox.top) };
    });
}

/** Direction belongs to a main IC even when an inductor or resistor is the
 * largest footprint. Pin count distinguishes a core from small support ICs. */
export function orientationAnchor(components: PcbComponent[]): PcbComponent | undefined {
    return [...components].filter(c=>c.pcb.role==='main_ic').sort((a,b)=>b.pins.length-a.pins.length || fp.sub(area(b), area(a)) || a.designator.localeCompare(b.designator))[0]
        ?? components.find(c=>c.designator===alignmentAnchor(components));
}

/** Match numbered pad centroids, not footprint names or library zero angles.
 * Only comparable footprints receive an orientation preference. Offset rotates
 * A's local pad pattern onto B's, so equal physical direction means rotA=rotB+offset. */
export function footprintOrientationOffset(a: PcbComponent, b: PcbComponent): number | undefined {
    if(family(a)!==family(b))return undefined;
    const pattern=(c:PcbComponent)=>{
        const groups=new Map<string,Point[]>();
        for(const p of c.footprint.pads){const key=String(p.pin_number);const ps=groups.get(key)??[];ps.push(p);groups.set(key,ps);}
        const ps=[...groups].sort(([a],[b])=>a.localeCompare(b)).map(([key,ps])=>({key,x:fp.div(ps.reduce((s,p)=>fp.add(s, p.x),0), ps.length),y:fp.div(ps.reduce((s,p)=>fp.add(s, p.y),0), ps.length)}));
        const cx=fp.div(ps.reduce((s,p)=>fp.add(s, p.x),0), ps.length),cy=fp.div(ps.reduce((s,p)=>fp.add(s, p.y),0), ps.length);
        return ps.map(p=>({...p,x:fp.sub(p.x, cx),y:fp.sub(p.y, cy)}));
    };
    const ap=pattern(a),bp=pattern(b);
    if(ap.length<2 || ap.length!==bp.length || ap.some((p,i)=>p.key!==bp[i].key))return undefined;
    const span=fp.max(...ap.map(p=>fp.hypot(p.x, p.y)), ...bp.map(p=>fp.hypot(p.x, p.y)));
    if(span<.01)return undefined;
    const matches=[0,90,180,270].map(angle=>{
        const [s,c]=sinCosDegrees(angle);
        const rotated=ap.map(p=>({x:fp.sub(fp.mul(p.x, c), fp.mul(p.y, s)),y:fp.add(fp.mul(p.x, s), fp.mul(p.y, c))}));
        // Libraries may choose different row spacing for the same package. Fit
        // modest positive axis scales; never permit reflection or pin remapping.
        const scale=(axis:'x'|'y')=>{
            const from=fp.max(...rotated.map(p=>fp.abs(p[axis]))),to=fp.max(...bp.map(p=>fp.abs(p[axis])));
            return from<.01&&to<.01?1:fp.div(to, fp.max(.0001, from));
        };
        const sx=scale('x'),sy=scale('y');
        return {angle,error:sx<.8||sx>1.25||sy<.8||sy>1.25?Infinity:
            fp.max(...rotated.map((p,i)=>fp.hypot(fp.sub(fp.mul(p.x, sx), bp[i].x), fp.sub(fp.mul(p.y, sy), bp[i].y))))};
    }).sort((a,b)=>fp.sub(a.error, b.error)||a.angle-b.angle);
    return matches[0].error<=fp.max(.05, fp.mul(span, .03))?matches[0].angle:undefined;
}

export interface AlignmentPair { a: string; b: string; similarity: number; anchorA?: string; anchorB?: string;
    orientation?: {a:string;b:string;offset:number} }
export function findAlignmentPairs(input: PlacementInput, roots: PlacementPrimitive[], placements: Placement[], local = true): AlignmentPair[] {
    const current = currentRoots(input, roots, placements), pairs: AlignmentPair[] = [];
    const components = (root: PlacementPrimitive) => input.components.filter(c => root.placements.some(p => p.designator === c.designator));
    for (let i = 0; i < current.length; i++) for (let j = i+1; j < current.length; j++) {
        const a = current[i], b = current[j];
        if (!a.placements.length || !b.placements.length || a.locked && b.locked) continue;
        if (local && boxGap(a.bbox, b.bbox) > BOARD_ALIGNMENT_POLICY.neighbourGap) continue;
        if (a.placements.some(p => p.layer !== a.placements[0].layer) || b.placements.some(p => p.layer !== a.placements[0].layer)) continue;
        const ac = components(a), bc = components(b), similarity = blockSimilarity(ac, bc);
        if (similarity < BOARD_ALIGNMENT_POLICY.similarity) continue;
        const oa=orientationAnchor(ac),ob=orientationAnchor(bc);
        const offset=oa&&ob?footprintOrientationOffset(oa,ob):undefined;
        pairs.push({ a: a.id, b: b.id, similarity, anchorA: alignmentAnchor(ac), anchorB: alignmentAnchor(bc),
            orientation:oa&&ob&&offset!==undefined?{a:oa.designator,b:ob.designator,
                offset:a.placements[0].layer==='bottom'?-offset:offset}:undefined });
    }
    return pairs.sort((a,b) => fp.sub(b.similarity, a.similarity) || a.a.localeCompare(b.a) || a.b.localeCompare(b.b));
}
export interface BoardSoftAlignment { pairs: AlignmentPair[]; weight: number; tolerance: number; orientationWeight?:number }
export function boardAlignmentPolicy(input: PlacementInput, roots: PlacementPrimitive[]): BoardSoftAlignment {
    const pairs = findAlignmentPairs(input,roots,roots.flatMap(p=>p.placements),false);
    // Cosmetic alignment must not grow stronger simply because more pairs exist.
    return { pairs, ...loadNativeBoardPacker().placementAlignmentPolicy(pairs.length, true) };
}
/** Zero at alignment, quadratic near the axis and linear beyond 1mm. No distance
 * attenuation: moving a pair apart cannot erase its misalignment penalty. */
export function boardAlignmentScore(roots: PlacementPrimitive[], policy: BoardSoftAlignment) {
    return loadNativeBoardPacker().scoreBoardAlignment(roots.map(encodePrimitive), policy);
}

function orientationError(a:PlacementPrimitive,b:PlacementPrimitive,pair:AlignmentPair):number|undefined {
    const o=pair.orientation;if(!o)return undefined;
    const ap=a.placements.find(p=>p.designator===o.a),bp=b.placements.find(p=>p.designator===o.b);
    if(!ap||!bp)return undefined;
    const angle=fp.mod((fp.add(fp.mod((fp.sub(ap.rotate-bp.rotate, o.offset)), 360), 360)), 360);
    return Math.min(angle,360-angle);
}
function center(root: PlacementPrimitive, anchor?: string): Point {
    return root.placements.find(p => p.designator === anchor) ?? boxCenter(root.bbox);
}
export function alignmentErrors(roots: PlacementPrimitive[], pairs: AlignmentPair[]) {
    return pairs.map(pair => {
        const a = center(roots.find(r => r.id === pair.a)!, pair.anchorA);
        const b = center(roots.find(r => r.id === pair.b)!, pair.anchorB);
        return { ...pair, error: fp.min(fp.abs(fp.sub(a.x, b.x)), fp.abs(fp.sub(a.y, b.y))),
            orientationError:orientationError(roots.find(r=>r.id===pair.a)!,roots.find(r=>r.id===pair.b)!,pair) };
    });
}

/** Native relative validation compares violation identities, not their magnitude.
 * A finishing pass must not deepen an existing mandatory clearance violation. */
export function alignmentHardHintsNoWorse(before: PlacementReport, after: PlacementReport) {
    return after.hintViolations.filter(v => v.hint.priority === 'critical' || ('hard' in v.hint && v.hint.hard)).every(v =>
        before.hintViolations.some(b => JSON.stringify(b.hint) === JSON.stringify(v.hint) && b.expected === v.expected &&
            (typeof b.actual === 'number' && typeof v.actual === 'number'
                ? v.expected.startsWith('>=') ? v.actual >= b.actual : v.actual <= b.actual
                : b.actual === v.actual)));
}

/** Wiring-only guard, independent of area/clearance rewards. Small external-net
 * tradeoffs are necessary when translating rigid blocks. Bound total wire growth
 * and individual stretch as well as the routing score; mandatory pin distances
 * and signal-path limits are checked separately against the same baseline. */
export function boardElectricalQuality(input: PlacementInput, placements: Placement[]) {
    const problem=encodeNativePostPlaceScoreProblem(input,placements);
    return {
        score:loadNativeBoardPacker().scorePostPlace({...problem,nets:problem.nets.map(net=>({...net,localScopes:undefined})),distances:[],clearances:[],fixedPenalties:[],edges:[],paths:[]}),
        lengths:loadNativeBoardPacker().postPlaceNetLengths(problem),
    };
}
export function boardElectricalRegression(before: ReturnType<typeof boardElectricalQuality>, after: ReturnType<typeof boardElectricalQuality>) {
    return loadNativeBoardPacker().boardElectricalRegression(before, after) ?? undefined;
}

/** One native call: Rust generates, scores, validates and selects every
 * candidate. The synchronous API evaluates on one Rust thread; selection is
 * deterministic and independent of the thread count. */
export function refineBoardAlignment(input: PlacementInput, roots: PlacementPrimitive[], initial: Placement[], threads = 1) {
    const pairs = findAlignmentPairs(input, roots, initial);
    const before = alignmentErrors(currentRoots(input, roots, initial), pairs);
    const diagnostics: PrimitiveSolveDiagnostic[] = [];
    if (!pairs.length) return { placements: initial, pairs, before, after: before, moves: [] as BoardAlignmentMove[], diagnostics, evaluated: 0,
        rejected: { objective: 0, electrical: 0, netLength: 0, geometry: 0, hardHint: 0, native: 0 } };
    const addon = loadNativeBoardPacker();
    if (addon.boardAlignmentContractVersion() !== NATIVE_BOARD_ALIGNMENT_CONTRACT_VERSION)
        throw new Error(`Rust board alignment contract does not match TypeScript contract ${NATIVE_BOARD_ALIGNMENT_CONTRACT_VERSION}; rebuild the native addon`);
    const result = addon.refineBoardAlignment(encodeNativeBoardAlignmentProblem(input, roots, initial, pairs, {
        threads, spacingGap: boardSpacingPolicy(input).gap, passes: BOARD_ALIGNMENT_POLICY.passes, maxShift: BOARD_ALIGNMENT_POLICY.maxShift }));
    const placements = result.moves.length ? initial.map((p, i) => {
        const moved = result.positions[i];
        return moved ? { ...p, x: moved[0], y: moved[1] } : p;
    }) : initial;
    diagnostics.push({ severity: 'warning', nodeId: 'board-alignment', message:
        `Board soft alignment: similarity >= ${BOARD_ALIGNMENT_POLICY.similarity}, ${pairs.length} nearby pairs, ${result.evaluated} candidates, ${result.moves.length} accepted; fixed components and block interiors preserved` });
    return { placements, pairs, before, after: alignmentErrors(currentRoots(input, roots, placements), pairs), moves: result.moves,
        diagnostics, evaluated: result.evaluated, rejected: result.rejected };
}
export interface BoardAlignmentMove { blocks: string[]; axis: 'x' | 'y'; shifts: number[]; perpendicularShifts: number[]; scoreBefore: number; scoreAfter: number }
