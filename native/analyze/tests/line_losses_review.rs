//! R1/R2 independent physical references plus recovery/publication diagnostics.
//! Alternative recovery is diagnostic only; no fitted coefficients enter production.
use pcb_analyze::extraction::{self, losses, Conductor, Dielectric, Field, Geometry};
use serde_json::json;
fn consistent(f: &Field, s: &losses::Surface) -> (f64, f64, f64) {
    let ids = &s.node_indices;
    let lookup: std::collections::BTreeMap<_, _> =
        ids.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut diag = vec![0.; ids.len()];
    let mut edges = vec![];
    for [a, b] in &s.edges {
        let l = (f.nodes[*a][0] - f.nodes[*b][0]).hypot(f.nodes[*a][1] - f.nodes[*b][1]);
        let i = lookup[a];
        let j = lookup[b];
        diag[i] += l / 3.;
        diag[j] += l / 3.;
        edges.push((i, j, l / 6.));
    }
    let q: Vec<_> = ids
        .iter()
        .map(|n| f.vacuum_reaction_c_per_m[0][*n] / f.vacuum_capacitance[0][0])
        .collect();
    let apply = |x: &[f64]| {
        let mut y: Vec<_> = x.iter().zip(&diag).map(|(x, d)| x * d).collect();
        for &(i, j, v) in &edges {
            y[i] += v * x[j];
            y[j] += v * x[i];
        }
        y
    };
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let mut x = vec![0.; ids.len()];
    let mut r = q.clone();
    let mut z: Vec<_> = r.iter().zip(&diag).map(|(r, d)| r / d).collect();
    let mut p = z.clone();
    let mut rz = dot(&r, &z);
    for _ in 0..100 {
        let ap = apply(&p);
        let a = rz / dot(&p, &ap);
        for i in 0..x.len() {
            x[i] += a * p[i];
            r[i] -= a * ap[i];
        }
        if dot(&r, &r).sqrt() < 1e-13 * dot(&q, &q).sqrt() {
            break;
        }
        z = r.iter().zip(&diag).map(|(r, d)| r / d).collect();
        let next = dot(&r, &z);
        let beta = next / rz;
        for i in 0..p.len() {
            p[i] = z[i] + beta * p[i];
        }
        rz = next;
    }
    let mx = apply(&x);
    let gram = dot(&x, &mx);
    let integrated: f64 = mx.iter().sum();
    let residual = mx
        .iter()
        .zip(&q)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
        / dot(&q, &q).sqrt();
    (gram, integrated, residual)
}
#[test]
fn r1_fixed_geometry_boundary_recovery() {
    for (w, h, t, freq, base) in [
        (0.0003, 0.00018, 0.000005, 20e9, 0.00000375),
        (0.001, 0.0003, 0.00001, 5e9, 0.00000625),
    ] {
        let g = Geometry {
            conductors: vec![Conductor {
                id: "signal".into(),
                role: "PEC".into(),
                bounds_m: [-w / 2., h, w / 2., h + t],
            }],
            dielectrics: vec![
                Dielectric {
                    id: "bottom".into(),
                    y_min_m: 0.,
                    y_max_m: h,
                    relative_permittivity: 1.,
                    state: "air".into(),
                },
                Dielectric {
                    id: "air".into(),
                    y_min_m: h,
                    y_max_m: h + t + w * 8.,
                    relative_permittivity: 1.,
                    state: "air".into(),
                },
            ],
            bounds_m: [-w * 8., 0., w * 8., h + t + w * 8.],
            reference_y_m: vec![0.],
            length: None,
            kind: "microstrip".into(),
            artificial_boundary: "natural".into(),
        };
        for (step, scale) in [
            (base * 2., 16.),
            (base, 16.),
            (base / 2., 16.),
            (base / 2., 32.),
        ] {
            let field = match extraction::solve_field(&g.domain(scale), step) {
                Ok(f) => f,
                Err(reason) => {
                    assert!(reason.starts_with("SECTION_MEMORY_LIMIT"), "{reason}");
                    println!(
                        "{}",
                        json!({"widthM":w,"stepM":step,"domainScale":scale,"status":"refused at unchanged diagnostic coupon budget","reason":reason})
                    );
                    continue;
                }
            };
            let p = losses::integrals(&field).unwrap();
            let rows:Vec<_>=p.surfaces.iter().map(|s|{let (gram,current,residual)=consistent(&field,s);assert!(residual<1e-12);assert!((current-if s.id=="signal"{1.}else{-1.}).abs()<1e-8);json!({"id":s.id,"lumpedGramPerM":s.gram_per_m[0][0],"consistentGramPerM":gram,"integratedCurrent":current,"residual":residual})}).collect();
            let rs = (std::f64::consts::PI * freq * extraction::MU0 / 58e6).sqrt();
            let z = (extraction::MU0 * extraction::EPS0).sqrt() / field.vacuum_capacitance[0][0];
            let lumped: f64 = p.surfaces.iter().map(|s| s.gram_per_m[0][0]).sum();
            let cons: f64 = rows
                .iter()
                .map(|r| r["consistentGramPerM"].as_f64().unwrap())
                .sum();
            println!(
                "{}",
                json!({"widthM":w,"heightM":h,"thicknessM":t,"frequencyHz":freq,"stepM":step,"domainScale":scale,"nodes":field.nodes.len(),"impedanceOhm":z,"surfaces":rows,"lumpedAlphaNpPerM":rs*lumped/(2.*z),"consistentAlphaNpPerM":rs*cons/(2.*z)})
            );
        }
    }
}

#[test]
fn r1_fixed_cases_match_independent_boundary_panel_reference() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-line-losses/boundary-panel-references.json"
    ))
    .unwrap();
    for case in reference["fixedCases"].as_array().unwrap() {
        let r = &case["finest"];
        let w = r["widthM"].as_f64().unwrap() * 1000.;
        let copper = json!({"conductivitySM":58e6,"state":"smooth fixed-temperature reference"});
        let line = json!({"kind":"microstrip","widthMm":w,"dielectricHeightMm":r["heightM"].as_f64().unwrap()*1000.,"copperThicknessMm":r["thicknessM"].as_f64().unwrap()*1000.,"material":{"relativePermittivity":1.,"lossTangent":0.,"state":"air"},"conductors":{"signal":copper,"referencePlanes":{"bottom":{"thicknessMm":0.035,"material":copper}}}});
        let output=extraction::run(serde_json::from_value(json!({"line":line,"frequencies":[r["frequencyHz"]],"mesh":{"maxNodes":300000,"maxTriangles":600000,"maxEdgeMm":if w<1.{0.00375}else{0.00625}},"budget":{"maxMemoryMiB":1024}})).unwrap());
        assert_eq!(output["status"], "complete", "{}", output["diagnostics"]);
        let actual = output["parameters"]["frequencyDependent"]["samples"][0]["modes"][0]
            ["conductorAlphaFirstOrderNpPerM"]
            .as_f64()
            .unwrap();
        let expected = r["alphaNpPerM"].as_f64().unwrap();
        let extrapolated = case["cornerRichardsonEstimate"]["alphaNpPerM"]
            .as_f64()
            .unwrap();
        for target in [expected, extrapolated] {
            assert!((actual / target - 1.).abs() <= 0.05, "{actual} vs {target}");
        }
        for (surface, key) in [(0, "signalGramPerM"), (1, "groundGramPerM")] {
            let got = output["lossIntegrals"]["surfaces"][surface]["gramPerM"][0][0]
                .as_f64()
                .unwrap();
            let target = r[key].as_f64().unwrap();
            assert!((got / target - 1.).abs() <= 0.05, "{key} {got} vs {target}");
        }
        println!(
            "{}",
            json!({"widthMm":w,"actualAlphaNpPerM":actual,"independentFinest":expected,"independentCornerExtrapolated":extrapolated,"relativeFinestError":actual/expected-1.,"relativeExtrapolatedError":actual/extrapolated-1.,"referenceEstimatedUncertainty":case["estimatedRelativeUncertainty"],"nodeCount":output["field"]["nodes"].as_array().unwrap().len()})
        );
    }
}

// Reviewer-approved R2 physical norm, 2026-10-23. Nullspace action must vanish
// within explicitly scaled floating-point roundoff, otherwise relative power
// error is unbounded. No epsilon denominator replaces a physical zero.
fn power_error(actual: &serde_json::Value, reference: &serde_json::Value) -> Result<f64, String> {
    let a: Vec<Vec<f64>> = serde_json::from_value(actual.clone()).unwrap();
    let b: Vec<Vec<f64>> = serde_json::from_value(reference.clone()).unwrap();
    if b.len() == 1 {
        if b[0][0] == 0. {
            return if a[0][0] == 0. {
                Ok(0.)
            } else {
                Err("changed exact zero reference/nullspace".into())
            };
        }
        return Ok((a[0][0] / b[0][0] - 1.).abs());
    }
    let theta = 0.5 * (2. * b[0][1]).atan2(b[0][0] - b[1][1]);
    let (sn, cs) = theta.sin_cos();
    let u = [[cs, sn], [-sn, cs]];
    let mid = (b[0][0] + b[1][1]) / 2.;
    let radius = ((b[0][0] - b[1][1]) / 2.).hypot(b[0][1]);
    let lambda = [mid + radius, mid - radius];
    let cutoff = 64. * f64::EPSILON * lambda[0].abs();
    if lambda[1] < -cutoff {
        return Err("reference is not PSD".into());
    }
    let d = [
        [a[0][0] - b[0][0], a[0][1] - b[0][1]],
        [a[1][0] - b[1][0], a[1][1] - b[1][1]],
    ];
    let scale = a
        .iter()
        .chain(&b)
        .flatten()
        .map(|v| v.abs())
        .fold(0., f64::max);
    let roundoff = 256. * f64::EPSILON * scale;
    let mut w = [[0.; 2]; 2];
    for i in 0..2 {
        if lambda[i] <= cutoff {
            let action = [
                d[0][0] * u[i][0] + d[0][1] * u[i][1],
                d[1][0] * u[i][0] + d[1][1] * u[i][1],
            ];
            if action[0].hypot(action[1]) > roundoff {
                return Err("changed reference nullspace: unbounded relative power error".into());
            }
        }
        for j in 0..2 {
            if lambda[i] > cutoff && lambda[j] > cutoff {
                let transformed = u[i][0] * (d[0][0] * u[j][0] + d[0][1] * u[j][1])
                    + u[i][1] * (d[1][0] * u[j][0] + d[1][1] * u[j][1]);
                w[i][j] = transformed / (lambda[i] * lambda[j]).sqrt();
            }
        }
    }
    let center = (w[0][0] + w[1][1]) / 2.;
    let radius = ((w[0][0] - w[1][1]) / 2.).hypot(w[0][1]);
    Ok((center + radius).abs().max((center - radius).abs()))
}
#[test]
fn power_reference_norm_checks_nullspace_and_aggregated_mutual_error() {
    assert_eq!(power_error(&json!([[0.]]), &json!([[0.]])).unwrap(), 0.);
    assert!(power_error(&json!([[1e-30]]), &json!([[0.]])).is_err());
    let reference = json!([[1., 1.], [1., 1.]]);
    assert!(
        (power_error(&json!([[1.01, 1.01], [1.01, 1.01]]), &reference).unwrap() - 0.01).abs()
            < 1e-12
    );
    assert!(power_error(&json!([[1.001, 1.], [1., 1.]]), &reference).is_err());
    // Several individually small entries have a larger power error.
    assert!(
        (power_error(
            &json!([[1.04, 0.04], [0.04, 1.04]]),
            &json!([[1., 0.], [0., 1.]])
        )
        .unwrap()
            - 0.08)
            .abs()
            < 1e-12
    );
}

#[test]
fn independent_pair_surface_and_mutual_resistance() {
    let references: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-line-losses/boundary-panel-references.json"
    ))
    .unwrap();
    for case in references["pairedCases"].as_array().unwrap() {
        let reference = case["history"].as_array().unwrap().last().unwrap();
        let gap = case["gapMm"].as_f64().unwrap();
        let history = case["history"].as_array().unwrap();
        let refinement = power_error(
            &history[history.len() - 1]["resistanceOhmPerM"],
            &history[history.len() - 2]["resistanceOhmPerM"],
        )
        .unwrap();
        assert!(refinement < 0.01, "reference refinement {refinement}");
        let material = json!({"relativePermittivity":1.,"lossTangent":0.,"state":"air reference"});
        let copper = json!({"conductivitySM":58e6,"state":"smooth fixed reference"});
        let line = json!({"kind":"coupled-microstrip","widthMm":0.3,"dielectricHeightMm":0.18,"copperThicknessMm":0.035,"gapMm":gap,"material":material,"conductors":{"signal":copper,"referencePlanes":{"bottom":{"thicknessMm":0.035,"material":copper}}}});
        let output=extraction::run(serde_json::from_value(json!({"line":line,"frequencies":[1e9],"mesh":{"maxNodes":300000,"maxTriangles":600000,"maxEdgeMm":if gap<0.1 {0.00125}else{0.00625},"domainScale":16.},"budget":{"maxMemoryMiB":1024}})).unwrap());
        assert_eq!(output["status"], "complete", "{}", output["diagnostics"]);
        let sample = &output["parameters"]["frequencyDependent"]["samples"][0];
        let mut errors = vec![];
        let mut absolute_errors = vec![];
        for i in 0..2 {
            for j in 0..2 {
                let a = sample["resistanceOhmPerM"][i][j].as_f64().unwrap();
                let b = reference["resistanceOhmPerM"][i][j].as_f64().unwrap();
                errors.push(a / b - 1.);
                absolute_errors.push(a - b);
            }
        }
        let mut surface_errors = vec![];
        for q in 0..3 {
            let a = &output["lossIntegrals"]["surfaces"][q]["gramPerM"];
            let b = &reference["surfaceGramPerM"][q];
            let mut relative = vec![];
            let mut normalized = vec![];
            for i in 0..2 {
                for j in 0..2 {
                    let x = a[i][j].as_f64().unwrap();
                    let y = b[i][j].as_f64().unwrap();
                    relative.push(x / y - 1.);
                    normalized.push(
                        (x - y) / (b[i][i].as_f64().unwrap() * b[j][j].as_f64().unwrap()).sqrt(),
                    );
                }
            }
            surface_errors.push(json!({"surface":q,"signedRelativeErrors":relative,"correlationNormalizedErrors":normalized}));
        }
        println!(
            "{}",
            json!({"gapMm":gap,"nativeResistance":sample["resistanceOhmPerM"],"referenceResistance":reference["resistanceOhmPerM"],"relativeErrors":errors,"absoluteErrorsOhmPerM":absolute_errors,"surfaceErrors":surface_errors,"nativeSurfaceGram":output["lossIntegrals"]["surfaces"].as_array().unwrap().iter().map(|v|v["gramPerM"].clone()).collect::<Vec<_>>(),"referenceSurfaceGram":reference["surfaceGramPerM"],"nodes":output["field"]["nodes"].as_array().unwrap().len(),"levels":output["convergence"].as_array().unwrap().iter().map(|v|json!({"stepM":v["stepM"],"domainScale":v["domainScale"],"nodes":v["nodes"],"loss":v["lossConvergence"]["maxNormalizedChange"]})).collect::<Vec<_>>(),"referenceIsPublished":false})
        );
        // Scalar mutual discrepancies are retained above and in the historical
        // red probe; the approved physical gate uses all-current power error.
        let total_power_error = power_error(
            &sample["resistanceOhmPerM"],
            &reference["resistanceOhmPerM"],
        )
        .unwrap();
        assert!(
            total_power_error <= 0.05,
            "gap {gap}: full R power error {total_power_error}"
        );
        let mut surface_power_errors = vec![];
        for q in 0..3 {
            let error = power_error(
                &output["lossIntegrals"]["surfaces"][q]["gramPerM"],
                &reference["surfaceGramPerM"][q],
            )
            .unwrap();
            assert!(error <= 0.05, "gap {gap}: surface {q} power error {error}");
            surface_power_errors.push(error);
        }
        println!(
            "{}",
            json!({"gapMm":gap,"approvedPowerNormError":total_power_error,"individualSurfacePowerNormErrors":surface_power_errors,"scalarMutualErrorRetained":errors[1],"cornerEstimatePowerErrorDiagnostic":power_error(&sample["resistanceOhmPerM"],&case["cornerRichardsonResistanceOhmPerM"]).unwrap(),"eachScalarWithinFivePercentClaim":false})
        );
        for (k, key) in [(0, "even"), (1, "odd")] {
            let a = sample["modes"][k]["resistanceOhmPerM"].as_f64().unwrap();
            let b = reference["modeResistanceOhmPerM"][key].as_f64().unwrap();
            assert!((a / b - 1.).abs() <= 0.05);
            let c = reference["capacitanceFPerM"][0][0].as_f64().unwrap()
                + if k == 0 { 1. } else { -1. }
                    * reference["capacitanceFPerM"][0][1].as_f64().unwrap();
            let z = (extraction::MU0 * extraction::EPS0).sqrt() / c;
            let alpha_ref = b / (2. * z);
            let alpha_native = sample["modes"][k]["conductorAlphaFirstOrderNpPerM"]
                .as_f64()
                .unwrap();
            assert!((alpha_native / alpha_ref - 1.).abs() <= 0.05);
            println!(
                "{}",
                json!({"gapMm":gap,"mode":key,"independentAlphaNpPerM":alpha_ref,"nativeAlphaNpPerM":alpha_native,"relativeAlphaError":alpha_native/alpha_ref-1.})
            );
        }
    }
}

// Supplementary publication comparison: all nominal-thickness endpoints were
// declared before calculation; no exact-thickness or mask C/Z/alpha claim.
#[test]
fn johnson_published_loop_resistance_nominal_thickness_bracket() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-line-losses/johnson-reference.json"
    ))
    .unwrap();
    let mut comparisons = vec![];
    for row in reference["rows"].as_array().unwrap() {
        for thickness in reference["thicknessBracket"]["valuesMm"]
            .as_array()
            .unwrap()
        {
            let copper = json!({"conductivitySM":reference["conductivitySM"],"state":"Explicit published conductivity; smooth fixed-temperature hypothesis"});
            let line = json!({"kind":"coupled-microstrip","widthMm":row["widthMil"].as_f64().unwrap()*0.0254,"dielectricHeightMm":0.127,"copperThicknessMm":thickness,"gapMm":row["gapMil"].as_f64().unwrap()*0.0254,"material":{"relativePermittivity":4.3,"lossTangent":0.,"state":"Explicit nonmagnetic dielectric; mask excluded from R-only comparison"},"conductors":{"signal":copper,"referencePlanes":{"bottom":{"thicknessMm":reference["referencePlane"]["thicknessMm"],"material":copper}}}});
            let output=extraction::run(serde_json::from_value(json!({"line":line,"frequencies":[reference["frequencyHz"]],"mesh":{"maxNodes":300000,"maxTriangles":600000,"maxEdgeMm":0.005},"budget":{"maxMemoryMiB":1024}})).unwrap());
            assert_eq!(output["status"], "complete", "{}", output["diagnostics"]);
            let actual = 2.
                * output["parameters"]["frequencyDependent"]["samples"][0]["modes"][1]
                    ["resistanceOhmPerM"]
                    .as_f64()
                    .unwrap();
            let target = row["loopResistanceOhmPerM"].as_f64().unwrap();
            comparisons.push(json!({"widthMil":row["widthMil"],"gapMil":row["gapMil"],"thicknessMm":thickness,"nativeLoopOhmPerM":actual,"publishedLoopOhmPerM":target,"relativeError":actual/target-1.,"withinFivePercentDiagnostic":(actual/target-1.).abs()<=0.05,"exactReferenceClaim":false,"nodes":output["field"]["nodes"].as_array().unwrap().len()}));
        }
    }
    println!(
        "{}",
        json!({"supplementaryJohnsonComparisons":comparisons,"mainQuantitativeGate":"independent BEM both modes/full R power norm <=5%","thicknessEndpointsSelectedByResult":false})
    );
    assert_eq!(comparisons.len(), 8);
}
