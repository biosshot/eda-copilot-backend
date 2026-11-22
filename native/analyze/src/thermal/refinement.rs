//! Small opt-in, bounded thermal mesh refinement. No public mesh replay/cache.
use super::*;
use geo::{Contains, Coord, CoordinatePosition, LineString, MultiPolygon, Point, Polygon};
use rstar::{RTree, RTreeObject, AABB};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;
fn bad(message: impl std::fmt::Display) -> String {
    format!("THERMAL_REFINEMENT: {message}")
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Policy {
    pub max_passes: usize,
    pub relative_tolerance: f64,
    pub validation_max_edge_mm: f64,
}
impl Policy {
    fn validate(&self, board: &board::BoardInput, request: &Request) -> Result<()> {
        if !(1..=6).contains(&self.max_passes)
            || !self.relative_tolerance.is_finite()
            || !(1e-4..=0.02).contains(&self.relative_tolerance)
            || !self.validation_max_edge_mm.is_finite()
            || self.validation_max_edge_mm <= 0.
            || self.validation_max_edge_mm >= board.max_edge_mm
        {
            return Err(bad("Expected maxPasses1..6, relativeTolerance1e-4.. .02 and 0<validationMaxEdgeMm<maxEdgeMm"));
        }
        if request.initial.as_ref().is_some_and(|s| s != "steady") {
            return Err(bad("Continuation cannot be remapped by mesh refinement"));
        }
        Ok(())
    }
}

/// Geometry and common-origin coordinates retained between solves; no factor,
/// assembled physical system, link manifest or intermediate public flow arrays.
#[cfg_attr(test, derive(Clone))]
struct Field {
    points: Vec<[f64; 2]>,
    triangles: Vec<[usize; 3]>,
    fractions: Vec<Vec<f64>>,
    layers: Vec<board::Layer>,
    domain: MultiPolygon,
    boundary_source: Option<super::boundary::Source>,
    axis: Vec<f64>,
    origin: f64,
    // Layer, planar vertex, time. Unsupported vertices remain NaN internally.
    values: Vec<Vec<Vec<f64>>>,
    compact: BTreeMap<String, Vec<f64>>,
    trace_mm: f64,
    summary: Value,
    seed_refinement: Value,
}

fn array<T: serde::de::DeserializeOwned>(v: &Value, what: &str) -> Result<T> {
    T::deserialize(v).map_err(|e| bad(format!("Invalid internal {what}: {e}")))
}
#[derive(Deserialize)]
struct DomainPolygon {
    outer: Vec<[f64; 2]>,
    holes: Vec<Vec<[f64; 2]>>,
}
fn polygon(s: &DomainPolygon) -> Polygon {
    let ring = |r: &Vec<[f64; 2]>| {
        let mut p: Vec<_> = r.iter().map(|p| Coord { x: p[0], y: p[1] }).collect();
        if p.first() != p.last() {
            p.push(p[0]);
        }
        LineString::new(p)
    };
    Polygon::new(ring(&s.outer), s.holes.iter().map(ring).collect())
}
impl Field {
    fn from_report(report: &Value) -> Result<Self> {
        let origin = report["coordinateFrame"]["originC"]
            .as_f64()
            .ok_or_else(|| bad("Missing internal coordinate frame"))?;
        let rows: Vec<Vec<f64>> = array(
            &report["coordinateFrame"]["temperaturesK"],
            "coordinate rows",
        )?;
        let ids: Vec<String> = array(&report["system"]["nodeIds"], "coordinate IDs")?;
        let axis: Vec<f64> = array(&report["axis"], "axis")?;
        if rows.len() != axis.len() || rows.iter().any(|r| r.len() != ids.len()) {
            return Err(bad("Coordinate axis/order mismatch"));
        }
        let points: Vec<[f64; 2]> = array(&report["spatial"]["mesh"]["pointsMm"], "points")?;
        let triangles: Vec<[usize; 3]> =
            array(&report["spatial"]["mesh"]["triangles"], "triangles")?;
        let fractions = array(
            &report["spatial"]["mesh"]["copperAreaFractions"],
            "fractions",
        )?;
        let layers: Vec<board::Layer> = array(&report["spatial"]["layers"], "layers")?;
        let mut values = vec![vec![vec![f64::NAN; axis.len()]; points.len()]; layers.len()];
        let mut compact = BTreeMap::new();
        for (i, id) in ids.iter().enumerate() {
            if let Some(rest) = id.strip_prefix("pcb:") {
                if let Some((layer, vertex)) = rest.rsplit_once(':') {
                    let li = layers
                        .iter()
                        .position(|l| l.name == layer)
                        .ok_or_else(|| bad("Unknown field layer"))?;
                    let p: usize = vertex.parse().map_err(|_| bad("Unknown field vertex"))?;
                    if p >= points.len() {
                        return Err(bad("Field vertex outside mesh"));
                    }
                    for (j, row) in rows.iter().enumerate() {
                        values[li][p][j] = row[i];
                    }
                }
            } else {
                compact.insert(id.clone(), rows.iter().map(|r| r[i]).collect());
            }
        }
        let domain: Vec<DomainPolygon> = array(
            &report["spatial"]["geometry"]["physicalDomain"],
            "physical domain",
        )?;
        let trace_mm = (2.
            * report["spatial"]["geometry"]["booleanErrorBoundMm"]
                .as_f64()
                .unwrap_or(0.))
        .max(1e-7);
        let summary = json!({"quality":report["quality"],"counters":report["counters"],"memoryPlan":report["memoryPlan"],"nodes":ids.len(),"planarVertices":points.len(),"triangles":triangles.len(),"axisSamples":axis.len()});
        let seed_refinement = if report["spatial"]["mesh"]["nested"]["seedRefinement"].is_array() {
            report["spatial"]["mesh"]["nested"]["seedRefinement"].clone()
        } else {
            report["spatial"]["mesh"]["localRefinement"].clone()
        };
        Ok(Self {
            points,
            triangles,
            fractions,
            layers,
            domain: MultiPolygon(domain.iter().map(polygon).collect()),
            boundary_source: super::boundary::Source::from_report(report),
            axis,
            origin,
            values,
            compact,
            trace_mm,
            summary,
            seed_refinement,
        })
    }
    fn bytes(&self) -> usize {
        // Includes nested Vec allocations, RTree/sample work admission separately.
        self.points.capacity() * 16
            + self.boundary_source.as_ref().map_or(0, super::boundary::Source::bytes)
            + self.triangles.capacity() * 24
            + self
                .fractions
                .iter()
                .map(|v| v.capacity() * 8 + 24)
                .sum::<usize>()
            + self
                .values
                .iter()
                .map(|l| l.capacity() * 24 + l.iter().map(|v| v.capacity() * 8).sum::<usize>())
                .sum::<usize>()
            + self
                .domain
                .0
                .iter()
                .map(|p| {
                    p.exterior().0.capacity() * 16
                        + p.interiors()
                            .iter()
                            .map(|r| r.0.capacity() * 16 + 24)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self.axis.capacity() * 8
            + self
                .compact
                .iter()
                .map(|(k, v)| k.capacity() + v.capacity() * 8 + 96)
                .sum::<usize>()
            + 16384
            + json_heap_bytes(&self.seed_refinement)
    }
}

fn json_heap_bytes(v: &Value) -> usize {
    match v {
        Value::String(s) => s.capacity(),
        Value::Array(a) => {
            a.capacity() * std::mem::size_of::<Value>()
                + a.iter().map(json_heap_bytes).sum::<usize>()
        }
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| k.capacity() + 96 + json_heap_bytes(v))
            .sum(),
        _ => 0,
    }
}

#[derive(Clone)]
struct Entry {
    index: usize,
    bounds: AABB<[f64; 2]>,
}
impl RTreeObject for Entry {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.bounds
    }
}
fn bounds(points: &[[f64; 2]]) -> AABB<[f64; 2]> {
    AABB::from_corners(
        [
            points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        ],
        [
            points
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max),
            points
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max),
        ],
    )
}
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
struct Interpolator<'a> {
    field: &'a Field,
    tree: RTree<Entry>,
}
impl<'a> Interpolator<'a> {
    fn new(f: &'a Field) -> Self {
        Self {
            field: f,
            tree: RTree::bulk_load(
                f.triangles
                    .iter()
                    .enumerate()
                    .map(|(index, t)| Entry {
                        index,
                        bounds: bounds(&t.map(|i| f.points[i])),
                    })
                    .collect(),
            ),
        }
    }
    fn at(&self, p: [f64; 2]) -> Option<([usize; 3], [f64; 3])> {
        let mut found = None;
        for e in self
            .tree
            .locate_in_envelope_intersecting(&AABB::from_corners(
                [p[0] - 1e-12, p[1] - 1e-12],
                [p[0] + 1e-12, p[1] + 1e-12],
            ))
        {
            let ids = self.field.triangles[e.index];
            if self
                .field
                .values
                .iter()
                .any(|l| ids.iter().any(|i| !l[*i][0].is_finite()))
            {
                continue;
            }
            let [a, b, c] = ids.map(|i| self.field.points[i]);
            let det = cross(a, b, c);
            if det.abs() < 1e-24 {
                continue;
            }
            let v = cross(a, p, c) / det;
            let w = cross(a, b, p) / det;
            let u = 1. - v - w;
            let quality = u.min(v).min(w);
            if quality < -1e-9 || u.max(v).max(w) > 1. + 1e-9 {
                continue;
            }
            if found
                .as_ref()
                .is_none_or(|(_, _, q): &([usize; 3], [f64; 3], f64)| quality > *q)
            {
                let ww = [u.max(0.), v.max(0.), w.max(0.)];
                let sum = ww.iter().sum::<f64>();
                found = Some((ids, ww.map(|v| v / sum), quality));
            }
        }
        found.map(|(i, w, _)| (i, w))
    }
}
fn check(start: Instant, seconds: f64) -> Result<()> {
    if start.elapsed().as_secs_f64() > seconds {
        Err("THERMAL_DEADLINE: Total refinement wall-clock budget exceeded".into())
    } else {
        Ok(())
    }
}
fn admit(bytes: usize, budget: usize) -> Result<()> {
    if bytes > budget {
        Err(format!(
            "THERMAL_BUDGET: Refinement simultaneous workspace plan {bytes} exceeds {budget} bytes"
        ))
    } else {
        Ok(())
    }
}
fn compare(
    a: &Field,
    b: &Field,
    r: &Request,
    tolerance: f64,
    start: Instant,
    seconds: f64,
) -> Result<Value> {
    let _phase=profile::timer(profile::Phase::Comparison);
    if a.axis != b.axis
        || a.layers
            .iter()
            .map(|l| &l.name)
            .ne(b.layers.iter().map(|l| &l.name))
        || a.domain != b.domain
    {
        return Err(bad(
            "Refinement changes physical domain, layer identity or observation axis",
        ));
    }
    check(start, seconds)?;
    let aa = Interpolator::new(a);
    let bb = Interpolator::new(b);
    let same_source_identity = matches!((&a.boundary_source,&b.boundary_source),(Some(x),Some(y)) if x.identity==y.identity);
    let al = super::boundary::Localizer::new(&a.domain,a.boundary_source.as_ref(),same_source_identity);
    let bl = super::boundary::Localizer::new(&b.domain,b.boundary_source.as_ref(),same_source_identity);
    let mut localization_records=vec![];let mut localization_count=0;let mut maximum_displacement=0f64;
    let mut missing_records=vec![];
    #[derive(Clone)]
    struct Witness {point:[f64;2],layer:usize,time:usize,coarse:f64,fine:f64,ambient:f64,difference:f64,kind:&'static str}
    let mut witnesses:Vec<Option<Witness>>=vec![None;a.axis.len()];
    let mut seen = BTreeSet::new();
    let mut max_difference = vec![0f64; a.axis.len()];
    let mut rise = vec![0f64; a.axis.len()];
    let mut count = 0usize;
    let mut voids = 0usize;
    let mut missing = 0usize;
    let physical = |p: [f64; 2]| {
        b.domain.coordinate_position(&Coord { x: p[0], y: p[1] })
            != geo::coordinate_position::CoordPos::Outside
    };
    let mut sample = |p: [f64; 2], kind: &'static str, boundary: bool| -> Result<()> {
        check(start, seconds)?;
        if !seen.insert((p[0].to_bits(), p[1].to_bits())) {
            return Ok(());
        }
        if seen.len() > 1_000_000 {
            return Err(bad(
                "Physical comparison sample workspace exceeds one million points",
            ));
        }
        let mut ca=aa.at(p);let mut cb=bb.at(p);
        if boundary {for (name,at,localizer,location) in [("coarse",&aa,&al,&mut ca),("fine",&bb,&bl,&mut cb)] {
            if location.is_some(){continue;}let Some(proof)=localizer.at(p)else{continue;};let Some(value)=at.at(proof.point)else{continue;};*location=Some(value);
            localization_count+=1;maximum_displacement=maximum_displacement.max(proof.displacement);
            if localization_records.len()<100 {localization_records.push(json!({"mesh":name,"kind":kind,"correspondence":proof.report}));}
        }}
        let (Some((ai, aw)), Some((bi, bw))) = (ca, cb) else {
            missing += 1;
            if missing_records.len()<100 {missing_records.push(json!({"pointMm":p,"kind":kind,"boundary":boundary,"coarseAvailable":ca.is_some(),"fineAvailable":cb.is_some(),"sourceIdentityMatches":same_source_identity}));}
            return Ok(());
        };
        count += 1;
        for li in 0..b.layers.len() {
            for (j, &time) in a.axis.iter().enumerate() {
                let av = (0..3).map(|k| aw[k] * a.values[li][ai[k]][j]).sum::<f64>();
                let bv = (0..3).map(|k| bw[k] * b.values[li][bi[k]][j]).sum::<f64>();
                let difference = ((av - bv) + (a.origin - b.origin)).abs();
                rise[j] = rise[j].max((bv + (b.origin - r.ambient_c.sample(time))).abs());
                if witnesses[j].is_none() || difference > max_difference[j] {
                    max_difference[j] = difference;
                    witnesses[j]=Some(Witness{point:p,layer:li,time:j,coarse:av+a.origin,fine:bv+b.origin,ambient:r.ambient_c.sample(time),difference,kind});
                }
            }
        }
        Ok(())
    };
    for (f, kind) in [(a, "coarseNodes"), (b, "fineNodes")] {
        for (i, p) in f.points.iter().enumerate() {
            if !f.values[0][i][0].is_finite() {
                continue;
            }
            if physical(*p) {
                sample(*p, kind,b.domain.coordinate_position(&Coord{x:p[0],y:p[1]})==geo::coordinate_position::CoordPos::OnBoundary)?;
            } else {
                voids += 1;
            }
        }
    }
    let trace = a.trace_mm.max(b.trace_mm);
    for polygon in &b.domain.0 {
        for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
            for segment in ring.0.windows(2) {
                let x = segment[0];
                let y = segment[1];
                let mid = [(x.x + y.x) / 2., (x.y + y.y) / 2.];
                sample([x.x, x.y], "boundaryVertex",true)?;
                sample(mid, "boundaryMidpoint",true)?;
                let dx = y.x - x.x;
                let dy = y.y - x.y;
                let length = dx.hypot(dy);
                if length > 0. {
                    for p in [[x.x, x.y], mid] {
                        for sign in [-1., 1.] {
                            let q = [
                                p[0] - sign * dy / length * trace,
                                p[1] + sign * dx / length * trace,
                            ];
                            if physical(q) {
                                sample(q, "inwardTrace",false)?;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut compact_delta = 0f64;
    let mut compact_pass = true;
    if a.compact.keys().ne(b.compact.keys()) {
        return Err(bad("Compact node identities change under refinement"));
    }
    for (id, values) in &b.compact {
        for (j, &v) in values.iter().enumerate() {
            let delta = ((a.compact[id][j] - v) + (a.origin - b.origin)).abs();
            let signal = (v + (b.origin - r.ambient_c.sample(a.axis[j]))).abs();
            compact_delta = compact_delta.max(delta);
            compact_pass &= delta <= tolerance * signal + 1e-9;
        }
    }
    if count == 0 {
        return Err(bad("No computed physical points for spatial comparison"));
    }
    let converged = missing == 0
        && compact_pass
        && max_difference
            .iter()
            .zip(&rise)
            .all(|(d, s)| *d <= tolerance * s + 1e-9);
    let witness_json=|w:&Witness|json!({"pointMm":w.point,"layer":b.layers[w.layer].name,"timeS":a.axis[w.time],"differenceK":w.difference,"kind":w.kind,"coarseTemperatureC":w.coarse,"fineTemperatureC":w.fine,"ambientC":w.ambient,"denominatorK":rise[w.time],"relativeChange":if rise[w.time]>1e-9{Some(w.difference/rise[w.time])}else{None},"gateAllowanceK":tolerance*rise[w.time]+1e-9,"temperatureUnit":"degC","differenceUnit":"K","normalization":"Maximum physical fine-field rise relative to instantaneous ambient at this output/event time"});
    let absolute=witnesses.iter().flatten().max_by(|x,y|x.difference.total_cmp(&y.difference)).map(witness_json).unwrap_or(json!(null));
    let relative=witnesses.iter().flatten().filter(|w|rise[w.time]>1e-9).max_by(|x,y|(x.difference/rise[x.time]).total_cmp(&(y.difference/rise[y.time]))).map(witness_json).unwrap_or_else(||absolute.clone());
    Ok(
        json!({"converged":converged,"maximumDifferenceK":max_difference.iter().copied().fold(0.,f64::max),"maximumGateUtilization":max_difference.iter().zip(&rise).map(|(d,r)|d/(tolerance*r+1e-9)).fold(0.,f64::max),"maximumRelativeChange":max_difference.iter().zip(&rise).filter(|(_,r)|**r>1e-9).map(|(d,r)|d/r).fold(0.,f64::max),"maximumFineRiseK":rise.iter().copied().fold(0.,f64::max),"compactMaximumDifferenceK":compact_delta,"compactConverged":compact_pass,"timeSamples":a.axis.len(),"observationAxisReference":"axis","sampledPoints":count,"classifiedVoidNodes":voids,"missingPhysicalOrBoundarySamples":missing,"missingCoverage":missing_records,"traceDistanceMm":trace,"worst":absolute,"absoluteWitness":absolute,"relativeWitness":relative,"boundaryLocalization":{"count":localization_count,"maximumDisplacementMm":maximum_displacement,"sourceIdentityMatches":same_source_identity,"records":localization_records},"absoluteAllowanceK":1e-9,"scope":"Both physical nodal sets, boundary vertices/midpoints and inward traces at every output/event time; sampled convergence, not certified continuous or absolute accuracy"}),
    )
}

fn healing_length(input: &board::BoardInput) -> Result<f64> {
    if input.layers.len() != 3
        || input.layers[0].kind != "copper"
        || input.layers[1].kind != "dielectric"
        || input.layers[2].kind != "copper"
    {
        return Err(bad("Opt-in policy currently supports three copper/core/copper slices; ordinary layered solver remains available"));
    }
    let core = &input.layers[1];
    let mut ell = f64::INFINITY;
    for i in [0, 2] {
        let l = &input.layers[i];
        let k = l
            .background
            .as_ref()
            .ok_or_else(|| bad("Explicit background material required"))?
            .thermal_conductivity_w_per_mk;
        let t = l.thickness_mm * 1e-3;
        let resistance = t / (2. * k)
            + core.thickness_mm * 1e-3 / (2. * core.material.thermal_conductivity_w_per_mk);
        ell = ell.min((k * t * resistance).sqrt() * 1e3);
    }
    if !ell.is_finite() || ell <= 0. {
        return Err(bad("Invalid background healing length"));
    }
    Ok(ell)
}

fn mark(
    field: &Field,
    input: &board::BoardInput,
    r: &Request,
    target: f64,
    start: Instant,
    seconds: f64,
) -> Result<board::NestedMeshPlan> {
    let mut edges = BTreeSet::new();
    let mut marked = BTreeSet::new();
    let mut max_eta = 0f64;
    let area = |ids: [usize; 3]| {
        let [a, b, c] = ids.map(|i| field.points[i]);
        cross(a, b, c).abs() / 2.
    };
    let exposed = field
        .triangles
        .iter()
        .enumerate()
        .map(|(i, t)| area(*t) * field.fractions[1][i] * 2e-6)
        .sum::<f64>();
    let mut signal = vec![0f64; field.axis.len()];
    for li in 0..3 {
        for (i, p) in field.points.iter().enumerate() {
            if !field.domain.contains(&Point::new(p[0], p[1])) {
                continue;
            }
            for (j, &v) in field.values[li][i].iter().enumerate() {
                if v.is_finite() {
                    signal[j] = signal[j]
                        .max((v + (field.origin - r.ambient_c.sample(field.axis[j]))).abs());
                }
            }
        }
    }
    for (ti, &ids) in field.triangles.iter().enumerate() {
        check(start, seconds)?;
        for j in 0..3 {
            let (a, b) = (ids[j], ids[(j + 1) % 3]);
            edges.insert((a.min(b), a.max(b)));
        }
        let physical = field.fractions[1][ti];
        if physical <= 1e-16 {
            continue;
        }
        let q = ids.map(|i| field.points[i]);
        let det = cross(q[0], q[1], q[2]);
        for li in [0, 2] {
            let layer = &field.layers[li];
            let core = &field.layers[1];
            let frac = (field.fractions[li][ti] / physical).clamp(0., 1.);
            let k0 = layer.material.thermal_conductivity_w_per_mk;
            let k1 = layer
                .background
                .as_ref()
                .unwrap()
                .thermal_conductivity_w_per_mk;
            let kt = layer.thickness_mm * 1e-3 * (k0 * frac + k1 * (1. - frac));
            let gv = frac
                / (layer.thickness_mm * 1e-3 / (2. * k0)
                    + core.thickness_mm * 1e-3
                        / (2. * core.material.thermal_conductivity_w_per_mk))
                + (1. - frac)
                    / (layer.thickness_mm * 1e-3 / (2. * k1)
                        + core.thickness_mm * 1e-3
                            / (2. * core.material.thermal_conductivity_w_per_mk));
            for (sample, &time) in field.axis.iter().enumerate() {
                let t = ids.map(|i| field.values[li][i][sample]);
                let c = ids.map(|i| field.values[1][i][sample]);
                if t.iter().chain(c.iter()).any(|v| !v.is_finite()) {
                    continue;
                }
                let gradient = [
                    ((t[1] - t[0]) * (q[2][1] - q[0][1]) - (t[2] - t[0]) * (q[1][1] - q[0][1]))
                        / det,
                    ((t[2] - t[0]) * (q[1][0] - q[0][0]) - (t[1] - t[0]) * (q[2][0] - q[0][0]))
                        / det,
                ];
                let magnitude = gradient[0].hypot(gradient[1]);
                let projection: [f64; 3] = std::array::from_fn(|j| {
                    let a = q[j];
                    let b = q[(j + 1) % 3];
                    if magnitude > 1e-12 {
                        ((b[0] - a[0]) * gradient[0] + (b[1] - a[1]) * gradient[1]).abs()
                            / magnitude
                    } else {
                        (b[0] - a[0]).hypot(b[1] - a[1])
                    }
                });
                let h = projection.iter().copied().fold(0., f64::max);
                let mut density = 0f64;
                for j in 0..3 {
                    let cooling = if let Some(law) = &input.surface_cooling {
                        law.evaluate(t[j] + (field.origin - r.ambient_c.sample(time)), time)?
                            .0
                            / exposed
                    } else {
                        0.
                    };
                    density = density.max((gv * (c[j] - t[j]) - cooling).abs());
                }
                let eta = h * h * 1e-6 * density / (8. * kt);
                max_eta = max_eta.max(eta);
                if eta > target * signal[sample] + 1e-9 {
                    for j in 0..3 {
                        if projection[j] >= h * 0.5 {
                            let (a, b) = (ids[j], ids[(j + 1) % 3]);
                            marked.insert((a.min(b), a.max(b)));
                        }
                    }
                }
            }
        }
    }
    let planned = field
        .points
        .len()
        .checked_add(marked.len())
        .ok_or_else(|| bad("Midpoint node count overflow"))?;
    if planned * field.layers.len() > input.max_nodes {
        return Err(format!(
            "THERMAL_BUDGET: Directional plan requires {} replicated vertices > {}",
            planned * field.layers.len(),
            input.max_nodes
        ));
    }
    let plan = board::NestedMeshPlan {
        points_mm: field.points.clone(),
        edges: edges
            .into_iter()
            .map(|(a, b)| board::NestedEdge {
                a,
                b,
                midpoint: marked.contains(&(a, b)).then(|| {
                    [
                        (field.points[a][0] + field.points[b][0]) / 2.,
                        (field.points[a][1] + field.points[b][1]) / 2.,
                    ]
                }),
            })
            .collect(),
        planned_planar_nodes: planned,
        admitted: true,
        physical_constraints_only: false,
        independent_coverage: false,
        coarse_triangles: Some(field.triangles.clone()),
        points_bits: None,
        metadata: json!({"role":"directional","markedEdges":marked.len(),"maximumIndicatorK":max_eta,"seedRefinement":field.seed_refinement,"criterion":"h_parallel² max|vertical reaction minus surface flux|/(8 kt); heuristic only"}),
    };
    Ok(plan)
}

fn observation_times(r: &Request, start: Instant, seconds: f64) -> Result<Vec<f64>> {
    if r.mode == "steady" {
        return Ok(vec![r.at_s.unwrap_or(0.)]);
    }
    let ts = r
        .time
        .as_ref()
        .ok_or_else(|| bad("Transient refinement requires time"))?;
    if !ts.stop_s.is_finite()
        || ts.stop_s <= 0.
        || !ts.sample_step_s.is_finite()
        || ts.sample_step_s <= 0.
        || ts.sample_step_s > ts.stop_s
    {
        return Err(bad("Invalid transient axis"));
    }
    let count = (ts.stop_s / ts.sample_step_s).ceil() as usize;
    if count > 99999 {
        return Err(bad("Refinement observation axis exceeds100000 samples"));
    }
    let mut axis: Vec<_> = (0..=count)
        .map(|i| (i as f64 * ts.sample_step_s).min(ts.stop_s))
        .collect();
    let signals: Vec<_> = std::iter::once(&r.ambient_c)
        .chain(r.network.sources.iter().map(|s| &s.power_w))
        .chain(
            r.network
                .links
                .iter()
                .filter_map(|l| l.cooling.scale.as_ref()),
        )
        .collect();
    for s in &signals {
        s.validate()?;
    }
    let mut t = 0.;
    loop {
        check(start, seconds)?;
        let event = signals
            .iter()
            .map(|s| s.next(t))
            .fold(f64::INFINITY, f64::min);
        if event > ts.stop_s {
            break;
        }
        if !event.is_finite() || event <= t {
            return Err(bad("Signal event axis did not advance"));
        }
        axis.push(event);
        t = event;
        if axis.len() > 100000 {
            return Err(bad("Output plus mandatory events exceeds100000 samples"));
        }
    }
    axis.sort_by(f64::total_cmp);
    axis.dedup();
    Ok(axis)
}

fn independent_plan(
    field: &Field,
    mut fine_points: Vec<[f64; 2]>,
    input: &board::BoardInput,
    start: Instant,
    seconds: f64,
) -> Result<board::NestedMeshPlan> {
    let cap = input.max_nodes / field.layers.len();
    let independent_global_vertices = fine_points.len();
    let mut points = field.points.clone();
    let mut occupied: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    let key = |p: [f64; 2]| ((p[0] / 1e-9).floor() as i64, (p[1] / 1e-9).floor() as i64);
    for (i, p) in points.iter().enumerate() {
        occupied.entry(key(*p)).or_default().push(i);
    }
    let mut edges = BTreeSet::new();
    for t in &field.triangles {
        for j in 0..3 {
            let (a, b) = (t[j], t[(j + 1) % 3]);
            edges.insert((a.min(b), a.max(b)));
        }
    }
    let edges: Vec<_> = edges.into_iter().collect();
    let tree = RTree::bulk_load(
        edges
            .iter()
            .enumerate()
            .map(|(index, (a, b))| {
                let p = points[*a];
                let q = points[*b];
                Entry {
                    index,
                    bounds: AABB::from_corners(
                        [p[0].min(q[0]) - 1e-12, p[1].min(q[1]) - 1e-12],
                        [p[0].max(q[0]) + 1e-12, p[1].max(q[1]) + 1e-12],
                    ),
                }
            })
            .collect(),
    );
    let mut splits: Vec<Vec<(f64, usize)>> = vec![vec![]; edges.len()];
    let mut added = 0usize;
    let mut split_count = 0usize;
    let mut deviation = 0f64;
    for p in fine_points.drain(..) {
        check(start, seconds)?;
        let (x, y) = key(p);
        let mut redundant = false;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for i in occupied.get(&(x + dx, y + dy)).into_iter().flatten() {
                    if (points[*i][0] - p[0]).hypot(points[*i][1] - p[1]) <= 1e-9 {
                        redundant = true;
                    }
                }
            }
        }
        if redundant {
            continue;
        }
        if points.len() >= cap {
            return Err(format!(
                "THERMAL_BUDGET: Independent nested union exceeds{} replicated vertices",
                input.max_nodes
            ));
        }
        let index = points.len();
        points.push(p);
        occupied.entry((x, y)).or_default().push(index);
        added += 1;
        for entry in tree.locate_in_envelope_intersecting(&AABB::from_point(p)) {
            let (a, b) = edges[entry.index];
            let a = points[a];
            let b = points[b];
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy);
            if t <= 0. || t >= 1. {
                continue;
            }
            let d = (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy);
            if d > 1e-12 {
                continue;
            }
            splits[entry.index].push((t, index));
            split_count += 1;
            deviation = deviation.max(d);
            if edges.len() + split_count > cap * 8 {
                return Err(bad("Independent constraint split budget"));
            }
        }
    }
    let mut constraints = Vec::with_capacity(edges.len() + split_count);
    for ((a, b), mut part) in edges.into_iter().zip(splits) {
        part.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut from = a;
        for (_, to) in part {
            constraints.push(board::NestedEdge {
                a: from,
                b: to,
                midpoint: None,
            });
            from = to;
        }
        constraints.push(board::NestedEdge {
            a: from,
            b,
            midpoint: None,
        });
    }
    let planned = points.len();
    Ok(board::NestedMeshPlan {
        points_mm: points,
        edges: constraints,
        planned_planar_nodes: planned,
        admitted: true,
        physical_constraints_only: false,
        independent_coverage: true,
        coarse_triangles: Some(field.triangles.clone()),
        points_bits: None,
        metadata: json!({"role":"independent-control","seedRefinement":field.seed_refinement,"independentGlobalVertices":independent_global_vertices,"addedGlobalVertices":added,"matchedExistingGlobalVertices":independent_global_vertices-added,"newCollinearConstraintSplits":split_count,"maximumNewSplitDeviationMm":deviation,"preservedCandidateVertices":field.points.len(),"preservedCandidateEdges":tree.size(),"scope":"All candidate edges retained; independent global samples inserted or already represented within1e-9 mm, with explicit collinear splitting"}),
    })
}

pub(super) fn run(mut r: Request, start: Instant) -> Result<Value> {
    let input = r
        .board
        .take()
        .ok_or_else(|| bad("Refinement requires a physical board"))?;
    let policy = input
        .refinement
        .clone()
        .ok_or_else(|| bad("Missing policy"))?;
    policy.validate(&input, &r)?;
    let ell = healing_length(&input)?;
    let seconds = r
        .budget
        .as_ref()
        .and_then(|b| b.max_seconds)
        .unwrap_or(120.);
    let mib = r
        .budget
        .as_ref()
        .and_then(|b| b.max_memory_mi_b)
        .unwrap_or(1024.);
    if !seconds.is_finite() || seconds <= 0. || !mib.is_finite() || mib <= 0. || mib > 4096. {
        return Err(bad("Invalid total refinement budget"));
    }
    let budget = (mib * 1048576.) as usize;
    let compact_network = r.network.clone();
    let compact_bytes = compact_network.nodes.len().saturating_mul(640)
        + compact_network.links.len().saturating_mul(384)
        + compact_network.sources.len().saturating_mul(160)
        + compact_network
            .sources
            .iter()
            .map(|s| s.power_w.heap_bytes())
            .sum::<usize>()
        + compact_network
            .links
            .iter()
            .map(|l| l.cooling.heap_bytes())
            .sum::<usize>();
    let authored_bytes = compact_bytes
        + 3 * (input.board_json.capacity()
            + input
                .heat
                .iter()
                .map(|h| h.power_w.heap_bytes() + json_heap_bytes(&h.target))
                .sum::<usize>()
            + input
                .contacts
                .iter()
                .map(|c| json_heap_bytes(&c.target))
                .sum::<usize>()
            + input
                .surface_cooling
                .as_ref()
                .map_or(0, Cooling::heap_bytes))
        + 65536;

    let mut previous: Option<Field> = None;
    let mut plan = None;
    let mut independent = false;
    let mut history = vec![];
    for pass in 0..=policy.max_passes + 1 {
        profile::stage(pass);
        check(start, seconds)?;
        let pass_start = Instant::now();
        let retained = previous.as_ref().map_or(0, Field::bytes);
        let preparation_plan = retained
            + authored_bytes
            + input.board_json.len() * 8
            + input.max_nodes * 1024
            + board::spatial_link_limit(input.max_nodes)
                .min(input.max_nodes.saturating_mul(8) + compact_network.links.len())
                * 512;
        admit(preparation_plan, budget)?;
        r.network = compact_network.clone();
        r.spatial = None;
        let mut solve_input = input.clone();
        if independent {
            solve_input.max_edge_mm = policy.validation_max_edge_mm;
        }
        let mesh_phase=profile::timer(profile::Phase::Mesh);
        r.spatial = Some(board::prepare_with_mesh(
            &solve_input,
            &mut r.network,
            r.mode == "time",
            start,
            r.budget.as_ref(),
            plan.take(),
            false,
            Some(ell),
        )?);
        r.capture_coordinates = true;
        drop(mesh_phase);
        r.observation_times = Some(observation_times(&r, start, seconds)?);
        let samples = r.observation_times.as_ref().unwrap().len();
        r.retained_bytes = retained
            .saturating_add(
                r.network
                    .nodes
                    .len()
                    .saturating_mul(samples)
                    .saturating_mul(8),
            )
            .saturating_add(authored_bytes);
        let mut report = super::solve::execute(&r, start)?;
        let solved_nodes = r.network.nodes.len();
        r.spatial = None;
        r.network = compact_network.clone();
        check(start, seconds)?;
        // Factors are now dropped. Admit simultaneous report/field extraction
        // and comparison separately from the preceding numeric-factor peak.
        let values = solved_nodes.saturating_mul(samples);
        let comparison_plan = retained
            + authored_bytes
            + values.saturating_mul(64)
            + input.max_nodes * 384
            + budget / 4
            + samples.saturating_mul(128) + 262144;
        admit(comparison_plan, budget)?;
        let field = Field::from_report(&report)?;
        check(start, seconds)?;
        let comparison = previous
            .as_ref()
            .map(|old| compare(old, &field, &r, policy.relative_tolerance, start, seconds))
            .transpose()?;
        let candidate_pass = comparison.as_ref().is_some_and(|c| c["converged"] == true);
        history.push(json!({"pass":pass,"role":if independent{"independent-control"}else if pass==0{"seed"}else{"directional"},"elapsedMs":pass_start.elapsed().as_secs_f64()*1000.,"summary":field.summary,"comparison":comparison,"retainedPreviousFieldMeshBytes":retained,"retainedAuthoredInputBytes":authored_bytes,"preparationPlannedPeakBytes":preparation_plan,"comparisonPlannedPeakBytes":comparison_plan,"cumulativeElapsedMs":start.elapsed().as_secs_f64()*1000.}));
        let done = independent || pass == policy.max_passes && !candidate_pass;
        if done {
            check(start, seconds)?;
            let maximum_peak = history
                .iter()
                .map(|h| {
                    h["summary"]["memoryPlan"]["totalPlannedPeakBytes"]
                        .as_u64()
                        .unwrap_or(0)
                        .max(h["preparationPlannedPeakBytes"].as_u64().unwrap_or(0))
                        .max(h["comparisonPlannedPeakBytes"].as_u64().unwrap_or(0))
                })
                .max()
                .unwrap_or(0);
            let total_linear_iterations = history
                .iter()
                .map(|h| {
                    h["summary"]["counters"]["linearIterations"]
                        .as_u64()
                        .unwrap_or(0)
                })
                .sum::<u64>();
            report["refinement"] = json!({"policy":policy,"converged":independent&&candidate_pass,"finalRole":if independent{"independent-control"}else{"unconverged-candidate"},"reason":if independent&&candidate_pass{"independent nested sampled field gate passed"}else if independent{"independent nested sampled field gate failed"}else{"maxPasses exhausted before candidate convergence"},"seedHealingLengthMm":ell,"history":history,"totalElapsedMs":start.elapsed().as_secs_f64()*1000.,"totalPhysicalAnalyses":pass+1,"maximumPlannedPeakBytes":maximum_peak,"totalLinearIterations":total_linear_iterations,"scope":"One overall deadline/memory/output budget; only final complete field published; indicator is heuristic and sampled refinement is not absolute accuracy"});
            check(start, seconds)?;
            return Ok(report);
        }
        // Only the minimal field/mesh survives into the next solve. Neither an
        // old factor nor an intermediate public result is retained.
        drop(previous.take());
        drop(report);
        r.spatial = None;
        r.network = compact_network.clone();
        let _mesh_planning_phase=profile::timer(profile::Phase::Mesh);
        if candidate_pass {
            independent = true;
            let mut finer = input.clone();
            finer.max_edge_mm = policy.validation_max_edge_mm;
            let geometry = board::prepare_with_mesh(
                &finer,
                &mut r.network,
                r.mode == "time",
                start,
                r.budget.as_ref(),
                None,
                true,
                Some(ell),
            )?;
            let points = array(
                &geometry.report["mesh"]["pointsMm"],
                "independent global points",
            )?;
            drop(geometry);
            plan = Some(independent_plan(&field, points, &input, start, seconds)?);
        } else {
            plan = Some(mark(
                &field,
                &input,
                &r,
                policy.relative_tolerance,
                start,
                seconds,
            )?);
        }
        previous = Some(field);
    }
    Err(bad("Bounded refinement control invariant"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(origin: f64) -> Field {
        let material = board::Material {
            thermal_conductivity_w_per_mk: 1.,
            density_kg_m3: Some(1000.),
            specific_heat_j_per_kg_k: Some(1000.),
            source: "analytic".into(),
            state: "constant".into(),
        };
        let layers = (0..3)
            .map(|i| board::Layer {
                kind: if i == 1 { "dielectric" } else { "copper" }.into(),
                name: format!("layer{i}"),
                thickness_mm: 0.1,
                material: material.clone(),
                background: Some(material.clone()),
            })
            .collect();
        Field {
            points: vec![[0., 0.], [1., 0.], [0., 1.]],
            triangles: vec![[0, 1, 2]],
            fractions: vec![vec![1.]; 3],
            layers,
            domain: MultiPolygon(vec![Polygon::new(
                LineString::from(vec![(0., 0.), (1., 0.), (0., 1.), (0., 0.)]),
                vec![],
            )]),
            boundary_source: None,
            axis: vec![0., 1.],
            origin,
            values: vec![vec![vec![1., 2.]; 3]; 3],
            compact: BTreeMap::new(),
            trace_mm: 1e-7,
            summary: json!(null),
            seed_refinement: json!([]),
        }
    }
    fn request(shift: f64) -> Request {
        serde_json::from_value(json!({"contextId":"comparison","network":{"nodes":[],"links":[],"sources":[]},"mode":"time","ambientC":{"kind":"step","from":25.+shift,"to":30.+shift,"atS":0.5,"riseS":0.}})).unwrap()
    }
    #[test]
    fn adaptive_refinement_comparison_uses_ambient_rise_and_common_coordinates() {
        for shift in [0., 1_000_000.] {
            let a = field(25. + shift);
            let mut b = field(25. + shift);
            for layer in &mut b.values {
                for row in layer {
                    row[0] += 0.01;
                    row[1] += 0.02;
                }
            }
            let comparison = compare(&a, &b, &request(shift), 0.02, Instant::now(), 10.).unwrap();
            assert_eq!(comparison["converged"], true);
            assert_eq!(comparison["absoluteWitness"]["timeS"],1.);
            assert_eq!(comparison["relativeWitness"]["timeS"],0.);
            assert!((comparison["absoluteWitness"]["coarseTemperatureC"].as_f64().unwrap()-(27.+shift)).abs()<1e-9);
            assert!((comparison["relativeWitness"]["denominatorK"].as_f64().unwrap()-1.01).abs()<1e-9);
            assert!(
                (comparison["maximumRelativeChange"].as_f64().unwrap() - 0.01 / 1.01).abs() < 1e-12
            );
            assert_eq!(
                compare(&a, &b, &request(shift), 0.001, Instant::now(), 10.).unwrap()["converged"],
                false
            );
        }
        let mut a = field(25.);
        for l in &mut a.values {
            for row in l {
                row.fill(0.);
            }
        }
        let mut r = request(0.);
        r.ambient_c = Signal::Constant(25.);
        let comparison = compare(&a, &a, &r, 0.02, Instant::now(), 10.).unwrap();
        assert_eq!(comparison["converged"], true);
        assert_eq!(comparison["maximumFineRiseK"], 0.);
        let mut missing = field(25.);
        missing.values[1][0][0] = f64::NAN;
        assert!(compare(&a, &missing, &r, 0.02, Instant::now(), 10.).is_err());
    }

    #[test]
    fn native_boundary_frozen_correspondence_and_negatives() {
        let fixture:Value=serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/../../tests/fixtures/thermal-boundary-correspondence.json"))).unwrap();
        let queries:Vec<[f64;2]>=serde_json::from_value(fixture["queries"].clone()).unwrap();
        for (scale,offset) in [(1.,[0.,0.]),(2.,[1000.,-1000.])] {
            let transform=|p:[f64;2]|[p[0]*scale+offset[0],p[1]*scale+offset[1]];
            let source:Vec<[f64;2]>=serde_json::from_value(fixture["sourceOuter"].clone()).unwrap();
            let domain:Vec<[f64;2]>=serde_json::from_value(fixture["domainOuter"].clone()).unwrap();
            let source:Vec<_>=source.into_iter().map(transform).collect();let domain:Vec<_>=domain.into_iter().map(transform).collect();
            let mut f=field(25.);f.points=serde_json::from_value::<Vec<[f64;2]>>(fixture["points"].clone()).unwrap().into_iter().map(transform).collect();
            f.triangles=serde_json::from_value(fixture["triangles"].clone()).unwrap();f.values=vec![vec![vec![1.,2.];f.points.len()];3];
            f.domain=MultiPolygon(vec![polygon(&DomainPolygon{outer:domain,holes:vec![]})]);
            let report=json!({"spatial":{"boardIdentity":fixture["identity"],"geometry":{"outline":{"outer":source},"booleanErrorBoundMm":fixture["booleanErrorBoundMm"].as_f64().unwrap()*scale}}});
            f.boundary_source=super::super::boundary::Source::from_report(&report);
            let locate=super::super::boundary::Localizer::new(&f.domain,f.boundary_source.as_ref(),true);let at=Interpolator::new(&f);
            let classification:Vec<_>=queries.iter().map(|p|{let p=transform(*p);let proof=locate.at(p).unwrap();assert!(proof.displacement<=proof.report["limitMm"].as_f64().unwrap());let strict=at.at(p).is_some();let localized=at.at(proof.point).is_some();assert_eq!(strict,fixture["expectedStrictAvailable"].as_bool().unwrap());assert_eq!(localized,fixture["expectedLocalizedAvailable"].as_bool().unwrap());json!({"pointMm":p,"strictAvailable":strict,"localizedAvailable":localized})}).collect();
            assert_eq!(classification.len(),24);if scale==1.{println!("THERMAL_FROZEN_BOUNDARY {}",json!({"classification":classification}));}
            assert!(locate.at(transform([0.,0.])).is_none(),"Interior/cutout points cannot map to an outer ring");
            let wrong=super::super::boundary::Localizer::new(&f.domain,f.boundary_source.as_ref(),false);assert!(queries.iter().all(|p|wrong.at(transform(*p)).is_none()),"Wrong source identity cannot supply correspondence");
            let mut wrong_field=f.clone();wrong_field.boundary_source.as_mut().unwrap().identity="different-source-board".into();let wrong_compare=compare(&f,&wrong_field,&request(0.),0.02,Instant::now(),10.).unwrap();assert_eq!(wrong_compare["boundaryLocalization"]["sourceIdentityMatches"],false);assert_eq!(wrong_compare["boundaryLocalization"]["count"],0);
            let mut cutout=f.domain.clone();let hole=vec![transform([-0.4,-0.4]),transform([0.4,-0.4]),transform([0.4,0.4]),transform([-0.4,0.4]),transform([-0.4,-0.4])];cutout.0[0]=Polygon::new(cutout.0[0].exterior().clone(),vec![LineString::from(hole.into_iter().map(|p|(p[0],p[1])).collect::<Vec<_>>())]);let holes=super::super::boundary::Localizer::new(&cutout,f.boundary_source.as_ref(),true);assert!(holes.at(transform([0.4,0.])).is_none(),"An actual hole boundary is never mapped to the outer outline");
            let mut over=report.clone();over["spatial"]["geometry"]["booleanErrorBoundMm"]=json!(1e-13*scale);let over=super::super::boundary::Source::from_report(&over);let over=super::super::boundary::Localizer::new(&f.domain,over.as_ref(),true);assert!(queries.iter().all(|p|over.at(transform(*p)).is_none()),"Beyond representation bound must stay missing");
            let query=transform(queries[0]);let proof=locate.at(query).unwrap();let interior=[(query[0]+proof.point[0])/2.,(query[1]+proof.point[1])/2.];assert!(locate.at(interior).is_none(),"Off-boundary point must not be localized");
            drop(at);drop(locate);f.triangles.clear();let gap=Interpolator::new(&f);assert!(gap.at(proof.point).is_none(),"Removed physical cell remains missing even at proven source point");
        }
    }
}
