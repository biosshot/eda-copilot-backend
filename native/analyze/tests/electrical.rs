use pcb_analyze::electrical::Waveform;
use pcb_analyze::electrical::{Request, Session};
use serde_json::{json, Value};

#[test]
fn periodic_boundaries_preserve_independent_left_right_values() {
    // Full duty cycle and a late repeat: no artificial gap before the rounded
    // end boundary, and no expansion of a hundred thousand periods.
    for count in [6, 100_000] {
        let pulse: Waveform = serde_json::from_value(json!({"kind":"pulse","low":0.,"high":1.,"delayS":0.03,"riseS":0.,"highS":0.1,"fallS":0.,"periodS":0.1,"count":count})).unwrap();
        pulse.validate().unwrap();
        let end: f64 = 0.03 + count as f64 * 0.1;
        near(pulse.sample(f64::from_bits(end.to_bits() - 1)), 1., 0.);
        near(pulse.sample(end), 0., 0.);
        near(pulse.sample(f64::from_bits(end.to_bits() + 1)), 0., 0.);
        near(pulse.next(end - 0.01), end, 0.);
    }
    for delay in [0., 0.03] {
        let pulse: Waveform = serde_json::from_value(json!({"kind":"pulse","low":0.,"high":1.,"delayS":delay,"riseS":0.,"highS":0.05,"fallS":0.,"periodS":0.1,"count":10})).unwrap();
        pulse.validate().unwrap();
        for k in [1, 3, 5, 6, 7, 9] {
            let boundary: f64 = delay + k as f64 * 0.1;
            near(pulse.sample(f64::from_bits(boundary.to_bits() - 1)), 0., 0.);
            near(pulse.sample(boundary), 1., 0.);
            near(pulse.sample(f64::from_bits(boundary.to_bits() + 1)), 1., 0.);
            let fall = boundary + 0.05;
            near(pulse.sample(f64::from_bits(fall.to_bits() - 1)), 1., 0.);
            near(pulse.sample(fall), 0., 0.);
            near(pulse.next(boundary - 0.01), boundary, 0.);
        }
        near(pulse.sample(delay + 1.), 0., 0.);
        near(
            pulse.sample(f64::from_bits((delay + 1.).to_bits() + 1)),
            0.,
            0.,
        );
    }
    for interpolation in ["hold", "linear"] {
        let pwl: Waveform = serde_json::from_value(json!({"kind":"pwl","points":[[0.,1.],[0.05,0.],[0.1,0.]],"interpolation":interpolation,"repeat":{"periodS":0.1,"count":10}})).unwrap();
        pwl.validate().unwrap();
        for k in [1, 3, 5, 6, 7, 9] {
            let boundary: f64 = k as f64 * 0.1;
            near(pwl.sample(f64::from_bits(boundary.to_bits() - 1)), 0., 0.);
            near(pwl.sample(boundary), 1., 0.);
            near(
                pwl.sample(f64::from_bits(boundary.to_bits() + 1)),
                1.,
                1e-13,
            );
            near(pwl.next(boundary - 0.01), boundary, 0.);
        }
        near(pwl.sample(1.), 0., 0.);
        near(pwl.sample(f64::from_bits(1f64.to_bits() + 1)), 0., 0.);
    }
}

#[test]
fn continuation_admission_counts_only_validated_new_interval() {
    let c = json!({"nodes":["gnd","p"],"reference":"gnd","ports":[],"elements":[{"id":"r","kind":"resistor","nodes":["p","gnd"],"resistanceOhm":1.},{"id":"i","kind":"current-source","nodes":["gnd","p"],"currentA":1.}]});
    let first = |s: &mut Session| {
        let mut r = request(c.clone(), "time");
        r["time"] = json!({"stopS":1.,"sampleStepS":1.});
        run(s, r)
    };
    let mut s = Session::default();
    let a = first(&mut s);
    let mut r = request(c.clone(), "time");
    r["initial"] = a["finalState"].clone();
    r["time"] = json!({"stopS":1.001,"sampleStepS":1e-6});
    let b = run(&mut s, r.clone());
    let axis = b["axis"].as_array().unwrap();
    assert_eq!(axis.len(), 1001);
    near(f(&axis[0]), 1., 0.);
    near(f(axis.last().unwrap()), 1.001, 0.);
    assert!(axis.windows(2).all(|w| f(&w[1]) > f(&w[0])));
    assert!(b["nodeVoltages"]["p"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| (f(v) - 1.).abs() < 1e-12));
    for case in ["large", "modified", "foreign", "end", "earlier"] {
        let a = first(&mut s);
        r["initial"] = a["finalState"].clone();
        r["time"] = json!({"stopS":1.001,"sampleStepS":1e-6});
        match case {
            "large" => r["time"]["stopS"] = json!(2.),
            "modified" => r["initial"]["timeS"] = json!(1.0009),
            "foreign" => r["initial"]["contextId"] = json!("other"),
            "end" => r["time"]["stopS"] = json!(1.),
            _ => r["time"]["stopS"] = json!(0.9),
        }
        let bad = s.solve(serde_json::from_value(r.clone()).unwrap());
        assert_eq!(bad["status"], "rejected");
        assert_eq!(
            bad["diagnostics"][0]["code"],
            if case == "large" {
                "NETWORK_BUDGET"
            } else {
                "NETWORK_STATE"
            }
        );
    }
}
fn circuit(elements: Value) -> Value {
    json!({"nodes":["gnd","p","q"],"reference":"gnd","ports":[{"id":"out","positive":"q","negative":"gnd"}],"elements":elements})
}
fn request(c: Value, mode: &str) -> Value {
    json!({"contextId":"test","circuit":c,"mode":mode,"frequencies":[1.,10.,100.,1000.],"time":{"stopS":0.005,"sampleStepS":0.00001},"initial":"steady"})
}
fn run(s: &mut Session, r: Value) -> Value {
    let report = s.solve(serde_json::from_value::<Request>(r).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    report
}
fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tol {tol})");
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn rc() -> Value {
    circuit(json!([
     {"id":"v","kind":"voltage-source","nodes":["p","gnd"],"voltageV":0.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":0.000001}},
     {"id":"r","kind":"resistor","nodes":["p","q"],"resistanceOhm":1000.},
     {"id":"c","kind":"capacitor","nodes":["q","gnd"],"capacitanceF":0.000001,"esrOhm":0.,"eslH":0.}
    ]))
}
#[test]
fn divider_sign_branch_dc_limits_and_rhs_reuse() {
    let c = circuit(
        json!([{"id":"v","kind":"voltage-source","nodes":["p","gnd"],"voltageV":10.},{"id":"r","kind":"resistor","nodes":["p","q"],"resistanceOhm":10.},{"id":"load","kind":"resistor","nodes":["q","gnd"],"resistanceOhm":10.}]),
    );
    let mut s = Session::default();
    let a = run(&mut s, request(c.clone(), "dc"));
    near(f(&a["nodeVoltages"]["q"]), 5., 1e-12);
    near(f(&a["currents"]["v"]), -0.5, 1e-12);
    near(f(&a["losses"]), 5., 1e-12);
    let mut b = request(c, "dc");
    b["circuit"]["elements"][0]["voltageV"] = json!(20.);
    let b = run(&mut s, b);
    assert_eq!(b["counters"]["reusedFactors"], 1);
    near(f(&b["nodeVoltages"]["q"]), 10., 1e-12);
    let c = circuit(
        json!([{"id":"v","kind":"voltage-source","nodes":["p","gnd"],"voltageV":3.3},{"id":"l","kind":"inductor","nodes":["p","q"],"inductanceH":1e-8,"seriesResistanceOhm":0.06},{"id":"i","kind":"current-source","nodes":["q","gnd"],"currentA":0.1},{"id":"c","kind":"capacitor","nodes":["q","gnd"],"capacitanceF":1e-4,"esrOhm":0.02,"eslH":1e-9}]),
    );
    let a = run(&mut s, request(c, "dc"));
    near(f(&a["nodeVoltages"]["q"]), 3.294, 1e-12);
    near(f(&a["currents"]["c"]), 0., 1e-12);
    near(f(&a["losses"]), 0.0006, 1e-12);
}
#[test]
fn ac_rc_rl_and_series_parallel_rlc() {
    for parallel in [false, true] {
        for kind in ["rc", "rl", "rlc"] {
            let mut elements = vec![
                json!({"id":"r","kind":"resistor","nodes":if parallel{["q","gnd"]}else{["q","p"]},"resistanceOhm":10.}),
            ];
            if kind != "rl" {
                elements.push(json!({"id":"c","kind":"capacitor","nodes":if parallel{["q","gnd"]}else{["p","gnd"]},"capacitanceF":1e-6,"esrOhm":if kind=="rlc"{0.1}else{0.},"eslH":if kind=="rlc"{1e-9}else{0.}}));
            }
            if kind == "rl" {
                elements.push(json!({"id":"l","kind":"inductor","nodes":if parallel{["q","gnd"]}else{["p","gnd"]},"inductanceH":1e-3,"seriesResistanceOhm":0.2}));
            }
            if kind == "rlc" {
                elements.push(json!({"id":"l","kind":"inductor","nodes":if parallel{["q","gnd"]}else{["gnd","p"]},"inductanceH":1e-3,"seriesResistanceOhm":0.2}));
            }
            // For a true series RLC use C ESL as the series L; parallel case uses separate L.
            if kind == "rlc" && !parallel {
                elements.pop();
                elements[1]["eslH"] = json!(0.001000001);
            }
            let mut c = circuit(json!(elements));
            if parallel {
                c["nodes"] = json!(["gnd", "q"]);
            }
            let a = run(&mut Session::default(), request(c, "frequency"));
            for (i, hz) in [1., 10., 100., 1000.].iter().enumerate() {
                let w = 2. * std::f64::consts::PI * hz;
                let cap = (
                    if kind == "rlc" { 0.1 } else { 0. },
                    if kind == "rlc" { w * 1e-9 } else { 0. } - 1. / (w * 1e-6),
                );
                let ind = (0.2, w * 1e-3);
                let inv = |(a, b): (f64, f64)| {
                    let d = a * a + b * b;
                    (a / d, -b / d)
                };
                let expected = if parallel {
                    let mut y = (0.1, 0.);
                    for z in [
                        if kind != "rl" { Some(cap) } else { None },
                        if kind != "rc" { Some(ind) } else { None },
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let z = inv(z);
                        y.0 += z.0;
                        y.1 += z.1;
                    }
                    inv(y)
                } else {
                    let z = if kind == "rl" { ind } else { cap };
                    (10. + z.0, z.1 + if kind == "rlc" { ind.1 } else { 0. })
                };
                let z = &a["impedances"]["out"][i];
                near(
                    f(&z["real"]),
                    expected.0,
                    expected.0.hypot(expected.1) * 1e-6 + 1e-10,
                );
                near(
                    f(&z["imaginary"]),
                    expected.1,
                    expected.0.hypot(expected.1) * 1e-6 + 1e-10,
                );
            }
        }
    }
}
#[test]
fn adaptive_rc_accuracy_output_independence_energy_and_continuation() {
    let mut r = request(rc(), "time");
    r["numerics"] = json!({"relativeTolerance":1e-6,"absoluteTolerance":1e-9});
    let a = run(&mut Session::default(), r.clone());
    let values = a["nodeVoltages"]["q"].as_array().unwrap();
    for (t, v) in a["axis"].as_array().unwrap().iter().zip(values) {
        let t = f(t);
        if t > 1e-6 {
            let expected = 1. - (1e-3 / 1e-6) * ((1e-6 / 1e-3f64).exp() - 1.) * (-t / 1e-3).exp();
            near(f(v), expected, 0.005);
        }
    }
    assert!(f(&a["quality"]["relativeEnergyError"]) < 0.005, "{a}");
    let mut coarse = r.clone();
    coarse["time"]["sampleStepS"] = json!(0.001);
    let b = run(&mut Session::default(), coarse);
    assert_eq!(a["counters"], b["counters"]);
    println!(
        "RC cadence counters identical: {}; energy error={}",
        a["counters"], a["quality"]["relativeEnergyError"]
    );
    near(
        f(values.last().unwrap()),
        f(b["nodeVoltages"]["q"].as_array().unwrap().last().unwrap()),
        1e-12,
    );
    let mut refined = r.clone();
    refined["numerics"]["relativeTolerance"] = json!(1e-7);
    let fine = run(&mut Session::default(), refined);
    assert!(f(&fine["quality"]["relativeEnergyError"]) < f(&a["quality"]["relativeEnergyError"]));
    let mut s = Session::default();
    let mut first = r.clone();
    first["time"]["stopS"] = json!(0.002);
    let first = run(&mut s, first);
    let mut second = r;
    second["initial"] = first["finalState"].clone();
    let second = run(&mut s, second);
    near(
        f(values.last().unwrap()),
        f(second["nodeVoltages"]["q"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()),
        0.0001,
    );
    near(f(&second["axis"][0]), 0.002, 1e-15);
}
#[test]
fn rl_step_and_damped_rlc_converge_to_independent_analytic_response() {
    for rlc in [false, true] {
        let mut elements = vec![
            json!({"id":"v","kind":"voltage-source","nodes":["p","gnd"],"voltageV":0.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":1e-6}}),
            json!({"id":"l","kind":"inductor","nodes":["p","q"],"inductanceH":0.001,"seriesResistanceOhm":1.}),
        ];
        elements.push(if rlc{json!({"id":"c","kind":"capacitor","nodes":["q","gnd"],"capacitanceF":1e-6,"esrOhm":0.,"eslH":0.})}else{json!({"id":"r","kind":"resistor","nodes":["q","gnd"],"resistanceOhm":1.})});
        let mut req = request(circuit(json!(elements)), "time");
        req["time"] = json!({"stopS":0.002,"sampleStepS":2e-6});
        req["numerics"] =
            json!({"relativeTolerance":1e-7,"absoluteTolerance":1e-10,"maxIterations":1000000});
        let a = run(&mut Session::default(), req.clone());
        let values = if rlc {
            &a["nodeVoltages"]["q"]
        } else {
            &a["currents"]["l"]
        };
        let mut max_error = 0f64;
        // Independent analytic finite-ramp response: average ideal step response
        // over the ramp interval, integrated with 200 midpoint samples.
        let step = |t: f64| {
            if t <= 0. {
                0.
            } else if rlc {
                let alpha = 500.;
                let omega = (1e9f64 - alpha * alpha).sqrt();
                1. - (-alpha * t).exp() * ((omega * t).cos() + alpha / omega * (omega * t).sin())
            } else {
                (1. - (-2000. * t).exp()) / 2.
            }
        };
        for (t, v) in a["axis"]
            .as_array()
            .unwrap()
            .iter()
            .zip(values.as_array().unwrap())
        {
            let expected = (0..200)
                .map(|i| step(f(t) - (i as f64 + 0.5) * 1e-6 / 200.))
                .sum::<f64>()
                / 200.;
            max_error = max_error.max((f(v) - expected).abs());
        }
        println!(
            "response rlc={rlc} max_abs_error={max_error} energy_error={} accepted_steps={}",
            a["quality"]["relativeEnergyError"], a["counters"]["acceptedSteps"]
        );
        assert!(max_error < 0.005, "RLC={rlc}, max error {max_error}");
        assert!(f(&a["quality"]["relativeEnergyError"]) < 0.005);
        req["numerics"]["relativeTolerance"] = json!(1e-8);
        let b = run(&mut Session::default(), req);
        let values = if rlc {
            &b["nodeVoltages"]["q"]
        } else {
            &b["currents"]["l"]
        };
        let refined_error = b["axis"]
            .as_array()
            .unwrap()
            .iter()
            .zip(values.as_array().unwrap())
            .map(|(t, v)| {
                let expected = (0..200)
                    .map(|i| step(f(t) - (i as f64 + 0.5) * 1e-6 / 200.))
                    .sum::<f64>()
                    / 200.;
                (f(v) - expected).abs()
            })
            .fold(0., f64::max);
        assert!(refined_error < max_error);
        println!("refined rlc={rlc} max_abs_error={refined_error}");
        println!(
            "refined rlc={rlc} energy_error={} accepted_steps={}",
            b["quality"]["relativeEnergyError"], b["counters"]["acceptedSteps"]
        );
        assert!(f(&b["quality"]["relativeEnergyError"]) < f(&a["quality"]["relativeEnergyError"]));
    }
}
#[test]
fn ac_and_sine_transient_agree_in_amplitude_phase() {
    let c = json!({"nodes":["gnd","q"],"reference":"gnd","ports":[{"id":"out","positive":"q","negative":"gnd"}],"elements":[{"id":"r","kind":"resistor","nodes":["q","gnd"],"resistanceOhm":10.},{"id":"c","kind":"capacitor","nodes":["q","gnd"],"capacitanceF":1e-6,"esrOhm":0.,"eslH":0.},{"id":"i","kind":"current-source","nodes":["gnd","q"],"currentA":0.,"waveform":{"kind":"sine","offset":0.,"amplitude":1.,"periodS":0.001,"phaseDeg":30.}}]});
    let mut req = request(c.clone(), "frequency");
    req["frequencies"] = json!([1000.]);
    let ac = run(&mut Session::default(), req);
    let z = &ac["impedances"]["out"][0];
    let mut req = request(c, "time");
    req["numerics"] = json!({"relativeTolerance":1e-7,"absoluteTolerance":1e-10});
    req["time"] = json!({"stopS":0.003,"sampleStepS":1e-6});
    let tr = run(&mut Session::default(), req);
    let mut max = 0f64;
    for (t, v) in tr["axis"]
        .as_array()
        .unwrap()
        .iter()
        .zip(tr["nodeVoltages"]["q"].as_array().unwrap())
    {
        let t = f(t);
        if t > 0.002 {
            let phase = 2. * std::f64::consts::PI * 1000. * t + std::f64::consts::PI / 6.;
            let expected = f(&z["real"]) * phase.sin() + f(&z["imaginary"]) * phase.cos();
            max = max.max((f(v) - expected).abs());
        }
    }
    println!(
        "AC/sine max_abs_error={max}, |Z|={}",
        f(&z["real"]).hypot(f(&z["imaginary"]))
    );
    assert!(
        max < 0.005 * f(&z["real"]).hypot(f(&z["imaginary"])),
        "{max}"
    );
}
#[test]
fn waveforms_breakpoints_absolute_values_and_repeat_end() {
    use pcb_analyze::electrical::Waveform;
    for (w, samples, next) in [
        (
            json!({"kind":"pulse","low":0.,"high":2.,"delayS":0.1,"riseS":0.01,"highS":0.01,"fallS":0.01,"periodS":0.05,"count":2}),
            vec![(0.105, 1.), (0.12, 2.), (0.155, 1.), (0.2, 0.)],
            0.11,
        ),
        (
            json!({"kind":"pwl","points":[[0.,0.],[0.01,1.],[0.02,0.]],"interpolation":"linear","repeat":{"periodS":0.02,"count":2}}),
            vec![(0.005, 0.5), (0.025, 0.5), (0.05, 0.)],
            0.11,
        ),
    ] {
        let w: Waveform = serde_json::from_value(w).unwrap();
        w.validate().unwrap();
        for (t, v) in samples {
            near(w.sample(t), v, 1e-12);
        }
        if next == 0.11 && matches!(w, Waveform::Pulse { .. }) {
            near(w.next(0.105), next, 1e-12);
        }
    }
    let mut req = request(rc(), "time");
    req["circuit"]["elements"][0]["voltageV"] = json!(42.);
    let a = run(&mut Session::default(), req);
    near(f(&a["nodeVoltages"]["p"][0]), 0., 1e-12);
    near(
        f(a["nodeVoltages"]["p"].as_array().unwrap().last().unwrap()),
        1.,
        1e-12,
    );
}
#[test]
fn rejects_modified_state_changed_parameters_context_impulse_and_nonfinite_recovery() {
    let mut s = Session::default();
    let req = request(rc(), "time");
    let a = run(&mut s, req.clone());
    for kind in ["parameter", "context", "state", "time"] {
        let mut q = req.clone();
        q["initial"] = a["finalState"].clone();
        q["time"]["stopS"] = json!(0.01);
        match kind {
            "parameter" => q["circuit"]["elements"][1]["resistanceOhm"] = json!(2000.),
            "context" => q["contextId"] = json!("foreign"),
            "state" => q["initial"]["x"][0] = json!(4.),
            _ => q["time"]["stopS"] = json!(0.001),
        }
        let r = s.solve(serde_json::from_value(q).unwrap());
        assert_eq!(r["status"], "rejected");
    }
    let mut q = req;
    q["circuit"]["elements"][0]["waveform"]["riseS"] = json!(0.);
    q["circuit"]["elements"][0]["waveform"]["atS"] = json!(0.0001);
    q["circuit"]["elements"].as_array_mut().unwrap().push(json!({"id":"impulse-cap","kind":"capacitor","nodes":["p","gnd"],"capacitanceF":1e-6,"esrOhm":0.,"eslH":0.}));
    let r = s.solve(serde_json::from_value(q).unwrap());
    assert_eq!(r["diagnostics"][0]["code"], "NETWORK_IMPULSE_UNSUPPORTED");
}
#[test]
fn singular_constraints_budgets_and_recovery() {
    let mut s = Session::default();
    let mut floating = request(rc(), "dc");
    floating["circuit"]["elements"] =
        json!([{"id":"r","kind":"resistor","nodes":["p","q"],"resistanceOhm":1.}]);
    let a = s.solve(serde_json::from_value(floating).unwrap());
    assert_eq!(a["status"], "rejected");
    let mut conflict = request(rc(), "dc");
    conflict["circuit"]["elements"][1] =
        json!({"id":"v2","kind":"voltage-source","nodes":["p","gnd"],"voltageV":2.});
    let a = s.solve(serde_json::from_value(conflict).unwrap());
    assert_eq!(a["status"], "rejected");
    let mut limited = request(rc(), "time");
    limited["numerics"] = json!({"maxIterations":1});
    let a = s.solve(serde_json::from_value(limited).unwrap());
    assert_eq!(a["status"], "rejected");
    let mut output = request(rc(), "time");
    output["time"]["sampleStepS"] = json!(1e-12);
    let a = s.solve(serde_json::from_value(output).unwrap());
    assert_eq!(a["status"], "rejected");
    let a = run(&mut s, request(rc(), "dc"));
    near(f(&a["nodeVoltages"]["q"]), 0., 1e-12);
    for case in ["memory", "frequencies", "port-solutions"] {
        let mut r = request(rc(), "frequency");
        match case {
            "memory" => r["budget"] = json!({"maxMemoryMiB":0.0001}),
            "frequencies" => r["frequencies"] = json!((1..=100001).collect::<Vec<_>>()),
            _ => {
                r["frequencies"] = json!((1..=50001).collect::<Vec<_>>());
                r["circuit"]["ports"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"other","positive":"p","negative":"gnd"}));
            }
        }
        let bad = s.solve(serde_json::from_value(r).unwrap());
        assert_eq!(bad["diagnostics"][0]["code"], "NETWORK_BUDGET", "{bad}");
    }
    run(&mut s, request(rc(), "dc"));
}
#[test]
fn branched_network_multiple_sources_and_rc_discharge() {
    let c = circuit(
        json!([{"id":"v","kind":"voltage-source","nodes":["p","gnd"],"voltageV":10.},{"id":"r1","kind":"resistor","nodes":["p","q"],"resistanceOhm":10.},{"id":"r2","kind":"resistor","nodes":["q","gnd"],"resistanceOhm":10.},{"id":"r3","kind":"resistor","nodes":["q","gnd"],"resistanceOhm":20.},{"id":"i","kind":"current-source","nodes":["gnd","q"],"currentA":0.5}]),
    );
    let a = run(&mut Session::default(), request(c, "dc"));
    near(f(&a["nodeVoltages"]["q"]), 6., 1e-12);
    near(f(&a["currents"]["v"]), -0.4, 1e-12);
    near(f(&a["losses"]), 7., 1e-12);
    let mut c = rc();
    c["elements"][0]["waveform"]["from"] = json!(1.);
    c["elements"][0]["waveform"]["to"] = json!(0.);
    let a = run(&mut Session::default(), request(c, "time"));
    near(f(&a["nodeVoltages"]["q"][0]), 1., 1e-12);
    for (t, v) in a["axis"]
        .as_array()
        .unwrap()
        .iter()
        .zip(a["nodeVoltages"]["q"].as_array().unwrap())
    {
        let at = f(t);
        let expected = if at == 0. {
            1.
        } else {
            1000. * (0.001f64).exp_m1() * (-at / 0.001).exp()
        };
        near(f(v), expected, 0.005);
    }
}
#[test]
fn discontinuous_current_and_repeated_pulse_pwl_do_not_smear_between_output_samples() {
    for waveform in [
        json!({"kind":"step","from":0.,"to":1.,"atS":0.000153,"riseS":0.}),
        json!({"kind":"pulse","low":0.,"high":1.,"delayS":0.000153,"riseS":0.000001,"highS":0.000002,"fallS":0.000001,"periodS":0.0002,"count":2}),
        json!({"kind":"pwl","points":[[0.,0.],[0.000153,1.],[0.0002,0.]],"interpolation":"hold","repeat":{"periodS":0.0002,"count":2}}),
    ] {
        let c = json!({"nodes":["q","gnd"],"reference":"gnd","ports":[{"id":"out","positive":"q","negative":"gnd"}],"elements":[{"id":"r","kind":"resistor","nodes":["q","gnd"],"resistanceOhm":1.},{"id":"i","kind":"current-source","nodes":["gnd","q"],"currentA":99.,"waveform":waveform}]});
        let mut req = request(c, "time");
        req["time"] = json!({"stopS":0.001,"sampleStepS":0.000001});
        let a = run(&mut Session::default(), req);
        let w: pcb_analyze::electrical::Waveform = serde_json::from_value(waveform).unwrap();
        for (t, v) in a["axis"]
            .as_array()
            .unwrap()
            .iter()
            .zip(a["nodeVoltages"]["q"].as_array().unwrap())
        {
            let t = f(t);
            if (t - 0.000153).abs() > 1e-15
                && (t - 0.000353).abs() > 1e-15
                && (t - 0.0002).abs() > 1e-15
                && (t - 0.0004).abs() > 1e-15
            {
                near(f(v), w.sample(t), 1e-8);
            }
        }
    }
}

#[test]
fn continuation_preserves_events_after_boundary_and_strict_output_axis() {
    let mut c = rc();
    c["elements"][0]["waveform"] = json!({"kind":"pulse","low":0.,"high":1.,"delayS":0.003,"riseS":1e-6,"highS":0.001,"fallS":1e-6,"periodS":0.005,"count":1});
    let mut req = request(c, "time");
    req["time"] = json!({"stopS":0.006,"sampleStepS":1e-5});
    let long = run(&mut Session::default(), req.clone());
    let mut s = Session::default();
    let mut first = req.clone();
    first["time"]["stopS"] = json!(0.002);
    let first = run(&mut s, first);
    req["initial"] = first["finalState"].clone();
    let second = run(&mut s, req);
    for window in second["axis"].as_array().unwrap().windows(2) {
        assert!(f(&window[1]) > f(&window[0]));
    }
    let a = long["nodeVoltages"]["q"].as_array().unwrap();
    let b = second["nodeVoltages"]["q"].as_array().unwrap();
    let err = b
        .iter()
        .enumerate()
        .map(|(i, v)| (f(v) - f(&a[i + 200])).abs())
        .fold(0., f64::max);
    assert!(err < 0.005, "{err}");
    println!("continuation event after boundary max_abs_error={err}");
    // The original duplicate-output counterexample (500 us continuation at 0.5 us cadence).
    let mut req = request(rc(), "time");
    req["time"] = json!({"stopS":0.0005,"sampleStepS":0.0000005});
    let mut s = Session::default();
    let first = run(&mut s, req.clone());
    req["initial"] = first["finalState"].clone();
    req["time"]["stopS"] = json!(0.001);
    let second = run(&mut s, req);
    assert_eq!(second["axis"].as_array().unwrap().len(), 1001);
    for w in second["axis"].as_array().unwrap().windows(2) {
        assert!(f(&w[1]) > f(&w[0]));
    }
}

#[test]
fn explicit_series_rlc_and_dependent_ideal_constraints() {
    let c = json!({"nodes":["gnd","q","p","m"],"reference":"gnd","ports":[{"id":"out","positive":"q","negative":"gnd"}],"elements":[{"id":"r","kind":"resistor","nodes":["q","p"],"resistanceOhm":10.},{"id":"l","kind":"inductor","nodes":["p","m"],"inductanceH":0.001,"seriesResistanceOhm":0.2},{"id":"c","kind":"capacitor","nodes":["m","gnd"],"capacitanceF":1e-6,"esrOhm":0.1,"eslH":1e-9}]});
    let mut req = request(c, "frequency");
    let f0 = 1. / (2. * std::f64::consts::PI * (0.001000001f64 * 1e-6).sqrt());
    req["frequencies"] = json!([100., f0, 10000.]);
    let a = run(&mut Session::default(), req);
    for (i, freq) in [100., f0, 10000.].iter().enumerate() {
        let w = 2. * std::f64::consts::PI * freq;
        let im = w * 0.001000001 - 1. / (w * 1e-6);
        near(f(&a["impedances"]["out"][i]["real"]), 10.3, 1e-8);
        near(
            f(&a["impedances"]["out"][i]["imaginary"]),
            im,
            im.abs() * 1e-6 + 1e-10,
        );
    }
    for equal in [false, true] {
        let c = json!({"nodes":["p","gnd"],"reference":"gnd","ports":[],"elements":[{"id":"v1","kind":"voltage-source","nodes":["p","gnd"],"voltageV":1.},{"id":"v2","kind":"voltage-source","nodes":["p","gnd"],"voltageV":if equal{1.}else{2.}}]});
        let a = Session::default().solve(serde_json::from_value(request(c, "dc")).unwrap());
        assert_eq!(a["status"], "rejected");
    }
    let c = json!({"nodes":["q","gnd"],"reference":"gnd","ports":[],"elements":[{"id":"l1","kind":"inductor","nodes":["q","gnd"],"inductanceH":0.001,"seriesResistanceOhm":0.},{"id":"l2","kind":"inductor","nodes":["q","gnd"],"inductanceH":0.002,"seriesResistanceOhm":0.},{"id":"c","kind":"capacitor","nodes":["q","gnd"],"capacitanceF":1e-6,"esrOhm":0.,"eslH":0.}]});
    let a = Session::default().solve(serde_json::from_value(request(c, "dc")).unwrap());
    assert_eq!(a["status"], "rejected");
}

#[test]
fn nonimpulsive_ideal_voltage_step_through_resistance_is_supported() {
    let mut c = rc();
    c["elements"][0]["waveform"]["riseS"] = json!(0.);
    c["elements"][0]["waveform"]["atS"] = json!(0.0001);
    let r = run(&mut Session::default(), request(c, "time"));
    let mut error = 0f64;
    for (t, v) in r["axis"]
        .as_array()
        .unwrap()
        .iter()
        .zip(r["nodeVoltages"]["q"].as_array().unwrap())
    {
        let t = f(t);
        let expected = if t <= 0.0001 {
            0.
        } else {
            1. - (-(t - 0.0001) / 0.001).exp()
        };
        error = error.max((f(v) - expected).abs());
    }
    assert!(error < 0.005);
    println!("ideal RC step max_abs_error={error}");
}

#[test]
fn integer_representations_survive_state_transport_but_changed_values_reject() {
    fn normalize(v: &mut Value) {
        match v {
            Value::Number(n) => {
                let x = n.as_f64().unwrap();
                if x.fract() == 0. && x.abs() < 1e15 {
                    *v = json!(x as i64);
                }
            }
            Value::Array(a) => a.iter_mut().for_each(normalize),
            Value::Object(o) => o.values_mut().for_each(normalize),
            _ => {}
        }
    }
    let mut c = rc();
    c["elements"][0]["voltageV"] = json!(20.);
    c["elements"][0]["waveform"] =
        json!({"kind":"step","from":20.,"to":21.,"atS":0.001,"riseS":1e-6});
    let req = request(c, "time");
    let mut s = Session::default();
    let a = run(&mut s, req.clone());
    let mut state = a["finalState"].clone();
    normalize(&mut state);
    let mut next = req;
    next["initial"] = state;
    next["time"]["stopS"] = json!(0.01);
    let a = run(&mut s, next.clone());
    next["initial"] = a["finalState"].clone();
    next["initial"]["x"][0] = json!(99);
    next["time"]["stopS"] = json!(0.02);
    let bad = s.solve(serde_json::from_value(next).unwrap());
    assert_eq!(bad["status"], "rejected");
}

#[test]
fn output_interval_cannot_hide_a_fast_rc_transition() {
    for resistance in [1.0, 0.001] {
        let mut c = rc();
        c["elements"][0]["waveform"] =
            json!({"kind":"step","from":0.,"to":1.,"atS":0.01,"riseS":0.});
        c["elements"][1]["resistanceOhm"] = json!(resistance);
        c["elements"][2]["capacitanceF"] = json!(1e-9);
        let mut req = request(c, "time");
        req["time"] = json!({"stopS":0.1,"sampleStepS":0.001});
        let r = Session::default().solve(serde_json::from_value(req).unwrap());
        if resistance == 1.0 {
            assert_eq!(r["status"], "complete", "{r}");
            assert!(f(&r["quality"]["relativeEnergyError"]) < 0.005);
            near(f(&r["quality"]["finalEnergyJ"]), 0.5e-9, 1e-12);
            near(f(&r["quality"]["dissipationJ"]), 0.5e-9, 0.005e-9);
            println!("fast 1 ns RC quality={}", r["quality"]);
        } else {
            // Preserve the original 1 ps / 1 kA counterexample: it formerly
            // completed with 50% energy error. F64 time/current accuracy cannot
            // meet these settings, so it must explicitly reject, never complete.
            assert_eq!(r["status"], "rejected");
            assert_eq!(r["diagnostics"][0]["code"], "NETWORK_STEP_UNDERFLOW");
            println!("unresolved 1 ps RC diagnostic={}", r["diagnostics"]);
        }
    }
}
