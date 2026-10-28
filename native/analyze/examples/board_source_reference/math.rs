//! Reference-only finite-source1/r potential with positive3D target cubature.
//! This does not call the production signed double-boundary pair integrator.
#![allow(dead_code)]
use pcb_analyze::extraction::local::{mesh,polyhedron};
#[path="../../src/extraction/local/accurate.rs"]mod accurate;
#[path="../../src/extraction/local/prism.rs"]mod prism;
type Result<T>=std::result::Result<T,String>;
fn error(code:&str,message:&str)->String{format!("REFERENCE_{code}: {message}")}
#[path="../../src/extraction/local/compensated.rs"]mod compensated;
use compensated::W;
#[path="checkpoint.rs"]mod checkpoint;
pub use checkpoint::controls as checkpoint_controls;
pub fn ordinary_rule_data(order:usize)->Result<(&'static[f64],&'static[f64])>{let rule=prism::cached_rule(order)?;Ok((&rule.nodes,&rule.weights))}
type V=[W;3];
fn sub(a:V,b:V)->V{std::array::from_fn(|k|a[k]-b[k])}
fn dot(a:V,b:V)->W{(0..3).fold(W::ZERO,|s,k|s+a[k]*b[k])}
fn cross(a:V,b:V)->V{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
struct Face{vertices:[V;3],normal:V,edges:[(V,V,W);3]}
struct Edge{a:usize,b:usize,tangent:V,length:W,adjacent:[(usize,V);2]}
pub struct Source{vertices:[V;4],volume:W,faces:[Face;4],face_ids:[[usize;3];4],edges:Vec<Edge>,pub shape_condition:f64}
pub fn volume_audit(tetra:&[[[f64;3];4]])->Result<serde_json::Value>{let mut maximum=0_f64;let mut worst=0;let mut rows=vec![];
 for(i,&t)in tetra.iter().enumerate(){let v=t.map(|p|p.map(W::f));let exact=(dot(sub(v[1],v[0]),cross(sub(v[2],v[0]),sub(v[3],v[0]))).abs()/W::f(6.)).value();let native=mesh::tetra_coefficients(t)?.0;let relative=(exact-native).abs()/exact;if relative>maximum{maximum=relative;worst=i;}rows.push(serde_json::json!({"i":i,"nativeVolumeBits":native.to_bits(),"exactVolumeBits":exact.to_bits(),"relativeDifference":relative}));}
 Ok(serde_json::json!({"kind":"exact-vertex-volume-audit-v0","maximumRelativeDifference":maximum,"worstSource":worst,"gate1eMinus10":maximum<=1e-10,"physicalAdmission":false,"rows":rows}))
}
impl Source{
 pub fn new(tetra:[[f64;3];4])->Result<Self>{
  let(volume,gradient)=mesh::tetra_coefficients(tetra)?;let vertices=tetra.map(|p|p.map(W::f));
  let mut faces=Vec::with_capacity(4);
  for omitted in 0..4{let mut tri=Vec::with_capacity(3);for(i,&p)in vertices.iter().enumerate(){if i!=omitted{tri.push(p);}}
   let mut tri:[V;3]=tri.try_into().map_err(|_|error("SOURCE","three face vertices required"))?;let mut n=cross(sub(tri[1],tri[0]),sub(tri[2],tri[0]));
   if dot(n,sub(vertices[omitted],tri[0])).value()>0.{tri.swap(1,2);n=n.map(|v|-v);}
   let length=dot(n,n).sqrt();if !(length.value()>0.){return Err(error("SOURCE","positive face required"));}let normal=n.map(|v|v/length);
   let edges=std::array::from_fn(|i|{let d=sub(tri[(i+1)%3],tri[i]);let length=dot(d,d).sqrt();let tangent=d.map(|v|v/length);(tangent,cross(tangent,normal),length)});faces.push(Face{vertices:tri,normal,edges});
  }
  let exact=dot(sub(vertices[1],vertices[0]),cross(sub(vertices[2],vertices[0]),sub(vertices[3],vertices[0]))).abs()/W::f(6.);
  if((exact-W::f(volume))/exact).value().abs()>1e-10{return Err(error("SOURCE",&format!("reference/native original volume mismatch: exact={:e}, native={volume:e}, relative={:e}, tetra={tetra:?}",exact.value(),((exact-W::f(volume))/exact).value())));}
  let span=tetra.iter().flat_map(|p|tetra.iter().map(move|q|p.iter().zip(q).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt())).fold(0_f64,f64::max);
  let shape_condition=span*gradient.iter().flat_map(|r|r.iter()).map(|x|x.abs()).fold(0_f64,f64::max);
  let faces:[Face;4]=faces.try_into().map_err(|_|error("SOURCE","four faces required"))?;
  let face_ids=std::array::from_fn(|i|faces[i].vertices.map(|p|vertices.iter().position(|q|(0..3).all(|k|p[k].0.hi==q[k].0.hi&&p[k].0.lo==q[k].0.lo)).unwrap()));let mut edges=Vec::with_capacity(6);
  for a in 0..4{for b in a+1..4{let d=sub(vertices[b],vertices[a]);let length=dot(d,d).sqrt();let tangent=d.map(|x|x/length);let mut adjacent=vec![];for(f,ids)in face_ids.iter().enumerate(){for k in 0..3{if ids[k].min(ids[(k+1)%3])==a&&ids[k].max(ids[(k+1)%3])==b{adjacent.push((f,faces[f].edges[k].1));}}}edges.push(Edge{a,b,length,tangent,adjacent:adjacent.try_into().map_err(|_|error("SOURCE","two faces per reference edge required"))?});}}
  Ok(Self{vertices,volume:exact,faces,face_ids,edges,shape_condition})
 }
 fn potential_fast(&self,p:V)->Option<W>{
  if self.shape_condition>16.{return None;}let p64=p.map(W::value);let delta=(0..3).map(|k|(p[k]-W::f(p64[k])).value().powi(2)).sum::<f64>().sqrt();let vertices=self.vertices.map(|p|p.map(W::value));let radius=vertices.iter().map(|v|v.iter().zip(p64).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()).fold(0_f64,f64::max)+delta;
  let distance=(0..3).map(|k|{let lo=vertices.iter().map(|p|p[k]).fold(f64::INFINITY,f64::min);let hi=vertices.iter().map(|p|p[k]).fold(f64::NEG_INFINITY,f64::max);(lo-p64[k]).max(p64[k]-hi).max(0.).powi(2)}).sum::<f64>().sqrt();
  let mut representation=4.*std::f64::consts::PI*radius*radius*delta/self.volume.value();if distance>delta{representation=representation.min(radius*delta/(distance-delta).powi(2));}if representation>=1e-12{return None;}
  let dot=|a:[f64;3],b:[f64;3]|a.iter().zip(b).map(|(x,y)|x*y).sum::<f64>();let cross=|a:[f64;3],b:[f64;3]|[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];let(mut total,mut bound)=(0.,0.);
  for face in &self.faces{let r=face.vertices.map(|v|std::array::from_fn::<_,3,_>(|k|v[k].value()-p64[k]));let radius=r.map(|v|v[0].hypot(v[1]).hypot(v[2]));let h=dot(r[0],face.normal.map(W::value));let(mut edges,mut edge_bound)=(0.,0.);
   for(i,&(tangent,outward,length))in face.edges.iter().enumerate(){let j=(i+1)%3;let tangent=tangent.map(W::value);let t=dot(r[i],tangent);let length=length.value();let end=t+length;let s=dot(r[i],outward.map(W::value));let perpendicular=std::array::from_fn(|k|r[i][k]-t*tangent[k]);let rho2=dot(perpendicular,perpendicular);let da=radius[i]+t.abs();let db=radius[j]+end.abs();let gap=(if da>0.{rho2/da}else{0.})+(if db>0.{rho2/db}else{0.})+if t>0.{2.*t}else if end<0.{-2.*end}else{0.};if gap==0.&&rho2==0.&&s==0.{continue;}if !(gap>0.){return None;}let term=s*(2.*length/gap).ln_1p();edges+=term;edge_bound+=term.abs();}
   let omega=2.*dot(r[0],cross(r[1],r[2])).atan2(radius[0]*radius[1]*radius[2]+radius[0]*dot(r[1],r[2])+radius[1]*dot(r[0],r[2])+radius[2]*dot(r[0],r[1]));total+=h*(edges-h*omega);bound+=h.abs()*(edge_bound+(h*omega).abs());
  }
  let value=total*0.5;if value>0.&&value.is_finite()&&512.*f64::EPSILON*bound/total+representation<=1e-12{Some(W::f(value))}else{None}
 }
 fn potential(&self,p:V)->Result<W>{if let Some(value)=self.potential_fast(p){return Ok(value);}self.potential_precise(p)}
 fn potential_precise(&self,p:V)->Result<W>{
  let r=self.vertices.map(|v|sub(v,p));let radius=r.map(|v|dot(v,v).sqrt());let h=std::array::from_fn::<_,4,_>(|i|dot(r[self.face_ids[i][0]],self.faces[i].normal));let(mut total,mut bound)=(W::ZERO,W::ZERO);
  for e in &self.edges{let t=dot(r[e.a],e.tangent);let end=t+e.length;let perpendicular=sub(r[e.a],e.tangent.map(|v|v*t));let rho2=dot(perpendicular,perpendicular);let da=radius[e.a]+t.abs();let db=radius[e.b]+end.abs();let gap=(if da.value()>0.{rho2/da}else{W::ZERO})+(if db.value()>0.{rho2/db}else{W::ZERO})+if t.value()>0.{t.scale(2.)}else if end.value()<0.{end.scale(-2.)}else{W::ZERO};
   if gap.value()==0.&&rho2.value()==0.&&e.adjacent.iter().all(|&(i,_)|h[i].value()==0.){continue;}if !(gap.value()>0.){return Err(error("POINT","unresolved shared reference source edge"));}let log=(W::ONE+e.length.scale(2.)/gap).ln();let(mut q,mut absolute)=(W::ZERO,W::ZERO);for &(i,outward)in &e.adjacent{let term=h[i]*dot(r[e.a],outward);q=q+term;absolute=absolute+term.abs();}total=total+q*log;bound=bound+absolute*log.abs();
  }
  for i in 0..4{let v=self.face_ids[i].map(|j|r[j]);let d=self.face_ids[i].map(|j|radius[j]);let omega=dot(v[0],cross(v[1],v[2])).atan2(d[0]*d[1]*d[2]+d[0]*dot(v[1],v[2])+d[1]*dot(v[0],v[2])+d[2]*dot(v[0],v[1])).scale(2.);let term=h[i]*h[i]*omega;total=total-term;bound=bound+term.abs();}
  let value=total.scale(0.5);if !(value.value()>0.)||!value.value().is_finite()||1024.*f64::EPSILON*f64::EPSILON*(bound/total).value()>1e-12{return Err(error("POINT","positive shared source or1e-12 roundoff gate failed"));}Ok(value)
 }
 fn potential_precise_faces(&self,p:V)->Result<W>{
  let(mut total,mut bound)=(W::ZERO,W::ZERO);
  for face in &self.faces{let r=face.vertices.map(|v|sub(v,p));let radius=r.map(|v|dot(v,v).sqrt());let h=dot(r[0],face.normal);let mut edge_sum=W::ZERO;let mut edge_bound=W::ZERO;
   for(i,&(tangent,outward,length))in face.edges.iter().enumerate(){let j=(i+1)%3;let t=dot(r[i],tangent);let end=t+length;let s=dot(r[i],outward);let perpendicular=sub(r[i],tangent.map(|v|v*t));let rho2=dot(perpendicular,perpendicular);
    let da=radius[i]+t.abs();let db=radius[j]+end.abs();let gap=(if da.value()>0.{rho2/da}else{W::ZERO})+(if db.value()>0.{rho2/db}else{W::ZERO})+if t.value()>0.{t.scale(2.)}else if end.value()<0.{end.scale(-2.)}else{W::ZERO};
    if gap.value()==0.&&rho2.value()==0.&&s.value()==0.{continue;}if !(gap.value()>0.){return Err(error("POINT","unresolved original source edge"));}
    let term=s*(W::ONE+length.scale(2.)/gap).ln();edge_sum=edge_sum+term;edge_bound=edge_bound+term.abs();
   }
   let omega=dot(r[0],cross(r[1],r[2])).atan2(radius[0]*radius[1]*radius[2]+radius[0]*dot(r[1],r[2])+radius[1]*dot(r[0],r[2])+radius[2]*dot(r[0],r[1])).scale(2.);
   total=total+h*(edge_sum-h*omega);bound=bound+h.abs()*(edge_bound+(h*omega).abs());
  }
  let value=total.scale(0.5);if !(value.value()>0.)||!value.value().is_finite()||1024.*f64::EPSILON*f64::EPSILON*(bound/total).value()>1e-12{return Err(error("POINT","positive source or1e-12 compensated roundoff gate failed"));}Ok(value)
 }
 fn integrate_target(&self,vertices:[V;4],volume:W,order:usize,graded:bool,mut check:impl FnMut()->Result<()>)->Result<W>{
  let rule=compensated::rule(order)?;let mut parameters=Vec::with_capacity(order*2);
  for(&x,&w)in rule.nodes.iter().zip(&rule.weights){if graded&&order>=8{let x2=x*x;let h=(x2*x2).scale(0.5);let jac=(x2*x).scale(2.);parameters.push((h,w*jac));parameters.push((W::ONE-h,w*jac));}else{parameters.push((x,w));}}
  for k in 0..=3{let mut moment=W::ZERO;for&(x,w)in &parameters{let mut p=W::ONE;for _ in 0..k{p=p*x;}moment=moment+w*p;}if(moment-W::ONE/W::f((k+1)as f64)).value().abs()>1e-28{return Err(error("QUADRATURE","positive target moments0..3 fail1e-28"));}}
  let edge=[sub(vertices[1],vertices[0]),sub(vertices[2],vertices[1]),sub(vertices[3],vertices[2])];let mut sum=W::ZERO;let mut samples=0usize;
  for&(u,wu)in &parameters{for&(v,wv)in &parameters{for&(w,ww)in &parameters{samples+=1;if samples%32==0{check()?;}let uv=u*v;let uvw=uv*w;let p=std::array::from_fn(|k|vertices[0][k]+u*edge[0][k]+uv*edge[1][k]+uvw*edge[2][k]);let weight=volume.scale(6.)*u*u*v*wu*wv*ww;if !(weight.value()>0.){return Err(error("QUADRATURE","positive target weight required"));}sum=sum+self.potential(p)?*weight;}}}
  let value=sum.value();if value>0.&&value.is_finite(){Ok(sum)}else{Err(error("QUADRATURE","positive finite volume pair required"))}
 }
 fn contact_order(&self,target:&Self)->[V;4]{let mut indices=[0,1,2,3];indices.sort_by_key(|&i|!self.vertices.iter().any(|p|(0..3).all(|k|p[k].0.hi==target.vertices[i][k].0.hi&&p[k].0.lo==target.vertices[i][k].0.lo)));indices.map(|i|target.vertices[i])}
 pub fn integral(&self,target:&Self,order:usize,check:impl FnMut()->Result<()>)->Result<f64>{Ok(self.integrate_target(self.contact_order(target),target.volume,order,true,check)?.value())}
 pub fn adaptive(&self,target:&Self,tolerance:f64,mut check:impl FnMut()->Result<()>)->Result<(f64,usize,f64)>{
  struct Leaf{vertices:[V;4],volume:W,value:W,error:f64}
  let mut evaluate=|vertices:[V;4],volume:W|->Result<Leaf>{check()?;let exact=dot(sub(vertices[1],vertices[0]),cross(sub(vertices[2],vertices[0]),sub(vertices[3],vertices[0]))).abs()/W::f(6.);if((exact-volume)/volume).value().abs()>1e-12{return Err(error("PARTITION","adaptive target loses original geometric volume"));}let low=self.integrate_target(vertices,volume,8,false,&mut check)?;let high=self.integrate_target(vertices,volume,16,false,&mut check)?;Ok(Leaf{vertices,volume,value:high,error:(high-low).value().abs()})};
  let mut leaves=vec![evaluate(self.contact_order(target),target.volume)?];
  loop{let value=leaves.iter().fold(W::ZERO,|s,p|s+p.value).value();let estimate=leaves.iter().map(|p|p.error).sum::<f64>();if estimate<=tolerance{return Ok((value,leaves.len(),estimate));}if leaves.len()>=128&&leaves.len().is_power_of_two(){eprintln!("reference adaptive leaves{}: estimated/tolerance={:e}",leaves.len(),estimate/tolerance);}if leaves.len()>=4096{return Err(error("QUADRATURE",&format!("adaptive4096-leaf estimate exhausted: error={estimate:e}, tolerance={tolerance:e}")));}
   let at=(0..leaves.len()).max_by(|&a,&b|leaves[a].error.total_cmp(&leaves[b].error)).unwrap();let old=leaves.swap_remove(at);let edge=|i:usize,j:usize|dot(sub(old.vertices[i],old.vertices[j]),sub(old.vertices[i],old.vertices[j])).value();let(a,b)=(0..4).flat_map(|a|(a+1..4).map(move|b|(a,b))).max_by(|&(a,b),&(c,d)|edge(a,b).total_cmp(&edge(c,d))).unwrap();let midpoint=std::array::from_fn(|k|(old.vertices[a][k]+old.vertices[b][k]).scale(0.5));let mut left=old.vertices;left[a]=midpoint;let mut right=old.vertices;right[b]=midpoint;leaves.push(evaluate(left,old.volume.scale(0.5))?);leaves.push(evaluate(right,old.volume.scale(0.5))?);
  }
 }
}
fn positive_interior_reference(tetra:[[f64;3];4],point:[f64;3])->Result<(f64,f64,usize)>{
 struct Leaf{vertices:[[f64;3];3],value:f64,error:f64,volume:f64}
 let started=std::time::Instant::now();let low=prism::GaussRule::new(8)?;let high=prism::GaussRule::new(16)?;
 let evaluate=|vertices:[[f64;3];3]|->Result<Leaf>{
  if started.elapsed().as_secs_f64()>100.{return Err(error("DEADLINE","independent positive interior source control"));}
  let volume=mesh::tetra_coefficients([point,vertices[0],vertices[1],vertices[2]])?.0;
  let r=vertices.map(|p|std::array::from_fn::<_,3,_>(|k|p[k]-point[k]));
  let integrate=|rule:&prism::GaussRule|{let mut sum=0.;for(&u,&wu)in rule.nodes.iter().zip(&rule.weights){for(&v,&wv)in rule.nodes.iter().zip(&rule.weights){let direction=std::array::from_fn::<_,3,_>(|k|(1.-u)*r[0][k]+u*(1.-v)*r[1][k]+u*v*r[2][k]);sum+=3.*volume*wu*wv*u/direction[0].hypot(direction[1]).hypot(direction[2]);}}sum};
  let a=integrate(&low);let b=integrate(&high);if !(a>0.&&b>0.)||!b.is_finite(){return Err(error("CONTROL","positive source cone integral required"));}Ok(Leaf{vertices,value:b,error:(b-a).abs(),volume})
 };
 let mut leaves=vec![];for skip in 0..4{let face=tetra.iter().enumerate().filter_map(|(i,p)|if i==skip{None}else{Some(*p)}).collect::<Vec<_>>();leaves.push(evaluate([face[0],face[1],face[2]])?);}
 let expected=mesh::tetra_coefficients(tetra)?.0;let actual=leaves.iter().map(|p|p.volume).sum::<f64>();if(actual-expected).abs()>1e-10*expected{return Err(error("CONTROL","positive interior source fan loses original volume"));}
 loop{let value=leaves.iter().map(|p|p.value).sum::<f64>();let estimate=leaves.iter().map(|p|p.error).sum::<f64>();if estimate<=1e-9*value{return Ok((value,estimate/value,leaves.len()));}if leaves.len()>=4096{return Err(error("CONTROL","positive source4096-leaf estimate exhausted"));}
  let at=(0..leaves.len()).max_by(|&a,&b|leaves[a].error.total_cmp(&leaves[b].error)).unwrap();let old=leaves.swap_remove(at);
  let(a,b)=[(0,1),(1,2),(2,0)].into_iter().max_by(|&(a,b),&(c,d)|{let edge=|i:usize,j:usize|old.vertices[i].iter().zip(old.vertices[j]).map(|(x,y)|(x-y).powi(2)).sum::<f64>();edge(a,b).total_cmp(&edge(c,d))}).unwrap();let midpoint=std::array::from_fn(|k|old.vertices[a][k]+(old.vertices[b][k]-old.vertices[a][k])/2.);let mut left=old.vertices;left[a]=midpoint;let mut right=old.vertices;right[b]=midpoint;let a=evaluate(left)?;let b=evaluate(right)?;if(a.volume+b.volume-old.volume).abs()>1e-10*old.volume{return Err(error("CONTROL","source face bisection loses original cone volume"));}leaves.push(a);leaves.push(b);
 }
}
pub fn controls(actual:&[[[f64;3];4]])->Result<serde_json::Value>{
 let t=[[0.,0.,0.],[0.001,0.,0.],[0.0004,0.0003,0.],[0.0004,0.0003,35e-6]];let source=Source::new(t)?;let mut worst=0_f64;
 for p in [[-0.0003,0.0002,0.0002],[0.002,0.001,0.001],[0.00045,0.0002,5e-6]]{let value=source.potential(p.map(W::f))?.value();let low=polyhedron::positive_tetra_potential_duffy(t,p,31)?;let high=polyhedron::positive_tetra_potential_duffy(t,p,64)?;let raw_refinement=(high-low).abs()/high;
  let(other,refinement,leaves)=if raw_refinement>1e-6&&source.faces.iter().all(|f|dot(sub(f.vertices[0],p.map(W::f)),f.normal).value()>0.){positive_interior_reference(t,p)?}else{(high,raw_refinement,0)};
  let discrepancy=(value-other).abs()/other;eprintln!("independent source point{p:?}: analytic={value:e}, positive31={low:e}, positive64={high:e}, acceptedPositive={other:e}, discrepancy={discrepancy:e}, sourceRefinement={refinement:e}, adaptiveLeaves={leaves}");worst=worst.max(discrepancy);if discrepancy>1e-6||refinement>1e-6{return Err(error("CONTROL",&format!("independent positive source point difference{discrepancy:e}, refinement{refinement:e}")));}}
 let mut fast_cases=0;let mut fast_worst=0_f64;let mut shared_faces_worst=0_f64;
 for &t in actual{let source=Source::new(t)?;let center=std::array::from_fn::<_,3,_>(|k|t.iter().map(|p|p[k]/4.).sum::<f64>());let span=t.iter().map(|p|p.iter().zip(center).map(|(x,y)|(x-y).powi(2)).sum::<f64>().sqrt()).fold(0_f64,f64::max);let mut offsets=vec![[2.,1.,3.],[-0.5,0.5,1.]];if source.shape_condition<=16.{offsets.push([0.,0.,0.]);}
  for offset in offsets{let p=std::array::from_fn(|k|center[k]+span*offset[k]);let value=source.potential(p.map(W::f))?.value();let shared=source.potential_precise(p.map(W::f))?.value();let faces=source.potential_precise_faces(p.map(W::f))?.value();let shared_error=(shared-faces).abs()/shared;shared_faces_worst=shared_faces_worst.max(shared_error);if shared_error>1e-12{return Err(error("CONTROL","shared/face point potential disagreement"));}if let Some(fast)=source.potential_fast(p.map(W::f)){fast_cases+=1;let exact=source.potential_precise(p.map(W::f))?.value();let discrepancy=(fast.value()-exact).abs()/exact;fast_worst=fast_worst.max(discrepancy);if discrepancy>1e-12{return Err(error("CONTROL","fast/compensated source point differs beyond1e-12"));}}let low=polyhedron::positive_tetra_potential_duffy(t,p,31)?;let high=polyhedron::positive_tetra_potential_duffy(t,p,64)?;let raw=(high-low).abs()/high;let(other,refinement,_)=if raw>1e-6&&source.faces.iter().all(|f|dot(sub(f.vertices[0],p.map(W::f)),f.normal).value()>0.){positive_interior_reference(t,p)?}else{(high,raw,0)};let discrepancy=(value-other).abs()/other;worst=worst.max(discrepancy);if discrepancy>1e-6||refinement>1e-6{return Err(error("CONTROL",&format!("actual finite source point difference{discrepancy:e}, source refinement{refinement:e}, shape={:e}",source.shape_condition)));}}
 }
 if fast_cases==0{return Err(error("CONTROL","fast source control must exercise its admitted branch"));}
 Ok(serde_json::json!({"method":"reference-only compensated finite point1/r and positive target-volume transform","maximumIndependentSourcePointRelativeError":worst,"fastPointCases":fast_cases,"maximumFastCompensatedDiscrepancy":fast_worst,"maximumSharedFaceDiscrepancy":shared_faces_worst,"physicalAdmission":false}))
}
