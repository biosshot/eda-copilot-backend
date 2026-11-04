use pcb_analyze::{
    electrical::{
        coupled::{self, System},
        Counters,
    },
    extraction::local::{
        board_foil::{self, TraceCut},
        board_mesh, volume_current, AllocationBudget,
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
    let outer = points(&[[-1., -1.], [1., -1.], [1., 1.], [-1., 1.]]);
    let hole = points(&[[-0.25, -0.25], [0., 0.25], [0.25, -0.25]]);
    let zone = |id, layer, net, holes: Vec<Vec<Value>>| {
        let p = json!({"outer":outer,"holes":holes});
        json!({"id":id,"kind":"pour","net":net,"layers":[layer],"outline":p,
               "priority":1,"preserveIslands":true,
               "refillRules":{"status":"unknown","unresolved":["Authored finite foil control"]},
               "filledPolygons":[{"layer":layer,"source":"cad","status":"available","freshness":"unknown","polygons":[p]}]})
    };
    b["copper"]["fixed"]["zones"] = json!([
        zone("signal-foil", "TOP", "SIGNAL", vec![hole.clone()]),
        zone("return-foil", "BOTTOM", "RETURN", vec![hole])
    ]);
    let pads = [[-0.75, -0.25], [0.5, 0.75]];
    for (i, pad) in b["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
        pad["at"] = json!({"x":pads[i%2][0],"y":pads[i%2][1]});
        pad["shape"] = json!({"kind":"rect","widthMm":0.25,"heightMm":0.25});
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
fn board_foil_retains_hole_pads_and_two_noncollinear_spatial_traces() {
    let (board, request) = fixture();
    let cuts = [
        TraceCut {
            id: "left".into(),
            layer: "BOTTOM".into(),
            endpoints_mm: [[-0.5, -1.], [-0.5, 1.]],
        },
        TraceCut {
            id: "top".into(),
            layer: "BOTTOM".into(),
            endpoints_mm: [[-1., 0.5], [1., 0.5]],
        },
    ];
    let prepared = board_foil::prepare(
        &board,
        &request,
        &cuts,
        10_000.,
        budget(),
        Duration::from_secs(120),
    )
    .unwrap();
    let space = &prepared.space;
    eprintln!(
        "foil supports={} potentials={} quadrature={} reciprocity={} refinedPairs={} fieldError={}",
        prepared.supports.len(),
        space.model.potential_nodes.len(),
        space.max_quadrature_change,
        space.max_reciprocity_error,
        space.reciprocity_refined_pairs,
        prepared.field_charge_energy_error
    );
    assert!(space.reciprocity_refined_pairs > 0);
    assert!(space.max_reciprocity_error <= 1e-9);
    assert_eq!(prepared.source_mapping.len(), 6);
    assert_eq!(prepared.electrode_areas_m2.len(), 4);
    assert!(prepared
        .electrode_areas_m2
        .values()
        .all(|a| (a - 6.25e-8).abs() < 1e-15));
    assert_eq!(prepared.supports.len(), space.cells.len());
    let copper_volume = prepared
        .supports
        .iter()
        .map(|s| s.copper_volume_m3)
        .sum::<f64>();
    assert!((copper_volume - 3.875e-11).abs() < 1e-18);
    assert!(prepared.max_copper_volume_change < 1e-8);
    assert!(prepared.field_charge_energy_error < 1e-8);
    for support in &prepared.supports {
        assert!(!support.source_region_ids.is_empty());
        assert!(support.source_region_ids.iter().all(|id| prepared
            .source_mapping
            .iter()
            .any(|s| &s.0 == id && s.3 == support.layer)));
    }
    let left = &prepared.trace_knots["left"];
    let top = &prepared.trace_knots["top"];
    assert!(left.len() >= 3 && top.len() >= 3);
    let left_shared = left.iter().find(|k| k.position_mm == [-0.5, 0.5]).unwrap();
    let top_shared = top.iter().find(|k| k.position_mm == [-0.5, 0.5]).unwrap();
    assert_eq!(left_shared.potential, top_shared.potential);
    assert_ne!(left[0].potential, left.last().unwrap().potential);
    assert_ne!(top[0].potential, top.last().unwrap().potential);
    let mut p6 = System::compile(&space.model, 2, None).unwrap();
    let initial = vec![0.; space.model.potential_nodes.len() + space.model.current_dofs.len()];
    let drive = p6.port_rhs(&[1., -1.]);
    let dc = p6
        .solve(0., drive, &initial, &mut Counters::default())
        .unwrap();
    // Sum the weak reaction from each side of each cut independently.
    // All physical port loads are on pads away from the two cuts.
    let nv = space.model.potential_nodes.len();
    for (name, axis, at) in [("left", 0, -0.5), ("top", 1, 0.5)] {
        let is_trace = |dof| {
            prepared.trace_knots[name]
                .iter()
                .any(|k| k.potential == dof)
        };
        let mut flow = [0., 0.];
        for &(dof, mode, a) in &space.model.incidence {
            let cell = mode / 2;
            if prepared.supports[cell].layer != "BOTTOM" || !is_trace(dof) {
                continue;
            }
            let centroid = request.roi_mm[0][axis]
                + 1e3
                    * space.cells[cell]
                        .triangle_m
                        .iter()
                        .map(|p| p[axis])
                        .sum::<f64>()
                    / 3.;
            flow[usize::from(centroid > at)] += a * dc[nv + mode];
        }
        assert!(
            (flow[0] + flow[1]).abs() < 1e-8 && (flow[0].abs() - 1.).abs() < 1e-8,
            "{name}: {flow:?}"
        );
    }
    let foil_r = dc[0] - dc[2] - dc[1] + dc[3];
    let lines: Vec<_> = cuts.iter().map(|c| c.endpoints_mm).collect();
    let matched = board_mesh::build_conforming(
        &board,
        &request,
        budget(),
        Duration::from_secs(20),
        None,
        &lines,
    )
    .unwrap();
    let matched_r = volume_loop_r(&matched, &request);
    let mut refined_request = request.clone();
    refined_request.max_triangle_area_m2 = 1.25e-7;
    let refined =
        board_mesh::build(&board, &refined_request, budget(), Duration::from_secs(20)).unwrap();
    let refined_r = volume_loop_r(&refined, &refined_request);
    eprintln!("foil loop R={foil_r:.9e}; matched-XY volume R={matched_r:.9e}; finer no-cut volume R={refined_r:.9e}");
    assert!((foil_r / matched_r - 1.).abs() < 0.02);
    let ac = coupled::impedance(&space.model, 2, &[1e3, 1e4], None).unwrap();
    assert_eq!(ac["parameters"].as_array().unwrap().len(), 2);
    assert!(ac["quality"]["kclAbsA"].as_f64().unwrap() < 1e-8);
    assert!(space
        .model
        .inductance_h
        .iter()
        .any(|&(i, j, l)| i != j && l > 0.));
    assert!(space
        .model
        .capacitance_f
        .iter()
        .any(|&(i, j, c)| i != j && c < 0.));
}

fn volume_loop_r(b: &board_mesh::BoardMesh, request: &board_mesh::Request) -> f64 {
    let contacts = request
        .electrodes
        .iter()
        .map(|e| b.electrode_nodes(e).unwrap())
        .collect::<Vec<_>>();
    let full = volume_current::assemble(&b.mesh, &b.conductivity_s_m, &contacts, budget()).unwrap();
    let dofs: Vec<_> = contacts
        .iter()
        .map(|nodes| full.node_potential[&nodes[0]])
        .collect();
    let factor = full.dc_factor(&[dofs[0], dofs[2]], budget()).unwrap();
    let mut rhs = vec![0.; full.potential_count()];
    for (&i, a) in dofs.iter().zip([1., -1., -1., 1.]) {
        rhs[i] = a;
    }
    let volts = factor.solve(&full, &vec![0.; rhs.len()], &rhs).unwrap();
    dofs.iter().map(|&i| volts[i] * rhs[i]).sum()
}

#[test]
fn trace_across_a_real_copper_hole_is_refused() {
    let (board, request) = fixture();
    let cut = TraceCut {
        id: "through-hole".into(),
        layer: "BOTTOM".into(),
        endpoints_mm: [[-1., 0.], [1., 0.]],
    };
    let error = match board_foil::prepare(
        &board,
        &request,
        &[cut],
        10_000.,
        budget(),
        Duration::from_secs(20),
    ) {
        Ok(_) => panic!("trace should not cross a copper opening"),
        Err(error) => error,
    };
    assert!(error.contains("BOARD_FOIL_TRACE"), "{error}");
}
