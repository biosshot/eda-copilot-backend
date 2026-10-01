import { F32_ROTATION_BITS as SIN_COS_BITS } from './f32-rotation-bits.ts';

export const PCB_NUMERIC_CONTRACT = 'f32-rte-ftz-v1';
const MIN_NORMAL = 2 ** -126;

/** Binary32 operands/results, with signed FTZ. JSON number remains transport. */
export function f32(value: number): number {
    const result = Math.fround(value);
    return result !== 0 && Math.abs(result) < MIN_NORMAL ? (result < 0 ? -0 : 0) : result;
}
export const add = (a: number, b: number) => f32(f32(a) + f32(b));
export const sub = (a: number, b: number) => f32(f32(a) - f32(b));
export const mul = (a: number, b: number) => f32(f32(a) * f32(b));
export const div = (a: number, b: number) => f32(f32(a) / f32(b));
export const sqrt = (a: number) => f32(Math.sqrt(f32(a)));
export const abs = (a: number) => Math.abs(f32(a));
export const min = (...values: number[]) => Math.min(...values.map(f32));
export const max = (...values: number[]) => Math.max(...values.map(f32));
export const hypot = (...values: number[]) => sqrt(values.reduce((sum, value) => add(sum, mul(value, value)), 0));
export const pow = (a: number, b: number) => f32(Math.pow(f32(a), f32(b)));
export const mod = (a: number, b: number) => f32(f32(a) % f32(b));
export const sin = (a: number) => f32(Math.sin(f32(a)));
export const cos = (a: number) => f32(Math.cos(f32(a)));
export const atan2 = (a: number, b: number) => f32(Math.atan2(f32(a), f32(b)));

/** Same F32 scale -> half towards +Infinity -> ticks -> RTE quotient as Rust. */
export function roundPlacement(value: number): number {
    const ticks = Math.round(mul(value, 1000));
    return ticks === 0 ? 0 : div(ticks, 1000);
}

const rotation = new Float32Array(720);
const bits = new Uint32Array(rotation.buffer);
for (let angle = 0; angle < 360; angle += 1) {
    bits[angle * 2] = SIN_COS_BITS[angle][0];
    bits[angle * 2 + 1] = SIN_COS_BITS[angle][1];
}

export function sinCosDegrees(angle: number): readonly [number, number] {
    if (!Number.isInteger(angle)) throw new Error(`PCB rotation must be an integer degree: ${angle}`);
    const index = ((angle % 360) + 360) % 360;
    return [rotation[index * 2], rotation[index * 2 + 1]];
}

export function rotatePoint(point: { x: number; y: number }, angle: number) {
    switch (((angle % 360) + 360) % 360) {
        case 0: return { x: f32(point.x), y: f32(point.y) };
        case 90: return { x: -f32(point.y), y: f32(point.x) };
        case 180: return { x: -f32(point.x), y: -f32(point.y) };
        case 270: return { x: f32(point.y), y: -f32(point.x) };
    }
    const [sin, cos] = sinCosDegrees(angle);
    return { x: sub(mul(point.x, cos), mul(point.y, sin)),
        y: add(mul(point.x, sin), mul(point.y, cos)) };
}

/** Noninteger offsets are permitted only for the existing soft orientation cost. */
export function cosDegrees(angle: number): number {
    return Number.isInteger(angle) ? sinCosDegrees(angle)[1] : cos(mul(angle, Math.fround(Math.PI / 180)));
}
