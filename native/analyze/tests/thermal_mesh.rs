#[path = "../src/thermal/mesh.rs"]
mod mesh;
use pcb_analyze::{
    geometry::{prepare, GeometryOptions},
    import_json,
};
use spade::Triangulation;

#[test]
fn controller_projected_contours() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json");
    let board = import_json(&std::fs::read_to_string(path).unwrap())
        .board
        .unwrap();
    for deviation in [0.01, 0.005] {
        let g = prepare(
            &board,
            GeometryOptions {
                max_deviation_mm: deviation,
                ..GeometryOptions::default()
            },
        );
        let mut lines = vec![];
        for p in g
            .outline
            .iter()
            .chain(g.layers.iter().flat_map(|l| l.copper.iter()))
        {
            for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {
                for i in 0..ring.len() {
                    lines.push((ring[i], ring[(i + 1) % ring.len()]));
                }
            }
        }
        let a = mesh::arrange(lines, 4., 50000 / 3, || Ok(())).unwrap();
        assert!(a.max_deviation_mm < 1e-9);
        let c = a.cdt;
        eprintln!(
            "deviation {deviation} points {} constraints {}",
            c.num_vertices(),
            c.num_constraints()
        );
    }
}

#[test]
fn shared_crossing_vertex_collinear_overlaps_and_budget_rejection() {
    let lines = vec![
        ([-1., 0.], [1., 0.]),
        ([0., -1.], [0., 1.]),
        ([-1., 0.], [0., 0.]),
    ];
    let arranged = mesh::arrange(lines.clone(), 4., 10, || Ok(())).unwrap();
    assert_eq!(arranged.cdt.num_vertices(), 5);
    assert_eq!(arranged.cdt.num_constraints(), 4);
    assert_eq!(arranged.max_deviation_mm, 0.);
    assert!(mesh::arrange(lines, 4., 4, || Ok(())).is_err());
    assert!(mesh::arrange(
        vec![([0., 0.], [1., 1.])],
        1.,
        10,
        || Err("deadline".into())
    )
    .is_err());
}

#[test]
fn captured_mesh_conditioning_diagnostics() {
    for (fixture, deviation) in [
        ("TPS63060DSCR", 0.001),
        ("BandAmpV5", 0.001),
        ("STM32G0_CAN_RS485", 0.01),
        ("STM32G0_CAN_RS485", 0.005),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tests/pcb-analyze/{fixture}/{fixture}.json"));
        let board = import_json(&std::fs::read_to_string(path).unwrap())
            .board
            .unwrap();
        let g = prepare(
            &board,
            GeometryOptions {
                max_deviation_mm: deviation,
                ..GeometryOptions::default()
            },
        );
        let mut lines = vec![];
        for p in g
            .outline
            .iter()
            .chain(g.layers.iter().flat_map(|l| l.copper.iter()))
        {
            for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {
                for i in 0..ring.len() {
                    lines.push((ring[i], ring[(i + 1) % ring.len()]));
                }
            }
        }
        let a = mesh::arrange(lines, 4., 50000 / 3, || Ok(())).unwrap();
        let mut minimum = f64::INFINITY;
        let mut shape = 0.0f64;
        let mut thin = 0;
        for face in a.cdt.inner_faces() {
            let p = face.vertices().map(|v| v.position());
            let det = ((p[1].x - p[0].x) * (p[2].y - p[0].y)
                - (p[1].y - p[0].y) * (p[2].x - p[0].x))
                .abs();
            let edge = (0..3)
                .map(|i| {
                    let q = p[(i + 1) % 3];
                    (p[i].x - q.x).powi(2) + (p[i].y - q.y).powi(2)
                })
                .fold(0., f64::max);
            minimum = minimum.min(det);
            shape = shape.max(edge / det);
            if det < 1e-10 {
                thin += 1;
            }
        }
        eprintln!("{fixture} deviation={deviation} minDet={minimum:e} maxShape={shape:e} thin={thin} constraint deviation={}",a.max_deviation_mm);
    }
}
