//! Called only after validation, so name lookups are total and never fill gaps.
use super::dto as d;
use crate::model as m;
use std::collections::BTreeMap;

fn source(path: String, id: Option<String>) -> m::Source {
    m::Source { path, id }
}
fn point(p: d::Point) -> m::Point {
    m::Point { x: p.x, y: p.y }
}
fn ring(p: Vec<d::Point>) -> Vec<m::Point> {
    p.into_iter().map(point).collect()
}
fn polygon(p: d::Polygon) -> m::Polygon {
    m::Polygon {
        outer: ring(p.outer),
        holes: p.holes.unwrap_or_default().into_iter().map(ring).collect(),
    }
}
fn shape(s: d::PadShape) -> m::PadShape {
    match s {
        d::PadShape::Circle { diameter_mm } => m::PadShape::Circle { diameter_mm },
        d::PadShape::Rect {
            width_mm,
            height_mm,
        } => m::PadShape::Rect {
            width_mm,
            height_mm,
        },
        d::PadShape::RoundRect {
            width_mm,
            height_mm,
            corner_radius_mm,
        } => m::PadShape::RoundRect {
            width_mm,
            height_mm,
            corner_radius_mm,
        },
        d::PadShape::Oval {
            width_mm,
            height_mm,
        } => m::PadShape::Oval {
            width_mm,
            height_mm,
        },
        d::PadShape::Polygon { polygon: p } => m::PadShape::Polygon(polygon(p)),
    }
}
fn drill(h: d::Hole) -> m::Drill {
    m::Drill {
        shape: match h.shape {
            d::HoleKind::Round => m::DrillShape::Round,
            d::HoleKind::Slot => m::DrillShape::Slot {
                length_mm: h.slot_length_mm.expect("validated slot length"),
            },
        },
        diameter_mm: h.diameter_mm,
        offset: h.offset.map(point),
        rotation_deg: h.rotation_deg,
        plated: h.plated,
    }
}
fn connection(c: d::PadConnection) -> m::PadConnection {
    m::PadConnection {
        mode: match c.mode {
            d::Connection::Solid => m::ConnectionMode::Solid,
            d::Connection::Thermal => m::ConnectionMode::Thermal,
            d::Connection::None => m::ConnectionMode::None,
        },
        thermal_gap_mm: c.thermal_gap_mm,
        spoke_width_mm: c.spoke_width_mm,
        spoke_angle_deg: c.spoke_angle_deg,
    }
}
struct Lookup {
    layers: BTreeMap<String, usize>,
    nets: BTreeMap<String, usize>,
    components: BTreeMap<String, usize>,
    pads: BTreeMap<String, usize>,
    terminals: BTreeMap<(String, String), usize>,
    groups: BTreeMap<String, usize>,
}
impl Lookup {
    fn layers(&self, ls: Vec<String>) -> Vec<usize> {
        ls.into_iter().map(|l| self.layers[&l]).collect()
    }
    fn net(&self, n: Option<String>) -> Option<usize> {
        n.map(|n| self.nets[&n])
    }
    fn rules(&self, r: d::RuleValues) -> m::RuleValues {
        m::RuleValues {
            clearance_mm: r.clearance_mm,
            edge_clearance_mm: r.edge_clearance_mm,
            hole_to_hole_clearance_mm: r.hole_to_hole_clearance_mm,
            min_track_width_mm: r.min_track_width_mm,
            preferred_track_width_mm: r.preferred_track_width_mm,
            via: m::ViaRules {
                min_diameter_mm: r.via.min_diameter_mm,
                preferred_diameter_mm: r.via.preferred_diameter_mm,
                min_drill_mm: r.via.min_drill_mm,
                preferred_drill_mm: r.via.preferred_drill_mm,
            },
            allowed_layers: r.allowed_layers.map(|v| self.layers(v)),
            impedance_ohm: r.impedance_ohm,
            impedance_tolerance_percent: r.impedance_tolerance_percent,
            impedance_topology: r.impedance_topology.map(|t| match t {
                d::Topology::Microstrip => m::ImpedanceTopology::Microstrip,
                d::Topology::Stripline => m::ImpedanceTopology::Stripline,
                d::Topology::Coplanar => m::ImpedanceTopology::Coplanar,
                d::Topology::GroundedCoplanar => m::ImpedanceTopology::GroundedCoplanar,
            }),
            impedance_reference_net: self.net(r.impedance_reference_net),
            impedance_reference_layers: r.impedance_reference_layers.map(|v| self.layers(v)),
            impedance_coplanar_gap_mm: r.impedance_coplanar_gap_mm,
            calculated_impedance_ohm: r.calculated_impedance_ohm,
            differential: r.differential.map(|d| m::DifferentialRules {
                track_width_mm: d.track_width_mm,
                gap_mm: d.gap_mm,
                max_skew_mm: d.max_skew_mm,
                max_uncoupled_length_mm: d.max_uncoupled_length_mm,
            }),
        }
    }
    fn fill(&self, f: d::FilledPolygons, path: String) -> m::CadFill {
        let (layer, native_ids, state) = match f {
            d::FilledPolygons::Available {
                layer,
                native_primitive_ids,
                polygons,
                freshness,
                ..
            } => (
                layer,
                native_primitive_ids,
                m::CadFillState::Available {
                    polygons: polygons.into_iter().map(polygon).collect(),
                    freshness: match freshness {
                        d::Freshness::Current => m::Freshness::Current,
                        d::Freshness::Stale => m::Freshness::Stale,
                        d::Freshness::Unknown => m::Freshness::Unknown,
                    },
                },
            ),
            d::FilledPolygons::Missing {
                layer,
                native_primitive_ids,
                reason,
                ..
            } => (
                layer,
                native_primitive_ids,
                m::CadFillState::Unavailable {
                    state: m::Unavailable::Missing,
                    reason,
                },
            ),
            d::FilledPolygons::Failed {
                layer,
                native_primitive_ids,
                reason,
                ..
            } => (
                layer,
                native_primitive_ids,
                m::CadFillState::Unavailable {
                    state: m::Unavailable::Failed,
                    reason,
                },
            ),
            d::FilledPolygons::Unsupported {
                layer,
                native_primitive_ids,
                reason,
                ..
            } => (
                layer,
                native_primitive_ids,
                m::CadFillState::Unavailable {
                    state: m::Unavailable::Unsupported,
                    reason,
                },
            ),
        };
        m::CadFill {
            source: source(path, None),
            layer: self.layers[&layer],
            native_ids,
            state,
        }
    }
    fn copper(&self, c: d::Copper, scope: &str, ownership: m::Ownership, b: &mut m::Board) {
        b.tracks
            .extend(c.tracks.into_iter().enumerate().map(|(i, t)| {
                m::Track {
                    source: source(format!("copper.{scope}.tracks[{i}]"), t.id),
                    ownership,
                    net: self.net(t.net),
                    layer: self.layers[&t.layer],
                    width_mm: t.width_mm,
                    points: ring(t.points),
                    connected_pads: t
                        .connected_pad_ids
                        .map(|v| v.into_iter().map(|id| self.pads[&id]).collect()),
                }
            }));
        b.vias
            .extend(c.vias.into_iter().enumerate().map(|(i, v)| m::Via {
                source: source(format!("copper.{scope}.vias[{i}]"), v.id),
                ownership,
                net: self.net(v.net),
                at: point(v.at),
                diameter_mm: v.diameter_mm,
                drill_mm: v.drill_mm,
                first_layer: self.layers[&v.from_layer],
                last_layer: self.layers[&v.to_layer],
                kind: match v.r#type {
                    d::ViaKind::Through => m::ViaKind::Through,
                    d::ViaKind::BlindBuried => m::ViaKind::BlindBuried,
                },
            }));
        b.zones
            .extend(c.zones.into_iter().enumerate().map(|(i, z)| {
                let p = format!("copper.{scope}.zones[{i}]");
                let (id, net, layers, outline, filled_polygons, kind) = match z {
                    d::Zone::Fill {
                        id,
                        net,
                        layers,
                        outline,
                        filled_polygons,
                    } => (id, net, layers, outline, filled_polygons, m::ZoneKind::Fill),
                    d::Zone::Pour {
                        id,
                        net,
                        layers,
                        outline,
                        priority,
                        preserve_islands,
                        filled_polygons,
                        refill_rules: r,
                    } => {
                        let pc = r.pad_connection;
                        let (single_layer, multi_layer) = pc
                            .map(|p| {
                                (
                                    p.single_layer.map(connection),
                                    p.multi_layer.map(connection),
                                )
                            })
                            .unwrap_or((None, None));
                        (
                            id,
                            net,
                            layers,
                            outline,
                            filled_polygons,
                            m::ZoneKind::Pour {
                                priority,
                                preserve_islands,
                                refill: m::RefillRules {
                                    clearance_mm: r.clearance_mm,
                                    single_layer,
                                    multi_layer,
                                    partial: r.status == d::RefillStatus::Partial,
                                    unresolved: r.unresolved,
                                },
                            },
                        )
                    }
                };
                m::Zone {
                    source: source(p.clone(), Some(id)),
                    ownership,
                    net: self.net(net),
                    layers: self.layers(layers),
                    boundary: polygon(outline),
                    kind,
                    fills: filled_polygons
                        .into_iter()
                        .enumerate()
                        .map(|(j, f)| self.fill(f, format!("{p}.filledPolygons[{j}]")))
                        .collect(),
                }
            }));
    }
}
pub(super) fn convert(b: d::RoutingBoardV2) -> m::Board {
    let l = Lookup {
        layers: b
            .layers
            .iter()
            .enumerate()
            .map(|(i, v)| (v.name.clone(), i))
            .collect(),
        nets: b
            .nets
            .iter()
            .enumerate()
            .map(|(i, v)| (v.name.clone(), i))
            .collect(),
        components: b
            .components
            .iter()
            .enumerate()
            .map(|(i, v)| (v.designator.clone(), i))
            .collect(),
        pads: b
            .pads
            .iter()
            .enumerate()
            .map(|(i, v)| (v.id.clone(), i))
            .collect(),
        terminals: b
            .pads
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                v.component
                    .as_ref()
                    .map(|s| ((s.clone(), v.number.clone()), i))
            })
            .collect(),
        groups: b
            .rules
            .matched_groups
            .iter()
            .flatten()
            .enumerate()
            .map(|(i, v)| (v.id.clone(), i))
            .collect(),
    };
    let rules = m::Rules {
        default: l.rules(b.rules.default),
        nets: b
            .rules
            .nets
            .into_iter()
            .map(|r| (l.nets[&r.net], l.rules(r.values)))
            .collect(),
        net_classes: b
            .rules
            .net_classes
            .unwrap_or_default()
            .into_iter()
            .map(|r| m::NetClass {
                name: r.name,
                nets: r.nets.into_iter().map(|n| l.nets[&n]).collect(),
                values: l.rules(r.values),
            })
            .collect(),
        differential_pairs: b
            .rules
            .differential_pairs
            .unwrap_or_default()
            .into_iter()
            .map(|r| m::DifferentialPair {
                id: r.id,
                positive: l.nets[&r.positive],
                negative: l.nets[&r.negative],
            })
            .collect(),
        matched_groups: b
            .rules
            .matched_groups
            .unwrap_or_default()
            .into_iter()
            .map(|r| m::MatchedGroup {
                id: r.id,
                nets: r.nets.into_iter().map(|n| l.nets[&n]).collect(),
                tolerance_mm: r.tolerance_mm,
                target_mm: r.target_mm,
                delay: r.delay.map(|d| m::Delay {
                    tolerance_ps: d.tolerance_ps,
                    target_ps: d.target_ps,
                    reference_net: l.nets[&d.reference_net],
                }),
                follow: r.follow.map(|f| (l.groups[&f.group], f.offset)),
                endpoints: r.endpoints.map(|es| {
                    es.into_iter()
                        .map(|e| m::Endpoints {
                            net: l.nets[&e.net],
                            from_pad: l.terminals[&(e.from.component, e.from.pad)],
                            to_pad: l.terminals[&(e.to.component, e.to.pad)],
                        })
                        .collect()
                }),
            })
            .collect(),
    };
    let mut result = m::Board {
        outline: ring(b.outline),
        cutouts: b.cutouts.into_iter().map(ring).collect(),
        layers: b
            .layers
            .into_iter()
            .map(|v| m::Layer {
                name: v.name,
                physical_index: v.index,
                side: match v.side {
                    d::Side::Top => m::Side::Top,
                    d::Side::Inner => m::Side::Inner,
                    d::Side::Bottom => m::Side::Bottom,
                },
                disable_routing: v.disable_routing,
            })
            .collect(),
        nets: b.nets.into_iter().map(|v| v.name).collect(),
        components: b
            .components
            .into_iter()
            .enumerate()
            .map(|(i, v)| m::Component {
                source: source(format!("components[{i}]"), v.id),
                designator: v.designator,
                at: point(v.at),
                rotation_deg: v.rotation_deg,
                side: match v.side {
                    d::ComponentSide::Top => m::Side::Top,
                    d::ComponentSide::Bottom => m::Side::Bottom,
                },
                body_outline: v.body_outline.map(polygon),
                courtyard_outline: v.courtyard_outline.map(polygon),
                bounds: v.bounds.map(polygon),
                part: v.part_uuid.map(|p| match p {
                    d::PartUuid::Lcsc(s) => m::Part::Lcsc(s),
                    d::PartUuid::Library(p) => m::Part::Library {
                        uuid: p.uuid,
                        library_uuid: p.library_uuid,
                    },
                }),
                value: v.value,
                footprint: v.footprint_name,
                package: v.package_name,
                body_size_mm: v.body_size_mm.map(|s| [s.x, s.y, s.z]),
            })
            .collect(),
        pads: b
            .pads
            .into_iter()
            .enumerate()
            .map(|(i, v)| m::Pad {
                source: source(format!("pads[{i}]"), Some(v.id)),
                component: v.component.map(|s| l.components[&s]),
                number: v.number,
                net: l.net(v.net),
                at: point(v.at),
                rotation_deg: v.rotation_deg,
                layers: l.layers(v.layers),
                common_shape: shape(v.shape),
                layer_shapes: v.shapes_by_layer.map(|ss| {
                    ss.0.into_iter()
                        .map(|(name, s)| (l.layers[&name], shape(s)))
                        .collect()
                }),
                drill: v.hole.map(drill),
            })
            .collect(),
        keepouts: b
            .keepouts
            .into_iter()
            .enumerate()
            .map(|(i, v)| m::Keepout {
                source: source(format!("keepouts[{i}]"), v.id),
                layers: l.layers(v.layers),
                polygon: polygon(v.polygon),
                forbid: m::Forbid {
                    tracks: v.forbid.tracks,
                    vias: v.forbid.vias,
                    zones: v.forbid.zones,
                },
            })
            .collect(),
        drills: b.holes.map(|holes| {
            holes
                .into_iter()
                .enumerate()
                .map(|(i, v)| m::IndependentDrill {
                    source: source(format!("holes[{i}]"), Some(v.id)),
                    at: point(v.at),
                    drill: drill(v.hole),
                })
                .collect()
        }),
        tracks: vec![],
        vias: vec![],
        zones: vec![],
        rules,
        stackup: b.stackup.map(|s| {
            let mask = |m: d::MaskLayer| m::Mask {
                thickness_mm: m.thickness_mm,
                relative_permittivity: m.relative_permittivity,
            };
            let (top_mask, bottom_mask) = s
                .solder_mask
                .map(|s| (s.top.map(mask), s.bottom.map(mask)))
                .unwrap_or((None, None));
            m::Stackup {
                board_thickness_mm: s.board_thickness_mm,
                fallback_copper_thickness_oz: s.fallback_copper_thickness_oz,
                via_plating_thickness_um: s.via_plating_thickness_um,
                layers: s
                    .layers
                    .into_iter()
                    .map(|s| match s {
                        d::StackLayer::Copper {
                            layer,
                            thickness_mm,
                        } => m::StackLayer::Copper {
                            layer: l.layers[&layer],
                            thickness_mm,
                        },
                        d::StackLayer::Dielectric {
                            name,
                            thickness_mm,
                            relative_permittivity,
                            loss_tangent,
                            material,
                        } => m::StackLayer::Dielectric {
                            name,
                            thickness_mm,
                            relative_permittivity,
                            loss_tangent,
                            material,
                        },
                    })
                    .collect(),
                top_mask,
                bottom_mask,
            }
        }),
    };
    l.copper(b.copper.fixed, "fixed", m::Ownership::Fixed, &mut result);
    l.copper(
        b.copper.editable,
        "editable",
        m::Ownership::Editable,
        &mut result,
    );
    result
}
