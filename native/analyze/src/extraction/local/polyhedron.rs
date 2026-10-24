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
    vertices: [[f64; 3]; 6],
    faces: Vec<Face>,
    edges: Vec<Edge>,
}

impl PointPotential {
    pub fn new(prism: &Prism) -> Result<Self> {
        let vertices = std::array::from_fn(|i| {
            [
                prism.triangle_m[i % 3][0],
                prism.triangle_m[i % 3][1],
                prism.z_m[i / 3],
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
            vertices,
            faces,
            edges: edges.into_values().collect(),
        })
    }

    /// Positive ∫ dV'/|r-r'| [m²]. Near-edge indeterminate F64 limits refuse;
    /// this does not substitute a softened/point/surface kernel.
    pub fn at(&self, point: [f64; 3]) -> Result<f64> {
        if point.iter().any(|x| !x.is_finite()) {
            return Err(error("POINT_INPUT", "nonfinite evaluation point"));
        }
        let r = self.vertices.map(|p| sub(p, point));
        let lengths = r.map(norm);
        let mut value = 0.;
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
                return Err(error(
                    "POLYHEDRON_EDGE_LIMIT",
                    "indeterminate point-on-edge logarithm; explicit refusal",
                ));
            }
            let logarithm = (2. * edge.length / denominator).ln_1p();
            value += quadratic * logarithm;
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
        }
        value *= 0.5;
        if value.is_finite() && value > 0. {
            Ok(value)
        } else {
            Err(error(
                "POLYHEDRON_RANGE",
                "nonpositive/nonfinite point potential; no clipping applied",
            ))
        }
    }
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
    let [a, b, c] = target.triangle_m;
    let mut value = 0.;
    for (&u, &wu) in xy_rule.nodes.iter().zip(&xy_rule.weights) {
        for (&v, &wv) in xy_rule.nodes.iter().zip(&xy_rule.weights) {
            for (&z, &wz) in z_rule.nodes.iter().zip(&z_rule.weights) {
                let p = [
                    a[0] + u * (b[0] - a[0]) + (1. - u) * v * (c[0] - a[0]),
                    a[1] + u * (b[1] - a[1]) + (1. - u) * v * (c[1] - a[1]),
                    target.z_m[0] + z * (target.z_m[1] - target.z_m[0]),
                ];
                value += 2. * target.volume_m3 * wu * wv * wz * (1. - u) * source.at(p)?;
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
