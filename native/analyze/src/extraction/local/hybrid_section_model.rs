//! Explicit physical-cell adapter for the experimental mixed Gram.
#![allow(dead_code)]
use std::{collections::{BTreeMap,BTreeSet},time::{Duration,Instant}};
use super::{error,Result,AllocationBudget,mesh::LayeredMesh,volume_current::Space,volume_reduction::Basis,volume_field::ElectricApply,prism::Prism,hybrid_section_magnetic::{self,Cell,Geometry}};
use crate::electrical::coupled::Model;

pub(super) struct Group { pub prism: Prism, pub cells: Vec<usize> }

fn cells(mesh:&LayeredMesh,space:&Space,modes:&[Vec<f64>],groups:&[Group],budget:AllocationBudget,start:Instant,limit:Duration)->Result<Vec<Cell>> {
 let check=||if start.elapsed()>=limit {Err(error("DEADLINE","hybrid adapter deadline exceeded"))}else{Ok(())};check()?;
 let n=space.cells.len();
 let bytes=n.checked_mul(512).and_then(|x|x.checked_add(n.checked_mul(modes.len())?.checked_mul(24)?)).and_then(|x|x.checked_add(mesh.planned_peak_bytes)).and_then(|x|x.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","hybrid adapter plan overflow"))?; budget.check(bytes)?;
 if n==0 || modes.is_empty() || modes.iter().any(|m|m.len()!=3*n || m.iter().any(|x|!x.is_finite())) {return Err(error("HYBRID_INPUT","invalid current modes"));}
 let mut mesh_ids=BTreeSet::new();
 for c in &space.cells {check()?;if c.mesh_cell>=mesh.copper_tetrahedra.len() || !mesh_ids.insert(c.mesh_cell) || !c.density_per_amp_m2.is_finite() || c.density_per_amp_m2<=0. {return Err(error("HYBRID_INPUT","invalid physical cell ownership"));}}
 if mesh_ids.len()!=mesh.copper_tetrahedra.len() {return Err(error("HYBRID_INPUT","space must cover copper cells"));}
 let density=|i:usize| modes.iter().map(|m|std::array::from_fn(|k|m[3*i+k]*space.cells[i].density_per_amp_m2)).collect::<Vec<[f64;3]>>();
 let local=|i:usize|->Result<Cell> {let t=&mesh.copper_tetrahedra[space.cells[i].mesh_cell]; let mut v=[[0.;3];4];for k in 0..4 {v[k]=*mesh.nodes_m.get(t.nodes[k]).ok_or_else(||error("HYBRID_INPUT","invalid mesh node"))?;} Ok(Cell{id:format!("cell-{i}"),geometry:Geometry::LocalTetra(v),currents:density(i)})};
 let mut used=vec![false;n];let mut out=Vec::new();
 for (gidx,g) in groups.iter().enumerate() {
  check()?;
  let p=Prism::new(g.prism.source_id.clone(),g.prism.triangle_m,g.prism.z_m)?;
  if p.volume_m3!=g.prism.volume_m3 || g.cells.len()!=3 {return Err(error("HYBRID_INPUT","section requires exactly three slab tetrahedra"));}
  let mut faces:BTreeMap<Vec<usize>,Vec<usize>>=BTreeMap::new();let mut vertices=BTreeSet::new();let mut volume=0.;let mut identity=None;
  for &i in &g.cells {
   check()?;if i>=n || used[i] {return Err(error("HYBRID_INPUT","duplicate or invalid section cell"));} used[i]=true;
   let t=&mesh.copper_tetrahedra[space.cells[i].mesh_cell];
   let key=(t.slab,t.planar_triangle,t.source_ids.clone());
   if t.source_ids.len()!=1 || t.source_ids[0]!=p.source_id || identity.as_ref().is_some_and(|v|*v!=key) {return Err(error("HYBRID_INPUT","section source or slab ownership mismatch"));} identity=Some(key);
   volume+=t.volume_m3;
   for &node in &t.nodes {let v=*mesh.nodes_m.get(node).ok_or_else(||error("HYBRID_INPUT","invalid mesh node"))?;let ix=p.triangle_m.iter().position(|xy|*xy==[v[0],v[1]]);let iz=p.z_m.iter().position(|z|*z==v[2]);match (ix,iz) {(Some(x),Some(z))=>{vertices.insert((x,z));},_=>return Err(error("HYBRID_INPUT","section vertex mismatch"))};}
   for skip in 0..4 {let mut face=Vec::new();for k in 0..4 {if k!=skip {face.push(t.nodes[k]);}}face.sort();faces.entry(face).or_default().push(t.nodes[skip]);}
  }
  if vertices.len()!=6 || faces.values().filter(|x|x.len()==2).count()!=2 || faces.values().any(|x|x.len()>2) || (volume-p.volume_m3).abs()>32.*f64::EPSILON*p.volume_m3 {return Err(error("HYBRID_INPUT","invalid slab topology or volume"));}
  for (face,opposite) in &faces {
   let a=mesh.nodes_m[face[0]];let b=mesh.nodes_m[face[1]];let c=mesh.nodes_m[face[2]];
   let u=std::array::from_fn::<_,3,_>(|k|b[k]-a[k]);let v=std::array::from_fn::<_,3,_>(|k|c[k]-a[k]);
   let normal=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
   let side=|node:usize| (0..3).map(|k|normal[k]*(mesh.nodes_m[node][k]-a[k])).sum::<f64>();
   if opposite.len()==2 {let x=side(opposite[0]);let y=side(opposite[1]);if !x.is_finite() || !y.is_finite() || x==0. || y==0. || x.signum()==y.signum() {return Err(error("HYBRID_INPUT","overlapping slab tetrahedra"));}}
   else {let coords:Vec<_>=face.iter().map(|&i|mesh.nodes_m[i]).collect();let boundary=p.z_m.iter().any(|z|coords.iter().all(|q|q[2]==*z)) || (0..3).any(|missing|coords.iter().all(|q|p.triangle_m.iter().enumerate().any(|(i,xy)|i!=missing && *xy==[q[0],q[1]])));if !boundary {return Err(error("HYBRID_INPUT","slab face is not on prism boundary"));}}
  }
  let first=density(g.cells[0]);let uniform=g.cells.iter().skip(1).all(|&i| {let d=density(i);d.iter().zip(&first).all(|(a,b)|(0..3).all(|k|(a[k]-b[k]).abs()<=16.*f64::EPSILON*a[k].abs().max(b[k].abs())))});
  if uniform {out.push(Cell{id:format!("section-{gidx}"),geometry:Geometry::Section(p),currents:first});} else {for &i in &g.cells {out.push(local(i)?);}}
 }
 for (i,covered) in used.iter().enumerate() {check()?;if !covered {out.push(local(i)?);}}
 Ok(out)
}
pub(super) fn model(mesh:&LayeredMesh,space:&Space,basis:&Basis,groups:&[Group],field:&dyn ElectricApply,ports:&[[usize;2]],order:usize,budget:AllocationBudget,limit:Duration)->Result<Model> {
 let start=Instant::now(); let ni=basis.current.len(); let nv=basis.voltage.len(); let n=space.cells.len();
 // Reserve persistent basis, adapter cells, matrix copies and full-space gradients
 // before asking either downstream stage to admit its own working set.
 let reserve=ni.checked_mul(n).and_then(|x|x.checked_mul(24)).and_then(|x|x.checked_add(nv.checked_mul(space.potential_count())?.checked_mul(8)?)).and_then(|x|x.checked_add(nv.checked_mul(space.current_count())?.checked_mul(8)?)).and_then(|x|x.checked_add(n.checked_mul(512)?)).and_then(|x|x.checked_add(n.checked_mul(ni)?.checked_mul(24)?)).and_then(|x|x.checked_add(ni.checked_mul(ni)?.checked_mul(24)?)).ok_or_else(||error("MEMORY_LIMIT","hybrid model plan overflow"))?;
 budget.check(reserve)?; let remaining=AllocationBudget{max_bytes:budget.max_bytes-reserve,..budget};
 let cells=cells(mesh,space,&basis.current,groups,remaining,start,limit)?;
 let left=limit.checked_sub(start.elapsed()).ok_or_else(||error("DEADLINE","hybrid adapter deadline exceeded"))?;
 let mut gram=hybrid_section_magnetic::gram(&cells,ni,order,remaining,left)?;
 scale_henries(&mut gram);
 basis.model_with_inductance(space,field,&gram,ports,remaining)
}
fn scale_henries(gram:&mut [Vec<f64>]) {
 for i in 0..gram.len() {for j in 0..=i {let v=0.5*(gram[i][j]+gram[j][i])*1e-7;gram[i][j]=v;gram[j][i]=v;}}
}
#[cfg(test)] mod tests {
 use super::*; use super::super::{mesh::{self,LayeredInput,Material},volume_current};
 #[test] fn units_once() {let mut g=vec![vec![2.,3.],vec![3.,4.]];scale_henries(&mut g);assert_eq!(g,vec![vec![2e-7,3e-7],vec![3e-7,4e-7]]);}
 #[test] fn coverage_and_uniform_fallback() {
  let b=AllocationBudget{max_bytes:16_000_000,max_nodes:100,max_tetrahedra:100};
  let mesh=mesh::extrude(&LayeredInput{xy_m:vec![[0.,0.],[1.,0.],[0.,1.]],triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,1.,2.],materials:vec![vec![Material::Copper{source_ids:vec!["a".into()]}];2]},b).unwrap();
  let space=volume_current::assemble(&mesh,&vec![1.;6],&[],b).unwrap();
  let group=||Group{prism:Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,1.]).unwrap(),cells:vec![0,1,2]};
  let mut mode=vec![0.;18];for i in 0..6 {mode[3*i]=1./space.cells[i].density_per_amp_m2;}
  let c=cells(&mesh,&space,&[mode.clone()],&[group()],b,Instant::now(),Duration::from_secs(30)).unwrap();assert_eq!(c.len(),4);assert!(matches!(c[0].geometry,Geometry::Section(_)));
  mode[3]*=2.;let c=cells(&mesh,&space,&[mode.clone()],&[group()],b,Instant::now(),Duration::from_secs(30)).unwrap();assert_eq!(c.len(),6);assert!(c.iter().all(|c|matches!(c.geometry,Geometry::LocalTetra(_))));
  assert!(cells(&mesh,&space,&[mode],&[group(),group()],b,Instant::now(),Duration::from_secs(30)).is_err());
 }
 #[test] fn physical_coupled_comparison() {
  use super::super::{volume_field,volume_reduction,volume_integral_magnetic,section_return};
  use crate::electrical::{coupled,Counters};
  let b=AllocationBudget{max_bytes:256*1024*1024,max_nodes:1000,max_tetrahedra:1000};
  let xs=[-0.001,0.,0.001,0.002];let ys=[-0.001,0.,0.001,0.002,0.003,0.004,0.005,0.006];
  let xy:Vec<_>=ys.iter().flat_map(|&y|xs.iter().map(move |&x|[x,y])).collect();
  let triangles:Vec<_>=(0..7).flat_map(|y|(0..3).flat_map(move |x|{let a=4*y+x;[[a,a+1,a+5],[a,a+5,a+4]]})).collect();
  let mut edges=vec![];for x in 0..3 {edges.extend([[x,x+1],[28+x,29+x]]);}for y in 0..7 {edges.extend([[4*y,4*y+4],[4*y+3,4*y+7]]);}
  let dielectric=||Material::Dielectric{label:"air".into(),epsilon_r:1.};
  let mat:Vec<_>=(0..7).flat_map(|y|(0..3).flat_map(move |x|{let m=if x==1 && [1,3,5].contains(&y) {Material::Copper{source_ids:vec![format!("c{}",(y-1)/2)]}}else{dielectric()};[m.clone(),m]})).collect();
  let mesh=mesh::extrude(&LayeredInput{xy_m:xy.clone(),triangles:triangles.clone(),boundary_edges:edges,z_m:vec![-0.001,0.,0.001,0.002],materials:vec![vec![dielectric();triangles.len()],mat,vec![dielectric();triangles.len()]]},b).unwrap();
  let electrodes:Vec<Vec<usize>>=(0..3).flat_map(|c|[0.,0.001].map(|z|mesh.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[2]==z && p[0]>=0. && p[0]<=0.001 && p[1]>=2.*c as f64*0.001 && p[1]<=(2*c+1) as f64*0.001).then_some(i)).collect())).collect();
  let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&electrodes,b).unwrap();
  let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,n)|(format!("p{i}"),space.node_potential[&n[0]])).collect();
  let basis=volume_reduction::harmonic(&space,&contacts,b).unwrap();
  let field=volume_field::Field::new(&mesh,&space,b).unwrap();
  let magnetic=volume_integral_magnetic::Operator::new(&mesh,b,8).unwrap();
  let groups:Vec<_>=space.cells.iter().filter_map(|c|{let t=&mesh.copper_tetrahedra[c.mesh_cell];(t.source_ids==["c0"]).then_some(t.planar_triangle)}).collect::<BTreeSet<_>>().into_iter().map(|t|Group{prism:Prism::new("c0".into(),triangles[t].map(|i|xy[i]),[0.,0.001]).unwrap(),cells:space.cells.iter().enumerate().filter_map(|(i,c)|(mesh.copper_tetrahedra[c.mesh_cell].planar_triangle==t).then_some(i)).collect()}).collect();
  let mapped=cells(&mesh,&space,&basis.current,&groups,b,Instant::now(),Duration::from_secs(30)).unwrap();assert!(mapped.iter().any(|c|matches!(c.geometry,Geometry::Section(_))));assert!(mapped.iter().any(|c|matches!(c.geometry,Geometry::LocalTetra(_))));
  let ports=[[0,4],[1,5],[2,4],[3,5]];
  let old=basis.model(&space,&field,&magnetic,&ports,b).unwrap();
  let new=model(&mesh,&space,&basis,&groups,&field,&ports,12,b,Duration::from_secs(60)).unwrap();
  let global_c:f64=old.capacitance_f.iter().map(|e|e.2).sum();eprintln!("global constant capacitance {global_c:e}");assert!(global_c>0.);
  assert_eq!(old.resistance_ohm,new.resistance_ohm);assert_eq!(old.capacitance_f,new.capacitance_f);assert_eq!(old.incidence,new.incidence);assert_eq!(old.port_incidence,new.port_incidence);assert_eq!(old.conserved_potential_modes,new.conserved_potential_modes);
  let lerr=old.inductance_h.iter().zip(&new.inductance_h).map(|(a,b)|(a.2-b.2).abs()).fold(0.,f64::max);let lscale=old.inductance_h.iter().map(|a|a.2.abs()).fold(0.,f64::max);eprintln!("L relative max {}",lerr/lscale);assert!(lerr/lscale<0.001);
  let ni=basis.current.len();assert!(basis.model_with_inductance(&space,&field,&vec![vec![0.;ni];ni+1],&ports,b).is_err());let mut bad=vec![vec![0.;ni];ni];bad[0][0]=f64::NAN;assert!(basis.model_with_inductance(&space,&field,&bad,&ports,b).is_err());if ni>1 {bad[0][0]=0.;bad[0][1]=1.;assert!(basis.model_with_inductance(&space,&field,&bad,&ports,b).is_err());}
  let mut worst_matched=0f64;let mut matched_success=0;
  for hz in [1000.,10000.] {for drive in 0..2 {for matched in [false,true] {
   let mut a=coupled::System::compile(&old,4,None).unwrap();let mut z=coupled::System::compile(&new,4,None).unwrap();
   if matched {for p in 0..4 {a.add_port_admittance(p,1./50.,0.).unwrap();z.add_port_admittance(p,1./50.,0.).unwrap();}}
   let n=old.potential_nodes.len()+old.current_dofs.len();let mut amplitudes=[0.;4];amplitudes[2*drive]=1.;amplitudes[2*drive+1]=-1.;let mut rhs=a.port_rhs(&amplitudes);rhs.resize(2*n,0.);let w=2.*std::f64::consts::PI*hz;
   let started=Instant::now();let xr=a.solve_charge_coordinates(w,&rhs,&mut Counters::default());assert!(started.elapsed()<Duration::from_secs(30));
   let started=Instant::now();let yr=z.solve_charge_coordinates(w,&rhs,&mut Counters::default());assert!(started.elapsed()<Duration::from_secs(30));
   let (x,y)=match (xr,yr) {(Ok(x),Ok(y))=>(x,y),(x,y) if matched=>{let errors:Vec<_>=[x.err(),y.err()].into_iter().flatten().collect();eprintln!("matched solver refusal {hz} drive {drive}: {errors:?}");assert!(!errors.is_empty() && errors.iter().all(|e|e.starts_with("NETWORK_RESIDUAL:")||e.starts_with("COUPLED_COORDINATES:")));continue;},(x,y)=>panic!("unloaded solver refusal: {:?} {:?}",x.err(),y.err())};
   assert!(a.test_relative_residual()<=1e-8);assert!(z.test_relative_residual()<=1e-8);
   let ar=a.port_voltages(&x,4);let ai=a.port_voltages(&x[n..],4);let br=z.port_voltages(&y,4);let bi=z.port_voltages(&y[n..],4);let err=(0..4).map(|i|(ar[i]-br[i]).hypot(ai[i]-bi[i])).fold(0.,f64::max);let scale=(0..4).map(|i|ar[i].hypot(ai[i])).fold(0.,f64::max);eprintln!("{hz} drive {drive} matched={matched} port relative {} residuals {} {}",err/scale,a.test_relative_residual(),z.test_relative_residual());if matched {matched_success+=1;worst_matched=worst_matched.max(err/scale);}
   if !matched {let mut injection=vec![0.;space.potential_count()];injection[contacts[2*drive].1]=1.;injection[contacts[2*drive+1].1]=-1.;injection[contacts[4].1]=-1.;injection[contacts[5].1]=1.;let started=Instant::now();let admission=section_return::solve_candidate(&basis,&space,&field,&magnetic,&mut z,w,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());assert!(started.elapsed()<Duration::from_secs(30));let reason=admission.unwrap_err();eprintln!("original-space refusal {hz} drive {drive}: {reason}");assert!(reason.starts_with("LOCAL_SECTION_RETURN_RESIDUAL:"));}
  }}}
  assert_eq!(matched_success,4);assert!(worst_matched<1e-3);
  eprintln!("matched max relative error {worst_matched:e}; candidate unadmitted by original-space KVL");

 }

}


