use pcb_analyze::extraction::local::{current_space::Foil, layered_capacitance::*};
use std::time::Duration;
pub fn budget() -> Budget {
    Budget {
        max_nodes: 2000,
        max_local_terms: 20000,
        max_bytes: 512 * 1024 * 1024,
        max_wall: Duration::from_secs(120),
    }
}
pub fn patch(split: bool) -> (Input, PotentialTrace, Vec<Foil>) {
    let xy = vec![[0., 0.], [0.0005, 0.], [0.0005, 0.0005], [0., 0.0005]];
    let tris = vec![[0, 1, 2], [0, 2, 3]];
    let meshes = vec![PlaneInput::Exact {
        source_id: "conforming-patch".into(),
        xy_m: xy.clone(),
        triangles: tris.clone(),
    }];
    let t = 35e-6;
    let h = 1e-3;
    let z = if split {
        vec![-t, 0., h / 2., h, h + t]
    } else {
        vec![-t, 0., h, h + t]
    };
    let metals: Vec<_> = [[-t, 0.], [h, h + t]]
        .iter()
        .enumerate()
        .map(|(i, z)| Metal {
            source_id: format!("foil{i}"),
            polygon_m: xy.clone(),
            z0_m: z[0],
            z1_m: z[1],
        })
        .collect();
    let trace = PotentialTrace {
        potential_nodes: (0..8).map(|i| format!("v{i}")).collect(),
        nodes: metals
            .iter()
            .enumerate()
            .flat_map(|(m, metal)| {
                let xy = xy.clone();
                [metal.z0_m, metal.z1_m].into_iter().flat_map(move |z| {
                    let source = metal.source_id.clone();
                    xy.clone()
                        .into_iter()
                        .enumerate()
                        .map(move |(i, p)| TraceNode {
                            metal_source_id: source.clone(),
                            position_m: [p[0], p[1], z],
                            weights: vec![(4 * m + i, 1.)],
                        })
                })
            })
            .collect(),
    };
    let foils = metals
        .iter()
        .enumerate()
        .map(|(i, m)| Foil {
            source_id: m.source_id.clone(),
            xy_m: xy.clone(),
            triangles: tris.clone(),
            potential_dofs: (0..4).map(|j| 4 * i + j).collect(),
            z_m: [m.z0_m, m.z1_m],
            conductivity_s_per_m: 58e6,
        })
        .collect();
    (
        Input {
            meshes,
            planes: vec![0; z.len()],
            z_m: z,
            xy_bounds_m: [[0., 0.], [0.0005, 0.0005]],
            metals,
            dielectrics: vec![Dielectric {
                source_id: "explicit-vacuum".into(),
                z0_m: -t,
                z1_m: h + t,
                epsilon_r: 1.,
            }],
            exterior: Exterior::InsulatedAnalyticCoupon,
        },
        trace,
        foils,
    )
}
pub fn quotient() -> (Input, PotentialTrace, Vec<Foil>) {
    let (input, mut trace, mut foils) = patch(false);
    trace.potential_nodes = (0..4).map(|i| format!("electrode{i}")).collect();
    for n in &mut trace.nodes {
        let index = n.weights[0].0;
        let mapped = index / 4 * 2
            + if index % 4 == 1 || index % 4 == 2 {
                1
            } else {
                0
            };
        n.weights = vec![(mapped, 1.)];
    }
    for (i, f) in foils.iter_mut().enumerate() {
        f.potential_dofs = vec![2 * i, 2 * i + 1, 2 * i + 1, 2 * i];
    }
    (input, trace, foils)
}

/// Explicit four-electrode neutral drive/observation j=[1,-1] in the two
/// longitudinal ports. This is a general P6 port, not a physical two-contact
/// terminal map, so physical intermodel joins are intentionally unavailable.
pub fn loop_model(input:&pcb_analyze::electrical::coupled::Model)->pcb_analyze::electrical::coupled::Model{
    let mut model=input.clone();let mut b=std::collections::BTreeMap::new();
    for &(i,j,v) in &model.port_incidence{*b.entry(i).or_insert(0.)+=if j==0{v}else{-v};}
    model.port_incidence=b.into_iter().filter(|(_,v)|*v!=0.).map(|(i,v)|(i,0,v)).collect();
    model.port_terminals=None;model
}
