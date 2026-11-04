//! Direct constant-vector finite-volume magnetostatic action on the unchanged
//! copper cells. The dense kernel is intended for bounded local blocks; its
//! quadratic storage is admitted explicitly instead of hidden behind a mesh solve.
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
    pair: PairQuadrature,
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
        let owned_bytes = PairQuadrature::extra_bytes(n, order)?;
        budget.check(
            base_bytes
                .checked_add(owned_bytes)
                .ok_or_else(|| error("MEMORY_LIMIT", "streamed tetra quadrature plan overflow"))?,
        )?;
        let pair = PairQuadrature::new(&geometry(mesh), order, || check_deadline(start, wall))?;
        Ok(Self {
            space,
            pair,
            owned_bytes,
            start,
            wall,
        })
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
