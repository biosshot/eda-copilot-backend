use pcb_analyze::extraction::{self, Conductor, Dielectric, Geometry, EPS0, MU0};
use serde_json::{json, Value};
fn request(line: Value) -> Value {
    extraction::run(
        serde_json::from_value(json!({"line":line,"frequencies":[1e6,1e8,1e9]})).unwrap(),
    )
}
fn line(kind: &str, t: f64) -> Value {
    let mat = json!({"relativePermittivity":4.,"lossTangent":0.,"state":"Explicit ideal test"});
    let mut l = json!({"kind":kind,"widthMm":1.,"copperThicknessMm":t,"lengthMm":20.});
    if kind.contains("microstrip") {
        l["dielectricHeightMm"] = json!(1.);
        l["material"] = mat;
    } else {
        l["bottomDielectricHeightMm"] = json!(1.);
        l["topDielectricHeightMm"] = json!(1.);
        l["bottomMaterial"] = mat.clone();
        l["topMaterial"] = mat;
    }
    if kind.starts_with("coupled") {
        l["gapMm"] = json!(1.);
    }
    l
}
#[test]
fn plates_analytic_energy_units() {
    let g = Geometry {
        conductors: vec![Conductor {
            id: "signal".into(),
            role: "signal PEC".into(),
            bounds_m: [0., 0.001, 0.002, 0.0011],
        }],
        dielectrics: vec![Dielectric {
            id: "slab".into(),
            y_min_m: 0.,
            y_max_m: 0.001,
            relative_permittivity: 4.,
            state: "test".into(),
        }],
        bounds_m: [0., 0., 0.002, 0.001],
        reference_y_m: vec![0.],
        length: None,
        kind: "analytic plates".into(),
        artificial_boundary: "natural sides".into(),
    };
    let f = extraction::solve_field(&g, 0.0002).unwrap();
    let c = EPS0 * 4. * 2.;
    assert!((f.capacitance[0][0] / c - 1.).abs() < 1e-10);
    let p = extraction::propagation::parameters(&f.capacitance, &f.vacuum_capacitance).unwrap();
    let l = p["inductanceHPerM"][0][0].as_f64().unwrap();
    assert!((l * f.vacuum_capacitance[0][0] / (MU0 * EPS0) - 1.).abs() < 1e-12);
}
#[test]
fn four_geometries_convergence() {
    for kind in [
        "microstrip",
        "stripline",
        "coupled-microstrip",
        "coupled-stripline",
    ] {
        let r = request(line(kind, 0.035));
        println!(
            "{} {}",
            kind,
            json!({"status":r["status"],"diagnostics":r["diagnostics"],"parameters":r["parameters"],"levels":r["convergence"].as_array().map(|a|a.iter().map(|r|json!({"kind":r["kind"],"nodes":r["nodes"],"change":r["maxRelativeChange"]})).collect::<Vec<_>>()),"elapsedMs":r["elapsedMs"]})
        );
        assert_eq!(r["status"], "complete", "{r}");
    }
}

#[test]
fn thin_and_varied_sections() {
    for (w, h, e, t) in [
        (1., 1., 4., 0.00001),
        (0.5, 1., 2.2, 0.00001),
        (2., 1., 9.5, 0.00001),
        (1., 1., 4., 0.1),
    ] {
        let mut l = line("microstrip", t);
        l["widthMm"] = json!(w);
        l["dielectricHeightMm"] = json!(h);
        l["material"]["relativePermittivity"] = json!(e);
        let r = request(l);
        println!(
            "reference w={w} h={h} e={e} t={t} {}",
            json!({"status":r["status"],"modes":r["parameters"]["modes"],"diagnostics":r["diagnostics"]})
        );
        assert_eq!(r["status"], "complete", "{r}");
    }
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn close(a: f64, b: f64, t: f64) {
    assert!(
        (a - b).abs() <= t * b.abs() + 1e-30,
        "{a} != {b}, relative {}",
        (a / b - 1.).abs()
    );
}
#[test]
fn published_engineering_references() {
    let refs: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-cross-section/references.json"
    ))
    .unwrap();
    for r in refs["single"].as_array().unwrap() {
        let mut l = line("microstrip", number(&r["thicknessMm"]));
        l["widthMm"] = r["widthMm"].clone();
        l["dielectricHeightMm"] = r["heightMm"].clone();
        l["material"]["relativePermittivity"] = r["epsilon"].clone();
        let out = request(l);
        assert_eq!(out["status"], "complete", "{out}");
        let m = &out["parameters"]["modes"][0];
        close(number(&m["impedanceOhm"]), number(&r["impedanceOhm"]), 0.03);
        close(
            (number(&m["delaySPerM"]) * 299792458.).powi(2),
            number(&r["effectivePermittivity"]),
            0.03,
        );
    }
    for r in refs["pair"].as_array().unwrap() {
        let mut l = line("coupled-microstrip", number(&r["thicknessMm"]));
        l["gapMm"] = r["gapMm"].clone();
        let out=extraction::run(serde_json::from_value(json!({"line":l,"frequencies":[1e6],"mesh":{"maxNodes":300000,"maxTriangles":600000},"budget":{"maxMemoryMiB":1024}})).unwrap());
        assert_eq!(out["status"], "complete", "{out}");
        for k in 0..2 {
            close(
                number(&out["parameters"]["modes"][k]["impedanceOhm"]),
                number(&r["impedanceOhm"][k]),
                0.03,
            );
        }
    }
    for r in refs["stripline"]["table"].as_array().unwrap() {
        let mut l = line("stripline", 0.01);
        l["widthMm"] = r["widthMm"].clone();
        for side in ["top", "bottom"] {
            l[format!("{side}DielectricHeightMm")] = json!(0.63);
            l[format!("{side}Material")]["relativePermittivity"] = json!(2.2);
        }
        let out = request(l);
        assert_eq!(out["status"], "complete", "{out}");
        close(
            number(&out["parameters"]["modes"][0]["impedanceOhm"]),
            number(&r["impedanceOhm"]),
            0.03,
        );
    }
}
#[test]
fn asymmetric_multilayer_stripline_and_air() {
    let mut l = line("stripline", 0.00001);
    l["topMaterial"]["relativePermittivity"] = json!(2.);
    l["bottomMaterial"]["relativePermittivity"] = json!(6.);
    let mixed = request(l.clone());
    l["topMaterial"]["relativePermittivity"] = json!(4.);
    l["bottomMaterial"]["relativePermittivity"] = json!(4.);
    let uniform = request(l.clone());
    assert_eq!(mixed["status"], "complete", "{mixed}");
    close(
        number(&mixed["parameters"]["capacitanceFPerM"][0][0]),
        number(&uniform["parameters"]["capacitanceFPerM"][0][0]),
        0.01,
    );
    l["topDielectricHeightMm"] = json!(0.5);
    let asymmetric = request(l);
    assert_eq!(asymmetric["status"], "complete", "{asymmetric}");
    assert!(
        number(&asymmetric["parameters"]["capacitanceFPerM"][0][0])
            > number(&uniform["parameters"]["capacitanceFPerM"][0][0])
    );
    let mut air = line("microstrip", 0.035);
    air["material"]["relativePermittivity"] = json!(1.);
    let r = request(air);
    let p = &r["parameters"];
    close(
        number(&p["capacitanceFPerM"][0][0]),
        number(&p["vacuumCapacitanceFPerM"][0][0]),
        1e-12,
    );
    close(
        number(&p["inductanceHPerM"][0][0]) * number(&p["capacitanceFPerM"][0][0]),
        MU0 * EPS0,
        1e-12,
    );
}
#[test]
fn invalid_inputs_limits_and_recovery() {
    assert!(extraction::propagation::parameters(&[vec![1e-320]], &[vec![1e-11]]).is_err());

    let base = line("microstrip", 0.035);
    let mut bad = vec![];
    for (key, v) in [
        ("widthMm", json!(-1)),
        ("lengthMm", json!(0)),
        ("solderMask", json!({})),
        ("kind", json!("channel")),
    ] {
        let mut b = base.clone();
        b[key] = v;
        bad.push(b);
    }
    for (key, v) in [
        ("lossTangent", json!(0.01)),
        ("conductivitySPerM", json!(1)),
        ("relativePermittivity", Value::Null),
        ("state", json!("")),
    ] {
        let mut b = base.clone();
        b["material"][key] = v;
        bad.push(b);
    }
    for b in bad {
        assert_eq!(request(b)["status"], "rejected");
    }
    for extra in [
        json!({"mesh":{"maxNodes":16}}),
        json!({"mesh":{"maxRefinements":1}}),
        json!({"budget":{"maxMemoryMiB":1}}),
        json!({"budget":{"maxSeconds":1e-9}}),
        json!({"frequencies":[1e15]}),
    ] {
        let mut r = json!({"line":base,"frequencies":[1e6]});
        for (k, v) in extra.as_object().unwrap() {
            r[k] = v.clone();
        }
        assert_eq!(
            extraction::run(serde_json::from_value(r).unwrap())["status"],
            "rejected"
        );
    }
    let mut tiny = base.clone();
    tiny["material"]["relativePermittivity"] = json!(1e-310);
    assert_eq!(request(tiny)["status"], "rejected");
    let good = request(base);
    assert_eq!(good["status"], "complete");
}
// Independent reflection-summation oracle (multiple round trips), not the
// production ABCD denominator; full four-port modal transformation.
#[derive(Clone, Copy)]
struct Q(f64, f64);
impl Q {
    fn add(self, b: Q) -> Q {
        Q(self.0 + b.0, self.1 + b.1)
    }
    fn mul(self, b: Q) -> Q {
        Q(self.0 * b.0 - self.1 * b.1, self.0 * b.1 + self.1 * b.0)
    }
    fn scale(self, a: f64) -> Q {
        Q(self.0 * a, self.1 * a)
    }
    fn norm(self) -> f64 {
        self.0.hypot(self.1)
    }
}
fn oracle(z: f64, delay: f64, length: f64, f: f64, r: f64) -> (Q, Q) {
    let rho = (z - r) / (z + r);
    let theta = -2. * std::f64::consts::PI * f * delay * length;
    let p = Q(theta.cos(), theta.sin());
    let round = p.mul(p).scale(rho * rho);
    let mut sum = Q(1., 0.);
    let mut term = Q(1., 0.);
    for _ in 0..1000 {
        term = term.mul(round);
        sum = sum.add(term);
        if term.norm() < 1e-17 {
            break;
        }
    }
    let trans = p.mul(sum).scale(1. - rho * rho);
    let refl = Q(1., 0.).add(p.mul(p).scale(-1.)).mul(sum).scale(rho);
    (refl, trans)
}
#[test]
fn modal_propagation_independent_complex_oracle() {
    for length in [0.01, 0.125, 1., 10.] {
        for f in [0., 1e6, 0.5e9, 1e9] {
            let p = json!({"modes":[{"impedanceOhm":80.,"delaySPerM":6e-9},{"impedanceOhm":40.,"delaySPerM":5e-9}]});
            let out = extraction::propagation::network(&p, &[f], length, 50.).unwrap();
            let even = oracle(80., 6e-9, length, f, 50.);
            let odd = oracle(40., 5e-9, length, f, 50.);
            for i in 0..4 {
                for j in 0..4 {
                    let (e, o) = if i / 2 == j / 2 {
                        (even.0, odd.0)
                    } else {
                        (even.1, odd.1)
                    };
                    let want = e
                        .add(o.scale(if i % 2 == j % 2 { 1. } else { -1. }))
                        .scale(0.5);
                    let got = &out["parameters"][0][i][j];
                    let err = Q(
                        number(&got["real"]) - want.0,
                        number(&got["imaginary"]) - want.1,
                    )
                    .norm();
                    assert!(err < 1e-10 + 1e-8 * want.norm(), "{i},{j} err {err}");
                }
            }
        }
    }
    for z in [50., 75.] {
        let p = json!({"modes":[{"impedanceOhm":z,"delaySPerM":5e-9}]});
        for f in [0., 0.5e9, 1e9] {
            let o = extraction::propagation::network(&p, &[f], 0.1, z).unwrap();
            let theta = -2. * std::f64::consts::PI * f * 0.1 * 5e-9;
            close(number(&o["parameters"][0][0][0]["real"]), 0., 0.);
            assert!(number(&o["parameters"][0][0][0]["imaginary"]).abs() < 1e-14);
            assert!((number(&o["parameters"][0][1][0]["real"]) - theta.cos()).abs() < 1e-12);
            assert!((number(&o["parameters"][0][1][0]["imaginary"]) - theta.sin()).abs() < 1e-12);
        }
    }
}

#[test]
fn multilayer_analytic_plate_and_conductor_permutation() {
    let mut g = Geometry {
        conductors: vec![Conductor {
            id: "signal".into(),
            role: "signal PEC".into(),
            bounds_m: [0., 0.001, 0.002, 0.0011],
        }],
        dielectrics: vec![
            Dielectric {
                id: "a".into(),
                y_min_m: 0.,
                y_max_m: 0.0003,
                relative_permittivity: 2.,
                state: "test".into(),
            },
            Dielectric {
                id: "b".into(),
                y_min_m: 0.0003,
                y_max_m: 0.001,
                relative_permittivity: 6.,
                state: "test".into(),
            },
        ],
        bounds_m: [0., 0., 0.002, 0.001],
        reference_y_m: vec![0.],
        length: None,
        kind: "analytic layered plates".into(),
        artificial_boundary: "natural sides".into(),
    };
    let f = extraction::solve_field(&g, 0.0001).unwrap();
    close(
        f.capacitance[0][0],
        EPS0 * 0.002 / (0.0003 / 2. + 0.0007 / 6.),
        0.01,
    );
    g.conductors = vec![
        Conductor {
            id: "left".into(),
            role: "signal PEC".into(),
            bounds_m: [-0.0015, 0.001, -0.0005, 0.001035],
        },
        Conductor {
            id: "right".into(),
            role: "signal PEC".into(),
            bounds_m: [0.0005, 0.001, 0.0015, 0.001035],
        },
    ];
    g.dielectrics = vec![Dielectric {
        id: "uniform".into(),
        y_min_m: 0.,
        y_max_m: 0.002035,
        relative_permittivity: 4.,
        state: "test".into(),
    }];
    g.bounds_m = [-0.01, 0., 0.01, 0.002035];
    g.reference_y_m.push(0.002035);
    let original = extraction::solve_field(&g, 0.00005).unwrap();
    g.conductors.swap(0, 1);
    let swapped = extraction::solve_field(&g, 0.00005).unwrap();
    assert!(
        original.nodes == swapped.nodes,
        "mesh depends on conductor order"
    );
    for i in 0..2 {
        for j in 0..2 {
            close(
                original.capacitance[i][j],
                swapped.capacitance[1 - i][1 - j],
                1e-10,
            );
        }
        for k in 0..original.nodes.len() {
            assert!((original.potentials_v[i][k] - swapped.potentials_v[1 - i][k]).abs() < 1e-10);
        }
    }
}
#[test]
fn weak_pair_approaches_two_isolated_striplines() {
    let single = request(line("stripline", 0.035));
    let mut pair = line("coupled-stripline", 0.035);
    pair["gapMm"] = json!(8.);
    let r=extraction::run(serde_json::from_value(json!({"line":pair,"frequencies":[1e6],"budget":{"maxMemoryMiB":1024},"mesh":{"maxNodes":300000,"maxTriangles":600000}})).unwrap());
    assert_eq!(r["status"], "complete", "{r}");
    let z = number(&single["parameters"]["modes"][0]["impedanceOhm"]);
    for m in r["parameters"]["modes"].as_array().unwrap() {
        close(number(&m["impedanceOhm"]), z, 0.01);
    }
    assert!(number(&r["parameters"]["capacitanceFPerM"][0][1]) < 0.);
    assert!(
        number(&r["parameters"]["capacitanceFPerM"][0][1]).abs()
            < 1e-3 * number(&r["parameters"]["capacitanceFPerM"][0][0])
    );
}
