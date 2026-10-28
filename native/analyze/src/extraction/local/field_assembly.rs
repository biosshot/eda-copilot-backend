//! Deduplicated P1 adjacency before numerical sparse storage; no element removal.
use super::{accurate::Sum,error,mesh::{self,LayeredMesh},AllocationBudget,Result};
use faer::sparse::{SparseColMat,SymbolicSparseColMat};
fn add(base:usize,n:usize,width:usize)->Result<usize>{base.checked_add(n.checked_mul(width).ok_or_else(||error("MEMORY_LIMIT","field sparse count overflow"))?).ok_or_else(||error("MEMORY_LIMIT","field sparse phase overflow"))}
pub(super) fn assemble(mesh:&LayeredMesh,free_map:&[usize],free:usize,base:usize,budget:AllocationBudget)->Result<(Vec<(usize,usize,f64)>,Vec<(usize,usize,f64)>,SparseColMat<usize,f64>,usize)>{
 let n=mesh.nodes_m.len();let raw=mesh.tetrahedra.len().checked_mul(16).ok_or_else(||error("MEMORY_LIMIT","field adjacency overflow"))?;
 // Raw adjacency and its possible compact reallocation coexist during shrink.
 let mut peak=add(add(base,raw,16)?,n+1,32)?;budget.check(peak)?;
 let mut offsets=vec![0usize;n+1];for t in &mesh.tetrahedra{for &a in &t.nodes{offsets[a+1]+=4;}}
 for i in 0..n{offsets[i+1]+=offsets[i];}let mut cursor=offsets[..n].to_vec();let mut rows=vec![0usize;raw];
 for t in &mesh.tetrahedra{for &a in &t.nodes{for &b in &t.nodes{rows[cursor[a]]=b;cursor[a]+=1;}}}drop(cursor);
 let mut pointer=vec![0usize;n+1];let mut write=0;
 for a in 0..n{rows[offsets[a]..offsets[a+1]].sort_unstable();let mut previous=None;for read in offsets[a]..offsets[a+1]{let b=rows[read];if previous!=Some(b){rows[write]=b;write+=1;previous=Some(b);}}pointer[a+1]=write;}
 rows.truncate(write);rows.shrink_to_fit();drop(offsets);let nnz=rows.len();
 // Pattern, two compensated accumulators, final absolute/RHS coefficients and
 // an upper free matrix all coexist only in this bounded assembly phase.
 let assembly=add(add(base,nnz,112)?,n+free+2,32)?;budget.check(assembly)?;peak=peak.max(assembly);
 let mut signed=vec![Sum::default();nnz];let mut absolute=vec![Sum::default();nnz];
 for tet in &mesh.tetrahedra{let k=mesh::stiffness(tet)?;for i in 0..4{for j in 0..4{let(a,b)=(tet.nodes[i],tet.nodes[j]);let at=pointer[a]+rows[pointer[a]..pointer[a+1]].binary_search(&b).map_err(|_|error("FIELD_MATRIX","element absent from complete adjacency"))?;let v=k[i][j]/super::super::EPS0;signed[at]=signed[at].add(Sum::from(v));absolute[at]=absolute[at].add(Sum::from(v.abs()));}}}
 let mut upper_pointer=vec![0usize;free+1];let mut rhs_count=0;
 for a in 0..n{if free_map[a]==usize::MAX{continue;}for at in pointer[a]..pointer[a+1]{let b=rows[at];if free_map[b]==usize::MAX{rhs_count+=1;}else if a<=b{upper_pointer[free_map[b]+1]+=1;}}}
 for i in 0..free{upper_pointer[i+1]+=upper_pointer[i];}let upper=upper_pointer[free];let mut upper_rows=vec![0usize;upper];let mut values=vec![0.;upper];let mut cursor=upper_pointer[..free].to_vec();
 let mut terms=Vec::with_capacity(nnz);let mut rhs=Vec::with_capacity(rhs_count);
 for a in 0..n{for at in pointer[a]..pointer[a+1]{let b=rows[at];terms.push((a,b,absolute[at].value()));if free_map[a]!=usize::MAX{if free_map[b]==usize::MAX{rhs.push((a,b,signed[at].value()));}else if a<=b{let col=free_map[b];let target=cursor[col];upper_rows[target]=free_map[a];values[target]=signed[at].value();cursor[col]+=1;}}}}
 let matrix=SparseColMat::new(SymbolicSparseColMat::new_checked(free,free,upper_pointer,None,upper_rows),values);
 Ok((terms,rhs,matrix,peak))
}
