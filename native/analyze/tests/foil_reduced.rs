use pcb_analyze::{
    electrical::Budget,
    extraction::local::{
        board_foil::{self, TraceCut},
        board_mesh, AllocationBudget,
    },
    import_json,
};
use serde_json::{json, Value};
use std::time::Duration;

fn points(xy: &[[f64; 2]]) -> Vec<Value> {
    xy.iter().map(|p| json!({"x":p[0],"y":p[1]})).collect()
}

fn fixture() -> (pcb_analyze::model::Board, board_mesh::Request) {
    let mut b: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-segmented-coupon.json"
    ))
    .unwrap();
    b["outline"] = json!(points(&[
        [-1.25, -1.25],
        [1.25, -1.25],
        [1.25, 1.25],
        [-1.25, 1.25]
    ]));
    b["copper"]["fixed"]["tracks"] = json!([]);
    let outer = points(&[[-1., -0.25], [1., -0.25], [1., 0.25], [-1., 0.25]]);
    let zone = |id, layer, net, holes: Vec<Vec<Value>>| {
        let p = json!({"outer":outer,"holes":holes});
        json!({"id":id,"kind":"pour","net":net,"layers":[layer],"outline":p,
               "priority":1,"preserveIslands":true,
               "refillRules":{"status":"unknown","unresolved":["Authored finite foil control"]},
               "filledPolygons":[{"layer":layer,"source":"cad","status":"available","freshness":"unknown","polygons":[p]}]})
    };
    b["copper"]["fixed"]["zones"] = json!([
        zone("signal-foil", "TOP", "SIGNAL", vec![]),
        zone("return-foil", "BOTTOM", "RETURN", vec![])
    ]);
    let floating =
        json!({"outer":points(&[[0.25,0.375],[0.5,0.375],[0.5,0.625],[0.25,0.625]]),"holes":[]});
    b["copper"]["fixed"]["zones"].as_array_mut().unwrap().push(json!({
        "id":"floating","kind":"pour","net":"SIGNAL","layers":["TOP"],"outline":floating,
        "priority":1,"preserveIslands":true,
        "refillRules":{"status":"unknown","unresolved":["Authored floating foil control"]},
        "filledPolygons":[{"layer":"TOP","source":"cad","status":"available","freshness":"unknown","polygons":[floating]}]
    }));
    let pads = [[-0.875, 0.], [0.875, 0.]];
    for (i, pad) in b["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
        pad["at"] = json!({"x":pads[i%2][0],"y":pads[i%2][1]});
        pad["shape"] = json!({"kind":"rect","widthMm":0.25,"heightMm":0.5});
    }
    let imported = import_json(&b.to_string());
    let board = imported
        .board
        .unwrap_or_else(|| panic!("{:?}", imported.diagnostics));
    let electrodes: Vec<_> = ["TOP","TOP","BOTTOM","BOTTOM"].into_iter().enumerate().map(|(i, layer)|
        json!({"name":format!("p{i}"),"sourcePath":format!("pads[{i}]"),"layer":layer,"upper":layer=="BOTTOM"})
    ).collect();
    let req = serde_json::from_value(json!({
        "roiMm":[[-1.25,-1.25],[1.25,1.25]],
        "stack":[
            {"name":"TOP","thicknessM":5e-6,"copper":true,"epsilonR":null,"conductivitySM":58e6},
            {"name":"dielectric","thicknessM":1e-4,"copper":false,"epsilonR":4.,"conductivitySM":null},
            {"name":"BOTTOM","thicknessM":5e-6,"copper":true,"epsilonR":null,"conductivitySM":58e6}
        ],
        "airMarginM":0.0005,"maxTriangleAreaM2":5e-6,"maxVerticalStepM":0.00025,
        "airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,
        "acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,
        "electrodes":electrodes,"ports":[["p0","p2"],["p1","p3"]]
    })).unwrap();
    (board, req)
}

fn budget() -> AllocationBudget {
    AllocationBudget {
        max_bytes: 128 * 1024 * 1024,
        max_nodes: 10000,
        max_tetrahedra: 30000,
    }
}

#[test]
fn sparse_harmonic_producer_retains_contacts_checks_full_residual_and_reports_unverified() {
    let (board, request) = fixture();
    let cut = TraceCut {
        id: "middle".into(),
        layer: "BOTTOM".into(),
        endpoints_mm: [[0., -0.25], [0., 0.25]],
    };
    let prepared = board_foil::prepare_reduced(
        &board,
        &request,
        &[cut],
        10_000.,
        budget(),
        Duration::from_secs(30),
    )
    .unwrap();
    let reduced = &prepared.space;
    let model = reduced.diagnostic_model();
    assert_eq!(model.port_terminals.as_ref().unwrap().len(), 2);
    assert_eq!(model.conserved_potential_modes.len(), 3);
    let knots = &prepared.trace_knots["middle"];
    assert_eq!(knots.len(), 2);
    assert_ne!(knots[0].potential, knots[1].potential);
    assert!(model.current_dofs.len() < prepared.supports.len() * 2);
    assert!(reduced.dc_harmonic_residual < 1e-8);
    assert!(reduced.max_magnetic_reciprocity <= 1e-9);
    let budget = Budget {
        max_memory_mi_b: Some(64.),
        max_seconds: Some(3.),
    };
    let dc = reduced
        .solve_checked(0., &[1., -1.], budget.clone())
        .unwrap();
    let r = dc.voltage_real[0] - dc.voltage_real[1];
    let exact = 2. * 1.5e-3 / (58e6 * 5e-6 * 0.5e-3);
    assert!((r / exact - 1.).abs() < 1e-10, "R={r:e} exact={exact:e}");
    assert_eq!(dc.coverage, "unverified");
    assert!(dc.residual.kcl_relative < 1e-8 && dc.residual.kvl_relative < 1e-8);
    let ac = match reduced.solve_checked(10_000., &[1., -1.], budget) {
        Err(e) => e,
        Ok(_) => panic!("harmonic-only model unexpectedly passed the full AC residual"),
    };
    assert!(ac.contains("FOIL_REDUCTION_RESIDUAL"), "{ac}");
    eprintln!("prisms={} retainedPotentials={} currentModes={} plannedBytes={} DC R={r:e} rejected AC={ac}", prepared.supports.len(), model.potential_nodes.len(), model.current_dofs.len(), reduced.planned_bytes);
}

#[test]
fn bounded_foil_enrichment_passes_full_ac_residual_on_the_same_coupon() {
    let (board, request) = fixture();
    let cut = TraceCut {
        id: "middle".into(),
        layer: "BOTTOM".into(),
        endpoints_mm: [[0., -0.25], [0., 0.25]],
    };
    let training = pcb_analyze::extraction::local::foil_reduced::TrainingPoint {
        frequency_hz: 10_000.,
        drive: vec![1., -1.],
        max_iterations: None,
    };
    let prepared = board_foil::prepare_reduced_enriched(
        &board,
        &request,
        &[cut],
        10_000.,
        budget(),
        Duration::from_secs(300),
        &training,
    )
    .unwrap();
    let space = &prepared.space;
    let evidence = space.enrichment.as_ref().unwrap();
    assert_eq!(evidence.training.frequency_hz, training.frequency_hz);
    assert_eq!(evidence.training.drive, training.drive);
    assert_eq!(evidence.history.len(), evidence.iterations + 1);
    assert!(evidence
        .history
        .windows(2)
        .all(|pair| pair[0].iteration + 1 == pair[1].iteration
            && pair[0].potential_modes <= pair[1].potential_modes
            && pair[0].current_modes <= pair[1].current_modes));
    assert!(evidence.initial_residual.kvl_relative > 1e-8);
    assert!(
        evidence.iterations <= 16
            && evidence.added_current_modes > 0
            && evidence.added_current_modes <= 32
    );
    assert!(evidence.added_potential_modes <= 32);
    let point = space
        .solve_checked(
            training.frequency_hz,
            &training.drive,
            Budget {
                max_memory_mi_b: Some(64.),
                max_seconds: Some(3.),
            },
        )
        .unwrap();
    assert!(point.residual.kcl_relative <= 1e-8 && point.residual.kvl_relative <= 1e-8);
    assert_eq!(point.coverage, "unverified");
    assert_ne!(
        prepared.trace_knots["middle"][0].potential,
        prepared.trace_knots["middle"][1].potential
    );
    assert_eq!(space.diagnostic_model().conserved_potential_modes.len(), 3);
    eprintln!(
        "enrichment={evidence:?} plannedBytes={} pointResidual={:?}",
        space.planned_bytes, point.residual
    );
}
