use pcb_analyze::dc::{Request, Session};
use serde_json::{json, Value};
fn pad(id: &str, x: f64, y: f64, w: f64, h: f64, net: &str) -> Value {
    json!({"id":id,"number":"1","net":net,"at":{"x":x,"y":y},"rotationDeg":0,"layers":["TOP"],"shape":{"kind":"rect","widthMm":w,"heightMm":h}})
}
fn board() -> Value {
    let mut b: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/conduction-contact-coupon.json"
    ))
    .unwrap();
    let b = b.get_mut("board").unwrap();
    b["outline"] = json!([{"x":-2.,"y":-3.},{"x":12.,"y":-3.},{"x":12.,"y":9.},{"x":-2.,"y":9.}]);
    b["layers"] = json!([{"name":"TOP","index":0,"side":"top"}]);
    b["nets"] = json!([{"name":"A"},{"name":"B"}]);
    b["pads"] = json!([
        pad("strip", 5., 0., 10., 2., "A"),
        pad("left", 0.5, 0., 1., 2., "A"),
        pad("right", 9.5, 0., 1., 2., "A")
    ]);
    b["holes"] = json!([]);
    b["components"] = json!([]);
    b["copper"] = json!({"fixed":{"tracks":[],"vias":[],"zones":[]},"editable":{"tracks":[],"vias":[],"zones":[]}});
    b.clone()
}
fn request(b: Value, area: f64) -> Request {
    serde_json::from_value(json!({"context_id":"test","board_json":b.to_string(),"conductor":null,
        "conduction":{"mesh":{"max_area_mm2":area,"max_edge_mm":(0.25*area).sqrt(),"min_angle_deg":0,"max_refinement_steps":64},"profile":{"stackup":[{"kind":"copper","layer":"TOP","thickness_mm":0.035}],"materials":{"TOP":{"conductivity_s_m":58e6,"state":"constant isotropic 20 C coupon"}}}},
        "terminals":[{"id":"p","pad_id":"left"},{"id":"n","pad_id":"right"}],
        "elements":[{"id":"source","kind":"current","positive":"p","negative":"n","value":1.,"role":"source"}]})).unwrap()
}
fn finite(r: &Value) -> f64 {
    assert_eq!(r["status"],"complete","{} domains={}",r["diagnostics"],r["mesh"]["domains"].as_array().map(|ds|ds.iter().map(|d|json!({"area":d["max_area_mm2"],"reasons":d["readiness"]["reasons"],"steps":d["refinement_steps"]})).collect::<Vec<_>>()).map(|v|json!(v)).unwrap_or(Value::Null));
    r["elements"][0]["voltage_v"].as_f64().unwrap()
}
fn near(a: f64, b: f64, rel: f64, abs: f64) {
    assert!((a - b).abs() <= abs + rel * b.abs(), "{a:e} != {b:e}");
}
fn evidence(name: &str, v: &Value) {
    if let Ok(dir) = std::env::var("PCB_ANALYZE_DC_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.json")),
            serde_json::to_vec(v).unwrap(),
        )
        .unwrap();
    }
}
fn quality(r: &Value) {
    assert!(r["quality"]["relative_residual"].as_f64().unwrap() <= 1e-8);
    assert!(
        r["quality"]["current_balance_a"].as_f64().unwrap() <= 1e-12
            || r["quality"]["normalized_current_balance"].as_f64().unwrap() <= 1e-8
    );
    assert!(
        r["quality"]["power_balance_w"].as_f64().unwrap().abs() <= 1e-15
            || r["quality"]["normalized_power_balance"].as_f64().unwrap() <= 1e-8
    );
}
#[test]
fn strip_analytic_fem_and_three_refinements() {
    let expected = 8e-3 / (58e6 * 2e-3 * 0.035e-3);
    for area in [0.5, 0.125, 0.03125] {
        let r = Session::default().solve(request(board(), area));
        near(finite(&r), expected, 0.01, 1e-12);
        near(r["copper_loss_w"].as_f64().unwrap(), expected, 0.01, 1e-15);
        quality(&r);
        evidence(&format!("strip-{area}"), &r);
    }
}
#[test]
fn rhs_reuse_scaling_and_profile_invalidation() {
    let mut s = Session::default();
    let mut q = request(board(), 0.5);
    let a = s.solve(q.clone());
    let v = finite(&a);
    q.elements[0].value = 2.;
    let b = s.solve(q.clone());
    near(finite(&b), 2. * v, 1e-10, 1e-12);
    near(
        b["copper_loss_w"].as_f64().unwrap(),
        4. * a["copper_loss_w"].as_f64().unwrap(),
        1e-10,
        1e-15,
    );
    assert_eq!(b["reused_factorization"], true);
    assert_eq!(b["reused_preparation"], true);
    q.conduction
        .profile
        .as_mut()
        .unwrap()
        .materials
        .get_mut("TOP")
        .unwrap()
        .conductivity_s_m /= 2.;
    let c = s.solve(q);
    near(finite(&c), 4. * v, 1e-10, 1e-12);
    assert_eq!(c["reused_factorization"], false);
}
#[test]
fn rotation_translation_preserves_physics() {
    let a = Session::default().solve(request(board(), 0.25));
    let mut b = board();
    for p in b["pads"].as_array_mut().unwrap() {
        let x = p["at"]["x"].as_f64().unwrap();
        let y = p["at"]["y"].as_f64().unwrap();
        p["at"] = json!({"x":100.-y,"y":-70.+x});
        p["rotationDeg"] = json!(90.);
    }
    for p in b["outline"].as_array_mut().unwrap() {
        let x = p["x"].as_f64().unwrap();
        let y = p["y"].as_f64().unwrap();
        *p = json!({"x":100.-y,"y":-70.+x});
    }
    let b = Session::default().solve(request(b, 0.25));
    near(finite(&a), finite(&b), 0.01, 1e-12);
    quality(&b);
}
#[test]
fn supply_and_return_resistance_and_resistive_load() {
    let mut b = board();
    for p in [
        pad("return", 5., 5., 10., 2., "B"),
        pad("rn", 0.5, 5., 1., 2., "B"),
        pad("rp", 9.5, 5., 1., 2., "B"),
    ] {
        b["pads"].as_array_mut().unwrap().push(p);
    }
    let mut q = request(b, 0.125);
    q.terminals
        .push(serde_json::from_value(json!({"id":"rn","pad_id":"rn"})).unwrap());
    q.terminals
        .push(serde_json::from_value(json!({"id":"rp","pad_id":"rp"})).unwrap());
    q.elements=serde_json::from_value(json!([
        {"id":"supply","kind":"voltage","positive":"p","negative":"rn","value":1.,"role":"source"},
        {"id":"load","kind":"resistance","positive":"n","negative":"rp","value":0.1,"role":"load"}])).unwrap();
    let r = Session::default().solve(q);
    finite(&r);
    quality(&r);
    evidence("resistive-return", &r);
    assert_eq!(r["terminals"]["rn"]["voltage_v"], 0.);
    for terminal in r["terminals"].as_object().unwrap().values() {
        assert!(r["system"]["gauges"]
            .as_array()
            .unwrap()
            .contains(&terminal["reference_node"]));
    }
    let strip = 8e-3 / (58e6 * 2e-3 * 0.035e-3);
    let current = 1. / (0.1 + 2. * strip);
    near(
        r["elements"][0]["delivered_current_a"].as_f64().unwrap(),
        current,
        0.01,
        1e-10,
    );
    near(
        r["elements"][1]["voltage_v"].as_f64().unwrap(),
        0.1 * current,
        0.01,
        1e-10,
    );
    near(
        r["terminals"]["rp"]["voltage_v"].as_f64().unwrap(),
        strip * current,
        0.01,
        1e-10,
    );
    near(
        r["copper_loss_w"].as_f64().unwrap(),
        2. * strip * current * current,
        0.01,
        1e-12,
    );
}
#[test]
fn missing_paths_physics_conflicts_and_memory_reject() {
    let mut q = request(board(), 0.5);
    q.terminals[1].pad_id = "missing".into();
    let r = Session::default().solve(q);
    assert_eq!(r["diagnostics"][0]["code"], "DC_TERMINAL_MISSING");
    let mut b = board();
    b["pads"][2] = pad("right", 9.5, 6., 1., 2., "A");
    let r = Session::default().solve(request(b, 0.5));
    assert_eq!(r["diagnostics"][0]["code"], "DC_OPEN_OR_FLOATING_PATH");
    let mut q = request(board(), 0.5);
    q.conduction.profile = None;
    let r = Session::default().solve(q);
    assert_eq!(r["diagnostics"][0]["code"], "DC_DOMAIN_NOT_READY");
    for (other, code) in [
        (1., "DC_INDETERMINATE_SOURCE_CURRENT"),
        (2., "DC_CONFLICTING_VOLTAGE_SOURCES"),
    ] {
        let mut q = request(board(), 0.5);
        q.elements[0].kind = pcb_analyze::dc::ElementKind::Voltage;
        let mut extra = q.elements[0].clone();
        extra.id = "other".into();
        extra.value = other;
        q.elements.push(extra);
        let r = Session::default().solve(q);
        assert_eq!(r["diagnostics"][0]["code"], code);
    }
    let mut q = request(board(), 0.5);
    q.limits.max_memory_mib = 1.;
    let r = Session::default().solve(q);
    assert_eq!(r["diagnostics"][0]["code"], "DC_MEMORY_LIMIT");
}
#[test]
fn standalone_is_si_and_never_invents_material() {
    let mut q = request(board(), 0.5);
    q.board_json = None;
    q.conductor = Some(pcb_analyze::dc::Conductor {
        length_mm: 10.,
        width_mm: 1.,
        thickness_mm: 0.035,
        conductivity_s_m: 58e6,
        state: "constant".into(),
    });
    let r = Session::default().solve(q.clone());
    assert!(r["method"].as_str().unwrap().contains("analytic uniform"));
    let expected = 0.01 / (58e6 * 1e-3 * 0.035e-3);
    near(
        r["standalone"]["resistance_ohm"].as_f64().unwrap(),
        expected,
        1e-12,
        1e-15,
    );
    q.conductor.as_mut().unwrap().state.clear();
    let r = Session::default().solve(q);
    assert_eq!(r["diagnostics"][0]["code"], "MISSING_CONDUCTOR_PHYSICS");
}

#[test]
fn finite_via_chain_retains_intermediate_contact_and_empty_bore() {
    let mut b = board();
    b["layers"] = json!([{"name":"TOP","index":0,"side":"top"},{"name":"I1","index":1,"side":"inner"},{"name":"BOTTOM","index":2,"side":"bottom"}]);
    let mut top = pad("left", 0., 0., 2., 2., "A");
    top["shape"] = json!({"kind":"circle","diameterMm":2.});
    let mut bottom = top.clone();
    bottom["id"] = json!("right");
    bottom["layers"] = json!(["BOTTOM"]);
    b["pads"] = json!([top, bottom]);
    b["copper"]["fixed"]["vias"] = json!([{"id":"v","net":"A","at":{"x":0.,"y":0.},"diameterMm":2.,"drillMm":1.,"fromLayer":"TOP","toLayer":"BOTTOM","type":"through"}]);
    let mut q = request(b, 0.1);
    q.conduction.profile=Some(serde_json::from_value(json!({"stackup":[{"kind":"copper","layer":"TOP","thickness_mm":0.035},{"kind":"dielectric","name":"gap1","thickness_mm":0.5},{"kind":"copper","layer":"I1","thickness_mm":0.035},{"kind":"dielectric","name":"gap2","thickness_mm":0.5},{"kind":"copper","layer":"BOTTOM","thickness_mm":0.035}],"materials":{"TOP":{"conductivity_s_m":58e6,"state":"constant"},"I1":{"conductivity_s_m":58e6,"state":"constant"},"BOTTOM":{"conductivity_s_m":58e6,"state":"constant"}},"barrel":{"plating_thickness_mm":0.025,"material":{"conductivity_s_m":58e6,"state":"constant"},"diameter_convention":"finished_hole"}})).unwrap());
    let r = Session::default().solve(q.clone());
    let v = finite(&r);
    quality(&r);
    evidence("via-chain", &r);
    let resistance =
        0.00107 / (58e6 * std::f64::consts::PI * (0.525f64.powi(2) - 0.5f64.powi(2)) * 1e-6);
    near(v, resistance, 0.01, 1e-12);
    assert_eq!(r["barrels"].as_array().unwrap().len(), 2);
    near(
        r["barrel_loss_w"].as_f64().unwrap(),
        resistance,
        0.01,
        1e-15,
    );
    for d in r["mesh"]["domains"].as_array().unwrap() {
        for p in d["nodes"].as_array().unwrap() {
            assert!(p[0].as_f64().unwrap().hypot(p[1].as_f64().unwrap()) >= 0.499);
        }
    }
    // Missing plating data/unsupported slot remain blockers, even with snapshot permission.
    q.conduction.profile.as_mut().unwrap().barrel = None;
    q.snapshot = true;
    let r = Session::default().solve(q);
    assert_eq!(r["status"], "rejected");
    assert_eq!(r["diagnostics"][0]["code"], "DC_BARREL_NOT_READY");
}

#[test]
fn two_physical_parallel_branches_divide_current_by_width() {
    let mut b = board();
    b["pads"] = json!([
        pad("wide", 5., 0., 10., 2., "A"),
        pad("narrow", 5., 5., 10., 1., "A"),
        pad("left", 0.5, 2.25, 1., 6.5, "A"),
        pad("right", 9.5, 2.25, 1., 6.5, "A")
    ]);
    let r = Session::default().solve(request(b, 0.125));
    let v = finite(&r);
    quality(&r);
    let expected = 8e-3 / (58e6 * 3e-3 * 0.035e-3);
    near(v, expected, 0.01, 1e-12);
    let domain = &r["mesh"]["domains"][0];
    let field = &r["domains"][0];
    let mut currents = [0., 0.];
    // Independent integral across x=4.731 mm, using piecewise constant FEM J.
    for (ti, t) in domain["triangles"].as_array().unwrap().iter().enumerate() {
        let nodes: Vec<_> = t
            .as_array()
            .unwrap()
            .iter()
            .map(|i| &domain["nodes"][i.as_u64().unwrap() as usize])
            .collect();
        let mut intersections = vec![];
        for edge in 0..3 {
            let a = nodes[edge];
            let b = nodes[(edge + 1) % 3];
            let ax = a[0].as_f64().unwrap();
            let bx = b[0].as_f64().unwrap();
            if (ax < 4.731) != (bx < 4.731) {
                let ay = a[1].as_f64().unwrap();
                let by = b[1].as_f64().unwrap();
                intersections.push(ay + (by - ay) * (4.731 - ax) / (bx - ax));
            }
        }
        if intersections.len() == 2 {
            let y = (intersections[0] + intersections[1]) / 2.;
            let group = if y < 2. { 0 } else { 1 };
            currents[group] += field["current_density_vector_a_mm2"][ti][0]
                .as_f64()
                .unwrap()
                .abs()
                * 0.035
                * (intersections[0] - intersections[1]).abs();
        }
    }
    near(currents[0], 2. / 3., 0.01, 1e-10);
    near(currents[1], 1. / 3., 0.01, 1e-10);
    evidence(
        "parallel-branches",
        &json!({"cut_mm":4.731,"integrated_current_a":currents,"report":r}),
    );
}

#[test]
fn turning_coupon_has_three_level_integral_convergence() {
    let mut b = board();
    let mut strip = pad("turn", 0., 0., 1., 1., "A");
    let points = [
        [0., 0.],
        [8., 0.],
        [8., 4.],
        [10., 4.],
        [10., 6.],
        [6., 6.],
        [6., 2.],
        [0., 2.],
    ];
    strip["shape"] = json!({"kind":"polygon","polygon":{"outer":points.iter().map(|p|json!({"x":p[0],"y":p[1]})).collect::<Vec<_>>()}});
    b["pads"] = json!([
        strip,
        pad("left", 0.5, 1., 1., 2., "A"),
        pad("right", 9.5, 5., 1., 2., "A")
    ]);
    let mut values = vec![];
    for area in [0.25, 0.0625, 0.015625] {
        let r = Session::default().solve(request(b.clone(), area));
        let v = finite(&r);
        quality(&r);
        values.push(v);
        evidence(&format!("turn-{area}"), &r);
    }
    for pair in values.windows(2) {
        near(pair[0], pair[1], 0.02, 1e-9);
    }
    evidence(
        "turn-convergence",
        &json!({"areas_mm2":[0.25,0.0625,0.015625],"drop_v_at_1a":values,"loss_w_at_1a":values}),
    );
}

#[test]
fn overflowing_postprocessing_is_rejected_and_old_mesh_is_not_attached_to_new_input() {
    let mut session = Session::default();
    let q = request(board(), 0.5);
    finite(&session.solve(q.clone()));
    let mut huge = q.clone();
    huge.elements[0].value = 1e200;
    let r = session.solve(huge);
    assert_eq!(r["status"], "rejected");
    assert_eq!(r["diagnostics"][0]["code"], "DC_NONFINITE");
    let mut other = q;
    other.board_json = Some("{}".into());
    let r = session.solve(other);
    assert_eq!(r["status"], "rejected");
    assert!(r.get("mesh").is_none());
}

#[test]
fn snapshot_is_narrow_permission_and_does_not_rewrite_mesh_readiness() {
    let mut b = board();
    b["pads"].as_array_mut().unwrap().remove(0);
    b["copper"]["fixed"]["zones"] = json!([{"id":"z","kind":"fill","net":"A","layers":["TOP"],"outline":{"outer":[{"x":0.,"y":-1.},{"x":10.,"y":-1.},{"x":10.,"y":1.},{"x":0.,"y":1.}]},"filledPolygons":[{"layer":"TOP","source":"cad","status":"available","freshness":"unknown","polygons":[{"outer":[{"x":0.,"y":-1.},{"x":10.,"y":-1.},{"x":10.,"y":1.},{"x":0.,"y":1.}]}]}]}]);
    let mut q = request(b.clone(), 0.25);
    let r = Session::default().solve(q.clone());
    assert_eq!(r["status"], "rejected");
    q.snapshot = true;
    let r = Session::default().solve(q);
    finite(&r);
    assert_eq!(r["conditional"], true);
    assert_eq!(r["mesh"]["domains"][0]["readiness"]["dc_ready"], false);
    assert_eq!(r["mesh"]["fills"][0]["freshness"], "unknown");
    for state in ["stale", "unknown"] {
        b["copper"]["fixed"]["zones"][0]["filledPolygons"][0]["freshness"] = json!(state);
        if state == "unknown" {
            b["copper"]["fixed"]["zones"][0]["filledPolygons"][0]["status"] = json!("failed");
            b["copper"]["fixed"]["zones"][0]["filledPolygons"][0]
                .as_object_mut()
                .unwrap()
                .remove("polygons");
        }
        let mut q = request(b.clone(), 0.25);
        q.snapshot = true;
        let r = Session::default().solve(q);
        assert_eq!(r["status"], "rejected");
    }
}

#[test]
fn small_copper_drops_survive_large_rail_offset() {
    // Numerical stress of the linear model, not a physical voltage rating:
    // fixed current gives the same strip fields/losses at either rail voltage.
    let mut b = board();
    for p in [
        pad("return", 5., 5., 10., 2., "B"),
        pad("rn", 0.5, 5., 1., 2., "B"),
        pad("rp", 9.5, 5., 1., 2., "B"),
    ] { b["pads"].as_array_mut().unwrap().push(p); }
    let resistance = 8e-3 / (58e6 * 2e-3 * 0.035e-3);
    for rail in [1., 1e8] {
        let mut q = request(b.clone(), 0.125);
        for id in ["rn", "rp"] {
            q.terminals.push(serde_json::from_value(json!({"id":id,"pad_id":id})).unwrap());
        }
        q.elements = serde_json::from_value(json!([
            {"id":"supply","kind":"voltage","positive":"p","negative":"rn","value":rail,"role":"source"},
            {"id":"load","kind":"current","positive":"n","negative":"rp","value":-1.,"role":"load"}
        ])).unwrap();
        let r = Session::default().solve(q);
        finite(&r);
        quality(&r);
        near(r["copper_loss_w"].as_f64().unwrap(), 2. * resistance, 0.01, 1e-12);
        near(r["elements"][0]["delivered_current_a"].as_f64().unwrap(), 1., 1e-8, 1e-12);
        for domain in r["domains"].as_array().unwrap() {
            let max_j = domain["current_density_a_mm2"].as_array().unwrap().iter()
                .map(|j| j.as_f64().unwrap()).fold(0., f64::max);
            near(max_j, 1. / (2. * 0.035), 0.01, 1e-8);
        }
    }
}
