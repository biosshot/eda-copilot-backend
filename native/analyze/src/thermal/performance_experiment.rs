//! Explicit bounded performance measurement. Not a physical acceptance gate.
use super::*;
use serde_json::json;
fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../debugging/thermal-verification")
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
fn instrumentation_preserves_coupon_fields_and_guards() {
    let a = run(coupon());
    profile::start();
    let b = run(coupon());
    let p = profile::finish();
    assert_eq!(a["status"], "complete");
    assert_eq!(b["status"], "complete");
    for key in [
        "axis",
        "temperaturesC",
        "heatFlowW",
        "inputPowerW",
        "outputPowerW",
        "inputEnergyJ",
        "outputEnergyJ",
        "storedEnergyJ",
        "energyBalance",
        "quality",
        "counters",
        "memoryPlan",
        "coordinateFrame",
    ] {
        assert_eq!(a[key], b[key], "Diagnostic timers must not change {key}");
    }
    let s = &p["stages"][0];
    assert_eq!(
        s["counters"]["acceptedSteps"],
        b["counters"]["acceptedSteps"]
    );
    assert_eq!(
        s["counters"]["rejectedSteps"],
        b["counters"]["rejectedSteps"]
    );
    assert_eq!(
        s["counters"]["numericBuilds"],
        b["counters"]["preconditionerBuilds"]
    );
    assert!(s["timings"]["preconditionerApply"]["ms"].as_f64().unwrap() > 0.);
    std::fs::write(root().join("performance-instrumentation-coupon.json"),serde_json::to_vec_pretty(&json!({"scope":"Same predeclared faithful coupon, instrumentation off/on; bitwise complete field/flow/energy/quality/counter/memory equality", "baselineMs":a["elapsedMs"],"profiledMs":b["elapsedMs"],"profile":p,"status":"pass"})).unwrap()).unwrap();
}
#[test]
#[ignore = "Exactly one approved TPS tighter diagnostic <=120s process, no complete-board acceptance"]
fn bounded_tps_tighter_profile() {
    let bytes = std::fs::read(root().join("performance-tps-tighter-request.json")).unwrap();
    let r: Request = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(r.budget.as_ref().unwrap().max_seconds, Some(110.));
    profile::start();
    let report = run(r);
    let p = profile::finish();
    let out = json!({"scope":"One bounded110s native TPS tighter diagnostic, <=120s process; rejected stays rejected. Scalar profile retained across native deadline, no partial fields.","status":report["status"],"diagnostics":report["diagnostics"],"elapsedMs":report["elapsedMs"],"profile":p});
    std::fs::write(
        root().join("performance-tps-tighter-profile.json"),
        serde_json::to_vec_pretty(&out).unwrap(),
    )
    .unwrap();
    assert_ne!(
        report["status"], "complete",
        "110s full-policy diagnostic must not be used as final-board acceptance"
    );
    println!(
        "THERMAL_PROFILE {}",
        json!({"status":out["status"],"elapsedMs":out["elapsedMs"]})
    );
}
