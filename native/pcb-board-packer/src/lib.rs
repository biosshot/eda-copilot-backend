mod post_place_refine;
mod board_alignment;
mod block_solver;
mod compute;
mod fast_route;
mod geometry;
mod geometry_policy;
mod group_endpoints;
mod float_env;
mod f32_policy;
mod interval;
mod boundary;
mod rotation;
#[path = "compute/numerics.rs"]
mod numerics;
mod lazy_rank;
mod model;
mod net_class;
mod ordinary_net;
mod passive_island_solver;
mod post_place;
mod local_stretch;
mod relation_score;
mod conditions;
mod pad_access;
mod placement_quality;
mod signal_path;
mod solver;

use model::{
    BlockSolveProblem, BoardPackProblem, PassiveIslandProblem, PostPlaceScoreProblem,
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
pub fn placement_geometry_epsilon() -> f32 { geometry_policy::DISTANCE_EPSILON_MM }

#[napi]
pub fn numeric_contract() -> String { "f32-rte-ftz-v1".into() }

#[napi]
pub fn evaluate_block_quality(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let mut problem: BlockSolveProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    block_solver::attach_group_relation_points(&mut problem).map_err(invalid_problem)?;
    problem.validate(BLOCK_CONTRACT_VERSION).map_err(invalid_problem)?;
    block_solver::quality(problem).map_err(invalid_problem)
}

#[napi]
pub fn evaluate_board_quality(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    solver::quality(problem).map_err(invalid_problem)
}

#[napi]
pub fn score_board_spacing(mut problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    Ok(solver::spacing(problem))
}

#[napi]
pub fn score_board_alignment(mut primitives: Value, policy: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    // Reuse the normal coordinate boundary for geometry-only callers.
    let mut envelope = serde_json::json!({"primitives":primitives});
    boundary::Frame::localize(&mut envelope).map_err(invalid_problem)?;
    primitives = envelope["primitives"].take();
    let primitives: Vec<model::Primitive> = serde_json::from_value(primitives).map_err(invalid_problem)?;
    let policy: model::SoftAlignment = serde_json::from_value(policy).map_err(invalid_problem)?;
    Ok(solver::alignment_score(primitives.iter(), &policy))
}

#[napi]
pub fn placement_score_sum(scores: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    let scores: Vec<f32> = serde_json::from_value(scores).map_err(invalid_problem)?;
    let score = scores.iter().copied().sum::<f32>();
    if !score.is_finite() { return Err(invalid_problem("non-finite placement score")); }
    Ok(score)
}

#[napi]
pub fn placement_net_weights(names: Vec<String>, board: bool) -> Vec<f32> {
    let _float_env = float_env::Guard::enter();
    names.iter().map(|name| if net_class::is_power(name) { if board {0.1} else {0.25} } else {1.0}).collect()
}

#[napi]
pub fn placement_net_class(name: String) -> Value {
    serde_json::json!({"ground":net_class::is_ground(&name),"power":net_class::is_power(&name),"switching":net_class::is_switching_power(&name)})
}

#[napi]
pub fn placement_orientation_weight(problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    let problem=serde_json::from_value(problem).map_err(invalid_problem)?;
    Ok(placement_quality::orientation_weight(&problem))
}


#[napi]
pub fn placement_layer_penalties(terms: Value) -> Result<Vec<f32>> {
    let _float_env = float_env::Guard::enter();
    let terms: Vec<(f32,bool)> = serde_json::from_value(terms).map_err(invalid_problem)?;
    Ok(terms.iter().map(|(weight,same)|post_place::layer_penalty(*weight,*same)).collect())
}

#[napi]
pub fn placement_score_contract_version() -> u32 { 13 }

#[napi]
pub fn placement_priority_weight(priority: String) -> f64 {
    let _env = float_env::Guard::enter();
    relation_score::priority_scale(&priority).1 as f64
}

#[napi]
pub fn placement_relation_profile(request: Value) -> Result<Value> {
    let _env = float_env::Guard::enter();
    let r: relation_score::Request = serde_json::from_value(request).map_err(|e| Error::from_reason(e.to_string()))?;
    serde_json::to_value(relation_score::profile(&r)).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi]
pub fn placement_spacing_policy(measurements: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    let [occupied,usable_area]: [f32;2] = serde_json::from_value(measurements).map_err(invalid_problem)?;
    let density=occupied/usable_area.max(1.0);
    let freedom=((0.60-density)/0.35).clamp(0.0,1.0);
    Ok(serde_json::json!({"gap":3.0*freedom,"compactnessScale":1.0-0.9*freedom,"density":density}))
}

#[napi]
pub fn placement_alignment_policy(pair_count: u32, normalize: bool) -> Value {
    let _float_env = float_env::Guard::enter();
    serde_json::json!({"weight":if normalize {1.0/pair_count.max(1) as f32}else{1.0},"tolerance":0.15f32,"orientationWeight":if normalize {4.0/pair_count.max(1) as f32}else{4.0}})
}

#[napi]
pub fn placement_block_similarity(problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    let problem=serde_json::from_value(problem).map_err(invalid_problem)?;
    Ok(placement_quality::similarity(&problem))
}

#[napi]
pub fn evaluate_island_quality(mut problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: placement_quality::IslandQuality = serde_json::from_value(problem).map_err(invalid_problem)?;
    let rank = placement_quality::island_rank(&problem);
    if !rank.score.is_finite() || !rank.hard_severity.is_finite() { return Err(invalid_problem("non-finite island rank")); }
    serde_json::to_value(rank).map_err(invalid_problem)
}

#[napi]
pub fn score_island_quality(mut problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: placement_quality::IslandQuality = serde_json::from_value(problem).map_err(invalid_problem)?;
    let score = placement_quality::island(&problem);
    if !score.is_finite() { return Err(invalid_problem("non-finite island score")); }
    Ok(score)
}

#[napi]
pub fn score_placement_orientation(problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    // All points in this contract are relative vectors, not world coordinates.
    let problem: placement_quality::OrientationQuality = serde_json::from_value(problem).map_err(invalid_problem)?;
    let score = placement_quality::orientation_score(&problem);
    if !score.is_finite() { return Err(invalid_problem("non-finite orientation score")); }
    Ok(score)
}

#[napi]
pub fn compare_block_quality(a: Value, b: Value, kind: String) -> Result<bool> {
    let _float_env = float_env::Guard::enter();
    let a = serde_json::from_value(a).map_err(invalid_problem)?;
    let b = serde_json::from_value(b).map_err(invalid_problem)?;
    placement_quality::compare_block(&a,&b,&kind).map_err(invalid_problem)
}

#[napi]
pub fn board_electrical_regression(before: Value, after: Value) -> Result<Option<String>> {
    let _float_env = float_env::Guard::enter();
    let before = serde_json::from_value(before).map_err(invalid_problem)?;
    let after = serde_json::from_value(after).map_err(invalid_problem)?;
    Ok(placement_quality::electrical_regression(&before,&after).map(str::to_owned))
}

#[napi]
pub fn post_place_net_lengths(mut problem: Value) -> Result<Vec<f32>> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: PostPlaceScoreProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    post_place::score(&problem).map_err(invalid_problem)?;
    Ok(problem.nets.iter().map(post_place::net_length).collect())
}

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

/// Canonical full-layout geometric routing score (also used by solver stages).
#[napi]
pub fn score_fast_route_layout(mut problem: Value) -> Result<f32> {
    let _float_env = float_env::Guard::enter();
    boundary::Frame::localize(&mut problem).map_err(invalid_problem)?;
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(invalid_problem)?;
    problem.validate(CONTRACT_VERSION).map_err(invalid_problem)?;
    let mut obstacles=problem.routing_obstacles.clone();
    obstacles.extend(problem.obstacles.iter().map(|b| model::RouteObstacle {box_:*b,layer:None,reference:None,net:None,primitive_id:None}));
    Ok(fast_route::layout_plan(&problem.primitives.iter().collect::<Vec<_>>(), &obstacles, &[], &problem.ignored_nets, problem.clearance, problem.bounds, &problem.relations).cpu())
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
    let mut problem: BlockSolveProblem =
        serde_json::from_value(problem).map_err(invalid_block_problem)?;
    block_solver::attach_group_relation_points(&mut problem).map_err(invalid_block_problem)?;
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
        let mut problem: BlockSolveProblem = serde_json::from_value(value).map_err(invalid_block_problem)?;
        block_solver::attach_group_relation_points(&mut problem).map_err(invalid_block_problem)?;
        problem.validate(BLOCK_CONTRACT_VERSION).map_err(invalid_block_problem)?;
        frame.capture("block",&problem);
        Ok((problem,frame))
    }).collect::<Result<Vec<_>>>()?;
    let count = problems.len();
    if count == 0 { return Ok(Vec::new()); }
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    let workers = (threads as usize).max(1).min(8).min(count).min(available);
    // Threads left over when there are fewer hypotheses than threads go to each
    // hypothesis' pair neighbourhood. This does not change any result.
    let pair_threads = ((threads as usize).max(1).min(available) / count).max(1);
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
                let result = block_solver::solve_block_with_threads(problem, pair_threads)
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

#[napi]
pub fn board_alignment_contract_version() -> u32 { board_alignment::CONTRACT_VERSION }

/// Complete board alignment finishing pass; see `board_alignment`.
#[napi]
pub fn refine_board_alignment(problem: Value) -> Result<Value> {
    let _float_env = float_env::Guard::enter();
    board_alignment::solve(problem).map_err(invalid_problem)
}

#[napi]
pub fn placement_access_profile(request: Value) -> Result<Value> {
    let _guard = float_env::Guard::enter();
    let request = serde_json::from_value(request).map_err(invalid_problem)?;
    serde_json::to_value(pad_access::profile(&request).map_err(invalid_problem)?).map_err(invalid_problem)
}
