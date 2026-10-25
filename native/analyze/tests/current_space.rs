//! Conforming spatial traces and coupled foil current space; no PCB/via claim.
use pcb_analyze::{
    electrical::{
        coupled::{self, System},
        Counters,
    },
    extraction::{
        local::{
            current_space::{self},
            layered_capacitance::*,
        },
        EPS0,
    },
};
use std::time::Duration;
#[path = "../../../tests/fixtures/analysis-local-extraction/current-space/fixture.rs"]
mod fixture;
use fixture::*;
fn close(x: f64, y: f64, scale: f64, tol: f64) {
    assert!(
        (x - y).abs() <= tol * scale,
        "actual={x:e} reference={y:e} scale={scale:e}"
    );
}
#[test]
fn spatial_surface_trace_matches_independent_mass_stiffness_tensor_and_recovers_fields() {
    let (input, trace, _) = patch(false);
    let s = extract_traced(&input, &trace, budget()).unwrap();
    // Independent exact P1 triangle mass and stiffness, then analytic z tensor.
    let l = 0.0005;
    let area = l * l / 2.;
    let h = 0.001;
    let mut mass = [[0.; 4]; 4];
    let mut stiff = [[0.; 4]; 4];
    let tris = [[0, 1, 2], [0, 2, 3]];
    let gradients = [
        [[-1. / l, 0.], [1. / l, -1. / l], [0., 1. / l]],
        [[0., -1. / l], [1. / l, 0.], [-1. / l, 1. / l]],
    ];
    for (k, t) in tris.iter().enumerate() {
        for i in 0..3 {
            for j in 0..3 {
                mass[t[i]][t[j]] += area * (if i == j { 2. } else { 1. }) / 12.;
                stiff[t[i]][t[j]] += area
                    * (gradients[k][i][0] * gradients[k][j][0]
                        + gradients[k][i][1] * gradients[k][j][1]);
            }
        }
    }
    let mut reference = vec![vec![0.; 8]; 8];
    for i in 0..8 {
        for j in 0..8 {
            reference[i][j] = EPS0
                * (stiff[i % 4][j % 4] * h * (if i / 4 == j / 4 { 1. / 3. } else { 1. / 6. })
                    + mass[i % 4][j % 4] / h * (if i / 4 == j / 4 { 1. } else { -1. }));
        }
    }
    let scale = (0..8).map(|i| reference[i][i]).fold(0., f64::max);
    let energy = s.energy_capacitance_f().unwrap();
    for i in 0..8 {
        for j in 0..8 {
            close(s.capacitance_f[i][j], reference[i][j], scale, 1e-10);
            close(energy[i][j], reference[i][j], scale, 1e-10);
            assert_eq!(energy[i][j], energy[j][i]);
        }
    }
    for v in [
        vec![0.; 8],
        vec![2.; 8],
        vec![0., 1., 2., 1., 0., 1., 2., 1.],
        vec![1., -2., 3., -4., 2., 1., -1., -2.],
    ] {
        let r = s.recover(&v).unwrap();
        let exact = (0..8)
            .flat_map(|i| (0..8).map(move |j| (i, j)))
            .map(|(i, j)| v[i] * reference[i][j] * v[j] / 2.)
            .sum::<f64>();
        close(r.energy_j, exact, scale * 100., 1e-10);
        assert!(r.normalized_charge_balance <= 1e-9);
    }
    // Nontrivial conservative interpolation, independently transformed from
    // the analytic tensor above (not from another native extraction result).
    let mut weighted=trace.clone();
    for node in &mut weighted.nodes {
        if node.weights[0].0==0 {node.weights=vec![(0,0.25),(1,0.75)];}
    }
    let w=extract_traced(&input,&weighted,budget()).unwrap();
    let mut transfer=vec![vec![0.;8];8];
    for i in 0..8 {transfer[i][i]=1.;}
    transfer[0][0]=0.25;transfer[0][1]=0.75;
    for i in 0..8 {for j in 0..8 {
        let mut expected=0.;
        for k in 0..8 {for l in 0..8 {expected+=transfer[k][i]*reference[k][l]*transfer[l][j];}}
        close(w.capacitance_f[i][j],expected,scale,1e-10);
    }}
    w.recover(&[1.,-2.,3.,-4.,2.,1.,-1.,-2.]).unwrap();
    // Uniform normal field is exact after separate z refinement.
    // Nonconstant tangential traces need domain/mesh accuracy validation separately.
    let (refined, tr, _) = patch(true);
    let refined = extract_traced(&refined, &tr, budget()).unwrap();
    let v = [1., 1., 1., 1., 0., 0., 0., 0.];
    let e = EPS0 * l * l / (2. * h);
    close(s.recover(&v).unwrap().energy_j, e, e, 1e-10);
    close(refined.recover(&v).unwrap().energy_j, e, e, 1e-10);
    // Project to constant metal potentials; no change to legacy equipotential path.
    let old = extract(&input, budget()).unwrap();
    for a in 0..2 {
        for b in 0..2 {
            let sum: f64 = (4 * a..4 * a + 4)
                .flat_map(|i| (4 * b..4 * b + 4).map(move |j| (i, j)))
                .map(|(i, j)| s.capacitance_f[i][j])
                .sum();
            close(sum, old.capacitance_f[a][b], EPS0 * l * l / h, 1e-10);
        }
    }
}
#[test]
fn trace_missing_duplicate_nonconservative_unknown_and_unmatched_nodes_refuse() {
    let (input, trace, _) = patch(false);
    for case in 0..7 {
        let mut t = trace.clone();
        match case {
            0 => {
                t.nodes.pop();
            }
            1 => t.nodes.push(t.nodes[0].clone()),
            2 => t.nodes[0].weights[0].1 = 0.5,
            3 => t.nodes[0].metal_source_id = "unknown".into(),
            4 => t.nodes[0].position_m[0] += 1e-12,
            5 => t.potential_nodes.push("unused".into()),
            _ => t.nodes[0].weights.push((0, 1.)),
        }
        assert!(extract_traced(&input, &t, budget())
            .err()
            .unwrap()
            .contains("FIELD_TRACE"));
    }
    let mut b = budget();
    b.max_bytes = 1;
    assert!(extract_traced(&input, &trace, b)
        .err()
        .unwrap()
        .contains("MEMORY_LIMIT"));
}
#[test]
fn spatial_foil_model_recovers_dc_energy_projection_and_structural_ac_gauge() {
    let (input, trace, foils) = quotient();
    let electric = extract_traced(&input, &trace, budget()).unwrap();
    let space = current_space::assemble(
        &foils,
        &electric,
        &[[0, 1], [2, 3]],
        1e5,
        current_space::Budget {
            max_bytes: 512 * 1024 * 1024,
            max_wall: Duration::from_secs(120),
        },
    )
    .unwrap();
    assert_eq!(space.cells.len(), 4);
    let cb = current_space::Budget {
        max_bytes: 512 * 1024 * 1024,
        max_wall: Duration::from_secs(120),
    };
    for case in 0..7 {
        let mut bad = foils.clone();
        let mut fmax = 1e5;
        match case {
            0 => fmax = 1e6,
            1 => bad[0].conductivity_s_per_m = 0.,
            2 => bad[0].potential_dofs.swap(0, 1),
            3 => {
                bad.pop();
            }
            4 => bad[0].source_id = "invented-source".into(),
            5 => bad[0].triangles[0] = [0, 0, 1],
            _ => bad[0].z_m[1] += 1e-6,
        }
        assert!(
            current_space::assemble(&bad, &electric, &[[0, 1], [2, 3]], fmax, cb).is_err(),
            "case{case}"
        );
    }
    assert!(current_space::assemble(
        &foils,
        &electric,
        &[[0, 1], [2, 3]],
        1e5,
        current_space::Budget {
            max_wall: Duration::ZERO,
            ..cb
        }
    )
    .err()
    .unwrap()
    .contains("DEADLINE"));

    let model = &space.model;
    let mut p6 = System::compile(model, 2, None).unwrap();
    let rhs = p6.port_rhs(&[1., -1.]);
    let x = p6
        .solve(0., rhs, &[0.; 12], &mut Counters::default())
        .unwrap();
    let r = 1. / (58e6 * 35e-6);
    close(x[0] - x[1] - x[2] + x[3], 2. * r, 2. * r, 1e-10);
    let currents = &x[4..];
    let density = space.recover_density(currents).unwrap();
    for j in &density {
        close(
            j[0].abs(),
            1. / (0.0005 * 35e-6),
            1. / (0.0005 * 35e-6),
            1e-10,
        );
        close(j[1], 0., 1. / (0.0005 * 35e-6), 1e-10);
    }
    close(
        space.cell_losses_w(currents).unwrap().iter().sum(),
        2. * r,
        2. * r,
        1e-10,
    );
    close(p6.joule_loss(&x), 2. * r, 2. * r, 1e-10);
    // Uniform x current of one square: independent frozen box difference-convolution.
    let uniform = [1. / 2f64.sqrt(), 0., 1. / 2f64.sqrt(), 0., 0., 0., 0., 0.];
    let l: f64 = model
        .inductance_h
        .iter()
        .map(|&(i, j, v)| uniform[i] * v * uniform[j])
        .sum();
    close(l, 1.4198058291191782e-10, 1.4198058291191782e-10, 0.002);
    use pcb_analyze::extraction::local::reduction::project_currents;
    let a = 1. / 2f64.sqrt();
    let basis = vec![
        vec![a, 0., a, 0., 0., 0., 0., 0.],
        vec![0., 0., 0., 0., a, 0., a, 0.],
    ];
    let reduced = project_currents(
        model,
        2,
        basis.clone(),
        512 * 1024 * 1024,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(reduced.model.current_dofs.len(), 2);
    assert_eq!(reduced.model.potential_nodes, model.potential_nodes);
    assert_eq!(reduced.model.capacitance_f, model.capacitance_f);
    let mut rp6 = System::compile(&reduced.model, 2, None).unwrap();
    let rr = rp6.port_rhs(&[1., -1.]);
    let rx = rp6
        .solve(0., rr, &[0.; 6], &mut Counters::default())
        .unwrap();
    let recovered = reduced.recover_currents(&rx[4..]).unwrap();
    for (a, b) in recovered.iter().zip(currents) {
        close(*a, *b, 1., 1e-9);
    }
    close(
        space.cell_losses_w(&recovered).unwrap().iter().sum(),
        p6.joule_loss(&x),
        2. * r,
        1e-9,
    );
    for coordinates in [vec![0., 0.], vec![1., -2.], vec![1e-12, 3e-12]] {
        let physical = reduced.recover_currents(&coordinates).unwrap();
        let restored = reduced.initial_currents(&physical).unwrap();
        for i in 0..2 {
            close(
                restored[i],
                coordinates[i],
                coordinates.iter().map(|v| v.abs()).fold(0., f64::max),
                1e-10,
            );
        }
        let full_state: Vec<_> = [vec![0.4, -0.3, 0.2, 0.1], physical].concat();
        let reduced_state: Vec<_> = [vec![0.4, -0.3, 0.2, 0.1], coordinates].concat();
        close(
            p6.energy(&full_state),
            rp6.energy(&reduced_state),
            p6.energy(&full_state).abs(),
            1e-10,
        );
        close(
            p6.joule_loss(&full_state),
            rp6.joule_loss(&reduced_state),
            p6.joule_loss(&full_state).abs(),
            1e-10,
        );
    }
    assert!(reduced
        .initial_currents(&[1., 0., -1., 0., 0., 0., 0., 0.])
        .unwrap_err()
        .contains("REDUCTION_INITIAL"));
    assert!(reduced
        .initial_currents(&[f64::NAN; 8])
        .unwrap_err()
        .contains("REDUCTION_INITIAL"));
    assert!(project_currents(
        model,
        2,
        vec![basis[0].clone(), basis[0].clone()],
        512 * 1024 * 1024,
        Duration::from_secs(60)
    )
    .err()
    .unwrap()
    .contains("REDUCTION_BASIS"));
    assert!(
        project_currents(model, 2, basis, 1, Duration::from_secs(60))
            .err()
            .unwrap()
            .contains("MEMORY_LIMIT")
    );
    // Cross-foil injection conserves global charge. It excites the physical
    // interfoil capacitor (large reactive Z), not the global structural gauge.
    let mut cross=model.clone();
    cross.port_incidence=vec![(0,0,1.),(2,0,-1.),(1,1,1.),(3,1,-1.)];
    cross.port_terminals=None;
    assert!(coupled::impedance(&cross,2,&[0.],None).unwrap_err().contains("COUPLED_DC_INCOMPATIBLE"));
    let axis=FREQUENCIES_HZ;
    for (candidate,ports) in [(&cross,2),(model,2),(&reduced.model,2),(&loop_model(model),1),(&loop_model(&reduced.model),1)] {
        let response=coupled::impedance(candidate,ports,&axis,None).unwrap();
        assert_eq!(response["parameters"].as_array().unwrap().len(),17);
    }

}

#[test]
fn real_source_roi_count_plans_refuse_without_dense_allocation_or_success_claim() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/analysis-local-extraction/copper-context/verification.json");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let b = current_space::Budget {
        max_bytes: 512 * 1024 * 1024,
        max_wall: Duration::from_secs(120),
    };
    let expected = [
        ("STM32G0_CAN_RS485", 903, 726),
        ("TPS63060DSCR", 1911, 1319),
        ("BandAmpV5", 2600, 1804),
    ];
    for (i, (name, foil, potentials)) in expected.iter().enumerate() {
        let roi = &saved["meshCounts"][i];
        assert_eq!(roi["name"], *name);
        assert_eq!(roi["foil"], *foil);
        assert_eq!(roi["potentialNodes"], *potentials);
        assert!(current_space::preflight_counts(*foil, *potentials, 0, b)
            .unwrap_err()
            .contains("CURRENT_SCOPE"));
        assert!(roi["axial"].as_u64().unwrap() > 0); // No missing real barrels silently treated as ideal shorts.
    }
    assert!(
        current_space::preflight_counts(4, 4, 0, current_space::Budget { max_bytes: 1, ..b })
            .unwrap_err()
            .contains("MEMORY_LIMIT")
    );
    assert!(
        current_space::preflight_counts(usize::MAX, usize::MAX, 0, b)
            .unwrap_err()
            .contains("CURRENT_SCOPE")
    );
}
