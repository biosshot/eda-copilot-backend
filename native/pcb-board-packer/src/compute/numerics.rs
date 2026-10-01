//! Canonical binary32 placement arithmetic, executed by CPU and GPU.
#[cfg(feature = "gpu")]
use cubecl::prelude::*;

/// Round to the nearest integer, half ties towards +inf. Adding 0.5 can itself
/// round across a tie; inspect the binary32 fraction instead.
#[cfg_attr(feature = "gpu", cube)]
pub fn js_round(x: f32) -> f32 {
    let bits = x.to_bits();
    let magnitude = bits & 0x7fffffffu32;
    let negative = (bits >> 31u32) != 0;
    let exponent = (magnitude >> 23u32) as i32 - 127i32;
    let mut result = x;
    if exponent < 23 {
        let mut integer = 0u32;
        if exponent < 0 {
            if (!negative && magnitude >= 0x3f000000u32)
                || (negative && magnitude > 0x3f000000u32) { integer = 1; }
        } else {
            let shift = (23i32 - exponent) as u32;
            let significand = (magnitude & 0x007fffffu32) | (1u32 << 23u32);
            integer = significand >> shift;
            let fraction = significand & ((1u32 << shift) - 1u32);
            let half = 1u32 << (shift - 1u32);
            if (!negative && fraction >= half) || (negative && fraction > half) { integer += 1; }
        }
        result = integer as f32;
        if negative { result = -result; }
    }
    result
}

#[cfg_attr(feature = "gpu", cube)]
pub fn rp(x: f32) -> f32 {
    let scaled = x * 1000.0;
    grid_quotient(js_round(scaled))
}

/// PCB distances use this explicit binary32 sequence on CPU, TS and GPU.
/// Supported local frames keep x*x+y*y finite; no platform hypot accumulator
/// or implicit FMA is part of the expression. sqrt retains API precision.
#[cfg_attr(feature = "gpu", cube)]
pub fn hypot(x: f32, y: f32) -> f32 { (x*x+y*y).sqrt() }

/// Correctly rounded integer ticks / 1000 without depending on GPU division.
/// For |ticks| <= 2^24 the shifted numerator and significand*125 fit i32.
#[cfg_attr(feature = "gpu", cube)]
pub fn grid_quotient(n: f32) -> f32 {
    let mut result = 0.0f32;
    if n != 0.0 {
        let a = n.abs();
        let integer = a as u32;
        let mut bits = (a * 0.001).to_bits();
        let significand = (bits & 0x007fffffu32) | (1u32 << 23u32);
        let exponent = bits >> 23u32;
        if exponent <= 147u32 {
            let shift = 147u32 - exponent;
            let residual = (integer << shift) as i32 - (significand * 125u32) as i32;
            let mut down = 63i32;
            if significand == (1u32 << 23u32) { down = 32; }
            if residual >= 63i32 { bits += 1; }
            else if residual <= -down { bits -= 1; }
            if n < 0.0 { bits |= 1u32 << 31u32; }
            result = f32::from_bits(bits);
        } else {
            // Domain guards reject frames this large before GPU dispatch.
            result = n / 1000.0;
        }
    }
    result
}

#[cfg(feature = "gpu")]
#[cube(launch_unchecked)]
pub fn rounding_probe(input: &Array<f32>, out: &mut Array<f32>) {
    let i = ABSOLUTE_POS as usize;
    if i < input.len() { out[i] = rp(input[i]); }
}

/// Exercise underflow, subnormal-input scaling and forbidden FMA contraction.
/// Runtime inputs prevent compiler constant folding of the arithmetic.
#[cfg(feature = "gpu")]
#[cube(launch_unchecked)]
pub fn execution_probe(input: &Array<f32>, out: &mut Array<f32>) {
    let i = ABSOLUTE_POS as usize;
    if i < input.len() {
        let x = input[i];
        out[i * 3] = x * 2.0;
        out[i * 3 + 1] = x * 0.5;
        out[i * 3 + 2] = x * 85070591730234615865843651857942052864.0;
    }
    if i == 0 {
        let a = input[input.len() - 2];
        let b = input[input.len() - 1];
        out[input.len() * 3] = a * b - 1.0;
    }
}

#[cfg(feature = "gpu")]
#[cube(launch_unchecked)]
pub fn arithmetic_probe(input: &Array<f32>, out: &mut Array<f32>) {
    let i=ABSOLUTE_POS as usize;
    if i<input.len() {let x=input[i];out[i*2]=x/1.3;out[i*2+1]=x.abs().sqrt();}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_ties_and_neighbors_in_both_directions() {
        for n in -8192..8192 {
            let middle = n as f32 + 0.5;
            assert_eq!(js_round(middle), (n + 1) as f32);
            assert_eq!(js_round(middle.next_down()), n as f32);
            assert_eq!(js_round(middle.next_up()), (n + 1) as f32);
        }
        assert_eq!(js_round(0.5f32.next_down()), 0.0);
        assert_eq!(js_round(-0.5).to_bits(), (-0.0f32).to_bits());
        assert_eq!(js_round(8_388_609.0), 8_388_609.0);
    }
    #[test]
    fn decimal_grid_matches_cpu_division_and_is_idempotent() {
        for n in -1_024_000..=1_024_000 {
            let expected = n as f32 / 1000.0;
            assert_eq!(grid_quotient(n as f32).to_bits(), expected.to_bits(), "tick {n}");
            assert_eq!(rp(expected).to_bits(), expected.to_bits(), "repeat tick {n}");
        }
        assert_eq!(rp(-0.0).to_bits(), 0.0f32.to_bits());
    }
}
