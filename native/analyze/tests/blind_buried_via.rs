use pcb_analyze::{
    extraction::local::{board_mesh, mixed_conductor::{self, Owner}, volume_current, AllocationBudget},
    import_json,
};
use serde_json::{json, Value};
use std::time::Duration;

const LAYERS: [&str; 4] = ["TOP", "I1", "I2", "BOTTOM"];
const DIELECTRICS: [&str; 3] = ["D0", "D1", "D2"];
const FOIL_M: f64 = 0.000035;
const BUDGET: AllocationBudget = AllocationBudget {
    max_bytes: 256 * 1024 * 1024,
    max_nodes: 40_000,
    max_tetrahedra: 80_000,
};

fn square(r: f64) -> Value {
    json!([{"x":-r,"y":-r},{"x":r,"y":-r},{"x":r,"y":r},{"x":-r,"y":r}])
}

fn coupon(first: usize, net: &str, antipad_net: &str) -> Value {
    let mut board: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-segmented-coupon.json"
    ))
    .unwrap();
    board["outline"] = square(0.65);
    board["layers"] = json!([
        {"name":"TOP","index":0,"side":"top"},
        {"name":"I1","index":1,"side":"inner"},
        {"name":"I2","index":2,"side":"inner"},
        {"name":"BOTTOM","index":3,"side":"bottom"}
    ]);
    board["components"] = json!([]);
    // A conductor directly below the drill must survive intact and remain
    // galvanically separate even though it carries the same net label.
    board["pads"] = json!([{
        "id":"below", "number":"1", "net":net, "at":{"x":0,"y":0},
        "rotationDeg":0, "layers":["BOTTOM"], "shape":{"kind":"rect","widthMm":0.12,"heightMm":0.12}
    }]);
    board["copper"]["fixed"] = json!({
        "tracks":[],
        "vias":[{"id":"transition","net":net,"at":{"x":0,"y":0},"diameterMm":0.44,
                 "drillMm":0.2,"fromLayer":LAYERS[first],"toLayer":"I2","type":"blind-buried"}],
        "zones":[{"id":"plane","kind":"fill","net":antipad_net,"layers":["I1"],
                  "outline":{"outer":square(0.55)},
                  "filledPolygons":[{"layer":"I1","source":"cad","status":"available",
                     "freshness":"current","nativePrimitiveIds":["plane-native"],
                     "polygons":[{"outer":square(0.55),"holes":[square(0.36)]}]}]}]
    });
    board
}

fn request(first: usize, takeoff: usize, polygon_domain: bool) -> board_mesh::Request {
    let mut stack = Vec::new();
    for (i, layer) in LAYERS.into_iter().enumerate() {
        stack.push(json!({"name":layer,"thicknessM":FOIL_M,"copper":true,"conductivitySM":58e6}));
        if let Some(layer) = DIELECTRICS.get(i) {
            stack.push(json!({"name":layer,"thicknessM":0.00013,"copper":false,"epsilonR":4}));
        }
    }
    let mut value = json!({
        "roiMm":[[-0.6,-0.6],[0.6,0.6]],
        "stack":stack, "finishedHolePlatingM":0.000025,"barrelConductivitySM":56e6,
        "airMarginM":0.0002,"airEdgeDivisions":1,"boreRefinementRings":0,
        "maxTriangleAreaM2":2e-7,"maxVerticalStepM":0.0002,
        "minTriangleAngleDeg":0,"curveDeviationMm":0.001,
        "electrodes":[
            {"name":"start","sourcePath":"copper.fixed.vias[0]","layer":LAYERS[first],"upper":false},
            {"name":"takeoff","sourcePath":"copper.fixed.vias[0]","layer":LAYERS[takeoff],"upper":true},
            {"name":"below","sourcePath":"pads[0]","layer":"BOTTOM","upper":true}
        ],
        "ports":[["start","takeoff"]]
    });
    if polygon_domain {
        value["domainPolygonMm"] = json!([[-0.6, -0.6], [0.6, -0.6], [0.6, 0.6], [-0.6, 0.6]]);
    }
    serde_json::from_value(value).unwrap()
}

fn check_span(first: usize, takeoff: usize, net: &str, antipad_net: &str, polygon_domain: bool) {
    let imported = import_json(&coupon(first, net, antipad_net).to_string());
    assert!(!imported.has_errors(), "{:?}", imported.diagnostics);
    let board = imported.board.unwrap();
    let r = request(first, takeoff, polygon_domain);
    let mesh = board_mesh::build(&board, &r, BUDGET, Duration::from_secs(15)).unwrap();
    let hole = &mesh.geometry.holes[0];
    assert!(hole
        .layers
        .iter()
        .map(String::as_str)
        .eq(LAYERS[first..=2].iter().copied()));
    assert_eq!(mesh.barrel_ids, [hole.id.clone()]);
    let mut via_foil_layers: Vec<_> = mesh
        .source_mapping
        .iter()
        .filter(|(_, path, _, _)| path == &hole.source.path)
        .map(|(_, _, _, layer)| layer.as_str())
        .collect();
    via_foil_layers.sort_unstable();
    let mut expected_layers = LAYERS[first..=2].to_vec();
    expected_layers.sort_unstable();
    assert_eq!(via_foil_layers, expected_layers);

    let start_z = mesh.layer_spans[LAYERS[first]][0];
    let end_z = mesh.layer_spans["I2"][1];
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    let mut below_volume = 0.;
    let mut plane_volume = 0.;
    let below_id = &mesh
        .source_mapping
        .iter()
        .find(|m| m.1 == "pads[0]")
        .unwrap()
        .0;
    let plane_id = &mesh
        .source_mapping
        .iter()
        .find(|m| m.3 == "I1" && m.2.as_deref() == Some(antipad_net))
        .unwrap()
        .0;
    for (tet, sigma) in mesh
        .mesh
        .copper_tetrahedra
        .iter()
        .zip(&mesh.conductivity_s_m)
    {
        if tet.source_ids.contains(&hole.id) {
            assert_eq!(*sigma, 56e6);
            for i in tet.nodes {
                low = low.min(mesh.mesh.nodes_m[i][2]);
                high = high.max(mesh.mesh.nodes_m[i][2]);
            }
        }
        if tet.source_ids.contains(below_id) {
            below_volume += tet.volume_m3;
        }
        if tet.source_ids.contains(plane_id) {
            plane_volume += tet.volume_m3;
        }
    }
    assert_eq!(
        [low, high],
        [start_z, end_z],
        "retain the entire plated stub beyond the signal takeoff"
    );
    assert!((below_volume / (0.12e-3f64.powi(2) * FOIL_M) - 1.).abs() < 1e-6);
    let plane_area_m2 = 1.1e-3f64.powi(2) - 0.72e-3f64.powi(2);
    assert!(
        (plane_volume / (plane_area_m2 * FOIL_M) - 1.).abs() < 1e-6,
        "captured antipad area"
    );

    for (i, name) in DIELECTRICS.into_iter().enumerate() {
        let lo = mesh.layer_spans[LAYERS[i]][1];
        let hi = mesh.layer_spans[LAYERS[i + 1]][0];
        let mut bore_cells = 0;
        for tet in &mesh.mesh.tetrahedra {
            let nodes = tet.nodes.map(|n| mesh.mesh.nodes_m[n]);
            let mid: [f64; 3] =
                std::array::from_fn(|k| nodes.iter().map(|p| p[k]).sum::<f64>() / 4.);
            let xy = [
                mid[0] * 1e3 + mesh.origin_mm[0],
                mid[1] * 1e3 + mesh.origin_mm[1],
            ];
            if lo < mid[2] && mid[2] < hi && xy[0].hypot(xy[1]) < 0.06 {
                bore_cells += 1;
                let drilled = (first..2).contains(&i);
                assert_eq!(tet.material_label, if drilled { "air" } else { name });
                assert_eq!(tet.epsilon_r, if drilled { 1. } else { 4. });
            }
        }
        assert!(
            bore_cells > 0,
            "fixture must sample the via centre at every dielectric depth"
        );
    }

    let electrodes: Vec<_> = r
        .electrodes
        .iter()
        .map(|e| mesh.electrode_nodes(e).unwrap())
        .collect();
    let current =
        volume_current::assemble(&mesh.mesh, &mesh.conductivity_s_m, &electrodes, BUDGET).unwrap();
    let [start, end, below] = std::array::from_fn(|i| current.node_potential[&electrodes[i][0]]);
    assert!(current
        .components
        .iter()
        .any(|c| c.contains(&start) && c.contains(&end)));
    assert!(!current
        .components
        .iter()
        .any(|c| c.contains(&start) && c.contains(&below)));
    if takeoff < 2 {
        assert!(
            mesh.electrode_areas_m2["takeoff"] < mesh.electrode_areas_m2["start"],
            "the continuing barrel hides part of the takeoff face"
        );
    }

    // Hybrid ownership is checked on this same finite-span copper mesh;
    // this is a topology control, not an electromagnetic accuracy result.
    let hybrid = mixed_conductor::prepare(&mesh, &r.electrodes, 1e6, BUDGET, Duration::from_secs(15)).unwrap();
    assert_eq!(hybrid.owners.len(), mesh.mesh.copper_tetrahedra.len());
    let mut barrel_cells=0;
    for (i,tet) in mesh.mesh.copper_tetrahedra.iter().enumerate() {
        if tet.source_ids.contains(&hole.id) {
            barrel_cells+=1;
            assert_eq!(hybrid.owners[i],Owner::Volume,"barrel must retain local volume ownership");
            for &node in &tet.nodes { let z=mesh.mesh.nodes_m[node][2]; assert!(z>=start_z && z<=end_z); }
        }
        if let Owner::Foil(index)=hybrid.owners[i] {
            assert!(hybrid.foils[index].tetrahedra.contains(&i));
        }
    }
    assert!(barrel_cells>0);
    for (index,foil) in hybrid.foils.iter().enumerate() {
        assert!(foil.tetrahedra.iter().all(|&i|hybrid.owners[i]==Owner::Foil(index)));
    }
    let [start,end,below]:[usize;3]=hybrid.electrodes.as_slice().try_into().unwrap();
    assert!(hybrid.potential_components.iter().any(|c|c.contains(&start)&&c.contains(&end)));
    assert!(!hybrid.potential_components.iter().any(|c|c.contains(&start)&&c.contains(&below)));

    let mut invalid = r;
    invalid.electrodes[0].layer = "BOTTOM".into();
    let error = board_mesh::build(&board, &invalid, BUDGET, Duration::from_secs(15))
        .err()
        .unwrap();
    assert!(
        error.contains("source copper.fixed.vias[0] absent on requested layer BOTTOM"),
        "{error}"
    );
}

#[test]
fn blind_signal_via_preserves_the_stub_and_stops_before_bottom_copper() {
    check_span(0, 1, "SIGNAL", "RETURN", true);
}

#[test]
fn buried_return_via_preserves_dielectric_above_and_below_its_span() {
    check_span(1, 2, "RETURN", "SIGNAL", false);
}

#[test]
fn missing_or_unknown_via_endpoint_cannot_reach_meshing() {
    let mut source = coupon(1, "RETURN", "SIGNAL");
    source["copper"]["fixed"]["vias"][0]["toLayer"] = json!("UNKNOWN");
    let imported = import_json(&source.to_string());
    assert!(imported.board.is_none());
    assert!(imported
        .diagnostics
        .iter()
        .any(|d| d.code == "unknown_layer" && d.path.ends_with(".toLayer")));

    source["copper"]["fixed"]["vias"][0]
        .as_object_mut()
        .unwrap()
        .remove("toLayer");
    let imported = import_json(&source.to_string());
    assert!(imported.board.is_none());
    assert!(imported
        .diagnostics
        .iter()
        .any(|d| d.code == "invalid_structure" && d.message.contains("toLayer")));
}

#[test]
fn native_via_endpoint_dc_matches_analytic_annulus() {
    use std::f64::consts::PI;

    const DC_FOIL_M: f64 = 1e-6;
    const CORE_M: f64 = 0.00013;
    const RADIUS_M: f64 = 0.0001;
    const PLATING_M: f64 = 0.000025;
    const SIGMA: f64 = 56e6;
    let area_m2 = PI * ((RADIUS_M + PLATING_M).powi(2) - RADIUS_M.powi(2));
    for (first, last, kind) in [
        (0, 3, "through"),
        (0, 2, "blind-buried"),
        (1, 2, "blind-buried"),
    ] {
        for (net, antipad_net) in [("SIGNAL", "RETURN"), ("RETURN", "SIGNAL")] {
            let mut source = coupon(first, net, antipad_net);
            let via = &mut source["copper"]["fixed"]["vias"][0];
            via["toLayer"] = json!(LAYERS[last]);
            via["type"] = json!(kind);
            if last == 3 {
                source["pads"] = json!([]);
            }
            let imported = import_json(&source.to_string());
            assert!(!imported.has_errors(), "{:?}", imported.diagnostics);
            let mut r = request(first, last, first != 1);
            if last == 3 {
                r.electrodes.pop();
            }
            // Thin contact foils bound end spreading and intermediate land
            // shunting while keeping the electrodes on actual copper faces.
            for layer in &mut r.stack {
                if layer.copper {
                    layer.thickness_m = DC_FOIL_M;
                }
            }
            r.curve_deviation_mm = 0.0001;
            let mesh = board_mesh::build(
                &imported.board.unwrap(),
                &r,
                BUDGET,
                Duration::from_secs(15),
            )
            .unwrap_or_else(|e| panic!("{kind} {net} {first}..{last}: {e}"));
            let electrodes: Vec<_> = r
                .electrodes
                .iter()
                .map(|e| mesh.electrode_nodes(e).unwrap())
                .collect();
            let start_z = first as f64 * (DC_FOIL_M + CORE_M);
            let length_m = (last - first) as f64 * CORE_M + (last - first + 1) as f64 * DC_FOIL_M;
            let end_z = start_z + length_m;
            for (nodes, z) in electrodes.iter().zip([start_z, end_z]) {
                assert!(nodes
                    .iter()
                    .all(|&i| (mesh.mesh.nodes_m[i][2] - z).abs() < 1e-15));
            }
            let [min_z, max_z] = mesh
                .mesh
                .copper_tetrahedra
                .iter()
                .filter(|cell| cell.source_ids.contains(&mesh.barrel_ids[0]))
                .flat_map(|cell| cell.nodes.map(|i| mesh.mesh.nodes_m[i][2]))
                .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], z| {
                    [lo.min(z), hi.max(z)]
                });
            assert!((min_z - start_z).abs() < 1e-15 && (max_z - end_z).abs() < 1e-15);

            let space =
                volume_current::assemble(&mesh.mesh, &mesh.conductivity_s_m, &electrodes, BUDGET)
                    .unwrap();
            let [start, end] = std::array::from_fn(|i| space.node_potential[&electrodes[i][0]]);
            let mut boundary = vec![0.; space.potential_count()];
            boundary[end] = 1.;
            let voltage = space
                .dc_factor(&[start, end], BUDGET)
                .unwrap()
                .solve(&space, &boundary, &vec![0.; boundary.len()])
                .unwrap();
            let reaction = space.dc_apply(&voltage).unwrap();
            let conductance = reaction[end];
            let expected = SIGMA * area_m2 / length_m;
            let relative_error = (conductance / expected - 1.).abs();
            eprintln!("{kind} {net} {first}..{last}: measured={conductance:.9e} S analytic={expected:.9e} S rel={relative_error:.6e}");
            // Largest foil fraction is 2/132. Tessellation of each annular
            // boundary at 0.1 um adds less than 0.81% area uncertainty.
            assert!(relative_error < 0.025);
            assert!((reaction[start] + conductance).abs() / expected < 1e-8);
            if let Some(nodes) = electrodes.get(2) {
                let outside = space.node_potential[&nodes[0]];
                assert!(reaction[outside].abs() / expected < 1e-8);
                assert!(!space
                    .components
                    .iter()
                    .any(|c| c.contains(&start) && c.contains(&outside)));
            }
        }
    }
}
