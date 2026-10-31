import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
import type { ChannelBlock, ChannelCut, CopperFragment, SegmentedChannel } from './segmented-channel.ts';
import { createProblemExecutor } from './execute-problem.ts';
import { capturedBoardGeometry } from './geometry-snapshot.ts';
import { buildRouteGraph, project, tangent, area, clipHalfPlane, triangles, joinDomains, type Point } from './route-partition.ts';
import { coupledRunCorridors, corridorCells } from './run-corridors.ts';
type Options = {
    domainScale?: number;
    magneticPatchLevel?: 1 | 2;
    boundaryFraction?: number;
};
type Face = {
    id: string;
    a: number;
    b: number;
    origin: Point;
    normal: Point;
    span: [
        number,
        number
    ];
    slice?: any;
};
export function boardChannelPlan(board: any, base: any, contacts: {
    sourcePath: string;
    layer: string;
}[], options: Options = {}) { return createBoardChannelPlanner(board, base, contacts)(options); }
const fail = (s: string): never => {
    throw new Error("CHANNEL_SEGMENTATION: " + s);
};
const add = (a: Point, b: Point, s = 1): Point => [a[0] + s * b[0], a[1] + s * b[1]];
const range = (ps: Point[], d: Point) => [Math.min(...ps.map((p) => project(p, d))), Math.max(...ps.map((p) => project(p, d)))];
export function createBoardChannelPlanner(board: any, base2: any, contacts: {
    sourcePath: string;
    layer: string;
}[]) {
    const execute = createProblemExecutor(true), boardJson = JSON.stringify(board), snapshot = JSON.parse(boardJson);
    const g = capturedBoardGeometry(boardJson);
    return (options: Options = {}): SegmentedChannel & {
        audit: any;
    } => {
        const island = (c: {
            sourcePath: string;
            layer: string;
        }) => {
            const r = g.regions.filter((r2) => r2.source.path === c.sourcePath && r2.layer === c.layer);
            if (r.length !== 1)
                fail("Ambiguous physical endpoint");
            return g.islands.find((i) => i.region_ids.includes(r[0].id)) ?? fail("Unconfirmed endpoint");
        };
        const signal = island(contacts[0]);
        if (signal.id !== island(contacts[1]).id)
            fail("Disconnected signal endpoints");
        const returns = [...new Map([island(contacts[2]), island(contacts[3])].map((i) => [i.id, i])).values()], returnIds = new Set(returns.flatMap((i) => i.region_ids));
        const selected = g.regions.filter((r) => signal.region_ids.includes(r.id) || contacts.some((c) => c.sourcePath === r.source.path && c.layer === r.layer));
        const height = base2.stack.reduce((s: number, l: any) => s + l.thicknessM * 1e3, 0), margin = Math.max(0.05, 2 * height) * (options.domainScale ?? 1);
        const points = selected.flatMap((r) => r.polygon.outer), xr = range(points, [1, 0]), yr = range(points, [0, 1]);
        const bounds = [[xr[0] - margin, yr[0] - margin], [xr[1] + margin, yr[1] + margin]];
        const wall = base2.finishedHolePlatingM * 1e3 + (base2.curveDeviationMm ?? 1e-3) + 2 * g.boolean_error_bound_mm;
        for (let pass = 0; pass <= g.holes.length; pass++) {
            let changed = false;
            for (const h of g.holes.filter((h2) => h2.plated && h2.span_confirmed)) {
                const ps = h.polygons.flatMap((p) => p.outer);
                if (!ps.length)
                    continue;
                const x = range(ps, [1, 0]), y = range(ps, [0, 1]);
                if (x[0] - wall > bounds[1][0] || x[1] + wall < bounds[0][0] || y[0] - wall > bounds[1][1] || y[1] + wall < bounds[0][1])
                    continue;
                for (let k = 0; k < 2; k++) {
                    const r = k === 0 ? x : y;
                    const lo = Math.min(bounds[0][k], r[0] - wall), hi = Math.max(bounds[1][k], r[1] + wall);
                    changed ||= lo !== bounds[0][k] || hi !== bounds[1][k];
                    bounds[0][k] = lo;
                    bounds[1][k] = hi;
                }
            }
            if (!changed)
                break;
            if (pass === g.holes.length)
                fail("Barrel domain closure did not stabilize");
        }
        let domain: Point[] = [[bounds[0][0], bounds[0][1]], [bounds[1][0], bounds[0][1]], [bounds[1][0], bounds[1][1]], [bounds[0][0], bounds[1][1]]];
        const seed = buildRouteGraph(snapshot, g, new Set(signal.region_ids), 0).runs.sort((a, b) => b.lengthMm - a.lengthMm)[0];
        if (seed) {
            const d = seed.direction, t = tangent(d), x = range(domain, d), y = range(domain, t);
            domain = [[x[0], y[0]], [x[1], y[0]], [x[1], y[1]], [x[0], y[1]]].map(([a, b]) => [a * d[0] + b * t[0], a * d[1] + b * t[1]]);
        }
        const clippedArea = (ring: number[][], polygon: Point[]) => {
            let p = ring as Point[];
            for (let i = 0; i < polygon.length; i++) {
                const a = polygon[i], b = polygon[(i + 1) % polygon.length], v = [b[0] - a[0], b[1] - a[1]], n = [v[1], -v[0]] as Point;
                p = clipHalfPlane(p, n, project(a, n));
            }
            return area(p);
        };
        const regionArea = (r: AnalysisGeometryReport['regions'][number], p: Point[]) => triangles(p).reduce((sum, triangle) => sum + Math.max(0, clippedArea(r.polygon.outer, triangle) - r.polygon.holes.reduce((s, h) => s + clippedArea(h, triangle), 0)), 0);
        const relevant = g.regions.filter((r) => regionArea(r, domain) > 1e-12), graph = buildRouteGraph(snapshot, g, new Set(relevant.map((r) => r.id)), Math.max(0.05, 2 * height));
        const signalRuns = graph.runs.filter((r) => r.regionIds.some((id) => signal.region_ids.includes(id)));
        // Physical run envelopes include mutual conductors; complete material events and lateral continuation decide admission.
        const corridors = coupledRunCorridors(g, signalRuns, margin + Math.max(...signalRuns.map((r) => r.widthMm / 2), 0));
        // A longitudinally uniform slice through an internal return hole is
        // still part of a connected bypass event. Its detached transverse return
        // pieces cannot become independent ribbons with an implied remote join.
        // Use the physical layer union, so CAD source seams do not create events.
        for(const c of corridors){const d=c.direction,t=tangent(d);const gaps=g.layers.flatMap(l=>l.copper.flatMap(p=>p.holes)).flatMap(h=>{
            const x=range(h as Point[],d),y=range(h as Point[],t);
            return y[1]>c.transverseMm[0]&&y[0]<c.transverseMm[1]?[x]:[];
        });
            c.admissibleIntervalsMm=c.admissibleIntervalsMm.filter(([a,b])=>!gaps.some(([x,y])=>Math.min(b,y)-Math.max(a,x)>1e-8));
        }
        const usable = corridors.filter((c) => c.admissibleIntervalsMm.length);
        if (signalRuns.length && !usable.length)
            throw Object.assign(new Error("CHANNEL_CORRIDOR: No admissible finite-return uniform corridor; distributed lateral copper/material events require a joint domain"), { code: "CHANNEL_CORRIDOR_UNSUPPORTED", corridorAudit: { corridors, contacts, sourceCopper: g.regions, physicalIslands: g.islands, physicalContacts: g.contacts, bores: g.holes, signalIsland: signal.id, returnIslands: returns.map((i) => i.id), geometryFindings: g.findings, signalRegions: g.regions.filter((r) => signal.region_ids.includes(r.id)).map((r) => ({ regionId: r.id, sourcePath: r.source.path, layer: r.layer, areaMm2: r.area_mm2 })) } });
        const cells = (signalRuns.length ? corridorCells(domain, usable, options.boundaryFraction) : [{ id: "event-0", polygonMm: domain, direction: [1, 0] as Point, routeIds: [] }]).filter((c) => relevant.some((r) => regionArea(r, c.polygonMm) > 1e-12));
        for (let pass = 0; pass < 32; pass++) {
            let merged = false;
            for (let i = 0; i < cells.length && !merged; i++)
                for (let j = 0; j < i && !merged; j++) {
                    if (/^run-[0-9]+$/.test(cells[i].id) || /^run-[0-9]+$/.test(cells[j].id))
                        continue;
                    try {
                        const polygon = joinDomains(cells[i].polygonMm, cells[j].polygonMm);
                        cells[i].polygonMm = polygon;
                        cells[i].id += "-event";
                        cells.splice(j, 1);
                        merged = true;
                    }
                    catch (error) {
                        if (!(error instanceof Error) || !["Disconnected", "hole", "did not close"].some((s) => error.message.includes(s)))
                            throw error;
                    }
                }
            if (!merged)
                break;
        }
        for (const contact of contacts) {
            const region = g.regions.find((r) => r.source.path === contact.sourcePath && r.layer === contact.layer)!;
            for (let pass = 0; pass < 32; pass++) {
                const occupied = cells.flatMap((c, i) => regionArea(region, c.polygonMm) > 1e-10 ? [i] : []);
                if (occupied.length < 2)
                    break;
                let merged = false;
                for (let i = 0; i < occupied.length && !merged; i++)
                    for (let j = 0; j < i && !merged; j++) {
                        const a = occupied[i], b = occupied[j];
                        try {
                            const polygon = joinDomains(cells[a].polygonMm, cells[b].polygonMm);
                            cells[a].polygonMm = polygon;
                            cells[a].id += "-contact";
                            cells.splice(b, 1);
                            merged = true;
                        }
                        catch (error) {
                            if (!(error instanceof Error) || !error.message.includes("Disconnected"))
                                throw error;
                        }
                    }
                if (!merged)
                    fail("External source fragments require disconnected joint domain");
            }
        }
        let faces: Face[] = [];
        function buildFaces() {
            faces = [];
            for (let i = 0; i < cells.length; i++)
                for (let j = 0; j < i; j++)
                    for (let a = 0; a < cells[i].polygonMm.length; a++)
                        for (let b = 0; b < cells[j].polygonMm.length; b++) {
                            const p = cells[i].polygonMm[a], q = cells[i].polygonMm[(a + 1) % cells[i].polygonMm.length], r = cells[j].polygonMm[b], s = cells[j].polygonMm[(b + 1) % cells[j].polygonMm.length], length = Math.hypot(q[0] - p[0], q[1] - p[1]);
                            const t: Point = [(q[0] - p[0]) / length, (q[1] - p[1]) / length], n = [t[1], -t[0]] as Point;
                            if (Math.abs(project(add(r, p, -1), n)) > 1e-8 || Math.abs(project(add(s, p, -1), n)) > 1e-8)
                                continue;
                            const lo = Math.max(0, Math.min(project(add(r, p, -1), t), project(add(s, p, -1), t))), hi = Math.min(length, Math.max(project(add(r, p, -1), t), project(add(s, p, -1), t)));
                            if (hi - lo > 1e-8)
                                faces.push({ id: "face-" + faces.length, a: i, b: j, origin: add(p, t, (lo + hi) / 2), normal: n, span: [-(hi - lo) / 2, (hi - lo) / 2] });
                        }
        }
        buildFaces();
        const inRing2 = (ring: number[][], p: Point) => {
            let yes = false;
            for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
                const a = ring[i], b = ring[j];
                if (a[1] > p[1] !== b[1] > p[1] && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0])
                    yes = !yes;
            }
            return yes;
        };
        function slice(origin: Point, n: Point, span: [
            number,
            number
        ], id: string) {
            const t = tangent(n), cuts = [span[0], span[1]];
            for (const r of relevant.filter((r2) => returnIds.has(r2.id)))
                for (const ring of [r.polygon.outer, ...r.polygon.holes])
                    for (let i = 0; i < ring.length; i++) {
                        const a = add(ring[i], origin, -1), b = add(ring[(i + 1) % ring.length], origin, -1), x = project(a, n), y = project(b, n);
                        if (x <= 0 && y > 0 || y <= 0 && x > 0) {
                            const v = project(a, t) + (project(b, t) - project(a, t)) * -x / (y - x);
                            if (v > span[0] && v < span[1])
                                cuts.push(v);
                        }
                    }
            cuts.sort((a, b) => a - b);
            let reference2: any, offset: number | undefined;
            for (let i = 1; i < cuts.length && !reference2; i++) {
                const y = (cuts[i - 1] + cuts[i]) / 2, p = add(origin, t, y);
                for (const r of relevant.filter((r2) => returnIds.has(r2.id)))
                    if (inRing2(r.polygon.outer, p) && !r.polygon.holes.some((h) => inRing2(h, p))) {
                        reference2 = r;
                        offset = y;
                        break;
                    }
            }
            if (!reference2)
                fail("No finite return at " + id + "; partition requires a joint/nonlocal return model");
            const value2 = execute({ kind: "board-section", boardJson, stack: base2.stack, slice: { atMm: origin, transverseUnit: t, halfWidthMm: (span[1] - span[0]) / 2, airMarginMm: Math.max(1, 4 * height) * (options.domainScale ?? 1), referenceLayer: reference2.layer, referenceSourcePath: reference2.source.path, referenceXMm: offset } });
            const mapping = value2.sourceMapping, ref = mapping.find((m: any) => m.role === "reference"), signals = mapping.filter((m: any) => m.role !== "reference");
            const key = (m: any) => JSON.stringify({ face: id, layer: m.layer, sources: m.sourcePaths, interval: m.intervalMm });
            return { ...value2, reference: ref, signals, cuts: signals.map((m: any) => ({ positive: key(m), negative: key(ref) })) };
        }
        // Match actual source intervals on one finite face, preserving disconnected returns and floating neighbours.
        function localFace(f: Face) {
            const t = tangent(f.normal), intervals = [];
            for (const r of relevant) {
                const points2 = [f.span[0], f.span[1]];
                for (const ring of [r.polygon.outer, ...r.polygon.holes])
                    for (let i = 0; i < ring.length; i++) {
                        const a = add(ring[i], f.origin, -1), b = add(ring[(i + 1) % ring.length], f.origin, -1), x = project(a, f.normal), y = project(b, f.normal);
                        if (x <= 0 && y > 0 || y <= 0 && x > 0) {
                            const at = project(a, t) + (project(b, t) - project(a, t)) * -x / (y - x);
                            if (at > f.span[0] && at < f.span[1])
                                points2.push(at);
                        }
                    }
                points2.sort((a, b) => a - b);
                for (let i = 1; i < points2.length; i++) {
                    const a = points2[i - 1], b = points2[i], p = add(f.origin, t, (a + b) / 2);
                    if (b - a > 1e-9 && inRing2(r.polygon.outer, p) && !r.polygon.holes.some((h) => inRing2(h, p)))
                        intervals.push({ layer: r.layer, sourcePaths: [r.source.path], regionIds: [r.id], island: g.islands.find((v: any) => v.region_ids.includes(r.id))?.id, intervalMm: [a, b] });
                }
            }
            intervals.sort((a, b) => a.layer.localeCompare(b.layer) || a.intervalMm[0] - b.intervalMm[0]);
            const mapping: any[] = [];
            for (const m of intervals) {
                const old = mapping.at(-1);
                if (old && old.layer === m.layer && old.island === m.island && m.intervalMm[0] <= old.intervalMm[1]) {
                    old.intervalMm[1] = Math.max(old.intervalMm[1], m.intervalMm[1]);
                    old.sourcePaths = [... /* @__PURE__ */new Set([...old.sourcePaths, ...m.sourcePaths])].sort();
                    old.regionIds.push(...m.regionIds);
                }
                else
                    mapping.push(m);
            }
            if (!mapping.length)
                return void 0;
            const reference2 = mapping.find((m: any) => m.regionIds.some((id: string) => returnIds.has(id))) ?? fail("No finite return contact at " + f.id + "; requires joint local domain");
            const signals = mapping.filter((m: any) => m !== reference2), key = (m: any) => JSON.stringify({ face: f.id, layer: m.layer, island: m.island, interval: m.intervalMm });
            if (!signals.length)
                fail("Return-only shared face " + f.id + " requires a joint local domain; no fictitious differential reference");
            return { reference: reference2, signals, sourceMapping: mapping, cuts: signals.map((m: any) => ({ positive: key(m), negative: key(reference2) })) };
        }
        for (let pass = 0; pass < 32; pass++) {
            let merged = false;
            for (const f of faces) {
                try {
                    f.slice = localFace(f);
                }
                catch (error) {
                    if (!(error instanceof Error) || !error.message.includes("Return-only"))
                        throw error;
                    const a = cells[f.a], b = cells[f.b], polygon = joinDomains(a.polygonMm, b.polygonMm);
                    if (Math.abs(area(polygon) - area(a.polygonMm) - area(b.polygonMm)) > 1e-7)
                        fail("Joint domain area changed");
                    a.polygonMm = polygon;
                    a.id += "-joint";
                    cells.splice(f.b, 1);
                    buildFaces();
                    merged = true;
                    break;
                }
            }
            if (!merged)
                break;
            if (pass === 31)
                fail("Local return-domain merge did not stabilize");
        }
        faces = faces.filter((f) => f.slice);
        const coverage: CopperFragment[] = [], blocks: ChannelBlock[] = [], ports: SegmentedChannel['ports'] = [];
        for (const [index, cell] of cells.entries()) {
            const own: CopperFragment[] = [];
            for (const r of relevant) {
                const physicalArea = regionArea(r, cell.polygonMm);
                if (physicalArea <= 1e-12)
                    continue;
                own.push({ kind: 'fragment', sourcePath: r.source.path, regionId: r.id, layer: r.layer, domainPolygonMm: cell.polygonMm, areaMm2: physicalArea, sourcePolygonMm: r.polygon as CopperFragment['sourcePolygonMm'] });
            }
            const incident = faces.filter((f) => f.a === index || f.b === index), d = cell.direction, t = tangent(d), long = range(cell.polygonMm, d), cross = range(cell.polygonMm, t), centre = (cross[0] + cross[1]) / 2;
            const rectangle = Math.abs(area(cell.polygonMm) - (long[1] - long[0]) * (cross[1] - cross[0])) < 1e-8;
            const external = contacts.some((c) => g.regions.some((r) => r.source.path === c.sourcePath && r.layer === c.layer && regionArea(r, cell.polygonMm) > 1e-12));
            const ends = incident.filter((f) => Math.abs(Math.abs(project(f.normal, d)) - 1) < 1e-8);
            // A rectangular remainder cannot borrow invariance from another transverse corridor.
            let uniform = /^run-[0-9]+$/.test(cell.id) && rectangle && !external && incident.length === 2 && ends.length === 2;
            uniform &&= usable.some((c) => Math.abs(Math.abs(project(c.direction, d)) - 1) < 1e-8 && Math.abs(c.transverseMm[0] - cross[0]) < 1e-8 && Math.abs(c.transverseMm[1] - cross[1]) < 1e-8 && c.admissibleIntervalsMm.some(([a, b]) => long[0] > a && long[1] < b));
            if (uniform) {
                ends.sort((a, b) => project(a.origin, d) - project(b.origin, d));
                const mid = slice(add([d[0] * (long[0] + long[1]) / 2, d[1] * (long[0] + long[1]) / 2], t, centre), d, [-(cross[1] - cross[0]) / 2, (cross[1] - cross[0]) / 2], cell.id + "-mid");
                const midOrigin = add([d[0] * (long[0] + long[1]) / 2, d[1] * (long[0] + long[1]) / 2], t, centre);
                // Source subdivision may change at the two ends; match physical island and face span, never a net label.
                const orderedCuts = (f: Face) => mid.signals.map((m: any) => {
                    const witness = add(midOrigin, t, (m.intervalMm[0] + m.intervalMm[1]) / 2);
                    const region = g.regions.find((r) => r.layer === m.layer && m.sourcePaths.includes(r.source.path) && inRing2(r.polygon.outer, witness) && !r.polygon.holes.some((h) => inRing2(h, witness)));
                    const island2 = g.islands.find((i) => region && i.region_ids.includes(region.id));
                    const match = f.slice.signals.findIndex((v: any) => v.layer === m.layer && v.island === island2?.id && Math.abs(project(add(f.origin, tangent(f.normal), (v.intervalMm[0] + v.intervalMm[1]) / 2), t) - centre - (m.intervalMm[0] + m.intervalMm[1]) / 2) < 1e-7);
                    if (match < 0)
                        fail("Shared physical conductor mapping mismatch");
                    return f.slice.cuts[match];
                });
                const metal = [...mid.geometry.conductors.filter((m: any) => m.role !== "reference"), mid.geometry.conductors.find((m: any) => m.role === "reference")], maps = [...mid.signals, mid.reference], resistance = [], internal = [];
                for (let i = 0; i < metal.length; i++) {
                    const p = metal[i].contour.outerM ?? metal[i].contour.outer_m ?? metal[i].contour.outer, x = p.map((v: any) => v[0]), z4 = p.map((v: any) => v[1]), w = Math.max(...x) - Math.min(...x), th = Math.max(...z4) - Math.min(...z4), sigma = base2.stack.find((l: any) => l.name === maps[i].layer).conductivitySM;
                    if (th / w > 0.25)
                        fail("Low-band ribbon requires thin conductor");
                    if (Math.sqrt(1 / (Math.PI * 1e4 * 4e-7 * Math.PI * sigma)) < 10 * th)
                        fail("Skin overlap not admitted");
                    resistance.push(1 / (sigma * w * th));
                    internal.push(4e-7 * Math.PI * th / (12 * w));
                }
                const n = resistance.length - 1, matrix2 = (v: any) => Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_2, j) => v[n] + (i === j ? v[i] : 0)));
                blocks.push({ id: cell.id, kind: "section", line: { kind: "polygon-section", lengthMm: long[1] - long[0], geometry: mid.geometry }, dcConductors: { resistanceOhmPerM: matrix2(resistance), internalInductanceHPerM: matrix2(internal) }, cuts: ends.flatMap(orderedCuts), ownership: own, bandHz: [1e3, 1e4] });
            }
            else {
                const electrodes: any[] = [], pairs: string[][] = [], cuts: ChannelCut[] = [], cutPlanes: Record<string, unknown> = {}, names = new Map<string, string>();
                const electrode = (key: string, m: any, f?: Face) => {
                    let name = names.get(key);
                    if (name)
                        return name;
                    name = "terminal-" + electrodes.length;
                    names.set(key, name);
                    electrodes.push({ name, ...m.sourcePaths?.length > 1 ? { sourcePaths: [...new Set(m.sourcePaths)].sort() } : { sourcePath: m.sourcePaths?.[0] ?? m.sourcePath }, layer: m.layer, upper: m.layer === base2.stack.filter((l: any) => l.copper).at(-1).name });
                    if (f)
                        cutPlanes[name] = { originMm: f.origin, normalXY: f.normal, spanMm: m.intervalMm };
                    return name;
                };
                for (const f of incident)
                    for (let k = 0; k < f.slice.cuts.length; k++) {
                        const c = f.slice.cuts[k];
                        pairs.push([electrode(c.positive, f.slice.signals[k], f), electrode(c.negative, f.slice.reference, f)]);
                        cuts.push(c);
                    }
                for (let p = 0; p < 2; p++) {
                    const contact = contacts[p], r = g.regions.find((r2) => r2.source.path === contact.sourcePath && r2.layer === contact.layer)!;
                    if (regionArea(r, cell.polygonMm) <= 1e-12)
                        continue;
                    if (Math.abs(regionArea(r, cell.polygonMm) - r.area_mm2) > 1e-7)
                        fail("A route cut split an external electrode");
                    const c = { positive: "external-" + p + "-signal", negative: "external-" + p + "-return" };
                    pairs.push([electrode(c.positive, contact), electrode(c.negative, contacts[p + 2])]);
                    cuts.push(c);
                    ports.push({ name: p === 0 ? "input" : "output", cut: c });
                }
                if (!pairs.length)
                    fail("Unconnected explicit local cell; a joint domain is required");
                const x = range(cell.polygonMm, [1, 0]), y = range(cell.polygonMm, [0, 1]);
                blocks.push({ id: cell.id, kind: "local", boardJson, geometry: { ...base2, roiMm: [[x[0], y[0]], [x[1], y[1]]], domainPolygonMm: cell.polygonMm, electrodes, ports: pairs, cutPlanes }, magneticPatchLevel: options.magneticPatchLevel ?? 1, cuts, ownership: own, bandHz: [1e3, 1e4] });
            }
        }
        const barrelOwnership=g.holes.filter(h=>h.plated&&h.span_confirmed).flatMap(h=>{
            const points=h.polygons.flatMap(p=>p.outer) as Point[];if(!points.length)return [];
            const x=range(points,[1,0]),y=range(points,[0,1]),envelope:Point[]=[[x[0]-wall,y[0]-wall],[x[1]+wall,y[0]-wall],[x[1]+wall,y[1]+wall],[x[0]-wall,y[1]+wall]];
            const expected=clippedArea(envelope,domain);if(expected<=1e-12)return [];
            const owners=cells.flatMap(c=>{const overlap=triangles(c.polygonMm).reduce((a,t)=>a+clippedArea(envelope,t),0);return overlap>1e-10?[{id:c.id,area:overlap}]:[];});
            if(owners.length!==1||blocks.find(b=>b.id===owners[0].id)?.kind!=='local'||Math.abs(owners[0].area-area(envelope))>1e-8)fail('Plated barrel requires one complete joint local source domain: '+h.source.path);
            return [{holeId:h.id,sourcePath:h.source.path,layers:h.layers,ownerBlockId:owners[0].id,contract:'Whole plated-wall envelope in one local; native bore/wall area, topology and contact guards remain authoritative'}];
        });
        for (const r of relevant)
            coverage.push({ kind: 'fragment', sourcePath: r.source.path, regionId: r.id, layer: r.layer, domainPolygonMm: domain, areaMm2: regionArea(r, domain), sourcePolygonMm: r.polygon as CopperFragment['sourcePolygonMm'] });
        if (ports.length !== 2)
            fail("External ports must have unique complete source ownership");
        ports.sort((a, b) => a.name === "input" ? -1 : b.name === "input" ? 1 : 0);
        return { blocks, ports, coverage, audit: { graph, corridors, cells, barrelOwnership, faces: faces.map(({ slice: slice2, ...f }) => ({ ...f, sourceMapping: slice2.sourceMapping })), ownershipConvention: "Explicit original-source fragments clipped by route/event domains, with region/layer identity and areaMm2; interval fixtures retain length units", signalIsland: signal.id, returnIslands: returns.map((i) => i.id), copperDecisions: g.regions.map((r) => ({ regionId: r.id, sourcePath: r.source.path, layer: r.layer, role: signal.region_ids.includes(r.id) ? "signal" : returnIds.has(r.id) ? "return" : "neighbour", included: relevant.includes(r), reason: relevant.includes(r) ? "Actual source fragment retained in joint route cell" : "Outside finite numerical domain; expansion remains required" })), policy: "All branches, finite return and spatial neighbours retained; local/section coupling requires numerical acceptance; nonlocal plane coupling not certified" } };
    };
}
/** B2 finite-domain candidates for C1 only; full-channel admission is unchanged.
 * All contour events inside each tested width are checked again. */
export function boardSectionCandidates(board:any,stack:any[],contacts:{sourcePath:string;layer:string}[]){
 const boardJson=JSON.stringify(board),g=capturedBoardGeometry(boardJson);
 const terminal=(c:{sourcePath:string;layer:string})=>{const r=g.regions.find(r=>r.source.path===c.sourcePath&&r.layer===c.layer);return g.islands.find(i=>r&&i.region_ids.includes(r.id))??fail('Unconfirmed section terminal');};
 const signal=terminal(contacts[0]);if(signal.id!==terminal(contacts[1]).id)fail('Disconnected section signal');const returns=[terminal(contacts[2]),terminal(contacts[3])];if(returns[0].id!==returns[1].id)fail('Separate return islands require joint model');
 const h=stack.reduce((sum,l)=>sum+l.thicknessM*1e3,0),widths=[h*2,h*3,h*4];
 const graph=buildRouteGraph(board,g,new Set(signal.region_ids),0);const candidates:any[]=[];
 for(const run of graph.runs){
  const cs=widths.map(w=>coupledRunCorridors(g,[run],w)[0]);
  // Dielectric outlines/cutouts and bores must also be invariant in the larger
  // field-only exterior. External copper is retained, not made a physical edge.
  const field=coupledRunCorridors({...g,layers:[],regions:[]} as AnalysisGeometryReport,[run],widths.at(-1)!+Math.max(h*3,1))[0];
  let intervals=cs[0].geometricIntervalsMm;
  for(const c of [...cs.slice(1),field])intervals=intervals.flatMap(([a,b])=>c.geometricIntervalsMm.map(([x,y])=>[Math.max(a,x),Math.min(b,y)] as [number,number]).filter(([x,y])=>y-x>1e-5));
  const d=run.direction,t=tangent(d),centre=project(run.from,t);
  for(const [a,b]of intervals){const inset=(b-a)/8,lo=a+inset,hi=b-inset,atMm=[d[0]*(lo+hi)/2+t[0]*centre,d[1]*(lo+hi)/2+t[1]*centre];
   candidates.push({runId:run.id,origin:atMm,direction:d,transverseUnit:t,lengthMm:hi-lo,longitudinalMm:[lo,hi],widthsMm:widths,airMarginMm:Math.max(h*2,1),sourcePaths:run.sourcePaths,sourceRegionIds:run.regionIds,corridors:cs,physicalReturnIsland:returns[0].id,signalIsland:signal.id});
  }
 }
 candidates.sort((a,b)=>b.lengthMm-a.lengthMm||a.runId.localeCompare(b.runId)||a.longitudinalMm[0]-b.longitudinalMm[0]);
 return {boardJson,g,graph,candidates,policy:'longest certified common-width interval, then source id and coordinate; no solver-method search',contacts};
}


