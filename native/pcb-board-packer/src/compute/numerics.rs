//! Exact placement-grid arithmetic shared by GPU consumers.
use cubecl::prelude::*;

#[cube]
pub fn rp(x: f64) -> f64 {
    // Consuming the rounded product's bits prevents driver FMA contraction of
    // x*1000+0.5. Reproduce floor(product+0.5), including its rounding at 0.5.
    let scaled = x * 1000.0;
    let bits = scaled.to_bits();
    let magnitude = bits & 0x7fffffffffffffffu64;
    let negative = (bits >> 63u64) != 0;
    let exponent = (magnitude >> 52u64) as i32 - 1023i32;
    let mut integer = 0u64;
    if exponent < 0 {
        if (!negative && magnitude >= 0x3fdfffffffffffffu64)
            || (negative && magnitude > 0x3fe0000000000000u64)
        {
            integer = 1;
        }
    } else {
        let shift = (52i32 - exponent) as u64;
        let significand = (magnitude & 0x000fffffffffffffu64) | (1u64 << 52u64);
        integer = significand >> shift;
        let fraction = significand & ((1u64 << shift) - 1u64);
        let half = 1u64 << (shift - 1u64);
        if (!negative && fraction >= half) || (negative && fraction > half) {
            integer += 1;
        }
    }
    let mut n = integer as f64;
    if negative {
        n = -n;
    }
    grid_quotient(n)
}

// Correctly rounded integer / 1000, with an exact integer residual correction.
// The multiplication estimate is at most one ULP away. In normalized units the
// exact value has denominator 125 (1000 = 125*8), so no halfway ties exist.
// Compare numerator - significand*125 to half an ULP; below a power of two the
// neighboring spacing is halved. This avoids expensive GPU U64 division.
// Host guards ensure |n| < 2^53 and all shifts/products fit in 64 bits.
#[cube]
fn grid_quotient(n: f64) -> f64 {
    let mut result = 0.0f64;
    if n != 0.0 {
        let a = n.abs();
        let integer = a as u64;
        let mut bits = (a * 0.001).to_bits();
        let significand = (bits & 0x000fffffffffffffu64) | (1u64 << 52u64);
        let shift = 1072u64 - (bits >> 52u64);
        let residual = (integer << shift) as i64 - (significand * 125u64) as i64;
        let mut down = 63i64;
        if significand == (1u64 << 52u64) {
            down = 32;
        }
        if residual >= 63i64 {
            bits += 1;
        } else if residual <= -down {
            bits -= 1;
        }
        if n < 0.0 {
            bits |= 1u64 << 63u64;
        }
        result = f64::from_bits(bits);
    }
    result
}

#[cfg(test)]
mod rounding_tests {
    use super::{grid_quotient, rp};
    #[test]
    fn rounding_matches_cpu_at_halfway_boundaries() {
        for n in -10000i64..=10000 {
            let middle = (n as f64 + 0.5) / 1000.0;
            for bits in [
                middle.to_bits().wrapping_sub(1),
                middle.to_bits(),
                middle.to_bits().wrapping_add(1),
            ] {
                let x = f64::from_bits(bits);
                assert_eq!(rp(x), crate::geometry::round_placement(x), "{x:?}");
            }
        }
        let mut state = 1u64;
        for _ in 0..100000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let x = (state as f64 / u64::MAX as f64 - 0.5) * 4e12;
            assert_eq!(rp(x), crate::geometry::round_placement(x), "{x:?}");
        }
    }
    #[test]
    fn decimal_grid_matches_correctly_rounded_cpu_division() {
        for n in -1000000i64..=1000000 {
            assert_eq!(
                grid_quotient(n as f64).to_bits(),
                (n as f64 / 1000.0).to_bits(),
                "{n}"
            );
        }
        let mut state = 1u64;
        for _ in 0..100000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let n = (state % 4000000000000001) as i64 - 2000000000000000;
            assert_eq!(
                grid_quotient(n as f64).to_bits(),
                (n as f64 / 1000.0).to_bits(),
                "{n}"
            );
        }
    }
}

#[cube(launch_unchecked)]
pub fn rounding_probe(input: &Array<f64>, out: &mut Array<f64>) {
    let i = ABSOLUTE_POS as usize;
    if i < input.len() {
        out[i] = rp(input[i]);
    }
}
