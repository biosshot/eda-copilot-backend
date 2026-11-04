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

#[derive(Clone)]
struct Cell {
    nodes: [usize; 3],
    incidence: [[f64; 2]; 3],
    resistance: f64,
}
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
    ports: Vec<[usize; 2]>,
    full_potentials: usize,
    max_frequency_hz: f64,
    pub planned_bytes: usize,
    pub max_magnetic_reciprocity: f64,
    pub max_magnetic_refinement: f64,
    pub max_electric_reciprocity: f64,
    pub dc_harmonic_residual: f64,
    pub enrichment: Option<Enrichment>,
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
    // lift/action slabs and the downstream reduced descriptor. No N² term.
    n.checked_mul(1024)
        .and_then(|x| x.checked_add(m.checked_mul(32768)?))
        .and_then(|x| {
            x.checked_add(
                n.checked_add(2usize.checked_mul(m)?)?
                    .checked_mul(r)?
                    .checked_mul(64)?,
            )
        })
        .and_then(|x| x.checked_add(r.checked_mul(2)?.checked_pow(2)?.checked_mul(2048)?))
        .and_then(|x| x.checked_add(field_bytes))
        .ok_or_else(|| error("MEMORY_LIMIT", "reduced foil plan overflow"))
}
fn gradient(cells: &[Cell], v: &[f64]) -> Vec<f64> {
    cells
        .iter()
        .flat_map(|c| {
            (0..2).map(move |k| {
                (1..3)
                    .map(|i| c.incidence[i][k] * (v[c.nodes[i]] - v[c.nodes[0]]))
                    .sum::<f64>()
            })
        })
        .collect()
}
fn divergence(cells: &[Cell], j: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![Sum::default(); n];
    for (i, c) in cells.iter().enumerate() {
        for k in 0..3 {
            for axis in 0..2 {
                let node = c.nodes[k];
                out[node] = out[node].add(Sum::from(c.incidence[k][axis]).mul(j[2 * i + axis]));
            }
        }
    }
    out.into_iter().map(Sum::value).collect()
}
fn conductive(cells: &[Cell], v: &[f64]) -> Vec<f64> {
    let mut j = gradient(cells, v);
    for (i, c) in cells.iter().enumerate() {
        j[2 * i] /= c.resistance;
        j[2 * i + 1] /= c.resistance;
    }
    j
}
struct DcCorrection {
    free_map: Vec<Option<usize>>,
    free: usize,
    owned_bytes: usize,
    factor: Option<(cholesky::SymbolicCholesky<usize>, Vec<f64>, usize, usize)>,
}
impl DcCorrection {
    fn solve(&self, rhs: &[f64]) -> Result<Vec<f64>> {
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

fn harmonic(
    cells: &[Cell],
    n: usize,
    contacts: &BTreeSet<usize>,
    budget: AllocationBudget,
    base: usize,
    start: Instant,
    wall: Duration,
    extra_modes: usize,
) -> Result<(Basis, usize, DcCorrection)> {
    let mut parents: Vec<_> = (0..n).collect();
    let mut used = vec![false; n];
    for c in cells {
        let a = root(&parents, c.nodes[0]);
        for node in c.nodes {
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
    let islands: Vec<_> = islands.into_values().collect();
    let mut retained = contacts.clone();
    // A floating island keeps a finite C coordinate. This is not grounding.
    for island in &islands {
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
    let plan = sizes(n, cells.len(), r, base)?;
    budget.check(plan)?;
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
    let factor = if next > 0 {
        let mut terms = Vec::with_capacity(cells.len() * 9);
        for c in cells {
            for a in 0..3 {
                for b in 0..3 {
                    if let (Some(i), Some(j)) = (free_map[c.nodes[a]], free_map[c.nodes[b]]) {
                        if i <= j {
                            let val = (0..2)
                                .map(|k| c.incidence[a][k] * c.incidence[b][k] / c.resistance)
                                .sum();
                            terms.push(Triplet::new(i, j, val));
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
    let retained: Vec<_> = retained.into_iter().collect();
    let mut voltage = vec![vec![0.; n]; retained.len()];
    let mut current = Vec::new();
    let mut components = Vec::new();
    let mut dc_residual: f64 = 0.;
    for island in &islands {
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
            if let Some((symbolic, values, _, _)) = &factor {
                let action = divergence(cells, &conductive(cells, v), n);
                let mut rhs = Mat::zeros(next, 1);
                for i in 0..n {
                    if let Some(k) = free_map[i] {
                        rhs[(k, 0)] = -action[i];
                    }
                }
                let par = super::parallel::current();
                let mut scratch =
                    MemBuffer::try_new(symbolic.solve_in_place_scratch::<f64>(1, par))
                        .map_err(|_| error("MEMORY_LIMIT", "foil sparse solve scratch"))?;
                cholesky::LltRef::new(symbolic, values).solve_in_place_with_conj(
                    Conj::No,
                    rhs.as_mut(),
                    par,
                    MemStack::new(&mut scratch),
                );
                for i in 0..n {
                    if let Some(k) = free_map[i] {
                        v[i] = rhs[(k, 0)];
                    }
                }
            }
            let mut q = conductive(cells, v);
            let norm = q
                .chunks_exact(2)
                .zip(cells)
                .map(|(j, c)| c.resistance * (j[0] * j[0] + j[1] * j[1]))
                .sum::<f64>()
                .sqrt();
            if !norm.is_finite() || norm <= 0. {
                return Err(error(
                    "FOIL_REDUCTION",
                    "independent finite harmonic current required",
                ));
            }
            let reaction = divergence(cells, &q, n);
            let scale = reaction.iter().map(|v| v.abs()).fold(0., f64::max);
            let residual = (0..n)
                .filter(|&i| free_map[i].is_some())
                .map(|i| reaction[i].abs())
                .fold(0., f64::max)
                / scale.max(f64::MIN_POSITIVE);
            dc_residual = dc_residual.max(residual);
            if !residual.is_finite() || residual > 1e-8 {
                return Err(error(
                    "FOIL_REDUCTION",
                    &format!("harmonic free residual {residual:e}"),
                ));
            }
            for v in &mut q {
                *v /= norm;
            }
            current.push(q);
        }
        for &i in island {
            voltage[anchor][i] = 1. - others.iter().map(|&j| voltage[j][i]).sum::<f64>();
        }
        components.push(modes);
    }
    if current.is_empty() {
        return Err(error(
            "FOIL_REDUCTION",
            "physical contacts excite no conductive harmonic mode",
        ));
    }
    let peak = factor.as_ref().map_or(plan, |x| x.2);
    Ok((
        Basis {
            voltage,
            current,
            retained,
            components,
            dc_residual,
        },
        peak,
        DcCorrection {
            free_map,
            free: next,
            owned_bytes: factor.as_ref().map_or(0, |x| x.3.saturating_sub(plan)),
            factor,
        },
    ))
}

struct Magnetic {
    actions: Vec<Vec<f64>>,
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
    let mode_reserve = training
        .map(TrainingPoint::iteration_limit)
        .transpose()?
        .unwrap_or(0)
        * 2;
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
    budget.check(sizes(n, supports, 0, field.planned_bytes())?)?;
    super::current_space::verify_foil_band(foils, max_frequency_hz, epsilon)?;
    let mut cells = Vec::with_capacity(supports);
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
            let mut a = sheet.incidence;
            // Construct the first P1 gradient by exact partition of unity.
            for k in 0..2 {
                a[0][k] = -(a[1][k] + a[2][k]);
            }
            cells.push(Cell {
                nodes: triangle.map(|i| foil.potential_dofs[i]),
                incidence: a,
                resistance: sheet.resistance_ohm,
            });
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
        budget,
        field.planned_bytes(),
        start,
        wall,
        mode_reserve,
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
    let training_resources = training.map(|point| (point, field, dc));
    let Magnetic {
        actions: magnetic_actions,
        reciprocity: max_magnetic_reciprocity,
        refinement: max_magnetic_refinement,
    } = magnetic(&prisms, &density, &basis.current, start, wall)?;
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
        ports: ports.to_vec(),
        full_potentials: n,
        max_frequency_hz,
        planned_bytes: peak,
        max_magnetic_reciprocity,
        max_magnetic_refinement,
        max_electric_reciprocity,
        dc_harmonic_residual,
        enrichment: None,
    };
    if let Some((training, field, dc)) = training_resources {
        space.train(
            training, &names, &dc, &field, &prisms, &density, budget, start, wall,
        )?;
    }
    Ok(space)
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
        .map(|q| {
            q.chunks_exact(2)
                .zip(cells)
                .flat_map(|(v, c)| [v[0] * c.resistance, v[1] * c.resistance])
                .collect()
        })
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
        let a = basis.retained.binary_search(&a).unwrap();
        let b = basis.retained.binary_search(&b).unwrap();
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

fn lift(basis: &[Vec<f64>], x: &[f64]) -> Vec<f64> {
    (0..basis[0].len())
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
        let residual = self.full_residual(w, &state[..n], im, drive)?.checked()?;
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
        let mut budget = budget;
        let requested = budget.max_memory_mi_b.unwrap_or(256.);
        let resident = self
            .full_potentials
            .saturating_mul(1024)
            .saturating_add(self.cells.len().saturating_mul(32768))
            .saturating_add(
                self.full_potentials
                    .saturating_add(2 * self.cells.len())
                    .saturating_mul(self.basis.voltage.len())
                    .saturating_mul(64),
            );
        let available = requested - resident as f64 / 1048576.;
        if !available.is_finite() || available <= 0. {
            return Err(error(
                "MEMORY_LIMIT",
                "reduced response and retained full-space actions exceed budget",
            ));
        }
        budget.max_memory_mi_b = Some(available);
        let mut system = System::compile(&self.model, self.ports.len(), Some(&budget))?;
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
    fn full_residual(&self, w: f64, re: &[f64], im: &[f64], drive: &[f64]) -> Result<FullResidual> {
        let nv = self.basis.voltage.len();
        let nr = self.full_potentials;
        let v = [
            lift(&self.basis.voltage, &re[..nv]),
            lift(&self.basis.voltage, &im[..nv]),
        ];
        let q = [
            lift(&self.basis.current, &re[nv..]),
            lift(&self.basis.current, &im[nv..]),
        ];
        let c = [
            lift(&self.charges, &re[..nv]),
            lift(&self.charges, &im[..nv]),
        ];
        let l = [
            lift(&self.magnetic_actions, &re[nv..]),
            lift(&self.magnetic_actions, &im[nv..]),
        ];
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
        let mut injected = vec![0.; nr];
        for (&[a, b], &i) in self.ports.iter().zip(drive) {
            injected[a] += i;
            injected[b] -= i;
        }
        let (mut kcl, mut kcl_scale, mut kvl, mut kvl_scale) = (0_f64, 0_f64, 0_f64, 0_f64);
        let mut kcl_rows = [vec![0.; nr], vec![0.; nr]];
        let mut kvl_rows = [
            vec![0.; 2 * self.cells.len()],
            vec![0.; 2 * self.cells.len()],
        ];
        for k in 0..2 {
            let s = if k == 0 { -1. } else { 1. };
            for i in 0..nr {
                let a = d[k][i];
                let b = s * w * c[1 - k][i];
                let z = if k == 0 { injected[i] } else { 0. };
                kcl_rows[k][i] = a + b - z;
                kcl = kcl.max(kcl_rows[k][i].abs());
                kcl_scale = kcl_scale.max(a.abs() + b.abs() + z.abs());
            }
            for (i, cell) in self.cells.iter().enumerate() {
                for axis in 0..2 {
                    let j = 2 * i + axis;
                    let a = cell.resistance * q[k][j];
                    let b = -g[k][j];
                    let z = s * w * l[1 - k][j];
                    kvl_rows[k][j] = a + b + z;
                    kvl = kvl.max(kvl_rows[k][j].abs());
                    kvl_scale = kvl_scale.max(a.abs() + b.abs() + z.abs());
                }
            }
        }
        let kcl_relative = kcl / kcl_scale.max(f64::MIN_POSITIVE);
        let kvl_relative = kvl / kvl_scale.max(f64::MIN_POSITIVE);
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
        })
    }
}

struct FullResidual {
    norm: Residual,
    kcl: [Vec<f64>; 2],
    kvl: [Vec<f64>; 2],
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

fn resistive_action(cells: &[Cell], q: &[f64]) -> Vec<f64> {
    q.chunks_exact(2)
        .zip(cells)
        .flat_map(|(v, c)| [v[0] * c.resistance, v[1] * c.resistance])
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
        prisms: &[Prism],
        density: &[f64],
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
                let full = self.full_residual(w, &state[..n], &state[n..], &training.drive)?;
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
                let ni = self.basis.current.len();
                let nv = self.basis.voltage.len();
                if nv == self.full_potentials && ni == 2 * self.cells.len() {
                    return Err(error(
                        "FOIL_ENRICHMENT",
                        "physical basis rank exhausted; coverage unverified",
                    ));
                }

                for (kcl, kvl) in full.kcl.iter().zip(&full.kvl) {
                    check_time(start, wall)?;
                    // A δi = -r_KCL on free nodes. The zero trace Dirichlet
                    // correction supplies a voltage bubble, while -R^-1 r_KVL
                    // retains the divergence-free induced current contribution.
                    let drop: Vec<_> = kvl
                        .chunks_exact(2)
                        .zip(&self.cells)
                        .flat_map(|(v, c)| [v[0] / c.resistance, v[1] / c.resistance])
                        .collect();
                    let rhs: Vec<_> = divergence(&self.cells, &drop, self.full_potentials)
                        .into_iter()
                        .zip(kcl)
                        .map(|(a, b)| a - b)
                        .collect();
                    let bubble = dc.solve(&rhs)?;
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
                    if self.basis.current.len() < 2 * self.cells.len() {
                        let actions: Vec<_> = self
                            .basis
                            .current
                            .iter()
                            .map(|q| resistive_action(&self.cells, q))
                            .collect();
                        if let Some((q, _)) = metric_mode(
                            candidate,
                            &self.basis.current,
                            &actions,
                            |q| Ok(resistive_action(&self.cells, q)),
                            true,
                        )? {
                            self.basis.current.push(q);
                        }
                    }
                }
                if self.basis.voltage.len() == nv && self.basis.current.len() == ni {
                    return Err(error(
                        "FOIL_ENRICHMENT",
                        &format!(
                    "no independent correction; unreduced KCL={:e}, KVL={:e}; coverage unverified",
                    full.norm.kcl_relative,full.norm.kvl_relative),
                    ));
                }
                if self.basis.current.len() > ni {
                    let added = magnetic(prisms, density, &self.basis.current[ni..], start, wall)?;
                    self.max_magnetic_reciprocity =
                        self.max_magnetic_reciprocity.max(added.reciprocity);
                    self.max_magnetic_refinement =
                        self.max_magnetic_refinement.max(added.refinement);
                    self.magnetic_actions.extend(added.actions);
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
            }
            unreachable!("bounded enrichment always returns at its final residual")
        })();
        outcome
            .map_err(|failure| format!("{failure}; enrichment limit={limit}; history={history:?}"))
    }
}
