//! Conforming section-current lift, not an admitted shared-return model.
//! Profiles include their physical reference copper. Several columns may use
//! the same return cells; those cells and common R/L/C operators exist once.
//! No section impedance or field-energy stamp is added here.
#![allow(dead_code)] // Private mapping control; no production dispatch yet.
use super::{error, mesh::LayeredMesh, volume_current::Space, AllocationBudget, Result};
use crate::extraction::section_transition::{Mesh, Recovery};
use std::collections::BTreeMap;

/// Candidate only: retain individual cut potentials and enrich physical currents.
/// Admission still requires the original-space residual at each requested drive.
pub(in crate::extraction) fn candidate(
    space:&Space, endpoints:&[(String,usize)], cut_dofs:&[usize], profiles:&[Vec<f64>],
    magnetic:&dyn super::volume_magnetic::MagneticApply, rounds:usize, budget:AllocationBudget,
) -> Result<super::volume_reduction::Basis> {
    if rounds>3 {return Err(error("SECTION_RETURN_MAP","at most three magnetic enrichment rounds"));}
    let mut contacts=endpoints.to_vec();
    for &dof in cut_dofs {
        if dof>=space.potential_count() {return Err(error("SECTION_RETURN_MAP","unknown cut potential"));}
        if !contacts.iter().any(|(_,i)|*i==dof) {contacts.push((format!("cut-{dof}"),dof));}
    }
    let mut basis=super::volume_reduction::harmonic(space,&contacts,budget)?;
    basis.include_current_profiles(space,profiles,budget)?;
    for _ in 0..rounds {
        let bytes=space.current_count().checked_mul(basis.current.len()).and_then(|n|n.checked_mul(24)).and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(magnetic.scratch_bytes())).ok_or_else(||error("MEMORY_LIMIT","magnetic enrichment plan overflow"))?;
        budget.check(bytes)?;
        basis.extend_magnetic(space,magnetic)?;
    }
    Ok(basis)
}

/// Solve in charge coordinates and verify each island's physical charge balance.
/// Verify original-space KCL/KVL as well as island charge before returning.
pub(in crate::extraction) fn solve_candidate(
    basis:&super::volume_reduction::Basis, space:&Space, field:&dyn super::volume_field::ElectricApply,
    magnetic:&dyn super::volume_magnetic::MagneticApply,
    system:&mut crate::electrical::coupled::System, omega:f64, rhs:&[f64],
    real_injection:&[f64], imaginary_injection:&[f64], stats:&mut crate::electrical::Counters,
) -> Result<Vec<f64>> {
    let nv=basis.voltage.len();let n=nv+basis.current.len();
    if omega<=0.||!omega.is_finite()||rhs.len()!=2*n||real_injection.len()!=space.potential_count()||imaginary_injection.len()!=space.potential_count()||rhs.iter().chain(real_injection).chain(imaginary_injection).any(|v|!v.is_finite()) {return Err(error("SECTION_RETURN_MAP","invalid candidate solve dimensions or values"));}
    let (x,shape)=system.solve_charge_coordinates_split(omega,rhs,stats)?;
    if x.len()!=2*n||x.iter().any(|v|!v.is_finite()) {return Err(error("SECTION_RETURN_RESIDUAL","nonfinite candidate solution"));}
    let potential=|offset:usize|->Vec<f64> {(0..space.potential_count()).map(|j|basis.voltage.iter().enumerate().map(|(k,v)|v[j]*x[offset+k]).sum()).collect()};
    let vr=potential(0);let vi=potential(n);let qr=field.apply(&vr)?;let qi=field.apply(&vi)?;
    for nodes in &space.components {
        let sr=nodes.iter().map(|&i|real_injection[i]).sum::<f64>();let si=nodes.iter().map(|&i|imaginary_injection[i]).sum::<f64>();
        let r=nodes.iter().map(|&i|qr[i]).sum::<f64>()-si/omega;
        let im=nodes.iter().map(|&i|qi[i]).sum::<f64>()+sr/omega;
        let scale=nodes.iter().map(|&i|qr[i].abs()+qi[i].abs()).sum::<f64>()+(sr.abs()+si.abs())/omega;
        if !r.is_finite()||!im.is_finite()||!scale.is_finite()||r.hypot(im)>1e-8*scale.max(f64::MIN_POSITIVE) {return Err(error("SECTION_RETURN_CHARGE","island charge balance unresolved"));}
    }
    audit_original(basis,space,magnetic,&x,&shape,&qr,&qi,None,omega,real_injection,imaginary_injection)?;
    Ok(x)
}

fn audit_original(basis:&super::volume_reduction::Basis,space:&Space,magnetic:&dyn super::volume_magnetic::MagneticApply,x:&[f64],shape:&[f64],qr:&[f64],qi:&[f64],precise:Option<(&[super::accurate::Sum],&[super::accurate::Sum])>,omega:f64,real_injection:&[f64],imaginary_injection:&[f64])->Result<()> {
    let nv=basis.voltage.len();let n=nv+basis.current.len();
    let current=|offset:usize|->Vec<f64> {(0..space.current_count()).map(|j|basis.current.iter().enumerate().map(|(k,v)|v[j]*x[offset+nv+k]).sum()).collect()};
    let ir=current(0);let ii=current(n);let dr=space.divergence(&ir)?;let di=space.divergence(&ii)?;
    // Differentiate the island partition using coefficient differences: large
    // capacitive island constants must cancel before floating-point gradients.
    let mut gr=vec![0.;space.current_count()];let mut gi=gr.clone();
    for columns in &basis.component_columns {let last=*columns.last().unwrap();for &a in &columns[..columns.len()-1] {
        let gradient=space.gradient(&basis.voltage[a])?;let cr=shape[a]-shape[last];let ci=shape[n+a]-shape[n+last];
        for j in 0..gr.len() {gr[j]+=cr*gradient[j];gi[j]+=ci*gradient[j];}
    }}
    let lr=magnetic.apply(space,&ir)?;let li=magnetic.apply(space,&ii)?;
    let source=real_injection.iter().chain(imaginary_injection).map(|v|v.abs()).fold(0.,f64::max);
    let kcl=if let Some((qrd,qid))=precise {if qrd.len()!=dr.len()||qid.len()!=dr.len(){return Err(error("SECTION_RETURN_MAP","invalid compensated charges"));}let mut maximum=0f64;for j in 0..dr.len(){let r=super::accurate::Sum::from(dr[j]).sub(qid[j].mul(omega)).sub(super::accurate::Sum::from(real_injection[j])).value();let i=super::accurate::Sum::from(di[j]).add(qrd[j].mul(omega)).sub(super::accurate::Sum::from(imaginary_injection[j])).value();let value=r.hypot(i);if !value.is_finite(){return Err(error("SECTION_RETURN_RESIDUAL","nonfinite compensated KCL"));}maximum=maximum.max(value);}maximum}else{(0..dr.len()).map(|j|(dr[j]-omega*qi[j]-real_injection[j]).hypot(di[j]+omega*qr[j]-imaginary_injection[j])).fold(0.,f64::max)};
    let mut kvl:f64=0.;let mut scale:f64=0.;
    for j in 0..ir.len() {let r=space.cells[j/3].resistance_ohm;
        kvl=kvl.max((r*ir[j]-gr[j]-omega*li[j]).hypot(r*ii[j]-gi[j]+omega*lr[j]));
        scale=scale.max(r*ir[j].hypot(ii[j])+gr[j].hypot(gi[j])+omega*lr[j].hypot(li[j]));
    }
    if !kcl.is_finite()||!kvl.is_finite()||!scale.is_finite()||kcl>1e-8*source.max(f64::MIN_POSITIVE)||kvl>1e-8*scale.max(f64::MIN_POSITIVE) {return Err(error("SECTION_RETURN_RESIDUAL",&format!("original KCL={kcl:e} KVL={kvl:e} scale={scale:e}")));}
    Ok(())
}

/// Source-only compensated state. Island corrections remain separate from the
/// f64 solver coordinates; port projections retain their low parts.
pub(in crate::extraction) struct CandidateState {
    pub high:Vec<f64>,pub correction:Vec<super::accurate::Sum>,
    pub real_charge:Vec<super::accurate::Sum>,pub imaginary_charge:Vec<super::accurate::Sum>,
    ports:Vec<[usize;2]>,n:usize,
}
impl CandidateState {
    pub fn port_voltage(&self,port:usize)->Result<[super::accurate::Sum;2]> {
        let [a,b]=*self.ports.get(port).ok_or_else(||error("SECTION_RETURN_MAP","unknown candidate port"))?;let size=self.n.checked_mul(2).ok_or_else(||error("SECTION_RETURN_MAP","candidate dimensions overflow"))?;
        if a>=self.n||b>=self.n||self.high.len()!=size||self.correction.len()!=size||self.high.iter().any(|x|!x.is_finite())||self.correction.iter().any(|x|!x.hi.is_finite()||!x.lo.is_finite()){return Err(error("SECTION_RETURN_MAP","invalid candidate port state"));}
        let out=[0,self.n].map(|off|super::accurate::Sum::from(self.high[off+a]).add(self.correction[off+a]).sub(super::accurate::Sum::from(self.high[off+b]).add(self.correction[off+b])));
        if out.iter().any(|x|!x.hi.is_finite()||!x.lo.is_finite()){return Err(error("SECTION_RETURN_MAP","nonfinite candidate port"));}Ok(out)
    }
}
/// Compile a fresh source-only model; external devices and stamped systems are
/// deliberately not accepted. Physical and assembled original gates both apply.
pub(in crate::extraction) fn solve_source_candidate(
    basis:&super::volume_reduction::Basis,space:&Space,field:&dyn super::volume_field::ElectricApply,
    magnetic:&dyn super::volume_magnetic::MagneticApply,inductance:&[Vec<f64>],ports:&[[usize;2]],
    real_drives:&[f64],imaginary_drives:&[f64],omega:f64,budget:AllocationBudget,limit:std::time::Duration,
    stats:&mut crate::electrical::Counters,
)->Result<CandidateState> {
    use super::{accurate::Sum,island_response::IslandResponse};use std::time::Instant;
    let start=Instant::now();let check=||if start.elapsed()>=limit {Err(error("DEADLINE","source candidate deadline"))}else{Ok(())};check()?;
    let nv=basis.voltage.len();let ni=basis.current.len();let n=nv.checked_add(ni).ok_or_else(||error("MEMORY_LIMIT","source candidate size overflow"))?;let nf=space.potential_count();let nc=space.current_count();let fixed=basis.retained_dofs();
    if !omega.is_finite()||omega<=0.||ports.is_empty()||real_drives.len()!=ports.len()||imaginary_drives.len()!=ports.len()||real_drives.iter().chain(imaginary_drives).any(|v|!v.is_finite())||fixed.len()>nv||ports.iter().any(|p|p[0]>=fixed.len()||p[1]>=fixed.len()||p[0]==p[1])||basis.names.len()!=nv||basis.voltage.iter().any(|v|v.len()!=nf||v.iter().any(|x|!x.is_finite()))||basis.current.iter().any(|v|v.len()!=nc||v.iter().any(|x|!x.is_finite())) {return Err(error("SECTION_RETURN_MAP","invalid source candidate dimensions"));}
    budget.check(nf.checked_add(nv).and_then(|x|x.checked_mul(std::mem::size_of::<usize>())).ok_or_else(||error("MEMORY_LIMIT","source ownership plan overflow"))?)?;
    let mut physical_owner=vec![usize::MAX;nf];for (c,nodes) in space.components.iter().enumerate(){check()?;if nodes.is_empty(){return Err(error("SECTION_RETURN_MAP","empty physical component"));}for (j,&dof) in nodes.iter().enumerate(){if j%256==0 {check()?;}if dof>=nf||physical_owner[dof]!=usize::MAX{return Err(error("SECTION_RETURN_MAP","invalid physical component partition"));}physical_owner[dof]=c;}}if physical_owner.iter().any(|&c|c==usize::MAX){return Err(error("SECTION_RETURN_MAP","incomplete physical component partition"));}
    let mut owners=vec![usize::MAX;nv];if basis.component_columns.len()!=space.components.len(){return Err(error("SECTION_RETURN_MAP","component partition mismatch"));}
    for (c,cols) in basis.component_columns.iter().enumerate(){check()?;if cols.is_empty(){return Err(error("SECTION_RETURN_MAP","empty component partition"));}for &i in cols {if i>=nv||owners[i]!=usize::MAX{return Err(error("SECTION_RETURN_MAP","duplicate component coordinate"));}owners[i]=c;}for j in 0..nf {if j%256==0 {check()?;}let sum=cols.iter().map(|&i|basis.voltage[i][j]).sum::<f64>();let target=if physical_owner[j]==c {1.}else{0.};if !sum.is_finite()||(sum-target).abs()>16.*f64::EPSILON*(cols.len() as f64).max(1.) {return Err(error("SECTION_RETURN_MAP","invalid component partition"));}}}
    if owners.iter().any(|&x|x==usize::MAX)||fixed.iter().any(|&i|i>=nf) {return Err(error("SECTION_RETURN_MAP","incomplete coordinate ownership"));}for (i,&dof) in fixed.iter().enumerate(){if physical_owner[dof]!=owners[i]{return Err(error("SECTION_RETURN_MAP","electrode ownership mismatch"));}}
    // Response live/scratch accounting includes Space and Field. Reserve it
    // alongside the source model, compiled system, basis and output states.
    let response=IslandResponse::new(space,field,budget,start,limit)?;check()?;
    let extra=n.checked_mul(n).and_then(|x|x.checked_mul(4096)).and_then(|x|x.checked_add(nv.checked_mul(nf)?.checked_mul(16)?)).and_then(|x|x.checked_add(ni.checked_mul(nc)?.checked_mul(16)?)).and_then(|x|x.checked_add(nf.checked_mul(512)?)).and_then(|x|x.checked_add(nc.checked_mul(256)?)).and_then(|x|x.checked_add(magnetic.planned_bytes())).and_then(|x|x.checked_add(magnetic.scratch_bytes())).ok_or_else(||error("MEMORY_LIMIT","source candidate plan overflow"))?;
    let plan=extra.checked_add(space.planned_bytes).and_then(|x|x.checked_add(field.planned_bytes())).and_then(|x|x.checked_add(response.retained_bytes())).and_then(|x|x.checked_add(response.scratch_bytes())).ok_or_else(||error("MEMORY_LIMIT","combined source response plan overflow"))?;budget.check(plan)?;
    let model=basis.model_with_inductance(space,field,inductance,ports,budget)?;check()?;let network_budget=crate::electrical::Budget{max_seconds:Some(limit.saturating_sub(start.elapsed()).as_secs_f64()),max_memory_mi_b:Some(budget.max_bytes as f64/(1024.*1024.))};let mut system=crate::electrical::coupled::System::compile(&model,ports.len(),Some(&network_budget))?;check()?;
    let mut rhs=system.port_rhs(real_drives);rhs.resize(2*n,0.);let imag=system.port_rhs(imaginary_drives);rhs[n..].copy_from_slice(&imag[..n]);
    let mut injection_r=vec![0.;nf];let mut injection_i=vec![0.;nf];for (p,(&r,&i)) in ports.iter().zip(real_drives.iter().zip(imaginary_drives)) {injection_r[fixed[p[0]]]+=r;injection_r[fixed[p[1]]]-=r;injection_i[fixed[p[0]]]+=i;injection_i[fixed[p[1]]]-=i;}
    let (high,shape)=system.solve_charge_coordinates_split(omega,&rhs,stats)?;check()?;
    let potential=|off:usize|->Vec<f64>{(0..nf).map(|j|basis.voltage.iter().enumerate().map(|(i,v)|v[j]*high[off+i]).sum()).collect()};let vr=potential(0);let vi=potential(n);
    let refined=response.refine(&vr,&vi,&injection_r,&injection_i,omega,budget,start,limit)?;check()?;
    let mut correction=vec![Sum::default();2*n];for (c,cols) in basis.component_columns.iter().enumerate(){for &i in cols {correction[i]=refined.real_constants[c];correction[n+i]=refined.imaginary_constants[c];}}
    system.audit_compensated_state(omega,&rhs,&high,&correction)?;check()?;
    let qr:Vec<_>=refined.real_charge.iter().map(|x|x.value()).collect();let qi:Vec<_>=refined.imaginary_charge.iter().map(|x|x.value()).collect();audit_original(basis,space,magnetic,&high,&shape,&qr,&qi,Some((&refined.real_charge,&refined.imaginary_charge)),omega,&injection_r,&injection_i)?;check()?;
    Ok(CandidateState{high,correction,real_charge:refined.real_charge,imaginary_charge:refined.imaginary_charge,ports:ports.to_vec(),n})
}

/// Enrich from the reduced model's physical KCL residual, never a full solve.
pub(in crate::extraction) fn enrich_kcl(
    basis:&mut super::volume_reduction::Basis, space:&Space, field:&dyn super::volume_field::ElectricApply,
    magnetic:&dyn super::volume_magnetic::MagneticApply, fixed:&[usize], ports:&[[usize;2]],
    drives:&[Vec<f64>], omega:f64, rounds:usize, magnetic_rounds:usize, budget:AllocationBudget,
) -> Result<()> {
    if rounds>5 || magnetic_rounds>3 || magnetic_rounds>rounds || omega<=0. || !omega.is_finite() || fixed!=basis.retained_dofs() || ports.iter().any(|p|p[0]>=fixed.len()||p[1]>=fixed.len()||p[0]==p[1]) || drives.iter().any(|d|d.len()!=ports.len()||d.iter().any(|x|!x.is_finite())) {return Err(error("SECTION_RETURN_MAP","invalid residual enrichment inputs"));}
    let factor=space.dc_factor(fixed,budget)?;
    budget.check(factor.planned_bytes.checked_add(factor.solve_scratch_bytes(space)).and_then(|x|x.checked_add(field.planned_bytes())).and_then(|x|x.checked_add(magnetic.planned_bytes())).ok_or_else(||error("MEMORY_LIMIT","residual factor plan overflow"))?)?;
    for round in 0..rounds {
        let nv=basis.voltage.len();let ni=basis.current.len();
        let live=nv.checked_mul(space.potential_count()).and_then(|x|ni.checked_mul(space.current_count()).and_then(|y|x.checked_add(y))).and_then(|x|drives.len().checked_mul(2).and_then(|d|d.checked_mul(space.potential_count())).and_then(|y|x.checked_add(y))).and_then(|x|x.checked_mul(16)).and_then(|x|nv.checked_add(ni).and_then(|n|n.checked_pow(2)).and_then(|n|n.checked_mul(1600)).and_then(|y|x.checked_add(y))).and_then(|x|x.checked_add(factor.planned_bytes)).and_then(|x|x.checked_add(factor.solve_scratch_bytes(space))).and_then(|x|x.checked_add(field.planned_bytes())).and_then(|x|x.checked_add(magnetic.planned_bytes())).and_then(|x|x.checked_add(magnetic.scratch_bytes())).ok_or_else(||error("MEMORY_LIMIT","residual enrichment live plan overflow"))?;budget.check(live)?;
        let model=basis.model(space,field,magnetic,ports,budget)?;
        let mut system=crate::electrical::coupled::System::compile(&model,ports.len(),None)?;
        let nv=basis.voltage.len();let n=nv+basis.current.len();let mut profiles=vec![];
        for drive in drives {
            if drive.len()!=ports.len() || drive.iter().any(|x|!x.is_finite()) {return Err(error("SECTION_RETURN_MAP","invalid drive"));}
            let mut rhs=system.port_rhs(drive);rhs.resize(2*n,0.);
            let x=system.solve_charge_coordinates(omega,&rhs,&mut crate::electrical::Counters::default())?;
            let recover=|offset:usize,columns:&Vec<Vec<f64>>| -> Vec<f64> {(0..columns[0].len()).map(|j|columns.iter().enumerate().map(|(k,v)|v[j]*x[offset+k]).sum()).collect()};
            let vr=recover(0,&basis.voltage);let vi=recover(n,&basis.voltage);
            let ir=recover(nv,&basis.current);let ii=recover(n+nv,&basis.current);
            let dr=space.divergence(&ir)?;let di=space.divergence(&ii)?;let qr=field.apply(&vr)?;let qi=field.apply(&vi)?;
            let mut injection=vec![0.;space.potential_count()];
            for (p,&a) in ports.iter().zip(drive) {injection[fixed[p[0]]]+=a;injection[fixed[p[1]]]-=a;}
            let rr:Vec<_>=(0..dr.len()).map(|i|injection[i]-dr[i]+omega*qi[i]).collect();
            let ri:Vec<_>=(0..di.len()).map(|i|-di[i]-omega*qr[i]).collect();
            profiles.push(factor.solve(space,&vec![0.;rr.len()],&rr)?);
            profiles.push(factor.solve(space,&vec![0.;ri.len()],&ri)?);
        }
        if basis.include_global_potential_profiles(space,&profiles,budget)?==0 {break;}
        let bytes=space.current_count().checked_mul(basis.current.len()).and_then(|n|n.checked_mul(24)).and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(magnetic.scratch_bytes())).ok_or_else(||error("MEMORY_LIMIT","magnetic enrichment plan overflow"))?;budget.check(bytes)?;
        if round<magnetic_rounds {basis.extend_magnetic(space,magnetic)?;}
    }
    Ok(())
}

/// Rigid right-handed mapping from section coordinates into physical space.
#[derive(Clone, Copy)]
pub(in crate::extraction) struct SectionFrame {
    pub origin:[f64;3], pub transverse_x:[f64;3], pub transverse_y:[f64;3], pub longitudinal:[f64;3],
}
impl SectionFrame {
    fn validate(&self)->Result<()> {
        let axes=[self.transverse_x,self.transverse_y,self.longitudinal];
        if self.origin.iter().chain(axes.iter().flatten()).any(|x|!x.is_finite()) {return Err(error("SECTION_RETURN_MAP","finite frame required"));}
        let dot=|a:[f64;3],b:[f64;3]|(0..3).map(|i|a[i]*b[i]).sum::<f64>();
        let tol=128.*f64::EPSILON;
        for i in 0..3 {for j in 0..3 {if (dot(axes[i],axes[j])-if i==j {1.} else {0.}).abs()>tol {return Err(error("SECTION_RETURN_MAP","orthonormal frame required"));}}}
        let a=axes[0];let b=axes[1];let cross=[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
        if (dot(cross,axes[2])-1.).abs()>tol {return Err(error("SECTION_RETURN_MAP","right-handed frame required"));}Ok(())
    }
}
/// Identity-frame compatibility wrapper for native Z extrusions.
pub(in crate::extraction) fn lift(
    section:&Mesh<'_>, recovery:&Recovery, mesh:&LayeredMesh, space:&Space,
    slabs:std::ops::Range<usize>, source_owners:&BTreeMap<String,usize>, budget:AllocationBudget,
) -> Result<Vec<Vec<f64>>> {
    lift_in_frame(section,recovery,mesh,space,slabs,source_owners,budget,SectionFrame{origin:[0.;3],transverse_x:[1.,0.,0.],transverse_y:[0.,1.,0.],longitudinal:[0.,0.,1.]})
}
/// Integrate the P1 physical section profile over each owned tetrahedron and
/// represent it by its P0 mean. This preserves integrated current, not P1
/// Joule loss; use the physical cell resistance operator for projected loss.
pub(in crate::extraction) fn lift_in_frame(
    section:&Mesh<'_>, recovery:&Recovery, mesh:&LayeredMesh, space:&Space,
    slabs:std::ops::Range<usize>, source_owners:&BTreeMap<String,usize>, budget:AllocationBudget, frame:SectionFrame,
) -> Result<Vec<Vec<f64>>> {
    frame.validate()?;
    if slabs.is_empty() {return Err(error("SECTION_RETURN_MAP","nonempty section extent required"));}
    let loops=section.conductors.len().checked_sub(1).ok_or_else(||error("SECTION_RETURN_MAP","conductors required"))?;
    let bytes=space.current_count().checked_mul(loops).and_then(|n|n.checked_mul(16)).and_then(|n|n.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","section lift overflow"))?;
    budget.check(bytes)?;
    let mut rows=BTreeMap::new();
    for row in &recovery.triangles {
        let tri=section.triangles.get(row.triangle_index).ok_or_else(||error("SECTION_RETURN_MAP","unknown section triangle"))?;
        if row.nodes!=tri.nodes||tri.conductor!=Some(row.conductor)||row.jz_a_per_m2.len()!=loops||rows.insert(row.triangle_index,row).is_some() {
            return Err(error("SECTION_RETURN_MAP","duplicate or mismatched section ownership"));
        }
    }
    let mut out=vec![vec![0.;space.current_count()];2*loops];
    let mut covered=vec![false;section.triangles.len()];
    for (ci,cell) in space.cells.iter().enumerate() {
        let tet=mesh.copper_tetrahedra.get(cell.mesh_cell).ok_or_else(||error("SECTION_RETURN_MAP","unknown physical cell"))?;
        if !slabs.contains(&tet.slab) {continue;}
        let row=rows.get(&tet.planar_triangle).ok_or_else(||error("SECTION_RETURN_MAP","copper cell lacks exclusive section owner"))?;
        if tet.source_ids.is_empty() || tet.source_ids.iter().any(|id|source_owners.get(id)!=Some(&row.conductor)) {
            return Err(error("SECTION_RETURN_MAP","physical source owner missing or mismatched"));
        }
        let mut weights=[0.;3];
        for &node in &tet.nodes {
            let p=mesh.nodes_m.get(node).ok_or_else(||error("SECTION_RETURN_MAP","unknown physical vertex"))?;
            let delta=[p[0]-frame.origin[0],p[1]-frame.origin[1],p[2]-frame.origin[2]];
            let local_xy=[(0..3).map(|i|delta[i]*frame.transverse_x[i]).sum::<f64>(),(0..3).map(|i|delta[i]*frame.transverse_y[i]).sum::<f64>()];
            let scale=p.iter().chain(frame.origin.iter()).chain(local_xy.iter()).map(|v|v.abs()).fold(f64::MIN_POSITIVE,f64::max);
            let mut matches=row.nodes.iter().enumerate().filter_map(|(i,&n)|section.nodes_m.get(n).and_then(|q|{if q.iter().any(|v|!v.is_finite()){return None;}let tolerance=64.*f64::EPSILON*scale.max(q[0].abs()).max(q[1].abs());((q[0]-local_xy[0]).abs()<=tolerance&&(q[1]-local_xy[1]).abs()<=tolerance).then_some(i)}));
            let local=matches.next().ok_or_else(||error("SECTION_RETURN_MAP","nonconforming section vertex"))?;
            if matches.next().is_some() {return Err(error("SECTION_RETURN_MAP","ambiguous section vertex"));}
            weights[local]+=0.25;
        }
        if !cell.density_per_amp_m2.is_finite()||cell.density_per_amp_m2<=0. {return Err(error("SECTION_RETURN_MAP","invalid physical density scale"));}
        for p in 0..loops {
            let real=(0..3).map(|k|weights[k]*row.jz_a_per_m2[p][k].real).sum::<f64>()/cell.density_per_amp_m2;
            let imag=(0..3).map(|k|weights[k]*row.jz_a_per_m2[p][k].imaginary).sum::<f64>()/cell.density_per_amp_m2;
            if !real.is_finite()||!imag.is_finite() {return Err(error("SECTION_RETURN_MAP","nonfinite profile"));}
            for k in 0..3 {out[2*p][3*ci+k]=real*frame.longitudinal[k];out[2*p+1][3*ci+k]=imag*frame.longitudinal[k];}
        }
        covered[tet.planar_triangle]=true;
    }
    if rows.keys().any(|&i|!covered[i]) {return Err(error("SECTION_RETURN_MAP","incomplete section copper support"));}
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{mesh::{self,LayeredInput,Material},volume_current,volume_reduction,volume_field,volume_magnetic,volume_solve};
    use crate::extraction::section_transition::{self,Conductor,Triangle};
    use std::time::{Instant,Duration};
    use crate::electrical::{coupled,Counters};

    #[test]
    fn actual_section_profiles_share_return_and_conserve_slab_current() { control(false); }
    #[test]
    fn long_section_residual_enrichment() { control(true); }
    fn control(long:bool) {
        let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let nodes:Vec<_>=(0..8).flat_map(|y|[[0.,y as f64*1e-3],[0.5e-3,y as f64*1e-3],[1e-3,y as f64*1e-3]]).collect();
        let triangles:Vec<_>=(0..7).flat_map(|y|(0..2).flat_map(move |x|{
            let conductor=if y%2==1 {Some(y/2)}else{None};
            let a=3*y+x;
            [[a,a+1,a+4],[a,a+4,a+3]].map(|nodes|Triangle{nodes,conductor})
        })).collect();
        let conductors=[Conductor{id:"signal-a",conductivity_s_m:58e6},Conductor{id:"signal-b",conductivity_s_m:58e6},Conductor{id:"finite-return",conductivity_s_m:58e6}];
        let section=Mesh{nodes_m:&nodes,triangles:&triangles,conductors:&conductors,reference:2};
        let (_,recovery)=section_transition::solve_with_recovery(&section,1000.,0,&section_transition::Budget{max_bytes:budget.max_bytes,deadline:Some(Instant::now()+Duration::from_secs(10))},1024*1024).unwrap();
        let material:Vec<_>=triangles.iter().map(|t|match t.conductor {Some(c)=>Material::Copper{source_ids:vec![conductors[c].id.into()]},None=>Material::Dielectric{label:"gap".into(),epsilon_r:1.}}).collect();
        let mut edges=vec![[0,1],[1,2],[21,22],[22,23]];
        for y in 0..7 {edges.extend([[3*y,3*y+3],[3*y+2,3*y+5]]);}
        for cut in [1e-3,0.7e-3] {
        let z_m=if long {vec![0.,cut/2.,cut,(cut+2e-3)/2.,2e-3]} else {vec![0.,cut,2e-3]};
        let slabs=z_m.len()-1;
        let physical=mesh::extrude(&LayeredInput{xy_m:nodes.clone(),triangles:triangles.iter().map(|t|t.nodes).collect(),boundary_edges:edges.clone(),z_m:z_m.clone(),materials:vec![material.clone();slabs]},budget).unwrap();
        let electrodes:Vec<Vec<usize>>=(0..3).flat_map(|c|[0.,2e-3].map(|z|physical.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[2]==z&&p[1]>=(2*c+1)as f64*1e-3&&p[1]<=(2*c+2)as f64*1e-3).then_some(i)).collect())).collect();
        let space=volume_current::assemble(&physical,&vec![58e6;physical.copper_tetrahedra.len()],&electrodes,budget).unwrap();
        let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,n)|(format!("contact-{i}"),space.node_potential[&n[0]])).collect();
        let mut basis=volume_reduction::harmonic(&space,&contacts,budget).unwrap();
        let original_voltage=basis.voltage.clone();let original_components=basis.component_columns.clone();
        let owners:BTreeMap<_,_>=conductors.iter().enumerate().map(|(i,c)|(c.id.to_string(),i)).collect();
        let mut both=vec![];
        for slab in 0..slabs {
            let profiles=lift(&section,&recovery,&physical,&space,slab..slab+1,&owners,budget).unwrap();
            if slab==0 && !long && cut==1e-3 {
                let s2=2_f64.sqrt();let s3=3_f64.sqrt();let s6=6_f64.sqrt();
                let frames=[SectionFrame{origin:[0.2,-0.1,0.3],transverse_x:[0.,1.,0.],transverse_y:[0.,0.,1.],longitudinal:[1.,0.,0.]},SectionFrame{origin:[0.2,-0.1,0.3],transverse_x:[1./s2,-1./s2,0.],transverse_y:[1./s6,1./s6,-2./s6],longitudinal:[1./s3,1./s3,1./s3]}];
                for frame in frames {
                    let rotate=|v:[f64;3]|-> [f64;3] {std::array::from_fn(|k|v[0]*frame.transverse_x[k]+v[1]*frame.transverse_y[k]+v[2]*frame.longitudinal[k])};
                    let mut transformed=physical.clone();
                    for p in &mut transformed.nodes_m {let r=rotate(*p);for k in 0..3 {p[k]=frame.origin[k]+r[k];}}
                    for t in &mut transformed.tetrahedra {for g in &mut t.gradients_per_m {*g=rotate(*g);}}
                    for t in &mut transformed.copper_tetrahedra {for g in &mut t.gradients_per_m {*g=rotate(*g);}}
                    let rotated_space=volume_current::assemble(&transformed,&vec![58e6;transformed.copper_tetrahedra.len()],&electrodes,budget).unwrap();
                    let rotated=lift_in_frame(&section,&recovery,&transformed,&rotated_space,0..1,&owners,budget,frame).unwrap();
                    for p in 0..4 {for c in 0..3 {
                        let flow=|mesh:&LayeredMesh,sp:&Space,v:&[f64],axis:[f64;3]|->f64 {sp.cells.iter().enumerate().filter(|(_,cell)|{let t=&mesh.copper_tetrahedra[cell.mesh_cell];t.slab==0&&triangles[t.planar_triangle].conductor==Some(c)}).map(|(i,cell)|(0..3).map(|k|v[3*i+k]*axis[k]).sum::<f64>()*cell.density_per_amp_m2*mesh.copper_tetrahedra[cell.mesh_cell].volume_m3/cut).sum()};
                        let a=flow(&physical,&space,&profiles[p],[0.,0.,1.]);let b=flow(&transformed,&rotated_space,&rotated[p],frame.longitudinal);assert!((a-b).abs()<1e-10);
                    }
                    let a=space.losses(&profiles[p]).unwrap().iter().sum::<f64>();let b=rotated_space.losses(&rotated[p]).unwrap().iter().sum::<f64>();assert!((a-b).abs()<1e-10*a.max(1e-30));}
                    let mut bad=frame;bad.transverse_x=frame.transverse_y;assert!(lift_in_frame(&section,&recovery,&transformed,&rotated_space,0..1,&owners,budget,bad).is_err());
                    bad=frame;bad.longitudinal=bad.longitudinal.map(|x|-x);assert!(lift_in_frame(&section,&recovery,&transformed,&rotated_space,0..1,&owners,budget,bad).is_err());
                    let mut bad_mesh=transformed.clone();let node=bad_mesh.copper_tetrahedra.iter().find(|t|t.slab==0).unwrap().nodes[0];for k in 0..3 {bad_mesh.nodes_m[node][k]+=1e-9*frame.transverse_x[k];}assert!(lift_in_frame(&section,&recovery,&bad_mesh,&rotated_space,0..1,&owners,budget,frame).is_err());
                    bad=frame;bad.origin[0]=f64::NAN;assert!(lift_in_frame(&section,&recovery,&transformed,&rotated_space,0..1,&owners,budget,bad).is_err());
                    let row=&recovery.triangles[0];let mut ambiguous_nodes=nodes.clone();ambiguous_nodes[row.nodes[1]]=ambiguous_nodes[row.nodes[0]];
                    let ambiguous=Mesh{nodes_m:&ambiguous_nodes,triangles:&triangles,conductors:&conductors,reference:2};
                    assert!(lift_in_frame(&ambiguous,&recovery,&transformed,&rotated_space,0..1,&owners,budget,frame).is_err());
                }
            }
            for p in 0..2 {for c in 0..3 {
                let total:f64=space.cells.iter().enumerate().filter(|(_,cell)|{let t=&physical.copper_tetrahedra[cell.mesh_cell];t.slab==slab&&triangles[t.planar_triangle].conductor==Some(c)}).map(|(i,cell)|profiles[2*p][3*i+2]*cell.density_per_amp_m2*physical.copper_tetrahedra[cell.mesh_cell].volume_m3/(z_m[slab+1]-z_m[slab])).sum();
                let expected=if c==p {1.}else if c==2 {-1.}else{0.};
                assert!((total-expected).abs()<1e-8,"slab={slab}, conductor={c}: {total}");
            }}
            // Both signal profiles overlap on the SAME physical return cells.
            assert!(space.cells.iter().enumerate().any(|(i,_)|profiles[0][3*i+2]!=0.&&profiles[2][3*i+2]!=0.));
            both.extend(profiles);
        }
        basis.include_current_profiles(&space,&both,budget).unwrap();
        let field=volume_field::Field::new(&physical,&space,budget).unwrap();
        let magnetic=volume_magnetic::Operator::new(&physical,budget).unwrap();
        let solver=volume_solve::Solver::new(&space,&field,&magnetic,budget).unwrap();
        let cut_dofs:Vec<_>=physical.nodes_m.iter().enumerate().filter_map(|(node,p)|if p[2]==cut {space.node_potential.get(&node).copied()}else{None}).collect();
        let trace=candidate(&space,&contacts,&cut_dofs,&both,&magnetic,0,budget).unwrap();
        let mut enriched=candidate(&space,&contacts,&cut_dofs,&both,&magnetic,if long {0}else{1},budget).unwrap();
        let constants:Vec<Vec<f64>>=enriched.component_columns.iter().map(|cols|(0..space.potential_count()).map(|i|cols.iter().map(|&c|enriched.voltage[c][i]).sum()).collect()).collect();
        let mut fixed:Vec<_>=contacts.iter().map(|v|v.1).collect();for &i in &cut_dofs {if !fixed.contains(&i) {fixed.push(i);}}
        if long {enrich_kcl(&mut enriched,&space,&field,&magnetic,&fixed,&[[0,4],[1,5],[2,4],[3,5]],&[vec![1.,-1.,0.,0.],vec![0.,0.,1.,-1.]],2.*std::f64::consts::PI*1000.,3,3,budget).unwrap();}
        for (j,cols) in enriched.component_columns.iter().enumerate() {for i in 0..space.potential_count() {assert!((cols.iter().map(|&c|enriched.voltage[c][i]).sum::<f64>()-constants[j][i]).abs()<1e-12);}}
        if long {assert!(enriched.voltage.len()<space.potential_count());}
        for (label,basis) in [("section",&basis),("trace",&trace),("trace+magnetic",&enriched)] {
        let model=basis.model(&space,&field,&magnetic,&[[0,4],[1,5],[2,4],[3,5]],budget).unwrap();
        let mut system=coupled::System::compile(&model,4,None).unwrap();
        let n=model.potential_nodes.len()+model.current_dofs.len();
        eprintln!("{label} dimensions full=({}, {}) reduced=({}, {})",space.potential_count(),space.current_count(),model.potential_nodes.len(),model.current_dofs.len());
        for drive in 0..2 {
        let mut injection=vec![0.;space.potential_count()];
        injection[contacts[2*drive].1]=1.;injection[contacts[2*drive+1].1]=-1.;injection[contacts[4].1]=-1.;injection[contacts[5].1]=1.;
        let solution=solver.solve(1000.,&injection,&vec![0.;injection.len()],Duration::from_secs(30)).unwrap();
        let mut amplitudes=[0.,0.,0.,0.];amplitudes[2*drive]=1.;amplitudes[2*drive+1]=-1.;
        let mut rhs=system.port_rhs(&amplitudes);rhs.resize(2*n,0.);
        let omega=2.*std::f64::consts::PI*1000.;
        if label=="section" || (long && label=="trace") {let mut negative=coupled::System::compile(&model,4,None).unwrap();assert!(solve_candidate(basis,&space,&field,&magnetic,&mut negative,omega,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default()).is_err());}
        assert!(solve_candidate(basis,&space,&field,&magnetic,&mut system,omega,&rhs,&[],&[],&mut Counters::default()).is_err());
        let x=if label=="trace+magnetic" {solve_candidate(basis,&space,&field,&magnetic,&mut system,omega,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default()).unwrap()} else {system.solve(omega,rhs,&vec![0.;n],&mut Counters::default()).unwrap()};
        let nv=model.potential_nodes.len();
        let recover=|offset:usize,columns:&Vec<Vec<f64>>| -> Vec<f64> {(0..columns[0].len()).map(|j|columns.iter().enumerate().map(|(k,v)|v[j]*x[offset+k]).sum()).collect()};
        let vr=recover(0,&basis.voltage);let vi=recover(n,&basis.voltage);
        let ir=recover(nv,&basis.current);let ii=recover(n+nv,&basis.current);
        let dr=space.divergence(&ir).unwrap();let di=space.divergence(&ii).unwrap();
        let qr=field.apply(&vr).unwrap();let qi=field.apply(&vi).unwrap();
        let full_potential=|shape:&[f64],correction:&[f64],constants:&[f64]| {let mut v:Vec<_>=shape.iter().zip(correction).map(|(a,b)|a+b).collect();for (c,nodes) in space.components.iter().enumerate(){for &i in nodes {v[i]+=constants[c];}}v};
        let full_qr=field.apply(&full_potential(&solution.real_shape_v,&solution.real_shape_correction_v,&solution.real_constants_v)).unwrap();
        let full_qi=field.apply(&full_potential(&solution.imaginary_shape_v,&solution.imaginary_shape_correction_v,&solution.imaginary_constants_v)).unwrap();
        if label=="trace+magnetic" {for (c,nodes) in space.components.iter().enumerate() {
            let sum=|v:&[f64]|nodes.iter().map(|&i|v[i]).sum::<f64>();
            let scale=nodes.iter().map(|&i|qr[i].abs()+qi[i].abs()).sum::<f64>();
            eprintln!("island={c} reduced Q=({:e},{:e}) full Q=({:e},{:e}) scale={scale:e}",sum(&qr),sum(&qi),sum(&full_qr),sum(&full_qi));
        }}
        let gr=space.gradient(&vr).unwrap();let gi=space.gradient(&vi).unwrap();
        let lr=magnetic.apply(&space,&ir).unwrap();let li=magnetic.apply(&space,&ii).unwrap();
        let w=2.*std::f64::consts::PI*1000.;
        let kcl=(0..dr.len()).map(|j|(dr[j]-w*qi[j]-injection[j]).hypot(di[j]+w*qr[j])).fold(0.,f64::max);
        let kvl=(0..ir.len()).map(|j|(space.cells[j/3].resistance_ohm*ir[j]-gr[j]-w*li[j]).hypot(space.cells[j/3].resistance_ohm*ii[j]-gi[j]+w*lr[j])).fold(0.,f64::max);
        let residual_energy=(0..ir.len()).map(|j|{let r=space.cells[j/3].resistance_ohm;((r*ir[j]-gr[j]-w*li[j]).powi(2)+(r*ii[j]-gi[j]+w*lr[j]).powi(2))/r}).sum::<f64>();
        let loss=space.losses(&ir).unwrap().iter().sum::<f64>()+space.losses(&ii).unwrap().iter().sum::<f64>();
        let full_loss=space.losses(&solution.real_currents_a).unwrap().iter().sum::<f64>()+space.losses(&solution.imaginary_currents_a).unwrap().iter().sum::<f64>();
        let reduced_r=system.port_voltages(&x,4);let reduced_i=system.port_voltages(&x[n..],4);
        let mut max_port_error:f64=0.;let mut full_ports=vec![];
        for port in 0..4 {
            let full=solver.port_voltage(&solution,contacts[port].1,contacts[4+port%2].1);
            full_ports.push(full);
            let err=(reduced_r[port]-full[0]).hypot(reduced_i[port]-full[1]);
            max_port_error=max_port_error.max(err/full[0].hypot(full[1]).max(1e-30));
        }
        let a=2*drive;let loop_full=[full_ports[a][0]-full_ports[a+1][0],full_ports[a][1]-full_ports[a+1][1]];
        let loop_error=((reduced_r[a]-reduced_r[a+1]-loop_full[0]).hypot(reduced_i[a]-reduced_i[a+1]-loop_full[1]))/loop_full[0].hypot(loop_full[1]);
        if label=="trace+magnetic" {
            assert!(kcl<if long {1e-8}else{1e-10} && kvl<1e-14 && residual_energy/full_loss<1e-18 && loop_error<1e-8 && max_port_error<1e-8,"port_error={max_port_error:e}");
            for slab in 0..slabs {
                let return_current:f64=space.cells.iter().enumerate().filter(|(_,cell)|{let t=&physical.copper_tetrahedra[cell.mesh_cell];t.slab==slab&&triangles[t.planar_triangle].conductor==Some(2)}).map(|(i,cell)|ir[3*i+2]*cell.density_per_amp_m2*physical.copper_tetrahedra[cell.mesh_cell].volume_m3/(z_m[slab+1]-z_m[slab])).sum();
                assert!((return_current+1.).abs()<1e-8,"return current {return_current}");
            }
        }
        eprintln!("cut={cut} drive={drive} {label} port_error={max_port_error:e} loop_error={loop_error:e} KCL={kcl:e} KVL={kvl:e} residual_energy={residual_energy:e} loss={loss:e} full_loss={full_loss:e}");
        }
        }
        assert_eq!(basis.voltage,original_voltage);assert_eq!(basis.component_columns,original_components);assert_eq!(basis.component_columns.len(),3);
        let before=basis.current.clone();
        assert!(basis.include_current_profiles(&space,&[vec![f64::NAN;space.current_count()]],budget).is_err());
        assert_eq!(basis.current,before);
        let mut bad=physical.clone();bad.nodes_m[3][0]+=1e-9;
        assert!(lift(&section,&recovery,&bad,&space,0..1,&owners,budget).unwrap_err().contains("nonconforming"));
        assert!(lift(&section,&recovery,&physical,&space,slabs..slabs+1,&owners,budget).unwrap_err().contains("incomplete"));
        let mut wrong=owners.clone();wrong.insert("signal-a".into(),1);
        assert!(lift(&section,&recovery,&physical,&space,0..1,&wrong,budget).unwrap_err().contains("source owner"));
        wrong.remove("signal-a");
        assert!(lift(&section,&recovery,&physical,&space,0..1,&wrong,budget).unwrap_err().contains("source owner"));
        }
    }
    #[test]
    fn remote_bridge_geometry_controls_connectivity() {
        let budget=AllocationBudget{max_bytes:128*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let nodes:Vec<_>=(0..10).flat_map(|y|[[0.,y as f64*1e-3],[0.5e-3,y as f64*1e-3],[1e-3,y as f64*1e-3]]).collect();
        let triangles:Vec<_>=(0..9).flat_map(|y|(0..2).flat_map(move |x|{
            let conductor=if y%2==1 {Some(y/2)}else{None};let a=3*y+x;
            [[a,a+1,a+4],[a,a+4,a+3]].map(|nodes|Triangle{nodes,conductor})
        })).collect();
        let conductors=[Conductor{id:"signal-a",conductivity_s_m:58e6},Conductor{id:"signal-b",conductivity_s_m:58e6},Conductor{id:"return-a",conductivity_s_m:58e6},Conductor{id:"return-b",conductivity_s_m:58e6}];
        let section=Mesh{nodes_m:&nodes,triangles:&triangles,conductors:&conductors,reference:3};
        let (_,recovery)=section_transition::solve_with_recovery(&section,1000.,0,&section_transition::Budget{max_bytes:budget.max_bytes,deadline:Some(Instant::now()+Duration::from_secs(10))},1024*1024).unwrap();
        let uniform:Vec<_>=triangles.iter().map(|t|match t.conductor {Some(c)=>Material::Copper{source_ids:vec![conductors[c].id.into()]},None=>Material::Dielectric{label:"gap".into(),epsilon_r:1.}}).collect();
        let mut edges=vec![[0,1],[1,2],[27,28],[28,29]];for y in 0..9 {edges.extend([[3*y,3*y+3],[3*y+2,3*y+5]]);}
        let owners:BTreeMap<_,_>=conductors.iter().enumerate().map(|(i,c)|(c.id.to_string(),i)).collect();
        for slabs in [3,5] {for bridge in [false,true] {
            let mut materials=vec![uniform.clone();slabs];
            if bridge {for t in 6*4..7*4 {materials[slabs-1][t]=Material::Copper{source_ids:vec!["bridge".into()]};}}
            let z_m:Vec<_>=(0..=slabs).map(|i|i as f64*1e-3).collect();
            let physical=mesh::extrude(&LayeredInput{xy_m:nodes.clone(),triangles:triangles.iter().map(|t|t.nodes).collect(),boundary_edges:edges.clone(),z_m,materials},budget).unwrap();
            let electrodes:Vec<Vec<usize>>=(0..4).map(|c|physical.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[2]==0.&&p[1]>=(2*c+1) as f64*1e-3&&p[1]<=(2*c+2) as f64*1e-3).then_some(i)).collect()).collect();
            let space=volume_current::assemble(&physical,&vec![58e6;physical.copper_tetrahedra.len()],&electrodes,budget).unwrap();
            assert_eq!(space.components.len(),if bridge {3}else{4});
            let returns_connected=space.components.iter().any(|c|c.contains(&space.node_potential[&electrodes[2][0]])&&c.contains(&space.node_potential[&electrodes[3][0]]));
            assert_eq!(returns_connected,bridge);
            let ra=space.node_potential[&electrodes[2][0]];let rb=space.node_potential[&electrodes[3][0]];
            let mut injection=vec![0.;space.potential_count()];injection[ra]=1.;injection[rb]=-1.;
            if bridge {
                let factor=space.dc_factor(&[rb],budget).unwrap();let v=factor.solve(&space,&vec![0.;space.potential_count()],&injection).unwrap();
                let actual=space.dc_apply(&v).unwrap();assert!((actual[rb]+1.).abs()<1e-8);assert!(v[ra]>0.);
            } else {
                assert!(space.components.iter().any(|component|component.iter().map(|&i|injection[i]).sum::<f64>().abs()>0.5));
            }
            let profiles=lift(&section,&recovery,&physical,&space,0..slabs-1,&owners,budget).unwrap();
            assert_eq!(profiles.len(),6);
            if slabs==3 {
                let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,n)|(format!("remote-{i}"),space.node_potential[&n[0]])).collect();
                let cuts:Vec<_>=physical.nodes_m.iter().enumerate().filter_map(|(i,p)|if p[2]==2e-3 {space.node_potential.get(&i).copied()}else{None}).collect();
                let field=volume_field::Field::new(&physical,&space,budget).unwrap();let magnetic=volume_magnetic::Operator::new(&physical,budget).unwrap();
                let mut basis=candidate(&space,&contacts,&cuts,&profiles,&magnetic,0,budget).unwrap();
                let mut local=vec![];for (i,cell) in space.cells.iter().enumerate() {if physical.copper_tetrahedra[cell.mesh_cell].slab==slabs-1 && (24..28).contains(&physical.copper_tetrahedra[cell.mesh_cell].planar_triangle) {for k in 0..3 {let mut v=vec![0.;space.current_count()];v[3*i+k]=1.;local.push(v);}}}
                basis.include_current_profiles(&space,&local,budget).unwrap();
                let fixed=basis.retained_dofs().to_vec();let ports=[[2,3],[0,2]];let drives=vec![vec![1.,0.]];let omega=2.*std::f64::consts::PI*1000.;
                enrich_kcl(&mut basis,&space,&field,&magnetic,&fixed,&ports,&drives,omega,3,3,budget).unwrap();
                let model=basis.model(&space,&field,&magnetic,&ports,budget).unwrap();let mut system=coupled::System::compile(&model,2,None).unwrap();
                let n=basis.voltage.len()+basis.current.len();let mut rhs=system.port_rhs(&drives[0]);rhs.resize(2*n,0.);
                let mut injection=vec![0.;space.potential_count()];injection[contacts[2].1]=1.;injection[contacts[3].1]=-1.;
                if bridge {let rejected=solve_candidate(&basis,&space,&field,&magnetic,&mut system,omega,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default()).unwrap_err();assert!(rejected.contains("SECTION_RETURN_RESIDUAL"));eprintln!("remote candidate rejected: {rejected}");}
                enrich_kcl(&mut basis,&space,&field,&magnetic,&fixed,&ports,&drives,omega,2,0,budget).unwrap();
                let model=basis.model(&space,&field,&magnetic,&ports,budget).unwrap();let mut system=coupled::System::compile(&model,2,None).unwrap();let n=basis.voltage.len()+basis.current.len();let mut rhs=system.port_rhs(&drives[0]);rhs.resize(2*n,0.);
                assert!(basis.voltage.len()<space.potential_count() && basis.current.len()<space.current_count());
                let x=solve_candidate(&basis,&space,&field,&magnetic,&mut system,omega,&rhs,&injection,&vec![0.;injection.len()],&mut Counters::default()).unwrap();
                let solver=volume_solve::Solver::new(&space,&field,&magnetic,budget).unwrap();let full=solver.solve(1000.,&injection,&vec![0.;injection.len()],Duration::from_secs(30)).unwrap();
                let pr=system.port_voltages(&x,2);let pi=system.port_voltages(&x[n..],2);
                for port in 0..2 {let f=solver.port_voltage(&full,contacts[ports[port][0]].1,contacts[ports[port][1]].1);let err=(pr[port]-f[0]).hypot(pi[port]-f[1])/f[0].hypot(f[1]).max(1e-30);eprintln!("remote bridge={bridge} admitted port={port} error={err:e}");assert!(err<1e-8);}
                eprintln!("remote bridge={bridge} AC full={}/{} reduced={}/{}",space.potential_count(),space.current_count(),basis.voltage.len(),basis.current.len());
            }
            eprintln!("remote bridge slabs={slabs} bridge={bridge} components={} potential={} current={}",space.components.len(),space.potential_count(),space.current_count());
        }}
    }

}
