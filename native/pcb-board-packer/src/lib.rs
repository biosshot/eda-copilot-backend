mod block_solver;
mod geometry;
mod lazy_rank;
mod model;
mod micro_router;
mod net_class;
mod ordinary_net;
mod passive_island_solver;
mod post_place;
mod signal_path;
mod solver;

use model::{
    BlockSolveProblem, BoardPackProblem, PassiveIslandProblem, PostPlaceScoreProblem, RouteObstacle,
    SignalPathBridgeProblem, SignalPathEvaluationProblem,
};
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::Value;

const CONTRACT_VERSION: u32 = 3;
const BLOCK_CONTRACT_VERSION: u32 = 2;
const PASSIVE_ISLAND_CONTRACT_VERSION: u32 = 1;
const POST_PLACE_SCORE_CONTRACT_VERSION: u32 = 1;
const SIGNAL_PATH_CONTRACT_VERSION: u32 = 1;

#[napi]
pub fn contract_version() -> u32 {
    CONTRACT_VERSION
}

#[napi]
pub fn solve_board_packed(problem: Value) -> Result<Value> {
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem
        .validate(CONTRACT_VERSION)
        .map_err(invalid_problem)?;

    let solution =
        solver::solve(problem).map_err(|message| Error::new(Status::GenericFailure, message))?;
    serde_json::to_value(solution).map_err(invalid_problem)
}

#[napi]
pub fn score_route_layout(problem: Value, changed_primitive_ids: Vec<String>) -> Result<f64> {
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem
        .validate(CONTRACT_VERSION)
        .map_err(invalid_problem)?;
    Ok(micro_router::changed_layout_penalty(
        &problem.primitives,
        &problem.relations,
        problem.bounds,
        &problem.board_outline,
        &problem.obstacles,
        &[],
        &changed_primitive_ids,
        &micro_router::MicroRouteConfig::post_place(),
    ))
}

#[napi]
pub fn score_route_layout_with_obstacles(
    problem: Value,
    changed_primitive_ids: Vec<String>,
    routing_obstacles: Value,
) -> Result<f64> {
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    let routing_obstacles: Vec<RouteObstacle> =
        serde_json::from_value(routing_obstacles).map_err(invalid_problem)?;
    Ok(micro_router::changed_layout_penalty(
        &problem.primitives,
        &problem.relations,
        problem.bounds,
        &problem.board_outline,
        &problem.obstacles,
        &routing_obstacles,
        &changed_primitive_ids,
        &micro_router::MicroRouteConfig::post_place(),
    ))
}

/// Freeze and evaluate one bounded job plan on the current layout.
#[napi]
pub fn prepare_route_layout_comparison(problem: Value, changed_primitive_ids: Vec<String>, routing_obstacles: Value) -> Result<Value> {
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    let obstacles: Vec<RouteObstacle> = serde_json::from_value(routing_obstacles).map_err(invalid_problem)?;
    serde_json::to_value(micro_router::comparison::prepare(&problem, &obstacles, &changed_primitive_ids)).map_err(invalid_problem)
}

/// Evaluate a variant against exactly the baseline's terminal pairs and order.
#[napi]
pub fn compare_route_layout_candidate(problem: Value, routing_obstacles: Value, baseline: Value) -> Result<Value> {
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    let obstacles: Vec<RouteObstacle> = serde_json::from_value(routing_obstacles).map_err(invalid_problem)?;
    let baseline: micro_router::comparison::RouteBaseline = serde_json::from_value(baseline).map_err(invalid_problem)?;
    let result = micro_router::comparison::compare(&problem, &obstacles, &baseline).map_err(invalid_problem)?;
    serde_json::to_value(result).map_err(invalid_problem)
}

#[napi]
pub fn block_contract_version() -> u32 {
    BLOCK_CONTRACT_VERSION
}

#[napi]
pub fn solve_block_primitives(problem: Value) -> Result<Value> {
    let problem: BlockSolveProblem =
        serde_json::from_value(problem).map_err(invalid_block_problem)?;
    problem
        .validate(BLOCK_CONTRACT_VERSION)
        .map_err(invalid_block_problem)?;
    let solution = block_solver::solve_block(problem)
        .map_err(|message| Error::new(Status::GenericFailure, message))?;
    serde_json::to_value(solution).map_err(invalid_block_problem)
}

#[napi]
pub fn passive_island_contract_version() -> u32 {
    PASSIVE_ISLAND_CONTRACT_VERSION
}

#[napi]
pub fn solve_passive_net_island(problem: Value) -> Result<Value> {
    let problem: PassiveIslandProblem =
        serde_json::from_value(problem).map_err(invalid_passive_island_problem)?;
    problem
        .validate(PASSIVE_ISLAND_CONTRACT_VERSION)
        .map_err(invalid_passive_island_problem)?;
    let solution = passive_island_solver::solve(problem)
        .map_err(|message| Error::new(Status::GenericFailure, message))?;
    serde_json::to_value(solution).map_err(invalid_passive_island_problem)
}

#[napi]
pub fn post_place_score_contract_version() -> u32 {
    POST_PLACE_SCORE_CONTRACT_VERSION
}

#[napi]
pub fn score_post_place(problem: Value) -> Result<f64> {
    let problem: PostPlaceScoreProblem = serde_json::from_value(problem).map_err(|error| {
        Error::new(
            Status::InvalidArg,
            format!("Invalid post-place score problem: {error}"),
        )
    })?;
    post_place::score(&problem).map_err(|error| {
        Error::new(
            Status::InvalidArg,
            format!("Invalid post-place score problem: {error}"),
        )
    })
}

#[napi]
pub fn signal_path_contract_version() -> u32 {
    SIGNAL_PATH_CONTRACT_VERSION
}

#[napi]
pub fn evaluate_signal_path(problem: Value) -> Result<Value> {
    let problem: SignalPathEvaluationProblem =
        serde_json::from_value(problem).map_err(|error| {
            Error::new(
                Status::InvalidArg,
                format!("Invalid signal-path problem: {error}"),
            )
        })?;
    if problem.version != SIGNAL_PATH_CONTRACT_VERSION {
        return Err(Error::new(
            Status::InvalidArg,
            format!(
                "Invalid signal-path problem: unsupported contract {}; expected {}",
                problem.version, SIGNAL_PATH_CONTRACT_VERSION
            ),
        ));
    }
    let evaluation = signal_path::evaluate_ports(
        &problem.path_id,
        &problem.ports,
        problem.shape.as_ref() == "straight",
        &problem.priority,
        problem.weight,
        problem.prefer_facing_pads,
    );
    serde_json::to_value(evaluation).map_err(invalid_problem)
}

#[napi]
pub fn signal_path_bridge_deltas(problem: Value) -> Result<Value> {
    let problem: SignalPathBridgeProblem = serde_json::from_value(problem).map_err(|error| {
        Error::new(
            Status::InvalidArg,
            format!("Invalid signal-path bridge problem: {error}"),
        )
    })?;
    if problem.version != SIGNAL_PATH_CONTRACT_VERSION {
        return Err(Error::new(
            Status::InvalidArg,
            format!(
                "Invalid signal-path bridge problem: unsupported contract {}; expected {}",
                problem.version, SIGNAL_PATH_CONTRACT_VERSION
            ),
        ));
    }
    serde_json::to_value(signal_path::bridge_deltas_for_ports(
        &problem.moving_ports,
        &problem.placed_ports,
    ))
    .map_err(invalid_problem)
}

fn invalid_problem(error: impl std::fmt::Display) -> Error {
    Error::new(
        Status::InvalidArg,
        format!("Invalid native board pack problem: {error}"),
    )
}

fn invalid_block_problem(error: impl std::fmt::Display) -> Error {
    Error::new(
        Status::InvalidArg,
        format!("Invalid native block solve problem: {error}"),
    )
}

fn invalid_passive_island_problem(error: impl std::fmt::Display) -> Error {
    Error::new(
        Status::InvalidArg,
        format!("Invalid native passive island problem: {error}"),
    )
}
