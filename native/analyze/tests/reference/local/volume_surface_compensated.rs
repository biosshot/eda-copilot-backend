//! The same boundary identity with matched projected target panels and two-F64
//! arithmetic. Original tetrahedra/curves/current coefficients are unchanged.
use super::{compensated::{self,W},error,Result};
type V=[W;3];type P=[W;2];
fn sub(a:V,b:V)->V{std::array::from_fn(|k|a[k]-b[k])}
fn dot(a:V,b:V)->W{(0..3).fold(W::ZERO,|s,k|s+a[k]*b[k])}
fn cross(a:V,b:V)->V{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn equal(a:P,b:P)->bool{(0..2).all(|k|a[k].0.hi==b[k].0.hi&&a[k].0.lo==b[k].0.lo)}
fn orient(a:P,b:P,c:P)->W{if equal(a,b)||equal(a,c)||equal(b,c){W::ZERO}else{(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])}}
fn inside(p:P,t:[P;3])->bool{let a=orient(t[0],t[1],t[2]).value();if a==0.{return false;}if t.iter().any(|&v|equal(p,v)){return true;}let s=std::array::from_fn::<_,3,_>(|i|orient(t[i],t[(i+1)%3],p).value());if a>0.{s.iter().all(|x|*x>=0.)}else{s.iter().all(|x|*x<=0.)}}
fn append(poly:&mut Vec<P>,p:P){if poly.last().is_none_or(|&q|!equal(p,q)){poly.push(p);}}
fn split(poly:&[P],line:[W;3])->Result<Option<(Vec<P>,Vec<P>)>>{
 let distance=poly.iter().map(|p|line[0]+p[0]*(line[1]-line[0])+p[1]*(line[2]-line[0])).collect::<Vec<_>>();if !distance.iter().any(|x|x.value()>0.)||!distance.iter().any(|x|x.value()<0.){return Ok(None);}
 let(mut positive,mut negative)=(Vec::with_capacity(poly.len()+2),Vec::with_capacity(poly.len()+2));
 for i in 0..poly.len(){let j=(i+1)%poly.len();let x=distance[i].value();let y=distance[j].value();if x>=0.{append(&mut positive,poly[i]);}if x<=0.{append(&mut negative,poly[i]);}if(x>0.&&y<0.)||(x<0.&&y>0.){let t=distance[i]/(distance[i]-distance[j]);let p=std::array::from_fn(|k|poly[i][k]+t*(poly[j][k]-poly[i][k]));append(&mut positive,p);append(&mut negative,p);}}
 for p in [&mut positive,&mut negative]{if p.len()>1&&equal(p[0],p[p.len()-1]){p.pop();}if p.len()>12{return Err(error("SURFACE_RANGE","bounded convex source-edge split failed"));}}
 Ok(Some((positive,negative)))
}
#[derive(Clone,Copy)]
struct Face{v:[V;3],c:V,n:V,det:W,edges:[(V,V,W);3]}
impl Face{
 fn new(mut v:[V;3],opposite:V)->Result<Self>{
  let mut c=cross(sub(v[1],v[0]),sub(v[2],v[0]));if dot(c,sub(opposite,v[0])).value()>0.{v.swap(1,2);c=c.map(|x|-x);}
  let det=dot(c,c).sqrt();if !(det.value()>0.)||!det.value().is_finite(){return Err(error("SURFACE_RANGE","compensated face is unresolved"));}let n=c.map(|x|x/det);
  let edges=std::array::from_fn(|i|{let e=sub(v[(i+1)%3],v[i]);let length=dot(e,e).sqrt();let tangent=e.map(|x|x/length);(tangent,cross(tangent,n),length)});Ok(Self{v,c,n,det,edges})
 }
 #[cfg(test)]fn distance(&self,p:V)->Result<(W,W)>{
  let r=self.v.map(|v|sub(v,p));let radius=r.map(|v|dot(v,v).sqrt());let h=dot(r[0],self.n);
  let omega=dot(r[0],cross(r[1],r[2])).atan2(radius[0]*radius[1]*radius[2]+radius[0]*dot(r[1],r[2])+radius[1]*dot(r[0],r[2])+radius[2]*dot(r[0],r[1])).scale(2.);
  let(mut el,mut ej,mut al,mut aj)=(W::ZERO,W::ZERO,W::ZERO,W::ZERO);
  for(i,&(tangent,outward,length))in self.edges.iter().enumerate(){let j=(i+1)%3;let t=dot(r[i],tangent);let end=t+length;let s=dot(r[i],outward);let perp=std::array::from_fn(|k|r[i][k]-t*tangent[k]);let a2=dot(perp,perp);
   let da=radius[i]+t.abs();let db=radius[j]+end.abs();
   let excess=if t.value()>=0.{t.scale(2.)}else if end.value()<=0.{end.scale(-2.)}else{W::ZERO};
   let gap=(if da.value()>0.{a2/da}else{W::ZERO})+(if db.value()>0.{a2/db}else{W::ZERO})+excess;
   if gap.value()==0.&&a2.value()==0.&&s.value()==0.{continue;}if !(gap.value()>0.){return Err(error("SURFACE_RANGE","compensated edge limit unresolved"));}
   let logarithm=(W::ONE+length.scale(2.)/gap).ln();let integral=(radius[j]+t*(t.scale(2.)+length)/(radius[i]+radius[j]))*length.scale(0.5)+a2*logarithm.scale(0.5);
   let l=s*logarithm;let j=s*integral;el=el+l;ej=ej+j;al=al+l.abs();aj=aj+j.abs();
  }
  let value=(ej+(el-h*omega)*h*h)/W::f(3.);let absolute=(aj+(al+(h*omega).abs())*h*h)/W::f(3.);
  if value.value()>0.&&value.value().is_finite(){Ok((value,absolute))}else{Err(error("SURFACE_RANGE","nonpositive compensated source-face distance"))}
 }
}
struct Panel{p:[P;3],det:W,faces:[usize;2]}
struct Edge{a:usize,b:usize,tangent:V,length:W,faces:[(usize,V);2]}
pub(super) struct Source{vertices:[V;4],faces:[Face;4],face_vertices:[[usize;3];4],edges:[Edge;6],axis:usize,ratios:[V;4],planes:[[W;3];4],panels:Vec<Panel>}
impl Source{
 pub fn new(tetra:[[f64;3];4])->Result<Self>{
  let t=tetra.map(|p|p.map(W::f));let mut faces=Vec::with_capacity(4);
  for opposite in 0..4{let mut v=[[W::ZERO;3];3];let mut j=0;for(i,&p)in t.iter().enumerate(){if i!=opposite{v[j]=p;j+=1;}}faces.push(Face::new(v,t[opposite])?);}
  let faces:[Face;4]=faces.try_into().map_err(|_|error("SURFACE_RANGE","four faces required"))?;
  let face_vertices=faces.map(|f|f.v.map(|v|t.iter().position(|p|(0..3).all(|k|p[k].value()==v[k].value())).unwrap()));
  let mut edges=Vec::with_capacity(6);
  for a in 0..4{for b in a+1..4{let edge=sub(t[b],t[a]);let length=dot(edge,edge).sqrt();let tangent=edge.map(|x|x/length);let mut adjacent=Vec::with_capacity(2);
   for(i,f)in faces.iter().enumerate(){let ids=face_vertices[i];for j in 0..3{if ids[j].min(ids[(j+1)%3])==a&&ids[j].max(ids[(j+1)%3])==b{adjacent.push((i,f.edges[j].1));}}}
   let adjacent:[(usize,V);2]=adjacent.try_into().map_err(|_|error("SURFACE_RANGE","two source faces per edge required"))?;edges.push(Edge{a,b,tangent,length,faces:adjacent});
  }}let edges:[Edge;6]=edges.try_into().map_err(|_|error("SURFACE_RANGE","six original source edges required"))?;
  let largest=(0..4).max_by(|&a,&b|faces[a].det.value().total_cmp(&faces[b].det.value())).unwrap();let axis=(0..3).max_by(|&a,&b|faces[largest].n[a].value().abs().total_cmp(&faces[largest].n[b].value().abs())).unwrap();let u=(axis+1)%3;let v=(axis+2)%3;
  let inverse=std::array::from_fn::<_,4,_>(|i|if faces[i].c[axis].value()!=0.{W::ONE/faces[i].c[axis]}else{W::ZERO});let ratios=std::array::from_fn(|i|faces[i].c.map(|x|x*inverse[i].abs()));let planes=std::array::from_fn(|i|[faces[i].v[0][axis],-faces[i].c[u]*inverse[i],-faces[i].c[v]*inverse[i]]);
  let p=t.map(|p|[p[u],p[v]]);let mut triangles=Vec::<[P;3]>::new();
  for i in 0..4{let other=(0..4).filter(|&j|j!=i).collect::<Vec<_>>();let tri=std::array::from_fn(|k|p[other[k]]);if inside(p[i],tri){for k in 0..3{let panel=[p[i],tri[k],tri[(k+1)%3]];if orient(panel[0],panel[1],panel[2]).value()!=0.{triangles.push(panel);}}break;}}
  if triangles.is_empty(){
   for(a,b,c,d)in[(0,1,2,3),(0,2,1,3),(0,3,1,2)]{
    let opposite=|a:f64,b:f64|(a>0.&&b<0.)||(a<0.&&b>0.);
    if opposite(orient(p[a],p[b],p[c]).value(),orient(p[a],p[b],p[d]).value())&&opposite(orient(p[c],p[d],p[a]).value(),orient(p[c],p[d],p[b]).value()){
     let ab=[p[b][0]-p[a][0],p[b][1]-p[a][1]];let cd=[p[d][0]-p[c][0],p[d][1]-p[c][1]];let ac=[p[c][0]-p[a][0],p[c][1]-p[a][1]];let fraction=(ac[0]*cd[1]-ac[1]*cd[0])/(ab[0]*cd[1]-ab[1]*cd[0]);let center=[p[a][0]+fraction*ab[0],p[a][1]+fraction*ab[1]];
     for i in 0..4{for j in i+1..4{if(i==a.min(b)&&j==a.max(b))||(i==c.min(d)&&j==c.max(d)){continue;}triangles.push([center,p[i],p[j]]);}}break;
    }
   }
  }
  if triangles.is_empty(){return Err(error("SURFACE_RANGE","projected tetra partition unresolved"));}
  let mut panels=Vec::with_capacity(triangles.len());let mut area=W::ZERO;
  for tri in triangles{let center=std::array::from_fn(|k|(tri[0][k]+tri[1][k]+tri[2][k])/W::f(3.));let covering=(0..4).filter(|&i|faces[i].c[axis].value()!=0.&&inside(center,faces[i].v.map(|x|[x[u],x[v]]))).collect::<Vec<_>>();
   if covering.len()!=2||faces[covering[0]].c[axis].value().is_sign_positive()==faces[covering[1]].c[axis].value().is_sign_positive(){return Err(error("SURFACE_RANGE","projected panel must have two opposite boundary traces"));}
   let det=orient(tri[0],tri[1],tri[2]).abs();area=area+det;panels.push(Panel{p:tri,det,faces:[covering[0],covering[1]]});
  }
  let expected=faces.iter().fold(W::ZERO,|s,f|s+f.c[axis].abs());if((area.scale(2.)-expected)/expected).value().abs()>1e-28{return Err(error("SURFACE_RANGE","projected common panels lose boundary coverage"));}
  let mut projected_volume=W::ZERO;
  for i in 0..4{let covered=panels.iter().filter(|p|p.faces.contains(&i)).fold(W::ZERO,|s,p|s+p.det);if(covered-faces[i].c[axis].abs()).value().abs()>1e-28*expected.value(){return Err(error("SURFACE_RANGE","individual projected face coverage failed"));}}
  for panel in &panels{let center=std::array::from_fn::<_,2,_>(|k|(panel.p[0][k]+panel.p[1][k]+panel.p[2][k])/W::f(3.));let mut height=W::ZERO;for &i in &panel.faces{let f=&faces[i];let plane=planes[i];let value=plane[0]+plane[1]*(center[0]-f.v[0][u])+plane[2]*(center[1]-f.v[0][v]);height=height+value.scale(if f.c[axis].value()>0.{1.}else{-1.});}projected_volume=projected_volume+height*panel.det.scale(0.5);}
  let exact_volume=dot(sub(t[1],t[0]),cross(sub(t[2],t[0]),sub(t[3],t[0]))).abs()/W::f(6.);if !exact_volume.value().is_finite()||!(exact_volume.value()>0.)||((projected_volume-exact_volume)/exact_volume).value().abs()>1e-12{return Err(error("SURFACE_RANGE","matched projected tetra volume failed1e-12"));}
  let mut surface=[W::ZERO;3];let mut moment=[[W::ZERO;3];3];let mut surface_scale=W::ZERO;
  for panel in &panels{let xy=std::array::from_fn::<_,2,_>(|k|(panel.p[0][k]+panel.p[1][k]+panel.p[2][k])/W::f(3.));for &i in &panel.faces{let f=&faces[i];let mut p=[W::ZERO;3];p[u]=xy[0];p[v]=xy[1];let plane=planes[i];p[axis]=plane[0]+plane[1]*(xy[0]-f.v[0][u])+plane[2]*(xy[1]-f.v[0][v]);let vector=ratios[i].map(|x|x*panel.det.scale(0.5));for k in 0..3{surface[k]=surface[k]+vector[k];surface_scale=surface_scale+vector[k].abs();for l in 0..3{moment[k][l]=moment[k][l]+vector[k]*p[l];}}}}
  for f in &faces{if f.c[axis].value()!=0.{continue;}let p=std::array::from_fn::<_,3,_>(|k|(f.v[0][k]+f.v[1][k]+f.v[2][k])/W::f(3.));for k in 0..3{let vector=f.c[k].scale(0.5);surface[k]=surface[k]+vector;surface_scale=surface_scale+vector.abs();for l in 0..3{moment[k][l]=moment[k][l]+vector*p[l];}}}
  for k in 0..3{if surface[k].value().abs()>1e-28*surface_scale.value(){return Err(error("SURFACE_RANGE","matched closed-surface vector area failed"));}for l in 0..3{let expected=if k==l{exact_volume}else{W::ZERO};if((moment[k][l]-expected)/exact_volume).value().abs()>1e-12{return Err(error("SURFACE_RANGE","matched surface first moments failed1e-12"));}}}
  Ok(Self{vertices:t,faces,face_vertices,edges,axis,ratios,planes,panels})
 }
 fn flux(&self,p:V)->Result<(V,V)>{
  let r=self.vertices.map(|v|sub(v,p));let radius=r.map(|v|dot(v,v).sqrt());let h=std::array::from_fn::<_,4,_>(|i|dot(r[self.face_vertices[i][0]],self.faces[i].n));let(mut sum,mut absolute)=([W::ZERO;3],[W::ZERO;3]);
  for e in &self.edges{let t=dot(r[e.a],e.tangent);let end=t+e.length;let perp=std::array::from_fn(|k|r[e.a][k]-t*e.tangent[k]);let a2=dot(perp,perp);let da=radius[e.a]+t.abs();let db=radius[e.b]+end.abs();let excess=if t.value()>=0.{t.scale(2.)}else if end.value()<=0.{end.scale(-2.)}else{W::ZERO};let gap=(if da.value()>0.{a2/da}else{W::ZERO})+(if db.value()>0.{a2/db}else{W::ZERO})+excess;
   if gap.value()==0.&&a2.value()==0.{continue;}if !(gap.value()>0.){return Err(error("SURFACE_RANGE","shared compensated edge limit unresolved"));}
   let logarithm=(W::ONE+e.length.scale(2.)/gap).ln();let integral=(radius[e.b]+t*(t.scale(2.)+e.length)/(radius[e.a]+radius[e.b]))*e.length.scale(0.5)+a2*logarithm.scale(0.5);
   for &(i,outward)in &e.faces{let s=dot(r[e.a],outward);let term=s*(integral+h[i]*h[i]*logarithm);let bound=s.abs()*(integral.abs()+h[i]*h[i]*logarithm.abs());for k in 0..3{sum[k]=sum[k]+self.faces[i].n[k]*term;absolute[k]=absolute[k]+self.faces[i].n[k].abs()*bound;}}
  }
  for(i,f)in self.faces.iter().enumerate(){let v=self.face_vertices[i].map(|j|r[j]);let d=self.face_vertices[i].map(|j|radius[j]);let omega=dot(v[0],cross(v[1],v[2])).atan2(d[0]*d[1]*d[2]+d[0]*dot(v[1],v[2])+d[1]*dot(v[0],v[2])+d[2]*dot(v[0],v[1])).scale(2.);let term=h[i]*h[i]*h[i]*omega;for k in 0..3{sum[k]=sum[k]-f.n[k]*term;absolute[k]=absolute[k]+f.n[k].abs()*term.abs();}}
  Ok((sum.map(|x|x/W::f(3.)),absolute.map(|x|x/W::f(3.))))
 }
 fn aligned_panels(&self,target:&Self)->Result<Vec<Panel>>{
  let u=(target.axis+1)%3;let v=(target.axis+2)%3;let mut result=Vec::new();
  for parent in &target.panels{let mut area=W::ZERO;let mut moment=[W::ZERO;2];let mut polygons=vec![vec![[W::ZERO,W::ZERO],[W::ONE,W::ZERO],[W::ZERO,W::ONE]]];
   for edge in &self.edges{let a=[self.vertices[edge.a][u],self.vertices[edge.a][v]];let b=[self.vertices[edge.b][u],self.vertices[edge.b][v]];if equal(a,b){continue;}let line=parent.p.map(|p|orient(a,b,p));let mut next=Vec::new();for polygon in polygons{if let Some((a,b))=split(&polygon,line)?{next.push(a);next.push(b);}else{next.push(polygon);}if next.len()>64{return Err(error("MEMORY_LIMIT","source-edge quadrature polygon workspace exceeds64"));}}polygons=next;}
   for polygon in polygons{for i in 1..polygon.len().saturating_sub(1){let local=[polygon[0],polygon[i],polygon[i+1]];let jac=orient(local[0],local[1],local[2]).abs();let det=jac*parent.det;if det.value()==0.{continue;}area=area+jac;for k in 0..2{moment[k]=moment[k]+jac*(local[0][k]+local[1][k]+local[2][k])/W::f(3.);}let p=local.map(|p|std::array::from_fn(|k|parent.p[0][k]+p[0]*(parent.p[1][k]-parent.p[0][k])+p[1]*(parent.p[2][k]-parent.p[0][k])));if result.len()>=256{return Err(error("MEMORY_LIMIT","source-edge quadrature exceeds256 panels"));}result.push(Panel{p,det,faces:parent.faces});}}
   // Area and first moments commute with the original affine panel map.
   // Audit in unit barycentric coordinates before any ill-conditioned inverse.
   if(area-W::ONE).value().abs()>1e-28{return Err(error("SURFACE_RANGE","source-edge subdivision lost target projected area"));}
   for k in 0..2{if(moment[k]-W::ONE/W::f(3.)).value().abs()>1e-28{return Err(error("SURFACE_RANGE","source-edge subdivision lost target first moment"));}}
  }Ok(result)
 }
 pub fn integral(&self,target:&Self,order:usize,mut check:impl FnMut()->Result<()>)->Result<(f64,f64)>{
  let rule=compensated::rule(order)?;let mut parameters=Vec::with_capacity(2*order);
  for(&x,&w)in rule.nodes.iter().zip(&rule.weights){if order>=8{let x2=x*x;let h=(x2*x2).scale(0.5);let jac=(x2*x).scale(2.);parameters.push((h,w*jac));parameters.push((W::ONE-h,w*jac));}else{parameters.push((x,w));}}
  for k in 0..=2{let mut sum=W::ZERO;for&(x,w)in &parameters{let mut power=W::ONE;for _ in 0..k{power=power*x;}sum=sum+w*power;}if(sum-W::ONE/W::f((k+1)as f64)).value().abs()>1e-28{return Err(error("COMPENSATED_QUADRATURE","endpoint mapping lost positive moments"));}}
  let(mut total,mut absolute)=(W::ZERO,W::ZERO);let mut samples=0usize;let axis=target.axis;let u=(axis+1)%3;let v=(axis+2)%3;
  let aligned=self.aligned_panels(target)?;let panels=&aligned;
  for panel in panels{let du=std::array::from_fn::<_,2,_>(|k|panel.p[1][k]-panel.p[0][k]);let dv=std::array::from_fn::<_,2,_>(|k|panel.p[2][k]-panel.p[1][k]);for&(a,wa)in &parameters{for&(b,wb)in &parameters{
   samples+=1;if samples%32==0{check()?;}let ab=a*b;let xy=std::array::from_fn::<_,2,_>(|k|panel.p[0][k]+a*du[k]+ab*dv[k]);let(mut value,mut bound)=(W::ZERO,W::ZERO);
   for &i in &panel.faces{let f=&target.faces[i];let plane=target.planes[i];let mut p=[W::ZERO;3];p[u]=xy[0];p[v]=xy[1];p[axis]=plane[0]+plane[1]*(p[u]-f.v[0][u])+plane[2]*(p[v]-f.v[0][v]);let(flux,limits)=self.flux(p)?;let ratio=target.ratios[i];value=value+dot(ratio,flux);bound=bound+dot(ratio.map(W::abs),limits);}
   let weight=wa*wb*a*panel.det;total=total+value*weight;absolute=absolute+bound*weight;
  }}}
  for f in &target.faces{if f.c[axis].value()!=0.{continue;}let du=sub(f.v[1],f.v[0]);let dv=sub(f.v[2],f.v[1]);for&(a,wa)in &parameters{for&(b,wb)in &parameters{
   samples+=1;if samples%32==0{check()?;}let ab=a*b;let p=std::array::from_fn(|k|f.v[0][k]+a*du[k]+ab*dv[k]);let(flux,bound)=self.flux(p)?;let weight=wa*wb*a;total=total+dot(f.c,flux)*weight;absolute=absolute+dot(f.c.map(W::abs),bound)*weight;
  }}}
  let value=total.scale(-0.5).value();let condition=absolute.scale(0.5).value()/value;
  if value>0.&&value.is_finite()&&condition.is_finite(){Ok((value,condition))}else{Err(error("SURFACE_RANGE",&format!("compensated matched boundary result={value:e}, condition={condition:e}")))}
 }
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn actual615_619_affine_first_moments_are_conserved(){
  let bits:[[[u64;3];4];2]=[[[4563403202799767388,4556432660818993087,0],[4563403203263623856,4556432662674418958,0],[4563407430421976187,4556412865211646608,0],[4563407430421976187,4556412865211646608,4540289913102728146]],[[4563403202799767388,4556432660818993087,0],[4563403203263623856,4556432662674418958,0],[4563403203263623856,4556432662674418958,4540289913102728146],[4563347109015053140,4556695373729287373,4540289913102728146]]];
  let[a,b]=bits.map(|t|Source::new(t.map(|p|p.map(f64::from_bits))).unwrap());
  assert!(!a.aligned_panels(&b).unwrap().is_empty());assert!(!b.aligned_panels(&a).unwrap().is_empty());
 }
 #[test]fn actual618_616_affine_panels_preserve_tiny_area_and_reciprocity(){
  let bits:[[[u64;3];4];2]=[[[4563403202799767388,4556432660818993087,0],[4563403203263623856,4556432662674418958,0],[4563347109015053140,4556695373729287373,0],[4563347109015053140,4556695373729287373,4540289913102728146]],[[4563403202799767388,4556432660818993087,0],[4563403203263623856,4556432662674418958,0],[4563403203263623856,4556432662674418958,4540289913102728146],[4563407430421976187,4556412865211646608,4540289913102728146]]];
  let[a,b]=bits.map(|t|Source::new(t.map(|p|p.map(f64::from_bits))).unwrap());
  let norm=(f64::from_bits(4093089433031096330)*f64::from_bits(4063510616520202797)).sqrt();
  let low=(a.integral(&b,8,||Ok(())).unwrap().0,b.integral(&a,8,||Ok(())).unwrap().0);
  let high=(a.integral(&b,16,||Ok(())).unwrap().0,b.integral(&a,16,||Ok(())).unwrap().0);
  assert!((high.0-high.1).abs()/norm<=1e-9);
  assert!((high.0-low.0).abs()/high.0<=0.002&&(high.1-low.1).abs()/high.1<=0.002);
 }
 const ACTUAL:[[[u64;3];4];3]=[
 [[4546088046406550946,4548142162825655091,0],[4548142162825655091,4546088046406550946,0],[4533838765593498485,4533838765593498485,0],[4533838765593498485,4533838765593498485,4540289913102728146]],
 [[4546088046406550946,4548142162825655091,0],[4546088046406550946,4548142162825655091,4540289913102728146],[4548142162825655091,4546088046406550946,4540289913102728146],[4533838765593498485,4533838765593498485,4540289913102728146]],
 [[4562215037549443809,4555498392594732286,0],[4562215037549443809,4555498392594732286,4540289913102728146],[4562369801507634676,4556029311575413228,4540289913102728146],[4562189382644632912,4555948000491515937,4540289913102728146]]];
 #[test]fn exact_duplicate_projections_and_vertical_faces_remain_exact(){for bits in ACTUAL{let t=bits.map(|t|t.map(f64::from_bits));let source=Source::new(t).expect("all actual nondegenerate projected tetra must be represented");for f in &source.faces{let same_xy=(0..3).any(|i|f.v[i][0].value()==f.v[(i+1)%3][0].value()&&f.v[i][1].value()==f.v[(i+1)%3][1].value());if same_xy{assert_eq!(f.c[2].value(),0.,"vertical original face lost its exact zero projection");}}}}
 #[test]fn actual_source_flux_matches_independent_positive_volume_quadrature(){for bits in ACTUAL{let t=bits.map(|t|t.map(f64::from_bits));let source=Source::new(t).unwrap();let p=[-0.0005,-0.0004,-0.0003];let(phi,_)=source.flux(p.map(W::f)).unwrap();let(volume,_)=super::super::mesh::tetra_coefficients(t).unwrap();let rule=super::super::prism::GaussRule::new(16).unwrap();let mut direct=[0.;3];super::super::polyhedron::tetra_integrate(t,volume,&rule,|q,w|{let d=std::array::from_fn::<_,3,_>(|k|q[k]-p[k]);let n=d[0].hypot(d[1]).hypot(d[2]);for k in 0..3{direct[k]+=w*d[k]/n;}Ok(())}).unwrap();for k in 0..3{assert!((phi[k].value()-direct[k]).abs()/volume<1e-10);}}}
 #[test]fn shared_edge_flux_preserves_independent_face_assembly_and_all_prism_splits(){
  let a=[0.,0.,0.];let b=[0.001,0.,0.];let c=[0.0004,2_f64.powi(-40),0.];let top=|mut p:[f64;3]|{p[2]=35e-6;p};
  for tetra in [[a,b,c,top(c)],[a,b,top(b),top(c)],[a,top(a),top(b),top(c)]]{let source=Source::new(tetra).unwrap();for p in [[-0.0002,0.0003,0.0001],[0.0004,0.0001,0.0003],[0.002,0.001,0.0004]]{let p=p.map(W::f);let(new,bound)=source.flux(p).unwrap();let mut old=[W::ZERO;3];let mut old_bound=[W::ZERO;3];for f in &source.faces{let(v,a)=f.distance(p).unwrap();for k in 0..3{old[k]=old[k]+f.n[k]*v;old_bound[k]=old_bound[k]+f.n[k].abs()*a;}}for k in 0..3{assert!((new[k]-old[k]).value().abs()<1e-27*(bound[k]+old_bound[k]).value());}}}
 }
}
