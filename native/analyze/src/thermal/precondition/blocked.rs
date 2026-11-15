//! Test-only pinned-faer prototype. Never selectable from production or DSL.
use super::*;
use faer::sparse::linalg::cholesky::supernodal;
use std::cell::{Cell, RefCell};
thread_local! {static SELECTED:Cell<bool>=const{Cell::new(false)};}
thread_local! {static METRICS:RefCell<Option<serde_json::Map<String,serde_json::Value>>>=const{RefCell::new(None)};}
pub(crate) fn metrics_start() {
    METRICS.with(|m| *m.borrow_mut() = Some(Default::default()));
}
pub(crate) fn metrics_finish() -> serde_json::Value {
    METRICS.with(|m| serde_json::Value::Object(m.borrow_mut().take().unwrap()))
}
pub(crate) fn record(name: &str, start: Instant) {
    METRICS.with(|m| {
        if let Some(m) = m.borrow_mut().as_mut() {
            let old = m.get(name).and_then(|v| v.as_f64()).unwrap_or(0.);
            m.insert(
                name.into(),
                serde_json::json!(old + start.elapsed().as_secs_f64() * 1000.),
            );
        }
    });
}
pub(crate) fn selected() -> bool {
    SELECTED.get()
}
pub(crate) fn with_variant<T>(blocked: bool, f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SELECTED.set(self.0);
        }
    }
    let restore = Restore(SELECTED.replace(blocked));
    let result = f();
    drop(restore);
    result
}
pub(crate) struct SuperPlan {
    symbolic: supernodal::SymbolicSupernodalCholesky<usize>,
    numeric_scratch: StackReq,
}
impl SuperPlan {
    pub(super) fn prepare(
        matrix: faer::sparse::SymbolicSparseColMatRef<'_, usize>,
        etree: simplicial::EliminationTreeRef<'_, usize>,
        counts: &[usize],
        base: usize,
        limit: usize,
        slots: usize,
        planning: &mut Planning,
    ) -> Result<Option<Self>> {
        let n = matrix.nrows();
        let fill = counts.iter().sum::<usize>();
        let req = supernodal::factorize_supernodal_symbolic_cholesky_scratch::<usize>(n);
        // Before allocation: every structural supernode row is covered by scalar
        // fill; row-major adjacency and temporary compressed construction each
        // have at most the same count. Reserve sixteen per-node arrays, including
        // temporary tree/postorder/count arrays (base already covers
        // ordering, original/mapped matrix contributions and solve buffers).
        let index_bound = (fill * 3 + n * 16 + 8) * 8;
        planning.bytes = base
            .saturating_add(index_bound)
            .saturating_add(planning.workspace_bytes)
            .saturating_add(req.unaligned_bytes_required());
        let symbolic_peak = planning.bytes;
        if planning.bytes > limit {
            return Ok(None);
        }
        let mut scratch = MemBuffer::try_new(req).map_err(memory)?;
        let symbolic = supernodal::factorize_supernodal_symbolic_cholesky(
            matrix,
            etree,
            counts,
            MemStack::new(&mut scratch),
            faer::sparse::linalg::SymbolicSupernodalParams { relax: None },
        )
        .map_err(memory)?;
        let numeric_scratch = supernodal::factorize_supernodal_numeric_llt_scratch::<usize, f64>(
            &symbolic,
            Par::Seq,
            Default::default(),
        );
        let solve = symbolic.solve_in_place_scratch::<f64>(1, Par::Seq);
        let numeric = symbolic.len_val() * 8;
        // Numeric factorization takes LOWER CSC; original mapped upper CSC and
        // its lower transpose coexist. Include both structure/value transport.
        let transpose =
            (matrix.col_ptr().len() + matrix.row_idx().len()) * 8 + matrix.row_idx().len() * 8;
        // The symbolic construction peak has NO resident numeric factors yet.
        // Do not add its transient compressed rows to every resident slot.
        // Public structure lengths cover all factor arrays; hidden row-major
        // adjacency has at most row_idx.len entries and <=ns+1 row pointers.
        let resident_indices = (symbolic.row_idx().len() * 2 + symbolic.n_supernodes() * 7 + 8) * 8;
        let workspace = (numeric_scratch.unaligned_bytes_required() + transpose)
            .max(solve.unaligned_bytes_required());
        let estimate = |s: usize| {
            symbolic_peak.max(
                base.saturating_add(resident_indices)
                    .saturating_add(workspace)
                    .saturating_add(numeric.saturating_mul(s)),
            )
        };
        planning.fill = symbolic.len_val();
        planning.symbolic_index_bytes = resident_indices;
        planning.bytes = estimate(1);
        let Some(slots) = (1..=slots).rev().find(|s| estimate(*s) <= limit) else {
            return Ok(None);
        };
        planning.cache_slots = slots;
        planning.numeric_factor_bytes = numeric * slots;
        planning.bytes = estimate(slots);
        planning.workspace_bytes =
            planning.bytes - base - resident_indices - planning.numeric_factor_bytes;
        Ok(Some(Self {
            symbolic,
            numeric_scratch,
        }))
    }
    pub(super) fn factor(
        &self,
        matrix: SparseColMatRef<'_, usize, f64>,
        plan: Arc<Plan>,
    ) -> Result<Option<Factor>> {
        let start = Instant::now();
        let lower = matrix.transpose().to_col_major().map_err(memory)?;
        record("packingMs", start);
        let start = Instant::now();
        let mut values = vec![0.; self.symbolic.len_val()];
        let mut scratch = MemBuffer::try_new(self.numeric_scratch).map_err(memory)?;
        match supernodal::factorize_supernodal_numeric_llt(
            &mut values,
            lower.as_ref(),
            Default::default(),
            &self.symbolic,
            Par::Seq,
            MemStack::new(&mut scratch),
            Default::default(),
        ) {
            Ok(info)
                if info.dynamic_regularization_count == 0
                    && values.iter().all(|v| v.is_finite()) => {}
            _ => return Ok(None),
        }
        record("numericMs", start);
        Ok(Some(Factor { plan, values }))
    }
    pub(super) fn solve(&self, values: &[f64], rhs: &mut [f64], cols: usize) -> Result<()> {
        let setup = Instant::now();
        let mut scratch =
            MemBuffer::try_new(self.symbolic.solve_in_place_scratch::<f64>(cols, Par::Seq))
                .map_err(memory)?;
        record("solveScratchMs", setup);
        let start = Instant::now();
        let _timer = super::super::profile::timer(super::super::profile::Phase::TriangularSolve);
        supernodal::SupernodalLltRef::new(&self.symbolic, values).solve_in_place_with_conj(
            Conj::No,
            MatMut::from_column_major_slice_mut(rhs, self.symbolic.nrows(), cols),
            Par::Seq,
            MemStack::new(&mut scratch),
        );
        record("triangularMs", start);
        Ok(())
    }
}
fn multiple(f: &Factor, rhs: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
    let n = f.plan.permutation.len();
    let cols = rhs.len();
    let mut z = Vec::with_capacity(n * cols);
    for b in rhs {
        z.extend(f.plan.permutation.iter().map(|i| b[*i]));
    }
    match &f.plan.symbolic {
        Symbolic::Supernodal(s) => s.solve(&f.values, &mut z, cols)?,
        Symbolic::Simplicial(s) => {
            let mut scratch =
                MemBuffer::try_new(s.solve_in_place_scratch::<f64>(cols)).map_err(memory)?;
            simplicial::SimplicialLltRef::new(s, &f.values).solve_in_place_with_conj(
                Conj::No,
                MatMut::from_column_major_slice_mut(&mut z, n, cols),
                Par::Seq,
                MemStack::new(&mut scratch),
            );
        }
    }
    let mut out = vec![vec![0.; n]; cols];
    for c in 0..cols {
        for (i, p) in f.plan.permutation.iter().enumerate() {
            out[c][*p] = z[c * n + i];
        }
    }
    Ok(out)
}
fn verify_both_triangles(f: &Factor, rhs: &[Vec<f64>], solved: &[Vec<f64>]) -> f64 {
    let n = f.plan.permutation.len();
    let mut l = vec![vec![0.; n]; n];
    match &f.plan.symbolic {
        Symbolic::Simplicial(s) => {
            for c in 0..n {
                for p in s.col_ptr()[c]..s.col_ptr()[c + 1] {
                    l[s.row_idx()[p]][c] = f.values[p];
                }
            }
        }
        Symbolic::Supernodal(s) => {
            let factor = supernodal::SupernodalLltRef::new(&s.symbolic, &f.values);
            for k in 0..s.symbolic.n_supernodes() {
                let node = factor.supernode(k);
                let a = node.val();
                let width = a.ncols();
                for c in 0..width {
                    for r in c..width {
                        l[node.start() + r][node.start() + c] = a[(r, c)];
                    }
                    for (r, i) in node.pattern().iter().enumerate() {
                        l[*i][node.start() + c] = a[(width + r, c)];
                    }
                }
            }
        }
    }
    let mut error = 0f64;
    for (b, solution) in rhs.iter().zip(solved) {
        let b: Vec<_> = f.plan.permutation.iter().map(|i| b[*i]).collect();
        let mut y = vec![0.; n];
        let mut x = vec![0.; n];
        for i in 0..n {
            y[i] = (b[i] - (0..i).map(|j| l[i][j] * y[j]).sum::<f64>()) / l[i][i];
        }
        for i in (0..n).rev() {
            x[i] = (y[i] - (i + 1..n).map(|j| l[j][i] * x[j]).sum::<f64>()) / l[i][i];
        }
        for i in 0..n {
            let forward = (0..=i).map(|j| l[i][j] * y[j]).sum::<f64>();
            let backward = (i..n).map(|j| l[j][i] * x[j]).sum::<f64>();
            assert!((forward - b[i]).abs() <= 1e-11 * (1. + b[i].abs()));
            assert!((backward - y[i]).abs() <= 1e-11 * (1. + y[i].abs()));
            error = error.max((x[i] - solution[f.plan.permutation[i]]).abs() / x[i].abs().max(1.));
        }
    }
    assert!(error < 1e-10);
    error
}
pub(crate) fn replay(
    pattern: Arc<Pattern>,
    cases: &[serde_json::Value],
    start: Instant,
) -> serde_json::Value {
    use serde_json::json;
    let mut results = vec![];
    let mut reference: Vec<Vec<Vec<f64>>> = vec![];
    let mut max_difference = 0f64;
    for repetition in 0..3 {
        for candidate in [false, true] {
            let begin = Instant::now();
            metrics_start();
            let mut planning = Planning::default();
            let symbolic_start = Instant::now();
            let plan = Arc::new(
                with_variant(candidate, || {
                    Plan::prepare(
                        pattern.clone(),
                        128 * 1024 * 1024,
                        3,
                        &mut planning,
                        start,
                        120.,
                    )
                })
                .unwrap()
                .unwrap(),
            );
            let symbolic_ms = symbolic_start.elapsed().as_secs_f64() * 1000.;
            let mut max_backward = 0f64;
            let mut captures = vec![];
            for (k, c) in cases.iter().enumerate() {
                let d: Vec<f64> = serde_json::from_value(c["diag"].clone()).unwrap();
                let g: Vec<f64> = serde_json::from_value(c["g"].clone()).unwrap();
                let rhs: Vec<Vec<f64>> = serde_json::from_value(c["rhs"].clone()).unwrap();
                let f = plan.factor(&d, &g, start, 120.).unwrap().unwrap();
                let solved = multiple(&f, &rhs).unwrap();
                for (b, x) in rhs.iter().zip(&solved) {
                    let (y, m) = pattern.apply_assembled(&d, &g, x);
                    for ((a, b), m) in y.iter().zip(b).zip(m) {
                        max_backward = max_backward.max((a - b).abs() / (m + b.abs()).max(1e-300));
                    }
                }
                if repetition == 0 && !candidate {
                    reference.push(solved.clone());
                }
                for (a, b) in solved.iter().flatten().zip(reference[k].iter().flatten()) {
                    max_difference = max_difference.max((a - b).abs());
                }
                // Fixed same factor/RHS sequence. Each RHS is exactly copied on each
                // application. A bounded repeated apply exercises both triangular
                // operations; construction/packing are included in total replay wall.
                for _ in 0..64 {
                    for b in &rhs {
                        std::hint::black_box(f.apply(b).unwrap());
                    }
                }
                if repetition == 0 {
                    captures.push(json!({"case":k,"values":f.values,"permutation":plan.permutation,"mappedColumns":plan.matrix.col_ptr(),"mappedRows":plan.matrix.row_idx(),"solution":solved}));
                }
            }
            let details = metrics_finish();
            assert!(
                max_backward < 1e-10,
                "Original matrix backward residual {max_backward}"
            );
            results.push(json!({"variant":if candidate{"supernodal"}else{"baseline"},"repetition":repetition,"wallMs":begin.elapsed().as_secs_f64()*1000.,"symbolicMs":symbolic_ms,"details":details,"planning":planning,"maxOriginalMatrixBackwardResidual":max_backward,"captures":captures}));
        }
    }
    json!({"scope":"Three bounded repeats per variant; identical four matrices/two RHS,64 replays per RHS; packing/construction/permutation included in wall. No cache slots added.","maxAbsoluteSolutionDifference":max_difference,"results":results})
}
#[test]
fn standalone_extremes_multiple_rhs_residual_ownership_and_invalidation() {
    let mut records = vec![];
    for n in [0, 1, 2, 63, 64, 65, 129] {
        for exponent in [0., 6., 10.] {
            let edges: Vec<_> = (1..n).map(|i| (i - 1, i)).collect();
            let scaling: Vec<_> = (0..n)
                .map(|i| 10f64.powf(-exponent * i as f64 / (n.max(2) - 1) as f64))
                .collect();
            let g: Vec<_> = edges
                .iter()
                .map(|(i, j)| 0.01 * scaling[*i].min(scaling[*j]))
                .collect();
            let mut d = scaling.clone();
            for ((i, j), v) in edges.iter().zip(&g) {
                d[*i] += v;
                d[*j] += v;
            }
            let pattern = Arc::new(Pattern::new(n, &edges));
            let rhs: Vec<Vec<f64>> = (0..3)
                .map(|k| (0..n).map(|i| ((i + k + 1) as f64 * 0.3).sin()).collect())
                .collect();
            let mut solutions = vec![];
            for candidate in [false, true] {
                let mut planning = Planning::default();
                let plan = Arc::new(
                    with_variant(candidate, || {
                        Plan::prepare(
                            pattern.clone(),
                            128 * 1024 * 1024,
                            3,
                            &mut planning,
                            Instant::now(),
                            10.,
                        )
                    })
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
                let factor = plan.factor(&d, &g, Instant::now(), 10.).unwrap().unwrap();
                let solved = multiple(&factor, &rhs).unwrap();
                let triangular_reference = verify_both_triangles(&factor, &rhs, &solved);
                let mut backward = 0f64;
                for (x, b) in solved.iter().zip(&rhs) {
                    let (y, m) = pattern.apply_assembled(&d, &g, x);
                    for ((a, b), m) in y.iter().zip(b).zip(m) {
                        backward = backward.max((a - b).abs() / (m + b.abs()).max(1e-300));
                    }
                }
                assert!(backward < 1e-11);
                // Reentrant apply creates private scratch, immutable shared factor.
                let f = Arc::new(factor);
                let expected = f.apply(&rhs[0]).unwrap();
                std::thread::scope(|scope| {
                    let f = f.clone();
                    let b = &rhs[0];
                    let t = scope.spawn(move || f.apply(b).unwrap());
                    assert_eq!(expected, t.join().unwrap());
                });
                if n > 0 {
                    // dt/capacity/boundary changes modify the original diagonal;
                    // tangent/barrel changes modify exact off-diagonal coefficients.
                    for change in 0..4 {
                        let mut dd = d.clone();
                        let mut gg = g.clone();
                        if change < 3 || gg.is_empty() {
                            dd[0] += 0.125 * (change + 1) as f64;
                        } else {
                            gg[0] *= 0.5;
                        }
                        let fresh = plan.factor(&dd, &gg, Instant::now(), 10.).unwrap().unwrap();
                        assert_ne!(fresh.values, f.values);
                        let x = fresh.apply(&rhs[0]).unwrap();
                        let (y, m) = pattern.apply_assembled(&dd, &gg, &x);
                        assert!(y
                            .iter()
                            .zip(&rhs[0])
                            .zip(m)
                            .all(|((a, b), m)| (a - b).abs() / (m + b.abs()).max(1e-300) < 1e-11));
                    }
                }
                // A changed free set/order/sparsity ALWAYS gets a distinct immutable
                // pattern/plan, even for the same board identity; no new reuse key.
                let changed = Arc::new(Pattern::new(
                    n + 1,
                    &(1..=n).map(|i| (i - 1, i)).collect::<Vec<_>>(),
                ));
                let next = with_variant(candidate, || {
                    Plan::prepare(
                        changed,
                        128 * 1024 * 1024,
                        3,
                        &mut Planning::default(),
                        Instant::now(),
                        10.,
                    )
                })
                .unwrap()
                .unwrap();
                assert!(!Arc::ptr_eq(&plan.pattern, &next.pattern));
                solutions.push(solved);
                records.push(serde_json::json!({"n":n,"diagonalScaleExponent":exponent,"variant":candidate,"backwardResidual":backward,"denseForwardBackwardRelativeDifference":triangular_reference,"planning":planning}));
            }
            for (a, b) in solutions[0]
                .iter()
                .flatten()
                .zip(solutions[1].iter().flatten())
            {
                assert!((a - b).abs() <= 1e-11 * a.abs().max(1.));
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../debugging/thermal-verification/blocked-solve");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("standalone.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"status":"pass","records":records})).unwrap(),
    )
    .unwrap();
}
#[test]
fn dependency_changes_do_not_reuse_factor_or_cross_mesh_plan() {
    // There is deliberately NO added numeric cache. Immutable structural plans
    // belong to one System; every invocation factors the supplied full matrix.
    // Exercise the seven physical dependencies separately, with one source board
    // identity deliberately held fixed outside the Plan (Plan takes no board ID).
    let n = 7;
    let original_edges: Vec<_> = (1..n).map(|i| (i - 1, i)).collect();
    let original_g = vec![0.2; n - 1];
    let rhs: Vec<_> = (0..n).map(|i| (i as f64 * 0.7).cos()).collect();
    let mut records = vec![];
    for candidate in [false, true] {
        let mut original: Option<Arc<Plan>> = None;
        for name in [
            "baseline",
            "dt",
            "capacity",
            "tangent",
            "free-order",
            "sparsity",
            "boundary",
            "barrel",
        ] {
            let mut edges = original_edges.clone();
            let mut g = original_g.clone();
            let mut capacity: Vec<_> = (0..n).map(|i| 1. + 0.1 * i as f64).collect();
            let mut dt = 0.1;
            let mut shunt: Vec<_> = (0..n).map(|i| 0.01 * (i + 1) as f64).collect();
            let mut b = rhs.clone();
            let mut size = n;
            match name {
                "dt" => dt = 0.2,
                "capacity" => capacity[0] = 2.,
                "tangent" => g[0] = 0.3,
                "free-order" => {
                    edges = edges
                        .into_iter()
                        .map(|(a, b)| (n - 1 - a, n - 1 - b))
                        .collect();
                    capacity.reverse();
                    shunt.reverse();
                    b.reverse();
                }
                "sparsity" => {
                    edges.push((0, 2));
                    g.push(0.125);
                }
                "boundary" => {
                    size = n - 1;
                    edges.pop();
                    g.pop();
                    capacity.pop();
                    shunt.pop();
                    b.pop();
                    shunt[size - 1] += 0.2;
                }
                "barrel" => {
                    edges.push((0, n - 1));
                    g.push(0.037);
                }
                _ => {}
            }
            let mut d: Vec<_> = capacity
                .iter()
                .zip(shunt)
                .map(|(c, s)| c / dt + s)
                .collect();
            for ((a, b), v) in edges.iter().zip(&g) {
                d[*a] += v;
                d[*b] += v;
            }
            let pattern = Arc::new(Pattern::new(size, &edges));
            let mut planning = Planning::default();
            let plan = Arc::new(
                with_variant(candidate, || {
                    Plan::prepare(
                        pattern.clone(),
                        128 * 1024 * 1024,
                        3,
                        &mut planning,
                        Instant::now(),
                        10.,
                    )
                })
                .unwrap()
                .unwrap(),
            );
            if let Some(old) = &original {
                assert!(!Arc::ptr_eq(old, &plan));
                assert!(!Arc::ptr_eq(&old.pattern, &plan.pattern));
            } else {
                original = Some(plan.clone());
            }
            // Same admitted three resident slots: release oldest BEFORE
            // replacement factor construction, as the unchanged System does.
            let mut factors = std::collections::VecDeque::new();
            for k in 0..4 {
                if factors.len() == 3 {
                    factors.pop_front();
                }
                let mut dd = d.clone();
                dd[0] += k as f64 * 0.01;
                let f = plan.factor(&dd, &g, Instant::now(), 10.).unwrap().unwrap();
                let x = f.apply(&b).unwrap();
                let (y, m) = pattern.apply_assembled(&dd, &g, &x);
                assert!(y
                    .iter()
                    .zip(&b)
                    .zip(m)
                    .all(|((a, b), m)| (a - b).abs() / (m + b.abs()).max(1e-300) < 1e-12));
                factors.push_back(f);
                assert!(factors.len() <= planning.cache_slots);
            }
            records.push(serde_json::json!({"boardIdentity":"same-source-board","variant":candidate,"dependency":name,"freeNodes":size,"edges":edges,"capacity":capacity,"dt":dt,"g":g,"diagonal":d,"freeOrder":if name=="free-order"{(0..size).rev().collect::<Vec<_>>()}else{(0..size).collect::<Vec<_>>()},"planning":planning,"maxResidentFactors":3,"crossPlanReuse":false}));
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../debugging/thermal-verification/blocked-solve");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("invalidation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"status":"pass","records":records})).unwrap(),
    )
    .unwrap();
}
#[test]
fn insufficient_admission_and_nonpositive_pivot_reject_without_regularization() {
    for candidate in [false, true] {
        let pattern = Arc::new(Pattern::new(2, &[(0, 1)]));
        assert!(with_variant(candidate, || Plan::prepare(
            pattern.clone(),
            1,
            3,
            &mut Planning::default(),
            Instant::now(),
            10.
        ))
        .unwrap()
        .is_none());
        let plan = Arc::new(
            with_variant(candidate, || {
                Plan::prepare(
                    pattern,
                    128 * 1024 * 1024,
                    3,
                    &mut Planning::default(),
                    Instant::now(),
                    10.,
                )
            })
            .unwrap()
            .unwrap(),
        );
        assert!(plan
            .factor(&[1., 1.], &[2.], Instant::now(), 10.)
            .unwrap()
            .is_none());
    }
}
