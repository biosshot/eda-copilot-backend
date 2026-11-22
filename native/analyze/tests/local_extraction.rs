//! Focused primitive gates. These do not admit a board extraction or close P7.
use pcb_analyze::extraction::{
    local::{
        self,
        mesh::{self, LayeredInput, Material},
        prism::{GaussRule, Prism},
        sheet::SheetCell,
        AllocationBudget,
    },
    EPS0,
};

fn close(actual: f64, expected: f64, relative: f64) {
    assert!(
        (actual - expected).abs() <= relative * expected.abs(),
        "actual={actual:e} expected={expected:e}"
    );
}

#[test]
fn sheet_elimination_matches_independent_p1_dc_and_recovers_joule_power() {
    // Independent gradients for (0,0),(2mm,0),(0,1mm), area=1mm².
    let cell = SheetCell::new(
        "authored-trace-17".into(),
        [[0., 0.], [0.002, 0.], [0., 0.001]],
        35e-6,
        58e6,
    )
    .unwrap();
    let g = [[-500., -1000.], [500., 0.], [0., 1000.]];
    for i in 0..3 {
        for j in 0..3 {
            let eliminated = (0..2)
                .map(|k| cell.incidence[i][k] * cell.incidence[j][k] / cell.resistance_ohm)
                .sum::<f64>();
            let expected = 58e6 * 35e-6 * 1e-6 * (g[i][0] * g[j][0] + g[i][1] * g[j][1]);
            if expected == 0. {
                assert_eq!(eliminated, 0.);
            } else {
                close(eliminated, expected, 2e-15);
            }
        }
    }
    let volts = [0.1, 0.2, -0.3];
    let i = std::array::from_fn(|axis| {
        (0..3)
            .map(|n| cell.incidence[n][axis] * volts[n])
            .sum::<f64>()
            / cell.resistance_ohm
    });
    let density = cell.recover_current_density(i).unwrap();
    let direct_j = [-58e6 * (500. * (0.2 - 0.1)), -58e6 * (1000. * (-0.3 - 0.1))];
    for k in 0..2 {
        close(density[k], direct_j[k], 2e-15);
    }
    let modal_power = cell.resistance_ohm * (i[0] * i[0] + i[1] * i[1]);
    let physical_power = (density[0] * density[0] + density[1] * density[1]) / 58e6 * 1e-6 * 35e-6;
    close(modal_power, physical_power, 2e-15);
    assert_eq!(cell.source_id, "authored-trace-17");
}

fn layered_input() -> LayeredInput {
    LayeredInput {
        xy_m: vec![[0., 0.], [1e-3, 0.], [1e-3, 1e-3], [0., 1e-3]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        boundary_edges: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
        z_m: vec![0., 0.2e-3, 0.5e-3],
        materials: vec![
            vec![
                Material::Dielectric {
                    label: "lower".into(),
                    epsilon_r: 2.
                };
                2
            ],
            vec![
                Material::Dielectric {
                    label: "upper".into(),
                    epsilon_r: 4.
                };
                2
            ],
        ],
    }
}
fn budget() -> AllocationBudget {
    AllocationBudget {
        max_bytes: 16 * 1024 * 1024,
        max_nodes: 1000,
        max_tetrahedra: 1000,
    }
}

#[test]
fn planar_embedding_rejects_overlapping_components_and_duplicate_coordinate_ownership() {
    let mut input = layered_input();
    input.xy_m = vec![
        [0., 0.],
        [1e-3, 0.],
        [0., 1e-3],
        [0.2e-3, 0.2e-3],
        [1.2e-3, 0.2e-3],
        [0.2e-3, 1.2e-3],
    ];
    input.triangles = vec![[0, 1, 2], [3, 4, 5]];
    input.boundary_edges = vec![[0, 1], [1, 2], [2, 0], [3, 4], [4, 5], [5, 3]];
    assert!(mesh::extrude(&input, budget())
        .unwrap_err()
        .contains("PLANAR_OVERLAP"));
    let mut input = layered_input();
    input.xy_m.push([0., -0.]);
    assert!(mesh::extrude(&input, budget())
        .unwrap_err()
        .contains("PLANAR_OWNERSHIP"));
}

#[test]
fn layered_extrusion_has_no_internal_cracks_and_exact_material_volumes() {
    let mesh = mesh::extrude(&layered_input(), budget()).unwrap();
    assert_eq!(mesh.nodes_m.len(), 12);
    assert_eq!(mesh.tetrahedra.len(), 12);
    assert_eq!(mesh.copper_interfaces.len(), 0);
    assert_eq!(mesh.external_faces.len(), 20);
    close(mesh.dielectric_volume_m3, 0.5e-9, 2e-15);
    let lower: f64 = mesh
        .tetrahedra
        .iter()
        .filter(|t| t.slab == 0)
        .map(|t| t.volume_m3)
        .sum();
    close(lower, 0.2e-9, 2e-15);
    for t in &mesh.tetrahedra {
        for axis in 0..3 {
            assert!(t.gradients_per_m.iter().map(|g| g[axis]).sum::<f64>().abs() < 1e-10);
        }
    }
}

#[test]
fn layered_plate_energy_matches_series_dielectric_reference() {
    // Dirichlet plates + insulated side walls. This is an explicit parallel
    // plate coupon, not infinity/fringing/finite-board capacitance evidence.
    let input = layered_input();
    let mesh = mesh::extrude(&input, budget()).unwrap();
    let c_exact = EPS0 * 1e-6 / (0.2e-3 / 2. + 0.3e-3 / 4.);
    let interface_voltage = (0.3e-3 / 4.) / (0.2e-3 / 2. + 0.3e-3 / 4.);
    let mut energy_twice = 0.;
    let mut reaction_top = 0.;
    let mut reaction_bottom = 0.;
    for t in &mesh.tetrahedra {
        let k = mesh::stiffness(t).unwrap();
        let v = t.nodes.map(|node| match node / 4 {
            0 => 1.,
            1 => interface_voltage,
            2 => 0.,
            _ => unreachable!(),
        });
        for i in 0..4 {
            for j in 0..4 {
                energy_twice += v[i] * k[i][j] * v[j];
                if t.nodes[i] / 4 == 0 {
                    reaction_bottom += k[i][j] * v[j];
                }
                if t.nodes[i] / 4 == 2 {
                    reaction_top += k[i][j] * v[j];
                }
            }
        }
    }
    close(energy_twice, c_exact, 3e-15);
    close(reaction_bottom, c_exact, 3e-15);
    close(reaction_top, -c_exact, 3e-15);
}

#[test]
fn copper_interfaces_preserve_exact_source_ids_and_cut_material_volume() {
    let mut input = layered_input();
    input.materials[0][0] = Material::Copper {
        source_ids: vec!["via-wall-3".into(), "pad-29".into()],
    };
    let m = mesh::extrude(&input, budget()).unwrap();
    assert_eq!(m.tetrahedra.len(), 9);
    assert!(!m.copper_interfaces.is_empty());
    close(m.copper_volume_m3, 0.1e-9, 2e-15);
    close(m.dielectric_volume_m3, 0.4e-9, 2e-15);
    for face in &m.copper_interfaces {
        assert_eq!(face.copper_source_ids, vec!["pad-29", "via-wall-3"]);
    }
}

#[test]
fn explicit_budgets_and_invalid_geometry_refuse_before_allocating_mesh_or_kernel() {
    assert_eq!(local::packed_kernel_bytes(3682, 384).unwrap(), 54_834_584);
    assert!(local::packed_kernel_bytes(usize::MAX, 0)
        .unwrap_err()
        .contains("MEMORY_LIMIT"));
    let mut b = budget();
    b.max_bytes = 1;
    assert!(mesh::extrude(&layered_input(), b)
        .unwrap_err()
        .contains("MEMORY_LIMIT"));
    let mut bad = layered_input();
    bad.boundary_edges.pop();
    assert!(mesh::extrude(&bad, budget())
        .unwrap_err()
        .contains("MESH_CRACK"));
    let mut bad = layered_input();
    bad.triangles.push([0, 1, 2]);
    for row in &mut bad.materials {
        row.push(row[0].clone());
    }
    assert!(mesh::extrude(&bad, budget())
        .unwrap_err()
        .contains("duplicate"));
    assert!(SheetCell::new("s".into(), [[0., 0.], [1., 0.], [0., 1.]], 0., 58e6).is_err());
    assert!(Prism::new("s".into(), [[0., 0.], [0., 1.], [1., 0.]], [0., 1.]).is_err());
    assert!(GaussRule::new(128).is_err());
}

fn square_pair(y: f64) -> [Prism; 2] {
    [
        Prism::new(
            "half-a".into(),
            [[0., y], [0.5e-3, y], [0.5e-3, y + 0.5e-3]],
            [0., 35e-6],
        )
        .unwrap(),
        Prism::new(
            "half-b".into(),
            [[0., y], [0.5e-3, y + 0.5e-3], [0., y + 0.5e-3]],
            [0., 35e-6],
        )
        .unwrap(),
    ]
}

#[test]
fn finite_prism_self_touching_and_explicit_return_match_independent_frozen_reference() {
    let a = square_pair(0.);
    let b = square_pair(1e-3);
    let rule = GaussRule::new(20).unwrap();
    let scale = 1e-7 / (0.5e-3 * 35e-6_f64).powi(2);
    let mut self_l = 0.;
    let mut mutual_l = 0.;
    for x in &a {
        for y in &a {
            self_l += local::prism::volume_integral(x, y, &rule).unwrap() * scale;
        }
    }
    for x in &a {
        for y in &b {
            mutual_l += local::prism::volume_integral(x, y, &rule).unwrap() * scale;
        }
    }
    // Frozen independent Duffy box convolution (orders 128/256/512), not values
    // produced by this triangular fan algorithm. 0.2% primitive gate.
    close(self_l, 1.4198058291191782e-10, 0.002);
    // Independent direct smooth 6D product quadrature; generous 0.2% gate.
    close(mutual_l, 2.5533210945850166e-11, 0.002);
    let xy = local::prism::volume_integral(&a[0], &b[1], &rule).unwrap();
    let yx = local::prism::volume_integral(&b[1], &a[0], &rule).unwrap();
    close(xy, yx, 0.002);
    assert!(self_l > mutual_l && mutual_l > 0.);
}

#[test]
fn sparse_c_apply_solves_layered_plate_without_materializing_full_c_and_preserves_gauge() {
    use local::capacitance::{Boundary, CapacitanceOperator, TraceRow};
    let mesh = mesh::extrude(&layered_input(), budget()).unwrap();
    let boundary = Boundary {
        potential_count: 2,
        far_zero_nodes: vec![],
        insulated_external: true,
        metal_trace: (0..4)
            .map(|i| TraceRow {
                node: i,
                weights: vec![(0, 1.)],
            })
            .chain((8..12).map(|i| TraceRow {
                node: i,
                weights: vec![(1, 1.)],
            }))
            .collect(),
    };
    let operator = CapacitanceOperator::new(&mesh, boundary, budget()).unwrap();
    let c = EPS0 * 1e-6 / (0.2e-3 / 2. + 0.3e-3 / 4.);
    let a = operator.apply(&[1., 0.]).unwrap();
    close(a.charges_c[0], c, 1e-12);
    close(a.charges_c[1], -c, 1e-12);
    close(a.energy_j, c / 2., 1e-12);
    let gauge = operator.apply(&[1e6 + 1., 1e6]).unwrap();
    for j in 0..2 {
        close(gauge.charges_c[j], a.charges_c[j], 1e-14);
    }
    let null = operator.apply(&[7., 7.]).unwrap();
    assert_eq!(null.charges_c, vec![0., 0.]);
    assert_eq!(null.energy_j, 0.);
    assert!(null.nodal_potentials_v.iter().all(|v| *v == 7.));
    assert_eq!(operator.factor_value_count > 0, true);
}

#[test]
fn explicit_far_boundary_retains_full_c_common_mode_and_balances_escaped_charge() {
    use local::capacitance::{Boundary, CapacitanceOperator, TraceRow};
    use std::collections::{BTreeMap, BTreeSet};
    let xy = [-2e-3, -0.5e-3, 0.5e-3, 2e-3];
    let z = vec![-2e-3, 0., 35e-6, 1e-3, 1.035e-3, 3e-3];
    let mut input = LayeredInput {
        xy_m: vec![],
        triangles: vec![],
        boundary_edges: vec![],
        z_m: z,
        materials: vec![],
    };
    for &y in &xy {
        for &x in &xy {
            input.xy_m.push([x, y]);
        }
    }
    let mut square_indices = vec![];
    for y in 0..3 {
        for x in 0..3 {
            let a = y * 4 + x;
            input
                .triangles
                .extend([[a, a + 1, a + 5], [a, a + 5, a + 4]]);
            square_indices.extend([(x, y), (x, y)]);
        }
    }
    for i in 0..3 {
        input.boundary_edges.extend([
            [i, i + 1],
            [12 + i, 13 + i],
            [4 * i, 4 * (i + 1)],
            [4 * i + 3, 4 * (i + 1) + 3],
        ]);
    }
    for slab in 0..5 {
        input.materials.push(
            square_indices
                .iter()
                .map(|&(x, y)| {
                    if x == 1 && y == 1 && (slab == 1 || slab == 3) {
                        Material::Copper {
                            source_ids: vec![format!("plate-{slab}")],
                        }
                    } else {
                        Material::Dielectric {
                            label: "vacuum".into(),
                            epsilon_r: 1.,
                        }
                    }
                })
                .collect(),
        );
    }
    let mesh = mesh::extrude(&input, budget()).unwrap();
    let mut trace = BTreeMap::new();
    for f in &mesh.copper_interfaces {
        let col = if f.copper_source_ids == vec!["plate-1"] {
            0
        } else {
            assert_eq!(f.copper_source_ids, vec!["plate-3"]);
            1
        };
        for node in f.nodes {
            assert!(trace.insert(node, col).is_none_or(|old| old == col));
        }
    }
    let far: BTreeSet<_> = mesh.external_faces.iter().flatten().copied().collect();
    let boundary = Boundary {
        potential_count: 2,
        metal_trace: trace
            .into_iter()
            .map(|(node, col)| TraceRow {
                node,
                weights: vec![(col, 1.)],
            })
            .collect(),
        far_zero_nodes: far.into_iter().collect(),
        insulated_external: false,
    };
    let op = CapacitanceOperator::new(&mesh, boundary, budget()).unwrap();
    let a = op.apply(&[1., 0.]).unwrap();
    let b = op.apply(&[0., 1.]).unwrap();
    let common = op.apply(&[1., 1.]).unwrap();
    close(a.charges_c[1], b.charges_c[0], 1e-12);
    assert!(a.charges_c[1] < 0. && a.charges_c[0] > 0. && b.charges_c[1] > 0.);
    assert!(a.charges_c[0] * b.charges_c[1] - a.charges_c[1] * b.charges_c[0] > 0.);
    assert!(common.energy_j > 0. && common.charges_c.iter().sum::<f64>() > 0.);
    close(common.charges_c.iter().sum(), -common.far_charge_c, 1e-12);
    assert!(common.normalized_charge_balance < 1e-12);
}

#[test]
fn bounded_far_moments_cover_vertices_and_bound_integral_and_physical_l() {
    use local::far::Moments;
    let a = square_pair(0.)[0].clone();
    let b = square_pair(0.02)[1].clone();
    let ma = Moments::new(&a).unwrap();
    let mb = Moments::new(&b).unwrap();
    for (p, m) in [(&a, &ma), (&b, &mb)] {
        for xy in p.triangle_m {
            for z in p.z_m {
                assert!(
                    (xy[0] - m.center_m[0])
                        .hypot(xy[1] - m.center_m[1])
                        .hypot(z - m.center_m[2])
                        <= m.radius_m
                );
            }
        }
    }
    let far = ma.pair(&mb).unwrap();
    assert!(far.radius_ratio < 0.05);
    let reference =
        local::prism::smooth_volume_integral(&a, &b, &GaussRule::new(4).unwrap()).unwrap();
    let roundoff = reference.abs() * 1e-13;
    assert!((far.integral_m5 - reference).abs() <= far.absolute_remainder_m5 + roundoff);
    let normalization_a = SheetCell::new("a".into(), a.triangle_m, 35e-6, 58e6)
        .unwrap()
        .current_density_per_amp_m2;
    let normalization_b = SheetCell::new("b".into(), b.triangle_m, 35e-6, 58e6)
        .unwrap()
        .current_density_per_amp_m2;
    let scale = 1e-7 * normalization_a * normalization_b;
    assert!(
        ((far.integral_m5 - reference) * scale).abs()
            <= (far.absolute_remainder_m5 + roundoff) * scale
    );
    assert!(ma.pair(&ma).unwrap_err().contains("FAR_SEPARATION"));
}

#[test]
fn exact_source_prism_handles_slender_self_and_micron_gap_return_against_external_reference() {
    use local::polyhedron::{volume_integral, PointPotential};
    let halves = |shift: f64| {
        [
            Prism::new(
                "long-half-a".into(),
                [[0., shift], [0.01, shift], [0.01, shift + 0.0005]],
                [0., 35e-6],
            )
            .unwrap(),
            Prism::new(
                "long-half-b".into(),
                [[0., shift], [0.01, shift + 0.0005], [0., shift + 0.0005]],
                [0., 35e-6],
            )
            .unwrap(),
        ]
    };
    let signal = halves(0.);
    let ret = halves(0.000501);
    let rule = GaussRule::new(8).unwrap();
    let scale = 1e-7 / (0.0005 * 35e-6_f64).powi(2);
    let mut self_l = 0.;
    let mut mutual_l = 0.;
    for a in &signal {
        for b in &signal {
            self_l += volume_integral(a, &PointPotential::new(b).unwrap(), &rule).unwrap() * scale;
        }
    }
    for a in &signal {
        for b in &ret {
            mutual_l +=
                volume_integral(a, &PointPotential::new(b).unwrap(), &rule).unwrap() * scale;
        }
    }
    close(self_l, 8.272304610981235e-9, 0.002);
    let external = 3.57844e-5 / (2. * std::f64::consts::PI * 1000.);
    close(mutual_l, external, 0.002);
    // Finite explicit-return loop; no subtraction of a fictitious ideal ground.
    assert!(2. * self_l - 2. * mutual_l > 0.);
}
