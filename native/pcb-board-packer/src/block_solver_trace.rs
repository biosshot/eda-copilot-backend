//! Opt-in C9 diagnostic. Observes search without changing candidates or ranking.
//! JSONL goes to stderr so concurrent workers can keep separate trace files.
use super::*;
use serde_json::{json, Value};

pub(super) fn enabled(p: &BlockSolveProblem) -> bool {
    std::env::var("PCB_BLOCK_TRACE_C9").ok().as_deref() == Some("1")
        && ["C9", "U2"].iter().all(|name| p.primitives.iter()
            .any(|q| q.placements.iter().any(|x| x.designator.as_ref() == *name)))
}

pub(super) fn is_target(p: &WorkingPrimitive) -> bool {
    p.primitive.placements.iter().any(|q| q.designator.as_ref() == "C9")
}

pub(super) fn poses(ps: &[WorkingPrimitive]) -> Value {
    json!(ps.iter().flat_map(|p| p.primitive.placements.iter()).collect::<Vec<_>>())
}

pub(super) fn emit(c: &Context, event: &str, data: Value) {
    if c.trace { eprintln!("PCB_TRACE {}", json!({"event":event,"phase":*c.trace_phase.borrow(),"data":data})); }
}

pub(super) fn row(p: &WorkingPrimitive, placed: &[WorkingPrimitive], base: f64, hard: usize, ordinal: usize, score: f64, route: f64) -> Value {
    let distances: Vec<_> = [("C9.1", "U2.6"), ("C9.2", "U2.7")].iter().map(|(a,b)| {
        let from = p.primitive.connection_points.iter().find(|cp| cp.reference.as_ref() == *a);
        let to = placed.iter().flat_map(|q| q.primitive.connection_points.iter()).find(|cp| cp.reference.as_ref() == *b);
        from.zip(to).map(|(a,b)| (a.x-b.x).hypot(a.y-b.y))
    }).collect();
    json!({"pose":p.primitive.placements,"distances":distances,"base":base,"hard":hard,"ordinal":ordinal,"score":score,"route":route})
}

pub(super) fn parts(ps: &[WorkingPrimitive], c: &Context) -> Value {
    let high = c.problem.compactness.as_ref() == "high";
    let n: usize = if c.problem.experiments.stable_net_weight { c.problem.components.len() }
        else { ps.iter().map(|p|p.primitive.placements.len()).sum() };
    let small = n > 0 && n < 5;
    let (signal,ground) = same_net_spread_penalties(ps,c,if small {None} else {Some(3)},if small {None} else {Some(4.0)});
    json!({"total":score_block(ps,c),
        "relations":scoped_relation_penalty(ps,c) * if high {0.45} else {1.0},
        "padCrossings":if c.problem.experiments.pad_crossings {direct_pad_crossing_penalty(ps,c)} else {0.0},
        "signalSpread":signal * if small {18.0} else {4.0} * if high {0.65} else {1.0},
        "groundSpread":ground * if small {2.5} else {0.15} * if high {0.65} else {1.0},
        "longNets":if c.problem.experiments.long_nets {long_local_net_penalty(ps,c)} else {0.0},
        "exposure":external_port_exposure_penalty(ps,c) * if high {0.35} else {1.0},
        "powerYield":power_yield_penalty(ps,c) * if high {0.3} else {1.0},
        "denseAccess":dense_ic_access_penalty(ps,c) * if high {0.45} else {1.0},
        "facing":port_facing_penalty(ps,c) * if high {0.55} else {1.0}})
}

pub(super) fn candidates(c: &Context, event: &str, placed: &[WorkingPrimitive], ranked: &[RankedCandidate]) {
    if !c.trace || !ranked.first().is_some_and(|r|is_target(&r.primitive)) { return; }
    let rows: Vec<_> = ranked.iter().map(|r|row(&r.primitive,placed,r.incremental.evaluation.score,r.hard_violations,r.ordinal,r.score,r.route_penalty)).collect();
    let nearest = rows.iter().filter(|r|r["hard"] == 0).min_by(|a,b| {
        let sum = |r: &Value| r["distances"].as_array().unwrap().iter().map(|n|n.as_f64().unwrap_or(f64::INFINITY)).sum::<f64>();
        compare_f64(sum(a),sum(b))
    }).and_then(|r|r["ordinal"].as_u64());
    let details: Vec<_> = ranked.iter().enumerate().filter(|(i,r)|*i == 0 || Some(r.ordinal as u64) == nearest).map(|(_,r)| {
        let mut ps=placed.to_vec(); ps.push(r.primitive.clone());
        json!({"ordinal":r.ordinal,"parts":parts(&ps,c)})
    }).collect();
    emit(c,event,json!({"placed":poses(placed),"candidates":rows,"details":details}));
}

pub(super) fn stage(c: &Context, name: &str, ps: &[WorkingPrimitive]) {
    if !c.trace { return; }
    emit(c,name,json!({"poses":poses(ps),"parts":parts(ps,c)}));
}
