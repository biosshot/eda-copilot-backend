import { randomUUID } from 'node:crypto';
import { boardChannelApplicability, solveHybridBoardChannel } from '../hybrid-board-channel.ts';
import { createProblemExecutor } from '../execute-problem.ts';
import { p7FrequencyBoundariesHz, type FrequencyInterval } from '../p7-frequency-planning.ts';
import { boardFoilResult } from './board-foil-result.ts';
import { json } from './data.ts';
import { electricalModel } from './electrical-models.ts';
import type { AnalysisTask, ExecutionResult } from './executor.ts';
import { localChannelGroupRequest, type LocalExtractionPlan } from './local-request.ts';
import { AnalysisDslError } from './schema.ts';
import { withBudgetHint } from './budget-hint.ts';
import { resolveMemoryMiB } from './machine-memory.ts';
import { sourceBandPlan } from './source-band.ts';
import type { Binding, Diagnostic } from './spec-doc.js';
import { transientStimulus } from './transient-executor.ts';

const method = 'Joint P7 board channel extraction';
const temporal = { status: 'unavailable', reason: 'P11 board SI waveforms and timing are unavailable' };
const messageOf = (error: unknown) => error instanceof Error ? error.message : String(error);

function sourceDemand(task: AnalysisTask<'si'>) {
  const { drivers, time } = task.config;
  if (!drivers?.length || !time) {
    throw new AnalysisDslError('CHANNEL_GROUP_SOURCE_DEMAND', 'Joint channel extraction needs authored drivers and time to derive spectral demand');
  }
  const sources = drivers.map(({ stimulus, model }, index) => {
    try {
      if (stimulus.kind === 'waveform') return sourceBandPlan(stimulus.voltageV, time);
      const resolved = electricalModel(model, task, ['si']).model;
      if (resolved.kind !== 'linear-driver') throw new Error('Digital stimulus requires authored finite linear-driver levels');
      return sourceBandPlan(transientStimulus(stimulus, resolved.lowV, resolved.highV), time);
    } catch (error) {
      throw new AnalysisDslError('CHANNEL_GROUP_SOURCE_DEMAND', 'Driver ' + (index + 1) + ': ' + messageOf(error));
    }
  });
  let stop = 0;
  for (const [index, source] of sources.entries()) {
    if (source.status === 'unavailable') {
      throw new AnalysisDslError('CHANNEL_GROUP_SOURCE_DEMAND', 'Driver ' + (index + 1) + ': ' + source.reason);
    }
    stop = Math.max(stop, source.requiredUpperHz);
  }
  const requestedIntervalHz: FrequencyInterval = [0, stop];
  return { sources, requestedIntervalHz };
}

function extractionAxis(requested: FrequencyInterval, applicability: FrequencyInterval, characteristicHz: readonly number[]) {
  const start = Math.max(requested[0], applicability[0]);
  const stop = Math.min(requested[1], applicability[1]);
  if (start > stop) return [];
  const inside = (f: number) => start < f && f < stop;
  const candidates = [...new Set(characteristicHz.filter(inside))].sort((a, b) => a - b);
  // Keep the exact intersection ends and P7 controls. Sample excess source
  // characteristics by sorted rank to bound the work independently of bit count.
  const limit = 8;
  const retained = candidates.length <= limit ? candidates :
    Array.from({ length: limit }, (_, i) => candidates[Math.round(i * (candidates.length - 1) / (limit - 1))]);
  return [...new Set([start, ...p7FrequencyBoundariesHz.filter(inside), ...retained, stop])].sort((a, b) => a - b);
}

function outsideApplicability([start, stop]: FrequencyInterval, [low, high]: FrequencyInterval) {
  if (low > high || stop < low || start > high) return [{ intervalHz: [start, stop], reason: 'outside applicability' }];
  return [
    ...(start < low ? [{ intervalHz: [start, low], includeStop: false, reason: 'outside applicability' }] : []),
    ...(stop > high ? [{ intervalHz: [high, stop], includeStart: false, reason: 'outside applicability' }] : []),
  ];
}

const executeFoil = createProblemExecutor(true);
const solveGroup = solveHybridBoardChannel.bind(null, executeFoil);

export function createGroupExecutor(solve = solveGroup) {
  return async (task: AnalysisTask<'si'>): Promise<ExecutionResult> => {
    const { config } = task;
    const { sources, requestedIntervalHz } = sourceDemand(task);
    if (task.variantChanges.length || config.numerics) {
      throw new AnalysisDslError('LOCAL_INPUT', 'Joint extraction requires unchanged captured copper and supported default numerics');
    }
    let plan: LocalExtractionPlan | undefined;
    const geometry = localChannelGroupRequest(task, prepared => { plan = prepared; });
    if (!plan?.terminalRequest || !plan.portHandles) {
      throw new AnalysisDslError('CHANNEL_GROUP_MAPPING', 'Joint physical terminal request or handle bindings are missing');
    }
    const { terminalRequest, portHandles } = plan;
    const diagnostics: Diagnostic[] = plan.diagnosticDecisions.map(({ diagnostic, decision, reason }) => ({
      code: diagnostic.code, severity: 'warning', message: (diagnostic.message ?? diagnostic.code) + '; ' + decision + ': ' + reason,
      path: diagnostic.path,
    }));
    diagnostics.push({ code: 'CHANNEL_GROUP_TEMPORAL_UNAVAILABLE', severity: 'warning', message: temporal.reason });
    const portByHandle = new Map(portHandles.map(({ handleId, portName }) => [handleId, portName]));
    const bindingPorts = (binding: Binding) => Object.fromEntries(
      (binding.port ? [['port', binding.port]] as const : Object.entries(binding.ports)).map(([name, handle]) => {
        const portName = portByHandle.get(handle.id);
        if (!portName) throw new AnalysisDslError('CHANNEL_GROUP_MAPPING', 'Driver or receiver port is outside the selected physical channel group: ' + handle.id);
        return [name, { handleId: handle.id, portName }];
      }),
    );
    const drivers = config.drivers!.map((driver, i) => ({ ports: bindingPorts(driver), spectrum: sources[i] }));
    const receivers = (config.receivers ?? []).map(receiver => ({ ports: bindingPorts(receiver) }));
    const terminals = new Map([...terminalRequest.signals, ...terminalRequest.references].map(t => [t.id, t]));
    const physicalPorts = terminalRequest.ports.map(({ name, positive, negative }) => ({
      name, positive: terminals.get(positive)!, negative: terminals.get(negative)!,
    }));
    const applicabilityBandHz = boardChannelApplicability(terminalRequest);
    const seeds = extractionAxis(requestedIntervalHz, applicabilityBandHz,
      sources.flatMap(source => source.status === 'bounded-input' ? source.characteristicHz : []));
    const uncovered = outsideApplicability(requestedIntervalHz, applicabilityBandHz);
    const extraction = {
      kind: 'board-channel-extraction', status: 'unverified', requestedIntervalHz, applicabilityBandHz,
      uncovered, seedFrequenciesHz: seeds, temporal, portHandles, physicalPorts, drivers, receivers,
    };
    if (uncovered.length) diagnostics.push({
      code: 'CHANNEL_GROUP_FREQUENCY_GAPS', severity: 'warning',
      message: 'Source demand includes frequencies outside AC applicability. The full request and gaps are recorded in the extraction artifact.',
    });
    const base = {
      id: task.contextId + ':group-' + randomUUID(), status: 'partial' as const, method, diagnostics, metrics: {},
      assumptions: [
        'The authored sources set input spectral demand only. Driver and receiver attachments and stored state are reserved for temporal analysis.',
        'Physical channel legs share one passive extraction with their original finite paired contacts. S50 references are mathematical reference impedances.',
      ],
    };
    if (!seeds.length) {
      diagnostics.push({ code: 'CHANNEL_GROUP_NO_APPLICABLE_AC', severity: 'warning', message: 'No requested AC frequency lies in the common board and component applicability.' });
      return { ...base, solution: json(extraction) };
    }
    const budget = { maxSeconds: config.budget?.maxSeconds ?? 1800, maxMemoryMiB: resolveMemoryMiB(config.budget?.maxMemoryMiB).maxMemoryMiB };
    let solved: ReturnType<typeof solve>;
    try {
      solved = solve(task.board, geometry, terminalRequest, seeds, budget, requestedIntervalHz);
    } catch (error) {
      if (error instanceof AnalysisDslError) throw error;
      const message = messageOf(error);
      { const code = message.match(/^([A-Z][A-Z0-9_]+):/)?.[1] ?? 'CHANNEL_REJECTED'; throw new AnalysisDslError(code, withBudgetHint(code, message, budget, config.budget?.maxMemoryMiB === undefined), diagnostics); }
    }
    const checkPorts = (ports?: readonly string[]) => {
      if (ports?.length !== physicalPorts.length || ports.some((name, i) => name !== physicalPorts[i].name)) {
        throw new AnalysisDslError('CHANNEL_GROUP_MAPPING', 'Solved port names differ from the physical channel group', diagnostics);
      }
    };
    const foil = boardFoilResult(task.contextId, solved.foil, solved.report, diagnostics);
    checkPorts(foil.network?.ports);
    return { ...base, id: foil.id, method: foil.method, diagnostics: foil.diagnostics, assumptions: [...base.assumptions, ...foil.assumptions],
      solution: json({ ...extraction, network: foil.network, evidence: foil.solution }) };
  };
}
