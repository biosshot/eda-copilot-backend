//! Wire types matching src/types/pcb/routing-board-v2.ts, never solver data.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Reject added fields instead of silently discarding new transport semantics.
macro_rules! dto {
    (@fields $name:ident [$($fields:tt)*]) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name { $($fields)* }
    };
    (@fields $name:ident [$($fields:tt)*] , $($rest:tt)*) => {
        dto!(@fields $name [$($fields)*] $($rest)*);
    };
    (@fields $name:ident [$($fields:tt)*] $field:ident : Option<$ty:ty>, $($rest:tt)*) => {
        dto!(@fields $name [$($fields)*
            #[serde(default, deserialize_with = "optional", skip_serializing_if = "Option::is_none")]
            pub $field: Option<$ty>,
        ] $($rest)*);
    };
    (@fields $name:ident [$($fields:tt)*] $field:ident : $ty:ty, $($rest:tt)*) => {
        dto!(@fields $name [$($fields)* pub $field: $ty,] $($rest)*);
    };
    ($name:ident { $($fields:tt)* }) => {
        dto!(@fields $name [] $($fields)* ,);
    };
}
// TS optional means omitted, never explicit null (especially shapesByLayer).
fn optional<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
macro_rules! choice {
    ($name:ident { $($variant:ident => $wire:literal),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $wire)] $variant),* }
    };
}
choice!(Side { Top => "top", Inner => "inner", Bottom => "bottom" });
choice!(ComponentSide { Top => "top", Bottom => "bottom" });
choice!(ViaKind { Through => "through", BlindBuried => "blind-buried" });
choice!(Connection { Solid => "solid", Thermal => "thermal", None => "none" });
choice!(RefillStatus { Partial => "partial", Unknown => "unknown" });
choice!(CadSource { Cad => "cad" });
choice!(Freshness { Current => "current", Stale => "stale", Unknown => "unknown" });
choice!(Topology {
    Microstrip => "microstrip", Stripline => "stripline",
    Coplanar => "coplanar-waveguide", GroundedCoplanar => "grounded-coplanar-waveguide"
});
dto!(Point { x: f64, y: f64 });
dto!(Size3 {
    x: f64,
    y: f64,
    z: f64
});
dto!(Polygon { outer: Vec<Point>, holes: Option<Vec<Vec<Point>>> });
dto!(Layer { name: String, index: usize, side: Side, disable_routing: Option<bool> });
dto!(Net { name: String });
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PartUuid {
    Lcsc(String),
    Library(LibraryPart),
}
dto!(LibraryPart {
    uuid: String,
    library_uuid: String
});
dto!(Component {
    id: Option<String>, designator: String, at: Point, rotation_deg: f64,
    side: ComponentSide, body_outline: Option<Polygon>, courtyard_outline: Option<Polygon>,
    bounds: Option<Polygon>, part_uuid: Option<PartUuid>, value: Option<String>,
    footprint_name: Option<String>, package_name: Option<String>, body_size_mm: Option<Size3>
});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PadShape {
    Circle {
        #[serde(rename = "diameterMm")]
        diameter_mm: f64,
    },
    Rect {
        #[serde(rename = "widthMm")]
        width_mm: f64,
        #[serde(rename = "heightMm")]
        height_mm: f64,
    },
    RoundRect {
        #[serde(rename = "widthMm")]
        width_mm: f64,
        #[serde(rename = "heightMm")]
        height_mm: f64,
        #[serde(rename = "cornerRadiusMm")]
        corner_radius_mm: f64,
    },
    Oval {
        #[serde(rename = "widthMm")]
        width_mm: f64,
        #[serde(rename = "heightMm")]
        height_mm: f64,
    },
    Polygon {
        polygon: Polygon,
    },
}
choice!(HoleKind { Round => "round", Slot => "slot" });
dto!(Hole {
    shape: HoleKind, diameter_mm: f64, slot_length_mm: Option<f64>,
    offset: Option<Point>, rotation_deg: Option<f64>, plated: bool
});
dto!(Drill {
    id: String,
    at: Point,
    hole: Hole
});
dto!(Pad {
    id: String, component: Option<String>, number: String, net: Option<String>, at: Point,
    rotation_deg: f64, layers: Vec<String>, shape: PadShape, hole: Option<Hole>,
    shapes_by_layer: Option<LayerShapes>
});
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct LayerShapes(pub BTreeMap<String, PadShape>);
impl<'de> Deserialize<'de> for LayerShapes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = LayerShapes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map with one shape per layer and no duplicate layer keys")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut shapes = BTreeMap::new();
                while let Some((key, value)) = a.next_entry::<String, PadShape>()? {
                    if shapes.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "Duplicate layer shape {key:?}"
                        )));
                    }
                }
                Ok(LayerShapes(shapes))
            }
        }
        d.deserialize_map(Visitor)
    }
}
dto!(Forbid {
    tracks: bool,
    vias: bool,
    zones: bool
});
dto!(Keepout { id: Option<String>, layers: Vec<String>, polygon: Polygon, forbid: Forbid });
dto!(Track {
    id: Option<String>, net: Option<String>, layer: String, width_mm: f64,
    points: Vec<Point>, connected_pad_ids: Option<Vec<String>>
});
dto!(Via {
    id: Option<String>, net: Option<String>, at: Point, diameter_mm: f64,
    drill_mm: f64, from_layer: String, to_layer: String, r#type: ViaKind
});
// Status payload is tagged separately so unavailable fill cannot contain polygons/freshness.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
pub enum FilledPolygons {
    Available {
        layer: String,
        source: CadSource,
        #[serde(
            rename = "nativePrimitiveIds",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        native_primitive_ids: Option<Vec<String>>,
        polygons: Vec<Polygon>,
        freshness: Freshness,
    },
    Missing {
        layer: String,
        source: CadSource,
        #[serde(
            rename = "nativePrimitiveIds",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        native_primitive_ids: Option<Vec<String>>,
        reason: String,
    },
    Failed {
        layer: String,
        source: CadSource,
        #[serde(
            rename = "nativePrimitiveIds",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        native_primitive_ids: Option<Vec<String>>,
        reason: String,
    },
    Unsupported {
        layer: String,
        source: CadSource,
        #[serde(
            rename = "nativePrimitiveIds",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        native_primitive_ids: Option<Vec<String>>,
        reason: String,
    },
}
dto!(PadConnection { mode: Connection, thermal_gap_mm: Option<f64>, spoke_width_mm: Option<f64>, spoke_angle_deg: Option<f64> });
dto!(PourConnections { single_layer: Option<PadConnection>, multi_layer: Option<PadConnection> });
dto!(RefillRules {
    clearance_mm: Option<f64>, pad_connection: Option<PourConnections>,
    status: RefillStatus, unresolved: Vec<String>
});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Zone {
    Pour {
        id: String,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        net: Option<String>,
        layers: Vec<String>,
        outline: Polygon,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        priority: Option<f64>,
        #[serde(
            rename = "preserveIslands",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        preserve_islands: Option<bool>,
        #[serde(rename = "filledPolygons")]
        filled_polygons: Vec<FilledPolygons>,
        #[serde(rename = "refillRules")]
        refill_rules: RefillRules,
    },
    Fill {
        id: String,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        net: Option<String>,
        layers: Vec<String>,
        outline: Polygon,
        #[serde(rename = "filledPolygons")]
        filled_polygons: Vec<FilledPolygons>,
    },
}
dto!(Copper { tracks: Vec<Track>, vias: Vec<Via>, zones: Vec<Zone> });
dto!(CopperGroups {
    fixed: Copper,
    editable: Copper
});
dto!(ViaRules {
    min_diameter_mm: f64,
    preferred_diameter_mm: f64,
    min_drill_mm: f64,
    preferred_drill_mm: f64
});
dto!(DifferentialRules { track_width_mm: f64, gap_mm: f64, max_skew_mm: Option<f64>, max_uncoupled_length_mm: Option<f64> });
dto!(RuleValues {
    clearance_mm: f64, edge_clearance_mm: f64, hole_to_hole_clearance_mm: Option<f64>,
    min_track_width_mm: f64, preferred_track_width_mm: f64, via: ViaRules,
    allowed_layers: Option<Vec<String>>, impedance_ohm: Option<f64>, impedance_tolerance_percent: Option<f64>,
    impedance_topology: Option<Topology>, impedance_reference_net: Option<String>,
    impedance_reference_layers: Option<Vec<String>>, impedance_coplanar_gap_mm: Option<f64>,
    calculated_impedance_ohm: Option<f64>, differential: Option<DifferentialRules>
});
dto!(NetRules {
    net: String,
    values: RuleValues
});
dto!(NetClass { name: String, nets: Vec<String>, values: RuleValues });
dto!(DifferentialPair {
    id: String,
    positive: String,
    negative: String
});
dto!(Delay { tolerance_ps: f64, target_ps: Option<f64>, reference_net: String });
dto!(Follow {
    group: String,
    offset: f64
});
dto!(Terminal {
    component: String,
    pad: String
});
dto!(Endpoints {
    net: String,
    from: Terminal,
    to: Terminal
});
dto!(MatchedGroup {
    id: String, nets: Vec<String>, tolerance_mm: Option<f64>, target_mm: Option<f64>,
    delay: Option<Delay>, follow: Option<Follow>, endpoints: Option<Vec<Endpoints>>
});
dto!(Rules {
    default: RuleValues, nets: Vec<NetRules>, differential_pairs: Option<Vec<DifferentialPair>>,
    matched_groups: Option<Vec<MatchedGroup>>, net_classes: Option<Vec<NetClass>>
});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum StackLayer {
    Copper {
        layer: String,
        #[serde(rename = "thicknessMm")]
        thickness_mm: f64,
    },
    Dielectric {
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        name: Option<String>,
        #[serde(rename = "thicknessMm")]
        thickness_mm: f64,
        #[serde(
            rename = "relativePermittivity",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        relative_permittivity: Option<f64>,
        #[serde(
            rename = "lossTangent",
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        loss_tangent: Option<f64>,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        material: Option<String>,
    },
}
dto!(MaskLayer { thickness_mm: Option<f64>, relative_permittivity: Option<f64> });
dto!(SolderMask { top: Option<MaskLayer>, bottom: Option<MaskLayer> });
dto!(Stackup {
    board_thickness_mm: Option<f64>, fallback_copper_thickness_oz: Option<f64>, via_plating_thickness_um: Option<f64>,
    layers: Vec<StackLayer>, solder_mask: Option<SolderMask>
});
dto!(RoutingBoardV2 {
    outline: Vec<Point>, cutouts: Vec<Vec<Point>>, layers: Vec<Layer>, nets: Vec<Net>,
    components: Vec<Component>, pads: Vec<Pad>, keepouts: Vec<Keepout>, holes: Option<Vec<Drill>>,
    stackup: Option<Stackup>, rules: Rules, copper: CopperGroups
});
