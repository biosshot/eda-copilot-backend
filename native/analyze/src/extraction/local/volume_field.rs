//! Surface-only dielectric trace lifted to all 3D conductor P1 coordinates.
//! Interior metal nodes have exactly zero electrostatic charge, not guessed C.
use super::{capacitance::{self,CapacitanceOperator},error,mesh::LayeredMesh,volume_current::Space,AllocationBudget,Result};
use std::collections::{BTreeMap,BTreeSet};
use faer::Mat;
pub trait ElectricApply: Sync {
    fn apply(&self,volts:&[f64])->Result<Vec<f64>>;
    fn energy_gram(&self,basis:&[Vec<f64>])->Result<Vec<(usize,usize,f64)>>;
    fn planned_bytes(&self)->usize;
    fn scratch_bytes(&self)->usize {self.planned_bytes()}
}
impl ElectricApply for Field<'_> {
    fn apply(&self,volts:&[f64])->Result<Vec<f64>> {Field::apply(self,volts)}
    fn energy_gram(&self,basis:&[Vec<f64>])->Result<Vec<(usize,usize,f64)>> {Field::energy_gram(self,basis)}
    fn planned_bytes(&self)->usize {self.operator.planned_peak_bytes}
}
/// Exact full trace-coordinate Schur materialization. No reduced voltage basis
/// or selected conductor is used; the FEM factor can then be released.
pub struct DenseField<'a> {
    mesh:&'a LayeredMesh,potential_dofs:Vec<usize>,potential_count:usize,capacitance:Mat<f64>,retained_bytes:usize,
    pub assembly_peak_bytes:usize,pub max_charge_energy_column_error:f64,pub max_directed_reciprocity:f64,
}
/// Internal intermediate coefficients, never an authored port circuit. Physics
/// convergence/evidence admission belongs to the board factory, not this loader.
#[derive(serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct TraceCheckpoint {
    kind:String,node_bits:Vec<[u64;3]>,copper_cells:Vec<[usize;4]>,
    potential_mesh_nodes:Vec<Vec<usize>>,physical_inputs:serde_json::Value,
    capacitance_f:Vec<Vec<f64>>,full_trace_complete:bool,
    max_charge_energy_column_error:f64,max_directed_reciprocity:f64,
    evidence:serde_json::Value,
}
impl<'a> DenseField<'a> {
    /// Exact geometry/contact/material binding of a complete extracted Schur
    /// checkpoint. Retains every potential, including certified buried zeros.
    pub fn load_checkpoint(mesh:&'a LayeredMesh,space:&Space,physical_inputs:&serde_json::Value,path:impl AsRef<std::path::Path>,budget:AllocationBudget)->Result<Self>{
        let n=space.potential_count();let bytes=std::fs::metadata(path.as_ref()).map_err(|e|error("FIELD_CACHE",&e.to_string()))?.len();
        let bytes=usize::try_from(bytes).map_err(|_|error("MEMORY_LIMIT","field checkpoint length overflow"))?;
        let bound=mesh.nodes_m.len().checked_mul(128).and_then(|v|v.checked_add(mesh.copper_tetrahedra.len().checked_mul(256)?)).and_then(|v|v.checked_add(n.checked_mul(n)?.checked_mul(64)?)).and_then(|v|v.checked_add(65536)).ok_or_else(||error("MEMORY_LIMIT","field checkpoint bound overflow"))?;
        if bytes>bound{return Err(error("FIELD_CACHE","serialized coefficient checkpoint exceeds source-size bound"));}
        let plan=bytes.checked_mul(32).and_then(|v|v.checked_add(n.checked_mul(n)?.checked_mul(32)?)).and_then(|v|v.checked_add(mesh.retained_bytes().ok()?)).and_then(|v|v.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","field checkpoint parse plan overflow"))?;budget.check(plan)?;
        let input=std::io::BufReader::new(std::fs::File::open(path).map_err(|e|error("FIELD_CACHE",&e.to_string()))?);
        let c:TraceCheckpoint=serde_json::from_reader(input).map_err(|e|error("FIELD_CACHE",&e.to_string()))?;
        if c.kind!="source-p1-full-field-trace-v0"||!c.full_trace_complete||c.node_bits!=mesh.nodes_m.iter().map(|p|p.map(f64::to_bits)).collect::<Vec<_>>()||c.copper_cells!=mesh.copper_tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>()||c.potential_mesh_nodes!=space.mesh_nodes||&c.physical_inputs!=physical_inputs{return Err(error("FIELD_CACHE","exact geometry/contact/material/complete-trace identity mismatch"));}
        if !c.evidence.is_object()||c.capacitance_f.len()!=n||c.capacitance_f.iter().any(|r|r.len()!=n||r.iter().any(|v|!v.is_finite()))||!c.max_charge_energy_column_error.is_finite()||c.max_charge_energy_column_error<0.||c.max_charge_energy_column_error>1e-8||!c.max_directed_reciprocity.is_finite()||c.max_directed_reciprocity<0.||c.max_directed_reciprocity>1e-9{return Err(error("FIELD_CACHE","finite complete Schur coefficients and unchanged numerical audits required"));}
        let exposed=mesh.copper_interfaces.iter().flat_map(|f|f.nodes).filter_map(|node|space.node_potential.get(&node).copied()).collect::<BTreeSet<_>>();
        if exposed.is_empty(){return Err(error("FIELD_CACHE","actual source has no dielectric/metal trace"));}
        for i in 0..n{if exposed.contains(&i){if c.capacitance_f[i][i]<=0.{return Err(error("FIELD_CACHE","positive exposed trace diagonal required"));}}else if(0..n).any(|j|c.capacitance_f[i][j]!=0.||c.capacitance_f[j][i]!=0.){return Err(error("FIELD_CACHE","buried source potentials must retain exact zero charge"));}}
        let dofs=exposed.into_iter().collect::<Vec<_>>();let mut reciprocity=0_f64;
        for &i in &dofs{for &j in &dofs{reciprocity=reciprocity.max((c.capacitance_f[i][j]-c.capacitance_f[j][i]).abs()/c.capacitance_f[i][i].sqrt()/c.capacitance_f[j][j].sqrt());}}
        if !reciprocity.is_finite()||reciprocity>1e-9{return Err(error("FIELD_CACHE","raw directed full trace reciprocity refuses1e-9"));}
        // Same canonical upper-triangle storage as native energy materialization,
        // only after the raw directed gate; no averaging or diagonal shift.
        let capacitance=Mat::from_fn(dofs.len(),dofs.len(),|i,j|{let(a,b)=(dofs[i].min(dofs[j]),dofs[i].max(dofs[j]));c.capacitance_f[a][b]});
        let scaled=Mat::from_fn(dofs.len(),dofs.len(),|i,j|capacitance[(i,j)]/capacitance[(i,i)].sqrt()/capacitance[(j,j)].sqrt());
        scaled.llt(faer::Side::Lower).map_err(|_|error("FIELD_CACHE","full exposed charge metric is not positive definite; no regularization"))?;
        let retained_bytes=mesh.retained_bytes()?.checked_add(dofs.len().checked_mul(dofs.len()).and_then(|v|v.checked_mul(16)).ok_or_else(||error("MEMORY_LIMIT","field checkpoint retained bytes overflow"))?).and_then(|v|v.checked_add(dofs.len().checked_mul(16)?)).ok_or_else(||error("MEMORY_LIMIT","field checkpoint retained bytes overflow"))?;
        Ok(Self{mesh,potential_count:n,potential_dofs:dofs,capacitance,retained_bytes,assembly_peak_bytes:plan,max_charge_energy_column_error:c.max_charge_energy_column_error,max_directed_reciprocity:reciprocity.max(c.max_directed_reciprocity)})
    }
    pub fn new(field:&Field<'a>,budget:AllocationBudget)->Result<Self> {Self::new_parallel(field,budget,1)}
    pub fn new_parallel(field:&Field<'a>,budget:AllocationBudget,workers:usize)->Result<Self> {
        use std::sync::{Mutex,atomic::{AtomicBool,AtomicUsize,Ordering}};
        if !(1..=4).contains(&workers){return Err(error("VOLUME_FIELD_WORKER","source column workers1..4 required"));}
        let n=field.potential_dofs.len();let nodes=field.mesh.nodes_m.len();
        let states=nodes.checked_mul(n).and_then(|v|v.checked_mul(16)).ok_or_else(||error("MEMORY_LIMIT","full field states overflow"))?;
        let gram_extra=n.checked_mul(n).and_then(|v|v.checked_mul(64)).and_then(|v|v.checked_add(256usize.checked_mul(3)?.checked_mul(n)?.checked_mul(16)?)).ok_or_else(||error("MEMORY_LIMIT","full C Gram storage overflow"))?;
        let solve_extra=field.operator.action_workspace_bytes()?.checked_mul(workers).and_then(|v|v.checked_add(n.checked_mul(n)?.checked_mul(8)?)).ok_or_else(||error("MEMORY_LIMIT","parallel field workspace overflow"))?;
        let live=field.operator.retained_bytes()?.checked_add(states).and_then(|v|v.checked_add(gram_extra.max(solve_extra))).ok_or_else(||error("MEMORY_LIMIT","full C materialization live overflow"))?;let peak=field.operator.planned_peak_bytes.max(live);budget.check(peak)?;
        let mut potentials=vec![0.;nodes*n];let mut potential_low=vec![0.;nodes*n];let mut charge_columns=vec![0.;n*n];
        let columns=Mutex::new(potentials.chunks_mut(nodes).zip(potential_low.chunks_mut(nodes)).zip(charge_columns.chunks_mut(n)).enumerate());let stopped=AtomicBool::new(false);let completed=AtomicUsize::new(0);
        let run=||->Result<()>{loop{if stopped.load(Ordering::Relaxed){return Ok(());}let item=columns.lock().map_err(|_|error("VOLUME_FIELD_WORKER","column mutex poisoned"))?.next();let Some((column,((hi,lo),charges)))=item else{return Ok(());};let mut v=vec![0.;n];v[column]=1.;let action=match field.operator.apply(&v){Ok(a)=>a,Err(e)=>{stopped.store(true,Ordering::Relaxed);return Err(e);}};hi.copy_from_slice(&action.nodal_potentials_v);lo.copy_from_slice(&action.nodal_potential_corrections_v);charges.copy_from_slice(&action.charges_c);completed.fetch_add(1,Ordering::Relaxed);}};
        if workers==1{run()?;}else{let errors=std::thread::scope(|scope|{let mut handles=vec![];let mut errors=vec![];for worker in 0..workers{match std::thread::Builder::new().name(format!("p7-field-{worker}")).stack_size(524288).spawn_scoped(scope,&run){Ok(h)=>handles.push(h),Err(e)=>{stopped.store(true,Ordering::Relaxed);errors.push(e.to_string());break;}}}for handle in handles{match handle.join(){Ok(Ok(()))=>(),Ok(Err(e))=>errors.push(e),Err(_)=>errors.push("field worker panicked".into())}}errors});if !errors.is_empty(){return Err(error("VOLUME_FIELD_WORKER",&errors.join("; ")));}}
        drop(columns);if completed.load(Ordering::Relaxed)!=n{return Err(error("VOLUME_FIELD_WORKER","incomplete source-column space"));}
        let charges=Mat::from_fn(n,n,|i,j|charge_columns[j*n+i]);drop(charge_columns);
        let mut gram=Mat::<f64>::zeros(n,n);
        for cells in field.mesh.tetrahedra.chunks(256) {let weighted=Mat::from_fn(3*cells.len(),n,|row,column|{let t=&cells[row/3];let k=row%3;let base=super::accurate::Sum{hi:potentials[column*nodes+t.nodes[0]],lo:potential_low[column*nodes+t.nodes[0]]};let gradient=(1..4).fold(super::accurate::Sum::default(),|sum,i|sum.add(super::accurate::Sum{hi:potentials[column*nodes+t.nodes[i]],lo:potential_low[column*nodes+t.nodes[i]]}.sub(base).mul(t.gradients_per_m[i][k]))).value();(super::super::EPS0*t.epsilon_r*t.volume_m3).sqrt()*gradient});let block=weighted.transpose()*&weighted;for i in 0..n{for j in 0..n{gram[(i,j)]+=block[(i,j)];}}}
        let mut discrepancy:f64=0.;let mut reciprocity:f64=0.;
        for j in 0..n {if (0..n).any(|i|!gram[(i,j)].is_finite()||!charges[(i,j)].is_finite()){return Err(error("VOLUME_FIELD","nonfinite full capacitance column"));}let scale=(0..n).map(|i|gram[(i,j)].powi(2)).sum::<f64>().sqrt();let error=(0..n).map(|i|(gram[(i,j)]-charges[(i,j)]).powi(2)).sum::<f64>().sqrt()/scale.max(f64::MIN_POSITIVE);discrepancy=discrepancy.max(error);for i in 0..n {let d=(charges[(i,j)]-charges[(j,i)]).abs()/(gram[(i,i)]*gram[(j,j)]).sqrt().max(f64::MIN_POSITIVE);reciprocity=reciprocity.max(d);}}
        if !discrepancy.is_finite()||discrepancy>1e-8||!reciprocity.is_finite()||reciprocity>1e-9{return Err(error("VOLUME_FIELD",&format!("full C charge/energy column={discrepancy:e}, directed reciprocity={reciprocity:e}")));}
        for i in 0..n{for j in 0..i{gram[(i,j)]=gram[(j,i)];}}
        let retained_bytes=field.mesh.retained_bytes()?.checked_add(n.checked_mul(n).and_then(|v|v.checked_mul(16)).ok_or_else(||error("MEMORY_LIMIT","full C retained matrix overflow"))?).and_then(|v|v.checked_add(n.checked_mul(16)?)).ok_or_else(||error("MEMORY_LIMIT","full C retained overflow"))?;
        Ok(Self{mesh:field.mesh,potential_dofs:field.potential_dofs.clone(),potential_count:field.potential_count,capacitance:gram,retained_bytes,assembly_peak_bytes:peak,max_charge_energy_column_error:discrepancy,max_directed_reciprocity:reciprocity})
    }
    pub fn mesh(&self)->&LayeredMesh {self.mesh}
}
impl ElectricApply for DenseField<'_> {
    fn scratch_bytes(&self)->usize {self.potential_count.saturating_mul(8)}
    fn planned_bytes(&self)->usize {self.retained_bytes}
    fn apply(&self,volts:&[f64])->Result<Vec<f64>> {if volts.len()!=self.potential_count||volts.iter().any(|v|!v.is_finite()){return Err(error("VOLUME_FIELD","finite full potential vector required"));}let mut charge=vec![0.;self.potential_count];for(i,&dof)in self.potential_dofs.iter().enumerate(){let mut sum=super::accurate::Sum::default();for(j,&source)in self.potential_dofs.iter().enumerate(){sum=sum.add(super::accurate::Sum::from(volts[source]).mul(self.capacitance[(i,j)]));}charge[dof]=sum.value();}Ok(charge)}
    fn energy_gram(&self,basis:&[Vec<f64>])->Result<Vec<(usize,usize,f64)>> {if basis.iter().any(|v|v.len()!=self.potential_count||v.iter().any(|v|!v.is_finite())){return Err(error("VOLUME_FIELD","finite full field basis required"));}let mut out=Vec::new();for a in 0..basis.len(){for b in a..basis.len(){let mut sum=super::accurate::Sum::default();for(i,&di)in self.potential_dofs.iter().enumerate(){for(j,&dj)in self.potential_dofs.iter().enumerate(){sum=sum.add(super::accurate::Sum::from(self.capacitance[(i,j)]).mul(basis[a][di]).mul(basis[b][dj]));}}let v=sum.value();if !v.is_finite(){return Err(error("VOLUME_FIELD","full C congruence overflow"));}if v!=0.{out.push((a,b,v));if a!=b{out.push((b,a,v));}}}}Ok(out)}
}
pub struct Field<'a> {
    mesh:&'a LayeredMesh,
    pub operator:CapacitanceOperator<'a>,
    pub potential_dofs:Vec<usize>,
    pub potential_count:usize,
}
impl<'a> Field<'a> {
    /// Independent XY/z dielectric refinement on the unchanged physical Cu
    /// surface. This is positive nodal sampling of the source surface P1 trace,
    /// followed by B^T C B; nonnested surface diagonals are not claimed exact.
    pub fn new_surface_refined(mesh:&'a LayeredMesh,current:&LayeredMesh,space:&Space,budget:AllocationBudget,wall:std::time::Duration)->Result<Self>{
        let started=std::time::Instant::now();
        if (mesh.copper_volume_m3-current.copper_volume_m3).abs()>1e-12*current.copper_volume_m3{return Err(error("VOLUME_FIELD_TRACE","refinement changed original copper volume"));}
        let plan=mesh.retained_bytes()?.checked_add(mesh.nodes_m.len().checked_mul(512).ok_or_else(||error("MEMORY_LIMIT","surface trace node plan overflow"))?).and_then(|v|v.checked_add(current.copper_interfaces.len().checked_mul(512)?)).ok_or_else(||error("MEMORY_LIMIT","surface trace plan overflow"))?;budget.check(plan)?;
        let exact=space.node_potential.iter().map(|(&n,&d)|(current.nodes_m[n].map(f64::to_bits),d)).collect::<BTreeMap<_,_>>();
        let exposed=current.copper_interfaces.iter().flat_map(|f|f.nodes).filter_map(|n|space.node_potential.get(&n).copied()).collect::<BTreeSet<_>>();
        let mut metal=BTreeMap::<usize,BTreeSet<&str>>::new();
        for f in &mesh.copper_interfaces{for n in f.nodes{metal.entry(n).or_default().extend(f.copper_source_ids.iter().map(String::as_str));}}
        let mut rows=Vec::with_capacity(metal.len());let mut represented=BTreeSet::new();
        for (index,(&node,ids)) in metal.iter().enumerate(){
            if index%256==0&&started.elapsed()>wall{return Err(error("TIME_LIMIT","source-surface trace deadline exceeded"));}
            let p=mesh.nodes_m[node];let mut weights=BTreeMap::new();
            if let Some(&d)=exact.get(&p.map(f64::to_bits)){weights.insert(d,1.);}else{
                for face in &current.copper_interfaces{
                    if !face.copper_source_ids.iter().any(|s|ids.contains(s.as_str())){continue;}
                    let q=face.nodes.map(|n|current.nodes_m[n]);
                    let scale=p.iter().chain(q.iter().flatten()).map(|x|x.abs()).fold(f64::MIN_POSITIVE,f64::max);
                    let tol=64.*f64::EPSILON*scale;
                    if (0..3).any(|k|p[k]<q.iter().map(|v|v[k]).fold(f64::INFINITY,f64::min)-tol||p[k]>q.iter().map(|v|v[k]).fold(f64::NEG_INFINITY,f64::max)+tol){continue;}
                    let a=std::array::from_fn::<_,3,_>(|k|q[1][k]-q[0][k]);let b=std::array::from_fn::<_,3,_>(|k|q[2][k]-q[0][k]);
                    let normal=[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
                    let drop=(0..3).max_by(|&i,&j|normal[i].abs().total_cmp(&normal[j].abs())).unwrap();let u=(drop+1)%3;let v=(drop+2)%3;
                    let det=a[u]*b[v]-a[v]*b[u];if det==0.{continue;}
                    let x=p[u]-q[0][u];let y=p[v]-q[0][v];let w1=(x*b[v]-y*b[u])/det;let w2=(a[u]*y-a[v]*x)/det;let mut w=[1.-w1-w2,w1,w2];
                    if w.iter().any(|&x|x< -64.*f64::EPSILON||x>1.+64.*f64::EPSILON){continue;}
                    for x in &mut w{if *x<0.{*x=0.;}}let sum=w.iter().sum::<f64>();for x in &mut w{*x/=sum;}
                    if (0..3).any(|k|((0..3).map(|j|w[j]*q[j][k]).sum::<f64>()-p[k]).abs()>tol){continue;}
                    for j in 0..3{if w[j]>0.{*weights.entry(*space.node_potential.get(&face.nodes[j]).ok_or_else(||error("VOLUME_FIELD_TRACE","surface vertex has no source potential"))?).or_insert(0.)+=w[j];}}break;
                }
                if weights.is_empty(){return Err(error("VOLUME_FIELD_TRACE",&format!("fine Cu surface node {node} is outside unchanged original surface; no proximity repair")));}
            }
            represented.extend(weights.keys().copied());rows.push(capacitance::TraceRow{node,weights:weights.into_iter().collect()});
        }
        if !exposed.is_subset(&represented){return Err(error("VOLUME_FIELD_TRACE","refinement lost an exposed source coordinate"));}
        Self::new_conforming_trace(mesh,space.potential_count(),rows,budget)
    }
    /// Caller supplies a geometry-certified conforming trace. Keep every exposed
    /// source coordinate and exact zeros on genuinely buried source potentials.
    pub fn new_conforming_trace(mesh:&'a LayeredMesh,potential_count:usize,mut trace:Vec<capacitance::TraceRow>,budget:AllocationBudget)->Result<Self>{
        let expected=mesh.copper_interfaces.iter().flat_map(|f|f.nodes).collect::<BTreeSet<_>>();let actual=trace.iter().map(|r|r.node).collect::<BTreeSet<_>>();
        if expected!=actual||actual.len()!=trace.len(){return Err(error("VOLUME_FIELD_TRACE","complete unique actual metal boundary rows required"));}
        let mut represented=BTreeSet::new();for row in &trace{let mut columns=BTreeSet::new();let mut sum=0.;for &(i,w)in &row.weights{if i>=potential_count||!w.is_finite()||w<=0.||!columns.insert(i){return Err(error("VOLUME_FIELD_TRACE","positive unique original source support required"));}sum+=w;represented.insert(i);}if(sum-1.).abs()>1e-14{return Err(error("VOLUME_FIELD_TRACE","original source trace lost constant partition"));}}
        let potential_dofs=represented.into_iter().collect::<Vec<_>>();for row in &mut trace{for(i,_)in &mut row.weights{*i=potential_dofs.binary_search(i).unwrap();}}
        let far_zero_nodes=mesh.external_faces.iter().flatten().filter(|i|!actual.contains(i)).copied().collect::<BTreeSet<_>>().into_iter().collect();
        let operator=CapacitanceOperator::new(mesh,capacitance::Boundary{potential_count:potential_dofs.len(),metal_trace:trace,far_zero_nodes,insulated_external:false},budget)?;
        Ok(Self{mesh,operator,potential_dofs,potential_count})
    }
    /// Independent z/domain refinement with the identical prepared XY copper.
    /// Every nontrivial trace row belongs to an actual coarse vertical Cu edge.
    pub fn new_vertical_refined(mesh:&'a LayeredMesh,current_mesh:&LayeredMesh,space:&Space,budget:AllocationBudget)->Result<Self> {
        let plan=mesh.retained_bytes()?.checked_add(mesh.nodes_m.len().checked_mul(256).ok_or_else(||error("MEMORY_LIMIT","field trace node plan overflow"))?).and_then(|n|n.checked_add(current_mesh.copper_tetrahedra.len().checked_mul(1024)?)).ok_or_else(||error("MEMORY_LIMIT","field trace plan overflow"))?;budget.check(plan)?;
        let mut columns=BTreeMap::<[u64;2],Vec<(f64,usize)>>::new();
        for (&node,_) in &space.node_potential {let p=current_mesh.nodes_m[node];columns.entry([p[0].to_bits(),p[1].to_bits()]).or_default().push((p[2],node));}
        for column in columns.values_mut(){column.sort_by(|a,b|a.0.total_cmp(&b.0));}
        let mut edges=BTreeSet::new();
        for t in &current_mesh.copper_tetrahedra {for i in 0..4 {for j in i+1..4 {let a=t.nodes[i];let b=t.nodes[j];if current_mesh.nodes_m[a][..2]==current_mesh.nodes_m[b][..2] {edges.insert((a.min(b),a.max(b)));}}}}
        let active=mesh.tetrahedra.iter().flat_map(|t|t.nodes).collect::<BTreeSet<_>>();
        let metal=mesh.copper_interfaces.iter().flat_map(|f|f.nodes).filter(|n|active.contains(n)).collect::<BTreeSet<_>>();
        let mut raw=vec![];let mut represented=BTreeSet::new();
        for &node in &metal {let p=mesh.nodes_m[node];let column=columns.get(&[p[0].to_bits(),p[1].to_bits()]).ok_or_else(||error("VOLUME_FIELD_TRACE","refined metal XY vertex has no exact coarse vertical trace; XY refinement is unsupported"))?;
            let mut weights=BTreeMap::new();
            if let Some(&(_,original))=column.iter().find(|&&(z,_)|z==p[2]) {weights.insert(space.node_potential[&original],1.);}else{
                let upper=column.partition_point(|&(z,_)|z<p[2]);
                if upper==0||upper==column.len(){return Err(error("VOLUME_FIELD_TRACE","fine metal z lies outside coarse Cu support"));}
                let (z0,a)=column[upper-1];let(z1,b)=column[upper];
                if !edges.contains(&(a.min(b),a.max(b))){return Err(error("VOLUME_FIELD_TRACE","fine metal trace would cross a dielectric gap"));}
                let t=(p[2]-z0)/(z1-z0);
                if !(t>0.&&t<1.){return Err(error("VOLUME_FIELD_TRACE","positive vertical interpolation could not be certified"));}
                *weights.entry(space.node_potential[&a]).or_insert(0.)+=1.-t;*weights.entry(space.node_potential[&b]).or_insert(0.)+=t;
            }
            for &i in weights.keys(){represented.insert(i);}raw.push((node,weights));
        }
        let potential_dofs=represented.into_iter().collect::<Vec<_>>();
        if potential_dofs.is_empty(){return Err(error("VOLUME_FIELD_TRACE","no represented metal trace"));}
        let metal_trace=raw.into_iter().map(|(node,weights)|capacitance::TraceRow{node,weights:weights.into_iter().map(|(i,w)|(potential_dofs.binary_search(&i).unwrap(),w)).collect()}).collect();
        let far_zero_nodes=mesh.external_faces.iter().flatten().filter(|n|active.contains(n)&&!metal.contains(n)).copied().collect::<BTreeSet<_>>().into_iter().collect();
        let operator=CapacitanceOperator::new(mesh,capacitance::Boundary{potential_count:potential_dofs.len(),metal_trace,far_zero_nodes,insulated_external:false},budget)?;
        Ok(Self{mesh,operator,potential_dofs,potential_count:space.potential_count()})
    }
    pub fn new(mesh:&'a LayeredMesh,space:&Space,budget:AllocationBudget)->Result<Self> {
        let active:BTreeSet<_>=mesh.tetrahedra.iter().flat_map(|t|t.nodes).collect();
        let potential_dofs:Vec<_>=active.iter().filter_map(|i|space.node_potential.get(i)).copied().collect::<BTreeSet<_>>().into_iter().collect();
        if potential_dofs.is_empty() {return Err(error("VOLUME_FIELD","no exposed metal surface"));}
        let metal_trace=active.iter().filter_map(|node|space.node_potential.get(node).map(|dof|capacitance::TraceRow{node:*node,weights:vec![(potential_dofs.binary_search(dof).unwrap(),1.)]})).collect();
        let far_zero_nodes=mesh.external_faces.iter().flatten().filter(|i|active.contains(i)&&!space.node_potential.contains_key(i)).copied().collect::<BTreeSet<_>>().into_iter().collect();
        let operator=CapacitanceOperator::new(mesh,capacitance::Boundary{potential_count:potential_dofs.len(),metal_trace,far_zero_nodes,insulated_external:false},budget)?;
        Ok(Self{mesh,operator,potential_dofs,potential_count:space.potential_count()})
    }
    pub fn apply(&self,volts:&[f64])->Result<Vec<f64>> {
        if volts.len()!=self.potential_count || volts.iter().any(|v|!v.is_finite()) {return Err(error("VOLUME_FIELD","finite full metal potential vector required"));}
        let action=self.operator.apply(&self.potential_dofs.iter().map(|&i|volts[i]).collect::<Vec<_>>())?;
        let mut charge=vec![0.;self.potential_count];
        for (&i,q) in self.potential_dofs.iter().zip(action.charges_c) {charge[i]=q;}
        Ok(charge)
    }
    pub fn energy_gram(&self,basis:&[Vec<f64>])->Result<Vec<(usize,usize,f64)>> {
        let actions=basis.iter().map(|q| {
            if q.len()!=self.potential_count {return Err(error("VOLUME_FIELD","basis dimension mismatch"));}
            self.operator.apply(&self.potential_dofs.iter().map(|&i|q[i]).collect::<Vec<_>>())
        }).collect::<Result<Vec<_>>>()?;
        let mut out=vec![];
        for a in 0..basis.len() {for b in a..basis.len() {
            let v=super::super::EPS0*self.mesh.tetrahedra.iter().enumerate().map(|(i,t)|t.epsilon_r*t.volume_m3*(0..3).map(|k|actions[a].electric_fields_v_per_m[i][k]*actions[b].electric_fields_v_per_m[i][k]).sum::<f64>()).sum::<f64>();
            if !v.is_finite() {return Err(error("VOLUME_FIELD","energy Gram overflow"));}
            if v!=0. {out.push((a,b,v));if a!=b {out.push((b,a,v));}}
        }}Ok(out)
    }
}
