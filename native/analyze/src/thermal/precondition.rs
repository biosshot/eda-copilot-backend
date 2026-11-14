//! Bounded sparse Cholesky preconditioning of the ordinary transient tangent.
//! The matrix-free physical operator additionally retains every finite barrel.
use faer::{
    dyn_stack::{MemBuffer, MemStack, StackReq},
    sparse::{
        linalg::{amd, cholesky::simplicial},
        SparseColMatRef, SymbolicSparseColMat,
    },
    Conj, MatMut, Par,
};
use std::{sync::Arc, time::Instant};
type Result<T> = std::result::Result<T, String>;
#[cfg(test)]
pub(super) mod blocked;
#[cfg(not(test))]
type Symbolic = simplicial::SymbolicSimplicialCholesky<usize>;
#[cfg(test)]
enum Symbolic {
    Simplicial(simplicial::SymbolicSimplicialCholesky<usize>),
    Supernodal(blocked::SuperPlan),
}
fn memory(e: impl std::fmt::Debug) -> String {
    format!("THERMAL_BUDGET: Preconditioner allocation {e:?}")
}
fn deadline(start: Instant, seconds: f64) -> Result<()> {
    if start.elapsed().as_secs_f64() > seconds {
        Err("THERMAL_DEADLINE: Sparse preconditioner deadline".into())
    } else {
        Ok(())
    }
}
pub(super) struct Pattern {
    rows: Vec<usize>,
    columns: Vec<usize>,
    positions: Vec<Option<usize>>,
    upper: SymbolicSparseColMat<usize>,
}
impl Pattern {
    pub fn new(n: usize, edges: &[(usize, usize)]) -> Self {
        let mut lower = vec![vec![]; n];
        for &(a, b) in edges {
            if a != usize::MAX && b != usize::MAX && a != b {
                lower[a.max(b)].push(a.min(b));
            }
        }
        let mut rows = vec![0];
        let mut columns = vec![];
        for row in &mut lower {
            row.sort_unstable();
            row.dedup();
            columns.extend_from_slice(row);
            rows.push(columns.len());
        }
        let positions = edges
            .iter()
            .map(|&(a, b)| {
                if a == usize::MAX || b == usize::MAX || a == b {
                    None
                } else {
                    let i = a.max(b);
                    Some(rows[i] + lower[i].binary_search(&a.min(b)).unwrap())
                }
            })
            .collect();
        // Lower CSR is the upper CSC of this symmetric matrix. Insert its diagonal.
        let mut row_idx = Vec::with_capacity(columns.len() + n);
        let mut col_ptr = vec![0];
        for i in 0..n {
            row_idx.extend_from_slice(&columns[rows[i]..rows[i + 1]]);
            row_idx.push(i);
            col_ptr.push(row_idx.len());
        }
        let upper = SymbolicSparseColMat::new_checked(n, n, col_ptr, None, row_idx);
        Self {
            rows,
            columns,
            positions,
            upper,
        }
    }
    #[cfg(test)]
    pub fn apply_assembled(&self, diag: &[f64], g: &[f64], x: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let values = self.values(diag, g);
        let mut y: Vec<_> = diag.iter().zip(x).map(|(a, b)| a * b).collect();
        let mut magnitude: Vec<_> = y.iter().map(|v| v.abs()).collect();
        for i in 0..x.len() {
            for p in self.rows[i]..self.rows[i + 1] {
                let j = self.columns[p];
                let a = values[p + i];
                y[i] += a * x[j];
                y[j] += a * x[i];
                magnitude[i] += (a * x[j]).abs();
                magnitude[j] += (a * x[i]).abs();
            }
        }
        (y, magnitude)
    }
    fn values(&self, diag: &[f64], g: &[f64]) -> Vec<f64> {
        let mut lower = vec![0.; self.columns.len()];
        for (p, g) in self.positions.iter().zip(g) {
            if let Some(p) = p {
                lower[*p] -= g;
            }
        }
        let mut values = vec![0.; lower.len() + diag.len()];
        for i in 0..diag.len() {
            for p in self.rows[i]..self.rows[i + 1] {
                values[p + i] = lower[p];
            }
            values[self.rows[i + 1] + i] = diag[i];
        }
        values
    }
}
#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Planning {
    pub nonzeros: usize,
    pub fill: usize,
    pub bytes: usize,
    pub cache_slots: usize,
    pub analysis_bytes: usize,
    pub contribution_bytes: usize,
    pub symbolic_index_bytes: usize,
    pub numeric_factor_bytes: usize,
    pub workspace_bytes: usize,
    pub solve_buffer_bytes: usize,
}
pub(super) struct Plan {
    pattern: Arc<Pattern>,
    matrix: SymbolicSparseColMat<usize>,
    mapping: Vec<usize>,
    permutation: Vec<usize>,
    symbolic: Symbolic,
    pub memory_bytes: usize,
    pub cache_slots: usize,
}
impl Plan {
    pub fn prepare(
        pattern: Arc<Pattern>,
        max_bytes: usize,
        cache_slots: usize,
        planning: &mut Planning,
        start: Instant,
        seconds: f64,
    ) -> Result<Option<Self>> {
        let _phase=super::profile::timer(super::profile::Phase::Symbolic);
        let n = pattern.rows.len() - 1;
        let nnz = pattern.columns.len() + n;
        planning.analysis_bytes = n.saturating_mul(192).saturating_add(nnz.saturating_mul(64));
        planning.contribution_bytes = pattern.positions.len().saturating_mul(32);
        planning.solve_buffer_bytes = n.saturating_mul(64);
        let base = planning
            .analysis_bytes
            .saturating_add(planning.contribution_bytes)
            .saturating_add(planning.solve_buffer_bytes);
        planning.nonzeros = nnz;
        planning.bytes = base;
        if base > max_bytes {
            return Ok(None);
        }
        deadline(start, seconds)?;
        let request = StackReq::any_of(&[
            amd::order_scratch::<usize>(n, nnz),
            simplicial::prefactorize_symbolic_cholesky_scratch::<usize>(n, nnz),
            simplicial::factorize_simplicial_symbolic_cholesky_scratch::<usize>(n),
        ]);
        planning.workspace_bytes = request.unaligned_bytes_required();
        planning.bytes = base.saturating_add(planning.workspace_bytes);
        if planning.bytes > max_bytes {
            return Ok(None);
        }
        let mut scratch = MemBuffer::try_new(request).map_err(memory)?;
        let mut permutation = vec![0; n];
        let mut inverse = vec![0; n];
        amd::order(
            &mut permutation,
            &mut inverse,
            pattern.upper.as_ref(),
            Default::default(),
            MemStack::new(&mut scratch),
        )
        .map_err(memory)?;
        deadline(start, seconds)?;
        let mut entries = Vec::with_capacity(nnz);
        for old in 0..n {
            for p in pattern.rows[old]..pattern.rows[old + 1] {
                let a = inverse[old];
                let b = inverse[pattern.columns[p]];
                entries.push((a.max(b), a.min(b), p + old));
            }
            entries.push((inverse[old], inverse[old], pattern.rows[old + 1] + old));
        }
        entries.sort_unstable_by_key(|&(c, r, _)| (c, r));
        let mut col_ptr = vec![0; n + 1];
        let mut row_idx = Vec::with_capacity(nnz);
        let mut mapping = Vec::with_capacity(nnz);
        for (c, r, p) in entries {
            col_ptr[c + 1] += 1;
            row_idx.push(r);
            mapping.push(p);
        }
        for i in 0..n {
            col_ptr[i + 1] += col_ptr[i];
        }
        let matrix = SymbolicSparseColMat::new_checked(n, n, col_ptr, None, row_idx);
        let mut tree = vec![0isize; n];
        let mut counts = vec![0usize; n];
        let etree = simplicial::prefactorize_symbolic_cholesky(
            &mut tree,
            &mut counts,
            matrix.as_ref(),
            MemStack::new(&mut scratch),
        );
        #[cfg(test)]
        if blocked::selected() {
            let symbolic = blocked::SuperPlan::prepare(
                matrix.as_ref(), etree, &counts, base, max_bytes, cache_slots, planning,
            )?;
            deadline(start, seconds)?;
            let Some(symbolic) = symbolic else { return Ok(None) };
            return Ok(Some(Self {
                pattern, matrix, mapping, permutation,
                memory_bytes: planning.bytes, cache_slots: planning.cache_slots,
                symbolic: Symbolic::Supernodal(symbolic),
            }));
        }
        // Count fill before allocation, then choose the largest bounded cache
        // that fits. Reducing cache capacity changes only preconditioner reuse.
        let fill = counts.iter().fold(0usize, |sum, x| sum.saturating_add(*x));
        let numeric = simplicial::factorize_simplicial_numeric_llt_scratch::<usize, f64>(n);
        let workspace = request
            .unaligned_bytes_required()
            .max(numeric.unaligned_bytes_required());
        let estimate = |slots: usize| {
            base.saturating_add(fill.saturating_mul(8 * (1 + slots)))
                .saturating_add(workspace)
        };
        planning.fill = fill;
        planning.symbolic_index_bytes = fill.saturating_mul(8);
        planning.workspace_bytes = workspace;
        let admitted = (1..=cache_slots)
            .rev()
            .find(|slots| estimate(*slots) <= max_bytes);
        let Some(cache_slots) = admitted else {
            planning.bytes = estimate(1);
            return Ok(None);
        };
        let memory_bytes = estimate(cache_slots);
        planning.bytes = memory_bytes;
        planning.cache_slots = cache_slots;
        planning.numeric_factor_bytes = fill.saturating_mul(8).saturating_mul(cache_slots);
        deadline(start, seconds)?;
        let symbolic = simplicial::factorize_simplicial_symbolic_cholesky(
            matrix.as_ref(),
            etree,
            &counts,
            MemStack::new(&mut scratch),
        )
        .map_err(memory)?;
        #[cfg(test)]
        let symbolic = Symbolic::Simplicial(symbolic);
        deadline(start, seconds)?;
        Ok(Some(Self {
            pattern,
            matrix,
            mapping,
            permutation,
            symbolic,
            memory_bytes,
            cache_slots,
        }))
    }
    pub fn factor(
        self: &Arc<Self>,
        diag: &[f64],
        g: &[f64],
        start: Instant,
        seconds: f64,
    ) -> Result<Option<Factor>> {
        let _phase=super::profile::timer(super::profile::Phase::NumericFactor);
        deadline(start, seconds)?;
        #[cfg(test)] let packing_start=Instant::now();
        let original = self.pattern.values(diag, g);
        let matrix_values: Vec<_> = self.mapping.iter().map(|p| original[*p]).collect();
        let matrix = SparseColMatRef::new(self.matrix.as_ref(), &matrix_values);
        #[cfg(test)] blocked::record("packingMs",packing_start);
        #[cfg(test)]
        if let Symbolic::Supernodal(s) = &self.symbolic {
            let factor=s.factor(matrix, self.clone())?;
            deadline(start, seconds)?;
            return Ok(factor);
        }
        #[cfg(test)]
        let Symbolic::Simplicial(symbolic) = &self.symbolic else { unreachable!() };
        #[cfg(not(test))]
        let symbolic = &self.symbolic;
        #[cfg(test)] let numeric_start=Instant::now();
        let mut values = vec![0.; symbolic.len_val()];
        let mut scratch =
            MemBuffer::try_new(simplicial::factorize_simplicial_numeric_llt_scratch::<
                usize,
                f64,
            >(diag.len()))
            .map_err(memory)?;
        // Default LLT regularization is disabled. A failed positive pivot falls back
        // to Jacobi; it never inserts a path into either M or the physical model.
        match simplicial::factorize_simplicial_numeric_llt(
            &mut values,
            matrix,
            Default::default(),
            symbolic,
            MemStack::new(&mut scratch),
        ) {
            Ok(info)
                if info.dynamic_regularization_count == 0
                    && values.iter().all(|v| v.is_finite()) => {}
            _ => return Ok(None),
        }
        #[cfg(test)] blocked::record("numericMs",numeric_start);
        deadline(start, seconds)?;
        Ok(Some(Factor {
            plan: self.clone(),
            values,
        }))
    }
}
pub(super) struct Factor {
    plan: Arc<Plan>,
    values: Vec<f64>,
}
impl Factor {
    pub fn apply(&self, rhs: &[f64]) -> Result<Vec<f64>> {
        let workspace_phase=super::profile::timer(super::profile::Phase::ApplyWorkspace);
        #[cfg(test)] let permutation_start=Instant::now();
        let n = rhs.len();
        let mut z: Vec<_> = self.plan.permutation.iter().map(|i| rhs[*i]).collect();
        #[cfg(test)] blocked::record("permutationMs",permutation_start);
        #[cfg(test)]
        if let Symbolic::Supernodal(s) = &self.plan.symbolic {
            drop(workspace_phase);
            s.solve(&self.values, &mut z, 1)?;
            let permutation_start=Instant::now();
            let mut out = vec![0.; n];
            for (i, p) in self.plan.permutation.iter().enumerate() { out[*p] = z[i]; }
            blocked::record("permutationMs",permutation_start);
            if out.iter().any(|v| !v.is_finite()) { return Err("THERMAL_PRECONDITIONER: Nonfinite sparse solve".into()); }
            return Ok(out);
        }
        #[cfg(test)]
        let Symbolic::Simplicial(symbolic) = &self.plan.symbolic else { unreachable!() };
        #[cfg(not(test))]
        let symbolic = &self.plan.symbolic;
        let mut scratch = MemBuffer::try_new(symbolic.solve_in_place_scratch::<f64>(1))
            .map_err(memory)?;
        drop(workspace_phase);
        let triangular_phase=super::profile::timer(super::profile::Phase::TriangularSolve);
        #[cfg(test)] let triangular_start=Instant::now();
        simplicial::SimplicialLltRef::new(symbolic, &self.values)
            .solve_in_place_with_conj(
                Conj::No,
                MatMut::from_column_major_slice_mut(&mut z, n, 1),
                Par::Seq,
                MemStack::new(&mut scratch),
            );
        #[cfg(test)] blocked::record("triangularMs",triangular_start);
        drop(triangular_phase);
        let _workspace_phase=super::profile::timer(super::profile::Phase::ApplyWorkspace);
        #[cfg(test)] let permutation_start=Instant::now();
        let mut out = vec![0.; n];
        for (i, p) in self.plan.permutation.iter().enumerate() {
            out[*p] = z[i];
        }
        #[cfg(test)] blocked::record("permutationMs",permutation_start);
        if out.iter().any(|v| !v.is_finite()) {
            return Err("THERMAL_PRECONDITIONER: Nonfinite sparse solve".into());
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_factor_reference_and_memory_admission() {
        let pattern = Arc::new(Pattern::new(3, &[(0, 1), (1, 2)]));
        let mut planning = Planning::default();
        assert!(
            Plan::prepare(pattern.clone(), 1, 3, &mut planning, Instant::now(), 10.)
                .unwrap()
                .is_none()
        );
        let plan = Arc::new(
            Plan::prepare(pattern, 1_000_000, 3, &mut planning, Instant::now(), 10.)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(
            planning.bytes,
            planning.analysis_bytes
                + planning.contribution_bytes
                + planning.solve_buffer_bytes
                + planning.symbolic_index_bytes
                + planning.numeric_factor_bytes
                + planning.workspace_bytes
        );
        let single_budget = planning.bytes - 16 * planning.fill;
        let single = Plan::prepare(
            Arc::new(Pattern::new(3, &[(0, 1), (1, 2)])),
            single_budget,
            3,
            &mut Planning::default(),
            Instant::now(),
            10.,
        )
        .unwrap()
        .unwrap();
        assert_eq!(single.cache_slots, 1);
        assert!(single.memory_bytes <= single_budget);
        let factor = plan
            .factor(&[2., 3., 2.], &[1., 1.], Instant::now(), 10.)
            .unwrap()
            .unwrap();
        let solved = factor.apply(&[0., 2., 4.]).unwrap();
        for (a, b) in solved.iter().zip([1., 2., 3.]) {
            assert!((a - b).abs() < 1e-12);
        }
        let pattern = Arc::new(Pattern::new(2, &[(0, 1)]));
        let plan = Arc::new(
            Plan::prepare(pattern, 1_000_000, 3, &mut planning, Instant::now(), 10.)
                .unwrap()
                .unwrap(),
        );
        assert!(plan
            .factor(&[1., 1.], &[2.], Instant::now(), 10.)
            .unwrap()
            .is_none());
    }
}
