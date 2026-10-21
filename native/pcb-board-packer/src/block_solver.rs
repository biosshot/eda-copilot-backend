use crate::geometry::{
    box_center, normalize_rotation, overlap_depth, rotate_box, rotate_point, round_placement,
    translate_box, union_boxes, Box2, Point,
};
use crate::net_class::{is_ground, is_power, is_switching_power};
use crate::model::{
    BlockComponentGeometry, BlockSolveProblem, BoardPackSolution, Placement, Primitive,
    PrimitiveState, Rank, Relation,
};
use crate::signal_path;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::sync::Arc;
use rustc_hash::{FxHashMap,FxHashSet}; 

#[derive(Clone)]
struct WorkingPrimitive {
    id: u32,
    source_index: usize,
    primitive: Primitive,
    components: Arc<Vec<(usize, BlockComponentGeometry)>>,
    source_components: Arc<Vec<(usize, BlockComponentGeometry)>>,
    source_placements: Arc<Vec<Placement>>,
    source_node_ids: Arc<FxHashSet<Arc<str>>>,
    point_net_ids: Arc<Vec<Option<u32>>>,
    rotation: i32,
}

#[derive(Clone)]
struct SearchState {
    placed: Vec<WorkingPrimitive>,
    remaining: Vec<WorkingPrimitive>,
    incremental: IncrementalEvaluation,
    hard_violations: usize,
    score: f32,
    route_penalty: f32,
    ordinal: usize,
}

#[derive(Clone)]
struct RankedCandidate {
    primitive: WorkingPrimitive,
    incremental: IncrementalEvaluation,
    hard_violations: usize,
    score: f32,
    route_penalty: f32,
    ordinal: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PrimitivePoseKey {
    primitive_id: u32,
    rotation: i32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SearchStateKey {
    remaining: Vec<u32>,
    placed: Vec<PrimitivePoseKey>,
}

#[derive(Clone, Copy)]
struct Evaluation {
    hard_violations: usize,
    score: f32,
}

#[derive(Clone)]
struct IncrementalEvaluation {
    evaluation: Evaluation,
    primitive_overlap_depths: Vec<f32>,
    size: usize,
}

#[path = "block_solver_trace.rs"]
mod trace;

#[cfg(feature = "gpu")]
use crate::compute::gpu as gpu_runtime;
#[cfg(feature = "gpu")]
const GPU_REQUIREMENTS: crate::compute::Requirements = crate::compute::Requirements { f32: true, u64: false };
#[cfg(feature = "gpu")]
#[path = "block_solver/gpu_kernels.rs"]
mod gpu_kernels;
#[cfg(feature = "gpu")]
#[path = "block_solver/cubecl.rs"]
mod cubecl;

#[derive(Default)]
struct DetailProfile {
    enabled: bool,
    totals: RefCell<std::collections::BTreeMap<&'static str, (u64, u64)>>,
}
struct ProfileSpan<'a> { profile: &'a DetailProfile, name: &'static str, started: Option<std::time::Instant> }
impl DetailProfile {
    fn span(&self, name: &'static str) -> ProfileSpan<'_> {
        ProfileSpan { profile: self, name, started: self.enabled.then(std::time::Instant::now) }
    }
    fn count(&self, name: &'static str, amount: usize) {
        if self.enabled { self.totals.borrow_mut().entry(name).or_default().0 += amount as u64; }
    }
}
impl Drop for ProfileSpan<'_> {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let mut totals = self.profile.totals.borrow_mut();
            let entry = totals.entry(self.name).or_default();
            entry.0 += 1; entry.1 += started.elapsed().as_nanos() as u64;
        }
    }
}
struct Context {
    conditions: crate::conditions::Plan,
    #[cfg(feature = "gpu")]
    gpu_sources: Vec<WorkingPrimitive>,
    #[cfg(feature = "gpu")]
    gpu_engine: RefCell<Option<cubecl::Engine>>,
    corridor_cache: RefCell<FxHashMap<(Arc<str>, usize, usize, usize), ([u32; 8], f32)>>,
    escape_cache: RefCell<FxHashMap<(usize, usize), EscapeEntry>>,
    detail: DetailProfile,
    source_pads: Vec<Vec<crate::model::RouteObstacle>>,
    pad_nets: RefCell<Vec<crate::model::PostPlaceNet>>,
    pad_net_indices: Vec<Vec<Option<usize>>>,
    pad_point_metadata: Vec<Vec<(Option<Arc<str>>, Option<Arc<str>>)>>,
    pad_geometry: RefCell<Vec<Option<(PrimitivePoseKey, Arc<Vec<crate::model::RouteObstacle>>)>>> ,
    net_endpoint_counts: FxHashMap<Arc<str>, usize>,
    trace: bool,
    trace_phase: RefCell<&'static str>,
    problem: BlockSolveProblem,
    relations: Vec<CompiledRelation>,
    net_ground: Vec<bool>,
    net_weights: Vec<f32>,
    net_signal: Vec<bool>,
    evaluation_cache: RefCell<FxHashMap<Vec<PrimitivePoseKey>, Evaluation>>,
    net_scoring_scratch: RefCell<NetScoringScratch>,
    validate_incremental_scoring: bool,
    /// Worker threads for the pair neighbourhood; 1 keeps it on the calling thread.
    pair_threads: usize,
}

struct EscapeEntry {
    point: [u32; 2],
    source: Option<u32>,
    layer: Option<Arc<str>>,
    pose: PrimitivePoseKey,
    contributions: Vec<[f32; 4]>,
}

struct NetScoringScratch {
    accumulators: Vec<NetAccumulator>,
    signal_order: Vec<usize>,
    ground_order: Vec<usize>,
}

#[derive(Clone, Copy)]
struct NetAccumulator {
    seen: bool,
    point_count: usize,
    primitive_count: usize,
    last_primitive: u32,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl Default for NetAccumulator {
    fn default() -> Self {
        Self {
            seen: false,
            point_count: 0,
            primitive_count: 0,
            last_primitive: u32::MAX,
            min_x: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            min_y: f32::INFINITY,
            max_y: f32::NEG_INFINITY,
        }
    }
}

#[derive(Clone, Copy)]
struct EndpointPoint {
    point: Point,
    primitive_id: Option<u32>,
}

#[derive(Clone)]
struct CompiledRelation {
    relation: Relation,
    from: CompiledEndpoint,
    to: CompiledEndpoint,
}

#[derive(Clone)]
enum CompiledEndpoint {
    Anchor(Option<Point>),
    Pad {
        primitive_id: u32,
        point_index: usize,
    },
    Component {
        primitive_id: u32,
        placement_index: usize,
    },
    Primitive {
        primitive_id: u32,
    },
    Missing,
}

pub fn solve_block(problem: BlockSolveProblem) -> Result<crate::model::BlockSolveSolution, String> {
    solve_block_with_threads(problem, 1)
}

/// `pair_threads` only parallelizes the pair neighbourhood; the result does not depend on it.
pub fn solve_block_with_threads(problem: BlockSolveProblem, pair_threads: usize) -> Result<crate::model::BlockSolveSolution, String> {
    solve_block_with_gpu_choice(problem, pair_threads, true)
}

/// `gpu_allowed=false` keeps an `auto` call on the CPU (explicit `cubecl` is unaffected).
pub(crate) fn solve_block_with_gpu_choice(mut problem: BlockSolveProblem, pair_threads: usize, gpu_allowed: bool) -> Result<crate::model::BlockSolveSolution, String> {
    #[cfg(not(feature = "gpu"))]
    let _ = gpu_allowed;
    attach_group_relation_points(&mut problem)?;
    #[cfg(feature = "gpu")]
    {
        let requested=std::env::var("PCB_BLOCK_BACKEND").unwrap_or_else(|_|"auto".into());
        // Relax workload floors by approximately 8x, with a minimum of one primitive.
        // Device/input guards and complete CPU recovery remain in force.
        let substantial=problem.primitives.len()>=1 || problem.primitives.iter().map(|p|p.connection_points.len()).sum::<usize>()>=30;
        let use_gpu=if requested=="cubecl" {problem.primitives.len()>=1}
            else {gpu_allowed && requested=="auto" && problem.primitives.len()>=1 && (substantial || gpu_runtime::ready())
                && (!gpu_runtime::auto_targets_metal() || metal_auto_profitable(&problem))};
        if use_gpu {
            let original=problem.clone();let started=std::time::Instant::now();
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||solve_block_inner(problem,true,pair_threads))) {
                Ok(result)=>return result,
                Err(payload)=>{
                    let expected=payload.downcast_ref::<cubecl::GpuFailure>();
                    let reason=expected.map(|e|e.0.clone()).or_else(||payload.downcast_ref::<String>().cloned())
                        .or_else(||payload.downcast_ref::<&str>().map(|s|(*s).into()))
                        .unwrap_or_else(||"GPU block attempt panicked".into());
                    let validating=["PCB_BLOCK_GPU_VERIFY","PCB_BLOCK_GPU_VERIFY_PRUNE","PCB_BLOCK_GPU_VERIFY_FRONTIER"].iter().any(|key|std::env::var_os(key).is_some());
                    if expected.is_none() && validating {return Err(reason);}
                    let failed_ms=started.elapsed().as_secs_f64()*1000.0;
                    let result=solve_block_inner(original,false,pair_threads);
                    if requested=="cubecl" || std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some() {
                        eprintln!("[block-gpu-fallback] {}",serde_json::json!({"requested":requested,"actual":"cpu","precision":"f32","reason":reason,"gpuAttemptMs":failed_ms,"totalMs":started.elapsed().as_secs_f64()*1000.0}));
                    }
                    return result;
                }
            }
        }
    }
    solve_block_inner(problem,false,pair_threads)
}
/// Apple GPUs beat the CPU threads only in the initial (beam/singles) stage of
/// blocks with many netted connection points; the pair stage and small blocks
/// lose (isolated replays of 298 block calls from six fixtures on an M5 Max).
/// Netted points exclude the group endpoints attached before this decision.
#[cfg(feature = "gpu")]
fn metal_auto_profitable(problem: &BlockSolveProblem) -> bool {
    problem.defer_pairs && problem.primitives.iter()
        .map(|p|p.connection_points.iter().filter(|c|c.net.is_some()).count()).sum::<usize>()>=METAL_AUTO_MIN_NETTED_POINTS
}
#[cfg(feature = "gpu")]
const METAL_AUTO_MIN_NETTED_POINTS: usize = 80;

/// On Metal, a batch hands the GPU only to its largest eligible hypothesis;
/// the rest run on the CPU threads meanwhile. Otherwise hypotheses race for the
/// single device and the loser of the initialization falls back to the CPU, so
/// which block got the GPU (and the wall time) changed from run to run.
#[cfg(feature = "gpu")]
pub(crate) fn metal_batch_gpu_choice(problems: &[&BlockSolveProblem]) -> Vec<bool> {
    if !gpu_runtime::auto_targets_metal() || std::env::var("PCB_BLOCK_BACKEND").as_deref()==Ok("cubecl") {
        return vec![true; problems.len()];
    }
    let netted=|p:&BlockSolveProblem| p.primitives.iter().map(|q|q.connection_points.iter().filter(|c|c.net.is_some()).count()).sum::<usize>();
    let chosen=problems.iter().enumerate().filter(|(_,p)|metal_auto_profitable(p))
        // Hypotheses of one block share their nets; more movable primitives
        // means more search work (ICM: 10 primitives 32 s vs 7 primitives 19 s).
        .max_by_key(|(i,p)|(netted(p),p.primitives.iter().filter(|q|!q.locked).count(),std::cmp::Reverse(*i))).map(|(i,_)|i);
    (0..problems.len()).map(|i|Some(i)==chosen).collect()
}

pub(crate) fn attach_group_relation_points(problem: &mut BlockSolveProblem) -> Result<(), String> {
    crate::group_endpoints::attach(&mut problem.primitives, &problem.relations, &Default::default(),
        &problem.components.iter().map(|c| (&c.designator, &c.primitive_id, &c.block_name, c.body_box)).collect::<Vec<_>>())?;
    crate::conditions::attach(&mut problem.primitives, &problem.relations,
        &problem.components.iter().map(|c| (&c.designator,&c.primitive_id,&c.block_name,&c.layer,c.hint_box.unwrap_or(c.body_box))).collect::<Vec<_>>());
    Ok(())
}

fn prepare_context(problem: BlockSolveProblem, _gpu_enabled: bool) -> (Vec<WorkingPrimitive>, Context) {
    let primitive_ids = lexical_ids(
        problem
            .primitives
            .iter()
            .map(|primitive| primitive.id.clone()),
    );
    let mut components_by_primitive: FxHashMap<Arc<str>, Vec<(usize, BlockComponentGeometry)>> =
        FxHashMap::default();
    for (index, component) in problem.components.iter().cloned().enumerate() {
        components_by_primitive
            .entry(component.primitive_id.clone())
            .or_default()
            .push((index, component));
    }
    let mut net_ids = FxHashMap::<Arc<str>, u32>::default();
    let mut net_ground = Vec::new();
    for primitive in &problem.primitives {
        for point in primitive.connection_points.iter() {
            let Some(net) = &point.net else { continue };
            if net_ids.contains_key(net) {
                continue;
            }
            let id = net_ids.len() as u32;
            net_ground.push(is_ground(net));
            net_ids.insert(net.clone(), id);
        }
    }
    let primitives: Vec<WorkingPrimitive> = problem
        .primitives
        .iter()
        .cloned()
        .enumerate()
        .map(|(source_index, primitive)| {
            let source_node_ids = Arc::new(primitive.source_node_ids.iter().cloned().collect());
            let point_net_ids = Arc::new(
                primitive
                    .connection_points
                    .iter()
                    .map(|point| point.net.as_ref().and_then(|net| net_ids.get(net).copied()))
                    .collect(),
            );
            let source_components = Arc::new(
                components_by_primitive
                    .remove(&primitive.id)
                    .unwrap_or_default(),
            );
            // Pad access rows are rounded to the placement grid whenever a component
            // moves (rebuild_component_geometry) and on GPU (geometry_box). Round the
            // working rows on load too, as a move to the authored pose would, so a
            // component that never moves scores the same on CPU and GPU. Moves still
            // rebuild from the authored rows: rounding a half tick before a rotation
            // can land on the other tick.
            let components = Arc::new(source_components.iter().cloned().map(|(index, mut component)| {
                for row in &mut component.access_rows { row.box_ = translate_box(&row.box_, 0.0, 0.0); }
                (index, component)
            }).collect());
            WorkingPrimitive {
                id: primitive_ids[&primitive.id],
                source_index,
                components,
                source_components,
                source_placements: primitive.placements.clone(),
                primitive,
                source_node_ids,
                point_net_ids,
                rotation: 0,
            }
        })
        .collect();
    let relations = compile_relations(&problem.relations, &primitives, problem.bounds.as_ref(), problem.experiments.local_access);
    let source_pads: Vec<Vec<_>> = problem.primitives.iter().map(|p| problem.routing_obstacles.iter()
        .filter(|o| o.primitive_id.as_ref() == Some(&p.id)).cloned().collect()).collect();
    let mut net_endpoint_counts = FxHashMap::default();
    for p in &problem.primitives { for cp in p.connection_points.iter() {
        if let Some(n) = &cp.net { *net_endpoint_counts.entry(n.clone()).or_insert(0usize) += 1; }
    }}
    let pad_nets: Vec<_> = problem.primitives.iter().flat_map(|p| p.connection_points.iter())
        .filter_map(|cp| cp.net.clone()).filter(|n| !n.is_empty() && !is_ground(n)
            && !problem.experiments.ignored_nets.iter().any(|ignored| ignored.eq_ignore_ascii_case(n)))
        .collect::<std::collections::BTreeSet<_>>().into_iter().map(|name| crate::model::PostPlaceNet {
            weight: if is_power(&name) { 0.25 } else { 1.0 }, name, points: vec![], layers: vec![], internal_owners: vec![], local_scopes: vec![],
        }).collect();
    let pad_net_indices = problem.primitives.iter().map(|p| p.connection_points.iter().map(|cp|
        cp.net.as_ref().and_then(|name| pad_nets.binary_search_by(|n| n.name.cmp(name)).ok())).collect()).collect();
    let pad_point_metadata = problem.primitives.iter().map(|p| p.connection_points.iter().map(|cp| {
        let layer = source_pads.iter().flatten().find(|pad| pad.reference.as_ref() == Some(&cp.reference)).and_then(|pad| pad.layer.clone());
        let owner = cp.reference.rsplit_once('.').map(|(d, _)| d);
        let core = problem.components.iter().find(|c| c.primitive_id == p.id && Some(c.designator.as_ref()) == owner
            && c.role.as_deref() == Some("main_ic")).map(|c| c.designator.clone());
        (layer, core)
    }).collect()).collect();
    let mut net_signal = vec![false; net_ids.len()];
    let mut net_weights = vec![1.0; net_ids.len()];
    for (net, id) in &net_ids { net_weights[*id as usize] = if is_power(net) { 0.25 } else { 1.0 }; }
    for (net, id) in &net_ids { net_signal[*id as usize] = !(is_ground(net) || is_power(net) || is_switching_power(net)); }
    let context = Context {
        conditions: crate::conditions::plan(&problem.primitives,&problem.relations,&Default::default(),
            &problem.components.iter().map(|c|(&c.designator,&c.primitive_id,&c.block_name,&c.layer,c.hint_box.unwrap_or(c.body_box))).collect::<Vec<_>>(),
            problem.world.as_ref().map(|w|w.bounds),problem.world.as_ref().map_or(0.0,|w|w.edge_clearance)),
        #[cfg(feature = "gpu")]
        gpu_sources: if _gpu_enabled {primitives.clone()} else {vec![]},
        #[cfg(feature = "gpu")]
        gpu_engine: Default::default(),
        corridor_cache: Default::default(), escape_cache: Default::default(),
        net_signal, net_weights,
        pad_nets: RefCell::new(pad_nets), pad_net_indices, pad_point_metadata,
        pad_geometry: RefCell::new(vec![None; source_pads.len()]), source_pads,
        net_endpoint_counts,
        detail: DetailProfile { enabled: std::env::var_os("PCB_BLOCK_SOLVER_DETAIL").is_some(), ..Default::default() },
        trace: trace::enabled(&problem),
        trace_phase: RefCell::new("search"),
        problem,
        relations,
        net_ground,
        evaluation_cache: RefCell::new(FxHashMap::default()),
        net_scoring_scratch: RefCell::new(NetScoringScratch {
            accumulators: vec![NetAccumulator::default(); net_ids.len()],
            signal_order: Vec::with_capacity(net_ids.len()),
            ground_order: Vec::with_capacity(net_ids.len()),
        }),
        validate_incremental_scoring: std::env::var_os("PCB_NATIVE_VALIDATE_INCREMENTAL_SCORING")
            .is_some_and(|value| value == "1"),
        pair_threads: 1,
    };
    (primitives, context)
}

fn solve_block_inner(problem: BlockSolveProblem, _gpu_enabled:bool, pair_threads: usize) -> Result<crate::model::BlockSolveSolution, String> {
    let (primitives, mut context) = prepare_context(problem, _gpu_enabled);
    context.pair_threads = pair_threads.max(1);
    #[cfg(feature = "gpu")]
    if _gpu_enabled {
        let engine=cubecl::init(&context).unwrap_or_else(|reason|cubecl::fail(reason));
        *context.gpu_engine.borrow_mut()=Some(engine);
    }
    let profile = std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some();
    let started = std::time::Instant::now();
    let resumed = context.problem.pair_seed.is_some();
    let solved = if let Some(seed) = &context.problem.pair_seed {
        seed.iter().map(|state| {
            let mut restored = primitives.iter().find(|p| p.primitive.id == state.primitive.id).expect("validated pair seed").clone();
            restored.primitive = state.primitive.clone();
            restored.rotation = state.rotation;
            rebuild_component_geometry(&mut restored);
            restored
        }).collect()
    } else if context.problem.search_width > 1 {
        solve_beam(primitives, &context)
    } else {
        solve_greedy(primitives, &context)
    };
    let beam_ms = started.elapsed().as_secs_f64() * 1000.0;
    #[cfg(feature = "gpu")]
    cubecl::checkpoint(&context,"beam");
    trace::stage(&context, "beam_complete", &solved);
    *context.trace_phase.borrow_mut() = "local_improve";
    let improved = if resumed { solved.clone() } else { local_improve(solved.clone(), &context) };
    let singles_ms = started.elapsed().as_secs_f64() * 1000.0 - beam_ms;
    #[cfg(feature = "gpu")]
    cubecl::checkpoint(&context,"singles");
    trace::stage(&context, "local_complete", &improved);
    let singles = improved.clone();
    // Preserve the search coordinate frame. Centering a display checkpoint can
    // change grid-rounded candidate generation when refinement resumes later.
    let pair_seed = if context.problem.defer_pairs { Some(singles.iter().map(|p| crate::model::BlockPairSeed { primitive: p.primitive.clone(), rotation: p.rotation }).collect()) } else { None };
    *context.trace_phase.borrow_mut() = "pair_improve";
    let improved = if !context.problem.defer_pairs && (context.problem.experiments.pair_swaps || context.problem.experiments.reinsert_pair) {
        pair_improve(improved, &context)
    } else { improved };
    #[cfg(feature = "gpu")]
    cubecl::checkpoint(&context,"pairs");
    if profile {
        eprintln!("[pcb-block-solver] components={} beam_ms={:.1} singles_ms={:.1} pairs_ms={:.1}",
            context.problem.components.len(), beam_ms, singles_ms,
            started.elapsed().as_secs_f64() * 1000.0 - beam_ms - singles_ms);
    }
    trace::stage(&context, "pair_complete", &improved);
    let has_global_frame =
        context.problem.bounds.is_some() || !context.problem.obstacles.is_empty() || context.problem.world.is_some();
    let final_primitives =
        if improved.iter().any(|primitive| primitive.primitive.locked) || has_global_frame {
            improved
        } else {
            center_primitives(improved, context.problem.grid)
        };
    trace::stage(&context, "native_final", &final_primitives);
    let mut checkpoints = Vec::new();
    for (stage, items) in [("beam", solved), ("singles", singles), ("pairs", final_primitives.clone())] {
        if resumed && stage != "pairs" { continue; }
        let centered = if items.iter().any(|p| p.primitive.locked) || has_global_frame { items }
            else { center_primitives(items, context.problem.grid) };
        checkpoints.push(crate::model::BlockCheckpoint { stage, result: solution(&context, &centered)? });
    }
    if context.detail.enabled {
        eprintln!("[block-detail] {}", serde_json::json!({"components":context.problem.components.len(),
            "pairsOnly":resumed,"totals":*context.detail.totals.borrow()}));
    }
    if profile {
        #[cfg(feature="gpu")]
        let counts=context.gpu_engine.borrow().as_ref().map(|e|e.counters());
        #[cfg(not(feature="gpu"))]
        let counts:Option<(usize,usize)>=None;
        eprintln!("[block-backend] {}",serde_json::json!({
            "block":context.problem.primitives.iter().map(|p|p.id.as_ref()).collect::<Vec<_>>(),
            "backend":if counts.is_some(){"cubecl"}else{"cpu"},"precision":"f32","pairsOnly":resumed,
            "beamMs":beam_ms,"singlesMs":singles_ms,"totalMs":started.elapsed().as_secs_f64()*1000.0,
            "gpuBatches":counts.map_or(0,|c|c.0),"gpuCandidates":counts.map_or(0,|c|c.1)}));
    }
    Ok(crate::model::BlockSolveSolution { result: solution(&context, &final_primitives)?, checkpoints, pair_seed })
}

fn solve_greedy(primitives: Vec<WorkingPrimitive>, context: &Context) -> Vec<WorkingPrimitive> {
    let mut placed: Vec<_> = primitives
        .iter()
        .filter(|primitive| primitive.primitive.locked)
        .cloned()
        .collect();
    let mut remaining: Vec<_> = primitives
        .into_iter()
        .filter(|primitive| !primitive.primitive.locked)
        .collect();
    remaining.sort_by(|a, b| compare_f32(seed_rank(b, context), seed_rank(a, context)));
    if placed.is_empty() && !remaining.is_empty() {
        let first = frontier_indices(&remaining, &placed, context)[0];
        let seed = remaining.remove(first);
        let candidates = block_candidates(&seed, &[], context);
        let candidate = best_candidate(&[], candidates, context)
            .unwrap_or_else(|| center_primitive(seed, context.problem.grid));
        placed.push(candidate);
    }
    let mut incremental = incremental_evaluation(&placed, context);
    let mut route_penalty = 0.0;
    while !remaining.is_empty() {
        let mut best_index = 0usize;
        let mut best: Option<WorkingPrimitive> = None;
        let mut best_hard = usize::MAX;
        let mut best_score = f32::INFINITY;
        let mut best_incremental = None;
        let mut best_route_penalty = route_penalty;
        for index in frontier_indices(&remaining, &placed, context) {
            let primitive = &remaining[index];
            for candidate in ranked_block_candidates(primitive, &placed, &incremental, route_penalty, 1, context) {
                let hard = candidate.hard_violations;
                let score = candidate.score;
                if hard < best_hard
                    || (hard == best_hard && compare_f32(score, best_score) == Ordering::Less)
                {
                    best_index = index;
                    best = Some(candidate.primitive);
                    best_hard = hard;
                    best_score = score;
                    best_route_penalty = candidate.route_penalty;
                    best_incremental = Some(candidate.incremental);
                }
            }
        }
        let next = remaining.remove(best_index);
        if let Some(best) = best {
            placed.push(best);
            incremental = best_incremental.expect("best candidate must have an evaluation");
            route_penalty = best_route_penalty;
        } else {
            placed.push(center_primitive(next, context.problem.grid));
            incremental = incremental_evaluation(&placed, context);
        }
    }
    placed
}

fn solve_beam(primitives: Vec<WorkingPrimitive>, context: &Context) -> Vec<WorkingPrimitive> {
    let search_width = context.problem.search_width.max(2);
    let locked: Vec<_> = primitives
        .iter()
        .filter(|primitive| primitive.primitive.locked)
        .cloned()
        .collect();
    let mut remaining: Vec<_> = primitives
        .into_iter()
        .filter(|primitive| !primitive.primitive.locked)
        .collect();
    remaining.sort_by(|a, b| compare_f32(seed_rank(b, context), seed_rank(a, context)));
    let mut ordinal = 0usize;
    let initial_incremental = incremental_evaluation(&locked, context);
    let initial_evaluation = initial_incremental.evaluation;
    let mut states = vec![SearchState {
        hard_violations: initial_evaluation.hard_violations,
        score: initial_evaluation.score,
        route_penalty: 0.0,
        placed: locked,
        remaining,
        incremental: initial_incremental,
        ordinal,
    }];
    while states.iter().any(|state| !state.remaining.is_empty()) {
        let mut expanded = Vec::new();
        for state in states {
            if state.remaining.is_empty() {
                expanded.push(state);
                continue;
            }
            let per_primitive_limit = 8usize.max(div_ceil(search_width, state.remaining.len()));
            for index in frontier_indices(&state.remaining, &state.placed, context) {
                let primitive = &state.remaining[index];
                let mut ranked = ranked_block_candidates(
                    primitive,
                    &state.placed,
                    &state.incremental,
                    state.route_penalty,
                    per_primitive_limit.min(16),
                    context,
                );
                ranked.truncate(per_primitive_limit.min(16));
                for candidate in ranked {
                    ordinal += 1;
                    let mut placed = state.placed.clone();
                    placed.push(candidate.primitive);
                    let mut remaining = state.remaining.clone();
                    remaining.remove(index);
                    expanded.push(SearchState {
                        placed,
                        remaining,
                        incremental: candidate.incremental,
                        hard_violations: candidate.hard_violations,
                        score: candidate.score,
                        route_penalty: candidate.route_penalty,
                        ordinal,
                    });
                }
            }
        }
        states = dedupe_states(expanded);
        states.sort_by(compare_states);
        if context.problem.experiments.order_branching && context.problem.experiments.frontier_order {
            // Keep up to three different placed subsets, with the normal position
            // beam within each subset. Partial costs of unlike subsets are biased.
            let mut subsets: Vec<(Vec<u32>, usize)> = Vec::new();
            states.retain(|state| {
                let mut key: Vec<_> = state.remaining.iter().map(|p| p.id).collect();
                key.sort();
                if let Some((_, count)) = subsets.iter_mut().find(|(k, _)| *k == key) {
                    *count += 1;
                    *count <= search_width
                } else if subsets.len() < 3 {
                    subsets.push((key, 1)); true
                } else { false }
            });
        } else {
            states.truncate(search_width);
        }
        if context.trace {
            trace::emit(&context, "beam_kept", serde_json::json!({"states": states.iter().map(|s| serde_json::json!({
                "poses": trace::poses(&s.placed), "score":s.score, "hard":s.hard_violations,
                "route":s.route_penalty, "ordinal":s.ordinal
            })).collect::<Vec<_>>()}));
        }
        if states.is_empty() {
            break;
        }
    }
    states.sort_by(compare_states);
    states
        .iter()
        .find(|state| state.remaining.is_empty())
        .or_else(|| states.first())
        .map(|state| state.placed.clone())
        .unwrap_or_default()
}

fn local_improve(mut current: Vec<WorkingPrimitive>, context: &Context) -> Vec<WorkingPrimitive> {
    let current_evaluation = evaluate(&current, context);
    let mut current_hard = current_evaluation.hard_violations;
    let mut current_score = current_evaluation.score;
    let max_passes = if context.problem.experiments.extra_passes {
        8
    } else if current_hard > 0 {
        2usize.max(current.len())
    } else {
        2
    };
    for _ in 0..max_passes {
        let mut changed = false;
        for index in 0..current.len() {
            if current[index].primitive.locked {
                continue;
            }
            let fixed: Vec<_> = current
                .iter()
                .enumerate()
                .filter(|(item, _)| *item != index)
                .map(|(_, value)| value.clone())
                .collect();
            let mut best = current[index].clone();
            let mut best_hard = current_hard;
            let mut best_base_score = current_score;
            let mut best_effective_score = current_score
                + block_route_penalty(&current[index], &fixed, context);
            #[cfg(feature = "gpu")]
            let gpu_ranked=if context.gpu_engine.borrow().is_some() {Some(cubecl::shortlist(&current,index,context,false))}else{None};
            #[cfg(not(feature = "gpu"))]
            let gpu_ranked:Option<Vec<(WorkingPrimitive,Evaluation,usize)>>=None;
            let mut ranked: Vec<_> = gpu_ranked.unwrap_or_else(||block_candidates(&current[index], &fixed, context)
                .into_iter()
                .enumerate()
                .map(|(ordinal, candidate)| {
                    let mut variant = current.clone();
                    variant[index] = candidate.clone();
                    (candidate, evaluate(&variant, context), ordinal)
                })
                .collect());
            ranked.sort_by(|a, b| {
                a.1.hard_violations
                    .cmp(&b.1.hard_violations)
                    .then_with(|| compare_f32(a.1.score, b.1.score))
                    .then_with(|| a.2.cmp(&b.2))
            });
            if context.trace && trace::is_target(&current[index]) {
                trace::emit(context, "local_candidates", serde_json::json!({"placed":trace::poses(&fixed),
                    "current":trace::row(&current[index], &fixed, current_score, current_hard, 0, best_effective_score, best_effective_score-current_score),
                    "candidates":ranked.iter().map(|(p,e,o)| trace::row(p,&fixed,e.score,e.hard_violations,*o,e.score,0.0)).collect::<Vec<_>>()}));
            }
            ranked.truncate(16);
            for (candidate, evaluation, _) in ranked {
                // Route corrections are nonnegative. This candidate cannot win
                // even with a free route; skip its FastRoute evaluation.
                if evaluation.hard_violations > best_hard
                    || (evaluation.hard_violations == best_hard
                        && evaluation.score + 0.001 >= best_effective_score) {
                    continue;
                }
                let effective_score = evaluation.score
                    + block_route_penalty(&candidate, &fixed, context);
                if evaluation.hard_violations < best_hard
                    || (evaluation.hard_violations == best_hard
                        && effective_score + 0.001 < best_effective_score)
                {
                    best = candidate;
                    best_hard = evaluation.hard_violations;
                    best_base_score = evaluation.score;
                    best_effective_score = effective_score;
                }
            }
            if context.trace && trace::is_target(&current[index]) {
                let mut after = current.clone(); after[index] = best.clone();
                trace::emit(context, "local_chosen", serde_json::json!({"placed":trace::poses(&fixed),
                    "beforeParts":trace::parts(&current,context), "afterParts":trace::parts(&after,context),
                    "candidate":trace::row(&best,&fixed,best_base_score,best_hard,0,best_effective_score,best_effective_score-best_base_score)}));
            }
            if primitive_pose_key(&best) != primitive_pose_key(&current[index]) {
                current[index] = best;
                current_hard = best_hard;
                current_score = best_base_score;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    current
}

/// Monotonically improving bound for membership in a score shortlist.
struct ScoreWindow { limit: usize, values: Vec<(usize, f32)> }
impl ScoreWindow {
    fn new(limit: usize) -> Self { Self { limit, values: Vec::with_capacity(limit + 1) } }
    fn ceiling(&self, hard: usize) -> f32 {
        if self.values.len() < self.limit { return f32::INFINITY; }
        let &(h, score) = self.values.last().unwrap();
        match hard.cmp(&h) { Ordering::Less => f32::INFINITY, Ordering::Greater => f32::NEG_INFINITY, Ordering::Equal => score }
    }
    fn push(&mut self, hard: usize, score: f32) {
        let index = self.values.partition_point(|&(h, s)| h < hard || (h == hard && s <= score));
        if index < self.limit { self.values.insert(index, (hard, score)); self.values.truncate(self.limit); }
    }
}

fn ranked_block_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    previous: &IncrementalEvaluation,
    parent_route_penalty: f32,
    limit: usize,
    context: &Context,
) -> Vec<RankedCandidate> {
    #[cfg(feature = "gpu")]
    if context.gpu_engine.borrow().is_some() {return cubecl::ranked(primitive,placed,previous,parent_route_penalty,limit,context);}
    let mut ranked = Vec::new();
    let diverse = context.problem.experiments.pad_owner_candidates;
    let origin = box_center(&union_boxes(&placed.iter().map(|p| p.primitive.bbox).collect::<Vec<_>>()));
    let mut global = ScoreWindow::new(if diverse { 64 } else { 16 });
    let mut windows: FxHashMap<(i32, bool, bool), ScoreWindow> = FxHashMap::default();
    let mut variant = Vec::with_capacity(placed.len() + 1);
    variant.extend_from_slice(placed);
    let power_frame = PowerYieldFrame::new(placed, context);
    let long_net_frame = context.problem.experiments.long_nets.then(|| LongNetFrame::new(placed, context));
    for (candidate_ordinal, candidate) in block_candidates(primitive, placed, context)
        .into_iter()
        .enumerate()
    {
        variant.push(candidate);
        let appended = variant.last().expect("appended candidate");
        let hard = previous.evaluation.hard_violations + candidate_hard_violation_count(appended, placed, context);
        let center = box_center(&appended.primitive.bbox);
        let bucket = (appended.rotation, center.x >= origin.x, center.y >= origin.y);
        let window = windows.entry(bucket).or_insert_with(|| ScoreWindow::new(4));
        let ceiling = if context.trace { f32::INFINITY } else if diverse {
            global.ceiling(hard).max(window.ceiling(hard))
        } else { global.ceiling(hard) };
        let incremental = append_incremental_evaluation(placed, &variant, previous, hard, ceiling,
            power_frame.as_ref(), long_net_frame.as_ref(), context);
        let candidate = variant.pop().expect("appended candidate");
        let Some(incremental) = incremental else { continue; };
        let evaluation = incremental.evaluation;
        global.push(hard, evaluation.score); window.push(hard, evaluation.score);
        ranked.push(RankedCandidate {
            hard_violations: evaluation.hard_violations,
            score: evaluation.score,
            route_penalty: parent_route_penalty,
            primitive: candidate,
            incremental,
            ordinal: candidate_ordinal,
        });
    }
    ranked.sort_by(compare_candidates);
    trace::candidates(context, "generated", placed, &ranked);
    // Retain spatial/orientation diversity before the expensive route score.
    if context.problem.experiments.pad_owner_candidates {
        let origin = box_center(&union_boxes(&placed.iter().map(|p| p.primitive.bbox).collect::<Vec<_>>()));
        let mut buckets = FxHashMap::default();
        let mut selected = Vec::new();
        let mut rest = Vec::new();
        for candidate in ranked {
            let center = box_center(&candidate.primitive.primitive.bbox);
            let key = (candidate.primitive.rotation, center.x >= origin.x, center.y >= origin.y);
            let count = buckets.entry(key).or_insert(0usize);
            if *count < 4 { *count += 1; selected.push(candidate); } else { rest.push(candidate); }
        }
        let space = 64usize.saturating_sub(selected.len());
        selected.extend(rest.into_iter().take(space));
        selected.sort_by(compare_candidates);
        ranked = selected;
    } else { ranked.truncate(16); }
    trace::candidates(context, "shortlist", placed, &ranked);
    let result = crate::lazy_rank::top_k(ranked, limit, |a, a_exact, b, b_exact| {
        let lower_score = |candidate: &RankedCandidate, _exact: bool| candidate.score;
        a.hard_violations.cmp(&b.hard_violations)
            .then_with(|| compare_f32(lower_score(a, a_exact), lower_score(b, b_exact)))
            .then_with(|| a.ordinal.cmp(&b.ordinal))
    }, |candidate| {
        {
            let correction = block_route_penalty(&candidate.primitive, placed, context);
            candidate.route_penalty = correction;
            candidate.score += candidate.route_penalty;
        }
    });
    trace::candidates(context, "ranked_returned", placed, &result);
    result
}

fn block_route_penalty(candidate: &WorkingPrimitive, placed: &[WorkingPrimitive], context: &Context) -> f32 {
    block_layout_route(&placed.iter().chain(std::iter::once(candidate)).collect::<Vec<_>>(), context)
}
fn block_layout_route(all: &[&WorkingPrimitive], context: &Context) -> f32 {
    if context.problem.experiments.routing_metric == crate::model::BlockRoutingMetric::Off { return 0.0; }
    let mut obstacles = transformed_pad_obstacles(all, context);
    obstacles.extend(context.problem.obstacles.iter().map(|b| crate::model::RouteObstacle { box_:*b,layer:None,reference:None,net:None,primitive_id:None }));
    let plan = crate::fast_route::layout_plan(&all.iter().map(|p| &p.primitive).collect::<Vec<_>>(), &obstacles,
        &context.problem.external_nets, &context.problem.experiments.ignored_nets, context.problem.clearance,
        union_boxes(&all.iter().map(|p|p.primitive.bbox).collect::<Vec<_>>()), &context.problem.relations);
    #[cfg(feature="gpu")]
    if context.gpu_engine.borrow().is_some() { return plan.gpu().unwrap_or_else(|e|cubecl::fail(e.to_string())); }
    plan.cpu()
}

fn prepared_pad_obstacles(item: &WorkingPrimitive, context: &Context) -> Arc<Vec<crate::model::RouteObstacle>> {
    let key = primitive_pose_key(item);
    let mut cache = context.pad_geometry.borrow_mut();
    if let Some((old, pads)) = &cache[item.source_index] { if *old == key { return pads.clone(); } }
    let source = &context.problem.primitives[item.source_index];
    let origin = box_center(&source.bbox);
    let center = box_center(&item.primitive.bbox);
    let pads = Arc::new(context.source_pads[item.source_index].iter().map(|pad| {
        let mut pad = pad.clone();
        pad.box_ = translate_box(&rotate_box(&pad.box_, &origin, item.rotation), center.x-origin.x, center.y-origin.y);
        pad
    }).collect());
    cache[item.source_index] = Some((key, Arc::clone(&pads)));
    pads
}
fn transformed_pad_obstacles(primitives: &[&WorkingPrimitive], context: &Context) -> Vec<crate::model::RouteObstacle> {
    primitives.iter().flat_map(|p| prepared_pad_obstacles(p, context).as_ref().clone()).collect()
}
fn best_candidate(
    placed: &[WorkingPrimitive],
    candidates: Vec<WorkingPrimitive>,
    context: &Context,
) -> Option<WorkingPrimitive> {
    let mut best = None;
    let mut best_hard = usize::MAX;
    let mut best_score = f32::INFINITY;
    for candidate in candidates {
        let mut variant = placed.to_vec();
        variant.push(candidate.clone());
        let evaluation = evaluate(&variant, context);
        let hard = evaluation.hard_violations;
        let score = evaluation.score + block_layout_route(&variant.iter().collect::<Vec<_>>(), context);
        if hard < best_hard
            || (hard == best_hard && compare_f32(score, best_score) == Ordering::Less)
        {
            best = Some(candidate);
            best_hard = hard;
            best_score = score;
        }
    }
    best
}

fn solution(
    context: &Context,
    primitives: &[WorkingPrimitive],
) -> Result<BoardPackSolution, String> {
    let states = primitives
        .iter()
        .map(|primitive| {
            let source = &context.problem.primitives[primitive.source_index];
            let source_origin = box_center(&source.bbox);
            let rotated_first = source.placements.first().map(|placement| {
                rotate_point(
                    &Point {
                        x: placement.x,
                        y: placement.y,
                    },
                    &source_origin,
                    primitive.rotation,
                )
            });
            let final_first = primitive
                .primitive
                .placements
                .first()
                .map(|placement| Point {
                    x: placement.x,
                    y: placement.y,
                });
            let (translation_x, translation_y) = match (rotated_first, final_first) {
                (Some(source), Some(final_)) => (
                    round_placement(final_.x - source.x),
                    round_placement(final_.y - source.y),
                ),
                _ => {
                    let rotated = rotate_box(&source.bbox, &source_origin, primitive.rotation);
                    (
                        round_placement(primitive.primitive.bbox.left - rotated.left),
                        round_placement(primitive.primitive.bbox.top - rotated.top),
                    )
                }
            };
            PrimitiveState {
                primitive_id: primitive.primitive.id.clone(),
                rotation: primitive.rotation,
                translation_x,
                translation_y,
                placements: primitive.primitive.placements.as_ref().clone(),
            }
        })
        .collect();
    let mut evaluation = evaluate(primitives, context);
    evaluation.score += block_layout_route(&primitives.iter().collect::<Vec<_>>(), context);
    if !evaluation.score.is_finite() {
        return Err("block F32 score overflow or invalid arithmetic".into());
    }
    Ok(BoardPackSolution {
        version: context.problem.version,
        states,
        rank: Rank {
            hard_count: evaluation.hard_violations,
            hard_severity: 0.0,
            score: evaluation.score,
        },
    })
}

// Candidate generation and scoring are kept below so the hot solver loop only
// operates on compact numeric working primitives.

fn block_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<WorkingPrimitive> {
    let result = block_candidates_inner(primitive, placed, context);
    context.detail.count("candidate_positions_unique", result.len());
    result
}


fn block_candidates_inner(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<WorkingPrimitive> {
    let _span = context.detail.span("block_candidates");
    if primitive.primitive.locked {
        return vec![primitive.clone()];
    }
    let variants = orientation_variants(primitive);
    context.detail.count("candidate_orientations", variants.len());
    if placed.is_empty() {
        let primary: Vec<_> = variants
            .iter()
            .cloned()
            .map(|variant| fit_to_bounds(center_primitive(variant, context.problem.grid), context))
            .collect();
        if context.problem.bounds.is_none()
            || primary
                .iter()
                .any(|candidate| candidate_hard_violation_count(candidate, &[], context) == 0)
        {
            return dedupe_primitives(primary);
        }
        let mut all = primary.clone();
        all.extend(board_fallback_candidates(&variants, &[], &primary, context));
        return dedupe_primitives(all);
    }
    let mut primary = Vec::new();
    for variant in &variants {
        primary.extend(block_candidates_for_orientation(variant, placed, context));
    }
    let primary = dedupe_primitives(primary);
    if context.problem.bounds.is_none()
        || primary
            .iter()
            .any(|candidate| candidate_hard_violation_count(candidate, placed, context) == 0)
    {
        return primary;
    }
    let mut all = primary.clone();
    all.extend(board_fallback_candidates(
        &variants, placed, &primary, context,
    ));
    dedupe_primitives(all)
}

fn block_candidates_for_orientation(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<WorkingPrimitive> {
    let mut anchors: Vec<(Box2, f32)> = placed
        .iter()
        .flat_map(|item| primitive_candidate_boxes(item, context).into_iter()
            .map(move |b| (b, candidate_clearance(primitive, item, context))))
        .collect();
    let union_clearance = anchors.iter().map(|(_, c)| *c).fold(0.0, f32::max);
    anchors.push((union_boxes(
        &placed
            .iter()
            .map(|item| item.primitive.bbox)
            .collect::<Vec<_>>(),
    ), union_clearance));
    let mut candidates = Vec::new();
    for (anchor, clearance) in anchors {
        let centers = adjacent_centers(
            primitive.primitive.width,
            primitive.primitive.height,
            &anchor,
            clearance,
        );
        for center in centers {
            candidates.push(fit_to_bounds(
                move_primitive_center_exact(primitive.clone(), center),
                context,
            ));
            candidates.push(fit_to_bounds(
                move_primitive_center(primitive.clone(), center, context.problem.grid),
                context,
            ));
        }
        if context.problem.candidate_box_mode.as_ref() == "collision" {
            for moving_box in primitive_candidate_boxes(primitive, context) {
                candidates.extend(box_anchored_candidates(
                    primitive,
                    &moving_box,
                    &anchor,
                    clearance,
                    context,
                ));
            }
        }
    }
    context.detail.count("candidate_body_raw", candidates.len());
    let relations = relation_anchored_candidates(primitive, placed, context);
    context.detail.count("candidate_relation_raw", relations.len());
    candidates.extend(relations);
    if context.problem.experiments.net_candidates {
        let nets = net_anchored_candidates(primitive, placed, context);
        context.detail.count("candidate_net_raw", nets.len());
        candidates.extend(nets);
    }
    let before_bridges = candidates.len();
    let placed_primitives: Vec<_> = placed.iter().map(|item| &item.primitive).collect();
    for delta in signal_path::bridge_deltas(&primitive.primitive, &placed_primitives) {
        candidates.push(fit_to_bounds(
            translate_primitive(primitive, delta.x, delta.y),
            context,
        ));
        candidates.push(fit_to_bounds(
            translate_primitive(
                primitive,
                snap(delta.x, context.problem.grid),
                snap(delta.y, context.problem.grid),
            ),
            context,
        ));
    }
    context.detail.count("candidate_bridge_raw", candidates.len() - before_bridges);
    context.detail.count("candidate_total_raw", candidates.len());
    let unique = dedupe_primitives(candidates);
    context.detail.count("candidate_per_orientation_unique", unique.len());
    unique
}

// Use the same pair matrix as the hard validator. Compounds conservatively use
// their largest conflicting clearance; the legacy path remains replayable.
fn candidate_clearance(a: &WorkingPrimitive, b: &WorkingPrimitive, context: &Context) -> f32 {
    if !context.problem.experiments.candidate_clearance
        || context.problem.hard_collision_mode.as_ref() != "components"
        || a.components.is_empty() || b.components.is_empty() {
        return context.problem.clearance;
    }
    let count = context.problem.components.len();
    let mut clearance: f32 = 0.0;
    for (i, _) in a.components.iter() {
        for (j, _) in b.components.iter() {
            if context.problem.component_conflict[i * count + j] != 0 {
                clearance = clearance.max(context.problem.component_pair_clearance[i * count + j]);
            }
        }
    }
    clearance + 0.001 // Survive placement rounding at the strict collision boundary.
}

fn net_anchored_candidates(
    primitive: &WorkingPrimitive, placed: &[WorkingPrimitive], context: &Context,
) -> Vec<WorkingPrimitive> {
    let mut candidates = Vec::new();
    // Fixed ordering and bounded anchors per pad keep high-fanout supply nets cheap.
    for moving in primitive.primitive.connection_points.iter() {
        let Some(net) = &moving.net else { continue };
        if net.is_empty() || is_ground(net) || context.problem.experiments.ignored_nets.iter().any(|n| n.eq_ignore_ascii_case(net)) { continue; }
        let mut anchors = Vec::new();
        for item in placed {
            for target in item.primitive.connection_points.iter() {
                if target.net.as_ref() == Some(net) {
                    anchors.push((item, target));
                }
            }
        }
        anchors.sort_by(|(a, ap), (b, bp)| a.id.cmp(&b.id).then(ap.reference.cmp(&bp.reference)));
        let anchor_limit = if context.problem.experiments.pad_owner_candidates { usize::MAX } else { 4 };
        for (item, target) in anchors.into_iter().take(anchor_limit) {
            context.detail.count("candidate_net_pad_pairs", 1);
            let center = box_center(&primitive.primitive.bbox);
            let dx = moving.x - center.x;
            let dy = moving.y - center.y;
            // A compound's empty envelope must not hide space next to its IC.
            // Hard legality still checks every actual component in the island.
            let owner = target.reference.split_once('.').map(|(name, _)| name);
            let b = if context.problem.experiments.pad_owner_candidates
                && context.problem.hard_collision_mode.as_ref() == "components" {
                item.components.iter().find(|(_, c)| Some(c.designator.as_ref()) == owner)
                    .map(|(_, c)| c.body_box).unwrap_or(item.primitive.bbox)
            } else { item.primitive.bbox };
            let clearance = candidate_clearance(primitive, item, context);
            let slide = context.problem.grid.max(0.25);
            let expanded = context.problem.experiments.candidate_rings;
            let slides = if expanded { vec![0.0, -slide, slide, -2.0 * slide, 2.0 * slide] }
                else { vec![0.0, -slide, slide] };
            for ring in 0..(if expanded { 3 } else { 1 }) {
            let c = clearance + ring as f32 * slide;
            for &s in &slides {
                for p in [
                    Point { x: b.left - c - primitive.primitive.width / 2.0, y: target.y - dy + s },
                    Point { x: b.right + c + primitive.primitive.width / 2.0, y: target.y - dy + s },
                    Point { x: target.x - dx + s, y: b.top - c - primitive.primitive.height / 2.0 },
                    Point { x: target.x - dx + s, y: b.bottom + c + primitive.primitive.height / 2.0 },
                ] {
                    candidates.push(fit_to_bounds(move_primitive_center_exact(primitive.clone(), p), context));
                }
            }
            }
        }
    }
    candidates
}

fn net_has_passive_ic_link(net: &str, context: &Context) -> bool {
    let mut core = false;
    let mut passive = false;
    for (p, primitive) in context.problem.primitives.iter().enumerate() {
        for (i, cp) in primitive.connection_points.iter().enumerate() {
            if cp.net.as_deref() != Some(net) { continue; }
            if context.pad_point_metadata[p][i].1.is_some() { core = true; } else { passive = true; }
        }
    }
    core && passive
}

fn connected_pin_links(primitives: &[WorkingPrimitive], context: &Context,
    mut visit: impl FnMut(&str, &str, f32, f32)) {
    for p in primitives {
        for (i, cp) in p.primitive.connection_points.iter().enumerate() {
            let Some(net_index) = context.pad_net_indices[p.source_index][i] else { continue };
            if context.pad_point_metadata[p.source_index][i].1.is_some() { continue; }
            let weight = context.pad_nets.borrow()[net_index].weight;
            for core in primitives.iter().flat_map(|p| p.components.iter().map(|(_, c)| c))
                .filter(|c| c.role.as_deref() == Some("main_ic")) {
                let mut nearest = f32::INFINITY;
                for q in primitives {
                    for (j, target) in q.primitive.connection_points.iter().enumerate() {
                        if context.pad_net_indices[q.source_index][j] == Some(net_index)
                            && context.pad_point_metadata[q.source_index][j].1.as_ref() == Some(&core.designator) {
                            nearest = nearest.min(distance(Point { x: cp.x, y: cp.y }, Point { x: target.x, y: target.y }));
                        }
                    }
                }
                if nearest.is_finite() { visit(&cp.reference, &core.designator, weight, nearest); }
            }
        }
    }
}

fn connected_pin_stretch(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let mut score = 0.0;
    connected_pin_links(primitives, context, |_, _, weight, d| {
        score += crate::local_stretch::penalty(d, weight);
    });
    score
}

/// Evaluate a supplied layout with the search evaluator, without generating or
/// moving candidates. Diagnostics are measurements, never extra score terms.
pub fn quality(mut problem: BlockSolveProblem) -> Result<serde_json::Value, String> {
    attach_group_relation_points(&mut problem)?;
    let (primitives, context) = prepare_context(problem, false);
    let score = score_block(&primitives, &context) + block_layout_route(&primitives.iter().collect::<Vec<_>>(), &context);
    if !score.is_finite() { return Err("block quality score is not finite".into()); }
    let bbox = union_boxes(&primitives.iter().map(|p| p.primitive.bbox).collect::<Vec<_>>());
    let width = bbox.right - bbox.left;
    let height = bbox.bottom - bbox.top;
    let mut links = std::collections::BTreeMap::new();
    connected_pin_links(&primitives, &context, |reference, core, _, d| {
        links.insert(format!("{reference}->{core}"), d);
    });
    let mut ports = std::collections::BTreeMap::new();
    let mut wire = 0.0;
    for net in context.pad_nets.borrow().iter() {
        let mut measured = net.clone();
        measured.points = primitives.iter().flat_map(|p| p.primitive.connection_points.iter())
            .filter(|cp| cp.net.as_ref() == Some(&net.name)).map(|cp| Point { x: cp.x, y: cp.y }).collect();
        wire += crate::post_place::net_length(&measured) * net.weight;
    }
    for cp in primitives.iter().flat_map(|p| p.primitive.connection_points.iter()) {
        if cp.net.as_ref().is_some_and(|n| context.problem.external_nets.contains(n) && !is_ground(n)) {
            ports.insert(cp.reference.clone(), Point { x: cp.x - (bbox.left + bbox.right) / 2.0,
                y: cp.y - (bbox.top + bbox.bottom) / 2.0 });
        }
    }
    let electrical = score_block_terms(&primitives, &context, None, f32::INFINITY, 0.0, None, None, false)
        .ok_or("unbounded block electrical score was pruned")?;
    Ok(serde_json::json!({"score":score,"electrical":electrical,
        "localStretch": if context.problem.experiments.long_nets { connected_pin_stretch(&primitives,&context) } else {0.0},
        "wire":wire,"maxLocal":links.values().copied().fold(0.0,f32::max),
        "area":width*height,"width":width,"height":height,
        "exposure":external_port_exposure_penalty(&primitives,&context),"links":links,"ports":ports}))
}

struct LongNetFrame {
    baseline: f32,
    open: FxHashMap<Arc<str>, (u32, Point, f32)>,
}

impl LongNetFrame {
    fn new(placed: &[WorkingPrimitive], context: &Context) -> Self {
        let _span = context.detail.span("long_net_frame_build");
        let baseline = long_local_net_penalty(placed, context);
        let mut open = FxHashMap::default();
        for p in placed {
            for cp in p.primitive.connection_points.iter() {
                let Some(net) = &cp.net else { continue };
                if net.is_empty() || is_ground(net) || net_has_passive_ic_link(net, context) || context.net_endpoint_counts.get(net) != Some(&2)
                    || context.problem.experiments.ignored_nets.iter().any(|n| n.eq_ignore_ascii_case(net)) { continue; }
                open.insert(net.clone(), (p.id, Point { x: cp.x, y: cp.y },
                    p.primitive.width.max(p.primitive.height)));
            }
        }
        Self { baseline, open }
    }

    fn with_candidate(&self, candidate: &WorkingPrimitive, context: &Context) -> f32 {
        let _span = context.detail.span("long_net_frame_append");
        let mut result = self.baseline;
        let size = candidate.primitive.width.max(candidate.primitive.height);
        for cp in candidate.primitive.connection_points.iter() {
            let Some(net) = &cp.net else { continue };
            if net.is_empty() || is_ground(net) || net_has_passive_ic_link(net, context) || context.net_endpoint_counts.get(net) != Some(&2)
                || context.problem.experiments.ignored_nets.iter().any(|n| n.eq_ignore_ascii_case(net)) { continue; }
            let Some(&(id, point, old_size)) = self.open.get(net) else { continue };
            if id == candidate.id { continue; }
            let scale = ((old_size + size) * 0.5 + context.problem.clearance).max(1.0);
            let excess = (distance(point, Point { x: cp.x, y: cp.y }) - scale).max(0.0);
            result += excess * excess / scale * 12.0 * if is_power(net) { 0.25 } else { 1.0 };
        }
        result
    }
}

fn long_local_net_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("long_local_net_penalty");
    let mut nets: FxHashMap<Arc<str>, Vec<(u32, Point)>> = FxHashMap::default();
    // Count endpoints in the complete problem, never reclassify a partial bus as a pair.
    let counts = &context.net_endpoint_counts;
    for p in primitives {
        for cp in p.primitive.connection_points.iter() {
            let Some(n) = &cp.net else { continue };
            if n.is_empty() || is_ground(n) || net_has_passive_ic_link(n, context) || counts.get(n) != Some(&2)
                || context.problem.experiments.ignored_nets.iter().any(|ignored| ignored.eq_ignore_ascii_case(n)) { continue; }
            nets.entry(n.clone()).or_default().push((p.id, Point { x: cp.x, y: cp.y }));
        }
    }
    nets.iter().filter(|(_, v)| v.len() == 2 && v[0].0 != v[1].0)
        .map(|(net, v)| {
            let a = primitives.iter().find(|p| p.id == v[0].0).unwrap();
            let b = primitives.iter().find(|p| p.id == v[1].0).unwrap();
            let scale = ((a.primitive.width.max(a.primitive.height) + b.primitive.width.max(b.primitive.height)) * 0.5
                + context.problem.clearance).max(1.0);
            let excess = (distance(v[0].1, v[1].1) - scale).max(0.0);
            excess * excess / scale * 12.0 * if is_power(net) { 0.25 } else { 1.0 }
        }).sum()
}

fn pair_improve(mut current: Vec<WorkingPrimitive>, context: &Context) -> Vec<WorkingPrimitive> {
    // Bounded block-only neighbourhood. Fixed primitives and board postrefine are untouched.
    if current.len() > 12 { return current; }
    let pairs: Vec<(usize, usize)> = (0..current.len())
        .filter(|&i| !current[i].primitive.locked)
        .flat_map(|i| (i + 1..current.len()).map(move |j| (i, j)))
        .filter(|&(_, j)| !current[j].primitive.locked)
        .collect();
    let threads = context.pair_threads.min(pairs.len());
    #[cfg(feature = "gpu")]
    let gpu_active = context.gpu_engine.borrow().is_some();
    #[cfg(not(feature = "gpu"))]
    let gpu_active = false;
    if threads > 1 && !context.trace && !gpu_active {
        return pair_improve_speculative(current, &pairs, threads, context);
    }
    for &(i, j) in &pairs {
        if let Some(next) = improve_pair(&current, i, j, context) { current = next; }
    }
    current
}

/// Same result as the serial pair loop. A batch of consecutive pairs is solved
/// against one state on worker contexts; the first accepted pair in input order
/// wins, and every later pair of that batch is solved again from the new state.
/// The contexts' caches are memoization only, so a fresh context per worker
/// yields the same evaluations as the caller's context. Each job works on an
/// unshared copy of the block, which changes no value.
fn pair_improve_speculative(
    mut current: Vec<WorkingPrimitive>,
    pairs: &[(usize, usize)],
    threads: usize,
    context: &Context,
) -> Vec<WorkingPrimitive> {
    use std::sync::{mpsc, Arc, Mutex};
    type Job = (usize, usize, usize, Arc<Vec<WorkingPrimitive>>);
    type Outcome = (usize, Result<Option<Vec<WorkingPrimitive>>, ()>);
    let problem = &context.problem;
    let (job_tx, job_rx) = mpsc::channel::<Option<Job>>();
    let job_rx = Mutex::new(job_rx);
    let (outcome_tx, outcome_rx) = mpsc::channel::<Outcome>();
    let mut next = 0;
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let outcome_tx = outcome_tx.clone();
            let job_rx = &job_rx;
            scope.spawn(move || {
                let _float_env = crate::float_env::Guard::enter();
                let (_, worker) = prepare_context(problem.clone(), false);
                loop {
                    let job = job_rx.lock().map_err(|_| ()).and_then(|rx| rx.recv().map_err(|_| ()));
                    let Ok(Some((slot, i, j, base))) = job else { break };
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                        improve_pair(&unshared(&base), i, j, &worker))).map_err(|_| ());
                    if outcome_tx.send((slot, outcome)).is_err() { break; }
                }
            });
        }
        drop(outcome_tx);
        while next < pairs.len() {
            let batch = &pairs[next..(next + threads).min(pairs.len())];
            let base = Arc::new(current.clone());
            for (slot, &(i, j)) in batch.iter().enumerate() {
                if job_tx.send(Some((slot, i, j, base.clone()))).is_err() { break; }
            }
            let mut outcomes: Vec<Option<Result<Option<Vec<WorkingPrimitive>>, ()>>> = vec![None; batch.len()];
            for _ in 0..batch.len() {
                let Ok((slot, outcome)) = outcome_rx.recv() else { break };
                outcomes[slot] = Some(outcome);
            }
            let mut advanced = batch.len();
            let mut failed = false;
            for (slot, outcome) in outcomes.into_iter().enumerate() {
                match outcome {
                    Some(Ok(Some(updated))) => { current = updated; advanced = slot + 1; break; }
                    Some(Ok(None)) => {}
                    // A worker failed: finish from this pair on the caller's context.
                    _ => { advanced = slot; failed = true; break; }
                }
            }
            next += advanced;
            if failed { break; }
        }
        for _ in 0..threads { let _ = job_tx.send(None); }
    });
    for &(i, j) in &pairs[next..] {
        if let Some(updated) = improve_pair(&current, i, j, context) { current = updated; }
    }
    current
}

/// Deep copy with fresh reference counts. Worker threads that clone a shared
/// block contend on the same atomic counters; an owned copy keeps them apart.
fn unshared(items: &[WorkingPrimitive]) -> Vec<WorkingPrimitive> {
    fn s(v: &Arc<str>) -> Arc<str> { Arc::from(&**v) }
    fn v<T: Clone>(x: &Arc<Vec<T>>) -> Arc<Vec<T>> { Arc::new((**x).clone()) }
    fn strs(x: &Arc<Vec<Arc<str>>>) -> Arc<Vec<Arc<str>>> { Arc::new(x.iter().map(s).collect()) }
    items.iter().map(|w| {
        let p = &w.primitive;
        WorkingPrimitive {
            id: w.id, source_index: w.source_index, rotation: w.rotation,
            primitive: Primitive { id: s(&p.id), kind: s(&p.kind), label: s(&p.label), source_node_id: s(&p.source_node_id),
                source_node_ids: strs(&p.source_node_ids), locked: p.locked, can_rotate: p.can_rotate,
                allowed_orientations: v(&p.allowed_orientations), bbox: p.bbox.clone(), collision_boxes: v(&p.collision_boxes),
                collision_box_layers: strs(&p.collision_box_layers), width: p.width, height: p.height,
                placements: v(&p.placements), connection_points: v(&p.connection_points),
                connection_point_layers: strs(&p.connection_point_layers), path_ports: v(&p.path_ports),
                edge_place: p.edge_place.clone() },
            components: v(&w.components), source_components: v(&w.source_components),
            source_placements: v(&w.source_placements),
            source_node_ids: Arc::new(w.source_node_ids.iter().map(s).collect()),
            point_net_ids: v(&w.point_net_ids),
        }
    }).collect()
}

/// One pair of the bounded pair neighbourhood. Returns the improved block, or
/// `None` when no variant is accepted.
fn improve_pair(base: &[WorkingPrimitive], i: usize, j: usize, context: &Context) -> Option<Vec<WorkingPrimitive>> {
    let fixed: Vec<_> = base.iter().enumerate().filter(|(k,_)| *k != i && *k != j)
        .map(|(_, p)| p.clone()).collect();
    let mut variants = Vec::new();
    if context.problem.experiments.pair_swaps {
        for a in orientation_variants(&base[i]) {
            for b in orientation_variants(&base[j]) {
                let mut v = base.to_vec();
                v[i] = move_primitive_center_exact(a.clone(), box_center(&base[j].primitive.bbox));
                v[j] = move_primitive_center_exact(b, box_center(&base[i].primitive.bbox));
                variants.push(v);
            }
        }
    }
    if context.problem.experiments.reinsert_pair {
        let initial = incremental_evaluation(&fixed, context);
        // Both insertion orders; narrow local beam, capped independently of global beam.
        for (a, b) in [(i, j), (j, i)] {
            for first in ranked_block_candidates(&base[a], &fixed, &initial, 0.0, 2, context) {
                let mut partial = fixed.clone(); partial.push(first.primitive.clone());
                for second in ranked_block_candidates(&base[b], &partial, &first.incremental, 0.0, 2, context) {
                    let mut v = base.to_vec(); v[a] = first.primitive.clone(); v[b] = second.primitive;
                    variants.push(v);
                }
            }
        }
    }
    let route_pair = |v: &[WorkingPrimitive]| block_layout_route(&v.iter().collect::<Vec<_>>(), context);
    let old = evaluate(base, context);
    let mut best_score = old.score + route_pair(base);
    let mut best_hard = old.hard_violations;
    let mut accepted: Option<Vec<WorkingPrimitive>> = None;
    let mut ranked: Vec<_> = variants.into_iter().map(|v| { let e = evaluate(&v, context); (v,e) }).collect();
    ranked.sort_by(|a,b| a.1.hard_violations.cmp(&b.1.hard_violations).then_with(|| compare_f32(a.1.score,b.1.score)));
    for (v,e) in ranked.into_iter().take(4) {
        if e.hard_violations > best_hard || (e.hard_violations == best_hard && e.score >= best_score) { continue; }
        let score = e.score + route_pair(&v);
        if e.hard_violations < best_hard || (e.hard_violations == best_hard && score + 0.001 < best_score) {
            let current = accepted.as_deref().unwrap_or(base);
            if context.trace && (trace::is_target(&current[i]) || trace::is_target(&current[j])) {
                trace::emit(context,"pair_accepted",serde_json::json!({"pair":[current[i].primitive.label,current[j].primitive.label],
                    "before":trace::poses(current),"after":trace::poses(&v),
                    "beforeParts":trace::parts(current,context),"afterParts":trace::parts(&v,context),
                    "beforeEffective":best_score,"afterEffective":score}));
            }
            accepted = Some(v); best_hard = e.hard_violations; best_score = score;
        }
    }
    accepted
}

fn adjacent_centers(width: f32, height: f32, anchor: &Box2, clearance: f32) -> [Point; 8] {
    let cx = (anchor.left + anchor.right) / 2.0;
    let cy = (anchor.top + anchor.bottom) / 2.0;
    [
        Point {
            x: anchor.right + clearance + width / 2.0,
            y: cy,
        },
        Point {
            x: anchor.left - clearance - width / 2.0,
            y: cy,
        },
        Point {
            x: cx,
            y: anchor.bottom + clearance + height / 2.0,
        },
        Point {
            x: cx,
            y: anchor.top - clearance - height / 2.0,
        },
        Point {
            x: anchor.right + clearance + width / 2.0,
            y: anchor.bottom + clearance + height / 2.0,
        },
        Point {
            x: anchor.right + clearance + width / 2.0,
            y: anchor.top - clearance - height / 2.0,
        },
        Point {
            x: anchor.left - clearance - width / 2.0,
            y: anchor.bottom + clearance + height / 2.0,
        },
        Point {
            x: anchor.left - clearance - width / 2.0,
            y: anchor.top - clearance - height / 2.0,
        },
    ]
}

fn box_anchored_candidates(
    primitive: &WorkingPrimitive,
    moving_box: &Box2,
    anchor: &Box2,
    clearance: f32,
    context: &Context,
) -> Vec<WorkingPrimitive> {
    adjacent_centers(
        moving_box.right - moving_box.left,
        moving_box.bottom - moving_box.top,
        anchor,
        clearance,
    )
    .into_iter()
    .flat_map(|center| {
        [
            fit_to_bounds(
                move_box_center_exact(primitive.clone(), moving_box, center),
                context,
            ),
            fit_to_bounds(
                move_box_center(primitive.clone(), moving_box, center, context.problem.grid),
                context,
            ),
        ]
    })
    .collect()
}

fn orientation_variants(primitive: &WorkingPrimitive) -> Vec<WorkingPrimitive> {
    let orientations: Vec<i32> = if primitive.primitive.locked {
        vec![0]
    } else if !primitive.primitive.allowed_orientations.is_empty() {
        primitive.primitive.allowed_orientations.as_ref().clone()
    } else if primitive.primitive.can_rotate {
        vec![0, 90, 180, 270]
    } else {
        vec![0]
    };
    orientations
        .into_iter()
        .map(|angle| rotate_primitive(primitive, angle))
        .collect()
}

fn translate_primitive(primitive: &WorkingPrimitive, dx: f32, dy: f32) -> WorkingPrimitive {
    let mut next = primitive.clone();
    next.primitive.bbox = translate_box(&next.primitive.bbox, dx, dy);
    next.primitive.collision_boxes = Arc::new(
        next.primitive
            .collision_boxes
            .iter()
            .map(|box_| translate_box(box_, dx, dy))
            .collect(),
    );
    next.primitive.placements = Arc::new(
        next.primitive
            .placements
            .iter()
            .cloned()
            .map(|mut placement| {
                placement.x = round_placement(placement.x + dx);
                placement.y = round_placement(placement.y + dy);
                placement
            })
            .collect(),
    );
    next.primitive.connection_points = Arc::new(
        next.primitive
            .connection_points
            .iter()
            .cloned()
            .map(|mut point| {
                point.x = round_placement(point.x + dx);
                point.y = round_placement(point.y + dy);
                point
            })
            .collect(),
    );
    next.primitive.path_ports = Arc::new(
        next.primitive
            .path_ports
            .iter()
            .cloned()
            .map(|mut port| {
                port.x = round_placement(port.x + dx);
                port.y = round_placement(port.y + dy);
                port
            })
            .collect(),
    );
    rebuild_component_geometry(&mut next);
    next
}

fn rotate_primitive(primitive: &WorkingPrimitive, angle: i32) -> WorkingPrimitive {
    let normalized = normalize_rotation(angle);
    if normalized == 0 {
        return primitive.clone();
    }
    let origin = box_center(&primitive.primitive.bbox);
    let mut next = primitive.clone();
    next.rotation = crate::geometry::add_rotation(next.rotation, normalized);
    if !next.primitive.allowed_orientations.is_empty() {
        let mut orientations: Vec<_> = next
            .primitive
            .allowed_orientations
            .iter()
            .map(|orientation| crate::geometry::subtract_rotation(*orientation, normalized))
            .collect();
        orientations.sort();
        orientations.dedup();
        next.primitive.allowed_orientations = Arc::new(orientations);
    }
    next.primitive.bbox = rotate_box(&primitive.primitive.bbox, &origin, normalized);
    next.primitive.width = round_placement(next.primitive.bbox.right - next.primitive.bbox.left);
    next.primitive.height = round_placement(next.primitive.bbox.bottom - next.primitive.bbox.top);
    next.primitive.collision_boxes = Arc::new(
        primitive
            .primitive
            .collision_boxes
            .iter()
            .map(|box_| rotate_box(box_, &origin, normalized))
            .collect(),
    );
    next.primitive.placements = Arc::new(
        primitive
            .primitive
            .placements
            .iter()
            .cloned()
            .map(|mut placement| {
                let point = rotate_point(
                    &Point {
                        x: placement.x,
                        y: placement.y,
                    },
                    &origin,
                    normalized,
                );
                placement.x = point.x;
                placement.y = point.y;
                placement.rotate = crate::geometry::add_rotation(placement.rotate, normalized);
                placement
            })
            .collect(),
    );
    next.primitive.connection_points = Arc::new(
        primitive
            .primitive
            .connection_points
            .iter()
            .cloned()
            .map(|mut point| {
                let rotated = rotate_point(
                    &Point {
                        x: point.x,
                        y: point.y,
                    },
                    &origin,
                    normalized,
                );
                point.x = rotated.x;
                point.y = rotated.y;
                point
            })
            .collect(),
    );
    next.primitive.path_ports = Arc::new(
        primitive
            .primitive
            .path_ports
            .iter()
            .cloned()
            .map(|mut port| {
                let rotated = rotate_point(
                    &Point {
                        x: port.x,
                        y: port.y,
                    },
                    &origin,
                    normalized,
                );
                port.x = rotated.x;
                port.y = rotated.y;
                port.normal = signal_path::rotate_normal(&port.normal, normalized);
                port
            })
            .collect(),
    );
    rebuild_component_geometry(&mut next);
    next
}

fn rebuild_component_geometry(primitive: &mut WorkingPrimitive) {
    primitive.components = Arc::new(
        primitive
            .source_components
            .iter()
            .cloned()
            .map(|(index, mut component)| {
                let source = primitive
                    .source_placements
                    .iter()
                    .find(|placement| placement.designator == component.designator);
                let target = primitive
                    .primitive
                    .placements
                    .iter()
                    .find(|placement| placement.designator == component.designator);
                if let (Some(source), Some(target)) = (source, target) {
                    let delta_rotation = crate::geometry::subtract_rotation(target.rotate, source.rotate);
                    let source_origin = Point {
                        x: source.x,
                        y: source.y,
                    };
                    let dx = round_placement(target.x - source.x);
                    let dy = round_placement(target.y - source.y);
                    // Body bounds can be offset from the footprint origin. Transform
                    // the original bounds just like the other component geometry;
                    // rebuilding a centered rectangle loses that authored offset.
                    for row in &mut component.access_rows { row.box_ = translate_box(&rotate_box(&row.box_, &source_origin, delta_rotation), dx, dy); }
                    component.body_box = translate_box(
                        &rotate_box(&component.body_box, &source_origin, delta_rotation),
                        dx,
                        dy,
                    );
                    component.through_hole_boxes = Arc::new(
                        component
                            .through_hole_boxes
                            .iter()
                            .map(|box_| {
                                translate_box(
                                    &rotate_box(box_, &source_origin, delta_rotation),
                                    dx,
                                    dy,
                                )
                            })
                            .collect(),
                    );
                }
                (index, component)
            })
            .collect(),
    );
}

fn center_primitives(primitives: Vec<WorkingPrimitive>, grid: f32) -> Vec<WorkingPrimitive> {
    if primitives.is_empty() {
        return primitives;
    }
    let bbox = union_boxes(
        &primitives
            .iter()
            .map(|primitive| primitive.primitive.bbox)
            .collect::<Vec<_>>(),
    );
    let dx = round_placement(snap(-(bbox.left + bbox.right) / 2.0, grid));
    let dy = round_placement(snap(-(bbox.top + bbox.bottom) / 2.0, grid));
    primitives
        .iter()
        .map(|primitive| translate_primitive(primitive, dx, dy))
        .collect()
}

fn center_primitive(primitive: WorkingPrimitive, grid: f32) -> WorkingPrimitive {
    move_primitive_center(primitive, Point { x: 0.0, y: 0.0 }, grid)
}

fn move_primitive_center(
    primitive: WorkingPrimitive,
    center: Point,
    grid: f32,
) -> WorkingPrimitive {
    let current = box_center(&primitive.primitive.bbox);
    let dx = snap(center.x - current.x, grid);
    let dy = snap(center.y - current.y, grid);
    translate_primitive(&primitive, dx, dy)
}

fn move_primitive_center_exact(primitive: WorkingPrimitive, center: Point) -> WorkingPrimitive {
    let current = box_center(&primitive.primitive.bbox);
    translate_primitive(
        &primitive,
        round_placement(center.x - current.x),
        round_placement(center.y - current.y),
    )
}

fn move_box_center(
    primitive: WorkingPrimitive,
    box_: &Box2,
    center: Point,
    grid: f32,
) -> WorkingPrimitive {
    let current = box_center(box_);
    let dx = snap(center.x - current.x, grid);
    let dy = snap(center.y - current.y, grid);
    translate_primitive(&primitive, dx, dy)
}

fn move_box_center_exact(
    primitive: WorkingPrimitive,
    box_: &Box2,
    center: Point,
) -> WorkingPrimitive {
    let current = box_center(box_);
    translate_primitive(
        &primitive,
        round_placement(center.x - current.x),
        round_placement(center.y - current.y),
    )
}

fn fit_to_bounds(primitive: WorkingPrimitive, context: &Context) -> WorkingPrimitive {
    let Some(bounds) = context.problem.bounds else {
        return primitive;
    };
    let dx = if primitive.primitive.bbox.left < bounds.left {
        bounds.left - primitive.primitive.bbox.left
    } else if primitive.primitive.bbox.right > bounds.right {
        bounds.right - primitive.primitive.bbox.right
    } else {
        0.0
    };
    let dy = if primitive.primitive.bbox.top < bounds.top {
        bounds.top - primitive.primitive.bbox.top
    } else if primitive.primitive.bbox.bottom > bounds.bottom {
        bounds.bottom - primitive.primitive.bbox.bottom
    } else {
        0.0
    };
    translate_primitive(&primitive, round_placement(dx), round_placement(dy))
}

fn primitive_candidate_boxes(primitive: &WorkingPrimitive, context: &Context) -> Vec<Box2> {
    if context.problem.candidate_box_mode.as_ref() == "collision" {
        primitive_collision_boxes(primitive, context).to_vec()
    } else {
        vec![primitive.primitive.bbox]
    }
}

fn primitive_collision_boxes<'a>(primitive: &'a WorkingPrimitive, context: &Context) -> &'a [Box2] {
    let envelope = context.problem.collision_mode.as_ref() == "envelope"
        || context.problem.collision_mode.as_ref() == "hybrid";
    if envelope
        && (primitive.primitive.kind.as_ref() == "block"
            || primitive.primitive.kind.as_ref() == "module")
    {
        std::slice::from_ref(&primitive.primitive.bbox)
    } else if primitive.primitive.collision_boxes.is_empty() {
        std::slice::from_ref(&primitive.primitive.bbox)
    } else {
        primitive.primitive.collision_boxes.as_ref()
    }
}

fn dedupe_primitives(primitives: Vec<WorkingPrimitive>) -> Vec<WorkingPrimitive> {
    let mut seen = FxHashSet::default();
    primitives
        .into_iter()
        .filter(|primitive| seen.insert(primitive_pose_key(primitive)))
        .collect()
}

fn primitive_pose_key(primitive: &WorkingPrimitive) -> PrimitivePoseKey {
    PrimitivePoseKey {
        primitive_id: primitive.id,
        rotation: normalize_rotation(primitive.rotation),
        left: number_key(primitive.primitive.bbox.left),
        top: number_key(primitive.primitive.bbox.top),
        right: number_key(primitive.primitive.bbox.right),
        bottom: number_key(primitive.primitive.bbox.bottom),
    }
}

fn evaluate(primitives: &[WorkingPrimitive], context: &Context) -> Evaluation {
    let key: Vec<_> = primitives.iter().map(primitive_pose_key).collect();
    if let Some(evaluation) = context.evaluation_cache.borrow().get(&key).copied() {
        return evaluation;
    }
    let evaluation = Evaluation {
        hard_violations: hard_geometry_violation_count(primitives, context),
        score: score_block(primitives, context),
    };
    let mut cache = context.evaluation_cache.borrow_mut();
    if cache.len() < 10_000 {
        cache.insert(key, evaluation);
    }
    evaluation
}

fn incremental_evaluation(
    primitives: &[WorkingPrimitive],
    context: &Context,
) -> IncrementalEvaluation {
    let primitive_overlap_depths = primitive_overlap_matrix(primitives, context);
    let evaluation = Evaluation {
        hard_violations: hard_geometry_violation_count(primitives, context),
        score: score_block_with_overlap_matrix(
            primitives,
            context,
            Some(&primitive_overlap_depths),
        ),
    };
    validate_incremental_evaluation(primitives, evaluation, context);
    IncrementalEvaluation {
        evaluation,
        primitive_overlap_depths,
        size: primitives.len(),
    }
}

fn append_incremental_evaluation(
    placed: &[WorkingPrimitive],
    primitives: &[WorkingPrimitive],
    previous: &IncrementalEvaluation,
    hard: usize,
    ceiling: f32,
    power_frame: Option<&PowerYieldFrame<'_>>,
    long_net_frame: Option<&LongNetFrame>,
    context: &Context,
) -> Option<IncrementalEvaluation> {
    debug_assert_eq!(previous.size, placed.len());
    debug_assert_eq!(primitives.len(), placed.len() + 1);
    let candidate = primitives.last().expect("appended primitive");
    if ceiling == f32::NEG_INFINITY { let _span = context.detail.span("candidates_pruned_hard"); return None; }
    let primitive_overlap_depths = extend_primitive_overlap_matrix(
        &previous.primitive_overlap_depths,
        placed,
        candidate,
        context,
    );
    // Incremental sums can differ from the full traversal by a few floating
    // point ulps. Widen the pruning boundary so a near-tie is never lost.
    let roundoff = if power_frame.is_some() || long_net_frame.is_some() {
        let count = context.problem.primitives.iter().map(|p|
            1 + p.connection_points.len() + p.path_ports.len() + p.collision_boxes.len()).sum::<usize>()
            + context.problem.components.len() + context.problem.relations.len();
        // Scorers have at most three nested entity loops; 128 bounds the
        // arithmetic per combination. Terms are nonnegative (configured
        // relation weights are clamped by relation_weight).
        let operations = count.saturating_pow(3).saturating_mul(128);
        crate::f32_policy::nonnegative_sum_error(operations, ceiling.abs())
    } else { 0.0 };
    let Some(score) = score_block_bounded(
        primitives, context, Some(&primitive_overlap_depths), ceiling, roundoff,
        power_frame, long_net_frame,
    ) else {
        if context.validate_incremental_scoring {
            let full = score_block_with_overlap_matrix(primitives, context, Some(&primitive_overlap_depths));
            assert!(full > ceiling, "invalid score lower bound");
        }
        let _span = context.detail.span("candidates_pruned_score"); return None;
    };
    let evaluation = Evaluation { hard_violations: hard, score };
    validate_incremental_evaluation(primitives, evaluation, context);
    Some(IncrementalEvaluation {
        evaluation,
        primitive_overlap_depths,
        size: primitives.len(),
    })
}

fn primitive_overlap_matrix(primitives: &[WorkingPrimitive], context: &Context) -> Vec<f32> {
    let size = primitives.len();
    let mut depths = vec![0.0; size * size];
    for i in 0..size {
        for j in (i + 1)..size {
            if !primitive_can_conflict(&primitives[i], &primitives[j], context) {
                continue;
            }
            let depth = primitive_overlap_depth(&primitives[i], &primitives[j], context);
            depths[i * size + j] = depth;
            depths[j * size + i] = depth;
        }
    }
    depths
}

fn extend_primitive_overlap_matrix(
    previous: &[f32],
    placed: &[WorkingPrimitive],
    candidate: &WorkingPrimitive,
    context: &Context,
) -> Vec<f32> {
    let old_size = placed.len();
    let size = old_size + 1;
    debug_assert_eq!(previous.len(), old_size * old_size);
    let mut depths = vec![0.0; size * size];
    for row in 0..old_size {
        depths[row * size..row * size + old_size]
            .copy_from_slice(&previous[row * old_size..(row + 1) * old_size]);
    }
    for (index, primitive) in placed.iter().enumerate() {
        if !primitive_can_conflict(primitive, candidate, context) {
            continue;
        }
        let depth = primitive_overlap_depth(primitive, candidate, context);
        depths[index * size + old_size] = depth;
        depths[old_size * size + index] = depth;
    }
    depths
}

fn validate_incremental_evaluation(
    primitives: &[WorkingPrimitive],
    actual: Evaluation,
    context: &Context,
) {
    if !context.validate_incremental_scoring {
        return;
    }
    let expected = evaluate(primitives, context);
    assert_eq!(
        actual.hard_violations, expected.hard_violations,
        "incremental hard violation count mismatch"
    );
    let tolerance = crate::f32_policy::diagnostic_tolerance(expected.score, actual.score);
    assert!((actual.score - expected.score).abs() <= tolerance,
        "incremental block score mismatch: {} vs {}", actual.score, expected.score);
}

fn hard_geometry_violation_count(primitives: &[WorkingPrimitive], context: &Context) -> usize {
    let _span = context.detail.span("hard_geometry_violation_count");
    let mut count = 0;
    for i in 0..primitives.len() {
        for j in (i + 1)..primitives.len() {
            if !primitive_can_conflict(&primitives[i], &primitives[j], context) {
                continue;
            }
            let overlap = if context.problem.hard_collision_mode.as_ref() == "primitive" {
                primitive_overlap_depth(&primitives[i], &primitives[j], context)
            } else {
                component_overlap_depth(&primitives[i], &primitives[j], context)
            };
            if overlap > 0.0 {
                count += 1;
            }
        }
    }
    for primitive in primitives {
        count += world_violations(primitive, context);
        for obstacle in &context.problem.obstacles {
            let component_boxes;
            let boxes = if context.problem.hard_collision_mode.as_ref() == "primitive" {
                primitive_collision_boxes(primitive, context)
            } else {
                component_boxes = primitive_component_boxes(primitive);
                &component_boxes
            };
            if boxes
                .iter()
                .any(|box_| overlap_depth(box_, obstacle, context.problem.clearance) > 0.0)
            {
                count += 1;
            }
        }
        if let Some(bounds) = context.problem.bounds {
            if !(context.problem.world.is_some() && primitive.primitive.locked) && primitive_outside_bounds(primitive, &bounds) {
                count += 1;
            }
        }
    }
    count + condition_evaluation(primitives,context).hard
}

fn candidate_hard_violation_count(
    candidate: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> usize {
    let _span = context.detail.span("candidate_hard_violation_count");
    let mut count = world_violations(candidate, context);
    for other in placed {
        if !primitive_can_conflict(candidate, other, context) {
            continue;
        }
        let overlap = if context.problem.hard_collision_mode.as_ref() == "primitive" {
            primitive_overlap_depth(candidate, other, context)
        } else {
            component_overlap_depth(candidate, other, context)
        };
        if overlap > 0.0 {
            count += 1;
        }
    }
    for obstacle in &context.problem.obstacles {
        let component_boxes;
        let boxes = if context.problem.hard_collision_mode.as_ref() == "primitive" {
            primitive_collision_boxes(candidate, context)
        } else {
            component_boxes = primitive_component_boxes(candidate);
            &component_boxes
        };
        if boxes
            .iter()
            .any(|box_| overlap_depth(box_, obstacle, context.problem.clearance) > 0.0)
        {
            count += 1;
        }
    }
    if let Some(bounds) = context.problem.bounds {
        if primitive_outside_bounds(candidate, &bounds) {
            count += 1;
        }
    }
    if !context.conditions.is_empty() {
        let mut all=placed.to_vec();all.push(candidate.clone());
        count += condition_evaluation(&all,context).hard.saturating_sub(condition_evaluation(placed,context).hard);
    }
    count
}

fn world_violations(primitive: &WorkingPrimitive, context: &Context) -> usize {
    let Some(world) = &context.problem.world else { return 0; };
    // The fixed anchor may intentionally overhang the board. Only its movable
    // support circuitry is constrained to the usable board interior.
    if primitive.primitive.locked { return 0; }
    let mut count = 0;
    for (_, component) in primitive.components.iter() {
        if std::iter::once(&component.body_box).chain(component.through_hole_boxes.iter()).any(|b|
            !crate::geometry::box_inside_polygon_board(b, &world.bounds,
                &world.outline, world.edge_clearance)) { count += 1; }
        for obstacle in world.obstacles.iter().filter(|o| o.designator == component.designator) {
            let boxes = if obstacle.layer.as_ref().is_none_or(|l| l == &component.layer) {
                std::slice::from_ref(&component.body_box)
            } else { component.through_hole_boxes.as_slice() };
            if boxes.iter().any(|b| overlap_depth(b, &obstacle.box_, obstacle.clearance) > 0.0) { count += 1; }
        }
    }
    count
}

fn primitive_can_conflict(a: &WorkingPrimitive, b: &WorkingPrimitive, context: &Context) -> bool {
    if a.components.is_empty() || b.components.is_empty() {
        return true;
    }
    let count = context.problem.components.len();
    a.components.iter().any(|(a_index, _)| {
        b.components
            .iter()
            .any(|(b_index, _)| context.problem.component_conflict[a_index * count + b_index] != 0)
    })
}

fn component_overlap_depth(a: &WorkingPrimitive, b: &WorkingPrimitive, context: &Context) -> f32 {
    let count = context.problem.components.len();
    let mut max_overlap: f32 = 0.0;
    for (a_index, a_component) in a.components.iter() {
        for (b_index, b_component) in b.components.iter() {
            if context.problem.component_conflict[a_index * count + b_index] == 0 {
                continue;
            }
            let clearance = context.problem.component_pair_clearance[a_index * count + b_index];
            if a_component.layer == b_component.layer {
                max_overlap = max_overlap.max(overlap_depth(
                    &a_component.body_box,
                    &b_component.body_box,
                    clearance,
                ));
                for a_box in a_component.through_hole_boxes.iter() {
                    for b_box in b_component.through_hole_boxes.iter() {
                        max_overlap = max_overlap.max(overlap_depth(a_box, b_box, clearance));
                    }
                }
            } else {
                for box_ in b_component.through_hole_boxes.iter() {
                    max_overlap =
                        max_overlap.max(overlap_depth(&a_component.body_box, box_, clearance));
                }
                for box_ in a_component.through_hole_boxes.iter() {
                    max_overlap =
                        max_overlap.max(overlap_depth(box_, &b_component.body_box, clearance));
                }
            }
        }
    }
    max_overlap
}

fn primitive_overlap_depth(a: &WorkingPrimitive, b: &WorkingPrimitive, context: &Context) -> f32 {
    let mut max_overlap: f32 = 0.0;
    for a_box in primitive_collision_boxes(a, context) {
        for b_box in primitive_collision_boxes(b, context) {
            max_overlap = max_overlap.max(overlap_depth(&a_box, &b_box, context.problem.clearance));
        }
    }
    max_overlap
}

fn primitive_component_boxes(primitive: &WorkingPrimitive) -> Vec<Box2> {
    let boxes: Vec<_> = primitive
        .components
        .iter()
        .flat_map(|(_, component)| {
            let mut boxes = vec![component.body_box];
            boxes.extend(component.through_hole_boxes.iter().copied());
            boxes
        })
        .collect();
    if boxes.is_empty() {
        primitive.primitive.collision_boxes.as_ref().clone()
    } else {
        boxes
    }
}

fn primitive_outside_bounds(primitive: &WorkingPrimitive, bounds: &Box2) -> bool {
    crate::geometry::box_outside_bounds_severity(&primitive.primitive.bbox,bounds)>0.0
}

fn score_block(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    score_block_with_overlap_matrix(primitives, context, None)
}

fn score_block_with_overlap_matrix(
    primitives: &[WorkingPrimitive],
    context: &Context,
    primitive_overlap_depths: Option<&[f32]>,
) -> f32 {
    score_block_bounded(primitives, context, primitive_overlap_depths, f32::INFINITY, 0.0, None, None)
        .expect("an unbounded score cannot be pruned")
}

fn score_block_bounded(primitives: &[WorkingPrimitive], context: &Context, primitive_overlap_depths: Option<&[f32]>, ceiling: f32,
    cached_roundoff: f32, power_frame: Option<&PowerYieldFrame<'_>>, long_net_frame: Option<&LongNetFrame>) -> Option<f32> {
    score_block_terms(primitives, context, primitive_overlap_depths, ceiling, cached_roundoff, power_frame, long_net_frame, true)
}

fn score_block_terms(primitives: &[WorkingPrimitive], context: &Context, primitive_overlap_depths: Option<&[f32]>, ceiling: f32,
    cached_roundoff: f32, power_frame: Option<&PowerYieldFrame<'_>>, long_net_frame: Option<&LongNetFrame>, include_geometry: bool) -> Option<f32> {
    let _span = context.detail.span("score_block_with_overlap_matrix");
    if primitives.is_empty() {
        return Some(0.0);
    }
    let bbox = union_boxes(
        &primitives
            .iter()
            .map(|primitive| primitive.primitive.bbox)
            .collect::<Vec<_>>(),
    );
    let width = bbox.right - bbox.left;
    let height = bbox.bottom - bbox.top;
    let high = context.problem.compactness.as_ref() == "high";
    let (
        bbox_area,
        bbox_perimeter,
        hull_area,
        hull_perimeter,
        aspect_weight,
        relation_weight_,
        net_weight,
        target_weight,
    ) = if high {
        (4.2, 5.4, 14.4, 12.0, 1.4, 1.0, 0.65, 1.2)
    } else {
        (3.2, 2.5, 10.0, 1.5, 1.0, 1.0, 1.0, 1.0)
    };
    let boxes: Vec<_> = primitives
        .iter()
        .flat_map(|primitive| {
            primitive_collision_boxes(primitive, context)
                .iter()
                .copied()
        })
        .collect();
    let (hull_a, hull_p) = { let _span = context.detail.span("convex_hull"); convex_hull_metrics(if boxes.is_empty() { vec![bbox] } else { boxes }) };
    let mut score = width * height * bbox_area
        + (width + height) * bbox_perimeter
        + hull_a * hull_area * if context.problem.experiments.reduced_hull { 0.25 } else { 1.0 }
        + hull_p * hull_perimeter;
    score += aspect_ratio_penalty(width, height) * aspect_weight;
    if context.problem.experiments.smooth_aspect {
        let ratio = width.max(height) / width.min(height).max(0.1);
        score += (ratio - 2.0).max(0.0).powi(2) * width.min(height) * 12.0;
    }
    score += overlap_penalty(primitives, context, primitive_overlap_depths);
    score += bounds_penalty(primitives, context.problem.bounds.as_ref());
    // Diagnostic electrical subtotal uses the exact same native terms/weights.
    // Geometry remains in the authoritative full search score above.
    if !include_geometry { score = 0.0; }
    // This exact prefix is identical in cached and uncached evaluation. Every
    // subsequent contribution is nonnegative, so monotone F32 additions cannot
    // bring the full score below this prefix. Cached-sum uncertainty does not
    // apply yet, even when its later bound is infinite.
    if score > ceiling { return None; }
    let safe_ceiling = ceiling + cached_roundoff;
    score += dense_ic_access_penalty(primitives, context);
    score += power_frame.map_or_else(|| power_yield_penalty(primitives, context), |frame| {
        frame.with_candidate(primitives.last().expect("appended candidate"), context)
    }) * if high { 0.3 } else { 1.0 };
    score += scoped_relation_penalty(primitives, context) * relation_weight_;
    if score > safe_ceiling { return None; }
    score += external_port_exposure_penalty(primitives, context) * if high { 0.35 } else { 1.0 };
    score += port_facing_penalty(primitives, context) * if high { 0.55 } else { 1.0 };
    if score > safe_ceiling { return None; }
    let component_count: usize = primitives
        .iter()
        .map(|primitive| primitive.primitive.placements.len())
        .sum();
    let count_for_weight = if context.problem.experiments.stable_net_weight {
        context.problem.components.len()
    } else { component_count };
    let small = count_for_weight > 0 && count_for_weight < 5;
    let (signal_spread, ground_spread) = same_net_spread_penalties(
        primitives,
        context,
        if small { None } else { Some(3) },
        if small { None } else { Some(4.0) },
    );
    score += signal_spread * if small { 18.0 } else { 4.0 } * net_weight;
    score += ground_spread * if small { 2.5 } else { 0.15 } * net_weight;
    if include_geometry { score += target_size_penalty(width, height, context) * target_weight; }
    if context.problem.experiments.long_nets {
        score += long_net_frame.map_or_else(|| long_local_net_penalty(primitives, context),
            |frame| frame.with_candidate(primitives.last().expect("appended candidate"), context));
        score += connected_pin_stretch(primitives, context);
    }
    if score > safe_ceiling { return None; }
    let path_primitives: Vec<_> = primitives.iter().map(|item| &item.primitive).collect();
    score += { let _span = context.detail.span("signal_path_topology"); signal_path::explicit_topology_penalty(&path_primitives, &context.problem.relations) }
        + crate::ordinary_net::penalty(&path_primitives, &context.problem.relations) * if high { 2.5 } else { 4.0 };
    if score > safe_ceiling { return None; }
    Some(score)
}

fn overlap_penalty(
    primitives: &[WorkingPrimitive],
    context: &Context,
    primitive_overlap_depths: Option<&[f32]>,
) -> f32 {
    let _span = context.detail.span("overlap_penalty");
    let envelope = context.problem.collision_mode.as_ref() == "envelope"
        || context.problem.collision_mode.as_ref() == "hybrid";
    let mut penalty = 0.0;
    for i in 0..primitives.len() {
        for j in (i + 1)..primitives.len() {
            if !primitive_can_conflict(&primitives[i], &primitives[j], context) {
                continue;
            }
            let overlap = primitive_overlap_depths
                .map(|depths| depths[i * primitives.len() + j])
                .unwrap_or_else(|| {
                    primitive_overlap_depth(&primitives[i], &primitives[j], context)
                });
            if overlap > 0.0 {
                penalty += if envelope {
                    250_000.0 + overlap * 25_000.0
                } else {
                    10_000_000.0 + overlap * 100_000.0
                };
            }
        }
    }
    for primitive in primitives {
        for obstacle in &context.problem.obstacles {
            for box_ in primitive_collision_boxes(primitive, context) {
                let overlap = overlap_depth(&box_, obstacle, context.problem.clearance);
                if overlap > 0.0 {
                    penalty += 10_000_000.0 + overlap * 100_000.0;
                }
            }
        }
    }
    penalty
}

fn bounds_penalty(primitives: &[WorkingPrimitive], bounds: Option<&Box2>) -> f32 {
    let Some(bounds) = bounds else {
        return 0.0;
    };
    primitives
        .iter()
        .map(|primitive| {
            let box_ = primitive.primitive.bbox;
            let overflow = crate::geometry::box_outside_bounds_severity(&box_,bounds);
            if overflow > 0.0 {
                10_000_000.0 + overflow * 500_000.0
            } else {
                0.0
            }
        })
        .sum()
}

fn aspect_ratio_penalty(width: f32, height: f32) -> f32 {
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    let ratio = (width / height).max(height / width);
    let excess = (ratio - 5.0).max(0.0);
    if excess <= 0.0 {
        0.0
    } else {
        excess * excess * width.min(height).max(1.0) * 220.0
    }
}

fn target_size_penalty(width: f32, height: f32, context: &Context) -> f32 {
    let excess = |value: f32| {
        let value = value.max(0.0);
        value * value * 20_000.0 + value * 2_000.0
    };
    context
        .problem
        .target_width
        .map(|target| excess(width - target))
        .unwrap_or(0.0)
        + context
            .problem
            .target_height
            .map(|target| excess(height - target))
            .unwrap_or(0.0)
}

fn convex_hull_vertices(boxes: &[Box2]) -> (Vec<Point>, bool) {
    let mut points: Vec<Point> = boxes
        .iter()
        .flat_map(|box_| {
            [
                Point {
                    x: box_.left,
                    y: box_.top,
                },
                Point {
                    x: box_.right,
                    y: box_.top,
                },
                Point {
                    x: box_.right,
                    y: box_.bottom,
                },
                Point {
                    x: box_.left,
                    y: box_.bottom,
                },
            ]
        })
        .map(|point| Point {
            x: round_placement(point.x),
            y: round_placement(point.y),
        })
        .collect();
    points.sort_by(|a, b| compare_f32(a.x, b.x).then_with(|| compare_f32(a.y, b.y)));
    points.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if points.len() < 3 {return (points,true);}
    let cross =
        |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut lower = Vec::new();
    for point in points.iter() {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= 0.0
        {
            lower.pop();
        }
        lower.push(*point);
    }
    let mut upper = Vec::new();
    for point in points.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= 0.0
        {
            upper.pop();
        }
        upper.push(*point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    (lower,false)
}

fn convex_hull_metrics(boxes: Vec<Box2>) -> (f32, f32) {
    let (lower,degenerate)=convex_hull_vertices(&boxes);
    if degenerate {
        let bbox=union_boxes(&boxes);let width=(bbox.right-bbox.left).max(0.0);let height=(bbox.bottom-bbox.top).max(0.0);
        return (width*height,width+height);
    }
    let mut area = 0.0;
    let mut perimeter = 0.0;
    for index in 0..lower.len() {
        let next = lower[(index + 1) % lower.len()];
        area += lower[index].x * next.y - next.x * lower[index].y;
        perimeter += crate::numerics::hypot(lower[index].x - next.x,lower[index].y - next.y);
    }
    (area.abs() / 2.0, perimeter)
}

fn relation_anchored_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<WorkingPrimitive> {
    let mut candidates = Vec::new();
    for compiled in &context.relations {
        let relation = &compiled.relation;
        if relation.effect.as_ref() == "lock"
            || relation.kind.as_ref() == "net"
            || relation.effect.as_ref() == "score_only"
        {
            continue;
        }
        let moving_from = endpoint_point(std::slice::from_ref(primitive), &compiled.from);
        let placed_to = endpoint_point(placed, &compiled.to)
            .map(|point| relation_target_point(point, relation));
        if let (Some(moving), Some(target)) = (moving_from, placed_to) {
            let anchor = placed
                .iter()
                .find(|item| Some(item.id) == target.primitive_id)
                .map(|item| item.primitive.bbox)
                .unwrap_or(point_box(target.point));
            candidates.extend(endpoint_anchored_candidates(
                primitive,
                moving.point,
                &anchor,
                target.point,
                relation,
                context,
            ));
        }
        let moving_to = endpoint_point(std::slice::from_ref(primitive), &compiled.to);
        let placed_from = endpoint_point(placed, &compiled.from)
            .map(|point| relation_target_point(point, relation));
        if let (Some(moving), Some(target)) = (moving_to, placed_from) {
            let anchor = placed
                .iter()
                .find(|item| Some(item.id) == target.primitive_id)
                .map(|item| item.primitive.bbox)
                .unwrap_or(point_box(target.point));
            candidates.extend(endpoint_anchored_candidates(
                primitive,
                moving.point,
                &anchor,
                target.point,
                relation,
                context,
            ));
        }
    }
    candidates
}

fn endpoint_anchored_candidates(
    primitive: &WorkingPrimitive,
    moving: Point,
    anchor: &Box2,
    target: Point,
    relation: &Relation,
    context: &Context,
) -> Vec<WorkingPrimitive> {
    let center = box_center(&primitive.primitive.bbox);
    let offset = Point {
        x: moving.x - center.x,
        y: moving.y - center.y,
    };
    let near_distance = context
        .problem
        .clearance
        .max(relation.min_distance.unwrap_or(0.0));
    let size = primitive.primitive.width.max(primitive.primitive.height);
    let slide_step = context
        .problem
        .grid
        .max((size * 0.25).min(relation.max_distance.unwrap_or(size)));
    let slides = dedupe_numbers(vec![
        0.0,
        -slide_step,
        slide_step,
        -slide_step * 2.0,
        slide_step * 2.0,
    ]);
    let c = context.problem.clearance;
    let mut centers = vec![
        Point {
            x: target.x - offset.x - c,
            y: target.y - offset.y,
        },
        Point {
            x: target.x - offset.x + c,
            y: target.y - offset.y,
        },
        Point {
            x: target.x - offset.x,
            y: target.y - offset.y - c,
        },
        Point {
            x: target.x - offset.x,
            y: target.y - offset.y + c,
        },
        Point {
            x: anchor.left - c - primitive.primitive.width / 2.0,
            y: target.y - offset.y,
        },
        Point {
            x: anchor.right + c + primitive.primitive.width / 2.0,
            y: target.y - offset.y,
        },
        Point {
            x: target.x - offset.x,
            y: anchor.top - c - primitive.primitive.height / 2.0,
        },
        Point {
            x: target.x - offset.x,
            y: anchor.bottom + c + primitive.primitive.height / 2.0,
        },
    ];
    for slide in slides {
        centers.extend([
            Point {
                x: anchor.left - c - primitive.primitive.width / 2.0,
                y: target.y - offset.y + slide,
            },
            Point {
                x: anchor.right + c + primitive.primitive.width / 2.0,
                y: target.y - offset.y + slide,
            },
            Point {
                x: target.x - offset.x + slide,
                y: anchor.top - c - primitive.primitive.height / 2.0,
            },
            Point {
                x: target.x - offset.x + slide,
                y: anchor.bottom + c + primitive.primitive.height / 2.0,
            },
        ]);
    }
    if let Some(max_distance) = relation.max_distance {
        if max_distance > near_distance {
            let distances = dedupe_numbers(
                vec![
                    near_distance,
                    max_distance * 0.35,
                    max_distance * 0.6,
                    max_distance * 0.9,
                ]
                .into_iter()
                .filter(|value| *value <= max_distance)
                .collect(),
            );
            let directions = [
                Point { x: 1.0, y: 0.0 },
                Point { x: -1.0, y: 0.0 },
                Point { x: 0.0, y: 1.0 },
                Point { x: 0.0, y: -1.0 },
                Point { x: 1.0, y: 1.0 },
                Point { x: 1.0, y: -1.0 },
                Point { x: -1.0, y: 1.0 },
                Point { x: -1.0, y: -1.0 },
            ]
            .map(normalize);
            for distance in distances {
                for direction in directions {
                    centers.push(Point {
                        x: target.x + direction.x * distance - offset.x,
                        y: target.y + direction.y * distance - offset.y,
                    });
                }
            }
        }
    }
    centers
        .into_iter()
        .flat_map(|center| {
            [
                fit_to_bounds(
                    move_primitive_center_exact(primitive.clone(), center),
                    context,
                ),
                fit_to_bounds(
                    move_primitive_center(primitive.clone(), center, context.problem.grid),
                    context,
                ),
            ]
        })
        .collect()
}

fn scoped_relation_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("scoped_relation_penalty");
    let mut penalty = 0.0;
    for compiled in &context.relations {
        let relation = &compiled.relation;
        if relation.effect.as_ref() == "lock" || relation.kind.as_ref() == "net" || crate::conditions::applies(relation) {
            continue;
        }
        let Some(from) = endpoint_point(primitives, &compiled.from) else {
            continue;
        };
        let Some(to) = endpoint_point(primitives, &compiled.to)
            .map(|point| relation_target_point(point, relation))
        else {
            continue;
        };
        if from.primitive_id == to.primitive_id {
            continue;
        }
        let weight = relation_weight(relation);
        let value = distance(from.point, to.point);
        let profile = crate::relation_score::for_relation(relation);
        penalty += crate::relation_score::distance_cost(value,weight,profile.min.unwrap_or(-1.0),profile.max.unwrap_or(-1.0),profile.inverse_tau_squared)
            + relation_side_penalty(relation, from.point, to.point, weight);
    }
    penalty + condition_evaluation(primitives,context).score
}

fn external_port_exposure_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("external_port_exposure_penalty");
    let bbox = union_boxes(
        &primitives
            .iter()
            .map(|primitive| primitive.primitive.bbox)
            .collect::<Vec<_>>(),
    );
    let count: usize = primitives
        .iter()
        .map(|primitive| primitive.primitive.placements.len())
        .sum();
    let weight = if count > 0 && count < 5 { 1.25 } else { 5.0 };
    let mut penalty = 0.0;
    for (relation_index, compiled) in context.relations.iter().enumerate() {
        let relation = &compiled.relation;
        if context.problem.experiments.local_access {
            // An endpoint in a not-yet-placed primitive is not an external port.
            let external = |e: &CompiledEndpoint| matches!(e, CompiledEndpoint::Missing | CompiledEndpoint::Anchor(None));
            if external(&compiled.from) == external(&compiled.to) { continue; }
        }
        if crate::conditions::applies(relation) || relation.effect.as_ref() == "lock"
            || relation.kind.as_ref() == "net"
            || relation.effect.as_ref() == "score_only"
        {
            continue;
        }
        let from = endpoint_point(primitives, &compiled.from);
        let to = endpoint_point(primitives, &compiled.to);
        if from.is_some() && to.is_some() {
            continue;
        }
        let Some(inside) = from.or(to) else {
            continue;
        };
        let boundary = if context.problem.experiments.local_access {
            escape_blockage_cached(inside, primitives, relation_index, context)
        } else { (inside.point.x - bbox.left)
            .abs()
            .min((bbox.right - inside.point.x).abs())
            .min((inside.point.y - bbox.top).abs())
            .min((bbox.bottom - inside.point.y).abs()) };
        penalty += boundary * relation_weight(relation) * weight;
    }
    penalty
}

fn escape_blockage_cached(source: EndpointPoint, primitives: &[WorkingPrimitive], relation: usize, context: &Context) -> f32 {
    let layer = primitives.iter().find(|p| Some(p.id) == source.primitive_id)
        .and_then(|p| p.primitive.placements.first()).map(|p| p.layer.clone());
    let point = [source.point.x.to_bits(), source.point.y.to_bits()];
    let mut cache = context.escape_cache.borrow_mut();
    let mut blocked = [0.0f32; 4];
    for p in primitives {
        if Some(p.id) == source.primitive_id { continue; }
        let key = (relation, p.source_index);
        let pose = primitive_pose_key(p);
        let valid = cache.get(&key).is_some_and(|v| v.point == point && v.source == source.primitive_id && v.layer == layer && v.pose == pose);
        if !valid {
            let mut contributions = Vec::new();
            for (_, c) in p.components.iter() {
                let boxes = if layer.as_ref().is_none_or(|l| l == &c.layer) { std::slice::from_ref(&c.body_box) }
                    else { c.through_hole_boxes.as_slice() };
                for b in boxes { contributions.push(escape_box_contribution(source.point, b, context.problem.clearance)); }
            }
            cache.insert(key, EscapeEntry { point, source: source.primitive_id, layer: layer.clone(), pose, contributions });
        }
        // Keep the reference accumulation order, including separate component boxes.
        for contribution in &cache[&key].contributions {
            for i in 0..4 { blocked[i] += contribution[i]; }
        }
    }
    let result = blocked.into_iter().fold(f32::INFINITY, f32::min);
    if context.validate_incremental_scoring {
        assert!((result - escape_blockage(source, primitives, context.problem.clearance)).abs() < 1e-8);
    }
    result
}

fn escape_box_contribution(point: Point, b: &Box2, clearance: f32) -> [f32; 4] {
    let mut blocked = [0.0; 4];
    if point.y >= b.top - clearance && point.y <= b.bottom + clearance {
        blocked[0] = (b.right + clearance - point.x.max(b.left - clearance)).max(0.0);
        blocked[1] = (point.x.min(b.right + clearance) - b.left + clearance).max(0.0);
    }
    if point.x >= b.left - clearance && point.x <= b.right + clearance {
        blocked[2] = (b.bottom + clearance - point.y.max(b.top - clearance)).max(0.0);
        blocked[3] = (point.y.min(b.bottom + clearance) - b.top + clearance).max(0.0);
    }
    blocked
}

// Four straight escape corridors: empty space has zero cost. Growing an
// unrelated side of the block no longer buries every external pin. Internal
// geometry of the rigid source island is assessed by its island solver.
fn escape_blockage(source: EndpointPoint, primitives: &[WorkingPrimitive], clearance: f32) -> f32 {
    let mut blocked = [0.0f32; 4];
    let layer = primitives.iter().find(|p| Some(p.id) == source.primitive_id)
        .and_then(|p| p.primitive.placements.first()).map(|p| &p.layer);
    for p in primitives {
        if Some(p.id) == source.primitive_id { continue; }
        for (_, c) in p.components.iter() {
            let boxes = if layer.is_none_or(|l| l == &c.layer) { std::slice::from_ref(&c.body_box) }
                else { c.through_hole_boxes.as_slice() };
            for b in boxes {
                if source.point.y >= b.top - clearance && source.point.y <= b.bottom + clearance {
                    blocked[0] += (b.right + clearance - source.point.x.max(b.left - clearance)).max(0.0);
                    blocked[1] += (source.point.x.min(b.right + clearance) - b.left + clearance).max(0.0);
                }
                if source.point.x >= b.left - clearance && source.point.x <= b.right + clearance {
                    blocked[2] += (b.bottom + clearance - source.point.y.max(b.top - clearance)).max(0.0);
                    blocked[3] += (source.point.y.min(b.bottom + clearance) - b.top + clearance).max(0.0);
                }
            }
        }
    }
    blocked.into_iter().fold(f32::INFINITY, f32::min)
}

fn port_facing_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("port_facing_penalty");
    let mut penalty = 0.0;
    for compiled in &context.relations {
        let relation = &compiled.relation;
        if crate::conditions::applies(relation) || relation.effect.as_ref() == "lock"
            || relation.kind.as_ref() == "net"
            || relation.effect.as_ref() == "score_only"
        {
            continue;
        }
        let (Some(from), Some(to)) = (
            endpoint_point(primitives, &compiled.from),
            endpoint_point(primitives, &compiled.to),
        ) else {
            continue;
        };
        if from.primitive_id == to.primitive_id {
            continue;
        }
        let facing = relation.kind.as_ref() == "critical_pair"
            || relation.kind.as_ref() == "island_target"
            || relation.relation.as_deref() == Some("very_near")
            || relation.relation.as_deref() == Some("cap_cluster");
        if facing {
            penalty += endpoint_facing_penalty(primitives, from, to.point)
                * relation_weight(relation)
                * 2.5;
            penalty += endpoint_facing_penalty(primitives, to, from.point)
                * relation_weight(relation)
                * 1.2;
        }
    }
    penalty
}

fn endpoint_facing_penalty(
    primitives: &[WorkingPrimitive],
    source: EndpointPoint,
    target: Point,
) -> f32 {
    let Some(id) = source.primitive_id else {
        return 0.0;
    };
    let Some(primitive) = primitives.iter().find(|primitive| primitive.id == id) else {
        return 0.0;
    };
    let center = box_center(&primitive.primitive.bbox);
    if distance(source.point, center) < 0.05 {
        return 0.0;
    }
    let port = normalize(Point {
        x: source.point.x - center.x,
        y: source.point.y - center.y,
    });
    let link = normalize(Point {
        x: target.x - center.x,
        y: target.y - center.y,
    });
    (1.0 - dot(port, link)).max(0.0)
}

fn endpoint_point(
    primitives: &[WorkingPrimitive],
    endpoint: &CompiledEndpoint,
) -> Option<EndpointPoint> {
    match endpoint {
        CompiledEndpoint::Anchor(point) => point.map(|point| EndpointPoint {
            point,
            primitive_id: None,
        }),
        CompiledEndpoint::Pad {
            primitive_id,
            point_index,
        } => {
            let primitive = primitives
                .iter()
                .find(|primitive| primitive.id == *primitive_id)?;
            let point = primitive.primitive.connection_points.get(*point_index)?;
            Some(EndpointPoint {
                point: Point {
                    x: point.x,
                    y: point.y,
                },
                primitive_id: Some(*primitive_id),
            })
        }
        CompiledEndpoint::Component { primitive_id, placement_index } => {
            let primitive=primitives.iter().find(|p|p.id==*primitive_id)?;
            let pose=primitive.primitive.placements.get(*placement_index)?;
            Some(EndpointPoint { point:Point{x:pose.x,y:pose.y},primitive_id:Some(*primitive_id) })
        }
        CompiledEndpoint::Primitive { primitive_id } => {
            let primitive = primitives
                .iter()
                .find(|primitive| primitive.id == *primitive_id)?;
            Some(EndpointPoint {
                point: box_center(&primitive.primitive.bbox),
                primitive_id: Some(*primitive_id),
            })
        }
        CompiledEndpoint::Missing => None,
    }
}

fn compile_relations(
    relations: &[Relation],
    primitives: &[WorkingPrimitive],
    bounds: Option<&Box2>,
    resolve_islands: bool,
) -> Vec<CompiledRelation> {
    relations
        .iter()
        .cloned()
        .map(|relation| CompiledRelation {
            from: compile_endpoint(&relation.from, primitives, bounds, resolve_islands),
            to: compile_endpoint(&relation.to, primitives, bounds, resolve_islands),
            relation,
        })
        .collect()
}

fn compile_endpoint(
    endpoint: &str,
    primitives: &[WorkingPrimitive],
    bounds: Option<&Box2>,
    resolve_islands: bool,
) -> CompiledEndpoint {
    if endpoint.starts_with("block:") || endpoint.starts_with("module:") {
        for primitive in primitives {
            if let Some(point_index) = primitive.primitive.connection_points.iter().position(|p| p.reference.as_ref() == endpoint) {
                return CompiledEndpoint::Pad { primitive_id: primitive.id, point_index };
            }
        }
    }
    if let Some(name) = endpoint.strip_prefix("component:") {
        for primitive in primitives {
            if let Some(placement_index) = primitive.primitive.placements.iter().position(|p|p.designator.as_ref()==name) {
                return CompiledEndpoint::Component {primitive_id:primitive.id,placement_index};
            }
        }
        return CompiledEndpoint::Missing;
    }
    if let Some(anchor) = endpoint.strip_prefix("anchor:") {
        return CompiledEndpoint::Anchor(anchor_point(anchor, bounds));
    }
    if let Some(reference) = endpoint.strip_prefix("pad:") {
        for primitive in primitives {
            if let Some(point_index) = primitive
                .primitive
                .connection_points
                .iter()
                .position(|point| point.reference.as_ref() == reference)
            {
                return CompiledEndpoint::Pad {
                    primitive_id: primitive.id,
                    point_index,
                };
            }
        }
        return CompiledEndpoint::Missing;
    }

    for (prefix, tree_prefix) in [("block:", "tree:block:"), ("module:", "tree:module:"), ("island:", "tree:island:")] {
        if prefix == "island:" && !resolve_islands { continue; }
        if let Some(name) = endpoint.strip_prefix(prefix) {
            let target = format!("{tree_prefix}{name}");
            return primitives
                .iter()
                .find(|primitive| {
                    primitive
                        .source_node_ids
                        .iter()
                        .any(|id| id.as_ref() == target)
                })
                .map(|primitive| CompiledEndpoint::Primitive {
                    primitive_id: primitive.id,
                })
                .unwrap_or(CompiledEndpoint::Missing);
        }
    }
    CompiledEndpoint::Missing
}

fn relation_target_point(mut point: EndpointPoint, relation: &Relation) -> EndpointPoint {
    if relation.satellite_anchor {
        if let Some(offset) = relation.anchor_offset {
            point.point.x = round_placement(point.point.x + offset.x);
            point.point.y = round_placement(point.point.y + offset.y);
        }
    }
    point
}

fn relation_weight(relation: &Relation) -> f32 {
    crate::relation_score::for_relation(relation).weight
}


fn relation_side_penalty(relation: &Relation, from: Point, to: Point, weight: f32) -> f32 {
    if !relation.satellite_anchor {
        return 0.0;
    }
    let desired = match relation.side_preference.as_deref() {
        Some("left") => Point { x: -1.0, y: 0.0 },
        Some("right") => Point { x: 1.0, y: 0.0 },
        Some("top") => Point { x: 0.0, y: -1.0 },
        Some("bottom") => Point { x: 0.0, y: 1.0 },
        _ => return 0.0,
    };
    let actual = normalize(Point {
        x: from.x - to.x,
        y: from.y - to.y,
    });
    (1.0 - dot(actual, desired)).max(0.0) * weight * 120.0
}

fn same_net_spread_penalties(
    primitives: &[WorkingPrimitive],
    context: &Context,
    ground_max_points: Option<usize>,
    ground_max_spread: Option<f32>,
) -> (f32, f32) {
    let _span = context.detail.span("same_net_spread_penalties");
    let mut scratch = context.net_scoring_scratch.borrow_mut();
    let NetScoringScratch {
        accumulators,
        signal_order,
        ground_order,
    } = &mut *scratch;
    accumulators.fill(NetAccumulator::default());
    signal_order.clear();
    ground_order.clear();
    for primitive in primitives {
        for (point_index, point) in primitive.primitive.connection_points.iter().enumerate() {
            let Some(net_id) = primitive.point_net_ids[point_index] else {
                continue;
            };
            let net_index = net_id as usize;
            let ground = context.net_ground[net_index];
            let accumulator = &mut accumulators[net_index];
            if !accumulator.seen {
                accumulator.seen = true;
                if ground {
                    ground_order.push(net_index);
                } else {
                    signal_order.push(net_index);
                }
            }
            accumulator.point_count += 1;
            if accumulator.last_primitive != primitive.id {
                accumulator.last_primitive = primitive.id;
                accumulator.primitive_count += 1;
            }
            accumulator.min_x = accumulator.min_x.min(point.x);
            accumulator.max_x = accumulator.max_x.max(point.x);
            accumulator.min_y = accumulator.min_y.min(point.y);
            accumulator.max_y = accumulator.max_y.max(point.y);
        }
    }
    let signal_penalty = spread_penalty(accumulators, signal_order, None, None, &context.net_weights);
    let ground_penalty = spread_penalty(
        accumulators,
        ground_order,
        ground_max_points,
        ground_max_spread,
        &context.net_weights,
    );
    (signal_penalty, ground_penalty)
}

fn spread_penalty(
    accumulators: &[NetAccumulator],
    order: &[usize],
    max_points: Option<usize>,
    max_spread: Option<f32>,
    weights: &[f32],
) -> f32 {
    let mut penalty = 0.0;
    for &net_index in order {
        let accumulator = accumulators[net_index];
        if accumulator.primitive_count < 2 {
            continue;
        }
        let spread = accumulator.max_x - accumulator.min_x + accumulator.max_y - accumulator.min_y;
        if max_points.is_some_and(|limit| accumulator.point_count > limit)
            || max_spread.is_some_and(|limit| spread > limit)
        {
            continue;
        }
        penalty += spread * weights[net_index];
    }
    penalty
}

fn dense_ic_access_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("dense_ic_access_penalty");
    if context.problem.search_width > 1 && !context.problem.experiments.keep_dense_access {
        return 0.0;
    }
    let components:Vec<_>=primitives.iter().flat_map(|p|p.components.iter().map(|(_,c)|c)).collect();
    let mut penalty=0.0;
    for a in &components {for b in &components {if a.designator!=b.designator && a.layer==b.layer {
        penalty+=crate::pad_access::cost(&a.access_rows,b.body_box);
    }}}
    penalty
}

struct PowerYieldFrame<'a> {
    baseline: f32,
    by_net: FxHashMap<Arc<str>, Vec<(u32, Point)>>,
    power: Vec<&'a WorkingPrimitive>,
    connection_count: usize,
}

impl<'a> PowerYieldFrame<'a> {
    fn new(placed: &'a [WorkingPrimitive], context: &Context) -> Option<Self> {
        let _span = context.detail.span("power_frame_build");
        let connection_count: usize = placed.iter().map(|p| crate::conditions::connection_count(&p.primitive)).sum();
        // The full scorer changes policy at these sizes; preserve that boundary.
        if placed.len() < 3 || placed.len() >= 36 || connection_count > 240 { return None; }
        let power: Vec<_> = placed.iter().filter(|p| power_affinity(p) >= 0.55 && !p.primitive.locked).collect();
        let baseline = if power.is_empty() { 0.0 } else { power_yield_penalty(placed, context) };
        let mut by_net: FxHashMap<Arc<str>, Vec<(u32, Point)>> = FxHashMap::default();
        for p in placed {
            if power_affinity(p) >= 0.75 { continue; }
            for (index, point) in p.primitive.connection_points.iter().enumerate() {
                let Some(net) = &point.net else { continue };
                if !context.net_signal[p.point_net_ids[index].expect("named net") as usize] { continue; }
                let points = by_net.entry(net.clone()).or_default();
                if points.len() < 8 { points.push((p.id, Point { x: point.x, y: point.y })); }
            }
        }
        Some(Self { baseline, by_net, power, connection_count })
    }

    fn with_candidate(&self, candidate: &WorkingPrimitive, context: &Context) -> f32 {
        let _span = context.detail.span("power_frame_append");
        if self.connection_count + crate::conditions::connection_count(&candidate.primitive) > 240 { return 0.0; }
        let mut score = self.baseline;
        let affinity = power_affinity(candidate);
        if affinity >= 0.55 && !candidate.primitive.locked {
            for (net, points) in &self.by_net {
                for i in 0..points.len() {
                    for j in (i + 1)..points.len() {
                        if points[i].0 != points[j].0 {
                            score += power_yield_segment(net, points[i].1, points[j].1, candidate, context);
                        }
                    }
                }
            }
        }
        if affinity < 0.75 {
            let mut new_by_net: FxHashMap<Arc<str>, Vec<Point>> = FxHashMap::default();
            for (index, point) in candidate.primitive.connection_points.iter().enumerate() {
                let Some(net) = &point.net else { continue };
                if !context.net_signal[candidate.point_net_ids[index].expect("named net") as usize] { continue; }
                let old = self.by_net.get(net).map_or(0, Vec::len);
                let points = new_by_net.entry(net.clone()).or_default();
                if old + points.len() < 8 { points.push(Point { x: point.x, y: point.y }); }
            }
            for (net, new_points) in new_by_net {
                let Some(old_points) = self.by_net.get(&net) else { continue };
                for new_point in new_points {
                    for &(old_id, old_point) in old_points {
                        if old_id == candidate.id { continue; }
                        for power in &self.power {
                            if power.id != old_id && power.id != candidate.id {
                                score += power_yield_segment(&net, old_point, new_point, power, context);
                            }
                        }
                    }
                }
            }
        }
        score
    }
}

fn power_affinity(primitive: &WorkingPrimitive) -> f32 {
    if primitive.components.is_empty() { return 0.0; }
    primitive.components.iter().filter(|(_, c)| c.power_component).count() as f32
        / primitive.components.len() as f32
}

fn power_yield_segment(net: &str, a: Point, b: Point, power: &WorkingPrimitive, context: &Context) -> f32 {
    let direct = distance(a, b);
    if !(0.5..=45.0).contains(&direct) { return 0.0; }
    let corridor = 1.2f32.max(context.problem.clearance * 1.75);
    let box_ = &power.primitive.bbox;
    let outside = a.x.max(b.x) < box_.left - corridor || a.x.min(b.x) > box_.right + corridor
        || a.y.max(b.y) < box_.top - corridor || a.y.min(b.y) > box_.bottom + corridor;
    let d = if outside { corridor } else { segment_box_distance(a, b, box_) };
    let depth = (corridor - d).max(0.0);
    (depth * depth * 220.0 + depth * 80.0) * power_affinity(power) * signal_net_weight(net)
}

fn power_yield_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("power_yield_penalty");
    if primitives.len() < 3 || primitives.len() > 36 {
        return 0.0;
    }
    let connection_count: usize = primitives
        .iter()
        .map(|primitive| crate::conditions::connection_count(&primitive.primitive))
        .sum();
    if connection_count > 240 {
        return 0.0;
    }
    let affinity = |primitive: &WorkingPrimitive| {
        if primitive.components.is_empty() {
            0.0
        } else {
            primitive
                .components
                .iter()
                .filter(|(_, component)| component.power_component)
                .count() as f32
                / primitive.components.len() as f32
        }
    };
    let power: Vec<_> = primitives
        .iter()
        .filter(|primitive| affinity(primitive) >= 0.55 && !primitive.primitive.locked)
        .collect();
    if power.is_empty() {
        return 0.0;
    }
    let mut by_net: FxHashMap<Arc<str>, Vec<(u32, Point)>> = FxHashMap::default();
    for primitive in primitives {
        if affinity(primitive) >= 0.75 {
            continue;
        }
        for (point_index, point) in primitive.primitive.connection_points.iter().enumerate() {
            let Some(net) = &point.net else {
                continue;
            };
            if !context.net_signal[primitive.point_net_ids[point_index].expect("named net") as usize] {
                continue;
            }
            by_net.entry(net.clone()).or_default().push((
                primitive.id,
                Point {
                    x: point.x,
                    y: point.y,
                },
            ));
        }
    }
    let corridor = 1.2f32.max(context.problem.clearance * 1.75);
    let mut cache = context.corridor_cache.borrow_mut();
    let mut penalty = 0.0;
    for (net, points) in by_net {
        let limited = &points[..points.len().min(8)];
        for i in 0..limited.len() {
            for j in (i + 1)..limited.len() {
                if limited[i].0 == limited[j].0 {
                    continue;
                }
                let direct = distance(limited[i].1, limited[j].1);
                if !(0.5..=45.0).contains(&direct) {
                    continue;
                }
                for primitive in &power {
                    if primitive.id == limited[i].0 || primitive.id == limited[j].0 {
                        continue;
                    }
                    let a = limited[i].1; let b = limited[j].1; let box_ = &primitive.primitive.bbox;
                    let pose = [a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits(),
                        box_.left.to_bits(), box_.right.to_bits(), box_.top.to_bits(), box_.bottom.to_bits()];
                    let key = (net.clone(), i, j, primitive.source_index);
                    let value = if let Some((_, value)) = cache.get(&key).filter(|(old, _)| *old == pose) { *value }
                    else {
                        let outside = a.x.max(b.x) < box_.left - corridor || a.x.min(b.x) > box_.right + corridor
                            || a.y.max(b.y) < box_.top - corridor || a.y.min(b.y) > box_.bottom + corridor;
                        let d = if outside { corridor } else { segment_box_distance(a, b, box_) };
                        let depth = (corridor - d).max(0.0);
                        let value = (depth * depth * 220.0 + depth * 80.0) * affinity(primitive) * signal_net_weight(&net);
                        cache.insert(key, (pose, value)); value
                    };
                    if context.validate_incremental_scoring {
                        let depth = (corridor - segment_box_distance(a, b, box_)).max(0.0);
                        let full = (depth * depth * 220.0 + depth * 80.0) * affinity(primitive) * signal_net_weight(&net);
                        assert!((full-value).abs() < 1e-8, "corridor cache mismatch");
                    }
                    penalty += value;
                }
            }
        }
    }
    penalty
}

fn board_fallback_candidates(
    variants: &[WorkingPrimitive],
    placed: &[WorkingPrimitive],
    primary: &[WorkingPrimitive],
    context: &Context,
) -> Vec<WorkingPrimitive> {
    let Some(bounds) = context.problem.bounds else {
        return Vec::new();
    };
    let preferred: Vec<_> = primary
        .iter()
        .map(|candidate| box_center(&candidate.primitive.bbox))
        .collect();
    let mut candidates = Vec::new();
    let max_per_variant = (256 / variants.len().max(1)).max(32);
    let step = context.problem.grid.max(0.25);
    for variant in variants {
        let min_x = snap_up(bounds.left + variant.primitive.width / 2.0, step);
        let max_x = snap_down(bounds.right - variant.primitive.width / 2.0, step);
        let min_y = snap_up(bounds.top + variant.primitive.height / 2.0, step);
        let max_y = snap_down(bounds.bottom - variant.primitive.height / 2.0, step);
        if min_x > max_x || min_y > max_y {
            continue;
        }
        let mut centers = Vec::new();
        let mut y = min_y;
        while y <= max_y + 0.0001 {
            let mut x = min_x;
            while x <= max_x + 0.0001 {
                centers.push(Point {
                    x: round_placement(x),
                    y: round_placement(y),
                });
                x += step;
            }
            y += step;
        }
        centers.sort_by(|a, b| {
            compare_f32(
                preferred_distance_squared(*a, &preferred),
                preferred_distance_squared(*b, &preferred),
            )
        });
        let mut accepted = 0;
        for center in centers {
            let candidate = move_primitive_center(variant.clone(), center, context.problem.grid);
            if candidate_hard_violation_count(&candidate, placed, context) > 0 {
                continue;
            }
            candidates.push(candidate);
            accepted += 1;
            if accepted >= max_per_variant || candidates.len() >= 256 {
                break;
            }
        }
    }
    candidates
}

// Select a common electrical frontier across beam states. Comparing different
// subsets by partial wire cost rewards postponing the IC and its missing nets.
fn frontier_indices(remaining: &[WorkingPrimitive], placed: &[WorkingPrimitive], context: &Context) -> Vec<usize> {
    let core_size = |p: &WorkingPrimitive| p.components.iter()
        .filter(|(_, c)| c.role.as_deref() == Some("main_ic"))
        .map(|(_, c)| c.pin_count).max().unwrap_or(0);
    if !context.problem.experiments.frontier_order
        || !remaining.iter().chain(placed).any(|p| core_size(p) > 0) {
        return (0..remaining.len()).collect();
    }
    if !placed.iter().any(|p| core_size(p) > 0) {
        let index = remaining.iter().enumerate().max_by(|(_, a), (_, b)|
            core_size(a).cmp(&core_size(b)).then_with(|| b.id.cmp(&a.id))).map(|(i, _)| i);
        return index.into_iter().collect();
    }
    let nets = |p: &WorkingPrimitive| -> FxHashSet<Arc<str>> {
        p.primitive.connection_points.iter().filter_map(|cp| cp.net.clone())
            .filter(|n| !n.is_empty() && !is_ground(n)
                && !context.problem.experiments.ignored_nets.iter().any(|v| v.eq_ignore_ascii_case(n)))
            .collect()
    };
    let mut fanout: FxHashMap<Arc<str>, usize> = FxHashMap::default();
    for p in remaining.iter().chain(placed) { for net in nets(p) { *fanout.entry(net).or_default() += 1; } }
    #[cfg(feature="gpu")]
    let gpu_scarcity=if context.problem.experiments.order_scarcity && context.gpu_engine.borrow().is_some() {
        Some(cubecl::scarcity(remaining,placed,context))
    } else {None};
    #[cfg(not(feature="gpu"))]
    let gpu_scarcity:Option<Vec<f32>>=None;
    let score = |p: &WorkingPrimitive| {
        let own = nets(p);
        let mut value = 0.0;
        for other in placed {
            for net in own.intersection(&nets(other)) {
                let affinity = if context.problem.experiments.order_core_affinity {
                    if core_size(other) > 0 { 4.0 } else { 0.25 }
                } else { 1.0 };
                value += affinity * 10.0 / (fanout[net].saturating_sub(1).max(1) as f32);
            }
            for r in &context.problem.relations {
                if r.kind.as_ref() == "critical_pair" &&
                    ((primitive_touches_endpoint(p, &r.from) && primitive_touches_endpoint(other, &r.to))
                    || (primitive_touches_endpoint(p, &r.to) && primitive_touches_endpoint(other, &r.from))) {
                    let bonus = if context.problem.experiments.order_equal_critical { 30.0 }
                        else if r.hard { 100.0 } else { 30.0 };
                    value += bonus * if context.problem.experiments.order_core_affinity && core_size(other) == 0 { 0.25 } else { 1.0 };
                }
            }
        }
        if context.problem.experiments.order_scarcity {
            value += gpu_scarcity.as_ref().map(|scores|scores[p.source_index]).unwrap_or_else(||frontier_scarcity(p, placed, context));
        }
        value
    };
    let mut ranked: Vec<_> = remaining.iter().enumerate().map(|(i, p)| (i, score(p))).collect();
    ranked.sort_by(|(ai, a), (bi, b)| compare_f32(*b, *a)
        .then_with(|| core_size(&remaining[*bi]).cmp(&core_size(&remaining[*ai])))
        .then_with(|| remaining[*ai].id.cmp(&remaining[*bi].id)));
    ranked.into_iter().take(if context.problem.experiments.order_branching { 3 } else { 1 })
        .map(|(i, _)| i).collect()
}

// Count geometric opportunities close to the IC without routing or scoring.
// Normalize by the number of generated nearby poses to avoid rewarding large
// candidate lists. This is a heuristic, not an exhaustive free-space measure.
fn frontier_scarcity(p: &WorkingPrimitive, placed: &[WorkingPrimitive], context: &Context) -> f32 {
    let _span = context.detail.span("frontier_scarcity");
    let cores: Vec<_> = placed.iter().filter(|q| q.components.iter()
        .any(|(_, c)| c.role.as_deref() == Some("main_ic"))).cloned().collect();
    let distance = |q: &WorkingPrimitive| {
        let mut sum = 0.0;
        let mut count = 0usize;
        for cp in q.primitive.connection_points.iter() {
            let Some(net) = &cp.net else { continue };
            if net.is_empty() || is_ground(net) || context.problem.experiments.ignored_nets.iter().any(|n| n.eq_ignore_ascii_case(net)) { continue; }
            let nearest = cores.iter().flat_map(|c| c.primitive.connection_points.iter())
                .filter(|t| t.net.as_ref() == Some(net))
                .map(|t| crate::numerics::hypot(cp.x - t.x,cp.y - t.y)).fold(f32::INFINITY, f32::min);
            if nearest.is_finite() { sum += nearest; count += 1; }
        }
        (sum, count)
    };
    let pins = distance(p).1;
    if pins == 0 { return 0.0; }
    let candidates: Vec<_> = orientation_variants(p).iter()
        .flat_map(|v| net_anchored_candidates(v, &cores, context)).collect();
    let candidates = dedupe_primitives(candidates);
    let best = candidates.iter().filter(|q| candidate_hard_violation_count(q, &cores, context) == 0)
        .map(|q| distance(q).0).fold(f32::INFINITY, f32::min);
    if !best.is_finite() { return 0.0; }
    let nearby: Vec<_> = candidates.iter().filter(|q| distance(q).0 <= best + pins as f32
        && candidate_hard_violation_count(q, &cores, context) == 0).collect();
    let legal = nearby.iter().filter(|q| candidate_hard_violation_count(q, placed, context) == 0).count();
    let scarcity = if legal > 0 { 60.0 * (1.0 - legal as f32 / nearby.len().max(1) as f32) } else { 0.0 };
    scarcity + 40.0 * pins.saturating_sub(1).min(2) as f32
}

fn seed_rank(primitive: &WorkingPrimitive, context: &Context) -> f32 {
    let degree = context
        .problem
        .relations
        .iter()
        .filter(|relation| {
            primitive_touches_endpoint(primitive, &relation.from)
                || primitive_touches_endpoint(primitive, &relation.to)
        })
        .count();
    primitive.primitive.width * primitive.primitive.height
        + degree as f32 * 10.0
        + if primitive.primitive.label.as_ref() == "core"
            || primitive.primitive.label.contains("main")
        {
            20.0
        } else {
            0.0
        }
}

fn primitive_touches_endpoint(primitive: &WorkingPrimitive, endpoint: &str) -> bool {
    if endpoint.starts_with("block:") || endpoint.starts_with("module:") {
        return primitive.primitive.connection_points.iter().any(|p| p.reference.as_ref() == endpoint);
    }
    if let Some(reference) = endpoint.strip_prefix("pad:") {
        return primitive
            .primitive
            .connection_points
            .iter()
            .any(|point| point.reference.as_ref() == reference);
    }
    if let Some(designator) = endpoint.strip_prefix("component:") {
        return primitive
            .primitive
            .placements
            .iter()
            .any(|placement| placement.designator.as_ref() == designator);
    }
    false
}

fn dedupe_states(states: Vec<SearchState>) -> Vec<SearchState> {
    let mut seen = FxHashSet::default();
    states
        .into_iter()
        .filter(|state| {
            let mut remaining: Vec<_> = state
                .remaining
                .iter()
                .map(|primitive| primitive.id)
                .collect();
            remaining.sort();
            let mut placed: Vec<_> = state.placed.iter().map(primitive_pose_key).collect();
            placed.sort();
            seen.insert(SearchStateKey { remaining, placed })
        })
        .collect()
}

fn number_key(value: f32) -> u32 {
    if value == 0.0 {
        0
    } else {
        value.to_bits()
    }
}

fn compare_candidates(a: &RankedCandidate, b: &RankedCandidate) -> Ordering {
    a.hard_violations
        .cmp(&b.hard_violations)
        .then_with(|| compare_f32(a.score, b.score))
        .then_with(|| a.ordinal.cmp(&b.ordinal))
}

fn compare_states(a: &SearchState, b: &SearchState) -> Ordering {
    a.hard_violations
        .cmp(&b.hard_violations)
        .then_with(|| compare_f32(a.score, b.score))
        .then_with(|| a.ordinal.cmp(&b.ordinal))
}

fn lexical_ids(values: impl IntoIterator<Item = Arc<str>>) -> FxHashMap<Arc<str>, u32> {
    let mut values: Vec<_> = values.into_iter().collect();
    values.sort();
    values.dedup();
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| (value, index as u32))
        .collect()
}

fn compare_f32(a: f32, b: f32) -> Ordering {
    a.total_cmp(&b)
}
fn div_ceil(value: usize, divisor: usize) -> usize {
    (value + divisor - 1) / divisor
}
fn snap(value: f32, grid: f32) -> f32 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid + 0.5).floor() * grid
    }
}
fn snap_up(value: f32, grid: f32) -> f32 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid).ceil() * grid
    }
}
fn snap_down(value: f32, grid: f32) -> f32 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid).floor() * grid
    }
}
fn distance(a: Point, b: Point) -> f32 {
    crate::numerics::hypot(a.x - b.x,a.y - b.y)
}
fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y
}
fn normalize(point: Point) -> Point {
    let length = crate::numerics::hypot(point.x,point.y);
    if length > 0.000001 {
        Point {
            x: point.x / length,
            y: point.y / length,
        }
    } else {
        Point { x: 0.0, y: 0.0 }
    }
}
fn point_box(point: Point) -> Box2 {
    Box2 {
        left: point.x,
        right: point.x,
        top: point.y,
        bottom: point.y,
    }
}
fn dedupe_numbers(values: Vec<f32>) -> Vec<f32> {
    let mut seen = FxHashSet::default();
    values
        .into_iter()
        .filter(|value| value.is_finite() && seen.insert(round_placement(*value).to_bits()))
        .collect()
}
fn preferred_distance_squared(point: Point, preferred: &[Point]) -> f32 {
    if preferred.is_empty() {
        point.x * point.x + point.y * point.y
    } else {
        preferred
            .iter()
            .map(|target| (point.x - target.x).powi(2) + (point.y - target.y).powi(2))
            .fold(f32::INFINITY, f32::min)
    }
}

fn anchor_point(anchor: &str, bounds: Option<&Box2>) -> Option<Point> {
    let bounds = bounds?;
    let x = if anchor.ends_with(".left")
        || anchor.ends_with(".top_left")
        || anchor.ends_with(".bottom_left")
    {
        bounds.left
    } else if anchor.ends_with(".right")
        || anchor.ends_with(".top_right")
        || anchor.ends_with(".bottom_right")
    {
        bounds.right
    } else {
        (bounds.left + bounds.right) / 2.0
    };
    let y = if anchor.ends_with(".top")
        || anchor.ends_with(".top_left")
        || anchor.ends_with(".top_right")
    {
        bounds.top
    } else if anchor.ends_with(".bottom")
        || anchor.ends_with(".bottom_left")
        || anchor.ends_with(".bottom_right")
    {
        bounds.bottom
    } else {
        (bounds.top + bounds.bottom) / 2.0
    };
    Some(Point {
        x: round_placement(x),
        y: round_placement(y),
    })
}

fn signal_net_weight(net: &str) -> f32 {
    let value = net.to_ascii_uppercase();
    if [
        "USB", "D+", "D-", "DP", "DM", "QSPI", "SPI", "I2C", "SDA", "SCL", "XIN", "XOUT", "CLK",
        "MISO", "MOSI", "SCLK", "CS",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
    {
        1.5
    } else {
        1.0
    }
}

fn segment_box_distance(a: Point, b: Point, box_: &Box2) -> f32 {
    if segment_intersects_box(a, b, box_) {
        return 0.0;
    }
    let corners = [
        Point {
            x: box_.left,
            y: box_.top,
        },
        Point {
            x: box_.right,
            y: box_.top,
        },
        Point {
            x: box_.right,
            y: box_.bottom,
        },
        Point {
            x: box_.left,
            y: box_.bottom,
        },
    ];
    let projected = [
        Point {
            x: box_.left,
            y: a.y.clamp(box_.top, box_.bottom),
        },
        Point {
            x: box_.right,
            y: a.y.clamp(box_.top, box_.bottom),
        },
        Point {
            x: a.x.clamp(box_.left, box_.right),
            y: box_.top,
        },
        Point {
            x: a.x.clamp(box_.left, box_.right),
            y: box_.bottom,
        },
    ];
    let mut best = point_to_box_distance(a, box_).min(point_to_box_distance(b, box_));
    for point in projected {
        best = best.min(point_to_segment_distance(point, a, b));
    }
    for corner in corners {
        best = best.min(point_to_segment_distance(corner, a, b));
    }
    best
}
pub(crate) fn segment_intersects_box(a: Point, b: Point, box_: &Box2) -> bool {
    crate::geometry_policy::hits_box(a.x,a.y,b.x,b.y,box_.left,box_.right,box_.top,box_.bottom)
}



fn point_to_segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= 0.000001 {
        return distance(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    distance(
        p,
        Point {
            x: a.x + t * dx,
            y: a.y + t * dy,
        },
    )
}
fn point_to_box_distance(p: Point, b: &Box2) -> f32 {
    let dx = if p.x < b.left {
        b.left - p.x
    } else if p.x > b.right {
        p.x - b.right
    } else {
        0.0
    };
    let dy = if p.y < b.top {
        b.top - p.y
    } else if p.y > b.bottom {
        p.y - b.bottom
    } else {
        0.0
    };
    crate::numerics::hypot(dx,dy)
}

#[cfg(test)]
mod candidate_tests {
    use super::*;

    #[test]
    fn folded_group_point_rotates_in_block_solver() {
        let (_, _, context) = usb_fixture();
        let mut problem = context.problem;
        problem.primitives.truncate(1);
        let owner = &mut problem.primitives[0];
        owner.bbox = Box2 { left: -10., right: 10., top: -10., bottom: 10. };
        owner.source_node_ids = Arc::new(vec![]);
        let id = owner.id.clone();
        problem.components.retain(|c| c.primitive_id == id);
        problem.components.truncate(1);
        problem.components[0].block_name = "folded".into();
        problem.components[0].body_box = Box2 {left: 5., right: 7., top: -1., bottom: 1.};
        problem.relations = vec![serde_json::from_value(serde_json::json!({
            "id":"near", "kind":"hint", "from":"block:folded", "to":"anchor:board.right",
            "relation":"near", "hard":false, "effect":"move_from", "satelliteAnchor":false
        })).unwrap()];
        problem.bounds = Some(Box2 {left:-20., right:20., top:-20., bottom:20.});
        attach_group_relation_points(&mut problem).unwrap();
        let count = problem.primitives[0].connection_points.len();
        attach_group_relation_points(&mut problem).unwrap();
        assert_eq!(count, problem.primitives[0].connection_points.len());
        let (mut ps, context) = prepare_context(problem, false);
        let endpoint = &context.relations[0].from;
        assert_eq!(endpoint_point(&ps, endpoint).unwrap().point.x, 6.);
        let before = scoped_relation_penalty(&ps, &context);
        ps[0] = rotate_primitive(&ps[0], 180);
        assert_eq!(endpoint_point(&ps, endpoint).unwrap().point.x, -6.);
        assert!(scoped_relation_penalty(&ps, &context) > before);
    }

    #[test]
    fn component_body_preserves_offset_on_rotation_and_translation() {
        let (mut primitive, _, _) = usb_fixture();
        let mut source = primitive.source_placements[0].clone();
        source.x = 10.0;
        source.y = -7.0;
        source.rotate = 0;
        primitive.source_placements = Arc::new(vec![source.clone()]);
        let mut component = primitive.source_components[0].clone();
        component.1.body_box = Box2 { left: 9.0, right: 13.0, top: -9.0, bottom: -6.0 };
        primitive.source_components = Arc::new(vec![component]);
        // Expected local bounds about the placement origin, not the body's center.
        for (angle, local) in [
            (0, [-1.0, 3.0, -2.0, 1.0]),
            (90, [-1.0, 2.0, -1.0, 3.0]),
            (180, [-3.0, 1.0, -1.0, 2.0]),
            (270, [-2.0, 1.0, -3.0, 1.0]),
        ] {
            let mut target = source.clone();
            target.x = -4.0;
            target.y = 6.0;
            target.rotate = angle;
            primitive.primitive.placements = Arc::new(vec![target]);
            rebuild_component_geometry(&mut primitive);
            assert_eq!(primitive.components[0].1.body_box, Box2 {
                left: -4.0 + local[0], right: -4.0 + local[1],
                top: 6.0 + local[2], bottom: 6.0 + local[3],
            });
        }
    }

    #[test]
    fn asymmetric_crystal_body_does_not_hide_clearance_violation() {
        let (mut primitive, _, _) = usb_fixture();
        let mut source = primitive.source_placements[0].clone();
        source.x = 0.0;
        source.y = 0.0;
        source.rotate = 0;
        primitive.source_placements = Arc::new(vec![source.clone()]);
        let mut component = primitive.source_components[0].clone();
        component.1.body_box = Box2 { left: -2.413, right: 2.413, top: -1.817, bottom: 1.7392 };
        primitive.source_components = Arc::new(vec![component]);
        source.x = -0.227;
        source.y = -4.745;
        source.rotate = 180;
        primitive.primitive.placements = Arc::new(vec![source]);
        rebuild_component_geometry(&mut primitive);
        let crystal = primitive.components[0].1.body_box;
        let mcu = Box2 { left: -1.5, right: 4.3025, top: -1.8315, bottom: 3.7735 };
        let depth = overlap_depth(&mcu, &crystal, 1.125);
        assert!((depth - 0.0285).abs() < 0.001, "missed U1-X1 clearance: {depth}");
    }

    /// H743 `usb_cc`, captured from a real run: the fixed connector USB1 never
    /// moves and arrives with off-grid pad access rows (-3.4999 instead of -3.5).
    /// Loading must give it the rows that a move to its authored pose gives (and
    /// that the GPU uploads) and keep that pose. Moves still rebuild the rows from
    /// the authored input: a half tick rounded before a rotation can land on the
    /// other tick (ELRS U1, 2.8025 at 270 degrees), so a second row adds one.
    #[test]
    fn unmoved_access_rows_match_moved_geometry_and_keep_fixed_pose() {
        let mut raw: BlockSolveProblem = serde_json::from_str(&std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/h743-usb-cc-bounded-block.json")
        ).unwrap()).unwrap();
        let usb_index = raw.components.iter().position(|c| &*c.designator == "USB1").unwrap();
        let mut half_tick = raw.components[usb_index].access_rows[0].clone();
        half_tick.box_ = Box2 { left: -14.9375, right: -14.4375, top: -0.5625, bottom: 0.0625 };
        raw.components[usb_index].access_rows.push(half_tick);
        let (primitives, _) = prepare_context(raw.clone(), false);
        let usb = primitives.iter().find(|p| p.primitive.locked).unwrap();
        let authored = &raw.primitives.iter().find(|p| p.locked).unwrap().placements[0];
        let placed = &usb.primitive.placements[0];
        assert_eq!((placed.designator.as_ref(), placed.x, placed.y, placed.rotate, placed.layer.as_ref()),
            (authored.designator.as_ref(), authored.x, authored.y, authored.rotate, authored.layer.as_ref()));
        let rows = |p: &WorkingPrimitive| p.components[0].1.access_rows.iter().map(|r| (r.box_, r.weight)).collect::<Vec<_>>();
        let authored_rows: Vec<_> = raw.components[usb_index].access_rows.iter().map(|r| (r.box_, r.weight)).collect();
        let weight = authored_rows[0].1;
        assert_eq!(rows(usb), vec![
            (Box2 { left: -14.357, right: -13.438, top: -3.5, bottom: 3.5 }, weight),
            (Box2 { left: -14.937, right: -14.437, top: -0.562, bottom: 0.063 }, weight),
        ]);
        let mut unmoved = usb.clone();
        rebuild_component_geometry(&mut unmoved);
        assert_eq!(rows(&unmoved), rows(usb));
        let source: Vec<_> = usb.source_components[0].1.access_rows.iter().map(|r| (r.box_, r.weight)).collect();
        assert_eq!(source, authored_rows);
        let origin = Point { x: authored.x, y: authored.y };
        let mut rounding_first_differs = false;
        for (x, y, rotate) in [(-18.19, 0.0, 270), (-17.0, 1.25, 270), (-16.5, -2.0, 0), (-15.25, 0.5, 90), (-19.0, 3.0, 180)] {
            let mut target = authored.clone();
            target.x = x;
            target.y = y;
            target.rotate = rotate;
            let mut moved = usb.clone();
            moved.primitive.placements = Arc::new(vec![target]);
            rebuild_component_geometry(&mut moved);
            let delta = crate::geometry::subtract_rotation(rotate, authored.rotate);
            let (dx, dy) = (round_placement(x - authored.x), round_placement(y - authored.y));
            let transform = |rows: &[(Box2, f32)]| rows.iter()
                .map(|(b, w)| (translate_box(&rotate_box(b, &origin, delta), dx, dy), *w)).collect::<Vec<_>>();
            assert_eq!(rows(&moved), transform(&authored_rows), "moved to ({x}, {y}, {rotate})");
            rounding_first_differs |= transform(&rows(usb)) != transform(&authored_rows);
        }
        assert!(rounding_first_differs);
    }

    fn usb_fixture() -> (WorkingPrimitive, WorkingPrimitive, Context) {
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/block-placement/Telemetry/block-23.json")
        ).unwrap()).unwrap();
        let problem: BlockSolveProblem = serde_json::from_value(json["problem"].clone()).unwrap();
        let make = |name: &str| {
            let (index, p) = problem.primitives.iter().enumerate()
                .find(|(_, p)| p.placements.iter().any(|q| q.designator.as_ref() == name)).unwrap();
            let components = Arc::new(problem.components.iter().cloned().enumerate()
                .filter(|(_, c)| c.primitive_id == p.id).collect());
            WorkingPrimitive { id: index as u32, source_index: index, primitive: p.clone(),
                components: Arc::clone(&components), source_components: components,
                source_placements: p.placements.clone(), source_node_ids: Arc::new(FxHashSet::default()),
                point_net_ids: Arc::new(vec![]), rotation: 0 }
        };
        let resistor = make("R36");
        let ic = make("U11");
        let context = Context { conditions: vec![], trace: false, trace_phase: RefCell::new("test"), problem, relations: vec![], net_ground: vec![],
            #[cfg(feature = "gpu")] gpu_sources:vec![],
            #[cfg(feature = "gpu")] gpu_engine:Default::default(),
            corridor_cache: Default::default(), escape_cache: Default::default(),
            detail: Default::default(), source_pads: vec![], pad_geometry: Default::default(),
            net_endpoint_counts: Default::default(),
            net_signal: vec![], net_weights: vec![], pad_nets: Default::default(), pad_net_indices: vec![], pad_point_metadata: vec![],
            evaluation_cache: RefCell::new(FxHashMap::default()),
            net_scoring_scratch: RefCell::new(NetScoringScratch {
                accumulators: vec![], signal_order: vec![], ground_order: vec![] }),
            validate_incremental_scoring: false, pair_threads: 1 };
        (resistor, ic, context)
    }

    #[test]
    fn usb_pad_candidates_respect_dense_ic_clearance_and_expand_outward() {
        let (resistor, ic, mut context) = usb_fixture();
        let old = net_anchored_candidates(&resistor, &[ic.clone()], &context);
        let left = |p: &WorkingPrimitive| p.primitive.bbox.right < ic.primitive.bbox.left
            && box_center(&p.primitive.bbox).y >= ic.primitive.bbox.top
            && box_center(&p.primitive.bbox).y <= ic.primitive.bbox.bottom;
        assert!(old.iter().any(left));
        assert!(old.iter().filter(|p| left(p)).all(|p| component_overlap_depth(p, &ic, &context) > 0.0));
        context.problem.experiments.candidate_clearance = true;
        let corrected = net_anchored_candidates(&resistor, &[ic.clone()], &context);
        assert!(corrected.iter().any(|p| left(p) && component_overlap_depth(p, &ic, &context) == 0.0));
        context.problem.experiments.candidate_rings = true;
        let expanded = net_anchored_candidates(&resistor, &[ic.clone()], &context);
        assert!(expanded.len() > corrected.len());
        assert!(expanded.iter().any(|p| left(p) && ic.primitive.bbox.left - p.primitive.bbox.right > 1.5));
    }

    #[test]
    fn pad_owner_candidates_reach_legal_space_inside_compound_envelope() {
        let (resistor, mut ic, mut context) = usb_fixture();
        context.problem.experiments.candidate_clearance = true;
        ic.primitive.bbox.left -= 10.0;
        ic.primitive.bbox.right += 10.0;
        ic.primitive.bbox.top -= 10.0;
        ic.primitive.bbox.bottom += 10.0;
        let inside = |p: &WorkingPrimitive| p.primitive.bbox.left > ic.primitive.bbox.left
            && p.primitive.bbox.right < ic.primitive.bbox.right
            && p.primitive.bbox.top > ic.primitive.bbox.top
            && p.primitive.bbox.bottom < ic.primitive.bbox.bottom;
        assert!(!net_anchored_candidates(&resistor, &[ic.clone()], &context).iter().any(inside));
        context.problem.experiments.pad_owner_candidates = true;
        assert!(net_anchored_candidates(&resistor, &[ic.clone()], &context).iter()
            .any(|p| inside(p) && component_overlap_depth(p, &ic, &context) == 0.0));
    }

    #[test]
    fn frontier_seeds_the_ic_and_preserves_legacy_fallback_without_an_ic() {
        let (resistor, ic, mut context) = usb_fixture();
        context.problem.experiments.frontier_order = true;
        assert_eq!(frontier_indices(&[resistor.clone(), ic.clone()], &[], &context), vec![1]);
        assert_eq!(frontier_indices(&[resistor.clone()], &[ic], &context), vec![0]);
        assert_eq!(frontier_indices(&[resistor.clone(), resistor], &[], &context), vec![0, 1]);
    }

    #[test]
    fn equal_critical_order_does_not_use_hardness_as_urgency() {
        let (mut a, ic, mut context) = usb_fixture();
        let mut b = a.clone(); a.id = 1; b.id = 2;
        for cp in Arc::make_mut(&mut b.primitive.connection_points) {
            cp.reference = Arc::from(format!("R99.{}", cp.reference.split('.').last().unwrap()));
        }
        context.problem.relations = vec![
            serde_json::from_value(serde_json::json!({"id":"a", "kind":"critical_pair", "from":"pad:R36.1", "to":"component:U11", "hard":false, "effect":"attract", "satelliteAnchor":false})).unwrap(),
            serde_json::from_value(serde_json::json!({"id":"b", "kind":"critical_pair", "from":"pad:R99.1", "to":"component:U11", "hard":true, "effect":"attract", "satelliteAnchor":false})).unwrap(),
        ];
        context.problem.experiments.frontier_order = true;
        assert_eq!(frontier_indices(&[a.clone(), b.clone()], &[ic.clone()], &context), vec![1]);
        context.problem.experiments.order_equal_critical = true;
        assert_eq!(frontier_indices(&[a.clone(), b.clone()], &[ic.clone()], &context), vec![0]);
        assert!(context.problem.relations[1].hard); // Constraints are not relaxed.
        context.problem.experiments.order_branching = true;
        assert_eq!(frontier_indices(&[a, b], &[ic], &context), vec![0, 1]);
    }

    #[test]
    fn scarcity_recognizes_two_ic_links_but_excludes_ignored_nets() {
        let (resistor, ic, mut context) = usb_fixture();
        context.problem.experiments.candidate_clearance = true;
        assert!(frontier_scarcity(&resistor, &[ic.clone()], &context) >= 40.0);
        context.problem.experiments.ignored_nets = resistor.primitive.connection_points.iter()
            .filter_map(|cp| cp.net.clone()).collect();
        assert_eq!(frontier_scarcity(&resistor, &[ic], &context), 0.0);
    }

    #[test]
    fn island_endpoint_is_internal_when_its_rigid_primitive_is_present() {
        let (_, mut ic, _) = usb_fixture();
        ic.source_node_ids = Arc::new([Arc::<str>::from("tree:island:bank")].into_iter().collect());
        assert!(matches!(compile_endpoint("island:bank", &[ic.clone()], None, false), CompiledEndpoint::Missing));
        assert!(matches!(compile_endpoint("island:bank", &[ic], None, true), CompiledEndpoint::Primitive { .. }));
    }

    #[test]
    fn open_escape_does_not_depend_on_the_whole_block_envelope() {
        let (resistor, ic, _) = usb_fixture();
        let source = EndpointPoint {point: box_center(&ic.primitive.bbox), primitive_id:Some(ic.id)};
        let far = translate_primitive(&resistor, 100.0, 100.0);
        assert_eq!(escape_blockage(source, &[ic.clone(), far], 0.35), 0.0);
        let far = translate_primitive(&resistor, 200.0, 200.0);
        assert_eq!(escape_blockage(source, &[ic, far], 0.35), 0.0);
    }

    #[test]
    fn shortlist_bounds_preserve_diversity_and_hard_rank() {
        for bucket_count in [3, 7, 16] {
        let mut global = ScoreWindow::new(64);
        let mut buckets: Vec<_> = (0..16).map(|_| ScoreWindow::new(4)).collect();
        let mut all = Vec::new(); let mut retained = Vec::new();
        for id in 0..4000usize {
            let bucket = (id * 37 + id / 17) % bucket_count;
            let hard = (id * 19 + id / 11) % 3;
            let score = ((id * 7919) % 997) as f32;
            let row = (hard, score, id, bucket); all.push(row);
            if score > global.ceiling(hard).max(buckets[bucket].ceiling(hard)) { continue; }
            global.push(hard, score); buckets[bucket].push(hard, score); retained.push(row);
        }
        let select = |mut rows: Vec<(usize, f32, usize, usize)>| {
            rows.sort_by(|a,b| a.0.cmp(&b.0).then_with(|| compare_f32(a.1,b.1)).then(a.2.cmp(&b.2)));
            let mut counts = [0usize;16]; let mut selected = Vec::new(); let mut rest = Vec::new();
            for row in rows { if counts[row.3] < 4 { counts[row.3] += 1; selected.push(row.2); } else { rest.push(row.2); } }
            selected.extend(rest.into_iter().take(64usize.saturating_sub(selected.len()))); selected.sort(); selected
        };
        assert!(retained.len() < all.len()/2);
        assert_eq!(select(all), select(retained));
        }
    }

    #[test]
    fn exact_score_prefix_prunes_even_with_unbounded_later_cache_error() {
        let _env = crate::float_env::Guard::enter();
        let (mut resistor, mut ic, context) = usb_fixture();
        for p in [&mut resistor, &mut ic] {
            p.primitive.connection_points = Arc::new(vec![]);
            p.primitive.path_ports = Arc::new(vec![]);
        }
        let primitives = [resistor, ic];
        let cached = LongNetFrame { baseline: 0.0, open: Default::default() };
        assert!(score_block_bounded(&primitives, &context, None, 0.0,
            f32::INFINITY, None, Some(&cached)).is_none());
        let full = score_block_with_overlap_matrix(&primitives, &context, None);
        assert!(full > 0.0 && full.is_finite());
        assert_eq!(score_block_bounded(&primitives, &context, None, f32::INFINITY,
            f32::INFINITY, None, Some(&cached)), Some(full));
        // Compare pruning with complete evaluation over different shapes and
        // score thresholds, including equality and adjacent F32 values.
        for step in -10..=10 {
            let changed = [translate_primitive(&primitives[0], step as f32, 0.0), primitives[1].clone()];
            let full = score_block_with_overlap_matrix(&changed, &context, None);
            for ceiling in [0.0, full.next_down(), full, full.next_up()] {
                let bounded = score_block_bounded(&changed, &context, None, ceiling,
                    f32::INFINITY, None, Some(&cached));
                if full <= ceiling { assert_eq!(bounded, Some(full)); }
                if bounded.is_none() { assert!(full > ceiling); }
            }
        }
    }

    #[test]
    fn escape_cache_tracks_moved_sources_and_foreign_obstacles() {
        let (resistor, ic, mut context) = usb_fixture();
        context.validate_incremental_scoring = true;
        for step in 0..20 {
            let moved = translate_primitive(&resistor, step as f32 * 0.7 - 5.0, 0.0);
            let source = EndpointPoint { point: Point { x: step as f32 * 0.2, y: 0.0 }, primitive_id: Some(ic.id) };
            let primitives = [ic.clone(), moved];
            for _ in 0..2 {
                let cached = escape_blockage_cached(source, &primitives, 0, &context);
                assert_eq!(cached, escape_blockage(source, &primitives, context.problem.clearance));
            }
        }
    }

    #[test]
    fn an_unplaced_internal_endpoint_is_not_an_external_escape_requirement() {
        let (resistor, ic, mut context) = usb_fixture();
        let mut r = context.problem.relations[0].clone();
        r.from = Arc::from(format!("component:{}", ic.primitive.placements[0].designator));
        r.to = Arc::from(format!("component:{}", resistor.primitive.placements[0].designator));
        r.kind = Arc::from("hint"); r.effect = Arc::from("move_from");
        context.relations = compile_relations(&[r], &[ic.clone(), resistor], None, true);
        assert!(external_port_exposure_penalty(&[ic.clone()], &context) > 0.0);
        context.problem.experiments.local_access = true;
        assert_eq!(external_port_exposure_penalty(&[ic], &context), 0.0);
    }
}

fn condition_evaluation(primitives:&[WorkingPrimitive],context:&Context)->crate::conditions::Evaluation {
    crate::conditions::evaluate(&context.conditions,|owner,index| {
        let p=primitives.iter().find(|p|p.id==owner)?;let q=p.primitive.connection_points.get(index)?;Some(Point{x:q.x,y:q.y})
    })
}
