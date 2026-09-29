import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { beginNativeSolveCapture, captureNativeSolve } from '../src/pcb-layout/pcb-auto-place-v2/native/debug-capture.ts';

test('PCB native debug capture is opt-in and preserves exact encoded inputs', () => {
    const directory = mkdtempSync(join(tmpdir(), 'pcb-debug-capture-'));
    const previous = process.env.PCB_LAYOUT_DEBUG_DIR;
    try {
        delete process.env.PCB_LAYOUT_DEBUG_DIR;
        captureNativeSolve('block', { components: [{ blockName: 'example' }], coordinate: -0 },
            { rank: { score: 2 } }, { stage: 'initial', batchSize: 2, batchWallMs: 10, encodeMs: 1, index: 0 });
        assert.deepEqual(readdirSync(directory), []);

        process.env.PCB_LAYOUT_DEBUG_DIR = directory;
        captureNativeSolve('block', { components: [{ blockName: 'example' }], coordinate: -0 },
            { rank: { score: 2 } }, { stage: 'initial', batchSize: 2, batchWallMs: 10, encodeMs: 1, index: 0 });
        const processDir = join(directory, 'native', 'block', readdirSync(join(directory, 'native', 'block'))[0]);
        const captureDir = join(processDir, readdirSync(processDir)[0]);
        assert.ok(existsSync(join(captureDir, 'solution.json')));
        const input = readFileSync(join(captureDir, 'problem.json'), 'utf8');
        const meta = JSON.parse(readFileSync(join(captureDir, 'meta.json'), 'utf8'));
        assert.equal(JSON.parse(input).coordinate.$nativeNumber, '-0');
        assert.equal(meta.inputSha256, createHash('sha256').update(input).digest('hex'));
        assert.equal(meta.stage, 'initial');
        assert.equal(meta.batchSize, 2);

        const finish = beginNativeSolveCapture('board', { version: 7 },
            { batchSize: 1, encodeMs: 0.5, index: 0 });
        const boardProcess = join(directory, 'native', 'board', readdirSync(join(directory, 'native', 'board'))[0]);
        const boardCapture = join(boardProcess, readdirSync(boardProcess)[0]);
        assert.equal(JSON.parse(readFileSync(join(boardCapture, 'meta.json'), 'utf8')).status, 'started');
        assert.equal(existsSync(join(boardCapture, 'solution.json')), false);
        finish?.({ rank: { score: 1 } }, 4);
        assert.equal(JSON.parse(readFileSync(join(boardCapture, 'meta.json'), 'utf8')).batchWallMs, 4);
    } finally {
        if (previous === undefined) delete process.env.PCB_LAYOUT_DEBUG_DIR;
        else process.env.PCB_LAYOUT_DEBUG_DIR = previous;
        rmSync(directory, { recursive: true, force: true });
    }
});
