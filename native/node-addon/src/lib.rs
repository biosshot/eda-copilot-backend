//! The only Node boundary. Library calls retain their own numeric environments.
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::Value;

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
