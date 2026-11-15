import {networkResource} from './frequency-model.ts';
import { createHash, randomUUID } from 'node:crypto';
import { z } from 'zod';
import type * as T from './spec-doc.js';
import * as S from './schema.ts';
import { freeze, json, ReportStore, value, type ReportUpdate } from './data.ts';
import { boardSnapshot, componentDescription, makeProfiles, object, stackup } from './descriptions.ts';
import { createAnalysisExecutor, resultFor, type AnalysisExecutor, type AnalysisTask } from './executor.ts';
import { prepare } from './prepare.ts';
import { conductionProfile } from './profile-adapter.ts';

export interface ContextState {
  id: string; board?: T.RoutingBoardV2; boardIdentity?: string; store: ReportStore;
  descriptions: Map<string, Record<string, unknown>>; changes: T.Json[];
  validate(input: unknown): void;
}
export interface DslInput { board?: T.RoutingBoardV2; profiles?: T.ProfilesInput; models?: T.Models }
export interface Limits { maxEntries: number; maxOutputBytes: number }

export function createAnalysisApi(input: DslInput, limits: Limits, changed: (update: ReportUpdate) => void, executor: AnalysisExecutor = createAnalysisExecutor()) {
  const reports: ReportStore[] = [];
  const budget = { entries: 0, bytes: 0 };
  const board = input.board === undefined ? undefined : boardSnapshot(input.board);
  const profiles = makeProfiles(input.profiles);
  const models = freeze(json(input.models ?? {}));
  if (!object(models) || Object.keys(models).some(k => !['materials', 'electrical', 'thermal', 'em'].includes(k))) S.invalid('Unknown models catalog category');
  const electricalModels=object(models)&&object(models.electrical)?models.electrical:undefined;
  const snapshot = () => reports.map(r => r.snapshot());
  function analysis(options?: { board: T.RoutingBoardV2 }, parent?: ContextState): T.AnalysisContext {
    if (reports.length >= limits.maxEntries) throw new S.AnalysisDslError('OUTPUT_LIMIT', 'Scenario context limit exceeded');
    if (options !== undefined && (!object(options) || Object.keys(options).length !== 1 || !('board' in options) || options.board === undefined)) S.invalid('analysis accepts only an explicit board; use analysis() for boardless models');
    const b = options ? boardSnapshot(options.board) : undefined;
    const id = `analysis-${reports.length + 1}-${randomUUID()}`;
    const identity = b ? createHash('sha256').update(JSON.stringify(b)).digest('hex') : undefined;
    const store = new ReportStore(id, identity, limits, changed, budget);
    reports.push(store);
    const descriptions = parent ? new Map(parent.descriptions) : new Map<string, Record<string, unknown>>();
    const state: ContextState = { id, board: b, boardIdentity: identity, store, descriptions, changes: [], validate };
    const acceptedContexts = new Set([id, ...(parent ? [...parent.descriptions.values()].map(d => String(d.contextId)) : [])]);
    function validate(input: unknown): void {
      const v = json(input);
      function visit(v: T.Json): void {
        if (Array.isArray(v)) { v.forEach(visit); return; }
        if (!object(v)) return;
        if(v.kind==='viaId'){
          const s=S.parse(S.viaSelector,v);
          if(!b)S.invalid('viaId selector requires an explicit board');
          const selected=[...b.copper.fixed.vias,...b.copper.editable.vias].filter(v=>v.id===s.id);
          if(selected.length!==1)S.invalid(`viaId ${s.id} resolves to ${selected.length} physical vias`);
        }
        if ('contextId' in v) {
          if (typeof v.contextId !== 'string' || !acceptedContexts.has(v.contextId)) S.invalid('Handle/result belongs to another analysis snapshot');
          if (['scalar', 'series', 'map', 'table', 'eye', 'polar', 'bar', 'finalState'].includes(String(v.kind)) && v.contextId !== id) S.invalid('Result data/final state cannot be reused across incompatible snapshots');
          if ('id' in v && 'kind' in v && v.kind !== 'finalState') {
            const descriptor = descriptions.get(String(v.id));
            if (!descriptor || descriptor.kind !== v.kind || descriptor.contextId !== v.contextId) S.invalid('Unknown or incompatible descriptor handle');
          }
        }
        if (v.kind === 'pad' || v.kind === 'padId' || v.kind === 'component' || v.kind === 'net' || v.kind === 'region') {
          const s = S.parse(S.selector, v);
          if (!b) S.invalid(`${s.kind} selector requires an explicit board`);
          if (s.kind === 'pad' || s.kind === 'padId') {
            const pads = b.pads.filter(p => s.kind === 'padId' ? p.id === s.id : p.component === s.designator && p.number === s.number);
            if (pads.length !== 1) S.invalid(`${JSON.stringify(s)} resolves to ${pads.length} physical pads; use explicit padId`);
          } else if (s.kind === 'component' && b.components.filter(c => c.designator === s.designator).length !== 1) S.invalid(`Unknown/ambiguous component ${s.designator}`);
          else if (s.kind === 'net' && b.nets.filter(n => n.name === s.name).length !== 1) S.invalid(`Unknown/ambiguous net ${s.name}`);
          else if (s.kind === 'region' && s.layers.some(l => !b.layers.some(x => x.name === l))) S.invalid('Unknown region layer');
        }
        for (const item of Object.values(v)) visit(item as T.Json);
      }
      visit(v);
    }
    function descriptor(kind: string, input: unknown, portNames?: readonly string[]): T.Handle {
      validate(input);
      const handle = { kind, id: `${id}:${kind}-${descriptions.size + 1}`, contextId: id };
      const description = freeze({ ...handle, input: json(input), ...(portNames ? { ports: [...portNames] } : {}) });
      descriptions.set(handle.id, description);
      store.add('descriptors', description);
      if (portNames) Object.defineProperty(handle, 'port', { value: (name: string) => {
        if (!portNames.includes(name)) S.invalid(`Unknown port '${name}' on ${kind}; available: ${portNames.join(', ')}`);
        const key = `${handle.id}:port:${name}`;
        const h = { kind: 'port', id: key, contextId: id };
        if (!descriptions.has(key)) descriptions.set(key, freeze({ ...h, input: { owner: handle.id, name } }));
        return freeze(h);
      } });
      return freeze(handle);
    }
    function build<K extends T.AnalysisKind>(kind: K, config: Record<string, unknown> = {}): T.Builder<K> {
      const builder: Record<string, unknown> = {};
      // Finite schema vocabulary. No Proxy and no arbitrary setter fallback.
      for (const key of Object.keys(S.schemas[kind].shape)) Object.defineProperty(builder, key, { value: (input: unknown) => {
        const section = (S.schemas[kind].shape as Record<string, z.ZodType>)[key];
        let supplied = input;
        if (key === 'target' && object(input) && 'pads' in input) {
          if (!b || JSON.stringify(boardSnapshot(input as unknown as T.RoutingBoardV2)) !== JSON.stringify(b)) S.invalid('Target board must be the explicit context board');
          supplied = { kind: 'board', id: `${id}:board`, contextId: id };
          descriptions.set(`${id}:board`, supplied as Record<string, unknown>);
        }
        const copied = S.parse(section, json(supplied));
        if (key === 'stackup') {
          const s = copied as T.StackupDescription;
          if (JSON.stringify(stackup(s.input).routing) !== JSON.stringify(s.routing)) S.invalid('Stackup adapter data does not match its input');
        }
        validate(copied);
        return build(kind, { ...config, [key]: freeze(copied) });
      } });
      const preparation = () => {
        const p = prepare(state, kind, config);
        store.add('calculations', { kind, phase: 'prepared', preparation: p });
        return p;
      };
      Object.defineProperties(builder, {
        prepare: { value: async () => preparation() },
        run: { value: async () => {
          const thermalConfig=kind==='thermal'?config as T.Configurations['thermal']:undefined;
          const deadlineAtMs=thermalConfig?.mesh?.refinement?performance.now()+1000*(thermalConfig.budget?.maxSeconds??120):undefined;
          const checkDeadline=()=>{if(deadlineAtMs!==undefined&&performance.now()>deadlineAtMs)throw new S.AnalysisDslError('THERMAL_DEADLINE','Total thermal refinement run deadline exceeded');};
          const p = preparation();
          checkDeadline();
          if (!p.structurallyValid) throw new S.AnalysisDslError('INVALID_CONFIGURATION', 'Analysis configuration is incomplete or inconsistent', p.diagnostics);
          const profile = kind === 'dc' ? conductionProfile(config.stackup as T.StackupDescription | undefined, config.materials as T.Materials | undefined, config.temperatureC as number | undefined) : undefined;
          const completedSolutions = kind==='thermal'&&config.losses ? store.report.calculations.flatMap(c=>object(c)&&c.kind==='dc'&&c.phase==='completed'&&object(c.result)&&c.result.status==='complete'&&object(c.result.solution)?[c.result.solution as T.Json]:[]) : undefined;
          const task: AnalysisTask<K> = freeze({ kind, contextId: id, ...(b ? { board: b, boardIdentity: identity } : {}), config: json(config) as T.Configurations[K], descriptors: [...descriptions.values()].map(json), variantChanges: json(state.changes) as T.Json[], resolvedLoads: p.components, ...(profile ? { conductionProfile: profile } : {}), ...((kind==='dc'||kind==='pi')&&config.circuit&&electricalModels?{electricalModels}:{}), ...(completedSolutions?{completedSolutions}:{}), ...(deadlineAtMs===undefined?{}:{deadlineAtMs}) });
          try {
            checkDeadline();
            const payload = await executor(task);
            checkDeadline();
            const result = resultFor(task, payload, validate, model => api.network({ model, ports: Object.fromEntries((model.ports ?? []).map(p => [p, p])) }), id => descriptions.get(id));
            checkDeadline();
            store.add('calculations', { kind, phase: 'completed', result: deadlineAtMs===undefined?json(result):result },deadlineAtMs);
            return result;
          } catch (error) {
            store.add('diagnostics', { code: error instanceof S.AnalysisDslError ? error.code : 'EXECUTOR_FAILED', severity: 'error', message: error instanceof Error ? error.message : String(error) });
            throw error;
          }
        } },
      });
      return freeze(builder) as T.Builder<K>;
    }
    const api = {
      geometry: () => build('geometry'), dc: () => build('dc'), pi: () => build('pi'), thermal: () => build('thermal'),
      electrothermal: () => build('electrothermal'), impedance: () => build('impedance'), tdr: () => build('tdr'), si: () => build('si'), eye: () => build('eye'), emi: () => build('emi'), em: () => build('em'), networkParameters: () => build('networkParameters'),
      port(v: Parameters<T.AnalysisContext['port']>[0]) {
        const pad = S.selector.refine(s => s.kind === 'pad' || s.kind === 'padId');
        const electrode=z.union([pad,S.viaSelector]);
        const parsed = S.parse(z.strictObject({ positive: z.union([electrode, z.array(electrode).min(1)]), negative: z.union([electrode, z.array(electrode).min(1)]), layer:S.text.optional(),positiveLayer:S.text.optional(),negativeLayer:S.text.optional() }), json(v));
        if(parsed.layer!==undefined&&(parsed.positiveLayer!==undefined||parsed.negativeLayer!==undefined))S.invalid('Use layer or separate positiveLayer/negativeLayer');
        for(const layer of [parsed.layer,parsed.positiveLayer,parsed.negativeLayer])if(layer!==undefined&&b&&!b.layers.some(l=>l.name===layer))S.invalid('Unknown physical port layer');
        return descriptor('port', parsed) as T.Port;
      },
      channel(v: Parameters<T.AnalysisContext['channel']>[0]) { return descriptor('channel', S.parse(z.strictObject({ from: S.port, to: S.port, via: z.array(S.selector).optional(), scope: z.array(S.selector).optional() }), json(v))) as T.Channel; },
      differential(v: Parameters<T.AnalysisContext['differential']>[0]) { return descriptor('differential', S.parse(z.strictObject({ positive: S.channel, negative: S.channel }), json(v))) as T.Differential; },
      conductor(v: Parameters<T.AnalysisContext['conductor']>[0]) { return descriptor('conductor', S.parse(S.conductor, json(v))) as T.Conductor; },
      line(v: T.LineInput) { const p = S.parse(S.lineInput, json(v)); return descriptor('line', p, p.kind.startsWith('coupled-') ? ['positiveInput', 'negativeInput', 'positiveOutput', 'negativeOutput'] : ['input', 'output']) as T.Line; },
      circuit(v: T.CircuitInput) {
        const p = S.parse(S.circuitInput, json(v));
        unique(p.nodes, 'circuit nodes'); unique(p.ports.map(x => x.id), 'circuit ports'); unique(p.elements.map(x => x.id), 'circuit elements');
        const reference = p.reference ?? (p.nodes.includes('gnd') ? 'gnd' : undefined);
        if (!reference || !p.nodes.includes(reference)) S.invalid('Circuit requires an explicit valid reference (or node gnd)');
        const nodes = [...p.ports.flatMap(x => [x.positive, x.negative]), ...p.elements.flatMap(x => 'nodes' in x ? x.nodes : Object.values(x.pins))];
        if (nodes.some(n => !p.nodes.includes(n))) S.invalid('Unknown circuit node');
        return descriptor('circuit', { ...p, reference }, p.ports.map(p => p.id)) as T.Circuit;
      },
      thermalNetwork(v: T.ThermalNetworkInput) {
        const p = S.parse(S.thermalNetworkInput, json(v)); unique(p.nodes.map(x => x.id), 'thermal nodes');
        const ids = new Set(p.nodes.map(x => x.id));
        if ([...p.links.flatMap(x => [x.from, x.to]), ...p.sources.map(x => x.node)].some(x => !ids.has(x))) S.invalid('Unknown thermal node');
        return descriptor('thermalNetwork', p) as T.ThermalNetwork;
      },
      model: ((v: T.LocalModel | T.NetworkResource) => {
        if (object(v) && v.kind === "network") return networkResource(v);
        const parsed = S.parse(S.localModel, json(v));
        if ('nodes' in parsed) {
          unique(parsed.nodes.map(n => n.id), 'thermal model nodes');
          const ids = new Set(parsed.nodes.map(n => n.id));
          if (parsed.links.some(l => !ids.has(l.from) || !ids.has(l.to))) S.invalid('Unknown thermal model node');
          if (parsed.contacts?.some(c => !ids.has(c.node))) S.invalid('Unknown thermal contact node');
          if (parsed.contacts) validate(parsed.contacts);
        }
        return freeze(parsed);
      }) as T.AnalysisContext["model"],
      scene(v: Parameters<T.AnalysisContext['scene']>[0]) {
        const p = S.parse(z.strictObject({ model: S.resource, parameters: z.record(S.text, z.number().finite()).optional() }), json(v));
        if (Object.keys(p.parameters ?? {}).some(k => !(k in (p.model.parameters ?? {})))) S.invalid('Unknown scene parameter');
        return descriptor('scene', p, p.model.ports ?? []) as T.Scene;
      },
      antenna(v: Parameters<T.AnalysisContext['antenna']>[0]) {
        if (!v || typeof v !== 'object' || Object.keys(v).some(k => !['structure', 'feed', 'models'].includes(k))) S.invalid('Invalid antenna description');
        const feed = S.parse(S.port, json(v.feed));
        const structure = 'kind' in v.structure && v.structure.kind === 'scene' ? S.parse(S.handle, json(v.structure)) : boardSnapshot(v.structure as T.RoutingBoardV2);
        if (!('kind' in structure) && (!b || JSON.stringify(structure) !== JSON.stringify(b))) S.invalid('Antenna structure must use this context board');
        validate(feed);
        const feedInput = descriptions.get(feed.id)?.input as Record<string, unknown> | undefined;
        if ('kind' in structure ? feedInput?.owner !== structure.id : !feedInput?.positive || !feedInput?.negative) S.invalid('Antenna feed does not belong to its board/scene structure');
        return descriptor('antenna', { structure: 'kind' in structure ? structure : { boardIdentity: identity }, feed, ...(v.models ? { models: v.models.map(m => S.parse(S.model, json(m))) } : {}) }) as T.Antenna;
      },
      network(v: Parameters<T.AnalysisContext['network']>[0]) {
        if (!object(v) || !object(v.model)) S.invalid("Network requires a resource model and port mapping");
        const model = v.model.kind === "network" ? networkResource(v.model) : S.parse(S.resource, json(v.model));
        const p = S.parse(z.strictObject({ model: S.resource, ports: z.record(S.text, S.text) }), json({...v, model}));
        unique(Object.values(p.ports), 'network port mapping');
        if (!p.model.ports || Object.values(p.ports).some(n => !p.model.ports!.includes(n))) S.invalid('Network mapping references undeclared resource ports');
        if (Object.keys(p.ports).length !== p.model.ports.length) S.invalid('Network mapping must cover every resource port');
        return descriptor('network', p, Object.keys(p.ports)) as T.Network;
      },
      connect(v: Parameters<T.AnalysisContext['connect']>[0]) {
        const p = S.parse(z.strictObject({ networks: z.array(S.handle.refine(h => ['network', 'connect', 'line'].includes(h.kind), 'Expected network/line handle')).min(1), connections: z.array(z.strictObject({ from: S.port, to: S.port, contacts:z.enum(['both','positive','negative']).optional() })), ports: z.record(S.text, S.port) }), json(v));
        validate(p);
        const ids = new Set(p.networks.map(n => n.id));
        for (const port of [...p.connections.flatMap(c => [c.from, c.to]), ...Object.values(p.ports)]) {
          const owner = (descriptions.get(port.id)?.input as Record<string, unknown> | undefined)?.owner;
          if (!ids.has(String(owner))) S.invalid('Connection port does not belong to a listed network');
        }
        return descriptor('connect', p, Object.keys(p.ports)) as T.Network;
      },
      variant(v: Parameters<T.AnalysisContext['variant']>[0]) {
        const p = S.parse(z.strictObject({ changes: z.array(z.discriminatedUnion('kind', [
          z.strictObject({ kind: z.literal('track-width'), target: S.selector, widthMm: z.number().positive() }),
          z.strictObject({ kind: z.literal('stackup'), stackup: S.stackupDescription }),
          z.strictObject({ kind: z.literal('material'), name: S.text, material: S.material }),
          z.strictObject({ kind: z.literal('component-model'), target: S.selector, model: S.model }),
          z.strictObject({ kind: z.literal('component-pose'), target: S.selector, xMm: z.number().finite(), yMm: z.number().finite(), rotationDeg: z.number().finite() }),
        ])).min(1) }), json(v));
        validate(p);
        const child = analysis(b ? { board: b } : undefined, state);
        const childState = contextStates.get(child)!;
        childState.changes = [...state.changes, ...p.changes.map(json)];
        childState.store.add('descriptors', { kind: 'variant', changes: childState.changes, parentContextId: id });
        return child;
      },
      check: (v: T.CheckInput) => store.check(v),
      log(v: { level: 'info' | 'warning' | 'error'; message: string; data?: T.Json }) { store.add('logs', S.parse(z.strictObject({ level: z.enum(['info', 'warning', 'error']), message: z.string(), data: z.json().optional() }), json(v))); },
      plot(v: T.PlotInput) {
        const p = S.parse(z.strictObject({ title: S.text, data: z.json(), kind: z.enum(['line', 'heatmap', 'table', 'eye', 'polar', 'bar']).optional(), layer: S.text.optional(), atS: z.number().finite().nonnegative().optional(), atHz: z.number().finite().positive().optional() }), json(v));
        if (p.atS !== undefined && p.atHz !== undefined) S.invalid('Plot selects time or frequency, not both');
        validate(p.data);
        const kinds = { scalar: 'line', series: 'line', map: 'heatmap', table: 'table', eye: 'eye', polar: 'polar', bar: 'bar' };
        const inferred = object(p.data) && typeof p.data.kind === 'string' ? kinds[p.data.kind as keyof typeof kinds] : undefined;
        if (!p.kind && !inferred) S.invalid('Untyped plot data requires an explicit kind');
        store.add('plots', { ...p, kind: p.kind ?? inferred, rendered: false, ...(object(p.data) && object(p.data.validity) && p.data.validity.status === 'unavailable' ? { message: p.data.validity.reason } : {}) });
      },
      report: () => store.snapshot(),
      sweep: (v: unknown) => sweep(v, store, limits),
      solve: (v: T.SolveOptions) => solve(v, store, limits),
      optimize: (v: T.OptimizeOptions) => optimize(v, store, limits),
    };
    contextStates.set(api, state);
    changed({ kind: 'context', report: store.snapshot() });
    return freeze(api) as T.AnalysisContext;
  }
  const contextStates = new WeakMap<object, ContextState>();
  const globalSelector = (v: unknown) => freeze(S.parse(S.selector, json(v)));
  return {
    globals: freeze({ board, profiles, models, analysis,
      pad: (designator: string, number: string | number) => { if (typeof number !== 'string' && !(typeof number === 'number' && Number.isFinite(number))) S.invalid('Invalid pad number'); return globalSelector({ kind: 'pad', designator, number: String(number) }); },
      padId: (id: string) => globalSelector({ kind: 'padId', id }), component: (designator: string) => globalSelector({ kind: 'component', designator }), net: (name: string) => globalSelector({ kind: 'net', name }),
      viaId:(id:string)=>freeze(S.parse(S.viaSelector,json({kind:'viaId',id}))),
      region: (v: Omit<Extract<T.Selector, { kind: 'region' }>, 'kind'>) => globalSelector({ ...v, kind: 'region' }),
      stackup, ic: (v: T.ComponentInput) => componentDescription('ic', v), resistor: (v: T.ComponentInput) => componentDescription('resistor', v), capacitor: (v: T.ComponentInput) => componentDescription('capacitor', v), inductor: (v: T.ComponentInput) => componentDescription('inductor', v), device: (v: T.ComponentInput) => componentDescription('device', v),
    }), snapshot,
  };
}

function unique(values: readonly string[], what: string) { if (new Set(values).size !== values.length) S.invalid(`Duplicate ${what}`); }

async function sweep(input: unknown, store: ReportStore, limits: Limits): Promise<T.SweepResult> {
  if (!object(input) || typeof input.evaluate !== 'function') S.invalid('sweep requires a JS evaluate callback');
  const { evaluate, ...description } = input;
  const param = z.strictObject({ unit: S.text.optional(), values: z.array(z.json()).min(1) });
  const p = S.parse(z.union([z.strictObject({ parameter: param.extend({ name: S.text }) }), z.strictObject({ parameters: z.record(S.text, param), mode: z.enum(['cartesian', 'zip']) })]), json(description));
  const definitions = 'parameter' in p ? [p.parameter] : Object.values(p.parameters);
  if (definitions.some(d => d.unit !== undefined && d.values.some(v => typeof v !== 'number'))) S.invalid('Only numerical sweep parameters have units; model/profile values omit unit');
  let candidates: T.Json[];
  if ('parameter' in p) candidates = p.parameter.values;
  else {
    const entries = Object.entries(p.parameters);
    if (!entries.length) S.invalid('Sweep parameters must not be empty');
    const count = p.mode === 'zip' ? entries[0][1].values.length : entries.reduce((n, [, d]) => n * d.values.length, 1);
    if (count > limits.maxEntries) throw new S.AnalysisDslError('OUTPUT_LIMIT', 'Sweep exceeds scenario attempt limit');
    if (p.mode === 'zip' && entries.some(([, d]) => d.values.length !== count)) S.invalid('Zip values must have equal lengths');
    candidates = p.mode === 'zip' ? Array.from({ length: count }, (_, i) => Object.fromEntries(entries.map(([k, d]) => [k, d.values[i]]))) : entries.reduce<T.Json[]>((all, [k, d]) => all.flatMap(previous => d.values.map(v => ({ ...(previous as Record<string, T.Json>), [k]: v }))), [{}]);
  }
  if (candidates.length > limits.maxEntries) throw new S.AnalysisDslError('OUTPUT_LIMIT', 'Sweep exceeds scenario attempt limit');
  const attempts: { parameters: T.Json; status: 'complete' | 'failed'; value?: T.Json; error?: string }[] = [];
  for (const candidate of candidates) {
    let attempt: (typeof attempts)[number];
    try { const result = await evaluate(freeze(json(candidate))); attempt = { parameters: candidate, status: 'complete', value: json(result) }; }
    catch (error) { attempt = { parameters: candidate, status: 'failed', error: error instanceof Error ? error.message : String(error) }; }
    store.add('calculations', { kind: 'sweep-attempt', ...attempt }); attempts.push(attempt);
  }
  return freeze({ attempts, series: value({ kind: 'table', unit: '1', validity: { status: attempts.some(a => a.status === 'failed') ? 'conditional' : 'valid', ...(attempts.some(a => a.status === 'failed') ? { reason: 'Some callbacks failed; see attempts' } : {}) }, diagnostics: [], data: json(attempts) }) });
}

function validateSearch(input: unknown, kind: 'solve' | 'optimize') {
  if (!object(input) || typeof input.evaluate !== 'function') S.invalid(`${kind} requires evaluate callback`);
  const { evaluate: _, ...description } = input;
  const parameter = z.strictObject({ unit: S.text.optional(), min: z.number().finite().optional(), max: z.number().finite().optional(), values: z.array(z.number().finite()).min(1).optional() }).refine(v => v.values ? v.min === undefined && v.max === undefined : v.min !== undefined && v.max !== undefined && v.min <= v.max, 'Choose interval or explicit values');
  if (kind === 'solve') S.parse(z.strictObject({ parameter: z.strictObject({ ...parameter.shape, name: S.text }).refine(v => v.values ? v.min === undefined && v.max === undefined : v.min !== undefined && v.max !== undefined && v.min <= v.max), target: z.number().finite(), tolerance: z.number().finite().nonnegative(), unit: S.text, maxEvaluations: z.number().int().positive() }), json(description));
  else S.parse(z.strictObject({ parameters: z.record(S.text, parameter), objective: z.strictObject({ metric: S.text, direction: z.enum(['minimize', 'maximize']), unit: S.text }), constraints: z.array(z.strictObject({ metric: S.text, min: z.number().finite().optional(), max: z.number().finite().optional(), unit: S.text })), maxEvaluations: z.number().int().positive(), seed: z.number().int().nonnegative() }), json(description));
}

type SearchStatus = T.SearchResult['status'];
type ScalarReading = { value: number } | { error: string };

/** One finite real scalar in the requested unit. Missing, unavailable, non-scalar and unit-mismatched values are errors, never zero. */
function readScalar(metric: unknown, unit: string, name: string): ScalarReading {
  if (metric === undefined) return { error: `${name} is missing from the evaluate result` };
  if (!object(metric) || metric.kind !== 'scalar') return { error: `${name} must be a scalar DataValue in ${unit}` };
  const validity = metric.validity;
  if (object(validity) && validity.status === 'unavailable') return { error: `${name} unavailable: ${String(validity.reason ?? 'no reason')}` };
  if (metric.unit !== unit) return { error: `${name} unit mismatch: expected ${unit}, got ${String(metric.unit)}` };
  if (typeof metric.value !== 'number' || !Number.isFinite(metric.value)) return { error: `${name} is not a finite real scalar` };
  return { value: metric.value };
}

/** Runs a user callback; a throw or rejection becomes a recorded failure instead of aborting the search. */
async function guarded<R>(run: () => R | Promise<R>): Promise<{ ok: true; value: R } | { ok: false; error: string }> {
  try { return { ok: true, value: await run() }; }
  catch (error) { return { ok: false, error: error instanceof Error ? error.message : String(error) }; }
}

/** Seeded PRNG (mulberry32). The same seed yields the same sequence; seeds are reduced modulo 2^32. */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** solve(): 1-D root finding of evaluate(x) - target.
 * Explicit values are evaluated in order (all of them within budget) and the smallest |residual| wins.
 * An interval is bracketed by its ends, then by up to 9 uniform interior points, then refined by bisection.
 * Failed evaluations (throw, unavailable, unit mismatch) are recorded in history and skipped.
 * `found` means |residual| <= tolerance at the best point; a non-found result is never a root claim. */
async function solve(input: unknown, store: ReportStore, limits: Limits): Promise<T.SearchResult> {
  validateSearch(input, 'solve');
  const { evaluate, ...description } = input as T.SolveOptions;
  const p = json(description) as unknown as Omit<T.SolveOptions, 'evaluate'>;
  if (p.maxEvaluations > limits.maxEntries) throw new S.AnalysisDslError('OUTPUT_LIMIT', 'Search exceeds scenario attempt limit');
  const { name, values, min, max } = p.parameter;
  const st = { attempts: 0, successes: 0, best: undefined as { x: number; metric: number; residual: number } | undefined };
  const history: T.Json[] = [];
  const samples: { x: number; residual: number }[] = [];
  const remaining = () => p.maxEvaluations - st.attempts;
  const within = () => st.best !== undefined && Math.abs(st.best.residual) <= p.tolerance;
  async function trial(x: number): Promise<number | undefined> {
    st.attempts++;
    const run = await guarded(() => evaluate(x));
    let record: Record<string, T.Json>;
    let residual: number | undefined;
    const reading = run.ok ? readScalar(run.value, p.unit, 'metric') : { error: run.error };
    if ('value' in reading) {
      residual = reading.value - p.target; st.successes++;
      record = { parameter: name, value: x, status: 'complete', metric: reading.value, residual };
      if (!st.best || Math.abs(residual) < Math.abs(st.best.residual)) st.best = { x, metric: reading.value, residual };
      samples.push({ x, residual });
    } else record = { parameter: name, value: x, status: 'failed', error: reading.error };
    history.push(record); store.add('calculations', { kind: 'solve-attempt', ...record });
    return residual;
  }
  let status: SearchStatus;
  let note: string | undefined;
  if (values) {
    let scanned = 0;
    for (const x of values) { if (remaining() <= 0) break; await trial(x); scanned++; }
    status = within() ? 'found' : st.successes === 0 ? 'failed' : scanned < values.length ? 'budget-exhausted' : 'not-found';
  } else {
    if (min === undefined || max === undefined) S.invalid('solve interval requires min and max');
    const lo = min, hi = max;
    type Bracket = { a: number; ra: number; b: number };
    let bracket: Bracket | undefined;
    const rLo = await trial(lo);
    const rHi = hi !== lo && !within() && remaining() > 0 ? await trial(hi) : undefined;
    if (rLo !== undefined && rHi !== undefined && Math.sign(rLo) !== Math.sign(rHi)) bracket = { a: lo, ra: rLo, b: hi };
    let grid = 0;
    if (hi !== lo && !within() && !bracket) {
      grid = Math.min(9, remaining());
      for (let k = 1; k <= grid && !within(); k++) await trial(lo + (hi - lo) * k / (grid + 1));
      const sorted = [...samples].sort((s, t) => s.x - t.x);
      for (let i = 0; i + 1 < sorted.length && !bracket; i++) {
        const s = sorted[i]!, t = sorted[i + 1]!;
        if (Math.sign(s.residual) !== Math.sign(t.residual)) bracket = { a: s.x, ra: s.residual, b: t.x };
      }
    }
    if (bracket && !within()) {
      // Bisection with fraction fallbacks: a failed point is retried at another interior fraction.
      const fractions = [0.5, 0.4, 0.6, 0.3, 0.7];
      let { a, ra, b } = bracket, k = 0;
      while (!within() && remaining() > 0) {
        if (Math.abs(b - a) <= 1e-12 * Math.max(1, Math.abs(a), Math.abs(b))) break;
        const x = a + (b - a) * fractions[k % fractions.length]!;
        const r = await trial(x);
        if (r === undefined) { k++; continue; }
        k = 0;
        if (Math.sign(r) === Math.sign(ra)) { a = x; ra = r; } else b = x;
      }
    }
    if (within()) status = 'found';
    else if (st.successes === 0) status = 'failed';
    else if (bracket) status = remaining() > 0 ? 'not-found' : 'budget-exhausted';
    else if (hi !== lo && grid === 0) status = 'budget-exhausted';
    else status = 'not-found';
    if (status === 'not-found') note = bracket
      ? 'Bracket collapsed onto a sign change that is not a root (discontinuity); a root is not guaranteed.'
      : 'No sign change at the interval ends or on the grid; a root is not guaranteed.';
  }
  if (status === 'not-found' && note === undefined) note = 'No evaluated value reaches the tolerance; a root is not guaranteed.';
  return freeze({
    status,
    ...(st.best ? { best: { parameter: name, value: st.best.x, metric: st.best.metric, residual: st.best.residual } } : {}),
    history, violations: [], ...(note ? { note } : {}),
  }) as T.SearchResult;
}

/** optimize(): bounded, derivative-free, deterministic for a given seed.
 * Phase 1 draws about 30% of the budget as seeded Latin-hypercube samples (continuous) and random levels (discrete).
 * Phase 2 is a greedy coordinate/pattern search from the best point: lexicographic (total constraint violation, objective),
 * continuous steps start at a quarter of the range and halve when a full pass does not improve, discrete moves go to
 * neighbouring listed values. Phase 3 spends any remaining budget on seeded random samples.
 * This is a local/heuristic search. `found` means the best feasible point seen within the budget; it is NOT a
 * global-optimum claim. Constraints are checked against each evaluate result; infeasible points go to `violations`. */
async function optimize(input: unknown, store: ReportStore, limits: Limits): Promise<T.SearchResult> {
  validateSearch(input, 'optimize');
  const { evaluate, ...description } = input as T.OptimizeOptions;
  const p = json(description) as unknown as Omit<T.OptimizeOptions, 'evaluate'>;
  if (p.maxEvaluations > limits.maxEntries) throw new S.AnalysisDslError('OUTPUT_LIMIT', 'Search exceeds scenario attempt limit');
  const entries = Object.entries(p.parameters).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  if (!entries.length) S.invalid('optimize requires at least one parameter');
  type Dim = { name: string; kind: 'continuous'; min: number; max: number } | { name: string; kind: 'discrete'; levels: number[] };
  const dims: Dim[] = entries.map(([name, d]) => d.values
    ? { name, kind: 'discrete', levels: [...new Set(d.values)].sort((a, b) => a - b) }
    : { name, kind: 'continuous', min: d.min ?? 0, max: d.max ?? 0 });
  type Point = Record<string, number>;
  type Scored = { point: Point; signed: number; objective: number; violation: number };
  const rng = mulberry32(p.seed);
  const st = { attempts: 0 };
  const history: T.Json[] = [], violations: T.Json[] = [];
  const scored: Scored[] = [];
  const seen = new Set<string>();
  const keyOf = (pt: Point) => dims.map(d => String(pt[d.name])).join('|');
  const better = (a: Scored, b: Scored) => a.violation < b.violation || (a.violation === b.violation && a.signed < b.signed);
  const minimize = p.objective.direction === 'minimize';

  async function trial(point: Point): Promise<void> {
    seen.add(keyOf(point)); st.attempts++;
    const run = await guarded(() => evaluate(freeze({ ...point })));
    const read = (metric: string, unit: string): number | string => {
      const bag: unknown = run.ok ? run.value : undefined;
      const r = readScalar(object(bag) ? bag[metric] : undefined, unit, `metric ${metric}`);
      return 'value' in r ? r.value : r.error;
    };
    let record: Record<string, T.Json>;
    if (!run.ok) record = { parameters: { ...point }, status: 'failed', error: run.error };
    else {
      const objective = read(p.objective.metric, p.objective.unit);
      const constraintValues = p.constraints.map(c => read(c.metric, c.unit));
      const error = [objective, ...constraintValues].find((v): v is string => typeof v === 'string');
      if (error !== undefined) record = { parameters: { ...point }, status: 'failed', error };
      else {
        const raw = objective as number;
        let violation = 0;
        const violated: T.Json[] = [];
        p.constraints.forEach((c, i) => {
          const v = constraintValues[i] as number;
          const below = c.min !== undefined && v < c.min, above = c.max !== undefined && v > c.max;
          if (!below && !above) return;
          if (below) violation += c.min! - v;
          if (above) violation += v - c.max!;
          violated.push({ metric: c.metric, value: v, ...(c.min !== undefined ? { min: c.min } : {}), ...(c.max !== undefined ? { max: c.max } : {}) });
        });
        const signed = minimize ? raw : -raw;
        scored.push({ point: { ...point }, signed, objective: raw, violation });
        record = { parameters: { ...point }, status: 'complete', objective: raw, feasible: violation === 0 };
        if (violated.length) { record.violated = violated; violations.push({ parameters: { ...point }, violated }); }
      }
    }
    history.push(record); store.add('calculations', { kind: 'optimize-attempt', ...record });
  }
  const budgetLeft = () => st.attempts < p.maxEvaluations;

  // Phase 1: seeded Latin-hypercube / random-level samples, about 30% of the budget.
  const initialCount = Math.max(1, Math.round(0.3 * p.maxEvaluations));
  const permutation = (n: number) => {
    const a = Array.from({ length: n }, (_, i) => i);
    for (let i = n - 1; i > 0; i--) { const j = Math.floor(rng() * (i + 1)); [a[i], a[j]] = [a[j]!, a[i]!]; }
    return a;
  };
  const strata = dims.map(d => d.kind === 'continuous' ? permutation(initialCount) : []);
  for (let i = 0; i < initialCount && budgetLeft(); i++) {
    const point: Point = {};
    dims.forEach((d, j) => {
      if (d.kind === 'discrete') point[d.name] = d.levels[Math.floor(rng() * d.levels.length)]!;
      else point[d.name] = d.min + (((strata[j]![i]! + rng()) / initialCount) * (d.max - d.min));
    });
    if (!seen.has(keyOf(point))) await trial(point);
  }

  // Phase 2: greedy coordinate/pattern search from the best point (violation first, then objective).
  const steps = dims.map(d => d.kind === 'continuous' ? (d.max - d.min) / 4 : 0);
  const minSteps = dims.map(d => d.kind === 'continuous' ? (d.max - d.min) * 1e-6 : 0);
  const moved = (from: Point, j: number, dir: 1 | -1): Point | undefined => {
    const d = dims[j]!, next: Point = { ...from };
    if (d.kind === 'discrete') {
      const i = d.levels.indexOf(from[d.name]!) + dir;
      if (i < 0 || i >= d.levels.length) return undefined;
      next[d.name] = d.levels[i]!; return next;
    }
    if (steps[j]! <= 0) return undefined;
    const x = Math.min(d.max, Math.max(d.min, from[d.name]! + dir * steps[j]!));
    if (x === from[d.name]) return undefined;
    next[d.name] = x; return next;
  };
  const bestScored = (list: Scored[]) => list.reduce<Scored | undefined>((b, s) => (!b || better(s, b) ? s : b), undefined);
  let current = bestScored(scored);
  while (current && budgetLeft()) {
    let improved = false;
    for (let j = 0; j < dims.length && budgetLeft(); j++) {
      for (const dir of [1, -1] as const) {
        if (!budgetLeft()) break;
        const candidate = moved(current.point, j, dir);
        if (!candidate || seen.has(keyOf(candidate))) continue;
        const before = scored.length;
        await trial(candidate);
        const s = scored[before];
        if (s && better(s, current)) { current = s; improved = true; break; }
      }
    }
    if (!improved) {
      if (!dims.some((d, j) => d.kind === 'continuous' && steps[j]! > minSteps[j]!)) break;
      dims.forEach((d, j) => { if (d.kind === 'continuous') steps[j] = steps[j]! / 2; });
    }
  }

  // Phase 3: any remaining budget goes to seeded random samples (bounded attempts for a finite discrete space).
  for (let tries = 0; budgetLeft() && tries < 50 * p.maxEvaluations; tries++) {
    const point: Point = {};
    for (const d of dims) point[d.name] = d.kind === 'discrete' ? d.levels[Math.floor(rng() * d.levels.length)]! : d.min + rng() * (d.max - d.min);
    if (!seen.has(keyOf(point))) await trial(point);
  }

  const bestFeasible = scored.filter(s => s.violation === 0).reduce<Scored | undefined>((b, s) => (!b || s.signed < b.signed ? s : b), undefined);
  const status: SearchStatus = scored.length === 0 && st.attempts > 0 ? 'failed' : bestFeasible ? 'found' : 'not-found';
  const note = status === 'found'
    ? 'Best feasible point within the evaluation budget; a bounded local search, not a global-optimum claim.'
    : status === 'not-found' ? 'No feasible point found within the evaluation budget; infeasibility is not proven.' : undefined;
  return freeze({
    status,
    ...(bestFeasible ? { best: { parameters: bestFeasible.point, objective: bestFeasible.objective, objectiveMetric: p.objective.metric, unit: p.objective.unit, feasible: true } } : {}),
    history, violations, ...(note ? { note } : {}),
  }) as T.SearchResult;
}
