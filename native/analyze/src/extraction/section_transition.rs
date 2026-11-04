#![allow(dead_code)] // Internal proof; polygon transport is a separate integration.
use super::{error, field, Result, MU0};
use crate::electrical::frequency::{math, Quality};
use faer::{
    c64,
    dyn_stack::{MemBuffer, MemStack},
    perm::PermRef,
    sparse::{
        linalg::lu::simplicial::{
            factorize_simplicial_numeric_lu, factorize_simplicial_numeric_lu_scratch,
            solve_in_place_scratch, SimplicialLu,
        },
        SparseColMat, Triplet,
    },
    Conj, Mat, Par,
};
use std::{collections::BTreeSet, time::Instant};

const AUDIT_TOLERANCE: f64 = 1e-8;

pub(super) struct Conductor<'a> {
    pub id: &'a str,
    pub conductivity_s_m: f64,
}

pub(super) struct Triangle {
    pub nodes: [usize; 3],
    pub conductor: Option<usize>,
}

pub(super) struct Mesh<'a> {
    pub nodes_m: &'a [[f64; 2]],
    pub triangles: &'a [Triangle],
    pub conductors: &'a [Conductor<'a>],
    pub reference: usize,
}

pub(super) struct Budget {
    pub max_bytes: usize,
    pub deadline: Instant,
}

impl Budget {
    fn check(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            return Err(error(
                "SECTION_TRANSITION_DEADLINE",
                "shared deadline exhausted",
            ));
        }
        Ok(())
    }
}

struct Element {
    nodes: [usize; 3],
    conductor: Option<usize>,
    area: f64,
    gradient: [[f64; 2]; 3],
}

pub(super) struct Response<'a> {
    pub signal_ids: Vec<&'a str>,
    /// Original physical mesh order, including the reference; loss columns use this order.
    pub conductor_order: Vec<&'a str>,
    pub reference_id: &'a str,
    pub series_impedance_ohm_per_m: math::Matrix,
    pub resistance_ohm_per_m: Vec<Vec<f64>>,
    pub total_inductance_h_per_m: Option<Vec<Vec<f64>>>,
    /// One row per 1 A RMS loop input, one column per physical conductor.
    pub conductor_loss_w_per_m: Vec<Vec<f64>>,
    pub max_field_residual: f64,
    pub max_current_residual_a: f64,
    pub max_reciprocity_error: f64,
    pub max_complex_power_error: f64,
    pub planned_bytes: usize,
}

pub(super) struct TriangleCurrent {
    pub triangle_index: usize,
    pub nodes: [usize; 3],
    pub conductor: usize,
    /// Each loop is in Response.signal_ids order, with 1 A RMS on its signal.
    pub jz_a_per_m2: Vec<[math::C; 3]>,
}

pub(super) struct Recovery {
    pub triangles: Vec<TriangleCurrent>,
    /// One Hermitian matrix per original conductor; loss for drive d is dᴴ G d.
    pub joule_gram_w_per_m: Vec<math::Matrix>,
    pub max_integrated_current_error_a: f64,
    pub max_joule_gram_error: f64,
    pub planned_output_bytes: usize,
}

struct RecoveryPlan {
    owned_triangles: usize,
    output_bytes: usize,
    live_bytes: usize,
}

fn recovery_plan(mesh: &Mesh<'_>, max_output_bytes: usize) -> Result<RecoveryPlan> {
    let owned_triangles = mesh
        .triangles
        .iter()
        .filter(|triangle| triangle.conductor.is_some())
        .count();
    let nc = mesh.conductors.len();
    let np = nc - 1;
    (|| {
        // Include owned DTO metadata, allocations per triangle and Gram row,
        // and a separate live allowance for current/Gram integration scratch.
        let triangle_bytes = size_of::<TriangleCurrent>()
            .checked_add(32)?
            .checked_add(np.checked_mul(size_of::<[math::C; 3]>())?)?;
        let gram_row = size_of::<Vec<math::C>>()
            .checked_add(32)?
            .checked_add(np.checked_mul(size_of::<math::C>())?)?;
        let gram = size_of::<math::Matrix>()
            .checked_add(32)?
            .checked_add(np.checked_mul(gram_row)?)?;
        let output_bytes = size_of::<Recovery>()
            .checked_add(owned_triangles.checked_mul(triangle_bytes)?)?
            .checked_add(nc.checked_mul(gram)?)?;
        if output_bytes > max_output_bytes {
            return None;
        }
        let live_bytes = output_bytes
            .checked_add(nc.checked_mul(np)?.checked_mul(64)?)?
            .checked_add(4096)?;
        Some(RecoveryPlan {
            owned_triangles,
            output_bytes,
            live_bytes,
        })
    })()
    .ok_or_else(|| {
        error(
            "SECTION_TRANSITION_RECOVERY_BUDGET",
            "triangle current or Joule Gram output exceeds byte budget",
        )
    })
}

fn recover(
    mesh: &Mesh<'_>,
    elements: &[Element],
    response: &Response<'_>,
    signals: &[usize],
    plan: &RecoveryPlan,
    budget: &Budget,
    current_density: impl Fn(usize, usize, usize) -> c64,
) -> Result<Recovery> {
    let nc = mesh.conductors.len();
    let np = signals.len();
    let mut result = Recovery {
        triangles: Vec::with_capacity(plan.owned_triangles),
        joule_gram_w_per_m: (0..nc).map(|_| math::zero(np, np)).collect(),
        max_integrated_current_error_a: 0.,
        max_joule_gram_error: 0.,
        planned_output_bytes: plan.output_bytes,
    };
    let mut integrated = vec![vec![c64::new(0., 0.); np]; nc];
    for (triangle_index, e) in elements.iter().enumerate() {
        budget.check()?;
        let Some(owner) = e.conductor else {
            continue;
        };
        let mut row = TriangleCurrent {
            triangle_index,
            nodes: e.nodes,
            conductor: owner,
            jz_a_per_m2: Vec::with_capacity(np),
        };
        let root_sigma = mesh.conductors[owner].conductivity_s_m.sqrt();
        for input in 0..np {
            let j = e.nodes.map(|node| current_density(input, owner, node));
            if j.iter().any(|v| !v.re.is_finite() || !v.im.is_finite()) {
                return Err(error(
                    "SECTION_TRANSITION_RECOVERY",
                    "nonfinite P1 current density",
                ));
            }
            integrated[owner][input] += j.into_iter().map(|v| v * (e.area / 3.)).sum::<c64>();
            row.jz_a_per_m2.push(j.map(|v| math::C::new(v.re, v.im)));
        }
        for (i, a) in row.jz_a_per_m2.iter().enumerate() {
            let a = a.map(|v| c64::new(v.real / root_sigma, v.imaginary / root_sigma));
            for (j, b) in row.jz_a_per_m2.iter().enumerate() {
                let b = b.map(|v| c64::new(v.real / root_sigma, v.imaginary / root_sigma));
                let product = a.into_iter().zip(b).map(|(x, y)| x.conj() * y).sum::<c64>();
                let gram = (product
                    + a.into_iter().sum::<c64>().conj() * b.into_iter().sum::<c64>())
                    * (e.area / 12.);
                let entry = &mut result.joule_gram_w_per_m[owner][i][j];
                *entry = *entry + math::C::new(gram.re, gram.im);
            }
        }
        result.triangles.push(row);
    }
    for (owner, gram) in result.joule_gram_w_per_m.iter().enumerate() {
        let scale = (0..np)
            .map(|i| response.conductor_loss_w_per_m[i][owner])
            .fold(0., f64::max)
            .max(f64::MIN_POSITIVE);
        for i in 0..np {
            let expected =
                (owner == signals[i]) as u8 as f64 - (owner == mesh.reference) as u8 as f64;
            let current_error = (integrated[owner][i] - expected).norm();
            let diagonal = math::C::new(response.conductor_loss_w_per_m[i][owner], 0.);
            let diagonal_error = (gram[i][i] - diagonal).abs() / scale;
            if !current_error.is_finite()
                || current_error > AUDIT_TOLERANCE
                || !diagonal_error.is_finite()
                || diagonal_error > AUDIT_TOLERANCE
            {
                return Err(error("SECTION_TRANSITION_RECOVERY", "integrated conductor current or Joule diagonal disagrees with the solved state"));
            }
            result.max_integrated_current_error_a =
                result.max_integrated_current_error_a.max(current_error);
            result.max_joule_gram_error = result.max_joule_gram_error.max(diagonal_error);
            for j in 0..i {
                let mismatch = (gram[i][j] - gram[j][i].conj()).abs() / scale;
                if !mismatch.is_finite() || mismatch > AUDIT_TOLERANCE {
                    return Err(error(
                        "SECTION_TRANSITION_RECOVERY",
                        "regional Joule Gram is not Hermitian",
                    ));
                }
                result.max_joule_gram_error = result.max_joule_gram_error.max(mismatch);
            }
        }
    }
    Ok(result)
}

fn mass(area: f64, i: usize, j: usize) -> f64 {
    area * if i == j { 1. / 6. } else { 1. / 12. }
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn root(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

pub(super) fn solve<'a>(
    mesh: &Mesh<'a>,
    frequency_hz: f64,
    anchor: usize,
    budget: &Budget,
) -> Result<Response<'a>> {
    Ok(solve_internal(mesh, frequency_hz, anchor, budget, None)?.0)
}

pub(super) fn solve_with_recovery<'a>(
    mesh: &Mesh<'a>,
    frequency_hz: f64,
    anchor: usize,
    budget: &Budget,
    max_output_bytes: usize,
) -> Result<(Response<'a>, Recovery)> {
    let (response, recovery) =
        solve_internal(mesh, frequency_hz, anchor, budget, Some(max_output_bytes))?;
    Ok((
        response,
        recovery.ok_or_else(|| {
            error(
                "SECTION_TRANSITION_RECOVERY",
                "requested recovery was not produced",
            )
        })?,
    ))
}

fn solve_internal<'a>(
    mesh: &Mesh<'a>,
    frequency_hz: f64,
    anchor: usize,
    budget: &Budget,
    max_output_bytes: Option<usize>,
) -> Result<(Response<'a>, Option<Recovery>)> {
    budget.check()?;
    let (nn, nc) = (mesh.nodes_m.len(), mesh.conductors.len());
    let omega = std::f64::consts::TAU * frequency_hz;
    if nn < 3
        || anchor >= nn
        || !(2..=17).contains(&nc)
        || mesh.reference >= nc
        || !frequency_hz.is_finite()
        || frequency_hz < 0.
        || !omega.is_finite()
        || mesh.triangles.is_empty()
        || mesh.nodes_m.iter().flatten().any(|x| !x.is_finite())
    {
        return Err(error(
            "SECTION_TRANSITION_INPUT",
            "finite mesh, frequency, reference and gauge required",
        ));
    }
    let ids = mesh
        .conductors
        .iter()
        .map(|c| c.id)
        .collect::<BTreeSet<_>>();
    if ids.len() != nc
        || mesh.conductors.iter().any(|c| {
            c.id.trim().is_empty() || !c.conductivity_s_m.is_finite() || c.conductivity_s_m <= 0.
        })
    {
        return Err(error(
            "SECTION_TRANSITION_INPUT",
            "unique physical conductor IDs and positive finite conductivity required",
        ));
    }
    let recovery_plan = max_output_bytes
        .map(|cap| recovery_plan(mesh, cap))
        .transpose()?;
    let nf = nn - 1;
    let ac = frequency_hz > 0.;
    let factor_scratch = factorize_simplicial_numeric_lu_scratch::<usize, c64>(nf, nf);
    let solve_scratch = solve_in_place_scratch::<usize, c64>(nf, nc, Par::Seq);
    let planned_bytes = (|| {
        let linear = nn
            .checked_mul(64)?
            .checked_add(mesh.triangles.len().checked_mul(128)?)?
            .checked_add(nc.checked_mul(nc)?.checked_mul(128)?)?
            .checked_add(4096)?;
        if !ac {
            return Some(linear);
        }
        // faer 0.24.4 simplicial numeric LU has two triangular factors,
        // each requesting at most triangular entries plus one full column
        // (the implementation reserves before separating L and U entries).
        // Four growing Vecs store indices/values: allow old capacity and
        // twice the largest request during amortized reallocation.
        let slots = nf
            .checked_mul(nf.checked_add(1)?)?
            .checked_div(2)?
            .checked_add(nf)?
            .checked_add(1)?;
        let entry_bytes = size_of::<usize>().checked_add(size_of::<c64>())?;
        linear
            .checked_add(slots.checked_mul(entry_bytes)?.checked_mul(2 * 3)?)?
            .checked_add(nn.checked_mul(nc)?.checked_mul(256)?)?
            .checked_add(mesh.triangles.len().checked_mul(768)?)?
            .checked_add(
                factor_scratch
                    .unaligned_bytes_required()
                    .max(solve_scratch.unaligned_bytes_required()),
            )?
            .checked_add(1 << 20)
    })()
    .and_then(|n| n.checked_add(recovery_plan.as_ref().map_or(0, |plan| plan.live_bytes)))
    .filter(|&n| n <= budget.max_bytes)
    .ok_or_else(|| {
        error(
            "SECTION_TRANSITION_MEMORY",
            "field factor/RHS byte plan exceeds shared budget",
        )
    })?;
    let mut elements = Vec::with_capacity(mesh.triangles.len());
    let mut d = vec![0.; nc];
    let mut parent = (0..nn).collect::<Vec<_>>();
    for triangle in mesh.triangles {
        budget.check()?;
        if triangle.nodes.iter().any(|&i| i >= nn) || triangle.conductor.is_some_and(|i| i >= nc) {
            return Err(error(
                "SECTION_TRANSITION_INPUT",
                "triangle indices outside typed mesh",
            ));
        }
        let (area, gradient) = field::element(mesh.nodes_m, triangle.nodes)?;
        let component = root(&mut parent, triangle.nodes[0]);
        for &node in &triangle.nodes[1..] {
            let other = root(&mut parent, node);
            parent[other] = component;
        }
        if let Some(i) = triangle.conductor {
            d[i] += area * mesh.conductors[i].conductivity_s_m;
        }
        elements.push(Element {
            nodes: triangle.nodes,
            conductor: triangle.conductor,
            area,
            gradient,
        });
    }
    let component = root(&mut parent, anchor);
    if (0..nn).any(|node| root(&mut parent, node) != component) {
        return Err(error(
            "SECTION_TRANSITION_INPUT",
            "connected magnetic domain with no unused vertices required",
        ));
    }
    if d.iter()
        .any(|x| !x.is_finite() || *x <= 0. || !x.recip().is_finite())
    {
        return Err(error(
            "SECTION_TRANSITION_INPUT",
            "every conductor needs positive representable mesh conductance",
        ));
    }
    let signals = (0..nc).filter(|&i| i != mesh.reference).collect::<Vec<_>>();
    let np = signals.len();
    let mut response = Response {
        signal_ids: signals.iter().map(|&i| mesh.conductors[i].id).collect(),
        conductor_order: mesh
            .conductors
            .iter()
            .map(|conductor| conductor.id)
            .collect(),
        reference_id: mesh.conductors[mesh.reference].id,
        series_impedance_ohm_per_m: math::zero(np, np),
        resistance_ohm_per_m: vec![vec![0.; np]; np],
        total_inductance_h_per_m: None,
        conductor_loss_w_per_m: vec![vec![0.; nc]; np],
        max_field_residual: 0.,
        max_current_residual_a: 0.,
        max_reciprocity_error: 0.,
        max_complex_power_error: 0.,
        planned_bytes,
    };
    // The only supported excitations are the prescribed physical loop columns
    // Q = [e_signal − e_reference]. Each is exactly balanced before assembly.
    let current = |metal, input| {
        (metal == signals[input]) as u8 as f64 - (metal == mesh.reference) as u8 as f64
    };
    if frequency_hz == 0. {
        for input in 0..np {
            for output in 0..np {
                let resistance = current(signals[output], input) / d[signals[output]]
                    + d[mesh.reference].recip();
                if !resistance.is_finite() {
                    return Err(error(
                        "SECTION_TRANSITION_RESIDUAL",
                        "nonfinite DC loop resistance",
                    ));
                }
                response.series_impedance_ohm_per_m[output][input].real = resistance;
                response.resistance_ohm_per_m[output][input] = resistance;
            }
            for (metal, loss) in response.conductor_loss_w_per_m[input]
                .iter_mut()
                .enumerate()
            {
                *loss = current(metal, input).powi(2) / d[metal];
            }
        }
        let recovery = recovery_plan
            .as_ref()
            .map(|plan| {
                recover(
                    mesh,
                    &elements,
                    &response,
                    &signals,
                    plan,
                    budget,
                    |input, owner, _| {
                        c64::new(
                            current(owner, input)
                                * (mesh.conductors[owner].conductivity_s_m / d[owner]),
                            0.,
                        )
                    },
                )
            })
            .transpose()?;
        budget.check()?;
        return Ok((response, recovery));
    }

    let free = |node| node - usize::from(node > anchor);
    let mut b = Mat::<c64>::zeros(nn - 1, nc);
    let mut triplets = Vec::with_capacity(elements.len() * 9);
    // Factor μ0(K+jωM). The common scale cancels from X=(K+jωM)^−1B.
    for e in &elements {
        budget.check()?;
        let sigma = e
            .conductor
            .map_or(0., |i| mesh.conductors[i].conductivity_s_m);
        for i in 0..3 {
            let row = e.nodes[i];
            if row == anchor {
                continue;
            }
            if let Some(metal) = e.conductor {
                b[(free(row), metal)].re += e.area * (sigma * MU0) / 3.;
            }
            for j in 0..3 {
                let col = e.nodes[j];
                if col != anchor {
                    let v = c64::new(
                        e.area * dot(e.gradient[i], e.gradient[j]),
                        omega * (sigma * MU0) * mass(e.area, i, j),
                    );
                    if !v.re.is_finite() || !v.im.is_finite() {
                        return Err(error(
                            "SECTION_TRANSITION_MATRIX",
                            "nonfinite element coefficient",
                        ));
                    }
                    triplets.push(Triplet::new(free(row), free(col), v));
                }
            }
        }
    }
    budget.check()?;
    let a = SparseColMat::try_new_from_triplets(nn - 1, nn - 1, &triplets)
        .map_err(|e| error("SECTION_TRANSITION_MATRIX", e))?;
    drop(triplets);
    // Identity column ordering avoids unbudgeted symbolic supernodal
    // machinery. Simplicial LU still performs numerical row pivoting.
    let columns = (0..nf).collect::<Vec<_>>();
    let columns = PermRef::new_checked(&columns, &columns, nf);
    let mut row_perm = vec![0usize; nf];
    let mut row_perm_inv = vec![0usize; nf];
    let mut factor = SimplicialLu::new();
    {
        let mut scratch = MemBuffer::try_new(factor_scratch)
            .map_err(|e| error("SECTION_TRANSITION_MEMORY", e))?;
        factorize_simplicial_numeric_lu(
            &mut row_perm,
            &mut row_perm_inv,
            &mut factor,
            a.as_ref(),
            columns,
            MemStack::new(&mut scratch),
        )
        .map_err(|e| error("SECTION_TRANSITION_FACTOR", e))?;
    }
    budget.check()?;
    let rows = PermRef::new_checked(&row_perm, &row_perm_inv, nf);
    let mut x = b.clone();
    {
        let mut scratch =
            MemBuffer::try_new(solve_scratch).map_err(|e| error("SECTION_TRANSITION_MEMORY", e))?;
        factor.solve_in_place_with_conj(
            rows,
            columns,
            Conj::No,
            x.as_mut(),
            Par::Seq,
            MemStack::new(&mut scratch),
        );
    }
    if (0..nn - 1).any(|i| (0..nc).any(|j| !x[(i, j)].re.is_finite() || !x[(i, j)].im.is_finite()))
    {
        return Err(error(
            "SECTION_TRANSITION_FACTOR",
            "nonfinite field solution",
        ));
    }
    drop((factor, a));
    budget.check()?;

    let mut schur = math::zero(nc, nc);
    for i in 0..nc {
        for j in 0..nc {
            let bx = (0..nn - 1).fold(c64::new(0., 0.), |sum, k| sum + b[(k, i)] * x[(k, j)]);
            let term = c64::new(0., omega / MU0) * bx;
            schur[i][j] = math::C::new(if i == j { d[i] - term.re } else { -term.re }, -term.im);
        }
    }
    let rhs = (0..nc)
        .map(|metal| {
            (0..np)
                .map(|input| math::C::new(current(metal, input), 0.))
                .collect()
        })
        .collect();
    let gradients = math::solve(&schur, &rhs, &mut Quality::default())?;
    let jw = c64::new(0., omega);
    let mut states = Vec::with_capacity(np);
    for input in 0..np {
        budget.check()?;
        let electric = gradients
            .iter()
            .map(|row| c64::new(row[input].real, row[input].imaginary))
            .collect::<Vec<_>>();
        let potential = (0..nn)
            .map(|node| {
                if node == anchor {
                    c64::new(0., 0.)
                } else {
                    (0..nc).fold(c64::new(0., 0.), |sum, i| {
                        sum + x[(free(node), i)] * electric[i]
                    })
                }
            })
            .collect::<Vec<_>>();
        if potential
            .iter()
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
        {
            return Err(error(
                "SECTION_TRANSITION_RESIDUAL",
                "nonfinite reconstructed vector potential",
            ));
        }
        let mut field_residual = vec![c64::new(0., 0.); nn];
        let mut field_scale = vec![0.; nn];
        let mut recovered = vec![c64::new(0., 0.); nc];
        for e in &elements {
            budget.check()?;
            let az = e.nodes.map(|i| potential[i]);
            let j_over_sigma = e.conductor.map(|i| az.map(|a| electric[i] - jw * a));
            let grad_a = [0, 1]
                .map(|k| (0..3).fold(c64::new(0., 0.), |sum, i| sum + az[i] * e.gradient[i][k]));
            if let (Some(metal), Some(j)) = (e.conductor, j_over_sigma) {
                let sigma = mesh.conductors[metal].conductivity_s_m;
                let total = j.into_iter().sum::<c64>();
                response.conductor_loss_w_per_m[input][metal] += sigma * e.area / 12.
                    * (total.norm_sqr() + j.into_iter().map(|v| v.norm_sqr()).sum::<f64>());
            }
            for i in 0..3 {
                let row = e.nodes[i];
                let diffusion =
                    (grad_a[0] * e.gradient[i][0] + grad_a[1] * e.gradient[i][1]) * (e.area / MU0);
                field_residual[row] += diffusion;
                field_scale[row] += diffusion.norm();
                if let (Some(metal), Some(j)) = (e.conductor, j_over_sigma) {
                    let sigma = mesh.conductors[metal].conductivity_s_m;
                    recovered[metal] += j[i] * (sigma * e.area / 3.);
                    for (k, &other) in j.iter().enumerate() {
                        let m = sigma * mass(e.area, i, k);
                        field_residual[row] -= other * m;
                        field_scale[row] += other.norm() * m;
                    }
                }
            }
        }
        if field_residual
            .iter()
            .chain(&recovered)
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
            || field_scale.iter().any(|v| !v.is_finite())
        {
            return Err(error(
                "SECTION_TRANSITION_RESIDUAL",
                "nonfinite original field/current audit",
            ));
        }
        // Original anchored-away row participates in exactly the same audit.
        let scale = field_scale
            .into_iter()
            .fold(0., f64::max)
            .max(f64::MIN_POSITIVE);
        let field_error = field_residual
            .iter()
            .map(|v| v.norm() / scale)
            .fold(0., f64::max);
        let current_error = recovered
            .iter()
            .enumerate()
            .map(|(i, &v)| (v - current(i, input)).norm())
            .fold(0., f64::max);
        if !field_error.is_finite()
            || !current_error.is_finite()
            || field_error > AUDIT_TOLERANCE
            || current_error > AUDIT_TOLERANCE
            || response.conductor_loss_w_per_m[input]
                .iter()
                .any(|loss| !loss.is_finite() || *loss < 0.)
        {
            return Err(error(
                "SECTION_TRANSITION_RESIDUAL",
                format!("field={field_error:e}, current={current_error:e}"),
            ));
        }
        response.max_field_residual = response.max_field_residual.max(field_error);
        response.max_current_residual_a = response.max_current_residual_a.max(current_error);
        for (output, &metal) in signals.iter().enumerate() {
            let z = electric[metal] - electric[mesh.reference];
            if !z.re.is_finite() || !z.im.is_finite() {
                return Err(error(
                    "SECTION_TRANSITION_RESIDUAL",
                    "nonfinite series impedance",
                ));
            }
            response.series_impedance_ohm_per_m[output][input] = math::C::new(z.re, z.im);
        }
        states.push((potential, electric));
    }
    let mut recovery = recovery_plan
        .as_ref()
        .map(|plan| {
            recover(
                mesh,
                &elements,
                &response,
                &signals,
                plan,
                budget,
                |input, owner, node| {
                    let (potential, electric) = &states[input];
                    (electric[owner] - jw * potential[node])
                        * mesh.conductors[owner].conductivity_s_m
                },
            )
        })
        .transpose()?;
    let mut inductance = vec![vec![0.; np]; np];
    let z = &response.series_impedance_ohm_per_m;
    let scale = (0..np)
        .map(|i| z[i][i].abs())
        .fold(0., f64::max)
        .max(f64::MIN_POSITIVE);
    // Cross-drive Hermitian Joule and magnetic Gram entries must reproduce
    // QᵀE; reciprocity of the complex-symmetric impedance is separate.
    for i in 0..np {
        for j in 0..np {
            budget.check()?;
            let (potential_i, electric_i) = &states[i];
            let (potential_j, electric_j) = &states[j];
            let mut joule = c64::new(0., 0.);
            let mut magnetic = c64::new(0., 0.);
            for (index, e) in elements.iter().enumerate() {
                if index % 1024 == 0 {
                    budget.check()?;
                }
                let a = e.nodes.map(|n| potential_i[n]);
                let b = e.nodes.map(|n| potential_j[n]);
                for k in 0..2 {
                    let gradient = |u: [c64; 3]| {
                        (0..3).fold(c64::new(0., 0.), |sum, q| sum + u[q] * e.gradient[q][k])
                    };
                    magnetic += gradient(a).conj() * gradient(b) * (e.area / MU0);
                }
                if let Some(metal) = e.conductor {
                    let sigma = mesh.conductors[metal].conductivity_s_m;
                    let a = a.map(|v| electric_i[metal] - jw * v);
                    let b = b.map(|v| electric_j[metal] - jw * v);
                    let product = a.into_iter().zip(b).map(|(x, y)| x.conj() * y).sum::<c64>();
                    joule += (product
                        + a.into_iter().sum::<c64>().conj() * b.into_iter().sum::<c64>())
                        * (sigma * e.area / 12.);
                }
            }
            let entry = c64::new(z[i][j].real, z[i][j].imaginary);
            let err = (entry - joule - jw * magnetic).norm() / scale;
            if !err.is_finite() || err > AUDIT_TOLERANCE {
                return Err(error(
                    "SECTION_TRANSITION_POWER",
                    format!("entry ({i},{j}) mismatch={err:e}"),
                ));
            }
            response.max_complex_power_error = response.max_complex_power_error.max(err);
            if !joule.re.is_finite()
                || !magnetic.re.is_finite()
                || [
                    joule.im,
                    omega * magnetic.im,
                    entry.re - joule.re,
                    entry.im - omega * magnetic.re,
                ]
                .into_iter()
                .any(|v| !v.is_finite() || v.abs() / scale > AUDIT_TOLERANCE)
            {
                return Err(error(
                    "SECTION_TRANSITION_POWER",
                    "real symmetric Joule/magnetic stamps disagree with observed impedance",
                ));
            }
            if i >= j {
                response.resistance_ohm_per_m[i][j] = joule.re;
                response.resistance_ohm_per_m[j][i] = joule.re;
                inductance[i][j] = magnetic.re;
                inductance[j][i] = magnetic.re;
            }
            if let Some(recovery) = &mut recovery {
                let total = recovery
                    .joule_gram_w_per_m
                    .iter()
                    .map(|g| c64::new(g[i][j].real, g[i][j].imaginary))
                    .sum::<c64>();
                let mismatch = (total - joule).norm() / scale;
                if !mismatch.is_finite() || mismatch > AUDIT_TOLERANCE {
                    return Err(error(
                        "SECTION_TRANSITION_RECOVERY",
                        "sum of regional Joule Grams disagrees with global power",
                    ));
                }
                recovery.max_joule_gram_error = recovery.max_joule_gram_error.max(mismatch);
            }
        }
    }
    for i in 0..np {
        for j in 0..i {
            response.max_reciprocity_error = response
                .max_reciprocity_error
                .max((z[i][j] - z[j][i]).abs() / scale);
        }
    }
    if response.max_reciprocity_error > AUDIT_TOLERANCE {
        return Err(error(
            "SECTION_TRANSITION_RECIPROCITY",
            response.max_reciprocity_error,
        ));
    }
    if z.iter()
        .flatten()
        .any(|value| !(value.imaginary / omega).is_finite())
    {
        return Err(error(
            "SECTION_TRANSITION_RESIDUAL",
            "nonfinite AC loop inductance",
        ));
    }
    response.total_inductance_h_per_m = Some(inductance);
    budget.check()?;
    Ok((response, recovery))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{f64::consts::PI, time::Duration};

    #[test]
    fn two_slab_dc_and_skin_transition_match_analytic_impedance_and_gauge() {
        let t = 0.001;
        let gap = 0.0005;
        let width = t / 12.;
        let sigma = 58e6;
        let frequency = 1. / (PI * MU0 * sigma * t * t); // δ=t
        let mut y = vec![0.];
        for (origin, span, divisions) in [(0., t, 24), (t, gap, 12), (t + gap, t, 24)] {
            y.extend((1..=divisions).map(|i| origin + span * i as f64 / divisions as f64));
        }
        let nodes = y
            .iter()
            .flat_map(|&y| [[0., y], [width / 2., y], [width, y]])
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for i in 0..y.len() - 1 {
            let conductor = if i < 24 {
                Some(0)
            } else if i >= 36 {
                Some(1)
            } else {
                None
            };
            let a = 3 * i;
            for nodes in [
                [a, a + 1, a + 4],
                [a, a + 4, a + 3],
                [a + 1, a + 2, a + 4],
                [a + 2, a + 5, a + 4],
            ] {
                triangles.push(Triangle { nodes, conductor });
            }
        }
        let conductors = [
            Conductor {
                id: "lower",
                conductivity_s_m: sigma,
            },
            Conductor {
                id: "upper",
                conductivity_s_m: sigma,
            },
        ];
        let mesh = Mesh {
            nodes_m: &nodes,
            triangles: &triangles,
            conductors: &conductors,
            reference: 1,
        };
        let budget = Budget {
            max_bytes: 32 * 1024 * 1024,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let dc = solve(&mesh, 0., 0, &budget).unwrap();
        let dc_exact = 2. / (sigma * width * t);
        assert!((dc.series_impedance_ohm_per_m[0][0].real / dc_exact - 1.).abs() < 1e-12);
        assert!(dc.total_inductance_h_per_m.is_none());

        let ac = solve(&mesh, frequency, 0, &budget).unwrap();
        let alternate = solve(&mesh, frequency, nodes.len() - 1, &budget).unwrap();
        let z = ac.series_impedance_ohm_per_m[0][0];
        let denominator = 2_f64.cosh() - 2_f64.cos();
        let exact = math::C::new(
            dc_exact * (2_f64.sinh() + 2_f64.sin()) / denominator,
            dc_exact * (2_f64.sinh() - 2_f64.sin()) / denominator
                + std::f64::consts::TAU * frequency * MU0 * gap / width,
        );
        let error = (z - exact).abs() / exact.abs();
        let loss_error = (ac.conductor_loss_w_per_m[0].iter().sum::<f64>() / exact.real - 1.).abs();
        eprintln!("slab f={frequency:.9} Hz, Z={z:?}, reference={exact:?}, Z error={error:e}, Cu loss error={loss_error:e}; gauge error={:e}",
            (z - alternate.series_impedance_ohm_per_m[0][0]).abs()/z.abs());

        assert!(
            error <= 0.02 && loss_error <= 0.02,
            "Z={z:?}, reference={exact:?}, error={error:e}, loss={loss_error:e}"
        );
        assert!(
            (z - alternate.series_impedance_ohm_per_m[0][0]).abs() / z.abs() <= AUDIT_TOLERANCE
        );
        assert!(ac.max_field_residual <= AUDIT_TOLERANCE);
        assert!(ac.max_current_residual_a <= AUDIT_TOLERANCE);
        assert!(ac.max_complex_power_error <= AUDIT_TOLERANCE);
    }

    #[test]
    fn three_conductor_cross_power_and_nonlast_reference_preserve_gauge() {
        let nodes = (0..=10)
            .flat_map(|i| [[0., i as f64 * 0.0005], [0.0005, i as f64 * 0.0005]])
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for i in 0..10 {
            let conductor = match i {
                0..=1 => Some(0),
                4..=5 => Some(1),
                8..=9 => Some(2),
                _ => None,
            };
            let a = 2 * i;
            for nodes in [[a, a + 1, a + 3], [a, a + 3, a + 2]] {
                triangles.push(Triangle { nodes, conductor });
            }
        }
        let conductors = [
            Conductor {
                id: "lower",
                conductivity_s_m: 58e6,
            },
            Conductor {
                id: "reference",
                conductivity_s_m: 40e6,
            },
            Conductor {
                id: "upper",
                conductivity_s_m: 30e6,
            },
        ];
        let mesh = Mesh {
            nodes_m: &nodes,
            triangles: &triangles,
            conductors: &conductors,
            reference: 1,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let dc_budget = Budget {
            max_bytes: 64 * 1024,
            deadline,
        };
        let dc = solve(&mesh, 0., 0, &dc_budget).unwrap();
        assert!(dc.planned_bytes < 64 * 1024);
        assert!(solve(&mesh, 5000., 0, &dc_budget).is_err());
        let budget = Budget {
            max_bytes: 8 * 1024 * 1024,
            deadline,
        };
        let (a, recovery) = solve_with_recovery(&mesh, 5000., 0, &budget, 64 * 1024).unwrap();
        let b = solve(&mesh, 5000., nodes.len() - 1, &budget).unwrap();
        assert_eq!(a.signal_ids, ["lower", "upper"]);
        assert_eq!(a.reference_id, "reference");
        assert_eq!(a.conductor_order, ["lower", "reference", "upper"]);
        assert_eq!(recovery.triangles.len(), 12);
        assert!(recovery.max_integrated_current_error_a <= AUDIT_TOLERANCE);
        assert!(recovery.max_joule_gram_error <= AUDIT_TOLERANCE);
        assert!(a.planned_bytes > b.planned_bytes);
        assert!(
            solve_with_recovery(&mesh, 5000., 0, &budget, recovery.planned_output_bytes - 1)
                .err()
                .unwrap()
                .contains("SECTION_TRANSITION_RECOVERY_BUDGET")
        );
        let tight = Budget {
            max_bytes: a.planned_bytes - 1,
            deadline,
        };
        assert!(solve_with_recovery(&mesh, 5000., 0, &tight, 64 * 1024)
            .err()
            .unwrap()
            .contains("SECTION_TRANSITION_MEMORY"));
        let (_, dc_recovery) = solve_with_recovery(&mesh, 0., 0, &dc_budget, 64 * 1024).unwrap();
        let dc_reference_cross = dc_recovery.joule_gram_w_per_m[1][0][1];
        assert!(
            (dc_reference_cross.real / dc.conductor_loss_w_per_m[0][1] - 1.).abs()
                <= AUDIT_TOLERANCE
        );
        assert_eq!(dc_reference_cross.imaginary, 0.);

        let drive = [math::C::new(1., 0.4), math::C::new(-0.7, 0.3)];
        let mut currents = [[math::C::default(); 2]; 3];
        let mut direct_loss = [0.; 3];
        for row in &recovery.triangles {
            let original = &mesh.triangles[row.triangle_index];
            assert_eq!(Some(row.conductor), original.conductor);
            assert_eq!(row.nodes, original.nodes);
            let (area, _) = field::element(mesh.nodes_m, row.nodes).unwrap();
            let combined = [0, 1, 2]
                .map(|v| row.jz_a_per_m2[0][v] * drive[0] + row.jz_a_per_m2[1][v] * drive[1]);
            for v in 0..3 {
                for input in 0..2 {
                    currents[row.conductor][input] =
                        currents[row.conductor][input] + row.jz_a_per_m2[input][v].scale(area / 3.);
                }
                for w in 0..3 {
                    direct_loss[row.conductor] += (combined[v].conj() * combined[w]).real
                        * mass(area, v, w)
                        / conductors[row.conductor].conductivity_s_m;
                }
            }
        }
        for (owner, expected) in [[1., 0.], [-1., -1.], [0., 1.]].into_iter().enumerate() {
            let gram = &recovery.joule_gram_w_per_m[owner];
            let mut total = math::C::default();
            for i in 0..2 {
                assert!(
                    (currents[owner][i] - math::C::new(expected[i], 0.)).abs() <= AUDIT_TOLERANCE
                );
                for j in 0..2 {
                    total = total + drive[i].conj() * gram[i][j] * drive[j];
                }
            }
            assert!(
                (total - math::C::new(direct_loss[owner], 0.)).abs() / direct_loss[owner]
                    <= AUDIT_TOLERANCE
            );
        }
        let canonical_l = a.total_inductance_h_per_m.as_ref().unwrap();
        assert_eq!(a.resistance_ohm_per_m[0][1], a.resistance_ohm_per_m[1][0]);
        assert_eq!(canonical_l[0][1], canonical_l[1][0]);
        assert_eq!(
            dc.resistance_ohm_per_m[0][1],
            dc.series_impedance_ohm_per_m[0][1].real
        );
        eprintln!(
            "recovery current={:e} A, Joule Gram={:e}, planned output={} B",
            recovery.max_integrated_current_error_a,
            recovery.max_joule_gram_error,
            recovery.planned_output_bytes
        );

        assert_eq!(dc.conductor_loss_w_per_m[0][2], 0.);
        assert_eq!(dc.conductor_loss_w_per_m[1][0], 0.);
        assert!(dc.conductor_loss_w_per_m.iter().all(|loss| loss[1] > 0.));
        let z = &a.series_impedance_ohm_per_m;
        let scale = z[0][0].abs().max(z[1][1].abs());
        assert!(z[0][1].imaginary.abs() > 1e-4 * scale);
        let mut gauge: f64 = 0.;
        for (i, row) in z.iter().enumerate() {
            for (j, &entry) in row.iter().enumerate() {
                gauge = gauge.max((entry - b.series_impedance_ohm_per_m[i][j]).abs() / scale);
            }
        }
        assert!(gauge <= AUDIT_TOLERANCE);
        assert!(a.max_reciprocity_error <= AUDIT_TOLERANCE);
        assert!(a.max_complex_power_error <= AUDIT_TOLERANCE);
        eprintln!(
            "three-conductor gauge={gauge:e}, reciprocity={:e}, complex-power={:e}",
            a.max_reciprocity_error, a.max_complex_power_error
        );
    }
}
