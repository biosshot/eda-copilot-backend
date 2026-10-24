//! Internal product-space coupons. Heavy frozen gates require explicit invocation.
use pcb_analyze::extraction::{local::layered_capacitance::*, EPS0};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/analysis-local-extraction/layered-native")
}
fn budget() -> Budget {
    Budget {
        max_nodes: 50_000,
        max_local_terms: 6_000_000,
        max_bytes: 512 * 1024 * 1024,
        max_wall: Duration::from_secs(60),
    }
}
fn fixture(name: &str) -> Input {
    serde_json::from_str(&std::fs::read_to_string(root().join(format!("{name}.json"))).unwrap())
        .unwrap()
}
fn small() -> Input {
    let xy = vec![[0., 0.], [0.001, 0.], [0.001, 0.001], [0., 0.001]];
    let meshes = vec![
        PlaneInput::Exact {
            source_id: "lower-original".into(),
            xy_m: xy.clone(),
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        },
        PlaneInput::Exact {
            source_id: "upper-original".into(),
            xy_m: xy.clone(),
            triangles: vec![[0, 1, 3], [1, 2, 3]],
        },
    ];
    let z = vec![
        0., 0.000035, 0.0002, 0.0005, 0.000535, 0.0007, 0.001, 0.001035,
    ];
    Input {
        meshes,
        planes: vec![0, 0, 1, 1, 1, 0, 0, 0],
        z_m: z,
        xy_bounds_m: [[0., 0.], [0.001, 0.001]],
        metals: vec![(0., 0.000035), (0.0005, 0.000535), (0.001, 0.001035)]
            .into_iter()
            .enumerate()
            .map(|(i, (a, b))| Metal {
                source_id: format!("physical-foil-{i}"),
                polygon_m: xy.clone(),
                z0_m: a,
                z1_m: b,
            })
            .collect(),
        dielectrics: vec![
            (0., 0.0002, 2.),
            (0.0002, 0.0007, 5.),
            (0.0007, 0.001035, 3.),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (a, b, e))| Dielectric {
            source_id: format!("dielectric-{i}"),
            z0_m: a,
            z1_m: b,
            epsilon_r: e,
        })
        .collect(),
        exterior: Exterior::InsulatedAnalyticCoupon,
    }
}
fn norm(a: &[Vec<f64>], b: &[Vec<f64>]) -> f64 {
    let a: Vec<Vec<f64>> = (0..a.len())
        .map(|i| (0..a.len()).map(|j| (a[i][j] + a[j][i]) / 2.).collect())
        .collect();
    let b: Vec<Vec<f64>> = (0..b.len())
        .map(|i| (0..b.len()).map(|j| (b[i][j] + b[j][i]) / 2.).collect())
        .collect();
    let n = b.len();
    let mut l = vec![vec![0.; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let x = b[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            l[i][j] = if i == j { x.sqrt() } else { x / l[j][j] };
        }
    }
    let mut w = vec![vec![0.; n]; n];
    for c in 0..n {
        for i in 0..n {
            w[i][c] = ((if i == c { 1. } else { 0. })
                - (0..i).map(|k| l[i][k] * w[k][c]).sum::<f64>())
                / l[i][i];
        }
    }
    let mut d = vec![vec![0.; n]; n];
    for i in 0..n {
        for j in 0..n {
            d[i][j] = (0..n)
                .flat_map(|k| (0..n).map(move |q| (k, q)))
                .map(|(k, q)| w[i][k] * (a[k][q] - b[k][q]) * w[j][q])
                .sum();
        }
    }
    for _ in 0..64 {
        let mut pair = (0, 0);
        let mut off: f64 = 0.;
        for i in 0..n {
            for j in i + 1..n {
                if d[i][j].abs() > off {
                    off = d[i][j].abs();
                    pair = (i, j);
                }
            }
        }
        if off < 1e-18 {
            break;
        }
        let (p, q) = pair;
        let theta = 0.5 * (2. * d[p][q]).atan2(d[q][q] - d[p][p]);
        let (c, s) = (theta.cos(), theta.sin());
        let mut rot = vec![vec![0.; n]; n];
        for i in 0..n {
            rot[i][i] = 1.;
        }
        rot[p][p] = c;
        rot[q][q] = c;
        rot[p][q] = s;
        rot[q][p] = -s;
        let old = d.clone();
        for i in 0..n {
            for j in 0..n {
                d[i][j] = (0..n)
                    .flat_map(|k| (0..n).map(move |t| (k, t)))
                    .map(|(k, t)| rot[k][i] * old[k][t] * rot[t][j])
                    .sum();
            }
        }
    }
    (0..n).map(|i| d[i][i].abs()).fold(0., f64::max)
}
#[test]
fn analytic_asymmetric_dielectric_three_conductor_full_matrix() {
    let input = small();
    let s = extract(&input, budget()).unwrap();
    let ca = EPS0 * 1e-6 / (0.000165 / 2. + 0.0003 / 5.);
    let cb = EPS0 * 1e-6 / (0.000165 / 5. + 0.0003 / 3.);
    let expected = [[ca, -ca, 0.], [-ca, ca + cb, -cb], [0., -cb, cb]];
    let scale = (ca + cb).abs();
    for i in 0..3 {
        for j in 0..3 {
            assert!(
                (s.capacitance_f[i][j] - expected[i][j]).abs() < scale * 1e-10,
                "{:?}",
                s.capacitance_f
            );
        }
    }
    let r = s.recover(&[0.3, -0.4, 0.8]).unwrap();
    let energy = (ca * 0.7f64.powi(2) + cb * 1.2f64.powi(2)) / 2.;
    assert!((r.energy_j / energy - 1.).abs() < 1e-10);
    assert!(r.recovered_energy_relative_error < 1e-10);
    let common = s.recover(&[1., 1., 1.]).unwrap();
    assert_eq!(common.energy_j, 0.);
    assert!(common
        .fields
        .iter()
        .all(|f| f.electric_field_v_per_m == [0.; 3]));
    assert_eq!(
        s.source_ids,
        vec!["physical-foil-0", "physical-foil-1", "physical-foil-2"]
    );
}
#[test]
fn native_cdt_keeps_source_constraints_and_analytic_energy() {
    let mut input = small();
    for m in &mut input.meshes {
        if let PlaneInput::Exact {
            source_id, xy_m, ..
        } = m.clone()
        {
            *m = PlaneInput::Cdt {
                source_id,
                xy_m,
                constraints: vec![[0, 1], [1, 2], [2, 3], [3, 0], [0, 2]],
            };
        }
    }
    let s = extract(&input, budget()).unwrap();
    let r = s.recover(&[1., 0., 0.]).unwrap();
    let expected = EPS0 * 1e-6 / (0.000165 / 2. + 0.0003 / 5.) / 2.;
    assert!((r.energy_j / expected - 1.).abs() < 1e-10);
}
#[test]
fn allocation_deadline_and_geometry_negatives() {
    let input = small();
    let mut b = budget();
    b.max_bytes = 1;
    assert!(extract(&input, b).err().unwrap().contains("MEMORY"));
    b = budget();
    b.max_wall = Duration::ZERO;
    assert!(extract(&input, b).err().unwrap().contains("DEADLINE"));
    b = budget();
    b.max_local_terms = 1;
    assert!(extract(&input, b).err().unwrap().contains("MESH_LIMIT"));
    let mut cut = input.clone();
    cut.metals[1].polygon_m[0][0] = 0.0001;
    cut.metals[1].polygon_m[3][0] = 0.0001;
    assert!(extract(&cut, budget())
        .err()
        .unwrap()
        .contains("COPPER_TRACE"));
    let mut bad = input.clone();
    bad.metals[1].z0_m += 1e-9;
    assert!(extract(&bad, budget())
        .err()
        .unwrap()
        .contains("COPPER_INPUT"));
    let mut bad = input.clone();
    bad.dielectrics[1].epsilon_r = -1.;
    assert!(extract(&bad, budget()).err().unwrap().contains("MATERIAL"));
    let mut bad = input.clone();
    bad.metals[1] = bad.metals[0].clone();
    bad.metals[1].source_id = "distinct-overlapping-source".into();
    assert!(extract(&bad, budget())
        .err()
        .unwrap()
        .contains("COPPER_OVERLAP"));
}
fn heavy(name: &str, cdt: bool) {
    let mut input = fixture(name);
    if cdt {
        for m in &mut input.meshes {
            if let PlaneInput::Exact {
                source_id,
                xy_m,
                triangles,
            } = m.clone()
            {
                let mut edges = std::collections::BTreeSet::new();
                for t in triangles {
                    let center = [
                        t.iter().map(|i| xy_m[*i][0]).sum::<f64>() / 3.,
                        t.iter().map(|i| xy_m[*i][1]).sum::<f64>() / 3.,
                    ];
                    if center[0].abs().max(center[1].abs()) <= 0.0005 {
                        continue;
                    }
                    for k in 0..3 {
                        let mut e = [t[k], t[(k + 1) % 3]];
                        e.sort();
                        edges.insert(e);
                    }
                }
                *m = PlaneInput::Cdt {
                    source_id,
                    xy_m,
                    constraints: edges.into_iter().collect(),
                };
            }
        }
    }
    let solution = extract(&input, budget()).unwrap();
    let exp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("expected.json")).unwrap())
            .unwrap();
    let raw: Vec<Vec<f64>> =
        serde_json::from_value(exp["frozenVacuumRawCapacitanceF"].clone()).unwrap();
    let external = norm(&solution.capacitance_f, &raw);
    let mut parity = None;
    if !cdt && exp["cases"].get(name).is_some() {
        let python: Vec<Vec<f64>> =
            serde_json::from_value(exp["cases"][name]["pythonCapacitanceF"].clone()).unwrap();
        let p = norm(&solution.capacitance_f, &python);
        parity = Some(p);
        assert!(p <= 1e-10, "same-mesh parity {p:e}");
    }
    let common = solution.recover(&vec![1.; input.metals.len()]).unwrap();
    let diff = solution
        .recover(
            &(0..input.metals.len())
                .map(|i| if i % 2 == 0 { 1. } else { -1. })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    println!(
        "LAYERED_NATIVE_RESULT {}",
        json!({"case":name,"cdt":cdt,"capacitanceF":solution.capacitance_f,"parityEnergyError":parity,"externalEnergyError":external,"plan":solution.plan,"commonEnergyJ":common.energy_j,"differentialEnergyJ":diff.energy_j,"commonRecoveryError":common.recovered_energy_relative_error,"differentialRecoveryError":diff.recovered_energy_relative_error})
    );
    if name == "failed-d32" {
        assert!(external > 0.05);
    } else {
        assert!(external <= 0.05, "external accuracy {external:e}");
    }
}
#[test]
#[ignore = "explicit 512MiB/60s frozen request"]
fn frozen_failed_d32() {
    heavy("failed-d32", false)
}
#[test]
#[ignore = "explicit 512MiB/60s frozen request"]
fn frozen_accepted_d64_domain() {
    heavy("accepted-d64-domain", false)
}
#[test]
#[ignore = "explicit 512MiB/60s frozen request"]
fn frozen_accepted_d64_mesh() {
    heavy("accepted-d64-mesh", false)
}
#[test]
#[ignore = "explicit 512MiB/60s frozen request"]
fn native_cdt_independent_accuracy() {
    heavy("accepted-d64-mesh", true)
}

#[test]
fn fully_prescribed_product_cell_retains_both_conductors() {
    let mut input = small();
    input.metals.remove(1);
    input.z_m = vec![0., 0.000035, 0.001, 0.001035];
    input.planes = vec![0, 0, 1, 1];
    input.dielectrics = vec![Dielectric {
        source_id: "vacuum".into(),
        z0_m: 0.,
        z1_m: 0.001035,
        epsilon_r: 1.,
    }];
    let s = extract(&input, budget()).unwrap();
    assert_eq!(s.plan.free_nodes, 0);
    let r = s.recover(&[1., 0.]).unwrap();
    let reference = EPS0 * 1e-6 / 0.000965;
    assert!((r.charges_c[0] / reference - 1.).abs() < 1e-10);
    assert!((r.charges_c[1] / reference + 1.).abs() < 1e-10);
}
