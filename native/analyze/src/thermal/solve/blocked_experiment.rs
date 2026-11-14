//! Explicit bounded experiments; no new production/DSL selector.
use super::*;
use crate::thermal::precondition::blocked;
use serde_json::json;
fn output() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../debugging/thermal-verification/blocked-solve")
}
fn save(name: &str, v: &Value) {
    std::fs::create_dir_all(output()).unwrap();
    std::fs::write(output().join(name), serde_json::to_vec(v).unwrap()).unwrap();
}
fn coupon() -> Request {
    let mut v: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/thermal-preconditioner-coupon.json"
    )))
    .unwrap();
    v["board"]["boardJson"] = json!(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/thermal-preconditioner-coupon-board.json"
    )));
    let mut r: Request = serde_json::from_value(v).unwrap();
    r.capture_coordinates = true;
    r.observation_times = Some(vec![0., 5., 20., 60.]);
    r
}
#[test]
#[ignore = "One full coupon per variant only; process bounded384MiB/120s; unchanged authored128MiB/30s"]
fn full_coupon_variant() {
    let name = std::env::var("THERMAL_BLOCKED_VARIANT").unwrap();
    assert!(["baseline", "supernodal"].contains(&name.as_str()));
    let r = coupon();
    save("request.json", &serde_json::to_value(&r).unwrap());
    blocked::metrics_start();
    profile::start();
    let report = blocked::with_variant(name == "supernodal", || run(r));
    let phases = profile::finish();
    let details = blocked::metrics_finish();
    save(&format!("{name}.json"), &report);
    save(
        &format!("{name}-metrics.json"),
        &json!({"profile":phases,"details":details,"elapsedMs":report["elapsedMs"],"status":report["status"]}),
    );
    assert_eq!(report["status"], "complete");
    assert_eq!(report["axis"], json!([0., 5., 20., 60.]));
    assert!(report["quality"]["originalNodalResidual"].as_f64().unwrap() <= 1e-8);
    assert!(
        report["quality"]["maximumAcceptedLocalError"]
            .as_f64()
            .unwrap()
            <= 1.
    );
}
#[test]
#[ignore = "One faithful matrix/RHS capture with max3 short replay repetitions per variant; <=120s total"]
fn faithful_matrix_rhs_replay() {
    let start = Instant::now();
    let mut r = coupon();
    let board = r.board.take().unwrap();
    r.spatial =
        Some(board::prepare(&board, &mut r.network, true, start, r.budget.as_ref()).unwrap());
    let system = System::compile(&r, start).unwrap();
    let pattern = system.pattern.as_ref().unwrap().clone();
    let states: Vec<Vec<f64>> = (0..3)
        .map(|k| {
            (0..r.network.nodes.len())
                .map(|i| 0.1 * k as f64 + (i as f64 * 0.13).sin() * 0.01)
                .collect()
        })
        .collect();
    let mut cases = vec![];
    for (k, h) in [0.001, 0.01, 0.1, 1.].into_iter().enumerate() {
        let (rhs, diag, g, _, _) = system
            .residual(
                &states[k % 3],
                Some((&states[(k + 1) % 3], h)),
                [5., 20., 30., 60.][k],
            )
            .unwrap();
        let mut coefficients = g;
        coefficients.extend_from_slice(&system.preconditioner_extra);
        cases.push(json!({"dt":h,"diag":diag,"g":coefficients,"rhs":[rhs,system.free.iter().enumerate().map(|(i,_)|(i as f64*0.17).cos()).collect::<Vec<_>>()]}));
    }
    save(
        "matrix-rhs-sequence.json",
        &json!({"scope":"Exact unchanged faithful coupon System; deterministic captured four capacity/dt+tangent matrices and two RHS each, replayed identically. This is NOT a full coupon run.","network":r.network,"distributed":r.spatial.as_ref().unwrap().distributed,"cases":cases}),
    );
    let result = blocked::replay(pattern, &cases, start);
    save("replay.json", &result);
    assert!(start.elapsed().as_secs_f64() < 120.);
}
