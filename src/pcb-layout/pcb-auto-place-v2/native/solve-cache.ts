import type { NativePrimitivePackSolution } from './contract.ts';
import { PCB_NUMERIC_CONTRACT } from '../../f32.ts';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { deserialize, serialize } from 'node:v8';
import { backendResource } from '#runtime/resources.ts';
import env from '#utils/env.ts';

/** Process-local, bounded LRU. Keys contain the complete encoded solver input.
 * No pose rounding or net/constraint omissions: a changed problem is a miss.
 * The byte budget estimates retained keys + serialized results; object overhead
 * is additionally bounded by the entry limit.
 */
export class NativeSolveCache {
    private readonly entries = new Map<string, { value: NativePrimitivePackSolution; bytes: number }>();
    private bytes = 0;

    constructor(private readonly maxEntries = 64, private readonly maxBytes = 16 * 1024 * 1024, private readonly disk?: DiskSolveCache) {}

    solve<T extends NativePrimitivePackSolution>(operation: string, problem: object, run: () => T): T {
        return this.solveMany(operation, [problem], () => [run()])[0];
    }

    /** Batch only misses; identical hypotheses share work and all returned values are isolated. */
    solveMany<P extends object, T extends NativePrimitivePackSolution>(operation: string, problems: P[], run: (misses: P[]) => T[]): T[] {
        const fullKeys = problems.map(problem => this.fullKey(operation, problem));
        const keys = fullKeys.map(key => key === undefined || this.maxEntries <= 0 || key.length * 2 > this.maxBytes ? undefined : key);
        const results = new Map<number, T>();
        const missing: P[] = [];
        const pending = new Map<string, number>();
        const diskPending = new Map<string, number>();
        const slots: number[] = [];
        for (let i = 0; i < problems.length; i++) {
            const key = keys[i];
            const hit = key === undefined ? undefined : this.entries.get(key);
            const stored = hit || fullKeys[i] === undefined ? undefined : this.disk?.get(fullKeys[i]!);
            if (hit && key !== undefined) {
                this.entries.delete(key);
                this.entries.set(key, hit);
                results.set(i, structuredClone(hit.value) as T);
            } else if (stored) {
                if (key !== undefined) this.put(key, stored);
                results.set(i, stored as T);
            } else {
                let slot = key === undefined ? undefined : pending.get(key);
                if (slot === undefined) {
                    slot = missing.length;
                    missing.push(problems[i]);
                    if (key !== undefined) pending.set(key, slot);
                    if (fullKeys[i] !== undefined) diskPending.set(fullKeys[i]!, slot);
                }
                slots[i] = slot;
            }
        }
        const values = missing.length ? run(missing) : [];
        if (values.length !== missing.length) throw new Error('Native batch returned an incorrect number of results');
        for (const [key, slot] of pending) this.put(key, values[slot]);
        for (const [key, slot] of diskPending) this.disk?.set(key, values[slot]);
        return problems.map((_, i) => results.get(i) ?? structuredClone(values[slots[i]]));
    }

    private fullKey(operation: string, problem: object): string | undefined {
        let cacheable = true;
        let numberIndex = 0;
        const negativeZeros: number[] = [];
        const serialized = JSON.stringify(problem, (_key, value: unknown) => {
            // Preserve signed zero in the key. Nonfinite inputs must reach native
            // validation rather than collide with JSON null. DTOs are plain data.
            if (typeof value === 'number') {
                if (!Number.isFinite(value)) cacheable = false;
                if (Object.is(value, -0)) negativeZeros.push(numberIndex);
                numberIndex++;
            }
            return value;
        });
        const key = `${PCB_NUMERIC_CONTRACT}:${operation}:${serialized}:${negativeZeros.join(',')}`;
        return cacheable ? key : undefined;
    }

    private put(key: string, value: NativePrimitivePackSolution): void {
        const bytes = 2 * (key.length + JSON.stringify(value).length);
        if (bytes <= this.maxBytes) {
            while (this.entries.size >= this.maxEntries || this.bytes + bytes > this.maxBytes) {
                const oldest = this.entries.keys().next().value!;
                this.bytes -= this.entries.get(oldest)!.bytes;
                this.entries.delete(oldest);
            }
            this.entries.set(key, { value: structuredClone(value), bytes });
            this.bytes += bytes;
        }
    }
}

/** Opt-in persistent layer (`PCB_NATIVE_SOLVE_CACHE_DIR`), shared by runs and
 * subtree processes. Keys bind the exact native binary and backend selection, so
 * a rebuilt addon never reuses older solutions. Values keep signed zeros. */
export class DiskSolveCache {
    private readonly identity: string;
    constructor(private readonly dir: string, identity?: string) {
        mkdirSync(dir, { recursive: true });
        if (identity !== undefined) { this.identity = identity; return; }
        const require = createRequire(import.meta.url);
        const { nativeFilename } = require(backendResource('native', 'node-addon', 'platform.cjs')) as { nativeFilename: () => string };
        const binary = env.PCB_BOARD_PACKER_NATIVE_PATH ? resolve(env.PCB_BOARD_PACKER_NATIVE_PATH) : backendResource('native', 'node-addon', nativeFilename());
        this.identity = createHash('sha256').update(readFileSync(binary)).update(`:${env.PCB_BLOCK_BACKEND ?? ''}:${env.PCB_BLOCK_GPU_DISABLED ?? ''}`).digest('hex');
    }
    private path(key: string) { return join(this.dir, createHash('sha256').update(this.identity).update(key).digest('hex') + '.bin'); }
    get(key: string): NativePrimitivePackSolution | undefined {
        try { return deserialize(readFileSync(this.path(key))) as NativePrimitivePackSolution; } catch { return undefined; }
    }
    set(key: string, value: NativePrimitivePackSolution) {
        const path = this.path(key), temp = `${path}.${process.pid}.tmp`;
        try { writeFileSync(temp, serialize(value)); renameSync(temp, path); } catch { /* best effort */ }
    }
}

const caches = new WeakMap<object, NativeSolveCache>();
let disk: DiskSolveCache | null | undefined;
const diskCache = () => disk === undefined ? (disk = env.PCB_NATIVE_SOLVE_CACHE_DIR ? new DiskSolveCache(resolve(env.PCB_NATIVE_SOLVE_CACHE_DIR)) : null) ?? undefined : disk ?? undefined;

export function cachedNativeSolve<T extends NativePrimitivePackSolution>(
    addon: object, operation: string, problem: object, run: () => T,
): T {
    // Diagnostic switch for cold/eager A/B measurements; no placement policy changes.
    if (process.env.PCB_NATIVE_SOLVE_CACHE === '0') return run();
    let cache = caches.get(addon);
    if (!cache) { cache = new NativeSolveCache(64, 16 * 1024 * 1024, diskCache()); caches.set(addon, cache); }
    return cache.solve(operation, problem, run);
}

export function cachedNativeSolveMany<P extends object, T extends NativePrimitivePackSolution>(
    addon: object, operation: string, problems: P[], run: (misses: P[]) => T[],
): T[] {
    if (process.env.PCB_NATIVE_SOLVE_CACHE === '0') return run(problems);
    let cache = caches.get(addon);
    if (!cache) { cache = new NativeSolveCache(64, 16 * 1024 * 1024, diskCache()); caches.set(addon, cache); }
    return cache.solveMany(operation, problems, run);
}
