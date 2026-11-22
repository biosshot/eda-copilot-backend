import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createPiBoardExecutor } from '../src/analyze/analysis-dsl/pi-board-executor.ts';
import { magnitudeExtrema, portAdmittance, reflection, solveComplex, terminatedZ, type Cx } from '../src/analyze/analysis-dsl/pi-frequency-compose.ts';
import { anchorFrequencies, fitTier, PI_BAND_MAX_RATIO, planPiFrequency, quasiStaticLimitHz } from '../src/analyze/analysis-dsl/pi-frequency-plan.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { segmentedCoupon } from './helpers/pcb-analysis-segmented-coupon.ts';

const layers = `stackup({boardThicknessMm:.12,viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole',layers:[{kind:'copper',name:'TOP',thicknessMm:.01,material:'cu'},{kind:'dielectric',name:'substrate',thicknessMm:.1,material:'d'},{kind:'copper',name:'BOTTOM',thicknessMm:.01,material:'cu'}]})`;
const materials = `{cu:{conductivitySM:58e6,state:'Explicit uniform copper coupon'},d:{relativePermittivity:4,state:'Explicit lossless coupon dielectric'}}`;
const mag = (v: Cx) => Math.hypot(v.re, v.im);

// ---- Pure algebra: the analytic PDN of a two-node board ------------------------------------------------------------
/** Open-port Z of a board of two rail nodes joined by Zs=R+jwL, each node with Cp to the return; ports 0 (node 0), 1 and 2 (node 1). */
function boardZ(f: number, r: number, l: number, cp: number): Cx[][] {
 const w = 2 * Math.PI * f, ys: Cx = (() => { const d = r * r + (w * l) ** 2; return { re: r / d, im: -w * l / d }; })();
 // Eigenmodes of the symmetric pair: common (1,1) with jwCp, differential (1,-1) with 2/Zs + jwCp (closed form, no ill-conditioned inverse).
 const inv = (v: Cx): Cx => { const d = v.re * v.re + v.im * v.im; return { re: v.re / d, im: -v.im / d }; };
 const zc = inv({ re: 0, im: w * cp }), zd = inv({ re: 2 * ys.re, im: 2 * ys.im + w * cp });
 const z = [[{ re: (zc.re + zd.re) / 2, im: (zc.im + zd.im) / 2 }, { re: (zc.re - zd.re) / 2, im: (zc.im - zd.im) / 2 }], [{ re: (zc.re - zd.re) / 2, im: (zc.im - zd.im) / 2 }, { re: (zc.re + zd.re) / 2, im: (zc.im + zd.im) / 2 }]];
 const node = [0, 1, 1];
 return node.map(i => node.map(j => z[i][j] as Cx));
}
/** S50 of an open-port Z matrix: (Z - Z0 I)(Z + Z0 I)^-1. */
function waves(z: Cx[][]): Cx[][] {
 const n = z.length, plus = z.map((row, i) => row.map((v, j) => ({ re: v.re + (i === j ? 50 : 0), im: v.im }))), minus = z.map((row, i) => row.map((v, j) => ({ re: v.re - (i === j ? 50 : 0), im: v.im })));
 // (Z+Z0)^-1 and (Z-Z0) commute, so S = (Z+Z0)^-1 (Z-Z0).
 return solveComplex(plus, minus)!.map(row => row.map(v => v)).slice(0, n);
}
/** Source port (own short removed): the series Zs + Zdecap seen from node 0 with the load open. */
function sourceImpedance(f: number, o: { r: number; l: number; cp: number; esr: number; esl: number; c: number }) {
 const terms = [[{ kind: 'short' as const }], [], [{ kind: 'series' as const, r: o.esr, l: o.esl, c: o.c }]];
 const gamma = terms.map(t => reflection(portAdmittance(t, 2 * Math.PI * f)));
 return terminatedZ(waves(boardZ(f, o.r, o.l, o.cp)), gamma)[0][0]!;
}
const grid = (lo: number, hi: number, n: number) => Array.from({ length: n }, (_, i) => lo * (hi / lo) ** (i / (n - 1)));
const minimum = (o: Parameters<typeof sourceImpedance>[1], axis = grid(1e4, 1e8, 401)) => {
 const m = axis.map(f => mag(sourceImpedance(f, o)));
 const extrema = magnitudeExtrema(axis, m).filter(e => e.kind === 'minimum');
 return extrema[0];
};
const rlc = { r: 2e-3, l: 5e-9, cp: 1e-12, esr: 5e-3, esl: 1e-9, c: 1e-6 };

test('analytic RLC PDN: terminated Z equals the closed form and its |Z| minimum is the series resonance of the decap leg', () => {
 for (const f of [1e4, 3e5, 4.1e6, 5e7]) {
  const w = 2 * Math.PI * f;
  // Series path Zs + Zd, with the node capacitance Cp at node 1 in parallel with the decap and the load open.
  const zd: Cx = { re: rlc.esr, im: w * rlc.esl - 1 / (w * rlc.c) };
  const yd: Cx = { re: zd.re / (zd.re ** 2 + zd.im ** 2), im: -zd.im / (zd.re ** 2 + zd.im ** 2) };
  const yn: Cx = { re: yd.re, im: yd.im + w * rlc.cp }, dn = yn.re ** 2 + yn.im ** 2;
  const series: Cx = { re: rlc.r + yn.re / dn, im: w * rlc.l - yn.im / dn };
  // Node 0 carries its own Cp, in parallel with the port.
  const ds = series.re ** 2 + series.im ** 2, y0: Cx = { re: series.re / ds, im: -series.im / ds + w * rlc.cp }, d0 = y0.re ** 2 + y0.im ** 2;
  const expected: Cx = { re: y0.re / d0, im: -y0.im / d0 };
  const got = sourceImpedance(f, rlc);
  assert.ok(mag({ re: got.re - expected.re, im: got.im - expected.im }) <= 1e-7 * mag(expected) + 1e-9, `f=${f}: ${JSON.stringify(got)} vs ${JSON.stringify(expected)}`);
 }
 const m = minimum(rlc)!, f0 = 1 / (2 * Math.PI * Math.sqrt((rlc.l + rlc.esl) * rlc.c));
 assert.ok(Math.abs(m.refinedFrequencyHz! / f0 - 1) < 0.01, `minimum at ${m.refinedFrequencyHz} vs ${f0}`);
 // ESR (plus the board resistance) sets the depth of the minimum.
 assert.ok(Math.abs(m.refinedMagnitudeOhm! / (rlc.r + rlc.esr) - 1) < 0.02, `|Z|min ${m.refinedMagnitudeOhm} vs ${rlc.r + rlc.esr}`);
});

test('ESR, ESL, capacitance and decap position move Z in the expected direction', () => {
 const base = minimum(rlc)!;
 const esr = minimum({ ...rlc, esr: 4 * rlc.esr })!, esl = minimum({ ...rlc, esl: 4 * rlc.esl })!;
 const farther = minimum({ ...rlc, l: 3 * rlc.l, r: 3 * rlc.r })!, bigger = minimum({ ...rlc, c: 4 * rlc.c })!;
 // More ESR: a shallower-looking (higher) minimum at the same frequency.
 assert.ok(esr.refinedMagnitudeOhm! > 1.5 * base.refinedMagnitudeOhm! && Math.abs(esr.refinedFrequencyHz! / base.refinedFrequencyHz! - 1) < 0.02);
 // More ESL, a longer loop to the decap (position) or a larger capacitance lower the resonance; ESR and the same C leave |Z|min put.
 assert.ok(esl.refinedFrequencyHz! < base.refinedFrequencyHz!);
 assert.ok(Math.abs(esl.refinedFrequencyHz! * Math.sqrt((rlc.l + 4 * rlc.esl) * rlc.c) * 2 * Math.PI - 1) < 0.02);
 assert.ok(farther.refinedFrequencyHz! < base.refinedFrequencyHz!);
 assert.ok(Math.abs(bigger.refinedFrequencyHz! / base.refinedFrequencyHz! - 0.5) < 0.02);
 assert.ok(Math.abs(esl.refinedMagnitudeOhm! / base.refinedMagnitudeOhm! - 1) < 0.02);
 // A short and a lossless open port reflect exactly; a purely reactive series leg never loses power.
 assert.deepEqual([reflection('short'), reflection(undefined)], [{ re: -1, im: 0 }, { re: 1, im: 0 }]);
});

test('frequency plan: f_hi is board dependent, points above it and below 1 kHz are unavailable, anchors respect the 256 column limit', () => {
 // Quasi-static limit: a 100 mm board is far below the 40 MHz ceiling, a coupon is not.
 const stack = [{ thicknessM: 1.6e-3, epsilonR: 4.2 }];
 const big = quasiStaticLimitHz([[0, 0], [100, 0]], stack), small = quasiStaticLimitHz([[0, 0], [10, 1]], [{ thicknessM: 1.2e-4, epsilonR: 4 }]);
 assert.ok(big > 1e7 && big < 3e7 && small > 4e7, `${big} ${small}`);
 const axis = [500, 2e3, 5e4, 5e5, 2e6, 1e7, 3e7, 8e7];
 const plan = planPiFrequency({ frequenciesHz: axis, portCount: 4, quasiStaticHz: big });
 assert.equal(plan.fHiHz, big);
 assert.deepEqual(plan.unavailable.map(u => u.frequencyHz), [500, 3e7, 8e7].filter(f => f < 1e3 || f > big));
 assert.match(plan.unavailable[0].reason, /1 kHz/);
 assert.match(plan.unavailable.at(-1)!.reason, /not extrapolated/);
 assert.match(plan.fHiReason, /quasi-static/);
 // Bands follow the P7 planner boundaries; each band keeps both extraction ends and at most 256/N anchors.
 assert.deepEqual(plan.bands.map(b => b.boundsHz[0]), [1e3, 1e5, 1e6]);
 for (const band of plan.bands) {
  assert.ok(band.anchorsHz.length <= 6 && band.anchorsHz.length * 4 <= 256);
  assert.equal(band.anchorsHz[0], band.extractionHz[0]);
  assert.equal(band.anchorsHz.at(-1), band.extractionHz[1]);
 }
 assert.deepEqual(plan.junctionsHz, [1e5, 1e6].filter(f => plan.bands.some(b => b.boundsHz[1] === f)));
 assert.equal(planPiFrequency({ frequenciesHz: [5e7], portCount: 2, quasiStaticHz: Infinity }).bands.length, 0);
 assert.deepEqual(planPiFrequency({ frequenciesHz: [5e7], portCount: 2, quasiStaticHz: Infinity }).unavailable.map(u => u.frequencyHz), [5e7]);
 // 16 ports leave 16 anchors per extraction; a wide band still takes about one anchor per decade.
 assert.ok(anchorFrequencies([1e6, 4e7], 16).length <= 16 && anchorFrequencies([1e6, 4e7], 16).length >= 3);
 assert.deepEqual(anchorFrequencies([1e6, 1e6], 16), [1e6]);
 assert.deepEqual([fitTier(1e-3), fitTier(0.05), fitTier(0.2)], ['noise', 'approximation', 'refusal']);
});

// ---- Coupon: extraction + reduced model + composition --------------------------------------------------------------
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
const cap = (c: number, esr: number, esl: number) => `.components([capacitor({designator:'C1',pins:{positive:'1',negative:'2'},capacitanceF:${c},esrOhm:${esr},eslH:${esl}})])`;
const scenario = (axis: string, components = '') => `const a=analysis({board});
const port=d=>a.port({positive:pad(d,'1'),negative:pad(d,'2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'});
const layers=${layers};
const J0=port('J0'),J1=port('J1');
const result=await a.pi().domain('frequency')${axis}.sources([{port:J0,voltageV:3.3,model:'ideal'}]).loads([{port:J1,currentA:1}])${components}.stackup(layers).materials(${materials}).budget({maxSeconds:300,maxMemoryMiB:2048}).run();
const zJ1=result.impedance(J1);`;
async function run(board: unknown, code: string) {
 const execute = createPiBoardExecutor();
 let response: Awaited<ReturnType<typeof execute>> | undefined;
 const result = await executeScenario({ board, code }, { filename: 'pi-frequency.js', maxEntries: 1000, maxOutputBytes: 16 * 1024 * 1024 }, () => {},
  async task => (response = await execute(task as AnalysisTask<'pi'>)));
 assert.equal(result.status, 'complete', JSON.stringify(result.error));
 return response!;
}
const seriesOf = (response: any, which: number) => {
 const key = Object.keys(response.metrics).filter(k => k.startsWith('impedance:'))[which];
 const m = response.metrics[key];
 return { values: m.values as ({ real: number; imaginary: number } | null)[], axis: m.axis.values as number[], validity: m.validity };
};
const axisMin = (s: ReturnType<typeof seriesOf>) => {
 const m = s.values.map(v => v && Math.hypot(v.real, v.imaginary));
 return magnitudeExtrema(s.axis, m).filter(e => e.kind === 'minimum')[0];
};

test('coupon end to end: the source-port |Z| minimum is the decap series resonance, ESR + loop R sets its depth, loop L matches the microstrip formula', async () => {
 const C = 1e-5, ESR = 0.01, ESL = 1e-9;
 const base = await run(withParts([['C1', 5]]), scenario(`.frequency({startHz:2e5,stopHz:1.5e6,points:15,scale:'log'})`, cap(C, ESR, ESL)));
 const s = base.solution as any;
 assert.equal(base.status, 'complete');
 assert.equal(s.portMap.length, 3);
 assert.equal(s.bands.length, 2);
 // The extracted model reproduces its own samples (noise tier) and neighbouring bands agree where they meet.
 assert.equal(s.bandFit.tier, 'noise');
 assert.ok(s.junctions.length === 1 && s.junctions[0].max < 1e-3, JSON.stringify(s.junctions));
 assert.ok(s.bands.every((b: any) => b.modelSize.potentials > 0 && b.modelSize.currents > 0 && b.bandFit.tier === 'noise'));
 assert.equal(base.diagnostics.some(d => d.code === 'PI_PHYSICAL_UNVERIFIED'), true);
 // Source port: board loop to the decap in series with the decap leg (load open, source own termination removed).
 const z = seriesOf(base, 0);
 assert.ok(z.values.every(v => v !== null) && z.validity.status === 'conditional');
 const m = axisMin(z)!;
 const dc = s.dcEndpoint.zMatrixDc[2][2] as number; // decap loop to the shorted source at DC
 assert.ok(Math.abs(m.refinedMagnitudeOhm! / (ESR + dc) - 1) < 0.05, `|Z|min ${m.refinedMagnitudeOhm} vs ESR+R_dc ${ESR + dc}`);
 // Loop inductance from the model (Im Z + 1/wC)/w - ESL at the top of the axis against Hammerstad microstrip, 0.1 mm line on 0.1 mm FR4 (er 4): L' = Z0 sqrt(eeff)/c.
 const top = z.values.at(-1)!, w = 2 * Math.PI * z.axis.at(-1)!;
 const loop = (top.imaginary + 1 / (w * C)) / w - ESL;
 const eeff = (4 + 1) / 2 + (4 - 1) / 2 / Math.sqrt(1 + 12), z0 = 60 / Math.sqrt(eeff) * Math.log(8 / 1 + 1 / 4);
 const analytic = z0 * Math.sqrt(eeff) / 299792458 * 5e-3;
 assert.ok(Math.abs(loop / analytic - 1) < 0.3, `loop L ${loop} vs microstrip ${analytic}`);
 // Series resonance of the extracted loop + ESL with the decap capacitance, against the |Z| minimum.
 const f0 = 1 / (2 * Math.PI * Math.sqrt((loop + ESL) * C));
 assert.ok(Math.abs(m.refinedFrequencyHz! / f0 - 1) < 0.1, `min ${m.refinedFrequencyHz} vs ${f0}`);
 // Resonance table and transfer accessor are populated; the table lists the same minimum.
 const table = (base.metrics.resonances as any).data.extrema as any[];
 assert.ok(table.some(e => e.kind === 'minimum'));
 assert.ok(Object.keys(base.metrics).some(k => k.startsWith('pi-transfer:')));
 // DC point stays the stage-2 path and its stitching difference is reported.
 assert.equal(s.dcStitch.status, 'reported');
 assert.ok(s.dcStitch.max > 0 && s.dcStitch.acFrequencyHz === s.bands[0].anchorsHz[0]);
 // The stitch compares the board with the DC terminations; with the decap conducting at 200 kHz the terminated difference is far larger.
 assert.ok(s.dcStitch.relativeDifference[1] < 0.05 && s.dcStitch.terminatedRelativeDifference[1] > 0.2, JSON.stringify(s.dcStitch));
 // A decap nearer to the source (shorter loop) resonates higher.
 const nearer = await run(withParts([['C1', 2.5]]), scenario(`.frequency({startHz:2e5,stopHz:1.5e6,points:15,scale:'log'})`, cap(C, ESR, ESL)));
 const mn = axisMin(seriesOf(nearer, 0))!;
 assert.ok(mn.refinedFrequencyHz! > m.refinedFrequencyHz!, `nearer ${mn.refinedFrequencyHz} vs ${m.refinedFrequencyHz}`);
});

test('coupon: DC/AC stitching at 1 kHz, a single AC frequency, and unavailable points outside the verified band', async () => {
 const low = await run(segmentedCoupon(), scenario('.frequencyHz(1e3)'));
 const s = low.solution as any;
 assert.equal(low.status, 'complete');
 const load = seriesOf(low, 1);
 assert.equal(load.axis.length, 1);
 // 1 kHz: the loop is resistive; the AC model agrees with the DC point within the noise tier.
 assert.ok(s.dcStitch.max < 0.01, JSON.stringify(s.dcStitch));
 const dcOhm = s.dcEndpoint.zMatrixDc[1][1] as number;
 assert.ok(Math.abs(load.values[0]!.real / dcOhm - 1) < 0.01 && Math.abs(load.values[0]!.imaginary) < 0.01 * dcOhm);
 // Above f_hi (and below 1 kHz) nothing is extrapolated and no extraction runs.
 const high = await run(segmentedCoupon(), scenario('.frequency({startHz:500,stopHz:1e8,points:2,scale:\'log\'})'));
 const h = high.solution as any;
 assert.equal(high.status, 'partial');
 assert.deepEqual(h.unavailable.map((u: any) => u.frequencyHz), [500, 1e8]);
 assert.equal(h.bands.length, 0);
 assert.equal(seriesOf(high, 1).validity.status, 'unavailable');
 assert.ok(seriesOf(high, 1).values.every(v => v === null));
 assert.equal(high.diagnostics.some(d => d.code === 'PI_BAND_UNAVAILABLE'), true);
});

test('frequency plan: a band in the top decade below f_hi never spans more than a decade, lower bands stay whole', () => {
 const fHi = 3.975e7;
 const axis = Array.from({ length: 30 }, (_, i) => 1e3 * (4e7 / 1e3) ** (i / 29));
 const plan = planPiFrequency({ frequenciesHz: axis, portCount: 6, quasiStaticHz: fHi });
 const whole = planPiFrequency({ frequenciesHz: axis.filter(f => f <= 1e6), portCount: 6, quasiStaticHz: fHi });
 // The 1 kHz-80 kHz band is wider than a decade but far below f_hi/10: unchanged.
 assert.deepEqual(plan.bands.slice(0, 2).map(b => b.extractionHz), whole.bands.map(b => b.extractionHz));
 const high = plan.bands.filter(b => b.boundsHz[0] >= 1e6);
 assert.ok(high.length >= 2);
 for (const band of high) assert.ok(band.extractionHz[1] / band.extractionHz[0] <= PI_BAND_MAX_RATIO, String(band.extractionHz));
 assert.deepEqual(high.flatMap(b => b.pointIndices).sort((a, b) => a - b), axis.map((f, i) => f > 1e6 && f <= fHi ? i : -1).filter(i => i >= 0));
 for (let i = 1; i < high.length; i++) {
  assert.equal(high[i].boundsHz[0], high[i - 1].boundsHz[1]);
  assert.ok(plan.junctionsHz.includes(high[i].boundsHz[0]));
 }
});
