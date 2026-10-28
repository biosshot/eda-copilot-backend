use pcb_analyze::extraction::local::{mesh::{self,LayeredInput,LayeredMesh,Material},volume_current,volume_field,AllocationBudget};
fn budget()->AllocationBudget {AllocationBudget{max_bytes:32*1024*1024,max_nodes:10000,max_tetrahedra:10000}}
#[test]
fn sparse_field_affine_energy_is_analytic_with_zero_or_one_free_node(){
 use pcb_analyze::extraction::{EPS0,local::capacitance::{Boundary,CapacitanceOperator,TraceRow}};
 use std::collections::BTreeSet;
 for z in[vec![0.,0.001],vec![0.,0.0005,0.001]]{
  let axis=[0.,0.0005,0.001];let xy_m=axis.iter().flat_map(|&y|axis.iter().map(move|&x|[x,y])).collect();let mut triangles=vec![];
  for j in 0..2{for i in 0..2{let a=j*3+i;triangles.extend([[a,a+1,a+4],[a,a+4,a+3]]);}}
  let mut boundary_edges=vec![];for i in 0..2{boundary_edges.extend([[i,i+1],[6+i,7+i],[3*i,3*(i+1)],[3*i+2,3*(i+1)+2]]);}
  let materials=vec![vec![Material::Dielectric{label:"affine uniform medium".into(),epsilon_r:4.2};triangles.len()];z.len()-1];let mesh=mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m:z,materials},budget()).unwrap();let nodes=mesh.external_faces.iter().flatten().copied().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();let affine=|p:[f64;3]|1.+700.*p[0]-1100.*p[1]+2300.*p[2];let volts=nodes.iter().map(|&i|affine(mesh.nodes_m[i])).collect::<Vec<_>>();
  let boundary=Boundary{potential_count:nodes.len(),metal_trace:nodes.iter().enumerate().map(|(i,&node)|TraceRow{node,weights:vec![(i,1.)]}).collect(),far_zero_nodes:vec![],insulated_external:true};let operator=CapacitanceOperator::new(&mesh,boundary,budget()).unwrap();let action=operator.apply(&volts).unwrap();let expected=0.5*EPS0*4.2*1e-9*(700_f64.powi(2)+1100_f64.powi(2)+2300_f64.powi(2));assert!((action.energy_j/expected-1.).abs()<1e-12);
  for(i,&p)in mesh.nodes_m.iter().enumerate(){assert!((action.nodal_potentials_v[i]+action.nodal_potential_corrections_v[i]-affine(p)).abs()<1e-13);}
  assert!(action.normalized_charge_balance<1e-12&&action.normalized_energy_reaction_error<1e-12);assert_eq!(operator.factor_value_count==0,nodes.len()==mesh.nodes_m.len());
 }
}
#[test]
fn complete_energy_gram_retains_sub_ulp_field_corrections(){
 use pcb_analyze::extraction::local::volume_field::ElectricApply;
 let coarse=cube(vec![-0.001,0.,0.001,0.002],false,false);let fine=cube(vec![-0.001,-1e-15,0.,0.001,0.001+1e-15,0.002],false,false);
 let space=volume_current::assemble(&coarse,&vec![58e6;coarse.copper_tetrahedra.len()],&[],budget()).unwrap();let field=volume_field::Field::new_vertical_refined(&fine,&coarse,&space,budget()).unwrap();let v=(0..space.potential_count()).map(|i|(i as f64*0.7).sin()).collect::<Vec<_>>();let boundary=field.potential_dofs.iter().map(|&i|v[i]).collect::<Vec<_>>();let action=field.operator.apply(&boundary).unwrap();assert!(action.nodal_potential_corrections_v.iter().any(|x|*x!=0.));
 let dense=volume_field::DenseField::new(&field,budget()).unwrap();let expected=field.apply(&v).unwrap();let actual=dense.apply(&v).unwrap();let relative=actual.iter().zip(&expected).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()/expected.iter().map(|x|x*x).sum::<f64>().sqrt();assert!(relative<1e-8,"full energy/charge discrepancy {relative:e}");
}
#[test]
fn four_source_workers_preserve_every_energy_gram_bit_and_resource_refusals(){
 use pcb_analyze::extraction::local::volume_field::ElectricApply;
 let coarse=cube(vec![-0.001,0.,0.001,0.002],false,false);let fine=cube(vec![-0.001,-1e-15,0.,0.001,0.001+1e-15,0.002],false,false);let space=volume_current::assemble(&coarse,&vec![58e6;coarse.copper_tetrahedra.len()],&[],budget()).unwrap();let field=volume_field::Field::new_vertical_refined(&fine,&coarse,&space,budget()).unwrap();let single=volume_field::DenseField::new(&field,budget()).unwrap();let parallel=volume_field::DenseField::new_parallel(&field,budget(),4).unwrap();
 for column in 0..space.potential_count(){let mut v=vec![0.;space.potential_count()];v[column]=1.;assert_eq!(single.apply(&v).unwrap(),parallel.apply(&v).unwrap());}
 for workers in[0,5]{assert!(volume_field::DenseField::new_parallel(&field,budget(),workers).err().unwrap().contains("workers1..4"));}
 assert!(volume_field::DenseField::new_parallel(&field,AllocationBudget{max_bytes:1024*1024,..budget()},4).err().unwrap().contains("MEMORY_LIMIT"));
}
fn cube(z:Vec<f64>,gap:bool,shift:bool)->LayeredMesh {
 let g=[-0.001,0.,0.001,0.002];let mut xy_m:Vec<_>=g.iter().flat_map(|&y|g.iter().map(move|&x|[x,y])).collect();if shift {xy_m[5][0]+=1e-5;}
 let mut triangles=vec![];let mut inside=vec![];for j in 0..3 {for i in 0..3 {let a=j*4+i;triangles.extend([[a,a+1,a+5],[a,a+5,a+4]]);inside.extend([i==1&&j==1;2]);}}
 let mut boundary_edges=vec![];for i in 0..3 {boundary_edges.extend([[i,i+1],[12+i,13+i],[4*i,4*(i+1)],[4*i+3,4*(i+1)+3]]);}
 let materials=z.windows(2).map(|s|inside.iter().map(|&cu|if cu&&s[0]>=0.&&s[1]<=0.001&&!(gap&&s[0]>=0.00025&&s[1]<=0.00075){Material::Copper{source_ids:vec!["actual-cube".into()]}}else{Material::Dielectric{label:"air".into(),epsilon_r:1.}}).collect()).collect();
 mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m:z,materials},budget()).unwrap()
}
#[test]
fn vertical_field_trace_preserves_affine_voltage_energy_charge_and_finite_contacts() {
 let coarse=cube(vec![-0.001,0.,0.001,0.002],false,false);let fine=cube(vec![-0.001,0.,0.0005,0.001,0.002],false,false);
 let cs=volume_current::assemble(&coarse,&vec![58e6;coarse.copper_tetrahedra.len()],&[],budget()).unwrap();let fs=volume_current::assemble(&fine,&vec![58e6;fine.copper_tetrahedra.len()],&[],budget()).unwrap();
 let lifted=volume_field::Field::new_vertical_refined(&fine,&coarse,&cs,budget()).unwrap();let direct=volume_field::Field::new(&fine,&fs,budget()).unwrap();
 for slope in [0.,1.,-3.] {let vc=cs.mesh_nodes.iter().map(|ns|1.+slope*coarse.nodes_m[ns[0]][2]).collect::<Vec<_>>();let vf=fs.mesh_nodes.iter().map(|ns|1.+slope*fine.nodes_m[ns[0]][2]).collect::<Vec<_>>();
  let qc=lifted.apply(&vc).unwrap();let qf=direct.apply(&vf).unwrap();let ec=vc.iter().zip(&qc).map(|(v,q)|v*q).sum::<f64>();let ef=vf.iter().zip(&qf).map(|(v,q)|v*q).sum::<f64>();assert!((ec-ef).abs()/ef<1e-10);
  let scale=qc.iter().map(|q|q.abs()).sum::<f64>();assert!((qc.iter().sum::<f64>()-qf.iter().sum::<f64>()).abs()/scale<1e-10);assert!(scale>0.);
 }
}
#[test]
fn field_trace_refuses_unknown_xy_and_a_coarse_dielectric_gap() {
 let coarse=cube(vec![-0.001,0.,0.001,0.002],false,false);let cs=volume_current::assemble(&coarse,&vec![58e6;coarse.copper_tetrahedra.len()],&[],budget()).unwrap();
 let shifted=cube(vec![-0.001,0.,0.0005,0.001,0.002],false,true);
 assert!(volume_field::Field::new_vertical_refined(&shifted,&coarse,&cs,budget()).err().unwrap().contains("XY vertex"));
 let gap=cube(vec![-0.001,0.,0.00025,0.00075,0.001,0.002],true,false);let gs=volume_current::assemble(&gap,&vec![58e6;gap.copper_tetrahedra.len()],&[],budget()).unwrap();let fine=cube(vec![-0.001,0.,0.0005,0.001,0.002],false,false);
 assert!(volume_field::Field::new_vertical_refined(&fine,&gap,&gs,budget()).err().unwrap().contains("dielectric gap"));
}
#[test]
fn full_capacitance_materialization_preserves_every_potential_and_energy() {
 use pcb_analyze::extraction::local::volume_field::ElectricApply;
 let mesh=cube(vec![-0.001,0.,0.0005,0.001,0.002],false,false);let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&[],budget()).unwrap();let field=volume_field::Field::new(&mesh,&space,budget()).unwrap();let dense=volume_field::DenseField::new(&field,budget()).unwrap();
 let vectors=(0..4).map(|k|(0..space.potential_count()).map(|i|if k==0{1.}else{((i+1)as f64*(k as f64+0.31)).sin()}).collect::<Vec<_>>()).collect::<Vec<_>>();
 for v in &vectors {let a=field.apply(v).unwrap();let b=dense.apply(v).unwrap();let scale=a.iter().map(|x|x*x).sum::<f64>().sqrt();let error=a.iter().zip(&b).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()/scale;assert!(error<1e-9,"all-coordinate charge error={error:e}");let scale=a.iter().map(|v|v.abs()).sum::<f64>();assert!((a.iter().sum::<f64>()-b.iter().sum::<f64>()).abs()<1e-9*scale);}
 let a=field.energy_gram(&vectors).unwrap();let b=dense.energy_gram(&vectors).unwrap();let scale=a.iter().map(|(_,_,v)|v.abs()).fold(0.,f64::max);for(i,j,v)in a{let w=b.iter().find(|&&(x,y,_)|x==i&&y==j).unwrap().2;assert!((v-w).abs()<1e-9*scale);}
 assert!(dense.max_charge_energy_column_error<1e-8);assert!(dense.max_directed_reciprocity<1e-9);assert!(volume_field::DenseField::new(&field,AllocationBudget{max_bytes:dense.assembly_peak_bytes-1,..budget()}).err().unwrap().contains("MEMORY_LIMIT"));
}

#[test]
fn complete_extracted_trace_checkpoint_replays_charge_and_refuses_wrong_source_or_audits(){
 use pcb_analyze::extraction::local::volume_field::ElectricApply;
 let mesh=cube(vec![-0.001,0.,0.0005,0.001,0.002],false,false);let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&[],budget()).unwrap();let field=volume_field::Field::new(&mesh,&space,budget()).unwrap();let n=space.potential_count();let mut c=vec![vec![0.;n];n];
 for j in 0..n{let mut v=vec![0.;n];v[j]=1.;let q=field.apply(&v).unwrap();for i in 0..n{c[i][j]=q[i];}}
 let physical=serde_json::json!({"epsilonR":1.,"finiteConductor":"cube"});
 let original=serde_json::json!({"kind":"source-p1-full-field-trace-v0","nodeBits":mesh.nodes_m.iter().map(|p|p.map(f64::to_bits)).collect::<Vec<_>>(),"copperCells":mesh.copper_tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"potentialMeshNodes":space.mesh_nodes,"physicalInputs":physical,"capacitanceF":c,"fullTraceComplete":true,"maxChargeEnergyColumnError":0.,"maxDirectedReciprocity":0.,"evidence":{"producer":"native field columns; replay control only","physicsAdmitted":false}});
 let path=std::env::temp_dir().join(format!("p7-field-trace-{}-{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
 let write=|value:&serde_json::Value|std::fs::write(&path,serde_json::to_vec(value).unwrap()).unwrap();write(&original);
 let cached=volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,budget()).unwrap();
 for k in 0..4{let v=(0..n).map(|i|((i+1)as f64*(k as f64+0.31)).sin()).collect::<Vec<_>>();let a=field.apply(&v).unwrap();let b=cached.apply(&v).unwrap();let scale=a.iter().map(|v|v*v).sum::<f64>().sqrt();assert!(a.iter().zip(&b).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()<1e-9*scale);let a=field.energy_gram(&[v.clone()]).unwrap();let b=cached.energy_gram(&[v]).unwrap();assert!((a[0].2-b[0].2).abs()<1e-9*a[0].2.abs());}
 assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&serde_json::json!({"epsilonR":4.2}),&path,budget()).err().unwrap().contains("identity"));
 assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,AllocationBudget{max_bytes:1,..budget()}).err().unwrap().contains("MEMORY_LIMIT"));
 let mut wrong=original.clone();wrong["nodeBits"][0][0]=serde_json::json!(mesh.nodes_m[0][0].to_bits()+1);write(&wrong);assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,budget()).err().unwrap().contains("identity"));
 let mut wrong=original.clone();wrong["fullTraceComplete"]=serde_json::json!(false);write(&wrong);assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,budget()).err().unwrap().contains("identity"));
 let mut wrong=original.clone();wrong["maxChargeEnergyColumnError"]=serde_json::json!(1e-6);write(&wrong);assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,budget()).err().unwrap().contains("audits"));
 let mut wrong=original.clone();wrong["capacitanceF"][0][1]=serde_json::json!(c[0][1]+1e-4*(c[0][0]*c[1][1]).sqrt());write(&wrong);assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,budget()).err().unwrap().contains("reciprocity"));
 let mut wrong=original.clone();wrong["capacitanceF"][0][0]=serde_json::json!(-1.);write(&wrong);assert!(volume_field::DenseField::load_checkpoint(&mesh,&space,&physical,&path,budget()).err().unwrap().contains("diagonal"));
 std::fs::remove_file(path).unwrap();
}
