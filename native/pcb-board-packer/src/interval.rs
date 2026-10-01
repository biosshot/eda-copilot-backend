//! Outward binary32 enclosures. These carry numerical uncertainty; they never
//! enlarge a physical clearance. Every endpoint is normal, zero or infinity,
//! so the bounds survive the scoped FTZ/DAZ environment.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Interval { pub lo: f32, pub hi: f32 }

fn down(x: f32) -> f32 {
    if x == 0.0 { return -f32::MIN_POSITIVE; }
    let y = x.next_down();
    if y.abs() < f32::MIN_POSITIVE { 0.0 } else { y }
}
fn up(x: f32) -> f32 {
    if x == 0.0 { return f32::MIN_POSITIVE; }
    let y = x.next_up();
    if y.abs() < f32::MIN_POSITIVE { -0.0 } else { y }
}
impl Interval {
    pub const WHOLE: Self = Self { lo: f32::NEG_INFINITY, hi: f32::INFINITY };
    pub fn exact(x: f32) -> Self {
        if x.is_nan() { Self::WHOLE } else { Self { lo: x, hi: x } }
    }
    fn rounded(lo: f32, hi: f32) -> Self {
        if lo.is_nan() || hi.is_nan() || lo > hi { Self::WHOLE }
        else { Self { lo: down(lo), hi: up(hi) } }
    }
    pub fn contains(self, x: f32) -> bool { x.is_finite() && self.lo <= x && x <= self.hi }
    pub fn add(self, b: Self) -> Self { Self::rounded(self.lo+b.lo, self.hi+b.hi) }
    pub fn sub(self, b: Self) -> Self { Self::rounded(self.lo-b.hi, self.hi-b.lo) }
    pub fn mul(self, b: Self) -> Self {
        let p = [self.lo*b.lo,self.lo*b.hi,self.hi*b.lo,self.hi*b.hi];
        if p.iter().any(|x|x.is_nan()) { return Self::WHOLE; }
        Self::rounded(p.iter().copied().fold(f32::INFINITY,f32::min),
            p.iter().copied().fold(f32::NEG_INFINITY,f32::max))
    }
    pub fn square(self) -> Self {
        let a = self.lo.abs(); let b = self.hi.abs();
        let lo = if self.lo <= 0.0 && self.hi >= 0.0 { 0.0 } else { a.min(b)*a.min(b) };
        Self::rounded(lo,a.max(b)*a.max(b)).max(Self::exact(0.0))
    }
    pub fn min(self,b: Self) -> Self { Self {lo:self.lo.min(b.lo),hi:self.hi.min(b.hi)} }
    pub fn max(self,b: Self) -> Self { Self {lo:self.lo.max(b.lo),hi:self.hi.max(b.hi)} }
    fn widen_ulps(self, count: usize) -> Self {
        let mut out = self;
        for _ in 0..count { out.lo=down(out.lo);out.hi=up(out.hi); }
        out
    }
    /// Vulkan OpFDiv permits 2.5 ULP, rather than correctly-rounded division.
    /// Four outward steps also cover CPU endpoint rounding/binade boundaries.
    /// Outside its specified denominator range, make no finite claim.
    pub fn div_vulkan(self,b: Self) -> Self {
        if b.lo<=0.0 && b.hi>=0.0 || b.lo.abs().min(b.hi.abs())<f32::MIN_POSITIVE
            || b.lo.abs().max(b.hi.abs())>2.0f32.powi(126) { return Self::WHOLE; }
        let p=[self.lo/b.lo,self.lo/b.hi,self.hi/b.lo,self.hi/b.hi];
        if p.iter().any(|x|x.is_nan()) {return Self::WHOLE;}
        Self {lo:p.iter().copied().fold(f32::INFINITY,f32::min),
            hi:p.iter().copied().fold(f32::NEG_INFINITY,f32::max)}.widen_ulps(4)
    }
    /// GLSL Sqrt inherits 1/InverseSqrt: 2 ULP for the latter, 2.5 for
    /// division. Eight outward steps conservatively cover their composition,
    /// binade changes and the correctly-rounded CPU endpoint evaluation.
    /// Reference: docs.vulkan.org/spec/latest/appendices/spirvenv.html.
    pub fn sqrt_vulkan(self) -> Self {
        if self.lo<0.0 || !self.hi.is_finite() {return Self::WHOLE;}
        Self {lo:self.lo.sqrt(),hi:self.hi.sqrt()}.widen_ulps(8).max(Self::exact(0.0))
    }
    /// Placement rounding is a monotone, discontinuous domain operation. Keep
    /// both possible ticks when uncertainty crosses a half-grid boundary.
    pub fn placement(self) -> Self {
        if !self.lo.is_finite() || !self.hi.is_finite() {return Self::WHOLE;}
        Self {lo:crate::geometry::round_placement(self.lo),hi:crate::geometry::round_placement(self.hi)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outward_bounds_survive_ftz_and_cancellation() {
        let _env=crate::float_env::Guard::enter();
        let zero=Interval::exact(1.0).sub(Interval::exact(1.0));
        assert!(zero.lo<0.0 && zero.hi>0.0);
        let tiny=Interval::exact(f32::MIN_POSITIVE).mul(Interval::exact(0.5));
        assert!(tiny.lo<=0.0 && tiny.hi>=f32::MIN_POSITIVE);
        let square=Interval {lo:-2.0,hi:3.0}.square();
        assert_eq!(square.lo,0.0);assert!(square.hi>=9.0);
        assert!(Interval::exact(16_777_216.0).add(Interval::exact(1.0)).hi>=16_777_218.0);
    }
    #[test]
    fn domain_discontinuities_and_api_precision_are_enclosed() {
        let _env=crate::float_env::Guard::enter();
        let half=0.0005f32;
        let ticks=Interval {lo:half.next_down(),hi:half.next_up()}.placement();
        assert_eq!(ticks.lo,0.0);assert_eq!(ticks.hi,0.001);
        let root=Interval::exact(2.0).sqrt_vulkan();
        assert!(root.contains(2.0f32.sqrt().next_up()));
        let singular=Interval::exact(1.0).div_vulkan(Interval {lo:-1.0,hi:1.0});
        assert_eq!(singular.lo,f32::NEG_INFINITY);assert_eq!(singular.hi,f32::INFINITY);
        assert!(!singular.contains(f32::NAN));
    }
}
