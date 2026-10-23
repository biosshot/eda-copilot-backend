//! Physical copper preparation; consumes validated model::Board only.
mod areas;
mod primitives;
mod spatial;
mod topology;
use crate::{
    model::{self, Board, CadFillState, Freshness, Source},
    Diagnostic,
};
use areas::{AreaSet, Poly};
use geo::{Area, BooleanOps, BoundingRect, Validation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeometryOptions {
    pub max_deviation_mm: f64,
    pub max_vertices_per_curve: usize,
    pub max_generated_vertices: usize,
}
impl Default for GeometryOptions {
    fn default() -> Self {
        Self {
            max_deviation_mm: 0.001,
            max_vertices_per_curve: 16384,
            max_generated_vertices: 2_000_000,
        }
    }
}
impl GeometryOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !self.max_deviation_mm.is_finite()
            || !(0.000001..=0.01).contains(&self.max_deviation_mm)
            || !(8..=65536).contains(&self.max_vertices_per_curve)
            || !(8..=10_000_000).contains(&self.max_generated_vertices)
        {
            Err("Invalid geometry options: deviation 0.000001..0.01 mm, curve vertices 8..65536, generated vertices 8..10000000".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct PolygonSnapshot {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Provenance {
    pub kind: String,
    pub path: String,
    pub source_id: Option<String>,
    pub component_id: Option<String>,
    pub pad_number: Option<String>,
    pub native_fill_ids: Vec<String>,
    pub net: Option<String>,
    pub ownership: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Region {
    pub id: String,
    pub layer: String,
    pub source: Provenance,
    pub polygon: PolygonSnapshot,
    pub area_mm2: f64,
    pub approximation_mm: f64,
}
#[derive(Debug, Serialize)]
pub struct Hole {
    pub id: String,
    pub source: Provenance,
    pub layers: Vec<String>,
    pub plated: bool,
    pub span_confirmed: bool,
    pub polygons: Vec<PolygonSnapshot>,
}
#[derive(Debug, Serialize)]
pub struct Contact {
    pub a: String,
    pub b: String,
    pub kind: String,
    pub reliable: bool,
    pub overlap_mm2: f64,
    pub drill_id: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Island {
    pub id: String,
    pub region_ids: Vec<String>,
    pub nets: Vec<String>,
    pub has_unknown_net: bool,
}
#[derive(Debug, Serialize)]
pub struct Finding {
    pub code: String,
    pub region_ids: Vec<String>,
    pub source_ids: Vec<String>,
    pub nets: Vec<String>,
    pub message: String,
}
#[derive(Debug, Serialize)]
pub struct GeometryDiagnostic {
    pub code: String,
    pub path: String,
    pub source_id: Option<String>,
    pub message: String,
    pub category: String,
}
#[derive(Debug, Serialize)]
pub struct FillStatus {
    pub source: Provenance,
    pub layer: String,
    pub status: String,
    pub freshness: Option<String>,
    pub polygons: usize,
}
#[derive(Debug, Serialize)]
pub struct LayerSnapshot {
    pub layer: String,
    pub union_area_mm2: f64,
    pub copper: Vec<PolygonSnapshot>,
    pub regions: usize,
}
#[derive(Debug, Default, Serialize)]
pub struct Statistics {
    pub source_counts: BTreeMap<String, usize>,
    pub accepted_sources: usize,
    pub excluded_sources: usize,
    pub regions: usize,
    pub islands: usize,
    pub candidate_pairs: usize,
    pub narrow_phase_pairs: usize,
    pub generated_vertices: usize,
    pub shorts: usize,
}
#[derive(Debug, Serialize)]
pub struct PreparationReport {
    pub operation: &'static str,
    pub status: String,
    pub units: &'static str,
    pub coordinates: &'static str,
    pub options: GeometryOptions,
    pub boolean_error_bound_mm: f64,
    pub diagnostics: Vec<GeometryDiagnostic>,
    pub import_diagnostics: Vec<Diagnostic>,
    pub outline: Option<PolygonSnapshot>,
    pub regions: Vec<Region>,
    pub holes: Vec<Hole>,
    pub layers: Vec<LayerSnapshot>,
    pub contacts: Vec<Contact>,
    pub islands: Vec<Island>,
    pub net_islands: BTreeMap<String, Vec<String>>,
    pub findings: Vec<Finding>,
    pub fills: Vec<FillStatus>,
    pub sources: Vec<Provenance>,
    pub excluded: Vec<Provenance>,
    pub statistics: Statistics,
}
struct Copper {
    poly: Poly,
    provenance: Provenance,
    layer: usize,
    error: f64,
    id: String,
}
struct Bore {
    drill: model::Drill,
    at: model::Point,
    rotation_deg: f64,
    shape: AreaSet,
    layers: Vec<usize>,
    plated: bool,
    span_confirmed: bool,
    id: String,
    source: Provenance,
}
struct Raw {
    shape: AreaSet,
    source: Provenance,
    layer: usize,
    error: f64,
}

fn provenance(
    board: &Board,
    source: &Source,
    kind: &str,
    net: Option<usize>,
    ownership: Option<model::Ownership>,
) -> Provenance {
    Provenance {
        kind: kind.into(),
        path: source.path.clone(),
        source_id: source.id.clone(),
        net: net.map(|n| board.nets[n].clone()),
        component_id: None,
        pad_number: None,
        native_fill_ids: vec![],
        ownership: ownership.map(|o| format!("{o:?}").to_lowercase()),
    }
}
fn key(p: &Provenance) -> String {
    format!("{}:{}", p.kind, p.source_id.as_deref().unwrap_or(&p.path))
}
fn diagnostic(
    r: &mut PreparationReport,
    p: &Provenance,
    code: &str,
    message: impl Into<String>,
    category: &str,
) {
    r.diagnostics.push(GeometryDiagnostic {
        code: code.into(),
        path: p.path.clone(),
        source_id: p.source_id.clone(),
        message: message.into(),
        category: category.into(),
    });
}
fn invalid_source(r: &mut PreparationReport, p: Provenance, error: String) {
    diagnostic(r, &p, "invalid_geometry", error, "invalid");
    r.excluded.push(p);
}
fn empty(options: GeometryOptions) -> PreparationReport {
    PreparationReport {
        operation: "prepare-geometry",
        status: "invalid".into(),
        units: "mm",
        coordinates: "absolute-y-down",
        options,
        boolean_error_bound_mm: 0.,
        diagnostics: vec![],
        import_diagnostics: vec![],
        outline: None,
        regions: vec![],
        holes: vec![],
        layers: vec![],
        contacts: vec![],
        islands: vec![],
        net_islands: BTreeMap::new(),
        findings: vec![],
        fills: vec![],
        sources: vec![],
        excluded: vec![],
        statistics: Statistics::default(),
    }
}
pub fn prepare_json(json: &str, options: GeometryOptions) -> PreparationReport {
    let mut imported = crate::import_json(json);
    let mut report = match imported.board.take() {
        Some(b) => prepare(&b, options),
        None => empty(options),
    };
    report.import_diagnostics = imported.diagnostics;
    report
}
pub fn prepare(board: &Board, options: GeometryOptions) -> PreparationReport {
    prepare_observed(board, options, None)
}
/// Validation-only hook: inspect discarded outside copper without retaining a
/// second whole-board geometry or changing the existing preparation contract.
pub(crate) fn prepare_observed(
    board: &Board,
    options: GeometryOptions,
    mut outside_observer: Option<&mut dyn FnMut(&Provenance, &str, &AreaSet, f64)>,
) -> PreparationReport {
    let mut r = empty(options);
    let root = Provenance {
        kind: "board".into(),
        path: "$".into(),
        source_id: None,
        component_id: None,
        pad_number: None,
        native_fill_ids: vec![],
        net: None,
        ownership: None,
    };
    if let Err(e) = options.validate() {
        diagnostic(&mut r, &root, "invalid_options", e, "invalid");
        return r;
    }
    let board_poly = match areas::polygon(&model::Polygon {
        outer: board.outline.clone(),
        holes: board.cutouts.clone(),
    }) {
        Ok(p) => p,
        Err(e) => {
            diagnostic(&mut r, &root, "invalid_outline", e, "invalid");
            return r;
        }
    };
    r.outline = Some(areas::snapshot(&board_poly));
    let material = areas::one(board_poly);
    let mut builder = primitives::Builder {
        options,
        generated: 0,
    };
    let mut raw = Vec::<Raw>::new();
    let mut bores = Vec::<Bore>::new();
    let all_layers: Vec<_> = (0..board.layers.len()).collect();
    // Keepouts constrain design; they do not subtract captured copper.
    for k in &board.keepouts {
        r.sources
            .push(provenance(board, &k.source, "keepout", None, None));
    }
    for pad in &board.pads {
        let mut p = provenance(board, &pad.source, "pad", pad.net, None);
        p.component_id = pad.component.map(|i| {
            board.components[i]
                .source
                .id
                .clone()
                .unwrap_or_else(|| board.components[i].designator.clone())
        });
        p.pad_number = Some(pad.number.clone());
        r.sources.push(p.clone());
        let mut pending = vec![];
        let mut error = None;
        for &layer in &pad.layers {
            let shape = pad
                .layer_shapes
                .as_ref()
                .and_then(|m| m.get(&layer))
                .unwrap_or(&pad.common_shape);
            match builder.shape(shape) {
                Ok((s, e)) => pending.push(Raw {
                    shape: primitives::transform(s, pad.at, pad.rotation_deg),
                    source: p.clone(),
                    layer,
                    error: e,
                }),
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = error {
            invalid_source(&mut r, p.clone(), e);
        } else {
            raw.extend(pending);
        }
        if let Some(drill) = &pad.drill {
            // V2 exporter expands native MULTI pads to all copper layers. A
            // subset alone cannot establish a through/blind drilled span.
            let through = pad.layers.len() == board.layers.len()
                && all_layers.iter().all(|l| pad.layers.contains(l));
            match builder.drill(drill, pad.at, pad.rotation_deg) {
                Ok(s) => {
                    if !through {
                        diagnostic(&mut r,&p,"drill_span_unconfirmed","Pad hole has no confirmed full span: cut only declared pad layers; no intermediate layers, through cut or barrel links inferred","limitation");
                    }
                    bores.push(Bore {
                        drill: drill.clone(),
                        at: pad.at,
                        rotation_deg: pad.rotation_deg,
                        shape: s,
                        layers: pad.layers.clone(),
                        plated: drill.plated,
                        span_confirmed: through,
                        id: format!("hole:{}", key(&p)),
                        source: p.clone(),
                    });
                }
                Err(e) => {
                    raw.retain(|x| x.source.path != p.path);
                    invalid_source(&mut r, p, e)
                }
            }
        }
    }
    for t in &board.tracks {
        let p = provenance(board, &t.source, "track", t.net, Some(t.ownership));
        r.sources.push(p.clone());
        match builder.stroke(&t.points, t.width_mm) {
            Ok(shape) => raw.push(Raw {
                shape,
                source: p,
                layer: t.layer,
                error: options.max_deviation_mm,
            }),
            Err(e) => invalid_source(&mut r, p, e),
        }
    }
    for v in &board.vias {
        let p = provenance(board, &v.source, "via", v.net, Some(v.ownership));
        r.sources.push(p.clone());
        let span: Vec<_> = (v.first_layer..=v.last_layer).collect();
        let result = (|| {
            let land = areas::one(builder.circle(v.at, v.diameter_mm / 2.)?);
            let hole = areas::one(builder.circle(v.at, v.drill_mm / 2.)?);
            Ok::<_, String>((land, hole))
        })();
        match result {
            Ok((s, h)) => {
                for &layer in &span {
                    raw.push(Raw {
                        shape: s.clone(),
                        source: p.clone(),
                        layer,
                        error: options.max_deviation_mm,
                    });
                }
                bores.push(Bore {
                    drill: model::Drill {
                        shape: model::DrillShape::Round,
                        diameter_mm: v.drill_mm,
                        offset: None,
                        rotation_deg: None,
                        plated: true,
                    },
                    at: v.at,
                    rotation_deg: 0.,
                    shape: h,
                    layers: span,
                    plated: true,
                    span_confirmed: true,
                    id: format!("hole:{}", key(&p)),
                    source: p,
                });
            }
            Err(e) => invalid_source(&mut r, p, e),
        }
    }
    for d in board.drills.iter().flatten() {
        let p = provenance(board, &d.source, "independent-drill", None, None);
        r.sources.push(p.clone());
        // Exported independent holes are native drill-only NPTH pads. V2 has
        // no span for independent plated drills: do not infer one from plating.
        let layers = if d.drill.plated {
            diagnostic(
                &mut r,
                &p,
                "drill_span_unconfirmed",
                "Independent plated drill has no confirmed span, net or lands",
                "limitation",
            );
            vec![]
        } else {
            all_layers.clone()
        };
        match builder.drill(&d.drill, d.at, 0.) {
            Ok(s) => bores.push(Bore {
                drill: d.drill.clone(),
                at: d.at,
                rotation_deg: 0.,
                shape: s,
                layers,
                plated: d.drill.plated,
                span_confirmed: !d.drill.plated,
                id: format!("hole:{}", key(&p)),
                source: p,
            }),
            Err(e) => invalid_source(&mut r, p, e),
        }
    }
    for z in &board.zones {
        let kind = match z.kind {
            model::ZoneKind::Fill => "fill",
            model::ZoneKind::Pour { .. } => "pour",
        };
        let p = provenance(board, &z.source, kind, z.net, Some(z.ownership));
        r.sources.push(p.clone());
        for f in &z.fills {
            let mut fp = p.clone();
            fp.path = f.source.path.clone();
            fp.native_fill_ids = f.native_ids.clone().unwrap_or_default();
            match &f.state {
                CadFillState::Available {
                    polygons,
                    freshness,
                } => {
                    let freshness = match freshness {
                        Freshness::Current => "current",
                        Freshness::Stale => "stale",
                        Freshness::Unknown => "unknown",
                    };
                    r.fills.push(FillStatus {
                        source: fp.clone(),
                        layer: board.layers[f.layer].name.clone(),
                        status: "available".into(),
                        freshness: Some(freshness.into()),
                        polygons: polygons.len(),
                    });
                    if freshness != "current" {
                        diagnostic(&mut r,&fp,"fill_freshness",format!("CAD fill freshness is {freshness}; snapshot is not a claim of current CAD copper"),"limitation");
                    }
                    // Validate each fragment independently. Invalid fragments are
                    // listed, retained fragments never stand for a complete zone.
                    for (i, poly) in polygons.iter().enumerate() {
                        let mut src = fp.clone();
                        src.path = format!("{}.polygons[{i}]", fp.path);
                        match areas::polygon(poly) {
                            Ok(poly) => raw.push(Raw {
                                shape: areas::one(poly),
                                source: src,
                                layer: f.layer,
                                error: 0.,
                            }),
                            Err(e) => invalid_source(&mut r, src, e),
                        }
                    }
                }
                CadFillState::Unavailable { state, reason } => {
                    let state = format!("{state:?}").to_lowercase();
                    r.fills.push(FillStatus {
                        source: fp.clone(),
                        layer: board.layers[f.layer].name.clone(),
                        status: state.clone(),
                        freshness: None,
                        polygons: 0,
                    });
                    diagnostic(
                        &mut r,
                        &fp,
                        "fill_unavailable",
                        format!("CAD fill {state}: {reason}; boundary is not copper"),
                        "missing",
                    );
                }
            }
        }
    }
    bores.retain(|b| {
        let error = b
            .shape
            .0
            .iter()
            .find_map(|p| p.check_validation().err().map(|e| e.to_string()))
            .or_else(|| {
                b.shape
                    .0
                    .iter()
                    .flat_map(|p| p.exterior().0.iter())
                    .any(|p| {
                        !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > 10000.
                    })
                    .then(|| "Drill exceeds finite coordinate limit".into())
            });
        if let Some(e) = error {
            raw.retain(|x| x.source.path != b.source.path);
            invalid_source(&mut r, b.source.clone(), e);
            false
        } else {
            true
        }
    });
    // Use one conservative bound for every geo/i_overlay conversion in this
    // result. Adapter integer scale is a power of two with ~29 significant
    // bits. The factor 64 covers rounding of both operands and output vertices.
    let bounds = raw
        .iter()
        .flat_map(|x| x.shape.0.iter())
        .chain(material.0.iter())
        .chain(bores.iter().flat_map(|b| b.shape.0.iter()))
        .filter_map(|p| p.bounding_rect())
        .filter(|b| {
            [b.min().x, b.min().y, b.max().x, b.max().y]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 10000.)
        })
        .collect::<Vec<_>>();
    let minx = bounds
        .iter()
        .map(|b| b.min().x)
        .fold(f64::INFINITY, f64::min);
    let maxx = bounds
        .iter()
        .map(|b| b.max().x)
        .fold(f64::NEG_INFINITY, f64::max);
    let miny = bounds
        .iter()
        .map(|b| b.min().y)
        .fold(f64::INFINITY, f64::min);
    let maxy = bounds
        .iter()
        .map(|b| b.max().y)
        .fold(f64::NEG_INFINITY, f64::max);
    r.boolean_error_bound_mm =
        ((maxx - minx).max(maxy - miny) * 64. / (1u64 << 29) as f64).max(1e-12);
    // Validate transformed/generated polygons before handing anything to boolean
    // operations. No repair() or small-area filter is used.
    let mut valid = vec![];
    for x in raw {
        if let Some(e) = x.shape.0.iter().find_map(|p| p.check_validation().err()) {
            invalid_source(&mut r, x.source, format!("Invalid generated geometry: {e}"));
        } else if x
            .shape
            .0
            .iter()
            .flat_map(|p| p.exterior().0.iter())
            .any(|p| p.x.abs().max(p.y.abs()) > 10000.)
        {
            invalid_source(
                &mut r,
                x.source,
                "Transformed geometry exceeds coordinate limit".into(),
            );
        } else {
            let unresolved = x.shape.0.iter().any(|p| {
                std::iter::once(p.exterior())
                    .chain(p.interiors())
                    .any(|ring| {
                        let b = ring.bounding_rect().unwrap();
                        b.width().min(b.height()) <= 2. * r.boolean_error_bound_mm
                    })
            });
            if unresolved {
                invalid_source(&mut r,x.source,"Ring feature below boolean representation resolution; excluded without simplification".into());
            } else {
                valid.push(x);
            }
        }
    }
    if board.drills.is_none() {
        diagnostic(
            &mut r,
            &root,
            "independent_drills_missing",
            "Independent drill inventory was not supplied; no holes inferred",
            "missing",
        );
    }
    // Retain one drilled geometry. Circumscribed circles/capsules contain the
    // physical drill; the conversion guard prevents a rounded false copper neck.
    for b in &mut bores {
        match builder.enclosing_drill(
            &b.drill,
            b.at,
            b.rotation_deg,
            2. * r.boolean_error_bound_mm,
        ) {
            Ok(shape) => b.shape = shape,
            Err(e) => {
                diagnostic(&mut r, &b.source, "drill_geometry_unresolved", e, "invalid");
                r.statistics.generated_vertices = builder.generated;
                return r;
            }
        }
        if b.shape.0.iter().any(|p| {
            !p.is_valid()
                || p.exterior().0.iter().any(|p| {
                    !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > 10000.
                })
        }) {
            diagnostic(
                &mut r,
                &b.source,
                "drill_geometry_unresolved",
                "Enclosing drill is invalid or exceeds finite coordinate limit",
                "invalid",
            );
            r.statistics.generated_vertices = builder.generated;
            return r;
        }
    }
    let drill_index = spatial::Index::new(
        bores
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.shape.bounding_rect().map(|r| (i, r))),
    );
    let mut copper = Vec::new();
    for x in &valid {
        let outside = x.shape.difference(&material).unsigned_area();
        if outside > 0. {
            diagnostic(&mut r,&x.source,"copper_outside_board",format!("{outside:.9} mm2 lies outside board material; derived snapshot intersects outline/cutouts, edge/castellation validity unresolved"),"limitation");
        }
        let inside = if outside == 0. {
            x.shape.clone()
        } else {
            x.shape.intersection(&material)
        };
        let nearby: Vec<_> = drill_index
            .query(x.shape.bounding_rect().unwrap(), 0.)
            .into_iter()
            .filter(|&i| bores[i].layers.contains(&x.layer))
            .collect();
        let drills = areas::union(nearby.iter().flat_map(|&i| bores[i].shape.0.clone()));
        if outside > 0. {
            if let Some(observer) = &mut outside_observer {
                observer(&x.source, &board.layers[x.layer].name,
                    &areas::subtract(&x.shape.difference(&material), &drills),
                    if nearby.is_empty() { x.error } else { x.error.max(options.max_deviation_mm) + 2. * r.boolean_error_bound_mm });
            }
        }
        let shape = areas::subtract(&inside, &drills);
        // Only a split/disappearing source can hide a path/feature with this
        // drill enclosure. Compare locally with inscribed drills, then discard
        // the temporary polygons; no second board or graph is retained.
        if !nearby.is_empty()
            && (shape.0.len() > inside.0.len() || (shape.0.is_empty() && !inside.0.is_empty()))
        {
            let check = (|| {
                let mut pieces = Vec::new();
                for &i in &nearby {
                    let b = &bores[i];
                    pieces.extend(builder.drill(&b.drill, b.at, b.rotation_deg)?.0);
                }
                Ok::<_, String>(areas::subtract(&inside, &areas::union(pieces)))
            })();
            match check {
                Ok(check) if check.0.len()!=shape.0.len() => diagnostic(&mut r,&x.source,"drill_topology_uncertain","Drill approximation changes the source's component count; retained copper uses enclosing drills. A split/absent region is not a confirmed open; tighten local geometry tolerance to resolve","limitation"),
                Err(e) => diagnostic(&mut r,&x.source,"drill_topology_unresolved",e,"limitation"),
                _=>{}
            }
        }
        for poly in areas::sorted(shape.0) {
            if !poly.is_valid() {
                invalid_source(
                    &mut r,
                    x.source.clone(),
                    "Boolean output invalid; fragment excluded".into(),
                );
                continue;
            }
            copper.push(Copper {
                poly,
                provenance: x.source.clone(),
                layer: x.layer,
                error: if nearby.is_empty() {
                    x.error
                } else {
                    x.error.max(options.max_deviation_mm) + 2. * r.boolean_error_bound_mm
                },
                id: String::new(),
            });
        }
    }
    copper.sort_by(|a, b| {
        key(&a.provenance)
            .cmp(&key(&b.provenance))
            .then(a.provenance.path.cmp(&b.provenance.path))
            .then(a.layer.cmp(&b.layer))
            .then_with(|| {
                a.poly
                    .bounding_rect()
                    .unwrap()
                    .min()
                    .x
                    .total_cmp(&b.poly.bounding_rect().unwrap().min().x)
            })
            .then_with(|| {
                a.poly
                    .bounding_rect()
                    .unwrap()
                    .min()
                    .y
                    .total_cmp(&b.poly.bounding_rect().unwrap().min().y)
            })
            .then_with(|| a.poly.unsigned_area().total_cmp(&b.poly.unsigned_area()))
    });
    for (i, c) in copper.iter_mut().enumerate() {
        c.id = format!("region-{i:06}");
    }
    topology::build(board, &copper, &bores, &valid, &material, &mut r);
    r.regions = copper
        .iter()
        .map(|c| Region {
            id: c.id.clone(),
            layer: board.layers[c.layer].name.clone(),
            source: c.provenance.clone(),
            polygon: areas::snapshot(&c.poly),
            area_mm2: c.poly.unsigned_area(),
            approximation_mm: c.error,
        })
        .collect();
    for b in &bores {
        r.holes.push(Hole {
            id: b.id.clone(),
            source: b.source.clone(),
            layers: b
                .layers
                .iter()
                .map(|&l| board.layers[l].name.clone())
                .collect(),
            plated: b.plated,
            span_confirmed: b.span_confirmed,
            polygons: b.shape.0.iter().map(areas::snapshot).collect(),
        });
    }
    for (layer, l) in board.layers.iter().enumerate() {
        let ps: Vec<_> = copper
            .iter()
            .filter(|c| c.layer == layer)
            .map(|c| c.poly.clone())
            .collect();
        let n = ps.len();
        let mut merged = areas::union(ps);
        let mut resolved=Vec::new();
        for p in merged.0 {
            if p.is_valid() { resolved.push(p); }
            else { match areas::separate_touches(&p) {
                Ok(parts)=>{ resolved.extend(parts); r.diagnostics.push(GeometryDiagnostic{code:"union_boundary_decomposed".into(),path:format!("layers.{}",l.name),source_id:None,message:"Overlay boundary decomposed at exact vertex touches; coordinates, area and voids retained".into(),category:"limitation".into()}); },
                Err(_)=>resolved.push(p),
            }}
        }
        merged.0=resolved;
        r.layers.push(LayerSnapshot {
            layer: l.name.clone(),
            union_area_mm2: merged.unsigned_area(),
            copper: merged.0.iter().map(areas::snapshot).collect(),
            regions: n,
        });
    }
    for p in &r.sources {
        *r.statistics
            .source_counts
            .entry(p.kind.clone())
            .or_default() += 1;
    }
    r.statistics.excluded_sources = r.excluded.iter().map(key).collect::<BTreeSet<_>>().len();
    r.statistics.accepted_sources = r
        .sources
        .len()
        .saturating_sub(r.statistics.excluded_sources);
    r.statistics.regions = r.regions.len();
    r.statistics.islands = r.islands.len();
    r.statistics.generated_vertices = builder.generated;
    r.statistics.shorts = r
        .findings
        .iter()
        .filter(|f| f.code == "physical_short")
        .count();
    r.status = if r.diagnostics.iter().any(|d| d.category == "invalid") {
        "invalid"
    } else if !r.diagnostics.is_empty() {
        "partial"
    } else {
        "complete"
    }
    .into();
    r
}
