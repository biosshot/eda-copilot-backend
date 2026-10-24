use pcb_analyze::electrical::coupled;
use pcb_analyze::electrical::{frequency::Resource, relaxation};
use serde_json::{json, Value};
fn resource() -> Resource {
    let frequencies: Vec<f64> = std::iter::once(0.)
        .chain((0..=160).map(|i| 10_f64.powf(-1. + i as f64 / 20.)))
        .collect();
    let parameters: Vec<_> = frequencies
        .iter()
        .map(|f| {
            let w = 2. * std::f64::consts::PI * f;
            let mut re = 0.002;
            let mut im = w * 1e-12;
            for (hz, h) in [(73., 0.004), (13370., 0.006)] {
                let p = 2. * std::f64::consts::PI * hz;
                re += h * w * w / (p * p + w * w);
                im += h * w * p / (p * p + w * w);
            }
            vec![vec![json!({"real":re,"imaginary":im})]]
        })
        .collect();
    serde_json::from_value(json!({"id":"two-relaxations","kind":"network","ports":["p"],"source":"Independent two-series-RC admittance closed form","data":{"format":"network-parameters","representation":"Y","referenceTopology":"common","nature":"passive","frequenciesHz":frequencies,"parameters":parameters,"temporal":{"method":"passive-relaxation","normalizationOhm":50.,"highFrequency":{"conductanceS":[0.012],"capacitanceF":[1e-12]}}}})).unwrap()
}
#[test]
fn nonconstant_sampled_passive_fit_retains_dc_closure_passivity_and_holdout() {
    let r = resource();
    let fit = relaxation::fit(&r, None).unwrap();
    assert_eq!(fit.report["validity"]["status"], "conditional");
    assert!(fit.report["refinement"].as_array().unwrap().len() >= 2);
    let last = fit.report["refinement"].as_array().unwrap().last().unwrap();
    assert!(last["trainingMaximum"].as_f64().unwrap() <= 0.005 + 1e-9);
    assert!(last["holdoutMaximum"].as_f64().unwrap() <= 0.005 + 1e-9);
    assert!(last["orderMaximumChange"].as_f64().unwrap() <= 0.001);
    let coefficients = &fit.report["coefficients"][0];
    assert_eq!(coefficients["dcConductanceS"], 0.002);
    let sum: f64 = coefficients["conductanceStrengthsS"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let x = v.as_f64().unwrap();
            assert!(x >= 0.);
            x
        })
        .sum();
    assert!((sum - 0.01).abs() < 1e-10);
    assert!(fit.model.resistance_ohm.iter().all(|(_, _, v)| *v > 0.));
    assert!(fit.model.current_dofs.len() <= 65);
}

#[test]
fn si_only_resource_is_authorized_for_fit_without_frequency_permission() {
    let mut r = resource();
    r.analyses = Some(vec!["si".into()]);
    assert!(relaxation::fit(&r, None).is_ok());
    r.analyses = Some(vec!["networkParameters".into()]);
    assert!(relaxation::fit(&r, None).is_err());
}
#[test]
fn missing_dc_closure_band_noncausal_and_active_models_do_not_become_time_models() {
    for mutation in 0..5 {
        let mut r = resource();
        match mutation {
            0 => {
                r.data.as_object_mut().unwrap().remove("temporal");
            }
            1 => {
                r.data["frequenciesHz"].as_array_mut().unwrap().remove(0);
                r.data["parameters"].as_array_mut().unwrap().remove(0);
            }
            2 => r.data["temporal"]["highFrequency"]["conductanceS"] = json!([0.001]),
            3 => r.data["nature"] = json!("active"),
            _ => {
                let values = r.data["parameters"].as_array_mut().unwrap();
                for matrix in values {
                    let im = matrix[0][0]["imaginary"].as_f64().unwrap();
                    matrix[0][0]["imaginary"] = Value::from(-im);
                }
            }
        }
        assert!(relaxation::fit(&r, None).is_err(), "mutation {mutation}");
    }
}

#[test]
fn imported_nonconstant_rc_to_time_matches_independent_ramp_response_and_refines() {
    let mut r = resource();
    for (i, f) in r.data["frequenciesHz"]
        .as_array()
        .unwrap()
        .clone()
        .iter()
        .enumerate()
    {
        let q = f.as_f64().unwrap() / 73.;
        r.data["parameters"][i] =
            json!([[{"real":0.002+0.01*q*q/(1.+q*q),"imaginary":0.01*q/(1.+q*q)}]]);
    }
    r.data["temporal"]["highFrequency"]["capacitanceF"] = json!([0.]);
    let request = json!({"resource":r,"drivers":[{"port":0,"resistanceOhm":50.,"waveform":{"kind":"step","from":0.,"to":1.,"atS":0.,"riseS":0.0001}}],"receivers":[],"time":{"stopS":0.02,"sampleStepS":0.00001},"numerics":{"relativeTolerance":1e-5,"absoluteTolerance":1e-9}});
    let coarse = coupled::time::solve(serde_json::from_value(request.clone()).unwrap()).unwrap();
    let mut refined = request;
    refined["numerics"]["relativeTolerance"] = json!(2.5e-6);
    let fine = coupled::time::solve(serde_json::from_value(refined).unwrap()).unwrap();
    let kdc = 1. / 1.1;
    let kinf = 1. / 1.6;
    let tau = 1.6 / (2. * std::f64::consts::PI * 73. * 1.1);
    let rise = 0.0001;
    let (mut maximum, mut change) = (0_f64, 0_f64);
    for (i, at) in fine["axis"].as_array().unwrap().iter().enumerate() {
        let t = at.as_f64().unwrap();
        let expected = if t <= rise {
            kinf * t / rise + (kdc - kinf) * (t - tau * (1. - (-t / tau).exp())) / rise
        } else {
            kdc - (kdc - kinf) * tau / rise * (1. - (-rise / tau).exp()) * (-(t - rise) / tau).exp()
        };
        let v = fine["portVoltages"][i][0].as_f64().unwrap();
        maximum = maximum.max((v - expected).abs());
        change = change.max((v - coarse["portVoltages"][i][0].as_f64().unwrap()).abs());
    }
    assert!(
        maximum <= 0.001 + 1e-6,
        "model/time absolute error {maximum}"
    );
    assert!(change <= 0.0005, "independent time refinement {change}");
    assert_eq!(fine["conditional"], true);
    assert_eq!(fine["fit"]["validity"]["status"], "conditional");
}

// Independent two-by-two complex arithmetic: deliberately not the solver's
// conversion or fitting helpers. Physical Y is the reference for both S tables.
#[derive(Clone, Copy)]
struct Z(f64, f64);
impl Z {
    fn add(self, b: Self) -> Self {
        Self(self.0 + b.0, self.1 + b.1)
    }
    fn mul(self, b: Self) -> Self {
        Self(self.0 * b.0 - self.1 * b.1, self.0 * b.1 + self.1 * b.0)
    }
    fn scale(self, b: f64) -> Self {
        Self(self.0 * b, self.1 * b)
    }
    fn div(self, b: Self) -> Self {
        let d = b.0 * b.0 + b.1 * b.1;
        Self(
            (self.0 * b.0 + self.1 * b.1) / d,
            (self.1 * b.0 - self.0 * b.1) / d,
        )
    }
    fn value(self) -> Value {
        json!({"real":self.0,"imaginary":self.1})
    }
}
#[test]
fn even_odd_modal_fit_preserves_power_and_physical_response_under_distinct_zref() {
    let base = resource();
    let u = 1. / 2_f64.sqrt();
    let mut y = base.clone();
    y.ports = vec!["a".into(), "b".into()];
    y.data["temporal"]["modeTransform"] = json!([[u, u], [u, -u]]);
    y.data["temporal"]["normalizationOhm"] = json!({"a":50.,"b":75.});
    y.data["temporal"]["highFrequency"] =
        json!({"conductanceS":[0.012,0.024],"capacitanceF":[1e-12,2e-12]});
    let physical: Vec<[[Z; 2]; 2]> = base.data["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let z = Z(
                v[0][0]["real"].as_f64().unwrap(),
                v[0][0]["imaginary"].as_f64().unwrap(),
            );
            [[z.scale(1.5), z.scale(-0.5)], [z.scale(-0.5), z.scale(1.5)]]
        })
        .collect();
    y.data["parameters"] = json!(physical
        .iter()
        .map(|m| m
            .iter()
            .map(|r| r.iter().map(|v| v.value()).collect::<Vec<_>>())
            .collect::<Vec<_>>())
        .collect::<Vec<_>>());
    let yf = relaxation::fit(&y, None).unwrap();
    for references in [[25_f64, 120.], [200., 35.]] {
        let mut s = y.clone();
        s.data["representation"] = json!("S");
        s.data["referenceOhm"] = json!({"a":references[0],"b":references[1]});
        let samples: Vec<_> = physical
            .iter()
            .map(|m| {
                let h: [[Z; 2]; 2] = std::array::from_fn(|i| {
                    std::array::from_fn(|j| m[i][j].scale((references[i] * references[j]).sqrt()))
                });
                let a = h[0][0].add(Z(1., 0.));
                let d = h[1][1].add(Z(1., 0.));
                let determinant = a.mul(d).add(h[0][1].mul(h[1][0]).scale(-1.));
                let inverse = [
                    [d.div(determinant), h[0][1].scale(-1.).div(determinant)],
                    [h[1][0].scale(-1.).div(determinant), a.div(determinant)],
                ];
                (0..2)
                    .map(|i| {
                        (0..2)
                            .map(|j| {
                                (0..2)
                                    .fold(Z(0., 0.), |sum, k| {
                                        sum.add(
                                            Z(if i == k { 1. } else { 0. }, 0.)
                                                .add(h[i][k].scale(-1.))
                                                .mul(inverse[k][j]),
                                        )
                                    })
                                    .value()
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        s.data["parameters"] = json!(samples);
        let sf = relaxation::fit(&s, None).unwrap();
        for f in 0..physical.len() {
            for i in 0..2 {
                for j in 0..2 {
                    for field in ["real", "imaginary"] {
                        assert!(
                            (sf.report["fittedAdmittanceS"][f][i][j][field]
                                .as_f64()
                                .unwrap()
                                - yf.report["fittedAdmittanceS"][f][i][j][field]
                                    .as_f64()
                                    .unwrap())
                            .abs()
                                < 1e-8
                        );
                    }
                }
            }
        }
    }
    let voltage = [1.2, -0.4];
    let current = [0.2, 0.7];
    let vm = [u * (voltage[0] + voltage[1]), u * (voltage[0] - voltage[1])];
    let im = [u * (current[0] + current[1]), u * (current[0] - current[1])];
    assert!(
        (voltage[0] * current[0] + voltage[1] * current[1] - vm[0] * im[0] - vm[1] * im[1]).abs()
            < 1e-15
    );
    // Same samples may hide a resonance between nodes. Neither holdout nor a
    // regular grid authorizes changing the finite-data provenance to valid.
    assert_eq!(yf.report["validity"]["status"], "conditional");
    assert!(yf.report["sourceLimitations"]
        .as_str()
        .unwrap()
        .contains("unseen narrow resonances"));
}

#[test]
fn unresolved_passive_narrow_resonance_keeps_conditionality_and_resolved_peak_rejects() {
    let mut r = resource();
    let at = 83;
    let left = r.data["frequenciesHz"][at].as_f64().unwrap();
    let right = r.data["frequenciesHz"][at + 1].as_f64().unwrap();
    let center = (left * right).sqrt();
    let w0 = 2. * std::f64::consts::PI * center;
    let gamma = w0 * 1e-8;
    let h = 0.1;
    // Passive series-RLC admittance h*gamma*s/(s²+gamma*s+w0²).
    // It vanishes at DC/infinity but has real peak h at the unsampled center.
    let resonance = |f: f64| {
        let w = 2. * std::f64::consts::PI * f;
        let a = w0 * w0 - w * w;
        let b = gamma * w;
        let denominator = a * a + b * b;
        (h * b * b / denominator, h * b * a / denominator)
    };
    let mut maximum_sample_change = 0_f64;
    for (i, f) in r.data["frequenciesHz"]
        .as_array()
        .unwrap()
        .clone()
        .iter()
        .enumerate()
    {
        let (re, im) = resonance(f.as_f64().unwrap());
        maximum_sample_change = maximum_sample_change.max(re.hypot(im) * 50.);
        r.data["parameters"][i][0][0]["real"] =
            json!(r.data["parameters"][i][0][0]["real"].as_f64().unwrap() + re);
        r.data["parameters"][i][0][0]["imaginary"] =
            json!(r.data["parameters"][i][0][0]["imaginary"].as_f64().unwrap() + im);
    }
    assert!(maximum_sample_change < 1e-5);
    assert!((resonance(center).0 - 0.1).abs() < 1e-12);
    let fit = relaxation::fit(&r, None).unwrap();
    assert_eq!(fit.report["validity"]["status"], "conditional");
    assert!(fit.report["sourceLimitations"]
        .as_str()
        .unwrap()
        .contains("unseen narrow resonances"));
    let w = 2. * std::f64::consts::PI * center;
    let mut re = 0.002;
    let mut im = w * 1e-12;
    for (hz, h) in [(73., 0.004), (13370., 0.006)] {
        let p = 2. * std::f64::consts::PI * hz;
        re += h * w * w / (p * p + w * w);
        im += h * w * p / (p * p + w * w);
    }
    let peak = resonance(center);
    r.data["frequenciesHz"]
        .as_array_mut()
        .unwrap()
        .insert(at + 1, json!(center));
    r.data["parameters"]
        .as_array_mut()
        .unwrap()
        .insert(at + 1, json!([[{"real":re+peak.0,"imaginary":im+peak.1}]]));
    assert!(relaxation::fit(&r, None).is_err());
}
