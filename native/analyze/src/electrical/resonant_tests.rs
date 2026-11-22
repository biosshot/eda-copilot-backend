use super::*;
fn source(mutual: bool, coarse: bool) -> frequency::Resource {
    let text = match (mutual, coarse) {
        (false, false) => {
            include_str!("../../../../tests/fixtures/analysis-resonant-checkpoint/series-1025.json")
        }
        (true, false) => {
            include_str!("../../../../tests/fixtures/analysis-resonant-checkpoint/mutual-1025.json")
        }
        (_, true) => {
            include_str!("../../../../tests/fixtures/analysis-resonant-checkpoint/mutual-257.json")
        }
    };
    let fixture: Value = serde_json::from_str(text).unwrap();
    let mut value = fixture["resource"].clone();
    value["analyses"] = json!(["si", "networkParameters"]);
    value["validity"] = json!({"status":"conditional","reason":"Authored frozen coupon","domain":{"bandHz":[0.,1e8]}});
    value["data"]["temporal"] = json!({"method":"passive-resonant-y","normalizationOhm":50.,
        "modeTransform":fixture["checkpointContract"]["orthonormalModes"],"dcSlopeF":if mutual {vec![1e-9,2.5e-10]}else{vec![1e-9]},
        "highFrequency":{"inverseInductancePerH":if mutual {vec![1e5,4e4]}else{vec![1e5]}}});
    serde_json::from_value(value).unwrap()
}
fn divide(a: C, b: C) -> C {
    let d = b.real * b.real + b.imaginary * b.imaginary;
    C::new(
        (a.real * b.real + a.imaginary * b.imaginary) / d,
        (a.imaginary * b.real - a.real * b.imaginary) / d,
    )
}
/// Physical full impedance matrix and explicit 2x2 inverse, independent of fit.
fn ac_reference(f: f64, n: usize) -> Matrix {
    let u = if n == 1 {
        vec![vec![1.]]
    } else {
        let (c, s) = (0.37_f64.cos(), 0.37_f64.sin());
        vec![vec![c, -s], vec![s, c]]
    };
    let mut z = math::zero(n, n);
    let w = 2. * std::f64::consts::PI * f;
    for (m, (r, l, c)) in [(50., 1e-5, 1e-9), (70., 2.5e-5, 2.5e-10)]
        .iter()
        .enumerate()
        .take(n)
    {
        for i in 0..n {
            for j in 0..n {
                z[i][j] = z[i][j] + C::new(*r, w * l - 1. / (w * c)).scale(u[i][m] * u[j][m]);
            }
        }
    }
    if n == 1 {
        return vec![vec![divide(C::new(1., 0.), z[0][0])]];
    }
    let det = z[0][0] * z[1][1] - z[0][1] * z[1][0];
    vec![
        vec![divide(z[1][1], det), divide(z[0][1].scale(-1.), det)],
        vec![divide(z[1][0].scale(-1.), det), divide(z[0][0], det)],
    ]
}
#[test]
fn frozen_series_mutual_closure_units_and_existing_descriptor_ac() {
    for mutual in [false, true] {
        let resource = source(mutual, false);
        let n = resource.ports.len();
        let fit = fit_started(&resource, None, Instant::now()).unwrap();
        for (m, (r, l, c)) in [(50., 1e-5, 1e-9), (70., 2.5e-5, 2.5e-10)]
            .iter()
            .enumerate()
            .take(n)
        {
            for (key, want) in [
                ("resistanceOhm", r),
                ("inductanceH", l),
                ("capacitanceF", c),
            ] {
                assert!(
                    (fit.report["coefficients"][m][key].as_f64().unwrap() / want - 1.).abs()
                        < 1e-10
                );
            }
        }
        let frequencies: Vec<f64> = (0..32)
            .map(|i| 1e4 * 10_f64.powf((i as f64 + 0.31873) / 8.))
            .collect();
        let mut descriptor = resource.clone();
        descriptor.data = serde_json::to_value(fit.model).unwrap();
        let request:frequency::Request=serde_json::from_value(json!({"instances":[{"id":"fit","model":descriptor,"ports":resource.ports}],
            "connections":[],"ports":(0..n).map(|i|json!({"name":format!("p{i}"),"index":i})).collect::<Vec<_>>(),
            "frequencies":frequencies,"representation":"Y","budget":{"maxSeconds":60,"maxMemoryMiB":256}})).unwrap();
        let result = frequency::run(request);
        assert_eq!(result["status"], "complete", "{result}");
        let values: Vec<Matrix> = serde_json::from_value(result["parameters"].clone()).unwrap();
        for (index, f) in frequencies.iter().enumerate() {
            let reference = ac_reference(*f, n);
            for i in 0..n {
                for j in 0..n {
                    let diff = (values[index][i][j] - reference[i][j]).abs() * 50.;
                    assert!(diff <= 0.005 * reference[i][j].abs() * 50. + 1e-8);
                }
            }
        }
    }
}
fn time_request(mutual: bool, refined: bool) -> coupled::time::Request {
    let resource = source(mutual, false);
    let n = resource.ports.len();
    serde_json::from_value(json!({"resource":resource,
        "drivers":(0..n).map(|i|json!({"port":i,"resistanceOhm":25.,"waveform":{"kind":"step","from":0.,"to":if i==0{1.}else{0.4},"atS":if i==0{0.}else{1.5e-7},"riseS":2e-7}})).collect::<Vec<_>>(),
        "receivers":[],"time":{"stopS":2e-5,"sampleStepS":1e-8},"initial":"steady",
        "numerics":{"relativeTolerance":if refined{2.5e-6}else{1e-5},"absoluteTolerance":if refined{2.5e-10}else{1e-9},"maxStepS":if refined{1e-9}else{2e-9}},
        "budget":{"maxSeconds":60,"maxMemoryMiB":256}})).unwrap()
}
/// Closed-form integrated step for a finite ramp, not a numerical ODE solve.
fn time_reference(t: f64, n: usize) -> (Vec<f64>, f64) {
    let u = if n == 1 {
        vec![vec![1.]]
    } else {
        let (c, s) = (0.37_f64.cos(), 0.37_f64.sin());
        vec![vec![c, -s], vec![s, c]]
    };
    let mut currents = vec![0.; n];
    let mut energy = 0.;
    for (m, (r, l, c)) in [(50., 1e-5, 1e-9), (70., 2.5e-5, 2.5e-10)]
        .iter()
        .enumerate()
        .take(n)
    {
        let k = 1. / l;
        let b: f64 = 1. / (l * c);
        let alpha = (r + 25.) / l / 2.;
        let beta = (b - alpha * alpha).sqrt();
        let p = C::new(-alpha, beta);
        let integrated = |v: f64| {
            let v = v.max(0.);
            let e = divide(
                C::new(
                    (-alpha * v).exp() * (beta * v).cos() - 1.,
                    (-alpha * v).exp() * (beta * v).sin(),
                ),
                p,
            );
            [
                (v - (C::new(1., -alpha / beta) * e).real) / b,
                e.imaginary / beta,
            ]
        };
        let mut x = [0.; 2];
        for (i, amplitude) in [1., 0.4].iter().enumerate().take(n) {
            let delay = if i == 0 { 0. } else { 1.5e-7 };
            let a = integrated(t - delay);
            let b = integrated(t - delay - 2e-7);
            for j in 0..2 {
                x[j] += (a[j] - b[j]) / 2e-7 * amplitude * u[i][m];
            }
        }
        for i in 0..n {
            currents[i] += u[i][m] * k * x[1];
        }
        energy += 0.5 * k * (b * x[0] * x[0] + x[1] * x[1]);
    }
    (currents, energy)
}
fn transient_coupon(mutual: bool) {
    let mut saved = vec![];
    for refined in [false, true] {
        let request = time_request(mutual, refined);
        let n = request.drivers.len();
        let result = coupled::time::solve(request).unwrap();
        assert_eq!(result["status"], "complete");
        assert_eq!(result["conditional"], true);
        let axis: Vec<f64> = serde_json::from_value(result["axis"].clone()).unwrap();
        let currents: Vec<Vec<f64>> =
            serde_json::from_value(result["driverCurrentsA"].clone()).unwrap();
        let energy: Vec<f64> = serde_json::from_value(result["energyJ"].clone()).unwrap();
        for (j, t) in axis.iter().enumerate() {
            let (reference, eref) = time_reference(*t, n);
            for i in 0..n {
                assert!((currents[j][i] - reference[i]).abs() <= 0.01 * 0.02 + 1e-12);
            }
            assert!((energy[j] - eref).abs() <= 0.01 * 2e-9 + 1e-20);
        }
        assert!(
            result["quality"]["normalizedEnergyBalance"]
                .as_f64()
                .unwrap()
                <= 1e-4
        );
        saved.push(currents);
    }
    for (a, b) in saved[0].iter().zip(&saved[1]) {
        for (a, b) in a.iter().zip(b) {
            assert!((a - b).abs() <= 0.001 * 0.02 + 1e-12);
        }
    }
}
#[test]
fn series_existing_production_transient_and_refinement() {
    transient_coupon(false);
}
#[test]
fn assembled_dc_bias_retains_physical_capacitor_energy() {
    let resource = source(true, false);
    let fit = fit_started(&resource, None, Instant::now()).unwrap();
    let mut system = coupled::System::compile(&fit.model, 2, None).unwrap();
    for port in 0..2 {
        system.add_port_admittance(port, 1. / 25., 0.).unwrap();
    }
    let x = system
        .solve(
            0.,
            system.port_rhs(&[0.2 / 25., 0.1 / 25.]),
            &vec![0.; 6],
            &mut Counters::default(),
        )
        .unwrap();
    let (c, s) = (0.37_f64.cos(), 0.37_f64.sin());
    let modal = [c * 0.2 + s * 0.1, -s * 0.2 + c * 0.1];
    for i in 0..2 {
        assert!((x[2 + i] - modal[i]).abs() < 1e-12);
        assert!(x[4 + i].abs() < 1e-12);
    }
    let expected = 0.5 * (1e-9 * modal[0] * modal[0] + 2.5e-10 * modal[1] * modal[1]);
    assert!((system.energy(&x) - expected).abs() < 1e-22);
}
#[test]
fn mutual_existing_production_transient_and_refinement() {
    transient_coupon(true);
}
#[test]
fn invalid_metadata_samples_condition_resolution_and_budgets_refuse() {
    let good = source(true, false);
    for (name, path, value) in [
        ("U", "/temporal/modeTransform", json!([[1., 1.], [0., 1.]])),
        ("closure", "/temporal/dcSlopeF", json!([2e-9, 2.5e-10])),
        ("shape", "/temporal/dcSlopeF", json!([1e-9])),
        ("nonfinite-null", "/temporal/normalizationOhm", Value::Null),
        ("unknown-class", "/temporal/method", json!("unknown")),
        ("Z", "/representation", json!("Z")),
        ("reciprocity", "/parameters/4/0/1/imaginary", json!(0.5)),
        ("residual", "/parameters/6/0/0/real", json!(0.1)),
    ] {
        let mut bad = good.clone();
        *bad.data.pointer_mut(path).unwrap() = value;
        assert!(fit_started(&bad, None, Instant::now()).is_err(), "{name}");
    }
    let mut bad = good.clone();
    bad.data["temporal"]
        .as_object_mut()
        .unwrap()
        .remove("dcSlopeF");
    assert!(fit_started(&bad, None, Instant::now()).is_err());
    let mut bad = good.clone();
    for row in bad.data["parameters"].as_array_mut().unwrap() {
        for row in row.as_array_mut().unwrap() {
            for value in row.as_array_mut().unwrap() {
                for key in ["real", "imaginary"] {
                    value[key] = json!(-value[key].as_f64().unwrap());
                }
            }
        }
    }
    assert!(fit_started(&bad, None, Instant::now())
        .err()
        .unwrap()
        .contains("PASSIVITY"));
    assert!(fit_started(&source(true, true), None, Instant::now())
        .err()
        .unwrap()
        .contains("RESOLUTION"));
    let rows: Vec<[f64; 3]> = (0..32).map(|i| [1., 1. + i as f64 * 1e-13, 1.]).collect();
    assert!(qr(&rows, &vec![1.; 32]).is_err());
    let mut request = time_request(true, false);
    request.drivers[0].waveform = Waveform::Step {
        from: 0.,
        to: 1.,
        at: 0.,
        rise: 1e-9,
    };
    assert!(coupled::time::solve(request)
        .err()
        .unwrap()
        .contains("BAND"));
    let tiny = Budget {
        max_seconds: Some(60.),
        max_memory_mi_b: Some(0.001),
    };
    assert!(fit_started(&good, Some(&tiny), Instant::now())
        .err()
        .unwrap()
        .contains("BUDGET"));
    let expired = Budget {
        max_seconds: Some(1e-6),
        max_memory_mi_b: Some(256.),
    };
    assert!(fit_started(
        &good,
        Some(&expired),
        Instant::now() - std::time::Duration::from_secs(1)
    )
    .is_err());
}
