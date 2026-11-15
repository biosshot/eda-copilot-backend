import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createPiBoardExecutor } from '../src/analyze/analysis-dsl/pi-board-executor.ts';
import { createLocalExecutor } from '../src/analyze/analysis-dsl/local-executor.ts';
import { portDcImpedanceMatrix } from '../src/analyze/analysis-dsl/pi-board-dc.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { createProblemExecutor } from '../src/analyze/execute-problem.ts';
import type { BoardFoilProblem } from '../src/analyze/types.ts';
import { segmentedCoupon, segmentedCouponDsl } from './helpers/pcb-analysis-segmented-coupon.ts';

const layers = `stackup({boardThicknessMm:.12,viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole',layers:[{kind:'copper',name:'TOP',thicknessMm:.01,material:'cu'},{kind:'dielectric',name:'substrate',thicknessMm:.1,material:'d'},{kind:'copper',name:'BOTTOM',thicknessMm:.01,material:'cu'}]})`;
const materials = `{cu:{conductivitySM:58e6,state:'Explicit uniform copper coupon'},d:{relativePermittivity:4,state:'Explicit lossless coupon dielectric'}}`;

/** Extra two-pad parts on the coupon: pad 1 on SIGNAL/TOP, pad 2 on RETURN/BOTTOM at x. */
function withParts(parts: [string, number][]) {
 const board = segmentedCoupon();
 for (const [name, x] of parts) {
  board.components.push({ ...board.components[0], designator: name, at: { x, y: 0 } });
  for (const [pad, id] of [['s0', 's'], ['r0', 'r']] as const) {
   const source = board.pads.find((p: any) => p.id === pad);
   board.pads.push({ ...structuredClone(source), component: name, id: id + name, number: pad === 's0' ? '1' : '2', at: { x, y: 0 } });
  }
 }
 return board;
}
const scenario = (body: string, parts = '') => `const a=analysis({board});
const port=d=>a.port({positive:pad(d,'1'),negative:pad(d,'2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'});
const layers=${layers};${parts}
const run=()=>a.pi().domain('frequency')${body}.run();
const result=await run();`;
const base = (loads: string, freq = '.frequencyHz(0)', extra = '') =>
 `${freq}.sources([{port:port('J0'),voltageV:3.3,model:'ideal'}]).loads([${loads}]).stackup(layers).materials(${materials}).budget({maxSeconds:300,maxMemoryMiB:2048})${extra}`;

async function run(board: unknown, code: string, execute = createPiBoardExecutor()) {
 let response: Awaited<ReturnType<typeof execute>> | undefined;
 const result = await executeScenario({ board, code }, { filename: 'pi-board.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 }, () => {},
  async task => (response = await execute(task as AnalysisTask<'pi'>)));
 return { result, response };
}
const never = (() => { throw new Error('native solver must not be reached'); }) as never;
const codeOf = (result: any) => result.error?.code ?? JSON.stringify(result.error);

test('DC N-port algebra: terminations, shorts, open ports and mutual terms', () => {
 // Rail electrodes 0-1 (0.6 Ohm), return electrodes 2-3 (0.4 Ohm); port 0 = (0,2) source side, port 1 = (1,3) load side.
 const gr = 1 / .6, gg = 1 / .4;
 const G = [[gr, -gr, 0, 0], [-gr, gr, 0, 0], [0, 0, gg, -gg], [0, 0, -gg, gg]];
 const pairs = [[0, 2], [1, 3]] as const, isl = [0, 0, 1, 1];
 const diag = (g: number[]) => portDcImpedanceMatrix(G, isl, pairs, g).map((row, i) => row[i]);
 // Source shorted: the load sees rail + return = 1.0; with a 2 Ohm source resistance 3.0; both ports open: no DC path.
 assert.ok(Math.abs((diag([Infinity, 0])[1] as number) - 1) < 1e-12);
 assert.ok(Math.abs((diag([.5, 0])[1] as number) - 3) < 1e-12);
 assert.deepEqual(diag([0, 0]), [null, null]);
 // Column j has the other port terminated: 1 A at the source with the load shorted gives 1.0 at the source and 0 at the load.
 const z = portDcImpedanceMatrix(G, isl, pairs, [0, Infinity]);
 assert.ok(Math.abs((z[0][0] as number) - 1) < 1e-12 && Math.abs(z[1][0] as number) < 1e-12);
 // Resistive load (1 Ohm) with a shorted source: the load port sees 1.0 for itself and 1 V for 1 A at the source port.
 const mutual = portDcImpedanceMatrix(G, isl, pairs, [Infinity, 1]);
 assert.ok(Math.abs((mutual[1][1] as number) - 1) < 1e-12);
 assert.ok(Math.abs((mutual[1][0] as number) - 1) < 1e-12);
 const loaded = portDcImpedanceMatrix(G, isl, pairs, [0, 1]);
 // 1 A at the source with a 1 Ohm load: the loop is .6+.4+1 = 2 Ohm, the load sees 1 V per A.
 assert.ok(Math.abs((loaded[0][0] as number) - 2) < 1e-12 && Math.abs((loaded[1][0] as number) - 1) < 1e-12);
});

test('assembly refuses missing models, undescribed rail parts, too many ports and unsupported scope with explicit codes', async () => {
 const cap = (extra: string) => `capacitor({designator:'C1',pins:{positive:'1',negative:'2'},capacitanceF:1e-6${extra}})`;
 const withCap = withParts([]);
 // Undescribed decoupling part on a rail net.
 const loads = `{port:port('J1'),currentA:1}`;
 let r = await run(withParts([['C1', 5]]), scenario(base(loads)), createPiBoardExecutor(never));
 assert.equal(codeOf(r.result), 'PI_COMPONENT_MODEL', JSON.stringify(r.result.error));
 assert.match(JSON.stringify(r.result.error), /C1/);
 // Capacitor with only a nominal value, no parasitics and no ideal flag: the DSL schema already refuses it (never ideal decoupling).
 r = await run(withParts([['C1', 5]]), scenario(base(loads, '.frequencyHz(0)', `.components([{kind:'capacitor',designator:'C1',pins:{positive:'1',negative:'2'},model:{kind:'capacitor',capacitanceF:1e-6}}])`)), createPiBoardExecutor(never));
 assert.equal(codeOf(r.result), 'INVALID_CONFIGURATION', JSON.stringify(r.result.error));
 assert.match(JSON.stringify(r.result.error), /parasitics or explicit ideal/);
 // Source without a VRM model: the schema already refuses it; PI_COMPONENT_MODEL stays as a second guard.
 r = await run(withCap, scenario(`.frequencyHz(0).sources([{port:port('J0'),voltageV:3.3}]).loads([${loads}]).stackup(layers).materials(${materials})`), createPiBoardExecutor(never));
 assert.equal(codeOf(r.result), 'INVALID_CONFIGURATION', JSON.stringify(r.result.error));
 // More than 16 ports.
 const many = Array.from({ length: 16 }, (_, i) => `K${i}`);
 r = await run(withParts(many.map((k, i): [string, number] => [k, i * .5 + .5])),
  scenario(base(`{port:port('J1'),currentA:1},` + many.map(k => `{port:port('${k}'),currentA:.1}`).join(','))), createPiBoardExecutor(never));
 assert.equal(codeOf(r.result), 'PI_PORT_LIMIT', JSON.stringify(r.result.error));
 // Stage gate: the time domain is refused until stage 4 (the AC axis is stage 3, tests/analyze-pi-frequency.test.ts).
 r = await run(withCap, scenario(base(loads, `.time({stopS:1e-6,sampleStepS:1e-8}).initial('steady')`)).replace("domain('frequency')", "domain('time')"), createPiBoardExecutor(never));
 assert.equal(codeOf(r.result), 'PI_NOT_IMPLEMENTED_STAGE');
 // Ideal decoupling is accepted only when explicit, and is reported.
 const idealCap = cap(',ideal:true');
 assert.ok(idealCap.includes('ideal'));
 // Resource multiports on a board are out of v1.
 r = await run(withParts([['C1', 5]]), scenario(base(loads, '.frequencyHz(0)', `.components([{kind:'device',designator:'C1',pins:{positive:'1',negative:'2'}}])`)), createPiBoardExecutor(never));
 assert.equal(codeOf(r.result), 'PI_RESOURCE_UNSUPPORTED', JSON.stringify(r.result.error));
});

test('board pi on the coupon: 2-port DC equals the old channel DC path; 4 ports are reciprocal and follow the line', async () => {
 const old = await (async () => {
  const execute = createLocalExecutor();
  let response: Awaited<ReturnType<typeof execute>> | undefined;
  const code = segmentedCouponDsl.split('a.plot(')[0].replace(".frequency({startHz:1000,stopHz:10000,points:3,scale:'log'})", '.frequencyHz(0)');
  const result = await executeScenario({ board: segmentedCoupon(), code }, { filename: 'old-dc.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 }, () => {},
   async task => (response = await execute(task as AnalysisTask<'impedance'>)));
  assert.equal(result.status, 'complete', JSON.stringify(result.error));
  return (response!.solution as any).loopProbe.resistanceOhm as number;
 })();
 let r = await run(segmentedCoupon(), scenario(base(`{port:port('J1'),currentA:1}`)));
 assert.equal(r.result.status, 'complete', JSON.stringify(r.result.error));
 const s = r.response!.solution as any;
 assert.deepEqual(s.portMap.map((p: any) => p.roles), [['source'], ['load']]);
 const z = s.zMatrixDc as number[][];
 // The load seen with the ideal source shorted is the loop the old channel path measures with a balanced 1 A drive.
 const loop = z[1][1];
 assert.ok(Math.abs(loop / old - 1) < 1e-9, `loop ${loop} vs old ${old}`);
  assert.equal(r.response!.diagnostics.some(d => d.code === 'PI_IDEAL_SOURCE'), true);
 assert.equal(s.portMap[1].dcImpedanceOhm, z[1][1]);
 // Three connected ports (source + two loads) and a decoupling part that is open (floating) at DC.
 r = await run(withParts([['J2', 5], ['C1', 2.5]]), scenario(base(`{port:port('J1'),currentA:1},{port:port('J2'),currentA:.5}`, '.frequencyHz(0)',
  `.components([capacitor({designator:'C1',pins:{positive:'1',negative:'2'},capacitanceF:1e-6,esrOhm:.01,eslH:1e-9})])`)));
 assert.equal(r.result.status, 'complete', JSON.stringify(r.result.error));
 const n = r.response!.solution as any;
 assert.equal(n.portMap.length, 4);
 assert.deepEqual(n.portMap.map((p: any) => p.floating), [true, false, false, false]);
 const nz = n.zMatrixDc as number[][];
 // Ports 1..3 share the shorted source and open terminations: reciprocal. The line gives Z_ij ~ R(min(x_i,x_j)).
 for (const i of [1, 2, 3]) for (const j of [1, 2, 3]) assert.ok(Math.abs(nz[i][j] - nz[j][i]) <= 1e-9 * nz[1][1]);
 assert.ok(Math.abs(nz[2][2] / nz[1][1] - .5) < .05 && Math.abs(nz[3][3] / nz[1][1] - .25) < .05 && Math.abs(nz[1][2] / nz[2][2] - 1) < .05);
});

test('budget: a tiny DC budget is an honest refusal with a hint; the hybrid guard is off unless enforced', async () => {
 const r = await run(segmentedCoupon(), scenario(base(`{port:port('J1'),currentA:1}`).replace('maxSeconds:300', 'maxSeconds:1e-9')));
 assert.equal(r.result.status === 'complete', false);
 assert.match(JSON.stringify(r.result.error), /DEADLINE/);
 // Capture one real hybrid problem, then replay it with a 1 MiB enforced ceiling and unchanged without enforcement.
 let captured: BoardFoilProblem | undefined;
 const execute = createProblemExecutor(true);
 const local = createLocalExecutor(((problem: BoardFoilProblem) => { captured ??= problem.kind === 'board-foil-checked' ? problem : captured; return execute(problem); }) as never);
 const code = segmentedCouponDsl.split('a.plot(')[0];
 const done = await executeScenario({ board: segmentedCoupon(), code }, { filename: 'hybrid.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 }, () => {},
  async task => local(task as AnalysisTask<'impedance'>));
 assert.equal(done.status, 'complete', JSON.stringify(done.error));
 assert.ok(captured);
 // Different axes keep the results out of each other's cache entries (the cache key does not contain the budget).
 const replay = (frequencyHz: number, enforce: boolean) => ({ ...captured!, request: { ...captured!.request, frequenciesHz: [frequencyHz],
  budget: { maxSeconds: 600, maxMemoryMiB: 1 }, ...(enforce && { enforceBudget: true }) } }) as BoardFoilProblem;
 assert.doesNotThrow(() => execute(replay(2000, false)));
 assert.throws(() => execute(replay(3000, true)), /MEMORY_LIMIT|exceeds|budget/i);
});
