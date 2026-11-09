//! Discrete spatial current and Joule-loss recovery, not physical acceptance.
use super::{error, mixed_conductor::{MixedMap, Owner}, AllocationBudget, Result};
use std::time::{Duration, Instant};
fn density(entries:&[(usize,[f64;3])], re:&[f64], im:&[f64]) -> Result<([f64;3],[f64;3])> {
    let mut a=[0.;3]; let mut b=[0.;3];
    for &(i,v) in entries {
        let (&r,&j)=re.get(i).zip(im.get(i)).ok_or_else(||error("SPATIAL_RECOVERY","current coordinate missing"))?;
        for axis in 0..3 { a[axis]+=v[axis]*r; b[axis]+=v[axis]*j; }
    }
    if a.iter().chain(&b).any(|v|!v.is_finite()) {return Err(error("SPATIAL_RECOVERY","nonfinite current density"));}
    Ok((a,b))
}
fn power(a:[f64;3],b:[f64;3],volume:f64,sigma:f64)->Result<f64> {
    let p=a.iter().chain(&b).map(|v|v*v).sum::<f64>()*volume/sigma;
    if !p.is_finite() || p<0. {return Err(error("SPATIAL_RECOVERY","nonfinite Joule loss"));} Ok(p)
}
/// Currents use the 1 A RMS matched-Norton response convention.
pub(super) fn recover(map:&MixedMap<'_>, re:&[f64], im:&[f64], expected_loss:f64, budget:AllocationBudget, start:Instant, wall:Duration)->Result<serde_json::Value> {
    if re.len()!=map.current_count() || im.len()!=map.current_count() || re.iter().chain(im).any(|v|!v.is_finite()) || !expected_loss.is_finite() || expected_loss<0. {return Err(error("SPATIAL_RECOVERY","invalid mixed current or expected loss"));}
    let n=map.foils.len().checked_add(map.barrels.len()).and_then(|v|v.checked_add(map.owners.iter().filter(|o|matches!(o,Owner::Volume)).count())).ok_or_else(||error("MEMORY_LIMIT","spatial count overflow"))?;
    let bytes=n.checked_mul(6144).and_then(|v|v.checked_add(map.current_count().checked_mul(32)?)).ok_or_else(||error("MEMORY_LIMIT","spatial output overflow"))?; budget.check(bytes)?;
    let planar=map.planar_current_supports()?; let local=map.local_current_supports()?;
    if planar.len()!=map.foils.len() || local.len()!=map.owners.iter().filter(|o|matches!(o,Owner::Volume)).count() {return Err(error("SPATIAL_RECOVERY","support ownership count mismatch"));}
    let mut supports=Vec::with_capacity(n); let mut loss=0.;
    for (foil,support) in map.foils.iter().zip(&planar) {
        if start.elapsed()>=wall {return Err(error("DEADLINE","spatial recovery"));}
        let (a,b)=density(&support.current_density,re,im)?;
        let Some(&first)=foil.tetrahedra.first() else {return Err(error("SPATIAL_RECOVERY","empty foil support"));};
        let sigma=map.mesh().conductivity_s_m[first];
        if !sigma.is_finite() || sigma<=0. || foil.tetrahedra.iter().any(|&i|map.mesh().conductivity_s_m[i]!=sigma) {return Err(error("SPATIAL_RECOVERY","nonuniform foil conductivity"));}
        let p=power(a,b,foil.volume_m3,sigma)?; loss+=p;
        supports.push(serde_json::json!({"kind":"foil","layer":foil.layer,"triangle":foil.planar_triangle,"zM":foil.z_span_m,"xyM":support.prism.triangle_m,"volumeM3":foil.volume_m3,"currentDensityRealAPerM2":a,"currentDensityImaginaryAPerM2":b,"lossW":p}));
    }
    let barrel=map.barrel_current_supports()?;
    for (prism,support) in map.barrels.iter().zip(&barrel) {
        if start.elapsed()>=wall {return Err(error("DEADLINE","spatial recovery"));}
        let (a,b)=density(&support.current_density,re,im)?;
        let sigma=map.mesh().conductivity_s_m[prism.tetrahedra[0]];
        let p=power(a,b,prism.volume_m3,sigma)?; loss+=p;
        supports.push(serde_json::json!({"kind":"barrel","barrel":prism.barrel_id,"triangle":prism.planar_triangle,"zM":prism.z_span_m,"xyM":support.prism.triangle_m,"volumeM3":prism.volume_m3,"currentDensityRealAPerM2":a,"currentDensityImaginaryAPerM2":b,"lossW":p}));
    }
    let mut cursor=0;
    for (i,owner) in map.owners.iter().enumerate() {
        if !matches!(owner,Owner::Volume) {continue;}
        if start.elapsed()>=wall {return Err(error("DEADLINE","spatial recovery"));}
        let support=&local[cursor];cursor+=1;
        let (a,b)=density(&support.densities,re,im)?;
        let volume=map.mesh().mesh.copper_tetrahedra[i].volume_m3;
        let p=power(a,b,volume,map.mesh().conductivity_s_m[i])?;loss+=p;
        supports.push(serde_json::json!({"kind":"tetrahedron","verticesM":support.tetra,"volumeM3":volume,"currentDensityRealAPerM2":a,"currentDensityImaginaryAPerM2":b,"lossW":p}));
    }
    if !loss.is_finite() || (loss-expected_loss).abs()>1e-18+1e-8*(loss.abs()+expected_loss.abs()) {return Err(error("SPATIAL_RECOVERY","recovered copper power disagrees with model"));}
    Ok(serde_json::json!({"status":"recovered-discrete","physicalAccuracy":"unverified","driveConvention":"1 A RMS matched Norton","copperLossW":loss,"supports":supports}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn complex_modes_interfere_before_power() {
        let entries=[(0,[1.,0.,0.]),(1,[1.,0.,0.])];
        let (a,b)=density(&entries,&[2.,-1.],&[3.,-3.]).unwrap();
        assert_eq!(a,[1.,0.,0.]); assert_eq!(b,[0.,0.,0.]); assert_eq!(power(a,b,2.,4.).unwrap(),0.5);
        let (a,b)=density(&entries,&[2.,1.],&[3.,-1.]).unwrap();
        assert_eq!(power(a,b,2.,4.).unwrap(),6.5);
        assert_ne!(power(a,b,2.,4.).unwrap(),(4.+1.+9.+1.)*2./4.);
    }
}
