import type { RoutingBoardV2 } from '../../types/pcb/routing-board-v2.ts';
import { accent, color, colorKey, escape, finite, format, frame, label, list, record, rows, unknown } from './svg.ts';

export type Point = readonly [number, number];
export type Entry = Record<string, unknown>;
const plotWidth = 896;
const plotHeight = 510;
const idleColor = '#9985be';

export function point(value: unknown): Point | undefined {
  const [x, y] = Array.isArray(value) ? value : [record(value).x, record(value).y];
  if (finite(x) && finite(y)) return [x, y];
}

function points(value: unknown): Point[] {
  return list(value).map(point).filter(p => p !== undefined);
}

function bounds(outline: Point[]) {
  let x = Infinity, y = Infinity, right = -Infinity, bottom = -Infinity;
  for (const [px, py] of outline) {
    x = Math.min(x, px); y = Math.min(y, py);
    right = Math.max(right, px); bottom = Math.max(bottom, py);
  }
  const scale = Math.min(plotWidth / (right - x || 1), plotHeight / (bottom - y || 1)) * 0.95;
  const project = ([px, py]: Point): Point => [
    32 + plotWidth / 2 + (px - (x + right) / 2) * scale,
    95 + plotHeight / 2 + (py - (y + bottom) / 2) * scale,
  ];
  const xy = (p: Point) => project(p).map(n => n.toFixed(2)).join(',');
  return { project, scale, xy, ring: (ring: Point[]) => ring.length ? 'M' + ring.map(xy).join('L') + 'Z' : '' };
}

type Projection = ReturnType<typeof bounds>;
function polygon(value: unknown, project: Projection) {
  const poly = record(value);
  return [points(poly.outer), ...list(poly.holes).map(points)].map(project.ring).join('');
}

export function trackLine(track: RoutingBoardV2['copper']['fixed']['tracks'][number], projection: Projection, stroke: string) {
  return `<polyline points="${points(track.points).map(projection.xy).join(' ')}" stroke="${stroke}" fill="none" stroke-width="${track.widthMm * projection.scale}" stroke-linecap="round" stroke-linejoin="round"/>`;
}

function boardCopper(board: RoutingBoardV2, layer: string, project: Projection) {
  const collections = [board.copper.fixed, board.copper.editable];
  const fills = collections.flatMap(group => group.zones.flatMap(zone => zone.filledPolygons))
    .flatMap(fill => fill.layer === layer && fill.status === 'available' ? fill.polygons : []);
  const tracks = collections.flatMap(group => group.tracks).filter(track => track.layer === layer);
  const pads = board.pads.filter(pad => pad.layers.includes(layer));
  return fills.map(poly => `<path d="${polygon(poly, project)}" fill-rule="evenodd" fill="${unknown}"/>`).join('') +
    tracks.map(track => trackLine(track, project, unknown)).join('') +
    pads.map(pad => {
      const [x, y] = project.project([pad.at.x, pad.at.y]);
      const shape = pad.shapesByLayer?.[layer] ?? pad.shape;
      const scale = project.scale;
      const drawing = shape.kind === 'polygon' ? `<path d="${polygon(shape.polygon, {
        ...project, ring: ring => ring.length ? 'M' + ring.map(([a, b]) => `${a * scale},${b * scale}`).join('L') + 'Z' : '',
      })}" fill-rule="evenodd"/>` :
        shape.kind === 'circle' ? `<circle r="${shape.diameterMm * scale / 2}"/>` :
        `<rect x="${-shape.widthMm * scale / 2}" y="${-shape.heightMm * scale / 2}" width="${shape.widthMm * scale}" height="${shape.heightMm * scale}" rx="${
          shape.kind === 'oval' ? Math.min(shape.widthMm, shape.heightMm) * scale / 2 :
            shape.kind === 'round-rect' ? shape.cornerRadiusMm * scale : 0}"/>`;
      return `<g transform="translate(${x} ${y}) rotate(${pad.rotationDeg})" fill="${unknown}">${drawing}</g>`;
    }).join('');
}

export interface Cell { vertices: Point[]; value: number }
export interface LayerField { cells: Cell[]; peak?: { at: Point; value: number } }

function vertices(triangle: unknown, nodes: readonly unknown[]) {
  const indices = list(triangle);
  if (indices.length !== 3) return [];
  return indices.map(index => finite(index) ? point(nodes[index]) : undefined).filter(p => p !== undefined);
}

export function dcFields(metric: Entry, mesh: Entry): Map<string, LayerField> {
  const fields = new Map<string, LayerField>();
  const data = record(metric.data);
  const values = list(metric.values);
  const domains = new Map(rows(mesh.domains).map(domain => [domain.id, domain]));
  for (const slice of rows(data.slices)) {
    const domain = domains.get(slice.domainId);
    if (!domain || typeof domain.layer !== 'string' || !finite(slice.start) || !finite(slice.count)) continue;
    const field = fields.get(domain.layer) ?? { cells: [] };
    fields.set(domain.layer, field);
    const offset = slice.start;
    const samples = values.slice(offset, offset + slice.count);
    const nodes = list(domain.nodes);
    list(domain.triangles).forEach((triangle, index) => {
      const corners = vertices(triangle, nodes);
      if (corners.length !== 3) return;
      const nodal = list(triangle).map(i => finite(i) ? samples[i] : undefined);
      const value = data.association === 'mesh-node'
        ? (nodal.every(finite) ? nodal.reduce((a, b) => a + b, 0) / 3 : undefined)
        : samples[index];
      if (finite(value)) field.cells.push({ vertices: corners, value });
    });
  }
  return fields;
}

export function thermalFields(metric: Entry, mesh: Entry) {
  const nodes = list(mesh.pointsMm);
  const values = list(metric.values);
  const byLayer = new Map<string, Map<number, number>>();
  const fields = new Map<string, LayerField>();
  list(mesh.fieldNodes).forEach((item, index) => {
    const { layer, id } = record(item);
    const value = values[index];
    if (typeof layer !== 'string' || typeof id !== 'string' || !finite(value)) return;
    const node = Number(id.split(':').at(-1));
    const at = point(nodes[node]);
    if (!at) return;
    const samples = byLayer.get(layer) ?? new Map<number, number>();
    samples.set(node, value);
    byLayer.set(layer, samples);
    const field = fields.get(layer) ?? { cells: [] };
    if (!field.peak || value > field.peak.value) field.peak = { at, value };
    fields.set(layer, field);
  });
  for (const [layer, samples] of byLayer) {
    const field = fields.get(layer)!;
    for (const triangle of list(mesh.triangles)) {
      const corners = vertices(triangle, nodes);
      const nodal = list(triangle).map(i => finite(i) ? samples.get(i) : undefined);
      if (corners.length === 3 && nodal.every(finite)) {
        field.cells.push({ vertices: corners, value: nodal.reduce((a, b) => a + b, 0) / 3 });
      }
    }
  }
  return fields;
}

interface BoardView {
  board?: RoutingBoardV2;
  geometry: Entry;
  layer: string;
  fallbackPoints?: Point[];
}

function boardBody(view: BoardView, overlay: (projection: Projection) => string, marks?: (projection: Projection) => string) {
  const { board, layer, geometry } = view;
  const outline = points(record(geometry.outline).outer ?? board?.outline);
  const extent = outline.length ? outline : view.fallbackPoints ?? [];
  if (!extent.length) return '';
  const projection = bounds(extent);
  const capturedLayer = rows(geometry.layers ?? geometry.copper).find(g => g.layer === layer);
  const outlinePath = geometry.outline ? polygon(geometry.outline, projection) : projection.ring(outline);
  const copper = capturedLayer
    ? list(capturedLayer.copper).map(p => `<path d="${polygon(p, projection)}" fill="${unknown}" fill-rule="evenodd"/>`).join('')
    : board ? boardCopper(board, layer, projection) : '';
  const holes = rows(geometry.holes).filter(h => list(h.layers).includes(layer)).flatMap(h => list(h.polygons));
  const outlineDrawing = outlinePath ? `<path d="${outlinePath}" fill="none" stroke="#8196ac" fill-rule="evenodd"/>` : '';
  const scaleBar = 10 ** Math.floor(Math.log10(130 / projection.scale));
  return `<rect x="32" y="94" width="896" height="515" fill="#152338" stroke="#33465a" rx="6"/>` +
    (outlinePath ? `<defs><clipPath id="board-outline"><path d="${outlinePath}" clip-rule="evenodd"/></clipPath></defs><g clip-path="url(#board-outline)">` : '') +
    copper + overlay(projection) + holes.map(p => `<path d="${polygon(p, projection)}" fill="#152338" fill-rule="evenodd"/>`).join('') +
    (outlinePath ? '</g>' : '') + outlineDrawing + (marks?.(projection) ?? '') +
    `<path d="M${905-scaleBar*projection.scale} 596H905" stroke="#cadbea" stroke-width="2"/>` +
    label(905, 585, `${format(scaleBar)} mm`, { anchor: 'end', size: 12 });
}

export function mapSvg(title: string, status: string, view: BoardView, field: LayerField, unit: string, notes: string, markPeak: boolean) {
  let min = Infinity, max = -Infinity;
  let peak = field.peak;
  for (const cell of field.cells) {
    min = Math.min(min, cell.value);
    max = Math.max(max, cell.value);
    if (!field.peak && (!peak || cell.value > peak.value)) {
      peak = { value: cell.value, at: [
        cell.vertices.reduce((sum, p) => sum + p[0], 0) / 3,
        cell.vertices.reduce((sum, p) => sum + p[1], 0) / 3,
      ] };
    }
  }
  if (!field.cells.length) return;
  if (field.peak) max = Math.max(max, field.peak.value);
  const body = boardBody({ ...view, fallbackPoints: field.cells.flatMap(cell => cell.vertices) }, projection => {
    const triangles = field.cells.map(cell => `<polygon points="${cell.vertices.map(projection.xy).join(' ')}" fill="${color(cell.value, min, max)}"/>`).join('');
    return triangles;
  }, projection => markPeak && peak ? marker(projection, peak.at, 'Maximum', '#ffffff', false) : '');
  const peakText = peak && markPeak ? `Max ${format(peak.value)} ${unit} · (${format(peak.at[0])}, ${format(peak.at[1])}) mm` : '';
  return frame(title, status, body + colorKey(min, max, unit, 630, 'Gray copper · unknown') +
    label(928, 641, peakText, { anchor: 'end' }) + label(32, 690, notes), 743);
}

export function marker(projection: Projection, at: Point, name: string, fill: string, showLabel = true) {
  const [x, y] = projection.project(at);
  const anchor = x > 760 ? 'end' : 'start';
  return `<circle cx="${x}" cy="${y}" r="5" fill="${fill}" stroke="#101b2a" stroke-width="2"><title>${escape(name)}</title></circle>` +
    (showLabel ? label(x + (anchor === 'end' ? -8 : 8), y - 8, name, { size: 12, anchor }) : '');
}

export function overviewSvg(status: string, view: BoardView, solution: Entry, dcMesh: Entry, terminals: { at: Point; name: string; color: string }[]) {
  const included = new Set(list(solution.included_domains));
  const idle = new Set(rows(solution.idle_domains).map(d => d.domain_id));
  const foilPlan = record(solution.foilPlan);
  const domain = record(foilPlan.domain ?? solution.domain);
  const provenance = record(solution.provenance);
  const roi = list(record(provenance.roiFallback ?? provenance.electricalRoi ?? domain.roiFallback ?? domain.electricalRoi).roiMmAfter ?? foilPlan.roiMm ?? solution.roiMm ?? domain.roiMm);
  const cascade = record(solution.cascade);
  const fallbackPoints = points(cascade.portPlaneMm);
  const body = boardBody({ ...view, fallbackPoints }, projection => {
    let regions = '';
    for (const mesh of rows(dcMesh.domains).filter(m => m.layer === view.layer)) {
      const fill = included.has(mesh.id) ? '#247871' : idle.has(mesh.id) ? idleColor : unknown;
      regions += list(mesh.triangles).map(triangle => {
        const corners = vertices(triangle, list(mesh.nodes));
        return corners.length === 3 ? `<polygon points="${corners.map(projection.xy).join(' ')}" fill="${fill}"/>` : '';
      }).join('');
    }
    if (view.board) {
      for (const section of rows(cascade.sections)) {
        const source = /^(?:copper\.(fixed|editable)\.(tracks)|(?:pads))\[(\d+)\]$/.exec(String(section.sourcePath));
        if (!source || !source[1]) continue;
        const track = view.board.copper[source[1] as 'fixed' | 'editable'].tracks[Number(source[3])];
        if (track?.layer === view.layer) regions += trackLine(track, projection, '#5cd7f3');
      }
    }
    const [lo, hi] = points(roi);
    if (lo && hi) {
      regions += `<path d="${projection.ring([lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]])}" fill="none" stroke="${accent}" stroke-width="2" stroke-dasharray="7 4"/>`;
    }
    return regions;
  }, projection => terminals.map(t => marker(projection, t.at, t.name, t.color)).join('') +
    (view.board ? '' : fallbackPoints.map((at, i) => marker(projection, at, `Port plane ${i + 1}`, '#7ce3ae')).join('')));
  if (!body) return;
  const notes = included.size || idle.size
    ? [`${included.size} solved domains · ${idle.size} idle (scenario assertion) · ${list(solution.excluded_domains).filter(id => !idle.has(id)).length} excluded`,
        'Teal · solved     Purple · idle     Gray · unknown     Green · source     Orange · load     Pink · return']
    : [cascade.layer ? (view.board ? 'Cyan · channel traces     Green · port signal     Pink · port return     Gray · board copper' : 'Port planes from the saved cascade. Board geometry unavailable.') : 'Gray · captured copper', roi.length ? 'Dashed · reported analysis region. Coverage outside the region is unknown.' : ''];
  return frame(`Scenario overview · ${view.layer}`, status, body +
    notes.map((note, i) => label(32, 640 + i * 23, note)).join(''), 717);
}

