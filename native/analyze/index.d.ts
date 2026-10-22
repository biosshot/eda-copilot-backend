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
  max_edge_mm?: number | null;
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

export interface AnalysisDcRequest {
  context_id: string; board_json: string | null;
  conductor: { length_mm: number; width_mm: number; thickness_mm: number; conductivity_s_m: number; state: string } | null;
  conduction?: AnalysisConductionOptions; snapshot?: boolean;
  terminals: Array<{ id: string; pad_id: string }>;
  elements: Array<{ id: string; kind: 'voltage' | 'current' | 'resistance'; positive: string; negative: string; value: number; role: 'source' | 'load' }>;
  limits?: { max_memory_mib: number; max_unknowns: number; max_seconds: number };
}
export interface AnalysisDcReport {
  operation: 'dc'; status: 'complete' | 'rejected'; method: string; conditional: boolean;
  diagnostics: Array<{ code: string; message: string }>;
  mesh?: AnalysisConductionReport | null; elapsed_ms: number;
  geometry?: Pick<AnalysisGeometryReport, 'outline'|'layers'|'holes'> | null;
  reused_preparation?: boolean; reused_factorization?: boolean;
  standalone?: { resistance_ohm: number; voltage_v: number; current_a: number; loss_w: number; current_density_a_mm2: number; conductor: NonNullable<AnalysisDcRequest['conductor']> };
  included_domains?: string[]; excluded_domains?: string[];
  terminals?: Record<string, { voltage_v: number; reference_node: number; electrical_node: number }>;
  domains?: Array<{ domain_id: string; layer: string; voltage_v: number[]; current_density_a_mm2: number[]; current_density_vector_a_mm2: [number,number][]; loss_w: number[]; reference_node: number }>;
  barrels?: Array<{ hole_id: string; from_layer: string; to_layer: string; current_a: number; drop_v: number; loss_w: number }>;
  elements?: Array<{id: string; kind: string; role: string; positive: string; negative: string; voltage_v: number; delivered_current_a: number; absorbed_power_w: number}>;
  copper_loss_w?: number; barrel_loss_w?: number; load_power_w?: number; source_power_w?: number;
  quality?: { relative_residual: number; current_balance_a: number; power_balance_w: number; normalized_current_balance?: number; normalized_power_balance?: number; voltage_constraint_error_v?: number };
  system?: { unknowns: number; nonzeros: number; memory_plan_bytes: number; memory_metric: string; gauges: number[]; node_count: number; solver: string; refinement_steps: number };
  timings_ms?: Record<string,number>;
}
/** Internal addon object, disposed with its owning worker/scenario. */
export declare class AnalysisDcSession { constructor(); solve(request: AnalysisDcRequest): AnalysisDcReport; clear(): void }
