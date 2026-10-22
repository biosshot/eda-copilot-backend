// Analysis module declarations, aggregated by native/node-addon/index.d.cts.
export interface AnalysisImportReport {
  imported: boolean;
  accepted: {
    layers: number; nets: number; components: number; pads: number; tracks: number;
    vias: number; zones: number; keepouts: number; drills: number;
    available_fills: number; unavailable_fills: number;
  };
  diagnostics: Array<{
    severity: 'error' | 'missing' | 'limitation';
    code: string; path: string; source_id: string | null; message: string;
  }>;
}
export declare function importAnalysisBoard(json: string): AnalysisImportReport;

export interface AnalysisGeometryOptions {
  max_deviation_mm?: number;
  max_vertices_per_curve?: number;
  max_generated_vertices?: number;
}
export interface AnalysisProvenance {
  kind: string; path: string; source_id: string | null; component_id: string | null; pad_number: string | null;
  native_fill_ids: string[]; net: string | null; ownership: string | null;
}
export interface AnalysisPolygon { outer: [number, number][]; holes: [number, number][][] }
export interface AnalysisGeometryReport {
  operation: 'prepare-geometry'; status: 'complete' | 'partial' | 'invalid'; units: 'mm'; coordinates: 'absolute-y-down';
  options: Required<AnalysisGeometryOptions>; boolean_error_bound_mm: number;
  diagnostics: Array<{code: string; path: string; source_id: string | null; message: string; category: string}>;
  import_diagnostics: AnalysisImportReport['diagnostics'];
  outline: AnalysisPolygon | null;
  regions: Array<{ id: string; layer: string; source: AnalysisProvenance; polygon: AnalysisPolygon; area_mm2: number; approximation_mm: number }>;
  /** Conservative graph nodes; regions/layers above remain nominal snapshots. */
  reliable_regions: AnalysisGeometryReport['regions'];
  possible_regions: AnalysisGeometryReport['regions'];
  possible_islands: AnalysisGeometryReport['islands'];
  possible_contacts: AnalysisGeometryReport['contacts'];
  holes: Array<{id: string; source: AnalysisProvenance; layers: string[]; plated: boolean; span_confirmed: boolean; polygons: AnalysisPolygon[]}>;
  layers: Array<{layer: string; union_area_mm2: number; copper: AnalysisPolygon[]; regions: number}>;
  contacts: Array<{a: string; b: string; kind: 'area' | 'boundary' | 'point' | 'uncertain' | 'barrel'; reliable: boolean; overlap_mm2: number; drill_id: string | null}>;
  islands: Array<{id: string; region_ids: string[]; nets: string[]; has_unknown_net: boolean}>;
  net_islands: Record<string, string[]>;
  findings: Array<{code: string; region_ids: string[]; source_ids: string[]; nets: string[]; message: string}>;
  fills: Array<{source: AnalysisProvenance; layer: string; status: string; freshness: string | null; polygons: number}>;
  sources: AnalysisProvenance[]; excluded: AnalysisProvenance[];
  statistics: { source_counts: Record<string, number>; accepted_sources: number; excluded_sources: number; regions: number; islands: number;
    reliable_regions: number; possible_regions: number; possible_islands: number; uncertain_components: number;
    candidate_pairs: number; narrow_phase_pairs: number; generated_vertices: number; shorts: number };
}
export declare function prepareAnalysisGeometry(json: string, options?: AnalysisGeometryOptions): AnalysisGeometryReport;
