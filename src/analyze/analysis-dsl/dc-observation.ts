import type { AnalysisDcReport } from '../../../native/analyze/index.js';
import type { RoutingBoardV2 } from '../../types/pcb/routing-board-v2.js';
import type * as T from './spec-doc.js';
import { object } from './descriptions.ts';

export function dcPadId(board: RoutingBoardV2 | undefined, selector: T.Selector): string | undefined {
  const pads = board?.pads.filter(p => selector.kind === 'padId' ? p.id === selector.id : selector.kind === 'pad' && p.component === selector.designator && p.number === selector.number);
  return pads?.length === 1 ? pads[0].id : undefined;
}

/** Integrate the existing piecewise-linear field; never modifies the native request. */
export function dcPadPotentials(board: RoutingBoardV2, r: AnalysisDcReport, validity: T.Validity, diagnostics: readonly T.Diagnostic[]): Record<string, T.DataDescription> {
  const metrics: Record<string, T.DataDescription> = {};
  const mesh = r.mesh, fields = new Map(r.domains?.map(d => [d.domain_id, d]));
  const missing = (reason: string): T.DataDescription => ({ kind: 'scalar', unit: 'V', validity: { status: 'unavailable', reason }, diagnostics });
  for (const pad of board.pads) {
    let result = missing('Pad is outside the computed field or its finite-area coverage is unavailable');
    const active = r.terminals?.[`pad:${pad.id}`];
    if (active) result = { kind: 'scalar', unit: 'V', value: active.voltage_v, validity, diagnostics, data: { referenceNode: active.reference_node, measurement: 'active electrode potential' } };
    else if (mesh && r.status === 'complete') {
      const path = mesh.sources.find(s => s.kind === 'pad' && s.source_id === pad.id)?.path;
      // Same first physical surface as active contacts. Never fall back from a
      // missing/uncomputed first surface to another layer of a plated pad.
      const surface = mesh.profile.layers.find(l => mesh.domains.some(d => d.layer === l.layer && path !== undefined && d.source_paths.includes(path)));
      const domains = mesh.domains.filter(d => d.layer === surface?.layer && path !== undefined && d.source_paths.includes(path));
      let area = 0, integral = 0, reference: number | undefined, complete = domains.length > 0;
      for (const d of domains) {
        const field = fields.get(d.id), terminals = d.terminals.filter(t => t.source.source_id === pad.id);
        if (!field || !d.readiness.mesh_complete || terminals.length !== 1 || !terminals[0].triangles.length || !(terminals[0].area_mm2 > 0)) { complete = false; break; }
        if (reference !== undefined && reference !== field.reference_node) { complete = false; break; }
        reference = field.reference_node;
        let covered = 0;
        for (const i of terminals[0].triangles) {
          const triangle = d.triangles[i];
          if (!triangle) { complete = false; break; }
          const [a, b, c] = triangle.map(n => d.nodes[n]);
          const weight = Math.abs((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2;
          const v = triangle.reduce((sum, n) => sum + field.voltage_v[n], 0) / 3;
          if (!(weight > 0) || !Number.isFinite(v)) { complete = false; break; }
          integral += weight * v; covered += weight;
        }
        // Constrained terminal triangles already exclude bores and cover the
        // prepared copper contact. Partial/missing coverage is never a sample.
        if (Math.abs(covered - terminals[0].area_mm2) > 1e-8 * terminals[0].area_mm2) complete = false;
        area += covered;
      }
      if (complete && area > 0 && Number.isFinite(integral / area) && reference !== undefined) result = {
        kind: 'scalar', unit: 'V', value: integral / area, validity, diagnostics,
        data: { referenceNode: reference, measurement: 'area-weighted linear FEM potential on first physical pad surface, excluding bores', areaMm2: area, layer: surface!.layer },
      };
    }
    metrics[`padVoltage:${pad.id}`] = result;
  }
  return metrics;
}

/** Also works for a port declared after solve, using immutable per-pad samples. */
export function dcObservedVoltage(board: RoutingBoardV2 | undefined, descriptor: unknown, metrics: Readonly<Record<string, T.DataDescription>>): T.DataDescription {
  const missing = (reason: string): T.DataDescription => ({ kind: 'scalar', unit: 'V', validity: { status: 'unavailable', reason }, diagnostics: [] });
  if (!object(descriptor) || !object(descriptor.input)) return missing('No physical pad-pair descriptor for this DC observation');
  if(descriptor.input.layer!==undefined)return missing('Surface-specific port layer is not supported by this DC observation');
  const sides = [descriptor.input.positive, descriptor.input.negative].map(v => Array.isArray(v) ? v : [v]);
  if (sides.some(s => s.length !== 1 || !object(s[0]))) return missing('Aggregated DC voltage observations are unsupported');
  const samples = sides.map(s => {
    const id = dcPadId(board, s[0] as unknown as T.Selector); return id === undefined ? undefined : metrics[`padVoltage:${id}`];
  });
  const [p, n] = samples;
  if (!p || !n || typeof p.value !== 'number' || typeof n.value !== 'number' || p.validity.status === 'unavailable' || n.validity.status === 'unavailable') return missing(samples.find(s => s?.validity.status === 'unavailable')?.validity.reason ?? 'Pad observation is outside the computed field');
  if (!object(p.data) || !object(n.data) || p.data.referenceNode !== n.data.referenceNode) return missing('Pad observations use incompatible mathematical references');
  return { kind: 'scalar', unit: 'V', value: p.value - n.value,
    validity: p.validity.status === 'conditional' ? p.validity : n.validity,
    diagnostics: p.diagnostics, data: { positive: p.data, negative: n.data, measurement: 'passive potential difference; no electrode added' } };
}
