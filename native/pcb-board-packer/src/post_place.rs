use crate::geometry::Point;
use crate::model::{PostPlaceNet, PostPlaceScoreProblem, RouteObstacle};
use crate::signal_path;

const EPSILON: f32 = 0.001;

pub(crate) fn layer_penalty(weight: f32, same_side: bool) -> f32 {
    crate::conditions::layer_cost(1,2,weight,same_side)
}

#[derive(Clone, Copy)]
struct Segment {
    from: usize,
    to: usize,
    length: f32,
    weight: f32,
}

pub(crate) fn route_plan(problem: &PostPlaceScoreProblem) -> crate::fast_route::plan::Plan {
    crate::fast_route::nets_plan(&problem.nets, &problem.routing_obstacles, &problem.explicit_connections,
        &[], &[], problem.route_clearance, crate::geometry::Box2 { left:0.0,right:0.0,top:0.0,bottom:0.0 })
}

pub fn score(problem: &PostPlaceScoreProblem) -> Result<f32, String> {
    score_with_route(problem, |p| route_plan(p).cpu())
}

/// `score`, with the route term supplied by an equivalent evaluator.
pub(crate) fn score_with_route(problem: &PostPlaceScoreProblem, route: impl FnOnce(&PostPlaceScoreProblem) -> f32) -> Result<f32, String> {
    validate(problem)?;
    let segments: Vec<_> = problem
        .nets
        .iter()
        .enumerate()
        .flat_map(|(index, net)| minimum_spanning_segments(index, net))
        .collect();
    let mut score = crate::pad_access::score(&problem.access_components);
    for segment in &segments {
        score += segment.length * 10.0 * segment.weight;
        score += segment.length * segment.length * 0.35 * segment.weight;
    }
    for term in &problem.distances {
        let value = distance(term.source, term.target);
        score += crate::relation_score::distance_cost(value,term.weight,term.min.unwrap_or(-1.0),term.max.unwrap_or(-1.0),term.inverse_tau_squared);
    }

    for term in &problem.clearances {
        let gap = box_clearance_gap(&term.source, &term.target);
        if gap < term.minimum {
            score += crate::conditions::clearance_cost(gap,term.minimum,term.weight);
        }
    }
    score += problem.fixed_penalties.iter().sum::<f32>();
    for term in &problem.edges {
        let kind=match term.edge.as_ref(){"left"=>3,"right"=>4,"top"=>5,_=>6};
        let a=term.source;let b=term.board;
        let distance=crate::conditions::measure(kind,a.left,a.right,a.top,a.bottom,b.left,b.right,b.top,b.bottom);
        score += distance * term.weight;
    }
    for net in &problem.nets {
        for (source, targets) in local_stretch_targets(net) {
            score += local_stretch_cost(net, source, &targets);
        }
    }
    for path in &problem.paths {
        score += signal_path::topology_penalty_for_ports(
            &path.ports,
            path.shape.as_ref() == "straight",
            &path.priority,
            path.weight,
            path.prefer_facing_pads,
        );
    }
    score += route(problem);
    if !score.is_finite() {
        return Err("post-place F32 score overflow or invalid arithmetic".into());
    }
    Ok(score)
}

#[cfg(test)]
mod numeric_result_tests {
    use super::*;
    #[test]
    fn rejects_computed_overflow_from_finite_input_without_confusing_empty_score() {
        let _env=crate::float_env::Guard::enter();
        let mut problem: PostPlaceScoreProblem=serde_json::from_value(serde_json::json!({
            "version":1,"nets":[],"distances":[],"clearances":[],"fixedPenalties":[],"edges":[],"paths":[]
        })).unwrap();
        assert_eq!(score(&problem).unwrap(),0.0);
        problem.fixed_penalties=vec![f32::MAX,f32::MAX];
        assert!(score(&problem).unwrap_err().contains("overflow"));
    }
}

pub(crate) fn net_length(net: &PostPlaceNet) -> f32 {
    minimum_spanning_segments(0, net).iter().map(|s| s.length).sum()
}

fn minimum_spanning_segments(_net_index: usize, net: &PostPlaceNet) -> Vec<Segment> {
    if net.points.len() < 2 {
        return Vec::new();
    }
    let mut connected = vec![false; net.points.len()];
    connected[0] = true;
    let mut connected_count = 1;
    let mut result = Vec::with_capacity(net.points.len() - 1);
    while connected_count < net.points.len() {
        let mut best: Option<(usize, usize, f32)> = None;
        for from in 0..net.points.len() {
            if !connected[from] {
                continue;
            }
            for to in 0..net.points.len() {
                if connected[to] {
                    continue;
                }
                let length = distance(net.points[from], net.points[to]);
                if best.is_none_or(|(best_from, best_to, best_length)| {
                    length < best_length - EPSILON
                        || ((length - best_length).abs() <= EPSILON
                            && (from, to) < (best_from, best_to))
                }) {
                    best = Some((from, to, length));
                }
            }
        }
        let Some((from, to, length)) = best else {
            break;
        };
        connected[to] = true;
        connected_count += 1;
        result.push(Segment {
            from,
            to,
            length,
            weight: net.weight,
        });
    }
    result
}

fn distance(a: Point, b: Point) -> f32 {
    crate::numerics::hypot(a.x - b.x,a.y - b.y)
}

fn box_clearance_gap(a: &crate::geometry::Box2, b: &crate::geometry::Box2) -> f32 {
    crate::conditions::clearance_gap(a.left,a.right,a.top,a.bottom,b.left,b.right,b.top,b.bottom)
}

fn validate(problem: &PostPlaceScoreProblem) -> Result<(), String> {
    if problem.access_components.iter().any(|c|!crate::geometry::is_finite_box(&c.body)||!crate::pad_access::valid(&c.rows)){return Err("invalid access rows".into());}
    if problem.version != 1 {
        return Err(format!(
            "unsupported post-place score contract {}; expected 1",
            problem.version
        ));
    }
    if problem.explicit_connections.iter().any(|e| !e.source.x.is_finite() || !e.source.y.is_finite() || !e.target.x.is_finite() || !e.target.y.is_finite() || e.net.is_empty() || crate::relation_score::connection_class(&e.kind)==0) {
        return Err("invalid explicit geometric connection".into());
    }
    let all_finite = problem.route_clearance.is_finite() && problem.route_clearance >= 0.0 && problem.nets.iter().all(|net| {
        (net.layers.is_empty() || net.layers.len() == net.points.len()) &&
        (net.internal_owners.is_empty() || net.internal_owners.len() == net.points.len()) &&
        (net.local_scopes.is_empty() || net.local_scopes.len() == net.points.len()) &&
        net.weight.is_finite()
            && net
                .points
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
    }) && problem
        .fixed_penalties
        .iter()
        .all(|value| value.is_finite());
    if !all_finite {
        return Err("post-place score contains a non-finite value".into());
    }
    Ok(())
}

/// Independent CPU graph evidence for the optional GPU validation path.
#[cfg(feature="gpu")]
pub(crate) fn graph_reference(problem:&PostPlaceScoreProblem)->Vec<(usize,usize,usize,f32)> {
    let mut base=0;let mut result=Vec::new();
    for (i,net) in problem.nets.iter().enumerate(){for edge in minimum_spanning_segments(i,net){result.push((base+edge.from,base+edge.to,i,edge.length));}base+=net.points.len();}result
}

/// Validate each GPU score family through the original scalar scorer, rather
/// than maintaining a second host implementation of its formulas.
#[cfg(feature="gpu")]
pub(crate) fn term_reference(p:&PostPlaceScoreProblem)->Result<Vec<f32>,String>{
 let mut empty=PostPlaceScoreProblem{route_clearance:0.0,access_components:vec![],explicit_connections:vec![],version:1,routing_obstacles:vec![],nets:vec![],distances:vec![],clearances:vec![],fixed_penalties:vec![],edges:vec![],paths:vec![]};let mut result=Vec::new();
 for t in &p.distances {empty.distances=vec![*t];result.push(score(&empty)?);}empty.distances.clear();
 for t in &p.clearances {empty.clearances=vec![*t];result.push(score(&empty)?);}empty.clearances.clear();
 empty.fixed_penalties=p.fixed_penalties.clone();result.push(score(&empty)?);empty.fixed_penalties.clear();
 for t in &p.edges {empty.edges=vec![t.clone()];result.push(score(&empty)?);}empty.edges.clear();
 for net in &p.nets {for (source,targets) in local_stretch_targets(net) {result.push(local_stretch_cost(net,source,&targets));}}
 for t in &p.paths {empty.paths=vec![t.clone()];result.push(score(&empty)?);}Ok(result)
}

/// One nearest target pad per connected IC within the original assembly scope.
/// Shared target selection for the scalar evaluator and GPU term compiler.
pub(crate) fn local_stretch_targets(net: &PostPlaceNet) -> Vec<(usize, Vec<usize>)> {
    let mut links = Vec::new();
    if crate::net_class::is_ground(&net.name) || net.local_scopes.len() != net.points.len()
        || net.internal_owners.len() != net.points.len() { return links; }
    for (source, scope) in net.local_scopes.iter().enumerate() {
        let Some(scope) = scope else { continue };
        if net.internal_owners[source].is_some() { continue; }
        let mut owners = Vec::new();
        for target in 0..net.points.len() {
            if net.local_scopes[target].as_ref() != Some(scope) { continue; }
            let Some(owner) = &net.internal_owners[target] else { continue };
            if owners.contains(&owner) { continue; }
            owners.push(owner);
            let targets = (0..net.points.len()).filter(|&i|
                net.local_scopes[i].as_ref() == Some(scope)
                && net.internal_owners[i].as_ref() == Some(owner)).collect();
            links.push((source, targets));
        }
    }
    links
}

fn local_stretch_cost(net: &PostPlaceNet, source: usize, targets: &[usize]) -> f32 {
    let nearest = targets.iter().map(|&i| distance(net.points[source], net.points[i]))
        .fold(f32::INFINITY, f32::min);
    crate::local_stretch::penalty(nearest, net.weight)
}

#[cfg(test)]
mod local_stretch_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn nearest_pad_per_local_ic_excludes_foreign_family_and_ground() {
        let mut net = PostPlaceNet {
            name: Arc::from("+3V3"), weight: 0.25,
            points: vec![Point{x:0.0,y:0.0}, Point{x:5.0,y:0.0},
                Point{x:2.0,y:0.0}, Point{x:0.5,y:0.0}],
            layers: vec![],
            internal_owners: vec![None,Some(Arc::from("U5")),Some(Arc::from("U5")),Some(Arc::from("U3"))],
            local_scopes: vec![Some(Arc::from("rs485")),Some(Arc::from("rs485")),
                Some(Arc::from("rs485")),Some(Arc::from("mcu"))],
        };
        assert_eq!(local_stretch_targets(&net), vec![(0,vec![1,2])]);
        assert_eq!(local_stretch_cost(&net,0,&[1,2]),3.0);
        net.points[2].x = 6.0;
        assert_eq!(local_stretch_cost(&net,0,&[1,2]),39.5);
        net.name = Arc::from("GND");
        assert!(local_stretch_targets(&net).is_empty());
        net.name = Arc::from("signal");
        net.local_scopes.clear();
        assert!(local_stretch_targets(&net).is_empty());
    }
}

#[cfg(test)]
mod explicit_crossing_tests {
 use super::*;
 fn problem()->PostPlaceScoreProblem {
  serde_json::from_value(serde_json::json!({"version":1,"nets":[
   {"name":"A","weight":1,"points":[{"x":0,"y":0},{"x":2,"y":0},{"x":1,"y":0.8}],"layers":["top","top","top"]},
   {"name":"B","weight":1,"points":[{"x":1,"y":-0.1},{"x":1,"y":0.1}],"layers":["top","top"]}],
   "distances":[],"clearances":[],"fixedPenalties":[],"edges":[],"paths":[]})).unwrap()
 }
 fn explicit(net:&str,a:Point,b:Point,kind:&str)->crate::model::ExplicitConnection {
  crate::model::ExplicitConnection{source:a,target:b,net:net.into(),kind:kind.into(),layer:Some("top".into())}
 }
 #[test] fn mandatory_non_mst_segment_and_duplicate_priority() {
  let mut p=problem();let base=score(&p).unwrap();
  let a=explicit("A",Point{x:0.,y:0.},Point{x:2.,y:0.},"critical_pair");
  p.explicit_connections=vec![a.clone()];let critical=score(&p).unwrap();assert!(critical>base);
  let path=crate::model::ExplicitConnection{kind:"signal_path".into(),..a.clone()};
  p.explicit_connections.push(path.clone());let path_score=score(&p).unwrap();assert!(path_score>=critical);
  p.explicit_connections.push(path);assert_eq!(score(&p).unwrap(),path_score);
  p.explicit_connections.push(explicit("B",Point{x:1.,y:-0.1},Point{x:1.,y:0.1},"signal_path"));
  assert!(score(&p).unwrap()>=path_score);
 }
 #[test] fn opposite_layers_and_explicit_pad_cost() {
  let mut p=problem();p.nets[1].layers=vec![Some("bottom".into());2];let base=score(&p).unwrap();
  p.explicit_connections.push(explicit("A",Point{x:0.,y:0.},Point{x:2.,y:0.},"signal_path"));
  assert_eq!(score(&p).unwrap(),base);
  p.routing_obstacles=vec![RouteObstacle{box_:crate::geometry::Box2{left:0.95,right:1.05,top:-0.05,bottom:0.05},layer:Some("top".into()),reference:Some("X.1".into()),net:Some("X".into()),primitive_id:None}];
  assert!(score(&p).unwrap()>base);
 }
}
