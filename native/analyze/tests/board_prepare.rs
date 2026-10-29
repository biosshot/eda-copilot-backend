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
