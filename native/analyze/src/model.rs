//! Validated analysis data. Coordinates remain F64 millimetres, absolute Y-down.
//! Pad shapes and drill offsets remain local; no bottom-side mirror is applied.
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// Input field path, also identifies objects without a native ID.
    pub path: String,
    pub id: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub outer: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Inner,
    Bottom,
}
#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub physical_index: usize,
    pub side: Side,
    pub disable_routing: Option<bool>,
}
#[derive(Debug, Clone)]
pub enum Part {
    Lcsc(String),
    Library { uuid: String, library_uuid: String },
}
#[derive(Debug, Clone)]
pub struct Component {
    pub source: Source,
    pub designator: String,
    pub at: Point,
    pub rotation_deg: f64,
    pub side: Side,
    pub body_outline: Option<Polygon>,
    pub courtyard_outline: Option<Polygon>,
    pub bounds: Option<Polygon>,
    pub part: Option<Part>,
    pub value: Option<String>,
    pub footprint: Option<String>,
    pub package: Option<String>,
    pub body_size_mm: Option<[f64; 3]>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum PadShape {
    Circle {
        diameter_mm: f64,
    },
    Rect {
        width_mm: f64,
        height_mm: f64,
    },
    RoundRect {
        width_mm: f64,
        height_mm: f64,
        corner_radius_mm: f64,
    },
    Oval {
        width_mm: f64,
        height_mm: f64,
    },
    Polygon(Polygon),
}
#[derive(Debug, Clone, PartialEq)]
pub enum DrillShape {
    Round,
    Slot { length_mm: f64 },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Drill {
    pub shape: DrillShape,
    pub diameter_mm: f64,
    pub offset: Option<Point>,
    pub rotation_deg: Option<f64>,
    pub plated: bool,
}
#[derive(Debug, Clone)]
pub struct Pad {
    pub source: Source,
    pub component: Option<usize>,
    pub number: String,
    pub net: Option<usize>,
    pub at: Point,
    pub rotation_deg: f64,
    pub layers: Vec<usize>,
    pub common_shape: PadShape,
    /// None means use common_shape. Some contains exactly one shape per pad layer.
    pub layer_shapes: Option<BTreeMap<usize, PadShape>>,
    pub drill: Option<Drill>,
}
#[derive(Debug, Clone)]
pub struct IndependentDrill {
    pub source: Source,
    pub at: Point,
    pub drill: Drill,
}
#[derive(Debug, Clone)]
pub struct Keepout {
    pub source: Source,
    pub layers: Vec<usize>,
    pub polygon: Polygon,
    pub forbid: Forbid,
}
#[derive(Debug, Clone)]
pub struct Forbid {
    pub tracks: bool,
    pub vias: bool,
    pub zones: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Fixed,
    Editable,
}
#[derive(Debug, Clone)]
pub struct Track {
    pub source: Source,
    pub ownership: Ownership,
    pub net: Option<usize>,
    pub layer: usize,
    pub width_mm: f64,
    pub points: Vec<Point>,
    pub connected_pads: Option<Vec<usize>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViaKind {
    Through,
    BlindBuried,
}
#[derive(Debug, Clone)]
pub struct Via {
    pub source: Source,
    pub ownership: Ownership,
    pub net: Option<usize>,
    pub at: Point,
    pub diameter_mm: f64,
    pub drill_mm: f64,
    pub first_layer: usize,
    pub last_layer: usize,
    pub kind: ViaKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Stale,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    Missing,
    Failed,
    Unsupported,
}
#[derive(Debug, Clone)]
pub enum CadFillState {
    Available {
        polygons: Vec<Polygon>,
        freshness: Freshness,
    },
    Unavailable {
        state: Unavailable,
        reason: String,
    },
}
#[derive(Debug, Clone)]
pub struct CadFill {
    pub source: Source,
    pub layer: usize,
    pub native_ids: Option<Vec<String>>,
    pub state: CadFillState,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMode {
    Solid,
    Thermal,
    None,
}
#[derive(Debug, Clone)]
pub struct PadConnection {
    pub mode: ConnectionMode,
    pub thermal_gap_mm: Option<f64>,
    pub spoke_width_mm: Option<f64>,
    pub spoke_angle_deg: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct RefillRules {
    pub clearance_mm: Option<f64>,
    pub single_layer: Option<PadConnection>,
    pub multi_layer: Option<PadConnection>,
    pub partial: bool,
    pub unresolved: Vec<String>,
}
#[derive(Debug, Clone)]
pub enum ZoneKind {
    Fill,
    Pour {
        priority: Option<f64>,
        preserve_islands: Option<bool>,
        refill: RefillRules,
    },
}
#[derive(Debug, Clone)]
pub struct Zone {
    pub source: Source,
    pub ownership: Ownership,
    pub net: Option<usize>,
    pub layers: Vec<usize>,
    /// Authoring boundary, never substituted for actual copper.
    pub boundary: Polygon,
    pub kind: ZoneKind,
    pub fills: Vec<CadFill>,
}
#[derive(Debug, Clone)]
pub enum StackLayer {
    Copper {
        layer: usize,
        thickness_mm: f64,
    },
    Dielectric {
        name: Option<String>,
        thickness_mm: f64,
        relative_permittivity: Option<f64>,
        loss_tangent: Option<f64>,
        material: Option<String>,
    },
}
#[derive(Debug, Clone)]
pub struct Mask {
    pub thickness_mm: Option<f64>,
    pub relative_permittivity: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct Stackup {
    pub board_thickness_mm: Option<f64>,
    pub fallback_copper_thickness_oz: Option<f64>,
    pub via_plating_thickness_um: Option<f64>,
    pub layers: Vec<StackLayer>,
    pub top_mask: Option<Mask>,
    pub bottom_mask: Option<Mask>,
}
/// Fully materialized routing constraints, retained without interpreting them as materials.
#[derive(Debug, Clone)]
pub struct RuleValues {
    pub clearance_mm: f64,
    pub edge_clearance_mm: f64,
    pub hole_to_hole_clearance_mm: Option<f64>,
    pub min_track_width_mm: f64,
    pub preferred_track_width_mm: f64,
    pub via: ViaRules,
    pub allowed_layers: Option<Vec<usize>>,
    pub impedance_ohm: Option<f64>,
    pub impedance_tolerance_percent: Option<f64>,
    pub impedance_topology: Option<ImpedanceTopology>,
    pub impedance_reference_net: Option<usize>,
    pub impedance_reference_layers: Option<Vec<usize>>,
    pub impedance_coplanar_gap_mm: Option<f64>,
    pub calculated_impedance_ohm: Option<f64>,
    pub differential: Option<DifferentialRules>,
}
#[derive(Debug, Clone)]
pub enum ImpedanceTopology {
    Microstrip,
    Stripline,
    Coplanar,
    GroundedCoplanar,
}
#[derive(Debug, Clone)]
pub struct ViaRules {
    pub min_diameter_mm: f64,
    pub preferred_diameter_mm: f64,
    pub min_drill_mm: f64,
    pub preferred_drill_mm: f64,
}
#[derive(Debug, Clone)]
pub struct DifferentialRules {
    pub track_width_mm: f64,
    pub gap_mm: f64,
    pub max_skew_mm: Option<f64>,
    pub max_uncoupled_length_mm: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct NetClass {
    pub name: String,
    pub nets: Vec<usize>,
    pub values: RuleValues,
}
#[derive(Debug, Clone)]
pub struct DifferentialPair {
    pub id: String,
    pub positive: usize,
    pub negative: usize,
}
#[derive(Debug, Clone)]
pub struct Delay {
    pub tolerance_ps: f64,
    pub target_ps: Option<f64>,
    pub reference_net: usize,
}
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub net: usize,
    pub from_pad: usize,
    pub to_pad: usize,
}
#[derive(Debug, Clone)]
pub struct MatchedGroup {
    pub id: String,
    pub nets: Vec<usize>,
    pub tolerance_mm: Option<f64>,
    pub target_mm: Option<f64>,
    pub delay: Option<Delay>,
    pub follow: Option<(usize, f64)>,
    pub endpoints: Option<Vec<Endpoints>>,
}
#[derive(Debug, Clone)]
pub struct Rules {
    pub default: RuleValues,
    pub nets: BTreeMap<usize, RuleValues>,
    pub net_classes: Vec<NetClass>,
    pub differential_pairs: Vec<DifferentialPair>,
    pub matched_groups: Vec<MatchedGroup>,
}
#[derive(Debug, Clone)]
pub struct Board {
    pub outline: Vec<Point>,
    pub cutouts: Vec<Vec<Point>>,
    pub layers: Vec<Layer>,
    pub nets: Vec<String>,
    pub components: Vec<Component>,
    pub pads: Vec<Pad>,
    pub keepouts: Vec<Keepout>,
    /// None preserves the absence of independent-drill data in the transport.
    pub drills: Option<Vec<IndependentDrill>>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    pub zones: Vec<Zone>,
    pub stackup: Option<Stackup>,
    pub rules: Rules,
}
