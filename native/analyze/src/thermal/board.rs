//! Layered P1 FEM constrained by projected copper contours; coefficients integrate clipped
//! copper/void area; unresolved subelement features require explicit refinement.
use super::*;
use crate::geometry::{GeometryOptions, PolygonSnapshot};
use geo::{
    coordinate_position::CoordPos, dimensions::Dimensions, Area, BooleanOps, BoundingRect,
    Centroid, Contains, CoordinatePosition, Distance, Euclidean, LineString, MultiPolygon, Point,
    Polygon, Relate,
};
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use serde_json::json;
use spade::{Point2, Triangulation};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
type Result<T> = std::result::Result<T, String>;
pub(super) fn spatial_node_hard_limit() -> usize {
    140000
}
pub(super) fn spatial_link_limit(max_nodes: usize) -> usize {
    if max_nodes > 110000 {
        return 600000;
    }
    if max_nodes > 100000 {
        500000
    } else {
        250000
    }
}
// Private geometric seed parameters. The public opt-in policy derives its fixed
// physical healing length; test-only probes can select independent seed spacing.
#[derive(Clone, Copy)]
struct RefinementStudy {
    step_mm: f64,
    healing_length_mm: f64,
    graded: bool,
    mesh_only: bool,
    domain_only: bool,
    refine_constraints: bool,
}
#[cfg(test)]
thread_local! {
    static REFINEMENT_STUDY: Cell<Option<RefinementStudy>> = const { Cell::new(None) };
}
fn study_split_constraints(
    lines: Vec<([f64; 2], [f64; 2])>,
    selected: &MultiPolygon,
    step: f64,
    halo: f64,
    cap: usize,
) -> Result<Vec<([f64; 2], [f64; 2])>> {
    let mut output = vec![];
    let bounds = selected
        .bounding_rect()
        .ok_or_else(|| bad("Empty constraint study footprint"))?;
    for (a, b) in lines {
        if a[0].max(b[0]) < bounds.min().x - halo - step
            || a[0].min(b[0]) > bounds.max().x + halo + step
            || a[1].max(b[1]) < bounds.min().y - halo - step
            || a[1].min(b[1]) > bounds.max().y + halo + step
        {
            output.push((a, b));
            continue;
        }
        let count = ((b[0] - a[0]).hypot(b[1] - a[1]) / step).ceil().max(1.) as usize;
        if count > cap {
            return Err(bad("Constraint sizing candidate budget"));
        }
        let mut from = a;
        for i in 1..=count {
            let t = i as f64 / count as f64;
            let to = if i == count {
                b
            } else {
                [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
            };
            if i == count || Euclidean.distance(&Point::new(to[0], to[1]), selected) <= halo + step
            {
                output.push((from, to));
                from = to;
                if output.len() > cap * 8 {
                    return Err(bad("Constraint sizing output budget"));
                }
            }
        }
    }
    Ok(output)
}
// Private saved-mesh experiment: retain every previous triangle edge as a
// numerical constraint and split selected edges at their shared midpoint. The
// coefficient polygons stay authoritative and are independently re-integrated.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NestedMeshPlan {
    pub(super) points_mm: Vec<[f64; 2]>,
    pub(super) edges: Vec<NestedEdge>,
    pub(super) planned_planar_nodes: usize,
    pub(super) admitted: bool,
    #[serde(default)]
    pub(super) physical_constraints_only: bool,
    #[serde(default)]
    pub(super) independent_coverage: bool,
    pub(super) coarse_triangles: Option<Vec<[usize; 3]>>,
    pub(super) points_bits: Option<Vec<[String; 2]>>,
    #[serde(default)]
    pub(super) metadata: Value,
}
#[derive(serde::Deserialize)]
pub(super) struct NestedEdge {
    pub(super) a: usize,
    pub(super) b: usize,
    pub(super) midpoint: Option<[f64; 2]>,
}
#[cfg(test)]
thread_local! {
    static MIDPOINT_STUDY: std::cell::RefCell<Option<NestedMeshPlan>> = const { std::cell::RefCell::new(None) };
}
fn build_nested_mesh(
    plan: NestedMeshPlan,
    cap: usize,
    check: impl Fn() -> Result<()>,
) -> Result<(
    spade::ConstrainedDelaunayTriangulation<Point2<f64>>,
    Option<Value>,
)> {
    use spade::ConstrainedDelaunayTriangulation;
    if !plan.admitted || plan.planned_planar_nodes > cap || plan.edges.len() > cap * 8 {
        return Err(bad("Saved midpoint mesh plan exceeds node/edge admission"));
    }
    let coarse_triangles = plan.coarse_triangles;
    let mut points = plan.points_mm;
    if let Some(bits) = plan.points_bits {
        if bits.len() != points.len() {
            return Err(bad("Saved F64 point identity length"));
        }
        for (point, bits) in points.iter_mut().zip(bits) {
            for j in 0..2 {
                let value = f64::from_bits(u64::from_str_radix(&bits[j], 16).map_err(bad)?);
                if !value.is_finite() || (value - point[j]).abs() > 1e-12 {
                    return Err(bad("Saved F64 point identity mismatch"));
                }
                point[j] = value;
            }
        }
    }
    let original_count = points.len();
    if original_count > cap
        || plan
            .edges
            .iter()
            .any(|e| e.a >= original_count || e.b >= original_count)
    {
        return Err(bad("Invalid saved midpoint topology"));
    }
    let mut split_edges = Vec::with_capacity(plan.edges.len() * 2);
    let mut added = BTreeMap::new();
    for e in plan.edges {
        check()?;
        if let Some(mid) = e.midpoint {
            if mid.iter().any(|v| !v.is_finite()) {
                return Err(bad("Nonfinite midpoint"));
            }
            let [a, b] = [points[e.a], points[e.b]];
            let expected = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
            if (mid[0] - expected[0]).hypot(mid[1] - expected[1]) > 1e-12 {
                return Err(bad(
                    "Saved split differs from arithmetic midpoint beyond transport roundoff",
                ));
            }
            // Recompute from admitted native endpoints: JSON parsers can differ
            // by an ulp, but no separately transported point moves the split.
            let mid = expected;
            let key = (mid[0].to_bits(), mid[1].to_bits());
            let index = if let Some(i) = added.get(&key) {
                *i
            } else {
                if points.len() >= cap {
                    return Err(bad("Saved midpoint mesh node budget"));
                }
                let i = points.len();
                points.push(mid);
                added.insert(key, i);
                i
            };
            split_edges.extend([(e.a, index), (index, e.b)]);
        } else {
            split_edges.push((e.a, e.b));
        }
    }
    if points.len() != plan.planned_planar_nodes {
        return Err(bad("Saved midpoint planned count mismatch"));
    }
    let mut cdt = ConstrainedDelaunayTriangulation::new();
    let handles = points
        .iter()
        .map(|p| cdt.insert(Point2::new(p[0], p[1])).map_err(bad))
        .collect::<Result<Vec<_>>>()?;
    for (a, b) in split_edges {
        check()?;
        if cdt.try_add_constraint(handles[a], handles[b]).is_empty() {
            return Err(bad(format!("Saved midpoint constraint crossing {a} {b}")));
        }
    }
    let proof = coarse_triangles
        .map(|coarse| prove_nested_partition(&cdt, &points, &coarse, &check))
        .transpose()?;
    Ok((cdt, proof))
}
fn prove_nested_partition(
    cdt: &spade::ConstrainedDelaunayTriangulation<Point2<f64>>,
    points: &[[f64; 2]],
    coarse: &[[usize; 3]],
    check: impl Fn() -> Result<()>,
) -> Result<Value> {
    // All old edges remain constraints. Independently verify the geometric
    // partition: every accepted fine triangle lies in one coarse triangle and
    // the areas of its children recover every coarse cell. Thus, in exact
    // collinear arithmetic, each coarse P1 basis is affine on every fine cell.
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let outside = |p: [f64; 2], q: [[f64; 2]; 3]| {
        let sign = cross(q[0], q[1], q[2]).signum();
        (0..3)
            .map(|j| {
                let a = q[j];
                let b = q[(j + 1) % 3];
                (-sign * cross(a, b, p) / (b[0] - a[0]).hypot(b[1] - a[1])).max(0.)
            })
            .fold(0., f64::max)
    };
    let tree = RTree::bulk_load(
        coarse
            .iter()
            .enumerate()
            .map(|(index, ids)| {
                let q = ids.map(|i| points[i]);
                TriangleIndex {
                    index,
                    bounds: AABB::from_corners(
                        [
                            q.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
                            q.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
                        ],
                        [
                            q.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max),
                            q.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
                        ],
                    ),
                }
            })
            .collect(),
    );
    let mut area_by_parent = vec![0.; coarse.len()];
    let mut checked = 0usize;
    let mut exterior = 0usize;
    let mut max_deviation = 0f64;
    let mut max_diameter_ratio = 0f64;
    for face in cdt.inner_faces() {
        check()?;
        let q = face.vertices().map(|v| {
            let p = v.position();
            [p.x, p.y]
        });
        let area = cross(q[0], q[1], q[2]).abs() / 2.;
        if area < 5e-17 {
            continue;
        }
        let center = [
            q.iter().map(|p| p[0]).sum::<f64>() / 3.,
            q.iter().map(|p| p[1]).sum::<f64>() / 3.,
        ];
        let envelope = AABB::from_corners(
            [center[0] - 1e-12, center[1] - 1e-12],
            [center[0] + 1e-12, center[1] + 1e-12],
        );
        let mut parent = None;
        for entry in tree.locate_in_envelope_intersecting(&envelope) {
            let p = coarse[entry.index].map(|i| points[i]);
            if outside(center, p) > 1e-12 {
                continue;
            }
            let deviation = q.iter().map(|v| outside(*v, p)).fold(0., f64::max);
            if parent.is_none_or(|(_, old): (usize, f64)| deviation < old) {
                parent = Some((entry.index, deviation));
            }
        }
        let Some((parent, deviation)) = parent else {
            exterior += 1;
            continue;
        };
        if deviation > 1e-12 {
            return Err(bad(format!(
                "Nested fine triangle crosses coarse parent by {deviation:e} mm"
            )));
        }
        let p = coarse[parent].map(|i| points[i]);
        let diameter = |p: [[f64; 2]; 3]| {
            (0..3)
                .map(|j| (p[j][0] - p[(j + 1) % 3][0]).hypot(p[j][1] - p[(j + 1) % 3][1]))
                .fold(0., f64::max)
        };
        let ratio = diameter(q) / diameter(p);
        if ratio > 1. + 1e-8 {
            return Err(bad("Nested fine element diameter exceeds parent"));
        }
        max_diameter_ratio = max_diameter_ratio.max(ratio);
        max_deviation = max_deviation.max(deviation);
        area_by_parent[parent] += area;
        checked += 1;
    }
    let mut max_area_error = 0f64;
    let mut coarse_area = 0f64;
    let mut fine_area = 0f64;
    for (ids, child_area) in coarse.iter().zip(area_by_parent) {
        let q = ids.map(|i| points[i]);
        let area = cross(q[0], q[1], q[2]).abs() / 2.;
        let error = (area - child_area).abs();
        if error > 1e-10 * (1. + area) {
            return Err(bad(format!(
                "Nested children do not partition parent: area {area:e}, child area {child_area:e}"
            )));
        }
        max_area_error = max_area_error.max(error);
        coarse_area += area;
        fine_area += child_area;
    }
    Ok(
        json!({"checkedFineCells":checked,"coarseCells":coarse.len(),"facesOutsideCoarseDomain":exterior,"maximumParentContainmentDeviationMm":max_deviation,"maximumFineToParentDiameterRatio":max_diameter_ratio,"maximumParentAreaErrorMm2":max_area_error,"coarseCellAreaMm2":coarse_area,"fineCellAreaMm2":fine_area,"coarseP1Space":"All old triangle edges retained and split; fine cells partition each parent. Coarse P1 restricts affinely to every fine cell, up to reported F64 segment/containment roundoff. Convex-cell containment prevents coarsening in every fixed direction; this is not a claim of certified exact arithmetic."}),
    )
}
struct ContourSegment {
    a: [f64; 2],
    b: [f64; 2],
}
impl RTreeObject for ContourSegment {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.a[0].min(self.b[0]), self.a[1].min(self.b[1])],
            [self.a[0].max(self.b[0]), self.a[1].max(self.b[1])],
        )
    }
}
impl PointDistance for ContourSegment {
    fn distance_2(&self, p: &[f64; 2]) -> f64 {
        let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
        let norm = d[0] * d[0] + d[1] * d[1];
        let t = if norm > 0. {
            ((p[0] - self.a[0]) * d[0] + (p[1] - self.a[1]) * d[1]) / norm
        } else {
            0.
        }
        .clamp(0., 1.);
        (p[0] - self.a[0] - t * d[0]).powi(2) + (p[1] - self.a[1] - t * d[1]).powi(2)
    }
}
fn bad(message: impl std::fmt::Display) -> String {
    format!("THERMAL_BOARD: {message}")
}
fn nominal_pad_width(shape: &crate::model::PadShape) -> Option<f64> {
    use crate::model::PadShape;
    match shape {
        PadShape::Circle { diameter_mm } => Some(*diameter_mm),
        PadShape::Rect {
            width_mm,
            height_mm,
        }
        | PadShape::RoundRect {
            width_mm,
            height_mm,
            ..
        }
        | PadShape::Oval {
            width_mm,
            height_mm,
        } => Some(width_mm.min(*height_mm)),
        PadShape::Polygon(_) => None,
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Material {
    #[serde(rename = "thermalConductivityWPerMK")]
    pub thermal_conductivity_w_per_mk: f64,
    pub density_kg_m3: Option<f64>,
    pub specific_heat_j_per_kg_k: Option<f64>,
    pub source: String,
    pub state: String,
}
impl Material {
    fn validate(&self, time: bool) -> Result<()> {
        if !self.thermal_conductivity_w_per_mk.is_finite()
            || self.thermal_conductivity_w_per_mk <= 0.
            || self.source.trim().is_empty()
            || self.state.trim().is_empty()
            || time && (self.capacity().is_none())
        {
            return Err(bad(
                "Material requires explicit positive k, source/state and rho/c for time",
            ));
        }
        Ok(())
    }
    fn capacity(&self) -> Option<f64> {
        let r = self.density_kg_m3?;
        let c = self.specific_heat_j_per_kg_k?;
        if r.is_finite() && r > 0. && c.is_finite() && c > 0. {
            let capacity = r * c;
            if capacity.is_finite() && capacity > 0. {
                Some(capacity)
            } else {
                None
            }
        } else {
            None
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Layer {
    pub kind: String,
    pub name: String,
    pub thickness_mm: f64,
    pub material: Material,
    pub background: Option<Material>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Heat {
    pub id: String,
    pub target: Value,
    pub node: String,
    pub power_w: Signal,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Contact {
    pub id: String,
    pub node: String,
    pub target: Value,
    pub resistance_k_per_w: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Barrel {
    pub thickness_mm: f64,
    pub diameter_convention: String,
    pub material: Material,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BarrelHeat {
    pub id: String,
    pub barrel: String,
    pub from_layer: String,
    pub to_layer: String,
    pub power_w: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BoardInput {
    pub board_json: String,
    pub board_identity: String,
    pub layers: Vec<Layer>,
    pub max_edge_mm: f64,
    #[serde(default = "default_geometry_deviation_mm")]
    pub geometry_deviation_mm: f64,
    pub max_nodes: usize,
    pub snapshot: bool,
    pub heat: Vec<Heat>,
    pub contacts: Vec<Contact>,
    pub surface_cooling: Option<Cooling>,
    pub barrel: Option<Barrel>,
    #[serde(default)]
    pub barrel_heat: Vec<BarrelHeat>,
    pub refinement: Option<super::refinement::Policy>,
}
fn default_geometry_deviation_mm() -> f64 {
    0.001
}
#[derive(Clone)]
pub struct Spatial {
    pub fem_links: BTreeSet<String>,
    pub distributed: Vec<DistributedPath>,
    pub report: Value,
}
#[derive(Clone, Serialize)]
pub struct DistributedPath {
    pub id: String,
    pub from: Vec<(String, f64)>,
    pub to: Vec<(String, f64)>,
    pub conductance: f64,
}
struct TriangleIndex {
    index: usize,
    bounds: AABB<[f64; 2]>,
}
impl RTreeObject for TriangleIndex {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.bounds
    }
}
fn polygon(p: &PolygonSnapshot) -> Polygon {
    let ring = |r: &Vec<[f64; 2]>| {
        let mut p: Vec<_> = r.iter().map(|p| (p[0], p[1])).collect();
        if p.first() != p.last() {
            p.push(p[0]);
        }
        LineString::from(p)
    };
    Polygon::new(ring(&p.outer), p.holes.iter().map(ring).collect())
}
fn domain_report(domain: &MultiPolygon) -> Vec<PolygonSnapshot> {
    let ring = |r: &LineString| r.0[..r.0.len() - 1].iter().map(|p| [p.x, p.y]).collect();
    domain
        .0
        .iter()
        .map(|p| PolygonSnapshot {
            outer: ring(p.exterior()),
            holes: p.interiors().iter().map(ring).collect(),
        })
        .collect()
}
fn selector_polygon(v: &Value) -> Result<Polygon> {
    let ring = |v: &Value| -> Result<LineString> {
        let a = v
            .as_array()
            .ok_or_else(|| bad("Invalid selector polygon"))?;
        let mut out = vec![];
        for p in a {
            out.push((
                p["x"].as_f64().ok_or_else(|| bad("Invalid polygon x"))?,
                p["y"].as_f64().ok_or_else(|| bad("Invalid polygon y"))?,
            ));
        }
        if out.len() < 3 {
            return Err(bad("Invalid polygon ring"));
        }
        out.push(out[0]);
        Ok(LineString::from(out))
    };
    let holes = v["holes"]
        .as_array()
        .map(|a| a.iter().map(ring).collect::<Result<Vec<_>>>())
        .transpose()?
        .unwrap_or_default();
    Ok(Polygon::new(ring(&v["outer"])?, holes))
}
fn add(network: &mut Network, id: String, a: String, b: String, g: f64) {
    if g != 0. {
        network.links.push(Link {
            id,
            from: a,
            to: b,
            cooling: Cooling {
                resistance_k_per_w: Some(1. / g),
                table: None,
                scale: None,
            },
        });
    }
}
fn scale(law: &Cooling, weight: f64) -> Cooling {
    let mut x = law.clone();
    if let Some(r) = x.resistance_k_per_w.as_mut() {
        *r /= weight;
    }
    if let Some(t) = x.table.as_mut() {
        for p in t {
            p.heat_flow_w *= weight;
        }
    }
    x
}
/// Exact topological fast path: a connected triangle's open interior has one
/// material class if no material boundary enters it. Boundary touches have zero
/// area. Every cut/ambiguous case retains the original Boolean intersection.
struct Clipper<'a> {
    area: &'a MultiPolygon,
    boundaries: RTree<ContourSegment>,
}
impl<'a> Clipper<'a> {
    fn new(area: &'a MultiPolygon) -> Self {
        let boundaries = RTree::bulk_load(
            area.0
                .iter()
                .flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()))
                .flat_map(|r| {
                    r.lines().map(|l| ContourSegment {
                        a: [l.start.x, l.start.y],
                        b: [l.end.x, l.end.y],
                    })
                })
                .collect(),
        );
        Self { area, boundaries }
    }
    fn clip(&self, triangle: &Polygon) -> MultiPolygon {
        let b = triangle.bounding_rect().unwrap();
        let envelope = AABB::from_corners([b.min().x, b.min().y], [b.max().x, b.max().y]);
        let cut = self
            .boundaries
            .locate_in_envelope_intersecting(&envelope)
            .any(|s| {
                geo::Line::new(s.a, s.b)
                    .relate(triangle)
                    .get(CoordPos::Inside, CoordPos::Inside)
                    == Dimensions::OneDimensional
            });
        if !cut {
            if let Some(center) = triangle.centroid() {
                match self.area.coordinate_position(&center.0) {
                    CoordPos::Inside => return MultiPolygon(vec![triangle.clone()]),
                    CoordPos::Outside => return MultiPolygon(vec![]),
                    CoordPos::OnBoundary => {}
                }
            }
        }
        self.area
            .intersection(&MultiPolygon(vec![triangle.clone()]))
    }
}
struct Triangle {
    ids: [usize; 3],
    p: Polygon,
    area: f64,
    gradient: [[f64; 2]; 3],
}
pub fn prepare(
    input: &BoardInput,
    network: &mut Network,
    time: bool,
    start: Instant,
    budget: Option<&crate::electrical::Budget>,
) -> Result<Spatial> {
    prepare_with_mesh(input, network, time, start, budget, None, false, None)
}
pub(super) fn prepare_with_mesh(
    input: &BoardInput,
    network: &mut Network,
    time: bool,
    start: Instant,
    budget: Option<&crate::electrical::Budget>,
    supplied_mesh: Option<NestedMeshPlan>,
    geometry_only: bool,
    source_healing_mm: Option<f64>,
) -> Result<Spatial> {
    let refinement_study = source_healing_mm.map(|ell| RefinementStudy {
        step_mm: ell / 2.,
        healing_length_mm: ell,
        graded: true,
        mesh_only: false,
        domain_only: false,
        refine_constraints: true,
    });
    #[cfg(test)]
    let refinement_study = refinement_study.or_else(|| REFINEMENT_STUDY.get());
    let deadline = budget.and_then(|b| b.max_seconds).unwrap_or(120.);
    let memory_mib = budget.and_then(|b| b.max_memory_mi_b).unwrap_or(1024.);
    if !deadline.is_finite()
        || deadline <= 0.
        || !memory_mib.is_finite()
        || memory_mib <= 0.
        || memory_mib > 4096.
        || input.board_json.len() as f64 * 8. > memory_mib * 1048576.
    {
        return Err(bad(
            "Invalid preparation budget or board input exceeds memory plan",
        ));
    }
    let stage = Cell::new("input validation");
    let check = || {
        if start.elapsed().as_secs_f64() > deadline {
            Err(bad(format!(
                "Preparation deadline exceeded in {}",
                stage.get()
            )))
        } else {
            Ok(())
        }
    };
    if input.board_identity.is_empty()
        || input.layers.len() < 3
        || input.layers.len() > 33
        || !input.max_edge_mm.is_finite()
        || input.max_edge_mm <= 0.
        || !input.geometry_deviation_mm.is_finite()
        || !(0.000001..=0.01).contains(&input.geometry_deviation_mm)
        || input.max_nodes < 4
        || input.max_nodes > spatial_node_hard_limit()
    {
        return Err(bad("Invalid layered mesh dimensions/budget/identity"));
    }
    if input.max_nodes > 100000 && input.refinement.is_none() && !cfg!(test) {
        return Err(bad("Nodes above100000 require explicit opt-in refinement"));
    }
    let imported = crate::import_json(&input.board_json);
    let board = imported.board.ok_or_else(|| bad("Board import rejected"))?;
    stage.set("geometry preparation");
    let geo = crate::geometry::prepare(
        &board,
        GeometryOptions {
            max_deviation_mm: input.geometry_deviation_mm,
            ..GeometryOptions::default()
        },
    );
    check()?;
    if geo.status == "invalid"
        || !geo.excluded.is_empty()
        || geo.fills.iter().any(|f| {
            f.status != "available"
                || f.freshness.as_deref() == Some("stale")
                || (!input.snapshot && f.freshness.as_deref() != Some("current"))
        })
    {
        return Err(bad("Incomplete geometry or unaccepted CAD fill freshness"));
    }
    if geo.diagnostics.iter().any(|d| {
        d.category == "invalid"
            || d.code == "independent_drills_missing"
            || d.code == "drill_span_unconfirmed"
    }) {
        return Err(bad("Incomplete drill/geometry data"));
    }
    if board
        .vias
        .iter()
        .any(|v| v.first_layer != 0 || v.last_layer + 1 != board.layers.len())
        || board
            .pads
            .iter()
            .filter_map(|p| p.drill.as_ref())
            .any(|d| !matches!(d.shape, crate::model::DrillShape::Round))
    {
        return Err(bad("First spatial class supports round through bores only; blind/buried/slots require a separate model"));
    }
    if geo
        .holes
        .iter()
        .any(|h| h.plated && (!h.span_confirmed || h.layers.len() != board.layers.len()))
    {
        return Err(bad(
            "Unconfirmed or partial plated bore span is outside the first spatial class",
        ));
    }
    let names: Vec<_> = input
        .layers
        .iter()
        .filter(|l| l.kind == "copper")
        .map(|l| l.name.as_str())
        .collect();
    if names
        != board
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
    {
        return Err(bad(
            "Thermal stack copper order must exactly match captured board",
        ));
    }
    for (i, l) in input.layers.iter().enumerate() {
        if l.kind != if i % 2 == 0 { "copper" } else { "dielectric" }
            || !l.thickness_mm.is_finite()
            || l.thickness_mm <= 0.
            || l.thickness_mm > 100.
        {
            return Err(bad("Alternating positive copper/dielectric stack required"));
        }
        l.material.validate(time)?;
        if l.kind == "copper" {
            l.background
                .as_ref()
                .ok_or_else(|| bad(format!("{} requires explicit backgroundMaterial", l.name)))?
                .validate(time)?;
        } else if l.background.is_some() {
            return Err(bad("Dielectric background is not applicable"));
        }
    }
    let outline = polygon(geo.outline.as_ref().ok_or_else(|| bad("Missing outline"))?);
    let bounds = outline
        .bounding_rect()
        .ok_or_else(|| bad("Empty outline"))?;
    let union = |polys: Vec<Polygon>| -> MultiPolygon {
        if polys.is_empty() {
            MultiPolygon(vec![])
        } else {
            geo::algorithm::bool_ops::unary_union(&polys)
        }
    };
    let bores = union(
        geo.holes
            .iter()
            .flat_map(|h| h.polygons.iter().map(polygon))
            .collect(),
    );
    let mut via_sections = vec![];
    let mut barrel_outer = vec![];
    let plated: Vec<_> = board
        .vias
        .iter()
        .map(|v| {
            (
                v.at,
                v.drill_mm,
                format!("via:{}", v.source.id.as_deref().unwrap_or(&v.source.path)),
            )
        })
        .chain(board.pads.iter().filter_map(|p| {
            p.drill.as_ref().filter(|d| d.plated).map(|d| {
                let offset = d.offset.unwrap_or(crate::model::Point { x: 0., y: 0. });
                let angle = p.rotation_deg.to_radians();
                let at = crate::model::Point {
                    x: p.at.x + angle.cos() * offset.x - angle.sin() * offset.y,
                    y: p.at.y + angle.sin() * offset.x + angle.cos() * offset.y,
                };
                (
                    at,
                    d.diameter_mm,
                    format!("pad:{}", p.source.id.as_deref().unwrap_or(&p.source.path)),
                )
            })
        }))
        .collect();
    if !plated.is_empty() {
        let b = input.barrel.as_ref().ok_or_else(|| {
            bad("Plated bores require explicit thickness/material/diameter convention")
        })?;
        b.material.validate(time)?;
        if !b.thickness_mm.is_finite()
            || b.thickness_mm <= 0.
            || !["finished_hole", "before_plating"].contains(&b.diameter_convention.as_str())
        {
            return Err(bad("Invalid plating profile"));
        }
        for (at, diameter, id) in plated {
            let outer = if b.diameter_convention == "finished_hole" {
                diameter / 2. + b.thickness_mm
            } else {
                diameter / 2.
            };
            let inner = outer - b.thickness_mm;
            if inner <= 0. {
                return Err(bad("Plating exceeds bore radius"));
            }
            let circle = |radius: f64| {
                let mut p: Vec<_> = (0..128)
                    .map(|i| {
                        let a = i as f64 * std::f64::consts::TAU / 128.;
                        (at.x + radius * a.cos(), at.y + radius * a.sin())
                    })
                    .collect();
                p.push(p[0]);
                Polygon::new(LineString::from(p), vec![])
            };
            let out = circle(outer);
            via_sections.push((
                id,
                std::f64::consts::PI * (outer * outer - inner * inner),
                at,
                outer,
            ));
            barrel_outer.push(out);
        }
    }
    let barrel_union = union(barrel_outer);
    // Planarize all layer projections together before CDT. Shared exact
    // intersections avoid the third-party incremental split assertion on
    // dense crossing copper contours. Physical coefficient polygons stay intact.
    let mut contour_lines = vec![];
    for poly in std::iter::once(outline.clone())
        .chain(geo.layers.iter().flat_map(|l| l.copper.iter().map(polygon)))
    {
        for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
            for segment in ring.lines() {
                contour_lines.push((
                    [segment.start.x, segment.start.y],
                    [segment.end.x, segment.end.y],
                ));
            }
        }
    }
    let study_constraint_selection = if refinement_study.is_some_and(|s| s.refine_constraints) {
        let targets: Vec<_> = input
            .contacts
            .iter()
            .map(|c| &c.target)
            .chain(input.heat.iter().map(|h| &h.target))
            .collect();
        if targets
            .iter()
            .any(|t| t["kind"] != "component" && t["kind"] != "region")
        {
            return Err(bad(
                "Opt-in source halo supports explicit components or polygon regions",
            ));
        }
        let paths: Vec<_> = board
            .pads
            .iter()
            .filter(|p| {
                p.component.is_some_and(|i| {
                    targets.iter().any(|t| {
                        t["designator"].as_str() == Some(board.components[i].designator.as_str())
                    })
                })
            })
            .map(|p| p.source.path.as_str())
            .collect();
        let mut selected: Vec<_> = geo
            .regions
            .iter()
            .filter(|r| paths.contains(&r.source.path.as_str()))
            .map(|r| polygon(&r.polygon))
            .collect();
        for target in &targets {
            if target["kind"] == "region" {
                selected.push(selector_polygon(&target["polygon"])?);
            }
        }
        Some(union(selected))
    } else {
        None
    };
    if let Some(selected) = &study_constraint_selection {
        let study = refinement_study.unwrap();
        contour_lines = study_split_constraints(
            contour_lines,
            selected,
            study.step_mm,
            4. * study.healing_length_mm,
            input.max_nodes / input.layers.len(),
        )?;
    }
    stage.set("projected contour arrangement");
    let arrangement = super::mesh::arrange(
        contour_lines,
        input.max_edge_mm,
        input.max_nodes / input.layers.len(),
        check,
    )?;
    let mut cdt = arrangement.cdt;
    let midpoint_plan = supplied_mesh;
    #[cfg(test)]
    let midpoint_plan = midpoint_plan.or_else(|| MIDPOINT_STUDY.with(|p| p.borrow_mut().take()));
    let imported_midpoint_mesh = midpoint_plan.is_some();
    let mut imported_mesh_metadata = midpoint_plan
        .as_ref()
        .map(|p| p.metadata.clone())
        .filter(|p| !p.is_null());
    if let Some(plan) = midpoint_plan {
        let (nested, proof) = build_nested_mesh(plan, input.max_nodes / input.layers.len(), check)?;
        cdt = nested;
        if let Some(metadata) = &mut imported_mesh_metadata {
            if let Some(proof) = proof {
                metadata["nestedProof"] = proof;
            }
            metadata["actualConstraintEdges"] = json!(cdt
                .undirected_edges()
                .filter(|e| e.is_constraint_edge())
                .count());
            metadata["actualTotalMeshEdges"] = json!(cdt.undirected_edges().count());
            metadata["actualPlanarNodes"] = json!(cdt.num_vertices());
            metadata["actualUnclippedTriangles"] = json!(cdt.num_inner_faces());
        }
    }
    // Interior sampling points are optional mesh aids. Do not insert one a
    // few ulps from an authored contour vertex: that creates artificial skinny
    // slivers and disconnected duplicate interface nodes. Physical coordinates
    // and contours are unchanged; only redundant interior samples are omitted.
    let mut contour_segments = vec![];
    for layer in &geo.layers {
        for snapshot in &layer.copper {
            let polygon = polygon(snapshot);
            for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
                for line in ring.lines() {
                    contour_segments.push(ContourSegment {
                        a: [line.start.x, line.start.y],
                        b: [line.end.x, line.end.y],
                    });
                }
            }
        }
    }
    let contour_index = RTree::bulk_load(contour_segments);
    let on_contour = |x: f64, y: f64| {
        contour_index
            .nearest_neighbor(&[x, y])
            .is_some_and(|s| s.distance_2(&[x, y]) <= 1e-18)
    };
    let key = |x: f64, y: f64| ((x * 1e9).round() as i64, (y * 1e9).round() as i64);
    let mut occupied: BTreeSet<_> = cdt
        .vertices()
        .map(|v| key(v.position().x, v.position().y))
        .collect();
    let nx = ((bounds.max().x - bounds.min().x) / input.max_edge_mm).ceil() as usize;
    let ny = ((bounds.max().y - bounds.min().y) / input.max_edge_mm).ceil() as usize;
    if nx.saturating_mul(ny).saturating_mul(input.layers.len()) > input.max_nodes {
        return Err(bad("Interior mesh plan exceeds node budget"));
    }
    for ix in 1..if imported_midpoint_mesh { 1 } else { nx } {
        check()?;
        for iy in 1..ny {
            let x = bounds.min().x + (bounds.max().x - bounds.min().x) * ix as f64 / nx as f64;
            let y = bounds.min().y + (bounds.max().y - bounds.min().y) * iy as f64 / ny as f64;
            if outline.contains(&Point::new(x, y))
                && !on_contour(x, y)
                && occupied.insert(key(x, y))
            {
                cdt.insert(Point2::new(x, y)).map_err(bad)?;
            }
        }
    }
    // Resolve source/contact footprints locally without globally resolving every
    // trace at the smallest contact dimension. Projected copper constraints and
    // remaining cut-cell coefficients still require explicit mesh refinement.
    let mut local_refinement_report = vec![];
    for target in input
        .heat
        .iter()
        .map(|h| &h.target)
        .chain(input.contacts.iter().map(|c| &c.target))
    {
        if imported_midpoint_mesh || target["kind"] == "region" && refinement_study.is_none() {
            continue;
        }
        let selected_pads: Vec<_> = board
            .pads
            .iter()
            .filter(|p| match target["kind"].as_str() {
                Some("padId") => p.source.id.as_deref() == target["id"].as_str(),
                Some("pad") => {
                    p.component.is_some_and(|c| {
                        Some(board.components[c].designator.as_str())
                            == target["designator"].as_str()
                    }) && Some(p.number.as_str()) == target["number"].as_str()
                }
                Some("component") => p.component.is_some_and(|c| {
                    Some(board.components[c].designator.as_str()) == target["designator"].as_str()
                }),
                _ => false,
            })
            .collect();
        let paths: Vec<_> = selected_pads
            .iter()
            .map(|p| p.source.path.as_str())
            .collect();
        let shapes: Vec<_> = selected_pads
            .iter()
            .flat_map(|p| {
                p.layer_shapes
                    .as_ref()
                    .map_or_else(|| vec![&p.common_shape], |s| s.values().collect())
            })
            .collect();
        let minimum_pad_width = shapes
            .iter()
            .try_fold(f64::INFINITY, |a, s| nominal_pad_width(s).map(|w| a.min(w)))
            .filter(|w| w.is_finite() && *w > 0.);
        let unsupported_width_shapes = shapes
            .iter()
            .filter(|s| nominal_pad_width(s).is_none())
            .count();
        let selected = if target["kind"] == "region" {
            MultiPolygon(vec![selector_polygon(&target["polygon"])?])
        } else {
            union(
                geo.regions
                    .iter()
                    .filter(|r| paths.contains(&r.source.path.as_str()))
                    .map(|r| polygon(&r.polygon))
                    .collect(),
            )
        };
        if let Some(study) = refinement_study {
            let bounds = selected
                .bounding_rect()
                .ok_or_else(|| bad("Empty study target"))?;
            let halo = 4. * study.healing_length_mm;
            let low = [bounds.min().x - halo, bounds.min().y - halo];
            let high = [bounds.max().x + halo, bounds.max().y + halo];
            let bands = if study.graded {
                vec![
                    (study.step_mm, -1., study.healing_length_mm),
                    (
                        2. * study.step_mm,
                        study.healing_length_mm,
                        2. * study.healing_length_mm,
                    ),
                    (4. * study.step_mm, 2. * study.healing_length_mm, halo),
                ]
            } else {
                vec![(study.step_mm, -1., halo)]
            };
            let mut candidates = 0usize;
            let mut selected_count = 0usize;
            let before = cdt.num_vertices();
            let mut samples = vec![];
            for (step, inner, outer) in bands {
                let nx = ((high[0] - low[0]) / step).ceil() as usize;
                let ny = ((high[1] - low[1]) / step).ceil() as usize;
                let planned = (nx + 1).saturating_mul(ny + 1);
                candidates = candidates.saturating_add(planned);
                if candidates > 3 * input.max_nodes {
                    return Err(bad("Study candidate plan exceeds bounded workspace"));
                }
                for i in 0..=nx {
                    check()?;
                    for j in 0..=ny {
                        let x = low[0] + (high[0] - low[0]) * i as f64 / nx as f64;
                        let y = low[1] + (high[1] - low[1]) * j as f64 / ny as f64;
                        let distance = Euclidean.distance(&Point::new(x, y), &selected);
                        if distance <= inner || distance > outer {
                            continue;
                        }
                        selected_count += 1;
                        if outline.contains(&Point::new(x, y))
                            && !on_contour(x, y)
                            && occupied.insert(key(x, y))
                        {
                            samples.push([x, y]);
                        }
                    }
                }
            }
            let planned = (before + samples.len()) * input.layers.len();
            if planned > input.max_nodes {
                return Err(bad(format!("Study complete mesh plan {planned} > {} replicated nodes ({} planar; {} new samples; global={} local={} healing={} halo={} graded={})",input.max_nodes,before+samples.len(),samples.len(),input.max_edge_mm,study.step_mm,study.healing_length_mm,halo,study.graded)));
            }
            for p in samples {
                cdt.insert(Point2::new(p[0], p[1])).map_err(bad)?;
            }
            local_refinement_report.push(json!({"target":target,"minimumNominalPadWidthMm":minimum_pad_width,"unsupportedWidthShapes":unsupported_width_shapes,"widthScope":"Explicit physical healing-length seed; no inferred width for arbitrary polygons","derivedSpacingMm":study.step_mm,"healingLengthMm":study.healing_length_mm,"haloMm":halo,"graded":study.graded,"candidatePoints":candidates,"selectedHaloCandidates":selected_count,"insertedPoints":cdt.num_vertices()-before,"studyOnly":input.refinement.is_none()}));
            continue;
        }
        if let Some(bounds) = selected.bounding_rect() {
            let step =
                (input.max_edge_mm / 8.).min(minimum_pad_width.map_or(f64::INFINITY, |w| w / 2.));
            let low = [bounds.min().x - step, bounds.min().y - step];
            let high = [bounds.max().x + step, bounds.max().y + step];
            let nx = ((high[0] - low[0]) / step).ceil() as usize;
            let ny = ((high[1] - low[1]) / step).ceil() as usize;
            if nx.saturating_mul(ny).saturating_mul(input.layers.len()) > input.max_nodes {
                return Err(bad("Local contact refinement exceeds node budget"));
            }
            let before = cdt.num_vertices();
            let mut halo_candidates = 0usize;
            let mut local_points = Vec::new();
            for i in 0..=nx {
                check()?;
                for j in 0..=ny {
                    let x = low[0] + (high[0] - low[0]) * i as f64 / nx as f64;
                    let y = low[1] + (high[1] - low[1]) * j as f64 / ny as f64;
                    if Euclidean.distance(&Point::new(x, y), &selected) > step {
                        continue;
                    }
                    halo_candidates += 1;
                    if outline.contains(&Point::new(x, y))
                        && !on_contour(x, y)
                        && occupied.insert(key(x, y))
                    {
                        local_points.push([x, y]);
                    }
                }
            }
            let planned = (before + local_points.len()) * input.layers.len();
            if planned > input.max_nodes {
                return Err(bad(format!("Local mesh node budget exceeded: complete target plan requires {planned} replicated nodes > {} ({} planar, {} new contact samples)",input.max_nodes,before+local_points.len(),local_points.len())));
            }
            for p in local_points {
                check()?;
                cdt.insert(Point2::new(p[0], p[1])).map_err(bad)?;
            }
            local_refinement_report.push(json!({"target":target,"minimumNominalPadWidthMm":minimum_pad_width,"unsupportedWidthShapes":unsupported_width_shapes,"derivedSpacingMm":step,"haloMm":step,"candidatePoints":(nx+1)*(ny+1),"selectedHaloCandidates":halo_candidates,"insertedPoints":cdt.num_vertices()-before,"widthScope":"Authored primitive nominal pad width; arbitrary polygons, annular webs and narrow necks require explicit mesh convergence"}));
        }
    }
    if cdt.num_vertices() * input.layers.len() > input.max_nodes {
        return Err(bad("Mesh node budget exceeded"));
    }
    #[cfg(test)]
    if refinement_study.is_some_and(|s| s.mesh_only) {
        let mut max_edge = 0f64;
        let mut local_edges = 0usize;
        if let Some(selected) = &study_constraint_selection {
            let study = refinement_study.unwrap();
            for e in cdt.undirected_edges().filter(|e| e.is_constraint_edge()) {
                let [a, b] = e.vertices().map(|v| v.position());
                let midpoint = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                if Euclidean.distance(&midpoint, selected) <= 4. * study.healing_length_mm {
                    max_edge = max_edge.max((b.x - a.x).hypot(b.y - a.y));
                    local_edges += 1;
                }
            }
        }
        return Ok(Spatial {
            fem_links: BTreeSet::new(),
            distributed: vec![],
            report: json!({"studyOnly":true,"meshOnly":true,"globalEdgeMm":input.max_edge_mm,"planarNodes":cdt.num_vertices(),"replicatedNodes":cdt.num_vertices()*input.layers.len(),"unclippedTriangles":cdt.num_inner_faces(),"localRefinement":local_refinement_report,"locallySizedConstraintEdges":local_edges,"maximumLocalConstraintEdgeMm":max_edge,"constraintDeviationMm":arrangement.max_deviation_mm}),
        });
    }
    let points: Vec<_> = cdt
        .vertices()
        .map(|v| [v.position().x, v.position().y])
        .collect();
    let mut triangles = vec![];
    for f in cdt.inner_faces() {
        let vertices = f.vertices();
        let p = vertices.map(|v| [v.position().x, v.position().y]);
        let center = Point::new(
            (p[0][0] + p[1][0] + p[2][0]) / 3.,
            (p[0][1] + p[1][1] + p[2][1]) / 3.,
        );
        if !outline.contains(&center) {
            continue;
        }
        let det =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if det.abs() < 1e-16 {
            continue;
        }
        let gradient = std::array::from_fn(|i| {
            let a = p[(i + 1) % 3];
            let b = p[(i + 2) % 3];
            [(a[1] - b[1]) / det, (b[0] - a[0]) / det]
        });
        let poly = Polygon::new(
            LineString::from(vec![
                (p[0][0], p[0][1]),
                (p[1][0], p[1][1]),
                (p[2][0], p[2][1]),
                (p[0][0], p[0][1]),
            ]),
            vec![],
        );
        triangles.push(Triangle {
            ids: vertices.map(|v| v.fix().index()),
            p: poly,
            area: det.abs() / 2.,
            gradient,
        });
    }
    if let Some(metadata) = &imported_mesh_metadata {
        if let Some(count) = metadata["nestedProof"]["checkedFineCells"].as_u64() {
            if count as usize != triangles.len() {
                return Err(bad(
                    "Nested proof did not cover every accepted board mesh cell",
                ));
            }
        }
    }
    if geometry_only {
        return Ok(Spatial {
            fem_links: BTreeSet::new(),
            distributed: vec![],
            report: json!({"mesh":{"pointsMm":points,"triangles":triangles.iter().map(|t|t.ids).collect::<Vec<_>>(),"constraintDeviationMm":arrangement.max_deviation_mm}}),
        });
    }
    let triangle_index = RTree::bulk_load(
        triangles
            .iter()
            .enumerate()
            .map(|(index, t)| {
                let b = t.p.bounding_rect().unwrap();
                TriangleIndex {
                    index,
                    bounds: AABB::from_corners([b.min().x, b.min().y], [b.max().x, b.max().y]),
                }
            })
            .collect(),
    );
    let planar = points.len();
    let count = planar * input.layers.len();
    let mut capacity = vec![0.; count];
    let mut support = vec![0.; count];
    let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut physical = vec![];
    let mut copper = vec![];
    let mut fractions = vec![];
    // The supported through-bore class has identical physical support in every
    // slice. Compute its clipped cells once; repeated whole-board self-booleans
    // per triangle add cost without changing that domain.
    let domain = MultiPolygon(vec![outline.clone()])
        .difference(&bores)
        .difference(&barrel_union);
    #[cfg(test)]
    if refinement_study.is_some_and(|s| s.domain_only) {
        return Ok(Spatial {
            fem_links: BTreeSet::new(),
            distributed: vec![],
            report: json!({"boardIdentity":input.board_identity,"geometryDeviationMm":input.geometry_deviation_mm,"physicalDomain":domain_report(&domain),"areaMm2":domain.unsigned_area(),"scope":"Exact native domain used before all triangle coefficient clipping; independent of mesh sampling"}),
        });
    }
    stage.set("physical domain clipping");
    let domain_clipper = Clipper::new(&domain);
    let mut physical_cells = Vec::with_capacity(triangles.len());
    for t in &triangles {
        check()?;
        physical_cells.push(domain_clipper.clip(&t.p));
    }
    for l in &input.layers {
        check()?;
        let cu = if l.kind == "copper" {
            MultiPolygon(
                geo.layers
                    .iter()
                    .filter(|g| g.layer == l.name)
                    .flat_map(|g| g.copper.iter().map(polygon))
                    .collect(),
            )
        } else {
            domain.clone()
        };
        physical.push(domain.clone());
        copper.push(cu);
    }
    let copper_clippers: Vec<_> = copper.iter().map(Clipper::new).collect();
    let mut overlaps: Vec<Vec<f64>> = vec![];
    stage.set("in-plane coefficients");
    for (li, l) in input.layers.iter().enumerate() {
        let mut frac = vec![];
        let mut cuts = vec![];
        let thickness = l.thickness_mm * 1e-3;
        for (ti, t) in triangles.iter().enumerate() {
            check()?;
            let domain = &physical_cells[ti];
            let area = domain.unsigned_area();
            let cu = if l.kind == "dielectric" {
                domain.clone()
            } else {
                let clipped = copper_clippers[li].clip(&t.p);
                if clipped == MultiPolygon(vec![t.p.clone()]) {
                    domain.clone()
                } else {
                    clipped.intersection(domain)
                }
            };
            let ca = cu.unsigned_area().min(area);
            let ba = (area - ca).max(0.);
            let k = l.material.thermal_conductivity_w_per_mk;
            let kb = l
                .background
                .as_ref()
                .unwrap_or(&l.material)
                .thermal_conductivity_w_per_mk;
            let integral = thickness * (k * ca + kb * ba);
            let cp = thickness
                * 1e-6
                * (l.material.capacity().unwrap_or(0.) * ca
                    + l.background
                        .as_ref()
                        .unwrap_or(&l.material)
                        .capacity()
                        .unwrap_or(0.)
                        * ba);
            for (a, i) in t.ids.iter().enumerate() {
                let i = li * planar + i;
                capacity[i] += cp / 3.;
                support[i] += area / 3.;
                for (b, j) in t.ids.iter().enumerate().skip(a + 1) {
                    let j = li * planar + j;
                    let g = -integral
                        * (t.gradient[a][0] * t.gradient[b][0]
                            + t.gradient[a][1] * t.gradient[b][1]);
                    *pairs.entry((i.min(j), i.max(j))).or_default() += g;
                }
            }
            frac.push(if t.area > 0. { ca / t.area } else { 0. });
            cuts.push(ca);
        }
        fractions.push(frac);
        overlaps.push(cuts);
    }
    stage.set("through-thickness coefficients");
    for li in 0..input.layers.len() - 1 {
        let a = &input.layers[li];
        let b = &input.layers[li + 1];
        let k = [
            a.material.thermal_conductivity_w_per_mk,
            a.background
                .as_ref()
                .unwrap_or(&a.material)
                .thermal_conductivity_w_per_mk,
        ];
        let j = [
            b.material.thermal_conductivity_w_per_mk,
            b.background
                .as_ref()
                .unwrap_or(&b.material)
                .thermal_conductivity_w_per_mk,
        ];
        for (ti, t) in triangles.iter().enumerate() {
            check()?;
            let domain = &physical_cells[ti];
            let ca = overlaps[li][ti];
            let cb = overlaps[li + 1][ti];
            // Alternating slices guarantee one homogeneous dielectric here;
            // its material overlap is exactly the neighboring copper area.
            let cc = if a.kind == "dielectric" { cb } else { ca };
            let aa = domain.unsigned_area();
            let areas = [
                [cc, (ca - cc).max(0.)],
                [(cb - cc).max(0.), (aa - ca - cb + cc).max(0.)],
            ];
            let mut g = 0.;
            for x in 0..2 {
                for y in 0..2 {
                    g += areas[x][y] * 1e-6
                        / (a.thickness_mm * 1e-3 / (2. * k[x])
                            + b.thickness_mm * 1e-3 / (2. * j[y]));
                }
            }
            for p in t.ids {
                *pairs
                    .entry((li * planar + p, (li + 1) * planar + p))
                    .or_default() += g / 3.;
            }
        }
    }
    let node = |i: usize| format!("pcb:{}:{}", input.layers[i / planar].name, i % planar);
    let mut field_nodes = vec![];
    // Retain the exact mesh-to-network mapping. Barrel mass visits only its
    // normalized rim supports; scanning all physical node IDs here is quadratic.
    let mut node_indices = vec![None; count];
    for i in 0..count {
        if support[i] > 1e-16 {
            let id = node(i);
            node_indices[i] = Some(network.nodes.len());
            network.nodes.push(Node {
                id: id.clone(),
                boundary: None,
                heat_capacity_j_per_k: if time
                    || input.layers[i / planar].material.capacity().is_some()
                        && input.layers[i / planar]
                            .background
                            .as_ref()
                            .is_none_or(|m| m.capacity().is_some())
                {
                    Some(capacity[i])
                } else {
                    None
                },
            });
            field_nodes.push(json!({"id":id,"layer":input.layers[i/planar].name,"pointMm":points[i%planar],"capacityJPerK":capacity[i],"supportAreaMm2":support[i]}));
        }
    }
    let mut fem_links = BTreeSet::new();
    for ((a, b), g) in pairs {
        if !g.is_finite() {
            return Err(bad("Nonfinite assembled thermal coefficient"));
        }
        if g.abs() > 1e-20 {
            if support[a] <= 1e-16 || support[b] <= 1e-16 {
                return Err(bad("Uncomputed FEM support in active stiffness"));
            }
            let id = format!("fem:{a}:{b}");
            fem_links.insert(id.clone());
            add(network, id, node(a), node(b), g);
        }
    }
    if network.nodes.iter().any(|n| n.id == "pcb:ambient") {
        return Err(bad("Reserved board ambient node collision"));
    }
    network.nodes.push(Node {
        id: "pcb:ambient".into(),
        boundary: Some("environment".into()),
        heat_capacity_j_per_k: None,
    });
    stage.set("source and contact coverage");
    let target_weights = |target: &Value| -> Result<Vec<(usize, f64)>> {
        if target["kind"] == "component"
            && board
                .components
                .iter()
                .filter(|c| Some(c.designator.as_str()) == target["designator"].as_str())
                .count()
                != 1
        {
            return Err(bad(
                "Unknown or ambiguous component source/contact identity",
            ));
        }
        let mut selections: Vec<(usize, MultiPolygon)> = vec![];
        if target["kind"] == "region" {
            let poly = selector_polygon(&target["polygon"])?;
            let names = target["layers"]
                .as_array()
                .ok_or_else(|| bad("Region requires layers"))?;
            for name in names {
                let name = name.as_str().ok_or_else(|| bad("Invalid layer"))?;
                let li = input
                    .layers
                    .iter()
                    .position(|l| l.name == name)
                    .ok_or_else(|| bad("Unknown selected thermal layer"))?;
                selections.push((li, MultiPolygon(vec![poly.clone()])));
            }
        } else {
            let paths: Vec<_> = board
                .pads
                .iter()
                .filter(|p| match target["kind"].as_str() {
                    Some("padId") => p.source.id.as_deref() == target["id"].as_str(),
                    Some("pad") => {
                        p.component.is_some_and(|c| {
                            Some(board.components[c].designator.as_str())
                                == target["designator"].as_str()
                        }) && Some(p.number.as_str()) == target["number"].as_str()
                    }
                    Some("component") => p.component.is_some_and(|c| {
                        Some(board.components[c].designator.as_str())
                            == target["designator"].as_str()
                    }),
                    _ => false,
                })
                .map(|p| p.source.path.as_str())
                .collect();
            if paths.is_empty() {
                return Err(bad("Unknown or unsupported source/contact target"));
            }
            let side = if target["kind"] == "component" {
                let c = board
                    .components
                    .iter()
                    .find(|c| Some(c.designator.as_str()) == target["designator"].as_str())
                    .ok_or_else(|| bad("Unknown component"))?;
                Some(if c.side == crate::model::Side::Bottom {
                    input.layers.len() - 1
                } else {
                    0
                })
            } else {
                None
            };
            for (li, l) in input
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| l.kind == "copper")
            {
                if side.is_some_and(|s| s != li) {
                    continue;
                }
                let polys = geo
                    .regions
                    .iter()
                    .filter(|r| r.layer == l.name && paths.contains(&r.source.path.as_str()))
                    .map(|r| polygon(&r.polygon))
                    .collect::<Vec<_>>();
                if !polys.is_empty() {
                    selections.push((li, union(polys)));
                }
            }
        }
        let mut weights: BTreeMap<usize, f64> = BTreeMap::new();
        for (li, selected) in selections {
            let selected = selected.intersection(&physical[li]);
            let Some(bounds) = selected.bounding_rect() else {
                continue;
            };
            for candidate in triangle_index.locate_in_envelope_intersecting(&AABB::from_corners(
                [bounds.min().x, bounds.min().y],
                [bounds.max().x, bounds.max().y],
            )) {
                let t = &triangles[candidate.index];
                let clipped = selected.intersection(&MultiPolygon(vec![t.p.clone()]));
                let a = clipped.unsigned_area();
                if a > 0. {
                    let center = clipped
                        .centroid()
                        .ok_or_else(|| bad("Missing contact centroid"))?;
                    for (j, p) in t.ids.iter().enumerate() {
                        let v = points[*p];
                        let basis = (1.
                            + t.gradient[j][0] * (center.x() - v[0])
                            + t.gradient[j][1] * (center.y() - v[1]))
                            .max(0.);
                        if basis > 0. {
                            *weights.entry(li * planar + p).or_default() += a * basis;
                        }
                    }
                }
            }
        }
        let total: f64 = weights.values().sum();
        if !total.is_finite() || total <= 1e-16 {
            return Err(bad("Selected source/contact has no physical mesh overlap"));
        }
        Ok(weights.into_iter().map(|(i, w)| (i, w / total)).collect())
    };
    let mut source_report = vec![];
    let mut power_targets = BTreeSet::new();
    let mut owned_signal_bytes = network
        .sources
        .iter()
        .map(|s| s.power_w.heap_bytes())
        .sum::<usize>()
        + network
            .links
            .iter()
            .map(|l| l.cooling.heap_bytes().saturating_mul(2))
            .sum::<usize>();
    let fanout_admission = |network: &Network, extra: usize| -> Result<()> {
        let planned = network
            .nodes
            .len()
            .saturating_mul(640)
            .saturating_add(network.links.len().saturating_mul(384))
            .saturating_add(network.sources.len().saturating_mul(160))
            .saturating_add(extra);
        if planned as f64 > memory_mib * 1048576. * 0.75 {
            Err(format!("THERMAL_BUDGET: Source/cooling fanout memory plan{planned} exceeds physical allowance before cloning"))
        } else {
            Ok(())
        }
    };
    for h in &input.heat {
        check()?;
        h.power_w.validate().map_err(bad)?;
        if !power_targets.insert((h.target.to_string(), h.node.clone())) {
            return Err(bad("Duplicate selected heat assignment"));
        }
        let weights = target_weights(&h.target)?;
        owned_signal_bytes =
            owned_signal_bytes.saturating_add(weights.len().saturating_mul(h.power_w.heap_bytes()));
        fanout_admission(
            network,
            owned_signal_bytes.saturating_add(weights.len().saturating_mul(160)),
        )?;
        for (i, w) in &weights {
            network.sources.push(Source {
                node: node(*i),
                power_w: h.power_w.clone(),
                weight: Some(*w),
            });
        }
        source_report.push(json!({"id":h.id,"target":h.target,"node":h.node,"powerW":h.power_w,"weights":weights.iter().map(|(i,w)|json!({"node":node(*i),"weight":w})).collect::<Vec<_>>()}));
    }
    let mut contact_report = vec![];
    for c in &input.contacts {
        if !c.resistance_k_per_w.is_finite()
            || c.resistance_k_per_w <= 0.
            || !network.nodes.iter().any(|n| n.id == c.node)
        {
            return Err(bad(
                "Unknown compact node or invalid explicit contact resistance",
            ));
        }
        let weights = target_weights(&c.target)?;
        for (i, w) in &weights {
            add(
                network,
                format!("contact:{}:{i}", c.id),
                c.node.clone(),
                node(*i),
                w / c.resistance_k_per_w,
            );
        }
        contact_report.push(json!({"id":c.id,"target":c.target,"node":c.node,"resistanceKPerW":c.resistance_k_per_w,"weights":weights.iter().map(|(i,w)|json!({"node":node(*i),"weight":w})).collect::<Vec<_>>()}));
    }
    if let Some(law) = &input.surface_cooling {
        law.validate().map_err(bad)?;
        let faces = [0, input.layers.len() - 1];
        let total: f64 = faces
            .iter()
            .flat_map(|li| support[li * planar..(li + 1) * planar].iter())
            .sum();
        if total <= 0. {
            return Err(bad("Missing exposed top/bottom surface area"));
        }
        let paths = faces
            .iter()
            .flat_map(|li| support[li * planar..(li + 1) * planar].iter())
            .filter(|a| **a > 1e-16)
            .count();
        owned_signal_bytes = owned_signal_bytes
            .saturating_add(paths.saturating_mul(law.heap_bytes()).saturating_mul(2));
        fanout_admission(
            network,
            owned_signal_bytes.saturating_add(paths.saturating_mul(384)),
        )?;
        for li in faces {
            for p in 0..planar {
                let i = li * planar + p;
                if support[i] > 1e-16 {
                    network.links.push(Link {
                        id: format!("surface:{li}:{p}"),
                        from: node(i),
                        to: "pcb:ambient".into(),
                        cooling: scale(law, support[i] / total),
                    });
                }
            }
        }
    }
    let mut via_report = vec![];
    let mut distributed = vec![];
    let mut barrel_source_report = vec![];
    let mut consumed_barrel_heat = BTreeSet::new();
    if let Some(b) = &input.barrel {
        let mut z = 0.;
        let centers: Vec<_> = input
            .layers
            .iter()
            .map(|l| {
                let c = z + l.thickness_mm / 2.;
                z += l.thickness_mm;
                c
            })
            .collect();
        let copper_layers: Vec<_> = input
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == "copper")
            .map(|(i, _)| i)
            .collect();
        for (id, area, at, radius) in via_sections {
            check()?;
            let weights_for = |li: usize| -> Result<Vec<(usize, f64)>> {
                let mut weights: BTreeMap<usize, f64> = BTreeMap::new();
                for sample in 0..128 {
                    let angle = sample as f64 * std::f64::consts::TAU / 128.;
                    let point = [at.x + radius * angle.cos(), at.y + radius * angle.sin()];
                    let candidates =
                        triangle_index.locate_in_envelope_intersecting(&AABB::from_corners(
                            [point[0] - 1e-10, point[1] - 1e-10],
                            [point[0] + 1e-10, point[1] + 1e-10],
                        ));
                    let mut found = false;
                    for candidate in candidates {
                        let triangle = &triangles[candidate.index];
                        let basis: [f64; 3] = std::array::from_fn(|j| {
                            let v = points[triangle.ids[j]];
                            1. + triangle.gradient[j][0] * (point[0] - v[0])
                                + triangle.gradient[j][1] * (point[1] - v[1])
                        });
                        if basis.iter().all(|v| *v >= -1e-8 && *v <= 1. + 1e-8) {
                            for (j, vertex) in triangle.ids.iter().enumerate() {
                                let weight = basis[j].max(0.);
                                if weight > 1e-12 {
                                    let index = li * planar + vertex;
                                    if support[index] <= 1e-16 {
                                        return Err(bad("Barrel outer-rim contact has uncomputed physical support"));
                                    }
                                    *weights.entry(index).or_default() += weight / 128.;
                                }
                            }
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(bad("Barrel outer-rim contact is not fully covered by the physical board mesh"));
                    }
                }
                let total: f64 = weights.values().sum();
                if total <= 0. {
                    return Err(bad("Missing barrel rim contact"));
                }
                Ok(weights.into_iter().map(|(i, w)| (i, w / total)).collect())
            };
            let weights = copper_layers
                .iter()
                .map(|li| weights_for(*li))
                .collect::<Result<Vec<_>>>()?;
            for i in 0..copper_layers.len() - 1 {
                let length = (centers[copper_layers[i + 1]] - centers[copper_layers[i]]) * 1e-3;
                let g = b.material.thermal_conductivity_w_per_mk * area * 1e-6 / length;
                distributed.push(DistributedPath {
                    id: format!("barrel:{id}:{i}"),
                    from: weights[i].iter().map(|(i, w)| (node(*i), *w)).collect(),
                    to: weights[i + 1].iter().map(|(i, w)| (node(*i), *w)).collect(),
                    conductance: g,
                });
                via_report.push(json!({"id":id,"fromLayer":input.layers[copper_layers[i]].name,"toLayer":input.layers[copper_layers[i+1]].name,"centerMm":[at.x,at.y],"outerRadiusMm":radius,"rimPolygonVertices":128,"areaM2":area*1e-6,"lengthM":length,"conductanceWPerK":g,"fromWeights":weights[i],"toWeights":weights[i+1]}));
                for heat in input.barrel_heat.iter().filter(|h| {
                    h.barrel == id
                        && h.from_layer == input.layers[copper_layers[i]].name
                        && h.to_layer == input.layers[copper_layers[i + 1]].name
                }) {
                    if !heat.power_w.is_finite()
                        || heat.power_w < 0.
                        || !consumed_barrel_heat.insert(heat.id.clone())
                    {
                        return Err(bad("Invalid/duplicate finite barrel heat assignment"));
                    }
                    for side in [&weights[i], &weights[i + 1]] {
                        for (node_index, weight) in side {
                            network.sources.push(Source {
                                node: node(*node_index),
                                power_w: Signal::Constant(heat.power_w),
                                weight: Some(weight / 2.),
                            });
                        }
                    }
                    barrel_source_report.push(json!({"id":heat.id,"barrel":id,"fromLayer":heat.from_layer,"toLayer":heat.to_layer,"powerW":heat.power_w,"fromWeights":weights[i],"toWeights":weights[i+1],"distribution":"Half volumetric segment power to each endpoint outer-rim contact; normalized P1 weights"}));
                }
            }
            // Entire barrel volume is excluded from sheet coefficients/capacity and
            // represented once by the finite axial paths and distributed barrel mass.
            let extra: f64 = input
                .layers
                .iter()
                .map(|l| l.thickness_mm * 1e-3 * area * 1e-6 * b.material.capacity().unwrap_or(0.))
                .sum();
            for layer_weights in &weights {
                for (i, w) in layer_weights {
                    if let Some(index) = node_indices[*i] {
                        let n = &mut network.nodes[index];
                        if let Some(capacity) = n.heat_capacity_j_per_k.as_mut() {
                            *capacity += extra * w / copper_layers.len() as f64;
                        }
                    }
                }
            }
        }
    }
    if consumed_barrel_heat.len() != input.barrel_heat.len() {
        return Err(bad("Unresolved barrel heat source/contact identity"));
    }
    // Report the actual final capacities including the explicitly modeled barrel mass.
    let capacities: BTreeMap<_, _> = network
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.heat_capacity_j_per_k))
        .collect();
    for field in &mut field_nodes {
        if let Some(id) = field["id"].as_str() {
            field["capacityJPerK"] = json!(capacities.get(id).copied().flatten());
        }
    }
    let link_limit = spatial_link_limit(input.max_nodes);
    if network.nodes.len() > input.max_nodes || network.links.len() > link_limit {
        return Err(bad(format!("Assembled spatial/contact network exceeds budget: {} nodes (limit {}), {} links (limit {link_limit})",network.nodes.len(),input.max_nodes,network.links.len())));
    }
    let report = json!({"boardIdentity":input.board_identity,"units":{"coordinates":"mm","temperature":"degC","power":"W","capacity":"J/K"},"method":"layered P1 FEM with projected copper contour constraints, exact coefficient-area integration, lumped mass and harmonic half-layer resistance","mesh":{"maxEdgeMm":input.max_edge_mm,"maxNodes":input.max_nodes,"geometryDeviationMm":input.geometry_deviation_mm,"constraintDeviationMm":arrangement.max_deviation_mm,"localRefinement":local_refinement_report,"pointsMm":points,"triangles":triangles.iter().map(|t|t.ids).collect::<Vec<_>>(),"fieldNodes":field_nodes,"copperAreaFractions":fractions},"layers":input.layers,"sources":source_report,"barrelSources":barrel_source_report,"contacts":contact_report,"barrels":via_report,"surfaceCooling":input.surface_cooling,"geometry":{"options":geo.options,"curveApproximationBoundMm":geo.regions.iter().map(|r|r.approximation_mm).fold(0.,f64::max),"outline":geo.outline,"physicalDomain":domain_report(&domain),"holes":geo.holes,"copper":geo.layers,"regionSources":geo.regions.iter().map(|r|json!({"id":r.id,"layer":r.layer,"source":r.source,"areaMm2":r.area_mm2,"approximationMm":r.approximation_mm})).collect::<Vec<_>>(),"diagnostics":geo.diagnostics,"booleanErrorBoundMm":geo.boolean_error_bound_mm},"conditional":input.snapshot||geo.fills.iter().any(|f|f.freshness.as_deref()!=Some("current")),"assumptions":[format!("Authored geometry deviation {} mm; reported per-region curve approximation and Boolean error are bounds, not measured exact-geometry errors; no automatic coarsening at node cap",input.geometry_deviation_mm),"Physical copper fractions integrated exactly against fixed P1 triangle gradients; subelement features are unresolved until demonstrated by refinement","Separate temperature per authored slice; through-thickness transfer lumped at planar vertices using harmonic half-layer resistances","Drilled bores carry zero material; entire barrel area excluded from layer coefficients and finite axial paths/mass added once","Via finite axial element uses normalized P1 outer-rim averages at 128 angles; ring quadrature and sheet variation require refinement","Whole top+bottom cooling characteristic apportioned by exposed area; no sidewall convection","Sources and finite compact contacts distribute normalized projected contact area, preserving total watts"]});
    let report = if let Some(metadata) = imported_mesh_metadata {
        let mut report = report;
        if input.refinement.is_some() {
            report["mesh"]["nested"] = metadata;
        } else {
            report["mesh"]["study"] = metadata;
        }
        report
    } else {
        report
    };
    Ok(Spatial {
        fem_links,
        distributed,
        report,
    })
}

#[cfg(test)]
mod clipping_tests {
    use super::*;
    mod captured {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/thermal_fixture.rs"
        ));
    }
    #[test]
    #[ignore = "Explicit mesh-only physical-halo admission study, no production override"]
    fn controller_fixed_physical_halo_mesh_only_study() {
        let mut results = vec![];
        for global in [1., 0.5] {
            let mut value =
                captured::captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, global);
            value["board"]["geometryDeviationMm"] = json!(0.005);
            value["board"]["maxNodes"] = json!(110000);
            value["budget"]["maxMemoryMiB"] = json!(768.);
            value["budget"]["maxSeconds"] = json!(240.);
            let request: Request = serde_json::from_value(value).unwrap();
            let board = request.board.as_ref().unwrap();
            let sheet = &board.layers[0];
            let core = &board.layers[1];
            let k = sheet
                .background
                .as_ref()
                .unwrap()
                .thermal_conductivity_w_per_mk;
            let g_per_m2 = 1.
                / (sheet.thickness_mm * 1e-3 / (2. * k)
                    + core.thickness_mm * 1e-3
                        / (2. * core.material.thermal_conductivity_w_per_mk));
            let ell = (k * sheet.thickness_mm * 1e-3 / g_per_m2).sqrt() * 1e3;
            for local in [0.125, ell / 2., 0.0625] {
                for graded in [false, true] {
                    REFINEMENT_STUDY.set(Some(RefinementStudy {
                        step_mm: local,
                        healing_length_mm: ell,
                        graded,
                        mesh_only: true,
                        domain_only: false,
                        refine_constraints: false,
                    }));
                    let start = Instant::now();
                    let mut network = request.network.clone();
                    let result =
                        prepare(board, &mut network, false, start, request.budget.as_ref());
                    REFINEMENT_STUDY.set(None);
                    let data = match result {
                        Ok(s) => json!({"status":"mesh-admitted","report":s.report}),
                        Err(e) => json!({"status":"mesh-rejected","reason":e}),
                    };
                    results.push(json!({"globalEdgeMm":global,"localStepMm":local,"graded":graded,"healingLengthMm":ell,"haloMm":4.*ell,"verticalConductancePerAreaWPerM2K":g_per_m2,"elapsedMs":start.elapsed().as_secs_f64()*1000.,"result":data}));
                }
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../debugging/thermal-verification/controller-fixed-physical-halo-mesh-only.json",
        );
        std::fs::write(path, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
        eprintln!("{}", serde_json::to_string_pretty(&results).unwrap());
    }
    #[test]
    #[ignore = "Explicit factor-separated physical-halo mathematical probe"]
    fn controller_fixed_physical_halo_factor_separation() {
        run_halo_factor_cases(false);
    }
    #[test]
    #[ignore = "Constraint/interior factor-separated physical-halo board probe"]
    fn controller_constraint_halo_factor_separation() {
        run_halo_factor_cases(true);
    }
    fn run_halo_factor_cases(refine_constraints: bool) {
        let ell = 0.16549169163435365;
        let mut summary = vec![];
        let cases = if refine_constraints {
            vec![(1., ell / 2.), (1., 0.0625), (0.5, ell / 2.)]
        } else {
            vec![(1., 0.125), (1., ell / 2.), (1., 0.0625), (0.5, ell / 2.)]
        };
        let prefix = if refine_constraints {
            "controller-constraint-halo"
        } else {
            "controller-halo"
        };
        for &(global, local) in &cases {
            let mut request =
                captured::captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, global);
            request["board"]["geometryDeviationMm"] = json!(0.005);
            request["board"]["maxNodes"] = json!(110000);
            request["budget"]["maxMemoryMiB"] = json!(768.);
            request["budget"]["maxSeconds"] = json!(240.);
            REFINEMENT_STUDY.set(Some(RefinementStudy {
                step_mm: local,
                healing_length_mm: ell,
                graded: true,
                mesh_only: false,
                domain_only: false,
                refine_constraints,
            }));
            let result = crate::thermal::run(serde_json::from_value(request.clone()).unwrap());
            REFINEMENT_STUDY.set(None);
            let filename = format!("{prefix}-global{global}-local{local}-graded-result.json");
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../debugging/thermal-verification")
                .join(&filename);
            #[derive(serde::Serialize)]
            struct Saved<'a> {
                request: &'a Value,
                result: &'a Value,
            }
            serde_json::to_writer(
                std::io::BufWriter::new(std::fs::File::create(path).unwrap()),
                &Saved {
                    request: &request,
                    result: &result,
                },
            )
            .unwrap();
            let complete = result["status"] == "complete";
            summary.push(json!({"file":filename,"globalEdgeMm":global,"localStepMm":local,"healingLengthMm":ell,"haloMm":4.*ell,"status":result["status"],"diagnostics":result["diagnostics"],"elapsedMs":result["elapsedMs"],"junctionC":result["temperaturesC"]["U3:junction"],"quality":result["quality"],"memoryPlan":result["memoryPlan"],"counters":result["counters"]}));
            eprintln!("{}", summary.last().unwrap());
            if !complete {
                break;
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../debugging/thermal-verification/{prefix}-factor-separation.json"
        ));
        std::fs::write(path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();
        assert_eq!(summary.len(), cases.len());
        assert!(summary.iter().all(|s| s["status"] == "complete"));
    }
    #[test]
    #[ignore = "Native authoritative physical-domain export for saved field evidence"]
    fn controller_export_physical_domain() {
        let mut value = captured::captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, 1.);
        value["board"]["geometryDeviationMm"] = json!(0.005);
        value["board"]["maxNodes"] = json!(110000);
        value["budget"]["maxMemoryMiB"] = json!(768.);
        let mut request: Request = serde_json::from_value(value).unwrap();
        REFINEMENT_STUDY.set(Some(RefinementStudy {
            step_mm: 0.125,
            healing_length_mm: 0.16549169163435365,
            graded: true,
            mesh_only: false,
            domain_only: true,
            refine_constraints: false,
        }));
        let result = prepare(
            request.board.as_ref().unwrap(),
            &mut request.network,
            false,
            Instant::now(),
            request.budget.as_ref(),
        );
        REFINEMENT_STUDY.set(None);
        let result = result.unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../debugging/thermal-verification/controller-authoritative-physical-domain.json",
        );
        std::fs::write(path, serde_json::to_vec(&result.report).unwrap()).unwrap();
        eprintln!(
            "Native physical-domain report bytes={}, area={}",
            serde_json::to_vec(&result.report).unwrap().len(),
            result.report["areaMm2"]
        );
    }
    #[test]
    #[ignore = "Constraint/interior sizing mesh admission only, private experiment"]
    fn controller_constraint_halo_mesh_only_study() {
        let mut results = vec![];
        for global in [1., 0.5] {
            for local in [0.08274584581717682, 0.0625] {
                let mut value =
                    captured::captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, global);
                value["board"]["geometryDeviationMm"] = json!(0.005);
                value["board"]["maxNodes"] = json!(110000);
                value["budget"]["maxMemoryMiB"] = json!(768.);
                let mut request: Request = serde_json::from_value(value).unwrap();
                REFINEMENT_STUDY.set(Some(RefinementStudy {
                    step_mm: local,
                    healing_length_mm: 0.16549169163435365,
                    graded: true,
                    mesh_only: true,
                    domain_only: false,
                    refine_constraints: true,
                }));
                let start = Instant::now();
                let solved = prepare(
                    request.board.as_ref().unwrap(),
                    &mut request.network,
                    false,
                    start,
                    request.budget.as_ref(),
                );
                REFINEMENT_STUDY.set(None);
                let result = match solved {
                    Ok(s) => {
                        assert!(
                            s.report["maximumLocalConstraintEdgeMm"].as_f64().unwrap()
                                <= local * (1. + 1e-8)
                        );
                        json!({"status":"mesh-admitted","report":s.report})
                    }
                    Err(e) => json!({"status":"mesh-rejected","reason":e}),
                };
                results.push(json!({"globalEdgeMm":global,"localStepMm":local,"elapsedMs":start.elapsed().as_secs_f64()*1000.,"result":result}));
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../debugging/thermal-verification/controller-constraint-halo-mesh-only.json");
        std::fs::write(path, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
        eprintln!("{}", serde_json::to_string_pretty(&results).unwrap());
    }
    #[test]
    #[ignore = "One cfg(test) directional midpoint refinement pass, explicit saved plan"]
    fn controller_directional_midpoint_pass() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../debugging/thermal-verification");
        // Fixed names deliberately avoid any new public request/env mechanism.
        let plan: NestedMeshPlan = serde_json::from_reader(std::io::BufReader::new(
            std::fs::File::open(root.join("controller-directional-active-plan.json")).unwrap(),
        ))
        .unwrap();
        let independent = plan.physical_constraints_only || plan.independent_coverage;
        let max_nodes = if independent { 140000 } else { 110000 };
        assert!(plan.admitted && plan.planned_planar_nodes * 3 <= max_nodes);
        let mut request = captured::captured_board_request(
            "STM32G0_CAN_RS485",
            "U3",
            0.8,
            if independent { 0.5 } else { 1. },
        );
        request["board"]["geometryDeviationMm"] = json!(0.005);
        request["board"]["maxNodes"] = json!(max_nodes);
        request["budget"]["maxMemoryMiB"] = json!(if independent { 1024. } else { 768. });
        request["budget"]["maxSeconds"] = json!(240.);
        MIDPOINT_STUDY.with(|p| *p.borrow_mut() = Some(plan));
        let result = crate::thermal::run(serde_json::from_value(request.clone()).unwrap());
        MIDPOINT_STUDY.with(|p| *p.borrow_mut() = None);
        #[derive(serde::Serialize)]
        struct Saved<'a> {
            request: &'a Value,
            result: &'a Value,
        }
        serde_json::to_writer(
            std::io::BufWriter::new(
                std::fs::File::create(root.join("controller-directional-active-result.json"))
                    .unwrap(),
            ),
            &Saved {
                request: &request,
                result: &result,
            },
        )
        .unwrap();
        eprintln!(
            "directional midpoint status={} Tj={} elapsed={} memory={} quality={}",
            result["status"],
            result["temperaturesC"]["U3:junction"],
            result["elapsedMs"],
            result["memoryPlan"],
            result["quality"]
        );
        assert_eq!(result["status"], "complete", "{}", result["diagnostics"]);
    }
    #[test]
    fn healing_constraint_sizing_preserves_area_material_geometry_and_topology() {
        let square = Polygon::new(
            LineString::from(vec![(0., 0.), (1., 0.), (1., 1.), (0., 1.), (0., 0.)]),
            vec![],
        );
        let selected = MultiPolygon(vec![square.clone()]);
        let original: Vec<_> = square
            .exterior()
            .lines()
            .map(|l| ([l.start.x, l.start.y], [l.end.x, l.end.y]))
            .collect();
        let refined =
            study_split_constraints(original.clone(), &selected, 0.08, 0.5, 1000).unwrap();
        let old = super::super::mesh::arrange(original, 1., 1000, || Ok(())).unwrap();
        let new = super::super::mesh::arrange(refined.clone(), 1., 1000, || Ok(())).unwrap();
        let max_edge = |cdt: &spade::ConstrainedDelaunayTriangulation<Point2<f64>>| {
            cdt.undirected_edges()
                .filter(|e| e.is_constraint_edge())
                .map(|e| {
                    let [a, b] = e.vertices().map(|v| v.position());
                    (a.x - b.x).hypot(a.y - b.y)
                })
                .fold(0., f64::max)
        };
        assert!(
            max_edge(&old.cdt) > 0.9,
            "Interior refinement alone leaves the long constraint"
        );
        assert!(max_edge(&new.cdt) <= 0.08 * (1. + 1e-12));
        let signed_area: f64 = refined
            .iter()
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum::<f64>()
            / 2.;
        assert!((signed_area - 1.).abs() < 1e-14);
        assert_eq!(selected.unsigned_area(), 1.);
        let key = |p: [f64; 2]| (p[0].to_bits(), p[1].to_bits());
        let mut graph: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for (a, b) in refined {
            graph.entry(key(a)).or_default().insert(key(b));
            graph.entry(key(b)).or_default().insert(key(a));
        }
        assert!(graph.values().all(|neighbors| neighbors.len() == 2));
        let mut seen = BTreeSet::new();
        let mut pending = vec![*graph.keys().next().unwrap()];
        while let Some(p) = pending.pop() {
            if seen.insert(p) {
                pending.extend(graph[&p].iter().copied());
            }
        }
        assert_eq!(
            seen.len(),
            graph.len(),
            "Closed material boundary remains one connected component"
        );
        assert!(new.max_deviation_mm <= 1e-9);
    }
    #[test]
    fn indexed_topological_clipping_matches_boolean_with_holes_and_islands() {
        let ring = |x: f64, y: f64, w: f64, h: f64| {
            LineString::from(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)])
        };
        let material = MultiPolygon(vec![
            Polygon::new(ring(0., 0., 4., 4.), vec![ring(1., 1., 1., 1.)]),
            Polygon::new(ring(5., 0., 0.5, 0.5), vec![]),
        ]);
        let indexed = Clipper::new(&material);
        for ix in -2..24 {
            for iy in -2..18 {
                for size in [0.125, 0.5, 1.5] {
                    let x = ix as f64 * 0.25;
                    let y = iy as f64 * 0.25;
                    let triangle = Polygon::new(
                        LineString::from(vec![(x, y), (x + size, y), (x, y + size), (x, y)]),
                        vec![],
                    );
                    let expected = material
                        .intersection(&MultiPolygon(vec![triangle.clone()]))
                        .unsigned_area();
                    let actual = indexed.clip(&triangle).unsigned_area();
                    assert!(
                        (actual - expected).abs() <= 1e-8,
                        "{x} {y} {size}: {actual} {expected}"
                    );
                }
            }
        }
    }
}
