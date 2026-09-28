import type { PcbComponent, Placement, PlacementInput, PlacementReport, Point } from '#types/pcb/layout-model.ts';
import { boxCenter, boxGap, componentBox, unionBoxes } from '../pcb-auto-place/geometry.ts';
import { minimumSpanningEdges } from '../pcb-auto-place/ratsnest.ts';
import { isConnectedSignalName } from '#utils/signals.ts';
import { createPlacementReport } from '../pcb-auto-place/placement-report.ts';
import { boardSpacingPenalty, boardSpacingPolicy } from './board-spacing.ts';
import { globalPostPlaceScore } from './post-place-refiner.ts';
import { encodeNativePostPlaceScoreProblem } from './native/encode-post-place-score.ts';
import { encodeNativePostPlaceRefineProblem } from './native/encode-post-place-refine.ts';
import { loadNativeBoardPacker } from './native/load-native-board-packer.ts';
import type { PlacementPrimitive, PrimitiveSolveDiagnostic } from './primitives.ts';

/** Conservative finishing pass, in mm. Similarity never attracts distant blocks. */
export const BOARD_ALIGNMENT_POLICY = Object.freeze({ similarity: .78, neighbourGap: 8, maxShift: 4, passes: 2 });
const ALIGNMENT_SCORE = Object.freeze({weight:8,tolerance:.15,orientationWeight:24});

function family(c: PcbComponent): string {
    if (c.pcb.role === 'main_ic') return `IC${c.pins.length}`;
    return c.designator.match(/^[A-Za-z]+/)?.[0].toUpperCase() ?? c.pcb.role;
}
const area = (c: PcbComponent) => c.footprint.width * c.footprint.height;
const weight = (key: string) => key.startsWith('IC') ? 4 : key === 'L' ? 3 : 1;
function histogram(keys: string[]) {
    const result = new Map<string, number>();
    for (const key of keys) result.set(key, (result.get(key) ?? 0) + 1);
    return result;
}
function dice(a: Map<string, number>, b: Map<string, number>, weighted = false) {
    let shared = 0, total = 0;
    for (const key of new Set([...a.keys(), ...b.keys()])) {
        const w = weighted ? weight(key) : 1;
        shared += w * Math.min(a.get(key) ?? 0, b.get(key) ?? 0);
        total += w * ((a.get(key) ?? 0) + (b.get(key) ?? 0));
    }
    return total ? 2 * shared / total : 1;
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
    const sorted = [...components].sort((a, b) => area(b) - area(a) || a.designator.localeCompare(b.designator));
    if (!sorted.length) return undefined;
    if (sorted.length === 1 || area(sorted[0]) >= 1.5 * area(sorted[1])) return sorted[0].designator;
    // When no footprint dominates, prefer a substantial IC; otherwise the block
    // bbox center is less arbitrary than choosing one of many equal passives.
    return sorted.find(c => c.pcb.role === 'main_ic' && area(c) >= .7 * area(sorted[0]))?.designator;
}

export function blockSimilarity(a: PcbComponent[], b: PcbComponent[]): number {
    if (!a.length || !b.length) return 0;
    const aa = alignmentAnchor(a), ba = alignmentAnchor(b);
    if (Boolean(aa) !== Boolean(ba)) return 0;
    let shape = 1;
    if (aa && ba) {
        const x = a.find(c => c.designator === aa)!, y = b.find(c => c.designator === ba)!;
        if (family(x) !== family(y)) return 0;
        const dims = (c: PcbComponent) => [c.footprint.width, c.footprint.height].sort((a,b) => a-b);
        const xd = dims(x), yd = dims(y);
        shape = Math.min(xd[0], yd[0]) / Math.max(xd[0], yd[0], .001)
            * Math.min(xd[1], yd[1]) / Math.max(xd[1], yd[1], .001);
        if (shape < .5) return 0;
    }
    const x = signature(a), y = signature(b);
    return .55 * dice(x.nodes, y.nodes, true) + .3 * dice(x.edges, y.edges) + .15 * shape;
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
        return { ...root, placements: poses, bbox, width: bbox.right-bbox.left, height: bbox.bottom-bbox.top };
    });
}

/** Direction belongs to a main IC even when an inductor or resistor is the
 * largest footprint. Pin count distinguishes a core from small support ICs. */
export function orientationAnchor(components: PcbComponent[]): PcbComponent | undefined {
    return [...components].filter(c=>c.pcb.role==='main_ic').sort((a,b)=>b.pins.length-a.pins.length || area(b)-area(a) || a.designator.localeCompare(b.designator))[0]
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
        const ps=[...groups].sort(([a],[b])=>a.localeCompare(b)).map(([key,ps])=>({key,x:ps.reduce((s,p)=>s+p.x,0)/ps.length,y:ps.reduce((s,p)=>s+p.y,0)/ps.length}));
        const cx=ps.reduce((s,p)=>s+p.x,0)/ps.length,cy=ps.reduce((s,p)=>s+p.y,0)/ps.length;
        return ps.map(p=>({...p,x:p.x-cx,y:p.y-cy}));
    };
    const ap=pattern(a),bp=pattern(b);
    if(ap.length<2 || ap.length!==bp.length || ap.some((p,i)=>p.key!==bp[i].key))return undefined;
    const span=Math.max(...ap.map(p=>Math.hypot(p.x,p.y)),...bp.map(p=>Math.hypot(p.x,p.y)));
    if(span<.01)return undefined;
    const matches=[0,90,180,270].map(angle=>{
        const t=angle*Math.PI/180,c=Math.cos(t),s=Math.sin(t);
        const rotated=ap.map(p=>({x:p.x*c-p.y*s,y:p.x*s+p.y*c}));
        // Libraries may choose different row spacing for the same package. Fit
        // modest positive axis scales; never permit reflection or pin remapping.
        const scale=(axis:'x'|'y')=>{
            const from=Math.max(...rotated.map(p=>Math.abs(p[axis]))),to=Math.max(...bp.map(p=>Math.abs(p[axis])));
            return from<.01&&to<.01?1:to/Math.max(.0001,from);
        };
        const sx=scale('x'),sy=scale('y');
        return {angle,error:sx<.8||sx>1.25||sy<.8||sy>1.25?Infinity:
            Math.max(...rotated.map((p,i)=>Math.hypot(p.x*sx-bp[i].x,p.y*sy-bp[i].y)))};
    }).sort((a,b)=>a.error-b.error||a.angle-b.angle);
    return matches[0].error<=Math.max(.05,span*.03)?matches[0].angle:undefined;
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
    return pairs.sort((a,b) => b.similarity-a.similarity || a.a.localeCompare(b.a) || a.b.localeCompare(b.b));
}
export interface BoardSoftAlignment { pairs: AlignmentPair[]; weight: number; tolerance: number; orientationWeight?:number }
export function boardAlignmentPolicy(input: PlacementInput, roots: PlacementPrimitive[]): BoardSoftAlignment {
    return { pairs: findAlignmentPairs(input,roots,roots.flatMap(p=>p.placements),false), ...ALIGNMENT_SCORE };
}
/** Zero at alignment, quadratic near the axis and linear beyond 1mm. No distance
 * attenuation: moving a pair apart cannot erase its misalignment penalty. */
export function boardAlignmentScore(roots: PlacementPrimitive[], policy: BoardSoftAlignment) {
    let score=0;
    for(const pair of policy.pairs) {
        const a=roots.find(p=>p.id===pair.a), b=roots.find(p=>p.id===pair.b);
        if(!a||!b)continue;
        const ac=center(a,pair.anchorA), bc=center(b,pair.anchorB);
        const error=Math.max(0,Math.min(Math.abs(ac.x-bc.x),Math.abs(ac.y-bc.y))-policy.tolerance);
        score+=policy.weight*pair.similarity*(error<=1?error*error/2:error-.5);
        const angle=orientationError(a,b,pair);
        if(angle!==undefined)score+=(policy.orientationWeight??0)*pair.similarity*(1-Math.cos(angle*Math.PI/180))/2;
    }
    return score;
}
function orientationError(a:PlacementPrimitive,b:PlacementPrimitive,pair:AlignmentPair):number|undefined {
    const o=pair.orientation;if(!o)return undefined;
    const ap=a.placements.find(p=>p.designator===o.a),bp=b.placements.find(p=>p.designator===o.b);
    if(!ap||!bp)return undefined;
    const angle=((ap.rotate-bp.rotate-o.offset)%360+360)%360;
    return Math.min(angle,360-angle);
}
function center(root: PlacementPrimitive, anchor?: string): Point {
    return root.placements.find(p => p.designator === anchor) ?? boxCenter(root.bbox);
}
export function alignmentErrors(roots: PlacementPrimitive[], pairs: AlignmentPair[]) {
    return pairs.map(pair => {
        const a = center(roots.find(r => r.id === pair.a)!, pair.anchorA);
        const b = center(roots.find(r => r.id === pair.b)!, pair.anchorB);
        return { ...pair, error: Math.min(Math.abs(a.x-b.x), Math.abs(a.y-b.y)),
            orientationError:orientationError(roots.find(r=>r.id===pair.a)!,roots.find(r=>r.id===pair.b)!,pair) };
    });
}
function penalty(roots: PlacementPrimitive[], pairs: AlignmentPair[]) {
    return boardAlignmentScore(roots, {pairs,...ALIGNMENT_SCORE});
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
        score:loadNativeBoardPacker().scorePostPlace({...problem,distances:[],clearances:[],fixedPenalties:[],edges:[],paths:[]}),
        lengths:problem.nets.map(net=>minimumSpanningEdges(net.points).reduce((s,[a,b])=>
            s+Math.hypot(net.points[a].x-net.points[b].x,net.points[a].y-net.points[b].y),0)),
    };
}
export function boardElectricalRegression(before: ReturnType<typeof boardElectricalQuality>, after: ReturnType<typeof boardElectricalQuality>) {
    if(after.score>before.score+Math.min(40,.001*Math.abs(before.score)))return 'wiring score';
    const beforeLength=before.lengths.reduce((sum,n)=>sum+n,0);
    const afterLength=after.lengths.reduce((sum,n)=>sum+n,0);
    if(afterLength>beforeLength+Math.max(.5,.005*beforeLength))return 'total net length';
    if(after.lengths.some((n,i)=>n>before.lengths[i]+Math.max(.5,.25*before.lengths[i])))return 'individual net length';
    return undefined;
}

export function refineBoardAlignment(input: PlacementInput, roots: PlacementPrimitive[], initial: Placement[]) {
    let placements = initial;
    let current = currentRoots(input, roots, placements);
    const pairs = findAlignmentPairs(input, roots, initial);
    const before = alignmentErrors(current, pairs);
    const diagnostics: PrimitiveSolveDiagnostic[] = [];
    const rejected = { objective: 0, electrical: 0, netLength: 0, geometry: 0, hardHint: 0, native: 0 };
    const moves: Array<{ blocks: string[]; axis: 'x' | 'y'; shifts: number[]; perpendicularShifts: number[]; scoreBefore: number; scoreAfter: number }> = [];
    if (!pairs.length) return { placements, pairs, before, after: before, moves, diagnostics, evaluated: 0, rejected };
    const addon = loadNativeBoardPacker();
    const constraints = encodeNativePostPlaceRefineProblem(input, initial, 1);
    const spacing = boardSpacingPolicy(input).gap;
    const baseline = new Map(initial.map(p => [p.designator, p]));
    const baselineReport = createPlacementReport(input,initial);
    const movable = (root: PlacementPrimitive) => !root.locked && !root.anchored && root.placements.every(p => {
        const c = input.components.find(c => c.designator === p.designator)!;
        return !c.pcb.fixedPlacement && !c.pcb.edgeMount && !c.pcb.edgePlace;
    });
    const electrical = (ps: Placement[]) => boardElectricalQuality(input,ps);
    const baseElectrical = electrical(initial);
    const objective = (ps: Placement[], rs: PlacementPrimitive[]) => globalPostPlaceScore(input, ps)
        + boardSpacingPenalty(input, rs, spacing) + penalty(rs, pairs);
    let score = objective(placements, current), evaluated = 0;
    for (let pass = 0; pass < BOARD_ALIGNMENT_POLICY.passes; pass++) {
        let accepted = false;
        for (const pair of pairs) {
            const a = current.find(r => r.id === pair.a)!, b = current.find(r => r.id === pair.b)!;
            const ac = center(a,pair.anchorA), bc = center(b,pair.anchorB);
            let best: { ps: Placement[]; rs: PlacementPrimitive[]; score: number; axis: 'x'|'y'; shifts: number[]; perpendicularShifts: number[] } | undefined;
            for (const axis of ['x','y'] as const) {
                const delta = bc[axis]-ac[axis];
                if (Math.abs(delta) < .001 || Math.abs(delta) > 2*BOARD_ALIGNMENT_POLICY.maxShift) continue;
                const axisShifts = [1,.5,.25].flatMap(fraction => [[1,0],[0,-1],[.5,-.5]].map(shares=>shares.map(s=>s*delta*fraction)));
                // Alignment need not happen at either existing axis or exactly
                // halfway. Pad crossings and neighbouring blocks make small
                // changes of the shared axis meaningful. Keep the old moves and
                // add exact shared axes across the bounded interval.
                const grid=Math.max(.5,input.solverOptions.placementGridStep??.5);
                for(let target=Math.min(ac[axis],bc[axis]);target<=Math.max(ac[axis],bc[axis]);target+=grid)
                    axisShifts.push([target-ac[axis],target-bc[axis]]);
                const seenShifts=new Set<string>();
                for (const shifts of axisShifts)
                for (const perpendiculars of [[0,0],[-.5,-.5],[.5,.5],[-1,-1],[1,1],[0,-.5],[0,.5],[-.5,0],[.5,0],[0,-1],[0,1],[-1,0],[1,0]]) {
                    const key=[...shifts,...perpendiculars].map(n=>n.toFixed(3)).join(',');
                    if(seenShifts.has(key))continue;
                    seenShifts.add(key);
                    if (shifts.some((d,i) => (Math.abs(d) > .00001 || perpendiculars[i] !== 0) && !movable(i===0?a:b))) continue;
                    const offsets = new Map([...a.placements.map(p => [p.designator,[shifts[0],perpendiculars[0]]] as const),
                        ...b.placements.map(p => [p.designator,[shifts[1],perpendiculars[1]]] as const)]);
                    const other = axis === 'x' ? 'y' : 'x';
                    const ps = placements.map(p => offsets.get(p.designator)?.some(d=>d!==0) ? { ...p,
                        [axis]: Math.round((p[axis]+offsets.get(p.designator)![0])*1000)/1000,
                        [other]: Math.round((p[other]+offsets.get(p.designator)![1])*1000)/1000 } : p);
                    if (ps.some(p => Math.hypot(p.x-baseline.get(p.designator)!.x,p.y-baseline.get(p.designator)!.y) > BOARD_ALIGNMENT_POLICY.maxShift+.001)) continue;
                    evaluated++;
                    const rs = currentRoots(input, roots, ps), next = objective(ps,rs);
                    if (next >= (best?.score ?? score)-1e-6 || penalty(rs,pairs) >= penalty(current,pairs)-1e-6) { rejected.objective++; continue; }
                    const e = electrical(ps);
                    const regression=boardElectricalRegression(baseElectrical,e);
                    if (regression==='wiring score') { rejected.electrical++; continue; }
                    if (regression) { rejected.netLength++; continue; }
                    const report = createPlacementReport(input,ps);
                    if (!report.ok) { rejected.geometry++; continue; }
                    if (!alignmentHardHintsNoWorse(baselineReport,report)) { rejected.hardHint++; continue; }
                    if (!addon.validatePlacementChange(constraints,ps)) { rejected.native++; continue; }
                    best = { ps,rs,score: next,axis,shifts,perpendicularShifts:perpendiculars };
                }
            }
            if (!best) continue;
            moves.push({ blocks: [a.label,b.label], axis: best.axis, shifts: best.shifts, perpendicularShifts: best.perpendicularShifts, scoreBefore: score, scoreAfter: best.score });
            placements = best.ps; current = best.rs; score = best.score; accepted = true;
        }
        if (!accepted) break;
    }
    diagnostics.push({ severity: 'warning', nodeId: 'board-alignment', message:
        `Board soft alignment: similarity >= ${BOARD_ALIGNMENT_POLICY.similarity}, ${pairs.length} nearby pairs, ${evaluated} candidates, ${moves.length} accepted; fixed components and block interiors preserved` });
    return { placements, pairs, before, after: alignmentErrors(current,pairs), moves, diagnostics, evaluated, rejected };
}
