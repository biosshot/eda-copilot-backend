#![allow(dead_code)]
#[path="__MODEL__"] mod model;
mod geometry {
 #[derive(Clone,Copy)] pub struct GeometryOptions {pub max_deviation_mm:f64,pub max_vertices_per_curve:usize,pub max_generated_vertices:usize}
 pub struct PolygonSnapshot {pub outer:Vec<[f64;2]>,pub holes:Vec<Vec<[f64;2]>>}
 #[path="__AREAS__"] pub mod areas;
 #[path="__PRIMITIVES__"] pub mod primitives;
}
use geo::{ConvexHull,MultiPoint,Point};
use serde_json::{Value,json};
fn snap(p:&geo::Polygon<f64>)->Value {let q=geometry::areas::snapshot(p);json!({"outer":q.outer,"holes":q.holes})}
fn main(){
 let args:Vec<_>=std::env::args().collect();let v:Value=serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
 let options=geometry::GeometryOptions{max_deviation_mm:0.001,max_vertices_per_curve:16384,max_generated_vertices:2000000};
 for t in v["tracks"].as_array().unwrap(){
  let pts:Vec<_>=t["points"].as_array().unwrap().iter().map(|p|model::Point{x:p["x"].as_f64().unwrap(),y:p["y"].as_f64().unwrap()}).collect();
  let w=t["widthMm"].as_f64().unwrap(); let mut b=geometry::primitives::Builder{options,generated:0};
  let circles:Vec<_>=pts.iter().map(|p|b.circle(*p,w/2.).unwrap()).collect();
  let raw:Vec<_>=circles.iter().map(snap).collect();
  let hull=MultiPoint::new(circles.iter().flat_map(|p|p.exterior().0.iter().map(|c|Point(*c))).collect()).convex_hull();
  let actual=b.stroke(&pts,w).unwrap();
  println!("{}",json!({"id":t["id"],"rawCircles":raw,"actual":actual.0.iter().map(snap).collect::<Vec<_>>(),"experimentalHull":snap(&hull),"scope":"two-point-only inscribed circle convex hull, experimental, not production"}));
 }
}
