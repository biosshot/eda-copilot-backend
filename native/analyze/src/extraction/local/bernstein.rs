//! Exact scalar Bernstein simplex moments for source-resolving H1 potentials.
use super::{error,Result};
pub(crate) fn indices(p:usize)->Vec<[usize;4]> {
 let mut out=vec![];for a in 0..=p {for b in 0..=p-a {for c in 0..=p-a-b {out.push([a,b,c,p-a-b-c]);}}}out
}
fn factorial(n:usize)->f64 {(1..=n).map(|x|x as f64).product()}
fn coefficient(a:[usize;4])->f64 {factorial(a.iter().sum())/a.into_iter().map(factorial).product::<f64>()}
pub(crate) struct Gradients {terms:Vec<Vec<([usize;4],f64,usize)>>,degree:usize}
impl Gradients {
 pub(crate) fn new(alpha:&[[usize;4]])->Self {
  let degree=alpha.iter().map(|a|a.iter().sum::<usize>()).max().unwrap_or(0);
  let terms=alpha.iter().map(|a|(0..4).filter(|&i|a[i]>0).map(|i|{let mut b=*a;b[i]-=1;(b,a.iter().sum::<usize>() as f64*coefficient(b),i)}).collect()).collect();Self{terms,degree}
 }
 pub(crate) fn at(&self,lambda:[f64;4],g:[[f64;3];4],out:&mut[[f64;3]]) {
  let mut powers=[[1.;9];4];for i in 0..4 {for j in 1..=self.degree {powers[i][j]=powers[i][j-1]*lambda[i];}}
  for (value,terms) in out.iter_mut().zip(&self.terms) {*value=[0.;3];for &(a,c,i) in terms {let w=c*(0..4).map(|j|powers[j][a[j]]).product::<f64>();for k in 0..3 {value[k]+=w*g[i][k];}}}
 }
}
pub(crate) fn mixed_stiffness_weights(alpha:&[[usize;4]])->Result<Vec<[f64;16]>> {
 if alpha.iter().any(|a|!(1..=8).contains(&a.iter().sum::<usize>())) {return Err(error("MAGNETIC_DEGREE","Bernstein mixed degrees1..8 required"));}
 let mut weights=Vec::with_capacity(alpha.len()*alpha.len());for a in alpha {for b in alpha {let mut row=[0.;16];
  let p=a.iter().sum::<usize>();let q=b.iter().sum::<usize>();
  for i in 0..4 {for j in 0..4 {if a[i]>0&&b[j]>0 {
   let mut u=*a;u[i]-=1;let mut v=*b;v[j]-=1;
   row[4*i+j]=(p*q) as f64*coefficient(u)*coefficient(v)*6.*(0..4).map(|k|factorial(u[k]+v[k])).product::<f64>()/factorial(p+q+1);
  }}}weights.push(row);
 }}Ok(weights)
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn exact_bernstein_partition_moments_and_stiffness_match_positive_quadrature() {
  let tetra=[[0.,0.,0.],[0.003,0.,0.],[0.0002,0.0007,0.],[0.0001,0.0002,0.0004]];
  let(volume,g)=super::super::mesh::tetra_coefficients(tetra).unwrap();
  let mut cases=(1..=4).map(|p|{let a=indices(p);let n=a.len();(a,p+2,n)}).collect::<Vec<_>>();
  for p in [6,8] {let mut a=indices(3);a.extend(indices(p).into_iter().filter(|a|a.iter().all(|&i|i>0)));cases.push((a,p+2,20));}
  let mut entity=vec![[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]];
  entity.extend(indices(5).into_iter().filter(|a|[2,3].contains(&a.iter().filter(|&&n|n>0).count())));
  entity.extend(indices(6).into_iter().filter(|a|a.iter().all(|&n|n>0)));cases.push((entity,8,4));
  for (a,order,partition) in cases {let n=a.len();let weights=mixed_stiffness_weights(&a).unwrap();let rule=super::super::prism::GaussRule::new(order).unwrap();let mut gram=vec![0.;n*n];let mut moments=vec![0.;n];let derivative=Gradients::new(&a);let mut shape=vec![[0.;3];n];
   super::super::polyhedron::tetra_integrate(tetra,volume,&rule,|x,w| {
    let mut lambda=[0.;4];for i in 1..4 {lambda[i]=(0..3).map(|k|g[i][k]*(x[k]-tetra[0][k])).sum();}lambda[0]=1.-lambda[1]-lambda[2]-lambda[3];
    derivative.at(lambda,g,&mut shape);
    let values:Vec<_>=a.iter().map(|b|coefficient(*b)*(0..4).map(|i|lambda[i].powi(b[i]as i32)).product::<f64>()).collect();
    assert!((values[..partition].iter().sum::<f64>()-1.).abs()<1e-13);
    for i in 0..n {moments[i]+=w*values[i];for j in 0..n {gram[i*n+j]+=w*(0..3).map(|k|shape[i][k]*shape[j][k]).sum::<f64>();}}Ok(())
   }).unwrap();
   for (i,m) in moments.into_iter().enumerate() {let p=a[i].iter().sum::<usize>();let mean=volume*6./((p+1)*(p+2)*(p+3))as f64;assert!((m-mean).abs()<1e-12*volume);}
   let scale=gram.iter().map(|x|x.abs()).fold(0.,f64::max);
   for i in 0..n {for j in 0..n {let exact=volume*(0..4).flat_map(|u|(0..4).map(move|v|(u,v))).map(|(u,v)|weights[i*n+j][4*u+v]*(0..3).map(|k|g[u][k]*g[v][k]).sum::<f64>()).sum::<f64>();assert!((exact-gram[i*n+j]).abs()<1e-12*scale);}}
  }
 }
}
