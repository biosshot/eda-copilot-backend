import type { PartUuid } from '../lcsc.ts';

// Structural routing primitives retained from the V1 contract. This module has
// no dependency on the router package; existing V1 consumers keep their contract.
export type Millimeters = number

export type PointMm = Readonly<{ x: Millimeters; y: Millimeters }>
export type PolygonMm = Readonly<{
  outer: readonly PointMm[]
  holes?: readonly (readonly PointMm[])[]
}>

export type RoutingLayer = Readonly<{
  /** Canonical layer key used by every other RoutingBoard structure. */
  name: string
  index: number
  side: "top" | "inner" | "bottom"
  /** Disallow new tracks; retain physical copper, planes and via spans. */
  disableRouting?: boolean
}>

export type RoutingNet = Readonly<{
  name: string
}>

export type RoutingComponent = Readonly<{
  designator: string
  at: PointMm
  rotationDeg: number
  side: "top" | "bottom"
  /** Optional absolute board-coordinate outlines for region selection. */
  bodyOutline?: PolygonMm
  courtyardOutline?: PolygonMm
  bounds?: PolygonMm
}>

export type RoutingPadShape =
  | Readonly<{ kind: "circle"; diameterMm: Millimeters }>
  | Readonly<{ kind: "rect"; widthMm: Millimeters; heightMm: Millimeters }>
  | Readonly<{
      kind: "round-rect"
      widthMm: Millimeters
      heightMm: Millimeters
      cornerRadiusMm: Millimeters
    }>
  | Readonly<{ kind: "oval"; widthMm: Millimeters; heightMm: Millimeters }>
  | Readonly<{ kind: "polygon"; polygon: PolygonMm }>

export type RoutingPad = Readonly<{
  id?: string
  component: string
  number: string
  net?: string
  at: PointMm
  rotationDeg: number
  layers: readonly string[]
  shape: RoutingPadShape
  hole?: Readonly<{
    shape: "round" | "slot"
    diameterMm: Millimeters
    slotLengthMm?: Millimeters
    offset?: PointMm
    rotationDeg?: number
    plated: boolean
  }>
}>

export type RoutingKeepout = Readonly<{
  id?: string
  layers: readonly string[]
  polygon: PolygonMm
  forbid: Readonly<{
    tracks: boolean
    vias: boolean
    zones: boolean
  }>
}>

export type RoutedTrack = Readonly<{
  id?: string
  net: string
  layer: string
  widthMm: Millimeters
  /** A track is one constant-width polyline with at least two points. */
  points: readonly PointMm[]
  /** Native evidence of pads already connected by existing copper; references RoutingPad.id. */
  connectedPadIds?: readonly string[]
}>

export type RoutedVia = Readonly<{
  id?: string
  net: string
  at: PointMm
  diameterMm: Millimeters
  drillMm: Millimeters
  fromLayer: string
  toLayer: string
  type?: "through" | "blind-buried" | "micro"
}>

export type RoutedZone = Readonly<{
  id?: string
  /** Omitted only for immutable copper whose electrical net is unknown. */
  net?: string
  layers: readonly string[]
  outline: PolygonMm
  priority?: number
  minThicknessMm?: Millimeters
  clearanceMm?: Millimeters
  connection?: "solid" | "thermal" | "none"
  fill?: Readonly<{
    style: "solid" | "hatched"
    hatchThicknessMm?: Millimeters
    hatchGapMm?: Millimeters
    hatchOrientationDeg?: number
  }>
  padConnection?: Readonly<{
    mode: "solid" | "thermal" | "none"
    thermalGapMm?: Millimeters
    spokeWidthMm?: Millimeters
    spokeCount?: number
    spokeAngleDeg?: number
  }>
  removeIslandsBelowMm2?: number
}>

export type RoutingCopper = Readonly<{
  tracks: readonly RoutedTrack[]
  vias: readonly RoutedVia[]
  zones: readonly RoutedZone[]
}>

export type ViaRules = Readonly<{
  minDiameterMm: Millimeters
  preferredDiameterMm: Millimeters
  minDrillMm: Millimeters
  preferredDrillMm: Millimeters
}>

export type DifferentialRules = Readonly<{
  trackWidthMm: Millimeters
  gapMm: Millimeters
  maxSkewMm?: Millimeters
  maxUncoupledLengthMm?: Millimeters
}>

/** Fully materialized hard and preferred geometry for one rule scope. */
export type RoutingRuleValues = Readonly<{
  clearanceMm: Millimeters
  edgeClearanceMm: Millimeters
  holeToHoleClearanceMm?: Millimeters
  minTrackWidthMm: Millimeters
  preferredTrackWidthMm: Millimeters
  via: ViaRules
  allowedLayers?: readonly string[]
  impedanceOhm?: number
  impedanceTolerancePercent?: number
  impedanceTopology?: "microstrip" | "stripline" | "coplanar-waveguide" | "grounded-coplanar-waveguide"
  impedanceReferenceNet?: string
  impedanceReferenceLayers?: readonly string[]
  impedanceCoplanarGapMm?: Millimeters
  calculatedImpedanceOhm?: number
  differential?: DifferentialRules
}>

export type MatchedLengthRule = Readonly<{
  id: string
  nets: readonly string[]
  toleranceMm?: Millimeters
  targetMm?: Millimeters
  delay?: Readonly<{ tolerancePs: number; targetPs?: number; referenceNet: string }>
  follow?: Readonly<{ group: string; offset: number }>
  endpoints?: readonly Readonly<{ net: string; from: Readonly<{ component: string; pad: string }>; to: Readonly<{ component: string; pad: string }> }>[]
}>

export type DifferentialPairRule = Readonly<{
  id: string
  positive: string
  negative: string
}>

export type RoutingRules = Readonly<{
  default: RoutingRuleValues
  /** Each entry is already fully materialized; absent nets inherit default. */
  nets: readonly Readonly<{ net: string; values: RoutingRuleValues }>[]
  differentialPairs?: readonly DifferentialPairRule[]
  matchedGroups?: readonly MatchedLengthRule[]
  /** Complete effective named-class state, including unchanged imported classes. */
  netClasses?: readonly Readonly<{
    name: string
    nets: readonly string[]
    values: RoutingRuleValues
  }>[]
}>

export type RoutingStackup = Readonly<{
  boardThicknessMm?: Millimeters
  fallbackCopperThicknessOz?: number
  viaPlatingThicknessUm?: number
  layers: readonly (
    | Readonly<{ kind: "copper"; layer: string; thicknessMm: Millimeters }>
    | Readonly<{
        kind: "dielectric"
        name?: string
        thicknessMm: Millimeters
        relativePermittivity?: number
        lossTangent?: number
        material?: string
      }>
  )[]
  solderMask?: Readonly<{
    top?: Readonly<{ thicknessMm?: Millimeters; relativePermittivity?: number }>
    bottom?: Readonly<{ thicknessMm?: Millimeters; relativePermittivity?: number }>
  }>
}>

/**
 * The single EDA-neutral in-memory model. It intentionally contains only data
 * required by routing and is not a lossless EasyEDA or KiCad document.
 */
type RoutingBoardBase = Readonly<{
  outline: readonly PointMm[]
  cutouts: readonly (readonly PointMm[])[]
  layers: readonly RoutingLayer[]
  nets: readonly RoutingNet[]
  components: readonly RoutingComponent[]
  pads: readonly RoutingPad[]
  keepouts: readonly RoutingKeepout[]
  stackup?: RoutingStackup
  rules: RoutingRules
  copper: Readonly<{
    /** Immutable source copper and obstacles. */
    fixed: RoutingCopper
    /** Copper that may be edited by the routing job. */
    editable: RoutingCopper
  }>
}>


/** CAD copper on one layer. An available empty array means a verified empty result. */
export type FilledPolygons = {
  layer: string;
  source: 'cad';
  nativePrimitiveIds?: readonly string[];
} & (
  | { status: 'available'; polygons: readonly PolygonMm[]; freshness: 'current' | 'stale' | 'unknown' }
  | { status: 'missing' | 'failed' | 'unsupported'; reason: string }
);
export type PourPadConnection = Pick<NonNullable<RoutedZone['padConnection']>,
  'mode' | 'thermalGapMm' | 'spokeWidthMm' | 'spokeAngleDeg'>;
export type PourZoneV2 = Pick<RoutedZone, 'net' | 'layers' | 'outline' | 'priority'> & {
  id: string;
  kind: 'pour';
  preserveIslands?: boolean;
  filledPolygons: readonly FilledPolygons[];
  refillRules: {
    clearanceMm?: number;
    padConnection?: { singleLayer?: PourPadConnection; multiLayer?: PourPadConnection };
    status: 'partial' | 'unknown';
    unresolved: readonly string[];
  };
};
export type FillZoneV2 = Pick<RoutedZone, 'net' | 'layers' | 'outline'> & {
  id: string;
  kind: 'fill';
  filledPolygons: readonly FilledPolygons[];
};
export type RoutedZoneV2 = PourZoneV2 | FillZoneV2;
export type RoutingComponentV2 = RoutingComponent & {
  id?: string;
  partUuid?: PartUuid;
  value?: string;
  footprintName?: string;
  packageName?: string;
  /** Local body dimensions, not a bounding box of all footprint primitives. */
  bodySizeMm?: { x: number; y: number; z: number };
};
export type RoutingPadV2 = Omit<RoutingPad, 'id' | 'component'> & {
  id: string;
  component?: string;
  /** When present, resolves every layer in layers; shapes are pad-local. */
  shapesByLayer?: Readonly<Record<string, RoutingPadShape>>;
};
export type RoutedTrackV2 = Omit<RoutedTrack, 'net'> & { net?: string };
export type RoutedViaV2 = Omit<RoutedVia, 'net' | 'type'> & {
  net?: string;
  type: 'through' | 'blind-buried';
};
export type RoutingCopperV2 = {
  tracks: readonly RoutedTrackV2[];
  vias: readonly RoutedViaV2[];
  zones: readonly RoutedZoneV2[];
};
/** Input transport for analyze. Millimetres, absolute Y-down board coordinates;
 * local pad shapes/holes rotate with their pad. Layer indices follow physical order.
 * No implied material, stackup, missing copper or via span defaults.
 */
export type RoutingBoardV2 = Omit<RoutingBoardBase, 'components' | 'pads' | 'copper'> & {
  components: readonly RoutingComponentV2[];
  pads: readonly RoutingPadV2[];
  copper: { fixed: RoutingCopperV2; editable: RoutingCopperV2 };
  /** Independent drills only; never duplicate a pad/via hole. */
  holes?: readonly { id: string; at: PointMm; hole: NonNullable<RoutingPad['hole']> }[];
};
