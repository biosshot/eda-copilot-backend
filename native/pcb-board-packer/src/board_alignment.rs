//! Board alignment finishing pass. One native call generates, scores, validates
//! and selects every candidate translation of a similar block pair, as the
//! TypeScript `refineBoardAlignment` did. That pass computed geometry on JS
//! numbers and then called separate addon entry points, each of which chose its
//! own coordinate frame. `ts` reproduces that arithmetic, and every native
//! score below is evaluated in the frame its standalone call would choose, so
//! placements, accepted moves and rejection counters are identical.
use crate::geometry::{Box2, Point};
use crate::model::{
    ExplicitConnection, PathPort, Placement, PostPlaceClearance, PostPlaceDistance, PostPlaceEdge, PostPlaceNet,
    PostPlacePath, PostPlaceScoreProblem, RouteObstacle,
};
use crate::placement_quality::{electrical_regression, ElectricalQuality};
use crate::post_place_refine::{ChangeCheck, ChangeScratch, RefineProblem};
use rustc_hash::FxHashSet;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Instant};

pub const CONTRACT_VERSION: u32 = 1;

/// JS-number arithmetic of `src/pcb-layout/f32.ts` and `coordinate-transport.ts`.
pub(crate) mod ts {
    const MIN_NORMAL: f64 = 1.1754943508222875e-38;
    pub fn placement_epsilon() -> f64 { 0.001f32 as f64 }
    /// `Math.fround` with signed flush to zero.
    #[inline]
    pub fn f(v: f64) -> f64 {
        let r = v as f32 as f64;
        if r != 0.0 && r.abs() < MIN_NORMAL { if r < 0.0 { -0.0 } else { 0.0 } } else { r }
    }
    #[inline] pub fn add(a: f64, b: f64) -> f64 { f(f(a) + f(b)) }
    #[inline] pub fn sub(a: f64, b: f64) -> f64 { f(f(a) - f(b)) }
    #[inline] pub fn mul(a: f64, b: f64) -> f64 { f(f(a) * f(b)) }
    #[inline] pub fn div(a: f64, b: f64) -> f64 { f(f(a) / f(b)) }
    #[inline] pub fn sqrt(a: f64) -> f64 { f(f(a).sqrt()) }
    #[inline] pub fn abs(a: f64) -> f64 { f(a).abs() }
    /// `fp.hypot(a, b)`, a reduction that starts at zero.
    #[inline] pub fn hypot(a: f64, b: f64) -> f64 { sqrt(add(add(0.0, mul(a, a)), mul(b, b))) }
    /// `Math.min(...values.map(f32))`, including signed zeros.
    pub fn min(values: impl IntoIterator<Item = f64>) -> f64 {
        let mut m = f64::INFINITY;
        for v in values {
            let v = f(v);
            if v.is_nan() { return f64::NAN; }
            if v < m || (v == 0.0 && m == 0.0 && v.is_sign_negative()) { m = v; }
        }
        m
    }
    pub fn max(values: impl IntoIterator<Item = f64>) -> f64 {
        let mut m = f64::NEG_INFINITY;
        for v in values {
            let v = f(v);
            if v.is_nan() { return f64::NAN; }
            if v > m || (v == 0.0 && m == 0.0 && m.is_sign_negative()) { m = v; }
        }
        m
    }
    /// `Math.round`: ties towards +Infinity, negative results keep their sign.
    pub fn js_round(x: f64) -> f64 {
        if !x.is_finite() { return x; }
        let floor = x.floor();
        let r = if x - floor >= 0.5 { floor + 1.0 } else { floor };
        if r == 0.0 && x < 0.0 { -0.0 } else { r }
    }
    pub fn source_add(absolute: f64, offset: f64) -> f64 {
        let origin = js_round(absolute);
        add(f(absolute - origin), offset) + origin
    }
    pub fn coordinate_origin(value: f64) -> f64 { if value.abs() <= 1024.0 { 0.0 } else { js_round(value) } }
    pub fn coordinate_difference(a: f64, b: f64) -> f64 {
        let origin = js_round(b);
        f(f(a - origin) - f(b - origin))
    }
    pub fn coordinate_center(a: f64, b: f64) -> f64 {
        let origin = coordinate_origin(a);
        f(add(f(a - origin), f(b - origin)) / 2.0) + origin
    }
    pub fn round_placement(value: f64) -> f64 {
        let ticks = js_round(mul(value, 1000.0));
        if ticks == 0.0 { 0.0 } else { div(ticks, 1000.0) }
    }
    pub fn coordinate_round(value: f64) -> f64 {
        let origin = coordinate_origin(value);
        round_placement(f(value - origin)) + origin
    }
    /// `round` of the placement report (two decimals).
    pub fn round2(value: f64) -> f64 { div(js_round(mul(value, 100.0)), 100.0) }
    /// Identity of `value.toFixed(3)`: sign and decimal ticks (ties upward).
    pub fn fixed3(value: f64) -> (bool, i64) {
        let ticks = (value.abs() * 1000.0 + 0.5).floor();
        (value < 0.0, ticks as i64)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct BoxD { left: f64, right: f64, top: f64, bottom: f64 }
#[derive(Clone, Copy, Debug, Deserialize)]
struct PointD { x: f64, y: f64 }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Problem {
    version: u32,
    threads: usize,
    passes: usize,
    max_shift: f64,
    grid: f64,
    placements: Vec<Pose>,
    components: Vec<Component>,
    roots: Vec<Root>,
    pairs: Vec<Pair>,
    alignment_policy: Policy,
    score: ScoreTemplate,
    spacing: SpacingTemplate,
    report: ReportTemplate,
}
#[derive(Deserialize)]
struct Pose { x: f64, y: f64 }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Component {
    designator: Arc<str>,
    pose_index: usize,
    x: f64,
    y: f64,
    rotate: i32,
    layer: Arc<str>,
    body: BoxD,
    hint_box: BoxD,
    pack_box: BoxD,
    rows: Vec<Row>,
    pads: Vec<Pad>,
    collision: Collision,
    board_exempt: bool,
    outside: Outside,
    regions: Vec<usize>,
}
#[derive(Deserialize)]
struct Row { weight: f32, #[serde(rename = "box")] box_: BoxD }
#[derive(Deserialize)]
struct Pad { point: PointD, #[serde(rename = "box")] box_: BoxD }
#[derive(Deserialize)]
struct Collision { top: Vec<BoxD>, bottom: Vec<BoxD> }
#[derive(Deserialize)]
struct Outside { polygon: bool, edge: f64, limits: [f64; 4] }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Root {
    id: Arc<str>,
    label: Arc<str>,
    members: Vec<usize>,
    movable: bool,
    bbox_collision: bool,
    static_points: Vec<PointD>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pair {
    a: i64,
    b: i64,
    similarity: f32,
    anchor_a: Option<Arc<str>>,
    anchor_b: Option<Arc<str>>,
    orientation: Option<crate::model::AlignmentOrientation>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Policy { weight: f32, tolerance: f32, orientation_weight: f32 }

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Target {
    Missing,
    Point { x: f64, y: f64 },
    Component { component: usize },
    Pin { component: usize, pad: usize },
    Block { members: Vec<usize> },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoreTemplate {
    route_clearance: f32,
    access: Vec<usize>,
    explicit: Vec<Explicit>,
    obstacles: Vec<Obstacle>,
    nets: Vec<NetTemplate>,
    distances: Vec<DistanceTemplate>,
    clearances: Vec<ClearanceTemplate>,
    edges: Vec<EdgeTemplate>,
    paths: Vec<PathTemplate>,
    fixed_penalties: Vec<f32>,
}
#[derive(Deserialize)]
struct Explicit { source: (usize, usize), target: (usize, usize), net: Arc<str>, layer: Option<Arc<str>>, kind: Arc<str> }
#[derive(Deserialize)]
struct Obstacle {
    component: usize,
    pad: usize,
    #[serde(rename = "ref")]
    reference: Arc<str>,
    net: Option<Arc<str>>,
    layer: Option<Arc<str>>,
}
#[derive(Deserialize)]
struct NetTemplate {
    name: Arc<str>,
    weight: f32,
    points: Vec<(usize, usize)>,
    layers: Vec<Option<Arc<str>>>,
    owners: Vec<Option<Arc<str>>>,
    scopes: Vec<Option<Arc<str>>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DistanceTemplate {
    source: Target,
    target: Target,
    weight: f32,
    inverse_tau_squared: Option<f32>,
    min: Option<f32>,
    max: Option<f32>,
}
#[derive(Deserialize)]
struct ClearanceTemplate { source: Target, target: Target, minimum: f32, weight: f32 }
#[derive(Deserialize)]
struct EdgeTemplate { source: Target, board: BoxD, edge: Arc<str>, weight: f32 }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathTemplate {
    path_id: Arc<str>,
    ports: Vec<PortTemplate>,
    shape: Arc<str>,
    priority: Arc<str>,
    #[serde(default = "crate::relation_score::default_path_weight")]
    weight: f32,
    #[serde(default)]
    prefer_facing_pads: bool,
}
#[derive(Deserialize)]
struct PortTemplate {
    component: usize,
    pad: usize,
    order: i32,
    #[serde(rename = "ref")]
    reference: Arc<str>,
    role: Arc<str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpacingTemplate {
    gap: f32,
    clearance: f32,
    members: Vec<Vec<usize>>,
    pairs: Vec<(usize, usize)>,
    bounds: BoxD,
    fixed_boxes: Vec<Option<Vec<BoxD>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportTemplate {
    board: BoxD,
    polygon: Vec<PointD>,
    holes: Vec<Hole>,
    regions: Vec<Region>,
    static_failure: bool,
    rules: Vec<Rule>,
    baseline: Vec<Baseline>,
    pair_clearance: Vec<Vec<f64>>,
}
#[derive(Deserialize)]
struct Hole { x: f64, y: f64, required: f64 }
#[derive(Deserialize)]
struct Region { #[serde(rename = "box")] box_: BoxD, layers: Vec<Arc<str>> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    kind: String,
    cls: usize,
    source: Target,
    target: Target,
    #[serde(default)]
    edge: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
    min_expected: Option<usize>,
    max_expected: Option<usize>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Baseline { cls: usize, expected: usize, actual: f64, at_least: bool }

/// World geometry of one component, as JS numbers.
#[derive(Clone)]
struct Geo {
    x: f64,
    y: f64,
    body: BoxD,
    hint: BoxD,
    pack: BoxD,
    rows: Vec<BoxD>,
    pad_points: Vec<PointD>,
    pad_boxes: Vec<BoxD>,
    /// Collision boxes on the top and bottom layer, and their unions.
    collision: [Vec<BoxD>; 2],
    envelope: [Option<BoxD>; 2],
}

#[derive(Clone)]
struct World {
    geo: Vec<Geo>,
    /// `currentRoots` bbox per root.
    bbox: Vec<BoxD>,
}

fn layer_index(layer: &str) -> usize { usize::from(layer != "top") }

fn shifted(b: &BoxD, x: f64, y: f64) -> BoxD {
    BoxD { left: ts::source_add(x, b.left), right: ts::source_add(x, b.right), top: ts::source_add(y, b.top), bottom: ts::source_add(y, b.bottom) }
}
/// `unionBoxes`: raw JS comparisons, first box first.
fn union_raw<'a>(boxes: impl IntoIterator<Item = &'a BoxD>) -> Option<BoxD> {
    let mut result: Option<BoxD> = None;
    for b in boxes {
        result = Some(match result {
            None => *b,
            Some(mut u) => {
                if b.left < u.left { u.left = b.left; }
                if b.right > u.right { u.right = b.right; }
                if b.top < u.top { u.top = b.top; }
                if b.bottom > u.bottom { u.bottom = b.bottom; }
                u
            }
        });
    }
    result
}

fn place(c: &Component, x: f64, y: f64) -> Geo {
    let collision = [
        c.collision.top.iter().map(|b| shifted(b, x, y)).collect::<Vec<_>>(),
        c.collision.bottom.iter().map(|b| shifted(b, x, y)).collect::<Vec<_>>(),
    ];
    Geo {
        x,
        y,
        body: shifted(&c.body, x, y),
        hint: shifted(&c.hint_box, x, y),
        pack: shifted(&c.pack_box, x, y),
        rows: c.rows.iter().map(|r| shifted(&r.box_, x, y)).collect(),
        pad_points: c.pads.iter().map(|p| PointD { x: ts::source_add(x, p.point.x), y: ts::source_add(y, p.point.y) }).collect(),
        pad_boxes: c.pads.iter().map(|p| shifted(&p.box_, x, y)).collect(),
        envelope: [union_raw(&collision[0]), union_raw(&collision[1])],
        collision,
    }
}

/// Min/max of every absolute coordinate, as `boundary::Frame::localize` scans it.
#[derive(Clone, Copy)]
struct Bounds { min: [f64; 2], max: [f64; 2] }
impl Bounds {
    fn new() -> Self { Self { min: [f64::INFINITY; 2], max: [f64::NEG_INFINITY; 2] } }
    #[inline]
    fn value(&mut self, axis: usize, v: f64) { self.min[axis] = self.min[axis].min(v); self.max[axis] = self.max[axis].max(v); }
    #[inline]
    fn point(&mut self, p: PointD) { self.value(0, p.x); self.value(1, p.y); }
    #[inline]
    fn bx(&mut self, b: &BoxD) { self.value(0, b.left); self.value(0, b.right); self.value(1, b.top); self.value(1, b.bottom); }
    fn origin(&self) -> [f64; 2] {
        let mut origin = [0.0; 2];
        for axis in 0..2 { if self.min[axis].is_finite() { origin[axis] = ((self.min[axis] + self.max[axis]) * 0.5).round(); } }
        origin
    }
}
fn frame_of_box(b: &BoxD) -> [f64; 2] { [((b.left + b.right) * 0.5).round(), ((b.top + b.bottom) * 0.5).round()] }
#[inline]
fn lp(p: PointD, o: [f64; 2]) -> Point { Point { x: (p.x - o[0]) as f32, y: (p.y - o[1]) as f32 } }
#[inline]
fn lb(b: &BoxD, o: [f64; 2]) -> Box2 {
    Box2 { left: (b.left - o[0]) as f32, right: (b.right - o[0]) as f32, top: (b.top - o[1]) as f32, bottom: (b.bottom - o[1]) as f32 }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage { Objective, Electrical, NetLength, Geometry, HardHint, Native, Accepted }

struct Outcome { next: f32, stage: Stage }

#[derive(Clone)]
struct Candidate {
    axis: usize,
    shifts: [f64; 2],
    perpendicular: [f64; 2],
    /// Component, new x and new y.
    moved: Vec<(usize, f64, f64)>,
}

#[derive(Default)]
struct Counters { objective: usize, electrical: usize, net_length: usize, geometry: usize, hard_hint: usize, native: usize }
impl Counters {
    fn count(&mut self, stage: Stage) {
        match stage {
            Stage::Objective => self.objective += 1,
            Stage::Electrical => self.electrical += 1,
            Stage::NetLength => self.net_length += 1,
            Stage::Geometry => self.geometry += 1,
            Stage::HardHint => self.hard_hint += 1,
            Stage::Native => self.native += 1,
            Stage::Accepted => {}
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Timing { objective_ms: f64, electrical_ms: f64, report_ms: f64, native_ms: f64, objective: usize, electrical: usize, report: usize, native: usize }
impl Timing {
    fn add(&mut self, o: &Self) {
        self.objective_ms += o.objective_ms; self.electrical_ms += o.electrical_ms; self.report_ms += o.report_ms; self.native_ms += o.native_ms;
        self.objective += o.objective; self.electrical += o.electrical; self.report += o.report; self.native += o.native;
    }
}

struct Pass {
    p: Problem,
    check: ChangeCheck,
    /// Origin of the refine-problem frame used by `validatePlacementChange`.
    check_origin: [f64; 2],
    base: Vec<(f64, f64)>,
    base_electrical: ElectricalQuality,
}

/// Per-thread mutable state for candidate evaluation.
struct Scratch { world: World, change: ChangeScratch }

/// Route plans of the current layout, one per native frame in use. A
/// candidate in the same frame only re-evaluates the terms it changes.
struct Routes { caches: Vec<([f64; 2], crate::fast_route::RouteCache)> }
impl Routes {
    fn route(&self, origin: [f64; 2], problem: &PostPlaceScoreProblem) -> f32 {
        match self.caches.iter().find(|(o, _)| *o == origin) {
            Some((_, cache)) => cache.evaluate(problem),
            None => crate::post_place::route_plan(problem).cpu(),
        }
    }
}

const PERPENDICULARS: [[f64; 2]; 13] = [[0.0, 0.0], [-0.5, -0.5], [0.5, 0.5], [-1.0, -1.0], [1.0, 1.0], [0.0, -0.5], [0.0, 0.5],
    [-0.5, 0.0], [0.5, 0.0], [0.0, -1.0], [0.0, 1.0], [-1.0, 0.0], [1.0, 0.0]];

impl Pass {
    fn new(p: Problem, mut constraints: Value) -> Result<Self, String> {
        if p.version != CONTRACT_VERSION { return Err(format!("unsupported board alignment contract {}; expected {CONTRACT_VERSION}", p.version)); }
        let n = p.components.len();
        let valid_target = |t: &Target| match t {
            Target::Component { component } => *component < n,
            Target::Pin { component, pad } => *component < n && *pad < p.components[*component].pads.len(),
            Target::Block { members } => !members.is_empty() && members.iter().all(|i| *i < n),
            _ => true,
        };
        let pad = |(c, i): (usize, usize)| c < n && i < p.components[c].pads.len();
        if p.components.iter().any(|c| c.pose_index >= p.placements.len() || c.regions.iter().any(|r| *r >= p.report.regions.len()))
            || p.roots.iter().any(|r| r.members.iter().any(|i| *i >= n))
            || p.pairs.iter().any(|q| q.a >= p.roots.len() as i64 || q.b >= p.roots.len() as i64)
            || p.score.access.iter().any(|i| *i >= n)
            || p.score.explicit.iter().any(|e| !pad(e.source) || !pad(e.target))
            || p.score.obstacles.iter().any(|o| !pad((o.component, o.pad)))
            || p.score.nets.iter().any(|net| net.points.iter().any(|q| !pad(*q)) || net.layers.len() != net.points.len()
                || net.owners.len() != net.points.len() || net.scopes.len() != net.points.len())
            || p.score.distances.iter().any(|d| !valid_target(&d.source) || !valid_target(&d.target))
            || p.score.clearances.iter().any(|d| !valid_target(&d.source) || !valid_target(&d.target))
            || p.score.edges.iter().any(|d| !valid_target(&d.source))
            || p.score.paths.iter().any(|path| path.ports.iter().any(|q| !pad((q.component, q.pad))))
            || p.spacing.members.len() != p.roots.len() || p.spacing.fixed_boxes.len() != p.roots.len()
            || p.spacing.members.iter().flatten().any(|i| *i >= n)
            || p.spacing.pairs.iter().any(|(a, b)| *a >= p.roots.len() || *b >= p.roots.len())
            || p.report.rules.iter().any(|r| !valid_target(&r.source) || !valid_target(&r.target))
            || p.report.pair_clearance.len() != n || p.report.pair_clearance.iter().any(|r| r.len() != n)
        {
            return Err("invalid board alignment references".into());
        }
        let frame = crate::boundary::Frame::localize(&mut constraints)?;
        let refine: RefineProblem = serde_json::from_value(constraints).map_err(|e| e.to_string())?;
        let check = ChangeCheck::new(refine)?;
        if check.placements().len() != p.placements.len() { return Err("board alignment constraints do not match placements".into()); }
        let base = p.components.iter().map(|c| (c.x, c.y)).collect();
        let mut pass = Self { p, check, check_origin: frame.origin(), base, base_electrical: ElectricalQuality { score: 0.0, lengths: vec![] } };
        let world = pass.world();
        let full = pass.score_problem(&world, true);
        pass.base_electrical = pass.electrical(&world, &full, &Routes { caches: vec![] })?;
        Ok(pass)
    }

    fn world(&self) -> World {
        let geo: Vec<Geo> = self.p.components.iter().map(|c| place(c, c.x, c.y)).collect();
        let mut w = World { geo, bbox: vec![] };
        w.bbox = (0..self.p.roots.len()).map(|r| self.root_bbox(&w, r)).collect();
        w
    }
    fn root_bbox(&self, w: &World, root: usize) -> BoxD {
        union_raw(self.p.roots[root].members.iter().map(|&i| &w.geo[i].pack)).unwrap_or(BoxD { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 })
    }
    fn roots_of(&self, moved: &[(usize, f64, f64)]) -> Vec<usize> {
        (0..self.p.roots.len()).filter(|&r| self.p.roots[r].members.iter().any(|i| moved.iter().any(|m| m.0 == *i))).collect()
    }
    fn apply(&self, w: &mut World, moved: &[(usize, f64, f64)]) {
        for &(i, x, y) in moved { w.geo[i] = place(&self.p.components[i], x, y); }
        for r in self.roots_of(moved) { w.bbox[r] = self.root_bbox(w, r); }
    }

    // ---- target resolution (`resolveTargetPoint` / `resolveTargetBox`) ----
    fn block_box(&self, w: &World, members: &[usize]) -> BoxD {
        BoxD {
            left: ts::min(members.iter().map(|&i| w.geo[i].hint.left)),
            right: ts::max(members.iter().map(|&i| w.geo[i].hint.right)),
            top: ts::min(members.iter().map(|&i| w.geo[i].hint.top)),
            bottom: ts::max(members.iter().map(|&i| w.geo[i].hint.bottom)),
        }
    }
    fn target_point(&self, w: &World, t: &Target) -> PointD {
        match t {
            Target::Point { x, y } => PointD { x: *x, y: *y },
            Target::Component { component } => PointD { x: w.geo[*component].x, y: w.geo[*component].y },
            Target::Pin { component, pad } => w.geo[*component].pad_points[*pad],
            Target::Block { members } => {
                let b = self.block_box(w, members);
                PointD { x: ts::div(ts::add(b.left, b.right), 2.0), y: ts::div(ts::add(b.top, b.bottom), 2.0) }
            }
            Target::Missing => PointD { x: f64::NAN, y: f64::NAN },
        }
    }
    fn target_box(&self, w: &World, t: &Target) -> BoxD {
        match t {
            Target::Component { component } => w.geo[*component].hint,
            Target::Block { members } => self.block_box(w, members),
            _ => { let p = self.target_point(w, t); BoxD { left: p.x, right: p.x, top: p.y, bottom: p.y } }
        }
    }

    // ---- `globalPostPlaceScore` and `boardElectricalQuality` ----
    fn path_ports(&self, w: &World, path: &PathTemplate) -> Vec<(PointD, Point)> {
        path.ports.iter().map(|port| {
            let g = &w.geo[port.component];
            let point = g.pad_points[port.pad];
            let vx = ts::sub(point.x, g.x);
            let vy = ts::sub(point.y, g.y);
            let length = ts::hypot(vx, vy);
            let normal = if length > 0.001 { Point { x: ts::div(vx, length) as f32, y: ts::div(vy, length) as f32 } } else { Point { x: 0.0, y: 0.0 } };
            (point, normal)
        }).collect()
    }
    fn score_frame(&self, w: &World, full: bool) -> [f64; 2] {
        let t = &self.p.score;
        let mut b = Bounds::new();
        for &i in &t.access { b.bx(&w.geo[i].body); for row in &w.geo[i].rows { b.bx(row); } }
        for e in &t.explicit { b.point(w.geo[e.source.0].pad_points[e.source.1]); b.point(w.geo[e.target.0].pad_points[e.target.1]); }
        for o in &t.obstacles { b.bx(&w.geo[o.component].pad_boxes[o.pad]); }
        for net in &t.nets { for &(c, pad) in &net.points { b.point(w.geo[c].pad_points[pad]); } }
        if full {
            for d in &t.distances { b.point(self.target_point(w, &d.source)); b.point(self.target_point(w, &d.target)); }
            for c in &t.clearances { b.bx(&self.target_box(w, &c.source)); b.bx(&self.target_box(w, &c.target)); }
            for e in &t.edges { b.bx(&self.target_box(w, &e.source)); b.bx(&e.board); }
            for path in &t.paths { for port in &path.ports { b.point(w.geo[port.component].pad_points[port.pad]); } }
        }
        b.origin()
    }
    fn score_problem(&self, w: &World, full: bool) -> PostPlaceScoreProblem { self.framed_problem(w, full).1 }
    fn framed_problem(&self, w: &World, full: bool) -> ([f64; 2], PostPlaceScoreProblem) {
        let o = self.score_frame(w, full);
        (o, self.problem_in(w, full, o))
    }
    fn problem_in(&self, w: &World, full: bool, o: [f64; 2]) -> PostPlaceScoreProblem {
        let t = &self.p.score;
        let point = |(c, pad): (usize, usize)| lp(w.geo[c].pad_points[pad], o);
        PostPlaceScoreProblem {
            route_clearance: t.route_clearance,
            access_components: t.access.iter().map(|&i| {
                let g = &w.geo[i];
                crate::pad_access::Component {
                    body: lb(&g.body, o),
                    layer: self.p.components[i].layer.to_string(),
                    rows: g.rows.iter().zip(&self.p.components[i].rows).map(|(b, r)| crate::pad_access::Row { box_: lb(b, o), weight: r.weight }).collect(),
                }
            }).collect(),
            explicit_connections: t.explicit.iter().map(|e| ExplicitConnection {
                source: point(e.source), target: point(e.target), net: e.net.clone(), layer: e.layer.clone(), kind: e.kind.clone(),
            }).collect(),
            version: 1,
            routing_obstacles: t.obstacles.iter().map(|ob| RouteObstacle {
                box_: lb(&w.geo[ob.component].pad_boxes[ob.pad], o), layer: ob.layer.clone(), reference: Some(ob.reference.clone()),
                net: ob.net.clone(), primitive_id: None,
            }).collect(),
            nets: t.nets.iter().map(|net| PostPlaceNet {
                local_scopes: if full { net.scopes.clone() } else { vec![] },
                internal_owners: net.owners.clone(),
                name: net.name.clone(),
                points: net.points.iter().map(|q| point(*q)).collect(),
                layers: net.layers.clone(),
                weight: net.weight,
            }).collect(),
            distances: if !full { vec![] } else { t.distances.iter().map(|d| PostPlaceDistance {
                inverse_tau_squared: d.inverse_tau_squared.unwrap_or_else(crate::relation_score::default_inverse_tau_squared),
                source: lp(self.target_point(w, &d.source), o), target: lp(self.target_point(w, &d.target), o),
                weight: d.weight, min: d.min, max: d.max,
            }).collect() },
            clearances: if !full { vec![] } else { t.clearances.iter().map(|c| PostPlaceClearance {
                source: lb(&self.target_box(w, &c.source), o), target: lb(&self.target_box(w, &c.target), o), minimum: c.minimum, weight: c.weight,
            }).collect() },
            fixed_penalties: if full { t.fixed_penalties.clone() } else { vec![] },
            edges: if !full { vec![] } else { t.edges.iter().map(|e| PostPlaceEdge {
                source: lb(&self.target_box(w, &e.source), o), board: lb(&e.board, o), edge: e.edge.clone(), weight: e.weight,
            }).collect() },
            paths: if !full { vec![] } else { t.paths.iter().map(|path| PostPlacePath {
                path_id: path.path_id.clone(),
                ports: self.path_ports(w, path).into_iter().zip(&path.ports).map(|((pt, normal), port)| {
                    let q = lp(pt, o);
                    PathPort { x: q.x, y: q.y, path_id: path.path_id.clone(), order: port.order, reference: port.reference.clone(), role: port.role.clone(), normal }
                }).collect(),
                shape: path.shape.clone(), priority: path.priority.clone(), weight: path.weight, prefer_facing_pads: path.prefer_facing_pads,
            }).collect() },
        }
    }
    /// `boardElectricalQuality`: routing-only score and net lengths.
    fn electrical(&self, w: &World, full: &PostPlaceScoreProblem, routes: &Routes) -> Result<ElectricalQuality, String> {
        let (o, wiring) = self.framed_problem(w, false);
        Ok(ElectricalQuality { score: crate::post_place::score_with_route(&wiring, |p| routes.route(o, p))?,
            lengths: full.nets.iter().map(crate::post_place::net_length).collect() })
    }
    fn routes(&self, w: &World) -> Routes {
        let mut caches: Vec<([f64; 2], crate::fast_route::RouteCache)> = Vec::new();
        for full in [true, false] {
            let (o, problem) = self.framed_problem(w, full);
            if !caches.iter().any(|(c, _)| *c == o) { caches.push((o, crate::fast_route::RouteCache::new(&problem))); }
        }
        Routes { caches }
    }

    // ---- `boardSpacingPenalty` ----
    fn spacing(&self, w: &World) -> f32 {
        let s = &self.p.spacing;
        if s.gap <= 0.0 { return 0.0; }
        let o = frame_of_box(&s.bounds);
        let envelopes: Vec<[Option<Box2>; 2]> = (0..self.p.roots.len()).map(|r| {
            if let Some(boxes) = &s.fixed_boxes[r] {
                let all = crate::solver::envelope(boxes.iter().map(|b| lb(b, o)));
                return [all, all];
            }
            let layer = |l: usize| crate::solver::envelope(s.members[r].iter().flat_map(|&i| {
                let g = &w.geo[i];
                if layer_index(&self.p.components[i].layer) == l { vec![lb(&g.body, o)] } else { g.collision[l].iter().map(|b| lb(b, o)).collect() }
            }));
            [layer(0), layer(1)]
        }).collect();
        let mut score = 0.0;
        for &(a, b) in &s.pairs { score += crate::solver::spacing_term(|l| Some((envelopes[a][l]?, envelopes[b][l]?)), s.clearance, s.gap); }
        score
    }

    // ---- `boardAlignmentScore` with the unnormalized policy ----
    fn penalty(&self, w: &World) -> f32 {
        let mut b = Bounds::new();
        for (r, root) in self.p.roots.iter().enumerate() {
            for p in &root.static_points { b.point(*p); }
            b.bx(&w.bbox[r]);
            for &i in &root.members { b.point(PointD { x: w.geo[i].x, y: w.geo[i].y }); }
        }
        let o = b.origin();
        let primitives: Vec<crate::model::Primitive> = self.p.roots.iter().enumerate().map(|(r, root)| {
            let placements = root.members.iter().map(|&i| {
                let c = &self.p.components[i];
                let q = lp(PointD { x: w.geo[i].x, y: w.geo[i].y }, o);
                Placement { designator: c.designator.clone(), x: q.x, y: q.y, rotate: c.rotate, layer: c.layer.clone(), score: 0.0 }
            }).collect();
            crate::solver::alignment_primitive(root.id.clone(), lb(&w.bbox[r], o), placements)
        }).collect();
        let policy = crate::model::SoftAlignment {
            orientation_weight: self.p.alignment_policy.orientation_weight,
            weight: self.p.alignment_policy.weight,
            tolerance: self.p.alignment_policy.tolerance,
            pairs: self.p.pairs.iter().filter(|q| q.a >= 0 && q.b >= 0).map(|q| crate::model::AlignmentPair {
                orientation: q.orientation.clone(), a: self.p.roots[q.a as usize].id.clone(), b: self.p.roots[q.b as usize].id.clone(),
                anchor_a: q.anchor_a.clone(), anchor_b: q.anchor_b.clone(), similarity: q.similarity,
            }).collect(),
        };
        crate::solver::alignment_score(primitives.iter(), &policy)
    }

    fn objective(&self, w: &World, routes: &Routes) -> Result<(f32, f32, PostPlaceScoreProblem), String> {
        let (o, full) = self.framed_problem(w, true);
        let global = crate::post_place::score_with_route(&full, |p| routes.route(o, p))?;
        let penalty = self.penalty(w);
        let score = [global, self.spacing(w), penalty].iter().copied().sum::<f32>();
        if !score.is_finite() { return Err("non-finite placement score".into()); }
        Ok((score, penalty, full))
    }

    // ---- `createPlacementReport(...).ok` ----
    fn outside(&self, c: &Component, b: &BoxD) -> bool {
        let o = &c.outside;
        if !o.polygon {
            return b.left < o.limits[0] || b.right > o.limits[1] || b.top < o.limits[2] || b.bottom > o.limits[3];
        }
        !geometry::box_inside_board(&self.p.report.board, &self.p.report.polygon, b, o.edge)
    }
    fn component_ok(&self, w: &World, i: usize) -> bool {
        let c = &self.p.components[i];
        if c.board_exempt { return true; }
        let g = &w.geo[i];
        let opposite = 1 - layer_index(&c.layer);
        if std::iter::once(&g.body).chain(&g.collision[opposite]).any(|b| self.outside(c, b)) { return false; }
        let eps = ts::placement_epsilon();
        for hole in &self.p.report.holes {
            let gap = geometry::box_point_gap(&g.hint, hole.x, hole.y);
            if ts::add(gap, eps) < hole.required { return false; }
        }
        for &r in &c.regions {
            let region = &self.p.report.regions[r];
            let overlap = ts::max(std::iter::once(0.0).chain(region.layers.iter().flat_map(|layer| {
                g.collision[layer_index(layer)].iter().map(|b| geometry::box_overlap_depth(b, &region.box_))
            })));
            if overlap > 0.0 { return false; }
        }
        true
    }
    fn pair_ok(&self, w: &World, i: usize, j: usize) -> bool {
        let (a, b) = (&w.geo[i], &w.geo[j]);
        let (la, lbi) = (layer_index(&self.p.components[i].layer), layer_index(&self.p.components[j].layer));
        if la != lbi && a.collision[lbi].is_empty() && b.collision[la].is_empty() { return true; }
        let required = self.p.report.pair_clearance[i][j];
        // Exact separation of the per-layer unions bounds every box pair from below.
        let far = (0..2).all(|l| match (&a.envelope[l], &b.envelope[l]) {
            (Some(x), Some(y)) => (y.left - x.right).max(x.left - y.right).max(y.top - x.bottom).max(x.top - y.bottom) >= required + 0.01,
            _ => true,
        });
        if far { return true; }
        let mut gaps = Vec::new();
        for l in 0..2 { for x in &a.collision[l] { for y in &b.collision[l] { gaps.push(geometry::box_clearance_gap(x, y)); } } }
        gaps.is_empty() || !(ts::add(ts::min(gaps), ts::placement_epsilon()) < required)
    }
    fn report_ok(&self, w: &World) -> bool {
        if self.p.report.static_failure { return false; }
        let n = self.p.components.len();
        (0..n).all(|i| self.component_ok(w, i)) && (0..n).all(|i| (i + 1..n).all(|j| self.pair_ok(w, i, j)))
    }

    // ---- `alignmentHardHintsNoWorse` ----
    fn hard_hints_ok(&self, w: &World) -> bool {
        let eps = ts::placement_epsilon();
        let report = &self.p.report;
        for rule in &report.rules {
            let known = |expected: Option<usize>, actual: f64| {
                let expected = expected.expect("rule bound has an expected text");
                report.baseline.iter().any(|b| b.cls == rule.cls && b.expected == expected
                    && if b.at_least { actual >= b.actual } else { actual <= b.actual })
            };
            match rule.kind.as_str() {
                "distance" => {
                    let a = self.target_point(w, &rule.source);
                    let b = self.target_point(w, &rule.target);
                    let actual = geometry::dist(a, b);
                    if rule.min.is_some_and(|m| actual < m) && !known(rule.min_expected, ts::round2(actual)) { return false; }
                    if rule.max.is_some_and(|m| actual > m) && !known(rule.max_expected, ts::round2(actual)) { return false; }
                }
                "clearance" => {
                    let actual = geometry::box_clearance_gap(&self.target_box(w, &rule.source), &self.target_box(w, &rule.target));
                    if rule.min.is_some_and(|m| ts::add(actual, eps) < m) && !known(rule.min_expected, ts::round2(actual)) { return false; }
                }
                "edge" => {
                    let Target::Component { component } = rule.source else { continue };
                    let (c, board) = (&w.geo[component].hint, &report.board);
                    let actual = match rule.edge.as_deref() {
                        Some("left") => ts::abs(ts::sub(c.left, board.left)),
                        Some("right") => ts::abs(ts::sub(board.right, c.right)),
                        Some("top") => ts::abs(ts::sub(c.top, board.top)),
                        _ => ts::abs(ts::sub(board.bottom, c.bottom)),
                    };
                    if rule.max.is_some_and(|m| actual > m) && !known(rule.max_expected, ts::round2(actual)) { return false; }
                    if rule.min.is_some_and(|m| actual < m) && !known(rule.min_expected, ts::round2(actual)) { return false; }
                }
                _ => {}
            }
        }
        true
    }

    // ---- candidates ----
    fn center(&self, w: &World, root: usize, anchor: &Option<Arc<str>>) -> PointD {
        let r = &self.p.roots[root];
        if let Some(i) = anchor.as_ref().and_then(|name| r.members.iter().find(|&&i| &self.p.components[i].designator == name)) {
            return PointD { x: w.geo[*i].x, y: w.geo[*i].y };
        }
        let b = &w.bbox[root];
        PointD { x: ts::coordinate_round(ts::coordinate_center(b.left, b.right)), y: ts::coordinate_round(ts::coordinate_center(b.top, b.bottom)) }
    }
    fn candidates(&self, w: &World, pair: &Pair) -> Vec<Candidate> {
        let (ra, rb) = (pair.a as usize, pair.b as usize);
        let ac = self.center(w, ra, &pair.anchor_a);
        let bc = self.center(w, rb, &pair.anchor_b);
        let mut result = Vec::new();
        for axis in 0..2 {
            let (a, b) = if axis == 0 { (ac.x, bc.x) } else { (ac.y, bc.y) };
            let delta = ts::sub(b, a);
            if ts::abs(delta) < 0.001 || ts::abs(delta) > ts::mul(2.0, self.p.max_shift) { continue; }
            let mut axis_shifts = Vec::new();
            for fraction in [1.0, 0.5, 0.25] {
                for shares in [[1.0, 0.0], [0.0, -1.0], [0.5, -0.5]] {
                    axis_shifts.push(shares.map(|s| ts::mul(ts::mul(s, delta), fraction)));
                }
            }
            // Alignment need not happen at either existing axis or exactly
            // halfway: also try every shared axis on the grid between them.
            let mut target = ts::min([a, b]);
            while target <= ts::max([a, b]) {
                axis_shifts.push([ts::sub(target, a), ts::sub(target, b)]);
                target = ts::add(target, self.p.grid);
            }
            let mut seen = FxHashSet::default();
            for shifts in &axis_shifts {
                for perpendicular in &PERPENDICULARS {
                    let key = [ts::fixed3(shifts[0]), ts::fixed3(shifts[1]), ts::fixed3(perpendicular[0]), ts::fixed3(perpendicular[1])];
                    if !seen.insert(key) { continue; }
                    let roots = [ra, rb];
                    if (0..2).any(|i| (ts::abs(shifts[i]) > 0.00001 || perpendicular[i] != 0.0) && !self.p.roots[roots[i]].movable) { continue; }
                    let mut offsets: Vec<(usize, [f64; 2])> = Vec::new();
                    for (k, &root) in roots.iter().enumerate() {
                        for &i in &self.p.roots[root].members {
                            let offset = [shifts[k], perpendicular[k]];
                            match offsets.iter_mut().find(|(j, _)| *j == i) { Some(entry) => entry.1 = offset, None => offsets.push((i, offset)) }
                        }
                    }
                    let round = |v: f64| ts::div(ts::js_round(ts::mul(v, 1000.0)), 1000.0);
                    let moved: Vec<_> = offsets.into_iter().filter(|(_, d)| d[0] != 0.0 || d[1] != 0.0).map(|(i, d)| {
                        let g = &w.geo[i];
                        let (along, across) = if axis == 0 { (g.x, g.y) } else { (g.y, g.x) };
                        let (along, across) = (round(ts::add(along, d[0])), round(ts::add(across, d[1])));
                        if axis == 0 { (i, along, across) } else { (i, across, along) }
                    }).collect();
                    let limit = ts::add(self.p.max_shift, 0.001);
                    if moved.iter().any(|&(i, x, y)| ts::hypot(ts::sub(x, self.base[i].0), ts::sub(y, self.base[i].1)) > limit) { continue; }
                    result.push(Candidate { axis, shifts: *shifts, perpendicular: *perpendicular, moved });
                }
            }
        }
        result
    }

    /// Candidate moves as (pose index, x, y) in the `ChangeCheck` frame.
    fn check_moves(&self, moved: &[(usize, f64, f64)]) -> Vec<(usize, f32, f32)> {
        moved.iter().map(|&(i, x, y)| {
            (self.p.components[i].pose_index, (x - self.check_origin[0]) as f32, (y - self.check_origin[1]) as f32)
        }).collect()
    }

    /// Every check of one candidate, in the original order. `limit` only
    /// decides the objective stage; later stages never depend on it.
    fn evaluate(&self, routes: &Routes, scratch: &mut Scratch, candidate: &Candidate, limit: f64, penalty_limit: f64, timing: &mut Timing) -> Result<Outcome, String> {
        let saved: Vec<_> = candidate.moved.iter().map(|&(i, _, _)| (i, scratch.world.geo[i].clone())).collect();
        let roots = self.roots_of(&candidate.moved);
        let saved_bbox: Vec<_> = roots.iter().map(|&r| (r, scratch.world.bbox[r])).collect();
        self.apply(&mut scratch.world, &candidate.moved);
        let outcome = self.stages(routes, scratch, candidate, limit, penalty_limit, timing);
        for (i, g) in saved { scratch.world.geo[i] = g; }
        for (r, b) in saved_bbox { scratch.world.bbox[r] = b; }
        outcome
    }
    fn stages(&self, routes: &Routes, scratch: &mut Scratch, candidate: &Candidate, limit: f64, penalty_limit: f64, timing: &mut Timing) -> Result<Outcome, String> {
        let w = &scratch.world;
        let started = Instant::now();
        let (next, penalty, full) = self.objective(w, routes)?;
        timing.objective += 1; timing.objective_ms += elapsed(started);
        let done = |stage| Ok(Outcome { next, stage });
        if next as f64 >= limit || penalty as f64 >= penalty_limit { return done(Stage::Objective); }
        let started = Instant::now();
        let regression = electrical_regression(&self.base_electrical, &self.electrical(w, &full, routes)?);
        timing.electrical += 1; timing.electrical_ms += elapsed(started);
        match regression { Some("wiring score") => return done(Stage::Electrical), Some(_) => return done(Stage::NetLength), None => {} }
        let started = Instant::now();
        let ok = self.report_ok(w);
        let hints = ok && self.hard_hints_ok(w);
        timing.report += 1; timing.report_ms += elapsed(started);
        if !ok { return done(Stage::Geometry); }
        if !hints { return done(Stage::HardHint); }
        let started = Instant::now();
        let valid = self.check.check(&mut scratch.change, &self.check_moves(&candidate.moved))?;
        timing.native += 1; timing.native_ms += elapsed(started);
        if !valid { return done(Stage::Native); }
        done(Stage::Accepted)
    }
}

fn elapsed(started: Instant) -> f64 { started.elapsed().as_secs_f64() * 1000.0 }

/// Outcomes of one pair's candidates, all against the layout at the start of
/// the pair. Serially, each objective limit is the exact running one. In
/// parallel, a worker uses the limit of the resolved prefix, which is never
/// tighter than the exact one, so any rejection it makes is also a serial
/// rejection; `select` then applies the exact limits in candidate order.
fn evaluate_pair(pass: &Pass, routes: &Routes, current: &World, scratch: &mut Scratch, candidates: &[Candidate], score: f32,
    penalty_limit: f64, threads: usize, timing: &mut Timing) -> Result<Vec<Outcome>, String> {
    let limit_after = |best: Option<f32>| ts::sub(best.unwrap_or(score) as f64, 1e-6);
    if threads <= 1 || candidates.len() < 2 {
        let mut best = None;
        return candidates.iter().map(|candidate| {
            let outcome = pass.evaluate(routes, scratch, candidate, limit_after(best), penalty_limit, timing)?;
            if outcome.stage == Stage::Accepted { best = Some(outcome.next); }
            Ok(outcome)
        }).collect();
    }
    use std::sync::{atomic::{AtomicU64, AtomicUsize, Ordering}, Mutex, OnceLock};
    let slots: Vec<OnceLock<Outcome>> = (0..candidates.len()).map(|_| OnceLock::new()).collect();
    let next = AtomicUsize::new(0);
    let limit = AtomicU64::new(limit_after(None).to_bits());
    let prefix = Mutex::new((0usize, None::<f32>));
    let workers = threads.min(candidates.len());
    let shared = &scratch.change;
    let timings = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers).map(|_| scope.spawn(|| -> Result<Timing, String> {
            let _float_env = crate::float_env::Guard::enter();
            // Workers start from the accepted layout, like the serial scratch.
            let mut scratch = Scratch { world: current.clone(), change: shared.clone() };
            let mut timing = Timing::default();
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= candidates.len() { break; }
                let bound = f64::from_bits(limit.load(Ordering::Acquire));
                let outcome = pass.evaluate(routes, &mut scratch, &candidates[i], bound, penalty_limit, &mut timing)?;
                let _ = slots[i].set(outcome);
                let mut resolved = prefix.lock().map_err(|_| "board alignment prefix poisoned")?;
                while let Some(outcome) = slots.get(resolved.0).and_then(OnceLock::get) {
                    if outcome.stage == Stage::Accepted && (outcome.next as f64) < limit_after(resolved.1) {
                        resolved.1 = Some(outcome.next);
                        limit.store(limit_after(resolved.1).to_bits(), Ordering::Release);
                    }
                    resolved.0 += 1;
                }
            }
            Ok(timing)
        })).collect();
        handles.into_iter().map(|h| h.join().map_err(|_| "board alignment worker panicked".to_string())?).collect::<Result<Vec<_>, String>>()
    })?;
    for t in &timings { timing.add(t); }
    slots.into_iter().map(|slot| slot.into_inner().ok_or_else(|| "board alignment candidate was not evaluated".to_string())).collect()
}

/// Serial acceptance with the exact running objective limit.
fn select(outcomes: &[Outcome], score: f32, counters: &mut Counters) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (index, outcome) in outcomes.iter().enumerate() {
        let limit = ts::sub(best.map_or(score, |b| b.1) as f64, 1e-6);
        let stage = if outcome.stage == Stage::Objective || outcome.next as f64 >= limit { Stage::Objective } else { outcome.stage };
        counters.count(stage);
        if stage == Stage::Accepted { best = Some((index, outcome.next)); }
    }
    best
}

/// JS-number geometry of `pcb-auto-place/geometry.ts` used by the report.
mod geometry {
    use super::{ts, BoxD, PointD};
    pub fn box_clearance_gap(a: &BoxD, b: &BoxD) -> f64 {
        let x = ts::max([ts::coordinate_difference(b.left, a.right), ts::coordinate_difference(a.left, b.right)]);
        let y = ts::max([ts::coordinate_difference(b.top, a.bottom), ts::coordinate_difference(a.top, b.bottom)]);
        ts::max([x, y])
    }
    pub fn box_point_gap(b: &BoxD, x: f64, y: f64) -> f64 {
        let gx = ts::max([ts::coordinate_difference(b.left, x), 0.0, ts::coordinate_difference(x, b.right)]);
        let gy = ts::max([ts::coordinate_difference(b.top, y), 0.0, ts::coordinate_difference(y, b.bottom)]);
        ts::sqrt(ts::add(ts::mul(gx, gx), ts::mul(gy, gy)))
    }
    pub fn box_overlap_depth(a: &BoxD, b: &BoxD) -> f64 {
        let x = ts::min([ts::sub(a.right, b.left), ts::sub(b.right, a.left)]);
        let y = ts::min([ts::sub(a.bottom, b.top), ts::sub(b.bottom, a.top)]);
        ts::max([0.0, ts::min([x, y])])
    }
    pub fn dist(a: PointD, b: PointD) -> f64 {
        let dx = ts::coordinate_difference(a.x, b.x);
        let dy = ts::coordinate_difference(a.y, b.y);
        ts::sqrt(ts::add(ts::mul(dx, dx), ts::mul(dy, dy)))
    }
    fn side(a: PointD, b: PointD, p: PointD) -> i32 {
        let dx = ts::sub(b.x, a.x);
        let dy = ts::sub(b.y, a.y);
        let cross = ts::sub(ts::mul(dx, ts::sub(p.y, a.y)), ts::mul(dy, ts::sub(p.x, a.x)));
        let band = ts::mul(ts::placement_epsilon(), ts::hypot(dx, dy));
        if cross > band { 1 } else if cross < -band { -1 } else { 0 }
    }
    fn on_segment(p: PointD, a: PointD, b: PointD) -> bool {
        let eps = ts::placement_epsilon();
        side(a, b, p) == 0
            && p.x <= ts::add(ts::max([a.x, b.x]), eps) && p.x >= ts::sub(ts::min([a.x, b.x]), eps)
            && p.y <= ts::add(ts::max([a.y, b.y]), eps) && p.y >= ts::sub(ts::min([a.y, b.y]), eps)
    }
    fn in_polygon(p: PointD, polygon: &[PointD]) -> bool {
        let (ox, oy) = (ts::coordinate_origin(p.x), ts::coordinate_origin(p.y));
        if ox != 0.0 || oy != 0.0 {
            let local = |q: &PointD| PointD { x: ts::f(q.x - ox), y: ts::f(q.y - oy) };
            return in_polygon(local(&p), &polygon.iter().map(local).collect::<Vec<_>>());
        }
        if polygon.len() < 3 { return false; }
        let mut inside = false;
        let mut j = polygon.len() - 1;
        for i in 0..polygon.len() {
            let (a, b) = (polygon[i], polygon[j]);
            if on_segment(p, a, b) { return true; }
            let crosses = ((a.y > p.y) != (b.y > p.y))
                && p.x < ts::add(ts::div(ts::mul(ts::sub(b.x, a.x), ts::sub(p.y, a.y)), ts::sub(b.y, a.y)), a.x);
            if crosses { inside = !inside; }
            j = i;
        }
        inside
    }
    fn segment_distance(p: PointD, a: PointD, b: PointD) -> f64 {
        let (ox, oy) = (ts::coordinate_origin(p.x), ts::coordinate_origin(p.y));
        if ox != 0.0 || oy != 0.0 {
            let local = |q: PointD| PointD { x: ts::f(q.x - ox), y: ts::f(q.y - oy) };
            return segment_distance(local(p), local(a), local(b));
        }
        let dx = ts::sub(b.x, a.x);
        let dy = ts::sub(b.y, a.y);
        let length = ts::add(ts::mul(dx, dx), ts::mul(dy, dy));
        if length < 1e-6 { return dist(p, a); }
        let t = ts::max([0.0, ts::min([1.0, ts::div(ts::add(ts::mul(ts::sub(p.x, a.x), dx), ts::mul(ts::sub(p.y, a.y), dy)), length)])]);
        let px = ts::sub(p.x, ts::add(a.x, ts::mul(t, dx)));
        let py = ts::sub(p.y, ts::add(a.y, ts::mul(t, dy)));
        ts::sqrt(ts::add(ts::mul(px, px), ts::mul(py, py)))
    }
    fn polygon_distance(p: PointD, polygon: &[PointD]) -> f64 {
        let mut min = f64::INFINITY;
        for i in 0..polygon.len() {
            let d = segment_distance(p, polygon[i], polygon[(i + 1) % polygon.len()]);
            if d < min { min = d; }
        }
        min
    }
    fn in_board(board: &BoxD, polygon: &[PointD], p: PointD, edge: f64) -> bool {
        let eps = ts::placement_epsilon();
        if p.x < ts::sub(ts::add(board.left, edge), eps) || p.x > ts::add(ts::sub(board.right, edge), eps)
            || p.y < ts::sub(ts::add(board.top, edge), eps) || p.y > ts::add(ts::sub(board.bottom, edge), eps) { return false; }
        if !in_polygon(p, polygon) { return false; }
        edge <= 0.0 || ts::add(polygon_distance(p, polygon), eps) >= edge
    }
    /// `boxInsideBoard` for a polygon outline.
    pub fn box_inside_board(board: &BoxD, polygon: &[PointD], b: &BoxD, edge: f64) -> bool {
        let corners = [PointD { x: b.left, y: b.top }, PointD { x: b.right, y: b.top }, PointD { x: b.right, y: b.bottom }, PointD { x: b.left, y: b.bottom }];
        if !corners.iter().all(|c| in_board(board, polygon, *c, edge)) { return false; }
        let eps = ts::placement_epsilon();
        if polygon.iter().any(|p| ts::sub(p.x, b.left) > eps && ts::sub(b.right, p.x) > eps && ts::sub(p.y, b.top) > eps && ts::sub(b.bottom, p.y) > eps) { return false; }
        (0..4).all(|k| {
            let (a, c) = (corners[k], corners[(k + 1) % 4]);
            (0..polygon.len()).all(|i| {
                let (p, q) = (polygon[i], polygon[(i + 1) % polygon.len()]);
                side(a, c, p) * side(a, c, q) >= 0 || side(p, q, a) * side(p, q, c) >= 0
            })
        })
    }
}

pub fn solve(mut problem: Value) -> Result<Value, String> {
    let started = Instant::now();
    let constraints = problem.get_mut("constraints").map(Value::take).ok_or("missing board alignment constraints")?;
    let p: Problem = serde_json::from_value(problem).map_err(|e| e.to_string())?;
    let pass = Pass::new(p, constraints)?;
    let setup_ms = elapsed(started);
    let threads = pass.p.threads.max(1).min(std::thread::available_parallelism().map_or(1, |n| n.get()));
    let mut current = pass.world();
    let mut scratch = Scratch { world: current.clone(), change: pass.check.scratch() };
    let mut routes = pass.routes(&current);
    let (mut score, _, _) = pass.objective(&current, &routes)?;
    let mut counters = Counters::default();
    let mut timing = Timing::default();
    let mut evaluated = 0usize;
    let mut wall_ms = 0.0;
    let mut moves = Vec::new();
    for _ in 0..pass.p.passes {
        let mut accepted = false;
        for pair in &pass.p.pairs {
            if pair.a < 0 || pair.b < 0 { return Err("board alignment pair references a missing root".into()); }
            let candidates = pass.candidates(&current, pair);
            let current_penalty = pass.penalty(&current);
            let penalty_limit = ts::sub(current_penalty as f64, 1e-6);
            evaluated += candidates.len();
            let started = Instant::now();
            let outcomes = evaluate_pair(&pass, &routes, &current, &mut scratch, &candidates, score, penalty_limit, threads, &mut timing)?;
            wall_ms += elapsed(started);
            let Some((index, next)) = select(&outcomes, score, &mut counters) else { continue };
            let candidate = &candidates[index];
            moves.push(json!({"blocks":[pass.p.roots[pair.a as usize].label.as_ref(),pass.p.roots[pair.b as usize].label.as_ref()],
                "axis":if candidate.axis == 0 {"x"} else {"y"},"shifts":candidate.shifts,"perpendicularShifts":candidate.perpendicular,
                "scoreBefore":score,"scoreAfter":next}));
            pass.apply(&mut current, &candidate.moved);
            pass.apply(&mut scratch.world, &candidate.moved);
            pass.check.accept(&mut scratch.change, &pass.check_moves(&candidate.moved))?;
            routes = pass.routes(&current);
            score = next;
            accepted = true;
        }
        if !accepted { break; }
    }
    let mut positions: Vec<Option<[f64; 2]>> = vec![None; pass.p.placements.len()];
    for (i, c) in pass.p.components.iter().enumerate() {
        let g = &current.geo[i];
        let initial = &pass.p.placements[c.pose_index];
        if g.x != initial.x || g.y != initial.y { positions[c.pose_index] = Some([g.x, g.y]); }
    }
    let profile = json!({"threads":threads,"setupMs":setup_ms,"totalMs":elapsed(started),"evaluationWallMs":wall_ms,
        "objective":{"count":timing.objective,"ms":timing.objective_ms},"electrical":{"count":timing.electrical,"ms":timing.electrical_ms},
        "report":{"count":timing.report,"ms":timing.report_ms},"native":{"count":timing.native,"ms":timing.native_ms}});
    if std::env::var_os("PCB_BOARD_PACKER_PROFILE").is_some() { eprintln!("[pcb-board-alignment-native] {profile}"); }
    Ok(json!({"positions":positions,"moves":moves,"evaluated":evaluated,
        "rejected":{"objective":counters.objective,"electrical":counters.electrical,"netLength":counters.net_length,
            "geometry":counters.geometry,"hardHint":counters.hard_hint,"native":counters.native},
        "profile":profile}))
}
