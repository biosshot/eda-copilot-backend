//! Common native tetrahedral ownership and Galerkin maps for a mixed foil/
//! volume conductor. This is a geometric/current-space building block.
//! A caller still needs the common electric/magnetic operators and full-volume
//! residual checks to admit a response.
use super::{
    accurate::Sum,
    board_mesh::{self, BoardMesh, Electrode},
    capacitance::TraceRow,
    copper_skin_profile, error,
    foil_reduced::{self, MatchedGrid},
    spatial_partition::Partition,
    volume_current,
    volume_field::{Field,ElectricApply},
    AllocationBudget, Result,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MIXED_PREPARE_MAX_HZ: f64 = 10_000_000_000.;
const SKIN_VOLUME_MAX_HZ: f64 = 100_000.;

#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub enum CheckedOperator {
    #[default]
    #[serde(skip)]
    Legacy,
    #[serde(rename = "skin-volume-v3")]
    SkinVolumeV3,
    #[serde(rename = "hybrid-planar-v3")]
    HybridPlanarV3,
}

/// Internal production automatic routing policy; no public DSL syntax.
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub enum PartitionPolicy {
    #[default]
    #[serde(skip)]
    Legacy,
    #[serde(rename = "automatic-conductive-schur-v1")]
    AutomaticConductiveSchurV1,
    #[serde(rename = "automatic-hybrid-v1")]
    AutomaticHybridV1,
}

impl CheckedOperator {
    pub(super) fn admits(self, frequency_hz: f64) -> bool {
        let upper = match self {
            Self::Legacy => 10_000.,
            Self::SkinVolumeV3 => SKIN_VOLUME_MAX_HZ,
            Self::HybridPlanarV3 => MIXED_PREPARE_MAX_HZ,
        };
        frequency_hz.is_finite() && (1000. ..=upper).contains(&frequency_hz)
    }

    pub(super) fn axis_scope(self) -> &'static str {
        match self {
            Self::Legacy => "1..10 kHz",
            Self::SkinVolumeV3 => "1..100 kHz profiled implementation limit",
            Self::HybridPlanarV3 => "1 kHz..10 GHz hybrid implementation limit",
        }
    }

    pub(super) fn validate_partition(self, policy: PartitionPolicy) -> Result<()> {
        if self==Self::HybridPlanarV3 && policy!=PartitionPolicy::AutomaticHybridV1 {
            return Err(error("BOARD_FOIL_POLICY", "hybrid-planar-v3 requires automatic-hybrid-v1"));
        }
        Ok(())
    }
    fn policy_name(self) -> &'static str {
        match self { Self::Legacy=>"legacy", Self::SkinVolumeV3=>"skin-volume-v3", Self::HybridPlanarV3=>"hybrid-planar-v3" }
    }

    pub(super) fn skin_profile(
        self,
        requested_upper_hz: f64,
        profile_upper_hz: Option<f64>,
        vertical_step_m: f64,
    ) -> Result<Option<copper_skin_profile::Profile>> {
        if self == Self::Legacy {
            if profile_upper_hz.is_some() {
                return Err(error(
                    "BOARD_FOIL_GRID",
                    "profileUpperHz requires skin-volume-v3",
                ));
            }
            return Ok(None);
        }
        // Training/enrichment uses only requested exact-grid points. A
        // declared upper bound lets supplemental points reuse the same mesh
        // and thin/volume ownership even when their grid has a lower maximum.
        let upper_hz = profile_upper_hz.unwrap_or(requested_upper_hz);
        if !self.admits(upper_hz) || upper_hz < requested_upper_hz {
            return Err(error("BOARD_FOIL_GRID", &format!("profileUpperHz must cover the exact grid within {}",self.axis_scope())));
        }
        Ok(Some(copper_skin_profile::Profile {
            upper_hz,
            current_step_m: vertical_step_m,
            field_step_m: vertical_step_m,
        }))
    }
}

pub(super) fn skin_profile_evidence(
    mesh: &BoardMesh,
    requested_upper_hz: f64,
    effective_upper_hz: f64,
) -> serde_json::Value {
    serde_json::json!({
        "version": copper_skin_profile::VERSION,
        "requestedUpperHz": requested_upper_hz, "trainingUpperBoundHz": requested_upper_hz,
        "effectiveUpperHz": effective_upper_hz, "skinDepthDivisor": 3,
        "currentAndElectricShareMesh": true,
        "layers": mesh.copper_skin_layers, "roundBarrels": mesh.copper_skin_barrels,
        "barrelSheet": copper_skin_profile::barrel_sheet_enabled().then(|| serde_json::json!({
            "approximation": true,
            "criterion": "P7_BARREL_SHEET: one radial layer per via wall at any skin depth",
            "resistanceScaled": copper_skin_profile::barrel_sheet_scales_resistance(),
            "multiplierTag": copper_skin_profile::barrel_sheet_scales_resistance().then_some("approximation, one multiplier per band; one-sided-field approximation"),
            "status": "approximation",
            "statusNote": copper_skin_profile::barrel_sheet_scales_resistance().then_some("R завышено до 16% на двух купонах при 40 МГц, эталон не проверен (оценка для плоских полулистов; стенка via против эталона не сверялась)"),
            "resistanceMultiplierMax": copper_skin_profile::barrel_sheet_scales_resistance().then(|| copper_skin_profile::foil_resistance_ratio(mesh.copper_skin_barrels.iter().map(|b| b.maximum_radial_step_m * b.shells as f64 / b.skin_depth_m).fold(0., f64::max))),
            "estimatedResistanceError": copper_skin_profile::foil_sheet_error(mesh.copper_skin_barrels.iter().map(|b| b.maximum_radial_step_m * b.shells as f64 / b.skin_depth_m).fold(0., f64::max)),
            "warning": "wall radial step exceeds delta/3 when delta < 3t; current redistribution through the wall thickness is not resolved",
            "maxThicknessOverSkin": mesh.copper_skin_barrels.iter().map(|b| b.maximum_radial_step_m * b.shells as f64 / b.skin_depth_m).fold(0., f64::max),
        })),
        "scope": "Planar copper thickness and round barrel radial resolution screen; lateral, proximity, domain and band accuracy unverified",
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Foil(usize),
    Volume,
    /// Thin-wall vertical prism segment of a plated barrel (axial current only).
    Barrel(usize),
    /// Noise-level sliver column (policy: noise); no current mode, no support.
    Dropped,
}
/// Copper ownership of plated barrel columns. `ThinWall` is the default of the
/// automatic hybrid; `P7_BARREL_MODEL=volume` restores the tetrahedral path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarrelModel {
    Volume,
    ThinWall,
}
impl BarrelModel {
    pub fn hybrid_default() -> Self {
        match std::env::var("P7_BARREL_MODEL") {
            Ok(v) if v.trim().eq_ignore_ascii_case("volume") => Self::Volume,
            _ => Self::ThinWall,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Volume => "volume",
            Self::ThinWall => "thin-wall-axial-prism-v1",
        }
    }
}
/// One annulus triangle between two adjacent copper-layer potentials: a single
/// uniform axial current mode over its full z span.
#[derive(Debug)]
pub struct BarrelPrism {
    pub barrel_id: String,
    pub planar_triangle: usize,
    pub z_span_m: [f64; 2],
    pub tetrahedra: Vec<usize>,
    pub mode: usize,
    pub volume_m3: f64,
    pub area_m2: f64,
}
struct BarrelSegment {
    barrel: usize,
    triangle: usize,
    span: [f64; 2],
    cells: Vec<usize>,
}
#[derive(Debug)]
pub struct FoilPrism {
    pub layer: String,
    pub planar_triangle: usize,
    /// Exact physical extrusion interval represented by this foil support.
    pub z_span_m: [f64; 2],
    pub tetrahedra: Vec<usize>,
    pub first_current: usize,
    pub volume_m3: f64,
}
#[derive(Clone, Copy, Debug)]
struct CurrentEntry {
    mode: usize,
    weight: f64,
}
pub struct MixedMap<'a> {
    mesh: &'a BoardMesh,
    pub full: volume_current::Space,
    /// Exactly one owner in original copper tetrahedron order.
    pub owners: Vec<Owner>,
    pub foils: Vec<FoilPrism>,
    pub barrels: Vec<BarrelPrism>,
    pub barrel_model: BarrelModel,
    /// Distinct plated barrels represented by prisms, and annulus columns that
    /// could not be (kept as volume tetrahedra).
    pub barrel_count: usize,
    pub barrel_fallback_columns: usize,
    /// Pad-copper mesh slivers that carry an annulus source id but are not wall.
    pub barrel_sliver_columns: usize,
    /// Foil columns admitted by the skin-depth rule although thickness > altitude/4.
    pub foil_aspect_relaxed_columns: usize,
    /// Noise columns without current mode, and their copper volume.
    pub sliver_columns_dropped: usize,
    pub dropped_volume_m3: f64,
    pub dropped_volume_fraction: f64,
    pub electrodes: Vec<usize>,
    pub potential_components: Vec<Vec<usize>>,
    pub incidence: Vec<(usize, usize, f64)>,
    pub resistance_ohm: Vec<f64>,
    /// Resistance multiplier applied to each foil / barrel prism (1 unless `P7_FOIL_SHEET=2` / `P7_BARREL_SHEET=2`);
    /// the spatial power recovery applies the same factor.
    pub foil_resistance_scale: Vec<f64>,
    pub barrel_resistance_scale: Vec<f64>,
    potential_map: Vec<usize>,
    full_electrodes: Vec<usize>,
    current_map: Vec<Option<CurrentEntry>>,
    pub planned_bytes: usize,
}
struct Candidate {
    layer: usize,
    triangle: usize,
    span: [f64; 2],
    cells: Vec<usize>,
    volume: f64,
    foil_density: Option<f64>,
    dropped: bool,
}

fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
fn merge(parent: &mut [usize], a: usize, b: usize) {
    let a = root(parent, a);
    let b = root(parent, b);
    parent[b] = a;
}
fn check_time(start: Instant, wall: Duration) -> Result<()> {
    if start.elapsed() >= wall {
        return Err(error("DEADLINE", "mixed conductor ownership"));
    }
    Ok(())
}

/// P7_HIGHBAND_CUTS (developer switch, default on since 2026-11-14, `0` = off): planner-declared cut-plane
/// electrodes are admitted in the hybrid-planar-v3 path as equipotential faces.
pub(super) fn highband_cuts_enabled() -> bool {
    super::super::p7_default_on("P7_HIGHBAND_CUTS")
}

/// P7_BAND_SCREEN=<kD> (developer switch, default off, approximation): replaces the 0.1 factor of the quasistatic
/// electrical-size screen, only together with P7_HIGHBAND_CUTS=1. Valid only for 0 < kD <= 0.5.
pub(super) fn band_screen_kd() -> Result<Option<f64>> {
    band_kd_from(std::env::var("P7_BAND_SCREEN").ok().as_deref(), highband_cuts_enabled())
}

fn band_kd_from(text: Option<&str>, cuts: bool) -> Result<Option<f64>> {
    let Some(text) = text else { return Ok(None) };
    if !cuts { return Ok(None) }
    match text.trim().parse::<f64>() {
        Ok(k) if k.is_finite() && k > 0. && k <= 0.5 => Ok(Some(k)),
        _ => Err(error("BOARD_FOIL_SCOPE", &format!("P7_BAND_SCREEN must be a number with 0 < kD <= 0.5, got {text:?}"))),
    }
}

/// Electrical-size terms of the screen: (diagonal D in metres, maximum relative permittivity).
fn screen_terms(request: &board_mesh::Request) -> (f64, f64) {
    let dx = 1e-3 * (request.roi_mm[1][0] - request.roi_mm[0][0]);
    let dy = 1e-3 * (request.roi_mm[1][1] - request.roi_mm[0][1]);
    let dz: f64 = request.stack.iter().map(|l| l.thickness_m).sum();
    let epsilon = request.stack.iter().filter_map(|l| l.epsilon_r).fold(1., f64::max);
    (dx.hypot(dy).hypot(dz), epsilon)
}

/// `provenance.bandScreen` record, written only when P7_BAND_SCREEN is active.
pub(super) fn band_screen_record(request: &board_mesh::Request, frequencies_hz: &[f64], skin: Option<copper_skin_profile::Profile>) -> Result<Option<serde_json::Value>> {
    let Some(kd) = band_screen_kd()? else { return Ok(None) };
    let maximum = skin.map_or(*frequencies_hz.last().unwrap(), |profile| profile.upper_hz);
    let (diagonal, epsilon) = screen_terms(request);
    let c = 299792458.;
    let threshold = kd * c / (2. * std::f64::consts::PI * diagonal * epsilon.sqrt());
    let size = 2. * std::f64::consts::PI * maximum / c * diagonal * epsilon.sqrt();
    Ok(Some(serde_json::json!({"kD": kd, "thresholdHz": threshold, "sizeM": diagonal, "epsilonR": epsilon, "maximumHz": maximum, "electricalSize": size,
        "status": "approximation",
        "statusNote": "порог квазистатики kD вместо 0,1 (только с разрезами): излучение и запаздывание в области не проверены, эталона нет"})))
}

fn projected_pad_collisions<'a>(
    planar_count: usize,
    electrodes: &'a [Electrode],
    electrode_nodes: &[Vec<usize>],
) -> BTreeSet<(&'a str, usize)> {
    let mut owner = BTreeMap::new();
    let mut collisions = BTreeSet::new();
    for (i, (electrode, nodes)) in electrodes.iter().zip(electrode_nodes).enumerate() {
        for &node in nodes {
            let column = (electrode.layer.as_str(), node % planar_count);
            if owner.insert(column, i).is_some_and(|prior| prior != i) {
                collisions.insert(column);
            }
        }
    }
    collisions
}

pub(super) fn has_projected_pad_collision(
    mesh: &BoardMesh,
    electrodes: &[Electrode],
) -> Result<bool> {
    let planar_count = mesh.planar_nodes_mm.len();
    if planar_count == 0 {
        return Err(error(
            "MIXED_FOIL_GEOMETRY",
            "native common planar arrangement missing",
        ));
    }
    let nodes = electrodes
        .iter()
        .map(|e| mesh.electrode_nodes(e))
        .collect::<Result<Vec<_>>>()?;
    Ok(!projected_pad_collisions(planar_count, electrodes, &nodes).is_empty())
}

/// Distance from `p` to the closest segment of the closed rings, in the input unit.
fn ring_distance(p: [f64; 2], rings: &[Vec<[f64; 2]>]) -> f64 {
    let mut best = f64::INFINITY;
    for ring in rings {
        for w in ring.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let len2 = d[0] * d[0] + d[1] * d[1];
            let t = if len2 > 0. {
                (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0., 1.)
            } else {
                0.
            };
            best = best.min((p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1]));
        }
    }
    best
}

/// Split every annulus column of every plated barrel into prisms between the
/// potentials of adjacent copper layers. The first segment starts at the outer
/// face of the first copper layer; each further segment starts at the lower face
/// of the next interior copper layer; the last ends at the outer face of the
/// last layer. Segments therefore tile the column exactly (copper volume and
/// single ownership are preserved). Columns that do not satisfy the regular
/// extrusion contract are returned as `fallback` and stay volumetric.
fn plan_barrel_segments(
    mesh: &BoardMesh,
    layers: &[(&String, &[f64; 2])],
    barrel_cell: &[bool],
    barrel_of: &dyn Fn(usize) -> Option<usize>,
    start: Instant,
    wall: Duration,
) -> Result<(Vec<BarrelSegment>, usize, usize, Vec<usize>)> {
    let tets = &mesh.mesh.copper_tetrahedra;
    let first_barrel_shape = mesh.field_copper.len().saturating_sub(mesh.barrel_ids.len());
    let mut columns = BTreeMap::<usize, Vec<usize>>::new();
    for (i, tet) in tets.iter().enumerate() {
        if barrel_cell[i] {
            columns.entry(tet.planar_triangle).or_default().push(i);
        }
    }
    let mut segments = Vec::new();
    let mut fallback = 0usize;
    let mut slivers = 0usize;
    let mut sliver_cells = Vec::new();
    'column: for (triangle, mut cells) in columns {
        check_time(start, wall)?;
        cells.sort_by_key(|&i| tets[i].slab);
        let mut barrel = None;
        for &i in &cells {
            let b = barrel_of(i);
            if b.is_none() || barrel.is_some_and(|x| Some(x) != b) {
                fallback += 1;
                continue 'column;
            }
            barrel = b;
        }
        let ids = mesh.planar_triangles[triangle];
        let [a, b, c] = ids.map(|i| mesh.mesh.nodes_m[i]);
        let twice = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
        let z = |i: usize| {
            let zs = tets[i].nodes.map(|j| mesh.mesh.nodes_m[j][2]);
            (
                zs.into_iter().fold(f64::INFINITY, f64::min),
                zs.into_iter().fold(f64::NEG_INFINITY, f64::max),
            )
        };
        let sigma = mesh.conductivity_s_m[cells[0]];
        // Regular extrusion: three tetrahedra per slab, contiguous slabs.
        let mut slabs: Vec<(usize, f64, f64, Vec<usize>)> = Vec::new();
        for &i in &cells {
            let (lo, hi) = z(i);
            match slabs.last_mut() {
                Some(s) if s.0 == tets[i].slab && s.1 == lo && s.2 == hi => s.3.push(i),
                _ => slabs.push((tets[i].slab, lo, hi, vec![i])),
            }
        }
        let (zmin, zmax) = (slabs[0].1, slabs[slabs.len() - 1].2);
        let tol = 1e-9 * (zmax - zmin);
        // Only columns that run through the whole physical barrel span are barrel
        // wall. A mesh sliver sharing a source id with the annulus only inside one
        // pad layer is pad copper and keeps the ordinary (volume) ownership.
        match mesh.field_copper.get(first_barrel_shape + barrel.unwrap()) {
            Some((_, span, true)) => {
                if (span[0] - zmin).abs() > tol || (span[1] - zmax).abs() > tol {
                    slivers += 1;
                    sliver_cells.extend_from_slice(&cells);
                    continue;
                }
            }
            _ => {
                fallback += 1;
                continue;
            }
        }
        if !(twice.is_finite() && twice > 0. && zmax > zmin)
            || cells.iter().any(|&i| mesh.conductivity_s_m[i] != sigma)
            || slabs.iter().any(|s| s.3.len() != 3)
            || slabs.windows(2).any(|w| (w[0].2 - w[1].1).abs() > tol)
        {
            fallback += 1;
            continue;
        }
        let volume = cells
            .iter()
            .fold(Sum::default(), |sum, &i| sum.add(Sum::from(tets[i].volume_m3)))
            .value();
        let expected = 0.5 * twice * (zmax - zmin);
        if (volume - expected).abs() > 1e-8 * expected {
            fallback += 1;
            continue;
        }
        // Boundaries at lower faces of interior copper layers.
        let mut bounds = vec![zmin];
        for (_, span) in layers {
            if span[0] > zmin + tol && span[1] < zmax - tol {
                bounds.push(span[0]);
            }
        }
        bounds.push(zmax);
        bounds.sort_by(f64::total_cmp);
        bounds.dedup_by(|a, b| (*a - *b).abs() <= tol);
        // Every boundary must be an existing z plane of this column.
        if bounds[1..bounds.len() - 1]
            .iter()
            .any(|&zb| !slabs.iter().any(|s| (s.1 - zb).abs() <= tol))
        {
            fallback += 1;
            continue;
        }
        for w in bounds.windows(2) {
            let seg: Vec<usize> = slabs
                .iter()
                .filter(|s| s.1 >= w[0] - tol && s.2 <= w[1] + tol)
                .flat_map(|s| s.3.iter().copied())
                .collect();
            segments.push(BarrelSegment {
                barrel: barrel.unwrap(),
                triangle,
                span: [w[0], w[1]],
                cells: seg,
            });
        }
    }
    Ok((segments, fallback, slivers, sliver_cells))
}

/// A wall column whose area share of its barrel is below this carries a negligible part of
/// the barrel conductance (columns of one barrel share the same axial length and sigma).
const BARREL_NOISE_SHARE: f64 = 1e-6;

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v.trim() == "1")
}

/// Columns (distinct annulus triangles) of each barrel whose conductance share is below
/// `BARREL_NOISE_SHARE`. Returned as `(barrel, triangle)` pairs. `area` is the planar
/// triangle area of a column.
fn barrel_noise_columns(
    segments: &[BarrelSegment],
    area: &dyn Fn(usize) -> f64,
) -> BTreeSet<(usize, usize)> {
    let mut by_barrel = BTreeMap::<usize, BTreeMap<usize, f64>>::new();
    for s in segments {
        by_barrel.entry(s.barrel).or_default().insert(s.triangle, area(s.triangle));
    }
    let mut noise = BTreeSet::new();
    for (&barrel, columns) in &by_barrel {
        let total: f64 = columns.values().sum();
        for (&triangle, &a) in columns {
            if !(a >= BARREL_NOISE_SHARE * total) {
                noise.insert((barrel, triangle));
            }
        }
    }
    noise
}

/// Diagnostic only (`P7_BARREL_PROBE=1`): per barrel column count, min/median triangle area,
/// axial length and the number of columns below the noise share.
fn barrel_probe(mesh: &BoardMesh, segments: &[BarrelSegment], area: &dyn Fn(usize) -> f64) {
    let mut by_barrel = BTreeMap::<usize, (BTreeMap<usize, f64>, f64, f64)>::new();
    for s in segments {
        let e = by_barrel.entry(s.barrel).or_insert((BTreeMap::new(), f64::INFINITY, f64::NEG_INFINITY));
        e.0.insert(s.triangle, area(s.triangle));
        e.1 = e.1.min(s.span[0]);
        e.2 = e.2.max(s.span[1]);
    }
    let (mut barrels, mut columns, mut slivers) = (0usize, 0usize, 0usize);
    let mut sliver_barrels = 0usize;
    let mut global_min = f64::INFINITY;
    for (&b, (cols, z0, z1)) in &by_barrel {
        let mut areas: Vec<f64> = cols.values().copied().collect();
        areas.sort_by(f64::total_cmp);
        let total: f64 = areas.iter().sum();
        let n_sliver = areas.iter().filter(|&&a| !(a >= BARREL_NOISE_SHARE * total)).count();
        barrels += 1;
        columns += areas.len();
        slivers += n_sliver;
        sliver_barrels += usize::from(n_sliver > 0);
        global_min = global_min.min(areas[0]);
        eprintln!(
            "P7_BARREL_PROBE barrel={} id={} columns={} min_area_m2={:e} median_area_m2={:e} total_area_m2={:e} length_m={:e} slivers={}",
            b, mesh.barrel_ids[b], areas.len(), areas[0], areas[areas.len() / 2], total, z1 - z0, n_sliver
        );
    }
    eprintln!(
        "P7_BARREL_PROBE_SUMMARY barrels={barrels} columns={columns} sliver_columns={slivers} barrels_with_slivers={sliver_barrels} min_area_m2={global_min:e} share_limit={BARREL_NOISE_SHARE:e}"
    );
}

pub fn prepare<'a>(
    mesh: &'a BoardMesh,
    electrodes: &[Electrode],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    wall: Duration,
) -> Result<MixedMap<'a>> {
    prepare_model(mesh, electrodes, max_frequency_hz, budget, wall, BarrelModel::Volume)
}

pub fn prepare_model<'a>(
    mesh: &'a BoardMesh,
    electrodes: &[Electrode],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    wall: Duration,
    barrel_model: BarrelModel,
) -> Result<MixedMap<'a>> {
    // Dropping noise sliver columns is the default. Only when that would disconnect a conductor
    // are the sole-link slivers kept as ordinary volume (they are numerically poor tetrahedra).
    match prepare_model_inner(mesh, electrodes, max_frequency_hz, budget, wall, barrel_model, false) {
        Err(e) if format!("{e:?}").contains("dropped sliver columns would disconnect") => {
            prepare_model_inner(mesh, electrodes, max_frequency_hz, budget, wall, barrel_model, true)
        }
        other => other,
    }
}

fn prepare_model_inner<'a>(
    mesh: &'a BoardMesh,
    electrodes: &[Electrode],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    wall: Duration,
    barrel_model: BarrelModel,
    keep_sole_link_slivers: bool,
) -> Result<MixedMap<'a>> {
    let start = Instant::now();
    if !max_frequency_hz.is_finite() || !(0. ..=MIXED_PREPARE_MAX_HZ).contains(&max_frequency_hz) {
        return Err(error(
            "MIXED_FOIL_BAND",
            "finite explicit thin-foil screen within 0..10 GHz required",
        ));
    }
    let electrode_nodes = electrodes
        .iter()
        .map(|e| mesh.electrode_nodes(e))
        .collect::<Result<Vec<_>>>()?;
    let full =
        volume_current::assemble(&mesh.mesh, &mesh.conductivity_s_m, &electrode_nodes, budget)?;
    let nt = full.cells.len();
    let nv = full.potential_count();
    let planned_bytes = full
        .planned_bytes
        .checked_add(
            nt.checked_mul(4096)
                .ok_or_else(|| error("MEMORY_LIMIT", "mixed ownership count overflow"))?,
        )
        .and_then(|n| n.checked_add(nv.checked_mul(1024)?))
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed map plan overflow"))?;
    budget.check(planned_bytes)?;
    let nxy = mesh.planar_nodes_mm.len();
    if nxy == 0 || mesh.mesh.nodes_m.len() < nxy {
        return Err(error(
            "MIXED_FOIL_GEOMETRY",
            "native common planar arrangement missing",
        ));
    }
    let collisions = projected_pad_collisions(nxy, electrodes, &electrode_nodes);
    let mut layers: Vec<_> = mesh.layer_spans.iter().collect();
    layers.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
    let barrels: BTreeSet<_> = mesh.barrel_ids.iter().map(String::as_str).collect();
    // Only resolved skin meshes need separate through-thickness current modes.
    // The existing whole-layer grouping stays bitwise unchanged below the screen.
    let mut stratified = vec![false; layers.len()];
    if !mesh.copper_skin_layers.is_empty() && max_frequency_hz > 0. {
        for (i, tet) in mesh.mesh.copper_tetrahedra.iter().enumerate() {
            let z = tet.nodes.map(|j| mesh.mesh.nodes_m[j][2]);
            let lo = z.into_iter().fold(f64::INFINITY, f64::min);
            let hi = z.into_iter().fold(f64::NEG_INFINITY, f64::max);
            if let Some(layer) = layers.iter().position(|(_, span)| lo >= span[0] && hi <= span[1]) {
                let t = layers[layer].1[1] - layers[layer].1[0];
                let inverse_delta = (std::f64::consts::PI * max_frequency_hz * super::super::MU0 * mesh.conductivity_s_m[i]).sqrt();
                if !inverse_delta.is_finite() { return Err(error("MIXED_FOIL_RANGE", "invalid skin-depth screen")); }
                stratified[layer] |= t * inverse_delta > 0.2;
            }
        }
    }
    let mut by_key = BTreeMap::<(usize, usize, u64, u64), usize>::new();
    let mut candidates = Vec::<Candidate>::new();
    let mut groups = vec![None; nt];
    let mut barrel_cell = vec![false; nt];
    for (i, tet) in mesh.mesh.copper_tetrahedra.iter().enumerate() {
        barrel_cell[i] = tet
            .source_ids
            .iter()
            .any(|id| barrels.contains(id.as_str()));
    }
    let barrel_index: BTreeMap<&str, usize> = mesh
        .barrel_ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let barrel_of = |i: usize| -> Option<usize> {
        let mut found = None;
        for id in &mesh.mesh.copper_tetrahedra[i].source_ids {
            if let Some(&b) = barrel_index.get(id.as_str()) {
                if found.is_some_and(|x| x != b) {
                    return None;
                }
                found = Some(b);
            }
        }
        found
    };
    let (mut barrel_segments, mut barrel_fallback_columns, barrel_sliver_columns, sliver_list) = if barrel_model == BarrelModel::ThinWall {
        plan_barrel_segments(mesh, &layers, &barrel_cell, &barrel_of, start, wall)?
    } else {
        (Vec::new(), 0, 0, Vec::new())
    };
    // P7_BARREL_NOISE=1: wall columns with a negligible conductance share get no current mode
    // (owner Dropped, accounted with the sliver columns). Off in the sole-link retry, which
    // keeps every column when dropping would disconnect a conductor.
    let triangle_area = |t: usize| {
        let [a, b, c] = mesh.planar_triangles[t].map(|i| mesh.mesh.nodes_m[i]);
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
    };
    if env_flag("P7_BARREL_PROBE") {
        barrel_probe(mesh, &barrel_segments, &triangle_area);
    }
    let mut barrel_noise_cells = Vec::<usize>::new();
    let mut barrel_noise_columns_count = 0usize;
    let mut barrel_noise_volume = Sum::default();
    if !keep_sole_link_slivers && super::super::p7_default_on("P7_BARREL_NOISE") && !barrel_segments.is_empty() {
        let noise = barrel_noise_columns(&barrel_segments, &triangle_area);
        barrel_noise_columns_count = noise.len();
        let (dropped, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut barrel_segments)
            .into_iter()
            .partition(|s| noise.contains(&(s.barrel, s.triangle)));
        barrel_segments = kept;
        for s in dropped {
            for &i in &s.cells {
                barrel_noise_volume = barrel_noise_volume.add(Sum::from(mesh.mesh.copper_tetrahedra[i].volume_m3));
            }
            barrel_noise_cells.extend(s.cells);
        }
        if env_flag("P7_BARREL_PROBE") {
            eprintln!("P7_BARREL_NOISE dropped_columns={barrel_noise_columns_count} cells={}", barrel_noise_cells.len());
        }
    }
    let v3 = barrel_model == BarrelModel::ThinWall;
    let mut sliver_cell = vec![false; nt];
    for &i in &sliver_list {
        sliver_cell[i] = true;
    }
    // Radial collapse of the thin wall: every ring vertex of a column is merged
    // (per copper layer) with the outermost vertex reached along increasing
    // distance from the bore. Different azimuthal vertices are never merged.
    let mut radial_rep = BTreeMap::<usize, usize>::new();
    if !barrel_segments.is_empty() {
        let nl = mesh.field_copper.len().saturating_sub(mesh.barrel_ids.len());
        let mut failed = BTreeSet::new();
        let mut by_barrel = BTreeMap::<usize, BTreeSet<usize>>::new();
        for s in &barrel_segments {
            by_barrel.entry(s.barrel).or_default().insert(s.triangle);
        }
        for (b, triangles) in by_barrel {
            check_time(start, wall)?;
            let rings: Vec<Vec<[f64; 2]>> = match mesh.field_copper.get(nl + b) {
                Some((poly, _, true)) => poly
                    .0
                    .iter()
                    .flat_map(|p| p.interiors())
                    .map(|r| r.0.iter().map(|c| [c.x, c.y]).collect())
                    .collect(),
                _ => Vec::new(),
            };
            if rings.is_empty() {
                failed.insert(b);
                continue;
            }
            let mut neighbors = BTreeMap::<usize, BTreeSet<usize>>::new();
            for &t in &triangles {
                let ids = mesh.planar_triangles[t];
                for k in 0..3 {
                    neighbors.entry(ids[k]).or_default().insert(ids[(k + 1) % 3]);
                    neighbors.entry(ids[(k + 1) % 3]).or_default().insert(ids[k]);
                }
            }
            let distance: BTreeMap<usize, f64> = neighbors
                .keys()
                .map(|&v| (v, ring_distance(mesh.planar_nodes_mm[v], &rings)))
                .collect();
            let mut next = BTreeMap::<usize, usize>::new();
            for (&v, around) in &neighbors {
                let mut best: Option<(f64, usize)> = None;
                for &w in around {
                    let length = (mesh.planar_nodes_mm[v][0] - mesh.planar_nodes_mm[w][0])
                        .hypot(mesh.planar_nodes_mm[v][1] - mesh.planar_nodes_mm[w][1]);
                    let slope = (distance[&w] - distance[&v]) / length;
                    if slope > 0.1 && best.is_none_or(|(s, _)| slope > s) {
                        best = Some((slope, w));
                    }
                }
                if let Some((_, w)) = best {
                    next.insert(v, w);
                }
            }
            for &v in neighbors.keys() {
                let mut at = v;
                while let Some(&w) = next.get(&at) {
                    at = w;
                }
                radial_rep.insert(v, at);
            }
        }
        if !failed.is_empty() {
            let (keep, drop): (Vec<_>, Vec<_>) = barrel_segments.into_iter().partition(|s| !failed.contains(&s.barrel));
            barrel_fallback_columns += drop.iter().map(|s| s.triangle).collect::<BTreeSet<_>>().len();
            barrel_segments = keep;
        }
    }
    let mut barrel_prism_cell = vec![false; nt];
    for s in &barrel_segments {
        for &c in &s.cells {
            barrel_prism_cell[c] = true;
        }
    }
    for &c in &barrel_noise_cells {
        barrel_prism_cell[c] = true;
    }
    for (i, tet) in mesh.mesh.copper_tetrahedra.iter().enumerate() {
        check_time(start, wall)?;
        let z = tet.nodes.map(|j| mesh.mesh.nodes_m[j][2]);
        let lo = z.into_iter().fold(f64::INFINITY, f64::min);
        let hi = z.into_iter().fold(f64::NEG_INFINITY, f64::max);
        if barrel_prism_cell[i] {
            continue;
        }
        let Some(layer) = layers
            .iter()
            .position(|(_, span)| lo >= span[0] && hi <= span[1])
        else {
            continue;
        };
        let triangle = tet.planar_triangle;
        let next = candidates.len();
        let span = if stratified[layer] { [lo, hi] } else { *layers[layer].1 };
        let group = *by_key.entry((layer, triangle, span[0].to_bits(), span[1].to_bits())).or_insert_with(|| {
            candidates.push(Candidate {
                layer,
                triangle,
                span,
                cells: Vec::new(),
                volume: 0.,
                foil_density: None,
                dropped: false,
            });
            next
        });
        groups[i] = Some(group);
        candidates[group].cells.push(i);
    }
    let mut relaxed_columns = 0usize;
    let mut dropped_columns = barrel_noise_columns_count;
    let mut dropped_volume = barrel_noise_volume;
    for group in &mut candidates {
        let thickness = group.span[1] - group.span[0];
        let triangle = mesh.planar_triangles[group.triangle];
        let [a, b, c] = triangle.map(|i| mesh.mesh.nodes_m[i]);
        let twice = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
        let longest = (a[0] - b[0])
            .hypot(a[1] - b[1])
            .max((a[0] - c[0]).hypot(a[1] - c[1]))
            .max((b[0] - c[0]).hypot(b[1] - c[1]));
        let expected = 0.5 * twice * thickness;
        let volume = group
            .cells
            .iter()
            .fold(Sum::default(), |sum, &i| {
                sum.add(Sum::from(mesh.mesh.copper_tetrahedra[i].volume_m3))
            })
            .value();
        group.volume = volume;
        let has_contested_pad_column = !collisions.is_empty()
            && group.cells.iter().any(|&i| {
                mesh.mesh.copper_tetrahedra[i]
                    .nodes
                    .iter()
                    .any(|node| collisions.contains(&(layers[group.layer].0.as_str(), node % nxy)))
            });
        let conductivity = mesh.conductivity_s_m[group.cells[0]];
        let delta = (1.
            / (std::f64::consts::PI * max_frequency_hz * super::super::MU0 * conductivity))
            .sqrt();
        let altitude = twice / longest;
        let aspect_ok = thickness <= altitude / 4.;
        // V3: skin depth, not mesh triangle shape, decides planarity (policy: approximation).
        let relaxed_ok = v3 && altitude >= thickness / 200.;
        let uniform = group
            .cells
            .iter()
            .all(|&i| (!barrel_cell[i] || (v3 && sliver_cell[i])) && mesh.conductivity_s_m[i] == conductivity);
        if !has_contested_pad_column
            && expected.is_finite()
            && expected > 0.
            && (volume - expected).abs() <= 1e-8 * expected
            && (aspect_ok || relaxed_ok)
            && (delta >= 3. * thickness
                || (copper_skin_profile::foil_sheet_enabled() && (thickness <= delta || copper_skin_profile::foil_sheet_scales_resistance())))
            && uniform
        {
            let density = 1. / (thickness * (twice / 2.).sqrt());
            if density.is_finite() && density > 0. {
                group.foil_density = Some(density);
                relaxed_columns += usize::from(!aspect_ok);
            }
        }
        // Policy: noise. Columns below thickness/200 and annulus-source slivers that
        // are not foil carry no current mode; nodes stay merged with neighbours.
        if v3
            && group.foil_density.is_none()
            && !has_contested_pad_column
            && (altitude < thickness / 200. || group.cells.iter().all(|&i| sliver_cell[i]))
        {
            group.dropped = true;
            dropped_columns += 1;
            dropped_volume = dropped_volume.add(Sum::from(volume));
        }
    }
    let mut dropped_volume = dropped_volume.value();
    let copper_volume = mesh
        .mesh
        .copper_tetrahedra
        .iter()
        .fold(Sum::default(), |sum, t| sum.add(Sum::from(t.volume_m3)))
        .value();
    let dropped_fraction = if copper_volume > 0. { dropped_volume / copper_volume } else { 0. };
    // Three tiers: <=1e-6 noise, <=1e-3 approximation (recorded as quality.sliverPolicy), above that refused.
    if !(dropped_fraction <= 1e-3) {
        return Err(error(
            "MIXED_FOIL_NOISE",
            &format!(
                "dropped sliver copper volume {dropped_volume:e} m3 in {dropped_columns} columns exceeds 1e-3 of copper volume {copper_volume:e} m3"
            ),
        ));
    }
    // A sliver column that is the only galvanic link inside a conductor (union noise: two copper
    // pieces joined across a sub-grid neck) must not be dropped. Keep the minimal set of such columns
    // as ordinary volume; the physical connectivity check below stays unchanged.
    if keep_sole_link_slivers && dropped_columns > 0 {
        let mut comp_of = vec![usize::MAX; nv];
        for (c, component) in full.components.iter().enumerate() {
            for &p in component {
                comp_of[p] = c;
            }
        }
        let mut uf: Vec<usize> = (0..nv).collect();
        for (i, cell) in full.cells.iter().enumerate() {
            if !groups[i].is_some_and(|j| candidates[j].dropped) {
                for &p in &cell.potentials[1..] {
                    merge(&mut uf, cell.potentials[0], p);
                }
            }
        }
        // Foil and dropped columns share their through-thickness nodes (merged later in `parent`),
        // so those nodes are already connected and must not make a sliver look necessary.
        for candidate in candidates.iter().filter(|c| c.dropped || c.foil_density.is_some()) {
            let mut columns = BTreeMap::new();
            for &i in &candidate.cells {
                for (&node, &dof) in mesh.mesh.copper_tetrahedra[i].nodes.iter().zip(&full.cells[i].potentials) {
                    if let Some(&previous) = columns.get(&(node % nxy)) {
                        merge(&mut uf, previous, dof);
                    } else {
                        columns.insert(node % nxy, dof);
                    }
                }
            }
        }
        let mut kept = 0usize;
        for group in candidates.iter_mut().filter(|g| g.dropped) {
            let mut needed = false;
            for &i in &group.cells {
                let ps = &full.cells[i].potentials;
                if ps.iter().any(|&p| comp_of[p] == comp_of[ps[0]] && root(&uf, p) != root(&uf, ps[0])) {
                    needed = true;
                    break;
                }
            }
            if needed {
                for &i in &group.cells {
                    let ps = &full.cells[i].potentials;
                    for &p in &ps[1..] {
                        merge(&mut uf, ps[0], p);
                    }
                }
                group.dropped = false;
                dropped_columns -= 1;
                dropped_volume = dropped_volume - group.volume;
                kept += 1;
            }
        }
        if kept > 0 {
            eprintln!("P7_MIXED_SLIVER_KEPT columns={kept} (sole galvanic links, kept as volume)");
        }
    }
    // One complete prism on the other side of each real conductor face next
    // to mandatory volume stays volumetric. This is a starting partition,
    // not a field-decay bound. No XY overlap alone creates a connection.
    let mandatory: Vec<_> = groups
        .iter()
        .map(|g| g.is_none_or(|j| candidates[j].foil_density.is_none() && !candidates[j].dropped))
        .collect();
    let mut halo = BTreeSet::new();
    let mut faces = BTreeMap::<[usize; 3], (usize, bool)>::new();
    for (i, tet) in mesh.mesh.copper_tetrahedra.iter().enumerate() {
        check_time(start, wall)?;
        for skip in 0..4 {
            let mut f = [
                tet.nodes[(skip + 1) % 4],
                tet.nodes[(skip + 2) % 4],
                tet.nodes[(skip + 3) % 4],
            ];
            f.sort_unstable();
            if let Some((j, shared)) = faces.get_mut(&f) {
                if *shared {
                    return Err(error("MIXED_FOIL_GEOMETRY", "nonmanifold copper face"));
                }
                *shared = true;
                // A thin-wall barrel prism never forces its neighbours to volume.
                if barrel_prism_cell[i] || barrel_prism_cell[*j] {
                    continue;
                }
                if mandatory[i] && !mandatory[*j] {
                    halo.insert(groups[*j].unwrap());
                }
                if mandatory[*j] && !mandatory[i] {
                    halo.insert(groups[i].unwrap());
                }
            } else {
                faces.insert(f, (i, false));
            }
        }
    }
    drop(faces);
    for j in halo {
        candidates[j].foil_density = None;
    }
    let mut owners = vec![Owner::Volume; nt];
    for &i in &barrel_noise_cells {
        owners[i] = Owner::Dropped;
    }
    let mut current_map = vec![None; full.current_count()];
    let mut parent: Vec<_> = (0..nv).collect();
    let mut foils = Vec::new();
    let mut nc = 0;
    for candidate in candidates {
        if candidate.dropped {
            // Noise column: no mode, but its through-thickness nodes stay merged.
            let mut columns = BTreeMap::new();
            for &i in &candidate.cells {
                owners[i] = Owner::Dropped;
                for (&node, &dof) in mesh.mesh.copper_tetrahedra[i].nodes.iter().zip(&full.cells[i].potentials) {
                    if let Some(&previous) = columns.get(&(node % nxy)) {
                        merge(&mut parent, previous, dof);
                    } else {
                        columns.insert(node % nxy, dof);
                    }
                }
            }
            continue;
        }
        let Some(density) = candidate.foil_density else {
            continue;
        };
        let index = foils.len();
        let mut columns = BTreeMap::new();
        for &i in &candidate.cells {
            let tet = &mesh.mesh.copper_tetrahedra[i];
            let cell = &full.cells[i];
            owners[i] = Owner::Foil(index);
            let weight = density / cell.density_per_amp_m2;
            if !weight.is_finite() || weight <= 0. {
                return Err(error(
                    "MIXED_FOIL_RANGE",
                    "invalid physical current normalization",
                ));
            }
            for k in 0..2 {
                current_map[3 * i + k] = Some(CurrentEntry {
                    mode: nc + k,
                    weight,
                });
            }
            for (&node, &dof) in tet.nodes.iter().zip(&cell.potentials) {
                let planar = node % nxy;
                if let Some(&previous) = columns.get(&planar) {
                    merge(&mut parent, previous, dof);
                } else {
                    columns.insert(planar, dof);
                }
            }
        }
        foils.push(FoilPrism {
            layer: layers[candidate.layer].0.clone(),
            planar_triangle: candidate.triangle,
            z_span_m: candidate.span,
            tetrahedra: candidate.cells,
            first_current: nc,
            volume_m3: candidate.volume,
        });
        nc += 2;
    }
    // Thin-wall barrel prisms: one uniform axial current mode per prism.
    // Ring vertices merge per copper layer (and radially onto the outer ring
    // vertex); dielectric-plane nodes join the nearest copper layer.
    let mut barrels_out = Vec::<BarrelPrism>::with_capacity(barrel_segments.len());
    let mut ring_groups = BTreeMap::<(usize, usize), usize>::new();
    let mut barrel_ids_used = BTreeSet::new();
    let layer_of = |z: f64| -> usize {
        let mut best = (f64::INFINITY, 0usize);
        for (k, (_, span)) in layers.iter().enumerate() {
            let d = (span[0] - z).max(z - span[1]).max(0.);
            if d < best.0 {
                best = (d, k);
            }
        }
        best.1
    };
    for segment in barrel_segments {
        check_time(start, wall)?;
        let ids = mesh.planar_triangles[segment.triangle];
        let [a, b, c] = ids.map(|i| mesh.mesh.nodes_m[i]);
        let area = 0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
        let density = 1. / area;
        let mut volume = Sum::default();
        for &i in &segment.cells {
            let tet = &mesh.mesh.copper_tetrahedra[i];
            let cell = &full.cells[i];
            owners[i] = Owner::Barrel(barrels_out.len());
            let weight = density / cell.density_per_amp_m2;
            if !weight.is_finite() || weight <= 0. {
                return Err(error(
                    "MIXED_FOIL_RANGE",
                    "invalid physical current normalization",
                ));
            }
            current_map[3 * i + 2] = Some(CurrentEntry { mode: nc, weight });
            volume = volume.add(Sum::from(tet.volume_m3));
            for (&node, &dof) in tet.nodes.iter().zip(&cell.potentials) {
                let planar = node % nxy;
                let rep = radial_rep.get(&planar).copied().unwrap_or(planar);
                let key = (rep, layer_of(mesh.mesh.nodes_m[node][2]));
                if let Some(&previous) = ring_groups.get(&key) {
                    merge(&mut parent, previous, dof);
                } else {
                    ring_groups.insert(key, dof);
                }
            }
        }
        barrel_ids_used.insert(segment.barrel);
        barrels_out.push(BarrelPrism {
            barrel_id: mesh.barrel_ids[segment.barrel].clone(),
            planar_triangle: segment.triangle,
            z_span_m: segment.span,
            tetrahedra: segment.cells,
            mode: nc,
            volume_m3: volume.value(),
            area_m2: area,
        });
        nc += 1;
    }
    drop(ring_groups);
    for (i, owner) in owners.iter().enumerate() {
        if *owner == Owner::Volume {
            for k in 0..3 {
                current_map[3 * i + k] = Some(CurrentEntry {
                    mode: nc + k,
                    weight: 1.,
                });
            }
            nc += 3;
        }
    }
    if dropped_columns > 0 {
        // A noise column can hold hanging nodes (a polygon vertex on a neighbour's
        // edge). Collapse each onto the nearest node of the same cell that is
        // carried by a current mode, so connectivity and KCL stay exact.
        let mut attached = vec![false; nv];
        for (i, cell) in full.cells.iter().enumerate() {
            if owners[i] != Owner::Dropped {
                for &p in &cell.potentials {
                    attached[root(&parent, p)] = true;
                }
            }
        }
        for _ in 0..4 {
            let mut changed = false;
            for (i, cell) in full.cells.iter().enumerate() {
                if owners[i] != Owner::Dropped {
                    continue;
                }
                let nodes = &mesh.mesh.copper_tetrahedra[i].nodes;
                for k in 0..4 {
                    if attached[root(&parent, cell.potentials[k])] {
                        continue;
                    }
                    let d = |m: usize| {
                        let (a, b) = (mesh.mesh.nodes_m[nodes[k]], mesh.mesh.nodes_m[nodes[m]]);
                        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
                    };
                    let near = (0..4)
                        .filter(|&m| attached[root(&parent, cell.potentials[m])])
                        .min_by(|&x, &y| d(x).total_cmp(&d(y)));
                    if let Some(m) = near {
                        merge(&mut parent, cell.potentials[m], cell.potentials[k]);
                        let r = root(&parent, cell.potentials[m]);
                        attached[r] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
    let mut ids = BTreeMap::new();
    let potential_map: Vec<_> = (0..nv)
        .map(|i| {
            let next = ids.len();
            *ids.entry(root(&parent, i)).or_insert(next)
        })
        .collect();
    if dropped_columns > 0 {
        // Dropping must not disconnect any conductor: current-carrying cells alone
        // must still join every mapped potential of each galvanic component.
        let mut conn: Vec<usize> = (0..ids.len()).collect();
        for (i, cell) in full.cells.iter().enumerate() {
            if owners[i] != Owner::Dropped {
                let first = potential_map[cell.potentials[0]];
                for &p in &cell.potentials[1..] {
                    merge(&mut conn, first, potential_map[p]);
                }
            }
        }
        for component in &full.components {
            let first = root(&conn, potential_map[component[0]]);
            if component.iter().any(|&p| root(&conn, potential_map[p]) != first) {
                return Err(error(
                    "MIXED_FOIL_CONTACT",
                    "dropped sliver columns would disconnect a conductor",
                ));
            }
        }
    }
    let mut known_components = BTreeSet::new();
    let mut potential_components = Vec::new();
    for component in &full.components {
        let mapped: BTreeSet<_> = component.iter().map(|&i| potential_map[i]).collect();
        if mapped.iter().any(|&i| !known_components.insert(i)) {
            return Err(error(
                "MIXED_FOIL_CONTACT",
                "restriction joins disconnected galvanic islands",
            ));
        }
        potential_components.push(mapped.into_iter().collect());
    }
    let full_electrodes: Vec<_> = electrode_nodes
        .iter()
        .map(|nodes| full.node_potential[&nodes[0]])
        .collect();
    let electrodes: Vec<_> = full_electrodes.iter().map(|&i| potential_map[i]).collect();
    if electrodes.iter().copied().collect::<BTreeSet<_>>().len() != electrodes.len() {
        return Err(error(
            "MIXED_FOIL_CONTACT",
            "depth restriction shorts distinct physical electrodes; retain their local volume",
        ));
    }
    let mut resistance = vec![Sum::default(); nc];
    let mut projected = BTreeMap::<(usize, usize), Sum>::new();
    let mut col_scale = vec![0.; nc];
    // P7_BARREL_SHEET=2: the wall resistance is scaled by Re(kt coth kt) for t/delta of the wall at the
    // profile upper frequency (approximation, one value for the band). 1 only keeps one radial layer.
    let barrel_ratio: BTreeMap<&str, f64> = if copper_skin_profile::barrel_sheet_scales_resistance() {
        mesh.copper_skin_barrels
            .iter()
            .map(|b| {
                let x = b.maximum_radial_step_m * b.shells as f64 / b.skin_depth_m;
                (b.id.as_str(), copper_skin_profile::foil_resistance_ratio(x))
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    // P7_FOIL_SHEET=2: the same Re(kt coth kt) multiplier for each sheet at the band upper frequency.
    let foil_resistance_scale: Vec<f64> = foils.iter().map(|f: &FoilPrism| {
        if !copper_skin_profile::foil_sheet_scales_resistance() { return 1.; }
        let sigma = mesh.conductivity_s_m[f.tetrahedra[0]];
        // P7_FOIL_SHEET=4: only layers with t/delta > 1 are split into two half-sheets; t/delta <= 1 stays as =1 (no multiplier).
        if copper_skin_profile::foil_sheet_half_sheets() {
            let layer_t = mesh.layer_spans.get(&f.layer).map_or(0., |s| s[1] - s[0]);
            if !copper_skin_profile::thickness_over_skin(layer_t, sigma, max_frequency_hz).is_some_and(|x| x > 1.) { return 1.; }
        }
        copper_skin_profile::thickness_over_skin(f.z_span_m[1] - f.z_span_m[0], sigma, max_frequency_hz)
            .map_or(1., copper_skin_profile::foil_sheet_multiplier)
    }).collect();
    let barrel_resistance_scale: Vec<f64> = barrels_out.iter()
        .map(|b| barrel_ratio.get(b.barrel_id.as_str()).copied().unwrap_or(1.)).collect();
    for (i, cell) in full.cells.iter().enumerate() {
        let scale = match owners[i] {
            Owner::Barrel(p) => barrel_resistance_scale[p],
            Owner::Foil(p) => foil_resistance_scale[p],
            _ => 1.,
        };
        for k in 0..3 {
            let Some(entry) = current_map[3 * i + k] else {
                continue;
            };
            resistance[entry.mode] = resistance[entry.mode].add(
                Sum::from(cell.resistance_ohm * scale)
                    .mul(entry.weight)
                    .mul(entry.weight),
            );
            for j in 0..4 {
                let term = Sum::from(cell.incidence[j][k]).mul(entry.weight);
                let key = (potential_map[cell.potentials[j]], entry.mode);
                let previous = projected.entry(key).or_default();
                *previous = previous.add(term);
                col_scale[entry.mode] += term.value().abs();
            }
        }
    }
    let resistance_ohm: Vec<_> = resistance.into_iter().map(Sum::value).collect();
    let incidence: Vec<_> = projected
        .into_iter()
        .filter_map(|((i, j), v)| {
            let v = v.value();
            (v != 0.).then_some((i, j, v))
        })
        .collect();
    let mut balance = vec![Sum::default(); nc];
    for &(_, j, v) in &incidence {
        balance[j] = balance[j].add(Sum::from(v));
    }
    if resistance_ohm.iter().any(|v| !v.is_finite() || *v <= 0.)
        || balance
            .iter()
            .zip(col_scale)
            .any(|(q, s)| !s.is_finite() || !q.value().is_finite() || q.value().abs() > 1e-12 * s)
    {
        return Err(error(
            "MIXED_FOIL_RANGE",
            "finite positive projected R and conservative incidence required",
        ));
    }
    check_time(start, wall)?;
    Ok(MixedMap {
        mesh,
        full,
        owners,
        foils,
        barrel_count: barrel_ids_used.len(),
        barrels: barrels_out,
        barrel_model,
        barrel_fallback_columns,
        barrel_sliver_columns,
        foil_aspect_relaxed_columns: relaxed_columns,
        sliver_columns_dropped: dropped_columns,
        dropped_volume_m3: dropped_volume,
        dropped_volume_fraction: dropped_fraction,
        electrodes,
        potential_components,
        incidence,
        resistance_ohm,
        foil_resistance_scale,
        barrel_resistance_scale,
        potential_map,
        full_electrodes,
        current_map,
        planned_bytes,
    })
}
impl MixedMap<'_> {
    fn eligible_skin_groups(&self, upper_hz: f64, budget: AllocationBudget, start: Instant, wall: Duration) -> Result<Vec<Vec<&FoilPrism>>> {
        check_time(start, wall)?;
        if !upper_hz.is_finite() || upper_hz <= 0. { return Err(error("HYBRID_FREQUENCY", "finite positive frequency required")); }
        budget.check(self.foils.len().checked_mul(512).ok_or_else(||error("MEMORY_LIMIT", "skin group storage overflow"))?)?;
        let mut groups = BTreeMap::<(&str,usize),Vec<&FoilPrism>>::new();
        for foil in &self.foils { check_time(start,wall)?; groups.entry((&foil.layer,foil.planar_triangle)).or_default().push(foil); }
        let mut eligible=Vec::new();
        for ((layer,_),mut slabs) in groups {
            check_time(start,wall)?;
            if slabs.len()<2 {continue;}
            slabs.sort_by(|a,b|a.z_span_m[0].total_cmp(&b.z_span_m[0]));
            let Some(&span)=self.mesh.layer_spans.get(layer) else {return Err(error("HYBRID_INPUT", "foil layer span missing"));};
            if slabs[0].z_span_m[0]!=span[0] || slabs.last().unwrap().z_span_m[1]!=span[1] || slabs.windows(2).any(|w|w[0].z_span_m[1]!=w[1].z_span_m[0]) {continue;}
            let Some(&first)=slabs[0].tetrahedra.first() else {return Err(error("HYBRID_INPUT", "empty foil"));};
            let sigma=self.mesh.conductivity_s_m[first];
            if !sigma.is_finite() || sigma<=0. {return Err(error("HYBRID_INPUT", "invalid conductor material"));}
            if slabs.iter().any(|foil|foil.tetrahedra.iter().any(|&i|self.mesh.conductivity_s_m[i]!=sigma)) {continue;}
            let inverse=(std::f64::consts::PI*upper_hz*super::super::MU0*sigma).sqrt();
            if !inverse.is_finite() {return Err(error("HYBRID_FREQUENCY", "invalid skin depth"));}
            if (span[1]-span[0])*inverse>0.2 {eligible.push(slabs);}
        }
        Ok(eligible)
    }

    /// Count eligible groups before reserving harmonic vectors. Scratch bound is 512 bytes per foil.
    pub(super) fn skin_hint_group_count(&self, upper_hz:f64, budget:AllocationBudget, start:Instant, wall:Duration)->Result<usize> {
        Ok(self.eligible_skin_groups(upper_hz,budget,start,wall)?.len())
    }

    /// Experimental thickness-profile hints; the caller must train and audit them.
    /// These hints do not stamp loss or assert continuity or RF accuracy.
    pub(super) fn skin_current_hints(&self, seeds: &[Vec<f64>], upper_hz: f64, budget: AllocationBudget, start: Instant, wall: Duration) -> Result<Vec<Vec<f64>>> {
        check_time(start, wall)?;
        if !upper_hz.is_finite() || upper_hz <= 0. || seeds.iter().any(|s| s.len() != self.current_count() || s.iter().any(|v| !v.is_finite())) {
            return Err(error("HYBRID_FREQUENCY", "finite frequency and complete current seeds required"));
        }
        let hint_count = seeds.len().checked_mul(4).ok_or_else(|| error("MEMORY_LIMIT", "skin hint count overflow"))?;
        let hint_bytes = hint_count.checked_mul(self.current_count()).and_then(|v|v.checked_mul(8)).and_then(|v|v.checked_add(hint_count.checked_mul(std::mem::size_of::<Vec<f64>>())?)).ok_or_else(|| error("MEMORY_LIMIT", "skin hint storage overflow"))?;
        let group_bytes = self.foils.len().checked_mul(512).ok_or_else(|| error("MEMORY_LIMIT", "skin group storage overflow"))?;
        let groups = self.eligible_skin_groups(upper_hz,budget,start,wall)?;
        if groups.is_empty() { return Ok(Vec::new()); }
        budget.check(hint_bytes.checked_add(group_bytes).ok_or_else(|| error("MEMORY_LIMIT", "skin hint plan overflow"))?)?;
        let mut hints = vec![vec![0.; self.current_count()]; hint_count];
        for slabs in groups {
            check_time(start, wall)?;
            let span = self.mesh.layer_spans[&slabs[0].layer];
            let sigma = self.mesh.conductivity_s_m[slabs[0].tetrahedra[0]];
            let thickness = span[1] - span[0];
            let mut z = Vec::with_capacity(slabs.len()+1);
            z.push(0.);
            z.extend(slabs.iter().map(|foil|foil.z_span_m[1]-span[0]));
            let basis = super::planar_skin::build(&z,upper_hz,sigma,super::super::MU0).map_err(|e|error("HYBRID_FREQUENCY",e))?;
            for (seed_index,seed) in seeds.iter().enumerate() {
                for (mode_index,profile) in basis.modes.iter().skip(1).take(4).enumerate() {
                    let out=&mut hints[4*seed_index+mode_index];
                    for (slab, &weight) in slabs.iter().zip(profile) {
                        let factor=weight*thickness.sqrt();
                        for axis in 0..2 {
                            let value=seed[slab.first_current+axis]*factor;
                            if !value.is_finite() {return Err(error("HYBRID_FREQUENCY","skin current hint outside finite range"));}
                            out[slab.first_current+axis]=value;
                        }
                    }
                }
            }
        }
        hints.retain(|hint|hint.iter().any(|&v|v != 0.));
        Ok(hints)
    }

    /// Post-assembly mixed storage. Keep the full-space/mesh conservative
    /// bound, but replace mixed ownership construction scratch with the live
    /// capacities of every owned field. Candidate maps and projected sums
    /// have been dropped when assemble() returns.
    pub fn retained_bytes(&self) -> Result<usize> {
        let mut bytes=self.full.planned_bytes;
        let mut add=|count:usize,width:usize| -> Result<()> {
            bytes=bytes.checked_add(count.checked_mul(width).ok_or_else(||error("MEMORY_LIMIT","mixed retained count overflow"))?)
                .ok_or_else(||error("MEMORY_LIMIT","mixed retained storage overflow"))?;
            Ok(())
        };
        add(1,std::mem::size_of::<Self>())?;
        add(self.owners.capacity(),std::mem::size_of::<Owner>())?;
        add(self.foils.capacity(),std::mem::size_of::<FoilPrism>())?;
        for foil in &self.foils {add(foil.layer.capacity(),1)?;add(foil.tetrahedra.capacity(),std::mem::size_of::<usize>())?;}
        add(self.barrels.capacity(),std::mem::size_of::<BarrelPrism>())?;
        for prism in &self.barrels {add(prism.barrel_id.capacity(),1)?;add(prism.tetrahedra.capacity(),std::mem::size_of::<usize>())?;}
        for values in [&self.electrodes,&self.potential_map,&self.full_electrodes] {add(values.capacity(),std::mem::size_of::<usize>())?;}
        add(self.potential_components.capacity(),std::mem::size_of::<Vec<usize>>())?;
        for component in &self.potential_components {add(component.capacity(),std::mem::size_of::<usize>())?;}
        add(self.incidence.capacity(),std::mem::size_of::<(usize,usize,f64)>())?;
        add(self.resistance_ohm.capacity(),std::mem::size_of::<f64>())?;
        add(self.current_map.capacity(),std::mem::size_of::<Option<CurrentEntry>>())?;
        Ok(bytes)
    }
    /// Mixed potential coordinate of a copper mesh node, if the node is copper.
    pub(super) fn mixed_potential(&self, node: usize) -> Option<usize> {
        self.full.node_potential.get(&node).map(|&full| self.potential_map[full])
    }
    pub fn potential_count(&self) -> usize {
        self.potential_components.iter().map(Vec::len).sum()
    }
    pub fn current_count(&self) -> usize {
        self.resistance_ohm.len()
    }

    pub fn lift_potentials(&self, mixed: &[f64]) -> Result<Vec<f64>> {
        dimensions(mixed, self.potential_count())?;
        Ok(self.potential_map.iter().map(|&i| mixed[i]).collect())
    }
    pub fn lift_currents(&self, mixed: &[f64]) -> Result<Vec<f64>> {
        dimensions(mixed, self.current_count())?;
        Ok(self
            .current_map
            .iter()
            .map(|e| e.map_or(0., |e| e.weight * mixed[e.mode]))
            .collect())
    }
    // Least-squares restriction of a current vector, rather than the transpose
    // used for dual operator actions. Each full coordinate has at most one owner.
    fn restrict_current_profile(&self, full: &[f64]) -> Result<Vec<f64>> {
        dimensions(full, self.full.current_count())?;
        let mut numerator = vec![0.; self.current_count()];
        let mut denominator = vec![0.; self.current_count()];
        for (&entry, &value) in self.current_map.iter().zip(full) {
            if let Some(e) = entry { numerator[e.mode] += e.weight * value; denominator[e.mode] += e.weight * e.weight; }
        }
        for (value, weight) in numerator.iter_mut().zip(denominator) {
            if weight > 0. { *value /= weight; }
            if !value.is_finite() { return Err(error("MIXED_FOIL_PROFILE", "invalid restricted current profile")); }
        }
        Ok(numerator)
    }
    pub fn project_potential_action(&self, full: &[f64]) -> Result<Vec<f64>> {
        dimensions(full, self.full.potential_count())?;
        let mut mixed = vec![Sum::default(); self.potential_count()];
        for (&i, &x) in self.potential_map.iter().zip(full) {
            mixed[i] = mixed[i].add(Sum::from(x));
        }
        Ok(mixed.into_iter().map(Sum::value).collect())
    }
    pub fn project_current_action(&self, full: &[f64]) -> Result<Vec<f64>> {
        dimensions(full, self.full.current_count())?;
        let mut mixed = vec![Sum::default(); self.current_count()];
        for (&e, &x) in self.current_map.iter().zip(full) {
            if let Some(e) = e {
                mixed[e.mode] = mixed[e.mode].add(Sum::from(x).mul(e.weight));
            }
        }
        Ok(mixed.into_iter().map(Sum::value).collect())
    }

    pub(super) fn original_representatives(&self) -> Vec<usize> {
        let mut original = vec![usize::MAX; self.potential_count()];
        for (i, &mixed) in self.potential_map.iter().enumerate() {
            original[mixed] = original[mixed].min(i);
        }
        for (&full, &mixed) in self.full_electrodes.iter().zip(&self.electrodes) {
            original[mixed] = full;
        }
        original
    }

    /// Common electrostatic trace on actual copper/dielectric faces. Internal
    /// foil/barrel material interfaces do not acquire additional charge rows.
    pub fn electric_trace(&self) -> Result<Vec<TraceRow>> {
        self.trace(Some(&self.potential_map))
    }

    fn trace(&self, mapping: Option<&[usize]>) -> Result<Vec<TraceRow>> {
        self.mesh
            .mesh
            .copper_interfaces
            .iter()
            .flat_map(|f| f.nodes)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|node| {
                let full = self.full.node_potential.get(&node).ok_or_else(|| {
                    error(
                        "MIXED_FOIL_TRACE",
                        "native surface lacks a physical conductor coordinate",
                    )
                })?;
                Ok(TraceRow {
                    node,
                    weights: vec![(mapping.map_or(*full, |map| map[*full]), 1.)],
                })
            })
            .collect()
    }
}
fn dimensions(v: &[f64], n: usize) -> Result<()> {
    if v.len() != n || v.iter().any(|v| !v.is_finite()) {
        return Err(error(
            "MIXED_FOIL_RANGE",
            "finite vector in the requested physical space required",
        ));
    }
    Ok(())
}

/// The private producer's observed, matched frequency grid. The implementation
/// publishes only checked samples, with spatial, quadrature and band coverage
/// explicitly unverified.
#[derive(Clone, Copy)]
pub struct CheckedGrid<'a> {
    pub frequencies_hz: &'a [f64],
    pub reference_ohm: &'a [f64],
    pub max_iterations: usize,
}

pub fn solve_checked(
    board: &crate::model::Board,
    request: &board_mesh::Request,
    grid: CheckedGrid<'_>,
    budget: AllocationBudget,
    wall: Duration,
) -> Result<serde_json::Value> {
    let start = Instant::now();
    let reserve = checked_reserve(request, grid, None)?;
    let mesh_budget=AllocationBudget { max_bytes:budget.max_bytes.checked_sub(reserve).ok_or_else(|| error("MEMORY_LIMIT", "mixed checked response exceeds budget"))?, ..budget };
    let mesh = board_mesh::build(board, request, mesh_budget, wall.saturating_sub(start.elapsed()))?;
    solve_checked_mesh(
        mesh,
        request,
        grid,
        None,
        &[],
        PartitionPolicy::Legacy,
        budget,
        start,
        wall,
    )
}

pub(super) fn checked_reserve(
    request: &board_mesh::Request,
    grid: CheckedGrid<'_>,
    skin: Option<copper_skin_profile::Profile>,
) -> Result<usize> {
    let operator=if skin.is_some() {CheckedOperator::SkinVolumeV3} else {CheckedOperator::Legacy};
    checked_reserve_policy(request,grid,skin,operator,PartitionPolicy::Legacy)
}

pub(super) fn checked_reserve_policy(request:&board_mesh::Request,grid:CheckedGrid<'_>,skin:Option<copper_skin_profile::Profile>,operator:CheckedOperator,policy:PartitionPolicy)->Result<usize> {
    operator.validate_partition(policy)?;
    if skin.is_some_and(|profile|!operator.admits(profile.upper_hz)) {
        return Err(error("BOARD_FOIL_POLICY","skin profile exceeds its explicit operator implementation band"));
    }
    if operator==CheckedOperator::HybridPlanarV3 && (skin.is_none() || skin.is_some_and(|p|!operator.admits(p.upper_hz) || grid.frequencies_hz.last().is_some_and(|&f|p.upper_hz<f))) {
        return Err(error("BOARD_FOIL_POLICY","hybrid-planar-v3 requires a covering hybrid skin profile"));
    }
    let CheckedGrid { frequencies_hz, reference_ohm, max_iterations }=grid;
    let port_count = request.ports.len();
    if !(1..=16).contains(&port_count)
        || frequencies_hz.is_empty()
        || frequencies_hz.len().saturating_mul(port_count) > 256
        || frequencies_hz.iter().any(|&f| !operator.admits(f))
        || frequencies_hz.windows(2).any(|f| f[0] >= f[1])
        || reference_ohm.len() != port_count
        || reference_ohm
            .iter()
            .any(|r| !r.is_finite() || *r <= 0. || !(1. / r).is_finite())
        || !(1..=16 + foil_reduced::CONTINUATION_EXTRA_MAX).contains(&max_iterations)
    {
        return Err(error(
            "MIXED_FOIL_GRID",
            &format!(
                "bounded {} exact grid, finite matched ports and enrichment budget required",
                operator.axis_scope()
            ),
        ));
    }
    let cut_ports = highband_cuts_enabled() && !request.cut_planes.is_empty();
    if !cut_ports && (!request.cut_planes.is_empty() || request.allow_insulated_copper_cuts) {
        return Err(error(
            "MIXED_FOIL_CONTACT",
            "closed physical copper and original finite pad faces required",
        ));
    }
    for electrode in &request.electrodes {
        if cut_ports && request.cut_planes.contains_key(&electrode.name) {
            continue;
        }
        if electrode
            .canonical_paths()?
            .iter()
            .any(|p| !p.starts_with("pads[") || !p.ends_with(']'))
        {
            return Err(error(
                "MIXED_FOIL_CONTACT",
                "original physical pad faces required",
            ));
        }
    }
    let maximum = skin.map_or(*frequencies_hz.last().unwrap(), |profile| profile.upper_hz);
    let (diagonal, epsilon) = screen_terms(request);
    let size = 2. * std::f64::consts::PI * maximum / 299792458. * diagonal * epsilon.sqrt();
    let band_kd = band_screen_kd()?;
    let limit = band_kd.unwrap_or(0.1);
    if !size.is_finite() || size > limit {
        let fmax = limit * 299792458. / (2. * std::f64::consts::PI * diagonal * epsilon.sqrt());
        return Err(error(
            "MIXED_FOIL_BAND",
            &format!("quasistatic electrical-size screen failed: size {size:.3} > {limit} at {maximum:.4e} Hz for a region of diagonal {:.2} mm; maximum valid frequency for this region {fmax:.4e} Hz", diagonal * 1e3),
        ));
    }

    let reserve = frequencies_hz.len() * port_count * (port_count * 256 + 4096) + 256 * 1024;
    Ok(reserve)
}

pub(super) fn solve_checked_mesh(
    mesh: BoardMesh, request:&board_mesh::Request, grid:CheckedGrid<'_>, skin:Option<copper_skin_profile::Profile>,
    packages:&[super::checked_packages::Validated], partition_policy:PartitionPolicy, budget:AllocationBudget, start:Instant, wall:Duration,
)->Result<serde_json::Value> {
    let operator=if skin.is_some(){CheckedOperator::SkinVolumeV3}else{CheckedOperator::Legacy};
    solve_checked_mesh_policy(mesh,request,grid,skin,packages,partition_policy,operator,budget,start,wall)
}

pub(super) fn solve_checked_mesh_policy(
    mesh: BoardMesh, request:&board_mesh::Request, grid:CheckedGrid<'_>, skin:Option<copper_skin_profile::Profile>,
    packages:&[super::checked_packages::Validated], partition_policy:PartitionPolicy, operator:CheckedOperator,
    budget:AllocationBudget,start:Instant,wall:Duration,
)->Result<serde_json::Value> {
    operator.validate_partition(partition_policy)?;
    solve_checked_mesh_impl_policy(&mesh,request,grid,skin,packages,None,partition_policy,operator,budget,start,wall)
}

/// Developer-only native interface proof on one already-built common mesh.
/// The unpartitioned control uses the same coordinates, terminals and fields.
#[doc(hidden)]
pub fn solve_checked_spatial_partition(
    mesh: &BoardMesh,
    request: &board_mesh::Request,
    grid: CheckedGrid<'_>,
    owners: Option<&[usize]>,
    max_trace_output_bytes: usize,
    budget: AllocationBudget,
    wall: Duration,
) -> Result<serde_json::Value> {
    let start = Instant::now();
    if owners.is_some_and(|ids| ids.len() != mesh.mesh.copper_tetrahedra.len()) {
        return Err(error(
            "SHARED_RETURN_OWNERSHIP",
            "exact original Cu tetrahedron ownership required",
        ));
    }
    solve_checked_mesh_impl(
        mesh,
        request,
        grid,
        None,
        &[],
        owners.map(|ids| (ids, max_trace_output_bytes)),
        PartitionPolicy::Legacy,
        budget,
        start,
        wall,
    )
}

/// Strictly conforming Z-axis straight extrusion only; all other geometries use
/// the existing mixed path. Components and return are physical, never nets.
fn uniform_section_profiles(mesh:&BoardMesh,map:&MixedMap<'_>,ports:&[[usize;2]],frequency:f64,budget:AllocationBudget,start:Instant,wall:Duration)->Result<Option<Vec<Vec<f64>>>> {
    use crate::extraction::section_transition::{self,Conductor,Triangle,Mesh};
    let nt=mesh.planar_triangles.len();let np=mesh.planar_nodes_mm.len();let nc=map.full.components.len();
    if nt==0||np==0||!(2..=17).contains(&nc)||ports.len()<2 {return Ok(None);}
    check_time(start,wall)?;let seed_start=Instant::now();let seed_wall=if wall == Duration::MAX { Duration::MAX } else { wall.saturating_sub(start.elapsed())/8 };
    let work_limit=budget.max_bytes/8;
    let source_bytes=mesh.mesh.copper_tetrahedra.iter().try_fold(0usize,|n,t|t.source_ids.iter().try_fold(n,|n,id|n.checked_add(id.len().checked_add(128)?)));
    let preflight=source_bytes.and_then(|bytes|map.full.potential_count().checked_mul(16).and_then(|n|n.checked_add(bytes))).and_then(|n|n.checked_add(map.full.cells.len().checked_mul(256)?)).and_then(|n|n.checked_add(np.checked_mul(128)?)).and_then(|n|n.checked_add(nt.checked_mul(128)?)).and_then(|n|n.checked_add(map.full.current_count().checked_mul(2*(nc-1))?.checked_mul(8)?));
    if preflight.is_none_or(|n|n>work_limit) {return Ok(None);}
    let mut component=vec![usize::MAX;map.full.potential_count()];for (c,nodes) in map.full.components.iter().enumerate(){for &i in nodes {component[i]=c;}}
    let reference=component[ports[0][1]];if reference==usize::MAX||ports.iter().any(|p|component[p[1]]!=reference||component[p[0]]==reference) {return Ok(None);}
    let first_slab=match mesh.mesh.copper_tetrahedra.iter().map(|t|t.slab).min(){Some(v)=>v,None=>return Ok(None)};
    let last_slab=mesh.mesh.copper_tetrahedra.iter().map(|t|t.slab).max().unwrap();let slab_count=last_slab-first_slab+1;
    let mut zs:Vec<f64>=mesh.mesh.copper_tetrahedra.iter().flat_map(|t|t.nodes.iter().map(|&i|mesh.mesh.nodes_m[i][2])).collect();zs.sort_by(f64::total_cmp);zs.dedup();
    if zs.len()!=slab_count+1 {return Ok(None);}
    if ports.iter().any(|p|p.iter().any(|&d|{let nodes=&map.full.mesh_nodes[d];nodes.is_empty()||!nodes.iter().all(|&i|mesh.mesh.nodes_m[i][2]==zs[0]||mesh.mesh.nodes_m[i][2]==zs[slab_count])})) {return Ok(None);}
    let ends=|d:usize|->Option<usize>{let nodes=&map.full.mesh_nodes[d];let z=mesh.mesh.nodes_m[*nodes.first()?][2];if nodes.iter().any(|&i|mesh.mesh.nodes_m[i][2]!=z){None}else if z==zs[0]{Some(0)}else if z==zs[slab_count]{Some(1)}else{None}};
    if ports.iter().any(|p|ends(p[0])!=ends(p[1])||ends(p[0]).is_none()) || !(0..2).all(|end|ports.iter().any(|p|ends(p[0])==Some(end)&&ends(p[1])==Some(end))) {return Ok(None);}
    let estimated=nt.checked_mul(slab_count).and_then(|n|n.checked_mul(128)).and_then(|n|n.checked_add(np.checked_mul(128)?)).and_then(|n|n.checked_add(map.full.current_count().checked_mul(2*(nc-1))?.checked_mul(8)?));
    if estimated.and_then(|n|n.checked_add(preflight.unwrap())).is_none_or(|n|n>work_limit) {return Ok(None);}
    let mut owner=vec![vec![None;nt];slab_count];let mut counts=vec![vec![0usize;nt];slab_count];let mut source=BTreeMap::new();let mut sigma=vec![None;nc];
    for (i,cell) in map.full.cells.iter().enumerate(){
        if i%256==0 {check_time(start,wall)?;if seed_start.elapsed()>=seed_wall{return Ok(None);}}
        let tet=&mesh.mesh.copper_tetrahedra[cell.mesh_cell];if tet.slab<first_slab||tet.slab>last_slab||tet.planar_triangle>=nt{return Ok(None);}let slab=tet.slab-first_slab;counts[slab][tet.planar_triangle]+=1;let c=component[cell.potentials[0]];
        if c==usize::MAX||cell.potentials.iter().any(|&p|component[p]!=c)||tet.source_ids.is_empty() {return Ok(None);}
        for id in &tet.source_ids {if source.insert(id.clone(),c).is_some_and(|old|old!=c){return Ok(None);}}
        if owner[slab][tet.planar_triangle].replace(c).is_some_and(|old|old!=c){return Ok(None);}
        let conductivity=mesh.conductivity_s_m[i];if !conductivity.is_finite()||conductivity<=0.||sigma[c].replace(conductivity).is_some_and(|old|old!=conductivity){return Ok(None);}
    }
    if owner.iter().any(|row|row!=&owner[0])||counts.iter().zip(&owner).any(|(counts,row)|counts.iter().zip(row).any(|(&n,c)|n!=if c.is_some(){3}else{0}))||sigma.iter().any(Option::is_none) {return Ok(None);}
    let mut dielectric=vec![vec![None;nt];slab_count];
    for t in &mesh.mesh.tetrahedra {if t.slab>=first_slab && t.slab<=last_slab {let slab=t.slab-first_slab;if t.planar_triangle>=nt||!t.epsilon_r.is_finite()||t.epsilon_r<=0. {return Ok(None);}if dielectric[slab][t.planar_triangle].replace(t.epsilon_r).is_some_and(|old|old!=t.epsilon_r) {return Ok(None);}}}
    if dielectric.iter().any(|row|row!=&dielectric[0]) {return Ok(None);}
    let names:Vec<_>=(0..nc).map(|c|source.iter().find(|(_,v)|**v==c).unwrap().0.as_str()).collect();
    let conductors:Vec<_>=names.iter().enumerate().map(|(i,id)|Conductor{id,conductivity_s_m:sigma[i].unwrap()}).collect();
    let nodes:Vec<_>=mesh.mesh.nodes_m[..np].iter().map(|p|[p[0],p[1]]).collect();
    let triangles:Vec<_>=mesh.planar_triangles.iter().enumerate().map(|(i,&nodes)|Triangle{nodes,conductor:owner[0][i]}).collect();
    let section=Mesh{nodes_m:&nodes,triangles:&triangles,conductors:&conductors,reference};
    let remaining=wall.saturating_sub(start.elapsed());if remaining.is_zero(){return Err(error("DEADLINE","section producer deadline"));}
    let deadline=if seed_wall == Duration::MAX { None } else { Some(seed_start.checked_add(seed_wall).ok_or_else(||error("DEADLINE","section producer deadline overflow"))?) };
    let transient=preflight.unwrap().checked_add(estimated.unwrap()).ok_or_else(||error("MEMORY_LIMIT","section producer plan overflow"))?;
    let work_budget=AllocationBudget{max_bytes:work_limit.saturating_sub(transient),..budget};
    let result=section_transition::solve_with_recovery(&section,frequency,0,&section_transition::Budget{max_bytes:work_budget.max_bytes,deadline},work_budget.max_bytes)
        .and_then(|(_,recovery)|super::section_return::lift(&section,&recovery,&mesh.mesh,&map.full,first_slab..last_slab+1,&source,work_budget));
    let profiles=match result {Ok(v)=>v,Err(e)=>{check_time(start,wall)?;eprintln!("P7_SECTION_HINT_FALLBACK reason={e}");return Ok(None);}};
    Ok(Some(profiles))
}

/// Optional sampled XY section currents. These are predictor coordinates;
/// the common physical operator and residual checks determine the solution.
fn planar_section_profiles(
    mesh: &BoardMesh,
    map: &MixedMap<'_>,
    ports: &[[usize; 2]],
    frequency: f64,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
) -> Result<Option<Vec<Vec<f64>>>> {
    use crate::extraction::section_transition::{self, Conductor, Mesh, Triangle};
    if ports.is_empty() || map.foils.is_empty() || map.foils.len() > 5000 {
        return Ok(None);
    }
    let estimate = map
        .full
        .current_count()
        .checked_mul(256)
        .and_then(|x| x.checked_add(map.foils.len().checked_mul(1024)?));
    if estimate.is_none_or(|x| x > budget.max_bytes / 4) {
        return Ok(None);
    }
    let mut component = vec![usize::MAX; map.full.potential_count()];
    for (c, nodes) in map.full.components.iter().enumerate() {
        for &n in nodes {
            component[n] = c;
        }
    }
    let reference = component[ports[0][1]];
    let deadline = if wall == Duration::MAX { None } else { Some(Instant::now()
        .checked_add(wall.saturating_sub(start.elapsed()) / 8)
        .unwrap_or(start)) };
    for axis in 0..2 {
        let transverse = 1 - axis;
        let mut breaks: Vec<_> = mesh.mesh.nodes_m[..mesh.planar_nodes_mm.len()]
            .iter()
            .map(|p| p[axis])
            .collect();
        breaks.sort_by(f64::total_cmp);
        breaks.dedup();
        let mut gaps: Vec<_> = breaks
            .windows(2)
            .filter_map(|w| ((w[1] - w[0]) > 1e-12).then_some([w[0], w[1]]))
            .collect();
        gaps.sort_by(|a, b| (b[1] - b[0]).total_cmp(&(a[1] - a[0])));
        for gap in gaps.into_iter().take(16) {
            check_time(start, wall)?;
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(None);
            }
            let cut = (gap[0] + gap[1]) * 0.5;
            let mut intervals = Vec::new();
            for foil in &map.foils {
                let points = mesh.planar_triangles[foil.planar_triangle]
                    .map(|i| [mesh.mesh.nodes_m[i][axis], mesh.mesh.nodes_m[i][transverse]]);
                let mut hits = Vec::new();
                for k in 0..3 {
                    let a = points[k];
                    let b = points[(k + 1) % 3];
                    if (a[0] < cut && b[0] > cut) || (b[0] < cut && a[0] > cut) {
                        hits.push(a[1] + (b[1] - a[1]) * (cut - a[0]) / (b[0] - a[0]));
                    }
                }
                if hits.len() != 2 {
                    continue;
                }
                hits.sort_by(f64::total_cmp);
                let cell = &map.full.cells[foil.tetrahedra[0]];
                let c = component[cell.potentials[0]];
                if c == usize::MAX || cell.potentials.iter().any(|&p| component[p] != c) {
                    return Ok(None);
                }
                intervals.push((
                    [hits[0], hits[1]],
                    foil.z_span_m,
                    c,
                    mesh.conductivity_s_m[foil.tetrahedra[0]],
                ));
            }
            let mut owners: Vec<_> = intervals.iter().map(|v| v.2).collect();
            owners.sort();
            owners.dedup();
            if !(2..=17).contains(&owners.len()) || !owners.contains(&reference) {
                continue;
            }
            let mut xs: Vec<_> = intervals.iter().flat_map(|v| v.0).collect();
            let mut zs: Vec<_> = intervals.iter().flat_map(|v| v.1).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup_by(|a,b| (*a-*b).abs() <= 64.*f64::EPSILON*a.abs().max(b.abs()).max(1e-12));
            zs.sort_by(f64::total_cmp);
            zs.dedup_by(|a,b| (*a-*b).abs() <= 64.*f64::EPSILON*a.abs().max(b.abs()).max(1e-12));
            if xs.len() > 64 || zs.len() > 64 || xs.len() < 2 || zs.len() < 2 {
                continue;
            }
            let dx = (xs[xs.len() - 1] - xs[0]).max(1e-6);
            let dz = (zs[zs.len() - 1] - zs[0]).max(1e-6);
            xs.insert(0, xs[0] - dx);
            xs.push(xs[xs.len() - 1] + dx);
            zs.insert(0, zs[0] - dz);
            zs.push(zs[zs.len() - 1] + dz);
            let mut sigmas = vec![None; owners.len()];
            let mut nodes = Vec::new();
            for &z in &zs {
                for &x in &xs {
                    nodes.push([x, z]);
                }
            }
            let mut triangles = Vec::new();
            for j in 0..zs.len() - 1 {
                for i in 0..xs.len() - 1 {
                    let x = (xs[i] + xs[i + 1]) * 0.5;
                    let z = (zs[j] + zs[j + 1]) * 0.5;
                    let mut owner = None;
                    for &(u, v, c, sigma) in &intervals {
                        if x > u[0] && x < u[1] && z > v[0] && z < v[1] {
                            let k = owners.binary_search(&c).unwrap();
                            if owner.is_some_and(|o| o != k)
                                || sigmas[k].is_some_and(|s| s != sigma)
                            {
                                return Ok(None);
                            }
                            owner = Some(k);
                            sigmas[k] = Some(sigma);
                        }
                    }
                    let a = j * xs.len() + i;
                    let b = a + 1;
                    let c = a + xs.len();
                    let d = c + 1;
                    triangles.push(Triangle {
                        nodes: [a, b, d],
                        conductor: owner,
                    });
                    triangles.push(Triangle {
                        nodes: [a, d, c],
                        conductor: owner,
                    });
                }
            }
            if sigmas.iter().any(Option::is_none) {
                continue;
            }
            let ids: Vec<_> = owners.iter().map(|x| format!("component-{x}")).collect();
            let conductors: Vec<_> = ids
                .iter()
                .enumerate()
                .map(|(i, id)| Conductor {
                    id,
                    conductivity_s_m: sigmas[i].unwrap(),
                })
                .collect();
            let section = Mesh {
                nodes_m: &nodes,
                triangles: &triangles,
                conductors: &conductors,
                reference: owners.binary_search(&reference).unwrap(),
            };
            let work = budget.max_bytes / 4 - estimate.unwrap();
            let recovery = match section_transition::solve_with_recovery(
                &section,
                frequency,
                0,
                &section_transition::Budget {
                    max_bytes: work,
                    deadline,
                },
                work,
            ) {
                Ok((_, r)) => r,
                Err(_) => continue,
            };
            let mut out = vec![vec![0.; map.full.current_count()]; 2 * (owners.len() - 1)];
            for foil in &map.foils {
                let tri = mesh.planar_triangles[foil.planar_triangle];
                let along = tri.iter().map(|&i| mesh.mesh.nodes_m[i][axis]).sum::<f64>() / 3.;
                if along <= gap[0] || along >= gap[1] {
                    continue;
                }
                let x = tri
                    .iter()
                    .map(|&i| mesh.mesh.nodes_m[i][transverse])
                    .sum::<f64>()
                    / 3.;
                let z = (foil.z_span_m[0] + foil.z_span_m[1]) * 0.5;
                let Some(i) = xs.windows(2).position(|v| x >= v[0] && x <= v[1]) else {
                    continue;
                };
                let Some(j) = zs.windows(2).position(|v| z >= v[0] && z <= v[1]) else {
                    continue;
                };
                let fx = (x - xs[i]) / (xs[i + 1] - xs[i]);
                let fz = (z - zs[j]) / (zs[j + 1] - zs[j]);
                let ti = 2 * (j * (xs.len() - 1) + i) + usize::from(fz > fx);
                let Some(row) = recovery.triangles.iter().find(|r| r.triangle_index == ti) else {
                    continue;
                };
                let c = component[map.full.cells[foil.tetrahedra[0]].potentials[0]];
                if owners[row.conductor] != c {
                    continue;
                }
                let weights = if fz <= fx {
                    [1. - fx, fx - fz, fz]
                } else {
                    [1. - fz, fx, fz - fx]
                };
                for &cell in &foil.tetrahedra {
                    for p in 0..owners.len() - 1 {
                        let density = map.full.cells[cell].density_per_amp_m2;
                        out[2 * p][3 * cell + axis] = (0..3)
                            .map(|k| weights[k] * row.jz_a_per_m2[p][k].real)
                            .sum::<f64>()
                            / density;
                        out[2 * p + 1][3 * cell + axis] = (0..3)
                            .map(|k| weights[k] * row.jz_a_per_m2[p][k].imaginary)
                            .sum::<f64>()
                            / density;
                    }
                }
            }
            if out.iter().any(|v| v.iter().any(|x| *x != 0.)) {
                return Ok(Some(out));
            }
        }
    }
    Ok(None)
}

fn solve_checked_mesh_impl(
    mesh:&BoardMesh,request:&board_mesh::Request,grid:CheckedGrid<'_>,skin:Option<copper_skin_profile::Profile>,
    packages:&[super::checked_packages::Validated],partition_request:Option<(&[usize],usize)>,partition_policy:PartitionPolicy,
    budget:AllocationBudget,start:Instant,wall:Duration,
)->Result<serde_json::Value> {
    let operator=if skin.is_some(){CheckedOperator::SkinVolumeV3}else{CheckedOperator::Legacy};
    solve_checked_mesh_impl_policy(mesh,request,grid,skin,packages,partition_request,partition_policy,operator,budget,start,wall)
}

fn solve_checked_mesh_impl_policy(
    mesh: &BoardMesh,
    request: &board_mesh::Request,
    grid: CheckedGrid<'_>,
    skin: Option<copper_skin_profile::Profile>,
    packages: &[super::checked_packages::Validated],
    partition_request: Option<(&[usize], usize)>,
    partition_policy: PartitionPolicy,
    operator: CheckedOperator,
    mut budget: AllocationBudget,
    start: Instant,
    wall: Duration,
) -> Result<serde_json::Value> {
    use serde_json::json;
    let reserve = checked_reserve_policy(request, grid, skin, operator, partition_policy)?
        .checked_add(super::checked_packages::output_reserve(
            packages,
            grid.frequencies_hz.len() * request.ports.len(),
        )).ok_or_else(||error("MEMORY_LIMIT","mixed output reservation overflow"))?;
    // This reserve belongs to the mixed solver, independently of any outer
    // serializer reservation. Admit it before map/operator allocation.
    budget.max_bytes=budget.max_bytes.checked_sub(reserve).ok_or_else(||error("MEMORY_LIMIT","mixed checked output exceeds budget"))?;
    let CheckedGrid {
        frequencies_hz,
        reference_ohm,
        max_iterations,
    } = grid;
    let requested_upper = *frequencies_hz.last().unwrap();
    let effective_upper = skin.map_or(requested_upper, |profile| profile.upper_hz);
    let provenance = mesh
        .source_mapping
        .iter()
        .try_fold(0usize, |n, (id, path, net, layer)| {
            n.checked_add(256)?.checked_add(
                id.len()
                    .checked_add(path.len())?
                    .checked_add(net.as_ref().map_or(0, String::len))?
                    .checked_add(layer.len())?
                    .checked_mul(6)?,
            )
        })
        .and_then(|n| {
            request.electrodes.iter().try_fold(n, |n, e| {
                n.checked_add(256)?
                    .checked_add(e.name.len().checked_mul(24)?)
            })
        })
        .and_then(|n| {
            mesh.barrel_ids.iter().try_fold(n, |n, id| {
                n.checked_add(256)?.checked_add(id.len().checked_mul(6)?)
            })
        })
        .and_then(|n| {
            if skin.is_none() {
                return Some(n);
            }
            let layer_bytes = request.stack.iter().filter(|layer| layer.copper).try_fold(
                1024usize,
                |sum, layer| {
                    sum.checked_add(512)?
                        .checked_add(layer.name.len().checked_mul(6)?)
                },
            )?;
            mesh.barrel_ids
                .iter()
                .try_fold(n.checked_add(layer_bytes)?, |sum, id| {
                    sum.checked_add(512)?.checked_add(id.len().checked_mul(6)?)
                })
        })
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed provenance plan overflow"))?;
    let provenance = provenance
        .checked_add(
            if partition_policy != PartitionPolicy::Legacy {
                4096
            } else {
                0
            },
        )
        .ok_or_else(|| error("MEMORY_LIMIT", "automatic partition metadata overflow"))?;
    budget.max_bytes = budget
        .max_bytes
        .checked_sub(provenance)
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed provenance exceeds budget"))?;

    let barrel_model = if operator == CheckedOperator::HybridPlanarV3 && partition_policy == PartitionPolicy::AutomaticHybridV1 {
        BarrelModel::hybrid_default()
    } else {
        BarrelModel::Volume
    };
    let map = prepare_model(
        mesh,
        &request.electrodes,
        effective_upper,
        budget,
        wall.saturating_sub(start.elapsed()),
        barrel_model,
    )?;
    let (automatic_partition, automatic_evidence) =
        if partition_policy == PartitionPolicy::AutomaticConductiveSchurV1 {
            check_time(start, wall)?;
            let originals = map.original_representatives();
            let mut retained: BTreeSet<_> = map.electrodes.iter().copied().collect();
            for component in &map.potential_components {
                if !component.iter().any(|i| retained.contains(i)) {
                    retained.insert(component[0]);
                }
            }
            let retained = retained.into_iter().map(|i| originals[i]).collect();
            let available = budget
                .max_bytes
                .checked_sub(map.planned_bytes)
                .ok_or_else(|| {
                    error(
                        "MEMORY_LIMIT",
                        "mixed map exhausts automatic topology budget",
                    )
                })?;
            let (partition, evidence) = Partition::automatic(
                &mesh.mesh,
                &map.full,
                &retained,
                AllocationBudget {
                    max_bytes: available,
                    ..budget
                },
                start,
                wall,
            )?;
            (partition, Some(evidence))
        } else {
            (None, None)
        };
    let partition = partition_request
        .map(|(owners, max_output)| {
            let available = budget
                .max_bytes
                .checked_sub(map.planned_bytes)
                .ok_or_else(|| {
                    error(
                        "MEMORY_LIMIT",
                        "mixed map exhausts shared return topology budget",
                    )
                })?;
            Partition::new(
                &mesh.mesh,
                &map.full,
                owners,
                frequencies_hz.len() * reference_ohm.len(),
                max_output,
                AllocationBudget {
                    max_bytes: available,
                    ..budget
                },
                start,
                wall,
            )
        })
        .transpose()?
        .or(automatic_partition);
    let partition_bytes = partition
        .as_ref()
        .map_or(0, |partition| partition.planned_bytes);
    budget.max_bytes = budget
        .max_bytes
        .checked_sub(partition_bytes)
        .ok_or_else(|| {
            error(
                "MEMORY_LIMIT",
                "shared return topology exceeds mixed budget",
            )
        })?;
    let n = map.full.potential_count();
    let names: Vec<_> = (0..n).map(|i| format!("native-potential-{i}")).collect();
    let mut electrodes = BTreeMap::new();
    for (electrode, &dof) in request.electrodes.iter().zip(&map.full_electrodes) {
        electrodes.insert(electrode.name.as_str(), dof);
    }
    let ports = request
        .ports
        .iter()
        .chain(packages.iter().map(|p| &p.electrodes))
        .map(|p| {
            let a = electrodes.get(p[0].as_str()).copied();
            let b = electrodes.get(p[1].as_str()).copied();
            match (a, b) {
                (Some(a), Some(b)) if a != b => Ok([a, b]),
                _ => Err(error(
                    "MIXED_FOIL_CONTACT",
                    "distinct native pad electrodes required per port",
                )),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let hybrid_requested=partition_policy==PartitionPolicy::AutomaticHybridV1;
    if hybrid_requested || std::env::var_os("P7_COMPACT_PLANAR").as_deref() == Some(std::ffi::OsStr::new("1")) {
        if partition.is_some() {
            return Err(error("HYBRID_UNSUPPORTED", "compact planar partition coupling is not implemented"));
        }
        let layered = request.stack.iter().filter(|layer| !layer.copper).any(|layer| layer.epsilon_r != Some(1.));
        let mut foil_sheet_x = 0f64;
        for foil in &map.foils {
            let span = foil.z_span_m;
            let sigma = mesh.conductivity_s_m[foil.tetrahedra[0]];
            let screen = super::hybrid_applicability::screen(effective_upper, span[1]-span[0], sigma, super::super::MU0).map_err(|reason| error("HYBRID_FREQUENCY", reason))?;
            if matches!(screen.action(), super::hybrid_applicability::Action::EnrichOrEscalate) && !copper_skin_profile::foil_sheet_scales_resistance() {
                return Err(error("HYBRID_FREQUENCY", "foil thickness requires frequency enrichment or local volume"));
            }
            // Passing above 0.2 is only possible under P7_FOIL_SHEET=1 (t/delta <= 1).
            if screen.thickness_over_skin_depth > 0.2 { foil_sheet_x = foil_sheet_x.max(screen.thickness_over_skin_depth); }
        }
        let foil_sheet = (foil_sheet_x > 0.).then(|| json!({"approximation": true, "maxThicknessOverSkin": foil_sheet_x,
            "estimatedResistanceError": copper_skin_profile::foil_sheet_error(foil_sheet_x), "bandTopHz": effective_upper,
            "criterion": if copper_skin_profile::foil_sheet_half_sheets() {"P7_FOIL_SHEET=4: two half-sheets (2 layers through thickness) per copper layer with thickness/skin depth > 1, one sheet otherwise; each half resistance scaled by Re(kt coth kt) at (t/2)/delta"} else if copper_skin_profile::foil_sheet_scales_resistance() {"P7_FOIL_SHEET=2: one foil sheet per copper layer at any thickness/skin depth, resistance scaled by Re(kt coth kt)"} else {"P7_FOIL_SHEET=1: one foil sheet per copper layer for thickness/skin depth <= 1"},
            "warning": "through-thickness current redistribution is not resolved; uniform-sheet resistance error estimated as Re(kt coth kt) - 1",
            "resistanceScaled": copper_skin_profile::foil_sheet_scales_resistance(),
            "multiplierOneSidedRatioX": copper_skin_profile::foil_resistance_ratio(foil_sheet_x),
            "multiplierTwoSidedRatioHalfX": copper_skin_profile::foil_resistance_ratio(foil_sheet_x / 2.),
            "twoSidedField": copper_skin_profile::foil_sheet_two_sided(),
            "resistanceMultiplierMax": map.foil_resistance_scale.iter().copied().fold(1., f64::max),
            "resistanceMultiplierMin": map.foil_resistance_scale.iter().copied().fold(f64::INFINITY, f64::min),
            "barrelResistanceMultiplierMax": map.barrel_resistance_scale.iter().copied().fold(1., f64::max),
            "halfSheets": copper_skin_profile::foil_sheet_half_sheets(),
            "status": "approximation",
            "statusNote": if copper_skin_profile::foil_sheet_half_sheets() {json!("R завышено до 16% на двух купонах при 40 МГц, эталон не проверен")} else {serde_json::Value::Null},
            "halfSheetLayers": if copper_skin_profile::foil_sheet_half_sheets() {json!(2)} else {serde_json::Value::Null},
            "multiplierTag": if copper_skin_profile::foil_sheet_half_sheets() {json!("approximation, two half-sheets per copper layer with t/delta > 1 (2 layers through thickness), each half resistance multiplied by Re(kt coth kt) at (t/2)/delta of the band top (one multiplier per band); layers with t/delta <= 1 stay one sheet, multiplier 1")} else if copper_skin_profile::foil_sheet_scales_resistance() {json!("approximation, one multiplier per band (Re(kt coth kt) at the band top applied at every frequency of the grid)")} else {serde_json::Value::Null},
            "fieldApproximation": if copper_skin_profile::foil_sheet_half_sheets() {json!("approximation, two half-sheets: the solver finds the current split between the halves; the multiplier is exact for equal currents and the single-sided limit is underestimated (about -8% at t/delta 3.35)")} else if copper_skin_profile::foil_sheet_scales_resistance() {json!(if copper_skin_profile::foil_sheet_two_sided() {"two-sided field: multiplier ratio(x/2); approximation, field assumed on both faces of the foil"} else {"one-sided field: the multiplier assumes the field acts on one face of the foil; for a plane between two signal layers this is an approximation"})} else {serde_json::Value::Null}}));
        // Thin-wall barrel applicability: one radial cell needs delta >= 3 t at the
        // band top. Above that the model is still admitted, with an explicit warning.
        let barrel_skin = if map.barrels.is_empty() {
            None
        } else {
            let sigma = request.barrel_conductivity_s_m.ok_or_else(|| error("BOARD_PADSTACK", "barrel conductivity is missing"))?;
            let wall_m = request.finished_hole_plating_m.ok_or_else(|| error("BOARD_PADSTACK", "finished-hole plating thickness is missing"))?;
            let delta = 1. / (std::f64::consts::PI * effective_upper * super::super::MU0 * sigma).sqrt();
            Some(json!({"approximation": delta < 3. * wall_m, "skinDepthM": delta, "wallThicknessM": wall_m, "bandTopHz": effective_upper,
                "criterion": "skin depth >= 3 * wall thickness for a single radial cell",
                "warning": if delta < 3. * wall_m {json!("barrel wall treated as one radial cell with uniform axial current above the skin criterion; skin and proximity redistribution inside the wall is not resolved")} else {serde_json::Value::Null}}))
        };
        let mixed_names: Vec<_> = (0..map.potential_count()).map(|i| format!("mixed-potential-{i}")).collect();
        let mixed_ports: Vec<_> = ports.iter().map(|p| [map.potential_map[p[0]], map.potential_map[p[1]]]).collect();
        let adaptive = hybrid_requested || std::env::var_os("P7_COMPACT_FULL_MIXED").as_deref() == Some(std::ffi::OsStr::new("1"));
        let charge_bytes = if layered && !adaptive { map.potential_count().checked_mul(map.potential_count()).and_then(|n|n.checked_mul(8)).ok_or_else(|| error("MEMORY_LIMIT", "layered compact trace size overflow"))? } else { 0 };
        let field_available = budget.max_bytes.checked_sub(map.retained_bytes()?).and_then(|n|n.checked_sub(charge_bytes)).ok_or_else(|| error("MEMORY_LIMIT", "layered field resident budget exhausted"))?;
        // One product-space prism contributes up to 36 local terms while the
        // source physical budget counts its three tetrahedra.
        let field_terms=budget.max_tetrahedra.saturating_mul(12);
        let mut field_refinement: Option<serde_json::Value> = None;
        let mut field_refinement_skipped: Option<serde_json::Value> = None;
        let prepared_field = if adaptive {
            let available=(budget.max_bytes/3).checked_sub(map.retained_bytes()?).ok_or_else(||error("MEMORY_LIMIT","adaptive field budget exhausted"))?;
            let field_budget=super::layered_capacitance::Budget {max_nodes:budget.max_nodes,max_local_terms:field_terms,max_bytes:available,max_wall:wall.saturating_sub(start.elapsed())};
            eprintln!("P7_COMPACT_STAGE stage=spatial-field-start elapsed={:.6}",start.elapsed().as_secs_f64());
            let (input,spatial,original)=super::planar_trace::input_from_board(mesh,request,field_budget)?;
            eprintln!("P7_COMPACT_STAGE stage=spatial-input-end elapsed={:.6}",start.elapsed().as_secs_f64());
            let (trace,certified)=super::planar_trace::certified_trace(&input,&original,&map,super::layered_capacitance::Budget{max_wall:wall.saturating_sub(start.elapsed()),..field_budget})?;
            eprintln!("P7_COMPACT_STAGE stage=spatial-certified-end elapsed={:.6}",start.elapsed().as_secs_f64());
            let field_budget_now=super::layered_capacitance::Budget{max_wall:wall.saturating_sub(start.elapsed()),..field_budget};
            // P7_FIELD_REFINE (default automatic since 2026-11-14, `0` = off): field-only XY refinement, potentials stay on the current-mesh nodes;
            // automatic mode skips it (coarse field mesh) when the refined field node count would exceed P7_FIELD_REFINE_MAX_NODES.
            let (refine_mode,auto_cap)=match super::field_xy_refine::mode(){super::field_xy_refine::Mode::Off=>(false,None),super::field_xy_refine::Mode::Forced=>(true,None),super::field_xy_refine::Mode::Auto(cap)=>(true,Some(cap))};
            let (outcome,trace,certified)=if refine_mode {
                match super::field_xy_refine::refine_capped(&input,trace,certified,&map,field_budget_now,auto_cap)? {
                    super::field_xy_refine::Outcome::Refined(r)=>(Some(r),None,None),
                    super::field_xy_refine::Outcome::Skipped{trace,cert,field_nodes,cap}=>{
                        field_refinement_skipped=Some(serde_json::json!({"fieldNodes":field_nodes,"cap":cap}));
                        (None,Some(trace),Some(cert))
                    }
                }
            } else {(None,Some(trace),Some(certified))};
            let field=if let Some(refined)=outcome {
                field_refinement=Some(refined.record.clone());
                let overlay=super::layered_capacitance::CertifiedOverlay{input:&refined.input,triangles:refined.triangles,copper:refined.copper,epsilon:refined.epsilon,node_owners:refined.node_owners,finite_contacts:refined.finite_contacts,rowless_modes:refined.rowless_modes,bytes:refined.bytes};
                super::layered_capacitance::prepare_certified_action(&refined.input,&refined.trace,&spatial,&overlay,super::layered_capacitance::Budget{max_wall:wall.saturating_sub(start.elapsed()),..field_budget})?
            } else {super::layered_capacitance::prepare_certified_action(&input,&trace.expect("coarse trace kept"),&spatial,&certified.expect("coarse certificate kept"),field_budget_now)?};
            eprintln!("P7_COMPACT_STAGE stage=spatial-field-end elapsed={:.6} plannedBytes={}",start.elapsed().as_secs_f64(),field.planned_bytes());
            Some(field)
        } else {None};
        let layered_c = if layered && !adaptive {
            let field_budget = super::layered_capacitance::Budget { max_nodes: budget.max_nodes, max_local_terms: field_terms, max_bytes: field_available, max_wall: wall.saturating_sub(start.elapsed()) };
            let (input, spatial, _original) = super::planar_trace::input_from_board(mesh, request, field_budget)?;
            let trace = super::planar_trace::trace(&input, &map, super::layered_capacitance::Budget { max_wall: wall.saturating_sub(start.elapsed()), ..field_budget })?;
            let field = super::layered_capacitance::extract_traced_spatial(&input, &trace, &spatial, super::layered_capacitance::Budget { max_wall: wall.saturating_sub(start.elapsed()), ..field_budget })?;
            Some(field.energy_capacitance_f()?)
        } else { None };
        let model_budget = AllocationBudget { max_bytes: budget.max_bytes.checked_sub(charge_bytes).ok_or_else(|| error("MEMORY_LIMIT", "layered trace matrix exceeds budget"))?, ..budget };
        let profiles = if adaptive && (hybrid_requested || std::env::var_os("P7_SECTION_HINTS").as_deref()==Some(std::ffi::OsStr::new("1"))) {
            let available = (model_budget.max_bytes / 3).saturating_sub(map.retained_bytes()?);
            // Reserve the conversion peak in addition to the producer's own
            // bounded working set: up to 32 full and compact vectors and one
            // compact denominator. Refuse the optional predictor if it does not fit.
            let conversion = map.full.current_count().checked_add(map.current_count())
                .and_then(|n|n.checked_mul(32)).and_then(|n|n.checked_add(map.current_count()))
                .and_then(|n|n.checked_mul(8)).and_then(|n|n.checked_add(32 * std::mem::size_of::<Vec<f64>>()));
            if let Some(producer_bytes) = conversion.and_then(|n|available.checked_sub(n)) {
                let producer_budget=AllocationBudget { max_bytes: producer_bytes, ..budget };
                let profiles=uniform_section_profiles(mesh, &map, &ports, frequencies_hz[0], producer_budget, start, wall)?;
                let profiles=if profiles.is_some(){profiles}else{planar_section_profiles(mesh,&map,&ports,frequencies_hz[0],producer_budget,start,wall)?};
                profiles
                    .map(|profiles| profiles.into_iter().map(|p| map.restrict_current_profile(&p)).collect::<Result<Vec<_>>>()).transpose()?
            } else { None }
        } else { None };
        super::board_foil::phase_trace("field-prepared", start, json!({"adaptive": adaptive, "tetrahedra": map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count(), "barrelPrisms": map.barrels.len(), "barrelModel": map.barrel_model.name()}));
        let (model, mut evidence, planned, adaptive_result) = if adaptive {
            let (mut result,model,planned)=foil_reduced::solve_compact_adaptive(&map,&mixed_names,&mixed_ports,packages,MatchedGrid{frequencies:frequencies_hz,references:reference_ohm,max_iterations},prepared_field.as_ref().ok_or_else(||error("HYBRID_INPUT","missing prepared field"))?,model_budget,start,wall,profiles.as_deref())?;
            let evidence=result["compactEvidence"].take();(model,evidence,planned,Some(result))
        } else {
            let (model,evidence,planned)=foil_reduced::compact_planar_model(&map,&mixed_names,&mixed_ports,super::super::EPS0,layered_c.as_deref(),model_budget,start,wall)?;
            (model,evidence,planned,None)
        };
        evidence["electricOperator"] = json!(if layered || adaptive {"layered-spatial-field-mesh"} else {"homogeneous-charge-gram"});
        evidence["version"] = json!("compact-planar-v1");
        evidence["foilSupports"] = json!(map.foils.len());
        evidence["localTetraSupports"] = json!(map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count());
        evidence["barrelModel"] = json!(map.barrel_model.name());
        evidence["barrelEvidence"] = json!({"model": map.barrel_model.name(), "barrels": map.barrel_count, "barrelPrisms": map.barrels.len(), "barrelFallbackColumns": map.barrel_fallback_columns, "barrelSliverColumns": map.barrel_sliver_columns, "foilAspectRelaxedColumns": map.foil_aspect_relaxed_columns, "sliverColumnsDropped": map.sliver_columns_dropped, "droppedVolumeM3": map.dropped_volume_m3, "droppedVolumeFraction": map.dropped_volume_fraction, "remainingTetrahedra": map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count()});
        if let Some(skin) = &barrel_skin {
            evidence["barrelSkinApproximation"] = skin.clone();
        }
        if let Some(record) = &field_refinement {
            evidence["fieldRefinement"] = record.clone();
        }
        if let Some(record) = &field_refinement_skipped {
            evidence["fieldRefinementSkipped"] = record.clone();
        }
        if let Some(sheet) = &foil_sheet {
            evidence["foilSheet"] = sheet.clone();
        }
        evidence["potentialCoordinates"] = json!(map.potential_count());
        evidence["currentCoordinates"] = json!(map.current_count());
        let mut result = if let Some(result)=adaptive_result {result} else {foil_reduced::compact_planar_response(&model, frequencies_hz, reference_ohm, packages, budget, start, wall)?};
        result["status"]=json!("complete"); result["coverage"]=json!("unverified"); result["representation"]=json!("S"); result["operator"]=json!("compact-planar-homogeneous-prototype"); result["frequenciesHz"]=json!(frequencies_hz); result["referenceOhm"]=json!(reference_ohm);
        if layered || adaptive { result["operator"] = json!("compact-planar-layered-prototype"); }
        result["ports"] = json!(request.ports);
        result["sourceMapping"] = json!(mesh.source_mapping);
        result["compactOperator"] = json!({"version":"compact-planar-v1", "ownership":"one physical owner per copper tetrahedron", "foilSupports":map.foils.len(), "localTetraSupports":map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count(), "potentialCoordinates":map.potential_count(), "currentCoordinates":map.current_count(), "dielectric":if layered || adaptive {"spatial-layered-field"} else {"homogeneous-air"}, "accuracy":"unverified", "limits":if adaptive {"sparse spatial-field and mixed basis limits; no proximity or physical approximation acceptance"} else {"bounded dense charge matrix; no proximity or physical approximation acceptance"}});
        result["provenance"] = json!({"method":"compact mixed foil and local volume", "modelKind":"mixed", "sourceMapping":mesh.source_mapping, "barrelIds":mesh.barrel_ids, "electrodeAreasM2":mesh.electrode_areas_m2, "prisms":map.foils.len(), "nativeCopperTetrahedra":map.owners.len(), "volumeTetrahedra":map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count(), "maxCopperVolumeChange":0., "fullPotentialCount":map.full.potential_count(), "fullCurrentCount":map.full.current_count(), "retainedPotentials":model.potential_nodes.len(), "currentModes":model.current_dofs.len(), "plannedBytes":if layered && !adaptive && budget.max_bytes <= 1usize << 62 {budget.max_bytes.saturating_add(reserve + provenance + partition_bytes)} else {planned + reserve + provenance + partition_bytes}, "maxSeconds":wall.as_secs_f64(), "maxMemoryMiB":(budget.max_bytes + reserve + provenance + partition_bytes) as f64 / 1048576., "spatialCoverage":"unverified", "bandCoverage":"unverified", "magneticQuadratureCoverage":"unverified", "driveConvention":"1 A RMS matched Norton basis columns", "compactOperator":evidence});
        if !packages.is_empty() { result["provenance"]["attachments"] = json!(packages.iter().map(|p| json!({"id":p.id,"electrodes":p.electrodes})).collect::<Vec<_>>()); }
        if hybrid_requested {
            result["provenance"]["partitionPolicy"]=json!("automatic-hybrid-v1");
            result["provenance"]["hybridAssembly"]=json!({"policy":"automatic-hybrid-v1","method":"global-compact-mixed","potentialCoordinates":map.potential_count(),"currentCoordinates":map.current_count(),"physicalAccuracy":"unverified"});
            result["provenance"]["barrelModel"]=json!(map.barrel_model.name());
            result["provenance"]["barrelPrisms"]=json!(map.barrels.len());
        }
        if skin.is_some() {
            result["provenance"]["operatorPolicy"]=json!(operator.policy_name());
            result["provenance"]["copperSkinMesh"]=skin_profile_evidence(mesh,requested_upper,effective_upper);
        }
        {
            let f=map.dropped_volume_fraction;
            let tier=if f<=1e-6 {"noise"} else {"approximation"};
            result["quality"]["sliverPolicy"]=json!({"tier":tier,"droppedVolumeRelative":f,"columns":map.sliver_columns_dropped,"noiseLimit":1e-6,"approximationLimit":1e-3,
                "warning":if tier=="approximation" {json!("Sliver copper columns carry no current mode; their volume exceeds the 1e-6 noise level. Unverified approximation.")} else {serde_json::Value::Null}});
        }
        result["quality"]["scope"] = json!("compact-discrete");
        result["quality"]["physicalError"] = json!("unverified");
        if super::board_foil::reduced_model_export() {
            // P8: the final reduced coupled-RLC model the S samples were evaluated from (open ports, no terminations).
            // R and L carry the skin profile frozen at `profileUpperHz`; the model was trained on `trainedAtHz`.
            result["reducedModel"] = json!({"model":model,"portCount":request.ports.len(),"trainedAtHz":frequencies_hz,"profileUpperHz":effective_upper,"scope":"compact-discrete","physicalAccuracy":"unverified"});
        }
        return Ok(result);
    }
    let field_budget = AllocationBudget {
        max_bytes: budget
            .max_bytes
            .checked_sub(map.planned_bytes)
            .ok_or_else(|| error("MEMORY_LIMIT", "mixed mapping exhausts field budget"))?,
        ..budget
    };
    // A single original surface field serves reduced congruences and full
    // residuals, including charge on every exposed barrel and foil surface.
    let field = Field::new_conforming_trace(&mesh.mesh, n, map.trace(None)?, field_budget)?;
    let profiles=if std::env::var_os("P7_SECTION_HINTS").as_deref()==Some(std::ffi::OsStr::new("1")) {
        if let Some(max_bytes)=budget.max_bytes.checked_sub(map.retained_bytes()?).and_then(|n|n.checked_sub(field.planned_bytes())) {uniform_section_profiles(mesh,&map,&ports,frequencies_hz[0],AllocationBudget{max_bytes,..budget},start,wall)?} else {None}
    } else {None};
    let section_profile_count=profiles.as_ref().map_or(0,Vec::len);
    let (mut checked, planned) = foil_reduced::solve_mixed_grid(
        &map,
        &mesh.mesh,
        &names,
        &ports,
        packages,
        &field,
        partition.as_ref(),
        budget,
        start,
        wall,
        MatchedGrid {
            frequencies: frequencies_hz,
            references: reference_ohm,
            max_iterations,
        },
        profiles.as_deref(),
    )?;
    check_time(start, wall)?;
    checked["sectionProfileCount"]=serde_json::json!(section_profile_count);
    checked["status"] = json!("complete");
    checked["representation"] = json!("S");
    checked["coverage"] = json!("unverified");
    checked["frequenciesHz"] = json!(frequencies_hz);
    checked["referenceOhm"] = json!(reference_ohm);
    checked["ports"] = json!(request.ports);
    let final_step = checked["enrichment"]
        .as_array()
        .and_then(|h| h.last())
        .ok_or_else(|| error("MIXED_FOIL_GRID", "checked rank evidence missing"))?;
    let retained_potentials = final_step["potentialModes"].clone();
    let current_modes = final_step["currentModes"].clone();
    let method = if map.foils.is_empty() {
        "original volume, checked matched exact grid"
    } else {
        "mixed physical foil and volume, checked matched exact grid"
    };
    let skin_evidence =
        skin.map(|profile| skin_profile_evidence(&mesh, requested_upper, profile.upper_hz));
    checked["provenance"] = json!({
        "method":method, "modelKind":"mixed",
        "sourceMapping":mesh.source_mapping, "barrelIds":mesh.barrel_ids,
        "electrodeAreasM2":mesh.electrode_areas_m2,
        "prisms":map.foils.len(), "nativeCopperTetrahedra":map.owners.len(),
        "maxCopperVolumeChange":0.,
        "volumeTetrahedra":map.owners.iter().filter(|&&o| o == Owner::Volume).count(),
        "fullPotentialCount":n, "fullCurrentCount":map.full.current_count(),
        "retainedPotentials":retained_potentials, "currentModes":current_modes,
        "plannedBytes":planned + reserve + provenance + partition_bytes,
        "spatialCoverage":"unverified", "bandCoverage":"unverified",
        "magneticQuadratureCoverage":"unverified",
        "driveConvention":"1 A RMS matched Norton basis columns, checked only at listed frequencies",
    });
    checked["provenance"]["magneticOperator"] = checked["magneticOperator"].take();
    checked.as_object_mut().unwrap().remove("magneticOperator");
    if let Some(partition) = partition {
        if !partition.compact {
            checked["spatialPartition"] = partition.topology();
        }
    }
    if let Some(evidence) = automatic_evidence {
        checked["provenance"]["partitionPolicy"] = json!("automatic-conductive-schur-v1");
        checked["provenance"]["automaticPartition"] = evidence;
    }
    if let Some(skin) = skin_evidence {
        checked["provenance"]["operatorPolicy"] = json!(operator.policy_name());
        checked["provenance"]["copperSkinMesh"] = skin;
    }
    if !packages.is_empty() {
        checked["provenance"]["attachments"] = json!(packages
            .iter()
            .map(|p| { json!({"id":p.id,"electrodes":p.electrodes}) })
            .collect::<Vec<_>>());
    }
    Ok(checked)
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn profiled_checked_grid_builds_one_shared_mesh_and_preserves_finite_pad_ownership() {
        let board = crate::import_json(include_str!(
            "../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json"
        )).board.unwrap();
        let mut geometry: board_mesh::Request = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json"
        )).unwrap();
        geometry.cut_planes.clear();
        geometry.allow_insulated_copper_cuts = false;
        geometry.domain_polygon_mm = None;
        geometry.max_vertical_step_m = 0.00025;
        geometry.stack[0].thickness_m = 0.0001;
        geometry.electrodes = ["s0", "s1", "r0", "r1"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| Electrode {
                name: name.into(),
                source_path: Some(format!("pads[{i}]")),
                source_paths: None,
                layer: if i == 0 { "TOP" } else { "BOTTOM" }.into(),
                upper: i != 0,
            })
            .collect();
        geometry.ports = vec![["s0".into(), "r0".into()], ["s1".into(), "r1".into()]];
        let before = serde_json::to_value(&geometry).unwrap();
        let frequency = 100_000.;
        let grid = CheckedGrid {
            frequencies_hz: &[frequency],
            reference_ohm: &[50., 50.],
            max_iterations: 1,
        };
        assert!(checked_reserve(&geometry, grid, None).is_err());
        let operator = CheckedOperator::SkinVolumeV3;
        assert!(!operator.admits(frequency + 1.));
        assert!(operator
            .skin_profile(
                frequency,
                Some(frequency - 1.),
                geometry.max_vertical_step_m
            )
            .is_err());
        assert!(CheckedOperator::Legacy
            .skin_profile(1000., Some(frequency), geometry.max_vertical_step_m)
            .is_err());
        let profile = operator
            .skin_profile(frequency, None, geometry.max_vertical_step_m)
            .unwrap();
        assert!(checked_reserve(&geometry, grid, profile).is_ok());
        let supplemental = operator
            .skin_profile(20_000., Some(frequency), geometry.max_vertical_step_m)
            .unwrap();
        assert_eq!(supplemental.unwrap().upper_hz, profile.unwrap().upper_hz);
        let budget = AllocationBudget {
            max_bytes: 512 * 1024 * 1024,
            max_nodes: 100_000,
            max_tetrahedra: 200_000,
        };
        let mesh = board_mesh::build_profiled(
            &board,
            &geometry,
            budget,
            Duration::from_secs(20),
            None,
            &[],
            profile,
        )
        .unwrap();
        let evidence = skin_profile_evidence(&mesh, 20_000., frequency);
        assert_eq!(evidence["requestedUpperHz"], json!(20_000.));
        assert_eq!(evidence["trainingUpperBoundHz"], json!(20_000.));
        assert_eq!(evidence["effectiveUpperHz"], json!(frequency));
        assert_eq!(evidence["currentAndElectricShareMesh"], true);
        let top = evidence["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|layer| layer["layer"] == "TOP")
            .unwrap();
        assert_eq!(top["slabs"], 2);
        assert!(top["maximumStepM"].as_f64().unwrap() <= top["skinDepthM"].as_f64().unwrap() / 3.);
        assert!(geometry
            .electrodes
            .iter()
            .all(|electrode| !mesh.electrode_nodes(electrode).unwrap().is_empty()));
        assert_eq!(mesh.barrel_ids.len(), 1);
        let map = prepare(&mesh, &geometry.electrodes, frequency, budget, Duration::from_secs(20)).unwrap();
        let mut owned = vec![0usize; mesh.mesh.copper_tetrahedra.len()];
        for (index, foil) in map.foils.iter().enumerate() {
            let mut volume = 0.;
            for &cell in &foil.tetrahedra {
                assert!(matches!(map.owners[cell], Owner::Foil(i) if i == index));
                owned[cell] += 1;
                let tet = &mesh.mesh.copper_tetrahedra[cell];
                let z = tet.nodes.map(|n| mesh.mesh.nodes_m[n][2]);
                assert!(z.iter().all(|&v| v >= foil.z_span_m[0] && v <= foil.z_span_m[1]));
                volume += tet.volume_m3;
            }
            assert!((volume-foil.volume_m3).abs() <= 1e-8*foil.volume_m3);
        }
        for (i, owner) in map.owners.iter().enumerate() {
            assert_eq!(owned[i], usize::from(matches!(owner, Owner::Foil(_))));
        }
        let x: Vec<_> = (0..map.current_count()).map(|i| ((i%11) as f64-5.)/7.).collect();
        let y: Vec<_> = (0..map.full.current_count()).map(|i| ((i%13) as f64-6.)/9.).collect();
        let lifted = map.lift_currents(&x).unwrap();
        let restricted = map.restrict_current_profile(&lifted).unwrap();
        for (actual, expected) in restricted.iter().zip(&x) {
            assert!((actual-expected).abs() <= 1e-10 * expected.abs().max(1.));
        }
        let projected = map.project_current_action(&y).unwrap();
        let left: f64 = lifted.iter().zip(&y).map(|(a,b)| a*b).sum();
        let right: f64 = x.iter().zip(&projected).map(|(a,b)| a*b).sum();
        assert!((left-right).abs() <= 1e-10*left.abs().max(right.abs()).max(1.));
        assert_eq!(map.electrodes.len(), geometry.electrodes.len());
        assert_eq!(serde_json::to_value(&geometry).unwrap(), before);
    }

    #[test]
    fn wide_isolated_foil_retains_stratified_ownership_and_projection() {
        let mut value: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
        value["outline"] = json!([{"x":-4.,"y":-3.},{"x":4.,"y":-3.},{"x":4.,"y":3.},{"x":-4.,"y":3.}]);
        value["copper"]["fixed"]["tracks"] = json!([]);
        value["copper"]["fixed"]["zones"] = json!([]);
        value["pads"].as_array_mut().unwrap().truncate(2);
        for (i, pad) in value["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
            pad["at"] = json!({"x": if i == 0 {-1.5} else {1.5}, "y":0.});
            pad["shape"] = json!({"kind":"rect","widthMm":2.,"heightMm":2.});
        }
        let board = crate::import_json(&value.to_string()).board.unwrap();
        let geometry: board_mesh::Request = serde_json::from_value(json!({
            "roiMm":[[-4.,-3.],[4.,3.]],
            "stack":[{"name":"TOP","thicknessM":0.0001,"copper":true,"epsilonR":null,"conductivitySM":58000000.},
                     {"name":"dielectric","thicknessM":0.0001,"copper":false,"epsilonR":4.,"conductivitySM":null},
                     {"name":"BOTTOM","thicknessM":0.000005,"copper":true,"epsilonR":null,"conductivitySM":58000000.}],
            "airMarginM":0.0002,"maxTriangleAreaM2":0.000001,"maxVerticalStepM":0.0001,
            "airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,
            "acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,
            "electrodes":[{"name":"a","sourcePath":"pads[0]","layer":"TOP","upper":true},
                          {"name":"b","sourcePath":"pads[1]","layer":"TOP","upper":true}],
            "ports":[["a","b"]]
        })).unwrap();
        let budget = AllocationBudget { max_bytes:512*1024*1024, max_nodes:100_000, max_tetrahedra:200_000 };
        let wall=Duration::from_secs(20);
        let profile=CheckedOperator::SkinVolumeV3.skin_profile(100_000.,None,geometry.max_vertical_step_m).unwrap().map(|profile|profile.with_thickness_screen([58e6]).unwrap());
        let mesh=board_mesh::build_profiled(&board,&geometry,budget,wall,None,&[],profile).unwrap();
        let map=prepare(&mesh,&geometry.electrodes,100_000.,budget,wall).unwrap();
        assert!(map.foils.iter().any(|foil| foil.z_span_m != mesh.layer_spans[&foil.layer]), "expected a stratified foil away from the volume halo");
        let mut counts=vec![0usize;mesh.mesh.copper_tetrahedra.len()];
        for (i,foil) in map.foils.iter().enumerate() {
            assert!(super::super::hybrid_applicability::screen(100_000.,foil.z_span_m[1]-foil.z_span_m[0],58e6,super::super::super::MU0).unwrap().thickness_passes);
            let mut volume=0.;
            for &cell in &foil.tetrahedra {
                assert!(matches!(map.owners[cell],Owner::Foil(owner) if owner==i)); counts[cell]+=1;
                let tet=&mesh.mesh.copper_tetrahedra[cell]; volume+=tet.volume_m3;
                assert!(tet.nodes.iter().all(|&n|mesh.mesh.nodes_m[n][2]>=foil.z_span_m[0] && mesh.mesh.nodes_m[n][2]<=foil.z_span_m[1]));
            }
            assert!((volume-foil.volume_m3).abs()<=1e-8*foil.volume_m3);
        }
        for (i,owner) in map.owners.iter().enumerate() {assert_eq!(counts[i],usize::from(matches!(owner,Owner::Foil(_))));}
        let x:Vec<_>=(0..map.current_count()).map(|i|(i%11) as f64-5.).collect();
        let y:Vec<_>=(0..map.full.current_count()).map(|i|(i%13) as f64-6.).collect();
        let a:f64=map.lift_currents(&x).unwrap().iter().zip(&y).map(|(a,b)|a*b).sum();
        let b:f64=x.iter().zip(map.project_current_action(&y).unwrap()).map(|(a,b)|a*b).sum();
        assert!((a-b).abs()<=1e-10*a.abs().max(b.abs()).max(1.));
        assert_eq!(map.electrodes.len(),2);
        let mut seed=vec![0.;map.current_count()];
        for foil in &map.foils { seed[foil.first_current]=foil.z_span_m[1]-foil.z_span_m[0]; }
        assert!(map.skin_hint_group_count(100_000.,budget,Instant::now(),wall).unwrap()>0);
        let hints=map.skin_current_hints(&[seed],100_000.,budget,Instant::now(),wall).unwrap();
        assert!(!hints.is_empty());
        for hint in &hints {
            assert!(hint.iter().all(|v|v.is_finite()));
            let mut transport=BTreeMap::<(&str,usize),f64>::new();
            for foil in &map.foils {
                let cell=foil.tetrahedra[0];
                let density=map.full.cells[cell].density_per_amp_m2 * map.current_map[3*cell].unwrap().weight;
                *transport.entry((&foil.layer,foil.planar_triangle)).or_default() += hint[foil.first_current]*density*foil.volume_m3;
            }
            assert!(transport.values().all(|v|v.abs()<1e-10));
        }
        let tiny=AllocationBudget { max_bytes:1, ..budget };
        assert!(map.skin_current_hints(&[vec![1.;map.current_count()]],100_000.,tiny,Instant::now(),wall).is_err());
    }
    #[test]
    fn connected_profiled_foil_runs_coupled_skin_hints() {
        let frequency=1_000_000.;
        let mut value:serde_json::Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
        value["outline"]=json!([{"x":-1.5,"y":-1.},{"x":1.5,"y":-1.},{"x":1.5,"y":1.},{"x":-1.5,"y":1.}]);
        value["pads"].as_array_mut().unwrap().truncate(2);
        for (i,pad) in value["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
            pad["at"]=json!({"x":if i==0 {-0.6} else {0.6},"y":0.});
            pad["shape"]=json!({"kind":"rect","widthMm":0.8,"heightMm":0.8});
        }
        value["copper"]["fixed"]["tracks"]=json!([{"id":"wide","net":"SIGNAL","layer":"TOP","widthMm":0.8,"points":[{"x":-0.6,"y":0.},{"x":0.6,"y":0.}]}]);
        value["copper"]["fixed"]["zones"]=json!([]);
        value["copper"]["fixed"]["vias"]=json!([]);
        let board=crate::import_json(&value.to_string()).board.unwrap();
        let request:board_mesh::Request=serde_json::from_value(json!({
            "roiMm":[[-1.5,-1.],[1.5,1.]],
            "stack":[{"name":"TOP","thicknessM":0.000035,"copper":true,"epsilonR":null,"conductivitySM":58000000.},
                     {"name":"dielectric","thicknessM":0.0001,"copper":false,"epsilonR":1.,"conductivitySM":null},
                     {"name":"BOTTOM","thicknessM":0.000005,"copper":true,"epsilonR":null,"conductivitySM":58000000.}],
            "airMarginM":0.0002,"maxTriangleAreaM2":0.000001,"maxVerticalStepM":0.0001,
            "airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,
            "acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,
            "electrodes":[{"name":"a","sourcePath":"pads[0]","layer":"TOP","upper":true},{"name":"b","sourcePath":"pads[1]","layer":"TOP","upper":true}],
            "ports":[["a","b"]]
        })).unwrap();
        let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:10_000,max_tetrahedra:100_000};
        let wall=Duration::from_secs(25); let start=Instant::now();
        let profile=CheckedOperator::HybridPlanarV3.skin_profile(frequency,None,request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        let mesh=board_mesh::build_profiled(&board,&request,budget,wall,None,&[],Some(profile)).unwrap();
        let result=solve_checked_mesh_impl_policy(&mesh,&request,CheckedGrid{frequencies_hz:&[frequency],reference_ohm:&[50.],max_iterations:16},Some(profile),&[],None,PartitionPolicy::AutomaticHybridV1,CheckedOperator::HybridPlanarV3,budget,start,wall).unwrap();
        assert_eq!(result["status"],"complete");
        assert_eq!(result["coverage"],"unverified");
        assert_eq!(result["provenance"]["operatorPolicy"],"hybrid-planar-v3");
        assert!(result["provenance"]["compactOperator"]["skinHints"]["added"].as_u64().unwrap()>0);
        assert!(result["quality"]["maxKclRelative"].as_f64().unwrap()<1e-8);
        assert!(result["quality"]["maxKvlRelative"].as_f64().unwrap()<1e-8);
        assert!(result["quality"]["maxColumnPowerBalanceErrorW"].as_f64().unwrap()<=1e-12);
    }

}


#[cfg(test)]
mod uniform_section_seed_tests {
    use super::*;
    #[test]
    fn xy_section_predictor_uses_physical_foil_supports() {
        use serde_json::{json, Value};
        let mut value: Value = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/analysis-segmented-coupon.json"
        ))
        .unwrap();
        value["outline"] = json!([{"x":-1.,"y":-1.},{"x":1.,"y":-1.},{"x":1.,"y":1.},{"x":-1.,"y":1.}]);
        value["copper"]["fixed"]["tracks"] = json!([{"id":"signal-run","net":"SIGNAL","layer":"TOP","widthMm":0.2,"points":[{"x":-0.6,"y":-0.4},{"x":0.6,"y":-0.4}]},{"id":"return-run","net":"RETURN","layer":"TOP","widthMm":0.2,"points":[{"x":-0.6,"y":0.4},{"x":0.6,"y":0.4}]}]);
        value["copper"]["fixed"]["zones"] = json!([]);
        value["pads"].as_array_mut().unwrap().truncate(2);
        value["pads"][1]["net"] = json!("RETURN");
        for (i, p) in value["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
            p["at"] = json!({"x":-0.6,"y":if i==0 {-0.4}else{0.4}});
            p["shape"] = json!({"kind":"rect","widthMm":0.2,"heightMm":0.2});
        }
        let board = crate::import_json(&value.to_string()).board.unwrap();
        let request:board_mesh::Request=serde_json::from_value(json!({"roiMm":[[-1.,-1.],[1.,1.]],"stack":[{"name":"TOP","thicknessM":0.000005,"copper":true,"epsilonR":null,"conductivitySM":58000000.},{"name":"dielectric","thicknessM":0.0001,"copper":false,"epsilonR":4.,"conductivitySM":null},{"name":"BOTTOM","thicknessM":0.000005,"copper":true,"epsilonR":null,"conductivitySM":58000000.}],"airMarginM":0.0002,"maxTriangleAreaM2":0.000002,"maxVerticalStepM":0.0001,"airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"electrodes":[{"name":"a","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"b","sourcePath":"pads[1]","layer":"TOP","upper":false}],"ports":[["a","b"]]})).unwrap();
        let budget = AllocationBudget {
            max_bytes: 128 * 1024 * 1024,
            max_nodes: 10000,
            max_tetrahedra: 10000,
        };
        let wall = Duration::from_secs(30);
        let mesh = board_mesh::build(&board, &request, budget, wall).unwrap();
        let map = prepare(&mesh, &request.electrodes, 1000000., budget, wall).unwrap();
        let ports = [[map.full_electrodes[0], map.full_electrodes[1]]];
        let profiles =
            planar_section_profiles(&mesh, &map, &ports, 1000000., budget, Instant::now(), wall)
                .unwrap()
                .expect("XY section predictor");
        let compact: Vec<_> = profiles
            .iter()
            .map(|p| map.restrict_current_profile(p).unwrap())
            .collect();
        assert!(compact.iter().flatten().any(|v| *v != 0.));
        assert!(planar_section_profiles(
            &mesh,
            &map,
            &ports,
            1000000.,
            AllocationBudget {
                max_bytes: 1,
                ..budget
            },
            Instant::now(),
            wall
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn board_mesh_two_axial_pads_produces_real_section_profiles() {
        use serde_json::{json,Value};
        let mut value:Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
        value["outline"]=json!([{"x":-1.,"y":-1.},{"x":1.,"y":-1.},{"x":1.,"y":1.},{"x":-1.,"y":1.}]);
        value["copper"]["fixed"]["tracks"]=json!([]);value["copper"]["fixed"]["zones"]=json!([]);
        value["pads"].as_array_mut().unwrap().truncate(2);
        for (i,p) in value["pads"].as_array_mut().unwrap().iter_mut().enumerate(){p["at"]=json!({"x":if i==0 {-0.4}else{0.4},"y":0.});p["shape"]=json!({"kind":"rect","widthMm":0.25,"heightMm":0.25});}
        let board=crate::import_json(&value.to_string()).board.unwrap();
        let request:board_mesh::Request=serde_json::from_value(json!({"roiMm":[[-1.,-1.],[1.,1.]],"stack":[{"name":"TOP","thicknessM":0.0001,"copper":true,"epsilonR":null,"conductivitySM":58000000.},{"name":"dielectric","thicknessM":0.0001,"copper":false,"epsilonR":4.,"conductivitySM":null},{"name":"BOTTOM","thicknessM":0.000005,"copper":true,"epsilonR":null,"conductivitySM":58000000.}],"airMarginM":0.0002,"maxTriangleAreaM2":0.000002,"maxVerticalStepM":0.0001,"airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"electrodes":[{"name":"a0","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"b0","sourcePath":"pads[1]","layer":"TOP","upper":false},{"name":"a1","sourcePath":"pads[0]","layer":"TOP","upper":true},{"name":"b1","sourcePath":"pads[1]","layer":"TOP","upper":true}],"ports":[["a0","b0"],["a1","b1"]]})).unwrap();
        let budget=AllocationBudget{max_bytes:128*1024*1024,max_nodes:10000,max_tetrahedra:10000};let start=Instant::now();let wall=Duration::from_secs(30);
        let mesh=board_mesh::build(&board,&request,budget,wall).unwrap();let map=prepare(&mesh,&request.electrodes,1000.,budget,wall).unwrap();
        let ports=[[map.full_electrodes[0],map.full_electrodes[1]],[map.full_electrodes[2],map.full_electrodes[3]]];
        let profiles=uniform_section_profiles(&mesh,&map,&ports,1000.,budget,start,wall).unwrap();assert!(profiles.as_ref().is_some_and(|p|!p.is_empty()),"valid extrusion was refused");
        let mut nonuniform=board_mesh::build(&board,&request,budget,wall).unwrap();nonuniform.conductivity_s_m[0]*=0.5;
        let nonmap=prepare(&nonuniform,&request.electrodes,1000.,budget,wall).unwrap();let nonports=[[nonmap.full_electrodes[0],nonmap.full_electrodes[1]],[nonmap.full_electrodes[2],nonmap.full_electrodes[3]]];
        assert!(uniform_section_profiles(&nonuniform,&nonmap,&nonports,1000.,budget,Instant::now(),wall).unwrap().is_none());
        let make_grid=||MatchedGrid{frequencies:&[1000.],references:&[50.,50.],max_iterations:4};
        let names:Vec<_>=(0..map.full.potential_count()).map(|i|format!("p{i}")).collect();
        let field=Field::new_conforming_trace(&mesh.mesh,map.full.potential_count(),map.trace(None).unwrap(),budget).unwrap();
        let run=|hints:Option<&[Vec<f64>]>|foil_reduced::solve_mixed_grid(&map,&mesh.mesh,&names,&ports,&[],&field,None,budget,Instant::now(),Duration::from_secs(30),make_grid(),hints);
        let t=Instant::now();let (seeded,_)=run(profiles.as_deref()).unwrap();let seeded_wall=t.elapsed();let t=Instant::now();let (plain,_)=run(None).unwrap();let plain_wall=t.elapsed();eprintln!("section timings seeded={seeded_wall:?} plain={plain_wall:?}");
        for row in 0..2 {for col in 0..2 {for part in ["real","imaginary"] {let a=seeded["parameters"][0][row][col][part].as_f64().unwrap();let b=plain["parameters"][0][row][col][part].as_f64().unwrap();assert!((a-b).abs()<1e-8,"seeded={a:e} plain={b:e}");}}}
        for result in [&seeded,&plain] {assert!(result["quality"]["maxKclRelative"].as_f64().unwrap()<1e-8);assert!(result["quality"]["maxKvlRelative"].as_f64().unwrap()<1e-8);}
        let bad_ref=[[ports[0][0],ports[0][1]],[ports[1][1],ports[1][0]]];assert!(uniform_section_profiles(&mesh,&map,&bad_ref,1000.,budget,start,wall).unwrap().is_none());
        let side=[[ports[0][0],ports[1][1]],[ports[1][0],ports[0][1]]];assert!(uniform_section_profiles(&mesh,&map,&side,1000.,budget,start,wall).unwrap().is_none());
        assert!(seeded["sectionHints"]["submitted"].as_u64().unwrap()>0);assert!(seeded["sectionHints"]["addedCurrentModes"].as_u64().unwrap()>0);
        if std::env::var_os("P7_SECTION_HINTS").as_deref()==Some(std::ffi::OsStr::new("1")) {
            let result=solve_checked_mesh_impl(&mesh,&request,CheckedGrid{frequencies_hz:&[1000.],reference_ohm:&[50.,50.],max_iterations:4},None,&[],None,PartitionPolicy::Legacy,budget,Instant::now(),Duration::from_secs(30)).unwrap();
            assert!(result["sectionProfileCount"].as_u64().unwrap()>0);assert!(result["sectionHints"]["addedCurrentModes"].as_u64().unwrap()>0);
            for row in 0..2 {for col in 0..2 {for part in ["real","imaginary"] {assert!((result["parameters"][0][row][col][part].as_f64().unwrap()-plain["parameters"][0][row][col][part].as_f64().unwrap()).abs()<1e-8);}}}
        }
        eprintln!("seeded hints={} quality={} plain quality={}",seeded["sectionHints"],seeded["quality"],plain["quality"]);
    }
}

pub(super) struct LocalSupport { pub tetra: [[f64; 3]; 4], pub volume: f64, pub currents: Vec<[f64;3]>, pub charge_index: usize, pub charge_density: f64 }

impl MixedMap<'_> {
    /// Build disjoint, constant-volume charge tests and current densities from
    /// the physical foil ownership. Local volume needs a separate cross kernel.
    pub(super) fn planar_supports(&self, modes: &[Vec<f64>]) -> Result<(Vec<super::planar_operator::Support>, Vec<LocalSupport>, Vec<Vec<f64>>)> {
        self.reject_barrel_prisms("homogeneous planar charge model")?;
        if self.foils.is_empty() || modes.iter().any(|m| m.len() != self.current_count() || m.iter().any(|x| !x.is_finite())) {
            return Err(error("HYBRID_INPUT", "invalid compact current basis"));
        }
        let local_count = self.owners.iter().filter(|o| matches!(o, Owner::Volume)).count();
        let nq = self.foils.len() + local_count;
        let mut supports = Vec::with_capacity(self.foils.len());
        let mut dual = Vec::with_capacity(nq);
        for (q, foil) in self.foils.iter().enumerate() {
            let span = foil.z_span_m;
            let indices = self.mesh.planar_triangles[foil.planar_triangle];
            let mut xy = indices.map(|i| { let p = self.mesh.mesh.nodes_m[i]; [p[0], p[1]] });
            let cross = (xy[1][0]-xy[0][0])*(xy[2][1]-xy[0][1])-(xy[1][1]-xy[0][1])*(xy[2][0]-xy[0][0]);
            if cross < 0. { xy.swap(1, 2); }
            let prism = super::prism::Prism::new(format!("foil-{q}"), xy, span)?;
            if (prism.volume_m3 - foil.volume_m3).abs() > 1e-8 * foil.volume_m3 { return Err(error("HYBRID_INPUT", "foil support volume mismatch")); }
            let mut charge_density = vec![0.; nq];
            charge_density[q] = 1. / prism.volume_m3;
            let mut row = vec![0.; self.potential_count()];
            for &t in &foil.tetrahedra {
                let tet = &self.mesh.mesh.copper_tetrahedra[t];
                let cell = &self.full.cells[t];
                for &p in &cell.potentials { row[self.potential_map[p]] += tet.volume_m3 / (4. * prism.volume_m3); }
            }
            let total: f64 = row.iter().sum();
            if !total.is_finite() || (total-1.).abs() > 1e-10 { return Err(error("HYBRID_CHARGE", "charge dual does not preserve constants")); }
            let density = self.full.cells[foil.tetrahedra[0]].density_per_amp_m2 * self.current_map[3*foil.tetrahedra[0]].ok_or_else(|| error("HYBRID_INPUT", "foil current mapping missing"))?.weight;
            let current_density = modes.iter().map(|mode| [density * mode[foil.first_current], density * mode[foil.first_current+1], 0.]).collect();
            supports.push(super::planar_operator::Support { prism, charge_density, current_density });
            dual.push(row);
        }
        let mut locals = Vec::with_capacity(local_count);
        for (t, owner) in self.owners.iter().enumerate() {
            if !matches!(owner, Owner::Volume) { continue; }
            let tet = &self.mesh.mesh.copper_tetrahedra[t];
            let cell = &self.full.cells[t];
            let mut row = vec![0.; self.potential_count()];
            for &p in &cell.potentials { row[self.potential_map[p]] += 0.25; }
            let mut currents = vec![[0.;3]; modes.len()];
            for axis in 0..3 {
                let entry = self.current_map[3*t+axis].ok_or_else(|| error("HYBRID_INPUT", "local current mapping missing"))?;
                for (i, mode) in modes.iter().enumerate() { currents[i][axis] = cell.density_per_amp_m2 * entry.weight * mode[entry.mode]; }
            }
            locals.push(LocalSupport { tetra: tet.nodes.map(|i| self.mesh.mesh.nodes_m[i]), volume: tet.volume_m3, currents, charge_index: self.foils.len()+locals.len(), charge_density: 1./tet.volume_m3 });
            dual.push(row);
        }
        Ok((supports, locals, dual))
    }
}
impl MixedMap<'_> { pub(super) fn mesh(&self) -> &BoardMesh { self.mesh } }

#[cfg(test)]
mod compact_board_dispatch_test {
    use super::*;
    use serde_json::{json, Value};
    #[test]
    fn imported_air_board_two_drives_and_missing_contact() {
        let mut value: Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
        value["outline"] = json!([{"x":-1.,"y":-1.},{"x":1.,"y":-1.},{"x":1.,"y":1.},{"x":-1.,"y":1.}]);
        value["copper"]["fixed"]["tracks"] = json!([]); value["copper"]["fixed"]["zones"] = json!([]);
        value["pads"].as_array_mut().unwrap().truncate(2);
        for (i,p) in value["pads"].as_array_mut().unwrap().iter_mut().enumerate() { p["at"]=json!({"x":if i==0 {-0.4}else{0.4},"y":0.}); p["shape"]=json!({"kind":"rect","widthMm":0.25,"heightMm":0.25}); }
        let board=crate::import_json(&value.to_string()).board.unwrap();
        let request: board_mesh::Request=serde_json::from_value(json!({"roiMm":[[-1.,-1.],[1.,1.]],"stack":[{"name":"TOP","thicknessM":0.00001,"copper":true,"epsilonR":null,"conductivitySM":58000000.},{"name":"air","thicknessM":0.0001,"copper":false,"epsilonR":1.,"conductivitySM":null},{"name":"BOTTOM","thicknessM":0.00001,"copper":true,"epsilonR":null,"conductivitySM":58000000.}],"airMarginM":0.0002,"maxTriangleAreaM2":0.000002,"maxVerticalStepM":0.0001,"airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"electrodes":[{"name":"a","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"b","sourcePath":"pads[1]","layer":"TOP","upper":false}],"ports":[["a","b"],["b","a"]]})).unwrap();
        let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let wall=Duration::from_secs(25); let mesh=board_mesh::build(&board,&request,budget,wall).unwrap();
        // Run this isolated test under a single-test supervisor: environment is process-wide.
        struct EnvRestore(Option<std::ffi::OsString>);
        impl Drop for EnvRestore { fn drop(&mut self) { unsafe { if let Some(value)=&self.0 { std::env::set_var("P7_COMPACT_PLANAR", value); } else { std::env::remove_var("P7_COMPACT_PLANAR"); } } } }
        let _restore = EnvRestore(std::env::var_os("P7_COMPACT_PLANAR"));
        unsafe { std::env::set_var("P7_COMPACT_PLANAR", "1"); }
        let run=|r: &board_mesh::Request| solve_checked_mesh_impl(&mesh,r,CheckedGrid{frequencies_hz:&[1000.],reference_ohm:&[50.,50.],max_iterations:1},None,&[],None,PartitionPolicy::Legacy,budget,Instant::now(),wall);
        let result=run(&request);
        let mut broken=request.clone(); broken.ports[1][1]="missing".into();
        let rejected=run(&broken);
        let result=result.unwrap();
        assert_eq!(result["coverage"],"unverified");
        assert_eq!(result["samples"][0]["columns"].as_array().unwrap().len(),2);
        assert!(rejected.is_err());
        let attachment: super::super::checked_packages::Attachment = serde_json::from_value(json!({"id":"bridge","electrodes":["a","b"],"model":{"id":"resistor","kind":"network","ports":["terminals"],"data":{"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["positive","negative"],"currentDofs":["series"],"incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.]],"resistanceOhm":[[0,0,25.]],"inductanceH":[],"capacitanceF":[],"structuralGaugeModes":[[1.,1.]],"conservedPotentialModes":[],"portTerminals":[{"positive":{"node":"positive"},"negative":{"node":"negative"}}]},"validity":{"status":"conditional","domain":{"bandHz":[900.,1100.]}},"analyses":["networkParameters"]}})).unwrap();
        let packages=super::super::checked_packages::validate(vec![attachment],&request,&[1000.]).unwrap();
        let with_package=solve_checked_mesh_impl(&mesh,&request,CheckedGrid{frequencies_hz:&[1000.],reference_ohm:&[50.,50.],max_iterations:1},None,&packages,None,PartitionPolicy::Legacy,budget,Instant::now(),wall).unwrap();
        assert!(with_package["provenance"]["plannedBytes"].as_u64().unwrap() <= budget.max_bytes as u64);
        assert!(with_package["provenance"]["maxMemoryMiB"].as_f64().unwrap() <= 512.);
        for column in with_package["samples"][0]["columns"].as_array().unwrap() {
            assert_eq!(column["packages"].as_array().unwrap().len(),1);
            assert!(column["powerBalanceErrorW"].as_f64().unwrap() < 1e-8);
        }
        let mut dielectric=request.clone();
        dielectric.stack.iter_mut().find(|layer| !layer.copper).unwrap().epsilon_r=Some(4.);
        let layered=run(&dielectric).unwrap();
        assert_eq!(layered["coverage"],"unverified");
        assert_eq!(layered["operator"],"compact-planar-layered-prototype");
        assert_eq!(layered["samples"][0]["columns"].as_array().unwrap().len(),2);
        unsafe { std::env::remove_var("P7_COMPACT_PLANAR"); }
        let automatic=solve_checked_mesh_impl(&mesh,&dielectric,CheckedGrid{frequencies_hz:&[1000.],reference_ohm:&[50.,50.],max_iterations:1},None,&[],None,PartitionPolicy::AutomaticHybridV1,budget,Instant::now(),wall).unwrap();
        assert_eq!(automatic["provenance"]["partitionPolicy"],"automatic-hybrid-v1");
        assert_eq!(automatic["provenance"]["hybridAssembly"]["method"],"global-compact-mixed");
        assert_eq!(automatic["provenance"]["compactOperator"]["basis"],"adaptive-mixed-closure");
        assert!(automatic["provenance"].get("automaticPartition").is_none());
        assert!(automatic.get("conductiveCorrection").is_none());
        assert_eq!(automatic["coverage"],"unverified");
    }
}
impl MixedMap<'_> {
    pub(super) fn electric_node_trace(&self) -> Vec<TraceRow> {
        self.full.node_potential.iter().map(|(&node,&full)| TraceRow { node, weights: vec![(self.potential_map[full], 1.)] }).collect()
    }
}
impl MixedMap<'_> {
    pub(super) fn planar_barycentric_supports(&self, modes: &[Vec<f64>]) -> Result<(Vec<super::planar_operator::Support>, Vec<LocalSupport>, Vec<Vec<f64>>)> {
        self.reject_barrel_prisms("barycentric planar charge model")?;
        let (_, original_locals, _) = self.planar_supports(modes)?;
        let local_count = original_locals.len();
        let nxy = self.mesh.planar_nodes_mm.len();
        let mut triangles = Vec::with_capacity(self.foils.len());
        let mut masses = BTreeMap::<usize,f64>::new();
        for foil in &self.foils {
            let ids = self.mesh.planar_triangles[foil.planar_triangle];
            let mut nodes = [usize::MAX;3];
            for &t in &foil.tetrahedra { let tet=&self.mesh.mesh.copper_tetrahedra[t]; let cell=&self.full.cells[t];
                for (&node,&p) in tet.nodes.iter().zip(&cell.potentials) { for k in 0..3 { if node % nxy == ids[k] { let m=self.potential_map[p]; if nodes[k]!=usize::MAX && nodes[k]!=m { return Err(error("HYBRID_CHARGE","inconsistent foil vertex ownership")); } nodes[k]=m; } } }
            }
            if nodes.contains(&usize::MAX) { return Err(error("HYBRID_CHARGE","missing foil vertex")); }
            for &node in &nodes { *masses.entry(node).or_default() += foil.volume_m3/3.; }
            triangles.push((ids,nodes));
        }
        let mut local_nodes=Vec::with_capacity(local_count);
        for (t,owner) in self.owners.iter().enumerate() { if !matches!(owner,Owner::Volume) {continue;} let cell=&self.full.cells[t]; let nodes=cell.potentials.map(|p|self.potential_map[p]);
            for &node in &nodes {*masses.entry(node).or_default() += self.mesh.mesh.copper_tetrahedra[t].volume_m3/4.;}
            local_nodes.push(nodes);
        }
        let keys: Vec<_> = masses.keys().copied().collect();
        let lookup: BTreeMap<_,_> = keys.iter().enumerate().map(|(i,&p)|(p,i)).collect();
        let nq=keys.len(); let mut dual=vec![vec![0.;self.potential_count()];nq];
        let mut supports=Vec::with_capacity(6*self.foils.len());
        for (fi,foil) in self.foils.iter().enumerate() {
            let (ids,nodes)=triangles[fi];
            let xy=ids.map(|i| {let p=self.mesh.mesh.nodes_m[i];[p[0],p[1]]});
            let center=[(xy[0][0]+xy[1][0]+xy[2][0])/3.,(xy[0][1]+xy[1][1]+xy[2][1])/3.];
            let span=foil.z_span_m;
            let density=self.full.cells[foil.tetrahedra[0]].density_per_amp_m2*self.current_map[3*foil.tetrahedra[0]].ok_or_else(||error("HYBRID_INPUT","foil current mapping missing"))?.weight;
            let current_density:Vec<_>=modes.iter().map(|mode|[density*mode[foil.first_current],density*mode[foil.first_current+1],0.]).collect();
            if nodes[0]==nodes[1] && nodes[1]==nodes[2] {
                let q=lookup[&nodes[0]]; let mass=masses[&nodes[0]];
                dual[q][nodes[0]] += foil.volume_m3/mass;
                let mut triangle=xy;
                let cross=(triangle[1][0]-triangle[0][0])*(triangle[2][1]-triangle[0][1])-(triangle[1][1]-triangle[0][1])*(triangle[2][0]-triangle[0][0]);
                if cross<0. {triangle.swap(1,2);}
                let prism=super::prism::Prism::new(format!("foil-{fi}-coalesced"),triangle,span)?;
                let mut charge_density=vec![0.;nq]; charge_density[q]=1./mass;
                supports.push(super::planar_operator::Support {prism,charge_density,current_density});
                continue;
            }
            for k in 0..3 {
                let q=lookup[&nodes[k]]; let mass=masses[&nodes[k]];
                for j in 0..3 { dual[q][nodes[j]] += foil.volume_m3/3./mass * if j==k {11./18.} else {7./36.}; }
                for neighbor in [(k+1)%3,(k+2)%3] {
                    let mid=[(xy[k][0]+xy[neighbor][0])/2.,(xy[k][1]+xy[neighbor][1])/2.];
                    let mut triangle=[xy[k],mid,center];
                    let cross=(triangle[1][0]-triangle[0][0])*(triangle[2][1]-triangle[0][1])-(triangle[1][1]-triangle[0][1])*(triangle[2][0]-triangle[0][0]);
                    if cross<0. {triangle.swap(1,2);}
                    let prism=super::prism::Prism::new(format!("foil-{fi}-vertex-{k}-edge-{neighbor}"),triangle,span)?;
                    let mut charge_density=vec![0.;nq];charge_density[q]=1./mass;
                    supports.push(super::planar_operator::Support {prism,charge_density,current_density:current_density.clone()});
                }
            }
        }
        let mut locals=Vec::with_capacity(24*local_count);
        for (index, original) in original_locals.iter().enumerate() {
            let nodes=local_nodes[index]; let points=original.tetra;
            let centroid: [f64;3]=std::array::from_fn(|d|points.iter().map(|p|p[d]).sum::<f64>()/4.);
            if nodes.iter().all(|&node|node==nodes[0]) {
                let q=lookup[&nodes[0]]; let mass=masses[&nodes[0]];
                dual[q][nodes[0]]+=original.volume/mass;
                locals.push(LocalSupport { tetra:points, volume:original.volume, currents:original.currents.clone(), charge_index:q, charge_density:1./mass });
                continue;
            }
            for k in 0..4 {
                let q=lookup[&nodes[k]]; let mass=masses[&nodes[k]];
                for j in 0..4 { dual[q][nodes[j]] += original.volume/4./mass * if j==k {25./48.} else {23./144.}; }
                for a in 0..4 {if a==k {continue;} for b in 0..4 {if b==k || b==a {continue;}
                    let midpoint=std::array::from_fn(|d|(points[k][d]+points[a][d])/2.);
                    let face=std::array::from_fn(|d|(points[k][d]+points[a][d]+points[b][d])/3.);
                    locals.push(LocalSupport { tetra:[points[k],midpoint,face,centroid], volume:original.volume/24., currents:original.currents.clone(), charge_index:q, charge_density:1./mass });
                }}
            }
        }
        for row in &dual { let total:f64=row.iter().sum(); if !total.is_finite() || (total-1.).abs()>1e-10 {return Err(error("HYBRID_CHARGE","barycentric dual does not preserve constants"));} }
        Ok((supports,locals,dual))
    }
}
#[cfg(test)]
mod compact_via_dispatch_test {
    use super::*;
    use serde_json::json;
    fn imported_via_fixture() -> (BoardMesh, board_mesh::Request, AllocationBudget) {
        let board=crate::import_json(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
        let mut request:board_mesh::Request=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json")).unwrap();
        request.cut_planes.clear(); request.allow_insulated_copper_cuts=false; request.domain_polygon_mm=None;
        request.max_vertical_step_m=0.00025;
        request.electrodes=["s0","s1","r0","r1"].into_iter().enumerate().map(|(i,name)| Electrode {name:name.into(),source_path:Some(format!("pads[{i}]")),source_paths:None,layer:if i==0 {"TOP"} else {"BOTTOM"}.into(),upper:i!=0}).collect();
        request.ports=vec![["s0".into(),"r0".into()],["s1".into(),"r1".into()]];
        for layer in &mut request.stack {if !layer.copper {layer.epsilon_r=Some(1.);}}
        let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let mesh=board_mesh::build(&board,&request,budget,Duration::from_secs(25)).unwrap();
        (mesh, request, budget)
    }

    #[test]
    fn imported_via_connects_both_ports_and_rejects_missing_contact() {
        let (mesh, mut request, budget) = imported_via_fixture();
        let wall = Duration::from_secs(25);
        let map = prepare(&mesh, &request.electrodes, 1000., budget, wall).unwrap();
        assert!(!mesh.barrel_ids.is_empty());
        let [s0, s1, r0, r1] = map.electrodes[..] else { panic!("expected four electrodes") };
        let component = |dof| map.potential_components.iter().position(|group| group.contains(&dof)).unwrap();
        assert_ne!(s0, s1);
        assert_ne!(r0, r1);
        assert_eq!(component(s0), component(s1));
        assert_eq!(component(r0), component(r1));
        assert_ne!(component(s0), component(r0));

        request.ports[1][1] = "missing".into();
        let error = solve_checked_mesh_impl(
            &mesh, &request, CheckedGrid { frequencies_hz: &[1000.], reference_ohm: &[50., 50.], max_iterations: 1 },
            None, &[], None, PartitionPolicy::Legacy, budget, Instant::now(), wall,
        ).unwrap_err();
        assert!(error.starts_with("MIXED_FOIL_CONTACT:"), "{error}");
    }

    #[test]
    #[ignore = "full volumetric via benchmark; evaluates tens of thousands of magnetic pairs"]
    fn imported_via_two_drives_full_frequency_benchmark() {
        let (mesh, request, budget) = imported_via_fixture();
        let wall = Duration::from_secs(1800);
        struct Restore(Vec<(&'static str,Option<std::ffi::OsString>)>);
        impl Drop for Restore {fn drop(&mut self){for (key,value) in &self.0 {unsafe {if let Some(v)=value {std::env::set_var(key,v)}else{std::env::remove_var(key)}}}}}
        let _restore=Restore(["P7_COMPACT_PLANAR","P7_COMPACT_FULL_MIXED"].into_iter().map(|key|(key,std::env::var_os(key))).collect());
        unsafe {std::env::set_var("P7_COMPACT_PLANAR","1");std::env::set_var("P7_COMPACT_FULL_MIXED","1");}
        let run=|r:&board_mesh::Request|solve_checked_mesh_impl(&mesh,r,CheckedGrid{frequencies_hz:&[1000.],reference_ohm:&[50.,50.],max_iterations:1},None,&[],None,PartitionPolicy::Legacy,budget,Instant::now(),wall);
        let result=run(&request).unwrap();
        assert_eq!(result["coverage"],json!("unverified"));
        assert!(result["provenance"]["volumeTetrahedra"].as_u64().unwrap()>0);
        assert_eq!(result["samples"][0]["columns"].as_array().unwrap().len(),2);
    }
}
impl MixedMap<'_> {
    /// Sparse current densities on each original foil owner, independent of
    /// any reduced basis. Local tetrahedra are handled by the mixed action.
    pub(super) fn planar_current_supports(&self) -> Result<Vec<super::planar_operator::SparseSupport>> {
        let mut out=Vec::with_capacity(self.foils.len());
        for (i,foil) in self.foils.iter().enumerate() {
            let ids=self.mesh.planar_triangles[foil.planar_triangle];
            let mut xy=ids.map(|id|{let p=self.mesh.mesh.nodes_m[id];[p[0],p[1]]});
            let cross=(xy[1][0]-xy[0][0])*(xy[2][1]-xy[0][1])-(xy[1][1]-xy[0][1])*(xy[2][0]-xy[0][0]); if cross<0. {xy.swap(1,2);}
            let span=foil.z_span_m;
            let prism=super::prism::Prism::new(format!("foil-current-{i}"),xy,span)?;
            let density=self.full.cells[foil.tetrahedra[0]].density_per_amp_m2*self.current_map[3*foil.tetrahedra[0]].ok_or_else(||error("HYBRID_INPUT","foil current mapping missing"))?.weight;
            out.push(super::planar_operator::SparseSupport {prism,charge_density:vec![],current_density:vec![(foil.first_current,[density,0.,0.]),(foil.first_current+1,[0.,density,0.])]});
        }
        Ok(out)
    }
}
impl MixedMap<'_> {
    /// z-directed uniform current density of each thin-wall barrel prism
    /// (one mode per prism), in the order of `self.barrels`.
    pub(super) fn barrel_current_supports(&self) -> Result<Vec<super::planar_operator::SparseSupport>> {
        let mut out=Vec::with_capacity(self.barrels.len());
        for (i,prism) in self.barrels.iter().enumerate() {
            let ids=self.mesh.planar_triangles[prism.planar_triangle];
            let mut xy=ids.map(|id|{let p=self.mesh.mesh.nodes_m[id];[p[0],p[1]]});
            let cross=(xy[1][0]-xy[0][0])*(xy[2][1]-xy[0][1])-(xy[1][1]-xy[0][1])*(xy[2][0]-xy[0][0]); if cross<0. {xy.swap(1,2);}
            let shape=super::prism::Prism::new(format!("barrel-current-{i}"),xy,prism.z_span_m)?;
            let first=prism.tetrahedra[0];
            let density=self.full.cells[first].density_per_amp_m2*self.current_map[3*first+2].ok_or_else(||error("HYBRID_INPUT","barrel current mapping missing"))?.weight;
            out.push(super::planar_operator::SparseSupport {prism:shape,charge_density:vec![],current_density:vec![(prism.mode,[0.,0.,density])]});
        }
        Ok(out)
    }
    fn reject_barrel_prisms(&self, what: &str) -> Result<()> {
        if self.barrels.is_empty() { Ok(()) } else { Err(error("HYBRID_INPUT", &format!("{what} does not support thin-wall barrel prisms"))) }
    }
}
pub(super) struct LocalCurrentSupport { pub tetra:[[f64;3];4], pub densities:Vec<(usize,[f64;3])> }
impl MixedMap<'_> {
    pub(super) fn local_current_supports(&self) -> Result<Vec<LocalCurrentSupport>> {
        let mut out=Vec::new();
        for (i,owner) in self.owners.iter().enumerate() {
            if !matches!(owner,Owner::Volume) {continue;}
            let cell=&self.full.cells[i]; let tetra=self.mesh.mesh.copper_tetrahedra[i].nodes.map(|n|self.mesh.mesh.nodes_m[n]);
            let mut densities=Vec::with_capacity(3);
            for axis in 0..3 {let entry=self.current_map[3*i+axis].ok_or_else(||error("HYBRID_INPUT","local current mapping missing"))?; let mut v=[0.;3]; v[axis]=entry.weight*cell.density_per_amp_m2; densities.push((entry.mode,v));}
            out.push(LocalCurrentSupport {tetra,densities});
        }
        Ok(out)
    }
}

impl MixedMap<'_> {
    pub(super) fn planar_sparse_charge_supports(&self, modes: &[Vec<f64>]) -> Result<(Vec<super::planar_operator::SparseSupport>, Vec<LocalSupport>, Vec<Vec<(usize,f64)>>)> {
        self.reject_barrel_prisms("sparse planar charge model")?;
        let original_locals = self.local_current_supports()?.into_iter().map(|s| LocalSupport { tetra:s.tetra, volume: { let a=s.tetra[0]; let b=s.tetra[1]; let c=s.tetra[2]; let d=s.tetra[3]; let u=[b[0]-a[0],b[1]-a[1],b[2]-a[2]]; let v=[c[0]-a[0],c[1]-a[1],c[2]-a[2]]; let w=[d[0]-a[0],d[1]-a[1],d[2]-a[2]]; (u[0]*(v[1]*w[2]-v[2]*w[1])-u[1]*(v[0]*w[2]-v[2]*w[0])+u[2]*(v[0]*w[1]-v[1]*w[0])).abs()/6. }, currents:vec![],charge_index:0,charge_density:0. }).collect::<Vec<_>>();
        let local_count = original_locals.len();
        let nxy = self.mesh.planar_nodes_mm.len();
        let mut triangles = Vec::with_capacity(self.foils.len());
        let mut masses = BTreeMap::<usize,f64>::new();
        for foil in &self.foils {
            let ids = self.mesh.planar_triangles[foil.planar_triangle];
            let mut nodes = [usize::MAX;3];
            for &t in &foil.tetrahedra { let tet=&self.mesh.mesh.copper_tetrahedra[t]; let cell=&self.full.cells[t];
                for (&node,&p) in tet.nodes.iter().zip(&cell.potentials) { for k in 0..3 { if node % nxy == ids[k] { let m=self.potential_map[p]; if nodes[k]!=usize::MAX && nodes[k]!=m { return Err(error("HYBRID_CHARGE","inconsistent foil vertex ownership")); } nodes[k]=m; } } }
            }
            if nodes.contains(&usize::MAX) { return Err(error("HYBRID_CHARGE","missing foil vertex")); }
            for &node in &nodes { *masses.entry(node).or_default() += foil.volume_m3/3.; }
            triangles.push((ids,nodes));
        }
        let mut local_nodes=Vec::with_capacity(local_count);
        for (t,owner) in self.owners.iter().enumerate() { if !matches!(owner,Owner::Volume) {continue;} let cell=&self.full.cells[t]; let nodes=cell.potentials.map(|p|self.potential_map[p]);
            for &node in &nodes {*masses.entry(node).or_default() += self.mesh.mesh.copper_tetrahedra[t].volume_m3/4.;}
            local_nodes.push(nodes);
        }
        let keys: Vec<_> = masses.keys().copied().collect();
        let lookup: BTreeMap<_,_> = keys.iter().enumerate().map(|(i,&p)|(p,i)).collect();
        let nq=keys.len(); let mut dual=vec![BTreeMap::<usize,f64>::new();nq];
        let mut supports=Vec::with_capacity(6*self.foils.len());
        for (fi,foil) in self.foils.iter().enumerate() {
            let (ids,nodes)=triangles[fi];
            let xy=ids.map(|i| {let p=self.mesh.mesh.nodes_m[i];[p[0],p[1]]});
            let center=[(xy[0][0]+xy[1][0]+xy[2][0])/3.,(xy[0][1]+xy[1][1]+xy[2][1])/3.];
            let span=foil.z_span_m;
            let density=self.full.cells[foil.tetrahedra[0]].density_per_amp_m2*self.current_map[3*foil.tetrahedra[0]].ok_or_else(||error("HYBRID_INPUT","foil current mapping missing"))?.weight;
            let current_density:Vec<_>=modes.iter().map(|mode|[density*mode[foil.first_current],density*mode[foil.first_current+1],0.]).collect();
            if nodes[0]==nodes[1] && nodes[1]==nodes[2] {
                let q=lookup[&nodes[0]]; let mass=masses[&nodes[0]];
                *dual[q].entry(nodes[0]).or_default() += foil.volume_m3/mass;
                let mut triangle=xy;
                let cross=(triangle[1][0]-triangle[0][0])*(triangle[2][1]-triangle[0][1])-(triangle[1][1]-triangle[0][1])*(triangle[2][0]-triangle[0][0]);
                if cross<0. {triangle.swap(1,2);}
                let prism=super::prism::Prism::new(format!("foil-{fi}-coalesced"),triangle,span)?;
                let charge_density=vec![(q,1./mass)];
                supports.push(super::planar_operator::SparseSupport {prism,charge_density,current_density:current_density.iter().enumerate().filter_map(|(i,&v)|(v!=[0.;3]).then_some((i,v))).collect()});
                continue;
            }
            for k in 0..3 {
                let q=lookup[&nodes[k]]; let mass=masses[&nodes[k]];
                for j in 0..3 { *dual[q].entry(nodes[j]).or_default() += foil.volume_m3/3./mass * if j==k {11./18.} else {7./36.}; }
                for neighbor in [(k+1)%3,(k+2)%3] {
                    let mid=[(xy[k][0]+xy[neighbor][0])/2.,(xy[k][1]+xy[neighbor][1])/2.];
                    let mut triangle=[xy[k],mid,center];
                    let cross=(triangle[1][0]-triangle[0][0])*(triangle[2][1]-triangle[0][1])-(triangle[1][1]-triangle[0][1])*(triangle[2][0]-triangle[0][0]);
                    if cross<0. {triangle.swap(1,2);}
                    let prism=super::prism::Prism::new(format!("foil-{fi}-vertex-{k}-edge-{neighbor}"),triangle,span)?;
                    let charge_density=vec![(q,1./mass)];
                    supports.push(super::planar_operator::SparseSupport {prism,charge_density,current_density:current_density.iter().enumerate().filter_map(|(i,&v)|(v!=[0.;3]).then_some((i,v))).collect()});
                }
            }
        }
        let mut locals=Vec::with_capacity(24*local_count);
        for (index, original) in original_locals.iter().enumerate() {
            let nodes=local_nodes[index]; let points=original.tetra;
            let centroid: [f64;3]=std::array::from_fn(|d|points.iter().map(|p|p[d]).sum::<f64>()/4.);
            if nodes.iter().all(|&node|node==nodes[0]) {
                let q=lookup[&nodes[0]]; let mass=masses[&nodes[0]];
                *dual[q].entry(nodes[0]).or_default()+=original.volume/mass;
                locals.push(LocalSupport { tetra:points, volume:original.volume, currents:original.currents.clone(), charge_index:q, charge_density:1./mass });
                continue;
            }
            for k in 0..4 {
                let q=lookup[&nodes[k]]; let mass=masses[&nodes[k]];
                for j in 0..4 { *dual[q].entry(nodes[j]).or_default() += original.volume/4./mass * if j==k {25./48.} else {23./144.}; }
                for a in 0..4 {if a==k {continue;} for b in 0..4 {if b==k || b==a {continue;}
                    let midpoint=std::array::from_fn(|d|(points[k][d]+points[a][d])/2.);
                    let face=std::array::from_fn(|d|(points[k][d]+points[a][d]+points[b][d])/3.);
                    locals.push(LocalSupport { tetra:[points[k],midpoint,face,centroid], volume:original.volume/24., currents:original.currents.clone(), charge_index:q, charge_density:1./mass });
                }}
            }
        }
        for row in &dual { let total:f64=row.values().sum(); if !total.is_finite() || (total-1.).abs()>1e-10 {return Err(error("HYBRID_CHARGE","barycentric dual does not preserve constants"));} }
        Ok((supports,locals,dual.into_iter().map(|row|row.into_iter().collect()).collect()))
    }
}
#[cfg(test)]
mod hybrid_rf_policy_tests {
    use super::*;
    #[test]
    fn hybrid_band_requires_policy_and_preserves_existing_screens() {
        let mut request:board_mesh::Request=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json")).unwrap();
        request.cut_planes.clear();request.allow_insulated_copper_cuts=false;request.domain_polygon_mm=None;
        request.roi_mm=[[0.,0.],[0.00001,0.00001]];
        for layer in &mut request.stack {layer.thickness_m=1e-7;}
        request.electrodes=vec![Electrode{name:"a".into(),source_path:Some("pads[0]".into()),source_paths:None,layer:"TOP".into(),upper:false},Electrode{name:"b".into(),source_path:Some("pads[1]".into()),source_paths:None,layer:"TOP".into(),upper:false}];
        request.ports=vec![["a".into(),"b".into()]];
        let hybrid=CheckedOperator::HybridPlanarV3;
        assert!(serde_json::from_str::<CheckedOperator>("\"hybrid-planar-v3\"").unwrap()==hybrid);
        assert!(hybrid.admits(1000.) && hybrid.admits(10_000_000_000.) && !hybrid.admits(10_000_000_001.));
        assert!(CheckedOperator::Legacy.admits(10_000.) && !CheckedOperator::Legacy.admits(10_001.));
        assert!(CheckedOperator::SkinVolumeV3.admits(100_000.) && !CheckedOperator::SkinVolumeV3.admits(100_001.));
        assert!(hybrid.validate_partition(PartitionPolicy::Legacy).is_err());
        assert!(hybrid.validate_partition(PartitionPolicy::AutomaticConductiveSchurV1).is_err());
        assert!(hybrid.validate_partition(PartitionPolicy::AutomaticHybridV1).is_ok());
        let profile=hybrid.skin_profile(10_000_000_000.,None,request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        assert!(profile.current_step_m<=profile.skin_depth_m(58e6)/5. && profile.field_step_m<=profile.skin_depth_m(58e6)/5.);
        let grid=CheckedGrid{frequencies_hz:&[10_000_000_000.],reference_ohm:&[50.],max_iterations:1};
        assert!(checked_reserve(&request,grid,Some(profile)).is_err());
        assert!(checked_reserve_policy(&request,grid,Some(profile),hybrid,PartitionPolicy::Legacy).is_err());
        assert!(checked_reserve_policy(&request,grid,Some(profile),hybrid,PartitionPolicy::AutomaticHybridV1).is_ok());
        let mut large=request.clone();large.roi_mm=[[0.,0.],[100.,100.]];
        assert!(checked_reserve_policy(&large,grid,Some(profile),hybrid,PartitionPolicy::AutomaticHybridV1).unwrap_err().contains("MIXED_FOIL_BAND"));
        assert_eq!(band_kd_from(None,true).unwrap(),None);
        assert_eq!(band_kd_from(Some("0.3"),false).unwrap(),None);
        assert_eq!(band_kd_from(Some("0.5"),true).unwrap(),Some(0.5));
        for bad in ["0","-0.1","0.51","1","abc","nan",""] { assert!(band_kd_from(Some(bad),true).unwrap_err().contains("BOARD_FOIL_SCOPE"),"{bad}"); }
        let mut missing=request.clone();missing.electrodes[0].source_path=Some("copper.fixed.tracks[0]".into());
        assert!(checked_reserve_policy(&missing,grid,Some(profile),hybrid,PartitionPolicy::AutomaticHybridV1).unwrap_err().contains("MIXED_FOIL_CONTACT"));
    }
}

#[cfg(test)]
mod aligned_pad_orientation_tests {
    use super::*;
    #[test]
    fn inner_signal_with_split_top_and_bottom_references_stays_disconnected() {
        let cases:serde_json::Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/orientation-aligned-pads.json")).unwrap();
        let mut board_json:serde_json::Value=serde_json::from_str(cases[0][0].as_str().unwrap()).unwrap();
        board_json["layers"]=serde_json::json!([{ "name":"TOP","index":0,"side":"top" },{ "name":"INNER","index":1,"side":"inner" },{ "name":"BOTTOM","index":2,"side":"bottom" }]);
        let pads=board_json["pads"].as_array_mut().unwrap();
        pads[0]["layers"]=serde_json::json!(["INNER"]);pads[1]["layers"]=serde_json::json!(["INNER"]);
        for i in 2..4 {
            let mut p=pads[i].clone();p["id"]=serde_json::json!(format!("top-return-{i}"));p["number"]=serde_json::json!("3");p["layers"]=serde_json::json!(["TOP"]);pads.push(p);
        }
        let tracks=board_json["copper"]["fixed"]["tracks"].as_array_mut().unwrap();
        tracks.truncate(1);tracks[0]["layer"]=serde_json::json!("INNER");
        let board=crate::import_json(&board_json.to_string()).board.unwrap();
        let mut request_json=cases[0][1].clone();
        request_json["stack"]=serde_json::json!([
            {"name":"TOP","copper":true,"thicknessM":3.5e-5,"conductivitySM":58e6},
            {"name":"upper-dielectric","copper":false,"thicknessM":1e-4,"epsilonR":4.},
            {"name":"INNER","copper":true,"thicknessM":3.5e-5,"conductivitySM":58e6},
            {"name":"lower-dielectric","copper":false,"thicknessM":1e-4,"epsilonR":4.},
            {"name":"BOTTOM","copper":true,"thicknessM":3.5e-5,"conductivitySM":58e6}
        ]);
        for i in 0..2 {request_json["electrodes"][i]["layer"]=serde_json::json!("INNER");}
        let request:board_mesh::Request=serde_json::from_value(request_json).unwrap();
        let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000};
        let wall=Duration::from_secs(15);let started=Instant::now();
        let profile=CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap();
        let mesh=board_mesh::build_profiled(&board,&request,budget,wall,None,&[],profile).unwrap();
        let map=prepare(&mesh,&request.electrodes,1e6,budget,wall).unwrap();
        assert_eq!(map.full.components.len(),5,"one inner signal and four separated references");
        let mut layers=std::collections::BTreeMap::<&str,std::collections::BTreeSet<usize>>::new();
        for cell in &map.full.cells {
            let tet=&mesh.mesh.copper_tetrahedra[cell.mesh_cell];
            let points=tet.nodes.map(|i|mesh.mesh.nodes_m[i]);
            let z=points.iter().map(|p|p[2]).sum::<f64>()/4.;
            let (name,_)=mesh.layer_spans.iter().find(|(_,span)|z>span[0] && z<span[1]).unwrap();
            let component=map.full.components.iter().position(|c|c.contains(&cell.potentials[0])).unwrap();
            layers.entry(name.as_str()).or_default().insert(component);
            if name=="TOP" || name=="BOTTOM" {
                for p in points {let x=mesh.origin_mm[0]+1000.*p[0];assert!(x.abs()>=0.199,"reference gap filled at x={x}");}
            }
        }
        assert_eq!(layers["INNER"].len(),1);
        assert_eq!(layers["TOP"].len(),2);
        assert_eq!(layers["BOTTOM"].len(),2);
        assert!(layers["TOP"].is_disjoint(&layers["BOTTOM"]));
        assert!(layers["INNER"].is_disjoint(&layers["TOP"]) && layers["INNER"].is_disjoint(&layers["BOTTOM"]));
        assert!(started.elapsed()<wall);
    }
    #[test]
    fn aligned_rotations_and_reversed_tracks_keep_planar_ownership() {
        let cases:serde_json::Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/orientation-aligned-pads.json")).unwrap();
        let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000};
        let wall=Duration::from_secs(5);
        for case in cases.as_array().unwrap() {for reversed in [false,true] {
            let mut board_json:serde_json::Value=serde_json::from_str(case[0].as_str().unwrap()).unwrap();
            if reversed {for track in board_json["copper"]["fixed"]["tracks"].as_array_mut().unwrap() {track["points"].as_array_mut().unwrap().reverse();}}
            let board=crate::import_json(&board_json.to_string()).board.unwrap();
            let request:board_mesh::Request=serde_json::from_value(case[1].clone()).unwrap();
            let profile=CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap();
            let mesh=board_mesh::build_profiled(&board,&request,budget,wall,None,&[],profile).unwrap();
            let map=prepare(&mesh,&request.electrodes,1e6,budget,wall).unwrap();
            assert!(map.owners.iter().all(|o|matches!(o,Owner::Foil(_))),"aligned pad edges produced local slivers");
        }}
    }
    #[test]
    fn physical_narrow_pad_gap_remains_open() {
        let cases:serde_json::Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/orientation-aligned-pads.json")).unwrap();
        let mut board_json:serde_json::Value=serde_json::from_str(cases[0][0].as_str().unwrap()).unwrap();
        board_json["copper"]["fixed"]["tracks"]=serde_json::json!([]);
        for (i,pad) in board_json["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
            pad["at"]["x"]=serde_json::json!(if i%2==0 {-0.4000001} else {0.4000001});
        }
        let board=crate::import_json(&board_json.to_string()).board.unwrap();
        let request:board_mesh::Request=serde_json::from_value(cases[0][1].clone()).unwrap();
        let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000};
        let wall=Duration::from_secs(5);
        let mesh=board_mesh::build(&board,&request,budget,wall).unwrap();
        let map=prepare(&mesh,&request.electrodes,1e6,budget,wall).unwrap();
        assert!(map.full.components.len()>=4,"real pad gaps must not merge conductor islands");
    }
}

#[cfg(test)]
mod barrel_noise_test {
    use super::*;
    #[test]
    fn needle_wall_column_is_flagged_by_conductance_share_per_barrel() {
        let seg = |barrel, triangle| BarrelSegment { barrel, triangle, span: [0., 1e-3], cells: vec![] };
        // Barrel 0: 20 normal columns (1e-9 m2) and one needle (1e-16 m2). Barrel 1: one column only.
        let mut segments: Vec<_> = (0..21).map(|t| seg(0, t)).collect();
        segments.push(seg(1, 100));
        // Two segments over the same column must count the column once.
        segments.push(seg(0, 20));
        let area = |t: usize| if t == 20 { 1e-16 } else { 1e-9 };
        let noise = barrel_noise_columns(&segments, &area);
        assert_eq!(noise, BTreeSet::from([(0, 20)]));
        // A lone column is never noise even if tiny.
        assert!(barrel_noise_columns(&[seg(1, 100)], &|_| 1e-20).is_empty());
        // Uniform columns: nothing flagged.
        assert!(barrel_noise_columns(&segments[..20], &|_| 1e-9).is_empty());
    }
}

#[cfg(test)]
#[path = "hybrid_connector_test.rs"]
mod hybrid_connector_test;

#[cfg(test)]
#[path = "hybrid_ground_chain_test.rs"]
mod hybrid_ground_chain_test;

#[cfg(test)]
#[path = "hybrid_barrel_test.rs"]
pub(super) mod hybrid_barrel_test;

#[cfg(test)]
#[path = "microstrip_reference_test.rs"]
mod microstrip_reference_test;
