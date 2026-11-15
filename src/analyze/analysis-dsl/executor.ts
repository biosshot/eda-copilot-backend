import {createLocalExecutor,isLocalTarget} from './local-executor.ts';
import {createGroupExecutor} from './group-executor.ts';
import {createGeometryExecutor} from './geometry-executor.ts';
import {createCrossSectionExecutor} from './cross-section-executor.ts';
import {createFrequencyExecutor} from './frequency-executor.ts';
import type { AnalysisKind, Configurations, DataDescription, Diagnostic, Json, Network, Resource, Results, TimingOptions } from './spec-doc.js';
import type { RoutingBoardV2 } from '../../types/pcb/routing-board-v2.ts';
import type { AnalysisPhysicalProfile } from '../../../native/analyze/index.js';
import { z } from 'zod';
import { AnalysisDslError, parse, port, selector, text, timingOptions } from './schema.ts';
import { freeze, json, unavailable, value } from './data.ts';
import { createDcExecutor } from './dc-executor.ts';
import { dcObservedVoltage } from './dc-observation.ts';
import {createElectricalExecutor,circuitObservedVoltage,circuitObservedTransfer} from './electrical-executor.ts';
import {createThermalExecutor,thermalObservedData} from './thermal-executor.ts';
import {createElectrothermalExecutor} from './electrothermal-executor.ts';
import { object } from './descriptions.ts';
import {createPiBoardExecutor} from './pi-board-executor.ts';
import {createTransientExecutor,transientObservedVoltage} from './transient-executor.ts';
import {eyeFromSignal,timingFromSignals} from './timing-eye.ts';

/** Small analysis dispatch boundary. No transport/session, no callbacks, no
 * internal mesh or import operation masquerading as a completed analysis. */
export interface AnalysisTask<K extends AnalysisKind = AnalysisKind> {
  kind: K; contextId: string; boardIdentity?: string; board?: RoutingBoardV2;
  config: Readonly<Configurations[K]>; descriptors: readonly Json[];
  variantChanges: readonly Json[];
  conductionProfile?: AnalysisPhysicalProfile;
  resolvedLoads?: readonly Json[];
  electricalModels?: Json;
  /** Internal references to already accepted bounded report solutions, only for
   * same-context transfer; never a second published mesh/cache. */
  completedSolutions?: readonly Json[];
  /** Optional absolute monotonic deadline for the complete opt-in thermal run,
   * including adapter and public-result construction; never a per-pass reset. */
  deadlineAtMs?: number;
}
/** Internal serializable result envelope; accessors are created on JS side.
 * Test executors inject this envelope explicitly; production never fabricates it. */
export interface ExecutionResult {
  id: string; status: 'complete' | 'partial' | 'failed'; method: string;
  diagnostics: readonly Diagnostic[]; assumptions: readonly string[];
  metrics: Readonly<Record<string, DataDescription>>;
  finalState?: Json; violations?: readonly Json[];
  network?: Resource;
  electrical?: ExecutionResult; thermal?: ExecutionResult;
  solution?: Json;
}
export type AnalysisExecutor = (task: AnalysisTask) => Promise<ExecutionResult>;
async function eye(task: AnalysisTask<'eye'>): Promise<ExecutionResult> {
  const config = task.config as Configurations['eye'];
  const analysis = eyeFromSignal({ signal: config.signal as DataDescription, bitRateBps: config.bitRateBps!, discardS: config.discardS!, phaseS: config.phaseS!, thresholdV: config.thresholdV! });
  return { id: `${task.contextId}:eye`, status: analysis.status, method: 'Supplied time-voltage fold (post-processing, no solver)', diagnostics: analysis.diagnostics, assumptions: analysis.assumptions, metrics: analysis.metrics };
}
export function createAnalysisExecutor(group = createGroupExecutor()): AnalysisExecutor {
  const local=createLocalExecutor();
  const geometry = createGeometryExecutor();
  const frequency = createFrequencyExecutor();
  const crossSection = createCrossSectionExecutor();
  const dc = createDcExecutor();
  const electrical=createElectricalExecutor();
  const transient=createTransientExecutor();
  const piBoard=createPiBoardExecutor();
  const thermal=createThermalExecutor();
  const electrothermal=createElectrothermalExecutor(electrical,thermal);
  return async task => {
    if (task.kind==='thermal') return thermal(task as AnalysisTask<'thermal'>);
    if (task.kind==='electrothermal') return electrothermal(task as AnalysisTask<'electrothermal'>);
    if(isLocalTarget(task))return local(task as AnalysisTask<'impedance'>);
    if (task.kind==='geometry') return geometry(task as AnalysisTask<'geometry'>);
    if (task.kind==='impedance') return crossSection(task as AnalysisTask<'impedance'>);
    if (task.kind==='networkParameters') return frequency(task as AnalysisTask<'networkParameters'>);
    if (task.kind==='eye') return eye(task as AnalysisTask<'eye'>);
    if (task.kind === 'si') {
      const si = task as AnalysisTask<'si'>;
      return si.config.channels ? group(si) : transient(si);
    }
    if ((task.kind==='dc'||task.kind==='pi')&&'circuit'in task.config&&task.config.circuit) return electrical(task as AnalysisTask<'dc'|'pi'>);
    if (task.kind==='pi') return piBoard(task as AnalysisTask<'pi'>);
    if (task.kind === 'dc') return dc(task as AnalysisTask<'dc'>);
    throw new AnalysisDslError('ANALYZER_NOT_IMPLEMENTED', `Analysis executor '${task.kind}' is not implemented`, [{ code: 'ANALYZER_NOT_IMPLEMENTED', severity: 'error', message: `No production ${task.kind} solver is installed` }]);
  };
}

const properties: Record<AnalysisKind, Record<string, string>> = {
  geometry: { measurements: 'mm' },
  dc: { resistance: 'Ohm', losses: 'W', lossMap: 'W', voltageMap: 'V', currentDensityMap: 'A/mm2' },
  pi: { resistance: 'Ohm', losses: 'W', lossMap: 'W', voltageMap: 'V', currentDensityMap: 'A/mm2', resonances: 'Hz' },
  thermal: { temperatureMap: 'degC', energyBalance: 'J' },
  electrothermal: { losses: 'W', convergence: '1' },
  impedance: { characteristicImpedance: 'Ohm', rlgc: '1', delay: 's', segments: '1' },
  tdr: { reflection: '1', impedance: 'Ohm', distance: 'mm' },
  si: {}, eye: { diagram: 'V', height: 'V', width: 's', timingMargin: 's', voltageMargin: 'V' },
  emi: { risks: '1', spectra: 'V' }, em: { sParameters: '1', fields: 'V/m', radiation: 'W', pattern: '1', radiatedPower: 'W' }, networkParameters: {},
};
export function resultFor<K extends AnalysisKind>(task: AnalysisTask<K>, payload: ExecutionResult, validate: (input: unknown) => void, networkFactory: (model: Resource) => Network, describe?: (id: string) => unknown): Results[K] {
  const safe = freeze(json(payload)) as unknown as ExecutionResult;
  const result: Record<string, unknown> = { id: safe.id, kind: task.kind, contextId: task.contextId, status: safe.status, method: safe.method, inputs: json(task.config), diagnostics: safe.diagnostics, assumptions: safe.assumptions };
  if (safe.solution) result.solution = safe.solution;
  const metric = (name: string, unit: string, argument?: unknown) => {
    if (argument !== undefined) validate(argument);
    const suffix = argument && typeof argument === 'object' && 'id' in argument ? `:${argument.id}` : argument === undefined ? '' : `:${JSON.stringify(json(argument))}`;
    const descriptor=argument&&typeof argument==='object'&&'id'in argument?describe?.(String(argument.id))??task.descriptors.find(d=>object(d)&&d.id===argument.id):undefined;
    const d = safe.metrics[name + suffix] ?? (task.kind==='thermal'?thermalObservedData(task as AnalysisTask<'thermal'>,name,argument,safe):name==='transfer'&&'circuit'in task.config&&task.config.circuit?circuitObservedTransfer(task,argument,safe.metrics,describe):name==='voltage'&&'circuit'in task.config&&task.config.circuit?circuitObservedVoltage(task,descriptor,safe.metrics):task.kind==='dc'&&name==='voltage'?dcObservedVoltage(task.board,descriptor,safe.metrics):name==='transfer'&&task.kind==='pi'&&object(argument)&&object(argument.from)&&object(argument.to)?safe.metrics[`pi-transfer:${String(argument.from.id)}:${String(argument.to.id)}`]:undefined);
    const transient=task.kind==='si'&&name==='voltage'?transientObservedVoltage(task,descriptor,safe.metrics):undefined;
    if(task.kind==='si'&&name==='differentialVoltage'&&object(argument)&&object(argument.positive)&&object(argument.negative)){
      const get=(p:Record<string,unknown>)=>transientObservedVoltage(task,describe?.(String(p.id))??task.descriptors.find(d=>object(d)&&d.id===p.id),safe.metrics);
      const a=get(argument.positive),b=get(argument.negative);
      if(a?.values&&b?.values)return value({...a,contextId:task.contextId,values:a.values.map((v,i)=>typeof v==='number'&&typeof b.values![i]==='number'?v-(b.values![i] as number):null),validity:{status:a.validity.status==='conditional'||b.validity.status==='conditional'?'conditional':'valid'}});
    }
    return d||transient ? value({ ...(d??transient!), contextId: task.contextId }) : unavailable(unit, `Result metric '${name + suffix}' is unavailable`, task.contextId);
  };
  for (const [name, unit] of Object.entries(properties[task.kind])) result[name] = metric(name, unit);
  const accessors: Partial<Record<AnalysisKind, Record<string, string>>> = {
    dc: { voltage: 'V', current: 'A' }, pi: { voltage: 'V', current: 'A', impedance: 'Ohm', transfer: '1' },
    thermal: { temperature: 'degC', heatFlow: 'W' }, si: { voltage: 'V', differentialVoltage: 'V' },
    emi: { voltage: 'V', current: 'A', measurement: 'V' }, networkParameters: { parameter: (task.config as Configurations['networkParameters']).representation==='Z'?'Ohm':(task.config as Configurations['networkParameters']).representation==='Y'?'S':'1' },
  };
  const argumentsFor: Record<string, z.ZodType> = {
    voltage: port, impedance: port, current: z.union([port, selector, text]), transfer: z.strictObject({ from: port, to: port }),
    temperature: z.union([text, z.strictObject({ target: selector, node: text })]), heatFlow: text,
    differentialVoltage: z.strictObject({ positive: port, negative: port }), measurement: text,
    parameter: z.strictObject({ out: text, in: text }),
  };
  for (const [name, unit] of Object.entries(accessors[task.kind] ?? {}) as [string, string][]) Object.defineProperty(result, name, { value: (argument: unknown) => metric(name, unit, parse(argumentsFor[name], json(argument))) });
  if (task.kind === 'geometry') result.violations = freeze(Object.defineProperty({ items: safe.violations ?? [] }, 'count', { value: () => safe.metrics.violationCount ? value({...safe.metrics.violationCount, contextId: task.contextId}) : safe.violations ? value({ kind: 'scalar', value: safe.violations.length, unit: '1', validity: { status: safe.status === 'complete' ? 'valid' : 'unavailable', reason: safe.status === 'complete' ? undefined : 'Requested geometry coverage is incomplete; confirmed items remain available' }, diagnostics: [], contextId: task.contextId }) : unavailable('1', 'Violations were not evaluated', task.contextId) }));
  if (task.kind === 'si') Object.defineProperty(result, 'timing', { value: (options: TimingOptions) => {
    parse(timingOptions, json(options));
    validate(options);
    const timing = timingFromSignals(parse(timingOptions, json(options)) as TimingOptions, p => metric('voltage', 'V', parse(port, json(p))));
    return freeze({ ...Object.fromEntries((['samples', 'events', 'setup', 'hold', 'delay', 'skew', 'delaySpread'] as const).map(k => [k, value({ ...timing[k], contextId: task.contextId })])), diagnostics: timing.diagnostics });
  } });
  if (['pi', 'thermal'].includes(task.kind)) Object.defineProperty(result, 'finalState', { enumerable: !!safe.finalState, get: () => {
    if (safe.finalState === undefined) throw new AnalysisDslError('RESULT_UNAVAILABLE', 'No final state in result');
    return freeze({ kind: 'finalState', contextId: task.contextId, analysisId: safe.id, data: safe.finalState });
  } });
  if (task.kind === 'electrothermal') {
    for (const kind of ['electrical', 'thermal'] as const) Object.defineProperty(result, kind, { enumerable: !!safe[kind], get: () => {
      if (!safe[kind]) throw new AnalysisDslError('RESULT_UNAVAILABLE', `No ${kind} result`);
      return resultFor({ ...task, kind: kind === 'electrical' ? 'dc' : 'thermal', config: {} }, safe[kind], validate, networkFactory);
    } });
  }
  if (['impedance', 'em', 'networkParameters'].includes(task.kind)) {
    let network: Network | undefined;
    Object.defineProperty(result, 'network', { enumerable: !!safe.network, get: () => {
      if (!safe.network) throw new AnalysisDslError('RESULT_UNAVAILABLE', 'No extracted network in result');
      return network ??= networkFactory(safe.network);
    } });
  }
  return freeze(result) as unknown as Results[K];
}
