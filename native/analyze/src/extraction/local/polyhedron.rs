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

#[derive(Debug)]
struct FarSamples {
    order: usize,
    center: [f64; 3],
    radius: f64,
    volume: f64,
    points: Vec<[f64; 3]>,
    weights: Vec<f64>,
    mass_error: f64,
}
#[derive(Clone, Debug)]
pub struct PointPotential {
    origin: [f64; 3],
    prism: Option<Prism>,
    tetra: Option<[[f64; 3]; 4]>,
    tetra_tolerance: f64,
    far_samples: std::sync::Arc<[std::sync::OnceLock<FarSamples>; 7]>,
    vertices: Vec<[f64; 3]>,
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
        let vertices = (0..6)
            .map(|i| {
                [
                    prism.triangle_m[i % 3][0] - origin[0],
                    prism.triangle_m[i % 3][1] - origin[1],
                    prism.z_m[i / 3] - origin[2],
                ]
            })
            .collect();
        let polygons = vec![
            vec![0, 2, 1],
            vec![3, 4, 5],
            vec![0, 1, 4, 3],
            vec![1, 2, 5, 4],
            vec![2, 0, 3, 5],
        ];
        Self::from_faces(origin, vertices, polygons, Some(prism.clone()), None)
    }

    /// The same finite convex-polyhedron source formula, now on a tetrahedron.
    /// No prism surrogate, changed support, or centroid distance is introduced.
    pub fn tetrahedron(points: [[f64; 3]; 4]) -> Result<Self> {
        super::mesh::tetra_coefficients(points)?;
        let origin = points[0];
        let vertices: Vec<_> = points.iter().map(|&p| sub(p, origin)).collect();
        let mut polygons = vec![];
        for opposite in 0..4 {
            let mut face: Vec<_> = (0..4).filter(|&i| i != opposite).collect();
            let normal = cross(
                sub(vertices[face[1]], vertices[face[0]]),
                sub(vertices[face[2]], vertices[face[0]]),
            );
            if dot(normal, sub(vertices[opposite], vertices[face[0]])) > 0. {
                face.reverse();
            }
            polygons.push(face);
        }
        Self::from_faces(origin, vertices, polygons, None, Some(points))
    }
    fn from_faces(
        origin: [f64; 3],
        vertices: Vec<[f64; 3]>,
        polygons: Vec<Vec<usize>>,
        prism: Option<Prism>,
        tetra: Option<[[f64; 3]; 4]>,
    ) -> Result<Self> {
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
            prism,
            tetra,
            tetra_tolerance: 1e-10,
            far_samples: std::sync::Arc::new(std::array::from_fn(|_| std::sync::OnceLock::new())),
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

    /// Tighten the internal source/roundoff budget for an independent reference.
    /// Existing production defaults and all target/reciprocity guards remain.
    pub fn with_tighter_tetra_tolerance(mut self, tolerance: f64) -> Result<Self> {
        if self.tetra.is_none() || !tolerance.is_finite() || !(1e-14..=1e-10).contains(&tolerance) {
            return Err(error(
                "POINT_INPUT",
                "tetra source tolerance must tighten1e-10 within1e-14..1e-10",
            ));
        }
        self.tetra_tolerance = tolerance;
        Ok(self)
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
        let r: [[f64; 3]; 6] = std::array::from_fn(|i| {
            if i < self.vertices.len() {
                sub(self.vertices[i], local)
            } else {
                [0.; 3]
            }
        });
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
        let tolerance = if self.tetra.is_some() {
            self.tetra_tolerance
        } else {
            1e-7
        };
        if value.is_finite() && value > 0. && cancellation_ratio * 64. * f64::EPSILON <= tolerance {
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

    fn bounded_far_tetra(&self, point: [f64; 3], tolerance: f64) -> Result<Option<(f64, f64, usize)>> {
        let Some(tetra) = self.tetra else {
            return Ok(None);
        };
        let center = std::array::from_fn(|k| tetra.iter().map(|p| p[k]).sum::<f64>() / 4.);
        let radius = tetra
            .iter()
            .map(|&v| norm(sub(v, center)))
            .fold(0., f64::max)
            * (1. + 16. * f64::EPSILON);
        let r = sub(point, center);
        let distance = norm(r);
        let q = radius / distance * (1. + 32. * f64::EPSILON);
        if !q.is_finite() || q >= 1. {
            return Ok(None);
        }
        // A degree-d Cartesian polynomial becomes degree at most d+2
        // in each Duffy coordinate after including its Jacobian. An n-point
        // Gauss rule therefore integrates through d=2n-3 exactly.
        let rounding = 1024. * f64::EPSILON * (1. + q) / (1. - q).powi(2);
        let remainder_for = |order: usize| {
            2. * (1. + q) / (1. - q) * q.powi((2 * order - 2) as i32)
        };
        let Some(required) = (4..=10).find(|&n| remainder_for(n) + rounding <= tolerance) else {
            return Ok(None);
        };
        for required in required..=10 {
        let cache = &self.far_samples[required - 4];
        if cache.get().is_none() {
            let volume = super::mesh::tetra_coefficients(tetra)?.0;
            let mut points = Vec::with_capacity(required.pow(3));
            let mut weights = Vec::with_capacity(required.pow(3));
            tetra_integrate(tetra, volume, super::prism::cached_rule(required)?, |p, w| {
                if w <= 0. || !w.is_finite() {
                    return Err(error("TETRA_FAR_RANGE", "positive far cubature required"));
                }
                points.push(sub(p, center));
                weights.push(w);
                Ok(())
            })?;
            let mass = weights.iter().fold(super::accurate::Sum::default(), |s, &w| {
                s.add(super::accurate::Sum::from(w))
            }).value();
            let _ = cache.set(FarSamples {
                order: required, center, radius, volume, points, weights,
                mass_error: (mass - volume).abs() / volume,
            });
        }
        let samples = cache.get().unwrap();
        let bound = remainder_for(samples.order) + rounding + samples.mass_error * (1. + q) / (1. - q);
        if bound > tolerance {
            continue;
        }
        let r = sub(point, samples.center);
        let mut sum = super::accurate::Sum::default();
        for (&p, &w) in samples.points.iter().zip(&samples.weights) {
            let delta = sub(r, p);
            // Three squares, two additions and sqrt add only a few epsilons
            // for a normal finite sum, within the 1024-epsilon allowance.
            // Preserve hypot for underflow and overflow.
            let squared = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
            let d = if squared.is_normal() { squared.sqrt() } else { norm(delta) };
            if d <= 0. || !d.is_finite() {
                return Err(error(
                    "TETRA_FAR_RANGE",
                    "strict far source separation required",
                ));
            }
            sum = sum.add(super::accurate::Sum::from(1. / d).mul(w));
        }
        let value = sum.value();
        if value <= 0. || !value.is_finite() || samples.radius <= 0. || samples.volume <= 0. {
            return Err(error(
                "TETRA_FAR_RANGE",
                "positive finite bounded far source required",
            ));
        }
        return Ok(Some((value, bound, samples.order)));
        }
        Ok(None)
    }
    fn positive_source(
        &self,
        point: [f64; 3],
        cancellation_ratio: f64,
    ) -> Result<PotentialEvaluation> {
        let tolerance = if self.tetra.is_some() {
            self.tetra_tolerance
        } else {
            1e-5
        };
        if let Some((value, bound, order)) = self.bounded_far_tetra(point, tolerance)? {
            return Ok(PotentialEvaluation {
                value_m2: value,
                cancellation_ratio,
                source_order: order,
                source_relative_change: bound,
            });
        }
        let mut previous = self.positive_rule(point, 7)?;
        let mut samples = vec![(7, previous)];
        for order in [15, 31, 63] {
            let value = self.positive_rule(point, order)?;
            let change = (value - previous).abs() / value;
            samples.push((order, value));
            if change <= tolerance {
                return Ok(PotentialEvaluation {
                    value_m2: value,
                    cancellation_ratio,
                    source_order: order,
                    source_relative_change: change,
                });
            }
            previous = value;
        }
        Err(error("POLYHEDRON_SOURCE_QUADRATURE", &format!("positive source rule did not meet explicit{tolerance:e} refinement estimate through order63; point={point:?}, tetra={:?}, samples={samples:?}; no unchecked fallback",self.tetra)))
    }

    fn positive_rule(&self, point: [f64; 3], order: usize) -> Result<f64> {
        if let Some(tetra) = self.tetra {
            return positive_tetra_potential(tetra, point, order);
        }
        let prism = self.prism.as_ref().unwrap();
        let rule = super::prism::cached_rule(order)?;
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
        let z0 = prism.z_m[0] - point[2];
        let z1 = prism.z_m[1] - point[2];
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

/// Positive Duffy cubature. For an interior query the radial integral is exact
/// after splitting into four query-to-face tetrahedra. This independent source
/// construction also supplies a fallback when face/edge summation is unresolved.
pub fn positive_tetra_potential_duffy(
    tetra: [[f64; 3]; 4],
    point: [f64; 3],
    order: usize,
) -> Result<f64> {
    let (volume, gradients) = super::mesh::tetra_coefficients(tetra)?;
    let delta = sub(point, tetra[0]);
    let mut weights = [0.; 4];
    for k in 1..4 {
        weights[k] = dot(gradients[k], delta);
    }
    weights[0] = 1. - weights[1] - weights[2] - weights[3];
    let rule = super::prism::cached_rule(order)?;
    if weights.iter().all(|&x| x >= 0.) {
        let mut value = 0.;
        for skip in 0..4 {
            let face: Vec<_> = (0..4)
                .filter(|&i| i != skip)
                .map(|i| sub(tetra[i], point))
                .collect();
            let determinant = dot(face[0], cross(face[1], face[2])).abs();
            if determinant == 0. {
                continue;
            }
            for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
                for (&v, &wv) in rule.nodes.iter().zip(&rule.weights) {
                    let direction = std::array::from_fn(|i| {
                        u * face[0][i]
                            + (1. - u) * v * face[1][i]
                            + (1. - u) * (1. - v) * face[2][i]
                    });
                    value += determinant * 0.5 * wu * wv * (1. - u) / norm(direction);
                }
            }
        }
        if value > 0. && value.is_finite() {
            return Ok(value);
        }
        return Err(error(
            "TETRA_SOURCE_RANGE",
            "positive interior tetra potential overflow",
        ));
    }
    let mut value = 0.;
    tetra_integrate(tetra, volume, &rule, |p, w| {
        let distance = norm(sub(p, point));
        if distance == 0. {
            return Err(error(
                "TETRA_SOURCE_LIMIT",
                "quadrature hit an exterior singularity",
            ));
        }
        value += w / distance;
        Ok(())
    })?;
    if value > 0. && value.is_finite() {
        Ok(value)
    } else {
        Err(error(
            "TETRA_SOURCE_RANGE",
            "positive exterior tetra potential overflow",
        ))
    }
}

/// Positive finite tetra source rule with the longest segment integrated
/// analytically. The remaining simplex fan has a fixed positive Jacobian;
/// fourth-power grading resolves logarithmic angular/radial endpoint limits.
pub fn positive_tetra_potential(
    tetra: [[f64; 3]; 4],
    point: [f64; 3],
    order: usize,
) -> Result<f64> {
    let (volume, _) = super::mesh::tetra_coefficients(tetra)?;
    let (ia, ib) = (0..4)
        .flat_map(|i| (i + 1..4).map(move |j| (i, j)))
        .max_by(|&(i, j), &(k, l)| {
            norm(sub(tetra[i], tetra[j])).total_cmp(&norm(sub(tetra[k], tetra[l])))
        })
        .unwrap();
    let other: Vec<_> = (0..4).filter(|&i| i != ia && i != ib).collect();
    let a = tetra[ia];
    let edge = sub(tetra[ib], a);
    let length = norm(edge);
    let e = edge.map(|x| x / length);
    let c = sub(tetra[other[0]], a);
    let d = sub(tetra[other[1]], a);
    let query = sub(point, a);
    let v = cross(e, c);
    let w = cross(e, d);
    let q = cross(e, query);
    let determinant = dot(e, cross(v, w));
    if determinant == 0. || !determinant.is_finite() {
        return Err(error(
            "TETRA_SOURCE_RANGE",
            "projected source simplex is unresolved; no volume removed",
        ));
    }
    let projected = [
        dot(e, cross(q, w)) / determinant,
        dot(e, cross(v, q)) / determinant,
    ];
    let vertices = [[0., 0.], [1., 0.], [0., 1.]];
    let physical = |p: [f64; 2]| std::array::from_fn::<_, 3, _>(|k| p[0] * v[k] + p[1] * w[k]);
    let barycentric_vertices = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let apex_full = if projected[0] >= 0. && projected[1] >= 0. && projected[0] + projected[1] <= 1.
    {
        [
            1. - (projected[0] + projected[1]),
            projected[0],
            projected[1],
        ]
    } else {
        let mut best = ([1., 0., 0.], f64::INFINITY);
        for i in 0..3 {
            let start = physical(vertices[i]);
            let finish = physical(vertices[(i + 1) % 3]);
            let delta = sub(finish, start);
            let squared = dot(delta, delta);
            if squared == 0. {
                return Err(error("TETRA_SOURCE_RANGE", "projected edge is unresolved"));
            }
            let t = (dot(sub(q, start), delta) / squared).clamp(0., 1.);
            let candidate = std::array::from_fn(|k| {
                (1. - t) * barycentric_vertices[i][k] + t * barycentric_vertices[(i + 1) % 3][k]
            });
            let distance = norm(sub(physical([candidate[1], candidate[2]]), q));
            if distance < best.1 {
                best = (candidate, distance);
            }
        }
        best.0
    };
    let apex = [apex_full[1], apex_full[2]];
    let ray = |lambda: [f64; 3]| -> Result<f64> {
        let fraction = lambda[0];
        if fraction <= 0. {
            return Err(error(
                "TETRA_SOURCE_RANGE",
                "source quadrature left positive barycentric simplex",
            ));
        }
        let start = std::array::from_fn(|k| -query[k] + lambda[1] * c[k] + lambda[2] * d[k]);
        let t = dot(start, e);
        let rho = norm(cross(e, start));
        let line_length = fraction * length;
        let end = t + line_length;
        let ratio = line_length / (rho.hypot(t) + rho.hypot(end));
        let value = if ratio <= 0.5 {
            2. * ratio.atanh()
        } else if rho > 0. {
            if t >= 0. {
                (end / rho).asinh() - (t / rho).asinh()
            } else if end <= 0. {
                (-t / rho).asinh() - (-end / rho).asinh()
            } else {
                (end / rho).asinh() + (-t / rho).asinh()
            }
        } else if t * end > 0. {
            (end.abs() / t.abs()).ln().abs()
        } else {
            f64::INFINITY
        };
        if value > 0. && value.is_finite() {
            Ok(value)
        } else {
            Err(error(
                "TETRA_SOURCE_LIMIT",
                "unresolved positive analytic source segment",
            ))
        }
    };
    let rule = super::prism::cached_rule(order)?;
    let mut value = super::accurate::Sum::default();
    let endpoints = |lambda: [f64; 3]| {
        let start = -dot(query, e) + lambda[1] * dot(c, e) + lambda[2] * dot(d, e);
        [start, start + lambda[0] * length]
    };
    let graded = |mut cuts: Vec<f64>| {
        cuts.retain(|x| x.is_finite() && *x >= 0. && *x <= 1.);
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        let mut result = vec![];
        for interval in cuts.windows(2) {
            let half = (interval[1] - interval[0]) / 2.;
            if half > 0. {
                result.extend([(interval[0], half), (interval[1], -half)]);
            }
        }
        result
    };
    for edge in 0..3 {
        let first = vertices[edge];
        let second = vertices[(edge + 1) % 3];
        let da = [first[0] - apex[0], first[1] - apex[1]];
        let db = [second[0] - apex[0], second[1] - apex[1]];
        // On the unit simplex this determinant is exactly the opposite
        // barycentric coordinate, including a truly zero boundary fan.
        let jacobian = apex_full[(edge + 2) % 3];
        if jacobian == 0. {
            continue;
        }
        let pa = physical(da);
        let pb = physical(db);
        let delta = sub(pb, pa);
        let squared = dot(delta, delta);
        let minimum = (-dot(pa, delta) / squared).clamp(0., 1.);
        let mut cuts = vec![0., 1., minimum];
        let first_endpoint = endpoints(barycentric_vertices[edge]);
        let second_endpoint = endpoints(barycentric_vertices[(edge + 1) % 3]);
        for k in 0..2 {
            let delta = second_endpoint[k] - first_endpoint[k];
            if delta != 0. {
                cuts.push(-first_endpoint[k] / delta);
            }
        }
        for (begin, delta) in graded(cuts) {
            for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
                let displacement = delta * u.powi(4);
                let s = begin + displacement;
                let one_minus_s = (1. - begin) - displacement;
                let ds = 4. * delta.abs() * u.powi(3);
                let edge_barycentric: [f64; 3] = std::array::from_fn(|k| {
                    one_minus_s * barycentric_vertices[edge][k]
                        + s * barycentric_vertices[(edge + 1) % 3][k]
                });
                // The analytic segment also changes rapidly where either
                // endpoint crosses the query's axial coordinate. Split there
                // independently of the perpendicular distance minimum.
                let apex_endpoints = endpoints(apex_full);
                let edge_endpoints = endpoints(edge_barycentric);
                let mut radial_cuts = vec![0., 1.];
                for k in 0..2 {
                    let delta = edge_endpoints[k] - apex_endpoints[k];
                    if delta != 0. {
                        radial_cuts.push(-apex_endpoints[k] / delta);
                    }
                }
                for (radial_begin, radial_delta) in graded(radial_cuts) {
                    for (&t, &wt) in rule.nodes.iter().zip(&rule.weights) {
                        // Evaluate every barycentric coordinate positively. Computing
                        // lambdaA=1-lambdaC-lambdaD erases thin boundary rays.
                        let displacement = radial_delta * t.powi(4);
                        let radius = radial_begin + displacement;
                        let one_minus_radius = (1. - radial_begin) - displacement;
                        let lambda = std::array::from_fn(|k| {
                            one_minus_radius * apex_full[k] + radius * edge_barycentric[k]
                        });
                        value = value.add(super::accurate::Sum::from(ray(lambda)?).mul(
                            wu * wt * jacobian * ds * radius * 4. * radial_delta.abs() * t.powi(3),
                        ));
                    }
                }
            }
        }
    }
    let result = value.mul(6. * volume / length).value();
    if result > 0. && result.is_finite() {
        Ok(result)
    } else {
        Err(error(
            "TETRA_SOURCE_RANGE",
            "positive line-integrated tetra source overflow",
        ))
    }
}

#[doc(hidden)]
pub fn tetra_integrate(
    t: [[f64; 3]; 4],
    volume: f64,
    rule: &GaussRule,
    mut f: impl FnMut([f64; 3], f64) -> Result<()>,
) -> Result<()> {
    for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
        for (&v, &wv) in rule.nodes.iter().zip(&rule.weights) {
            for (&w, &ww) in rule.nodes.iter().zip(&rule.weights) {
                let p = std::array::from_fn(|i| {
                    t[0][i]
                        + u * (t[1][i] - t[0][i])
                        + (1. - u) * v * (t[2][i] - t[0][i])
                        + (1. - u) * (1. - v) * w * (t[3][i] - t[0][i])
                });
                f(p, 6. * volume * wu * wv * ww * (1. - u).powi(2) * (1. - v))?;
            }
        }
    }
    Ok(())
}
pub fn tetra_integral(target: [[f64; 3]; 4], source: &PointPotential, order: usize) -> Result<f64> {
    let (volume, _) = super::mesh::tetra_coefficients(target)?;
    let rule = super::prism::cached_rule(order)?;
    let mut result = 0.;
    tetra_integrate(target, volume, &rule, |p, w| {
        result += w * source.at(p)?;
        Ok(())
    })?;
    if result > 0. && result.is_finite() {
        Ok(result)
    } else {
        Err(error(
            "TETRA_INTEGRAL_RANGE",
            "finite positive tetra integral required",
        ))
    }
}

struct TetraFarMoments {
    center: [f64; 3],
    radius: f64,
}

fn tetra_far_moments(tetra: [[f64; 3]; 4]) -> Result<TetraFarMoments> {
    super::mesh::tetra_coefficients(tetra)?;
    let center = std::array::from_fn(|axis| tetra.iter().map(|p| p[axis]).sum::<f64>() / 4.);
    let centered = tetra.map(|p| sub(p, center));
    let radius = centered.iter().map(|&p| norm(p)).fold(0., f64::max) * (1. + 32. * f64::EPSILON);
    Ok(TetraFarMoments { center, radius })
}

/// Reused positive tensor cubature on one unchanged tetrahedron.
pub struct TetraCubature {
    center: [f64; 3],
    radius: f64,
    points: Vec<[f64; 3]>,
    weights: Vec<f64>,
    axial_lines: Option<Vec<AxialLine>>,
    axial_refined: Option<Vec<AxialLine>>,
    axial_retry: Option<Vec<AxialLine>>,
}

#[derive(Clone, Copy)]
struct AxialLine { origin: [f64; 3], center: [f64; 3], half: f64, weight: f64 }

pub(super) struct AxialDetail { pub value:Option<(f64,f64)>, pub reason:Option<usize>, pub coarse:Option<f64>, pub fine:Option<f64> }
impl AxialDetail { fn fallback(reason:usize,coarse:Option<f64>,fine:Option<f64>)->Self { Self { value:None,reason:Some(reason),coarse,fine } } }
impl TetraCubature {
    pub fn new(tetra: [[f64; 3]; 4], order: usize) -> Result<Self> {
        let mut result = Self::geometry_only(tetra)?;
        result.prepare_direct(tetra, order)?;
        Ok(result)
    }
    /// Geometry required for separation and axial rules, without direct samples.
    pub(super) fn geometry_only(tetra: [[f64; 3]; 4]) -> Result<Self> {
        let moments = tetra_far_moments(tetra)?;
        Ok(Self { center: moments.center, radius: moments.radius, points: Vec::new(), weights: Vec::new(), axial_lines: None, axial_refined: None, axial_retry: None })
    }
    /// Prepare the original positive direct rule once, committing only on success.
    pub(super) fn prepare_direct(&mut self, tetra: [[f64; 3]; 4], order: usize) -> Result<bool> {
        if !self.points.is_empty() {
            return if order.checked_pow(3) == Some(self.points.len()) { Ok(false) } else {
                Err(error("TETRA_INTEGRAL_ORDER", "direct cubature already prepared at another order"))
            };
        }
        let (volume, _) = super::mesh::tetra_coefficients(tetra)?;
        let rule = super::prism::cached_rule(order)?;
        let mut points = Vec::with_capacity(order.pow(3));
        let mut weights = Vec::with_capacity(order.pow(3));
        tetra_integrate(tetra, volume, rule, |point, weight| {
            points.push(point);
            weights.push(weight);
            Ok(())
        })?;
        self.points = points;
        self.weights = weights;
        Ok(true)
    }
    #[cfg(test)]
    pub(super) fn direct_point_count(&self) -> usize { self.points.len() }
    /// Opt-in exact vertical-edge line cubature. Original points remain available for fallback.
    pub fn new_axial(tetra: [[f64; 3]; 4], order: usize) -> Result<Self> {
        let mut result=Self::new(tetra,order)?;
        result.prepare_axial(tetra,order)?;
        Ok(result)
    }
    /// Attach line rules for the same tetrahedron and order used by `new`.
    pub fn prepare_axial(&mut self, tetra: [[f64; 3]; 4], order: usize) -> Result<()> {
        let coarse=Self::axial_line_rule(tetra,order)?;
        let fine=if coarse.is_some() {
            Self::axial_line_rule(tetra,order.checked_add(2).ok_or_else(||error("QUADRATURE_ORDER","order overflow"))?)?
        } else {None};
        self.axial_lines=coarse;
        self.axial_refined=fine;
        self.axial_retry=None;
        Ok(())
    }
    /// Lazily prepare the n+4 line rule for the same tetrahedron and original order.
    pub(super) fn prepare_axial_retry(&mut self, tetra:[[f64;3];4], order:usize)->Result<()> {
        if self.axial_retry.is_none() && self.axial_refined.is_some() {
            self.axial_retry=Self::axial_line_rule(tetra,order.checked_add(4).ok_or_else(||error("QUADRATURE_ORDER","order overflow"))?)?;
        }
        Ok(())
    }
    /// Retry only an outer-convergence refusal, reusing its already evaluated n+2 result.
    pub(super) fn try_axial_retry(&self, other:&Self, previous:&AxialDetail)->Result<AxialDetail> {
        if previous.reason!=Some(3) {return Ok(AxialDetail::fallback(0,None,None));}
        let Some(coarse)=previous.fine else {return Ok(AxialDetail::fallback(0,None,None))};
        let (Some(a),Some(b))=(&self.axial_retry,&other.axial_retry) else {return Ok(AxialDetail::fallback(0,Some(coarse),None))};
        let (fine,err)=match Self::axial_lines_integral(a,b) {Ok(v)=>v,Err(reason)=>return Ok(AxialDetail::fallback(reason,Some(coarse),None))};
        if (coarse-fine).abs()>1e-10*fine {return Ok(AxialDetail::fallback(3,Some(coarse),Some(fine)))}
        Ok(AxialDetail{value:Some((fine,err)),reason:None,coarse:Some(coarse),fine:Some(fine)})
    }
    fn axial_line_rule(tetra: [[f64; 3]; 4], order: usize) -> Result<Option<Vec<AxialLine>>> {
        let mut edge = None;
        for i in 0..4 { for j in i+1..4 {
            if tetra[i][0] == tetra[j][0] && tetra[i][1] == tetra[j][1] && tetra[i][2] != tetra[j][2] { edge=Some((i,j)); break; }
        } if edge.is_some() { break; } }
        let Some((i,j))=edge else { return Ok(None); };
        let rest: Vec<_>=(0..4).filter(|&k| k!=i && k!=j).collect();
        let t=[tetra[i],tetra[rest[0]],tetra[rest[1]],tetra[j]];
        let (volume,_)=super::mesh::tetra_coefficients(tetra)?;
        let rule=super::prism::cached_rule(order)?;
        let count=order.checked_mul(order).ok_or_else(||error("MEMORY_LIMIT","axial line count overflow"))?;
        let mut lines=Vec::new(); lines.try_reserve_exact(count).map_err(|_|error("MEMORY_LIMIT","axial line allocation failed"))?;
        for (&u,&wu) in rule.nodes.iter().zip(&rule.weights) { for (&v,&wv) in rule.nodes.iter().zip(&rule.weights) {
            let base: [f64;3]=std::array::from_fn(|k| u*(t[1][k]-t[0][k])+(1.-u)*v*(t[2][k]-t[0][k]));
            let dz=(1.-u)*(1.-v)*(t[3][2]-t[0][2]);
            let line=AxialLine {origin:t[0],center:[base[0],base[1],base[2]+dz/2.],half:dz.abs()/2.,weight:6.*volume*wu*wv*(1.-u).powi(2)*(1.-v)};
            if !line.center.iter().chain([&line.half,&line.weight]).all(|x|x.is_finite()) || line.half<=0. || line.weight<=0. { return Ok(None); }
            lines.push(line);
        }}
        Ok(Some(lines))
    }
    /// Experimental estimate; outer refinement and roundoff are not certified error bounds.
    pub fn try_axial_integral(&self, other: &Self) -> Result<Option<(f64,f64)>> {
        Ok(self.try_axial_detailed(other)?.value)
    }
    pub(super) fn try_axial_detailed(&self, other: &Self) -> Result<AxialDetail> {
        let (Some(a),Some(b),Some(ar),Some(br))=(&self.axial_lines,&other.axial_lines,&self.axial_refined,&other.axial_refined) else {return Ok(AxialDetail::fallback(0,None,None))};
        let coarse=match Self::axial_lines_integral(a,b) { Ok(v)=>v, Err(reason)=>return Ok(AxialDetail::fallback(reason,None,None)) };
        let (fine,err)=match Self::axial_lines_integral(ar,br) { Ok(v)=>v, Err(reason)=>return Ok(AxialDetail::fallback(reason,Some(coarse.0),None)) };
        if (coarse.0-fine).abs()>1e-10*fine {return Ok(AxialDetail::fallback(3,Some(coarse.0),Some(fine)))}
        Ok(AxialDetail { value:Some((fine,err)), reason:None, coarse:Some(coarse.0), fine:Some(fine) })
    }
    fn axial_lines_integral(a: &[AxialLine], b: &[AxialLine]) -> std::result::Result<(f64,f64),usize> {
        if a.len()!=b.len() {return Err(2)}
        let mut total=super::accurate::Sum::default(); let mut error_sum=0.;
        for x in a { let mut row=0.; for y in b {
            let d: [f64;3]=std::array::from_fn(|i| super::accurate::Sum::from(x.origin[i]).sub(super::accurate::Sum::from(y.origin[i])).add(super::accurate::Sum::from(x.center[i])).sub(super::accurate::Sum::from(y.center[i])).value()); let r=norm(d); let q=(x.half+y.half)/r*(1.+16.*f64::EPSILON);
            if !r.is_finite() || r<=0. || !q.is_finite() {return Err(2)} if q>0.25 {return Err(1)}
            let aa=x.half/r; let bb=y.half/r; let z=d[2]/r;
            let mut p0=1.; let mut p1=z; let mut sum=1.; let mut abs_sum=1.; let mut tail=q*q/(1.-q*q); let mut degree=0usize;
            let u=(aa+bb).powi(2); let v=(aa-bb).powi(2); let mut moment_poly=1.; let mut vpower=1.; let mut qpower=q*q;
            for n in 2..=24 { let pn=((2*n-1) as f64*z*p1-(n-1) as f64*p0)/(n as f64); p0=p1; p1=pn;
                if n%2==0 { let k=n/2; vpower*=v; moment_poly=u*moment_poly+vpower;
                    let moment=moment_poly/((2*k+1)*(k+1)) as f64;
                    let term=pn*moment; sum+=term; abs_sum+=term.abs(); degree=n; qpower*=q*q; tail=qpower/(1.-q*q); if tail<1e-14 {break;}
                }
            }
            let allowance=(tail+128.*f64::EPSILON*(degree+1) as f64*abs_sum)/r;
            let avg=sum/r; if !avg.is_finite() || avg<=0. || !allowance.is_finite() || allowance>1e-12*avg {return Err(2)}
            row+=y.weight*avg; error_sum+=x.weight*y.weight*allowance;
        } total=total.add(super::accurate::Sum::from(x.weight*row)); }
        let value=total.value(); if !value.is_finite() || value<=0. || !error_sum.is_finite() {return Err(2)} Ok((value,error_sum))
    }
    pub fn separation_q(&self, other: &Self) -> f64 {
        (self.radius + other.radius) / norm(sub(self.center, other.center)) * (1. + 64. * f64::EPSILON)
    }
    pub fn direct_integral(&self, other: &Self) -> Result<f64> {
        if self.points.is_empty() || other.points.is_empty() {
            return Err(error("TETRA_INTEGRAL_ORDER", "direct pair cubature is not prepared"));
        }
        if self.points.len() != other.points.len() {
            return Err(error("TETRA_INTEGRAL_ORDER", "direct pair cubature orders must match"));
        }
        let mut total = super::accurate::Sum::default();
        for (p, &wp) in self.points.iter().zip(&self.weights) {
            // All terms are positive. Accumulate one target row directly, then
            // compensate only the outer sum instead of an error-free transform
            // for every point pair in the hot kernel.
            let mut row = 0.;
            for (s, &ws) in other.points.iter().zip(&other.weights) {
                let delta = sub(*p, *s);
                let distance = norm(delta);
                if distance <= 0. || !distance.is_finite() {
                    return Err(error("TETRA_INTEGRAL_RANGE", "separated direct cubature intersects or overflows"));
                }
                row += ws / distance;
            }
            total = total.add(super::accurate::Sum::from(wp * row));
        }
        let value = total.value();
        if value > 0. && value.is_finite() { Ok(value) } else { Err(error("TETRA_INTEGRAL_RANGE", "finite positive direct pair integral required")) }
    }
}
pub(crate) fn separated_order(q: f64, maximum: usize) -> Option<usize> {
    let order = if q.is_finite() && q < 0.12 { 4 } else if q < 0.6 { 6 } else if q < 1. { 8 } else { return None };
    (order <= maximum).then_some(order)
}

/// Smooth separated pairs use direct positive double cubature of 1/|r-r'|.
/// The order grows with the geometric separation ratio; near pairs retain the
/// analytic source path. Independent fixed-order audits remain mandatory.
fn tetra_far_integral(target: [[f64; 3]; 4], source: &PointPotential, maximum: usize) -> Result<Option<f64>> {
    let Some(source_tetra) = source.tetra else { return Ok(None); };
    let a = tetra_far_moments(target)?;
    let b = tetra_far_moments(source_tetra)?;
    let q = (a.radius + b.radius) / norm(sub(a.center, b.center)) * (1. + 64. * f64::EPSILON);
    let Some(order) = separated_order(q, maximum) else { return Ok(None); };
    Ok(Some(TetraCubature::new(target, order)?.direct_integral(&TetraCubature::new(source_tetra, order)?)?))
}

#[cfg(test)]
mod far_pair_tests {
    use super::*;
    fn tetra(offset: f64) -> [[f64; 3]; 4] {
        [[offset, 0., 0.], [offset + 0.001, 0., 0.], [offset, 0.001, 0.], [offset, 0., 0.001]]
    }
    #[test]
    fn separated_double_cubature_converges_and_preserves_reciprocity() {
        let a = tetra(0.);
        for offset in [0.1, 0.01, 0.005, 0.004, 0.0036] {
            let b = tetra(offset);
            let source_b = PointPotential::tetrahedron(b).unwrap();
            let source_a = PointPotential::tetrahedron(a).unwrap();
            let ab = tetra_far_integral(a, &source_b, 16).unwrap().unwrap();
            let ba = tetra_far_integral(b, &source_a, 16).unwrap().unwrap();
            let reference = tetra_integral(a, &source_b, 16).unwrap();
            assert!((ab - ba).abs() / ab < 1e-12);
            assert!((ab - reference).abs() / reference < 1e-10, "offset {offset}: {ab} vs {reference}");
        }
        assert!(tetra_far_integral(a, &PointPotential::tetrahedron(tetra(0.001)).unwrap(), 12).unwrap().is_none());
    }
    #[test]
    fn measure_direct_order_errors_near_separation_limit() {
        let a = tetra(0.);
        let ca = TetraCubature::new(a, 16).unwrap();
        for offset in [0.005, 0.004, 0.0036, 0.003, 0.0025, 0.0022] {
            let b = tetra(offset);
            let cb = TetraCubature::new(b, 16).unwrap();
            let reference = tetra_integral(a, &PointPotential::tetrahedron(b).unwrap(), 16).unwrap();
            let q = ca.separation_q(&cb);
            let order = separated_order(q, 16).unwrap();
            let direct = TetraCubature::new(a, order).unwrap().direct_integral(&TetraCubature::new(b, order).unwrap()).unwrap();
            let relative_error = (direct - reference).abs() / reference;
            eprintln!("offset={offset} q={q} order={order} relativeError={relative_error:e}");
            assert!(relative_error < 1e-10, "offset={offset} order={order} relativeError={relative_error:e}");
        }
    }
}

/// Adapt only the outer target cubature; the source potential retains its own
/// analytic/positive near-field guards. Cross-block reduced modes use 1e-12;
/// local blocks may use 1e-10 only with independent fixed-order/physical gates.
pub fn tetra_integral_adaptive(target: [[f64; 3]; 4], source: &PointPotential, maximum: usize, tolerance: f64) -> Result<f64> {
    if !(8..=16).contains(&maximum) || !tolerance.is_finite() || !(1e-12..=1e-10).contains(&tolerance) {
        return Err(error("TETRA_INTEGRAL_ORDER", "maximum target order 8..16 and tolerance 1e-12..1e-10 required"));
    }
    if let Some(value) = tetra_far_integral(target, source, maximum)? {
        return Ok(value);
    }
    let mut previous: Option<f64> = None;
    for step in [2, 4, 8, 12, 16] {
        let order = step.min(maximum);
        let value = tetra_integral(target, source, order)?;
        if let Some(old) = previous {
            let change = (value - old).abs() / value;
            if change.is_finite() && change <= tolerance {
                return Ok(value);
            }
        }
        previous = Some(value);
        if order == maximum { break; }
    }
    previous.ok_or_else(|| error("TETRA_INTEGRAL_ORDER", "target cubature produced no value"))
}

/// Positive two-ended Z grading on an unchanged original layered tetrahedron.
/// The exact section fan and t=u^4/2 mappings retain volume and first moments.
pub fn tetra_integral_z_graded(
    target: [[f64; 3]; 4],
    source: &PointPotential,
    order: usize,
    start: std::time::Instant,
    limit: std::time::Duration,
) -> Result<f64> {
    let (volume, _) = super::mesh::tetra_coefficients(target)?;
    let z0 = target.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let z1 = target
        .iter()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    if z1 <= z0 || target.iter().any(|p| p[2] != z0 && p[2] != z1) {
        return Err(error(
            "TETRA_GRADED_INPUT",
            "two original layered Z planes required",
        ));
    }
    let lower = target.iter().filter(|p| p[2] == z0).collect::<Vec<_>>();
    let upper = target.iter().filter(|p| p[2] == z1).collect::<Vec<_>>();
    let rule = super::prism::cached_rule(order)?;
    let mut total = super::accurate::Sum::default();
    let mut mass = super::accurate::Sum::default();
    let mut moments = [super::accurate::Sum::default(); 3];
    let mut samples = 0usize;
    for side in 0..2 {
        for (&u, &wu) in rule.nodes.iter().zip(&rule.weights) {
            let t = if side == 0 {
                0.5 * u.powi(4)
            } else {
                1. - 0.5 * u.powi(4)
            };
            let jac = 2. * u.powi(3) * (z1 - z0);
            let z = z0 + (z1 - z0) * t;
            let mut section = Vec::with_capacity(4);
            for a in &lower {
                for b in &upper {
                    section.push([a[0] * (1. - t) + b[0] * t, a[1] * (1. - t) + b[1] * t, z]);
                }
            }
            let centre = [
                section.iter().map(|p| p[0]).sum::<f64>() / section.len() as f64,
                section.iter().map(|p| p[1]).sum::<f64>() / section.len() as f64,
            ];
            section.sort_by(|a, b| {
                (a[1] - centre[1])
                    .atan2(a[0] - centre[0])
                    .total_cmp(&(b[1] - centre[1]).atan2(b[0] - centre[0]))
            });
            for k in 1..section.len() - 1 {
                let triangle = [section[0], section[k], section[k + 1]];
                // Exact affine section Jacobian; computing it by subtracting
                // nearly coincident endpoint-section vertices loses tiny weights.
                let det = 6. * volume / (z1 - z0)
                    * match lower.len() {
                        3 => (1. - t).powi(2),
                        1 => t * t,
                        2 => t * (1. - t),
                        _ => {
                            return Err(error("TETRA_GRADED_INPUT", "unsupported layered section"))
                        }
                    };
                if det <= 0. || !det.is_finite() {
                    return Err(error(
                        "TETRA_GRADED_RANGE",
                        "positive exact section required",
                    ));
                }
                for (&v, &wv) in rule.nodes.iter().zip(&rule.weights) {
                    for (&w, &ww) in rule.nodes.iter().zip(&rule.weights) {
                        samples += 1;
                        if samples % 128 == 0 && start.elapsed() > limit {
                            return Err(error(
                                "DEADLINE",
                                "positive layered target grading deadline",
                            ));
                        }
                        let lambda = [1. - v, v * (1. - w), v * w];
                        let point = std::array::from_fn(|axis| {
                            (0..3).map(|i| lambda[i] * triangle[i][axis]).sum::<f64>()
                        });
                        let weight = jac * wu * det * v * wv * ww;
                        if weight <= 0. || !weight.is_finite() {
                            return Err(error(
                                "TETRA_GRADED_RANGE",
                                "positive target weight required",
                            ));
                        }
                        mass = mass.add(super::accurate::Sum::from(weight));
                        for axis in 0..3 {
                            moments[axis] = moments[axis]
                                .add(super::accurate::Sum::from(point[axis]).mul(weight));
                        }
                        total =
                            total.add(super::accurate::Sum::from(source.at(point)?).mul(weight));
                    }
                }
            }
        }
    }
    if (mass.value() - volume).abs() > 1e-10 * volume {
        return Err(error("TETRA_GRADED_PARTITION", "target volume changed"));
    }
    for axis in 0..3 {
        let expected = target.iter().map(|p| p[axis]).sum::<f64>() * volume / 4.;
        let scale = target.iter().map(|p| p[axis].abs()).fold(0., f64::max) * volume;
        if (moments[axis].value() - expected).abs() > 1e-10 * scale.max(f64::MIN_POSITIVE) {
            return Err(error(
                "TETRA_GRADED_PARTITION",
                "target first moment changed",
            ));
        }
    }
    let value = total.value();
    if value <= 0. || !value.is_finite() {
        return Err(error(
            "TETRA_GRADED_RANGE",
            "positive finite graded integral required",
        ));
    }
    Ok(value)
}
/// Exact radial/segment reductions of the constant-density tetrahedron self
/// integral. Only one smooth, graded facet coordinate remains numerical.
pub fn tetra_self_integral(tetra: [[f64; 3]; 4], order: usize) -> Result<f64> {
    let (volume, _) = super::mesh::tetra_coefficients(tetra)?;
    let columns = [
        sub(tetra[1], tetra[0]),
        sub(tetra[2], tetra[0]),
        sub(tetra[3], tetra[0]),
    ];
    let mut facets = vec![(columns[2], columns[0], sub(columns[1], columns[2]), true)];
    for i in 0..3 {
        let others: Vec<_> = (0..3).filter(|&j| j != i).collect();
        facets.push((
            columns[i],
            sub(columns[i], columns[others[0]]),
            columns[others[1]].map(|x| -x),
            true,
        ));
        facets.push((
            columns[others[1]],
            columns[others[0]],
            columns[i].map(|x| -x),
            false,
        ));
    }
    let rule = super::prism::cached_rule(order)?;
    let mut total = super::accurate::Sum::default();
    for (start0, start1, direction, triangle) in facets {
        let length = norm(direction);
        let e = direction.map(|x| x / length);
        let perpendicular0 = cross(e, start0);
        let perpendicular1 = cross(e, start1);
        let delta = sub(perpendicular1, perpendicular0);
        let squared = dot(delta, delta);
        let mut cuts = vec![0., 1.];
        if squared > 0. {
            let minimum = (-dot(perpendicular0, delta) / squared).clamp(0., 1.);
            cuts.push(minimum);
            // A nearly planar physical tetrahedron has a logarithmic layer
            // much narrower than the facet interval. Its width is geometric:
            // rho(s)^2 = rho_min^2 + |delta|^2 (s-s_min)^2.
            // Resolve every dyadic scale rather than discard the thin volume
            // or infer convergence from a single fixed power grading.
            let nearest = std::array::from_fn(|k| {
                super::accurate::Sum::from(perpendicular0[k])
                    .add(super::accurate::Sum::from(delta[k]).mul(minimum))
                    .value()
            });
            let mut width = norm(nearest) / squared.sqrt();
            if width > 0. && width.is_finite() {
                for _ in 0..1075 {
                    let left = minimum - width;
                    let right = minimum + width;
                    if left > 0. && left < 1. {
                        cuts.push(left);
                    }
                    if right > 0. && right < 1. {
                        cuts.push(right);
                    }
                    if left <= 0. && right >= 1. {
                        break;
                    }
                    width *= 2.;
                }
            }
        }
        let first = dot(start0, e);
        let last = dot(start1, e);
        for (a, b) in [
            (first, last),
            (first + length, last + if triangle { 0. } else { length }),
        ] {
            if b != a {
                let root = -a / (b - a);
                if root > 0. && root < 1. {
                    cuts.push(root);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        for interval in cuts.windows(2) {
            let half = (interval[1] - interval[0]) / 2.;
            for (begin, delta) in [(interval[0], half), (interval[1], -half)] {
                for (&u, &weight) in rule.nodes.iter().zip(&rule.weights) {
                    let displacement = delta * u.powi(4);
                    let s = begin + displacement;
                    let complement = (1. - begin) - displacement;
                    let start = std::array::from_fn(|k| {
                        super::accurate::Sum::from(start0[k])
                            .mul(complement)
                            .add(super::accurate::Sum::from(start1[k]).mul(s))
                            .value()
                    });
                    let axial = dot(start, e);
                    let rho = norm(cross(e, start));
                    let segment = if triangle {
                        complement * length
                    } else {
                        length
                    };
                    let end = axial + segment;
                    let ratio = segment / (rho.hypot(axial) + rho.hypot(end));
                    let log = if ratio <= 0.5 {
                        2. * ratio.atanh()
                    } else if rho > 0. {
                        if axial >= 0. {
                            (end / rho).asinh() - (axial / rho).asinh()
                        } else if end <= 0. {
                            (-axial / rho).asinh() - (-end / rho).asinh()
                        } else {
                            (end / rho).asinh() + (-axial / rho).asinh()
                        }
                    } else if axial * end > 0. {
                        (end.abs() / axial.abs()).ln().abs()
                    } else {
                        f64::INFINITY
                    };
                    if log < 0. || !log.is_finite() {
                        return Err(error(
                            "TETRA_SELF_RANGE",
                            "finite positive self facet integral required",
                        ));
                    }
                    total = total.add(
                        super::accurate::Sum::from(log / length)
                            .mul(weight * 4. * delta.abs() * u.powi(3)),
                    );
                }
            }
        }
    }
    let value = total.mul(0.6 * volume * volume).value();
    if value > 0. && value.is_finite() {
        Ok(value)
    } else {
        Err(error(
            "TETRA_SELF_RANGE",
            "finite positive self integral required",
        ))
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

#[cfg(test)]
mod synthetic_near_profile {
    use super::*;
    /// Debug-only synthetic geometry; not an STM32 measurement or speedup claim.
    /// cargo test --manifest-path native/analyze/Cargo.toml --lib synthetic_near_profile::profile -- --ignored --nocapture
    #[test]
    #[ignore]
    fn profile() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        for offset in [0.00001, 0.0001, 0.001] {
            let a = [[0., 0., 0.], [0.002, 0., 0.], [0., 0.001, 0.], [0., 0., 0.000035]];
            let b = a.map(|p| [p[0] + offset, p[1], p[2] + 0.000035]);
            let src = PointPotential::tetrahedron(b).unwrap();
            let (volume, _) = super::super::mesh::tetra_coefficients(a).unwrap();
            for order in [2, 4, 8, 12, 16] {
                assert!(std::time::Instant::now() < deadline, "30-second profile deadline exceeded");
                let rule = super::super::prism::cached_rule(order).unwrap();
                let mut counts = [0usize; 2];
                let mut times = [std::time::Duration::ZERO; 2];
                let start = std::time::Instant::now();
                let mut value = 0.;
                tetra_integrate(a, volume, rule, |p, w| {
                    if std::time::Instant::now() >= deadline { panic!("30-second profile deadline exceeded"); }
                    let now = std::time::Instant::now();
                    let v = src.at_diagnostic(p)?;
                    let k = usize::from(v.source_order > 0);
                    counts[k] += 1;
                    times[k] += now.elapsed();
                    value += w * v.value_m2;
                    Ok(())
                }).unwrap();
                eprintln!("offset={offset} order={order} counts={counts:?} times={times:?} total={:?} value={value:e}", start.elapsed());
            }
        }
    }
}
#[cfg(test)]
mod captured_adaptive_comparison {
    use super::*;
    #[test]
    #[ignore]
    fn captured_pairs_match_fixed_order_ten() {
        let path = std::env::var("P7_PAIR_OUTPUT").unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        let started = std::time::Instant::now();
        let mut count = 0;
        let mut max_relative = 0_f64;
        for line in text.lines() {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let target: [[u64; 3]; 4] = serde_json::from_value(row["targetBits"].clone()).unwrap();
            let source: [[u64; 3]; 4] = serde_json::from_value(row["sourceBits"].clone()).unwrap();
            let source = PointPotential::tetrahedron(source.map(|p| p.map(f64::from_bits))).unwrap();
            let value = tetra_integral_adaptive(target.map(|p| p.map(f64::from_bits)), &source, 12, 1e-10).unwrap();
            let reference = f64::from_bits(row["valueBits"].as_u64().unwrap());
            let relative = (value - reference).abs() / reference.abs();
            max_relative = max_relative.max(relative);
            assert!(relative <= 1e-10, "pair {count}: relative {relative:e}");
            count += 1;
        }
        assert_eq!(count, 16);
        eprintln!("{count} captured pairs in {:.6}s; max relative {max_relative:e}", started.elapsed().as_secs_f64());
    }
}

#[cfg(test)]
mod far_source_guards {
    use super::*;
    #[test]
    fn rejected_mass_bound_tries_next_order_and_extreme_distance_stays_finite() {
        let tetra=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        let ordinary=PointPotential::tetrahedron(tetra).unwrap();
        let point=[1.,1.,1.];
        let reference=ordinary.bounded_far_tetra(point,1e-10).unwrap().unwrap();
        assert_eq!(reference.2,4);
        let samples=ordinary.far_samples[0].get().unwrap();
        let source=PointPotential::tetrahedron(tetra).unwrap();
        source.far_samples[0].set(FarSamples{order:4,center:samples.center,radius:samples.radius,
            volume:samples.volume,points:samples.points.clone(),weights:samples.weights.clone(),mass_error:1.}).unwrap();
        let recovered=source.bounded_far_tetra(point,1e-10).unwrap().unwrap();
        assert!(recovered.2>4&&recovered.1<=1e-10);
        assert!((recovered.0-reference.0).abs()/reference.0<1e-10);
        let extreme=ordinary.bounded_far_tetra([1e200,0.,0.],1e-10).unwrap().unwrap();
        assert!(extreme.0.is_finite()&&extreme.0>0.);
        assert!((extreme.0/(samples.volume/1e200)-1.).abs()<1e-12);
    }
}


#[cfg(test)]
mod axial_tetra_tests {
    use super::*;
    #[test]
    fn axial_tetra_controls() {
        let a=[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,0.01]];
        let b=[[2.,0.,0.1],[3.,0.,0.1],[2.,1.,0.1],[2.,0.,0.13]];
        for order in [6,8] {
            let mut x=TetraCubature::new(a,order).unwrap(); x.prepare_axial(a,order).unwrap(); let y=TetraCubature::new_axial(b,order).unwrap();
            let now=std::time::Instant::now(); let (v,e)=x.try_axial_integral(&y).unwrap().unwrap(); let axial=now.elapsed();
            let now=std::time::Instant::now(); let old=x.direct_integral(&y).unwrap(); let direct=now.elapsed();
            println!("order {order}: axial {axial:?}, direct {direct:?}, line pairs {}, point pairs {}, rel diff {}, bound {}",order.pow(4)+(order+2).pow(4),order.pow(6),(v-old).abs()/old,e/v);
            assert!((v-old).abs()<1e-10*old); assert!((y.try_axial_integral(&x).unwrap().unwrap().0-v).abs()<1e-12*v);
            let src=PointPotential::tetrahedron(b).unwrap(); let reference=tetra_integral(a,&src,16).unwrap(); assert!((v-reference).abs()<1e-7*reference);
            let mut at=a; at.swap(0,2); let perm=TetraCubature::new_axial(at,order).unwrap().try_axial_integral(&y).unwrap().unwrap().0; assert!((v-perm).abs()<1e-7*v);
            let shift=|mut t:[[f64;3];4]| {for p in &mut t {p[0]+=10.;p[1]-=3.;p[2]+=2.;} t};
            let shifted=TetraCubature::new_axial(shift(a),order).unwrap().try_axial_integral(&TetraCubature::new_axial(shift(b),order).unwrap()).unwrap().unwrap().0; assert!((v-shifted).abs()<1e-11*v);
            assert!(x.try_axial_integral(&x).unwrap().is_none());
        }
        let unsupported=[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.2,0.3,0.1]];
        assert!(TetraCubature::new_axial(unsupported,6).unwrap().axial_lines.is_none());
    }
}

#[cfg(test)]
mod axial_line_more_tests {
    use super::*;
    fn one(z:f64,half:f64)->TetraCubature { TetraCubature {center:[0.,0.,z],radius:half,points:vec![],weights:vec![],axial_lines:Some(vec![AxialLine{origin:[0.,0.,z],center:[0.;3],half,weight:1.}]),axial_refined:Some(vec![AxialLine{origin:[0.,0.,z],center:[0.;3],half,weight:1.}]),axial_retry:None} }
    #[test] fn detailed_reasons_and_wrapper_equivalence() {
        let a=one(0.,0.2); let b=one(3.,0.1);
        let normal=a.try_axial_detailed(&b).unwrap();
        assert_eq!(normal.value.unwrap().0.to_bits(),a.try_axial_integral(&b).unwrap().unwrap().0.to_bits());
        let mut unsupported=one(3.,0.1); unsupported.axial_lines=None;
        let mut invalid=one(3.,0.1); invalid.axial_lines.as_mut().unwrap()[0].origin[0]=f64::NAN;
        let mut outer=one(3.,0.1); outer.axial_refined.as_mut().unwrap()[0].origin[2]+=0.1;
        for (other,reason) in [(&unsupported,0),(&one(0.1,0.2),1),(&invalid,2),(&outer,3)] {
            let detail=a.try_axial_detailed(other).unwrap(); assert_eq!(detail.reason,Some(reason));
            assert!(detail.value.is_none()); assert!(a.try_axial_integral(other).unwrap().is_none());
        }
    }
    #[test] fn axial_line_disjoint_and_extremes() {
        let a=one(0.,0.2); let b=one(3.,0.1); let (v,e)=a.try_axial_integral(&b).unwrap().unwrap();
        let f=|s:f64| s*s.abs().ln()-s.abs();
        let exact=(f(3.3)+f(2.7)-f(3.1)-f(2.9))/0.08;
        println!("rho-zero axial average {v}, endpoint reference {exact}, bound {e}"); assert!((v-exact).abs()<2e-13*exact);
        assert!(a.try_axial_integral(&one(0.1,0.2)).unwrap().is_none());
        assert!(a.try_axial_integral(&one(1.,0.2)).unwrap().is_none());
    }
    #[test] fn moment_recurrence_and_endpoint() {
        for (a,b) in [(0.1_f64,0.07_f64),(0.1,0.1),(0.1,1e-9)] {
            let u=(a+b).powi(2); let v=(a-b).powi(2); let mut s=1.; let mut vp=1.;
            for k in 1..=12 {vp*=v; s=u*s+vp; let recurrence=s/((2*k+1)*(k+1)) as f64;
                let n=2*k; let mut bin=1.; let mut reference=0.; for j in 0..=n {if j%2==0 {reference+=bin*a.powi(j as i32)*b.powi((n-j) as i32)/((j+1)*(n-j+1)) as f64;} if j<n {bin*=(n-j) as f64/(j+1) as f64;}}
                assert!((recurrence-reference).abs()<2e-14*reference);
            }
        }
        let mut a=one(0.,0.2); let mut b=one(2.,0.1); for lines in [&mut a.axial_lines,&mut a.axial_refined] {lines.as_mut().unwrap()[0].origin[0]=1.;} for lines in [&mut b.axial_lines,&mut b.axial_refined] {lines.as_mut().unwrap()[0].origin[0]=2.;}
        let value=a.try_axial_integral(&b).unwrap().unwrap().0;
        let exact=super::super::axial_magnetic::axial_kernel(1.,[-0.2,0.2],[1.9,2.1]).unwrap()/0.08;
        assert!((value-exact).abs()<1e-12*exact);
    }
    #[test] fn thick_tetra_outer_refinement() {
        let a=[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,0.7]];
        let b=[[4.,0.,0.2],[5.,0.,0.2],[4.,1.,0.2],[4.,0.,1.3]];
        let evaluate=|n| TetraCubature::new_axial(a,n).unwrap().try_axial_integral(&TetraCubature::new_axial(b,n).unwrap()).unwrap().unwrap().0;
        let v=evaluate(8); let refined=evaluate(10); let reference=tetra_integral(a,&PointPotential::tetrahedron(b).unwrap(),16).unwrap();
        println!("thick outer 8/10 rel {}, analytic source rel {}",(v-refined).abs()/refined,(refined-reference).abs()/reference);
        assert!((v-refined).abs()<1e-8*refined); assert!((refined-reference).abs()<1e-9*reference);
    }
}

#[cfg(test)]
pub(super) fn axial_retry_fixture()->([[f64;3];4],[[f64;3];4]) {
    let a: [[u64;3];4]=[[4570759899201579418, 4581854491553607503, 4564860111517780814], [4569525120155796275, 4585730642473594022, 4564860111517780814], [4569525120155796275, 4574885801521499550, 4564860111517780814], [4569525120155796275, 4574885801521499550, 4565021520528425773]];
    let b: [[u64;3];4]=[[4586981900965294533, 4583256213504285003, 0], [4587066828033697933, 4583256213504285003, 0], [4587066828033697933, 4583256213504285003, 4540289913102728146], [4587066828033697933, 4583537308668447739, 4540289913102728146]];
    let a=a.map(|p|p.map(f64::from_bits)); let b=b.map(|p|p.map(f64::from_bits));
    (a,b)
}

#[cfg(test)]
mod axial_outer_retry_tests {
    use super::*;
    #[test] fn captured_outer_retry_and_limits() {
        let (a,b)=axial_retry_fixture();
        let mut x=TetraCubature::new_axial(a,6).unwrap(); let mut y=TetraCubature::new_axial(b,6).unwrap();
        let detail=x.try_axial_detailed(&y).unwrap(); assert_eq!(detail.reason,Some(3));
        x.prepare_axial_retry(a,6).unwrap(); y.prepare_axial_retry(b,6).unwrap();
        let retry=x.try_axial_retry(&y,&detail).unwrap(); let value=retry.value.expect("captured retry must converge").0;
        let direct=|n|TetraCubature::new(a,n).unwrap().direct_integral(&TetraCubature::new(b,n).unwrap()).unwrap();
        let d8=direct(8); let d10=direct(10);
        println!("direct8/10={}, retry/direct10={}",(d8-d10).abs()/d10,(value-d10).abs()/d10);
        assert!((d8-d10).abs()<=1e-10*d10 && (value-d10).abs()<=1e-10*d10);
        let reference=tetra_integral(a,&PointPotential::tetrahedron(b).unwrap(),16).unwrap();
        println!("retry={value:.16e} source_reference={reference:.16e} difference={}",(value-reference).abs()/reference);
        assert!((value-reference).abs()<1e-7*reference);
        let reverse=y.try_axial_detailed(&x).unwrap(); assert_eq!(reverse.reason,Some(3));
        let reverse=y.try_axial_retry(&x,&reverse).unwrap().value.unwrap().0; assert!((value-reverse).abs()<1e-12*value);
        let fake=AxialDetail::fallback(3,None,Some(value)); assert!(x.try_axial_retry(&x,&fake).unwrap().value.is_none());
        let line=std::mem::size_of::<AxialLine>(); let headers=6*std::mem::size_of::<Option<Vec<AxialLine>>>();
        assert!(line*(6*6+8*8+10*10+8*8+10*10+12*12)+headers <=40*1024);
    }
}

#[cfg(test)]
mod lazy_direct_tests {
    use super::*;
    #[test]
    fn lazy_direct_matches_eager_and_preserves_axial_laziness() {
        let a = [[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        let b = a.map(|p| [p[0]+0.1,p[1],p[2]]);
        for order in [4,6,8] {
            let eager_a = TetraCubature::new(a,order).unwrap();
            let eager_b = TetraCubature::new(b,order).unwrap();
            let mut lazy_a = TetraCubature::geometry_only(a).unwrap();
            let mut lazy_b = TetraCubature::geometry_only(b).unwrap();
            assert!(lazy_a.direct_integral(&lazy_b).is_err());
            assert!(lazy_a.prepare_direct(a,order).unwrap());
            lazy_b.prepare_direct(b,order).unwrap();
            assert_eq!(lazy_a.direct_integral(&lazy_b).unwrap().to_bits(), eager_a.direct_integral(&eager_b).unwrap().to_bits());
            let saved = lazy_a.points.clone();
            assert!(!lazy_a.prepare_direct(a,order).unwrap());
            assert_eq!(lazy_a.points, saved);
        }
        let mut unprepared = TetraCubature::geometry_only(a).unwrap();
        assert!(unprepared.prepare_direct(a, 1).is_err());
        assert!(unprepared.points.is_empty() && unprepared.weights.is_empty());
        let invalid = [[0.;3];4];
        assert!(TetraCubature::geometry_only(invalid).is_err());
        let (a,b) = axial_retry_fixture();
        let mut x = TetraCubature::geometry_only(a).unwrap();
        let mut y = TetraCubature::geometry_only(b).unwrap();
        x.prepare_axial(a,6).unwrap(); y.prepare_axial(b,6).unwrap();
        let detail = x.try_axial_detailed(&y).unwrap();
        assert_eq!(detail.reason, Some(3));
        x.prepare_axial_retry(a,6).unwrap(); y.prepare_axial_retry(b,6).unwrap();
        assert!(x.try_axial_retry(&y,&detail).unwrap().value.is_some());
        assert!(x.points.is_empty() && y.points.is_empty());
    }
}
