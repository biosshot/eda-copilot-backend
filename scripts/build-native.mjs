import { spawnSync } from 'node:child_process';
import { copyFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const native = fileURLToPath(new URL('../native/pcb-board-packer/', import.meta.url));
const { nativeFilename } = createRequire(import.meta.url)('../native/pcb-board-packer/platform.cjs');
const filename = nativeFilename();
const rustTarget = process.platform === 'win32'
  ? `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-pc-windows-msvc` : null;
const args = ['build', '--release', '--locked', '--manifest-path', join(native, 'Cargo.toml')];
const buildEnv = { ...process.env };
if (rustTarget) {
  // Explicit target keeps host proc-macros dynamic while the addon embeds CRT.
  args.push('--target', rustTarget);
  const key = `CARGO_TARGET_${rustTarget.toUpperCase().replaceAll('-', '_')}_RUSTFLAGS`;
  buildEnv[key] = `${buildEnv[key] ?? ''} -C target-feature=+crt-static`.trim();
}
const result = spawnSync('cargo', args, {
  cwd: native, env: buildEnv, stdio: 'inherit', windowsHide: true,
});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const library = process.platform === 'win32' ? 'pcb_board_packer.dll'
  : process.platform === 'darwin' ? 'libpcb_board_packer.dylib' : 'libpcb_board_packer.so';
const target = resolve(native, process.env.CARGO_TARGET_DIR || 'target');
copyFileSync(join(target, ...(rustTarget ? [rustTarget] : []), 'release', library), join(native, filename));
console.log(`Built ${filename}`);
