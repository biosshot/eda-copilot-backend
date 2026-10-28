//! Double divergence identity for original tetrahedron Newton integrals.
//! Signed boundary cancellation is measured; this is never an accuracy certificate.
use super::{accurate::Sum, error, prism::GaussRule, Result};
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|k|a[k]-b[k])}
fn dot(a:[f64;3],b:[f64;3])->f64{(0..3).fold(Sum::default(),|s,k|s.add(Sum::from(a[k]).mul(b[k]))).value()}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn norm(a:[f64;3])->f64{a[0].hypot(a[1]).hypot(a[2])}
#[derive(Clone)]
struct Face{vertices:[[f64;3];3],normal:[f64;3],det:f64,edges:[([f64;3],[f64;3],f64);3]}
pub struct Source{faces:[Face;4],tetra:[[f64;3];4],compensated:std::sync::OnceLock<Result<super::volume_surface_compensated::Source>>}
impl Source{
 pub fn new(tetra:[[f64;3];4])->Result<Self>{
  super::mesh::tetra_coefficients(tetra)?;
  let faces=std::array::from_fn(|opposite|{
   let mut v=[[0.;3];3];let mut j=0;for(i,&p)in tetra.iter().enumerate(){if i!=opposite{v[j]=p;j+=1;}}
   let mut c=cross(sub(v[1],v[0]),sub(v[2],v[0]));
   if dot(c,sub(tetra[opposite],v[0]))>0.{v.swap(1,2);c=c.map(|x|-x);}
   let det=norm(c);let normal=c.map(|x|x/det);
   let edges=std::array::from_fn(|i|{let edge=sub(v[(i+1)%3],v[i]);let length=norm(edge);let tangent=edge.map(|x|x/length);(tangent,cross(tangent,normal),length)});
   Face{vertices:v,normal,det,edges}
  });Ok(Self{faces,tetra,compensated:std::sync::OnceLock::new()})
 }
 pub fn integral_compensated(&self,target:&Self,order:usize,check:impl FnMut()->Result<()>)->Result<(f64,f64)>{
  let source=self.compensated.get_or_init(||super::volume_surface_compensated::Source::new(self.tetra)).as_ref().map_err(|e|e.clone())?;
  let target=target.compensated.get_or_init(||super::volume_surface_compensated::Source::new(target.tetra)).as_ref().map_err(|e|e.clone())?;
  source.integral(target,order,check)
 }
 /// Integrate a source face's distance kernel, with analytic line primitives.
 fn face_distance(face:&Face,point:[f64;3])->Result<f64>{
  let r=face.vertices.map(|v|sub(v,point));let radius=r.map(norm);let h=dot(r[0],face.normal);
  let denominator=Sum::from(radius[0]).mul(radius[1]).mul(radius[2]).add(Sum::from(dot(r[1],r[2])).mul(radius[0])).add(Sum::from(dot(r[0],r[2])).mul(radius[1])).add(Sum::from(dot(r[0],r[1])).mul(radius[2])).value();
  let omega=2.*dot(r[0],cross(r[1],r[2])).atan2(denominator);
  let mut edge_l=Sum::default();let mut edge_j=Sum::default();
  for(i,&(tangent,outward,length))in face.edges.iter().enumerate(){
   let j=(i+1)%3;let t=dot(r[i],tangent);let s=dot(r[i],outward);
   let perpendicular=std::array::from_fn(|k|r[i][k]-t*tangent[k]);let a2=dot(perpendicular,perpendicular);
   // Stable positive ra+rb-length even when the projection lies in the edge.
   let da=radius[i]+t.abs();let db=radius[j]+(t+length).abs();
   let gap=(if da>0.{a2/da}else{0.})+(if db>0.{a2/db}else{0.})+(t.abs()+(t+length).abs()-length).max(0.);
   if gap==0.&&a2==0.&&s==0.{continue;}
   if !(gap>0.){return Err(error("SURFACE_RANGE","singular edge limit not admitted"));}
   let logarithm=(2.*length/gap).ln_1p();
   let integral=Sum::from(radius[j]).add(Sum::from(t).mul((2.*t+length)/(radius[i]+radius[j]))).mul(0.5*length).add(Sum::from(logarithm).mul(0.5*a2));
   edge_l=edge_l.add(Sum::from(logarithm).mul(s));edge_j=edge_j.add(integral.mul(s));
  }
  let value=edge_j.add(edge_l.sub(Sum::from(omega).mul(h)).mul(h*h)).mul(1./3.).value();
  if value>0.&&value.is_finite(){Ok(value)}else{Err(error("SURFACE_RANGE","nonpositive analytic source-face distance integral"))}
 }
 pub fn integral(&self,target:&Self,rule:&GaussRule,mut check:impl FnMut()->Result<()>)->Result<(f64,f64)>{
  let mut sum=Sum::default();let mut absolute=0.;let mut samples=0;
  for a in &target.faces{for b in &self.faces{
   let coefficient=-0.5*dot(a.normal,b.normal)*a.det;if coefficient==0.{continue;}
   let mut face=Sum::default();
   for(&u,&wu)in rule.nodes().iter().zip(rule.weights()){for(&v,&wv)in rule.nodes().iter().zip(rule.weights()){
    samples+=1;if samples%128==0{check()?;}
    let bary=[1.-u,u*(1.-v),u*v];let point=std::array::from_fn(|k|(0..3).fold(Sum::default(),|s,i|s.add(Sum::from(a.vertices[i][k]).mul(bary[i]))).value());
    face=face.add(Sum::from(Self::face_distance(b,point)?).mul(wu*wv*u));
   }}let term=face.mul(coefficient);absolute+=term.value().abs();sum=sum.add(term);
  }}let value=sum.value();
  if value>0.&&value.is_finite(){Ok((value,absolute/value))}else{Err(error("SURFACE_RANGE",&format!("nonpositive signed double-boundary Newton integral: value={value:e}, absoluteTerms={absolute:e}")))}
 }
}
