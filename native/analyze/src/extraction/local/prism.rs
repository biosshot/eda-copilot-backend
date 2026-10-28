//! Finite triangular prism scalar 1/r integrals; no skin/magnetic material model.
//!
//! Near/self rule integrates the source z and fan radial coordinates analytically.
//! The remaining source-edge and target triangle/z integrals use Gauss rules.
//! Returned refinement differences are estimates, not independent error bounds.
use super::{error, Result};

#[derive(Clone, Debug)]
pub struct Prism {
    pub source_id: String,
    pub triangle_m: [[f64; 2]; 3],
    pub z_m: [f64; 2],
    pub volume_m3: f64,
}

impl Prism {
    pub fn new(source_id: String, triangle_m: [[f64; 2]; 3], z_m: [f64; 2]) -> Result<Self> {
        if source_id.is_empty()
            || !triangle_m
                .iter()
                .flatten()
                .chain(z_m.iter())
                .all(|x| x.is_finite())
            || z_m[1] <= z_m[0]
        {
            return Err(error(
                "PRISM_INPUT",
                "finite SI coordinates, source and positive thickness required",
            ));
        }
        let [a, b, c] = triangle_m;
        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        let volume_m3 = cross * (z_m[1] - z_m[0]) / 2.;
        if !volume_m3.is_finite() || volume_m3 <= 0. {
            return Err(error(
                "PRISM_GEOMETRY",
                "nondegenerate counterclockwise prism required",
            ));
        }
        Ok(Self {
            source_id,
            triangle_m,
            z_m,
            volume_m3,
        })
    }
}

#[derive(Clone, Debug)]
pub struct GaussRule {
    pub(super) nodes: Vec<f64>,
    pub(super) weights: Vec<f64>,
}

impl GaussRule {
    /// Read-only quadrature data shared with independent test references.
    pub fn nodes(&self)->&[f64] {&self.nodes}
    pub fn weights(&self)->&[f64] {&self.weights}
    pub fn new(order: usize) -> Result<Self> {
        if !(2..=64).contains(&order) {
            return Err(error("QUADRATURE_ORDER", "supported Gauss orders 2..64"));
        }
        let mut nodes = vec![0.; order];
        let mut weights = vec![0.; order];
        for i in 0..order.div_ceil(2) {
            let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (order as f64 + 0.5)).cos();
            let mut derivative = 0.;
            let mut converged = false;
            for _ in 0..32 {
                let (mut a, mut b) = (1., x);
                for k in 2..=order {
                    let c = ((2 * k - 1) as f64 * x * b - (k - 1) as f64 * a) / k as f64;
                    a = b;
                    b = c;
                }
                derivative = order as f64 * (x * b - a) / (x * x - 1.);
                let next = x - b / derivative;
                if (next - x).abs() <= 4. * f64::EPSILON {
                    x = next;
                    converged = true;
                    break;
                }
                x = next;
            }
            if !converged {
                return Err(error(
                    "QUADRATURE_CONVERGENCE",
                    "Gauss node iteration failed",
                ));
            }
            // Mapping [-1,1] to [0,1] includes the half factor in this weight.
            let w = 1. / ((1. - x * x) * derivative * derivative);
            nodes[i] = (1. - x) / 2.;
            nodes[order - 1 - i] = (1. + x) / 2.;
            weights[i] = w;
            weights[order - 1 - i] = w;
        }
        Ok(Self { nodes, weights })
    }
}

#[doc(hidden)]
pub fn cached_rule(order:usize)->Result<&'static GaussRule>{
    use std::sync::OnceLock;
    static RULES:[OnceLock<GaussRule>;65]=[const{OnceLock::new()};65];
    if !(2..=64).contains(&order){return Err(error("QUADRATURE_ORDER","supported Gauss orders2..64"));}
    if let Some(rule)=RULES[order].get(){return Ok(rule);}
    let rule=GaussRule::new(order)?;Ok(RULES[order].get_or_init(||rule))
}
#[test]
fn cached_gauss_nodes_and_weights_are_bit_identical(){for q in [2,7,15,31,63]{let a=GaussRule::new(q).unwrap();let b=cached_rule(q).unwrap();assert_eq!(a.nodes,b.nodes);assert_eq!(a.weights,b.weights);}}

pub(super) fn radial(k: f64) -> f64 {
    // ∫_0^1 r asinh(k/r) dr; this form avoids subtracting k*|k|.
    0.5 * (k.asinh() + k / (1_f64.hypot(k) + k.abs()))
}

/// ∫_source dV'/|point-r'| [m²]. Signed triangle fans preserve outside points.
pub fn point_potential(source: &Prism, point: [f64; 3], rule: &GaussRule) -> Result<f64> {
    if !point.iter().all(|x| x.is_finite()) {
        return Err(error("POINT_INPUT", "nonfinite evaluation point"));
    }
    let mut total = 0.;
    for edge in 0..3 {
        let a = source.triangle_m[edge];
        let b = source.triangle_m[(edge + 1) % 3];
        let a = [a[0] - point[0], a[1] - point[1]];
        let b = [b[0] - point[0], b[1] - point[1]];
        let cross = a[0] * b[1] - a[1] * b[0];
        if cross == 0. {
            continue;
        }
        let mut value = 0.;
        for (&u, &w) in rule.nodes.iter().zip(&rule.weights) {
            let distance = (a[0] * (1. - u) + b[0] * u).hypot(a[1] * (1. - u) + b[1] * u);
            if distance == 0. {
                return Err(error(
                    "QUADRATURE_DEGENERATE",
                    "fan quadrature crosses evaluation point",
                ));
            }
            value += w
                * (radial((source.z_m[1] - point[2]) / distance)
                    - radial((source.z_m[0] - point[2]) / distance));
        }
        total += cross * value;
    }
    if total.is_finite() && total > 0. {
        Ok(total)
    } else {
        Err(error(
            "QUADRATURE_RANGE",
            "nonpositive/nonfinite prism potential; no clipping applied",
        ))
    }
}

/// Directed double volume integral [m^5]; caller checks both directions/refinement.
pub fn volume_integral(target: &Prism, source: &Prism, rule: &GaussRule) -> Result<f64> {
    let [a, b, c] = target.triangle_m;
    let mut integral = 0.;
    for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
        for (&v, &wv) in rule.nodes.iter().zip(&rule.weights) {
            let xy = [
                a[0] + u * (b[0] - a[0]) + (1. - u) * v * (c[0] - a[0]),
                a[1] + u * (b[1] - a[1]) + (1. - u) * v * (c[1] - a[1]),
            ];
            for (&z, &wz) in rule.nodes.iter().zip(&rule.weights) {
                integral += wu
                    * wv
                    * wz
                    * (1. - u)
                    * 2.
                    * target.volume_m3
                    * point_potential(
                        source,
                        [
                            xy[0],
                            xy[1],
                            target.z_m[0] + z * (target.z_m[1] - target.z_m[0]),
                        ],
                        rule,
                    )?;
            }
        }
    }
    if integral.is_finite() && integral > 0. {
        Ok(integral)
    } else {
        Err(error(
            "QUADRATURE_RANGE",
            "double prism integral outside F64 range",
        ))
    }
}

/// Independent smooth product rule for strictly bbox-separated volume pairs.
/// Cost is n^6; bounded to order<=8 and used as a small reference/far fallback,
/// never silently applied to millions of pairs or to self/touching supports.
pub fn smooth_volume_integral(target: &Prism, source: &Prism, rule: &GaussRule) -> Result<f64> {
    if rule.nodes.len() > 8 {
        return Err(error(
            "SMOOTH_ORDER",
            "direct product reference bounded to order<=8",
        ));
    }
    let bounds = |p: &Prism| {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for xy in p.triangle_m {
            for i in 0..2 {
                lo[i] = lo[i].min(xy[i]);
                hi[i] = hi[i].max(xy[i]);
            }
        }
        lo[2] = p.z_m[0];
        hi[2] = p.z_m[1];
        (lo, hi)
    };
    let (al, ah) = bounds(target);
    let (bl, bh) = bounds(source);
    if !(0..3).any(|i| al[i] > bh[i] || bl[i] > ah[i]) {
        return Err(error(
            "SMOOTH_SEPARATION",
            "direct smooth reference requires strictly separated bounding boxes",
        ));
    }
    let sample = |p: &Prism| {
        let mut samples = Vec::new();
        let [a, b, c] = p.triangle_m;
        for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
            for (&v, &wv) in rule.nodes.iter().zip(&rule.weights) {
                for (&z, &wz) in rule.nodes.iter().zip(&rule.weights) {
                    samples.push((
                        [
                            a[0] + u * (b[0] - a[0]) + (1. - u) * v * (c[0] - a[0]),
                            a[1] + u * (b[1] - a[1]) + (1. - u) * v * (c[1] - a[1]),
                            p.z_m[0] + z * (p.z_m[1] - p.z_m[0]),
                        ],
                        2. * p.volume_m3 * (1. - u) * wu * wv * wz,
                    ));
                }
            }
        }
        samples
    };
    let a = sample(target);
    let b = sample(source);
    let mut integral = 0.;
    for (a, wa) in a {
        for (b, wb) in &b {
            integral += wa * wb / (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2]);
        }
    }
    if integral.is_finite() && integral > 0. {
        Ok(integral)
    } else {
        Err(error(
            "SMOOTH_RANGE",
            "direct smooth integral outside finite positive range",
        ))
    }
}
