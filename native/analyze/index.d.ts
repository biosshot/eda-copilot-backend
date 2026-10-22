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
  holes: Array<{id: string; source: AnalysisProvenance; layers: string[]; plated: boolean; span_confirmed: boolean; polygons: AnalysisPolygon[]}>;
  layers: Array<{layer: string; union_area_mm2: number; copper: AnalysisPolygon[]; regions: number}>;
  contacts: Array<{a: string; b: string; kind: 'area' | 'boundary' | 'point' | 'uncertain' | 'barrel'; reliable: boolean; overlap_mm2: number; drill_id: string | null}>;
  islands: Array<{id: string; region_ids: string[]; nets: string[]; has_unknown_net: boolean}>;
  net_islands: Record<string, string[]>;
  findings: Array<{code: string; region_ids: string[]; source_ids: string[]; nets: string[]; message: string}>;
  fills: Array<{source: AnalysisProvenance; layer: string; status: string; freshness: string | null; polygons: number}>;
  sources: AnalysisProvenance[]; excluded: AnalysisProvenance[];
  statistics: { source_counts: Record<string, number>; accepted_sources: number; excluded_sources: number; regions: number; islands: number;
    candidate_pairs: number; narrow_phase_pairs: number; generated_vertices: number; shorts: number };
}
export declare function prepareAnalysisGeometry(json: string, options?: AnalysisGeometryOptions): AnalysisGeometryReport;

export interface AnalysisMaterial { conductivity_s_m: number; state: string }
export type AnalysisStackEntry = { kind: 'copper'; layer: string; thickness_mm: number }
  | { kind: 'dielectric'; name: string; thickness_mm: number };
export interface AnalysisPhysicalProfile {
  stackup?: AnalysisStackEntry[];
  materials?: Record<string, AnalysisMaterial>;
  barrel?: { plating_thickness_mm?: number; material?: AnalysisMaterial; diameter_convention?: 'finished_hole' | 'before_plating' };
}
export interface AnalysisMeshOptions {
  max_area_mm2?: number; min_angle_deg?: number; max_nodes?: number; max_triangles?: number; max_refinement_steps?: number;
}
export interface AnalysisConductionOptions { geometry?: AnalysisGeometryOptions; mesh?: AnalysisMeshOptions; profile?: AnalysisPhysicalProfile }
export interface AnalysisConductionDomain {
  id: string; layer: string; region_ids: string[]; source_paths: string[];
  readiness: { mesh_complete: boolean; topology_confirmed: boolean; copper_complete: boolean; physics_complete: boolean; dc_ready: boolean; reasons: string[] };
  nodes: [number, number][]; triangles: [number, number, number][];
  boundaries: Array<{ nodes: [number, number]; kind: string; source_paths: string[] }>;
  terminals: Array<{source: AnalysisProvenance; region_ids: string[]; triangles: number[]; boundary_edges: number[]; area_mm2: number}>;
  area_mm2: number; mesh_area_mm2: number; min_angle_deg: number; max_area_mm2: number; refinement_steps: number; terminal_alignment_mm: number; sheet_conductance_s: number | null;
}
export interface AnalysisConductionReport {
  operation: 'prepare-conduction'; status: 'complete' | 'partial' | 'invalid'; units: 'mm; coefficients SI'; coordinates: 'absolute-y-down';
  options: { geometry: Required<AnalysisGeometryOptions>; mesh: Required<AnalysisMeshOptions>;
    profile: {stackup: AnalysisStackEntry[] | null; materials: Record<string, AnalysisMaterial>;
      barrel: {plating_thickness_mm: number | null; material: AnalysisMaterial | null; diameter_convention: 'finished_hole' | 'before_plating' | null} | null} | null };
  approximation_mm: number; boolean_error_bound_mm: number;
  profile: { layers: Array<{layer: string; thickness_mm: number | null; z_mm: number | null; geometry_origin: string | null; material: AnalysisMaterial | null; material_origin: string | null; sheet_conductance_s: number | null}>;
    barrel: {plating_thickness_mm: number | null; plating_origin: string | null; material: AnalysisMaterial | null; material_origin: string | null; diameter_convention: 'finished_hole' | 'before_plating' | null; diameter_convention_origin: string | null} };
  domains: AnalysisConductionDomain[];
  barrels: Array<{source: AnalysisProvenance; hole_id: string; diameter_mm: number; ready: boolean; reasons: string[];
    /** Geometry-confirmed regions retained independently of mesh success; ready covers mapping, mesh and topology, not material physics. */
    expected_contacts: Array<{region_id: string; source_path: string; source_id: string | null; layer: string; domain_ids: string[]; ready: boolean; reasons: string[]}>;
    contacts: Array<{domain: string; layer: string; boundary_edges: number[]}>;
    segments: Array<{from_layer: string; to_layer: string; length_m: number | null; cross_section_m2: number | null; conductance_s: number | null}>}>;
  diagnostics: AnalysisGeometryReport['diagnostics']; import_diagnostics: AnalysisImportReport['diagnostics'];
  sources: AnalysisProvenance[]; fills: AnalysisGeometryReport['fills']; excluded: AnalysisProvenance[]; findings: AnalysisGeometryReport['findings'];
  counts: {domains: number; built_domains: number; ready_domains: number; nodes: number; triangles: number; terminals: number; barrels: number};
  timings_ms: Record<string, number>; serialized_bytes: number;
}
export declare function prepareAnalysisConduction(json: string, options?: AnalysisConductionOptions): AnalysisConductionReport;
