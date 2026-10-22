import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { createAnalysisApi } from '../src/analyze/analysis-dsl/context.ts';
import { value, unavailable } from '../src/analyze/analysis-dsl/data.ts';
import { schemas } from '../src/analyze/analysis-dsl/schema.ts';
import type { AnalysisTask, ExecutionResult } from '../src/analyze/analysis-dsl/executor.ts';
import type { AnalysisContext, RoutingBoardV2 } from '../src/analyze/analysis-dsl/spec-doc.js';
import { runAnalysisDsl, analysisDslDocumentation } from '../src/analyze/run-analysis-dsl.ts';
import * as examples from './analysis-dsl/examples.ts';
import { conductionProfile } from '../src/analyze/analysis-dsl/profile-adapter.ts';

const options = { filename: 'test-analysis.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 };
function board(): RoutingBoardV2 {
  const b = JSON.parse(readFileSync('tests/fixtures/conduction-contact-coupon.json', 'utf8')).board as RoutingBoardV2;
  return { ...b, layers: [{ name: 'TOP', index: 0, side: 'top' }, { name: 'BOTTOM', index: 1, side: 'bottom' }],
    components: ['J1', 'U1', 'U2', 'C1', 'ANT1'].map(designator => ({ designator, at: { x: 0, y: 0 }, rotationDeg: 0, side: 'top' })),
    nets: ['VCC', 'GND', 'P', 'N'].map(name => ({ name })),
    pads: ['J1', 'U1', 'U2', 'C1', 'ANT1'].flatMap(component => ['VCC', 'GND', 'P', 'N', 'VCC', 'GND'].map((net, index) => ({ id: `${component}-${index + 1}`, component, number: String(index + 1), net, at: { x: index, y: 0 }, rotationDeg: 0, layers: ['TOP'], shape: { kind: 'circle', diameterMm: 1 } }))),
  };
}
function context(b?: RoutingBoardV2) {
  const api = createAnalysisApi({ board: b }, options, () => {});
  return { api, a: b ? api.globals.analysis({ board: b }) : api.globals.analysis() };
}
const wire = `const a=analysis(); const c=a.conductor({lengthMm:10,widthMm:1,thicknessMm:.035,material:{conductivitySM:58e6}});`;

test('production runner awaits JS, retains failed/unknown checks and explicit solver failure diagnostics', async () => {
  const result = await runAnalysisDsl({ code: `${wire}
    a.check({name:'fail', actual:2,max:1,unit:'V'});
    a.check({name:'unknown',actual:NaN,min:0,unit:'V'});
    await Promise.resolve(); a.log({level:'info',message:'after checks'});
    await a.dc().conductor(c).currentA(1).run();` });
  assert.equal(result.status, 'failed'); assert.equal(result.error?.code, 'ANALYZER_NOT_IMPLEMENTED');
  assert.deepEqual(result.reports[0].summary, { pass: 0, fail: 1, unknown: 1 });
  assert.equal((result.reports[0].logs[0] as { message: string }).message, 'after checks');
  assert.ok(result.reports[0].diagnostics.some(d => d.code === 'ANALYZER_NOT_IMPLEMENTED'));
  assert.ok(result.error?.line);
});

test('production equals checks reject incompatible units without stopping execution', async () => {
  const result = await runAnalysisDsl({ code: `const a=analysis();
    const actual={kind:'scalar',unit:'1',value:1,validity:{status:'valid'},diagnostics:[]};
    a.check({name:'mismatch',actual,equals:1,unit:'V'});
    a.log({level:'info',message:'after mismatch'});
    a.check({name:'implicit dimensionless',actual,equals:1});
    a.check({name:'explicit dimensionless',actual,equals:1,unit:'1'});
    a.check({name:'different value',actual,equals:2,unit:'1'});
    a.check({name:'unavailable',actual:{...actual,validity:{status:'unavailable',reason:'not evaluated'}},equals:1,unit:'1'});
    a.check({name:'range',actual,min:0,max:2,unit:'1'});
    a.check({name:'range mismatch',actual,min:0,max:2,unit:'V'});` });
  assert.equal(result.status, 'complete');
  assert.deepEqual(result.reports[0].checks.map(c => c.status), ['unknown', 'pass', 'pass', 'fail', 'unknown', 'pass', 'unknown']);
  assert.match(result.reports[0].checks[0].message, /Expected unit V; actual unit 1/);
  assert.equal((result.reports[0].logs[0] as { message: string }).message, 'after mismatch');
});

test('prepare diagnostics are retained when subsequent user code fails', async () => {
  const result = await runAnalysisDsl({ code: 'const a=analysis();await a.dc().prepare();\nthrow Error("later user error");' });
  assert.equal(result.error?.line, 2);
  const preparation = (result.reports[0].calculations[0] as unknown as { preparation: { structurallyValid: boolean; diagnostics: { code: string }[] } }).preparation;
  assert.equal(preparation.structurallyValid, false);
  assert.ok(preparation.diagnostics.some(d => d.code === 'MISSING_MODEL'));
});

test('injected executor: dependent await, actual results/check/plot, callbacks remain JS, frozen tasks', async () => {
  const tasks: AnalysisTask[] = [];
  const result = await executeScenario({ code: `${wire}
    const base=a.dc().conductor(c).currentA(1).temperatureC(25);
    const first=await base.run();
    a.check({name:'resistance',actual:first.resistance.max(),target:.25,tolerance:0,unit:'Ohm'});
    const second=await a.dc().conductor(c).voltageV(first.resistance.value*4).run();
    const cases=await a.sweep({parameter:{name:'amps',unit:'A',values:[1,2]},evaluate:async current => {
      const r=await base.currentA(current).run(); return r.resistance;
    }});
    a.plot({title:'cases',kind:'table',data:cases.series});
    a.check({name:'unknown map',actual:second.lossMap.max(),max:1,unit:'W'});
  ` }, options, () => {}, async task => {
    assert.ok(Object.isFrozen(task)); assert.ok(Object.isFrozen(task.config));
    assert.doesNotThrow(() => structuredClone(task));
    assert.ok(!JSON.stringify(task).includes('evaluate'));
    tasks.push(task);
    return { id: `test-${tasks.length}`, status: 'complete', method: 'explicit-test-executor', diagnostics: [], assumptions: ['test only'], metrics: { resistance: { kind: 'scalar', unit: 'Ohm', value: .25, validity: { status: 'valid' }, diagnostics: [] } } };
  });
  assert.equal(result.status, 'complete', JSON.stringify(result.error));
  assert.equal(tasks.length, 4); assert.equal(tasks[1].config.voltageV, 1);
  assert.equal(tasks[1].config.temperatureC, undefined);
  assert.deepEqual(tasks.slice(2).map(t => t.config.currentA), [1, 2]);
  assert.deepEqual(result.reports[0].summary, { pass: 1, fail: 0, unknown: 1 });
  assert.equal((result.reports[0].plots[0] as { rendered: boolean }).rendered, false);
  assert.ok(!JSON.stringify(result.reports).includes('file://'));
});

test('builders snapshot arrays on setter and run, repeated setter replaces, await builder does not run', async () => {
  const tasks: AnalysisTask[] = [];
  let release: () => void = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  const api = createAnalysisApi({ board: board() }, options, () => {}, async task => { tasks.push(task); await gate; return { id: 'test', status: 'complete', method: 'test', diagnostics: [], assumptions: [], metrics: {} }; });
  const a = api.globals.analysis({ board: board() });
  const p = a.port({ positive: api.globals.pad('J1', 1), negative: api.globals.pad('J1', 2) });
  const sources = [{ port: p, voltageV: 3.3, model: 'ideal' as const }];
  const base = a.dc().sources(sources).loads([]);
  const replaced = base.sources([{ port: p, voltageV: 5, model: 'ideal' }]);
  sources[0].voltageV = 99;
  await base; assert.equal(tasks.length, 0);
  const run = base.run(); sources.push({ port: p, voltageV: 10, model: 'ideal' });
  const run2 = replaced.run(); release(); await Promise.all([run, run2]);
  assert.equal(tasks[0].config.sources?.length, 1); assert.equal(tasks[0].config.sources?.[0].voltageV, 3.3);
  assert.equal(tasks[1].config.sources?.length, 1); assert.equal(tasks[1].config.sources?.[0].voltageV, 5);
});

test('finite API vocabulary agrees with schemas; excluded legacy names fail', async () => {
  const { a } = context();
  for (const [kind, schema] of Object.entries(schemas)) {
    const builder = a[kind as keyof typeof schemas]();
    assert.deepEqual(Object.getOwnPropertyNames(builder).sort(), [...Object.keys(schema.shape), 'prepare', 'run'].sort());
    assert.equal('then' in builder, false);
  }
  for (const method of ['channel', 'transmitter', 'receiver', 'aggressors', 'stimulus', 'solver']) {
    const r = await executeScenario({ code: `analysis().si().${method}({});` }, options);
    assert.equal(r.status, 'failed', method);
  }
  for (const method of ['assign', 'converters', 'timing', 'ddr']) {
    const r = await executeScenario({ code: `analysis().${method}({});` }, options);
    assert.equal(r.status, 'failed', method);
  }
});

test('missing sections, units, modes and fabricated targets are rejected without defaults', async () => {
  const { a } = context();
  assert.equal((await a.dc().prepare()).structurallyValid, false);
  assert.equal((await a.si().prepare()).structurallyValid, false);
  const c = a.conductor({ lengthMm: 10, widthMm: 1, thicknessMm: .035, material: { conductivitySM: 58e6 } });
  assert.equal((await a.dc().conductor(c).currentA(1).voltageV(5).prepare()).structurallyValid, false);
  assert.throws(() => a.dc().temperatureC('25C' as never), /number/);
  assert.throws(() => a.dc().conductor(c).budget({ seconds: 1 } as never), /Unrecognized/);
  assert.throws(() => a.line({ kind: 'microstrip', widthMm: .3, copperThicknessMm: .035, dielectricHeightMm: .18, material: {}, ignored: 1 } as never), /Unrecognized/);
  assert.throws(() => a.impedance().target({ arbitrary: 1 } as never), /Invalid/);
  assert.throws(() => a.model({ kind: 'driver', lowV: 0, highV: 3.3, outputResistanceOhm: 20 } as never));
  assert.throws(() => a.model({ kind: 'capacitor', capacitanceF: 1e-6 }), /parasitics/);
  const p = await a.dc().conductor(c).currentA(1).prepare();
  assert.equal(p.structurallyValid, true); assert.equal(p.physicalReady, false); assert.equal(p.executorAvailable, false);
});

test('boardless analysis does not capture global board; unknown and ambiguous references fail', async () => {
  const b = board(), { api } = context(b);
  assert.equal((await api.globals.analysis().geometry().rules([]).prepare()).structurallyValid, false);
  assert.throws(() => api.globals.analysis({ board: undefined as never }), /explicit board/);
  const a = api.globals.analysis({ board: b });
  assert.throws(() => a.port({ positive: api.globals.pad('missing', 1), negative: api.globals.pad('J1', 2) }), /0 physical pads/);
  assert.throws(() => a.geometry().scope([api.globals.net('AGND')]), /Unknown/);
  const duplicate = { ...b, pads: [...b.pads, { ...b.pads[0], id: 'duplicate' }] };
  const d = api.globals.analysis({ board: duplicate });
  assert.throws(() => d.port({ positive: api.globals.pad('J1', 1), negative: api.globals.pad('J1', 2) }), /2 physical pads/);
  assert.doesNotThrow(() => d.port({ positive: api.globals.padId('J1-1'), negative: api.globals.padId('J1-2') }));
});

test('component equal residuals are independent for supply/return and retain explicit zero', async () => {
  const { api, a } = context(board());
  const s = a.port({ positive: api.globals.pad('J1', 1), negative: api.globals.pad('J1', 2) });
  const load = { designator: 'U1', rail: api.globals.net('VCC'), currentA: .1, pinCurrentsA: { '1': .04 }, returnPinCurrentsA: { '2': 0 } };
  const p = await a.dc().sources([{ port: s, voltageV: 3.3, model: 'ideal' }]).loads([load]).prepare();
  assert.equal(p.structurallyValid, true, JSON.stringify(p.diagnostics));
  const distribution = (p.components[0] as unknown as { distributions: { pins: { currentA: number; source: string }[] }[] }).distributions;
  assert.deepEqual(distribution.map(s => s.pins.map(p => p.currentA)), [[.04, .060000000000000005], [0, .1]]);
  assert.equal(distribution[1].pins[0].source, 'manual');
  const bad = await a.dc().sources([{ port: s, voltageV: 3.3, model: 'ideal' }]).loads([{ ...load, pinCurrentsA: { '1': .2 } }]).prepare();
  assert.equal(bad.structurallyValid, false);
  const outside = await a.dc().sources([{ port: s, voltageV: 3.3, model: 'ideal' }]).loads([{ ...load, pins: ['1'], pinCurrentsA: { '5': .04 } }]).prepare();
  assert.equal(outside.structurallyValid, false);
  assert.ok(outside.diagnostics.some(d => d.code === 'MANUAL_PIN_GROUP'));
});

test('manual component groups and duplicate components are not silently repaired', async () => {
  const { api, a } = context(board());
  const c = api.globals.ic({ designator: 'U1', ground: [], supplies: { io: ['1'] } });
  const p = await a.dc().components([c, c]).sources([]).loads([]).prepare();
  assert.equal(p.structurallyValid, false); assert.ok(p.diagnostics.some(d => d.code === 'DUPLICATE_COMPONENT'));
  const p2 = await a.dc().components([c]).sources([]).loads([{ designator: 'U1', rail: api.globals.net('VCC'), currentA: .1 }]).prepare();
  assert.equal(p2.structurallyValid, false); assert.ok(p2.diagnostics.some(d => d.message.includes('No return pads')));
});

test('descriptors validate graph nodes, network ownership/model ports and context identity', () => {
  const { a } = context();
  assert.throws(() => a.circuit({ nodes: ['n'], ports: [], elements: [{ kind: 'resistor', id: 'R', nodes: ['n', 'x'], resistanceOhm: 1 }] }), /reference/);
  assert.throws(() => a.thermalNetwork({ nodes: [{ id: 'a' }], links: [{ from: 'a', to: 'b', resistanceKPerW: 1 }], sources: [] }), /Unknown thermal node/);
  const n = a.network({ model: { id: 'rf', kind: 'network', ports: ['one', 'two'] }, ports: { input: 'one', output: 'two' } });
  assert.throws(() => n.port('foo'), /Unknown port/);
  assert.throws(() => a.network({ model: { id: 'rf', kind: 'network', ports: ['one'] }, ports: { output: 'two' } }), /undeclared/);
  const another = context().a;
  assert.throws(() => another.networkParameters().network(n), /another analysis|Unknown or incompatible/);
  assert.doesNotThrow(() => a.connect({ networks: [n], connections: [], ports: { input: n.port('input') } }));
  const one = a.scene({ model: { id: 'one', kind: 'scene', ports: ['feed'] } });
  const two = a.scene({ model: { id: 'two', kind: 'scene', ports: ['feed'] } });
  assert.throws(() => a.antenna({ structure: one, feed: two.port('feed') }), /does not belong/);
});

test('checks preserve units, validity and unknown data; result operations do not invent math', () => {
  const { a } = context();
  const real = value({ kind: 'scalar', unit: 'V', value: 3.3, validity: { status: 'conditional', domain: { approximation: 'provided model' } }, diagnostics: [] });
  assert.equal(a.check({ name: 'valid approximation', actual: real.min(), min: 3, unit: 'V' }).status, 'pass');
  assert.equal(a.check({ name: 'units', actual: real, min: 3, unit: 'A' }).status, 'unknown');
  assert.equal(a.check({ name: 'missing', actual: unavailable('V', 'not evaluated'), min: 0, unit: 'V' }).status, 'unknown');
  assert.equal(a.check({ name: 'config', actual: 1, min: 2, max: 0, unit: 'V' }).status, 'unknown');
  assert.equal(a.check({ name: 'no condition', actual: 1 }).status, 'unknown');
  assert.equal(real.abs().validity.status, 'unavailable');
  assert.equal(real.at({ frequencyHz: 1 }).validity.status, 'unavailable');
  assert.doesNotThrow(() => JSON.stringify(a.report()));
});

test('sweep executes full cartesian/zip and continues callback failures; search never calls fake evaluation', async () => {
  const { a } = context(); let calls = 0;
  const s = await a.sweep({ parameters: { x: { values: [1, 2] }, y: { values: [10, 20] } }, mode: 'cartesian', evaluate: async ({ x, y }) => { calls++; if (x === 1 && y === 20) throw Error('case failed'); return Number(x) + Number(y); } });
  assert.equal(calls, 4); assert.equal(s.attempts.filter(a => a.status === 'failed').length, 1);
  const zip = await a.sweep({ parameters: { x: { values: [1, 2] }, y: { values: [10, 20] } }, mode: 'zip', evaluate: async ({ x }) => Number(x) });
  assert.equal(zip.attempts.length, 2);
  await assert.rejects(a.solve({ parameter: { name: 'x', unit: 'mm', min: 1, max: 2 }, evaluate: () => { calls++; return value({ kind: 'scalar', unit: 'Ohm', value: 50, validity: { status: 'valid' }, diagnostics: [] }); }, target: 50, tolerance: 1, unit: 'Ohm', maxEvaluations: 10 }), { code: 'ANALYZER_NOT_IMPLEMENTED' });
  await assert.rejects(a.optimize({ parameters: { x: { values: [1, 2] } }, evaluate: () => { calls++; return {}; }, objective: { metric: 'loss', direction: 'minimize', unit: 'dB' }, constraints: [], maxEvaluations: 2, seed: 1 }), { code: 'ANALYZER_NOT_IMPLEMENTED' });
  assert.equal(calls, 4);
});

test('syntax and runtime errors preserve filename, user line and column', async () => {
  for (const [code, line, errorCode] of [['const a=analysis();\nconst = 1;', 2, 'SYNTAX_ERROR'], ['const a=analysis();\nawait Promise.resolve();\nmissingName();', 3, 'RUNTIME_ERROR']] as const) {
    const r = await runAnalysisDsl({ code }, { filename: 'my-scenario.js' });
    assert.equal(r.error?.code, errorCode); assert.equal(r.error?.filename, 'my-scenario.js');
    assert.equal(r.error?.line, line, JSON.stringify(r)); assert.ok(r.error?.column);
  }
  const collision = await runAnalysisDsl({ code: '\nanalysis().dc().temperatureC("hot");' }, { filename: 'run-analysis-dsl.worker.js' });
  assert.equal(collision.error?.line, 2, 'An internal file suffix must not be mistaken for the user script');
  const eof = await runAnalysisDsl({ code: 'const a =' });
  assert.equal(eof.error?.line, 1); assert.equal(eof.error?.column, 'const a ='.length + 1);
});

test('deadline terminates loops before/after await; abort terminates loop; later scenario recovers', { timeout: 20_000 }, async () => {
  for (const prefix of ['', 'await Promise.resolve();']) {
    const r = await runAnalysisDsl({ code: `const a=analysis(); a.log({level:'info',message:'before loop'}); ${prefix} while(true){}` }, { timeoutMs: 1000 });
    assert.equal(r.error?.code, 'TIMEOUT'); assert.equal(r.reports[0].logs.length, 1);
  }
  const controller = new AbortController();
  const run = runAnalysisDsl({ code: 'const a=analysis(); await Promise.resolve(); while(true){}' }, { signal: controller.signal, timeoutMs: 10_000 });
  const timer = setTimeout(() => controller.abort(), 700);
  const aborted = await run; clearTimeout(timer);
  assert.equal(aborted.error?.code, 'ABORTED');
  const recovered = await runAnalysisDsl({ code: 'analysis().log({level:"info",message:"recovered"});' });
  assert.equal(recovered.status, 'complete');
});

test('globals are fresh, no process/require/fetch exposed; output limits preserve bounded prior log', async () => {
  const r = await runAnalysisDsl({ code: 'globalThis.leak=1; const a=analysis(); a.check({name:"globals",actual:typeof process+typeof require+typeof fetch,equals:"undefinedundefinedundefined"});' });
  assert.equal(r.reports[0].checks[0].status, 'pass');
  const next = await runAnalysisDsl({ code: 'analysis().check({name:"fresh",actual:typeof leak,equals:"undefined"});' });
  assert.equal(next.reports[0].checks[0].status, 'pass');
  const limited = await runAnalysisDsl({ code: 'const a=analysis(); for(let i=0;i<10;i++)a.log({level:"info",message:String(i)});' }, { maxEntries: 3 });
  assert.equal(limited.error?.code, 'OUTPUT_LIMIT'); assert.equal(limited.reports[0].logs.length, 3);
});

test('standalone DSL documentation ships beside PCB docs and declares complete vocabulary', () => {
  const doc = analysisDslDocumentation();
  assert.match(doc, /ANALYZER_NOT_IMPLEMENTED/); assert.match(doc, /eda-copilot-backend\/types/);
  assert.ok(!doc.includes('../../types/'));
  assert.ok(readFileSync('dist/spec-doc.d.ts', 'utf8').includes('BlockOptions'));
  assert.equal(readFileSync('dist/spec-doc.d.ts', 'utf8'), readFileSync('src/pcb-layout/pcb-layout-dsl/spec-doc.d.ts', 'utf8'));
  for (const kind of Object.keys(schemas)) assert.ok(doc.includes(`${kind}(): Builder<'${kind}'>`), kind);
});

test('typed complete scenarios compile against the standalone declarations', () => {
  const result = spawnSync(process.execPath, ['node_modules/typescript/bin/tsc', '--noEmit', '--strict', '--target', 'ES2022', '--module', 'NodeNext', '--moduleResolution', 'NodeNext', '--allowImportingTsExtensions', 'tests/analysis-dsl/examples.ts'], { encoding: 'utf8', windowsHide: true, timeout: 30_000 });
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

test('stackup adapters preserve existing contracts and do not synthesize material state/bore conventions', () => {
  const { api } = context();
  const stack = api.globals.stackup({ viaPlatingThicknessUm: 25, layers: [{ kind: 'copper', name: 'TOP', thicknessOz: 1, material: 'cu' }, { kind: 'dielectric', name: 'core', thicknessMm: 1 }, { kind: 'copper', name: 'BOTTOM', thicknessMm: .035, material: 'cu' }] });
  assert.equal(stack.routing.layers[0].thicknessMm, .03479);
  const partial = conductionProfile(stack, { cu: { conductivitySM: 58e6 } })!;
  assert.equal(partial.materials, undefined); assert.deepEqual(partial.barrel, { plating_thickness_mm: .025 });
  const explicit = conductionProfile(stack, { cu: { conductivitySM: 58e6, state: 'constant at 25C' } })!;
  assert.deepEqual(explicit.materials?.TOP, { conductivity_s_m: 58e6, state: 'constant at 25C' });
  assert.equal(explicit.barrel?.diameter_convention, undefined);
  assert.throws(() => api.globals.stackup({ layers: [{ kind: 'copper', name: 'TOP', thicknessMm: .035, thicknessOz: 1 }] }), /Choose/);
  assert.throws(() => api.globals.stackup({ boardThicknessMm: 5, layers: [{ kind: 'copper', name: 'TOP', thicknessMm: .035 }] }), /layer sum/);
});

test('environment functions have explicit resources and preserve time conventions', () => {
  const { api } = context(); const p = api.globals.profiles;
  assert.deepEqual(Object.keys(p.environments), [], 'project profiles are not invented');
  const step = p.environment({ name: 'step', ambientC: { kind: 'step', from: -20, to: 70, atS: 60, riseS: 5 } });
  assert.equal(p.sample({ environment: step, atS: 0 }), -20);
  assert.equal(p.sample({ environment: step, atS: 62.5 }), 25);
  assert.equal(p.sample({ environment: step, atS: 70 }), 70);
  assert.throws(() => p.environment({ name: 'bad', ambientC: { kind: 'pwl', points: [[1, 0], [2, 1]], interpolation: 'linear' } }), /PWL/);
});

test('cooling characteristic tables are typed descriptions, with no hidden evaluation or mode merging', () => {
  const { a } = context();
  assert.deepEqual(a.model({ kind: 'cooling', table: [{ deltaTemperatureK: 0, heatFlowW: 0 }, { deltaTemperatureK: 20, heatFlowW: 2 }] }), { kind: 'cooling', table: [{ deltaTemperatureK: 0, heatFlowW: 0 }, { deltaTemperatureK: 20, heatFlowW: 2 }] });
  assert.throws(() => a.model({ kind: 'cooling', table: [{ deltaTemperatureK: 20, heatFlowW: 2 }, { deltaTemperatureK: 0, heatFlowW: 0 }] }), /strictly increasing/);
  assert.throws(() => a.model({ kind: 'cooling', resistanceKPerW: 10, table: [{ deltaTemperatureK: 0, heatFlowW: 0 }, { deltaTemperatureK: 20, heatFlowW: 2 }] } as never), /Choose/);
});

test('variants retain intent and stable selectors, never mutate the source board or claim a rebuild', async () => {
  const b = board(), original = structuredClone(b), { api, a } = context(b);
  const candidate = a.variant({ changes: [{ kind: 'track-width', target: api.globals.net('P'), widthMm: .8 }] });
  const p = await candidate.geometry().rules([{ kind: 'connectivity' }]).prepare();
  assert.equal(p.structurallyValid, true); assert.equal(p.physicalReady, false);
  assert.ok(p.diagnostics.some(d => d.code === 'VARIANT_REBUILD_REQUIRED'));
  assert.deepEqual(b, original);
  await assert.rejects(candidate.geometry().rules([]).run(), { code: 'ANALYZER_NOT_IMPLEMENTED' });
  assert.throws(() => candidate.thermal().initial({ kind: 'finalState', contextId: a.report().contextId, analysisId: 'previous', data: {} }), /incompatible|another analysis/);
});

test('named network/result references flow to dependent analyses without serialization of functions', async () => {
  const calls: AnalysisTask[] = [];
  const r = await executeScenario({ code: `
    const a=analysis();
    const line=a.line({kind:'microstrip',widthMm:.3,copperThicknessMm:.035,dielectricHeightMm:.18,material:{relativePermittivity:4.2},lengthMm:30});
    const extraction=await a.impedance().target(line).method('cross-section').frequencyHz(1e9).run();
    const result=await a.networkParameters().network(extraction.network).frequencyHz(1e9).representation('S').referenceOhm(50).run();
    a.check({name:'not calculated',actual:result.parameter({out:'output',in:'input'}).magnitude().max(),max:1,unit:'1'});
  ` }, options, () => {}, async task => {
    calls.push(task);
    return { id: String(calls.length), status: 'complete', method: 'test only', diagnostics: [], assumptions: [], metrics: {}, ...(task.kind === 'impedance' ? { network: { id: 'extracted-test', kind: 'network', ports: ['input', 'output'] } } : {}) };
  });
  assert.equal(r.status, 'complete', JSON.stringify(r.error)); assert.equal(calls.length, 2);
  assert.ok(calls[1].descriptors.some(d => JSON.stringify(d).includes('extracted-test')));
  assert.equal(r.reports[0].checks[0].status, 'unknown');
});

test('incompatible resource analyses, wrong spectrum units, SI model bindings and reference maps fail prepare', async () => {
  const { a } = context();
  const n = a.network({ model: { id: 'n', kind: 'network', ports: ['in', 'out'] }, ports: { input: 'in', output: 'out' } });
  const np = await a.networkParameters().network(n).frequencyHz(1e6).representation('S').referenceOhm({ typo: 50 }).prepare();
  assert.equal(np.structurallyValid, false);
  const si = await a.si().network(n).drivers([{ port: n.port('input'), model: { id: 'pair-driver', kind: 'driver', ports: ['p', 'n'], analyses: ['thermal'] }, stimulus: { kind: 'bits', bits: '01', bitRateBps: 1e6, riseS: 1e-9, fallS: 1e-9, repeatCount: 1 } }]).receivers([{ port: n.port('output'), model: { kind: 'rc-input', capacitanceF: 1e-12 } }]).time({ stopS: 1e-5, sampleStepS: 1e-9 }).prepare();
  assert.equal(si.structurallyValid, false);
  assert.ok(si.diagnostics.some(d => d.code === 'INVALID_BINDING')); assert.ok(si.diagnostics.some(d => d.code === 'INCOMPATIBLE_MODEL'));
});

test('byte/entry limits apply across contexts and large errors remain bounded', async () => {
  const limited = await runAnalysisDsl({ code: 'for(let i=0;i<20;i++) analysis().log({level:"info",message:"x".repeat(300)});' }, { maxOutputBytes: 4096 });
  assert.equal(limited.error?.code, 'OUTPUT_LIMIT');
  assert.ok(Buffer.byteLength(JSON.stringify(limited)) <= 4096);
  const error = await runAnalysisDsl({ code: 'const a=analysis();a.log({level:"info",message:"retained"}); throw Error("z".repeat(100000));' }, { maxOutputBytes: 4096 });
  assert.equal(error.status, 'failed'); assert.match(error.error!.message, /truncated/);
  assert.ok(Buffer.byteLength(JSON.stringify(error)) <= 4096);
});

test('provided resource payloads stay in executor tasks and become identities in automatic report entries', async () => {
  const api = createAnalysisApi({}, options, () => {}, async task => {
    assert.ok(JSON.stringify(task.descriptors).includes('large-resource-payload'));
    return { id: 'only-test', status: 'complete', method: 'test', diagnostics: [], assumptions: [], metrics: {} };
  });
  const a = api.globals.analysis();
  const n = a.network({ model: { id: 'resource', kind: 'network', ports: ['one'], source: 'explicit fixture', data: { text: 'large-resource-payload'.repeat(1000) } }, ports: { input: 'one' } });
  await a.networkParameters().network(n).frequencyHz(1e6).representation('S').referenceOhm(50).run();
  const report = JSON.stringify(a.report());
  assert.ok(!report.includes('large-resource-payload')); assert.ok(report.includes('sha256'));
});

test('explicit namespaced host resources are immutable and material table payloads remain uncomputed', async () => {
  const models = { materials: { project: { laminate: { id: 'laminate', kind: 'material' as const, data: { providedTable: [[1e6, 4.2], [1e9, 4.1]] } } } } };
  const original = structuredClone(models);
  const result = await runAnalysisDsl({ models, code: `const a=analysis();
    const line=a.line({kind:'microstrip',widthMm:.3,copperThicknessMm:.035,dielectricHeightMm:.18,material:models.materials.project.laminate});
    await a.impedance().target(line).method('cross-section').frequencyHz(1e9).prepare();
    try {models.materials.project.laminate.data.providedTable.push([2e9,4]);} catch(e) {a.log({level:'info',message:'resource is frozen'});}` });
  assert.equal(result.status, 'complete', JSON.stringify(result.error));
  assert.equal(result.reports[0].logs.length, 1); assert.deepEqual(models, original);
});

test('report progress does not resend a large plot with every subsequent log entry', () => {
  let bytes = 0;
  const api = createAnalysisApi({}, options, update => { bytes += Buffer.byteLength(JSON.stringify(update)); });
  const a = api.globals.analysis();
  a.plot({ title: 'Supplied table', kind: 'table', data: { text: 'x'.repeat(100_000) } });
  for (let i = 0; i < 100; i++) a.log({ level: 'info', message: String(i) });
  assert.equal(a.report().logs.length, 100); assert.equal(a.report().plots.length, 1);
  assert.ok(bytes < 200_000, `Progress copied accumulated data (${bytes} bytes)`);
});

test('eight complete USB/CAN/MCU/RF/DDR/EMI/thermal/antenna examples validate with explicit test executor', async () => {
  for (const name of ['usb', 'can', 'mcu', 'rf', 'ddr', 'emi', 'thermal', 'antenna'] as const) {
    const executed: AnalysisTask[] = [];
    const code = `const ensure=${examples.ensure.toString()}; await (${examples[name].toString()})();`;
    const r = await executeScenario({ code, board: board() }, options, () => {}, async task => {
      executed.push(task);
      const metrics: Record<string, ExecutionResult['metrics'][string]> = {};
      for (const d of task.descriptors) if (d && typeof d === 'object' && !Array.isArray(d) && 'id' in d && d.kind === 'port') metrics[`voltage:${d.id}`] = { kind: 'series', unit: 'V', axis: { name: 'timeS', unit: 's', values: [0, 1e-9] }, values: [0, 1], validity: { status: 'valid' }, diagnostics: [] };
      return { id: `test-${name}`, status: 'complete', method: 'explicit-language-test', diagnostics: [], assumptions: ['test data only'], metrics };
    });
    assert.equal(r.status, 'complete', `${name}: ${JSON.stringify(r.error)}`);
    if (name === 'usb' || name === 'ddr') assert.equal(executed.length, 1);
    if (name === 'ddr') assert.equal(r.reports[0].checks[0].status, 'unknown', 'timing extraction is not fabricated');
  }
});

export { board as analysisTestBoard };
