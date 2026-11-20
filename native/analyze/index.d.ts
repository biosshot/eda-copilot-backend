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

export interface AnalysisValidationRequest {
  board_json: string;
  rules: readonly unknown[];
  scope?: readonly unknown[];
  geometry?: AnalysisGeometryOptions;
  budget?: {maxSeconds?: number; maxMemoryMiB?: number};
  limits?: {max_candidates?: number; max_exact_checks?: number; max_measurements?: number; max_report_vertices?: number; max_report_bytes?: number};
}
export interface AnalysisValidationMeasurement {
  id: string; rule: string; code: string; status: 'confirmed'|'uncertain'|'pass';
  region_ids: string[]; witness_region_ids: string[]; source_ids: string[]; nets: string[]; layers: string[];
  actual_mm: number; required_mm: number; error_mm: number; witness: [[number, number], [number, number]]; outside_witness: [number,number]|null; message: string;
}
export interface AnalysisValidationSuccess {
  operation: 'geometry-validation'; status: 'complete'|'partial'; units: 'mm'; coordinates: 'absolute-y-down'; method: string;
  diagnostics: Array<{code: string; message: string; severity: 'warning'|'error'; path?: string; source_id?: string|null}>;
  coverage: Array<{rule: string; required_mm: number; status: 'complete'|'partial'; measurements: number; opens: string}>;
  measurements: AnalysisValidationMeasurement[]; violations: AnalysisValidationMeasurement[]; uncertain_count: number;
  outline: AnalysisPolygon; regions: AnalysisGeometryReport['regions']; graph: unknown;
  fills: AnalysisGeometryReport['fills']; excluded: AnalysisProvenance[];
  statistics: {regions: number; selected_regions: number; outside_regions: number; report_vertices: number; candidate_pairs: number; exact_segment_checks: number; preparation: AnalysisGeometryReport['statistics']};
  boolean_error_bound_mm: number; elapsed_ms: number;
}
export type AnalysisValidationReport = AnalysisValidationSuccess | {operation:'geometry-validation';status:'rejected';diagnostics:AnalysisValidationSuccess['diagnostics'];elapsed_ms:number};
export declare function validateAnalysisGeometry(request: AnalysisValidationRequest): AnalysisValidationReport;
/** Drop operators kept for reuse between bands of one calculation. */
export declare function releaseSharedAnalysisOperators(): void;

export interface RationalComplex { real: number; imaginary: number }
export type RationalMatrix = RationalComplex[][];
export interface RationalFitRequest {
  kind: 'Y' | 'Z' | 'S';
  /** Strictly increasing, >= 0 (0 allowed first), maximum > 0. */
  frequenciesHz: number[];
  /** One n x n (1..8) matrix per frequency. */
  samples: RationalMatrix[];
  /** Pole count; odd counts get one real pole, the rest are conjugate pairs. */
  poles: number;
  iterations?: number;
  weights?: number[];
  /** Default true. */
  fitD?: boolean;
  /** Default true for Y/Z, false for S. */
  fitE?: boolean;
  /** Only perturbs the model when true; the result reports whether passivity was verified. */
  enforcePassivity?: boolean;
  /** Refuse with RATIONAL_FIT_NOT_CONVERGED above this max relative error. */
  maxRelativeError?: number;
  budget?: { maxSeconds?: number | null; maxMemoryMiB?: number | null };
}
/** H(s) = D + s E + sum R_p / (s - p), s = j 2 pi f, SI units; complex poles in conjugate pairs. */
export interface RationalModel { kind: 'Y' | 'Z' | 'S'; ports: number; poles: RationalComplex[]; residues: RationalMatrix[]; d: RationalMatrix; e: RationalMatrix }
export interface RationalPassivity {
  passive: boolean; reason: string | null; tolerance: number; gridPoints: number;
  violations: Array<{ startHz: number; endHz: number; worstHz: number; worstValue: number }>;
  /** Min Hermitian-part eigenvalue (Y/Z) or max singular value (S) over the grid. */
  worstValue: number; worstFrequencyHz: number; asymptoticDValue: number; asymptoticEValue: number;
}
export interface RationalFitResult {
  model: RationalModel; rmsRelativeError: number; maxRelativeError: number; iterations: number;
  passive: boolean; passivityReason: string | null; passivity: RationalPassivity;
  enforcement: null | {
    iterations: number; reached: boolean; reason: string | null;
    maxRelativeErrorBefore: number; maxRelativeErrorAfter: number; rmsRelativeErrorBefore: number; rmsRelativeErrorAfter: number;
  };
}
/** Errors start with RATIONAL_FIT_INPUT or RATIONAL_FIT_NOT_CONVERGED. */
export declare function fitRationalModel(request: RationalFitRequest): RationalFitResult;
export declare function evaluateRationalModel(request: { model: RationalModel; frequenciesHz: number[] }): { samples: RationalMatrix[] };

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

/** Explicit CPU/F64 thermal network transport, owned by the existing scenario worker. */
export interface AnalysisThermalCooling {resistanceKPerW?:number;table?:{deltaTemperatureK:number;heatFlowW:number}[];scale?:unknown}
export interface AnalysisThermalNetwork {nodes:{id:string;boundary?:'environment';heatCapacityJPerK?:number}[];links:{id:string;from:string;to:string;cooling:AnalysisThermalCooling}[];sources:{node:string;powerW:unknown}[]}
export interface AnalysisThermalRequest {contextId:string;network:AnalysisThermalNetwork;mode:'steady'|'time';ambientC:unknown;initialC?:unknown;initial?:unknown;time?:{stopS:number;sampleStepS:number};atS?:number;numerics?:AnalysisElectricalRequest['numerics'];budget?:AnalysisElectricalRequest['budget'];board?:unknown}
export interface AnalysisThermalPreconditionerPlanning {nonzeros:number;fill:number;bytes:number;cacheSlots:number;analysisBytes:number;contributionBytes:number;symbolicIndexBytes:number;numericFactorBytes:number;workspaceBytes:number;solveBufferBytes:number}
export interface AnalysisThermalMemoryPlan {requestBytes:number;physicalSystemBytes:number;transportReportReserveBytes:number;preconditionerAllowanceBytes:number;preconditionerPeakBytes:number;totalPlannedPeakBytes:number;refinementWorkspaceBytes?:number;scope:string}
export interface AnalysisThermalReport {status:'complete'|'rejected';method?:string;diagnostics:{code:string;message:string;severity:'error'|'warning'|'info'}[];axis?:number[];temperaturesC?:Record<string,number[]>;heatFlowW?:Record<string,number[]>;inputPowerW?:number[];outputPowerW?:number[];storedEnergyJ?:number[];inputEnergyJ?:number[];outputEnergyJ?:number[];energyBalance?:number[];quality?:Record<string,number>;counters?:{acceptedSteps:number;rejectedSteps:number;attempts:number;linearIterations:number;preconditionerBuilds:number;preconditionerFallbacks:number;preconditionerPlannedBytes:number;preconditionerInputPairs:number;preconditionerBarrelPairs:number;preconditionerPlanning:AnalysisThermalPreconditionerPlanning};memoryPlan?:AnalysisThermalMemoryPlan;finalState?:unknown;elapsedMs:number;resources?:{peakRssBytes:number;metric:string};spatial?:unknown;refinement?:{converged:boolean;finalRole:string;reason:string;history:unknown[];[key:string]:unknown};coordinateFrame?:{originC:number;nodeOrder:string;temperaturesK:number[][];scope:string}}
export declare function solveAnalysisThermal(request:AnalysisThermalRequest):AnalysisThermalReport;

/** Explicit network DTOs. One owning scenario/process; CPU/F64, no PCB extraction. */
export interface AnalysisElectricalCircuit {
  nodes: string[]; reference: string; ports: {id:string;positive:string;negative:string}[];
  elements: Array<{id:string;kind:string;nodes:[string,string];resistanceOhm?:number;capacitanceF?:number;esrOhm?:number;eslH?:number;inductanceH?:number;seriesResistanceOhm?:number;voltageV?:number;currentA?:number;waveform?:unknown}>;
}
export interface AnalysisElectricalRequest {
  contextId:string; circuit:AnalysisElectricalCircuit; mode:'dc'|'frequency'|'time'; frequencies?:number[];
  acSources?:{elementId:string;values:{real:number;imaginary:number}[]}[];
  time?:{stopS:number;sampleStepS:number}; initial?:unknown;
  numerics?:{relativeTolerance?:number;absoluteTolerance?:number;maxIterations?:number;maxStepS?:number}; budget?:{maxSeconds?:number;maxMemoryMiB?:number};
}
export interface AnalysisElectricalReport {
  status:'complete'|'rejected'; mode?:string; diagnostics:{code:string;message:string;severity:'error'|'warning'|'info'}[];
  axis?:number[]; nodeVoltages?:Record<string,number|number[]|{real:number;imaginary:number}[]>; currents?:Record<string,number|number[]|{real:number;imaginary:number}[]>;
  impedances?:Record<string,{real:number;imaginary:number}[]>; losses?:number|number[];
  quality?:Record<string,number>; counters?:Record<string,number>; finalState?:unknown;
  request?:AnalysisElectricalRequest; elapsedMs:number; excitation?:string; interpolation?:string;
  resources?:{peakRssBytes:number;metric:string};
  system?:{unknowns:number;nodalUnknowns:number;gNonzeros:number;eNonzeros:number;solver:string};
}
export interface AnalysisTransientRequest {
 resource?: AnalysisFrequencyRequest['instances'][number]['model'];
 topology?:Pick<AnalysisFrequencyRequest,'instances'|'connections'|'connectionContacts'|'ports'>;
 drivers:{port:number;resistanceOhm:number;waveform:unknown}[];
 receivers:{port:number;resistanceOhm?:number;capacitanceF:number}[];
 time:{stopS:number;sampleStepS:number};initial?:unknown;
 numerics?:AnalysisElectricalRequest['numerics'];budget?:AnalysisElectricalRequest['budget'];
}
export interface AnalysisTransientReport {
 status:'complete'|'rejected';diagnostics:{code:string;severity:'error'|'warning';message:string}[];
 axis?:number[];portVoltages?:number[][];driverCurrentsA?:number[][];portCurrentsA?:number[][];energyJ?:number[];
 lumpedEnergyJ?:number[];delayEnergyJ?:number[];sourceWorkJ?:number[];resistiveLossJ?:number[];attenuationLossJ?:number[];beNumericalDissipationJ?:number[];
 fit?:unknown;conditional?:boolean;counters?:Record<string,number>;quality?:Record<string,number>;
 physicalRecovery?:unknown;assembledModel?:unknown;conductorPotentialsV?:number[][];conductorChargesC?:number[][];initialConductorChargesC?:number[];finalConductorChargesC?:number[];zeroEnergyGauges?:number;finalState?:unknown;assumptions?:string[];elapsedMs?:number;memoryPlanBytes?:number;
 resources?:{peakRssBytes:number;metric:string};
}
export interface AnalysisLocalDcRequest {
  geometry: unknown;
  drive?: number[];
  loadsOhm?: (number | null)[];
  observablesOnly?: boolean;
  externalPortCount?: number;
  maxMemoryMib?: number;
}
export interface AnalysisLocalDcReport {
  status: 'complete';
  frequencyHz: 0;
  balancedPortResponse?: {
    basisDrivesA: number[][];
    modalImpedanceOhm: number[][];
    portVoltagesVByDrive: (number | null)[][];
    normalizedElectrodeResidualByDrive: number[];
    independentPortDrives: boolean;
    powerScope: string;
    powerByDrive: {
      terminalInputW: number;
      copperSchurLossW: number;
      externalLoadLossW: number;
      packageLossW: number;
      totalLossW: number;
    }[];
  };
  ports: [string,string][];
  driveA: number[];
  loadsOhm: (number | null)[];
  portRealV: (number | null)[];
  floatingPortIndices: number[];
  electrodeNames: string[];
  electrodeVoltagesV: number[];
  electrodeGalvanicComponents: number[];
  electrodeLoadedComponents: number[];
  dcConductanceS: number[][];
  totalLossW: number;
  loadLossW: number;
  drivePowerW: number;
  normalizedElectrodeResidual: number;
  roiMm: [[number,number],[number,number]];
  domainPolygonMm: [number,number][] | null;
  allowInsulatedCopperCuts: boolean;
  sourceMapping: [string,string,string | null,string][];
  cellCurrentA?: number[];
  cellLossW?: number[];
  nodesM?: [number,number,number][];
  copperCells?: [number,number,number,number][];
  sourceIds?: string[][];
  originMm?: [number,number];
  validity: {status:'conditional';domain:{frequencyHz:0};reason:string};
}
export interface AnalysisBoardFoilCheckedRequest {
    frequenciesHz: number[];
    partitionPolicy?: 'automatic-conductive-schur-v1' | 'automatic-hybrid-v1';
    operatorPolicy?: 'skin-volume-v3' | 'hybrid-planar-v3';
    /** Freeze the profiled mesh at this upper frequency for supplemental samples. */
    profileUpperHz?: number;
    referenceOhm?: number[];
    budget: { maxSeconds: number; maxMemoryMiB: number };
    /** Apply `budget` as a real memory ceiling and wall limit in the hybrid path (default: unlimited, as before). */
    enforceBudget?: boolean;
    /** P8: also return the shared reduced coupled-RLC model as `reducedModel` (hybrid path only; large, default off). */
    exportReducedModel?: boolean;
    maxIterations?: number;
    attachments?: { id: string; electrodes: [string, string]; model: AnalysisFrequencyResource }[];
}
export interface AnalysisMagneticCertificate {
    policy: 'certified-adaptive-harmonic-v1';
    coverSignature: string; envelopeSignature: string;
    absoluteActionGoals: number[]; maximumAbsoluteErrorBounds: number[];
    chosenOrderHistogram: number[]; farClusterPairs: number; exactPairs: number;
    zeroBlocks: number; activeEnvelopeTerminals: number; actionWallSeconds: number;
    nearBoundEnabled?: boolean; boundedOmittedBlocks?: number;
    nearBoundPolicy?: 'summed-per-tetra-volume-v1';
    /** Experimental quadrature estimates are separate from the interval/far-tail bounds. */
    nearQuadraturePolicy?: 'axial-tetra-estimated-v1';
    nearQuadratureErrorCertified?: false;
}
export interface AnalysisMagneticBasisCover extends Omit<AnalysisMagneticCertificate, 'policy'> {
    modelRevision: number; rhsColumns: number; visitedNodePairs: number; nearTiles: number;
    batchColumns?: number; chunkCount?: number; coverAggregation?: string; chunkCertificates?: AnalysisMagneticBasisCover[];
    maximumMomentDegreeHistogram: number[]; maximumLocalDegreeHistogram: number[];
    m2lScalarContractionsPerAction?: number; shiftStencilApplicationsPerAction?: number;
}
export interface AnalysisBoardFoilLossGrams {
    copperWPerA2: {real:number;imaginary:number}[][];
    packages: {id:string;powerWPerA2:{real:number;imaginary:number}[][]}[];
}
export interface AnalysisReturnBoundary {
    policy: 'internal-guard-flux-v1';
    boundsMm: [[number, number, number], [number, number, number]];
    conductiveA: number[];
    displacementA: number[];
    portCurrentA: number;
    unresolvedA: number;
    relative: number;
    absoluteFloorA: number;
    threshold: number;
    passed: boolean;
}
export interface AnalysisBoardFoilCheckedReport {
    status: 'complete';
    coverage: 'unverified';
    representation: 'S';
    operator?: 'compact-planar-homogeneous-prototype' | 'compact-planar-layered-prototype';
    /** Open-port reduced model behind the S samples (only with `exportReducedModel`). R/L carry the skin profile frozen at `profileUpperHz`. */
    reducedModel?: {
        model: {format:'coupled-rlc';referenceTopology:string;potentialNodes:string[];currentDofs:string[];incidence:[number,number,number][];portIncidence:[number,number,number][];resistanceOhm:[number,number,number][];inductanceH:[number,number,number][];capacitanceF:[number,number,number][];conservedPotentialModes:number[][];structuralGaugeModes?:number[][];portTerminals?:unknown[]};
        portCount: number; trainedAtHz: number[]; profileUpperHz: number; scope: 'compact-discrete'; physicalAccuracy: 'unverified';
    };
    modelRegimeAudit?: {status:'pass'|'fail';scope:'discrete-model-only';physicalCoverage:'unverified';probeHz:number;dcPortVoltagesV:number[][];acPortVoltagesV:{real:number[];imaginary:number[]}[];timeDiagnostics:{relativePeakErrors:number[];timeQuality:{relativeResidual:number;maximumLocalErrorRatio:number;energyBalanceJ:number;energyNormalizationJ:number;maximumAcceptedStepS:number}[]}[]};
    frequenciesHz: number[];
    referenceOhm: number[];
    ports: [string, string][];
    parameters: { real: number; imaginary: number }[][][];
    samples: {
        frequencyHz: number;
        lossGrams?: AnalysisBoardFoilLossGrams;
        columns: { input: number; kclRelative: number; kvlRelative: number; kvlUncertaintyRelative?: number; magneticCertificate?: AnalysisMagneticCertificate; compactKvlUncertaintyRelative?: number; compactMagneticCertificate?: { policy: 'compact-mixed-near-far'; nearQuadratureErrorCertified: false }; copperLossW: number; powerBalanceErrorW: number; returnBoundary?: AnalysisReturnBoundary; spatialRecovery?: { status: "recovered-discrete"; physicalAccuracy: "unverified"; driveConvention: string; copperLossW: number; summary?: true; totalLossW?: number; supportCount?: number; supportCountByKind?: { foil: number; barrel: number; tetrahedron: number }; supports: (({ kind: "foil"; layer: string; triangle: number; zM: [number, number]; xyM: [[number, number], [number, number], [number, number]] } | { kind: "barrel"; barrel: string | number; triangle: number; zM: [number, number]; xyM: [[number, number], [number, number], [number, number]] } | { kind: "tetrahedron"; verticesM: [[number, number, number], [number, number, number], [number, number, number], [number, number, number]] }) & { volumeM3: number; currentDensityRealAPerM2: [number, number, number]; currentDensityImaginaryAPerM2: [number, number, number]; lossW: number })[] }; packages?: { id: string; currentA: { real: number; imaginary: number }; lossW: number }[] }[];
    }[];
    enrichment: {
        iteration: number; frequencyHz: number; input: number; potentialModes: number; currentModes: number;
        maxKclRelative: number; maxKvlRelative: number;
    }[];
    quality: { maxKclRelative: number; maxKvlRelative: number; maxColumnPowerBalanceErrorW: number; maxFullKvlUncertaintyRelative?: number; maxCompactKvlUncertaintyRelative?: number; scope?: 'compact-discrete'; physicalError?: 'unverified' };
    conductiveCorrection?: {
        method: 'sparse owner interiors and shared conductive trace Schur';
        ownerInteriorDofs: number[]; freeSharedTraceDofs: number; traceUpperEntries: number;
        ownedPlannedBytes: number; maxOriginalFreeResidualRelative: number; scope: string;
    };
    provenance: {
        method: string;
        compactOperator?: { version: 'compact-planar-v1'; basis: 'full-mixed' | 'dc-harmonic-only' | 'adaptive-mixed-closure'; nearQuadratureEstimatedTolerance?: number; electricOperator: 'homogeneous-charge-gram' | 'layered-spatial-field-mesh'; approximationError: 'unverified'; scope: 'compact-discrete'; originalVolumeActions: 0; foilSupports: number; localTetraSupports: number; potentialCoordinates: number; currentCoordinates: number; planarMaxOrder: number; planarMaxRelativeChange: number; mixedMaxRelativeChange: number; tetraSelfMaxRelativeChange: number; tetraPairAdaptiveTolerance: number; physicalAccuracy: 'unverified'; foilSheet?: { approximation: true; maxThicknessOverSkin: number; estimatedResistanceError: number; bandTopHz: number; criterion: string; warning: string; resistanceScaled?: boolean; halfSheets?: boolean; halfSheetLayers?: number | null; resistanceMultiplierMax?: number; resistanceMultiplierMin?: number; multiplierTag?: string | null; status?: 'approximation'; statusNote?: string | null } };
        magneticOperator?: {
            scope: string; maxFullKvlUncertaintyRelative: number;
        } & (
            | { policy: 'exact-streamed-v1'; method: 'unchanged direct tetrahedral pairs' }
            | { policy: 'certified-adaptive-harmonic-v1'; method: 'symmetric adaptive Newton expansion';
                representationVersion: 3; harmonicBasis: 'laplace-quotient-z01';
                minimumExpansionOrder: 0; admissibility: 'strict-disjoint-balls-certified-tail';
                momentCoefficientCount: number; m2lCouplings: number; shiftStencilTerms: number;
                axisSerial: true; leafTetrahedra: number;
                interactionStorage: 'immutable-adaptive-dual-tree-v1'; traversalStorageBytes: number;
                interactionStackSlots: number; interactionLevels: number; interactionReplaysPerAction: 3;
                expansionOrder: number; separationRatioMaximum: number; treeNodes: number;
                plannedBytes: number; treeSignature: string; geometrySignature: string;
                basisCover: AnalysisMagneticBasisCover }
        );
        /** P7_HIGHBAND_CUTS=1 only: cut-plane port electrodes admitted in hybrid-planar-v3 (approximation, unverified). */
        cutPorts?: { count: number; names: string[]; areasM2: Record<string, number>; model: 'equipotential-face'; status: 'approximation'; statusNote: string; facePotentialSpread: null };
        /** P7_BAND_SCREEN=<kD> with P7_HIGHBAND_CUTS=1 only: the quasistatic size screen used kD (0 < kD <= 0.5) instead of 0.1 (approximation, unverified). */
        bandScreen?: { kD: number; thresholdHz: number; sizeM: number; epsilonR: number; maximumHz: number; electricalSize: number; status: 'approximation'; statusNote: string };
        partitionPolicy?: 'automatic-conductive-schur-v1' | 'automatic-hybrid-v1';
        hybridAssembly?: { policy: 'automatic-hybrid-v1'; method: 'global-compact-mixed'; potentialCoordinates: number; currentCoordinates: number; physicalAccuracy: 'unverified' };
        automaticPartition?: {
            policy: 'automatic-conductive-schur-v1'; status: 'selected' | 'not-applicable';
            reason: string; algorithm: 'longest-extent-midpoint-v1'; meshOwnershipSignature: string;
            owners: number; originalCopperTetrahedra: number; crossOwnerFaces: number;
            freeSharedTraceDofs: number; ownerInteriorDofs: number[];
        };
        operatorPolicy?: 'skin-volume-v3' | 'hybrid-planar-v3';
        copperSkinMesh?: {
            version: 3;
            requestedUpperHz: number;
            trainingUpperBoundHz: number;
            effectiveUpperHz: number;
            skinDepthDivisor: 3;
            currentAndElectricShareMesh: true;
            layers: { layer: string; skinDepthM: number; maximumStepM: number; slabs: number }[];
            roundBarrels: { id: string; skinDepthM: number; maximumRadialStepM: number; shells: number }[];
            barrelSheet?: { approximation: true; criterion: string; warning: string; resistanceScaled: boolean; maxThicknessOverSkin: number; estimatedResistanceError: number; resistanceMultiplierMax?: number | null; multiplierTag?: string | null; status?: 'approximation'; statusNote?: string | null };
            scope: string;
        };
        barrelIds: string[];
        attachments?: { id: string; electrodes: [string, string] }[];
        sourceMapping: [regionId: string, sourcePath: string, net: string | null, layer: string][];
        electrodeAreasM2: Record<string, number>;
        maxCopperVolumeChange: number;
        prisms: number;
        retainedPotentials: number;
        currentModes: number;
        plannedBytes: number;
        maxSeconds?: number;
        maxMemoryMiB?: number;
        spatialCoverage: 'unverified';
        bandCoverage: 'unverified';
        driveConvention: string;
    } & (
        | { modelKind: 'foil'; maxMagneticReciprocity: number; maxMagneticRefinement: number; maxElectricReciprocity: number }
        | {
            modelKind: 'mixed'; nativeCopperTetrahedra: number; volumeTetrahedra: number;
            fullPotentialCount: number; fullCurrentCount: number; magneticQuadratureCoverage: 'unverified';
        }
    );
}
export declare class AnalysisElectricalSession {constructor();sliceBoardSection(boardJson:string,stack:unknown,slice:unknown):any;solveBoardFoilChecked(boardJson:string,geometry:unknown,request:AnalysisBoardFoilCheckedRequest):AnalysisBoardFoilCheckedReport;prepareLocal(boardJson:string,request:unknown,remainingSeconds?:number,magneticPatchLevel?:1|2,frequencyAudit?:unknown):any;solveLocalDc(boardJson:string,request:AnalysisLocalDcRequest,remainingSeconds?:number):AnalysisLocalDcReport;solveLocal(request:{prepared:unknown;frequenciesHz:number[];drive?:number[];loadsOhm?:(number|null)[]},remainingSeconds?:number,observablesOnly?:boolean):any;solve(request:AnalysisElectricalRequest):AnalysisElectricalReport; solveFrequency(request:AnalysisFrequencyRequest):AnalysisFrequencyReport; solveChannelMagnetic(request:unknown):any; solveTransient(request:AnalysisTransientRequest):AnalysisTransientReport; solveCrossSection(request:AnalysisCrossSectionRequest):AnalysisCrossSectionReport;propagateCrossSection(request:AnalysisCrossSectionRequest,prepared:unknown):AnalysisCrossSectionReport;clear():void}

/** Bounded common-return frequency multiports. Internal addon transport. */
export interface AnalysisFrequencyResource {id:string;kind:string;ports:string[];data:unknown;source?:string;validity?:unknown;analyses?:string[]}
export interface AnalysisFrequencyRequest {
 instances:{id:string;model:AnalysisFrequencyResource;ports:string[]}[];
 connectionContacts?:('both'|'positive'|'negative'|'positive-negative'|'negative-positive')[];connections:[number,number][];ports:{name:string;index:number}[];frequencies:number[];
 representation:'S'|'Z'|'Y';referenceOhm?:number[];budget?:AnalysisElectricalRequest['budget'];
 instanceInductanceH?:{a:string;b:string;localA:number;localB:number;valueH:number}[];
 /** Matched S only, with physical coupled-RLC instance mappings. */
 recoverInstanceLoss?:boolean;
}
export interface AnalysisLoadedInstanceLoss {
 instanceIds:string[];
 samples:{frequencyHz:number;jouleWPerA2:number[][][];oppositeDriveW:number[]}[];
 convention:string;
}
export interface AnalysisFrequencyReport {
 loadedInstanceLoss?:AnalysisLoadedInstanceLoss;
 status:'complete'|'rejected';diagnostics:{code:string;message:string;severity:'error'|'warning'|'info'}[];elapsedMs:number;
 frequenciesHz?:number[];ports?:{name:string;index:number}[];representation?:'S'|'Z'|'Y';referenceOhm?:number[]|null;
 physicalRecovery?:unknown;assembledModel?:unknown;validityLimitations?:string;parameters?:{real:number;imaginary:number}[][][];nature?:'passive'|'active'|'unspecified';conditional?:boolean;
 bandHz?:number[];interpolation?:unknown[];provenance?:unknown[];quality?:{maxResidual:number;maxCondition:number;solves:number};
 resources?:{peakRssBytes:number;metric:string};
}

/** Internal one-shot PEC or bounded weak-loss extraction; existing process/session. */
export interface AnalysisCrossSectionRequest {boardIdentity?:string;line:unknown;frequencies:number[];mesh?:{maxNodes?:number;maxTriangles?:number;maxRefinements?:number;maxEdgeMm?:number;domainScale?:number};budget?:AnalysisElectricalRequest['budget']}
export interface AnalysisCrossSectionReport {
 status:'complete'|'rejected';diagnostics:{code:string;message:string;severity:'error'|'warning'|'info'}[];elapsedMs:number;
 assumptions?:string[];parameters?:{modes:{name:string;impedanceOhm:number;delaySPerM:number}[];differentialOhm?:number|null;commonOhm?:number|null;[key:string]:unknown};
 network?:{ports:string[];parameters:{real:number;imaginary:number}[][][];referenceOhm:number;maxUnitarityError?:number;maxPassivityExcess?:number;maxReciprocityError?:number;maxPowerBalanceResidual?:number;powerBalances?:unknown[];phaseConvention:string}|null;
 resources?:{peakRssBytes:number;metric:string};applicability?:unknown;
 [key:string]:unknown;
}
