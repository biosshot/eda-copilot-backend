//! Independent frozen complete scalar-current subset; no reference resampling.
#[path="reference/local/mod.rs"] mod local_reference;
use pcb_analyze::extraction::local::{mesh::{self,LayeredInput,LayeredMesh,Material},volume_current,volume_magnetic,AllocationBudget};
use spade::{ConstrainedDelaunayTriangulation,Point2,Triangulation};
use std::collections::BTreeMap;
fn profile()->AllocationBudget {AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:200000}}
fn control_mesh()->LayeredMesh {
 let mut cdt=ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
 let points=[[0.,0.],[0.001,0.],[0.,0.0005]];
 let handles=points.map(|p|cdt.insert(Point2::new(p[0],p[1])).unwrap());
 for i in 0..3 {cdt.add_constraint(handles[i],handles[(i+1)%3]);}
 let mut distances=vec![];let mut d=0.000125;
 loop {distances.push(d);let corners=[[-d,-d],[0.001+d,-d],[0.001+d,0.0005+d],[-d,0.0005+d]];
  for p in corners {cdt.insert(Point2::new(p[0],p[1])).unwrap();}
  if d==0.008 {break;}d=(2.*d).min(0.008);
 }
 let xy_m=cdt.vertices().map(|v|[v.position().x,v.position().y]).collect();
 let triangles:Vec<_>=cdt.inner_faces().map(|f|f.vertices().map(|v|v.fix().index())).collect();
 let copper:Vec<_>=triangles.iter().map(|t|{let mut s=*t;s.sort();s==[0,1,2]}).collect();assert_eq!(copper.iter().filter(|v|**v).count(),1);
 let mut edges=BTreeMap::new();for t in &triangles {for i in 0..3 {let mut e=[t[i],t[(i+1)%3]];e.sort();*edges.entry(e).or_insert(0)+=1;}}
 let boundary_edges=edges.into_iter().filter(|(_,n)|*n==1).map(|(e,_)|e).collect();
 let mut z_m:Vec<_>=distances.iter().rev().map(|d|-d).collect();z_m.extend([0.,0.0005]);z_m.extend(distances.iter().map(|d|0.0005+d));
 let materials=z_m.windows(2).map(|z|copper.iter().map(|&cu|if cu&&z[0]==0.&&z[1]==0.0005 {Material::Copper{source_ids:vec!["frozen-prism-0".into()]}}else{Material::Dielectric{label:"air".into(),epsilon_r:1.}}).collect()).collect();
 mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m,materials},profile()).unwrap()
}
fn full_space_error(actual:&[Vec<f64>],reference:&[Vec<f64>])->f64 {
 let n=reference.len();let mut l=vec![vec![0.;n];n];
 for i in 0..n {for j in 0..=i {let v=reference[i][j]-(0..j).map(|k|l[i][k]*l[j][k]).sum::<f64>();l[i][j]=if i==j{assert!(v>0.);v.sqrt()}else{v/l[j][j]};}}
 let mut inv=vec![vec![0.;n];n];for j in 0..n {for i in 0..n {inv[i][j]=((if i==j{1.}else{0.})-(0..i).map(|k|l[i][k]*inv[k][j]).sum::<f64>())/l[i][i];}}
 let mut norm=0.;for i in 0..n {for j in 0..n {let v=(0..n).flat_map(|k|(0..n).map(move|m|(k,m))).map(|(k,m)|inv[i][k]*(actual[k][m]-reference[k][m])*inv[j][m]).sum::<f64>();norm+=v*v;}}norm.sqrt()
}
#[test]
fn independent_complete_three_tetra_source_space_with_degree_refinement() {
 screen()
}
fn screen() {
 let frozen:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/tetra-space-reference.json")).unwrap();
 let mesh=control_mesh();assert_eq!(mesh.copper_tetrahedra.len(),3);
 let bits:Vec<[[u64;3];4]>=serde_json::from_value(frozen["tetraVertexBits"].clone()).unwrap();
 assert_eq!(mesh.copper_tetrahedra.iter().map(|t|t.nodes.map(|n|mesh.nodes_m[n].map(f64::to_bits))).collect::<Vec<_>>(),bits[..3]);
 let reference:Vec<Vec<f64>>=serde_json::from_value(frozen["matrix"].clone()).unwrap();let reference=reference[..3].iter().map(|r|r[..3].to_vec()).collect::<Vec<_>>();
 let uncertainty=frozen["empiricalWhitenedReferenceScreen"].as_f64().unwrap();
 let space=volume_current::assemble(&mesh,&vec![58e6;3],&[],profile()).unwrap();
 for (trace,degree) in [(5,7),(6,8)] {
  let operator=volume_magnetic::Operator::new_enriched_patch(&mesh,profile(),8,trace,degree,3).unwrap();
  let mut actual=vec![vec![0.;3];3];
  for j in 0..3 {let mut q=vec![0.;9];q[3*j+2]=1./space.cells[j].density_per_amp_m2;let image=operator.apply(&space,&q).unwrap();for i in 0..3 {actual[i][j]=image[3*i+2]/space.cells[i].density_per_amp_m2;}}
  let error=full_space_error(&actual,&reference);
  eprintln!("Entity airP3/Cu-trace{trace}/bubble{degree}; source3-space error={error:e}, inherited empirical reference screen={uncertainty:e}; plan={} bytes; matrix={actual:?}",operator.planned_bytes);
  if degree==7 {assert!(error+uncertainty>=0.01,"lower profile unexpectedly meets the frozen1% screen; review applicability evidence");}else{assert!(error+uncertainty<0.01,"complete three-cell energy screen failed");}
 }
 assert!(volume_magnetic::Operator::new_enriched_degree(&mesh,profile(),8,usize::MAX).err().unwrap().contains("MAGNETIC_DEGREE"));
}

#[test]
fn complete_source_cache_preserves_all_cartesian_cells_and_refuses_incomplete_or_wrong_geometry() {
 use local_reference::volume_matrix::{Operator,Signature};
 use pcb_analyze::extraction::local::volume_magnetic::MagneticApply;
 let mesh=control_mesh();let space=volume_current::assemble(&mesh,&vec![58e6;3],&[],profile()).unwrap();
 let path=std::env::temp_dir().join(format!("p7-full-cell-cache-{}.jsonl",std::process::id()));
 let matrix=[[2.,0.4,0.3],[0.4,3.,0.2],[0.3,0.2,4.]];
 let header=serde_json::to_string(&Signature::new(&mesh,&space,vec![10,10,16])).unwrap();
 let write=|count:usize|{let mut body=header.clone();for j in 0..count{body.push('\n');body.push_str(&serde_json::json!({"column":j,"values":(0..3).map(|i|matrix[i][j]).collect::<Vec<_>>(),"seconds":0.}).to_string());}std::fs::write(&path,body).unwrap();};
 write(2);assert!(Operator::load(&mesh,&space,vec![10,10,16],&[&path],profile()).err().unwrap().contains("incomplete"));write(3);
 assert!(Operator::load(&mesh,&space,vec![10,12,16],&[&path],profile()).err().unwrap().contains("identity"));
 let op=Operator::load(&mesh,&space,vec![10,10,16],&[&path],profile()).unwrap();let v=vec![0.3,0.,-0.2,0.1,0.7,0.4,-0.6,0.2,0.9];let action=op.apply(&space,&v).unwrap();for i in 0..3{for axis in 0..3{let expected=(0..3).map(|j|matrix[i][j]*v[3*j+axis]).sum::<f64>();assert!((action[3*i+axis]-expected).abs()<1e-14);}}std::fs::remove_file(path).unwrap();
}

#[test]
fn immutable_action_reuse_preserves_every_bit_and_scope_budget_guards(){use pcb_analyze::extraction::local::{magnetic_actions::Reuse,volume_magnetic::MagneticApply};let mesh=control_mesh();let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&[],profile()).unwrap();let operator=volume_magnetic::Operator::new(&mesh,profile()).unwrap();let memo=Reuse::new(&operator,&space,4096).unwrap();let v=(0..space.current_count()).map(|i|(i as f64*0.37).sin()).collect::<Vec<_>>();let direct=operator.apply(&space,&v).unwrap();let first=memo.apply(&space,&v).unwrap();let warm=memo.apply(&space,&v).unwrap();assert_eq!(direct.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),first.iter().map(|v|v.to_bits()).collect::<Vec<_>>());assert_eq!(first,warm);assert_eq!(memo.stats()["hits"],1);assert!(memo.stats()["retainedBytes"].as_u64().unwrap()<=4096);let other=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&[],profile()).unwrap();assert!(memo.apply(&other,&v).is_err());assert!(memo.apply(&space,&vec![f64::NAN;v.len()]).is_err());let tiny=Reuse::new(&operator,&space,1).unwrap();tiny.apply(&space,&v).unwrap();tiny.apply(&space,&v).unwrap();assert_eq!(tiny.stats()["hits"],0);assert_eq!(tiny.stats()["retainedBytes"],0);}

#[test]
fn action_reuse_evicts_and_does_not_cache_failed_operator_actions(){use pcb_analyze::extraction::local::{magnetic_actions::Reuse,volume_magnetic::MagneticApply};let mesh=control_mesh();let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&[],profile()).unwrap();let operator=volume_magnetic::Operator::new(&mesh,profile()).unwrap();let memo=Reuse::new(&operator,&space,512).unwrap();let a=vec![1.;space.current_count()];let b=vec![2.;space.current_count()];memo.apply(&space,&a).unwrap();memo.apply(&space,&b).unwrap();memo.apply(&space,&a).unwrap();assert_eq!(memo.stats()["misses"],3);assert!(memo.stats()["retainedBytes"].as_u64().unwrap()<=512);let failed=Reuse::new(&operator,&space,4096).unwrap();for _ in 0..2{assert!(failed.apply(&space,&vec![1e308;space.current_count()]).is_err());}assert_eq!(failed.stats()["misses"],2);assert_eq!(failed.stats()["retainedBytes"],0);}
