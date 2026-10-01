use crate::geometry::Point;
use crate::model::{PostPlaceNet, PostPlaceScoreProblem, RouteObstacle};
use crate::signal_path;

const EPSILON: f32 = 0.001;

#[derive(Clone, Copy)]
struct Segment {
    net: usize,
    from: usize,
    to: usize,
    a: Point,
    b: Point,
    length: f32,
    weight: f32,
}

pub fn score(problem: &PostPlaceScoreProblem) -> Result<f32, String> {
    validate(problem)?;
    let segments: Vec<_> = problem
        .nets
        .iter()
        .enumerate()
        .flat_map(|(index, net)| minimum_spanning_segments(index, net))
        .collect();
    let mut score = 0.0;
    for segment in &segments {
        score += segment.length * 10.0 * segment.weight;
        score += segment.length * segment.length * 0.35 * segment.weight;
    }
    score += crossing_penalty(&segments) * 180.0;
    if problem.pad_crossing_weight > 0.0 {
        score += pad_hits(&segments, &problem.nets, &problem.routing_obstacles) * problem.pad_crossing_weight;
    }

    for term in &problem.distances {
        let value = distance(term.source, term.target);
        score += value * term.weight;
        if let Some(max) = term.max {
            if value > max {
                score += (value - max).powi(2) * term.weight * 20.0;
            }
        }
        if let Some(min) = term.min {
            if value < min {
                score += (min - value).powi(2) * term.weight * 20.0;
            }
        }
    }
    for term in &problem.clearances {
        let gap = box_clearance_gap(&term.source, &term.target);
        if gap < term.minimum {
            score += (term.minimum - gap).powi(2) * term.weight * 20.0;
        }
    }
    score += problem.fixed_penalties.iter().sum::<f32>();
    for term in &problem.edges {
        let distance = match term.edge.as_ref() {
            "left" => (term.source.left - term.board.left).abs(),
            "right" => (term.board.right - term.source.right).abs(),
            "top" => (term.source.top - term.board.top).abs(),
            _ => (term.board.bottom - term.source.bottom).abs(),
        };
        score += distance * term.weight;
    }
    for path in &problem.paths {
        score += signal_path::topology_penalty_for_ports(
            &path.ports,
            path.shape.as_ref() == "straight",
            &path.priority,
            path.weight,
            path.prefer_facing_pads,
        ) * 20.0;
    }
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

fn minimum_spanning_segments(net_index: usize, net: &PostPlaceNet) -> Vec<Segment> {
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
            net: net_index,
            from,
            to,
            a: net.points[from],
            b: net.points[to],
            length,
            weight: net.weight,
        });
    }
    result
}

/// One charge per segment / foreign physical pad. In particular, do not exempt
/// the source/target component of an external connection. Only a segment with
/// both endpoints on one IC may ignore pads of that same immovable IC.
fn pad_hits(segments: &[Segment], nets: &[PostPlaceNet], obstacles: &[RouteObstacle]) -> f32 {
    let mut penalty = 0.0;
    for s in segments {
        let net = &nets[s.net];
        let a = net.layers.get(s.from).and_then(|l| l.as_deref());
        let b = net.layers.get(s.to).and_then(|l| l.as_deref());
        let layer = match (a, b) { (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) | (None, Some(a)) => Some(a), _ => None };
        let internal_owner = match (net.internal_owners.get(s.from).and_then(|v| v.as_deref()),
            net.internal_owners.get(s.to).and_then(|v| v.as_deref())) {
            (Some(a), Some(b)) if a == b => Some(a), _ => None,
        };
        let mut seen = rustc_hash::FxHashSet::default();
        for (i, pad) in obstacles.iter().enumerate() {
            if internal_owner.is_some_and(|owner| pad.reference.as_deref()
                .and_then(|r| r.rsplit_once('.')).is_some_and(|(d, _)| d == owner)) { continue; }
            if pad.net.as_ref() == Some(&net.name)
                || (layer.is_some() && pad.layer.is_some() && layer != pad.layer.as_deref()) { continue; }
            if crate::fast_route::hits(s.a, s.b, pad.box_) {
                // Named pads can occur more than once in imported obstacle lists.
                let key = pad.reference.as_deref().map(|r| (r, 0)).unwrap_or(("", i + 1));
                if seen.insert(key) { penalty += s.weight; }
            }
        }
    }
    penalty
}

pub(crate) fn pad_crossing_penalty(nets: &[PostPlaceNet], obstacles: &[RouteObstacle]) -> f32 {
    let segments: Vec<_> = nets.iter().enumerate().flat_map(|(i, n)| minimum_spanning_segments(i, n)).collect();
    pad_hits(&segments, nets, obstacles)
}

/// Exact per-net/per-primitive reuse. A moved obstacle invalidates its contribution
/// even when the net itself did not move. Call only when named-pad ownership is
/// disjoint between groups; duplicate imports otherwise need global deduplication.
#[derive(Default)]
pub(crate) struct PadCrossingCache {
    nets: rustc_hash::FxHashMap<std::sync::Arc<str>, CachedPadNet>,
    hits: u64,
    misses: u64,
}
struct CachedPadNet {
    points: Vec<u32>,
    layers: Vec<Option<std::sync::Arc<str>>>,
    owners: Vec<Option<std::sync::Arc<str>>>,
    weight: f32,
    segments: Vec<Segment>,
    groups: rustc_hash::FxHashMap<usize, (std::sync::Arc<Vec<RouteObstacle>>, f32)>,
}
impl PadCrossingCache {
    pub(crate) fn stats(&self) -> (u64, u64) { (self.hits, self.misses) }
    pub(crate) fn score(&mut self, nets: &[PostPlaceNet], groups: &[(usize, std::sync::Arc<Vec<RouteObstacle>>)]) -> f32 {
        let mut total = 0.0;
        for net in nets {
            if net.points.len() < 2 { continue; }
            let cached = self.nets.entry(net.name.clone()).or_insert_with(|| CachedPadNet {
                points: vec![], layers: vec![], owners: vec![], weight: f32::NAN,
                segments: vec![], groups: Default::default(),
            });
            if cached.points.len() != net.points.len()*2
                || !cached.points.iter().copied().eq(net.points.iter().flat_map(|p| [p.x.to_bits(), p.y.to_bits()]))
                || cached.layers != net.layers || cached.owners != net.internal_owners || cached.weight != net.weight {
                cached.points.clear(); cached.points.extend(net.points.iter().flat_map(|p| [p.x.to_bits(), p.y.to_bits()]));
                cached.layers.clone_from(&net.layers); cached.owners.clone_from(&net.internal_owners);
                cached.weight = net.weight; cached.segments = minimum_spanning_segments(0, net); cached.groups.clear();
            }
            for (id, obstacles) in groups {
                let value = if let Some((old, value)) = cached.groups.get(id).filter(|(old, _)| std::sync::Arc::ptr_eq(old, obstacles)) {
                    let _ = old; self.hits += 1; *value
                } else {
                    self.misses += 1;
                    let value = pad_hits(&cached.segments, std::slice::from_ref(net), obstacles);
                    cached.groups.insert(*id, (obstacles.clone(), value)); value
                };
                total += value;
            }
        }
        total
    }
}

#[cfg(test)]
mod incremental_pad_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn moving_foreign_pad_and_changing_net_invalidate_only_reusable_contributions() {
        let mut cache = PadCrossingCache::default();
        let mut net = PostPlaceNet { name: Arc::from("signal"), points: vec![Point{x:0.0,y:0.0},Point{x:4.0,y:0.0}],
            layers: vec![Some(Arc::from("top"));2], internal_owners: vec![None;2], weight: 1.0 };
        let obstacle = |y| Arc::new(vec![RouteObstacle {
            box_: crate::geometry::Box2{left:1.0,right:2.0,top:y,bottom:y+0.5}, layer:Some(Arc::from("top")),
            reference:Some(Arc::from("R2.1")),net:Some(Arc::from("other")),primitive_id:Some(Arc::from("R2")),
        }]);
        let mut groups = vec![(0,obstacle(2.0))];
        for step in 0..6 {
            if step==2 { groups[0].1=obstacle(-0.25); }
            if step==3 { net.points[1].y=8.0; }
            if step==4 { net.points[1].y=0.0; net.weight=0.25; }
            if step==5 { net.layers=vec![Some(Arc::from("bottom"));2]; }
            let expected=pad_crossing_penalty(std::slice::from_ref(&net),&groups[0].1);
            assert_eq!(cache.score(std::slice::from_ref(&net),&groups),expected);
            if step==2 { assert_eq!(expected,1.0); }
        }
        assert!(cache.stats().0>0);
    }
}

fn crossing_penalty(segments: &[Segment]) -> f32 {
    let mut penalty = 0.0;
    for a_index in 0..segments.len() {
        for b_index in (a_index + 1)..segments.len() {
            let a = segments[a_index];
            let b = segments[b_index];
            if a.net == b.net || share_endpoint(a, b) {
                continue;
            }
            if properly_intersect(a.a, a.b, b.a, b.b) {
                penalty += (a.weight * b.weight).sqrt();
            }
        }
    }
    penalty
}

fn properly_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = cross(a, b, c);
    let o2 = cross(a, b, d);
    let o3 = cross(c, d, a);
    let o4 = cross(c, d, b);
    ((o1 > EPSILON && o2 < -EPSILON) || (o1 < -EPSILON && o2 > EPSILON))
        && ((o3 > EPSILON && o4 < -EPSILON) || (o3 < -EPSILON && o4 > EPSILON))
}

fn share_endpoint(a: Segment, b: Segment) -> bool {
    [a.a, a.b].iter().any(|left| {
        [b.a, b.b]
            .iter()
            .any(|right| distance(*left, *right) < EPSILON)
    })
}

fn cross(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn distance(a: Point, b: Point) -> f32 {
    crate::numerics::hypot(a.x - b.x,a.y - b.y)
}

fn box_clearance_gap(a: &crate::geometry::Box2, b: &crate::geometry::Box2) -> f32 {
    let x_separation = (b.left - a.right).max(a.left - b.right);
    let y_separation = (b.top - a.bottom).max(a.top - b.bottom);
    x_separation.max(y_separation)
}

fn validate(problem: &PostPlaceScoreProblem) -> Result<(), String> {
    if problem.version != 1 {
        return Err(format!(
            "unsupported post-place score contract {}; expected 1",
            problem.version
        ));
    }
    let all_finite = problem.pad_crossing_weight.is_finite() && problem.pad_crossing_weight >= 0.0 && problem.nets.iter().all(|net| {
        (net.layers.is_empty() || net.layers.len() == net.points.len()) &&
        (net.internal_owners.is_empty() || net.internal_owners.len() == net.points.len()) &&
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
 let mut empty=PostPlaceScoreProblem{version:1,pad_crossing_weight:0.0,routing_obstacles:vec![],nets:vec![],distances:vec![],clearances:vec![],fixed_penalties:vec![],edges:vec![],paths:vec![]};let mut result=Vec::new();
 for t in &p.distances {empty.distances=vec![*t];result.push(score(&empty)?);}empty.distances.clear();
 for t in &p.clearances {empty.clearances=vec![*t];result.push(score(&empty)?);}empty.clearances.clear();
 empty.fixed_penalties=p.fixed_penalties.clone();result.push(score(&empty)?);empty.fixed_penalties.clear();
 for t in &p.edges {empty.edges=vec![t.clone()];result.push(score(&empty)?);}empty.edges.clear();
 for t in &p.paths {empty.paths=vec![t.clone()];result.push(score(&empty)?);}Ok(result)
}
