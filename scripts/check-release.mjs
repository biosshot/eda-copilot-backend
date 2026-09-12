import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const read = name => JSON.parse(readFileSync(join(root, name), 'utf8'));
const pkg = read('package.json');
assert.equal(pkg.name, 'eda-copilot-backend');
assert.ok(!pkg.private, 'Backend must be publishable');
assert.ok(!pkg.workspaces, 'Backend must be standalone');
for (const group of ['dependencies', 'devDependencies']) {
  for (const [name, spec] of Object.entries(pkg[group] ?? {})) {
    assert.ok(name !== '@copilot/shared', 'Shared workspace dependency must not return');
    assert.ok(!/^(?:file:|link:|workspace:|git|https?:|\.\.?[\\/])/.test(spec), `${name}: local dependency ${spec}`);
  }
}
for (const [path, entry] of Object.entries(read('package-lock.json').packages)) {
  assert.ok(!path.startsWith('../') && !entry.link, `Non-portable lockfile entry: ${path}`);
}
function scan(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) scan(path);
    else if (/\.(?:ts|js)$/.test(entry.name)) assert.ok(!readFileSync(path, 'utf8').includes('@copilot/shared'), `External shared import: ${path}`);
  }
}
scan(join(root, 'src'));
scan(join(root, 'dist'));
const platforms = ['win32-x64-msvc', 'linux-x64-gnu', 'darwin-x64', 'darwin-arm64'];
for (const platform of platforms) {
  const filename = join(root, 'native/pcb-board-packer', `pcb-board-packer.${platform}.node`);
  assert.ok(statSync(filename).size > 0, `Missing native binary: ${platform}`);
}
if (process.env.GITHUB_REF?.startsWith('refs/tags/')) {
  assert.equal(process.env.GITHUB_REF, `refs/tags/v${pkg.version}`, 'Tag/package version mismatch');
}
console.log('Standalone package and all four release binaries verified.');
