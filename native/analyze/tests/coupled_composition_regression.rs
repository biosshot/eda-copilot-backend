use pcb_analyze::electrical::{coupled::{composition, System},frequency};
#[test]
fn physical_bend_composition_preserves_certified_conservation_modes(){
 let raw:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/bend-p6-conservation-request.json")).unwrap();
 let topology:composition::Topology=serde_json::from_value(serde_json::json!({"instances":raw["instances"],"connections":raw["connections"],"connectionContacts":raw["connectionContacts"],"ports":raw["ports"]})).unwrap();
 let assembled=composition::build(&topology,None,std::time::Instant::now()).unwrap();
 System::compile(&assembled.model,topology.ports.len(),None).unwrap();
 let result=frequency::run(serde_json::from_value(raw).unwrap());
 assert_eq!(result["status"],"complete","{result}");
}

#[test]
fn physical_via_composition_preserves_exact_constraint_support(){
 let raw:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/via-p6-conservation-request.json")).unwrap();
 let topology:composition::Topology=serde_json::from_value(serde_json::json!({"instances":raw["instances"],"connections":raw["connections"],"connectionContacts":raw["connectionContacts"],"ports":raw["ports"]})).unwrap();
 let assembled=composition::build(&topology,None,std::time::Instant::now()).unwrap();
 System::compile(&assembled.model,topology.ports.len(),None).unwrap();
 let result=frequency::run(serde_json::from_value(raw).unwrap());
 assert_eq!(result["status"],"complete","{result}");
}
