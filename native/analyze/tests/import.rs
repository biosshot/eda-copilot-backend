use pcb_analyze::{import_board, import_json, input::dto, model, ImportReport, Severity};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn input() -> Value {
    json!({
        "outline":[{"x":-10,"y":-10},{"x":10,"y":-10},{"x":10,"y":10},{"x":-10,"y":10}],
        "cutouts":[],
        "layers":[{"name":"TOP","index":0,"side":"top"},{"name":"I1","index":1,"side":"inner"},{"name":"I2","index":2,"side":"inner"},{"name":"BOTTOM","index":3,"side":"bottom"}],
        "nets":[{"name":"GND"},{"name":"VCC"}],
        "components":[{"id":"native-u1","designator":"U1","at":{"x":2,"y":-3},"rotationDeg":-90,"side":"bottom"}],
        "pads":[{"id":"pad1","component":"U1","number":"1","net":"GND","at":{"x":2.5,"y":-4},"rotationDeg":-90,
            "layers":["TOP","BOTTOM"],"shape":{"kind":"rect","widthMm":2,"heightMm":3},
            "shapesByLayer":{"TOP":{"kind":"circle","diameterMm":2},"BOTTOM":{"kind":"oval","widthMm":2,"heightMm":3}},
            "hole":{"shape":"slot","diameterMm":0.6,"slotLengthMm":1.5,"offset":{"x":0.2,"y":-0.1},"rotationDeg":30,"plated":true}}],
        "keepouts":[],"holes":[{"id":"drill1","at":{"x":-5,"y":2},"hole":{"shape":"round","diameterMm":1,"plated":false}}],
        "rules":{"default":{"clearanceMm":0.2,"edgeClearanceMm":0.3,"minTrackWidthMm":0.1,"preferredTrackWidthMm":0.2,
            "via":{"minDiameterMm":0.5,"preferredDiameterMm":0.6,"minDrillMm":0.2,"preferredDrillMm":0.3}},"nets":[]},
        "copper":{
            "fixed":{"tracks":[{"id":"track1","net":"GND","layer":"BOTTOM","widthMm":0.2,"points":[{"x":2.5,"y":-4},{"x":5,"y":-4}],"connectedPadIds":["pad1"]}],
                "vias":[{"id":"via1","net":"GND","at":{"x":5,"y":-4},"diameterMm":0.6,"drillMm":0.3,"fromLayer":"TOP","toLayer":"BOTTOM","type":"through"}],"zones":[]},
            "editable":{"tracks":[],"vias":[],"zones":[]}}
    })
}
fn run(v: &Value) -> ImportReport {
    import_json(&v.to_string())
}
fn rejected(v: &Value, code: &str) -> ImportReport {
    let r = run(v);
    assert!(r.board.is_none(), "Expected rejection for {code}");
    assert!(r.has_errors());
    assert_eq!(r.accepted.layers, 0);
    assert!(
        r.diagnostics.iter().any(|d| d.code == code),
        "Missing {code}: {:?}",
        r.diagnostics
    );
    r
}
fn same_polygon(a: &model::Polygon, b: &dto::Polygon) {
    assert_eq!(a.outer.len(), b.outer.len());
    for (a, b) in a.outer.iter().zip(&b.outer) {
        assert_eq!((a.x, a.y), (b.x, b.y));
    }
    let holes = b.holes.as_deref().unwrap_or_default();
    assert_eq!(a.holes.len(), holes.len());
    for (a, b) in a.holes.iter().zip(holes) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!((a.x, a.y), (b.x, b.y));
        }
    }
}
fn zone(state: &str) -> Value {
    let mut f = json!({"layer":"TOP","source":"cad","nativePrimitiveIds":["native-fragment"],"status":state});
    if state == "available" {
        f["polygons"] = json!([]);
        f["freshness"] = json!("current");
    } else {
        f["reason"] = json!("native evidence unavailable");
    }
    json!({"id":"zone1","kind":"pour","net":"GND","layers":["TOP"],
        "outline":{"outer":[{"x":0,"y":0},{"x":5,"y":0},{"x":0,"y":5}],"holes":[[{"x":1,"y":1},{"x":1,"y":2},{"x":2,"y":1}]]},
        "filledPolygons":[f],"refillRules":{"status":"unknown","unresolved":["native policy"]}})
}

#[test]
fn preserves_ids_local_geometry_bottom_side_and_ownership() {
    let mut v = input();
    v["copper"]["editable"]["tracks"] =
        json!([{"layer":"TOP","widthMm":0.2,"points":[{"x":-1,"y":-2},{"x":-3,"y":-2}]}]);
    let r = run(&v);
    assert!(!r.has_errors(), "{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "stackup_missing"));
    assert!(r.diagnostics.iter().any(|d| d.code == "source_id_missing"));
    let b = r.board.unwrap();
    assert_eq!(b.components[0].source.id.as_deref(), Some("native-u1"));
    assert_eq!(b.components[0].side, model::Side::Bottom);
    assert_eq!(b.pads[0].at, model::Point { x: 2.5, y: -4.0 });
    assert_eq!(b.pads[0].rotation_deg, -90.0);
    assert_eq!(b.pads[0].component, Some(0));
    assert_eq!(b.pads[0].net, Some(0));
    assert_eq!(b.pads[0].source.path, "pads[0]");
    assert_eq!(
        b.pads[0].layer_shapes.as_ref().unwrap()[&3],
        model::PadShape::Oval {
            width_mm: 2.0,
            height_mm: 3.0
        }
    );
    let h = b.pads[0].drill.as_ref().unwrap();
    assert_eq!(h.offset, Some(model::Point { x: 0.2, y: -0.1 }));
    assert_eq!(h.rotation_deg, Some(30.0));
    assert_eq!(h.shape, model::DrillShape::Slot { length_mm: 1.5 });
    assert_eq!(
        b.drills.as_ref().unwrap()[0].source.id.as_deref(),
        Some("drill1")
    );
    assert_eq!(b.tracks[0].connected_pads, Some(vec![0]));
    assert_eq!(b.tracks[1].ownership, model::Ownership::Editable);
    assert_eq!(b.tracks[1].source.path, "copper.editable.tracks[0]");
    assert!(b.stackup.is_none());
}
#[test]
fn rejects_malformed_unknown_and_duplicate_fields_without_defaulting() {
    for text in [
        "{}",
        "[]",
        "null",
        "{",
        &format!("{} {{}}", input()),
        &input()
            .to_string()
            .replace("\"cutouts\":[]", "\"cutouts\":[],\"cutouts\":[]"),
    ] {
        assert!(import_json(text).has_errors(), "Accepted {text}");
    }
    let mut v = input();
    v["copper"]["fixed"]["vias"][0]["type"] = json!("micro");
    rejected(&v, "invalid_structure");
    let mut v = input();
    v["pads"][0]["inventedField"] = json!(true);
    rejected(&v, "invalid_structure");
    let mut v = input();
    v["pads"][0].as_object_mut().unwrap().remove("shape");
    rejected(&v, "invalid_structure");
}
#[test]
fn rejects_nonfinite_typed_numbers_and_json_overflow() {
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut b: dto::RoutingBoardV2 = serde_json::from_value(input()).unwrap();
        b.pads[0].at.x = n;
        let r = import_board(b);
        assert!(r.has_errors());
        let d = r
            .diagnostics
            .iter()
            .find(|d| d.code == "non_finite")
            .unwrap();
        assert_eq!(d.path, "pads[0].at.x");
        assert_eq!(d.source_id.as_deref(), Some("pad1"));
    }
    let text = input()
        .to_string()
        .replacen("\"widthMm\":0.2", "\"widthMm\":1e999", 1);
    assert!(import_json(&text).has_errors());
    // Numeric fields away from geometry must be checked too.
    let mut b: dto::RoutingBoardV2 = serde_json::from_value(input()).unwrap();
    b.rules.default.impedance_tolerance_percent = Some(f64::NAN);
    assert!(import_board(b).has_errors());
}
#[test]
fn preserves_authored_shapes_layer_indices_and_copper_below_design_rules() {
    let mut v = input();
    for (i, layer) in v["layers"].as_array_mut().unwrap().iter_mut().enumerate() {
        layer["index"] = json!(10 + i * 10);
    }
    v["copper"]["fixed"]["tracks"][0]["widthMm"] = json!(0.05);
    v["rules"]["default"]["via"]["minDiameterMm"] = json!(0.2); // independent minima are not a proposed via
    v["pads"][0]
        .as_object_mut()
        .unwrap()
        .remove("shapesByLayer");
    v["pads"][0]["shape"] =
        json!({"kind":"round-rect","widthMm":2,"heightMm":3,"cornerRadiusMm":0.25});
    let r = run(&v);
    assert!(!r.has_errors(), "{:?}", r.diagnostics);
    let b = r.board.unwrap();
    assert_eq!(b.layers[3].physical_index, 40);
    assert_eq!(b.vias[0].last_layer, 3);
    assert_eq!(b.tracks[0].width_mm, 0.05);
    assert_eq!(
        b.pads[0].common_shape,
        model::PadShape::RoundRect {
            width_mm: 2.0,
            height_mm: 3.0,
            corner_radius_mm: 0.25
        }
    );
    v["pads"][0]["shape"] = json!({"kind":"polygon","polygon":{"outer":[{"x":-1,"y":-2},{"x":1,"y":-2},{"x":0,"y":1}],"holes":[[{"x":-0.1,"y":0},{"x":0.1,"y":0},{"x":0,"y":0.1}]]}});
    let b = run(&v).board.unwrap();
    let model::PadShape::Polygon(p) = &b.pads[0].common_shape else {
        panic!("Expected polygon pad");
    };
    assert_eq!(p.outer[0], model::Point { x: -1.0, y: -2.0 });
    assert_eq!(p.holes.len(), 1);
    let mut v = input();
    v["holes"][0]["id"] = json!("pad1");
    rejected(&v, "duplicate_identifier");
}
#[test]
fn optional_is_not_null_and_layer_shape_keys_cannot_overwrite() {
    for path in [
        "/holes",
        "/pads/0/shapesByLayer",
        "/pads/0/net",
        "/components/0/id",
    ] {
        let mut v = input();
        *v.pointer_mut(path).unwrap() = Value::Null;
        rejected(&v, "invalid_structure");
    }
    let mut v = input();
    v["copper"]["fixed"]["zones"] = json!([zone("available")]);
    v["copper"]["fixed"]["zones"][0]["net"] = Value::Null;
    rejected(&v, "invalid_structure");
    let text = input().to_string().replace(
        "\"shapesByLayer\":{",
        "\"shapesByLayer\":{\"TOP\":{\"kind\":\"circle\",\"diameterMm\":99},",
    );
    assert!(import_json(&text).has_errors());
    let mut v = input();
    v.as_object_mut().unwrap().remove("holes");
    let r = run(&v);
    assert!(!r.has_errors());
    assert!(r.board.unwrap().drills.is_none());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "independent_drills_unspecified"));
    // DTO serialization also preserves omission, including tagged enum fields.
    let b: dto::RoutingBoardV2 = serde_json::from_value(input()).unwrap();
    assert!(!import_json(&serde_json::to_string(&b).unwrap()).has_errors());
}
#[test]
fn resolves_rule_scopes_endpoints_and_rejects_bad_rule_references() {
    let mut v = input();
    let values = v["rules"]["default"].clone();
    v["rules"]["nets"] = json!([{"net":"GND","values":values}]);
    v["rules"]["netClasses"] = json!([{"name":"power","nets":["GND"],"values":values}]);
    v["rules"]["differentialPairs"] = json!([{"id":"pair","positive":"GND","negative":"VCC"}]);
    v["rules"]["matchedGroups"] = json!([
        {"id":"first","nets":["GND"],"toleranceMm":0.1,"targetMm":5,
            "delay":{"tolerancePs":5,"referenceNet":"GND"},
            "endpoints":[{"net":"GND","from":{"component":"U1","pad":"1"},"to":{"component":"U1","pad":"1"}}]},
        {"id":"second","nets":["VCC"],"follow":{"group":"first","offset":-1}}
    ]);
    let r = run(&v);
    assert!(!r.has_errors(), "{:?}", r.diagnostics);
    let b = r.board.unwrap();
    assert_eq!(b.rules.nets.len(), 1);
    assert_eq!(b.rules.net_classes[0].nets, [0]);
    assert_eq!(b.rules.matched_groups[1].follow, Some((0, -1.0)));
    assert_eq!(
        b.rules.matched_groups[0].endpoints.as_ref().unwrap()[0].from_pad,
        0
    );
    let mut bad = v.clone();
    bad["rules"]["nets"][0]["net"] = json!("missing");
    rejected(&bad, "unknown_net");
    let mut bad = v.clone();
    bad["rules"]["default"]["allowedLayers"] = json!(["missing"]);
    rejected(&bad, "unknown_layer");
    let mut bad = v.clone();
    bad["rules"]["matchedGroups"][0]["follow"] = json!({"group":"second","offset":0});
    rejected(&bad, "cyclic_matched_group");
    let mut bad = v.clone();
    bad["rules"]["matchedGroups"][1]["follow"]["group"] = json!("missing");
    rejected(&bad, "unknown_matched_group");
    let mut bad = v.clone();
    bad["rules"]["matchedGroups"][0]["endpoints"][0]["from"]["pad"] = json!("404");
    rejected(&bad, "ambiguous_terminal");
    let mut bad = v.clone();
    bad["pads"][0]["net"] = json!("VCC");
    rejected(&bad, "endpoint_net_mismatch");
    let mut bad = input();
    bad["pads"][0]["net"] = json!("VCC");
    rejected(&bad, "connected_pad_net_mismatch");
}
#[test]
fn rejects_dimensions_references_duplicate_ids_and_layer_order() {
    let cases = [
        ("/pads/0/component", json!("missing"), "unknown_component"),
        ("/pads/0/net", json!("missing"), "unknown_net"),
        ("/pads/0/layers", json!(["UNKNOWN"]), "unknown_layer"),
        ("/pads/0/hole/slotLengthMm", json!(0.1), "invalid_slot"),
        ("/pads/0/shape/widthMm", json!(-1), "invalid_dimension"),
        (
            "/copper/fixed/tracks/0/widthMm",
            json!(0),
            "invalid_dimension",
        ),
        (
            "/copper/fixed/tracks/0/connectedPadIds",
            json!(["missing"]),
            "unknown_pad",
        ),
        (
            "/copper/fixed/vias/0/id",
            json!("track1"),
            "duplicate_identifier",
        ),
        ("/layers/1/index", json!(2), "invalid_layer_order"),
        ("/layers/1/side", json!("bottom"), "invalid_layer_side"),
        (
            "/rules/default/via/preferredDrillMm",
            json!(0.7),
            "inconsistent_rules",
        ),
    ];
    for (path, val, code) in cases {
        let mut v = input();
        *v.pointer_mut(path).unwrap() = val;
        rejected(&v, code);
    }
    let mut v = input();
    v["pads"][0]["shape"] =
        json!({"kind":"round-rect","widthMm":1,"heightMm":2,"cornerRadiusMm":0.6});
    rejected(&v, "invalid_corner_radius");
    let mut v = input();
    v["outline"] = json!([{"x":0,"y":0},{"x":1,"y":0},{"x":2,"y":0}]);
    rejected(&v, "invalid_ring");
    let mut v = input();
    let p = v["pads"][0].clone();
    v["pads"].as_array_mut().unwrap().push(p);
    rejected(&v, "duplicate_identifier");
}
#[test]
fn per_layer_shapes_require_exact_coverage_and_valid_shapes() {
    for key in ["TOP", "BOTTOM"] {
        let mut v = input();
        v["pads"][0]["shapesByLayer"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        rejected(&v, "incomplete_layer_shapes");
    }
    let mut v = input();
    v["pads"][0]["shapesByLayer"]["I1"] = json!({"kind":"circle","diameterMm":1});
    rejected(&v, "incomplete_layer_shapes");
    let mut v = input();
    v["pads"][0]["shapesByLayer"]["TOP"]["diameterMm"] = json!(0);
    rejected(&v, "invalid_dimension");
    let mut v = input();
    v["pads"][0]
        .as_object_mut()
        .unwrap()
        .remove("shapesByLayer");
    assert!(run(&v).board.unwrap().pads[0].layer_shapes.is_none());
}
#[test]
fn through_blind_and_buried_spans_are_explicit_not_repaired() {
    for (from, to) in [("TOP", "I1"), ("I1", "I2"), ("I2", "BOTTOM")] {
        let mut v = input();
        let via = &mut v["copper"]["fixed"]["vias"][0];
        via["fromLayer"] = json!(from);
        via["toLayer"] = json!(to);
        via["type"] = json!("blind-buried");
        let b = run(&v).board.unwrap();
        assert_eq!(b.vias[0].kind, model::ViaKind::BlindBuried);
        assert!(b.vias[0].first_layer < b.vias[0].last_layer);
    }
    for (from, to, kind, code) in [
        ("BOTTOM", "TOP", "through", "invalid_via_span"),
        ("I1", "I1", "blind-buried", "invalid_via_span"),
        ("TOP", "I1", "through", "inconsistent_via_type"),
        ("TOP", "BOTTOM", "blind-buried", "inconsistent_via_type"),
        ("UNKNOWN", "BOTTOM", "through", "unknown_layer"),
    ] {
        let mut v = input();
        let via = &mut v["copper"]["fixed"]["vias"][0];
        via["fromLayer"] = json!(from);
        via["toLayer"] = json!(to);
        via["type"] = json!(kind);
        rejected(&v, code);
    }
}
#[test]
fn fill_states_and_empty_actual_copper_remain_distinct() {
    for state in ["available", "missing", "failed", "unsupported"] {
        let mut v = input();
        v["copper"]["fixed"]["zones"] = json!([zone(state)]);
        let r = run(&v);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(
            r.accepted.available_fills,
            usize::from(state == "available")
        );
        assert_eq!(
            r.accepted.unavailable_fills,
            usize::from(state != "available")
        );
        assert_eq!(
            r.diagnostics
                .iter()
                .filter(|d| d.code == "cad_fill_unavailable")
                .count(),
            usize::from(state != "available")
        );
        let b = r.board.unwrap();
        let z = &b.zones[0];
        assert_eq!(z.source.id.as_deref(), Some("zone1"));
        assert_eq!(z.boundary.holes.len(), 1);
        assert_eq!(
            z.fills[0].native_ids.as_ref().unwrap(),
            &["native-fragment"]
        );
        match &z.fills[0].state {
            model::CadFillState::Available { polygons, .. } => assert!(polygons.is_empty()),
            model::CadFillState::Unavailable { state: s, .. } => assert_eq!(
                *s,
                match state {
                    "missing" => model::Unavailable::Missing,
                    "failed" => model::Unavailable::Failed,
                    _ => model::Unavailable::Unsupported,
                }
            ),
        }
    }
    for freshness in ["stale", "unknown"] {
        let mut v = input();
        let mut z = zone("available");
        z["filledPolygons"][0]["freshness"] = json!(freshness);
        v["copper"]["fixed"]["zones"] = json!([z]);
        let r = run(&v);
        assert!(!r.has_errors());
        assert!(r.diagnostics.iter().any(|d| d.code == "fill_not_current"));
    }
}
#[test]
fn rejects_contradictory_or_missing_fill_states() {
    let mut v = input();
    v["copper"]["fixed"]["zones"] = json!([zone("available")]);
    let mut bad = v.clone();
    bad["copper"]["fixed"]["zones"][0]["filledPolygons"] = json!([]);
    rejected(&bad, "missing_fill_state");
    let mut bad = v.clone();
    let f = bad["copper"]["fixed"]["zones"][0]["filledPolygons"][0].clone();
    bad["copper"]["fixed"]["zones"][0]["filledPolygons"]
        .as_array_mut()
        .unwrap()
        .push(f);
    rejected(&bad, "duplicate_fill_layer");
    let mut bad = v.clone();
    bad["copper"]["fixed"]["zones"][0]["filledPolygons"][0]["layer"] = json!("BOTTOM");
    rejected(&bad, "fill_layer_mismatch");
    let mut bad = v.clone();
    bad["copper"]["fixed"]["zones"][0]["filledPolygons"][0]["status"] = json!("missing");
    rejected(&bad, "invalid_structure");
    let mut bad = v;
    bad["copper"]["fixed"]["zones"][0]["filledPolygons"][0]
        .as_object_mut()
        .unwrap()
        .remove("freshness");
    rejected(&bad, "invalid_structure");
}
#[test]
fn stackup_and_materials_are_validated_without_defaults() {
    let example = include_str!("../../../docs/experimental/analyze/routing-board-v2-example.json");
    let r = import_json(example);
    assert!(!r.has_errors(), "{:?}", r.diagnostics);
    let b = r.board.unwrap();
    let s = b.stackup.unwrap();
    assert_eq!(s.layers.len(), 3);
    assert_eq!(s.via_plating_thickness_um, Some(25.0));
    let mut v: Value = serde_json::from_str(example).unwrap();
    v["stackup"]["layers"][1]["thicknessMm"] = json!(-1);
    rejected(&v, "invalid_dimension");
    let mut v: Value = serde_json::from_str(example).unwrap();
    v["stackup"]["layers"][2]["layer"] = json!("top");
    rejected(&v, "inconsistent_stackup");
    let mut v: Value = serde_json::from_str(example).unwrap();
    v["stackup"]["boardThicknessMm"] = json!(2);
    rejected(&v, "inconsistent_stackup_thickness");
}
#[test]
fn eight_saved_boards_import_with_inventory_and_provenance() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/pcb-analyze");
    let cases = [
        ("NanoScope", 4, 203, 724, 1135, 249, 10),
        ("NanoSpec", 2, 128, 509, 823, 553, 25),
        ("ESPower", 2, 53, 172, 326, 74, 2),
        ("BandAmpV5", 2, 9, 29, 21, 133, 2),
        ("USB_PD_Trigger_5A", 2, 17, 65, 144, 80, 2),
        ("STM32G0_CAN_RS485", 2, 68, 213, 472, 66, 6),
        ("TPS63060DSCR", 2, 15, 39, 73, 24, 6),
        ("ThunderF722", 6, 116, 340, 494, 136, 31),
    ];
    for (name, layers, components, pads, tracks, vias, zones) in cases {
        let text = fs::read_to_string(root.join(name).join(format!("{name}.json"))).unwrap();
        let dto: dto::RoutingBoardV2 = serde_json::from_str(&text).unwrap();
        let report = import_json(&text);
        let errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
        assert_eq!(
            (
                report.accepted.layers,
                report.accepted.components,
                report.accepted.pads,
                report.accepted.tracks,
                report.accepted.vias,
                report.accepted.zones
            ),
            (layers, components, pads, tracks, vias, zones),
            "{name}"
        );
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "stackup_missing"));
        let board = report.board.unwrap();
        assert_eq!(board.outline.len(), dto.outline.len());
        for (a, b) in board.outline.iter().zip(&dto.outline) {
            assert_eq!((a.x, a.y), (b.x, b.y));
        }
        for (a, b) in board.components.iter().zip(&dto.components) {
            assert_eq!(a.source.id, b.id);
            assert_eq!(a.designator, b.designator);
            assert_eq!(
                (a.at.x, a.at.y, a.rotation_deg),
                (b.at.x, b.at.y, b.rotation_deg)
            );
        }
        for (a, b) in board.pads.iter().zip(&dto.pads) {
            assert_eq!(a.source.id.as_deref(), Some(b.id.as_str()));
            assert_eq!(
                (a.at.x, a.at.y, a.rotation_deg),
                (b.at.x, b.at.y, b.rotation_deg)
            );
            assert_eq!(
                a.component.map(|i| board.components[i].designator.as_str()),
                b.component.as_deref()
            );
            assert_eq!(a.net.map(|i| board.nets[i].as_str()), b.net.as_deref());
            assert_eq!(
                a.layers
                    .iter()
                    .map(|i| board.layers[*i].name.as_str())
                    .collect::<Vec<_>>(),
                b.layers.iter().map(String::as_str).collect::<Vec<_>>()
            );
            assert_eq!(
                a.drill.as_ref().map(|h| h.diameter_mm),
                b.hole.as_ref().map(|h| h.diameter_mm)
            );
        }
        for (a, b) in board.tracks.iter().zip(
            dto.copper
                .fixed
                .tracks
                .iter()
                .chain(&dto.copper.editable.tracks),
        ) {
            assert_eq!(a.source.id, b.id);
            assert_eq!(a.width_mm, b.width_mm);
            assert_eq!(board.layers[a.layer].name, b.layer);
            assert_eq!(a.points.len(), b.points.len());
            for (a, b) in a.points.iter().zip(&b.points) {
                assert_eq!((a.x, a.y), (b.x, b.y));
            }
        }
        for (a, b) in board.vias.iter().zip(
            dto.copper
                .fixed
                .vias
                .iter()
                .chain(&dto.copper.editable.vias),
        ) {
            assert_eq!(a.source.id, b.id);
            assert_eq!(
                (a.at.x, a.at.y, a.diameter_mm, a.drill_mm),
                (b.at.x, b.at.y, b.diameter_mm, b.drill_mm)
            );
            assert_eq!(&board.layers[a.first_layer].name, &b.from_layer);
            assert_eq!(&board.layers[a.last_layer].name, &b.to_layer);
        }
        for (a, b) in board.zones.iter().zip(
            dto.copper
                .fixed
                .zones
                .iter()
                .chain(&dto.copper.editable.zones),
        ) {
            let (id, outline, fills) = match b {
                dto::Zone::Pour {
                    id,
                    outline,
                    filled_polygons,
                    ..
                }
                | dto::Zone::Fill {
                    id,
                    outline,
                    filled_polygons,
                    ..
                } => (id, outline, filled_polygons),
            };
            assert_eq!(a.source.id.as_deref(), Some(id.as_str()));
            same_polygon(&a.boundary, outline);
            assert_eq!(a.fills.len(), fills.len());
            for (a, b) in a.fills.iter().zip(fills) {
                match (&a.state, b) {
                    (
                        model::CadFillState::Available {
                            polygons: a_polys,
                            freshness: a_fresh,
                        },
                        dto::FilledPolygons::Available {
                            layer,
                            polygons: b_polys,
                            freshness: b_fresh,
                            native_primitive_ids,
                            ..
                        },
                    ) => {
                        assert_eq!(board.layers[a.layer].name, *layer);
                        assert_eq!(&a.native_ids, native_primitive_ids);
                        assert_eq!(format!("{a_fresh:?}"), format!("{b_fresh:?}"));
                        assert_eq!(a_polys.len(), b_polys.len());
                        for (a, b) in a_polys.iter().zip(b_polys) {
                            same_polygon(a, b);
                        }
                    }
                    _ => panic!("Saved fixture fill state changed: {name}"),
                }
            }
        }
        println!(
            "{name}: {} pads, {} tracks, {} vias, {} zones; {} diagnostics",
            board.pads.len(),
            board.tracks.len(),
            board.vias.len(),
            board.zones.len(),
            report.diagnostics.len()
        );
    }
}
