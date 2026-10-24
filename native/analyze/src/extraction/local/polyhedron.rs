//! Uniform finite-prism point potential via divergence theorem, face solid
//! angles and edge logarithms. Outer volume quadrature still requires refinement.
//! This alternative near primitive does not replace independent box references.
use super::{
    error,
    prism::{GaussRule, Prism},
    Result,
};
use std::collections::BTreeMap;

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}

#[derive(Clone, Debug)]
struct Face {
    normal: [f64; 3],
    triangles: Vec<[usize; 3]>,
    anchor: usize,
}
#[derive(Clone, Debug)]
struct Edge {
    endpoints: [usize; 2],
    length: f64,
    tangent: [f64; 3],
    dyad: [[f64; 3]; 3],
    incident: usize,
}

#[derive(Clone, Debug)]
pub struct PointPotential {
    origin: [f64; 3],
    prism: Prism,
    vertices: [[f64; 3]; 6],
    faces: Vec<Face>,
    edges: Vec<Edge>,
}

/// Source integration diagnostics, separate from outer volume quadrature.
/// The order change is an estimator, not a certified integral error bound.
#[derive(Clone, Debug)]
pub struct PotentialEvaluation {
    pub value_m2: f64,
    pub cancellation_ratio: f64,
    pub source_order: usize,
    pub source_relative_change: f64,
}

impl PointPotential {
    pub fn new(prism: &Prism) -> Result<Self> {
        let origin = [prism.triangle_m[0][0], prism.triangle_m[0][1], prism.z_m[0]];
        let vertices = std::array::from_fn(|i| {
            [
                prism.triangle_m[i % 3][0] - origin[0],
                prism.triangle_m[i % 3][1] - origin[1],
                prism.z_m[i / 3] - origin[2],
            ]
        });
        let polygons = [
            vec![0, 2, 1],
            vec![3, 4, 5],
            vec![0, 1, 4, 3],
            vec![1, 2, 5, 4],
            vec![2, 0, 3, 5],
        ];
        let mut faces = Vec::new();
        let mut edges = BTreeMap::<[usize; 2], Edge>::new();
        for polygon in polygons {
            let raw = cross(
                sub(vertices[polygon[1]], vertices[polygon[0]]),
                sub(vertices[polygon[2]], vertices[polygon[1]]),
            );
            let length = norm(raw);
            if length <= 0. || !length.is_finite() {
                return Err(error(
                    "POLYHEDRON_GEOMETRY",
                    "degenerate face; no feature discarded",
                ));
            }
            let normal = raw.map(|x| x / length);
            let triangles = (1..polygon.len() - 1)
                .map(|i| [polygon[0], polygon[i], polygon[i + 1]])
                .collect();
            faces.push(Face {
                normal,
                triangles,
                anchor: polygon[0],
            });
            for i in 0..polygon.len() {
                let from = polygon[i];
                let to = polygon[(i + 1) % polygon.len()];
                let raw = sub(vertices[to], vertices[from]);
                let length = norm(raw);
                let tangent = raw.map(|x| x / length);
                let outward = cross(tangent, normal);
                let mut key = [from, to];
                key.sort();
                let edge = edges.entry(key).or_insert_with(|| {
                    let raw = sub(vertices[key[1]], vertices[key[0]]);
                    Edge {
                        endpoints: key,
                        length,
                        tangent: raw.map(|x| x / length),
                        dyad: [[0.; 3]; 3],
                        incident: 0,
                    }
                });
                edge.incident += 1;
                for a in 0..3 {
                    for b in 0..3 {
                        edge.dyad[a][b] += normal[a] * outward[b];
                    }
                }
            }
        }
        if edges.values().any(|e| e.incident != 2) {
            return Err(error("POLYHEDRON_TOPOLOGY", "nonmanifold prism edge"));
        }
        Ok(Self {
            origin,
            prism: prism.clone(),
            vertices,
            faces,
            edges: edges.into_values().collect(),
        })
    }

    /// Positive ∫ dV'/|r-r'| [m²]. Near-edge indeterminate F64 limits refuse;
    /// this does not substitute a softened/point/surface kernel.
    pub fn at(&self, point: [f64; 3]) -> Result<f64> {
        self.at_diagnostic(point).map(|v| v.value_m2)
    }

    /// Analytic faces/edges are used only when their summation condition is
    /// resolved. Otherwise use positive source-XY quadrature with exact source-z
    /// integration, explicit order control and refusal on failed refinement.
    /// No negative value is clipped, no support is widened or removed.
    pub fn at_diagnostic(&self, point: [f64; 3]) -> Result<PotentialEvaluation> {
        if point.iter().any(|x| !x.is_finite()) {
            return Err(error("POINT_INPUT", "nonfinite evaluation point"));
        }
        let local = sub(point, self.origin);
        let r = self.vertices.map(|p| sub(p, local));
        let lengths = r.map(norm);
        let mut value = 0.;
        let mut magnitude = 0.;
        for edge in &self.edges {
            let a = r[edge.endpoints[0]];
            let mut quadratic = 0.;
            for i in 0..3 {
                for j in 0..3 {
                    quadratic += a[i] * edge.dyad[i][j] * a[j];
                }
            }
            if quadratic == 0. {
                continue;
            }
            let t = dot(a, edge.tangent);
            let perpendicular = std::array::from_fn(|i| a[i] - t * edge.tangent[i]);
            let rho2 = dot(perpendicular, perpendicular);
            let ra = lengths[edge.endpoints[0]];
            let rb = lengths[edge.endpoints[1]];
            // Stable ra+rb-l, including points with projection inside the edge.
            let denominator = rho2 / (ra + t.abs())
                + rho2 / (rb + (t + edge.length).abs())
                + (t.abs() + (t + edge.length).abs() - edge.length).max(0.);
            if denominator <= 0. || !denominator.is_finite() {
                return self.positive_source(point, f64::INFINITY);
            }
            let logarithm = (2. * edge.length / denominator).ln_1p();
            let term = quadratic * logarithm;
            value += term;
            magnitude += term.abs();
        }
        for face in &self.faces {
            let h = dot(r[face.anchor], face.normal);
            if h == 0. {
                continue;
            }
            let mut omega = 0.;
            for &[a, b, c] in &face.triangles {
                let numerator = dot(r[a], cross(r[b], r[c]));
                let denominator = lengths[a] * lengths[b] * lengths[c]
                    + lengths[a] * dot(r[b], r[c])
                    + lengths[b] * dot(r[c], r[a])
                    + lengths[c] * dot(r[a], r[b]);
                omega += 2. * numerator.atan2(denominator);
            }
            value -= h * h * omega;
            magnitude += (h * h * omega).abs();
        }
        value *= 0.5;
        let cancellation_ratio = magnitude * 0.5 / value.abs();
        if value.is_finite() && value > 0. && cancellation_ratio * 64. * f64::EPSILON <= 1e-7 {
            Ok(PotentialEvaluation {
                value_m2: value,
                cancellation_ratio,
                source_order: 0,
                source_relative_change: 0.,
            })
        } else {
            self.positive_source(point, cancellation_ratio)
        }
    }

    fn positive_source(
        &self,
        point: [f64; 3],
        cancellation_ratio: f64,
    ) -> Result<PotentialEvaluation> {
        let mut previous = self.positive_rule(point, 7)?;
        for order in [15, 31, 63] {
            let value = self.positive_rule(point, order)?;
            let change = (value - previous).abs() / value;
            if change <= 1e-5 {
                return Ok(PotentialEvaluation {
                    value_m2: value,
                    cancellation_ratio,
                    source_order: order,
                    source_relative_change: change,
                });
            }
            previous = value;
        }
        Err(error("POLYHEDRON_SOURCE_QUADRATURE", "positive source rule did not meet explicit1e-5 refinement estimate through order63; no unchecked fallback"))
    }

    fn positive_rule(&self, point: [f64; 3], order: usize) -> Result<f64> {
        let rule = GaussRule::new(order)?;
        // Query-relative coordinates retain the authored narrow support. Choose
        // the query as fan apex only inside XY; outside use an original vertex,
        // so all fan weights remain nonnegative without geometric clipping.
        let p = sub(point, self.origin);
        let tri: [[f64; 2]; 3] =
            std::array::from_fn(|i| [self.vertices[i][0] - p[0], self.vertices[i][1] - p[1]]);
        let cross2 = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
        let inside = (0..3).all(|i| cross2(tri[i], tri[(i + 1) % 3]) >= 0.);
        let apex_index = (0..3)
            .min_by(|&a, &b| {
                tri[a][0]
                    .hypot(tri[a][1])
                    .total_cmp(&tri[b][0].hypot(tri[b][1]))
            })
            .unwrap();
        let apex = if inside { [0., 0.] } else { tri[apex_index] };
        let z0 = self.prism.z_m[0] - point[2];
        let z1 = self.prism.z_m[1] - point[2];
        let mut total = 0.;
        for edge in 0..3 {
            let relative = |i: usize| {
                if inside {
                    tri[i]
                } else {
                    [
                        self.vertices[i][0] - self.vertices[apex_index][0],
                        self.vertices[i][1] - self.vertices[apex_index][1],
                    ]
                }
            };
            let a = relative(edge);
            let b = relative((edge + 1) % 3);
            let jacobian = cross2(a, b);
            if jacobian == 0. {
                continue;
            }
            if jacobian < 0. {
                return Err(error(
                    "POLYHEDRON_SOURCE_GEOMETRY",
                    "positive source fan orientation lost; no clipping",
                ));
            }
            if inside && z0 <= 0. && z1 >= 0. {
                // The distance minimum along a long edge can lie between Gauss
                // nodes. Split at that geometric minimum and grade toward it;
                // every interval is retained, with its exact Jacobian.
                let edge_delta = [b[0] - a[0], b[1] - a[1]];
                let length2 = edge_delta[0] * edge_delta[0] + edge_delta[1] * edge_delta[1];
                let minimum = -(a[0] * edge_delta[0] + a[1] * edge_delta[1]) / length2;
                let intervals = if minimum > 0. && minimum < 1. {
                    vec![(minimum, -minimum), (minimum, 1. - minimum)]
                } else if minimum >= 1. {
                    vec![(1., -1.)]
                } else {
                    vec![(0., 1.)]
                };
                for (start, delta) in intervals {
                    for (&t, &w) in rule.nodes.iter().zip(&rule.weights) {
                        let s = start + delta * t * t;
                        let d = [a[0] * (1. - s) + b[0] * s, a[1] * (1. - s) + b[1] * s];
                        let rho = d[0].hypot(d[1]);
                        total += jacobian
                            * w
                            * 2.
                            * delta.abs()
                            * t
                            * (super::prism::radial(z1 / rho) - super::prism::radial(z0 / rho));
                    }
                }
                continue;
            }
            for (&s, &ws) in rule.nodes.iter().zip(&rule.weights) {
                let d = [a[0] * (1. - s) + b[0] * s, a[1] * (1. - s) + b[1] * s];
                {
                    for (&r, &wr) in rule.nodes.iter().zip(&rule.weights) {
                        let rho = (apex[0] + r * d[0]).hypot(apex[1] + r * d[1]);
                        if rho <= 0. {
                            return Err(error(
                                "POLYHEDRON_SOURCE_LIMIT",
                                "source quadrature hit logarithmic line singularity",
                            ));
                        }
                        total += jacobian * ws * wr * r * stable_z_integral(rho, z0, z1);
                    }
                }
            }
        }
        if total.is_finite() && total > 0. {
            Ok(total)
        } else {
            Err(error(
                "POLYHEDRON_SOURCE_RANGE",
                "positive source integral not finite/positive",
            ))
        }
    }
}

fn stable_z_integral(rho: f64, z0: f64, z1: f64) -> f64 {
    if z0 <= 0. && z1 >= 0. {
        return (z1 / rho).asinh() - (z0 / rho).asinh();
    }
    let (a, b) = if z0 >= 0. { (z0, z1) } else { (-z1, -z0) };
    let ra = a.hypot(rho);
    let rb = b.hypot(rho);
    (((b - a) * (1. + (a + b) / (ra + rb))) / (a + ra)).ln_1p()
}

pub fn volume_integral(target: &Prism, source: &PointPotential, rule: &GaussRule) -> Result<f64> {
    volume_integral_anisotropic(target, source, rule, rule)
}

/// The outer XY and z quadrature orders can be refined independently. The
/// source and target remain full finite volumes, with their exact thicknesses.
/// Selection of a smaller order requires separate reference/convergence gates.
pub fn volume_integral_anisotropic(
    target: &Prism,
    source: &PointPotential,
    xy_rule: &GaussRule,
    z_rule: &GaussRule,
) -> Result<f64> {
    let mut tri = target.triangle_m;
    let edge2 = |i: usize| {
        (tri[i][0] - tri[(i + 1) % 3][0]).powi(2) + (tri[i][1] - tri[(i + 1) % 3][1]).powi(2)
    };
    let edges = [edge2(0), edge2(1), edge2(2)];
    let twice_area = target.volume_m3 * 2. / (target.z_m[1] - target.z_m[0]);
    let graded = edges.into_iter().fold(0., f64::max) / twice_area > 1e6;
    if graded {
        let shortest = (0..3)
            .min_by(|&i, &j| edges[i].total_cmp(&edges[j]))
            .unwrap();
        tri.rotate_left(shortest);
    }
    let [a, b, c] = tri;
    let mut value = 0.;
    for (&u, &wu) in xy_rule.nodes.iter().zip(&xy_rule.weights) {
        for (&v, &wv) in xy_rule.nodes.iter().zip(&xy_rule.weights) {
            let (v, graded_jacobian) = if graded { (v * v, 2. * v) } else { (v, 1.) };
            for (&z, &wz) in z_rule.nodes.iter().zip(&z_rule.weights) {
                let p = [
                    a[0] + u * (b[0] - a[0]) + (1. - u) * v * (c[0] - a[0]),
                    a[1] + u * (b[1] - a[1]) + (1. - u) * v * (c[1] - a[1]),
                    target.z_m[0] + z * (target.z_m[1] - target.z_m[0]),
                ];
                value += 2.
                    * target.volume_m3
                    * wu
                    * wv
                    * wz
                    * (1. - u)
                    * graded_jacobian
                    * source.at(p)?;
            }
        }
    }
    if value.is_finite() && value > 0. {
        Ok(value)
    } else {
        Err(error(
            "POLYHEDRON_RANGE",
            "double prism integral outside finite positive range",
        ))
    }
}
