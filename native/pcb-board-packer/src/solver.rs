mod channels;
mod profile;
#[cfg(feature = "gpu")]
mod compact;
#[cfg(feature = "gpu")]
mod cubecl;
#[cfg(feature = "gpu")]
mod gpu_kernels;
use crate::geometry::{
    box_center, box_corners, box_inside_polygon_board, box_outside_bounds_severity,
    boxes_overlap_depth, normalize_rotation, overlap_depth, point_in_polygon,
    point_to_polygon_distance, rotate_box, rotate_point, round_placement, translate_box,
    union_boxes, Box2, Point,
};
use crate::model::{
    BoardPackProblem, BoardPackSolution, ComponentGeometry, Primitive, PrimitiveState, Rank,
    Relation,
};
use crate::signal_path;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use rustc_hash::{FxHashMap,FxHashSet}; 

#[derive(Clone)]
struct WorkingPrimitive {
    id: u32,
    source_index: usize,
    primitive: Primitive,
    placement_ids: Arc<Vec<u32>>,
    point_component_ids: Arc<Vec<Option<u32>>>,
    components: Arc<Vec<(usize, ComponentGeometry)>>,
    rotation: i32,
    #[cfg(feature = "gpu")]
    gpu_pose: Option<compact::Pose>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PlacementKey {
    designator: u32,
    x: u32,
    y: u32,
    rotation: i32,
    layer: u8,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PrimitiveKey {
    id: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    placements: Vec<PlacementKey>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SearchStateKey {
    remaining: Vec<u32>,
    placed: Vec<PrimitiveKey>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PoseKey {
    primitive: u32,
    rotation: i32,
    left: u32,
    top: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PosePairKey(PoseKey, PoseKey);

const GEOMETRY_CACHE_LIMIT: usize = 1_000_000;

#[derive(Clone)]
struct SearchState {
    placed: Vec<WorkingPrimitive>,
    remaining: Vec<WorkingPrimitive>,
    rank: Rank,
    route_penalty: f32,
    ordinal: usize,
}

#[derive(Clone)]
struct RankedCandidate {
    primitive: WorkingPrimitive,
    rank: Rank,
    route_penalty: f32,
    ordinal: usize,
    alignment: bool,
}

#[derive(Clone, Copy)]
enum CompiledEndpoint {
    Anchor(Point),
    Pad { primitive: u32, point: usize },
    Component { primitive: u32, designator: u32 },
    Primitive { primitive: u32 },
    Missing,
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone)]
struct CompiledRelation {
    from: CompiledEndpoint,
    to: CompiledEndpoint,
    skip: bool,
    hard: bool,
    weight: f32,
    inverse_tau_squared: f32,
    max_distance: Option<f32>,
    min_distance: Option<f32>,
    satellite_anchor: bool,
    anchor_offset: Option<Point>,
    side: Option<Side>,
}

struct Context {
    conditions: crate::conditions::Plan,
    problem: Arc<BoardPackProblem>,
    relations: Arc<Vec<CompiledRelation>>,
    hard_overlap_cache: RefCell<FxHashMap<PosePairKey, f32>>,
    outside_cache: RefCell<FxHashMap<PoseKey, bool>>,
    outside_severity_cache: RefCell<FxHashMap<PoseKey, f32>>,
    joint_candidates: Arc<AtomicUsize>,
    profile: Arc<profile::Profile>,
    #[cfg(feature = "gpu")]
    gpu: Option<Arc<cubecl::Control>>,
}

pub fn solve(mut problem: BoardPackProblem) -> Result<BoardPackSolution, String> {
    attach_block_relation_points(&mut problem)?;
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    // Default to half the logical cores (at most 8); an explicit override may use every core.
    let limit = (available / 2).clamp(1, 8);
    let threads = std::env::var("PCB_BOARD_PACKER_THREADS")
        .ok().and_then(|value| value.parse::<usize>().ok())
        .map_or(limit, |value| value.clamp(1, available.max(1)));
    let backend=std::env::var("PCB_BOARD_BACKEND").unwrap_or_else(|_|"auto".into());
    if !matches!(backend.as_str(),"cpu"|"cubecl"|"auto") {return Err(format!("unsupported PCB_BOARD_BACKEND {backend}"));}
    #[cfg(feature = "gpu")]
    if problem.primitives.iter().any(|p| !p.locked)
        && (backend=="cubecl" || (backend=="auto" && cubecl::auto_profitable(&problem))) {
        let control=match cubecl::Control::new(&problem,backend=="cubecl") {
            Ok(control)=>control,
            Err(error)=>{
                eprintln!("[board-backend] {}",serde_json::json!({"backend":"cpu","requested":backend,"reason":error.to_string(),"kind":error.kind}));
                return solve_with_threads(problem,threads);
            }
        };
        let original=problem.clone();
        // CPU preparation/routing keeps its existing budget independently of GPU admission.
        let gpu_threads=threads;
        let attempt=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||solve_inner(problem,gpu_threads,Some(control.clone()))));
        if let Some(error)=control.failure() {
            eprintln!("[board-gpu-fallback] {}",serde_json::json!({"reason":error.to_string(),"kind":error.kind,"replay":"original board native call"}));
            eprintln!("[board-backend] {}",serde_json::json!({"backend":"cpu","requested":backend,"reason":"gpu failure; full CPU replay"}));
            drop(control);drop(attempt);
            return solve_with_threads(original,threads);
        }
        return match attempt {
            Ok(result)=>{if result.is_ok() {control.report();eprintln!("[board-backend] {}",serde_json::json!({"backend":if control.performed_work() {"cubecl"} else {"cpu"},"requested":backend}));}result},
            Err(payload)=>std::panic::resume_unwind(payload),
        };
    }
    if std::env::var_os("PCB_BOARD_PACKER_PROFILE").is_some() {
        eprintln!("[board-backend] {}",serde_json::json!({"backend":"cpu","requested":backend,
            "reason":if backend=="cpu" {"explicit CPU"}else if !cfg!(feature="gpu") {"CPU-only build"}else{"outside measured board GPU workload/thread threshold"}}));
    }
    solve_with_threads(problem, threads)
}

fn solve_with_threads(problem: BoardPackProblem, threads: usize) -> Result<BoardPackSolution, String> {
    solve_inner(problem,threads,#[cfg(feature = "gpu")] None)
}
fn prepare_context(problem: BoardPackProblem,
    #[cfg(feature = "gpu")] gpu: Option<Arc<cubecl::Control>>,
) -> (Vec<WorkingPrimitive>, Context) {
    let primitive_ids = lexical_ids(
        problem
            .primitives
            .iter()
            .map(|primitive| primitive.id.clone()),
    );
    let designator_ids = lexical_ids(problem.primitives.iter().flat_map(|primitive| {
        primitive
            .placements
            .iter()
            .map(|placement| placement.designator.clone())
    }));
    let relations = compile_relations(&problem, &primitive_ids, &designator_ids);
    let context = Context {
        conditions: crate::conditions::plan(&problem.primitives,&problem.relations,&problem.relation_groups,
            &problem.components.iter().map(|c|(&c.designator,&c.primitive_id,&c.block_name,&c.layer,c.hint_box.unwrap_or(c.body_box))).collect::<Vec<_>>(),
            Some(problem.full_board_bounds),problem.edge_clearance),
        problem: Arc::new(problem),
        relations: Arc::new(relations),
        hard_overlap_cache: RefCell::new(FxHashMap::default()),
        outside_cache: RefCell::new(FxHashMap::default()),
        outside_severity_cache: RefCell::new(FxHashMap::default()),
        joint_candidates: Arc::new(AtomicUsize::new(0)),
        profile: profile::Profile::new(),
        #[cfg(feature = "gpu")]
        gpu,
    };
    let mut components_by_primitive: FxHashMap<Arc<str>, Vec<(usize, ComponentGeometry)>> =
        FxHashMap::default();
    for (index, component) in context.problem.components.iter().cloned().enumerate() {
        components_by_primitive
            .entry(component.primitive_id.clone())
            .or_default()
            .push((index, component));
    }
    let primitives: Vec<WorkingPrimitive> = context
        .problem
        .primitives
        .iter()
        .cloned()
        .enumerate()
        .map(|(source_index, primitive)| {
            let components = components_by_primitive
                .remove(&primitive.id)
                .unwrap_or_default();
            let placement_ids = Arc::new(
                primitive
                    .placements
                    .iter()
                    .map(|placement| designator_ids[&placement.designator])
                    .collect(),
            );
            let point_component_ids = Arc::new(
                primitive
                    .connection_points
                    .iter()
                    .map(|point| {
                        point
                            .reference
                            .split_once('.')
                            .and_then(|(designator, _)| designator_ids.get(designator).copied())
                    })
                    .collect(),
            );
            WorkingPrimitive {
                id: primitive_ids[&primitive.id],
                source_index,
                primitive,
                placement_ids,
                point_component_ids,
                components: Arc::new(components),
                rotation: 0,
                #[cfg(feature = "gpu")]
                gpu_pose: None,
            }
        })
        .collect();
    (primitives, context)
}

pub fn quality(mut problem: BoardPackProblem) -> Result<serde_json::Value, String> {
    attach_block_relation_points(&mut problem)?;
    let (primitives, context) = prepare_context(problem, #[cfg(feature = "gpu")] None);
    let rank = full_state_rank(&primitives, &context);
    if !rank.score.is_finite() { return Err("board quality score is not finite".into()); }
    Ok(serde_json::json!({"rank":rank,"spacing":soft_spacing_penalty(&primitives,&context),
        "alignment":soft_alignment_score(&primitives,&context)}))
}

pub fn spacing(problem: BoardPackProblem) -> f32 {
    let (primitives, context) = prepare_context(problem, #[cfg(feature = "gpu")] None);
    soft_spacing_penalty(&primitives, &context)
}

fn solve_inner(problem: BoardPackProblem, threads: usize,
    #[cfg(feature = "gpu")] gpu: Option<Arc<cubecl::Control>>,
) -> Result<BoardPackSolution, String> {
    let profile = std::env::var_os("PCB_BOARD_PACKER_PROFILE").is_some();
    let started = std::time::Instant::now();
    let (mut primitives, context) = prepare_context(problem, #[cfg(feature = "gpu")] gpu);
    for primitive in &mut primitives {
        #[cfg(feature = "gpu")]
        if context.gpu.is_some() {primitive.gpu_pose=Some(compact::Pose::from_primitive(primitive));}
        if !primitive.primitive.locked {
            fit_to_bounds(primitive, &context);
        }
    }

    let mut locked = Vec::new();
    let mut remaining = Vec::new();
    for primitive in primitives {
        if primitive.primitive.locked {
            locked.push(primitive);
        } else {
            remaining.push(primitive);
        }
    }
    remaining.sort_by(|a, b| compare_f32(seed_rank(b, &context), seed_rank(a, &context)));

    let mut ordinal = 0usize;
    let initial_rank = full_state_rank(&locked, &context);
    let mut states = vec![SearchState {
        placed: locked,
        remaining,
        rank: initial_rank,
        route_penalty: 0.0,
        ordinal,
    }];
    let search_width = context.problem.search_width.max(32);
    // Each lane retains its own mutable geometry and routing caches across beam
    // levels. No N-API calls or JS objects cross these native thread boundaries.
    #[cfg(feature="gpu")]
    let lane_count=if context.gpu.is_some(){threads.saturating_add(8)}else{threads};
    #[cfg(not(feature="gpu"))]
    let lane_count=threads;
    #[cfg(feature="gpu")]
    let cpu_budget=crate::compute::cpu::Budget::new(threads);
    let mut lanes: Vec<Context> = (0..lane_count.min(search_width).min(states[0].remaining.len().max(1)))
        .map(|_| Context {
            conditions: context.conditions.clone(),
            problem: context.problem.clone(),
            relations: context.relations.clone(),
            hard_overlap_cache: RefCell::new(FxHashMap::default()),
            outside_cache: RefCell::new(FxHashMap::default()),
            outside_severity_cache: RefCell::new(FxHashMap::default()),
            joint_candidates: context.joint_candidates.clone(),
            profile: context.profile.clone(),
            #[cfg(feature = "gpu")]
            gpu: context.gpu.clone(),
        }).collect();

    while states.iter().any(|state| !state.remaining.is_empty()) {
        // Atomic groups advance several blocks. Defer them until independent
        // expansions reach the same depth and can compete on the same inventory.
        let depth = states.iter().map(|s| s.remaining.len()).max().unwrap_or(0);
        let (active, deferred): (Vec<_>, Vec<_>) = states.into_iter()
            .partition(|s| s.remaining.len() == depth);
        states = active;
        let mut expanded = if lanes.len() == 1 || states.len() == 1 {
            expand_states(&states, search_width, &lanes[0])
        } else {
            let chunk_size = states.len().div_ceil(lanes.len());
            std::thread::scope(|scope| {
                let handles: Vec<_> = states.chunks(chunk_size).zip(lanes.iter_mut())
                    .map(|(chunk, lane)| {
                        #[cfg(feature="gpu")]
                        let budget=cpu_budget.clone();
                        scope.spawn(move || {let _float_env=crate::float_env::Guard::enter();
                            #[cfg(feature="gpu")]
                            let _cpu=budget.enter();
                            expand_states(chunk,search_width,lane)
                        })
                    })
                    .collect();
                let mut expanded = Vec::new();
                // Joining in input order keeps tie breaks independent of scheduling.
                for handle in handles {
                    expanded.extend(handle.join().map_err(|_| "Board search thread panicked".to_string())?);
                }
                Ok::<_, String>(expanded)
            })?
        };
        expanded.extend(deferred);
        for state in &mut expanded {
            if state.ordinal == 0 {
                ordinal += 1;
                state.ordinal = ordinal;
            }
        }
        states = dedupe_states(expanded);
        // Different placement sets have different partial objectives. Rank only
        // within an identical remaining inventory; share the per-depth budget
        // deterministically between inventories instead of comparing their scores.
        let mut inventories = std::collections::BTreeMap::<Vec<u32>, Vec<SearchState>>::new();
        for state in states {
            let mut ids: Vec<_> = state.remaining.iter().map(|p|p.id).collect(); ids.sort();
            inventories.entry(ids).or_default().push(state);
        }
        let mut levels = std::collections::BTreeMap::<usize, Vec<Vec<SearchState>>>::new();
        for (ids, candidates) in inventories {
            levels.entry(ids.len()).or_default().push(candidates);
        }
        states = Vec::new();
        for (_, groups) in levels.into_iter().rev() {
            states.extend(channels::select_level(groups,search_width,&context));
        }
        if states.is_empty() {
            break;
        }
    }

    states.sort_by(compare_states);
    let best = states
        .into_iter()
        .find(|state| state.remaining.is_empty())
        .ok_or_else(|| "Rust board packer did not produce a complete state".to_string())?;
    if profile { eprintln!("[pcb-board-packer] beam {:.3}s, threads={}, hard={}, joint candidates={}", started.elapsed().as_secs_f64(), threads, best.rank.hard_count,context.joint_candidates.load(AtomicOrdering::Relaxed)); }
    #[cfg(feature="gpu")]
    if profile && context.gpu.is_some() {eprintln!("[board-cpu-scheduler] {}",serde_json::json!({"lanes":lanes.len(),"cpu":cpu_budget.report()}));}
    profile::checkpoint("beam",&best.placed,&context);
    #[cfg(feature = "gpu")]
    if let Some(gpu)=&context.gpu {gpu.checkpoint("after_beam");gpu.set_phase("local_improve");}
    let phase = std::time::Instant::now();
    // Extra lanes compensate for beam GPU waits. Local/repair CPU fallbacks
    // spawn one worker per lane without that scheduler, so retain the CPU cap.
    lanes.truncate(threads.max(1));
    let improved = local_improve(best.placed, &context, &mut lanes);
    if profile { eprintln!("[pcb-board-packer] local_improve {:.3}s", phase.elapsed().as_secs_f64()); }
    profile::checkpoint("local_improve",&improved,&context);
    #[cfg(feature = "gpu")]
    if let Some(gpu)=&context.gpu {gpu.checkpoint("after_local_improve");gpu.set_phase("repair");}
    let phase = std::time::Instant::now();
    let repaired = repair_hard_violations(improved, &context, &mut lanes);
    if profile { eprintln!("[pcb-board-packer] repair {:.3}s, total {:.3}s", phase.elapsed().as_secs_f64(), started.elapsed().as_secs_f64()); }
    profile::checkpoint("repair",&repaired,&context);
    #[cfg(feature = "gpu")]
    if let Some(gpu)=&context.gpu {gpu.checkpoint("after_repair");}
    let final_rank = full_state_rank(&repaired, &context);
    if !final_rank.score.is_finite() || !final_rank.hard_severity.is_finite() {
        return Err("board F32 rank overflow or invalid arithmetic".into());
    }
    let states = repaired
        .iter()
        .map(|primitive| {
            let source = &context.problem.primitives[primitive.source_index];
            let source_origin = box_center(&source.bbox);
            let source_point = source.placements.first().map(|placement| {
                rotate_point(
                    &Point {
                        x: placement.x,
                        y: placement.y,
                    },
                    &source_origin,
                    primitive.rotation,
                )
            });
            let final_point = primitive
                .primitive
                .placements
                .first()
                .map(|placement| Point {
                    x: placement.x,
                    y: placement.y,
                });
            let (translation_x, translation_y) = match (source_point, final_point) {
                (Some(source), Some(final_)) => (
                    round_placement(final_.x - source.x),
                    round_placement(final_.y - source.y),
                ),
                _ => {
                    let rotated_box = rotate_box(&source.bbox, &source_origin, primitive.rotation);
                    (
                        round_placement(primitive.primitive.bbox.left - rotated_box.left),
                        round_placement(primitive.primitive.bbox.top - rotated_box.top),
                    )
                }
            };
            Ok(PrimitiveState {
                primitive_id: primitive.primitive.id.clone(),
                rotation: primitive.rotation,
                translation_x,
                translation_y,
                placements: primitive.primitive.placements.as_ref().clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    context.profile.report();
    Ok(BoardPackSolution {
        version: context.problem.version,
        states,
        rank: final_rank,
    })
}

fn expand_states(states: &[SearchState], search_width: usize, context: &Context) -> Vec<SearchState> {
    let mut expanded = Vec::new();
    for state in states {
        if state.remaining.is_empty() {
            expanded.push(state.clone());
            continue;
        }
        let next_index = choose_next(state, context);
        let next = &state.remaining[next_index];
        let limit = candidate_limit(search_width, state.remaining.len());
        let candidates = ranked_candidates(next, &state.placed, state.route_penalty, Some(limit), context);
        // Additional transitions; the original single-component expansion below
        // is always generated, even when an aligned pair is possible.
        expanded.extend(joint_channel_states(state, next_index, context));
        let any_legal = candidates.iter().any(|candidate| candidate.rank.hard_count == state.rank.hard_count);
        for candidate in candidates.into_iter()
            .filter(|candidate| !any_legal || candidate.rank.hard_count == state.rank.hard_count)
            .take(limit) {
            let mut placed = state.placed.clone();
            placed.push(candidate.primitive);
            let mut remaining = state.remaining.clone();
            remaining.remove(next_index);
            expanded.push(SearchState {
                placed, remaining, rank: candidate.rank,
                route_penalty: candidate.route_penalty, ordinal: 0,
            });
        }
    }
    expanded
}

fn ranked_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    parent_route_penalty: f32,
    limit: Option<usize>,
    context: &Context,
) -> Vec<RankedCandidate> {
    let candidates = cheap_candidates(primitive, placed, parent_route_penalty, context);
    rerank_candidates(candidates, primitive, placed, parent_route_penalty, limit, context)
}

fn joint_channel_states(state: &SearchState, next_index: usize, context: &Context) -> Vec<SearchState> {
    rank_joint_moves(state,channels::proposals(state,next_index,context),context)
}

fn rank_joint_moves(state: &SearchState, moves: Vec<Vec<WorkingPrimitive>>, context: &Context) -> Vec<SearchState> {
    let mut seen=FxHashSet::default();
    let moves: Vec<_>=moves.into_iter().filter(|ps| {
        let mut keys:Vec<_>=ps.iter().map(primitive_key).collect();keys.sort();seen.insert(keys)
    }).collect();
    if moves.is_empty() {return vec![];}
    // Each batch has one inventory. Route is non-negative, so lazy top-k can
    // safely discard a lower bound only after it loses to a fully scored result.
    let mut inventories=std::collections::BTreeMap::<Vec<u32>,Vec<Vec<WorkingPrimitive>>>::new();
    for mut ps in moves {
        // A second connector/path may order the same members differently.
        // GPU frames require identical identity order throughout one batch.
        ps.sort_by_key(|p|p.id);
        let ids:Vec<_>=ps.iter().map(|p|p.id).collect();
        inventories.entry(ids).or_default().push(ps);
    }
    let mut result=Vec::new();
    for (ids, moves) in inventories {
        let baseline=hard_count(&state.placed,context);
        #[cfg(feature="gpu")]
        let ranked=if let Some(gpu)=&context.gpu {
            let legality=gpu.legality(&state.placed,&moves,context);
            let survivors:Vec<_>=moves.into_iter().zip(legality).filter_map(|(ps,r)|(r.hard_count==baseline).then_some(ps)).collect();
            let rows=gpu.ranks(&state.placed,&survivors,context);
            rows.into_iter().map(|r|(survivors[r.index].clone(),r.rank)).collect::<Vec<_>>()
        } else {rank_joint_cpu(state,moves,context)};
        #[cfg(not(feature="gpu"))]
        let ranked=rank_joint_cpu(state,moves,context);
        let _=baseline;
        let remaining:Vec<_>=state.remaining.iter().filter(|p|!ids.contains(&p.id)).cloned().collect();
        let candidates=ranked.into_iter().enumerate().map(|(ordinal,(ps,rank))| {
            let mut placed=state.placed.clone();placed.extend(ps);
            SearchState{placed,remaining:remaining.clone(),rank,route_penalty:0.0,ordinal:ordinal+1}
        }).collect();
        let mut winners=crate::lazy_rank::top_k(candidates,4,|a,_,b,_|compare_states(a,b),|s| {
            s.route_penalty=board_layout_route(&s.placed.iter().collect::<Vec<_>>(),context);
            s.rank.score+=s.route_penalty;
        });
        for winner in &mut winners {winner.ordinal=0;}
        result.extend(winners);
    }
    context.joint_candidates.fetch_add(result.len(),AtomicOrdering::Relaxed);
    result
}

fn rank_joint_cpu(state: &SearchState, moves: Vec<Vec<WorkingPrimitive>>, context: &Context) -> Vec<(Vec<WorkingPrimitive>,Rank)> {
    moves.into_iter().filter_map(|ps| {
        let mut placed=state.placed.clone();
        for p in &ps {if candidate_hard_count(p,&placed,context)>0 {return None;}placed.push(p.clone());}
        Some((ps,state_rank(&placed,context)))
    }).collect()
}

fn cheap_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    parent_route_penalty: f32,
    context: &Context,
) -> Vec<RankedCandidate> {
    finish_cheap_candidates(score_positions(orientation_variants(primitive).iter(), placed, parent_route_penalty, context, false))
}

// Candidate generation and scoring depend only on the current placement snapshot.
// Keep movement commits sequential; evaluate independent orientations in parallel.
fn local_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
    lanes: &mut [Context],
) -> Vec<RankedCandidate> {
    let variants = orientation_variants(primitive);
    // Select after dedupe over every orientation. Per-lane shortlist selection
    // could discard needed candidates when identical poses occur in two lanes.
    #[cfg(feature = "gpu")]
    if context.gpu.is_some() {return finish_cheap_candidates(score_positions(variants.iter(),placed,0.0,context,true));}
    if lanes.len() == 1 || variants.len() <= 1 {
        return finish_cheap_candidates(score_positions(variants.iter(), placed, 0.0, context, true));
    }
    let chunk_size = variants.len().div_ceil(lanes.len());
    let candidates = std::thread::scope(|scope| {
        let handles: Vec<_> = variants.chunks(chunk_size).zip(lanes.iter_mut())
            .map(|(chunk, lane)| scope.spawn(move || { let _float_env = crate::float_env::Guard::enter(); score_positions(chunk.iter(), placed, 0.0, lane, true) }))
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().expect("Local candidate thread panicked")).collect()
    });
    finish_cheap_candidates(candidates)
}

fn score_positions<'a>(
    variants: impl Iterator<Item = &'a WorkingPrimitive>,
    placed: &[WorkingPrimitive],
    parent_route_penalty: f32,
    context: &Context,
    precise_layers: bool,
) -> Vec<RankedCandidate> {
    #[cfg(feature = "gpu")]
    if let Some(gpu)=&context.gpu {
        let baseline=hard_count(placed,context);
        let mut groups=Vec::new();let mut moves=Vec::new();
        for variant in variants {
            for alignment in [false,true] {
                let start=moves.len();
                moves.extend(position_proposals(variant,placed,context,alignment,precise_layers).into_iter().map(|p|vec![p]));
                groups.push((start,moves.len(),alignment));
            }
        }
        let rows=gpu.legality(placed,&moves,context);
        let mut selected=Vec::new();
        for (start,end,alignment) in groups {
            let any_legal=rows[start..end].iter().any(|r|r.hard_count==baseline);
            for index in start..end {
                if !any_legal || rows[index].hard_count==baseline {selected.push((index,alignment));}
            }
        }
        if std::env::var("PCB_BOARD_GPU_VERIFY").as_deref()==Ok("1") && std::env::var("PCB_BOARD_GPU_VERIFY_SHORTLIST").as_deref()!=Ok("1") {
            return selected.into_iter().map(|(i,alignment)| {
                let mut primitive=moves[i][0].clone();compact::materialize(&mut primitive);
                RankedCandidate {primitive,rank:rows[i].clone(),route_penalty:parent_route_penalty,ordinal:0,alignment}
            }).collect();
        }
        // Dedupe after each source group's legal filter, exactly as the CPU
        // finish step does. GPU shortlist IDs are returned in generator order.
        let mut seen=FxHashSet::default();
        selected.retain(|&(i,_)|seen.insert(primitive_key(&moves[i][0])));
        let survivors=selected.iter().map(|&(i,_)|moves[i].clone()).collect::<Vec<_>>();
        let flags=selected.iter().map(|&(_,alignment)|alignment).collect::<Vec<_>>();
        let winners=gpu.shortlist(placed,&survivors,&flags,32,16,context);
        let candidates=winners.into_iter().map(|row| {
            let (i,alignment)=selected[row.index];
            let mut primitive=moves[i][0].clone();
            compact::materialize(&mut primitive);
            RankedCandidate {primitive,rank:row.rank,route_penalty:parent_route_penalty,ordinal:0,alignment}
        }).collect();
        return candidates;
    }
    let mut candidates = Vec::new();
    for variant in variants {
        for alignment in [false,true] {
        for candidate in position_candidates(variant, placed, context, alignment, precise_layers) {
            let mut all = placed.to_vec();
            all.push(candidate.clone());
            candidates.push(RankedCandidate {
                primitive: candidate,
                rank: state_rank(&all, context),
                route_penalty: parent_route_penalty,
                ordinal: 0,
                alignment,
            });
        }
        }
    }
    candidates
}

fn finish_cheap_candidates(mut candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
    for (index, candidate) in candidates.iter_mut().enumerate() { candidate.ordinal = index + 1; }
    dedupe_candidates(&mut candidates);
    candidates.sort_by(compare_candidates);
    // Added axis candidates must not consume the ordinary shortlist's budget.
    let mut ordinary=0; let mut aligned=0;
    candidates.retain(|c| { let (count,limit)=if c.alignment {(&mut aligned,16)} else {(&mut ordinary,32)};
        *count+=1; *count<=limit });
    candidates
}

fn rerank_candidates(
    mut candidates: Vec<RankedCandidate>,
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    _parent_route_penalty: f32,
    limit: Option<usize>,
    context: &Context,
) -> Vec<RankedCandidate> {
    let baseline_hard = hard_count(placed, context);
    // Beam expansion consumes only legal candidates when any exist. Apply the
    // same filter before lazy ranking, otherwise illegal bounds could hide them.
    if limit.is_some() && candidates.iter().any(|candidate| candidate.rank.hard_count == baseline_hard) {
        candidates.retain(|candidate| candidate.rank.hard_count == baseline_hard);
    }
    let candidates = crate::lazy_rank::top_k(candidates, limit.unwrap_or(32), |a, a_exact, b, b_exact| {
        let lower_score = |candidate: &RankedCandidate, _exact: bool| candidate.rank.score;
        a.rank.hard_count.cmp(&b.rank.hard_count)
            .then_with(|| compare_f32(a.rank.hard_severity, b.rank.hard_severity))
            .then_with(|| compare_f32(lower_score(a, a_exact), lower_score(b, b_exact)))
            .then_with(|| a.ordinal.cmp(&b.ordinal))
    }, |candidate| {
        {
            let correction = board_geometric_route_penalty(&candidate.primitive, placed, context);
            candidate.route_penalty = correction;
            candidate.rank.score += candidate.route_penalty;
        }
    });
    if std::env::var("PCB_BOARD_PACKER_TRACE_DESIGNATOR")
        .ok()
        .as_deref()
        == primitive
            .primitive
            .placements
            .first()
            .map(|placement| placement.designator.as_ref())
    {
        let values: Vec<_> = candidates
            .iter()
            .take(12)
            .filter_map(|candidate| {
                candidate
                    .primitive
                    .primitive
                    .placements
                    .first()
                    .map(|placement| {
                        (
                            placement.x,
                            placement.y,
                            placement.rotate,
                            candidate.rank.hard_count,
                            candidate.rank.score,
                        )
                    })
            })
            .collect();
        eprintln!(
            "[pcb-board-packer] candidates {}: {:?}",
            primitive.primitive.id, values
        );
    }
    candidates
}

fn board_geometric_route_penalty(
    candidate: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> f32 {
    board_layout_route(&placed.iter().chain(std::iter::once(candidate)).collect::<Vec<_>>(), context)
}
fn board_layout_route(all: &[&WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(12);
    let mut obstacles = Vec::new();
    for item in all {
        let source = &context.problem.primitives[item.source_index];
        let origin = box_center(&source.bbox);
        let center = box_center(&item.primitive.bbox);
        for pad in context.problem.routing_obstacles.iter().filter(|pad| pad.primitive_id.as_ref() == Some(&source.id)) {
            let mut pad = pad.clone();
            pad.box_ = translate_box(&rotate_box(&pad.box_, &origin, item.rotation), center.x-origin.x, center.y-origin.y);
            obstacles.push(pad);
        }
    }
    obstacles.extend(context.problem.routing_obstacles.iter().filter(|p| p.primitive_id.is_none()).cloned());
    obstacles.extend(context.problem.obstacles.iter().map(|b| crate::model::RouteObstacle {
        box_: *b, layer: None, reference: None, net: None, primitive_id: None,
    }));
    let plan = crate::fast_route::layout_plan(&all.iter().map(|p| &p.primitive).collect::<Vec<_>>(), &obstacles,
        &[], &context.problem.ignored_nets, context.problem.clearance, context.problem.bounds, &context.problem.relations);
    #[cfg(feature="gpu")]
    if let Some(gpu)=&context.gpu { return gpu.route(&plan); }
    plan.cpu()
}

fn position_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
    alignment_only: bool,
    precise_layers: bool,
) -> Vec<WorkingPrimitive> {
    let candidates = position_proposals(primitive, placed, context, alignment_only, precise_layers);
    let legal: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate_hard_count(candidate, placed, context) == 0)
        .cloned()
        .collect();
    if legal.is_empty() {
        candidates
    } else {
        legal
    }
}
fn position_proposals(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
    alignment_only: bool,
    precise_layers: bool,
) -> Vec<WorkingPrimitive> {
    let _profile = context.profile.span(0);
    let pack_box = packing_box(primitive);
    let width = pack_box.right - pack_box.left;
    let height = pack_box.bottom - pack_box.top;
    let mut centers = Vec::new();
    if alignment_only {
        centers.extend(alignment_slot_centers(primitive, placed, context));
    } else {
    centers.extend(edge_place_centers(primitive, width, height, context));
    centers.extend(board_slot_centers(width, height, &context.problem.bounds));
    centers.extend(free_rect_slot_centers(primitive, width, height, placed, context));
    centers.extend(placed_slot_centers(primitive, width, height, placed, context));
    if precise_layers && needs_layer_slots(primitive) {
        centers.extend(layer_slot_centers(primitive, placed, context));
    }
    centers.extend(relation_slot_centers(primitive, placed, context));
    let placed_primitives: Vec<_> = placed.iter().map(|item| &item.primitive).collect();
    let current_center = box_center(&pack_box);
    centers.extend(
        signal_path::bridge_deltas(&primitive.primitive, &placed_primitives)
            .into_iter()
            .map(|delta| Point {
                x: current_center.x + delta.x,
                y: current_center.y + delta.y,
            }),
    );
    centers.extend(coarse_grid_centers(
        width,
        height,
        &context.problem.bounds,
        context.problem.grid,
    ));
    }
    dedupe_points(&mut centers);

    let mut candidates = Vec::with_capacity(centers.len());
    for center in centers {
        let mut candidate = primitive.clone();
        #[cfg(feature = "gpu")]
        if context.gpu.is_some() {
            if let Some(pose)=&mut candidate.gpu_pose {pose.deferred=true;}
        }
        move_packing_center(&mut candidate, center);
        fit_to_bounds(&mut candidate, context);
        candidates.push(candidate);
    }
    candidates
}

fn orientation_variants(primitive: &WorkingPrimitive) -> Vec<WorkingPrimitive> {
    let orientations = if !primitive.primitive.allowed_orientations.is_empty() {
        primitive.primitive.allowed_orientations.as_ref().clone()
    } else if primitive.primitive.can_rotate {
        vec![0, 90, 180, 270]
    } else {
        vec![0]
    };
    orientations
        .into_iter()
        .map(|angle| {
            let mut variant = primitive.clone();
            rotate_primitive(&mut variant, angle);
            variant
        })
        .collect()
}

fn translate_primitive(primitive: &mut WorkingPrimitive, dx: f32, dy: f32) {
    #[cfg(feature = "gpu")]
    if let Some(pose) = &mut primitive.gpu_pose {
        pose.translations.push((dx, dy));
        if !pose.template.primitive.collision_boxes.is_empty() {pose.packing_bounds=translate_box(&pose.packing_bounds,dx,dy);}
        if pose.deferred {
            primitive.primitive.bbox=translate_box(&primitive.primitive.bbox,dx,dy);
            return;
        }
    }
    primitive.primitive.bbox = translate_box(&primitive.primitive.bbox, dx, dy);
    for box_ in Arc::make_mut(&mut primitive.primitive.collision_boxes) {
        *box_ = translate_box(box_, dx, dy);
    }
    for placement in Arc::make_mut(&mut primitive.primitive.placements) {
        placement.x = round_placement(placement.x + dx);
        placement.y = round_placement(placement.y + dy);
    }
    for point in Arc::make_mut(&mut primitive.primitive.connection_points) {
        point.x = round_placement(point.x + dx);
        point.y = round_placement(point.y + dy);
    }
    for port in Arc::make_mut(&mut primitive.primitive.path_ports) {
        port.x = round_placement(port.x + dx);
        port.y = round_placement(port.y + dy);
    }
    for (_, component) in Arc::make_mut(&mut primitive.components) {
        for row in &mut component.access_rows {row.box_=translate_box(&row.box_,dx,dy);}
        component.body_box = translate_box(&component.body_box, dx, dy);
        for box_ in Arc::make_mut(&mut component.through_hole_boxes) {
            *box_ = translate_box(box_, dx, dy);
        }
    }
}

fn rotate_primitive(primitive: &mut WorkingPrimitive, angle: i32) {
    let angle = normalize_rotation(angle);
    if angle == 0 {
        return;
    }
    let origin = box_center(&primitive.primitive.bbox);
    primitive.primitive.bbox = rotate_box(&primitive.primitive.bbox, &origin, angle);
    for box_ in Arc::make_mut(&mut primitive.primitive.collision_boxes) {
        *box_ = rotate_box(box_, &origin, angle);
    }
    primitive.primitive.width =
        round_placement(primitive.primitive.bbox.right - primitive.primitive.bbox.left);
    primitive.primitive.height =
        round_placement(primitive.primitive.bbox.bottom - primitive.primitive.bbox.top);
    let orientations = Arc::make_mut(&mut primitive.primitive.allowed_orientations);
    for orientation in orientations.iter_mut() {
        *orientation = crate::geometry::subtract_rotation(*orientation, angle);
    }
    orientations.sort_unstable();
    orientations.dedup();
    for placement in Arc::make_mut(&mut primitive.primitive.placements) {
        let point = rotate_point(
            &Point {
                x: placement.x,
                y: placement.y,
            },
            &origin,
            angle,
        );
        placement.x = point.x;
        placement.y = point.y;
        placement.rotate = crate::geometry::add_rotation(placement.rotate, angle);
    }
    for point in Arc::make_mut(&mut primitive.primitive.connection_points) {
        let rotated = rotate_point(
            &Point {
                x: point.x,
                y: point.y,
            },
            &origin,
            angle,
        );
        point.x = rotated.x;
        point.y = rotated.y;
    }
    for port in Arc::make_mut(&mut primitive.primitive.path_ports) {
        let rotated = rotate_point(
            &Point {
                x: port.x,
                y: port.y,
            },
            &origin,
            angle,
        );
        port.x = rotated.x;
        port.y = rotated.y;
        port.normal = signal_path::rotate_normal(&port.normal, angle);
    }
    for (_, component) in Arc::make_mut(&mut primitive.components) {
        for row in &mut component.access_rows {row.box_=rotate_box(&row.box_,&origin,angle);}
        component.body_box = rotate_box(&component.body_box, &origin, angle);
        for box_ in Arc::make_mut(&mut component.through_hole_boxes) {
            *box_ = rotate_box(box_, &origin, angle);
        }
    }
    primitive.rotation = crate::geometry::add_rotation(primitive.rotation, angle);
    #[cfg(feature = "gpu")]
    if primitive.gpu_pose.is_some() { primitive.gpu_pose = Some(compact::Pose::from_primitive(primitive)); }
}

fn full_state_rank(primitives: &[WorkingPrimitive], context: &Context) -> Rank {
    let mut rank = state_rank(primitives, context);
    rank.score += board_layout_route(&primitives.iter().collect::<Vec<_>>(), context);
    rank
}

fn state_rank(primitives: &[WorkingPrimitive], context: &Context) -> Rank {
    let _profile = context.profile.span(2);
    let hard_count = hard_count(primitives, context);
    let hard_severity = hard_severity(primitives, context);
    Rank {
        hard_count,
        hard_severity,
        score: board_score(primitives, context, hard_count, hard_severity),
    }
}

fn local_improve(mut current: Vec<WorkingPrimitive>, context: &Context, lanes: &mut [Context]) -> Vec<WorkingPrimitive> {
    let mut current_rank = state_rank(&current, context);
    let passes = if current_rank.hard_count > 0 { 4 } else { 2 };
    for _ in 0..passes {
        let mut changed = false;
        for index in 0..current.len() {
            if current[index].primitive.locked {
                continue;
            }
            let fixed: Vec<_> = current
                .iter()
                .enumerate()
                .filter(|(item_index, _)| *item_index != index)
                .map(|(_, item)| item.clone())
                .collect();
            let mut best = current[index].clone();
            let best_rank = Rank {
                hard_count: current_rank.hard_count,
                hard_severity: current_rank.hard_severity,
                score: current_rank.score
                    + board_geometric_route_penalty(&current[index], &fixed, context),
            };
            if let Some(candidate) = crate::lazy_rank::improve(
                local_candidates(&current[index], &fixed, context, lanes), best_rank,
                compare_candidates, |candidate, best_rank| rank_improves(&candidate.rank, best_rank),
                |candidate| candidate.rank.clone(), |candidate| {
                    {
                        candidate.route_penalty = board_geometric_route_penalty(&candidate.primitive, &fixed, context);
                        candidate.rank.score += candidate.route_penalty;
                    }
                },
            ) {
                best = candidate.primitive;
            }
            if primitive_key(&best) != primitive_key(&current[index]) {
                current[index] = best;
                current_rank = state_rank(&current, context);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    current
}

fn repair_hard_violations(
    mut current: Vec<WorkingPrimitive>,
    context: &Context,
    lanes: &mut [Context],
) -> Vec<WorkingPrimitive> {
    let mut current_rank = full_state_rank(&current, context);
    let max_passes = 8.max(current.len() * 6);
    for _ in 0..max_passes {
        let variants = hard_repair_variants(&current, context);
        #[cfg(feature = "gpu")]
        let (variants,gpu_ranks)=if let Some(gpu)=&context.gpu {
            let winners=gpu.ranks(&[],&variants,context);
            let selected=winners.iter().map(|row|variants[row.index].clone()).collect();
            (selected,Some(winners.into_iter().map(|row|row.rank).collect::<Vec<_>>()))
        }else{(variants,None)};
        #[cfg(not(feature = "gpu"))]
        let gpu_ranks:Option<Vec<Rank>>=None;
        let ranks = if let Some(mut rows)=gpu_ranks { for (row,v) in rows.iter_mut().zip(&variants) { row.score+=board_layout_route(&v.iter().collect::<Vec<_>>(),context); } rows} else if variants.len() <= 1 || lanes.len() == 1 {
            variants.iter().map(|variant| full_state_rank(variant, context)).collect::<Vec<_>>()
        } else {
            let chunk_size = variants.len().div_ceil(lanes.len());
            std::thread::scope(|scope| {
                let handles: Vec<_> = variants.chunks(chunk_size).zip(lanes.iter_mut())
                    .map(|(chunk, lane)| scope.spawn(move || { let _float_env = crate::float_env::Guard::enter(); chunk.iter().map(|variant| full_state_rank(variant, lane)).collect::<Vec<_>>() }))
                    .collect();
                handles.into_iter().flat_map(|handle| handle.join().expect("Repair scoring thread panicked")).collect()
            })
        };
        let mut best: Option<(Vec<WorkingPrimitive>, Rank)> = None;
        for (variant, rank) in variants.into_iter().zip(ranks) {
            if compare_rank(
                &rank,
                best.as_ref().map(|(_, rank)| rank).unwrap_or(&current_rank),
            ) == Ordering::Less
            {
                best = Some((variant, rank));
            }
        }
        let Some((variant, rank)) = best else {
            break;
        };
        current = variant;
        current_rank = rank;
        if current_rank.hard_count == 0 {
            current = local_improve(current, context, lanes);
            current_rank = full_state_rank(&current, context);
            if current_rank.hard_count == 0 {
                break;
            }
        }
    }
    current
}

fn hard_repair_variants(
    primitives: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Vec<WorkingPrimitive>> {
    let mut variants = Vec::new();
    for index in 0..primitives.len() {
        if primitives[index].primitive.locked {
            continue;
        }
        let mut fitted = primitives[index].clone();
        fit_to_bounds(&mut fitted, context);
        if primitive_key(&fitted) != primitive_key(&primitives[index]) {
            let mut variant = primitives.to_vec();
            variant[index] = fitted;
            variants.push(variant);
        }
    }
    for a_index in 0..primitives.len() {
        for b_index in a_index + 1..primitives.len() {
            let clearance =
                primitive_clearance(&primitives[a_index], &primitives[b_index], context);
            if boxes_overlap_depth(
                packing_boxes(&primitives[a_index]),
                packing_boxes(&primitives[b_index]),
                clearance,
            ) <= 0.0
            {
                continue;
            }
            variants.extend(separation_variants(primitives, a_index, b_index, context));
        }
    }
    variants
}

fn separation_variants(
    primitives: &[WorkingPrimitive],
    a_index: usize,
    b_index: usize,
    context: &Context,
) -> Vec<Vec<WorkingPrimitive>> {
    let a = &primitives[a_index];
    let b = &primitives[b_index];
    if a.primitive.locked && b.primitive.locked {
        return Vec::new();
    }
    // Contacts must use the shapes that actually meet on a common layer.
    // The union of both faces can demand a displacement larger than a legal slot.
    let clearance = primitive_clearance(a, b, context);
    let mut offsets = Vec::new();
    // Keep conservative whole-block escapes too: for a crowded compound block,
    // clearing one component contact can merely expose another equal violation.
    let aa = packing_box(a);
    let bb = packing_box(b);
    if overlap_depth(&aa, &bb, clearance) > 0.0 {
        offsets.extend([
            ('x', bb.left - clearance - aa.right - 0.001),
            ('x', bb.right + clearance - aa.left + 0.001),
            ('y', bb.top - clearance - aa.bottom - 0.001),
            ('y', bb.bottom + clearance - aa.top + 0.001),
        ]);
    }
    for layer in ["top", "bottom"] {
        for aa in occupied_boxes_on_layer(a, layer) {
            for bb in occupied_boxes_on_layer(b, layer) {
                if overlap_depth(&aa, &bb, clearance) <= 0.0 { continue; }
                offsets.extend([
                    ('x', bb.left - clearance - aa.right - 0.001),
                    ('x', bb.right + clearance - aa.left + 0.001),
                    ('y', bb.top - clearance - aa.bottom - 0.001),
                    ('y', bb.bottom + clearance - aa.top + 0.001),
                ]);
            }
        }
    }
    let mut seen = FxHashSet::default();
    offsets.retain(|(axis, distance)| seen.insert((*axis, round_placement(*distance).to_bits())));
    offsets.sort_by(|a, b| compare_f32(a.1.abs(), b.1.abs()));
    let mut variants = Vec::new();
    for (axis, distance) in offsets {
        let a_share = if a.primitive.locked {
            0.0
        } else if b.primitive.locked {
            1.0
        } else {
            0.5
        };
        let b_share = if b.primitive.locked {
            0.0
        } else if a.primitive.locked {
            1.0
        } else {
            0.5
        };
        let mut both = primitives.to_vec();
        if a_share > 0.0 {
            both[a_index] = translated_for_repair(a, axis, distance * a_share, context);
        }
        if b_share > 0.0 {
            both[b_index] =
                translated_for_repair(b, axis, -distance * b_share, context);
        }
        variants.push(both);
        if !a.primitive.locked {
            let mut one = primitives.to_vec();
            one[a_index] = translated_for_repair(a, axis, distance, context);
            variants.push(one);
        }
        if !b.primitive.locked {
            let mut one = primitives.to_vec();
            one[b_index] = translated_for_repair(b, axis, -distance, context);
            variants.push(one);
        }
    }
    variants
}

fn translated_for_repair(
    primitive: &WorkingPrimitive,
    axis: char,
    distance: f32,
    context: &Context,
) -> WorkingPrimitive {
    let mut moved = primitive.clone();
    if axis == 'x' {
        translate_primitive(&mut moved, round_placement(distance), 0.0);
    } else {
        translate_primitive(&mut moved, 0.0, round_placement(distance));
    }
    fit_to_bounds(&mut moved, context);
    moved
}

fn board_score(
    primitives: &[WorkingPrimitive],
    context: &Context,
    _hard_count: usize,
    hard_severity: f32,
) -> f32 {
    if primitives.is_empty() {
        return 0.0;
    }
    let boxes: Vec<_> = primitives
        .iter()
        .flat_map(|primitive| packing_boxes(primitive).iter().copied())
        .collect();
    let bbox = union_boxes(&boxes);
    let width = bbox.right - bbox.left;
    let height = bbox.bottom - bbox.top;
    let high = context.problem.compactness.as_ref() == "high";
    let area_weight = if high { 1.2 } else { 0.8 };
    let perimeter_weight = if high { 8.0 } else { 1.5 };
    let overlap_weight = if high { 1.2 } else { 1.0 };
    let edge_weight = if high { 0.4 } else { 1.0 };
    let path_primitives: Vec<_> = primitives.iter().map(|item| &item.primitive).collect();
    let topology = {
        let _profile = context.profile.span(11);
        signal_path::explicit_topology_penalty(&path_primitives, &context.problem.relations)
    };
    let scale = context.problem.soft_spacing.as_ref().map_or(1.0, |s| s.compactness_scale);
    relation_penalty(primitives, context)
        + hard_severity * 1_000_000.0
        // Count is already the first integer rank field. Its constant offset
        // cancels in every same-count comparison, but at F32 scales of 2e8
        // it would erase improvements of up to 16 score units. Keep severity
        // here because rank_improves allows its existing 0.001 tie zone.
        + envelope_overlap_penalty(primitives, context) * overlap_weight
        + width * height * area_weight * scale
        + (width + height) * perimeter_weight * scale
        + soft_spacing_penalty(primitives, context)
        + soft_alignment_score(primitives, context)
        + edge_bias_penalty(primitives, context) * edge_weight
        + edge_place_penalty(primitives, context)
        + topology
        + crate::ordinary_net::penalty(&path_primitives, &context.problem.relations) * if high { 3.0 } else { 5.0 }
        + access_penalty(primitives)
}

fn access_penalty(ps:&[WorkingPrimitive])->f32{
    let cs:Vec<_>=ps.iter().flat_map(|p|p.components.iter().map(|(_,c)|c)).collect();let mut total=0.0;
    for a in &cs {for b in &cs {if a.designator!=b.designator && a.layer==b.layer {total+=crate::pad_access::cost(&a.access_rows,b.body_box);}}}total
}

fn hard_count(primitives: &[WorkingPrimitive], context: &Context) -> usize {
    let _profile = context.profile.span(3);
    let mut count = 0;
    for i in 0..primitives.len() {
        for j in i + 1..primitives.len() {
            if primitive_hard_overlap(&primitives[i], &primitives[j], context) > 0.0 {
                count += 1;
            }
        }
    }
    for primitive in primitives {
        if primitive_outside(primitive, context) {
            count += 1;
        }
        if edge_place_violation(primitive, context) > 0.0 {
            count += 1;
        }
        for obstacle in &context.problem.obstacles {
            if boxes_overlap_depth(
                packing_boxes(primitive),
                &[*obstacle],
                context.problem.clearance,
            ) > 0.0
            {
                count += 1;
            }
        }
        count += constraint_violation_count(primitive, context);
    }
    count + condition_evaluation(primitives,context).hard
}

fn hard_severity(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(4);
    let mut severity = 0.0;
    for i in 0..primitives.len() {
        for j in i + 1..primitives.len() {
            severity += primitive_hard_overlap(&primitives[i], &primitives[j], context);
        }
    }
    for primitive in primitives {
        severity += primitive_outside_severity(primitive, context)
            + edge_place_violation(primitive, context);
        for obstacle in &context.problem.obstacles {
            severity += boxes_overlap_depth(
                packing_boxes(primitive),
                &[*obstacle],
                context.problem.clearance,
            );
        }
        severity += constraint_violation_severity(primitive, context);
    }
    severity + condition_evaluation(primitives,context).severity
}

fn candidate_hard_count(
    candidate: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> usize {
    let _profile = context.profile.span(1);
    let mut count = usize::from(primitive_outside(candidate, context));
    count += usize::from(edge_place_violation(candidate, context) > 0.0);
    count += placed
        .iter()
        .filter(|other| primitive_hard_overlap(candidate, other, context) > 0.0)
        .count();
    count += context
        .problem
        .obstacles
        .iter()
        .filter(|obstacle| {
            boxes_overlap_depth(
                packing_boxes(candidate),
                &[**obstacle],
                context.problem.clearance,
            ) > 0.0
        })
        .count();
    if !context.conditions.is_empty() {
        let mut all=placed.to_vec();all.push(candidate.clone());
        count += condition_evaluation(&all,context).hard.saturating_sub(condition_evaluation(placed,context).hard);
    }
    count + constraint_violation_count(candidate, context)
}

fn primitive_hard_overlap(a: &WorkingPrimitive, b: &WorkingPrimitive, context: &Context) -> f32 {
    let key = pose_pair_key(a, b);
    if let Some(value) = context.hard_overlap_cache.borrow().get(&key).copied() {
        return value;
    }
    let broad_clearance = primitive_clearance(a, b, context);
    if boxes_overlap_depth(packing_boxes(a), packing_boxes(b), broad_clearance) <= 0.0 {
        cache_insert(&context.hard_overlap_cache, key, 0.0);
        return 0.0;
    }
    let mut max_overlap: f32 = 0.0;
    for (a_index, a_component) in a.components.iter() {
        for (b_index, b_component) in b.components.iter() {
            let count = context.problem.components.len();
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
                max_overlap = max_overlap.max(boxes_overlap_depth(
                    &a_component.through_hole_boxes,
                    &b_component.through_hole_boxes,
                    clearance,
                ));
            } else {
                max_overlap = max_overlap.max(boxes_overlap_depth(
                    &[a_component.body_box],
                    &b_component.through_hole_boxes,
                    clearance,
                ));
                max_overlap = max_overlap.max(boxes_overlap_depth(
                    &a_component.through_hole_boxes,
                    &[b_component.body_box],
                    clearance,
                ));
            }
        }
    }
    let result = max_overlap;
    cache_insert(&context.hard_overlap_cache, key, result);
    result
}

fn primitive_clearance(a: &WorkingPrimitive, b: &WorkingPrimitive, context: &Context) -> f32 {
    let mut clearance = context.problem.clearance;
    let count = context.problem.components.len();
    for (a_index, _) in a.components.iter() {
        for (b_index, _) in b.components.iter() {
            clearance =
                clearance.max(context.problem.component_pair_clearance[a_index * count + b_index]);
        }
    }
    clearance
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

fn primitive_outside(primitive: &WorkingPrimitive, context: &Context) -> bool {
    let key = pose_key(primitive);
    if let Some(value) = context.outside_cache.borrow().get(&key).copied() {
        return value;
    }
    let result = if primitive.components.is_empty() {
        let edge = primitive
            .primitive
            .edge_place
            .as_ref()
            .and_then(|intent| intent.inset)
            .unwrap_or(context.problem.edge_clearance)
            .max(0.0);
        packing_boxes(primitive).iter().any(|box_| {
            !box_inside_polygon_board(
                box_,
                &context.problem.full_board_bounds,
                &context.problem.board_outline,
                edge,
            )
        })
    } else {
        primitive
            .components
            .iter()
            .any(|(_, component)| component_outside(component, context))
    };
    cache_insert(&context.outside_cache, key, result);
    result
}

fn primitive_outside_severity(primitive: &WorkingPrimitive, context: &Context) -> f32 {
    let key = pose_key(primitive);
    if let Some(value) = context.outside_severity_cache.borrow().get(&key).copied() {
        return value;
    }
    let result = if primitive.components.is_empty() {
        let edge = primitive
            .primitive
            .edge_place
            .as_ref()
            .and_then(|intent| intent.inset)
            .unwrap_or(context.problem.edge_clearance)
            .max(0.0);
        let bounds = inset_box(&context.problem.full_board_bounds, edge);
        packing_boxes(primitive)
            .iter()
            .map(|box_| polygon_board_outside_severity(box_, &bounds, edge, context))
            .sum()
    } else {
        primitive
            .components
            .iter()
            .map(|(_, component)| component_outside_severity(component, context))
            .sum()
    };
    cache_insert(&context.outside_severity_cache, key, result);
    result
}

fn component_outside(component: &ComponentGeometry, context: &Context) -> bool {
    if has_board_overflow(component) {
        return std::iter::once(&component.body_box).chain(component.through_hole_boxes.iter())
            .any(|box_| box_outside_bounds_severity(box_, &component_board_bounds(component, context)) > 0.0);
    }
    std::iter::once(&component.body_box).chain(component.through_hole_boxes.iter()).any(|box_| {
        !box_inside_polygon_board(box_, &context.problem.full_board_bounds,
            &context.problem.board_outline, component.edge_clearance)
    })
}

fn component_outside_severity(component: &ComponentGeometry, context: &Context) -> f32 {
    let bounds = component_board_bounds(component, context);
    if has_board_overflow(component) {
        return std::iter::once(&component.body_box).chain(component.through_hole_boxes.iter())
            .map(|box_| box_outside_bounds_severity(box_, &bounds)).sum();
    }
    std::iter::once(&component.body_box).chain(component.through_hole_boxes.iter())
        .map(|box_| polygon_board_outside_severity(box_, &bounds, component.edge_clearance, context)).sum()
}

fn component_board_bounds(component: &ComponentGeometry, context: &Context) -> Box2 {
    let full = context.problem.full_board_bounds;
    let overflow = component.board_overflow;
    let edge = component.edge_clearance.max(0.0);
    Box2 {
        left: if overflow.left > 0.0 {
            full.left - overflow.left
        } else {
            full.left + edge
        },
        right: if overflow.right > 0.0 {
            full.right + overflow.right
        } else {
            full.right - edge
        },
        top: if overflow.top > 0.0 {
            full.top - overflow.top
        } else {
            full.top + edge
        },
        bottom: if overflow.bottom > 0.0 {
            full.bottom + overflow.bottom
        } else {
            full.bottom - edge
        },
    }
}

fn has_board_overflow(component: &ComponentGeometry) -> bool {
    component.board_overflow.left > 0.0
        || component.board_overflow.right > 0.0
        || component.board_overflow.top > 0.0
        || component.board_overflow.bottom > 0.0
}

fn polygon_board_outside_severity(box_: &Box2, bounds: &Box2, edge: f32, context: &Context) -> f32 {
    let mut severity = box_outside_bounds_severity(box_, bounds);
    for corner in box_corners(box_) {
        let inside = point_in_polygon(&corner, &context.problem.board_outline)
            && (edge <= 0.0
                || point_to_polygon_distance(&corner, &context.problem.board_outline) + crate::geometry_policy::DISTANCE_EPSILON_MM
                    >= edge);
        if inside {
            continue;
        }
        let edge_distance = point_to_polygon_distance(&corner, &context.problem.board_outline);
        severity += edge_distance.max(0.01) + (edge - edge_distance).max(0.0);
    }
    severity
}

fn component_region_overlap(component: &ComponentGeometry, region: &crate::model::ConstraintRegion) -> f32 {
    let opposite = if component.layer.as_ref() == "top" { "bottom" } else { "top" };
    let mut depth: f32 = 0.0;
    if region.layers.contains(&component.layer) {
        depth = overlap_depth(&component.body_box, &region.box_, 0.0);
    }
    if region.layers.iter().any(|layer| layer.as_ref() == opposite) {
        for box_ in component.through_hole_boxes.iter() {
            depth = depth.max(overlap_depth(box_, &region.box_, 0.0));
        }
    }
    depth
}

fn constraint_violation_count(primitive: &WorkingPrimitive, context: &Context) -> usize {
    context
        .problem
        .constraint_regions
        .iter()
        .filter(|region| {
            primitive.components.iter().any(|(_, component)| {
                !region.allow_blocks.contains(&component.block_name)
                    && component_region_overlap(component, region) > 0.0
            })
        })
        .count()
}

fn constraint_violation_severity(primitive: &WorkingPrimitive, context: &Context) -> f32 {
    context
        .problem
        .constraint_regions
        .iter()
        .flat_map(|region| {
            primitive.components.iter().filter_map(|(_, component)| {
                if region.allow_blocks.contains(&component.block_name)
                {
                    return None;
                }
                Some(component_region_overlap(component, region))
            })
        })
        .sum()
}

fn envelope_overlap_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(6);
    let mut penalty = 0.0;
    for i in 0..primitives.len() {
        for j in i + 1..primitives.len() {
            if !primitive_can_conflict(&primitives[i], &primitives[j], context) {
                continue;
            }
            let depth = ["top", "bottom"].into_iter().filter_map(|layer| {
                Some(overlap_depth(
                    &layer_envelope(&primitives[i], layer)?,
                    &layer_envelope(&primitives[j], layer)?,
                    context.problem.clearance,
                ))
            }).fold(0.0f32, f32::max);
            if depth > 0.0 {
                penalty += depth.powi(2) * 700.0 + depth * 140.0;
            }
        }
    }
    penalty
}

fn relation_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(5);
    let mut penalty = 0.0;
    for relation in context.relations.iter() {
        if relation.skip {
            continue;
        }
        let Some(from) = endpoint_point(primitives, relation.from) else {
            continue;
        };
        let Some(mut to) = endpoint_point(primitives, relation.to) else {
            continue;
        };
        if relation.satellite_anchor {
            if let Some(offset) = relation.anchor_offset {
                to.0.x = round_placement(to.0.x + offset.x);
                to.0.y = round_placement(to.0.y + offset.y);
            }
        }
        if from.1 == to.1 {
            continue;
        }
        let weight = relation.weight;
        let distance = ((from.0.x - to.0.x).powi(2) + (from.0.y - to.0.y).powi(2)).sqrt();
        penalty += crate::relation_score::distance_cost(distance,weight,relation.min_distance.unwrap_or(-1.0),relation.max_distance.unwrap_or(-1.0),relation.inverse_tau_squared);
        if relation.satellite_anchor {
            if let Some(side) = relation.side {
                penalty += side_penalty(side, from.0, to.0, weight);
            }
        }
    }
    penalty + condition_evaluation(primitives,context).score
}

/// Diagnostic enclosure for the relation expression on bit-identical
/// materialized F32 geometry. This is not a proof about original physical
/// geometry and is never used by legality, pruning or candidate selection.
#[cfg(feature = "gpu")]
fn relation_penalty_enclosure(primitives: &[WorkingPrimitive], context: &Context) -> crate::interval::Interval {
    use crate::interval::Interval as I;
    fn endpoint(primitives: &[WorkingPrimitive], e: CompiledEndpoint) -> Option<([I;2],Option<u32>)> {
        let (point,owner)=endpoint_point(primitives,e)?;
        Some(([I::exact(point.x),I::exact(point.y)],owner))
    }
    let zero=I::exact(0.0);let mut total=zero;
    for relation in context.relations.iter() {
        if relation.skip {continue;}
        let Some((from,a))=endpoint(primitives,relation.from) else {continue;};
        let Some((mut to,b))=endpoint(primitives,relation.to) else {continue;};
        if a==b {continue;}
        if relation.satellite_anchor {
            if let Some(offset)=relation.anchor_offset {
                to[0]=to[0].add(I::exact(offset.x)).placement();
                to[1]=to[1].add(I::exact(offset.y)).placement();
            }
        }
        let dx=from[0].sub(to[0]);let dy=from[1].sub(to[1]);
        let distance=dx.square().add(dy.square()).max(zero).sqrt_vulkan();
        let weight=I::exact(relation.weight);
        let scale=I::exact(relation.inverse_tau_squared);
        let mut limits=zero;
        if let Some(max)=relation.max_distance {
            let excess=distance.sub(I::exact(max)).max(zero);
            limits=limits.add(excess.square().mul(excess).mul(scale).mul(weight));
        }
        if let Some(min)=relation.min_distance {
            let shortage=I::exact(min).sub(distance).max(zero);
            limits=limits.add(shortage.square().mul(shortage).mul(scale).mul(weight));
        }
        total=total.add(distance.mul(weight).add(limits));
        if relation.satellite_anchor {
            if let Some(side)=relation.side {
                let (ax,ay)=if distance.lo>0.000001 {
                    (dx.div_vulkan(distance),dy.div_vulkan(distance))
                } else if distance.hi<=0.000001 { (zero,zero) }
                else {return I::WHOLE;};
                let (x,y)=match side {Side::Left=>(-1.0,0.0),Side::Right=>(1.0,0.0),Side::Top=>(0.0,-1.0),Side::Bottom=>(0.0,1.0)};
                total=total.add(I::exact(1.0).sub(ax.mul(I::exact(x))).sub(ay.mul(I::exact(y))).max(zero).mul(weight).mul(I::exact(120.0)));
            }
        }
    }
    total.add(I::exact(condition_evaluation(primitives,context).score))
}

fn edge_bias_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(9);
    let board_area = box_area(&context.problem.bounds).max(1.0);
    primitives
        .iter()
        .filter(|primitive| !primitive.primitive.locked)
        .map(|primitive| {
            let box_ = packing_box(primitive);
            let ratio = box_area(&box_) / board_area;
            if ratio < 0.055 && primitive.primitive.kind.as_ref() != "module" {
                return 0.0;
            }
            let distance = (box_.left - context.problem.bounds.left)
                .abs()
                .min((context.problem.bounds.right - box_.right).abs())
                .min((box_.top - context.problem.bounds.top).abs())
                .min((context.problem.bounds.bottom - box_.bottom).abs());
            distance * (1.0 + ratio * 60.0).min(8.0)
        })
        .sum()
}

fn edge_place_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(10);
    primitives
        .iter()
        .map(|primitive| {
            let Some(intent) = &primitive.primitive.edge_place else {
                return 0.0;
            };
            let box_ = packing_box(primitive);
            let center = box_center(&box_);
            intent
                .edges
                .iter()
                .map(|edge| {
                    let inset = intent.inset.unwrap_or(0.0).max(0.0);
                    let distance = match edge.as_ref() {
                        "left" => {
                            (box_.left - (context.problem.full_board_bounds.left + inset)).abs()
                        }
                        "right" => {
                            (box_.right - (context.problem.full_board_bounds.right - inset)).abs()
                        }
                        "top" => (box_.top - (context.problem.full_board_bounds.top + inset)).abs(),
                        _ => {
                            (box_.bottom - (context.problem.full_board_bounds.bottom - inset)).abs()
                        }
                    };
                    let exact = if edge.as_ref() == "left" || edge.as_ref() == "right" {
                        intent.y.map(|y| (center.y - y).abs())
                    } else {
                        intent.x.map(|x| (center.x - x).abs())
                    };
                    distance * 800.0 + exact.unwrap_or(0.0) * 35.0
                })
                .fold(f32::INFINITY, f32::min)
        })
        .sum()
}

fn edge_place_violation(primitive: &WorkingPrimitive, context: &Context) -> f32 {
    let Some(intent) = &primitive.primitive.edge_place else {
        return 0.0;
    };
    let box_ = packing_box(primitive);
    let tolerance = 0.05f32.max(context.problem.grid * 0.51);
    let best = intent
        .edges
        .iter()
        .map(|edge| {
            let inset = intent.inset.unwrap_or(0.0).max(0.0);
            match edge.as_ref() {
                "left" => (box_.left - (context.problem.full_board_bounds.left + inset)).abs(),
                "right" => (box_.right - (context.problem.full_board_bounds.right - inset)).abs(),
                "top" => (box_.top - (context.problem.full_board_bounds.top + inset)).abs(),
                _ => (box_.bottom - (context.problem.full_board_bounds.bottom - inset)).abs(),
            }
        })
        .fold(f32::INFINITY, f32::min);
    (best - tolerance).max(0.0) * 8.0
}

fn board_slot_centers(width: f32, height: f32, bounds: &Box2) -> Vec<Point> {
    let left = bounds.left + width / 2.0;
    let right = bounds.right - width / 2.0;
    let top = bounds.top + height / 2.0;
    let bottom = bounds.bottom - height / 2.0;
    let center = box_center(bounds);
    vec![
        center,
        Point { x: left, y: top },
        Point { x: right, y: top },
        Point { x: left, y: bottom },
        Point {
            x: right,
            y: bottom,
        },
        Point {
            x: center.x,
            y: top,
        },
        Point {
            x: center.x,
            y: bottom,
        },
        Point {
            x: left,
            y: center.y,
        },
        Point {
            x: right,
            y: center.y,
        },
    ]
}

fn needs_layer_slots(moving: &WorkingPrimitive) -> bool {
    matches!((layer_envelope(moving, "top"), layer_envelope(moving, "bottom")),
        (Some(top), Some(bottom)) if top != bottom)
}

// Configuration-space obstacles for the packing centre. Each face uses its
// own moving envelope, including its offset from that centre. Never project a
// bottom body into a top-only gap (or assume the two envelopes are concentric).
fn layer_center_obstacles(
    moving: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Box2> {
    let center = box_center(&packing_box(moving));
    let mut result = Vec::new();
    for layer in ["top", "bottom"] {
        let Some(envelope) = layer_envelope(moving, layer) else { continue; };
        for other in placed {
            if !primitive_can_conflict(moving, other, context) { continue; }
            let clearance = primitive_clearance(moving, other, context) + 0.001;
            for occupied in occupied_boxes_on_layer(other, layer) {
                result.push(Box2 {
                    left: occupied.left - clearance - (envelope.right - center.x),
                    right: occupied.right + clearance - (envelope.left - center.x),
                    top: occupied.top - clearance - (envelope.bottom - center.y),
                    bottom: occupied.bottom + clearance - (envelope.top - center.y),
                });
            }
        }
    }
    result
}

// Fine search supplements the unchanged coarse beam portfolio. Removing or
// replacing coarse poses can destabilize packing of unrelated compound blocks.
fn layer_slot_centers(moving: &WorkingPrimitive, placed: &[WorkingPrimitive], context: &Context) -> Vec<Point> {
    let pack = packing_box(moving);
    let center = box_center(&pack);
    let width = pack.right - pack.left;
    let height = pack.bottom - pack.top;
    let mut occupied = layer_center_obstacles(moving, placed, context);
    occupied.extend(context.problem.obstacles.iter().map(|box_| Box2 {
        left: box_.left - context.problem.clearance - width / 2.0 - 0.001,
        right: box_.right + context.problem.clearance + width / 2.0 + 0.001,
        top: box_.top - context.problem.clearance - height / 2.0 - 0.001,
        bottom: box_.bottom + context.problem.clearance + height / 2.0 + 0.001,
    }));
    let bounds = Box2 {
        left: context.problem.bounds.left + width / 2.0,
        right: context.problem.bounds.right - width / 2.0,
        top: context.problem.bounds.top + height / 2.0,
        bottom: context.problem.bounds.bottom - height / 2.0,
    };
    let mut free = vec![bounds];
    let mut centers = Vec::new();
    for box_ in occupied {
        // Exact contacts also survive bounded free-rectangle pruning.
        centers.extend([
            Point { x: box_.left, y: center.y }, Point { x: box_.right, y: center.y },
            Point { x: center.x, y: box_.top }, Point { x: center.x, y: box_.bottom },
        ]);
        free = free.into_iter().flat_map(|rect| subtract_rect(&rect, &box_)).collect();
        if free.len() > 160 { free = prune_free_rects(free, 100); }
    }
    for rect in prune_free_rects(free, 80) {
        if rect.left <= rect.right && rect.top <= rect.bottom {
            centers.extend(board_slot_centers(0.0, 0.0, &rect));
        }
    }
    // Do not snap to the coarse grid: the J1/J2 corridor is only 0.1741 mm wide.
    centers
}

fn free_rect_slot_centers(
    moving: &WorkingPrimitive,
    width: f32,
    height: f32,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Point> {
    let mut occupied: Vec<Box2> = placed
        .iter()
        .flat_map(|primitive| relevant_occupied_boxes(primitive, moving)
            .into_iter().map(|box_| inflate_box(&box_, context.problem.clearance)))
        .collect();
    occupied.extend(
        context
            .problem
            .obstacles
            .iter()
            .map(|box_| inflate_box(box_, context.problem.clearance)),
    );
    let mut free_rects = vec![context.problem.bounds];
    for occupied_box in occupied {
        free_rects = free_rects
            .into_iter()
            .flat_map(|rect| subtract_rect(&rect, &occupied_box))
            .collect();
        if free_rects.len() > 160 {
            free_rects = prune_free_rects(free_rects, 100);
        }
    }
    let mut centers = Vec::new();
    for rect in prune_free_rects(free_rects, 80) {
        if rect.right - rect.left + 0.0001 < width || rect.bottom - rect.top + 0.0001 < height {
            continue;
        }
        let left = rect.left + width / 2.0;
        let right = rect.right - width / 2.0;
        let top = rect.top + height / 2.0;
        let bottom = rect.bottom - height / 2.0;
        let center = box_center(&rect);
        centers.extend([
            Point { x: left, y: top },
            Point { x: right, y: top },
            Point { x: left, y: bottom },
            Point {
                x: right,
                y: bottom,
            },
            Point {
                x: center.x,
                y: top,
            },
            Point {
                x: center.x,
                y: bottom,
            },
            Point {
                x: left,
                y: center.y,
            },
            Point {
                x: right,
                y: center.y,
            },
            center,
        ]);
    }
    for point in &mut centers {
        *point = snap_point(*point, context.problem.grid);
    }
    centers
}

fn subtract_rect(rect: &Box2, cut: &Box2) -> Vec<Box2> {
    let Some(overlap) = intersect_box(rect, cut) else {
        return vec![*rect];
    };
    let mut result = Vec::new();
    if overlap.left > rect.left {
        result.push(Box2 {
            left: rect.left,
            right: overlap.left,
            top: rect.top,
            bottom: rect.bottom,
        });
    }
    if overlap.right < rect.right {
        result.push(Box2 {
            left: overlap.right,
            right: rect.right,
            top: rect.top,
            bottom: rect.bottom,
        });
    }
    if overlap.top > rect.top {
        result.push(Box2 {
            left: overlap.left,
            right: overlap.right,
            top: rect.top,
            bottom: overlap.top,
        });
    }
    if overlap.bottom < rect.bottom {
        result.push(Box2 {
            left: overlap.left,
            right: overlap.right,
            top: overlap.bottom,
            bottom: rect.bottom,
        });
    }
    result
        .into_iter()
        .filter(|box_| box_.right - box_.left > 0.05 && box_.bottom - box_.top > 0.05)
        .collect()
}

fn intersect_box(a: &Box2, b: &Box2) -> Option<Box2> {
    let result = Box2 {
        left: a.left.max(b.left),
        right: a.right.min(b.right),
        top: a.top.max(b.top),
        bottom: a.bottom.min(b.bottom),
    };
    (result.left < result.right && result.top < result.bottom).then_some(result)
}

fn prune_free_rects(rects: Vec<Box2>, limit: usize) -> Vec<Box2> {
    let mut unique = Vec::new();
    let mut seen = FxHashSet::default();
    for box_ in rects {
        let rounded = Box2 {
            left: round_placement(box_.left),
            right: round_placement(box_.right),
            top: round_placement(box_.top),
            bottom: round_placement(box_.bottom),
        };
        let key = (
            rounded.left.to_bits(),
            rounded.right.to_bits(),
            rounded.top.to_bits(),
            rounded.bottom.to_bits(),
        );
        if seen.insert(key) {
            unique.push(rounded);
        }
    }
    let snapshot = unique.clone();
    unique.retain(|item| {
        !snapshot
            .iter()
            .any(|other| other != item && box_contains(other, item))
    });
    unique.sort_by(|a, b| compare_f32(box_area(b), box_area(a)));
    unique.truncate(limit);
    unique
}

fn box_contains(container: &Box2, item: &Box2) -> bool {
    container.left <= item.left
        && container.right >= item.right
        && container.top <= item.top
        && container.bottom >= item.bottom
}

fn inflate_box(box_: &Box2, value: f32) -> Box2 {
    Box2 {
        left: round_placement(box_.left - value),
        right: round_placement(box_.right + value),
        top: round_placement(box_.top - value),
        bottom: round_placement(box_.bottom + value),
    }
}

fn placed_slot_centers(
    moving: &WorkingPrimitive,
    width: f32,
    height: f32,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Point> {
    let mut centers = Vec::new();
    let placed_boxes: Vec<_> = placed.iter()
        .flat_map(|primitive| relevant_occupied_boxes(primitive, moving))
        .collect();
    for box_ in &placed_boxes {
        // Both tight legal slots and slots at the preferred soft gap.
        if let Some(spacing) = &context.problem.soft_spacing {
            if spacing.gap > 0.0 {
                let c = context.problem.clearance + spacing.gap;
                let x = (box_.left+box_.right)/2.0; let y = (box_.top+box_.bottom)/2.0;
                centers.extend([Point{x:box_.left-c-width/2.0,y},Point{x:box_.right+c+width/2.0,y},
                    Point{x,y:box_.top-c-height/2.0},Point{x,y:box_.bottom+c+height/2.0}]);
            }
        }

        let clearance = context.problem.clearance;
        let left = box_.left - clearance - width / 2.0;
        let right = box_.right + clearance + width / 2.0;
        let top = box_.top - clearance - height / 2.0;
        let bottom = box_.bottom + clearance + height / 2.0;
        let x = (box_.left + box_.right) / 2.0;
        let y = (box_.top + box_.bottom) / 2.0;
        centers.extend([
            Point { x: left, y },
            Point { x: right, y },
            Point { x, y: top },
            Point { x, y: bottom },
            Point { x: left, y: top },
            Point { x: right, y: top },
            Point { x: left, y: bottom },
            Point {
                x: right,
                y: bottom,
            },
        ]);
    }
    let x_values: Vec<_> = placed_boxes
        .iter()
        .flat_map(|box_| {
            [
                box_.left - context.problem.clearance - width / 2.0,
                box_.right + context.problem.clearance + width / 2.0,
                (box_.left + box_.right) / 2.0,
            ]
        })
        .collect();
    let y_values: Vec<_> = placed_boxes
        .iter()
        .flat_map(|box_| {
            [
                box_.top - context.problem.clearance - height / 2.0,
                box_.bottom + context.problem.clearance + height / 2.0,
                (box_.top + box_.bottom) / 2.0,
            ]
        })
        .collect();
    let x_slots = slot_positions(
        context.problem.bounds.left + width / 2.0,
        context.problem.bounds.right - width / 2.0,
        &x_values,
    );
    let y_slots = slot_positions(
        context.problem.bounds.top + height / 2.0,
        context.problem.bounds.bottom - height / 2.0,
        &y_values,
    );
    for x in x_slots {
        for y in &y_slots {
            centers.push(Point { x, y: *y });
        }
    }
    for point in &mut centers {
        *point = snap_point(*point, context.problem.grid);
    }
    centers
}

fn edge_place_centers(
    primitive: &WorkingPrimitive,
    width: f32,
    height: f32,
    context: &Context,
) -> Vec<Point> {
    let Some(intent) = &primitive.primitive.edge_place else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for edge in intent.edges.iter() {
        let inset = intent.inset.unwrap_or(0.0).max(0.0);
        for cross in edge_cross_coordinates(edge, width, height, intent, context) {
            let point = match edge.as_ref() {
                "left" => Point {
                    x: context.problem.full_board_bounds.left + inset + width / 2.0,
                    y: cross,
                },
                "right" => Point {
                    x: context.problem.full_board_bounds.right - inset - width / 2.0,
                    y: cross,
                },
                "top" => Point {
                    x: cross,
                    y: context.problem.full_board_bounds.top + inset + height / 2.0,
                },
                _ => Point {
                    x: cross,
                    y: context.problem.full_board_bounds.bottom - inset - height / 2.0,
                },
            };
            result.push(snap_point(point, context.problem.grid));
        }
    }
    result
}

fn edge_cross_coordinates(
    edge: &str,
    width: f32,
    height: f32,
    intent: &crate::model::EdgePlaceIntent,
    context: &Context,
) -> Vec<f32> {
    let offset = intent.offset.unwrap_or(0.0);
    let exact = if edge == "left" || edge == "right" {
        intent.y
    } else {
        intent.x
    };
    if let Some(exact) = exact {
        return vec![exact + offset];
    }
    let (min, max) = if edge == "left" || edge == "right" {
        (
            context.problem.bounds.top + height / 2.0,
            context.problem.bounds.bottom - height / 2.0,
        )
    } else {
        (
            context.problem.bounds.left + width / 2.0,
            context.problem.bounds.right - width / 2.0,
        )
    };
    if max < min {
        return vec![(min + max) / 2.0 + offset];
    }
    let start = min + offset;
    let end = max + offset;
    let center = (min + max) / 2.0 + offset;
    let preferred = match intent.align.as_deref() {
        Some("start") => start,
        Some("end") => end,
        _ => center,
    };
    let mut values = vec![preferred, center, start, end];
    for index in 1..8 {
        values.push(min + (max - min) * index as f32 / 8.0 + offset);
    }
    let mut seen = FxHashSet::default();
    values.retain(|value| value.is_finite() && seen.insert(value.clamp(min, max).to_bits()));
    for value in &mut values {
        *value = value.clamp(min, max);
    }
    values.sort_by(|a, b| compare_f32((a - preferred).abs(), (b - preferred).abs()));
    values
}

fn relation_slot_centers(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Point> {
    let mut result = Vec::new();
    let primitive_center = box_center(&packing_box(primitive));
    for relation in context.relations.iter() {
        if relation.skip {
            continue;
        }
        if let (Some(moving), Some(mut target)) = (
            endpoint_point(std::slice::from_ref(primitive), relation.from),
            endpoint_point(placed, relation.to),
        ) {
            apply_relation_target_offset(&mut target.0, relation);
            result.extend(endpoint_centers(
                primitive_center,
                moving.0,
                target.0,
                relation,
                context,
            ));
        }
        if let (Some(moving), Some(mut target)) = (
            endpoint_point(std::slice::from_ref(primitive), relation.to),
            endpoint_point(placed, relation.from),
        ) {
            apply_relation_target_offset(&mut target.0, relation);
            result.extend(endpoint_centers(
                primitive_center,
                moving.0,
                target.0,
                relation,
                context,
            ));
        }
    }
    for point in &mut result {
        *point = snap_point(*point, context.problem.grid);
    }
    result
}

fn apply_relation_target_offset(point: &mut Point, relation: &CompiledRelation) {
    if !relation.satellite_anchor {
        return;
    }
    if let Some(offset) = relation.anchor_offset {
        point.x = round_placement(point.x + offset.x);
        point.y = round_placement(point.y + offset.y);
    }
}

fn endpoint_centers(
    center: Point,
    moving: Point,
    target: Point,
    relation: &CompiledRelation,
    context: &Context,
) -> Vec<Point> {
    let offset = Point {
        x: moving.x - center.x,
        y: moving.y - center.y,
    };
    let clearance = context
        .problem
        .clearance
        .max(relation.min_distance.unwrap_or(0.0));
    let directions: Vec<Point> = match relation.side {
        Some(Side::Left) => vec![
            Point { x: -1.0, y: 0.0 },
            Point { x: 0.0, y: 1.0 },
            Point { x: 0.0, y: -1.0 },
            Point { x: 1.0, y: 0.0 },
        ],
        Some(Side::Right) => vec![
            Point { x: 1.0, y: 0.0 },
            Point { x: 0.0, y: 1.0 },
            Point { x: 0.0, y: -1.0 },
            Point { x: -1.0, y: 0.0 },
        ],
        Some(Side::Top) => vec![
            Point { x: 0.0, y: -1.0 },
            Point { x: 1.0, y: 0.0 },
            Point { x: -1.0, y: 0.0 },
            Point { x: 0.0, y: 1.0 },
        ],
        Some(Side::Bottom) => vec![
            Point { x: 0.0, y: 1.0 },
            Point { x: 1.0, y: 0.0 },
            Point { x: -1.0, y: 0.0 },
            Point { x: 0.0, y: -1.0 },
        ],
        _ => vec![
            Point { x: 1.0, y: 0.0 },
            Point { x: -1.0, y: 0.0 },
            Point { x: 0.0, y: 1.0 },
            Point { x: 0.0, y: -1.0 },
            Point { x: 0.0, y: 0.0 },
        ],
    };
    directions
        .into_iter()
        .map(|direction| Point {
            x: target.x + direction.x * clearance - offset.x,
            y: target.y + direction.y * clearance - offset.y,
        })
        .collect()
}

fn coarse_grid_centers(width: f32, height: f32, bounds: &Box2, grid: f32) -> Vec<Point> {
    let left = bounds.left + width / 2.0;
    let right = bounds.right - width / 2.0;
    let top = bounds.top + height / 2.0;
    let bottom = bounds.bottom - height / 2.0;
    let mut result = Vec::with_capacity(49);
    for x in 0..7 {
        for y in 0..7 {
            result.push(snap_point(
                Point {
                    x: left + (right - left) * x as f32 / 6.0,
                    y: top + (bottom - top) * y as f32 / 6.0,
                },
                grid,
            ));
        }
    }
    result
}

fn endpoint_point(
    primitives: &[WorkingPrimitive],
    endpoint: CompiledEndpoint,
) -> Option<(Point, Option<u32>)> {
    match endpoint {
        CompiledEndpoint::Anchor(point) => Some((point, None)),
        CompiledEndpoint::Pad { primitive, point } => {
            let primitive = primitives.iter().find(|item| item.id == primitive)?;
            let point = primitive.primitive.connection_points.get(point)?;
            Some((
                Point {
                    x: point.x,
                    y: point.y,
                },
                Some(primitive.id),
            ))
        }
        CompiledEndpoint::Component { primitive, designator } => {
            let primitive=primitives.iter().find(|p|p.id==primitive)?;
            let index=primitive.placement_ids.iter().position(|id|*id==designator)?;
            let pose=primitive.primitive.placements.get(index)?;
            Some((Point{x:pose.x,y:pose.y},Some(primitive.id)))
        }
        CompiledEndpoint::Primitive { primitive } => {
            let primitive = primitives.iter().find(|item| item.id == primitive)?;
            Some((box_center(&packing_box(primitive)), Some(primitive.id)))
        }
        CompiledEndpoint::Missing => None,
    }
}

fn compile_relations(
    problem: &BoardPackProblem,
    primitive_ids: &FxHashMap<Arc<str>, u32>,
    designator_ids: &FxHashMap<Arc<str>, u32>,
) -> Vec<CompiledRelation> {
    problem
        .relations
        .iter()
        .map(|relation| CompiledRelation {
            from: compile_endpoint(&relation.from, problem, primitive_ids, designator_ids),
            to: compile_endpoint(&relation.to, problem, primitive_ids, designator_ids),
            skip: relation.effect.as_ref() == "lock" || relation.kind.as_ref() == "net" || crate::conditions::applies(relation),
            hard: relation.hard,
            weight: relation_weight(relation),
            inverse_tau_squared: crate::relation_score::for_relation(relation).inverse_tau_squared,
            max_distance: crate::relation_score::for_relation(relation).max,
            min_distance: crate::relation_score::for_relation(relation).min,
            satellite_anchor: relation.satellite_anchor,
            anchor_offset: relation.anchor_offset,
            side: compile_side(relation.side_preference.as_deref()),
        })
        .collect()
}

/// Authored block ownership survives island/satellite folding in component
/// metadata. Keep the group's center attached to its rigid owner, using the
/// existing CPU/GPU point transforms rather than the parent's packing center.
fn attach_block_relation_points(problem: &mut BoardPackProblem) -> Result<(), String> {
    crate::group_endpoints::attach(&mut problem.primitives, &problem.relations, &problem.relation_groups,
        &problem.components.iter().map(|c| (&c.designator, &c.primitive_id, &c.block_name, c.body_box)).collect::<Vec<_>>())?;
    crate::conditions::attach(&mut problem.primitives, &problem.relations,
        &problem.components.iter().map(|c| (&c.designator,&c.primitive_id,&c.block_name,&c.layer,c.hint_box.unwrap_or(c.body_box))).collect::<Vec<_>>());
    Ok(())
}

fn compile_endpoint(
    endpoint: &str,
    problem: &BoardPackProblem,
    primitive_ids: &FxHashMap<Arc<str>, u32>,
    designator_ids: &FxHashMap<Arc<str>, u32>,
) -> CompiledEndpoint {
    if endpoint.starts_with("block:") || endpoint.starts_with("module:") {
        for primitive in &problem.primitives {
            if let Some(point) = primitive.connection_points.iter().position(|p| p.reference.as_ref() == endpoint) {
                return CompiledEndpoint::Pad { primitive: primitive_ids[&primitive.id], point };
            }
        }
    }
    if let Some(designator) = endpoint.strip_prefix("component:") {
        for primitive in &problem.primitives {
            if primitive
                .placements
                .iter()
                .any(|placement| placement.designator.as_ref() == designator)
            {
                let Some(&designator) = designator_ids.get(designator) else {
                    break;
                };
                return CompiledEndpoint::Component {
                    primitive: primitive_ids[&primitive.id],
                    designator,
                };
            }
        }
    }
    if let Some(anchor) = endpoint.strip_prefix("anchor:") {
        return anchor_point(anchor, &problem.bounds)
            .map(CompiledEndpoint::Anchor)
            .unwrap_or(CompiledEndpoint::Missing);
    }
    if let Some(reference) = endpoint.strip_prefix("pad:") {
        for primitive in &problem.primitives {
            if let Some(point) = primitive
                .connection_points
                .iter()
                .position(|point| point.reference.as_ref() == reference)
            {
                return CompiledEndpoint::Pad {
                    primitive: primitive_ids[&primitive.id],
                    point,
                };
            }
        }
    }

    let source = endpoint
        .strip_prefix("block:")
        .map(|name| format!("tree:block:{name}"))
        .or_else(|| {
            endpoint
                .strip_prefix("module:")
                .map(|name| format!("tree:module:{name}"))
        });
    if let Some(source) = source {
        for primitive in &problem.primitives {
            if primitive
                .source_node_ids
                .iter()
                .any(|id| id.as_ref() == source)
            {
                return CompiledEndpoint::Primitive {
                    primitive: primitive_ids[&primitive.id],
                };
            }
        }
    }
    CompiledEndpoint::Missing
}

fn compile_side(side: Option<&str>) -> Option<Side> {
    match side {
        Some("left") => Some(Side::Left),
        Some("right") => Some(Side::Right),
        Some("top") => Some(Side::Top),
        Some("bottom") => Some(Side::Bottom),
        _ => None,
    }
}

fn anchor_point(anchor: &str, bounds: &Box2) -> Option<Point> {
    if !anchor.starts_with("board.") {
        return None;
    }
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

fn seed_rank(primitive: &WorkingPrimitive, context: &Context) -> f32 {
    let area = box_area(&packing_box(primitive));
    let board_area = box_area(&context.problem.bounds).max(1.0);
    let relation_count = context
        .relations
        .iter()
        .filter(|relation| touches(primitive, relation.from) || touches(primitive, relation.to))
        .count();
    let label = primitive.primitive.label.to_ascii_lowercase();
    let label_weight = [
        "connector",
        "usb",
        "jack",
        "terminal",
        "mount",
        "hole",
        "power",
        "buck",
        "reg",
        "mcu",
        "core",
        "main",
    ]
    .iter()
    .any(|word| label.contains(word));
    (if primitive.primitive.locked {
        50_000.0
    } else {
        0.0
    }) + if primitive.primitive.edge_place.is_some() {
        20_000.0
    } else {
        0.0
    } + area / board_area * 8_000.0
        + primitive.primitive.width.max(primitive.primitive.height) * 45.0
        + relation_count as f32 * 90.0
        + if primitive.primitive.kind.as_ref() == "module" {
            700.0
        } else if primitive.primitive.kind.as_ref() == "block" {
            500.0
        } else {
            0.0
        }
        + if label_weight { 250.0 } else { 0.0 }
}

fn choose_next(state: &SearchState, context: &Context) -> usize {
    let mut best = 0;
    let mut best_rank = f32::NEG_INFINITY;
    for (index, primitive) in state.remaining.iter().enumerate() {
        let placed_relations = context
            .relations
            .iter()
            .filter(|relation| {
                (touches(primitive, relation.from) || touches(primitive, relation.to))
                    && state.placed.iter().any(|placed| {
                        touches(placed, relation.from) || touches(placed, relation.to)
                    })
            })
            .map(|relation| relation.weight)
            .sum::<f32>();
        let rank = seed_rank(primitive, context) + placed_relations * 80.0;
        if rank > best_rank {
            best = index;
            best_rank = rank;
        }
    }
    best
}

fn touches(primitive: &WorkingPrimitive, endpoint: CompiledEndpoint) -> bool {
    match endpoint {
        CompiledEndpoint::Pad {
            primitive: owner, ..
        }
        | CompiledEndpoint::Component {
            primitive: owner, ..
        }
        | CompiledEndpoint::Primitive { primitive: owner } => primitive.id == owner,
        CompiledEndpoint::Anchor(_) | CompiledEndpoint::Missing => false,
    }
}

fn relation_weight(relation: &Relation) -> f32 {
    crate::relation_score::for_relation(relation).weight
}


fn side_penalty(side: Side, from: Point, to: Point, weight: f32) -> f32 {
    let dx = from.x - to.x;
    let dy = from.y - to.y;
    let length = (dx * dx + dy * dy).sqrt();
    let actual = if length > 0.000001 {
        Point {
            x: dx / length,
            y: dy / length,
        }
    } else {
        Point { x: 0.0, y: 0.0 }
    };
    let desired = match side {
        Side::Left => Point { x: -1.0, y: 0.0 },
        Side::Right => Point { x: 1.0, y: 0.0 },
        Side::Top => Point { x: 0.0, y: -1.0 },
        Side::Bottom => Point { x: 0.0, y: 1.0 },
    };
    (1.0 - actual.x * desired.x - actual.y * desired.y).max(0.0) * weight * 120.0
}

fn fit_to_bounds(primitive: &mut WorkingPrimitive, context: &Context) {
    let box_ = packing_box(primitive);
    let base = if let Some(intent) = &primitive.primitive.edge_place {
        inset_box(
            &context.problem.full_board_bounds,
            intent.inset.unwrap_or(0.0).max(0.0),
        )
    } else {
        context.problem.bounds
    };
    let full = context.problem.full_board_bounds;
    let overflow = primitive.components.iter().fold(
        crate::model::BoardOverflow::default(),
        |mut result, (_, component)| {
            result.left = result.left.max(component.board_overflow.left);
            result.right = result.right.max(component.board_overflow.right);
            result.top = result.top.max(component.board_overflow.top);
            result.bottom = result.bottom.max(component.board_overflow.bottom);
            result
        },
    );
    let bounds = Box2 {
        left: if overflow.left > 0.0 {
            full.left - overflow.left
        } else {
            base.left
        },
        right: if overflow.right > 0.0 {
            full.right + overflow.right
        } else {
            base.right
        },
        top: if overflow.top > 0.0 {
            full.top - overflow.top
        } else {
            base.top
        },
        bottom: if overflow.bottom > 0.0 {
            full.bottom + overflow.bottom
        } else {
            base.bottom
        },
    };
    let dx = if box_.left < bounds.left {
        bounds.left - box_.left
    } else if box_.right > bounds.right {
        bounds.right - box_.right
    } else {
        0.0
    };
    let dy = if box_.top < bounds.top {
        bounds.top - box_.top
    } else if box_.bottom > bounds.bottom {
        bounds.bottom - box_.bottom
    } else {
        0.0
    };
    if dx != 0.0 || dy != 0.0 {
        translate_primitive(primitive, round_placement(dx), round_placement(dy));
    }
}

fn slot_positions(min: f32, max: f32, values: &[f32]) -> Vec<f32> {
    if max < min {
        return Vec::new();
    }
    let center = (min + max) / 2.0;
    let mut result = vec![min, max, center];
    result.extend(values.iter().copied());
    let mut seen = FxHashSet::default();
    result.retain(|value| {
        value.is_finite() && seen.insert(round_placement(value.clamp(min, max)).to_bits())
    });
    for value in &mut result {
        *value = round_placement(value.clamp(min, max));
    }
    result.sort_by(|a, b| compare_f32((a - center).abs(), (b - center).abs()));
    result.truncate(18);
    result
}

fn move_packing_center(primitive: &mut WorkingPrimitive, center: Point) {
    let current = box_center(&packing_box(primitive));
    translate_primitive(
        primitive,
        round_placement(center.x - current.x),
        round_placement(center.y - current.y),
    );
}

fn packing_boxes(primitive: &WorkingPrimitive) -> &[Box2] {
    &primitive.primitive.collision_boxes
}
// `through_hole_boxes` also carries bottom-side silk and SMD pads on a
// top-mounted footprint (and vice versa). Keep the existing component shape.
fn occupied_boxes_on_layer(primitive: &WorkingPrimitive, layer: &str) -> Vec<Box2> {
    if primitive.components.is_empty() {
        return packing_boxes(primitive).to_vec();
    }
    primitive.components.iter().flat_map(|(_, component)| {
        let mut boxes = if component.layer.as_ref() == layer {
            vec![component.body_box]
        } else {
            Vec::new()
        };
        if component.layer.as_ref() != layer {
            boxes.extend(component.through_hole_boxes.iter().copied());
        }
        boxes
    }).collect()
}
fn layer_envelope(primitive: &WorkingPrimitive, layer: &str) -> Option<Box2> {
    let mut result: Option<Box2> = None;
    let mut add = |box_: Box2| {
        result = Some(match result {
            Some(current) => Box2 { left: current.left.min(box_.left), right: current.right.max(box_.right),
                top: current.top.min(box_.top), bottom: current.bottom.max(box_.bottom) },
            None => box_,
        });
    };
    if primitive.components.is_empty() {
        for box_ in packing_boxes(primitive) { add(*box_); }
    } else {
        for (_, component) in primitive.components.iter() {
            if component.layer.as_ref() == layer { add(component.body_box); }
            else { for box_ in component.through_hole_boxes.iter() { add(*box_); } }
        }
    }
    result
}
/// The same union as `layer_envelope`, for boxes already selected by layer.
pub(crate) fn envelope(boxes: impl IntoIterator<Item = Box2>) -> Option<Box2> {
    boxes.into_iter().reduce(|current, box_| Box2 { left: current.left.min(box_.left), right: current.right.max(box_.right),
        top: current.top.min(box_.top), bottom: current.bottom.max(box_.bottom) })
}
fn relevant_occupied_boxes(placed: &WorkingPrimitive, moving: &WorkingPrimitive) -> Vec<Box2> {
    if moving.components.is_empty() { return packing_boxes(placed).to_vec(); }
    ["top", "bottom"].into_iter()
        .filter(|layer| layer_envelope(moving, layer).is_some())
        .flat_map(|layer| occupied_boxes_on_layer(placed, layer))
        .collect()
}
fn packing_box(primitive: &WorkingPrimitive) -> Box2 {
    #[cfg(feature = "gpu")]
    if let Some(pose)=primitive.gpu_pose.as_ref().filter(|p|p.deferred) {return pose.packing_bounds;}
    union_boxes(packing_boxes(primitive))
}
fn inset_box(box_: &Box2, value: f32) -> Box2 {
    Box2 {
        left: box_.left + value,
        right: box_.right - value,
        top: box_.top + value,
        bottom: box_.bottom - value,
    }
}
fn box_area(box_: &Box2) -> f32 {
    (box_.right - box_.left).max(0.0) * (box_.bottom - box_.top).max(0.0)
}
fn snap_point(point: Point, grid: f32) -> Point {
    if grid <= 0.0 {
        Point {
            x: round_placement(point.x),
            y: round_placement(point.y),
        }
    } else {
        Point {
            x: round_placement(crate::geometry::js_round(point.x / grid) * grid),
            y: round_placement(crate::geometry::js_round(point.y / grid) * grid),
        }
    }
}

fn dedupe_points(points: &mut Vec<Point>) {
    let mut seen = FxHashSet::default();
    points.retain(|point| seen.insert((number_key(point.x), number_key(point.y))));
}
fn dedupe_candidates(candidates: &mut Vec<RankedCandidate>) {
    let mut seen = FxHashSet::default();
    candidates.retain(|candidate| seen.insert(primitive_key(&candidate.primitive)));
}
fn dedupe_states(states: Vec<SearchState>) -> Vec<SearchState> {
    let mut seen = FxHashSet::default();
    states
        .into_iter()
        .filter(|state| {
            let mut keys: Vec<_> = state.placed.iter().map(primitive_key).collect();
            keys.sort();
            let mut remaining: Vec<_> = state
                .remaining
                .iter()
                .map(|primitive| primitive.id)
                .collect();
            remaining.sort();
            seen.insert(SearchStateKey {
                remaining,
                placed: keys,
            })
        })
        .collect()
}
fn primitive_key(primitive: &WorkingPrimitive) -> PrimitiveKey {
    let mut placements: Vec<_> = primitive
        .primitive
        .placements
        .iter()
        .zip(primitive.placement_ids.iter())
        .map(|(placement, designator)| {
            let (mut x,mut y)=(placement.x,placement.y);
            #[cfg(feature = "gpu")]
            if let Some(pose)=primitive.gpu_pose.as_ref().filter(|p|p.deferred) {
                // Deferred placements are the last materialized state, whereas
                // the template plus ordered translations is the exact origin.
                let original=pose.template.primitive.placements.iter().find(|p|p.designator==placement.designator).unwrap();
                x=compact::coordinate(pose,original.x,0);y=compact::coordinate(pose,original.y,1);
            }
            PlacementKey {
            designator: *designator,
            x: number_key(x),
            y: number_key(y),
            rotation: placement.rotate,
            layer: layer_key(&placement.layer),
        }})
        .collect();
    placements.sort();
    PrimitiveKey {
        id: primitive.id,
        left: number_key(primitive.primitive.bbox.left),
        top: number_key(primitive.primitive.bbox.top),
        right: number_key(primitive.primitive.bbox.right),
        bottom: number_key(primitive.primitive.bbox.bottom),
        placements,
    }
}

fn lexical_ids(values: impl IntoIterator<Item = Arc<str>>) -> FxHashMap<Arc<str>, u32> {
    let values: BTreeSet<_> = values.into_iter().collect();
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| (value, index as u32))
        .collect()
}

fn number_key(value: f32) -> u32 {
    if value == 0.0 {
        0
    } else {
        value.to_bits()
    }
}

fn pose_key(primitive: &WorkingPrimitive) -> PoseKey {
    PoseKey {
        primitive: primitive.id,
        rotation: primitive.rotation,
        left: number_key(primitive.primitive.bbox.left),
        top: number_key(primitive.primitive.bbox.top),
    }
}

fn pose_pair_key(a: &WorkingPrimitive, b: &WorkingPrimitive) -> PosePairKey {
    let a = pose_key(a);
    let b = pose_key(b);
    if a <= b {
        PosePairKey(a, b)
    } else {
        PosePairKey(b, a)
    }
}

fn cache_insert<K, V>(cache: &RefCell<FxHashMap<K, V>>, key: K, value: V)
where
    K: Eq + std::hash::Hash,
{
    let mut cache = cache.borrow_mut();
    if cache.len() < GEOMETRY_CACHE_LIMIT {
        cache.insert(key, value);
    }
}

fn layer_key(layer: &str) -> u8 {
    match layer {
        "top" => 0,
        "bottom" => 1,
        "multi" => 2,
        _ => 3,
    }
}
fn candidate_limit(search_width: usize, remaining: usize) -> usize {
    28.max(120.min((search_width * 3).div_ceil(remaining.max(1))))
}
fn compare_f32(a: f32, b: f32) -> Ordering {
    a.total_cmp(&b)
}
fn compare_rank(a: &Rank, b: &Rank) -> Ordering {
    a.hard_count
        .cmp(&b.hard_count)
        .then_with(|| compare_f32(a.hard_severity, b.hard_severity))
        .then_with(|| compare_f32(a.score, b.score))
}
pub(crate) fn rank_improves(rank: &Rank, best: &Rank) -> bool {
    rank.hard_count < best.hard_count
        || (rank.hard_count == best.hard_count && rank.hard_severity + 0.001 < best.hard_severity)
        || (rank.hard_count == best.hard_count && (rank.hard_severity - best.hard_severity).abs() <= 0.001
            && rank.score + 0.001 < best.score)
}
fn compare_states(a: &SearchState, b: &SearchState) -> Ordering {
    compare_rank(&a.rank, &b.rank).then(a.ordinal.cmp(&b.ordinal))
}
fn compare_candidates(a: &RankedCandidate, b: &RankedCandidate) -> Ordering {
    compare_rank(&a.rank, &b.rank).then(a.ordinal.cmp(&b.ordinal))
}

fn soft_spacing_penalty(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(7);
    let Some(spacing) = &context.problem.soft_spacing else { return 0.0; };
    if spacing.gap <= 0.0 { return 0.0; }
    let mut score = 0.0;
    for i in 0..primitives.len() { for j in i+1..primitives.len() {
        let a = &primitives[i]; let b = &primitives[j];
        if (a.primitive.locked && b.primitive.locked) || !primitive_can_conflict(a,b,context) { continue; }
        if spacing.exempt_pairs.iter().any(|p| (p[0]==a.primitive.id && p[1]==b.primitive.id)
            || (p[1]==a.primitive.id && p[0]==b.primitive.id)) { continue; }
        score += spacing_term(|layer| Some((layer_envelope(a, LAYERS[layer])?, layer_envelope(b, LAYERS[layer])?)),
            context.problem.clearance, spacing.gap);
    }}
    score
}

const LAYERS: [&str; 2] = ["top", "bottom"];
/// Soft spacing deficit of one primitive pair; `envelopes(layer)` gives both
/// primitives' envelopes on top (0) and bottom (1), when both occupy it.
pub(crate) fn spacing_term(envelopes: impl Fn(usize) -> Option<(Box2, Box2)>, clearance: f32, gap: f32) -> f32 {
    let closest = (0..2).filter_map(|layer| {
        let (a, b) = envelopes(layer)?;
        let dx = (a.left-b.right).max(b.left-a.right).max(0.0);
        let dy = (a.top-b.bottom).max(b.top-a.bottom).max(0.0);
        Some(crate::numerics::hypot(dx,dy))
    }).fold(f32::INFINITY, f32::min);
    if !closest.is_finite() { return 0.0; }
    let deficit = (clearance + gap - closest).max(0.0);
    18.0 * deficit * deficit
}


fn alignment_center(p: &WorkingPrimitive, anchor: &Option<Arc<str>>) -> Point {
    anchor.as_ref().and_then(|name| p.primitive.placements.iter().find(|q| &q.designator == name))
        .map_or_else(|| box_center(&p.primitive.bbox), |q| Point { x:q.x, y:q.y })
}

fn soft_alignment_score(primitives: &[WorkingPrimitive], context: &Context) -> f32 {
    let _profile = context.profile.span(8);
    let Some(s) = &context.problem.soft_alignment else { return 0.0; };
    alignment_score(primitives.iter().map(|p| &p.primitive), s)
}

fn primitive_alignment_center(p: &Primitive, anchor: &Option<Arc<str>>) -> Point {
    anchor.as_ref().and_then(|name| p.placements.iter().find(|q| &q.designator == name))
        .map_or_else(|| box_center(&p.bbox), |q| Point { x:q.x, y:q.y })
}

/// Minimal primitive for `alignment_score`, which reads ids, bboxes and placements.
pub(crate) fn alignment_primitive(id: Arc<str>, bbox: Box2, placements: Vec<crate::model::Placement>) -> Primitive {
    let empty: Arc<str> = Arc::from("");
    Primitive { id, kind: empty.clone(), label: empty.clone(), source_node_id: empty, source_node_ids: Arc::new(vec![]), locked: false,
        can_rotate: false, allowed_orientations: Arc::new(vec![0]), bbox, collision_boxes: Arc::new(vec![bbox]), collision_box_layers: Arc::new(vec![]),
        width: bbox.right - bbox.left, height: bbox.bottom - bbox.top, placements: Arc::new(placements), connection_points: Arc::new(vec![]),
        connection_point_layers: Arc::new(vec![]), path_ports: Arc::new(vec![]), edge_place: None }
}

pub fn alignment_score<'a>(primitives: impl Clone + Iterator<Item=&'a Primitive>, s: &crate::model::SoftAlignment) -> f32 {
    let mut score=0.0;
    for pair in &s.pairs {
        let Some(a)=primitives.clone().find(|p| p.id==pair.a) else { continue; };
        let Some(b)=primitives.clone().find(|p| p.id==pair.b) else { continue; };
        let ac=primitive_alignment_center(a,&pair.anchor_a); let bc=primitive_alignment_center(b,&pair.anchor_b);
        let error=((ac.x-bc.x).abs().min((ac.y-bc.y).abs())-s.tolerance).max(0.0);
        score+=s.weight*pair.similarity*if error<=1.0 {error*error/2.0} else {error-0.5};
        if let Some(o) = &pair.orientation {
            if let (Some(ap),Some(bp))=(a.placements.iter().find(|p| p.designator==o.a),b.placements.iter().find(|p| p.designator==o.b)) {
                let angle=ap.rotate as f32-bp.rotate as f32-o.offset;
                score+=s.orientation_weight*pair.similarity*(1.0-crate::rotation::cos_degrees(angle))/2.0;
            }
        }
    }
    score
}

// These are ordinary candidates, not constraints. Use transformed component
// centers so rotating an asymmetric block does not align its bbox by mistake.
fn alignment_slot_centers(p: &WorkingPrimitive, placed: &[WorkingPrimitive], context: &Context) -> Vec<Point> {
    let Some(s) = &context.problem.soft_alignment else { return Vec::new(); };
    let mut centers=Vec::new();
    let pb=packing_box(p); let pc=box_center(&pb);
    let width=pb.right-pb.left; let height=pb.bottom-pb.top;
    for pair in &s.pairs {
        let (other,anchor,other_anchor)=if p.primitive.id==pair.a { (&pair.b,&pair.anchor_a,&pair.anchor_b) }
            else if p.primitive.id==pair.b { (&pair.a,&pair.anchor_b,&pair.anchor_a) } else { continue; };
        let Some(q)=placed.iter().find(|q| &q.primitive.id==other) else { continue; };
        let a=alignment_center(p,anchor); let b=alignment_center(q,other_anchor);
        let x=pc.x+b.x-a.x; let y=pc.y+b.y-a.y;
        let qb=packing_box(q);
        let comfort=context.problem.soft_spacing.as_ref().map_or(0.0,|s| s.gap);
        for extra in [0.0,comfort,2.0] {
            let gap=context.problem.clearance+extra;
            centers.extend([Point{x,y:qb.top-gap-height/2.0},Point{x,y:qb.bottom+gap+height/2.0},
                Point{x:qb.left-gap-width/2.0,y},Point{x:qb.right+gap+width/2.0,y}]);
        }
        // Project available free-rectangle slots onto the shared axes. This lets
        // the packer leave room around the pair without forcing adjacency.
        for slot in free_rect_slot_centers(p,width,height,placed,context).into_iter().take(24) {
            centers.extend([Point{x,y:slot.y},Point{x:slot.x,y}]);
        }
    }
    centers
}

#[cfg(test)]
mod alignment_search_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn top_display_only_occupies_bottom_at_its_drilled_pads() {
        let (mut context, state) = fixture();
        let display_box = Box2 { left: -50.0, right: 50.0, top: -30.0, bottom: 30.0 };
        let pad_box = Box2 { left: -0.5, right: 0.5, top: -0.5, bottom: 0.5 };
        let bottom_box = Box2 { left: 19.0, right: 21.0, top: -1.0, bottom: 1.0 };
        let mut display = state.remaining[0].clone();
        display.primitive.bbox = display_box;
        display.primitive.collision_boxes = Arc::new(vec![display_box, pad_box]);
        display.components = Arc::new(vec![(0, ComponentGeometry { hint_box: None,access_rows:vec![],
            designator: Arc::from("U6"), primitive_id: Arc::from("U6"), block_name: Arc::from("display"),
            layer: Arc::from("top"), body_box: display_box, through_hole_boxes: Arc::new(vec![pad_box]),
            board_overflow: Default::default(), edge_clearance: 0.0,
        })]);
        let mut candidate = state.remaining[1].clone();
        candidate.primitive.bbox = bottom_box;
        candidate.primitive.collision_boxes = Arc::new(vec![bottom_box]);
        candidate.components = Arc::new(vec![(1, ComponentGeometry { hint_box: None,access_rows:vec![],
            designator: Arc::from("R1"), primitive_id: Arc::from("R1"), block_name: Arc::from("bottom"),
            layer: Arc::from("bottom"), body_box: bottom_box, through_hole_boxes: Arc::new(vec![]),
            board_overflow: Default::default(), edge_clearance: 0.0,
        })]);
        let problem = Arc::get_mut(&mut context.problem).unwrap();
        problem.components = vec![display.components[0].1.clone(), candidate.components[0].1.clone()];
        problem.component_pair_clearance = vec![0.0, 0.2, 0.2, 0.0];
        problem.component_conflict = vec![1, 1, 1, 1];
        assert_eq!(primitive_hard_overlap(&display, &candidate, &context), 0.0);
        assert_eq!(envelope_overlap_penalty(&[display.clone(), candidate.clone()], &context), 0.0);
        let occupied = relevant_occupied_boxes(&display, &candidate);
        assert_eq!(occupied, vec![pad_box]);
        let mut region: crate::model::ConstraintRegion = serde_json::from_value(json!({
            "name":"bottom-only", "box":bottom_box, "layers":["bottom"], "allowBlocks":[]
        })).unwrap();
        assert_eq!(component_region_overlap(&display.components[0].1, &region), 0.0);
        region.box_ = pad_box;
        assert!(component_region_overlap(&display.components[0].1, &region) > 0.0);
        region.box_ = bottom_box;
        region.layers = vec![Arc::from("top")];
        assert!(component_region_overlap(&display.components[0].1, &region) > 0.0);
        let mut near_pad = candidate.clone();
        translate_primitive(&mut near_pad, -20.0, 0.0);
        assert!(primitive_hard_overlap(&display, &near_pad, &context) > 0.0);
    }

    // Reduced J1/J2 corridor: only the 4.24 mm top pad envelope has to fit
    // between top obstacles; the 5.08 mm bottom body can pass underneath.
    fn layered_corridor() -> (Context, Vec<WorkingPrimitive>) {
        let (mut context, state) = fixture();
        let bodies = [
            Box2 { left: -2.54, right: 2.54, top: -2.0, bottom: 2.0 },
            Box2 { left: -11.0, right: -2.1241, top: -11.0, bottom: 11.0 },
            Box2 { left: 2.99, right: 11.0, top: -11.0, bottom: 11.0 },
        ];
        let pad = Box2 { left: -2.12, right: 2.12, top: -2.0, bottom: 2.0 };
        let mut primitives = state.remaining;
        for (i, p) in primitives.iter_mut().enumerate() {
            let holes = if i == 0 { vec![pad] } else { vec![] };
            p.primitive.bbox = bodies[i];
            p.primitive.collision_boxes = Arc::new(std::iter::once(bodies[i]).chain(holes.iter().copied()).collect());
            p.primitive.locked = i != 0;
            p.components = Arc::new(vec![(i, ComponentGeometry { hint_box: None,access_rows:vec![],
                designator: p.primitive.id.clone(), primitive_id: p.primitive.id.clone(), block_name: p.primitive.id.clone(),
                layer: Arc::from(if i == 0 { "bottom" } else { "top" }), body_box: bodies[i],
                through_hole_boxes: Arc::new(holes), board_overflow: Default::default(), edge_clearance: 0.0,
            })]);
        }
        let problem = Arc::get_mut(&mut context.problem).unwrap();
        problem.grid = 1.0;
        problem.clearance = 0.35;
        problem.components = primitives.iter().map(|p| p.components[0].1.clone()).collect();
        problem.component_pair_clearance = vec![0.35; 9];
        problem.component_conflict = vec![1; 9];
        (context, primitives)
    }

    #[test]
    fn coarse_grid_keeps_layer_specific_narrow_corridor_candidates() {
        let (context, primitives) = layered_corridor();
        let centers = layer_slot_centers(&primitives[0], &primitives[1..], &context);
            assert!(centers.iter().any(|center| {
                let mut p = primitives[0].clone();
                move_packing_center(&mut p, *center);
                !primitive_outside(&p, &context)
                    && primitives[1..].iter().all(|other| primitive_hard_overlap(&p, other, &context) == 0.0)
            }), "no legal candidate in 0.1741 mm corridor: {centers:?}");
    }

    #[test]
    fn repair_resolves_top_pad_contact_without_pushing_bottom_body_into_display() {
        let (context, primitives) = layered_corridor();
        assert!(primitive_hard_overlap(&primitives[0], &primitives[1], &context) > 0.0);
        let variants = separation_variants(&primitives, 0, 1, &context);
        assert!(variants.iter().any(|ps| {
            primitive_hard_overlap(&ps[0], &ps[1], &context) == 0.0
                && primitive_hard_overlap(&ps[0], &ps[2], &context) == 0.0
                && !primitive_outside(&ps[0], &context)
        }));
        // Moving the body to top makes this corridor physically impossible.
        let mut top = primitives[0].clone();
        Arc::make_mut(&mut top.components)[0].1.layer = Arc::from("top");
        context.hard_overlap_cache.borrow_mut().clear();
        for x in [0.347, 0.43, 0.519] {
            let mut p = top.clone();
            move_packing_center(&mut p, Point { x, y: 0.0 });
            assert!(primitives[1..].iter().any(|other| primitive_hard_overlap(&p, other, &context) > 0.0));
        }
    }

    fn fixture() -> (Context,SearchState) {
        let primitives:Vec<_>=(0..3).map(|i| {
            let name=format!("U{}",i+1);
            serde_json::from_value::<Primitive>(json!({"id":name,"kind":"block","label":name,"sourceNodeId":name,
                "sourceNodeIds":[name],"locked":false,"canRotate":true,"allowedOrientations":[0,90,180,270],
                "bbox":{"left":-2,"right":2,"top":-1,"bottom":1},"collisionBoxes":[{"left":-2,"right":2,"top":-1,"bottom":1}],
                "width":4,"height":2,"placements":[{"designator":name,"x":0,"y":0,"rotate":0,"layer":"top","score":0}],
                "connectionPoints":[],"pathPorts":[],"edgePlace":null})).unwrap()
        }).collect();
        let problem:BoardPackProblem=serde_json::from_value(json!({"version":7,"grid":0.5,"clearance":0.2,"searchWidth":32,
            "compactness":"normal","bounds":{"left":-12,"right":12,"top":-12,"bottom":12},
            "fullBoardBounds":{"left":-12,"right":12,"top":-12,"bottom":12},
            "boardOutline":[{"x":-12,"y":-12},{"x":12,"y":-12},{"x":12,"y":12},{"x":-12,"y":12}],
            "edgeClearance":0.2,"primitives":[],"relations":[],"obstacles":[],"constraintRegions":[],"components":[],
            "componentPairClearance":[],"componentConflict":[],"softAlignment":{"weight":8,"tolerance":0.15,"orientationWeight":24,
            "pairs":[{"a":"U1","b":"U2","anchorA":"U1","anchorB":"U2","similarity":1,"orientation":{"a":"U1","b":"U2","offset":0}}]}})).unwrap();
        let mut problem=problem;
        problem.primitives=primitives.clone();
        let context=Context{conditions:vec![],
            problem:Arc::new(problem),relations:Arc::new(vec![]),hard_overlap_cache:RefCell::new(FxHashMap::default()),
            outside_cache:RefCell::new(FxHashMap::default()),outside_severity_cache:RefCell::new(FxHashMap::default()),
            joint_candidates:Arc::new(AtomicUsize::new(0)),profile:profile::Profile::new(),
            #[cfg(feature = "gpu")] gpu:None};
        let remaining=primitives.into_iter().enumerate().map(|(i,p)|WorkingPrimitive{id:i as u32,source_index:i,
            primitive:p,placement_ids:Arc::new(vec![i as u32]),point_component_ids:Arc::new(vec![]),components:Arc::new(vec![]),rotation:0,
            #[cfg(feature = "gpu")] gpu_pose:None}).collect();
        let state=SearchState{placed:vec![],remaining,rank:state_rank(&[],&context),route_penalty:0.0,ordinal:0};
        (context,state)
    }

    fn channel_fixture() -> (Context,SearchState) {
        let (mut context,mut state)=fixture();
        for (i,p) in state.remaining.iter_mut().enumerate() {
            let ports=if i<2 {vec![json!({"pathId":format!("ch{i}"),"order":1,"ref":format!("U{}.1",i+1),"role":"target","x":0,"y":0,"normal":{"x":0,"y":-1}})]}
                else {(0..2).map(|j|json!({"pathId":format!("ch{j}"),"order":0,"ref":format!("J1.{}",j+1),"role":"source","x":if j==0 {-3}else{3},"y":0,"normal":{"x":0,"y":1}})).collect()};
            p.primitive.path_ports=Arc::new(ports.into_iter().map(|p|serde_json::from_value(p).unwrap()).collect());
        }
        let mut connector=state.remaining.pop().unwrap();connector.primitive.locked=true;
        state.placed.push(connector);
        let problem=Arc::make_mut(&mut context.problem);
        for p in state.remaining.iter().chain(state.placed.iter()) {problem.primitives[p.source_index]=p.primitive.clone();}
        problem.soft_alignment.as_mut().unwrap().orientation_weight=0.0;
        state.rank=full_state_rank(&state.placed,&context);
        (context,state)
    }

    #[test]
    fn channels_require_shared_connector_and_preserve_target_order() {
        let (context,mut state)=channel_fixture();
        let proposals=channels::proposals(&state,0,&context);
        assert!(!proposals.is_empty());
        for ps in proposals {assert_eq!(ps.len(),2);assert!(ps[0].primitive.path_ports[0].x<ps[1].primitive.path_ports[0].x);}
        Arc::make_mut(&mut state.placed[0].primitive.path_ports)[1].reference="J2.1".into();
        assert!(channels::proposals(&state,0,&context).is_empty(),"different connectors in one locked owner must not create a row");
    }

    #[test]
    fn diversity_retains_fine_elites_and_other_orientations_without_extra_hard_errors() {
        let (context,mut state)=fixture();
        state.placed.push(state.remaining.remove(0));
        let mut states=Vec::new();
        for i in 0..40 {let mut s=state.clone();translate_primitive(&mut s.placed[0],i as f32*0.002,0.0);s.rank.score=i as f32;s.ordinal=i;states.push(s);}
        let mut rotated=state.clone();rotate_primitive(&mut rotated.placed[0],90);rotated.rank.score=100.0;states.push(rotated);
        let mut shifted=state.clone();translate_primitive(&mut shifted.placed[0],1.0,0.0);shifted.rank.score=101.0;states.push(shifted);
        let mut illegal=state.clone();rotate_primitive(&mut illegal.placed[0],180);illegal.rank.hard_count=1;illegal.rank.score=0.0;states.push(illegal);
        let winners=channels::diverse(states,32,&context);
        assert_eq!(winners.len(),32);
        assert!(winners.iter().take(16).enumerate().all(|(i,s)|s.ordinal==i));
        assert!(winners.iter().any(|s|s.placed[0].rotation==90));
        assert!(winners.iter().any(|s|box_center(&s.placed[0].primitive.bbox).x==1.0));
        assert!(winners.iter().all(|s|s.rank.hard_count==0));
        let mixed=channels::select_level(vec![winners.clone(),winners],32,&context);
        assert_eq!(mixed.len(),32);
        assert_eq!(mixed.iter().filter(|s|s.placed[0].rotation==90).count(),2,
            "each inventory must preserve diversity inside its actual 16-slot quota");
    }

    #[test]
    fn atomic_pair_adds_transitions_without_removing_single_placements() {
        let (context,state)=channel_fixture();
        let pairs=joint_channel_states(&state,0,&context);
        assert!(!pairs.is_empty());
        for s in &pairs {
            assert!(s.remaining.is_empty());
            assert_eq!(s.rank.hard_count,0);
            let a=&s.placed[1].primitive.placements[0];let b=&s.placed[2].primitive.placements[0];
            assert!((a.x-b.x).abs().min((a.y-b.y).abs())<0.001);
        }
        let expanded=expand_states(&[state],32,&context);
        assert!(expanded.iter().any(|s|s.remaining.len()==1));
        assert!(expanded.iter().any(|s|s.remaining.is_empty()));
    }

    #[test]
    fn folded_block_endpoint_follows_member_geometry_without_tree_identity() {
        let (context, _) = fixture();
        let mut problem = (*context.problem).clone();
        problem.components = vec![ComponentGeometry { hint_box: None,
            designator: "U1".into(), primitive_id: "U1".into(), block_name: "divider".into(),
            layer: "top".into(), body_box: Box2 {left: 5., right: 7., top: -1., bottom: 1.},
            access_rows: vec![], through_hole_boxes: Arc::new(vec![]),
            board_overflow: Default::default(), edge_clearance: 0.,
        }];
        problem.relations = vec![serde_json::from_value(json!({
            "id":"near-divider", "kind":"hint", "from":"block:divider", "to":"anchor:board.right",
            "relation":"near", "priority":"high", "weight":140, "hard":false, "effect":"move_from", "satelliteAnchor":false
        })).unwrap()];
        attach_block_relation_points(&mut problem).unwrap();
        attach_block_relation_points(&mut problem).unwrap();
        assert_eq!(problem.primitives[0].connection_points.len(), 1);
        assert!(problem.primitives[0].connection_points[0].net.is_none());
        let (mut ps, context) = prepare_context(problem, #[cfg(feature="gpu")] None);
        let endpoint = context.relations[0].from;
        let initial = endpoint_point(&ps, endpoint).unwrap().0;
        assert_eq!((initial.x, initial.y), (6., 0.));
        let before = relation_penalty(&ps, &context);
        rotate_primitive(&mut ps[0], 180);
        let rotated = endpoint_point(&ps, endpoint).unwrap().0;
        assert_eq!((rotated.x, rotated.y), (-6., 0.));
        assert!(relation_penalty(&ps, &context) > before);
        translate_primitive(&mut ps[0], 3., 2.);
        let moved = endpoint_point(&ps, endpoint).unwrap().0;
        assert_eq!((moved.x, moved.y), (-3., 2.));
    }

    #[test]
    fn alignment_candidates_do_not_consume_the_ordinary_shortlist() {
        let (context,mut state)=fixture();
        let mut placed=state.remaining.remove(1);translate_primitive(&mut placed,4.0,3.0);
        state.placed.push(placed);
        let next=&state.remaining[0];
        let mut ordinary=Vec::new();
        for variant in orientation_variants(next) {
            for p in position_candidates(&variant,&state.placed,&context,false,false) {
                let mut all=state.placed.clone();all.push(p.clone());
                ordinary.push(RankedCandidate{primitive:p,rank:state_rank(&all,&context),route_penalty:0.0,ordinal:0,alignment:false});
            }
        }
        let expected=finish_cheap_candidates(ordinary);
        let actual=cheap_candidates(next,&state.placed,0.0,&context);
        assert!(!expected.is_empty());
        assert!(expected.iter().all(|a|actual.iter().any(|b|primitive_key(&a.primitive)==primitive_key(&b.primitive))));
        assert!(actual.iter().any(|c|c.alignment));
    }

    #[test]
    fn atomic_pair_respects_keepouts_and_disappears_without_disabling_singles() {
        let (mut context,state)=channel_fixture();
        Arc::make_mut(&mut context.problem).obstacles=vec![Box2{left:-12.0,right:12.0,top:-12.0,bottom:12.0}];
        assert!(joint_channel_states(&state,0,&context).is_empty());
        assert!(!position_candidates(&state.remaining[0],&[],&context,false,false).is_empty());
    }
}

fn condition_evaluation(primitives:&[WorkingPrimitive],context:&Context)->crate::conditions::Evaluation {
    crate::conditions::evaluate(&context.conditions,|owner,index| {
        let p=primitives.iter().find(|p|p.id==owner)?;let q=p.primitive.connection_points.get(index)?;Some(Point{x:q.x,y:q.y})
    })
}
