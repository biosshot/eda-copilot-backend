import { minimumSpanningEdges } from '../src/pcb-layout/pcb-auto-place/ratsnest.ts';
import { getPadWorld, getBox, componentPadBox, isThroughHolePad } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { segmentIntersectsBox } from '../src/pcb-layout/pcb-auto-place/utils.ts';
const round = n => Math.round(n * 100) / 100;
export function placementMetrics(input, placements) {
    const nets = new Map(), boxes = [], pads = [];
    for (const c of input.components) {
        const p = placements.find(p => p.designator === c.designator);
        if (!p) continue;
        boxes.push(getBox(c, p));
        for (const pad of c.footprint.pads) pads.push({ box: componentPadBox(p, pad), layer: isThroughHolePad(pad) ? null : p.layer,
            ref: `${c.designator}.${pad.pin_number}`,
            net: c.pins.find(pin => String(pin.pin_number) === String(pad.pin_number))?.signal_name });
        for (const pin of c.pins) {
            if (!pin.signal_name || input.solverOptions.ignoredRatsnestSignals.some(n => n.toUpperCase() === pin.signal_name.toUpperCase())) continue;
            const q = getPadWorld(c, p, pin.pin_number);
            if (!q) continue;
            q.layer = isThroughHolePad(c.footprint.pads.find(pad => String(pad.pin_number) === String(pin.pin_number))) ? null : p.layer;
            const points = nets.get(pin.signal_name) ?? []; points.push(q); nets.set(pin.signal_name, points);
        }
    }
    const pairs = [...nets.values()].filter(p => p.length === 2);
    const segments = [...nets].flatMap(([net, ps]) => minimumSpanningEdges(ps).map(([a, b]) => [net, [ps[a], ps[b]]]));
    const wireLength = segments.reduce((total, [, [a, b]]) => total + Math.hypot(a.x - b.x, a.y - b.y), 0);
    const foreignPadHits = segments.reduce((total, [net, [a, b]]) => {
        const layer = a.layer === b.layer ? a.layer : !a.layer ? b.layer : !b.layer ? a.layer : null;
        return total + new Set(pads.filter(p => p.net !== net && (!layer || !p.layer || layer === p.layer)
            && segmentIntersectsBox(a, b, p.box)).map(p => p.ref)).size;
    }, 0);
    const hpwl = [...nets.values()].reduce((total, ps) => total + Math.max(...ps.map(p => p.x)) - Math.min(...ps.map(p => p.x))
        + Math.max(...ps.map(p => p.y)) - Math.min(...ps.map(p => p.y)), 0);
    const lengths = pairs.map(([a, b]) => Math.hypot(a.x - b.x, a.y - b.y));
    const orient = (a, b, c) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let crossings = 0;
    for (let i = 0; i < segments.length; i++) for (let j = i + 1; j < segments.length; j++) {
        if (segments[i][0] === segments[j][0]) continue;
        const [a, b] = segments[i][1], [c, d] = segments[j][1];
        if (orient(a, b, c) * orient(a, b, d) < -1e-9 && orient(c, d, a) * orient(c, d, b) < -1e-9) crossings++;
    }
    const w = Math.max(...boxes.map(b => b.right)) - Math.min(...boxes.map(b => b.left));
    const h = Math.max(...boxes.map(b => b.bottom)) - Math.min(...boxes.map(b => b.top));
    return { pairCount: pairs.length, pairSum: round(lengths.reduce((a, b) => a + b, 0)), pairMax: round(Math.max(0, ...lengths)), crossings,
        foreignPadHits, wireLength: round(wireLength), hpwl: round(hpwl), area: round(w * h), aspect: round(Math.max(w, h) / Math.min(w, h)) };
}

