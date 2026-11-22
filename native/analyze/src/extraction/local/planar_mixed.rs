//! Compact prism/tetra interaction blocks. Experimental; no runtime admission.
use std::time::{Duration, Instant};
use super::{error, Result, AllocationBudget, hybrid_section_magnetic, polyhedron::PointPotential, prism::{GaussRule, Prism}};

/// Constant physical densities per coefficient: charge m^-3, current m^-2.
pub(super) struct Densities<'a> { pub charge: &'a [f64], pub current: &'a [[f64; 3]] }
pub(super) struct CrossBlock { pub potential_per_farad: Vec<Vec<f64>>, pub inductance_h: Vec<Vec<f64>>, pub relative_order_change: Option<f64>, pub used_tetra_fallback: bool }

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
        return Ok(CrossBlock { potential_per_farad: vec![vec![0.;local.charge.len()];planar.charge.len()], inductance_h: vec![vec![0.;local.current.len()];planar.current.len()], relative_order_change: None, used_tetra_fallback: false });
    }
    // Integrate over the smaller local tetrahedron against the analytic whole
    // prism potential when geometry makes it the more compact observer.
    let [a,b,c,d]=tetra;
    let u=std::array::from_fn::<_,3,_>(|k|b[k]-a[k]);let v=std::array::from_fn::<_,3,_>(|k|c[k]-a[k]);let w=std::array::from_fn::<_,3,_>(|k|d[k]-a[k]);
    let volume=(u[0]*(v[1]*w[2]-v[2]*w[1])-u[1]*(v[0]*w[2]-v[2]*w[0])+u[2]*(v[0]*w[1]-v[1]*w[0])).abs()/6.;
    let reversed=if volume.is_finite() && volume>0. && volume<prism.volume_m3 {
        let source=PointPotential::new(prism)?;
        super::polyhedron::tetra_integral_adaptive(tetra,&source,16,1e-10).ok()
    } else {None};
    let (fine,relative_order_change,used_tetra_fallback)=if let Some(value)=reversed {
        if !value.is_finite() || value<=0. {return Err(error("PLANAR_MIXED_RANGE","invalid reversed adaptive integral"));}
        (value,None,true)
    } else {
    let source = PointPotential::tetrahedron(tetra)?.with_tighter_tetra_tolerance(1e-12)?;
    let coarse = hybrid_section_magnetic::cross_integral(prism, &source, &GaussRule::new(order)?, start, limit)?;
    let direct = hybrid_section_magnetic::cross_integral(prism, &source, &GaussRule::new(order+4)?, start, limit)?;
    let relative = (direct-coarse).abs()/direct.abs();
    let used_tetra_fallback = !relative.is_finite() || relative > 1e-8;
    let fine = if used_tetra_fallback {
        // Partition the physical prism into three disjoint tetrahedra. The
        // existing local pair kernel keeps its 1e-10 convergence gate and
        // bounded workspace; no additional geometry is introduced.
        let [a,b,c]=prism.triangle_m; let [lo,hi]=prism.z_m;
        let a=[a[0],a[1],lo];let b=[b[0],b[1],lo];let c=[c[0],c[1],lo];
        let aa=[a[0],a[1],hi];let bb=[b[0],b[1],hi];let cc=[c[0],c[1],hi];
        let parts=vec![[a,b,c,cc],[a,b,bb,cc],[a,aa,bb,cc],tetra];
        let kernel=super::volume_integral_magnetic::CompactLocalPairs::new(parts,budget,start,limit)?;
        let mut total=0.;for i in 0..3 {total+=kernel.pair(i,3).map_err(|failure|format!("{failure}; prism tetra-part={i} triangle={:?} z={:?} local={tetra:?}",prism.triangle_m,prism.z_m))?;}
        if !total.is_finite() || total<=0. {return Err(error("PLANAR_MIXED_UNRESOLVED","tetra partition produced invalid integral"));} total
    } else {direct};
    (fine,if used_tetra_fallback {None} else {Some(relative)},used_tetra_fallback)
    };
    let p = planar.charge.iter().map(|a| local.charge.iter().map(|b| fine*a*b/(4.*std::f64::consts::PI*epsilon)).collect()).collect::<Vec<Vec<f64>>>();
    let l = planar.current.iter().map(|a| local.current.iter().map(|b| 1e-7*fine*(0..3).map(|k|a[k]*b[k]).sum::<f64>()).collect()).collect::<Vec<Vec<f64>>>();
    if !p.iter().flatten().chain(l.iter().flatten()).all(|v|v.is_finite()) { return Err(error("PLANAR_MIXED_RANGE", "nonfinite mixed block")); }
    if start.elapsed() >= limit { return Err(error("DEADLINE", "mixed pair deadline exceeded")); }
    Ok(CrossBlock { potential_per_farad: p, inductance_h: l, relative_order_change, used_tetra_fallback })
}

#[cfg(test)]
mod tests {
 use super::*;
 fn budget() -> AllocationBudget { AllocationBudget { max_bytes: 4*1024*1024, max_nodes: 8, max_tetrahedra: 8 } }
 fn prism() -> Prism { Prism::new("p".into(), [[0.,0.],[1.,0.],[0.,1.]], [0.,1.]).unwrap() }
 fn tetra() -> [[f64;3];4] { [[4.,0.,0.],[5.,0.,0.],[4.,1.,0.],[4.,0.,1.]] }
 #[test] fn reversed_small_tetra_matches_direct_observer() {
  let p=Prism::new("large".into(),[[0.,0.],[0.01,0.],[0.,0.01]],[0.,0.0001]).unwrap();
  let t=[[0.003,0.003,0.0005],[0.0031,0.003,0.0005],[0.003,0.0031,0.0005],[0.003,0.003,0.0006]];
  let q=[1.];let j=[[1.,0.,0.]];
  let result=cross(&p,t,Densities{charge:&q,current:&j},Densities{charge:&q,current:&j},1./(4.*std::f64::consts::PI),8,budget(),Duration::from_secs(20)).unwrap();
  assert!(result.used_tetra_fallback);assert!(result.relative_order_change.is_none());
  let [a,b,c]=p.triangle_m;let [lo,hi]=p.z_m;
  let a=[a[0],a[1],lo];let b=[b[0],b[1],lo];let c=[c[0],c[1],lo];
  let aa=[a[0],a[1],hi];let bb=[b[0],b[1],hi];let cc=[c[0],c[1],hi];
  let parts=[[a,b,c,cc],[a,b,bb,cc],[a,aa,bb,cc]];
  let mut reference=0.;for part in parts {let source=PointPotential::tetrahedron(part).unwrap().with_tighter_tetra_tolerance(1e-12).unwrap();reference+=super::super::polyhedron::tetra_integral_adaptive(t,&source,16,1e-10).unwrap();}
  eprintln!("reversed whole-prism={} tetra-partition={} relative={:e}",result.potential_per_farad[0][0],reference,(result.potential_per_farad[0][0]-reference).abs()/reference);
  assert!((result.potential_per_farad[0][0]-reference).abs() <= 1e-7*reference);
 }
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
