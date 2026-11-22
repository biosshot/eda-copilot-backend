import { accent, color, colorKey, colors, finite, format, frame, label, list, muted, record, rows, textCard, unknown } from './svg.ts';
import type { Entry } from './spatial.ts';

interface Series { name: string; values: readonly unknown[] }
interface Chart { x: readonly unknown[]; xUnit: string; unit: string; series: Series[]; target?: number; logX?: boolean; bars?: boolean; note?: string }

export function chart(title: string, status: string, spec: Chart) {
  const { x, series, unit, xUnit, target, logX, bars, note } = spec;
  const axis = x.map(value => finite(value) && (!logX || value > 0) ? (logX ? Math.log10(value) : value) : undefined);
  let loX = Infinity, hiX = -Infinity, loY = target ?? Infinity, hiY = target ?? -Infinity;
  let available = 0;
  for (const value of axis) {
    if (!finite(value)) continue;
    loX = Math.min(loX, value); hiX = Math.max(hiX, value);
  }
  for (const row of series) row.values.forEach((value, i) => {
    if (!finite(value) || !finite(axis[i])) return;
    loY = Math.min(loY, value); hiY = Math.max(hiY, value);
    available++;
  });
  if (!available) return;
  if (bars) { loY = Math.min(loY, 0); hiY = Math.max(hiY, 0); }
  if (loX === hiX) { loX -= 0.5; hiX += 0.5; }
  const pad = hiY === loY ? Math.abs(hiY) * 0.08 || 1 : (hiY - loY) * 0.07;
  loY -= pad; hiY += pad;
  const px = (v: number) => 104 + 782 * (v - loX) / (hiX - loX);
  const py = (v: number) => 375 - 258 * (v - loY) / (hiY - loY);
  let body = label(47, 105, unit, { color: muted });
  for (let i = 0; i <= 4; i++) {
    const f = i / 4;
    const y = loY + (hiY - loY) * f;
    const xval = loX + (hiX - loX) * f;
    const displayX = logX ? (10 ** xval).toExponential(1).replace('e+', 'e') : format(xval);
    body += `<path d="M104 ${py(y)}H886 M${px(xval)} 117V375" stroke="#2a3b50"/>` +
      label(96, py(y) + 4, format(y), { anchor: 'end', size: 12 }) +
      label(px(xval), 397, displayX, { anchor: 'middle', size: 12 });
  }
  body += label(495, 424, xUnit + (logX ? ' · log scale' : ''), { anchor: 'middle', color: muted });
  if (target !== undefined) {
    body += `<path d="M104 ${py(target)}H886" stroke="${accent}" stroke-width="2" stroke-dasharray="7 5"/>` +
      label(888, py(target) - 6, `Target ${format(target)} ${unit}`, { color: accent, anchor: 'end', size: 13 });
  }
  series.forEach((series, i) => {
    const stroke = colors[i % colors.length]!;
    let path = '', connected = false;
    series.values.forEach((value, j) => {
      const at = axis[j];
      if (!finite(at) || !finite(value)) { connected = false; return; }
      const x = px(at), y = py(value);
      if (bars) {
        const bar = Math.min(34, 650 / (axis.length * series.values.length || 1));
        body += `<rect x="${x - bar / 2}" y="${Math.min(y, py(0))}" width="${bar}" height="${Math.abs(y - py(0))}" fill="${stroke}"/>`;
        return;
      }
      path += `${connected ? 'L' : 'M'}${x.toFixed(2)},${y.toFixed(2)}`;
      connected = true;
      const exceeds = target !== undefined && value > target;
      body += `<circle cx="${x.toFixed(2)}" cy="${y.toFixed(2)}" r="${exceeds ? 4 : 2}" fill="${exceeds ? accent : stroke}"><title>${format(value)}</title></circle>`;
    });
    body += `<path d="${path}" fill="none" stroke="${stroke}" stroke-width="1.7"/>`;
    body += label(32 + i % 3 * 300, 457 + Math.floor(i / 3) * 22, series.name, { color: stroke, size: 13 });
  });
  const y = 465 + 22 * Math.ceil(series.length / 3);
  if (note) body += label(32, y, note, { color: muted, size: 13 });
  return frame(title, status, body, y + 59);
}

function complex(value: unknown) {
  const { real, imaginary } = record(value);
  return finite(real) && finite(imaginary) ? { magnitude: Math.hypot(real, imaginary), phase: Math.atan2(imaginary, real) * 180 / Math.PI } : undefined;
}

export function frequencyCharts(solution: Entry, inputs: Entry, kind: string, status: string) {
  const circuitSweep = kind === 'pi' && solution.mode === 'frequency';
  const x = list(solution.frequenciesHz ?? (circuitSweep ? solution.axis : undefined));
  if (!x.length) return [];
  const xUnit = 'Frequency [Hz]';
  const output: { name: string; svg: string }[] = [];
  const add = (name: string, title: string, unit: string, series: Series[], target?: number, note?: string) => {
    const svg = chart(title, status, { x, xUnit, unit, series, logX: x.some(v => finite(v) && v > 0) && !x.includes(0), target, note });
    if (svg) output.push({ name, svg });
  };
  const z = list(solution.zMatrix ?? (solution.representation === 'Z' ? solution.parameters : undefined));
  const s = list(solution.representation === 'S' || solution.mode === 'cascade' ||
    (kind === 'impedance' && solution.referenceOhm !== undefined && solution.representation === undefined) ? solution.parameters : undefined);
  const pick = (matrix: readonly unknown[], i: number, j: number) => matrix.map(sample => complex(list(list(sample)[i])[j]));
  const circuitPorts = circuitSweep ? Object.entries(record(solution.impedances)) : [];
  if (circuitPorts.length) {
    const target = finite(inputs.targetImpedanceOhm) ? inputs.targetImpedanceOhm : undefined;
    add('impedance', 'Driving-point impedance · magnitude', 'Ω', circuitPorts.map(([name, values]) => ({
      name, values: list(values).map(value => complex(value)?.magnitude),
    })), target, target === undefined ? 'Target impedance unspecified.' : undefined);
    add('phase', 'Driving-point impedance · phase', '°', circuitPorts.map(([name, values]) => ({
      name, values: list(values).map(value => complex(value)?.phase),
    })));
  }
  if (z.length) {
    const portNames = list(solution.portNames ?? solution.ports);
    const portMap = rows(solution.portMap);
    const count = list(z.find(Array.isArray)).length;
    const names = Array.from({ length: count }, (_, i) => {
      const port = portMap[i];
      if (!port) return String(portNames[i] ?? `Port ${i + 1}`);
      const contact = record(port.positive);
      return `${i + 1} · ${String(list(port.roles)[0] ?? 'port')} ${contact.designator ?? ''}.${contact.number ?? ''}`;
    });
    const target = kind === 'pi' && finite(inputs.targetImpedanceOhm) ? inputs.targetImpedanceOhm : undefined;
    const unavailable = x.length - z.filter(Array.isArray).length;
    const notes = [target === undefined && kind === 'pi' ? 'Target impedance unspecified.' : '',
      unavailable ? `${unavailable} frequency samples unavailable.` : ''].filter(Boolean).join(' ');
    add('impedance', 'Driving-point impedance · magnitude', 'Ω', names.map((name, i) => ({
      name, values: pick(z, i, i).map(value => value?.magnitude),
    })), target, notes);
    add('phase', 'Driving-point impedance · phase', '°', names.map((name, i) => ({
      name, values: pick(z, i, i).map(value => value?.phase),
    })), undefined, unavailable ? `${unavailable} frequency samples unavailable.` : undefined);
  }
  if (s.length) {
    const coefficients = [{ name: 'S11', row: 0 }, { name: 'S21', row: 1 }];
    add('s-parameters', 'Scattering parameters', 'dB', coefficients.map(({ name, row }) => ({
      name, values: pick(s, row, 0).map(value => value && value.magnitude > 0 ? 20 * Math.log10(value.magnitude) : undefined),
    })).filter(series => series.values.some(finite)), undefined,
      list(solution.referenceOhm).length ? `Reference: ${list(solution.referenceOhm).join(' / ')} Ω. Zero magnitude is −∞ dB.` :
        'Zero magnitude is −∞ dB and is omitted from the finite scale.');
  }
  return output;
}

export function eyeChart(title: string, status: string, data: Entry) {
  return chart(title, status, { x: list(data.phaseS), xUnit: 'Phase within the unit interval [s]', unit: 'V',
    series: ['highMinV', 'highMaxV', 'lowMinV', 'lowMaxV'].map((key, i) => ({
      name: ['High minimum', 'High maximum', 'Low minimum', 'Low maximum'][i]!, values: list(data[key]),
    })), note: 'Envelope by phase bin. Empty bins remain gaps.' });
}

export function plotSvg(plot: Entry, status: string) {
  const metric = record(plot.data);
  const data = record(metric.data ?? plot.data);
  const title = String(plot.title ?? 'Scenario plot');
  const unit = String(metric.unit ?? 'value');
  const validity = record(metric.validity);
  if (validity.status === 'unavailable') return textCard(title, status, [String(validity.reason ?? 'Data unavailable')]);
  if (plot.kind === 'eye') return eyeChart(title, status, data) ?? textCard(title, status, ['Eye envelope unavailable.']);
  const values = list(metric.values ?? data.values ?? plot.data);
  const axis = record(metric.axis ?? data.axis);
  if (plot.kind === 'line' || plot.kind === 'bar') {
    const complexSeries = values.some(value => complex(value) !== undefined);
    const plotted = complexSeries ? values.map(value => complex(value)?.magnitude) : values;
    return chart(title, status, { x: axis.values ? list(axis.values) : Array.from(plotted.keys()),
      xUnit: String(axis.unit ?? 'Sample index'), unit, series: [{ name: complexSeries ? 'Magnitude' : title, values: plotted }],
      bars: plot.kind === 'bar', logX: axis.unit === 'Hz' && list(axis.values).every(v => finite(v) && v > 0) }) ??
      textCard(title, status, ['No finite series samples.']);
  }
  if (plot.kind === 'heatmap') {
    const grid = list(data.grid ?? data.values ?? metric.values ?? plot.data).map(list);
    const columns = grid.reduce((max, row) => Math.max(max, row.length), 0);
    let min = Infinity, max = -Infinity;
    for (const row of grid) for (const value of row) if (finite(value)) { min = Math.min(min, value); max = Math.max(max, value); }
    if (min === Infinity) return textCard(title, status, ['No finite grid cells.']);
    const w = 896 / columns, h = Math.min(50, 410 / grid.length);
    const cells = grid.flatMap((row, y) => Array.from({ length: columns }, (_, x) => {
      const v = row[x];
      return `<rect x="${32 + w * x}" y="${98 + h * y}" width="${w}" height="${h}" fill="${finite(v) ? color(v, min, max) : unknown}"/>`;
    })).join('');
    const end = 98 + h * grid.length;
    return frame(title, status, cells + colorKey(min, max, unit, end + 25, 'Unknown grid cell'), end + 115);
  }
  if (plot.kind === 'table') {
    const table = list(data.rows ?? plot.data);
    const entries = table.length ? table : Object.entries(data);
    const display = (value: unknown): string => {
      if (Array.isArray(value)) return `[${value.length} items]`;
      if (value && typeof value === 'object') return '{' + Object.entries(value).slice(0, 8)
        .map(([key, val]) => `${key}=${Array.isArray(val) ? `[${val.length} items]` : typeof val === 'object' && val !== null ? '{…}' : display(val)}`).join(' · ') + '}';
      return finite(value) ? format(value) : String(value ?? 'unknown');
    };
    return textCard(title, status, [`Units · ${unit}`, ...entries.slice(0, 25).map(row =>
      Array.isArray(row) ? row.map(display).join('  |  ') : display(row)),
      ...(entries.length > 25 ? [`${entries.length - 25} additional rows in result data.`] : [])]);
  }
  return textCard(title, status, [`Plot type ${String(plot.kind)} has no SVG renderer.`]);
}

export function channelProfile(cascade: Entry, status: string, frequenciesHz: readonly unknown[]) {
  const sections = rows(cascade.sections);
  const values = sections.flatMap(section => rows(section.modes).map(mode => mode.impedanceOhm)).filter(finite);
  if (!values.length || !sections.length) return;
  const min = Math.min(...values), max = Math.max(...values);
  const length = sections.reduce((sum, row) => sum + (finite(row.lengthMm) ? row.lengthMm : 0), 0);
  if (!(length > 0)) return;
  const x = (distance: number) => 102 + 803 * distance / length;
  const modeCount = Math.max(...sections.map(section => rows(section.modes).length));
  const joints = rows(cascade.joints);
  const jointY = 160 + modeCount * 36;
  const end = jointY + 95;
  let distance = 0;
  let body = label(32, 110, 'Section modal Z₀ [Ω] · distance along the cascade [mm]', { color: muted });
  for (let mode = 0; mode < modeCount; mode++) body += label(32, 170 + mode * 36, `Mode ${mode + 1}`, { size: 13 });
  for (const [i, section] of sections.entries()) {
    if (!finite(section.lengthMm)) continue;
    const start = x(distance);
    const w = 803 * section.lengthMm / length;
    body += label(start + w / 2, 138, section.id, { anchor: 'middle', size: 12 });
    for (let mode = 0; mode < modeCount; mode++) {
      const v = rows(section.modes)[mode]?.impedanceOhm;
      const y = 151 + mode * 36;
      body += `<rect x="${start}" y="${y}" width="${w}" height="30" fill="${finite(v) ? color(v, min, max) : unknown}" stroke="#101b2a"/>` +
        label(start + w / 2, y + 20, finite(v) ? format(v) : 'unknown', { anchor: 'middle', color: finite(v) && v > (min + max) / 2 ? '#101b2a' : '#ffffff', size: 13 });
    }
    distance += section.lengthMm;
    const joint = joints.find(j => j.afterSection === section.id);
    const v = joint?.estimatedS11AtTopFrequency;
    if (finite(v)) {
      const top = frequenciesHz.filter(finite).at(-1);
      body += `<path d="M${x(distance)} 150V${jointY + 12 + (i % 2) * 20}" stroke="${accent}" stroke-dasharray="4 3"/>` +
        label(x(distance), jointY + 25 + (i % 2) * 20, `|ΔS11| ≈ ${format(v)}${joint?.shortNeighbour === true ? '*' : ''}${top === undefined ? '' : ` @ ${top.toExponential(1).replace('e+', 'e')} Hz`}`, { anchor: 'middle', color: accent, size: 12 });
    }
  }
  body += label(x(0), end - 22, '0 mm', { anchor: 'middle' }) + label(x(length), end - 22, `${format(length)} mm`, { anchor: 'end' }) +
    colorKey(min, max, 'Ω', end, 'Missing modal value');
  if (joints.some(j => j.shortNeighbour === true)) {
    body += label(32, end + 59, '* Adjacent sections are too short for the joint estimate to be valid.', { color: muted, size: 13 });
  }
  return frame('Characteristic impedance along the channel', status, body, end + 109);
}

