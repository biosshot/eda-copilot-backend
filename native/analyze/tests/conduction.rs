use geo::{Area, BooleanOps, LineString, Polygon};
use pcb_analyze::{
    conduction::{
        self,
        profile::{BarrelProfile, DiameterConvention, Material, PhysicalProfile, StackEntry},
        ConductionOptions, ConductionReport,
    },
    geometry, import_json,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
fn pts(p: &[[f64; 2]]) -> Value {
    json!(p
        .iter()
        .map(|p| json!({"x":p[0],"y":p[1]}))
        .collect::<Vec<_>>())
}
fn board() -> Value {
    json!({"outline":pts(&[[-10.,-10.],[10.,-10.],[10.,10.],[-10.,10.]]),"cutouts":[],"layers":[{"name":"TOP","index":0,"side":"top"},{"name":"I1","index":1,"side":"inner"},{"name":"I2","index":2,"side":"inner"},{"name":"BOTTOM","index":3,"side":"bottom"}],"nets":[{"name":"A"},{"name":"B"}],"components":[],"pads":[],"holes":[],"keepouts":[],"rules":{"default":{"clearanceMm":0.2,"edgeClearanceMm":0.3,"minTrackWidthMm":0.1,"preferredTrackWidthMm":0.2,"via":{"minDiameterMm":0.5,"preferredDiameterMm":0.6,"minDrillMm":0.2,"preferredDrillMm":0.3}},"nets":[]},"copper":{"fixed":{"tracks":[],"vias":[],"zones":[]},"editable":{"tracks":[],"vias":[],"zones":[]}}})
}
fn pad(id: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    json!({"id":id,"number":"1","net":"A","at":{"x":x,"y":y},"rotationDeg":0,"layers":["TOP"],"shape":{"kind":"rect","widthMm":w,"heightMm":h}})
}
fn profile() -> PhysicalProfile {
    let names = ["TOP", "I1", "I2", "BOTTOM"];
    let mut stack = vec![];
    for (i, l) in names.iter().enumerate() {
        if i > 0 {
            stack.push(StackEntry::Dielectric {
                name: format!("gap{i}"),
                thickness_mm: 0.5,
            });
        }
        stack.push(StackEntry::Copper {
            layer: l.to_string(),
            thickness_mm: 0.035,
        });
    }
    PhysicalProfile {
        stackup: Some(stack),
        materials: names
            .iter()
            .map(|l| {
                (
                    l.to_string(),
                    Material {
                        conductivity_s_m: 58e6,
                        state: "constant isotropic at 20 C (coupon assumption)".into(),
                    },
                )
            })
            .collect(),
        barrel: Some(BarrelProfile {
            plating_thickness_mm: Some(0.025),
            material: Some(Material {
                conductivity_s_m: 58e6,
                state: "20 C".into(),
            }),
            diameter_convention: Some(DiameterConvention::FinishedHole),
        }),
    }
}
fn options() -> ConductionOptions {
    let mut o = ConductionOptions::default();
    o.profile = Some(profile());
    o.mesh.min_angle_deg = 0.;
    o.mesh.max_area_mm2 = 0.5;
    o
}
fn run(b: &Value, o: ConductionOptions) -> ConductionReport {
    let i = import_json(&b.to_string());
    assert!(i.board.is_some(), "{:?}", i.diagnostics);
    conduction::prepare_json(&b.to_string(), o)
}
fn tris(r: &ConductionReport) -> Vec<Polygon<f64>> {
    r.domains
        .iter()
        .flat_map(|d| {
            d.triangles.iter().map(|t| {
                let p = t.map(|i| d.nodes[i]);
                let a = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                    - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
                assert!(a > 0. && a.is_finite());
                Polygon::new(LineString::from(vec![p[0], p[1], p[2], p[0]]), vec![])
            })
        })
        .collect()
}
fn area(r: &ConductionReport) -> f64 {
    tris(r).iter().map(|t| t.unsigned_area()).sum()
}
fn near(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() < t, "{a} vs {b}");
}
fn no_overlap(r: &ConductionReport) {
    let t = tris(r);
    for i in 0..t.len() {
        for j in 0..i {
            assert!(t[i].intersection(&t[j]).unsigned_area() < 1e-9);
        }
    }
}
#[test]
fn ready_strip_and_direct_rust_api() {
    let mut b = board();
    b["pads"] = json!([pad("p", 0., 0., 4., 2.)]);
    let r = run(&b, options());
    assert_eq!(r.status, "complete", "{:?}", r.diagnostics);
    near(area(&r), 8., 1e-8);
    assert!(r.domains[0].readiness.dc_ready);
    near(
        r.profile.layers[0].sheet_conductance_s.unwrap(),
        2030.,
        1e-10,
    );
    no_overlap(&r);
    let imported = import_json(&b.to_string()).board.unwrap();
    let g = geometry::prepare(&imported, options().geometry);
    let direct = conduction::prepare_geometry(&imported, &g, options());
    assert_eq!(r.domains[0].triangles, direct.domains[0].triangles);
    let mut v = serde_json::to_value(&r).unwrap();
    v["timings_ms"] = json!({});
    if let Ok(dir) = std::env::var("PCB_ANALYZE_CONDUCTION_REPORT_DIR") {
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            PathBuf::from(dir).join("ready-coupon.json"),
            serde_json::to_vec_pretty(&r).unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn l_union_and_overlapping_terminals_no_double_thickness() {
    let mut b = board();
    b["pads"] = json!([pad("a", 0., 0., 4., 2.), pad("b", 1., 1., 2., 4.)]);
    let r = run(&b, options());
    near(area(&r), 12., 1e-8);
    assert_eq!(r.domains.len(), 1);
    assert_eq!(r.domains[0].terminals.len(), 2);
    no_overlap(&r);
    for t in &r.domains[0].terminals {
        near(t.area_mm2, 8., 1e-7);
        assert!(!t.boundary_edges.is_empty());
    }
}
#[test]
fn annulus_and_island_in_void_preserve_holes() {
    let mut b = board();
    let mut p = pad("ring", 0., 0., 6., 6.);
    p["shape"] = json!({"kind":"circle","diameterMm":6});
    p["hole"] = json!({"shape":"round","diameterMm":2,"plated":false});
    b["pads"] = json!([p]);
    let r = run(&b, options());
    near(area(&r), 8. * std::f64::consts::PI, 0.06);
    assert!(r.domains[0].boundaries.iter().any(|e| e.kind == "hole"));
    assert!(r.domains[0].terminals[0].area_mm2 > 20.);
    for t in tris(&r) {
        for p in t.exterior().points() {
            assert!(p.x().hypot(p.y()) >= 0.998);
        }
    }
    // An independent island inside a zone void remains a separate domain.
    b["pads"] = json!([pad("island", 0., 0., 0.5, 0.5)]);
    b["copper"]["fixed"]["zones"] = json!([{"id":"zone","kind":"fill","net":"A","layers":["TOP"],"outline":{"outer":pts(&[[-4.,-4.],[4.,-4.],[4.,4.],[-4.,4.]])},"filledPolygons":[{"layer":"TOP","source":"cad","status":"available","freshness":"current","polygons":[{"outer":pts(&[[-3.,-3.],[3.,-3.],[3.,3.],[-3.,3.]]),"holes":[pts(&[[-1.,-1.],[-1.,1.],[1.,1.],[1.,-1.]])]}]}]}]);
    let r = run(&b, options());
    assert_eq!(r.domains.len(), 2);
    near(area(&r), 32.25, 1e-7);
    no_overlap(&r);
}
#[test]
fn gap_point_touch_and_net_names_do_not_join() {
    for x in [2., 2.01] {
        let mut b = board();
        b["pads"] = json!([pad("a", 0., 0., 2., 2.), pad("b", x, 2., 2., 2.)]);
        let r = run(&b, options());
        assert_eq!(r.domains.len(), 2, "{:?}", r.diagnostics);
        near_with_report(&r, 8.);
        assert!(r.domains.iter().all(|d| d.nodes.len() > 0));
    }
}
#[test]
fn r1_and_narrow_bridge_never_get_coarse_ready_model() {
    for w in [1.999, 2.001] {
        let mut b = board();
        let mut p = pad("bridge", 0., 0., 6., w);
        p["rotationDeg"] = json!(if w < 2. { -87.5 } else { -90. });
        b["pads"] = json!([p]);
        b["holes"] = json!([{"id":"cut","at":{"x":0,"y":0},"hole":{"shape":"round","diameterMm":2,"plated":false}}]);
        let r = run(&b, options());
        assert!(r.domains.iter().all(|d| !d.readiness.dc_ready));
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code == "drill_topology_uncertain"));
        let mut o = options();
        o.geometry.max_deviation_mm = 1e-5;
        let fine = run(&b, o);
        assert!(fine.domains.iter().all(|d| d.readiness.topology_confirmed));
        assert_eq!(fine.domains.len(), if w < 2. { 2 } else { 1 });
        assert!(
            fine.domains.iter().all(|d| d.readiness.dc_ready),
            "{:?}",
            fine.diagnostics
        );
    }
}
#[test]
fn refinement_changes_actual_elements_and_preserves_subedges() {
    let mut b = board();
    b["pads"] = json!([pad("a", 0., 0., 4., 2.), pad("b", 1., 0., 1., 2.)]);
    let a = run(&b, options());
    let mut o = options();
    o.mesh.max_area_mm2 = 0.05;
    let z = run(&b, o);
    assert!(z.counts.triangles > a.counts.triangles);
    near(area(&a), area(&z), 1e-8);
    assert!(z.domains[0].boundaries.len() > a.domains[0].boundaries.len());
    for t in &z.domains[0].terminals {
        assert!(t.boundary_edges.len() >= 4);
    }
    no_overlap(&z);
}
#[test]
fn budgets_acute_angles_and_invalid_requests_are_explicit() {
    let mut b = board();
    b["pads"] = json!([pad("p", 0., 0., 4., 2.)]);
    let mut o = options();
    o.mesh.max_nodes = 3;
    let r = run(&b, o);
    assert_eq!(r.status, "partial");
    assert_eq!(r.counts.nodes, 0);
    assert!(r.diagnostics.iter().any(|d| d.code == "mesh_unbuilt"));
    let mut o = options();
    o.mesh.max_refinement_steps = 0;
    let r = run(&b, o);
    assert!(!r.domains[0].readiness.mesh_complete);
    assert!(!r.domains[0].triangles.is_empty());
    let mut o = options();
    o.mesh.max_area_mm2 = f64::NAN;
    assert_eq!(run(&b, o).status, "invalid");
    b["pads"][0]["shape"] =
        json!({"kind":"polygon","polygon":{"outer":pts(&[[0.,0.],[5.,0.],[0.,0.01]])}});
    let mut o = options();
    o.mesh.min_angle_deg = 20.;
    o.mesh.max_nodes = 2000;
    o.mesh.max_refinement_steps = 1;
    let r = run(&b, o);
    assert_eq!(r.status, "partial");
    assert!(r.domains.iter().all(|d| !d.readiness.dc_ready));
}
#[test]
fn transformations_order_and_repetition_preserve_invariants() {
    let mut b = board();
    b["pads"] = json!([pad("a", 0., 0., 4., 2.), pad("b", 1., 1., 2., 4.)]);
    let a = run(&b, options());
    let again = run(&b, options());
    assert_eq!(a.domains[0].nodes, again.domains[0].nodes);
    assert_eq!(a.domains[0].triangles, again.domains[0].triangles);
    b["pads"].as_array_mut().unwrap().reverse();
    for p in b["pads"].as_array_mut().unwrap() {
        let x = p["at"]["x"].as_f64().unwrap();
        let y = p["at"]["y"].as_f64().unwrap();
        p["at"] = json!({"x":3.-y,"y":2.+x});
        p["rotationDeg"] = json!(90.);
    }
    let r = run(&b, options());
    near(area(&a), area(&r), 1e-7);
    assert_eq!(a.domains.len(), r.domains.len());
    assert_eq!(a.counts.terminals, r.counts.terminals);
}
#[test]
fn plated_spans_npth_missing_layer_contacts_and_finite_barrels() {
    let mut b = board();
    b["copper"]["fixed"]["vias"] = json!([{"id":"v","net":"A","at":{"x":0,"y":0},"diameterMm":2,"drillMm":1,"fromLayer":"I1","toLayer":"I2","type":"blind-buried"}]);
    let r = run(&b, options());
    assert_eq!(r.barrels.len(), 1);
    let barrel = &r.barrels[0];
    assert!(barrel.ready, "{:?} {:?}", barrel.reasons, r.diagnostics);
    assert_eq!(barrel.contacts.len(), 2);
    assert_eq!(barrel.segments.len(), 1);
    let s = &barrel.segments[0];
    near(s.length_m.unwrap(), 0.000535, 1e-12);
    let section = std::f64::consts::PI * (0.525f64.powi(2) - 0.5f64.powi(2)) * 1e-6;
    near(s.cross_section_m2.unwrap(), section, 1e-14);
    near(s.conductance_s.unwrap(), 58e6 * section / 0.000535, 1e-8);
    assert!(barrel
        .contacts
        .iter()
        .all(|c| c.layer == "I1" || c.layer == "I2"));
    b["copper"]["fixed"]["vias"] = json!([]);
    let mut p = pad("p", 0., 0., 3., 3.);
    p["layers"] = json!(["TOP", "I1", "I2", "BOTTOM"]);
    p["hole"] = json!({"shape":"round","diameterMm":1,"plated":true});
    b["pads"] = json!([p]);
    let r = run(&b, options());
    assert_eq!(r.barrels[0].segments.len(), 3);
    assert_eq!(r.counts.terminals, 4);
    b["pads"][0]["hole"]["plated"] = json!(false);
    assert!(run(&b, options()).barrels.is_empty());
    b["pads"][0]["hole"]["plated"] = json!(true);
    b["pads"][0]["hole"]["shape"] = json!("slot");
    b["pads"][0]["hole"]["slotLengthMm"] = json!(2);
    let r = run(&b, options());
    assert!(!r.barrels[0].ready);
    assert!(r.barrels[0]
        .reasons
        .contains(&"plated_slot_unsupported".into()));
    assert!(r.counts.triangles > 0);
}
#[test]
fn profile_missing_override_validation_and_diameter_convention() {
    let mut b = board();
    b["pads"] = json!([pad("p", 0., 0., 2., 2.)]);
    let mut o = options();
    o.profile = None;
    let r = run(&b, o);
    assert_eq!(r.status, "partial");
    assert!(r.domains[0].sheet_conductance_s.is_none());
    let mut p = profile();
    p.materials.insert(
        "NO_LAYER".into(),
        Material {
            conductivity_s_m: 1.,
            state: "state".into(),
        },
    );
    let mut o = options();
    o.profile = Some(p);
    let r = run(&b, o);
    assert_eq!(r.status, "invalid");
    assert!(r.domains.is_empty());
    let mut o = options();
    o.profile
        .as_mut()
        .unwrap()
        .stackup
        .as_mut()
        .unwrap()
        .reverse();
    assert_eq!(run(&b, o).status, "invalid");
    let mut o = options();
    o.profile
        .as_mut()
        .unwrap()
        .materials
        .get_mut("TOP")
        .unwrap()
        .conductivity_s_m = 0.;
    assert_eq!(run(&b, o).status, "invalid");
    let a = conduction::profile::barrel_area_mm2(1., 0.025, &DiameterConvention::BeforePlating)
        .unwrap();
    near(a, std::f64::consts::PI * (0.25 - 0.475f64.powi(2)), 1e-12);
    assert!(
        conduction::profile::barrel_area_mm2(0.01, 0.025, &DiameterConvention::BeforePlating)
            .is_none()
    );
    // Board thickness is used only when present, never guessed from oz or routing rules.
    b["stackup"] = json!({"layers":[{"kind":"copper","layer":"TOP","thicknessMm":0.07},{"kind":"dielectric","thicknessMm":0.5},{"kind":"copper","layer":"I1","thicknessMm":0.035},{"kind":"dielectric","thicknessMm":0.5},{"kind":"copper","layer":"I2","thicknessMm":0.035},{"kind":"dielectric","thicknessMm":0.5},{"kind":"copper","layer":"BOTTOM","thicknessMm":0.035}]});
    let mut o = options();
    o.profile.as_mut().unwrap().stackup = None;
    let r = run(&b, o);
    near(r.profile.layers[0].thickness_mm.unwrap(), 0.07, 1e-12);
    assert_eq!(
        r.profile.layers[0].geometry_origin.as_deref(),
        Some("board")
    );
    let r = run(&b, options());
    near(r.profile.layers[0].thickness_mm.unwrap(), 0.035, 1e-12);
}
#[test]
fn unknown_fill_is_not_ready_and_invalid_input_has_no_mesh() {
    let mut b = board();
    b["pads"] = json!([pad("p", 0., 0., 2., 2.)]);
    b["copper"]["fixed"]["zones"] = json!([{"id":"z","kind":"fill","layers":["TOP"],"outline":{"outer":pts(&[[-4.,-4.],[4.,-4.],[4.,4.],[-4.,4.]])},"filledPolygons":[{"layer":"TOP","source":"cad","status":"missing","reason":"not captured"}]}]);
    let r = run(&b, options());
    assert!(!r.domains[0].readiness.copper_complete);
    assert!(!r.domains[0].readiness.dc_ready);
    assert_eq!(conduction::prepare_json("{", options()).status, "invalid");
}
#[test]
fn saved_boards_conduction_corpus() {
    let Ok(dir) = std::env::var("PCB_ANALYZE_CONDUCTION_REPORT_DIR") else {
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/pcb-analyze");
    fs::create_dir_all(&dir).unwrap();
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
        let text = fs::read_to_string(root.join(name).join(format!("{name}.json"))).unwrap();
        let mut o = ConductionOptions::default();
        o.mesh.min_angle_deg = 0.;
        let r = conduction::prepare_json(&text, o);
        assert_ne!(r.status, "invalid", "{name}");
        assert!(r.counts.built_domains > 0, "{name}");
        assert_eq!(r.counts.ready_domains, 0, "No material assumptions: {name}");
        fs::write(
            PathBuf::from(&dir).join(format!("{name}.json")),
            serde_json::to_vec(&r).unwrap(),
        )
        .unwrap();
        println!(
            "{name}: {:?}, {} unbuilt",
            r.counts,
            r.counts.domains - r.counts.built_domains
        );
    }
}

fn near_with_report(r: &ConductionReport, expected: f64) {
    assert!(
        (area(r) - expected).abs() < 1e-7,
        "{} {:?}",
        area(r),
        r.diagnostics
    );
}

#[test]
fn two_layer_barrel_missing_contact_and_missing_physics() {
    let mut b = board();
    b["layers"] =
        json!([{"name":"TOP","index":0,"side":"top"},{"name":"BOTTOM","index":1,"side":"bottom"}]);
    let mut p = pad("through", 0., 0., 3., 3.);
    p["layers"] = json!(["TOP", "BOTTOM"]);
    p["hole"] = json!({"shape":"round","diameterMm":1,"plated":true});
    b["pads"] = json!([p]);
    let mut o = options();
    let profile = o.profile.as_mut().unwrap();
    profile.stackup = Some(vec![
        StackEntry::Copper {
            layer: "TOP".into(),
            thickness_mm: 0.035,
        },
        StackEntry::Dielectric {
            name: "gap".into(),
            thickness_mm: 1.5,
        },
        StackEntry::Copper {
            layer: "BOTTOM".into(),
            thickness_mm: 0.035,
        },
    ]);
    profile.materials.retain(|k, _| k == "TOP" || k == "BOTTOM");
    let r = run(&b, o.clone());
    assert_eq!(r.domains.len(), 2);
    assert_eq!(r.barrels[0].segments.len(), 1);
    assert!(r.barrels[0].ready);
    near(r.barrels[0].segments[0].length_m.unwrap(), 0.001535, 1e-12);
    assert_ne!(
        r.barrels[0].contacts[0].domain,
        r.barrels[0].contacts[1].domain
    );
    for which in [0, 1, 2] {
        let mut absent = o.clone();
        let bp = absent.profile.as_mut().unwrap().barrel.as_mut().unwrap();
        match which {
            0 => bp.plating_thickness_mm = None,
            1 => bp.material = None,
            _ => bp.diameter_convention = None,
        };
        let r = run(&b, absent);
        assert!(!r.barrels[0].ready);
        assert!(r.domains.iter().all(|d| !d.readiness.dc_ready));
    }
    let mut board_values = b.clone();
    board_values["stackup"] = json!({"viaPlatingThicknessUm":50,"layers":[{"kind":"copper","layer":"TOP","thicknessMm":0.035},{"kind":"dielectric","thicknessMm":1.5},{"kind":"copper","layer":"BOTTOM","thicknessMm":0.035}]});
    let mut fallback = o.clone();
    fallback
        .profile
        .as_mut()
        .unwrap()
        .barrel
        .as_mut()
        .unwrap()
        .plating_thickness_mm = None;
    let resolved = run(&board_values, fallback);
    near(
        resolved.profile.barrel.plating_thickness_mm.unwrap(),
        0.05,
        1e-12,
    );
    assert_eq!(
        resolved.profile.barrel.plating_origin.as_deref(),
        Some("board")
    );
    assert!(resolved.barrels[0].ready);
    let overridden = run(&board_values, o.clone());
    near(
        overridden.profile.barrel.plating_thickness_mm.unwrap(),
        0.025,
        1e-12,
    );
    assert_eq!(
        overridden.profile.barrel.plating_origin.as_deref(),
        Some("profile")
    );
    b["pads"][0]["shapesByLayer"] =
        json!({"TOP":{"kind":"circle","diameterMm":3},"BOTTOM":{"kind":"circle","diameterMm":0.8}});
    let r = run(&b, o);
    assert_eq!(r.barrels[0].contacts.len(), 1);
    assert!(r.barrels[0].segments.is_empty());
    assert!(!r.barrels[0].ready);
}

#[test]
fn unrepresentable_physical_coefficients_are_invalid_not_ready() {
    let mut b = board();
    b["pads"] = json!([pad("p", 0., 0., 2., 2.)]);
    let mut o = options();
    o.profile
        .as_mut()
        .unwrap()
        .materials
        .get_mut("TOP")
        .unwrap()
        .conductivity_s_m = 1e-320;
    let r = run(&b, o);
    assert_eq!(r.status, "invalid");
    assert!(r.domains.is_empty());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "invalid_sheet_coefficient"));
    assert!(
        conduction::profile::barrel_area_mm2(1., 1e-320, &DiameterConvention::FinishedHole)
            .is_none()
    );
}

#[test]
fn blind_buried_and_through_vias_keep_only_their_physical_spans() {
    for (from, to, kind, n) in [
        ("TOP", "I1", "blind-buried", 2),
        ("I1", "I2", "blind-buried", 2),
        ("TOP", "BOTTOM", "through", 4),
    ] {
        let mut b = board();
        b["copper"]["fixed"]["vias"] = json!([{"id":"v","net":"A","at":{"x":0,"y":0},"diameterMm":2,"drillMm":1,"fromLayer":from,"toLayer":to,"type":kind}]);
        let r = run(&b, options());
        let barrel = &r.barrels[0];
        assert!(barrel.ready);
        assert_eq!(barrel.contacts.len(), n);
        assert_eq!(barrel.segments.len(), n - 1);
        assert_eq!(barrel.segments[0].from_layer, from);
        assert_eq!(barrel.segments.last().unwrap().to_layer, to);
        let domains: std::collections::BTreeSet<_> =
            barrel.contacts.iter().map(|c| c.domain.clone()).collect();
        assert_eq!(domains.len(), n);
    }
    let mut b = board();
    b["pads"] = json!([pad("p", 0., 0., 2., 2.)]);
    let mut o = options();
    o.profile
        .as_mut()
        .unwrap()
        .stackup
        .as_mut()
        .unwrap()
        .push(StackEntry::Copper {
            layer: "TOP".into(),
            thickness_mm: 0.035,
        });
    let r = run(&b, o);
    assert_eq!(r.status, "invalid");
    assert!(r.domains.is_empty());
}

#[test]
fn unknown_drill_span_cannot_hide_behind_global_partial_status() {
    let mut b = board();
    let mut p = pad("unknown-span", 0., 0., 3., 3.);
    p["hole"] = json!({"shape":"round","diameterMm":1,"plated":true});
    let mut other = pad("inner-copper", 0., 0., 4., 4.);
    other["layers"] = json!(["I1"]);
    b["pads"] = json!([p, other]);
    let r = run(&b, options());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "drill_span_unconfirmed"));
    assert!(r
        .domains
        .iter()
        .all(|d| !d.readiness.copper_complete && !d.readiness.dc_ready));
}

fn contact_coupon() -> (Value, ConductionOptions) {
    let input: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/conduction-contact-coupon.json"
    ))
    .unwrap();
    (
        input["board"].clone(),
        serde_json::from_value(input["options"].clone()).unwrap(),
    )
}

#[test]
fn confirmed_contact_lost_to_mesh_budget_blocks_barrel_and_only_dependents() {
    let (b, o) = contact_coupon();
    let g = geometry::prepare_json(&b.to_string(), o.geometry);
    let expected: std::collections::BTreeSet<_> = g
        .contacts
        .iter()
        .filter(|c| c.kind == "barrel" && c.reliable)
        .flat_map(|c| [c.a.clone(), c.b.clone()])
        .collect();
    assert_eq!(expected.len(), 4, "{:?}", g.diagnostics);
    let r = run(&b, o);
    let top = r
        .domains
        .iter()
        .find(|d| d.layer == "TOP" && d.source_paths.contains(&"pads[0]".into()))
        .unwrap();
    assert!(!top.readiness.mesh_complete);
    assert_eq!(r.barrels[0].contacts.len(), 3, "{:?}", r.diagnostics);
    assert!(
        !r.barrels[0].ready,
        "Mesh loss must not erase a confirmed branch: {:?}",
        r.barrels[0]
    );
    assert_eq!(r.barrels[0].expected_contacts.len(), 4);
    let missing = r.barrels[0]
        .expected_contacts
        .iter()
        .find(|c| c.layer == "TOP")
        .unwrap();
    assert!(!missing.ready);
    assert_eq!(missing.domain_ids, vec![top.id.clone()]);
    assert!(missing
        .reasons
        .contains(&format!("mesh_unbuilt:{}", top.id)));
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "barrel_contact_unready"
            && d.path == "pads[0]"
            && d.message.contains(&top.id)));
    assert!(r
        .domains
        .iter()
        .filter(|d| d.source_paths.contains(&"pads[0]".into()))
        .all(|d| !d.readiness.dc_ready));
    assert!(
        r.domains
            .iter()
            .find(|d| d.source_paths.contains(&"pads[1]".into()))
            .unwrap()
            .readiness
            .dc_ready
    );
}

#[test]
fn barrel_contact_warning_blocks_own_region_confirmation() {
    let (mut b, mut o) = contact_coupon();
    o.mesh.max_nodes = 50_000;
    b["pads"][0]["shapesByLayer"]["TOP"] = json!({"kind":"circle","diameterMm":1.005});
    let g = geometry::prepare_json(&b.to_string(), o.geometry);
    assert!(
        g.diagnostics
            .iter()
            .any(|d| d.code == "barrel_contact_uncertain" && d.path == "pads[0]"),
        "{:?}",
        g.diagnostics
    );
    let r = run(&b, o);
    assert!(r
        .domains
        .iter()
        .filter(|d| d.source_paths.contains(&"pads[0]".into()))
        .all(|d| !d.readiness.topology_confirmed));
    assert!(!r.barrels[0].ready);
    assert!(r.barrels[0]
        .reasons
        .contains(&"barrel_contact_uncertain:pads[0]".into()));
    assert!(
        r.barrels[0].contacts.is_empty(),
        "own regions must not bypass known uncertainty"
    );
    assert!(
        r.domains
            .iter()
            .find(|d| d.source_paths.contains(&"pads[1]".into()))
            .unwrap()
            .readiness
            .dc_ready
    );
}

#[test]
fn absent_geometry_contact_does_not_require_copper_on_every_span_layer() {
    let (mut b, mut o) = contact_coupon();
    o.mesh.max_nodes = 50_000;
    b["pads"][0]["shapesByLayer"]["TOP"] = json!({"kind":"circle","diameterMm":0.8});
    let g = geometry::prepare_json(&b.to_string(), o.geometry);
    assert!(!g
        .diagnostics
        .iter()
        .any(|d| d.code == "barrel_contact_uncertain"));
    let r = run(&b, o);
    let barrel = &r.barrels[0];
    assert!(barrel.ready, "{barrel:?}");
    assert_eq!(barrel.expected_contacts.len(), 3);
    assert!(barrel
        .expected_contacts
        .iter()
        .all(|c| c.layer != "TOP" && c.ready));
    assert_eq!(barrel.contacts.len(), 3);
    assert_eq!(barrel.segments.len(), 2);
    assert!(r.domains.iter().all(|d| d.readiness.dc_ready));
}
