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

#[test]
fn board_foil_checks_every_matched_column_on_one_shared_exact_grid() {
    let (board, geometry) = fixture();
    let request = serde_json::from_value(json!({
        "frequenciesHz":[1000.,10000.],"referenceOhm":[50.,50.],
        "budget":{"maxSeconds":300.,"maxMemoryMiB":128.}
    }))
    .unwrap();
    let result =
        board_foil::solve_checked(&board, &geometry, request, std::time::Instant::now()).unwrap();
    assert_eq!(result["status"], "complete");
    assert_eq!(result["coverage"], "unverified");
    assert_eq!(result["representation"], "S");
    assert_eq!(result["frequenciesHz"], json!([1000., 10000.]));
    assert_eq!(result["referenceOhm"], json!([50., 50.]));
    assert_eq!(result["ports"], json!(geometry.ports));
    assert!(result.get("model").is_none());
    assert_eq!(
        result["provenance"]["electrodeAreasM2"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
    assert!(
        result["provenance"]["sourceMapping"]
            .as_array()
            .unwrap()
            .len()
            >= 4
    );
    assert_eq!(result["provenance"]["spatialCoverage"], "unverified");
    assert_eq!(result["provenance"]["bandCoverage"], "unverified");
    for (i, &f) in [1000., 10000.].iter().enumerate() {
        let sample = &result["samples"][i];
        assert_eq!(sample["frequencyHz"], f);
        assert_eq!(sample["columns"].as_array().unwrap().len(), 2);
        for input in 0..2 {
            let c = &sample["columns"][input];
            assert_eq!(c["input"], input);
            assert!(c["kclRelative"].as_f64().unwrap() <= 1e-8);
            assert!(c["kvlRelative"].as_f64().unwrap() <= 1e-8);
            let copper = c["copperLossW"].as_f64().unwrap();
            assert!(copper >= 0.);
            let outgoing = (0..2)
                .map(|output| {
                    let wave = &result["parameters"][i][output][input];
                    wave["real"].as_f64().unwrap().powi(2)
                        + wave["imaginary"].as_f64().unwrap().powi(2)
                })
                .sum::<f64>();
            let net = 12.5 * (1. - outgoing);
            assert!((net - copper).abs() <= 1e-12 + 1e-8 * (net.abs() + copper));
        }
        assert!(
            result["parameters"][i][0][1]["imaginary"]
                .as_f64()
                .unwrap()
                .abs()
                > 0.
        );
    }
    assert!(result["enrichment"].as_array().unwrap().len() <= 17);
    eprintln!(
        "checked native grid quality={} history={} provenance={}",
        result["quality"], result["enrichment"], result["provenance"]
    );
}

#[test]
fn asymmetric_four_foil_stack_preserves_floating_planes_and_physical_response() {
    use pcb_analyze::{
        extraction::local::foil_reduced::TrainingPoint,
        model::{Layer, Side},
    };
    let (mut board, mut request) = fixture();
    let mut bottom = board.layers.pop().unwrap();
    bottom.physical_index = 3;
    for (index, name) in [(1, "IN1"), (2, "IN2")] {
        board.layers.push(Layer {
            name: name.into(),
            physical_index: index,
            side: Side::Inner,
            disable_routing: None,
        });
    }
    board.layers.push(bottom);
    for pad in &mut board.pads {
        if pad.layers == [1] {
            pad.layers[0] = 3;
        }
    }
    let return_foil = board.zones[1].clone();
    board.zones[1].layers = vec![3];
    board.zones[1].fills[0].layer = 3;
    for (layer, id) in [(1, "inner-one"), (2, "inner-two")] {
        let mut plane = return_foil.clone();
        plane.layers = vec![layer];
        plane.source.path = format!("test.{id}");
        plane.source.id = Some(id.into());
        plane.fills[0].layer = layer;
        plane.fills[0].source.path = format!("test.{id}.fill");
        plane.fills[0].source.id = Some(format!("{id}-fill"));
        plane.fills[0].native_ids = None;
        board.zones.push(plane);
    }

    let top_sigma = 58e6;
    let bottom_sigma = 41e6;
    let top_thickness = 5e-6;
    let bottom_thickness = 7e-6;
    let foil = |name: &str, thickness_m, conductivity| board_mesh::Layer {
        name: name.into(),
        copper: true,
        thickness_m,
        epsilon_r: None,
        conductivity_s_m: Some(conductivity),
    };
    let dielectric = |name: &str, thickness_m, epsilon| board_mesh::Layer {
        name: name.into(),
        copper: false,
        thickness_m,
        epsilon_r: Some(epsilon),
        conductivity_s_m: None,
    };
    request.stack = vec![
        foil("TOP", top_thickness, top_sigma),
        dielectric("d1", 35e-6, 2.7),
        foil("IN1", 6e-6, 51e6),
        dielectric("d2", 80e-6, 4.2),
        foil("IN2", 8e-6, 48e6),
        dielectric("d3", 120e-6, 3.3),
        foil("BOTTOM", bottom_thickness, bottom_sigma),
    ];
    let frequency = 1000.;
    let drive = [1., -1.];
    let training = TrainingPoint {
        frequency_hz: frequency,
        drive: drive.to_vec(),
        max_iterations: Some(12),
    };
    let prepared = board_foil::prepare_reduced_enriched(
        &board,
        &request,
        &[],
        frequency,
        budget(),
        Duration::from_secs(300),
        &training,
    )
    .unwrap();
    let space = &prepared.space;
    let model = space.diagnostic_model();
    for layer in ["TOP", "IN1", "IN2", "BOTTOM"] {
        assert!(prepared.supports.iter().any(|s| s.layer == layer));
    }
    for (path, layer) in [
        ("test.inner-one.fill.polygons[0]", "IN1"),
        ("test.inner-two.fill.polygons[0]", "IN2"),
    ] {
        let entry = prepared
            .source_mapping
            .iter()
            .find(|entry| entry.1 == path && entry.3 == layer)
            .unwrap_or_else(|| panic!("missing physical fill polygon {path} on {layer}"));
        assert!(
            prepared.supports.iter().any(|support| {
                support.layer == layer && support.source_region_ids.contains(&entry.0)
            }),
            "mapped fill polygon {path} is not retained in any {layer} prism"
        );
    }
    assert_eq!(model.conserved_potential_modes.len(), 5);
    assert!(space.max_electric_reciprocity <= 1e-9);

    let modes = &model.conserved_potential_modes;
    let electric = faer::Mat::from_fn(modes.len(), modes.len(), |i, j| {
        model
            .capacitance_f
            .iter()
            .map(|&(a, b, c)| modes[i][a] * c * modes[j][b])
            .sum::<f64>()
    });
    assert!(
        electric.llt(faer::Side::Lower).is_ok(),
        "floating plane charge metric must be positive definite"
    );
    let solve_budget = Budget {
        max_memory_mi_b: Some(128.),
        max_seconds: Some(3.),
    };
    let dc = space
        .solve_checked(0., &drive, solve_budget.clone())
        .unwrap();
    let drop = dc.voltage_real[0] - dc.voltage_real[1];
    let exact = 1.5e-3 / 0.5e-3
        * (1. / (top_sigma * top_thickness) + 1. / (bottom_sigma * bottom_thickness));
    assert!(
        (drop / exact - 1.).abs() < 1e-9,
        "DC drop {drop:e} vs {exact:e}"
    );
    let ac = space
        .solve_checked(frequency, &drive, solve_budget)
        .unwrap();
    assert!(ac.residual.kcl_relative <= 1e-8 && ac.residual.kvl_relative <= 1e-8);
    assert_eq!(ac.coverage, "unverified");
}

#[test]
fn mixed_thickness_board_checks_shared_matched_response_in_original_coordinates() {
    use pcb_analyze::extraction::local::mixed_conductor::{solve_checked, CheckedGrid};
    let (board, mut request) = fixture();
    // Use the same finite pads and dielectric. The bottom copper exceeds the
    // thin screen on its narrow cells, while the top retains sheet modes.
    request.stack.last_mut().unwrap().thickness_m = 0.1e-3;
    let checked = solve_checked(
        &board,
        &request,
        CheckedGrid {
            frequencies_hz: &[1_000.],
            reference_ohm: &[50., 50.],
            max_iterations: 16,
        },
        AllocationBudget {
            max_bytes: 512 * 1024 * 1024,
            ..budget()
        },
        Duration::from_secs(300),
    )
    .unwrap();
    assert_eq!(checked["coverage"], "unverified");
    assert_eq!(checked["ports"], json!(request.ports));
    assert!(checked["provenance"]["volumeTetrahedra"].as_u64().unwrap() > 0);
    assert!(checked["provenance"]["prisms"].as_u64().unwrap() > 0);
    let columns = checked["samples"][0]["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 2);
    for column in columns {
        assert!(column["kclRelative"].as_f64().unwrap() <= 1e-8);
        assert!(column["kvlRelative"].as_f64().unwrap() <= 1e-8);
        assert!(column["copperLossW"].as_f64().unwrap() > 0.);
    }
    let s21 = &checked["parameters"][0][1][0];
    assert!(s21["real"].as_f64().unwrap().abs() > 0.9);
    eprintln!(
        "mixed quality={} provenance={} enrichment={}",
        checked["quality"], checked["provenance"], checked["enrichment"]
    );
}

#[test]
fn profiled_joint_exact_sample_checks_the_existing_thick_coupon_at_100khz() {
    let (board, mut geometry) = fixture();
    // Same geometry as mixed_thickness_board_checks_shared_matched_response_in_original_coordinates.
    geometry.stack.last_mut().unwrap().thickness_m = 0.1e-3;
    let request = serde_json::from_value(json!({
        "frequenciesHz":[100000.],"referenceOhm":[50.,50.],"operatorPolicy":"skin-volume-v3",
        "budget":{"maxSeconds":1800.,"maxMemoryMiB":3072.}
    }))
    .unwrap();
    let report =
        board_foil::solve_checked(&board, &geometry, request, std::time::Instant::now()).unwrap();
    assert_eq!(report["frequenciesHz"], json!([100000.]));
    assert_eq!(report["ports"], json!(geometry.ports));
    assert_eq!(report["coverage"], "unverified");
    assert_eq!(report["provenance"]["operatorPolicy"], "skin-volume-v3");
    assert_eq!(report["provenance"]["bandCoverage"], "unverified");
    assert!(report["provenance"]["volumeTetrahedra"].as_u64().unwrap() > 0);
    let skin = &report["provenance"]["copperSkinMesh"];
    assert_eq!(skin["effectiveUpperHz"], json!(100000.));
    let bottom = skin["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["layer"] == "BOTTOM")
        .unwrap();
    assert_eq!(bottom["slabs"], 2);
    for column in report["samples"][0]["columns"].as_array().unwrap() {
        assert!(column["kclRelative"].as_f64().unwrap() <= 1e-8);
        assert!(column["kvlRelative"].as_f64().unwrap() <= 1e-8);
        assert!(column["copperLossW"].as_f64().unwrap() > 0.);
    }
    println!(
        "profiled joint evidence: {}",
        json!({
            "quality":report["quality"], "provenance":report["provenance"],
            "samples":report["samples"], "enrichment":report["enrichment"]
        })
    );
}

#[test]
fn shared_return_split_weld_matches_unsplit_exact_grid_and_retains_trace() {
    use pcb_analyze::extraction::local::mixed_conductor::{
        solve_checked_spatial_partition, CheckedGrid,
    };
    use std::time::Instant;
    let started = Instant::now();
    let wall = Duration::from_secs(1800);
    let remaining = || {
        wall.checked_sub(started.elapsed())
            .expect("shared fixture deadline")
    };
    let (board, mut geometry) = fixture();
    geometry.stack.last_mut().unwrap().thickness_m = 0.1e-3;
    let budget = AllocationBudget {
        max_bytes: 3072 * 1024 * 1024,
        ..budget()
    };
    // These two internal CDT edges resolve both legs of the quadrant cut
    // without changing copper ownership, materials or finite pad contacts.
    let traces = [[[0., 0.], [1., 0.]], [[0., 0.], [0., 0.25]]];
    let mesh = board_mesh::build_conforming(&board, &geometry, budget, remaining(), None, &traces)
        .unwrap();
    let bottom = mesh.layer_spans["BOTTOM"];
    let owners = mesh
        .mesh
        .copper_tetrahedra
        .iter()
        .map(|tet| {
            let points = tet.nodes.map(|node| mesh.mesh.nodes_m[node]);
            let in_bottom = points
                .iter()
                .all(|p| p[2] >= bottom[0] && p[2] <= bottom[1]);
            // Preserve ownership through the whole copper thickness. The
            // planar triangle centroid is shared by all of its tetrahedra.
            let planar =
                mesh.planar_triangles[tet.planar_triangle].map(|node| mesh.planar_nodes_mm[node]);
            let x_mm = planar.iter().map(|p| p[0]).sum::<f64>() / 3.;
            let y_mm = planar.iter().map(|p| p[1]).sum::<f64>() / 3.;
            usize::from(in_bottom && x_mm > 0. && y_mm > 0.)
        })
        .collect::<Vec<_>>();
    assert!(owners.contains(&0) && owners.contains(&1));

    // Geometry-only preflight: verify the actual conforming face set and
    // both planar normal directions before running either matched solve.
    let direction = |nodes: [usize; 3]| {
        let [a, b, c] = nodes.map(|i| mesh.mesh.nodes_m[i]);
        let u: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
        let v: [f64; 3] = std::array::from_fn(|i| c[i] - a[i]);
        let normal = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        (0..3)
            .max_by(|&i, &j| normal[i].abs().total_cmp(&normal[j].abs()))
            .unwrap()
    };
    let mut seen = std::collections::BTreeMap::new();
    let mut expected_faces = std::collections::BTreeSet::new();
    for (tet, &owner) in mesh.mesh.copper_tetrahedra.iter().zip(&owners) {
        for opposite in 0..4 {
            let mut face = [
                tet.nodes[(opposite + 1) % 4],
                tet.nodes[(opposite + 2) % 4],
                tet.nodes[(opposite + 3) % 4],
            ];
            face.sort_unstable();
            if seen.insert(face, owner).is_some_and(|other| other != owner) {
                expected_faces.insert(face);
            }
        }
    }
    let normal_mask = expected_faces
        .iter()
        .fold(0u8, |mask, &face| mask | (1 << direction(face)));
    assert_eq!(
        normal_mask, 0b011,
        "return partition must retain both x- and y-directed vertical faces before solving"
    );
    let grid = CheckedGrid {
        frequencies_hz: &[1000.],
        reference_ohm: &[50., 50.],
        max_iterations: 16,
    };
    let cap = 1024 * 1024;
    let missing = solve_checked_spatial_partition(
        &mesh,
        &geometry,
        grid,
        Some(&owners[..owners.len() - 1]),
        cap,
        budget,
        remaining(),
    );
    assert!(missing.unwrap_err().contains("SHARED_RETURN_OWNERSHIP"));
    let split = solve_checked_spatial_partition(
        &mesh,
        &geometry,
        grid,
        Some(&owners),
        cap,
        budget,
        remaining(),
    )
    .unwrap();
    let unsplit =
        solve_checked_spatial_partition(&mesh, &geometry, grid, None, 0, budget, remaining())
            .unwrap();
    assert_eq!(split["ports"], unsplit["ports"]);
    assert_eq!(
        split["provenance"]["sourceMapping"],
        unsplit["provenance"]["sourceMapping"]
    );
    for (actual, expected) in split["parameters"][0]
        .as_array()
        .unwrap()
        .iter()
        .zip(unsplit["parameters"][0].as_array().unwrap())
    {
        for (a, b) in actual
            .as_array()
            .unwrap()
            .iter()
            .zip(expected.as_array().unwrap())
        {
            let re = a["real"].as_f64().unwrap() - b["real"].as_f64().unwrap();
            let im = a["imaginary"].as_f64().unwrap() - b["imaginary"].as_f64().unwrap();
            assert!(re.hypot(im) <= 1e-10);
        }
    }
    assert_eq!(split["coverage"], "unverified");
    assert_eq!(unsplit["coverage"], "unverified");
    let topology = &split["spatialPartition"];
    assert_eq!(topology["owners"], 2);
    assert!(topology["tracePotentialDofs"].as_array().unwrap().len() > 1);
    let faces = topology["orientedCrossOwnerFaces"].as_array().unwrap();
    assert!(faces
        .iter()
        .all(|face| face["outwardPositiveOwner"] != face["outwardNegativeOwner"]));
    let reported_faces = faces
        .iter()
        .map(|face| serde_json::from_value::<[usize; 3]>(face["nodes"].clone()).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(reported_faces, expected_faces);
    let mut owner_flow = 0_f64;
    let mut common_displacement = 0_f64;
    for (col, base) in split["samples"][0]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .zip(unsplit["samples"][0]["columns"].as_array().unwrap())
    {
        let loss = col["copperLossW"].as_f64().unwrap();
        let control = base["copperLossW"].as_f64().unwrap();
        assert!((loss - control).abs() <= 1e-10 * loss.abs().max(control.abs()).max(1e-18));
        let trace = &col["spatialTrace"];
        assert!(trace["maxWeldRelative"].as_f64().unwrap() <= 1e-8);
        assert!(trace["maxKclRelative"].as_f64().unwrap() <= 1e-8);
        let owner_loss = trace["ownerJouleLossW"].as_array().unwrap();
        assert_eq!(owner_loss.len(), 2);
        assert!(owner_loss.iter().all(|value| value.as_f64().unwrap() > 0.));
        assert!(
            (owner_loss
                .iter()
                .map(|value| value.as_f64().unwrap())
                .sum::<f64>()
                - loss)
                .abs()
                <= 1e-8 * loss.max(1e-18)
        );
        for row in trace["traces"].as_array().unwrap() {
            let [re, im] =
                serde_json::from_value::<[f64; 2]>(row["commonDisplacementA"].clone()).unwrap();
            common_displacement = common_displacement.max(re.hypot(im));
            for owner in row["ownerDuals"].as_array().unwrap() {
                let [re, im] =
                    serde_json::from_value::<[f64; 2]>(owner["weakConductionA"].clone()).unwrap();
                owner_flow = owner_flow.max(re.hypot(im));
            }
        }
    }
    assert!(owner_flow > 1e-4 && common_displacement > 0.);
    println!(
        "P7_SHARED_RETURN_INTERFACE {}",
        json!({
            "schema":"p7-shared-return-interface-v1","frequencyHz":1000.,
            "ports":split["ports"],"sourceMapping":split["provenance"]["sourceMapping"],
            "topology":topology,"splitParameters":split["parameters"],"unsplitParameters":unsplit["parameters"],
            "splitColumns":split["samples"][0]["columns"],"unsplitColumns":unsplit["samples"][0]["columns"],
            "maxOwnerWeakConductionA":owner_flow,"maxCommonDisplacementA":common_displacement,
            "faceDirectionMask":normal_mask,"coverage":"unverified",
        })
    );
}
