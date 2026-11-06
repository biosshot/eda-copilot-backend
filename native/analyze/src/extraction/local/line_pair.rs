//! Experimental finite segment-pair average; no production dispatch.
//! The returned roundoff estimate is heuristic, not an error certificate.
use super::{accurate::Sum, error, Result};

fn sub(a:[f64;3],b:[f64;3])->[f64;3] { std::array::from_fn(|i| Sum::from(a[i]).sub(Sum::from(b[i])).value()) }
fn add(a:[f64;3],b:[f64;3])->[f64;3] { std::array::from_fn(|i| Sum::from(a[i]).add(Sum::from(b[i])).value()) }
fn dot(a:[f64;3],b:[f64;3])->f64 {a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn cross(a:[f64;3],b:[f64;3])->[f64;3] {[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn norm(a:[f64;3])->f64 {a[0].hypot(a[1]).hypot(a[2])}
fn scale(a:[f64;3],s:f64)->[f64;3] {a.map(|x|x*s)}

/// Unit-parameter average of 1/distance; `None` means use another solver.
pub(super) fn average_segment_pair(a:[[f64;3];2],b:[[f64;3];2])->Result<Option<(f64,f64)>> {
    if !a.iter().chain(&b).flatten().all(|x|x.is_finite()) {return Err(error("LINE_PAIR_INPUT","finite endpoints required"));}
    let av=sub(a[1],a[0]); let bv=sub(b[1],b[0]); let la=norm(av); let lb=norm(bv);
    if !la.is_finite() || !lb.is_finite() || la<=0. || lb<=0. {return Ok(None)}
    let r=sub(a[0],b[0]);
    let component=|i:usize,j:usize,k:usize,l:usize| Sum::from(av[i]).mul(bv[j]).sub(Sum::from(av[k]).mul(bv[l]));
    let parts=[component(1,2,2,1),component(2,0,0,2),component(0,1,1,0)];
    let exactly_parallel=parts.iter().all(|v|v.hi==0. && v.lo==0.);
    let cr=parts.map(|v|v.value()); let area=norm(cr); let denom=la*lb;
    if !area.is_finite() || !denom.is_finite() || denom<=0. {return Ok(None)}
    if exactly_parallel {
        let axis=scale(av,1./la); let z=dot(r,axis); let bz=dot(bv,axis);
        let rc=|i:usize,j:usize,k:usize,l:usize| Sum::from(r[i]).mul(av[j]).sub(Sum::from(r[k]).mul(av[l]));
        let parts=[rc(1,2,2,1),rc(2,0,0,2),rc(0,1,1,0)];
        if parts.iter().all(|v|v.hi==0. && v.lo==0.) {return Ok(None)}
        let rho=norm(parts.map(|v|v.value()))/la;
        if rho==0. || !rho.is_finite() {return Ok(None)}
        let ends=[0_f64.min(bz),0_f64.max(bz)];
        return Ok(super::axial_magnetic::axial_kernel_with_scale(rho,[z,z+la],ends).ok().and_then(|(raw,terms)| {
            let v=raw/denom; let estimate=128.*f64::EPSILON*(terms+raw.abs())/denom;
            if v.is_finite() && v>0. && estimate.is_finite() && estimate<=1e-10*v {Some((v,estimate))} else {None}
        }));
    }
    if area==0. || area/denom < 1e-5 {return Ok(None)}
    let minus_b=scale(bv,-1.); let n=scale(cross(av,minus_b),1./area);
    let vertices=[r,add(r,av),add(add(r,av),minus_b),add(r,minus_b)];
    if !vertices.iter().flatten().all(|x|x.is_finite()) {return Ok(None)}
    let lengths=vertices.map(norm); if lengths.iter().any(|x|!x.is_finite() || *x==0.) {return Ok(None)}
    let h=dot(vertices[0],n); let mut sum=Sum::default(); let mut magnitude=0.;
    for i in 0..4 {
        let j=(i+1)%4; let edge=sub(vertices[j],vertices[i]); let len=norm(edge); if !len.is_finite() || len<=0. {return Ok(None)}
        let tangent=scale(edge,1./len); let outward=cross(tangent,n); let d=dot(vertices[i],outward); let t=dot(vertices[i],tangent);
        let perp=sub(vertices[i],scale(tangent,t)); let rho2=dot(perp,perp);
        let residual=if t>=0. {2.*t} else if t<=-len {-2.*(t+len)} else {0.};
        let den=rho2/(lengths[i]+t.abs())+rho2/(lengths[j]+(t+len).abs())+residual;
        if !den.is_finite() || den<=0. {return Ok(None)}
        let term=d*(2.*len/den).ln_1p(); if !term.is_finite() {return Ok(None)} sum=sum.add(Sum::from(term)); magnitude+=term.abs();
    }
    if h!=0. { let mut omega=0.; for [i,j,k] in [[0,1,2],[0,2,3]] {
        let numerator=dot(vertices[i],cross(vertices[j],vertices[k]));
        let den=lengths[i]*lengths[j]*lengths[k]+lengths[i]*dot(vertices[j],vertices[k])+lengths[j]*dot(vertices[k],vertices[i])+lengths[k]*dot(vertices[i],vertices[j]);
        if !numerator.is_finite() || !den.is_finite() || (numerator==0. && den==0.) {return Ok(None)}
        omega+=2.*numerator.atan2(den);
    } let term= -h*omega; if !term.is_finite() {return Ok(None)} sum=sum.add(Sum::from(term)); magnitude+=term.abs(); }
    let surface=sum.value(); if !surface.is_finite() || surface<=0. || !magnitude.is_finite() {return Ok(None)}
    let estimate=256.*f64::EPSILON*magnitude/area; let value=surface/area;
    if !value.is_finite() || !estimate.is_finite() || estimate>1e-10*value {return Ok(None)}
    Ok(Some((value,estimate)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::prism::GaussRule;
    fn reference(a:[[f64;3];2],b:[[f64;3];2],order:usize)->f64 {let q=GaussRule::new(order).unwrap(); let mut v=0.; for (&x,&wx) in q.nodes().iter().zip(q.weights()) {for (&y,&wy) in q.nodes().iter().zip(q.weights()) {let p=add(a[0],scale(sub(a[1],a[0]),x)); let r=add(b[0],scale(sub(b[1],b[0]),y));v+=wx*wy/norm(sub(p,r));}} v}
    #[test] fn segment_pair_controls() {
        let cases=[([[0.,0.,0.],[2.,0.,0.]],[[0.3,-0.8,0.7],[0.3,1.2,0.7]]), ([[0.,0.,0.],[1.,0.2,0.]],[[0.3,0.5,-0.8],[0.5,1.7,-0.6]]), ([[0.,0.,0.],[1.,0.,0.]],[[0.,0.8,0.4],[2.,0.8,0.4]])];
        for (a,b) in cases {let (v,_)=average_segment_pair(a,b).unwrap().unwrap(); let ref32=reference(a,b,32); let ref48=reference(a,b,48); println!("value {v}, reference {ref48}, refinement {}",(ref32-ref48).abs()/ref48); assert!((v-ref48).abs()<1e-10*ref48); let reverse=average_segment_pair(b,a).unwrap().unwrap().0; assert!((v-reverse).abs()<1e-10*v);
            let transform=|mut x:[[f64;3];2]| {for p in &mut x {let [x,y,z]=*p; *p=[-y+10.,x-5.,z+2.];} x}; let shifted=average_segment_pair(transform(a),transform(b)).unwrap().unwrap().0; assert!((shifted-v).abs()<1e-10*v);
            let rev=[a[1],a[0]]; assert!((average_segment_pair(rev,b).unwrap().unwrap().0-v).abs()<1e-10*v);
            let scale2=|x:[[f64;3];2]| x.map(|p|scale(p,2.)); assert!((average_segment_pair(scale2(a),scale2(b)).unwrap().unwrap().0*2.-v).abs()<1e-10*v);
        }
        let diagonal=[[0.,0.,0.],[1.,1.,0.]];
        assert!(average_segment_pair(diagonal,[[0.5,0.5,0.],[1.5,1.5,0.]]).unwrap().is_none());
        assert!(average_segment_pair(diagonal,[[2.,2.,0.],[3.,3.,0.]]).unwrap().is_none());
        let p=[[0.,0.,0.],[1.,0.,0.]]; let q=[[0.1,0.5,0.],[1.1,0.5,0.]];
        let v=average_segment_pair(p,q).unwrap().unwrap().0; let anti=average_segment_pair(p,[q[1],q[0]]).unwrap().unwrap().0;
        assert!((v-reference(p,q,48)).abs()<1e-10*v && (v-anti).abs()<1e-10*v);
        assert!(average_segment_pair(p,[[1e9,1.,0.],[1e9+1.,1.,0.]]).unwrap().is_none());
        assert!(average_segment_pair([[0.,0.,0.],[1.,0.,0.]],[[0.,1.,0.],[1.,1.00000001,0.]]).unwrap().is_none());
        assert!(average_segment_pair([[0.,0.,0.],[1.,0.,0.]],[[1e12,1.,0.],[1e12,2.,0.]]).unwrap().is_none());
    }
}
