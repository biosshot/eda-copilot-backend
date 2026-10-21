import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

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
const staleBinaries = [];
for (const platform of platforms) {
  const filename = join(root, 'native/node-addon', `eda-copilot-node.${platform}.node`);
  assert.ok(statSync(filename).size > 0, `Missing native binary: ${platform}`);
  // Foreign binaries cannot execute on this host. Reject known legacy addons;
  // their actual contract values still require the corresponding CI runner.
  const binary = readFileSync(filename);
  const missing = ['numericContract', 'placementScoreContractVersion', 'postPlaceRefineContractVersion', 'importAnalysisBoard']
    .filter(name => !binary.includes(Buffer.from(name)));
  if (missing.length) staleBinaries.push(`${platform}: missing ${missing.join(', ')}`);
}
assert.deepEqual(staleBinaries, [], 'Legacy release binaries must be rebuilt on their target platforms');
const require = createRequire(import.meta.url);
const native = require(join(root, 'native/node-addon/index.cjs'));
assert.equal(native.numericContract(), 'f32-rte-ftz-v1', 'Host numeric contract');
assert.equal(native.placementScoreContractVersion(), 13, 'Host score contract');
assert.equal(native.postPlaceRefineContractVersion(), 3, 'Host refiner contract');
assert.equal(native.importAnalysisBoard('{').imported, false, 'Host analysis import diagnostics');
if (process.env.GITHUB_REF?.startsWith('refs/tags/')) {
  assert.equal(process.env.GITHUB_REF, `refs/tags/v${pkg.version}`, 'Tag/package version mismatch');
}
console.log('Standalone package, native API markers and host contracts verified. Target-platform CI is still required.');
