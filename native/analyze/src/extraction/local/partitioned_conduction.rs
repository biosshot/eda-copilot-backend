//! Exact owner-interior elimination for the real conductive correction only.
use super::{
    check_time, conductive, divergence, error, sparse_potential, AllocationBudget, Cell, Result,
    Sum,
};
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    sparse::{linalg::cholesky, SparseColMat, Triplet},
    Conj, Mat,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const RESIDUAL_LIMIT: f64 = 1e-8;
type Factor = (cholesky::SymbolicCholesky<usize>, Vec<f64>, usize, usize);

struct Owner {
    interiors: Vec<usize>,
    traces: Vec<usize>,
    /// One sparse interior column of K_IΓ for each owner trace coordinate.
    coupling: Vec<Vec<(usize, f64)>>,
    factor: Option<Factor>,
}

pub(super) struct Backend {
    owners: Vec<Owner>,
    trace_dofs: Vec<usize>,
    trace_factor: Option<Factor>,
    trace_nonzeros: usize,
    pub owned_bytes: usize,
    max_original_residual: AtomicU64,
    start: Instant,
    wall: Duration,
}

fn plan(base: usize, count: usize, bytes: usize) -> Result<usize> {
    base.checked_add(
        count
            .checked_mul(bytes)
            .ok_or_else(|| error("MEMORY_LIMIT", "shared trace byte plan overflow"))?,
    )
    .ok_or_else(|| error("MEMORY_LIMIT", "shared trace byte plan overflow"))
}

fn factor(
    size: usize,
    entries: &[Triplet<usize, usize, f64>],
    live: usize,
    budget: AllocationBudget,
    start: Instant,
    wall: Duration,
) -> Result<Option<Factor>> {
    if size == 0 {
        return Ok(None);
    }
    check_time(start, wall)?;
    if entries.iter().any(|entry| !entry.val.is_finite()) {
        return Err(error(
            "SHARED_RETURN_SCHUR",
            "nonfinite conductive coefficient",
        ));
    }
    let matrix = SparseColMat::try_new_from_triplets(size, size, entries)
        .map_err(|e| error("SHARED_RETURN_SCHUR", &e.to_string()))?;
    let result = sparse_potential::factor(&matrix, live, budget, &[])?;
    check_time(start, wall)?;
    Ok(Some(result))
}

fn apply_factor(factor: &Option<Factor>, rhs: &mut Mat<f64>) -> Result<()> {
    if let Some((symbolic, values, _, _)) = factor {
        let par = super::super::parallel::current();
        let mut scratch = MemBuffer::try_new(symbolic.solve_in_place_scratch::<f64>(1, par))
            .map_err(|_| error("MEMORY_LIMIT", "shared return sparse solve scratch"))?;
        cholesky::LltRef::new(symbolic, values).solve_in_place_with_conj(
            Conj::No,
            rhs.as_mut(),
            par,
            MemStack::new(&mut scratch),
        );
    }
    Ok(())
}

impl Backend {
    pub fn factor(
        cells: &[Cell],
        free: &[Option<usize>],
        owner_count: usize,
        owner_of: impl Fn(usize) -> Option<usize>,
        shared: &[usize],
        base: usize,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<(usize, Self)> {
        check_time(start, wall)?;
        if !(2..=16).contains(&owner_count) {
            return Err(error(
                "SHARED_RETURN_SCHUR",
                "2..16 original owners required",
            ));
        }
        // A four-term column can create up to four owner trace set entries.
        let metadata = plan(plan(4096, free.len(), 128)?, cells.len(), 368)?;
        // Full solution/trace/owner RHS work, original conductive q, original
        // divergence accumulator plus result. All coexist with the factors.
        let solve_vectors = plan(plan(0, free.len(), 128)?, cells.len(), 16)?;
        let mut live = plan(plan(base, metadata, 1)?, solve_vectors, 1)?;
        eprintln!("P7_MIXED_MEMORY_STAGE stage=partitioned-factor-admission owners={owner_count} baseBytes={base} metadataBytes={metadata} solveVectorBytes={solve_vectors} plannedBytes={live} budgetBytes={}",budget.max_bytes);
        budget.check(live).map_err(|failure|format!("{failure}; stage=partitioned-factor-admission baseBytes={base} metadataBytes={metadata} solveVectorBytes={solve_vectors} plannedBytes={live} budgetBytes={}",budget.max_bytes))?;

        let mut trace_index = vec![None; free.len()];
        let mut trace_dofs = Vec::new();
        for &dof in shared {
            if dof >= free.len() || trace_index[dof].is_some() {
                return Err(error(
                    "SHARED_RETURN_SCHUR",
                    "invalid original trace coordinate",
                ));
            }
            if free[dof].is_some() {
                trace_index[dof] = Some(trace_dofs.len());
                trace_dofs.push(dof);
            }
        }
        let mut by_owner = vec![Vec::new(); owner_count];
        let mut owner_traces = vec![BTreeSet::new(); owner_count];
        let mut interiors = vec![Vec::new(); owner_count];
        let mut interior_owner = vec![None; free.len()];
        for (index, cell) in cells.iter().enumerate() {
            if index % 1024 == 0 {
                check_time(start, wall)?;
            }
            let owner = owner_of(index)
                .filter(|&owner| owner < owner_count)
                .ok_or_else(|| error("SHARED_RETURN_SCHUR", "missing physical current owner"))?;
            if cell.terms.is_empty()
                || cell.terms.len() > 4
                || !cell.resistance.is_finite()
                || cell.resistance <= 0.
                || cell.terms.iter().any(|(_, value)| !value.is_finite())
            {
                return Err(error(
                    "SHARED_RETURN_SCHUR",
                    "invalid original conductive current stencil",
                ));
            }
            by_owner[owner].push(index);
            for &(dof, _) in &cell.terms {
                if dof >= free.len() {
                    return Err(error(
                        "SHARED_RETURN_SCHUR",
                        "original potential index outside conductive coordinates",
                    ));
                }
                if free[dof].is_none() {
                    continue;
                }
                if let Some(trace) = trace_index[dof] {
                    owner_traces[owner].insert(trace);
                } else {
                    match interior_owner[dof] {
                        None => {
                            interior_owner[dof] = Some(owner);
                            interiors[owner].push(dof);
                        }
                        Some(previous) if previous == owner => {}
                        _ => {
                            return Err(error(
                                "SHARED_RETURN_SCHUR",
                                "unwelded shared interior coordinate",
                            ))
                        }
                    }
                }
            }
        }
        if free.iter().enumerate().any(|(dof, value)| {
            value.is_some() && trace_index[dof].is_none() && interior_owner[dof].is_none()
        }) {
            return Err(error(
                "SHARED_RETURN_SCHUR",
                "unrepresented free conductive coordinate",
            ));
        }

        // The only fill materialized is on owner trace cliques. Charge the
        // BTreeMap, upper triplets and conversion together before allocation.
        let trace_fill = owner_traces
            .iter()
            .try_fold(0usize, |sum, traces| {
                sum.checked_add(traces.len().checked_mul(traces.len())?)
            })
            .ok_or_else(|| error("MEMORY_LIMIT", "shared trace fill plan overflow"))?;
        let trace_workspace = plan(0, trace_fill, 160)?;
        budget.check(plan(live, trace_workspace, 1)?)?;
        let mut peak = plan(live, trace_workspace, 1)?;
        let mut schur = BTreeMap::<(usize, usize), Sum>::new();
        let mut owners = Vec::with_capacity(owner_count);
        let mut local = vec![None; free.len()];
        for ((owned_cells, interiors), traces) in
            by_owner.into_iter().zip(interiors).zip(owner_traces)
        {
            check_time(start, wall)?;
            let traces: Vec<_> = traces.into_iter().collect();
            for (i, &dof) in interiors.iter().enumerate() {
                local[dof] = Some(i);
            }
            let temporary = plan(0, owned_cells.len(), 16 * 128)?;
            let phase = plan(plan(live, trace_workspace, 1)?, temporary, 1)?;
            budget.check(phase)?;
            peak = peak.max(phase);
            let mut entries = Vec::new();
            let mut cross = BTreeMap::<(usize, usize), Sum>::new();
            for index in owned_cells {
                check_time(start, wall)?;
                let cell = &cells[index];
                for &(a, av) in &cell.terms {
                    for &(b, bv) in &cell.terms {
                        let value = av * bv / cell.resistance;
                        if !value.is_finite() {
                            return Err(error(
                                "SHARED_RETURN_SCHUR",
                                "nonfinite original conductive coefficient",
                            ));
                        }
                        match (local[a], local[b], trace_index[a], trace_index[b]) {
                            (Some(i), Some(j), _, _) if i <= j => {
                                entries.push(Triplet::new(i, j, value))
                            }
                            (Some(i), _, _, Some(t)) => {
                                let entry = cross.entry((t, i)).or_default();
                                *entry = entry.add(Sum::from(value));
                            }
                            (_, _, Some(i), Some(j)) if i <= j => {
                                let entry = schur.entry((i, j)).or_default();
                                *entry = entry.add(Sum::from(value));
                            }
                            _ => {}
                        }
                    }
                }
            }
            let coupling_bytes = plan(plan(0, cross.len(), 48)?, traces.len(), 48)?;
            let mut coupling = vec![Vec::new(); traces.len()];
            for ((t, i), value) in cross {
                let j = traces
                    .binary_search(&t)
                    .map_err(|_| error("SHARED_RETURN_SCHUR", "missing owner trace column"))?;
                let value = value.value();
                if !value.is_finite() {
                    return Err(error(
                        "SHARED_RETURN_SCHUR",
                        "nonfinite assembled owner coupling",
                    ));
                }
                coupling[j].push((i, value));
            }
            let factor_base = plan(phase, coupling_bytes, 1)?;
            budget.check(factor_base)?;
            let interior_factor =
                factor(interiors.len(), &entries, factor_base, budget, start, wall)?;
            drop(entries);
            if let Some(f) = &interior_factor {
                peak = peak.max(f.2);
                live = plan(
                    live,
                    f.3.checked_sub(factor_base)
                        .ok_or_else(|| error("MEMORY_LIMIT", "invalid interior factor plan"))?,
                    1,
                )?;
            }
            live = plan(live, coupling_bytes, 1)?;
            budget.check(plan(live, trace_workspace, 1)?)?;
            // Stream one K_II^{-1} K_IΓ column. Do not retain dense response
            // columns or explicitly invert any interior factor.
            if interior_factor.is_some() {
                let mut rhs = Mat::zeros(interiors.len(), 1);
                for (j, &tj) in traces.iter().enumerate() {
                    if coupling[j].is_empty() {
                        continue;
                    }
                    check_time(start, wall)?;
                    for i in 0..interiors.len() {
                        rhs[(i, 0)] = 0.;
                    }
                    for &(i, v) in &coupling[j] {
                        rhs[(i, 0)] = v;
                    }
                    apply_factor(&interior_factor, &mut rhs)?;
                    for (i, &ti) in traces[..=j].iter().enumerate() {
                        let correction = coupling[i].iter().fold(Sum::default(), |sum, &(k, v)| {
                            sum.add(Sum::from(v).mul(rhs[(k, 0)]))
                        });
                        let entry = schur.entry((ti, tj)).or_default();
                        *entry = entry.sub(correction);
                    }
                }
            }
            for &dof in &interiors {
                local[dof] = None;
            }
            owners.push(Owner {
                interiors,
                traces,
                coupling,
                factor: interior_factor,
            });
        }

        let trace_entries = schur
            .into_iter()
            .map(|((i, j), value)| Triplet::new(i, j, value.value()))
            .collect::<Vec<_>>();
        let trace_nonzeros = trace_entries.len();
        let trace_base = plan(live, trace_workspace, 1)?;
        let trace_factor = factor(
            trace_dofs.len(),
            &trace_entries,
            trace_base,
            budget,
            start,
            wall,
        )?;
        if let Some(f) = &trace_factor {
            peak = peak.max(f.2);
            live = plan(
                live,
                f.3.checked_sub(trace_base)
                    .ok_or_else(|| error("MEMORY_LIMIT", "invalid shared trace factor plan"))?,
                1,
            )?;
        }
        // Factor construction includes scratch at that stage only. During
        // back-substitution every retained factor coexists with the largest
        // single-factor scratch; owner and trace solves run sequentially.
        let par = super::super::parallel::current();
        let solve_scratch = owners
            .iter()
            .filter_map(|owner| owner.factor.as_ref())
            .chain(trace_factor.as_ref())
            .map(|factor| {
                factor
                    .0
                    .solve_in_place_scratch::<f64>(1, par)
                    .unaligned_bytes_required()
            })
            .max()
            .unwrap_or(0);
        live = plan(live, solve_scratch, 1)?;
        budget.check(live)?;
        Ok((
            peak.max(live),
            Self {
                owners,
                trace_dofs,
                trace_factor,
                trace_nonzeros,
                owned_bytes: live - base,
                max_original_residual: AtomicU64::new(0),
                start,
                wall,
            },
        ))
    }

    pub fn solve(&self, cells: &[Cell], free: &[Option<usize>], rhs: &[f64]) -> Result<Vec<f64>> {
        check_time(self.start, self.wall)?;
        if rhs.len() != free.len() || rhs.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "SHARED_RETURN_SCHUR",
                "finite original conductive RHS required",
            ));
        }
        let mut trace_rhs: Vec<_> = self.trace_dofs.iter().map(|&i| Sum::from(rhs[i])).collect();
        for owner in &self.owners {
            check_time(self.start, self.wall)?;
            if owner.interiors.is_empty() || owner.traces.is_empty() {
                continue;
            }
            let mut q = Mat::from_fn(owner.interiors.len(), 1, |i, _| rhs[owner.interiors[i]]);
            apply_factor(&owner.factor, &mut q)?;
            for (col, &trace) in owner.traces.iter().enumerate() {
                for &(i, v) in &owner.coupling[col] {
                    trace_rhs[trace] = trace_rhs[trace].sub(Sum::from(v).mul(q[(i, 0)]));
                }
            }
        }
        let mut trace = Mat::from_fn(self.trace_dofs.len(), 1, |i, _| trace_rhs[i].value());
        apply_factor(&self.trace_factor, &mut trace)?;
        let mut solution = vec![0.; rhs.len()];
        for (i, &dof) in self.trace_dofs.iter().enumerate() {
            solution[dof] = trace[(i, 0)];
        }
        for owner in &self.owners {
            check_time(self.start, self.wall)?;
            if owner.interiors.is_empty() {
                continue;
            }
            let mut q: Vec<_> = owner.interiors.iter().map(|&i| Sum::from(rhs[i])).collect();
            for (col, &j) in owner.traces.iter().enumerate() {
                for &(i, v) in &owner.coupling[col] {
                    q[i] = q[i].sub(Sum::from(v).mul(trace[(j, 0)]));
                }
            }
            let mut q = Mat::from_fn(q.len(), 1, |i, _| q[i].value());
            apply_factor(&owner.factor, &mut q)?;
            for (i, &dof) in owner.interiors.iter().enumerate() {
                solution[dof] = q[(i, 0)];
            }
        }
        // Audit against the original welded operator, independently of local
        // blocks and the trace Schur. Retained physical coordinates stay zero.
        let applied = divergence(cells, &conductive(cells, &solution), rhs.len());
        let (mut error_max, mut scale) = (0_f64, 0_f64);
        for i in 0..rhs.len() {
            if !solution[i].is_finite() || !applied[i].is_finite() {
                return Err(error(
                    "SHARED_RETURN_SCHUR",
                    "nonfinite original conductive solution",
                ));
            }
            if free[i].is_some() {
                error_max = error_max.max((applied[i] - rhs[i]).abs());
                scale = scale.max(applied[i].abs().max(rhs[i].abs()));
            }
        }
        let relative = error_max / scale.max(f64::MIN_POSITIVE);
        if !relative.is_finite() || relative > RESIDUAL_LIMIT {
            return Err(error(
                "SHARED_RETURN_SCHUR",
                &format!("original free conductive residual {relative:e}"),
            ));
        }
        self.max_original_residual
            .fetch_max(relative.to_bits(), Ordering::Relaxed);
        check_time(self.start, self.wall)?;
        Ok(solution)
    }

    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "method":"sparse owner interiors and shared conductive trace Schur",
            "ownerInteriorDofs":self.owners.iter().map(|o|o.interiors.len()).collect::<Vec<_>>(),
            "freeSharedTraceDofs":self.trace_dofs.len(),
            "traceUpperEntries":self.trace_nonzeros,
            "ownedPlannedBytes":self.owned_bytes,
            "maxOriginalFreeResidualRelative":f64::from_bits(self.max_original_residual.load(Ordering::Relaxed)),
            "scope":"real conductive correction; electric and magnetic operators remain common",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conductive_owner_interiors_and_shared_trace_recover_original_free_solution() {
        let cells = [
            (0, 1, 2.),
            (1, 2, 3.),
            (1, 3, 4.),
            (2, 4, 5.),
            (3, 4, 6.),
            (4, 5, 7.),
        ]
        .map(|(a, b, r)| Cell::new(vec![(a, 1.), (b, -1.)], r));
        let retained = BTreeSet::from([0, 5]);
        let free = [None, Some(0), Some(1), Some(2), Some(3), None];
        let budget = AllocationBudget {
            max_bytes: 4 * 1024 * 1024,
            max_nodes: 100,
            max_tetrahedra: 100,
        };
        let start = Instant::now();
        let wall = Duration::from_secs(5);
        let (_, original) =
            super::super::correction(&cells, 6, &retained, 0, budget, start, wall, None).unwrap();
        let (_, split) = Backend::factor(
            &cells,
            &free,
            2,
            |i| Some(i / 3),
            &[2, 3],
            0,
            budget,
            start,
            wall,
        )
        .unwrap();
        let rhs = [1., 0.7, -0.4, 0.3, -0.8, 2.];
        let a = original.solve(&cells, &rhs).unwrap();
        let b = split.solve(&cells, &free, &rhs).unwrap();
        assert_eq!([b[0], b[5]], [0., 0.]);
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-12));
        let evidence = split.evidence();
        assert_eq!(evidence["ownerInteriorDofs"], serde_json::json!([1, 1]));
        assert_eq!(evidence["freeSharedTraceDofs"], 2);
        assert_eq!(evidence["traceUpperEntries"], 3);
        assert!(
            evidence["maxOriginalFreeResidualRelative"]
                .as_f64()
                .unwrap()
                < 1e-12
        );
        assert!(Backend::factor(
            &cells,
            &free,
            2,
            |i| Some(i / 3),
            &[2],
            0,
            budget,
            start,
            wall
        )
        .is_err());
    }
}
