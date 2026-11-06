//! Experimental mixed section/event Gram. No production dispatch or KCL network claim. Densities are per ampere, not necessarily unit-current profiles.
#![allow(dead_code)]
use std::{collections::HashSet, time::{Duration, Instant}};
use super::{error, Result, AllocationBudget, axial_magnetic, polyhedron::{self, PointPotential}, prism::{GaussRule, Prism}};

#[derive(Clone)]
pub(super) enum Geometry { Section(Prism), LocalTetra([[f64; 3]; 4]) }
#[derive(Clone)]
pub(super) struct Cell { pub id: String, pub geometry: Geometry, pub currents: Vec<[f64; 3]> }

fn bounds(g: &Geometry) -> ([f64; 3], [f64; 3]) {
    let points: Vec<[f64; 3]> = match g {
        Geometry::Section(p) => p.triangle_m.iter().flat_map(|xy| p.z_m.iter().map(move |z| [xy[0], xy[1], *z])).collect(),
        Geometry::LocalTetra(t) => t.to_vec(),
    };
    let mut lo=[f64::INFINITY;3]; let mut hi=[f64::NEG_INFINITY;3];
    for p in points { for d in 0..3 {lo[d]=lo[d].min(p[d]); hi[d]=hi[d].max(p[d]);} } (lo,hi)
}
fn duplicate(a: &Geometry, b: &Geometry) -> bool {
 fn same<const N: usize, const D: usize>(mut a: [[f64;D];N],mut b: [[f64;D];N]) -> bool {
  let cmp=|x:&[f64;D],y:&[f64;D]| {for d in 0..D {let c=x[d].total_cmp(&y[d]);if !c.is_eq(){return c;}} std::cmp::Ordering::Equal};
  a.sort_by(cmp);b.sort_by(cmp);a==b
 }
 match (a,b) {(Geometry::Section(x),Geometry::Section(y))=>x.z_m==y.z_m && same(x.triangle_m,y.triangle_m),(Geometry::LocalTetra(x),Geometry::LocalTetra(y))=>same(*x,*y),_=>false}
}fn separated(a: &Geometry,b: &Geometry) -> bool {
    let (al,ah)=bounds(a); let (bl,bh)=bounds(b);
    (0..3).any(|d| ah[d] <= bl[d] || bh[d] <= al[d])
}
fn check(start: Instant, limit: Duration) -> Result<()> {
    if start.elapsed() >= limit { Err(error("DEADLINE", "mixed Gram deadline exceeded")) } else { Ok(()) }
}
fn cross_integral(p: &Prism, source: &PointPotential, rule: &GaussRule, start: Instant, limit: Duration) -> Result<f64> {
    let [a,b,c]=p.triangle_m; let mut total=0.;
    for (&u,&wu) in rule.nodes().iter().zip(rule.weights()) { for (&v,&wv) in rule.nodes().iter().zip(rule.weights()) { for (&z,&wz) in rule.nodes().iter().zip(rule.weights()) {
        check(start,limit)?;
        let point=[a[0]+u*(b[0]-a[0])+(1.-u)*v*(c[0]-a[0]),a[1]+u*(b[1]-a[1])+(1.-u)*v*(c[1]-a[1]),p.z_m[0]+z*(p.z_m[1]-p.z_m[0])];
        let phi=source.at(point)?; check(start,limit)?;
        total+=2.*p.volume_m3*(1.-u)*wu*wv*wz*phi;
    }}}
    if total.is_finite() && total>0. {Ok(total)} else {Err(error("HYBRID_RANGE","nonfinite mixed integral"))}
}
/// Raw density-weighted kernel Gram: multiply by 1e-7 for inductance in henries. IDs represent unique physical supports, so a shared return is supplied once.
pub(super) fn gram(cells: &[Cell], modes: usize, order: usize, budget: AllocationBudget, limit: Duration) -> Result<Vec<Vec<f64>>> {
    let start=Instant::now(); check(start,limit)?;
    if !(8..=60).contains(&order) {return Err(error("QUADRATURE_ORDER", "mixed section order must be 8..60"));}
    if cells.is_empty() || modes == 0 || cells.len()>budget.max_tetrahedra || modes>budget.max_nodes {return Err(error("HYBRID_INPUT", "nonempty bounded dimensions required"));}
    let bytes=modes.checked_mul(modes).and_then(|n|n.checked_mul(8)).and_then(|n|n.checked_add(cells.len().checked_mul(256)?)).and_then(|n|n.checked_add(cells.len().checked_mul(modes)?.checked_mul(24)?)).and_then(|n|n.checked_add(cells.iter().try_fold(0usize, |a,c| a.checked_add(c.id.len())?.checked_add(match &c.geometry {Geometry::Section(p)=>p.source_id.len(),_=>0}))?)).and_then(|n|n.checked_add(512*1024)).ok_or_else(||error("MEMORY_LIMIT","mixed Gram size overflow"))?;
    budget.check(bytes)?;
    let mut ids=HashSet::new();
    for c in cells {
        check(start,limit)?;
        if c.id.is_empty() || !ids.insert(&c.id) || c.currents.len()!=modes || !c.currents.iter().flatten().all(|x|x.is_finite()) {return Err(error("HYBRID_INPUT", "unique physical IDs and finite mode densities required"));}
        match &c.geometry {
            Geometry::Section(p) => { let validated=Prism::new(p.source_id.clone(),p.triangle_m,p.z_m)?; if validated.volume_m3 != p.volume_m3 {return Err(error("HYBRID_INPUT","invalid section volume"));} },
            Geometry::LocalTetra(t) => {PointPotential::tetrahedron(*t)?;},
        }
    }
    for (i,a) in cells.iter().enumerate() {for b in &cells[..i] {check(start,limit)?; if duplicate(&a.geometry,&b.geometry) {return Err(error("HYBRID_UNSUPPORTED", "duplicate physical geometry"));} if matches!((&a.geometry,&b.geometry),(Geometry::Section(_),Geometry::LocalTetra(_)) | (Geometry::LocalTetra(_),Geometry::Section(_))) && !separated(&a.geometry,&b.geometry) {return Err(error("HYBRID_UNSUPPORTED","section/event bounding boxes overlap; use full-volume fallback"));}}}
    let q=GaussRule::new(order)?; let mut out=vec![vec![0.;modes];modes];
    for (i,a) in cells.iter().enumerate() {for (j,b) in cells[..=i].iter().enumerate() {
        check(start,limit)?;
        let k=match (&a.geometry,&b.geometry) {
            (Geometry::Section(x),Geometry::Section(y)) => axial_magnetic::constant_pair(x,y,&q)?,
            (Geometry::LocalTetra(x),Geometry::LocalTetra(y)) => if i==j {polyhedron::tetra_self_integral(*x,order.min(16))?} else {polyhedron::tetra_integral_adaptive(*x,&PointPotential::tetrahedron(*y)?.with_tighter_tetra_tolerance(1e-12)?,order.min(16),1e-12)?},
            (Geometry::Section(p),Geometry::LocalTetra(t)) | (Geometry::LocalTetra(t),Geometry::Section(p)) => { let source=PointPotential::tetrahedron(*t)?.with_tighter_tetra_tolerance(1e-12)?; let coarse=cross_integral(p,&source,&q,start,limit)?; check(start,limit)?; let fine=cross_integral(p,&source,&GaussRule::new((order+4).min(64))?,start,limit)?; if order>60 || (fine-coarse).abs()>1e-8*fine.abs() {return Err(error("HYBRID_UNRESOLVED","mixed quadrature did not converge"));} fine },
        };
        check(start,limit)?;
        for m in 0..modes {for n in 0..modes {
            let dot=|x: [f64;3],y:[f64;3]| (0..3).map(|d|x[d]*y[d]).sum::<f64>();
            out[m][n]+=k*(dot(a.currents[m],b.currents[n])+if i==j {0.} else {dot(b.currents[m],a.currents[n])});
            if !out[m][n].is_finite() {return Err(error("HYBRID_RANGE","nonfinite Gram"));}
        }}
    }}
    Ok(out)
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn shared_return_and_event() {
  let mk=|id:&str,x:f64,currents:Vec<[f64;3]>| Cell{id:id.into(),geometry:Geometry::Section(Prism::new(id.into(),[[x,0.],[x+0.4,0.],[x,0.4]],[0.,1.]).unwrap()),currents};
  let mut cells=vec![mk("s1",0.,vec![[0.,0.,1.],[0.,0.,0.]]),mk("s2",1.,vec![[0.,0.,0.],[0.,0.,1.]]),mk("return",2.,vec![[0.,0.,-1.],[0.,0.,-1.]]),Cell{id:"event".into(),geometry:Geometry::LocalTetra([[0.,0.,1.5],[0.3,0.,1.5],[0.,0.3,1.5],[0.,0.,1.8]]),currents:vec![[0.,0.,1.],[0.,0.,0.]]}];
  let budget=AllocationBudget{max_bytes:2097152,max_nodes:100,max_tetrahedra:100};
  let g=gram(&cells,2,12,budget,Duration::from_secs(30)).unwrap();
  assert!((g[0][1]-g[1][0]).abs()<1e-12 && g[0][0]>0. && g[1][1]>0. && g[0][0]*g[1][1]>=g[0][1]*g[1][0]);
    // Independent full-volume quadrature: reverse each mixed pair, and use prism source fields for section pairs.
  let rule=GaussRule::new(16).unwrap(); let mut reference=[[0.;2];2];
  for i in 0..cells.len() {for j in 0..cells.len() {
   let k=match (&cells[i].geometry,&cells[j].geometry) {
    (Geometry::Section(a),Geometry::Section(b))=>polyhedron::volume_integral(a,&PointPotential::new(b).unwrap(),&rule).unwrap(),
    (Geometry::Section(p),Geometry::LocalTetra(t)) | (Geometry::LocalTetra(t),Geometry::Section(p))=>polyhedron::tetra_integral(*t,&PointPotential::new(p).unwrap(),16).unwrap(),
    (Geometry::LocalTetra(t),Geometry::LocalTetra(_))=>polyhedron::tetra_self_integral(*t,12).unwrap(),
   };
   for m in 0..2 {for n in 0..2 {reference[m][n]+=k*(0..3).map(|d|cells[i].currents[m][d]*cells[j].currents[n][d]).sum::<f64>();}}
  }}
  let mut max_error=0f64; for m in 0..2 {for n in 0..2 {max_error=max_error.max((g[m][n]-reference[m][n]).abs()/reference[m][n].abs()); assert!((g[m][n]-reference[m][n]).abs()<0.001*reference[m][n].abs(),"{m} {n}: {} {}",g[m][n],reference[m][n]);}} println!("full matrix max relative error {max_error:.12e}");
  cells[3].currents[0]=[0.;3]; let without=gram(&cells,2,12,budget,Duration::from_secs(30)).unwrap(); let t=match cells[3].geometry {Geometry::LocalTetra(t)=>t,_=>unreachable!()}; let self_term=polyhedron::tetra_self_integral(t,12).unwrap(); let mutual=g[0][0]-without[0][0]-self_term; assert!(mutual.abs()>1e-8, "mixed mutual vanished: {mutual}");
  let mut duplicate=cells[2].clone(); duplicate.id="other".into(); cells.push(duplicate); assert!(gram(&cells,2,12,budget,Duration::from_secs(30)).is_err());
 }
 #[test] fn cross_reference_and_splits() {
  let start=Instant::now(); let limit=Duration::from_secs(30);
  let p=Prism::new("section".into(),[[0.,0.],[0.4,0.],[0.,0.4]],[0.,1.]).unwrap();
  let t=[[0.,0.,1.5],[0.3,0.,1.5],[0.,0.3,1.5],[0.,0.,1.8]];
  let source=PointPotential::tetrahedron(t).unwrap().with_tighter_tetra_tolerance(1e-12).unwrap();
  let q=GaussRule::new(12).unwrap(); let fine=GaussRule::new(16).unwrap();
  let cross=cross_integral(&p,&source,&fine,start,limit).unwrap();
  let reverse_source=PointPotential::new(&p).unwrap();
  let r12=polyhedron::tetra_integral(t,&reverse_source,12).unwrap(); let r16=polyhedron::tetra_integral(t,&reverse_source,16).unwrap();
  let coarse=cross_integral(&p,&source,&q,start,limit).unwrap(); println!("cross forward 12/16 relative {:.12e}; reverse 12/16 {:.12e}; forward/reverse {:.12e}",(cross-coarse).abs()/cross,(r16-r12).abs()/r16,(cross-r16).abs()/r16); assert!((r16-r12).abs()<1e-8*r16 && (cross-r16).abs()<1e-8*r16, "{cross} {r12} {r16}");
  for n in [2,4] {let mut sum=0.;for i in 0..n {let mut part=p.clone();part.z_m=[i as f64/n as f64,(i+1) as f64/n as f64];part.volume_m3=p.volume_m3/n as f64;sum+=cross_integral(&part,&source,&q,start,limit).unwrap();} assert!((sum-cross).abs()<1e-8*cross);}
  assert!(cross>0.);
 }
 #[test] fn full_gram_section_splits() {
  let mk=|id:&str,x:f64,currents:Vec<[f64;3]>| Cell{id:id.into(),geometry:Geometry::Section(Prism::new(id.into(),[[x,0.],[x+0.4,0.],[x,0.4]],[0.,1.]).unwrap()),currents};
  let cells=vec![mk("s1",0.,vec![[0.,0.,1.],[0.,0.,0.]]),mk("s2",1.,vec![[0.,0.,0.],[0.,0.,1.]]),mk("return",2.,vec![[0.,0.,-1.],[0.,0.,-1.]]),Cell{id:"event".into(),geometry:Geometry::LocalTetra([[0.,0.,1.5],[0.3,0.,1.5],[0.,0.3,1.5],[0.,0.,1.8]]),currents:vec![[0.,0.,1.],[0.,0.,0.]]}];
  let budget=AllocationBudget{max_bytes:4*1024*1024,max_nodes:100,max_tetrahedra:100}; let start=Instant::now();
  let base=gram(&cells,2,8,budget,Duration::from_secs(30)).unwrap();
  for n in [2,4] {let mut split=Vec::new();for c in &cells {match &c.geometry {Geometry::Section(p)=>for i in 0..n {let mut s=c.clone();s.id=format!("{}:{i}",c.id);let mut p=p.clone();p.z_m=[i as f64/n as f64,(i+1) as f64/n as f64];p.volume_m3/=n as f64;s.geometry=Geometry::Section(p);split.push(s);},_=>split.push(c.clone())}} let got=gram(&split,2,8,budget,Duration::from_secs(30)).unwrap();let mut max_error=0f64; for i in 0..2 {for j in 0..2 {max_error=max_error.max((got[i][j]-base[i][j]).abs()/base[i][j].abs());assert!((got[i][j]-base[i][j]).abs()<0.0001*base[i][j].abs(),"n={n} {i} {j}: {} {}",got[i][j],base[i][j]);}} println!("split {n} max relative error {max_error:.12e}");}
  assert!(start.elapsed()<Duration::from_secs(30));
 }
 #[test] fn equal_bounds_are_not_duplicate_geometry() {
  let a=Geometry::LocalTetra([[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]);
  let b=Geometry::LocalTetra([[1.,1.,1.],[0.,1.,1.],[1.,0.,1.],[1.,1.,0.]]);
  assert_eq!(bounds(&a),bounds(&b)); assert!(!duplicate(&a,&b)); assert!(duplicate(&a,&a));
 }
 #[test] fn admission_rejects_expired_and_budget() {
  let p=Prism::new("x".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,1.]).unwrap(); let c=Cell{id:"x".into(),geometry:Geometry::Section(p),currents:vec![[0.,0.,1.]]};
  let budget=AllocationBudget{max_bytes:0,max_nodes:1,max_tetrahedra:1};
  assert!(gram(&[c.clone()],1,8,budget,Duration::from_secs(1)).is_err());
  assert!(gram(&[c],1,4,budget,Duration::ZERO).unwrap_err().contains("DEADLINE"));
 }
}










