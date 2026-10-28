//! Resource admission before sparse potential ordering, symbolic fill and numeric
//! factor allocation. Pinned faer0.24.4: no supernode amalgamation, so stored
//! dense-panel values are bounded by twice the exact simplicial fill count.
use super::{error,AllocationBudget,Result};
use faer::{dyn_stack::{MemBuffer,MemStack,StackReq},sparse::{linalg::{amd,cholesky,SymbolicSupernodalParams},SparseColMat,SymbolicSparseColMat},Par,Side};
fn add(base:usize,count:usize,width:usize)->Result<usize>{base.checked_add(count.checked_mul(width).ok_or_else(||error("MEMORY_LIMIT","sparse phase plan overflow"))?).ok_or_else(||error("MEMORY_LIMIT","sparse phase plan overflow"))}
fn admit(budget:AllocationBudget,plan:usize,phase:&str)->Result<()> {budget.check(plan).map_err(|e|format!("{e}; sparse {phase}: requestedBytes={plan}, admittedBytes={}",budget.max_bytes))}
pub(crate) fn factor(matrix:&SparseColMat<usize,f64>,base:usize,budget:AllocationBudget,private:&[usize])->Result<(cholesky::SymbolicCholesky<usize>,Vec<f64>,usize,usize)> {
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
  let m=trace.len();let mut pointer=vec![0usize;m+1];let mut rows=Vec::new();
  for &j in &trace {for &i in &matrix.symbolic().row_idx()[matrix.symbolic().col_ptr()[j]..matrix.symbolic().col_ptr()[j+1]] {if map[i]<m {rows.push(map[i]);}}pointer[map[j]+1]=rows.len();}
  let graph=SymbolicSparseColMat::new_checked(m,m,pointer,None,rows);let req=amd::order_scratch::<usize>(m,graph.compute_nnz());
  let phase=add(add(add(live,graph.compute_nnz(),8)?,n,48)?,req.unaligned_bytes_required(),1)?;admit(budget,phase,"trace-ordering")?;peak=peak.max(phase);
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
 eprintln!("sparse potential n={n} upper={nnz} private={} exact-fill={fill}",private.len());
 drop(upper);drop(cursor);drop(tree);drop(counts);
 let n_req=StackReq::new::<usize>(n);
 let analysis_req=StackReq::all_of(&[StackReq::new::<usize>(n+1),StackReq::new::<usize>(nnz),n_req,n_req,n_req,StackReq::or(cholesky::supernodal::factorize_supernodal_symbolic_cholesky_scratch::<usize>(n),cholesky::simplicial::factorize_simplicial_symbolic_cholesky_scratch::<usize>(n))]);
 let symbolic_plan=add(add(add(live,fill,8)?,n,288)?,analysis_req.unaligned_bytes_required(),1)?;admit(budget,symbolic_plan,"symbolic-pattern")?;peak=peak.max(symbolic_plan);
 let parameters=cholesky::CholeskySymbolicParams{supernodal_params:SymbolicSupernodalParams{relax:None},..Default::default()};
 let symbolic=cholesky::factorize_symbolic_cholesky(matrix.symbolic(),Side::Upper,cholesky::SymmetricOrdering::Custom(faer::perm::PermRef::new_checked(&perm,&inverse,n)),parameters).map_err(|e|error("MAGNETIC_MATRIX",&e.to_string()))?;
 drop(perm);drop(inverse);
 if symbolic.len_val()>fill.checked_mul(2).ok_or_else(||error("MEMORY_LIMIT","sparse fill bound overflow"))? {return Err(error("MEMORY_LIMIT","unamalgamated panel fill exceeded predeclared symbolic bound"));}
 let scratch=symbolic.factorize_numeric_llt_scratch::<f64>(Par::Seq,Default::default());let solve=symbolic.solve_in_place_scratch::<f64>(9,Par::Seq);
 let numeric_plan=add(add(add(add(live,fill,8)?,n,256)?,symbolic.len_val(),8)?,scratch.unaligned_bytes_required().max(solve.unaligned_bytes_required()),1)?;admit(budget,numeric_plan,"numeric-factor")?;peak=peak.max(numeric_plan);
 let mut values=vec![0.;symbolic.len_val()];let mut workspace=MemBuffer::try_new(scratch).map_err(|_|error("MEMORY_LIMIT","sparse numeric workspace"))?;
 symbolic.factorize_numeric_llt(&mut values,matrix.as_ref(),Side::Upper,Default::default(),Par::Seq,MemStack::new(&mut workspace),Default::default()).map_err(|_|error("MAGNETIC_SINGULAR","potential Laplacian unresolved; no regularization"))?;
 let persistent=add(add(add(live,fill,8)?,n,256)?,symbolic.len_val(),8)?;
 Ok((symbolic,values,peak,persistent))
}
