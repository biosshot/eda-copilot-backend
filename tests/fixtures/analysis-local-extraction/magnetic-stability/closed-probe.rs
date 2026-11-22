//! Full finite solenoidal two-streamfunction magnetic coupon; offline only.
#![allow(dead_code)]
#[path="__PRISM__"] mod prism;
#[path="__POLYHEDRON__"] mod polyhedron;
type Result<T>=std::result::Result<T,String>;
fn error(code:&str,text:&str)->String {format!("LOCAL_{code}: {text}")}
use serde_json::{Value,json};use std::time::Instant;
fn main(){
 let args:Vec<_>=std::env::args().collect();let v:Value=serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();let started=Instant::now();
 let cells=v["cells"].as_array().unwrap();let prisms:Vec<_>=cells.iter().map(|p|prism::Prism::new(p["sourceId"].as_str().unwrap().into(),serde_json::from_value(p["triangleM"].clone()).unwrap(),serde_json::from_value(p["zM"].clone()).unwrap()).unwrap()).collect();let sources:Vec<_>=prisms.iter().map(|p|polyhedron::PointPotential::new(p).unwrap()).collect();let densities:Vec<[[f64;2];2]>=cells.iter().map(|c|serde_json::from_value(c["densityPerAmpM2"].clone()).unwrap()).collect();let mut levels=vec![];
 for order in [8,12]{let q=prism::GaussRule::new(order).unwrap();let mut kernel=vec![vec![0.;cells.len()];cells.len()];let mut l=[[0.;2];2];let mut costs=vec![];
  for i in 0..cells.len(){for j in 0..cells.len(){let time=Instant::now();let result=polyhedron::volume_integral(&prisms[i],&sources[j],&q);let seconds=time.elapsed().as_secs_f64();let value=match &result {Ok(v)=>*v,Err(_)=>0.};kernel[i][j]=value;
   for a in 0..2{for b in 0..2{let dot=densities[i][a][0]*densities[j][b][0]+densities[i][a][1]*densities[j][b][1];l[a][b]+=1e-7*value*dot;}}
   costs.push(json!({"i":i,"j":j,"seconds":seconds,"result":result}));
  }}levels.push(json!({"order":order,"rawKernelM5":kernel,"rawLH":l,"allOrderedPairCosts":costs}));
 }println!("{}",json!({"levels":levels,"seconds":started.elapsed().as_secs_f64(),"scope":"Entire2D solenoidal current space; no external terminal/return geometry"}));
}
