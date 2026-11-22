use pcb_analyze::extraction::{
    polygon_section::{self, Contour, MaterialRegion, Metal, Request},
    EPS0,
};
fn save_evidence(name:&str,value:&serde_json::Value){if let Some(directory)=std::env::var_os("P7_TEST_EVIDENCE_DIR"){std::fs::create_dir_all(&directory).unwrap();std::fs::write(std::path::Path::new(&directory).join(name),serde_json::to_vec_pretty(value).unwrap()).unwrap();}}
fn rectangle(a: f64, b: f64, c: f64, d: f64) -> Contour {
    Contour {
        outer_m: vec![[a, b], [c, b], [c, d], [a, d]],
        holes_m: vec![],
    }
}
fn circle(radius: f64) -> Vec<[f64; 2]> {
    (0..128)
        .map(|i| {
            let t = i as f64 * std::f64::consts::TAU / 128.;
            [radius * t.cos(), radius * t.sin()]
        })
        .collect()
}
#[test]
fn polygon_coax_matches_independent_analytic_capacitance_and_delay() {
    let mut reference = rectangle(-0.001, -0.001, 0.001, 0.001);
    reference.holes_m.push(circle(0.0008));
    let domain = rectangle(-0.0012, -0.0012, 0.0012, 0.0012);
    let mut r = Request {
        loss: None,
        domain: domain.clone(),
        conductors: vec![
            Metal {
                id: "inner".into(),
                role: "signal".into(),
                contour: Contour {
                    outer_m: circle(0.0002),
                    holes_m: vec![],
                },
            },
            Metal {
                id: "shield".into(),
                role: "reference".into(),
                contour: reference,
            },
        ],
        dielectrics: vec![MaterialRegion {
            id: "homogeneous".into(),
            relative_permittivity: 4.,
            contour: domain,
        }],
        max_triangle_area_m2: 1e-9,
        max_nodes: 30000,
        max_seconds: 120.,
        max_memory_mib: 256,
    };
    let result = polygon_section::solve(&r).unwrap();
    let c = result["capacitanceFPerM"][0][0].as_f64().unwrap();
    let reference = std::f64::consts::TAU * EPS0 * 4. / 4_f64.ln();
    assert!(
        (c / reference - 1.).abs() < 0.02,
        "C={c:e}, exact={reference:e}"
    );
    let delay = result["parameters"]["modes"][0]["delaySPerM"]
        .as_f64()
        .unwrap();
    assert!((delay / (2. / 299792458.) - 1.).abs() < 1e-8);
    let transport=pcb_analyze::extraction::run(serde_json::from_value(serde_json::json!({"line":{"kind":"polygon-section","geometry":r,"lengthMm":10.},"frequencies":[0.,1e6],"mesh":{"maxRefinements":2}})).unwrap());
    assert_eq!(transport["status"], "complete", "{transport}");
    assert!(
        transport["network"]["quality"]["unitarityResidual"]
            .as_f64()
            .unwrap()
            < 1e-8
    );
    r.loss = Some(polygon_section::LossInput {
        frequencies_hz: vec![1e9],
        conductivity_s_m: vec![58e6, 58e6],
        minimum_skin_dimensions_m: vec![0.0004, 0.0002],
        dielectric_loss_tangents: vec![0.01],
        band_hz: [1e8, 1e10],
    });
    let lossy = polygon_section::solve(&r).unwrap();
    let sample = &lossy["frequencyDependent"]["samples"][0];
    let w = std::f64::consts::TAU * 1e9;
    let rs = (w * pcb_analyze::extraction::MU0 / (2. * 58e6)).sqrt();
    let expected = rs / std::f64::consts::TAU * (1. / 0.0002 + 1. / 0.0008);
    let resistance = sample["resistanceOhmPerM"][0][0].as_f64().unwrap();
    assert!(
        (resistance / expected - 1.).abs() < 0.02,
        "surface R={resistance:e}, analytic={expected:e}"
    );
    let g = sample["conductanceSPerM"][0][0].as_f64().unwrap();
    let c = lossy["capacitanceFPerM"][0][0].as_f64().unwrap();
    assert!((g / (w * 0.01 * c) - 1.).abs() < 1e-8);
    let waves = polygon_section::network(sample, &[1e9], 0.02, 50.).unwrap();
    assert!(waves["quality"]["maxPassivityExcess"].as_f64().unwrap() < 1e-8);
    let exact_c = std::f64::consts::TAU * EPS0 * 4. / 4_f64.ln();
    let exact_l = pcb_analyze::extraction::MU0 * 4_f64.ln() / std::f64::consts::TAU;
    let x = w * exact_l + expected;
    let g_exact = w * exact_c * 0.01;
    let q_real = expected * g_exact - x * w * exact_c;
    let q_imag = expected * w * exact_c + x * g_exact;
    let alpha = ((q_real.hypot(q_imag) + q_real) / 2.).sqrt();
    let beta = ((q_real.hypot(q_imag) - q_real) / 2.).sqrt();
    let gamma = &sample["modes"][0]["propagationConstantPerM"];
    assert!((gamma["real"].as_f64().unwrap() / alpha - 1.).abs() < 0.02);
    assert!((gamma["imaginary"].as_f64().unwrap() / beta - 1.).abs() < 0.02);
    // Electrically small homogeneous control at10GHz, not a PCB-wide claim.
    for contour in std::iter::once(&mut r.domain)
        .chain(r.conductors.iter_mut().map(|m| &mut m.contour))
        .chain(r.dielectrics.iter_mut().map(|m| &mut m.contour))
    {
        for point in contour
            .outer_m
            .iter_mut()
            .chain(contour.holes_m.iter_mut().flatten())
        {
            point[0] *= 0.1;
            point[1] *= 0.1;
        }
    }
    r.max_triangle_area_m2 *= 0.01;
    let loss = r.loss.as_mut().unwrap();
    loss.frequencies_hz = vec![1e10];
    for thickness in &mut loss.minimum_skin_dimensions_m {
        *thickness *= 0.1;
    }
    let hf=pcb_analyze::extraction::run(serde_json::from_value(serde_json::json!({"line":{"kind":"polygon-section","geometry":r,"lengthMm":1.},"frequencies":[1e10],"mesh":{"maxRefinements":2}})).unwrap());
    assert_eq!(hf["status"], "complete", "{hf}");
    assert!(
        hf["network"]["quality"]["maxPassivityExcess"]
            .as_f64()
            .unwrap()
            < 1e-8
    );
    save_evidence("polygon-coax-reference.json",&serde_json::json!({"capacitance":{"actual":c,"analytic":reference,"relativeError":c/reference-1.},"delay":{"actual":delay,"analytic":2./299792458.,"relativeError":delay/(2./299792458.)-1.},"surfaceResistanceAt1GHz":{"actual":resistance,"analytic":expected,"relativeError":resistance/expected-1.},"gammaAt1GHz":{"actual":gamma,"analytic":{"real":alpha,"imaginary":beta},"alphaRelativeError":gamma["real"].as_f64().unwrap()/alpha-1.,"betaRelativeError":gamma["imaginary"].as_f64().unwrap()/beta-1.},"electricallySmall10GHzControl":hf,"scope":"independent coax controls; polygon and skin/asymptotic assumptions, no whole-board accuracy certificate"}));
}
#[test]
fn unequal_three_conductor_modes_preserve_full_matrices() {
    let c0 = vec![
        vec![3e-11, -1e-11, -0.2e-11],
        vec![-1e-11, 4e-11, -0.3e-11],
        vec![-0.2e-11, -0.3e-11, 2e-11],
    ];
    let c = c0
        .iter()
        .map(|r| r.iter().map(|v| v * 3.).collect())
        .collect::<Vec<Vec<f64>>>();
    let p = polygon_section::parameters(&c, &c0).unwrap();
    assert_eq!(p["modes"].as_array().unwrap().len(), 3);
    for mode in p["modes"].as_array().unwrap() {
        let d = mode["delaySPerM"].as_f64().unwrap();
        assert!((d / (3_f64.sqrt() / 299792458.) - 1.).abs() < 1e-8);
    }
    let l = p["inductanceHPerM"].as_array().unwrap();
    for i in 0..3 {
        for j in 0..3 {
            let product = (0..3)
                .map(|k| c0[i][k] * l[k][j].as_f64().unwrap())
                .sum::<f64>();
            let expected = if i == j {
                pcb_analyze::extraction::MU0 * EPS0
            } else {
                0.
            };
            assert!((product - expected).abs() < 1e-28);
        }
    }
    let network = polygon_section::network(&p, &[0., 1e6, 1e9, 1e10], 0.02, 50.).unwrap();
    assert_eq!(network["ports"].as_array().unwrap().len(), 6);
    assert!(network["quality"]["unitarityResidual"].as_f64().unwrap() < 1e-10);
    for i in 0..3 {
        assert!((network["parameters"][0][i + 3][i]["real"].as_f64().unwrap() - 1.).abs() < 1e-10);
    }
}

#[test]
fn radial_multilayer_coax_matches_independent_series_capacitance() {
    let mut reference = rectangle(-0.001, -0.001, 0.001, 0.001);
    reference.holes_m.push(circle(0.0008));
    let domain = rectangle(-0.0012, -0.0012, 0.0012, 0.0012);
    let mut outer = domain.clone();
    outer.holes_m.push(circle(0.0004));
    let r = Request {
        loss: None,
        domain: domain.clone(),
        conductors: vec![
            Metal {
                id: "inner".into(),
                role: "signal".into(),
                contour: Contour {
                    outer_m: circle(0.0002),
                    holes_m: vec![],
                },
            },
            Metal {
                id: "shield".into(),
                role: "reference".into(),
                contour: reference,
            },
        ],
        dielectrics: vec![
            MaterialRegion {
                id: "inner-material".into(),
                relative_permittivity: 2.,
                contour: Contour {
                    outer_m: circle(0.0004),
                    holes_m: vec![],
                },
            },
            MaterialRegion {
                id: "outer-material".into(),
                relative_permittivity: 4.,
                contour: outer,
            },
        ],
        max_triangle_area_m2: 1e-9,
        max_nodes: 15000,
        max_seconds: 120.,
        max_memory_mib: 256,
    };
    let v = polygon_section::solve(&r).unwrap();
    let expected = std::f64::consts::TAU * EPS0 / (2_f64.ln() / 2. + 2_f64.ln() / 4.);
    let actual = v["capacitanceFPerM"][0][0].as_f64().unwrap();
    assert!(
        (actual / expected - 1.).abs() < 0.02,
        "C={actual:e}, analytic={expected:e}"
    );
    save_evidence("polygon-radial-multilayer-reference.json",&serde_json::json!({"actualFPerM":actual,"analyticFPerM":expected,"relativeError":actual/expected-1.,"quality":v["quality"],"scope":"independent series-dielectric coax capacitance control"}));
}
#[test]
fn finite_coplanar_and_cross_layer_unequal_three_signal_section() {
    let domain = rectangle(-0.002, -0.002, 0.002, 0.002);
    let metal = |id: &str, role: &str, a, b, c, d| Metal {
        id: id.into(),
        role: role.into(),
        contour: rectangle(a, b, c, d),
    };
    let mut r = Request {
        loss: None,
        domain: domain.clone(),
        conductors: vec![
            metal(
                "finite-return",
                "reference",
                -0.0015,
                -0.001,
                0.001,
                -0.0009,
            ),
            metal("a", "signal", -0.0008, 0.0001, -0.0005, 0.00015),
            metal("b", "signal", 0.0001, 0.0001, 0.0005, 0.00015),
            metal("c", "signal", 0.0005, -0.0003, 0.0007, -0.00025),
        ],
        dielectrics: vec![
            MaterialRegion {
                id: "bottom".into(),
                relative_permittivity: 4.,
                contour: rectangle(-0.002, -0.002, 0.002, 0.),
            },
            MaterialRegion {
                id: "top".into(),
                relative_permittivity: 2.,
                contour: rectangle(-0.002, 0., 0.002, 0.002),
            },
        ],
        max_triangle_area_m2: 2e-9,
        max_nodes: 15000,
        max_seconds: 120.,
        max_memory_mib: 256,
    };
    let v = polygon_section::solve(&r).unwrap();
    assert_eq!(v["parameters"]["modes"].as_array().unwrap().len(), 3);
    let c = &v["capacitanceFPerM"];
    assert_ne!(c[0][0], c[1][1]);
    for i in 0..3 {
        assert!(c[i][i].as_f64().unwrap() > 0.);
        for j in 0..3 {
            if i != j {
                assert!(c[i][j].as_f64().unwrap() < 0.);
            }
        }
    }
    let network = polygon_section::network(&v["parameters"], &[0., 1e6, 1e8], 0.01, 50.).unwrap();
    assert!(network["quality"]["reciprocityResidual"].as_f64().unwrap() < 1e-8);
    r.loss = Some(polygon_section::LossInput {
        frequencies_hz: vec![1e9],
        conductivity_s_m: vec![58e6; 4],
        minimum_skin_dimensions_m: vec![0.00005, 0.00005, 0.00005, 0.0001],
        dielectric_loss_tangents: vec![0.01, 0.015],
        band_hz: [1e9, 1e10],
    });
    let lossy = polygon_section::solve(&r).unwrap();
    let network = polygon_section::network(
        &lossy["frequencyDependent"]["samples"][0],
        &[1e9],
        0.01,
        50.,
    )
    .unwrap();
    assert!(network["quality"]["maxPassivityExcess"].as_f64().unwrap() < 1e-8);
    assert!(network["quality"]["reciprocityResidual"].as_f64().unwrap() < 1e-8);
}
