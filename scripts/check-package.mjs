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
  if (/^(?:EASYEDA_COPILOT_SERVER_URL|KICAD_COPILOT_SERVER_URL|PCB_.*|VK_DRIVER_FILES|VK_ICD_FILENAMES|NODE_PATH|NODE_OPTIONS|OPENAI_API_KEY|ANTHROPIC_API_KEY|COPILOT_ROUTER_.*|KICAD_ROUTING_TOOLS_DIR|KICAD_PYTHON|PYTHONPATH|PYTHONHOME)$/i.test(key)) delete cleanEnv[key];
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
for (const file of ['dist/spec-doc.d.ts', 'dist/analysis-spec-doc.d.ts', 'dist/analyze.js', 'dist/analyze.d.ts', 'dist/run-analysis-dsl.worker.js', 'dist/run-pcb-layout.worker.js', 'dist/tree-subtree.worker.js', 'dist/run-analysis.worker.js', 'dist/analysis-runtime.js', 'native/node-addon/index.cjs', 'native/node-addon/platform.cjs', 'native/node-addon/index.d.cts', 'native/pcb-board-packer/index.d.ts', 'native/analyze/index.d.ts']) assert.ok(filenames.includes(file), file);
const { nativeFilename } = require('../native/node-addon/platform.cjs');
assert.ok(filenames.includes('native/node-addon/' + nativeFilename()), 'Host native addon missing');
assert.equal(filenames.filter(name => name.endsWith('/' + nativeFilename())).length, 1, 'Exactly one installed host addon');
assert.ok(!filenames.some(name => /^native\/(?:analyze|pcb-board-packer)\/.*\.(?:node|cjs)$/.test(name)), 'Rust libraries contain declarations only, no separate runtime addons');
assert.ok(!filenames.some(name => /^native\/.*(?:Cargo\.|\.rs$)/.test(name)), 'Rust sources are not installed runtime assets');
assert.ok(!filenames.some(name => /^(?:src|tests|\.artifacts)\/|\/target\/|\.env/.test(name)), 'Unexpected package contents');
for (const file of filenames.filter(name => name.startsWith('dist/') && /\.(?:js|ts)$/.test(name))) {
  assert.ok(!/post-place-refiner\.reference|evaluatePostPlaceBatch/.test(readFileSync(join(backend, file), 'utf8')), `Retired TypeScript refiner leaked into package: ${file}`);
}
const consumer = mkdtempSync(join(tmpdir(), 'eda-backend-package-'));
writeFileSync(join(consumer, 'package.json'), JSON.stringify({ name: 'backend-package-check', private: true, type: 'module' }));
console.log('Installing standalone backend archive into an isolated consumer...');
run([npmCli, 'install', '--ignore-scripts', '--no-audit', '--no-fund', '--fetch-timeout=30000', '--fetch-retries=0', '--registry=https://registry.npmjs.org', backendPack.archive], consumer);
const manifest = JSON.parse(readFileSync(join(consumer, 'node_modules/eda-copilot-backend/package.json'), 'utf8'));
assert.ok(!manifest.dependencies['@copilot/shared'] && !manifest.devDependencies['@copilot/shared']);
assert.deepEqual(manifest.exports['./analyze'], { types: './dist/analyze.d.ts', import: './dist/analyze.js' });
assert.ok(!manifest.exports['./analysis-runtime'], 'Low-level analysis runtime stays internal');
assert.ok(!manifest.dependencies.tsx, 'Installed workers do not require tsx');
copyFileSync(join(backend, 'tests/fixtures/api-fixtures.mjs'), join(consumer, 'fixtures.mjs'));
const localNative = require('../native/node-addon/index.cjs');
assert.equal(typeof localNative.importAnalysisBoard, 'function', 'Build the shared addon before packaging');
const analysisFixtures = ['NanoScope', 'NanoSpec', 'ESPower', 'BandAmpV5', 'USB_PD_Trigger_5A', 'STM32G0_CAN_RS485', 'TPS63060DSCR', 'ThunderF722']
  .map(name => {
    const json = readFileSync(join(backend, 'tests/pcb-analyze', name, name + '.json'), 'utf8');
    return { name, json, expected: localNative.importAnalysisBoard(json) };
  });
writeFileSync(join(consumer, 'analysis-fixtures.json'), JSON.stringify(analysisFixtures));
copyFileSync(join(backend,'tests/fixtures/analysis-dc-coupon.json'),join(consumer,'dc-fixture.json'));
copyFileSync(join(backend,'tests/fixtures/analysis-electrical.js'),join(consumer,'rlc-fixture.js'));
copyFileSync(join(backend,'tests/fixtures/analysis-thermal.js'),join(consumer,'thermal-fixture.js'));
copyFileSync(join(backend,'tests/fixtures/analysis-frequency/resources.json'),join(consumer,'frequency-resources.json'));
copyFileSync(join(backend,'tests/fixtures/analysis-frequency/network.js'),join(consumer,'frequency-fixture.js'));
copyFileSync(join(backend,'tests/fixtures/analysis-line-losses/example.js'),join(consumer,'line-losses-fixture.js'));
const geometryCoupon = JSON.parse(analysisFixtures[0].json);
geometryCoupon.pads = geometryCoupon.pads.slice(0, 1);
geometryCoupon.holes = [];
geometryCoupon.copper = { fixed: { tracks: [], vias: [], zones: [] }, editable: { tracks: [], vias: [], zones: [] } };
const geometryJson = JSON.stringify(geometryCoupon);
assert.equal(typeof localNative.prepareAnalysisGeometry, 'function', 'Rebuild the shared addon for copper geometry');
writeFileSync(join(consumer, 'geometry-fixture.json'), JSON.stringify({ json: geometryJson, expected: localNative.prepareAnalysisGeometry(geometryJson) }));
writeFileSync(join(consumer, 'smoke.mjs'), `
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { componentSearch, searchReusedBlock, extractCircuit, getPcbComponentSizes, makePcbLayout, disposeBackend, runAnalysisDsl, analysisDslDocumentation } from 'eda-copilot-backend';
import { runAnalysisDsl as runAnalysisSubpath, analysisDslDocumentation as analysisDocsSubpath, renderAnalysisReport } from 'eda-copilot-backend/analyze';
import { installEasyEdaFixture, schematicInput, pcbInput } from './fixtures.mjs';
const nativeRequire = createRequire(import.meta.url);
const backendRoot = join(process.cwd(), 'node_modules/eda-copilot-backend');
const native = nativeRequire(join(backendRoot, 'native/node-addon/index.cjs'));
const { runAnalysisProblem, terminateAnalysisPool } = await import(pathToFileURL(join(backendRoot, 'dist/analysis-runtime.js')).href);
assert.match(analysisDslDocumentation(), /ANALYZER_NOT_IMPLEMENTED/);
assert.match(analysisDslDocumentation(), /eda-copilot-backend\\/types/);
assert.match(readFileSync(join(backendRoot, 'dist/spec-doc.d.ts'), 'utf8'), /BlockOptions/);
const dsl = await runAnalysisDsl({ code: 'const a=analysis(); await Promise.resolve(); a.check({name:"retained",actual:2,max:1,unit:"V"}); const c=a.conductor({lengthMm:10,widthMm:1,thicknessMm:.035,material:{conductivitySM:58e6,state:"constant coupon"}}); const dc=await a.dc().conductor(c).currentA(1).run(); a.check({name:"physical resistance",actual:dc.resistance,target:0.0049261083743842365,tolerance:1e-14,unit:"Ohm"}); a.plot({title:"DC",data:dc.lossMap});' });
assert.equal(runAnalysisSubpath, runAnalysisDsl);
assert.equal(analysisDocsSubpath, analysisDslDocumentation);
assert.equal(renderAnalysisReport(dsl).summary.status, 'complete');
assert.equal(dsl.status, 'complete');
assert.equal(dsl.reports[0].checks[0].status, 'fail');
assert.equal(dsl.reports[0].checks[1].status, 'pass');
assert.equal(dsl.reports[0].plots[0].data.validity.status, 'valid');
const rlcDsl=await runAnalysisDsl({code:readFileSync('rlc-fixture.js','utf8')},{maxOutputBytes:16*1024*1024});
assert.equal(rlcDsl.status,'complete',JSON.stringify(rlcDsl.error));
assert.deepEqual(rlcDsl.reports[0].calculations.filter(c=>c.phase==='completed').map(c=>c.result.solution.mode),['dc','frequency','time','time']);
assert.deepEqual(rlcDsl.reports[0].checks.map(c=>c.status),['pass','pass']);
assert.equal(rlcDsl.reports[0].plots.length,5);
assert.equal(typeof native.solveAnalysisThermal,'function');
const thermalDsl=await runAnalysisDsl({code:readFileSync('thermal-fixture.js','utf8')},{maxOutputBytes:8*1024*1024});
assert.equal(thermalDsl.status,'complete',JSON.stringify(thermalDsl.error));
assert.deepEqual(thermalDsl.reports[0].checks.map(c=>c.status),['pass','pass','fail','pass']);
const thermalRuns=thermalDsl.reports[0].calculations.filter(c=>c.phase==='completed');
assert.equal(thermalRuns.length,3);assert.ok(Math.abs(thermalRuns[0].result.solution.temperaturesC.junction[0]-55)<1e-8);
assert.equal(thermalRuns[1].result.solution.axis.length,201);assert.equal(thermalRuns[2].result.solution.axis[0],100);
assert.ok(thermalRuns.every(c=>c.result.solution.quality.normalizedEnergyBalance<=1e-6));
assert.equal(thermalDsl.reports[0].plots.at(-1).data.validity.status,'unavailable');
const dcBoard=JSON.parse(readFileSync('dc-fixture.json','utf8'));
const frequency=await runAnalysisDsl({code:readFileSync('frequency-fixture.js','utf8'),models:JSON.parse(readFileSync('frequency-resources.json','utf8'))},{maxOutputBytes:16*1024*1024});
assert.equal(frequency.status,'complete',JSON.stringify(frequency.error));
assert.deepEqual(frequency.reports.flatMap(r=>r.checks).map(c=>c.status),Array.from({length:5},()=>['fail','pass']).flat());
const frequencyRuns=frequency.reports.flatMap(r=>r.calculations).filter(c=>c.phase==='completed');
assert.equal(frequencyRuns.length,5);assert.ok(frequencyRuns.every(c=>c.result.solution.quality.maxResidual<=1e-8));
assert.ok(Math.abs(frequencyRuns[1].result.solution.parameters[0][1][0].real-.2/.94)<1e-10);
const sectionDsl=await runAnalysisDsl({code:'const a=analysis();const line=a.line({kind:"microstrip",widthMm:.3,copperThicknessMm:.035,dielectricHeightMm:.18,lengthMm:20,material:{relativePermittivity:4.2,lossTangent:0,state:"Explicit ideal PEC hypothesis"}});const z=await a.impedance().target(line).method("cross-section").frequencyHz(1e8).run();a.check({name:"positive Z0",actual:z.characteristicImpedance,min:1,unit:"Ohm"});const s=await a.networkParameters().network(z.network).frequencyHz(1e8).representation("S").referenceOhm(50).run();a.plot({title:"field-derived S21",data:s.parameter({out:"output",in:"input"}).phaseDeg()});'},{maxOutputBytes:64*1024*1024});
assert.equal(sectionDsl.status,'complete',JSON.stringify(sectionDsl.error));assert.equal(sectionDsl.reports[0].checks[0].status,'pass');
const sectionRuns=sectionDsl.reports[0].calculations.filter(c=>c.phase==='completed');assert.equal(sectionRuns[0].result.solution.status,'complete');assert.ok(sectionRuns[0].result.solution.field.quality.actual.normalizedResidual<=1e-8);assert.deepEqual(sectionRuns[1].result.solution.parameters,sectionRuns[0].result.solution.network.parameters);
const lineLosses=await runAnalysisDsl({code:readFileSync('line-losses-fixture.js','utf8')},{maxOutputBytes:64*1024*1024});
assert.equal(lineLosses.status,'complete',JSON.stringify(lineLosses.error));
const lossRuns=lineLosses.reports[0].calculations.filter(c=>c.phase==='completed');assert.equal(lossRuns.length,2);assert.equal(lossRuns[0].result.characteristicImpedance.kind,'series');assert.equal(lossRuns[0].result.rlgc.data.samples.length,11);assert.ok(lossRuns[0].result.solution.network.maxPowerBalanceResidual<=1e-8);assert.ok(lossRuns[0].result.rlgc.data.samples.every(s=>s.surfaceContributions.find(v=>v.id==='bottom').resistanceOhmPerM[0][0]>0));assert.deepEqual(lossRuns[0].result.solution.network.parameters,lossRuns[1].result.solution.parameters);
const boardDsl=await runAnalysisDsl({board:dcBoard,code:'const a=analysis({board});const s=a.port({positive:pad("J1","1"),negative:pad("J1","2")});const p=a.port({positive:pad("U1","1"),negative:pad("U1","2")});const base=a.dc().stackup(stackup({layers:[{kind:"copper",name:"TOP",thicknessMm:.035,material:"cu"}]})).materials({cu:{conductivitySM:58e6,state:"constant coupon"}}).sources([{port:s,voltageV:1,model:"ideal"}]).mesh({maxAreaMm2:.125,minAngleDeg:0,maxRefinementSteps:64});const unused=a.port({positive:[pad("J1","1"),pad("U1","1")],negative:pad("J1","2")});const observed=a.port({positive:padId("rail"),negative:padId("return")});for(const currentA of [1,2]){const d=await base.loads([{port:p,currentA}]).run();a.check({name:"passive board voltage",actual:d.voltage(observed),target:1-currentA*8e-3/(58e6*2e-3*.035e-3),tolerance:1e-8,unit:"V"});a.check({name:"physical board voltage",actual:d.voltage(p),target:1-currentA*16e-3/(58e6*2e-3*.035e-3),tolerance:1e-8,unit:"V"});a.plot({title:"Actual board current",data:d.currentDensityMap});}'});
assert.equal(boardDsl.status,'complete');
assert.deepEqual(boardDsl.reports[0].checks.map(c=>c.status),['pass','pass','pass','pass']);
const boardDcRuns=boardDsl.reports[0].calculations.filter(c=>c.phase==='completed');
assert.equal(boardDcRuns[0].result.solution.status,'complete');
assert.ok(boardDcRuns[0].result.solution.copper_loss_w>0);
assert.equal(boardDcRuns[1].result.solution.reused_factorization,true);
assert.equal(native.postPlaceRefineContractVersion(), 3, 'Installed package must contain the F32 Rust refiner');
assert.equal(native.numericContract(), 'f32-rte-ftz-v1');
assert.equal(native.placementScoreContractVersion(), 13);
assert.equal(typeof native.refinePostPlacement, 'function');
assert.deepEqual(Object.keys(native).sort(), ${JSON.stringify(Object.keys(localNative).sort())});
assert.equal(native.importAnalysisBoard('{').imported, false);
try {
for (const { name, json, expected } of JSON.parse(readFileSync('analysis-fixtures.json', 'utf8'))) {
  assert.deepEqual(native.importAnalysisBoard(json), expected, 'Installed analysis import: ' + name);
  const progress = [];
  assert.deepEqual(await runAnalysisProblem({ kind: 'import-board', boardJson: json }, { onProgress: event => progress.push(event.phase) }), expected, 'Installed analysis worker: ' + name);
  assert.deepEqual(progress, ['queued', 'started', 'completed']);
}
assert.equal((await runAnalysisProblem({ kind: 'import-board', boardJson: '{' })).imported, false);
const geometry = JSON.parse(readFileSync('geometry-fixture.json', 'utf8'));
assert.deepEqual(native.prepareAnalysisGeometry(geometry.json), geometry.expected);
const validationRequest={board_json:geometry.json,rules:[{kind:'connectivity'},{kind:'clearance',minMm:.2},{kind:'edge-clearance',minMm:.3}]};
const validation=native.validateAnalysisGeometry(validationRequest);
assert.equal(validation.status,'partial');
assert.ok(validation.coverage.some(c=>c.rule==='connectivity'&&c.opens==='unavailable'));
const workerValidation=await runAnalysisProblem({kind:'geometry-validation',request:validationRequest});
delete validation.elapsed_ms;delete workerValidation.elapsed_ms;
assert.deepEqual(workerValidation,validation);
const geometryDsl=await runAnalysisDsl({board:JSON.parse(geometry.json),code:'const a=analysis({board});const g=await a.geometry().rules([{kind:"connectivity"},{kind:"clearance",minMm:.2},{kind:"edge-clearance",minMm:.3}]).run();a.check({name:"incomplete coverage",actual:g.violations.count(),equals:0});a.plot({title:"geometry measurements",data:g.measurements});'});
assert.equal(geometryDsl.status,'complete',JSON.stringify(geometryDsl.error));assert.equal(geometryDsl.reports[0].checks[0].status,'unknown');
assert.deepEqual(await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: geometry.json }), geometry.expected);
assert.equal((await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: '{' })).status, 'invalid');
const conduction = await runAnalysisProblem({ kind: 'prepare-conduction', boardJson: geometry.json, mesh: { min_angle_deg: 0 } });
const directConduction = native.prepareAnalysisConduction(geometry.json, { mesh: { min_angle_deg: 0 } });
const semantic = ({ timings_ms, serialized_bytes, ...rest }) => rest;
assert.deepEqual(semantic(conduction), semantic(directConduction));
assert.ok(conduction.counts.built_domains > 0, 'Installed worker actually meshes copper');
assert.ok(conduction.domains.every(d => !d.readiness.dc_ready), 'No assumed conductivity in installed smoke');
assert.equal((await runAnalysisProblem({ kind: 'prepare-conduction', boardJson: '{' })).status, 'invalid');
await terminateAnalysisPool();
assert.equal((await runAnalysisProblem({ kind: 'import-board', boardJson: '{}' })).imported, false);
} finally { await terminateAnalysisPool(); }
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
console.log('Installed backend: component search, schematic, native PCB worker, analysis workers, thermal DSL and assets passed.');
`);
process.stdout.write(run([join(consumer, 'smoke.mjs')], consumer, cleanEnv));
console.log('Checking the installed archive with GPU disabled and with both GPU drivers unavailable...');
for (const options of [
  { PCB_BLOCK_GPU_DISABLED: '1' },
  { PCB_GPU_RUNTIME: 'auto', PCB_GPU_CUDA_DISABLED: '1', VK_DRIVER_FILES: join(consumer, 'nonexistent-vulkan-driver.json') },
]) {
  process.stdout.write(run([join(consumer, 'smoke.mjs')], consumer, {
    ...cleanEnv, PCB_BLOCK_BACKEND: 'cubecl', PCB_BOARD_BACKEND: 'cubecl', PCB_POST_PLACE_BACKEND: 'cubecl', ...options,
  }));
}
writeFileSync(join(consumer, 'consumer.ts'), `
import { componentSearch, extractCircuit, makePcbLayout, getPcbComponentSizes, disposeBackend } from 'eda-copilot-backend';
import type { CircuitMod, FootprintSpec } from 'eda-copilot-backend/types';
import native = require('./node_modules/eda-copilot-backend/native/node-addon/index.cjs');
declare const analysisApi:ReturnType<typeof analysis>;
const localNetwork=analysisApi.model({id:'types-coupon',kind:'network',ports:['p'],data:{format:'network-parameters',representation:'S',frequenciesHz:[1e9],parameters:[[[{real:0,imaginary:0}]]],referenceOhm:50,referenceTopology:'common',nature:'passive'}});
const typedResource=localNetwork;
const typedLine=analysisApi.line({kind:'coupled-stripline',widthMm:.2,gapMm:.3,copperThicknessMm:.035,lengthMm:3,topDielectricHeightMm:.3,bottomDielectricHeightMm:.3,topMaterial:{relativePermittivity:4,lossTangent:0,state:'ideal'},bottomMaterial:{relativePermittivity:4,lossTangent:0,state:'ideal'}});
const sectionBuilder=analysisApi.impedance().target(typedLine).method('cross-section').mesh({maxNodes:200000,maxRefinements:5}).frequencyHz(1e8);
const typedLossLine=analysisApi.line({kind:'microstrip',widthMm:.3,copperThicknessMm:.035,dielectricHeightMm:.18,material:{relativePermittivity:4.2,lossTangent:.015,state:'constant band'},conductors:{signal:{conductivitySM:58e6,state:'smooth fixed temperature'},referencePlanes:{bottom:{thicknessMm:.035,material:{conductivitySM:58e6,state:'smooth fixed temperature'}}}}});
analysisApi.impedance().target(typedLossLine).method('cross-section').frequencyHz(1e9);
async function extractedLineTypes(){const z=await sectionBuilder.run();const network=z.network;const data=z.rlgc;analysisApi.networkParameters().network(network).frequencyHz(1e8).representation('S').referenceOhm(50);return data;}

analysisApi.network({model:localNetwork,ports:{input:'p'}});
const textNetwork=analysisApi.model({id:'text-coupon',kind:'network',ports:['p'],data:{format:'touchstone',content:'# Hz S RI\\n1000000000 0 0\\n',referenceTopology:'common',nature:'passive'}});
analysisApi.network({model:textNetwork,ports:{input:'p'}});
analysisApi.model({kind:'resistor',resistanceOhm:1});
const imported: boolean = native.importAnalysisBoard('{}').imported;
const sourceId: string | null = native.importAnalysisBoard('{}').diagnostics[0].source_id;
const existingContract: number = native.contractVersion();
const geometryStatus: 'complete' | 'partial' | 'invalid' = native.prepareAnalysisGeometry('{}', { max_deviation_mm: 0.001 }).status;
const validationStatus: 'complete' | 'partial' | 'rejected' = native.validateAnalysisGeometry({board_json:'{}',rules:[{kind:'connectivity'}]}).status;
const conductionStatus: 'complete' | 'partial' | 'invalid' = native.prepareAnalysisConduction('{}', { mesh: { max_area_mm2: 1 } }).status;
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
