use pcb_analyze::extraction::{
    local::{
        board_prepare::verification_axis,
        capacitance::{Boundary, CapacitanceOperator, TraceRow},
        mesh::{self, LayeredInput, LayeredMesh, Material},
        volume_current::{self, Space},
        volume_field::ElectricApply,
        volume_magnetic::MagneticApply,
        volume_solve::{Solution, Solver},
        AllocationBudget, Result,
    },
    EPS0,
};
use std::{f64::consts::TAU, time::Duration};

const WIDTH: f64 = 1e-3;
const GAP: f64 = 0.5e-3;
const METAL: f64 = 1e-3;
const GUARD: f64 = 4e-3;
const EPSILON_R: f64 = 4.;
const SIGMA: f64 = 0.01;

fn budget() -> AllocationBudget {
    AllocationBudget {
        max_bytes: 32 * 1024 * 1024,
        max_nodes: 1000,
        max_tetrahedra: 1000,
    }
}

fn coupon(broken: bool) -> (LayeredMesh, [Vec<usize>; 2]) {
    let dielectric = Material::Dielectric {
        label: "manufactured uniform dielectric".into(),
        epsilon_r: EPSILON_R,
    };
    let conductor = Material::Copper {
        source_ids: vec!["manufactured resistive conductor".into()],
    };
    let mesh = mesh::extrude(
        &LayeredInput {
            xy_m: vec![[0., 0.], [WIDTH, 0.], [WIDTH, WIDTH], [0., WIDTH]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            boundary_edges: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
            z_m: vec![
                -GUARD,
                0.,
                METAL,
                METAL + GAP,
                2. * METAL + GAP,
                2. * METAL + GAP + GUARD,
            ],
            materials: [
                dielectric.clone(),
                conductor.clone(),
                if broken {
                    dielectric.clone()
                } else {
                    conductor.clone()
                },
                conductor,
                dielectric,
            ]
            .into_iter()
            .map(|material| vec![material; 2])
            .collect(),
        },
        budget(),
    )
    .unwrap();
    (mesh, [(4..8).collect(), (16..20).collect()])
}

fn assemble(mesh: &LayeredMesh, electrodes: &[Vec<usize>; 2]) -> Space {
    volume_current::assemble(
        mesh,
        &vec![SIGMA; mesh.copper_tetrahedra.len()],
        electrodes,
        budget(),
    )
    .unwrap()
}

struct PlateField<'a>(CapacitanceOperator<'a>);

impl ElectricApply for PlateField<'_> {
    fn apply(&self, volts: &[f64]) -> Result<Vec<f64>> {
        Ok(self.0.apply(volts)?.charges_c)
    }

    fn energy_gram(&self, basis: &[Vec<f64>]) -> Result<Vec<(usize, usize, f64)>> {
        let mut gram = Vec::new();
        for (column, volts) in basis.iter().enumerate() {
            let charge = self.apply(volts)?;
            for (row, test) in basis.iter().enumerate() {
                gram.push((
                    row,
                    column,
                    test.iter().zip(&charge).map(|(v, q)| v * q).sum(),
                ));
            }
        }
        Ok(gram)
    }

    fn planned_bytes(&self) -> usize {
        self.0.planned_peak_bytes
    }
}

struct Electroquasistatic;

impl MagneticApply for Electroquasistatic {
    fn apply(&self, _space: &Space, current: &[f64]) -> Result<Vec<f64>> {
        Ok(vec![0.; current.len()])
    }

    fn planned_bytes(&self) -> usize {
        0
    }
}

fn voltage(space: &Space, state: &Solution, imaginary: bool) -> Vec<f64> {
    let (shape, correction, constants) = if imaginary {
        (
            &state.imaginary_shape_v,
            &state.imaginary_shape_correction_v,
            &state.imaginary_constants_v,
        )
    } else {
        (
            &state.real_shape_v,
            &state.real_shape_correction_v,
            &state.real_constants_v,
        )
    };
    let mut volts = vec![0.; shape.len()];
    for (component, nodes) in space.components.iter().enumerate() {
        for &node in nodes {
            volts[node] = shape[node] + correction[node] + constants[component];
        }
    }
    volts
}

fn assert_close(actual: f64, expected: f64, scale: f64) {
    assert!(
        (actual - expected).abs() <= 1e-7 * scale,
        "{actual:e} vs {expected:e}, scale {scale:e}"
    );
}

#[test]
fn severed_return_carries_displacement_but_no_conduction() {
    // Manufactured one-dimensional EQS slab with insulated sidewalls. Grounded
    // outer planes provide explicit equal shunt capacitances and fix the absolute
    // potential. This validates coupling in the finite-volume solver, without
    // relying on open-board fringing or a magnetic model of displacement current.
    let (mesh, electrodes) = coupon(true);
    let space = assemble(&mesh, &electrodes);
    assert_eq!(space.components.len(), 2);
    assert!(mesh.copper_tetrahedra.iter().all(|cell| {
        cell.nodes.iter().all(|&node| node < 12) || cell.nodes.iter().all(|&node| node >= 12)
    }));
    let field = PlateField(
        CapacitanceOperator::new(
            &mesh,
            Boundary {
                potential_count: space.potential_count(),
                metal_trace: space
                    .node_potential
                    .iter()
                    .map(|(&node, &dof)| TraceRow {
                        node,
                        weights: vec![(dof, 1.)],
                    })
                    .collect(),
                far_zero_nodes: (0..4).chain(20..24).collect(),
                insulated_external: true,
            },
            budget(),
        )
        .unwrap(),
    );
    let solver = Solver::new(&space, &field, &Electroquasistatic, budget()).unwrap();
    let [positive, negative] = electrodes
        .each_ref()
        .map(|nodes| space.node_potential[&nodes[0]]);
    let zero = vec![0.; space.potential_count()];
    let mut drive = zero.clone();
    drive[positive] = 1.;
    drive[negative] = -1.;

    let area = WIDTH * WIDTH;
    let copper_resistance = 2. * METAL / (SIGMA * area);
    let gap_c = EPS0 * EPSILON_R * area / GAP;
    let guard_c = EPS0 * EPSILON_R * area / GUARD;
    let axis = verification_axis(&[0.5e6, 1.2e6, 2.0e6]).unwrap();
    let deadline = Duration::from_secs(5);
    for invalid in [7, 17] {
        let error = solver
            .solve_with_restart_limit(axis[0], &drive, &zero, deadline, invalid)
            .err()
            .expect("out-of-range restart limit must be refused");
        assert!(error.contains("restart limit must be in 8..16"), "{error}");
    }
    for &frequency in axis {
        let omega = TAU * frequency;
        let state = solver.solve(frequency, &drive, &zero, deadline).unwrap();
        let [vr, vi] = solver.port_voltage(&state, positive, negative);
        if frequency == axis[0] {
            for limit in [8, 16] {
                let explicit = solver
                    .solve_with_restart_limit(frequency, &drive, &zero, deadline, limit)
                    .unwrap();
                assert_eq!(solver.port_voltage(&explicit, positive, negative), [vr, vi]);
                assert_eq!(explicit.real_currents_a, state.real_currents_a);
                assert_eq!(explicit.imaginary_currents_a, state.imaginary_currents_a);
                assert_eq!(explicit.audit.iterations, state.audit.iterations);
            }
        }
        // Yport = 1 / (Rc + 1/(jω Cgap)) + jω Cguard/2.
        let gap_reactance = 1. / (omega * gap_c);
        let denominator = copper_resistance.powi(2) + gap_reactance.powi(2);
        let yr = copper_resistance / denominator;
        let yi = gap_reactance / denominator + omega * guard_c / 2.;
        let impedance_scale = 1. / yr.hypot(yi);
        let expected_vr = yr * impedance_scale.powi(2);
        let expected_vi = -yi * impedance_scale.powi(2);
        assert_close(vr, expected_vr, impedance_scale);
        assert_close(vi, expected_vi, impedance_scale);

        let real_volts = voltage(&space, &state, false);
        let imaginary_volts = voltage(&space, &state, true);
        let real_charge = field.apply(&real_volts).unwrap();
        let imaginary_charge = field.apply(&imaginary_volts).unwrap();
        for nodes in &space.components {
            let total = |v: &[f64]| nodes.iter().map(|&node| v[node]).sum::<f64>();
            assert_close(-omega * total(&imaginary_charge), total(&drive), 1.);
            assert_close(omega * total(&real_charge), 0., 1.);
        }
        let lower_gap = space.node_potential[&8];
        let upper_gap = space.node_potential[&12];
        let gap_real = -omega * gap_c * (imaginary_volts[lower_gap] - imaginary_volts[upper_gap]);
        let gap_imaginary = omega * gap_c * (real_volts[lower_gap] - real_volts[upper_gap]);
        assert_close(gap_real, 1. + omega * guard_c * expected_vi / 2., 1.);
        assert_close(gap_imaginary, -omega * guard_c * expected_vr / 2., 1.);
    }

    for broken in [true, false] {
        let (mesh, electrodes) = coupon(broken);
        let space = assemble(&mesh, &electrodes);
        let [positive, negative] = electrodes
            .each_ref()
            .map(|nodes| space.node_potential[&nodes[0]]);
        assert_eq!(space.components.len(), if broken { 2 } else { 1 });
        let mut prescribed = vec![0.; space.potential_count()];
        prescribed[positive] = 1.;
        let potential = space
            .dc_factor(&[positive, negative], budget())
            .unwrap()
            .solve(&space, &prescribed, &vec![0.; prescribed.len()])
            .unwrap();
        let dc_current = space.dc_apply(&potential).unwrap();
        let continuous_conductance = SIGMA * area / (2. * METAL + GAP);
        let expected = if broken { 0. } else { continuous_conductance };
        assert_close(dc_current[positive], expected, continuous_conductance);
        assert_close(dc_current[negative], -expected, continuous_conductance);
    }
}
