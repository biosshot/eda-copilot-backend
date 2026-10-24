//! Sparse dielectric Schur apply with explicit metal trace and far boundary.
//! All metal potentials remain; no conductor is selected as a hidden ground.
use super::{
    error,
    mesh::{self, LayeredMesh},
    AllocationBudget, Result,
};
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    sparse::{linalg::cholesky, SparseColMat, Triplet},
    Conj, Mat, Par, Side,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct TraceRow {
    pub node: usize,
    pub weights: Vec<(usize, f64)>,
}

#[derive(Debug)]
pub struct Boundary {
    pub potential_count: usize,
    pub metal_trace: Vec<TraceRow>,
    pub far_zero_nodes: Vec<usize>,
    /// Explicitly permit natural insulated boundary faces, e.g. a plate coupon.
    /// Real infinity truncation must assign the complete external far boundary.
    pub insulated_external: bool,
}

pub struct CapacitanceOperator<'a> {
    mesh: &'a LayeredMesh,
    trace: BTreeMap<usize, Vec<(usize, f64)>>,
    far: BTreeSet<usize>,
    free_map: Vec<usize>,
    potential_count: usize,
    free: usize,
    terms: Vec<(usize, usize, f64)>,
    symbolic: Option<cholesky::SymbolicCholesky<usize>>,
    factor_values: Vec<f64>,
    pub planned_peak_bytes: usize,
    pub factor_value_count: usize,
}

#[derive(Debug)]
pub struct CapacitanceAction {
    pub charges_c: Vec<f64>,
    pub nodal_potentials_v: Vec<f64>,
    pub electric_fields_v_per_m: Vec<[f64; 3]>,
    pub energy_j: f64,
    pub far_charge_c: f64,
    pub normalized_free_residual: f64,
    pub normalized_charge_balance: f64,
    pub normalized_energy_reaction_error: f64,
}

impl<'a> CapacitanceOperator<'a> {
    pub fn new(
        mesh: &'a LayeredMesh,
        boundary: Boundary,
        budget: AllocationBudget,
    ) -> Result<Self> {
        let n = mesh.nodes_m.len();
        let t = mesh.tetrahedra.len();
        let base = n
            .checked_mul(2048)
            .and_then(|v| v.checked_add(t.checked_mul(1024)?))
            .and_then(|v| v.checked_add(mesh.planned_peak_bytes))
            .ok_or_else(|| error("MEMORY_LIMIT", "dielectric plan overflow"))?;
        budget.check(base)?;
        if n > budget.max_nodes || t > budget.max_tetrahedra || boundary.potential_count == 0 {
            return Err(error(
                "FIELD_INPUT",
                "mesh/potential count outside explicit budget",
            ));
        }
        let external: BTreeSet<_> = mesh
            .external_faces
            .iter()
            .flat_map(|f| f.iter().copied())
            .collect();
        let copper: BTreeSet<_> = mesh
            .copper_interfaces
            .iter()
            .flat_map(|f| f.nodes)
            .collect();
        let active: BTreeSet<_> = mesh.tetrahedra.iter().flat_map(|t| t.nodes).collect();
        let mut trace = BTreeMap::new();
        let mut represented = BTreeSet::new();
        for row in boundary.metal_trace {
            if row.node >= n
                || !active.contains(&row.node)
                || (!external.contains(&row.node) && !copper.contains(&row.node))
                || row.weights.is_empty()
                || trace.contains_key(&row.node)
            {
                return Err(error(
                    "TRACE_INPUT",
                    "duplicate/nonboundary/empty metal trace row",
                ));
            }
            let mut columns = BTreeSet::new();
            let mut sum = 0.;
            for &(col, value) in &row.weights {
                if col >= boundary.potential_count
                    || !value.is_finite()
                    || value < 0.
                    || !columns.insert(col)
                {
                    return Err(error(
                        "TRACE_INPUT",
                        "invalid/duplicate metal trace support",
                    ));
                }
                if value > 0. {
                    represented.insert(col);
                }
                sum += value;
            }
            if (sum - 1.).abs() > 1e-12 {
                return Err(error(
                    "TRACE_PARTITION",
                    "metal trace must preserve constant potential",
                ));
            }
            trace.insert(row.node, row.weights);
        }
        if represented.len() != boundary.potential_count
            || copper.iter().any(|i| !trace.contains_key(i))
        {
            return Err(error(
                "TRACE_INCOMPLETE",
                "all metal interface vertices and potential modes need trace support",
            ));
        }
        let mut far = BTreeSet::new();
        for i in boundary.far_zero_nodes {
            if !external.contains(&i) || trace.contains_key(&i) || !far.insert(i) {
                return Err(error(
                    "FAR_BOUNDARY",
                    "far zero nodes must be unique external nonmetal vertices",
                ));
            }
        }
        if !boundary.insulated_external
            && external
                .iter()
                .any(|i| !trace.contains_key(i) && !far.contains(i))
        {
            return Err(error(
                "FAR_INCOMPLETE",
                "external boundary lacks explicit far/electrode condition",
            ));
        }
        let mut free_map = vec![usize::MAX; n];
        let mut free = 0;
        for &i in &active {
            if !trace.contains_key(&i) && !far.contains(&i) {
                free_map[i] = free;
                free += 1;
            }
        }
        let mut triplets = Vec::new();
        let mut terms = Vec::with_capacity(t * 16);
        for tet in &mesh.tetrahedra {
            let k = mesh::stiffness(tet)?;
            for i in 0..4 {
                for j in 0..4 {
                    let (a, b) = (tet.nodes[i], tet.nodes[j]);
                    let value = k[i][j] / super::super::EPS0;
                    terms.push((a, b, value));
                    if free_map[a] != usize::MAX && free_map[b] != usize::MAX {
                        triplets.push(Triplet::new(free_map[a], free_map[b], value));
                    }
                }
            }
        }
        let (mut symbolic, mut factor_values, mut plan) = (None, Vec::new(), base);
        if free > 0 {
            let matrix = SparseColMat::<usize, f64>::try_new_from_triplets(free, free, &triplets)
                .map_err(|e| error("FIELD_MATRIX", &e.to_string()))?;
            drop(triplets);
            let s = cholesky::factorize_symbolic_cholesky(
                matrix.symbolic(),
                Side::Lower,
                Default::default(),
                Default::default(),
            )
            .map_err(|e| error("FIELD_MATRIX", &e.to_string()))?;
            let scratch = s.factorize_numeric_llt_scratch::<f64>(Par::Seq, Default::default());
            let solve = s.solve_in_place_scratch::<f64>(1, Par::Seq);
            plan = base
                .checked_add(
                    s.len_val()
                        .checked_mul(8)
                        .ok_or_else(|| error("MEMORY_LIMIT", "factor plan overflow"))?,
                )
                .and_then(|v| {
                    v.checked_add(
                        scratch
                            .unaligned_bytes_required()
                            .max(solve.unaligned_bytes_required()),
                    )
                })
                .ok_or_else(|| error("MEMORY_LIMIT", "factor plan overflow"))?;
            budget.check(plan)?;
            factor_values = vec![0.; s.len_val()];
            let mut workspace = MemBuffer::try_new(scratch)
                .map_err(|_| error("MEMORY_LIMIT", "factor scratch allocation"))?;
            s.factorize_numeric_llt(
                &mut factor_values,
                matrix.as_ref(),
                Side::Lower,
                Default::default(),
                Par::Seq,
                MemStack::new(&mut workspace),
                Default::default(),
            )
            .map_err(|_| {
                error(
                    "FIELD_SINGULAR",
                    "dielectric free matrix is not positive definite; no gmin/ground added",
                )
            })?;
            symbolic = Some(s);
        }
        let factor_value_count = factor_values.len();
        Ok(Self {
            mesh,
            trace,
            far,
            free_map,
            potential_count: boundary.potential_count,
            free,
            terms,
            symbolic,
            factor_values,
            planned_peak_bytes: plan,
            factor_value_count,
        })
    }

    pub fn apply(&self, volts: &[f64]) -> Result<CapacitanceAction> {
        if volts.len() != self.potential_count || volts.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "FIELD_VOLTAGE",
                "finite full potential vector required",
            ));
        }
        // Only an entirely insulated exterior has a constant-potential gauge.
        // Infinity truncation has explicit far nodes and preserves common mode.
        let gauge = if self.far.is_empty() { volts[0] } else { 0. };
        let shifted: Vec<_> = volts.iter().map(|v| v - gauge).collect();
        let mut phi = vec![0.; self.mesh.nodes_m.len()];
        for (&node, row) in &self.trace {
            phi[node] = row.iter().map(|&(col, w)| shifted[col] * w).sum();
        }
        if self.free > 0 {
            let mut rhs = Mat::<f64>::zeros(self.free, 1);
            for &(a, b, k) in &self.terms {
                if self.free_map[a] != usize::MAX && self.free_map[b] == usize::MAX {
                    rhs[(self.free_map[a], 0)] -= k * phi[b];
                }
            }
            let s = self.symbolic.as_ref().unwrap();
            let scratch = s.solve_in_place_scratch::<f64>(1, Par::Seq);
            let mut workspace = MemBuffer::try_new(scratch)
                .map_err(|_| error("MEMORY_LIMIT", "solve scratch allocation"))?;
            cholesky::LltRef::new(s, &self.factor_values).solve_in_place_with_conj(
                Conj::No,
                rhs.as_mut(),
                Par::Seq,
                MemStack::new(&mut workspace),
            );
            for (i, &map) in self.free_map.iter().enumerate() {
                if map != usize::MAX {
                    phi[i] = rhs[(map, 0)];
                }
            }
        }
        let eps0 = super::super::EPS0;
        let mut reaction = vec![0.; phi.len()];
        let mut scale = vec![0.; phi.len()];
        let mut twice_energy = 0.;
        for &(a, b, k) in &self.terms {
            let q = k * phi[b] * eps0;
            reaction[a] += q;
            scale[a] += q.abs();
            twice_energy += phi[a] * q;
        }
        let mut charges = vec![0.; volts.len()];
        for (&node, row) in &self.trace {
            for &(col, w) in row {
                charges[col] += w * reaction[node];
            }
        }
        let far_charge_c = self.far.iter().map(|&i| reaction[i]).sum::<f64>();
        let residual = self
            .free_map
            .iter()
            .enumerate()
            .filter(|(_, m)| **m != usize::MAX)
            .map(|(i, _)| reaction[i].abs() / scale[i].max(f64::MIN_POSITIVE))
            .fold(0., f64::max);
        let charge_scale = charges.iter().map(|x| x.abs()).sum::<f64>() + far_charge_c.abs();
        let balance = (charges.iter().sum::<f64>() + far_charge_c).abs()
            / charge_scale.max(f64::MIN_POSITIVE);
        let boundary_work = shifted
            .iter()
            .zip(&charges)
            .map(|(v, q)| v * q)
            .sum::<f64>();
        let mut fields = Vec::with_capacity(self.mesh.tetrahedra.len());
        for tet in &self.mesh.tetrahedra {
            fields.push(std::array::from_fn(|axis| {
                -(0..4)
                    .map(|i| phi[tet.nodes[i]] * tet.gradients_per_m[i][axis])
                    .sum::<f64>()
            }));
        }
        let field_energy_twice = self
            .mesh
            .tetrahedra
            .iter()
            .zip(&fields)
            .map(|(t, e)| eps0 * t.epsilon_r * t.volume_m3 * e.iter().map(|x| x * x).sum::<f64>())
            .sum::<f64>();
        let energy_error = (field_energy_twice - boundary_work).abs()
            / field_energy_twice
                .abs()
                .max(boundary_work.abs())
                .max(f64::MIN_POSITIVE);
        if field_energy_twice == 0. && fields.iter().flatten().any(|x| *x != 0.) {
            return Err(error(
                "FIELD_RANGE",
                "nonzero recovered field energy underflow",
            ));
        }
        if phi
            .iter()
            .chain(charges.iter())
            .chain(fields.iter().flatten())
            .any(|x| !x.is_finite())
            || ![residual, balance, energy_error, twice_energy]
                .iter()
                .all(|x| x.is_finite())
            || residual > 1e-8
            || balance > 1e-8
            || energy_error > 1e-8
            || field_energy_twice < 0.
        {
            return Err(error(
                "FIELD_BALANCE",
                "free KCL, full charge or energy/reaction gate failed",
            ));
        }
        for (i, p) in phi.iter_mut().enumerate() {
            if self.free_map[i] != usize::MAX || self.trace.contains_key(&i) {
                *p += gauge;
            }
        }
        if phi.iter().any(|x| !x.is_finite()) {
            return Err(error("FIELD_RANGE", "potential gauge recovery overflow"));
        }
        Ok(CapacitanceAction {
            charges_c: charges,
            nodal_potentials_v: phi,
            electric_fields_v_per_m: fields,
            energy_j: field_energy_twice / 2.,
            far_charge_c,
            normalized_free_residual: residual,
            normalized_charge_balance: balance,
            normalized_energy_reaction_error: energy_error,
        })
    }
}
