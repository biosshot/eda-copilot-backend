//! P7-R stage 3.2: straight 10 mm microstrip coupon, compact hybrid model against the 2D polygon cross-section.
//! Diagnostic, one frequency per run (env-driven): `MICROSTRIP_HZ` (default 1e6), `MICROSTRIP_T_M` (copper thickness,
//! default 35e-6). The model side follows the process switches (`P7_FOIL_SHEET`, ...), so the same test gives the
//! one-sheet run and the layered reference run. Physical accuracy stays unverified.
use super::*;

fn parameter(parameters: &serde_json::Value, i: usize, j: usize) -> (f64, f64) {
    let entry = &parameters[0][i][j];
    (entry["real"].as_f64().unwrap(), entry["imaginary"].as_f64().unwrap())
}
fn mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) { (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0) }
fn div(a: (f64, f64), b: (f64, f64)) -> (f64, f64) { let d = b.0 * b.0 + b.1 * b.1; ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d) }

/// Input impedance of the two-port with port 2 shorted: Gamma = S11 - S12 S21 / (1 + S22), Z = z0 (1 + G) / (1 - G).
fn shorted_input_impedance(parameters: &serde_json::Value, z0: f64) -> (f64, f64) {
    let (s11, s12, s21, s22) = (parameter(parameters, 0, 0), parameter(parameters, 0, 1), parameter(parameters, 1, 0), parameter(parameters, 1, 1));
    let correction = div(mul(s12, s21), (1. + s22.0, s22.1));
    let gamma = (s11.0 - correction.0, s11.1 - correction.1);
    let z = div((1. + gamma.0, gamma.1), (1. - gamma.0, -gamma.1));
    (z0 * z.0, z0 * z.1)
}

#[test]
#[ignore = "diagnostic: stage 3.2 microstrip R and L per metre against the 2D cross-section"]
fn diag_microstrip_r_l_against_cross_section() {
    let hz: f64 = std::env::var("MICROSTRIP_HZ").ok().map_or(1e6, |v| v.parse().unwrap());
    let thickness: f64 = std::env::var("MICROSTRIP_T_M").ok().map_or(35e-6, |v| v.parse().unwrap());
    // `MICROSTRIP_LEN_MM` (default 10) rescales the coupon length; two lengths separate the end effects.
    let length_mm: f64 = std::env::var("MICROSTRIP_LEN_MM").ok().map_or(10., |v| v.parse().unwrap());
    let length_m = length_mm * 1e-3;
    let mut board_json: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
    board_json["outline"][1]["x"] = serde_json::json!(length_mm + 0.3);
    board_json["outline"][2]["x"] = serde_json::json!(length_mm + 0.3);
    board_json["components"][1]["at"]["x"] = serde_json::json!(length_mm);
    for pad in board_json["pads"].as_array_mut().unwrap() { if pad["component"] == "J1" { pad["at"]["x"] = serde_json::json!(length_mm); } }
    for track in board_json["copper"]["fixed"]["tracks"].as_array_mut().unwrap() { track["points"][1]["x"] = serde_json::json!(length_mm); }
    // `MICROSTRIP_KIND=plane`: 1 mm signal over a solid return plane (4 mm wide track), 0.1 mm substrate (stage 3.3a).
    let plane = std::env::var("MICROSTRIP_KIND").as_deref() == Ok("plane");
    // `MICROSTRIP_PLANE_MM` (default 4) is the return plane width of the plane coupon (3.3b uses 3 with 2 mm length).
    let plane_mm: f64 = std::env::var("MICROSTRIP_PLANE_MM").ok().map_or(4., |v| v.parse().unwrap());
    let (half_y, substrate_m) = if plane { (plane_mm / 2. + 0.6, 0.0001) } else { (0.8, 0.0002) };
    if plane {
        for (i, y) in [-half_y - 0.4, -half_y - 0.4, half_y + 0.4, half_y + 0.4].into_iter().enumerate() { board_json["outline"][i]["y"] = serde_json::json!(y); }
        for pad in board_json["pads"].as_array_mut().unwrap() { pad["shape"]["heightMm"] = serde_json::json!(1.0); }
        for track in board_json["copper"]["fixed"]["tracks"].as_array_mut().unwrap() { track["widthMm"] = serde_json::json!(if track["id"] == "signal" { 1.0 } else { plane_mm }); }
    }
    let board = crate::import_json(&board_json.to_string()).board.unwrap();
    let request: board_mesh::Request = serde_json::from_value(serde_json::json!({
        "roiMm": [[-0.5, -half_y], [length_mm + 0.5, half_y]],
        "stack": [
            {"name": "TOP", "copper": true, "thicknessM": thickness, "conductivitySM": 58000000, "epsilonR": null},
            {"name": "substrate", "copper": false, "thicknessM": substrate_m, "conductivitySM": null, "epsilonR": 4},
            {"name": "BOTTOM", "copper": true, "thicknessM": thickness, "conductivitySM": 58000000, "epsilonR": null}],
        "finishedHolePlatingM": 2.5e-05, "barrelConductivitySM": 58000000, "airMarginM": 0.002,
        "maxTriangleAreaM2": std::env::var("MICROSTRIP_AREA_M2").ok().map_or(1e-6, |v| v.parse::<f64>().unwrap()), "maxVerticalStepM": 0.002, "curveDeviationMm": 0.001,
        "electrodes": [
            {"name": "foil-terminal-0", "sourcePath": "pads[0]", "layer": "TOP", "upper": false},
            {"name": "foil-terminal-1", "sourcePath": "pads[1]", "layer": "TOP", "upper": false},
            {"name": "foil-terminal-2", "sourcePath": "pads[2]", "layer": "BOTTOM", "upper": true},
            {"name": "foil-terminal-3", "sourcePath": "pads[3]", "layer": "BOTTOM", "upper": true}],
        "ports": [["foil-terminal-0", "foil-terminal-2"], ["foil-terminal-1", "foil-terminal-3"]],
        "acceptUnknownFillFreshness": true, "allowInsulatedCopperCuts": false, "minTriangleAngleDeg": 0, "airEdgeDivisions": 1,
        "domainPolygonMm": [[-0.5, -half_y], [length_mm + 0.5, -half_y], [length_mm + 0.5, half_y], [-0.5, half_y]], "cutPlanes": {}
    })).unwrap();
    let mib: usize = std::env::var("MICROSTRIP_MIB").ok().map_or(2048, |v| v.parse().unwrap());
    let budget = AllocationBudget { max_bytes: mib * 1024 * 1024, max_nodes: 1_000_000, max_tetrahedra: 2_000_000 };
    let wall = Duration::from_secs(std::env::var("MICROSTRIP_WALL_S").ok().map_or(900, |v| v.parse().unwrap()));

    // 2D reference of the same board at mid-length.
    let geometry = crate::geometry::prepare(&board, crate::geometry::GeometryOptions::default());
    let reference_region = geometry.regions.iter()
        .filter(|r| r.layer == "BOTTOM" && r.source.net.as_deref() == Some("RETURN"))
        .max_by(|a, b| a.area_mm2.total_cmp(&b.area_mm2)).expect("return copper region");
    let slice = crate::extraction::board_section::Slice {
        at_mm: [length_mm / 2., 0.], transverse_unit: [0., 1.], half_width_mm: half_y + 0.2, air_margin_mm: 2.,
        reference_layer: "BOTTOM".into(), reference_source_path: reference_region.source.path.clone(), reference_x_mm: 0.,
    };
    let extracted = crate::extraction::board_section::slice(&geometry, &request.stack, &slice).unwrap();
    let mut section: crate::extraction::polygon_section::Request = serde_json::from_value(extracted["geometry"].clone()).unwrap();
    section.max_triangle_area_m2 = 2e-9;
    let signal_count = section.conductors.len();
    let plain = crate::extraction::polygon_section::solve(&section).unwrap();
    let external_h_per_m = plain["parameters"]["inductanceHPerM"][0][0].as_f64().unwrap();
    section.loss = Some(crate::extraction::polygon_section::LossInput {
        frequencies_hz: vec![hz], conductivity_s_m: vec![58e6; signal_count], minimum_skin_dimensions_m: vec![thickness; signal_count],
        dielectric_loss_tangents: section.dielectrics.iter().map(|_| 0.).collect(), band_hz: [hz, hz],
    });
    let (section_r, section_total_l) = match crate::extraction::polygon_section::solve(&section) {
        Ok(lossy) => { let s = &lossy["frequencyDependent"]["samples"][0]; (s["resistanceOhmPerM"][0][0].as_f64(), s["totalInductanceHPerM"][0][0].as_f64()) }
        Err(e) => { eprintln!("MICROSTRIP_2D_LOSS refused: {e:?}"); (None, None) }
    };

    // 3D compact model.
    let profile = Some(CheckedOperator::HybridPlanarV3.skin_profile(hz, Some(hz), request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap());
    let started = Instant::now();
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, budget, wall, profile).unwrap();
    eprintln!("MICROSTRIP_MESH cells={} nodes={}", mesh.mesh.copper_tetrahedra.len(), mesh.mesh.nodes_m.len());
    if std::env::var_os("MICROSTRIP_MESH_ONLY").is_some() { return; }
    let grid = CheckedGrid { frequencies_hz: &[hz], reference_ohm: &[50., 50.], max_iterations: 16 };
    let result = solve_checked_mesh_impl_policy(&mesh, &request, grid, profile, &[], None, PartitionPolicy::AutomaticHybridV1, CheckedOperator::HybridPlanarV3, budget, started, wall);
    match result {
        Ok(r) => {
            let z = shorted_input_impedance(&r["parameters"], 50.);
            let c = &r["provenance"]["compactOperator"];
            let history = c["enrichment"].as_array().map_or(0, |h| h.len());
            let thickness_over_skin = thickness * (std::f64::consts::PI * super::super::super::MU0 * hz * 58e6).sqrt();
            eprintln!("MICROSTRIP {}", serde_json::json!({
                "hz": hz, "thicknessM": thickness, "thicknessOverSkin": thickness_over_skin, "lengthM": length_m, "plane": plane, "lengthMm": length_mm,
                "zShortOhm": [z.0, z.1], "modelROhmPerM": z.0 / length_m, "modelLHPerM": z.1 / (2. * std::f64::consts::PI * hz) / length_m,
                "section2dExternalLHPerM": external_h_per_m, "section2dTotalLHPerM": section_total_l, "section2dROhmPerM": section_r,
                "seconds": started.elapsed().as_secs_f64(), "iterations": history, "quality": r["quality"],
                "foilSheet": c["foilSheet"], "sliverPolicy": r["quality"]["sliverPolicy"],
                "copperSkinBarrelSheet": r["provenance"]["copperSkinMesh"]["barrelSheet"],
                "cells": mesh.mesh.copper_tetrahedra.len(), "nodes": mesh.mesh.nodes_m.len(), "barrels": mesh.barrel_ids.len(),
            }));
        }
        Err(e) => eprintln!("MICROSTRIP REFUSED hz={hz:e} seconds={:.1} error={e:?}", started.elapsed().as_secs_f64()),
    }
}
