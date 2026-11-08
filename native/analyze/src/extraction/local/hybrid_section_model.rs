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
fn physical_groups(mesh:&LayeredMesh,space:&Space,modes:&[Vec<f64>],groups:&[Group],budget:AllocationBudget,start:Instant,limit:Duration)->Result<Vec<hybrid_section_magnetic::PhysicalGroup>> {
 // Reuse the existing complete-coverage, slab topology and ownership admission.
 // Discard its all-mode representation choice, retaining the original cells.
 drop(cells(mesh,space,modes,groups,budget,start,limit)?);
 let mut original:Vec<_>=cells(mesh,space,modes,&[],budget,start,limit)?.into_iter().map(Some).collect();
 let mut out=Vec::new();
 for g in groups {
  let tetrahedra=g.cells.iter().map(|&i|original.get_mut(i).and_then(Option::take).ok_or_else(||error("HYBRID_INPUT","duplicate or invalid physical group"))).collect::<Result<Vec<_>>>()?;
  out.push(hybrid_section_magnetic::PhysicalGroup::from_validated_adapter(Some(g.prism.clone()),tetrahedra,modes.len(),budget,start,limit)?);
 }
 for c in original.into_iter().flatten() {out.push(hybrid_section_magnetic::PhysicalGroup::from_validated_adapter(None,vec![c],modes.len(),budget,start,limit)?);}
 Ok(out)
}
pub(super) fn model(mesh:&LayeredMesh,space:&Space,basis:&Basis,groups:&[Group],field:&dyn ElectricApply,ports:&[[usize;2]],order:usize,budget:AllocationBudget,limit:Duration)->Result<Model> {
 let start=Instant::now(); let ni=basis.current.len(); let nv=basis.voltage.len(); let n=space.cells.len();
 // Reserve persistent basis, adapter cells, matrix copies and full-space gradients
 // before asking either downstream stage to admit its own working set.
 let reserve=ni.checked_mul(n).and_then(|x|x.checked_mul(24)).and_then(|x|x.checked_add(nv.checked_mul(space.potential_count())?.checked_mul(8)?)).and_then(|x|x.checked_add(nv.checked_mul(space.current_count())?.checked_mul(8)?)).and_then(|x|x.checked_add(n.checked_mul(512)?)).and_then(|x|x.checked_add(n.checked_mul(ni)?.checked_mul(24)?)).and_then(|x|x.checked_add(ni.checked_mul(ni)?.checked_mul(24)?)).ok_or_else(||error("MEMORY_LIMIT","hybrid model plan overflow"))?;
 budget.check(reserve)?; let remaining=AllocationBudget{max_bytes:budget.max_bytes-reserve,..budget};
 let groups=physical_groups(mesh,space,&basis.current,groups,remaining,start,limit)?;
 let left=limit.checked_sub(start.elapsed()).ok_or_else(||error("DEADLINE","hybrid adapter deadline exceeded"))?;
 let mut gram=hybrid_section_magnetic::grouped_gram(&groups,ni,order,remaining,left)?;
 scale_henries(&mut gram);
 basis.model_with_inductance(space,field,&gram,ports,remaining)
}
fn scale_henries(gram:&mut [Vec<f64>]) {
 for i in 0..gram.len() {for j in 0..=i {let v=0.5*(gram[i][j]+gram[j][i])*1e-7;gram[i][j]=v;gram[j][i]=v;}}
}
#[cfg(test)] mod tests {
 use super::*; use super::super::{mesh::{self,LayeredInput,Material},volume_current};
 #[test] fn units_once() {let mut g=vec![vec![2.,3.],vec![3.,4.]];scale_henries(&mut g);assert_eq!(g,vec![vec![2e-7,3e-7],vec![3e-7,4e-7]]);}
 #[test]
 fn disconnected_islands_carry_ac_displacement_current() {
  use super::super::{volume_field,volume_reduction};
  use crate::electrical::{coupled,Counters};
  let started=Instant::now();
  let b=AllocationBudget{max_bytes:256*1024*1024,max_nodes:1000,max_tetrahedra:1000};
  // Same finite three-strip fixture as physical_coupled_comparison: separated
  // copper islands embedded in air, with no conductor spanning either gap.
  let xs=[-0.001,0.,0.001,0.002];let ys=[-0.001,0.,0.001,0.002,0.003,0.004,0.005,0.006];
  let xy:Vec<_>=ys.iter().flat_map(|&y|xs.iter().map(move |&x|[x,y])).collect();
  let triangles:Vec<_>=(0..7).flat_map(|y|(0..3).flat_map(move |x|{let a=4*y+x;[[a,a+1,a+5],[a,a+5,a+4]]})).collect();
  let mut edges=vec![];for x in 0..3 {edges.extend([[x,x+1],[28+x,29+x]]);}for y in 0..7 {edges.extend([[4*y,4*y+4],[4*y+3,4*y+7]]);}
  let dielectric=||Material::Dielectric{label:"air".into(),epsilon_r:1.};
  let mat:Vec<_>=(0..7).flat_map(|y|(0..3).flat_map(move |x|{let m=if x==1 && [1,3,5].contains(&y) {Material::Copper{source_ids:vec![format!("c{}",(y-1)/2)]}}else{dielectric()};[m.clone(),m]})).collect();
  let mesh=mesh::extrude(&LayeredInput{xy_m:xy.clone(),triangles:triangles.clone(),boundary_edges:edges,z_m:vec![-0.001,0.,0.001,0.002],materials:vec![vec![dielectric();triangles.len()],mat,vec![dielectric();triangles.len()]]},b).unwrap();
  let electrodes:Vec<Vec<usize>>=(0..3).flat_map(|c|[0.,0.001].map(|z|mesh.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[2]==z && p[0]>=0. && p[0]<=0.001 && p[1]>=2.*c as f64*0.001 && p[1]<=(2*c+1) as f64*0.001).then_some(i)).collect())).collect();
  let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&electrodes,b).unwrap();
  assert_eq!(space.components.len(),3);
  let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,n)|(format!("p{i}"),space.node_potential[&n[0]])).collect();
  let island=|i:usize|space.components.iter().position(|v|v.contains(&contacts[i].1)).unwrap();
  assert_ne!(island(0),island(4));
  let basis=volume_reduction::harmonic(&space,&contacts,b).unwrap();
  let field=volume_field::Field::new(&mesh,&space,b).unwrap();
  let groups:Vec<_>=space.cells.iter().filter_map(|c|{let t=&mesh.copper_tetrahedra[c.mesh_cell];(t.source_ids==["c0"]).then_some(t.planar_triangle)}).collect::<BTreeSet<_>>().into_iter().map(|t|Group{prism:Prism::new("c0".into(),triangles[t].map(|i|xy[i]),[0.,0.001]).unwrap(),cells:space.cells.iter().enumerate().filter_map(|(i,c)|(mesh.copper_tetrahedra[c.mesh_cell].planar_triangle==t).then_some(i)).collect()}).collect();
  let ports=[[0,4],[1,5],[2,4],[3,5]];
  let extracted=model(&mesh,&space,&basis,&groups,&field,&ports,12,b,Duration::from_secs(30)).unwrap();
  assert_eq!(extracted.reference_topology,"infinity");
  let nv=extracted.potential_nodes.len();let n=nv+extracted.current_dofs.len();
  let mut sys=coupled::System::compile(&extracted,4,None).unwrap();
  let rhs=sys.port_rhs(&[1.,0.,0.,0.]);
  assert!(sys.solve(0.,rhs.clone(),&vec![0.;n],&mut Counters::default()).unwrap_err().starts_with("COUPLED_DC_INCOMPATIBLE"));
  for hz in [1000.,10000.] {
   let w=std::f64::consts::TAU*hz;let mut ac_rhs=rhs.clone();ac_rhs.resize(2*n,0.);
   let x=sys.solve_charge_coordinates(w,&ac_rhs,&mut Counters::default()).unwrap();
   let voltage_re=sys.port_voltages(&x,4);let voltage_im=sys.port_voltages(&x[n..],4);
   assert!(voltage_im[0].abs()>1e-6 && voltage_re[0].is_finite());
   for (k,cols) in basis.component_columns.iter().enumerate() {
    let mut qr=0.;let mut qi=0.;
    for &(i,j,c) in &extracted.capacitance_f {if cols.contains(&i) {qr+=c*x[j];qi+=c*x[n+j];}}
    let injection=if k==island(0){1.}else if k==island(4){-1.}else{0.};
    let defect=(-w*qi-injection).hypot(w*qr);
    assert!(defect<=1e-8*(1.+injection.abs()),"island={k} frequency={hz} defect={defect:e}");
    if injection!=0. {assert!((w*qi).abs()>0.99);}
   }
  }
  assert!(started.elapsed()<Duration::from_secs(30));
 }
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
   if !matched {let mut injection=vec![0.;space.potential_count()];injection[contacts[2*drive].1]=1.;injection[contacts[2*drive+1].1]=-1.;injection[contacts[4].1]=-1.;injection[contacts[5].1]=1.;let started=Instant::now();let admission=section_return::solve_candidate(&basis,&space,&field,&magnetic,&mut z,w,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());assert!(started.elapsed()<Duration::from_secs(30));let reason=admission.unwrap_err();eprintln!("original-space refusal {hz} drive {drive}: {reason}");assert!(reason.starts_with("LOCAL_SECTION_RETURN_RESIDUAL:")||reason.starts_with("LOCAL_SECTION_RETURN_CHARGE:"));}
  }}}
  assert_eq!(matched_success,4);assert!(worst_matched<1e-3);
  eprintln!("matched max relative error {worst_matched:e}; candidate unadmitted by original-space KVL");

 }

 #[test]
 #[ignore = "bounded diagnostic: original-space basis enrichment"]
 fn physical_basis_enrichment_diagnostic() {
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
  let mut basis=volume_reduction::harmonic(&space,&contacts,b).unwrap();
  let field=volume_field::Field::new(&mesh,&space,b).unwrap();
  let magnetic=volume_integral_magnetic::Operator::new(&mesh,b,8).unwrap();
  let groups:Vec<_>=space.cells.iter().filter_map(|c|{let t=&mesh.copper_tetrahedra[c.mesh_cell];(t.source_ids==["c0"]).then_some(t.planar_triangle)}).collect::<BTreeSet<_>>().into_iter().map(|t|Group{prism:Prism::new("c0".into(),triangles[t].map(|i|xy[i]),[0.,0.001]).unwrap(),cells:space.cells.iter().enumerate().filter_map(|(i,c)|(mesh.copper_tetrahedra[c.mesh_cell].planar_triangle==t).then_some(i)).collect()}).collect();
  let ports=[[0,4],[1,5],[2,4],[3,5]];
  use super::super::volume_magnetic::MagneticApply;
  let fixed=basis.retained_dofs().to_vec();let drives=vec![vec![1.,-1.,0.,0.],vec![0.,0.,1.,-1.]];
  for round in 0..=3 {
   if round>0 {
    let bytes=space.current_count().checked_mul(basis.current.len()).and_then(|n|n.checked_mul(24)).and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(magnetic.scratch_bytes())).unwrap();b.check(bytes).unwrap();
    let start=Instant::now();let added=basis.extend_magnetic(&space,&magnetic).unwrap();eprintln!("magnetic round={round} added={added}");
    for hz in [1000.,10000.] {let result=section_return::enrich_kcl(&mut basis,&space,&field,&magnetic,&fixed,&ports,&drives,2.*std::f64::consts::PI*hz,1,0,b);eprintln!("KCL enrich round={round} hz={hz} result={result:?}");}assert!(start.elapsed()<Duration::from_secs(30));
   }
   let supports=physical_groups(&mesh,&space,&basis.current,&groups,b,Instant::now(),Duration::from_secs(30)).unwrap();
   for m in 0..basis.current.len() {let count=supports.iter().filter(|g|matches!(g.support(m).unwrap(),hybrid_section_magnetic::ModeSupport::Section(_, _))).count();eprintln!("round={round} mode={m} sections={count} local_groups={}",supports.len()-count);if m<3 {assert_eq!(count,2);}}
   let old=basis.model(&space,&field,&magnetic,&ports,b).unwrap();let n=old.potential_nodes.len()+old.current_dofs.len();let nv=basis.voltage.len();
   for hz in [1000.,10000.] {for drive in 0..2 {
    let mut sys=coupled::System::compile(&old,4,None).unwrap();let mut rhs=sys.port_rhs(&drives[drive]);rhs.resize(2*n,0.);let w=2.*std::f64::consts::PI*hz;
    let mut injection=vec![0.;space.potential_count()];injection[contacts[2*drive].1]=1.;injection[contacts[2*drive+1].1]=-1.;injection[contacts[4].1]=-1.;injection[contacts[5].1]=1.;
    let x=sys.solve_charge_coordinates(w,&rhs,&mut Counters::default()).unwrap();
    let v=|off:usize|->Vec<f64>{(0..space.potential_count()).map(|i|basis.voltage.iter().enumerate().map(|(k,q)|q[i]*x[off+k]).sum()).collect()};let qr=field.apply(&v(0)).unwrap();let qi=field.apply(&v(n)).unwrap();
    for (island,nodes) in space.components.iter().enumerate() {let cols=&basis.component_columns[island];let projected=|off:usize|old.capacitance_f.iter().filter(|e|cols.contains(&e.0)).map(|e|e.2*x[off+e.1]).sum::<f64>();let direct_r=nodes.iter().map(|&i|qr[i]).sum::<f64>();let direct_i=nodes.iter().map(|&i|qi[i]).sum::<f64>();eprintln!("charge reconstruction island={island} projected=({:e},{:e}) direct=({direct_r:e},{direct_i:e})",projected(0),projected(n));let sr:f64=nodes.iter().map(|&i|injection[i]).sum();let r:f64=nodes.iter().map(|&i|qr[i]).sum();let im=nodes.iter().map(|&i|qi[i]).sum::<f64>()+sr/w;let scale=nodes.iter().map(|&i|qr[i].abs()+qi[i].abs()).sum::<f64>()+sr.abs()/w;eprintln!("charge round={round} hz={hz} drive={drive} island={island} re={r:e} im={im:e} scale={scale:e} ratio={:e}",r.hypot(im)/scale.max(f64::MIN_POSITIVE));}
    let start=Instant::now();let result=section_return::solve_candidate(&basis,&space,&field,&magnetic,&mut sys,w,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());assert!(start.elapsed()<Duration::from_secs(30));eprintln!("original round={round} hz={hz} drive={drive} nv={nv} ni={} result={:?}",basis.current.len(),result.as_ref().err());if round==3 && drive==1 {assert!(result.is_ok());} if round==3 && drive==0 {assert!(result.unwrap_err().starts_with("LOCAL_SECTION_RETURN_CHARGE:"));}
   }}
  }
  // Compare finite-axial self convergence with the independent volume source;
  // neither discretized value is assumed to be an exact oracle.
  let prism=&groups[0].prism;let source=super::super::polyhedron::PointPotential::new(prism).unwrap();
  let reference=super::super::polyhedron::volume_integral(prism,&source,&super::super::prism::GaussRule::new(16).unwrap()).unwrap();
  for order in [12,32,60] {let start=Instant::now();let value=super::super::axial_magnetic::constant_pair(prism,prism,&super::super::prism::GaussRule::new(order).unwrap()).unwrap();eprintln!("section self order={order} value={value:e} volume16={reference:e} relative={:e} elapsed={:?}",(value-reference).abs()/reference,start.elapsed());assert!(start.elapsed()<Duration::from_secs(30));}
  let start=Instant::now();let fine=model(&mesh,&space,&basis,&groups,&field,&ports,60,b,Duration::from_secs(60));eprintln!("q60 grouped build elapsed={:?} error={:?}",start.elapsed(),fine.as_ref().err());
  let fine=fine.unwrap(); {let old=basis.model(&space,&field,&magnetic,&ports,b).unwrap();let n=old.potential_nodes.len()+old.current_dofs.len();let mut sys=coupled::System::compile(&fine,4,None).unwrap();let mut rhs=sys.port_rhs(&drives[1]);rhs.resize(2*n,0.);let mut injection=vec![0.;space.potential_count()];for (p,&a) in ports.iter().zip(&drives[1]) {injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}let result=section_return::solve_candidate(&basis,&space,&field,&magnetic,&mut sys,2.*std::f64::consts::PI*10000.,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());eprintln!("q60 original gate 10000 drive=1: {:?}",result.as_ref().err());let entry=|v:&Vec<(usize,usize,f64)>,i,j|v.iter().find(|e|e.0==i&&e.1==j).map_or(0.,|e|e.2);let mut diff=0f64;let mut scale=0f64;for i in 0..basis.current.len() {for j in 0..basis.current.len() {let a=entry(&old.inductance_h,i,j);let z=entry(&fine.inductance_h,i,j);diff=diff.max((a-z).abs());scale=scale.max(a.abs());}}eprintln!("q60 L max absolute={diff:e} relative={:e}",diff/scale);}
  let start=Instant::now();let hybrid=model(&mesh,&space,&basis,&groups,&field,&ports,12,b,Duration::from_secs(60));eprintln!("enriched hybrid build elapsed={:?} result={:?}",start.elapsed(),hybrid.as_ref().err());
  let hybrid=hybrid.unwrap();let mut matched_success=0; {let old=basis.model(&space,&field,&magnetic,&ports,b).unwrap();let lm=|entries:&Vec<(usize,usize,f64)>,i,j|entries.iter().find(|e|e.0==i&&e.1==j).map_or(0.,|e|e.2);let mut ld=0f64;let mut ls=0f64;for i in 0..basis.current.len(){for j in 0..basis.current.len(){let a=lm(&old.inductance_h,i,j);let z=lm(&hybrid.inductance_h,i,j);ld=ld.max((a-z).abs());ls=ls.max(a.abs());}}eprintln!("enriched projected L max absolute={ld:e} relative={:e}",ld/ls);let n=old.potential_nodes.len()+old.current_dofs.len();for hz in [1000.,10000.] {for drive in 0..2 {let w=2.*std::f64::consts::PI*hz;let mut o=coupled::System::compile(&old,4,None).unwrap();let mut h=coupled::System::compile(&hybrid,4,None).unwrap();let mut rhs=o.port_rhs(&drives[drive]);rhs.resize(2*n,0.);let mut injection=vec![0.;space.potential_count()];for (p,&a) in ports.iter().zip(&drives[drive]) {injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}let admitted=section_return::solve_candidate(&basis,&space,&field,&magnetic,&mut h,w,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());eprintln!("hybrid original gate {hz} drive={drive}: {:?}",admitted.as_ref().err());if hz==1000. && drive==1 {assert!(admitted.is_ok());}for p in 0..4 {o.add_port_admittance(p,0.02,0.).unwrap();h.add_port_admittance(p,0.02,0.).unwrap();}let a=o.solve_charge_coordinates(w,&rhs,&mut Counters::default());let z=h.solve_charge_coordinates(w,&rhs,&mut Counters::default());match (a,z) {(Ok(a),Ok(z))=>{matched_success+=1;let ar=o.port_voltages(&a,4);let ai=o.port_voltages(&a[n..],4);let br=h.port_voltages(&z,4);let bi=h.port_voltages(&z[n..],4);let err=(0..4).map(|i|(ar[i]-br[i]).hypot(ai[i]-bi[i])).fold(0.,f64::max);let scale=(0..4).map(|i|ar[i].hypot(ai[i])).fold(0.,f64::max);eprintln!("matched {hz} drive={drive} relative={:e}",err/scale);assert!(err/scale<1e-3);},(a,z)=>eprintln!("matched refusal {hz} drive={drive}: {:?} {:?}",a.err(),z.err())}}}}assert_eq!(matched_success,4);

 }

 #[test]
 #[ignore = "bounded full-trace charge representation diagnostic"]
 fn dense_trace_admission_diagnostic() {
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
  let mut basis=volume_reduction::harmonic(&space,&contacts,b).unwrap();
  let field=volume_field::Field::new(&mesh,&space,b).unwrap();
  let magnetic=volume_integral_magnetic::Operator::new(&mesh,b,8).unwrap();
  let groups:Vec<_>=space.cells.iter().filter_map(|c|{let t=&mesh.copper_tetrahedra[c.mesh_cell];(t.source_ids==["c0"]).then_some(t.planar_triangle)}).collect::<BTreeSet<_>>().into_iter().map(|t|Group{prism:Prism::new("c0".into(),triangles[t].map(|i|xy[i]),[0.,0.001]).unwrap(),cells:space.cells.iter().enumerate().filter_map(|(i,c)|(mesh.copper_tetrahedra[c.mesh_cell].planar_triangle==t).then_some(i)).collect()}).collect();
  use super::super::volume_field::ElectricApply;
  use super::super::volume_magnetic::MagneticApply;
  let ports=[[0,4],[1,5],[2,4],[3,5]];let fixed=basis.retained_dofs().to_vec();let drives=vec![vec![1.,-1.,0.,0.],vec![0.,0.,1.,-1.]];
  for round in 0..3 {let bytes=space.current_count().checked_mul(basis.current.len()).and_then(|n|n.checked_mul(24)).and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(magnetic.scratch_bytes())).unwrap();b.check(bytes).unwrap();basis.extend_magnetic(&space,&magnetic).unwrap();for hz in [1000.,10000.] {section_return::enrich_kcl(&mut basis,&space,&field,&magnetic,&fixed,&ports,&drives,2.*std::f64::consts::PI*hz,1,0,b).unwrap();}eprintln!("round={round} currents={}",basis.current.len());}
  let started=Instant::now();let dense=volume_field::DenseField::new(&field,b).unwrap();eprintln!("full-trace field assembly elapsed={:?} charge-energy={} reciprocity={} full_potentials={} matrix_bytes~{}",started.elapsed(),dense.max_charge_energy_column_error,dense.max_directed_reciprocity,space.potential_count(),8*space.potential_count().pow(2));assert!(started.elapsed()<Duration::from_secs(30));
  let supports=physical_groups(&mesh,&space,&basis.current,&groups,b,Instant::now(),Duration::from_secs(30)).unwrap();for m in 0..basis.current.len() {let count=supports.iter().filter(|g|matches!(g.support(m).unwrap(),hybrid_section_magnetic::ModeSupport::Section(_, _))).count();eprintln!("mode={m} section_groups={count} local_groups={}",supports.len()-count);}
  for (label,f) in [("raw",&field as &dyn ElectricApply),("dense",&dense as &dyn ElectricApply)] {
   let old=basis.model(&space,f,&magnetic,&ports,b).unwrap();let n=old.potential_nodes.len()+old.current_dofs.len();
   for hz in [1000.,10000.] {for drive in 0..2 {let mut sys=coupled::System::compile(&old,4,None).unwrap();let mut rhs=sys.port_rhs(&drives[drive]);rhs.resize(2*n,0.);let mut injection=vec![0.;space.potential_count()];for (p,&a) in ports.iter().zip(&drives[drive]) {injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}let w=2.*std::f64::consts::PI*hz;
    let x=sys.solve_charge_coordinates(w,&rhs,&mut Counters::default()).unwrap();let v:Vec<_>=(0..space.potential_count()).map(|i|basis.voltage.iter().enumerate().map(|(k,q)|q[i]*x[k]).sum()).collect();
    if label=="dense" && drive==0 {let mut perturbed=v.clone();for &i in &space.components[1] {perturbed[i]+=1e-6;}let q=f.apply(&perturbed).unwrap();let total=space.components[1].iter().map(|&i|q[i]).sum::<f64>();let scale=space.components[1].iter().map(|&i|q[i].abs()).sum::<f64>();eprintln!("deliberate passive island constant +1e-6 V: Q={total:e} scale={scale:e}");assert!(total.abs()>1e-8*scale.max(f64::MIN_POSITIVE));}
    let start=Instant::now();let result=section_return::solve_candidate(&basis,&space,f,&magnetic,&mut sys,w,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());assert!(start.elapsed()<Duration::from_secs(30));eprintln!("field={label} original gate hz={hz} drive={drive} result={:?}",result.as_ref().err());if label=="raw" && drive==0 {assert!(result.unwrap_err().starts_with("LOCAL_SECTION_RETURN_CHARGE:"));}
   }}
  }
  let start=Instant::now();let hybrid=model(&mesh,&space,&basis,&groups,&dense,&ports,60,b,Duration::from_secs(60));eprintln!("dense q60 hybrid build elapsed={:?} error={:?}",start.elapsed(),hybrid.as_ref().err());
  if let Ok(hybrid)=hybrid {let n=hybrid.potential_nodes.len()+hybrid.current_dofs.len();for hz in [1000.,10000.] {for drive in 0..2 {let mut sys=coupled::System::compile(&hybrid,4,None).unwrap();let mut rhs=sys.port_rhs(&drives[drive]);rhs.resize(2*n,0.);let mut injection=vec![0.;space.potential_count()];for (p,&a) in ports.iter().zip(&drives[drive]) {injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}let start=Instant::now();let result=section_return::solve_candidate(&basis,&space,&dense,&magnetic,&mut sys,2.*std::f64::consts::PI*hz,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default());assert!(start.elapsed()<Duration::from_secs(30));eprintln!("dense q60 original gate hz={hz} drive={drive}: {:?}",result.as_ref().err());}}}
 }

 #[test]
 #[ignore = "physical island-constant refinement experiment"]
 fn island_constant_refinement_diagnostic() {
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
  let mut basis=volume_reduction::harmonic(&space,&contacts,b).unwrap();
  let field=volume_field::Field::new(&mesh,&space,b).unwrap();
  let magnetic=volume_integral_magnetic::Operator::new(&mesh,b,8).unwrap();
  let groups:Vec<_>=space.cells.iter().filter_map(|c|{let t=&mesh.copper_tetrahedra[c.mesh_cell];(t.source_ids==["c0"]).then_some(t.planar_triangle)}).collect::<BTreeSet<_>>().into_iter().map(|t|Group{prism:Prism::new("c0".into(),triangles[t].map(|i|xy[i]),[0.,0.001]).unwrap(),cells:space.cells.iter().enumerate().filter_map(|(i,c)|(mesh.copper_tetrahedra[c.mesh_cell].planar_triangle==t).then_some(i)).collect()}).collect();
  use super::super::volume_magnetic::MagneticApply;
  let ports=[[0,4],[1,5],[2,4],[3,5]];let fixed=basis.retained_dofs().to_vec();let drives=vec![vec![1.,-1.,0.,0.],vec![0.,0.,1.,-1.]];
  for round in 0..3 {let bytes=space.current_count().checked_mul(basis.current.len()).and_then(|n|n.checked_mul(24)).and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(magnetic.scratch_bytes())).unwrap();b.check(bytes).unwrap();basis.extend_magnetic(&space,&magnetic).unwrap();for hz in [1000.,10000.] {section_return::enrich_kcl(&mut basis,&space,&field,&magnetic,&fixed,&ports,&drives,2.*std::f64::consts::PI*hz,1,0,b).unwrap();}eprintln!("round={round} currents={}",basis.current.len());}
  let old=basis.model(&space,&field,&magnetic,&ports,b).unwrap();let nv=basis.voltage.len();let n=nv+basis.current.len();let nc=space.components.len();assert_eq!(nc,3);
  let mut jac=vec![vec![0.;nc];nc];for j in 0..nc {let mut indicator=vec![0.;space.potential_count()];for &i in &space.components[j] {indicator[i]=1.;}let q=field.apply(&indicator).unwrap();for i in 0..nc {jac[i][j]=space.components[i].iter().map(|&k|q[k]).sum();}}
  let solve=|rhs:&[f64]| {let mut a=jac.clone();let mut y=rhs.to_vec();for k in 0..nc {let p=(k..nc).max_by(|&i,&j|a[i][k].abs().total_cmp(&a[j][k].abs())).unwrap();a.swap(k,p);y.swap(k,p);assert!(a[k][k]!=0.);for i in k+1..nc {let t=a[i][k]/a[k][k];for j in k..nc {a[i][j]-=t*a[k][j];}y[i]-=t*y[k];}}let mut x=vec![0.;nc];for i in (0..nc).rev(){x[i]=(y[i]-(i+1..nc).map(|j|a[i][j]*x[j]).sum::<f64>())/a[i][i];}x};
  for hz in [1000.,10000.] {for drive in 0..2 {let w=2.*std::f64::consts::PI*hz;let mut system=coupled::System::compile(&old,4,None).unwrap();let mut rhs=system.port_rhs(&drives[drive]);rhs.resize(2*n,0.);let mut injection=vec![0.;space.potential_count()];for (p,&a) in ports.iter().zip(&drives[drive]){injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}let start=Instant::now();let (mut x,shape)=system.solve_charge_coordinates_split(w,&rhs,&mut Counters::default()).unwrap();let before=x.clone();
   let potential=|x:&Vec<f64>,off:usize|->Vec<f64>{(0..space.potential_count()).map(|i|basis.voltage.iter().enumerate().map(|(k,q)|q[i]*x[off+k]).sum()).collect()};
   let mut previous=f64::INFINITY;let mut converged=false;
   for iteration in 0..32 {let vr=potential(&x,0);let vi=potential(&x,n);let qr=field.apply(&vr).unwrap();let qi=field.apply(&vi).unwrap();let mut rr=vec![0.;nc];let mut ri=vec![0.;nc];let mut worst=0f64;
    for (c,nodes) in space.components.iter().enumerate(){rr[c]=-nodes.iter().map(|&i|qr[i]).sum::<f64>();ri[c]=-nodes.iter().map(|&i|qi[i]).sum::<f64>()-nodes.iter().map(|&i|injection[i]).sum::<f64>()/w;let scale=nodes.iter().map(|&i|qr[i].abs()+qi[i].abs()).sum::<f64>()+nodes.iter().map(|&i|injection[i]).sum::<f64>().abs()/w;let ratio=rr[c].hypot(ri[c])/scale.max(f64::MIN_POSITIVE);worst=worst.max(ratio);eprintln!("constant refine hz={hz} drive={drive} iter={iteration} island={c} residual=({:e},{:e}) scale={scale:e} ratio={ratio:e}",rr[c],ri[c]);}
    if worst<=1e-8 {converged=true;break;}if iteration>=8 && worst>=previous {eprintln!("constant refinement stagnated");break;}previous=worst;let dr=solve(&rr);let di=solve(&ri);let old=x.clone();for c in 0..nc {for &j in &basis.component_columns[c] {x[j]+=dr[c];x[n+j]+=di[c];}}if x==old {eprintln!("constant refinement correction below representable state");break;}
   }
   let vr=potential(&x,0);let vi=potential(&x,n);let qr=field.apply(&vr).unwrap();let qi=field.apply(&vi).unwrap();let current=|off:usize|->Vec<f64>{(0..space.current_count()).map(|i|basis.current.iter().enumerate().map(|(k,q)|q[i]*x[off+nv+k]).sum()).collect()};let ir=current(0);let ii=current(n);let dr=space.divergence(&ir).unwrap();let di=space.divergence(&ii).unwrap();let mut gr=vec![0.;space.current_count()];let mut gi=gr.clone();for cols in &basis.component_columns {let last=*cols.last().unwrap();for &a in &cols[..cols.len()-1]{let g=space.gradient(&basis.voltage[a]).unwrap();for j in 0..gr.len(){gr[j]+=(shape[a]-shape[last])*g[j];gi[j]+=(shape[n+a]-shape[n+last])*g[j];}}}let lr=magnetic.apply(&space,&ir).unwrap();let li=magnetic.apply(&space,&ii).unwrap();let mut kvl=0f64;let mut scale=0f64;for j in 0..ir.len(){let r=space.cells[j/3].resistance_ohm;kvl=kvl.max((r*ir[j]-gr[j]-w*li[j]).hypot(r*ii[j]-gi[j]+w*lr[j]));scale=scale.max(r*ir[j].hypot(ii[j])+gr[j].hypot(gi[j])+w*lr[j].hypot(li[j]));}let kcl=(0..dr.len()).map(|j|(dr[j]-w*qi[j]-injection[j]).hypot(di[j]+w*qr[j])).fold(0.,f64::max);
   let pr=system.port_voltages(&x,4);let pi=system.port_voltages(&x[n..],4);let ar=system.port_voltages(&before,4);let ai=system.port_voltages(&before[n..],4);let diff=(0..4).map(|i|(pr[i]-ar[i]).hypot(pi[i]-ai[i])).fold(0.,f64::max);let mag=(0..4).map(|i|ar[i].hypot(ai[i])).fold(0.,f64::max);
   // This independently projected field residual is diagnostic; it is not the
   // coupled solver's assembled-matrix residual or its row normalization.
   let mut algebraic=0f64;let mut projected_relative=0f64;for v in &basis.voltage {let r=(0..dr.len()).map(|j|v[j]*(dr[j]-w*qi[j]-injection[j])).sum::<f64>();let i=(0..di.len()).map(|j|v[j]*(di[j]+w*qr[j])).sum::<f64>();algebraic=algebraic.max(r.hypot(i));let scale=(0..dr.len()).map(|j|v[j].abs()*(dr[j].abs()+w*qi[j].abs()+injection[j].abs()+di[j].abs()+w*qr[j].abs())).sum::<f64>();projected_relative=projected_relative.max(r.hypot(i)/scale.max(1e-15));}for q in &basis.current {let r=(0..ir.len()).map(|j|q[j]*(space.cells[j/3].resistance_ohm*ir[j]-gr[j]-w*li[j])).sum::<f64>();let i=(0..ir.len()).map(|j|q[j]*(space.cells[j/3].resistance_ohm*ii[j]-gi[j]+w*lr[j])).sum::<f64>();algebraic=algebraic.max(r.hypot(i));let scale=(0..ir.len()).map(|j|q[j].abs()*(space.cells[j/3].resistance_ohm*(ir[j].abs()+ii[j].abs())+gr[j].abs()+gi[j].abs()+w*(lr[j].abs()+li[j].abs()))).sum::<f64>();projected_relative=projected_relative.max(r.hypot(i)/scale.max(1e-15));}
   eprintln!("refine result hz={hz} drive={drive} converged={converged} KCL={kcl:e} KVL={kvl:e} kvl_scale={scale:e} projected_residual={algebraic:e} projected_relative={projected_relative:e} port_relative={:e} elapsed={:?}",diff/mag,start.elapsed());assert!(start.elapsed()<Duration::from_secs(30));
   if converged {assert!(kcl<=1e-8 && kvl<=1e-8*scale);}
  }}
 }

 #[test]
 #[ignore = "compensated island potential representation experiment"]
 fn compensated_island_refinement_diagnostic() {
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
  let mut basis=volume_reduction::harmonic(&space,&contacts,b).unwrap();
  let field=volume_field::Field::new(&mesh,&space,b).unwrap();
  let magnetic=volume_integral_magnetic::Operator::new(&mesh,b,16).unwrap();let magnetic8=volume_integral_magnetic::Operator::new(&mesh,b,8).unwrap();
  let groups:Vec<_>=space.cells.iter().map(|c|mesh.copper_tetrahedra[c.mesh_cell].planar_triangle).collect::<BTreeSet<_>>().into_iter().map(|t|{let indices:Vec<_>=space.cells.iter().enumerate().filter_map(|(i,c)|(mesh.copper_tetrahedra[c.mesh_cell].planar_triangle==t).then_some(i)).collect();let owner=mesh.copper_tetrahedra[space.cells[indices[0]].mesh_cell].source_ids[0].clone();Group{prism:Prism::new(owner,triangles[t].map(|i|xy[i]),[0.,0.001]).unwrap(),cells:indices}}).collect();assert_eq!(groups.len(),6);assert_eq!(groups.iter().map(|g|g.prism.source_id.as_str()).collect::<BTreeSet<_>>().len(),3);
  use super::super::volume_field::ElectricApply;
  use super::super::volume_magnetic::MagneticApply;
  let ports=[[0,4],[1,5],[2,4],[3,5]];let fixed=basis.retained_dofs().to_vec();let drives=vec![vec![1.,-1.,0.,0.],vec![0.,0.,1.,-1.]];
  for round in 0..3 {let bytes=space.current_count().checked_mul(basis.current.len()).and_then(|n|n.checked_mul(24)).and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(magnetic.scratch_bytes())).unwrap();b.check(bytes).unwrap();basis.extend_magnetic(&space,&magnetic).unwrap();for hz in [1000.,10000.] {section_return::enrich_kcl(&mut basis,&space,&field,&magnetic,&fixed,&ports,&drives,2.*std::f64::consts::PI*hz,1,0,b).unwrap();}eprintln!("round={round} currents={}",basis.current.len());}
  use super::super::accurate::Sum;
  let old=basis.model(&space,&field,&magnetic,&ports,b).unwrap();let old8=basis.model(&space,&field,&magnetic8,&ports,b).unwrap();let entry=|v:&Vec<(usize,usize,f64)>,i,j|v.iter().find(|e|e.0==i&&e.1==j).map_or(0.,|e|e.2);let mut ldiff=0f64;let mut lscale=0f64;for i in 0..basis.current.len(){for j in 0..basis.current.len(){let x=entry(&old.inductance_h,i,j);ldiff=ldiff.max((x-entry(&old8.inductance_h,i,j)).abs());lscale=lscale.max(x.abs());}}eprintln!("independent volume order8 vs16 L max absolute={ldiff:e} relative={:e}",ldiff/lscale);let nv=basis.voltage.len();let n=nv+basis.current.len();let nc=space.components.len();assert_eq!(nc,3);
  let response=super::super::island_response::IslandResponse::new(&space,&field,b,Instant::now(),Duration::from_secs(30)).unwrap();
  assert!(super::super::island_response::IslandResponse::new(&space,&field,AllocationBudget{max_bytes:1,..b},Instant::now(),Duration::from_secs(30)).is_err());assert!(super::super::island_response::IslandResponse::new(&space,&field,b,Instant::now(),Duration::ZERO).is_err());
  let malformed=super::super::volume_current::Space{mesh_nodes:vec![vec![];space.potential_count()],node_potential:Default::default(),cells:vec![],components:vec![vec![0,0]],planned_bytes:0};assert!(super::super::island_response::IslandResponse::new(&malformed,&field,b,Instant::now(),Duration::from_secs(30)).is_err());
  let physical=physical_groups(&mesh,&space,&basis.current,&groups,b,Instant::now(),Duration::from_secs(30)).unwrap();assert_eq!(physical.len(),6);
  for mode in 0..3 {assert_eq!(physical.iter().filter(|g|matches!(g.support(mode).unwrap(),hybrid_section_magnetic::ModeSupport::Section(_, _))).count(),6);}
  let (mut raw,counts)=hybrid_section_magnetic::grouped_gram_counted(&physical,basis.current.len(),60,b,Duration::from_secs(60)).unwrap();eprintln!("physical grouped kernel misses far={} local={} mixed={}",counts.far_section_pairs,counts.local_section_tetra_pairs,counts.mixed_tetra_pairs);assert!(counts.far_section_pairs>0 && counts.local_section_tetra_pairs>0 && counts.mixed_tetra_pairs>0);scale_henries(&mut raw);let invalid_ports=[[0,fixed.len()]];assert!(section_return::solve_source_candidate(&basis,&space,&field,&magnetic,&raw,&invalid_ports,&[1.],&[0.],1000.,b,Duration::from_secs(30),&mut Counters::default()).is_err());assert!(section_return::solve_source_candidate(&basis,&space,&field,&magnetic,&raw,&ports,&drives[0],&[0.;4],1000.,AllocationBudget{max_bytes:1,..b},Duration::from_secs(30),&mut Counters::default()).is_err());assert!(section_return::solve_source_candidate(&basis,&space,&field,&magnetic,&raw,&ports,&drives[0],&[0.;4],1000.,b,Duration::ZERO,&mut Counters::default()).is_err());let hybrid=basis.model_with_inductance(&space,&field,&raw,&ports,b).unwrap();assert_eq!(old.incidence,hybrid.incidence);assert_eq!(old.resistance_ohm,hybrid.resistance_ohm);assert_eq!(old.capacitance_f,hybrid.capacitance_f);assert_eq!(old.port_incidence,hybrid.port_incidence);assert_eq!(old.conserved_potential_modes,hybrid.conserved_potential_modes);
  for hz in [1000.,10000.] {for drive in 0..2 {let start=Instant::now();let w=2.*std::f64::consts::PI*hz;let mut system=coupled::System::compile(&old,4,None).unwrap();let mut rhs=system.port_rhs(&drives[drive]);rhs.resize(2*n,0.);let mut injection=vec![0.;space.potential_count()];for (p,&a) in ports.iter().zip(&drives[drive]) {injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}let (x,shape)=system.solve_charge_coordinates_split(w,&rhs,&mut Counters::default()).unwrap();
   let result8=section_return::solve_source_candidate(&basis,&space,&field,&magnetic8,&raw,&ports,&drives[drive],&[0.;4],w,b,Duration::from_secs(30),&mut Counters::default());eprintln!("source candidate order8 hz={hz} drive={drive}: {:?}",result8.as_ref().err());if hz==1000. {assert!(result8.is_ok());}else{assert!(result8.err().unwrap().contains("SECTION_RETURN_RESIDUAL"));}
   let candidate=section_return::solve_source_candidate(&basis,&space,&field,&magnetic,&raw,&ports,&drives[drive],&[0.;4],w,b,Duration::from_secs(30),&mut Counters::default()).unwrap();
   let ar=system.port_voltages(&x,4);let ai=system.port_voltages(&x[n..],4);let mut err=0f64;let mut scale=0f64;for p in 0..4 {let z=candidate.port_voltage(p).unwrap();let [pa,pb]=ports[p];for (part,off) in [0,n].into_iter().enumerate(){let expected=Sum::from(candidate.high[off+pa]).add(candidate.correction[off+pa]).sub(Sum::from(candidate.high[off+pb]).add(candidate.correction[off+pb]));assert_eq!(z[part].hi,expected.hi);assert_eq!(z[part].lo,expected.lo);}err=err.max((ar[p]-z[0].value()).hypot(ai[p]-z[1].value()));scale=scale.max(ar[p].hypot(ai[p]));}eprintln!("source candidate all gates admitted hz={hz} drive={drive} port_relative={:e}",err/scale);assert!(err/scale<1e-3);
   let audit=coupled::System::compile(&hybrid,4,None).unwrap();assert!(audit.audit_compensated_state(w,&rhs,&candidate.high,&candidate.correction).is_ok());let mut bad_correction=candidate.correction.clone();bad_correction[0]=bad_correction[0].add(Sum::from(1.));assert!(audit.audit_compensated_state(w,&rhs,&candidate.high,&bad_correction).is_err());let mut loaded=coupled::System::compile(&hybrid,4,None).unwrap();loaded.add_port_admittance(2*drive,1.,0.).unwrap();assert!(loaded.audit_compensated_state(w,&rhs,&candidate.high,&candidate.correction).is_err());
   let potential=|off:usize|->Vec<f64>{(0..space.potential_count()).map(|i|basis.voltage.iter().enumerate().map(|(k,q)|q[i]*x[off+k]).sum()).collect()};let vr=potential(0);let vi=potential(n);let base_r=field.apply(&vr).unwrap();
   if drive==0 {let mut bad=vec![Sum::default();nc];bad[1]=Sum::from(1e-6);let q=response.compose(&base_r,&bad,Instant::now(),Duration::from_secs(30)).unwrap();let total=space.components[1].iter().fold(Sum::default(),|a,&i|a.add(q[i])).value();let scale=space.components[1].iter().map(|&i|q[i].value().abs()).sum::<f64>();eprintln!("perturbed hz={hz} charge={total:e} scale={scale:e}");assert!(total.abs()>1e-8*scale.max(f64::MIN_POSITIVE));}
   let refined=response.refine(&vr,&vi,&injection,&vec![0.;injection.len()],w,b,Instant::now(),Duration::from_secs(30)).unwrap();let delta_r=refined.real_constants;let delta_i=refined.imaginary_constants;let converged=true;eprintln!("helper iterations={} worst={:e}",refined.iterations,refined.worst_ratio);
   let qr=refined.real_charge;let qi=refined.imaginary_charge;let mut rounded_r=vr.clone();let mut rounded_i=vi.clone();for c in 0..nc {for &j in &space.components[c] {rounded_r[j]+=delta_r[c].value();rounded_i[j]+=delta_i[c].value();}}let direct_r=field.apply(&rounded_r).unwrap();let direct_i=field.apply(&rounded_i).unwrap();let difference=(0..qr.len()).map(|i|(qr[i].value()-direct_r[i]).hypot(qi[i].value()-direct_i[i])).fold(0.,f64::max);
   let current=|off:usize|->Vec<f64>{(0..space.current_count()).map(|i|basis.current.iter().enumerate().map(|(k,q)|q[i]*x[off+nv+k]).sum()).collect()};let ir=current(0);let ii=current(n);let dr=space.divergence(&ir).unwrap();let di=space.divergence(&ii).unwrap();let kcl=(0..dr.len()).map(|j|(dr[j]-w*qi[j].value()-injection[j]).hypot(di[j]+w*qr[j].value())).fold(0.,f64::max);let mut gr=vec![0.;space.current_count()];let mut gi=gr.clone();for cols in &basis.component_columns {let last=*cols.last().unwrap();for &a in &cols[..cols.len()-1] {let g=space.gradient(&basis.voltage[a]).unwrap();for j in 0..gr.len(){gr[j]+=(shape[a]-shape[last])*g[j];gi[j]+=(shape[n+a]-shape[n+last])*g[j];}}}let lr=magnetic.apply(&space,&ir).unwrap();let li=magnetic.apply(&space,&ii).unwrap();let mut kvl=0f64;let mut scale=0f64;for j in 0..ir.len(){let r=space.cells[j/3].resistance_ohm;kvl=kvl.max((r*ir[j]-gr[j]-w*li[j]).hypot(r*ii[j]-gi[j]+w*lr[j]));scale=scale.max(r*ir[j].hypot(ii[j])+gr[j].hypot(gi[j])+w*lr[j].hypot(li[j]));}let max_delta=(0..nc).map(|c|delta_r[c].value().hypot(delta_i[c].value())).fold(0.,f64::max);let port_delta=ports.iter().map(|p|{let a=basis.component_columns.iter().position(|c|c.contains(&p[0])).unwrap();let b=basis.component_columns.iter().position(|c|c.contains(&p[1])).unwrap();assert!(space.components[a].contains(&fixed[p[0]]) && space.components[b].contains(&fixed[p[1]]));delta_r[a].sub(delta_r[b]).value().hypot(delta_i[a].sub(delta_i[b]).value())}).fold(0.,f64::max);let ports_r=system.port_voltages(&x,4);let ports_i=system.port_voltages(&x[n..],4);let port_scale=(0..4).map(|i|ports_r[i].hypot(ports_i[i])).fold(0.,f64::max);eprintln!("composition outcome hz={hz} drive={drive} converged={converged} direct_delta_q={difference:e} KCL={kcl:e} KVL={kvl:e} scale={scale:e} max_island_delta_V={max_delta:e} max_port_delta_V={port_delta:e} port_delta_relative={:e} elapsed={:?}",port_delta/port_scale.max(f64::MIN_POSITIVE),start.elapsed());assert!(start.elapsed()<Duration::from_secs(30));assert!(converged,"compensated island charge did not converge");assert!(kcl<=1e-8 && kvl<=1e-8*scale);
  }}
 }

}



#[cfg(test)]
#[path = "hybrid_five_island_test.rs"]
mod hybrid_five_island_test;
