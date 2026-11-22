//! Layered CPU/F64 discretization, shared by diagnostics and the separate DC module.
mod mesh;
pub mod profile;
use crate::{
    geometry::{
        self, GeometryDiagnostic, GeometryOptions, PolygonSnapshot, PreparationReport, Provenance,
    },
    model::Board,
    Diagnostic,
};
use profile::{PhysicalProfile, ResolvedProfile};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MeshOptions {
    pub max_area_mm2: f64,
    /// Optional boundary subdivision cap. None preserves the diagnostic mesh.
    pub max_edge_mm: Option<f64>,
    pub min_angle_deg: f64,
    pub max_nodes: usize,
    pub max_triangles: usize,
    pub max_refinement_steps: usize,
}
impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            max_area_mm2: 1.,
            max_edge_mm: None,
            min_angle_deg: 10.,
            max_nodes: 500_000,
            max_triangles: 1_000_000,
            max_refinement_steps: 8,
        }
    }
}
impl MeshOptions {
    pub fn validate(&self) -> bool {
        self.max_area_mm2.is_finite()
            && self
                .max_edge_mm
                .is_none_or(|v| v.is_finite() && (1e-6..=1e6).contains(&v))
            && (1e-8..=1e6).contains(&self.max_area_mm2)
            && self.min_angle_deg.is_finite()
            && (0.0..=30.).contains(&self.min_angle_deg)
            && (3..=2_000_000).contains(&self.max_nodes)
            && (1..=4_000_000).contains(&self.max_triangles)
            && (0..=64).contains(&self.max_refinement_steps)
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConductionOptions {
    pub geometry: GeometryOptions,
    pub mesh: MeshOptions,
    pub profile: Option<PhysicalProfile>,
}
#[derive(Debug, Serialize)]
pub struct Readiness {
    pub mesh_complete: bool,
    pub topology_confirmed: bool,
    pub copper_complete: bool,
    pub physics_complete: bool,
    pub dc_ready: bool,
    pub reasons: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Boundary {
    pub nodes: [usize; 2],
    pub kind: String,
    pub source_paths: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Terminal {
    pub source: Provenance,
    pub region_ids: Vec<String>,
    pub triangles: Vec<usize>,
    pub boundary_edges: Vec<usize>,
    pub area_mm2: f64,
}
#[derive(Debug, Serialize)]
pub struct Domain {
    pub id: String,
    pub layer: String,
    pub region_ids: Vec<String>,
    pub source_paths: Vec<String>,
    pub readiness: Readiness,
    pub nodes: Vec<[f64; 2]>,
    pub triangles: Vec<[usize; 3]>,
    pub boundaries: Vec<Boundary>,
    pub terminals: Vec<Terminal>,
    pub area_mm2: f64,
    pub mesh_area_mm2: f64,
    pub min_angle_deg: f64,
    pub max_area_mm2: f64,
    pub refinement_steps: usize,
    pub terminal_alignment_mm: f64,
    pub sheet_conductance_s: Option<f64>,
}
#[derive(Debug, Serialize)]
pub struct BarrelContact {
    pub domain: String,
    pub layer: String,
    pub boundary_edges: Vec<usize>,
}
#[derive(Debug, Serialize)]
pub struct ExpectedBarrelContact {
    pub region_id: String,
    pub source_path: String,
    pub source_id: Option<String>,
    pub layer: String,
    pub domain_ids: Vec<String>,
    pub ready: bool,
    pub reasons: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Barrel {
    pub source: Provenance,
    pub hole_id: String,
    pub diameter_mm: f64,
    pub expected_contacts: Vec<ExpectedBarrelContact>,
    pub contacts: Vec<BarrelContact>,
    pub segments: Vec<BarrelSegment>,
    pub ready: bool,
    pub reasons: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct BarrelSegment {
    pub from_layer: String,
    pub to_layer: String,
    pub length_m: Option<f64>,
    pub cross_section_m2: Option<f64>,
    pub conductance_s: Option<f64>,
}
#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub domains: usize,
    pub built_domains: usize,
    pub ready_domains: usize,
    pub nodes: usize,
    pub triangles: usize,
    pub terminals: usize,
    pub barrels: usize,
}
#[derive(Debug, Serialize)]
pub struct ConductionReport {
    pub operation: &'static str,
    pub status: String,
    pub units: &'static str,
    pub coordinates: &'static str,
    pub options: ConductionOptions,
    pub approximation_mm: f64,
    pub boolean_error_bound_mm: f64,
    pub profile: ResolvedProfile,
    pub domains: Vec<Domain>,
    pub barrels: Vec<Barrel>,
    pub diagnostics: Vec<GeometryDiagnostic>,
    pub import_diagnostics: Vec<Diagnostic>,
    pub sources: Vec<Provenance>,
    pub fills: Vec<geometry::FillStatus>,
    pub excluded: Vec<Provenance>,
    pub findings: Vec<geometry::Finding>,
    pub counts: Counts,
    pub timings_ms: BTreeMap<String, f64>,
    pub serialized_bytes: usize,
}
fn empty(options: ConductionOptions) -> ConductionReport {
    ConductionReport {
        operation: "prepare-conduction",
        status: "invalid".into(),
        units: "mm; coefficients SI",
        coordinates: "absolute-y-down",
        approximation_mm: options.geometry.max_deviation_mm,
        boolean_error_bound_mm: 0.,
        options,
        profile: Default::default(),
        domains: vec![],
        barrels: vec![],
        diagnostics: vec![],
        import_diagnostics: vec![],
        sources: vec![],
        fills: vec![],
        excluded: vec![],
        findings: vec![],
        counts: Default::default(),
        timings_ms: BTreeMap::new(),
        serialized_bytes: 0,
    }
}
pub(super) fn issue(
    r: &mut ConductionReport,
    code: &str,
    path: &str,
    source_id: Option<String>,
    message: &str,
    category: &str,
) {
    r.diagnostics.push(GeometryDiagnostic {
        code: code.into(),
        path: path.into(),
        source_id,
        message: message.into(),
        category: category.into(),
    });
}
pub fn prepare_json(json: &str, options: ConductionOptions) -> ConductionReport {
    let start = Instant::now();
    let mut imported = crate::import_json(json);
    let mut r = if let Some(board) = imported.board.take() {
        prepare(&board, options)
    } else {
        empty(options)
    };
    r.import_diagnostics = imported.diagnostics;
    r.timings_ms
        .insert("total".into(), start.elapsed().as_secs_f64() * 1000.);
    finalize_size(&mut r);
    r
}
/// Direct Rust API: caller can retain the board and reuse its prepared geometry without JSON.
pub fn prepare(board: &Board, options: ConductionOptions) -> ConductionReport {
    let start = Instant::now();
    let g = geometry::prepare(board, options.geometry);
    let geom_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut r = prepare_geometry(board, &g, options);
    r.timings_ms.insert("geometry".into(), geom_ms);
    finalize_size(&mut r);
    r
}
/// A warning cannot change connectivity when an independently confirmed,
/// same-layer copper path already joins its endpoints. Never use net names or
/// barrel edges as evidence for a planar path.
struct PlanarConnectivity {
    roots: std::collections::BTreeMap<String, usize>,
}
impl PlanarConnectivity {
    fn new(g: &PreparationReport) -> Self {
        let indices: std::collections::BTreeMap<_, _> = g
            .regions
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.as_str(), i))
            .collect();
        let mut roots: Vec<_> = (0..g.regions.len()).collect();
        fn root(roots: &[usize], mut i: usize) -> usize {
            while roots[i] != i {
                i = roots[i];
            }
            i
        }
        for c in g
            .contacts
            .iter()
            .filter(|c| c.reliable && c.kind != "barrel")
        {
            let (Some(&a), Some(&b)) = (indices.get(c.a.as_str()), indices.get(c.b.as_str()))
            else {
                continue;
            };
            if g.regions[a].layer != g.regions[b].layer {
                continue;
            }
            let (a, b) = (root(&roots, a), root(&roots, b));
            roots[a.max(b)] = a.min(b);
        }
        Self {
            roots: g
                .regions
                .iter()
                .enumerate()
                .map(|(i, r)| (r.id.clone(), root(&roots, i)))
                .collect(),
        }
    }
    fn connected(&self, a: &str, b: &str) -> bool {
        self.roots
            .get(a)
            .is_some_and(|r| Some(r) == self.roots.get(b))
    }
    fn split_warning_redundant(&self, g: &PreparationReport, warning: &GeometryDiagnostic) -> bool {
        if warning.code != "drill_topology_uncertain" {
            return false;
        }
        let regions: Vec<_> = g
            .regions
            .iter()
            .filter(|r| r.source.path == warning.path)
            .collect();
        // Every surviving fragment of the split source must already have an
        // independent planar path around the drill; a disappeared source fails.
        regions.len() > 1
            && regions.iter().all(|r| {
                let same: Vec<_> = regions
                    .iter()
                    .filter(|other| other.layer == r.layer)
                    .collect();
                same.len() > 1 && same.iter().all(|other| self.connected(&r.id, &other.id))
            })
    }
    fn barrel_warning_redundant(
        &self,
        g: &PreparationReport,
        warning: &geometry::GeometryDiagnostic,
    ) -> bool {
        let Some((hole, _)) = warning.message.split_once(": ") else {
            return false;
        };
        let regions: Vec<_> = g
            .regions
            .iter()
            .filter(|r| r.source.path == warning.path)
            .collect();
        !regions.is_empty()
            && regions.iter().all(|region| {
                g.contacts
                    .iter()
                    .filter(|c| {
                        c.reliable && c.kind == "barrel" && c.drill_id.as_deref() == Some(hole)
                    })
                    .any(|c| self.connected(&region.id, &c.a) || self.connected(&region.id, &c.b))
            })
    }
}
pub fn prepare_geometry(
    board: &Board,
    g: &PreparationReport,
    options: ConductionOptions,
) -> ConductionReport {
    let start = Instant::now();
    let mut r = empty(options);
    if !r.options.mesh.validate() || r.options.geometry.validate().is_err() {
        issue(
            &mut r,
            "invalid_options",
            "options",
            None,
            "Mesh/geometry options outside supported finite bounds",
            "invalid",
        );
        return r;
    }
    if r.options.geometry.max_deviation_mm != g.options.max_deviation_mm
        || r.options.geometry.max_vertices_per_curve != g.options.max_vertices_per_curve
        || r.options.geometry.max_generated_vertices != g.options.max_generated_vertices
    {
        issue(
            &mut r,
            "geometry_options_mismatch",
            "options.geometry",
            None,
            "Prepared geometry tolerance differs from requested tolerance",
            "invalid",
        );
        return r;
    }
    r.boolean_error_bound_mm = g.boolean_error_bound_mm;
    r.diagnostics
        .extend(g.diagnostics.iter().map(|d| GeometryDiagnostic {
            code: d.code.clone(),
            path: d.path.clone(),
            source_id: d.source_id.clone(),
            message: d.message.clone(),
            category: d.category.clone(),
        }));
    if g.status == "invalid" {
        return r;
    }
    let profile = r.options.profile.clone();
    if !profile::resolve(board, profile.as_ref(), &mut r) {
        return r;
    }
    r.diagnostics.retain(|d| {
        d.code != "missing_layer_physics"
            || g.regions
                .iter()
                .any(|v| d.path == format!("layers.{}", v.layer))
    });
    let planar = PlanarConnectivity::new(g);
    for c in g
        .contacts
        .iter()
        .filter(|c| !c.reliable && c.kind != "point" && planar.connected(&c.a, &c.b))
    {
        issue(&mut r,"contact_topology_redundant",&c.a,None,&format!("{} and {} have an independently confirmed same-layer path; the uncertain direct contact is retained as a geometry limitation",c.a,c.b),"limitation");
    }
    for warning in g
        .diagnostics
        .iter()
        .filter(|d| d.code == "barrel_contact_uncertain" && planar.barrel_warning_redundant(g, d))
    {
        issue(&mut r,"barrel_contact_topology_redundant",&warning.path,warning.source_id.clone(),&format!("{}; every affected source fragment has an independently confirmed same-layer path to a confirmed contact of this barrel",warning.message),"limitation");
    }
    for warning in g
        .diagnostics
        .iter()
        .filter(|d| planar.split_warning_redundant(g, d))
    {
        issue(&mut r,"drill_split_topology_redundant",&warning.path,warning.source_id.clone(),"All surviving split fragments have an independently confirmed same-layer path around the drill; drill approximation remains recorded","limitation");
    }
    for layer in &g.layers {
        for polygon in &layer.copper {
            mesh::build_domain(g, &planar, &layer.layer, polygon, &mut r);
        }
    }
    build_barrels(board, g, &mut r);
    if r.diagnostics.iter().any(|d| d.category == "invalid") {
        r.domains.clear();
        r.barrels.clear();
        r.counts = Counts::default();
        return r;
    }
    r.sources = g.sources.clone();
    r.fills = g
        .fills
        .iter()
        .map(|f| geometry::FillStatus {
            source: f.source.clone(),
            layer: f.layer.clone(),
            status: f.status.clone(),
            freshness: f.freshness.clone(),
            polygons: f.polygons,
        })
        .collect();
    r.excluded = g.excluded.clone();
    r.findings = g
        .findings
        .iter()
        .map(|f| geometry::Finding {
            code: f.code.clone(),
            region_ids: f.region_ids.clone(),
            source_ids: f.source_ids.clone(),
            nets: f.nets.clone(),
            message: f.message.clone(),
        })
        .collect();
    r.counts.domains = r.domains.len();
    r.counts.built_domains = r.domains.iter().filter(|d| !d.triangles.is_empty()).count();
    r.counts.ready_domains = r.domains.iter().filter(|d| d.readiness.dc_ready).count();
    r.counts.terminals = r.domains.iter().map(|d| d.terminals.len()).sum();
    r.counts.barrels = r.barrels.len();
    r.status = if r.domains.iter().all(|d| d.readiness.dc_ready)
        && r.barrels.iter().all(|b| b.ready)
        && g.status == "complete"
        && !r
            .diagnostics
            .iter()
            .any(|d| d.category == "missing" || d.category == "invalid")
    {
        "complete"
    } else {
        "partial"
    }
    .into();
    r.timings_ms
        .insert("conduction".into(), start.elapsed().as_secs_f64() * 1000.);
    finalize_size(&mut r);
    r
}
fn finalize_size(r: &mut ConductionReport) {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 += b.len();
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for _ in 0..4 {
        let mut counter = Counter(0);
        let _ = serde_json::to_writer(&mut counter, r);
        let n = counter.0;
        if n == r.serialized_bytes {
            break;
        }
        r.serialized_bytes = n;
    }
}
fn build_barrels(board: &Board, g: &PreparationReport, r: &mut ConductionReport) {
    let planar = PlanarConnectivity::new(g);
    for h in g.holes.iter().filter(|h| h.plated) {
        let drill = board
            .pads
            .iter()
            .find(|p| p.source.path == h.source.path)
            .and_then(|p| p.drill.as_ref())
            .or_else(|| {
                board
                    .drills
                    .as_ref()
                    .and_then(|ds| ds.iter().find(|d| d.source.path == h.source.path))
                    .map(|d| &d.drill)
            });
        let via = board.vias.iter().find(|v| v.source.path == h.source.path);
        let diameter = drill
            .map(|d| d.diameter_mm)
            .or_else(|| via.map(|v| v.drill_mm));
        let mut b = Barrel {
            source: h.source.clone(),
            hole_id: h.id.clone(),
            diameter_mm: diameter.unwrap_or(0.),
            expected_contacts: vec![],
            contacts: vec![],
            segments: vec![],
            ready: true,
            reasons: vec![],
        };
        if !h.span_confirmed {
            b.reasons.push("span_unconfirmed".into());
        }
        // Geometry diagnostics identify the hole in the message, not in a
        // contact edge. An own annulus is not proof that this warning is redundant.
        let hole_prefix = format!("{}:", h.id);
        for warning in g.diagnostics.iter().filter(|d| {
            d.code == "barrel_contact_uncertain"
                && d.message.starts_with(&hole_prefix)
                && !planar.barrel_warning_redundant(g, d)
        }) {
            b.reasons
                .push(format!("barrel_contact_uncertain:{}", warning.path));
        }
        // Only geometry-confirmed barrel contacts participate. An isolated annulus
        // also qualifies when its source and full bore boundary are present.
        for d in &r.domains {
            if !h.layers.contains(&d.layer) {
                continue;
            }
            let edges: Vec<_> = d
                .boundaries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == "hole" && e.source_paths.contains(&h.source.path))
                .map(|(i, _)| i)
                .collect();
            let own = g
                .regions
                .iter()
                .any(|v| d.region_ids.contains(&v.id) && v.source.path == h.source.path);
            let confirmed = g.contacts.iter().any(|c| {
                c.reliable
                    && c.kind == "barrel"
                    && c.drill_id.as_ref() == Some(&h.id)
                    && (d.region_ids.contains(&c.a) || d.region_ids.contains(&c.b))
            });
            if !edges.is_empty() && (own || confirmed) && d.readiness.topology_confirmed {
                b.contacts.push(BarrelContact {
                    domain: d.id.clone(),
                    layer: d.layer.clone(),
                    boundary_edges: edges,
                });
            }
        }
        // Keep the geometry-confirmed branches even when meshing cleared their
        // boundaries. Absence from this set means no asserted geometry contact;
        // it does not require copper on every layer of the drill span.
        let expected_ids: std::collections::BTreeSet<_> = g
            .contacts
            .iter()
            .filter(|c| c.reliable && c.kind == "barrel" && c.drill_id.as_ref() == Some(&h.id))
            .flat_map(|c| [&c.a, &c.b])
            .collect();
        for region in g.regions.iter().filter(|v| expected_ids.contains(&v.id)) {
            let mut expected = ExpectedBarrelContact {
                region_id: region.id.clone(),
                source_path: region.source.path.clone(),
                source_id: region.source.source_id.clone(),
                layer: region.layer.clone(),
                domain_ids: vec![],
                ready: false,
                reasons: vec![],
            };
            for d in r
                .domains
                .iter()
                .filter(|d| d.region_ids.contains(&region.id))
            {
                expected.domain_ids.push(d.id.clone());
                if d.triangles.is_empty() {
                    expected.reasons.push(format!("mesh_unbuilt:{}", d.id));
                } else if !d.readiness.mesh_complete {
                    expected
                        .reasons
                        .push(format!("refinement_incomplete:{}", d.id));
                }
                if !d.readiness.topology_confirmed {
                    expected
                        .reasons
                        .push(format!("topology_uncertain:{}", d.id));
                }
                if !d
                    .boundaries
                    .iter()
                    .any(|e| e.kind == "hole" && e.source_paths.contains(&h.source.path))
                {
                    expected
                        .reasons
                        .push(format!("boundary_mapping_missing:{}", d.id));
                }
            }
            if expected.domain_ids.is_empty() {
                expected.reasons.push("domain_mapping_missing".into());
            }
            expected.ready = expected.reasons.is_empty();
            if !expected.ready {
                b.reasons
                    .push(format!("barrel_contact_unready:{}", region.id));
                issue(
                    r,
                    "barrel_contact_unready",
                    &expected.source_path,
                    expected.source_id.clone(),
                    &format!(
                        "{}: region {} on {}: {}",
                        h.id,
                        region.id,
                        region.layer,
                        expected.reasons.join(", ")
                    ),
                    "limitation",
                );
            }
            b.expected_contacts.push(expected);
        }
        let mut layers: Vec<_> = b.contacts.iter().map(|c| c.layer.clone()).collect();
        layers.sort_by_key(|name| board.layers.iter().position(|l| &l.name == name));
        layers.dedup();
        if layers.len() < 2 {
            b.reasons.push("insufficient_confirmed_contacts".into());
        }
        let bp = r.profile.barrel.clone();
        let section = diameter
            .zip(bp.plating_thickness_mm)
            .zip(bp.diameter_convention.as_ref())
            .and_then(|((d, t), c)| match drill.map(|d| &d.shape) {
                Some(crate::model::DrillShape::Slot { length_mm }) => {
                    profile::slot_barrel_area_mm2(d, *length_mm, t, c)
                }
                _ => profile::barrel_area_mm2(d, t, c),
            })
            .map(profile::square_metres);
        if section.is_none() || bp.material.is_none() {
            b.reasons
                .push("missing_barrel_physics_or_diameter_convention".into());
        }
        for pair in layers.windows(2) {
            let z = |name: &str| {
                r.profile
                    .layers
                    .iter()
                    .find(|l| l.layer == name)
                    .and_then(|l| l.z_mm)
            };
            let length = z(&pair[0])
                .zip(z(&pair[1]))
                .map(|(a, b)| profile::metres(b - a))
                .filter(|v| *v > 0.);
            if length.is_none() {
                b.reasons.push("missing_stack_z".into());
            }
            let conductance = length
                .zip(section)
                .zip(bp.material.as_ref())
                .map(|((l, a), m)| m.conductivity_s_m * a / l);
            if conductance.is_some_and(|v| !v.is_finite() || v <= 0.) {
                issue(
                    r,
                    "invalid_barrel_coefficient",
                    &h.source.path,
                    h.source.source_id.clone(),
                    "Resolved barrel coefficient is nonfinite or underflows to zero in F64/SI",
                    "invalid",
                );
                b.reasons.push("invalid_barrel_coefficient".into());
            }
            b.segments.push(BarrelSegment {
                from_layer: pair[0].clone(),
                to_layer: pair[1].clone(),
                length_m: length,
                cross_section_m2: section,
                conductance_s: conductance.filter(|v| v.is_finite() && *v > 0.),
            });
        }
        b.reasons.sort();
        b.reasons.dedup();
        b.ready = b.reasons.is_empty();
        if !b.ready {
            for d in &mut r.domains {
                if b.contacts.iter().any(|c| c.domain == d.id)
                    || b.expected_contacts
                        .iter()
                        .any(|c| c.domain_ids.contains(&d.id))
                    || d.source_paths.contains(&b.source.path)
                {
                    d.readiness.physics_complete = false;
                    d.readiness.dc_ready = false;
                    d.readiness
                        .reasons
                        .push(format!("barrel_not_ready:{}", b.hole_id));
                }
            }
            issue(
                r,
                "barrel_not_ready",
                &h.source.path,
                h.source.source_id.clone(),
                &b.reasons.join(", "),
                "limitation",
            );
        }
        r.barrels.push(b);
    }
}
#[cfg(test)]
mod tests;
pub(super) fn polygon(p: &PolygonSnapshot) -> geo::Polygon<f64> {
    let ring = |r: &Vec<[f64; 2]>| {
        let mut v: Vec<_> = r.iter().map(|p| geo::Coord { x: p[0], y: p[1] }).collect();
        if let Some(p) = v.first().copied() {
            v.push(p);
        }
        geo::LineString(v)
    };
    geo::Polygon::new(ring(&p.outer), p.holes.iter().map(ring).collect())
}
