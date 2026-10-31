import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
import { project, tangent, clipHalfPlane, area, type Point, type RouteRun, type RouteCell } from './route-partition.ts';
export interface CorridorEvent {
    kind: 'copper' | 'material' | 'bore';
    source: string;
    startMm: number;
    stopMm: number;
}
export interface WideReturnDomain {
    intervalMm: [number, number];
    polygonMm: Point[];
    transverseMm: [number, number];
    inspectedTransverseMm: [number, number];
    lateralBoundary: 'numerical-cut-not-physical-edge';
    electricalAdmission: 'requires-boundary-expansion';
    requiredChecks: string[];
    sources: { regionId: string; sourcePath: string; layer: string; islandId?: string;
        disposition: 'potentially-intersects-domain' | 'outside-domain-retained';
        polygon: AnalysisGeometryReport['regions'][number]['polygon'] }[];
}
export interface RunCorridor {
    geometricEvents: CorridorEvent[];
    geometricIntervalsMm: [number, number][];
    wideReturnDomains: WideReturnDomain[];
    runId: string;
    direction: Point;
    polygonMm: Point[];
    longitudinalMm: [
        number,
        number
    ];
    transverseMm: [
        number,
        number
    ];
    events: CorridorEvent[];
    uniformIntervalsMm: [
        number,
        number
    ][];
    admissibleIntervalsMm: [
        number,
        number
    ][];
    lateralContinuations: {
        classification?: 'uniform-continuing-support' | 'joint-event-required';
        islandId?: string;
        regionId: string;
        sourcePath: string;
        layer: string;
        sideMm: number;
        lengthMm: number;
        startMm: number;
        stopMm: number;
    }[];
}
const diff = (a: Point, b: Point): Point => [a[0] - b[0], a[1] - b[1]];
const point = (d: Point, t: Point, x: number, y: number): Point => [x * d[0] + y * t[0], x * d[1] + y * t[1]];
/** Exact segment/convex-window intersection. Events cover complete boundary
 * segments; narrow features between sampled native slices cannot escape it. */
export function segmentWindow(a: Point, b: Point, polygon: Point[]): [
    Point,
    Point
] | undefined {
    let low = 0, high = 1;
    const v = diff(b, a);
    for (let i = 0; i < polygon.length; i++) {
        const p = polygon[i], q = polygon[(i + 1) % polygon.length], n: Point = [q[1] - p[1], p[0] - q[0]], velocity = project(v, n), gap = project(diff(p, a), n);
        if (Math.abs(velocity) < 1e-15) {
            if (gap < -1e-10)
                return;
            continue;
        }
        if (velocity > 0)
            high = Math.min(high, gap / velocity);
        else
            low = Math.max(low, gap / velocity);
    }
    if (high - low <= 1e-10)
        return;
    return [[a[0] + low * v[0], a[1] + low * v[1]], [a[0] + high * v[0], a[1] + high * v[1]]];
}
function inRing(ring: number[][], p: Point) { let yes = false; for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const a = ring[i], b = ring[j];
    if ((a[1] > p[1]) !== (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0])
        yes = !yes;
} return yes; }
/** Corridors are proposals only. Lateral physical copper continuation is an
 * explicit unsupported distributed interface, never an insulated plane cut. */
function proposeCore(g: AnalysisGeometryReport, runs: RouteRun[], halfWidthMm: number): RunCorridor[] {
    return runs.map(run => {
        const d = run.direction, t = tangent(d), a = project(run.from, d), b = project(run.to, d), centre = project(run.from, t), cross: [
            number,
            number
        ] = [centre - halfWidthMm, centre + halfWidthMm], polygon: Point[] = [point(d, t, a, cross[0]), point(d, t, b, cross[0]), point(d, t, b, cross[1]), point(d, t, a, cross[1])], events: CorridorEvent[] = [], lateralContinuations: RunCorridor['lateralContinuations'] = [];
        const rings: {
            ring: number[][];
            kind: CorridorEvent['kind'];
            source: string;
        }[] = [];
        for (const layer of g.layers)
            for (const polygon of layer.copper)
                for (const ring of [polygon.outer, ...polygon.holes])
                    rings.push({ ring, kind: 'copper', source: 'physical-layer-union:' + layer.layer });
        if (g.outline)
            for (const ring of [g.outline.outer, ...g.outline.holes])
                rings.push({ ring, kind: 'material', source: 'board-outline/cutout' });
        for (const h of g.holes)
            for (const p of h.polygons)
                for (const ring of [p.outer, ...p.holes])
                    rings.push({ ring, kind: 'bore', source: h.source.path });
        const epsilon = Math.max(1e-8, 2 * g.boolean_error_bound_mm);
        for (const { ring, kind, source } of rings)
            for (let i = 0; i < ring.length; i++) {
                const originalA = ring[i] as Point, originalB = ring[(i + 1) % ring.length] as Point, clipped = segmentWindow(originalA, originalB, polygon);
                if (!clipped)
                    continue;
                if (kind !== 'bore' && Math.abs(project(diff(originalB, originalA), t)) <= epsilon)
                    continue;
                const x = clipped.map(p => project(p, d));
                events.push({ kind, source, startMm: Math.max(a, Math.min(...x) - epsilon), stopMm: Math.min(b, Math.max(...x) + epsilon) });
            }
        for (const side of cross)
            for (const r of g.regions) {
                const cuts = [a, b];
                for (const ring of [r.polygon.outer, ...r.polygon.holes])
                    for (let i = 0; i < ring.length; i++) {
                        const p = ring[i] as Point, q = ring[(i + 1) % ring.length] as Point, x = project(p, t) - side, y = project(q, t) - side;
                        if ((x <= 0 && y > 0) || (y <= 0 && x > 0)) {
                            const at = project(p, d) + (project(q, d) - project(p, d)) * (-x) / (y - x);
                            if (at > a && at < b)
                                cuts.push(at);
                        }
                    }
                cuts.sort((x, y) => x - y);
                let lengthMm = 0;
                for (let i = 1; i < cuts.length; i++) {
                    const p = point(d, t, (cuts[i - 1] + cuts[i]) / 2, side);
                    if (inRing(r.polygon.outer, p) && !r.polygon.holes.some(h => inRing(h, p))) {
                        lengthMm += cuts[i] - cuts[i - 1];
                        lateralContinuations.push({ regionId: r.id, sourcePath: r.source.path, layer: r.layer, sideMm: side, lengthMm: cuts[i] - cuts[i - 1], startMm: cuts[i - 1], stopMm: cuts[i] });
                    }
                }
            }
        const merged: [
            number,
            number
        ][] = [];
        for (const e of events.sort((a, b) => a.startMm - b.startMm)) {
            const old = merged.at(-1);
            if (old && e.startMm <= old[1])
                old[1] = Math.max(old[1], e.stopMm);
            else
                merged.push([e.startMm, e.stopMm]);
        }
        const uniformIntervalsMm: [
            number,
            number
        ][] = [];
        let cursor = a;
        for (const [lo, hi] of merged) {
            if (lo - cursor > epsilon)
                uniformIntervalsMm.push([cursor, lo]);
            cursor = Math.max(cursor, hi);
        }
        if (b - cursor > epsilon)
            uniformIntervalsMm.push([cursor, b]);
        const admissibleIntervalsMm = uniformIntervalsMm.filter(([a, b]) => !lateralContinuations.some(v => Math.min(b, v.stopMm) - Math.max(a, v.startMm) > epsilon));
        return { geometricEvents: [], geometricIntervalsMm: [], wideReturnDomains: [], runId: run.id, direction: d, polygonMm: polygon, longitudinalMm: [a, b], transverseMm: cross, events, uniformIntervalsMm, admissibleIntervalsMm, lateralContinuations };
    });
}
/** Finite transverse physical-union sweep. Complete boundary events inside
 * this numerical candidate split intervals; external geometry remains in sources. CAD source seams are absent
 * from layers[].copper. No source polygon is used as the numerical domain.
 * The production admission list remains guarded separately. */
export function proposeCorridors(g: AnalysisGeometryReport, runs: RouteRun[], halfWidthMm: number): RunCorridor[] {
    if (!(halfWidthMm > 0 && Number.isFinite(halfWidthMm))) throw Error('CHANNEL_CORRIDOR: Invalid width');
    const proposals = proposeCore(g, runs, halfWidthMm);
    return proposals.map((c, index) => {
        const run = runs[index], d = run.direction, t = tangent(d), centre = project(run.from, t);
        const extent = halfWidthMm;
        const inspected = proposeCore(g, [run], extent)[0];
        // Exact complete-boundary sweep, not agreement at a handful of slices.
        c.geometricIntervalsMm = inspected.uniformIntervalsMm;
        c.geometricEvents = inspected.events;
        for (const v of c.lateralContinuations) {
            v.islandId = g.islands?.find(i => i.region_ids.includes(v.regionId))?.id;
            v.classification = inspected.events.some(e => Math.min(v.stopMm,e.stopMm)>Math.max(v.startMm,e.startMm))
                ? 'joint-event-required' : 'uniform-continuing-support';
        }
        c.wideReturnDomains = c.geometricIntervalsMm.map(intervalMm => {
            const [a,b] = intervalMm, [lo,hi] = c.transverseMm;
            const polygonMm = [point(d,t,a,lo),point(d,t,b,lo),point(d,t,b,hi),point(d,t,a,hi)];
            const sources = g.regions.map(r => {
                let xmin=Infinity,xmax=-Infinity,ymin=Infinity,ymax=-Infinity;
                for(const p of r.polygon.outer){const x=project(p as Point,d),y=project(p as Point,t);xmin=Math.min(xmin,x);xmax=Math.max(xmax,x);ymin=Math.min(ymin,y);ymax=Math.max(ymax,y);}
                return {regionId:r.id,sourcePath:r.source.path,layer:r.layer,islandId:g.islands?.find(i=>i.region_ids.includes(r.id))?.id,
                    disposition: xmax>a && xmin<b && ymax>lo && ymin<hi ? 'potentially-intersects-domain' as const : 'outside-domain-retained' as const,
                    polygon:r.polygon};
            });
            return {intervalMm,polygonMm,transverseMm:c.transverseMm,inspectedTransverseMm:inspected.transverseMm,
                lateralBoundary:'numerical-cut-not-physical-edge' as const,electricalAdmission:'requires-boundary-expansion' as const,
                requiredChecks:['joint multiconductor transverse extraction','successive lateral field-domain expansion','physical island and return mapping','distributed section/local interface coupling'],sources};
        });
        return c;
    });
}
export function corridorCells(domain: Point[], corridors: RunCorridor[], boundaryFraction = 1 / 8): RouteCell[] {
    let locals: Point[][] = [domain];
    const sections: RouteCell[] = [];
    for (const corridor of corridors) {
        const d = corridor.direction, t = tangent(d);
        for (const [lo, hi] of corridor.admissibleIntervalsMm) {
            // Both contacts lie strictly inside the proven invariant interval. Final
            // admission still requires boundary-displacement numerical controls.
            if (!(boundaryFraction > 0 && boundaryFraction < .5))
                throw new Error('CHANNEL_CORRIDOR: Invalid boundary placement fraction');
            const inset = (hi - lo) * boundaryFraction, a = lo + inset, b = hi - inset;
            if (b - a < 1e-5)
                continue;
            const rectangle = [point(d, t, a, corridor.transverseMm[0]), point(d, t, b, corridor.transverseMm[0]), point(d, t, b, corridor.transverseMm[1]), point(d, t, a, corridor.transverseMm[1])];
            const next: Point[][] = [];
            for (const local of locals) {
                let interior = local;
                for (let i = 0; i < 4; i++) {
                    const p = rectangle[i], q = rectangle[(i + 1) % 4], n: Point = [q[1] - p[1], p[0] - q[0]], at = project(p, n), outside = clipHalfPlane(interior, n, at, false);
                    if (area(outside) > 1e-10)
                        next.push(outside);
                    interior = clipHalfPlane(interior, n, at);
                }
                if (area(interior) > 1e-10) {
                    if (Math.abs(area(interior) - area(rectangle)) > 1e-8)
                        throw new Error('CHANNEL_CORRIDOR: Overlapping corridors require joint event domain');
                    sections.push({ id: 'run-' + sections.length, polygonMm: interior, direction: d, routeIds: [corridor.runId] });
                }
            }
            locals = next;
        }
    }
    if (sections.length + locals.length > 32)
        throw new Error('CHANNEL_CORRIDOR: More than32 explicit domains; no copper discarded');
    const d = corridors[0]?.direction ?? [1, 0] as Point;
    return [...sections, ...locals.map((polygonMm, i) => ({ id: 'event-' + i, polygonMm, direction: d, routeIds: [] }))].sort((a, b) => project(a.polygonMm.reduce((s, p) => [s[0] + p[0] / a.polygonMm.length, s[1] + p[1] / a.polygonMm.length] as Point, [0, 0]), d) - project(b.polygonMm.reduce((s, p) => [s[0] + p[0] / b.polygonMm.length, s[1] + p[1] / b.polygonMm.length] as Point, [0, 0]), d));
}
/** Spatially overlapping parallel runs share ONE transverse extraction. This
 * also catches remote route-order meanders; layer/net names never separate the
 * matrices. Complete boundary events and lateral-continuation guards still apply. */
export function coupledRunCorridors(g:AnalysisGeometryReport,runs:RouteRun[],halfWidthMm:number):RunCorridor[]{
 const groups:RouteRun[][]=runs.map(run=>[run]);
 const near=(a:RouteRun,b:RouteRun)=>{
  if(Math.abs(a.direction[0]*b.direction[1]-a.direction[1]*b.direction[0])>1e-12||project(a.direction,b.direction)<0)return false;
  const d=a.direction,t=tangent(d),x=[project(a.from,d),project(a.to,d)],y=[project(b.from,d),project(b.to,d)];
  return Math.min(x[1],y[1])-Math.max(x[0],y[0])>1e-9&&Math.abs(project(a.from,t)-project(b.from,t))<2*halfWidthMm;
 };
 for(let pass=0,maxPasses=groups.length;pass<maxPasses;pass++){let merged=false;for(let i=0;i<groups.length&&!merged;i++)for(let j=0;j<i&&!merged;j++)if(groups[i].some(a=>groups[j].some(b=>near(a,b)))){groups[i].push(...groups[j]);groups.splice(j,1);merged=true;}if(!merged)break;}
 return groups.flatMap(group=>{
  const d=group[0].direction,t=tangent(d),xs=group.flatMap(r=>[project(r.from,d),project(r.to,d)]),ys=group.map(r=>project(r.from,t)),low=Math.min(...xs),high=Math.max(...xs),bottom=Math.min(...ys)-halfWidthMm,top=Math.max(...ys)+halfWidthMm,centre=(bottom+top)/2;
  const run:RouteRun={...group[0],id:group.map(r=>r.id).join('+'),from:point(d,t,low,centre),to:point(d,t,high,centre),lengthMm:high-low,sourcePaths:[...new Set(group.flatMap(r=>r.sourcePaths))].sort(),regionIds:[...new Set(group.flatMap(r=>r.regionIds))]};
  return proposeCorridors(g,[run],(top-bottom)/2);
 });
}
