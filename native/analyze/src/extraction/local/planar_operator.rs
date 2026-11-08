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
    aligned_integral(p,p,order,start,limit)
}
// The same XY autocorrelation applies to distinct vertical intervals; the
// analytic axial kernel carries their separation and individual thicknesses.
fn aligned_integral(p: &Prism, other: &Prism, order: usize, start: Instant, limit: Duration) -> Result<f64> {
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
            let axial=axial_magnetic::axial_kernel(rho,[0.,h],[other.z_m[0]-p.z_m[0],other.z_m[1]-p.z_m[0]])?;
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

// Shared-edge barycentric difference coordinates: integrate the common
// edge position and the remaining radial coordinate before 2D quadrature.
fn edge_integral(a:&Prism,b:&Prism,order:usize,start:Instant,limit:Duration) -> Result<Option<f64>> {
    let common:Vec<_>=a.triangle_m.iter().copied().filter(|p|b.triangle_m.contains(p)).collect();
    if common.len()!=2 || triangle_interior_overlap(a.triangle_m,b.triangle_m) { return Ok(None); }
    let ca=*a.triangle_m.iter().find(|p|!common.contains(p)).unwrap();
    let cb=*b.triangle_m.iter().find(|p|!common.contains(p)).unwrap();
    let q=GaussRule::new(order)?;let ha=a.z_m[1]-a.z_m[0];let hb=b.z_m[1]-b.z_m[0];
    let e=[common[1][0]-common[0][0],common[1][1]-common[0][1]];
    let p=[ca[0]-common[0][0],ca[1]-common[0][1]];
    let z=[cb[0]-common[0][0],cb[1]-common[0][1]];
    let za=[0.,ha];let zb=[b.z_m[0]-a.z_m[0],b.z_m[1]-a.z_m[0]];
    let mut total=super::accurate::Sum::default();
    // In barycentric coordinates, integrate the common-edge position:
    // its available length is 1-max(v,u+d) for d>=0. Split at v=u+d,
    // scale by r=max(v,u+d), and use A/B symmetry for d<0.
    for (&x,&wx) in q.nodes().iter().zip(q.weights()) {
        if start.elapsed()>=limit {return Err(error("DEADLINE","shared edge quadrature deadline exceeded"));}
        for (&y,&wy) in q.nodes().iter().zip(q.weights()) {
            for (p,z) in [(p,z),(z,p)] {
                let direction=[(1.-x)*e[0]+x*p[0],(1.-x)*e[1]+x*p[1]];
                let first=(direction[0]-y*z[0]).hypot(direction[1]-y*z[1]);
                let second=(y*direction[0]-z[0]).hypot(y*direction[1]-z[1]);
                let value=radial_edge_axial(first,za,zb)?+y*radial_edge_axial(second,za,zb)?;
                total=total.add(super::accurate::Sum::from(wx*wy*value));
            }
        }
    }
    let value=4.*(a.volume_m3/ha)*(b.volume_m3/hb)*total.value();
    if !value.is_finite() || value<=0. {return Err(error("PLANAR_RANGE","invalid shared edge integral"));} Ok(Some(value))
}

// Analytic z/radial integration avoids resolving thin copper thickness with
// the outer z rule. If cancellation invalidates this primitive, use the
// independent finite-prism kernel; neither route relaxes the error threshold.
// Two triangles sharing only one vertex: split the radial (u,v) square
// into u>=v and v>=u, then u=r,v=r*s (or its transpose). The complete
// XY Jacobian is 4*A*B*r^3*s. Both r and z are integrated analytically.
fn radial_cubic_axial(distance:f64,a:[f64;2],b:[f64;2])->Result<f64> {
    if !distance.is_finite() || distance<=0. {return Err(error("AXIAL_INPUT","positive radial distance required"));}
    // Integration by parts: int_0^1 r^3 F(s,r*d) dr =
    // (s*asinh(s/d)-hypot(s,d) + int_0^1 r^3*hypot(s,r*d)dr)/4.
    // Factor the latter primitive through hypot(s,d)-abs(s) to avoid
    // subtracting nearly equal fifth powers.
    let primitive=|s:f64| {
        let x=s.abs();let h=x.hypot(distance);let denominator=h+x;
        let t=x/denominator;let u=(distance/denominator).powi(2);
        let moment=denominator*(t.powi(3)+(5./3.)*t*t*u+t*u*u+u.powi(3)/5.);
        (s*(s/distance).asinh()-h+moment)/4.
    };
    let terms=[primitive(a[1]-b[0]),primitive(a[0]-b[1]),-primitive(a[1]-b[1]),-primitive(a[0]-b[0])];
    let sum=terms.iter().fold(super::accurate::Sum::default(),|s,&v|s.add(super::accurate::Sum::from(v))).value();
    let scale=terms.iter().map(|v|v.abs()).sum::<f64>();
    if !sum.is_finite() || sum<=0. || sum<=1e-10*scale {return Err(error("AXIAL_UNRESOLVED","radial endpoint cancellation"));}
    Ok(sum)
}
// Integral_0^1 r^2(1-r)*K_z(r*d) dr. Integration by parts reduces it
// to the second/third moments of hypot(s,r*d), with stable small-d series.
fn radial_edge_axial(distance:f64,a:[f64;2],b:[f64;2])->Result<f64> {
    if !distance.is_finite() || distance<=0. {return Err(error("AXIAL_INPUT","positive edge radial distance required"));}
    let primitive=|s:f64| {
        let x=s.abs();let h=x.hypot(distance);let denominator=h+x;
        let t=x/denominator;let u=(distance/denominator).powi(2);
        let j3=denominator*(t.powi(3)+(5./3.)*t*t*u+t*u*u+u.powi(3)/5.);
        let j2=if x==0. {distance/4.} else if distance<0.125*x {
            let ratio=(distance/x).powi(2);let mut coefficient=1.;let mut power=1.;let mut sum=1./3.;
            for k in 1..=12 {coefficient*=(1.5-k as f64)/k as f64;power*=ratio;sum+=coefficient*power/(2*k+3) as f64;}
            x*sum
        }else{
            let ratio=x/distance;
            (h*(2.+ratio*ratio)-x*ratio.powi(3)*(distance/x).asinh())/8.
        };
        (s*(s/distance).asinh()-h+4.*j2-3.*j3)/12.
    };
    let terms=[primitive(a[1]-b[0]),primitive(a[0]-b[1]),-primitive(a[1]-b[1]),-primitive(a[0]-b[0])];
    let value=terms.iter().fold(super::accurate::Sum::default(),|s,&v|s.add(super::accurate::Sum::from(v))).value();
    let scale=terms.iter().map(|v|v.abs()).sum::<f64>();
    if !value.is_finite() || value<=0. || value<=1e-10*scale {return Err(error("AXIAL_UNRESOLVED","edge radial endpoint cancellation"));}
    Ok(value)
}
fn vertex_integral(a:&Prism,b:&Prism,order:usize,start:Instant,limit:Duration)->Result<Option<f64>> {
    let common:Vec<_>=a.triangle_m.iter().copied().filter(|p|b.triangle_m.contains(p)).collect();
    if common.len()!=1 || triangle_interior_overlap(a.triangle_m,b.triangle_m) {return Ok(None);}
    let c=common[0];
    let edges=|p:&Prism|p.triangle_m.iter().filter(|&&v|v!=c).map(|v|[v[0]-c[0],v[1]-c[1]]).collect::<Vec<_>>();
    let ea=edges(a);let eb=edges(b);let rule=GaussRule::new(order)?;
    let area_a=a.volume_m3/(a.z_m[1]-a.z_m[0]);let area_b=b.volume_m3/(b.z_m[1]-b.z_m[0]);
    let za=[0.,a.z_m[1]-a.z_m[0]];let zb=[b.z_m[0]-a.z_m[0],b.z_m[1]-a.z_m[0]];
    let mut total=super::accurate::Sum::default();
    for sector in 0..2 {for (&s,&ws) in rule.nodes().iter().zip(rule.weights()) {
            for (&u,&wu) in rule.nodes().iter().zip(rule.weights()) {
                if start.elapsed()>=limit {return Err(error("DEADLINE","shared vertex quadrature deadline exceeded"));}
                let da=[(1.-u)*ea[0][0]+u*ea[1][0],(1.-u)*ea[0][1]+u*ea[1][1]];
                for (&v,&wv) in rule.nodes().iter().zip(rule.weights()) {
                    let db=[(1.-v)*eb[0][0]+v*eb[1][0],(1.-v)*eb[0][1]+v*eb[1][1]];
                    let delta=if sector==0 {[da[0]-s*db[0],da[1]-s*db[1]]} else {[s*da[0]-db[0],s*da[1]-db[1]]};
                    let axial=radial_cubic_axial(delta[0].hypot(delta[1]),za,zb)?;
                    total=total.add(super::accurate::Sum::from(ws*wu*wv*s*axial));
                }
            }
    }}
    let value=4.*area_a*area_b*total.value();
    if !value.is_finite() || value<=0. {return Err(error("PLANAR_RANGE","invalid shared vertex integral"));}
    Ok(Some(value))
}

pub(super) fn pair_integral(a: &Prism, b: &Prism, order: usize, start: Instant, limit: Duration) -> Result<(f64,usize,f64)> {
    let canonical=|mut t:[[f64;2];3]|{t.sort_by(|a,b|a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));t};
    if canonical(a.triangle_m)==canonical(b.triangle_m) {
        let mut prev=aligned_integral(a,b,order,start,limit)?; let mut n=order;
        while n<64 {
            n+=4; let next=aligned_integral(a,b,n,start,limit)?;
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
    if let Some(mut previous)=vertex_integral(a,b,order,start,limit)? {
        let mut n=order;
        while n<64 {
            n+=4;let next=vertex_integral(a,b,n,start,limit)?.ok_or_else(||error("PLANAR_RANGE","vertex classification changed"))?;
            let relative=(next-previous).abs()/next.abs();
            if relative<=1e-8 {return Ok((next,n,relative));}
            previous=next;
        }
        return Err(error("PLANAR_UNRESOLVED","shared vertex Duffy quadrature did not converge"));
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
    fn aligned_unequal_slabs_match_tetra_and_reciprocity() {
        use super::super::polyhedron::{self,PointPotential};
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[1.,0.],[0.,1.],[0.,0.]],[0.075,0.125]).unwrap();
        let start=Instant::now();let wall=Duration::from_secs(30);
        let (x,_,_)=pair_integral(&a,&b,8,start,wall).unwrap();
        let (reverse,_,_)=pair_integral(&b,&a,8,start,wall).unwrap();
        assert!((x-reverse).abs()<=1e-8*x);
        let mut reference=0.;
        for ta in prism_tets(&a){for tb in prism_tets(&b){
            assert!(start.elapsed()<wall);
            reference+=polyhedron::tetra_integral_adaptive(ta,&PointPotential::tetrahedron(tb).unwrap(),16,1e-10).unwrap();
        }}
        println!("aligned={x:e} tetra={reference:e}");
        assert!((x-reference).abs()<1e-5*reference);
    }

    #[test]
    fn cubic_radial_primitive_matches_independent_outer_rule() {
        let q=GaussRule::new(64).unwrap();
        for distance in [0.1,1.,10.] {for b in [[0.,0.035],[0.075,0.125]] {
            let a=[0.,0.035];let exact=radial_cubic_axial(distance,a,b).unwrap();
            let reference=q.nodes().iter().zip(q.weights()).map(|(&r,&w)|w*r.powi(3)*axial_magnetic::axial_kernel(r*distance,a,b).unwrap()).sum::<f64>();
            assert!((exact-reference).abs()<1e-8*reference,"{distance} {b:?}: {exact:e} {reference:e}");
            let edge=radial_edge_axial(distance,a,b).unwrap();
            // r=t^2 resolves the endpoint logarithm in this independent
            // numerical reference; confirm its own order sensitivity first.
            let outer=|rule:&GaussRule|rule.nodes().iter().zip(rule.weights()).map(|(&t,&w)|{
                let r=t*t;2.*t*w*r*r*(1.-r)*axial_magnetic::axial_kernel(r*distance,a,b).unwrap()
            }).sum::<f64>();
            let edge_reference=outer(&q);
            assert!((outer(&GaussRule::new(32).unwrap())-edge_reference).abs()<1e-9*edge_reference);
            assert!((edge-edge_reference).abs()<1e-8*edge_reference,"edge {distance} {b:?}: {edge:e} {edge_reference:e}");
        }}
    }

    #[test]
    fn skinny_vertex_pair_matches_high_order_axial_rule() {
        let a=Prism::new("a".into(),[[0.,0.],[0.2,0.],[0.,0.1]],[0.,0.01]).unwrap();
        let b=Prism::new("b".into(),[[0.2,0.],[4.85,0.05],[0.2,0.1]],[0.,0.01]).unwrap();
        let start=Instant::now();let wall=Duration::from_secs(30);
        let (value,order,error)=pair_integral(&a,&b,8,start,wall).unwrap();
        let reference=axial_magnetic::constant_pair_guarded(&a,&b,&GaussRule::new(56).unwrap(),start,wall).unwrap();
        let reverse=pair_integral(&b,&a,8,start,wall).unwrap().0;
        println!("vertex order={order} estimator={error:e} relative={:e}",(value-reference).abs()/reference);
        assert!((value-reference).abs()<1e-7*reference);
        assert!((value-reverse).abs()<1e-8*value);
    }

    #[test]
    fn shared_vertex_pair_matches_tetra_reference() {
        use super::super::polyhedron::{self,PointPotential};
        let a=Prism::new("a".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,0.035]).unwrap();
        let b=Prism::new("b".into(),[[0.,0.],[-1.,0.],[0.,-0.5]],[0.,0.035]).unwrap();
        let start=Instant::now();let wall=Duration::from_secs(30);
        let value=pair_integral(&a,&b,8,start,wall).unwrap().0;
        let mut reference=0.;
        for ta in prism_tets(&a){for tb in prism_tets(&b){
            assert!(start.elapsed()<wall);
            reference+=polyhedron::tetra_integral_adaptive(ta,&PointPotential::tetrahedron(tb).unwrap(),16,1e-10).unwrap();
        }}
        assert!((value-reference).abs()<1e-5*reference);
    }

    #[test]
    fn skinny_shared_edge_pair_retains_gate_and_reciprocity() {
        let a=Prism::new("a".into(),[[0.2,0.1],[0.,0.1],[0.2,0.]],[0.,0.01]).unwrap();
        let b=Prism::new("b".into(),[[0.2,0.],[4.85,0.05],[0.2,0.1]],[0.,0.01]).unwrap();
        let start=Instant::now();let wall=Duration::from_secs(30);
        let (v,n,e)=pair_integral(&a,&b,8,start,wall).unwrap();
        let reverse=pair_integral(&b,&a,8,start,wall).unwrap().0;
        println!("skinny edge order={n} estimator={e:e}");
        assert!(e<=1e-8 && (v-reverse).abs()<=1e-8*v);
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
