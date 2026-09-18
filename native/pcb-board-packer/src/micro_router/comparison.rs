//! Paired post-place evaluation: freeze the routing obligations before trying
//! swap/rotate variants, and never reward an unfinished search as a cheap path.
use super::*;
use crate::model::BoardPackProblem;
use serde::{Deserialize, Serialize};
use rustc_hash::{FxHashSet}; 

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedJob {
    pub net: Arc<str>,
    pub source_primitive: Arc<str>,
    pub source_ref: Arc<str>,
    pub target_primitive: Arc<str>,
    pub target_ref: Arc<str>,
    pub priority: u8,
    pub weight: f64,
    pub via_cost: f64,
    pub ordinary: bool,
}

impl From<&RouteJob> for PlannedJob {
    fn from(job: &RouteJob) -> Self {
        Self {
            net: job.net.clone(), source_primitive: job.source.primitive_id.clone(),
            source_ref: job.source.reference.clone(), target_primitive: job.target.primitive_id.clone(),
            target_ref: job.target.reference.clone(), priority: job.priority,
            weight: job.weight, via_cost: job.via_cost, ordinary: job.ordinary,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStatus { Found, BudgetExhausted, NoPath }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSample {
    pub job: PlannedJob,
    pub status: RouteStatus,
    pub detour: Option<f64>,
    pub physical_cost: Option<f64>,
    pub planar_length: Option<f64>,
    pub vias: usize,
    pub expanded: usize,
    pub used_fallback: bool,
}

/// Plain serializable data, scoped to one current layout / refinement iteration.
/// The TS caller may cache this by changed-designator set; no native handles.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteBaseline {
    pub version: u32,
    pub jobs: Vec<RouteSample>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteComparison {
    pub before_penalty: f64,
    pub after_penalty: f64,
    pub unresolved_before: usize,
    pub unresolved_after: usize,
    pub budget_exhausted_before: usize,
    pub budget_exhausted_after: usize,
    /// -1: fewer unresolved obligations; 0: tie; 1: more (highest priority first).
    /// BudgetExhausted remains an unknown result, never a proof of NoPath.
    pub feasibility_order: i8,
    pub jobs: Vec<RouteSample>,
}

pub fn prepare(problem: &BoardPackProblem, obstacles: &[RouteObstacle], changed: &[String]) -> RouteBaseline {
    let config = MicroRouteConfig::post_place();
    let jobs = plan_jobs(&problem.primitives, &problem.relations, changed, &config);
    RouteBaseline { version: 1, jobs: evaluate(problem, obstacles, jobs, &config) }
}

pub fn compare(problem: &BoardPackProblem, obstacles: &[RouteObstacle], baseline: &RouteBaseline) -> Result<RouteComparison, String> {
    let config = MicroRouteConfig::post_place();
    if baseline.version != 1 || baseline.jobs.len() > config.max_total_jobs {
        return Err("invalid route baseline version or job count".into());
    }
    let mut jobs = Vec::new();
    for sample in &baseline.jobs {
        let spec = &sample.job;
        if spec.priority > 4 || !spec.weight.is_finite() || spec.weight <= 0.0
            || !spec.via_cost.is_finite() || spec.via_cost < 0.0
            || sample.detour.is_some_and(|value| !value.is_finite() || value < 0.0)
            || (sample.status == RouteStatus::Found) != sample.detour.is_some()
        { return Err("invalid route baseline sample".into()); }
        let source = resolve_planned_endpoint(&problem.primitives, &spec.source_primitive, &spec.source_ref, &spec.net, &config)?;
        let target = resolve_planned_endpoint(&problem.primitives, &spec.target_primitive, &spec.target_ref, &spec.net, &config)?;
        jobs.push(RouteJob { source, target, net: spec.net.clone(), priority: spec.priority,
            weight: spec.weight, via_cost: spec.via_cost, ordinary: spec.ordinary });
    }
    let after = evaluate(problem, obstacles, jobs, &config);
    Ok(compare_samples(&baseline.jobs, after, &config))
}

fn resolve_planned_endpoint(primitives: &[Primitive], id: &str, reference: &str, net: &str, config: &MicroRouteConfig) -> Result<RouteEndpoint, String> {
    let found = primitives.iter().find(|primitive| primitive.id.as_ref() == id).and_then(|primitive| {
        primitive.connection_points.iter()
            .find(|point| point.reference.as_ref() == reference && point.net.as_deref() == Some(net))
            .map(|point| RouteEndpoint { point: Point { x: point.x, y: point.y },
                primitive_id: primitive.id.clone(), reference: point.reference.clone(),
                net: Arc::from(net), layer: primitive_layer(primitive, config) })
    });
    found.ok_or_else(|| format!("route plan endpoint {id}/{reference} on net {net} is missing; rebuild baseline"))
}

fn plan_jobs(primitives: &[Primitive], relations: &[Relation], changed: &[String], config: &MicroRouteConfig) -> Vec<RouteJob> {
    let changed: FxHashSet<&str> = changed.iter().map(String::as_str).collect();
    let mut jobs = Vec::new();
    for candidate in primitives.iter().filter(|p| changed.contains(p.id.as_ref())) {
        let placed: Vec<_> = primitives.iter().filter(|p| p.id != candidate.id).collect();
        // Collect before capping: limits apply to the whole comparison, not to
        // each changed component separately.
        jobs.extend(explicit_jobs(candidate, &placed, relations, config));
        jobs.extend(ordinary_jobs(candidate, &placed, relations, config));
    }
    jobs.sort_by(|a, b| a.ordinary.cmp(&b.ordinary).then_with(|| job_order(a, b)));
    let jobs = dedupe_jobs(jobs);
    let explicit: Vec<_> = jobs.iter().filter(|job| !job.ordinary).cloned().collect();
    // A bounded estimator needs one representative primitive-to-primitive
    // obligation per net. USB-C A/B duplicate pads must not introduce a second
    // obligation only when a swap makes B geometrically closer than explicit A.
    let covered: FxHashSet<_> = explicit.iter().map(component_pair_key).collect();
    let ordinary: Vec<_> = jobs.into_iter().filter(|job| job.ordinary && !covered.contains(&component_pair_key(job))).collect();
    let mut result = cap_priority_jobs(explicit, config.max_total_jobs);
    let capacity = config.max_ordinary_jobs.min(config.max_total_jobs.saturating_sub(result.len()));
    result.extend(farthest_point_sample(cap_per_net(ordinary, config.max_ordinary_jobs_per_net), capacity));
    result.sort_by(job_order);
    result
}

fn component_pair_key(job: &RouteJob) -> (Arc<str>, Arc<str>, Arc<str>) {
    let (a, b) = (&job.source.primitive_id, &job.target.primitive_id);
    if a <= b { (job.net.clone(), a.clone(), b.clone()) } else { (job.net.clone(), b.clone(), a.clone()) }
}

fn evaluate(problem: &BoardPackProblem, routing_obstacles: &[RouteObstacle], jobs: Vec<RouteJob>, config: &MicroRouteConfig) -> Vec<RouteSample> {
    let all: Vec<_> = problem.primitives.iter().collect();
    let obstacles = collect_obstacles(&all, &problem.obstacles, routing_obstacles, config);
    let mut temporary = TemporaryRoutes::default();
    jobs.into_iter().map(|job| {
        let mut sample = RouteSample { job: (&job).into(), status: RouteStatus::BudgetExhausted,
            detour: None, physical_cost: None, planar_length: None, vias: 0, expanded: 0, used_fallback: false };
        match route_job(&job, problem.bounds, &problem.board_outline, &obstacles, &temporary, config) {
            RouteOutcome::Found(result) => {
                sample.status = RouteStatus::Found;
                sample.detour = Some((result.cost.physical - baseline_cost(&job, config)).max(0.0));
                sample.physical_cost = Some(result.cost.physical);
                sample.planar_length = Some(result.cost.physical - result.cost.vias as f64 * job.via_cost);
                sample.vias = result.cost.vias;
                sample.expanded = result.expanded;
                sample.used_fallback = result.used_fallback;
                temporary.reserve(&result.cells, &job.net);
            }
            RouteOutcome::NoPath { expanded } => { sample.status = RouteStatus::NoPath; sample.expanded = expanded; }
            RouteOutcome::BudgetExhausted { expanded } => { sample.expanded = expanded; }
        }
        sample
    }).collect()
}

fn compare_samples(before: &[RouteSample], after: Vec<RouteSample>, config: &MicroRouteConfig) -> RouteComparison {
    let mut result = RouteComparison { before_penalty: 0.0, after_penalty: 0.0,
        unresolved_before: 0, unresolved_after: 0, budget_exhausted_before: 0, budget_exhausted_after: 0,
        feasibility_order: 0, jobs: after };
    let mut before_unresolved = [0.0; 5];
    let mut after_unresolved = [0.0; 5];
    for (a, b) in before.iter().zip(&result.jobs) {
        let missing = a.detour.unwrap_or(0.0).max(b.detour.unwrap_or(0.0))
            .max(2.0 * a.job.via_cost) + config.unroutable_penalty_mm;
        let scale = a.job.weight * config.route_scale;
        result.before_penalty += a.detour.unwrap_or(missing) * scale;
        result.after_penalty += b.detour.unwrap_or(missing) * scale;
        if a.status != RouteStatus::Found {
            result.unresolved_before += 1;
            before_unresolved[a.job.priority as usize] += a.job.weight;
        }
        if b.status != RouteStatus::Found {
            result.unresolved_after += 1;
            after_unresolved[b.job.priority as usize] += b.job.weight;
        }
        result.budget_exhausted_before += usize::from(a.status == RouteStatus::BudgetExhausted);
        result.budget_exhausted_after += usize::from(b.status == RouteStatus::BudgetExhausted);
    }
    for priority in (0..5).rev() {
        match compare_f64(after_unresolved[priority], before_unresolved[priority]) {
            Ordering::Less => { result.feasibility_order = -1; break; }
            Ordering::Greater => { result.feasibility_order = 1; break; }
            Ordering::Equal => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{primitive, bounds};

    #[test]
    fn search_cutoff_is_not_no_path() {
        let a = primitive("A", -4.0, 0.0, "SIG");
        let b = primitive("B", 4.0, 0.0, "SIG");
        let mut config = MicroRouteConfig::post_place();
        config.max_expanded = 0;
        config.retry_expanded = 0;
        let job = schedule_jobs(&a, &[&b], &[], &config).remove(0);
        assert!(matches!(route_job(&job, bounds(), &[], &[], &TemporaryRoutes::default(), &config),
            RouteOutcome::BudgetExhausted { expanded: 0 }));
        let target = point_to_cell(job.target.point, 0, bounds(), config.grid);
        let mut temporary = TemporaryRoutes::default();
        temporary.reserve(&[target], &Arc::from("OTHER"));
        assert!(matches!(route_job(&job, bounds(), &[], &[], &temporary, &config),
            RouteOutcome::NoPath { expanded: 0 }));
    }

    #[test]
    fn unresolved_job_is_never_priced_below_its_known_expensive_route() {
        let spec = PlannedJob { net: Arc::from("SIG"), source_ref: Arc::from("A.1"), target_ref: Arc::from("B.1"),
            source_primitive: Arc::from("A"), target_primitive: Arc::from("B"), priority: 4, weight: 16.0, via_cost: 45.0, ordinary: false };
        let found = RouteSample { job: spec, status: RouteStatus::Found, detour: Some(250.0), physical_cost: Some(260.0),
            planar_length: Some(170.0), vias: 2, expanded: 42, used_fallback: false };
        for status in [RouteStatus::NoPath, RouteStatus::BudgetExhausted] {
            let mut failed = found.clone(); failed.status = status; failed.detour = None; failed.physical_cost = None;
            let comparison = compare_samples(&[found.clone()], vec![failed], &MicroRouteConfig::post_place());
            assert!(comparison.after_penalty > comparison.before_penalty);
            assert_eq!(comparison.feasibility_order, 1);
        }
    }
}
