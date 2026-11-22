//! Frozen diagnostic harness: G0/G1 preparation, G1/G2 count plans and coupon.
use pcb_analyze::{geometry::{self,GeometryOptions},extraction::local::{near_far::{Budget,Plan,Operator,ExactCache},prism::Prism}};
use serde_json::{json,Value};
use std::{time::{Instant,Duration},fs};
fn budget()->Budget{Budget{max_bytes:512*1024*1024,max_nodes:8192,max_blocks:100000,max_pairs:6000000,max_wall:Duration::from_secs(120)}}
fn prism(c:&Value,mm:bool)->Prism {
    let mut t:[[f64;2];3]=serde_json::from_value(c[if mm{"triangleMm"}else{"triangleM"}].clone()).unwrap();
    let mut z:[f64;2]=serde_json::from_value(c[if mm{"zRangeMm"}else{"zM"}].clone()).unwrap();
    if mm {for p in &mut t{for x in p{*x*=1e-3;}}for x in &mut z{*x*=1e-3;}}
    if (t[1][0]-t[0][0])*(t[2][1]-t[0][1])-(t[1][1]-t[0][1])*(t[2][0]-t[0][0])<0.{t.swap(1,2);}
    Prism::new("frozen".into(),t,z).unwrap()
}
fn stats(p:&Plan)->Value{json!({"nearPairs":p.near_pairs,"farPairs":p.far_pairs,"promotedPairs":p.promoted_pairs,"plannedBytes":p.planned_bytes,"nearCacheUpperBytes":p.near_cache_upper_bytes().unwrap(),"constructionSeconds":p.construction_seconds,"moments":p.moments.len(),"blocks":p.blocks.len()})}
fn main(){
    let a:Vec<_>=std::env::args().collect();let mode=&a[1];let v:Value=serde_json::from_str(&fs::read_to_string(&a[3]).unwrap()).unwrap();
    if mode=="geometry" {
        let board=pcb_analyze::import_json(&fs::read_to_string(&a[3]).unwrap()).board.unwrap();
        let mut times=vec![];let mut saved=None;
        for _ in 0..2 {let start=Instant::now();let r=if a[2]=="G0"{geometry::prepare_legacy_for_diagnostic(&board,GeometryOptions::default())}else{geometry::prepare(&board,GeometryOptions::default())};times.push(start.elapsed().as_secs_f64());saved=Some(r);}
        let r=saved.unwrap();let roi=if a.len()>4 {let b:[f64;4]=serde_json::from_str(&a[4]).unwrap();Some(geometry::clip_layers_for_diagnostic(&board,&r,b).unwrap())}else{None};
        println!("{}",json!({"configuration":a[2],"coldWarmPreparationSeconds":times,"report":r,"roi":roi}));return;
    }
    if mode=="plans" {
        let mut out=vec![];
        for key in ["foil","axialSubprisms"] {let ps:Vec<_>=v[key].as_array().unwrap().iter().map(|p|prism(p,true)).collect();
            for coarse in [true,false] {let p=if coarse{Plan::new_coarse(&ps,0.4,8,budget())}else{Plan::new(&ps,0.4,8,budget())};
                out.push(match p{Ok(p)=>json!({"family":key,"configuration":if coarse{"G1"}else{"G2"},"plan":stats(&p)}),Err(e)=>json!({"family":key,"refusal":e})});}
        }println!("{}",json!({"plans":out}));return;
    }
    assert_eq!(mode,"coupon");let cells=v["cells"].as_array().unwrap();let ps:Vec<_>=cells.iter().map(|p|prism(p,false)).collect();
    let ds:Vec<Vec<[f64;3]>>=cells.iter().map(|p|serde_json::from_value(p["densities"].clone()).unwrap()).collect();let mut runs=vec![];
    for _ in 0..3 {
        let start=Instant::now();let mut cache=ExactCache::new(ps.clone(),24,budget()).unwrap();
        let op=if a[2]=="G1"{Operator::new_coarse(&mut cache,ds.clone(),0.4,8)}else{Operator::new(&mut cache,ds.clone(),0.4,8)}.unwrap();
        let matrix=op.matrix().unwrap();let mut actions=vec![];
        for q in [vec![1.,-0.5,0.2,0.8],vec![1.;4],vec![1.,-1.,1.,-1.],vec![0.;4],vec![1e-12,-1e-12,0.,0.]] {
            let x=op.apply(&q).unwrap();actions.push(json!({"q":q,"response":x.response,"energyJ":x.energy_j,"boundJ":x.absolute_truncation_energy_bound_j,"integratedPotential":x.integrated_potential,"potentialBounds":x.absolute_far_potential_bounds}));
        }
        runs.push(json!({"wallSeconds":start.elapsed().as_secs_f64(),"plan":stats(&op.plan),"matrixH":matrix,"boundMatrixH":op.bound_matrix_h,"rowsM5":op.truncation_rows_m5,"actions":actions,"kernelEvaluations":cache.evaluations,"kernelSeconds":cache.evaluation_seconds,"aggregateBoundSeconds":op.far_pair_preparation_seconds,"plannedSimultaneousBytes":op.planned_bytes}));
    }println!("{}",json!({"configuration":a[2],"runs":runs}));
}
