//! Block congruence in the physical current space. Potential coordinates,
//! charge, finite contacts and complete C remain unchanged. This is a bounded
//! projection primitive and bounded automatic current Krylov selection. This is
//! not a general descriptor PRIMA implementation or scalable PCB admission.
use super::{error, Result};
use crate::electrical::{
    coupled::{Entry, Model, System},
    Budget,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
pub struct Projection {
    pub model: Model,
    pub original_current_dofs: Vec<String>,
    /// Columns of Q, with Q^T Q=I. Original current i=Q*i_reduced.
    pub basis: Vec<Vec<f64>>,
    pub planned_bytes: usize,
    pub elapsed_seconds: f64,
}
fn congruent(
    input: &[Entry],
    q: &[Vec<f64>],
    start: Instant,
    wall: Duration,
) -> Result<Vec<Entry>> {
    let mut out = vec![];
    for a in 0..q.len() {
        for b in a..q.len() {
            if start.elapsed() > wall {
                return Err(error("DEADLINE", "projection matrix deadline"));
            }
            let v = input
                .iter()
                .map(|&(i, j, v)| q[a][i] * v * q[b][j])
                .sum::<f64>();
            if !v.is_finite() {
                return Err(error(
                    "REDUCTION_RANGE",
                    "nonfinite projected energy matrix",
                ));
            }
            if v != 0. {
                out.push((a, b, v));
                if a != b {
                    out.push((b, a, v));
                }
            }
        }
    }
    Ok(out)
}
pub fn project_currents(
    input: &Model,
    ports: usize,
    basis: Vec<Vec<f64>>,
    max_bytes: usize,
    max_wall: Duration,
) -> Result<Projection> {
    let start = Instant::now();
    let nv = input.potential_nodes.len();
    let ni = input.current_dofs.len();
    let nr = basis.len();
    let plan = (nv + ni)
        .checked_pow(2)
        .and_then(|n| n.checked_mul(1600))
        .and_then(|n| n.checked_add((nv + nr).checked_pow(2)?.checked_mul(1600)?))
        .and_then(|n| n.checked_add(ni.checked_mul(nr)?.checked_mul(32)?))
        .ok_or_else(|| error("MEMORY_LIMIT", "reduction allocation overflow"))?;
    if plan > max_bytes {
        return Err(error(
            "MEMORY_LIMIT",
            "simultaneous full/reduced P6 and basis plan",
        ));
    }
    if start.elapsed() > max_wall {
        return Err(error("DEADLINE", "projection deadline"));
    }
    if nr == 0
        || nr > ni
        || basis
            .iter()
            .any(|v| v.len() != ni || v.iter().any(|x| !x.is_finite()))
    {
        return Err(error(
            "REDUCTION_BASIS",
            "nonempty finite independent current columns required",
        ));
    }
    for a in 0..nr {
        for b in 0..nr {
            let dot = basis[a]
                .iter()
                .zip(&basis[b])
                .map(|(a, b)| a * b)
                .sum::<f64>();
            if (dot - if a == b { 1. } else { 0. }).abs() > 1e-10 {
                return Err(error(
                    "REDUCTION_BASIS",
                    "orthonormal columns required; no hidden normalization or rank truncation",
                ));
            }
        }
    }
    let budget = Budget {
        max_memory_mi_b: Some(max_bytes as f64 / 1048576.),
        max_seconds: Some(max_wall.as_secs_f64()),
    };
    System::compile(input, ports, Some(&budget))?;
    if start.elapsed() > max_wall {
        return Err(error(
            "DEADLINE",
            "projection deadline after full validation",
        ));
    }
    let mut model = input.clone();
    model.current_dofs = (0..nr).map(|i| format!("projected-current-{i}")).collect();
    let mut a = BTreeMap::new();
    for &(i, j, v) in &input.incidence {
        for k in 0..nr {
            *a.entry((i, k)).or_insert(0.) += v * basis[k][j];
        }
    }
    model.incidence = a
        .into_iter()
        .filter(|(_, v)| *v != 0.)
        .map(|((i, j), v)| (i, j, v))
        .collect();
    model.resistance_ohm = congruent(&input.resistance_ohm, &basis, start, max_wall)?;
    model.inductance_h = congruent(&input.inductance_h, &basis, start, max_wall)?;
    System::compile(&model, ports, Some(&budget))?;
    if start.elapsed() > max_wall {
        return Err(error(
            "DEADLINE",
            "projection deadline after reduced validation",
        ));
    }
    Ok(Projection {
        model,
        original_current_dofs: input.current_dofs.clone(),
        basis,
        planned_bytes: plan,
        elapsed_seconds: start.elapsed().as_secs_f64(),
    })
}
impl Projection {
    pub fn recover_currents(&self, coordinates: &[f64]) -> Result<Vec<f64>> {
        if coordinates.len() != self.basis.len() || coordinates.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "REDUCTION_RECOVERY",
                "complete finite reduced current vector required",
            ));
        }
        let out: Vec<_> = (0..self.original_current_dofs.len())
            .map(|i| {
                self.basis
                    .iter()
                    .zip(coordinates)
                    .map(|(q, c)| q[i] * c)
                    .sum::<f64>()
            })
            .collect();
        if out.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "REDUCTION_RANGE",
                "physical current recovery overflow",
            ));
        }
        Ok(out)
    }
    /// Project only representable initial currents. No hidden removal of
    /// magnetic energy, loop currents or physical initial conditions.
    pub fn initial_currents(&self, physical: &[f64]) -> Result<Vec<f64>> {
        if physical.len() != self.original_current_dofs.len()
            || physical.iter().any(|v| !v.is_finite())
        {
            return Err(error(
                "REDUCTION_INITIAL",
                "complete finite physical current state required",
            ));
        }
        let q: Vec<_> = self
            .basis
            .iter()
            .map(|b| b.iter().zip(physical).map(|(a, b)| a * b).sum::<f64>())
            .collect();
        let back = self.recover_currents(&q)?;
        let scale = physical.iter().map(|v| v.abs()).fold(0., f64::max);
        if physical
            .iter()
            .zip(back)
            .any(|(a, b)| (a - b).abs() > 1e-10 * scale)
        {
            return Err(error(
                "REDUCTION_INITIAL",
                "initial current contains an unrepresented physical mode",
            ));
        }
        Ok(q)
    }
}

/// Automatic current-space moment matching retains every potential coordinate.
/// Starting with R^-1 A^T preserves all static conduction directions, including
/// directions outside the selected port drives. Subsequent blocks R^-1 L Q
/// represent the low-frequency current moments. Congruence preserves the energy
/// forms; agreement with the full model is separately checked on `training_hz`.
/// Independent holdout/continuum validation remains the caller's responsibility.
pub struct AutomaticOptions {
    pub training_hz: Vec<f64>,
    pub relative_tolerance: f64,
    pub max_current_order: usize,
    pub max_bytes: usize,
    pub max_wall: Duration,
}

#[derive(serde::Serialize)]
pub struct AutomaticAudit {
    pub current_orders: Vec<usize>,
    pub port_errors: Vec<f64>,
    pub state_errors: Vec<f64>,
    pub loss_errors: Vec<f64>,
    pub training_hz: Vec<f64>,
    pub elapsed_seconds: f64,
}
pub struct AutomaticProjection {
    pub projection: Projection,
    pub audit: AutomaticAudit,
}

fn remaining(start: Instant, wall: Duration) -> Result<Duration> {
    wall.checked_sub(start.elapsed())
        .filter(|v| !v.is_zero())
        .ok_or_else(|| error("DEADLINE", "automatic current reduction deadline"))
}

/// Coefficient-space rank test, not a physical energy cutoff. Scale before two
/// orthogonalization passes so dimensional current moments cannot under/overflow
/// solely through their common magnitude. Deflated columns are never substituted
/// into physical R/L/C. Every accepted model must still pass its response gate.
fn append_column(q: &mut Vec<Vec<f64>>, mut v: Vec<f64>) -> Result<bool> {
    if v.iter().any(|x| !x.is_finite()) {
        return Err(error("REDUCTION_RANGE", "nonfinite Krylov column"));
    }
    let scale = v.iter().map(|x| x.abs()).fold(0., f64::max);
    if scale == 0. {
        return Ok(false);
    }
    for x in &mut v {
        *x /= scale;
    }
    let original = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    for _ in 0..2 {
        for old in q.iter() {
            let dot = old.iter().zip(&v).map(|(a, b)| a * b).sum::<f64>();
            for (x, a) in v.iter_mut().zip(old) {
                *x -= dot * a;
            }
        }
    }
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm <= 64. * f64::EPSILON * (v.len() + 1) as f64 * original {
        return Ok(false);
    }
    for x in &mut v {
        *x /= norm;
    }
    q.push(v);
    Ok(true)
}

fn response(
    system: &mut System,
    nv: usize,
    ni: usize,
    ports: usize,
    f: f64,
    p: usize,
) -> Result<Vec<f64>> {
    let mut drive = vec![0.; ports];
    drive[p] = 1.;
    let mut rhs = system.port_rhs(&drive);
    if f > 0. {
        rhs.resize(2 * (nv + ni), 0.);
    }
    system.solve(
        2. * std::f64::consts::PI * f,
        rhs,
        &vec![0.; nv + ni],
        &mut crate::electrical::Counters::default(),
    )
}

fn local_losses(input: &Model, state: &[f64], nv: usize, ni: usize, complex: bool) -> Vec<f64> {
    // Diagonal R only: each coordinate's own physical volume loss is observable.
    let mut losses = vec![0.; ni];
    for &(i, _, r) in &input.resistance_ohm {
        losses[i] = r
            * (state[nv + i].powi(2)
                + if complex {
                    state[nv + ni + nv + i].powi(2)
                } else {
                    0.
                });
    }
    losses
}

fn normalized_error(actual: &[f64], reference: &[f64]) -> f64 {
    if actual.iter().chain(reference).any(|x| !x.is_finite()) {
        return f64::INFINITY;
    }
    let scale = reference.iter().map(|x| x.abs()).fold(0., f64::max);
    actual
        .iter()
        .zip(reference)
        .map(|(a, b)| {
            let denominator = b.abs().max(1e-12 * scale);
            if denominator == 0. {
                if a == b {
                    0.
                } else {
                    f64::INFINITY
                }
            } else {
                (a - b).abs() / denominator
            }
        })
        .fold(0., f64::max)
}

fn complex_error(actual: &[f64], reference: &[f64], size: usize, complex: bool) -> f64 {
    if actual.iter().chain(reference).any(|x| !x.is_finite()) {
        return f64::INFINITY;
    }
    let scale = (0..size)
        .map(|i| reference[i].hypot(if complex { reference[size + i] } else { 0. }))
        .fold(0., f64::max);
    if !scale.is_finite() {
        return f64::INFINITY;
    }
    (0..size)
        .map(|i| {
            let delta = (actual[i] - reference[i]).hypot(if complex {
                actual[size + i] - reference[size + i]
            } else {
                0.
            });
            let denominator = reference[i]
                .hypot(if complex { reference[size + i] } else { 0. })
                .max(1e-12 * scale);
            if !delta.is_finite() || !denominator.is_finite() {
                f64::INFINITY
            } else if denominator == 0. {
                if delta == 0. {
                    0.
                } else {
                    f64::INFINITY
                }
            } else {
                delta / denominator
            }
        })
        .fold(0., f64::max)
}

/// Bounded automatic path. Deliberately validates the complete input with P6;
/// it cannot be used to bypass full-model passivity, gauge or memory admission.
/// It therefore does not solve the separate real-ROI pre-reduction requirement.
pub fn automatic_currents(
    input: &Model,
    ports: usize,
    options: AutomaticOptions,
) -> Result<AutomaticProjection> {
    let start = Instant::now();
    remaining(start, options.max_wall)?;
    let nv = input.potential_nodes.len();
    let ni = input.current_dofs.len();
    if ni == 0
        || options.max_current_order == 0
        || options.max_current_order > ni
        || options.training_hz.is_empty()
        || options.training_hz.len() > 64
        || options
            .training_hz
            .iter()
            .any(|f| !f.is_finite() || *f < 0.)
        || options.training_hz.windows(2).any(|w| w[0] >= w[1])
        || !options.relative_tolerance.is_finite()
        || options.relative_tolerance <= 0.
        || options.relative_tolerance > 0.01
    {
        return Err(error(
            "REDUCTION_INPUT",
            "ordered finite training axis, tolerance0..1%, and bounded current order required",
        ));
    }
    // Four dense envelopes conservatively cover full and reduced compilation,
    // saved full complex responses, Q, its projected clone and iteration scratch.
    let plan = nv
        .checked_add(ni)
        .and_then(|n| n.checked_pow(2))
        .and_then(|n| n.checked_mul(6400))
        .and_then(|n| {
            n.checked_add(
                (nv + ni)
                    .checked_mul(ports)?
                    .checked_mul(options.training_hz.len())?
                    .checked_mul(32)?,
            )
        })
        .ok_or_else(|| error("MEMORY_LIMIT", "automatic reduction allocation overflow"))?;
    if plan > options.max_bytes {
        return Err(error(
            "MEMORY_LIMIT",
            "automatic full/reduced/responses admission",
        ));
    }
    let mut full = System::compile(
        input,
        ports,
        Some(&Budget {
            max_memory_mi_b: Some(options.max_bytes as f64 / 1048576.),
            max_seconds: Some(remaining(start, options.max_wall)?.as_secs_f64()),
        }),
    )?;
    let mut r = vec![0.; ni];
    for &(i, j, v) in &input.resistance_ohm {
        if i != j || v <= 0. {
            return Err(error(
                "REDUCTION_RESISTANCE",
                "automatic current moments require positive diagonal physical R",
            ));
        }
        r[i] = v;
    }
    if r.iter().any(|v| *v <= 0.) {
        return Err(error(
            "REDUCTION_RESISTANCE",
            "missing positive diagonal R coordinate",
        ));
    }
    let mut reference = Vec::new();
    for &f in &options.training_hz {
        for p in 0..ports {
            remaining(start, options.max_wall)?;
            reference.push(response(&mut full, nv, ni, ports, f, p)?);
        }
    }
    let mut q = Vec::new();
    for v in 0..nv {
        remaining(start, options.max_wall)?;
        let mut seed = vec![0.; ni];
        for &(i, j, a) in &input.incidence {
            if i == v {
                seed[j] = a / r[j];
            }
        }
        append_column(&mut q, seed)?;
        if q.len() > options.max_current_order {
            return Err(error(
                "REDUCTION_ORDER",
                "order cap cannot retain all static conduction directions",
            ));
        }
    }
    if q.is_empty() {
        return Err(error("REDUCTION_BASIS", "no static conduction directions"));
    }
    let mut audit = AutomaticAudit {
        current_orders: vec![],
        port_errors: vec![],
        state_errors: vec![],
        loss_errors: vec![],
        training_hz: options.training_hz.clone(),
        elapsed_seconds: 0.,
    };
    let mut first = 0;
    loop {
        remaining(start, options.max_wall)?;
        // If the Krylov space has exactly as many nonzero physical coordinate
        // rows as columns, independence proves it is that complete coordinate
        // subspace. Use its exact coordinate basis to avoid unnecessary rotated
        // weak-response cancellation. This uses exact zero support, never a
        // physical coefficient/energy threshold or additional truncation.
        let active: Vec<_> = (0..ni).filter(|&i| q.iter().any(|v| v[i] != 0.)).collect();
        if active.len() == q.len() {
            q = active
                .into_iter()
                .map(|i| {
                    let mut v = vec![0.; ni];
                    v[i] = 1.;
                    v
                })
                .collect();
            first = 0;
        }
        let mut projection = project_currents(
            input,
            ports,
            q.clone(),
            options.max_bytes,
            remaining(start, options.max_wall)?,
        )?;
        let nr = q.len();
        let mut reduced = System::compile(
            &projection.model,
            ports,
            Some(&Budget {
                max_memory_mi_b: Some(options.max_bytes as f64 / 1048576.),
                max_seconds: Some(remaining(start, options.max_wall)?.as_secs_f64()),
            }),
        )?;
        let (mut port_error, mut state_error, mut loss_error): (f64, f64, f64) = (0., 0., 0.);
        for (k, &f) in options.training_hz.iter().enumerate() {
            for p in 0..ports {
                remaining(start, options.max_wall)?;
                let x = response(&mut reduced, nv, nr, ports, f, p)?;
                let mut back = x[..nv].to_vec();
                back.extend(projection.recover_currents(&x[nv..nv + nr])?);
                if f > 0. {
                    back.extend_from_slice(&x[nv + nr..nv + nr + nv]);
                    back.extend(projection.recover_currents(&x[nv + nr + nv..])?);
                }
                let original = &reference[k * ports + p];
                let mut actual_ports = full.port_voltages(&back, ports);
                let mut reference_ports = full.port_voltages(original, ports);
                if f > 0. {
                    actual_ports.extend(full.port_voltages(&back[nv + ni..], ports));
                    reference_ports.extend(full.port_voltages(&original[nv + ni..], ports));
                }
                port_error = port_error.max(complex_error(
                    &actual_ports,
                    &reference_ports,
                    ports,
                    f > 0.,
                ));
                // Complex magnitude avoids ill-defined relative error in a
                // real/imaginary component that is zero by symmetry or at DC.
                for (offset, count) in [(0, nv), (nv, ni)] {
                    let n = nv + ni;
                    let scale = (offset..offset + count)
                        .map(|i| original[i].hypot(if f > 0. { original[n + i] } else { 0. }))
                        .fold(0., f64::max);
                    for i in offset..offset + count {
                        let delta = (back[i] - original[i]).hypot(if f > 0. {
                            back[n + i] - original[n + i]
                        } else {
                            0.
                        });
                        let denominator = original[i]
                            .hypot(if f > 0. { original[n + i] } else { 0. })
                            .max(1e-12 * scale);
                        state_error =
                            state_error.max(if !delta.is_finite() || !denominator.is_finite() {
                                f64::INFINITY
                            } else if denominator == 0. {
                                if delta == 0. {
                                    0.
                                } else {
                                    f64::INFINITY
                                }
                            } else {
                                delta / denominator
                            });
                    }
                }
                loss_error = loss_error.max(normalized_error(
                    &local_losses(input, &back, nv, ni, f > 0.),
                    &local_losses(input, original, nv, ni, f > 0.),
                ));
            }
        }
        audit.current_orders.push(nr);
        audit.port_errors.push(port_error);
        audit.state_errors.push(state_error);
        audit.loss_errors.push(loss_error);
        if port_error <= options.relative_tolerance
            && state_error <= options.relative_tolerance
            && loss_error <= options.relative_tolerance
        {
            audit.elapsed_seconds = start.elapsed().as_secs_f64();
            remaining(start, options.max_wall)?;
            projection.planned_bytes = plan;
            return Ok(AutomaticProjection { projection, audit });
        }
        let end = q.len();
        for k in first..end {
            remaining(start, options.max_wall)?;
            let mut next = vec![0.; ni];
            for &(i, j, l) in &input.inductance_h {
                next[i] += l * q[k][j] / r[i];
            }
            if q.len() == options.max_current_order {
                let mut probe = q.clone();
                if append_column(&mut probe, next)? {
                    return Err(error(
                        "REDUCTION_ORDER",
                        "training state/local-loss error exceeds gate at order cap",
                    ));
                }
            } else {
                append_column(&mut q, next)?;
            }
        }
        if q.len() == end {
            return Err(error("REDUCTION_ACCURACY", &format!("Krylov space exhausted before state/local-loss gate: orders={:?}, state={:?}, loss={:?}",audit.current_orders,audit.state_errors,audit.loss_errors)));
        }
        first = end;
    }
}

#[cfg(test)]
mod automatic_range_tests {
    use super::*;
    #[test]
    fn nonfinite_and_overflowing_observables_cannot_pass_error_folds() {
        assert!(normalized_error(&[f64::INFINITY], &[f64::INFINITY]).is_infinite());
        assert!(normalized_error(&[f64::NAN], &[1.]).is_infinite());
        assert!(complex_error(&[f64::MAX, f64::MAX], &[f64::MAX, f64::MAX], 1, true).is_infinite());
        assert!(complex_error(&[f64::MAX], &[-f64::MAX], 1, false).is_infinite());
        assert_eq!(complex_error(&[0., 0.], &[0., 0.], 1, true), 0.);
    }
}
