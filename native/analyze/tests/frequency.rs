use pcb_analyze::electrical::frequency::{run, Request};
use serde_json::{json, Value};
fn c(r: f64, i: f64) -> Value {
    json!({"real":r,"imaginary":i})
}
fn real(rows: &[&[f64]]) -> Value {
    json!(rows
        .iter()
        .map(|r| r.iter().map(|v| c(*v, 0.)).collect::<Vec<_>>())
        .collect::<Vec<_>>())
}
fn model(id: &str, rep: &str, m: Value, refs: Value, nature: &str) -> Value {
    let n = m.as_array().unwrap().len();
    let mut d = json!({"format":"network-parameters","representation":rep,"frequenciesHz":[0.,1e9,2e9],"parameters":[m,m,m],"referenceTopology":"common","nature":nature});
    if rep == "S" {
        d["referenceOhm"] = refs;
    }
    json!({"id":id,"kind":"network","ports":(0..n).map(|i|format!("p{i}")).collect::<Vec<_>>(),"data":d})
}
fn request(m: Value, rep: &str, refs: Value) -> Value {
    let n = m["ports"].as_array().unwrap().len();
    let ports = m["ports"].clone();
    let mut r = json!({"instances":[{"id":"one","model":m,"ports":ports}],"connections":[],"ports":(0..n).map(|i|json!({"name":format!("p{i}"),"index":i})).collect::<Vec<_>>(),"frequencies":[1e9],"representation":rep});
    if rep == "S" {
        r["referenceOhm"] = if refs.is_array() {
            refs
        } else {
            json!(vec![refs.as_f64().unwrap(); n])
        };
    }
    r
}
fn solve(r: Value) -> Value {
    run(serde_json::from_value::<Request>(r).unwrap())
}
fn ok(r: Value) -> Value {
    let v = solve(r);
    assert_eq!(v["status"], "complete", "{v}");
    v
}
fn rejects(r: Value, code: &str) {
    let v = solve(r);
    assert_eq!(v["status"], "rejected", "{v}");
    assert_eq!(v["diagnostics"][0]["code"], code, "{v}");
}
fn close(z: &Value, re: f64, im: f64) {
    let tol = 1e-10 + 1e-8 * re.hypot(im);
    assert!(
        (z["real"].as_f64().unwrap() - re).hypot(z["imaginary"].as_f64().unwrap() - im) <= tol,
        "{z} != {re}+j{im}"
    );
}
fn text_model(text: String, n: usize) -> Value {
    json!({"id":"file-model","kind":"network","ports":(0..n).map(|i|format!("p{i}")).collect::<Vec<_>>(),"data":{"format":"touchstone","content":text,"referenceTopology":"common","nature":"unspecified"}})
}
#[test]
fn touchstone_nonsymmetric_formats_orders_defaults_and_units() {
    for format in ["RI", "MA", "DB"] {
        for version in ["1.0", "21_12", "12_21"] {
            let entries = [(0.1, 0.2), (0.3, -0.4), (0.01, 0.02), (-0.2, 0.1)];
            let pair = |k: usize| {
                let (re, im): (f64, f64) = entries[k];
                if format == "RI" {
                    format!("{re} {im}")
                } else {
                    let mag = re.hypot(im);
                    format!(
                        "{} {}",
                        if format == "DB" {
                            20. * mag.log10()
                        } else {
                            mag
                        },
                        im.atan2(re).to_degrees()
                    )
                }
            };
            let order = if version == "12_21" {
                [0, 2, 1, 3]
            } else {
                [0, 1, 2, 3]
            };
            let values = order.map(pair).join(" ");
            let text = if version == "1.0" {
                format!("! asymmetric\n# MHz S {format} R 50\n1000 {values} ! end\n")
            } else {
                format!("[Version] 2.0\n# MHz S {format} R 50\n[Number of Ports] 2\n[Two-Port Data Order] {version}\n[Number of Frequencies] 1\n[Matrix Format] Full\n[Network Data]\n1000 {values}\n[End]\n")
            };
            let v = ok(request(text_model(text, 2), "S", json!(50.)));
            let m = &v["parameters"][0];
            close(&m[0][0], 0.1, 0.2);
            close(&m[1][0], 0.3, -0.4);
            close(&m[0][1], 0.01, 0.02);
            close(&m[1][1], -0.2, 0.1);
            assert_eq!(v["frequenciesHz"][0], 1e9);
        }
    }
    let v = ok(request(
        text_model("#\n1 0.5 90\n".into(), 1),
        "S",
        json!(50.),
    ));
    close(&v["parameters"][0][0][0], 0., 0.5);
    assert_eq!(
        v["provenance"][0]["normalization"]["coefficientFormat"],
        "MA"
    );
    for (unit, freq) in [("Hz", 1e9), ("kHz", 1e6), ("MHz", 1e3), ("GHz", 1.)] {
        ok(request(
            text_model(format!("# {unit} S RI\n{freq} .1 .2\n"), 1),
            "S",
            json!(50.),
        ));
    }
}
#[test]
fn touchstone_multiport_row_order_wrapping_and_references() {
    let text="[Version] 2.0\n# Hz S RI\n[Number of Ports] 3\n[Number of Frequencies] 1\n[Reference] 25 50\n100\n[Begin Information]\nignored vendor text\n[End Information]\n[Network Data]\n1000000000 .01 .001 .02 .002\n.03 .003 .04 .004 .05 .005 .06 .006\n.07 .007 .08 .008 .09 .009\n[End]\n";
    let mut r = request(text_model(text.into(), 3), "S", json!([25., 50., 100.]));
    let v = ok(r.clone());
    for i in 0..3 {
        for j in 0..3 {
            let k = (3 * i + j + 1) as f64;
            close(&v["parameters"][0][i][j], k * 0.01, k * 0.001);
        }
    }
    // Direct source reference metadata is preserved, independently of output refs.
    assert_eq!(
        v["provenance"][0]["sourceReferenceOhm"],
        json!([25., 50., 100.])
    );
    r["instances"][0]["model"]["data"]["content"]=json!("# Hz S RI\n1000000000 .01 .001 .02 .002 .03 .003\n.04 .004 .05 .005 .06 .006\n.07 .007 .08 .008 .09 .009\n");
    r["referenceOhm"] = json!([50., 50., 50.]);
    let v = ok(r);
    close(&v["parameters"][0][2][0], 0.07, 0.007);
}
#[test]
fn touchstone_strict_negative_and_early_limits() {
    let valid="[Version] 2.0\n# Hz S RI R 50\n[Number of Ports] 2\n[Number of Frequencies] 1\n[Two-Port Data Order] 21_12\n[Network Data]\n1000000000 .1 0 .3 0 .01 0 .2 0\n[End]\n";
    for text in [
        valid.replace("2.0", "2.1"),
        valid.replace("[Number of Ports] 2", "[Number of Ports] 3"),
        valid.replace("[Number of Frequencies] 1", "[Number of Frequencies] 2"),
        valid.replace("[Network Data]", "[Matrix Format] Lower\n[Network Data]"),
        valid.replace("[Network Data]", "[Mixed-Mode Order] D1,2\n[Network Data]"),
        valid.replace("[End]", "[Noise Data]"),
        valid.replace("[End]", ""),
        valid.replace(".01 0", "NaN 0"),
        valid.replace("1000000000", "-1"),
        valid.replace(
            "[Number of Frequencies] 1",
            "[Number of Frequencies] 1\n[Number of Frequencies] 1",
        ),
        valid.replace("[Two-Port Data Order] 21_12\n", ""),
        valid.replace("# Hz S RI R 50", "# Hz Z RI R 50"),
        valid.replace("[Network Data]", "[Unknown Physics] 1\n[Network Data]"),
    ] {
        rejects(
            request(text_model(text, 2), "S", json!(50.)),
            "TOUCHSTONE_INPUT",
        );
    }
    rejects(
        request(
            text_model(
                valid.replace(
                    "[Number of Frequencies] 1",
                    "[Number of Frequencies] 1000000000",
                ),
                2,
            ),
            "S",
            json!(50.),
        ),
        "NETWORK_BUDGET",
    );
    rejects(
        request(
            text_model("!".repeat(8 * 1024 * 1024 + 1), 1),
            "S",
            json!(50.),
        ),
        "NETWORK_BUDGET",
    );
    rejects(
        request(
            text_model("# Hz S RI\n1 .1 0\n1 .2 0\n".into(), 1),
            "S",
            json!(50.),
        ),
        "TOUCHSTONE_INPUT",
    );
}
#[test]
fn resistor_szy_and_unequal_references_have_independent_physical_oracles() {
    for resistance in [1., 25., 50., 100., 1000.] {
        for reference in [25., 50., 100.] {
            let m = model("R", "Z", real(&[&[resistance]]), Value::Null, "passive");
            let v = ok(request(m.clone(), "S", json!(reference)));
            close(
                &v["parameters"][0][0][0],
                (resistance - reference) / (resistance + reference),
                0.,
            );
            let v = ok(request(m.clone(), "Y", Value::Null));
            close(&v["parameters"][0][0][0], 1. / resistance, 0.);
            let v = ok(request(m, "Z", Value::Null));
            close(&v["parameters"][0][0][0], resistance, 0.);
        }
    }
    let m = model(
        "nonreciprocal Z",
        "Z",
        real(&[&[100., 20.], &[30., 75.]]),
        Value::Null,
        "active",
    );
    let v = ok(request(m, "S", json!([25., 100.])));
    let det = 125. * 175. - 20. * 30.;
    for j in 0..2 {
        let rhs = if j == 0 { [10., 0.] } else { [0., 20.] };
        let current = [
            (175. * rhs[0] - 20. * rhs[1]) / det,
            (-30. * rhs[0] + 125. * rhs[1]) / det,
        ];
        for i in 0..2 {
            let reference = if i == 0 { 25_f64 } else { 100. };
            let incident = if i == j { 1. } else { 0. };
            close(
                &v["parameters"][0][i][j],
                incident - current[i] * reference.sqrt(),
                0.,
            );
        }
    }
}
#[test]
fn rlc_complex_oracle_and_roundtrip() {
    for f in [1e3, 1e6, 1e9] {
        let w = std::f64::consts::TAU * f;
        let re = 0.1;
        let im = w * 10e-9 - 1. / (w * 100e-6);
        let z = json!([[c(re, im)]]);
        let mut m = model("series RLC", "Z", z, Value::Null, "passive");
        m["data"]["frequenciesHz"] = json!([f]);
        m["data"]["parameters"] = json!([[[c(re, im)]]]);
        for rep in ["S", "Y"] {
            let mut r = request(m.clone(), rep, json!(50.));
            r["frequencies"] = json!([f]);
            let v = ok(r);
            let den = if rep == "S" {
                (re + 50.).powi(2) + im * im
            } else {
                re * re + im * im
            };
            let (er, ei) = if rep == "S" {
                ((re * re - 2500. + im * im) / den, 100. * im / den)
            } else {
                (re / den, -im / den)
            };
            close(&v["parameters"][0][0][0], er, ei);
            let mut back = model(
                "roundtrip",
                rep,
                v["parameters"][0].clone(),
                json!(50.),
                "passive",
            );
            back["data"]["frequenciesHz"] = json!([1e9]);
            back["data"]["parameters"] = json!([v["parameters"][0]]);
            let b = ok(request(back, "Z", Value::Null));
            close(&b["parameters"][0][0][0], re, im);
        }
    }
}
#[test]
fn ideal_open_short_thru_and_singular_representations() {
    for (s, bad) in [(1., "Z"), (-1., "Y")] {
        let m = model("ideal", "S", real(&[&[s]]), json!(50.), "passive");
        let v = ok(request(m.clone(), "S", json!(75.)));
        close(&v["parameters"][0][0][0], s, 0.);
        rejects(request(m, bad, Value::Null), "NETWORK_SINGULAR");
    }
    let m = model(
        "thru",
        "S",
        real(&[&[0., 1.], &[1., 0.]]),
        json!(50.),
        "passive",
    );
    ok(request(m.clone(), "S", json!(50.)));
    rejects(request(m, "Z", Value::Null), "NETWORK_SINGULAR");
}
fn cascade(a: Value, b: Value) -> Value {
    json!({"instances":[{"id":"a","ports":a["ports"],"model":a},{"id":"b","ports":b["ports"],"model":b}],"connections":[[1,2]],"ports":[{"name":"input","index":0},{"name":"output","index":3}],"frequencies":[1e9],"representation":"S","referenceOhm":[50.,50.]})
}
#[test]
fn reflecting_cascade_full_matrix_and_thru() {
    let a = model(
        "a",
        "S",
        real(&[&[0.1, 0.3], &[0.4, 0.2]]),
        json!(50.),
        "passive",
    );
    let b = model(
        "b",
        "S",
        real(&[&[0.3, 0.2], &[0.5, -0.1]]),
        json!(50.),
        "passive",
    );
    let v = ok(cascade(a.clone(), b));
    let d = 1. - 0.2 * 0.3;
    for (i, j, expected) in [
        (0, 0, 0.1 + 0.3 * 0.3 * 0.4 / d),
        (0, 1, 0.3 * 0.2 / d),
        (1, 0, 0.5 * 0.4 / d),
        (1, 1, -0.1 + 0.5 * 0.2 * 0.2 / d),
    ] {
        close(&v["parameters"][0][i][j], expected, 0.);
    }
    let thru = model(
        "thru",
        "S",
        real(&[&[0., 1.], &[1., 0.]]),
        json!(50.),
        "passive",
    );
    let v = ok(cascade(a, thru));
    close(&v["parameters"][0][1][0], 0.4, 0.);
    let pad = model(
        "Attenuator Pad",
        "S",
        real(&[&[0., 0.5], &[0.5, 0.]]),
        json!(50.),
        "passive",
    );
    let v = ok(cascade(pad.clone(), pad));
    close(&v["parameters"][0][1][0], 0.25, 0.);
}
#[test]
fn active_nonreciprocal_and_collective_passivity() {
    let amp = model(
        "amp",
        "S",
        real(&[&[0.1, 0.01], &[2., 0.2]]),
        json!(50.),
        "active",
    );
    let v = ok(request(amp.clone(), "S", json!(50.)));
    close(&v["parameters"][0][1][0], 2., 0.);
    assert_eq!(v["nature"], "active");
    let mut wrong = amp;
    wrong["data"]["nature"] = json!("passive");
    rejects(request(wrong, "S", json!(50.)), "NETWORK_PASSIVITY");
    rejects(
        request(
            model(
                "collective gain",
                "S",
                real(&[&[0.8, 0.8], &[0.8, 0.8]]),
                json!(50.),
                "passive",
            ),
            "S",
            json!(50.),
        ),
        "NETWORK_PASSIVITY",
    );
    let v = ok(request(
        model("small", "S", real(&[&[1e-14]]), json!(50.), "unspecified"),
        "S",
        json!(50.),
    ));
    close(&v["parameters"][0][0][0], 1e-14, 0.);
    assert_eq!(v["nature"], "unspecified");
}
#[test]
fn multiport_with_explicit_load_and_topology_errors() {
    let tri = model(
        "coupled",
        "S",
        real(&[&[0.1, 0.2, 0.3], &[0.2, 0.1, 0.2], &[0.3, 0.2, 0.1]]),
        json!(50.),
        "passive",
    );
    let load = model("load", "Z", real(&[&[100.]]), Value::Null, "passive");
    let r = json!({"instances":[{"id":"tri","model":tri,"ports":["p0","p1","p2"]},{"id":"load","model":load,"ports":["p0"]}],"connections":[[2,3]],"ports":[{"name":"in","index":0},{"name":"out","index":1}],"frequencies":[1e9],"representation":"S","referenceOhm":[50.,50.]});
    let v = ok(r.clone());
    let gamma = 1. / 3.;
    let d = 1. - 0.1 * gamma;
    for (i, j, a, b, base) in [
        (0, 0, 0.3, 0.3, 0.1),
        (0, 1, 0.3, 0.2, 0.2),
        (1, 0, 0.2, 0.3, 0.2),
        (1, 1, 0.2, 0.2, 0.1),
    ] {
        close(&v["parameters"][0][i][j], base + a * gamma * b / d, 0.);
    }
    for pairs in [json!([]), json!([[2, 3], [2, 0]]), json!([[2, 2]])] {
        let mut bad = r.clone();
        bad["connections"] = pairs;
        rejects(bad, "NETWORK_TOPOLOGY");
    }
    let mut hidden = r;
    hidden["connections"] = json!([[1, 2]]);
    hidden["ports"] = json!([{"name":"load","index":3}]);
    hidden["referenceOhm"] = json!([50.]);
    hidden["instances"][1]["model"] = model(
        "isolated load",
        "Z",
        real(&[&[100.]]),
        Value::Null,
        "passive",
    );
    rejects(hidden, "NETWORK_TOPOLOGY");
}
#[test]
fn interpolation_source_coordinates_phase_wrap_single_point_and_band() {
    let mut m = model(
        "affine",
        "S",
        json!([[c(-0.5, 0.1)]]),
        json!(50.),
        "passive",
    );
    m["data"]["frequenciesHz"] = json!([0., 2e9]);
    m["data"]["parameters"] = json!([[[c(-0.5, 0.1)]], [[c(-0.5, -0.1)]]]);
    let v = ok(request(m.clone(), "S", json!(50.)));
    close(&v["parameters"][0][0][0], -0.5, 0.);
    assert_eq!(v["conditional"], true);
    assert_eq!(v["interpolation"][0]["intervalHz"], json!([0., 2e9]));
    let mut r = request(m, "S", json!(50.));
    r["frequencies"] = json!([0., 2e9]);
    assert_eq!(ok(r.clone())["conditional"], false);
    r["frequencies"] = json!([3e9]);
    rejects(r, "NETWORK_BAND");
    let mut one = model("single", "S", real(&[&[0.2]]), json!(50.), "passive");
    one["data"]["frequenciesHz"] = json!([1e9]);
    one["data"]["parameters"] = json!([real(&[&[0.2]])]);
    let mut r = request(one, "S", json!(50.));
    ok(r.clone());
    r["frequencies"] = json!([1.1e9]);
    rejects(r, "NETWORK_BAND");
}
#[test]
fn malformed_resources_budgets_and_recovery() {
    let m = model("r", "Z", real(&[&[100.]]), Value::Null, "passive");
    for field in ["nature", "referenceTopology", "parameters", "frequenciesHz"] {
        let mut x = m.clone();
        x["data"].as_object_mut().unwrap().remove(field);
        rejects(request(x, "S", json!(50.)), "NETWORK_INPUT");
    }
    let mut bad = m.clone();
    bad["data"]["referenceOhm"] = json!(50.);
    rejects(request(bad, "S", json!(50.)), "NETWORK_INPUT");
    let mut r = request(m.clone(), "S", json!(50.));
    r["budget"] = json!({"maxMemoryMiB":0.000001});
    rejects(r, "NETWORK_BUDGET");
    let mut r = request(m.clone(), "S", json!(50.));
    r["frequencies"] = json!(vec![1e9; 100001]);
    rejects(r, "NETWORK_BUDGET");
    ok(request(m, "S", json!(50.)));
}

#[test]
fn unequal_reference_connection_adds_no_resistance_and_feedback_singularity_is_explicit() {
    let mut thru = model(
        "physical thru, unequal wave references",
        "S",
        real(&[&[0.6, 0.8], &[0.8, -0.6]]),
        json!({"p0":25.,"p1":100.}),
        "passive",
    );
    let v = ok(request(thru.clone(), "S", json!(50.)));
    close(&v["parameters"][0][0][0], 0., 0.);
    close(&v["parameters"][0][1][0], 1., 0.);
    let pad = model(
        "pad",
        "S",
        real(&[&[0.1, 0.3], &[0.4, 0.2]]),
        json!(50.),
        "passive",
    );
    let v = ok(cascade(pad, thru.clone()));
    close(&v["parameters"][0][1][0], 0.4, 0.);
    close(&v["parameters"][0][0][0], 0.1, 0.);
    thru = model(
        "singular feedback",
        "S",
        real(&[&[0.1, 0.1, 0.1], &[0.1, 1., 0.], &[0.1, 0., 1.]]),
        json!(50.),
        "active",
    );
    let mut r = request(thru, "S", json!(50.));
    r["connections"] = json!([[1, 2]]);
    r["ports"] = json!([{"name":"external","index":0}]);
    r["referenceOhm"] = json!([50.]);
    rejects(r, "NETWORK_SINGULAR");
}

#[test]
fn extreme_gain_is_rejected_before_successful_null_coefficients() {
    let huge = model(
        "finite huge gain",
        "S",
        real(&[&[0., 0.], &[1e200, 0.]]),
        json!(50.),
        "active",
    );
    rejects(cascade(huge.clone(), huge), "NETWORK_SINGULAR");
}

#[test]
fn touchstone_line_positions_are_correct_for_lf_crlf_and_cr() {
    for eol in ["\n", "\r\n", "\r"] {
        let text = ["! comment", "# Hz S RI", "1000000000 .1 invalid"].join(eol);
        let v = solve(request(text_model(text, 1), "S", json!(50.)));
        assert_eq!(v["status"], "rejected");
        assert!(
            v["diagnostics"][0]["message"]
                .as_str()
                .unwrap()
                .contains("line 3:"),
            "{v}"
        );
        ok(request(
            text_model(["# Hz S RI", "1000000000 .1 .2"].join(eol), 1),
            "S",
            json!(50.),
        ));
    }
}

#[test]
fn signed_zero_dc_and_independent_return_groups_are_explicit() {
    let mut m = model("zero", "S", real(&[&[0.2]]), json!(50.), "passive");
    m["data"]["frequenciesHz"] = json!([-0.0]);
    m["data"]["parameters"] = json!([[[c(0.2, 0.)]]]);
    let mut r = request(m, "S", json!(50.));
    r["frequencies"] = json!([0.0]);
    close(&ok(r)["parameters"][0][0][0], 0.2, 0.);
    let m = model(
        "separate return",
        "Z",
        real(&[&[100.]]),
        Value::Null,
        "passive",
    );
    let r = json!({"instances":[{"id":"a","model":m,"ports":["p0"]},{"id":"b","model":m,"ports":["p0"]}],"connections":[],"ports":[{"name":"a","index":0},{"name":"b","index":1}],"frequencies":[1e9],"representation":"S","referenceOhm":[50.,50.]});
    rejects(r, "NETWORK_TOPOLOGY");
}
