//! Canonical full-layout route objective. Stage adapters supply geometry; all
//! stages share the tree, seven alternatives, layer filtering and arithmetic.
use super::*;
use crate::model::{ExplicitConnection, PostPlaceNet};

pub(crate) fn layout_plan(primitives: &[&Primitive], obstacles: &[RouteObstacle], external: &[Arc<str>], ignored: &[Arc<str>], clearance: f32, bounds: Box2, relations: &[crate::model::Relation]) -> Plan {
    let mut nets = std::collections::BTreeMap::<Arc<str>, PostPlaceNet>::new();
    for p in primitives {
        for cp in p.connection_points.iter() {
            let Some(name) = &cp.net else { continue };
            let net = nets.entry(name.clone()).or_insert_with(|| PostPlaceNet {
                name: name.clone(), points: vec![], layers: vec![], internal_owners: vec![], local_scopes: vec![], weight: 1.0,
            });
            net.points.push(Point { x: cp.x, y: cp.y });
            net.layers.push(pad_layer(&cp.reference, obstacles).map(Arc::from));
        }
    }
    nets_plan(&nets.into_values().collect::<Vec<_>>(), obstacles, &explicit_connections(primitives, relations, obstacles), external, ignored, clearance, bounds)
}

pub(crate) fn nets_plan(nets: &[PostPlaceNet], obstacles: &[RouteObstacle], explicit: &[ExplicitConnection], external: &[Arc<str>], ignored: &[Arc<str>], clearance: f32, bounds: Box2) -> Plan {
    // Stable accumulation order even when primitive insertion order changes.
    let mut obstacles = obstacles.to_vec();
    obstacles.sort_by(obstacle_order);
    obstacles.dedup_by(|a,b| same_obstacle(a,b));
    let obstacles = obstacles.as_slice();
    let nets = route_nets(nets, ignored);
    let explicit = sorted_explicit(explicit);
    let links = route_links(&nets, nets.iter().map(|net| spanning_segments(net.points.clone())), &explicit);
    let layers: Vec<_> = links.iter().map(|(n,(a,b))| link_layer(&nets, &explicit, n, *a, *b)).collect();
    let mut plan = Plan::default();
    let mut costs = Vec::new();
    for ((net,(a,b)),layer) in links.iter().zip(&layers) {
        let compatible: Vec<_> = links.iter().zip(&layers).filter(|(_,other)| layer.is_none() || other.is_none() || *other==layer).map(|(link,_)| link.clone()).collect();
        costs.push(plan.path(*a,*b,net,obstacles,&compatible,clearance,layer.as_deref(),&explicit));
    }
    for net in nets.iter().filter(|n| external.contains(&n.name)) {
        let mut exits = Vec::new();
        for (i,a) in net.points.iter().enumerate() {
            for b in [Point{x:bounds.left,y:a.y},Point{x:bounds.right,y:a.y},Point{x:a.x,y:bounds.top},Point{x:a.x,y:bounds.bottom}] {
                exits.push(plan.path(*a,b,&net.name,obstacles,&links,clearance,net.layers.get(i).and_then(|l|l.as_deref()),&[])+distance(*a,b)*0.25);
            }
        }
        if let Some(exit) = exits.into_iter().reduce(Cost::min) { costs.push(exit*0.5); }
    }
    plan.root(costs.into_iter().sum());
    plan
}

fn obstacle_order(a: &RouteObstacle, b: &RouteObstacle) -> std::cmp::Ordering {
    a.net.cmp(&b.net).then(a.layer.cmp(&b.layer)).then(a.reference.cmp(&b.reference))
        .then(a.box_.left.total_cmp(&b.box_.left)).then(a.box_.right.total_cmp(&b.box_.right))
        .then(a.box_.top.total_cmp(&b.box_.top)).then(a.box_.bottom.total_cmp(&b.box_.bottom))
}
fn same_obstacle(a: &RouteObstacle, b: &RouteObstacle) -> bool {
    a.reference.is_some() && a.reference==b.reference && a.net==b.net && a.layer==b.layer
        && a.box_.left==b.box_.left && a.box_.right==b.box_.right && a.box_.top==b.box_.top && a.box_.bottom==b.box_.bottom
}
/// Input indices of the obstacles in the same order and deduplication as `nets_plan`.
pub(super) fn sorted_obstacles(obstacles: &[RouteObstacle]) -> Vec<usize> {
    let mut ids: Vec<usize> = (0..obstacles.len()).collect();
    ids.sort_by(|&a,&b| obstacle_order(&obstacles[a],&obstacles[b]));
    ids.dedup_by(|a,b| same_obstacle(&obstacles[*a],&obstacles[*b]));
    ids
}
/// Routed nets, in name order.
pub(super) fn route_nets<'a>(nets: &'a [PostPlaceNet], ignored: &[Arc<str>]) -> Vec<&'a PostPlaceNet> {
    let mut nets: Vec<_> = nets.iter().filter(|n| !n.name.is_empty() && !is_ground(&n.name) && !ignored.iter().any(|s| s.eq_ignore_ascii_case(&n.name))).collect();
    nets.sort_by(|a,b| a.name.cmp(&b.name));
    nets
}
pub(super) fn sorted_explicit(explicit: &[ExplicitConnection]) -> Vec<ExplicitConnection> {
    let mut explicit = explicit.to_vec();
    explicit.sort_by(|a,b| a.net.cmp(&b.net).then(a.source.x.total_cmp(&b.source.x)).then(a.source.y.total_cmp(&b.source.y)).then(a.target.x.total_cmp(&b.target.x)).then(a.target.y.total_cmp(&b.target.y)));
    explicit
}
/// Spanning links of every routed net, then authored connections not already in them.
pub(super) fn route_links(nets: &[&PostPlaceNet], segments: impl Iterator<Item = Vec<Segment>>, explicit: &[ExplicitConnection]) -> Vec<(Arc<str>, Segment)> {
    let mut links = Vec::new();
    for (net, segments) in nets.iter().zip(segments) { links.extend(segments.into_iter().map(|s| (net.name.clone(), s))); }
    extend_explicit_skeleton(&mut links, explicit);
    links
}
/// Copper side of one link: its authored connection's, else a side shared by both endpoints.
pub(super) fn link_layer(nets: &[&PostPlaceNet], explicit: &[ExplicitConnection], net: &Arc<str>, a: Point, b: Point) -> Option<Arc<str>> {
    if let Some(e) = explicit.iter().find(|e| &e.net==net && ((distance(a,e.source)<0.001 && distance(b,e.target)<0.001)||(distance(b,e.source)<0.001 && distance(a,e.target)<0.001))) { return e.layer.clone(); }
    let endpoint = |p: Point| {
        let mut layers = nets.iter().filter(|n| &n.name==net).flat_map(|n| n.points.iter().enumerate().filter(move |(_,q)|distance(p,**q)<0.001).map(move |(i,_)| n.layers.get(i).cloned().flatten()));
        let first=layers.next().flatten();
        if layers.all(|l| l==first) { first } else { None }
    };
    match (endpoint(a),endpoint(b)) { (Some(a),Some(b)) if a==b => Some(a), (Some(a),None)|(None,Some(a))=>Some(a), _=>None }
}

pub(super) fn spanning_segments(mut pts: Vec<Point>) -> Vec<Segment> {
    pts.sort_by(|a,b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a,b| distance(*a,*b)<1e-6);
    if pts.is_empty() { return vec![]; }
    let mut used=vec![false;pts.len()]; used[0]=true;
    let mut result=Vec::new();
    for _ in 1..pts.len() {
        let mut best=(f32::INFINITY,0,0);
        for a in 0..pts.len() { if used[a] { for b in 0..pts.len() { if !used[b] {
            let d=distance(pts[a],pts[b]); if d<best.0 { best=(d,a,b); }
        } } } }
        used[best.2]=true; result.push((pts[best.1],pts[best.2]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn nets() -> Vec<PostPlaceNet> {
        vec![PostPlaceNet { name:"SIG".into(),points:vec![Point{x:0.,y:0.},Point{x:9.,y:0.},Point{x:5.,y:3.}],layers:vec![Some("top".into());3],internal_owners:vec![],local_scopes:vec![],weight:1. }]
    }
    fn obstacles() -> Vec<RouteObstacle> {
        (0..12).map(|i|RouteObstacle {box_:Box2{left:i as f32*0.7,right:i as f32*0.7+0.3,top:-2.,bottom:2.},net:Some("OTHER".into()),layer:None,reference:None,primitive_id:None}).collect()
    }
    fn plan(nets:&[PostPlaceNet],obstacles:&[RouteObstacle])->Plan {
        nets_plan(nets,obstacles,&[],&[],&[],0.25,Box2{left:0.,right:10.,top:-5.,bottom:5.})
    }
    #[test]
    fn geometry_order_and_batching_do_not_change_scores() {
        let mut n=nets();let mut o=obstacles();let before=plan(&n,&o).cpu();assert!(before>0.);
        n[0].points.reverse();n[0].layers.reverse();o.reverse();
        assert_eq!(before.to_bits(),plan(&n,&o).cpu().to_bits());
        let mut batch=Plan::default();batch.append(plan(&n,&o));batch.append(plan(&n,&[]));
        assert_eq!(batch.reduce_many(&batch.cpu_paths()),vec![before,0.]);
    }
    #[test]
    fn moving_foreign_obstacles_can_reduce_the_full_objective() {
        let n=nets();let before=plan(&n,&obstacles()).cpu();
        let mut o=obstacles();for p in &mut o {p.box_.top+=20.;p.box_.bottom+=20.;}
        let after=plan(&n,&o).cpu();assert!(after<before);
        // A parent's full penalty is consequently not a candidate lower bound.
        assert_eq!(after,0.);
    }
    #[test]
    fn post_place_uses_identical_route_geometry() {
        let problem:crate::model::PostPlaceScoreProblem=serde_json::from_value(serde_json::json!({"version":1,"routeClearance":0.25,"nets":nets(),"routingObstacles":obstacles(),"distances":[],"clearances":[],"fixedPenalties":[],"edges":[],"paths":[]})).unwrap();
        assert_eq!(crate::post_place::route_plan(&problem).cpu().to_bits(),plan(&nets(),&obstacles()).cpu().to_bits());
    }
}
