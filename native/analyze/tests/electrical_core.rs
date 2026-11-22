use pcb_analyze::electrical::{Request, Session};
use serde_json::{json, Value};
fn solve(r: Value) -> Value {
    Session::default().solve(serde_json::from_value::<Request>(r).unwrap())
}
fn rc() -> Value {
    json!({"contextId":"p6-explicit-ac","mode":"frequency","frequencies":[1000.,100000.,1000000.],
  "circuit":{"nodes":["gnd","in","out"],"reference":"gnd","ports":[{"id":"in","positive":"in","negative":"gnd"},{"id":"out","positive":"out","negative":"gnd"}],
   "elements":[{"id":"source","kind":"voltage-source","nodes":["in","gnd"],"voltageV":123.},
    {"id":"R","kind":"resistor","nodes":["in","out"],"resistanceOhm":1000.},
    {"id":"C","kind":"capacitor","nodes":["out","gnd"],"capacitanceF":1e-9,"esrOhm":0.,"eslH":0.}]},
  "acSources":[{"elementId":"source","values":[{"real":1.,"imaginary":0.5},{"real":1.,"imaginary":0.5},{"real":1.,"imaginary":0.5}]}]})
}
#[test]
fn prescribed_rc_peak_phasors_match_independent_complex_reference_and_keep_impedance_tests() {
    let r = rc();
    let output = solve(r.clone());
    assert_eq!(output["status"], "complete", "{output}");
    for (i, f) in [1000., 100000., 1000000.].iter().enumerate() {
        let w = 2. * std::f64::consts::PI * f;
        let q = w * 1000. * 1e-9;
        let den = 1. + q * q;
        let expected = [(1. + 0.5 * q) / den, (0.5 - q) / den];
        let actual = &output["nodeVoltages"]["out"][i];
        for (part, value) in ["real", "imaginary"].iter().zip(expected) {
            assert!((actual[part].as_f64().unwrap() - value).abs() < 1e-11);
        }
        let ir = (1. - expected[0]) / 1000.;
        let ii = (0.5 - expected[1]) / 1000.;
        assert!((output["currents"]["R"][i]["real"].as_f64().unwrap() - ir).abs() < 1e-12);
        assert!(
            (output["currents"]["C"][i]["imaginary"].as_f64().unwrap() - w * 1e-9 * expected[0])
                .abs()
                < 1e-12
        );
        assert!(
            (output["currents"]["source"][i]["imaginary"]
                .as_f64()
                .unwrap()
                + ii)
                .abs()
                < 1e-12
        );
    }
    let mut impedance = r;
    impedance.as_object_mut().unwrap().remove("acSources");
    let separate = solve(impedance);
    assert_eq!(output["impedances"], separate["impedances"]);
    assert!(separate.get("nodeVoltages").is_none());
    assert!(
        output["quality"]["relativeComplexPowerError"]
            .as_f64()
            .unwrap()
            < 1e-8
    );
    assert_eq!(output["counters"]["solves"], 9);
}
#[test]
fn current_phasor_sign_mapping_and_invalid_spectra() {
    let mut r = rc();
    r["circuit"]["elements"] = json!([{"id":"source","kind":"current-source","nodes":["gnd","out"],"currentA":999.},{"id":"R","kind":"resistor","nodes":["out","gnd"],"resistanceOhm":10.}]);
    r["circuit"]["nodes"] = json!(["gnd", "out"]);
    r["circuit"]["ports"] = json!([{"id":"out","positive":"out","negative":"gnd"}]);
    let v = solve(r.clone());
    assert_eq!(v["status"], "complete", "{v}");
    assert!((v["nodeVoltages"]["out"][0]["real"].as_f64().unwrap() - 10.).abs() < 1e-12);
    assert!((v["nodeVoltages"]["out"][0]["imaginary"].as_f64().unwrap() - 5.).abs() < 1e-12);
    for change in 0..4 {
        let mut bad = r.clone();
        match change {
            0 => bad["acSources"][0]["elementId"] = json!("foreign"),
            1 => bad["acSources"][0]["values"] = json!([]),
            2 => bad["mode"] = json!("dc"),
            _ => {
                let s = bad["acSources"][0].clone();
                bad["acSources"].as_array_mut().unwrap().push(s);
            }
        }
        assert_eq!(solve(bad)["diagnostics"][0]["code"], "NETWORK_AC_SOURCE");
    }
}
