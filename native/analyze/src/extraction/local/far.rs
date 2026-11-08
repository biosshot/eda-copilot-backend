//! Bounded per-pair degree-four Legendre expansion, without spatial hierarchy.
//! The geometric remainder bounds the scalar integral; it is not an energy norm.
use super::{
    error,
    prism::{GaussRule, Prism},
    Result,
};

#[derive(Clone, Debug)]
pub struct Moments {
    pub center_m: [f64; 3],
    /// Convex prism lies in the ball because all six vertices lie in it.
    pub radius_m: f64,
    pub volume_m3: f64,
    second: [[f64; 3]; 3],
    third: [[[f64; 3]; 3]; 3],
    fourth: [[[[f64; 3]; 3]; 3]; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct FarIntegral {
    pub integral_m5: f64,
    pub absolute_remainder_m5: f64,
    pub radius_ratio: f64,
}

impl Moments {
    /// Exact degree-four polynomial moments of one finite tetrahedron.
    /// Positive Duffy weights integrate degrees <=6/5/4 in u/v/w, so a
    /// four-point rule in each coordinate is sufficient; no source sampling
    /// approximation is introduced into the existing geometric tail bound.
    pub fn tetrahedron(vertices: [[f64;3];4]) -> Result<Self> {
        super::polyhedron::PointPotential::tetrahedron(vertices)?;
        let edges: [[f64;3];3]=std::array::from_fn(|i|std::array::from_fn(|d|vertices[i+1][d]-vertices[0][d]));
        let cross: [f64;3]=std::array::from_fn(|d| {
            let i=(d+1)%3;let j=(d+2)%3;
            super::accurate::Sum::from(edges[1][i]).mul(edges[2][j]).sub(super::accurate::Sum::from(edges[1][j]).mul(edges[2][i])).value()
        });
        let volume=edges[0].iter().zip(cross).fold(super::accurate::Sum::default(),|s,(&x,y)|s.add(super::accurate::Sum::from(x).mul(y))).value().abs()/6.;
        let center=std::array::from_fn(|d|vertices[0][d]+vertices.iter().skip(1).map(|p|p[d]-vertices[0][d]).sum::<f64>()/4.);
        let mut out=Self{center_m:center,radius_m:0.,volume_m3:volume,second:[[0.;3];3],third:[[[0.;3];3];3],fourth:[[[[0.;3];3];3];3]};
        for p in vertices {out.radius_m=out.radius_m.max((p[0]-center[0]).hypot(p[1]-center[1]).hypot(p[2]-center[2]));}
        out.radius_m*=1.+8.*f64::EPSILON;
        let q=GaussRule::new(4)?;
        for (&u,&wu) in q.nodes().iter().zip(q.weights()) {for (&v,&wv) in q.nodes().iter().zip(q.weights()) {for (&w,&ww) in q.nodes().iter().zip(q.weights()) {
            let bary=[1.-u,u*(1.-v),u*v*(1.-w),u*v*w];
            let p: [f64;3]=std::array::from_fn(|d|(0..4).map(|i|bary[i]*(vertices[i][d]-center[d])).sum());
            let weight=6.*wu*wv*ww*u*u*v;
            for i in 0..3 {for j in 0..3 {
                out.second[i][j]+=weight*p[i]*p[j];
                for k in 0..3 {out.third[i][j][k]+=weight*p[i]*p[j]*p[k];
                    for l in 0..3 {out.fourth[i][j][k][l]+=weight*p[i]*p[j]*p[k]*p[l];}
                }
            }}
        }}}
        if !volume.is_finite() || volume<=0. || !out.radius_m.is_finite() || out.radius_m<=0. ||
           out.fourth.iter().flatten().flatten().flatten().any(|x|!x.is_finite()) {
            return Err(error("FAR_INPUT","invalid finite tetra moments"));
        }
        Ok(out)
    }
    pub fn new(prism: &Prism) -> Result<Self> {
        let [a, b, c] = prism.triangle_m;
        let center = [
            (a[0] + b[0] + c[0]) / 3.,
            (a[1] + b[1] + c[1]) / 3.,
            (prism.z_m[0] + prism.z_m[1]) / 2.,
        ];
        let mut out = Self {
            center_m: center,
            radius_m: 0.,
            volume_m3: prism.volume_m3,
            second: [[0.; 3]; 3],
            third: [[[0.; 3]; 3]; 3],
            fourth: [[[[0.; 3]; 3]; 3]; 3],
        };
        for p in prism.triangle_m {
            for z in prism.z_m {
                let d = (p[0] - center[0])
                    .hypot(p[1] - center[1])
                    .hypot(z - center[2]);
                out.radius_m = out.radius_m.max(d);
            }
        }
        // Conservative F64 guard around the convex-hull radius computation.
        out.radius_m *= 1. + 8. * f64::EPSILON;
        let rule = GaussRule::new(3)?; // Degree-four moments incl. triangle Jacobian degree five.
        for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
            for (&v, &wv) in rule.nodes.iter().zip(&rule.weights) {
                for (&z, &wz) in rule.nodes.iter().zip(&rule.weights) {
                    let p = [
                        a[0] + u * (b[0] - a[0]) + (1. - u) * v * (c[0] - a[0]) - center[0],
                        a[1] + u * (b[1] - a[1]) + (1. - u) * v * (c[1] - a[1]) - center[1],
                        prism.z_m[0] + z * (prism.z_m[1] - prism.z_m[0]) - center[2],
                    ];
                    let w = 2. * wu * wv * wz * (1. - u);
                    for i in 0..3 {
                        for j in 0..3 {
                            out.second[i][j] += w * p[i] * p[j];
                            for k in 0..3 {
                                out.third[i][j][k] += w * p[i] * p[j] * p[k];
                                for l in 0..3 {
                                    out.fourth[i][j][k][l] += w * p[i] * p[j] * p[k] * p[l];
                                }
                            }
                        }
                    }
                }
            }
        }
        if !out.radius_m.is_finite()
            || out.radius_m <= 0.
            || !out
                .fourth
                .iter()
                .flatten()
                .flatten()
                .flatten()
                .all(|x| x.is_finite())
        {
            return Err(error(
                "MOMENT_RANGE",
                "finite prism moment/radius range exceeded",
            ));
        }
        Ok(out)
    }

    /// Caller evaluates a pair once and mirrors this single returned value.
    pub fn pair(&self, other: &Self) -> Result<FarIntegral> {
        let r = std::array::from_fn::<_, 3, _>(|i| other.center_m[i] - self.center_m[i]);
        let distance = r[0].hypot(r[1]).hypot(r[2]);
        let q = (self.radius_m + other.radius_m) / distance * (1. + 32. * f64::EPSILON);
        if !q.is_finite() || q >= 1. {
            return Err(error(
                "FAR_SEPARATION",
                "prism bounding balls not strictly separated",
            ));
        }
        // Work with unit direction and normalized central moments to avoid high
        // powers of dimensional distances overflowing/underflowing needlessly.
        let e = r.map(|x| x / distance);
        let inv = 1. / distance;
        let mut m2 = [[0.; 3]; 3];
        let mut m3 = [[[0.; 3]; 3]; 3];
        let mut m4 = [[[[0.; 3]; 3]; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                m2[i][j] = (self.second[i][j] + other.second[i][j]) * inv.powi(2);
                for k in 0..3 {
                    m3[i][j][k] = (other.third[i][j][k] - self.third[i][j][k]) * inv.powi(3);
                    for l in 0..3 {
                        let cross = self.second[i][j] * other.second[k][l]
                            + self.second[i][k] * other.second[j][l]
                            + self.second[i][l] * other.second[j][k]
                            + other.second[i][j] * self.second[k][l]
                            + other.second[i][k] * self.second[j][l]
                            + other.second[i][l] * self.second[j][k];
                        m4[i][j][k][l] =
                            (self.fourth[i][j][k][l] + other.fourth[i][j][k][l] + cross)
                                * inv.powi(4);
                    }
                }
            }
        }
        let (mut proj2, mut trace2, mut proj3, mut mixed3, mut proj4, mut mixed4, mut trace4) =
            (0., 0., 0., 0., 0., 0., 0.);
        for i in 0..3 {
            trace2 += m2[i][i];
            for j in 0..3 {
                proj2 += e[i] * e[j] * m2[i][j];
                mixed3 += e[i] * m3[i][j][j];
                trace4 += m4[i][i][j][j];
                for k in 0..3 {
                    proj3 += e[i] * e[j] * e[k] * m3[i][j][k];
                    mixed4 += e[i] * e[j] * m4[i][j][k][k];
                    for l in 0..3 {
                        proj4 += e[i] * e[j] * e[k] * e[l] * m4[i][j][k][l];
                    }
                }
            }
        }
        let expansion = 1.
            + (3. * proj2 - trace2) / 2.
            + (-5. * proj3 + 3. * mixed3) / 2.
            + (35. * proj4 - 30. * mixed4 + 3. * trace4) / 8.;
        let scale = self.volume_m3 * other.volume_m3 * inv;
        let integral = scale * expansion;
        let remainder = scale * q.powi(5) / (1. - q);
        if !integral.is_finite() || integral <= 0. || !remainder.is_finite() {
            return Err(error(
                "FAR_RANGE",
                "integral/remainder outside positive finite F64 range",
            ));
        }
        Ok(FarIntegral {
            integral_m5: integral,
            absolute_remainder_m5: remainder,
            radius_ratio: q,
        })
    }
}

#[cfg(test)]
mod tetra_tests {
    use super::*;
    fn unit() -> [[f64;3];4] {[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]}
    #[test]
    fn tetra_moments_match_exact_simplex_integrals() {
        let m=Moments::tetrahedron(unit()).unwrap();
        assert_eq!(m.center_m,[0.25;3]);
        assert!((m.volume_m3-1./6.).abs()<1e-15);
        for i in 0..3 {
            assert!((m.second[i][i]-3./80.).abs()<1e-14);
            assert!((m.third[i][i][i]-1./160.).abs()<1e-14);
            assert!((m.fourth[i][i][i][i]-39./8960.).abs()<1e-14);
            for j in 0..3 {if i!=j {assert!((m.second[i][j]+1./80.).abs()<1e-14);}}
        }
        let mut swapped=unit();swapped.swap(0,1);
        let other=Moments::tetrahedron(swapped).unwrap();
        assert!((other.second[0][0]-m.second[0][0]).abs()<1e-14);
        assert!(Moments::tetrahedron([[0.;3];4]).is_err());
    }
    #[test]
    fn tetra_far_bound_covers_independent_finite_integral() {
        let a=unit();let b=a.map(|p|[p[0]+10.,p[1],p[2]]);
        let x=Moments::tetrahedron(a).unwrap().pair(&Moments::tetrahedron(b).unwrap()).unwrap();
        let source=super::super::polyhedron::PointPotential::tetrahedron(b).unwrap().with_tighter_tetra_tolerance(1e-12).unwrap();
        let reference=super::super::polyhedron::tetra_integral_adaptive(a,&source,16,1e-10).unwrap();
        assert!((x.integral_m5-reference).abs()<=x.absolute_remainder_m5);
    }
}
