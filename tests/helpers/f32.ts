/** Diagnostic score bound only; never use this for hard geometry assertions. */
export function f32ScoreTolerance(a: number, b: number): number {
    const value = Math.max(Math.abs(a), Math.abs(b));
    const data = new Float32Array([value]);
    const bits = new Uint32Array(data.buffer);
    bits[0] += 1;
    return Math.max(1e-3, 4 * (data[0] - Math.fround(value)));
}
