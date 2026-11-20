use super::*;
use std::f64::consts::PI;

fn c(re: f64, im: f64) -> C {
    C::new(re, im)
}
fn logspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n).map(|k| (lo.ln() + (hi.ln() - lo.ln()) * k as f64 / (n - 1) as f64).exp()).collect()
}
fn zero(n: usize) -> Matrix {
    super::super::frequency::math::zero(n, n)
}
/// Conjugate pair with residue matrix `r` (for the pole with positive imaginary part).
fn push_pair(m: &mut Model, p: C, r: Matrix) {
    m.poles.push(p);
    m.poles.push(p.conj());
    m.residues.push(r.iter().map(|row| row.iter().map(|z| z.conj()).collect()).collect());
    let last = m.residues.len() - 1;
    m.residues.insert(last, r);
}
fn request(kind: &str, f: &[f64], h: Vec<Matrix>, poles: usize) -> Request {
    Request {
        kind: kind.into(),
        frequencies_hz: f.to_vec(),
        samples: h,
        poles,
        iterations: Some(20),
        weights: None,
        fit_d: None,
        fit_e: Some(false),
        enforce_passivity: None,
        max_relative_error: None,
        budget: None,
    }
}

#[test]
fn exact_recovery_two_port_four_poles() {
    let mut model = Model { kind: "Y".into(), ports: 2, poles: vec![], residues: vec![], d: zero(2), e: zero(2) };
    let (w1, w2) = (2. * PI * 2e8, 2. * PI * 1e9);
    let mut r1 = zero(2);
    r1[0][0] = c(0.02 * w1, 0.01 * w1);
    r1[0][1] = c(0.005 * w1, -0.002 * w1);
    r1[1][0] = r1[0][1];
    r1[1][1] = c(0.03 * w1, -0.01 * w1);
    let mut r2 = zero(2);
    r2[0][0] = c(0.01 * w2, -0.004 * w2);
    r2[0][1] = c(-0.003 * w2, 0.002 * w2);
    r2[1][0] = r2[0][1];
    r2[1][1] = c(0.02 * w2, 0.006 * w2);
    push_pair(&mut model, c(-0.05 * w1, w1), r1);
    push_pair(&mut model, c(-0.1 * w2, w2), r2);
    model.d[0][0] = c(0.01, 0.);
    model.d[1][1] = c(0.02, 0.);
    model.d[0][1] = c(0.002, 0.);
    model.d[1][0] = c(0.002, 0.);
    let f = logspace(1e7, 3e9, 120);
    let h = evaluate(&model, &f).unwrap();
    let out = fit(request("Y", &f, h, 4)).unwrap();
    eprintln!("exact: max {:e} rms {:e} iters {}", out.max_relative_error, out.rms_relative_error, out.iterations);
    assert!(out.max_relative_error < 1e-10, "{}", out.max_relative_error);
    // Evaluate off the fitting points too.
    let g = logspace(2e7, 2.5e9, 37);
    let (a, b) = (evaluate(&model, &g).unwrap(), evaluate(&out.model, &g).unwrap());
    let worst = a.iter().zip(&b).map(|(x, y)| (0..2).flat_map(|i| (0..2).map(move |j| (x[i][j] - y[i][j]).abs())).fold(0., f64::max) / frob(x)).fold(0., f64::max);
    eprintln!("exact off-grid: {worst:e}");
    assert!(worst < 1e-10);
}

#[test]
fn parallel_rlc_one_port_z_held_out() {
    let (r, l, cap) = (50., 10e-9, 1e-12);
    let z = |f: f64| {
        let s = c(0., 2. * PI * f);
        // Z = 1 / (1/R + sC + 1/(sL))
        let y = c(1. / r, 0.) + s * c(cap, 0.) + inv(s * c(l, 0.));
        inv(y)
    };
    let f = logspace(1e8, 5e9, 161);
    let fit_f: Vec<f64> = f.iter().step_by(2).copied().collect();
    let held: Vec<f64> = f.iter().skip(1).step_by(2).copied().collect();
    let h: Vec<Matrix> = fit_f.iter().map(|&x| vec![vec![z(x)]]).collect();
    let out = fit(request("Z", &fit_f, h, 2)).unwrap();
    let pred = evaluate(&out.model, &held).unwrap();
    let worst = held.iter().zip(&pred).map(|(&x, p)| (p[0][0] - z(x)).abs() / z(x).abs()).fold(0., f64::max);
    eprintln!("rlc: fit max {:e}, held-out {:e}, passive {}", out.max_relative_error, worst, out.passive);
    assert!(worst < 1e-6);
    assert!(out.passive, "{:?}", out.passivity_reason);
}

fn nonpassive_y() -> (Model, Vec<f64>, Vec<Matrix>) {
    let w0 = 2. * PI * 1e9;
    let mut model = Model { kind: "Y".into(), ports: 1, poles: vec![], residues: vec![], d: zero(1), e: zero(1) };
    model.d[0][0] = c(0.02, 0.);
    model.e[0][0] = c(0.05 / w0, 0.);
    push_pair(&mut model, c(-0.1 * w0, w0), vec![vec![c(-0.00205 * w0, 0.)]]);
    let f = logspace(1e8, 3e9, 100);
    let h = evaluate(&model, &f).unwrap();
    (model, f, h)
}

#[test]
fn nonpassive_is_detected_and_enforced() {
    let (_, f, h) = nonpassive_y();
    let mut base = request("Y", &f, h.clone(), 2);
    base.fit_e = Some(true);
    let plain = fit(base).unwrap();
    eprintln!("plain: {:?} err {:e}", plain.passivity, plain.max_relative_error);
    assert!(!plain.passive);
    assert!(!plain.passivity.violations.is_empty());
    assert!(plain.passivity.worst_value < 0.);
    assert!(plain.enforcement.is_none());
    let mut req = request("Y", &f, h, 2);
    req.fit_e = Some(true);
    req.enforce_passivity = Some(true);
    let out = fit(req).unwrap();
    let e = out.enforcement.as_ref().unwrap();
    eprintln!(
        "enforce: worst before {:e}, passive {} in {} iter, err {:e} -> {:e}, reason {:?}",
        plain.passivity.worst_value, out.passive, e.iterations, e.max_relative_error_before, e.max_relative_error_after, e.reason
    );
    assert!(out.passive && e.reached, "{:?}", e.reason);
    assert!(out.passivity.violations.is_empty());
    assert!(e.max_relative_error_after < 0.05);
    assert!(e.max_relative_error_before < 1e-9);
}

#[test]
fn bad_input_is_refused() {
    let (_, f, h) = nonpassive_y();
    let code = |r: Request| fit(r).unwrap_err();
    assert!(code(request("X", &f, h.clone(), 2)).starts_with("RATIONAL_FIT_INPUT"));
    assert!(code(request("Y", &f, h.clone(), 0)).starts_with("RATIONAL_FIT_INPUT"));
    assert!(code(request("Y", &f[..5], h[..5].to_vec(), 4)).starts_with("RATIONAL_FIT_INPUT"));
    assert!(code(request("Y", &f[..50], h.clone(), 2)).starts_with("RATIONAL_FIT_INPUT"));
    let mut rev = f.clone();
    rev.swap(3, 4);
    assert!(code(request("Y", &rev, h.clone(), 2)).starts_with("RATIONAL_FIT_INPUT"));
    let mut nan = h.clone();
    nan[7][0][0] = c(f64::NAN, 0.);
    assert!(code(request("Y", &f, nan, 2)).starts_with("RATIONAL_FIT_INPUT"));
    let mut ragged = h.clone();
    ragged[9] = vec![vec![c(1., 0.); 2]; 2];
    assert!(code(request("Y", &f, ragged, 2)).starts_with("RATIONAL_FIT_INPUT"));
    let mut neg = f.clone();
    neg[0] = -1.;
    assert!(code(request("Y", &neg, h.clone(), 2)).starts_with("RATIONAL_FIT_INPUT"));
    let mut w = request("Y", &f, h.clone(), 2);
    w.weights = Some(vec![1.; 3]);
    assert!(code(w).starts_with("RATIONAL_FIT_INPUT"));
    let mut strict = request("Y", &f, h, 2);
    strict.max_relative_error = Some(1e-30);
    assert!(code(strict).starts_with("RATIONAL_FIT_NOT_CONVERGED"));
    assert!(fit_json(serde_json::json!({"kind": "Y", "bogus": 1})).unwrap_err().starts_with("RATIONAL_FIT_INPUT"));
}

#[test]
fn s_constant_thru_and_attenuator() {
    let f = logspace(1e6, 1e9, 40);
    for t in [1., 0.501] {
        let mut s = zero(2);
        s[0][1] = c(t, 0.);
        s[1][0] = c(t, 0.);
        let out = fit(request("S", &f, vec![s; f.len()], 2)).unwrap();
        eprintln!("S const t={t}: max {:e}, passive {}, sigma {:e}", out.max_relative_error, out.passive, out.passivity.worst_value);
        assert!(out.max_relative_error < 1e-9);
        assert!(out.passive, "{:?}", out.passivity_reason);
        assert!((out.model.d[0][1].real - t).abs() < 1e-9);
    }
}

#[test]
fn json_round_trip() {
    let (_, f, h) = nonpassive_y();
    let out = fit_json(serde_json::json!({
        "kind": "Y", "frequenciesHz": f, "samples": h, "poles": 2, "fitE": true
    }))
    .unwrap();
    let ev = evaluate_json(serde_json::json!({"model": out["model"], "frequenciesHz": [1e9]})).unwrap();
    assert_eq!(ev["samples"].as_array().unwrap().len(), 1);
}
