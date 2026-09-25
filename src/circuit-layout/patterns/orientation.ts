import { effectiveLayoutArea } from '../quality.ts';
import { createMacroInstance, rotateSymbolGeometry } from './helpers.ts';
import type { MacroInstance, OrthogonalSide } from './types.ts';

/** Consider the permitted shapes before ELK freezes the group's footprint.
 * Large passive banks otherwise cannot turn in the bounded local refiner. */
export function orientPassiveMacro(input: MacroInstance): MacroInstance {
    if (input.layoutChildBlock || input.placements.some(p => /^U/i.test(p.designator) || p.pins.length > 2)) return input;
    const width = input.node.symbol.width, height = input.node.symbol.height;
    if (height < width * 4) return input;
    const rotation = input.refinementRotations?.find(r => r !== 180);
    if (!rotation || effectiveLayoutArea(height, width) >= effectiveLayoutArea(width, height) * 0.85) return input;
    const sides: OrthogonalSide[] = ['NORTH', 'WEST', 'SOUTH', 'EAST'];
    const side = (s: OrthogonalSide) => sides[(sides.indexOf(s) + rotation / 90) % 4];
    const point = (p: { x: number; y: number }) => rotation === 90 ? { x: p.y, y: width - p.x } : { x: height - p.y, y: p.x };
    const placements = input.placements.map(p => {
        const center = point({ x: p.x + p.width / 2, y: p.y + p.height / 2 });
        const geometry = rotateSymbolGeometry({ width: p.width, height: p.height, center: p.center, pins: p.pins }, rotation);
        return { ...p, x: center.x - geometry.width / 2, y: center.y - geometry.height / 2,
            width: geometry.width, height: geometry.height, center: geometry.center, rotate: (p.rotate + rotation) % 360,
            pins: p.pins.map((pin, i) => ({ ...pin, x: geometry.pins[i].x, y: geometry.pins[i].y, side: side(pin.side) })) };
    });
    const ports = input.ports.map(p => ({ ...p, ...point(p), side: side(p.side), terminalSide: side(p.terminalSide),
        tailBendPoints: p.tailBendPoints?.map(point) }));
    return { ...input, ...createMacroInstance({ ...input, width: height, height: width, placements, ports }),
        routedPaths: input.routedPaths.map(p => ({ ...p, points: p.points.map(point) })) };
}
