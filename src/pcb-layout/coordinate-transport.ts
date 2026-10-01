import { add, f32, roundPlacement } from './f32.ts';

/** Coordinate transport only. Computational geometry uses the local F32 part.
 * Whole-mm origins retain the phase of the 0.001-mm placement grid. */
export function coordinateOrigin(value: number): number {
    return Math.abs(value) <= 1024 ? 0 : Math.round(value);
}

export function coordinateAdd(absolute: number, offset: number): number {
    const origin = coordinateOrigin(absolute);
    return add(f32(absolute - origin), offset) + origin;
}
/** Authored shape construction must localize even a modest absolute offset;
 * a micron-sized pad can disappear at 1000 mm as well as at 1e6 mm. */
export function sourceCoordinateAdd(absolute: number, offset: number): number {
    const origin = Math.round(absolute);
    return add(f32(absolute - origin), offset) + origin;
}

export function coordinateDifference(a: number, b: number): number {
    const origin = Math.round(b);
    return f32(f32(a - origin) - f32(b - origin));
}

export function coordinateRound(absolute: number): number {
    const origin = coordinateOrigin(absolute);
    return roundPlacement(f32(absolute - origin)) + origin;
}

export function coordinateCenter(a: number, b: number): number {
    const origin = coordinateOrigin(a);
    return f32(add(f32(a - origin), f32(b - origin)) / 2) + origin;
}
