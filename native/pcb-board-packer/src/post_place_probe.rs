//! Optional diagnostic API; excluded from normal release builds.
use crate::{model::{BoardPackProblem, PostPlaceScoreProblem, RouteObstacle}, micro_router::comparison::{self, RouteBaseline}, post_place};
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::Value;
use std::{hint::black_box, time::Instant};

fn error(message: impl std::fmt::Display) -> Error {
    Error::new(Status::InvalidArg, message.to_string())
}
fn repeats(value: u32) -> Result<usize> {
    if !(1..=32).contains(&value) { return Err(error("probe repetitions must be 1..32")); }
    Ok(value as usize)
}

#[napi]
pub fn probe_decode_score(value: Value) -> Result<Value> {
    let started = Instant::now();
    let problem: PostPlaceScoreProblem = serde_json::from_value(value).map_err(error)?;
    let items = problem.nets.len();
    drop(black_box(problem));
    Ok(serde_json::json!({ "items": items, "insideMs": started.elapsed().as_secs_f64() * 1000.0 }))
}
#[napi]
pub fn probe_decode_route(problem: Value, obstacles: Value, baseline: Value) -> Result<Value> {
    let started = Instant::now();
    let problem: BoardPackProblem = serde_json::from_value(problem).map_err(error)?;
    problem.validate(crate::CONTRACT_VERSION).map_err(error)?;
    let obstacles: Vec<RouteObstacle> = serde_json::from_value(obstacles).map_err(error)?;
    let baseline: RouteBaseline = serde_json::from_value(baseline).map_err(error)?;
    let items = problem.primitives.len() + obstacles.len() + baseline.jobs.len();
    drop(black_box(problem)); drop(black_box(obstacles)); drop(black_box(baseline));
    Ok(serde_json::json!({ "items": items, "insideMs": started.elapsed().as_secs_f64() * 1000.0 }))
}

#[napi]
pub struct PreparedPostPlaceProbe {
    problem: BoardPackProblem,
    obstacles: Vec<RouteObstacle>,
    score: PostPlaceScoreProblem,
    baseline: RouteBaseline,
}
#[napi]
impl PreparedPostPlaceProbe {
    #[napi(constructor)]
    pub fn new(problem: Value, obstacles: Value, score: Value, baseline: Value) -> Result<Self> {
        let problem: BoardPackProblem = serde_json::from_value(problem).map_err(error)?;
        problem.validate(crate::CONTRACT_VERSION).map_err(error)?;
        Ok(Self { problem, obstacles: serde_json::from_value(obstacles).map_err(error)?,
            score: serde_json::from_value(score).map_err(error)?, baseline: serde_json::from_value(baseline).map_err(error)? })
    }
    #[napi]
    pub fn score(&self, count: u32) -> Result<Value> {
        let count = repeats(count)?;
        let started = Instant::now();
        let mut result = 0.0;
        for _ in 0..count { result = black_box(post_place::score(black_box(&self.score)).map_err(error)?); }
        Ok(serde_json::json!({ "result": result, "computeMs": started.elapsed().as_secs_f64() * 1000.0, "repetitions": count }))
    }
    #[napi]
    pub fn prepare(&self, changed: Vec<String>, count: u32) -> Result<Value> {
        let count = repeats(count)?;
        let started = Instant::now();
        let mut result = None;
        for _ in 0..count { result = Some(black_box(comparison::prepare(black_box(&self.problem), &self.obstacles, &changed))); }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        Ok(serde_json::json!({ "result": result, "computeMs": elapsed, "repetitions": count }))
    }
    #[napi]
    pub fn compare(&self, count: u32) -> Result<Value> {
        let count = repeats(count)?;
        let started = Instant::now();
        let mut result = None;
        for _ in 0..count { result = Some(black_box(comparison::compare(black_box(&self.problem), &self.obstacles, &self.baseline).map_err(error)?)); }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        Ok(serde_json::json!({ "result": result, "computeMs": elapsed, "repetitions": count }))
    }
}
