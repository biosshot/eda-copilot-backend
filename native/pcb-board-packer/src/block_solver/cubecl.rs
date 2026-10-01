//! CubeCL block scorer; host layout and guards adapted from archived opencl.rs.
use super::gpu_kernels as k;
use super::*;
use ::cubecl::prelude::*;
use ::cubecl::server::Handle;
use ::cubecl::wgpu::WgpuRuntime;
// Local meanings and layout belong to this scorer, never to compute.
#[derive(Clone, Copy)]
enum ScoreScratch { Hull, HullCount, Pads, Segments, SegmentTags, Costs, Scores, Tags, Best, Mask, OutputIds, OutputScores }
impl ScoreScratch {
    fn key(self) -> crate::compute::ScratchKey {
        crate::compute::ScratchKey::new("block-score", self as usize)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct B {
    l: f64,
    r: f64,
    t: f64,
    b: f64,
}
impl From<Box2> for B {
    fn from(v: Box2) -> Self {
        Self {
            l: v.left,
            r: v.right,
            t: v.top,
            b: v.bottom,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Prim {
    bbox: B,
    body: B,
    pad_dx: f64,
    pad_dy: f64,
    cp: u32,
    nc: u32,
    pad: u32,
    np: u32,
    component: u32,
    layer: i32,
    pins: u32,
    role: u32,
    power: u32,
    unused: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Cp {
    x: f64,
    y: f64,
    net: i32,
    owner: i32,
    layer: i32,
    unused: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Ref {
    primitive: u32,
    point: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Net {
    start: u32,
    count: u32,
    primitives: u32,
    ground: u32,
    id: i32,
    units: i32,
    segment: u32,
    unused: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Endpoint {
    primitive: i32,
    start: u32,
    count: u32,
    unused: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rel {
    from: Endpoint,
    to: Endpoint,
    weight: f64,
    min: f64,
    max: f64,
    ox: f64,
    oy: f64,
    dx: f64,
    dy: f64,
    hard: u32,
    offset: u32,
    side: u32,
    unused: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Config {
    clearance: f64,
    nprim: u32,
    moving: u32,
    ncandidate: u32,
    ncp: u32,
    npad: u32,
    nnet: u32,
    nref: u32,
    nrel: u32,
    nepref: u32,
    ncomponent: u32,
    nsegment: u32,
    high: u32,
    reduced: u32,
    smooth: u32,
    small: u32,
    dense: u32,
}
fn supported(context: &Context) -> bool {
    let p = &context.problem;
    if p.experiments.routing_metric != crate::model::BlockRoutingMetric::Geometric
        || p.primitives.iter().any(|p| {
            p.locked
                || p.allowed_orientations
                    .iter()
                    .any(|r| !matches!(normalize_rotation(*r), 0 | 90 | 180 | 270))
        })
        || p.primitives.len() > 20
        || p.components.len() > 20
        || p.world.is_some()
        || p.bounds.is_some()
        || !p.obstacles.is_empty()
        || p.target_width.is_some()
        || p.target_height.is_some()
        || p.collision_mode.as_ref() != "components"
        || p.hard_collision_mode.as_ref() != "components"
        || !p.experiments.local_access
        || !p.experiments.pad_crossings
        || !p.experiments.stable_net_weight
        || !context.split_pad_cache_safe
    {
        return false;
    }
    let mut seen = FxHashSet::default();
    for pads in &context.source_pads {
        for pad in pads {
            if pad
                .reference
                .as_ref()
                .is_some_and(|r| !seen.insert(r.clone()))
            {
                return false;
            }
            if pad
                .layer
                .as_deref()
                .is_some_and(|l| l != "top" && l != "bottom")
            {
                return false;
            }
        }
    }
    for prim in &p.primitives {
        if prim.placements.len() != 1
            || prim.collision_boxes.len() != 1
            || prim.collision_boxes[0] != prim.bbox
            || !prim.path_ports.is_empty()
        {
            return false;
        }
        let cs: Vec<_> = p
            .components
            .iter()
            .filter(|c| c.primitive_id == prim.id)
            .collect();
        if cs.len() != 1
            || !cs[0].through_hole_boxes.is_empty()
            || !matches!(cs[0].layer.as_ref(), "top" | "bottom")
        {
            return false;
        }
        // Any subset containing this non-power primitive exceeds the power-yield endpoint limit.
        // Other subsets consist entirely of power components and have no non-power signal corridors.
        if !cs[0].power_component && prim.connection_points.len() <= 240 {
            return false;
        }
    }
    for rel in &context.relations {
        let r = &rel.relation;
        if r.kind.as_ref() == "net" || r.effect.as_ref() == "lock" {
            continue;
        }
        if matches!(rel.from, CompiledEndpoint::Anchor(Some(_)))
            || matches!(rel.to, CompiledEndpoint::Anchor(Some(_)))
            || r.kind.as_ref() == "critical_pair"
            || r.kind.as_ref() == "island_target"
            || matches!(r.relation.as_deref(), Some("very_near" | "cap_cluster"))
            || r.path_id.is_some()
        {
            return false;
        }
    }
    if p.experiments.long_nets {
        let mut owner = FxHashMap::default();
        for (i, prim) in p.primitives.iter().enumerate() {
            for cp in prim.connection_points.iter() {
                if let Some(n) = &cp.net {
                    if n.is_empty()
                        || is_ground(n)
                        || context.net_endpoint_counts.get(n) != Some(&2)
                        || p.experiments
                            .ignored_nets
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(n))
                    {
                        continue;
                    }
                    if owner.insert(n, i).is_some_and(|old| old != i) {
                        return false;
                    }
                }
            }
        }
    }
    true
}
fn layer(l: Option<&str>) -> i32 {
    match l {
        Some("top") => 0,
        Some("bottom") => 1,
        _ => -1,
    }
}
fn endpoint(e: &CompiledEndpoint, prims: &[WorkingPrimitive], indices: &mut Vec<u32>) -> Endpoint {
    let (id, points) = match e {
        CompiledEndpoint::Pad {
            primitive_id,
            point_index,
        } => (*primitive_id, vec![*point_index]),
        CompiledEndpoint::Component {
            primitive_id,
            point_indices,
        } => (*primitive_id, point_indices.as_ref().clone()),
        CompiledEndpoint::Primitive { primitive_id } => (*primitive_id, vec![]),
        _ => {
            return Endpoint {
                primitive: -1,
                ..Default::default()
            }
        }
    };
    let primitive = prims
        .iter()
        .position(|p| p.id == id)
        .map_or(-1, |i| i as i32);
    let start = indices.len() as u32;
    indices.extend(points.iter().map(|&x| x as u32));
    Endpoint {
        primitive,
        start,
        count: points.len() as u32,
        unused: 0,
    }
}

#[path = "compact_candidates.rs"]
mod candidates;
#[path = "gpu_frontier.rs"]
mod frontier;
pub(super) use frontier::scarcity;
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Pad {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    net: i32,
    layer: i32,
    owner: i32,
    unused: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
pub(super) struct Pose {
    template_index: u32,
    ordinal: u32,
    dx: f64,
    dy: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Row {
    index: u32,
    hard: u32,
    base: f64,
}

pub(super) struct Engine {
    templates: Vec<Prim>,
    cp: Vec<Cp>,
    pads: Vec<Pad>,
    names: Vec<Arc<str>>,
    ids: FxHashMap<Arc<str>, i32>,
    sf: Vec<f64>,
    si: Vec<i32>,
    offsets: [u32; 6],
    handles: Option<(Handle, Handle)>,
    pub batches: usize,
    pub candidates: usize,
    fail_batch: Option<usize>,
    frontiers: FxHashMap<Vec<PrimitivePoseKey>, frontier::Frame>,
    frontier_batches: usize,
    frontier_candidates: usize,
}
impl Engine {
    fn new(context: &Context) -> Self {
        let names: Vec<_> = context
            .problem
            .primitives
            .iter()
            .flat_map(|p| p.connection_points.iter())
            .filter_map(|p| p.net.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let ids: FxHashMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as i32))
            .collect();
        let owners: FxHashMap<_, _> = context
            .problem
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| (c.designator.clone(), i as i32))
            .collect();
        let mut engine = Self {
            templates: vec![],
            cp: vec![],
            pads: vec![],
            names,
            ids,
            sf: vec![],
            si: vec![],
            offsets: [0; 6],
            handles: None,
            batches: 0,
            candidates: 0,
            frontiers: Default::default(),
            frontier_batches: 0,
            frontier_candidates: 0,
            fail_batch: std::env::var("PCB_BLOCK_GPU_FAIL_BATCH")
                .ok()
                .and_then(|v| v.parse().ok()),
        };
        for original in &context.gpu_sources {
            for rotation in [0, 90, 180, 270] {
                let p = rotate_primitive(original, rotation);
                let cp = engine.cp.len() as u32;
                let pad = engine.pads.len() as u32;
                for (i, v) in p.primitive.connection_points.iter().enumerate() {
                    let (l, o) = &context.pad_point_metadata[p.source_index][i];
                    engine.cp.push(Cp {
                        x: v.x,
                        y: v.y,
                        net: v
                            .net
                            .as_ref()
                            .and_then(|n| engine.ids.get(n).copied())
                            .unwrap_or(-1),
                        owner: o
                            .as_ref()
                            .and_then(|n| owners.get(n).copied())
                            .unwrap_or(-1),
                        layer: layer(l.as_deref()),
                        unused: v.net.as_ref().is_some_and(|name| {
                            !name.is_empty()
                                && !is_ground(name)
                                && !context
                                    .problem
                                    .experiments
                                    .ignored_nets
                                    .iter()
                                    .any(|n| n.eq_ignore_ascii_case(name))
                        }) as i32,
                    });
                }
                let origin = box_center(&original.primitive.bbox);
                for v in &context.source_pads[p.source_index] {
                    let b = rotate_box(&v.box_, &origin, rotation);
                    engine.pads.push(Pad {
                        left: b.left,
                        right: b.right,
                        top: b.top,
                        bottom: b.bottom,
                        net: v
                            .net
                            .as_ref()
                            .and_then(|n| engine.ids.get(n).copied())
                            .unwrap_or(-1),
                        layer: layer(v.layer.as_deref()),
                        owner: v
                            .reference
                            .as_deref()
                            .and_then(|s| s.rsplit_once('.'))
                            .and_then(|(s, _)| owners.get(s).copied())
                            .unwrap_or(-1),
                        unused: 0,
                    });
                }
                let (ci, c) = &p.components[0];
                engine.templates.push(Prim {
                    bbox: p.primitive.bbox.into(),
                    body: c.body_box.into(),
                    pad_dx: 0.,
                    pad_dy: 0.,
                    cp,
                    nc: p.primitive.connection_points.len() as u32,
                    pad,
                    np: engine.pads.len() as u32 - pad,
                    component: *ci as u32,
                    layer: layer(Some(&c.layer)),
                    pins: c.pin_count as u32,
                    role: match c.role.as_deref() {
                        Some("main_ic") => 1,
                        Some("decoupling_cap") => 2,
                        Some("passive") => 3,
                        Some("connector") => 4,
                        _ => 0,
                    },
                    power: c.power_component as u32,
                    unused: 0,
                });
            }
        }
        if engine.templates.iter().any(|p| p.np > 512) {
            fail("GPU moving-pad workspace limit exceeded".into());
        }
        engine.pack_static(context);
        engine
    }
    fn frame_primitive(&self, p: &WorkingPrimitive, context: &Context) -> Prim {
        let template =
            self.templates[p.source_index * 4 + normalize_rotation(p.rotation) as usize / 90];
        let a = box_center(&context.problem.primitives[p.source_index].bbox);
        let b = box_center(&p.primitive.bbox);
        Prim {
            bbox: p.primitive.bbox.into(),
            body: p.components[0].1.body_box.into(),
            pad_dx: round_placement(b.x - a.x),
            pad_dy: round_placement(b.y - a.y),
            ..template
        }
    }
}
fn materialize(primitive: &WorkingPrimitive, pose: Pose) -> WorkingPrimitive {
    let p = rotate_primitive(
        primitive,
        normalize_rotation((pose.template_index % 4) as i32 * 90 - primitive.rotation),
    );
    translate_primitive(&p, pose.dx, pose.dy)
}

fn prepare(
    engine: &Engine,
    current: &[WorkingPrimitive],
    moving: usize,
    poses: &[Pose],
    context: &Context,
) -> (Config, Vec<Prim>, Vec<Net>, Vec<Ref>, Vec<Rel>, Vec<u32>) {
    let prims: Vec<_> = current
        .iter()
        .map(|p| engine.frame_primitive(p, context))
        .collect();
    let mut by_net: FxHashMap<i32, Vec<Ref>> = FxHashMap::default();
    let mut order = Vec::new();
    for (i, p) in current.iter().enumerate() {
        for (j, v) in p.primitive.connection_points.iter().enumerate() {
            if let Some(id) = v.net.as_ref().and_then(|n| engine.ids.get(n)) {
                if !by_net.contains_key(id) {
                    order.push(*id);
                }
                by_net.entry(*id).or_default().push(Ref {
                    primitive: i as u32,
                    point: j as u32,
                });
            }
        }
    }
    let mut refs = Vec::new();
    let mut nets = Vec::new();
    let mut nsegment = 0;
    for id in order {
        let r = &by_net[&id];
        let name = &engine.names[id as usize];
        let ground = is_ground(name);
        let eligible = !name.is_empty()
            && !ground
            && !context
                .problem
                .experiments
                .ignored_nets
                .iter()
                .any(|s| s.eq_ignore_ascii_case(name));
        let units = if eligible {
            if is_power(name) {
                1
            } else {
                4
            }
        } else {
            0
        };
        if units != 0 && r.len() > 32 {
            fail("GPU net exceeds the 32-endpoint workspace".into());
        }
        nets.push(Net {
            start: refs.len() as u32,
            count: r.len() as u32,
            primitives: r
                .iter()
                .map(|r| r.primitive)
                .collect::<FxHashSet<_>>()
                .len() as u32,
            ground: ground as u32,
            id,
            units,
            segment: nsegment,
            unused: (context.problem.external_nets.contains(name) as u32)
                | ((r.iter().any(|r| r.primitive == moving as u32) as u32) << 1),
        });
        if units > 0 {
            nsegment += r.len().saturating_sub(1) as u32;
        }
        refs.extend_from_slice(r);
    }
    if nsegment > 128 || nets.len() > 256 || engine.names.len() > 256 {
        fail("GPU frame exceeds net/segment workspace limits".into());
    }
    let mut eprefs = Vec::new();
    let mut relations = Vec::new();
    for r in &context.relations {
        let rel = &r.relation;
        if rel.kind.as_ref() == "net" || rel.effect.as_ref() == "lock" {
            continue;
        }
        let (dx, dy) = match rel.side_preference.as_deref() {
            Some("left") => (-1., 0.),
            Some("right") => (1., 0.),
            Some("top") => (0., -1.),
            Some("bottom") => (0., 1.),
            _ => (0., 0.),
        };
        let o = rel.anchor_offset.unwrap_or(Point { x: 0., y: 0. });
        relations.push(Rel {
            from: endpoint(&r.from, &current, &mut eprefs),
            to: endpoint(&r.to, &current, &mut eprefs),
            weight: relation_weight(rel),
            min: rel.min_distance.unwrap_or(-1.),
            max: rel.max_distance.unwrap_or(-1.),
            ox: o.x,
            oy: o.y,
            dx,
            dy,
            hard: rel.hard as u32,
            offset: (rel.satellite_anchor && rel.anchor_offset.is_some()) as u32,
            side: (rel.satellite_anchor && (dx != 0. || dy != 0.)) as u32,
            unused: ((rel.effect.as_ref() != "score_only")
                && (matches!(
                    r.from,
                    CompiledEndpoint::Missing | CompiledEndpoint::Anchor(None)
                ) != matches!(
                    r.to,
                    CompiledEndpoint::Missing | CompiledEndpoint::Anchor(None)
                ))) as u32,
        });
    }
    let c = Config {
        clearance: context.problem.clearance,
        nprim: prims.len() as u32,
        moving: moving as u32,
        ncandidate: poses.len() as u32,
        ncp: engine.cp.len() as u32,
        npad: engine.pads.len() as u32,
        nnet: nets.len() as u32,
        nref: refs.len() as u32,
        nrel: relations.len() as u32,
        nepref: eprefs.len() as u32,
        ncomponent: context.problem.components.len() as u32,
        nsegment,
        high: (context.problem.compactness.as_ref() == "high") as u32,
        reduced: context.problem.experiments.reduced_hull as u32,
        smooth: context.problem.experiments.smooth_aspect as u32,
        small: (context.problem.components.len() < 5) as u32,
        dense: (context.problem.search_width <= 1 || context.problem.experiments.keep_dense_access)
            as u32,
    };
    (c, prims, nets, refs, relations, eprefs)
}

fn prim_f(p: Prim) -> [f64; 10] {
    [
        p.bbox.l, p.bbox.r, p.bbox.t, p.bbox.b, p.body.l, p.body.r, p.body.t, p.body.b, p.pad_dx,
        p.pad_dy,
    ]
}
fn prim_i(p: Prim) -> [i32; 9] {
    [
        p.cp as i32,
        p.nc as i32,
        p.pad as i32,
        p.np as i32,
        p.component as i32,
        p.layer,
        p.pins as i32,
        p.role as i32,
        p.power as i32,
    ]
}
fn finite(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite() && v.abs() <= 1e12)
}
impl Engine {
    pub(super) fn counters(&self) -> (usize, usize) {
        (
            self.batches + self.frontier_batches,
            self.candidates + self.frontier_candidates,
        )
    }
    fn pack_static(&mut self, context: &Context) {
        for &p in &self.templates {
            self.sf.extend(prim_f(p));
            self.si.extend(prim_i(p));
        }
        self.offsets[0] = self.sf.len() as u32;
        self.offsets[3] = self.si.len() as u32;
        for p in &self.cp {
            self.sf.extend([p.x, p.y]);
            self.si.extend([p.net, p.owner, p.layer, p.unused]);
        }
        self.offsets[1] = self.sf.len() as u32;
        self.offsets[4] = self.si.len() as u32;
        for p in &self.pads {
            self.sf.extend([p.left, p.right, p.top, p.bottom]);
            self.si.extend([p.net, p.layer, p.owner]);
        }
        self.offsets[2] = self.sf.len() as u32;
        self.offsets[5] = self.si.len() as u32;
        self.sf.extend(&context.problem.component_pair_clearance);
        self.si
            .extend(context.problem.component_conflict.iter().map(|v| *v as i32));
    }
    fn evaluate(
        &mut self,
        current: &[WorkingPrimitive],
        moving: usize,
        poses: &[Pose],
        diverse: bool,
        origin: Point,
        context: &Context,
        force_no_prune: bool,
    ) -> Result<Vec<Row>, String> {
        let preparing = context.detail.span("gpu_frame_prepare");
        let (c, prims, nets, refs, relations, eprefs) =
            prepare(self, current, moving, poses, context);
        let mut ff = vec![c.clearance, origin.x, origin.y];
        for &p in &prims {
            ff.extend(prim_f(p));
        }
        let mut fi = vec![0u32; k::HEADER];
        fi[..13].copy_from_slice(&[
            c.nprim,
            c.moving,
            c.ncandidate,
            c.nnet,
            c.nrel,
            c.ncomponent,
            c.nsegment,
            c.high,
            c.reduced,
            c.smooth,
            c.small,
            c.dense,
            diverse as u32,
        ]);
        fi[k::CP_F..k::CP_F + 6].copy_from_slice(&self.offsets);
        fi[k::FIXED_I] = fi.len() as u32;
        for &p in &prims {
            fi.extend(prim_i(p).map(|v| v as u32));
        }
        fi[k::NET_I] = fi.len() as u32;
        for n in &nets {
            fi.extend([
                n.start,
                n.count,
                n.primitives,
                n.ground,
                n.id as u32,
                n.units as u32,
                n.segment,
                n.unused,
            ]);
        }
        fi[k::REF_I] = fi.len() as u32;
        for r in &refs {
            fi.extend([r.primitive, r.point]);
        }
        fi[k::REL_I] = fi.len() as u32;
        for r in &relations {
            ff.extend([r.weight, r.min, r.max, r.ox, r.oy, r.dx, r.dy]);
            fi.extend([
                r.from.primitive as u32,
                r.from.start,
                r.from.count,
                r.to.primitive as u32,
                r.to.start,
                r.to.count,
                r.hard,
                r.offset,
                r.side,
                r.unused,
            ]);
        }
        fi[k::EP_I] = fi.len() as u32;
        fi.extend(eprefs);
        // Archived proposals are relative to the current moving primitive.
        // GPU templates are resident in the original coordinate frame.
        let source = box_center(&context.problem.primitives[current[moving].source_index].bbox);
        let now = box_center(&current[moving].primitive.bbox);
        let shift = Point {
            x: round_placement(now.x - source.x),
            y: round_placement(now.y - source.y),
        };
        let pf: Vec<f64> = poses
            .iter()
            .flat_map(|p| {
                [
                    round_placement(p.dx + shift.x),
                    round_placement(p.dy + shift.y),
                ]
            })
            .collect();
        let pi: Vec<u32> = poses
            .iter()
            .flat_map(|p| [p.template_index, p.ordinal])
            .collect();
        if !finite(&self.sf) || !finite(&ff) || !finite(&pf) {
            return Err("unsupported numeric range in GPU block data".into());
        }
        let pad_capacity = self.templates[current[moving].source_index * 4]
            .np
            .max(1)
            .next_power_of_two() as usize;
        let verify = std::env::var_os("PCB_BLOCK_GPU_VERIFY").is_some();
        let no_prune =
            force_no_prune || verify || std::env::var_os("PCB_BLOCK_GPU_NO_PRUNE").is_some();
        drop(preparing);
        let batch_span = context.detail.span("gpu_batch");
        let (rows, all) = gpu_runtime::with_session(GPU_REQUIREMENTS, |session| {
            if self.fail_batch == Some(self.batches + 1) {
                panic!("injected GPU runtime failure at batch {}", self.batches + 1);
            }
            let upload = context.detail.span("gpu_upload");
            if self.handles.is_none() {
                self.handles = Some((
                    session.client.create_from_slice(f64::as_bytes(&self.sf)),
                    session.client.create_from_slice(i32::as_bytes(&self.si)),
                ));
            }
            let (sf, si) = self.handles.as_ref().unwrap();
            let fh = session.client.create_from_slice(f64::as_bytes(&ff));
            let ih = session.client.create_from_slice(u32::as_bytes(&fi));
            let ph = session.client.create_from_slice(f64::as_bytes(&pf));
            let th = session.client.create_from_slice(u32::as_bytes(&pi));
            let hull = session.workspace(ScoreScratch::Hull.key(), 160 * 8);
            let hc = session.workspace(ScoreScratch::HullCount.key(), 4);
            let pads = session.workspace(ScoreScratch::Pads.key(), self.pads.len() * 4 * 8);
            let segments = session.workspace(ScoreScratch::Segments.key(), c.nsegment as usize * 4 * 8);
            let st = session.workspace(ScoreScratch::SegmentTags.key(), c.nsegment as usize * 4 * 4);
            let costs = session.workspace(ScoreScratch::Costs.key(), c.nsegment as usize * 4);
            let scores = session.workspace(ScoreScratch::Scores.key(), poses.len() * 8);
            let tags = session.workspace(ScoreScratch::Tags.key(), poses.len() * 3 * 4);
            let best = session.workspace(ScoreScratch::Best.key(), 128 * 4);
            let mask = session.workspace(ScoreScratch::Mask.key(), poses.len() * 4);
            let oi = session.workspace(ScoreScratch::OutputIds.key(), 130 * 4);
            let of = session.workspace(ScoreScratch::OutputScores.key(), 64 * 8);
            let client = &session.client;
            let input = || unsafe {
                k::InputLaunch::new(
                    ArrayArg::from_raw_parts(sf.clone(), self.sf.len()),
                    ArrayArg::from_raw_parts(si.clone(), self.si.len()),
                    ArrayArg::from_raw_parts(fh.clone(), ff.len()),
                    ArrayArg::from_raw_parts(ih.clone(), fi.len()),
                    ArrayArg::from_raw_parts(ph.clone(), pf.len()),
                    ArrayArg::from_raw_parts(th.clone(), pi.len()),
                )
            };
            drop(upload);
            let dispatch = context.detail.span("gpu_dispatch");
            let count = CubeCount::Static(poses.len().div_ceil(128) as u32, 1, 1);
            let dim = CubeDim::new_1d(128);
            unsafe {
                macro_rules! a {
                    ($h:expr,$n:expr) => {
                        ArrayArg::from_raw_parts($h.clone(), $n as usize)
                    };
                }
                k::frame_hull::launch_unchecked::<WgpuRuntime>(
                    client,
                    CubeCount::Static(1, 1, 1),
                    CubeDim::new_1d(1),
                    input(),
                    a!(hull, 160),
                    a!(hc, 1),
                );
                k::frame_pads::launch_unchecked::<WgpuRuntime>(
                    client,
                    CubeCount::Static(c.nprim, 1, 1),
                    dim,
                    input(),
                    a!(pads, self.pads.len() * 4),
                );
                if c.nnet > 0 && c.nsegment > 0 {
                    k::frame_mst::launch_unchecked::<WgpuRuntime>(
                        client,
                        CubeCount::Static(c.nnet.div_ceil(32), 1, 1),
                        CubeDim::new_1d(32),
                        input(),
                        a!(segments, c.nsegment * 4),
                        a!(st, c.nsegment * 4),
                    );
                    k::frame_hits::launch_unchecked::<WgpuRuntime>(
                        client,
                        CubeCount::Static(c.nsegment, 1, 1),
                        dim,
                        input(),
                        a!(segments, c.nsegment * 4),
                        a!(st, c.nsegment * 4),
                        a!(pads, self.pads.len() * 4),
                        a!(costs, c.nsegment),
                    );
                }
                k::cheap::launch_unchecked::<WgpuRuntime>(
                    client,
                    count.clone(),
                    dim,
                    input(),
                    a!(hull, 160),
                    a!(hc, 1),
                    a!(scores, poses.len()),
                    a!(tags, poses.len() * 3),
                );
                if !no_prune {
                    k::clear_best::launch_unchecked::<WgpuRuntime>(
                        client,
                        CubeCount::Static(1, 1, 1),
                        dim,
                        a!(best, 128),
                    );
                    k::rank::launch_unchecked::<WgpuRuntime>(
                        client,
                        count.clone(),
                        dim,
                        input(),
                        a!(scores, poses.len()),
                        a!(tags, poses.len() * 3),
                        a!(best, 128),
                        a!(mask, poses.len()),
                        0,
                        1,
                    );
                }
                k::full::launch_unchecked::<WgpuRuntime>(
                    client,
                    CubeCount::Static(c.ncandidate, 1, 1),
                    dim,
                    input(),
                    a!(segments, c.nsegment * 4),
                    a!(st, c.nsegment * 4),
                    a!(costs, c.nsegment),
                    a!(pads, self.pads.len() * 4),
                    a!(scores, poses.len()),
                    a!(tags, poses.len() * 3),
                    a!(mask, poses.len()),
                    (!no_prune) as u32,
                    pad_capacity,
                );
                if !no_prune {
                    k::clear_best::launch_unchecked::<WgpuRuntime>(
                        client,
                        CubeCount::Static(1, 1, 1),
                        dim,
                        a!(best, 128),
                    );
                    k::rank::launch_unchecked::<WgpuRuntime>(
                        client,
                        count.clone(),
                        dim,
                        input(),
                        a!(scores, poses.len()),
                        a!(tags, poses.len() * 3),
                        a!(best, 128),
                        a!(mask, poses.len()),
                        1,
                        0,
                    );
                    k::prune::launch_unchecked::<WgpuRuntime>(
                        client,
                        count.clone(),
                        dim,
                        input(),
                        a!(scores, poses.len()),
                        a!(tags, poses.len() * 3),
                        a!(best, 128),
                        a!(mask, poses.len()),
                    );
                    k::full::launch_unchecked::<WgpuRuntime>(
                        client,
                        CubeCount::Static(c.ncandidate, 1, 1),
                        dim,
                        input(),
                        a!(segments, c.nsegment * 4),
                        a!(st, c.nsegment * 4),
                        a!(costs, c.nsegment),
                        a!(pads, self.pads.len() * 4),
                        a!(scores, poses.len()),
                        a!(tags, poses.len() * 3),
                        a!(mask, poses.len()),
                        1,
                        pad_capacity,
                    );
                }
                k::clear_best::launch_unchecked::<WgpuRuntime>(
                    client,
                    CubeCount::Static(1, 1, 1),
                    dim,
                    a!(best, 128),
                );
                k::rank::launch_unchecked::<WgpuRuntime>(
                    client,
                    count,
                    dim,
                    input(),
                    a!(scores, poses.len()),
                    a!(tags, poses.len() * 3),
                    a!(best, 128),
                    a!(mask, poses.len()),
                    1,
                    0,
                );
                k::compact::launch_unchecked::<WgpuRuntime>(
                    client,
                    CubeCount::Static(1, 1, 1),
                    CubeDim::new_1d(1),
                    input(),
                    a!(scores, poses.len()),
                    a!(tags, poses.len() * 3),
                    a!(best, 128),
                    a!(oi, 130),
                    a!(of, 64),
                );
            }
            drop(dispatch);
            let read = context.detail.span("gpu_readback");
            let buffers = ::cubecl::future::block_on(client.read_async(vec![oi, of]))
                .map_err(|e| format!("GPU shortlist readback: {e:?}"))?;
            drop(read);
            let ids = u32::from_bytes(&buffers[0]);
            let values = f64::from_bytes(&buffers[1]);
            if ids[0] > 64 || ids[129] != 0 {
                return Err("invalid GPU scores or shortlist count".into());
            }
            let mut rows = Vec::new();
            let mut seen = FxHashSet::default();
            for j in 0..ids[0] as usize {
                let index = ids[1 + j * 2];
                let hard = ids[2 + j * 2];
                let base = values[j];
                if index >= c.ncandidate
                    || !seen.insert(index)
                    || !base.is_finite()
                    || hard > c.nprim * c.nprim
                {
                    return Err("invalid GPU winner".into());
                }
                rows.push(Row { index, hard, base });
            }
            let all = if verify {
                let f = client
                    .read_one(scores)
                    .map_err(|e| format!("GPU verify scores: {e:?}"))?;
                let i = client
                    .read_one(tags)
                    .map_err(|e| format!("GPU verify counts: {e:?}"))?;
                Some((
                    f64::from_bytes(&f)[..poses.len()].to_vec(),
                    u32::from_bytes(&i)[..poses.len() * 3].to_vec(),
                ))
            } else {
                None
            };
            Ok((rows, all))
        }).map_err(|e| e.to_string())?;
        drop(batch_span);
        self.batches += 1;
        self.candidates += poses.len();
        if let Some((scores, tags)) = all {
            let fixed: Vec<_> = current
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != moving)
                .map(|(_, p)| p.clone())
                .collect();
            let native = block_candidates(&current[moving], &fixed, context);
            assert_eq!(poses.len(), native.len(), "compact candidate count");
            let mut variant = current.to_vec();
            let mut checked_templates = FxHashSet::default();
            for (i, pose) in poses.iter().enumerate() {
                variant[moving] = materialize(&current[moving], *pose);
                assert_eq!(
                    primitive_pose_key(&variant[moving]),
                    primitive_pose_key(&native[i]),
                    "compact candidate order {i}"
                );
                if checked_templates.insert(pose.template_index) {
                    assert_eq!(
                        serde_json::to_value(&variant[moving].primitive).unwrap(),
                        serde_json::to_value(&native[i].primitive).unwrap(),
                        "compact geometry/metadata template {}",
                        pose.template_index
                    );
                }
                let cpu = evaluate(&variant, context);
                let tolerance = 1e-8 + cpu.score.abs() * 1e-12;
                assert_eq!(
                    cpu.hard_violations,
                    tags[i * 3] as usize,
                    "GPU hard candidate {i}"
                );
                if (cpu.score - scores[i]).abs() > tolerance {
                    if let Some(path) = std::env::var_os("PCB_BLOCK_GPU_FAILURE_CAPTURE") {
                        let geometry = gpu_runtime::with_session(GPU_REQUIREMENTS, |session| {
                            let client = &session.client;
                            macro_rules! upload {
                                ($v:expr,$t:ty) => {{
                                    let h = client.create_from_slice(<$t>::as_bytes($v));
                                    ArrayArg::from_raw_parts(h, $v.len())
                                }};
                            }
                            let output = client.empty(current.len() * 16 * 8);
                            unsafe {
                                let input = k::InputLaunch::new(
                                    upload!(&self.sf, f64),
                                    upload!(&self.si, i32),
                                    upload!(&ff, f64),
                                    upload!(&fi, u32),
                                    upload!(&pf, f64),
                                    upload!(&pi, u32),
                                );
                                k::inspect_geometry::launch_unchecked::<WgpuRuntime>(
                                    client,
                                    CubeCount::Static(1, 1, 1),
                                    CubeDim::new_1d(32),
                                    input,
                                    i as u32,
                                    ArrayArg::from_raw_parts(output.clone(), current.len() * 16),
                                );
                            }
                            let bytes = client
                                .read_one(output)
                                .map_err(|e| format!("diagnostic read: {e:?}"))?;
                            Ok(f64::from_bytes(&bytes).to_vec())
                        })
                        .unwrap();
                        let dump = serde_json::json!({"gpuGeometry":geometry,"candidate":i,"cpu":cpu.score,"gpu":scores[i],"phase":*context.trace_phase.borrow(),
                            "sf":self.sf,"si":self.si,"ff":ff,"fi":fi,"poses":pf,"ids":pi,
                            "cpuBoxes":variant.iter().map(|p|p.primitive.bbox).collect::<Vec<_>>()});
                        std::fs::write(path, serde_json::to_vec(&dump).unwrap())
                            .expect("write numerical failure capture");
                    }
                    panic!(
                        "GPU score candidate {i}: CPU={} GPU={} phase={}",
                        cpu.score,
                        scores[i],
                        context.trace_phase.borrow()
                    );
                }
            }
        }
        if !force_no_prune && std::env::var_os("PCB_BLOCK_GPU_VERIFY_PRUNE").is_some() {
            let unpruned = self.evaluate(current, moving, poses, diverse, origin, context, true)?;
            let keys = |v: &[Row]| {
                v.iter()
                    .map(|r| (r.index, r.hard, r.base.to_bits()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                keys(&rows),
                keys(&unpruned),
                "GPU shortlist differs with pruning on/off"
            );
        }
        Ok(rows)
    }
}
pub(super) fn fail(reason: String) -> ! {
    // Deliberate recovery transfer: do not emit the global panic hook.
    std::panic::resume_unwind(Box::new(GpuFailure(reason)))
}
#[derive(Debug)]
pub(super) struct GpuFailure(pub String);
pub(super) fn init(context: &Context) -> Result<Engine, String> {
    if !supported(context) {
        return Err("unsupported block features for complete GPU scoring".into());
    }
    gpu_runtime::with_session(GPU_REQUIREMENTS, |_| Ok(())).map_err(|e| e.to_string())?;
    Ok(Engine::new(context))
}
pub(super) fn shortlist(
    current: &[WorkingPrimitive],
    moving: usize,
    context: &Context,
    diverse: bool,
) -> Vec<(WorkingPrimitive, Evaluation, usize)> {
    let fixed: Vec<_> = current
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != moving)
        .map(|(_, p)| p.clone())
        .collect();
    let poses = {
        let _span = context.detail.span("gpu_compact_generate");
        candidates::generate(&current[moving], &fixed, context)
    };
    if poses.is_empty() {
        return vec![];
    }
    let origin = box_center(&union_boxes(
        &fixed.iter().map(|p| p.primitive.bbox).collect::<Vec<_>>(),
    ));
    let rows = context
        .gpu_engine
        .borrow_mut()
        .as_mut()
        .unwrap()
        .evaluate(current, moving, &poses, diverse, origin, context, false)
        .unwrap_or_else(|reason| fail(reason));
    let _span = context.detail.span("gpu_materialize");
    rows.into_iter()
        .map(|r| {
            (
                materialize(&current[moving], poses[r.index as usize]),
                Evaluation {
                    hard_violations: r.hard as usize,
                    score: r.base,
                },
                r.index as usize,
            )
        })
        .collect()
}
pub(super) fn ranked(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    previous: &IncrementalEvaluation,
    parent: f64,
    limit: usize,
    context: &Context,
) -> Vec<RankedCandidate> {
    let mut current = placed.to_vec();
    current.push(primitive.clone());
    let rows = shortlist(
        &current,
        placed.len(),
        context,
        context.problem.experiments.pad_owner_candidates,
    );
    let mut ranked: Vec<_> = rows
        .into_iter()
        .map(|(p, e, ordinal)| {
            let depths = extend_primitive_overlap_matrix(
                &previous.primitive_overlap_depths,
                placed,
                &p,
                context,
            );
            RankedCandidate {
                primitive: p,
                incremental: IncrementalEvaluation {
                    evaluation: e,
                    primitive_overlap_depths: depths,
                    size: placed.len() + 1,
                },
                hard_violations: e.hard_violations,
                score: e.score,
                route_penalty: parent,
                ordinal,
            }
        })
        .collect();
    ranked = crate::lazy_rank::top_k(
        ranked,
        limit,
        |a, ae, b, be| {
            let lower = |v: &RankedCandidate, exact: bool| {
                v.score
                    + if !exact && v.hard_violations == previous.evaluation.hard_violations {
                        parent
                    } else {
                        0.0
                    }
            };
            a.hard_violations
                .cmp(&b.hard_violations)
                .then_with(|| compare_f64(lower(a, ae), lower(b, be)))
                .then_with(|| a.ordinal.cmp(&b.ordinal))
        },
        |v| {
            if v.hard_violations == previous.evaluation.hard_violations {
                v.route_penalty = parent + block_micro_route_penalty(&v.primitive, placed, context);
                v.score += v.route_penalty;
            }
        },
    );
    ranked
}
pub(super) fn checkpoint(context: &Context, stage: &str) {
    if let Some(engine) = context.gpu_engine.borrow().as_ref() {
        if std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some() {
            eprintln!(
                "[block-gpu-stage] {}",
                serde_json::json!({"stage":stage,"backend":"cubecl","precision":"f64","batches":engine.batches,"candidates":engine.candidates,"frontierBatches":engine.frontier_batches,"frontierCandidates":engine.frontier_candidates,"frontierFrames":engine.frontiers.len(),"runtime":gpu_runtime::statistics()})
            );
        }
        if std::env::var("PCB_BLOCK_GPU_FAIL_AT").as_deref() == Ok(stage) {
            fail(format!("injected GPU failure after {stage}"));
        }
    }
}
