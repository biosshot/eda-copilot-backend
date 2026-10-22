use pcb_analyze::{
    geometry::{prepare_json, GeometryOptions, PreparationReport},
    import_json,
};
use serde_json::{json, Value};
use std::{f64::consts::PI, fs, path::PathBuf, time::Instant};
fn points(v: &[[f64; 2]]) -> Value {
    json!(v
        .iter()
        .map(|p| json!({"x":p[0],"y":p[1]}))
        .collect::<Vec<_>>())
}
fn polygon(v: &[[f64; 2]]) -> Value {
    json!({"outer":points(v)})
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> Value {
    polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]])
}
fn board() -> Value {
    json!({
    "outline":points(&[[-100.,-100.],[100.,-100.],[100.,100.],[-100.,100.]]),"cutouts":[],
    "layers":[{"name":"TOP","index":0,"side":"top"},{"name":"I1","index":1,"side":"inner"},{"name":"I2","index":2,"side":"inner"},{"name":"BOTTOM","index":3,"side":"bottom"}],
    "nets":[{"name":"A"},{"name":"B"}],"components":[],"pads":[],"holes":[],"keepouts":[],
    "rules":{"default":{"clearanceMm":0.2,"edgeClearanceMm":0.3,"minTrackWidthMm":0.1,"preferredTrackWidthMm":0.2,"via":{"minDiameterMm":0.5,"preferredDiameterMm":0.6,"minDrillMm":0.2,"preferredDrillMm":0.3}},"nets":[]},
    "copper":{"fixed":{"tracks":[],"vias":[],"zones":[]},"editable":{"tracks":[],"vias":[],"zones":[]}}})
}
fn pad(id: &str, x: f64, y: f64, shape: Value) -> Value {
    json!({"id":id,"number":"1","net":"A","at":{"x":x,"y":y},"rotationDeg":0,"layers":["TOP"],"shape":shape})
}
fn rp(id: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    pad(id, x, y, json!({"kind":"rect","widthMm":w,"heightMm":h}))
}
fn track(id: &str, pts: &[[f64; 2]], w: f64) -> Value {
    json!({"id":id,"net":"A","layer":"TOP","widthMm":w,"points":points(pts)})
}
fn run(v: &Value) -> PreparationReport {
    run_e(v, 0.001)
}
fn run_e(v: &Value, e: f64) -> PreparationReport {
    let i = import_json(&v.to_string());
    assert!(i.board.is_some(), "{:?}", i.diagnostics);
    prepare_json(
        &v.to_string(),
        GeometryOptions {
            max_deviation_mm: e,
            ..Default::default()
        },
    )
}
fn area(r: &PreparationReport) -> f64 {
    r.layers[0].union_area_mm2
}
fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b}, tolerance {tol}");
}
fn zone(id: &str, polys: Vec<Value>) -> Value {
    json!({"id":id,"kind":"fill","net":"A","layers":["TOP"],"outline":rect(-90.,-90.,180.,180.),"filledPolygons":[{"layer":"TOP","source":"cad","status":"available","freshness":"current","nativePrimitiveIds":["native-fill"],"polygons":polys}]})
}
#[test]
fn analytic_shapes_and_refinement() {
    for (shape, expected, perimeter) in [
        (json!({"kind":"rect","widthMm":4,"heightMm":2}), 8., 12.),
        (json!({"kind":"circle","diameterMm":4}), 4. * PI, 4. * PI),
        (
            json!({"kind":"oval","widthMm":6,"heightMm":2}),
            8. + PI,
            8. + 2. * PI,
        ),
        (
            json!({"kind":"round-rect","widthMm":6,"heightMm":4,"cornerRadiusMm":1}),
            20. + PI,
            12. + 2. * PI,
        ),
    ] {
        let width = shape["widthMm"]
            .as_f64()
            .or_else(|| shape["diameterMm"].as_f64())
            .unwrap();
        let height = shape["heightMm"]
            .as_f64()
            .or_else(|| shape["diameterMm"].as_f64())
            .unwrap();
        let mut v = board();
        v["pads"] = json!([pad("p", 0., 0., shape)]);
        let a = run(&v);
        let b = run_e(&v, 0.0001);
        assert_eq!(a.status, "complete", "{:?}", a.diagnostics);
        near(area(&a), expected, perimeter * 0.001 + 1e-5);
        assert!((area(&b) - expected).abs() <= (area(&a) - expected).abs() + 1e-7);
        let bounds = &a.regions[0].polygon.outer;
        assert!(bounds.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        near(
            bounds.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            -width / 2.,
            1e-7,
        );
        near(
            bounds
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max),
            width / 2.,
            1e-7,
        );
        near(
            bounds.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
            -height / 2.,
            1e-7,
        );
        near(
            bounds
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max),
            height / 2.,
            1e-7,
        );
        assert!(b.statistics.generated_vertices >= a.statistics.generated_vertices);
        if v["pads"][0]["shape"]["kind"] == "circle" {
            let radius = width / 2.;
            for (i, p) in bounds.iter().enumerate() {
                let q = bounds[(i + 1) % bounds.len()];
                near(p[0].hypot(p[1]), radius, 1e-12);
                assert!(radius - ((p[0] + q[0]) / 2.).hypot((p[1] + q[1]) / 2.) <= 0.001 + 1e-12);
            }
        }
    }
}
#[test]
fn analytical_strokes_round_caps_bends_closed_and_overlap() {
    let mut v = board();
    v["copper"]["fixed"]["tracks"] = json!([track("s", &[[0., 0.], [4., 0.]], 2.)]);
    near(area(&run(&v)), 8. + PI, 0.02);
    v["copper"]["editable"]["tracks"] = json!([track("same", &[[0., 0.], [4., 0.]], 2.)]);
    near(area(&run(&v)), 8. + PI, 0.02);
    v["copper"]["editable"]["tracks"] = json!([track("cross", &[[2., -2.], [2., 2.]], 2.)]);
    near(area(&run(&v)), 2. * (8. + PI) - 4., 0.03);
    v["copper"]["editable"]["tracks"] = json!([]);
    v["copper"]["fixed"]["tracks"] = json!([track("bend", &[[0., 0.], [4., 0.], [4., 4.]], 2.)]);
    // Two capsules minus intersection: 1 square + three quarter disks.
    near(area(&run(&v)), 2. * (8. + PI) - (1. + 3. * PI / 4.), 0.03);
    v["copper"]["fixed"]["tracks"] = json!([track(
        "closed",
        &[[0., 0.], [4., 0.], [4., 4.], [0., 4.], [0., 0.]],
        2.
    )]);
    let r = run(&v);
    near(area(&r), 32. - 4. + PI, 0.04);
    assert_eq!(r.regions[0].polygon.holes.len(), 1);
}
#[test]
fn polygon_holes_annulus_and_island_in_void() {
    let mut v = board();
    let mut p = rect(-3., -3., 6., 6.);
    p["holes"] = json!([points(&[[-2., -2.], [2., -2.], [2., 2.], [-2., 2.]])]);
    v["pads"] = json!([
        pad("frame", 0., 0., json!({"kind":"polygon","polygon":p})),
        rp("island", 0., 0., 1., 1.)
    ]);
    let r = run(&v);
    near(area(&r), 21., 1e-6);
    assert_eq!(r.islands.len(), 2);
    v["pads"] = json!([pad("ring", 0., 0., json!({"kind":"circle","diameterMm":4}))]);
    v["holes"] = json!([{"id":"npth","at":{"x":0,"y":0},"hole":{"shape":"round","diameterMm":2,"plated":false}}]);
    near(area(&run(&v)), 3. * PI, 0.04);
}
#[test]
fn physical_topology_short_unknown_split_bbox_gap_boundary_point_bridge() {
    let mut v = board();
    v["pads"] = json!([rp("a", 0., 0., 2., 2.), rp("b", 5., 0., 2., 2.)]);
    let r = run(&v);
    assert_eq!(r.islands.len(), 2);
    assert_eq!(r.net_islands["A"].len(), 2);
    v["pads"][1]["at"]["x"] = json!(1.);
    v["pads"][1]["net"] = json!("B");
    let r = run(&v);
    assert_eq!(r.statistics.shorts, 1);
    near(area(&r), 6., 1e-6);
    v["pads"] = json!([
        rp("a", 0., 0., 2., 2.),
        rp("unknown", 1., 0., 2., 2.),
        rp("b", 2., 0., 2., 2.)
    ]);
    v["pads"][1].as_object_mut().unwrap().remove("net");
    v["pads"][2]["net"] = json!("B");
    let r = run(&v);
    assert_eq!(r.statistics.shorts, 1);
    assert!(r.islands[0].has_unknown_net);
    for (x, y, kind, components) in [
        (2., 0., "boundary", 1),
        (2., 2., "point", 2),
        (2.0000001, 0., "uncertain", 2),
        (2.0001, 0., "none", 2),
    ] {
        v["pads"] = json!([rp("a", 0., 0., 2., 2.), rp("b", x, y, 2., 2.)]);
        let r = run(&v);
        assert_eq!(r.islands.len(), components, "{kind}");
        if kind != "none" {
            assert!(
                r.contacts.iter().any(|c| c.kind == kind),
                "{kind}: {:?}",
                r.contacts
            );
        }
    }
    v["pads"] = json!([
        pad(
            "tri1",
            0.,
            0.,
            json!({"kind":"polygon","polygon":polygon(&[[0.,0.],[2.,0.],[0.,2.]])})
        ),
        pad(
            "tri2",
            0.,
            0.,
            json!({"kind":"polygon","polygon":polygon(&[[2.,2.],[2.,0.5],[0.5,2.]])})
        )
    ]);
    assert_eq!(run(&v).islands.len(), 2);
    assert!(run(&v).contacts.is_empty());
    v["pads"] = json!([
        rp("a", 0., 0., 2., 2.),
        rp("b", 4., 0., 2., 2.),
        rp("bridge", 2., 0., 2.1, 0.02)
    ]);
    assert_eq!(run(&v).islands.len(), 1);
}
#[test]
fn malformed_topology_is_never_repaired_and_equivalent_vertices_normalize() {
    let mut v = board();
    let bad = polygon(&[[0., 0.], [3., 2.], [0., 2.], [2., 0.]]);
    v["pads"] = json!([pad("bad", 0., 0., json!({"kind":"polygon","polygon":bad}))]);
    let r = run(&v);
    assert_eq!(r.status, "invalid");
    assert!(!r.excluded.is_empty());
    assert!(r.regions.is_empty());
    let mut p = rect(0., 0., 2., 2.);
    p["holes"] = json!([rect(3., 3., 1., 1.)["outer"]]);
    v["pads"][0]["shape"]["polygon"] = p;
    assert_eq!(run(&v).status, "invalid");
    v["pads"][0]["shape"]["polygon"] =
        polygon(&[[0., 0.], [0., 0.], [2., 0.], [2., 2.], [0., 2.], [0., 0.]]);
    near(area(&run(&v)), 4., 1e-6);
}
#[test]
fn via_spans_and_drill_global_subtraction() {
    for (from, to, kind, n) in [
        ("TOP", "BOTTOM", "through", 4),
        ("TOP", "I1", "blind-buried", 2),
        ("I1", "I2", "blind-buried", 2),
    ] {
        let mut v = board();
        v["copper"]["fixed"]["vias"] = json!([{"id":"via","net":"A","at":{"x":0,"y":0},"diameterMm":2,"drillMm":1,"fromLayer":from,"toLayer":to,"type":kind}]);
        let r = run(&v);
        assert_eq!(r.regions.len(), n);
        assert_eq!(r.islands.len(), 1, "{kind}: {:?}", r.diagnostics);
        assert_eq!(r.holes[0].layers.len(), n);
        for layer in &r.layers {
            near(
                layer.union_area_mm2,
                if r.holes[0].layers.contains(&layer.layer) {
                    0.75 * PI
                } else {
                    0.
                },
                0.02,
            );
        }
    }
    let mut v = board();
    v["pads"] = json!([rp("neighbor", 0., 0., 0.2, 0.2)]);
    v["holes"] = json!([{"id":"npth","at":{"x":0,"y":0},"hole":{"shape":"round","diameterMm":1,"plated":false}}]);
    let r = run(&v);
    assert!(r.regions.is_empty());
    assert!(r.contacts.is_empty());
    v["holes"][0]["hole"]["plated"] = json!(true);
    let r = run(&v);
    assert_eq!(r.status, "partial");
    assert!(r.holes[0].layers.is_empty());
    assert_eq!(r.regions.len(), 1);
}
#[test]
fn plated_pad_npth_and_slot_local_rotation_layer_shapes() {
    let mut v = board();
    let mut p = rp("p", 3., 4., 4., 4.);
    p["layers"] = json!(["TOP", "I1", "I2", "BOTTOM"]);
    p["hole"] = json!({"shape":"round","diameterMm":1,"plated":true});
    v["pads"] = json!([p]);
    let r = run(&v);
    assert_eq!(r.islands.len(), 1, "{:?}", r.diagnostics);
    v["pads"][0]["hole"]["plated"] = json!(false);
    assert_eq!(run(&v).islands.len(), 4);
    v["pads"][0]["hole"] = json!({"shape":"slot","diameterMm":1,"slotLengthMm":3,"offset":{"x":1,"y":0},"rotationDeg":30,"plated":false});
    v["pads"][0]["rotationDeg"] = json!(90.);
    let r = run(&v);
    let outer = &r.holes[0].polygons[0].outer;
    let (sx, sy) = outer
        .iter()
        .fold((0., 0.), |(x, y), p| (x + p[0], y + p[1]));
    near(sx / outer.len() as f64, 3., 1e-6);
    near(sy / outer.len() as f64, 5., 1e-6);
    let a = polygon_area(&r.holes[0].polygons[0].outer);
    near(a, 2. + PI / 4., 0.01);
    v["pads"][0].as_object_mut().unwrap().remove("hole");
    v["pads"][0]["shapesByLayer"] = json!({"TOP":{"kind":"rect","widthMm":2,"heightMm":1},"I1":{"kind":"circle","diameterMm":1},"I2":{"kind":"rect","widthMm":3,"heightMm":1},"BOTTOM":{"kind":"rect","widthMm":4,"heightMm":1}});
    let r = run(&v);
    near(r.layers[0].union_area_mm2, 2., 1e-6);
    near(r.layers[3].union_area_mm2, 4., 1e-6);
}
fn polygon_area(ps: &[[f64; 2]]) -> f64 {
    (0..ps.len())
        .map(|i| ps[i][0] * ps[(i + 1) % ps.len()][1] - ps[i][1] * ps[(i + 1) % ps.len()][0])
        .sum::<f64>()
        .abs()
        / 2.
}
#[test]
fn transforms_bottom_component_no_pin_short_translation_and_permutation() {
    let mut v = board();
    v["components"] =
        json!([{"id":"u","designator":"U1","at":{"x":50,"y":60},"rotationDeg":30,"side":"bottom"}]);
    v["pads"] = json!([rp("a", 0., 0., 2., 1.), rp("b", 4., 0., 2., 1.)]);
    for p in v["pads"].as_array_mut().unwrap() {
        p["component"] = json!("U1");
        p["rotationDeg"] = json!(37.);
    }
    let r = run(&v);
    assert_eq!(r.islands.len(), 2);
    near(area(&r), 4., 1e-6);
    assert_eq!(r.regions[0].source.component_id.as_deref(), Some("u"));
    // Component transform would move these far from the origin; bottom mirror
    // would invert the first vertex of the local asymmetric rotated rectangle.
    let p = &r.regions[0].polygon.outer;
    let target = [
        -37f64.to_radians().cos() + 0.5 * 37f64.to_radians().sin(),
        -37f64.to_radians().sin() - 0.5 * 37f64.to_radians().cos(),
    ];
    assert!(p
        .iter()
        .any(|p| (p[0] - target[0]).hypot(p[1] - target[1]) < 1e-8));
    v["pads"].as_array_mut().unwrap().reverse();
    for p in v["pads"].as_array_mut().unwrap() {
        p["at"]["x"] = json!(p["at"]["x"].as_f64().unwrap() + 10.);
        p["at"]["y"] = json!(10.);
    }
    let b = run(&v);
    near(area(&b), area(&r), 1e-6);
    assert_eq!(b.islands.len(), 2);
    assert_eq!(
        b.regions
            .iter()
            .map(|r| &r.source.source_id)
            .collect::<Vec<_>>(),
        r.regions
            .iter()
            .map(|r| &r.source.source_id)
            .collect::<Vec<_>>()
    );
}
#[test]
fn zones_fills_freshness_empty_boundary_keepout_and_outline() {
    let mut v = board();
    v["copper"]["fixed"]["zones"] = json!([zone(
        "fill",
        vec![rect(0., 0., 2., 2.), rect(5., 0., 1., 1.)]
    )]);
    let r = run(&v);
    near(area(&r), 5., 1e-6);
    assert_eq!(r.islands.len(), 2);
    assert_eq!(r.regions[0].source.native_fill_ids, vec!["native-fill"]);
    v["keepouts"] = json!([{"id":"ko","layers":["TOP"],"polygon":rect(-90.,-90.,180.,180.),"forbid":{"tracks":true,"vias":true,"zones":true}}]);
    near(area(&run(&v)), 5., 1e-6);
    v["copper"]["fixed"]["zones"][0]["filledPolygons"][0]["polygons"] = json!([]);
    near(area(&run(&v)), 0., 0.);
    for status in ["missing", "unsupported", "failed"] {
        v["copper"]["fixed"]["zones"][0]["filledPolygons"] =
            json!([{"layer":"TOP","source":"cad","status":status,"reason":"test"}]);
        let r = run(&v);
        assert_eq!(r.status, "partial");
        near(area(&r), 0., 0.);
    }
    v["copper"]["fixed"]["zones"] = json!([zone("pour", vec![rect(0., 0., 2., 2.)])]);
    v["copper"]["fixed"]["zones"][0]["kind"] = json!("pour");
    v["copper"]["fixed"]["zones"][0]["refillRules"] = json!({"status":"unknown","unresolved":[]});
    for freshness in ["stale", "unknown"] {
        v["copper"]["fixed"]["zones"][0]["filledPolygons"][0]["freshness"] = json!(freshness);
        let r = run(&v);
        assert_eq!(r.status, "partial");
        near(area(&r), 4., 1e-6);
    }
    v["pads"] = json!([rp("edge", 100., 0., 2., 2.)]);
    let r = run(&v);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "copper_outside_board"));
    near(
        r.regions
            .iter()
            .filter(|r| r.source.source_id.as_deref() == Some("edge"))
            .map(|r| r.area_mm2)
            .sum(),
        2.,
        1e-6,
    );
}
#[test]
fn refinement_near_tangency_never_bridges_positive_gap() {
    let mut v = board();
    v["pads"] = json!([
        pad("a", 0., 0., json!({"kind":"circle","diameterMm":2})),
        pad("b", 2.0001, 0., json!({"kind":"circle","diameterMm":2}))
    ]);
    for e in [0.001, 0.0001, 0.00001] {
        let r = run_e(&v, e);
        assert_eq!(r.islands.len(), 2);
        assert!(r.contacts.iter().all(|c| !c.reliable));
    }
}
#[test]
fn explicit_generation_limits_and_nonfinite_options() {
    let mut v = board();
    v["pads"] = json!([pad("p", 0., 0., json!({"kind":"circle","diameterMm":4}))]);
    let mut o = GeometryOptions::default();
    o.max_vertices_per_curve = 8;
    let r = prepare_json(&v.to_string(), o);
    assert_eq!(r.status, "invalid");
    assert_eq!(r.excluded.len(), 1);
    o.max_deviation_mm = f64::NAN;
    assert_eq!(prepare_json(&v.to_string(), o).status, "invalid");
}
#[test]
fn separated_object_growth_candidates_and_corpus() {
    for n in [100, 500, 1000] {
        let mut v = board();
        v["pads"] = json!((0..n)
            .map(|i| rp(
                &format!("p{i}"),
                (i % 50) as f64 * 3. - 75.,
                (i / 50) as f64 * 3. - 75.,
                1.,
                1.
            ))
            .collect::<Vec<_>>());
        let now = Instant::now();
        let r = run(&v);
        assert_eq!(r.statistics.candidate_pairs, 0);
        assert_eq!(r.islands.len(), n);
        println!(
            "growth n={n} candidates=0 elapsed_ms={}",
            now.elapsed().as_millis()
        );
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/pcb-analyze");
    for name in [
        "BandAmpV5",
        "ESPower",
        "NanoScope",
        "NanoSpec",
        "STM32G0_CAN_RS485",
        "TPS63060DSCR",
        "ThunderF722",
        "USB_PD_Trigger_5A",
    ] {
        let s = fs::read_to_string(root.join(name).join(format!("{name}.json"))).unwrap();
        let now = Instant::now();
        let r = prepare_json(&s, GeometryOptions::default());
        assert!(r.outline.is_some(), "{name}");
        assert!(!r.regions.is_empty(), "{name}");
        assert!(r.regions.iter().all(|x| x.area_mm2 > 0.));
        if let Some(output) = std::env::var_os("PCB_ANALYZE_RUST_REPORT_DIR") {
            fs::create_dir_all(&output).unwrap();
            fs::write(
                PathBuf::from(output).join(format!("{name}.json")),
                serde_json::to_vec(&r).unwrap(),
            )
            .unwrap();
        }
        println!(
            "corpus {name}: {} regions={} islands={} shorts={} elapsed_ms={} bytes={}",
            r.status,
            r.regions.len(),
            r.islands.len(),
            r.statistics.shorts,
            now.elapsed().as_millis(),
            serde_json::to_vec(&r).unwrap().len()
        );
        assert_eq!(
            r.status,
            "partial",
            "{name}: {:?}",
            r.diagnostics
                .iter()
                .filter(|d| d.category == "invalid")
                .collect::<Vec<_>>()
        );
        assert!(
            r.excluded.is_empty(),
            "{name}: captured sources must remain in nominal snapshots"
        );
    }
}

#[test]
fn blind_drill_does_not_cut_other_layers_and_npth_splits_neighbor_track() {
    let mut v = board();
    let mut p = rp("small", 0., 0., 0.2, 0.2);
    p["layers"] = json!(["TOP", "I1", "I2", "BOTTOM"]);
    v["pads"] = json!([p]);
    v["copper"]["fixed"]["vias"] = json!([{"id":"buried","net":"A","at":{"x":0,"y":0},"diameterMm":1.2,"drillMm":0.6,"fromLayer":"I1","toLayer":"I2","type":"blind-buried"}]);
    let r = run(&v);
    let pad_layers: Vec<_> = r
        .regions
        .iter()
        .filter(|r| r.source.kind == "pad")
        .map(|r| r.layer.as_str())
        .collect();
    assert_eq!(pad_layers, vec!["TOP", "BOTTOM"]);
    assert_eq!(r.islands.len(), 3);
    v["pads"] = json!([]);
    v["copper"]["fixed"]["vias"] = json!([]);
    v["copper"]["fixed"]["tracks"] = json!([track("neighbor", &[[-4., 0.], [4., 0.]], 1.)]);
    v["holes"] = json!([{"id":"hole","at":{"x":0,"y":0},"hole":{"shape":"round","diameterMm":2,"plated":false}}]);
    let r = run(&v);
    assert_eq!(r.islands.len(), 2);
    assert_eq!(r.regions.len(), 2);
    assert!(r.contacts.iter().all(|c| !c.reliable));
    v["holes"] = json!([]);
    v["cutouts"] = json!([rect(-1., -2., 2., 4.)["outer"]]);
    let r = run(&v);
    assert_eq!(r.islands.len(), 2);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "copper_outside_board"));
}

#[test]
fn cad_connectivity_is_evidence_and_global_rotation_preserves_topology() {
    let mut v = board();
    v["pads"] = json!([rp("pad", 0., 0., 2., 2.)]);
    let mut t = track("far", &[[5., 0.], [8., 0.]], 0.4);
    t["connectedPadIds"] = json!(["pad"]);
    v["copper"]["fixed"]["tracks"] = json!([t]);
    let r = run(&v);
    assert_eq!(r.islands.len(), 2);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "cad_pad_contact_mismatch"));
    v["copper"]["fixed"]["tracks"] = json!([track("touch", &[[0., 0.], [4., 0.]], 0.4)]);
    v["layers"][0]["disableRouting"] = json!(true);
    let baseline = run(&v);
    assert_eq!(baseline.islands.len(), 1);
    for angle in [90f64, 37.] {
        let mut rotated = v.clone();
        let (s, c) = angle.to_radians().sin_cos();
        rotated["pads"][0]["rotationDeg"] = json!(angle);
        for q in rotated["copper"]["fixed"]["tracks"][0]["points"]
            .as_array_mut()
            .unwrap()
        {
            let x = q["x"].as_f64().unwrap();
            let y = q["y"].as_f64().unwrap();
            q["x"] = json!(x * c - y * s);
            q["y"] = json!(x * s + y * c);
        }
        let r = run(&rotated);
        assert_eq!(r.islands.len(), 1);
        near(area(&r), area(&baseline), 0.002);
    }
}

#[test]
fn copper_within_drill_approximation_band_is_not_a_confirmed_barrel_path() {
    let mut v = board();
    // The inscribed 52-gon of a 0.5 mm radius drill has a chord near r=0.4991.
    // This tiny foreign pad is wholly within the mathematical circular drill.
    let a = PI / 52.;
    let mut p = rp(
        "inside-actual-hole",
        0.4995 * a.cos(),
        0.4995 * a.sin(),
        0.0001,
        0.0001,
    );
    p["net"] = json!("B");
    v["pads"] = json!([p]);
    v["copper"]["fixed"]["vias"] = json!([{"id":"v","net":"A","at":{"x":0,"y":0},"diameterMm":1.5,"drillMm":1,"fromLayer":"TOP","toLayer":"BOTTOM","type":"through"}]);
    let r = run(&v);
    assert_eq!(r.statistics.shorts, 0);
    assert!(r.contacts.iter().filter(|c| c.kind == "barrel").all(|c| {
        [&c.a, &c.b].iter().all(|id| {
            r.reliable_regions
                .iter()
                .find(|r| r.id == ***id)
                .unwrap()
                .source
                .kind
                == "via"
        })
    }));
}

#[test]
fn subset_pad_hole_cuts_confirmed_planes_without_inventing_a_barrel() {
    let mut v = board();
    let mut p = rp("top", 0., 0., 2., 2.);
    p["hole"] = json!({"shape":"round","diameterMm":1,"plated":true});
    let mut bottom = rp("bottom", 0., 0., 2., 2.);
    bottom["layers"] = json!(["BOTTOM"]);
    v["pads"] = json!([p, bottom]);
    let r = run(&v);
    assert_eq!(r.status, "partial");
    assert!(!r.holes[0].span_confirmed);
    assert_eq!(r.holes[0].layers, vec!["TOP"]);
    near(r.layers[0].union_area_mm2, 4. - PI / 4., 0.01);
    near(r.layers[3].union_area_mm2, 4., 1e-8);
    assert!(r.contacts.iter().all(|c| c.kind != "barrel"));
    // Straight generated vertices also count against the generation budget.
    let mut o = GeometryOptions::default();
    o.max_generated_vertices = 8;
    v["pads"] = json!([
        rp("a", 0., 0., 1., 1.),
        rp("b", 3., 0., 1., 1.),
        rp("c", 6., 0., 1., 1.)
    ]);
    let r = prepare_json(&v.to_string(), o);
    assert_eq!(r.status, "invalid");
    assert_eq!(r.statistics.generated_vertices, 8);
    assert_eq!(r.excluded.len(), 1);
}

#[test]
fn extreme_finite_input_is_diagnosed_before_curve_generation_or_overlay() {
    let mut v = board();
    v["pads"] = json!([pad(
        "huge",
        0.,
        0.,
        json!({"kind":"circle","diameterMm":1e300})
    )]);
    let r = run(&v);
    assert_eq!(r.status, "invalid");
    assert!(r.regions.is_empty());
    assert_eq!(r.statistics.generated_vertices, 0);
    assert!(r.boolean_error_bound_mm.is_finite());
    v["pads"] = json!([rp("far", 1e300, 0., 1., 1.)]);
    let r = run(&v);
    assert_eq!(r.status, "invalid");
    assert!(r.boolean_error_bound_mm.is_finite());
    assert!(serde_json::to_string(&r).is_ok());
}

// Review regression: a sagitta bound is not a proof of preserved topology.
// The true 2 mm NPTH crosses both edges of the 1.999 mm strip, splitting it.
// At 0.001 mm deviation the inscribed 72-gon leaves thin false edge bridges
// when its support direction is halfway between vertices (2.5 degrees).
#[test]
fn drill_approximation_cannot_create_a_reliable_path_inside_one_source() {
    let mut v = board();
    let angle: f64 = -87.5;
    let (s, c) = angle.to_radians().sin_cos();
    let mut bridge = rp("bridge", 0., 0., 6., 1.999);
    bridge.as_object_mut().unwrap().remove("net");
    bridge["rotationDeg"] = json!(angle);
    let mut left = rp("left", -2. * c, -2. * s, 1., 1.);
    let mut right = rp("right", 2. * c, 2. * s, 1., 1.);
    left["rotationDeg"] = json!(angle);
    right["rotationDeg"] = json!(angle);
    right["net"] = json!("B");
    v["pads"] = json!([bridge, left, right]);
    v["holes"] = json!([{"id":"cut","at":{"x":0.,"y":0.},
        "hole":{"shape":"round","diameterMm":2.,"plated":false}}]);
    for deviation in [0.0001, 0.001] {
        let report = run_e(&v, deviation);
        assert!(
            !report.findings.iter().any(|f| f.code == "physical_short"),
            "Inscribed drill created a false confirmed short at deviation {deviation}: {:?}",
            report.findings
        );
        assert!(
            !report
                .islands
                .iter()
                .any(|i| i.nets.iter().any(|n| n == "A") && i.nets.iter().any(|n| n == "B")),
            "Uncertain intra-source neck must not join the reliable connectivity graph"
        );
    }
}

// Review controls: the same cut in arbitrary board axes, plus a true bridge
// on the other side of the tolerance. The unknown source must remain in both
// graphs; net labels cannot be used to repair topology.
fn neck_coupon(angle: f64, tx: f64, ty: f64, width: f64, slot: bool) -> Value {
    let mut v = board();
    let (s, c) = angle.to_radians().sin_cos();
    let mut bridge = rp("bridge", tx, ty, 6., width);
    bridge.as_object_mut().unwrap().remove("net");
    bridge["rotationDeg"] = json!(angle);
    let mut a = rp("left", tx - 2. * c, ty - 2. * s, 1., 1.);
    let mut b = rp("right", tx + 2. * c, ty + 2. * s, 1., 1.);
    a["rotationDeg"] = json!(angle);
    b["rotationDeg"] = json!(angle);
    b["net"] = json!("B");
    v["pads"] = json!([bridge, a, b]);
    let mut drill = json!({"shape":"round","diameterMm":2.,"plated":false});
    if slot {
        drill["shape"] = json!("slot");
        drill["slotLengthMm"] = json!(3.);
        drill["rotationDeg"] = json!(angle);
    }
    v["holes"] = json!([{"id":"cut","at":{"x":tx,"y":ty},"hole":drill}]);
    v
}
fn ab(islands: &[pcb_analyze::geometry::Island]) -> bool {
    islands
        .iter()
        .any(|i| i.nets.iter().any(|n| n == "A") && i.nets.iter().any(|n| n == "B"))
}
#[test]
fn drill_necks_rotations_translations_slots_and_near_edge() {
    for angle in [-87.5, -42.5, 2.5, 37.1, 92.5, 178.75] {
        for (tx, ty) in [(0., 0.), (13.123, -7.321)] {
            for slot in [false, true] {
                let mut v = neck_coupon(angle, tx, ty, 1.999, slot);
                // Off-centre cut almost reaches the two copper edges.
                let (s, c) = angle.to_radians().sin_cos();
                v["holes"][0]["at"]["x"] = json!(tx - 0.0002 * s);
                v["holes"][0]["at"]["y"] = json!(ty + 0.0002 * c);
                for e in [0.001, 0.0001] {
                    let r = run_e(&v, e);
                    assert!(!ab(&r.islands), "{angle} {slot} {e}: {:?}", r.findings);
                    assert_eq!(r.statistics.shorts, 0);
                    assert!(r
                        .reliable_regions
                        .iter()
                        .any(|r| r.source.source_id.as_deref() == Some("bridge")
                            && r.source.net.is_none()));
                }
            }
        }
    }
}
#[test]
fn two_sided_neck_uncertainty_refines_without_claiming_an_open() {
    // A real 0.0005 mm edge bridge is too narrow to certify at 0.001,
    // but must be preserved as possible and resolved by finer input tolerance.
    let v = neck_coupon(-87.5, 0., 0., 2.001, false);
    let coarse = run(&v);
    assert!(!ab(&coarse.islands));
    assert!(ab(&coarse.possible_islands));
    assert_eq!(coarse.status, "partial");
    assert!(coarse
        .diagnostics
        .iter()
        .any(|d| d.code == "topology_uncertain"
            && d.source_id.as_deref() == Some("bridge")
            && d.path == "pads[0]"));
    assert!(coarse.possible_contacts.iter().all(|c| !c.reliable));
    let fine = run_e(&v, 0.00001);
    assert!(ab(&fine.islands), "{:?}", fine.diagnostics);
    assert_eq!(fine.statistics.shorts, 1);
    let robust = run(&neck_coupon(-87.5, 0., 0., 2.04, false));
    assert!(ab(&robust.islands));
    assert_eq!(robust.statistics.shorts, 1);
    let cut = neck_coupon(-87.5, 0., 0., 1.999, false);
    let coarse_cut = run(&cut);
    assert!(!ab(&coarse_cut.islands));
    assert!(ab(&coarse_cut.possible_islands));
    let fine_cut = run_e(&cut, 0.0001);
    assert!(!ab(&fine_cut.islands));
    assert!(!ab(&fine_cut.possible_islands));
    let limited = prepare_json(
        &v.to_string(),
        GeometryOptions {
            max_generated_vertices: 100,
            ..Default::default()
        },
    );
    assert_eq!(limited.status, "invalid");
    assert!(!ab(&limited.islands));
    assert!(limited.statistics.generated_vertices <= 100);
    assert!(limited
        .diagnostics
        .iter()
        .any(|d| d.code == "envelope_budget" && d.source_id.is_some()));
}

#[test]
fn finer_true_neck_remains_resolvable_on_small_boards() {
    for extent in [5., 10., 20., 100.] {
        let mut v = neck_coupon(-87.5, 0., 0., 2.001, false);
        v["outline"] = points(&[
            [-extent, -extent],
            [extent, -extent],
            [extent, extent],
            [-extent, extent],
        ]);
        let r = run_e(&v, 0.00001);
        assert_ne!(r.status, "invalid", "{extent}: {:?}", r.diagnostics);
        assert!(ab(&r.islands), "{extent}: {:?}", r.diagnostics);
        assert_eq!(r.statistics.shorts, 1);
    }
}
