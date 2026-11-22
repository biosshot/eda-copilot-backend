import { autoFoilPlan, type AutoFoilPlan, type FoilClosureStep } from './auto-foil-plan.ts';
import { componentReachableIslands, MAX_CHANNEL_COMPONENTS, validateChannelAttachments, type BoardChannelRequest } from './channel-components.ts';
import { normalizePlannerTerminals, type LegacyChannelContact } from './channel-terminals.ts';
import type { AnalysisGeometryReport } from '../../native/analyze/index.js';
import type { ChannelBlock, ChannelCut, CopperFragment, SegmentedChannel } from './segmented-channel.ts';
import { createProblemExecutor } from './execute-problem.ts';
import { capturedBoardGeometry } from './geometry-snapshot.ts';
import { buildRouteGraph, project, tangent, area, clipHalfPlane, triangles, joinDomains, type Point } from './route-partition.ts';
import { coupledRunCorridors, corridorCells } from './run-corridors.ts';
export const boardChannelBlockBandHz = [1e3, 1e4] as const;
export const boardChannelJointSkinBandHz = [1e3, 1e5] as const;
/** Implementation envelope; native electrical-size, mesh and physical gates still apply. */
export const boardChannelHybridBandHz = [1e3, 1e10] as const;
type Options = {
    domainScale?: number;
    guardMarginMm?: number;
    magneticPatchLevel?: 1 | 2;
    boundaryFraction?: number;
    autoFoilDeadlineMs?: number;
    onClosureStep?: (step:FoilClosureStep)=>void;
};
type ClosedJointOptions = Options & { closedJointDeadlineMs: number; guardedReturn?: boolean };
type SegmentedGeometryPlan = SegmentedChannel & { audit: any };
type JointGeometryReason = "finite-return-closure" | "no-finite-return-corridor" | "no-finite-return-interface";
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
export function boardChannelPlan(board: any, base: any, contacts: LegacyChannelContact[] | BoardChannelRequest, options: Options = {}) { return createBoardChannelPlanner(board, base, contacts)(options); }
const fail = (s: string): never => {
    throw new Error("CHANNEL_SEGMENTATION: " + s);
};
const add = (a: Point, b: Point, s = 1): Point => [a[0] + s * b[0], a[1] + s * b[1]];
const range = (ps: Point[], d: Point) => [Math.min(...ps.map((p) => project(p, d))), Math.max(...ps.map((p) => project(p, d)))];
export function createBoardChannelPlanner(board: any, base2: any, input: LegacyChannelContact[] | BoardChannelRequest) {
    const { contacts, signalIds, ports: externalPorts } = normalizePlannerTerminals(input);
    const legacy = Array.isArray(input);
    if (!legacy && (input.components?.length ?? 0) > MAX_CHANNEL_COMPONENTS)
        fail("Component bridge count exceeds " + MAX_CHANNEL_COMPONENTS);
    const components = legacy ? [] : structuredClone(input.components ?? []);
    // The generic local default is too coarse to refine the already constrained
    // coupon mesh. Automatic channels start from the verified current grid;
    // level 2 then performs a real fourfold area refinement instead of a no-op.
    const foilGeometry=structuredClone(base2);
    base2 = { ...base2, maxTriangleAreaM2: Math.min(base2.maxTriangleAreaM2, 5e-8) };
    const execute = createProblemExecutor(true), boardJson = JSON.stringify(board), snapshot = JSON.parse(boardJson);
    const g = capturedBoardGeometry(boardJson);
    function plan(options: ClosedJointOptions): AutoFoilPlan;
    function plan(options?: Options): SegmentedGeometryPlan;
    function plan(options: Options | ClosedJointOptions = {}): SegmentedGeometryPlan | AutoFoilPlan {
        const islandByRegion = new Map(g.islands.flatMap(i => i.region_ids.map(id => [id, i] as const)));
        const resolveTerminal = (contact: typeof contacts[number]) => {
            const regions = g.regions.filter(r => r.source.path === contact.sourcePath && r.layer === contact.layer);
            if (regions.length !== 1)
                fail("Missing or ambiguous physical terminal " + contact.id);
            const region = regions[0];
            const island = islandByRegion.get(region.id) ?? fail("Unconfirmed physical terminal " + contact.id);
            return { contact, region, island };
        };
        const terminals = new Map(contacts.map(contact => [contact.id, resolveTerminal(contact)] as const));
        const islands = (signal: boolean) => [...new Map([...terminals.values()]
            .filter(({ contact }) => signalIds.has(contact.id) === signal)
            .map(({ island }) => [island.id, island])).values()];
        const signals = islands(true), returns = islands(false);
        if (legacy && signals.length > 1)
            fail("Disconnected legacy signal endpoints; capacitive or component bridge continuity requires a verified model");
        const terminalSourceKey = (contact: typeof contacts[number]) => JSON.stringify([contact.sourcePath, contact.layer]);
        const terminalsBySource = new Map([...terminals.values()].map(t => [terminalSourceKey(t.contact), t] as const));
        const componentPlans = components.map(({ id, designator, pins, model, equivalentKind }) => {
            const resolvePin = (pin: typeof contacts[number]) => {
                const key = terminalSourceKey(pin);
                const known = terminalsBySource.get(key);
                if (known)
                    return known;
                if (terminals.has(pin.id))
                    fail("Component pin ID collides with a separate copper terminal: " + pin.id);
                const resolved = resolveTerminal(pin);
                terminals.set(pin.id, resolved);
                terminalsBySource.set(key, resolved);
                return resolved;
            };
            return { id, designator, model, equivalentKind, pins: [resolvePin(pins[0]), resolvePin(pins[1])] as const };
        });
        const componentEdges = componentPlans.map(({ pins: [a, b] }) => [a.island.id, b.island.id] as const);
        const reachable = componentReachableIslands([...signals, ...returns].map(i => i.id), componentEdges);
        if (componentEdges.some(([a, b]) => !reachable.has(a) || !reachable.has(b)))
            fail("Component bridge is detached from all declared channel contacts");
        const signalIslandIds = componentReachableIslands(signals.map(i => i.id), componentEdges, new Set(returns.map(i => i.id)));
        const signalRegions = new Set(g.islands.filter(i => signalIslandIds.has(i.id)).flatMap(i => i.region_ids));
        const returnIds = new Set(returns.flatMap(i => i.region_ids));
        const contactRegions = new Set([...terminals.values()].map(({ region }) => region.id));
        const selected = g.regions.filter(r => signalRegions.has(r.id) || contactRegions.has(r.id));
        const height = base2.stack.reduce((s: number, l: any) => s + l.thicknessM * 1e3, 0);
        const margin = Math.max(0.05, 2 * height) * (options.domainScale ?? 1);
        const closedJoint = (deadlineMs: number, searchMarginMm?: number): AutoFoilPlan | undefined => {
            // Compute this only for an actual joint candidate. The run frame is
            // fixed for the complete closure, including newly found islands.
            let direction:[number,number]|undefined;
            let directionReason='hybrid direction not requested';
            if (searchMarginMm===undefined && process.env.P7_COMPACT_PLANAR==='1') {
                if (performance.now()>=deadlineMs) throw new Error('CHANNEL_FOIL_DEADLINE: shared geometry closure deadline exhausted');
                const runs=buildRouteGraph(board,g,signalRegions,margin).runs;
                if (performance.now()>=deadlineMs) throw new Error('CHANNEL_FOIL_DEADLINE: shared geometry closure deadline exhausted');
                if (runs.length) {
                    const d=runs[0].direction;
                    if (runs.every(r=>Math.abs(d[0]*r.direction[1]-d[1]*r.direction[0])<=1e-12)) direction=d;
                    else directionReason='multiple non-collinear physical runs';
                } else directionReason='no physical run direction';
            }
            const result=autoFoilPlan({boardJson,geometry:foilGeometry,captured:g,terminals:[...terminals.values()].map(entry=>entry.contact),ports:externalPorts,
                modePairs:legacy?undefined:input.modePairs,
                components:componentPlans.map(({pins:[a,b],...model})=>({...model,pins:[a.contact,b.contact]})),
                seedRegions:new Set([...signalRegions,...returnIds,...contactRegions,
                    ...componentPlans.flatMap(({pins})=>pins.flatMap(({island})=>island.region_ids))]),
                marginMm:searchMarginMm??margin,guardMarginMm:searchMarginMm??options.guardMarginMm,deadlineMs,onClosureStep:options.onClosureStep,direction,directionReason});
            if (!result) return;
            if (searchMarginMm!==undefined) {
                const box=result.observationRoiMm!;
                const zTop=foilGeometry.stack.reduce((sum:number,layer:any)=>sum+layer.thicknessM*1000,0);
                result.geometry.returnBoundaryMm=[[box[0][0],box[0][1],-searchMarginMm],[box[1][0],box[1][1],zTop+searchMarginMm]];
                result.geometry.electricAirMarginM=Math.max(Number(foilGeometry.electricAirMarginM)||0,2*searchMarginMm/1000);
                result.geometry.coordinateOriginMm=[(box[0][0]+box[1][0])/2,(box[0][1]+box[1][1])/2];
            }
            Object.defineProperty(result,'returnSearch',{enumerable:false,value:{initialMarginMm:margin,expand:(domainScale:number,deadline:number)=>{
                if (!(domainScale>=1) || !Number.isFinite(domainScale)) throw new Error('CHANNEL_FOIL_DOMAIN: invalid return search scale');
                if (domainScale===1 && searchMarginMm===margin) return result;
                const expanded=closedJoint(deadline,margin*domainScale);
                if (!expanded) throw new Error('CHANNEL_JOINT_CLOSURE: unable to expand return search domain');
                return expanded;
            }}});
            return result;
        };
        if ('closedJointDeadlineMs' in options) {
            const joint = closedJoint(options.closedJointDeadlineMs, options.guardedReturn ? margin : undefined);
            if (!joint) throw new Error('CHANNEL_JOINT_CLOSURE: frequency demand requires a bounded closed physical copper candidate');
            return joint;
        }
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
        const seed = buildRouteGraph(snapshot, g, signalRegions, 0).runs.sort((a, b) => b.lengthMm - a.lengthMm)[0];
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
        const regionSelection = (polygon: Point[]) => g.regions.filter(r => regionArea(r, polygon) > 1e-12);
        const routeGraph = (regions: AnalysisGeometryReport['regions']) =>
            buildRouteGraph(snapshot, g, new Set(regions.map(r => r.id)), Math.max(0.05, 2 * height));
        let relevant = regionSelection(domain), graph = routeGraph(relevant);
        const returnRegions = g.regions.filter(r => returnIds.has(r.id));
        const signalRuns = graph.runs.filter((r) => r.regionIds.some(id => signalRegions.has(id)));
        const inRing2 = (ring: number[][], p: Point) => {
            let yes = false;
            for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
                const a = ring[i], b = ring[j];
                if (a[1] > p[1] !== b[1] > p[1] && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0])
                    yes = !yes;
            }
            return yes;
        };
        function polygonIntervals(polygon: AnalysisGeometryReport['regions'][number]['polygon'], origin: Point, normal: Point, span: [number, number]) {
            const t = tangent(normal), cuts = [...span], intervals: [number, number][] = [];
            for (const ring of [polygon.outer, ...polygon.holes])
                for (let i = 0; i < ring.length; i++) {
                    const a = add(ring[i], origin, -1), b = add(ring[(i + 1) % ring.length], origin, -1);
                    const x = project(a, normal), y = project(b, normal);
                    if (x <= 0 && y > 0 || y <= 0 && x > 0) {
                        const at = project(a, t) + (project(b, t) - project(a, t)) * -x / (y - x);
                        if (at > span[0] && at < span[1])
                            cuts.push(at);
                    }
                }
            cuts.sort((a, b) => a - b);
            for (let i = 1; i < cuts.length; i++) {
                const a = cuts[i - 1], b = cuts[i], p = add(origin, t, (a + b) / 2);
                if (b - a > 1e-9 && inRing2(polygon.outer, p) && !polygon.holes.some(h => inRing2(h, p)))
                    intervals.push([a, b]);
            }
            return intervals;
        }
        const baseHalfWidth = margin + Math.max(...signalRuns.map(r => r.widthMm / 2), 0);
        function sweptCorridors(halfWidth: number) {
            const corridors = coupledRunCorridors(g, signalRuns, halfWidth);
            for (const c of corridors) {
                const d = c.direction, t = tangent(d), [low, high] = c.transverseMm;
                // A section through a longitudinal hole still belongs to the
                // connected bypass. Inspect the physical union across the width.
                const gaps = g.layers.flatMap(l => l.copper.flatMap(p => p.holes)).flatMap(h => {
                    const x = range(h, d), y = range(h, t);
                    return y[1] > low && y[0] < high ? [x] : [];
                });
                c.admissibleIntervalsMm = c.admissibleIntervalsMm.filter(([a, b]) => {
                    if (gaps.some(([x, y]) => Math.min(b, y) - Math.max(a, x) > 1e-8))
                        return false;
                    const origin = add(add([0, 0], d, (a + b) / 2), t, (low + high) / 2);
                    const halfWidth = (high - low) / 2;
                    return returnRegions.some(r => polygonIntervals(r.polygon, origin, d, [-halfWidth, halfWidth]).length > 0);
                });
            }
            return corridors;
        }

        let corridors = sweptCorridors(baseHalfWidth);
        let usable = corridors.filter(c => c.admissibleIntervalsMm.length);
        let returnClosure: {
            reason: string;
            polygonMm?: Point[];
            areaMm2?: number;
            transverseMm?: [number, number];
        } | undefined;
        function refuseCorridor(geometryReason: JointGeometryReason, reason: string): never {
            const foilCandidate=options.autoFoilDeadlineMs!==undefined?closedJoint(options.autoFoilDeadlineMs):undefined;
            throw Object.assign(new Error("CHANNEL_CORRIDOR: " + reason), {
                code: "CHANNEL_CORRIDOR_UNSUPPORTED",
                geometryReason,
                ...(foilCandidate && { foilCandidate }),
                corridorAudit: { corridors, returnClosure, contacts, sourceCopper: g.regions, physicalIslands: g.islands, physicalContacts: g.contacts, bores: g.holes, signalIsland: signalIslandIds.size === 1 ? [...signalIslandIds][0] : undefined, signalIslands: [...signalIslandIds], returnIslands: returns.map(i => i.id), geometryFindings: g.findings, signalRegions: g.regions.filter(r => signalRegions.has(r.id)).map(r => ({ regionId: r.id, sourcePath: r.source.path, layer: r.layer, areaMm2: r.area_mm2 })) }
            });
        }
        if (corridors.length) {
            const d = seed?.direction ?? corridors[0].direction, t = tangent(d);
            const originalLong = range(domain, d), originalCross = range(domain, t);
            const closedLong = [...originalLong], closedCross = [...originalCross];
            // Inspect every source of the confirmed terminal islands. This
            // rectangle is invariant to CAD subdivision and excludes islands
            // connected only by their net name.
            for (const r of returnRegions)
                for (const p of r.polygon.outer) {
                    const x = project(p, d), y = project(p, t);
                    closedLong[0] = Math.min(closedLong[0], x);
                    closedLong[1] = Math.max(closedLong[1], x);
                    closedCross[0] = Math.min(closedCross[0], y);
                    closedCross[1] = Math.max(closedCross[1], y);
                }
            const clipped = closedLong.some((v, i) => Math.abs(v - originalLong[i]) > 1e-8)
                || closedCross.some((v, i) => Math.abs(v - originalCross[i]) > 1e-8);
            const lateralReturn = corridors.some(c => c.lateralContinuations.some(v => returnIds.has(v.regionId)));
            if (clipped || (!usable.length && lateralReturn)) {
                if (corridors.length !== 1) {
                    returnClosure = { reason: "Finite return closure needs distributed interface coordinates across multiple corridors" };
                    refuseCorridor("finite-return-closure", returnClosure.reason);
                }
                const [low, high] = corridors[0].transverseMm, centre = (low + high) / 2;
                const halfWidth = Math.max(baseHalfWidth, centre - closedCross[0], closedCross[1] - centre) + margin;
                const transverseMm: [number, number] = [centre - halfWidth, centre + halfWidth];
                const [a, b] = [closedLong[0] - margin, closedLong[1] + margin];
                const polygon = [[a, transverseMm[0]], [b, transverseMm[0]], [b, transverseMm[1]], [a, transverseMm[1]]]
                    .map(([x, y]) => add(add([0, 0], d, x), t, y));
                const expanded = sweptCorridors(halfWidth);
                returnClosure = { reason: "Complete finite terminal return islands inside the re-swept physical envelope", polygonMm: polygon, transverseMm, areaMm2: area(polygon) };
                if (expanded.length !== 1 || expanded[0].lateralContinuations.length || !expanded[0].admissibleIntervalsMm.length) {
                    corridors = expanded;
                    returnClosure.reason = "Expanded finite return envelope still needs a distributed lateral or joint interface";
                    refuseCorridor("finite-return-closure", returnClosure.reason);
                }
                domain = polygon;
                corridors = expanded;
                usable = expanded;
                relevant = regionSelection(domain);
                graph = routeGraph(relevant);
            }
        }
        if (signalRuns.length && !usable.length)
            refuseCorridor("no-finite-return-corridor", "No admissible finite-return uniform corridor; distributed lateral copper/material events require a joint domain");
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
        for (const { region } of terminals.values()) {
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
        function slice(origin: Point, n: Point, span: [
            number,
            number
        ], id: string) {
            const t = tangent(n);
            const intervals = g.layers.flatMap(layer => layer.copper.flatMap(polygon =>
                polygonIntervals(polygon, origin, n, span).map(([low, high]) => ({ layer: layer.layer, low, high }))));
            intervals.sort((a, b) => a.low - b.low);
            // Native source attribution uses the physical union interval's
            // centre. A CAD seam can give an off-centre point another owner.
            const reference = intervals.map(({ layer, low, high }) => {
                const offset = low + (high - low) / 2, point = add(origin, t, offset);
                const region = returnRegions.find(r => r.layer === layer && inRing2(r.polygon.outer, point)
                    && !r.polygon.holes.some(h => inRing2(h, point)));
                return { region, offset };
            }).find(candidate => candidate.region);
            if (!reference?.region)
                return refuseCorridor("no-finite-return-interface", "No finite return at " + id + "; partition requires a joint/nonlocal return model");
            const { region: reference2, offset } = reference;
            const value2 = execute({ kind: "board-section", boardJson, stack: base2.stack, slice: { atMm: origin, transverseUnit: t, halfWidthMm: (span[1] - span[0]) / 2, airMarginMm: Math.max(1, 4 * height) * (options.domainScale ?? 1), referenceLayer: reference2.layer, referenceSourcePath: reference2.source.path, referenceXMm: offset } });
            const mapping = value2.sourceMapping, ref = mapping.find((m: any) => m.role === "reference"), signals = mapping.filter((m: any) => m.role !== "reference");
            const key = (m: any) => JSON.stringify({ face: id, layer: m.layer, sources: m.sourcePaths, interval: m.intervalMm });
            return { ...value2, reference: ref, signals, cuts: signals.map((m: any) => ({ positive: key(m), negative: key(ref) })) };
        }
        const faceKey = (f: Face, m: { layer: string; island: string; intervalMm: number[] }) =>
            JSON.stringify({ face: f.id, layer: m.layer, island: m.island, interval: m.intervalMm });
        // Match actual source intervals on one finite face, preserving disconnected returns and floating neighbours.
        function localFace(f: Face) {
            const intervals = [];
            for (const r of relevant) {
                for (const intervalMm of polygonIntervals(r.polygon, f.origin, f.normal, f.span))
                    intervals.push({ layer: r.layer, sourcePaths: [r.source.path], regionIds: [r.id], island: islandByRegion.get(r.id)?.id, intervalMm });
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
            const reference2 = mapping.find((m: any) => m.regionIds.some((id: string) => returnIds.has(id))) ?? refuseCorridor("no-finite-return-interface", "No finite return contact at " + f.id + "; requires joint local domain");
            const signals = mapping.filter((m: any) => m !== reference2), key = (m: any) => faceKey(f, m);
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
        if (cells.length > 32)
            fail("Physical block count exceeds the composition budget of 32");
        const terminalOwners = new Map<string, number>();
        for (const [id, { region }] of terminals) {
            const owners = cells.flatMap((cell, index) => {
                const overlap = regionArea(region, cell.polygonMm);
                return overlap > 1e-12 ? [{ index, overlap }] : [];
            });
            if (owners.length !== 1 || Math.abs(owners[0].overlap - region.area_mm2) > 1e-7)
                fail("Physical terminal lacks unique complete source ownership: " + id);
            terminalOwners.set(id, owners[0].index);
        }
        const externalKey = (id: string) => "external:" + id;
        const cellPorts = externalPorts.map(({ name, positive, negative }) => {
            const owner = terminalOwners.get(positive)!;
            if (terminalOwners.get(negative) !== owner)
                fail("External port spans separate physical blocks: " + name);
            return { name, positive, negative, owner, cut: { positive: externalKey(positive), negative: externalKey(negative) } };
        });
        const componentPins = new Set(componentPlans.flatMap(component => component.pins.map(pin => pin.contact.id)));
        const attachments = componentPlans.map(({ id, model, pins: [positive, negative] }) => ({
            id, model, cut: { positive: externalKey(positive.contact.id), negative: externalKey(negative.contact.id) },
        }));
        const coverage: CopperFragment[] = [], blocks: ChannelBlock[] = [];
        const ports: SegmentedChannel['ports'] = cellPorts.map(({ name, cut }) => ({ name, cut }));
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
            const external = cellPorts.filter(port => port.owner === index);
            const localPins = [...componentPins].filter(id => terminalOwners.get(id) === index);
            const ends = incident.filter((f) => Math.abs(Math.abs(project(f.normal, d)) - 1) < 1e-8);
            // A rectangular remainder cannot borrow invariance from another transverse corridor.
            let uniform = /^run-[0-9]+$/.test(cell.id) && rectangle && !external.length && !localPins.length && incident.length === 2 && ends.length === 2;
            uniform &&= usable.some((c) => Math.abs(Math.abs(project(c.direction, d)) - 1) < 1e-8 && Math.abs(c.transverseMm[0] - cross[0]) < 1e-8 && Math.abs(c.transverseMm[1] - cross[1]) < 1e-8 && c.admissibleIntervalsMm.some(([a, b]) => long[0] > a && long[1] < b));
            if (uniform) {
                ends.sort((a, b) => project(a.origin, d) - project(b.origin, d));
                const mid = slice(add([d[0] * (long[0] + long[1]) / 2, d[1] * (long[0] + long[1]) / 2], t, centre), d, [-(cross[1] - cross[0]) / 2, (cross[1] - cross[0]) / 2], cell.id + "-mid");
                const midOrigin = add([d[0] * (long[0] + long[1]) / 2, d[1] * (long[0] + long[1]) / 2], t, centre);
                // Match every metal at each end, including the native voltage gauge.
                // The face basis can choose a different gauge while preserving physical incidence.
                const maps = [...mid.signals, mid.reference];
                const sectionEnd = (f: Face) => {
                    if (f.slice.sourceMapping.length !== maps.length)
                        fail("Shared physical conductor count mismatch");
                    const entries = maps.map((m: any) => {
                        const centreMm = (m.intervalMm[0] + m.intervalMm[1]) / 2;
                        const witness = add(midOrigin, t, centreMm);
                        const region = g.regions.find(r => r.layer === m.layer && m.sourcePaths.includes(r.source.path)
                            && inRing2(r.polygon.outer, witness) && !r.polygon.holes.some(h => inRing2(h, witness)));
                        const island = region && islandByRegion.get(region.id);
                        const matches = f.slice.sourceMapping.filter((v: any) => v.layer === m.layer && v.island === island?.id
                            && Math.abs(project(add(f.origin, tangent(f.normal), (v.intervalMm[0] + v.intervalMm[1]) / 2), t) - centre - centreMm) < 1e-7);
                        if (!island || matches.length !== 1)
                            fail("Shared physical conductor mapping mismatch");
                        return [m.conductorId, faceKey(f, matches[0])] as const;
                    });
                    if (new Set(entries.map(([, key]) => key)).size !== maps.length)
                        fail("Shared physical conductor mapping is not one-to-one");
                    return Object.fromEntries(entries);
                };
                const sectionEnds = [sectionEnd(ends[0]), sectionEnd(ends[1])] as const;
                const metal = [...mid.geometry.conductors.filter((m: any) => m.role !== "reference"), mid.geometry.conductors.find((m: any) => m.role === "reference")], resistance = [], internal = [];
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
                const sectionMeshFactor = options.magneticPatchLevel === 1 ? 1 : 0.5;
                blocks.push({ id: cell.id, kind: "section", line: { kind: "polygon-section", lengthMm: long[1] - long[0], geometry: { ...mid.geometry, maxTriangleAreaM2: mid.geometry.maxTriangleAreaM2 * sectionMeshFactor } }, spatial: { originMm: midOrigin, direction: d }, dcConductors: { resistanceOhmPerM: matrix2(resistance), internalInductanceHPerM: matrix2(internal) }, sectionEnds, cuts: ends.flatMap(f => f.slice.cuts), ownership: own, bandHz: [...boardChannelBlockBandHz] });
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
                for (const port of external) {
                    const a = terminals.get(port.positive)!.contact, b = terminals.get(port.negative)!.contact;
                    pairs.push([electrode(port.cut.positive, a), electrode(port.cut.negative, b)]);
                    cuts.push(port.cut);
                }
                const unexposedPins = localPins.filter(id => !names.has(externalKey(id)));
                if (unexposedPins.length) {
                    // A private pair exposes the pin to P6. Its other electrode
                    // is a physical contact already present in this local cell.
                    let anchor = cuts[0]?.negative;
                    if (!anchor && unexposedPins.length > 1) {
                        const id = unexposedPins.shift()!;
                        anchor = externalKey(id);
                        electrode(anchor, terminals.get(id)!.contact);
                    }
                    if (!anchor)
                        fail("Component pin lacks a second physical electrode in its local block: " + unexposedPins[0]);
                    const anchorKey = anchor;
                    for (const id of unexposedPins) {
                        const key = externalKey(id);
                        pairs.push([electrode(key, terminals.get(id)!.contact), names.get(anchorKey)!]);
                        cuts.push({ positive: key, negative: anchorKey });
                    }
                }
                if (!pairs.length)
                    fail("Unconnected explicit local cell; a joint domain is required");
                const x = range(cell.polygonMm, [1, 0]), y = range(cell.polygonMm, [0, 1]);
                // The magnetic volume integral has no patch/domain mesh of its own.
                // Spatial levels therefore refine the actual current/electric mesh.
                const meshFactor = options.magneticPatchLevel === 1 ? 1 : 0.25;
                blocks.push({ id: cell.id, kind: "local", boardJson, geometry: { ...base2, maxTriangleAreaM2: base2.maxTriangleAreaM2 * meshFactor, roiMm: [[x[0], y[0]], [x[1], y[1]]], coordinateOriginMm: base2.roiMm[0], electricAirMarginM: 0.008 * (options.domainScale ?? 1), domainPolygonMm: cell.polygonMm, electrodes, ports: pairs, cutPlanes }, magneticPatchLevel: options.magneticPatchLevel ?? 1, cuts, ownership: own, bandHz: [...boardChannelBlockBandHz] });
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
        if (blocks.some(block => block.cuts.length > 16) || blocks.reduce((n, block) => n + block.cuts.length, attachments.length) > 128)
            fail("Physical cut count exceeds the composition budget (16 per block, 128 total)");
        validateChannelAttachments(blocks, attachments);
        const componentAudit = componentPlans.map(({ id, designator, pins, model }) => ({
            id, designator, modelId: model.id, modelValidity: model.validity, pins: pins.map(({contact,island}) => ({
                ...contact, island: island.id, owner: cells[terminalOwners.get(contact.id)!].id,
            })),
            continuity: "Supplied model dependency; physical copper islands and DC connectivity remain separate",
        }));
        return { blocks, ports, coverage, ...(attachments.length && { attachments }), audit: { ...(attachments.length && { components: componentAudit }), graph, corridors, returnClosure, cells, barrelOwnership, physicalTerminals: [...terminals.values()].map(({ contact, island }) => ({ ...contact, island: island.id, owner: cells[terminalOwners.get(contact.id)!].id })), faces: faces.map(({ slice: slice2, ...f }) => ({ ...f, sourceMapping: slice2.sourceMapping })), ownershipConvention: "Explicit original-source fragments clipped by route/event domains, with region/layer identity and areaMm2; interval fixtures retain length units", signalIsland: signalIslandIds.size === 1 ? [...signalIslandIds][0] : undefined, signalIslands: [...signalIslandIds], returnIslands: returns.map((i) => i.id), copperDecisions: g.regions.map((r) => ({ regionId: r.id, sourcePath: r.source.path, layer: r.layer, role: signalRegions.has(r.id) ? "signal" : returnIds.has(r.id) ? "return" : "neighbour", included: relevant.includes(r), reason: relevant.includes(r) ? "Actual source fragment retained in joint route cell" : "Outside finite numerical domain; expansion remains required" })), policy: "All branches, finite return and spatial neighbours retained; local/section coupling requires numerical acceptance; nonlocal plane coupling not certified" } };
    }
    // Producer-wide upper capability envelope, not physical acceptance of every geometry.
    const capability: {readonly segmentedCapabilityBandHz?: readonly [number,number]} = {
        segmentedCapabilityBandHz: Object.freeze([...boardChannelBlockBandHz] as [number,number]),
    };
    return Object.assign(plan, capability);
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


