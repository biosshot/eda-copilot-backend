//! Experimental compact prism Gram operators; no runtime or acceptance claim.
use std::time::{Duration, Instant};
use super::{error, Result, AllocationBudget, axial_magnetic, prism::{self, Prism, GaussRule}};

/// Coefficients multiply volume-constant densities, in m^-3 and m^-2.
pub(super) struct Support {
    pub prism: Prism,
    pub charge_density: Vec<f64>,
    pub current_density: Vec<[f64; 3]>,
}
pub(super) struct Gram { pub potential_per_farad: Vec<Vec<f64>>, pub inductance_h: Vec<Vec<f64>>, pub max_order: usize, pub max_relative_change: f64 }

/// Homogeneous permittivity in F/m. The caller owns topology and disjointness.
/// Returns an error when the order-pair estimator does not converge.
pub(super) fn gram(supports: &[Support], charge_modes: usize, current_modes: usize, order: usize, epsilon: f64, budget: AllocationBudget, limit: Duration) -> Result<Gram> {
    let start = Instant::now();
    if supports.is_empty() || charge_modes > budget.max_nodes || current_modes > budget.max_nodes || supports.len() > budget.max_tetrahedra || !(8..=60).contains(&order) || order % 4 != 0 || !epsilon.is_finite() || epsilon <= 0. { return Err(error("PLANAR_INPUT", "invalid dimensions, order or permittivity")); }
    let bytes = charge_modes.checked_mul(charge_modes).and_then(|n| n.checked_add(current_modes.checked_mul(current_modes)?)).and_then(|n| n.checked_mul(8)).and_then(|n| n.checked_add(supports.len().checked_mul(charge_modes.checked_mul(8)?.checked_add(current_modes.checked_mul(24)?)?)?)).ok_or_else(|| error("MEMORY_LIMIT", "planar Gram size overflow"))?;
    let bytes = bytes.checked_add(supports.iter().try_fold(0usize, |n,s| n.checked_add(s.prism.source_id.len())) .ok_or_else(|| error("MEMORY_LIMIT", "source IDs overflow"))?).and_then(|n| n.checked_add(64 * 1024)).ok_or_else(|| error("MEMORY_LIMIT", "planar scratch overflow"))?;
    budget.check(bytes)?;
    for (i, s) in supports.iter().enumerate() {
        if start.elapsed() >= limit { return Err(error("DEADLINE", "planar preflight deadline exceeded")); }
        let checked = Prism::new(s.prism.source_id.clone(), s.prism.triangle_m, s.prism.z_m)?;
        if checked.volume_m3 != s.prism.volume_m3 || s.charge_density.len() != charge_modes || s.current_density.len() != current_modes || !s.charge_density.iter().chain(s.current_density.iter().flatten()).all(|x| x.is_finite()) { return Err(error("PLANAR_INPUT", "invalid prism or basis")); }
        for t in &supports[..i] {
            if start.elapsed() >= limit { return Err(error("DEADLINE", "planar preflight deadline exceeded")); }
            if s.prism.z_m[0] < t.prism.z_m[1] && t.prism.z_m[0] < s.prism.z_m[1] && triangle_interior_overlap(s.prism.triangle_m, t.prism.triangle_m) {
                return Err(error("PLANAR_OWNERSHIP", "overlapping copper supports"));
            }
        }
    }
    let mut max_order = 0; let mut max_relative_change: f64 = 0.;
    let mut p = vec![vec![0.; charge_modes]; charge_modes]; let mut l = vec![vec![0.; current_modes]; current_modes];
    for (a, sa) in supports.iter().enumerate() { for (b, sb) in supports[..=a].iter().enumerate() {
        if start.elapsed() >= limit { return Err(error("DEADLINE", "planar Gram deadline exceeded")); }
        if sa.charge_density.iter().all(|&v| v == 0.) && sa.current_density.iter().flatten().all(|&v| v == 0.) || sb.charge_density.iter().all(|&v| v == 0.) && sb.current_density.iter().flatten().all(|&v| v == 0.) { continue; }
        let (y, n, relative) = pair_integral(&sa.prism, &sb.prism, order, start, limit)?;
        max_order = max_order.max(n); max_relative_change = max_relative_change.max(relative);
        for i in 0..charge_modes { for j in 0..charge_modes {
            if sa.charge_density[i] == 0. || sb.charge_density[j] == 0. { continue; }
            let value = y * sa.charge_density[i] * sb.charge_density[j] / (4. * std::f64::consts::PI * epsilon);
            p[i][j] += value; if a != b { p[j][i] += value; }
        }}
        for i in 0..current_modes { for j in 0..current_modes {
            if sa.current_density[i] == [0.;3] || sb.current_density[j] == [0.;3] { continue; }
            let dot: f64 = (0..3).map(|d| sa.current_density[i][d] * sb.current_density[j][d]).sum();
            let value = 1e-7 * y * dot;
            l[i][j] += value; if a != b { l[j][i] += value; }
        }}
    }}
    if !p.iter().flatten().chain(l.iter().flatten()).all(|v| v.is_finite()) { return Err(error("PLANAR_RANGE", "non-finite Gram")); }
    Ok(Gram { potential_per_farad: p, inductance_h: l, max_order, max_relative_change })
}

/// Compact basis entries must have strictly increasing unique mode indices.
pub(super) struct SparseSupport { pub prism: Prism, pub charge_density: Vec<(usize,f64)>, pub current_density: Vec<(usize,[f64;3])> }
fn sparse_valid(s: &SparseSupport, nq: usize, ni: usize) -> bool {
    s.charge_density.iter().all(|(i,v)| *i < nq && v.is_finite() && *v != 0.) && s.charge_density.windows(2).all(|w| w[0].0 < w[1].0)
    && s.current_density.iter().all(|(i,v)| *i < ni && v.iter().all(|x| x.is_finite()) && *v != [0.;3]) && s.current_density.windows(2).all(|w| w[0].0 < w[1].0)
}

pub(super) fn gram_sparse(supports: &[SparseSupport], charge_modes: usize, current_modes: usize, order: usize, epsilon: f64, budget: AllocationBudget, limit: Duration) -> Result<Gram> {
    let start = Instant::now();
    if supports.is_empty() || charge_modes > budget.max_nodes || current_modes > budget.max_nodes || supports.len() > budget.max_tetrahedra || !(8..=60).contains(&order) || order % 4 != 0 || !epsilon.is_finite() || epsilon <= 0. { return Err(error("PLANAR_INPUT", "invalid dimensions, order or permittivity")); }
    let bytes = charge_modes.checked_mul(charge_modes).and_then(|n| n.checked_add(current_modes.checked_mul(current_modes)?)).and_then(|n| n.checked_mul(8)).and_then(|n| n.checked_add(supports.iter().try_fold(0usize, |sum, s| sum.checked_add(s.charge_density.len().checked_mul(16)?)?.checked_add(s.current_density.len().checked_mul(32)?))?)).ok_or_else(|| error("MEMORY_LIMIT", "planar Gram size overflow"))?;
    let bytes = bytes.checked_add(supports.iter().try_fold(0usize, |n,s| n.checked_add(s.prism.source_id.len())) .ok_or_else(|| error("MEMORY_LIMIT", "source IDs overflow"))?).and_then(|n| n.checked_add(64 * 1024)).ok_or_else(|| error("MEMORY_LIMIT", "planar scratch overflow"))?;
    budget.check(bytes)?;
    for (i, s) in supports.iter().enumerate() {
        if start.elapsed() >= limit { return Err(error("DEADLINE", "planar preflight deadline exceeded")); }
        let checked = Prism::new(s.prism.source_id.clone(), s.prism.triangle_m, s.prism.z_m)?;
        if checked.volume_m3 != s.prism.volume_m3 || !sparse_valid(s, charge_modes, current_modes) { return Err(error("PLANAR_INPUT", "invalid prism or basis")); }
        for t in &supports[..i] {
            if start.elapsed() >= limit { return Err(error("DEADLINE", "planar preflight deadline exceeded")); }
            if s.prism.z_m[0] < t.prism.z_m[1] && t.prism.z_m[0] < s.prism.z_m[1] && triangle_interior_overlap(s.prism.triangle_m, t.prism.triangle_m) {
                return Err(error("PLANAR_OWNERSHIP", "overlapping copper supports"));
            }
        }
    }
    let mut max_order = 0; let mut max_relative_change: f64 = 0.;
    let mut p = vec![vec![0.; charge_modes]; charge_modes]; let mut l = vec![vec![0.; current_modes]; current_modes];
    for (a, sa) in supports.iter().enumerate() { for (b, sb) in supports[..=a].iter().enumerate() {
        if start.elapsed() >= limit { return Err(error("DEADLINE", "planar Gram deadline exceeded")); }
        if sa.charge_density.is_empty() && sa.current_density.is_empty() || sb.charge_density.is_empty() && sb.current_density.is_empty() { continue; }
        let (y, n, relative) = pair_integral(&sa.prism, &sb.prism, order, start, limit)?;
        max_order = max_order.max(n); max_relative_change = max_relative_change.max(relative);
        for &(i, va) in &sa.charge_density { for &(j, vb) in &sb.charge_density {
            let value = y * va * vb / (4. * std::f64::consts::PI * epsilon);
            p[i][j] += value; if a != b { p[j][i] += value; }
        }}
        for &(i, va) in &sa.current_density { for &(j, vb) in &sb.current_density {
            let dot: f64 = (0..3).map(|d| va[d] * vb[d]).sum();
            let value = 1e-7 * y * dot;
            l[i][j] += value; if a != b { l[j][i] += value; }
        }}
    }}
    if !p.iter().flatten().chain(l.iter().flatten()).all(|v| v.is_finite()) { return Err(error("PLANAR_RANGE", "non-finite Gram")); }
    Ok(Gram { potential_per_farad: p, inductance_h: l, max_order, max_relative_change })
}

// Triangle autocorrelation: difference domain is a six-sector hexagon.
// On each sector overlap area is A*(1-r)^2; xy difference Jacobian
// is 2*A*r. r=t^4 regularizes the logarithmic axial kernel at zero.
fn self_integral(p: &Prism, order: usize, start: Instant, limit: Duration) -> Result<f64> {
    let q=GaussRule::new(order)?;
    let h=p.z_m[1]-p.z_m[0]; let area=p.volume_m3/h;
    let e1=[p.triangle_m[1][0]-p.triangle_m[0][0],p.triangle_m[1][1]-p.triangle_m[0][1]];
    let e2=[p.triangle_m[2][0]-p.triangle_m[0][0],p.triangle_m[2][1]-p.triangle_m[0][1]];
    let vertices=[[1.,0.],[0.,1.],[-1.,1.],[-1.,0.],[0.,-1.],[1.,-1.]];
    let mut sum=0.;
    for k in 0..6 { for (&t,&wt) in q.nodes().iter().zip(q.weights()) {
        if start.elapsed() >= limit { return Err(error("DEADLINE", "self quadrature deadline exceeded")); }
        let r=t.powi(4); let dr=4.*t.powi(3);
        for (&v,&wv) in q.nodes().iter().zip(q.weights()) {
            let a=vertices[k]; let b=vertices[(k+1)%6];
            let u=(1.-v)*a[0]+v*b[0]; let w=(1.-v)*a[1]+v*b[1];
            let rho=r*(u*e1[0]+w*e2[0]).hypot(u*e1[1]+w*e2[1]);
            let axial=axial_magnetic::axial_kernel(rho,[0.,h],[0.,h])?;
            sum += wt*wv*dr*r*(1.-r).powi(2)*axial;
        }
    }}
    let value=2.*area*area*sum;
    if !value.is_finite() || value<=0. { return Err(error("PLANAR_RANGE", "invalid self integral")); } Ok(value)
}

// For the two triangles of an axis-aligned rectangular prism, compute the
// cross term from the rectangular autocorrelation and the two self terms.
// Split the rectangle difference quadrant on its diagonal; r=t^4 resolves
// the logarithmic origin without sampling the singularity.
fn rectangle_pair(a: &Prism,b: &Prism,order: usize,start: Instant,limit: Duration) -> Result<Option<f64>> {
    if a.z_m != b.z_m { return Ok(None); }
    let mut points=Vec::new();
    for p in a.triangle_m.into_iter().chain(b.triangle_m) { if !points.contains(&p) { points.push(p); } }
    if points.len()!=4 { return Ok(None); }
    let xmin=points.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min); let xmax=points.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max);
    let ymin=points.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min); let ymax=points.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max);
    if !points.iter().all(|p| (p[0]==xmin || p[0]==xmax) && (p[1]==ymin || p[1]==ymax)) { return Ok(None); }
    let w=xmax-xmin; let h=ymax-ymin; let z=a.z_m[1]-a.z_m[0];
    if w<=0. || h<=0. || (a.volume_m3+b.volume_m3-w*h*z).abs()>64.*f64::EPSILON*w*h*z { return Ok(None); }
    let q=GaussRule::new(order)?; let mut sum=0.;
    for (&t,&wt) in q.nodes().iter().zip(q.weights()) {
        if start.elapsed()>=limit { return Err(error("DEADLINE","rectangle quadrature deadline exceeded")); }
        let r=t.powi(4); let dr=4.*t.powi(3);
        for (&v,&wv) in q.nodes().iter().zip(q.weights()) {
            for (x,y) in [(r,r*v),(r*v,r)] {
                let rho=(w*x).hypot(h*y);
                sum+=wt*wv*dr*r*(1.-x)*(1.-y)*axial_magnetic::axial_kernel(rho,[0.,z],[0.,z])?;
            }
        }
    }
    let whole=4.*w*w*h*h*sum;
    let cross=(whole-self_integral(a,order,start,limit)?-self_integral(b,order,start,limit)?)*0.5;
    if !cross.is_finite() || cross<=0. { return Err(error("PLANAR_RANGE","invalid rectangle cross integral")); }
    Ok(Some(cross))
}

// Shared-edge Duffy transform. Coordinates u,v are distances from the
// common edge; q is split on either side of t. The singular corner
// (u,v,|q-t|)=0 is partitioned into three largest-coordinate sectors.
fn edge_integral(a:&Prism,b:&Prism,order:usize,start:Instant,limit:Duration) -> Result<Option<f64>> {
    if a.z_m != b.z_m { return Ok(None); }
    let common:Vec<_>=a.triangle_m.iter().copied().filter(|p|b.triangle_m.contains(p)).collect();
    if common.len()!=2 { return Ok(None); }
    let ca=*a.triangle_m.iter().find(|p|!common.contains(p)).unwrap();
    let cb=*b.triangle_m.iter().find(|p|!common.contains(p)).unwrap();
    let q=GaussRule::new(order)?; let h=a.z_m[1]-a.z_m[0];
    let area_a=a.volume_m3/h; let area_b=b.volume_m3/h; let mut total=0.;
    for (&t,&wt) in q.nodes().iter().zip(q.weights()) { for (&r0,&wr) in q.nodes().iter().zip(q.weights()) {
        if start.elapsed()>=limit {return Err(error("DEADLINE","shared edge quadrature deadline exceeded"));}
        let r=r0*r0; let jac=2.*r0*r*r;
        for (&x,&wx) in q.nodes().iter().zip(q.weights()) { for (&y,&wy) in q.nodes().iter().zip(q.weights()) {
            for sector in 0..3 { let (u,v,w)=match sector {0=>(r,r*x,r*y),1=>(r*x,r,r*y),_=>(r*x,r*y,r)};
                for side in 0..2 { let (qv,qjac)=if side==0 {(t*(1.-w),t)}else{(t+(1.-t)*w,1.-t)};
                    let p=std::array::from_fn::<_,2,_>(|k|(1.-u)*((1.-t)*common[0][k]+t*common[1][k])+u*ca[k]);
                    let z=std::array::from_fn::<_,2,_>(|k|(1.-v)*((1.-qv)*common[0][k]+qv*common[1][k])+v*cb[k]);
                    let rho=(p[0]-z[0]).hypot(p[1]-z[1]);
                    total+=wt*wr*wx*wy*jac*qjac*(1.-u)*(1.-v)*axial_magnetic::axial_kernel(rho,[0.,h],[0.,h])?;
                }
            }
        }}
    }}
    let value=4.*area_a*area_b*total;
    if !value.is_finite() || value<=0. {return Err(error("PLANAR_RANGE","invalid shared edge integral"));} Ok(Some(value))
}

// Analytic z/radial integration avoids resolving thin copper thickness with
// the outer z rule. If cancellation invalidates this primitive, use the
// independent finite-prism kernel; neither route relaxes the error threshold.
fn pair_integral(a: &Prism, b: &Prism, order: usize, start: Instant, limit: Duration) -> Result<(f64,usize,f64)> {
    if a.triangle_m == b.triangle_m && a.z_m == b.z_m {
        let mut prev=self_integral(a,order,start,limit)?; let mut n=order;
        while n<64 {
            n+=4; let next=self_integral(a,n,start,limit)?;
            let relative=(next-prev).abs()/next.abs();
            if relative<=1e-8 { return Ok((next,n,relative)); }
            prev=next;
        }
        return Err(error("PLANAR_UNRESOLVED", "self quadrature did not converge by order 64"));
    }
    if let Some(mut previous)=rectangle_pair(a,b,order,start,limit)? {
        let mut n=order;
        while n<64 {
            n+=4; let next=rectangle_pair(a,b,n,start,limit)?.ok_or_else(||error("PLANAR_RANGE","rectangle classification changed"))?;
            let relative=(next-previous).abs()/next.abs();
            if relative<=1e-8 { return Ok((next,n,relative)); }
            previous=next;
        }
        return Err(error("PLANAR_UNRESOLVED","rectangle cross quadrature did not converge by order 64"));
    }
    if let Some(mut previous)=edge_integral(a,b,order,start,limit)? {
        let mut n=order;
        while n<64 {
            n+=4; let next=edge_integral(a,b,n,start,limit)?.ok_or_else(||error("PLANAR_RANGE","edge classification changed"))?;
            let relative=(next-previous).abs()/next.abs();
            if relative<=1e-8 { return Ok((next,n,relative)); }
            previous=next;
        }
        return Err(error("PLANAR_UNRESOLVED","shared edge Duffy quadrature did not converge"));
    }
    for axial in [true, false] {
        let evaluate = |n| -> Result<f64> { let q=GaussRule::new(n)?; if axial { axial_magnetic::constant_pair_guarded(a,b,&q,start,limit) } else { prism::volume_integral_guarded(a,b,&q,start,limit) } };
        let mut previous = match evaluate(order) { Ok(x) => x, Err(_) if axial && start.elapsed() < limit => continue, Err(e) => return Err(e) };
        let mut n=order;
        while n<64 {
            if start.elapsed() >= limit { return Err(error("DEADLINE", "planar Gram deadline exceeded")); }
            n+=4;
            let next = match evaluate(n) { Ok(x)=>x, Err(_) if axial && start.elapsed() < limit => break, Err(e)=>return Err(e) };
            if start.elapsed() >= limit { return Err(error("DEADLINE", "planar Gram deadline exceeded")); }
            let relative=(previous-next).abs()/next.abs();
            if !next.is_finite() || next<=0. || !relative.is_finite() { return Err(error("PLANAR_RANGE", "invalid prism integral")); }
            if relative <= 1e-8 { return Ok((next,n,relative)); }
            previous=next;
        }
    }
    Err(error("PLANAR_UNRESOLVED", "prism quadrature did not converge by order 64"))
}

// Convex separating-axis test; equality means boundary contact, not overlap.
fn triangle_interior_overlap(a: [[f64; 2]; 3], b: [[f64; 2]; 3]) -> bool {
    for tri in [a, b] {
        for k in 0..3 {
            let u = tri[k]; let v = tri[(k+1)%3];
            let axis = [-(v[1]-u[1]), v[0]-u[0]];
            let project = |p: [f64;2]| (p[0]-u[0])*axis[0] + (p[1]-u[1])*axis[1];
            let range = |t: [[f64;2];3]| t.into_iter().map(project).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo,hi),x| (lo.min(x),hi.max(x)));
            let (al,ah)=range(a); let (bl,bh)=range(b);
            if ah <= bl || bh <= al { return false; }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triangle_overlap_distinguishes_contact_and_interior() {
        let a = [[0.,0.],[1.,0.],[0.,1.]];
        let adjacent = [[1.,1.],[0.,1.],[1.,0.]];
        let overlap = [[0.1,0.1],[0.8,0.1],[0.1,0.8]];
        assert!(!triangle_interior_overlap(a, adjacent));
        assert!(triangle_interior_overlap(a, overlap));
        assert!(triangle_interior_overlap(a, a));
    }
    fn budget() -> AllocationBudget { AllocationBudget { max_bytes: 4*1024*1024, max_nodes: 32, max_tetrahedra: 32 } }
    fn support() -> Support { Support { prism: Prism::new("a".into(), [[0.,0.],[1.,0.],[0.,1.]], [0.,1.]).unwrap(), charge_density: vec![1.,0.], current_density: vec![[1.,0.,0.]] } }
    #[test]
    fn bounded_invalid_and_deadline_controls() {
        assert!(gram(&[support()], 2, 1, 8, 0., budget(), Duration::from_secs(1)).is_err());
        assert!(gram(&[support()], 2, 1, 8, 1., budget(), Duration::ZERO).is_err());
        assert!(gram(&[support(), support()], 2, 1, 8, 1., budget(), Duration::from_secs(1)).is_err());
    }
    #[test]
    fn empty_support_different_mode_dimensions() {
        let mut s = support(); s.charge_density.fill(0.); s.current_density.fill([0.;3]);
        let g = gram(&[s], 2, 1, 8, 1., budget(), Duration::from_secs(1)).unwrap();
        assert_eq!(g.potential_per_farad, vec![vec![0.;2];2]);
        assert_eq!(g.inductance_h, vec![vec![0.]]);
    }

    #[test]
    fn self_gram_is_symmetric_with_distinct_dimensions() {
        let mut s = support(); s.charge_density = vec![1., 2.];
        let g = gram(&[s], 2, 1, 32, 1., budget(), Duration::from_secs(30)).unwrap();
        assert!(g.potential_per_farad[0][0] > 0.);
        assert!(g.inductance_h[0][0] > 0.);
        assert!((g.potential_per_farad[0][1] - g.potential_per_farad[1][0]).abs() <= 1e-12 * g.potential_per_farad[0][1].abs());
    }

    #[test]
    fn sparse_validation_and_empty_gram() {
        let mut s = SparseSupport { prism: support().prism, charge_density: vec![], current_density: vec![] };
        let g = gram_sparse(std::slice::from_ref(&s), 2, 1, 8, 1., budget(), Duration::from_secs(1)).unwrap();
        assert_eq!(g.max_order, 0);
        assert_eq!(g.potential_per_farad, vec![vec![0.;2];2]);
        s.charge_density = vec![(1,1.),(1,2.)];
        assert!(gram_sparse(&[s], 2, 1, 8, 1., budget(), Duration::from_secs(1)).is_err());
    }

    #[test]
    fn guarded_kernel_deadline_and_equivalence() {
        let p = support().prism; let q = GaussRule::new(8).unwrap();
        let start = Instant::now();
        assert!(prism::volume_integral_guarded(&p,&p,&q,start,Duration::ZERO).is_err());
        assert!(axial_magnetic::constant_pair_guarded(&p,&p,&q,start,Duration::ZERO).is_err());
        let a = axial_magnetic::constant_pair(&p,&p,&q).unwrap();
        let b = axial_magnetic::constant_pair_guarded(&p,&p,&q,Instant::now(),Duration::from_secs(5)).unwrap();
        assert_eq!(a.to_bits(), b.to_bits());
        let a = prism::volume_integral(&p,&p,&q).unwrap();
        let b = prism::volume_integral_guarded(&p,&p,&q,Instant::now(),Duration::from_secs(5)).unwrap();
        assert_eq!(a.to_bits(), b.to_bits());
    }

    #[test]
    fn self_orders_diagnostic() {
        let p=support().prism;
        for n in [8,12,16,24,32,40,48,56,64] {
            let x=self_integral(&p,n,Instant::now(),Duration::from_secs(10));
            println!("self order={n} value={x:?}");
        }
    }

    fn prism_tets(p: &Prism) -> [[[f64;3];4];3] {
        let a=p.triangle_m.map(|x|[x[0],x[1],p.z_m[0]]);
        let b=p.triangle_m.map(|x|[x[0],x[1],p.z_m[1]]);
        [[a[0],a[1],a[2],b[2]], [a[0],a[1],b[1],b[2]], [a[0],b[0],b[1],b[2]]]
    }
    #[test]
    fn self_matches_independent_tetra_reference() {
        use super::super::polyhedron::{self,PointPotential};
        let p=Prism::new("thin".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,0.035]).unwrap();
        let (x,_,_)=pair_integral(&p,&p,8,Instant::now(),Duration::from_secs(30)).unwrap();
        let t=prism_tets(&p); let mut reference=0.;
        for i in 0..3 { for j in 0..=i {
            let v=if i==j { polyhedron::tetra_self_integral(t[i],12).unwrap() }
                else { polyhedron::tetra_integral_adaptive(t[i],&PointPotential::tetrahedron(t[j]).unwrap(),12,1e-10).unwrap() };
            reference+=if i==j {v} else {2.*v};
        }}
        println!("self thin planar={x:e} tetra={reference:e} relative={:e}",(x-reference).abs()/reference);
        assert!((x-reference).abs() < 1e-5*reference);
    }
    #[test]
    fn touching_thin_prisms_positive() {
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[1.,1.],[0.,1.],[1.,0.]],[0.,0.035]).unwrap();
        let (x,n,e)=pair_integral(&a,&b,8,Instant::now(),Duration::from_secs(30)).unwrap();
        println!("thin shared edge {x:e} order={n} estimator={e:e}");
        assert!(x>0. && e<=1e-8);
    }

    #[test]
    fn touching_thin_matches_tetra_reference() {
        use super::super::polyhedron::{self,PointPotential};
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[1.,1.],[0.,1.],[1.,0.]],[0.,0.035]).unwrap();
        let (x,_,_)=pair_integral(&a,&b,8,Instant::now(),Duration::from_secs(30)).unwrap();
        let mut reference=0.; for ta in prism_tets(&a) { for tb in prism_tets(&b) {
            reference+=polyhedron::tetra_integral_adaptive(ta,&PointPotential::tetrahedron(tb).unwrap(),12,1e-10).unwrap();
        }}
        println!("near thin planar={x:e} tetra={reference:e} relative={:e}",(x-reference).abs()/reference);
        assert!((x-reference).abs()<1e-5*reference);
    }

    #[test]
    fn capacitive_only_zero_current_modes() {
        let mut s=support(); s.current_density.clear();
        let g=gram(&[s],2,0,8,1.,budget(),Duration::from_secs(1)).unwrap();
        assert!(g.inductance_h.is_empty()); assert_eq!(g.potential_per_farad.len(),2); assert!(g.potential_per_farad[0][0]>0.);
        let s=SparseSupport{prism:support().prism,charge_density:vec![],current_density:vec![]};
        let g=gram_sparse(&[s],2,0,8,1.,budget(),Duration::from_secs(1)).unwrap();
        assert!(g.inductance_h.is_empty());
    }

    #[test]
    fn arbitrary_shared_edge_positive() {
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.2,0.8]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[1.,0.],[0.,0.],[0.4,-0.6]],[0.,0.035]).unwrap();
        let (x,_,e)=pair_integral(&a,&b,8,Instant::now(),Duration::from_secs(30)).unwrap();
        assert!(x>0. && e<=1e-8);
    }

    #[test]
    fn arbitrary_edge_orders_diagnostic() {
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.2,0.8]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[1.,0.],[0.,0.],[0.4,-0.6]],[0.,0.035]).unwrap();
        for n in [8,12,16,20,24,28,32] { println!("edge order={n} value={:?}", edge_integral(&a,&b,n,Instant::now(),Duration::from_secs(15))); }
    }

    #[test]
    fn arbitrary_edge_matches_tetra_and_reciprocity() {
        use super::super::polyhedron::{self,PointPotential};
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.2,0.8]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[1.,0.],[0.,0.],[0.4,-0.6]],[0.,0.035]).unwrap();
        let start=Instant::now(); let limit=Duration::from_secs(30);
        let (x,_,_)=pair_integral(&a,&b,8,start,limit).unwrap();
        let (reverse,_,_)=pair_integral(&b,&a,8,start,limit).unwrap();
        let mut reference=0.;
        for ta in prism_tets(&a) { for tb in prism_tets(&b) {
            assert!(start.elapsed()<limit);
            reference+=polyhedron::tetra_integral_adaptive(ta,&PointPotential::tetrahedron(tb).unwrap(),16,1e-10).unwrap();
        }}
        println!("edge planar={x:e} reverse={reverse:e} tetra={reference:e} relative={:e}",(x-reference).abs()/reference);
        assert!((x-reverse).abs()<1e-8*x);
        assert!((x-reference).abs()<1e-5*reference);
    }

}
