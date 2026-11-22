#[path = "reference/local/coax_diffusion_reference.rs"]
mod coax_diffusion_reference;

use coax_diffusion_reference::{reference, Coax, MU0};
use faer::c64 as C;
use pcb_analyze::extraction::local::{
    mesh::{extrude, LayeredInput, Material},
    volume_current, volume_field, volume_integral_magnetic, volume_solve, AllocationBudget,
};
use std::{
    f64::consts::PI,
    time::{Duration, Instant},
};

const COAX: Coax = Coax {
    a: 0.3e-3,
    b: 0.6e-3,
    c: 0.9e-3,
    sigma: 58e6,
};

fn close(actual: f64, expected: f64, relative: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs() * relative,
        "{actual:e} vs {expected:e}"
    );
}

#[test]
fn independent_coax_reference_dc_overlap_and_power() {
    let dc = reference(COAX, 0., 256);
    let low = reference(COAX, 100., 256);
    close(low.impedance.re, dc.impedance.re, 1e-6);
    for frequency in [25e3, 100e3, 400e3, 25e6] {
        let coarse = reference(COAX, frequency, 512);
        let fine = reference(COAX, frequency, 1024);
        close(coarse.inner_loss, fine.inner_loss, 1e-5);
        close(coarse.return_loss, fine.return_loss, 1e-5);
        close(coarse.impedance.re, fine.impedance.re, 1e-5);
        close(coarse.impedance.im, fine.impedance.im, 1e-5);
        close(fine.inner_loss + fine.return_loss, fine.impedance.re, 1e-5);
    }
    let frequency = 25e6;
    let omega = 2. * PI * frequency;
    let surface_resistance = (omega * MU0 / (2. * COAX.sigma)).sqrt();
    let expected_inner = surface_resistance / (2. * PI * COAX.a);
    let expected_return = surface_resistance / (2. * PI * COAX.b);
    let high = reference(COAX, frequency, 1024);
    close(high.inner_loss, expected_inner, 0.04);
    close(high.return_loss, expected_return, 0.04);
    let external = omega * MU0 * (COAX.b / COAX.a).ln() / (2. * PI);
    close(
        high.impedance.im - external,
        expected_inner + expected_return,
        0.04,
    );
}

#[derive(Clone, Copy)]
struct MeshProfile {
    angular: usize,
    radial_metal: usize,
    radial_dielectric: usize,
    axial_step: f64,
    far_radius: f64,
}

type Sample = Result<(C, [f64; 2]), String>;
const GMRES_RESTARTS: usize = 16;

fn sample(
    profile: MeshProfile,
    length: f64,
    frequencies: &[f64],
    budget: AllocationBudget,
    deadline: Instant,
    progress: &mut std::fs::File,
) -> Vec<Sample> {
    let Coax { a, b, c, sigma } = COAX;
    let MeshProfile {
        angular,
        radial_metal,
        radial_dielectric,
        axial_step,
        far_radius,
    } = profile;
    let mut xy_m = vec![[0., 0.]];
    let mut triangles = Vec::new();
    let mut region = Vec::new();
    let spans = [
        (0., a, radial_metal),
        (a, b, radial_dielectric),
        (b, c, radial_metal),
        (c, far_radius, radial_dielectric),
    ];
    let mut previous = 0;
    let mut ring_nodes = Vec::new();
    for (material, (lo, hi, rings)) in spans.into_iter().enumerate() {
        let first = xy_m.len();
        for ring in 1..=rings {
            let radius = lo + (hi - lo) * ring as f64 / rings as f64;
            let start = xy_m.len();
            for i in 0..angular {
                let theta = 2. * PI * i as f64 / angular as f64;
                xy_m.push([radius * theta.cos(), radius * theta.sin()]);
            }
            for i in 0..angular {
                let j = (i + 1) % angular;
                if previous == 0 {
                    triangles.push([0, start + i, start + j]);
                    region.push(material);
                } else {
                    triangles.extend([
                        [previous + i, start + i, start + j],
                        [previous + i, start + j, previous + j],
                    ]);
                    region.extend([material; 2]);
                }
            }
            previous = start;
        }
        ring_nodes.push(first..xy_m.len());
    }
    let boundary_edges = (0..angular)
        .map(|i| [previous + i, previous + (i + 1) % angular])
        .collect();
    let nz = (length / axial_step).ceil() as usize;
    let z_m = (0..=nz)
        .map(|i| length * i as f64 / nz as f64)
        .chain([length + a / 2.])
        .collect();
    let copper = |id: &str| Material::Copper {
        source_ids: vec![id.into()],
    };
    let air = Material::Dielectric {
        label: "lossless vacuum".into(),
        epsilon_r: 1.,
    };
    let materials = (0..=nz)
        .map(|z| {
            region
                .iter()
                .map(|&r| match r {
                    0 => copper("signal"),
                    1 if z == nz => copper("short"),
                    2 => copper("return"),
                    _ => air.clone(),
                })
                .collect()
        })
        .collect();
    let mesh = extrude(
        &LayeredInput {
            xy_m,
            triangles,
            boundary_edges,
            z_m,
            materials,
        },
        budget,
    )
    .unwrap();
    // The physical remote short connects both conductors. Drive equal/opposite
    // terminal currents on the two finite input faces, with no omitted lead at
    // the far end. Absolute input impedance includes that short and fringe.
    let electrodes = [
        (0..ring_nodes[0].end).collect(),
        (ring_nodes[1].end - angular..ring_nodes[2].end).collect(),
    ];
    let space = volume_current::assemble(
        &mesh,
        &vec![sigma; mesh.copper_tetrahedra.len()],
        &electrodes,
        budget,
    )
    .unwrap();
    assert_eq!(space.components.len(), 1);
    let field = volume_field::Field::new(&mesh, &space, budget).unwrap();
    use pcb_analyze::extraction::local::volume_field::ElectricApply;
    let magnetic_budget = AllocationBudget {
        max_bytes: budget
            .max_bytes
            .checked_sub(space.planned_bytes + field.planned_bytes())
            .unwrap(),
        ..budget
    };
    assert!(Instant::now() < deadline);
    let magnetic = volume_integral_magnetic::Operator::new(&mesh, magnetic_budget, 8).unwrap();
    let solver = volume_solve::Solver::new(&space, &field, &magnetic, budget).unwrap();
    let terminals = electrodes
        .each_ref()
        .map(|nodes| space.node_potential[&nodes[0]]);
    let mut drive = vec![0.; space.potential_count()];
    drive[terminals[0]] = 1.;
    drive[terminals[1]] = -1.;
    let zero = vec![0.; drive.len()];
    let solve_frequency = |f| -> Sample {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("campaign deadline before frequency solve")?;
        let solution =
            solver.solve_with_restart_limit(f, &drive, &zero, remaining, GMRES_RESTARTS)?;
        let [r, x] = solver.port_voltage(&solution, terminals[0], terminals[1]);
        let real = space.losses(&solution.real_currents_a).unwrap();
        let imag = space.losses(&solution.imaginary_currents_a).unwrap();
        // Aggregate a centered slab range with whole tetrahedra so normalization
        // uses the actual selected axial length at each independent axial step.
        let lo = (nz / 3) as f64 * length / nz as f64;
        let hi = (nz - nz / 3) as f64 * length / nz as f64;
        let mut loss = [0.; 2];
        for (i, cell) in space.cells.iter().enumerate() {
            let tet = &mesh.copper_tetrahedra[cell.mesh_cell];
            let z = tet
                .nodes
                .iter()
                .map(|&node| mesh.nodes_m[node][2])
                .sum::<f64>()
                / 4.;
            if z > lo && z < hi {
                let region = usize::from(tet.source_ids.iter().any(|id| id == "return"));
                loss[region] += (real[i] + imag[i]) / (hi - lo);
            }
        }
        Ok((C::new(r, x), loss))
    };
    frequencies.iter().map(|&f| {
            use std::io::Write;
            let point = solve_frequency(f);
            let event = match &point {
                Ok((z, loss)) => serde_json::json!({"lengthM":length,"frequencyHz":f,"status":"converged","inputImpedanceOhm":[z.re,z.im],"centralRegionalLossWPerM":loss}),
                Err(error) => serde_json::json!({"lengthM":length,"frequencyHz":f,"status":"failed","error":error}),
            };
            writeln!(progress, "{event}").expect("write progress event");
            progress.flush().expect("flush progress event");
            point
        })
        .collect()
}

/// Developer evidence only. Run under the external cold memory/time supervisor.
#[test]
#[ignore = "full 3D direct magnetic coupon; run under external cold resource caps"]
fn finite_shorted_coax_volume_transition_diagnostic() {
    let axis = [25e3, 100e3, 400e3];
    let integer_limit = |name: &str, default, lower, upper| {
        let value = std::env::var(name).ok().map_or(default, |value| {
            value
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {name}"))
        });
        assert!((lower..=upper).contains(&value), "{name} out of range");
        value
    };
    let seconds = integer_limit("P7_VERIFY_SECONDS", 300, 1, 1800);
    let memory_mib = integer_limit("P7_VERIFY_MEMORY_MIB", 512, 256, 3072);
    let budget = AllocationBudget {
        max_bytes: memory_mib * 1024 * 1024,
        max_nodes: 10000,
        max_tetrahedra: 20000,
    };
    let deadline = Instant::now() + Duration::from_secs(seconds as u64);
    let base = MeshProfile {
        angular: 8,
        radial_metal: 1,
        radial_dielectric: 1,
        axial_step: 1e-3,
        far_radius: 3e-3,
    };
    let label = std::env::var("P7_COAX_PROFILE").expect("select exactly one P7_COAX_PROFILE");
    let profile = match label.as_str() {
        "base" => base,
        "radial" => MeshProfile {
            radial_metal: 2,
            ..base
        },
        "angular" => MeshProfile {
            angular: 12,
            ..base
        },
        "axial" => MeshProfile {
            axial_step: 0.75e-3,
            ..base
        },
        "domain" => MeshProfile {
            far_radius: 4.5e-3,
            ..base
        },
        _ => panic!("P7_COAX_PROFILE must be base, radial, angular, axial, or domain"),
    };
    let output = std::env::var_os("P7_COAX_REPORT").expect("provide a new P7_COAX_REPORT path");
    assert!(
        !std::path::Path::new(&output).exists(),
        "report already exists"
    );
    let lengths = [3e-3, 5e-3];
    let progress_path = std::path::Path::new(&output).with_extension("progress.jsonl");
    let mut progress = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(progress_path)
        .unwrap();
    let short = sample(profile, lengths[0], &axis, budget, deadline, &mut progress);
    let long = sample(profile, lengths[1], &axis, budget, deadline, &mut progress);
    let mut samples = Vec::new();
    let mut max_error = 0_f64;
    let mut compared = 0;
    let pair = |z: C| [z.re, z.im];
    for (index, &f) in axis.iter().enumerate() {
        let exact = reference(COAX, f, 1024);
        let expected = serde_json::json!({"seriesImpedanceOhmPerM": pair(exact.impedance), "regionalLossWPerM": [exact.inner_loss, exact.return_loss]});
        let (Ok(&(short_z, _)), Ok(&(long_z, loss))) =
            (short[index].as_ref(), long[index].as_ref())
        else {
            samples.push(serde_json::json!({
                "frequencyHz":f,"status":"incomplete","expected":expected,
                "errors":[short[index].as_ref().err(), long[index].as_ref().err()]
            }));
            continue;
        };
        let gradient = (long_z - short_z) / (lengths[1] - lengths[0]);
        let impedance_error = (gradient - exact.impedance).norm() / exact.impedance.norm();
        let loss_errors = [
            (loss[0] / exact.inner_loss - 1.).abs(),
            (loss[1] / exact.return_loss - 1.).abs(),
        ];
        assert!(impedance_error.is_finite() && loss_errors.iter().all(|v| v.is_finite()));
        compared += 1;
        max_error = max_error
            .max(impedance_error)
            .max(loss_errors[0])
            .max(loss_errors[1]);
        samples.push(serde_json::json!({
            "frequencyHz": f,
            "status":"compared",
            "inputImpedanceOhm": [pair(short_z), pair(long_z)],
            "actual": {"seriesImpedanceOhmPerM": pair(gradient), "regionalLossWPerM": loss},
            "expected": expected,
            "relativeError": {"seriesImpedance": impedance_error, "regionalLoss": loss_errors}
        }));
    }
    let classification = if compared != axis.len() {
        "incomplete"
    } else if max_error <= 0.05 {
        "within-target"
    } else if max_error <= 0.065 {
        "requires-review"
    } else {
        "exceeds-review-range"
    };
    let report = serde_json::json!({
        "schema": "p7-coax-volume-diagnostic-v2",
        "scope": "finite shorted coax compared against infinite coax; one mesh profile",
        "physicalAdmission": false,
        "profile": label,
        "mesh": {"angular": profile.angular, "radialMetal": profile.radial_metal, "radialDielectric": profile.radial_dielectric, "axialStepM": profile.axial_step, "farRadiusM": profile.far_radius},
        "conductors": {"radiiM": [COAX.a, COAX.b, COAX.c], "conductivitySM": COAX.sigma},
        "lengthsM": lengths,
        "driveRmsA": 1.,
        "complexPairs": ["real", "imaginary"],
        "regionalOrder": ["signal", "return"],
        "nativeSeconds": seconds,
        "nativeMemoryMiB": memory_mib,
        "gmresRestartLimit": GMRES_RESTARTS,
        "samples": samples,
        "comparison": {"comparedPoints": compared, "maxRelativeError": (compared > 0).then_some(max_error), "target": 0.05, "reviewCeiling": 0.065, "classification": classification}
    });
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(file, &report).unwrap();
    println!("{report}");
    assert_eq!(
        compared,
        axis.len(),
        "frequency solve incomplete; retained report and progress"
    );
}
