//! Direct constant-vector finite-volume magnetostatic action on the unchanged
//! copper cells. Bounded local blocks may use an explicitly budgeted dense
//! kernel; the mixed operator streams the same pair quadrature without retaining
//! quadratic kernel storage.
use super::{
    accurate::Sum,
    error, magnetic_kernel_cache,
    mesh::LayeredMesh,
    polyhedron::{
        separated_order, tetra_integral_adaptive, tetra_self_integral, PointPotential,
        TetraCubature,
    },
    volume_current::Space,
    volume_magnetic::MagneticApply,
    AllocationBudget, Result,
};

use std::time::{Duration, Instant};

#[path = "hierarchical_magnetic.rs"]
pub(super) mod hierarchical_magnetic;

const CUBATURE_ORDERS: [usize; 5] = [4, 6, 8, 12, 16];

struct PairQuadrature {
    tetra: Vec<[[f64; 3]; 4]>,
    sources: Vec<PointPotential>,
    cubatures: Vec<Vec<TetraCubature>>,
    order: usize,
}

impl PairQuadrature {
    fn extra_bytes(n: usize, order: usize) -> Result<usize> {
        if !(8..=16).contains(&order) {
            return Err(error(
                "MAGNETIC_QUADRATURE",
                "direct volume integral orders8..16 required",
            ));
        }
        let per_cell = CUBATURE_ORDERS
            .into_iter()
            .filter(|&o| o <= order)
            .map(|o| o.pow(3) * 32)
            .sum::<usize>();
        n.checked_mul(per_cell + 16384)
            .ok_or_else(|| error("MEMORY_LIMIT", "direct magnetic cubature overflow"))
    }

    fn new(
        tetra: &[[[f64; 3]; 4]],
        order: usize,
        deadline: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        let mut sources = Vec::with_capacity(tetra.len());
        let mut cubatures = Vec::with_capacity(tetra.len());
        for &cell in tetra {
            deadline()?;
            sources.push(PointPotential::tetrahedron(cell)?);
            cubatures.push(
                CUBATURE_ORDERS
                    .into_iter()
                    .filter(|&o| o <= order)
                    .map(|o| TetraCubature::new(cell, o))
                    .collect::<Result<Vec<_>>>()?,
            );
        }
        Ok(Self {
            tetra: tetra.to_vec(),
            sources,
            cubatures,
            order,
        })
    }

    fn pair(&self, i: usize, j: usize) -> Result<f64> {
        if i == j {
            return tetra_self_integral(self.tetra[i], self.order);
        }
        let q = self.cubatures[i][0].separation_q(&self.cubatures[j][0]);
        if let Some(k) = separated_order(q, self.order)
            .and_then(|o| CUBATURE_ORDERS.iter().position(|&candidate| candidate == o))
        {
            return self.cubatures[i][k].direct_integral(&self.cubatures[j][k]);
        }
        tetra_integral_adaptive(self.tetra[i], &self.sources[j], self.order, 1e-10)
    }
}

// Fixed exact working set, not a spatial approximation or retained all-pairs
// kernel. One slot includes 25,344 B separated cubature at orders4/6/8,
// 32,000 B lazy far-source samples, tetra faces/edges/headers, and allocator
// capacity padding. Two additional slots cover construction and near/self
// graded integration scratch; no cache-owned reference escapes pair().
const STREAMED_PAIR_SLOT_BYTES: usize = 128 * 1024;
const STREAMED_PAIR_MAX_SLOTS: usize = 64;

struct PairCell {
    index: usize,
    touched: u64,
    source: PointPotential,
    cubatures: Vec<TetraCubature>,
}
// Diagnostic counters exist only when explicitly requested.
struct PairTrace {
    counts: [u64; 3],
    seconds: [f64; 3],
    last: Instant,
}
impl PairTrace {
    fn emit(&self, final_record: bool) {
        use std::io::Write;
        let record = serde_json::json!({"phase":"near-kernels","final":final_record,"countsIncludeFailed":true,
            "selfCalls":self.counts[0],"separatedCalls":self.counts[1],"adaptiveCalls":self.counts[2],
            "selfSeconds":self.seconds[0],"separatedSeconds":self.seconds[1],"adaptiveSeconds":self.seconds[2]});
        let _ = writeln!(std::io::stderr().lock(), "P7_MAGNETIC_TRACE {record}");
    }
}
impl Drop for PairTrace {
    fn drop(&mut self) { self.emit(true); }
}
struct BoundedPairs {
    trace: Option<std::cell::RefCell<PairTrace>>,
    tetra: Vec<[[f64; 3]; 4]>,
    cells: std::cell::RefCell<Vec<PairCell>>,
    slots: usize,
    tick: std::cell::Cell<u64>,
    order: usize,
    start: Instant,
    wall: Duration,
}
impl BoundedPairs {
    fn plan(
        n: usize,
        order: usize,
        base: usize,
        budget: AllocationBudget,
    ) -> Result<(usize, usize)> {
        PairQuadrature::extra_bytes(0, order)?; // unchanged supported order contract
        let fixed = n
            .checked_mul(std::mem::size_of::<[[f64; 3]; 4]>())
            .and_then(|v| v.checked_add(2 * STREAMED_PAIR_SLOT_BYTES))
            .ok_or_else(|| error("MEMORY_LIMIT", "bounded magnetic geometry plan overflow"))?;
        let available = budget
            .max_bytes
            .checked_sub(base)
            .and_then(|v| v.checked_sub(fixed))
            .ok_or_else(|| {
                error(
                    "MEMORY_LIMIT",
                    "bounded exact magnetic working set exceeds budget",
                )
            })?;
        let slots = (available / STREAMED_PAIR_SLOT_BYTES).min(STREAMED_PAIR_MAX_SLOTS);
        if slots < 2 {
            return Err(error(
                "MEMORY_LIMIT",
                "two exact magnetic cache slots and scratch required",
            ));
        }
        let owned = fixed
            .checked_add(slots * STREAMED_PAIR_SLOT_BYTES)
            .ok_or_else(|| error("MEMORY_LIMIT", "bounded magnetic cache overflow"))?;
        budget.check(
            base.checked_add(owned)
                .ok_or_else(|| error("MEMORY_LIMIT", "bounded magnetic total overflow"))?,
        )?;
        Ok((owned, slots))
    }
    fn new(
        tetra: Vec<[[f64; 3]; 4]>,
        order: usize,
        slots: usize,
        start: Instant,
        wall: Duration,
    ) -> Result<Self> {
        check_deadline(start, wall)?;
        Ok(Self {
            trace: (std::env::var_os("P7_MAGNETIC_TRACE").as_deref() == Some(std::ffi::OsStr::new("1")))
                .then(|| std::cell::RefCell::new(PairTrace { counts: [0; 3], seconds: [0.; 3], last: Instant::now() })),
            tetra,
            cells: std::cell::RefCell::new(Vec::with_capacity(slots)),
            slots,
            tick: std::cell::Cell::new(0),
            order,
            start,
            wall,
        })
    }
    fn ensure(&self, cells: &mut Vec<PairCell>, index: usize, pin: Option<usize>) -> Result<()> {
        check_deadline(self.start, self.wall)?;
        let tick = self.tick.get().saturating_add(1);
        self.tick.set(tick);
        if let Some(cell) = cells.iter_mut().find(|cell| cell.index == index) {
            cell.touched = tick;
            return Ok(());
        }
        // Evict before constructing the replacement. The current target stays
        // pinned for every source of its original upper-triangle row.
        if cells.len() == self.slots {
            let remove = cells
                .iter()
                .enumerate()
                .filter(|(_, cell)| Some(cell.index) != pin)
                .min_by_key(|(_, cell)| cell.touched)
                .map(|(i, _)| i)
                .ok_or_else(|| {
                    error(
                        "MEMORY_LIMIT",
                        "bounded magnetic cache has no evictable slot",
                    )
                })?;
            cells.remove(remove);
        }
        let tetra = self.tetra[index];
        let source = PointPotential::tetrahedron(tetra)?;
        let mut cubatures = Vec::with_capacity(3);
        // separated_order selects only4,6,8; near/self keep maximum=self.order.
        for order in [4, 6, 8] {
            check_deadline(self.start, self.wall)?;
            if order <= self.order {
                cubatures.push(TetraCubature::new(tetra, order)?);
            }
        }
        cells.push(PairCell {
            index,
            touched: tick,
            source,
            cubatures,
        });
        Ok(())
    }
    fn pair(&self, i: usize, j: usize) -> Result<f64> {
        let started = self.trace.as_ref().map(|_| Instant::now());
        let mut kind = if i == j { 0 } else { 2 };
        let result = self.pair_inner(i, j, &mut kind);
        if let (Some(trace), Some(started)) = (&self.trace, started) {
            let mut trace = trace.borrow_mut();
            trace.counts[kind] += 1;
            trace.seconds[kind] += started.elapsed().as_secs_f64();
            if trace.counts.iter().sum::<u64>() % 64 == 0 && trace.last.elapsed() >= Duration::from_secs(1) {
                trace.emit(false);
                trace.last = Instant::now();
            }
        }
        result
    }
    fn pair_inner(&self, i: usize, j: usize, kind: &mut usize) -> Result<f64> {
        check_deadline(self.start, self.wall)?;
        if i == j {
            return tetra_self_integral(self.tetra[i], self.order);
        }
        *kind = 2;
        let mut cells = self.cells.borrow_mut();
        self.ensure(&mut cells, i, None)?;
        self.ensure(&mut cells, j, Some(i))?;
        let target = cells.iter().find(|cell| cell.index == i).unwrap();
        let source = cells.iter().find(|cell| cell.index == j).unwrap();
        let q = target.cubatures[0].separation_q(&source.cubatures[0]);
        if let Some(k) = separated_order(q, self.order)
            .and_then(|o| [4, 6, 8].iter().position(|&candidate| candidate == o))
        {
            *kind = 1;
            return target.cubatures[k].direct_integral(&source.cubatures[k]);
        }
        tetra_integral_adaptive(self.tetra[i], &source.source, self.order, 1e-10)
    }
}

fn geometry(mesh: &LayeredMesh) -> Vec<[[f64; 3]; 4]> {
    mesh.copper_tetrahedra
        .iter()
        .map(|cell| cell.nodes.map(|i| mesh.nodes_m[i]))
        .collect()
}

/// Applies the unchanged direct tetrahedral Newton operator to bounded batches.
/// Pair quadrature is reused across columns; no all-pairs array is retained.
pub(super) struct Streamed<'a> {
    space: &'a Space,
    pair: BoundedPairs,
    hierarchical: Option<hierarchical_magnetic::Operator>,
    pub owned_bytes: usize,
    start: Instant,
    wall: Duration,
}

impl<'a> Streamed<'a> {
    pub fn new(
        mesh: &LayeredMesh,
        space: &'a Space,
        order: usize,
        base_bytes: usize,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<Self> {
        let n = mesh.copper_tetrahedra.len();
        if n == 0
            || n != space.cells.len()
            || space
                .cells
                .iter()
                .enumerate()
                .any(|(i, cell)| cell.mesh_cell != i)
        {
            return Err(error(
                "MAGNETIC_MESH",
                "unchanged native copper cell order required",
            ));
        }
        let (mut owned_bytes, slots) = BoundedPairs::plan(n, order, base_bytes, budget)?;
        eprintln!("P7_STREAMED_MEMORY_PLAN tetrahedra={n} geometryBytes={} exactCacheSlots={slots} slotBytes={STREAMED_PAIR_SLOT_BYTES} constructionScratchSlots=2 ownedBytes={owned_bytes} baseBytes={base_bytes} budgetBytes={} operator=unchanged-direct-pairs complexity=quadratic", n*std::mem::size_of::<[[f64;3];4]>(),budget.max_bytes);
        // Move the original geometry into the cache; do not duplicate its Vec.
        let pair = BoundedPairs::new(geometry(mesh), order, slots, start, wall)?;
        let hierarchical = {
            let tree = hierarchical_magnetic::Operator::new(
                &pair.tetra,
                12,
                1,
                0.2,
                base_bytes
                    .checked_add(owned_bytes)
                    .ok_or_else(|| error("MEMORY_LIMIT", "magnetic hierarchy base overflow"))?,
                budget,
                start,
                wall,
            ).map_err(|failure| format!("{failure}; stage=adaptive-magnetic-hierarchy-admission originalTetrahedra={n} callerBaseBytes={base_bytes} exactCacheBytes={owned_bytes} availableBudgetBytes={}", budget.max_bytes))?;
            owned_bytes = owned_bytes
                .checked_add(tree.planned_bytes)
                .ok_or_else(|| error("MEMORY_LIMIT", "magnetic hierarchy plan overflow"))?;
            Some(tree)
        };
        Ok(Self {
            space,
            pair,
            hierarchical,
            owned_bytes,
            start,
            wall,
        })
    }

    pub fn evidence(&self) -> serde_json::Value {
        self.hierarchical.as_ref().map_or_else(||serde_json::json!({"policy":"exact-streamed-v1",
            "method":"unchanged direct tetrahedral pairs","scope":"existing near/far quadrature, mesh and physical coverage remain unverified"}),|tree|tree.evidence())
    }
    pub fn resistive_goals(
        &self,
        columns: &[Vec<f64>],
        angular_frequency: f64,
    ) -> Result<Vec<f64>> {
        hierarchical_magnetic::adaptive::resistive_goals(self.space, columns, angular_frequency)
    }
    pub fn actions_certified(
        &self,
        columns: &[Vec<f64>],
        absolute_goals: &[f64],
    ) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, serde_json::Value)> {
        self.hierarchical
            .as_ref()
            .ok_or_else(|| {
                error(
                    "HIERARCHICAL_MAGNETIC_POLICY",
                    "adaptive common-batch tree required",
                )
            })?
            .batch(&self.pair, self.space, columns, absolute_goals)
    }

    /// Upper triangle visited once per batch. Both action rows receive the
    /// same scalar pair and exact vector-axis dot product.
    pub fn actions(&self, columns: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
        let n = self.space.current_count();
        if columns
            .iter()
            .any(|col| col.len() != n || col.iter().any(|v| !v.is_finite()))
        {
            return Err(error(
                "MAGNETIC_INPUT",
                "finite native current columns required",
            ));
        }
        if columns.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = vec![vec![Sum::default(); n]; columns.len()];
        for (i, target) in self.space.cells.iter().enumerate() {
            for (j, source) in self.space.cells[..=i].iter().enumerate() {
                check_deadline(self.start, self.wall)?;
                let coefficient = 1e-7
                    * target.density_per_amp_m2
                    * source.density_per_amp_m2
                    * self.pair.pair(i, j)?;
                for (column, action) in columns.iter().zip(&mut out) {
                    for k in 0..3 {
                        action[3 * i + k] =
                            action[3 * i + k].add(Sum::from(column[3 * j + k]).mul(coefficient));
                        if i != j {
                            action[3 * j + k] = action[3 * j + k]
                                .add(Sum::from(column[3 * i + k]).mul(coefficient));
                        }
                    }
                }
            }
        }
        out.into_iter()
            .map(|col| {
                let values: Vec<_> = col.into_iter().map(Sum::value).collect();
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(error(
                        "MAGNETIC_RANGE",
                        "nonfinite streamed volume magnetic action",
                    ));
                }
                Ok(values)
            })
            .collect()
    }
}

fn check_deadline(start: Instant, wall: Duration) -> Result<()> {
    if start.elapsed() >= wall {
        return Err(error("DEADLINE", "streamed tetra magnetic action"));
    }
    Ok(())
}
pub struct Operator<'a> {
    mesh: &'a LayeredMesh,
    kernel: Vec<f64>,
    planned_bytes: usize,
}

impl<'a> Operator<'a> {
    pub fn new(mesh: &'a LayeredMesh, budget: AllocationBudget, order: usize) -> Result<Self> {
        let n = mesh.copper_tetrahedra.len();
        if n == 0 {
            return Err(error(
                "MAGNETIC_MESH",
                "direct volume integral requires copper cells",
            ));
        }
        let kernel_bytes = n
            .checked_mul(n)
            .and_then(|v| v.checked_mul(8))
            .ok_or_else(|| error("MEMORY_LIMIT", "direct magnetic kernel overflow"))?;
        let cubature_bytes = PairQuadrature::extra_bytes(n, order)?;
        let plan = mesh
            .retained_bytes()?
            .checked_add(kernel_bytes)
            .and_then(|v| v.checked_add(cubature_bytes))
            .ok_or_else(|| error("MEMORY_LIMIT", "direct magnetic plan overflow"))?;
        budget.check(plan)?;
        let tetra = geometry(mesh);
        let kernel = magnetic_kernel_cache::square(&tetra, order, 1e-10, |tetra| {
            let pair = PairQuadrature::new(tetra, order, || Ok(()))?;
            let mut kernel = vec![0.; n * n];
            for i in 0..n {
                for j in 0..=i {
                    let value = pair.pair(i, j)?;
                    kernel[i * n + j] = value;
                    kernel[j * n + i] = value;
                }
            }
            Ok(kernel)
        })?;
        Ok(Self {
            mesh,
            kernel,
            planned_bytes: plan,
        })
    }
    fn apply_current(&self, space: &Space, current: &[f64]) -> Result<Vec<f64>> {
        if current.len() != space.current_count() || current.iter().any(|x| !x.is_finite()) {
            return Err(error(
                "MAGNETIC_INPUT",
                "finite full current vector required",
            ));
        }
        let n = self.mesh.copper_tetrahedra.len();
        let mut out = vec![Sum::default(); current.len()];
        for (i, target) in space.cells.iter().enumerate() {
            for (j, source) in space.cells.iter().enumerate() {
                let coefficient = 1e-7
                    * target.density_per_amp_m2
                    * source.density_per_amp_m2
                    * self.kernel[target.mesh_cell * n + source.mesh_cell];
                for k in 0..3 {
                    out[3 * i + k] =
                        out[3 * i + k].add(Sum::from(current[3 * j + k]).mul(coefficient));
                }
            }
        }
        let out: Vec<_> = out.into_iter().map(|v| v.value()).collect();
        if out.iter().any(|x| !x.is_finite()) {
            return Err(error(
                "MAGNETIC_RANGE",
                "nonfinite direct volume magnetic action",
            ));
        }
        Ok(out)
    }
}

impl MagneticApply for Operator<'_> {
    fn apply(&self, space: &Space, current: &[f64]) -> Result<Vec<f64>> {
        self.apply_current(space, current)
    }
    fn planned_bytes(&self) -> usize {
        self.planned_bytes
    }
    fn scratch_bytes(&self) -> usize {
        self.planned_bytes
    }
}

#[cfg(test)]
mod bounded_pair_tests {
    use super::*;
    #[test]
    fn bounded_pair_cache_eviction_preserves_exact_coefficients_and_preflights_memory() {
        let tetra = |x: f64| {
            [
                [x, 0., 0.],
                [x + 0.001, 0., 0.],
                [x, 0.001, 0.],
                [x, 0., 0.001],
            ]
        };
        let cells = vec![tetra(0.), tetra(0.001), tetra(0.1)];
        let reference = PairQuadrature::new(&cells, 12, || Ok(())).unwrap();
        let bounded =
            BoundedPairs::new(cells, 12, 2, Instant::now(), Duration::from_secs(30)).unwrap();
        for (i, j) in [(2, 0), (1, 0), (2, 1), (1, 1), (2, 0)] {
            let expected = reference.pair(i, j).unwrap();
            let actual = bounded.pair(i, j).unwrap();
            assert_eq!(actual.to_bits(), expected.to_bits(), "pair({i},{j})");
            assert!(bounded.cells.borrow().len() <= 2);
        }
        let budget = AllocationBudget {
            max_bytes: 32 * 1024 * 1024,
            max_nodes: 1000000,
            max_tetrahedra: 1000000,
        };
        let (owned, slots) = BoundedPairs::plan(154899, 12, 0, budget).unwrap();
        assert_eq!(slots, 64);
        assert!(owned < 24 * 1024 * 1024);
        assert!(BoundedPairs::plan(
            154899,
            12,
            0,
            AllocationBudget {
                max_bytes: 1,
                ..budget
            }
        )
        .is_err());
        assert!(BoundedPairs::new(vec![tetra(0.)], 12, 2, Instant::now(), Duration::ZERO).is_err());
    }
}

#[cfg(test)]
mod actual_tree_planning {
    use super::*;
    #[test]
    #[ignore = "requires an explicitly supplied saved actual-board request and supervisor"]
    fn saved_stm32_geometry_and_hierarchy_plan_only() {
        use std::{env, fs, path::PathBuf};
        let request_path = env::var("P7_SAVED_PROBLEM").expect("explicit saved problem path");
        let output = PathBuf::from(env::var("P7_TEST_EVIDENCE_DIR").expect("evidence directory"));
        let raw = fs::read_to_string(request_path).unwrap();
        let problem: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let board = crate::import_json(problem["boardJson"].as_str().unwrap())
            .board
            .unwrap();
        let request: super::super::board_mesh::Request =
            serde_json::from_value(problem["geometry"].clone()).unwrap();
        let started = Instant::now();
        let wall = Duration::from_secs(1800);
        let mut budget = super::super::board_prepare::allocation_budget(3072);
        let ports = request.ports.len();
        let samples = problem["request"]["frequenciesHz"]
            .as_array()
            .unwrap()
            .len();
        budget.max_bytes -= samples * ports * (ports * 256 + 512) + 256 * 1024;
        let mesh = super::super::board_mesh::build(&board, &request, budget, wall).unwrap();
        let mesh_retained = mesh.mesh.retained_bytes().unwrap();
        // Preserve the larger geometry preparation bound and reserve decoded
        // saved-input storage, rather than allocating a second whole budget.
        let geometry_base = mesh
            .mesh
            .planned_peak_bytes
            .max(mesh_retained)
            .checked_add(raw.len().checked_mul(8).unwrap())
            .unwrap();
        let tetra_bytes = mesh
            .mesh
            .copper_tetrahedra
            .len()
            .checked_mul(std::mem::size_of::<[[f64; 3]; 4]>())
            .unwrap();
        let base = geometry_base.checked_add(tetra_bytes).unwrap();
        budget.check(base).unwrap();
        let tetra = geometry(&mesh.mesh);
        let tree =
            hierarchical_magnetic::Operator::new(&tetra, 12, 1, 0.2, base, budget, started, wall);
        let report = match tree {
            Ok(tree) => serde_json::json!({
                "status":"pass", "phase":"geometry and hierarchy plan only",
                "seconds":started.elapsed().as_secs_f64(), "operator":tree.evidence(),
                "meshRetainedBytes":mesh_retained, "meshPreparationBoundBytes":mesh.mesh.planned_peak_bytes,
                "geometryAndInputBaseBytes":geometry_base, "tetraGeometryBytes":tetra_bytes,
                "totalPlannedBytes":base.checked_add(tree.planned_bytes).unwrap(),
                "admittedBudgetBytes":budget.max_bytes, "fieldSolves":0, "magneticActions":0,
                "originalCopperTetrahedra":tetra.len(), "sourceMappings":mesh.source_mapping.len(),
                "barrels":mesh.barrel_ids.len(), "physicalCoverage":"unverified"
            }),
            Err(error) => serde_json::json!({
                "status":"refused", "phase":"hierarchy planning", "error":error,
                "seconds":started.elapsed().as_secs_f64(), "geometryAndInputBaseBytes":geometry_base,
                "tetraGeometryBytes":tetra_bytes, "admittedBudgetBytes":budget.max_bytes,
                "fieldSolves":0, "magneticActions":0
            }),
        };
        fs::create_dir_all(&output).unwrap();
        fs::write(
            output.join("comparison.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("P7_ACTUAL_TREE_PREFLIGHT {report}");
        assert_eq!(report["status"], "pass", "{report}");
    }
}
