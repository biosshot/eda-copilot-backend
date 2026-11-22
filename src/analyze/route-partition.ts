import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
export type Point = [
    number,
    number
];
export interface RouteRun {
    id: string;
    sourcePath: string;
    sourcePaths: string[];
    layer: string;
    regionIds: string[];
    from: Point;
    to: Point;
    direction: Point;
    lengthMm: number;
    widthMm: number;
}
export interface RouteGraph {
    runs: RouteRun[];
    contacts: AnalysisGeometryReport['contacts'];
    explicitRegionIds: string[];
    couplingPairs: {
        a: string;
        b: string;
        distanceMm: number;
        reason: string;
    }[];
}
const dot = (a: Point, b: Point) => a[0] * b[0] + a[1] * b[1];
export const project = (p: Point, d: Point) => dot(p, d);
export const tangent = (d: Point): Point => [-d[1], d[0]];
export function area(p: Point[]) { return Math.abs(p.reduce((s, a, i) => { const b = p[(i + 1) % p.length]; return s + a[0] * b[1] - a[1] * b[0]; }, 0)) / 2; }
/** Convex-domain clipping only; the original source polygon stays in the native
 * request. This routine partitions domains, never approximates copper. */
export function clipHalfPlane(p: Point[], normal: Point, at: number, keepLow = true): Point[] {
    const result: Point[] = [];
    for (let i = 0; i < p.length; i++) {
        const a = p[i], b = p[(i + 1) % p.length], da = (dot(a, normal) - at) * (keepLow ? 1 : -1), db = (dot(b, normal) - at) * (keepLow ? 1 : -1);
        if (da <= 0)
            result.push(a);
        if ((da < 0 && db > 0) || (da > 0 && db < 0)) {
            const t = da / (da - db);
            result.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    return result.filter((p, i) => i === 0 || Math.hypot(p[0] - result[i - 1][0], p[1] - result[i - 1][1]) > 1e-10);
}
function segmentDistance(a: Point, b: Point, c: Point, d: Point) {
    const pointDistance = (p: Point, x: Point, y: Point) => { const v: Point = [y[0] - x[0], y[1] - x[1]], q: Point = [p[0] - x[0], p[1] - x[1]], t = Math.max(0, Math.min(1, dot(q, v) / dot(v, v))); return Math.hypot(q[0] - t * v[0], q[1] - t * v[1]); };
    const cross = (x: Point, y: Point, z: Point) => (y[0] - x[0]) * (z[1] - x[1]) - (y[1] - x[1]) * (z[0] - x[0]);
    if (cross(a, b, c) * cross(a, b, d) <= 0 && cross(c, d, a) * cross(c, d, b) <= 0 && Math.max(Math.min(a[0], b[0]), Math.min(c[0], d[0])) <= Math.min(Math.max(a[0], b[0]), Math.max(c[0], d[0])) && Math.max(Math.min(a[1], b[1]), Math.min(c[1], d[1])) <= Math.min(Math.max(a[1], b[1]), Math.max(c[1], d[1])))
        return 0;
    return Math.min(pointDistance(a, c, d), pointDistance(b, c, d), pointDistance(c, a, b), pointDistance(d, a, b));
}
/** Authored line segments are witnesses for a direction, never replacements for
 * captured polygons. Reliable native region contacts preserve branches/vias. */
export function buildRouteGraph(board: any, g: AnalysisGeometryReport, selectedRegionIds: Set<string>, environmentMm: number): RouteGraph {
    const runs: RouteRun[] = [];
    for (const kind of ['fixed', 'editable'])
        for (const [index, track] of (board.copper?.[kind]?.tracks ?? []).entries()) {
            const sourcePath = `copper.${kind}.tracks[${index}]`;
            const regions = g.regions.filter(r => selectedRegionIds.has(r.id) && r.source.path === sourcePath && r.layer === track.layer);
            if (!regions.length)
                continue;
            for (let i = 1; i < (track.points?.length ?? 0); i++) {
                const a = track.points[i - 1], b = track.points[i], lengthMm = Math.hypot(b.x - a.x, b.y - a.y);
                if (!(lengthMm > 0) || !(track.widthMm > 0))
                    continue;
                let from: Point = [a.x, a.y], to: Point = [b.x, b.y];
                if (to[0] < from[0] || to[0] === from[0] && to[1] < from[1])
                    [from, to] = [to, from];
                runs.push({ id: `${sourcePath}/segment/${i - 1}`, sourcePath, sourcePaths: [sourcePath], layer: track.layer, regionIds: regions.map(r => r.id), from, to, direction: [(to[0] - from[0]) / lengthMm, (to[1] - from[1]) / lengthMm], lengthMm, widthMm: track.widthMm });
            }
        }
    // Source file subdivision is not a physical junction. Merge only degree-2
    // collinear authored runs whose connectivity is confirmed by the importer.
    const samePoint=(a:Point,b:Point)=>a[0]===b[0]&&a[1]===b[1];
    for(let pass=0,maxPasses=runs.length;pass<maxPasses;pass++){let merged=false;
        for(let i=0;i<runs.length&&!merged;i++)for(let j=0;j<runs.length&&!merged;j++){
            if(i===j)continue;const a=runs[i],b=runs[j];
            if(a.layer!==b.layer||a.widthMm!==b.widthMm||!samePoint(a.to,b.from)||Math.abs(a.direction[0]*b.direction[1]-a.direction[1]*b.direction[0])>1e-12||dot(a.direction,b.direction)<0)continue;
            if(runs.filter(r=>r.layer===a.layer&&(samePoint(r.from,a.to)||samePoint(r.to,a.to))).length!==2)continue;
            const connected=a.regionIds.some(id=>b.regionIds.includes(id))||(g.islands??[]).some(island=>a.regionIds.some(id=>island.region_ids.includes(id))&&b.regionIds.some(id=>island.region_ids.includes(id)));
            if(!connected)continue;
            a.to=b.to;a.lengthMm=Math.hypot(a.to[0]-a.from[0],a.to[1]-a.from[1]);a.direction=[(a.to[0]-a.from[0])/a.lengthMm,(a.to[1]-a.from[1])/a.lengthMm];a.regionIds=[...new Set([...a.regionIds,...b.regionIds])];a.sourcePaths=[...new Set([...a.sourcePaths,...b.sourcePaths])].sort();a.id+='+'+b.id;runs.splice(j,1);merged=true;
        }
        if(!merged)break;
    }
    const recognized = new Set(runs.flatMap(r => r.regionIds));
    const couplingPairs: RouteGraph['couplingPairs'] = [];
    for (let i = 0; i < runs.length; i++)
        for (let j = 0; j < i; j++) {
            const a = runs[j], b = runs[i], distanceMm = Math.max(0, segmentDistance(a.from, a.to, b.from, b.to) - (a.widthMm + b.widthMm) / 2);
            if (distanceMm <= environmentMm)
                couplingPairs.push({ a: a.id, b: b.id, distanceMm, reason: 'Spatial near interaction, including remote route-order meanders/crossings; retain joint extraction' });
        }
    return { runs, contacts: g.contacts.filter(c => c.reliable && selectedRegionIds.has(c.a) && selectedRegionIds.has(c.b)), explicitRegionIds: [...selectedRegionIds].filter(id => !recognized.has(id)), couplingPairs };
}
export interface RouteCell {
    id: string;
    polygonMm: Point[];
    direction: Point;
    routeIds: string[];
}
/** Triangulation for exact integration of simple (possibly concave) domains. */
export function triangles(p: Point[]): Point[][] {
    const indices = p.map((_, i) => i), out: Point[][] = [];
    const cross = (a: Point, b: Point, c: Point) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    while (indices.length > 3) {
        let found = false;
        for (let i = 0; i < indices.length; i++) {
            const a = p[indices[(i + indices.length - 1) % indices.length]], b = p[indices[i]], c = p[indices[(i + 1) % indices.length]];
            if (cross(a, b, c) <= 1e-12)
                continue;
            if (indices.some(j => p[j] !== a && p[j] !== b && p[j] !== c && cross(a, b, p[j]) >= -1e-12 && cross(b, c, p[j]) >= -1e-12 && cross(c, a, p[j]) >= -1e-12))
                continue;
            out.push([a, b, c]);
            indices.splice(i, 1);
            found = true;
            break;
        }
        if (!found)
            throw new Error('CHANNEL_SEGMENTATION: Non-simple or degenerate joint domain');
    }
    if (indices.length === 3)
        out.push(indices.map(i => p[i]));
    return out;
}
export function joinDomains(a: Point[], b: Point[]): Point[] {
    const points = [...a, ...b], edges = new Map<string, [
        Point,
        Point
    ]>(), key = (p: Point) => p.map(v => Math.round(v * 1e8)).join(',');
    for (const polygon of [a, b])
        for (let i = 0; i < polygon.length; i++) {
            const p = polygon[i], q = polygon[(i + 1) % polygon.length], v: Point = [q[0] - p[0], q[1] - p[1]], l = dot(v, v);
            const stops = points.map(x => ({ p: x, t: dot([x[0] - p[0], x[1] - p[1]], v) / l })).filter(x => x.t >= -1e-10 && x.t <= 1 + 1e-10 && Math.abs((x.p[0] - p[0]) * v[1] - (x.p[1] - p[1]) * v[0]) < 1e-8).sort((x, y) => x.t - y.t);
            for (let j = 1; j < stops.length; j++) {
                const x = stops[j - 1].p, y = stops[j].p;
                if (key(x) === key(y))
                    continue;
                const forward = key(x) + '/' + key(y), reverse = key(y) + '/' + key(x);
                if (edges.has(reverse))
                    edges.delete(reverse);
                else
                    edges.set(forward, [x, y]);
            }
        }
    const all = [...edges.values()];
    if (!all.length)
        throw new Error('CHANNEL_SEGMENTATION: Empty joint domain');
    const result: Point[] = [all[0][0]], start = key(all[0][0]);
    let current = all[0][0];
    for (let i = 0; i <= all.length; i++) {
        const edge = [...edges.entries()].find(([, e]) => key(e[0]) === key(current));
        if (!edge)
            throw new Error('CHANNEL_SEGMENTATION: Disconnected joint domain boundary');
        edges.delete(edge[0]);
        current = edge[1][1];
        if (key(current) === start) {
            if (edges.size)
                throw new Error('CHANNEL_SEGMENTATION: Joint domain would have a hole');
            return result;
        }
        result.push(current);
    }
    throw new Error('CHANNEL_SEGMENTATION: Joint boundary did not close');
}
