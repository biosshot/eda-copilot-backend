use pcb_analyze::extraction::local::{board_mesh,AllocationBudget};
#[test]
fn automatic_stm32_complete_via_contacts_preserve_source_area_without_filtering_copper(){
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
 let board=pcb_analyze::import_json(&std::fs::read_to_string(root.join("tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json")).unwrap()).board.unwrap();
 let request:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(root.join("tests/fixtures/analysis-local-extraction/board-integration/stm32-automatic-via-request.json")).unwrap()).unwrap();
 let mesh=board_mesh::build(&board,&request,AllocationBudget{max_bytes:2*1024*1024*1024,max_nodes:100000,max_tetrahedra:262144},std::time::Duration::from_secs(300)).unwrap();
 assert!(mesh.max_contact_area_change<=1e-8);assert!(mesh.max_copper_area_change<=1e-8);assert!(mesh.mesh.copper_volume_m3>0.);
 for contact in &request.electrodes{assert!(mesh.electrode_nodes(contact).unwrap().len()>=3);}
 if let Some(directory)=std::env::var_os("P7_TEST_EVIDENCE_DIR"){std::fs::create_dir_all(&directory).unwrap();std::fs::write(std::path::Path::new(&directory).join("stm32-automatic-contact-geometry.json"),serde_json::to_vec_pretty(&serde_json::json!({"request":request,"sourceCopperRestorations":mesh.source_copper_restorations,"maxContactAreaChange":mesh.max_contact_area_change,"maxCopperAreaChange":mesh.max_copper_area_change,"electrodeAreasM2":mesh.electrode_areas_m2,"nodes":mesh.mesh.nodes_m.len(),"copperTetrahedra":mesh.mesh.copper_tetrahedra.len(),"dielectricTetrahedra":mesh.mesh.tetrahedra.len(),"wallSeconds":mesh.elapsed_seconds,"scope":"geometry/contact coverage only, no field/reduction accuracy certificate"})).unwrap()).unwrap();}

}

#[test]
fn accepted_explicit_stm32_geometry_does_not_need_source_reconciliation(){
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let board=pcb_analyze::import_json(&std::fs::read_to_string(root.join("tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json")).unwrap()).board.unwrap();
 let request:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(root.join("tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap()).unwrap();
 let mesh=board_mesh::build(&board,&request,AllocationBudget{max_bytes:2*1024*1024*1024,max_nodes:100000,max_tetrahedra:262144},std::time::Duration::from_secs(300)).unwrap();assert!(mesh.source_copper_restorations.is_empty(),"historical accepted geometry must remain on its original path");assert!(mesh.max_contact_area_change<=1e-8);
}


#[test]
fn finite_interior_via_contact_keeps_barrel_continuity_and_separate_return(){
 use pcb_analyze::extraction::local::volume_current;
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
 let mut source:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(root.join("tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json")).unwrap()).unwrap();
 source["layers"]=serde_json::json!([{"name":"TOP","index":0,"side":"top"},{"name":"INNER1","index":1,"side":"inner"},{"name":"BOTTOM","index":2,"side":"bottom"}]);
 source["pads"]=serde_json::json!([]);source["copper"]["fixed"]["tracks"]=serde_json::json!([]);source["copper"]["fixed"]["zones"]=serde_json::json!([]);
 source["copper"]["fixed"]["vias"]=serde_json::json!([source["copper"]["fixed"]["vias"][0].clone(),source["copper"]["fixed"]["vias"].as_array().unwrap().iter().find(|v|v["id"]=="aa8ff23739620a07").unwrap().clone()]);
 let board=pcb_analyze::import_json(&source.to_string()).board.unwrap();
 let mut request:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(root.join("tests/fixtures/analysis-local-extraction/board-integration/stm32-automatic-via-request.json")).unwrap()).unwrap();
 let mut dielectric=request.stack[1].clone();dielectric.thickness_m=0.0007475;let mut inner=request.stack[0].clone();inner.name="INNER1".into();
 request.stack=vec![request.stack[0].clone(),dielectric.clone(),inner,dielectric,request.stack[2].clone()];
 request.electrodes[1].layer="INNER1".into();request.electrodes[1].upper=false;
 // Keep the second captured via as a physically disconnected neighbour.
 request.electrodes.truncate(2);request.ports=vec![[request.electrodes[0].name.clone(),request.electrodes[1].name.clone()]];
 let budget=AllocationBudget{max_bytes:2*1024*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 let mesh=board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(300)).unwrap();
 let electrodes=request.electrodes.iter().map(|e|mesh.electrode_nodes(e).unwrap()).collect::<Vec<_>>();
 let space=volume_current::assemble(&mesh.mesh,&mesh.conductivity_s_m,&electrodes,budget).unwrap();
 assert_eq!(space.components.len(),2,"no net-name connection to the neighbouring barrel");
 let a=space.node_potential[&electrodes[0][0]];let b=space.node_potential[&electrodes[1][0]];assert_ne!(a,b);
 assert!(space.components.iter().any(|c|c.contains(&a)&&c.contains(&b)));
 let mut injection=vec![0.;space.potential_count()];injection[a]=1.;injection[b]=-1.;
 let voltage=space.dc_factor(&[],budget).unwrap().solve(&space,&vec![0.;space.potential_count()],&injection).unwrap();
 assert!(voltage[a]-voltage[b]>0.);
 let current=space.cells.iter().flat_map(|c|(0..3).map(|axis|(0..4).map(|j|c.incidence[j][axis]*voltage[c.potentials[j]]).sum::<f64>()/c.resistance_ohm)).collect::<Vec<_>>();
 let residual=space.divergence(&current).unwrap().iter().zip(&injection).map(|(a,b)|(a-b).abs()).fold(0.,f64::max);assert!(residual<1e-8,"interior terminal KCL {residual:e}");
 let loss=space.losses(&current).unwrap().iter().sum::<f64>();assert!((loss/(voltage[a]-voltage[b])-1.).abs()<1e-8);
}
