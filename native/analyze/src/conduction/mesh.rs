use super::*;
use geo::{
    Area, BooleanOps, BoundingRect, Contains, Intersects, LineString, Point, Polygon,
    PreparedGeometry, Relate, Validation,
};
use rstar::{RTree, RTreeObject, AABB};
use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};
use std::collections::{BTreeMap, BTreeSet};
type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;
#[derive(Clone)]
struct Segment {
    a: [f64; 2],
    b: [f64; 2],
    kind: String,
    paths: Vec<String>,
}
impl RTreeObject for Segment {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.a[0].min(self.b[0]), self.a[1].min(self.b[1])],
            [self.a[0].max(self.b[0]), self.a[1].max(self.b[1])],
        )
    }
}
fn segments(p: &Polygon<f64>, kind: &str, paths: &[String]) -> Vec<Segment> {
    std::iter::once(p.exterior())
        .chain(p.interiors())
        .enumerate()
        .flat_map(|(i, r)| {
            r.lines().map(move |l| Segment {
                a: [l.start.x, l.start.y],
                b: [l.end.x, l.end.y],
                kind: if kind == "outer" && i > 0 {
                    "hole".into()
                } else {
                    kind.into()
                },
                paths: paths.to_vec(),
            })
        })
        .collect()
}
fn on(p: [f64; 2], s: &Segment, tol: f64) -> bool {
    let dx = s.b[0] - s.a[0];
    let dy = s.b[1] - s.a[1];
    let len = dx.hypot(dy);
    if len == 0. {
        return false;
    }
    let t = ((p[0] - s.a[0]) * dx + (p[1] - s.a[1]) * dy) / (len * len);
    t >= -tol / len
        && t <= 1. + tol / len
        && ((p[0] - s.a[0]) * dy - (p[1] - s.a[1]) * dx).abs() <= tol * len
}
fn distance(p: [f64; 2], s: &Segment) -> f64 {
    let dx = s.b[0] - s.a[0];
    let dy = s.b[1] - s.a[1];
    let t = (((p[0] - s.a[0]) * dx + (p[1] - s.a[1]) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
    (p[0] - s.a[0] - t * dx).hypot(p[1] - s.a[1] - t * dy)
}
fn min_angle(p: [[f64; 2]; 3]) -> f64 {
    (0..3)
        .map(|i| {
            let a = p[(i + 1) % 3];
            let b = p[i];
            let c = p[(i + 2) % 3];
            let u = [a[0] - b[0], a[1] - b[1]];
            let v = [c[0] - b[0], c[1] - b[1]];
            ((u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1])))
                .clamp(-1., 1.)
                .acos()
                .to_degrees()
        })
        .fold(180., f64::min)
}
fn err(d: &mut Domain, r: &mut ConductionReport, code: &str, msg: &str) {
    d.readiness.mesh_complete = false;
    d.readiness.reasons.push(code.into());
    issue(r, code, &d.id, None, msg, "limitation");
}
pub(super) fn build_domain(
    g: &PreparationReport,
    layer: &str,
    snapshot: &PolygonSnapshot,
    r: &mut ConductionReport,
) {
    let poly = polygon(snapshot);
    let bounds = poly.bounding_rect().unwrap();
    let candidates: Vec<_> = g
        .regions
        .iter()
        .filter(|v| v.layer == layer)
        .filter_map(|v| {
            let p = polygon(&v.polygon);
            p.bounding_rect()
                .filter(|b| b.intersects(&bounds))
                .map(|_| (v, p))
        })
        .collect();
    let regions: Vec<_> = candidates
        .iter()
        .filter(|(_, p)| p.intersection(&poly).unsigned_area() > 0.)
        .map(|(v, _)| *v)
        .collect();
    let paths: Vec<_> = regions
        .iter()
        .map(|v| v.source.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let ids: Vec<_> = regions.iter().map(|v| v.id.clone()).collect();
    let mut reasons = Vec::<String>::new();
    let uncertain =
        g.contacts.iter().any(|c| {
            !c.reliable && c.kind != "point" && (ids.contains(&c.a) || ids.contains(&c.b))
        }) || g.diagnostics.iter().any(|d| {
            paths.contains(&d.path)
                && (d.code.contains("topology_uncertain")
                    || d.code.contains("topology_unresolved")
                    || d.code == "barrel_contact_uncertain")
        });
    if uncertain {
        reasons.push("topology_uncertain".into());
    }
    let copper_complete = g.excluded.is_empty()
        && !g
            .diagnostics
            .iter()
            .any(|d| d.code == "independent_drills_missing" || d.code == "drill_span_unconfirmed")
        && !g.fills.iter().any(|f| {
            f.layer == layer
                && (f.status != "available" || f.freshness.as_deref() != Some("current"))
        });
    if !copper_complete {
        reasons.push("incomplete_copper_or_fill_freshness".into());
    }
    let sheet = r
        .profile
        .layers
        .iter()
        .find(|l| l.layer == layer)
        .and_then(|l| l.sheet_conductance_s);
    if sheet.is_none() {
        reasons.push("missing_layer_physics".into());
    }
    let mut d = Domain {
        id: format!("domain-{:06}", r.domains.len()),
        layer: layer.into(),
        region_ids: ids,
        source_paths: paths,
        readiness: Readiness {
            mesh_complete: true,
            topology_confirmed: !uncertain,
            copper_complete,
            physics_complete: sheet.is_some(),
            dc_ready: false,
            reasons,
        },
        nodes: vec![],
        triangles: vec![],
        boundaries: vec![],
        terminals: vec![],
        area_mm2: poly.unsigned_area(),
        mesh_area_mm2: 0.,
        min_angle_deg: 180.,
        max_area_mm2: 0.,
        refinement_steps: 0,
        terminal_alignment_mm: 0.,
        sheet_conductance_s: sheet,
    };
    match build(&poly, &regions, g, r, &mut d) {
        Ok(()) => {}
        Err(message) => {
            d.nodes.clear();
            d.triangles.clear();
            d.boundaries.clear();
            d.terminals.clear();
            d.mesh_area_mm2 = 0.;
            d.max_area_mm2 = 0.;
            d.min_angle_deg = 0.;
            err(&mut d, r, "mesh_unbuilt", &message);
        }
    }
    d.readiness.dc_ready = d.readiness.mesh_complete
        && d.readiness.topology_confirmed
        && d.readiness.copper_complete
        && d.readiness.physics_complete;
    r.counts.nodes += d.nodes.len();
    r.counts.triangles += d.triangles.len();
    r.domains.push(d);
}
fn build(
    poly: &Polygon<f64>,
    regions: &[&geometry::Region],
    g: &PreparationReport,
    r: &mut ConductionReport,
    d: &mut Domain,
) -> Result<(), String> {
    poly.check_validation()
        .map_err(|e| format!("Invalid union boundary: {e}"))?;
    let opt = r.options.mesh;
    let max_nodes = opt
        .max_nodes
        .saturating_sub(r.counts.nodes)
        .min((opt.max_triangles.saturating_sub(r.counts.triangles) + 5) / 2);
    // A domain contributed exclusively by one pad already IS that terminal.
    // Re-clipping and reinserting its coincident contour creates artificial
    // near-coincident PSLG edges from boolean rounding. Reuse the domain and
    // provenance directly, without a second geometric representation.
    let full_pad = regions.first().filter(|first| {
        first.source.kind == "pad" && regions.iter().all(|v| v.source.path == first.source.path)
    });
    let full_paths: Vec<_> = full_pad
        .map(|v| vec![v.source.path.clone()])
        .unwrap_or_default();
    let mut constraints = segments(poly, "outer", &full_paths);
    // Holes retain drill provenance even when several source cuts overlap.
    let tol = (poly
        .bounding_rect()
        .unwrap()
        .max()
        .x
        .abs()
        .max(poly.bounding_rect().unwrap().max().y.abs())
        + 1.)
        * 1e-12;
    let hole_segments: Vec<_> = g
        .holes
        .iter()
        .filter(|h| h.layers.contains(&d.layer))
        .flat_map(|h| {
            h.polygons
                .iter()
                .flat_map(|p| segments(&polygon(p), "hole", std::slice::from_ref(&h.source.path)))
        })
        .collect();
    let holes = RTree::bulk_load(hole_segments);
    let source_edges = RTree::bulk_load(
        regions
            .iter()
            .flat_map(|v| {
                segments(
                    &polygon(&v.polygon),
                    "source",
                    std::slice::from_ref(&v.source.path),
                )
            })
            .collect(),
    );
    for s in &mut constraints {
        {
            let mid = [(s.a[0] + s.b[0]) / 2., (s.a[1] + s.b[1]) / 2.];
            let e = g.boolean_error_bound_mm * 4. + tol;
            let envelope = AABB::from_corners([mid[0] - e, mid[1] - e], [mid[0] + e, mid[1] + e]);
            for edge in source_edges.locate_in_envelope_intersecting(&envelope) {
                if distance(mid, edge) <= e {
                    s.paths.extend(edge.paths.clone());
                }
            }
            for h in holes.locate_in_envelope_intersecting(&envelope) {
                if distance(mid, h) <= e {
                    s.paths.extend(h.paths.clone());
                    s.kind = "hole".into();
                }
            }
            s.paths.sort();
            s.paths.dedup();
            if s.paths.is_empty() {
                s.paths.push("outline/cutouts".into());
            }
        }
    }
    // Pad fragments are finite-area terminals, retaining their holes and all parts.
    let terminals: Vec<_> = regions
        .iter()
        .filter(|v| v.source.kind == "pad")
        .map(|v| {
            (
                *v,
                if full_pad.is_some() {
                    geo::MultiPolygon(vec![poly.clone()])
                } else {
                    polygon(&v.polygon).intersection(poly)
                },
            )
        })
        .collect();
    let domain_segments = RTree::bulk_load(constraints.clone());
    let align = g.boolean_error_bound_mm * 4. + tol * 16.;
    for (v, parts) in &terminals {
        if full_pad.is_some() {
            continue;
        }
        for p in &parts.0 {
            let mut pad_edges = segments(p, "terminal", std::slice::from_ref(&v.source.path));
            // A globally empty drill/cutout is already a domain constraint.
            // Reuse coincident hole rings instead of inserting a second almost
            // identical PSLG ring. Authored pad voids filled by other copper
            // remain internal terminal boundaries and are still constrained.
            let mut reused = Vec::<Segment>::new();
            for ring in p.interiors() {
                let ring_edges: Vec<_> = ring
                    .lines()
                    .map(|l| Segment {
                        a: [l.start.x, l.start.y],
                        b: [l.end.x, l.end.y],
                        kind: "terminal".into(),
                        paths: vec![v.source.path.clone()],
                    })
                    .collect();
                let mut max_distance = 0f64;
                let coincident = ring_edges.iter().all(|s| {
                    [s.a, s.b, [(s.a[0] + s.b[0]) / 2., (s.a[1] + s.b[1]) / 2.]]
                        .iter()
                        .all(|point| {
                            let area = AABB::from_corners(
                                [point[0] - align, point[1] - align],
                                [point[0] + align, point[1] + align],
                            );
                            let distance = domain_segments
                                .locate_in_envelope_intersecting(&area)
                                .map(|e| distance(*point, e))
                                .fold(f64::INFINITY, f64::min);
                            max_distance = max_distance.max(distance);
                            distance <= align
                        })
                });
                if coincident {
                    d.terminal_alignment_mm = d.terminal_alignment_mm.max(max_distance);
                    reused.extend(ring_edges);
                }
            }
            pad_edges.retain(|s| !reused.iter().any(|e| e.a == s.a && e.b == s.b));
            // Boolean source/union snapshots have independently bounded grid
            // roundoff. Align only terminal constraints to the fixed copper
            // boundary, within that declared bound; never move copper itself.
            for edge in &mut pad_edges {
                for point in [&mut edge.a, &mut edge.b] {
                    let area = AABB::from_corners(
                        [point[0] - align, point[1] - align],
                        [point[0] + align, point[1] + align],
                    );
                    let nearby: Vec<_> = domain_segments
                        .locate_in_envelope_intersecting(&area)
                        .collect();
                    let original = *point;
                    let vertex = nearby
                        .iter()
                        .flat_map(|s| [s.a, s.b])
                        .filter_map(|p| {
                            let dist = (p[0] - point[0]).hypot(p[1] - point[1]);
                            (dist <= align).then_some((dist, p))
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0));
                    if let Some((_, p)) = vertex {
                        *point = p;
                    } else if let Some(s) = nearby
                        .iter()
                        .filter(|s| distance(*point, s) <= align)
                        .min_by(|a, b| distance(*point, a).total_cmp(&distance(*point, b)))
                    {
                        let dx = s.b[0] - s.a[0];
                        let dy = s.b[1] - s.a[1];
                        let t = (((point[0] - s.a[0]) * dx + (point[1] - s.a[1]) * dy)
                            / (dx * dx + dy * dy))
                            .clamp(0., 1.);
                        *point = [s.a[0] + t * dx, s.a[1] + t * dy];
                    }
                    d.terminal_alignment_mm = d
                        .terminal_alignment_mm
                        .max((original[0] - point[0]).hypot(original[1] - point[1]));
                }
            }
            constraints.extend(pad_edges.into_iter().filter(|s| s.a != s.b));
        }
    }
    if constraints.len().saturating_mul(2) > max_nodes || max_nodes < 3 {
        return Err("Input constraints exceed remaining mesh node/triangle budget".into());
    }
    constraints.sort_by(|a, b| {
        a.a[0]
            .total_cmp(&b.a[0])
            .then(a.a[1].total_cmp(&b.a[1]))
            .then(a.b[0].total_cmp(&b.b[0]))
            .then(a.b[1].total_cmp(&b.b[1]))
            .then(a.kind.cmp(&b.kind))
    });
    let mut cdt = Cdt::new();
    for s in &constraints {
        // A new segment can intersect at most every existing edge once. Reserve
        // this worst case before Spade constructs its split-point vector.
        if cdt.num_vertices() + cdt.num_undirected_edges() + 2 > max_nodes {
            return Err("Constraint intersection budget exhausted before insertion".into());
        }
        let length = (s.b[0] - s.a[0]).hypot(s.b[1] - s.a[1]);
        let pieces = (length / (2. * opt.max_area_mm2).sqrt()).ceil().max(1.) as usize;
        if pieces > max_nodes.saturating_sub(cdt.num_vertices()) {
            return Err("Boundary subdivision exceeds node budget".into());
        }
        let mut a = cdt
            .insert(Point2::new(s.a[0], s.a[1]))
            .map_err(|e| e.to_string())?;
        for i in 1..=pieces {
            if cdt.num_vertices() + cdt.num_undirected_edges() + 1 > max_nodes {
                return Err("Constraint splitting exceeds node budget".into());
            }
            let t = i as f64 / pieces as f64;
            let p = if i == pieces {
                s.b
            } else {
                [
                    s.a[0] + t * (s.b[0] - s.a[0]),
                    s.a[1] + t * (s.b[1] - s.a[1]),
                ]
            };
            let b = cdt
                .insert(Point2::new(p[0], p[1]))
                .map_err(|e| e.to_string())?;
            if a != b {
                cdt.add_constraint_and_split(a, b, |p| p);
            }
            a = b;
        }
    }
    let mut refinement_complete = false;
    for _ in 0..opt.max_refinement_steps {
        let remaining = max_nodes.saturating_sub(cdt.num_vertices());
        if remaining == 0 {
            break;
        }
        let result = cdt.refine(
            RefinementParameters::new()
                .keep_constraint_edges()
                .with_max_allowed_area(opt.max_area_mm2)
                .with_angle_limit(AngleLimit::from_deg(opt.min_angle_deg))
                .with_max_additional_vertices(remaining.min(4096)),
        );
        d.refinement_steps += 1;
        if result.refinement_complete {
            refinement_complete = true;
            break;
        }
    }
    let prep = PreparedGeometry::from(poly);
    let mut all_triangles = vec![];
    let mut used = BTreeSet::new();
    for face in cdt.inner_faces() {
        let vertices = face.vertices();
        let mut ids = vertices.map(|v| v.fix().index());
        let mut p = vertices.map(|v| [v.position().x, v.position().y]);
        let centre = Point::new(
            (p[0][0] + p[1][0] + p[2][0]) / 3.,
            (p[0][1] + p[1][1] + p[2][1]) / 3.,
        );
        if !poly.contains(&centre) {
            continue;
        }
        let signed =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if !signed.is_finite() || signed == 0. {
            return Err("Nonfinite or zero-area triangle".into());
        }
        if signed < 0. {
            ids.swap(1, 2);
            p.swap(1, 2);
        }
        let triangle = Polygon::new(LineString::from(vec![p[0], p[1], p[2], p[0]]), vec![]);
        // Full polygon coverage, not only centroid containment. Tiny boundary
        // roundoff is bounded by coordinate-scale arithmetic, never curve tolerance.
        let perimeter = (0..3)
            .map(|i| (p[i][0] - p[(i + 1) % 3][0]).hypot(p[i][1] - p[(i + 1) % 3][1]))
            .sum::<f64>();
        // The fallback boolean has its own bounded integer adapter. Its tiny
        // difference slivers must be measured against that adapter's bound,
        // not against machine epsilon alone.
        let coverage_allowance = (g.boolean_error_bound_mm * 8. + tol * 16.) * perimeter;
        if !prep.relate(&triangle).is_covers()
            && triangle.difference(poly).unsigned_area() > coverage_allowance
        {
            return Err("Triangle crosses a hole or copper boundary".into());
        }
        d.mesh_area_mm2 += signed.abs() / 2.;
        d.max_area_mm2 = d.max_area_mm2.max(signed.abs() / 2.);
        d.min_angle_deg = d.min_angle_deg.min(min_angle(p));
        used.extend(ids);
        all_triangles.push(ids);
        if all_triangles.len() > opt.max_triangles.saturating_sub(r.counts.triangles) {
            return Err("Triangle budget exhausted".into());
        }
    }
    let mut remap = BTreeMap::new();
    for index in used {
        let p = cdt
            .vertex(spade::handles::FixedVertexHandle::from_index(index))
            .position();
        remap.insert(index, d.nodes.len());
        d.nodes.push([p.x, p.y]);
    }
    d.triangles = all_triangles.iter().map(|t| t.map(|i| remap[&i])).collect();
    if d.triangles.is_empty() {
        return Err("No copper triangles produced".into());
    }
    let mut incidence = BTreeMap::<[usize; 2], usize>::new();
    for t in &d.triangles {
        for i in 0..3 {
            let mut edge = [t[i], t[(i + 1) % 3]];
            edge.sort();
            *incidence.entry(edge).or_default() += 1;
        }
    }
    let tree = RTree::bulk_load(constraints.clone());
    let mut coverage = vec![0.; constraints.len()];
    // Trace each post-refinement subedge to all original boundary/terminal segments.
    for (nodes, count) in &incidence {
        if *count > 2 {
            return Err("Nonmanifold triangle edge".into());
        }
        let a = d.nodes[nodes[0]];
        let b = d.nodes[nodes[1]];
        let e = AABB::from_corners(
            [a[0].min(b[0]) - tol, a[1].min(b[1]) - tol],
            [a[0].max(b[0]) + tol, a[1].max(b[1]) + tol],
        );
        let matches: Vec<_> = tree
            .locate_in_envelope_intersecting(&e)
            .filter(|s| on(a, s, tol * 16.) && on(b, s, tol * 16.))
            .collect();
        if *count == 1
            && !matches
                .iter()
                .any(|s| s.kind == "outer" || s.kind == "hole")
        {
            return Err("Open mesh edge has no copper boundary provenance".into());
        }
        if !matches.is_empty() {
            let kind = if matches.iter().any(|s| s.kind == "hole") {
                "hole"
            } else if *count == 1 {
                "outer"
            } else {
                "terminal"
            };
            let paths = matches
                .iter()
                .flat_map(|s| s.paths.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            d.boundaries.push(Boundary {
                nodes: *nodes,
                kind: kind.into(),
                source_paths: paths,
            });
        }
    }
    // A single union polygon must have one edge-connected FEM component.
    // Merely touching at an isolated vertex never makes a conducting bridge.
    let mut owner = BTreeMap::<[usize; 2], usize>::new();
    let mut neighbours = vec![vec![]; d.triangles.len()];
    for (i, t) in d.triangles.iter().enumerate() {
        for j in 0..3 {
            let mut e = [t[j], t[(j + 1) % 3]];
            e.sort();
            if let Some(k) = owner.insert(e, i) {
                neighbours[i].push(k);
                neighbours[k].push(i);
            }
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending = vec![0];
    while let Some(i) = pending.pop() {
        if reached.insert(i) {
            pending.extend(neighbours[i].iter().copied());
        }
    }
    if reached.len() != d.triangles.len() {
        return Err(
            "Copper domain has disconnected triangle components (no finite-width connection)"
                .into(),
        );
    }
    // Coverage of every original segment by constrained mesh subedges. RTree
    // lookup avoids a quadratic scan on curved fill/terminal boundaries.
    #[derive(Clone)]
    struct Edge {
        s: Segment,
    }
    impl RTreeObject for Edge {
        type Envelope = AABB<[f64; 2]>;
        fn envelope(&self) -> Self::Envelope {
            self.s.envelope()
        }
    }
    let edges = RTree::bulk_load(
        d.boundaries
            .iter()
            .map(|b| Edge {
                s: Segment {
                    a: d.nodes[b.nodes[0]],
                    b: d.nodes[b.nodes[1]],
                    kind: b.kind.clone(),
                    paths: vec![],
                },
            })
            .collect(),
    );
    for (i, s) in constraints.iter().enumerate() {
        let e = AABB::from_corners(
            [s.a[0].min(s.b[0]) - tol, s.a[1].min(s.b[1]) - tol],
            [s.a[0].max(s.b[0]) + tol, s.a[1].max(s.b[1]) + tol],
        );
        let length = (s.a[0] - s.b[0]).hypot(s.a[1] - s.b[1]);
        let mut intervals = vec![];
        for edge in edges.locate_in_envelope_intersecting(&e) {
            if on(edge.s.a, s, tol * 16.) && on(edge.s.b, s, tol * 16.) {
                let project = |p: [f64; 2]| {
                    ((p[0] - s.a[0]) * (s.b[0] - s.a[0]) + (p[1] - s.a[1]) * (s.b[1] - s.a[1]))
                        / length
                };
                let a = project(edge.s.a);
                let b = project(edge.s.b);
                intervals.push((a.min(b).max(0.), a.max(b).min(length)));
            }
        }
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = 0.;
        for (a, b) in intervals {
            coverage[i] += (b - a.max(end)).max(0.);
            end = end.max(b);
        }
        if (coverage[i] - length).abs() > tol * 64. * (1. + length) {
            return Err(format!(
                "Boundary/terminal constraint lost after refinement: {} ({}, {}), sources={:?}",
                s.kind, coverage[i], length, s.paths
            ));
        }
    }
    if (d.mesh_area_mm2 - d.area_mm2).abs() > tol * 64. * (1. + d.area_mm2) {
        return Err("Triangle union area does not cover copper domain".into());
    }
    let mut terminal_by_path = BTreeMap::<String, Terminal>::new();
    for (region, parts) in terminals {
        let t = terminal_by_path
            .entry(region.source.path.clone())
            .or_insert_with(|| Terminal {
                source: region.source.clone(),
                region_ids: vec![],
                triangles: vec![],
                boundary_edges: vec![],
                area_mm2: 0.,
            });
        t.region_ids.push(region.id.clone());
        for (i, tri) in d.triangles.iter().enumerate() {
            let p = tri.map(|i| d.nodes[i]);
            let centre = Point::new(
                (p[0][0] + p[1][0] + p[2][0]) / 3.,
                (p[0][1] + p[1][1] + p[2][1]) / 3.,
            );
            if parts.contains(&centre) {
                t.triangles.push(i);
            }
        }
        t.boundary_edges.extend(
            d.boundaries
                .iter()
                .enumerate()
                .filter(|(_, b)| b.source_paths.contains(&region.source.path))
                .map(|(i, _)| i),
        );
    }
    for (_, mut t) in terminal_by_path {
        t.triangles.sort();
        t.triangles.dedup();
        t.boundary_edges.sort();
        t.boundary_edges.dedup();
        t.area_mm2 = t
            .triangles
            .iter()
            .map(|&i| {
                let p = d.triangles[i].map(|i| d.nodes[i]);
                ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                    - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]))
                    / 2.
            })
            .sum();
        d.terminals.push(t);
    }
    if !refinement_complete
        || d.max_area_mm2 > opt.max_area_mm2 * (1. + 1e-10)
        || d.min_angle_deg + 1e-8 < opt.min_angle_deg
    {
        err(d,r,"refinement_incomplete","Size/quality target not reached within bounded refinement; acute input angles may be unachievable");
    }
    Ok(())
}
