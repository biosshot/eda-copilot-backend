//! Experimental through-thickness current basis. No impedance is stamped here.
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default)]
struct C {
    re: f64,
    im: f64,
}
impl C {
    fn add(self, b: Self) -> Self {
        Self {
            re: self.re + b.re,
            im: self.im + b.im,
        }
    }
    fn sub(self, b: Self) -> Self {
        Self {
            re: self.re - b.re,
            im: self.im - b.im,
        }
    }
    fn mul(self, b: Self) -> Self {
        Self {
            re: self.re * b.re - self.im * b.im,
            im: self.re * b.im + self.im * b.re,
        }
    }
    fn scale(self, x: f64) -> Self {
        Self {
            re: self.re * x,
            im: self.im * x,
        }
    }
    fn div(self, b: Self) -> Self {
        let d = b.re * b.re + b.im * b.im;
        Self {
            re: (self.re * b.re + self.im * b.im) / d,
            im: (self.im * b.re - self.re * b.im) / d,
        }
    }
    fn exp(self) -> Self {
        let a = self.re.exp();
        Self {
            re: a * self.im.cos(),
            im: a * self.im.sin(),
        }
    }
    fn expm1(self) -> Self {
        if self.re.abs() + self.im.abs() < 0.01 {
            let mut term = self;
            let mut sum = self;
            for n in 2..=18 {
                term = term.mul(self).scale(1. / n as f64);
                sum = sum.add(term);
            }
            sum
        } else {
            self.exp().sub(Self { re: 1., im: 0. })
        }
    }
}

fn cell_average(t: f64, a: f64, b: f64, inv: f64) -> (C, C) {
    if inv == 0. || t * inv < 1e-70 {
        return (
            C { re: 1. / t, im: 0. },
            C {
                re: 4. * (a + b - t) / (t * t),
                im: 0.,
            },
        );
    }
    let k = C { re: inv, im: inv };
    let h = b - a;
    let step = k.scale(-h).expm1().scale(-1.);
    let left = k.scale(-a).exp().mul(step);
    let right = k.scale(-(t - b)).exp().mul(step);
    let den = k.scale(-t).expm1().scale(-1.);
    let half = k.scale(-t / 2.).expm1().scale(-1.);
    let d = a + b - t;
    let diff = if d <= 0. {
        left.mul(k.scale(d).expm1())
    } else {
        right.mul(k.scale(-d).expm1()).scale(-1.)
    };
    (
        left.add(right).div(den.scale(2.)).scale(1. / h),
        diff.div(half.mul(half)).scale(1. / h),
    )
}

#[derive(Debug, Clone)]
pub(super) struct Basis {
    /// Cell-average real current densities, normalized per metre of lateral width.
    pub modes: Vec<Vec<f64>>,
    /// Integral over thickness for each mode; zero for circulating modes.
    pub transport: Vec<f64>,
    /// Integral J_i J_j / sigma dz; multiply by the geometric lateral factor once.
    pub resistance_gram: Vec<Vec<f64>>,
    pub skin_depth_m: f64,
    pub max_cell_over_skin_depth: f64,
    pub thickness_screen_passes: bool,
    /// Profiling and orthogonality do not prove lateral proximity or physical accuracy.
    pub applicability: &'static str,
}

/// `z_m` are strictly increasing boundaries from zero to thickness; all units SI.
/// Produces a weighted-orthonormal real basis, with transport coefficients retained.
pub(super) fn build(
    z_m: &[f64],
    frequency_hz: f64,
    conductivity_s_m: f64,
    permeability_h_m: f64,
) -> Result<Basis, &'static str> {
    if z_m.len() < 2
        || z_m[0] != 0.
        || z_m.iter().any(|x| !x.is_finite())
        || z_m.windows(2).any(|w| w[1] <= w[0])
        || !frequency_hz.is_finite()
        || frequency_hz < 0.
        || !conductivity_s_m.is_finite()
        || conductivity_s_m <= 0.
        || !permeability_h_m.is_finite()
        || permeability_h_m <= 0.
    {
        return Err("invalid profile geometry or material");
    }
    let t = *z_m.last().unwrap();
    let inv = if frequency_hz == 0. {
        0.
    } else {
        (PI * frequency_hz).sqrt() * conductivity_s_m.sqrt() * permeability_h_m.sqrt()
    };
    if !inv.is_finite() || (frequency_hz > 0. && inv <= 0.) || !(t * inv).is_finite() {
        return Err("unrepresentable skin depth");
    }
    let depth = if inv == 0. { f64::INFINITY } else { 1. / inv };
    let max_step = z_m.windows(2).map(|w| w[1] - w[0]).fold(0., f64::max);
    let mut candidates = vec![vec![1. / t; z_m.len() - 1]];
    if inv > 0. {
        let mut er = Vec::new();
        let mut ei = Vec::new();
        let mut or = Vec::new();
        let mut oi = Vec::new();
        for w in z_m.windows(2) {
            let (even, odd) = cell_average(t, w[0], w[1], inv);
            er.push(even.re);
            ei.push(even.im);
            or.push(odd.re);
            oi.push(odd.im);
        }
        candidates.extend([er, ei, or, oi]);
    }
    let dot = |a: &[f64], b: &[f64]| -> f64 {
        a.iter()
            .zip(b)
            .zip(z_m.windows(2))
            .map(|((&x, &y), w)| x * y * (w[1] - w[0]))
            .sum()
    };
    let mut modes: Vec<Vec<f64>> = Vec::new();
    for mut v in candidates {
        let initial = dot(&v, &v).sqrt();
        if !initial.is_finite() {
            return Err("profile out of range");
        }
        if initial == 0. {
            continue;
        }
        // Reorthogonalization controls cancellation between near-identical modes.
        for _ in 0..2 {
            for q in &modes {
                let projection = dot(&v, q);
                for (x, y) in v.iter_mut().zip(q) {
                    *x -= projection * y;
                }
            }
        }
        let norm = dot(&v, &v).sqrt();
        if norm <= initial * 1e-10 {
            continue;
        }
        if !norm.is_finite() || norm <= 0. {
            return Err("profile orthogonalization failed");
        }
        for x in &mut v {
            *x /= norm;
        }
        modes.push(v);
    }
    let transport: Vec<f64> = modes
        .iter()
        .map(|v| {
            v.iter()
                .zip(z_m.windows(2))
                .map(|(x, w)| x * (w[1] - w[0]))
                .sum()
        })
        .collect();
    let resistance_gram: Vec<Vec<f64>> = modes
        .iter()
        .map(|a| modes.iter().map(|b| dot(a, b) / conductivity_s_m).collect())
        .collect();
    if transport
        .iter()
        .chain(resistance_gram.iter().flatten())
        .any(|x| !x.is_finite())
        || (inv > 0. && (!depth.is_finite() || depth <= 0.))
    {
        return Err("profile coefficients outside finite range");
    }
    Ok(Basis { modes,transport,resistance_gram,skin_depth_m:depth,max_cell_over_skin_depth:max_step*inv,thickness_screen_passes:t*inv<=0.2,applicability:"unverified: requires independent thickness refinement, lateral proximity, interface and physical error checks" })
}

#[cfg(test)]
mod tests {
    use super::*;
    const MU: f64 = 4e-7 * PI;
    #[test]
    fn dc_uniform_and_invalid() {
        let b = build(&[0., 0.5e-3, 1e-3], 0., 58e6, MU).unwrap();
        assert_eq!(b.modes.len(), 1);
        assert!((b.transport[0] - 1e-3f64.sqrt()).abs() < 1e-14);
        assert!(build(&[0., 0.], 1., 58e6, MU).is_err());
        assert!(build(&[0., 1.], f64::NAN, 58e6, MU).is_err());
        assert!(build(&[0., 1.], 0., 1e-320, MU).is_err());
    }
    #[test]
    fn weighted_orthogonality_and_zero_transport() {
        let z: Vec<_> = (0..=64).map(|i| 35e-6 * i as f64 / 64.).collect();
        let b = build(&z, 1e8, 58e6, MU).unwrap();
        assert!(b.modes.len() >= 3);
        assert!(b.transport.iter().skip(1).all(|x| x.abs() < 1e-10));
        for (i, row) in b.resistance_gram.iter().enumerate() {
            for (j, x) in row.iter().enumerate() {
                let expected = if i == j { 1. / 58e6 } else { 0. };
                assert!((x - expected).abs() < 1e-15);
            }
        }
        assert!(b.max_cell_over_skin_depth < 1.);
        assert!(!b.thickness_screen_passes);
    }
    #[test]
    fn even_cell_averages_converge_to_slab_loss_and_odd_has_zero_transport() {
        let t = 35e-6;
        let sigma = 58e6;
        let f = 1e8;
        let inv = (PI * f * sigma * MU).sqrt();
        let k = C { re: inv, im: inv };
        let full = k.scale(-t).expm1().scale(-1.);
        let coth = C { re: 2., im: 0. }.sub(full).div(full);
        let exact = k.mul(coth).re / (2. * sigma);
        let evaluate = |n: usize| {
            let h = t / n as f64;
            let z: Vec<_> = (0..=n).map(|i| t * i as f64 / n as f64).collect();
            let basis = build(&z, f, sigma, MU).unwrap();
            let mut coefficients = vec![C::default(); basis.modes.len()];
            let mut loss = 0.;
            let mut even_sum = C::default();
            let mut odd_sum = C::default();
            for i in 0..n {
                let a = i as f64 * h;
                let b = a + h;
                let (even, odd) = cell_average(t, a, b, inv);
                for (coefficient, mode) in coefficients.iter_mut().zip(&basis.modes) {
                    *coefficient = coefficient.add(even.scale(mode[i] * h));
                }
                even_sum = even_sum.add(even.scale(h));
                odd_sum = odd_sum.add(odd.scale(h));
                loss += (even.re * even.re + even.im * even.im) * h / sigma;
            }
            assert!((even_sum.re - 1.).abs() < 1e-12 && even_sum.im.abs() < 1e-12);
            assert!(odd_sum.re.abs() < 1e-12 && odd_sum.im.abs() < 1e-12);
            let mut basis_loss = 0.;
            for (i, a) in coefficients.iter().enumerate() {
                for (j, b) in coefficients.iter().enumerate() {
                    basis_loss += (a.re * b.re + a.im * b.im) * basis.resistance_gram[i][j];
                }
            }
            assert!((basis_loss - loss).abs() < loss * 1e-10);
            (loss - exact).abs() / exact
        };
        assert!(evaluate(256) < evaluate(32));
        assert!(evaluate(256) < 1e-3);
    }
    #[test]
    fn thick_and_low_frequency_profiles_are_finite_and_continuous() {
        let t = 1e-3;
        for i in 0..100 {
            let a = t * i as f64 / 100.;
            let b = t * (i + 1) as f64 / 100.;
            let (e, o) = cell_average(t, a, b, 1e6);
            assert!([e.re, e.im, o.re, o.im].iter().all(|v| v.is_finite()));
        }
        let (e, o) = cell_average(t, 0., t / 4., 1e-6);
        let (dc, odd_dc) = cell_average(t, 0., t / 4., 0.);
        assert!((e.re - dc.re).abs() < 1e-8 * dc.re);
        assert!((o.re - odd_dc.re).abs() < 1e-8 * odd_dc.re.abs());
        let z: Vec<_> = (0..=100).map(|i| t * i as f64 / 100.).collect();
        assert!(build(&z, 1e12, 1., 1. / PI).is_ok());
    }
}
