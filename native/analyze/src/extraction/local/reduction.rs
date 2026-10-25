//! Block congruence in the physical current space. Potential coordinates,
//! charge, finite contacts and complete C remain unchanged. This is a bounded
//! projection primitive, not automatic PRIMA basis selection or sampled fitting.
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
