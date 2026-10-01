//! F32 diagnostics and conservative bounds. Never use score tolerance as a
//! physical clearance or as the minimum accepted score improvement.

pub(crate) fn ulp(value: f32) -> f32 {
    let a = value.abs();
    if !a.is_finite() { return f32::INFINITY; }
    (a.next_up() - a).max(f32::MIN_POSITIVE)
}
pub(crate) fn diagnostic_tolerance(a: f32, b: f32) -> f32 {
    0.001f32.max(4.0 * ulp(a).max(ulp(b)))
}
pub(crate) fn score_close(a: f32, b: f32) -> bool {
    a.is_finite() && b.is_finite() && (a-b).abs() <= diagnostic_tolerance(a,b)
}

/// A rounded path penalty may move by one existing 0.001 quantum. Adjacent
/// mathematical ticks are represented with up to half an ULP at each endpoint;
/// their F32 difference can therefore exceed 0.001 (e.g. near score 510).
/// This accounts for that representation in diagnostics, not physical geometry
/// or search thresholds. Other expression error still needs its own bound.
pub(crate) fn quantized_score_close(a: f32,b: f32) -> bool {
    let tolerance=diagnostic_tolerance(a,b).max((0.001+ulp(a).max(ulp(b))).next_up());
    a.is_finite() && b.is_finite() && (a-b).abs()<=tolerance
}

/// Summation roundoff bound for nonnegative terms under RTE+FTZ. Callers must
/// supply an upper bound on operations and on the absolute term sum. If those
/// assumptions cannot be established, infinity means do not prune.
pub(crate) fn nonnegative_sum_error(operations: usize, absolute_sum: f32) -> f32 {
    if !absolute_sum.is_finite() || absolute_sum < 0.0 { return f32::INFINITY; }
    let nu = ((operations as f32) * (f32::EPSILON * 0.5)).next_up();
    if nu >= 1.0 { return f32::INFINITY; }
    let gamma = (nu / (1.0-nu).next_down()).next_up();
    (gamma * absolute_sum).next_up().max(f32::MIN_POSITIVE * operations as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_do_not_treat_nan_or_overflow_as_a_score() {
        assert!(!score_close(f32::NAN,f32::NAN));
        assert!(!score_close(f32::INFINITY,f32::INFINITY));
        assert!(score_close(1_000_000.0,1_000_000.0f32.next_up()));
        assert!(!score_close(1.0,1.1));
        for tick in (-1_024_000..1_024_000).step_by(1024) {
            let a=crate::numerics::grid_quotient(tick as f32);
            let b=crate::numerics::grid_quotient((tick+1) as f32);
            assert!(quantized_score_close(a,b));
        }
    }
    #[test]
    fn nonnegative_bound_covers_cancellation_and_accumulation_without_f64() {
        let _env = crate::float_env::Guard::enter();
        let n=1024;
        let mut sum=16_777_216.0f32;
        for _ in 0..n {sum+=1.0;}
        assert_eq!(sum,16_777_216.0);
        assert!(nonnegative_sum_error(n,16_778_240.0)>=1024.0);
        assert!(nonnegative_sum_error(1<<24,1.0).is_infinite());
        assert!(nonnegative_sum_error(2,-1.0).is_infinite());
    }
}
