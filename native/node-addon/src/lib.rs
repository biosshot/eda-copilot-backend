//! The only Node boundary. Library calls retain their own numeric environments.
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::Value;

#[napi]
pub fn validate_analysis_geometry(request: Value) -> Value {
    pcb_analyze::validation::validate_json(request)
}

fn packer_error(error: pcb_board_packer::Error) -> Error {
    Error::new(
        match error.kind {
            pcb_board_packer::ErrorKind::InvalidInput => Status::InvalidArg,
            pcb_board_packer::ErrorKind::Failure => Status::GenericFailure,
        },
        error.message,
    )
}

#[napi]
pub fn placement_geometry_epsilon() -> f32 {
    pcb_board_packer::placement_geometry_epsilon()
}

#[napi]
pub fn numeric_contract() -> String {
    pcb_board_packer::numeric_contract()
}

#[napi]
pub fn evaluate_block_quality(problem: Value) -> Result<Value> {
    pcb_board_packer::evaluate_block_quality(problem).map_err(packer_error)
}

#[napi]
pub fn evaluate_board_quality(problem: Value) -> Result<Value> {
    pcb_board_packer::evaluate_board_quality(problem).map_err(packer_error)
}

#[napi]
pub fn score_board_spacing(problem: Value) -> Result<f32> {
    pcb_board_packer::score_board_spacing(problem).map_err(packer_error)
}

#[napi]
pub fn score_board_alignment(primitives: Value, policy: Value) -> Result<f32> {
    pcb_board_packer::score_board_alignment(primitives, policy).map_err(packer_error)
}

#[napi]
pub fn placement_score_sum(scores: Value) -> Result<f32> {
    pcb_board_packer::placement_score_sum(scores).map_err(packer_error)
}

#[napi]
pub fn placement_net_weights(names: Vec<String>, board: bool) -> Vec<f32> {
    pcb_board_packer::placement_net_weights(names, board)
}

#[napi]
pub fn placement_net_class(name: String) -> Value {
    pcb_board_packer::placement_net_class(name)
}

#[napi]
pub fn placement_orientation_weight(problem: Value) -> Result<f32> {
    pcb_board_packer::placement_orientation_weight(problem).map_err(packer_error)
}

#[napi]
pub fn placement_layer_penalties(terms: Value) -> Result<Vec<f32>> {
    pcb_board_packer::placement_layer_penalties(terms).map_err(packer_error)
}

#[napi]
pub fn placement_score_contract_version() -> u32 {
    pcb_board_packer::placement_score_contract_version()
}

#[napi]
pub fn placement_priority_weight(priority: String) -> f64 {
    pcb_board_packer::placement_priority_weight(priority)
}

#[napi]
pub fn placement_relation_profile(request: Value) -> Result<Value> {
    pcb_board_packer::placement_relation_profile(request).map_err(packer_error)
}

#[napi]
pub fn placement_spacing_policy(measurements: Value) -> Result<Value> {
    pcb_board_packer::placement_spacing_policy(measurements).map_err(packer_error)
}

#[napi]
pub fn placement_alignment_policy(pair_count: u32, normalize: bool) -> Value {
    pcb_board_packer::placement_alignment_policy(pair_count, normalize)
}

#[napi]
pub fn placement_block_similarity(problem: Value) -> Result<f32> {
    pcb_board_packer::placement_block_similarity(problem).map_err(packer_error)
}

#[napi]
pub fn evaluate_island_quality(problem: Value) -> Result<Value> {
    pcb_board_packer::evaluate_island_quality(problem).map_err(packer_error)
}

#[napi]
pub fn score_island_quality(problem: Value) -> Result<f32> {
    pcb_board_packer::score_island_quality(problem).map_err(packer_error)
}

#[napi]
pub fn score_placement_orientation(problem: Value) -> Result<f32> {
    pcb_board_packer::score_placement_orientation(problem).map_err(packer_error)
}

#[napi]
pub fn compare_block_quality(a: Value, b: Value, kind: String) -> Result<bool> {
    pcb_board_packer::compare_block_quality(a, b, kind).map_err(packer_error)
}

#[napi]
pub fn board_electrical_regression(before: Value, after: Value) -> Result<Option<String>> {
    pcb_board_packer::board_electrical_regression(before, after).map_err(packer_error)
}

#[napi]
pub fn post_place_net_lengths(problem: Value) -> Result<Vec<f32>> {
    pcb_board_packer::post_place_net_lengths(problem).map_err(packer_error)
}

#[cfg(all(feature = "gpu", feature = "placement-bench"))]
#[napi]
pub fn block_gpu_probe(values: Vec<f64>, inject_panic: bool) -> Result<Value> {
    // N-API accepts JS numbers as f64; this opt-in probe's Rust input is F32.
    pcb_board_packer::block_gpu_probe(values.into_iter().map(|v| v as f32).collect(), inject_panic)
        .map_err(packer_error)
}

#[napi]
pub fn contract_version() -> u32 {
    pcb_board_packer::contract_version()
}

#[napi]
pub fn solve_board_packed(problem: Value) -> Result<Value> {
    pcb_board_packer::solve_board_packed(problem).map_err(packer_error)
}

#[napi]
pub fn score_fast_route_layout(problem: Value) -> Result<f32> {
    pcb_board_packer::score_fast_route_layout(problem).map_err(packer_error)
}

#[napi]
pub fn block_contract_version() -> u32 {
    pcb_board_packer::block_contract_version()
}

#[napi]
pub fn solve_block_primitives(problem: Value) -> Result<Value> {
    pcb_board_packer::solve_block_primitives(problem).map_err(packer_error)
}

#[napi]
pub fn passive_island_contract_version() -> u32 {
    pcb_board_packer::passive_island_contract_version()
}

#[napi]
pub fn block_search_stages_version() -> u32 {
    pcb_board_packer::block_search_stages_version()
}

#[napi]
pub fn solve_block_primitives_batch(problems: Vec<Value>, threads: u32) -> Result<Vec<Value>> {
    pcb_board_packer::solve_block_primitives_batch(problems, threads).map_err(packer_error)
}

#[napi]
pub fn solve_passive_net_island(problem: Value) -> Result<Value> {
    pcb_board_packer::solve_passive_net_island(problem).map_err(packer_error)
}

#[napi]
pub fn post_place_score_contract_version() -> u32 {
    pcb_board_packer::post_place_score_contract_version()
}

#[napi]
pub fn score_post_place(problem: Value) -> Result<f32> {
    pcb_board_packer::score_post_place(problem).map_err(packer_error)
}

#[napi]
pub fn signal_path_contract_version() -> u32 {
    pcb_board_packer::signal_path_contract_version()
}

#[napi]
pub fn evaluate_signal_path(problem: Value) -> Result<Value> {
    pcb_board_packer::evaluate_signal_path(problem).map_err(packer_error)
}

#[napi]
pub fn signal_path_bridge_deltas(problem: Value) -> Result<Value> {
    pcb_board_packer::signal_path_bridge_deltas(problem).map_err(packer_error)
}

#[napi]
pub fn post_place_refine_contract_version() -> u32 {
    pcb_board_packer::post_place_refine_contract_version()
}

#[napi]
pub fn validate_placement(problem: Value, scope: Option<Vec<String>>) -> Result<bool> {
    pcb_board_packer::validate_placement(problem, scope).map_err(packer_error)
}

#[napi]
pub fn validate_placement_change(problem: Value, placements: Value) -> Result<bool> {
    pcb_board_packer::validate_placement_change(problem, placements).map_err(packer_error)
}

#[napi]
pub fn refine_post_placement(problem: Value) -> Result<Value> {
    pcb_board_packer::refine_post_placement(problem).map_err(packer_error)
}

#[napi]
pub fn placement_access_profile(request: Value) -> Result<Value> {
    pcb_board_packer::placement_access_profile(request).map_err(packer_error)
}

/// Structural import only; no packer float guard or GPU entry point.
#[napi]
pub fn import_analysis_board(json: String) -> Value {
    let report = pcb_analyze::import_json(&json);
    serde_json::json!({"imported":report.board.is_some(),"accepted":report.accepted,"diagnostics":report.diagnostics})
}

/// F64 physical copper preparation; no placement float guard or GPU context.
#[napi]
pub fn prepare_analysis_geometry(json: String, options: Option<Value>) -> Result<Value> {
    let options = options.map(serde_json::from_value::<pcb_analyze::geometry::GeometryOptions>)
        .transpose().map_err(|e| napi::Error::from_reason(e.to_string()))?.unwrap_or_default();
    let report = pcb_analyze::geometry::prepare_json(&json, options);
    serde_json::to_value(report).map_err(|e| napi::Error::from_reason(e.to_string()))
}

/// CPU/F64 layered discretization; no placement guard, GPU or electrical solve.
#[napi]
pub fn prepare_analysis_conduction(json: String, options: Option<Value>) -> Result<Value> {
    let options = options.map(serde_json::from_value::<pcb_analyze::conduction::ConductionOptions>)
        .transpose().map_err(|e| napi::Error::from_reason(e.to_string()))?.unwrap_or_default();
    serde_json::to_value(pcb_analyze::conduction::prepare_json(&json, options))
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}

/// Scenario-owned preparation and sparse factorization. No GPU or process-global cache.
#[napi]
pub struct AnalysisDcSession { inner: pcb_analyze::dc::Session }
#[napi]
impl AnalysisDcSession {
    #[napi(constructor)]
    pub fn new() -> Self {
        // Preserve pre-Rayon serial defaults; this allocates no pool or fields.
        pcb_analyze::extraction::local::parallel::initialize();
        Self { inner: Default::default() }
    }
    #[napi]
    pub fn solve(&mut self, request: Value) -> Result<Value> {
        let request=serde_json::from_value::<pcb_analyze::dc::Request>(request)
            .map_err(|e| napi::Error::from_reason(format!("Invalid DC request: {e}")))?;
        Ok(self.inner.solve(request))
    }
    #[napi]
    pub fn clear(&mut self) { self.inner=Default::default(); }
}

/// Explicit linear RLC network, CPU/F64, scenario-owned sparse factors.
#[napi]
pub struct AnalysisElectricalSession { inner: pcb_analyze::electrical::Session,local_response:pcb_analyze::extraction::local::board_response::Session }
#[napi]
impl AnalysisElectricalSession {
    #[napi(constructor)]
    pub fn new() -> Self {
        // Local sparse FEM opts into its own bounded pool explicitly.
        pcb_analyze::extraction::local::parallel::initialize();
        Self { inner: Default::default(),local_response:Default::default() }
    }
    #[napi]
    pub fn solve(&mut self, request: Value) -> Result<Value> {
        let request=serde_json::from_value::<pcb_analyze::electrical::Request>(request)
            .map_err(|e| napi::Error::from_reason(format!("Invalid electrical request: {e}")))?;
        Ok(self.inner.solve(request))
    }
    #[napi]
    pub fn prepare_local(&mut self, board_json:String, request:Value, remaining_seconds:Option<f64>,magnetic_patch_level:Option<u32>)->Result<Value> {
        let magnetic_config=match magnetic_patch_level{None=>[6,8,16,3],Some(1)=>[4,6,16,1],Some(2)=>[6,8,16,1],_=>return Err(napi::Error::from_reason("LOCAL_SPATIAL_INPUT: unsupported bounded patch level"))};
        let started=std::time::Instant::now();self.local_response.clear();
        use pcb_analyze::extraction::local::{board_mesh,board_prepare};
        let board=pcb_analyze::import_json(&board_json).board.ok_or_else(||napi::Error::from_reason("Local board import rejected"))?;
        let request:board_mesh::Request=serde_json::from_value(request).map_err(|e|napi::Error::from_reason(e.to_string()))?;
        board_prepare::run_with_budget(&board,&request,board_prepare::Options{developer_checkpoint:None,mode:"prepare",magnetic_config,field_request:None,cached_field:None,frequencies_hz:&[]},remaining_seconds.unwrap_or(300.)-started.elapsed().as_secs_f64()).map_err(napi::Error::from_reason)
    }
    #[napi]
    pub fn solve_local(&mut self, request:Value,remaining_seconds:Option<f64>,observables_only:Option<bool>)->Result<Value> {
        let request=serde_json::from_value(request).map_err(|e|napi::Error::from_reason(e.to_string()))?;
        self.local_response.solve_with_options(request,remaining_seconds.unwrap_or(30.),observables_only.unwrap_or(false)).map_err(napi::Error::from_reason)
    }
    #[napi]
    pub fn solve_frequency(&mut self, request: Value) -> Result<Value> {
        let request=serde_json::from_value::<pcb_analyze::electrical::frequency::Request>(request)
            .map_err(|e| napi::Error::from_reason(format!("Invalid frequency request: {e}")))?;
        Ok(pcb_analyze::electrical::frequency::run(request))
    }
    #[napi]
    pub fn solve_transient(&mut self, request: Value) -> Result<Value> {
        let request=serde_json::from_value::<pcb_analyze::electrical::coupled::time::Request>(request)
            .map_err(|e| napi::Error::from_reason(format!("Invalid transient request: {e}")))?;
        Ok(pcb_analyze::electrical::coupled::time::run(request))
    }
    #[napi]
    pub fn solve_cross_section(&mut self, request: Value) -> Result<Value> {
        let request=serde_json::from_value::<pcb_analyze::extraction::Request>(request)
            .map_err(|e| napi::Error::from_reason(format!("Invalid cross-section request: {e}")))?;
        Ok(pcb_analyze::extraction::run(request))
    }
    #[napi]
    pub fn propagate_cross_section(&mut self, request:Value,prepared:Value)->Result<Value>{
        let request=serde_json::from_value::<pcb_analyze::extraction::Request>(request).map_err(|e|napi::Error::from_reason(format!("Invalid section propagation request: {e}")))?;
        Ok(pcb_analyze::extraction::section_cache::replay(request,prepared))
    }
    #[napi]
    pub fn clear(&mut self) { self.inner=Default::default();self.local_response.clear(); }
}
