use pcb_analyze::electrical::{
    coupled::{self, Model, System},
    Counters,
};
use serde_json::json;
fn sphere() -> Model {
    let c1 = 2.225300110895741e-13;
    let c2 = 4.450600221791482e-13;
    let c3 = c2;
    serde_json::from_value(json!({"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["inner","middle","outer"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.],[2,0,-1.],[1,1,1.],[2,1,-1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,c1],[0,1,-c1],[1,0,-c1],[1,1,c1+c2],[1,2,-c2],[2,1,-c2],[2,2,c2+c3]]})).unwrap()
}
fn rlc() -> Model {
    serde_json::from_value(json!({"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["a","b"],"currentDofs":["i1","i2"],"incidence":[[0,0,1.],[1,1,1.]],"portIncidence":[[0,0,1.],[1,1,1.]],"resistanceOhm":[[0,0,0.02],[1,1,0.03]],"inductanceH":[[0,0,8e-9],[0,1,2e-9],[1,0,2e-9],[1,1,12e-9]],"capacitanceF":[[0,0,200e-12],[0,1,-40e-12],[1,0,-40e-12],[1,1,160e-12]]})).unwrap()
}
#[test]
fn full_infinity_capacitance_sphere_ports_and_charged_energy() {
    let model = sphere();
    let result = coupled::impedance(&model, 2, &[1e5], None).unwrap();
    let expected = [
        [-10728099.705245916, -3576033.235081972],
        [-3576033.235081972, -3576033.235081972],
    ];
    for i in 0..2 {
        for j in 0..2 {
            let z = &result["parameters"][0][i][j];
            assert!(z["real"].as_f64().unwrap().abs() < 1e-6);
            assert!(
                (z["imaginary"].as_f64().unwrap() - expected[i][j]).abs()
                    < 1e-6 + 1e-10 * expected[i][j].abs()
            );
        }
    }
    assert!(coupled::impedance(&model, 2, &[0.], None)
        .unwrap_err()
        .starts_with("COUPLED_DC_INCOMPATIBLE"));
    let mut s = System::compile(&model, 2, None).unwrap();
    let charge = 1e-12;
    let outer = charge / 4.450600221791482e-13;
    let initial = vec![outer; 3];
    let x = s
        .solve(0., vec![0.; 3], &initial, &mut Counters::default())
        .unwrap();
    for v in &x {
        assert!((v - outer).abs() < 1e-10);
    }
    let expected_energy = charge * charge / (2. * 4.450600221791482e-13);
    assert!((s.energy(&x) - expected_energy).abs() < 1e-24);
    let neutral = s
        .solve(0., vec![0.; 3], &[0.; 3], &mut Counters::default())
        .unwrap();
    assert_eq!(neutral, vec![0.; 3]);
    // An erroneous grounded-middle reduction gives half these transfer entries.
    let false_transfer = -1. / (2. * std::f64::consts::PI * 1e5 * (2. * 4.450600221791482e-13));
    assert!((false_transfer - expected[1][1]).abs() / expected[1][1].abs() > 0.49);
}
#[test]
fn assembled_resistors_change_charge_invariants_without_grounding_metal() {
    let mut s = System::compile(&sphere(), 2, None).unwrap();
    s.add_port_admittance(0, 0.1, 0.).unwrap();
    let rhs = s.port_rhs(&[0.1, 0.]);
    let x = s
        .solve(0., rhs.clone(), &[0.; 3], &mut Counters::default())
        .unwrap();
    assert!((x[0] - 1.).abs() < 1e-10);
    assert!((x[1] - 1. / 3.).abs() < 1e-10);
    assert!(x[2].abs() < 1e-10);
    s.add_port_admittance(1, 0.2, 0.).unwrap();
    let y = s
        .solve(0., rhs, &[0.; 3], &mut Counters::default())
        .unwrap();
    assert!((y[0] - 1.).abs() < 1e-10);
    assert!(y[1].abs() < 1e-10);
    assert!(y[2].abs() < 1e-10);
}
#[test]
fn coupled_rlc_matches_frozen_p7_complex_oracle_and_basis_scaling() {
    let m = rlc();
    let r = coupled::impedance(&m, 2, &[0., 1e5], None).unwrap();
    let expected = [
        [
            (0.020000024002765595, 0.005026500996462809),
            (-6.315725198242782e-10, 0.0012566528951862992),
        ],
        [
            (-6.315725198242782e-10, 0.0012566528951862992),
            (0.03000004358393629, 0.0075397373278135605),
        ],
    ];
    for i in 0..2 {
        for j in 0..2 {
            for (field, value) in [("real", expected[i][j].0), ("imaginary", expected[i][j].1)] {
                assert!(
                    (r["parameters"][1][i][j][field].as_f64().unwrap() - value).abs()
                        < 1e-12 + 1e-10 * value.abs()
                );
            }
        }
    }
    let mut scaled = m.clone();
    let d = [2., 0.5];
    for (i, j, v) in &mut scaled.incidence {
        let _ = i;
        *v *= d[*j];
    }
    for matrix in [&mut scaled.resistance_ohm, &mut scaled.inductance_h] {
        for (i, j, v) in matrix {
            *v *= d[*i] * d[*j];
        }
    }
    let q = coupled::impedance(&scaled, 2, &[0., 1e5], None).unwrap();
    for f in 0..2 {
        for i in 0..2 {
            for j in 0..2 {
                for field in ["real", "imaginary"] {
                    assert!(
                        (r["parameters"][f][i][j][field].as_f64().unwrap()
                            - q["parameters"][f][i][j][field].as_f64().unwrap())
                        .abs()
                            < 1e-12
                    );
                }
            }
        }
    }
}
#[test]
fn zero_energy_gauge_is_not_a_stored_charge_or_external_ground() {
    let mut m = sphere();
    m.current_dofs = vec!["i1".into(), "i2".into()];
    m.incidence = vec![(0, 0, 1.), (1, 0, -1.), (1, 1, 1.), (2, 1, -1.)];
    m.resistance_ohm = vec![(0, 0, 1.), (1, 1, 2.)];
    m.capacitance_f.clear();
    m.conserved_potential_modes = vec![vec![1.; 3]];
    let mut s = System::compile(&m, 2, None).unwrap();
    let rhs = s.port_rhs(&[1., 0.]);
    let x = s
        .solve(0., rhs, &[0.; 5], &mut Counters::default())
        .unwrap();
    assert!((x[0] - x[2] - 3.).abs() < 1e-12);
    assert!(x[..3].iter().sum::<f64>().abs() < 1e-12);
    assert_eq!(s.energy(&x), 0.);
    let mut duplicate = m.clone();
    duplicate.conserved_potential_modes.push(vec![2.; 3]);
    assert!(System::compile(&duplicate, 2, None)
        .err()
        .unwrap()
        .starts_with("COUPLED_GAUGE"));
    let mut wrong = m;
    wrong.port_incidence[0].2 = 2.;
    assert!(System::compile(&wrong, 2, None)
        .err()
        .unwrap()
        .starts_with("COUPLED_CHARGE"));
}

#[test]
fn finite_front_capacitor_time_matches_closed_form_and_discrete_energy() {
    let input = json!({"resource":{"id":"one-cap","kind":"network","ports":["p"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["v"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,1e-6]]}},"drivers":[{"port":0,"resistanceOhm":1000.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":1e-4}}],"receivers":[],"time":{"stopS":0.005,"sampleStepS":0.0001},"numerics":{"relativeTolerance":1e-6,"absoluteTolerance":1e-9}});
    let result = coupled::time::solve(serde_json::from_value(input).unwrap()).unwrap();
    for (i, t) in result["axis"].as_array().unwrap().iter().enumerate() {
        let t = t.as_f64().unwrap();
        let tau = 0.001;
        let rise = 1e-4;
        let expected = if t <= rise {
            (t - tau * (1. - (-t / tau).exp())) / rise
        } else {
            1. - tau / rise * (1. - (-rise / tau).exp()) * (-(t - rise) / tau).exp()
        };
        assert!((result["portVoltages"][i][0].as_f64().unwrap() - expected).abs() < 0.001);
    }
    assert!(result["quality"]["energyBalanceJ"].as_f64().unwrap().abs() < 1e-14);
    assert!(
        result["quality"]["beNumericalDissipationJ"]
            .as_f64()
            .unwrap()
            > 0.
    );
    assert_eq!(result["conditional"], false);
}

fn attenuator() -> serde_json::Value {
    json!({"id":"half-voltage-matched-pi","kind":"network","ports":["in","out"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["in","out"],"currentDofs":["shunt-in","series","shunt-out"],"incidence":[[0,0,1.],[0,1,1.],[1,1,-1.],[1,2,1.]],"portIncidence":[[0,0,1.],[1,1,1.]],"resistanceOhm":[[0,0,150.],[1,1,37.5],[2,2,150.]],"inductanceH":[],"capacitanceF":[]}})
}
#[test]
fn grounded_cascade_joins_only_authored_nodes_and_matches_loaded_attenuator() {
    let r = attenuator();
    let request = json!({"topology":{"instances":[{"id":"first","model":r,"ports":["i1","o1"]},{"id":"second","model":r,"ports":["i2","o2"]}],"connections":[[1,2]],"ports":[{"name":"input","index":0},{"name":"output","index":3}]},"drivers":[{"port":0,"resistanceOhm":50.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":1e-6}}],"receivers":[{"port":1,"resistanceOhm":50.,"capacitanceF":0.}],"time":{"stopS":5e-6,"sampleStepS":1e-7}});
    let result = coupled::time::solve(serde_json::from_value(request.clone()).unwrap()).unwrap();
    for (i, at) in result["axis"].as_array().unwrap().iter().enumerate() {
        let level = (at.as_f64().unwrap() / 1e-6).min(1.);
        assert!((result["portVoltages"][i][0].as_f64().unwrap() - 0.5 * level).abs() < 1e-10);
        assert!((result["portVoltages"][i][1].as_f64().unwrap() - 0.125 * level).abs() < 1e-10);
    }
    let mut bad = request;
    bad["topology"]["connections"] = json!([]);
    assert!(coupled::time::solve(serde_json::from_value(bad).unwrap())
        .unwrap_err()
        .starts_with("COUPLED_TOPOLOGY"));
}
#[test]
fn public_frequency_route_keeps_descriptor_terminal_topology_and_rejects_implicit_join() {
    let resource = json!({"id":"sphere","kind":"network","ports":["p1","p2"],"validity":{"status":"conditional","domain":{"bandHz":[1e3,1e6],"returnContact":"outer"}},"data":sphere()});
    let input = json!({"instances":[{"id":"s","model":resource,"ports":["a","b"]}],"connections":[],"ports":[{"name":"a","index":0},{"name":"b","index":1}],"frequencies":[1e5],"representation":"Z"});
    let report =
        pcb_analyze::electrical::frequency::run(serde_json::from_value(input.clone()).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    assert_eq!(report["referenceTopology"], "infinity");
    assert_eq!(report["conditional"], true);
    assert_eq!(
        report["provenance"][0]["resource"]["validity"]["domain"]["returnContact"],
        "outer"
    );
    assert!(
        (report["parameters"][0][0][1]["imaginary"].as_f64().unwrap() + 3576033.235081972).abs()
            < 1e-6
    );
    let mut out = input.clone();
    out["frequencies"] = json!([1e7]);
    let rejected = pcb_analyze::electrical::frequency::run(serde_json::from_value(out).unwrap());
    assert_eq!(rejected["diagnostics"][0]["code"], "COUPLED_BAND");
    let mut joined = input;
    let second = joined["instances"][0].clone();
    joined["instances"].as_array_mut().unwrap().push(second);
    let rejected = pcb_analyze::electrical::frequency::run(serde_json::from_value(joined).unwrap());
    assert_eq!(rejected["diagnostics"][0]["code"], "COUPLED_TOPOLOGY");
    // Existing common-return S conversion is independently checked for an exact
    // matched pi attenuator, with no frequency samples substituted for its RLC.
    let input = json!({"instances":[{"id":"pad","model":attenuator(),"ports":["a","b"]}],"connections":[],"ports":[{"name":"a","index":0},{"name":"b","index":1}],"frequencies":[0.,1e5],"representation":"S","referenceOhm":[50.,50.]});
    let report = pcb_analyze::electrical::frequency::run(serde_json::from_value(input).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    for f in 0..2 {
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (report["parameters"][f][i][j]["real"].as_f64().unwrap()
                        - if i == j { 0. } else { 0.5 })
                    .abs()
                        < 1e-12
                );
                assert!(
                    report["parameters"][f][i][j]["imaginary"]
                        .as_f64()
                        .unwrap()
                        .abs()
                        < 1e-12
                );
            }
        }
    }
}

#[test]
fn full_mutual_lc_time_matches_independent_even_odd_ramp_integrals() {
    let mut m = rlc();
    m.resistance_ohm = vec![(0, 0, 10.), (1, 1, 10.)];
    m.inductance_h = vec![(0, 0, 2e-6), (0, 1, 0.5e-6), (1, 0, 0.5e-6), (1, 1, 2e-6)];
    m.capacitance_f = vec![(0, 0, 2e-9), (0, 1, -0.5e-9), (1, 0, -0.5e-9), (1, 1, 2e-9)];
    let rise = 5e-8;
    let input = json!({"resource":{"id":"commuting-mutual-lc","kind":"network","ports":["a","b"],"data":m},"drivers":[{"port":0,"resistanceOhm":1000.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":rise}},{"port":1,"resistanceOhm":1000.,"waveform":{"kind":"step","from":0.,"to":0.,"atS":0.,"riseS":rise}}],"receivers":[],"time":{"stopS":2e-6,"sampleStepS":2e-9},"numerics":{"relativeTolerance":1e-7,"absoluteTolerance":1e-10,"maxIterations":100000}});
    let result = coupled::time::solve(serde_json::from_value(input).unwrap()).unwrap();
    let response = |t: f64, l: f64, c: f64| {
        let g = 0.001;
        let resistance = 10.;
        let alpha = (resistance * c + g * l) / (2. * l * c);
        let omega = ((1. + g * resistance) / (l * c) - alpha * alpha).sqrt();
        let dc = resistance / (1000. * (1. + g * resistance));
        let a = -dc;
        let b = (1. / (1000. * c) - alpha * dc) / omega;
        let primitive = |at: f64| {
            let (sin, cos) = (omega * at).sin_cos();
            dc * at
                + (-alpha * at).exp()
                    * (a * (-alpha * cos + omega * sin) + b * (-alpha * sin - omega * cos))
                    / (alpha * alpha + omega * omega)
        };
        (primitive(t) - primitive((t - rise).max(0.))) / rise
    };
    let (mut peak, mut error) = ([0_f64; 2], [0_f64; 2]);
    for (i, at) in result["axis"].as_array().unwrap().iter().enumerate() {
        let t = at.as_f64().unwrap();
        let even = response(t, 2.5e-6, 1.5e-9);
        let odd = response(t, 1.5e-6, 2.5e-9);
        let expected = [(even + odd) / 2., (even - odd) / 2.];
        for j in 0..2 {
            peak[j] = peak[j].max(expected[j].abs());
            error[j] =
                error[j].max((result["portVoltages"][i][j].as_f64().unwrap() - expected[j]).abs());
        }
    }
    for j in 0..2 {
        assert!(peak[j] > 1e-3);
        assert!(
            error[j] <= 1e-6 + 0.001 * peak[j],
            "port{j} peak{} error{}",
            peak[j],
            error[j]
        );
    }
}

#[test]
fn finite_band_coupled_time_requires_dc_front_and_operation_permission() {
    let input = json!({"resource":{"id":"author-si-cap","kind":"network","ports":["p"],"analyses":["si"],"validity":{"status":"valid","domain":{"bandHz":[0.,1e5]}},"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["v"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,1e-6]]}},"drivers":[{"port":0,"resistanceOhm":1000.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":1e-4}}],"receivers":[],"time":{"stopS":0.001,"sampleStepS":1e-5}});
    for mutation in 0..4 {
        let mut bad = input.clone();
        match mutation {
            0 => bad["resource"]["validity"]["domain"]["bandHz"] = json!([100., 1e5]),
            1 => bad["drivers"][0]["waveform"]["riseS"] = json!(1e-6),
            2 => bad["resource"]["analyses"] = json!(["networkParameters"]),
            _ => bad["budget"] = json!({"maxSeconds":1e-12}),
        };
        assert!(
            coupled::time::solve(serde_json::from_value(bad).unwrap()).is_err(),
            "mutation{mutation}"
        );
    }
    let result = coupled::time::solve(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(result["conditional"], true);
}
