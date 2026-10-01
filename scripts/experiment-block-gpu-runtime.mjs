// Release CubeCL runtime checks; this does not measure block-solver speed.
import { readFileSync, writeFileSync, mkdirSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const addonPath = resolve(process.argv[2] ?? 'debugging/cubecl-block-migration-2026-09-30/runtime/packer.node');
const mode = process.argv[3];
const out = resolve(process.argv[4] ?? 'debugging/cubecl-block-migration-2026-09-30/runtime');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
if (mode) {
  const addon = createRequire(import.meta.url)(addonPath);
  assert.equal(typeof addon.solveBlockPrimitives, 'function');
  assert.equal(typeof addon.blockGpuProbe, 'function');
  if (mode === 'import-disabled') {
    console.log(JSON.stringify({ mode, imported: true }));
  } else if (mode === 'disabled') {
    assert.throws(() => addon.blockGpuProbe([1], false), /GPU disabled/);
    console.log(JSON.stringify({ mode, rejected: true, processAlive: true }));
  } else if (mode === 'panic') {
    assert.throws(() => addon.blockGpuProbe([1], true), /injected GPU runtime panic/);
    assert.throws(() => addon.blockGpuProbe([1], false), /injected GPU runtime panic/);
    console.log(JSON.stringify({ mode, panicContained: true, disabledAfterFailure: true, processAlive: true }));
  } else {
    const calls = [];
    for (const count of [4097, 1, 8193, 129]) {
      const input = Array.from({ length: count }, (_, i) => 16777216 + i * 0.25);
      const result = addon.blockGpuProbe(input, false);
      assert.equal(result.precision, 'f64');
      assert.equal(result.initializations, 1);
      assert.deepEqual(result.values, input.map(v => (v + 0.125) * 2));
      calls.push({ count, device: result.device, initializations: result.initializations });
    }
    console.log(JSON.stringify({ mode, exactF64: true, calls }));
  }
} else {
  mkdirSync(out, { recursive: true });
  const rows = [];
  for (const check of ['normal', 'import-disabled', 'disabled', 'panic']) {
    const child = spawnSync(process.execPath, [process.argv[1], addonPath, check, out], {
      encoding: 'utf8', windowsHide: true, timeout: 120000,
      env: { ...process.env, PCB_BLOCK_GPU_DISABLED: check.includes('disabled') ? '1' : '0' },
    });
    writeFileSync(resolve(out, `${check}.log`), `${child.stdout ?? ''}\n${child.stderr ?? ''}`);
    rows.push({ mode: check, exitCode: child.status, signal: child.signal, error: child.error?.message,
      result: child.status === 0 ? JSON.parse(child.stdout.trim()) : null });
  }
  const files = ['native/pcb-board-packer/Cargo.toml', 'native/pcb-board-packer/Cargo.lock',
    'native/pcb-board-packer/src/compute/mod.rs', 'native/pcb-board-packer/src/compute/gpu.rs',
    'native/pcb-board-packer/src/compute/workspace.rs', 'native/pcb-board-packer/src/compute/numerics.rs', 'native/pcb-board-packer/src/lib.rs',
    'scripts/build-native.mjs', 'scripts/experiment-block-gpu-runtime.mjs'];
  const report = { scope: 'runtime smoke only; no solver performance claim', addonPath,
    addonSha256: hash(readFileSync(addonPath)), bytes: statSync(addonPath).size,
    sources: Object.fromEntries(files.map(p => [p, hash(readFileSync(p))])), rows };
  writeFileSync(resolve(out, 'results.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  assert(rows.every(r => r.exitCode === 0), 'runtime checks failed; inspect per-check logs');
}
