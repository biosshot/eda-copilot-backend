import type { ElkNode, ElkExtendedEdge } from 'elkjs';
import type { Circuit } from '../../src/types/circuit.ts';
import type { SymbolWithMeta } from '../../src/types/symbol.ts';
import type { autoPlaceCircuitWithHierarchy } from '../../src/circuit-layout/index.ts';
import { evaluateLayoutQuality } from '../../src/circuit-layout/quality.ts';
import { measureRouteShape } from '../../src/circuit-layout/refinement/net-routes.ts';
import { signalCheck, type SignalMismatch } from './schematic-signals.ts';

export type BankFixture = { source: string; circuit: Circuit; symbols: SymbolWithMeta[] };
export type LayoutResult = Awaited<ReturnType<typeof autoPlaceCircuitWithHierarchy>>;
type Point = { x: number; y: number };
type Segment = { a: Point; b: Point; net: string };
const key = (p: Point) => `${Math.round(p.x * 10000)},${Math.round(p.y * 10000)}`;
const near = (a: number, b: number) => Math.abs(a - b) < 1e-4;
const on = (p: Point, s: Segment) => (near(s.a.x, s.b.x) ? near(p.x, s.a.x) : near(p.y, s.a.y))
    && p.x >= Math.min(s.a.x, s.b.x) - 1e-4 && p.x <= Math.max(s.a.x, s.b.x) + 1e-4
    && p.y >= Math.min(s.a.y, s.b.y) - 1e-4 && p.y <= Math.max(s.a.y, s.b.y) + 1e-4;

export function flatGraph(result: LayoutResult): ElkNode {
    if (result.renderGraph) return result.renderGraph;
    const children: ElkNode[] = [];
    const visit = (node: ElkNode, ox = 0, oy = 0) => {
        const x = ox + (node.x ?? 0), y = oy + (node.y ?? 0);
        if (node.children?.length) for (const child of node.children) visit(child, x, y);
        else children.push({ ...node, x, y });
    };
    if (result.layoutedGraph) visit(result.layoutedGraph);
    return { id: 'flat', width: result.width, height: result.height, children, edges: result.edges };
}

/** Occupied geometry excludes ELK container padding and canvas margins. */
export function drawingBounds(result: LayoutResult) {
    const graph = flatGraph(result);
    const points = (graph.children ?? []).flatMap(n => [{ x: n.x!, y: n.y! }, { x: n.x! + n.width!, y: n.y! + n.height! }]);
    for (const e of result.edges) for (const s of e.sections ?? []) points.push(s.startPoint, ...(s.bendPoints ?? []), s.endPoint);
    const x = Math.min(...points.map(p => p.x)), y = Math.min(...points.map(p => p.y));
    const width = Math.max(...points.map(p => p.x)) - x, height = Math.max(...points.map(p => p.y)) - y;
    return { x, y, width, height, area: width * height };
}

/** Reconstruct drawn connections from coordinates. Only explicit net symbols and
 * explicitly delegated client labels join drawings; pin names alone never do. */
export function inspectLayout(fixture: BankFixture, result: LayoutResult) {
    const graph = flatGraph(result);
    const expected = fixture.circuit.components.flatMap(c => c.pins.map(p => ({
        id: `${c.designator}_pin_${p.pin_number}`, designator: c.designator, pinNumber: String(p.pin_number), signalName: p.signal_name,
        net: p.signal_name.trim() && !/^NC$/i.test(p.signal_name.trim()) ? p.signal_name : `unconnected:${c.designator}:${p.pin_number}`,
    })));
    const differences = new Map<string, SignalMismatch>();
    const mismatch = (p: typeof expected[number], problem: string) => differences.set(JSON.stringify([p.id, problem]), {
        signalName: p.signalName, designator: p.designator, pinNumber: p.pinNumber, problem,
    });
    const components = [...fixture.circuit.components, ...result.addedSymbol];
    const netByPin = new Map<string, string>(components.flatMap(c => c.pins.map(p => [`${c.designator}_pin_${p.pin_number}`, p.signal_name] as const)));
    const pins = new Map<string, Point>();
    for (const node of graph.children ?? []) for (const p of node.ports ?? []) pins.set(p.id, { x: node.x! + p.x!, y: node.y! + p.y! });
    const segments: Segment[] = [];
    const errors: string[] = [];
    for (const edge of result.edges) {
        for (const id of [...edge.sources, ...edge.targets]) if (!netByPin.has(id) || !pins.has(id)) errors.push(`Unknown edge terminal ${id}`);
        const nets = new Set([...edge.sources, ...edge.targets].map(id => netByPin.get(id)).filter(Boolean));
        if (nets.size !== 1) errors.push(`Edge ${edge.id} has ${nets.size} known nets`);
        const net = [...nets][0] ?? `unknown:${edge.id}`;
        if (!edge.sections?.length) errors.push(`Unrouted edge ${edge.id}`);
        for (const s of edge.sections ?? []) {
            for (const [id, point] of [[s.incomingShape ?? edge.sources[0], s.startPoint], [s.outgoingShape ?? edge.targets[0], s.endPoint]] as const) {
                const p = id ? pins.get(id) : undefined;
                if (p && key(p) !== key(point)) errors.push(`Detached endpoint ${id}`);
            }
            const points = [s.startPoint, ...(s.bendPoints ?? []), s.endPoint];
            for (let i = 1; i < points.length; i++) {
                const a = points[i - 1], b = points[i];
                if (!near(a.x, b.x) && !near(a.y, b.y)) { errors.push(`Diagonal ${edge.id}`); continue; }
                if (key(a) !== key(b)) segments.push({ a, b, net });
            }
        }
    }
    const points = new Map<string, Point>();
    for (const p of pins.values()) points.set(key(p), p);
    for (const s of segments) for (const p of [s.a, s.b]) points.set(key(p), p);
    const parent = new Map([...points.keys()].map(k => [k, k]));
    const root = (k: string): string => { const p = parent.get(k)!; if (p === k) return k; const r = root(p); parent.set(k, r); return r; };
    const join = (a: string, b: string) => parent.set(root(b), root(a));
    for (const s of segments) for (const [k, p] of points) if (on(p, s)) join(key(s.a), k);
    const flags = new Map<string, string[]>();
    for (const c of result.addedSymbol) for (const p of c.pins) {
        const point = pins.get(`${c.designator}_pin_${p.pin_number}`);
        if (!point) { errors.push(`Missing flag ${c.designator}`); continue; }
        const list = flags.get(p.signal_name) ?? [];
        list.push(key(point)); flags.set(p.signal_name, list);
    }
    // Count redundant flags on an already physically connected wire tree.
    let redundantFlags = 0;
    for (const list of flags.values()) redundantFlags += list.length - new Set(list.map(root)).size;
    // EasyEDA placeNet uses text labels for five or more unwired attachments.
    // Include only the terminals explicitly delegated by the layout, not every
    // disconnected pin with a matching name (which would hide broken routes).
    const wiredPins = new Set(result.edges.flatMap(edge => [...edge.sources, ...edge.targets]));
    const namedWirePins = new Set(result.namedWireLabels?.map(label => label.pinId) ?? []);
    for (const { pinId, signalName } of [...(result.clientManagedLabels ?? []), ...(result.namedWireLabels ?? [])]) {
        const point = pins.get(pinId);
        if (!point || netByPin.get(pinId) !== signalName || (namedWirePins.has(pinId) && !wiredPins.has(pinId))) {
            errors.push(`Invalid client label ${pinId}`);
            const p = expected.find(p => p.id === pinId);
            if (p) mismatch(p, `неверная клиентская метка ${JSON.stringify(signalName)}`);
            continue;
        }
        const list = flags.get(signalName) ?? [];
        list.push(key(point)); flags.set(signalName, list);
    }
    for (const list of flags.values()) for (const k of list.slice(1)) join(list[0], k);
    const labelsByRoot = new Map<string, Set<string>>();
    for (const [signal, list] of flags) for (const k of list) {
        const r = root(k), labels = labelsByRoot.get(r) ?? new Set<string>();
        labels.add(signal); labelsByRoot.set(r, labels);
    }
    for (const p of expected) {
        const point = pins.get(p.id);
        if (!point) { errors.push(`Missing pin ${p.id}`); mismatch(p, 'вывод отсутствует на схеме'); continue; }
        // Connectivity alone cannot detect renaming a whole isolated net (or a
        // singleton). Check the actual port/label names on its connected tree.
        for (const label of labelsByRoot.get(root(key(point))) ?? []) if (label !== p.net) {
            errors.push(`Wrong signal label ${p.id}: ${label}`);
            mismatch(p, `подключён к порту/метке ${JSON.stringify(label)}`);
        }
    }
    let pairs = 0, disagreements = 0;
    for (let i = 0; i < expected.length; i++) for (const b of expected.slice(i + 1)) {
        const a = expected[i], ap = pins.get(a.id), bp = pins.get(b.id);
        if (!ap || !bp) continue;
        pairs++;
        if ((a.net === b.net) !== (root(key(ap)) === root(key(bp)))) {
            disagreements++;
            const problem = a.net === b.net ? 'разрыв цепи' : 'замыкание разных сигналов';
            mismatch(a, problem); mismatch(b, problem);
            if (disagreements <= 12) errors.push(`Connectivity ${a.id}/${b.id}: expected ${a.net === b.net ? 'joined' : 'separate'}`);
        }
    }
    // Union collinear intervals before measuring physical ink and nearby duplicate rails.
    const lines = new Map<string, { net: string; vertical: boolean; fixed: number; intervals: number[][] }>();
    for (const s of segments) {
        const vertical = near(s.a.x, s.b.x), fixed = vertical ? s.a.x : s.a.y;
        const k = JSON.stringify([s.net, vertical, Math.round(fixed * 10000)]);
        const line = lines.get(k) ?? { net: s.net, vertical, fixed, intervals: [] };
        line.intervals.push([Math.min(vertical ? s.a.y : s.a.x, vertical ? s.b.y : s.b.x), Math.max(vertical ? s.a.y : s.a.x, vertical ? s.b.y : s.b.x)]);
        lines.set(k, line);
    }
    let physicalWireLength = 0;
    for (const line of lines.values()) {
        const merged: number[][] = [];
        for (const interval of line.intervals.sort((a, b) => a[0] - b[0])) {
            const last = merged.at(-1);
            if (last && interval[0] <= last[1] + 1e-4) last[1] = Math.max(last[1], interval[1]);
            else merged.push([...interval]);
        }
        line.intervals = merged;
        physicalWireLength += merged.reduce((s, [a, b]) => s + b - a, 0);
    }
    const lineList = [...lines.values()];
    const crossings = new Set<string>();
    for (let i = 0; i < segments.length; i++) for (const b of segments.slice(i + 1)) {
        const a = segments[i];
        if (a.net === b.net || near(a.a.x, a.b.x) === near(b.a.x, b.b.x)) continue;
        const v = near(a.a.x, a.b.x) ? a : b, h = near(a.a.x, a.b.x) ? b : a;
        const point = { x: v.a.x, y: h.a.y };
        if (on(point, a) && on(point, b)) crossings.add(JSON.stringify([[a.net, b.net].sort(), key(point)]));
    }
    let nearbySameNetParallelLength = 0;
    for (let i = 0; i < lineList.length; i++) for (const b of lineList.slice(i + 1)) {
        const a = lineList[i];
        if (a.net !== b.net || a.vertical !== b.vertical || Math.abs(a.fixed - b.fixed) > 15) continue;
        for (const x of a.intervals) for (const y of b.intervals) {
            const overlap = Math.min(x[1], y[1]) - Math.max(x[0], y[0]);
            if (overlap >= 20) nearbySameNetParallelLength += overlap;
        }
    }
    const netEdges = new Map<string, ElkExtendedEdge[]>();
    for (const edge of result.edges) {
        const net = netByPin.get(edge.sources[0]) ?? edge.id;
        const list = netEdges.get(net) ?? []; list.push(edge); netEdges.set(net, list);
    }
    const shapes = [...netEdges.values()].map(measureRouteShape);
    const routeShape = { elbows: shapes.reduce((sum, s) => sum + s.elbows, 0), shortJogs: shapes.reduce((sum, s) => sum + s.shortJogs, 0) };
    const protectedRotationChanges = fixture.circuit.components.filter(c => c.pins.length > 4 || c.designator.startsWith('U'))
        .filter(c => (result.positioned.find(p => p.designator === c.designator)?.rotate ?? 0) !== 0).map(c => c.designator);
    return { valid: errors.length === 0, errors, signalCheck: signalCheck([...differences.values()]), checkedPins: expected.length, checkedPairs: pairs, disagreements, routeShape, protectedRotationChanges,
        quality: evaluateLayoutQuality(graph), drawingBounds: drawingBounds(result), physicalWireLength: Math.round(physicalWireLength),
        differentNetCrossings: crossings.size,
        nearbySameNetParallelLength: Math.round(nearbySameNetParallelLength), redundantFlags,
        flags: result.addedSymbol.length, clientManagedLabels: result.clientManagedLabels?.length ?? 0,
        namedWireLabels: result.namedWireLabels?.length ?? 0 };
}

const escape = (s: string) => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;');
export function renderSvg(fixture: BankFixture, result: LayoutResult) {
    const graph = flatGraph(result);
    const content: string[] = [];
    const byId = new Map([...fixture.circuit.components, ...result.addedSymbol].map(c => [c.designator, c]));
    const nets = new Map<string, string>([...byId.values()].flatMap(c => c.pins.map(p => [`${c.designator}_pin_${p.pin_number}`, p.signal_name] as const)));
    const colour = (edge: ElkExtendedEdge) => /GND/i.test(nets.get(edge.sources[0]) ?? '') ? '#2f8b57' : '#286eaa';
    const wireSegments: Segment[] = [];
    for (const edge of graph.edges ?? []) for (const s of edge.sections ?? []) {
        content.push(`<polyline points="${[s.startPoint, ...(s.bendPoints ?? []), s.endPoint].map(p => `${p.x},${p.y}`).join(' ')}" stroke="${colour(edge)}" stroke-width="1.4" fill="none"/>`);
        const points = [s.startPoint, ...(s.bendPoints ?? []), s.endPoint];
        for (let i = 1; i < points.length; i++) wireSegments.push({ a: points[i - 1], b: points[i], net: nets.get(edge.sources[0]) ?? edge.id });
    }
    const junctions = new Map(wireSegments.flatMap(s => [s.a, s.b]).map(p => [key(p), p]));
    for (const p of junctions.values()) {
        const directions = new Map<string, Set<string>>();
        for (const s of wireSegments) if (on(p, s)) {
            const set = directions.get(s.net) ?? new Set<string>();
            if (near(s.a.x, s.b.x)) {
                if (Math.min(s.a.y, s.b.y) < p.y - 1e-4) set.add('N');
                if (Math.max(s.a.y, s.b.y) > p.y + 1e-4) set.add('S');
            } else {
                if (Math.min(s.a.x, s.b.x) < p.x - 1e-4) set.add('W');
                if (Math.max(s.a.x, s.b.x) > p.x + 1e-4) set.add('E');
            }
            directions.set(s.net, set);
        }
        for (const [net, set] of directions) if (set.size >= 3) content.push(`<circle cx="${p.x}" cy="${p.y}" r="2.3" fill="${/GND/i.test(net) ? '#2f8b57' : '#286eaa'}"/>`);
    }
    // Preview the labels that the EasyEDA assembler puts on retained named wires.
    // The editor chooses its own text position; this only makes the gallery readable.
    for (const { pinId, signalName } of result.namedWireLabels ?? []) {
        const candidates = (graph.edges ?? [])
            .filter(edge => edge.sources.includes(pinId) || edge.targets.includes(pinId))
            .flatMap(edge => (edge.sections ?? []).flatMap(section => {
                const points = [section.startPoint, ...(section.bendPoints ?? []), section.endPoint];
                return points.slice(1).map((point, index) => ({ a: points[index], b: point }));
            }))
            .filter(segment => Math.abs(segment.a.y - segment.b.y) < 1e-4);
        const segment = candidates.sort((a, b) => Math.abs(b.b.x - b.a.x) - Math.abs(a.b.x - a.a.x))[0];
        if (!segment) continue;
        content.push(`<text x="${(segment.a.x + segment.b.x) / 2}" y="${segment.a.y - 3}" text-anchor="middle" font-size="8" fill="#286eaa" stroke="white" stroke-width="2" paint-order="stroke">${escape(signalName)}</text>`);
    }
    for (const n of graph.children ?? []) {
        const c = byId.get(n.id);
        content.push(`<rect x="${n.x}" y="${n.y}" width="${n.width}" height="${n.height}" fill="#fffef0" stroke="#a25136"/>`);
        content.push(`<text x="${n.x! + 3}" y="${n.y! + 10}" font-size="9">${escape(c?.value ?? n.id)}</text><text x="${n.x}" y="${n.y! - 4}" font-size="10" font-weight="bold">${escape(c && !result.addedSymbol.includes(c) ? n.id : c?.value ?? n.id)}</text>`);
        for (const p of n.ports ?? []) {
            const pin = c?.pins.find(pin => String(pin.pin_number) === p.id.slice(p.id.lastIndexOf('_pin_') + 5));
            const x = n.x! + p.x!, y = n.y! + p.y!;
            content.push(`<circle cx="${x}" cy="${y}" r="1.6" fill="#a25136"/><text x="${x + (p.x! > n.width! / 2 ? -3 : 3)}" y="${y - 3}" text-anchor="${p.x! > n.width! / 2 ? 'end' : 'start'}" font-size="7">${escape(pin?.name ?? '')}</text>`);
        }
    }
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${result.width + 40}" height="${result.height + 40}" viewBox="-20 -20 ${result.width + 40} ${result.height + 40}"><rect x="-20" y="-20" width="100%" height="100%" fill="white"/><g font-family="Arial" fill="#343434">${content.join('')}</g></svg>`;
}
