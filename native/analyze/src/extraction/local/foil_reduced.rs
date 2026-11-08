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

trait FoilField {
    fn apply(&self, v:&[f64]) -> Result<Vec<f64>>;
    fn planned_bytes(&self) -> usize;
}
impl FoilField for Field<'_> {
    fn apply(&self,v:&[f64])->Result<Vec<f64>> { Field::apply(self,v) }
    fn planned_bytes(&self)->usize { ElectricApply::planned_bytes(self) }
}
impl FoilField for super::layered_capacitance::SpatialFieldAction {
    fn apply(&self,v:&[f64])->Result<Vec<f64>> {super::layered_capacitance::SpatialFieldAction::apply(self,v)}
    fn planned_bytes(&self)->usize {super::layered_capacitance::SpatialFieldAction::planned_bytes(self)}
}
struct CompactField<'a> { action:&'a CompactChargeAction, diagonal:Vec<f64>, bytes:usize }
impl FoilField for CompactField<'_> {
    fn apply(&self,v:&[f64])->Result<Vec<f64>> {self.action.capacitance_action(v,&self.diagonal,128)}
    fn planned_bytes(&self)->usize {self.bytes}
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
fn loss_gram_reservation(ports:usize,modes:usize,packages:&[super::checked_packages::Validated],frequencies:usize)->Result<usize>{
    // A serde_json object owns a small B-tree leaf even for two fields; include
    // that allocation, raw complex entries, row vectors and package metadata.
    let output=ports.checked_mul(ports).and_then(|v|v.checked_mul(packages.len()+1)).and_then(|v|v.checked_mul(frequencies)).and_then(|v|v.checked_mul(2048));
    let ids=packages.iter().try_fold(0usize,|sum,p|sum.checked_add(p.id.len())).and_then(|v|v.checked_mul(frequencies));
    let states=ports.checked_mul(modes.checked_add(packages.len()).ok_or_else(||error("MEMORY_LIMIT","loss Gram modes overflow"))?).and_then(|v|v.checked_mul(16)).and_then(|v|v.checked_add(ports.checked_mul(48)?));
    output.and_then(|v|v.checked_add(ids?)).and_then(|v|v.checked_add(states?)).ok_or_else(||error("MEMORY_LIMIT","loss Gram reservation overflow"))
}
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
    current_profiles: Option<&[Vec<f64>]>,
) -> Result<(serde_json::Value, usize)> {
    let n = map.full.potential_count();
    let ni = map.full.current_count();
    let profiles=current_profiles.unwrap_or(&[]);
    if profiles.len()>32 || profiles.iter().any(|v|v.len()!=ni||v.iter().any(|x|!x.is_finite())) {return Err(error("MIXED_FOIL_PROFILE","at most 32 finite full-space current profiles required"));}
    let profile_bytes=profiles.len().checked_mul(ni).and_then(|n|n.checked_mul(std::mem::size_of::<f64>())).and_then(|n|n.checked_add(profiles.len().checked_mul(std::mem::size_of::<Vec<f64>>())?)).ok_or_else(||error("MEMORY_LIMIT","mixed profile plan overflow"))?;
    // Each phase can supply a voltage shape and its conductive current,
    // plus one independent induced-current correction.
    let reserve = 4 * grid.max_iterations;
    let map_retained = map.retained_bytes()?;
    let field_base = map_retained
        .checked_add(profile_bytes).and_then(|n|n.checked_add(ElectricApply::planned_bytes(field)))
        .ok_or_else(|| error("MEMORY_LIMIT", "mixed field plan overflow"))?;
    // Admit all non-modal full-space buffers before constructing the bounded
    // exact magnetic cache. Later factor plans add their actual fill/scratch.
    let buffer_base = native_sizes(n, ni, 0, field_base)?;
    eprintln!("P7_MIXED_MEMORY_STAGE stage=magnetic-admission fullPotentials={n} fullCurrents={ni} mapBytes={} electricPlanBytes={} nativeWorkBytes={} totalBaseBytes={buffer_base} budgetBytes={}",
        map_retained, ElectricApply::planned_bytes(field), native_work_bytes(n, ni, 0)?, budget.max_bytes);
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
        .and_then(|v|v.checked_add(reserve)).and_then(|v|v.checked_add(profiles.len())).ok_or_else(||error("MEMORY_LIMIT","native current mode bound overflow"))?;
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
    // Hints live in original physical current coordinates. Admit them only
    // after the full DC reset, using the same R metric as residual enrichment.
    let before_profiles=basis.current.len();
    for profile in profiles {check_time(start,wall)?;append_current_mode(&cells,&mut basis.current,profile.clone())?;}
    let added_profiles=basis.current.len()-before_profiles;
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
        .checked_add(profile_bytes).and_then(|n|n.checked_add(magnetic.owned_bytes))
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
        false,
    )?;
    checked["sectionHints"]=serde_json::json!({"submitted":profiles.len(),"addedCurrentModes":added_profiles});
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
    budget.check(sizes(n, 2 * supports, 0, ElectricApply::planned_bytes(&field))?)?;
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
        ElectricApply::planned_bytes(&field),
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
            false,
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
    make_model_matrices(cells, n, basis, names, ports, capacitance_f, inductance_h, resistance_ohm).map(|model| (model, max_electric_reciprocity))
}

fn make_model_matrices(cells: &[Cell], n: usize, basis: &Basis, names: &[String], ports: &[[usize; 2]], capacitance_f: Vec<(usize,usize,f64)>, inductance_h: Vec<(usize,usize,f64)>, resistance_ohm: Vec<(usize,usize,f64)>) -> Result<Model> {
    let nv = basis.voltage.len();
    let ni = basis.current.len();
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
    Ok(model)
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
        field: &dyn FoilField,
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
        field: &dyn FoilField,
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

// Conjugate the first current: this convention makes d^H G d the loss
// for a complex vector of Norton drive amplitudes.
fn loss_gram_term(x: [f64; 2], y: [f64; 2], resistance: f64) -> [f64; 2] {
    [resistance * (x[0]*y[0] + x[1]*y[1]),
     resistance * (x[0]*y[1] - x[1]*y[0])]
}

#[cfg(test)]
mod loss_gram_tests {
    use super::loss_gram_term;

    #[test]
    fn complex_drives_include_interference_and_are_hermitian() {
        let x = [1., 1.];
        let y = [2., -1.];
        let xy = loss_gram_term(x, y, 3.);
        let yx = loss_gram_term(y, x, 3.);
        assert_eq!(xy, [3., -9.]);
        assert_eq!(yx, [xy[0], -xy[1]]);
        let xx = loss_gram_term(x, x, 3.);
        let yy = loss_gram_term(y, y, 3.);
        // Drives [1, i] produce current x + i*y = 2 + 3i.
        let power = xx[0] + yy[0] - 2. * xy[1];
        assert_eq!(power, 39.);
        assert_ne!(power, xx[0] + yy[0]);
    }
}

impl ReducedSpace {
    fn train_grid(
        &mut self,
        grid: &MatchedGrid<'_>,
        names: &[String],
        dc: &DcCorrection,
        field: &dyn FoilField,
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
        loss_grams: bool,
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
                let gram_reserve = if loss_grams {
                    if ports > 16 { return Err(error("MEMORY_LIMIT", "loss Gram port limit exceeded")); }
                    let entries=loss_gram_reservation(ports,self.basis.current.len(),packages,grid.frequencies.len())?;
                    budget.check(entries)?;
                    entries
                } else { 0 };
                let available = budget
                    .max_bytes
                    .saturating_sub(field.planned_bytes())
                    .saturating_sub(dc.owned_bytes)
                    .checked_sub(gram_reserve).ok_or_else(|| error("MEMORY_LIMIT", "loss Gram reservation exceeds budget"))?;
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
                    let mut gram_copper: Vec<Vec<[f64; 2]>> = Vec::with_capacity(if loss_grams { ports } else { 0 });
                    let mut gram_packages: Vec<Vec<[f64; 2]>> = Vec::with_capacity(if loss_grams { ports } else { 0 });
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
                        if loss_grams {
                            let nv = self.basis.voltage.len();
                            gram_copper.push(copper_re[nv..].iter().zip(&copper_im[nv..]).map(|(&r,&i)| [r,i]).collect());
                            gram_packages.push(package_samples.iter().map(|s| s.current()).collect());
                        }
                        columns.push(column);
                    }
                    let mut sample = json!({"frequencyHz":frequency,"columns":columns});
                    if loss_grams && gram_copper.len() == ports {
                        let mut copper = vec![vec![[0.; 2]; ports]; ports];
                        let mut package_grams = vec![vec![vec![[0.; 2]; ports]; ports]; packages.len()];
                        for i in 0..ports { for j in 0..ports {
                            for &(a,b,r) in &self.model.resistance_ohm {
                                let x = gram_copper[i][a]; let y = gram_copper[j][b];
                                let term = loss_gram_term(x, y, r);
                                copper[i][j][0] += term[0];
                                copper[i][j][1] += term[1];
                            }
                            for (k,p) in packages.iter().enumerate() {
                                let x=gram_packages[i][k]; let y=gram_packages[j][k];
                                package_grams[k][i][j] = loss_gram_term(x, y, p.r);
                            }
                        }}
                        let as_json = |g: &Vec<Vec<[f64; 2]>>| -> Value {
                            Value::Array(g.iter().map(|row| Value::Array(row.iter().map(|v| json!({"real":v[0],"imaginary":v[1]})).collect())).collect())
                        };
                        for i in 0..ports { for j in 0..ports {
                            let mut actual = copper[i][j];
                            for g in &package_grams { actual[0]+=g[i][j][0]; actual[1]+=g[i][j][1]; }
                            let mut expected = [if i == j {1.} else {0.}, 0.];
                            for row in &matrix {
                                let a=&row[i]; let b=&row[j];
                                let ar=a["real"].as_f64().unwrap(); let ai=a["imaginary"].as_f64().unwrap();
                                let br=b["real"].as_f64().unwrap(); let bi=b["imaginary"].as_f64().unwrap();
                                expected[0]-=ar*br+ai*bi; expected[1]-=ar*bi-ai*br;
                            }
                            let scale=(grid.references[i].sqrt()/2.)*(grid.references[j].sqrt()/2.);
                            for part in 0..2 {
                                expected[part]*=scale;
                                if !actual[part].is_finite() || (actual[part]-expected[part]).abs() > 1e-12+1e-8*(actual[part].abs()+expected[part].abs()) {
                                    return Err(error("BOARD_FOIL_POWER", "loss Gram power balance failed"));
                                }
                            }
                            for g in std::iter::once(&copper).chain(package_grams.iter()) {
                                if !g[i][j][0].is_finite() || !g[i][j][1].is_finite() ||
                                    (g[i][j][0]-g[j][i][0]).abs() > 1e-12+1e-8*(g[i][j][0].abs()+g[j][i][0].abs()) ||
                                    (g[i][j][1]+g[j][i][1]).abs() > 1e-12+1e-8*(g[i][j][1].abs()+g[j][i][1].abs()) {
                                    return Err(error("BOARD_FOIL_POWER", "invalid Hermitian loss Gram"));
                                }
                            }
                        }}
                        for i in 0..ports {
                            let original=sample["columns"][i]["copperLossW"].as_f64().unwrap();
                            if (copper[i][i][0]-original).abs() > 1e-12+1e-8*(copper[i][i][0].abs()+original.abs()) { return Err(error("BOARD_FOIL_POWER", "copper loss Gram diagonal mismatch")); }
                            for (k,g) in package_grams.iter().enumerate() {
                                let original=sample["columns"][i]["packages"][k]["lossW"].as_f64().unwrap();
                                if (g[i][i][0]-original).abs() > 1e-12+1e-8*(g[i][i][0].abs()+original.abs()) { return Err(error("BOARD_FOIL_POWER", "package loss Gram diagonal mismatch")); }
                            }
                        }
                        sample["lossGrams"] = json!({"copperWPerA2":as_json(&copper),"packages":packages.iter().enumerate().map(|(k,p)| json!({"id":p.id,"powerWPerA2":as_json(&package_grams[k])})).collect::<Vec<_>>()});
                    }
                    parameters.push(matrix);
                    samples.push(sample);
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
        "batchColumns",
        "chunkCount",
        "coverAggregation",
        "chunkCertificates",
        "coverSignature",
        "envelopeSignature",
        "absoluteActionGoals",
        "maximumAbsoluteErrorBounds",
        "chosenOrderHistogram",
        "farClusterPairs",
        "exactPairs",
        "zeroBlocks",
        "boundedOmittedBlocks",
        "nearBoundEnabled",
        "nearBoundPolicy",
        "nearQuadraturePolicy",
        "nearQuadratureErrorCertified",
        "activeEnvelopeTerminals",
        "actionWallSeconds",
    ] {
        if let Some(item) = cover.get(key) { value[key] = item.clone(); }
    }
    value
}
#[test]
fn compact_cover_keeps_optional_partition_metadata_absent_or_complete() {
    let common=serde_json::json!({"coverSignature":"fnv1a64:0123456789abcdef"});
    let compact=compact_magnetic_certificate(&common);
    assert!(compact.get("batchColumns").is_none());
    assert!(compact.get("chunkCertificates").is_none());
    assert!(compact.get("nearBoundPolicy").is_none());
    assert!(compact.get("nearQuadraturePolicy").is_none());
    let mut chunked=common;
    chunked["batchColumns"]=serde_json::json!(1);
    chunked["chunkCount"]=serde_json::json!(2);
    chunked["coverAggregation"]=serde_json::json!("sum-of-sequential-chunk-covers");
    chunked["chunkCertificates"]=serde_json::json!([{"rhsColumns":1},{"rhsColumns":1}]);
    chunked["nearBoundEnabled"]=serde_json::json!(true);
    chunked["nearBoundPolicy"]=serde_json::json!("summed-per-tetra-volume-v1");
    chunked["boundedOmittedBlocks"]=serde_json::json!(2);
    chunked["nearQuadraturePolicy"]=serde_json::json!("axial-tetra-estimated-v1");
    chunked["nearQuadratureErrorCertified"]=serde_json::json!(false);
    let compact=compact_magnetic_certificate(&chunked);
    for key in ["batchColumns","chunkCount","coverAggregation","chunkCertificates","nearBoundEnabled","nearBoundPolicy","boundedOmittedBlocks"] {
        assert_eq!(compact[key],chunked[key]);
    }
    for key in ["nearQuadraturePolicy","nearQuadratureErrorCertified"] {assert_eq!(compact[key],chunked[key]);}
}
#[cfg(test)]
#[path = "adaptive_magnetic_bench.rs"]
mod adaptive_magnetic_bench;

/// Convert a compact charge-potential operator into potential-coordinate
/// capacitance. `dual[a][i]` is the potential test for charge mode a at
/// potential coordinate i. No artificial gauge is imposed: the free-space
/// potential matrix must be positive definite on the supplied charge modes.
pub(super) fn compact_capacitance(p: &[Vec<f64>], dual: &[Vec<f64>], budget: AllocationBudget) -> Result<Vec<Vec<f64>>> {
    let n = p.len();
    let m = dual.first().map_or(0, Vec::len);
    if n == 0 || m == 0 || dual.len() != n || p.iter().any(|r| r.len() != n) || dual.iter().any(|r| r.len() != m) || p.iter().flatten().chain(dual.iter().flatten()).any(|v| !v.is_finite()) {
        return Err(error("HYBRID_CHARGE", "invalid compact charge operator or dual mapping"));
    }
    let bytes = n.checked_mul(n).and_then(|v| v.checked_add(n.checked_mul(m)?)).and_then(|v| v.checked_add(m.checked_mul(m)?)).and_then(|v| v.checked_mul(8)).ok_or_else(|| error("MEMORY_LIMIT", "compact charge factor size overflow"))?;
    budget.check(bytes)?;
    let mut l = vec![vec![0.; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let scale = p[i][j].abs().max(p[j][i].abs());
            if (p[i][j] - p[j][i]).abs() > 1e-10 * scale.max(f64::MIN_POSITIVE) {
                return Err(error("HYBRID_CHARGE", "potential operator is not reciprocal"));
            }
            let mut value = 0.5 * (p[i][j] + p[j][i]);
            for k in 0..j { value -= l[i][k] * l[j][k]; }
            if i == j {
                if !value.is_finite() || value <= 0. { return Err(error("HYBRID_CHARGE", "dependent charge modes or nonpositive potential operator")); }
                l[i][j] = value.sqrt();
            } else { l[i][j] = value / l[j][j]; }
        }
    }
    // B^T P^-1 B = (L^-1 B)^T (L^-1 B), preserving reciprocity.
    let mut x = vec![vec![0.; m]; n];
    for i in 0..n { for j in 0..m {
        let mut value = dual[i][j];
        for k in 0..i { value -= l[i][k] * x[k][j]; }
        x[i][j] = value / l[i][i];
        if !x[i][j].is_finite() { return Err(error("HYBRID_CHARGE", "charge solve overflow")); }
    }}
    let mut c = vec![vec![0.; m]; m];
    for i in 0..m { for j in 0..=i {
        let value = (0..n).fold(Sum::default(), |s,k| s.add(Sum::from(x[k][i]).mul(x[k][j]))).value();
        if !value.is_finite() { return Err(error("HYBRID_CHARGE", "capacitance overflow")); }
        c[i][j] = value; c[j][i] = value;
    }}
    Ok(c)
}

/// Construct a compact, homogeneous-dielectric candidate. This uses the
/// existing conservative mixed incidence and contact-preserving harmonic
/// modes; physical approximation and frequency coverage remain unverified.
pub(super) fn compact_planar_model(
    map: &super::mixed_conductor::MixedMap<'_>, names: &[String], ports: &[[usize; 2]],
    epsilon: f64, layered_c: Option<&[Vec<f64>]>, budget: AllocationBudget, start: Instant, wall: Duration,
) -> Result<(Model, serde_json::Value, usize)> {
    let n = map.potential_count();
    if names.len() != n { return Err(error("HYBRID_INPUT", "mixed potential names required")); }
    let mut terms = vec![Vec::new(); map.current_count()];
    for &(i,j,a) in &map.incidence { terms[j].push((i,a)); }
    let cells: Vec<_> = terms.into_iter().zip(&map.resistance_ohm).map(|(t,&r)| Cell::new(t,r)).collect();
    let full_mixed = std::env::var_os("P7_COMPACT_FULL_MIXED").as_deref() == Some(std::ffi::OsStr::new("1"));
    let basis = if full_mixed {
        let ni = map.current_count();
        if n > 1024 || ni > 1024 { return Err(error("HYBRID_FULL_MIXED_LIMIT", "full mixed prototype exceeds coordinate limit")); }
        let bytes = n.checked_mul(n).and_then(|x| x.checked_add(ni.checked_mul(ni)?)).and_then(|x| x.checked_mul(8)).and_then(|x| x.checked_add(map.retained_bytes().ok()?)).ok_or_else(|| error("MEMORY_LIMIT", "full mixed basis size overflow"))?;
        budget.check(bytes)?;
        let voltage = (0..n).map(|i| (0..n).map(|j| if i==j {1.} else {0.}).collect()).collect();
        let current = (0..ni).map(|i| (0..ni).map(|j| if i==j {1.} else {0.}).collect()).collect();
        Basis { voltage, current, retained:(0..n).collect(), components:map.potential_components.clone(), dc_residual:0. }
    } else {
        harmonic(&cells, n, &map.electrodes.iter().copied().collect(), Some(&map.potential_components), budget, map.retained_bytes()?, start, wall, 0, true)?.0
    };
    // Reserve the entire simultaneous dense working set before constructing
    // support densities or dual rows. No full-board automatic admission.
    let nq = (if full_mixed { n } else { map.foils.len().checked_add(map.owners.iter().filter(|o| matches!(o, super::mixed_conductor::Owner::Volume)).count()).ok_or_else(||error("MEMORY_LIMIT", "support count overflow"))? }).checked_add(0).ok_or_else(|| error("MEMORY_LIMIT", "compact support count overflow"))?;
    let nc = basis.current.len(); let nv = basis.voltage.len();
    let resident = map.retained_bytes()?.checked_add(n.checked_mul(nv).and_then(|v| v.checked_mul(8)).ok_or_else(|| error("MEMORY_LIMIT", "compact basis overflow"))?).and_then(|x| x.checked_add(map.current_count().checked_mul(nc)?.checked_mul(8)?)).and_then(|x| x.checked_add(nq.checked_mul(n)?.checked_mul(8)?)).and_then(|x| x.checked_add(nq.checked_mul(if full_mixed {map.foils.len().checked_mul(6)?} else {map.foils.len()})?.checked_mul(8)?)).and_then(|x| x.checked_add((if full_mixed {map.foils.len().checked_mul(6)?} else {map.foils.len()}).checked_mul(nc)?.checked_mul(24)?)).and_then(|x| x.checked_add(nq.checked_add(map.foils.len().checked_mul(if full_mixed {6} else {1})?)?.checked_mul(1024)?)).ok_or_else(|| error("MEMORY_LIMIT", "compact resident size overflow"))?;
    let scratch = nq.checked_mul(nq).and_then(|x| x.checked_mul(32)).and_then(|x| x.checked_add(nq.checked_mul(nv)?.checked_mul(24)?)).and_then(|x| x.checked_add(nc.checked_mul(nc)?.checked_mul(32)?)).and_then(|x| x.checked_add(nv.checked_mul(nv)?.checked_mul(32)?)).and_then(|x| x.checked_add(1024*1024)).ok_or_else(|| error("MEMORY_LIMIT", "compact scratch size overflow"))?;
    let local_subcells = map.owners.iter().filter(|o|matches!(o,super::mixed_conductor::Owner::Volume)).count().checked_mul(if full_mixed {24} else {1}).ok_or_else(||error("MEMORY_LIMIT","local support count overflow"))?;
    let local_reserve = local_subcells.checked_mul(nc.checked_mul(24).and_then(|n|n.checked_add(512)).ok_or_else(||error("MEMORY_LIMIT","local support size overflow"))?).ok_or_else(||error("MEMORY_LIMIT","local support size overflow"))?;
    let scratch = scratch.checked_add(local_reserve).ok_or_else(||error("MEMORY_LIMIT","local scratch overflow"))?;
    let sparse_reserve = (if full_mixed {map.foils.len().checked_mul(6)} else {Some(map.foils.len())}).and_then(|x| x.checked_mul(nc.checked_mul(32)?.checked_add(16)?)).ok_or_else(|| error("MEMORY_LIMIT", "sparse support size overflow"))?;
    let scratch = scratch.checked_add(sparse_reserve).ok_or_else(|| error("MEMORY_LIMIT", "compact scratch overflow"))?;
    budget.check(resident.checked_add(scratch).ok_or_else(|| error("MEMORY_LIMIT", "compact peak overflow"))?)?;
    let work_budget = AllocationBudget { max_bytes: budget.max_bytes - resident, ..budget };
    let (supports, locals, dual) = if full_mixed { map.planar_barycentric_supports(&basis.current)? } else { map.planar_supports(&basis.current)? };
    let remaining = wall.checked_sub(start.elapsed()).ok_or_else(|| error("DEADLINE", "compact planar assembly"))?;
    let sparse: Vec<_> = supports.iter().map(|s| super::planar_operator::SparseSupport {
        prism:s.prism.clone(),
        charge_density:if layered_c.is_some() {vec![]} else {s.charge_density.iter().enumerate().filter_map(|(i,&v)| (v!=0.).then_some((i,v))).collect()},
        current_density:s.current_density.iter().enumerate().filter_map(|(i,&v)| (v!=[0.;3]).then_some((i,v))).collect(),
    }).collect();
    let mut gram = super::planar_operator::gram_sparse(&sparse, if layered_c.is_some() {0} else {dual.len()}, basis.current.len(), 8, epsilon, work_budget, remaining)?;
    drop(sparse);
    let nc = basis.current.len();
    let mut mixed_change = 0_f64; let mut self_change = 0_f64;
    for (t, local) in locals.iter().enumerate() {
        let q = local.charge_index;
        for support in &supports {
            check_time(start, wall)?;
            if layered_c.is_some() && nc == 0 {continue;}
            let planar_charge = if layered_c.is_some() {vec![]} else {vec![support.charge_density.iter().copied().find(|&v|v!=0.).ok_or_else(|| error("HYBRID_CHARGE", "missing support charge"))?]};
            let local_charge = if layered_c.is_some() {vec![]} else {vec![local.charge_density]};
            let block = super::planar_mixed::cross(&support.prism, local.tetra,
                super::planar_mixed::Densities { charge: &planar_charge, current: &support.current_density },
                super::planar_mixed::Densities { charge: &local_charge, current: &local.currents },
                epsilon, 8, AllocationBudget{max_bytes:2*1024*1024,..work_budget}, wall.saturating_sub(start.elapsed()))?;
            if let Some(change)=block.relative_order_change {mixed_change = mixed_change.max(change);}
            if layered_c.is_none() {
                let charge_index = support.charge_density.iter().position(|&v|v!=0.).ok_or_else(||error("HYBRID_CHARGE", "missing support charge"))?;
                gram.potential_per_farad[charge_index][q] += block.potential_per_farad[0][0];
                gram.potential_per_farad[q][charge_index] += block.potential_per_farad[0][0];
            }
            for a in 0..nc { for b in 0..nc { gram.inductance_h[a][b] += block.inductance_h[a][b] + block.inductance_h[b][a]; }}
        }
        for (u, other) in locals[..=t].iter().enumerate() {
            check_time(start, wall)?;
            if layered_c.is_some() && nc == 0 {continue;}
            let integral = if t == u {
                let coarse = polyhedron::tetra_self_integral(local.tetra, 12)?;
                let fine = polyhedron::tetra_self_integral(local.tetra, 16)?;
                let change = (fine-coarse).abs()/fine.abs();
                if !change.is_finite() || change > 1e-8 { return Err(error("HYBRID_UNRESOLVED", "tetra self order estimator exceeds tolerance")); }
                self_change = self_change.max(change); fine
            } else {
                polyhedron::tetra_integral_adaptive(local.tetra, &PointPotential::tetrahedron(other.tetra)?.with_tighter_tetra_tolerance(1e-12)?, 16, 1e-10)?
            };
            check_time(start, wall)?;
            if layered_c.is_none() {
                let p = integral * local.charge_density * other.charge_density / (4. * std::f64::consts::PI * epsilon);
                gram.potential_per_farad[q][other.charge_index] += p;
                if t != u { gram.potential_per_farad[other.charge_index][q] += p; }
            }
            for a in 0..nc { for b in 0..nc {
                let dot: f64 = (0..3).map(|k| local.currents[a][k]*other.currents[b][k]).sum();
                let value = 1e-7 * integral * dot;
                gram.inductance_h[a][b] += value;
                if t != u { gram.inductance_h[b][a] += value; }
            }}
        }
    }
    let capacitance_f = if let Some(c) = layered_c {
        if c.len()!=n || c.iter().any(|row| row.len()!=n || row.iter().any(|x| !x.is_finite())) { return Err(error("HYBRID_CHARGE", "invalid layered trace matrix")); }
        let actions: Vec<Vec<f64>> = basis.voltage.iter().map(|v| c.iter().map(|row| sum(row,v)).collect()).collect();
        let (entries, _) = congruence(&basis.voltage, &actions, 1e-9, "layered C")?;
        let mut matrix=vec![vec![0.;basis.voltage.len()];basis.voltage.len()];
        for (i,j,x) in entries { matrix[i][j]=x; } matrix
    } else {
        let reduced_dual: Vec<Vec<f64>> = dual.iter().map(|row| basis.voltage.iter().map(|v| sum(row, v)).collect()).collect();
        compact_capacitance(&gram.potential_per_farad, &reduced_dual, work_budget)?
    };
    // Every retained voltage mode must be visible to the charge tests.
    // A singular reduced C means the centroid-like dual lost a mode.
    let nv = basis.voltage.len();
    let identity: Vec<Vec<f64>> = (0..nv).map(|i| (0..nv).map(|j| if i == j {1.} else {0.}).collect()).collect();
    let _ = compact_capacitance(&capacitance_f, &identity, work_budget)?;
    let resistance_actions: Vec<Vec<f64>> = basis.current.iter().map(|q| resistive_action(&cells, q)).collect();
    let (resistance_ohm, _) = congruence(&basis.current, &resistance_actions, 1e-9, "R")?;
    let entries = |matrix: Vec<Vec<f64>>| -> Vec<(usize,usize,f64)> { matrix.into_iter().enumerate().flat_map(|(i,row)| row.into_iter().enumerate().filter_map(move |(j,v)| (v != 0.).then_some((i,j,v)))).collect() };
    let evidence = serde_json::json!({"scope":"compact-discrete", "originalVolumeActions":0, "planarMaxOrder":gram.max_order, "planarMaxRelativeChange":gram.max_relative_change, "mixedMaxRelativeChange":mixed_change, "tetraSelfMaxRelativeChange":self_change, "tetraPairAdaptiveTolerance":1e-10, "physicalAccuracy":"unverified", "basis":if full_mixed {"full-mixed"} else {"dc-harmonic-only"}, "approximationError":"unverified"});
    let model = make_model_matrices(&cells, n, &basis, names, ports, entries(capacitance_f), entries(gram.inductance_h), resistance_ohm)?;
    Ok((model, evidence, resident+scratch))
}

pub(super) fn compact_planar_response(model: &Model, frequencies: &[f64], references: &[f64], packages: &[super::checked_packages::Validated], budget: AllocationBudget, start: Instant, wall: Duration) -> Result<serde_json::Value> {
    let b = Budget { max_memory_mi_b: Some(budget.max_bytes as f64 / 1048576.), max_seconds: Some(wall.saturating_sub(start.elapsed()).as_secs_f64()) };
    let (mut system, n, recovery) = if packages.is_empty() {
        (System::compile(model, references.len(), Some(&b))?, model.potential_nodes.len()+model.current_dofs.len(), None)
    } else {
        let joint = super::checked_packages::compile(model, packages, references.len(), &b)?;
        (joint.system, joint.size, Some(joint.recovery))
    };
    for (p, r) in references.iter().enumerate() { system.add_port_admittance(p, 1. / r, 0.)?; }
    if frequencies.is_empty() || references.is_empty() || frequencies.iter().any(|f| !f.is_finite() || *f <= 0.) || references.iter().any(|r| !r.is_finite() || *r <= 0.) { return Err(error("HYBRID_INPUT", "positive finite frequencies and references required")); }
    let mut samples = Vec::new();
    let mut parameters = Vec::new();
    let (mut max_kcl, mut max_kvl, mut max_power) = (0_f64, 0_f64, 0_f64);
    for &frequency in frequencies {
        check_time(start,wall)?;
        let mut matrix = vec![vec![serde_json::Value::Null; references.len()]; references.len()];
        let mut columns = Vec::new();
        for input in 0..references.len() {
            let mut drive = vec![0.; references.len()]; drive[input] = 1.;
            let mut rhs = system.port_rhs(&drive); rhs.resize(2*n, 0.);
            let state = system.solve(2.*std::f64::consts::PI*frequency, rhs, &vec![0.;n], &mut Counters::default())?;
            let re = system.port_voltages(&state[..n], references.len());
            let im = system.port_voltages(&state[n..], references.len());
            let package_samples = recovery.as_ref().map(|r| r.package_states(2.*std::f64::consts::PI*frequency, &state[..n], &state[n..])).transpose()?.unwrap_or_default();
            let recovered = recovery.as_ref().map(|r| r.copper_state(&state[..n], &state[n..]));
            let copper_n = model.potential_nodes.len() + model.current_dofs.len();
            let mut copper_state = vec![0.; 2*copper_n];
            if let Some(c) = &recovered { copper_state[..copper_n].copy_from_slice(&c[0]); copper_state[copper_n..].copy_from_slice(&c[1]); } else { copper_state.copy_from_slice(&state); }
            let mut net_r: Vec<f64> = drive.iter().zip(&re).zip(references).map(|((&d,&v),&r)|d-v/r).collect();
            let mut net_i: Vec<f64> = im.iter().zip(references).map(|(&v,&r)|-v/r).collect();
            for package in &package_samples { let [r,i]=package.current(); net_r.push(-r); net_i.push(-i); }
            // Audit the assembled compact equations for each excitation.
            let n = copper_n; let state = &copper_state;
            let nv = model.potential_nodes.len();
            let mut rr = vec![0.; 2*n];
            let mut scales = vec![0.; 2*n];
            let mut add = |row: usize, value: f64| { rr[row] += value; scales[row] += value.abs(); };
            let w = 2.*std::f64::consts::PI*frequency;
            for &(i,j,a) in &model.incidence { for part in 0..2 { add(part*n+i, a*state[part*n+nv+j]); add(part*n+nv+j, -a*state[part*n+i]); }}
            for &(i,j,a) in &model.capacitance_f { add(i,-w*a*state[n+j]); add(n+i,w*a*state[j]); }
            for &(i,j,a) in &model.resistance_ohm { for part in 0..2 { add(part*n+nv+i,a*state[part*n+nv+j]); }}
            for &(i,j,a) in &model.inductance_h { add(nv+i,-w*a*state[n+nv+j]); add(n+nv+i,w*a*state[nv+j]); }
            for &(i,p,a) in &model.port_incidence { add(i,-a*net_r[p]); add(n+i,-a*net_i[p]); }
            drop(add);
            let mut column_kcl = 0_f64; let mut column_kvl = 0_f64;
            for part in 0..2 { for i in 0..n { let v = rr[part*n+i].abs()/scales[part*n+i].max(1e-300); if i<nv {column_kcl=column_kcl.max(v);} else {column_kvl=column_kvl.max(v);} }}
            if !column_kcl.is_finite() || !column_kvl.is_finite() || column_kcl > 1e-8 || column_kvl > 1e-8 { return Err(error("HYBRID_RESIDUAL", &format!("compact KCL={column_kcl:e} KVL={column_kvl:e}"))); }
            max_kcl = max_kcl.max(column_kcl); max_kvl = max_kvl.max(column_kvl);
            let mut outgoing = 0.;
            for output in 0..references.len() {
                let scale = 2. / (references[input].sqrt()*references[output].sqrt());
                let sr = scale*re[output] - if input==output {1.} else {0.};
                let si = scale*im[output];
                if !sr.is_finite() || !si.is_finite() { return Err(error("HYBRID_RANGE", "nonfinite wave")); }
                outgoing += sr*sr + si*si;
                matrix[output][input] = serde_json::json!({"real":sr, "imaginary":si});
            }
            let copper = if let Some(c) = &recovered { super::checked_packages::copper_loss(model,c) } else { system.joule_loss(&state[..n]) + system.joule_loss(&state[n..]) };
            if let Some(c) = &recovered { super::checked_packages::check_copper_power(model,c,[&net_r,&net_i],copper)?; }
            let dissipated = copper + package_samples.iter().map(|p|p.loss_w).sum::<f64>();
            let wave_power = (1. - outgoing) * references[input] / 4.;
            let power_error = (wave_power-dissipated).abs();
            if !copper.is_finite() || copper < 0. || !power_error.is_finite() || power_error > 1e-12 + 1e-8*(wave_power.abs()+dissipated.abs()) { return Err(error("HYBRID_POWER", "compact wave and copper powers disagree")); }
            max_power = max_power.max(power_error);
            columns.push(serde_json::json!({"input":input,"kclRelative":column_kcl,"kvlRelative":column_kvl,"copperLossW":copper,"powerBalanceErrorW":power_error,"packages":package_samples}));
        }
        parameters.push(matrix);
        samples.push(serde_json::json!({"frequencyHz":frequency,"columns":columns}));
    }
    Ok(serde_json::json!({"status":"complete", "coverage":"unverified", "representation":"S", "operator":"compact-planar-homogeneous-prototype", "quality":{"maxKclRelative":max_kcl,"maxKvlRelative":max_kvl,"maxColumnPowerBalanceErrorW":max_power}, "frequenciesHz": frequencies, "referenceOhm": references, "parameters":parameters, "samples":samples, "model":model, "limitations":"No independent physical error, proximity, interface, or frequency-band validation"}))
}

#[cfg(test)]
mod compact_planar_tests {
    use super::*;
    #[test]
    fn compact_geometric_two_supports_two_drives() {
        let budget = AllocationBudget { max_nodes: 100, max_tetrahedra: 100, max_bytes: 32_000_000 };
        let prism = |id: &str, x: f64| super::super::prism::Prism::new(id.into(), [[x,0.],[x+0.001,0.],[x,0.001]], [0.,0.00001]).unwrap();
        let p0 = prism("a", 0.); let p1 = prism("b", 0.003);
        let supports = vec![
            super::super::planar_operator::Support { charge_density: vec![1./p0.volume_m3,0.], current_density: vec![[1e8,0.,0.]], prism:p0 },
            super::super::planar_operator::Support { charge_density: vec![0.,1./p1.volume_m3], current_density: vec![[-1e8,0.,0.]], prism:p1 },
        ];
        let gram = super::super::planar_operator::gram(&supports, 2, 1, 8, super::super::super::EPS0, budget, Duration::from_secs(25)).unwrap();
        let c = compact_capacitance(&gram.potential_per_farad, &[vec![1.,0.],vec![0.,1.]], budget).unwrap();
        let cells = vec![Cell::new(vec![(0,1.),(1,-1.)],1.)];
        let basis = Basis { voltage:vec![vec![1.,0.],vec![0.,1.]], current:vec![vec![1.]], retained:vec![0,1], components:vec![vec![0,1]], dc_residual:0. };
        let names = vec!["a".into(),"b".into()];
        let entries = |m: Vec<Vec<f64>>| m.into_iter().enumerate().flat_map(|(i,r)| r.into_iter().enumerate().map(move |(j,v)|(i,j,v))).collect();
        let make = |ports: &[[usize;2]]| make_model_matrices(&cells,2,&basis,&names,ports,entries(c.clone()),entries(gram.inductance_h.clone()),vec![(0,0,1.)]);
        let model = make(&[[0,1],[1,0]]).unwrap();
        let response = compact_planar_response(&model,&[1000.],&[50.,50.],&[],budget,Instant::now(),Duration::from_secs(5)).unwrap();
        assert_eq!(response["samples"][0]["columns"].as_array().unwrap().len(),2);
        assert!(make(&[[0,2]]).is_err());
    }
    #[test]
    fn compact_response_keeps_ports_and_refuses_lost_contact() {
        let budget = AllocationBudget { max_nodes: 100, max_tetrahedra: 100, max_bytes: 16_000_000 };
        let cells = vec![Cell::new(vec![(0, 1.), (1, -1.)], 1.)];
        let basis = Basis { voltage: vec![vec![1.,0.],vec![0.,1.]], current: vec![vec![1.]], retained: vec![0,1], components: vec![vec![0,1]], dc_residual: 0. };
        let names = vec!["a".into(), "b".into()];
        let make = |ports: &[[usize;2]]| make_model_matrices(&cells, 2, &basis, &names, ports, vec![(0,0,1e-12),(1,1,1e-12)], vec![(0,0,1e-9)], vec![(0,0,1.)]);
        let model = make(&[[0,1], [1,0]]).unwrap();
        let response = compact_planar_response(&model, &[1000.], &[50.,50.], &[], budget, Instant::now(), Duration::from_secs(5)).unwrap();
        assert_eq!(response["coverage"], "unverified");
        assert!(response["parameters"][0][0][0]["real"].as_f64().unwrap().is_finite());
        assert!(response["parameters"][0][1][1]["real"].as_f64().unwrap().is_finite());
        assert!(make(&[[0,2]]).is_err());
    }
    #[test]
    fn compact_action_matches_small_projected_gram() {
        let budget=AllocationBudget{max_nodes:32,max_tetrahedra:32,max_bytes:4_000_000};
        let prism=super::super::prism::Prism::new("p".into(),[[0.,0.],[0.001,0.],[0.,0.001]],[0.,0.00001]).unwrap();
        let supports=vec![super::super::planar_operator::SparseSupport{prism,charge_density:vec![],current_density:vec![(0,[1e8,0.,0.]),(1,[0.,2e8,0.])]}];
        let gram=super::super::planar_operator::gram_sparse(&supports,0,2,8,super::super::super::EPS0,budget,Duration::from_secs(5)).unwrap();
        let x=[0.7,-0.2]; let action=compact_planar_magnetic_action(&supports,&x,budget,Instant::now(),Duration::from_secs(5)).unwrap();
        for i in 0..2 {let expected=gram.inductance_h[i][0]*x[0]+gram.inductance_h[i][1]*x[1];assert!((action[i]-expected).abs()<1e-12*expected.abs().max(1e-30));}
        let prepared=CompactPlanarAction::new(supports,2,budget,Instant::now(),Duration::from_secs(5)).unwrap();
        let (accelerated,bound)=prepared.apply(&x).unwrap();
        for i in 0..2 {assert!((accelerated[i]-action[i]).abs() <= bound[i]+1e-12*action[i].abs().max(1e-30));}
        let (batch,bounds)=prepared.apply_batch(&[x.to_vec(),vec![-x[0],-x[1]]]).unwrap();
        for i in 0..2 {assert!((batch[0][i]-action[i]).abs()<=bounds[0][i]+1e-12*action[i].abs().max(1e-30));assert!((batch[1][i]+action[i]).abs()<=bounds[1][i]+1e-12*action[i].abs().max(1e-30));}
    }
    #[test]
    fn charge_action_and_cg_match_direct_gram() {
        let budget=AllocationBudget{max_nodes:32,max_tetrahedra:32,max_bytes:8_000_000};
        let p=super::super::prism::Prism::new("p".into(),[[0.,0.],[0.001,0.],[0.,0.001]],[0.,0.00001]).unwrap();
        let supports=vec![super::super::planar_operator::SparseSupport{charge_density:vec![(0,1./p.volume_m3)],current_density:vec![],prism:p.clone()}];
        let gram=super::super::planar_operator::gram_sparse(&supports,1,0,8,super::super::super::EPS0,budget,Duration::from_secs(5)).unwrap();
        let plan=super::super::near_far::Plan::new(&[p],0.2,12,super::super::near_far::Budget{max_bytes:4_000_000,max_nodes:32,max_blocks:100,max_pairs:1,max_wall:Duration::from_secs(5)}).unwrap();
        let action=CompactChargeAction{planar:supports,local:vec![],dual:vec![vec![(0,1.)]],plan,epsilon:super::super::super::EPS0,budget,start:Instant::now(),wall:Duration::from_secs(5),pair_cache:std::cell::RefCell::new(BTreeMap::new()),action_count:std::cell::Cell::new(0)};
        let (y,bound)=action.apply(&[2.]).unwrap();
        assert!((y[0]-2.*gram.potential_per_farad[0][0]).abs() <= bound[0]+1e-12*y[0].abs());
        let c=action.capacitance_action(&[1.],&[gram.potential_per_farad[0][0]],4).unwrap();
        assert!((c[0]*gram.potential_per_farad[0][0]-1.).abs()<1e-8);
    }
    #[test]
    fn unified_mixed_action_covers_prism_tetra_and_bounds() {
        assert_eq!(compact_bound_product(1e-300,1e-300),f64::from_bits(1));
        assert_eq!(compact_bound_product(0.,1e-300),0.);
        let budget=AllocationBudget{max_nodes:32,max_tetrahedra:32,max_bytes:16_000_000};
        let prism=super::super::prism::Prism::new("p".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,1.]).unwrap();
        let tetra=[[8.,0.,0.],[9.,0.,0.],[8.,1.,0.],[8.,0.,1.]];
        let supports=vec![super::super::planar_operator::SparseSupport{prism:prism.clone(),charge_density:vec![],current_density:vec![(0,[1.,0.,0.])]}];
        let local=vec![super::super::mixed_conductor::LocalCurrentSupport{tetra,densities:vec![(1,[1.,0.,0.])]}];
        let moments=vec![super::super::far::Moments::new(&prism).unwrap(),super::super::far::Moments::tetrahedron(tetra).unwrap()];
        let started=Instant::now();let wall=Duration::from_secs(20);
        let plan=super::super::near_far::Plan::from_moments(moments,0.01,1,super::super::near_far::Budget{max_bytes:4_000_000,max_nodes:32,max_blocks:32,max_pairs:3,max_wall:wall}).unwrap();
        let pairs=super::super::volume_integral_magnetic::CompactLocalPairs::new(vec![tetra],AllocationBudget{max_bytes:4_000_000,..budget},started,wall).unwrap();
        let mut op=CompactMixedMagnetic{supports,dimensions:2,planar_max_order:std::cell::Cell::new(0),planar_max_change:std::cell::Cell::new(0.),local,local_pairs:Some(pairs),mixed_plan:plan,pl_pool:None,pl_workers:1,cache:std::cell::RefCell::new(BTreeMap::new()),budget,start:started,wall,action_count:std::cell::Cell::new(0),cache_limit:32,batch_limit:1_000_000,pair_count:std::cell::Cell::new(0),promoted_pairs:std::cell::Cell::new(0),mixed_max_change:std::cell::Cell::new(0.),mixed_fallback_count:std::cell::Cell::new(0),self_max_change:std::cell::Cell::new(0.)};
        assert_eq!(op.mixed_plan.near_pairs,3);
        let (a,bounds)=op.apply_batch(&[vec![1.,0.],vec![0.,1.]],Some(&[1.,1.])).unwrap();
        assert!(op.promoted_pairs.get()>0);
        let promoted=op.promoted_pairs.get();
        let (exact,exact_bounds)=op.apply_batch(&[vec![1.,0.],vec![0.,1.]],Some(&[1e-300,1e-300])).unwrap();
        assert_eq!(op.promoted_pairs.get(),promoted);
        assert!(exact_bounds.iter().flatten().all(|&b|b==0.));
        for c in 0..2 {for i in 0..2 {assert!((a[c][i]-exact[c][i]).abs()<=bounds[c][i]+1e-12*exact[c][i].abs().max(1e-30));}}
        let q=[1.];let direct=super::super::planar_mixed::cross(&prism,tetra,super::super::planar_mixed::Densities{charge:&q,current:&[]},super::super::planar_mixed::Densities{charge:&q,current:&[]},1./(4.*std::f64::consts::PI),12,budget,wall).unwrap().potential_per_farad[0][0]*1e-7;
        assert!((a[0][1]-direct).abs()<=bounds[0][1]+1e-10*direct.abs());
        assert!((a[1][0]-direct).abs()<=bounds[1][0]+1e-10*direct.abs());
        assert!((a[0][1]-a[1][0]).abs()<=1e-12*direct.abs());
        // Row 1 is shared by both supports; first column also cancels the
        // two planar density contributions exactly.
        op.supports[0].current_density.push((1,[1.,0.,0.]));
        let columns=[vec![1.,-1.],vec![1.,1.]];
        let (approx,err)=op.apply_batch(&columns,Some(&[1.,1.])).unwrap();
        let (exact,exact_err)=op.apply_batch(&columns,Some(&[1e-300,1e-300])).unwrap();
        assert!(exact_err.iter().flatten().all(|&b|b==0.));
        for c in 0..2 {for i in 0..2 {assert!((approx[c][i]-exact[c][i]).abs()<=err[c][i]+1e-12*exact[c][i].abs().max(1e-30));}}
        op.cache.borrow_mut().clear();
        op.pl_pool=Some(rayon::ThreadPoolBuilder::new().num_threads(2).stack_size(2*1024*1024).build().unwrap());op.pl_workers=2;
        let (parallel,parallel_err)=op.apply_batch(&columns,Some(&[1e-300,1e-300])).unwrap();
        for c in 0..2 {for i in 0..2 {assert_eq!(parallel[c][i].to_bits(),exact[c][i].to_bits());assert_eq!(parallel_err[c][i].to_bits(),exact_err[c][i].to_bits());}}

    }
    #[test]
    fn parallel_pl_two_pairs_preserves_order_cache_and_bits() {
        let budget=AllocationBudget{max_nodes:32,max_tetrahedra:32,max_bytes:16_000_000};
        let prism=super::super::prism::Prism::new("p".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,1.]).unwrap();
        let t0=[[4.,0.,0.],[5.,0.,0.],[4.,1.,0.],[4.,0.,1.]];
        let t1=[[7.,0.,0.],[8.,0.,0.],[7.,1.,0.],[7.,0.,1.]];
        let supports=vec![super::super::planar_operator::SparseSupport{prism:prism.clone(),charge_density:vec![],current_density:vec![(0,[1.,0.,0.]),(1,[0.5,0.,0.])]}];
        let local=vec![super::super::mixed_conductor::LocalCurrentSupport{tetra:t0,densities:vec![(1,[1.,0.,0.])]},super::super::mixed_conductor::LocalCurrentSupport{tetra:t1,densities:vec![(1,[-0.5,0.,0.])]}];
        let moments=vec![super::super::far::Moments::new(&prism).unwrap(),super::super::far::Moments::tetrahedron(t0).unwrap(),super::super::far::Moments::tetrahedron(t1).unwrap()];
        let start=Instant::now();let wall=Duration::from_secs(20);
        let plan=super::super::near_far::Plan::from_moments(moments,0.2,1,super::super::near_far::Budget{max_bytes:4_000_000,max_nodes:32,max_blocks:32,max_pairs:6,max_wall:wall}).unwrap();
        let pairs=super::super::volume_integral_magnetic::CompactLocalPairs::new(vec![t0,t1],AllocationBudget{max_bytes:4_000_000,..budget},start,wall).unwrap();
        let mut op=CompactMixedMagnetic{supports,dimensions:2,planar_max_order:std::cell::Cell::new(0),planar_max_change:std::cell::Cell::new(0.),local,local_pairs:Some(pairs),mixed_plan:plan,pl_pool:None,pl_workers:1,cache:std::cell::RefCell::new(BTreeMap::new()),budget,start,wall,action_count:std::cell::Cell::new(0),cache_limit:32,batch_limit:1_000_000,pair_count:std::cell::Cell::new(0),promoted_pairs:std::cell::Cell::new(0),mixed_max_change:std::cell::Cell::new(0.),mixed_fallback_count:std::cell::Cell::new(0),self_max_change:std::cell::Cell::new(0.)};
        let jobs=[(1,0),(0,2)];let density=vec![vec![[1.,0.,0.],[2.,0.,0.],[-1.,0.,0.]],vec![[-1.,0.,0.],[0.5,0.,0.],[3.,0.,0.]]];
        let mut serial=vec![vec![0.;2];2];
        for &(a,b) in &jobs {let value=op.near_pair(a,b).unwrap();for c in 0..2 {for &(i,d) in op.densities(a) {serial[c][i]+=1e-7*value*(0..3).map(|k|d[k]*density[c][b][k]).sum::<f64>();}for &(i,d) in op.densities(b) {serial[c][i]+=1e-7*value*(0..3).map(|k|d[k]*density[c][a][k]).sum::<f64>();}}}
        assert_eq!(op.pair_count.get(),2);assert_eq!(op.cache.borrow().len(),2);
        op.cache.borrow_mut().clear();op.pair_count.set(0);
        op.pl_pool=Some(rayon::ThreadPoolBuilder::new().num_threads(2).stack_size(2*1024*1024).build().unwrap());op.pl_workers=2;
        let mut queued=jobs.to_vec();let mut parallel=vec![vec![0.;2];2];op.flush_pl(&mut queued,&density,&mut parallel).unwrap();
        assert!(queued.is_empty());assert_eq!(op.pair_count.get(),2);assert_eq!(op.cache.borrow().len(),2);
        for c in 0..2 {for i in 0..2 {assert_eq!(parallel[c][i].to_bits(),serial[c][i].to_bits());}}
        let before=op.pair_count.get();op.near_pair(0,1).unwrap();assert_eq!(op.pair_count.get(),before);
    }
    #[test]
    fn parallel_pl_worker_reservation_refuses_small_budget() {
        let budget=AllocationBudget{max_nodes:32,max_tetrahedra:32,max_bytes:19*1024*1024};
        assert!(compact_pl_worker_bytes(4,budget).is_err());
        assert!(compact_pl_worker_bytes(2,budget).is_ok());
        assert!(compact_pl_worker_bytes(5,budget).is_err());
    }
    #[test]
    fn tetra_vertex_dual_retains_four_coordinates() {
        let budget=AllocationBudget{max_nodes:32,max_tetrahedra:32,max_bytes:1_000_000};
        let p:Vec<Vec<f64>>=(0..4).map(|i|(0..4).map(|j|if i==j {1.} else {0.}).collect()).collect();
        let b:Vec<Vec<f64>>=(0..4).map(|i|(0..4).map(|j|if i==j {25./48.} else {23./144.}).collect()).collect();
        for row in &b { assert!((row.iter().sum::<f64>()-1.).abs()<1e-14); }
        let c=compact_capacitance(&p,&b,budget).unwrap();
        compact_capacitance(&c,&p,budget).unwrap();
    }
    #[test]
    fn charge_inverse_preserves_dual_and_refuses_dependent_modes() {
        let budget = AllocationBudget { max_nodes: 100, max_tetrahedra: 100, max_bytes: 1_000_000 };
        let p = vec![vec![2., 0.], vec![0., 4.]];
        let dual = vec![vec![1., 0.], vec![0., 1.]];
        let c = compact_capacitance(&p, &dual, budget).unwrap();
        assert!((c[0][0] - 0.5).abs() < 1e-12);
        assert!((c[1][1] - 0.25).abs() < 1e-12);
        assert_eq!(c[0][1], 0.);
        assert!(compact_capacitance(&[vec![1.,1.],vec![1.,1.]], &dual, budget).is_err());
    }
}

/// Matrix-free planar magnetic action in mixed current coordinates. Supports
/// must be disjoint physical owners, established by MixedMap. The returned
/// vector is L*i; no full current-coordinate Gram is materialized.
pub(super) fn compact_planar_magnetic_action(
    supports: &[super::planar_operator::SparseSupport], current: &[f64],
    budget: AllocationBudget, start: Instant, wall: Duration,
) -> Result<Vec<f64>> {
    if current.iter().any(|v|!v.is_finite()) || current.len()>budget.max_nodes || supports.len()>budget.max_tetrahedra {
        return Err(error("HYBRID_INPUT", "invalid compact magnetic action dimensions"));
    }
    let bytes=current.len().checked_mul(8).and_then(|v|v.checked_add(supports.len().checked_mul(24)?)).and_then(|v|v.checked_add(1024*1024)).ok_or_else(||error("MEMORY_LIMIT","compact action plan overflow"))?;
    budget.check(bytes)?;
    let mut densities=Vec::with_capacity(supports.len());
    for support in supports {
        let mut j=[0.;3];
        for &(i,v) in &support.current_density {
            if i>=current.len() || v.iter().any(|x|!x.is_finite()) {return Err(error("HYBRID_INPUT","invalid compact current support"));}
            for d in 0..3 {j[d]+=v[d]*current[i];}
        }
        densities.push(j);
    }
    let mut out=vec![0.;current.len()];
    for (a,sa) in supports.iter().enumerate() {for (b,sb) in supports[..=a].iter().enumerate() {
        check_time(start,wall)?;
        if densities[a]==[0.;3] && densities[b]==[0.;3] {continue;}
        let (integral,_,_)=super::planar_operator::pair_integral(&sa.prism,&sb.prism,8,start,wall)?;
        for &(i,v) in &sa.current_density {out[i]+=1e-7*integral*(0..3).map(|d|v[d]*densities[b][d]).sum::<f64>();}
        if a!=b {for &(i,v) in &sb.current_density {out[i]+=1e-7*integral*(0..3).map(|d|v[d]*densities[a][d]).sum::<f64>();}}
    }}
    if out.iter().any(|v|!v.is_finite()) {return Err(error("HYBRID_RANGE","nonfinite compact magnetic action"));}
    Ok(out)
}

/// Reusable bounded near/far action over sparse physical current supports.
/// Far remainder is propagated per mixed row; near quadrature remains an
/// order-estimated approximation rather than an independent error certificate.
pub(super) struct CompactPlanarAction {
    supports: Vec<super::planar_operator::SparseSupport>,
    plan: super::near_far::Plan,
    dimensions: usize,
    max_order:std::cell::Cell<usize>,
    max_change:std::cell::Cell<f64>,
    pair_attempts:std::cell::Cell<usize>,
    started: Instant,
    wall: Duration,
}
impl CompactPlanarAction {
    pub(super) fn new(supports: Vec<super::planar_operator::SparseSupport>, dimensions:usize, budget:AllocationBudget, started:Instant, wall:Duration) -> Result<Self> {
        if supports.is_empty() || dimensions>budget.max_nodes {return Err(error("HYBRID_INPUT","invalid near/far compact dimensions"));}
        let prisms:Vec<_>=supports.iter().map(|s|s.prism.clone()).collect();
        let pairs=prisms.len().checked_mul(prisms.len().checked_add(1).ok_or_else(||error("MEMORY_LIMIT","near/far pair overflow"))?).and_then(|n|n.checked_div(2)).ok_or_else(||error("MEMORY_LIMIT","near/far pair overflow"))?;
        let plan=super::near_far::Plan::new(&prisms,0.2,12,super::near_far::Budget {max_bytes:budget.max_bytes,max_nodes:budget.max_nodes,max_blocks:budget.max_bytes/128,max_pairs:pairs,max_wall:wall.saturating_sub(started.elapsed())})?;
        Ok(Self{supports,plan,dimensions,max_order:std::cell::Cell::new(0),max_change:std::cell::Cell::new(0.),pair_attempts:std::cell::Cell::new(0),started,wall})
    }
    fn apply_batch(&self, columns:&[Vec<f64>])->Result<(Vec<Vec<f64>>,Vec<Vec<f64>>)> {
        let count=columns.len();if columns.iter().any(|x|x.len()!=self.dimensions || x.iter().any(|v|!v.is_finite())) {return Err(error("HYBRID_INPUT","invalid planar action columns"));}
        let mut density=vec![vec![[0.;3];self.supports.len()];count];
        for (c,x) in columns.iter().enumerate() {for (s,j) in self.supports.iter().zip(&mut density[c]) {for &(i,v) in &s.current_density {if i>=x.len(){return Err(error("HYBRID_INPUT","current coordinate outside support"));}for d in 0..3 {j[d]+=v[d]*x[i];}}}}
        let mut out=vec![vec![0.;self.dimensions];count];let mut bound=out.clone();
        for far in [false,true] {self.plan.visit_pairs(far,|a,b| {
            check_time(self.started,self.wall).map_err(|failure|format!("{failure}; next compact planar pair a={a} b={b} triangleA={:?} zA={:?} triangleB={:?} zB={:?}",self.supports[a].prism.triangle_m,self.supports[a].prism.z_m,self.supports[b].prism.triangle_m,self.supports[b].prism.z_m))?;if (0..count).all(|c|density[c][a]==[0.;3] && density[c][b]==[0.;3]) {return Ok(());}
            let (value,error_bound)=if far {let p=self.plan.moments[a].pair(&self.plan.moments[b])?;(p.integral_m5,p.absolute_remainder_m5)} else {
                let index=self.pair_attempts.get()+1;self.pair_attempts.set(index);
                if index<=4 {eprintln!("P7_COMPACT_PLANAR_PAIR stage=start index={} a={} b={} triangleA={:?} zA={:?} triangleB={:?} zB={:?} elapsed={:.6}",index,a,b,self.supports[a].prism.triangle_m,self.supports[a].prism.z_m,self.supports[b].prism.triangle_m,self.supports[b].prism.z_m,self.started.elapsed().as_secs_f64());}
                let (v,order,change)=super::planar_operator::pair_integral(&self.supports[a].prism,&self.supports[b].prism,8,self.started,self.wall).map_err(|failure|format!("{failure}; compact planar pair a={a} b={b} triangleA={:?} zA={:?} triangleB={:?} zB={:?}",self.supports[a].prism.triangle_m,self.supports[a].prism.z_m,self.supports[b].prism.triangle_m,self.supports[b].prism.z_m))?;
                if index<=4 {eprintln!("P7_COMPACT_PLANAR_PAIR stage=end index={} order={} change={:e} elapsed={:.6}",index,order,change,self.started.elapsed().as_secs_f64());}
                self.max_order.set(self.max_order.get().max(order));self.max_change.set(self.max_change.get().max(change));(v,0.)
            };
            for c in 0..count {for &(i,v) in &self.supports[a].current_density {let dot=(0..3).map(|d|v[d]*density[c][b][d]).sum::<f64>();out[c][i]+=1e-7*value*dot;bound[c][i]+=1e-7*error_bound*(0..3).map(|d|v[d].abs()*density[c][b][d].abs()).sum::<f64>();}if a!=b {for &(i,v) in &self.supports[b].current_density {let dot=(0..3).map(|d|v[d]*density[c][a][d]).sum::<f64>();out[c][i]+=1e-7*value*dot;bound[c][i]+=1e-7*error_bound*(0..3).map(|d|v[d].abs()*density[c][a][d].abs()).sum::<f64>();}}}
            Ok(())
        })?;}
        if out.iter().flatten().chain(bound.iter().flatten()).any(|v|!v.is_finite()){return Err(error("HYBRID_RANGE","planar action overflow"));} Ok((out,bound))
    }
    pub(super) fn apply(&self, current:&[f64]) -> Result<(Vec<f64>,Vec<f64>)> {
        if current.len()!=self.dimensions || current.iter().any(|x|!x.is_finite()) {return Err(error("HYBRID_INPUT","invalid near/far current"));}
        let mut density=vec![[0.;3];self.supports.len()];
        for (s,j) in self.supports.iter().zip(&mut density) {for &(i,v) in &s.current_density {if i>=current.len(){return Err(error("HYBRID_INPUT","current coordinate outside support"));}for d in 0..3 {j[d]+=v[d]*current[i];}}}
        let mut out=vec![0.;current.len()];let mut bound=vec![0.;current.len()];
        for far in [false,true] {self.plan.visit_pairs(far,|a,b| {
            check_time(self.started,self.wall)?;
            if density[a]==[0.;3] && density[b]==[0.;3] {return Ok(());}
            let (value,error_bound)=if far {let p=self.plan.moments[a].pair(&self.plan.moments[b])?;(p.integral_m5,p.absolute_remainder_m5)} else {let index=self.pair_attempts.get()+1;self.pair_attempts.set(index);
                if index<=4 {eprintln!("P7_COMPACT_PLANAR_PAIR stage=start index={} a={} b={} triangleA={:?} zA={:?} triangleB={:?} zB={:?} elapsed={:.6}",index,a,b,self.supports[a].prism.triangle_m,self.supports[a].prism.z_m,self.supports[b].prism.triangle_m,self.supports[b].prism.z_m,self.started.elapsed().as_secs_f64());}
                let (v,order,change)=super::planar_operator::pair_integral(&self.supports[a].prism,&self.supports[b].prism,8,self.started,self.wall).map_err(|failure|format!("{failure}; compact planar pair a={a} b={b} triangleA={:?} zA={:?} triangleB={:?} zB={:?}",self.supports[a].prism.triangle_m,self.supports[a].prism.z_m,self.supports[b].prism.triangle_m,self.supports[b].prism.z_m))?;
                if index<=4 {eprintln!("P7_COMPACT_PLANAR_PAIR stage=end index={} order={} change={:e} elapsed={:.6}",index,order,change,self.started.elapsed().as_secs_f64());}
                self.max_order.set(self.max_order.get().max(order));self.max_change.set(self.max_change.get().max(change));(v,0.)};
            for &(i,v) in &self.supports[a].current_density {let dot=(0..3).map(|d|v[d]*density[b][d]).sum::<f64>();out[i]+=1e-7*value*dot;bound[i]+=1e-7*error_bound*(0..3).map(|d|v[d].abs()*density[b][d].abs()).sum::<f64>();}
            if a!=b {for &(i,v) in &self.supports[b].current_density {let dot=(0..3).map(|d|v[d]*density[a][d]).sum::<f64>();out[i]+=1e-7*value*dot;bound[i]+=1e-7*error_bound*(0..3).map(|d|v[d].abs()*density[a][d].abs()).sum::<f64>();}}
            Ok(())
        })?;}
        if out.iter().chain(&bound).any(|x|!x.is_finite()) {return Err(error("HYBRID_RANGE","nonfinite near/far action"));}
        Ok((out,bound))
    }
}
/// Matrix-free potential-coefficient action and exact sparse voltage/charge
/// dual map. Local and foil sources own disjoint copper; far remainders remain
/// separate from the algebraic residual.
pub(super) struct CompactChargeAction {
    planar:Vec<super::planar_operator::SparseSupport>,
    local:Vec<super::mixed_conductor::LocalSupport>,
    dual:Vec<Vec<(usize,f64)>>,
    plan:super::near_far::Plan,
    epsilon:f64,
    budget:AllocationBudget,
    start:Instant,
    wall:Duration,
    pair_cache:std::cell::RefCell<BTreeMap<(usize,usize),f64>>,
    action_count:std::cell::Cell<usize>,
}
impl CompactChargeAction {
    pub(super) fn new(map:&super::mixed_conductor::MixedMap<'_>,epsilon:f64,budget:AllocationBudget,start:Instant,wall:Duration)->Result<Self> {
        if !epsilon.is_finite() || epsilon<=0. {return Err(error("HYBRID_INPUT","positive permittivity required"));}
        let local=map.owners.iter().filter(|o|matches!(o,super::mixed_conductor::Owner::Volume)).count();
        let supports=map.foils.len().checked_mul(6).and_then(|n|n.checked_add(local.checked_mul(24)?)).ok_or_else(||error("MEMORY_LIMIT","charge support overflow"))?;
        let bytes=supports.checked_mul(1536).and_then(|v|v.checked_add(1024*128+2*1024*1024)).and_then(|v|v.checked_add(map.potential_count().checked_mul(1024)?)).and_then(|v|v.checked_add(map.retained_bytes().ok()?)).ok_or_else(||error("MEMORY_LIMIT","charge support plan overflow"))?;
        budget.check(bytes)?;
        let (planar,local,dual)=map.planar_sparse_charge_supports(&[])?;
        if planar.is_empty() || dual.is_empty() {return Err(error("HYBRID_INPUT","charge action requires foil support"));}
        let prisms:Vec<_>=planar.iter().map(|s|s.prism.clone()).collect();
        let pairs=prisms.len().checked_mul(prisms.len()+1).and_then(|n|n.checked_div(2)).ok_or_else(||error("MEMORY_LIMIT","charge pair overflow"))?;
        let available=budget.max_bytes.checked_sub(bytes).ok_or_else(||error("MEMORY_LIMIT","charge plan budget exhausted"))?;
        let plan=super::near_far::Plan::new(&prisms,0.2,12,super::near_far::Budget{max_bytes:available,max_nodes:budget.max_nodes,max_blocks:available/128,max_pairs:pairs,max_wall:wall.saturating_sub(start.elapsed())})?;
        Ok(Self{planar,local,dual,plan,epsilon,budget,start,wall,pair_cache:std::cell::RefCell::new(BTreeMap::new()),action_count:std::cell::Cell::new(0)})
    }
    pub(super) fn dual_voltage(&self,v:&[f64])->Result<Vec<f64>> {
        if v.iter().any(|x|!x.is_finite()) || self.dual.iter().flatten().any(|(i,_)|*i>=v.len()) {return Err(error("HYBRID_INPUT","invalid compact potential vector"));}
        Ok(self.dual.iter().map(|row|row.iter().fold(Sum::default(),|s,(i,w)|s.add(Sum::from(v[*i]).mul(*w))).value()).collect())
    }
    pub(super) fn project_charge(&self,q:&[f64],n:usize)->Result<Vec<f64>> {
        if q.len()!=self.dual.len() || q.iter().any(|x|!x.is_finite()) {return Err(error("HYBRID_INPUT","invalid compact charge vector"));}
        let mut out=vec![0.;n];for (row,&x) in self.dual.iter().zip(q) {for &(i,w) in row {if i>=n{return Err(error("HYBRID_INPUT","charge dual coordinate out of range"));}out[i]+=w*x;}} Ok(out)
    }
    /// Returns P*q and a conservative far-only row error bound.
    fn exact_planar_pair(&self,a:usize,b:usize)->Result<f64> {
        let key=(a.min(b),a.max(b));
        if let Some(&value)=self.pair_cache.borrow().get(&key) {return Ok(value);}
        let value=super::planar_operator::pair_integral(&self.planar[a].prism,&self.planar[b].prism,8,self.start,self.wall)?.0;
        let mut cache=self.pair_cache.borrow_mut();
        if cache.len()>=1024 {if let Some(&old)=cache.keys().next() {cache.remove(&old);}}
        cache.insert(key,value); Ok(value)
    }
    fn cache_value(&self,key:(usize,usize),f:impl FnOnce()->Result<f64>)->Result<f64> {
        if let Some(&v)=self.pair_cache.borrow().get(&key) {return Ok(v);}
        let value=f()?;let mut cache=self.pair_cache.borrow_mut();
        if cache.len()>=1024 {if let Some(&old)=cache.keys().next(){cache.remove(&old);}}
        cache.insert(key,value);Ok(value)
    }
    fn exact_cross(&self,p:usize,t:usize)->Result<f64> {
        self.cache_value((p,self.planar.len()+t),||Ok(super::planar_mixed::cross(&self.planar[p].prism,self.local[t].tetra,super::planar_mixed::Densities{charge:&[1.],current:&[]},super::planar_mixed::Densities{charge:&[1.],current:&[]},1./(4.*std::f64::consts::PI),8,AllocationBudget{max_bytes:2*1024*1024,..self.budget},self.wall.saturating_sub(self.start.elapsed()))?.potential_per_farad[0][0]))
    }
    fn exact_local(&self,i:usize,j:usize)->Result<f64> {
        let a=i.min(j);let b=i.max(j);self.cache_value((self.planar.len()+a,self.planar.len()+b),|| {
            let x=&self.local[a];let y=&self.local[b];
            if a==b {let coarse=polyhedron::tetra_self_integral(x.tetra,12)?;let fine=polyhedron::tetra_self_integral(x.tetra,16)?;if (fine-coarse).abs()>1e-8*fine.abs(){return Err(error("HYBRID_UNRESOLVED","tetra self order estimator"));}Ok(fine)} else {polyhedron::tetra_integral_adaptive(x.tetra,&PointPotential::tetrahedron(y.tetra)?.with_tighter_tetra_tolerance(1e-12)?,16,1e-10)}
        })
    }
    pub(super) fn apply(&self,q:&[f64])->Result<(Vec<f64>,Vec<f64>)> {self.apply_policy(q,false)}
    fn apply_policy(&self,q:&[f64],exact_far:bool)->Result<(Vec<f64>,Vec<f64>)> {
        self.action_count.set(self.action_count.get()+1);
        if q.len()!=self.dual.len() || q.iter().any(|x|!x.is_finite()) {return Err(error("HYBRID_INPUT","invalid charge action vector"));}
        if q.iter().all(|&x|x==0.) {return Ok((vec![0.;q.len()],vec![0.;q.len()]));}
        let mut out=vec![0.;q.len()];let mut bounds=vec![0.;q.len()];let scale=1./(4.*std::f64::consts::PI*self.epsilon);
        for far in [false,true] {self.plan.visit_pairs(far,|a,b| {
            check_time(self.start,self.wall)?;
            let (value,err)=if far && !exact_far {let p=self.plan.moments[a].pair(&self.plan.moments[b])?;(p.integral_m5,p.absolute_remainder_m5)} else {(self.exact_planar_pair(a,b)?,0.)};
            for &(i,da) in &self.planar[a].charge_density {for &(j,db) in &self.planar[b].charge_density {let k=scale*da*db;out[i]+=k*value*q[j];bounds[i]+=k.abs()*err*q[j].abs();if a!=b {out[j]+=k*value*q[i];bounds[j]+=k.abs()*err*q[i].abs();}}} Ok(())
        })?;}
        for (i,a) in self.local.iter().enumerate() {
            for (pi,p) in self.planar.iter().enumerate() {check_time(self.start,self.wall)?;let cross=self.exact_cross(pi,i)?;
                for &(j,d) in &p.charge_density {let k=scale*cross*d*a.charge_density;out[j]+=k*q[a.charge_index];out[a.charge_index]+=k*q[j];}}
            for (j,b) in self.local[..=i].iter().enumerate() {check_time(self.start,self.wall)?;
                let integral=self.exact_local(i,j)?;
                let k=scale*integral*a.charge_density*b.charge_density;out[a.charge_index]+=k*q[b.charge_index];if i!=j {out[b.charge_index]+=k*q[a.charge_index];}
            }
        }
        if out.iter().chain(&bounds).any(|x|!x.is_finite()){return Err(error("HYBRID_RANGE","nonfinite charge action"));} Ok((out,bounds))
    }
}
impl CompactChargeAction {
    /// Apply C = B^T P^-1 B using a caller-supplied positive P diagonal.
    /// The far remainder is checked separately from the CG true residual.
    pub(super) fn capacitance_action(&self,voltage:&[f64],diagonal:&[f64],max_iterations:usize)->Result<Vec<f64>> {
        let rhs=self.dual_voltage(voltage)?;
        for exact in [false,true] {
            let solution=super::compact_cg::solve(&rhs,diagonal,1e-9,max_iterations,self.budget,self.start,self.wall,|q,out| {
                let (p,_)=self.apply_policy(q,exact)?;out.copy_from_slice(&p);Ok(())
            })?;
            let (p,bound)=self.apply_policy(&solution.coefficients,exact)?;
            let norm=rhs.iter().map(|x|x*x).sum::<f64>().sqrt().max(f64::MIN_POSITIVE);
            let uncertainty=bound.iter().map(|x|x*x).sum::<f64>().sqrt()/norm;
            let residual=rhs.iter().zip(&p).map(|(a,b)|(a-b)*(a-b)).sum::<f64>().sqrt()/norm;
            if uncertainty.is_finite() && residual.is_finite() && uncertainty+residual<=1e-8 {return self.project_charge(&solution.coefficients,voltage.len());}
            if exact {return Err(error("HYBRID_CHARGE_RESIDUAL","compact charge action uncertainty exceeds numeric gate"));}
        }
        Err(error("HYBRID_CHARGE_RESIDUAL","compact charge solve did not converge"))
    }
}
impl CompactChargeAction {
    /// Assemble only diagonal(P), never the dense matrix. Shared-support
    /// contributions for one physical charge mode are included exactly once.
    pub(super) fn diagonal(&self)->Result<Vec<f64>> {
        let mut out=vec![0.;self.dual.len()];let scale=1./(4.*std::f64::consts::PI*self.epsilon);
        for far in [false,true] {self.plan.visit_pairs(far,|a,b| {
            for &(i,da) in &self.planar[a].charge_density {for &(j,db) in &self.planar[b].charge_density {if i==j {
                check_time(self.start,self.wall)?;
                let value=if far {self.plan.moments[a].pair(&self.plan.moments[b])?.integral_m5} else {super::planar_operator::pair_integral(&self.planar[a].prism,&self.planar[b].prism,8,self.start,self.wall)?.0};
                out[i]+=scale*value*da*db*if a==b {1.} else {2.};
            }}} Ok(())
        })?;}
        for (i,a) in self.local.iter().enumerate() {
            for (pi,p) in self.planar.iter().enumerate() {for &(j,d) in &p.charge_density {if j==a.charge_index {
                check_time(self.start,self.wall)?;
                let value=self.exact_cross(pi,i)?;
                out[j]+=2.*scale*value*d*a.charge_density;
            }}}
            for (j,b) in self.local[..=i].iter().enumerate() {if a.charge_index==b.charge_index {
                check_time(self.start,self.wall)?;
                let value=self.exact_local(i,j)?;
                out[a.charge_index]+=scale*value*a.charge_density*b.charge_density*if i==j {1.} else {2.};
            }}
        }
        if out.iter().any(|v|!v.is_finite() || *v<=0.) {return Err(error("HYBRID_CHARGE","invalid charge diagonal"));}Ok(out)
    }
}
fn compact_upper(v:f64)->f64 {if v==0. {0.} else if v.is_finite() && v>0. {f64::from_bits(v.to_bits()+1)} else {v}}
fn compact_bound_product(a:f64,b:f64)->f64 {if a==0. || b==0. {0.} else {let product=a*b;if product==0. {f64::from_bits(1)} else {compact_upper(product)}}}
fn compact_bound_sum(a:f64,b:f64)->f64 {if a==0. {b} else if b==0. {a} else {compact_upper(a+b)}}

fn compact_pl_worker_bytes(workers:usize,budget:AllocationBudget)->Result<usize> {
    if !(1..=4).contains(&workers) {return Err(error("HYBRID_INPUT","PL thread count must be 1..4"));}
    let bytes=workers.checked_mul(if workers==1 {2*1024*1024} else {5*1024*1024}).ok_or_else(||error("MEMORY_LIMIT","PL worker reservation overflow"))?;
    budget.check(bytes)?;Ok(bytes)
}

struct CompactMixedMagnetic {
    supports:Vec<super::planar_operator::SparseSupport>,
    dimensions:usize,
    planar_max_order:std::cell::Cell<usize>,
    planar_max_change:std::cell::Cell<f64>,
    local:Vec<super::mixed_conductor::LocalCurrentSupport>,
    local_pairs:Option<super::volume_integral_magnetic::CompactLocalPairs>,
    mixed_plan:super::near_far::Plan,
    pl_pool:Option<rayon::ThreadPool>,
    pl_workers:usize,
    cache:std::cell::RefCell<BTreeMap<(usize,usize),f64>>,
    budget:AllocationBudget,start:Instant,wall:Duration,
    action_count:std::cell::Cell<usize>,
    cache_limit:usize,
    batch_limit:usize,
    pair_count:std::cell::Cell<usize>,
    promoted_pairs:std::cell::Cell<usize>,
    mixed_max_change:std::cell::Cell<f64>,
    mixed_fallback_count:std::cell::Cell<usize>,
    self_max_change:std::cell::Cell<f64>,
}
impl CompactMixedMagnetic {
    fn new(map:&super::mixed_conductor::MixedMap<'_>,budget:AllocationBudget,start:Instant,wall:Duration)->Result<Self> {
        let workers=match std::env::var("P7_COMPACT_PL_THREADS") {Ok(value)=>value.parse::<usize>().map_err(|_|error("HYBRID_INPUT","PL thread count must be 1..4"))?,Err(std::env::VarError::NotPresent)=>1,Err(_)=>return Err(error("HYBRID_INPUT","invalid PL thread count"))};
        // Per worker: 2 MiB kernel scratch, 2 MiB stack and 1 MiB overhead.
        let worker_bytes=compact_pl_worker_bytes(workers,budget)?;
        let n=map.foils.len().checked_add(map.owners.len()).and_then(|n|n.checked_mul(1024)).and_then(|n|n.checked_add(map.retained_bytes().ok()?)).and_then(|n|n.checked_add(worker_bytes)).ok_or_else(||error("MEMORY_LIMIT","mixed magnetic plan overflow"))?;budget.check(n)?;
        let pl_pool=if workers>1 {Some(rayon::ThreadPoolBuilder::new().num_threads(workers).stack_size(2*1024*1024).build().map_err(|_|error("MEMORY_LIMIT","PL worker pool creation failed"))?)} else {None};
        let remaining=budget.max_bytes-n;let cache_bytes=remaining/4;let cache_limit=cache_bytes/128;let batch_limit=remaining/8;let local_budget=remaining/8;let mixed_budget=remaining-cache_bytes-batch_limit-local_budget;
        let supports=map.planar_current_supports()?;
        let local=map.local_current_supports()?;
        let local_pairs=if local.is_empty(){None}else{Some(super::volume_integral_magnetic::CompactLocalPairs::new(local.iter().map(|s|s.tetra).collect(),AllocationBudget{max_bytes:local_budget,..budget},start,wall)?)};
        let mut moments=Vec::with_capacity(supports.len()+local.len());
        for s in &supports {check_time(start,wall)?;moments.push(super::far::Moments::new(&s.prism)?);}
        for s in &local {check_time(start,wall)?;moments.push(super::far::Moments::tetrahedron(s.tetra)?);}
        let pairs=moments.len().checked_mul(moments.len().checked_add(1).ok_or_else(||error("MEMORY_LIMIT","mixed pair overflow"))?).and_then(|n|n.checked_div(2)).ok_or_else(||error("MEMORY_LIMIT","mixed pair overflow"))?;
        let mixed_plan=super::near_far::Plan::from_moments(moments,0.2,12,super::near_far::Budget{max_bytes:mixed_budget,max_nodes:budget.max_nodes,max_blocks:mixed_budget/128,max_pairs:pairs,max_wall:wall.saturating_sub(start.elapsed())})?;
        eprintln!("P7_COMPACT_UNIFIED_PLAN supports={} nearPairs={} farPairs={} planBytes={} cacheCapacity={}",mixed_plan.moments.len(),mixed_plan.near_pairs,mixed_plan.far_pairs,mixed_plan.planned_bytes,cache_limit);
        Ok(Self{supports,dimensions:map.current_count(),planar_max_order:std::cell::Cell::new(0),planar_max_change:std::cell::Cell::new(0.),local,local_pairs,mixed_plan,pl_pool,pl_workers:workers,cache:std::cell::RefCell::new(BTreeMap::new()),budget,start,wall,action_count:std::cell::Cell::new(0),cache_limit,batch_limit,pair_count:std::cell::Cell::new(0),promoted_pairs:std::cell::Cell::new(0),mixed_max_change:std::cell::Cell::new(0.),mixed_fallback_count:std::cell::Cell::new(0),self_max_change:std::cell::Cell::new(0.)})
    }
    fn cached(&self,key:(usize,usize),f:impl FnOnce()->Result<f64>)->Result<f64> {
        if let Some(&v)=self.cache.borrow().get(&key){return Ok(v);}
        let count=self.pair_count.get()+1;
        if count<=4 {eprintln!("P7_COMPACT_PAIR stage=start index={} kind={} a={} b={} elapsed={:.6}",count,if key.1<self.supports.len() {"prism-prism"} else if key.0<self.supports.len() {"prism-tetra"} else if key.0==key.1 {"tetra-self"} else {"tetra-pair"},key.0,key.1,self.start.elapsed().as_secs_f64());}
        let v=f()?;self.pair_count.set(count);
        if count<=4 {eprintln!("P7_COMPACT_PAIR stage=end index={} elapsed={:.6}",count,self.start.elapsed().as_secs_f64());}
        if count%64==0 {eprintln!("P7_COMPACT_MAGNETIC pairs={} cacheEntries={} elapsed={:.6}",count,self.cache.borrow().len(),self.start.elapsed().as_secs_f64());}
        if self.cache_limit>0 {let mut c=self.cache.borrow_mut();if c.len()>=self.cache_limit {if let Some(&k)=c.keys().next(){c.remove(&k);}}c.insert(key,v);}Ok(v)
    }
    fn densities(&self,index:usize)->&[(usize,[f64;3])] {
        if index<self.supports.len() {&self.supports[index].current_density} else {&self.local[index-self.supports.len()].densities}
    }
    fn near_pair(&self,a:usize,b:usize)->Result<f64> {
        let (a,b)=(a.min(b),a.max(b));let np=self.supports.len();
        self.cached((a,b),|| {
            if b<np {
                let (v,order,change)=super::planar_operator::pair_integral(&self.supports[a].prism,&self.supports[b].prism,8,self.start,self.wall)?;
                self.planar_max_order.set(self.planar_max_order.get().max(order));self.planar_max_change.set(self.planar_max_change.get().max(change));Ok(v)
            } else if a<np {
                let block=super::planar_mixed::cross(&self.supports[a].prism,self.local[b-np].tetra,super::planar_mixed::Densities{charge:&[1.],current:&[]},super::planar_mixed::Densities{charge:&[1.],current:&[]},1./(4.*std::f64::consts::PI),8,AllocationBudget{max_bytes:2*1024*1024,..self.budget},self.wall.saturating_sub(self.start.elapsed()))?;
                if block.used_tetra_fallback {self.mixed_fallback_count.set(self.mixed_fallback_count.get()+1);}else if let Some(change)=block.relative_order_change {self.mixed_max_change.set(self.mixed_max_change.get().max(change));}Ok(block.potential_per_farad[0][0])
            } else {
                let (i,j)=(a-np,b-np);
                if i==j {let c=polyhedron::tetra_self_integral(self.local[i].tetra,12)?;let f=polyhedron::tetra_self_integral(self.local[i].tetra,16)?;let change=(f-c).abs()/f.abs();if !change.is_finite() || change>1e-8 {Err(error("HYBRID_UNRESOLVED","tetra self order estimator"))}else{self.self_max_change.set(self.self_max_change.get().max(change));Ok(f)}}
                else {self.local_pairs.as_ref().ok_or_else(||error("HYBRID_INPUT","local pair kernel missing"))?.pair(i,j)}
            }
        })
    }
    fn apply_batch(&self,columns:&[Vec<f64>],goals:Option<&[f64]>)->Result<(Vec<Vec<f64>>,Vec<Vec<f64>>)> {
        if let Some(goals)=goals {if goals.len()!=columns.len() || goals.iter().any(|g|!g.is_finite() || *g<=0.) {return Err(error("HYBRID_INPUT","invalid magnetic action goals"));}}
        let (actions,bounds)=self.apply_batch_once(columns,false,goals)?;
        if let Some(goals)=goals {
            if bounds.iter().zip(goals).any(|(row,g)|row.iter().any(|b|*b>*g)) {return self.apply_batch_once(columns,true,None);}
        }
        Ok((actions,bounds))
    }
    fn flush_pl(&self,jobs:&mut Vec<(usize,usize)>,density:&[Vec<[f64;3]>],output:&mut [Vec<f64>])->Result<()> {
        if jobs.is_empty() {return Ok(());}
        use rayon::prelude::*;
        let pool=self.pl_pool.as_ref().ok_or_else(||error("HYBRID_INPUT","parallel PL pool missing"))?;
        let supports=&self.supports;let local=&self.local;let np=supports.len();let budget=AllocationBudget{max_bytes:2*1024*1024,..self.budget};let start=self.start;let wall=self.wall;
        let results:Vec<_>=pool.install(||jobs.par_iter().map(|&(a,b)| {
            check_time(start,wall)?;
            super::planar_mixed::cross(&supports[a.min(b)].prism,local[a.max(b)-np].tetra,super::planar_mixed::Densities{charge:&[1.],current:&[]},super::planar_mixed::Densities{charge:&[1.],current:&[]},1./(4.*std::f64::consts::PI),8,budget,wall.saturating_sub(start.elapsed()))
        }).collect());
        for ((a,b),result) in jobs.drain(..).zip(results) {
            let block=result?;let value=self.cached((a.min(b),a.max(b)),||Ok(block.potential_per_farad[0][0]))?;
            if block.used_tetra_fallback {self.mixed_fallback_count.set(self.mixed_fallback_count.get()+1);} else if let Some(change)=block.relative_order_change {self.mixed_max_change.set(self.mixed_max_change.get().max(change));}
            for c in 0..output.len() {
                for &(i,d) in self.densities(a) {output[c][i]+=1e-7*value*(0..3).map(|k|d[k]*density[c][b][k]).sum::<f64>();}
                for &(i,d) in self.densities(b) {output[c][i]+=1e-7*value*(0..3).map(|k|d[k]*density[c][a][k]).sum::<f64>();}
            }
        }
        Ok(())
    }
    fn apply_batch_once(&self,columns:&[Vec<f64>],exact_far:bool,goals:Option<&[f64]>)->Result<(Vec<Vec<f64>>,Vec<Vec<f64>>)> {
        let count=columns.len();if count==0 {return Ok((vec![],vec![]));}let dim=self.dimensions;
        if columns.iter().any(|v|v.len()!=dim || v.iter().any(|x|!x.is_finite())) {return Err(error("HYBRID_INPUT","invalid magnetic columns"));}
        let ns=self.supports.len()+self.local.len();
        let bytes=count.checked_mul(ns).and_then(|n|n.checked_mul(48)).and_then(|n|n.checked_add(count.checked_mul(dim)?.checked_mul(16)?)).ok_or_else(||error("MEMORY_LIMIT","batch size overflow"))?;
        if bytes>self.batch_limit {return Err(error("MEMORY_LIMIT","mixed magnetic batch exceeds reservation"));}
        self.action_count.set(self.action_count.get()+count);
        let mut density=vec![vec![[0.;3];ns];count];let mut envelope=density.clone();let mut output=vec![vec![0.;dim];count];let mut bounds=output.clone();
        for (c,x) in columns.iter().enumerate() {for s in 0..ns {for &(i,d) in self.densities(s) {if i>=dim {return Err(error("HYBRID_INPUT","invalid current support"));}for k in 0..3 {density[c][s][k]+=d[k]*x[i];envelope[c][s][k]=compact_bound_sum(envelope[c][s][k],compact_bound_product(d[k].abs(),x[i].abs()));}}}}
        let mut pl_jobs=Vec::with_capacity(32);
        for far in [true,false] {self.mixed_plan.visit_pairs(far,|a,b| {
            check_time(self.start,self.wall)?;if (0..count).all(|c|density[c][a]==[0.;3] && density[c][b]==[0.;3]) {return Ok(());}
            let candidate=if !exact_far && a!=b {self.mixed_plan.moments[a].pair(&self.mixed_plan.moments[b]).ok()} else {None};
            let contribution=|c:usize,row:usize,err:f64| {
                let mut total=0.;
                for &(i,d) in self.densities(a) {if i==row {for k in 0..3 {total=compact_bound_sum(total,compact_bound_product(d[k].abs(),envelope[c][b][k]));}}}
                if a!=b {for &(i,d) in self.densities(b) {if i==row {for k in 0..3 {total=compact_bound_sum(total,compact_bound_product(d[k].abs(),envelope[c][a][k]));}}}}
                compact_bound_product(compact_bound_product(1e-7,err),total)
            };
            let accept=if let (Some(p),Some(goals))=(&candidate,goals) {
                (0..count).all(|c| self.densities(a).iter().chain(if a!=b {self.densities(b)} else {&[]}).all(|(i,_)| compact_bound_sum(bounds[c][*i],contribution(c,*i,p.absolute_remainder_m5))<=goals[c]))
            } else {false};
            let (lo,hi)=(a.min(b),a.max(b));let np=self.supports.len();
            if !accept && self.pl_pool.is_some() && lo<np && hi>=np && !self.cache.borrow().contains_key(&(lo,hi)) {
                pl_jobs.push((a,b));
                if pl_jobs.len()==32 {self.flush_pl(&mut pl_jobs,&density,&mut output)?;}
                return Ok(());
            }
            if !pl_jobs.is_empty() {self.flush_pl(&mut pl_jobs,&density,&mut output)?;}
            let (value,error_bound)=if accept {let p=candidate.unwrap();if !far {self.promoted_pairs.set(self.promoted_pairs.get()+1);} (p.integral_m5,p.absolute_remainder_m5)}else{(self.near_pair(a,b)?,0.)};
            for c in 0..count {
                for &(i,d) in self.densities(a) {output[c][i]+=1e-7*value*(0..3).map(|k|d[k]*density[c][b][k]).sum::<f64>();}
                if a!=b {for &(i,d) in self.densities(b) {output[c][i]+=1e-7*value*(0..3).map(|k|d[k]*density[c][a][k]).sum::<f64>();}}
                if error_bound>0. {let mut rows=Vec::with_capacity(self.densities(a).len()+self.densities(b).len());rows.extend(self.densities(a).iter().map(|x|x.0));if a!=b {rows.extend(self.densities(b).iter().map(|x|x.0));}rows.sort_unstable();rows.dedup();for i in rows {bounds[c][i]=compact_bound_sum(bounds[c][i],contribution(c,i,error_bound));}}
            } Ok(())
        })?; if !pl_jobs.is_empty() {self.flush_pl(&mut pl_jobs,&density,&mut output)?;}}
        if output.iter().flatten().chain(bounds.iter().flatten()).any(|v|!v.is_finite()){return Err(error("HYBRID_RANGE","mixed magnetic batch overflow"));}Ok((output,bounds))
    }

}
pub(super) fn solve_compact_adaptive(
    map:&super::mixed_conductor::MixedMap<'_>, names:&[String], ports:&[[usize;2]],
    packages:&[super::checked_packages::Validated], grid:MatchedGrid<'_>, field:&super::layered_capacitance::SpatialFieldAction, budget:AllocationBudget, start:Instant, wall:Duration,
)->Result<(serde_json::Value,Model,usize)> {
    let n=map.potential_count();let ni=map.current_count();
    let share=budget.max_bytes/3;
    let operator_budget=AllocationBudget{max_bytes:share,..budget};
    if field.planned_bytes()>share {return Err(error("MEMORY_LIMIT","adaptive layered field exceeds reserved share"));}
    eprintln!("P7_COMPACT_STAGE stage=magnetic-prepare-start elapsed={:.6}",start.elapsed().as_secs_f64());
    let magnetic=CompactMixedMagnetic::new(map,operator_budget,start,wall)?;
    eprintln!("P7_COMPACT_STAGE stage=magnetic-prepare-end elapsed={:.6}",start.elapsed().as_secs_f64());
    let mut terms=vec![Vec::new();ni];for &(i,j,a) in &map.incidence {terms[j].push((i,a));}
    let cells:Vec<_>=terms.into_iter().zip(&map.resistance_ohm).map(|(t,&r)|Cell::new(t,r)).collect();
    let basis_budget=AllocationBudget{max_bytes:share,..budget};
    let upper_hz=grid.frequencies.last().copied().ok_or_else(||error("HYBRID_INPUT","empty frequency grid"))?;
    let stratified=map.foils.iter().any(|foil|map.mesh().layer_spans.get(&foil.layer).is_some_and(|span|*span!=foil.z_span_m));
    let group_bytes=if stratified {map.foils.len().checked_mul(512).ok_or_else(||error("MEMORY_LIMIT","skin group plan overflow"))?} else {0};
    let eligible_groups=if stratified {
        basis_budget.check(native_mode_sizes(n,ni,0,0,group_bytes)?)?;
        map.skin_hint_group_count(upper_hz,AllocationBudget{max_bytes:group_bytes,..basis_budget},start,wall)?
    } else {0};
    let mut retained:BTreeSet<usize>=map.electrodes.iter().copied().collect();
    for island in &map.potential_components {if !island.iter().any(|i|retained.contains(i)) {retained.insert(island[0]);}}
    let seed_count=retained.len().checked_sub(map.potential_components.len()).ok_or_else(||error("HYBRID_INPUT","invalid physical island anchors"))?;
    let hint_cap=if eligible_groups>0 {seed_count.checked_mul(4).ok_or_else(||error("MEMORY_LIMIT","skin hint count overflow"))?} else {0};
    let growth=grid.max_iterations.checked_mul(4).ok_or_else(||error("MEMORY_LIMIT","enrichment mode count overflow"))?;
    let voltage_cap=retained.len().checked_add(growth).ok_or_else(||error("MEMORY_LIMIT","potential mode count overflow"))?;
    let current_cap=seed_count.checked_add(growth).and_then(|v|v.checked_add(hint_cap)).ok_or_else(||error("MEMORY_LIMIT","current mode count overflow"))?.min(ni);
    let hint_bytes=hint_cap.checked_mul(ni).and_then(|v|v.checked_mul(8)).and_then(|v|v.checked_add(hint_cap.checked_mul(std::mem::size_of::<Vec<f64>>())?)).ok_or_else(||error("MEMORY_LIMIT","skin hint buffer overflow"))?;
    let hinted_plan=if hint_cap>0 {
        if voltage_cap>1024 || current_cap>1024 || voltage_cap.checked_add(current_cap).is_none_or(|v|v>2048) {return Err(error("FOIL_REDUCTION","skin hint mode cap exceeds P6 coordinate limits"));}
        let plan=native_mode_sizes(n,ni,voltage_cap,current_cap,hint_bytes.checked_add(group_bytes).ok_or_else(||error("MEMORY_LIMIT","skin hint scratch overflow"))?)?;
        basis_budget.check(plan)?; Some(plan)
    } else {None};
    let (mut basis,harmonic_peak,dc)=harmonic(&cells,n,&map.electrodes.iter().copied().collect(),Some(&map.potential_components),basis_budget,0,start,wall,growth,true)?;
    let peak=hinted_plan.map_or(harmonic_peak,|plan|plan.max(harmonic_peak));
    let before_hints=basis.current.len();let mut submitted_hints=0;
    if hint_cap>0 {
        if basis.current.len()>seed_count {return Err(error("HYBRID_INPUT","skin seed count exceeds admitted bound"));}
        let hints=map.skin_current_hints(&basis.current,upper_hz,AllocationBudget{max_bytes:hint_bytes.checked_add(group_bytes).ok_or_else(||error("MEMORY_LIMIT","skin hint budget overflow"))?,..basis_budget},start,wall)?;
        if hints.len()>hint_cap {return Err(error("HYBRID_INPUT","skin hints exceed admitted bound"));}
        submitted_hints=hints.len();
        for hint in hints {check_time(start,wall)?;append_current_mode(&cells,&mut basis.current,hint)?;}
    }
    let added_hints=basis.current.len()-before_hints;
    eprintln!("P7_COMPACT_STAGE stage=charge-actions-start elapsed={:.6} modes={}",start.elapsed().as_secs_f64(),basis.voltage.len());
    let charges=basis.voltage.iter().map(|v|field.apply(v)).collect::<Result<Vec<_>>>()?;
    eprintln!("P7_COMPACT_STAGE stage=charge-actions-end elapsed={:.6} actions={}",start.elapsed().as_secs_f64(),field.action_count());
    let angular_max=2.*std::f64::consts::PI*grid.frequencies.last().copied().ok_or_else(||error("HYBRID_INPUT","empty frequency grid"))?;
    let batch=|q:&[Vec<f64>],goals:Option<&[f64]>,_:Instant,_:Duration|->Result<Magnetic>{
        let derived;
        let goals=if let Some(goals)=goals {goals} else {
            derived=q.iter().map(|column| {
                if column.len()!=map.current_count() || column.iter().any(|v|!v.is_finite()) {return Err(error("HYBRID_INPUT","invalid current column"));}
                if column.iter().all(|&v|v==0.) {return Ok(1.);}
                let mut scale=0_f64;
                for (&resistance,&current) in map.resistance_ohm.iter().zip(column) {
                    let (_,lower,_)=super::volume_integral_magnetic::hierarchical_magnetic::kvl_cell(resistance,current,&[],&[],0.,0.,0.)?;
                    scale=scale.max(lower);
                }
                super::volume_integral_magnetic::hierarchical_magnetic::action_goal(scale,angular_max)
            }).collect::<Result<Vec<_>>>()?; &derived
        };
        let (actions,bounds)=magnetic.apply_batch(q,Some(goals))?;
        Ok(Magnetic{actions,error_bounds:Some(bounds),certificate:Some(serde_json::json!({"policy":"compact-mixed-near-far","nearQuadratureErrorCertified":false})),reciprocity:0.,refinement:0.})
    };
    eprintln!("P7_COMPACT_STAGE stage=magnetic-actions-start elapsed={:.6}",start.elapsed().as_secs_f64());
    let initial=batch(&basis.current,None,start,wall)?;
    eprintln!("P7_COMPACT_STAGE stage=magnetic-actions-end elapsed={:.6} actions={}",start.elapsed().as_secs_f64(),magnetic.action_count.get());
    let (model,max_electric_reciprocity)=make_model(&cells,n,&basis,names,&charges,&initial.actions,ports)?;
    let mut space=ReducedSpace{model,cells,dc_harmonic_residual:basis.dc_residual,basis,charges,magnetic_actions:initial.actions,magnetic_basis_certificate:initial.certificate,ports:ports.to_vec(),full_potentials:n,max_frequency_hz:*grid.frequencies.last().ok_or_else(||error("HYBRID_INPUT","empty frequency grid"))?,planned_bytes:peak,max_magnetic_reciprocity:0.,max_magnetic_refinement:0.,max_electric_reciprocity,enrichment:None,native_allocation:true};
    let training_budget=AllocationBudget{max_bytes:share*2,..budget};
    eprintln!("P7_COMPACT_STAGE stage=training-start elapsed={:.6}",start.elapsed().as_secs_f64());
    let result=space.train_grid(&grid,names,&dc,field,&batch,training_budget,start,wall,true,packages,None,true);
    eprintln!("P7_COMPACT_STAGE stage=training-end elapsed={:.6} chargeActions={} magneticActions={}",start.elapsed().as_secs_f64(),field.action_count(),magnetic.action_count.get());
    let mut result=result?;
    if std::env::var_os("P7_HYBRID_REGIME_AUDIT").as_deref()==Some(std::ffi::OsStr::new("1")) {
        let report_bytes=loss_gram_reservation(grid.references.len(),0,packages,grid.frequencies.len())?;
        let available=(share*2).checked_sub(field.planned_bytes()).and_then(|v|v.checked_sub(space.planned_bytes)).and_then(|v|v.checked_sub(dc.owned_bytes)).and_then(|v|v.checked_sub(report_bytes)).ok_or_else(||error("MEMORY_LIMIT","regime audit working set exceeds reservation"))?;
        let audit_budget=AllocationBudget{max_bytes:if packages.is_empty(){available}else{available/2},..budget};
        let joint_model=if packages.is_empty(){None}else{
            let joint=super::checked_packages::compile(&space.model,packages,grid.references.len(),&Budget{max_memory_mi_b:Some(audit_budget.max_bytes as f64/1048576.),max_seconds:Some(wall.saturating_sub(start.elapsed()).as_secs_f64())})?;
            Some(joint.model)
        };
        let upper=*grid.frequencies.last().ok_or_else(||error("HYBRID_INPUT","empty regime grid"))?;
        let probe=grid.frequencies[0].min(upper/20.);
        let labels=(0..grid.references.len()).map(|i|format!("physical-port-{i}")).collect::<Vec<_>>();
        let audit=super::regime_audit::audit(joint_model.as_ref().unwrap_or(&space.model),&labels,grid.references,probe,upper,audit_budget,start,wall)?;
        if audit["status"]!="pass" {return Err(error("REGIME_MODEL","DC/AC/time consistency control failed"));}
        result["modelRegimeAudit"]=audit;
    }
    let full_uncertainty=result["quality"]["maxFullKvlUncertaintyRelative"].take();
    result["quality"].as_object_mut().unwrap().remove("maxFullKvlUncertaintyRelative");
    result["quality"]["maxCompactKvlUncertaintyRelative"]=full_uncertainty;
    if let Some(samples)=result["samples"].as_array_mut() {for sample in samples {if let Some(columns)=sample["columns"].as_array_mut() {for column in columns {
        let uncertainty=column["kvlUncertaintyRelative"].take();
        column.as_object_mut().unwrap().remove("kvlUncertaintyRelative");
        column["compactKvlUncertaintyRelative"]=uncertainty;
        let mut certificate=column["magneticCertificate"].take();
        column.as_object_mut().unwrap().remove("magneticCertificate");
        certificate["policy"]=serde_json::json!("compact-mixed-near-far");
        certificate["nearQuadratureErrorCertified"]=serde_json::json!(false);
        column["compactMagneticCertificate"]=certificate;
    }}}}
    result["quality"]["scope"]=serde_json::json!("compact-discrete");result["quality"]["physicalError"]=serde_json::json!("unverified");
    result["compactEvidence"]=serde_json::json!({"basis":"adaptive-mixed-closure","skinHints":{"eligibleGroups":eligible_groups,"submitted":submitted_hints,"added":added_hints,"upperHz":upper_hz,"coverage":"unverified"},"planarMaxOrder":magnetic.planar_max_order.get(),"planarMaxRelativeChange":magnetic.planar_max_change.get(),"mixedMaxRelativeChange":magnetic.mixed_max_change.get(),"mixedTetraFallbackCount":magnetic.mixed_fallback_count.get(),"mixedTetraFallbackTolerance":1e-10,"goalAdmittedNearPairs":magnetic.promoted_pairs.get(),"parallelPlWorkers":magnetic.pl_workers,"tetraSelfMaxRelativeChange":magnetic.self_max_change.get(),"tetraPairAdaptiveTolerance":1e-10,"nearQuadratureEstimatedTolerance":1e-8,"originalVolumeActions":0,"scope":"compact-discrete","physicalAccuracy":"unverified","approximationError":"unverified"});
    Ok((result,space.model,budget.max_bytes))
}
