//! CubeCL block scorer; host layout and guards adapted from archived opencl.rs.
use super::gpu_kernels as k;
use super::*;
use ::cubecl::prelude::*;
use ::cubecl::server::Handle;
use crate::compute::f32_runtime::PcbRuntime as WgpuRuntime;
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
    l: f32,
    r: f32,
    t: f32,
    b: f32,
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
    pad_dx: f32,
    pad_dy: f32,
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
    components: u32,
    component_count: u32,
    boxes: u32,
    box_count: u32,
    locked: u32,
    ports: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Cp {
    x: f32,
    y: f32,
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
    x: f32, y: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rel {
    from: Endpoint,
    to: Endpoint,
    weight: f32,
    min: f32,
    max: f32,
    ox: f32,
    oy: f32,
    dx: f32,
    dy: f32,
    hard: u32,
    offset: u32,
    side: u32,
    unused: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Config {
    clearance: f32,
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
            p.allowed_orientations
                    .iter()
                    .any(|r| !matches!(normalize_rotation(*r), 0 | 90 | 180 | 270))
        })
        || context.gpu_sources.iter().any(|p|p.components.is_empty())
        || p.primitives.len() > 256
        || p.components.len() > 256
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
        if prim.placements.is_empty() { return false; }
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
    if let CompiledEndpoint::Anchor(Some(point))=e {return Endpoint {primitive:-2,x:point.x,y:point.y,..Default::default()};}
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
        unused: 0, x:0.0,y:0.0,
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
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
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
    dx: f32,
    dy: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Row {
    index: u32,
    hard: u32,
    base: f32,
}

pub(super) struct Engine {
    templates: Vec<Prim>,
    cp: Vec<Cp>,
    pads: Vec<Pad>,
    names: Vec<Arc<str>>,
    ids: FxHashMap<Arc<str>, i32>,
    sf: Vec<f32>,
    si: Vec<i32>,
    offsets: [u32; 6],
    geometry: Vec<f32>,
    ports: Vec<f32>, port_offset:u32,
    component_data: Vec<i32>,
    geometry_offset: u32,
    component_offset: u32,
    hull_capacity: usize,
    batch_limit: usize,
    max_allocation: usize,
    shared_limit: usize,
    options:[u32;11],
    net_capacity:usize, segment_capacity:usize, pad_capacity:usize,
    handles: Option<(Handle, Handle)>,
    pub batches: usize,
    pub candidates: usize,
    fail_batch: Option<usize>,
    frontiers: FxHashMap<Vec<PrimitivePoseKey>, frontier::Frame>,
    frontier_batches: usize,
    frontier_candidates: usize,
    _call:Option<gpu_runtime::CallPermit>,
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
            ports:vec![],port_offset:0,
            geometry: vec![], component_data: vec![], geometry_offset: 0, component_offset: 0,
            options:[0;11],net_capacity:2,segment_capacity:1,pad_capacity:1,
            batch_limit:4096,max_allocation:64*1024*1024,shared_limit:16384,
            hull_capacity: (context.gpu_sources.iter().map(|p| primitive_collision_boxes(p, context).len()).sum::<usize>().max(1) * 4).next_power_of_two(),
            handles: None,
            batches: 0,
            candidates: 0,
            frontiers: Default::default(),
            _call:None,
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
                let components = (engine.component_data.len() / 8) as u32;
                for (index, component) in p.components.iter() {
                    let body = (engine.geometry.len() / 4) as i32;
                    engine.geometry.extend([component.body_box.left, component.body_box.right, component.body_box.top, component.body_box.bottom]);
                    let through = (engine.geometry.len() / 4) as i32;
                    for b in component.through_hole_boxes.iter() { engine.geometry.extend([b.left,b.right,b.top,b.bottom]); }
                    let role = match component.role.as_deref() {Some("main_ic")=>1,Some("decoupling_cap")=>2,Some("passive")=>3,Some("connector")=>4,_=>0};
                    engine.component_data.extend([*index as i32,layer(Some(&component.layer)),component.pin_count as i32,role,component.power_component as i32,body,through,component.through_hole_boxes.len() as i32]);
                }
                let boxes = (engine.geometry.len() / 4) as u32;
                let collision = primitive_collision_boxes(&p, context);
                for b in collision { engine.geometry.extend([b.left,b.right,b.top,b.bottom]); }
                let ports=(engine.ports.len()/4) as u32;
                for q in p.primitive.path_ports.iter() {engine.ports.extend([q.x,q.y,q.normal.x,q.normal.y]);}
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
                    components, component_count: p.components.len() as u32, boxes, box_count: collision.len() as u32, locked: p.primitive.locked as u32, ports,
                });
            }
        }
        if engine.hull_capacity > 4096 { fail("GPU collision-box workspace limit exceeded".into()); }
        if engine.templates.iter().any(|p| p.np > 4096) {
            fail("GPU moving-pad workspace limit exceeded".into());
        }
        engine.net_capacity=context.net_endpoint_counts.iter().filter(|(n,_)| !n.is_empty() && !is_ground(n) && !context.problem.experiments.ignored_nets.iter().any(|s|s.eq_ignore_ascii_case(n))).map(|(_,n)|*n).max().unwrap_or(1).max(2).next_power_of_two();
        engine.segment_capacity=context.net_endpoint_counts.iter().filter(|(n,_)| !n.is_empty() && !is_ground(n) && !context.problem.experiments.ignored_nets.iter().any(|s|s.eq_ignore_ascii_case(n))).map(|(_,n)|n.saturating_sub(1)).sum::<usize>().max(1).next_power_of_two();
        engine.pad_capacity=engine.templates.iter().map(|p|p.np as usize).max().unwrap_or(1).max(1).next_power_of_two();
        if engine.net_capacity>256 {fail("GPU net exceeds 256 endpoints".into());}
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
        let units = if eligible && context.problem.experiments.pad_crossings {
            if is_power(name) {
                1
            } else {
                4
            }
        } else {
            0
        };
        if units != 0 && r.len() > 256 {
            fail("GPU net exceeds the 256-endpoint workspace".into());
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
                | ((r.iter().any(|r| r.primitive == moving as u32) as u32) << 1)
                | (((context.problem.experiments.long_nets && eligible && context.net_endpoint_counts.get(name)==Some(&2)) as u32) << 2)
                | ((!(is_ground(name) || is_power(name) || is_switching_power(name)) as u32) << 3)
                | (((signal_net_weight(name)>1.0) as u32) << 4),
        });
        if units > 0 {
            nsegment += r.len().saturating_sub(1) as u32;
        }
        refs.extend_from_slice(r);
    }
    if nsegment > 4096 || nets.len() > 4096 || engine.names.len() > 4096 {
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
                ))) as u32
                | (u32::from(rel.effect.as_ref() != "score_only" && (matches!(rel.kind.as_ref(), "critical_pair" | "island_target") || matches!(rel.relation.as_deref(), Some("very_near" | "cap_cluster")))) << 1) | (u32::from(rel.effect.as_ref()=="score_only")<<2),
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

fn prim_f(p: Prim) -> [f32; 10] {
    [
        p.bbox.l, p.bbox.r, p.bbox.t, p.bbox.b, p.body.l, p.body.r, p.body.t, p.body.b, p.pad_dx,
        p.pad_dy,
    ]
}
fn prim_i(p: Prim) -> [i32; 15] {
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
        p.components as i32, p.component_count as i32, p.boxes as i32, p.box_count as i32, p.locked as i32, p.ports as i32,
    ]
}
fn finite(values: &[f32]) -> bool {
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
        self.si.extend(context.problem.component_conflict.iter().map(|v| *v as i32));
        self.geometry_offset = self.sf.len() as u32;
        self.component_offset = self.si.len() as u32;
        self.sf.extend(&self.geometry);
        self.si.extend(&self.component_data);
        self.port_offset=self.sf.len() as u32;self.sf.extend(&self.ports);
        let p=&context.problem;
        self.options[0]=u32::from(matches!(p.collision_mode.as_ref(),"envelope"|"hybrid"))
            |(u32::from(p.hard_collision_mode.as_ref()=="primitive")<<1)|(u32::from(p.experiments.local_access)<<2)|(u32::from(p.experiments.stable_net_weight)<<3);
        self.options[1]=u32::MAX;
        if let Some(b)=p.bounds {self.options[1]=self.sf.len() as u32;self.sf.extend([b.left,b.right,b.top,b.bottom]);}
        self.options[2]=self.sf.len() as u32;self.sf.extend([p.target_width.unwrap_or(-1.0),p.target_height.unwrap_or(-1.0)]);
        self.options[3]=self.sf.len() as u32;self.options[4]=p.obstacles.len() as u32;
        for b in &p.obstacles {self.sf.extend([b.left,b.right,b.top,b.bottom]);}
        self.options[5]=u32::MAX;
        if let Some(w)=&p.world {
            self.options[5]=self.sf.len() as u32;self.sf.extend([w.bounds.left,w.bounds.right,w.bounds.top,w.bounds.bottom,w.edge_clearance]);
            self.options[6]=self.sf.len() as u32;self.options[7]=w.outline.len() as u32;
            for q in &w.outline {self.sf.extend([q.x,q.y]);}
            self.options[8]=self.si.len() as u32;
            for o in &w.obstacles {
                if let Some(index)=p.components.iter().position(|c|c.designator==o.designator) {
                    self.si.extend([index as i32,layer(o.layer.as_deref()),self.sf.len() as i32]);
                    self.sf.extend([o.box_.left,o.box_.right,o.box_.top,o.box_.bottom,o.clearance]);self.options[9]+=1;
                }
            }
        }
    }
    fn pack_topology(&self,current:&[WorkingPrimitive],context:&Context,ff:&mut Vec<f32>,fi:&mut Vec<u32>) {
        // Compile only membership/order on the host; candidate distances and penalties stay on GPU.
        let metadata=crate::signal_path::path_metadata(&context.problem.relations);
        let mut paths=std::collections::BTreeMap::<Arc<str>,std::collections::BTreeMap<i32,(Arc<str>,usize,usize)>>::new();
        for (i,p) in current.iter().enumerate() {for (j,port) in p.primitive.path_ports.iter().enumerate() {
            let entry=paths.entry(port.path_id.clone()).or_default().entry(port.order).or_insert((port.reference.clone(),i,j));
            if port.reference<entry.0 {*entry=(port.reference.clone(),i,j);}
        }}
        fi[k::PATH_I]=fi.len() as u32;fi[k::PATH_COUNT]=paths.len() as u32;
        let start=fi.len();fi.resize(start+paths.len()*3,0);
        for (i,(name,ports)) in paths.iter().enumerate() {
            let meta=metadata.get(name.as_ref());let f=ff.len() as u32;
            ff.extend([if meta.is_some_and(|m|m.straight){22.0}else{8.0},if meta.is_some_and(|m|m.straight){18.0}else{5.0},
                match meta.map(|m|m.priority){Some(3)=>1.8,Some(2)=>1.35,Some(0)=>0.65,_=>1.0},meta.map_or(1.0,|m|m.weight),if meta.is_some_and(|m|m.prefer_facing_pads){1.0}else{0.0}]);
            let refs=fi.len() as u32;
            for (&order,(_,p,j)) in ports {fi.extend([*p as u32,*j as u32,order as u32]);}
            fi[start+i*3..start+i*3+3].copy_from_slice(&[refs,ports.len() as u32,f]);
        }
        let markers:std::collections::BTreeMap<&str,f32>=context.problem.relations.iter().filter_map(|r|{
            let net=r.from.strip_prefix(crate::ordinary_net::MARKER_PREFIX)?;let weight=r.weight.unwrap_or(1.0);
            (r.from==r.to && weight.is_finite() && weight>0.0).then_some((net,weight))
        }).collect();
        let mut pairs=FxHashMap::<(usize,usize),(f32,Vec<[u32;4]>)>::default();
        for (net,weight) in markers {
            let members:Vec<_>=current.iter().enumerate().filter_map(|(i,p)|{
                let points:Vec<_>=p.primitive.connection_points.iter().enumerate().filter_map(|(j,c)|(c.net.as_deref()==Some(net)).then_some(j)).collect();
                (!points.is_empty()).then_some((i,points))
            }).collect();
            if !(2..=8).contains(&members.len()) {continue;}
            let contribution=weight/(members.len() as f32-1.0);
            for a in 0..members.len() {for b in a+1..members.len() {
                let pair=pairs.entry((members[a].0,members[b].0)).or_default();pair.0=(pair.0+contribution).min(4.0);
                for &ai in &members[a].1 {for &bi in &members[b].1 {pair.1.push([members[a].0 as u32,ai as u32,members[b].0 as u32,bi as u32]);}}
            }}
        }
        fi[k::ORDINARY_I]=fi.len() as u32;fi[k::ORDINARY_COUNT]=pairs.len() as u32;
        let start=fi.len();fi.resize(start+pairs.len()*3,0);
        for (i,(affinity,refs)) in pairs.into_values().enumerate() {
            let f=ff.len() as u32;ff.push(affinity);let r=fi.len() as u32;let count=refs.len() as u32;
            for pair in refs {fi.extend(pair);}
            fi[start+i*3..start+i*3+3].copy_from_slice(&[r,count,f]);
        }
    }
    fn evaluate(&mut self,current:&[WorkingPrimitive],moving:usize,poses:&[Pose],diverse:bool,origin:Point,context:&Context,force_no_prune:bool)->Result<Vec<Row>,String> {
        let fixed=(self.sf.len()+self.si.len())*4+self.pads.len()*16+self.hull_capacity*8;
        let batch_limit=gpu_runtime::with_session(GPU_REQUIREMENTS,|session|
            session.batch_capacity(fixed,36,self.batch_limit)).map_err(|e|e.to_string())?;
        if poses.len()<=batch_limit {return self.evaluate_batch(current,moving,poses,diverse,origin,context,force_no_prune);}
        let mut rows=Vec::new();
        for (chunk,part) in poses.chunks(batch_limit).enumerate() {
            let offset=(chunk*batch_limit) as u32;
            rows.extend(self.evaluate_batch(current,moving,part,diverse,origin,context,force_no_prune)?.into_iter().map(|mut r|{r.index+=offset;r}));
        }
        rows.sort_by(|a,b|a.hard.cmp(&b.hard).then_with(||compare_f32(a.base,b.base)).then_with(||a.index.cmp(&b.index)));
        if !diverse {rows.truncate(16);return Ok(rows);}
        let mut buckets=[0usize;16];let mut selected=FxHashSet::default();
        for row in &rows {
            let p=materialize(&current[moving],poses[row.index as usize]);let center=box_center(&p.primitive.bbox);
            let bucket=(poses[row.index as usize].template_index as usize%4)*4+usize::from(center.x>=origin.x)*2+usize::from(center.y>=origin.y);
            if buckets[bucket]<4 {buckets[bucket]+=1;selected.insert(row.index);}
        }
        for row in &rows {if selected.len()>=64 {break;}selected.insert(row.index);}
        rows.retain(|r|selected.contains(&r.index));Ok(rows)
    }
    fn evaluate_batch(
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
        // Size the private candidate hull from the invariant parent hull, using
        // exactly the rounded template coordinates consumed by frame_hull.
        let fixed_boxes:Vec<_>=prims.iter().enumerate().filter(|(i,_)|*i!=moving)
            .flat_map(|(_,p)|(p.boxes..p.boxes+p.box_count).map(|index| {
                let f=index as usize*4;
                Box2 {left:round_placement(self.geometry[f]+p.pad_dx),right:round_placement(self.geometry[f+1]+p.pad_dx),
                    top:round_placement(self.geometry[f+2]+p.pad_dy),bottom:round_placement(self.geometry[f+3]+p.pad_dy)}
            })).collect();
        let cheap_capacity=(convex_hull_vertices(&fixed_boxes).0.len()+prims[moving].box_count as usize*4).max(64).next_power_of_two();
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
        fi[k::GEOMETRY_F] = self.geometry_offset;
        fi[k::COMPONENT_I] = self.component_offset;
        fi[k::SOURCES] = context.problem.primitives.len() as u32;
        fi[k::PORT_F]=self.port_offset;
        fi[k::FLAGS..k::FLAGS+11].copy_from_slice(&self.options);
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
            ff.extend([r.weight, r.min, r.max, r.ox, r.oy, r.dx, r.dy,r.from.x,r.from.y,r.to.x,r.to.y]);
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
        self.pack_topology(current,context,&mut ff,&mut fi);
        // Archived proposals are relative to the current moving primitive.
        // GPU templates are resident in the original coordinate frame.
        let source = box_center(&context.problem.primitives[current[moving].source_index].bbox);
        let now = box_center(&current[moving].primitive.bbox);
        let shift = Point {
            x: round_placement(now.x - source.x),
            y: round_placement(now.y - source.y),
        };
        let pf: Vec<f32> = poses
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
        let pad_capacity=self.pad_capacity;
        let segment_capacity=self.segment_capacity;
        let net_capacity=self.net_capacity;
        let shared_bytes=segment_capacity*36+pad_capacity*16+128*4;
        if shared_bytes>self.shared_limit {return Err(format!("GPU shared-memory requirement {shared_bytes} exceeds {}",self.shared_limit));}
        let allocations=[self.sf.len()*4,self.si.len()*4,ff.len()*4,fi.len()*4,pf.len()*4,pi.len()*4,self.pads.len()*16,self.hull_capacity*8,poses.len()*12];
        if allocations.iter().any(|&n|n>self.max_allocation) || allocations.iter().sum::<usize>()>self.max_allocation {return Err("GPU frame memory budget exceeded".into());}
        let verify = std::env::var_os("PCB_BLOCK_GPU_VERIFY").is_some();
        let numerical_entities = context.problem.primitives.iter().map(|p|
            1 + p.connection_points.len() + p.path_ports.len() + p.collision_boxes.len()).sum::<usize>()
            + context.problem.components.len() + context.problem.relations.len();
        let pruning_operations = numerical_entities.saturating_pow(3).saturating_mul(128).min(u32::MAX as usize) as u32;
        let no_prune =
            force_no_prune || verify || std::env::var_os("PCB_BLOCK_GPU_NO_PRUNE").is_some();
        drop(preparing);
        let batch_span = context.detail.span("gpu_batch");
        let (rows, all) = gpu_runtime::with_batch(GPU_REQUIREMENTS, "block-score", poses.len().saturating_mul(context.problem.components.len().max(1)), |session| {
            if self.fail_batch == Some(self.batches + 1) {
                panic!("injected GPU runtime failure at batch {}", self.batches + 1);
            }
            let upload = context.detail.span("gpu_upload");
            if self.handles.is_none() {
                self.handles = Some((
                    session.client.create_from_slice(f32::as_bytes(&self.sf)),
                    session.client.create_from_slice(i32::as_bytes(&self.si)),
                ));
            }
            let (sf, si) = self.handles.as_ref().unwrap();
            let fh = session.client.create_from_slice(f32::as_bytes(&ff));
            let ih = session.client.create_from_slice(u32::as_bytes(&fi));
            let ph = session.client.create_from_slice(f32::as_bytes(&pf));
            let th = session.client.create_from_slice(u32::as_bytes(&pi));
            let hull = session.workspace(ScoreScratch::Hull.key(), self.hull_capacity * 2 * 4);
            let hc = session.workspace(ScoreScratch::HullCount.key(), 4);
            let pads = session.workspace(ScoreScratch::Pads.key(), self.pads.len() * 4 * 4);
            let segments = session.workspace(ScoreScratch::Segments.key(), c.nsegment as usize * 4 * 4);
            let st = session.workspace(ScoreScratch::SegmentTags.key(), c.nsegment as usize * 4 * 4);
            let costs = session.workspace(ScoreScratch::Costs.key(), c.nsegment as usize * 4);
            let scores = session.workspace(ScoreScratch::Scores.key(), poses.len() * 4);
            let tags = session.workspace(ScoreScratch::Tags.key(), poses.len() * 3 * 4);
            let best = session.workspace(ScoreScratch::Best.key(), 128 * 4);
            let mask = session.workspace(ScoreScratch::Mask.key(), poses.len() * 4);
            let oi = session.workspace(ScoreScratch::OutputIds.key(), 130 * 4);
            let of = session.workspace(ScoreScratch::OutputScores.key(), 64 * 4);
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
                    a!(hull, self.hull_capacity * 2),
                    a!(hc, 1),
                    self.hull_capacity,
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
                        net_capacity,
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
                    a!(hull, self.hull_capacity * 2),
                    a!(hc, 1),
                    a!(scores, poses.len()),
                    a!(tags, poses.len() * 3),
                    cheap_capacity,
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
                    pad_capacity, segment_capacity, net_capacity,
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
                        pruning_operations,
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
                        pad_capacity, segment_capacity, net_capacity,
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
            let buffers = crate::compute::cpu::waiting(||::cubecl::future::block_on(client.read_async(vec![oi, of])))
                .map_err(|e| format!("GPU shortlist readback: {e:?}"))?;
            drop(read);
            let ids = u32::from_bytes(&buffers[0]);
            let values = f32::from_bytes(&buffers[1]);
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
                    || hard as usize > current.len()*current.len()+context.problem.components.len()*(1+context.problem.world.as_ref().map_or(0,|w|w.obstacles.len()))+current.len()*(context.problem.obstacles.len()+1)
                {
                    return Err("invalid GPU winner".into());
                }
                rows.push(Row { index, hard, base });
            }
            let all = if verify {
                let f = crate::compute::cpu::waiting(||client.read_one(scores))
                    .map_err(|e| format!("GPU verify scores: {e:?}"))?;
                let i = crate::compute::cpu::waiting(||client.read_one(tags))
                    .map_err(|e| format!("GPU verify counts: {e:?}"))?;
                Some((
                    f32::from_bytes(&f)[..poses.len()].to_vec(),
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
            assert!(poses.iter().all(|p|(p.ordinal as usize)<native.len()), "compact candidate ordinal");
            let mut variant = current.to_vec();
            let mut checked_templates = FxHashSet::default();
            for (i, pose) in poses.iter().enumerate() {
                variant[moving] = materialize(&current[moving], *pose);
                assert_eq!(
                    primitive_pose_key(&variant[moving]),
                    primitive_pose_key(&native[pose.ordinal as usize]),
                    "compact candidate order {i}"
                );
                if checked_templates.insert(pose.template_index) {
                    assert_eq!(
                        serde_json::to_value(&variant[moving].primitive).unwrap(),
                        serde_json::to_value(&native[pose.ordinal as usize].primitive).unwrap(),
                        "compact geometry/metadata template {}",
                        pose.template_index
                    );
                }
                let cpu = evaluate(&variant, context);
                let tolerance = crate::f32_policy::diagnostic_tolerance(cpu.score, scores[i]);
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
                            let output = client.empty(current.len() * 16 * 4);
                            unsafe {
                                let input = k::InputLaunch::new(
                                    upload!(&self.sf, f32),
                                    upload!(&self.si, i32),
                                    upload!(&ff, f32),
                                    upload!(&fi, u32),
                                    upload!(&pf, f32),
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
                            let bytes = crate::compute::cpu::waiting(||client.read_one(output))
                                .map_err(|e| format!("diagnostic read: {e:?}"))?;
                            Ok(f32::from_bytes(&bytes).to_vec())
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
    let policy=gpu_runtime::Admission::explicit(std::env::var("PCB_BLOCK_BACKEND").as_deref()==Ok("cubecl"));
    let call=gpu_runtime::enter(GPU_REQUIREMENTS,policy).map_err(|e|e.to_string())?;
    let mut engine=Engine::new(context);engine._call=Some(call);
    gpu_runtime::with_session(GPU_REQUIREMENTS,|session| {
        let props=session.client.properties();
        engine.max_allocation=session.allocation_limit();
        engine.shared_limit=props.hardware.max_shared_memory_size;
        let static_bytes=(engine.sf.len()+engine.si.len())*4;
        // Per candidate: pose + IDs + score + tags + mask = 36 bytes.
        engine.batch_limit=4096.min(props.hardware.max_cube_count.0 as usize).max(1);
        session.batch_capacity(static_bytes,36,engine.batch_limit)?;
        if let Ok(limit)=std::env::var("PCB_BLOCK_GPU_BATCH_SIZE").unwrap_or_default().parse::<usize>() {engine.batch_limit=engine.batch_limit.min(limit.max(1));}
        if std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some() {eprintln!("[block-gpu-memory] {}",serde_json::json!({"components":context.problem.components.len(),"primitives":context.problem.primitives.len(),"residentBytes":static_bytes,"batchLimit":engine.batch_limit,"allocationBudgetBytes":engine.max_allocation,"sharedLimitBytes":engine.shared_limit,"hullPoints":engine.hull_capacity,"netEndpoints":engine.net_capacity,"segments":engine.segment_capacity,"movingPads":engine.pad_capacity,"sharedBytes":engine.segment_capacity*36+engine.pad_capacity*16+512}));}
        Ok(())
    }).map_err(|e|e.to_string())?;
    Ok(engine)
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
    parent: f32,
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
                .then_with(|| compare_f32(lower(a, ae), lower(b, be)))
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
                serde_json::json!({"stage":stage,"backend":"cubecl","precision":"f32","batches":engine.batches,"candidates":engine.candidates,"frontierBatches":engine.frontier_batches,"frontierCandidates":engine.frontier_candidates,"frontierFrames":engine.frontiers.len(),"runtime":gpu_runtime::statistics()})
            );
        }
        if std::env::var("PCB_BLOCK_GPU_FAIL_AT").as_deref() == Ok(stage) {
            fail(format!("injected GPU failure after {stage}"));
        }
    }
}
