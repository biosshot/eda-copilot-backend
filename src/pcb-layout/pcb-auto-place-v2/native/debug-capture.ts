import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { threadId } from 'node:worker_threads';

export type NativeDebugKind = 'block' | 'board' | 'refine';
type CaptureMeta = { stage?: string; batchSize: number; batchWallMs: number; encodeMs: number; index: number };

let sequence = 0;

/** Writes the input before entering Rust, so a hung or crashed solve remains replayable. */
export function beginNativeSolveCapture(
    kind: NativeDebugKind,
    problem: object,
    meta: Omit<CaptureMeta, 'batchWallMs'>,
): ((solution: unknown, batchWallMs: number) => void) | undefined {
    const root = process.env.PCB_LAYOUT_DEBUG_DIR;
    if (!root) return undefined;
    // JSON normally collapses -0 to 0 (and nonfinite numbers to null). Preserve
    // the exact native DTO for reproducible replays, even for invalid inputs.
    const input = JSON.stringify(problem, (_key, value: unknown) => {
        if (typeof value === 'number') {
            if (Object.is(value, -0)) return { $nativeNumber: '-0' };
            if (Number.isNaN(value)) return { $nativeNumber: 'NaN' };
            if (value === Infinity) return { $nativeNumber: 'Infinity' };
            if (value === -Infinity) return { $nativeNumber: '-Infinity' };
        }
        return value;
    });
    const hash = createHash('sha256').update(input).digest('hex');
    const label = kind === 'block'
        ? [...new Set(((problem as { components?: Array<{ blockName?: string }> }).components ?? [])
            .map(component => component.blockName).filter(Boolean))].join('+')
        : kind;
    const safeLabel = (label || 'unnamed').replace(/[^a-zA-Z0-9_-]+/g, '_').slice(0, 64);
    const id = `${String(++sequence).padStart(5, '0')}-${safeLabel}-${hash.slice(0, 10)}`;
    const directory = join(root, 'native', kind, `process-${process.pid}-thread-${threadId}`, id);
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, 'problem.json'), input);
    const metadata = {
        version: 1, kind, label, inputSha256: hash, pid: process.pid, threadId,
        ...meta,
        capturedAt: new Date().toISOString(),
    };
    writeFileSync(join(directory, 'meta.json'), JSON.stringify({ ...metadata, status: 'started' }, null, 2));
    return (solution, batchWallMs) => {
        writeFileSync(join(directory, 'solution.json'), JSON.stringify(solution));
        writeFileSync(join(directory, 'meta.json'), JSON.stringify({ ...metadata, status: 'completed', batchWallMs }, null, 2));
    };
}

/** Convenience wrapper for existing standalone diagnostic callers. */
export function captureNativeSolve(kind: NativeDebugKind, problem: object, solution: unknown, meta: CaptureMeta): void {
    beginNativeSolveCapture(kind, problem, meta)?.(solution, meta.batchWallMs);
}
