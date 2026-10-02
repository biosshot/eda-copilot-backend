mod post_place_refine;
#[cfg(feature = "placement-bench")]
mod post_place_probe;
mod block_solver;
mod compute;
mod fast_route;
mod geometry;
mod float_env;
mod f32_policy;
mod interval;
mod boundary;
mod rotation;
#[path = "compute/numerics.rs"]
mod numerics;
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

const CONTRACT_VERSION: u32 = 7;
const BLOCK_CONTRACT_VERSION: u32 = 4;
const PASSIVE_ISLAND_CONTRACT_VERSION: u32 = 1;
const POST_PLACE_SCORE_CONTRACT_VERSION: u32 = 1;
const SIGNAL_PATH_CONTRACT_VERSION: u32 = 1;

#[napi]
pub fn numeric_contract() -> String { "f32-rte-ftz-v1".into() }

#[cfg(all(test,feature="gpu"))]
#[test]
#[ignore="requires an unleased Vulkan GPU and PCB_F32_REPLAY_CAPTURE; run alone"]
fn replay_f32_board_verification_capture() {
    let _env=float_env::Guard::enter();
    let path=std::env::var_os("PCB_F32_REPLAY_CAPTURE").expect("canonical capture path");
    let capture:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(capture["schema"],"pcb-f32-canonical-v1");
    let mut value=capture["problem"].clone();
    if let Ok(mode)=std::env::var("PCB_F32_REPLAY_COMPACTNESS") {value["compactness"]=Value::from(mode);}
    let problem:BoardPackProblem=serde_json::from_value(value).unwrap();
    problem.validate(CONTRACT_VERSION).unwrap();
    let count=problem.primitives.len();
    let solution=solver::solve(problem).unwrap();
    assert_eq!(solution.states.len(),count);
    assert_eq!(solution.rank.hard_count,0);
    assert!(solution.rank.score.is_finite());
    eprintln!("[f32-board-replay] {}",serde_json::to_string(&solution.rank).unwrap());
}

#[cfg(all(feature = "gpu", feature = "placement-bench"))]
#[napi]
pub fn block_gpu_probe(values: Vec<f32>, inject_panic: bool) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    compute::gpu::probe(&values, inject_panic)
        .map_err(|message| Error::new(Status::GenericFailure, message.to_string()))
}

#[napi]
pub fn contract_version() -> u32 {
    let _float_env = float_env::Guard::enter();
    CONTRACT_VERSION
}

#[napi]
pub fn solve_board_packed(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem
        .validate(CONTRACT_VERSION)
        .map_err(invalid_problem)?;

    frame.capture("board",&problem);
    let solution =
        solver::solve(problem).map_err(|message| Error::new(Status::GenericFailure, message))?;
    let mut output = serde_json::to_value(solution).map_err(invalid_problem)?;
    frame.restore(&mut output).map_err(invalid_problem)?;
    output["numericFrame"] = frame.metadata();
    Ok(output)
}

#[napi]
pub fn score_route_layout(mut problem: Value, changed_primitive_ids: Vec<String>) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
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
    mut problem: Value,
    changed_primitive_ids: Vec<String>,
    mut routing_obstacles: Value,
) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    frame.localize_related(&mut routing_obstacles).map_err(invalid_problem)?;
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
pub fn prepare_route_layout_comparison(mut problem: Value, changed_primitive_ids: Vec<String>, mut routing_obstacles: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    frame.localize_related(&mut routing_obstacles).map_err(invalid_problem)?;
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    let obstacles: Vec<RouteObstacle> = serde_json::from_value(routing_obstacles).map_err(invalid_problem)?;
    serde_json::to_value(micro_router::comparison::prepare(&problem, &obstacles, &changed_primitive_ids)).map_err(invalid_problem)
}

/// Evaluate a variant against exactly the baseline's terminal pairs and order.
#[napi]
pub fn compare_route_layout_candidate(mut problem: Value, mut routing_obstacles: Value, baseline: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    frame.localize_related(&mut routing_obstacles).map_err(invalid_problem)?;
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    let obstacles: Vec<RouteObstacle> = serde_json::from_value(routing_obstacles).map_err(invalid_problem)?;
    let baseline: micro_router::comparison::RouteBaseline = serde_json::from_value(baseline).map_err(invalid_problem)?;
    let result = micro_router::comparison::compare(&problem, &obstacles, &baseline).map_err(invalid_problem)?;
    serde_json::to_value(result).map_err(invalid_problem)
}

#[napi]
pub fn block_contract_version() -> u32 {
    let _float_env = float_env::Guard::enter();
    BLOCK_CONTRACT_VERSION
}

#[napi]
pub fn solve_block_primitives(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize_block(&mut problem).map_err(invalid_block_problem)?;
    let problem: BlockSolveProblem =
        serde_json::from_value(problem).map_err(invalid_block_problem)?;
    problem
        .validate(BLOCK_CONTRACT_VERSION)
        .map_err(invalid_block_problem)?;
    frame.capture("block",&problem);
    let solution = block_solver::solve_block(problem)
        .map_err(|message| Error::new(Status::GenericFailure, message))?;
    frame.finish_block(serde_json::to_value(solution).map_err(invalid_block_problem)?)
        .map_err(invalid_block_problem)
}

#[napi]
pub fn passive_island_contract_version() -> u32 {
    let _float_env = float_env::Guard::enter();
    PASSIVE_ISLAND_CONTRACT_VERSION
}

#[napi]
pub fn block_search_stages_version() -> u32 {
    let _float_env = float_env::Guard::enter(); 1 }

/// Independent hypotheses share no solver state. Results retain input order,
/// so completion order cannot change checkpoint selection or tie breaking.
#[napi]
pub fn solve_block_primitives_batch(problems: Vec<Value>, threads: u32) -> Result<Vec<Value>> {
    let _float_env = float_env::Guard::enter();
    let problems = problems.into_iter().map(|mut value| {
        let frame = boundary::Frame::localize_block(&mut value).map_err(invalid_block_problem)?;
        let problem: BlockSolveProblem = serde_json::from_value(value).map_err(invalid_block_problem)?;
        problem.validate(BLOCK_CONTRACT_VERSION).map_err(invalid_block_problem)?;
        frame.capture("block",&problem);
        Ok((problem,frame))
    }).collect::<Result<Vec<_>>>()?;
    let count = problems.len();
    if count == 0 { return Ok(Vec::new()); }
    let workers = (threads as usize).max(1).min(8).min(count)
        .min(std::thread::available_parallelism().map_or(1, |n| n.get()));
    #[cfg(feature="gpu")]
    let cpu_budget=compute::cpu::Budget::new(workers);
    // At most eight additional stacks may wait for GPU. Active CPU execution
    // remains bounded by `workers`, including full-call CPU recovery.
    #[cfg(feature="gpu")]
    let worker_slots=if std::env::var("PCB_BLOCK_BACKEND").as_deref()!=Ok("cpu")
        && std::env::var("PCB_BLOCK_GPU_DISABLED").as_deref()!=Ok("1") {count.min(workers+8)}else{workers};
    #[cfg(not(feature="gpu"))]
    let worker_slots=workers;
    let jobs = std::sync::Mutex::new(problems.into_iter().enumerate());
    let mut results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..worker_slots).map(|_| scope.spawn(|| {
            let _float_env = float_env::Guard::enter();
            #[cfg(feature="gpu")]
            let _cpu_worker=cpu_budget.enter();
            let mut completed = Vec::new();
            loop {
                let job = jobs.lock().expect("block job queue poisoned").next();
                let Some((index, (problem,frame))) = job else { break; };
                let result = block_solver::solve_block(problem)
                    .and_then(|solution| serde_json::to_value(solution).map_err(|e| e.to_string()))
                    .and_then(|solution| frame.finish_block(solution));
                completed.push((index, result));
            }
            completed
        })).collect();
        let mut completed = Vec::with_capacity(count);
        // Join every worker even if one panics; never unwind through N-API.
        let mut failed = false;
        for handle in handles {
            match handle.join() { Ok(items) => completed.extend(items), Err(_) => failed = true }
        }
        if failed { Err(Error::new(Status::GenericFailure, "Block solver worker panicked")) }
        else { Ok(completed) }
    })?;
    #[cfg(feature="gpu")]
    if std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some() {
        eprintln!("[block-cpu-scheduler] {}",serde_json::json!({"threads":worker_slots,"jobs":count,"cpu":cpu_budget.report()}));
    }
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result.map_err(|e| Error::new(Status::GenericFailure, e))).collect()
}

#[napi]
pub fn solve_passive_net_island(problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
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
    let _float_env = float_env::Guard::enter();
    POST_PLACE_SCORE_CONTRACT_VERSION
}

#[napi]
pub fn score_post_place(mut problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
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
    let _float_env = float_env::Guard::enter();
    SIGNAL_PATH_CONTRACT_VERSION
}

#[napi]
pub fn evaluate_signal_path(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
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
pub fn signal_path_bridge_deltas(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
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

#[napi]
pub fn post_place_refine_contract_version() -> u32 {
    let _float_env = float_env::Guard::enter(); 3 }

#[napi]
pub fn validate_placement(mut problem: Value, scope: Option<Vec<String>>) -> Result<bool> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let p: post_place_refine::RefineProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    post_place_refine::validate_layout_scope(p, scope).map_err(invalid_problem)
}

#[napi]
pub fn validate_placement_change(mut problem: Value, mut placements: Value) -> Result<bool> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    frame.localize_related(&mut placements).map_err(invalid_problem)?;
    let p: post_place_refine::RefineProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    let placements: Vec<model::Placement> = serde_json::from_value(placements).map_err(invalid_problem)?;
    post_place_refine::validate_change(p, placements).map_err(invalid_problem)
}

#[napi]
pub fn refine_post_placement(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    let frame = boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: post_place_refine::RefineProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    frame.capture("refine",&problem);
    let mut output = post_place_refine::solve(problem).map_err(invalid_problem)?;
    frame.restore(&mut output).map_err(invalid_problem)?;
    output["numericFrame"] = frame.metadata();
    Ok(output)
}
