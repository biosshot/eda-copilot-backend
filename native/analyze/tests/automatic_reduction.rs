use pcb_analyze::{
    electrical::{
        coupled::{Model, System},
        Counters,
    },
    extraction::local::reduction::{automatic_currents, AutomaticOptions},
};
use std::time::Duration;

fn options(order: usize) -> AutomaticOptions {
    AutomaticOptions {
        training_hz: vec![0., 1., 100.],
        relative_tolerance: 1e-6,
        max_current_order: order,
        max_bytes: 512 * 1024 * 1024,
        max_wall: Duration::from_secs(120),
    }
}
fn parallel() -> Model {
    // Three distinct relaxation times, each represented by two equal parallel
    // copper-current coordinates. Independent analytic reference below does not
    // call extraction, projection or another descriptor solver.
    Model {
        format: "coupled-rlc".into(),
        reference_topology: "infinity".into(),
        potential_nodes: vec!["positive".into(), "return".into()],
        current_dofs: (0..6).map(|i| format!("branch{i}")).collect(),
        incidence: (0..6).flat_map(|i| [(0, i, 1.), (1, i, -1.)]).collect(),
        port_incidence: vec![(0, 0, 1.), (1, 0, -1.)],
        resistance_ohm: (0..6).map(|i| (i, i, 1. + (i / 2) as f64)).collect(),
        inductance_h: (0..6)
            .map(|i| (i, i, 0.01 * (1. + (i / 2) as f64).powi(2)))
            .collect(),
        capacitance_f: vec![(0, 0, 1e-6), (1, 1, 1e-6), (0, 1, -1e-6), (1, 0, -1e-6)],
        conserved_potential_modes: vec![vec![1., 1.]],
        structural_gauge_modes: vec![vec![1., 1.]],
        port_terminals: None,
    }
}
fn close(a: f64, b: f64, scale: f64) {
    assert!(
        (a - b).abs() < 1e-8 * scale,
        "{a:e} vs {b:e} (scale{scale:e})"
    );
}
#[test]
fn automatic_order_and_holdout_match_independent_parallel_rlc_and_local_losses() {
    let input = parallel();
    let result = automatic_currents(&input, 1, options(6)).unwrap();
    assert_eq!(result.audit.current_orders, vec![1, 2, 3]);
    assert!(result.audit.state_errors[0] > 1e-6);
    assert!(result.audit.loss_errors[0] > 1e-6);
    assert_eq!(result.projection.basis.len(), 3);
    assert_eq!(result.projection.model.capacitance_f, input.capacitance_f);
    assert_eq!(
        result.projection.model.structural_gauge_modes,
        input.structural_gauge_modes
    );
    let mut system = System::compile(&result.projection.model, 1, None).unwrap();
    let nv = 2;
    let nr = 3;
    let n = nv + nr;
    // Disjoint holdout axis; all internal complex currents, both potentials,
    // every local physical R loss and stored energy are independently checked.
    for f in [0.25, 3.1622776601683795, 17., 1000.] {
        let w = 2. * std::f64::consts::PI * f;
        let mut yr = 0.;
        let mut yi = w * 1e-6;
        for k in 0..6 {
            let r = 1. + (k / 2) as f64;
            let l = 0.01 * r * r;
            let d = r * r + (w * l).powi(2);
            yr += r / d;
            yi -= w * l / d;
        }
        let d = yr * yr + yi * yi;
        let zr = yr / d;
        let zi = -yi / d;
        let mut rhs = system.port_rhs(&[1.]);
        rhs.resize(2 * n, 0.);
        let x = system
            .solve(w, rhs, &vec![0.; n], &mut Counters::default())
            .unwrap();
        close(x[0] - x[1], zr, zr.hypot(zi));
        close(x[n] - x[n + 1], zi, zr.hypot(zi));
        close(x[0] + x[1], 0., zr.hypot(zi));
        close(x[n] + x[n + 1], 0., zr.hypot(zi));
        let ir = result.projection.recover_currents(&x[nv..n]).unwrap();
        let ii = result.projection.recover_currents(&x[n + nv..]).unwrap();
        let mut loss = 0.;
        let mut magnetic = 0.;
        for k in 0..6 {
            let r = 1. + (k / 2) as f64;
            let l = 0.01 * r * r;
            let d = r * r + (w * l).powi(2);
            let ar = (zr * r + zi * w * l) / d;
            let ai = (zi * r - zr * w * l) / d;
            close(ir[k], ar, ar.hypot(ai));
            close(ii[k], ai, ar.hypot(ai));
            let power = r * (ar * ar + ai * ai);
            close(r * (ir[k] * ir[k] + ii[k] * ii[k]), power, power);
            loss += power;
            magnetic += 0.5 * l * (ar * ar + ai * ai);
        }
        close(loss, zr, zr);
        let energy = system.energy(&x[..n]) + system.energy(&x[n..]);
        close(energy, magnetic + 0.5e-6 * (zr * zr + zi * zi), energy);
    }
    let physical = result.projection.recover_currents(&[1., -2., 3.]).unwrap();
    result.projection.initial_currents(&physical).unwrap();
    assert!(result
        .projection
        .initial_currents(&[1., 0., 0., 0., 0., 0.])
        .is_err());
}
#[test]
fn limits_and_unsupported_matrices_refuse_with_specific_diagnostics() {
    let model = parallel();
    assert!(automatic_currents(&model, 1, options(1))
        .err()
        .unwrap()
        .contains("REDUCTION_ORDER"));
    let mut bad = options(6);
    bad.max_bytes = 1;
    assert!(automatic_currents(&model, 1, bad)
        .err()
        .unwrap()
        .contains("MEMORY_LIMIT"));
    let mut bad = options(6);
    bad.max_wall = Duration::ZERO;
    assert!(automatic_currents(&model, 1, bad)
        .err()
        .unwrap()
        .contains("DEADLINE"));
    for axis in [
        vec![],
        vec![f64::NAN],
        vec![-1.],
        vec![1., 1.],
        vec![2., 1.],
    ] {
        let mut bad = options(6);
        bad.training_hz = axis;
        assert!(automatic_currents(&model, 1, bad)
            .err()
            .unwrap()
            .contains("REDUCTION_INPUT"));
    }
    let mut coupled = model.clone();
    coupled.resistance_ohm.extend([(0, 1, 0.01), (1, 0, 0.01)]);
    assert!(automatic_currents(&coupled, 1, options(6))
        .err()
        .unwrap()
        .contains("REDUCTION_RESISTANCE"));
    let mut active = model.clone();
    active.inductance_h[0].2 = -1.;
    assert!(automatic_currents(&active, 1, options(6))
        .err()
        .unwrap()
        .contains("COUPLED_PSD"));
    let mut false_gauge = model.clone();
    false_gauge.structural_gauge_modes[0] = vec![1., 0.];
    assert!(automatic_currents(&false_gauge, 1, options(6)).is_err());
}

#[path = "../../../tests/fixtures/analysis-local-extraction/current-space/fixture.rs"]
mod fixture;
#[test]
fn accepted_foil_space_automatically_reduces_on_disjoint_saved_frequency_holdouts() {
    use pcb_analyze::extraction::local::{current_space, layered_capacitance};
    let (input, trace, foils) = fixture::quotient();
    let electric = layered_capacitance::extract_traced(&input, &trace, fixture::budget()).unwrap();
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
    let mut config = options(space.model.current_dofs.len());
    config.training_hz = fixture::FREQUENCIES_HZ.iter().step_by(2).copied().collect();
    let reduced = automatic_currents(&space.model, 2, config).unwrap();
    assert!(reduced.projection.basis.len() < space.model.current_dofs.len());
    let nv = space.model.potential_nodes.len();
    let ni = space.model.current_dofs.len();
    let nr = reduced.projection.basis.len();
    let n = nv + ni;
    let m = nv + nr;
    let mut full = System::compile(&space.model, 2, None).unwrap();
    let mut small = System::compile(&reduced.projection.model, 2, None).unwrap();
    for &f in fixture::FREQUENCIES_HZ.iter().skip(1).step_by(2) {
        for p in 0..2 {
            let mut drive = [0., 0.];
            drive[p] = 1.;
            let mut rhs = full.port_rhs(&drive);
            rhs.resize(2 * n, 0.);
            let a = full
                .solve(
                    2. * std::f64::consts::PI * f,
                    rhs,
                    &vec![0.; n],
                    &mut Counters::default(),
                )
                .unwrap();
            let mut rhs = small.port_rhs(&drive);
            rhs.resize(2 * m, 0.);
            let b = small
                .solve(
                    2. * std::f64::consts::PI * f,
                    rhs,
                    &vec![0.; m],
                    &mut Counters::default(),
                )
                .unwrap();
            let mut back = b[..nv].to_vec();
            back.extend(reduced.projection.recover_currents(&b[nv..m]).unwrap());
            back.extend_from_slice(&b[m..m + nv]);
            back.extend(reduced.projection.recover_currents(&b[m + nv..]).unwrap());
            for k in 0..n {
                let scale = a[k].hypot(a[n + k]).max(1e-12);
                assert!((a[k] - back[k]).hypot(a[n + k] - back[n + k]) < 1e-6 * scale);
            }
            let af = space.cell_losses_w(&a[nv..n]).unwrap();
            let bf = space.cell_losses_w(&back[nv..n]).unwrap();
            for (a, b) in af.iter().zip(bf) {
                close(*a, b, a.abs().max(1e-30));
            }
        }
    }
}
