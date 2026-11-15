// Shared authored inputs for captured-board mathematical gates and assembly QA.
use serde_json::{json, Value};
pub fn captured_board_request(fixture: &str, designator: &str, power: f64, edge: f64) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../tests/pcb-analyze/{fixture}/{fixture}.json"));
    let material = |copper: bool| json!({"thermalConductivityWPerMK":if copper{390.}else{0.3},"densityKgM3":if copper{8960.}else{1850.},"specificHeatJPerKgK":if copper{385.}else{1100.},"source":"Explicit captured-board test hypothesis","state":"constant isotropic"});
    json!({"contextId":"spatial-reference","mode":"steady","ambientC":30.,
        "network":{"nodes":[{"id":format!("{designator}:junction"),"heatCapacityJPerK":0.2},{"id":format!("{designator}:case"),"heatCapacityJPerK":0.8}],"links":[{"id":format!("{designator}:jc"),"from":format!("{designator}:junction"),"to":format!("{designator}:case"),"cooling":{"resistanceKPerW":3.}}],"sources":[{"node":format!("{designator}:junction"),"powerW":power}]},
        "board":{"boardJson":std::fs::read_to_string(path).unwrap(),"boardIdentity":fixture,"snapshot":true,"maxEdgeMm":edge,"maxNodes":50000,
            "layers":[{"kind":"copper","name":"TOP","thicknessMm":0.035,"material":material(true),"background":material(false)},{"kind":"dielectric","name":"core","thicknessMm":1.53,"material":material(false)},{"kind":"copper","name":"BOTTOM","thicknessMm":0.035,"material":material(true),"background":material(false)}],
            "heat":[],"contacts":[{"id":format!("{designator}:0"),"node":format!("{designator}:case"),"target":{"kind":"component","designator":designator},"resistanceKPerW":8.}],"surfaceCooling":{"resistanceKPerW":35.},"barrel":{"thicknessMm":0.025,"diameterConvention":"finished_hole","material":material(true)}},
        "budget":{"maxSeconds":120.,"maxMemoryMiB":384.}})
}
