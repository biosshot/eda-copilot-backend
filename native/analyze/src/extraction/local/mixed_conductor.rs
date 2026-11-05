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
    volume_field::Field,
    AllocationBudget, Result,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MIXED_PREPARE_MAX_HZ: f64 = 100_000.;

#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub enum CheckedOperator {
    #[default]
    #[serde(skip)]
    Legacy,
    #[serde(rename = "skin-volume-v3")]
    SkinVolumeV3,
}

/// Internal production automatic routing policy; no public DSL syntax.
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub enum PartitionPolicy {
    #[default]
    #[serde(skip)]
    Legacy,
    #[serde(rename = "automatic-conductive-schur-v1")]
    AutomaticConductiveSchurV1,
}

impl CheckedOperator {
    pub(super) fn admits(self, frequency_hz: f64) -> bool {
        let upper = match self {
            Self::Legacy => 10_000.,
            Self::SkinVolumeV3 => MIXED_PREPARE_MAX_HZ,
        };
        frequency_hz.is_finite() && (1000. ..=upper).contains(&frequency_hz)
    }

    pub(super) fn axis_scope(self) -> &'static str {
        match self {
            Self::Legacy => "1..10 kHz",
            Self::SkinVolumeV3 => "1..100 kHz profiled implementation limit",
        }
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
            return Err(error("BOARD_FOIL_GRID", "profileUpperHz must cover the exact grid and stay within the 100 kHz implementation limit"));
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
        "scope": "Planar copper thickness and round barrel radial resolution screen; lateral, proximity, domain and band accuracy unverified",
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Foil(usize),
    Volume,
}
#[derive(Debug)]
pub struct FoilPrism {
    pub layer: String,
    pub planar_triangle: usize,
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
    pub electrodes: Vec<usize>,
    pub potential_components: Vec<Vec<usize>>,
    pub incidence: Vec<(usize, usize, f64)>,
    pub resistance_ohm: Vec<f64>,
    potential_map: Vec<usize>,
    full_electrodes: Vec<usize>,
    current_map: Vec<Option<CurrentEntry>>,
    pub planned_bytes: usize,
}
struct Candidate {
    layer: usize,
    triangle: usize,
    cells: Vec<usize>,
    volume: f64,
    foil_density: Option<f64>,
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

pub fn prepare<'a>(
    mesh: &'a BoardMesh,
    electrodes: &[Electrode],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    wall: Duration,
) -> Result<MixedMap<'a>> {
    let start = Instant::now();
    if !max_frequency_hz.is_finite() || !(0. ..=MIXED_PREPARE_MAX_HZ).contains(&max_frequency_hz) {
        return Err(error(
            "MIXED_FOIL_BAND",
            "finite explicit thin-foil screen within 0..100 kHz required",
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
    let mut by_key = BTreeMap::<(usize, usize), usize>::new();
    let mut candidates = Vec::<Candidate>::new();
    let mut groups = vec![None; nt];
    let mut barrel_cell = vec![false; nt];
    for (i, tet) in mesh.mesh.copper_tetrahedra.iter().enumerate() {
        check_time(start, wall)?;
        let z = tet.nodes.map(|j| mesh.mesh.nodes_m[j][2]);
        let lo = z.into_iter().fold(f64::INFINITY, f64::min);
        let hi = z.into_iter().fold(f64::NEG_INFINITY, f64::max);
        barrel_cell[i] = tet
            .source_ids
            .iter()
            .any(|id| barrels.contains(id.as_str()));
        let Some(layer) = layers
            .iter()
            .position(|(_, span)| lo >= span[0] && hi <= span[1])
        else {
            continue;
        };
        let triangle = tet.planar_triangle;
        let next = candidates.len();
        let group = *by_key.entry((layer, triangle)).or_insert_with(|| {
            candidates.push(Candidate {
                layer,
                triangle,
                cells: Vec::new(),
                volume: 0.,
                foil_density: None,
            });
            next
        });
        groups[i] = Some(group);
        candidates[group].cells.push(i);
    }
    for group in &mut candidates {
        let span = layers[group.layer].1;
        let thickness = span[1] - span[0];
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
        if !has_contested_pad_column
            && expected.is_finite()
            && expected > 0.
            && (volume - expected).abs() <= 1e-8 * expected
            && thickness <= twice / longest / 4.
            && delta >= 3. * thickness
            && group
                .cells
                .iter()
                .all(|&i| !barrel_cell[i] && mesh.conductivity_s_m[i] == conductivity)
        {
            let density = 1. / (thickness * (twice / 2.).sqrt());
            if density.is_finite() && density > 0. {
                group.foil_density = Some(density);
            }
        }
    }
    // One complete prism on the other side of each real conductor face next
    // to mandatory volume stays volumetric. This is a starting partition,
    // not a field-decay bound. No XY overlap alone creates a connection.
    let mandatory: Vec<_> = groups
        .iter()
        .map(|g| g.is_none_or(|j| candidates[j].foil_density.is_none()))
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
    let mut current_map = vec![None; full.current_count()];
    let mut parent: Vec<_> = (0..nv).collect();
    let mut foils = Vec::new();
    let mut nc = 0;
    for candidate in candidates {
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
            tetrahedra: candidate.cells,
            first_current: nc,
            volume_m3: candidate.volume,
        });
        nc += 2;
    }
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
    let mut ids = BTreeMap::new();
    let potential_map: Vec<_> = (0..nv)
        .map(|i| {
            let next = ids.len();
            *ids.entry(root(&parent, i)).or_insert(next)
        })
        .collect();
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
    for (i, cell) in full.cells.iter().enumerate() {
        for k in 0..3 {
            let Some(entry) = current_map[3 * i + k] else {
                continue;
            };
            resistance[entry.mode] = resistance[entry.mode].add(
                Sum::from(cell.resistance_ohm)
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
        electrodes,
        potential_components,
        incidence,
        resistance_ohm,
        potential_map,
        full_electrodes,
        current_map,
        planned_bytes,
    })
}
impl MixedMap<'_> {
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
    mut budget: AllocationBudget,
    wall: Duration,
) -> Result<serde_json::Value> {
    let start = Instant::now();
    let reserve = checked_reserve(request, grid, None)?;
    budget.max_bytes = budget
        .max_bytes
        .checked_sub(reserve)
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed checked response exceeds budget"))?;
    let mesh = board_mesh::build(board, request, budget, wall.saturating_sub(start.elapsed()))?;
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
    let CheckedGrid {
        frequencies_hz,
        reference_ohm,
        max_iterations,
    } = grid;
    let operator = if skin.is_some() {
        CheckedOperator::SkinVolumeV3
    } else {
        CheckedOperator::Legacy
    };
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
        || !(1..=16).contains(&max_iterations)
    {
        return Err(error(
            "MIXED_FOIL_GRID",
            &format!(
                "bounded {} exact grid, finite matched ports and enrichment budget required",
                operator.axis_scope()
            ),
        ));
    }
    if !request.cut_planes.is_empty() || request.allow_insulated_copper_cuts {
        return Err(error(
            "MIXED_FOIL_CONTACT",
            "closed physical copper and original finite pad faces required",
        ));
    }
    for electrode in &request.electrodes {
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
    let dx = 1e-3 * (request.roi_mm[1][0] - request.roi_mm[0][0]);
    let dy = 1e-3 * (request.roi_mm[1][1] - request.roi_mm[0][1]);
    let dz: f64 = request.stack.iter().map(|l| l.thickness_m).sum();
    let epsilon = request
        .stack
        .iter()
        .filter_map(|l| l.epsilon_r)
        .fold(1., f64::max);
    let size =
        2. * std::f64::consts::PI * maximum / 299792458. * dx.hypot(dy).hypot(dz) * epsilon.sqrt();
    if !size.is_finite() || size > 0.1 {
        return Err(error(
            "MIXED_FOIL_BAND",
            "quasistatic electrical-size screen failed",
        ));
    }

    let reserve = frequencies_hz.len() * port_count * (port_count * 256 + 4096) + 256 * 1024;
    Ok(reserve)
}

pub(super) fn solve_checked_mesh(
    mesh: BoardMesh,
    request: &board_mesh::Request,
    grid: CheckedGrid<'_>,
    skin: Option<copper_skin_profile::Profile>,
    packages: &[super::checked_packages::Validated],
    partition_policy: PartitionPolicy,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
) -> Result<serde_json::Value> {
    solve_checked_mesh_impl(
        &mesh,
        request,
        grid,
        skin,
        packages,
        None,
        partition_policy,
        budget,
        start,
        wall,
    )
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
    mut budget: AllocationBudget,
    wall: Duration,
) -> Result<serde_json::Value> {
    let start = Instant::now();
    if owners.is_some_and(|ids| ids.len() != mesh.mesh.copper_tetrahedra.len()) {
        return Err(error(
            "SHARED_RETURN_OWNERSHIP",
            "exact original Cu tetrahedron ownership required",
        ));
    }
    let reserve = checked_reserve(request, grid, None)?;
    budget.max_bytes = budget.max_bytes.checked_sub(reserve).ok_or_else(|| {
        error(
            "MEMORY_LIMIT",
            "shared return exact-grid output exceeds budget",
        )
    })?;
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

fn solve_checked_mesh_impl(
    mesh: &BoardMesh,
    request: &board_mesh::Request,
    grid: CheckedGrid<'_>,
    skin: Option<copper_skin_profile::Profile>,
    packages: &[super::checked_packages::Validated],
    partition_request: Option<(&[usize], usize)>,
    partition_policy: PartitionPolicy,
    mut budget: AllocationBudget,
    start: Instant,
    wall: Duration,
) -> Result<serde_json::Value> {
    use serde_json::json;
    let reserve = checked_reserve(request, grid, skin)?
        + super::checked_packages::output_reserve(
            packages,
            grid.frequencies_hz.len() * request.ports.len(),
        );
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
            if partition_policy == PartitionPolicy::AutomaticConductiveSchurV1 {
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

    let map = prepare(
        mesh,
        &request.electrodes,
        effective_upper,
        budget,
        wall.saturating_sub(start.elapsed()),
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
    )?;
    check_time(start, wall)?;
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
        checked["provenance"]["operatorPolicy"] = json!("skin-volume-v3");
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
        assert_eq!(serde_json::to_value(&geometry).unwrap(), before);
    }
}
