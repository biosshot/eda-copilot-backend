//! Physical PCB validation over the existing conservative copper preparation.
//! No second connectivity solver, CAD refill, topology repair or placement code.
use crate::geometry::{self, GeometryOptions, PolygonSnapshot, Provenance, Region};
use geo::line_intersection::{line_intersection, LineIntersection};
use geo::{
    BoundingRect, Coord, Distance, Euclidean, InteriorPoint, Intersects, Line, LineString, Point,
    Polygon,
};
use rstar::{
    primitives::{GeomWithData, Rectangle},
    RTree, AABB,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
type Result<T> = std::result::Result<T, String>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub board_json: String,
    pub rules: Vec<Value>,
    pub scope: Option<Vec<Value>>,
    pub geometry: Option<GeometryOptions>,
    pub budget: Option<Budget>,
    pub limits: Option<Caps>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Budget {
    pub max_seconds: Option<f64>,
    pub max_memory_mi_b: Option<f64>,
}
#[derive(Deserialize, Clone, Copy)]
#[serde(default, deny_unknown_fields)]
pub struct Caps {
    pub max_candidates: usize,
    pub max_exact_checks: usize,
    pub max_measurements: usize,
    pub max_report_vertices: usize,
    pub max_report_bytes: usize,
}
impl Default for Caps {
    fn default() -> Self {
        Self {
            max_candidates: 1_000_000,
            max_exact_checks: 5_000_000,
            max_measurements: 100_000,
            max_report_vertices: 2_000_000,
            max_report_bytes: 96 * 1024 * 1024,
        }
    }
}
struct Guard {
    start: Instant,
    seconds: f64,
    memory: usize,
    reserved: usize,
    caps: Caps,
    candidates: usize,
    exact: usize,
}
impl Guard {
    fn check(&self) -> Result<()> {
        if self.start.elapsed().as_secs_f64() > self.seconds {
            Err("GEOMETRY_TIMEOUT: native time budget exceeded".into())
        } else {
            Ok(())
        }
    }
    fn candidate(&mut self) -> Result<()> {
        self.candidates += 1;
        if self.candidates > self.caps.max_candidates {
            Err("GEOMETRY_CANDIDATE_LIMIT: broad phase cap reached".into())
        } else {
            self.check()
        }
    }
    fn exact(&mut self) -> Result<()> {
        self.exact += 1;
        if self.exact > self.caps.max_exact_checks {
            Err("GEOMETRY_EXACT_LIMIT: segment check cap reached".into())
        } else {
            if self.exact % 1024 == 0 {
                self.check()?;
            }
            Ok(())
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Measurement {
    pub id: String,
    pub rule: String,
    pub code: String,
    pub status: String,
    pub region_ids: Vec<String>,
    pub witness_region_ids: Vec<String>,
    pub source_ids: Vec<String>,
    pub nets: Vec<String>,
    pub layers: Vec<String>,
    pub actual_mm: f64,
    pub required_mm: f64,
    pub error_mm: f64,
    pub witness: [[f64; 2]; 2],
    pub outside_witness: Option<[f64; 2]>,
    pub message: String,
}
struct Shape {
    poly: Polygon<f64>,
    edges: Vec<Line<f64>>,
    index: EdgeIndex,
}
type EdgeIndex = RTree<GeomWithData<Rectangle<[f64; 2]>, usize>>;
fn index(lines: &[Line<f64>]) -> EdgeIndex {
    RTree::bulk_load(
        lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let r = l.bounding_rect();
                GeomWithData::new(
                    Rectangle::from_corners([r.min().x, r.min().y], [r.max().x, r.max().y]),
                    i,
                )
            })
            .collect(),
    )
}
fn polygon(s: &PolygonSnapshot) -> Polygon<f64> {
    let ring = |r: &Vec<[f64; 2]>| {
        let mut v: Vec<Coord<f64>> = r.iter().map(|p| Coord { x: p[0], y: p[1] }).collect();
        if v.first() != v.last() {
            v.push(v[0]);
        }
        LineString(v)
    };
    Polygon::new(ring(&s.outer), s.holes.iter().map(ring).collect())
}
fn shape(s: &PolygonSnapshot) -> Shape {
    let poly = polygon(s);
    let edges = std::iter::once(poly.exterior())
        .chain(poly.interiors())
        .flat_map(|r| r.lines())
        .collect::<Vec<_>>();
    let index = index(&edges);
    Shape { poly, edges, index }
}
fn xy(c: Coord<f64>) -> [f64; 2] {
    [c.x, c.y]
}
fn project(p: Coord<f64>, l: Line<f64>) -> Coord<f64> {
    let dx = l.end.x - l.start.x;
    let dy = l.end.y - l.start.y;
    let q = dx * dx + dy * dy;
    let t = if q == 0. {
        0.
    } else {
        ((p.x - l.start.x) * dx + (p.y - l.start.y) * dy) / q
    }
    .clamp(0., 1.);
    Coord {
        x: l.start.x + t * dx,
        y: l.start.y + t * dy,
    }
}
fn segment_distance(a: Line<f64>, b: Line<f64>) -> (f64, [[f64; 2]; 2]) {
    if let Some(hit) = line_intersection(a, b) {
        let p = match hit {
            LineIntersection::SinglePoint { intersection, .. } => intersection,
            LineIntersection::Collinear { intersection } => intersection.start,
        };
        return (0., [xy(p), xy(p)]);
    }
    [
        (a.start, project(a.start, b)),
        (a.end, project(a.end, b)),
        (project(b.start, a), b.start),
        (project(b.end, a), b.end),
    ]
    .into_iter()
    .map(|(p, q)| ((p.x - q.x).hypot(p.y - q.y), [xy(p), xy(q)]))
    .min_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap())
    })
    .unwrap()
}
fn distance(
    a: &Shape,
    b: &Shape,
    closed_regions: bool,
    guard: &mut Guard,
) -> Result<(f64, [[f64; 2]; 2])> {
    if closed_regions {
        for (p, other) in [
            (a.poly.exterior().0[0], &b.poly),
            (b.poly.exterior().0[0], &a.poly),
        ] {
            if other.intersects(&Point(p)) {
                return Ok((0., [xy(p), xy(p)]));
            }
        }
    }
    // Iterate the shorter boundary and query the larger indexed boundary.
    // Stable source-ID order must not force a large plane to scan every edge
    // against each small pad. Reverse endpoints back to the caller's order.
    if a.edges.len() > b.edges.len() {
        let (d, [p, q]) = distance(b, a, false, guard)?;
        return Ok((d, [q, p]));
    }
    guard.exact()?;
    let mut best = segment_distance(a.edges[0], b.edges[0]);
    // Exact point-to-segment upper bounds only seed pruning. Every segment
    // capable of improving that bound is still checked below; no sampling
    // approximation enters the result. R-tree rectangle distance is a lower
    // bound, so nearest iteration stops only beyond the current upper bound.
    for i in (0..a.edges.len()).step_by((a.edges.len() / 8).max(1)) {
        let p = a.edges[i].start;
        for (candidate, bound2) in b.index.nearest_neighbor_iter_with_distance_2(&[p.x, p.y]) {
            if bound2 > (best.0 + 1e-12).powi(2) {
                break;
            }
            guard.exact()?;
            let q = project(p, b.edges[candidate.data]);
            let found = ((p.x - q.x).hypot(p.y - q.y), [xy(p), xy(q)]);
            if found.0 < best.0 || found.0 == best.0 && found.1 < best.1 {
                best = found;
            }
        }
    }
    for &edge in &a.edges {
        let r = edge.bounding_rect();
        let pad = best.0 + 1e-12;
        let env = AABB::from_corners(
            [r.min().x - pad, r.min().y - pad],
            [r.max().x + pad, r.max().y + pad],
        );
        let mut candidates: Vec<_> = b
            .index
            .locate_in_envelope_intersecting(&env)
            .map(|v| v.data)
            .collect();
        candidates.sort_unstable();
        for i in candidates {
            guard.exact()?;
            let d = segment_distance(edge, b.edges[i]);
            if d.0 < best.0 || d.0 == best.0 && d.1 < best.1 {
                best = d;
            }
            if best.0 == 0. {
                return Ok(best);
            }
        }
    }
    Ok(best)
}
fn canonical_ring(v: &[[f64; 2]]) -> String {
    let mut r = v.to_vec();
    if r.first() == r.last() {
        r.pop();
    }
    let format = |r: &Vec<[f64; 2]>| {
        let i = (0..r.len())
            .min_by(|&a, &b| r[a].partial_cmp(&r[b]).unwrap())
            .unwrap();
        serde_json::to_string(
            &(0..r.len())
                .map(|j| r[(i + j) % r.len()])
                .collect::<Vec<_>>(),
        )
        .unwrap()
    };
    let a = format(&r);
    r.reverse();
    a.min(format(&r))
}
fn stable_key(r: &Region) -> String {
    let mut holes: Vec<_> = r.polygon.holes.iter().map(|v| canonical_ring(v)).collect();
    holes.sort();
    format!(
        "{}:{}:{}:{}:{}",
        r.source.kind,
        r.source.source_id.as_deref().unwrap_or(&r.source.path),
        r.layer,
        canonical_ring(&r.polygon.outer),
        holes.join(";")
    )
}
fn hash(s: &str) -> String {
    let h = s.as_bytes().iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    format!("{h:016x}")
}
fn source_id(p: &Provenance) -> String {
    p.source_id.clone().unwrap_or_else(|| p.path.clone())
}
fn classify(d: f64, e: f64, min: f64) -> &'static str {
    if d + e < min {
        "confirmed"
    } else if (d - e).max(0.) >= min {
        "pass"
    } else {
        "uncertain"
    }
}
fn measurement(
    rule: &str,
    code: &str,
    status: &str,
    rs: &[&Region],
    required: f64,
    error: f64,
    d: (f64, [[f64; 2]; 2]),
    message: &str,
) -> Measurement {
    let mut ids: Vec<_> = rs.iter().map(|r| r.id.clone()).collect();
    ids.sort();
    ids.dedup();
    let mut sources: Vec<_> = rs.iter().map(|r| source_id(&r.source)).collect();
    sources.sort();
    sources.dedup();
    let mut nets: Vec<_> = rs.iter().filter_map(|r| r.source.net.clone()).collect();
    nets.sort();
    nets.dedup();
    let mut layers: Vec<_> = rs.iter().map(|r| r.layer.clone()).collect();
    layers.sort();
    layers.dedup();
    Measurement {
        id: format!(
            "finding-{}",
            hash(&format!(
                "{rule}:{code}:{required}:{status}:{}",
                ids.join(":")
            ))
        ),
        rule: rule.into(),
        code: code.into(),
        status: status.into(),
        witness_region_ids: rs.iter().take(2).map(|r| r.id.clone()).collect(),
        region_ids: ids,
        source_ids: sources,
        nets,
        layers,
        actual_mm: d.0,
        required_mm: required,
        error_mm: error,
        witness: d.1,
        outside_witness: None,
        message: message.into(),
    }
}
fn point_polygon(v: &Value) -> Result<Polygon<f64>> {
    let points = |v: &Value| -> Result<Vec<crate::model::Point>> {
        let ring = v
            .as_array()
            .ok_or("GEOMETRY_SELECTOR: polygon ring expected")?;
        if ring.len() < 3 || ring.len() > 100_000 {
            return Err("GEOMETRY_SELECTOR: polygon ring length out of bounds".into());
        }
        ring.iter()
            .map(|p| {
                Ok(crate::model::Point {
                    x: p["x"]
                        .as_f64()
                        .filter(|v| v.is_finite() && v.abs() <= 10000.)
                        .ok_or("GEOMETRY_SELECTOR: finite x within +/-10000 mm expected")?,
                    y: p["y"]
                        .as_f64()
                        .filter(|v| v.is_finite() && v.abs() <= 10000.)
                        .ok_or("GEOMETRY_SELECTOR: finite y within +/-10000 mm expected")?,
                })
            })
            .collect()
    };
    let p = crate::model::Polygon {
        outer: points(&v["outer"])?,
        holes: v["holes"]
            .as_array()
            .map(|rs| rs.iter().map(points).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default(),
    };
    let snap = PolygonSnapshot {
        outer: p.outer.iter().map(|p| [p.x, p.y]).collect(),
        holes: p
            .holes
            .iter()
            .map(|r| r.iter().map(|p| [p.x, p.y]).collect())
            .collect(),
    };
    let poly = polygon(&snap);
    use geo::Validation;
    poly.check_validation()
        .map_err(|e| format!("GEOMETRY_SELECTOR: {e}"))?;
    Ok(poly)
}
fn select(
    scope: &Option<Vec<Value>>,
    regions: &[Region],
    board: &crate::model::Board,
) -> Result<BTreeSet<usize>> {
    let Some(scope) = scope else {
        return Ok((0..regions.len()).collect());
    };
    if scope.is_empty() {
        return Err("GEOMETRY_SELECTOR: explicit empty scope".into());
    }
    let mut selected = BTreeSet::new();
    if scope.len() > 1000 {
        return Err("GEOMETRY_SELECTOR: scope cap exceeded".into());
    }
    for selector in scope {
        let kind = selector["kind"].as_str().unwrap_or("");
        let fields: &[&str] = match kind {
            "padId" => &["kind", "id"],
            "pad" => &["kind", "designator", "number"],
            "component" => &["kind", "designator"],
            "net" => &["kind", "name"],
            "region" => &["kind", "layers", "polygon"],
            _ => return Err("GEOMETRY_SELECTOR: unsupported selector kind".into()),
        };
        if selector.as_object().is_none_or(|o| {
            o.len() != fields.len() || o.keys().any(|k| !fields.contains(&k.as_str()))
        }) {
            return Err("GEOMETRY_SELECTOR: malformed selector fields".into());
        }
        let poly = if kind == "region" {
            Some(point_polygon(&selector["polygon"])?)
        } else {
            None
        };
        let paths: BTreeSet<_> = board
            .pads
            .iter()
            .filter(|p| match kind {
                "padId" => p.source.id.as_deref() == selector["id"].as_str(),
                "pad" => {
                    p.number == selector["number"].as_str().unwrap_or("")
                        && p.component.map(|i| board.components[i].designator.as_str())
                            == selector["designator"].as_str()
                }
                "component" => {
                    p.component.map(|i| board.components[i].designator.as_str())
                        == selector["designator"].as_str()
                }
                _ => false,
            })
            .map(|p| p.source.path.as_str())
            .collect();
        if matches!(kind, "padId" | "pad") && paths.len() != 1 {
            return Err("GEOMETRY_SELECTOR: pad identity is missing or ambiguous".into());
        }
        if kind == "region"
            && selector["layers"].as_array().is_none_or(|ls| {
                ls.is_empty()
                    || ls.iter().any(|l| {
                        l.as_str()
                            .is_none_or(|name| !board.layers.iter().any(|b| b.name == name))
                    })
            })
        {
            return Err("GEOMETRY_SELECTOR: unknown/empty layer selection".into());
        }
        let hits: Vec<_> = regions
            .iter()
            .enumerate()
            .filter(|(_, r)| match kind {
                "padId" | "pad" | "component" => paths.contains(r.source.path.as_str()),
                "net" => r.source.net.as_deref() == selector["name"].as_str(),
                "region" => {
                    selector["layers"]
                        .as_array()
                        .is_some_and(|ls| ls.iter().any(|l| l.as_str() == Some(&r.layer)))
                        && poly.as_ref().unwrap().intersects(&polygon(&r.polygon))
                }
                _ => false,
            })
            .map(|(i, _)| i)
            .collect();
        if hits.is_empty() {
            return Err(format!(
                "GEOMETRY_SELECTOR: unsupported or unresolved selector {selector}"
            ));
        }
        selected.extend(hits);
    }
    Ok(selected)
}
fn add(rows: &mut Vec<Measurement>, m: Measurement, g: &Guard) -> Result<()> {
    if rows.len() >= g.caps.max_measurements {
        return Err("GEOMETRY_REPORT_LIMIT: measurement cap reached".into());
    }
    if g.reserved
        .saturating_add((rows.len() + 1).saturating_mul(8192))
        > g.memory
    {
        return Err("GEOMETRY_MEMORY_LIMIT: report allocation plan exceeds memory budget".into());
    }
    rows.push(m);
    Ok(())
}
pub fn validate_json(input: Value) -> Value {
    let start = Instant::now();
    match serde_json::from_value::<Request>(input)
        .map_err(|e| format!("GEOMETRY_INPUT: {e}"))
        .and_then(|r| validate(r, start))
    {
        Ok(v) => v,
        Err(e) => {
            let code = e.split(':').next().unwrap_or("GEOMETRY_REJECTED");
            json!({"operation":"geometry-validation","status":"rejected","diagnostics":[{"code":code,"message":e,"severity":"error"}],"elapsed_ms":start.elapsed().as_secs_f64()*1000.})
        }
    }
}
fn validate(request: Request, start: Instant) -> Result<Value> {
    let caps = request.limits.unwrap_or_default();
    let budget = request.budget.unwrap_or_default();
    let seconds = budget.max_seconds.unwrap_or(120.);
    let memory_mib = budget.max_memory_mi_b.unwrap_or(512.);
    if !seconds.is_finite()
        || seconds <= 0.
        || seconds > 3600.
        || !memory_mib.is_finite()
        || !(1. ..=4096.).contains(&memory_mib)
        || [
            caps.max_candidates,
            caps.max_exact_checks,
            caps.max_measurements,
            caps.max_report_vertices,
            caps.max_report_bytes,
        ]
        .iter()
        .any(|v| *v == 0)
        || caps.max_candidates > 2_000_000
        || caps.max_exact_checks > 20_000_000
        || caps.max_measurements > 200_000
        || caps.max_report_vertices > 4_000_000
        || caps.max_report_bytes > 128 * 1024 * 1024
    {
        return Err("GEOMETRY_LIMITS: invalid bounded budget/caps".into());
    }
    let options = request.geometry.unwrap_or_default();
    options
        .validate()
        .map_err(|e| format!("GEOMETRY_INPUT: {e}"))?;
    let mut guard = Guard {
        start,
        seconds,
        memory: (memory_mib * 1048576.) as usize,
        reserved: 0,
        caps,
        candidates: 0,
        exact: 0,
    };
    if request.board_json.len() > 32 * 1024 * 1024
        || request.board_json.len().saturating_mul(12)
            + options.max_generated_vertices.saturating_mul(128)
            > guard.memory
    {
        return Err(
            "GEOMETRY_MEMORY_LIMIT: preparation allocation plan exceeds memory budget".into(),
        );
    }
    if request.rules.is_empty() || request.rules.len() > 32 {
        return Err("GEOMETRY_INPUT: supply 1..32 rules".into());
    }
    let mut rules = BTreeMap::<String, f64>::new();
    for r in &request.rules {
        let kind = r["kind"].as_str().unwrap_or("");
        let required = match kind {
            "connectivity" => 0.,
            "clearance" | "edge-clearance" => r["minMm"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0. && *v <= 1000.)
                .ok_or("GEOMETRY_INPUT: finite minMm in 0..1000 required")?,
            _ => {
                return Err(format!(
                    "GEOMETRY_UNSUPPORTED_RULE: {kind}; channel topology is unavailable"
                ))
            }
        };
        if r.as_object().is_none_or(|o| {
            o.keys()
                .any(|k| k != "kind" && !(k == "minMm" && kind != "connectivity"))
        }) {
            return Err("GEOMETRY_INPUT: unknown rule field".into());
        }
        let key = format!("{kind}:{required}");
        rules.insert(key, required);
    }
    if rules.is_empty() || rules.len() > 32 {
        return Err("GEOMETRY_INPUT: supply 1..32 distinct rules".into());
    }
    let imported = crate::import_json(&request.board_json);
    let Some(board) = imported.board else {
        return Err(format!(
            "GEOMETRY_IMPORT: {}",
            serde_json::to_string(&imported.diagnostics).unwrap()
        ));
    };
    guard.check()?;
    let mut outside = Vec::<Region>::new();
    let mut outside_vertices = 0usize;
    let mut outside_overflow = false;
    let mut observe = |p: &Provenance, layer: &str, ps: &geo::MultiPolygon<f64>, error: f64| {
        for poly in &ps.0 {
            let n =
                poly.exterior().0.len() + poly.interiors().iter().map(|r| r.0.len()).sum::<usize>();
            if outside_vertices.saturating_add(n) > caps.max_report_vertices {
                outside_overflow = true;
                return;
            }
            let snapshot = PolygonSnapshot {
                outer: poly.exterior().0[..poly.exterior().0.len() - 1]
                    .iter()
                    .map(|p| [p.x, p.y])
                    .collect(),
                holes: poly
                    .interiors()
                    .iter()
                    .map(|r| r.0[..r.0.len() - 1].iter().map(|p| [p.x, p.y]).collect())
                    .collect(),
            };
            outside_vertices +=
                snapshot.outer.len() + snapshot.holes.iter().map(Vec::len).sum::<usize>();
            if outside_vertices > caps.max_report_vertices {
                outside_overflow = true;
                return;
            }
            use geo::Area;
            outside.push(Region {
                id: String::new(),
                layer: layer.into(),
                source: p.clone(),
                polygon: snapshot,
                area_mm2: poly.unsigned_area(),
                approximation_mm: error,
            });
        }
    };
    let mut prep = geometry::prepare_observed(&board, options, Some(&mut observe));
    guard.check()?;
    if outside_overflow {
        return Err("GEOMETRY_REPORT_LIMIT: outside witness vertex cap reached".into());
    }
    if prep.status == "invalid" {
        return Err(format!(
            "GEOMETRY_PREPARATION: {}",
            serde_json::to_string(&prep.diagnostics).unwrap()
        ));
    }
    let mut vertices = outside_vertices;
    for r in &prep.regions {
        vertices += r.polygon.outer.len() + r.polygon.holes.iter().map(Vec::len).sum::<usize>();
    }
    guard.reserved = vertices
        .saturating_mul(384)
        .saturating_add(request.board_json.len().saturating_mul(12))
        .saturating_add(prep.contacts.len().saturating_mul(1024))
        .saturating_add(prep.regions.len().saturating_mul(1024));
    if vertices > caps.max_report_vertices || guard.reserved > guard.memory {
        return Err("GEOMETRY_MEMORY_LIMIT: validation polygon/index plan exceeds budget".into());
    }
    prep.layers.clear();
    prep.holes.clear();
    prep.sources.clear();
    let inside_count = prep.regions.len();
    let mut regions = std::mem::take(&mut prep.regions);
    regions.extend(outside);
    let mut remap = BTreeMap::new();
    let mut canonical = BTreeMap::new();
    for r in &mut regions {
        let key = stable_key(r);
        let id = format!("region-{}", hash(&key));
        if canonical.get(&id).is_some_and(|old| old != &key) {
            return Err("GEOMETRY_IDENTITY: region hash collision".into());
        }
        canonical.insert(id.clone(), key);
        if !r.id.is_empty() {
            remap.insert(r.id.clone(), id.clone());
        }
        r.id = id;
    }
    drop(canonical);
    // Identical physical source fragments have one validation identity.
    let outside_ids: BTreeSet<_> = regions[inside_count..]
        .iter()
        .map(|r| r.id.clone())
        .collect();
    regions.sort_by(|a, b| a.id.cmp(&b.id));
    regions.dedup_by(|a, b| a.id == b.id);
    let selected = select(&request.scope, &regions, &board)?;
    let region_index: BTreeMap<_, _> = regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.as_str(), i))
        .collect();
    let shapes: Vec<_> = regions.iter().map(|r| shape(&r.polygon)).collect();
    let mut rows = Vec::new();
    let mut coverage = Vec::new();
    let mut diagnostics:Vec<Value>=prep.diagnostics.iter().map(|d|json!({"code":d.code,"path":d.path,"source_id":d.source_id,"message":d.message,"severity":"warning"})).collect();
    diagnostics.extend(imported.diagnostics.iter().map(|d|json!({"code":d.code,"path":d.path,"source_id":d.source_id,"message":d.message,"severity":"warning"})));
    let mut incomplete = !prep.diagnostics.is_empty();
    if regions.iter().any(|r| r.source.source_id.is_none()) {
        incomplete = true;
        diagnostics.push(json!({"code":"GEOMETRY_PATH_IDENTITY","severity":"warning","message":"Sources without IDs use path identity; reorder stability unavailable for those sources"}));
    }
    for (rule_key, required) in rules {
        let kind = rule_key.split(':').next().unwrap();
        let mut rule_partial = incomplete;
        let before = rows.len();
        if kind == "clearance" {
            let bounds: Vec<_> = shapes
                .iter()
                .map(|s| s.poly.bounding_rect().unwrap())
                .collect();
            let tree = RTree::bulk_load(
                bounds
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        GeomWithData::new(
                            Rectangle::from_corners([r.min().x, r.min().y], [r.max().x, r.max().y]),
                            i,
                        )
                    })
                    .collect(),
            );
            let max_error = regions
                .iter()
                .map(|r| r.approximation_mm)
                .fold(0., f64::max);
            for &a in &selected {
                let r = bounds[a];
                let padding = required
                    + regions[a].approximation_mm
                    + max_error
                    + 2. * prep.boolean_error_bound_mm;
                let env = AABB::from_corners(
                    [r.min().x - padding, r.min().y - padding],
                    [r.max().x + padding, r.max().y + padding],
                );
                let mut nearby: Vec<_> = tree
                    .locate_in_envelope_intersecting(&env)
                    .map(|v| v.data)
                    .collect();
                nearby.sort_unstable();
                for b in nearby {
                    if a == b || regions[a].layer != regions[b].layer {
                        continue;
                    }
                    let pair = (
                        regions[a].id.clone().min(regions[b].id.clone()),
                        regions[a].id.clone().max(regions[b].id.clone()),
                    );
                    if pair.0 == pair.1 || (selected.contains(&b) && a > b) {
                        continue;
                    }
                    let (na, nb) = (&regions[a].source.net, &regions[b].source.net);
                    guard.candidate()?;
                    if na.is_some() && na == nb {
                        continue;
                    }
                    let error = regions[a].approximation_mm
                        + regions[b].approximation_mm
                        + 2. * prep.boolean_error_bound_mm;
                    let d = distance(&shapes[a], &shapes[b], true, &mut guard)?;
                    let mut status = classify(d.0, error, required);
                    let unknown = na.as_ref().is_none_or(|n| n.trim().is_empty())
                        || nb.as_ref().is_none_or(|n| n.trim().is_empty());
                    if unknown {
                        rule_partial = true;
                        if status != "pass" {
                            status = "uncertain";
                        }
                    }
                    if status != "pass" {
                        if status == "uncertain" {
                            rule_partial = true;
                        }
                        add(
                            &mut rows,
                            measurement(
                                kind,
                                if unknown {
                                    "unknown_net_clearance"
                                } else {
                                    "copper_clearance"
                                },
                                status,
                                &[&regions[a], &regions[b]],
                                required,
                                error,
                                d,
                                "Minimum closed copper-region distance on the same layer",
                            ),
                            &guard,
                        )?;
                    }
                }
            }
        } else if kind == "edge-clearance" {
            let outline = prep
                .outline
                .as_ref()
                .ok_or("GEOMETRY_OUTLINE: physical outline unavailable")?;
            let border = shape(outline);
            for &i in &selected {
                guard.candidate()?;
                let r = &regions[i];
                let error = r.approximation_mm + 2. * prep.boolean_error_bound_mm;
                let d = distance(&shapes[i], &border, false, &mut guard)?;
                let is_outside = outside_ids.contains(&r.id);
                // A point with clearance from both contours certifies material
                // outside the board. Area/perimeter heuristics are not proof.
                let outside_witness = if is_outside {
                    shapes[i]
                        .poly
                        .interior_point()
                        .filter(|p| {
                            std::iter::once(shapes[i].poly.exterior())
                                .chain(shapes[i].poly.interiors())
                                .all(|ring| Euclidean.distance(p, ring) > error)
                                && std::iter::once(border.poly.exterior())
                                    .chain(border.poly.interiors())
                                    .all(|ring| Euclidean.distance(p, ring) > error)
                                && !border.poly.intersects(p)
                        })
                        .map(|p| [p.x(), p.y()])
                } else {
                    None
                };
                let status = if is_outside {
                    if outside_witness.is_some() {
                        "confirmed"
                    } else {
                        "uncertain"
                    }
                } else if required == 0. && d.0 <= error {
                    // A nonnegative nominal spacing does not certify that
                    // approximated copper remains inside material at min=0.
                    "uncertain"
                } else {
                    classify(d.0, error, required)
                };
                if status == "uncertain" {
                    rule_partial = true;
                }
                let mut measured = measurement(
                    kind,
                    if is_outside {
                        "copper_outside_material"
                    } else {
                        "board_edge_clearance"
                    },
                    status,
                    &[r],
                    required,
                    error,
                    d,
                    if is_outside {
                        "Physical copper outside the outline or inside a board cutout; positive edge distance does not legalize it"
                    } else {
                        "Minimum distance to external board edge and physical cutouts; drills excluded"
                    },
                );
                measured.outside_witness = outside_witness;
                add(&mut rows, measured, &guard)?;
            }
        } else {
            rule_partial = true;
            diagnostics.push(json!({"code":"GEOMETRY_OPEN_INTENT_UNAVAILABLE","severity":"warning","message":"Connectivity reuses reliable shorts and uncertain graph contacts; net labels/islands do not specify an open-check intent"}));
            let selected_ids: BTreeSet<_> =
                selected.iter().map(|&i| regions[i].id.clone()).collect();
            for f in &prep.findings {
                if f.code != "physical_short" {
                    continue;
                }
                let ids: Vec<_> = f.region_ids.iter().filter_map(|id| remap.get(id)).collect();
                if !ids.iter().any(|id| selected_ids.contains(*id)) {
                    continue;
                }
                let rs: Vec<_> = ids
                    .iter()
                    .filter_map(|id| region_index.get(id.as_str()).map(|&i| &regions[i]))
                    .collect();
                if rs.is_empty() {
                    return Err("GEOMETRY_IDENTITY: graph short lost source regions".into());
                }
                let contact=prep.contacts.iter().filter(|c|c.reliable&&f.region_ids.contains(&c.a)&&f.region_ids.contains(&c.b)).min_by_key(|c|{let a=remap.get(&c.a);let b=remap.get(&c.b);(a.cloned(),b.cloned())}).ok_or("GEOMETRY_IDENTITY: confirmed short lacks existing reliable contact witness")?;
                let ia = regions
                    .iter()
                    .position(|r| Some(&r.id) == remap.get(&contact.a))
                    .ok_or("GEOMETRY_IDENTITY: contact source absent")?;
                let ib = regions
                    .iter()
                    .position(|r| Some(&r.id) == remap.get(&contact.b))
                    .ok_or("GEOMETRY_IDENTITY: contact source absent")?;
                let d = distance(&shapes[ia], &shapes[ib], true, &mut guard)?;
                let mut m = measurement(
                    kind,
                    "physical_short",
                    "confirmed",
                    &rs,
                    0.,
                    0.,
                    d,
                    &f.message,
                );
                m.witness_region_ids = vec![regions[ia].id.clone(), regions[ib].id.clone()];
                m.nets = f.nets.clone();
                add(&mut rows, m, &guard)?;
            }
            for contact in prep.contacts.iter().filter(|c| !c.reliable) {
                let Some(a) = remap.get(&contact.a) else {
                    continue;
                };
                let Some(b) = remap.get(&contact.b) else {
                    continue;
                };
                if !selected_ids.contains(a) && !selected_ids.contains(b) {
                    continue;
                }
                let ia = *region_index
                    .get(a.as_str())
                    .ok_or("GEOMETRY_IDENTITY: contact source absent")?;
                let ib = *region_index
                    .get(b.as_str())
                    .ok_or("GEOMETRY_IDENTITY: contact source absent")?;
                let d = distance(&shapes[ia], &shapes[ib], true, &mut guard)?;
                add(
                    &mut rows,
                    measurement(
                        kind,
                        "contact_uncertain",
                        "uncertain",
                        &[&regions[ia], &regions[ib]],
                        0.,
                        regions[ia].approximation_mm
                            + regions[ib].approximation_mm
                            + 2. * prep.boolean_error_bound_mm,
                        d,
                        "Existing conservative graph cannot certify this contact",
                    ),
                    &guard,
                )?;
            }
        }
        coverage.push(json!({"rule":kind,"required_mm":required,"status":if rule_partial{"partial"}else{"complete"},"measurements":rows.len()-before,"opens":if kind=="connectivity"{"unavailable"}else{"not-applicable"}}));
    }
    if rows.iter().any(|r| r.code == "unknown_net_clearance") {
        diagnostics.push(json!({"code":"GEOMETRY_UNKNOWN_NET","severity":"warning","message":"Unknown-net nearby copper is checked conservatively; missing labels cannot establish equal or different electrical nets"}));
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.dedup_by(|a, b| a.id == b.id);
    let complete = coverage.iter().all(|c| c["status"] == "complete");
    let findings: Vec<_> = rows.iter().filter(|m| m.status == "confirmed").collect();
    let uncertain = rows.iter().filter(|m| m.status == "uncertain").count();
    drop(shapes);
    let graph = json!({"contacts":prep.contacts.iter().filter_map(|c|Some(json!({"a":remap.get(&c.a)?,"b":remap.get(&c.b)?,"kind":c.kind,"reliable":c.reliable,"drill_id":c.drill_id}))).collect::<Vec<_>>(),"islands":prep.islands.iter().map(|i|json!({"id":i.id,"nets":i.nets,"has_unknown_net":i.has_unknown_net,"region_ids":i.region_ids.iter().filter_map(|id|remap.get(id)).collect::<Vec<_>>()})).collect::<Vec<_>>(),"net_islands":prep.net_islands,"open_check":"unavailable: no terminal-group intent"});
    // Store only source polygons, not aggregate copper/layers a second time.
    let mut unique_regions = BTreeMap::new();
    for r in &regions {
        unique_regions.insert(r.id.clone(), r);
    }
    let result = json!({"operation":"geometry-validation","status":if complete{"complete"}else{"partial"},"units":"mm","coordinates":"absolute-y-down","method":"existing conservative connectivity; R-tree broad phase and robust segment/closed-region CPU/F64 distance","diagnostics":diagnostics,"coverage":coverage,"measurements":rows,"violations":findings,"uncertain_count":uncertain,"outline":prep.outline,"regions":unique_regions.values().collect::<Vec<_>>(),"graph":graph,"fills":prep.fills,"excluded":prep.excluded,"statistics":{"regions":unique_regions.len(),"selected_regions":selected.len(),"outside_regions":outside_ids.len(),"report_vertices":vertices,"candidate_pairs":guard.candidates,"exact_segment_checks":guard.exact,"preparation":prep.statistics},"boolean_error_bound_mm":prep.boolean_error_bound_mm,"elapsed_ms":start.elapsed().as_secs_f64()*1000.});
    guard.check()?;
    let bytes = serde_json::to_vec(&result)
        .map_err(|e| format!("GEOMETRY_SERIALIZATION: {e}"))?
        .len();
    if bytes > caps.max_report_bytes || bytes.saturating_mul(3) > guard.memory {
        return Err("GEOMETRY_REPORT_LIMIT: serialized report exceeds byte/memory budget".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_observer_preserves_base_preparation_bytes() {
        let mut input: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/analysis-dc-coupon.json"
        ))
        .unwrap();
        input["outline"] = json!([{"x":1,"y":-3},{"x":12,"y":-3},{"x":12,"y":8},{"x":1,"y":8}]);
        let board = crate::import_json(&input.to_string()).board.unwrap();
        let a = geometry::prepare(&board, GeometryOptions::default());
        let mut outside = 0;
        let mut observer =
            |_: &Provenance, _: &str, ps: &geo::MultiPolygon<f64>, _: f64| outside += ps.0.len();
        let b = geometry::prepare_observed(&board, GeometryOptions::default(), Some(&mut observer));
        assert!(outside > 0);
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
}
