use pcb_analyze::electrical::coupled;
use serde_json::{json, Value};
fn line(delay: f64, gain: f64) -> Value {
    json!({"id":"line","kind":"network","ports":["a","b"],"data":{"format":"delayed-scattering","referenceTopology":"common","nature":"passive","referenceOhm":50.,"scattering":[[0.,gain],[gain,0.]],"delayS":delay}})
}
fn filter(r: f64, l: f64, c: f64) -> Value {
    json!({"id":"filter","kind":"network","ports":["a","b"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["a","b"],"currentDofs":["series"],"incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,1,1.]],"resistanceOhm":[[0,0,r]],"inductanceH":[[0,0,l]],"capacitanceF":[[1,1,c]]}})
}
fn shunt(c: f64) -> Value {
    json!({"id":"shunt","kind":"network","ports":["a","b"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["v"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.],[0,1,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,c]]}})
}
fn request(models: Vec<Value>, rs: f64, rl: f64, rise: f64, stop: f64) -> Value {
    let count = models.len();
    let instances: Vec<_> = models.into_iter().enumerate().map(|(i,model)|json!({"id":format!("b{i}"),"model":model,"ports":[format!("a{i}"),format!("z{i}")]})).collect();
    let connections: Vec<_> = (0..count - 1)
        .map(|i| json!([2 * i + 1, 2 * i + 2]))
        .collect();
    json!({"topology":{"instances":instances,"connections":connections,"ports":[{"name":"input","index":0},{"name":"output","index":2*count-1}]},"drivers":[{"port":0,"resistanceOhm":rs,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":rise}}],"receivers":[{"port":1,"resistanceOhm":rl,"capacitanceF":0.}],"time":{"stopS":stop,"sampleStepS":0.1e-9}})
}
fn run(input: Value) -> Value {
    let report = coupled::time::solve(serde_json::from_value(input.clone()).unwrap()).unwrap();
    if let Ok(dir) = std::env::var("P6_MIXED_EVIDENCE") {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        input.to_string().hash(&mut hash);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            format!("{dir}/case-{:016x}.json", hash.finish()),
            serde_json::to_vec(&json!({"request":input,"report":report})).unwrap(),
        )
        .unwrap();
    }
    report
}
fn check(report: &Value, expected: impl Fn(f64) -> f64) -> f64 {
    let error = report["axis"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            (report["portVoltages"][i][1].as_f64().unwrap() - expected(t.as_f64().unwrap())).abs()
        })
        .fold(0_f64, f64::max);
    assert!(error <= 0.001001, "reference error {error}");
    let q = &report["quality"];
    assert!(q["relativeResidual"].as_f64().unwrap() <= 1e-8);
    assert!(
        q["energyBalanceJ"].as_f64().unwrap().abs()
            <= 1e-4 * q["energyNormalizationJ"].as_f64().unwrap()
    );
    assert!(q["beNumericalDissipationJ"].as_f64().unwrap() >= 0.);
    error
}
fn rc_ramp(t: f64, rise: f64, tau: f64) -> f64 {
    let integral = |t: f64| {
        if t <= 0. {
            0.
        } else {
            t + tau * (-t / tau).exp_m1()
        }
    };
    (integral(t) - integral(t - rise)) / rise
}
#[test]
fn matched_delay_rc_finite_ramp_and_global_refinement() {
    let mut input = request(
        vec![line(2e-9, 1.), filter(50., 0., 30e-12)],
        50.,
        50.,
        0.5e-9,
        16e-9,
    );
    let coarse = run(input.clone());
    for (i, at) in coarse["axis"].as_array().unwrap().iter().enumerate() {
        if at.as_f64().unwrap() < 2e-9 {
            assert!(coarse["portVoltages"][i][1].as_f64().unwrap().abs() < 1e-12);
        }
    }
    check(&coarse, |t| rc_ramp(t - 2e-9, 0.5e-9, 1e-9) / 3.);
    input["numerics"] = json!({"relativeTolerance":2.5e-6,"absoluteTolerance":2.5e-9});
    let fine = run(input);
    check(&fine, |t| rc_ramp(t - 2e-9, 0.5e-9, 1e-9) / 3.);
    let delta = coarse["portVoltages"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fine["portVoltages"].as_array().unwrap())
        .map(|(a, b)| (a[1].as_f64().unwrap() - b[1].as_f64().unwrap()).abs())
        .fold(0_f64, f64::max);
    assert!(delta <= 0.0005, "refinement {delta}");
    if let Ok(dir) = std::env::var("P6_MIXED_EVIDENCE") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            format!("{dir}/rc.json"),
            serde_json::to_vec(&json!({"coarse":coarse,"fine":fine})).unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn two_delays_rlc_damped_sine_convolution() {
    let input = request(
        vec![line(2e-9, 1.), filter(5., 50e-9, 20e-12), line(3e-9, 1.)],
        50.,
        50.,
        0.5e-9,
        25e-9,
    );
    let report = run(input);
    let a: f64 = 50e-9 * 50. * 20e-12;
    let b = 50e-9 + 55. * 50. * 20e-12;
    let alpha = b / (2. * a);
    let omega = (105. / a - alpha * alpha).sqrt();
    let step = |t: f64| {
        if t <= 0. {
            0.
        } else {
            50. / 105.
                * (1.
                    - (-alpha * t).exp() * ((omega * t).cos() + alpha / omega * (omega * t).sin()))
        }
    };
    check(&report, |t| {
        let rise = 0.5e-9;
        let n = 200;
        let h = rise / n as f64;
        (0..=n)
            .map(|i| {
                step(t - 5e-9 - i as f64 * h)
                    * if i == 0 || i == n {
                        1.
                    } else if i % 2 == 0 {
                        2.
                    } else {
                        4.
                    }
            })
            .sum::<f64>()
            * h
            / (3. * rise)
    });
    if let Ok(dir) = std::env::var("P6_MIXED_EVIDENCE") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            format!("{dir}/rlc.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
    }
}
fn erlang_ramp(t: f64, rise: f64, tau: f64, m: usize) -> f64 {
    let integral = |t: f64| {
        if t <= 0. {
            return 0.;
        }
        let z = t / tau;
        let mut term = 1.;
        let mut sum = 0.;
        for q in 0..m {
            if q > 0 {
                term *= z / q as f64;
            }
            sum += (m - q) as f64 * term;
        }
        t - tau * m as f64 + tau * (-z).exp() * sum
    };
    (integral(t) - integral(t - rise)) / rise
}
#[test]
fn finite_driver_rc_receiver_multiple_reflections() {
    let mut input = request(vec![line(2e-9, 1.), shunt(20e-12)], 25., 75., 0.5e-9, 16e-9);
    input["receivers"][0]["capacitanceF"] = json!(2e-12);
    let report = run(input);
    check(&report, |t| {
        let mut total = 0.;
        for k in 0..4 {
            let mut choose = 1.;
            for j in 0..=k {
                if j > 0 {
                    choose *= (k - j + 1) as f64 / j as f64;
                }
                total += (2. / 3.)
                    * 1.2
                    * (-1_f64 / 3.).powi(k as i32)
                    * choose
                    * (-1_f64).powi((k - j) as i32)
                    * 1.2_f64.powi(j as i32)
                    * erlang_ramp(t - (2 + 4 * k) as f64 * 1e-9, 0.5e-9, 0.66e-9, j + 1);
            }
        }
        total
    });
    if let Ok(dir) = std::env::var("P6_MIXED_EVIDENCE") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            format!("{dir}/reflections.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn mixed_steady_rail_zero_delay_and_passive_attenuation() {
    for delay in [0., 2e-9] {
        let mut input = request(
            vec![line(delay, 0.8), filter(50., 10e-9, 30e-12)],
            50.,
            50.,
            0.5e-9,
            8e-9,
        );
        input["drivers"][0]["waveform"]["from"] = json!(1.);
        let report = run(input);
        assert!(report["quality"]["initialLumpedEnergyJ"].as_f64().unwrap() > 0.);
        assert!(report["quality"]["internalDissipatedJ"].as_f64().unwrap() > 0.);
        for row in report["portVoltages"].as_array().unwrap() {
            assert!(
                (row[1].as_f64().unwrap() - report["portVoltages"][0][1].as_f64().unwrap()).abs()
                    < 1e-10
            );
        }
    }
}
#[test]
fn mixed_rejects_unmapped_infinity_nonpassive_and_shared_budgets() {
    for mutation in 0..10 {
        let mut input = request(
            vec![line(2e-9, 1.), filter(50., 0., 30e-12)],
            50.,
            50.,
            0.5e-9,
            16e-9,
        );
        match mutation {
            0 => {
                input["topology"]["instances"][1]["model"]["data"]["referenceTopology"] =
                    json!("infinity")
            }
            1 => {
                input["topology"]["instances"][1]["model"]["data"]["resistanceOhm"] =
                    json!([[0, 0, -1.]])
            }
            2 => input["topology"]["connections"] = json!([[1, 2], [1, 3]]),
            3 => input["budget"] = json!({"maxMemoryMiB":1.}),
            4 => input["budget"] = json!({"maxSeconds":1e-15}),
            5 => input["initial"] = json!({"x":[1.]}),
            6 => input["topology"]["instances"][0]["model"]["data"]["delayS"] = json!(1e-30),
            7 => {
                input["topology"]["instances"][1]["model"]["validity"] =
                    json!({"status":"conditional","domain":{"bandHz":[0.,1e8]}})
            }
            8 => {
                input["topology"]["instances"][1]["model"]["analyses"] =
                    json!(["networkParameters"])
            }
            _ => input["numerics"] = json!({"maxIterations":1}),
        }
        assert!(
            coupled::time::solve(serde_json::from_value(input).unwrap()).is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn renaming_permutation_and_zero_delay_wave_reference_leave_physical_traces_invariant() {
    let base = request(
        vec![line(2e-9, 1.), filter(50., 0., 30e-12)],
        50.,
        50.,
        0.5e-9,
        16e-9,
    );
    let reference = run(base.clone());
    let mut permuted = base;
    permuted["topology"]["instances"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    permuted["topology"]["instances"][0]["ports"] = json!(["renamed-one", "renamed-two"]);
    permuted["topology"]["connections"] = json!([[3, 0]]);
    permuted["topology"]["ports"] =
        json!([{"name":"renamed-input","index":2},{"name":"renamed-output","index":1}]);
    let r = run(permuted);
    for (a, b) in reference["portVoltages"]
        .as_array()
        .unwrap()
        .iter()
        .zip(r["portVoltages"].as_array().unwrap())
    {
        for j in 0..2 {
            assert!((a[j].as_f64().unwrap() - b[j].as_f64().unwrap()).abs() < 1e-9);
        }
    }
    for (r1, r2) in [(25_f64, 100_f64), (120., 30.)] {
        let mut thru = line(0., 1.);
        thru["data"]["referenceOhm"] = json!({"a":r1,"b":r2});
        let diagonal = (r2 - r1) / (r1 + r2);
        let transmission = 2. * (r1 * r2).sqrt() / (r1 + r2);
        thru["data"]["scattering"] = json!([[diagonal, transmission], [transmission, -diagonal]]);
        let r = run(request(
            vec![line(2e-9, 1.), thru, filter(50., 0., 30e-12)],
            50.,
            50.,
            0.5e-9,
            16e-9,
        ));
        for (a, b) in reference["portVoltages"]
            .as_array()
            .unwrap()
            .iter()
            .zip(r["portVoltages"].as_array().unwrap())
        {
            for j in 0..2 {
                assert!((a[j].as_f64().unwrap() - b[j].as_f64().unwrap()).abs() < 0.0005);
            }
        }
    }
}
#[test]
fn sinusoid_matches_complex_rc_transfer_after_twenty_time_constants() {
    let mut input = request(
        vec![line(2e-9, 1.), filter(50., 0., 30e-12)],
        50.,
        50.,
        0.5e-9,
        80e-9,
    );
    input["drivers"][0]["waveform"] =
        json!({"kind":"sine","offset":0.,"amplitude":1.,"periodS":10e-9,"phaseDeg":0.});
    let r = run(input);
    let w = 2. * std::f64::consts::PI / 10e-9;
    let amplitude = 1. / (3. * (1. + (w * 1e-9).powi(2)).sqrt());
    let phase = -w * 2e-9 - (w * 1e-9).atan();
    let mut error = 0_f64;
    for (i, t) in r["axis"].as_array().unwrap().iter().enumerate() {
        let t = t.as_f64().unwrap();
        if t > 22e-9 {
            error = error.max(
                (r["portVoltages"][i][1].as_f64().unwrap() - amplitude * (w * t + phase).sin())
                    .abs(),
            );
        }
    }
    assert!(error < 0.001001, "settled sine reference {error}");
}
#[test]
fn sampled_rc_and_explicit_rc_have_the_same_delayed_reflections() {
    let frequencies: Vec<f64> = std::iter::once(0.)
        .chain((0..=160).map(|i| 10_f64.powf(-1. + i as f64 / 20.)))
        .collect();
    let sampled = json!({"id":"fit","kind":"network","ports":["p"],"analyses":["si"],"data":{"format":"network-parameters","representation":"Y","referenceTopology":"common","nature":"passive","frequenciesHz":frequencies,"parameters":frequencies.iter().map(|f|{let q=f/73.;vec![vec![json!({"real":0.002+0.01*q*q/(1.+q*q),"imaginary":0.01*q/(1.+q*q)})]]}).collect::<Vec<_>>(),"temporal":{"method":"passive-relaxation","normalizationOhm":50.,"highFrequency":{"conductanceS":[0.012],"capacitanceF":[0.]}}}});
    let explicit = json!({"id":"explicit","kind":"network","ports":["p"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["v","c"],"currentDofs":["dc","rc"],"incidence":[[0,0,1.],[0,1,1.],[1,1,-1.]],"portIncidence":[[0,0,1.]],"resistanceOhm":[[0,0,500.],[1,1,100.]],"inductanceH":[],"capacitanceF":[[1,1,1./(100.*2.*std::f64::consts::PI*73.)]]}});
    let make = |model: Value| json!({"topology":{"instances":[{"id":"line","model":line(0.002,1.),"ports":["a","b"]},{"id":"load","model":model,"ports":["p"]}],"connections":[[1,2]],"ports":[{"name":"in","index":0}]},"drivers":[{"port":0,"resistanceOhm":50.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":0.0001}}],"receivers":[],"time":{"stopS":0.02,"sampleStepS":0.00005}});
    let exact = run(make(explicit));
    let fitted = run(make(sampled));
    assert_eq!(fitted["conditional"], true);
    assert!(!fitted["fit"].as_array().unwrap().is_empty());
    let error = exact["portVoltages"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fitted["portVoltages"].as_array().unwrap())
        .map(|(a, b)| (a[0].as_f64().unwrap() - b[0].as_f64().unwrap()).abs())
        .fold(0_f64, f64::max);
    assert!(error < 0.001001, "separate fit/time error {error}");
}
#[test]
fn two_mutual_channels_follow_independent_even_odd_rlc_modes() {
    for sign in [1., -1.] {
        let mut delay = line(2e-9, 1.);
        delay["ports"] = json!(["a1", "b1", "a2", "b2"]);
        delay["data"]["scattering"] = json!([
            [0., 1., 0., 0.],
            [1., 0., 0., 0.],
            [0., 0., 0., 1.],
            [0., 0., 1., 0.]
        ]);
        let m = json!({"id":"mutual","kind":"network","ports":["a1","b1","a2","b2"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["a1","b1","a2","b2"],"currentDofs":["j1","j2"],"incidence":[[0,0,1.],[1,0,-1.],[2,1,1.],[3,1,-1.]],"portIncidence":[[0,0,1.],[1,1,1.],[2,2,1.],[3,3,1.]],"resistanceOhm":[[0,0,5.],[1,1,5.]],"inductanceH":[[0,0,50e-9],[0,1,10e-9],[1,0,10e-9],[1,1,50e-9]],"capacitanceF":[[1,1,20e-12],[1,3,-4e-12],[3,1,-4e-12],[3,3,20e-12]]}});
        let waveform = |amplitude: f64| json!({"kind":"step","from":0.,"to":amplitude,"atS":0.,"riseS":0.5e-9});
        let input = json!({"topology":{"instances":[{"id":"lines","model":delay,"ports":["i1","o1","i2","o2"]},{"id":"filter","model":m,"ports":["fi1","fo1","fi2","fo2"]}],"connections":[[1,4],[3,6]],"ports":[{"name":"i1","index":0},{"name":"o1","index":5},{"name":"i2","index":2},{"name":"o2","index":7}]},"drivers":[{"port":0,"resistanceOhm":50.,"waveform":waveform(1.)},{"port":2,"resistanceOhm":50.,"waveform":waveform(sign)}],"receivers":[{"port":1,"resistanceOhm":50.,"capacitanceF":0.},{"port":3,"resistanceOhm":50.,"capacitanceF":0.}],"time":{"stopS":20e-9,"sampleStepS":0.1e-9}});
        let report = run(input);
        let inductance = 50e-9 + sign * 10e-9;
        let capacitance = 20e-12 - sign * 4e-12;
        let a = inductance * 50. * capacitance;
        let b = inductance + 55. * 50. * capacitance;
        let alpha = b / (2. * a);
        let omega = (105. / a - alpha * alpha).sqrt();
        let step = |t: f64| {
            if t <= 0. {
                0.
            } else {
                50. / 105.
                    * (1.
                        - (-alpha * t).exp()
                            * ((omega * t).cos() + alpha / omega * (omega * t).sin()))
            }
        };
        for (i, t) in report["axis"].as_array().unwrap().iter().enumerate() {
            let t = t.as_f64().unwrap();
            let expected = (0..200)
                .map(|k| step(t - 2e-9 - (k as f64 + 0.5) * 0.5e-9 / 200.))
                .sum::<f64>()
                / 200.;
            assert!((report["portVoltages"][i][1].as_f64().unwrap() - expected).abs() < 0.001001);
            assert!(
                (report["portVoltages"][i][3].as_f64().unwrap() - sign * expected).abs() < 0.001001
            );
        }
    }
}
#[test]
fn assembled_zero_energy_gauge_does_not_erase_a_physical_charge_mode() {
    let mut model = filter(50., 0., 30e-12);
    model["data"]["potentialNodes"] = json!(["a", "b", "unused"]);
    model["data"]["conservedPotentialModes"] = json!([[0., 0., 1.]]);
    let report = run(request(
        vec![line(0., 1.), model.clone()],
        50.,
        50.,
        0.5e-9,
        8e-9,
    ));
    assert_eq!(report["history"]["zeroEnergyGauges"], 1);
    model["data"]["capacitanceF"]
        .as_array_mut()
        .unwrap()
        .push(json!([2, 2, 1e-12]));
    let input = request(vec![line(0., 1.), model], 50., 50., 0.5e-9, 8e-9);
    assert!(coupled::time::solve(serde_json::from_value(input).unwrap())
        .unwrap_err()
        .contains("Nonunique physical steady charge"));
}
#[test]
fn zero_stimulus_and_redundant_ideal_wave_loop_have_explicit_outcomes() {
    let mut zero = request(
        vec![line(2e-9, 1.), filter(50., 0., 30e-12)],
        50.,
        50.,
        0.5e-9,
        8e-9,
    );
    zero["drivers"][0]["waveform"]["to"] = json!(0.);
    let r = run(zero);
    assert_eq!(r["quality"]["energyNormalizationJ"], 0.);
    assert_eq!(r["quality"]["energyBalanceJ"], 0.);
    let mut cap = shunt(30e-12);
    cap["ports"] = json!(["a", "b", "drive"]);
    cap["data"]["portIncidence"] = json!([[0, 0, 1.], [0, 1, 1.], [0, 2, 1.]]);
    let input = json!({"topology":{"instances":[{"id":"thru","model":line(0.,1.),"ports":["l1","l2"]},{"id":"cap","model":cap,"ports":["a","b","drive"]}],"connections":[[0,2],[1,3]],"ports":[{"name":"drive","index":4}]},"drivers":[{"port":0,"resistanceOhm":50.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":0.5e-9}}],"receivers":[],"time":{"stopS":8e-9,"sampleStepS":0.1e-9}});
    assert!(coupled::time::solve(serde_json::from_value(input).unwrap()).is_err());
}
#[test]
fn mixed_frequency_all_reflections_and_transmissions_match_analytic_two_port() {
    use pcb_analyze::electrical::frequency;
    for (r, l, c, last_delay) in [(50., 0., 30e-12, 0.), (5., 50e-9, 20e-12, 3e-9)] {
        let mut models = vec![line(2e-9, 1.), filter(r, l, c)];
        if last_delay > 0. {
            models.push(line(last_delay, 1.));
        }
        let time_input = request(models, 50., 50., 0.5e-9, 25e-9);
        let mut input = time_input["topology"].clone();
        input["frequencies"] = json!([0., 1e8, 5e8]);
        input["representation"] = json!("S");
        input["referenceOhm"] = json!([50., 50.]);
        let report = frequency::run(serde_json::from_value(input.clone()).unwrap());
        assert_eq!(report["status"], "complete", "{report}");
        assert_eq!(
            report["provenance"][1]["resource"]["data"]["format"],
            "coupled-rlc"
        );
        for (i, f) in [0., 1e8, 5e8].into_iter().enumerate() {
            let w = 2. * std::f64::consts::PI * f;
            // ABCD of series(R+jwL) then shunt(jwC), references50 Ohm.
            let a = (1. - w * w * l * c, w * r * c);
            let b = (r / 50., w * l / 50.);
            let cc = (0., w * c * 50.);
            let den = (a.0 + b.0 + cc.0 + 1., a.1 + b.1 + cc.1);
            let divide = |x: (f64, f64)| {
                (
                    (x.0 * den.0 + x.1 * den.1) / (den.0 * den.0 + den.1 * den.1),
                    (x.1 * den.0 - x.0 * den.1) / (den.0 * den.0 + den.1 * den.1),
                )
            };
            let s21 = divide((2., 0.));
            let s11 = divide((a.0 + b.0 - cc.0 - 1., a.1 + b.1 - cc.1));
            let s22 = divide((-a.0 + b.0 - cc.0 + 1., -a.1 + b.1 - cc.1));
            for (out, input, z, delay) in [
                (0, 0, s11, 4e-9),
                (1, 1, s22, 2. * last_delay),
                (0, 1, s21, 2e-9 + last_delay),
                (1, 0, s21, 2e-9 + last_delay),
            ] {
                let phase = -w * delay;
                let expected = (
                    z.0 * phase.cos() - z.1 * phase.sin(),
                    z.0 * phase.sin() + z.1 * phase.cos(),
                );
                assert!(
                    (report["parameters"][i][out][input]["real"]
                        .as_f64()
                        .unwrap()
                        - expected.0)
                        .abs()
                        < 1e-8
                );
                assert!(
                    (report["parameters"][i][out][input]["imaginary"]
                        .as_f64()
                        .unwrap()
                        - expected.1)
                        .abs()
                        < 1e-8
                );
            }
        }
        if let Ok(dir) = std::env::var("P6_MIXED_EVIDENCE") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                format!(
                    "{dir}/frequency-{}.json",
                    if l == 0. { "rc" } else { "rlc" }
                ),
                serde_json::to_vec(&json!({"request":input,"report":report})).unwrap(),
            )
            .unwrap();
        }
    }
}
#[test]
fn finite_authored_values_cannot_return_nonfinite_storage_or_work() {
    for initial_rail in [false, true] {
        let mut input = request(
            vec![line(2e-9, 1.), filter(50., 0., 30e-12)],
            50.,
            50.,
            0.5e-9,
            8e-9,
        );
        input["drivers"][0]["waveform"]["to"] = json!(1e200);
        if initial_rail {
            input["drivers"][0]["waveform"]["from"] = json!(1e200);
        }
        assert!(coupled::time::solve(serde_json::from_value(input).unwrap()).is_err());
    }
}
