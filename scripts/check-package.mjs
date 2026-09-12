import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, copyFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const backend = fileURLToPath(new URL('../', import.meta.url));
const artifacts = join(backend, '.artifacts');
mkdirSync(artifacts, { recursive: true });
const npmCli = process.env.npm_execpath;
assert.ok(npmCli, 'Run this check with npm run test:package');
const require = createRequire(import.meta.url);
const cleanEnv = { ...process.env };
for (const key of Object.keys(cleanEnv)) {
  if (/^(?:EASYEDA_COPILOT_SERVER_URL|KICAD_COPILOT_SERVER_URL|PCB_BOARD_PACKER_NATIVE_PATH|NODE_PATH|NODE_OPTIONS|OPENAI_API_KEY|ANTHROPIC_API_KEY|COPILOT_ROUTER_.*|KICAD_ROUTING_TOOLS_DIR|KICAD_PYTHON|PYTHONPATH|PYTHONHOME)$/i.test(key)) delete cleanEnv[key];
}
function run(args, cwd, env = process.env, timeout = 180_000) {
  const result = spawnSync(process.execPath, args, { cwd, encoding: 'utf8', env, windowsHide: true, timeout });
  if (result.error) throw new Error(`${result.error.message}\n${result.stdout}\n${result.stderr}`);
  assert.equal(result.status, 0, `${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
function pack(cwd) {
  const packed = JSON.parse(run([npmCli, 'pack', '--ignore-scripts', '--json', '--pack-destination', artifacts], cwd));
  const [result] = Array.isArray(packed) ? packed : Object.values(packed);
  return { ...result, archive: join(artifacts, result.filename) };
}

const backendPack = pack(backend);
const filenames = backendPack.files.map(file => file.path);
for (const file of ['dist/spec-doc.d.ts', 'dist/run-pcb-layout.worker.js', 'dist/tree-subtree.worker.js', 'native/pcb-board-packer/index.cjs', 'native/pcb-board-packer/platform.cjs']) assert.ok(filenames.includes(file), file);
const { nativeFilename } = require('../native/pcb-board-packer/platform.cjs');
assert.ok(filenames.includes('native/pcb-board-packer/' + nativeFilename()), 'Host native solver missing');
assert.ok(!filenames.some(name => /^(?:src|tests)\/|\/target\/|\.env/.test(name)), 'Unexpected package contents');
const consumer = mkdtempSync(join(tmpdir(), 'eda-backend-package-'));
writeFileSync(join(consumer, 'package.json'), JSON.stringify({ name: 'backend-package-check', private: true, type: 'module' }));
console.log('Installing standalone backend archive into an isolated consumer...');
run([npmCli, 'install', '--ignore-scripts', '--no-audit', '--no-fund', '--fetch-timeout=30000', '--fetch-retries=0', '--registry=https://registry.npmjs.org', backendPack.archive], consumer);
const manifest = JSON.parse(readFileSync(join(consumer, 'node_modules/eda-copilot-backend/package.json'), 'utf8'));
assert.ok(!manifest.dependencies['@copilot/shared'] && !manifest.devDependencies['@copilot/shared']);
copyFileSync(join(backend, 'tests/fixtures/api-fixtures.mjs'), join(consumer, 'fixtures.mjs'));
writeFileSync(join(consumer, 'smoke.mjs'), `
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { componentSearch, searchReusedBlock, extractCircuit, getPcbComponentSizes, makePcbLayout, disposeBackend } from 'eda-copilot-backend';
import { installEasyEdaFixture, schematicInput, pcbInput } from './fixtures.mjs';
const transport = installEasyEdaFixture();
try {
  assert.deepEqual(await searchReusedBlock({ query: 'power' }), []);
  assert.ok((await componentSearch({ MPN: 'TEST-1K' })).components.length);
  const schematic = await extractCircuit(schematicInput);
  assert.ok(schematic.circuit.components.some(c => c.designator === 'R1'));
  assert.equal((await getPcbComponentSizes(pcbInput)).report.selected, 2);
  const events = [];
  const board = await makePcbLayout(pcbInput, { onProgress: p => events.push(p) });
  assert.ok(board.pcb, board.content);
  assert.equal(events.at(-1).stage, 'done');
  const failed = await makePcbLayout({ ...pcbInput, code: 'nonexistent();' });
  assert.match(failed.content, /nonexistent/);
} finally { transport.restore(); await disposeBackend(); }
console.log('Installed backend: component search, schematic, native PCB worker and assets passed.');
`);
process.stdout.write(run([join(consumer, 'smoke.mjs')], consumer, cleanEnv));
writeFileSync(join(consumer, 'consumer.ts'), `
import { componentSearch, extractCircuit, makePcbLayout, getPcbComponentSizes, disposeBackend } from 'eda-copilot-backend';
import type { CircuitMod, FootprintSpec } from 'eda-copilot-backend/types';
const circuit: CircuitMod = { add_components: [], add_reused_blocks: [], rm_components: null, external_rm_connect: null, external_connect: null };
const footprint: FootprintSpec = { name: 'local', width: 2, height: 1, pads: [] };
componentSearch({ MPN: 'TEST' });
extractCircuit({ circuit });
getPcbComponentSizes({ circuit: { components: [] }, footprints: { local: footprint } });
makePcbLayout({ code: 'board.rect(20, 10);', circuit: { components: [] } }, { onProgress: progress => { const percent: number = progress.progress; } });
disposeBackend();
`);
run([require.resolve('typescript/bin/tsc'), '--noEmit', '--strict', '--target', 'ES2022', '--module', 'NodeNext', '--moduleResolution', 'NodeNext', 'consumer.ts'], consumer, cleanEnv);
writeFileSync(join(artifacts, 'package-check.json'), JSON.stringify({ backend: backendPack.filename, consumer, size: backendPack.size, unpackedSize: backendPack.unpackedSize, checkedAt: new Date().toISOString() }, null, 2));
console.log('Standalone backend package checks passed: ' + consumer);
