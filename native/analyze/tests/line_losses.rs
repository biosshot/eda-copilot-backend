use pcb_analyze::extraction::{self, losses, Conductor, Dielectric, Geometry};
use serde_json::json;
use serde_json::Value;

#[test]
fn independent_complex_propagation_and_long_absorbing_limit() {
    let references: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-line-losses/references.json"
    ))
    .unwrap();
    for case in references["propagation"].as_array().unwrap() {
        let output = extraction::propagation::lossy_network(
            &[json!({"frequencyHz":case["frequencyHz"],"modes":case["modes"]})],
            case["lengthM"].as_f64().unwrap(),
            50.,
        )
        .unwrap();
        let actual = output["parameters"][0].as_array().unwrap();
        for (i, row) in actual.iter().enumerate() {
            for (j, z) in row.as_array().unwrap().iter().enumerate() {
                for part in ["real", "imaginary"] {
                    let a = z[part].as_f64().unwrap();
                    let b = case["scattering"][i][j][part].as_f64().unwrap();
                    assert!(
                        (a - b).abs() <= 1e-10 + 1e-8 * b.abs(),
                        "{} [{i},{j}] {part}: {a} vs {b}",
                        case["name"]
                    );
                }
            }
        }
        assert!(output["maxPowerBalanceResidual"].as_f64().unwrap() <= 1e-8);
        assert!(output["maxPassivityExcess"].as_f64().unwrap() <= 1e-10);
        if let Some(power) = case.get("independentPower") {
            let incident = power["incident"].as_array().unwrap();
            let mut outgoing = 0.;
            for row in actual {
                let mut re = 0.;
                let mut im = 0.;
                for (z, a) in row.as_array().unwrap().iter().zip(incident) {
                    let zr = z["real"].as_f64().unwrap();
                    let zi = z["imaginary"].as_f64().unwrap();
                    let ar = a["real"].as_f64().unwrap();
                    let ai = a["imaginary"].as_f64().unwrap();
                    re += zr * ar - zi * ai;
                    im += zr * ai + zi * ar;
                }
                outgoing += 0.5 * (re * re + im * im);
            }
            let incoming = power["incidentW"].as_f64().unwrap();
            let dissipated = power["integratedDissipatedW"].as_f64().unwrap();
            assert!((incoming - outgoing - dissipated).abs() / incoming <= 1e-8);
        }
    }
}
#[test]
fn parallel_plate_surface_normalization() {
    let g = Geometry {
        conductors: vec![Conductor {
            id: "signal".into(),
            role: "signal PEC".into(),
            bounds_m: [0., 0.001, 0.002, 0.0011],
        }],
        dielectrics: vec![Dielectric {
            id: "bottom".into(),
            y_min_m: 0.,
            y_max_m: 0.001,
            relative_permittivity: 4.,
            state: "analytic".into(),
        }],
        bounds_m: [0., 0., 0.002, 0.001],
        reference_y_m: vec![0.],
        length: None,
        kind: "analytic plate".into(),
        artificial_boundary: "natural sides".into(),
    };
    let f = extraction::solve_field(&g, 0.0001).unwrap();
    let p = losses::integrals(&f).unwrap();
    for (i, s) in p.surfaces.iter().enumerate() {
        assert!((s.gram_per_m[0][0] / 500. - 1.).abs() < 1e-10);
        assert!((s.integrated_current_a_per_a[0] - if i == 0 { 1. } else { -1. }).abs() < 1e-10);
    }
}
#[test]
fn surface_feasibility() {
    for kind in [
        "microstrip",
        "stripline",
        "coupled-microstrip",
        "coupled-stripline",
    ] {
        let mat = json!({"relativePermittivity":4.,"lossTangent":0.,"state":"test"});
        let mut line = json!({"kind":kind,"widthMm":0.3,"copperThicknessMm":0.035});
        if kind.contains("microstrip") {
            line["dielectricHeightMm"] = json!(0.18);
            line["material"] = mat.clone();
        } else {
            line["bottomDielectricHeightMm"] = json!(0.18);
            line["topDielectricHeightMm"] = json!(0.25);
            line["bottomMaterial"] = mat.clone();
            line["topMaterial"] = mat.clone();
        }
        if kind.starts_with("coupled") {
            line["gapMm"] = json!(0.2);
        }
        let copper = json!({"conductivitySM":5.8e7,"state":"smooth fixed temperature"});
        line["conductors"] = json!({"signal":copper,"referencePlanes":{"bottom":{"thicknessMm":0.035,"material":copper}}});
        if !kind.contains("microstrip") {
            line["conductors"]["referencePlanes"]["top"] =
                json!({"thicknessMm":0.035,"material":copper});
        }
        line["lengthMm"] = json!(20.);
        for k in ["material", "bottomMaterial", "topMaterial"] {
            if line.get(k).is_some() {
                line[k]["lossTangent"] = json!(0.015);
            }
        }
        let r=extraction::run(serde_json::from_value(json!({"line":line,"frequencies":[1e9],"mesh":{"maxNodes":300000,"maxTriangles":600000},"budget":{"maxMemoryMiB":1024}})).unwrap());
        println!(
            "{}",
            json!({"kind":kind,"status":r["status"],"diagnostics":r["diagnostics"],"convergence":r["convergence"],"sample":r["parameters"]["frequencyDependent"]["samples"][0]})
        );
        assert_eq!(r["status"], "complete", "{}", r["diagnostics"]);
        let mode = &r["parameters"]["frequencyDependent"]["samples"][0]["modes"][0];
        assert!(mode["alphaNpPerM"].as_f64().unwrap() > 0.);
        assert!(r["network"]["maxPassivityExcess"].as_f64().unwrap() <= 1e-10);
    }
}
