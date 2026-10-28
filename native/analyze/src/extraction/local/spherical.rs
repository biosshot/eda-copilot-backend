//! Smooth regular solid-harmonic trial functions with analytic exterior energy.
//! These are field basis functions, never substitutes for copper current cells.
pub(crate) fn values_gradients(point:[f64;3],center:[f64;3],a:f64)->([f64;9],[[f64;3];9]) {
    let p=std::array::from_fn::<_,3,_>(|i|(point[i]-center[i])/a);
    let [x,y,z]=p;let r2=x*x+y*y+z*z;
    let h=[1.,x,y,z,x*y,x*z,y*z,x*x-y*y,3.*z*z-r2];
    let dh=[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.],[y,x,0.],[z,0.,x],[0.,z,y],[2.*x,-2.*y,0.],[-2.*x,-2.*y,4.*z]];
    let factors=if r2<=1. {[(3.-r2)/2.,(5.-3.*r2)/2.,(7.-5.*r2)/2.]}else{let f=1./r2.sqrt();[f,f/r2,f/(r2*r2)]};
    let mut values=[0.;9];let mut gradients=[[0.;3];9];
    for i in 0..9 {
        let l=if i==0 {0}else if i<=3 {1}else{2};let power=(2*l+1) as f64;
        let f=factors[l];
        values[i]=h[i]*f;
        for k in 0..3 {let df=if r2<=1. {-power*p[k]}else{-power*f*p[k]/r2};gradients[i][k]=(dh[i][k]*f+h[i]*df)/a;}
    }
    (values,gradients)
}
pub(crate) fn energies(a:f64)->[f64;9] {
    let pi=std::f64::consts::PI;
    let norms=[4.*pi,4.*pi/3.,4.*pi/3.,4.*pi/3.,4.*pi/15.,4.*pi/15.,4.*pi/15.,16.*pi/15.,16.*pi/5.];
    std::array::from_fn(|i|{let l=if i==0 {0}else if i<=3 {1}else{2};a*norms[i]*2.*(2*l+1) as f64*(2*l+3) as f64/(2*l+5) as f64})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exterior_gradient_and_independent_whole_space_energy() {
        let a=0.001;let center=[0.;3];let eps=a*1e-6;
        for p in [[0.2*a,0.3*a,-0.1*a],[1.5*a,0.1*a,0.2*a],[a,0.,0.]] {
            let (_,g)=values_gradients(p,center,a);
            for k in 0..3 {let mut hi=p;let mut lo=p;hi[k]+=eps;lo[k]-=eps;let(vhi,_)=values_gradients(hi,center,a);let(vlo,_)=values_gradients(lo,center,a);
                for j in 0..9 {assert!((g[j][k]-(vhi[j]-vlo[j])/(2.*eps)).abs()*a<3e-5,"p={p:?} mode={j} axis={k} analytic={} difference={}",g[j][k],(vhi[j]-vlo[j])/(2.*eps));}
            }
        }
        // Integrate the actual gradient over all R³, splitting the radial
        // interface. Angular trapezoids are exact for these finite harmonics;
        // radial Gauss integrates the remaining polynomials independently.
        let rule=super::super::prism::GaussRule::new(8).unwrap();let mut gram=[[0.;9];9];let pi=std::f64::consts::PI;
        for exterior in [false,true] {for(&t,&wt)in rule.nodes.iter().zip(&rule.weights) {
            let(r,jac)=if exterior {(a/t,a.powi(3)/t.powi(4))}else{(a*t,a.powi(3)*t*t)};
            for(&u,&wu)in rule.nodes.iter().zip(&rule.weights) {let z=2.*u-1.;let rho=(1.-z*z).sqrt();for k in 0..24 {let phi=2.*pi*k as f64/24.;let(_,g)=values_gradients([r*rho*phi.cos(),r*rho*phi.sin(),r*z],center,a);let weight=jac*wt*2.*wu*2.*pi/24.;
                for i in 0..9 {for j in 0..9 {gram[i][j]+=weight*(0..3).map(|k|g[i][k]*g[j][k]).sum::<f64>();}}
            }}
        }}
        let expected=energies(a);for i in 0..9 {for j in 0..9 {let reference=if i==j {expected[i]}else{0.};assert!((gram[i][j]-reference).abs()/expected[i].max(expected[j])<1e-11,"({i},{j}) {} vs {reference}",gram[i][j]);}}
    }
}
