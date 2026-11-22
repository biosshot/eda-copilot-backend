use pcb_analyze::{
    extraction::local::{
        board_mesh::{self, Electrode, Request},
        mixed_conductor::{self, Owner},
        AllocationBudget,
    },
    import_json,
};
use std::{collections::BTreeSet, time::Duration};

fn relative(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let difference = a
        .iter()
        .zip(b)
        .map(|(a, b)| (a - b).abs())
        .fold(0., f64::max);
    let scale = a.iter().chain(b).map(|x| x.abs()).fold(0., f64::max);
    difference / scale
}

#[test]
fn real_barrel_contact_and_foil_share_conservative_physical_coordinates() {
    let board = import_json(include_str!(
        "../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json"
    ))
    .board
    .unwrap();
    let mut request: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json"
    )).unwrap();
    request.cut_planes.clear();
    request.allow_insulated_copper_cuts = false;
    request.domain_polygon_mm = None;
    request.electrodes = ["s0", "s1", "r0", "r1"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Electrode {
            name: name.into(),
            source_path: Some(format!("pads[{i}]")),
            source_paths: None,
            layer: if i == 0 { "TOP" } else { "BOTTOM" }.into(),
            upper: i != 0,
        })
        .collect();
    request.ports = vec![["s0".into(), "r0".into()], ["s1".into(), "r1".into()]];
    let budget = AllocationBudget {
        max_bytes: 1 << 30,
        max_nodes: 200_000,
        max_tetrahedra: 400_000,
    };
    let wall = Duration::from_secs(20);
    let mesh = board_mesh::build(&board, &request, budget, wall).unwrap();
    let map = mixed_conductor::prepare(&mesh, &request.electrodes, 10_000., budget, wall).unwrap();

    assert_eq!(mesh.barrel_ids.len(), 1);
    assert_eq!(map.owners.len(), mesh.mesh.copper_tetrahedra.len());
    assert!(!map.foils.is_empty());
    assert!(map.current_count() < map.full.current_count());
    assert!(map.potential_count() < map.full.potential_count());

    let mut barrel_z = [f64::INFINITY, f64::NEG_INFINITY];
    let mut barrel_cells = 0;
    for (owner, cell) in map.owners.iter().zip(&mesh.mesh.copper_tetrahedra) {
        if cell.source_ids.contains(&mesh.barrel_ids[0]) {
            barrel_cells += 1;
            assert_eq!(*owner, Owner::Volume);
            for node in cell.nodes {
                let z = mesh.mesh.nodes_m[node][2];
                barrel_z = [barrel_z[0].min(z), barrel_z[1].max(z)];
            }
        }
    }
    assert!(barrel_cells > 0);
    assert_eq!(
        barrel_z,
        [mesh.layer_spans["TOP"][0], mesh.layer_spans["BOTTOM"][1]]
    );

    let electrodes: [usize; 4] = map.electrodes.as_slice().try_into().unwrap();
    assert_eq!(BTreeSet::from(electrodes).len(), 4);
    let component = |node| {
        map.potential_components
            .iter()
            .position(|c| c.contains(&node))
            .unwrap()
    };
    assert_eq!(component(electrodes[0]), component(electrodes[1]));
    assert_eq!(component(electrodes[2]), component(electrodes[3]));
    assert_ne!(component(electrodes[0]), component(electrodes[2]));

    let trace = map.electric_trace().unwrap();
    let surface_nodes: BTreeSet<_> = mesh
        .mesh
        .copper_interfaces
        .iter()
        .flat_map(|face| face.nodes)
        .collect();
    assert_eq!(
        trace.iter().map(|row| row.node).collect::<BTreeSet<_>>(),
        surface_nodes
    );
    assert_eq!(trace.len(), surface_nodes.len());
    assert!(trace.iter().all(|row| row.weights.len() == 1
        && row.weights[0].0 < map.potential_count()
        && row.weights[0].1 == 1.));

    let vector = |n| {
        (0..n)
            .map(|i| ((i * 13 % 19) as f64 - 9.) / 11.)
            .collect::<Vec<_>>()
    };
    let (q, v) = (vector(map.current_count()), vector(map.potential_count()));
    let (full_q, full_v) = (
        map.lift_currents(&q).unwrap(),
        map.lift_potentials(&v).unwrap(),
    );
    let mut a_q = vec![0.; v.len()];
    let mut at_v = vec![0.; q.len()];
    for &(i, j, a) in &map.incidence {
        a_q[i] += a * q[j];
        at_v[j] += a * v[i];
    }
    let full_div = map
        .project_potential_action(&map.full.divergence(&full_q).unwrap())
        .unwrap();
    let full_grad = map
        .project_current_action(&map.full.gradient(&full_v).unwrap())
        .unwrap();
    assert!(relative(&a_q, &full_div) < 1e-12);
    assert!(relative(&at_v, &full_grad) < 1e-12);
    let loss = map
        .resistance_ohm
        .iter()
        .zip(q)
        .map(|(r, q)| r * q * q)
        .sum::<f64>();
    let full_loss = map.full.losses(&full_q).unwrap().iter().sum::<f64>();
    assert!(relative(&[loss], &[full_loss]) < 1e-12);
    eprintln!(
        "{} foil prisms, {barrel_cells} barrel tetrahedra, {} surface trace rows, {} / {} current modes, A relative error {:.2e}, R relative error {:.2e}",
        map.foils.len(), trace.len(), map.current_count(), map.full.current_count(),
        relative(&a_q, &full_div), relative(&[loss], &[full_loss]),
    );
}
