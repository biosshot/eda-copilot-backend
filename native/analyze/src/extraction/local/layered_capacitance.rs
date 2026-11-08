//! Internal CPU/F64 product-space electrostatics. No PCB/DSL admission.
//! P1(XY)*P1(z) on conservative overlays; all finite conductor voltages remain.
use super::{error, mesh::validate_planar_embedding_xy, Result};
use crate::extraction::EPS0;
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    sparse::{linalg::cholesky, SparseColMatRef, SymbolicSparseColMatRef},
    Conj, Mat, Par, Side,
};
use geo::algorithm::{coordinate_position::CoordPos, dimensions::Dimensions};
#[cfg(test)]
use geo::BooleanOps;
use geo::Validation;
use geo::{Area, BoundingRect, Intersects, LineString, Point, Polygon, Relate};
use rstar::{RTree, RTreeObject, AABB};
use serde::{Deserialize, Serialize};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PlaneInput {
    Exact {
        source_id: String,
        xy_m: Vec<[f64; 2]>,
        triangles: Vec<[usize; 3]>,
    },
    Cdt {
        source_id: String,
        xy_m: Vec<[f64; 2]>,
        constraints: Vec<[usize; 2]>,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metal {
    pub source_id: String,
    pub polygon_m: Vec<[f64; 2]>,
    /// Actual finite void contours; omitted only for a simply connected metal.
    /// Field meshes must carry every contour as exact constraints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes_m: Vec<Vec<[f64; 2]>>,
    pub z0_m: f64,
    pub z1_m: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dielectric {
    pub source_id: String,
    pub z0_m: f64,
    pub z1_m: f64,
    pub epsilon_r: f64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum Exterior {
    InfinityBox,
    InsulatedAnalyticCoupon,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub meshes: Vec<PlaneInput>,
    pub planes: Vec<usize>,
    pub z_m: Vec<f64>,
    pub xy_bounds_m: [[f64; 2]; 2],
    pub metals: Vec<Metal>,
    pub dielectrics: Vec<Dielectric>,
    pub exterior: Exterior,
}
/// Explicit spatial Dirichlet trace on a finite metal surface. Coordinates are
/// exact SI coordinates of the existing field mesh, not nearest-node matches.
#[derive(Clone, Debug, Serialize)]
pub struct TraceNode {
    pub metal_source_id: String,
    pub position_m: [f64; 3],
    pub weights: Vec<(usize, f64)>,
}
/// Potential coordinates shared with a current mesh. Every metal boundary node
/// must be covered; nonnegative partition-of-unity weights preserve common charge.
#[derive(Clone, Debug, Serialize)]
pub struct PotentialTrace {
    pub potential_nodes: Vec<String>,
    pub nodes: Vec<TraceNode>,
}
fn trace_key(metal: usize, p: [f64; 3]) -> (usize, [u64; 3]) {
    (metal, p.map(|x| if x == 0. { 0 } else { x.to_bits() }))
}

/// Explicit request budget; no production defaults are changed.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_nodes: usize,
    pub max_local_terms: usize,
    pub max_bytes: usize,
    pub max_wall: Duration,
}
struct Guard {
    started: Instant,
    budget: Budget,
}
impl Guard {
    fn time(&self) -> Result<()> {
        if self.started.elapsed() > self.budget.max_wall {
            Err(error("DEADLINE", "one whole-request deadline exceeded"))
        } else {
            Ok(())
        }
    }
    fn memory(&self, n: usize) -> Result<()> {
        if n > self.budget.max_bytes {
            Err(error(
                "MEMORY_LIMIT",
                &format!("simultaneous plan {n} exceeds {}", self.budget.max_bytes),
            ))
        } else {
            Ok(())
        }
    }
}
fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| error("MEMORY_LIMIT", "allocation overflow"))
}
fn mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| error("MEMORY_LIMIT", "allocation overflow"))
}
fn input_bytes(input: &Input) -> Result<usize> {
    let mut n = std::mem::size_of::<Input>();
    n = add(
        n,
        mul(input.meshes.capacity(), std::mem::size_of::<PlaneInput>())?,
    )?;
    n = add(n, mul(input.planes.capacity() + input.z_m.capacity(), 8)?)?;
    for m in &input.meshes {
        match m {
            PlaneInput::Exact {
                source_id,
                xy_m,
                triangles,
            } => {
                n = add(n, source_id.capacity())?;
                n = add(n, mul(xy_m.capacity(), 16)?)?;
                n = add(n, mul(triangles.capacity(), 24)?)?;
            }
            PlaneInput::Cdt {
                source_id,
                xy_m,
                constraints,
            } => {
                n = add(n, source_id.capacity())?;
                n = add(n, mul(xy_m.capacity(), 16)?)?;
                n = add(n, mul(constraints.capacity(), 16)?)?;
            }
        }
    }
    n = add(
        n,
        mul(input.metals.capacity(), std::mem::size_of::<Metal>())?,
    )?;
    for m in &input.metals {
        n = add(
            n,
            add(m.source_id.capacity(), mul(m.polygon_m.capacity(), 16)?)?,
        )?;
        n = add(n, mul(m.holes_m.capacity(), std::mem::size_of::<Vec<[f64;2]>>())?)?;
        for ring in &m.holes_m { n = add(n, mul(ring.capacity(), 16)?)?; }
    }
    n = add(
        n,
        mul(
            input.dielectrics.capacity(),
            std::mem::size_of::<Dielectric>(),
        )?,
    )?;
    for d in &input.dielectrics {
        n = add(n, d.source_id.capacity())?;
    }
    Ok(n)
}
fn polygon(p: &[[f64; 2]]) -> Polygon<f64> {
    Polygon::new(LineString::from(p.to_vec()), vec![])
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
#[derive(Clone)]
struct Triangle {
    nodes: [usize; 3],
    origin: [f64; 2],
    gradients: [[f64; 2]; 3],
    polygon: Polygon<f64>,
}
impl Triangle {
    fn values(&self, p: [f64; 2]) -> [f64; 3] {
        let q = sub(p, self.origin);
        [
            1. + dot(q, self.gradients[0]),
            dot(q, self.gradients[1]),
            dot(q, self.gradients[2]),
        ]
    }
}
struct Mesh {
    source_id: String,
    xy: Vec<[f64; 2]>,
    triangles: Vec<Triangle>,
}
#[derive(Clone)]
struct Envelope {
    index: usize,
    bounds: AABB<[f64; 2]>,
}
impl RTreeObject for Envelope {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.bounds
    }
}
fn tree(mesh: &Mesh) -> RTree<Envelope> {
    RTree::bulk_load(
        mesh.triangles
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let b = t.polygon.bounding_rect().unwrap();
                Envelope {
                    index: i,
                    bounds: AABB::from_corners([b.min().x, b.min().y], [b.max().x, b.max().y]),
                }
            })
            .collect(),
    )
}

fn prepare(input: &PlaneInput, bounds: [[f64; 2]; 2], g: &Guard) -> Result<Mesh> {
    g.time()?;
    let (source_id, xy, triangles) = match input {
        PlaneInput::Exact {
            source_id,
            xy_m,
            triangles,
        } => (source_id.clone(), xy_m.clone(), triangles.clone()),
        PlaneInput::Cdt {
            source_id,
            xy_m,
            constraints,
        } => {
            if xy_m.len() > g.budget.max_nodes {
                return Err(error("MESH_LIMIT", "CDT seeds exceed node budget"));
            }
            let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
            let mut ids = Vec::with_capacity(xy_m.len());
            for &p in xy_m {
                g.time()?;
                if p.iter().any(|x| !x.is_finite()) {
                    return Err(error("PLANAR_INPUT", "nonfinite CDT seed"));
                }
                ids.push(
                    cdt.insert(Point2::new(p[0], p[1]))
                        .map_err(|_| error("PLANAR_INPUT", "CDT seed rejected"))?,
                );
            }
            if cdt.num_vertices() != xy_m.len() {
                return Err(error("PLANAR_INPUT", "duplicate CDT seed; no drop policy"));
            }
            for e in constraints {
                g.time()?;
                if e[0] >= ids.len() || e[1] >= ids.len() || e[0] == e[1] {
                    return Err(error("PLANAR_INPUT", "invalid source constraint"));
                }
                cdt.add_constraint_and_split(ids[e[0]], ids[e[1]], |p| p);
                if cdt.num_vertices() > g.budget.max_nodes {
                    return Err(error(
                        "MESH_LIMIT",
                        "CDT constraint intersections exceed cap",
                    ));
                }
            }
            let xy: Vec<_> = cdt
                .vertices()
                .map(|v| {
                    let p = v.position();
                    [p.x, p.y]
                })
                .collect();
            // Input endpoints are identities, not candidates for snapping/alignment.
            if xy_m.iter().any(|p| !xy.contains(p)) {
                return Err(error("PLANAR_INPUT", "original CDT coordinate lost"));
            }
            let triangles = cdt
                .inner_faces()
                .map(|f| f.vertices().map(|v| v.fix().index()))
                .collect();
            (source_id.clone(), xy, triangles)
        }
    };
    if source_id.is_empty() || xy.len() < 3 || xy.iter().flatten().any(|x| !x.is_finite()) {
        return Err(error("PLANAR_INPUT", "source identity/finite XY required"));
    }
    if xy.iter().any(|p| {
        p[0] < bounds[0][0] || p[0] > bounds[1][0] || p[1] < bounds[0][1] || p[1] > bounds[1][1]
    }) {
        return Err(error("PLANAR_INPUT", "vertex outside explicit far box"));
    }
    if triangles.iter().flatten().any(|&i| i >= xy.len()) {
        return Err(error("PLANAR_INPUT", "triangle index outside registry"));
    }
    validate_planar_embedding_xy(&xy, &triangles)?;
    let mut edges = BTreeMap::new();
    let mut ts = Vec::with_capacity(triangles.len());
    let mut total = 0.;
    for mut t in triangles {
        g.time()?;
        let a = xy[t[0]];
        let mut b = xy[t[1]];
        let mut c = xy[t[2]];
        let mut det = cross(sub(b, a), sub(c, a));
        if det < 0. {
            t.swap(1, 2);
            std::mem::swap(&mut b, &mut c);
            det = -det;
        }
        if !det.is_finite() || det <= 0. {
            return Err(error("PLANAR_INPUT", "degenerate triangle; no removal"));
        }
        let gb = [(c[1] - a[1]) / det, -(c[0] - a[0]) / det];
        let gc = [-(b[1] - a[1]) / det, (b[0] - a[0]) / det];
        let ga = [-gb[0] - gc[0], -gb[1] - gc[1]];
        total += det / 2.;
        for k in 0..3 {
            let mut e = [t[k], t[(k + 1) % 3]];
            e.sort();
            *edges.entry(e).or_insert(0usize) += 1;
        }
        ts.push(Triangle {
            nodes: t,
            origin: a,
            gradients: [ga, gb, gc],
            polygon: polygon(&[a, b, c]),
        });
    }
    for (e, n) in edges {
        let a = xy[e[0]];
        let b = xy[e[1]];
        let outer = (a[0] == b[0] && (a[0] == bounds[0][0] || a[0] == bounds[1][0]))
            || (a[1] == b[1] && (a[1] == bounds[0][1] || a[1] == bounds[1][1]));
        if n != if outer { 1 } else { 2 } {
            return Err(error(
                "PLANAR_INPUT",
                "incomplete/nonmanifold far-domain coverage",
            ));
        }
    }
    let area = (bounds[1][0] - bounds[0][0]) * (bounds[1][1] - bounds[0][1]);
    if (total / area - 1.).abs() > 1e-11 {
        return Err(error("PLANAR_INPUT", "triangle areas do not cover far box"));
    }
    Ok(Mesh {
        source_id,
        xy,
        triangles: ts,
    })
}
#[derive(Clone)]
struct Block {
    a: usize,
    b: usize,
    polygon: Polygon<f64>,
    area: f64,
    mass: [[[f64; 3]; 3]; 3],
}
/// Stable exact degree-two moments, evaluated in each triangle's local frame.
fn block(a: usize, b: usize, p: Polygon<f64>, lo: &Triangle, hi: &Triangle) -> Result<Block> {
    if !p.interiors().is_empty() {
        return Err(error("OVERLAY", "triangle intersection produced holes"));
    }
    let points: Vec<_> = p.exterior().0.iter().map(|p| [p.x, p.y]).collect();
    let mut mass = [[[0.; 3]; 3]; 3];
    let mut area = 0.;
    for k in 1..points.len().saturating_sub(2) {
        let v = [points[0], points[k], points[k + 1]];
        let ar = cross(sub(v[1], v[0]), sub(v[2], v[0])).abs() / 2.;
        if ar == 0. {
            continue;
        }
        area += ar;
        let na = v.map(|p| lo.values(p));
        let nb = v.map(|p| hi.values(p));
        for i in 0..3 {
            for j in 0..3 {
                for (which, l, r) in [(0, na, na), (1, nb, nb), (2, na, nb)] {
                    mass[which][i][j] += ar / 12.
                        * ((0..3).map(|q| l[q][i]).sum::<f64>()
                            * (0..3).map(|q| r[q][j]).sum::<f64>()
                            + (0..3).map(|q| l[q][i] * r[q][j]).sum::<f64>());
                }
            }
        }
    }
    if area <= 0. || !area.is_finite() {
        return Err(error("OVERLAY", "nonpositive overlap area"));
    }
    Ok(Block {
        a,
        b,
        polygon: p,
        area,
        mass,
    })
}
// Convex triangle intersection without integer-grid quantization. Endpoints
// lying exactly on the clipping edge retain their original F64 coordinates.
fn triangle_intersection(a: &Triangle, b: &Triangle) -> Option<Polygon<f64>> {
    let mut points: Vec<[f64; 2]> = a.polygon.exterior().0[..3]
        .iter()
        .map(|p| [p.x, p.y])
        .collect();
    let clip: Vec<[f64; 2]> = b.polygon.exterior().0[..3]
        .iter()
        .map(|p| [p.x, p.y])
        .collect();
    for edge in 0..3 {
        if points.is_empty() {
            return None;
        }
        let origin = clip[edge];
        let direction = sub(clip[(edge + 1) % 3], origin);
        let mut out = Vec::with_capacity(8);
        for i in 0..points.len() {
            let p = points[i];
            let q = points[(i + 1) % points.len()];
            let cp = cross(direction, sub(p, origin));
            let cq = cross(direction, sub(q, origin));
            if cp >= 0. {
                out.push(p);
            }
            if (cp < 0. && cq > 0.) || (cp > 0. && cq < 0.) {
                let t = cp / (cp - cq);
                out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
        out.dedup();
        if out.len() > 1 && out.first() == out.last() {
            out.pop();
        }
        points = out;
    }
    if points.len() < 3 {
        return None;
    }
    let p = polygon(&points);
    if p.unsigned_area() > 0. {
        Some(p)
    } else {
        None
    }
}
fn overlaps(a: &Mesh, b: &Mesh, same: bool, g: &Guard, base_bytes: usize) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    let t = tree(b);
    let mut covered = 0.;
    for (i, ta) in a.triangles.iter().enumerate() {
        g.time()?;
        let polys: Vec<_> = if same {
            vec![(i, ta.polygon.clone())]
        } else {
            let rect = ta.polygon.bounding_rect().unwrap();
            let e = AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]);
            let mut v = Vec::new();
            for tb in t.locate_in_envelope_intersecting(&e) {
                if let Some(p) = triangle_intersection(ta, &b.triangles[tb.index]) {
                    v.push((tb.index, p));
                }
            }
            v
        };
        for (j, p) in polys {
            g.memory(add(base_bytes, mul(blocks.len() + 1, 1024)?)?)?;
            let q = block(i, j, p, ta, &b.triangles[j])?;
            covered += q.area;
            blocks.push(q);
        }
    }
    let total = a
        .triangles
        .iter()
        .map(|t| t.polygon.unsigned_area())
        .sum::<f64>();
    if (covered / total - 1.).abs() > 1e-10 {
        return Err(error("OVERLAY", "conservative overlap coverage failed"));
    }
    Ok(blocks)
}
/// [int eps dz, int eps(1-s)^2 dz, int eps s^2 dz, int eps s(1-s) dz].
pub fn z_moments(z0: f64, z1: f64, materials: &[Dielectric]) -> Result<[f64; 4]> {
    if materials.iter().any(|m| {
        !m.z0_m.is_finite()
            || !m.z1_m.is_finite()
            || m.z1_m <= m.z0_m
            || !m.epsilon_r.is_finite()
            || m.epsilon_r <= 0.
    }) || materials.windows(2).any(|m| m[1].z0_m < m[0].z1_m)
    {
        return Err(error("MATERIAL", "invalid or overlapping epsilon bands"));
    }
    let h = z1 - z0;
    if !h.is_finite() || h <= 0. {
        return Err(error("MATERIAL", "nonpositive slab"));
    }
    let mut out = [0.; 4];
    let mut covered = 0.;
    for m in materials {
        let a = z0.max(m.z0_m);
        let b = z1.min(m.z1_m);
        if b <= a {
            continue;
        }
        if !m.epsilon_r.is_finite() || m.epsilon_r <= 0. {
            return Err(error("MATERIAL", "positive finite epsilon required"));
        }
        let s = (a - z0) / h;
        let t = (b - z0) / h;
        let f = |v: f64| {
            [
                v,
                v - v * v + v * v * v / 3.,
                v * v * v / 3.,
                v * v / 2. - v * v * v / 3.,
            ]
        };
        let fa = f(s);
        let fb = f(t);
        for k in 0..4 {
            out[k] += h * m.epsilon_r * (fb[k] - fa[k]);
        }
        covered += b - a;
    }
    if (covered / h - 1.).abs() > 1e-12 {
        return Err(error("MATERIAL", "missing/overlapping dielectric interval"));
    }
    Ok(out)
}
fn stiffness(b: &Block, a: &Triangle, c: &Triangle, h: f64, z: [f64; 4]) -> [[f64; 6]; 6] {
    let mut k = [[0.; 6]; 6];
    for i in 0..3 {
        for j in 0..3 {
            k[i][j] = z[1] * b.area * dot(a.gradients[i], a.gradients[j])
                + z[0] / (h * h) * b.mass[0][i][j];
            k[i + 3][j + 3] = z[2] * b.area * dot(c.gradients[i], c.gradients[j])
                + z[0] / (h * h) * b.mass[1][i][j];
            let v = z[3] * b.area * dot(a.gradients[i], c.gradients[j])
                - z[0] / (h * h) * b.mass[2][i][j];
            k[i][j + 3] = v;
            k[j + 3][i] = v;
        }
    }
    k
}
#[derive(Clone, Copy)]
struct Coo {
    row: u32,
    col: u32,
    value: f64,
}
struct Csc {
    ptr: Vec<usize>,
    indices: Vec<usize>,
    values: Vec<f64>,
}
impl Csc {
    fn from_coo(n: usize, mut entries: Vec<Coo>) -> Self {
        entries.sort_unstable_by_key(|v| (v.col, v.row));
        let mut unique = 0;
        for i in 0..entries.len() {
            if unique > 0
                && entries[unique - 1].col == entries[i].col
                && entries[unique - 1].row == entries[i].row
            {
                entries[unique - 1].value += entries[i].value;
            } else {
                entries[unique] = entries[i];
                unique += 1;
            }
        }
        entries.truncate(unique);
        let mut ptr = vec![0; n + 1];
        let mut indices = Vec::with_capacity(unique);
        let mut values = Vec::with_capacity(unique);
        for e in entries {
            ptr[e.col as usize + 1] += 1;
            indices.push(e.row as usize);
            values.push(e.value);
        }
        for i in 0..n {
            ptr[i + 1] += ptr[i];
        }
        Self {
            ptr,
            indices,
            values,
        }
    }
    fn apply(&self, phi: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let mut y = vec![0.; phi.len()];
        let mut scale = vec![0.; phi.len()];
        for (col, &p) in phi.iter().enumerate() {
            for k in self.ptr[col]..self.ptr[col + 1] {
                let q = self.values[k] * p * EPS0;
                y[self.indices[k]] += q;
                scale[self.indices[k]] += q.abs();
            }
        }
        (y, scale)
    }
    fn bytes(&self) -> usize {
        (self.ptr.capacity() + self.indices.capacity() + self.values.capacity()) * 8
    }
}
/// A conforming spatial override of the baseline dielectric in a whole z slab.
#[derive(Clone, Debug)]
pub struct SpatialDielectric {
    pub z0_m: f64,
    pub z1_m: f64,
    pub epsilon_r: f64,
    pub polygon_m: Vec<[f64; 2]>,
    pub holes_m: Vec<Vec<[f64; 2]>>,
}
#[derive(Clone)]
struct Cell {
    epsilon_override: Option<f64>,
    slab: usize,
    pair: usize,
    block: usize,
    nodes: [usize; 6],
}
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub nodes: usize,
    pub free_nodes: usize,
    pub local_terms: usize,
    pub full_nnz: usize,
    pub free_nnz: usize,
    pub factor_bytes: usize,
    pub scratch_bytes: usize,
    pub planned_simultaneous_bytes: usize,
    pub elapsed_seconds: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct FieldSample {
    pub slab: usize,
    pub lower_triangle: usize,
    pub upper_triangle: usize,
    pub position_m: [f64; 3],
    pub electric_field_v_per_m: [f64; 3],
}
#[derive(Clone, Debug, Serialize)]
pub struct Recovery {
    pub charges_c: Vec<f64>,
    pub far_charge_c: f64,
    pub nodal_potentials_v: Vec<f64>,
    pub fields: Vec<FieldSample>,
    pub energy_j: f64,
    pub normalized_free_residual: f64,
    pub normalized_charge_balance: f64,
    pub normalized_energy_reaction_error: f64,
    pub recovered_energy_relative_error: f64,
}
pub struct Solution {
    pub capacitance_f: Vec<Vec<f64>>,
    pub source_ids: Vec<String>,
    pub mesh_source_ids: Vec<String>,
    pub plan: Plan,
    pub basis_potentials_v: Vec<Vec<f64>>,
    meshes: Vec<Mesh>,
    planes: Vec<usize>,
    z: Vec<f64>,
    materials: Vec<Dielectric>,
    pairs: Vec<Vec<Block>>,
    cells: Vec<Cell>,
    matrix: Csc,
    started: Instant,
    budget: Budget,
    metals: Vec<Metal>,
    known: Vec<Option<usize>>,
    contact_owners: Vec<Vec<usize>>,
    trace_weights: Vec<Vec<(usize, f64)>>,
    far: Vec<bool>,
    insulated: bool,
    active: Vec<bool>,
    prepared: Option<PreparedFactor>,
}

struct PreparedFactor {
    symbolic: cholesky::SymbolicCholesky<usize>,
    values: Vec<f64>,
    mapping: Vec<usize>,
    free: usize,
}
/// A sparse dielectric factor and trace, without an N-port capacitance matrix
/// or one field vector per potential coordinate.
pub struct SpatialFieldAction {
    solution: Solution,
    calls: std::sync::atomic::AtomicUsize,
}
impl SpatialFieldAction {
    pub fn planned_bytes(&self) -> usize { self.solution.plan.planned_simultaneous_bytes }
    pub fn max_epsilon_r(&self) -> f64 { self.solution.max_epsilon_r() }
    pub fn action_count(&self) -> usize { self.calls.load(std::sync::atomic::Ordering::Relaxed) }
    pub fn apply(&self, voltage: &[f64]) -> Result<Vec<f64>> {
        let s=&self.solution;
        let g=Guard { started:s.started, budget:s.budget }; g.time()?;
        if voltage.len()!=s.source_ids.len() || voltage.iter().any(|v|!v.is_finite()) {
            return Err(error("FIELD_TRACE", "complete finite trace voltage required"));
        }
        self.calls.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
        let f=s.prepared.as_ref().ok_or_else(||error("FIELD_MATRIX","missing prepared field factor"))?;
        let nodes=s.matrix.ptr.len()-1;
        let mut phi=vec![0.;nodes];
        if voltage.iter().all(|&v|v==0.) { return Ok(vec![0.;voltage.len()]); }
        for i in 0..nodes { for &(j,w) in &s.trace_weights[i] { phi[i]+=w*voltage[j]; } }
        let mut rhs=Mat::<f64>::zeros(f.free,1);
        for col in 0..nodes {
            if col%256==0 { g.time()?; }
            if phi[col]==0. {continue;}
            for k in s.matrix.ptr[col]..s.matrix.ptr[col+1] {
                let row=s.matrix.indices[k];
                if f.mapping[row]!=usize::MAX { rhs[(f.mapping[row],0)]-=s.matrix.values[k]*phi[col]; }
            }
        }
        let scratch=f.symbolic.solve_in_place_scratch::<f64>(1,Par::Seq);
        let mut memory=MemBuffer::try_new(scratch).map_err(|_|error("MEMORY_LIMIT","prepared field solve scratch"))?;
        cholesky::LltRef::new(&f.symbolic,&f.values).solve_in_place_with_conj(Conj::No,rhs.as_mut(),Par::Seq,MemStack::new(&mut memory));
        g.time()?;
        for i in 0..nodes {if f.mapping[i]!=usize::MAX {phi[i]=rhs[(f.mapping[i],0)];}}
        let (reaction,scale)=s.matrix.apply(&phi);
        let mut charge=vec![0.;voltage.len()];
        for i in 0..nodes {
            if f.mapping[i]!=usize::MAX && reaction[i].abs()>1e-8*scale[i].max(f64::MIN_POSITIVE) {
                return Err(error("FIELD_BALANCE","prepared dielectric free-row residual failed"));
            }
            for &(j,w) in &s.trace_weights[i] {charge[j]+=w*reaction[i];}
        }
        if phi.iter().chain(&charge).chain(&reaction).any(|v|!v.is_finite()) {
            return Err(error("FIELD_RANGE","nonfinite prepared field action"));
        }
        g.time()?; Ok(charge)
    }
}

pub fn extract(input: &Input, budget: Budget) -> Result<Solution> {
    extract_impl(input, None, &[], false, false, budget)
}
/// Same field solver with spatial potential coordinates. This internal path
/// does not establish PCB accuracy or create a physical-terminal mapping.
pub fn extract_traced(input: &Input, trace: &PotentialTrace, budget: Budget) -> Result<Solution> {
    extract_impl(input, Some(trace), &[], false, false, budget)
}
pub fn extract_traced_spatial(input: &Input, trace: &PotentialTrace, spatial: &[SpatialDielectric], budget: Budget) -> Result<Solution> {
    extract_impl(input, Some(trace), spatial, true, false, budget)
}
pub fn prepare_traced_spatial_action(input: &Input, trace: &PotentialTrace, spatial: &[SpatialDielectric], budget: Budget) -> Result<SpatialFieldAction> {
    Ok(SpatialFieldAction { solution:extract_impl(input,Some(trace),spatial,true,true,budget)?, calls:std::sync::atomic::AtomicUsize::new(0) })
}
fn extract_impl(input: &Input, trace: Option<&PotentialTrace>, spatial: &[SpatialDielectric], spatial_route: bool, action_only: bool, budget: Budget) -> Result<Solution> {
    let guard = Guard {
        started: Instant::now(),
        budget,
    };
    let g = &guard;
    g.time()?;
    if input.meshes.is_empty()
        || input.z_m.len() < 2
        || input.planes.len() != input.z_m.len()
        || input.z_m.len() > budget.max_nodes
        || input.meshes.len() > budget.max_nodes
        || input.metals.is_empty()
        || (!spatial_route && input.metals.len() > 16)
        || (spatial_route && (input.metals.len() > budget.max_nodes || input.metals.len() > budget.max_local_terms))
        || input.z_m.iter().any(|v| !v.is_finite())
        || input.z_m.windows(2).any(|v| v[1] <= v[0])
        || input.planes.iter().any(|&m| m >= input.meshes.len())
    {
        return Err(error(
            "FIELD_INPUT",
            "finite increasing planes and budgeted conductors required",
        ));
    }
    let source_ids = trace.map_or_else(
        || {
            input
                .metals
                .iter()
                .map(|m| m.source_id.clone())
                .collect::<Vec<_>>()
        },
        |t| t.potential_nodes.clone(),
    );
    let modes = source_ids.len();
    let solve_columns=if action_only {1} else {modes};
    if modes == 0
        || (!spatial_route && modes > 64)
        || (spatial_route && (modes > budget.max_nodes || modes > budget.max_local_terms))
        || source_ids.iter().any(|s| s.is_empty())
        || source_ids.iter().collect::<BTreeSet<_>>().len() != modes
    {
        return Err(error(
            "FIELD_TRACE",
            "distinct named potential coordinates within budget required",
        ));
    }
    // The spatial route admits larger counts only after preflighting dense
    // capacitance and pair/contact metadata against the request limits.
    if spatial_route {
        let matrix = if action_only {mul(modes,64)?} else {mul(mul(modes, modes)?, 64)?};
        let contacts = mul(mul(input.metals.len(), input.metals.len())?, 16)?;
        let pair_work = mul(input.metals.len(), input.metals.len())?;
        if pair_work > budget.max_local_terms { return Err(error("MESH_LIMIT", "metal pair work exceeds request cap")); }
        g.memory(add(matrix, contacts)?)?;
    }
    let mut supplied = BTreeMap::new();
    let mut trace_bytes = 0;
    if let Some(trace) = trace {
        trace_bytes = add(mul(trace.nodes.len(), 256)?, mul(modes, 128)?)?;
        for node in &trace.nodes {
            trace_bytes = add(
                trace_bytes,
                add(
                    node.metal_source_id.capacity(),
                    mul(node.weights.capacity(), 32)?,
                )?,
            )?;
        }
        g.memory(trace_bytes)?;
        for node in &trace.nodes {
            g.time()?;
            let metal = input
                .metals
                .iter()
                .position(|m| m.source_id == node.metal_source_id)
                .ok_or_else(|| error("FIELD_TRACE", "unknown metal source in trace"))?;
            let mut seen = BTreeSet::new();
            if node.position_m.iter().any(|x| !x.is_finite())
                || node.weights.is_empty()
                || node.weights.len() > modes
                || node.weights.iter().any(|&(i, w)| {
                    i >= modes || !w.is_finite() || w <= 0. || w > 1. || !seen.insert(i)
                })
                || (node.weights.iter().map(|x| x.1).sum::<f64>() - 1.).abs() > 1e-12
            {
                return Err(error(
                    "FIELD_TRACE",
                    "finite positive partition-of-unity trace weights required",
                ));
            }
            if supplied
                .insert(trace_key(metal, node.position_m), node.weights.clone())
                .is_some()
            {
                return Err(error("FIELD_TRACE", "duplicate surface node trace"));
            }
        }
    }
    let bounds = input.xy_bounds_m;
    if bounds.iter().flatten().any(|x| !x.is_finite())
        || bounds[1][0] <= bounds[0][0]
        || bounds[1][1] <= bounds[0][1]
    {
        return Err(error("FIELD_INPUT", "explicit rectangular domain required"));
    }
    let input_nodes: usize = input
        .meshes
        .iter()
        .map(|m| match m {
            PlaneInput::Exact { xy_m, .. } | PlaneInput::Cdt { xy_m, .. } => xy_m.len(),
        })
        .sum();
    // The retained physical surface metadata coexists with the borrowed input
    // and prepared polygons. Admit its clone explicitly, even for very large
    // source contours with a small field mesh.
    let mut retained_metal_bytes = mul(input.metals.len(), std::mem::size_of::<Metal>())?;
    for metal in &input.metals {
        retained_metal_bytes = add(retained_metal_bytes,
            add(metal.source_id.len(), mul(metal.polygon_m.len(), 16)?)?)?;
        retained_metal_bytes = add(retained_metal_bytes,
            mul(metal.holes_m.len(), std::mem::size_of::<Vec<[f64;2]>>())?)?;
        for ring in &metal.holes_m {
            retained_metal_bytes = add(retained_metal_bytes,mul(ring.len(),16)?)?;
        }
    }
    // Prepared geo rings can grow from n to 2*n coordinates while closing an
    // unclosed contour; the retained Solution metadata coexists with them and
    // the borrowed input. Reserve all three additional buffers before making
    // any polygons, including the temporary old ring during reallocation.
    let owned_input_bytes = add(add(input_bytes(input)?, trace_bytes)?, mul(retained_metal_bytes,3)?)?;
    g.memory(add(owned_input_bytes, mul(input_nodes, 4096)?)?)?;
    let mut meshes = Vec::new();
    for m in &input.meshes {
        meshes.push(prepare(m, bounds, g)?);
    }
    let mut ids = BTreeSet::new();
    let metal_polygons: Vec<_> = input.metals.iter().map(|m|
        Polygon::new(LineString::from(m.polygon_m.clone()),
            m.holes_m.iter().cloned().map(LineString::from).collect())).collect();
    let mut finite_contacts=BTreeSet::new();
    for (i, m) in input.metals.iter().enumerate() {
        if m.source_id.is_empty()
            || !ids.insert(&m.source_id)
            || !m.z0_m.is_finite()
            || !m.z1_m.is_finite()
            || m.z1_m <= m.z0_m
            || !input.z_m.contains(&m.z0_m)
            || !input.z_m.contains(&m.z1_m)
            || m.polygon_m.len() < 3
            || m.holes_m.iter().any(|ring| ring.len() < 3)
            || std::iter::once(&m.polygon_m).chain(&m.holes_m).flatten().flatten().any(|x| !x.is_finite())
            || std::iter::once(&m.polygon_m).chain(&m.holes_m).flatten().any(|p| {
                p[0] < bounds[0][0]
                    || p[0] > bounds[1][0]
                    || p[1] < bounds[0][1]
                    || p[1] > bounds[1][1]
            })
            || metal_polygons[i].unsigned_area() <= 0.
            || !metal_polygons[i].is_valid()
        {
            return Err(error(
                "COPPER_INPUT",
                "finite source-ID polygon and exact foil-face planes required; vias unsupported",
            ));
        }
        for j in 0..i {
            let relation=metal_polygons[i].relate(&metal_polygons[j]);
            let overlap_z=m.z1_m.min(input.metals[j].z1_m)-m.z0_m.max(input.metals[j].z0_m);
            if m.z0_m < input.metals[j].z1_m
                && m.z1_m > input.metals[j].z0_m
                && relation.get(CoordPos::Inside, CoordPos::Inside)
                    == Dimensions::TwoDimensional
            {
                return Err(error("COPPER_OVERLAP", "ambiguous conductor ownership"));
            }
            if overlap_z>0. && relation.get(CoordPos::OnBoundary,CoordPos::OnBoundary)==Dimensions::OneDimensional
                || overlap_z==0. && relation.get(CoordPos::Inside,CoordPos::Inside)==Dimensions::TwoDimensional {
                finite_contacts.insert((j,i));
            }
        }
    }
    let mut materials = input.dielectrics.clone();
    materials.sort_by(|a, b| a.z0_m.total_cmp(&b.z0_m));
    if materials.is_empty()
        || materials[0].z0_m != input.z_m[0]
        || materials.last().unwrap().z1_m != *input.z_m.last().unwrap()
        || materials.windows(2).any(|p| p[0].z1_m != p[1].z0_m)
        || materials.iter().any(|m| {
            m.source_id.is_empty()
                || !m.z0_m.is_finite()
                || !m.z1_m.is_finite()
                || m.z1_m <= m.z0_m
                || !m.epsilon_r.is_finite()
                || m.epsilon_r <= 0.
        })
    {
        return Err(error(
            "MATERIAL",
            "explicit nonoverlapping full z material cover required",
        ));
    }
    let mut spatial_polygons = Vec::new();
    let mut spatial_bytes = 0usize;
    for region in spatial {
        g.time()?;
        if !region.z0_m.is_finite() || !region.z1_m.is_finite() || region.z0_m >= region.z1_m || !region.epsilon_r.is_finite() || region.epsilon_r <= 0. || !input.z_m.contains(&region.z0_m) || !input.z_m.contains(&region.z1_m) {
            return Err(error("MATERIAL", "spatial dielectric requires finite properties and exact z planes"));
        }
        let vertices = region.polygon_m.len().checked_add(region.holes_m.iter().map(Vec::len).sum()).ok_or_else(||error("MEMORY_LIMIT", "spatial vertex overflow"))?;
        spatial_bytes = add(spatial_bytes, mul(vertices, 128)?)?;
        g.memory(spatial_bytes)?;
        let poly = Polygon::new(LineString::from(region.polygon_m.clone()), region.holes_m.iter().cloned().map(LineString::from).collect());
        if !poly.is_valid() || poly.unsigned_area() <= 0. { return Err(error("MATERIAL", "invalid spatial dielectric footprint")); }
        spatial_polygons.push(poly);
    }
    let mut offset = vec![0usize];
    for &p in &input.planes {
        offset.push(add(*offset.last().unwrap(), meshes[p].xy.len())?);
    }
    let nodes = *offset.last().unwrap();
    if nodes > budget.max_nodes || nodes > u32::MAX as usize {
        return Err(error(
            "MESH_LIMIT",
            "global nodes exceed explicit request cap",
        ));
    }
    let geometry_bytes = add(
        add(owned_input_bytes, spatial_bytes)?,
        add(mul(input_nodes, 4096)?, mul(nodes, add(160, mul(input.metals.len(),8)?)?)?)?,
    )?;
    g.memory(geometry_bytes)?;
    let mut pair_ids = BTreeMap::new();
    let mut pairs = Vec::new();
    let max_cells = budget.max_local_terms / 36;
    let cell_storage_bytes = mul(max_cells, std::mem::size_of::<Cell>())?;
    g.memory(add(geometry_bytes, cell_storage_bytes)?)?;
    let mut cells = Vec::with_capacity(max_cells);
    let mut active = vec![false; nodes];
    let mut terms = 0usize;
    for slab in 0..input.z_m.len() - 1 {
        g.time()?;
        let ma = input.planes[slab];
        let mb = input.planes[slab + 1];
        let pair = if let Some(&id) = pair_ids.get(&(ma, mb)) {
            id
        } else {
            let id = pairs.len();
            let bytes = add(
                geometry_bytes,
                mul(pairs.iter().map(|p: &Vec<Block>| p.len()).sum(), 1024)?,
            )?;
            pairs.push(overlaps(&meshes[ma], &meshes[mb], ma == mb, g, bytes)?);
            pair_ids.insert((ma, mb), id);
            id
        };
        for (index, b) in pairs[pair].iter().enumerate() {
            let mut excluded = false;
            for (metal, p) in input.metals.iter().zip(&metal_polygons) {
                if input.z_m[slab] >= metal.z0_m && input.z_m[slab + 1] <= metal.z1_m {
                    let relation = b.polygon.relate(p);
                    if relation.is_within() || relation.is_equal_topo() {
                        excluded = true;
                        break;
                    }
                    if relation.get(CoordPos::Inside, CoordPos::Inside)
                        == Dimensions::TwoDimensional
                    {
                        return Err(error(
                            "COPPER_TRACE",
                            "foil-sidewall cuts cell; exact source constraints absent",
                        ));
                    }
                }
            }
            if excluded {
                continue;
            }
            terms = add(terms, 36)?;
            if terms > budget.max_local_terms {
                return Err(error("MESH_LIMIT", "local term count exceeds request cap"));
            }
            g.memory(add(
                geometry_bytes,
                add(mul(terms, 16)?, cell_storage_bytes)?,
            )?)?;
            let a = &meshes[ma].triangles[b.a];
            let c = &meshes[mb].triangles[b.b];
            let ns = std::array::from_fn(|i| {
                if i < 3 {
                    offset[slab] + a.nodes[i]
                } else {
                    offset[slab + 1] + c.nodes[i - 3]
                }
            });
            for &n in &ns {
                active[n] = true;
            }
            let mut epsilon_override = None;
            for (region, poly) in spatial.iter().zip(&spatial_polygons) {
                if input.z_m[slab] >= region.z0_m && input.z_m[slab+1] <= region.z1_m {
                    let relation = b.polygon.relate(poly);
                    if relation.is_within() || relation.is_equal_topo() {
                        if epsilon_override.replace(region.epsilon_r).is_some() { return Err(error("MATERIAL", "overlapping spatial dielectric footprints")); }
                    } else if relation.get(CoordPos::Inside, CoordPos::Inside) == Dimensions::TwoDimensional {
                        return Err(error("MATERIAL", "dielectric boundary cuts field cell"));
                    }
                }
            }
            cells.push(Cell {
                epsilon_override,
                slab,
                pair,
                block: index,
                nodes: ns,
            });
        }
    }
    let mut known = vec![None; nodes];
    let mut contact_owners:Vec<Vec<usize>> = (0..nodes)
        .map(|_|Vec::with_capacity(input.metals.len())).collect();
    let mut trace_weights = vec![vec![]; nodes];
    let mut used_traces = BTreeSet::new();
    let mut used_modes = vec![false; modes];
    let mut far = vec![false; nodes];
    let mut represented = vec![false; input.metals.len()];
    for (k, &m) in input.planes.iter().enumerate() {
        for (i, &p) in meshes[m].xy.iter().enumerate() {
            let n = offset[k] + i;
            for (c, metal) in input.metals.iter().enumerate() {
                if input.z_m[k] >= metal.z0_m
                    && input.z_m[k] <= metal.z1_m
                    && metal_polygons[c].intersects(&Point::from(p))
                {
                    let weights = if trace.is_some() {
                        let key = trace_key(c, [p[0], p[1], input.z_m[k]]);
                        let weights = supplied.get(&key).ok_or_else(|| {
                            error("FIELD_TRACE", "missing exact metal surface node trace")
                        })?;
                        used_traces.insert(key);
                        weights.clone()
                    } else {
                        vec![(c, 1.)]
                    };
                    if known[n].is_some() {
                        if contact_owners[n].iter().any(|&old| !finite_contacts.contains(&(old,c))) {
                            return Err(error("COPPER_TRACE", "point/edge-only metal contact has no finite surface area"));
                        }
                        let existing:BTreeMap<_,_> = trace_weights[n].iter().copied().collect();
                        let incoming:BTreeMap<_,_> = weights.iter().copied().collect();
                        // A shared geometric surface is a contact only when the
                        // explicitly authored potential traces agree exactly.
                        // No net-name weld, nearest-point match or tolerance join.
                        if trace.is_none() || existing != incoming {
                            return Err(error("COPPER_TRACE", "touching metal pieces require identical explicit potential traces"));
                        }
                    } else {
                        known[n] = Some(c);
                        trace_weights[n] = weights.clone();
                    }
                    contact_owners[n].push(c);
                    for &(mode, _) in &weights {
                        // A matched node wholly inside copper has no incident
                        // dielectric cell and therefore contributes an exact
                        // zero row/column to the field action. It still names
                        // a genuine current-mesh coordinate, not a missing
                        // trace. Only the conductor itself requires an active
                        // boundary node below; all supplied nodes must match.
                        used_modes[mode] = true;
                    }
                    represented[c] |= active[n];
                }
            }
            far[n] = input.exterior == Exterior::InfinityBox
                && (k == 0
                    || k + 1 == input.z_m.len()
                    || p[0] == bounds[0][0]
                    || p[0] == bounds[1][0]
                    || p[1] == bounds[0][1]
                    || p[1] == bounds[1][1]);
            if far[n] && known[n].is_some() {
                return Err(error(
                    "COPPER_TRACE",
                    "metal touches explicit far-zero boundary",
                ));
            }
        }
    }
    if used_modes.iter().any(|x| !*x) || used_traces.len() != supplied.len() {
        let unused:Vec<_>=used_modes.iter().enumerate().filter_map(|(i,&used)|(!used).then_some(i)).take(16).collect();
        let unmatched:Vec<_>=supplied.keys().filter(|key|!used_traces.contains(key)).take(8).collect();
        return Err(error(
            "FIELD_TRACE",
            &format!("unused potential coordinate or unmatched supplied trace node: unusedCount={} unusedFirst={unused:?} unmatchedCount={} unmatchedFirst={unmatched:?}",used_modes.iter().filter(|&&v|!v).count(),supplied.len()-used_traces.len()),
        ));
    }
    if represented.iter().any(|x| !*x) {
        return Err(error("COPPER_TRACE", "conductor missing full trace"));
    }
    let pair_bytes = mul(pairs.iter().map(|p| p.len()).sum(), 1024)?;
    let recovery_bytes = add(
        mul(cells.len(), std::mem::size_of::<FieldSample>())?,
        mul(nodes, 24)?,
    )?;
    let base_bytes = add(
        geometry_bytes,
        add(pair_bytes, add(cell_storage_bytes, recovery_bytes)?)?,
    )?;
    // Simultaneous COO+worst CSC and RHS/output before allocation, not dense fill.
    let assembly_bytes = add(
        base_bytes,
        add(mul(terms, 32)?, mul(nodes, add(128, mul(solve_columns, 24)?)?)?)?,
    )?;
    g.memory(assembly_bytes)?;
    let mut coo = Vec::with_capacity(terms);
    for cell in &cells {
        g.time()?;
        let slab = cell.slab;
        let b = &pairs[cell.pair][cell.block];
        let a = &meshes[input.planes[slab]].triangles[b.a];
        let c = &meshes[input.planes[slab + 1]].triangles[b.b];
        let h = input.z_m[slab + 1] - input.z_m[slab];
        let k = stiffness(
            b,
            a,
            c,
            h,
            if let Some(epsilon) = cell.epsilon_override { [h*epsilon, h*epsilon/3., h*epsilon/3., h*epsilon/6.] } else { z_moments(input.z_m[slab], input.z_m[slab + 1], &materials)? },
        );
        for i in 0..6 {
            for j in 0..6 {
                if !k[i][j].is_finite() {
                    return Err(error("FIELD_RANGE", "nonfinite element coefficient"));
                }
                coo.push(Coo {
                    row: cell.nodes[i] as u32,
                    col: cell.nodes[j] as u32,
                    value: k[i][j],
                });
            }
        }
    }
    let matrix = Csc::from_coo(nodes, coo);
    g.time()?;
    let mut mapping = vec![usize::MAX; nodes];
    let mut free = 0;
    for i in 0..nodes {
        if active[i] && known[i].is_none() && !far[i] {
            mapping[i] = free;
            free += 1;
        }
    }
    let mut fp = vec![0; free + 1];
    let mut fi = Vec::with_capacity(matrix.values.len());
    let mut fv = Vec::with_capacity(matrix.values.len());
    let mut rhs = Mat::<f64>::zeros(free, if action_only {0} else {modes});
    for col in 0..nodes {
        for k in matrix.ptr[col]..matrix.ptr[col + 1] {
            let row = matrix.indices[k];
            if mapping[row] != usize::MAX {
                if !action_only {for &(c, w) in &trace_weights[col] {
                    rhs[(mapping[row], c)] -= matrix.values[k] * w;
                }}
                if mapping[col] != usize::MAX {
                    fi.push(mapping[row]);
                    fv.push(matrix.values[k]);
                    fp[mapping[col] + 1] += 1;
                }
            }
        }
    }
    for i in 0..free {
        fp[i + 1] += fp[i];
    }
    // Admit symbolic working/index storage while the full/free CSC and RHS
    // coexist; numeric factor/scratch follow actual symbolic counts below.
    let symbolic_envelope_bytes = add(mul(free, 512)?, mul(fi.len(), 16)?)?;
    let retained = add(
        add(base_bytes, symbolic_envelope_bytes)?,
        add(
            matrix.bytes(),
            add(
                mul(fp.capacity() + fi.capacity() + fv.capacity(), 8)?,
                mul(nodes, add(128, mul(solve_columns, 24)?)?)?,
            )?,
        )?,
    )?;
    g.memory(retained)?;
    g.time()?;
    let sparse = SymbolicSparseColMatRef::new_checked(free, free, &fp, None, &fi);
    let symbolic = cholesky::factorize_symbolic_cholesky(
        sparse,
        Side::Lower,
        Default::default(),
        Default::default(),
    )
    .map_err(|_| error("FIELD_MATRIX", "symbolic LLT refused"))?;
    g.time()?;
    let factor_bytes = mul(symbolic.len_val(), 8)?;
    let factor_scratch =
        symbolic.factorize_numeric_llt_scratch::<f64>(Par::Seq, Default::default());
    let solve_scratch = symbolic.solve_in_place_scratch::<f64>(solve_columns, Par::Seq);
    let scratch_bytes = factor_scratch
        .unaligned_bytes_required()
        .max(solve_scratch.unaligned_bytes_required());
    let plan_bytes = assembly_bytes.max(add(retained, add(factor_bytes, scratch_bytes)?)?);
    g.memory(plan_bytes)?;
    g.time()?;
    let mut factor = vec![0.; symbolic.len_val()];
    {
        let mut scratch = MemBuffer::try_new(factor_scratch)
            .map_err(|_| error("MEMORY_LIMIT", "factor scratch allocation"))?;
        symbolic
            .factorize_numeric_llt(
                &mut factor,
                SparseColMatRef::new(sparse, &fv),
                Side::Lower,
                Default::default(),
                Par::Seq,
                MemStack::new(&mut scratch),
                Default::default(),
            )
            .map_err(|_| error("FIELD_SINGULAR", "SPD failure; no hidden ground/gmin"))?;
    }
    g.time()?;
    if action_only {
        let plan=Plan {nodes,free_nodes:free,local_terms:terms,full_nnz:matrix.values.len(),free_nnz:fv.len(),factor_bytes,scratch_bytes,planned_simultaneous_bytes:plan_bytes,elapsed_seconds:g.started.elapsed().as_secs_f64()};
        return Ok(Solution {
            capacitance_f:vec![],source_ids,mesh_source_ids:meshes.iter().map(|m|m.source_id.clone()).collect(),plan,
            basis_potentials_v:vec![],meshes,planes:input.planes.clone(),z:input.z_m.clone(),materials,pairs,cells,matrix,
            started:g.started,budget,metals:input.metals.clone(),known,contact_owners,trace_weights,far,
            insulated:input.exterior==Exterior::InsulatedAnalyticCoupon,active,
            prepared:Some(PreparedFactor {symbolic,values:factor,mapping,free}),
        });
    }
    {
        let mut scratch = MemBuffer::try_new(solve_scratch)
            .map_err(|_| error("MEMORY_LIMIT", "solve scratch allocation"))?;
        cholesky::LltRef::new(&symbolic, &factor).solve_in_place_with_conj(
            Conj::No,
            rhs.as_mut(),
            Par::Seq,
            MemStack::new(&mut scratch),
        );
    }
    g.time()?;
    let mut basis = vec![vec![0.; nodes]; modes];
    let mut cap = vec![vec![0.; modes]; modes];
    for c in 0..modes {
        for i in 0..nodes {
            basis[c][i] = if known[i].is_some() {
                trace_weights[i]
                    .iter()
                    .filter(|&&(k, _)| k == c)
                    .map(|&(_, w)| w)
                    .sum()
            } else if mapping[i] != usize::MAX {
                rhs[(mapping[i], c)]
            } else {
                0.
            };
        }
        let (reaction, _) = matrix.apply(&basis[c]);
        for i in 0..nodes {
            for &(k, w) in &trace_weights[i] {
                cap[k][c] += w * reaction[i];
            }
        }
    }
    if cap
        .iter()
        .flatten()
        .chain(basis.iter().flatten())
        .any(|x| !x.is_finite())
    {
        return Err(error("FIELD_RANGE", "nonfinite solution"));
    }
    let cap_scale = cap.iter().flatten().map(|v| v.abs()).fold(0., f64::max);
    if cap_scale <= 0.
        || (0..cap.len())
            .any(|i| (0..cap.len()).any(|j| (cap[i][j] - cap[j][i]).abs() > cap_scale * 1e-9))
    {
        return Err(error("FIELD_BALANCE", "full matrix reciprocity failed"));
    }
    let plan = Plan {
        nodes,
        free_nodes: free,
        local_terms: terms,
        full_nnz: matrix.values.len(),
        free_nnz: fv.len(),
        factor_bytes,
        scratch_bytes,
        planned_simultaneous_bytes: plan_bytes,
        elapsed_seconds: g.started.elapsed().as_secs_f64(),
    };
    let mut result = Solution {
        capacitance_f: cap,
        source_ids,
        mesh_source_ids: meshes.iter().map(|m| m.source_id.clone()).collect(),
        plan,
        basis_potentials_v: basis,
        meshes,
        planes: input.planes.clone(),
        z: input.z_m.clone(),
        materials,
        pairs,
        cells,
        matrix,
        started: g.started,
        budget,
        metals: input.metals.clone(),
        known,
        contact_owners,
        trace_weights,
        far,
        insulated: input.exterior == Exterior::InsulatedAnalyticCoupon,
        active,
        prepared:None,
    };
    // Recover each independent voltage once before admission; same whole deadline.
    for c in 0..modes {
        let mut volts = vec![0.; modes];
        volts[c] = 1.;
        result.recover_checked(&volts, Some(g))?;
    }
    g.time()?;
    result.plan.elapsed_seconds = g.started.elapsed().as_secs_f64();
    Ok(result)
}
impl Solution {
    /// One connected product field domain with natural insulated boundaries.
    /// Partition-of-unity traces make a uniform shift exactly field-free.
    /// InfinityBox has a prescribed physical exterior and supplies no gauge.
    pub fn structural_gauge_modes(&self) -> Vec<Vec<f64>> {
        if self.insulated { vec![vec![1.; self.source_ids.len()]] } else { vec![] }
    }

    /// The prescribed physical surface map, including finite foil sidewalls.
    pub fn surface_traces(&self) -> Vec<TraceNode> {
        let mut out = Vec::new();
        let mut offset = 0;
        for (plane, &mesh_id) in self.planes.iter().enumerate() {
            let mesh = &self.meshes[mesh_id];
            for (i, p) in mesh.xy.iter().enumerate() {
                for &m in &self.contact_owners[offset + i] {
                    out.push(TraceNode {
                        metal_source_id: self.metals[m].source_id.clone(),
                        position_m: [p[0], p[1], self.z[plane]],
                        weights: self.trace_weights[offset + i].clone(),
                    });
                }
            }
            offset += mesh.xy.len();
        }
        out
    }
    pub fn metals(&self) -> &[Metal] {
        &self.metals
    }
    pub fn max_epsilon_r(&self) -> f64 {
        self.materials
            .iter()
            .map(|m| m.epsilon_r)
            .chain(self.cells.iter().filter_map(|cell| cell.epsilon_override))
            .fold(1., f64::max)
    }
    /// Galerkin energy matrix, evaluated once per unordered pair and mirrored.
    /// This is B^T K B, not averaging the independently retained reaction C.
    pub fn energy_capacitance_f(&self) -> Result<Vec<Vec<f64>>> {
        let guard = Guard {
            started: self.started,
            budget: self.budget,
        };
        guard.time()?;
        let n = self.source_ids.len();
        let mut out = vec![vec![0.; n]; n];
        let scale = self
            .capacitance_f
            .iter()
            .flatten()
            .map(|v| v.abs())
            .fold(0., f64::max);
        for j in 0..n {
            guard.time()?;
            let (q, _) = self.matrix.apply(&self.basis_potentials_v[j]);
            for i in 0..=j {
                let v = self.basis_potentials_v[i]
                    .iter()
                    .zip(&q)
                    .map(|(p, q)| p * q)
                    .sum::<f64>();
                if !v.is_finite() || (v - self.capacitance_f[i][j]).abs() > 1e-9 * scale {
                    return Err(error(
                        "FIELD_BALANCE",
                        "energy/reaction matrix disagreement",
                    ));
                }
                out[i][j] = v;
                out[j][i] = v;
            }
        }
        Ok(out)
    }
    pub fn recover(&self, volts: &[f64]) -> Result<Recovery> {
        self.recover_checked(
            volts,
            Some(&Guard {
                started: self.started,
                budget: self.budget,
            }),
        )
    }
    fn recover_checked(&self, volts: &[f64], guard: Option<&Guard>) -> Result<Recovery> {
        if let Some(g) = guard {
            g.time()?;
        }
        if volts.len() != self.source_ids.len() || volts.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "FIELD_VOLTAGE",
                "finite full conductor vector required",
            ));
        }
        let gauge = if self.far.iter().all(|x| !*x) {
            volts[0]
        } else {
            0.
        };
        let mut phi = vec![0.; self.plan.nodes];
        for (v, b) in volts.iter().zip(&self.basis_potentials_v) {
            for (p, x) in phi.iter_mut().zip(b) {
                *p += (v - gauge) * x;
            }
        }
        let (reaction, scale) = self.matrix.apply(&phi);
        let mut charges = vec![0.; volts.len()];
        let mut far_charge = 0.;
        let mut residual: f64 = 0.;
        for i in 0..phi.len() {
            if self.known[i].is_some() {
                for &(c, w) in &self.trace_weights[i] {
                    charges[c] += w * reaction[i];
                }
            } else if self.far[i] {
                far_charge += reaction[i];
            } else if self.active[i] {
                residual = residual.max(reaction[i].abs() / scale[i].max(f64::MIN_POSITIVE));
            }
        }
        let energy = phi
            .iter()
            .zip(&reaction)
            .map(|(v, q)| v * q / 2.)
            .sum::<f64>();
        let work = volts
            .iter()
            .zip(&charges)
            .map(|(v, q)| (v - gauge) * q / 2.)
            .sum::<f64>();
        let denom = energy.abs().max(work.abs()).max(f64::MIN_POSITIVE);
        let balance = (charges.iter().sum::<f64>() + far_charge).abs()
            / (charges.iter().map(|q| q.abs()).sum::<f64>() + far_charge.abs())
                .max(f64::MIN_POSITIVE);
        let mut fields = Vec::with_capacity(self.cells.len());
        let mut recovered = 0.;
        for cell in &self.cells {
            if let Some(g) = guard {
                g.time()?;
            }
            let slab = cell.slab;
            let b = &self.pairs[cell.pair][cell.block];
            let a = &self.meshes[self.planes[slab]].triangles[b.a];
            let c = &self.meshes[self.planes[slab + 1]].triangles[b.b];
            let va = [phi[cell.nodes[0]], phi[cell.nodes[1]], phi[cell.nodes[2]]];
            let vb = [phi[cell.nodes[3]], phi[cell.nodes[4]], phi[cell.nodes[5]]];
            let h = self.z[slab + 1] - self.z[slab];
            let ga = std::array::from_fn::<_, 2, _>(|axis| {
                (0..3).map(|i| va[i] * a.gradients[i][axis]).sum::<f64>()
            });
            let gb = std::array::from_fn::<_, 2, _>(|axis| {
                (0..3).map(|i| vb[i] * c.gradients[i][axis]).sum::<f64>()
            });
            let value = |t: &Triangle, v: [f64; 3], p: [f64; 2]| {
                t.values(p).iter().zip(v).map(|(n, x)| n * x).sum::<f64>()
            };
            let e = |p: [f64; 2], s: f64| {
                [
                    -((1. - s) * ga[0] + s * gb[0]),
                    -((1. - s) * ga[1] + s * gb[1]),
                    -(value(c, vb, p) - value(a, va, p)) / h,
                ]
            };
            let points: Vec<_> = b.polygon.exterior().0.iter().map(|p| [p.x, p.y]).collect();
            let center = [
                points[..points.len() - 1].iter().map(|p| p[0]).sum::<f64>()
                    / (points.len() - 1) as f64,
                points[..points.len() - 1].iter().map(|p| p[1]).sum::<f64>()
                    / (points.len() - 1) as f64,
            ];
            fields.push(FieldSample {
                slab,
                lower_triangle: b.a,
                upper_triangle: b.b,
                position_m: [center[0], center[1], (self.z[slab] + self.z[slab + 1]) / 2.],
                electric_field_v_per_m: e(center, 0.5),
            });
            for k in 1..points.len().saturating_sub(2) {
                let tri = [points[0], points[k], points[k + 1]];
                let area = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0])).abs() / 2.;
                for weights in [
                    [2. / 3., 1. / 6., 1. / 6.],
                    [1. / 6., 2. / 3., 1. / 6.],
                    [1. / 6., 1. / 6., 2. / 3.],
                ] {
                    let p = std::array::from_fn(|axis| {
                        (0..3).map(|i| weights[i] * tri[i][axis]).sum::<f64>()
                    });
                    for material in &self.materials {
                        let lo = self.z[slab].max(material.z0_m);
                        let hi = self.z[slab + 1].min(material.z1_m);
                        if hi <= lo {
                            continue;
                        }
                        for q in [-1. / 3f64.sqrt(), 1. / 3f64.sqrt()] {
                            let z = (lo + hi) / 2. + q * (hi - lo) / 2.;
                            let field = e(p, (z - self.z[slab]) / h);
                            recovered += EPS0 * cell.epsilon_override.unwrap_or(material.epsilon_r) * area / 3. * (hi - lo) / 2.
                                * field.iter().map(|v| v * v).sum::<f64>()
                                / 2.;
                        }
                    }
                }
            }
        }
        let recovery =
            (recovered - energy).abs() / recovered.abs().max(energy.abs()).max(f64::MIN_POSITIVE);
        let work_error = (energy - work).abs() / denom;
        if phi
            .iter()
            .chain(&charges)
            .chain(fields.iter().flat_map(|f| f.electric_field_v_per_m.iter()))
            .any(|v| !v.is_finite())
            || ![energy, recovered, residual, balance, work_error, recovery]
                .iter()
                .all(|x| x.is_finite())
            || residual > 1e-9
            || balance > 1e-9
            || work_error > 1e-9
            || recovery > 1e-9
            || energy < -1e-28
        {
            return Err(error("FIELD_BALANCE",&format!("free={residual:e} charge={balance:e} work={work_error:e} recovery={recovery:e}")));
        }
        for p in &mut phi {
            *p += gauge;
        }
        Ok(Recovery {
            charges_c: charges,
            far_charge_c: far_charge,
            nodal_potentials_v: phi,
            fields,
            energy_j: recovered,
            normalized_free_residual: residual,
            normalized_charge_balance: balance,
            normalized_energy_reaction_error: work_error,
            recovered_energy_relative_error: recovery,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_affine_constant_and_asymmetric_epsilon_overlap_patch() {
        let a = Triangle {
            nodes: [0, 1, 2],
            origin: [0., 0.],
            gradients: [[-1000., 0.], [1000., -1000.], [0., 1000.]],
            polygon: polygon(&[[0., 0.], [0.001, 0.], [0.001, 0.001]]),
        };
        let b = Triangle {
            nodes: [0, 1, 2],
            origin: [0., 0.],
            gradients: [[-1000., -1000.], [1000., 0.], [0., 1000.]],
            polygon: polygon(&[[0., 0.], [0.001, 0.], [0., 0.001]]),
        };
        let p = triangle_intersection(&a, &b).unwrap();
        let block = block(0, 0, p, &a, &b).unwrap();
        assert!((block.area / 0.00000025 - 1.).abs() < 1e-14);
        let h = 0.00037;
        let material = vec![
            Dielectric {
                source_id: "quarter".into(),
                z0_m: 0.,
                z1_m: h / 4.,
                epsilon_r: 2.,
            },
            Dielectric {
                source_id: "rest".into(),
                z0_m: h / 4.,
                z1_m: h,
                epsilon_r: 5.,
            },
        ];
        let m = z_moments(0., h, &material).unwrap();
        assert!((m[0] / (h * 4.25) - 1.).abs() < 1e-15);
        let k = stiffness(&block, &a, &b, h, m);
        let v = [0., 0.002, -0.001, 5. * h, 0.002 + 5. * h, -0.003 + 5. * h];
        let energy = (0..6)
            .map(|i| (0..6).map(|j| v[i] * k[i][j] * v[j]).sum::<f64>())
            .sum::<f64>();
        let expected = block.area * h * 4.25 * 38.;
        assert!((energy / expected - 1.).abs() < 2e-12);
        let scale = k.iter().flatten().map(|x| x.abs()).fold(0., f64::max);
        assert!(
            k.iter()
                .map(|row| row.iter().sum::<f64>().abs())
                .fold(0., f64::max)
                < scale * 1e-12
        );
        let old = a.polygon.intersection(&b.polygon).unsigned_area();
        println!(
            "overlay analytic area={:e}, legacy BooleanOps={old:e}, native={:e}",
            0.00000025, block.area
        );
    }
}


#[cfg(test)]
mod spatial_material_tests {
    use super::*;
    #[test]
    fn footprint_preserves_hole_and_detects_cut() {
        let footprint=Polygon::new(LineString::from(vec![[0.,0.],[4.,0.],[4.,4.],[0.,4.],[0.,0.]]),vec![LineString::from(vec![[1.,1.],[3.,1.],[3.,3.],[1.,3.],[1.,1.]])]);
        let inside=polygon(&[[0.1,0.1],[0.9,0.1],[0.1,0.9],[0.1,0.1]]);
        let hole=polygon(&[[1.2,1.2],[1.8,1.2],[1.2,1.8],[1.2,1.2]]);
        let cut=polygon(&[[0.5,0.5],[1.5,0.5],[1.5,1.5],[0.5,0.5]]);
        assert!(inside.relate(&footprint).is_within());
        assert_ne!(hole.relate(&footprint).get(CoordPos::Inside,CoordPos::Inside),Dimensions::TwoDimensional);
        let relation=cut.relate(&footprint);
        assert!(!relation.is_within());
        assert_eq!(relation.get(CoordPos::Inside,CoordPos::Inside),Dimensions::TwoDimensional);
    }
}

#[cfg(test)]
#[path = "layered_spatial_tests.rs"]
mod layered_spatial_tests;
