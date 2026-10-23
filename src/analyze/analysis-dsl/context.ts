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
          const p = preparation();
          if (!p.structurallyValid) throw new S.AnalysisDslError('INVALID_CONFIGURATION', 'Analysis configuration is incomplete or inconsistent', p.diagnostics);
          const profile = kind === 'dc' ? conductionProfile(config.stackup as T.StackupDescription | undefined, config.materials as T.Materials | undefined, config.temperatureC as number | undefined) : undefined;
          const task: AnalysisTask<K> = freeze({ kind, contextId: id, ...(b ? { board: b, boardIdentity: identity } : {}), config: json(config) as T.Configurations[K], descriptors: [...descriptions.values()].map(json), variantChanges: json(state.changes) as T.Json[], resolvedLoads: p.components, ...(profile ? { conductionProfile: profile } : {}) });
          try {
            const payload = await executor(task);
            const result = resultFor(task, payload, validate, model => api.network({ model, ports: Object.fromEntries((model.ports ?? []).map(p => [p, p])) }), id => descriptions.get(id));
            store.add('calculations', { kind, phase: 'completed', result: json(result) });
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
        const parsed = S.parse(z.strictObject({ positive: z.union([pad, z.array(pad).min(1)]), negative: z.union([pad, z.array(pad).min(1)]) }), json(v));
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
        const p = S.parse(z.strictObject({ networks: z.array(S.handle.refine(h => ['network', 'connect', 'line'].includes(h.kind), 'Expected network/line handle')).min(1), connections: z.array(z.strictObject({ from: S.port, to: S.port })), ports: z.record(S.text, S.port) }), json(v));
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
      async solve(v: T.SolveOptions) { validateSearch(v, 'solve'); throw new S.AnalysisDslError('ANALYZER_NOT_IMPLEMENTED', 'solve search algorithm is not implemented'); },
      async optimize(v: T.OptimizeOptions) { validateSearch(v, 'optimize'); throw new S.AnalysisDslError('ANALYZER_NOT_IMPLEMENTED', 'optimize search algorithm is not implemented'); },
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
