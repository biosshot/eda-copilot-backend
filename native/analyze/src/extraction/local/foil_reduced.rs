//! Bounded harmonic Galerkin primitive for the board-derived foil path.
//! No full-dimensional dense conductor, capacitance or magnetic matrix.
use super::{
    accurate::Sum,
    current_space::Foil,
    error,
    polyhedron::{self, PointPotential},
    prism::{GaussRule, Prism},
    sheet::SheetCell,
    sparse_potential,
    volume_field::{ElectricApply, Field},
    AllocationBudget, Result,
};
use crate::electrical::{
    coupled::{
        physical::{Node, PortTerminals, Terminal},
        Model, System,
    },
    Budget, Counters,
};
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    prelude::Solve,
    sparse::{linalg::cholesky, SparseColMat, Triplet},
    Conj, Mat, Side,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[path = "partitioned_conduction.rs"]
mod partitioned_conduction;

#[derive(Clone)]
struct Cell {
    /// Sparse incidence for one Cartesian current coordinate.
    terms: Vec<(usize, f64)>,
    resistance: f64,
}

impl Cell {
    fn new(mut terms: Vec<(usize, f64)>, resistance: f64) -> Self {
        // Set the anchor by partition of unity, in every physical space.
        if let Some((anchor, remaining)) = terms.split_first_mut() {
            anchor.1 = -remaining.iter().map(|&(_, a)| a).sum::<f64>();
        }
        Self { terms, resistance }
    }
}

type MagneticBatch<'a> =
    dyn Fn(&[Vec<f64>], Option<&[f64]>, Instant, Duration) -> Result<Magnetic> + 'a;
struct Basis {
    voltage: Vec<Vec<f64>>,
    current: Vec<Vec<f64>>,
    retained: Vec<usize>,
    components: Vec<Vec<usize>>,
    dc_residual: f64,
}
const RESIDUAL_LIMIT: f64 = 1e-8;
const MAX_ENRICHMENT_ITERATIONS: usize = 16;

#[derive(Clone, Debug)]
pub struct TrainingPoint {
    pub frequency_hz: f64,
    pub drive: Vec<f64>,
    /// Resource limit for the adaptive basis construction. None uses 16.
    pub max_iterations: Option<usize>,
}
impl TrainingPoint {
    fn iteration_limit(&self) -> Result<usize> {
        let count = self.max_iterations.unwrap_or(MAX_ENRICHMENT_ITERATIONS);
        if !(1..=MAX_ENRICHMENT_ITERATIONS).contains(&count) {
            return Err(error(
                "FOIL_ENRICHMENT",
                "iteration budget must be in 1..=16",
            ));
        }
        Ok(count)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnrichmentStep {
    pub iteration: usize,
    pub potential_modes: usize,
    pub current_modes: usize,
    pub residual: Residual,
}

#[derive(Debug)]
pub struct Enrichment {
    pub training: TrainingPoint,
    pub iterations: usize,
    pub history: Vec<EnrichmentStep>,
    pub added_potential_modes: usize,
    pub added_current_modes: usize,
    pub initial_residual: Residual,
    pub final_residual: Residual,
}
#[derive(Clone, Copy, Debug)]
pub struct Residual {
    pub kcl_relative: f64,
    pub kvl_relative: f64,
}
pub struct CheckedPoint {
    pub voltage_real: Vec<f64>,
    pub voltage_imaginary: Vec<f64>,
    pub residual: Residual,
    /// A passed algebraic point does not verify mesh, field domain or band.
    pub coverage: &'static str,
}
pub struct ReducedSpace {
    model: Model,
    cells: Vec<Cell>,
    basis: Basis,
    charges: Vec<Vec<f64>>,
    magnetic_actions: Vec<Vec<f64>>,
    magnetic_basis_certificate: Option<serde_json::Value>,
    ports: Vec<[usize; 2]>,
    full_potentials: usize,
    max_frequency_hz: f64,
    pub planned_bytes: usize,
    pub max_magnetic_reciprocity: f64,
    pub max_magnetic_refinement: f64,
    pub max_electric_reciprocity: f64,
    pub dc_harmonic_residual: f64,
    pub enrichment: Option<Enrichment>,
    native_allocation: bool,
}

fn sum(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .fold(Sum::default(), |s, (&x, &y)| s.add(Sum::from(x).mul(y)))
        .value()
}
fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
fn check_time(start: Instant, wall: Duration) -> Result<()> {
    if start.elapsed() >= wall {
        Err(error("DEADLINE", "reduced foil assembly"))
    } else {
        Ok(())
    }
}
fn sizes(n: usize, m: usize, r: usize, field_bytes: usize) -> Result<usize> {
    // Conservative simultaneous bounds for coefficient buffers, sparse input,
    // Lift/action slabs and the downstream reduced descriptor have no N-squared term.
    n.checked_mul(1024)
        .and_then(|x| x.checked_add(m.checked_mul(16384)?))
        .and_then(|x| x.checked_add(n.checked_add(m)?.checked_mul(r)?.checked_mul(64)?))
        .and_then(|x| x.checked_add(r.checked_mul(2)?.checked_pow(2)?.checked_mul(2048)?))
        .and_then(|x| x.checked_add(field_bytes))
        .ok_or_else(|| error("MEMORY_LIMIT", "reduced foil plan overflow"))
}
/// Native mixed-only buffers, separate from the legacy prism preparation.
/// Sparse symbolic fill, factor residency and queried solve scratch are added
/// by sparse_potential/partitioned_conduction before their own allocations.
fn native_work_bytes(n: usize, m: usize, r: usize) -> Result<usize> {
    native_work_modes(n, m, r, r)
}
fn native_global_sparse_bytes(m: usize) -> Result<usize> {
    m.checked_mul(16 * (std::mem::size_of::<Triplet<usize,usize,f64>>()
        +4*std::mem::size_of::<usize>()+std::mem::size_of::<f64>()))
        .ok_or_else(||error("MEMORY_LIMIT","native global sparse buffer overflow"))
}
fn native_work_modes(n: usize, m: usize, potential_modes: usize, current_modes: usize) -> Result<usize> {
    // Per four-term current column: Cell+coefficient storage; up to16 triplets
    // plus pinned faer0.24.4 argsort/row-index/CSC construction (four index
    // slabs+one value slab conservatively bound csc/mod.rs1163,2373). The32
    // real current-vector slots cover simultaneous old/new residuals, lift/
    // gradient/metric candidates, and transient construction Vec capacities.
    let cell = std::mem::size_of::<Cell>() + 4 * std::mem::size_of::<(usize, f64)>();
    let sparse = 16
        * (std::mem::size_of::<Triplet<usize, usize, f64>>()
            + 4 * std::mem::size_of::<usize>()
            + std::mem::size_of::<f64>());
    let current = cell + sparse + 32 * std::mem::size_of::<f64>();
    // Eight real full vectors per possible mode cover resident basis/action,
    // temporary resistive Gram images and Sum magnetic output+conversion.
    // n*1024 covers nodal maps, retained/component coordinates, RHS/lifts,
    // physical injections, complex residuals and compensated accumulators.
    n.checked_mul(1024)
        .and_then(|v| v.checked_add(m.checked_mul(current)?))
        .and_then(|v| v.checked_add(n.checked_mul(potential_modes)?.checked_mul(64)?))
        .and_then(|v| v.checked_add(m.checked_mul(current_modes)?.checked_mul(64)?))
        .ok_or_else(|| error("MEMORY_LIMIT", "native mixed buffer plan overflow"))
}
fn native_sizes(n: usize, m: usize, r: usize, base: usize) -> Result<usize> {
    native_mode_sizes(n, m, r, r, base)
}
fn native_mode_sizes(n: usize, m: usize, potential_modes: usize, current_modes: usize, base: usize) -> Result<usize> {
    native_work_modes(n, m, potential_modes, current_modes)?
        .checked_add(base)
        .and_then(|v| v.checked_add(potential_modes.checked_add(current_modes)?.checked_pow(2)?.checked_mul(2048)?))
        .ok_or_else(|| error("MEMORY_LIMIT", "native mixed descriptor plan overflow"))
}
fn native_partitioned_sizes(n: usize, m: usize, potential_modes: usize, current_modes: usize, base: usize) -> Result<usize> {
    // correction(partition=Some) returns before constructing global triplets
    // or CSC. Its backend separately admits owner assembly, Schur assembly,
    // metadata, factor residency and solve scratch. Keep every Cell, vector,
    // mode and descriptor allowance; remove only absent global assembly.
    native_mode_sizes(n,m,potential_modes,current_modes,base)?
        .checked_sub(native_global_sparse_bytes(m)?)
        .ok_or_else(||error("MEMORY_LIMIT","partitioned buffer accounting underflow"))
}
fn gradient(cells: &[Cell], v: &[f64]) -> Vec<f64> {
    cells
        .iter()
        .map(|c| {
            let Some((&(anchor, _), remaining)) = c.terms.split_first() else {
                return 0.;
            };
            remaining.iter().map(|&(i, a)| a * (v[i] - v[anchor])).sum()
        })
        .collect()
}
fn divergence(cells: &[Cell], j: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![Sum::default(); n];
    for (cell, &current) in cells.iter().zip(j) {
        for &(node, a) in &cell.terms {
            out[node] = out[node].add(Sum::from(a).mul(current));
        }
    }
    out.into_iter().map(Sum::value).collect()
}
fn conductive(cells: &[Cell], v: &[f64]) -> Vec<f64> {
    gradient(cells, v)
        .into_iter()
        .zip(cells)
        .map(|(a, c)| a / c.resistance)
        .collect()
}

struct DcCorrection {
    free_map: Vec<Option<usize>>,
    free: usize,
    owned_bytes: usize,
    factor: Option<(cholesky::SymbolicCholesky<usize>, Vec<f64>, usize, usize)>,
    partitioned: Option<partitioned_conduction::Backend>,
}
impl DcCorrection {
    fn solve(&self, cells: &[Cell], rhs: &[f64]) -> Result<Vec<f64>> {
        if let Some(backend) = &self.partitioned {
            return backend.solve(cells, &self.free_map, rhs);
        }
        let mut solution = vec![0.; rhs.len()];
        if let Some((symbolic, values, _, _)) = &self.factor {
            let mut free_rhs = Mat::zeros(self.free, 1);
            for (i, k) in self.free_map.iter().enumerate() {
                if let Some(k) = *k {
                    free_rhs[(k, 0)] = rhs[i];
                }
            }
            let par = super::parallel::current();
            let mut scratch = MemBuffer::try_new(symbolic.solve_in_place_scratch::<f64>(1, par))
                .map_err(|_| error("MEMORY_LIMIT", "foil sparse solve scratch"))?;
            cholesky::LltRef::new(symbolic, values).solve_in_place_with_conj(
                Conj::No,
                free_rhs.as_mut(),
                par,
                MemStack::new(&mut scratch),
            );
            for (i, k) in self.free_map.iter().enumerate() {
                if let Some(k) = *k {
                    solution[i] = free_rhs[(k, 0)];
                }
            }
        }
        Ok(solution)
    }
}

fn infer_islands(cells: &[Cell], n: usize) -> Result<Vec<Vec<usize>>> {
    let mut parents: Vec<_> = (0..n).collect();
    let mut used = vec![false; n];
    for c in cells {
        let a = root(&parents, c.terms[0].0);
        for &(node, _) in &c.terms {
            used[node] = true;
            let b = root(&parents, node);
            parents[b] = a;
        }
    }
    if used.contains(&false) {
        return Err(error(
            "FOIL_REDUCTION",
            "unrepresented physical potential coordinate",
        ));
    }
    let mut islands = BTreeMap::<usize, Vec<usize>>::new();
    for i in 0..n {
        islands.entry(root(&parents, i)).or_default().push(i);
    }
    Ok(islands.into_values().collect())
}

fn harmonic(
    cells: &[Cell],
    n: usize,
    contacts: &BTreeSet<usize>,
    known_islands: Option<&[Vec<usize>]>,
    budget: AllocationBudget,
    base: usize,
    start: Instant,
    wall: Duration,
    extra_modes: usize,
    native_allocation: bool,
) -> Result<(Basis, usize, DcCorrection)> {
    let inferred;
    let islands = if let Some(physical) = known_islands {
        physical
    } else {
        inferred = infer_islands(cells, n)?;
        &inferred
    };
    let mut retained = contacts.clone();
    // A floating island keeps a finite C coordinate. This is not grounding.
    for island in islands {
        if !island.iter().any(|i| retained.contains(i)) {
            retained.insert(island[0]);
        }
    }
    let r = retained.len() + extra_modes;
    if r > 1024 || r.checked_mul(2).is_none_or(|x| x > 2048) {
        return Err(error(
            "FOIL_REDUCTION",
            "retained trace exceeds P6 coordinate limits",
        ));
    }
    let plan = if native_allocation {
        native_mode_sizes(n, cells.len(), r, retained.len().checked_sub(islands.len())
            .and_then(|v|v.checked_add(extra_modes)).ok_or_else(||error("MEMORY_LIMIT","harmonic current mode bound overflow"))?, base)?
    } else {
        sizes(n, cells.len(), r, base)?
    };
    if native_allocation {
        eprintln!("P7_MIXED_MEMORY_STAGE stage=harmonic-admission potentials={n} currents={} retained={} reservedModes={extra_modes} modeCap={r} baseBytes={base} plannedBytes={plan} budgetBytes={}", cells.len(), retained.len(), budget.max_bytes);
    }
    budget.check(plan).map_err(|failure|format!("{failure}; stage=harmonic-admission potentials={n} currents={} modeCap={r} plannedBytes={plan} budgetBytes={}",cells.len(),budget.max_bytes))?;
    let (peak, dc) = correction(cells, n, &retained, plan, budget, start, wall, None)?;
    let retained: Vec<_> = retained.into_iter().collect();
    let mut voltage = vec![vec![0.; n]; retained.len()];
    let mut current = Vec::new();
    let mut components = Vec::new();
    let mut dc_residual: f64 = 0.;
    for island in islands {
        let modes: Vec<_> = retained
            .iter()
            .enumerate()
            .filter_map(|(j, i)| island.contains(i).then_some(j))
            .collect();
        let (&anchor, others) = modes.split_last().unwrap();
        for &j in others {
            check_time(start, wall)?;
            let v = &mut voltage[j];
            v[retained[j]] = 1.;
            let (q, residual) = complete_harmonic_current(cells, v, &dc, start, wall)?;
            dc_residual = dc_residual.max(residual);
            current.push(q);
        }
        for &i in island {
            voltage[anchor][i] = 1. - others.iter().map(|&j| voltage[j][i]).sum::<f64>();
        }
        components.push(modes);
    }
    if current.is_empty() && known_islands.is_none() {
        return Err(error(
            "FOIL_REDUCTION",
            "physical contacts excite no conductive harmonic mode",
        ));
    }
    Ok((
        Basis {
            voltage,
            current,
            retained,
            components,
            dc_residual,
        },
        peak,
        dc,
    ))
}

/// Remove any free-coordinate conductive reaction while keeping the finite
/// electrode values. The input may already contain a mixed sheet lift.
fn complete_harmonic_current(
    cells: &[Cell],
    voltage: &mut [f64],
    dc: &DcCorrection,
    start: Instant,
    wall: Duration,
) -> Result<(Vec<f64>, f64)> {
    // Reuse the admitted factor, never perturb retained physical contacts.
    // Accumulate corrections before rounding back to the actual stored F64
    // voltage. Acceptance is always measured on that stored voltage, not an
    // unrepresented compensated state.
    let mut accumulated: Vec<_> = voltage.iter().copied().map(Sum::from).collect();
    let mut history = Vec::with_capacity(9);
    for iteration in 0..=8 {
        check_time(start, wall)?;
        let mut q = conductive(cells, voltage);
        let reaction = divergence(cells, &q, voltage.len());
        let scale = reaction.iter().map(|v| v.abs()).fold(0., f64::max);
        let residual = reaction.iter().zip(&dc.free_map)
            .filter(|(_, free)| free.is_some()).map(|(v, _)| v.abs())
            .fold(0., f64::max) / scale.max(f64::MIN_POSITIVE);
        history.push(residual);
        if residual.is_finite() && residual <= RESIDUAL_LIMIT {
            let norm = q.iter().zip(cells).map(|(&j,c)| c.resistance*j*j).sum::<f64>().sqrt();
            if !norm.is_finite() || norm <= 0. {
                return Err(error("FOIL_REDUCTION", "independent finite harmonic current required"));
            }
            for v in &mut q { *v /= norm; }
            if iteration > 1 { eprintln!("P7_HARMONIC_REFINEMENT iterations={iteration} residual={residual:e}"); }
            return Ok((q,residual));
        }
        if iteration == 8 || dc.free == 0 || !residual.is_finite() { break; }
        let rhs: Vec<_> = reaction.into_iter().map(|v| -v).collect();
        let delta=dc.solve(cells,&rhs)?;
        for (i,correction) in delta.into_iter().enumerate() {
            if dc.free_map[i].is_some() {
                accumulated[i]=accumulated[i].add(Sum::from(correction));
                voltage[i]=accumulated[i].value();
                if !voltage[i].is_finite() { return Err(error("FOIL_REDUCTION","nonfinite harmonic residual correction")); }
            }
        }
    }
    Err(error("FOIL_REDUCTION", &format!("harmonic free residual failed after bounded refinement; history={history:?}")))
}

fn correction(
    cells: &[Cell],
    n: usize,
    retained: &BTreeSet<usize>,
    plan: usize,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
    partition: Option<&super::spatial_partition::Partition>,
) -> Result<(usize, DcCorrection)> {
    let mut next = 0;
    let free_map: Vec<_> = (0..n)
        .map(|i| {
            if retained.contains(&i) {
                None
            } else {
                let j = next;
                next += 1;
                Some(j)
            }
        })
        .collect();
    if let Some(partition) = partition {
        let (peak, backend) = partitioned_conduction::Backend::factor(
            cells,
            &free_map,
            partition.owner_count(),
            |i| partition.current_owner(i),
            partition.trace_dofs(),
            plan,
            budget,
            start,
            wall,
        )?;
        return Ok((
            peak,
            DcCorrection {
                free_map,
                free: next,
                owned_bytes: backend.owned_bytes,
                factor: None,
                partitioned: Some(backend),
            },
        ));
    }
    let factor = if next > 0 {
        let mut terms = Vec::with_capacity(cells.len() * 16);
        for c in cells {
            check_time(start, wall)?;
            for &(a, av) in &c.terms {
                for &(b, bv) in &c.terms {
                    if let (Some(i), Some(j)) = (free_map[a], free_map[b]) {
                        if i <= j {
                            terms.push(Triplet::new(i, j, av * bv / c.resistance));
                        }
                    }
                }
            }
        }
        let matrix = SparseColMat::<usize, f64>::try_new_from_triplets(next, next, &terms)
            .map_err(|e| error("FOIL_REDUCTION", &e.to_string()))?;
        drop(terms);
        Some(sparse_potential::factor(&matrix, plan, budget, &[])?)
    } else {
        None
    };
    let peak = factor.as_ref().map_or(plan, |x| x.2);
    Ok((
        peak,
        DcCorrection {
            free_map,
            free: next,
            owned_bytes: factor.as_ref().map_or(0, |x| x.3.saturating_sub(plan)),
            factor,
            partitioned: None,
        },
    ))
}

struct Magnetic {
    actions: Vec<Vec<f64>>,
    error_bounds: Option<Vec<Vec<f64>>>,
    certificate: Option<serde_json::Value>,
    reciprocity: f64,
    refinement: f64,
}
fn magnetic(
    prisms: &[Prism],
    density: &[f64],
    q: &[Vec<f64>],
    start: Instant,
    wall: Duration,
) -> Result<Magnetic> {
    let sources = prisms
        .iter()
        .map(PointPotential::new)
        .collect::<Result<Vec<_>>>()?;
    let rules = [(12, 8), (24, 16), (48, 24), (64, 32)]
        .map(|(a, b)| Ok((GaussRule::new(a)?, GaussRule::new(b)?)))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let integral = |i, j, order: usize| {
        check_time(start, wall)?;
        polyhedron::volume_integral_anisotropic(
            &prisms[i],
            &sources[j],
            &rules[order].0,
            &rules[order].1,
        )
    };
    let diagonal = (0..prisms.len())
        .map(|i| integral(i, i, 1))
        .collect::<Result<Vec<_>>>()?;
    let scale = diagonal
        .iter()
        .zip(density)
        .map(|(&l, &d)| l * d * d)
        .fold(0., f64::max);
    let mut actions = vec![vec![Sum::default(); prisms.len() * 2]; q.len()];
    let (mut reciprocity, mut refinement) = (0_f64, 0_f64);
    for i in 0..prisms.len() {
        for j in 0..=i {
            let low = integral(i, j, 0)?;
            let low_reverse = if i == j { low } else { integral(j, i, 0)? };
            let mut a = if i == j {
                diagonal[i]
            } else {
                integral(i, j, 1)?
            };
            let mut b = if i == j { a } else { integral(j, i, 1)? };
            let weight = density[i] * density[j] / scale;
            for order in [2, 3] {
                if (a - b).abs() * weight <= 1e-9 {
                    break;
                }
                a = integral(i, j, order)?;
                b = integral(j, i, order)?;
            }
            let pair_reciprocity = (a - b).abs() * weight;
            let pair_refinement = (a - low).abs().max((b - low_reverse).abs()) * weight;
            reciprocity = reciprocity.max(pair_reciprocity);
            refinement = refinement.max(pair_refinement);
            if !pair_reciprocity.is_finite()
                || pair_reciprocity > 1e-9
                || !pair_refinement.is_finite()
                || pair_refinement > 0.002
            {
                return Err(error("CURRENT_QUADRATURE",&format!("streamed pair {i}/{j}: refinement={pair_refinement:e}, reciprocity={pair_reciprocity:e}")));
            }
            // Same canonical upper directed integral as the bounded dense foil.
            let l = 1e-7 * b * density[i] * density[j];
            for (col, source) in actions.iter_mut().zip(q) {
                for axis in 0..2 {
                    let (u, v) = (2 * i + axis, 2 * j + axis);
                    col[u] = col[u].add(Sum::from(l).mul(source[v]));
                    if i != j {
                        col[v] = col[v].add(Sum::from(l).mul(source[u]));
                    }
                }
            }
        }
    }
    let actions = actions
        .into_iter()
        .map(|c| c.into_iter().map(Sum::value).collect())
        .collect();
    Ok(Magnetic {
        actions,
        error_bounds: None,
        certificate: None,
        reciprocity,
        refinement,
    })
}
fn congruence(
    basis: &[Vec<f64>],
    action: &[Vec<f64>],
    gate: f64,
    label: &str,
) -> Result<(Vec<(usize, usize, f64)>, f64)> {
    let scale = (0..basis.len())
        .map(|i| sum(&basis[i], &action[i]).abs())
        .fold(0., f64::max);
    let mut entries = Vec::new();
    let mut error_max = 0_f64;
    for i in 0..basis.len() {
        for j in i..basis.len() {
            let a = sum(&basis[i], &action[j]);
            let b = sum(&basis[j], &action[i]);
            let err = (a - b).abs() / scale.max(f64::MIN_POSITIVE);
            if !a.is_finite() || !b.is_finite() || err > gate {
                return Err(error(
                    "FOIL_REDUCTION",
                    &format!("{label} projected reciprocity {err:e}"),
                ));
            }
            error_max = error_max.max(err);
            if a != 0. {
                entries.push((i, j, a));
                if i != j {
                    entries.push((j, i, a));
                }
            }
        }
    }
    Ok((entries, error_max))
}

/// Internal factory. The field shares the exact certified foil trace. The
/// caller must reserve its construction against the same extraction budget.
pub(super) fn assemble(
    foils: &[Foil],
    names: Vec<String>,
    retained: &[usize],
    ports: &[[usize; 2]],
    field: Field<'_>,
    max_frequency_hz: f64,
    epsilon: f64,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
    training: Option<&TrainingPoint>,
) -> Result<ReducedSpace> {
    let preparation = training.map_or(Preparation::Harmonic, Preparation::Single);
    Ok(assemble_impl(
        foils,
        names,
        retained,
        ports,
        field,
        max_frequency_hz,
        epsilon,
        budget,
        start,
        wall,
        preparation,
    )?
    .0)
}

pub(super) struct MatchedGrid<'a> {
    pub frequencies: &'a [f64],
    pub references: &'a [f64],
    pub max_iterations: usize,
}
enum Preparation<'a> {
    Harmonic,
    Single(&'a TrainingPoint),
    Matched(MatchedGrid<'a>),
}
pub(super) fn solve_matched_grid(
    foils: &[Foil],
    names: Vec<String>,
    retained: &[usize],
    ports: &[[usize; 2]],
    field: Field<'_>,
    epsilon: f64,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
    grid: MatchedGrid<'_>,
) -> Result<(ReducedSpace, serde_json::Value)> {
    let max_frequency_hz = *grid
        .frequencies
        .last()
        .ok_or_else(|| error("BOARD_FOIL_GRID", "empty frequency axis"))?;
    let (space, checked) = assemble_impl(
        foils,
        names,
        retained,
        ports,
        field,
        max_frequency_hz,
        epsilon,
        budget,
        start,
        wall,
        Preparation::Matched(grid),
    )?;
    Ok((
        space,
        checked.ok_or_else(|| error("BOARD_FOIL_GRID", "checked output missing"))?,
    ))
}
/// Use the mixed projection only to construct the initial sparse harmonic
/// basis. Every action and correction thereafter is on original native cells.
pub(super) fn solve_mixed_grid(
    map: &super::mixed_conductor::MixedMap<'_>,
    mesh: &super::mesh::LayeredMesh,
    names: &[String],
    ports: &[[usize; 2]],
    packages: &[super::checked_packages::Validated],
    field: &Field<'_>,
    partition: Option<&super::spatial_partition::Partition>,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
    grid: MatchedGrid<'_>,
) -> Result<(serde_json::Value, usize)> {
    let n = map.full.potential_count();
    let ni = map.full.current_count();
    // Each phase can supply a voltage shape and its conductive current,
    // plus one independent induced-current correction.
    let reserve = 4 * grid.max_iterations;
    let map_retained = map.retained_bytes()?;
    let field_base = map_retained
        .checked_add(field.planned_bytes())
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed field plan overflow"))?;
    // Admit all non-modal full-space buffers before constructing the bounded
    // exact magnetic cache. Later factor plans add their actual fill/scratch.
    let buffer_base = native_sizes(n, ni, 0, field_base)?;
    eprintln!("P7_MIXED_MEMORY_STAGE stage=magnetic-admission fullPotentials={n} fullCurrents={ni} mapBytes={} electricPlanBytes={} nativeWorkBytes={} totalBaseBytes={buffer_base} budgetBytes={}",
        map_retained, field.planned_bytes(), native_work_bytes(n, ni, 0)?, budget.max_bytes);
    budget.check(buffer_base)?;
    let magnetic = super::volume_integral_magnetic::Streamed::new(
        mesh,
        &map.full,
        12,
        buffer_base,
        budget,
        start,
        wall,
    )?;
    let base = field_base
        .checked_add(magnetic.owned_bytes)
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed operator plan overflow"))?;
    let resident = native_sizes(n, ni, 0, base)?;
    budget.check(resident).map_err(|failure|format!("{failure}; stage=mixed-resident-admission plannedBytes={resident} budgetBytes={}",budget.max_bytes))?;

    let mut terms = vec![Vec::new(); map.current_count()];
    for &(i, j, a) in &map.incidence {
        terms[j].push((i, a));
    }
    if terms.iter().any(|col| col.len() > 4) {
        return Err(error(
            "MIXED_FOIL_CONTACT",
            "mixed current needs original conservative stencil",
        ));
    }
    let mixed_cells: Vec<_> = terms
        .into_iter()
        .zip(&map.resistance_ohm)
        .map(|(t, &r)| Cell::new(t, r))
        .collect();
    let (mut basis, mixed_peak, mixed_dc) = harmonic(
        &mixed_cells,
        map.potential_count(),
        &map.electrodes.iter().copied().collect(),
        Some(&map.potential_components),
        budget,
        // Full native Cell/triplet/work buffers are constructed only after
        // mixed_cells and mixed_dc below are dropped. harmonic() accounts
        // its own mixed-coordinate work; do not add the future full work a
        // second time. The shared map/field/magnetic storage stays in base.
        base,
        start,
        wall,
        reserve,
        true,
    )?;
    drop(mixed_dc);
    drop(mixed_cells);

    let mode_cap = basis
        .retained
        .len()
        .checked_add(reserve)
        .ok_or_else(|| error("MEMORY_LIMIT", "native mixed mode cap overflow"))?;
    // Each physical component has one constant-potential anchor and no
    // corresponding DC current. Retain all voltage/charge modes; do not
    // reserve a full current/action vector for a floating constant mode.
    // grow() adds at most four currents and two voltages per iteration;
    // the existing four-per-kind reserve remains conservative.
    let current_mode_cap = basis.retained.len().checked_sub(map.full.components.len())
        .and_then(|v|v.checked_add(reserve)).ok_or_else(||error("MEMORY_LIMIT","native current mode bound overflow"))?;
    let full_plan = if partition.is_some() {
        native_partitioned_sizes(n, ni, mode_cap, current_mode_cap, base)?
    } else {
        native_mode_sizes(n, ni, mode_cap, current_mode_cap, base)?
    };
    if partition.is_some() {eprintln!("P7_MIXED_MEMORY_STAGE stage=partitioned-global-scratch-omission omittedBytes={} reason=global-triplets-and-CSC-not-constructed",native_global_sparse_bytes(ni)?);}
    eprintln!("P7_REDUCED_MEMORY_PLAN fullPotentials={n} fullCurrents={ni} retained={} enrichmentIterations={} modeCap={mode_cap} workBytes={} descriptorBytes={} baseBytes={base} plannedBytes={full_plan} budgetBytes={}",
        basis.retained.len(),grid.max_iterations,native_work_modes(n,ni,mode_cap,current_mode_cap)?,(mode_cap+current_mode_cap)*(mode_cap+current_mode_cap)*2048,budget.max_bytes);
    budget.check(full_plan).map_err(|failure|format!("{failure}; stage=full-mode-admission modeCap={mode_cap} plannedBytes={full_plan} budgetBytes={}",budget.max_bytes))?;
    let originals = map.original_representatives();
    for i in &mut basis.retained {
        *i = originals[*i];
    }
    basis.voltage = basis
        .voltage
        .iter()
        .map(|v| map.lift_potentials(v))
        .collect::<Result<Vec<_>>>()?;
    let mut cells = Vec::with_capacity(ni);
    for (index, cell) in map.full.cells.iter().enumerate() {
        check_time(start, wall)?;
        let potentials = partition.map_or(cell.potentials, |partition| partition.weld(index));
        for axis in 0..3 {
            let terms = potentials
                .iter()
                .enumerate()
                .map(|(k, &i)| (i, cell.incidence[k][axis]))
                .collect();
            cells.push(Cell::new(terms, cell.resistance_ohm));
        }
    }
    let (full_peak, dc) = correction(
        &cells,
        n,
        &basis.retained.iter().copied().collect(),
        full_plan,
        budget,
        start,
        wall,
        partition,
    )?;
    // A uniform-depth lift changes the native reaction at a surface pad.
    // Restore the exact full DC cardinal states before spending any AC mode
    // budget on charge or induction. No authored electrode value is changed.
    basis.current.clear();
    for (coordinates, modes) in map.full.components.iter().zip(&basis.components) {
        let (&anchor, other) = modes.split_last().unwrap();
        for &mode in other {
            check_time(start, wall)?;
            let (q, residual) = complete_harmonic_current(&cells, &mut basis.voltage[mode], &dc, start, wall)?;
            basis.current.push(q);
            basis.dc_residual = basis.dc_residual.max(residual);
        }
        for &i in coordinates {
            basis.voltage[anchor][i] = 1. - other.iter().map(|&m| basis.voltage[m][i]).sum::<f64>();
        }
    }
    let charges = basis
        .voltage
        .iter()
        .map(|v| {
            check_time(start, wall)?;
            field.apply(v)
        })
        .collect::<Result<Vec<_>>>()?;
    let angular_max = 2.
        * std::f64::consts::PI
        * grid
            .frequencies
            .last()
            .copied()
            .ok_or_else(|| error("MIXED_FOIL_GRID", "frequency grid required"))?;
    let batch = |q: &[Vec<f64>], goals: Option<&[f64]>, _, _| -> Result<Magnetic> {
        let derived;
        let goals = if let Some(goals) = goals {
            goals
        } else {
            derived = magnetic.resistive_goals(q, angular_max)?;
            &derived
        };
        let (actions, bounds, certificate) = magnetic.actions_certified(q, goals)?;
        Ok(Magnetic {
            actions,
            error_bounds: Some(bounds),
            certificate: Some(certificate),
            reciprocity: 0.,
            refinement: 0.,
        })
    };
    let initial_magnetic = batch(&basis.current, None, start, wall)?;
    let actions = initial_magnetic.actions;
    let basis_certificate = initial_magnetic.certificate;
    let (model, max_electric_reciprocity) =
        make_model(&cells, n, &basis, names, &charges, &actions, ports)?;
    let mut space = ReducedSpace {
        model,
        cells,
        dc_harmonic_residual: basis.dc_residual,
        basis,
        charges,
        magnetic_actions: actions,
        magnetic_basis_certificate: basis_certificate,
        ports: ports.to_vec(),
        full_potentials: n,
        max_frequency_hz: *grid
            .frequencies
            .last()
            .ok_or_else(|| error("MIXED_FOIL_GRID", "empty frequency axis"))?,
        planned_bytes: full_peak.max(mixed_peak),
        // Pair reciprocity is structural. Quadrature and spatial coverage are
        // independently unverified; these fields are not exposed for mixed.
        max_magnetic_reciprocity: 0.,
        max_magnetic_refinement: 0.,
        max_electric_reciprocity,
        enrichment: None,
        native_allocation: true,
    };
    let retained_bytes = map_retained
        .checked_add(magnetic.owned_bytes)
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed retained plan overflow"))?;
    let training_budget = AllocationBudget {
        max_bytes: budget
            .max_bytes
            .checked_sub(retained_bytes)
            .ok_or_else(|| error("MEMORY_LIMIT", "mixed retained plan exceeds budget"))?,
        ..budget
    };
    let mut checked = space.train_grid(
        &grid,
        names,
        &dc,
        field,
        &batch,
        training_budget,
        start,
        wall,
        true,
        packages,
        partition
            .filter(|partition| !partition.compact)
            .map(|partition| (partition, &map.full)),
    )?;
    checked["magneticOperator"] = magnetic.evidence();
    checked["magneticOperator"]["basisCover"] =
        space.magnetic_basis_certificate.clone().ok_or_else(|| {
            error(
                "HIERARCHICAL_MAGNETIC_POLICY",
                "common basis cover certificate required",
            )
        })?;
    checked["magneticOperator"]["maxFullKvlUncertaintyRelative"] =
        checked["quality"]["maxFullKvlUncertaintyRelative"].clone();
    if let Some(backend) = &dc.partitioned {
        checked["conductiveCorrection"] = backend.evidence();
    }
    Ok((checked, space.planned_bytes))
}

fn assemble_impl(
    foils: &[Foil],
    names: Vec<String>,
    retained: &[usize],
    ports: &[[usize; 2]],
    field: Field<'_>,
    max_frequency_hz: f64,
    epsilon: f64,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
    preparation: Preparation<'_>,
) -> Result<(ReducedSpace, Option<serde_json::Value>)> {
    let mode_reserve = 2 * match &preparation {
        Preparation::Harmonic => 0,
        Preparation::Single(training) => training.iteration_limit()?,
        Preparation::Matched(grid) => grid.max_iterations,
    };
    let n = names.len();
    let supports = foils
        .iter()
        .try_fold(0usize, |n, f| n.checked_add(f.triangles.len()))
        .ok_or_else(|| error("MEMORY_LIMIT", "foil support count overflow"))?;
    if n == 0
        || supports == 0
        || n > budget.max_nodes
        || supports > budget.max_tetrahedra
        || ports.is_empty()
        || ports.len() > 16
        || retained.iter().any(|&i| i >= n)
        || ports.iter().any(|p| p[0] >= n || p[1] >= n || p[0] == p[1])
    {
        return Err(error(
            "FOIL_REDUCTION",
            "finite bounded physical foil, retained traces and ports required",
        ));
    }
    budget.check(sizes(n, 2 * supports, 0, field.planned_bytes())?)?;
    super::current_space::verify_foil_band(foils, max_frequency_hz, epsilon)?;
    let mut cells = Vec::with_capacity(2 * supports);
    let mut prisms = Vec::with_capacity(supports);
    let mut density = Vec::with_capacity(supports);
    for foil in foils {
        for triangle in &foil.triangles {
            check_time(start, wall)?;
            let xy = triangle.map(|i| foil.xy_m[i]);
            let sheet = SheetCell::new(
                foil.source_id.clone(),
                xy,
                foil.z_m[1] - foil.z_m[0],
                foil.conductivity_s_per_m,
            )?;
            for axis in 0..2 {
                let terms = triangle
                    .iter()
                    .enumerate()
                    .map(|(k, &i)| (foil.potential_dofs[i], sheet.incidence[k][axis]))
                    .collect();
                cells.push(Cell::new(terms, sheet.resistance_ohm));
            }
            prisms.push(Prism::new(foil.source_id.clone(), xy, foil.z_m)?);
            density.push(sheet.current_density_per_amp_m2);
        }
    }
    let contacts: BTreeSet<_> = retained
        .iter()
        .copied()
        .chain(ports.iter().flatten().copied())
        .collect();
    let (basis, peak, dc) = harmonic(
        &cells,
        n,
        &contacts,
        None,
        budget,
        field.planned_bytes(),
        start,
        wall,
        mode_reserve,
        false,
    )?;
    let charges = basis
        .voltage
        .iter()
        .map(|v| {
            check_time(start, wall)?;
            field.apply(v)
        })
        .collect::<Result<Vec<_>>>()?;
    if charges.iter().any(|c| c.len() != n) {
        return Err(error(
            "FOIL_REDUCTION",
            "common C action does not match physical trace",
        ));
    }
    let training_resources = if matches!(preparation, Preparation::Harmonic) {
        None
    } else {
        Some((field, dc))
    };
    let batch = |q: &[Vec<f64>], _goals: Option<&[f64]>, start, wall| {
        magnetic(&prisms, &density, q, start, wall)
    };
    let Magnetic {
        actions: magnetic_actions,
        reciprocity: max_magnetic_reciprocity,
        refinement: max_magnetic_refinement,
        ..
    } = batch(&basis.current, None, start, wall)?;
    let (model, max_electric_reciprocity) = make_model(
        &cells,
        n,
        &basis,
        &names,
        &charges,
        &magnetic_actions,
        ports,
    )?;
    let dc_harmonic_residual = basis.dc_residual;
    check_time(start, wall)?;
    let mut space = ReducedSpace {
        model,
        cells,
        basis,
        charges,
        magnetic_actions,
        magnetic_basis_certificate: None,
        ports: ports.to_vec(),
        full_potentials: n,
        max_frequency_hz,
        planned_bytes: peak,
        max_magnetic_reciprocity,
        max_magnetic_refinement,
        max_electric_reciprocity,
        dc_harmonic_residual,
        enrichment: None,
        native_allocation: false,
    };
    let checked = match (preparation, training_resources) {
        (Preparation::Harmonic, None) => None,
        (Preparation::Single(training), Some((field, dc))) => {
            space.train(training, &names, &dc, &field, &batch, budget, start, wall)?;
            None
        }
        (Preparation::Matched(grid), Some((field, dc))) => Some(space.train_grid(
            &grid,
            &names,
            &dc,
            &field,
            &batch,
            budget,
            start,
            wall,
            false,
            &[],
            None,
        )?),
        _ => unreachable!("preparation resources match their scope"),
    };
    Ok((space, checked))
}

fn make_model(
    cells: &[Cell],
    n: usize,
    basis: &Basis,
    names: &[String],
    charges: &[Vec<f64>],
    magnetic_actions: &[Vec<f64>],
    ports: &[[usize; 2]],
) -> Result<(Model, f64)> {
    let nv = basis.voltage.len();
    let ni = basis.current.len();
    let (capacitance_f, max_electric_reciprocity) =
        congruence(&basis.voltage, &charges, 1e-9, "C")?;
    let (inductance_h, _) = congruence(&basis.current, &magnetic_actions, 1e-9, "L")?;
    let resistance_actions = basis
        .current
        .iter()
        .map(|q| resistive_action(cells, q))
        .collect::<Vec<Vec<f64>>>();
    let (resistance_ohm, _) = congruence(&basis.current, &resistance_actions, 1e-9, "R")?;
    // Differentiate the exact component partition analytically, as in
    // volume_reduction. A divergence-free loop need not acquire spurious
    // net incidence from independent roundoff in its cancellation rows.
    let mut incidence = Vec::new();
    for (j, q) in basis.current.iter().enumerate() {
        let d = divergence(cells, q, n);
        for component in &basis.components {
            let (&anchor, other) = component.split_last().unwrap();
            let mut balance = Sum::default();
            for &i in other {
                let value = sum(&basis.voltage[i], &d);
                balance = balance.add(Sum::from(value));
                if value != 0. {
                    incidence.push((i, j, value));
                }
            }
            let value = -balance.value();
            if value != 0. {
                incidence.push((anchor, j, value));
            }
        }
    }
    let potential_nodes: Vec<_> = basis
        .retained
        .iter()
        .map(|&i| names[i].clone())
        .chain((basis.retained.len()..nv).map(|i| format!("foil-bubble-{i}")))
        .collect();
    let mut model = Model {
        format: "coupled-rlc".into(),
        reference_topology: "infinity".into(),
        potential_nodes,
        current_dofs: (0..ni).map(|i| format!("harmonic-current-{i}")).collect(),
        incidence,
        port_incidence: vec![],
        resistance_ohm,
        inductance_h,
        capacitance_f,
        structural_gauge_modes: vec![],
        conserved_potential_modes: vec![],
        port_terminals: Some(vec![]),
    };
    for component in &basis.components {
        let mut mode = vec![0.; nv];
        for &i in component {
            mode[i] = 1.;
        }
        model.conserved_potential_modes.push(mode);
    }
    for (p, &[a, b]) in ports.iter().enumerate() {
        let a =
            basis.retained.iter().position(|&i| i == a).ok_or_else(|| {
                error("BOARD_PORT", "positive electrode missing in retained trace")
            })?;
        let b =
            basis.retained.iter().position(|&i| i == b).ok_or_else(|| {
                error("BOARD_PORT", "negative electrode missing in retained trace")
            })?;
        model.port_incidence.extend([(a, p, 1.), (b, p, -1.)]);
        model.port_terminals.as_mut().unwrap().push(PortTerminals {
            positive: Terminal::Node(Node {
                node: model.potential_nodes[a].clone(),
            }),
            negative: Terminal::Node(Node {
                node: model.potential_nodes[b].clone(),
            }),
        });
    }
    Ok((model, max_electric_reciprocity))
}

fn lift(basis: &[Vec<f64>], x: &[f64], dimension: usize) -> Vec<f64> {
    (0..dimension)
        .map(|i| {
            basis
                .iter()
                .zip(x)
                .fold(Sum::default(), |s, (b, &v)| s.add(Sum::from(b[i]).mul(v)))
                .value()
        })
        .collect()
}
impl ReducedSpace {
    /// These coefficients are diagnostic. Only solve_checked evaluates the
    /// unreduced equations before returning a point.
    pub fn diagnostic_model(&self) -> &Model {
        &self.model
    }
    pub fn reduced_potential(&self, full: usize) -> Option<usize> {
        self.basis.retained.binary_search(&full).ok()
    }
    pub fn solve_checked(
        &self,
        frequency_hz: f64,
        drive: &[f64],
        budget: Budget,
    ) -> Result<CheckedPoint> {
        let (system, state) = self.solve_state(frequency_hz, drive, budget)?;
        let n = self.basis.voltage.len() + self.basis.current.len();
        let zero = vec![0.; n];
        let im = if frequency_hz > 0. {
            &state[n..]
        } else {
            &zero
        };
        let w = 2. * std::f64::consts::PI * frequency_hz;
        let residual = self
            .full_residual(w, &state[..n], im, drive, None, None)?
            .checked()?;
        Ok(CheckedPoint {
            voltage_real: system.port_voltages(&state, self.ports.len()),
            voltage_imaginary: system.port_voltages(im, self.ports.len()),
            residual,
            coverage: "unverified",
        })
    }
    fn solve_state(
        &self,
        frequency_hz: f64,
        drive: &[f64],
        budget: Budget,
    ) -> Result<(System, Vec<f64>)> {
        if !frequency_hz.is_finite()
            || frequency_hz < 0.
            || frequency_hz > self.max_frequency_hz
            || drive.len() != self.ports.len()
            || drive.iter().any(|x| !x.is_finite())
        {
            return Err(error(
                "FOIL_REDUCTION",
                "finite screened frequency and complete drive required",
            ));
        }
        let mut system = self.compile_system(budget)?;
        let nv = self.basis.voltage.len();
        let n = nv + self.basis.current.len();
        let mut rhs = system.port_rhs(drive);
        if frequency_hz > 0. {
            rhs.resize(2 * n, 0.);
        }
        let w = 2. * std::f64::consts::PI * frequency_hz;
        let state = system.solve(w, rhs, &vec![0.; n], &mut Counters::default())?;
        Ok((system, state))
    }
    fn compile_system(&self, budget: Budget) -> Result<System> {
        let budget = self.system_budget(budget)?;
        System::compile(&self.model, self.ports.len(), Some(&budget))
    }
    fn system_budget(&self, mut budget: Budget) -> Result<Budget> {
        let requested = budget.max_memory_mi_b.unwrap_or(256.);
        let resident = if self.native_allocation {
            native_work_modes(
                self.full_potentials,
                self.cells.len(),
                self.basis.voltage.len(),
                self.basis.current.len(),
            )?
        } else {
            self.full_potentials
                .saturating_mul(1024)
                .saturating_add(self.cells.len().saturating_mul(16384))
                .saturating_add(
                    self.full_potentials
                        .saturating_add(self.cells.len())
                        .saturating_mul(self.basis.voltage.len().max(self.basis.current.len()))
                        .saturating_mul(64),
                )
        };
        let available = requested - resident as f64 / 1048576.;
        if !available.is_finite() || available <= 0. {
            return Err(error(
                "MEMORY_LIMIT",
                "reduced response and retained full-space actions exceed budget",
            ));
        }
        budget.max_memory_mi_b = Some(available);
        Ok(budget)
    }
    fn physical_injection(&self, drive: &[f64]) -> Vec<f64> {
        let mut injected = vec![Sum::default(); self.full_potentials];
        for (&[a, b], &i) in self.ports.iter().zip(drive) {
            injected[a] = injected[a].add(Sum::from(i));
            injected[b] = injected[b].sub(Sum::from(i));
        }
        injected.into_iter().map(Sum::value).collect()
    }
    fn full_residual(
        &self,
        w: f64,
        re: &[f64],
        im: &[f64],
        drive: &[f64],
        drive_im: Option<&[f64]>,
        magnetic: Option<(&MagneticBatch<'_>, Instant, Duration)>,
    ) -> Result<FullResidual> {
        let nv = self.basis.voltage.len();
        let nr = self.full_potentials;
        let v = [
            lift(&self.basis.voltage, &re[..nv], nr),
            lift(&self.basis.voltage, &im[..nv], nr),
        ];
        let q = [
            lift(&self.basis.current, &re[nv..], self.cells.len()),
            lift(&self.basis.current, &im[nv..], self.cells.len()),
        ];
        let c = [
            lift(&self.charges, &re[..nv], nr),
            lift(&self.charges, &im[..nv], nr),
        ];
        let (l, magnetic_bounds, magnetic_certificate) = if self.native_allocation {
            // Certify the original recovered current, not a reduced matrix's
            // own residual or a rounded lift of precomputed basis images.
            let (action, start, wall) = magnetic.ok_or_else(|| {
                error(
                    "HIERARCHICAL_MAGNETIC_UNCERTIFIED",
                    "original-state magnetic action callback required",
                )
            })?;
            let mut denominator_lower = 0f64;
            for phase in 0..2 {
                for (j, cell) in self.cells.iter().enumerate() {
                    let (_, lower, _) =
                        super::volume_integral_magnetic::hierarchical_magnetic::kvl_cell(
                            cell.resistance,
                            q[phase][j],
                            &cell.terms,
                            &v[phase],
                            0.,
                            0.,
                            0.,
                        )?;
                    denominator_lower = denominator_lower.max(lower);
                }
            }
            let zero = q.iter().all(|row| row.iter().all(|v| *v == 0.));
            let goal = if zero {
                1.
            } else {
                super::volume_integral_magnetic::hierarchical_magnetic::action_goal(
                    denominator_lower,
                    w,
                )?
            };
            let result = action(
                &[q[0].clone(), q[1].clone()],
                Some(&[goal, goal]),
                start,
                wall,
            )?;
            let certificate = result.certificate;
            let actions: [Vec<f64>; 2] = result.actions.try_into().map_err(|_| {
                error(
                    "HIERARCHICAL_MAGNETIC_INTERVAL",
                    "two complete original actions required",
                )
            })?;
            let bounds: [Vec<f64>; 2] = result
                .error_bounds
                .ok_or_else(|| {
                    error(
                        "HIERARCHICAL_MAGNETIC_UNCERTIFIED",
                        "original-state action error enclosure required",
                    )
                })?
                .try_into()
                .map_err(|_| {
                    error(
                        "HIERARCHICAL_MAGNETIC_INTERVAL",
                        "two original action enclosures required",
                    )
                })?;
            if actions.iter().any(|row| row.len() != self.cells.len())
                || bounds.iter().any(|row| {
                    row.len() != self.cells.len() || row.iter().any(|v| !v.is_finite() || *v < 0.)
                })
            {
                return Err(error(
                    "HIERARCHICAL_MAGNETIC_INTERVAL",
                    "original action/error shape mismatch",
                ));
            }
            (actions, Some(bounds), certificate)
        } else {
            (
                [
                    lift(&self.magnetic_actions, &re[nv..], self.cells.len()),
                    lift(&self.magnetic_actions, &im[nv..], self.cells.len()),
                ],
                None,
                None,
            )
        };
        let d = q.each_ref().map(|x| divergence(&self.cells, x, nr));
        let g = v.each_ref().map(|x| gradient(&self.cells, x));
        if [&v, &q, &c, &l, &d, &g]
            .into_iter()
            .flatten()
            .flatten()
            .any(|x| !x.is_finite())
        {
            return Err(error(
                "FOIL_REDUCTION_RESIDUAL",
                "nonfinite unreduced action",
            ));
        }

        let injected = [
            self.physical_injection(drive),
            drive_im.map_or_else(|| vec![0.; nr], |drive| self.physical_injection(drive)),
        ];
        let (mut kcl, mut kcl_scale, mut kvl, mut kvl_scale) = (0_f64, 0_f64, 0_f64, 0_f64);
        let mut kvl_uncertainty = 0f64;
        let mut kcl_rows = [vec![0.; nr], vec![0.; nr]];
        let mut kvl_rows = [vec![0.; self.cells.len()], vec![0.; self.cells.len()]];
        for k in 0..2 {
            let s = if k == 0 { -1. } else { 1. };
            for i in 0..nr {
                let a = d[k][i];
                let b = s * w * c[1 - k][i];
                let z = injected[k][i];
                kcl_rows[k][i] = a + b - z;
                kcl = kcl.max(kcl_rows[k][i].abs());
                kcl_scale = kcl_scale.max(a.abs() + b.abs() + z.abs());
            }
            for (j, cell) in self.cells.iter().enumerate() {
                let a = cell.resistance * q[k][j];
                let b = -g[k][j];
                let z = s * w * l[1 - k][j];
                kvl_rows[k][j] = a + b + z;
                if let Some(bounds) = &magnetic_bounds {
                    let (numerator, denominator, uncertainty) =
                        super::volume_integral_magnetic::hierarchical_magnetic::kvl_cell(
                            cell.resistance,
                            q[k][j],
                            &cell.terms,
                            &v[k],
                            l[1 - k][j],
                            s * w,
                            bounds[1 - k][j],
                        )?;
                    kvl = kvl.max(numerator);
                    kvl_scale = kvl_scale.max(denominator);
                    kvl_uncertainty = kvl_uncertainty.max(uncertainty);
                } else {
                    kvl = kvl.max(kvl_rows[k][j].abs());
                    kvl_scale = kvl_scale.max(a.abs() + b.abs() + z.abs());
                }
            }
        }
        let kcl_relative = kcl / kcl_scale.max(f64::MIN_POSITIVE);
        let kvl_relative = if magnetic_bounds.is_some() {
            super::volume_integral_magnetic::hierarchical_magnetic::ratio(kvl, kvl_scale)?
        } else {
            kvl / kvl_scale.max(f64::MIN_POSITIVE)
        };
        let kvl_uncertainty_relative = if magnetic_bounds.is_some() {
            super::volume_integral_magnetic::hierarchical_magnetic::ratio(
                kvl_uncertainty,
                kvl_scale,
            )?
        } else {
            0.
        };
        if !kcl_relative.is_finite() || !kvl_relative.is_finite() {
            return Err(error(
                "FOIL_REDUCTION_RESIDUAL",
                "nonfinite unreduced residual",
            ));
        }
        Ok(FullResidual {
            norm: Residual {
                kcl_relative,
                kvl_relative,
            },
            kcl: kcl_rows,
            kvl: kvl_rows,
            kvl_uncertainty_relative,
            magnetic_certificate,
        })
    }
}

struct FullResidual {
    norm: Residual,
    kcl: [Vec<f64>; 2],
    kvl: [Vec<f64>; 2],
    kvl_uncertainty_relative: f64,
    magnetic_certificate: Option<serde_json::Value>,
}
impl FullResidual {
    fn checked(&self) -> Result<Residual> {
        if self.norm.kcl_relative > RESIDUAL_LIMIT || self.norm.kvl_relative > RESIDUAL_LIMIT {
            return Err(error(
                "FOIL_REDUCTION_RESIDUAL",
                &format!(
                    "unreduced KCL={:e}, KVL={:e}; foil coverage unverified",
                    self.norm.kcl_relative, self.norm.kvl_relative
                ),
            ));
        }
        Ok(self.norm)
    }
}

fn append_current_mode(
    cells: &[Cell],
    basis: &mut Vec<Vec<f64>>,
    candidate: Vec<f64>,
) -> Result<()> {
    if basis.len() < cells.len() {
        let actions: Vec<_> = basis.iter().map(|q| resistive_action(cells, q)).collect();
        if let Some((q, _)) = metric_mode(
            candidate,
            basis,
            &actions,
            |q| Ok(resistive_action(cells, q)),
            true,
        )? {
            basis.push(q);
        }
    }
    Ok(())
}

fn resistive_action(cells: &[Cell], q: &[f64]) -> Vec<f64> {
    q.iter()
        .zip(cells)
        .map(|(&v, c)| v * c.resistance)
        .collect()
}
fn metric_mode(
    mut candidate: Vec<f64>,
    basis: &[Vec<f64>],
    actions: &[Vec<f64>],
    apply: impl Fn(&[f64]) -> Result<Vec<f64>>,
    unit_metric: bool,
) -> Result<Option<(Vec<f64>, Vec<f64>)>> {
    // Normalize the incoming amplitude and solve the scaled Gram projection.
    // Dependence is measured in the candidate's own energy norm, not against
    // a global scale that would erase a weak island.
    let scale = candidate.iter().map(|x| x.abs()).fold(0., f64::max);
    if scale == 0. {
        return Ok(None);
    }
    for v in &mut candidate {
        *v /= scale;
    }
    let mut image = apply(&candidate)?;
    let original = sum(&candidate, &image);
    if !original.is_finite() || original <= 0. {
        return Err(error("FOIL_ENRICHMENT", "nonpositive candidate energy"));
    }
    if !basis.is_empty() {
        let scales: Vec<_> = basis
            .iter()
            .zip(actions)
            .map(|(v, a)| sum(v, a).sqrt())
            .collect();
        if scales.iter().any(|s| !s.is_finite() || *s <= 0.) {
            return Err(error(
                "FOIL_ENRICHMENT",
                "existing metric rank is unresolved",
            ));
        }
        let gram = Mat::from_fn(basis.len(), basis.len(), |i, j| {
            let (a, b) = (i.min(j), i.max(j));
            sum(&basis[a], &actions[b]) / scales[a] / scales[b]
        });
        let factor = gram.llt(Side::Lower).map_err(|_| {
            error(
                "FOIL_ENRICHMENT",
                "scaled Gram is not positive definite; no regularization",
            )
        })?;
        for _ in 0..2 {
            let rhs = Mat::from_fn(basis.len(), 1, |i, _| sum(&basis[i], &image) / scales[i]);
            let weights = factor.solve(&rhs);
            for (j, (v, a)) in basis.iter().zip(actions).enumerate() {
                let w = weights[(j, 0)] / scales[j];
                for (i, (v, a)) in v.iter().zip(a).enumerate() {
                    candidate[i] -= w * v;
                    image[i] -= w * a;
                }
            }
        }
    }
    let remainder = sum(&candidate, &image);
    if remainder.abs() <= 1e-24 * original {
        return Ok(None);
    }
    if !remainder.is_finite() || remainder <= 0. {
        return Err(error(
            "FOIL_ENRICHMENT",
            "unresolved projected candidate energy",
        ));
    }
    let norm = if unit_metric {
        remainder.sqrt()
    } else {
        candidate.iter().map(|x| x.abs()).fold(0., f64::max)
    };
    for v in &mut candidate {
        *v /= norm;
    }
    let image = apply(&candidate)?;
    Ok(Some((candidate, image)))
}

impl ReducedSpace {
    fn train(
        &mut self,
        training: &TrainingPoint,
        names: &[String],
        dc: &DcCorrection,
        field: &Field<'_>,
        magnetic: &MagneticBatch<'_>,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<()> {
        if training.frequency_hz <= 0. || !training.drive.iter().any(|&x| x != 0.) {
            return Err(error(
                "FOIL_ENRICHMENT",
                "positive AC frequency and nonzero physical drive required",
            ));
        }
        let initial_nv = self.basis.voltage.len();
        let initial_ni = self.basis.current.len();
        let limit = training.iteration_limit()?;
        let mut history = Vec::with_capacity(limit + 1);
        let outcome = (|| {
            for iteration in 0..=limit {
                check_time(start, wall)?;
                let available = budget
                    .max_bytes
                    .saturating_sub(field.planned_bytes())
                    .saturating_sub(dc.owned_bytes);
                let solve_budget = Budget {
                    max_memory_mi_b: Some((available as f64 / 1048576.).min(4096.)),
                    max_seconds: Some(wall.saturating_sub(start.elapsed()).as_secs_f64()),
                };
                let (_, state) =
                    self.solve_state(training.frequency_hz, &training.drive, solve_budget)?;
                let n = self.basis.voltage.len() + self.basis.current.len();
                let w = 2. * std::f64::consts::PI * training.frequency_hz;
                let full = self.full_residual(
                    w,
                    &state[..n],
                    &state[n..],
                    &training.drive,
                    None,
                    Some((magnetic, start, wall)),
                )?;
                history.push(EnrichmentStep {
                    iteration,
                    potential_modes: self.basis.voltage.len(),
                    current_modes: self.basis.current.len(),
                    residual: full.norm,
                });
                if let Ok(final_residual) = full.checked() {
                    self.enrichment = Some(Enrichment {
                        training: training.clone(),
                        iterations: iteration,
                        history: history.clone(),
                        added_potential_modes: self.basis.voltage.len() - initial_nv,
                        added_current_modes: self.basis.current.len() - initial_ni,
                        initial_residual: history[0].residual,
                        final_residual,
                    });
                    return Ok(());
                }
                if iteration == limit {
                    return full.checked().map(|_| ());
                }
                self.grow(
                    &full, initial_nv, names, dc, field, magnetic, start, wall, false,
                )?;
            }
            unreachable!("bounded enrichment always returns at its final residual")
        })();
        outcome
            .map_err(|failure| format!("{failure}; enrichment limit={limit}; history={history:?}"))
    }

    fn grow(
        &mut self,
        full: &FullResidual,
        initial_nv: usize,
        names: &[String],
        dc: &DcCorrection,
        field: &Field<'_>,
        magnetic: &MagneticBatch<'_>,
        start: Instant,
        wall: Duration,
        pair_gradient: bool,
    ) -> Result<()> {
        let ni = self.basis.current.len();
        let nv = self.basis.voltage.len();
        if nv == self.full_potentials && ni == self.cells.len() {
            return Err(error(
                "FOIL_ENRICHMENT",
                "physical basis rank exhausted; coverage unverified",
            ));
        }

        for (kcl, kvl) in full.kcl.iter().zip(&full.kvl) {
            check_time(start, wall)?;
            // A delta_i = -r_KCL on free nodes. The zero trace Dirichlet
            // correction supplies a voltage bubble, while -R^-1 r_KVL
            // retains the divergence-free induced current contribution.
            let drop: Vec<_> = kvl
                .iter()
                .zip(&self.cells)
                .map(|(&v, c)| v / c.resistance)
                .collect();
            let rhs: Vec<_> = divergence(&self.cells, &drop, self.full_potentials)
                .into_iter()
                .zip(kcl)
                .map(|(a, b)| a - b)
                .collect();
            let bubble = dc.solve(&self.cells, &rhs)?;
            let candidate: Vec<_> = conductive(&self.cells, &bubble)
                .into_iter()
                .zip(drop)
                .map(|(a, b)| a - b)
                .collect();
            if self.basis.voltage.len() < self.full_potentials {
                if let Some((v, c)) = metric_mode(
                    bubble,
                    &self.basis.voltage[initial_nv..],
                    &self.charges[initial_nv..],
                    |v| field.apply(v),
                    false,
                )? {
                    if pair_gradient {
                        // Keep the R^-1 A^T image of the final C-orthogonal
                        // shape in the current space. The residual current
                        // may mostly be a divergence-free inductive loop.
                        let gradient = conductive(&self.cells, &v);
                        append_current_mode(&self.cells, &mut self.basis.current, gradient)?;
                    }
                    // Preserve the original constant lift by adding and
                    // subtracting the same zero-contact shape. The anchor
                    // is a coordinate transformation, not an electrical tie.
                    let anchor = self.basis.components[0][0];
                    for (x, b) in self.basis.voltage[anchor].iter_mut().zip(&v) {
                        *x -= b;
                    }
                    self.charges[anchor] = field.apply(&self.basis.voltage[anchor])?;
                    self.basis.components[0].push(self.basis.voltage.len());
                    self.basis.voltage.push(v);
                    self.charges.push(c);
                }
            }
            append_current_mode(&self.cells, &mut self.basis.current, candidate)?;
        }
        if self.basis.voltage.len() == nv && self.basis.current.len() == ni {
            return Err(error(
                "FOIL_ENRICHMENT",
                &format!(
                    "no independent correction; unreduced KCL={:e}, KVL={:e}; coverage unverified",
                    full.norm.kcl_relative, full.norm.kvl_relative
                ),
            ));
        }
        if self.basis.current.len() > ni {
            let common = if self.native_allocation {
                &self.basis.current[..]
            } else {
                &self.basis.current[ni..]
            };
            let added = magnetic(common, None, start, wall)?;
            self.max_magnetic_reciprocity = self.max_magnetic_reciprocity.max(added.reciprocity);
            self.max_magnetic_refinement = self.max_magnetic_refinement.max(added.refinement);
            if self.native_allocation {
                let revision = self
                    .magnetic_basis_certificate
                    .as_ref()
                    .and_then(|v| v["modelRevision"].as_u64())
                    .unwrap_or(0)
                    + 1;
                self.magnetic_actions = added.actions;
                self.magnetic_basis_certificate = added.certificate;
                if let Some(certificate) = &mut self.magnetic_basis_certificate {
                    certificate["modelRevision"] = serde_json::json!(revision);
                }
            } else {
                self.magnetic_actions.extend(added.actions);
            }
        }
        let (model, reciprocity) = make_model(
            &self.cells,
            self.full_potentials,
            &self.basis,
            names,
            &self.charges,
            &self.magnetic_actions,
            &self.ports,
        )?;
        self.model = model;
        self.max_electric_reciprocity = reciprocity;
        Ok(())
    }
}

impl ReducedSpace {
    fn train_grid(
        &mut self,
        grid: &MatchedGrid<'_>,
        names: &[String],
        dc: &DcCorrection,
        field: &Field<'_>,
        magnetic: &MagneticBatch<'_>,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
        pair_gradient: bool,
        packages: &[super::checked_packages::Validated],
        partition: Option<(
            &super::spatial_partition::Partition,
            &super::volume_current::Space,
        )>,
    ) -> Result<serde_json::Value> {
        use super::checked_packages;
        use serde_json::{json, Value};
        let initial_nv = self.basis.voltage.len();
        let ports = grid.references.len();
        let conductances: Vec<_> = grid.references.iter().map(|r| 1. / r).collect();
        let mut history = Vec::<Value>::with_capacity(grid.max_iterations + 1);
        let outcome = (|| {
            for iteration in 0..=grid.max_iterations {
                check_time(start, wall)?;
                let available = budget
                    .max_bytes
                    .saturating_sub(field.planned_bytes())
                    .saturating_sub(dc.owned_bytes);
                let solve_budget = Budget {
                    max_memory_mi_b: Some((available as f64 / 1048576.).min(4096.)),
                    max_seconds: Some(wall.saturating_sub(start.elapsed()).as_secs_f64()),
                };
                let (mut system, n, recovery) = if packages.is_empty() {
                    (
                        self.compile_system(solve_budget)?,
                        self.basis.voltage.len() + self.basis.current.len(),
                        None,
                    )
                } else {
                    let joint = checked_packages::compile(
                        &self.model,
                        packages,
                        ports,
                        &self.system_budget(solve_budget)?,
                    )?;
                    (joint.system, joint.size, Some(joint.recovery))
                };
                for (p, &g) in conductances.iter().enumerate() {
                    system.add_port_admittance(p, g, 0.)?;
                }
                let initial = vec![0.; n];
                let mut stats = Counters::default();
                let mut worst: Option<(f64, usize, FullResidual)> = None;
                let mut parameters = Vec::with_capacity(grid.frequencies.len());
                let mut samples = Vec::with_capacity(grid.frequencies.len());
                let (mut max_kcl, mut max_kvl, mut max_power) = (0_f64, 0_f64, 0_f64);
                let mut max_kvl_uncertainty = 0f64;
                for &frequency in grid.frequencies {
                    let w = 2. * std::f64::consts::PI * frequency;
                    let mut matrix = vec![vec![Value::Null; ports]; ports];
                    let mut columns = Vec::with_capacity(ports);
                    for input in 0..ports {
                        check_time(start, wall)?;
                        // Same 1 A RMS Norton and diagonal matched loads as
                        // P6 matched_waves. Native open Z is not formed.
                        let mut drive = vec![0.; ports];
                        drive[input] = 1.;
                        let mut rhs = system.port_rhs(&drive);
                        rhs.resize(2 * n, 0.);
                        let state = system.solve(w, rhs, &initial, &mut stats)?;
                        let real = system.port_voltages(&state[..n], ports);
                        let imaginary = system.port_voltages(&state[n..], ports);
                        let mut net_real: Vec<_> = drive
                            .iter()
                            .zip(&real)
                            .zip(&conductances)
                            .map(|((&source, &voltage), &g)| {
                                Sum::from(source).sub(Sum::from(voltage).mul(g)).value()
                            })
                            .collect();
                        let mut net_imaginary: Vec<_> = imaginary
                            .iter()
                            .zip(&conductances)
                            .map(|(&voltage, &g)| -voltage * g)
                            .collect();
                        let (re, im) = state.split_at(n);
                        let recovered = recovery.as_ref().map(|r| r.copper_state(re, im));
                        let package_samples = recovery
                            .as_ref()
                            .map(|r| r.package_states(w, re, im))
                            .transpose()?
                            .unwrap_or_default();
                        for package in &package_samples {
                            let [real, imaginary] = package.current();
                            net_real.push(-real);
                            net_imaginary.push(-imaginary);
                        }
                        let (copper_re, copper_im) = recovered
                            .as_ref()
                            .map_or((re, im), |s| (s[0].as_slice(), s[1].as_slice()));
                        let residual = self.full_residual(
                            w,
                            copper_re,
                            copper_im,
                            &net_real,
                            Some(&net_imaginary),
                            Some((magnetic, start, wall)),
                        )?;
                        let norm = residual.norm;
                        let kvl_uncertainty_relative = residual.kvl_uncertainty_relative;
                        let magnetic_certificate = residual.magnetic_certificate.clone();
                        max_kcl = max_kcl.max(norm.kcl_relative);
                        max_kvl = max_kvl.max(norm.kvl_relative);
                        max_kvl_uncertainty = max_kvl_uncertainty.max(kvl_uncertainty_relative);
                        let score = norm.kcl_relative.max(norm.kvl_relative);
                        if worst.as_ref().is_none_or(|(_, _, old)| {
                            score > old.norm.kcl_relative.max(old.norm.kvl_relative)
                        }) {
                            worst = Some((frequency, input, residual));
                        }
                        if score > RESIDUAL_LIMIT {
                            continue;
                        }
                        let mut outgoing = Sum::default();
                        for output in 0..ports {
                            let re = 2.
                                * (real[output]
                                    / grid.references[input].sqrt()
                                    / grid.references[output].sqrt())
                                - if output == input { 1. } else { 0. };
                            let im = 2.
                                * (imaginary[output]
                                    / grid.references[input].sqrt()
                                    / grid.references[output].sqrt());
                            if !re.is_finite() || !im.is_finite() {
                                return Err(error(
                                    "BOARD_FOIL_GRID",
                                    "nonfinite matched wave coefficient",
                                ));
                            }
                            outgoing = outgoing
                                .add(Sum::from(re).mul(re))
                                .add(Sum::from(im).mul(im));
                            matrix[output][input] = json!({"real":re,"imaginary":im});
                        }
                        let incident = grid.references[input] / 4.;
                        let net_power = Sum::from(1.).sub(outgoing).mul(incident).value();
                        let copper = recovered.as_ref().map_or_else(
                            || system.joule_loss(re) + system.joule_loss(im),
                            |s| checked_packages::copper_loss(&self.model, s),
                        );
                        if let Some(copper_state) = &recovered {
                            checked_packages::check_copper_power(
                                &self.model,
                                copper_state,
                                [&net_real, &net_imaginary],
                                copper,
                            )?;
                        }
                        let dissipated = package_samples
                            .iter()
                            .fold(Sum::from(copper), |p, s| p.add(Sum::from(s.loss_w)))
                            .value();
                        let power_error = (net_power - dissipated).abs();
                        if !power_error.is_finite()
                            || !copper.is_finite()
                            || copper < 0.
                            || power_error > 1e-12 + 1e-8 * (net_power.abs() + dissipated.abs())
                        {
                            return Err(error("BOARD_FOIL_POWER",&format!(
                                "f={frequency}, input={input}, wavePower={net_power:e}, dissipated={dissipated:e}, error={power_error:e}")));
                        }
                        max_power = max_power.max(power_error);
                        let mut column = json!({"input":input,"kclRelative":norm.kcl_relative,
                            "kvlRelative":norm.kvl_relative,"kvlUncertaintyRelative":kvl_uncertainty_relative,"copperLossW":copper,"powerBalanceErrorW":power_error});
                        if let Some(certificate) = magnetic_certificate {
                            column["magneticCertificate"] =
                                compact_magnetic_certificate(&certificate);
                        }
                        if !package_samples.is_empty() {
                            column["packages"] = json!(package_samples);
                        }
                        if let Some((partition, space)) = partition {
                            let nv = self.basis.voltage.len();
                            let state = [copper_re, copper_im];
                            let v = state
                                .map(|s| lift(&self.basis.voltage, &s[..nv], self.full_potentials));
                            let q = state
                                .map(|s| lift(&self.basis.current, &s[nv..], self.cells.len()));
                            let c =
                                state.map(|s| lift(&self.charges, &s[..nv], self.full_potentials));
                            let injection = [
                                self.physical_injection(&net_real),
                                self.physical_injection(&net_imaginary),
                            ];
                            column["spatialTrace"] = partition.observe(
                                space,
                                v.each_ref().map(Vec::as_slice),
                                q.each_ref().map(Vec::as_slice),
                                c.each_ref().map(Vec::as_slice),
                                injection.each_ref().map(Vec::as_slice),
                                w,
                                copper,
                                start,
                                wall,
                            )?;
                        }
                        columns.push(column);
                    }
                    parameters.push(matrix);
                    samples.push(json!({"frequencyHz":frequency,"columns":columns}));
                }
                drop(system);
                let (frequency, input, worst) =
                    worst.ok_or_else(|| error("BOARD_FOIL_GRID", "empty checked grid"))?;
                history.push(json!({"iteration":iteration,"frequencyHz":frequency,"input":input,
                    "potentialModes":self.basis.voltage.len(),"currentModes":self.basis.current.len(),
                    "maxKclRelative":max_kcl,"maxKvlRelative":max_kvl}));
                if worst.checked().is_ok() {
                    check_time(start, wall)?;
                    return Ok(json!({
                        "parameters":parameters,"samples":samples,"enrichment":history,
                        "quality":{"maxKclRelative":max_kcl,"maxKvlRelative":max_kvl,
                                   "maxFullKvlUncertaintyRelative":max_kvl_uncertainty,"maxColumnPowerBalanceErrorW":max_power},
                    }));
                }
                if worst.kvl_uncertainty_relative > RESIDUAL_LIMIT {
                    return Err(error("HIERARCHICAL_MAGNETIC_UNCERTIFIED","fixed geometry/order far-field enclosure exceeds original full KVL gate; no physical response admitted"));
                }
                if iteration == grid.max_iterations {
                    return Err(worst.checked().unwrap_err());
                }
                drop(parameters);
                drop(samples);
                self.grow(
                    &worst,
                    initial_nv,
                    names,
                    dc,
                    field,
                    magnetic,
                    start,
                    wall,
                    pair_gradient,
                )?;
            }
            unreachable!("bounded matched grid reaches a final residual")
        })();
        outcome.map_err(|failure| format!("{failure}; checked-grid history={history:?}"))
    }
}

#[cfg(test)]
mod mixed_basis_tests {
    use super::*;

    #[test]
    fn harmonic_refinement_preserves_contacts_on_high_contrast_series_network() {
        let resistance=[1.,1e-6,1e-6,1.];
        let cells: Vec<_> = resistance.iter().enumerate()
            .map(|(i,&r)|Cell::new(vec![(i,-1.),(i+1,1.)],r)).collect();
        let start=Instant::now();let wall=Duration::from_secs(10);
        let budget=AllocationBudget{max_bytes:8*1024*1024,max_nodes:32,max_tetrahedra:32};
        let (_,dc)=correction(&cells,5,&BTreeSet::from([0,4]),0,budget,start,wall,None).unwrap();
        let mut voltage=vec![1.,0.17,0.91,0.07,0.];
        let (current,residual)=complete_harmonic_current(&cells,&mut voltage,&dc,start,wall).unwrap();
        assert!(residual<=RESIDUAL_LIMIT);
        assert_eq!(voltage[0].to_bits(),1f64.to_bits());
        assert_eq!(voltage[4].to_bits(),0f64.to_bits());
        let total=resistance.iter().sum::<f64>();
        for i in 1..4 {
            let expected=1.-resistance[..i].iter().sum::<f64>()/total;
            assert!((voltage[i]-expected).abs()<1e-9);
        }
        assert!((current.iter().zip(&resistance).map(|(q,r)|q*q*r).sum::<f64>()-1.).abs()<1e-12);
        assert!(complete_harmonic_current(&cells,&mut voltage,&dc,start,Duration::ZERO).is_err());
    }

    #[test]
    fn compatible_gradient_spans_and_capacitive_seed_keeps_zero_incidence_modes() {
        let start = Instant::now();
        let wall = Duration::from_secs(10);
        let budget = AllocationBudget {
            max_bytes: 8 * 1024 * 1024,
            max_nodes: 32,
            max_tetrahedra: 32,
        };
        let cells = [Cell::new(vec![], 2.), Cell::new(vec![], 3.)];
        let contacts = BTreeSet::from([0, 1]);
        let islands = [vec![0], vec![1]];
        let (seed, _, _) = harmonic(
            &cells,
            2,
            &contacts,
            Some(&islands),
            budget,
            0,
            start,
            wall,
            0,
            false,
        )
        .unwrap();
        assert!(seed.current.is_empty());
        assert_eq!(seed.voltage, vec![vec![1., 0.], vec![0., 1.]]);
        assert_eq!(lift(&seed.current, &[], cells.len()), vec![0., 0.]);

        // Manufactured positive electric Gram for two isolated electrodes.
        // The algebraic seed has no conductive path and still compiles.
        let charges = [vec![2e-12, -1e-12], vec![-1e-12, 2e-12]];
        let (model, _) = make_model(
            &cells,
            2,
            &seed,
            &["a".into(), "b".into()],
            &charges,
            &[],
            &[[0, 1]],
        )
        .unwrap();
        assert!(model.current_dofs.is_empty());
        assert!(System::compile(&model, 1, None).is_ok());

        let cells = [
            Cell::new(vec![], 2.),
            Cell::new(vec![(0, -1.), (2, 1.)], 3.),
        ];
        let mut basis = Vec::new();
        // A physical current with zero incidence can carry magnetic energy.
        append_current_mode(&cells, &mut basis, vec![1., 0.]).unwrap();
        let bubble = [0., 0., 0.25];
        let gradient = conductive(&cells, &bubble);
        append_current_mode(&cells, &mut basis, gradient.clone()).unwrap();
        assert_eq!(basis.len(), 2);
        let weights: Vec<_> = basis
            .iter()
            .map(|q| sum(q, &resistive_action(&cells, &gradient)))
            .collect();
        let recovered = lift(&basis, &weights, cells.len());
        let error: Vec<_> = gradient.iter().zip(recovered).map(|(a, b)| a - b).collect();
        assert!(
            sum(&error, &resistive_action(&cells, &error))
                <= 1e-24 * sum(&gradient, &resistive_action(&cells, &gradient))
        );
    }
}

#[cfg(test)]
mod native_memory_tests {
    use super::*;
    #[test]
    fn partitioned_plan_omits_only_unused_global_sparse_assembly() {
        let (n,m,v,i,base)=(1000,12000,70,8,12345);
        let global=native_mode_sizes(n,m,v,i,base).unwrap();
        let partitioned=native_partitioned_sizes(n,m,v,i,base).unwrap();
        assert_eq!(global-partitioned,native_global_sparse_bytes(m).unwrap());
        let cell=std::mem::size_of::<Cell>()+4*std::mem::size_of::<(usize,f64)>();
        assert_eq!(partitioned,base+n*1024+m*(cell+32*std::mem::size_of::<f64>())
            +(n*v+m*i)*64+(v+i)*(v+i)*2048);
        assert!(AllocationBudget{max_bytes:partitioned-1,max_nodes:n,max_tetrahedra:m}.check(partitioned).is_err());
        assert!(native_partitioned_sizes(n,usize::MAX,v,i,base).is_err());
    }
    #[test]
    fn floating_constant_modes_keep_voltage_without_reserving_fictitious_currents() {
        let cells=[Cell::new(vec![],2.),Cell::new(vec![],3.)];
        let contacts=BTreeSet::from([0,1]);
        let islands=[vec![0],vec![1]];
        let admitted=native_mode_sizes(2,2,2,0,0).unwrap();
        assert!(admitted<native_sizes(2,2,2,0).unwrap());
        let budget=AllocationBudget{max_bytes:admitted,max_nodes:32,max_tetrahedra:32};
        let (basis,_,_)=harmonic(&cells,2,&contacts,Some(&islands),budget,0,
            Instant::now(),Duration::from_secs(10),0,true).unwrap();
        assert_eq!(basis.voltage,vec![vec![1.,0.],vec![0.,1.]]);
        assert!(basis.current.is_empty());
        assert!(harmonic(&cells,2,&contacts,Some(&islands),budget,0,
            Instant::now(),Duration::from_secs(10),4,true).is_err());
        assert!(native_mode_sizes(2,2,usize::MAX,1,0).is_err());
    }
    #[test]
    fn native_request_buffer_plan_tracks_mode_cap_and_preserves_overflow_refusal() {
        let n = 1000;
        let m = 12000;
        assert!(native_sizes(n, m, 4, 0).unwrap() < sizes(n, m, 4, 0).unwrap());
        assert_eq!(
            native_sizes(n, m, 9, 123).unwrap() - native_sizes(n, m, 4, 123).unwrap(),
            (n + m) * 5 * 64 + (9 * 9 - 4 * 4) * 8192
        );
        assert!(native_sizes(usize::MAX, m, 4, 0).is_err());
        assert!(native_work_bytes(n, usize::MAX, 4).is_err());
        let plan = native_sizes(n, m, 4, 0).unwrap();
        assert!(AllocationBudget {
            max_bytes: plan - 1,
            max_nodes: n,
            max_tetrahedra: m
        }
        .check(plan)
        .is_err());
    }
}

fn compact_magnetic_certificate(cover: &serde_json::Value) -> serde_json::Value {
    let mut value = serde_json::json!({"policy":"certified-adaptive-harmonic-v1"});
    for key in [
        "coverSignature",
        "envelopeSignature",
        "absoluteActionGoals",
        "maximumAbsoluteErrorBounds",
        "chosenOrderHistogram",
        "farClusterPairs",
        "exactPairs",
        "zeroBlocks",
        "activeEnvelopeTerminals",
        "actionWallSeconds",
    ] {
        value[key] = cover[key].clone();
    }
    value
}
#[cfg(test)]
#[path = "adaptive_magnetic_bench.rs"]
mod adaptive_magnetic_bench;
