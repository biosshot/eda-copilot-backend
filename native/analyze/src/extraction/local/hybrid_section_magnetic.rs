//! Experimental mixed section/event Gram. No production dispatch or KCL network claim. Densities are per ampere, not necessarily unit-current profiles.
#![allow(dead_code)]
use std::{collections::HashSet, time::{Duration, Instant}};
use super::{error, Result, AllocationBudget, axial_magnetic, polyhedron::{self, PointPotential}, prism::{GaussRule, Prism}};

#[derive(Clone)]
pub(super) enum Geometry { Section(Prism), LocalTetra([[f64; 3]; 4]) }
#[derive(Clone)]
pub(super) struct Cell { pub id: String, pub geometry: Geometry, pub currents: Vec<[f64; 3]> }

/// One physical support; the adapter validates slab ownership and topology.
pub(super) struct PhysicalGroup { prism: Option<Prism>, tetrahedra: Vec<Cell> }
pub(super) enum ModeSupport<'a> { Section(&'a Prism, [f64; 3]), Tetrahedra(&'a [Cell]) }
impl PhysicalGroup {
    pub(super) fn from_validated_adapter(prism: Option<Prism>, tetrahedra: Vec<Cell>, modes: usize, budget: AllocationBudget, start: Instant, limit: Duration) -> Result<Self> {
        check(start,limit)?;
        if modes == 0 || modes > budget.max_nodes || tetrahedra.len() > budget.max_tetrahedra || tetrahedra.len() != if prism.is_some() {3} else {1} {return Err(error("HYBRID_INPUT", "invalid physical group dimensions"));}
        let bytes=tetrahedra.len().checked_mul(256).and_then(|n|n.checked_add(tetrahedra.len().checked_mul(modes)?.checked_mul(24)?)).and_then(|n|tetrahedra.iter().try_fold(n,|a,c|a.checked_add(c.id.len()))).and_then(|n|n.checked_add(prism.as_ref().map_or(0,|p|p.source_id.len()))).ok_or_else(||error("MEMORY_LIMIT","physical group size overflow"))?;
        budget.check(bytes)?;
        if let Some(p)=&prism {let v=Prism::new(p.source_id.clone(),p.triangle_m,p.z_m)?;if v.volume_m3!=p.volume_m3 {return Err(error("HYBRID_INPUT","invalid section volume"));}}
        let mut ids=HashSet::new();
        for (i,c) in tetrahedra.iter().enumerate() {
            check(start,limit)?;
            if c.id.is_empty() || !ids.insert(&c.id) || c.currents.len()!=modes || !c.currents.iter().flatten().all(|x|x.is_finite()) {return Err(error("HYBRID_INPUT","invalid group currents or IDs"));}
            match &c.geometry {Geometry::LocalTetra(t)=>{PointPotential::tetrahedron(*t)?;},_=>return Err(error("HYBRID_INPUT","group requires tetrahedra"))}
            for other in &tetrahedra[..i] {if duplicate(&c.geometry,&other.geometry) {return Err(error("HYBRID_UNSUPPORTED","duplicate physical geometry"));}}
        }
        check(start,limit)?; Ok(Self{prism,tetrahedra})
    }
    pub(super) fn support(&self, mode: usize) -> Result<ModeSupport<'_>> {
        let first=*self.tetrahedra[0].currents.get(mode).ok_or_else(||error("HYBRID_INPUT","invalid mode"))?;
        if let Some(p)=&self.prism {if self.tetrahedra.iter().skip(1).all(|c|(0..3).all(|k|{let a=c.currents[mode][k];let b=first[k];(a-b).abs()<=16.*f64::EPSILON*a.abs().max(b.abs())})) {return Ok(ModeSupport::Section(p,first));}}
        Ok(ModeSupport::Tetrahedra(&self.tetrahedra))
    }
}

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
pub(super) fn cross_integral(p: &Prism, source: &PointPotential, rule: &GaussRule, start: Instant, limit: Duration) -> Result<f64> {
    let [a,b,c]=p.triangle_m; let mut total=0.;
    for (&u,&wu) in rule.nodes().iter().zip(rule.weights()) { for (&v,&wv) in rule.nodes().iter().zip(rule.weights()) { for (&z,&wz) in rule.nodes().iter().zip(rule.weights()) {
        check(start,limit)?;
        let point=[a[0]+u*(b[0]-a[0])+(1.-u)*v*(c[0]-a[0]),a[1]+u*(b[1]-a[1])+(1.-u)*v*(c[1]-a[1]),p.z_m[0]+z*(p.z_m[1]-p.z_m[0])];
        let phi=source.at(point)?; check(start,limit)?;
        total+=2.*p.volume_m3*(1.-u)*wu*wv*wz*phi;
    }}}
    if total.is_finite() && total>0. {Ok(total)} else {Err(error("HYBRID_RANGE","nonfinite mixed integral"))}
}
fn kernel(a:&Geometry,b:&Geometry,same:bool,order:usize,q:&GaussRule,start:Instant,limit:Duration)->Result<f64> {
    let value=
    match (a,b) {
            (Geometry::Section(x),Geometry::Section(y)) => axial_magnetic::constant_pair(x,y,q)?,
            (Geometry::LocalTetra(x),Geometry::LocalTetra(y)) => if same {polyhedron::tetra_self_integral(*x,order.min(16))?} else {polyhedron::tetra_integral_adaptive(*x,&PointPotential::tetrahedron(*y)?.with_tighter_tetra_tolerance(1e-12)?,order.min(16),1e-12)?},
            (Geometry::Section(p),Geometry::LocalTetra(t)) | (Geometry::LocalTetra(t),Geometry::Section(p)) => { let source=PointPotential::tetrahedron(*t)?.with_tighter_tetra_tolerance(1e-12)?; let coarse=cross_integral(p,&source,q,start,limit)?; check(start,limit)?; let fine=cross_integral(p,&source,&GaussRule::new((order+4).min(64))?,start,limit)?; if order>60 || (fine-coarse).abs()>1e-8*fine.abs() {return Err(error("HYBRID_UNRESOLVED","mixed quadrature did not converge"));} fine },
        };
    Ok(value)
}
/// Mode-specific representations of unique physical groups. The adapter owns
/// the topology proof; mixed Section/Tet pairs use the original tetrahedra.
#[derive(Default, Debug)]
pub(super) struct GroupKernelCounts {
 pub local_section_tetra_pairs: usize,
 pub far_section_pairs: usize,
 pub mixed_tetra_pairs: usize,
}
pub(super) fn grouped_gram(groups:&[PhysicalGroup],modes:usize,order:usize,budget:AllocationBudget,limit:Duration)->Result<Vec<Vec<f64>>> {
 Ok(grouped_gram_counted(groups,modes,order,budget,limit)?.0)
}
pub(super) fn grouped_gram_counted(groups:&[PhysicalGroup],modes:usize,order:usize,budget:AllocationBudget,limit:Duration)->Result<(Vec<Vec<f64>>,GroupKernelCounts)> {
 let start=Instant::now();check(start,limit)?;
 if groups.is_empty() || modes==0 || modes>budget.max_nodes || !(8..=60).contains(&order) {return Err(error("HYBRID_INPUT","invalid grouped Gram dimensions"));}
 let count=groups.iter().try_fold(0usize,|n,g|n.checked_add(g.tetrahedra.len())?.checked_add(usize::from(g.prism.is_some()))).ok_or_else(||error("MEMORY_LIMIT","group support overflow"))?;
 if groups.iter().map(|g|g.tetrahedra.len()).sum::<usize>()>budget.max_tetrahedra {return Err(error("HYBRID_INPUT","too many group supports"));}
 let bytes=count.checked_mul(256).and_then(|n|n.checked_add(16*8)).and_then(|n|n.checked_add(count.checked_mul(modes)?.checked_mul(24)?)).and_then(|n|n.checked_add(groups.len().checked_mul(128)?)).and_then(|n|groups.iter().try_fold(n,|a,g| {let a=g.tetrahedra.iter().try_fold(a,|b,t|b.checked_add(t.id.len()))?;a.checked_add(g.prism.as_ref().map_or(Some(0),|p|p.source_id.len().checked_mul(2))?)})).and_then(|n|n.checked_add(modes.checked_mul(modes)?.checked_mul(8)?)).and_then(|n|n.checked_add(512*1024)).ok_or_else(||error("MEMORY_LIMIT","group cache overflow"))?;budget.check(bytes)?;
 let mut geom=Vec::with_capacity(count);let mut owners=Vec::with_capacity(count);let mut starts=Vec::with_capacity(groups.len());let mut ids=HashSet::new();
 for (gi,g) in groups.iter().enumerate() {check(start,limit)?;starts.push(geom.len());for t in &g.tetrahedra {if t.currents.len()!=modes || !ids.insert(&t.id) {return Err(error("HYBRID_INPUT","invalid group modes or duplicate physical ID"));}geom.push(t.geometry.clone());owners.push(gi);}if let Some(p)=&g.prism {geom.push(Geometry::Section(p.clone()));owners.push(gi);}}
 for i in 0..count {for j in 0..i {check(start,limit)?;if owners[i]!=owners[j] {if duplicate(&geom[i],&geom[j]) {return Err(error("HYBRID_UNSUPPORTED","duplicate physical geometry"));}}}}
 let q=GaussRule::new(order)?;let mut out=vec![vec![0.;modes];modes]; let mut counts=GroupKernelCounts::default();
 for (gi,g) in groups.iter().enumerate() {for (hj,h) in groups[..=gi].iter().enumerate() {let mut cache=[[f64::NAN;4];4];for m in 0..modes {for n in 0..modes {
  check(start,limit)?;
  let select=|group:&PhysicalGroup,base:usize,mode:usize|->Result<Vec<(usize,[f64;3])>> {Ok(match group.support(mode)? {ModeSupport::Section(_,v)=>vec![(base+group.tetrahedra.len(),v)],ModeSupport::Tetrahedra(ts)=>ts.iter().enumerate().map(|(i,t)|(base+i,t.currents[mode])).collect()})};
  for (a,va) in select(g,starts[gi],m)? {for (b,vb) in select(h,starts[hj],n)? {
   let coefficient=(0..3).map(|d|va[d]*vb[d]).sum::<f64>();if coefficient==0. {continue;}
   let mut pairs=Vec::new();
   let section_pair=matches!((&geom[a],&geom[b]),(Geometry::Section(_),Geometry::Section(_)));
   // Strict bounding-box separation is sufficient for the axial far path;
   // overlap or boundary contact is conservatively treated as local.
   let strict_separation=|| {let (al,ah)=bounds(&geom[a]);let (bl,bh)=bounds(&geom[b]);(0..3).any(|d|ah[d]<bl[d] || bh[d]<al[d])};
   let local_section=section_pair && (gi==hj || !strict_separation());
   let mixed=matches!((&geom[a],&geom[b]),(Geometry::Section(_),Geometry::LocalTetra(_)) | (Geometry::LocalTetra(_),Geometry::Section(_)));
   if local_section {for x in 0..g.tetrahedra.len(){for y in 0..h.tetrahedra.len(){pairs.push((starts[gi]+x,starts[hj]+y));}}}
   else if matches!((&geom[a],&geom[b]),(Geometry::Section(_),Geometry::LocalTetra(_))) {for t in 0..g.tetrahedra.len(){pairs.push((starts[gi]+t,b));}}
   else if matches!((&geom[a],&geom[b]),(Geometry::LocalTetra(_),Geometry::Section(_))) {for t in 0..h.tetrahedra.len(){pairs.push((a,starts[hj]+t));}}
   else {pairs.push((a,b));}
   let mut k=0.;for (x,y) in pairs {let slotx=x-starts[gi];let sloty=y-starts[hj];if cache[slotx][sloty].is_nan(){let (u,v)=if x>=y {(x,y)}else{(y,x)};cache[slotx][sloty]=kernel(&geom[u],&geom[v],u==v,order,&q,start,limit)?;if local_section {counts.local_section_tetra_pairs+=1;} else if section_pair {counts.far_section_pairs+=1;} else if mixed {counts.mixed_tetra_pairs+=1;}}k+=cache[slotx][sloty];}
   let contribution=k*coefficient;out[m][n]+=contribution;
   if gi!=hj {out[n][m]+=contribution;}
   if !out[m][n].is_finite() || !out[n][m].is_finite(){return Err(error("HYBRID_RANGE","nonfinite Gram"));}
  }}
 }}}}
 Ok((out,counts))
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
        let k=kernel(&a.geometry,&b.geometry,i==j,order,&q,start,limit)?;
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
 #[test] fn physical_group_selects_per_mode() {
  let p=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,1.]).unwrap();
  let a=[0.,0.,0.];let b=[1.,0.,0.];let c=[0.,1.,0.];let d=[0.,0.,1.];let e=[1.,0.,1.];let f=[0.,1.,1.];
  let ts=[[a,b,c,f],[a,b,e,f],[a,d,e,f]];
  let mk=|enriched:bool| ts.iter().enumerate().map(|(i,t)|Cell{id:format!("t{i}"),geometry:Geometry::LocalTetra(*t),currents:if enriched {vec![[0.,0.,1.],[0.,0.,[-1.,0.,1.][i]]]} else {vec![[0.,0.,1.]]}}).collect();
  let budget=AllocationBudget{max_bytes:2_000_000,max_nodes:10,max_tetrahedra:10};
  let group=PhysicalGroup::from_validated_adapter(Some(p.clone()),mk(false),1,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  assert!(matches!(group.support(0).unwrap(),ModeSupport::Section(_, _)));
  let group=PhysicalGroup::from_validated_adapter(Some(p),mk(true),2,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  assert!(matches!(group.support(0).unwrap(),ModeSupport::Section(_, _)));
  assert!(matches!(group.support(1).unwrap(),ModeSupport::Tetrahedra(_)));
  assert!(group.support(2).is_err());
  let ret=Cell{id:"return".into(),geometry:Geometry::LocalTetra([[3.,0.,0.],[4.,0.,0.],[3.,1.,0.],[3.,0.,1.]]),currents:vec![[0.,0.,-1.],[0.,0.,-1.]]};
  let far_prism=Prism::new("far".into(),[[6.,0.],[7.,0.],[6.,1.]],[0.,1.]).unwrap();
  let far_cells:Vec<_>=ts.iter().enumerate().map(|(i,t)| Cell{id:format!("far{i}"),geometry:Geometry::LocalTetra(t.map(|mut p| {p[0]+=6.;p})),currents:vec![[0.,0.,1.],[0.,0.,1.]]}).collect();
  let far=PhysicalGroup::from_validated_adapter(Some(far_prism),far_cells.clone(),2,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  let mut all=group.tetrahedra.clone();all.extend(far_cells);all.push(ret.clone());
  let reference=gram(&all,2,12,budget,Duration::from_secs(30)).unwrap();
  let other=PhysicalGroup::from_validated_adapter(None,vec![ret],2,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  let duplicate=PhysicalGroup::from_validated_adapter(None,vec![all[0].clone()],2,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  assert!(grouped_gram(&[duplicate],2,12,AllocationBudget{max_bytes:0,..budget},Duration::from_secs(30)).is_err());
  let duplicate=PhysicalGroup::from_validated_adapter(None,vec![all[0].clone()],2,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  let duplicate2=PhysicalGroup::from_validated_adapter(None,vec![all[0].clone()],2,budget,Instant::now(),Duration::from_secs(1)).unwrap();
  assert!(grouped_gram(&[duplicate,duplicate2],2,12,budget,Duration::from_secs(30)).is_err());
  let (got,counts)=grouped_gram_counted(&[group,far,other],2,12,budget,Duration::from_secs(30)).unwrap();
  assert!(counts.local_section_tetra_pairs>0 && counts.far_section_pairs>0 && counts.mixed_tetra_pairs>0, "{counts:?}");
  assert!((got[0][1]-got[1][0]).abs()<1e-10*got[0][0].abs() && got[0][0]>0. && got[1][1]>0. && got[0][0]*got[1][1]>=got[0][1]*got[1][0]);
  for i in 0..2 {for j in 0..2 {assert!((got[i][j]-reference[i][j]).abs()<0.001*reference[0][0].abs(),"{i} {j}: {} {}",got[i][j],reference[i][j]);}}
 }
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
