//! Sparse, finite-box Newton-potential candidate. Requires independent mesh and
//! open-domain admission; reciprocity/passivity alone does not establish accuracy.
use super::{error,mesh::LayeredMesh,volume_current::Space,AllocationBudget,Result};
use faer::{dyn_stack::{MemBuffer,MemStack},sparse::{linalg::cholesky,SparseColMat,SymbolicSparseColMat,Triplet},Conj,Mat,Par,Side};
use std::collections::{BTreeMap,BTreeSet};
use faer::prelude::Solve;
use super::accurate::Sum;
pub trait MagneticApply: Sync {
    fn apply(&self,space:&Space,current:&[f64])->Result<Vec<f64>>;
    fn planned_bytes(&self)->usize;
    fn scratch_bytes(&self)->usize {self.planned_bytes()}
}
impl MagneticApply for Operator<'_> {
    // Full per-job reserve, including compensated load/field/residual/correction
    // vectors and their simultaneous live temporaries, plus queried faer scratch.
    fn scratch_bytes(&self)->usize {self.free.saturating_mul(512).saturating_add(self.mesh.copper_tetrahedra.len().saturating_mul(96)).saturating_add(self.symbolic.as_ref().map_or(0,|s|s.solve_in_place_scratch::<f64>(3,self.parallelism).unaligned_bytes_required())).saturating_add(65536)}
    fn apply(&self,space:&Space,current:&[f64])->Result<Vec<f64>> {Operator::apply(self,space,current)}
    fn planned_bytes(&self)->usize {self.planned_bytes}
}
struct Enrichment {
    coupling:Mat<f64>,images:Mat<f64>,gram:[f64;9],
    factor:faer::linalg::solvers::Llt<f64>,
    source_integrals:Vec<[f64;9]>,
}
pub struct Operator<'a> {
    mesh:&'a LayeredMesh,parallelism:Par,
    free:usize,
    cell_dofs:Vec<Vec<usize>>,cell_basis:Vec<Vec<usize>>,degree:usize,partition_count:usize,indices:Vec<[usize;4]>,
    terms:SparseColMat<usize,f64>,
    triangular:bool,
    symbolic:Option<cholesky::SymbolicCholesky<usize>>,values:Vec<f64>,
    pub planned_bytes:usize,
    enrichment:Option<Enrichment>,
    pub enrichment_radius_m:Option<f64>,
}
struct PolynomialSpace {
 indices:Vec<[usize;4]>,cell_basis:Vec<Vec<usize>>,cell_dofs:Vec<Vec<usize>>,free:usize,
 trace_degree:usize,body_degree:usize,partition_count:usize,size:usize,count:usize,retained:usize,template_bytes:usize,coordinate_plan:usize,
 weights:Vec<[f64;16]>,cells:Vec<([usize;4],f64,[[f64;3];4])>,
}
impl<'a> Operator<'a> {
    pub fn new(mesh:&'a LayeredMesh,budget:AllocationBudget)->Result<Self> {
        let n=mesh.nodes_m.len();let nt=mesh.tetrahedra.len()+mesh.copper_tetrahedra.len();
        // Bound the first map phase before allocating it. Once its actual
        // graph is known, plan matrix construction/fill separately; extrusion
        // face maps and the worst-case16 entries/tet are not all live factors.
        let retained=mesh.retained_bytes()?;
        let base=retained.checked_add(n.checked_mul(2048).ok_or_else(||error("MEMORY_LIMIT","magnetic nodes overflow"))?).and_then(|v|v.checked_add(nt.checked_mul(16*80)?)).ok_or_else(||error("MEMORY_LIMIT","magnetic plan overflow"))?.max(mesh.planned_peak_bytes);
        budget.check(base)?;
        if n>budget.max_nodes || nt>budget.max_tetrahedra {return Err(error("MEMORY_LIMIT","magnetic mesh exceeds budget"));}
        let fixed:BTreeSet<_>=mesh.external_faces.iter().flatten().copied().collect();
        let mut free=0;let map:Vec<_>=(0..n).map(|i|if fixed.contains(&i){usize::MAX}else{let j=free;free+=1;j}).collect();
        if free==0 {return Err(error("MAGNETIC_MESH","no internal Newton-potential coordinates"));}
        let mut entries=BTreeMap::new();
        for (nodes,volume,gradients) in mesh.tetrahedra.iter().map(|t|(t.nodes,t.volume_m3,t.gradients_per_m)).chain(mesh.copper_tetrahedra.iter().map(|t|(t.nodes,t.volume_m3,t.gradients_per_m))) {
            for a in 0..4 {for b in 0..4 {let i=map[nodes[a]];let j=map[nodes[b]];if i!=usize::MAX&&j!=usize::MAX {
                *entries.entry((i,j)).or_insert(0.)+=volume*(0..3).map(|k|gradients[a][k]*gradients[b][k]).sum::<f64>();
            }}}
        }
        let terms:Vec<_>=entries.into_iter().map(|((i,j),v)|(i,j,v)).collect();
        let assembled=retained.checked_add(n.checked_mul(2048).ok_or_else(||error("MEMORY_LIMIT","magnetic graph overflow"))?).and_then(|v|v.checked_add(terms.len().checked_mul(192)?)).ok_or_else(||error("MEMORY_LIMIT","magnetic graph plan overflow"))?;
        budget.check(assembled)?;
        let trips:Vec<_>=terms.iter().map(|&(i,j,v)|Triplet::new(i,j,v)).collect();
        let matrix=SparseColMat::<usize,f64>::try_new_from_triplets(free,free,&trips).map_err(|e|error("MAGNETIC_MATRIX",&e.to_string()))?;
        drop(trips);
        let symbolic=cholesky::factorize_symbolic_cholesky(matrix.symbolic(),Side::Lower,Default::default(),Default::default()).map_err(|e|error("MAGNETIC_MATRIX",&e.to_string()))?;
        let scratch=symbolic.factorize_numeric_llt_scratch::<f64>(super::parallel::current(),Default::default());
        let solve=symbolic.solve_in_place_scratch::<f64>(3,super::parallel::current());
        let plan=assembled.checked_add(symbolic.len_val().checked_mul(16).ok_or_else(||error("MEMORY_LIMIT","magnetic fill overflow"))?).and_then(|v|v.checked_add(scratch.unaligned_bytes_required().max(solve.unaligned_bytes_required()))).ok_or_else(||error("MEMORY_LIMIT","magnetic workspace overflow"))?.max(base);
        budget.check(plan)?;
        let mut values=vec![0.;symbolic.len_val()];let mut workspace=MemBuffer::try_new(scratch).map_err(|_|error("MEMORY_LIMIT","magnetic factor workspace"))?;
        symbolic.factorize_numeric_llt(&mut values,matrix.as_ref(),Side::Lower,Default::default(),super::parallel::current(),MemStack::new(&mut workspace),Default::default()).map_err(|_|error("MAGNETIC_SINGULAR","finite-box Laplacian not positive definite; no regularization"))?;
        let cell_dofs:Vec<Vec<usize>>=mesh.tetrahedra.iter().map(|t|t.nodes).chain(mesh.copper_tetrahedra.iter().map(|t|t.nodes)).map(|nodes|nodes.into_iter().map(|i|map[i]).collect()).collect();
        let cell_basis=cell_dofs.iter().map(|_|vec![0,1,2,3]).collect();
        Ok(Self{mesh,parallelism:super::parallel::current(),free,cell_dofs,cell_basis,degree:1,partition_count:4,indices:vec![[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]],terms:matrix,triangular:false,symbolic:Some(symbolic),values,planned_bytes:plan,enrichment:None,enrichment_radius_m:None})
    }
    pub fn new_enriched(mesh:&'a LayeredMesh,budget:AllocationBudget,interface_order:usize)->Result<Self> {
        Self::new_enriched_degree(mesh,budget,interface_order,1)
    }
    fn new_degree(mesh:&'a LayeredMesh,budget:AllocationBudget,degree:usize)->Result<Self> {
        Self::new_polynomials(mesh,budget,degree,None)
    }
    fn polynomial_space(mesh:&LayeredMesh,budget:AllocationBudget,degree:usize,bubbles:Option<(usize,usize,usize)>)->Result<PolynomialSpace>{
        if mesh.nodes_m.len()>budget.max_nodes||mesh.tetrahedra.len()+mesh.copper_tetrahedra.len()>budget.max_tetrahedra{return Err(error("MEMORY_LIMIT","physical mesh exceeds potential budget"));}
        if ![2,3,4].contains(&degree){return Err(error("MAGNETIC_DEGREE","source-resolving Bernstein degrees2/3/4 required"));}
        if bubbles.is_some_and(|(t,p,l)|![2,3].contains(&degree)||!(3..=6).contains(&t)||!(4..=8).contains(&p)||!(1..=3).contains(&l)){return Err(error("MAGNETIC_DEGREE","P3 air, Cu traces3..6, interior bubbles4..8 and1..3 vertex stars required"));}
        let (trace_degree,body_degree,stars)=bubbles.unwrap_or((degree,degree,0));
        let mut indices=vec![[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]];let partition_count=4;
        indices.extend(super::bernstein::indices(degree).into_iter().filter(|a|a.iter().filter(|&&i|i>0).count()>1));
        if trace_degree!=degree {indices.extend(super::bernstein::indices(trace_degree).into_iter().filter(|a|a.iter().filter(|&&i|i>0).count()>1));}
        if body_degree!=degree {for a in super::bernstein::indices(body_degree).into_iter().filter(|a|a.iter().all(|&i|i>0)){if !indices.contains(&a){indices.push(a);}}}
        let size=indices.len();
        let count=mesh.tetrahedra.len()+mesh.copper_tetrahedra.len();
        let retained=mesh.retained_bytes()?;
        let template_bytes=size.checked_mul(size).and_then(|n|n.checked_mul(128)).ok_or_else(||error("MEMORY_LIMIT","Bernstein template plan overflow"))?;
        let coordinate_plan=retained.checked_add(template_bytes).and_then(|n|n.checked_add(count.checked_mul(size)?.checked_mul(256)?)).ok_or_else(||error("MEMORY_LIMIT","Bernstein coordinate plan overflow"))?;budget.check(coordinate_plan)?;
        let weights=super::bernstein::mixed_stiffness_weights(&indices)?;
        let cells=mesh.tetrahedra.iter().map(|t|(t.nodes,t.volume_m3,t.gradients_per_m)).chain(mesh.copper_tetrahedra.iter().map(|t|(t.nodes,t.volume_m3,t.gradients_per_m))).collect::<Vec<_>>();
        let mut copper_vertices=mesh.copper_tetrahedra.iter().flat_map(|t|t.nodes).collect::<BTreeSet<_>>();
        for _ in 1..stars {let added=cells.iter().filter(|(nodes,_,_)|nodes.iter().any(|n|copper_vertices.contains(n))).flat_map(|(nodes,_,_)|*nodes).collect::<Vec<_>>();copper_vertices.extend(added);}
        let cell_degrees=cells.iter().map(|(nodes,_,_)|if nodes.iter().any(|n|copper_vertices.contains(n)){trace_degree}else{degree}).collect::<Vec<_>>();
        let mut entity_degrees=BTreeMap::new();
        for (ci,&(nodes,_,_)) in cells.iter().enumerate() {for mask in 1usize..16 {if [2,3].contains(&mask.count_ones()) {let mut entity:Vec<_>=(0..4).filter(|i|mask&(1<<i)!=0).map(|i|nodes[i]).collect();entity.sort();entity_degrees.entry(entity).and_modify(|p:&mut usize|*p=(*p).min(cell_degrees[ci])).or_insert(cell_degrees[ci]);}}}
        let mut cell_basis=vec![];let mut matrix_slots=0usize;
        for (ci,&(nodes,_,_)) in cells.iter().enumerate() {
            let mut row=vec![];for(a,alpha)in indices.iter().enumerate() {
                let mut entity:Vec<_>=(0..4).filter(|&i|alpha[i]>0).map(|i|nodes[i]).collect();entity.sort();let support=entity.len();let p=alpha.iter().sum::<usize>();
                let desired=if support==1 {1}else if support==4 {if ci>=mesh.tetrahedra.len(){body_degree}else{cell_degrees[ci]}}else{entity_degrees[&entity]};
                if p==desired {row.push(a);}
            }
            matrix_slots=matrix_slots.checked_add(row.len().checked_mul(row.len()).ok_or_else(||error("MEMORY_LIMIT","Bernstein cell matrix overflow"))?).ok_or_else(||error("MEMORY_LIMIT","Bernstein matrix count overflow"))?;cell_basis.push(row);
        }
        // The local dense slots are an operation count, not simultaneously
        // retained sparse entries. Admit the deduplicated graph incrementally.
        let _=matrix_slots;
        let key=|nodes:[usize;4],a:[usize;4]| {let mut out=[usize::MAX;8];let mut k=0;for i in 0..4 {for _ in 0..a[i] {out[k]=nodes[i];k+=1;}}out.sort();out};
        let mut fixed=BTreeSet::new();
        for face in &mesh.external_faces {for &a in &indices {if a[3]==0 {fixed.insert(key([face[0],face[1],face[2],usize::MAX],a));}}}
        let mut ids=BTreeMap::new();let mut cell_dofs=vec![];let mut free=0;
        for (ci,&(nodes,_,_)) in cells.iter().enumerate() {let mut row=vec![];for &index in &cell_basis[ci] {let a=indices[index];
            let k=key(nodes,a);let value=*ids.entry(k).or_insert_with(||if fixed.contains(&k){usize::MAX}else{let index=free;free+=1;index});row.push(value);
        }cell_dofs.push(row);}
        if free==0 {return Err(error("MAGNETIC_MESH","no free polynomial potential coordinates"));}
        drop(ids);
        drop(entity_degrees);drop(copper_vertices);drop(cell_degrees);
        Ok(PolynomialSpace{indices,cell_basis,cell_dofs,free,trace_degree,body_degree,partition_count,size,count,retained,template_bytes,coordinate_plan,weights,cells})
    }
    /// Read-only developer audit. Uses exactly the production coordinate map,
    /// stopping before sparse graph assembly, ordering or factorization.
    pub fn audit_patch_coordinates(mesh:&LayeredMesh,budget:AllocationBudget,air:usize,trace:usize,body:usize,stars:usize)->Result<serde_json::Value>{
        let space=Self::polynomial_space(mesh,budget,air,Some((trace,body,stars)))?;
        let private=space.cell_dofs.iter().zip(&space.cell_basis).flat_map(|(d,b)|d.iter().zip(b).filter_map(|(&i,&a)|(i!=usize::MAX&&space.indices[a].iter().all(|&v|v>0)).then_some(i))).collect::<BTreeSet<_>>();
        let copper=mesh.copper_tetrahedra.iter().flat_map(|t|t.nodes).collect::<BTreeSet<_>>();
        let near=space.cells.iter().zip(&space.cell_dofs).filter(|(c,_)|c.0.iter().any(|i|copper.contains(i))).flat_map(|(_,d)|d.iter().copied()).filter(|&i|i!=usize::MAX&&!private.contains(&i)).collect::<BTreeSet<_>>();
        Ok(serde_json::json!({"unknowns":space.free,"air":space.free-near.len()-private.len(),"nearCopperTrace":near.len(),"cellPrivate":private.len(),"tetrahedra":space.count,"nodes":mesh.nodes_m.len(),"phase":"coordinates-only; no sparse matrix or factor"}))
    }
    fn new_polynomials(mesh:&'a LayeredMesh,budget:AllocationBudget,degree:usize,bubbles:Option<(usize,usize,usize)>)->Result<Self> {
        let PolynomialSpace{indices,cell_basis,cell_dofs,free,trace_degree,body_degree,partition_count,size,count,retained,template_bytes,coordinate_plan,weights,cells}=Self::polynomial_space(mesh,budget,degree,bubbles)?;
        let slots=cell_dofs.iter().try_fold(0usize,|n,d|n.checked_add(d.len())).ok_or_else(||error("MEMORY_LIMIT","potential incidence count overflow"))?;
        let owned=retained.checked_add(slots.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT","potential trace storage overflow"))?).and_then(|n|n.checked_add(count.checked_mul(256)?)).and_then(|n|n.checked_add(size.checked_mul(64)?)).ok_or_else(||error("MEMORY_LIMIT","potential retained plan overflow"))?;
        let incidence_plan=owned.checked_add(template_bytes).and_then(|n|n.checked_add(slots.checked_mul(16)?)).and_then(|n|n.checked_add(free.checked_mul(40)?)).ok_or_else(||error("MEMORY_LIMIT","potential adjacency plan overflow"))?;budget.check(incidence_plan)?;
        let mut incident=vec![Vec::new();free];for (cell,dofs) in cell_dofs.iter().enumerate(){for &j in dofs {if j!=usize::MAX {incident[j].push(cell);}}}
        let temporary=incident.iter().map(|cells|cells.iter().map(|&c|cell_dofs[c].len()).sum::<usize>()).max().unwrap_or(0);
        let graph_base=incidence_plan.checked_add(temporary.checked_mul(8).ok_or_else(||error("MEMORY_LIMIT","potential column workspace overflow"))?).ok_or_else(||error("MEMORY_LIMIT","potential graph workspace overflow"))?;budget.check(graph_base)?;
        let mut buffer=Vec::with_capacity(temporary);let mut pointer=vec![0usize;free+1];let mut graph_plan=coordinate_plan.max(graph_base);
        for j in 0..free {buffer.clear();for &cell in &incident[j]{buffer.extend(cell_dofs[cell].iter().copied().filter(|&i|i<=j));}buffer.sort_unstable();buffer.dedup();pointer[j+1]=pointer[j].checked_add(buffer.len()).ok_or_else(||error("MEMORY_LIMIT","potential sparse count overflow"))?;
            let phase=graph_base.checked_add(pointer[j+1].checked_mul(16).ok_or_else(||error("MEMORY_LIMIT","potential CSC plan overflow"))?).ok_or_else(||error("MEMORY_LIMIT","potential CSC plan overflow"))?;budget.check(phase)?;graph_plan=graph_plan.max(phase);
        }
        let nnz=pointer[free];let mut rows=vec![0usize;nnz];let mut numeric=vec![0.;nnz];
        for j in 0..free {buffer.clear();for &cell in &incident[j]{buffer.extend(cell_dofs[cell].iter().copied().filter(|&i|i<=j));}buffer.sort_unstable();buffer.dedup();rows[pointer[j]..pointer[j+1]].copy_from_slice(&buffer);}
        drop(incident);drop(buffer);
        for (ci,&(_,volume,g)) in cells.iter().enumerate() {for a in 0..cell_dofs[ci].len() {let i=cell_dofs[ci][a];if i==usize::MAX {continue;}for b in 0..cell_dofs[ci].len() {let j=cell_dofs[ci][b];if j==usize::MAX||i>j {continue;}
            let value=volume*(0..4).flat_map(|u|(0..4).map(move|v|(u,v))).map(|(u,v)|weights[cell_basis[ci][a]*size+cell_basis[ci][b]][4*u+v]*(0..3).map(|k|g[u][k]*g[v][k]).sum::<f64>()).sum::<f64>();
            if !value.is_finite(){return Err(error("MAGNETIC_MATRIX","nonfinite local Bernstein stiffness"));}
            let start=pointer[j];let end=pointer[j+1];let position=start+rows[start..end].binary_search(&i).unwrap();numeric[position]+=value;
        }}}
        let matrix=SparseColMat::new(SymbolicSparseColMat::new_checked(free,free,pointer,None,rows),numeric);
        drop(cells);drop(weights);
        let private=cell_dofs.iter().zip(&cell_basis).flat_map(|(dofs,basis)|dofs.iter().zip(basis).filter_map(|(&i,&a)|(i!=usize::MAX&&indices[a].iter().all(|&v|v>0)).then_some(i))).collect::<Vec<_>>();
        let (factor,values,plan,persistent)=super::sparse_potential::factor(&matrix,owned,budget,&private)?;
        let symbolic=Some(factor);
        drop(private);
        let runtime=persistent.checked_add(free.checked_mul(512).ok_or_else(||error("MEMORY_LIMIT","potential action workspace overflow"))?).and_then(|n|n.checked_add(mesh.copper_tetrahedra.len().checked_mul(512)?)).ok_or_else(||error("MEMORY_LIMIT","potential runtime plan overflow"))?;budget.check(runtime)?;
        Ok(Self{mesh,parallelism:super::parallel::current(),free,cell_dofs,cell_basis,degree:body_degree.max(trace_degree),partition_count,indices,terms:matrix,triangular:true,symbolic,values,planned_bytes:plan.max(graph_plan).max(runtime),enrichment:None,enrichment_radius_m:None})
    }
    pub fn new_enriched_degree(mesh:&'a LayeredMesh,budget:AllocationBudget,interface_order:usize,degree:usize)->Result<Self> {
        if ![4,8,16].contains(&interface_order) {return Err(error("MAGNETIC_QUADRATURE","enrichment interface orders4/8/16 required"));}
        let out=if degree==1 {Self::new(mesh,budget)?}else{Self::new_degree(mesh,budget,degree)?};
        Self::enrich(out,budget,interface_order)
    }
    pub fn new_enriched_bubbles(mesh:&'a LayeredMesh,budget:AllocationBudget,interface_order:usize,bubble_degree:usize)->Result<Self> {
        if ![4,8,16].contains(&interface_order){return Err(error("MAGNETIC_QUADRATURE","enrichment interface orders4/8/16 required"));}
        Self::new_enriched_trace_bubbles(mesh,budget,interface_order,3,bubble_degree)
    }
    pub fn new_enriched_trace_bubbles(mesh:&'a LayeredMesh,budget:AllocationBudget,interface_order:usize,trace_degree:usize,bubble_degree:usize)->Result<Self> {
        Self::new_enriched_patch(mesh,budget,interface_order,trace_degree,bubble_degree,1)
    }
    /// Explicit developer air-degree experiment; geometry and source trace are unchanged.
    pub fn new_enriched_patch_air(mesh:&'a LayeredMesh,budget:AllocationBudget,interface_order:usize,trace_degree:usize,bubble_degree:usize,stars:usize,air_degree:usize)->Result<Self>{
        if ![4,8,16].contains(&interface_order){return Err(error("MAGNETIC_QUADRATURE","enrichment interface orders4/8/16 required"));}
        Self::enrich(Self::new_polynomials(mesh,budget,air_degree,Some((trace_degree,bubble_degree,stars)))?,budget,interface_order)
    }
    pub fn new_enriched_patch(mesh:&'a LayeredMesh,budget:AllocationBudget,interface_order:usize,trace_degree:usize,bubble_degree:usize,stars:usize)->Result<Self> {
        if ![4,8,16].contains(&interface_order){return Err(error("MAGNETIC_QUADRATURE","enrichment interface orders4/8/16 required"));}
        Self::enrich(Self::new_polynomials(mesh,budget,3,Some((trace_degree,bubble_degree,stars)))?,budget,interface_order)
    }
    fn solve(&self,rhs:&mut Mat<f64>)->Result<()> {
        if let Some(symbolic)=&self.symbolic {let mut workspace=MemBuffer::try_new(symbolic.solve_in_place_scratch::<f64>(rhs.ncols(),self.parallelism)).map_err(|_|error("MEMORY_LIMIT","magnetic solve workspace"))?;cholesky::LltRef::new(symbolic,&self.values).solve_in_place_with_conj(Conj::No,rhs.as_mut(),self.parallelism,MemStack::new(&mut workspace));Ok(())}
        else {Err(error("MAGNETIC_MATRIX","missing direct factor"))}
    }
    fn enrich(mut out:Self,budget:AllocationBudget,interface_order:usize)->Result<Self> {
        let mesh=out.mesh;
        let scratch=out.symbolic.as_ref().map_or(faer::dyn_stack::StackReq::empty(),|s|s.solve_in_place_scratch::<f64>(9,out.parallelism));
        let plan=out.planned_bytes.checked_add(out.free.checked_mul(9*24).ok_or_else(||error("MEMORY_LIMIT","exterior basis plan overflow"))?).and_then(|v|v.checked_add(mesh.copper_tetrahedra.len().checked_mul(9*8)?)).and_then(|v|v.checked_add(scratch.unaligned_bytes_required())).ok_or_else(||error("MEMORY_LIMIT","exterior plan overflow"))?;
        budget.check(plan)?;
        let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];
        for t in &mesh.copper_tetrahedra {for node in t.nodes {for k in 0..3 {lo[k]=lo[k].min(mesh.nodes_m[node][k]);hi[k]=hi[k].max(mesh.nodes_m[node][k]);}}}
        let center=std::array::from_fn::<_,3,_>(|k|(lo[k]+hi[k])/2.);
        let radius=mesh.copper_tetrahedra.iter().flat_map(|t|t.nodes).map(|n|(0..3).map(|k|(mesh.nodes_m[n][k]-center[k]).powi(2)).sum::<f64>().sqrt()).fold(0.,f64::max)*1.05;
        if !radius.is_finite()||radius<=0. {return Err(error("MAGNETIC_EXTERIOR","nonempty finite copper extent required"));}
        let low_rules=(4..=((out.degree+6)/2).max(4)).map(super::prism::GaussRule::new).collect::<Result<Vec<_>>>()?;let high=super::prism::GaussRule::new(interface_order)?;
        let mut coupling=Mat::from_fn(out.free,9,|_,_|0.);
        for (cell,(nodes,volume,gradients)) in mesh.tetrahedra.iter().map(|t|(t.nodes,t.volume_m3,t.gradients_per_m)).chain(mesh.copper_tetrahedra.iter().map(|t|(t.nodes,t.volume_m3,t.gradients_per_m))).enumerate() {
            let positions=nodes.map(|n|mesh.nodes_m[n]);
            let inside=positions.iter().all(|p|(0..3).map(|k|(p[k]-center[k]).powi(2)).sum::<f64>()<=radius*radius);
            let count=out.cell_dofs[cell].len();let mut integral=vec![[0.;9];count];
            let local_indices=out.cell_basis[cell].iter().map(|&i|out.indices[i]).collect::<Vec<_>>();
            let derivative=super::bernstein::Gradients::new(&local_indices);let mut basis=vec![[0.;3];count];
            let local_degree=local_indices.iter().map(|a|a.iter().sum::<usize>()).max().unwrap();
            let rule=if !inside {&high}else{&low_rules[((local_degree+6)/2).max(4)-4]};
            super::polyhedron::tetra_integrate(positions,volume,rule,|p,w| {
                let (_,g)=super::spherical::values_gradients(p,center,radius);
                let mut lambda=[0.;4];for i in 1..4 {lambda[i]=(0..3).map(|k|gradients[i][k]*(p[k]-positions[0][k])).sum();}lambda[0]=1.-lambda[1]-lambda[2]-lambda[3];
                derivative.at(lambda,gradients,&mut basis);
                for i in 0..basis.len() {for j in 0..9 {integral[i][j]+=w*(0..3).map(|k|basis[i][k]*g[j][k]).sum::<f64>();}}Ok(())
            })?;
            for j in 0..9 {integral[0][j]=-(1..out.partition_count).map(|i|integral[i][j]).sum::<f64>();}
            for i in 0..integral.len() {let row=out.cell_dofs[cell][i];if row!=usize::MAX {for j in 0..9 {coupling[(row,j)]+=integral[i][j];}}}
        }
        let mut source_integrals=vec![];
        for t in &mesh.copper_tetrahedra {
            let mut integral=[0.;9];
            super::polyhedron::tetra_integrate(t.nodes.map(|n|mesh.nodes_m[n]),t.volume_m3,&low_rules[0],|p,w| {let (psi,_)=super::spherical::values_gradients(p,center,radius);for j in 0..9 {integral[j]+=w*psi[j];}Ok(())})?;
            source_integrals.push(integral);
        }
        let mut images=coupling.clone();out.solve(&mut images)?;
        let gram=super::spherical::energies(radius);
        let schur=Mat::from_fn(9,9,|i,j|if i==j {gram[i]}else{0.}-(0..out.free).map(|k|coupling[(k,i)]*images[(k,j)]).sum::<f64>());
        let scale=gram.iter().copied().fold(0.,f64::max);
        for i in 0..9 {for j in 0..9 {if !schur[(i,j)].is_finite()||(schur[(i,j)]-schur[(j,i)]).abs()>1e-9*scale {return Err(error("MAGNETIC_EXTERIOR","unresolved directed exterior Schur reciprocity"));}}}
        let factor=schur.llt(Side::Lower).map_err(|_|error("MAGNETIC_EXTERIOR","exterior Schur not positive definite; refine quadrature/mesh, no regularization"))?;
        out.enrichment=Some(Enrichment{coupling,images,gram,factor,source_integrals});out.enrichment_radius_m=Some(radius);out.planned_bytes=plan;Ok(out)
    }
    pub fn apply(&self,space:&Space,current:&[f64])->Result<Vec<f64>> {
        if current.len()!=space.current_count()||current.iter().any(|x|!x.is_finite()) {return Err(error("MAGNETIC_INPUT","finite full current vector required"));}
        let mut load=vec![Sum::default();self.free*3];let mut exterior_load=[Sum::default();27];
        for(i,c)in space.cells.iter().enumerate() {
            let t=&self.mesh.copper_tetrahedra[c.mesh_cell];let dofs=&self.cell_dofs[self.mesh.tetrahedra.len()+c.mesh_cell];
            for(a,&j)in dofs.iter().enumerate() {let p=self.indices[self.cell_basis[self.mesh.tetrahedra.len()+c.mesh_cell][a]].iter().sum::<usize>();let weight=t.volume_m3*c.density_per_amp_m2*6./((p+1)*(p+2)*(p+3))as f64;if j!=usize::MAX {for k in 0..3 {load[3*j+k]=load[3*j+k].add(Sum::from(current[3*i+k]).mul(weight));}}}
            if let Some(e)=&self.enrichment {for j in 0..9 {for k in 0..3 {exterior_load[3*j+k]=exterior_load[3*j+k].add(Sum::from(current[3*i+k]).mul(e.source_integrals[c.mesh_cell][j]*c.density_per_amp_m2));}}}
        }
        let block_solve=|load:&[Sum],exterior_load:&[Sum]|->Result<(Mat<f64>,Mat<f64>)> {
            let mut rhs=Mat::from_fn(self.free,3,|i,k|load[3*i+k].value());
            self.solve(&mut rhs)?;
            let mut exterior=Mat::from_fn(9,3,|_,_|0.);
            if let Some(e)=&self.enrichment {
                for j in 0..9 {for k in 0..3 {exterior[(j,k)]=exterior_load[3*j+k].value()-(0..self.free).map(|i|e.coupling[(i,j)]*rhs[(i,k)]).sum::<f64>();}}
                exterior=e.factor.solve(&exterior);
                for i in 0..self.free {for k in 0..3 {rhs[(i,k)]-=(0..9).map(|j|e.images[(i,j)]*exterior[(j,k)]).sum::<f64>();}}
            }Ok((rhs,exterior))
        };
        let (initial,initial_exterior)=block_solve(&load,&exterior_load)?;
        let mut field:Vec<_>=(0..self.free*3).map(|i|Sum::from(initial[(i/3,i%3)])).collect();
        let mut exterior:Vec<_>=(0..27).map(|i|Sum::from(initial_exterior[(i/3,i%3)])).collect();
        drop(initial);drop(initial_exterior);
        let rhs_norm=load.iter().chain(exterior_load.iter()).map(|x|x.value().powi(2)).sum::<f64>().sqrt();
        let mut global_error=0.;let mut row_error:f64=0.;
        for step in 0..=8 {
            let mut residual:Vec<_>=load.iter().map(|x|x.neg()).collect();
            let mut scale:Vec<_>=load.iter().map(|x|x.value().abs()).collect();
            let mut exterior_residual:Vec<_>=exterior_load.iter().map(|x|x.neg()).collect();
            let mut exterior_scale:Vec<_>=exterior_load.iter().map(|x|x.value().abs()).collect();
            for entry in self.terms.triplet_iter() {let(i,j,value)=(entry.row,entry.col,*entry.val);for k in 0..3 {
                let term=field[3*j+k].mul(value);residual[3*i+k]=residual[3*i+k].add(term);scale[3*i+k]+=term.value().abs();
                if self.triangular&&i!=j {let term=field[3*i+k].mul(value);residual[3*j+k]=residual[3*j+k].add(term);scale[3*j+k]+=term.value().abs();}
            }}
            if let Some(e)=&self.enrichment {
                for j in 0..9 {for k in 0..3 {
                    let term=exterior[3*j+k].mul(e.gram[j]);exterior_residual[3*j+k]=exterior_residual[3*j+k].add(term);exterior_scale[3*j+k]+=term.value().abs();
                    for i in 0..self.free {
                        let term=exterior[3*j+k].mul(e.coupling[(i,j)]);residual[3*i+k]=residual[3*i+k].add(term);scale[3*i+k]+=term.value().abs();
                        let term=field[3*i+k].mul(e.coupling[(i,j)]);exterior_residual[3*j+k]=exterior_residual[3*j+k].add(term);exterior_scale[3*j+k]+=term.value().abs();
                    }
                }}
            }
            global_error=residual.iter().chain(exterior_residual.iter()).map(|x|x.value().powi(2)).sum::<f64>().sqrt()/rhs_norm.max(f64::MIN_POSITIVE);
            row_error=residual.iter().zip(&scale).chain(exterior_residual.iter().zip(&exterior_scale)).map(|(r,s)|r.value().abs()/s.max(f64::MIN_POSITIVE)).fold(0.,f64::max);
            if global_error<=1e-14||step==8 {break;}
            let negative:Vec<_>=residual.iter().map(|x|x.neg()).collect();let negative_exterior:Vec<_>=exterior_residual.iter().map(|x|x.neg()).collect();
            let(correction,correction_exterior)=block_solve(&negative,&negative_exterior)?;
            for i in 0..field.len() {field[i]=field[i].add(Sum::from(correction[(i/3,i%3)]));}
            for i in 0..27 {exterior[i]=exterior[i].add(Sum::from(correction_exterior[(i/3,i%3)]));}
        }
        if row_error>1e-8||global_error>1e-14||!row_error.is_finite()||!global_error.is_finite() {return Err(error("MAGNETIC_RESIDUAL",&format!("row={row_error:e}, refined RHS-relative={global_error:e}")));}
        let mut out=vec![Sum::default();space.current_count()];
        for(i,c)in space.cells.iter().enumerate() {
            let t=&self.mesh.copper_tetrahedra[c.mesh_cell];let dofs=&self.cell_dofs[self.mesh.tetrahedra.len()+c.mesh_cell];
            for(a,&j)in dofs.iter().enumerate() {let p=self.indices[self.cell_basis[self.mesh.tetrahedra.len()+c.mesh_cell][a]].iter().sum::<usize>();let weight=t.volume_m3*c.density_per_amp_m2*6./((p+1)*(p+2)*(p+3))as f64;if j!=usize::MAX {for k in 0..3 {out[3*i+k]=out[3*i+k].add(field[3*j+k].mul(weight));}}}
            if let Some(e)=&self.enrichment {for j in 0..9 {for k in 0..3 {out[3*i+k]=out[3*i+k].add(exterior[3*j+k].mul(e.source_integrals[c.mesh_cell][j]*c.density_per_amp_m2));}}}
        }
        let mu0=4e-7*std::f64::consts::PI;let out:Vec<_>=out.into_iter().map(|x|x.mul(mu0).value()).collect();
        if out.iter().any(|x|!x.is_finite()) {return Err(error("MAGNETIC_RANGE","nonfinite Newton action"));}Ok(out)
    }
}
