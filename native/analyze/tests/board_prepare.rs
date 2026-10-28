use pcb_analyze::extraction::local::{board_mesh,board_prepare};
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
