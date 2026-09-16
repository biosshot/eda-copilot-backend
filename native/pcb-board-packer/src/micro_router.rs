use crate::geometry::{point_in_polygon, point_to_polygon_distance, Box2, Point};
use crate::model::{Primitive, Relation};
use crate::net_class::{is_ground, is_power, is_switching_power};
use crate::ordinary_net::MARKER_PREFIX;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

const EPS: f64 = 1e-9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferredDirection {
    Horizontal,
    Vertical,
    Any,
}

#[derive(Clone, Debug)]
pub struct RouteLayer {
    pub name: Arc<str>,
    pub preferred_direction: PreferredDirection,
}

#[derive(Clone, Copy, Debug)]
pub struct ViaTransition {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug)]
pub struct MicroRouteConfig {
    pub layers: Vec<RouteLayer>,
    pub via_transitions: Vec<ViaTransition>,
    pub grid: f64,
    pub trace_width: f64,
    pub clearance: f64,
    /** Normal explicit-route via cost in mm-equivalent. */
    pub via_cost: f64,
    pub ordinary_via_cost: f64,
    pub high_via_cost: f64,
    pub critical_via_cost: f64,
    pub power_via_cost: f64,
    pub include_local_power: bool,
    pub max_power_fanout: usize,
    pub max_expanded: usize,
    pub max_total_jobs: usize,
    pub max_ordinary_jobs: usize,
    pub max_ordinary_jobs_per_net: usize,
    pub route_scale: f64,
    pub unroutable_penalty_mm: f64,
}

impl MicroRouteConfig {
    pub fn board() -> Self {
        Self::two_layer(16, false)
    }

    pub fn block() -> Self {
        Self::two_layer(8, true)
    }

    pub fn post_place() -> Self {
        Self::two_layer(16, true)
    }

    fn two_layer(max_ordinary_jobs: usize, include_local_power: bool) -> Self {
        Self {
            layers: vec![
                RouteLayer {
                    name: Arc::from("top"),
                    preferred_direction: PreferredDirection::Horizontal,
                },
                RouteLayer {
                    name: Arc::from("bottom"),
                    preferred_direction: PreferredDirection::Vertical,
                },
            ],
            via_transitions: vec![
                ViaTransition { from: 0, to: 1 },
                ViaTransition { from: 1, to: 0 },
            ],
            grid: 0.25,
            trace_width: 0.127,
            clearance: 0.254,
            via_cost: 15.0,
            ordinary_via_cost: 10.0,
            high_via_cost: 30.0,
            critical_via_cost: 45.0,
            power_via_cost: 20.0,
            include_local_power,
            max_power_fanout: 4,
            max_expanded: 1_500,
            max_total_jobs: 32,
            max_ordinary_jobs,
            max_ordinary_jobs_per_net: 2,
            route_scale: 1.0,
            unroutable_penalty_mm: 30.0,
        }
    }
}

#[derive(Clone, Debug)]
struct RouteEndpoint {
    point: Point,
    layer: usize,
    primitive_id: Arc<str>,
    reference: Arc<str>,
    net: Arc<str>,
}

#[derive(Clone, Debug)]
struct RouteJob {
    net: Arc<str>,
    source: RouteEndpoint,
    target: RouteEndpoint,
    priority: u8,
    weight: f64,
    via_cost: f64,
    ordinary: bool,
}

#[derive(Clone, Debug)]
struct StaticObstacle {
    box_: Box2,
    layer: Option<usize>,
    primitive_id: Option<Arc<str>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Cell {
    x: i32,
    y: i32,
    layer: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct State {
    cell: Cell,
    direction: u8,
}

#[derive(Clone, Copy, Debug)]
struct PathCost {
    physical: f64,
    vias: usize,
    bends: usize,
    preference: usize,
}

impl PathCost {
    fn zero() -> Self {
        Self { physical: 0.0, vias: 0, bends: 0, preference: 0 }
    }
}

#[derive(Clone, Copy, Debug)]
struct OpenNode {
    state: State,
    g: PathCost,
    estimate: f64,
    serial: usize,
}

impl Eq for OpenNode {}
impl PartialEq for OpenNode {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state && self.estimate.to_bits() == other.estimate.to_bits()
    }
}
impl Ord for OpenNode {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_f64(other.estimate, self.estimate)
            .then_with(|| other.g.vias.cmp(&self.g.vias))
            .then_with(|| other.g.bends.cmp(&self.g.bends))
            .then_with(|| other.g.preference.cmp(&self.g.preference))
            .then_with(|| other.serial.cmp(&self.serial))
    }
}
impl PartialOrd for OpenNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

#[derive(Clone, Debug)]
struct RouteResult {
    cost: PathCost,
    cells: Vec<Cell>,
}

/// Bounded route-aware correction for a single placement candidate. The
/// caller is expected to invoke this only after cheap placement scoring has
/// produced a shortlist.
pub fn candidate_penalty(
    candidate: &Primitive,
    placed: &[&Primitive],
    relations: &[Relation],
    bounds: Box2,
    board_outline: &[Point],
    global_obstacles: &[Box2],
    config: &MicroRouteConfig,
) -> f64 {
    if config.layers.is_empty() || config.grid <= 0.0 || config.max_total_jobs == 0 {
        return 0.0;
    }

    let mut primitives: Vec<&Primitive> = Vec::with_capacity(placed.len() + 1);
    primitives.push(candidate);
    primitives.extend_from_slice(placed);
    let jobs = schedule_jobs(candidate, placed, relations, config);
    if jobs.is_empty() {
        return 0.0;
    }
    route_jobs_penalty(jobs, &primitives, bounds, board_outline, global_obstacles, config)
}

/// Re-score only routes affected by component-level post-place changes.
/// The caller supplies one primitive per component, so endpoint carving cannot
/// make unrelated siblings in a block or module transparent.
pub fn changed_layout_penalty(
    primitives: &[Primitive],
    relations: &[Relation],
    bounds: Box2,
    board_outline: &[Point],
    global_obstacles: &[Box2],
    changed_primitive_ids: &[String],
    config: &MicroRouteConfig,
) -> f64 {
    if config.layers.is_empty() || config.grid <= 0.0 || config.max_total_jobs == 0 {
        return 0.0;
    }
    let changed: HashSet<&str> = changed_primitive_ids.iter().map(String::as_str).collect();
    if changed.is_empty() {
        return 0.0;
    }
    let all: Vec<&Primitive> = primitives.iter().collect();
    let mut jobs = Vec::new();
    for candidate in primitives.iter().filter(|primitive| changed.contains(primitive.id.as_ref())) {
        let placed: Vec<&Primitive> = all
            .iter()
            .copied()
            .filter(|primitive| primitive.id != candidate.id)
            .collect();
        jobs.extend(schedule_jobs(candidate, &placed, relations, config));
    }
    let mut jobs = dedupe_jobs(jobs);
    jobs.sort_by(job_order);
    jobs.truncate(config.max_total_jobs);
    route_jobs_penalty(jobs, &all, bounds, board_outline, global_obstacles, config)
}

fn route_jobs_penalty(
    jobs: Vec<RouteJob>,
    primitives: &[&Primitive],
    bounds: Box2,
    board_outline: &[Point],
    global_obstacles: &[Box2],
    config: &MicroRouteConfig,
) -> f64 {
    if jobs.is_empty() {
        return 0.0;
    }
    let obstacles = collect_obstacles(primitives, global_obstacles, config);
    let mut temporary: HashMap<Cell, Arc<str>> = HashMap::new();
    let mut penalty = 0.0;
    for job in jobs {
        let baseline = baseline_cost(&job, config);
        let result = route_job(
            &job,
            bounds,
            board_outline,
            &obstacles,
            &temporary,
            config,
        );
        match result {
            Some(result) => {
                let detour = (result.cost.physical - baseline).max(0.0);
                penalty += detour * job.weight * config.route_scale;
                for cell in result.cells {
                    temporary.entry(cell).or_insert_with(|| job.net.clone());
                }
            }
            None => {
                penalty += config.unroutable_penalty_mm * job.weight * config.route_scale;
            }
        }
    }
    penalty
}

fn schedule_jobs(
    candidate: &Primitive,
    placed: &[&Primitive],
    relations: &[Relation],
    config: &MicroRouteConfig,
) -> Vec<RouteJob> {
    let mut explicit = explicit_jobs(candidate, placed, relations, config);
    explicit.sort_by(job_order);
    explicit = cap_priority_jobs(explicit, config.max_total_jobs);

    let explicit_pairs: HashSet<_> = explicit.iter().map(job_pair_key).collect();
    let remaining = config.max_total_jobs.saturating_sub(explicit.len());
    if remaining == 0 {
        return explicit;
    }

    let ordinary_limit = remaining.min(config.max_ordinary_jobs);
    let mut ordinary = ordinary_jobs(candidate, placed, relations, config)
        .into_iter()
        .filter(|job| !explicit_pairs.contains(&job_pair_key(job)))
        .collect::<Vec<_>>();
    ordinary = cap_per_net(ordinary, config.max_ordinary_jobs_per_net);
    ordinary = farthest_point_sample(ordinary, ordinary_limit);
    ordinary.sort_by(job_order);
    explicit.extend(ordinary);
    explicit
}

fn explicit_jobs(
    candidate: &Primitive,
    placed: &[&Primitive],
    relations: &[Relation],
    config: &MicroRouteConfig,
) -> Vec<RouteJob> {
    let mut all = Vec::with_capacity(placed.len() + 1);
    all.push(candidate);
    all.extend_from_slice(placed);
    let candidate_id = candidate.id.as_ref();
    let mut jobs = Vec::new();

    for relation in relations {
        let route_relevant = relation.relation.as_deref() == Some("critical_pair")
            || relation.path_id.is_some();
        if !route_relevant || relation.effect.as_ref() == "lock" {
            continue;
        }
        let left = resolve_endpoint(&relation.from, &all, config);
        let right = resolve_endpoint(&relation.to, &all, config);
        let Some((a, b)) = closest_shared_net_pair(&left, &right) else { continue };
        let touches_candidate = a.primitive_id.as_ref() == candidate_id
            || b.primitive_id.as_ref() == candidate_id;
        if !touches_candidate || a.primitive_id == b.primitive_id {
            continue;
        }
        let (source, target) = if a.primitive_id.as_ref() == candidate_id { (a, b) } else { (b, a) };
        let priority = priority_rank(relation.priority.as_deref());
        let configured = relation.weight.unwrap_or(70.0);
        let weight = priority_weight(priority) * (configured / 70.0).clamp(0.25, 4.0);
        let via_cost = route_via_cost(priority, source.net.as_ref(), false, config);
        jobs.push(RouteJob {
            net: source.net.clone(),
            source,
            target,
            priority,
            weight,
            via_cost,
            ordinary: false,
        });
    }
    dedupe_jobs(jobs)
}

fn ordinary_jobs(
    candidate: &Primitive,
    placed: &[&Primitive],
    relations: &[Relation],
    config: &MicroRouteConfig,
) -> Vec<RouteJob> {
    let marker_nets: HashSet<&str> = relations
        .iter()
        .filter_map(|relation| relation.from.strip_prefix(MARKER_PREFIX))
        .collect();
    let marker_mode = !marker_nets.is_empty();
    let candidate_by_net = points_by_net(candidate, config);
    let mut jobs = Vec::new();

    for target_primitive in placed {
        let target_by_net = points_by_net(target_primitive, config);
        for (net, source_points) in &candidate_by_net {
            if is_ground(net) {
                continue;
            }
            let power = is_power(net) || is_switching_power(net);
            if power {
                let fanout = net_primitive_fanout(candidate, placed, net.as_ref());
                if !config.include_local_power || fanout > config.max_power_fanout {
                    continue;
                }
            }
            if marker_mode && !marker_nets.contains(net.as_ref()) {
                continue;
            }
            let Some(target_points) = target_by_net.get(net) else { continue };
            if let Some((source, target)) = closest_pair(source_points, target_points) {
                let priority = if is_switching_power(net) { 3 } else if is_power(net) { 2 } else { 0 };
                let weight = if is_switching_power(net) { 2.5 } else if is_power(net) { 1.5 } else { 1.0 };
                let via_cost = route_via_cost(priority, net.as_ref(), true, config);
                jobs.push(RouteJob {
                    net: net.clone(),
                    source,
                    target,
                    priority,
                    weight,
                    via_cost,
                    ordinary: true,
                });
            }
        }
    }
    dedupe_jobs(jobs)
}

fn net_primitive_fanout(candidate: &Primitive, placed: &[&Primitive], net: &str) -> usize {
    usize::from(primitive_has_net(candidate, net))
        + placed.iter().filter(|primitive| primitive_has_net(primitive, net)).count()
}

fn primitive_has_net(primitive: &Primitive, net: &str) -> bool {
    primitive.connection_points.iter().any(|point| point.net.as_deref() == Some(net))
}

fn points_by_net(primitive: &Primitive, config: &MicroRouteConfig) -> HashMap<Arc<str>, Vec<RouteEndpoint>> {
    let layer = primitive_layer(primitive, config);
    let mut result: HashMap<Arc<str>, Vec<RouteEndpoint>> = HashMap::new();
    for point in primitive.connection_points.iter() {
        let Some(net) = point.net.as_ref() else { continue };
        result.entry(net.clone()).or_default().push(RouteEndpoint {
            point: Point { x: point.x, y: point.y },
            layer,
            primitive_id: primitive.id.clone(),
            reference: point.reference.clone(),
            net: net.clone(),
        });
    }
    result
}

fn resolve_endpoint(endpoint: &str, primitives: &[&Primitive], config: &MicroRouteConfig) -> Vec<RouteEndpoint> {
    if let Some(reference) = endpoint.strip_prefix("pad:") {
        for primitive in primitives {
            let layer = primitive_layer(primitive, config);
            if let Some(point) = primitive.connection_points.iter().find(|point| point.reference.as_ref() == reference) {
                if let Some(net) = point.net.as_ref() {
                    return vec![RouteEndpoint {
                        point: Point { x: point.x, y: point.y },
                        layer,
                        primitive_id: primitive.id.clone(),
                        reference: point.reference.clone(),
                        net: net.clone(),
                    }];
                }
            }
        }
        return Vec::new();
    }
    if let Some(designator) = endpoint.strip_prefix("component:") {
        let prefix = format!("{designator}.");
        return primitives
            .iter()
            .filter(|primitive| primitive.placements.iter().any(|p| p.designator.as_ref() == designator))
            .flat_map(|primitive| {
                let layer = primitive_layer(primitive, config);
                let prefix = prefix.clone();
                primitive.connection_points.iter().filter_map(move |point| {
                    let net = point.net.as_ref()?;
                    point.reference.starts_with(&prefix).then(|| RouteEndpoint {
                        point: Point { x: point.x, y: point.y },
                        layer,
                        primitive_id: primitive.id.clone(),
                        reference: point.reference.clone(),
                        net: net.clone(),
                    })
                })
            })
            .collect();
    }
    for (prefix, tree_prefix) in [("block:", "tree:block:"), ("module:", "tree:module:")] {
        if let Some(name) = endpoint.strip_prefix(prefix) {
            let target = format!("{tree_prefix}{name}");
            if let Some(primitive) = primitives.iter().find(|primitive| {
                primitive.source_node_ids.iter().any(|id| id.as_ref() == target)
                    || primitive.source_node_id.as_ref() == target
            }) {
                return points_by_net(primitive, config).into_values().flatten().collect();
            }
        }
    }
    Vec::new()
}

fn closest_shared_net_pair(a: &[RouteEndpoint], b: &[RouteEndpoint]) -> Option<(RouteEndpoint, RouteEndpoint)> {
    let mut best: Option<(RouteEndpoint, RouteEndpoint, f64)> = None;
    for left in a {
        for right in b {
            if left.net != right.net { continue; }
            let d = distance(left.point, right.point);
            if best.as_ref().is_none_or(|(_, _, best_d)| d < *best_d - EPS) {
                best = Some((left.clone(), right.clone(), d));
            }
        }
    }
    best.map(|(a, b, _)| (a, b))
}

fn closest_pair(a: &[RouteEndpoint], b: &[RouteEndpoint]) -> Option<(RouteEndpoint, RouteEndpoint)> {
    let mut best: Option<(RouteEndpoint, RouteEndpoint, f64)> = None;
    for left in a {
        for right in b {
            let d = distance(left.point, right.point);
            if best.as_ref().is_none_or(|(_, _, best_d)| d < *best_d - EPS) {
                best = Some((left.clone(), right.clone(), d));
            }
        }
    }
    best.map(|(a, b, _)| (a, b))
}

fn primitive_layer(primitive: &Primitive, config: &MicroRouteConfig) -> usize {
    primitive.placements.first()
        .and_then(|placement| config.layers.iter().position(|layer| layer.name.as_ref() == placement.layer.as_ref()))
        .unwrap_or(0)
}

fn collect_obstacles(
    primitives: &[&Primitive],
    global_obstacles: &[Box2],
    config: &MicroRouteConfig,
) -> Vec<StaticObstacle> {
    let mut result = Vec::new();
    for primitive in primitives {
        let layer = Some(primitive_layer(primitive, config));
        let boxes: Vec<Box2> = if primitive.collision_boxes.is_empty() {
            vec![primitive.bbox]
        } else {
            primitive.collision_boxes.as_ref().clone()
        };
        for box_ in boxes {
            result.push(StaticObstacle {
                box_: inflate_box(box_, config.clearance + config.trace_width / 2.0),
                layer,
                primitive_id: Some(primitive.id.clone()),
            });
        }
    }
    for box_ in global_obstacles {
        result.push(StaticObstacle {
            box_: inflate_box(*box_, config.clearance + config.trace_width / 2.0),
            layer: None,
            primitive_id: None,
        });
    }
    result
}

fn route_job(
    job: &RouteJob,
    bounds: Box2,
    board_outline: &[Point],
    obstacles: &[StaticObstacle],
    temporary: &HashMap<Cell, Arc<str>>,
    config: &MicroRouteConfig,
) -> Option<RouteResult> {
    let start_cell = point_to_cell(job.source.point, job.source.layer, bounds, config.grid);
    let goal_cell = point_to_cell(job.target.point, job.target.layer, bounds, config.grid);
    let start = State { cell: start_cell, direction: 4 };
    let mut open = BinaryHeap::new();
    let mut best: HashMap<State, PathCost> = HashMap::new();
    let mut previous: HashMap<State, State> = HashMap::new();
    let mut serial = 0usize;
    best.insert(start, PathCost::zero());
    open.push(OpenNode {
        state: start,
        g: PathCost::zero(),
        estimate: heuristic(start_cell, goal_cell, config, job.via_cost),
        serial,
    });
    let mut expanded = 0usize;

    while let Some(node) = open.pop() {
        if expanded >= config.max_expanded { break; }
        let Some(known) = best.get(&node.state).copied() else { continue; };
        if compare_cost(node.g, known) == Ordering::Greater { continue; }
        expanded += 1;
        if node.state.cell.x == goal_cell.x && node.state.cell.y == goal_cell.y && node.state.cell.layer == goal_cell.layer {
            let cells = reconstruct(node.state, start, &previous);
            return Some(RouteResult { cost: node.g, cells });
        }

        for (next, step_cost) in neighbors(node.state, config, job.via_cost) {
            if blocked(
                next.cell,
                start_cell,
                goal_cell,
                &job.source.primitive_id,
                &job.target.primitive_id,
                &job.net,
                bounds,
                board_outline,
                obstacles,
                temporary,
                config,
            ) { continue; }
            let next_cost = add_cost(node.g, step_cost);
            if best.get(&next).is_some_and(|old| compare_cost(next_cost, *old) != Ordering::Less) {
                continue;
            }
            best.insert(next, next_cost);
            previous.insert(next, node.state);
            serial += 1;
            open.push(OpenNode {
                state: next,
                g: next_cost,
                estimate: next_cost.physical + heuristic(next.cell, goal_cell, config, job.via_cost),
                serial,
            });
        }
    }
    None
}

fn neighbors(state: State, config: &MicroRouteConfig, via_cost: f64) -> Vec<(State, PathCost)> {
    let mut result = Vec::with_capacity(6);
    let moves = [(1, 0, 0u8), (-1, 0, 1u8), (0, 1, 2u8), (0, -1, 3u8)];
    for (dx, dy, direction) in moves {
        let layer = &config.layers[state.cell.layer];
        let horizontal = dx != 0;
        let preference = match layer.preferred_direction {
            PreferredDirection::Any => 0,
            PreferredDirection::Horizontal => usize::from(!horizontal),
            PreferredDirection::Vertical => usize::from(horizontal),
        };
        result.push((
            State {
                cell: Cell { x: state.cell.x + dx, y: state.cell.y + dy, layer: state.cell.layer },
                direction,
            },
            PathCost {
                physical: config.grid,
                vias: 0,
                bends: usize::from(state.direction < 4 && state.direction != direction),
                preference,
            },
        ));
    }
    for transition in &config.via_transitions {
        if transition.from != state.cell.layer || transition.to >= config.layers.len() { continue; }
        result.push((
            State {
                cell: Cell { layer: transition.to, ..state.cell },
                direction: 4,
            },
            PathCost { physical: via_cost, vias: 1, bends: 0, preference: 0 },
        ));
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn blocked(
    cell: Cell,
    start: Cell,
    goal: Cell,
    source_id: &Arc<str>,
    target_id: &Arc<str>,
    net: &Arc<str>,
    bounds: Box2,
    board_outline: &[Point],
    obstacles: &[StaticObstacle],
    temporary: &HashMap<Cell, Arc<str>>,
    config: &MicroRouteConfig,
) -> bool {
    let endpoint_carve = cell.layer == start.layer && chebyshev(cell, start) <= 1
        || cell.layer == goal.layer && chebyshev(cell, goal) <= 1;
    let point = cell_to_point(cell, bounds, config.grid);
    if point.x < bounds.left - EPS || point.x > bounds.right + EPS || point.y < bounds.top - EPS || point.y > bounds.bottom + EPS {
        return true;
    }
    let edge_clearance = config.clearance + config.trace_width / 2.0;
    if !endpoint_carve && !board_outline.is_empty() {
        if !point_in_polygon(&point, board_outline)
            || point_to_polygon_distance(&point, board_outline) + EPS < edge_clearance
        {
            return true;
        }
    }
    if !endpoint_carve {
        for obstacle in obstacles {
            if obstacle.layer.is_some_and(|layer| layer != cell.layer) { continue; }
            if obstacle.primitive_id.as_ref().is_some_and(|id| id == source_id || id == target_id) { continue; }
            if point_in_box(point, obstacle.box_) { return true; }
        }
    }

    let radius = ((config.trace_width + config.clearance) / config.grid).ceil() as i32;
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            let nearby = Cell { x: cell.x + dx, y: cell.y + dy, layer: cell.layer };
            if temporary.get(&nearby).is_some_and(|occupied_net| occupied_net != net) {
                return true;
            }
        }
    }
    false
}

fn baseline_cost(job: &RouteJob, config: &MicroRouteConfig) -> f64 {
    let planar = (job.source.point.x - job.target.point.x).abs()
        + (job.source.point.y - job.target.point.y).abs();
    let vias = minimum_vias(job.source.layer, job.target.layer, config).unwrap_or(0);
    planar + vias as f64 * job.via_cost
}

fn heuristic(cell: Cell, goal: Cell, config: &MicroRouteConfig, via_cost: f64) -> f64 {
    let planar = ((cell.x - goal.x).abs() + (cell.y - goal.y).abs()) as f64 * config.grid;
    let vias = minimum_vias(cell.layer, goal.layer, config).unwrap_or(0);
    planar + vias as f64 * via_cost
}

fn minimum_vias(from: usize, to: usize, config: &MicroRouteConfig) -> Option<usize> {
    if from == to { return Some(0); }
    let mut queue = VecDeque::from([(from, 0usize)]);
    let mut seen = HashSet::from([from]);
    while let Some((layer, distance)) = queue.pop_front() {
        for transition in &config.via_transitions {
            if transition.from != layer || !seen.insert(transition.to) { continue; }
            if transition.to == to { return Some(distance + 1); }
            queue.push_back((transition.to, distance + 1));
        }
    }
    None
}

fn reconstruct(mut state: State, start: State, previous: &HashMap<State, State>) -> Vec<Cell> {
    let mut cells = vec![state.cell];
    while state != start {
        let Some(parent) = previous.get(&state).copied() else { break };
        state = parent;
        if cells.last().copied() != Some(state.cell) { cells.push(state.cell); }
    }
    cells.reverse();
    cells
}

fn cap_priority_jobs(jobs: Vec<RouteJob>, limit: usize) -> Vec<RouteJob> {
    if jobs.len() <= limit { return jobs; }
    let mut result = Vec::new();
    for priority in (1..=4).rev() {
        if result.len() >= limit { break; }
        let bucket: Vec<_> = jobs.iter().filter(|job| job.priority == priority).cloned().collect();
        let capacity = limit - result.len();
        result.extend(farthest_point_sample(bucket, capacity));
    }
    result
}

fn cap_per_net(jobs: Vec<RouteJob>, limit: usize) -> Vec<RouteJob> {
    if limit == 0 { return Vec::new(); }
    let mut by_net: HashMap<Arc<str>, Vec<RouteJob>> = HashMap::new();
    for job in jobs { by_net.entry(job.net.clone()).or_default().push(job); }
    let mut nets: Vec<_> = by_net.into_iter().collect();
    nets.sort_by(|a, b| a.0.cmp(&b.0));
    nets.into_iter().flat_map(|(_, jobs)| farthest_point_sample(jobs, limit)).collect()
}

fn farthest_point_sample(mut jobs: Vec<RouteJob>, limit: usize) -> Vec<RouteJob> {
    if jobs.len() <= limit { jobs.sort_by(job_key_order); return jobs; }
    if limit == 0 { return Vec::new(); }
    jobs.sort_by(job_key_order);
    let centroid = jobs.iter().fold((0.0, 0.0, 0.0, 0.0), |acc, job| (
        acc.0 + job.source.point.x,
        acc.1 + job.source.point.y,
        acc.2 + job.target.point.x,
        acc.3 + job.target.point.y,
    ));
    let n = jobs.len() as f64;
    let centroid = (centroid.0 / n, centroid.1 / n, centroid.2 / n, centroid.3 / n);
    let first = (0..jobs.len()).max_by(|&a, &b| {
        compare_f64(feature_distance_to_centroid(&jobs[a], centroid), feature_distance_to_centroid(&jobs[b], centroid))
            .then_with(|| job_key(&jobs[b]).cmp(&job_key(&jobs[a])))
    }).unwrap_or(0);
    let mut selected = vec![jobs.remove(first)];
    while selected.len() < limit && !jobs.is_empty() {
        let next = (0..jobs.len()).max_by(|&a, &b| {
            compare_f64(min_feature_distance(&jobs[a], &selected), min_feature_distance(&jobs[b], &selected))
                .then_with(|| job_key(&jobs[b]).cmp(&job_key(&jobs[a])))
        }).unwrap_or(0);
        selected.push(jobs.remove(next));
    }
    selected
}

fn min_feature_distance(job: &RouteJob, selected: &[RouteJob]) -> f64 {
    selected.iter().map(|other| feature_distance(job, other)).fold(f64::INFINITY, f64::min)
}
fn feature_distance(a: &RouteJob, b: &RouteJob) -> f64 {
    distance(a.source.point, b.source.point) + distance(a.target.point, b.target.point)
}
fn feature_distance_to_centroid(job: &RouteJob, c: (f64, f64, f64, f64)) -> f64 {
    (job.source.point.x - c.0).hypot(job.source.point.y - c.1)
        + (job.target.point.x - c.2).hypot(job.target.point.y - c.3)
}

fn dedupe_jobs(jobs: Vec<RouteJob>) -> Vec<RouteJob> {
    let mut seen = HashSet::new();
    jobs.into_iter().filter(|job| seen.insert(job_pair_key(job))).collect()
}
fn job_pair_key(job: &RouteJob) -> (Arc<str>, Arc<str>, Arc<str>) {
    let (a, b) = if job.source.reference <= job.target.reference {
        (job.source.reference.clone(), job.target.reference.clone())
    } else {
        (job.target.reference.clone(), job.source.reference.clone())
    };
    (job.net.clone(), a, b)
}
fn job_key(job: &RouteJob) -> (u8, Arc<str>, Arc<str>, Arc<str>) {
    let (_, a, b) = job_pair_key(job);
    (job.priority, job.net.clone(), a, b)
}
fn job_key_order(a: &RouteJob, b: &RouteJob) -> Ordering { job_key(a).cmp(&job_key(b)) }
fn job_order(a: &RouteJob, b: &RouteJob) -> Ordering {
    b.priority.cmp(&a.priority).then_with(|| job_key_order(a, b))
}
fn priority_rank(value: Option<&str>) -> u8 {
    match value { Some("critical") => 4, Some("high") => 3, Some("low") => 1, _ => 2 }
}
fn priority_weight(priority: u8) -> f64 {
    match priority { 4 => 4.0, 3 => 2.5, 2 => 1.5, _ => 1.0 }
}

fn route_via_cost(priority: u8, net: &str, ordinary: bool, config: &MicroRouteConfig) -> f64 {
    if ordinary {
        if is_switching_power(net) {
            return config.high_via_cost.max(config.power_via_cost);
        }
        if is_power(net) {
            return config.power_via_cost;
        }
        return config.ordinary_via_cost;
    }
    match priority {
        4 => config.critical_via_cost,
        3 => config.high_via_cost,
        2 => config.via_cost,
        _ => config.ordinary_via_cost,
    }
}

fn add_cost(a: PathCost, b: PathCost) -> PathCost {
    PathCost {
        physical: a.physical + b.physical,
        vias: a.vias + b.vias,
        bends: a.bends + b.bends,
        preference: a.preference + b.preference,
    }
}
fn compare_cost(a: PathCost, b: PathCost) -> Ordering {
    compare_f64(a.physical, b.physical)
        .then_with(|| a.vias.cmp(&b.vias))
        .then_with(|| a.bends.cmp(&b.bends))
        .then_with(|| a.preference.cmp(&b.preference))
}
fn compare_f64(a: f64, b: f64) -> Ordering { a.partial_cmp(&b).unwrap_or(Ordering::Equal) }
fn point_to_cell(point: Point, layer: usize, bounds: Box2, grid: f64) -> Cell {
    Cell {
        x: ((point.x - bounds.left) / grid).round() as i32,
        y: ((point.y - bounds.top) / grid).round() as i32,
        layer,
    }
}
fn cell_to_point(cell: Cell, bounds: Box2, grid: f64) -> Point {
    Point { x: bounds.left + cell.x as f64 * grid, y: bounds.top + cell.y as f64 * grid }
}
fn chebyshev(a: Cell, b: Cell) -> i32 { (a.x - b.x).abs().max((a.y - b.y).abs()) }
fn point_in_box(point: Point, box_: Box2) -> bool {
    point.x + EPS >= box_.left && point.x - EPS <= box_.right && point.y + EPS >= box_.top && point.y - EPS <= box_.bottom
}
fn inflate_box(box_: Box2, value: f64) -> Box2 {
    Box2 { left: box_.left - value, right: box_.right + value, top: box_.top - value, bottom: box_.bottom + value }
}
fn distance(a: Point, b: Point) -> f64 { (a.x - b.x).hypot(a.y - b.y) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionPoint, Placement};

    fn primitive(id: &str, x: f64, y: f64, net: &str) -> Primitive {
        Primitive {
            id: Arc::from(id), kind: Arc::from("component"), label: Arc::from(id),
            source_node_id: Arc::from(format!("component:{id}")), source_node_ids: Arc::new(vec![]),
            locked: false, can_rotate: false, allowed_orientations: Arc::new(vec![0]),
            bbox: Box2 { left: x - 0.5, right: x + 0.5, top: y - 0.5, bottom: y + 0.5 },
            collision_boxes: Arc::new(vec![Box2 { left: x - 0.5, right: x + 0.5, top: y - 0.5, bottom: y + 0.5 }]),
            width: 1.0, height: 1.0,
            placements: Arc::new(vec![Placement { designator: Arc::from(id), x, y, rotate: 0, layer: Arc::from("top"), score: 0.0 }]),
            connection_points: Arc::new(vec![ConnectionPoint { x, y, reference: Arc::from(format!("{id}.1")), net: Some(Arc::from(net)) }]),
            path_ports: Arc::new(vec![]), edge_place: None,
        }
    }

    fn bounds() -> Box2 { Box2 { left: -10.0, right: 10.0, top: -10.0, bottom: 10.0 } }

    #[test]
    fn free_manhattan_route_has_no_detour_penalty() {
        let a = primitive("A", -4.0, 0.0, "SIG");
        let b = primitive("B", 4.0, 0.0, "SIG");
        let penalty = candidate_penalty(&a, &[&b], &[], bounds(), &[], &[], &MicroRouteConfig::board());
        assert!(penalty.abs() < 1e-6, "penalty={penalty}");
    }

    #[test]
    fn obstacle_creates_positive_detour_penalty() {
        let a = primitive("A", -4.0, 0.0, "SIG");
        let b = primitive("B", 4.0, 0.0, "SIG");
        let obstacle = Box2 { left: -1.0, right: 1.0, top: -2.0, bottom: 2.0 };
        let penalty = candidate_penalty(&a, &[&b], &[], bounds(), &[], &[obstacle], &MicroRouteConfig::board());
        assert!(penalty > 0.1, "penalty={penalty}");
    }

    #[test]
    fn config_is_multilayer_ready() {
        let mut config = MicroRouteConfig::board();
        config.layers.push(RouteLayer { name: Arc::from("inner1"), preferred_direction: PreferredDirection::Horizontal });
        config.layers.push(RouteLayer { name: Arc::from("inner2"), preferred_direction: PreferredDirection::Vertical });
        config.via_transitions.extend([
            ViaTransition { from: 1, to: 2 }, ViaTransition { from: 2, to: 1 },
            ViaTransition { from: 2, to: 3 }, ViaTransition { from: 3, to: 2 },
        ]);
        assert_eq!(minimum_vias(0, 3, &config), Some(3));
    }

    #[test]
    fn temporary_route_blocks_different_net_but_not_same_net() {
        let config = MicroRouteConfig::board();
        let cell = Cell { x: 5, y: 5, layer: 0 };
        let mut temporary = HashMap::new();
        temporary.insert(cell, Arc::from("A"));
        let source = Arc::from("S");
        let target = Arc::from("T");
        assert!(blocked(cell, cell, Cell { x: 8, y: 8, layer: 0 }, &source, &target, &Arc::from("B"), bounds(), &[], &[], &temporary, &config));
        assert!(!blocked(cell, cell, Cell { x: 8, y: 8, layer: 0 }, &source, &target, &Arc::from("A"), bounds(), &[], &[], &temporary, &config));
    }
}
