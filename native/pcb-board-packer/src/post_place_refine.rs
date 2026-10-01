//! Complete post-placement search. Input is decoded once; candidate evaluation
//! stays on scoped Rust threads with private baseline caches and ordered reduction.
use crate::geometry::{
    box_inside_polygon_board, js_round, normalize_rotation, union_boxes, Box2, Point,
};
use crate::model::{
    BoardPackProblem, ConnectionPoint, PathPort, Placement, PostPlaceClearance, PostPlaceDistance,
    PostPlaceEdge, PostPlaceNet, PostPlacePath, PostPlaceScoreProblem, RouteObstacle,
};
use crate::{
    micro_router::comparison::{self, RouteBaseline, RouteComparison},
    post_place,
};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
const EPS: f32 = 0.001;
#[cfg(feature = "gpu")]
#[path = "post_place/cubecl.rs"]
mod gpu;

#[derive(Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum RefineRoutingMetric { #[default] Micro, Geometric }

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefineProblem {
    version: u32,
    #[serde(default)]
    routing_metric: RefineRoutingMetric,
    #[serde(default)]
    pad_crossing_weight: f32,
    threads: usize,
    iterations: usize,
    timeout_ms: u64,
    min_delta: f32,
    placements: Vec<Placement>,
    components: Vec<Component>,
    groups: Vec<Group>,
    compatibility: Vec<Vec<i32>>,
    nets: Vec<Net>,
    hints: Vec<Hint>,
    hierarchy: Vec<Hierarchy>,
    route_problem: BoardPackProblem,
    board: Box2,
    polygon: Vec<Point>,
    edge_clearance: f32,
    holes: Vec<Hole>,
    regions: Vec<Region>,
    pair_clearances: Vec<Vec<f32>>,
    paths: Vec<Path>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Component {
    #[serde(default)]
    internal_pad_owner: bool,
    designator: Arc<str>,
    pose_index: usize,
    obstacle_offset: usize,
    lex_rank: usize,
    automatic: bool,
    fixed: bool,
    diagnostic: bool,
    allowed_rotations: Vec<i32>,
    allowed_layers: Vec<Arc<str>>,
    overflow: Overflow,
    through: bool,
    keys: Vec<PoseKey>,
    pads: Vec<Pad>,
    pins: Vec<Pin>,
    orientations: Vec<Orientation>,
}
#[derive(Deserialize, Serialize)]
struct Overflow {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}
#[derive(Deserialize, Serialize)]
struct PoseKey {
    x: f32,
    y: f32,
    rotate: i32,
    layer: Arc<str>,
    key: String,
    rank: usize,
}
#[derive(Deserialize, Serialize)]
struct Pad {
    #[serde(rename = "ref")]
    reference: Arc<str>,
    through: bool,
    #[serde(default)]
    opposite: bool,
    net: Option<Arc<str>>,
}
#[derive(Deserialize, Serialize)]
struct Pin {
    pad: usize,
    net: Arc<str>,
    #[serde(rename = "ref")]
    reference: Arc<str>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Orientation {
    rotate: i32,
    layer: Arc<str>,
    #[serde(rename = "box")]
    box_: Box2,
    body: Box2,
    opposite: Vec<Box2>,
    points: Vec<Point>,
    pad_boxes: Vec<Box2>,
}
#[derive(Deserialize, Serialize)]
struct Group {
    name: String,
    members: Vec<usize>,
    rotate: bool,
    swap: bool,
    deltas: Vec<i32>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Target {
    kind: String,
    component: Option<usize>,
    pad: Option<usize>,
    members: Option<Vec<usize>>,
    point: Option<Point>,
}
#[derive(Deserialize, Serialize)]
struct BoundPoint {
    component: usize,
    pad: usize,
}
#[derive(Deserialize, Serialize)]
struct Net {
    name: Arc<str>,
    points: Vec<BoundPoint>,
    weight: f32,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Hint {
    kind: String,
    source: Target,
    target: Target,
    all: bool,
    weight: f32,
    min: Option<f32>,
    max: Option<f32>,
    hard: Option<bool>,
    edge: Option<String>,
    layer: Option<Arc<str>>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Hierarchy {
    key: String,
    source: Target,
    max_width: Option<f32>,
    max_height: Option<f32>,
    anchor: Option<Target>,
    offset: Option<Point>,
    max_gap: Option<f32>,
}
#[derive(Deserialize, Serialize)]
struct Hole {
    x: f32,
    y: f32,
    radius: f32,
}
#[derive(Deserialize, Serialize)]
struct Region {
    name: String,
    #[serde(rename = "box")]
    box_: Box2,
    layers: Vec<Arc<str>>,
    allowed: Vec<usize>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Path {
    id: Arc<str>,
    shape: Arc<str>,
    priority: Arc<str>,
    prefer_facing_pads: bool,
    ports: Vec<Port>,
}
#[derive(Deserialize, Serialize)]
struct Port {
    target: Target,
    order: i32,
    #[serde(rename = "ref")]
    reference: Arc<str>,
    role: Arc<str>,
}

#[derive(Clone)]
struct Candidate {
    changes: Vec<(usize, Placement)>,
    key: String,
    ranks: Vec<usize>,
    kind: &'static str,
    description: String,
}
#[derive(Clone)]
struct World {
    placements: Vec<Placement>,
    boxes: Vec<Box2>,
    bodies: Vec<Box2>,
    opposite: Vec<Vec<Box2>>,
    points: Vec<Vec<Point>>,
    route: BoardPackProblem,
    obstacles: Vec<RouteObstacle>,
}
fn shift(b: Box2, p: &Placement) -> Box2 {
    Box2 {
        left: b.left + p.x,
        right: b.right + p.x,
        top: b.top + p.y,
        bottom: b.bottom + p.y,
    }
}
fn gap(a: Box2, b: Box2) -> f32 {
    (b.left - a.right)
        .max(a.left - b.right)
        .max((b.top - a.bottom).max(a.top - b.bottom))
}
fn point_gap(p: Point, b: Box2) -> f32 {
    crate::numerics::hypot((b.left - p.x).max(0.0).max(p.x - b.right),
        (b.top - p.y).max(0.0).max(p.y - b.bottom))
}
fn center(b: Box2) -> Point {
    Point {
        x: (b.left + b.right) / 2.0,
        y: (b.top + b.bottom) / 2.0,
    }
}
fn rounded(x: f32) -> f32 {
    js_round(x * 1000.0) / 1000.0
}

impl RefineProblem {
    fn validate(&self) -> Result<(), String> {
        if !matches!(self.version, 2 | 3)
            || self.timeout_ms > 30_000
            || !self.min_delta.is_finite()
            || self.min_delta < 0.0
        {
            return Err("invalid refine contract/options".into());
        }
        self.route_problem.validate(crate::CONTRACT_VERSION)?;
        let n = self.components.len();
        if self.route_problem.primitives.len() != n
            || self.components.iter().enumerate().any(|(i, c)| {
                self.route_problem.primitives[i].id.as_ref() != format!("post:{}", c.designator)
            })
        {
            return Err("invalid refine route primitive order".into());
        }
        if self.compatibility.len() != n
            || self.pair_clearances.len() != n
            || self.compatibility.iter().any(|r| r.len() != n)
            || self
                .pair_clearances
                .iter()
                .any(|r| r.len() != n || r.iter().any(|v| !v.is_finite() || *v < 0.0))
        {
            return Err("invalid refine pair matrices".into());
        }
        if self
            .placements
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.score.is_finite())
        {
            return Err("non-finite refine placements".into());
        }
        let mut offset = 0;
        for c in &self.components {
            if c.obstacle_offset != offset {
                return Err("invalid refine obstacle offset".into());
            }
            offset += c.pads.len();
            if c.pose_index >= self.placements.len()
                || self.placements[c.pose_index].designator != c.designator
                || c.orientations.is_empty()
                || c.keys.is_empty()
                || c.pins.iter().any(|p| p.pad >= c.pads.len())
                || c.orientations
                    .iter()
                    .any(|o| o.points.len() != c.pads.len() || o.pad_boxes.len() != c.pads.len())
            {
                return Err("invalid refine component geometry".into());
            }
        }
        let valid_target = |t: &Target| -> bool {
            match t.kind.as_str() {
                "missing" => true,
                "point" => t.point.is_some_and(|p| p.x.is_finite() && p.y.is_finite()),
                "group" => t.members.as_ref().is_some_and(|m| m.iter().all(|i| *i < n)),
                "component" => t.component.is_some_and(|i| i < n),
                "pin" => t.component.is_some_and(|i| {
                    i < n && t.pad.is_some_and(|j| j < self.components[i].pads.len())
                }),
                _ => false,
            }
        };
        if self
            .groups
            .iter()
            .any(|g| g.members.iter().any(|i| *i >= n))
            || self.nets.iter().any(|net| {
                net.points
                    .iter()
                    .any(|p| p.component >= n || p.pad >= self.components[p.component].pads.len())
            })
            || self
                .hints
                .iter()
                .any(|h| !valid_target(&h.source) || !valid_target(&h.target))
            || self.hierarchy.iter().any(|h| {
                !valid_target(&h.source) || h.anchor.as_ref().is_some_and(|a| !valid_target(a))
            })
            || self
                .paths
                .iter()
                .any(|p| p.ports.iter().any(|q| !valid_target(&q.target)))
        {
            return Err("invalid refine references".into());
        }
        Ok(())
    }
    fn world(&self, poses: &[Placement]) -> Result<World, String> {
        let empty = Box2 {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        };
        let n = self.components.len();
        let route = self.route_problem.clone();
        let obstacles = self
            .components
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                c.pads.iter().map({
                    let id = route.primitives[i].id.clone();
                    move |pad| RouteObstacle {
                        box_: empty,
                        layer: None,
                        reference: Some(pad.reference.clone()),
                        net: pad.net.clone(),
                        primitive_id: Some(id.clone()),
                    }
                })
            })
            .collect();
        let mut w = World {
            placements: poses.to_vec(),
            boxes: vec![empty; n],
            bodies: vec![empty; n],
            opposite: vec![vec![]; n],
            points: vec![vec![]; n],
            route,
            obstacles,
        };
        for (i, c) in self.components.iter().enumerate() {
            self.apply(&mut w, i, &poses[c.pose_index])?;
        }
        Ok(w)
    }
    fn apply(&self, w: &mut World, i: usize, pose: &Placement) -> Result<(), String> {
        let c = &self.components[i];
        let o = c
            .orientations
            .iter()
            .find(|o| o.rotate == normalize_rotation(pose.rotate) && o.layer == pose.layer)
            .ok_or_else(|| format!("missing orientation for {}", c.designator))?;
        w.placements[c.pose_index] = pose.clone();
        w.boxes[i] = shift(o.box_, pose);
        w.bodies[i] = shift(o.body, pose);
        w.opposite[i] = o.opposite.iter().map(|b| shift(*b, pose)).collect();
        w.points[i] = o
            .points
            .iter()
            .map(|p| Point {
                x: p.x + pose.x,
                y: p.y + pose.y,
            })
            .collect();
        let primitive = &mut w.route.primitives[i];
        let body = w.bodies[i];
        primitive.bbox = body;
        primitive.collision_boxes = Arc::new(std::iter::once(body).chain(w.opposite[i].iter().copied()).collect());
        let other_layer: Arc<str> = Arc::from(if pose.layer.as_ref() == "top" { "bottom" } else { "top" });
        primitive.collision_box_layers = Arc::new(std::iter::once(pose.layer.clone())
            .chain(std::iter::repeat_n(other_layer.clone(), w.opposite[i].len())).collect());
        primitive.width = body.right - body.left;
        primitive.height = body.bottom - body.top;
        primitive.placements = Arc::new(vec![pose.clone()]);
        primitive.connection_points = Arc::new(
            c.pins
                .iter()
                .map(|pin| ConnectionPoint {
                    x: w.points[i][pin.pad].x,
                    y: w.points[i][pin.pad].y,
                    reference: pin.reference.clone(),
                    net: Some(pin.net.clone()),
                })
                .collect(),
        );
        primitive.connection_point_layers = Arc::new(c.pins.iter().map(|pin| {
            let pad = &c.pads[pin.pad];
            if pad.through { Arc::from("multi") } else if pad.opposite { other_layer.clone() } else { pose.layer.clone() }
        }).collect());
        for (j, pad) in c.pads.iter().enumerate() {
            let obstacle = &mut w.obstacles[c.obstacle_offset + j];
            obstacle.box_ = shift(o.pad_boxes[j], pose);
            obstacle.layer = if pad.through {
                None
            } else {
                Some(if pad.opposite { other_layer.clone() } else { pose.layer.clone() })
            };
        }
        Ok(())
    }
    fn pose<'a>(&self, w: &'a World, i: usize) -> &'a Placement {
        &w.placements[self.components[i].pose_index]
    }
    fn target_box(&self, t: &Target, w: &World) -> Option<Box2> {
        match t.kind.as_str() {
            "component" => Some(w.boxes[t.component?]),
            "group" => {
                let b: Vec<_> = t.members.as_ref()?.iter().map(|i| w.boxes[*i]).collect();
                if b.is_empty() {
                    None
                } else {
                    Some(union_boxes(&b))
                }
            }
            _ => self.target_point(t, w).map(|p| Box2 {
                left: p.x,
                right: p.x,
                top: p.y,
                bottom: p.y,
            }),
        }
    }
    fn target_point(&self, t: &Target, w: &World) -> Option<Point> {
        match t.kind.as_str() {
            "point" => t.point,
            "pin" => Some(w.points[t.component?][t.pad?]),
            "component" => {
                let p = self.pose(w, t.component?);
                Some(Point { x: p.x, y: p.y })
            }
            "group" => self.target_box(t, w).map(center),
            _ => None,
        }
    }
    fn score(&self, w: &World) -> Result<f32, String> { post_place::score(&self.score_problem(w)) }
    fn score_problem(&self, w: &World) -> PostPlaceScoreProblem {
        let mut p = PostPlaceScoreProblem {
            version: 1,
            pad_crossing_weight: self.pad_crossing_weight,
            routing_obstacles: if self.pad_crossing_weight > 0.0 { w.obstacles.clone() } else { vec![] },
            nets: self
                .nets
                .iter()
                .map(|net| PostPlaceNet {
                    name: net.name.clone(),
                    internal_owners: net.points.iter().map(|b| {
                        let c = &self.components[b.component];
                        c.internal_pad_owner.then(|| c.designator.clone())
                    }).collect(),
                    weight: net.weight,
                    layers: net.points.iter().map(|b| w.obstacles[self.components[b.component].obstacle_offset + b.pad].layer.clone()).collect(),
                    points: net
                        .points
                        .iter()
                        .map(|b| w.points[b.component][b.pad])
                        .collect(),
                })
                .collect(),
            distances: vec![],
            clearances: vec![],
            fixed_penalties: vec![],
            edges: vec![],
            paths: vec![],
        };
        for h in &self.hints {
            match h.kind.as_str() {
                "distance" if !h.all => {
                    if let (Some(source), Some(target)) = (
                        self.target_point(&h.source, w),
                        self.target_point(&h.target, w),
                    ) {
                        p.distances.push(PostPlaceDistance {
                            source,
                            target,
                            weight: h.weight,
                            min: h.min,
                            max: h.max,
                        });
                    }
                }
                "clearance" => {
                    if let (Some(source), Some(minimum)) = (self.target_box(&h.source, w), h.min) {
                        for (_, target) in self.clearance_targets(h, w) {
                            p.clearances.push(PostPlaceClearance {
                                source,
                                target,
                                minimum,
                                weight: h.weight,
                            });
                        }
                    }
                }
                "same_side" if h.source.kind == "component" && h.target.kind == "component" => {
                    if self.pose(w, h.source.component.unwrap()).layer
                        != self.pose(w, h.target.component.unwrap()).layer
                    {
                        p.fixed_penalties.push(h.weight * 20.0);
                    }
                }
                "prefer_layer" if h.source.kind == "component" => {
                    if h.layer
                        .as_ref()
                        .is_some_and(|l| *l != self.pose(w, h.source.component.unwrap()).layer)
                    {
                        p.fixed_penalties.push(h.weight * 10.0);
                    }
                }
                "edge" => {
                    if let (Some(source), Some(edge)) = (self.target_box(&h.source, w), &h.edge) {
                        p.edges.push(PostPlaceEdge {
                            source,
                            board: self.board,
                            edge: Arc::from(edge.as_str()),
                            weight: h.weight,
                        });
                    }
                }
                _ => {}
            }
        }
        for path in &self.paths {
            let ports = path
                .ports
                .iter()
                .filter_map(|port| {
                    let point = self.target_point(&port.target, w)?;
                    let i = port.target.component?;
                    let pose = self.pose(w, i);
                    let dx = point.x - pose.x;
                    let dy = point.y - pose.y;
                    let len = crate::numerics::hypot(dx,dy);
                    Some(PathPort {
                        x: point.x,
                        y: point.y,
                        path_id: path.id.clone(),
                        order: port.order,
                        reference: port.reference.clone(),
                        role: port.role.clone(),
                        normal: if len > EPS {
                            Point {
                                x: dx / len,
                                y: dy / len,
                            }
                        } else {
                            Point { x: 0.0, y: 0.0 }
                        },
                    })
                })
                .collect();
            p.paths.push(PostPlacePath {
                path_id: path.id.clone(),
                ports,
                shape: path.shape.clone(),
                priority: path.priority.clone(),
                weight: 1.0,
                prefer_facing_pads: path.prefer_facing_pads,
            });
        }
        p
    }
    fn clearance_targets(&self, h: &Hint, w: &World) -> Vec<(String, Box2)> {
        if h.all {
            self.components
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    !(matches!(h.source.kind.as_str(), "component" | "pin")
                        && h.source.component == Some(*i))
                })
                .map(|(i, c)| (c.designator.to_string(), w.boxes[i]))
                .collect()
        } else {
            self.target_box(&h.target, w)
                .map(|b| vec![("target".into(), b)])
                .unwrap_or_default()
        }
    }
    fn violations(&self, w: &World, changed: &[usize]) -> FxHashSet<String> {
        let mut keys = FxHashSet::default();
        for &i in changed {
            let c = &self.components[i];
            let p = self.pose(w, i);
            let b = w.boxes[i];
            let prefix = format!("component:{}", c.designator);
            if !c.allowed_rotations.contains(&normalize_rotation(p.rotate)) {
                keys.insert(format!("{prefix}:rotation"));
            }
            if !c.allowed_layers.contains(&p.layer) {
                keys.insert(format!("{prefix}:layer"));
            }
            let o = &c.overflow;
            let overflow = o.left > 0.0 || o.right > 0.0 || o.top > 0.0 || o.bottom > 0.0;
            let outside = if !self.polygon.is_empty() && !overflow {
                !box_inside_polygon_board(
                    &b,
                    &self.board,
                    &self.polygon,
                    (self.edge_clearance - 0.010001).max(0.0),
                )
            } else {
                b.left
                    < (if o.left > 0.0 {
                        self.board.left - o.left
                    } else {
                        self.board.left + self.edge_clearance
                    }) - 0.010001
                    || b.right
                        > (if o.right > 0.0 {
                            self.board.right + o.right
                        } else {
                            self.board.right - self.edge_clearance
                        }) + 0.010001
                    || b.top
                        < (if o.top > 0.0 {
                            self.board.top - o.top
                        } else {
                            self.board.top + self.edge_clearance
                        }) - 0.010001
                    || b.bottom
                        > (if o.bottom > 0.0 {
                            self.board.bottom + o.bottom
                        } else {
                            self.board.bottom - self.edge_clearance
                        }) + 0.010001
            };
            if outside {
                keys.insert(format!("{prefix}:outside-board"));
            }
            for (j, h) in self.holes.iter().enumerate() {
                if point_gap(Point { x: h.x, y: h.y }, b) + EPS < h.radius {
                    keys.insert(format!("{prefix}:board-hole:{j}"));
                }
            }
            for (j, r) in self.regions.iter().enumerate() {
                if !r.allowed.contains(&i)
                    && std::iter::once((&w.bodies[i], p.layer.as_ref()))
                        .chain(w.opposite[i].iter().map(|b| (b, if p.layer.as_ref() == "top" { "bottom" } else { "top" })))
                        .any(|(b, layer)| r.layers.iter().any(|l| l.as_ref() == layer)
                            && b.left < r.box_.right - EPS && b.right > r.box_.left + EPS
                            && b.top < r.box_.bottom - EPS && b.bottom > r.box_.top + EPS)
                {
                    keys.insert(format!("{prefix}:region:{}:{j}", r.name));
                }
            }
        }
        for a in 0..self.components.len() {
            for b in a + 1..self.components.len() {
                if !changed.contains(&a) && !changed.contains(&b) {
                    continue;
                }
                let same = self.pose(w, a).layer == self.pose(w, b).layer;
                if !same && !self.components[a].through && !self.components[b].through {
                    continue;
                }
                let required = self.pair_clearances[a][b];
                let collision = if same {
                    gap(w.bodies[a], w.bodies[b]) + EPS < required
                        || w.opposite[a].iter().any(|aa| w.opposite[b].iter()
                            .any(|bb| gap(*aa, *bb) + EPS < required))
                } else {
                    w.opposite[b]
                        .iter()
                        .any(|bb| gap(w.bodies[a], *bb) + EPS < required)
                        || w.opposite[a]
                            .iter()
                            .any(|aa| gap(*aa, w.bodies[b]) + EPS < required)
                };
                if collision {
                    keys.insert(format!(
                        "collision:{}:{}",
                        self.components[a].designator, self.components[b].designator
                    ));
                }
            }
        }
        for (i, h) in self
            .hints
            .iter()
            .enumerate()
            .filter(|(_, h)| h.hard == Some(true))
        {
            let prefix = format!("hint:{i}");
            match h.kind.as_str() {
                "distance" if !h.all => {
                    if let (Some(a), Some(b)) = (
                        self.target_point(&h.source, w),
                        self.target_point(&h.target, w),
                    ) {
                        let d = crate::numerics::hypot(a.x - b.x,a.y - b.y);
                        if h.min.is_some_and(|min| d + EPS < min) {
                            keys.insert(format!("{prefix}:min"));
                        }
                        if h.max.is_some_and(|max| d > max + EPS) {
                            keys.insert(format!("{prefix}:max"));
                        }
                    } else {
                        keys.insert(format!("{prefix}:unresolved"));
                    }
                }
                "clearance" => {
                    if let Some(source) = self.target_box(&h.source, w) {
                        if let Some(min) = h.min {
                            for (key, target) in self.clearance_targets(h, w) {
                                if gap(source, target) + EPS < min {
                                    keys.insert(format!("{prefix}:clearance:{key}"));
                                }
                            }
                        }
                    } else {
                        keys.insert(format!("{prefix}:unresolved-source"));
                    }
                }
                "edge" => {
                    if let (Some(source), Some(edge)) = (self.target_box(&h.source, w), &h.edge) {
                        let d = self.edge_gap(source, edge);
                        if h.min.is_some_and(|min| d + EPS < min) {
                            keys.insert(format!("{prefix}:min"));
                        }
                        if h.max.is_some_and(|max| d > max + EPS) {
                            keys.insert(format!("{prefix}:max"));
                        }
                    } else {
                        keys.insert(format!("{prefix}:unresolved-source"));
                    }
                }
                _ => {}
            }
        }
        for h in &self.hierarchy {
            if let Some(b) = self.target_box(&h.source, w) {
                if h.max_width.is_some_and(|v| b.right - b.left > v + EPS)
                    || h.max_height.is_some_and(|v| b.bottom - b.top > v + EPS)
                {
                    keys.insert(h.key.clone());
                }
                if let Some(anchor) = &h.anchor {
                    let offset = h.offset.unwrap_or(Point { x: 0.0, y: 0.0 });
                    let p = self.target_point(anchor, w).map(|p| Point {
                        x: p.x + offset.x,
                        y: p.y + offset.y,
                    });
                    if p.is_none_or(|p| point_gap(p, b) > h.max_gap.unwrap_or(0.0) + EPS) {
                        keys.insert(h.key.clone());
                    }
                }
            }
        }
        keys
    }
    fn edge_gap(&self, b: Box2, edge: &str) -> f32 {
        match edge {
            "left" => (b.left - self.board.left).abs(),
            "right" => (self.board.right - b.right).abs(),
            "top" => (b.top - self.board.top).abs(),
            _ => (self.board.bottom - b.bottom).abs(),
        }
    }
    fn candidate(
        &self,
        mut changes: Vec<(usize, Placement)>,
        kind: &'static str,
        description: String,
    ) -> Result<Candidate, String> {
        changes.sort_by_key(|(i, _)| self.components[*i].lex_rank);
        let mut keys = Vec::new();
        let mut ranks = Vec::new();
        for (i, p) in &changes {
            let key = self.components[*i]
                .keys
                .iter()
                .find(|k| k.x == p.x && k.y == p.y && k.rotate == p.rotate && k.layer == p.layer)
                .ok_or_else(|| format!("missing reachable pose key for {}", p.designator))?;
            keys.push(key.key.as_str());
            ranks.push(key.rank);
        }
        Ok(Candidate {
            key: keys.join("|"),
            ranks,
            changes,
            kind,
            description,
        })
    }
    fn swaps(
        &self,
        current: &World,
        a: usize,
        b: usize,
        deltas: &[i32],
        name: &str,
    ) -> Result<Vec<Candidate>, String> {
        let offset = self.compatibility[a][b];
        if offset < 0 {
            return Ok(vec![]);
        }
        let ap = self.pose(current, a);
        let bp = self.pose(current, b);
        let mut result = Vec::new();
        for &ad in deltas {
            for &bd in deltas {
                let mut pa = ap.clone();
                pa.x = bp.x;
                pa.y = bp.y;
                pa.layer = bp.layer.clone();
                pa.rotate = crate::geometry::add_rotation(crate::geometry::add_rotation(bp.rotate, offset), ad);
                let mut pb = bp.clone();
                pb.x = ap.x;
                pb.y = ap.y;
                pb.layer = ap.layer.clone();
                pb.rotate = crate::geometry::add_rotation(crate::geometry::subtract_rotation(ap.rotate, offset), bd);
                if !self.components[a].allowed_rotations.contains(&pa.rotate)
                    || !self.components[b].allowed_rotations.contains(&pb.rotate)
                {
                    continue;
                }
                let suffix = [
                    if ad != 0 {
                        format!("{}+=180", ap.designator)
                    } else {
                        String::new()
                    },
                    if bd != 0 {
                        format!("{}+=180", bp.designator)
                    } else {
                        String::new()
                    },
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
                let description = format!(
                    "{}{}<->{}{}",
                    if name.is_empty() {
                        String::new()
                    } else {
                        format!("{name}: ")
                    },
                    ap.designator,
                    bp.designator,
                    if suffix.is_empty() {
                        String::new()
                    } else {
                        format!("; {suffix}")
                    }
                );
                result.push(self.candidate(vec![(a, pa), (b, pb)], "swap", description)?);
            }
        }
        Ok(result)
    }
    fn candidates(&self, current: &World) -> Result<Vec<Candidate>, String> {
        let mut result = Vec::new();
        let mut rotate = |i: usize, name: &str| -> Result<(), String> {
            let mut pose = self.pose(current, i).clone();
            pose.rotate = crate::geometry::add_rotation(pose.rotate, 180);
            if self.components[i].allowed_rotations.contains(&pose.rotate) {
                result.push(self.candidate(
                    vec![(i, pose)],
                    "rotate_180",
                    format!(
                        "{}{} rotate += 180",
                        if name.is_empty() {
                            String::new()
                        } else {
                            format!("{name}: ")
                        },
                        self.components[i].designator
                    ),
                )?);
            }
            Ok(())
        };
        for (i, c) in self.components.iter().enumerate() {
            if c.automatic {
                rotate(i, "")?;
            }
        }
        for g in &self.groups {
            if g.rotate {
                for &i in &g.members {
                    rotate(i, &g.name)?;
                }
            }
        }
        for g in &self.groups {
            if g.swap {
                for a in 0..g.members.len() {
                    for b in a + 1..g.members.len() {
                        result.extend(self.swaps(
                            current,
                            g.members[a],
                            g.members[b],
                            &g.deltas,
                            &g.name,
                        )?);
                    }
                }
            }
        }
        let mut seen = FxHashSet::default();
        result.retain(|c| seen.insert(c.key.clone()));
        result.sort_by(|a, b| a.ranks.cmp(&b.ranks));
        Ok(result)
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchProfile {
    candidates: usize,
    hard_rejected: usize,
    bound_rejected: usize,
    feasibility_rejected: usize,
    insufficient_improvement: usize,
    baseline_evaluations: usize,
    baseline_cache_hits: usize,
    route_evaluations: usize,
    route_encoding_ms: f64,
    route_native_ms: f64,
    score_encoding_ms: f64,
    score_native_ms: f64,
    geometry_ms: f64,
    global_score_ms: f64,
    baseline_ms: f64,
    route_ms: f64,
}
impl BatchProfile {
    fn add(&mut self, b: &Self) {
        self.candidates += b.candidates;
        self.hard_rejected += b.hard_rejected;
        self.bound_rejected += b.bound_rejected;
        self.feasibility_rejected += b.feasibility_rejected;
        self.insufficient_improvement += b.insufficient_improvement;
        self.baseline_evaluations += b.baseline_evaluations;
        self.baseline_cache_hits += b.baseline_cache_hits;
        self.route_evaluations += b.route_evaluations;
        self.geometry_ms += b.geometry_ms;
        self.global_score_ms += b.global_score_ms;
        self.score_native_ms += b.score_native_ms;
        self.baseline_ms += b.baseline_ms;
        self.route_ms += b.route_ms;
        self.route_native_ms += b.route_native_ms;
    }
}
struct Cache {
    violations: FxHashSet<String>,
    baseline: Option<RouteBaseline>,
}
struct Evaluation {
    score: f32,
    comparison: RouteComparison,
    improvement: f32,
}
fn elapsed(s: Instant) -> f64 {
    s.elapsed().as_secs_f64() * 1000.0
}
fn evaluate(
    p: &RefineProblem,
    current: &World,
    current_score: f32,
    c: &Candidate,
    scratch: &mut World,
    cache: &mut FxHashMap<Vec<usize>, Cache>,
    stats: &mut BatchProfile,
    incumbent: Option<f32>,
    precomputed: Option<f32>,
    deadline: Instant,
) -> Result<Option<Evaluation>, String> {
    if Instant::now() >= deadline {
        return Ok(None);
    }
    let previous: Vec<_> = c
        .changes
        .iter()
        .map(|(i, _)| (*i, scratch.placements[p.components[*i].pose_index].clone()))
        .collect();
    for (i, pose) in &c.changes {
        p.apply(scratch, *i, pose)?;
    }
    let outcome = (|| -> Result<Option<Evaluation>, String> {
        stats.candidates += 1;
        let changed: Vec<_> = c.changes.iter().map(|(i, _)| *i).collect();
        let started = Instant::now();
        let entry = cache.entry(changed.clone()).or_insert_with(|| Cache {
            violations: if precomputed.is_some() { FxHashSet::default() } else { p.violations(current, &changed) },
            baseline: None,
        });
        let world = &*scratch;
        let valid = precomputed.is_some() || p.violations(world, &changed).is_subset(&entry.violations);
        stats.geometry_ms += elapsed(started);
        if !valid {
            stats.hard_rejected += 1;
            return Ok(None);
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let started = Instant::now();
        let score = match precomputed { Some(score) => score, None => p.score(world)? };
        let time = elapsed(started);
        stats.global_score_ms += time;
        stats.score_native_ms += time;
        if Instant::now() >= deadline {
            return Ok(None);
        }
        // Local block refinement uses the same geometric objective as its
        // candidate pool. Only the final board stage requests Micro-A*.
        if p.routing_metric == RefineRoutingMetric::Geometric {
            let improvement = current_score - score;
            if improvement <= p.min_delta {
                stats.insufficient_improvement += 1;
                return Ok(None);
            }
            return Ok(Some(Evaluation { score, comparison: RouteComparison::default(), improvement }));
        }
        if entry.baseline.is_none() {
            let started = Instant::now();
            let ids: Vec<_> = changed
                .iter()
                .map(|i| format!("post:{}", p.components[*i].designator))
                .collect();
            entry.baseline = Some(comparison::prepare(
                &current.route,
                &current.obstacles,
                &ids,
            ));
            let time = elapsed(started);
            stats.baseline_ms += time;
            stats.route_native_ms += time;
            stats.baseline_evaluations += 1;
        } else {
            stats.baseline_cache_hits += 1;
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let baseline = entry.baseline.as_ref().unwrap();
        if let Some(upper)=comparison::total_improvement_ceiling(baseline,current_score,score) {
            use crate::interval::Interval as I;
            if upper < p.min_delta
                || incumbent.is_some_and(|best| upper < I::exact(best).sub(I::exact(EPS)).lo)
            {
                stats.bound_rejected += 1;
                return Ok(None);
            }
        }
        let started = Instant::now();
        let comparison = comparison::compare(&world.route, &world.obstacles, baseline)?;
        let time = elapsed(started);
        stats.route_ms += time;
        stats.route_native_ms += time;
        stats.route_evaluations += 1;
        if comparison.feasibility_order > 0 {
            stats.feasibility_rejected += 1;
            return Ok(None);
        }
        let improvement =
            (current_score + comparison.before_penalty) - (score + comparison.after_penalty);
        if improvement <= p.min_delta {
            stats.insufficient_improvement += 1;
            return Ok(None);
        }
        Ok(Some(Evaluation {
            score,
            comparison,
            improvement,
        }))
    })();
    // Revert every changed pose, including rejected and failed evaluations.
    for (i, pose) in &previous {
        p.apply(scratch, *i, pose)?;
    }
    outcome
}
fn iteration(
    p: &RefineProblem,
    current: &World,
    current_score: f32,
    candidates: &[Candidate],
    threads: usize,
    deadline: Instant,
    precomputed: Option<&[Option<f32>]>,
) -> Result<(Vec<Option<Evaluation>>, BatchProfile), String> {
    if threads <= 1 {
        let mut cache = FxHashMap::default();
        let mut stats = BatchProfile::default();
        let mut result = Vec::new();
        let mut best: Option<f32> = None;
        let mut scratch = current.clone();
        for (i, c) in candidates.iter().enumerate() {
            if precomputed.is_some_and(|scores| scores[i].is_none()) { result.push(None); continue; }
            if Instant::now() >= deadline {
                break;
            }
            let value = evaluate(
                p,
                current,
                current_score,
                c,
                &mut scratch,
                &mut cache,
                &mut stats,
                best,
                precomputed.and_then(|scores|scores[i]),
                deadline,
            )?;
            if let Some(v) = &value {
                if best.is_none_or(|b| v.improvement > b + EPS) {
                    best = Some(v.improvement);
                }
            }
            result.push(value);
        }
        return Ok((result, stats));
    }
    let mut group_map = FxHashMap::<Vec<usize>, usize>::default();
    let mut groups: Vec<Vec<usize>> = vec![];
    for (i, c) in candidates.iter().enumerate() {
        let key = c.changes.iter().map(|(i, _)| *i).collect::<Vec<_>>();
        let g = *group_map.entry(key).or_insert_with(|| {
            groups.push(vec![]);
            groups.len() - 1
        });
        groups[g].push(i);
    }
    let next = AtomicUsize::new(0);
    let workers = threads.min(groups.len());
    let batches = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let groups = &groups;
                let next = &next;
                scope.spawn(move || -> Result<_, String> {
                    let _float_env = crate::float_env::Guard::enter();
                    let mut stats = BatchProfile::default();
                    let mut result = Vec::new();
                    let mut scratch = current.clone();
                    loop {
                        if Instant::now() >= deadline {
                            break;
                        }
                        let g = next.fetch_add(1, Ordering::Relaxed);
                        if g >= groups.len() {
                            break;
                        }
                        let mut cache = FxHashMap::default();
                        for &i in &groups[g] {
                            if precomputed.is_some_and(|scores| scores[i].is_none()) { continue; }
                            if Instant::now() >= deadline {
                                break;
                            }
                            result.push((
                                i,
                                evaluate(
                                    p,
                                    current,
                                    current_score,
                                    &candidates[i],
                                    &mut scratch,
                                    &mut cache,
                                    &mut stats,
                                    None,
                                    precomputed.and_then(|scores|scores[i]),
                                    deadline,
                                )?,
                            ));
                        }
                    }
                    Ok((result, stats))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .map_err(|_| "post-place native worker panicked".to_string())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut stats = BatchProfile::default();
    let mut result: Vec<_> = (0..candidates.len()).map(|_| None).collect();
    for (values, b) in batches {
        stats.add(&b);
        for (i, value) in values {
            result[i] = value;
        }
    }
    Ok((result, stats))
}

impl RefineProblem {
    fn diagnostics(
        &self,
        current: &World,
        current_score: f32,
        deadline: Instant,
    ) -> Result<Vec<Value>, String> {
        let fixed: Vec<_> = self
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.fixed && c.diagnostic)
            .map(|(i, _)| i)
            .collect();
        let mut paired = FxHashSet::default();
        let mut diagnostics = Vec::new();
        for a in 0..fixed.len() {
            for b in a + 1..fixed.len() {
                if Instant::now() >= deadline {
                    return Ok(diagnostics);
                }
                let a = fixed[a];
                let b = fixed[b];
                let variants = self.swaps(current, a, b, &[0, 180], "")?;
                let changed = vec![a, b];
                let before = self.violations(current, &changed);
                let mut best: Option<(Candidate, f32)> = None;
                for c in variants {
                    if Instant::now() >= deadline {
                        return Ok(diagnostics);
                    }
                    let mut poses = current.placements.clone();
                    for (i, p) in &c.changes {
                        poses[self.components[*i].pose_index] = p.clone();
                    }
                    let w = self.world(&poses)?;
                    if !self.violations(&w, &changed).is_subset(&before) {
                        continue;
                    }
                    let score = self.score(&w)?;
                    if score + self.min_delta >= current_score
                        || best.as_ref().is_some_and(|(_, s)| score >= s - EPS)
                    {
                        continue;
                    }
                    best = Some((c, score));
                }
                if let Some((c, score)) = best {
                    paired.insert(a);
                    paired.insert(b);
                    let da = &self.components[a].designator;
                    let db = &self.components[b].designator;
                    let options = if c.description.contains("+=180") {
                        "{ swap: true, rotateBy: [180] }"
                    } else {
                        "{ swap: true }"
                    };
                    diagnostics.push(json!({"severity":"warning","code":"post_place_opportunity","nodeId":format!("post-place:{da}:{db}"),"message":format!("{da}/{db}: safe post-place improvement {}; global score {} -> {}. It was not applied because fixed placement must be preserved. Add refineGroup(\"post_{da}_{db}\", [\"{da}\", \"{db}\"], {options}) if these fixed components may swap or rotate.",c.description,rounded(current_score),rounded(score))}));
                }
            }
        }
        for i in fixed {
            if Instant::now() >= deadline {
                return Ok(diagnostics);
            }
            if paired.contains(&i) {
                continue;
            }
            let mut pose = self.pose(current, i).clone();
            pose.rotate = crate::geometry::add_rotation(pose.rotate, 180);
            if !self.components[i].allowed_rotations.contains(&pose.rotate) {
                continue;
            }
            let mut poses = current.placements.clone();
            poses[self.components[i].pose_index] = pose;
            let w = self.world(&poses)?;
            if !self
                .violations(&w, &[i])
                .is_subset(&self.violations(current, &[i]))
            {
                continue;
            }
            let score = self.score(&w)?;
            if score + self.min_delta >= current_score {
                continue;
            }
            let d = &self.components[i].designator;
            diagnostics.push(json!({"severity":"warning","code":"post_place_opportunity","nodeId":format!("post-place:{d}"),"message":format!("{d}: safe post-place improvement {d} rotate += 180; global score {} -> {}. It was not applied because fixed placement must be preserved. Add refineGroup(\"post_{d}\", [\"{d}\"], {{ rotateBy: [180] }}) if this fixed component may rotate.",rounded(current_score),rounded(score))}));
        }
        Ok(diagnostics)
    }
}
/// Absolute admission check for checkpoints: no baseline violations are exempt.
pub fn validate_layout(p: RefineProblem) -> Result<bool, String> {
    validate_layout_scope(p, None)
}

/// Scope only the editable family, while testing its collisions against every
/// fixed obstacle. Pre-existing edge violations of unrelated mechanics are not
/// attributed to the new family. Hard electrical constraints remain checked.
pub fn validate_layout_scope(p: RefineProblem, scope: Option<Vec<String>>) -> Result<bool, String> {
    p.validate()?;
    let world = p.world(&p.placements)?;
    let all: Vec<_> = if let Some(names) = scope {
        let mut indices = Vec::new();
        for name in names {
            indices.push(p.components.iter().position(|c| c.designator.as_ref() == name)
                .ok_or_else(|| format!("Unknown validation component {name}"))?);
        }
        indices
    } else { (0..p.components.len()).collect() };
    Ok(p.violations(&world, &all).is_empty())
}

/// Compare against a baseline during refinement; existing violations may not gain new keys.
pub fn validate_change(p: RefineProblem, placements: Vec<Placement>) -> Result<bool, String> {
    p.validate()?;
    if placements.len()!=p.placements.len() {return Ok(false);}
    let by_name: FxHashMap<_,_> = placements.iter().map(|q|(&q.designator,q)).collect();
    if by_name.len()!=placements.len() {return Ok(false);}
    let mut ordered=Vec::new();
    for original in &p.placements {
        let Some(pose)=by_name.get(&original.designator) else {return Ok(false);};
        if !pose.x.is_finite() || !pose.y.is_finite() {return Ok(false);}
        ordered.push((*pose).clone());
    }
    let before = p.world(&p.placements)?;
    let after = match p.world(&ordered) { Ok(w)=>w, Err(_)=>return Ok(false) };
    let changed: Vec<usize> = (0..p.components.len()).filter(|&i| {
        let a=p.pose(&before,i);let b=p.pose(&after,i);
        a.x!=b.x || a.y!=b.y || a.rotate!=b.rotate || a.layer!=b.layer
    }).collect();
    Ok(p.violations(&after,&changed).is_subset(&p.violations(&before,&changed)))
}

pub fn solve(p: RefineProblem) -> Result<Value, String> {
    p.validate()?;
    let backend=std::env::var("PCB_POST_PLACE_BACKEND").unwrap_or_else(|_|"auto".into());
    if !matches!(backend.as_str(),"cpu"|"cubecl"|"auto") {return Err(format!("unsupported PCB_POST_PLACE_BACKEND {backend}"));}
    #[cfg(feature="gpu")]
    if backend=="cubecl" || (backend=="auto" && gpu::profitable(&p)) {
        let started=Instant::now();
        match gpu::Engine::new(&p) {
            Ok(engine)=>{
                let mut engine=Some(engine);
                let result=solve_inner(&p,started,&mut engine);
                let engine=engine.unwrap();
                if let Some(error)=&engine.failure {
                    eprintln!("[post-place-gpu-fallback] {}",json!({"reason":error.to_string(),"kind":error.kind,"discardedGpuMs":elapsed(started),"cpuBudgetMs":p.timeout_ms,"replay":"original native refine call"}));
                    drop(engine);
                    return solve_inner(&p,Instant::now(),&mut None);
                }
                engine.report();eprintln!("[post-place-backend] {}",json!({"backend":if engine.batches>0 {"cubecl"}else{"cpu"},"requested":backend}));return result;
            },
            Err(error)=>eprintln!("[post-place-backend] {}",json!({"backend":"cpu","requested":backend,"reason":error.to_string()})),
        }
    }
    if std::env::var_os("PCB_BOARD_PACKER_PROFILE").is_some(){eprintln!("[post-place-backend] {}",json!({"backend":"cpu","requested":backend,"reason":if backend=="cpu" {"explicit CPU"}else if !cfg!(feature="gpu") {"CPU-only build"}else{"outside measured workload/readiness threshold"}}));}
    solve_inner(&p,Instant::now(),#[cfg(feature="gpu")] &mut None)
}
fn solve_inner(p:&RefineProblem,started:Instant,#[cfg(feature="gpu")] engine:&mut Option<gpu::Engine>) -> Result<Value,String> {
    let deadline = started + Duration::from_millis(p.timeout_ms);
    let threads = p.threads.max(1).min(
        (thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            / 2)
        .clamp(1, 8),
    );
    let mut current = p.world(&p.placements)?;
    let initial_started = Instant::now();
    let initial_score = p.score(&current)?;
    let initial_ms = elapsed(initial_started);
    let mut current_score = initial_score;
    let mut moves = Vec::new();
    let mut profiles = Vec::new();
    let mut stop_reason = if p.iterations == 0 {
        "disabled"
    } else {
        "iteration_limit"
    };
    for pass in 0..p.iterations {
        if Instant::now() >= deadline {
            stop_reason = "timeout";
            break;
        }
        let generation = Instant::now();
        let candidates = p.candidates(&current)?;
        let generation_ms = elapsed(generation);
        let evaluation = Instant::now();
        let (results, stats) = {
            #[cfg(feature="gpu")]
            if let Some(gpu)=engine.as_mut() {
                gpu_iteration(p,&current,current_score,&candidates,threads,deadline,gpu)?
            } else {iteration(p,&current,current_score,&candidates,threads,deadline,None)?}
            #[cfg(not(feature="gpu"))]
            iteration(p,&current,current_score,&candidates,threads,deadline,None)?
        };
        let evaluation_ms = elapsed(evaluation);
        let mut best: Option<(usize, Evaluation)> = None;
        for (i, value) in results.into_iter().enumerate() {
            let Some(value) = value else {
                continue;
            };
            if let Some((_, b)) = &best {
                if value.improvement < b.improvement - EPS
                    || (value.improvement - b.improvement).abs() <= EPS
                {
                    continue;
                }
            }
            best = Some((i, value));
        }
        let mut profile = serde_json::to_value(stats).map_err(|e| e.to_string())?;
        profile["generationMs"] = json!(generation_ms);
        profile["evaluationWallMs"] = json!(evaluation_ms);
        profile["accepted"] = json!(best.is_some());
        profile["generatedCandidates"] = json!(candidates.len());
        profile["timedOut"] = json!(Instant::now() >= deadline);
        if std::env::var_os("PCB_BOARD_PACKER_PROFILE").is_some() {
            eprintln!("[pcb-post-place-native] iteration={} {}", pass + 1, profile);
        }
        profiles.push(profile);
        let Some((i, best)) = best else {
            stop_reason = if Instant::now() >= deadline {
                "timeout"
            } else {
                "no_improvement"
            };
            break;
        };
        let c = &candidates[i];
        let r = &best.comparison;
        moves.push(json!({"kind":c.kind,"designators":c.changes.iter().map(|(i,_)|p.components[*i].designator.as_ref()).collect::<Vec<_>>(),"description":c.description,"scoreBefore":rounded(current_score),"scoreAfter":rounded(best.score),"routePenaltyBefore":rounded(r.before_penalty),"routePenaltyAfter":rounded(r.after_penalty),"effectiveImprovement":rounded(best.improvement),"routeJobCount":r.jobs.len(),"routeUnresolvedBefore":r.unresolved_before,"routeUnresolvedAfter":r.unresolved_after,"routeBudgetExhaustedBefore":r.budget_exhausted_before,"routeBudgetExhaustedAfter":r.budget_exhausted_after}));
        for (i, pose) in &c.changes {
            p.apply(&mut current, *i, pose)?;
        }
        current_score = best.score;
        #[cfg(feature="gpu")]
        if let Some(gpu)=engine.as_mut(){gpu.injected("after_move")?;}
    }
    #[cfg(feature="gpu")]
    if let Some(gpu)=engine.as_mut(){gpu.injected("diagnostics")?;}
    let diag = Instant::now();
    let diagnostics = p.diagnostics(&current, current_score, deadline)?;
    let diag_ms = elapsed(diag);
    let timed_out = Instant::now() >= deadline;
    if timed_out {
        stop_reason = "timeout";
    }
    let total = elapsed(started);
    if std::env::var_os("PCB_BOARD_PACKER_PROFILE").is_some() {
        eprintln!("[pcb-post-place-native] total={total:.1}ms threads={threads} passes={}/{} stop={stop_reason}", profiles.len(), p.iterations);
    }
    Ok(
        json!({"placements":current.placements,"diagnostics":diagnostics,"moves":moves,"scoreBefore":rounded(initial_score),"scoreAfter":rounded(current_score),"profile":{"workers":threads,"initialScoreMs":initial_ms,"fixedDiagnosticsMs":diag_ms,"totalMs":total,"iterations":profiles,"iterationLimit":p.iterations,"timeoutMs":p.timeout_ms,"timedOut":timed_out,"stopReason":stop_reason}}),
    )
}

#[cfg(feature="gpu")]
fn gpu_iteration(p:&RefineProblem,current:&World,current_score:f32,candidates:&[Candidate],threads:usize,deadline:Instant,engine:&mut gpu::Engine)->Result<(Vec<Option<Evaluation>>,BatchProfile),String>{
    if threads<=1 {return gpu_iteration_serial(p,current,current_score,candidates,threads,deadline,engine);}
    // Completed geometric groups feed the original CPU route evaluator while
    // the producer works on the next GPU batch. A route group never spans
    // workers, so its baseline cache and feasibility sequence stay intact.
    let mut groups:Vec<Vec<usize>>=Vec::new();
    if threads<=1 {for chunk in (0..candidates.len()).collect::<Vec<_>>().chunks(engine.chunk_size()){groups.push(chunk.to_vec());}}
    else {let mut map=FxHashMap::default();for (i,c) in candidates.iter().enumerate(){let key=c.changes.iter().map(|(i,_)|*i).collect::<Vec<_>>();let id=*map.entry(key).or_insert_with(||{groups.push(vec![]);groups.len()-1});groups[id].push(i);}}
    let order:Vec<_>=groups.iter().flatten().copied().collect();
    let mut owner=vec![0;candidates.len()];for (g,ids) in groups.iter().enumerate(){for &id in ids {owner[id]=g;}}
    let mut pending:Vec<Vec<(usize,f32)>>=groups.iter().map(|_|vec![]).collect();
    let mut remaining:Vec<_>=groups.iter().map(Vec::len).collect();
    let (send,receive)=std::sync::mpsc::channel::<Vec<(usize,f32)>>();let receive=std::sync::Mutex::new(receive);
    let verify=std::env::var("PCB_POST_PLACE_GPU_VERIFY").ok().as_deref()==Some("1");
    let cpu_budget=CpuBudget::new(threads);
    thread::scope(|scope| {
        let mut handles=Vec::new();
        for _ in 0..threads.max(1).min(groups.len().max(1)) {let receive=&receive;let cpu_budget=&cpu_budget;
            handles.push(scope.spawn(move ||->Result<_,String>{
                let _float_env = crate::float_env::Guard::enter();
                let mut scratch=current.clone();let mut stats=BatchProfile::default();let mut values=Vec::new();let mut best:Option<f32>=None;let mut serial_cache=FxHashMap::default();
                loop {
                    let job=receive.lock().map_err(|_|"post-place route queue poisoned")?.recv();let Ok(job)=job else{break;};
                    if Instant::now()>=deadline {break;}
                    let mut group_cache=FxHashMap::default();let cache=if threads<=1 {&mut serial_cache}else{&mut group_cache};
                    for (id,score) in job {
                        if Instant::now()>=deadline {break;}
                        let permit=cpu_budget.acquire();
                        let value=evaluate(p,current,current_score,&candidates[id],&mut scratch,cache,&mut stats,if threads<=1 {best}else{None},Some(score),deadline)?;drop(permit);
                        if let Some(v)=&value {if best.is_none_or(|b|v.improvement>b+EPS){best=Some(v.improvement);}}
                        values.push((id,value));
                    }
                }Ok((values,stats))
            }));
        }
        let produced=(||->Result<_,String>{
            let mut scratch=current.clone();let mut violations=FxHashMap::default();let mut profile=BatchProfile::default();let mut next_group=0;
            for ids in order.chunks(engine.chunk_size()) {
                if Instant::now()>=deadline {break;}
                let mut valid=Vec::new();let mut valid_ids=Vec::new();let mut visited=Vec::new();let mut expected=Vec::new();let started=Instant::now();let permit=cpu_budget.acquire();
                for &id in ids {
                    if Instant::now()>=deadline {break;}
                    let c=&candidates[id];for (i,pose) in &c.changes {p.apply(&mut scratch,*i,pose)?;}
                    let changed:Vec<_>=c.changes.iter().map(|(i,_)|*i).collect();let baseline=violations.entry(changed.clone()).or_insert_with(||p.violations(current,&changed));
                    if p.violations(&scratch,&changed).is_subset(baseline){valid.push(c);valid_ids.push(id);if verify {expected.push(p.score(&scratch)?);}}
                    else{profile.hard_rejected+=1;profile.candidates+=1;}
                    for (i,_) in &c.changes {p.apply(&mut scratch,*i,p.pose(current,*i))?;}visited.push(id);
                }
                drop(permit);profile.geometry_ms+=elapsed(started);
                if !valid.is_empty() && Instant::now()<deadline {
                    let started=Instant::now();let scores=engine.scores(p,current,&valid,Some(&cpu_budget))?;let ms=elapsed(started);profile.global_score_ms+=ms;profile.score_native_ms+=ms;
                    for (j,(&id,score)) in valid_ids.iter().zip(scores).enumerate(){
                        if verify && !crate::f32_policy::score_close(score,expected[j]) {return Err(engine.fail(crate::compute::Error::new(crate::compute::ErrorKind::RuntimeFailure,format!("post-place GPU validation failed candidate {id}: GPU={score:.17} CPU={:.17}",expected[j]))));}
                        pending[owner[id]].push((id,score));
                    }
                }
                for id in visited {remaining[owner[id]]-=1;}
                while next_group<groups.len()&&remaining[next_group]==0 {let job=std::mem::take(&mut pending[next_group]);if !job.is_empty(){let _=send.send(job);}next_group+=1;}
            }
            // On a cooperative timeout only completed scores are eligible.
            for job in pending {if !job.is_empty(){let _=send.send(job);}}
            Ok(profile)
        })();
        drop(send);
        let mut result:Vec<_>=(0..candidates.len()).map(|_|None).collect();let mut profile=BatchProfile::default();
        for h in handles {let (values,stats)=h.join().map_err(|_|"post-place GPU route worker panicked")??;profile.add(&stats);for (id,value) in values {result[id]=value;}}
        profile.add(&produced?);Ok((result,profile))
    })
}

#[cfg(feature="gpu")]
fn gpu_iteration_serial(p:&RefineProblem,current:&World,current_score:f32,candidates:&[Candidate],threads:usize,deadline:Instant,engine:&mut gpu::Engine)->Result<(Vec<Option<Evaluation>>,BatchProfile),String>{
    let mut scores=vec![None;candidates.len()];let mut scratch=current.clone();
    let mut violations=FxHashMap::default();let mut pre=BatchProfile::default();
    let verify=std::env::var("PCB_POST_PLACE_GPU_VERIFY").ok().as_deref()==Some("1");
    for (chunk,items) in candidates.chunks(engine.chunk_size()).enumerate(){
        if Instant::now()>=deadline {break;}
        let offset=chunk*engine.chunk_size();let mut valid=Vec::new();let mut ids=Vec::new();let mut expected=Vec::new();
        let geometry_started=Instant::now();
        for (i,c) in items.iter().enumerate(){
            if Instant::now()>=deadline {break;}
            for (component,pose) in &c.changes {p.apply(&mut scratch,*component,pose)?;}
            let changed:Vec<_>=c.changes.iter().map(|(i,_)|*i).collect();
            let baseline=violations.entry(changed.clone()).or_insert_with(||p.violations(current,&changed));
            if p.violations(&scratch,&changed).is_subset(baseline){valid.push(c);ids.push(offset+i);if verify{expected.push(p.score(&scratch)?);}}
            else{pre.hard_rejected+=1;pre.candidates+=1;}
            for (component,_) in &c.changes {p.apply(&mut scratch,*component,p.pose(current,*component))?;}
        }
        pre.geometry_ms+=elapsed(geometry_started);
        if valid.is_empty() || Instant::now()>=deadline {continue;}
        let started=Instant::now();let values=engine.scores(p,current,&valid,None)?;let ms=elapsed(started);pre.global_score_ms+=ms;pre.score_native_ms+=ms;
        for (j,(&id,&score)) in ids.iter().zip(&values).enumerate(){
            if verify && !crate::f32_policy::score_close(score,expected[j]) {
                return Err(engine.fail(crate::compute::Error::new(crate::compute::ErrorKind::RuntimeFailure,format!("post-place GPU validation failed candidate {id}: GPU={score:.17} CPU={:.17}",expected[j]))));
            }
            scores[id]=Some(score);
        }
    }
    let (result,mut profile)=iteration(p,current,current_score,candidates,threads,deadline,Some(&scores))?;profile.add(&pre);Ok((result,profile))
}

// The GPU producer releases its CPU permit while awaiting the device. Route
// workers may use that core then, but CPU preparation/compilation and route
// evaluation never exceed the configured active CPU budget together.
#[cfg(feature="gpu")]
struct CpuBudget {available:std::sync::Mutex<usize>,wake:std::sync::Condvar}
#[cfg(feature="gpu")]
struct CpuPermit<'a>(&'a CpuBudget);
#[cfg(feature="gpu")]
impl CpuBudget {
 fn new(count:usize)->Self{Self{available:std::sync::Mutex::new(count.max(1)),wake:std::sync::Condvar::new()}}
 fn acquire(&self)->CpuPermit<'_>{let mut n=self.available.lock().unwrap();while *n==0 {n=self.wake.wait(n).unwrap();}*n-=1;CpuPermit(self)}
}
#[cfg(feature="gpu")]
impl Drop for CpuPermit<'_>{fn drop(&mut self){*self.0.available.lock().unwrap()+=1;self.0.wake.notify_one();}}
