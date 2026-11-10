//! Three-dimensional mixed P1-potential / constant-vector-current conductor.
//! Copper ownership comes from the same conforming mesh as the dielectric.
use super::{error, mesh::LayeredMesh, AllocationBudget, Result};
use super::accurate::Sum;
use std::collections::{BTreeMap, BTreeSet};
use faer::{dyn_stack::{MemBuffer,MemStack},sparse::{linalg::cholesky,SparseColMat,Triplet},Conj,Mat,Par,Side};

#[derive(Debug)]
pub struct Cell {
    pub mesh_cell: usize,
    pub potentials: [usize; 4],
    pub incidence: [[f64; 3]; 4],
    pub density_per_amp_m2: f64,
    pub resistance_ohm: f64,
}
#[derive(Debug)]
pub struct Space {
    pub mesh_nodes: Vec<Vec<usize>>,
    pub node_potential: BTreeMap<usize, usize>,
    pub cells: Vec<Cell>,
    pub components: Vec<Vec<usize>>,
    pub planned_bytes: usize,
}
fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i { i = parent[i]; }
    i
}
/// Each electrode lists an actual finite set of mesh vertices. Quotients apply
/// only to explicitly supplied electrodes, never to source IDs or net names.
pub fn assemble(mesh: &LayeredMesh, conductivity_s_m: &[f64], electrodes: &[Vec<usize>], budget: AllocationBudget) -> Result<Space> {
    let nt = mesh.copper_tetrahedra.len();
    let bytes = mesh.nodes_m.len().checked_mul(256)
        .and_then(|x| x.checked_add(nt.checked_mul(2048)?))
        .and_then(|x| x.checked_add(mesh.planned_peak_bytes))
        .ok_or_else(|| error("MEMORY_LIMIT", "volume current plan overflow"))?;
    budget.check(bytes)?;
    if nt == 0 || nt > budget.max_tetrahedra || mesh.nodes_m.len() > budget.max_nodes || conductivity_s_m.len() != nt || conductivity_s_m.iter().any(|x| !x.is_finite() || *x <= 0.) {
        return Err(error("VOLUME_INPUT", "nonempty copper and positive per-cell conductivity required"));
    }
    // First connect volumes by faces, then detect vertex/edge-only contacts.
    // A nodal FEM on touching disconnected volumes would invent an ideal short.
    let mut cell_parent: Vec<_> = (0..nt).collect();
    let mut faces = BTreeMap::<[usize; 3], usize>::new();
    for (i, t) in mesh.copper_tetrahedra.iter().enumerate() {
        for skip in 0..4 {
            let mut face = [0; 3];
            let mut at = 0;
            for k in 0..4 { if k != skip { face[at] = t.nodes[k]; at += 1; } }
            face.sort();
            if let Some(&j) = faces.get(&face) {
                let a = root(&cell_parent, i); let b = root(&cell_parent, j); cell_parent[b] = a;
            } else { faces.insert(face, i); }
        }
    }
    // A mesh vertex shared by two face-disconnected volumes is a point contact (union noise or a
    // true pinch). It carries no current: each volume gets its own potential at that vertex, so
    // no ideal short is invented. The count is reported.
    let mut node_component = BTreeMap::new();
    let mut shared_nodes = BTreeSet::new();
    for (i,t) in mesh.copper_tetrahedra.iter().enumerate() {
        let component = root(&cell_parent, i);
        for node in t.nodes {
            match node_component.get(&node) {
                Some(&old) if old != component => { shared_nodes.insert(node); }
                Some(_) => {}
                None => { node_component.insert(node, component); }
            }
        }
    }
    if !shared_nodes.is_empty() {
        eprintln!("P7_VOLUME_CONTACT_SPLIT vertices={} (copper touching only at a vertex or edge: no galvanic contact)", shared_nodes.len());
    }
    let mut parent: Vec<_> = (0..mesh.nodes_m.len()).collect();
    let mut electrode_nodes = BTreeSet::new();
    for electrode in electrodes {
        if electrode.len() < 3 || electrode.iter().any(|i| !node_component.contains_key(i) || !electrode_nodes.insert(*i)) {
            return Err(error("VOLUME_ELECTRODE", "distinct finite copper electrodes required"));
        }
        let component = node_component[&electrode[0]];
        if electrode.iter().any(|i| node_component[i] != component) {
            return Err(error("VOLUME_ELECTRODE", "electrode bridges disconnected copper"));
        }
        // At least one complete copper boundary triangle, rather than an
        // arbitrary collection of collinear points with zero contact area.
        let nodes: BTreeSet<_> = electrode.iter().copied().collect();
        let mut boundary=BTreeMap::new();
        for t in &mesh.copper_tetrahedra {for skip in 0..4 {let mut face=[0;3];let mut at=0;for k in 0..4 {if k!=skip {face[at]=t.nodes[k];at+=1;}}face.sort();*boundary.entry(face).or_insert(0usize)+=1;}}
        let finite_face = boundary.iter().any(|(f,&count)|count==1&&f.iter().all(|i|nodes.contains(i)));
        if !finite_face { return Err(error("VOLUME_ELECTRODE", "electrode has no finite boundary face")); }
        for &node in electrode { parent[node] = electrode[0]; }
    }
    let mut ids = BTreeMap::new();
    let mut mesh_nodes: Vec<Vec<usize>> = vec![];
    let mut node_potential = BTreeMap::new();
    for &node in node_component.keys() {
        let key = root(&parent, node);
        let next = ids.len();
        let p = *ids.entry(key).or_insert_with(|| { mesh_nodes.push(vec![]); next });
        mesh_nodes[p].push(node); node_potential.insert(node,p);
    }
    let mut components = BTreeMap::<usize,BTreeSet<usize>>::new();
    let mut extra_potential = BTreeMap::<(usize,usize),usize>::new();
    let mut cells = vec![];
    for (i,t) in mesh.copper_tetrahedra.iter().enumerate() {
        let length = t.volume_m3.cbrt();
        let density = 1. / (length * length);
        let resistance = 1. / (conductivity_s_m[i] * length);
        let mut incidence = t.gradients_per_m.map(|g| g.map(|v| -length*v));
        for axis in 0..3 {incidence[0][axis]=-incidence[1][axis]-incidence[2][axis]-incidence[3][axis];}
        if !density.is_finite() || !resistance.is_finite() || resistance <= 0. || !incidence.iter().flatten().all(|v| v.is_finite()) {
            return Err(error("VOLUME_RANGE", "current normalization overflow"));
        }
        let comp = root(&cell_parent, i);
        let potentials = t.nodes.map(|n| {
            if node_component[&n] == comp { node_potential[&n] } else {
                *extra_potential.entry((n, comp)).or_insert_with(|| { mesh_nodes.push(vec![n]); mesh_nodes.len() - 1 })
            }
        });
        components.entry(root(&cell_parent,i)).or_default().extend(potentials);
        cells.push(Cell { mesh_cell:i,potentials,incidence,density_per_amp_m2:density,resistance_ohm:resistance });
    }
    Ok(Space { mesh_nodes,node_potential,cells,components:components.into_values().map(|s|s.into_iter().collect()).collect(),planned_bytes:bytes })
}
/// Sparse conductivity factor with authored Dirichlet coordinates. One anchor
/// on each otherwise floating component is only a DC solve gauge; no resistor
/// or ground is added to the physical model.
pub struct DcFactor {
    fixed:BTreeSet<usize>, map:Vec<usize>, free:usize,
    symbolic:Option<cholesky::SymbolicCholesky<usize>>, values:Vec<f64>,
    pub planned_bytes:usize,
}
impl Space {
    pub fn dc_factor(&self,fixed_dofs:&[usize],budget:AllocationBudget)->Result<DcFactor> {
        let n=self.potential_count();let mut fixed:BTreeSet<_>=fixed_dofs.iter().copied().collect();
        if fixed.len()!=fixed_dofs.len() || fixed.iter().any(|&i|i>=n) {return Err(error("VOLUME_DC","distinct valid fixed coordinates required"));}
        for component in &self.components {if !component.iter().any(|i|fixed.contains(i)) {fixed.insert(component[0]);}}
        let mut free=0;let map:Vec<_>=(0..n).map(|i|if fixed.contains(&i){usize::MAX}else{let j=free;free+=1;j}).collect();
        let mut terms=BTreeMap::new();
        for cell in &self.cells {for a in 0..4 {for b in 0..4 {
            let i=map[cell.potentials[a]];let j=map[cell.potentials[b]];
            if i!=usize::MAX && j!=usize::MAX {let v=(0..3).map(|k|cell.incidence[a][k]*cell.incidence[b][k]/cell.resistance_ohm).sum::<f64>();*terms.entry((i,j)).or_insert(0.)+=v;}
        }}}
        let base=self.planned_bytes.checked_add(terms.len().checked_mul(128).ok_or_else(||error("MEMORY_LIMIT","DC terms overflow"))?).and_then(|v|v.checked_add(n.checked_mul(1024)?)).ok_or_else(||error("MEMORY_LIMIT","DC factor plan overflow"))?;
        budget.check(base)?;
        if free==0 {return Ok(DcFactor{fixed,map,free,symbolic:None,values:vec![],planned_bytes:base});}
        let trips:Vec<_>=terms.into_iter().map(|((i,j),v)|Triplet::new(i,j,v)).collect();
        let matrix=SparseColMat::<usize,f64>::try_new_from_triplets(free,free,&trips).map_err(|e|error("VOLUME_DC",&e.to_string()))?;
        let symbolic=cholesky::factorize_symbolic_cholesky(matrix.symbolic(),Side::Lower,Default::default(),Default::default()).map_err(|e|error("VOLUME_DC",&e.to_string()))?;
        let scratch=symbolic.factorize_numeric_llt_scratch::<f64>(Par::Seq,Default::default());
        let solve=symbolic.solve_in_place_scratch::<f64>(1,Par::Seq);
        let plan=base.checked_add(symbolic.len_val().checked_mul(8).ok_or_else(||error("MEMORY_LIMIT","DC fill overflow"))?).and_then(|v|v.checked_add(scratch.unaligned_bytes_required().max(solve.unaligned_bytes_required()))).ok_or_else(||error("MEMORY_LIMIT","DC scratch overflow"))?;
        budget.check(plan)?;
        let mut values=vec![0.;symbolic.len_val()];let mut workspace=MemBuffer::try_new(scratch).map_err(|_|error("MEMORY_LIMIT","DC scratch allocation"))?;
        symbolic.factorize_numeric_llt(&mut values,matrix.as_ref(),Side::Lower,Default::default(),Par::Seq,MemStack::new(&mut workspace),Default::default()).map_err(|_|error("VOLUME_DC","conductivity free block is not positive definite; no gmin"))?;
        Ok(DcFactor{fixed,map,free,symbolic:Some(symbolic),values,planned_bytes:plan})
    }
}
impl DcFactor {
    pub fn solve_scratch_bytes(&self,space:&Space)->usize {self.symbolic.as_ref().map_or(0,|s|s.solve_in_place_scratch::<f64>(1,Par::Seq).unaligned_bytes_required()).saturating_add(space.potential_count().saturating_mul(64)).saturating_add(space.current_count().saturating_mul(24)).saturating_add(65536)}
    /// Solve free K v = injection with prescribed authored electrode values.
    /// Residual includes every free coordinate, including fixed-gauge reaction.
    pub fn solve(&self,space:&Space,boundary:&[f64],injection:&[f64])->Result<Vec<f64>> {
        let n=space.potential_count();
        if boundary.len()!=n || injection.len()!=n || boundary.iter().chain(injection).any(|v|!v.is_finite()) || boundary.iter().enumerate().any(|(i,&v)|!self.fixed.contains(&i)&&v!=0.) {return Err(error("VOLUME_DC","finite full boundary/injection with zero free boundary values required"));}
        let mut voltage=boundary.to_vec();let action=space.dc_apply(boundary)?;
        if self.free>0 {
            let mut rhs=Mat::from_fn(self.free,1,|_,_|0.);
            for i in 0..n {if self.map[i]!=usize::MAX {rhs[(self.map[i],0)]=injection[i]-action[i];}}
            let symbolic=self.symbolic.as_ref().unwrap();let mut workspace=MemBuffer::try_new(symbolic.solve_in_place_scratch::<f64>(1,Par::Seq)).map_err(|_|error("MEMORY_LIMIT","DC solve workspace"))?;
            cholesky::LltRef::new(symbolic,&self.values).solve_in_place_with_conj(Conj::No,rhs.as_mut(),Par::Seq,MemStack::new(&mut workspace));
            for i in 0..n {if self.map[i]!=usize::MAX {voltage[i]=rhs[(self.map[i],0)];}}
        }
        let actual=space.dc_apply(&voltage)?;let mut denominator=vec![0.;n];
        for cell in &space.cells {for a in 0..4 {for b in 0..4 {denominator[cell.potentials[a]]+=(0..3).map(|k|cell.incidence[a][k]*cell.incidence[b][k]/cell.resistance_ohm*voltage[cell.potentials[b]]).sum::<f64>().abs();}}}
        let residual=(0..n).filter(|i|!self.fixed.contains(i)).map(|i|(actual[i]-injection[i]).abs()/(denominator[i]+injection[i].abs()).max(f64::MIN_POSITIVE)).fold(0.,f64::max);
        if residual>1e-8 || voltage.iter().any(|v|!v.is_finite()) {return Err(error("VOLUME_DC",&format!("free normalized residual={residual:e}")));}
        Ok(voltage)
    }
}
impl Space {
    pub fn potential_count(&self)->usize { self.mesh_nodes.len() }
    pub fn current_count(&self)->usize { 3*self.cells.len() }
    pub fn gradient(&self, volts:&[f64])->Result<Vec<f64>> {
        if volts.len()!=self.potential_count() || volts.iter().any(|v|!v.is_finite()) {return Err(error("VOLUME_VECTOR","finite full potential coordinates required"));}
        let mut out=vec![0.;self.current_count()];
        for (i,c) in self.cells.iter().enumerate() {
            // Evaluate P1 derivatives through nodal differences: the exact
            // constant mode is preserved without a floating cancellation test.
            for axis in 0..3 {
                out[3*i+axis]=(1..4).fold(Sum::default(),|sum,k|sum.add(Sum::from(volts[c.potentials[k]]).sub(Sum::from(volts[c.potentials[0]])).mul(c.incidence[k][axis]))).value();
            }
        }
        Ok(out)
    }
    pub fn divergence(&self, currents:&[f64])->Result<Vec<f64>> {
        if currents.len()!=self.current_count() || currents.iter().any(|v|!v.is_finite()) {return Err(error("VOLUME_VECTOR","finite full current coordinates required"));}
        let mut out=vec![Sum::default();self.potential_count()];
        for (i,c) in self.cells.iter().enumerate() {
            let mut reactions=[Sum::default();4];
            for k in 1..4 {reactions[k]=(0..3).fold(Sum::default(),|sum,axis|sum.add(Sum::from(currents[3*i+axis]).mul(c.incidence[k][axis])));}
            reactions[0]=reactions[1].add(reactions[2]).add(reactions[3]).neg();
            for k in 0..4 {
                let j=c.potentials[k];out[j]=out[j].add(reactions[k]);
            }
        }
        Ok(out.into_iter().map(Sum::value).collect())
    }
    pub fn dc_apply(&self, volts:&[f64])->Result<Vec<f64>> {
        let mut currents=self.gradient(volts)?;
        for (i,c) in self.cells.iter().enumerate() { for axis in 0..3 {currents[3*i+axis]/=c.resistance_ohm;} }
        self.divergence(&currents)
    }
    pub fn density(&self,currents:&[f64])->Result<Vec<[f64;3]>> {
        self.divergence(currents)?;
        Ok(self.cells.iter().enumerate().map(|(i,c)|std::array::from_fn(|axis|currents[3*i+axis]*c.density_per_amp_m2)).collect())
    }
    pub fn losses(&self,currents:&[f64])->Result<Vec<f64>> {
        self.divergence(currents)?;
        let losses:Vec<_>=self.cells.iter().enumerate().map(|(i,c)| c.resistance_ohm*(0..3).map(|a|currents[3*i+a].powi(2)).sum::<f64>()).collect();
        if losses.iter().any(|v|!v.is_finite()) {return Err(error("VOLUME_RANGE","Joule loss overflow"));} Ok(losses)
    }
}
