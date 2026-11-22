use pcb_analyze::extraction::local::{board_mesh,AllocationBudget};
#[test]
fn whole_stm32_joint_preserves_complete_source_barrel_and_finite_contacts(){
 let board=pcb_analyze::import_json(include_str!("../../../tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json")).board.unwrap();
 let request:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/stm32-joint-reference-request.json")).unwrap();
 let mesh=board_mesh::build(&board,&request,AllocationBudget{max_bytes:8*1024*1024*1024,max_nodes:500000,max_tetrahedra:1000000},std::time::Duration::from_secs(600)).unwrap();
 assert!(mesh.max_copper_area_change<=1e-8);assert!(mesh.max_contact_area_change<=1e-8);
 assert!(mesh.mesh.copper_volume_m3.is_finite()&&mesh.mesh.copper_volume_m3>0.);
 assert_eq!(mesh.source_mapping.len(),mesh.geometry.regions.len());
 for region in &mesh.geometry.regions {assert!(mesh.source_mapping.iter().any(|m|m.0==region.id&&m.1==region.source.path&&m.3==region.layer));}
 assert_eq!(mesh.barrel_ids.len(),mesh.geometry.holes.iter().filter(|h|h.plated).count());
 for hole in mesh.geometry.holes.iter().filter(|h|h.plated){assert!(mesh.barrel_ids.contains(&hole.id));}
 for contact in &request.electrodes {assert!(mesh.electrode_nodes(contact).unwrap().len()>=3);assert!(mesh.electrode_areas_m2[&contact.name]>0.);}
 let evidence=serde_json::json!({"seconds":mesh.elapsed_seconds,"sourceMappings":mesh.source_mapping.len(),"barrels":mesh.barrel_ids.len(),"holes":mesh.geometry.holes.len(),"atoms":mesh.arrangement_atoms,"nodes":mesh.mesh.nodes_m.len(),"copperTetrahedra":mesh.mesh.copper_tetrahedra.len(),"dielectricTetrahedra":mesh.mesh.tetrahedra.len(),"copperVolumeM3":mesh.mesh.copper_volume_m3,"maxCopperAreaChange":mesh.max_copper_area_change,"maxContactAreaChange":mesh.max_contact_area_change,"contactAreasM2":mesh.electrode_areas_m2});
 eprintln!("{evidence}");
 if let Some(dir)=std::env::var_os("P7_TEST_EVIDENCE_DIR"){std::fs::create_dir_all(&dir).unwrap();std::fs::write(std::path::Path::new(&dir).join("stm32-whole-geometry.json"),serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();}
}
