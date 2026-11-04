import type * as T from './spec-doc.js';
import type { ContextState } from './context.ts';
import { AnalysisDslError, schemas } from './schema.ts';
import { freeze, json } from './data.ts';
import { object, sampleWaveform } from './descriptions.ts';

export function prepare(state: ContextState, kind: T.AnalysisKind, config: Record<string, unknown>): T.Preparation {
  const diagnostics: T.Diagnostic[] = [];
  const components: T.Json[] = [];
  const assumptions: string[] = [];
  const issue = (code: string, message: string, path?: string, severity: T.Diagnostic['severity'] = 'error') => diagnostics.push({ code, message, severity, ...(path ? { path } : {}) });
  const required = (keys: string[], c = config, path = '') => keys.forEach(k => { if (c[k] === undefined) issue('MISSING_FIELD', `Required section '${path}${k}' is missing`, path + k); });
  const exclusive = (keys: string[], c = config, exactly = false) => {
    const set = keys.filter(k => c[k] !== undefined);
    if (set.length > 1 || exactly && set.length !== 1) issue('CONFLICTING_MODES', `Choose ${exactly ? 'exactly' : 'at most'} one of ${keys.join(', ')}`);
  };
  const parsed = schemas[kind].safeParse(config);
  if (!parsed.success) parsed.error.issues.forEach(e => issue('INVALID_CONFIGURATION', e.message, e.path.join('.')));
  try { state.validate(config); } catch (e) { issue('INVALID_REFERENCE', e instanceof Error ? e.message : String(e)); }
  exclusive(['frequency', 'frequencyHz']);
  exclusive(['time', 'frequency']); exclusive(['time', 'frequencyHz']);
  const frequencyRequired = () => { if (config.frequency === undefined && config.frequencyHz === undefined) issue('MISSING_FIELD', 'Explicit frequency or frequencyHz is required'); };
  const boardOr = (other: string[]) => {
    const modes = other.filter(k => config[k] !== undefined);
    if (modes.length > 1 || !modes.length && !state.board) issue('MISSING_MODEL', `Need board or exactly one ${other.join('/')}`);
    const boardOnly=config.circuit?['components','stackup','materials','heat','losses','cooling']:['components','sources','loads','stackup','materials','heat','losses','cooling'];
    if (modes.length && boardOnly.some(k => config[k] !== undefined)) issue('CONFLICTING_MODES', 'Standalone model cannot be combined with board-mode sections');
    return modes.length === 0;
  };
  if (kind === 'geometry') {
    if (!state.board) issue('BOARD_REQUIRED', 'geometry requires an explicit board');
    required(['rules']);
    if (Array.isArray(config.rules) && !config.rules.length) issue('MISSING_FIELD', 'geometry requires at least one explicit rule');
    if (Array.isArray(config.scope) && !config.scope.length) issue('INVALID_SELECTOR', 'Explicit geometry scope cannot be empty');
    for (const rule of (config.rules ?? []) as T.Rule[]) if (!['connectivity', 'clearance', 'edge-clearance'].includes(rule.kind)) issue('GEOMETRY_UNSUPPORTED_RULE', `Geometry rule ${rule.kind} needs unavailable channel topology`, undefined, 'warning');
  }
  if (kind === 'dc') {
    const boardMode = boardOr(['conductor', 'circuit']);
    if (boardMode) required(['sources', 'loads']);
    if (config.conductor) exclusive(['currentA', 'voltageV'], config, true);
    else if (config.currentA !== undefined || config.voltageV !== undefined) issue('CONFLICTING_MODES', 'currentA/voltageV setters are conductor-only');
  }
  if (kind === 'pi') {
    if (boardOr(['circuit'])) required(['sources', 'loads']);
    required(['domain']);
    if (config.domain === 'time') { required(['time', 'initial']); if (config.frequency !== undefined || config.frequencyHz !== undefined) issue('CONFLICTING_MODES', 'Time PI cannot use frequency'); }
    if (config.domain === 'frequency') { frequencyRequired(); if (config.time !== undefined || config.initial !== undefined) issue('CONFLICTING_MODES', 'Frequency PI cannot use time/initial'); }
  }
  const checkThermal = (c: Record<string, unknown>, path = '', standalone?: boolean) => {
    required(['environment'], c, path);
    exclusive(['initialC', 'initial'], c, true);
    if (c.domain === 'steady') { if (c.time !== undefined) issue('CONFLICTING_MODES', 'Steady thermal cannot use time'); }
    else if (path === '') required(['time'], c, path);
    if (!standalone && c.heat === undefined && c.losses === undefined) issue('MISSING_FIELD', `Need ${path}heat/losses or explicit heat:[]`);
    if (!standalone && c.cooling === undefined) issue('MISSING_FIELD', `Need ${path}cooling`);
    const env = c.environment as T.Environment | undefined;
    if (c.domain === 'steady' && env && typeof env.ambientC !== 'number' && c.atS === undefined) issue('MISSING_FIELD', 'Dynamic steady environment requires explicit atS');
    if (path && (c.time !== undefined || c.domain !== undefined)) issue('CONFLICTING_MODES', 'Electrothermal has one common time; no nested domain/time');
  };
  if (kind === 'thermal') { const boardMode = boardOr(['network']); checkThermal(config, '', !boardMode); }
  if (kind === 'electrothermal') {
    required(['electrical', 'thermal', 'couplings', 'time']);
    const e = config.electrical, t = config.thermal;
    if (object(e) && object(t)) {
      const standalone = e.circuit !== undefined || t.network !== undefined;
      if (standalone && (!e.circuit || !t.network)) issue('CONFLICTING_MODES', 'Boardless electrothermal needs circuit and thermal network together');
      if (!standalone && !state.board) issue('BOARD_REQUIRED', 'Electrothermal board mode requires board');
      if (!standalone) required(['sources', 'loads'], e, 'electrical.');
      if (e.circuit && ['sources', 'loads', 'components'].some(k => e[k] !== undefined)) issue('CONFLICTING_MODES', 'electrical.circuit cannot use board sources/loads/components');
      if (t.network && ['heat', 'losses', 'cooling', 'components'].some(k => t[k] !== undefined)) issue('CONFLICTING_MODES', 'thermal.network cannot use board heat/cooling/components');
      checkThermal(t, 'thermal.', standalone);
      if (e.stackup || e.materials || t.stackup || t.materials) issue('CONFLICTING_MODES', 'Electrothermal stackup/materials belong to the common builder');
      const circuit = descriptionInput(state, e.circuit), thermal = descriptionInput(state, t.network);
      for (const coupling of (config.couplings ?? []) as T.Coupling[]) {
        if ('element' in coupling && circuit && !(circuit.elements as { id: string }[]).some(e => e.id === coupling.element)) issue('INVALID_REFERENCE', `Unknown coupled circuit element ${coupling.element}`);
        if (thermal && !(thermal.nodes as { id: string }[]).some(n => n.id === coupling.thermalNode)) issue('INVALID_REFERENCE', `Unknown coupled thermal node ${coupling.thermalNode}`);
      }
    }
  }
  if (['impedance', 'tdr', 'em'].includes(kind)) {
    required(['target']);
    const target = config.target;
    if (object(target) && 'contextId' in target) {
      const allowed = kind === 'impedance' ? ['line', 'channel', 'differential'] : kind === 'tdr' ? ['line', 'channel', 'differential', 'network', 'connect'] : ['line', 'channel', 'differential', 'scene', 'antenna', 'board'];
      if (!allowed.includes(String(target.kind))) issue('INVALID_TARGET', `${kind} does not accept ${target.kind}`);
    }
    if (object(target) && target.kind === 'line' && kind !== 'impedance') {
      if (!descriptionInput(state, target)?.lengthMm) issue('MISSING_FIELD', 'Line lengthMm is required for TDR/EM');
    }
  }
  if (kind === 'impedance') { required(['method']); frequencyRequired();
    if(config.frequencyHz===0&&(config.method!=='layered'||!object(config.target)||!['channel','differential'].includes(String(config.target.kind))))
      issue('LOCAL_DC_TARGET','Exact DC impedance requires method layered and a physical board channel or differential target');
    if(config.method==='cross-section'){
      const input=descriptionInput(state,config.target);
      if(input){
        const loss=input.conductors!==undefined||['material','topMaterial','bottomMaterial'].some(k=>object(input[k])&&Number(input[k].lossTangent)>0);
        assumptions.push(input.conductors===undefined?'Line signal/reference surfaces explicitly retain PEC':'Line conductor conductivity and reference thickness are explicit fixed-temperature hypotheses');
        if(loss)issue('SECTION_LOSS_APPLICABILITY_UNVERIFIED','Valid loss description still requires native skin-depth/weak-loss/quasi-TEM screening, unit-current normalization and separate surface/region mesh/domain convergence',undefined,'warning');
      }
    }
  }
  if (kind === 'tdr') required(['referenceOhm', 'riseS', 'termination', 'time']);
  if (kind === 'si') {
    exclusive(['channels', 'network'], config, true); required(config.network && config.initial!==undefined?['time']:['drivers', 'receivers', 'time']);
    const targets = config.channels as T.Handle[] | undefined ?? (config.network ? [config.network as T.Handle] : []);
    for (const t of targets) if (t.kind === 'line' && !descriptionInput(state, t)?.lengthMm) issue('MISSING_FIELD', 'Line lengthMm is required for SI');
    for (const [i, binding] of [...((config.drivers ?? []) as T.Driver[]), ...((config.receivers ?? []) as T.Receiver[])].entries()) {
      if ('id' in binding.model && binding.model.ports) {
        const ports = binding.model.ports;
        const names = binding.ports ? Object.keys(binding.ports) : binding.port ? ports.length === 1 ? ports : [] : [];
        if (names.length !== ports.length || names.some(n => !ports.includes(n))) issue('INVALID_BINDING', 'port/ports must bind every named resource port exactly once', `bindings.${i}`);
      }
    }
  }
  if (kind === 'eye') { required(['signal', 'bitRateBps', 'discardS', 'phaseS', 'thresholdV']); const s = config.signal as T.DataDescription | undefined; if (s && (s.kind !== 'series' || s.axis?.name !== 'timeS' || s.unit !== 'V')) issue('INVALID_SIGNAL', 'Eye requires a time voltage series'); }
  if (kind === 'emi') {
    required(['kind']);
    if (config.kind === 'risk') { if (!state.board) issue('BOARD_REQUIRED', 'EMI risk requires board'); required(['signals', 'rules']); if (config.excitations || config.time || config.frequency || config.measurement) issue('CONFLICTING_MODES', 'Risk rules do not accept solver excitation/measurement modes'); }
    else if (config.kind) {
      if (!config.circuit && !state.board) issue('MISSING_MODEL', 'EMI needs board or circuit');
      if (config.circuit && config.kind === 'coupling') issue('CONFLICTING_MODES', 'Board coupling requires board; circuit is conducted mode');
      required(['excitations', 'measurement']);
      if (!config.time) frequencyRequired();
      if (config.spectrum && !config.time) issue('MISSING_FIELD', 'spectrum requires time');
      if (config.spectrum && config.time) {
        const s = config.spectrum as { discardS: number; startHz: number; stopHz: number }, t = config.time as T.Time;
        if (s.discardS >= t.stopS || s.stopHz < s.startHz || s.stopHz > 1 / (2 * t.sampleStepS)) issue('INVALID_SPECTRUM', 'Spectrum discard/range exceeds observation or Nyquist frequency');
      }
    }
  }
  if (kind === 'em') {
    required(['method', 'space', 'outputs']); frequencyRequired(); exclusive(['excitation', 'excitations']);
    if ((config.outputs as string[] | undefined)?.includes('radiation')) required(['observation']);
    if ((config.outputs as string[] | undefined)?.includes('radiation') && !config.excitation && !config.excitations) issue('MISSING_FIELD', 'Radiation needs explicit excitation(s)');
    if ((config.outputs as string[] | undefined)?.includes('fields') && !(config.outputs as string[]).includes('sParameters') && !config.excitation && !config.excitations) issue('MISSING_FIELD', 'Fields require explicit excitation or S-parameter normalized port excitations');
  }
  if (kind === 'networkParameters') { required(['network', 'representation']); frequencyRequired(); if (config.representation === 'S') required(['referenceOhm']); else if(config.referenceOhm!==undefined) issue('INVALID_REFERENCE','Z/Y output cannot specify referenceOhm'); }
  if (object(config.referenceOhm)) {
    const handle = config.network ?? config.target;
    const d = object(handle) ? state.descriptions.get(String(handle.id)) : undefined;
    const ports = d?.ports as string[] | undefined;
    if (ports && (Object.keys(config.referenceOhm).length !== ports.length || Object.keys(config.referenceOhm).some(k => !ports.includes(k)))) issue('INVALID_REFERENCE', 'referenceOhm map must match every named target/network port');
  }

  const validatePhysical = (c: Record<string, unknown>, path = '') => {
    const entries = (c.components ?? []) as T.ComponentDescription[];
    if (new Set(entries.map(e => e.designator)).size !== entries.length) issue('DUPLICATE_COMPONENT', 'components repeats designator', path + 'components');
    for (const component of entries) {
      const { designator } = component;
      if (!state.board || state.board.components.filter(e => e.designator === designator).length !== 1) { issue('INVALID_REFERENCE', `Unknown/ambiguous component ${designator}`, path + 'components'); continue; }
      const pins = [...(component.ground ?? []), ...Object.values(component.supplies ?? {}).flat(), ...Object.values(component.pins ?? {}).flat()];
      for (const number of pins) if (state.board.pads.filter(p => p.component === designator && p.number === number).length !== 1) issue('INVALID_REFERENCE', `Unknown/ambiguous pin ${designator}.${number}`);
      components.push(json({ designator, source: 'manual', fields: component, physicalRequirements: 'unverified' }));
    }
    const stack = c.stackup as T.StackupDescription | undefined;
    if (stack && state.board) {
      const expected = [...state.board.layers].sort((a, b) => a.index - b.index).map(l => l.name);
      const actual = stack.routing.layers.filter(l => l.kind === 'copper').map(l => l.layer);
      if (JSON.stringify(expected) !== JSON.stringify(actual)) issue('INVALID_STACKUP', 'Copper layer names and physical order must match board');
      if (state.board.stackup) assumptions.push('Explicit stackup replaces the CAD physical stackup; original board is retained');
    }
    for (const layer of stack?.input.layers ?? []) if (layer.material && !(layer.material in (c.materials as T.Materials ?? {}))) issue('INVALID_REFERENCE', `Missing material ${layer.material}`);
    const loads = c.loads;
    if (kind === 'dc' && object(loads) && state.board) {
      const defaults = loads as unknown as T.LoadDefaults;
      const eligible = entries.filter(e => ['ic','device'].includes(e.kind) && state.board!.pads.some(p => p.component === e.designator && p.net === defaults.rail.name));
      for (const designator of Object.keys(defaults.overrides)) if (!eligible.some(e => e.designator === designator)) issue('INVALID_LOAD', `Override ${designator} is not an explicitly classified IC/device on this rail`);
      for (const entry of eligible) try { components.push(resolveLoad(state, { designator: entry.designator, rail: defaults.rail, currentA: defaults.overrides[entry.designator]?.currentA ?? defaults.defaultCurrentA, ...(defaults.return ? { return: defaults.return } : {}) }, entries, assumptions, diagnostics)); } catch(e) { issue('INVALID_LOAD', e instanceof Error ? e.message : String(e)); }
      assumptions.push(`DC defaults selected explicit IC/device descriptors: ${eligible.map(e=>e.designator).join(', ') || 'none'}; other parts excluded, no footprint/BOM inference`);
      if (!eligible.length) issue('INVALID_LOAD', 'DC defaults found no explicitly classified consumers');
    }
    if (Array.isArray(loads)) for (const [i, load] of loads.entries()) {
      if (object(load) && 'designator' in load) {
        try { components.push(resolveLoad(state, load as unknown as T.ComponentLoad, entries, assumptions, diagnostics)); }
        catch (e) { issue('INVALID_LOAD', e instanceof Error ? e.message : String(e), `${path}loads.${i}`); }
      } else if (object(load) && object(load.waveform)) {
        const operating = load.currentA ?? load.powerW ?? load.resistanceOhm;
        if (typeof operating === 'number') {
          const start = sampleWaveform(load.waveform as unknown as T.Waveform, 0);
          if (Math.abs(start - operating) > 16 * Number.EPSILON * Math.max(1, Math.abs(start), Math.abs(operating))) issue('INVALID_OPERATING_POINT', 'Load waveform at zero differs from operating point', `${path}loads.${i}`);
        }
      }
    }
    else if (loads !== undefined) issue('AUTODETECTION_UNVERIFIED', 'Default load groups require future supported component recognition; no consumers guessed', path + 'loads', 'warning');
    for (const [i, source] of ((c.sources ?? []) as T.Source[]).entries()) {
      if (source.waveform) {
        const start = sampleWaveform(source.waveform, 0), operating = source.voltageV ?? source.currentA;
        if (operating === undefined || Math.abs(start - operating) > 16 * Number.EPSILON * Math.max(1, Math.abs(start), Math.abs(operating))) issue('INVALID_OPERATING_POINT', 'Source waveform at zero differs from operating point', `${path}sources.${i}`);
      }
    }
  };
  validatePhysical(config);
  if (object(config.electrical)) validatePhysical(config.electrical, 'electrical.');
  if (object(config.thermal)) validatePhysical(config.thermal, 'thermal.');
  function scan(v: unknown, key = ''): void {
    if (Array.isArray(v)) { v.forEach(x => scan(x, key)); return; }
    if (!object(v)) return;
    if ('id' in v && Array.isArray(v.analyses) && !v.analyses.includes(kind)) issue('INCOMPATIBLE_MODEL', `Resource ${v.id} does not declare ${kind} support`);
    if (['signal', 'losses'].includes(key) && object(v.validity) && v.validity.status === 'unavailable') issue('UNAVAILABLE_INPUT', `${key} has no usable result data`);
    if (key === 'voltageSpectrum' && v.unit !== 'V' || key === 'currentSpectrum' && v.unit !== 'A') issue('INVALID_UNIT', `${key} has the wrong unit`);
    for (const [k, x] of Object.entries(v)) scan(x, k);
  }
  scan(config);
  if (state.changes.length) issue('VARIANT_REBUILD_REQUIRED', 'Variant intent is retained; no stale copper solve or implicit refill is permitted. Geometry/material/model rebuilding is not implemented', undefined, 'warning');
  issue('PHYSICAL_REQUIREMENTS_UNVERIFIED', 'Structure validation does not establish physical readiness; extraction/model applicability is not evaluated', undefined, 'warning');
  const boardGroup = kind === 'si' && !!state.board && Array.isArray(config.channels) && config.channels.length > 0 &&
    config.channels.every(channel => object(channel) && (channel.kind === 'channel' || channel.kind === 'differential'));
  const available=boardGroup||kind==='impedance'&&config.method==='layered'&&object(config.target)&&config.target.kind==='channel'||kind==='geometry'||kind==='dc'||kind==='pi'&&!!config.circuit||kind==='networkParameters'||kind==='si'&&object(config.network)&&['network','connect'].includes(String(config.network.kind))||kind==='impedance'&&config.method==='cross-section'&&object(config.target)&&config.target.kind==='line';
  if (boardGroup) assumptions.push('Joint P7 extraction uses authored sources and time for input spectral demand. Board waveforms and timing remain unavailable.');
  else if(kind==='si'&&available)assumptions.push('Explicit supplied passive temporal model only: supported relaxation/authored resonant Y/full RLC or real-K delay class; not board channels, arbitrary sampled RF, eye or timing. Numerical/model applicability is checked by run().');
  if (!available) issue('ANALYZER_NOT_IMPLEMENTED', `No production ${kind} executor for this model`, undefined, 'warning');
  return freeze({ kind, contextId: state.id, structurallyValid: !diagnostics.some(d => d.severity === 'error'), physicalReady: false, executorAvailable: available, diagnostics, components, inputs: Object.fromEntries(Object.entries(config).map(([key, v]) => [key, { source: 'manual' as const, value: json(v) }])), assumptions });
}

function descriptionInput(state: ContextState, handle: unknown): Record<string, unknown> | undefined {
  return object(handle) ? state.descriptions.get(String(handle.id))?.input as Record<string, unknown> | undefined : undefined;
}

function resolveLoad(state: ContextState, load: T.ComponentLoad, components: readonly T.ComponentDescription[], assumptions: string[], diagnostics: T.Diagnostic[]): T.Json {
  const board = state.board;
  if (!board) throw new AnalysisDslError('INVALID_LOAD', 'Component load needs board');
  if (load.waveform && Math.abs(sampleWaveform(load.waveform, 0) - load.currentA) > 16 * Number.EPSILON * Math.max(1, load.currentA)) throw new Error('Component load waveform at zero differs from operating point');
  if (board.components.filter(c => c.designator === load.designator).length !== 1) throw new Error(`Unknown/ambiguous component ${load.designator}`);
  const description = components.find(c => c.designator === load.designator);
  const domains = Object.entries(description?.supplies ?? {}).filter(([, pins]) => pins.some(n => board.pads.some(p => p.component === load.designator && p.number === n && p.net === load.rail.name)));
  if (domains.length > 1 && !load.domain) throw new Error('Multiple supply domains on this rail require explicit domain');
  if (load.domain && !Object.hasOwn(description?.supplies ?? {}, load.domain)) throw new Error('Explicit domain is not in supplies');
  if (description?.supplies && !load.pins && !domains.length) throw new Error('Manual supplies do not include this rail');
  const supplyPins = load.domain ? description?.supplies?.[load.domain] : domains.length === 1 ? domains[0][1] : undefined;
  const returnNet = load.return?.name ?? 'GND';
  if (board.nets.filter(n => n.name === returnNet).length !== 1) throw new Error(`Unknown/ambiguous exact return net ${returnNet}`);
  const distributions = ([['supply', load.rail.name, load.pins ?? supplyPins, load.pinCurrentsA], ['return', returnNet, load.returnPins ?? description?.ground, load.returnPinCurrentsA]] as const).map(([side, net, manual, overrides]) => {
    const candidates = board.pads.filter(p => p.component === load.designator && p.net === net);
    let selected = candidates;
    if (manual) {
      if (new Set(manual).size !== manual.length) throw new Error('Duplicate pin in explicit load group');
      selected = manual.map(number => {
        const pads = board.pads.filter(p => p.component === load.designator && p.number === number);
        if (pads.length !== 1 || pads[0].net !== net) throw new Error(`Unknown/ambiguous pin or wrong net ${load.designator}.${number}`);
        return pads[0];
      });
      if (candidates.some(p => !selected.includes(p))) diagnostics.push({ code: 'MANUAL_PIN_GROUP', severity: 'warning', message: `${load.designator}: manual ${side} list excludes other pads on ${net}; not extended` });
    }
    if (!selected.length) throw new Error(`No ${side} pads on exact net ${net}`);
    if (new Set(selected.map(p => p.number)).size !== selected.length) throw new Error('Ambiguous physical pads with duplicate numbers; explicit padId load form is not yet specified');
    for (const key of Object.keys(overrides ?? {})) if (!selected.some(p => p.number === key)) throw new Error(`Current override outside selected group: ${key}`);
    if (Object.values(overrides ?? {}).some(v => typeof v !== 'number') || load.waveform) {
      diagnostics.push({ code: 'DYNAMIC_DISTRIBUTION_UNVERIFIED', severity: 'warning', message: 'Dynamic pin-current consistency requires future waveform validation; no assigned currents fabricated' });
      return { side, net, pins: selected.map(p => p.id), unverified: true };
    }
    const explicit = overrides as Record<string, number> | undefined;
    const sum = Object.values(explicit ?? {}).reduce((a, b) => a + b, 0);
    const remaining = load.currentA - sum;
    const unspecified = selected.filter(p => explicit?.[p.number] === undefined).length;
    const tolerance = 16 * Number.EPSILON * Math.max(1, load.currentA, sum);
    if (remaining < -tolerance || !unspecified && Math.abs(remaining) > tolerance) throw new Error('Pin current sum does not match total load');
    return { side, net, remainingA: remaining, pins: selected.map(p => ({ padId: p.id, number: p.number, currentA: explicit?.[p.number] ?? Math.max(0, remaining) / unspecified, source: explicit?.[p.number] !== undefined ? 'manual' : 'derived-equal' })) };
  });
  assumptions.push(`${load.designator}/${load.rail.name}: all selected rail/return pads are current contacts; equal residual distribution does not short PCB pads`);
  return json({ designator: load.designator, rail: load.rail.name, return: returnNet, totalCurrentA: load.currentA, distributions });
}
