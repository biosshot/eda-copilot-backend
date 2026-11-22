/// <reference path="../../dist/analysis-spec-doc.d.ts" />
import type { Preparation, AnalysisContext } from '../../dist/analysis-spec-doc.js';

/** Synthetic language examples. The host supplies the documented U/J coupon as
 * board. run() in USB/DDR needs an executor (explicit test executor in tests).
 * These parameters are illustrative, not validated hardware/physics models. */
export function ensure(p: Preparation) {
  if (!p.structurallyValid) throw Error(JSON.stringify(p.diagnostics));
}

export async function usb() {
  const a = analysis({ board: board! });
  const txP = a.port({ positive: pad('U1', 3), negative: pad('U1', 2) });
  const txN = a.port({ positive: pad('U1', 4), negative: pad('U1', 2) });
  const rxP = a.port({ positive: pad('J1', 3), negative: pad('J1', 2) });
  const rxN = a.port({ positive: pad('J1', 4), negative: pad('J1', 2) });
  const p = a.channel({ from: txP, to: rxP }), n = a.channel({ from: txN, to: rxN });
  const pair = a.differential({ positive: p, negative: n });
  ensure(await a.geometry().scope([net('P'), net('N')]).rules([{ kind: 'skew', channels: [p, n], maxMm: .5 }]).prepare());
  ensure(await a.impedance().target(pair).method('layered').frequencyHz(1e9).prepare());
  ensure(await a.tdr().target(pair).referenceOhm(90).riseS(1e-10).termination('matched').time({ stopS: 1e-8, sampleStepS: 1e-11 }).prepare());
  const driver = { id: 'usb-driver', kind: 'differential-driver', ports: ['positive', 'negative'] };
  const receiver = { id: 'usb-rx', kind: 'differential-input', ports: ['positive', 'negative'] };
  const task = a.si().channels([pair]).drivers([{ ports: { positive: txP, negative: txN }, model: driver, stimulus: { kind: 'bits', bits: '01011010', bitRateBps: 480e6, riseS: 1e-10, fallS: 1e-10, repeatCount: 1 } }]).receivers([{ ports: { positive: rxP, negative: rxN }, model: receiver }]).time({ stopS: 8 / 480e6, sampleStepS: 1e-11 });
  ensure(await task.prepare());
  const signal = await task.run();
  ensure(await a.eye().signal(signal.voltage(rxP)).bitRateBps(480e6).discardS(0).phaseS(0).thresholdV(1.65).prepare());
}

export async function can() {
  const a = analysis();
  const left = a.network({ model: { id: 'can-pcb', kind: 'multiport', ports: ['in', 'out'] }, ports: { input: 'in', output: 'out' } });
  const right = a.network({ model: { id: 'can-bus-load', kind: 'multiport', ports: ['in', 'out'] }, ports: { input: 'in', output: 'out' } });
  const bus = a.connect({ networks: [left, right], connections: [{ from: left.port('output'), to: right.port('input') }], ports: { input: left.port('input'), output: right.port('output') } });
  ensure(await a.si().network(bus).drivers([{ port: bus.port('input'), model: a.model({ kind: 'linear-driver', lowV: 0, highV: 2, outputResistanceOhm: 30 }), stimulus: { kind: 'bits', bits: '0101', bitRateBps: 1e6, riseS: 1e-8, fallS: 1e-8, repeatCount: 1 } }]).receivers([{ port: bus.port('output'), model: a.model({ kind: 'rc-input', capacitanceF: 1e-11, resistanceOhm: 120 }) }]).time({ stopS: 4e-6, sampleStepS: 1e-9 }).prepare());
  ensure(await a.networkParameters().network(bus).frequency({ startHz: 1e3, stopHz: 1e8, points: 101, scale: 'log' }).representation('S').referenceOhm({ input: 60, output: 60 }).prepare());
}

export async function mcu() {
  const a = analysis({ board: board! });
  // J1 is the explicitly supplied 3.3 V DC-DC output boundary in this coupon;
  // U1 is the MCU and U2 the display. No converter or energy transfer is inferred.
  const supply = a.port({ positive: pad('J1', 1), negative: pad('J1', 2) });
  const sources = [{ port: supply, voltageV: 3.3, model: 'ideal' as const }];
  const loads = [{ designator: 'U1', rail: net('VCC'), currentA: .1, pinCurrentsA: { '1': .04 } }, { designator: 'U2', rail: net('VCC'), currentA: .02 }];
  const components = [ic({ designator: 'U1', supplies: { core: ['1', '5'] }, ground: ['2', '6'] }), capacitor({ designator: 'C1', capacitanceF: 1e-6, esrOhm: .01, eslH: 1e-9 })];
  const layers = stackup({ viaPlatingThicknessUm: 25, layers: [{ kind: 'copper', name: 'TOP', thicknessMm: .035 }, { kind: 'dielectric', name: 'core', thicknessMm: 1.53, material: 'fr4' }, { kind: 'copper', name: 'BOTTOM', thicknessMm: .035 }] });
  const materials = { fr4: { relativePermittivity: 4.2 }, copper: { conductivitySM: 58e6 } };
  ensure(await a.dc().components(components).stackup(layers).materials(materials).sources(sources).loads(loads).temperatureC(25).prepare());
  const pi = a.pi().components(components).sources(sources).loads(loads);
  ensure(await pi.domain('frequency').frequencyHz(1e6).prepare());
  ensure(await pi.domain('time').initial('steady').time({ stopS: 1e-4, sampleStepS: 1e-7 }).prepare());
  ensure(await a.electrothermal().electrical({ sources, loads, components }).thermal({ heat: [{ target: component('U1'), node: 'junction', powerW: .1 }], cooling: { boardSurfaces: { kind: 'cooling', resistanceKPerW: 10 } }, environment: profiles.environment({ name: 'hot', ambientC: 70 }), initialC: 'ambient' }).couplings([{ target: component('U1'), thermalNode: 'junction' }]).time({ stopS: 10, sampleStepS: .1 }).prepare());
}

export async function rf() {
  const a = analysis();
  const line = a.line({ kind: 'microstrip', widthMm: .3, copperThicknessMm: .035, dielectricHeightMm: .18, material: { relativePermittivity: 4.2, lossTangent: .02 }, lengthMm: 50 });
  ensure(await a.impedance().target(line).method('cross-section').frequencyHz(2.4e9).prepare());
  ensure(await a.em().target(line).method('full-wave').frequency({ startHz: 1e9, stopHz: 3e9, points: 101, scale: 'linear' }).space({ kind: 'free-space' }).outputs(['sParameters', 'fields']).prepare());
  const amplifier = a.network({ model: { id: 'rf-amplifier', kind: 'multiport', ports: ['in', 'out'] }, ports: { input: 'in', output: 'out' } });
  const chain = a.connect({ networks: [line, amplifier], connections: [{ from: line.port('output'), to: amplifier.port('input') }], ports: { input: line.port('input'), output: amplifier.port('output') } });
  ensure(await a.networkParameters().network(chain).frequencyHz(2.4e9).representation('Z').prepare());
}

export async function ddr() {
  const a = analysis();
  const line = a.line({ kind: 'coupled-stripline', widthMm: .15, copperThicknessMm: .035, topDielectricHeightMm: .15, bottomDielectricHeightMm: .15, topMaterial: { relativePermittivity: 4.2 }, bottomMaterial: { relativePermittivity: 4.2 }, gapMm: .15, lengthMm: 40 });
  const data = line.port('positiveOutput'), clock = line.port('negativeOutput');
  const model = a.model({ kind: 'linear-driver', lowV: 0, highV: 1.2, outputResistanceOhm: 30 });
  const stimulus = { kind: 'prbs' as const, order: 7, seed: 1, bits: 128, bitRateBps: 1e9, riseS: 1e-10, fallS: 1e-10 };
  const r = await a.si().channels([line]).drivers([{ port: line.port('positiveInput'), model, stimulus }, { port: line.port('negativeInput'), model, stimulus: { ...stimulus, seed: 2 } }]).receivers([{ port: data, model: a.model({ kind: 'rc-input', capacitanceF: 2e-12 }) }, { port: clock, model: a.model({ kind: 'rc-input', capacitanceF: 2e-12 }) }]).time({ stopS: 128e-9, sampleStepS: 1e-11 }).run();
  const timing = r.timing({ kind: 'sampling', data: [{ port: data, lowV: .4, highV: .8 }], clock: { port: clock, thresholdV: .6, edges: 'rising', sampleOffsetS: 0 }, window: { startS: 0, stopS: 128e-9 } });
  a.check({ name: 'Setup', actual: timing.setup.min(), min: 1e-10, unit: 's' });
  const propagation = r.timing({ kind: 'propagation', paths: [{ from: line.port('positiveInput'), to: data, sourceThresholdV: .6, destinationThresholdV: .6 }], edges: 'rising', maxDelayS: 1e-8, window: { startS: 0, stopS: 128e-9 } });
  a.check({ name: 'Arrival delay', actual: propagation.delay.max(), max: 1e-8, unit: 's' });
}

export async function emi() {
  const a = analysis({ board: board! });
  const input = a.port({ positive: pad('U1', 3), negative: pad('U1', 2) });
  const stimulus = { kind: 'sine' as const, offset: 0, amplitude: 1, periodS: 1e-6, phaseDeg: 0 };
  ensure(await a.emi().kind('coupling').excitations([{ name: 'external source', model: { id: 'motor-field', kind: 'spatial-current-source' }, placement: { positionMm: { x: 50, y: 0, z: 30 }, rotationDeg: { x: 0, y: 0, z: 90 } }, currentA: stimulus }]).measurement([{ port: input, quantity: 'voltage' }]).time({ stopS: 1e-4, sampleStepS: 1e-8 }).prepare());
  ensure(await a.emi().kind('conducted').sources([]).loads([]).excitations([{ port: input, currentA: stimulus }]).measurement({ port: input, quantity: 'voltage', detector: 'rms', bandwidthHz: 9000, model: { id: 'measurement', kind: 'measurement-network', ports: ['input'] } }).time({ stopS: 1e-4, sampleStepS: 1e-8 }).spectrum({ discardS: 1e-5, window: 'hann', startHz: 1e5, stopHz: 3e7 }).prepare());
}

export async function thermal() {
  const a = analysis();
  const network = a.thermalNetwork({ nodes: [{ id: 'ambient', boundary: 'environment' }, { id: 'junction', heatCapacityJPerK: .1 }, { id: 'sink', heatCapacityJPerK: 20 }], links: [{ from: 'junction', to: 'sink', resistanceKPerW: 2 }, { from: 'sink', to: 'ambient', resistanceKPerW: 8 }], sources: [{ node: 'junction', powerW: 2 }] });
  const environment = profiles.environment({ name: 'rise', ambientC: { kind: 'step', from: -20, to: 70, atS: 60, riseS: 5 } });
  ensure(await a.thermal().network(network).environment(environment).initialC('ambient').time({ stopS: 600, sampleStepS: 1 }).prepare());
  const circuit = a.circuit({ nodes: ['gnd', 'v'], ports: [{ id: 'input', positive: 'v', negative: 'gnd' }], elements: [{ kind: 'voltage-source', id: 'V1', nodes: ['v', 'gnd'], voltageV: 5, model: 'ideal' }, { kind: 'resistor', id: 'R1', nodes: ['v', 'gnd'], resistanceOhm: 10, referenceC: 25, temperatureCoefficientPerK: .004 }] });
  const coupledThermal = a.thermalNetwork({ nodes: [{ id: 'ambient', boundary: 'environment' }, { id: 'junction', heatCapacityJPerK: .1 }], links: [{ from: 'junction', to: 'ambient', resistanceKPerW: 10 }], sources: [] });
  ensure(await a.electrothermal().electrical({ circuit }).thermal({ network: coupledThermal, environment, initialC: 'ambient' }).couplings([{ element: 'R1', thermalNode: 'junction' }]).time({ stopS: 10, sampleStepS: .1 }).prepare());
}

export async function antenna() {
  const a = analysis();
  const scene = a.scene({ model: { id: 'patch', kind: 'em-scene', parameters: { widthMm: 32 }, ports: ['feed'] }, parameters: { widthMm: 30 } });
  const antenna = a.antenna({ structure: scene, feed: scene.port('feed') });
  ensure(await a.em().target(antenna).method('full-wave').frequency({ startHz: 2.3e9, stopHz: 2.6e9, points: 101, scale: 'linear' }).space({ kind: 'free-space' }).excitation({ port: scene.port('feed'), incidentPowerW: 1, phaseDeg: 0 }).observation({ frequenciesHz: [2.45e9], thetaDeg: { from: 0, to: 180, step: 2 }, phiDeg: { from: 0, to: 360, step: 2 }, distanceM: 3 }).outputs(['sParameters', 'radiation']).prepare());
}

// Compile-time negative vocabulary: the comments must remain consumed by tsc.
export function negativeVocabulary(a: AnalysisContext) {
  // @ts-expect-error SI only accepts channels, not channel
  a.si().channel({});
  // @ts-expect-error unknown setter
  a.dc().mystery(1);
  // @ts-expect-error no transport/backend selection in physical API
  a.em().solver('openems');
  // @ts-expect-error wrong unit type
  a.dc().temperatureC('25C');
}
