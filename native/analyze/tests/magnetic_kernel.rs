//! Independent frozen magnetic-integral and full-operator regressions.
use pcb_analyze::extraction::local::{
    polyhedron::{self, PointPotential},
    prism::{GaussRule, Prism},
};
use serde_json::Value;
fn fixture(name: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/analysis-local-extraction/magnetic-stability")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
fn prism(v: &Value) -> Prism {
    Prism::new(
        "authored".into(),
        serde_json::from_value(v["triangleM"].clone()).unwrap(),
        serde_json::from_value(v["zM"].clone()).unwrap(),
    )
    .unwrap()
}
fn integrate(a: &Prism, b: &Prism) -> f64 {
    polyhedron::volume_integral_anisotropic(
        a,
        &PointPotential::new(b).unwrap(),
        &GaussRule::new(12).unwrap(),
        &GaussRule::new(8).unwrap(),
    )
    .unwrap()
}
#[test]
fn stm_both_directions_and_self_meet_independent_integral_gate() {
    let input = fixture("coupon.json");
    let reference = fixture("reference.json");
    let narrow = fixture("slender-self-reference.json");
    let a = prism(&input["stmPair"][0]);
    let b = prism(&input["stmPair"][1]);
    for (label, target, source) in [
        ("stm668-556", &a, &b),
        ("stm668-556", &b, &a),
        ("stm668-self", &a, &a),
        ("stm556-self", &b, &b),
    ] {
        let (expected, uncertainty) = if label == "stm668-self" {
            (
                narrow["referenceM5"].as_f64().unwrap(),
                narrow["absoluteTotalUncertaintyM5"].as_f64().unwrap(),
            )
        } else {
            let r = reference["stm"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["label"] == label)
                .unwrap();
            (
                r["referenceM5"].as_f64().unwrap(),
                r["empiricalAbsoluteUncertaintyM5"].as_f64().unwrap(),
            )
        };
        let value = integrate(target, source);
        assert!(
            ((value - expected).abs() + uncertainty) / (expected - uncertainty) <= 0.002,
            "{label} {value:e} vs {expected:e}"
        );
    }
}
#[test]
fn conditioned_path_preserves_translation_rotation_scale_and_refusals() {
    let input = fixture("coupon.json");
    let a = prism(&input["stmPair"][0]);
    let b = prism(&input["stmPair"][1]);
    let base = integrate(&a, &b);
    let transform = |p: &Prism, s: f64, shift: [f64; 3], rotate: bool| {
        Prism::new(
            p.source_id.clone(),
            p.triangle_m.map(|v| {
                if rotate {
                    [-s * v[1] + shift[0], s * v[0] + shift[1]]
                } else {
                    [s * v[0] + shift[0], s * v[1] + shift[1]]
                }
            }),
            p.z_m.map(|v| s * v + shift[2]),
        )
        .unwrap()
    };
    for (s, shift, rotate) in [
        (2., [0.; 3], false),
        (1., [0.001, -0.002, 0.0003], false),
        (1., [0.; 3], true),
    ] {
        let value = integrate(
            &transform(&a, s, shift, rotate),
            &transform(&b, s, shift, rotate),
        );
        assert!((value / (base * s.powi(5)) - 1.).abs() < 1e-9);
    }
    let point = [
        a.triangle_m.iter().map(|v| v[0]).sum::<f64>() / 3.,
        a.triangle_m.iter().map(|v| v[1]).sum::<f64>() / 3.,
        (a.z_m[0] + a.z_m[1]) / 2.,
    ];
    let source = PointPotential::new(&b).unwrap();
    let diagnostic = source.at_diagnostic(point).unwrap();
    assert!(
        diagnostic.source_order > 0
            && diagnostic.source_relative_change <= 1e-5
            && diagnostic.value_m2 > 0.
    );
    assert!(source
        .at([f64::NAN, 0., 0.])
        .unwrap_err()
        .contains("POINT_INPUT"));
    assert!(GaussRule::new(65).is_err());
    assert!(Prism::new("bad".into(), [[0., 0.], [1., 0.], [0., 1.]], [0., 0.]).is_err());
    assert!(Prism::new("bad".into(), [[0., 0.], [1., 0.], [2., 0.]], [0., 1.]).is_err());
}
#[test]
fn full_four_bar_operator_has_uniform_energy_error_bound() {
    let input = fixture("coupon.json");
    let reference = fixture("reference.json");
    let raw: &Vec<Value> = reference["operatorLevels"].as_array().unwrap();
    let expected: Vec<Vec<f64>> =
        serde_json::from_value(raw.last().unwrap()["rawLH"].clone()).unwrap();
    let mut bars = vec![];
    let mut areas = vec![];
    for b in input["bars"].as_array().unwrap() {
        let x = b["x0M"].as_f64().unwrap();
        let y = b["y0M"].as_f64().unwrap();
        let l = b["lengthM"].as_f64().unwrap();
        let w = b["widthM"].as_f64().unwrap();
        let z: [f64; 2] = serde_json::from_value(b["zM"].clone()).unwrap();
        bars.push([
            Prism::new("a".into(), [[x, y], [x + l, y], [x + l, y + w]], z).unwrap(),
            Prism::new("b".into(), [[x, y], [x + l, y + w], [x, y + w]], z).unwrap(),
        ]);
        areas.push(w * (z[1] - z[0]));
    }
    let mut actual = vec![vec![0.; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for a in &bars[i] {
                for b in &bars[j] {
                    actual[i][j] += integrate(a, b) * 1e-7 / (areas[i] * areas[j]);
                }
            }
        }
    }
    let lower = (0..4)
        .map(|i| {
            expected[i][i]
                - (0..4)
                    .filter(|&j| j != i)
                    .map(|j| expected[i][j].abs())
                    .sum::<f64>()
        })
        .fold(f64::INFINITY, f64::min);
    assert!(lower > 0.);
    let delta = (0..4)
        .map(|i| {
            (0..4)
                .map(|j| (actual[i][j] - expected[i][j]).abs())
                .sum::<f64>()
        })
        .fold(0., f64::max);
    // Gershgorin lower bound and symmetric infinity norm bound every voltage/
    // current direction, hence also the entire sum(I)=0 physical subspace.
    assert!(delta / lower < 0.05);
    for i in 0..4 {
        for j in 0..4 {
            assert!(
                (actual[i][j] - actual[j][i]).abs() < 1e-9 * (actual[i][i] * actual[j][j]).sqrt()
            );
            assert!(
                (actual[i][j] - expected[i][j]).abs()
                    < 0.002 * (expected[i][i] * expected[j][j]).sqrt()
            );
        }
    }
}

#[test]
fn entire_closed_streamfunction_space_is_solenoidal_and_accurate() {
    let input = fixture("closed-coupon.json");
    let reference = fixture("closed-reference.json");
    let audit = fixture("closed-verification.json");
    let expected: Vec<Vec<f64>> = serde_json::from_value(
        reference["levels"].as_array().unwrap().last().unwrap()["meanLH"].clone(),
    )
    .unwrap();
    let mut cells = vec![];
    let mut densities = vec![];
    let mut flux = std::collections::BTreeMap::<[usize; 2], [f64; 2]>::new();
    for c in input["cells"].as_array().unwrap() {
        let p = prism(c);
        let d: [[f64; 2]; 2] = serde_json::from_value(c["densityPerAmpM2"].clone()).unwrap();
        let ids: [usize; 3] = serde_json::from_value(c["nodeIds"].clone()).unwrap();
        for k in 0..3 {
            let a = p.triangle_m[k];
            let b = p.triangle_m[(k + 1) % 3];
            let normal = [b[1] - a[1], a[0] - b[0]];
            let mut key = [ids[k], ids[(k + 1) % 3]];
            key.sort();
            let entry = flux.entry(key).or_insert([0.; 2]);
            for q in 0..2 {
                entry[q] += (d[q][0] * normal[0] + d[q][1] * normal[1]) * (p.z_m[1] - p.z_m[0]);
            }
        }
        cells.push(p);
        densities.push(d);
    }
    assert!(flux.values().flatten().all(|v| v.abs() < 1e-12));
    let sources: Vec<_> = cells
        .iter()
        .map(|p| PointPotential::new(p).unwrap())
        .collect();
    let rule = GaussRule::new(24).unwrap();
    let mut actual = [[0.; 2]; 2];
    for i in 0..cells.len() {
        for j in 0..cells.len() {
            let value = polyhedron::volume_integral(&cells[i], &sources[j], &rule).unwrap();
            for a in 0..2 {
                for b in 0..2 {
                    actual[a][b] += 1e-7
                        * value
                        * (densities[i][a][0] * densities[j][b][0]
                            + densities[i][a][1] * densities[j][b][1]);
                }
            }
        }
    }
    let lower = (expected[0][0] - expected[0][1].abs()).min(expected[1][1] - expected[1][0].abs());
    assert!(lower > 0.);
    let delta = (0..2)
        .map(|i| {
            (0..2)
                .map(|j| (actual[i][j] - expected[i][j]).abs())
                .sum::<f64>()
        })
        .fold(0., f64::max);
    assert!(delta / lower + audit["referenceWorkingUncertainty"].as_f64().unwrap() < 0.05);
    assert!((actual[0][1] - actual[1][0]).abs() / actual[0][0].max(actual[1][1]) < 1e-9);
}
