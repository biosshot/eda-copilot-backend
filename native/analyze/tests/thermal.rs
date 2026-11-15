use pcb_analyze::thermal::{run, Request};
use serde_json::{json, Value};
fn request() -> Value {
    json!({"contextId":"analytic","mode":"time","ambientC":25.,"initialC":"ambient","time":{"stopS":100.,"sampleStepS":0.5},"network":{"nodes":[{"id":"junction","heatCapacityJPerK":2.},{"id":"ambient","boundary":"environment"}],"links":[{"id":"ja","from":"junction","to":"ambient","cooling":{"resistanceKPerW":10.}}],"sources":[{"node":"junction","powerW":3.}]}})
}
fn solve(r: Value) -> Value {
    run(serde_json::from_value::<Request>(r).unwrap())
}
fn complete(r: &Value) {
    assert_eq!(r["status"], "complete", "{r}");
}
fn near(actual: f64, expected: f64, tol: f64) {
    assert!(
        (actual - expected).abs() <= tol,
        "{actual} != {expected}, tolerance {tol}"
    );
}
#[test]
fn global_balance_and_stiff_floor_cannot_hide_original_nodal_residual() {
    let make = |resistance: f64| json!({"contextId":"mixed-stiff-weak-original-residual","mode":"steady","ambientC":25.,"initialC":125.,"network":{"nodes":[{"id":"a"},{"id":"b"},{"id":"c"},{"id":"d"},{"id":"ambient","boundary":"environment"}],"links":[{"id":"ab","from":"a","to":"b","cooling":{"resistanceKPerW":resistance}},{"id":"ba","from":"b","to":"ambient","cooling":{"resistanceKPerW":100.}},{"id":"cd","from":"c","to":"d","cooling":{"resistanceKPerW":100.}},{"id":"da","from":"d","to":"ambient","cooling":{"resistanceKPerW":100.}}],"sources":[{"node":"a","powerW":1.},{"node":"c","powerW":1.}]}});
    let ill_conditioned = solve(make(1e-12));
    eprintln!("Mixed stiff/weak original-equation regression: {ill_conditioned}");
    complete(&ill_conditioned);
    near(
        ill_conditioned["temperaturesC"]["c"][0].as_f64().unwrap(),
        225.,
        1e-8,
    );
    near(
        ill_conditioned["heatFlowW"]["ab"][0].as_f64().unwrap(),
        1.,
        1e-8,
    );
    assert!(
        ill_conditioned["quality"]["originalNodalResidual"]
            .as_f64()
            .unwrap()
            <= 1e-8
    );
    let mut shifted_request = make(1e-12);
    shifted_request["ambientC"] = json!(1_000_025.);
    shifted_request["initialC"] = json!(1_000_125.);
    let shifted = solve(shifted_request);
    complete(&shifted);
    near(
        shifted["temperaturesC"]["c"][0].as_f64().unwrap(),
        1_000_225.,
        1e-8,
    );
    near(shifted["heatFlowW"]["ab"][0].as_f64().unwrap(), 1., 1e-8);
    let reference = solve(make(1e-3));
    complete(&reference);
    near(
        reference["temperaturesC"]["c"][0].as_f64().unwrap(),
        225.,
        1e-8,
    );
    near(
        reference["temperaturesC"]["d"][0].as_f64().unwrap(),
        125.,
        1e-8,
    );
    assert!(
        reference["quality"]["originalNodalResidual"]
            .as_f64()
            .unwrap()
            <= 1e-8
    );
    // Local imbalances [-1,+1,-1,+1] cancel globally at the initial state;
    // the old uncapped maxdiag floor incorrectly returned complete, c=125 C.
    assert!((125f64 - 225.).abs() == 100.);
}
#[test]
fn resin_healing_layer_full_p1_field_matches_hyperbolic_reference() {
    // Independent thin-sheet reaction/diffusion patch, SI throughout:
    // -kt*u'' + gv*u = 0, u(L)=0, Q=G*u(0)-kt*u'(0).
    // Explicit finite Robin contact avoids an artificial Dirichlet penalty.
    let k: f64 = 0.3;
    let thickness = 0.035e-3;
    let width = 1e-3;
    let core = 1.53e-3;
    let kt = k * thickness * width;
    let gv = width / (thickness / (2. * k) + core / (2. * k));
    let ell: f64 = (kt / gv).sqrt();
    let length = 0.9172439459396457e-3;
    let ambient = 77.09311763765075;
    let rise = 34.2316568349821;
    let contact_g = 0.01;
    let power = rise * (contact_g + kt / ell / (length / ell).tanh());
    let reference = |x: f64| rise * ((length - x) / ell).sinh() / (length / ell).sinh();
    let mut errors = vec![];
    for n in [1usize, 2, 4, 8, 16, 32] {
        let h = length / n as f64;
        let mut nodes = vec![json!({"id":"ambient","boundary":"environment"})];
        let mut links = vec![];
        for i in 0..n {
            nodes.push(json!({"id":format!("x{i}")}));
            links.push(json!({"id":format!("sheet{i}"),"from":format!("x{i}"),"to":if i+1==n{"ambient".into()}else{format!("x{}",i+1)},"cooling":{"resistanceKPerW":h/kt}}));
            links.push(json!({"id":format!("vertical{i}"),"from":format!("x{i}"),"to":"ambient","cooling":{"resistanceKPerW":1./(gv*h*if i==0{0.5}else{1.})}}));
        }
        links.push(json!({"id":"finite-contact","from":"x0","to":"ambient","cooling":{"resistanceKPerW":1./contact_g}}));
        let solved = solve(
            json!({"contextId":"healing-layer-reference","mode":"steady","ambientC":ambient,"network":{"nodes":nodes,"links":links,"sources":[{"node":"x0","powerW":power}]},"budget":{"maxMemoryMiB":16.,"maxSeconds":5.}}),
        );
        complete(&solved);
        let mut values: Vec<_> = (0..n)
            .map(|i| {
                solved["temperaturesC"][format!("x{i}")][0]
                    .as_f64()
                    .unwrap()
                    - ambient
            })
            .collect();
        values.push(0.);
        let mut error = 0f64;
        for sample in 0..=4096 {
            let x = length * sample as f64 / 4096.;
            let cell = (x / h).floor().min((n - 1) as f64) as usize;
            let t = (x / h - cell as f64).clamp(0., 1.);
            let actual = values[cell] * (1. - t) + values[cell + 1] * t;
            error = error.max((actual - reference(x)).abs() / rise);
        }
        eprintln!("Healing reference n={n}, hMm={}, ellMm={}, full P1 relative maximum error={error}, midpoint rise={}",h*1e3,ell*1e3,if n==1{values[0]/2.}else{values[n/2]});
        errors.push(error);
        assert!(solved["quality"]["linearResidual"].as_f64().unwrap() <= 1e-8);
        assert!(
            solved["quality"]["normalizedEnergyBalance"]
                .as_f64()
                .unwrap()
                <= 1e-6
        );
    }
    assert!(
        errors[0] > 0.2,
        "Unsplit long edge must expose the underresolved field"
    );
    assert!(errors.windows(2).all(|e| e[1] < e[0]));
    assert!(errors[4] < 0.02);
    assert!(errors[5] < 0.005);
}
#[test]
fn analytic_rc_and_temporal_refinement() {
    let errors: Vec<_> = [1e-5, 1e-6, 1e-7]
        .iter()
        .map(|tol| {
            let mut r = request();
            r["numerics"] = json!({"relativeTolerance":tol,"absoluteTolerance":1e-8});
            let a = solve(r);
            complete(&a);
            let axis = a["axis"].as_array().unwrap();
            let values = a["temperaturesC"]["junction"].as_array().unwrap();
            let error = axis
                .iter()
                .zip(values)
                .map(|(t, v)| {
                    let expected = 25. + 30. * (1. - (-t.as_f64().unwrap() / 20.).exp());
                    (v.as_f64().unwrap() - expected).abs()
                })
                .fold(0., f64::max);
            assert!(error < 0.03, "{error}");
            assert!(a["quality"]["normalizedEnergyBalance"].as_f64().unwrap() < 1e-9);
            error
        })
        .collect();
    assert!(errors[2] < errors[1] && errors[1] < errors[0], "{errors:?}");
    let mut r = request();
    r["mode"] = json!("steady");
    r.as_object_mut().unwrap().remove("time");
    r.as_object_mut().unwrap().remove("initialC");
    let a = solve(r);
    complete(&a);
    near(
        a["temperaturesC"]["junction"][0].as_f64().unwrap(),
        55.,
        1e-9,
    );
}
#[test]
fn two_node_modal_reference() {
    // C1=C2=1, both nodes to ambient G=1, mutual G=1. Independent modes
    // u=(T1+T2)/2=.5(1-exp(-t)); v=(T1-T2)/2=(1-exp(-3t))/6.
    let mut r = request();
    r["ambientC"] = json!(0.);
    r["time"] = json!({"stopS":5.,"sampleStepS":0.05});
    r["network"] = json!({"nodes":[{"id":"a","heatCapacityJPerK":1.},{"id":"b","heatCapacityJPerK":1.},{"id":"ambient","boundary":"environment"}],"links":[{"id":"ab","from":"a","to":"b","cooling":{"resistanceKPerW":1.}},{"id":"aa","from":"a","to":"ambient","cooling":{"resistanceKPerW":1.}},{"id":"ba","from":"b","to":"ambient","cooling":{"resistanceKPerW":1.}}],"sources":[{"node":"a","powerW":1.}]});
    let a = solve(r);
    complete(&a);
    for (i, t) in a["axis"].as_array().unwrap().iter().enumerate() {
        let t = t.as_f64().unwrap();
        let u = 0.5 * (1. - (-t).exp());
        let v = (1. - (-3. * t).exp()) / 6.;
        near(a["temperaturesC"]["a"][i].as_f64().unwrap(), u + v, 0.0005);
        near(a["temperaturesC"]["b"][i].as_f64().unwrap(), u - v, 0.0005);
    }
}
#[test]
fn events_pulse_ambient_and_cooling_switch() {
    let mut r = request();
    r["network"]["sources"][0]["powerW"] = json!({"kind":"pulse","low":0.,"high":3.,"delayS":0.,"riseS":0.,"highS":10.,"fallS":0.,"periodS":100.,"count":1});
    r["ambientC"] = json!({"kind":"step","from":25.,"to":35.,"atS":20.,"riseS":0.});
    let a = solve(r);
    complete(&a);
    for (i, t) in a["axis"].as_array().unwrap().iter().enumerate() {
        let t = t.as_f64().unwrap();
        let pulse = if t <= 10. {
            30. * (1. - (-t / 20.).exp())
        } else {
            30. * (1. - (-0.5f64).exp()) * (-(t - 10.) / 20.).exp()
        };
        let env = if t < 20. {
            0.
        } else {
            10. * (1. - (-(t - 20.) / 20.).exp())
        };
        near(
            a["temperaturesC"]["junction"][i].as_f64().unwrap(),
            25. + pulse + env,
            0.015,
        );
    }
    let mut r = request();
    r["network"]["links"][0]["cooling"]["scale"] =
        json!({"kind":"step","from":0.,"to":1.,"atS":10.,"riseS":0.});
    let a = solve(r);
    complete(&a);
    near(
        a["temperaturesC"]["junction"][20].as_f64().unwrap(),
        40.,
        1e-9,
    );
}
#[test]
fn table_nonlinear_known_steady_and_range_guard() {
    let mut r = request();
    r["mode"] = json!("steady");
    r.as_object_mut().unwrap().remove("time");
    r.as_object_mut().unwrap().remove("initialC");
    r["network"]["links"][0]["cooling"] = json!({"table":[{"deltaTemperatureK":0.,"heatFlowW":0.},{"deltaTemperatureK":10.,"heatFlowW":1.},{"deltaTemperatureK":20.,"heatFlowW":5.}]});
    let a = solve(r.clone());
    complete(&a);
    near(
        a["temperaturesC"]["junction"][0].as_f64().unwrap(),
        40.,
        1e-8,
    );
    r["network"]["sources"][0]["powerW"] = json!(6.);
    assert_eq!(solve(r)["status"], "rejected");
}
#[test]
fn continuation_compatibility_and_budgets_recovery() {
    let mut r = request();
    r["time"]["stopS"] = json!(50.);
    let first = solve(r.clone());
    complete(&first);
    assert_eq!(
        first["finalState"]["networkIdentity"]
            .as_str()
            .unwrap()
            .len(),
        "thermal-state-v1:sha256:".len() + 64
    );
    r.as_object_mut().unwrap().remove("initialC");
    r["initial"] = first["finalState"].clone();
    r["time"]["stopS"] = json!(100.);
    let continued = solve(r.clone());
    complete(&continued);
    for change in 0..4 {
        let mut changed = r.clone();
        match change {
            0 => changed["initial"]["temperatureOriginC"] = json!(26.),
            1 => {
                changed["initial"]["temperatureCoordinatesK"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            2 => changed["initial"]["temperatureCoordinatesK"][0] = json!(999.),
            _ => changed["initial"]["temperaturesC"][0] = json!(999.),
        }
        assert_eq!(
            solve(changed)["diagnostics"][0]["code"],
            "THERMAL_CONTINUATION"
        );
    }
    near(continued["axis"][0].as_f64().unwrap(), 50., 0.);
    near(
        continued["temperaturesC"]["junction"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_f64()
            .unwrap(),
        25. + 30. * (1. - (-5f64).exp()),
        0.02,
    );
    for change in 0..6 {
        let mut changed = r.clone();
        match change {
            0=>changed["network"]["links"][0]["cooling"]["resistanceKPerW"]=json!(20.),
            1=>changed["network"]["nodes"][0]["heatCapacityJPerK"]=json!(3.),
            2=>changed["network"]["links"].as_array_mut().unwrap().push(json!({"id":"additional","from":"junction","to":"ambient","cooling":{"resistanceKPerW":20.}})),
            3=>changed["network"]["sources"][0]["powerW"]=json!(4.),
            4=>changed["ambientC"]=json!(30.),
            _=>changed["contextId"]=json!("foreign-context"),
        }
        assert_eq!(
            solve(changed)["diagnostics"][0]["code"],
            "THERMAL_CONTINUATION"
        );
    }
    r["initial"]["networkIdentity"] = json!("foreign");
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_CONTINUATION");
    let mut r = request();
    r["budget"] = json!({"maxMemoryMiB":0.00001});
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_BUDGET");
    let mut r = request();
    r["numerics"] = json!({"maxIterations":1});
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_ITERATIONS");
    complete(&solve(request()));
}
#[test]
fn floating_missing_capacity_invalid_table_and_duplicate_power_reject() {
    let mut r = request();
    r["mode"] = json!("steady");
    r.as_object_mut().unwrap().remove("time");
    r.as_object_mut().unwrap().remove("initialC");
    r["network"]["links"] = json!([]);
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_BOUNDARY");
    let mut r = request();
    r["network"]["nodes"][0]
        .as_object_mut()
        .unwrap()
        .remove("heatCapacityJPerK");
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_CAPACITY");
    let mut r = request();
    r["network"]["links"][0]["cooling"] = json!({"table":[{"deltaTemperatureK":0.,"heatFlowW":0.},{"deltaTemperatureK":20.,"heatFlowW":-1.}]});
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_COOLING");
    let mut r = request();
    r["network"]["sources"]
        .as_array_mut()
        .unwrap()
        .push(json!({"node":"junction","powerW":1.}));
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_SOURCE");
    let mut r = request();
    r["network"]["links"] = json!([]);
    let a = solve(r);
    complete(&a);
    near(
        a["temperaturesC"]["junction"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_f64()
            .unwrap(),
        175.,
        1e-8,
    );
}

#[test]
fn zero_power_near_zero_balance_and_physical_profile_guards() {
    let mut r = request();
    r["mode"] = json!("steady");
    r.as_object_mut().unwrap().remove("time");
    r["initialC"] = json!(-20.);
    r["ambientC"] = json!(25.3);
    r["network"]["sources"][0]["powerW"] = json!(0.);
    let solved = solve(r);
    complete(&solved);
    near(
        solved["temperaturesC"]["junction"][0].as_f64().unwrap(),
        25.3,
        1e-10,
    );
    assert!(solved["energyBalance"][0].as_f64().unwrap().abs() < 1e-10);
    let mut r = request();
    r["initialC"] = json!(-274.);
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_INITIAL");
    let mut r = request();
    r["network"]["sources"][0]["powerW"] =
        json!({"kind":"sine","offset":0.,"amplitude":1.,"periodS":1.,"phaseDeg":0.});
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_SOURCE");
    let mut r = request();
    r["network"]["links"][0]["cooling"]["scale"] =
        json!({"kind":"sine","offset":0.,"amplitude":1.,"periodS":1.,"phaseDeg":0.});
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_COOLING");
}

#[test]
fn large_rc_ring_sparse_preconditioner_matches_uniform_exponential_mode() {
    let mut r = request();
    let n = 65;
    let mut nodes: Vec<Value> = (0..n)
        .map(|i| json!({"id":format!("j{i}"),"heatCapacityJPerK":2.}))
        .collect();
    nodes.push(json!({"id":"ambient","boundary":"environment"}));
    let links:Vec<Value>=(0..n).flat_map(|i|[json!({"id":format!("ambient-{i}"),"from":format!("j{i}"),"to":"ambient","cooling":{"resistanceKPerW":10.}}),json!({"id":format!("ring-{i}"),"from":format!("j{i}"),"to":format!("j{}",(i+1)%n),"cooling":{"resistanceKPerW":10.}})]).collect();
    let sources: Vec<Value> = (0..n)
        .map(|i| json!({"node":format!("j{i}"),"powerW":3.}))
        .collect();
    r["network"] = json!({"nodes":nodes,"links":links,"sources":sources});
    let solved = solve(r);
    complete(&solved);
    assert!(solved["counters"]["preconditionerBuilds"].as_u64().unwrap() > 0);
    assert_eq!(solved["counters"]["preconditionerFallbacks"], 0);
    for (k, t) in solved["axis"].as_array().unwrap().iter().enumerate() {
        let expected = 25. + 30. * (1. - (-t.as_f64().unwrap() / 20.).exp());
        for i in 0..n {
            near(
                solved["temperaturesC"][format!("j{i}")][k]
                    .as_f64()
                    .unwrap(),
                expected,
                0.03,
            );
        }
    }
    assert!(solved["quality"]["linearResidual"].as_f64().unwrap() <= 1e-8);
    assert!(
        solved["quality"]["normalizedEnergyBalance"]
            .as_f64()
            .unwrap()
            <= 1e-6
    );
}

#[test]
fn fixed_coordinate_origin_preserves_varying_ambient_nonlinear_cooling_energy_and_continuation() {
    let make = |offset: f64| {
        let mut r = request();
        r["contextId"] = json!("fixed-origin-time-reference");
        r["ambientC"] =
            json!({"kind":"step","from":offset+25.,"to":offset+35.,"atS":20.,"riseS":0.});
        r["network"]["links"][0]["cooling"] = json!({"table":[{"deltaTemperatureK":0.,"heatFlowW":0.},{"deltaTemperatureK":20.,"heatFlowW":2.},{"deltaTemperatureK":50.,"heatFlowW":8.}],"scale":{"kind":"step","from":0.,"to":1.,"atS":10.,"riseS":0.}});
        r["time"]["stopS"] = json!(60.);
        r
    };
    let base = solve(make(0.));
    let translated = solve(make(1_000_000.));
    complete(&base);
    complete(&translated);
    assert_eq!(
        translated["finalState"]["temperatureOriginC"],
        json!(1_000_025.)
    );
    for key in [
        "inputEnergyJ",
        "outputEnergyJ",
        "storedEnergyJ",
        "energyBalance",
    ] {
        for (a, b) in base[key]
            .as_array()
            .unwrap()
            .iter()
            .zip(translated[key].as_array().unwrap())
        {
            near(a.as_f64().unwrap(), b.as_f64().unwrap(), 1e-9);
        }
    }
    for (a, b) in base["heatFlowW"]["ja"]
        .as_array()
        .unwrap()
        .iter()
        .zip(translated["heatFlowW"]["ja"].as_array().unwrap())
    {
        near(a.as_f64().unwrap(), b.as_f64().unwrap(), 1e-9);
    }
    let mut first = make(1_000_000.);
    first["time"]["stopS"] = json!(30.);
    let first = solve(first);
    complete(&first);
    let mut resumed = make(1_000_000.);
    resumed.as_object_mut().unwrap().remove("initialC");
    resumed["initial"] = first["finalState"].clone();
    let resumed = solve(resumed);
    complete(&resumed);
    assert_eq!(
        resumed["finalState"]["temperatureOriginC"],
        first["finalState"]["temperatureOriginC"]
    );
    near(
        resumed["temperaturesC"]["junction"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_f64()
            .unwrap(),
        translated["temperaturesC"]["junction"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_f64()
            .unwrap(),
        0.001,
    );
    assert!(
        resumed["quality"]["originalNodalResidual"]
            .as_f64()
            .unwrap()
            <= 1e-8
    );
    assert!(
        resumed["quality"]["normalizedEnergyBalance"]
            .as_f64()
            .unwrap()
            <= 1e-6
    );
}
