//! Bounded geometric routing proxy, not a connectivity proof. No A* and no vias
//! invented from line intersections. Length already exists in the block objective;
//! this module charges only detour, foreign pads, clearance bands and crossings.
use crate::geometry::{Box2, Point};
use crate::model::{Primitive, RouteObstacle};
use crate::net_class::is_ground;
use std::sync::Arc;

mod arithmetic;
mod incremental;
mod layout;
pub(crate) use incremental::RouteCache;
pub(crate) use layout::{layout_plan, nets_plan};
pub(crate) mod plan;
#[cfg(feature="gpu")]
mod gpu;
use plan::{Cost, Plan};

type Segment = (Point, Point);

fn pad_layer<'a>(reference: &str, obstacles: &'a [RouteObstacle]) -> Option<&'a str> {
    obstacles
        .iter()
        .find(|p| p.reference.as_deref() == Some(reference))
        .and_then(|p| p.layer.as_deref())
}

fn common_layer(a: &str, b: &str, obstacles: &[RouteObstacle]) -> Option<Arc<str>> {
    match (pad_layer(a, obstacles), pad_layer(b, obstacles)) {
        (Some(a), Some(b)) if a == b => Some(Arc::from(a)),
        (Some(a), None) | (None, Some(a)) => Some(Arc::from(a)),
        _ => None, // Mixed-side or through-hole pair: conservative projection.
    }
}

fn explicit_class(net:&str,a:Point,b:Point,links:&[crate::model::ExplicitConnection])->u32 {
 links.iter().filter(|e|e.net.as_ref()==net && ((distance(a,e.source)<0.001&&distance(b,e.target)<0.001)||(distance(a,e.target)<0.001&&distance(b,e.source)<0.001)))
 .map(|e|crate::relation_score::connection_class(&e.kind)).max().unwrap_or(0)
}
fn extend_explicit_skeleton(skeleton:&mut Vec<(Arc<str>,Segment)>,explicit:&[crate::model::ExplicitConnection]) {
 for e in explicit {if distance(e.source,e.target)<0.001 {continue;}if !skeleton.iter().any(|(n,(a,b))|n==&e.net && ((distance(*a,e.source)<0.001&&distance(*b,e.target)<0.001)||(distance(*a,e.target)<0.001&&distance(*b,e.source)<0.001))) {skeleton.push((e.net.clone(),(e.source,e.target)));}}
}
#[cfg(test)]
fn path_cost(a:Point,b:Point,net:&str,obstacles:&[RouteObstacle],skeleton:&[(Arc<str>,Segment)],clearance:f32,layer:Option<&str>)->f32 {
 weighted_path_cost(a,b,net,obstacles,skeleton,clearance,layer,&[])
}

#[cfg(test)]
fn weighted_path_cost(a:Point,b:Point,net:&str,obstacles:&[RouteObstacle],skeleton:&[(Arc<str>,Segment)],clearance:f32,layer:Option<&str>,explicit:&[crate::model::ExplicitConnection])->f32 {
    let mut plan=Plan::default(); let root=plan.path(a,b,net,obstacles,skeleton,clearance,layer,explicit); plan.root(root); plan.cpu()
}

fn distance(a: Point, b: Point) -> f32 {
    crate::numerics::hypot(a.x - b.x,a.y - b.y)
}
pub(crate) fn hits(a: Point, b: Point, q: Box2) -> bool {
    arithmetic::hits(arithmetic::P{x:a.x,y:a.y},arithmetic::P{x:b.x,y:b.y},q.left,q.right,q.top,q.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segment_box_handles_vertical_and_inside() {
        let q = Box2 {
            left: 0.,
            right: 1.,
            top: 0.,
            bottom: 1.,
        };
        assert!(hits(Point { x: 0.5, y: -1. }, Point { x: 0.5, y: 2. }, q));
        assert!(!hits(Point { x: 2., y: -1. }, Point { x: 2., y: 2. }, q));
        assert!(hits(Point { x: 0.5, y: 0.5 }, Point { x: 0.6, y: 0.6 }, q));
    }
    #[test]
    fn foreign_pad_cost_but_same_net_is_free() {
        let a = Point { x: 0., y: 0. };
        let b = Point { x: 10., y: 0. };
        let obstacle = RouteObstacle {
            box_: Box2 {
                left: 4.,
                right: 6.,
                top: -2.,
                bottom: 2.,
            },
            layer: None,
            reference: None,
            net: Some(Arc::from("OTHER")),
            primitive_id: None,
        };
        assert!(path_cost(a, b, "SIG", &[obstacle.clone()], &[], 0.25, None) > 0.0);
        assert_eq!(
            path_cost(a, b, "OTHER", &[obstacle.clone()], &[], 0.25, None),
            0.0
        );
        let bottom = RouteObstacle {
            layer: Some(Arc::from("bottom")),
            ..obstacle
        };
        assert_eq!(
            path_cost(a, b, "SIG", &[bottom], &[], 0.25, Some("top")),
            0.0
        );
    }
    #[test]
    fn crossing_foreign_ratline_costs_more_than_parallel() {
        let a=Point{x:0.,y:0.}; let b=Point{x:10.,y:0.};
        let crossing=vec![(Arc::from("OTHER"),(Point{x:5.,y:-5.},Point{x:5.,y:5.}))];
        let parallel=vec![(Arc::from("OTHER"),(Point{x:0.,y:5.},Point{x:10.,y:5.}))];
        assert!(path_cost(a,b,"SIG",&[],&crossing,0.25,None)>path_cost(a,b,"SIG",&[],&parallel,0.25,None));
    }
    #[test]
    fn explicit_classes_share_crossing_table_without_duplicate_multiplier() {
        let a=Point{x:0.,y:0.};let b=Point{x:10.,y:0.};
        let c=Point{x:5.,y:-5.};let d=Point{x:5.,y:5.};
        let skeleton=vec![(Arc::from("OTHER"),(c,d))];
        let path=crate::model::ExplicitConnection{source:a,target:b,net:"SIG".into(),kind:"signal_path".into(),layer:None};
        assert_eq!(weighted_path_cost(a,b,"SIG",&[],&skeleton,0.25,None,&[path.clone(),path.clone()]),9.);
        let other=crate::model::ExplicitConnection{source:c,target:d,net:"OTHER".into(),kind:"signal_path".into(),layer:None};
        assert_eq!(weighted_path_cost(a,b,"SIG",&[],&skeleton,0.25,None,&[path,other]),90.);
    }
    #[test]
    fn unrelated_new_pad_can_obstruct_an_existing_link() {
        let p: Primitive=serde_json::from_value(serde_json::json!({
            "id":"P","kind":"block","label":"P","sourceNodeId":"P","sourceNodeIds":[],
            "locked":false,"canRotate":false,"allowedOrientations":[0],
            "bbox":{"left":0,"right":10,"top":0,"bottom":1},"collisionBoxes":[],"width":10,"height":1,
            "placements":[],"connectionPoints":[{"ref":"A.1","net":"SIG","x":0,"y":0},{"ref":"B.1","net":"SIG","x":10,"y":0}]
        })).unwrap();
        let pad=RouteObstacle{box_:Box2{left:4.,right:6.,top:-2.,bottom:2.},layer:None,
            reference:Some(Arc::from("C.1")),net:Some(Arc::from("OTHER")),primitive_id:Some(Arc::from("C"))};
        assert!(layout_plan(&[&p],&[pad.clone()],&[],&[],0.25,p.bbox,&[]).cpu()>0.);
        let same=RouteObstacle{net:Some(Arc::from("SIG")),..pad};
        assert_eq!(layout_plan(&[&p],&[same],&[],&[],0.25,p.bbox,&[]).cpu(),0.);
    }

}

/// Resolve authored pad pairs independently of the ordinary net spanning tree.
pub fn explicit_connections(primitives:&[&Primitive],relations:&[crate::model::Relation],obstacles:&[RouteObstacle])->Vec<crate::model::ExplicitConnection> {
    relations.iter().filter_map(|r| {
        let kind=if r.path_id.is_some(){"signal_path"}else if r.kind.as_ref()=="critical_pair" || r.relation.as_deref()==Some("critical_pair"){"critical_pair"}else{return None;};
        let from=r.from.strip_prefix("pad:")?;let to=r.to.strip_prefix("pad:")?;
        let find=|reference:&str|primitives.iter().flat_map(|p|p.connection_points.iter()).find(|p|p.reference.as_ref()==reference);
        let a=find(from)?;let b=find(to)?;let net=a.net.as_ref().filter(|n|!n.is_empty() && b.net.as_ref()==Some(n))?;
        Some(crate::model::ExplicitConnection{source:Point{x:a.x,y:a.y},target:Point{x:b.x,y:b.y},net:net.clone(),layer:common_layer(from,to,obstacles),kind:Arc::from(kind)})
    }).collect()
}
