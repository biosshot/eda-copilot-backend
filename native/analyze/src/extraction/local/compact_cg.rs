//! Bounded SPD charge solve. Algebraic convergence does not verify field accuracy.
use super::{accurate::Sum, error, AllocationBudget, Result};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct Solution {
    pub coefficients: Vec<f64>,
    pub iterations: usize,
    pub relative_residual: f64,
}
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).fold(Sum::default(), |s, (&x, &y)| s.add(Sum::from(x).mul(y))).value()
}
/// `diagonal` is the positive diagonal of the fixed symmetric operator.
/// The caller budgets its resident operator and propagates the same deadline
/// into `apply`, which must overwrite every output coordinate.
pub(super) fn solve(
    rhs: &[f64], diagonal: &[f64], tolerance: f64, max_iterations: usize,
    budget: AllocationBudget, start: Instant, wall: Duration,
    mut apply: impl FnMut(&[f64], &mut [f64]) -> Result<()>,
) -> Result<Solution> {
    let n = rhs.len();
    if n == 0 || n > budget.max_nodes || diagonal.len() != n ||
        rhs.iter().any(|v| !v.is_finite()) || diagonal.iter().any(|v| !v.is_finite() || *v <= 0.) ||
        !tolerance.is_finite() || tolerance <= 0. || tolerance >= 1. || max_iterations == 0 {
        return Err(error("COMPACT_CG_INPUT", "finite RHS, positive diagonal and explicit solver limits required"));
    }
    budget.check(n.checked_mul(5 * 8).ok_or_else(|| error("MEMORY_LIMIT", "charge workspace overflow"))?)?;
    let clock = || if start.elapsed() >= wall { Err(error("DEADLINE", "compact charge solve")) } else { Ok(()) };
    clock()?;
    let scale = rhs.iter().fold(0_f64, |a, v| a.max(v.abs()));
    let mut x = vec![0.; n];
    if scale == 0. { return Ok(Solution { coefficients: x, iterations: 0, relative_residual: 0. }); }
    // Normalize RHS to keep products in range; restore physical units once.
    let mut r: Vec<_> = rhs.iter().map(|v| v / scale).collect();
    let rhs_norm = dot(&r, &r).sqrt();
    let mut z: Vec<_> = r.iter().zip(diagonal).map(|(r,d)| r/d).collect();
    let mut p = z.clone();
    let mut ap = vec![0.; n];
    let mut rz = dot(&r, &z);
    if !rz.is_finite() || rz <= 0. { return Err(error("COMPACT_CG_RANGE", "preconditioned charge norm is invalid")); }
    for iteration in 1..=max_iterations {
        clock()?;
        ap.fill(f64::NAN); apply(&p, &mut ap)?; clock()?;
        if ap.iter().any(|v| !v.is_finite()) { return Err(error("COMPACT_CG_RANGE", "operator output must be complete and finite")); }
        let curvature = dot(&p, &ap);
        if !curvature.is_finite() || curvature <= 0. { return Err(error("COMPACT_CG_SPD", "charge operator has nonpositive curvature")); }
        let alpha = rz / curvature;
        for i in 0..n { x[i] += alpha*p[i]; r[i] -= alpha*ap[i]; }
        if x.iter().chain(&r).any(|v| !v.is_finite()) { return Err(error("COMPACT_CG_RANGE", "charge iteration overflow")); }
        let mut relative = dot(&r, &r).sqrt() / rhs_norm;
        let refresh = relative <= tolerance || iteration % 32 == 0;
        if refresh {
            // A recursive residual alone cannot admit the charge solution.
            ap.fill(f64::NAN); apply(&x, &mut ap)?; clock()?;
            if ap.iter().any(|v| !v.is_finite()) { return Err(error("COMPACT_CG_RANGE", "invalid charge audit action")); }
            for i in 0..n { r[i] = rhs[i]/scale - ap[i]; }
            relative = dot(&r, &r).sqrt() / rhs_norm;
            if relative <= tolerance {
                for v in &mut x { *v *= scale; }
                if x.iter().any(|v| !v.is_finite()) { return Err(error("COMPACT_CG_RANGE", "physical charge overflow")); }
                return Ok(Solution { coefficients: x, iterations: iteration, relative_residual: relative });
            }
        }
        for i in 0..n { z[i] = r[i]/diagonal[i]; }
        let next = dot(&r, &z);
        if !next.is_finite() || next <= 0. { return Err(error("COMPACT_CG_RANGE", "invalid remaining charge norm")); }
        let beta = if refresh { 0. } else { next/rz };
        for i in 0..n { p[i] = z[i] + beta*p[i]; }
        rz = next;
    }
    Err(error("COMPACT_CG_UNRESOLVED", "charge solve exhausted iteration budget"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> AllocationBudget { AllocationBudget { max_nodes: 8, max_tetrahedra: 8, max_bytes: 4096 } }
    #[test]
    fn coupled_charge_solution_and_true_residual() {
        let mut calls=0;
        let s=solve(&[1e12,2e12], &[4.,3.], 1e-12, 8, budget(), Instant::now(), Duration::from_secs(1), |x,y| {
            calls+=1; y[0]=4.*x[0]+x[1]; y[1]=x[0]+3.*x[1]; Ok(())
        }).unwrap();
        assert!((s.coefficients[0]/1e12-1./11.).abs()<1e-12);
        assert!((s.coefficients[1]/1e12-7./11.).abs()<1e-12);
        assert!(s.relative_residual<=1e-12 && calls>s.iterations);
    }
    #[test]
    fn negative_curvature_incomplete_action_and_limits_refuse() {
        let run=|f:fn(&[f64],&mut[f64])->Result<()>|solve(&[1.,1.],&[1.,1.],1e-12,1,budget(),Instant::now(),Duration::from_secs(1),f);
        assert!(run(|x,y|{y[0]=-x[0];y[1]=-x[1];Ok(())}).unwrap_err().contains("SPD"));
        assert!(run(|_,y|{y[0]=0.;Ok(())}).is_err());
        assert!(solve(&[1.],&[1.],1e-12,1,budget(),Instant::now(),Duration::ZERO,|_,_|Ok(())).is_err());
        assert!(solve(&[1.],&[1.],1e-12,1,AllocationBudget{max_bytes:1,..budget()},Instant::now(),Duration::from_secs(1),|_,_|Ok(())).is_err());
    }
}
