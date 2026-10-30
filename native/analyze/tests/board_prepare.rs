use pcb_analyze::extraction::local::{board_mesh,board_prepare};
#[test]
fn explicit_verification_holdouts_are_not_replaced_with_the_legacy_axis(){
 let requested=[1.,10.,100.,316.22776601683796,1000.];assert_eq!(board_prepare::verification_axis(&requested).unwrap(),&requested);assert_eq!(board_prepare::verification_axis(&[]).unwrap(),&board_prepare::FREQUENCIES_HZ);for invalid in [vec![0.],vec![100.,10.],vec![1.,1.],vec![f64::NAN]]{assert!(board_prepare::verification_axis(&invalid).is_err());}
}
#[test]
fn accepted_electric_defaults_match_frozen_candidate_and_preserve_physical_inputs() {
    let current:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
    let expected:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/field-refinement-request-v1.json")).unwrap();
    let actual=board_prepare::accepted_electric_request(&current);
    assert_eq!(serde_json::to_value(&actual).unwrap(),serde_json::to_value(&expected).unwrap());
    assert_eq!(board_mesh::physical_signature(&current),board_mesh::physical_signature(&actual));
    assert_ne!(current.air_margin_m,actual.air_margin_m);
    assert_eq!(current.max_vertical_step_m,0.002);
}

#[test]
fn electric_refinement_retains_finer_current_vertical_boundaries(){
 let mut current:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();current.max_vertical_step_m=0.00153/3.9;
 let field=board_prepare::accepted_electric_request(&current);assert_eq!(field.max_vertical_step_m,current.max_vertical_step_m);assert_eq!(board_mesh::physical_signature(&current),board_mesh::physical_signature(&field));
}
#[test]
fn absent_internal_cut_transport_preserves_the_historical_physical_signature(){
 let request:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();let signature=board_mesh::physical_signature(&request);assert!(signature.get("cutPlanes").is_none());assert!(signature.get("electricMaxEdgeM").is_none());
}
#[test]
fn increased_explicit_developer_memory_admits_geometry_by_the_same_byte_plan(){
 let default=pcb_analyze::extraction::local::board_prepare::allocation_budget(2048);assert_eq!(default.max_nodes,100000);assert_eq!(default.max_tetrahedra,262144);
 let large=pcb_analyze::extraction::local::board_prepare::allocation_budget(3100);assert!(large.max_nodes>=139601);assert!(large.max_tetrahedra>=814320);assert!(large.check(814320*2048).is_ok());assert!(large.check(large.max_bytes+1).is_err());
}
