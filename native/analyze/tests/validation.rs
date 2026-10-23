use pcb_analyze::validation::validate_json;
use serde_json::{json, Value};
fn board() -> Value {
    let mut b: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-dc-coupon.json"
    ))
    .unwrap();
    b["pads"] = json!([]);
    b["components"] = json!([]);
    b["copper"] = json!({"fixed":{"tracks":[],"vias":[],"zones":[]},"editable":{"tracks":[],"vias":[],"zones":[]}});
    b
}
fn pad(id: &str, x: f64, y: f64, net: Option<&str>) -> Value {
    let mut p = json!({"id":id,"number":"1","at":{"x":x,"y":y},"rotationDeg":0,"layers":["TOP"],"shape":{"kind":"rect","widthMm":1.,"heightMm":1.}});
    if let Some(n) = net {
        p["net"] = json!(n);
    }
    p
}
fn run(b: &Value, rules: Value) -> Value {
    validate_json(json!({"board_json":b.to_string(),"rules":rules}))
}
fn rows(r: &Value) -> &Vec<Value> {
    r["measurements"].as_array().expect(&format!("{r}"))
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}
#[test]
fn rectangle_scope_threshold_and_witness() {
    let mut b = board();
    b["pads"] = json!([
        pad("a", 0., 0., Some("VCC")),
        pad("b", 1.2, 0., Some("GND"))
    ]);
    let r = validate_json(
        json!({"board_json":b.to_string(),"rules":[{"kind":"clearance","minMm":0.3}],"scope":[{"kind":"padId","id":"a"},{"kind":"padId","id":"a"}]}),
    );
    assert_eq!(r["status"], "complete", "{r}");
    assert_eq!(rows(&r).len(), 1);
    near(rows(&r)[0]["actual_mm"].as_f64().unwrap(), 0.2);
    assert_eq!(rows(&r)[0]["source_ids"], json!(["a", "b"]));
    assert_eq!(rows(&r)[0]["status"], "confirmed");
    let w = &rows(&r)[0]["witness"];
    near(
        (w[0][0].as_f64().unwrap() - w[1][0].as_f64().unwrap())
            .hypot(w[0][1].as_f64().unwrap() - w[1][1].as_f64().unwrap()),
        0.2,
    );
    let pass = run(&b, json!([{"kind":"clearance","minMm":0.1}]));
    assert!(rows(&pass).is_empty());
    let equal = run(&b, json!([{"kind":"clearance","minMm":0.2}]));
    assert_eq!(rows(&equal)[0]["status"], "uncertain");
}
#[test]
fn touch_overlap_diagonal_and_point_to_segment() {
    let mut b = board();
    for (x, y, expected) in [(1., 0., 0.), (0.8, 0., 0.), (1.3, 1.4, 0.5)] {
        b["pads"] = json!([pad("a", 0., 0., Some("VCC")), pad("b", x, y, Some("GND"))]);
        let r = run(&b, json!([{"kind":"clearance","minMm":0.7}]));
        near(rows(&r)[0]["actual_mm"].as_f64().unwrap(), expected);
    }
    // Point (1,1) to diagonal segment x+y=3 is 1/sqrt(2), attained inside the segment.
    b["pads"] = json!([pad("a",0.5,0.5,Some("VCC")),{"id":"b","number":"1","net":"GND","at":{"x":0,"y":0},"rotationDeg":0,"layers":["TOP"],"shape":{"kind":"polygon","polygon":{"outer":[{"x":0,"y":3},{"x":3,"y":0},{"x":4,"y":0},{"x":0,"y":4}]}}}]);
    let r = run(&b, json!([{"kind":"clearance","minMm":1.}]));
    near(rows(&r)[0]["actual_mm"].as_f64().unwrap(), 1. / 2f64.sqrt());
}
#[test]
fn net_and_layer_policies() {
    let mut b = board();
    b["layers"] =
        json!([{"name":"TOP","index":0,"side":"top"},{"name":"BOTTOM","index":1,"side":"bottom"}]);
    b["pads"] = json!([
        pad("a", 0., 0., Some("VCC")),
        pad("b", 0.8, 0., Some("VCC"))
    ]);
    assert!(rows(&run(&b, json!([{"kind":"clearance","minMm":0.3}]))).is_empty());
    b["pads"][1]["net"] = json!("GND");
    b["pads"][1]["layers"] = json!(["BOTTOM"]);
    assert!(rows(&run(&b, json!([{"kind":"clearance","minMm":0.3}]))).is_empty());
    b["pads"][1] = pad("b", 0.8, 0., None);
    let r = run(&b, json!([{"kind":"clearance","minMm":0.3}]));
    assert_eq!(r["status"], "partial");
    assert!(r["violations"].as_array().unwrap().is_empty());
    assert_eq!(rows(&r)[0]["code"], "unknown_net_clearance");
}
#[test]
fn polygon_hole_has_positive_clearance() {
    let mut b = board();
    b["outline"] = json!([{"x":-5,"y":-5},{"x":15,"y":-5},{"x":15,"y":15},{"x":-5,"y":15}]);
    b["pads"] = json!([pad("p", 5., 5., Some("GND"))]);
    b["copper"]["fixed"]["zones"] = json!([{"id":"pour","kind":"fill","net":"VCC","layers":["TOP"],"outline":{"outer":[{"x":0,"y":0},{"x":10,"y":0},{"x":10,"y":10},{"x":0,"y":10}]},"filledPolygons":[{"layer":"TOP","source":"cad","status":"available","freshness":"current","polygons":[{"outer":[{"x":0,"y":0},{"x":10,"y":0},{"x":10,"y":10},{"x":0,"y":10}],"holes":[[{"x":4,"y":4},{"x":6,"y":4},{"x":6,"y":6},{"x":4,"y":6}]]}]}]}]);
    let r = run(&b, json!([{"kind":"clearance","minMm":0.7}]));
    near(rows(&r)[0]["actual_mm"].as_f64().unwrap(), 0.5);
    assert_eq!(r["status"], "complete", "{r}");
}
#[test]
fn outside_cutout_concave_and_drill_policy() {
    let mut b = board();
    b["outline"] = json!([{"x":0,"y":0},{"x":10,"y":0},{"x":10,"y":10},{"x":0,"y":10}]);
    b["cutouts"] = json!([[{"x":4,"y":4},{"x":6,"y":4},{"x":6,"y":6},{"x":4,"y":6}]]);
    b["pads"] = json!([
        pad("outside", 12., 2., Some("VCC")),
        pad("cutout", 5., 5., Some("VCC")),
        pad("inside", 2., 2., Some("GND"))
    ]);
    let r = run(&b, json!([{"kind":"edge-clearance","minMm":0.3}]));
    let bad: Vec<_> = rows(&r)
        .iter()
        .filter(|m| m["code"] == "copper_outside_material")
        .collect();
    assert_eq!(bad.len(), 2, "{r}");
    assert!(bad.iter().all(|m| m["status"] == "confirmed"));
    let cutout = bad
        .iter()
        .find(|m| m["source_ids"] == json!(["cutout"]))
        .unwrap();
    near(cutout["actual_mm"].as_f64().unwrap(), 0.5);
    let out = bad
        .iter()
        .find(|m| m["source_ids"] == json!(["outside"]))
        .unwrap();
    near(out["actual_mm"].as_f64().unwrap(), 1.5);
    // Concave edge is the x=3 notch, not the board bounding box.
    b["cutouts"] = json!([]);
    b["outline"] = json!([{"x":0,"y":0},{"x":10,"y":0},{"x":10,"y":3},{"x":3,"y":3},{"x":3,"y":10},{"x":0,"y":10}]);
    b["pads"] = json!([pad("near-notch", 2., 5., Some("VCC"))]);
    let c = run(&b, json!([{"kind":"edge-clearance","minMm":0.7}]));
    near(rows(&c)[0]["actual_mm"].as_f64().unwrap(), 0.5);
    b["holes"] = json!([{"id":"drill","at":{"x":1.2,"y":5},"hole":{"diameterMm":0.2,"shape":"round","plated":false}}]);
    let d = run(&b, json!([{"kind":"edge-clearance","minMm":0.7}]));
    near(rows(&d)[0]["actual_mm"].as_f64().unwrap(), 0.5);
}
#[test]
fn shorts_and_islands_are_distinct_and_order_stable() {
    let mut b = board();
    b["pads"] = json!([
        pad("a", 0., 0., Some("VCC")),
        pad("b", 0.8, 0., Some("GND")),
        pad("c", 7., 0., Some("VCC"))
    ]);
    let a = run(
        &b,
        json!([{"kind":"connectivity"},{"kind":"clearance","minMm":0.3}]),
    );
    assert_eq!(a["status"], "partial");
    assert!(rows(&a).iter().any(|m| m["code"] == "physical_short"));
    assert!(!rows(&a).iter().any(|m| m["code"] == "open"));
    b["pads"].as_array_mut().unwrap().reverse();
    let c = run(
        &b,
        json!([{"kind":"clearance","minMm":0.3},{"kind":"connectivity"}]),
    );
    assert_eq!(a["measurements"], c["measurements"]);
}
#[test]
fn incomplete_and_invalid_inputs_do_not_claim_pass() {
    let mut b = board();
    b["pads"] = json!([pad("a", 0., 0., Some("VCC"))]);
    b.as_object_mut().unwrap().remove("outline");
    assert_eq!(
        run(&b, json!([{"kind":"edge-clearance","minMm":0.3}]))["status"],
        "rejected"
    );
    let b = board();
    for rule in [
        json!({"kind":"length","maxMm":10}),
        json!({"kind":"clearance","minMm":-1}),
        json!({"kind":"clearance","minMm":1001}),
    ] {
        assert_eq!(run(&b, json!([rule]))["status"], "rejected");
    }
    for scope in [
        json!([]),
        json!([{"kind":"via","id":"v"}]),
        json!([{"kind":"region","layers":["TOP"],"polygon":{"outer":[]}}]),
    ] {
        assert_eq!(
            validate_json(
                json!({"board_json":b.to_string(),"rules":[{"kind":"clearance","minMm":0.2}],"scope":scope})
            )["status"],
            "rejected"
        );
    }
}
#[test]
fn bounded_failures_and_recovery() {
    let mut b = board();
    b["pads"] = json!([
        pad("a", 0., 0., Some("VCC")),
        pad("b", 1.1, 0., Some("GND")),
        pad("c", 2.2, 0., Some("VCC"))
    ]);
    let base = json!({"board_json":b.to_string(),"rules":[{"kind":"clearance","minMm":0.3}]});
    for delta in [
        json!({"budget":{"maxMemoryMiB":1}}),
        json!({"budget":{"maxSeconds":1e-12}}),
        json!({"limits":{"max_candidates":1}}),
        json!({"limits":{"max_exact_checks":1}}),
        json!({"limits":{"max_measurements":1}}),
        json!({"limits":{"max_report_vertices":1}}),
        json!({"limits":{"max_report_bytes":16}}),
    ] {
        let mut r = base.clone();
        for (k, v) in delta.as_object().unwrap() {
            r[k] = v.clone();
        }
        let bad = validate_json(r);
        assert_eq!(bad["status"], "rejected", "{bad}");
        assert_eq!(validate_json(base.clone())["status"], "complete");
    }
}
#[test]
fn scope_component_pad_net_and_region_select_against_neighbours() {
    let mut b = board();
    b["components"] = json!([{"designator":"U1","at":{"x":0,"y":0},"rotationDeg":0,"side":"top"}]);
    let mut a = pad("a", 0., 0., Some("VCC"));
    a["component"] = json!("U1");
    b["pads"] = json!([a, pad("b", 1.2, 0., Some("GND"))]);
    for scope in [
        json!([{"kind":"pad","designator":"U1","number":"1"}]),
        json!([{"kind":"component","designator":"U1"}]),
        json!([{"kind":"net","name":"VCC"}]),
        json!([{"kind":"region","layers":["TOP"],"polygon":{"outer":[{"x":-0.6,"y":-0.6},{"x":0.6,"y":-0.6},{"x":0.6,"y":0.6},{"x":-0.6,"y":0.6}]}}]),
    ] {
        let r = validate_json(
            json!({"board_json":b.to_string(),"rules":[{"kind":"clearance","minMm":0.3}],"scope":scope}),
        );
        assert_eq!(rows(&r).len(), 1, "{r}");
        assert_eq!(rows(&r)[0]["source_ids"], json!(["a", "b"]));
    }
}
#[test]
fn barrel_short_evidence_and_partial_fill() {
    let mut b = board();
    b["layers"] =
        json!([{"name":"TOP","index":0,"side":"top"},{"name":"BOTTOM","index":1,"side":"bottom"}]);
    b["copper"]["fixed"]["vias"] = json!([{"id":"via","net":"VCC","at":{"x":0,"y":0},"diameterMm":2,"drillMm":1,"fromLayer":"TOP","toLayer":"BOTTOM","type":"through"}]);
    let mut p = pad("pad", 1.2, 0., Some("GND"));
    p["layers"] = json!(["BOTTOM"]);
    b["pads"] = json!([p]);
    let r = run(&b, json!([{"kind":"connectivity"}]));
    assert!(
        rows(&r).iter().any(|m| m["code"] == "physical_short"),
        "{r}"
    );
    assert!(r["graph"]["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["kind"] == "barrel" && c["reliable"] == true));
    b["copper"]["fixed"]["zones"] = json!([{"id":"missing","kind":"fill","net":"VCC","layers":["TOP"],"outline":{"outer":[{"x":3,"y":0},{"x":4,"y":0},{"x":4,"y":1},{"x":3,"y":1}]},"filledPolygons":[{"layer":"TOP","source":"cad","status":"missing","reason":"known unavailable fill"}]}]);
    let partial = run(&b, json!([{"kind":"clearance","minMm":0.2}]));
    assert_eq!(partial["status"], "partial");
    assert!(partial["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "fill_unavailable"));
    b["copper"]["fixed"]["zones"][0]["filledPolygons"][0] = json!({"layer":"TOP","source":"cad","status":"available","freshness":"stale","polygons":[{"outer":[{"x":3,"y":0},{"x":4,"y":0},{"x":4,"y":1},{"x":3,"y":1}]}]});
    let stale = run(&b, json!([{"kind":"clearance","minMm":0.2}]));
    assert_eq!(stale["status"], "partial");
    assert!(stale["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "fill_freshness"));
}
#[test]
fn zero_edge_limit_does_not_certify_uncertain_material_containment() {
    let mut b = board();
    b["outline"] = json!([{"x":0,"y":0},{"x":10,"y":0},{"x":10,"y":10},{"x":0,"y":10}]);
    let mut p = pad("curved", 0.4996, 2., Some("VCC"));
    p["rotationDeg"] = json!(3.6);
    p["shape"] = json!({"kind":"circle","diameterMm":1.});
    b["pads"] = json!([p]);
    let r = run(&b, json!([{"kind":"edge-clearance","minMm":0.}]));
    assert_eq!(r["status"], "partial", "{r}");
    assert!(rows(&r)
        .iter()
        .any(|m| m["status"] == "uncertain" || m["code"] == "copper_outside_material"));
}
#[test]
fn outside_observer_does_not_reintroduce_drilled_away_copper() {
    let mut b = board();
    b["outline"] = json!([{"x":0,"y":0},{"x":10,"y":0},{"x":10,"y":10},{"x":0,"y":10}]);
    b["pads"] = json!([pad("outside", 12., 2., Some("VCC"))]);
    b["holes"] = json!([{"id":"bore","at":{"x":12,"y":2},"hole":{"shape":"round","diameterMm":2,"plated":false}}]);
    let r = run(&b, json!([{"kind":"edge-clearance","minMm":0.3}]));
    assert!(rows(&r).is_empty(), "{r}");
    assert_eq!(r["statistics"]["outside_regions"], 0);
}
#[test]
fn large_source_distance_is_bounded_without_changing_the_exact_gap() {
    let mut b = board();
    b["outline"] = json!([{"x":-10,"y":-10},{"x":10,"y":-10},{"x":10,"y":10},{"x":-10,"y":10}]);
    let outer: Vec<_> = (0..4000)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 4000.;
            json!({"x":5.*t.cos(),"y":5.*t.sin()})
        })
        .collect();
    let mut p = pad("large", 0., 0., Some("VCC"));
    p["shape"] = json!({"kind":"polygon","polygon":{"outer":outer}});
    b["pads"] = json!([p, pad("small", 5.7, 0., Some("GND"))]);
    let r = validate_json(
        json!({"board_json":b.to_string(),"rules":[{"kind":"clearance","minMm":0.3}],"limits":{"max_exact_checks":1024}}),
    );
    assert_eq!(r["status"], "complete", "{r}");
    near(rows(&r)[0]["actual_mm"].as_f64().unwrap(), 0.2);
    assert_eq!(rows(&r)[0]["status"], "confirmed");
}
