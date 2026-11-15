//! One explicitly invoked test-only A/B; never changes production cache policy.
use super::*;
use serde_json::json;
#[test]
#[ignore = "One authorized bounded faithful coupon A/B only; not a controller feasibility certificate"]
fn single_slot_faithful_coupon_ab() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    let output = root.join("debugging/thermal-verification/single-slot-coupon-ab");
    std::fs::create_dir_all(&output).unwrap();
    let mut input: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/thermal-preconditioner-coupon.json"
    )))
    .unwrap();
    input["board"]["boardJson"] = json!(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/thermal-preconditioner-coupon-board.json"
    )));
    std::fs::write(
        output.join("request.json"),
        serde_json::to_vec_pretty(&input).unwrap(),
    )
    .unwrap();
    let mut reports = vec![];
    for single in [false, true] {
        super::solve::COUPON_SINGLE_SLOT.with(|v| v.set(single));
        let mut request: Request = serde_json::from_value(input.clone()).unwrap();
        request.capture_coordinates = true;
        request.observation_times = Some(vec![0., 5., 20., 60.]);
        let report = super::solve::run(request);
        super::solve::COUPON_SINGLE_SLOT.with(|v| v.set(false));
        let name = if single { "single-slot" } else { "baseline" };
        std::fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
        reports.push(report);
    }
    let a = &reports[0];
    let b = &reports[1];
    let mut checks = vec![];
    for (name, r) in [("baseline", a), ("single-slot", b)] {
        checks.push(json!({"name":name,"complete":r["status"]=="complete","elapsedMs":r["elapsedMs"],"quality":r["quality"],"counters":r["counters"],"memoryPlan":r["memoryPlan"],"experiment":r["couponPreconditionerExperiment"]}));
    }
    let mut max_abs = 0f64;
    let mut max_relative = 0f64;
    if a["status"] == "complete" && b["status"] == "complete" {
        assert_eq!(a["system"], b["system"]);
        assert_eq!(a["spatial"], b["spatial"]);
        assert_eq!(a["axis"], json!([0., 5., 20., 60.]));
        assert_eq!(a["axis"], b["axis"]);
        assert_eq!(
            a["finalState"]["networkIdentity"],
            b["finalState"]["networkIdentity"]
        );
        for (id, values) in a["temperaturesC"].as_object().unwrap() {
            for (j, v) in values.as_array().unwrap().iter().enumerate() {
                let va = v.as_f64().unwrap();
                let vb = b["temperaturesC"][id][j].as_f64().unwrap();
                let d = (va - vb).abs();
                max_abs = max_abs.max(d);
                max_relative = max_relative.max(d / 1f64.max((vb - 25.).abs()));
            }
        }
    }
    let summary = json!({"scope":"Test-only one A/B, same authored layered P1/copper strips/finite barrel/package/source/ambient and original operators; not fullboard or7-pass feasibility","checks":checks,"maximumTemperatureDifferenceK":max_abs,"maximumRelativeFieldDifference":max_relative,"rule":"Single admitted factor slot; reuse SPD across dt, refresh before next CG solve after >32 iterations or any8-iteration checkpoint without2x residual reduction; cooling tangent identity still required; no mid-CG replacement","productionCachePolicyChanged":false});
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    println!("THERMAL_COUPON_AB {summary}");
    for r in &reports {
        assert_eq!(r["status"], "complete", "{r}");
        assert!(r["quality"]["originalNodalResidual"].as_f64().unwrap() <= 1e-8);
        assert!(r["quality"]["normalizedEnergyBalance"].as_f64().unwrap() <= 1e-6);
        assert!(r["quality"]["maximumAcceptedLocalError"].as_f64().unwrap() <= 1. + 1e-12);
        assert!(r["memoryPlan"]["totalPlannedPeakBytes"].as_u64().unwrap() <= 128 * 1024 * 1024);
    }
    assert!(
        max_relative <= 0.001,
        "Unchanged complete-field temporal agreement gate"
    );
    assert_eq!(b["counters"]["preconditionerPlanning"]["cacheSlots"], 1);
}
