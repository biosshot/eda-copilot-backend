import { createHash } from 'node:crypto';
import type { RoutingBoardV2 } from '../../types/pcb/routing-board-v2.ts';
import { boardSnapshot } from '../analysis-dsl/descriptions.ts';
import type { Diagnostic, Report } from '../analysis-dsl/spec-doc.js';
import type { AnalysisDslOutput } from '../run-analysis-dsl.ts';
import { channelProfile, eyeChart, frequencyCharts, plotSvg } from './charts.ts';
import { dcFields, mapSvg, overviewSvg, thermalFields, type Entry } from './spatial.ts';
import { finite, format, list, record, rows, textCard } from './svg.ts';

export interface AnalysisImage {
  /** Stable, filesystem-safe basename without an extension. */
  name: string;
  kind: 'overview' | 'voltage' | 'current-density' | 'loss' | 'temperature' | 'frequency' | 'eye' | 'channel-z0' |
    'line' | 'heatmap' | 'table' | 'bar' | 'polar' | 'summary';
  analysis: string;
  layer?: string;
  atHz?: number;
  atS?: number;
  svg: string;
}

export interface AnalysisReportSummary {
  status: AnalysisDslOutput['status'];
  physicalAccuracy: 'unverified';
  calculations: { analysis: string; status: string; validity?: string; diagnostics: Diagnostic[] }[];
  checks: Report['summary'];
  diagnostics: Diagnostic[];
  error?: AnalysisDslOutput['error'];
}

export interface RenderedAnalysisReport { images: AnalysisImage[]; summary: AnalysisReportSummary }

function diagnostics(value: unknown): Diagnostic[] {
  return rows(value).flatMap(({ code, message, severity, path }) =>
    typeof code === 'string' && typeof message === 'string' &&
      (severity === 'error' || severity === 'warning' || severity === 'info') ? [{ code, message, severity, ...(typeof path === 'string' && { path }) }] : []);
}

function terminals(report: Report, result: Entry, board: RoutingBoardV2 | undefined, layer: string) {
  if (!board) return [];
  const request = record(record(result.solution).request);
  const markers = new Map<string, { at: readonly [number, number]; name: string; color: string }>();
  const add = (pad: RoutingBoardV2['pads'][number] | undefined, color: string) => {
    if (!pad?.layers.includes(layer) || markers.has(pad.id)) return;
    markers.set(pad.id, { at: [pad.at.x, pad.at.y], name: [pad.component, pad.number].filter(Boolean).join('.'), color });
  };
  const byId = new Map(board.pads.map(pad => [pad.id, pad]));
  const terminalIds = new Map(rows(request.terminals).map(terminal => [terminal.id, String(terminal.pad_id)]));
  const roles = rows(request.elements).filter(e => e.role === 'source' || e.role === 'load');
  for (const element of roles) add(byId.get(terminalIds.get(element.positive) ?? ''), element.role === 'source' ? '#7ce3ae' : '#ffbd73');
  for (const element of roles) add(byId.get(terminalIds.get(element.negative) ?? ''), '#ff91b4');
  for (const port of rows(report.descriptors).filter(entry => entry.kind === 'port')) {
    const input = record(port.input);
    for (const [polarity, color] of [['positive', '#7ce3ae'], ['negative', '#ff91b4']] as const) {
      const contact = record(input[polarity]);
      const pad = board.pads.find(p => p.component === contact.designator && p.number === contact.number ||
        contact.kind === 'via' && p.id === contact.id);
      add(pad, color);
    }
  }
  return Array.from(markers.values());
}

function dcGeometry(solution: Entry, byId: Map<unknown, Entry>) {
  const seen = new Set();
  let base = solution;
  while (base.meshReference !== undefined) {
    if (seen.has(base.meshReference)) return {};
    seen.add(base.meshReference);
    const next = byId.get(base.meshReference);
    if (!next || next.boardIdentity !== solution.boardIdentity) return {};
    base = next;
  }
  return base;
}

function calculationValidity(result: Entry) {
  const direct = record(record(result.solution).validity).status;
  const candidates = Object.values(result).flatMap(item => {
    const validity = record(record(item).validity).status;
    return typeof validity === 'string' ? [validity] : [];
  });
  if (typeof direct === 'string') candidates.push(direct);
  if (candidates.includes('conditional') || record(result.solution).conditional === true) return 'conditional';
  if (candidates.includes('valid') || record(result.solution).conditional === false) return 'valid';
  if (result.kind === 'pi' && list(record(result.solution).bands).length) return 'conditional';
  if (candidates.includes('unavailable')) return 'unavailable';
}

const dcMaps = [
  { key: 'voltageMap', kind: 'voltage', title: 'DC potential', unit: 'V', note: 'Separate mathematical voltage reference for each connected component.', peak: false },
  { key: 'currentDensityMap', kind: 'current-density', title: 'DC current density', unit: 'A/mm²', note: 'Magnitude per triangle. Peaks at sharp corners depend on mesh resolution.', peak: true },
  { key: 'lossMap', kind: 'loss', title: 'DC copper loss', unit: 'W', note: 'Power per triangle. Idle domains carry scenario-asserted zero loss.', peak: false },
] as const;

/** Render saved DSL reports without executing the scenario or loading the native solver.
 * Pass the analyzed board snapshot to include authored copper and pad locations.
 */
export function renderAnalysisReport(output: AnalysisDslOutput, board?: RoutingBoardV2): RenderedAnalysisReport {
  const images: AnalysisImage[] = [];
  const summary: AnalysisReportSummary = {
    status: output.status, physicalAccuracy: 'unverified', calculations: [], checks: { pass: 0, fail: 0, unknown: 0 },
    diagnostics: [], ...(output.error && { error: output.error }),
  };
  const boardHash = board ? createHash('sha256').update(JSON.stringify(boardSnapshot(board))).digest('hex') : undefined;
  const add = (image: Omit<AnalysisImage, 'svg'> & { svg: string | undefined }) => {
    const { svg, ...metadata } = image;
    if (!svg) return;
    const name = metadata.name.toLowerCase().replace(/[^a-z0-9_-]+/g, '-');
    images.push({ ...metadata, name: `${images.length + 1}-${name}`, svg });
  };
  for (const report of output.reports) {
    const matchedBoard = report.boardIdentity && report.boardIdentity === boardHash ? board : undefined;
    summary.checks.pass += report.summary.pass;
    summary.checks.fail += report.summary.fail;
    summary.checks.unknown += report.summary.unknown;
    summary.diagnostics.push(...report.diagnostics);
    const calculations = rows(report.calculations);
    const results = calculations.filter(c => c.phase === 'completed').map(c => record(c.result));
    const byId = new Map(results.map(result => [result.id, record(result.solution)]));
    const spatialById = new Map<unknown, {
      field: ReturnType<typeof dcFields>; geometry: Entry; analysis: string; unit: string; atS?: number; atHz?: number;
      thermal?: { mesh: Entry; axis: readonly unknown[]; temperatures: Entry };
    }>();
    for (const result of results) {
      const analysis = String(result.kind ?? 'analysis');
      const solution = record(result.solution);
      const inputs = record(result.inputs);
      const validity = calculationValidity(result);
      const status = String(result.status ?? 'unknown');
      const caption = [analysis, status, validity].filter(Boolean).join(' · ');
      summary.calculations.push({ analysis, status, validity, diagnostics: diagnostics(result.diagnostics) });
      const dc = analysis === 'dc' || analysis === 'pi' ? dcGeometry(solution, byId) : {};
      const mesh = record(dc.mesh);
      const geometry = record(dc.geometry);
      const spatial = record(solution.spatial);
      const cascade = record(solution.cascade);
      const thermalGeometry = record(spatial.geometry);
      const layerNames = typeof cascade.layer === 'string' ? [cascade.layer] :
        [...new Set([...rows(geometry.layers).map(l => l.layer), ...rows(thermalGeometry.copper).map(l => l.layer),
          ...(matchedBoard?.layers.map(l => l.name) ?? [])].filter((name): name is string => typeof name === 'string'))];
      for (const layer of layerNames) {
        const view = { board: matchedBoard, layer, geometry: analysis === 'thermal' ? thermalGeometry : geometry };
        add({ name: `${analysis}-overview-${layer}`, kind: 'overview', analysis, layer,
          svg: overviewSvg(analysis === 'dc' ? `${caption} · 0 Hz` : caption, view, solution, mesh, terminals(report, result, matchedBoard, layer)) });
      }
      for (const spec of dcMaps) {
        const metric = record(result[spec.key]);
        const field = dcFields(metric, mesh);
        if (field.size) spatialById.set(`${String(result.id)}:${String(record(metric.data).field)}`, { field, analysis, geometry, unit: spec.unit, atHz: 0 });
        for (const [layer, values] of field) {
          const view = { board: matchedBoard, geometry, layer };
          add({ name: `${analysis}-${spec.kind}-${layer}`, kind: spec.kind, analysis, layer, atHz: 0,
            svg: mapSvg(`${spec.title} · ${layer}`, `${caption} · 0 Hz`, view, values, spec.unit, spec.note, spec.peak) });
        }
      }
      const temperature = record(result.temperatureMap);
      const temperatureFields = thermalFields(temperature, record(spatial.mesh));
      const time = record(temperature.data).timeS;
      const atS = finite(time) ? time : undefined;
      if (temperatureFields.size) spatialById.set(`${String(result.id)}:temperature`,
        { field: temperatureFields, analysis, geometry: thermalGeometry, unit: '°C', atS,
          thermal: { mesh: record(spatial.mesh), axis: list(solution.axis), temperatures: record(solution.temperaturesC) } });
      for (const [layer, values] of temperatureFields) {
        const view = { board: matchedBoard, geometry: thermalGeometry, layer };
        const when = inputs.domain === 'steady' ? 'steady state' : atS === undefined ? 'snapshot' : `t = ${format(atS)} s`;
        add({ name: `thermal-temperature-${layer}`, kind: 'temperature', analysis, layer, atS,
          svg: mapSvg(`Temperature · ${layer}`, `${caption} · ${when}`, view, values, '°C',
            'Color shows triangle averages. Marker shows the highest nodal temperature on this layer.', true) });
      }
      for (const image of frequencyCharts(solution, inputs, analysis, typeof cascade.layer === 'string' ? `${caption} · ${cascade.layer}` : caption)) {
        add({ ...image, kind: 'frequency', analysis, ...(typeof cascade.layer === 'string' && { layer: cascade.layer }) });
      }
      if (analysis === 'eye') add({ name: 'eye', kind: 'eye', analysis, svg: eyeChart('Eye diagram', caption, record(record(result.diagram).data)) });
      if (typeof cascade.layer === 'string') add({ name: 'channel-z0', kind: 'channel-z0', analysis, layer: cascade.layer,
        svg: channelProfile(cascade, `${caption} · ${cascade.layer}`, list(solution.frequenciesHz)) });
    }
    for (const plot of rows(report.plots)) {
      const kind = plot.kind;
      if (kind !== 'line' && kind !== 'bar' && kind !== 'eye' && kind !== 'heatmap' && kind !== 'table' && kind !== 'polar') continue;
      const metric = record(plot.data);
      const validity = record(metric.validity).status;
      const layer = typeof plot.layer === 'string' ? plot.layer : undefined;
      const atHz = finite(plot.atHz) ? plot.atHz : undefined;
      const atS = finite(plot.atS) ? plot.atS : undefined;
      const caption = ['Scenario plot', validity ?? 'validity unspecified', layer,
        atHz === undefined ? undefined : `${format(atHz)} Hz`, atS === undefined ? undefined : `t = ${format(atS)} s`].filter(Boolean).join(' · ');
      const spatial = record(metric.data);
      const stored = spatialById.get(`${String(spatial.solutionId)}:${String(spatial.field ?? 'temperature')}`);
      if (kind === 'heatmap' && stored) {
        let fields = stored.field;
        if (atHz !== undefined && atHz !== stored.atHz) fields = new Map();
        if (atS !== undefined && atS !== stored.atS) {
          const thermal = stored.thermal;
          const timeIndex = thermal?.axis.indexOf(atS) ?? -1;
          fields = thermal && timeIndex >= 0 && atHz === undefined
            ? thermalFields({ values: rows(thermal.mesh.fieldNodes).map(node =>
              list(thermal.temperatures[String(node.id)])[timeIndex]) }, thermal.mesh)
            : new Map();
        }
        const selected = [...fields].filter(([fieldLayer]) => layer === undefined || layer === fieldLayer);
        for (const [fieldLayer, field] of selected) {
          const view = { board: matchedBoard, layer: fieldLayer, geometry: stored.geometry };
          add({ name: String(plot.title ?? kind), kind, analysis: stored.analysis, layer: fieldLayer, atHz: atHz ?? stored.atHz, atS: atS ?? stored.atS,
            svg: mapSvg(`${String(plot.title ?? 'Heatmap')} · ${fieldLayer}`, caption, view, field, stored.unit, '', false) });
        }
        if (!selected.length) {
          add({ name: String(plot.title ?? kind), kind, analysis: stored.analysis, layer, atHz, atS,
            svg: textCard(String(plot.title ?? 'Heatmap'), caption, ['No field available for the requested layer, frequency or time.']) });
        }
      } else {
        add({ name: String(plot.title ?? kind), kind, analysis: 'plot', layer, atHz, atS, svg: plotSvg(plot, caption) });
      }
    }
  }
  const lines = summary.calculations.flatMap((item, index) => [
    `${index + 1}. ${item.analysis}  |  ${item.status}${item.validity ? '  |  ' + item.validity : ''}`,
    ...item.diagnostics.map(d => `${d.code} · ${d.message}`),
  ]);
  for (const report of output.reports) for (const check of report.checks) {
    if (check.status !== 'pass') lines.push(`${check.status} · ${check.name} · ${check.message}`);
  }
  if (summary.error) lines.push(`${summary.error.code} · ${summary.error.message}`);
  lines.push(...summary.diagnostics.filter(d => d.code !== summary.error?.code).map(d => `${d.code} · ${d.message}`));
  lines.push(`Checks · ${summary.checks.pass} pass · ${summary.checks.fail} fail · ${summary.checks.unknown} unknown`);
  add({ name: 'summary', kind: 'summary', analysis: 'scenario',
    svg: textCard('Analysis status', `Scenario · ${output.status}`, lines) });
  return { images, summary };
}

