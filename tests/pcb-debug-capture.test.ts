import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { beginNativeSolveCapture, captureNativeSolve, nativeBackendRequest } from '../src/pcb-layout/pcb-auto-place-v2/native/debug-capture.ts';

test('capture records backend requests without overriding auto or explicit GPU/CPU choices', () => {
    const env = { PCB_BLOCK_BACKEND: 'cubecl', PCB_POST_PLACE_BACKEND: 'cpu', PCB_BOARD_PACKER_THREADS: '6' };
    const before = { ...env };
    assert.deepEqual(nativeBackendRequest('board', env), { variable: 'PCB_BOARD_BACKEND', requested: 'auto', explicit: false,
        threadsVariable: 'PCB_BOARD_PACKER_THREADS', threads: '6' });
    assert.equal(nativeBackendRequest('block', env).requested, 'cubecl');
    assert.equal(nativeBackendRequest('refine', env).requested, 'cpu');
    assert.equal(nativeBackendRequest('board', { PCB_BOARD_BACKEND: 'cubecl' }).explicit, true);
    assert.deepEqual(env, before);
});

test('capture summary keeps actual native backend choices, fallback reasons and board detail', async () => {
    const { nativeProfile } = await import('../scripts/debug-pcb-layout.mjs');
    const directory = mkdtempSync(join(tmpdir(), 'pcb-debug-profile-'));
    try {
        const file = join(directory, 'run.log');
        writeFileSync(file, [
            '[board-backend] {"backend":"cpu","requested":"auto","reason":"outside measured board GPU workload/thread threshold"}',
            '[board-gpu-fallback] {"reason":"device unavailable","replay":"original board native call"}',
            '[board-detail] {"totals":{"full_rank":{"calls":10,"workerMs":12}},"note":"worker time"}',
            '[board-backend] {partial',
            '[pcb-board-packer] beam 369.442s, threads=6, hard=9, joint candidates=0',
        ].join('\n'));
        const profile = nativeProfile(file);
        assert.equal(profile.backendDecisions.length, 2);
        assert.equal(profile.backendDecisions[0].requested, 'auto');
        assert.equal(profile.backendDecisions[0].backend, 'cpu');
        assert.equal(profile.backendDecisions[1].reason, 'device unavailable');
        assert.equal(profile.boardDetails[0].totals.full_rank.workerMs, 12);
        assert.match(profile.boardStageLines[0], /369.442s/);
    } finally {
        rmSync(directory, { recursive: true, force: true });
    }
});

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
        assert.deepEqual(meta.backendRequest, nativeBackendRequest('block'));

        const finish = beginNativeSolveCapture('board', { version: 7 },
            { batchSize: 1, encodeMs: 0.5, index: 0 });
        const boardProcess = join(directory, 'native', 'board', readdirSync(join(directory, 'native', 'board'))[0]);
        const boardCapture = join(boardProcess, readdirSync(boardProcess)[0]);
        assert.equal(JSON.parse(readFileSync(join(boardCapture, 'meta.json'), 'utf8')).status, 'started');
        assert.deepEqual(JSON.parse(readFileSync(join(boardCapture, 'meta.json'), 'utf8')).backendRequest, nativeBackendRequest('board'));
        assert.equal(existsSync(join(boardCapture, 'solution.json')), false);
        finish?.({ rank: { score: 1 } }, 4);
        assert.equal(JSON.parse(readFileSync(join(boardCapture, 'meta.json'), 'utf8')).batchWallMs, 4);
    } finally {
        if (previous === undefined) delete process.env.PCB_LAYOUT_DEBUG_DIR;
        else process.env.PCB_LAYOUT_DEBUG_DIR = previous;
        rmSync(directory, { recursive: true, force: true });
    }
});
