use super::{dto::*, Diagnostic, Severity};
use std::collections::{BTreeMap, BTreeSet};

struct Check<'a> {
    board: &'a RoutingBoardV2,
    layers: BTreeSet<&'a str>,
    nets: BTreeSet<&'a str>,
    components: BTreeSet<&'a str>,
    pads: BTreeSet<&'a str>,
    diagnostics: Vec<Diagnostic>,
    source_id: Option<String>,
}
impl Check<'_> {
    fn emit(
        &mut self,
        severity: Severity,
        code: &'static str,
        path: &str,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity,
            code,
            path: path.into(),
            source_id: self.source_id.clone(),
            message: message.into(),
        });
    }
    fn error(&mut self, code: &'static str, path: &str, message: impl Into<String>) {
        self.emit(Severity::Error, code, path, message);
    }
    fn finite(&mut self, n: f64, p: &str) {
        if !n.is_finite() {
            self.error("non_finite", p, "Expected a finite number");
        }
    }
    fn nonnegative(&mut self, n: f64, p: &str) {
        self.finite(n, p);
        if n < 0.0 {
            self.error("invalid_dimension", p, "Expected a nonnegative value");
        }
    }
    fn positive(&mut self, n: f64, p: &str) {
        self.finite(n, p);
        if n <= 0.0 {
            self.error("invalid_dimension", p, "Expected a positive value");
        }
    }
    fn optional_positive(&mut self, n: Option<f64>, p: &str) {
        if let Some(n) = n {
            self.positive(n, p);
        }
    }
    fn optional_nonnegative(&mut self, n: Option<f64>, p: &str) {
        if let Some(n) = n {
            self.nonnegative(n, p);
        }
    }
    fn text(&mut self, s: &str, p: &str) {
        if s.trim().is_empty() {
            self.error("empty_identifier", p, "Expected a nonempty string");
        }
    }
    fn unique<'a>(&mut self, values: impl Iterator<Item = &'a str>, p: &str) {
        let mut seen = BTreeSet::new();
        for (i, s) in values.enumerate() {
            self.text(s, &format!("{p}[{i}]"));
            if !seen.insert(s) {
                self.error(
                    "duplicate_identifier",
                    p,
                    format!("Duplicate identifier {s:?}"),
                );
            }
        }
    }
    fn layer(&mut self, s: &str, p: &str) {
        if !self.layers.contains(s) {
            self.error("unknown_layer", p, format!("Unknown layer {s:?}"));
        }
    }
    fn net(&mut self, s: &str, p: &str) {
        if !self.nets.contains(s) {
            self.error("unknown_net", p, format!("Unknown net {s:?}"));
        }
    }
    fn optional_net(&mut self, s: &Option<String>, p: &str) {
        if let Some(s) = s {
            self.net(s, p);
        } else {
            self.emit(
                Severity::Missing,
                "unknown_copper_net",
                p,
                "Electrical net is absent; connectivity must not be inferred",
            );
        }
    }
    fn layer_list(&mut self, ls: &[String], p: &str) {
        if ls.is_empty() {
            self.error("empty_layers", p, "Expected at least one layer");
        }
        self.unique(ls.iter().map(String::as_str), p);
        for (i, l) in ls.iter().enumerate() {
            self.layer(l, &format!("{p}[{i}]"));
        }
    }
    fn point(&mut self, v: &Point, p: &str) {
        self.finite(v.x, &format!("{p}.x"));
        self.finite(v.y, &format!("{p}.y"));
    }
    fn ring(&mut self, r: &[Point], p: &str) {
        for (i, v) in r.iter().enumerate() {
            self.point(v, &format!("{p}[{i}]"));
        }
        let distinct: BTreeSet<_> = r
            .iter()
            .map(|v| {
                (
                    (if v.x == 0.0 { 0.0 } else { v.x }).to_bits(),
                    (if v.y == 0.0 { 0.0 } else { v.y }).to_bits(),
                )
            })
            .collect();
        if distinct.len() < 3 {
            self.error(
                "invalid_ring",
                p,
                "A ring needs at least three distinct vertices",
            );
        }
        // Local origin avoids cancellation for boards with large absolute offsets.
        if let Some(o) = r.first() {
            let area: f64 = r
                .iter()
                .zip(r.iter().cycle().skip(1))
                .take(r.len())
                .map(|(a, b)| (a.x - o.x) * (b.y - o.y) - (b.x - o.x) * (a.y - o.y))
                .sum();
            if area == 0.0 || !area.is_finite() {
                self.error(
                    "invalid_ring",
                    p,
                    "Ring has zero or non-finite signed area; no repair performed",
                );
            }
        }
    }
    fn polygon(&mut self, v: &Polygon, p: &str) {
        self.ring(&v.outer, &format!("{p}.outer"));
        if let Some(holes) = &v.holes {
            for (i, h) in holes.iter().enumerate() {
                self.ring(h, &format!("{p}.holes[{i}]"));
            }
        }
    }
    fn shape(&mut self, s: &PadShape, p: &str) {
        match s {
            PadShape::Circle { diameter_mm } => {
                self.positive(*diameter_mm, &format!("{p}.diameterMm"))
            }
            PadShape::Rect {
                width_mm,
                height_mm,
            }
            | PadShape::Oval {
                width_mm,
                height_mm,
            }
            | PadShape::RoundRect {
                width_mm,
                height_mm,
                ..
            } => {
                self.positive(*width_mm, &format!("{p}.widthMm"));
                self.positive(*height_mm, &format!("{p}.heightMm"));
                if let PadShape::RoundRect {
                    corner_radius_mm, ..
                } = s
                {
                    self.nonnegative(*corner_radius_mm, &format!("{p}.cornerRadiusMm"));
                    if *corner_radius_mm > width_mm.min(*height_mm) / 2.0 {
                        self.error(
                            "invalid_corner_radius",
                            p,
                            "Corner radius exceeds half the smaller dimension",
                        );
                    }
                }
            }
            PadShape::Polygon { polygon } => self.polygon(polygon, &format!("{p}.polygon")),
        }
    }
    fn hole(&mut self, h: &Hole, p: &str) {
        self.positive(h.diameter_mm, &format!("{p}.diameterMm"));
        match (h.shape, h.slot_length_mm) {
            (HoleKind::Slot, Some(n)) => {
                self.positive(n, &format!("{p}.slotLengthMm"));
                if n < h.diameter_mm {
                    self.error(
                        "invalid_slot",
                        p,
                        "Slot length is smaller than its diameter",
                    );
                }
            }
            (HoleKind::Slot, None) => {
                self.error("missing_slot_length", p, "Slot requires slotLengthMm")
            }
            (HoleKind::Round, Some(_)) => {
                self.error("invalid_slot", p, "Round drill cannot carry slotLengthMm")
            }
            _ => {}
        }
        if let Some(v) = &h.offset {
            self.point(v, &format!("{p}.offset"));
        }
        if let Some(n) = h.rotation_deg {
            self.finite(n, &format!("{p}.rotationDeg"));
        }
    }
    fn connection(&mut self, c: &PadConnection, p: &str) {
        self.optional_nonnegative(c.thermal_gap_mm, &format!("{p}.thermalGapMm"));
        self.optional_positive(c.spoke_width_mm, &format!("{p}.spokeWidthMm"));
        if let Some(n) = c.spoke_angle_deg {
            self.finite(n, &format!("{p}.spokeAngleDeg"));
        }
    }
    fn rule_values(&mut self, r: &RuleValues, p: &str) {
        for (name, n) in [
            ("clearanceMm", r.clearance_mm),
            ("edgeClearanceMm", r.edge_clearance_mm),
        ] {
            self.nonnegative(n, &format!("{p}.{name}"));
        }
        self.optional_nonnegative(
            r.hole_to_hole_clearance_mm,
            &format!("{p}.holeToHoleClearanceMm"),
        );
        for (name, n) in [
            ("minTrackWidthMm", r.min_track_width_mm),
            ("preferredTrackWidthMm", r.preferred_track_width_mm),
            ("via.minDiameterMm", r.via.min_diameter_mm),
            ("via.preferredDiameterMm", r.via.preferred_diameter_mm),
            ("via.minDrillMm", r.via.min_drill_mm),
            ("via.preferredDrillMm", r.via.preferred_drill_mm),
        ] {
            self.positive(n, &format!("{p}.{name}"));
        }
        if r.preferred_track_width_mm < r.min_track_width_mm
            || r.via.preferred_diameter_mm < r.via.min_diameter_mm
            || r.via.preferred_drill_mm < r.via.min_drill_mm
        {
            self.error(
                "inconsistent_rules",
                p,
                "Preferred dimensions cannot be below minima",
            );
        }
        if r.via.preferred_drill_mm >= r.via.preferred_diameter_mm {
            self.error(
                "inconsistent_rules",
                p,
                "Preferred via drill must be smaller than preferred land diameter",
            );
        }
        for (name, ns) in [
            ("allowedLayers", &r.allowed_layers),
            ("impedanceReferenceLayers", &r.impedance_reference_layers),
        ] {
            if let Some(ls) = ns {
                self.layer_list(ls, &format!("{p}.{name}"));
            }
        }
        if let Some(n) = &r.impedance_reference_net {
            self.net(n, &format!("{p}.impedanceReferenceNet"));
        }
        for (name, n) in [
            ("impedanceOhm", r.impedance_ohm),
            ("calculatedImpedanceOhm", r.calculated_impedance_ohm),
        ] {
            self.optional_positive(n, &format!("{p}.{name}"));
        }
        self.optional_nonnegative(
            r.impedance_tolerance_percent,
            &format!("{p}.impedanceTolerancePercent"),
        );
        self.optional_nonnegative(
            r.impedance_coplanar_gap_mm,
            &format!("{p}.impedanceCoplanarGapMm"),
        );
        if let Some(d) = &r.differential {
            self.positive(d.track_width_mm, &format!("{p}.differential.trackWidthMm"));
            self.nonnegative(d.gap_mm, &format!("{p}.differential.gapMm"));
            self.optional_nonnegative(d.max_skew_mm, &format!("{p}.differential.maxSkewMm"));
            self.optional_nonnegative(
                d.max_uncoupled_length_mm,
                &format!("{p}.differential.maxUncoupledLengthMm"),
            );
        }
    }
    fn terminal(&mut self, t: &Terminal, net: &str, p: &str) {
        let found: Vec<_> = self
            .board
            .pads
            .iter()
            .filter(|v| v.component.as_deref() == Some(&t.component) && v.number == t.pad)
            .collect();
        if found.len() != 1 {
            self.error(
                "ambiguous_terminal",
                p,
                "Endpoint must resolve to exactly one component pad",
            );
        } else if found[0].net.as_deref() != Some(net) {
            self.error(
                "endpoint_net_mismatch",
                p,
                "Endpoint pad does not belong to endpoint net",
            );
        }
    }
}

pub(super) fn validate(b: &RoutingBoardV2) -> Vec<Diagnostic> {
    let mut c = Check {
        board: b,
        layers: b.layers.iter().map(|v| v.name.as_str()).collect(),
        nets: b.nets.iter().map(|v| v.name.as_str()).collect(),
        components: b.components.iter().map(|v| v.designator.as_str()).collect(),
        pads: b.pads.iter().map(|v| v.id.as_str()).collect(),
        diagnostics: vec![],
        source_id: None,
    };
    c.ring(&b.outline, "outline");
    for (i, r) in b.cutouts.iter().enumerate() {
        c.ring(r, &format!("cutouts[{i}]"));
    }
    if b.layers.is_empty() {
        c.error("empty_layers", "layers", "Board requires copper layers");
    }
    c.unique(b.layers.iter().map(|v| v.name.as_str()), "layers");
    for (i, l) in b.layers.iter().enumerate() {
        let p = format!("layers[{i}]");
        if i > 0 && b.layers[i - 1].index >= l.index {
            c.error("invalid_layer_order",&format!("{p}.index"),"Layer indices must strictly increase in input physical order; no sorting performed");
        }
        let side_ok = match l.side {
            Side::Top => i == 0,
            Side::Bottom => i + 1 == b.layers.len(),
            Side::Inner => i > 0 && i + 1 < b.layers.len(),
        };
        if !side_ok {
            c.error(
                "invalid_layer_side",
                &p,
                "Layer side contradicts physical order",
            );
        }
    }
    c.unique(b.nets.iter().map(|v| v.name.as_str()), "nets");
    c.unique(
        b.components.iter().map(|v| v.designator.as_str()),
        "components.designator",
    );
    c.unique(
        b.components.iter().filter_map(|v| v.id.as_deref()),
        "components.id",
    );
    c.unique(b.pads.iter().map(|v| v.id.as_str()), "pads.id");
    c.unique(
        b.keepouts.iter().filter_map(|v| v.id.as_deref()),
        "keepouts.id",
    );
    for (i, v) in b.components.iter().enumerate() {
        c.source_id = v.id.clone();
        let p = format!("components[{i}]");
        if v.id.is_none() {
            c.emit(
                Severity::Missing,
                "source_id_missing",
                &p,
                "Native ID absent; designator and input path retained",
            );
        }
        c.point(&v.at, &format!("{p}.at"));
        c.finite(v.rotation_deg, &format!("{p}.rotationDeg"));
        for (name, poly) in [
            ("bodyOutline", &v.body_outline),
            ("courtyardOutline", &v.courtyard_outline),
            ("bounds", &v.bounds),
        ] {
            if let Some(poly) = poly {
                c.polygon(poly, &format!("{p}.{name}"));
            }
        }
        if let Some(s) = &v.body_size_mm {
            for (k, n) in [("x", s.x), ("y", s.y), ("z", s.z)] {
                c.positive(n, &format!("{p}.bodySizeMm.{k}"));
            }
        }
        if let Some(part) = &v.part_uuid {
            match part {
                PartUuid::Lcsc(s) => c.text(s, &format!("{p}.partUuid")),
                PartUuid::Library(l) => {
                    c.text(&l.uuid, &format!("{p}.partUuid.uuid"));
                    c.text(&l.library_uuid, &format!("{p}.partUuid.libraryUuid"));
                }
            }
        }
    }
    for (i, v) in b.pads.iter().enumerate() {
        c.source_id = Some(v.id.clone());
        let p = format!("pads[{i}]");
        if let Some(component) = &v.component {
            if !c.components.contains(component.as_str()) {
                c.error(
                    "unknown_component",
                    &format!("{p}.component"),
                    format!("Unknown component {component:?}"),
                );
            }
        }
        c.point(&v.at, &format!("{p}.at"));
        c.finite(v.rotation_deg, &format!("{p}.rotationDeg"));
        c.layer_list(&v.layers, &format!("{p}.layers"));
        c.optional_net(&v.net, &format!("{p}.net"));
        c.shape(&v.shape, &format!("{p}.shape"));
        if let Some(h) = &v.hole {
            c.hole(h, &format!("{p}.hole"));
        }
        if let Some(shapes) = &v.shapes_by_layer {
            let shapes = &shapes.0;
            if shapes.keys().map(String::as_str).collect::<BTreeSet<_>>()
                != v.layers.iter().map(String::as_str).collect()
            {
                c.error(
                    "incomplete_layer_shapes",
                    &format!("{p}.shapesByLayer"),
                    "Shapes must cover exactly pad.layers",
                );
            }
            for (l, s) in shapes {
                c.layer(l, &format!("{p}.shapesByLayer.{l}"));
                c.shape(s, &format!("{p}.shapesByLayer.{l}"));
            }
        }
    }
    for (i, v) in b.keepouts.iter().enumerate() {
        c.source_id = v.id.clone();
        let p = format!("keepouts[{i}]");
        c.layer_list(&v.layers, &format!("{p}.layers"));
        c.polygon(&v.polygon, &format!("{p}.polygon"));
    }
    if let Some(holes) = &b.holes {
        c.unique(holes.iter().map(|h| h.id.as_str()), "holes.id");
        for (i, h) in holes.iter().enumerate() {
            c.source_id = Some(h.id.clone());
            let p = format!("holes[{i}]");
            c.point(&h.at, &format!("{p}.at"));
            c.hole(&h.hole, &format!("{p}.hole"));
        }
    }
    let mut primitive_ids: BTreeSet<String> = b.pads.iter().map(|p| p.id.clone()).collect();
    for (i, h) in b.holes.iter().flatten().enumerate() {
        c.source_id = Some(h.id.clone());
        check_id(
            &mut c,
            &mut primitive_ids,
            &Some(h.id.clone()),
            &format!("holes[{i}]"),
        );
    }
    for (i, k) in b.keepouts.iter().enumerate() {
        c.source_id = k.id.clone();
        check_id(&mut c, &mut primitive_ids, &k.id, &format!("keepouts[{i}]"));
    }
    for (scope, copper) in [("fixed", &b.copper.fixed), ("editable", &b.copper.editable)] {
        for (i, t) in copper.tracks.iter().enumerate() {
            c.source_id = t.id.clone();
            let p = format!("copper.{scope}.tracks[{i}]");
            check_id(&mut c, &mut primitive_ids, &t.id, &p);
            c.optional_net(&t.net, &format!("{p}.net"));
            c.layer(&t.layer, &format!("{p}.layer"));
            c.positive(t.width_mm, &format!("{p}.widthMm"));
            if t.points.len() < 2 {
                c.error(
                    "invalid_track",
                    &format!("{p}.points"),
                    "Track needs at least two points",
                );
            }
            for (j, v) in t.points.iter().enumerate() {
                c.point(v, &format!("{p}.points[{j}]"));
            }
            if let Some(ids) = &t.connected_pad_ids {
                c.unique(
                    ids.iter().map(String::as_str),
                    &format!("{p}.connectedPadIds"),
                );
                for id in ids {
                    if !c.pads.contains(id.as_str()) {
                        c.error(
                            "unknown_pad",
                            &format!("{p}.connectedPadIds"),
                            format!("Unknown pad {id:?}"),
                        );
                    }
                    if let Some(pad) = b.pads.iter().find(|p| p.id == *id) {
                        if let (Some(a), Some(b)) = (&t.net, &pad.net) {
                            if a != b {
                                c.error(
                                    "connected_pad_net_mismatch",
                                    &format!("{p}.connectedPadIds"),
                                    format!("Pad {id:?} belongs to a different net"),
                                );
                            }
                        }
                    }
                }
            }
        }
        for (i, v) in copper.vias.iter().enumerate() {
            c.source_id = v.id.clone();
            let p = format!("copper.{scope}.vias[{i}]");
            check_id(&mut c, &mut primitive_ids, &v.id, &p);
            c.optional_net(&v.net, &format!("{p}.net"));
            c.point(&v.at, &format!("{p}.at"));
            c.positive(v.diameter_mm, &format!("{p}.diameterMm"));
            c.positive(v.drill_mm, &format!("{p}.drillMm"));
            if v.drill_mm >= v.diameter_mm {
                c.error(
                    "invalid_via_dimensions",
                    &p,
                    "Via drill must be smaller than land diameter",
                );
            }
            c.layer(&v.from_layer, &format!("{p}.fromLayer"));
            c.layer(&v.to_layer, &format!("{p}.toLayer"));
            let from = b.layers.iter().position(|l| l.name == v.from_layer);
            let to = b.layers.iter().position(|l| l.name == v.to_layer);
            if let (Some(from), Some(to)) = (from, to) {
                if from >= to {
                    c.error("invalid_via_span",&p,"Via endpoints must increase in physical layer order; no reversal performed");
                }
                let full = from == 0 && to + 1 == b.layers.len();
                if (v.r#type == ViaKind::Through) != full {
                    c.error(
                        "inconsistent_via_type",
                        &p,
                        "Via type contradicts its explicit layer span",
                    );
                }
            }
        }
        for (i, z) in copper.zones.iter().enumerate() {
            let p = format!("copper.{scope}.zones[{i}]");
            let (id, net, ls, poly, fills) = match z {
                Zone::Pour {
                    id,
                    net,
                    layers,
                    outline,
                    filled_polygons,
                    ..
                }
                | Zone::Fill {
                    id,
                    net,
                    layers,
                    outline,
                    filled_polygons,
                } => (id, net, layers, outline, filled_polygons),
            };
            c.source_id = Some(id.clone());
            check_id(&mut c, &mut primitive_ids, &Some(id.clone()), &p);
            c.optional_net(net, &format!("{p}.net"));
            c.layer_list(ls, &format!("{p}.layers"));
            c.polygon(poly, &format!("{p}.outline"));
            let mut filled_layers = BTreeSet::new();
            for (j, f) in fills.iter().enumerate() {
                let fp = format!("{p}.filledPolygons[{j}]");
                let (l, ids) = match f {
                    FilledPolygons::Available {
                        layer,
                        native_primitive_ids,
                        polygons,
                        freshness,
                        ..
                    } => {
                        for (k, poly) in polygons.iter().enumerate() {
                            c.polygon(poly, &format!("{fp}.polygons[{k}]"));
                        }
                        if *freshness != Freshness::Current {
                            c.emit(Severity::Limitation,"fill_not_current",&fp,format!("CAD fill freshness is {freshness:?}; retained, not certified current"));
                        }
                        (layer, native_primitive_ids)
                    }
                    FilledPolygons::Missing {
                        layer,
                        native_primitive_ids,
                        reason,
                        ..
                    }
                    | FilledPolygons::Failed {
                        layer,
                        native_primitive_ids,
                        reason,
                        ..
                    }
                    | FilledPolygons::Unsupported {
                        layer,
                        native_primitive_ids,
                        reason,
                        ..
                    } => {
                        c.text(reason, &format!("{fp}.reason"));
                        c.emit(
                            Severity::Missing,
                            "cad_fill_unavailable",
                            &fp,
                            format!("CAD fill unavailable: {reason}; boundary is not copper"),
                        );
                        (layer, native_primitive_ids)
                    }
                };
                c.layer(l, &format!("{fp}.layer"));
                if !ls.contains(l) {
                    c.error(
                        "fill_layer_mismatch",
                        &fp,
                        "Fill layer is not in zone.layers",
                    );
                }
                if !filled_layers.insert(l.as_str()) {
                    c.error(
                        "duplicate_fill_layer",
                        &fp,
                        "One fill state per zone layer is required",
                    );
                }
                if let Some(ids) = ids {
                    c.unique(
                        ids.iter().map(String::as_str),
                        &format!("{fp}.nativePrimitiveIds"),
                    );
                }
            }
            for l in ls {
                if !filled_layers.contains(l.as_str()) {
                    c.error("missing_fill_state",&format!("{p}.filledPolygons"),format!("Layer {l:?} requires an explicit available/missing/failed/unsupported fill state"));
                }
            }
            if let Zone::Pour {
                priority,
                refill_rules: r,
                ..
            } = z
            {
                if let Some(n) = priority {
                    c.finite(*n, &format!("{p}.priority"));
                }
                c.optional_nonnegative(r.clearance_mm, &format!("{p}.refillRules.clearanceMm"));
                if let Some(pc) = &r.pad_connection {
                    for (name, v) in [
                        ("singleLayer", &pc.single_layer),
                        ("multiLayer", &pc.multi_layer),
                    ] {
                        if let Some(v) = v {
                            c.connection(v, &format!("{p}.refillRules.padConnection.{name}"));
                        }
                    }
                }
                c.emit(
                    Severity::Limitation,
                    "incomplete_refill_rules",
                    &format!("{p}.refillRules"),
                    format!(
                        "Refill rules {:?}; unresolved: {}. No refill performed",
                        r.status,
                        r.unresolved.join(", ")
                    ),
                );
            }
        }
    }
    c.source_id = None;
    if b.holes.is_none() {
        c.emit(
            Severity::Missing,
            "independent_drills_unspecified",
            "holes",
            "Independent-drill data is absent; not replaced by a verified empty list",
        );
    }
    validate_rules(&mut c, &b.rules);
    validate_stackup(&mut c, b.stackup.as_ref());
    c.emit(Severity::Limitation,"geometry_not_prepared","$","Structural import only: polygon intersections/containment, copper contacts, shorts and overlaps are not yet certified; no mesh or physical result");
    c.emit(Severity::Missing,"physical_models_not_supplied","$","RoutingBoardV2 does not supply conductivity, component electrical/thermal models or analysis boundary conditions");
    if b.components.iter().any(|v| v.body_size_mm.is_none()) {
        c.emit(
            Severity::Missing,
            "component_body_size_missing",
            "components",
            "Some component body dimensions are absent; footprint bounds are not body dimensions",
        );
    }
    c.diagnostics
}
fn check_id(c: &mut Check<'_>, seen: &mut BTreeSet<String>, id: &Option<String>, p: &str) {
    if let Some(id) = id {
        c.text(id, &format!("{p}.id"));
        if !seen.insert(id.clone()) {
            c.error(
                "duplicate_identifier",
                &format!("{p}.id"),
                "Primitive ID repeats across pads, drills, keepouts or copper objects",
            );
        }
    } else {
        c.emit(
            Severity::Missing,
            "source_id_missing",
            p,
            "Native ID absent; input path retained as provenance",
        );
    }
}

fn validate_rules(c: &mut Check<'_>, r: &Rules) {
    c.rule_values(&r.default, "rules.default");
    c.unique(r.nets.iter().map(|v| v.net.as_str()), "rules.nets");
    for (i, v) in r.nets.iter().enumerate() {
        let p = format!("rules.nets[{i}]");
        c.net(&v.net, &format!("{p}.net"));
        c.rule_values(&v.values, &format!("{p}.values"));
    }
    if let Some(classes) = &r.net_classes {
        c.unique(classes.iter().map(|v| v.name.as_str()), "rules.netClasses");
        for (i, v) in classes.iter().enumerate() {
            let p = format!("rules.netClasses[{i}]");
            c.unique(v.nets.iter().map(String::as_str), &format!("{p}.nets"));
            for n in &v.nets {
                c.net(n, &format!("{p}.nets"));
            }
            c.rule_values(&v.values, &format!("{p}.values"));
        }
    }
    if let Some(pairs) = &r.differential_pairs {
        c.unique(
            pairs.iter().map(|v| v.id.as_str()),
            "rules.differentialPairs",
        );
        for (i, v) in pairs.iter().enumerate() {
            let p = format!("rules.differentialPairs[{i}]");
            c.net(&v.positive, &format!("{p}.positive"));
            c.net(&v.negative, &format!("{p}.negative"));
            if v.positive == v.negative {
                c.error(
                    "invalid_differential_pair",
                    &p,
                    "Pair must contain distinct nets",
                );
            }
        }
    }
    if let Some(groups) = &r.matched_groups {
        c.unique(groups.iter().map(|v| v.id.as_str()), "rules.matchedGroups");
        let names: BTreeMap<_, _> = groups
            .iter()
            .enumerate()
            .map(|(i, v)| (v.id.as_str(), i))
            .collect();
        for (i, g) in groups.iter().enumerate() {
            let p = format!("rules.matchedGroups[{i}]");
            c.unique(g.nets.iter().map(String::as_str), &format!("{p}.nets"));
            for n in &g.nets {
                c.net(n, &format!("{p}.nets"));
            }
            c.optional_nonnegative(g.tolerance_mm, &format!("{p}.toleranceMm"));
            c.optional_nonnegative(g.target_mm, &format!("{p}.targetMm"));
            if let Some(d) = &g.delay {
                c.nonnegative(d.tolerance_ps, &format!("{p}.delay.tolerancePs"));
                c.optional_nonnegative(d.target_ps, &format!("{p}.delay.targetPs"));
                c.net(&d.reference_net, &format!("{p}.delay.referenceNet"));
            }
            if let Some(f) = &g.follow {
                c.finite(f.offset, &format!("{p}.follow.offset"));
                if !names.contains_key(f.group.as_str()) {
                    c.error(
                        "unknown_matched_group",
                        &format!("{p}.follow.group"),
                        "Unknown matched group",
                    );
                }
                let mut seen = BTreeSet::from([g.id.as_str()]);
                let mut next = Some(f.group.as_str());
                while let Some(n) = next {
                    if !seen.insert(n) {
                        c.error(
                            "cyclic_matched_group",
                            &format!("{p}.follow"),
                            "Follow references form a cycle",
                        );
                        break;
                    }
                    next = names
                        .get(n)
                        .and_then(|j| groups[*j].follow.as_ref())
                        .map(|f| f.group.as_str());
                }
            }
            if let Some(es) = &g.endpoints {
                for (j, e) in es.iter().enumerate() {
                    let p = format!("{p}.endpoints[{j}]");
                    c.net(&e.net, &format!("{p}.net"));
                    if !g.nets.contains(&e.net) {
                        c.error(
                            "endpoint_net_mismatch",
                            &p,
                            "Endpoint net is outside its matched group",
                        );
                    }
                    c.terminal(&e.from, &e.net, &format!("{p}.from"));
                    c.terminal(&e.to, &e.net, &format!("{p}.to"));
                }
            }
        }
    }
}
fn validate_stackup(c: &mut Check<'_>, s: Option<&Stackup>) {
    let Some(s) = s else {
        c.emit(Severity::Missing,"stackup_missing","stackup","Stackup, thicknesses, dielectric properties and via plating are absent; no defaults inferred");
        return;
    };
    c.optional_positive(s.board_thickness_mm, "stackup.boardThicknessMm");
    c.optional_positive(
        s.fallback_copper_thickness_oz,
        "stackup.fallbackCopperThicknessOz",
    );
    c.optional_positive(s.via_plating_thickness_um, "stackup.viaPlatingThicknessUm");
    if s.via_plating_thickness_um.is_none() {
        c.emit(
            Severity::Missing,
            "via_plating_missing",
            "stackup.viaPlatingThicknessUm",
            "No via plating thickness supplied",
        );
    }
    let mut order = vec![];
    let mut total = 0.0;
    for (i, l) in s.layers.iter().enumerate() {
        let p = format!("stackup.layers[{i}]");
        match l {
            StackLayer::Copper {
                layer,
                thickness_mm,
            } => {
                c.layer(layer, &format!("{p}.layer"));
                c.positive(*thickness_mm, &format!("{p}.thicknessMm"));
                total += thickness_mm;
                order.push(layer.as_str());
            }
            StackLayer::Dielectric {
                thickness_mm,
                relative_permittivity,
                loss_tangent,
                material,
                ..
            } => {
                c.positive(*thickness_mm, &format!("{p}.thicknessMm"));
                total += thickness_mm;
                c.optional_positive(*relative_permittivity, &format!("{p}.relativePermittivity"));
                c.optional_nonnegative(*loss_tangent, &format!("{p}.lossTangent"));
                if relative_permittivity.is_none() || loss_tangent.is_none() || material.is_none() {
                    c.emit(
                        Severity::Missing,
                        "dielectric_properties_missing",
                        &p,
                        "Dielectric properties/material are incomplete; not inferred",
                    );
                }
            }
        }
    }
    c.finite(total, "stackup.layers");
    if order
        != c.board
            .layers
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>()
    {
        c.error(
            "inconsistent_stackup",
            "stackup.layers",
            "Stackup copper must cover every board layer once in physical order",
        );
    }
    if let Some(thickness) = s.board_thickness_mm {
        if (total - thickness).abs() > 1e-9 * total.abs().max(thickness.abs()).max(1.0) {
            c.error(
                "inconsistent_stackup_thickness",
                "stackup.boardThicknessMm",
                format!("Layer thickness sum {total} differs from board thickness {thickness}"),
            );
        }
    }
    if let Some(mask) = &s.solder_mask {
        for (side, m) in [("top", &mask.top), ("bottom", &mask.bottom)] {
            if let Some(m) = m {
                c.optional_positive(
                    m.thickness_mm,
                    &format!("stackup.solderMask.{side}.thicknessMm"),
                );
                c.optional_positive(
                    m.relative_permittivity,
                    &format!("stackup.solderMask.{side}.relativePermittivity"),
                );
            }
        }
    }
    c.emit(
        Severity::Limitation,
        "mask_geometry_unavailable",
        "stackup.solderMask",
        "V2 has no solder-mask opening geometry; scalar properties do not define coverage",
    );
}
