//! Bounded board-derived all-foil preparation for physical interface controls.
//! The common electric field retains finite board edges and voids. This probe
//! does not change production planning or admit via/mixed-volume interfaces.
use super::{
    board_mesh,
    capacitance::TraceRow,
    current_space::{self, Foil, VerifiedTrace},
    error,
    mixed_conductor::{self, CheckedOperator},
    volume_field::{DenseField, ElectricApply, Field},
    AllocationBudget, Result,
};
use crate::model::Board;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

/// An internal conforming line on one physical foil. Positions are absolute
/// board millimetres. A line subdivides the mesh but prescribes no potential.
pub struct TraceCut {
    pub id: String,
    pub layer: String,
    pub endpoints_mm: [[f64; 2]; 2],
}
#[derive(Debug)]
pub struct Knot {
    pub position_mm: [f64; 2],
    pub potential: usize,
}
#[derive(Debug)]
pub struct Support {
    pub layer: String,
    pub planar_triangle: usize,
    pub source_region_ids: Vec<String>,
    pub copper_volume_m3: f64,
}
pub struct Prepared {
    pub space: current_space::CurrentSpace,
    /// One provenance entry for every prism, in the current-cell order.
    pub supports: Vec<Support>,
    pub trace_knots: BTreeMap<String, Vec<Knot>>,
    pub source_mapping: Vec<(String, String, Option<String>, String)>,
    pub electrode_areas_m2: BTreeMap<String, f64>,
    pub max_copper_volume_change: f64,
    pub field_charge_energy_error: f64,
}

fn remaining(start: Instant, limit: Duration) -> Result<Duration> {
    limit
        .checked_sub(start.elapsed())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| error("DEADLINE", "board foil preparation deadline"))
}

#[derive(Default)]
struct PrismOwnership {
    volume_m3: f64,
    source_ids: BTreeSet<String>,
}

pub fn prepare(
    board: &Board,
    request: &board_mesh::Request,
    cuts: &[TraceCut],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    max_wall: Duration,
) -> Result<Prepared> {
    let start = Instant::now();
    let Geometry {
        mesh,
        foils,
        potential_nodes,
        ports,
        rows,
        supports,
        trace_knots,
        max_copper_volume_change,
    } = prepare_geometry(
        board,
        request,
        cuts,
        max_frequency_hz,
        budget,
        start,
        max_wall,
        true,
    )?;
    let field = Field::new_conforming_trace(&mesh.mesh, potential_nodes.len(), rows, budget)?;
    remaining(start, max_wall)?;
    let dense = DenseField::new(&field, budget)?;
    let mut capacitance_f = Vec::new();
    for j in 0..potential_nodes.len() {
        remaining(start, max_wall)?;
        let mut v = vec![0.; potential_nodes.len()];
        v[j] = 1.;
        for (i, q) in dense.apply(&v)?.into_iter().enumerate() {
            if q != 0. {
                capacitance_f.push((i, j, q));
            }
        }
    }
    let field_charge_energy_error = dense.max_charge_energy_column_error;
    let planned_bytes = dense.assembly_peak_bytes;
    drop(dense);
    drop(field);
    let max_epsilon_r = request
        .stack
        .iter()
        .filter_map(|l| l.epsilon_r)
        .fold(1., f64::max);
    let source_mapping = mesh.source_mapping;
    let electrode_areas_m2 = mesh.electrode_areas_m2;
    // The bounded electric factor and mesh are no longer needed in quadrature.
    drop(mesh.mesh);
    let space = current_space::assemble_verified_trace(
        &foils,
        VerifiedTrace {
            potential_nodes,
            capacitance_f,
            structural_gauge_modes: Vec::new(),
            max_epsilon_r,
            planned_bytes,
            refine_reciprocity: true,
        },
        &ports,
        max_frequency_hz,
        current_space::Budget {
            max_bytes: budget.max_bytes,
            max_wall,
        },
        start,
    )?;
    Ok(Prepared {
        space,
        supports,
        trace_knots,
        source_mapping,
        electrode_areas_m2,
        max_copper_volume_change,
        field_charge_energy_error,
    })
}

pub struct ReducedPrepared {
    pub space: super::foil_reduced::ReducedSpace,
    pub supports: Vec<Support>,
    pub trace_knots: BTreeMap<String, Vec<Knot>>,
    pub source_mapping: Vec<(String, String, Option<String>, String)>,
    pub electrode_areas_m2: BTreeMap<String, f64>,
    pub max_copper_volume_change: f64,
}

/// Small internal harmonic producer. Retains exact physical pads and every
/// requested cut knot. Field/action residuals do not verify a frequency band.
pub fn prepare_reduced(
    board: &Board,
    request: &board_mesh::Request,
    cuts: &[TraceCut],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    max_wall: Duration,
) -> Result<ReducedPrepared> {
    prepare_reduced_impl(
        board,
        request,
        cuts,
        max_frequency_hz,
        budget,
        max_wall,
        None,
    )
}

/// Experimental training at one explicit AC drive. Every later solve still
/// uses the full residual gate. It does not establish a frequency interval.
pub fn prepare_reduced_enriched(
    board: &Board,
    request: &board_mesh::Request,
    cuts: &[TraceCut],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    max_wall: Duration,
    training: &super::foil_reduced::TrainingPoint,
) -> Result<ReducedPrepared> {
    prepare_reduced_impl(
        board,
        request,
        cuts,
        max_frequency_hz,
        budget,
        max_wall,
        Some(training),
    )
}

fn prepare_reduced_impl(
    board: &Board,
    request: &board_mesh::Request,
    cuts: &[TraceCut],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    max_wall: Duration,
    training: Option<&super::foil_reduced::TrainingPoint>,
) -> Result<ReducedPrepared> {
    let start = Instant::now();
    let Geometry {
        mesh,
        foils,
        potential_nodes,
        ports,
        rows,
        supports,
        mut trace_knots,
        max_copper_volume_change,
    } = prepare_geometry(
        board,
        request,
        cuts,
        max_frequency_hz,
        budget,
        start,
        max_wall,
        false,
    )?;
    let retained: Vec<_> = request
        .electrodes
        .iter()
        .enumerate()
        .map(|(i, _)| i)
        .chain(trace_knots.values().flatten().map(|k| k.potential))
        .collect();
    let epsilon = request
        .stack
        .iter()
        .filter_map(|l| l.epsilon_r)
        .fold(1., f64::max);
    let field = Field::new_conforming_trace(&mesh.mesh, potential_nodes.len(), rows, budget)?;
    let space = super::foil_reduced::assemble(
        &foils,
        potential_nodes,
        &retained,
        &ports,
        field,
        max_frequency_hz,
        epsilon,
        budget,
        start,
        max_wall,
        training,
    )?;
    for knot in trace_knots.values_mut().flatten() {
        knot.potential = space.reduced_potential(knot.potential).ok_or_else(|| {
            error(
                "FOIL_REDUCTION",
                "retained physical knot lost in projection",
            )
        })?;
    }
    Ok(ReducedPrepared {
        space,
        supports,
        trace_knots,
        source_mapping: mesh.source_mapping,
        electrode_areas_m2: mesh.electrode_areas_m2,
        max_copper_volume_change,
    })
}

struct Geometry {
    mesh: board_mesh::BoardMesh,
    foils: Vec<Foil>,
    potential_nodes: Vec<String>,
    ports: Vec<[usize; 2]>,
    rows: Vec<TraceRow>,
    supports: Vec<Support>,
    trace_knots: BTreeMap<String, Vec<Knot>>,
    max_copper_volume_change: f64,
}
fn prepare_geometry(
    board: &Board,
    request: &board_mesh::Request,
    cuts: &[TraceCut],
    max_frequency_hz: f64,
    budget: AllocationBudget,
    start: Instant,
    max_wall: Duration,
    dense: bool,
) -> Result<Geometry> {
    // Keep the planar-only path bounded to the declared 2–8 copper layers.
    // Board mesh validation additionally checks exact names/order and materials.
    if !(3..=15).contains(&request.stack.len())
        || request.stack.len() % 2 == 0
        || request
            .stack
            .iter()
            .enumerate()
            .any(|(i, layer)| layer.copper != (i % 2 == 0))
        || !request.cut_planes.is_empty()
        || request.allow_insulated_copper_cuts
    {
        return Err(error("BOARD_FOIL_SCOPE", "2..8 alternating copper foils, complete copper, finite pads and no barrels or equipotential cuts required"));
    }
    if !max_frequency_hz.is_finite() || max_frequency_hz <= 0. || max_frequency_hz > 1e5 {
        return Err(error(
            "CURRENT_BAND",
            "explicit thin-volume screening band0..fmax<=100kHz required",
        ));
    }
    if cuts.len() > 16
        || cuts
            .iter()
            .map(|cut| &cut.id)
            .collect::<BTreeSet<_>>()
            .len()
            != cuts.len()
    {
        return Err(error(
            "BOARD_FOIL_TRACE",
            "at most 16 uniquely named trace segments required",
        ));
    }
    for cut in cuts {
        if cut.id.is_empty()
            || !request
                .stack
                .iter()
                .any(|l| l.copper && l.name == cut.layer)
            || cut.endpoints_mm[0] == cut.endpoints_mm[1]
            || cut.endpoints_mm.iter().any(|p| {
                (0..2).any(|k| {
                    !p[k].is_finite() || p[k] < request.roi_mm[0][k] || p[k] > request.roi_mm[1][k]
                })
            })
        {
            return Err(error(
                "BOARD_FOIL_TRACE",
                "finite nonzero trace inside the physical ROI and on a known copper layer required",
            ));
        }
    }
    let lines: Vec<_> = cuts.iter().map(|c| c.endpoints_mm).collect();
    let mesh = board_mesh::build_conforming(
        board,
        request,
        budget,
        remaining(start, max_wall)?,
        None,
        &lines,
    )?;
    prepare_geometry_from_mesh(mesh, request, cuts, budget, start, max_wall, dense)
}

fn prepare_geometry_from_mesh(
    mesh: board_mesh::BoardMesh,
    request: &board_mesh::Request,
    cuts: &[TraceCut],
    budget: AllocationBudget,
    start: Instant,
    max_wall: Duration,
    dense: bool,
) -> Result<Geometry> {
    remaining(start, max_wall)?;
    if !mesh.barrel_ids.is_empty() {
        return Err(error(
            "BOARD_FOIL_SCOPE",
            "a plated hole requires the volume current operator",
        ));
    }
    let planar_count = mesh.planar_nodes_mm.len();
    let xy: Vec<_> = mesh.mesh.nodes_m[..planar_count]
        .iter()
        .map(|p| [p[0], p[1]])
        .collect();
    let layers: Vec<_> = request.stack.iter().filter(|l| l.copper).collect();
    let layer_at = |z| {
        layers.iter().position(|l| {
            let [lo, hi] = mesh.layer_spans[&l.name];
            z >= lo && z <= hi
        })
    };
    let mut cells = BTreeMap::<(usize, usize), PrismOwnership>::new();
    for (tet, &sigma) in mesh
        .mesh
        .copper_tetrahedra
        .iter()
        .zip(&mesh.conductivity_s_m)
    {
        remaining(start, max_wall)?;
        let z = tet.nodes.map(|n| mesh.mesh.nodes_m[n][2]);
        let lo = z.into_iter().fold(f64::INFINITY, f64::min);
        let hi = z.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let layer = layer_at(lo)
            .filter(|i| hi <= mesh.layer_spans[&layers[*i].name][1])
            .ok_or_else(|| {
                error(
                    "BOARD_FOIL_SCOPE",
                    "copper cell outside one physical foil span",
                )
            })?;
        if Some(sigma) != layers[layer].conductivity_s_m {
            return Err(error(
                "BOARD_FOIL_SCOPE",
                "non-foil copper material in the physical span",
            ));
        }
        let cell = cells.entry((layer, tet.planar_triangle)).or_default();
        cell.volume_m3 += tet.volume_m3;
        cell.source_ids.extend(tet.source_ids.iter().cloned());
    }
    if dense && cells.len() > current_space::MAX_FOIL_SUPPORTS {
        return Err(error(
            "BOARD_FOIL_SCOPE",
            "captured copper exceeds the 64-prism operator cap",
        ));
    }
    let mut potential_nodes = Vec::new();
    let mut node_dofs = BTreeMap::<(usize, usize), usize>::new();
    let mut electrode_dofs = BTreeMap::new();
    for electrode in &request.electrodes {
        remaining(start, max_wall)?;
        let layer = layers
            .iter()
            .position(|l| l.name == electrode.layer)
            .ok_or_else(|| {
                error(
                    "BOARD_FOIL_PAD",
                    "physical electrode does not lie on a foil",
                )
            })?;
        let dof = potential_nodes.len();
        potential_nodes.push(electrode.name.clone());
        electrode_dofs.insert(&electrode.name, dof);
        for node in mesh.electrode_nodes(electrode)? {
            if node_dofs
                .insert((layer, node % planar_count), dof)
                .is_some_and(|old| old != dof)
            {
                return Err(error(
                    "BOARD_FOIL_PAD",
                    "different finite pads share a physical foil vertex",
                ));
            }
        }
    }
    let mut foils = Vec::new();
    let mut supports = Vec::new();
    let mut trace_knots = BTreeMap::new();
    let mut max_copper_volume_change: f64 = 0.;
    for (layer, material) in layers.iter().enumerate() {
        let span = mesh.layer_spans[&material.name];
        let mut local_nodes = BTreeMap::new();
        let mut edges = BTreeMap::<[usize; 2], usize>::new();
        let mut foil = Foil {
            source_id: format!("board-foil:{}", material.name),
            xy_m: Vec::new(),
            triangles: Vec::new(),
            potential_dofs: Vec::new(),
            z_m: span,
            conductivity_s_per_m: material.conductivity_s_m.unwrap(),
        };
        for (&(_, index), cell) in cells.range((layer, 0)..=(layer, usize::MAX)) {
            remaining(start, max_wall)?;
            let mut triangle = mesh.planar_triangles[index];
            let [a, b, c] = triangle.map(|i| xy[i]);
            let twice = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if twice < 0. {
                triangle.swap(1, 2);
            }
            let volume = twice.abs() / 2. * (span[1] - span[0]);
            let change = (volume - cell.volume_m3).abs() / volume;
            if !change.is_finite() || change > 1e-8 || cell.source_ids.is_empty() {
                return Err(error(
                    "BOARD_FOIL_OWNERSHIP",
                    "incomplete original copper prism or missing provenance",
                ));
            }
            max_copper_volume_change = max_copper_volume_change.max(change);
            supports.push(Support {
                layer: material.name.clone(),
                planar_triangle: index,
                source_region_ids: cell.source_ids.iter().cloned().collect(),
                copper_volume_m3: volume,
            });
            for k in 0..3 {
                let [mut a, mut b] = [triangle[k], triangle[(k + 1) % 3]];
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                *edges.entry([a, b]).or_default() += 1;
            }
            foil.triangles.push(triangle.map(|i| {
                *local_nodes.entry(i).or_insert_with(|| {
                    let local = foil.xy_m.len();
                    foil.xy_m.push(xy[i]);
                    let dof = *node_dofs.entry((layer, i)).or_insert_with(|| {
                        let dof = potential_nodes.len();
                        potential_nodes.push(format!("foil-node:{}:{i}", material.name));
                        dof
                    });
                    foil.potential_dofs.push(dof);
                    local
                })
            }));
        }
        let mut boundary_degree = BTreeMap::<usize, usize>::new();
        for (&edge, &count) in &edges {
            if count == 1 {
                for vertex in edge {
                    *boundary_degree.entry(vertex).or_default() += 1;
                }
            }
            if count > 2 {
                return Err(error("BOARD_FOIL_TOPOLOGY", "nonmanifold copper edge"));
            }
        }
        if boundary_degree.values().any(|&degree| degree != 2) {
            return Err(error(
                "BOARD_FOIL_TOPOLOGY",
                "point-only metal contact or nonmanifold copper boundary",
            ));
        }
        for cut in cuts.iter().filter(|c| c.layer == material.name) {
            trace_knots.insert(
                cut.id.clone(),
                extract_knots(cut, &mesh, &edges, &node_dofs, layer)?,
            );
        }
        if !foil.triangles.is_empty() {
            foils.push(foil);
        }
    }
    if dense {
        current_space::preflight_counts(
            supports.len(),
            potential_nodes.len(),
            mesh.mesh.retained_bytes()?,
            current_space::Budget {
                max_bytes: budget.max_bytes,
                max_wall,
            },
        )?;
    }
    let ports = request
        .ports
        .iter()
        .map(|[a, b]| {
            let a = electrode_dofs
                .get(a)
                .copied()
                .ok_or_else(|| error("BOARD_FOIL_PAD", "unknown positive electrode"))?;
            let b = electrode_dofs
                .get(b)
                .copied()
                .ok_or_else(|| error("BOARD_FOIL_PAD", "unknown negative electrode"))?;
            Ok([a, b])
        })
        .collect::<Result<Vec<_>>>()?;
    let exposed = mesh
        .mesh
        .copper_interfaces
        .iter()
        .flat_map(|face| face.nodes)
        .collect::<BTreeSet<_>>();
    let rows = exposed
        .into_iter()
        .map(|node| {
            let z = mesh.mesh.nodes_m[node][2];
            let dof = layer_at(z)
                .and_then(|l| node_dofs.get(&(l, node % planar_count)).copied())
                .ok_or_else(|| {
                    error(
                        "BOARD_FOIL_TRACE",
                        "native metal surface lacks its exact foil coordinate",
                    )
                })?;
            Ok(TraceRow {
                node,
                weights: vec![(dof, 1.)],
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Geometry {
        mesh,
        foils,
        potential_nodes,
        ports,
        rows,
        supports,
        trace_knots,
        max_copper_volume_change,
    })
}

fn extract_knots(
    cut: &TraceCut,
    mesh: &board_mesh::BoardMesh,
    edges: &BTreeMap<[usize; 2], usize>,
    node_dofs: &BTreeMap<(usize, usize), usize>,
    layer: usize,
) -> Result<Vec<Knot>> {
    let [a, b] = cut.endpoints_mm;
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    let unit = [d[0] / length, d[1] / length];
    let tol = mesh
        .geometry
        .boolean_error_bound_mm
        .max(64. * f64::EPSILON * length);
    let distance = |node: usize| {
        let p = mesh.planar_nodes_mm[node];
        let rel = [p[0] - a[0], p[1] - a[1]];
        let along = rel[0] * unit[0] + rel[1] * unit[1];
        (along >= -tol
            && along <= length + tol
            && (rel[0] * unit[1] - rel[1] * unit[0]).abs() <= tol)
            .then_some(along)
    };
    let mut segments: Vec<_> = edges
        .iter()
        .filter_map(|(&[u, v], &owners)| {
            if owners != 2 {
                return None;
            }
            let (s, t) = (distance(u)?, distance(v)?);
            Some(if s < t { (s, t, u, v) } else { (t, s, v, u) })
        })
        .collect();
    segments.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some(&(first, _, start, _)) = segments.first() else {
        return Err(error(
            "BOARD_FOIL_TRACE",
            "trace has no conforming two-sided copper edge",
        ));
    };
    if first.abs() > tol {
        return Err(error(
            "BOARD_FOIL_TRACE",
            "trace starts outside the copper support",
        ));
    }
    let mut knots = vec![start];
    let mut covered = first;
    for (_, next, u, v) in segments {
        if knots.last().copied() != Some(u) {
            return Err(error(
                "BOARD_FOIL_TRACE",
                "trace crosses an opening or unmatched copper edge",
            ));
        }
        covered = next;
        knots.push(v);
    }
    if (covered - length).abs() > tol {
        return Err(error(
            "BOARD_FOIL_TRACE",
            "trace ends outside the copper support",
        ));
    }
    Ok(knots
        .into_iter()
        .map(|node| Knot {
            position_mm: mesh.planar_nodes_mm[node],
            potential: node_dofs[&(layer, node)],
        })
        .collect())
}

// Bound the exact-grid output and the number of checked drives in each pass.
const MAX_CHECKED_COLUMNS: usize = 256;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckedRequest {
    pub frequencies_hz: Vec<f64>,
    pub reference_ohm: Option<Vec<f64>>,
    pub budget: crate::electrical::Budget,
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub operator_policy: CheckedOperator,
    pub profile_upper_hz: Option<f64>,
    #[serde(default)]
    pub partition_policy: mixed_conductor::PartitionPolicy,
    #[serde(default)]
    pub attachments: Vec<super::checked_packages::Attachment>,
}

/// The checked automatic endpoint deliberately returns only the exact-grid
/// response and evidence. Its shared reduced model is never a public resource.
pub fn solve_checked(
    board: &Board,
    geometry: &board_mesh::Request,
    request: CheckedRequest,
    started: Instant,
) -> Result<serde_json::Value> {
    use serde_json::json;
    let port_count = geometry.ports.len();
    let axis = request.frequencies_hz;
    let operator = request.operator_policy;
    operator.validate_partition(request.partition_policy)?;
    if !(1..=16).contains(&port_count)
        || axis.is_empty()
        || axis.len().saturating_mul(port_count) > MAX_CHECKED_COLUMNS
        || axis
            .iter()
            .enumerate()
            .any(|(i, &f)| !operator.admits(f) || i > 0 && axis[i - 1] >= f)
    {
        return Err(error("BOARD_FOIL_GRID",
            &format!("1..16 physical ports and strictly increasing {} axis within 256 checked columns required", operator.axis_scope())));
    }
    let refs = request
        .reference_ohm
        .unwrap_or_else(|| vec![50.; port_count]);
    if refs.len() != port_count
        || refs
            .iter()
            .any(|r| !r.is_finite() || *r <= 0. || !(1. / r).is_finite())
    {
        return Err(error(
            "BOARD_FOIL_GRID",
            "positive finite matched reference per physical port required",
        ));
    }
    let iterations = request.max_iterations.unwrap_or(16);
    if !(1..=16).contains(&iterations) {
        return Err(error(
            "BOARD_FOIL_GRID",
            "total enrichment budget must be in 1..=16",
        ));
    }
    let seconds = request
        .budget
        .max_seconds
        .filter(|s| s.is_finite() && *s > 0. && *s <= 3600.)
        .ok_or_else(|| {
            error(
                "BOARD_FOIL_BUDGET",
                "finite positive maxSeconds <= 3600 required",
            )
        })?;
    let memory = request
        .budget
        .max_memory_mi_b
        .filter(|m| m.is_finite() && *m >= 1. && *m <= 65536.)
        .ok_or_else(|| {
            error(
                "BOARD_FOIL_BUDGET",
                "finite maxMemoryMiB in 1..=65536 required",
            )
        })?;
    let seconds_cap =
        super::board_prepare::verification_limit("P7_VERIFY_SECONDS", 300, 1, 3600)? as f64;
    let memory_cap =
        super::board_prepare::verification_limit("P7_VERIFY_MEMORY_MIB", 2048, 256, 65536)?;
    let admitted_seconds = seconds.min(seconds_cap);
    let admitted_memory = (memory.floor() as usize).min(memory_cap);
    let packages = super::checked_packages::validate(request.attachments, geometry, &axis)?;
    let reserve = axis.len() * port_count * (port_count * 256 + 512)
        + 256 * 1024
        + super::checked_packages::output_reserve(&packages, axis.len() * port_count);
    if admitted_memory * 1024 * 1024 <= super::parallel::stack_reservation() + reserve {
        return Err(error(
            "MEMORY_LIMIT",
            "checked foil output and native stacks exhaust budget",
        ));
    }
    let mut budget = super::board_prepare::allocation_budget(admitted_memory);
    budget.max_bytes -= reserve;
    let wall = Duration::from_secs_f64(admitted_seconds);
    remaining(started, wall)?;
    let maximum = *axis.last().unwrap();
    let mixed_grid = mixed_conductor::CheckedGrid {
        frequencies_hz: &axis,
        reference_ohm: &refs,
        max_iterations: iterations,
    };
    if !geometry.cut_planes.is_empty() || geometry.allow_insulated_copper_cuts {
        return Err(error(
            "BOARD_FOIL_SCOPE",
            "complete copper and original finite pads required",
        ));
    }
    let mut skin = operator.skin_profile(
        maximum,
        request.profile_upper_hz,
        geometry.max_vertical_step_m,
    )?;
    if request.partition_policy==mixed_conductor::PartitionPolicy::AutomaticHybridV1 {
        skin=skin.map(|profile|profile.with_thickness_screen(geometry.stack.iter().filter(|layer|layer.copper).filter_map(|layer|layer.conductivity_s_m))).transpose()?;
    }
    if skin.is_some() {
        // Apply the existing physical pad/closed copper and electrical-size
        // guard before paying for a larger profiled mesh.
        mixed_conductor::checked_reserve_policy(geometry, mixed_grid, skin, operator, request.partition_policy)?;
    }
    let mesh = board_mesh::build_profiled(
        board,
        geometry,
        budget,
        remaining(started, wall)?,
        None,
        &[],
        skin,
    )?;
    let planar_stack = (3..=15).contains(&geometry.stack.len())
        && geometry.stack.len() % 2 == 1
        && geometry
            .stack
            .iter()
            .enumerate()
            .all(|(i, layer)| layer.copper == (i % 2 == 0));
    let mixed = |mesh| {
        let mut checked = mixed_conductor::solve_checked_mesh_policy(
            mesh,
            geometry,
            mixed_grid,
            skin,
            &packages,
            request.partition_policy,
            operator,
            budget,
            started,
            wall,
        )?;
        checked["provenance"]["maxSeconds"] = json!(admitted_seconds);
        checked["provenance"]["maxMemoryMiB"] = json!(admitted_memory);
        Ok(checked)
    };
    if skin.is_some()
        || request.partition_policy != mixed_conductor::PartitionPolicy::Legacy
        || !packages.is_empty()
        || !planar_stack
        || !mesh.barrel_ids.is_empty()
        || super::mixed_conductor::has_projected_pad_collision(&mesh, &geometry.electrodes)?
    {
        return mixed(mesh);
    }
    let prepared = prepare_geometry_from_mesh(mesh, geometry, &[], budget, started, wall, false)?;
    if current_space::verify_local_thin_scope(&prepared.foils, maximum).is_err() {
        let Geometry { mesh, .. } = prepared;
        return mixed(mesh);
    }
    let Geometry {
        mesh,
        foils,
        potential_nodes,
        ports,
        rows,
        supports,
        max_copper_volume_change,
        ..
    } = prepared;
    // Source audit storage scales with captured provenance. Budget it from
    // the actual names and row count before constructing the JSON response.
    let provenance_bytes = mesh
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
            geometry.electrodes.iter().try_fold(n, |sum, e| {
                sum.checked_add(256)?
                    .checked_add(e.name.len().checked_mul(24)?)
            })
        })
        .ok_or_else(|| error("MEMORY_LIMIT", "checked foil provenance plan overflow"))?;
    budget.max_bytes = budget
        .max_bytes
        .checked_sub(provenance_bytes)
        .ok_or_else(|| error("MEMORY_LIMIT", "checked foil provenance exceeds budget"))?;
    budget.check(mesh.mesh.retained_bytes()?)?;
    let retained: Vec<_> = (0..geometry.electrodes.len()).collect();
    let epsilon = geometry
        .stack
        .iter()
        .filter_map(|l| l.epsilon_r)
        .fold(1., f64::max);
    let field = Field::new_conforming_trace(&mesh.mesh, potential_nodes.len(), rows, budget)?;
    let (space, mut checked) = super::foil_reduced::solve_matched_grid(
        &foils,
        potential_nodes,
        &retained,
        &ports,
        field,
        epsilon,
        budget,
        started,
        wall,
        super::foil_reduced::MatchedGrid {
            frequencies: &axis,
            references: &refs,
            max_iterations: iterations,
        },
    )?;
    remaining(started, wall)?;
    checked["status"] = json!("complete");
    checked["representation"] = json!("S");
    checked["coverage"] = json!("unverified");
    checked["frequenciesHz"] = json!(axis);
    checked["referenceOhm"] = json!(refs);
    checked["ports"] = json!(geometry.ports);
    checked["provenance"] = json!({
        "method":"board-derived conforming foil, checked matched exact grid",
        "modelKind":"foil","barrelIds":[],
        "sourceMapping":mesh.source_mapping,"electrodeAreasM2":mesh.electrode_areas_m2,
        "maxCopperVolumeChange":max_copper_volume_change,"prisms":supports.len(),
        "retainedPotentials":space.diagnostic_model().potential_nodes.len(),
        "currentModes":space.diagnostic_model().current_dofs.len(),
        "plannedBytes":space.planned_bytes+reserve+provenance_bytes,
        "maxSeconds":admitted_seconds,"maxMemoryMiB":admitted_memory,
        "maxMagneticReciprocity":space.max_magnetic_reciprocity,
        "maxMagneticRefinement":space.max_magnetic_refinement,
        "maxElectricReciprocity":space.max_electric_reciprocity,
        "spatialCoverage":"unverified","bandCoverage":"unverified",
        "driveConvention":"1 A RMS matched Norton basis columns, checked only at listed frequencies",
    });
    Ok(checked)
}
