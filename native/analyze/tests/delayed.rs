use pcb_analyze::electrical::{coupled, frequency};
use serde_json::{json, Value};
fn line(delay: f64, gain: f64) -> Value {
    json!({"id":"analytic-wave-resource","kind":"network","ports":["in","out"],"data":{"format":"delayed-scattering","referenceTopology":"common","nature":"passive","referenceOhm":50.,"scattering":[[0.,gain],[gain,0.]],"delayS":delay}})
}
fn request(rs: f64, rl: Option<f64>, from: f64) -> Value {
    json!({"resource":line(2e-9,1.),"drivers":[{"port":0,"resistanceOhm":rs,"waveform":{"kind":"step","from":from,"to":1.,"atS":0.,"riseS":1e-9}}],"receivers":rl.map(|r|vec![json!({"port":1,"resistanceOhm":r,"capacitanceF":0.})]).unwrap_or_default(),"time":{"stopS":30e-9,"sampleStepS":0.1e-9}})
}
fn ramp(t: f64) -> f64 {
    (t / 1e-9).clamp(0., 1.)
}
#[test]
fn exact_matched_delay_reflections_open_end_and_initial_travelling_history() {
    for (rs, rl) in [(50., Some(50.)), (25., Some(100.)), (25., None)] {
        let result =
            coupled::time::solve(serde_json::from_value(request(rs, rl, 0.)).unwrap()).unwrap();
        let gs = (rs - 50.) / (rs + 50.);
        let gl = rl.map(|r| (r - 50.) / (r + 50.)).unwrap_or(1.);
        let launch = 50. / (rs + 50.);
        let (mut maxin, mut maxout) = (0_f64, 0_f64);
        for (i, at) in result["axis"].as_array().unwrap().iter().enumerate() {
            let t = at.as_f64().unwrap();
            let mut expected_out = 0.;
            let mut expected_in = launch * ramp(t);
            for k in 0..20 {
                expected_out +=
                    launch * (1. + gl) * (gs * gl).powi(k) * ramp(t - (2 * k + 1) as f64 * 2e-9);
                expected_in += launch
                    * (1. + gs)
                    * gl
                    * (gs * gl).powi(k)
                    * ramp(t - 2. * (k + 1) as f64 * 2e-9);
            }
            maxin = maxin.max((result["portVoltages"][i][0].as_f64().unwrap() - expected_in).abs());
            maxout =
                maxout.max((result["portVoltages"][i][1].as_f64().unwrap() - expected_out).abs());
            if t < 2e-9 {
                assert!(result["portVoltages"][i][1].as_f64().unwrap().abs() < 1e-9);
            }
        }
        assert!(maxin <= 0.001 + 1e-6, "input bounce error {maxin}");
        assert!(maxout <= 0.001 + 1e-6, "output bounce error {maxout}");
        assert_eq!(result["conditional"], false);
        assert!(
            result["quality"]["internalDissipatedJ"]
                .as_f64()
                .unwrap()
                .abs()
                < 1e-24,
            "Lossless K must not add dissipation"
        );
        if rs == 50. && rl == Some(50.) {
            assert!(
                (result["quality"]["finalStoredEnergyJ"].as_f64().unwrap() - 1e-11).abs() < 1e-20
            );
            assert!(result["quality"]["energyBalanceJ"].as_f64().unwrap().abs() < 1e-20);
        }
    }
    let result =
        coupled::time::solve(serde_json::from_value(request(50., Some(50.), 1.)).unwrap()).unwrap();
    for row in result["portVoltages"].as_array().unwrap() {
        for i in 0..2 {
            assert!((row[i].as_f64().unwrap() - 0.5).abs() < 1e-12);
        }
    }
    assert!((result["quality"]["initialStoredEnergyJ"].as_f64().unwrap() - 1e-11).abs() < 1e-22);
}
#[test]
fn passive_attenuator_delay_cascade_agrees_in_frequency_and_time() {
    let topology = json!({"instances":[{"id":"pad","model":line(0.,0.5),"ports":["padin","padout"]},{"id":"line","model":line(2e-9,1.),"ports":["linein","lineout"]}],"connections":[[1,2]],"ports":[{"name":"input","index":0},{"name":"output","index":3}]});
    let mut input = request(50., Some(50.), 0.);
    input.as_object_mut().unwrap().remove("resource");
    input["topology"] = topology.clone();
    let result = coupled::time::solve(serde_json::from_value(input).unwrap()).unwrap();
    assert!(
        result["quality"]["internalDissipatedJ"].as_f64().unwrap() > 0.,
        "Attenuator loss must be separate from delay storage"
    );
    assert!(result["quality"]["finalStoredEnergyJ"].as_f64().unwrap() > 0.);
    for (i, at) in result["axis"].as_array().unwrap().iter().enumerate() {
        assert!(
            (result["portVoltages"][i][1].as_f64().unwrap()
                - 0.25 * ramp(at.as_f64().unwrap() - 2e-9))
            .abs()
                < 0.001 + 1e-6
        );
    }
    let mut freq = topology;
    freq["frequencies"] = json!([0., 1e7, 1e8]);
    freq["representation"] = json!("S");
    freq["referenceOhm"] = json!([50., 50.]);
    let report = frequency::run(serde_json::from_value(freq).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    for (i, f) in [0., 1e7, 1e8].into_iter().enumerate() {
        let phase = -2. * std::f64::consts::PI * f * 2e-9;
        for a in 0..2 {
            for b in 0..2 {
                let gain = if a == b { 0. } else { 0.5 };
                assert!(
                    (report["parameters"][i][a][b]["real"].as_f64().unwrap() - gain * phase.cos())
                        .abs()
                        < 1e-10
                );
                assert!(
                    (report["parameters"][i][a][b]["imaginary"].as_f64().unwrap()
                        - gain * phase.sin())
                    .abs()
                        < 1e-10
                );
            }
        }
    }
}
#[test]
fn active_delay_noncausal_initial_budget_and_incomplete_result_are_explicit() {
    for mutation in 0..5 {
        let mut input = request(50., Some(50.), 0.);
        match mutation {
            0 => input["resource"]["data"]["delayS"] = json!(-1.),
            1 => input["resource"]["data"]["scattering"] = json!([[0., 1.1], [1.1, 0.]]),
            2 => input["initial"] = json!({"x":[0.]}),
            3 => input["numerics"] = json!({"maxIterations":1}),
            _ => input["budget"] = json!({"maxMemoryMiB":1.}),
        };
        assert!(
            coupled::time::solve(serde_json::from_value(input).unwrap()).is_err(),
            "mutation{mutation}"
        );
    }
    assert!(
        coupled::time::solve(serde_json::from_value(request(50., Some(50.), 0.)).unwrap()).is_ok()
    );
}

#[test]
fn producer_bands_constrain_delay_sampled_frequency_and_temporal_fronts() {
    let mut resource = line(2e-9, 1.);
    resource["validity"] = json!({"status":"valid","domain":{"bandHz":[0.,1e11]}});
    let frequency_request = |resource: Value, f: f64| json!({"instances":[{"id":"first","model":resource,"ports":["a","b"]}],"connections":[],"ports":[{"name":"a","index":0},{"name":"b","index":1}],"frequencies":[f],"representation":"S","referenceOhm":[50.,50.]});
    let report =
        frequency::run(serde_json::from_value(frequency_request(resource.clone(), 1e9)).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    assert_eq!(report["bandHz"], json!([0., 1e11]));
    let report =
        frequency::run(serde_json::from_value(frequency_request(resource.clone(), 2e11)).unwrap());
    assert_eq!(report["diagnostics"][0]["code"], "NETWORK_BAND");
    let mut second = resource.clone();
    second["validity"]["domain"]["bandHz"] = json!([0., 5e10]);
    let mut cascade = frequency_request(resource.clone(), 1e9);
    cascade["instances"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second","model":second,"ports":["c","d"]}));
    cascade["connections"] = json!([[1, 2]]);
    cascade["ports"][1]["index"] = json!(3);
    let report = frequency::run(serde_json::from_value(cascade.clone()).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    assert_eq!(report["bandHz"], json!([0., 5e10]));
    cascade["frequencies"] = json!([6e10]);
    assert_eq!(
        frequency::run(serde_json::from_value(cascade).unwrap())["diagnostics"][0]["code"],
        "NETWORK_BAND"
    );
    let mut input = request(50., Some(50.), 0.);
    input["resource"] = resource;
    assert_eq!(
        coupled::time::solve(serde_json::from_value(input.clone()).unwrap()).unwrap()
            ["conditional"],
        true
    );
    for band in [json!([100., 1e11]), json!([0., 1e9])] {
        let mut bad = input.clone();
        bad["resource"]["validity"]["domain"]["bandHz"] = band;
        assert!(coupled::time::solve(serde_json::from_value(bad).unwrap())
            .unwrap_err()
            .starts_with("TEMPORAL_"));
    }
    let table = json!({"id":"sampled-y","kind":"network","ports":["p"],"analyses":["networkParameters"],"validity":{"status":"conditional","domain":{"bandHz":[10.,20.]}},"data":{"format":"network-parameters","representation":"Y","referenceTopology":"common","nature":"passive","frequenciesHz":[0.,10.,20.,30.],"parameters":[[[{"real":0.01,"imaginary":0.}]],[[{"real":0.01,"imaginary":0.}]],[[{"real":0.01,"imaginary":0.}]],[[{"real":0.01,"imaginary":0.}]]]}});
    let mut input = json!({"instances":[{"id":"table","model":table,"ports":["p"]}],"connections":[],"ports":[{"name":"p","index":0}],"frequencies":[15.],"representation":"Y"});
    let report = frequency::run(serde_json::from_value(input.clone()).unwrap());
    assert_eq!(report["status"], "complete", "{report}");
    assert_eq!(report["bandHz"], json!([10., 20.]));
    input["frequencies"] = json!([5.]);
    assert_eq!(
        frequency::run(serde_json::from_value(input).unwrap())["diagnostics"][0]["code"],
        "NETWORK_BAND"
    );
}
