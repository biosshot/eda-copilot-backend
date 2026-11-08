//! Compact prism/tetra interaction blocks. Experimental; no runtime admission.
use std::time::{Duration, Instant};
use super::{error, Result, AllocationBudget, hybrid_section_magnetic, polyhedron::PointPotential, prism::{GaussRule, Prism}};

/// Constant physical densities per coefficient: charge m^-3, current m^-2.
pub(super) struct Densities<'a> { pub charge: &'a [f64], pub current: &'a [[f64; 3]] }
pub(super) struct CrossBlock { pub potential_per_farad: Vec<Vec<f64>>, pub inductance_h: Vec<Vec<f64>>, pub relative_order_change: f64 }

/// Computes one unordered support pair; assembly owns its transpose and must
/// prove the prism and tetra have disjoint interiors. Contact is permitted.
/// The estimator is not an independent physical-error certificate.
pub(super) fn cross(prism: &Prism, tetra: [[f64;3];4], planar: Densities<'_>, local: Densities<'_>, epsilon: f64, order: usize, budget: AllocationBudget, limit: Duration) -> Result<CrossBlock> {
    let start = Instant::now();
    if start.elapsed() >= limit { return Err(error("DEADLINE", "mixed pair deadline exceeded")); }
    if !(8..=60).contains(&order) || !epsilon.is_finite() || epsilon <= 0. || [planar.charge.len(),local.charge.len(),planar.current.len(),local.current.len()].iter().any(|&n| n > budget.max_nodes) || !planar.charge.iter().chain(local.charge).chain(planar.current.iter().flatten()).chain(local.current.iter().flatten()).all(|v| v.is_finite()) {
        return Err(error("PLANAR_MIXED_INPUT", "invalid densities, dimensions, permittivity or order"));
    }
    let bytes = planar.charge.len().checked_mul(local.charge.len()).and_then(|n| n.checked_add(planar.current.len().checked_mul(local.current.len())?)).and_then(|n| n.checked_mul(8)).and_then(|n| n.checked_add(512*1024)).ok_or_else(|| error("MEMORY_LIMIT", "mixed block size overflow"))?;
    budget.check(bytes)?;
    let checked = Prism::new(prism.source_id.clone(), prism.triangle_m, prism.z_m)?;
    if checked.volume_m3 != prism.volume_m3 { return Err(error("PLANAR_MIXED_INPUT", "invalid prism volume")); }
    if (planar.charge.is_empty() || local.charge.is_empty()) && (planar.current.is_empty() || local.current.is_empty()) {
        return Ok(CrossBlock { potential_per_farad: vec![vec![0.;local.charge.len()];planar.charge.len()], inductance_h: vec![vec![0.;local.current.len()];planar.current.len()], relative_order_change: 0. });
    }
    let source = PointPotential::tetrahedron(tetra)?.with_tighter_tetra_tolerance(1e-12)?;
    let coarse = hybrid_section_magnetic::cross_integral(prism, &source, &GaussRule::new(order)?, start, limit)?;
    let fine = hybrid_section_magnetic::cross_integral(prism, &source, &GaussRule::new(order+4)?, start, limit)?;
    let relative = (fine-coarse).abs()/fine.abs();
    if !relative.is_finite() || relative > 1e-8 { return Err(error("PLANAR_MIXED_UNRESOLVED", "mixed quadrature order estimate exceeds tolerance")); }
    let p = planar.charge.iter().map(|a| local.charge.iter().map(|b| fine*a*b/(4.*std::f64::consts::PI*epsilon)).collect()).collect::<Vec<Vec<f64>>>();
    let l = planar.current.iter().map(|a| local.current.iter().map(|b| 1e-7*fine*(0..3).map(|k|a[k]*b[k]).sum::<f64>()).collect()).collect::<Vec<Vec<f64>>>();
    if !p.iter().flatten().chain(l.iter().flatten()).all(|v|v.is_finite()) { return Err(error("PLANAR_MIXED_RANGE", "nonfinite mixed block")); }
    if start.elapsed() >= limit { return Err(error("DEADLINE", "mixed pair deadline exceeded")); }
    Ok(CrossBlock { potential_per_farad: p, inductance_h: l, relative_order_change: relative })
}

#[cfg(test)]
mod tests {
 use super::*;
 fn budget() -> AllocationBudget { AllocationBudget { max_bytes: 4*1024*1024, max_nodes: 8, max_tetrahedra: 8 } }
 fn prism() -> Prism { Prism::new("p".into(), [[0.,0.],[1.,0.],[0.,1.]], [0.,1.]).unwrap() }
 fn tetra() -> [[f64;3];4] { [[4.,0.,0.],[5.,0.,0.],[4.,1.,0.],[4.,0.,1.]] }
 #[test] fn invalid_and_deadline_refuse() {
  let q=[1.]; let j=[[1.,0.,0.]];
  let d=|| Densities{charge:&q,current:&j};
  assert!(cross(&prism(),tetra(),d(),d(),0.,8,budget(),Duration::from_secs(1)).is_err());
  assert!(cross(&prism(),tetra(),d(),d(),1.,8,budget(),Duration::ZERO).is_err());
  assert!(cross(&prism(),[[0.;3];4],d(),d(),1.,8,budget(),Duration::from_secs(1)).is_err());
 }
 #[test] fn separated_block_scales_and_respects_orientation() {
  let a=[1.,2.]; let b=[3.]; let ja=[[1.,0.,0.]]; let jb=[[0.,1.,0.],[2.,0.,0.]];
  let block=cross(&prism(),tetra(),Densities{charge:&a,current:&ja},Densities{charge:&b,current:&jb},1.,12,budget(),Duration::from_secs(30)).unwrap();
  assert!(block.potential_per_farad[0][0]>0.);
  assert!((block.potential_per_farad[1][0]/block.potential_per_farad[0][0]-2.).abs()<1e-12);
  assert_eq!(block.inductance_h[0][0],0.);
  assert!(block.inductance_h[0][1]>0.);
 }
}
