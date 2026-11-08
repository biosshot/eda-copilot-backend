//! Experimental finite axial double integral and disjoint section bilinear form.
//! No production dispatch; constant-profile self terms use a signed fan. P1 self is unsupported.
use super::{error, Result};
use super::prism::{GaussRule, Prism};

pub(super) fn axial_kernel(rho: f64, a: [f64; 2], b: [f64; 2]) -> Result<f64> {
    axial_kernel_with_scale(rho,a,b).map(|(value,_)|value)
}

pub(super) fn axial_kernel_with_scale(rho: f64, a: [f64; 2], b: [f64; 2]) -> Result<(f64,f64)> {
    if !rho.is_finite() || rho <= 0. || !a.iter().chain(b.iter()).all(|x| x.is_finite()) || a[1] <= a[0] || b[1] <= b[0] {
        return Err(error("AXIAL_INPUT", "positive separation and finite ordered endpoints required; rho=0 requires singular outer integration"));
    }
    let f = |s: f64| s * (s / rho).asinh() - s * (s / (s.hypot(rho) + rho));
    let terms = [f(a[1]-b[0]), f(a[0]-b[1]), -f(a[1]-b[1]), -f(a[0]-b[0])];
    let value: f64 = terms.iter().sum();
    let scale: f64 = terms.iter().map(|x| x.abs()).sum();
    if !value.is_finite() || value <= 0. || value <= 1e-10 * scale {
        return Err(error("AXIAL_UNRESOLVED", "endpoint cancellation or range; use full solver"));
    }
    Ok((value,scale))
}

/// P1 nodal densities, with two axial dimensions integrated analytically.
/// Bounding-box separation conservatively refuses touching or overlapping sections.
pub(super) fn section_bilinear(a: &Prism, pa: [f64; 3], b: &Prism, pb: [f64; 3], rule: &GaussRule) -> Result<f64> {
    if !pa.iter().chain(pb.iter()).all(|x| x.is_finite()) { return Err(error("AXIAL_INPUT", "finite profiles required")); }
    let separated = (0..2).any(|d| {
        let bounds = |p: &Prism| p.triangle_m.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo,hi), x| (lo.min(x[d]),hi.max(x[d])));
        let (al,ah)=bounds(a); let (bl,bh)=bounds(b); ah < bl || bh < al
    });
    if !separated { return Err(error("AXIAL_SINGULAR", "section overlap or unproven separation requires singular outer rule")); }
    let samples = |p: &Prism, profile: [f64;3]| {
        let t=p.triangle_m;
        let jac=((t[1][0]-t[0][0])*(t[2][1]-t[0][1])-(t[1][1]-t[0][1])*(t[2][0]-t[0][0])).abs();
        let mut out=Vec::new();
        for (&u,&wu) in rule.nodes().iter().zip(rule.weights()) { for (&v,&wv) in rule.nodes().iter().zip(rule.weights()) {
            let l=[1.-u,u*(1.-v),u*v];
            out.push(([l[0]*t[0][0]+l[1]*t[1][0]+l[2]*t[2][0],l[0]*t[0][1]+l[1]*t[1][1]+l[2]*t[2][1]], jac*u*wu*wv*(0..3).map(|i| l[i]*profile[i]).sum::<f64>()));
        }} out
    };
    let aa=samples(a,pa); let bb=samples(b,pb); let mut total=0.;
    for (x,wx) in &aa { for (y,wy) in &bb { total += wx*wy*axial_kernel((x[0]-y[0]).hypot(x[1]-y[1]),a.z_m,b.z_m)?; }}
    if !total.is_finite() { return Err(error("AXIAL_UNRESOLVED", "nonfinite section integral")); } Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::prism::volume_integral;
    #[test] fn cross_prisms_match_volume_and_reciprocity() {
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,2.]).unwrap();
        let b=Prism::new("b".into(),[[3.,0.],[4.,0.],[3.,1.]],[0.3,1.1]).unwrap();
        let q=GaussRule::new(12).unwrap();
        let x=section_bilinear(&a,[1.;3],&b,[1.;3],&q).unwrap();
        let y=section_bilinear(&b,[1.;3],&a,[1.;3],&q).unwrap();
        let reference=volume_integral(&a,&b,&q).unwrap();
        assert!((x-y).abs()<1e-12*x && (x-reference).abs()<1e-8*x, "{x} {y} {reference}");
        for n in [2,4,8] { let mut sum=0.; for i in 0..n { for j in 0..n { let mut ai=a.clone(); let mut bj=b.clone(); ai.z_m=[2.*i as f64/n as f64,2.*(i+1) as f64/n as f64]; bj.z_m=[0.3+0.8*j as f64/n as f64,0.3+0.8*(j+1) as f64/n as f64]; sum+=section_bilinear(&ai,[1.;3],&bj,[1.;3],&q).unwrap(); }} assert!((sum-x).abs()<1e-10*x); }
        assert!(axial_kernel(0.,[0.,1.],[0.,1.]).is_err());
        assert!(axial_kernel(1.,[0.,1.],[1e12,1e12+1.]).is_err());
        // Independent six-dimensional volume rule for nonconstant profiles.
        let density_a=|p:[f64;2]|1.+p[0]+2.*p[1];
        let density_b=|p:[f64;2]|2.-0.1*p[0]+0.3*p[1];
        let weighted=section_bilinear(&a,a.triangle_m.map(density_a),&b,b.triangle_m.map(density_b),&q).unwrap();
        let samples=|p:&Prism,density:fn([f64;2])->f64,order| {
            let rule=GaussRule::new(order).unwrap();let mut out=Vec::new();let t=p.triangle_m;
            for (&u,&wu) in rule.nodes().iter().zip(rule.weights()) {for (&v,&wv) in rule.nodes().iter().zip(rule.weights()) {for (&z,&wz) in rule.nodes().iter().zip(rule.weights()) {
                let xy=[t[0][0]+u*(t[1][0]-t[0][0])+(1.-u)*v*(t[2][0]-t[0][0]),t[0][1]+u*(t[1][1]-t[0][1])+(1.-u)*v*(t[2][1]-t[0][1])];
                out.push(([xy[0],xy[1],p.z_m[0]+z*(p.z_m[1]-p.z_m[0])],2.*p.volume_m3*(1.-u)*wu*wv*wz*density(xy)));
            }}}out
        };
        let mut previous:Option<f64>=None;
        for order in [6,8] {let aa=samples(&a,density_a,order);let bb=samples(&b,density_b,order);let mut reference=0.;
            for (x,wx) in &aa {for (y,wy) in &bb {reference+=wx*wy/((x[0]-y[0]).hypot(x[1]-y[1]).hypot(x[2]-y[2]));}}
            assert!((weighted-reference).abs()<1e-7*reference);
            if let Some(old)=previous {assert!((reference-old).abs()<1e-7*reference);}previous=Some(reference);
        }
    }
}


/// Constant-density section pair; signed source fans analytically integrate radius.
pub(super) fn constant_pair(a: &Prism, b: &Prism, rule: &GaussRule) -> Result<f64> {
    constant_pair_impl(a,b,rule,None)
}
pub(super) fn constant_pair_guarded(a: &Prism, b: &Prism, rule: &GaussRule, start: std::time::Instant, limit: std::time::Duration) -> Result<f64> {
    constant_pair_impl(a,b,rule,Some((start,limit)))
}
fn constant_pair_impl(a: &Prism, b: &Prism, rule: &GaussRule, deadline: Option<(std::time::Instant,std::time::Duration)>) -> Result<f64> {
    if !a.z_m.iter().chain(b.z_m.iter()).all(|x| x.is_finite()) || a.z_m[1]<=a.z_m[0] || b.z_m[1]<=b.z_m[0] { return Err(error("AXIAL_INPUT", "finite ordered axial endpoints required")); }
    let t=a.triangle_m;
    let jac=((t[1][0]-t[0][0])*(t[2][1]-t[0][1])-(t[1][1]-t[0][1])*(t[2][0]-t[0][0])).abs();
    let mut total=0.; let mut magnitude=0.;
    for (&u,&wu) in rule.nodes().iter().zip(rule.weights()) { for (&v,&wv) in rule.nodes().iter().zip(rule.weights()) {
        if deadline.is_some_and(|(start,limit)| start.elapsed() >= limit) { return Err(error("DEADLINE", "axial quadrature deadline exceeded")); }
        let l=[1.-u,u*(1.-v),u*v];
        let p=[(0..3).map(|i|l[i]*t[i][0]).sum::<f64>(),(0..3).map(|i|l[i]*t[i][1]).sum::<f64>()];
        for e in 0..3 { let x=[b.triangle_m[e][0]-p[0],b.triangle_m[e][1]-p[1]]; let y=[b.triangle_m[(e+1)%3][0]-p[0],b.triangle_m[(e+1)%3][1]-p[1]];
            let cross=x[0]*y[1]-x[1]*y[0]; if cross == 0. { continue; }
            for (&w,&ww) in rule.nodes().iter().zip(rule.weights()) {
                let r=(x[0]*(1.-w)+y[0]*w).hypot(x[1]*(1.-w)+y[1]*w);
                if r==0. { return Err(error("AXIAL_UNRESOLVED","degenerate fan")); }
                let f=|s:f64| s*super::prism::radial(s/r)-s*s/3.*(1./(r.hypot(s)+r)+1./(r.hypot(s)+s.abs()));
                let terms=[f(a.z_m[1]-b.z_m[0]),f(a.z_m[0]-b.z_m[1]),-f(a.z_m[1]-b.z_m[1]),-f(a.z_m[0]-b.z_m[0])];
                let val:f64=terms.iter().sum(); let scale:f64=terms.iter().map(|x|x.abs()).sum();
                if !val.is_finite() || val<=1e-10*scale { return Err(error("AXIAL_UNRESOLVED","fan endpoint cancellation")); }
                let term=jac*u*wu*wv*cross*ww*val; total+=term; magnitude+=term.abs();
            }
        }
    }}
    if !total.is_finite() || total<=1e-10*magnitude { return Err(error("AXIAL_UNRESOLVED","fan cancellation")); } Ok(total)
}

#[cfg(test)]
mod shared_test {
    use super::*;
    use super::super::prism::volume_integral;
    #[test] fn shared_return_gram() {
        let q=GaussRule::new(32).unwrap();
        let shapes:Vec<_>=(0..3).map(|i| {let x=2.*i as f64; Prism::new(i.to_string(),[[x,0.],[x+1.,0.],[x,1.]],[0.,1.]).unwrap()}).collect();
        let mut g=[[0.;3];3]; let mut reference=[[0.;3];3];
        for i in 0..3 {for j in 0..3 {g[i][j]=constant_pair(&shapes[i],&shapes[j],&q).unwrap(); reference[i][j]=volume_integral(&shapes[i],&shapes[j],&q).unwrap(); assert!((g[i][j]-reference[i][j]).abs()<0.00001*reference[i][j],"{} {} {} {}",i,j,g[i][j],reference[i][j]); }}
        for i in 0..3 {for j in 0..3 {println!("entry {i},{j}: relative error {}", (g[i][j]-reference[i][j]).abs()/reference[i][j]); assert!((g[i][j]-g[j][i]).abs()<1e-8*g[i][j]);}}
        // Positive definiteness for every balanced combination of the two signals.
        let modes=[[1.,0.,-1.],[0.,1.,-1.]];
        let mut balanced=[[0.;2];2];
        for a in 0..2 {for b in 0..2 {for i in 0..3 {for j in 0..3 {balanced[a][b]+=4.*modes[a][i]*g[i][j]*modes[b][j];}}}}
        let mutual=0.5*(balanced[0][1]+balanced[1][0]);
        assert!(balanced[0][0]>0.&&balanced[1][1]>0.&&balanced[0][0]*balanced[1][1]>mutual*mutual);
        // Unit currents share one return triangle; density is current divided by area.
        for currents in [[2.,3.,-5.],[1.,0.,-1.],[0.,1.,-1.]] {let mut energy=0.; let mut ref_energy=0.; for i in 0..3 {for j in 0..3 {energy+=4.*currents[i]*currents[j]*g[i][j]; ref_energy+=4.*currents[i]*currents[j]*reference[i][j];}} println!("energy relative error {}",(energy-ref_energy).abs()/ref_energy); assert!(energy>0. && (energy-ref_energy).abs()<0.00001*ref_energy);}
        for order in [12,20,32] { let rule=GaussRule::new(order).unwrap(); let start=std::time::Instant::now(); let value=constant_pair(&shapes[0],&shapes[0],&rule).unwrap(); println!("order {order}: self relative error {}, whole 1 call {:?}",(value-reference[0][0]).abs()/reference[0][0],start.elapsed()); }
        let mut a=shapes[0].clone(); let mut b=shapes[1].clone(); a.z_m=[-0.4,1.7]; b.z_m=[0.2,0.9]; let original=constant_pair(&a,&b,&q).unwrap(); for p in [&mut a,&mut b] {for xy in &mut p.triangle_m {xy[0]+=100.;xy[1]-=80.;} for z in &mut p.z_m {*z+=30.;}} let shifted=constant_pair(&a,&b,&q).unwrap(); assert!((original-shifted).abs()<1e-11*original);
        let mut invalid=a.clone(); invalid.z_m=[1.,1.]; assert!(constant_pair(&invalid,&b,&q).is_err());
        for n in [2,4,8] {let start=std::time::Instant::now(); let mut sum=0.; for i in 0..n {for j in 0..n {let mut a=shapes[0].clone();let mut b=shapes[0].clone(); a.z_m=[i as f64/n as f64,(i+1) as f64/n as f64];b.z_m=[j as f64/n as f64,(j+1) as f64/n as f64];sum+=constant_pair(&a,&b,&q).unwrap();}} println!("split {n}: {} calls, {:?}",n*n,start.elapsed()); assert!((sum-g[0][0]).abs()<1e-8*g[0][0]);}
    }
}


