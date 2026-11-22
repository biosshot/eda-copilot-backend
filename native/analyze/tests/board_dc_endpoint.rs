use pcb_analyze::extraction::local::board_dc::{self, Request};
use serde_json::{json, Value};

#[test]
fn finite_board_loop_dc_keeps_physical_ports_and_open_gauges() {
    let board = pcb_analyze::import_json(include_str!(
        "../../../tests/fixtures/analysis-segmented-coupon.json"
    ))
    .board
    .unwrap();
    let mut geometry: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json"
    )).unwrap();
    let roi = [[-0.3, -0.6], [10.3, 0.6]];
    let thickness = 1e-5_f64;
    let sigma = 58e6_f64;
    geometry["roiMm"] = json!(roi);
    geometry["allowInsulatedCopperCuts"] = json!(false);
    geometry["airMarginM"] = json!(0.0002);
    geometry["stack"][0]["thicknessM"] = json!(thickness);
    geometry["stack"][1]["thicknessM"] = json!(1e-4);
    geometry["stack"][2]["thicknessM"] = json!(thickness);
    // Different electrode and port ordering exercises the authored names.
    geometry["electrodes"] = json!([
        {"name":"r0","sourcePath":"pads[2]","layer":"BOTTOM","upper":true},
        {"name":"s1","sourcePath":"pads[1]","layer":"TOP","upper":false},
        {"name":"s0","sourcePath":"pads[0]","layer":"TOP","upper":false},
        {"name":"r1","sourcePath":"pads[3]","layer":"BOTTOM","upper":true},
    ]);
    let ports = [["s0", "r0"], ["s1", "r1"]];
    geometry["ports"] = json!(ports);
    let request: Request = serde_json::from_value(json!({
        "geometry": geometry, "drive":[1.,-1.], "observablesOnly":true
    }))
    .unwrap();
    let report = board_dc::run(&board, request, 300.).unwrap();
    assert_eq!(report["status"], "complete");
    assert_eq!(report["frequencyHz"], 0);
    assert_eq!(report["roiMm"], json!(roi));
    assert_eq!(report["allowInsulatedCopperCuts"], false);
    assert_eq!(report["electrodeNames"], json!(["r0", "s1", "s0", "r1"]));
    assert_eq!(report["ports"], json!(ports));
    assert_eq!(report["portRealV"], json!([null, null]));
    assert_eq!(report["floatingPortIndices"], json!([0, 1]));
    assert_eq!(report["loadLossW"].as_f64(), Some(0.));
    assert!(report.get("cellLossW").is_none());

    let islands = report["electrodeGalvanicComponents"].as_array().unwrap();
    assert_eq!(islands[0], islands[3]);
    assert_eq!(islands[1], islands[2]);
    assert_ne!(islands[0], islands[1]);
    assert_eq!(
        report["electrodeLoadedComponents"],
        report["electrodeGalvanicComponents"]
    );
    assert_eq!(report["dcConductanceS"][0][1].as_f64(), Some(0.));
    let sources = report["sourceMapping"].as_array().unwrap();
    for source in [
        "pads[0]",
        "pads[1]",
        "pads[2]",
        "pads[3]",
        "copper.fixed.tracks[0]",
        "copper.fixed.tracks[1]",
    ] {
        assert!(
            sources.iter().any(|row| row[1] == source),
            "missing {source}"
        );
    }

    // Uniform rail reference, with an explicit 5% allowance for the finite
    // face contacts extending 0.1 mm inward at each end. At one ampere the
    // gauge-invariant loop power is numerically equal to loop resistance.
    let uniform_loop_ohm = 10e-3 / (sigma * thickness) * (1. / 0.1e-3 + 1. / 0.5e-3);
    let power = report["drivePowerW"].as_f64().unwrap();
    let copper_loss = report["totalLossW"].as_f64().unwrap();
    assert!(
        (power / uniform_loop_ohm - 1.).abs() < 0.05,
        "{power:e} vs {uniform_loop_ohm:e}"
    );
    assert!((copper_loss / power - 1.).abs() < 1e-8);
}
