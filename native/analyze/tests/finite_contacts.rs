//! Source-piece contacts and holes through the actual field/current assembly.
//! These are insulated finite-foil controls, not ordinary plated-via admission.
use pcb_analyze::{
    electrical::{coupled::System, Counters},
    extraction::{
        local::{current_space, layered_capacitance::*},
        EPS0,
    },
};
use std::time::Duration;
#[path = "../../../tests/fixtures/analysis-local-extraction/current-space/fixture.rs"]
mod fixture;

fn split() -> (Input, PotentialTrace, Vec<current_space::Foil>) {
    let (mut input, _, _) = fixture::quotient();
    let l = 0.0005;
    let xy = vec![
        [0., 0.],
        [l / 2., 0.],
        [l, 0.],
        [0., l],
        [l / 2., l],
        [l, l],
    ];
    let triangles = vec![[0, 1, 4], [0, 4, 3], [1, 2, 5], [1, 5, 4]];
    input.meshes = vec![PlaneInput::Exact {
        source_id: "split-continuous-foil".into(),
        xy_m: xy.clone(),
        triangles,
    }];
    let mut trace = PotentialTrace {
        potential_nodes: (0..6).map(|i| format!("v{i}")).collect(),
        nodes: vec![],
    };
    let mut foils = vec![];
    let mut metals = vec![];
    for (layer, old) in input.metals.iter().enumerate() {
        for (part, ids) in [[0, 1, 4, 3], [1, 2, 5, 4]].into_iter().enumerate() {
            let source = format!("source-{layer}-{part}");
            let p: Vec<_> = ids.iter().map(|&i| xy[i]).collect();
            let dofs: Vec<_> = ids.iter().map(|&i| 3 * layer + i % 3).collect();
            metals.push(Metal {
                source_id: source.clone(),
                polygon_m: p.clone(),
                holes_m: vec![],
                z0_m: old.z0_m,
                z1_m: old.z1_m,
            });
            for z in [old.z0_m, old.z1_m] {
                for (i, &p) in p.iter().enumerate() {
                    trace.nodes.push(TraceNode {
                        metal_source_id: source.clone(),
                        position_m: [p[0], p[1], z],
                        weights: vec![(dofs[i], 1.)],
                    });
                }
            }
            foils.push(current_space::Foil {
                source_id: source,
                xy_m: p,
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                potential_dofs: dofs,
                z_m: [old.z0_m, old.z1_m],
                conductivity_s_per_m: 58e6,
            });
        }
    }
    input.metals = metals;
    (input, trace, foils)
}
#[test]
fn touching_sources_retain_trace_identity_and_dc_conservation_without_net_welds() {
    let (input, trace, foils) = split();
    let field = extract_traced(&input, &trace, fixture::budget()).unwrap();
    // Every shared geometric surface has both source identities, one physical
    // potential interpolation and one reaction, never duplicated electric energy.
    assert_eq!(field.surface_traces().len(), trace.nodes.len());
    let mut space = current_space::assemble(
        &foils,
        &field,
        &[[0, 2], [3, 5]],
        1e5,
        current_space::Budget {
            max_bytes: 512 * 1024 * 1024,
            max_wall: Duration::from_secs(120),
        },
    )
    .unwrap();
    let mut p6 = System::compile(&space.model, 2, None).unwrap();
    let rhs = p6.port_rhs(&[1., -1.]);
    let x = p6
        .solve(0., rhs, &vec![0.; 22], &mut Counters::default())
        .unwrap();
    let exact_r = 1. / (58e6 * 35e-6);
    assert!((x[0] - x[2] - exact_r).abs() < exact_r * 1e-9);
    assert!((x[5] - x[3] - exact_r).abs() < exact_r * 1e-9);
    assert!((p6.joule_loss(&x) - 2. * exact_r).abs() < exact_r * 1e-9);
    for (k, j) in space.recover_density(&x[6..]).unwrap().iter().enumerate() {
        let expected = if k < 4 { 1. } else { -1. } / (0.0005 * 35e-6);
        assert!((j[0] - expected).abs() < expected.abs() * 1e-9);
        assert!(j[1].abs() < expected.abs() * 1e-9);
    }
    // Independent exact P1 tensor energy for a uniform normal field across
    // 0.5mm-square vacuum. Source decomposition changes no C energy.
    let v = [1., 1., 1., 0., 0., 0.];
    let energy = field.recover(&v).unwrap().energy_j;
    assert!((energy - 0.5 * EPS0 * 0.0005f64.powi(2) / 0.001).abs() < energy * 1e-10);
    // Different source names never authorize contacts by themselves.
    let mut incompatible = trace.clone();
    let n = incompatible
        .nodes
        .iter_mut()
        .find(|n| n.metal_source_id == "source-0-1" && n.position_m[0] == 0.00025)
        .unwrap();
    n.weights = vec![(0, 1.)];
    assert!(extract_traced(&input, &incompatible, fixture::budget())
        .err()
        .unwrap()
        .contains("identical explicit"));
    assert!(extract(&input, fixture::budget())
        .err()
        .unwrap()
        .contains("identical explicit"));
    // Keep otherwise unused helper reachable for the fixture's shared build.
    space.model = fixture::loop_model(&space.model);
}

#[test]
fn finite_holes_are_retained_and_missing_constraints_or_invalid_voids_refuse() {
    let (mut input, _, _) = fixture::patch(false);
    let outer = input.metals[0].polygon_m.clone();
    let hole = vec![
        [0.00015, 0.00015],
        [0.00035, 0.00015],
        [0.00035, 0.00035],
        [0.00015, 0.00035],
    ];
    let mut xy = outer;
    xy.extend(hole.clone());
    xy.push([0.00025, 0.00025]);
    let mut triangles = vec![];
    for i in 0..4 {
        let j = (i + 1) % 4;
        triangles.extend([[i, j, 4 + j], [i, 4 + j, 4 + i], [4 + i, 4 + j, 8]]);
    }
    input.meshes = vec![PlaneInput::Exact {
        source_id: "actual-constrained-hole".into(),
        xy_m: xy,
        triangles,
    }];
    for metal in &mut input.metals {
        metal.holes_m = vec![hole.clone()];
    }
    let open = extract(&input, fixture::budget()).unwrap();
    let restored: Input = serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
    assert_eq!(restored.metals[0].holes_m, input.metals[0].holes_m);
    let mut filled = input.clone();
    for metal in &mut filled.metals {
        metal.holes_m.clear();
    }
    let solid = extract(&filled, fixture::budget()).unwrap();
    // Independent count: four hole triangles per finite metal slab now contain
    // dielectric. 4 triangles * 2 slabs * 36 tensor entries = 288 extra terms.
    assert_eq!(open.plan.local_terms - solid.plan.local_terms, 288);
    assert!(open.capacitance_f[0][0] < solid.capacitance_f[0][0]);
    open.recover(&[1., 0.]).unwrap();
    let mut bad = input.clone();
    bad.metals[0].holes_m[0][0] = [-1., 0.];
    assert!(extract(&bad, fixture::budget())
        .err()
        .unwrap()
        .contains("COPPER_INPUT"));
    let mut absent = input.clone();
    absent.meshes = fixture::patch(false).0.meshes;
    assert!(extract(&absent, fixture::budget())
        .err()
        .unwrap()
        .contains("COPPER_TRACE"));
}

#[test]
fn authored_equal_potentials_do_not_turn_point_contacts_into_finite_junctions() {
    let (mut input, _, _) = fixture::patch(false);
    let l = 0.0005;
    let xy: Vec<_> = (0..3)
        .flat_map(|y| (0..3).map(move |x| [x as f64 * l / 2., y as f64 * l / 2.]))
        .collect();
    let mut triangles = vec![];
    for y in 0..2 {
        for x in 0..2 {
            let i = y * 3 + x;
            triangles.extend([[i, i + 1, i + 4], [i, i + 4, i + 3]]);
        }
    }
    input.meshes = vec![PlaneInput::Exact {
        source_id: "point-contact-control".into(),
        xy_m: xy.clone(),
        triangles,
    }];
    let bottom = input.metals[0].clone();
    let top = input.metals[1].clone();
    input.metals = vec![
        Metal {
            source_id: "corner-a".into(),
            polygon_m: vec![xy[0], xy[1], xy[4], xy[3]],
            holes_m: vec![],
            z0_m: bottom.z0_m,
            z1_m: bottom.z1_m,
        },
        Metal {
            source_id: "corner-b".into(),
            polygon_m: vec![xy[4], xy[5], xy[8], xy[7]],
            holes_m: vec![],
            z0_m: bottom.z0_m,
            z1_m: bottom.z1_m,
        },
        top,
    ];
    let mut trace = PotentialTrace {
        potential_nodes: vec!["shared-authored-electrode".into(), "return".into()],
        nodes: vec![],
    };
    for (m, metal) in input.metals.iter().enumerate() {
        for z in [metal.z0_m, metal.z1_m] {
            let nodes = if m < 2 {
                metal.polygon_m.clone()
            } else {
                xy.clone()
            };
            for p in nodes {
                trace.nodes.push(TraceNode {
                    metal_source_id: metal.source_id.clone(),
                    position_m: [p[0], p[1], z],
                    weights: vec![(if m < 2 { 0 } else { 1 }, 1.)],
                });
            }
        }
    }
    let field = extract_traced(&input, &trace, fixture::budget()).unwrap();
    let traces = field.surface_traces();
    let corner_owners: Vec<_> = traces
        .iter()
        .filter(|node| node.position_m[..2] == xy[4] && node.metal_source_id.starts_with("corner-"))
        .map(|node| node.metal_source_id.as_str())
        .collect();
    assert_eq!(corner_owners, ["corner-a", "corner-a"]);
    assert!(traces.iter().any(|node| node.metal_source_id == "corner-b"));
    assert!(field.recover(&[1., 0.]).unwrap().energy_j > 0.);
}

#[test]
fn retained_and_closing_void_buffers_are_admitted_before_polygon_allocation() {
    let (mut input, _, _) = fixture::patch(false);
    let count = 16384;
    input.metals[0].holes_m = vec![(0..count)
        .map(|i| {
            let angle = 2. * std::f64::consts::PI * i as f64 / count as f64;
            [
                0.00025 + 0.0001 * angle.cos(),
                0.00025 + 0.0001 * angle.sin(),
            ]
        })
        .collect()];
    let mut limit = fixture::budget();
    limit.max_bytes = 1_000_000;
    assert!(extract(&input, limit)
        .err()
        .unwrap()
        .contains("MEMORY_LIMIT"));
}
