//! Hierarchical finite-volume Newton candidate. Near cubature and separated
//! kernel interpolation have distinct mandatory admission/refinement gates.
use super::{accurate::Sum,error,mesh::LayeredMesh,polyhedron::{self,PointPotential},prism::GaussRule,volume_current::Space,AllocationBudget,Result};
use std::time::{Duration,Instant};
use std::sync::{Mutex,atomic::{AtomicU64,AtomicUsize,AtomicBool,Ordering}};
struct Maximum(AtomicU64);
impl Maximum{fn new(v:f64)->Self{Self(AtomicU64::new(v.to_bits()))}fn get(&self)->f64{f64::from_bits(self.0.load(Ordering::Relaxed))}fn include(&self,v:f64){if v>0.{self.0.fetch_max(v.to_bits(),Ordering::Relaxed);}}}
struct Counter(AtomicUsize);
impl Counter{fn new()->Self{Self(AtomicUsize::new(0))}fn get(&self)->usize{self.0.load(Ordering::Relaxed)}fn increment(&self){self.0.fetch_add(1,Ordering::Relaxed);}}

struct Cluster {
    start:usize,end:usize,center:[f64;3],half:[f64;3],
    children:Option<[usize;2]>,transfer:[Vec<f64>;3],weights:Vec<f64>,
}
struct Far {a:usize,b:usize,kernel:Vec<f64>}
struct Near {a:usize,b:usize,integral:f64}
pub struct Operator {
    nodes:Vec<Cluster>,order:Vec<usize>,near:Vec<Near>,far:Vec<Far>,p:usize,
    pub planned_bytes:usize,pub near_pairs:usize,pub far_blocks:usize,
    pub max_directed_reciprocity:f64,
    pub max_surface_condition:f64,pub surface_volume_fallbacks:usize,
    pub compensated_surface_evaluations:usize,pub max_compensated_surface_condition:f64,
    pub cached_pairs:usize,pub written_pairs:usize,
    pub constructor_workers:usize,
}
fn clock(start:Instant,limit:Duration)->Result<()> {if start.elapsed()>limit {Err(error("DEADLINE","finite-volume hierarchical Newton construction"))}else{Ok(())}}
fn length(v:[f64;3])->f64 {v[0].hypot(v[1]).hypot(v[2])}
fn abscissae(p:usize)->Vec<f64> {(0..p).map(|i|(std::f64::consts::PI*(i as f64+0.5)/p as f64).cos()).collect()}
fn lagrange(x:f64,nodes:&[f64])->Vec<f64> {
    if let Some(i)=nodes.iter().position(|&v|x==v) {return (0..nodes.len()).map(|j|if i==j{1.}else{0.}).collect();}
    let p=nodes.len();let mut w:Vec<_>=(0..p).map(|i| {
        let theta=std::f64::consts::PI*(i as f64+0.5)/p as f64;
        (if i%2==0{1.}else{-1.})*theta.sin()/(x-nodes[i])
    }).collect();let total=w.iter().sum::<f64>();for v in &mut w {*v/=total;}w
}
fn point(c:&Cluster,index:usize,x:&[f64])->[f64;3] {
    let p=x.len();let indices=[index/(p*p),(index/p)%p,index%p];
    std::array::from_fn(|k|c.center[k]+c.half[k]*x[indices[k]])
}
fn tree(mesh:&LayeredMesh,order:&mut[usize],offset:usize,nodes:&mut Vec<Cluster>,leaf:usize)->usize {
    let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];
    for &i in order.iter() {for n in mesh.copper_tetrahedra[i].nodes {for k in 0..3 {lo[k]=lo[k].min(mesh.nodes_m[n][k]);hi[k]=hi[k].max(mesh.nodes_m[n][k]);}}}
    let center=std::array::from_fn(|k|lo[k]+(hi[k]-lo[k])/2.);let half=std::array::from_fn(|k|(hi[k]-lo[k])/2.);
    let index=nodes.len();nodes.push(Cluster{start:offset,end:offset+order.len(),center,half,children:None,transfer:std::array::from_fn(|_|vec![]),weights:vec![]});
    if order.len()>leaf {
        let axis=(0..3).max_by(|&a,&b|half[a].total_cmp(&half[b])).unwrap();
        let coordinate=|i:usize|mesh.copper_tetrahedra[i].nodes.iter().map(|&n|mesh.nodes_m[n][axis]).sum::<f64>()/4.;
        order.sort_unstable_by(|&a,&b|coordinate(a).total_cmp(&coordinate(b)).then(a.cmp(&b)));
        let mid=order.len()/2;let(a,b)=order.split_at_mut(mid);
        nodes[index].children=Some([tree(mesh,a,offset,nodes,leaf),tree(mesh,b,offset+mid,nodes,leaf)]);
    }index
}
fn blocks(nodes:&[Cluster],a:usize,b:usize,far:&mut Vec<(usize,usize)>,near:&mut Vec<(usize,usize)>,max_blocks:usize)->Result<()> {
    if far.len()+near.len()>=max_blocks {return Err(error("MEMORY_LIMIT","cluster block storage plan exhausted before allocation"));}
    let ca=&nodes[a];let cb=&nodes[b];
    let gap=length(std::array::from_fn(|k|((ca.center[k]-cb.center[k]).abs()-ca.half[k]-cb.half[k]).max(0.)));
    let diameter=2.*length(ca.half).max(length(cb.half));
    if a!=b&&gap>0.&&diameter<=0.5*gap {far.push((a,b));return Ok(());}
    if a==b {
        if let Some([x,y])=ca.children {blocks(nodes,x,x,far,near,max_blocks)?;blocks(nodes,x,y,far,near,max_blocks)?;blocks(nodes,y,y,far,near,max_blocks)?;}else{near.push((a,b));}
    }else if ca.children.is_some()&&(cb.children.is_none()||length(ca.half)>=length(cb.half)) {
        for c in ca.children.unwrap(){blocks(nodes,c,b,far,near,max_blocks)?;}
    }else if let Some(children)=cb.children {for c in children {blocks(nodes,a,c,far,near,max_blocks)?;}}
    else {near.push((a,b));}
    Ok(())
}
fn transfer(input:&[f64],matrices:&[Vec<f64>;3],p:usize,transpose:bool)->Vec<f64> {
    let r=p*p*p;let mut previous=input.to_vec();
    for axis in 0..3 {
        let mut next=vec![0.;3*r];
        for x in 0..p {for y in 0..p {for z in 0..p {let position=[x,y,z];let row=position[axis];
            for component in 0..3 {
                next[3*((x*p+y)*p+z)+component]=(0..p).map(|column| {
                    let mut other=position;other[axis]=column;
                    matrices[axis][if transpose{column*p+row}else{row*p+column}]*previous[3*((other[0]*p+other[1])*p+other[2])+component]
                }).sum();
            }
        }}}
        previous=next;
    }previous
}
/// Local positive-volume target subdivision. This is an error estimator; the
/// independent directed/global gates remain mandatory at the call site.
pub fn adaptive_integral(target:[[f64;3];4],volume:f64,source:&PointPotential,tolerance:f64,start:Instant,limit:Duration)->Result<f64> {adaptive_integral_impl(target,volume,source,tolerance,start,limit,4096)}
/// More reference work under an explicit existing spatial/memory budget.
pub fn adaptive_integral_with_budget(target:[[f64;3];4],volume:f64,source:&PointPotential,tolerance:f64,start:Instant,limit:Duration,max_leaves:usize,budget:AllocationBudget)->Result<f64>{
 if max_leaves<1||max_leaves>budget.max_tetrahedra{return Err(error("MEMORY_LIMIT","adaptive reference leaf count exceeds explicit tetra budget"));}
 budget.check(max_leaves.checked_mul(256).ok_or_else(||error("MEMORY_LIMIT","adaptive reference allocation overflow"))?)?;
 adaptive_integral_impl(target,volume,source,tolerance,start,limit,max_leaves)
}
fn adaptive_integral_impl(target:[[f64;3];4],volume:f64,source:&PointPotential,tolerance:f64,start:Instant,limit:Duration,max_leaves:usize)->Result<f64> {
    struct Cell {vertices:[[f64;3];4],volume:f64,value:f64,error:f64}
    let low=GaussRule::new(8)?;let high=GaussRule::new(16)?;
    let cell=|vertices:[[f64;3];4],volume:f64|->Result<Cell> {
        clock(start,limit)?;
        let evaluate=|rule:&GaussRule|->Result<f64>{let mut sum=Sum::default();let mut samples=0usize;polyhedron::tetra_integrate(vertices,volume,rule,|p,w|{samples+=1;if samples%128==0 {clock(start,limit)?;}sum=sum.add(Sum::from(source.at(p)?).mul(w));Ok(())})?;Ok(sum.value())};
        let a=evaluate(&low)?;let b=evaluate(&high)?;
        if !(a>0.&&b>0.&&a.is_finite()&&b.is_finite()){return Err(error("INTEGRAL_RANGE","positive adaptive finite-volume integral required"));}
        Ok(Cell{vertices,volume,value:b,error:(b-a).abs()})
    };
    let mut cells=vec![cell(target,volume)?];
    loop {
        let estimated_error=cells.iter().map(|c|c.error).sum::<f64>();
        if estimated_error<=tolerance {return Ok(cells.iter().fold(Sum::default(),|s,c|s.add(Sum::from(c.value))).value());}
        if cells.len()>=max_leaves {return Err(error("INTEGRAL_QUADRATURE",&format!("adaptive target{max_leaves}-leaf error budget exhausted: estimated={estimated_error:e}, tolerance={tolerance:e}, maxLeaf={:e}",cells.iter().map(|c|c.error).fold(0.,f64::max))));}
        let worst=(0..cells.len()).max_by(|&a,&b|cells[a].error.total_cmp(&cells[b].error)).unwrap();let old=cells.swap_remove(worst);
        let (a,b)=(0..4).flat_map(|a|(a+1..4).map(move|b|(a,b))).max_by(|&(a,b),&(c,d)|length(std::array::from_fn(|k|old.vertices[a][k]-old.vertices[b][k])).total_cmp(&length(std::array::from_fn(|k|old.vertices[c][k]-old.vertices[d][k])))).unwrap();
        let middle=std::array::from_fn(|k|old.vertices[a][k]+(old.vertices[b][k]-old.vertices[a][k])/2.);
        let mut left=old.vertices;left[a]=middle;let mut right=old.vertices;right[b]=middle;
        let lv=super::mesh::tetra_coefficients(left)?.0;let rv=super::mesh::tetra_coefficients(right)?.0;
        if ((lv+rv)-old.volume).abs()>1e-10*old.volume {return Err(error("INTEGRAL_PARTITION","adaptive bisection lost physical target volume"));}
        cells.push(cell(left,old.volume/2.)?);cells.push(cell(right,old.volume/2.)?);
    }
}
impl Operator {
    pub fn new(mesh:&LayeredMesh,p:usize,near_order:usize,budget:AllocationBudget,limit:Duration)->Result<Self> {
        Self::with_surface(mesh,p,near_order,budget,limit,false,None,1)
    }
    pub fn new_surface(mesh:&LayeredMesh,p:usize,near_order:usize,budget:AllocationBudget,limit:Duration)->Result<Self> {
        Self::with_surface(mesh,p,near_order,budget,limit,true,None,1)
    }
    pub fn new_surface_checkpointed(mesh:&LayeredMesh,p:usize,near_order:usize,budget:AllocationBudget,limit:Duration,inputs:&[String],output:&str)->Result<Self>{
        Self::new_surface_checkpointed_workers(mesh,p,near_order,budget,limit,inputs,output,1)
    }
    pub fn new_surface_checkpointed_workers(mesh:&LayeredMesh,p:usize,near_order:usize,budget:AllocationBudget,limit:Duration,inputs:&[String],output:&str,workers:usize)->Result<Self>{
        if inputs.len()>64{return Err(error("INTEGRAL_CACHE","at most64 immutable input packets admitted"));}if !(1..=8).contains(&workers){return Err(error("INTEGRAL_INPUT","constructor CPU workers1..8 required"));}Self::with_surface(mesh,p,near_order,budget,limit,true,Some((inputs,output)),workers)
    }
    fn with_surface(mesh:&LayeredMesh,p:usize,near_order:usize,budget:AllocationBudget,limit:Duration,surface:bool,checkpoint:Option<(&[String],&str)>,workers:usize)->Result<Self> {
        let start=Instant::now();let n=mesh.copper_tetrahedra.len();
        if n==0||![3,4,5,6].contains(&p)||![4,8,16,32,64].contains(&near_order) {return Err(error("INTEGRAL_INPUT","finite tetrahedra, interpolation3..6 and near ceiling4/8/16/32/64 required"));}
        let r=p*p*p;
        // Parallel workers also own their bounded4096-leaf adaptive fallback.
        let workspace=if workers==1{262144}else{workers.checked_mul(1048576+524288).ok_or_else(||error("MEMORY_LIMIT","constructor worker workspace overflow"))?};
        let retained=mesh.retained_bytes()?.checked_add(workspace).ok_or_else(||error("MEMORY_LIMIT","source cache allowance overflow"))?;
        budget.check(retained.checked_add(n.checked_mul(65536).ok_or_else(||error("MEMORY_LIMIT","integral tree plan overflow"))?).ok_or_else(||error("MEMORY_LIMIT","integral tree plan overflow"))?)?;
        let mut order:Vec<_>=(0..n).collect();let mut nodes=Vec::with_capacity(2*n);tree(mesh,&mut order,0,&mut nodes,8);
        let tree_bytes=retained.checked_add(order.capacity()*std::mem::size_of::<usize>()).and_then(|v|v.checked_add(nodes.capacity()*std::mem::size_of::<Cluster>())).ok_or_else(||error("MEMORY_LIMIT","tree storage overflow"))?;
        // Both Vec capacities can grow to twice their lengths. Reserve that
        // bound before each block append, including the initial small buffers.
        let max_blocks=budget.max_bytes.checked_sub(tree_bytes).and_then(|v|v.checked_sub(128)).ok_or_else(||error("MEMORY_LIMIT","no block storage available"))?/32;
        let mut far_blocks=vec![];let mut near_blocks=vec![];blocks(&nodes,0,0,&mut far_blocks,&mut near_blocks,max_blocks)?;clock(start,limit)?;
        let near_pairs=near_blocks.iter().try_fold(0usize,|sum,&(a,b)| {
            let na=nodes[a].end-nodes[a].start;let nb=nodes[b].end-nodes[b].start;
            sum.checked_add(if a==b{na*(na+1)/2}else{na*nb}).ok_or_else(||error("MEMORY_LIMIT","near pair plan overflow"))
        })?;
        eprintln!("original finite-volume hierarchy: sources={n}, nearPairs={near_pairs}, farBlocks={}, CPUworkers={workers}",far_blocks.len());
        let plan=(||retained.checked_add(n.checked_mul(65536)?)?.checked_add(nodes.len().checked_mul(3*p*p*8+3*r*32+std::mem::size_of::<Cluster>())?)?.checked_add(n.checked_mul(r)?.checked_mul(8)?)?.checked_add(far_blocks.len().checked_mul(r*r*8+std::mem::size_of::<Far>())?)?.checked_add(near_pairs.checked_mul(std::mem::size_of::<Near>())?))().ok_or_else(||error("MEMORY_LIMIT","hierarchical Newton plan overflow"))?;
        budget.check(plan)?;
        let x=abscissae(p);let rule=GaussRule::new((3*p).div_ceil(2))?;
        for index in 0..nodes.len() {
            clock(start,limit)?;
            if let Some(children)=nodes[index].children {
                for child in children {for axis in 0..3 {
                    let mut matrix=vec![0.;p*p];
                    for column in 0..p {
                        let value=(nodes[child].center[axis]+nodes[child].half[axis]*x[column]-nodes[index].center[axis])/nodes[index].half[axis];
                        let row=lagrange(value,&x);for i in 0..p {matrix[i*p+column]=row[i];}
                    }nodes[child].transfer[axis]=matrix;
                }}
            }else {
                let mut weights=vec![0.;(nodes[index].end-nodes[index].start)*r];
                for(local,&cell)in order[nodes[index].start..nodes[index].end].iter().enumerate() {
                    let t=&mesh.copper_tetrahedra[cell];let c=&nodes[index];
                    polyhedron::tetra_integrate(t.nodes.map(|i|mesh.nodes_m[i]),t.volume_m3,&rule,|position,w| {
                        let values:[Vec<f64>;3]=std::array::from_fn(|axis|lagrange((position[axis]-c.center[axis])/c.half[axis],&x));
                        for i in 0..p {for j in 0..p {for k in 0..p {weights[local*r+(i*p+j)*p+k]+=w*values[0][i]*values[1][j]*values[2][k];}}}Ok(())
                    })?;
                    let sum=weights[local*r..(local+1)*r].iter().sum::<f64>();
                    if (sum-t.volume_m3).abs()>1e-12*t.volume_m3 {return Err(error("INTEGRAL_PARTITION","finite-volume interpolation weights lost constant partition"));}
                }nodes[index].weights=weights;
            }
        }
        let mut far=Vec::with_capacity(far_blocks.len());
        for(a,b)in far_blocks {
            clock(start,limit)?;let mut kernel=vec![0.;r*r];
            for i in 0..r {let pa=point(&nodes[a],i,&x);for j in 0..r {let pb=point(&nodes[b],j,&x);kernel[i*r+j]=1./length(std::array::from_fn(|k|pa[k]-pb[k]));}}
            far.push(Far{a,b,kernel});
        }
        let sources=mesh.copper_tetrahedra.iter().map(|t|PointPotential::tetrahedron(t.nodes.map(|i|mesh.nodes_m[i]))).collect::<Result<Vec<_>>>()?;
        // Covered by the existing64KiB/cell source-cache allowance.
        let surfaces=if surface{mesh.copper_tetrahedra.iter().map(|t|super::volume_surface::Source::new(t.nodes.map(|i|mesh.nodes_m[i]))).collect::<Result<Vec<_>>>()?}else{vec![]};
        let rules=[2,4,8,16,32,64].into_iter().filter(|&q|q<=near_order).map(|q|Ok((q,GaussRule::new(q)?))).collect::<Result<Vec<_>>>()?;
        let surface_condition=Maximum::new(0.);let surface_fallbacks=Counter::new();let compensated_calls=Counter::new();let compensated_condition=Maximum::new(0.);let stopped=AtomicBool::new(false);
        let worker_clock=||{if stopped.load(Ordering::Relaxed){Err(error("CANCELLED","another near constructor worker refused"))}else{clock(start,limit)}};
        let integral=|a:usize,b:usize,rule:&GaussRule|->Result<f64> {
            if surface{
                let result=surfaces[b].integral(&surfaces[a],rule,worker_clock);
                match result{
                    Ok((value,condition))=>{surface_condition.include(condition);if condition*128.*f64::EPSILON<=1e-9{return Ok(value);}if compensated_calls.get()<16{eprintln!("surface compensated fallback target{a}/source{b}: condition={condition:e}");}},
                    Err(e) if e.starts_with("LOCAL_SURFACE_RANGE:")=>{if compensated_calls.get()<16{eprintln!("surface compensated fallback target{a}/source{b}: {e}");}},
                    Err(e)=>return Err(e),
                }
                compensated_calls.increment();
                match surfaces[b].integral_compensated(&surfaces[a],rule.nodes().len(),worker_clock){
                    Ok((value,condition))=>{compensated_condition.include(condition);if condition*1024.*f64::EPSILON*f64::EPSILON<=1e-9{return Ok(value);}},
                    Err(e) if e.starts_with("LOCAL_SURFACE_RANGE:")=>{if surface_fallbacks.get()<16{eprintln!("compensated surface positive-volume fallback target{a}/source{b}: {e}");}},
                    Err(e)=>return Err(e),
                }
                surface_fallbacks.increment();
            }
            let t=&mesh.copper_tetrahedra[a];let mut result=Sum::default();
            let mut samples=0usize;polyhedron::tetra_integrate(t.nodes.map(|i|mesh.nodes_m[i]),t.volume_m3,rule,|p,w|{samples+=1;if samples%128==0 {clock(start,limit)?;}result=result.add(Sum::from(sources[b].at(p)?).mul(w));Ok(())})?;
            let v=result.value();if v>0.&&v.is_finite(){Ok(v)}else{Err(error("INTEGRAL_RANGE","positive finite near integral required"))}
        };
        let mut diagonal=vec![0.;n];
        for i in 0..n {
            clock(start,limit)?;let t=mesh.copper_tetrahedra[i].nodes.map(|j|mesh.nodes_m[j]);
            let mut previous=polyhedron::tetra_self_integral(t,8)?;let mut admitted=None;
            let mut samples=vec![(8,previous)];
            for order in [16,32,64] {
                let value=polyhedron::tetra_self_integral(t,order)?;
                samples.push((order,value));
                if (value-previous).abs()/value<=1e-10 {admitted=Some(value);break;}previous=value;
            }
            diagonal[i]=admitted.ok_or_else(||error("TETRA_SELF_QUADRATURE",&format!("diagonal source{i} failed1e-10 refinement through order64; tetra={t:?}, samples={samples:?}; near pairs={near_pairs}, far blocks={}",far.len())))?;
        }
        let cache=if let Some((inputs,output))=checkpoint{Some(super::volume_integral_cache::Cache::new(mesh,p,near_order,&diagonal,budget,plan,inputs,output,||clock(start,limit))?)}else{None};
        let plan=cache.as_ref().map_or(plan,|c|c.planned_bytes);
        let reciprocity=Maximum::new(cache.as_ref().map_or(0.,|c|c.max_reciprocity));let cache=cache.map(Mutex::new);
        let mut near=Vec::with_capacity(near_pairs);
        for(a,b)in near_blocks {
            for ia in nodes[a].start..nodes[a].end {for ib in nodes[b].start..nodes[b].end {
                if a==b&&ib<ia {continue;}clock(start,limit)?;
                near.push(Near{a:order[ia],b:order[ib],integral:f64::NAN});
            }}
        }
        let evaluate=|pair:&mut Near|->Result<()> {
                worker_clock()?;let i=pair.a;let j=pair.b;
                let cached=if i!=j{if let Some(c)=&cache{c.lock().map_err(|_|error("INTEGRAL_CACHE","checkpoint mutex poisoned"))?.get(i,j)}else{None}}else{None};
                let record=|q,forward,reverse,previous_forward,previous_reverse|->Result<f64>{if let Some(c)=&cache{c.lock().map_err(|_|error("INTEGRAL_CACHE","checkpoint mutex poisoned"))?.put(i,j,q,forward,reverse,previous_forward,previous_reverse)}else{Ok(forward)}};
                let value=if i==j {diagonal[i]}else if let Some(value)=cached{value}else {
                    let mut samples:Vec<(usize,f64,f64)>=vec![];let mut admitted=None;
                    for (q,rule) in &rules {
                        clock(start,limit)?;
                        let forward=integral(i,j,rule).map_err(|e|format!("{e}; near target{i}, source{j}, order{q}"))?;
                        let reverse=integral(j,i,rule).map_err(|e|format!("{e}; reverse near target{j}, source{i}, order{q}"))?;
                        let discrepancy=(forward-reverse).abs()/(diagonal[i]*diagonal[j]).sqrt();
                        let converged=samples.last().is_some_and(|&(_,a,b)|(forward-a).abs()/forward<=0.002&&(reverse-b).abs()/reverse<=0.002);
                        samples.push((*q,forward,reverse));
                        if converged&&discrepancy<=1e-9 {reciprocity.include(discrepancy);let &(_,pa,pb)=&samples[samples.len()-2];admitted=Some(record(*q,forward,reverse,pa,pb)?);break;}
                    }
                    if admitted.is_none()&&near_order==64 {
                        let normalization=(diagonal[i]*diagonal[j]).sqrt();
                        let a=&mesh.copper_tetrahedra[i];let b=&mesh.copper_tetrahedra[j];
                        let forward=adaptive_integral(a.nodes.map(|n|mesh.nodes_m[n]),a.volume_m3,&sources[j],1e-11*normalization,start,limit).map_err(|e|format!("{e}; forward tetra pair{i}/{j}"))?;
                        let reverse=adaptive_integral(b.nodes.map(|n|mesh.nodes_m[n]),b.volume_m3,&sources[i],1e-11*normalization,start,limit).map_err(|e|format!("{e}; reverse tetra pair{i}/{j}"))?;
                        let discrepancy=(forward-reverse).abs()/normalization;
                        let &(_,previous_a,previous_b)=samples.last().unwrap();
                        if (forward-previous_a).abs()/forward<=0.002&&(reverse-previous_b).abs()/reverse<=0.002&&discrepancy<=1e-9 {
                            reciprocity.include(discrepancy);admitted=Some(record(0,forward,reverse,previous_a,previous_b)?);
                        }
                        samples.push((0,forward,reverse));
                    }
                    admitted.ok_or_else(||error("INTEGRAL_RECIPROCITY",&format!("tetra pair{i}/{j}, near ceiling{near_order}; directed samples={samples:?}; require0.2% refinement and1e-9 self-normalized reciprocity, no averaging")))?
                };pair.integral=value;Ok(())
        };
        if workers==1{for pair in &mut near{evaluate(pair)?;}}
        else{
            let chunks=Mutex::new(near.chunks_mut(16));
            let failures=std::thread::scope(|scope| {
                let mut handles=Vec::with_capacity(workers);let mut failures=vec![];
                for worker in 0..workers{
                    let run=||->Result<()>{loop{
                        if stopped.load(Ordering::Relaxed){return Ok(());}
                        let chunk=chunks.lock().map_err(|_|error("INTEGRAL_WORKER","chunk mutex poisoned"))?.next();
                        let Some(chunk)=chunk else{return Ok(());};
                        for pair in chunk{if stopped.load(Ordering::Relaxed){return Ok(());}if let Err(e)=evaluate(pair){stopped.store(true,Ordering::Relaxed);return Err(e);}}
                    }};
                    match std::thread::Builder::new().name(format!("p7-near-{worker}")).stack_size(524288).spawn_scoped(scope,run){
                        Ok(handle)=>handles.push((worker,handle)),
                        Err(e)=>{stopped.store(true,Ordering::Relaxed);failures.push(error("INTEGRAL_WORKER",&format!("spawn{worker}: {e}")));break;}
                    }
                }
                for(worker,handle)in handles{match handle.join(){Ok(Ok(()))=>(),Ok(Err(e))=>failures.push(format!("worker{worker}: {e}")),Err(_)=>failures.push(error("INTEGRAL_WORKER",&format!("worker{worker} panicked")))}}
                failures
            });
            if !failures.is_empty(){return Err(failures.join("; "));}
        }
        if near.iter().any(|p|!p.integral.is_finite()){return Err(error("INTEGRAL_WORKER","incomplete original near space; no operator"));}
        if surface{eprintln!("surface diagnostics: maximum condition={:e}, compensatedEvaluations={}, maximumCompensatedCondition={:e}, original positive-volume evaluations={}, nearPairs={near_pairs}, farBlocks={}",surface_condition.get(),compensated_calls.get(),compensated_condition.get(),surface_fallbacks.get(),far.len());}
        let(cached_pairs,written_pairs)=if let Some(c)=&cache{let c=c.lock().map_err(|_|error("INTEGRAL_CACHE","checkpoint mutex poisoned"))?;(c.reused,c.written)}else{(0,0)};if checkpoint.is_some(){eprintln!("near checkpoints: reused={cached_pairs}, written={written_pairs}");}
        Ok(Self{nodes,order,near,far_blocks:far.len(),far,p,planned_bytes:plan,near_pairs,max_directed_reciprocity:reciprocity.get(),max_surface_condition:surface_condition.get(),surface_volume_fallbacks:surface_fallbacks.get(),compensated_surface_evaluations:compensated_calls.get(),max_compensated_surface_condition:compensated_condition.get(),cached_pairs,written_pairs,constructor_workers:workers})
    }
    pub fn apply(&self,space:&Space,current:&[f64])->Result<Vec<f64>> {
        if current.len()!=space.current_count()||space.cells.len()!=self.order.len()||current.iter().any(|v|!v.is_finite()) {return Err(error("INTEGRAL_VECTOR","finite matching complete current space required"));}
        let r=self.p*self.p*self.p;let stride=3*r;
        let density:Vec<_>=space.cells.iter().enumerate().flat_map(|(i,c)|(0..3).map(move|k|current[3*i+k]*c.density_per_amp_m2)).collect();
        let mut moments=vec![0.;self.nodes.len()*stride];let mut local=vec![0.;moments.len()];
        for(index,c)in self.nodes.iter().enumerate().rev() {
            if let Some(children)=c.children {
                for child in children {
                    let shifted=transfer(&moments[child*stride..(child+1)*stride],&self.nodes[child].transfer,self.p,false);
                    for k in 0..stride {moments[index*stride+k]+=shifted[k];}
                }
            }else {
                for k in 0..r {for axis in 0..3 {moments[index*stride+3*k+axis]=(c.start..c.end).fold(Sum::default(),|sum,pos|sum.add(Sum::from(density[3*self.order[pos]+axis]).mul(c.weights[(pos-c.start)*r+k]))).value();}}
            }
        }
        for block in &self.far {
            for i in 0..r {for j in 0..r {let value=block.kernel[i*r+j];for axis in 0..3 {
                local[block.a*stride+3*i+axis]+=value*moments[block.b*stride+3*j+axis];
                local[block.b*stride+3*j+axis]+=value*moments[block.a*stride+3*i+axis];
            }}}
        }
        let mut response=vec![Sum::default();current.len()];
        for(index,c)in self.nodes.iter().enumerate() {
            if let Some(children)=c.children {
                for child in children {let shifted=transfer(&local[index*stride..(index+1)*stride],&self.nodes[child].transfer,self.p,true);for k in 0..stride {local[child*stride+k]+=shifted[k];}}
            }else {
                for pos in c.start..c.end {let i=self.order[pos];for axis in 0..3 {
                    response[3*i+axis]=(0..r).fold(response[3*i+axis],|sum,k|sum.add(Sum::from(local[index*stride+3*k+axis]).mul(c.weights[(pos-c.start)*r+k])));
                }}
            }
        }
        for pair in &self.near {for axis in 0..3 {
            response[3*pair.a+axis]=response[3*pair.a+axis].add(Sum::from(density[3*pair.b+axis]).mul(pair.integral));
            if pair.a!=pair.b {response[3*pair.b+axis]=response[3*pair.b+axis].add(Sum::from(density[3*pair.a+axis]).mul(pair.integral));}
        }}
        let out:Vec<_>=response.into_iter().enumerate().map(|(i,v)|v.mul(1e-7*space.cells[i/3].density_per_amp_m2).value()).collect();
        if out.iter().any(|x|!x.is_finite()){return Err(error("INTEGRAL_RANGE","nonfinite finite-volume Newton action"));}Ok(out)
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::*;
    #[test]
    fn actual_barrel_foil_edge_pair_keeps_directed_reciprocity() {
        // Frozen actual STM32 RS485 strip cells268/201. The uniform64 rule
        // failed reciprocity here; retain this geometry as a positive control.
        let a=[[0.0002182767391204834,0.00037389206886291506,0.0],[0.00018172311782836914,0.00037389206886291506,0.0],[0.00018172311782836914,0.00037389206886291506,3.5e-5],[0.0,0.00039999985694885254,3.5e-5]];
        let b=[[0.00019999980926513672,0.0003508124351501465,0.0],[0.0001664409637451172,0.0003470313549041748,0.0],[0.00018172311782836914,0.00037389206886291506,0.0],[0.00018172311782836914,0.00037389206886291506,3.5e-5]];
        let scale=(polyhedron::tetra_self_integral(a,64).unwrap()*polyhedron::tetra_self_integral(b,64).unwrap()).sqrt();
        let start=Instant::now();let limit=Duration::from_secs(90);
        let ab=adaptive_integral(a,super::super::mesh::tetra_coefficients(a).unwrap().0,&PointPotential::tetrahedron(b).unwrap(),1e-11*scale,start,limit).unwrap();
        let ba=adaptive_integral(b,super::super::mesh::tetra_coefficients(b).unwrap().0,&PointPotential::tetrahedron(a).unwrap(),1e-11*scale,start,limit).unwrap();
        eprintln!("actual edge-pair forward={ab:e}, reverse={ba:e}, self scale={scale:e}");
        assert!((ab-ba).abs()/scale<=1e-9);
        assert!(adaptive_integral(a,super::super::mesh::tetra_coefficients(a).unwrap().0,&PointPotential::tetrahedron(b).unwrap(),1e-11*scale,start,Duration::ZERO).unwrap_err().contains("DEADLINE"));
    }
}
