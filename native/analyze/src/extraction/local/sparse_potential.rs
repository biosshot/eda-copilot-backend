//! Resource admission before sparse potential ordering, symbolic fill and numeric
//! factor allocation. Pinned faer0.24.4: no supernode amalgamation, so stored
//! dense-panel values are bounded by twice the exact simplicial fill count.
use super::{error,AllocationBudget,Result};
use faer::{dyn_stack::{MemBuffer,MemStack,StackReq},sparse::{linalg::{amd,cholesky,SymbolicSupernodalParams,SupernodalThreshold},SparseColMat,SymbolicSparseColMat},Side};
fn add(base:usize,count:usize,width:usize)->Result<usize>{base.checked_add(count.checked_mul(width).ok_or_else(||error("MEMORY_LIMIT","sparse phase plan overflow"))?).ok_or_else(||error("MEMORY_LIMIT","sparse phase plan overflow"))}
fn admit(budget:AllocationBudget,plan:usize,phase:&str)->Result<()> {budget.check(plan).map_err(|e|format!("{e}; sparse {phase}: requestedBytes={plan}, admittedBytes={}",budget.max_bytes))}
// Pinned faer0.24.4, Custom ordering, relax=None. Its AUTO selector uses
// (division+LDL multiplication flops)/simplicial fill >40. Fundamental
// supernodes store one shared row-index panel, not a full simplicial pattern.
#[derive(Debug)]
struct Layout {supernodal:bool,pattern:usize,values:usize}
fn layout(tree:&[isize],counts:&[usize])->Result<Layout>{
 let n=counts.len();if n==0||tree.len()!=n||counts.contains(&0){return Err(error("MAGNETIC_MATRIX","invalid symbolic prefill dimensions"));}
 let fill=counts.iter().try_fold(0usize,|a,&b|a.checked_add(b)).ok_or_else(||error("MEMORY_LIMIT","symbolic fill overflow"))?;
 let mut divisions=0u128;let mut products=0u128;for &c in counts{let c=c as u128;divisions+=c;products+=c*(c+1)/2;}
 let supernodal=(divisions as f64+products as f64)/fill as f64>40.;
 if !supernodal{return Ok(Layout{supernodal:false,pattern:fill,values:fill});}
 let mut children=vec![0usize;n];for (i,&parent) in tree.iter().enumerate(){if parent>=0{let parent=parent as usize;if parent>=n||parent<=i{return Err(error("MAGNETIC_MATRIX","invalid symbolic elimination tree"));}children[parent]+=1;}}
 let mut begin=0;let mut pattern=0usize;let mut values=0usize;
 for end in 1..=n {let merge=end<n&&tree[end-1]==end as isize&&children[end]==1&&counts[end-1]==counts[end]+1;if merge{continue;}
  let width=end-begin;let rows=counts[end-1]-1;pattern=pattern.checked_add(rows).ok_or_else(||error("MEMORY_LIMIT","supernode pattern overflow"))?;values=values.checked_add(width.checked_mul(width+rows).ok_or_else(||error("MEMORY_LIMIT","supernode panel overflow"))?).ok_or_else(||error("MEMORY_LIMIT","supernode panel overflow"))?;begin=end;
 }
 Ok(Layout{supernodal,pattern,values})
}
pub(crate) fn factor(matrix:&SparseColMat<usize,f64>,base:usize,budget:AllocationBudget,private:&[usize])->Result<(cholesky::SymbolicCholesky<usize>,Vec<f64>,usize,usize)> {
 let factor_started=std::time::Instant::now();
 let n=matrix.nrows();let nnz=matrix.compute_nnz();let live=add(base,nnz,16)?;let mut peak=live;
 let order_req=amd::order_scratch::<usize>(n,nnz);
 let order_plan=add(add(live,n,16)?,order_req.unaligned_bytes_required(),1)?;admit(budget,order_plan,"ordering")?;peak=peak.max(order_plan);
 let mut perm=vec![0usize;n];let mut inverse=vec![0usize;n];
 if private.is_empty() {
  let mut memory=MemBuffer::try_new(order_req).map_err(|_|error("MEMORY_LIMIT","sparse ordering workspace"))?;amd::order(&mut perm,&mut inverse,matrix.symbolic(),Default::default(),MemStack::new(&mut memory)).map_err(|e|error("MAGNETIC_MATRIX",&e.to_string()))?;
 }else{
  // Cell-interior basis functions vanish on every face. Eliminate these exact
  // local blocks first; the trace Schur graph is already the cell trace clique.
  let trace_bound=add(add(add(live,nnz,8)?,n,64)?,order_req.unaligned_bytes_required(),1)?;admit(budget,trace_bound,"trace-ordering-workspace")?;peak=peak.max(trace_bound);
  let mut map=vec![usize::MAX;n];for &i in private {if i>=n||map[i]!=usize::MAX{return Err(error("MAGNETIC_MATRIX","invalid cell-private elimination order"));}map[i]=n;}
  let mut trace=Vec::new();for i in 0..n {if map[i]==usize::MAX {map[i]=trace.len();trace.push(i);}}
  let m=trace.len();let mut pointer=vec![0usize;m+1];let mut rows=Vec::with_capacity(nnz);
  for &j in &trace {for &i in &matrix.symbolic().row_idx()[matrix.symbolic().col_ptr()[j]..matrix.symbolic().col_ptr()[j+1]] {if map[i]<m {rows.push(map[i]);}}pointer[map[j]+1]=rows.len();}
  let trace_capacity=rows.capacity();let graph=SymbolicSparseColMat::new_checked(m,m,pointer,None,rows);let req=amd::order_scratch::<usize>(m,graph.compute_nnz());
  let phase=add(add(add(live,trace_capacity,8)?,n,48)?,req.unaligned_bytes_required(),1)?;admit(budget,phase,"trace-ordering")?;peak=peak.max(phase);
  let mut p=vec![0usize;m];let mut inv=vec![0usize;m];let mut memory=MemBuffer::try_new(req).map_err(|_|error("MEMORY_LIMIT","trace ordering workspace"))?;
  amd::order(&mut p,&mut inv,graph.as_ref(),Default::default(),MemStack::new(&mut memory)).map_err(|e|error("MAGNETIC_MATRIX",&e.to_string()))?;
  for (j,&i) in private.iter().enumerate(){perm[j]=i;}for(j,&i)in p.iter().enumerate(){perm[private.len()+j]=trace[i];}for(j,&i)in perm.iter().enumerate(){inverse[i]=j;}
  drop(graph);drop(memory);drop(map);drop(p);drop(inv);
 }
 let pre_req=cholesky::simplicial::prefactorize_symbolic_cholesky_scratch::<usize>(n,nnz);
 let pre_plan=add(add(add(live,nnz,8)?,n,64)?,pre_req.unaligned_bytes_required(),1)?;admit(budget,pre_plan,"fill-count")?;peak=peak.max(pre_plan);
 // Build only a permuted upper pattern; numeric coefficients stay untouched.
 let mut pointer=vec![0usize;n+1];
 for j in 0..n {for &i in &matrix.symbolic().row_idx()[matrix.symbolic().col_ptr()[j]..matrix.symbolic().col_ptr()[j+1]] {pointer[inverse[i].max(inverse[j])+1]+=1;}}
 for i in 0..n {pointer[i+1]+=pointer[i];}
 let mut rows=vec![0usize;pointer[n]];let mut cursor=pointer[..n].to_vec();
 for j in 0..n {for &i in &matrix.symbolic().row_idx()[matrix.symbolic().col_ptr()[j]..matrix.symbolic().col_ptr()[j+1]] {let a=inverse[i].min(inverse[j]);let b=inverse[i].max(inverse[j]);rows[cursor[b]]=a;cursor[b]+=1;}}
 let upper=SymbolicSparseColMat::new_unsorted_checked(n,n,pointer,None,rows);let mut tree=vec![0isize;n];let mut counts=vec![0usize;n];
 {let mut memory=MemBuffer::try_new(pre_req).map_err(|_|error("MEMORY_LIMIT","sparse prefill workspace"))?;cholesky::simplicial::prefactorize_symbolic_cholesky(&mut tree,&mut counts,upper.as_ref(),MemStack::new(&mut memory));}
 let fill=counts.iter().try_fold(0usize,|a,&b|a.checked_add(b)).ok_or_else(||error("MEMORY_LIMIT","sparse fill count overflow"))?;
 let layout=layout(&tree,&counts)?;
 eprintln!("sparse potential n={n} upper={nnz} private={} exact-fill={fill}",private.len());
 drop(upper);drop(cursor);drop(tree);drop(counts);
 let n_req=StackReq::new::<usize>(n);
 let analysis_req=StackReq::all_of(&[StackReq::new::<usize>(n+1),StackReq::new::<usize>(nnz),n_req,n_req,n_req,StackReq::or(cholesky::supernodal::factorize_supernodal_symbolic_cholesky_scratch::<usize>(n),cholesky::simplicial::factorize_simplicial_symbolic_cholesky_scratch::<usize>(n))]);
 // During supernodal construction: shared row indices, their compressed
 // copy and row-major conversion coexist. Heap pointers/permutations remain
 // covered by288 bytes/node, in addition to the library's queried stack.
 let pattern_width=if layout.supernodal{24}else{8};
 let symbolic_plan=add(add(add(add(live,layout.pattern,pattern_width)?,n,288)?,1024,1)?,analysis_req.unaligned_bytes_required(),1)?;admit(budget,symbolic_plan,"symbolic-pattern")?;peak=peak.max(symbolic_plan);
 let parameters=cholesky::CholeskySymbolicParams{supernodal_flop_ratio_threshold:if layout.supernodal{SupernodalThreshold::FORCE_SUPERNODAL}else{SupernodalThreshold::FORCE_SIMPLICIAL},supernodal_params:SymbolicSupernodalParams{relax:None},..Default::default()};
 let symbolic_started=std::time::Instant::now();
 let ordering_seconds=factor_started.elapsed().as_secs_f64();
 let symbolic=cholesky::factorize_symbolic_cholesky(matrix.symbolic(),Side::Upper,cholesky::SymmetricOrdering::Custom(faer::perm::PermRef::new_checked(&perm,&inverse,n)),parameters).map_err(|e|error("MAGNETIC_MATRIX",&e.to_string()))?;
 drop(perm);drop(inverse);
 let actual_pattern=match symbolic.raw(){cholesky::SymbolicCholeskyRaw::Supernodal(s)=>s.row_idx().len(),cholesky::SymbolicCholeskyRaw::Simplicial(s)=>s.row_idx().len()};
 if actual_pattern!=layout.pattern||symbolic.len_val()!=layout.values{return Err(error("MEMORY_LIMIT","pinned unrelaxed symbolic storage differed from its preallocation proof"));}
 let symbolic_bytes=add(1024,layout.pattern,if layout.supernodal{16}else{8})?;
 if symbolic.len_val()>fill.checked_mul(2).ok_or_else(||error("MEMORY_LIMIT","sparse fill bound overflow"))? {return Err(error("MEMORY_LIMIT","unamalgamated panel fill exceeded predeclared symbolic bound"));}
 let scratch=symbolic.factorize_numeric_llt_scratch::<f64>(super::parallel::current(),Default::default());let solve=symbolic.solve_in_place_scratch::<f64>(9,super::parallel::current());
 let numeric_plan=add(add(add(add(live,symbolic_bytes,1)?,n,256)?,symbolic.len_val(),8)?,scratch.unaligned_bytes_required().max(solve.unaligned_bytes_required()),1)?;admit(budget,numeric_plan,"numeric-factor")?;peak=peak.max(numeric_plan);
 let symbolic_seconds=symbolic_started.elapsed().as_secs_f64();
 let numeric_started=std::time::Instant::now();
 let mut values=vec![0.;symbolic.len_val()];let mut workspace=MemBuffer::try_new(scratch).map_err(|_|error("MEMORY_LIMIT","sparse numeric workspace"))?;
 symbolic.factorize_numeric_llt(&mut values,matrix.as_ref(),Side::Upper,Default::default(),super::parallel::current(),MemStack::new(&mut workspace),Default::default()).map_err(|_|error("MAGNETIC_SINGULAR","potential Laplacian unresolved; no regularization"))?;
 eprintln!("P7_SYMBOLIC n={n} sharedPattern={actual_pattern} panelValues={} symbolicOwnedBound={symbolic_bytes}",symbolic.len_val());
 eprintln!("P7_FACTOR n={n} orderingAndPrefill={ordering_seconds:.6}s symbolic={symbolic_seconds:.6}s numeric={:.6}s parallelism={:?} kind={}",numeric_started.elapsed().as_secs_f64(),super::parallel::current(),if matches!(symbolic.raw(),cholesky::SymbolicCholeskyRaw::Supernodal(_)){"supernodal"}else{"simplicial"});
 let persistent=add(add(add(live,symbolic_bytes,1)?,n,256)?,symbolic.len_val(),8)?;
 Ok((symbolic,values,peak,persistent))
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn pinned_auto_layout_and_owned_storage_match_without_simplicial_overcharge(){
  use faer::sparse::Triplet;
  for (n,dense,full) in [(1,false,false),(128,true,false),(256,false,false),(128,true,true),(256,false,true)]{
   let mut entries=vec![];for j in 0..n{entries.push(Triplet::new(j,j,n as f64+2.));for i in 0..j{if dense||i+1==j{entries.push(Triplet::new(i,j,-0.1));if full{entries.push(Triplet::new(j,i,-0.1));}}}}
   let matrix=SparseColMat::<usize,f64>::try_new_from_triplets(n,n,&entries).unwrap();let upper_entries=entries.iter().filter(|e|e.row<=e.col).copied().collect::<Vec<_>>();let upper=SparseColMat::<usize,f64>::try_new_from_triplets(n,n,&upper_entries).unwrap();let mut tree=vec![0isize;n];let mut counts=vec![0usize;n];let mut memory=MemBuffer::new(cholesky::simplicial::prefactorize_symbolic_cholesky_scratch::<usize>(n,upper.compute_nnz()));cholesky::simplicial::prefactorize_symbolic_cholesky(&mut tree,&mut counts,upper.symbolic(),MemStack::new(&mut memory));let plan=layout(&tree,&counts).unwrap();
   let auto=cholesky::factorize_symbolic_cholesky(matrix.symbolic(),Side::Upper,cholesky::SymmetricOrdering::Identity,cholesky::CholeskySymbolicParams{supernodal_params:SymbolicSupernodalParams{relax:None},..Default::default()}).unwrap();
   assert_eq!(plan.supernodal,matches!(auto.raw(),cholesky::SymbolicCholeskyRaw::Supernodal(_)));assert_eq!(plan.values,auto.len_val());let stored=match auto.raw(){cholesky::SymbolicCholeskyRaw::Supernodal(s)=>s.row_idx().len(),cholesky::SymbolicCholeskyRaw::Simplicial(s)=>s.row_idx().len()};assert_eq!(stored,plan.pattern);
   let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:10000,max_tetrahedra:10000};let (symbolic,values,peak,persistent)=factor(&matrix,0,budget,&[]).unwrap();assert!(peak>=persistent);let mut rhs=faer::Mat::from_fn(n,1,|_,_|1.);let mut scratch=MemBuffer::new(symbolic.solve_in_place_scratch::<f64>(1,faer::Par::Seq));cholesky::LltRef::new(&symbolic,&values).solve_in_place_with_conj(faer::Conj::No,rhs.as_mut(),faer::Par::Seq,MemStack::new(&mut scratch));for i in 0..n{let actual=(n as f64+2.)*rhs[(i,0)]-(0..n).filter(|&j|j!=i&&(dense||j+1==i||i+1==j)).map(|j|0.1*rhs[(j,0)]).sum::<f64>();assert!((actual-1.).abs()<1e-12);}
  }
 }
}
