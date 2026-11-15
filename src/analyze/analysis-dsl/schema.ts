import { z } from 'zod';
import type { AnalysisKind, Diagnostic } from './spec-doc.js';

export class AnalysisDslError extends Error {
  constructor(readonly code: string, message: string, readonly diagnostics: readonly Diagnostic[] = []) {
    super(message); this.name = 'AnalysisDslError';
  }
}
export function invalid(message: string): never { throw new AnalysisDslError('INVALID_CONFIGURATION', message); }
export const text = z.string().min(1);
const finite = z.number().finite(), positive = finite.positive(), nonnegative = finite.nonnegative();
const integer = positive.int();
export const point = z.strictObject({ x: finite, y: finite });
export const polygon = z.strictObject({ outer: z.array(point).min(3), holes: z.array(z.array(point).min(3)).optional() });
export const selector = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('pad'), designator: text, number: text }),
  z.strictObject({ kind: z.literal('padId'), id: text }),
  z.strictObject({ kind: z.literal('component'), designator: text }),
  z.strictObject({ kind: z.literal('net'), name: text }),
  z.strictObject({ kind: z.literal('region'), layers: z.array(text).min(1), polygon }),
]);
export const viaSelector=z.strictObject({kind:z.literal('viaId'),id:text});
export const handle = z.strictObject({ kind: text, id: text, contextId: text });
export const port = handle.refine(v => v.kind === 'port', 'Expected a port');
const hs = (kinds: string[]) => handle.refine(v => kinds.includes(v.kind), `Expected ${kinds.join('/')}`);
export const channel = hs(['channel']), line = hs(['line']), circuit = hs(['circuit']);
export const frequency = z.strictObject({ startHz: positive, stopHz: positive, points: integer, scale: z.enum(['linear', 'log']) })
  .refine(v => v.stopHz >= v.startHz && (v.startHz === v.stopHz || v.points >= 2), 'Invalid frequency interval/points');
export const time = z.strictObject({ stopS: positive, sampleStepS: positive }).refine(v => v.sampleStepS <= v.stopS, 'sampleStepS exceeds stopS');
export const waveform = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('step'), from: finite, to: finite, atS: nonnegative, riseS: nonnegative }),
  z.strictObject({ kind: z.literal('sine'), offset: finite, amplitude: nonnegative, periodS: positive, phaseDeg: finite }),
  z.strictObject({ kind: z.literal('pulse'), low: finite, high: finite, delayS: nonnegative, riseS: nonnegative, highS: nonnegative, fallS: nonnegative, periodS: positive, count: integer })
    .refine(v => v.riseS + v.highS + v.fallS <= v.periodS, 'Pulse edges and plateau exceed period'),
  z.strictObject({ kind: z.literal('pwl'), points: z.array(z.tuple([nonnegative, finite])).min(2), interpolation: z.enum(['linear', 'hold']), repeat: z.strictObject({ periodS: positive, count: integer }).optional() })
    .refine(v => v.points[0][0] === 0 && v.points.every((p, i) => i === 0 || p[0] > v.points[i - 1][0])
      && (!v.repeat || v.points.at(-1)![0] === v.repeat.periodS), 'PWL starts at zero, strictly increases and ends at repeat period'),
]);
export const environment = z.strictObject({ name: text, ambientC: z.union([finite, waveform]) });
export const validity = z.strictObject({ status: z.enum(['valid', 'conditional', 'unavailable']), reason: text.optional(), domain: z.json().optional() });
export const resource = z.strictObject({ id: text, kind: text, source: text.optional(), analyses: z.array(z.enum(['geometry', 'dc', 'pi', 'thermal', 'electrothermal', 'impedance', 'tdr', 'si', 'eye', 'emi', 'em', 'networkParameters'])).optional(), ports: z.array(text).optional(), parameters: z.record(text, finite).optional(), validity: validity.optional(), data: z.json().optional() });
export const target = z.union([hs(['line', 'channel', 'differential', 'scene', 'antenna', 'network', 'connect', 'board']), resource]);
export const material = z.strictObject({ id: text.optional(), conductivitySM: positive.optional(), relativePermittivity: positive.optional(), lossTangent: nonnegative.optional(), thermalConductivityWPerMK: positive.optional(), densityKgM3: positive.optional(), specificHeatJPerKgK: positive.optional(), referenceC: finite.optional(), temperatureCoefficientPerK: finite.optional(), source: text.optional(), state: text.optional(), kind: z.literal('material').optional(), data: z.json().optional(), validity: validity.optional(), analyses: resource.shape.analyses });
const thermalNode = z.strictObject({ id: text, boundary: z.literal('environment').optional(), heatCapacityJPerK: positive.optional() });
const coolingModel = z.strictObject({ kind: z.literal('cooling'), resistanceKPerW: positive.optional(), scale: waveform.optional(), table: z.array(z.strictObject({ deltaTemperatureK: finite, heatFlowW: finite })).min(2).optional() }).refine(v => Number(v.resistanceKPerW !== undefined) + Number(v.table !== undefined) === 1 && (!v.table || v.table.some(p=>p.deltaTemperatureK===0&&p.heatFlowW===0)&&v.table.every((p, i) => i === 0 || p.deltaTemperatureK > v.table![i - 1].deltaTemperatureK&&p.heatFlowW>v.table![i-1].heatFlowW)), 'Choose cooling resistance or a strictly increasing passive deltaTemperatureK/heatFlowW table containing (0,0)');
// Model resources are data, not file paths. Validate local kinds before accepting them.
const thermalLinkBase = { id: text.optional(), from: text, to: text, resistanceKPerW: positive.optional(), model: z.union([resource, z.strictObject({ kind: z.literal('thermal-interface'), resistanceKPerW: positive }), coolingModel]).optional() };
const thermalLink = z.strictObject(thermalLinkBase).refine(v => Number(v.resistanceKPerW !== undefined) + Number(v.model !== undefined) === 1, 'Choose link resistance or model');
const capacitorFields = { capacitanceF: positive, esrOhm: nonnegative.optional(), eslH: nonnegative.optional(), ideal: z.boolean().optional() };
const inductorFields = { inductanceH: positive, seriesResistanceOhm: nonnegative.optional(), ideal: z.boolean().optional() };
function capMode(v: { ideal?: boolean; esrOhm?: number; eslH?: number }) { return v.ideal === true ? v.esrOhm === undefined && v.eslH === undefined : v.esrOhm !== undefined && v.eslH !== undefined; }
function indMode(v: { ideal?: boolean; seriesResistanceOhm?: number }) { return v.ideal === true ? v.seriesResistanceOhm === undefined : v.seriesResistanceOhm !== undefined; }
export const localModel = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('resistor'), resistanceOhm: positive, referenceC: finite.optional(), temperatureCoefficientPerK: finite.optional() }),
  z.strictObject({ kind: z.literal('capacitor'), ...capacitorFields }).refine(capMode, 'Specify parasitics or explicit ideal:true'),
  z.strictObject({ kind: z.literal('inductor'), ...inductorFields }).refine(indMode, 'Specify series resistance or explicit ideal:true'),
  z.strictObject({ kind: z.literal('linear-driver'), lowV: finite, highV: finite, outputResistanceOhm: positive }).refine(v => v.highV > v.lowV, 'highV must exceed lowV'),
  z.strictObject({ kind: z.literal('rc-input'), capacitanceF: positive, resistanceOhm: positive.optional() }),
  z.strictObject({ kind: z.enum(['thermal-network', 'thermal-path']), nodes: z.array(thermalNode).min(1), links: z.array(thermalLink), contacts:z.array(z.strictObject({node:text,target:selector,resistanceKPerW:positive})).optional() }),
  z.strictObject({ kind: z.literal('heatsink'), heatCapacityJPerK: positive }),
  z.strictObject({ kind: z.literal('thermal-interface'), resistanceKPerW: positive }),
  coolingModel,
]);
export const model = z.union([localModel, resource]);
export const componentInput = z.strictObject({ designator: text, model: model.optional(), pins: z.record(text, z.union([text, z.array(text)])).optional(), ground: z.array(text).optional(), supplies: z.record(text, z.array(text)).optional(), thermalNodes: z.array(text).optional(), resistanceOhm: positive.optional(), capacitanceF: positive.optional(), inductanceH: positive.optional(), esrOhm: nonnegative.optional(), eslH: nonnegative.optional(), seriesResistanceOhm: nonnegative.optional(), referenceC: finite.optional(), temperatureCoefficientPerK: finite.optional(), ideal: z.boolean().optional() });
export const componentDescription = componentInput.extend({ kind: z.enum(['ic', 'resistor', 'capacitor', 'inductor', 'device']) });
const maskSide = z.strictObject({ thicknessMm: positive.optional(), relativePermittivity: positive.optional() });
const mask = z.strictObject({ top: maskSide.optional(), bottom: maskSide.optional() });
export const stackupInput = z.strictObject({ platingMaterial: text.optional(), diameterConvention: z.enum(['finished_hole', 'before_plating']).optional(), boardThicknessMm: positive.optional(), fallbackCopperThicknessOz: positive.optional(), viaPlatingThicknessUm: positive.optional(), solderMask: mask.optional(), layers: z.array(z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('copper'), name: text, thicknessMm: positive.optional(), thicknessOz: positive.optional(), material: text.optional(), backgroundMaterial:text.optional() }),
  z.strictObject({ kind: z.literal('dielectric'), name: text, thicknessMm: positive, material: text.optional(), relativePermittivity: positive.optional(), lossTangent: nonnegative.optional() }),
])).min(1) });
export const stackupDescription = z.strictObject({ kind: z.literal('stackup'), input: stackupInput, routing: z.record(z.string(), z.unknown()) });
export const conductor = z.strictObject({ lengthMm: positive, widthMm: positive, thicknessMm: positive, material });
const lineConductorMaterial = z.strictObject({conductivitySM:positive,state:text,id:text.optional(),source:text.optional(),kind:z.literal('material').optional()});
const lineReferencePlane = z.strictObject({thicknessMm:positive,material:lineConductorMaterial});
const lineConductors = z.strictObject({signal:lineConductorMaterial,referencePlanes:z.strictObject({bottom:lineReferencePlane,top:lineReferencePlane.optional()})});
const lineCommon = { widthMm: positive, copperThicknessMm: positive, lengthMm: positive.optional(), solderMask: mask.optional(),conductors:lineConductors.optional() };
export const lineInput = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('microstrip'), ...lineCommon, dielectricHeightMm: positive, material }),
  z.strictObject({ kind: z.literal('coupled-microstrip'), ...lineCommon, dielectricHeightMm: positive, material, gapMm: positive }),
  z.strictObject({ kind: z.literal('stripline'), ...lineCommon, topDielectricHeightMm: positive, bottomDielectricHeightMm: positive, topMaterial: material, bottomMaterial: material }),
  z.strictObject({ kind: z.literal('coupled-stripline'), ...lineCommon, topDielectricHeightMm: positive, bottomDielectricHeightMm: positive, topMaterial: material, bottomMaterial: material, gapMm: positive }),
 ]).superRefine((line,ctx)=>{if(!line.conductors)return;const top=line.conductors.referencePlanes.top;if(line.kind.includes('microstrip')&&top)ctx.addIssue({code:'custom',message:'Microstrip supports bottom reference only',path:['conductors','referencePlanes','top']});if(!line.kind.includes('microstrip')&&!top)ctx.addIssue({code:'custom',message:'Stripline requires explicit top reference',path:['conductors','referencePlanes','top']});});
const element = z.discriminatedUnion('kind', [
  z.strictObject({ id: text, kind: z.literal('resistor'), nodes: z.tuple([text, text]), resistanceOhm: positive, referenceC: finite.optional(), temperatureCoefficientPerK: finite.optional() }),
  z.strictObject({ id: text, kind: z.literal('capacitor'), nodes: z.tuple([text, text]), ...capacitorFields }).refine(capMode, 'Specify parasitics or ideal:true'),
  z.strictObject({ id: text, kind: z.literal('inductor'), nodes: z.tuple([text, text]), ...inductorFields }).refine(indMode, 'Specify series resistance or ideal:true'),
  z.strictObject({ id: text, kind: z.literal('voltage-source'), nodes: z.tuple([text, text]), voltageV: finite, model: z.union([z.literal('ideal'), model]), waveform: waveform.optional() }),
  z.strictObject({ id: text, kind: z.literal('current-source'), nodes: z.tuple([text, text]), currentA: finite, waveform: waveform.optional() }),
  z.strictObject({ id: text, kind: z.literal('model'), model: z.union([model,componentDescription]), pins: z.record(text, text) }),
]);
export const circuitInput = z.strictObject({ nodes: z.array(text).min(1), reference: text.optional(), ports: z.array(z.strictObject({ id: text, positive: text, negative: text })), elements: z.array(element).min(1) });
export const thermalNetworkInput = z.strictObject({ nodes: z.array(thermalNode).min(1), links: z.array(thermalLink), sources: z.array(z.strictObject({ node: text, powerW: z.union([nonnegative, waveform]) })) });
export const stimulus = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('bits'), bits: z.string().regex(/^[01]+$/), bitRateBps: positive, riseS: nonnegative, fallS: nonnegative, repeatCount: integer }),
  z.strictObject({ kind: z.literal('prbs'), order: integer.max(31), seed: integer, bits: integer, bitRateBps: positive, riseS: nonnegative, fallS: nonnegative }).refine(v => v.seed < 2 ** v.order, 'Seed exceeds PRBS order'),
  z.strictObject({ kind: z.literal('waveform'), voltageV: waveform }),
]);
const binding = { port: port.optional(), ports: z.record(text, port).optional(), model };
const hasBinding = (v: { port?: unknown; ports?: Record<string, unknown> }) => Number(!!v.port) + Number(!!v.ports && Object.keys(v.ports).length > 0) === 1;
const driver = z.strictObject({ ...binding, stimulus }).refine(hasBinding, 'Choose port or ports');
const receiver = z.strictObject(binding).refine(hasBinding, 'Choose port or ports');
export const spectrum = z.strictObject({ frequencyHz: z.array(positive).min(1), values: z.array(z.strictObject({ real: finite, imaginary: finite })).min(1), unit: z.enum(['V', 'A']) })
  .refine(v => v.values.length === v.frequencyHz.length && v.frequencyHz.every((f, i) => i === 0 || f > v.frequencyHz[i - 1]), 'Spectrum axes must match and increase');
const source = z.strictObject({ port, voltageV: finite.optional(), currentA: finite.optional(), model: z.union([z.literal('ideal'), model]).optional(), waveform: waveform.optional(), voltageSpectrum: spectrum.optional(), currentSpectrum: spectrum.optional() })
  .refine(v => [v.voltageV, v.currentA, v.voltageSpectrum, v.currentSpectrum].filter(x => x !== undefined).length === 1 && (v.voltageSpectrum !== undefined || v.currentSpectrum !== undefined || v.model !== undefined), 'Source needs one excitation and explicit model/ideal');
const net = selector.refine(v => v.kind === 'net', 'Expected net selector');
export const componentLoad = z.strictObject({ designator: text, rail: net, currentA: nonnegative, return: net.optional(), domain: text.optional(), pins: z.array(text).optional(), returnPins: z.array(text).optional(), distribution: z.literal('equal').optional(), returnDistribution: z.literal('equal').optional(), waveform: waveform.optional(), pinCurrentsA: z.record(text, z.union([nonnegative, waveform])).optional(), returnPinCurrentsA: z.record(text, z.union([nonnegative, waveform])).optional() });
const portLoad = z.strictObject({ port, currentA: nonnegative.optional(), powerW: nonnegative.optional(), resistanceOhm: positive.optional(), model: model.optional(), waveform: waveform.optional() }).refine(v => [v.currentA, v.powerW, v.resistanceOhm, v.model].filter(x => x !== undefined).length === 1, 'Choose one load mode');
const loads = z.union([z.array(z.union([portLoad, componentLoad])), z.strictObject({ rail: net, defaultCurrentA: nonnegative, return: net.optional(), overrides: z.record(text, z.strictObject({ currentA: nonnegative })) })]);
const heat = z.union([z.array(z.strictObject({ target: selector, node: text, powerW: z.union([nonnegative, waveform]) })), z.strictObject({ category: z.enum(['ic', 'device', 'resistor']), defaultPowerW: nonnegative, overrides: z.record(text, z.strictObject({ powerW: nonnegative })) })]);
const cooling = z.strictObject({ boardSurfaces: model.optional(), heatsinks: z.array(z.strictObject({ target: selector, thermalNode: text, model, interface: model, cooling: model })).optional() });
export const numerics = z.strictObject({ relativeTolerance: positive.optional(), absoluteTolerance: positive.optional(), maxIterations: integer.optional(), maxStepS:positive.optional() });
export const rule = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('connectivity') }),
  z.strictObject({ kind: z.enum(['clearance', 'edge-clearance']), minMm: nonnegative }),
  z.strictObject({ kind: z.literal('length'), channel, maxMm: positive }),
  z.strictObject({ kind: z.literal('skew'), channels: z.array(channel).min(2), maxMm: nonnegative }),
  z.strictObject({ kind: z.literal('reference-continuity'), channel }),
  z.strictObject({ kind: z.literal('return-transition'), channel, maxDistanceMm: positive }),
]);
const window = z.strictObject({ startS: nonnegative, stopS: positive }).refine(v => v.stopS > v.startS, 'Invalid observation window');
const edges = z.enum(['rising', 'falling', 'both']);
export const timingOptions = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('sampling'), data: z.array(z.strictObject({ port, lowV: finite, highV: finite }).refine(v => v.lowV < v.highV, 'lowV must be below highV')).min(1), clock: z.strictObject({ port, thresholdV: finite, edges, sampleOffsetS: finite }), window }),
  z.strictObject({ kind: z.literal('propagation'), paths: z.array(z.strictObject({ from: port, to: port, sourceThresholdV: finite, destinationThresholdV: finite })).min(1), edges, maxDelayS: positive, window }),
]);
const finalState = z.strictObject({ kind: z.literal('finalState'), contextId: text, analysisId: text, data: z.json() });
export const data = z.strictObject({ kind: z.enum(['scalar', 'series', 'map', 'table', 'eye', 'polar', 'bar']), unit: text, validity, diagnostics: z.array(z.strictObject({ code: text, message: text, path: text.optional(), severity: z.enum(['error', 'warning', 'info']) })), value: z.union([finite, z.boolean(), z.string(), z.strictObject({ real: finite, imaginary: finite })]).optional(), values: z.array(z.union([finite, z.null(), z.strictObject({ real: finite, imaginary: finite })])).optional(), axis: z.strictObject({ name: z.enum(['timeS', 'frequencyHz']), unit: z.enum(['s', 'Hz']), values: z.array(finite) }).optional(), data: z.json().optional(), contextId: text.optional() });
const physical = { components: z.array(componentDescription).optional(), stackup: stackupDescription.optional(), materials: z.record(text, material).optional() };
const electrical = { ...physical, sources: z.array(source).optional(), loads: loads.optional(), temperatureC: finite.optional(), circuit: circuit.optional() };
const thermal = { ...physical, network: hs(['thermalNetwork']).optional(), heat: heat.optional(), losses: data.optional(), cooling: cooling.optional(), environment: environment.optional(), initialC: z.union([finite, z.literal('ambient')]).optional(), initial: z.union([z.literal('steady'), finalState]).optional(), domain: z.enum(['steady', 'time']).optional(), atS: nonnegative.optional(), time: time.optional(), mesh:z.strictObject({maxEdgeMm:positive.optional(),geometryDeviationMm:positive.min(1e-6).max(.01).optional(),maxNodes:integer.max(140000).optional(),refinement:z.strictObject({maxPasses:integer.max(6),relativeTolerance:positive.min(1e-4).max(.02),validationMaxEdgeMm:positive}).optional()}).refine(v=>(v.maxNodes??50000)<=100000||v.refinement!==undefined,'Nodes above100000 require explicit opt-in refinement').refine(v=>!v.refinement||v.refinement.validationMaxEdgeMm<(v.maxEdgeMm??2),'Independent validation edge must be smaller than seed edge').optional(),inputPolicy:z.strictObject({cadFill:z.enum(['strict','snapshot'])}).optional() };
const freq = { frequency: frequency.optional(), frequencyHz: positive.optional() };
const refOhm = z.union([positive, z.record(text, positive)]);
const measurement = z.strictObject({ id: text.optional(), port, quantity: z.enum(['voltage', 'current']), model: model.optional(), detector: z.enum(['rms', 'peak']).optional(), bandwidthHz: positive.optional() });
const excitation = z.union([
  z.strictObject({ name: text.optional(), port, voltageV: z.union([finite, waveform]).optional(), currentA: z.union([finite, waveform]).optional(), outputResistanceOhm: positive.optional(), voltageSpectrum: spectrum.optional(), currentSpectrum: spectrum.optional() }),
  z.strictObject({ name: text, model: resource, placement: z.strictObject({ positionMm: z.strictObject({ x: finite, y: finite, z: finite }), rotationDeg: z.strictObject({ x: finite, y: finite, z: finite }) }), voltageV: z.union([finite, waveform]).optional(), currentA: z.union([finite, waveform]).optional() }),
]).refine(v => ['voltageV', 'currentA', 'voltageSpectrum', 'currentSpectrum'].filter(k => k in v && v[k as keyof typeof v] !== undefined).length === 1, 'Choose one excitation');
const emExcitation = z.union([z.strictObject({ port, incidentPowerW: positive, phaseDeg: finite }), excitation]);
const angleRange = z.strictObject({ from: finite, to: finite, step: positive }).refine(v => v.to >= v.from);
const common = { name: text.optional(), numerics: numerics.optional(), budget: z.strictObject({ maxSeconds: positive.optional(), maxMemoryMiB: positive.optional() }).optional() };
const physicalState=z.strictObject({kind:z.literal('physical-state'),steady:z.boolean(),potentialsV:z.array(z.strictObject({instanceId:text,node:text,value:finite})).max(1024),currentsA:z.array(z.strictObject({instanceId:text,dof:text,value:finite})).max(1536)});
export const schemas = {
  geometry: z.strictObject({ ...common, scope: z.array(selector).optional(), rules: z.array(rule).optional() }),
  dc: z.strictObject({ ...common, ...electrical, conductor: hs(['conductor']).optional(), currentA: finite.optional(), voltageV: finite.optional(),
    inputPolicy: z.strictObject({ cadFill: z.enum(['strict', 'snapshot']) }).optional(),
    mesh: z.strictObject({ maxAreaMm2: positive.min(1e-8).max(1e6).optional(), minAngleDeg: nonnegative.max(30).optional(), maxNodes: integer.min(3).max(2_000_000).optional(), maxTriangles: integer.max(4_000_000).optional(), maxRefinementSteps: nonnegative.int().max(64).optional(), geometryDeviationMm: positive.min(1e-6).max(.01).optional() }).optional(),
  }),
  pi: z.strictObject({ ...common, ...electrical, ...freq, frequencyHz: nonnegative.optional(), domain: z.enum(['frequency', 'time']).optional(), time: time.optional(), initial: z.union([z.literal('steady'), finalState]).optional() }),
  thermal: z.strictObject({ ...common, ...thermal }),
  electrothermal: z.strictObject({ ...common, stackup: physical.stackup, materials: physical.materials, electrical: z.strictObject({ ...electrical, operating: z.array(z.strictObject({ target: selector, gateVoltageV: finite })).optional() }).optional(), thermal: z.strictObject(thermal).optional(), couplings: z.array(z.union([z.strictObject({ target: selector, thermalNode: text }), z.strictObject({ element: text, thermalNode: text })])).optional(), time: time.optional(), coupling: z.strictObject({ toleranceK: positive.optional(), relativeLossTolerance: positive.optional(), maxIterations: integer.max(1000).optional(), relaxation: positive.max(1).optional() }).optional() }),
  impedance: z.strictObject({ ...common, ...physical, ...freq, frequencyHz:nonnegative.optional(), mesh:z.strictObject({maxNodes:integer.min(16).max(300000).optional(),maxTriangles:integer.min(16).max(600000).optional(),maxRefinements:integer.max(5).optional(),maxEdgeMm:positive.optional(),domainScale:positive.min(4).max(32).optional()}).optional(), target: target.optional(), method: z.enum(['formula', 'cross-section', 'layered']).optional(), temperatureC: finite.optional() }),
  tdr: z.strictObject({ ...common, ...physical, target: target.optional(), referenceOhm: refOhm.optional(), riseS: positive.optional(), termination: z.union([z.enum(['matched', 'open', 'short']), model]).optional(), time: time.optional(), temperatureC: finite.optional() }),
  si: z.strictObject({ ...common, ...physical, initial:z.union([z.literal('steady'),physicalState]).optional(), channels: z.array(hs(['channel', 'differential', 'line'])).min(1).optional(), network: hs(['network', 'connect', 'line']).optional(), drivers: z.array(driver).min(1).optional(), receivers: z.array(receiver).min(1).optional(), time: time.optional(), temperatureC: finite.optional() }),
  eye: z.strictObject({ ...common, signal: data.optional(), bitRateBps: positive.optional(), discardS: nonnegative.optional(), phaseS: nonnegative.optional(), thresholdV: finite.optional() }),
  emi: z.strictObject({ ...common, ...electrical, ...freq, kind: z.enum(['risk', 'conducted', 'coupling']).optional(), signals: z.array(z.strictObject({ channel: hs(['channel', 'differential']), riseS: positive })).optional(), rules: z.array(rule).optional(), excitations: z.array(excitation).optional(), measurement: z.union([measurement, z.array(measurement).min(1)]).optional(), time: time.optional(), spectrum: z.strictObject({ discardS: nonnegative, window: z.enum(['hann', 'rectangular']), startHz: positive, stopHz: positive }).optional() }),
  em: z.strictObject({ ...common, ...physical, ...freq, target: target.optional(), method: z.literal('full-wave').optional(), space: z.strictObject({ kind: z.literal('free-space') }).optional(), outputs: z.array(z.enum(['sParameters', 'fields', 'radiation'])).min(1).optional(), excitation: emExcitation.optional(), excitations: z.array(emExcitation).min(1).optional(), observation: z.strictObject({ frequenciesHz: z.array(positive).min(1), thetaDeg: angleRange, phiDeg: angleRange, distanceM: positive }).optional(), temperatureC: finite.optional() }),
  networkParameters: z.strictObject({ ...common, ...freq, network: hs(['network', 'connect', 'line']).optional(), representation: z.enum(['S', 'Z', 'Y']).optional(), referenceOhm: refOhm.optional() }),
} satisfies Record<AnalysisKind, z.ZodType>;

export function parse<T extends z.ZodType>(schema: T, value: unknown): z.output<T> {
  const result = schema.safeParse(value);
  if (!result.success) throw new AnalysisDslError('INVALID_CONFIGURATION', result.error.issues.map(i => `${i.path.join('.') || 'input'}: ${i.message}`).join('; '), result.error.issues.map(i => ({ code: 'INVALID_CONFIGURATION', severity: 'error', path: i.path.join('.'), message: i.message })));
  return result.data;
}
