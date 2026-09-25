import { RouteQueue } from './route-queue.ts';
import { type Box, type Point, EPS } from './geometry.ts';
import { SCHEMATIC_CLEARANCE as gap } from './policy.ts';

/** Bounded visibility-grid fallback for a complete port-row candidate. The
 * ordinary local search keeps its cheap one-channel routes. */
export function routeChannels(start: Point, end: Point, boxes: Box[], clear: (a: Point, b: Point) => boolean) {
    const axis = (key: 'x' | 'y', size: 'width' | 'height') => {
        const values = [...new Set([start[key], end[key], ...boxes.flatMap(b => [b[key] - gap.pinEscape, b[key] + b[size] + gap.pinEscape])])];
        const distance = (v: number) => Math.max(0, Math.min(start[key], end[key]) - v, v - Math.max(start[key], end[key]));
        return values.sort((a, b) => Number(b === start[key] || b === end[key]) - Number(a === start[key] || a === end[key])
            || distance(a) - distance(b) || a - b).slice(0, 48).sort((a, b) => a - b);
    };
    const xs = axis('x', 'width'), ys = axis('y', 'height');
    const sx = xs.indexOf(start.x), sy = ys.indexOf(start.y), ex = xs.indexOf(end.x), ey = ys.indexOf(end.y);
    const id = (x: number, y: number, d: number) => `${x}:${y}:${d}`;
    const initial = id(sx, sy, 2), queue = new RouteQueue(), costs = new Map([[initial, 0]]), previous = new Map<string, string>();
    const heuristic = (x: number, y: number) => Math.abs(xs[x] - end.x) + Math.abs(ys[y] - end.y);
    queue.push(initial, heuristic(sx, sy));
    const checked = new Map<string, boolean>();
    let expanded = 0;
    while (queue.size && expanded++ < 4000) {
        const current = queue.pop(), [x, y, d] = current.id.split(':').map(Number), cost = costs.get(current.id)!;
        if (current.distance > cost + heuristic(x, y) + EPS) continue;
        if (x === ex && y === ey) {
            const points: Point[] = []; let cursor: string | undefined = current.id;
            while (cursor) { const [a, b] = cursor.split(':').map(Number); points.push({ x: xs[a], y: ys[b] }); cursor = previous.get(cursor); }
            return points.reverse();
        }
        for (const [nx, ny, nd] of [[x - 1, y, 0], [x + 1, y, 0], [x, y - 1, 1], [x, y + 1, 1]]) {
            if (nx < 0 || ny < 0 || nx >= xs.length || ny >= ys.length) continue;
            const key = [`${x}:${y}`, `${nx}:${ny}`].sort().join('|');
            if (!checked.has(key)) checked.set(key, clear({ x: xs[x], y: ys[y] }, { x: xs[nx], y: ys[ny] }));
            if (!checked.get(key)) continue;
            const next = id(nx, ny, nd), value = cost + Math.abs(xs[x] - xs[nx]) + Math.abs(ys[y] - ys[ny]) + (d !== 2 && d !== nd ? gap.pinEscape : 0);
            if (value >= (costs.get(next) ?? Infinity) - EPS) continue;
            costs.set(next, value); previous.set(next, current.id); queue.push(next, value + heuristic(nx, ny));
        }
    }
    return null;
}
