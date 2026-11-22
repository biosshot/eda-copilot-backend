//! Magnetostatic mutual inductance between disjoint extracted channel blocks.
//! Every tetrahedron carries one constant Cartesian current density per local
//! current DOF. Self terms remain owned by the original block producers.
use super::{error, local::{accurate::Sum, magnetic_kernel_cache, polyhedron::{PointPotential, TetraCubature, separated_order, tetra_integral_adaptive}}, Result};
use crate::electrical::Budget;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Cell {
    vertices_m: [[f64; 3]; 4],
    currents_per_amp_m2: Vec<[f64; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Block {
    id: String,
    dofs: usize,
    cells: Vec<Cell>,
    #[serde(default)]
    direct_self: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    blocks: Vec<Block>,
    order: Option<usize>,
    budget: Option<Budget>,
}

pub fn run(request: Request) -> Result<Value> {
    let start = Instant::now();
    let order = request.order.unwrap_or(12);
    let seconds = request.budget.as_ref().and_then(|b| b.max_seconds).unwrap_or(300.);
    let memory = request.budget.as_ref().and_then(|b| b.max_memory_mi_b).unwrap_or(1024.);
    if !(8..=16).contains(&order)
        || request.blocks.is_empty() || request.blocks.len() > 32
        || !seconds.is_finite() || seconds <= 0.
        || !memory.is_finite() || memory <= 0. || memory > 4096.
    {
        return Err(error("CHANNEL_MAGNETIC_INPUT", "1..32 blocks, quadrature order 8..16 and finite positive budgets required"));
    }
    let ids: BTreeSet<_> = request.blocks.iter().map(|b| &b.id).collect();
    let total_dofs = request.blocks.iter().map(|b| b.dofs).sum::<usize>();
    let total_cells = request.blocks.iter().map(|b| b.cells.len()).sum::<usize>();
    if ids.len() != request.blocks.len()
        || request.blocks.iter().any(|b| b.id.trim().is_empty() || b.dofs == 0 || b.cells.is_empty())
        || total_dofs > 1536 || total_cells > 4096
    {
        return Err(error("CHANNEL_MAGNETIC_INPUT", "Unique named nonempty blocks within 1536 DOFs and 4096 cells required"));
    }
    let mut planned = 0usize;
    for block in &request.blocks {
        for cell in &block.cells {
            if cell.currents_per_amp_m2.len() != block.dofs
                || cell.vertices_m.iter().flatten().chain(cell.currents_per_amp_m2.iter().flatten()).any(|v| !v.is_finite())
            {
                return Err(error("CHANNEL_MAGNETIC_INPUT", format!("Block {} has a nonfinite cell or current-basis size mismatch", block.id)));
            }
            planned = planned.checked_add(12 * 8 + block.dofs * 3 * 8 + 2048)
                .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Input allocation plan overflow"))?;
        }
    }
    let pair_cells = (0..request.blocks.len()).flat_map(|i| (0..i).map(move |j| (i, j)))
        .map(|(i, j)| request.blocks[i].cells.len().checked_mul(request.blocks[j].cells.len()))
        .collect::<Option<Vec<_>>>().ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Pair allocation plan overflow"))?;
    let pair_kernel = pair_cells.iter().copied().max().unwrap_or(0).checked_mul(8)
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Kernel allocation plan overflow"))?;
    let self_kernel = request.blocks.iter().filter(|b| b.direct_self)
        .map(|b| b.cells.len().checked_mul(b.cells.len())).collect::<Option<Vec<_>>>()
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Self kernel allocation plan overflow"))?
        .into_iter().max().unwrap_or(0).checked_mul(8)
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Self kernel allocation plan overflow"))?;
    let outputs = (0..request.blocks.len()).flat_map(|i| (0..i).map(move |j| (i, j)))
        .try_fold(0usize, |n, (i, j)| n.checked_add(request.blocks[i].dofs.checked_mul(request.blocks[j].dofs)?))
        .and_then(|n| n.checked_mul(8))
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Output allocation plan overflow"))?;
    let self_outputs = request.blocks.iter().filter(|b| b.direct_self)
        .try_fold(0usize, |n, b| n.checked_add(b.dofs.checked_mul(b.dofs)?))
        .and_then(|n| n.checked_mul(8))
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Self output allocation plan overflow"))?;
    let orders: Vec<_> = [4, 6, 8, 12, 16].into_iter().filter(|&value| value <= order).collect();
    let cubature_bytes = total_cells.checked_mul(orders.iter().map(|value| value.pow(3) * 32).sum())
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Cubature allocation plan overflow"))?;
    planned = planned.checked_add(pair_kernel.max(self_kernel)).and_then(|n| n.checked_add(outputs)).and_then(|n| n.checked_add(self_outputs)).and_then(|n| n.checked_add(cubature_bytes))
        .ok_or_else(|| error("CHANNEL_MAGNETIC_BUDGET", "Total allocation plan overflow"))?;
    if planned as f64 > memory * 1048576. {
        return Err(error("CHANNEL_MAGNETIC_BUDGET", format!("Planned {planned} bytes exceed request memory")));
    }
    let mut self_matrices = vec![];
    for block in &request.blocks {
        if !block.direct_self { continue; }
        let n = block.cells.len();
        let tetra: Vec<_> = block.cells.iter().map(|cell| cell.vertices_m).collect();
        let kernel = magnetic_kernel_cache::square(&tetra, order, 1e-12, |tetra| {
            let sources: Vec<_> = tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<Vec<_>>>()?;
            let cubatures: Vec<Vec<_>> = tetra.iter().copied().map(|cell| orders.iter().map(|&value| TetraCubature::new(cell, value)).collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
            let mut kernel = vec![0.; n * n];
            for i in 0..n {
                for j in 0..=i {
                    let value = if i == j {
                        super::local::polyhedron::tetra_self_integral(tetra[i], order)?
                    } else {
                        let q = cubatures[i][0].separation_q(&cubatures[j][0]);
                        if let Some(index) = separated_order(q, order).and_then(|value| orders.iter().position(|&candidate| candidate == value)) {
                            cubatures[i][index].direct_integral(&cubatures[j][index])?
                        } else {
                            tetra_integral_adaptive(tetra[i], &sources[j], order, 1e-12)?
                        }
                    };
                    kernel[i * n + j] = value;
                    kernel[j * n + i] = value;
                }
                if i % 16 == 0 && start.elapsed().as_secs_f64() > seconds {
                    return Err(error("CHANNEL_MAGNETIC_DEADLINE", "Shared self-inductance deadline exceeded"));
                }
            }
            Ok(kernel)
        })?;
        let mut matrix = vec![vec![Sum::default(); block.dofs]; block.dofs];
        for (i, target) in block.cells.iter().enumerate() {
            for (j, source) in block.cells.iter().enumerate() {
                for u in 0..block.dofs {
                    for v in 0..block.dofs {
                        let dot = (0..3).map(|axis| target.currents_per_amp_m2[u][axis] * source.currents_per_amp_m2[v][axis]).sum::<f64>();
                        matrix[u][v] = matrix[u][v].add(Sum::from(dot).mul(kernel[i * n + j]));
                    }
                }
            }
        }
        let matrix: Vec<Vec<f64>> = matrix.into_iter().map(|row| row.into_iter().map(|v| 1e-7 * v.value()).collect()).collect();
        if matrix.iter().flatten().any(|v| !v.is_finite()) {
            return Err(error("CHANNEL_MAGNETIC_RANGE", "Nonfinite direct self-inductance"));
        }
        self_matrices.push(json!({"id":block.id,"matrixH":matrix,"cellPairs":n*n}));
    }
    let mut couplings = vec![];
    for a in 0..request.blocks.len() {
        for b in 0..a {
            if start.elapsed().as_secs_f64() > seconds {
                return Err(error("CHANNEL_MAGNETIC_DEADLINE", "Shared mutual-inductance deadline exceeded"));
            }
            let left = &request.blocks[a];
            let right = &request.blocks[b];
            let left_tetra: Vec<_> = left.cells.iter().map(|cell| cell.vertices_m).collect();
            let right_tetra: Vec<_> = right.cells.iter().map(|cell| cell.vertices_m).collect();
            let kernel = magnetic_kernel_cache::pair(&left_tetra, &right_tetra, order, 1e-12, |left_tetra, right_tetra| {
                let sources: Vec<_> = right_tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<Vec<_>>>()?;
                let left_cubatures: Vec<Vec<_>> = left_tetra.iter().copied().map(|cell| orders.iter().map(|&value| TetraCubature::new(cell, value)).collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
                let right_cubatures: Vec<Vec<_>> = right_tetra.iter().copied().map(|cell| orders.iter().map(|&value| TetraCubature::new(cell, value)).collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
                let mut kernel = vec![0.; left_tetra.len() * right_tetra.len()];
                for (i, target) in left_tetra.iter().enumerate() {
                    for (j, source) in sources.iter().enumerate() {
                        let q = left_cubatures[i][0].separation_q(&right_cubatures[j][0]);
                        let direct = separated_order(q, order);
                        kernel[i * right_tetra.len() + j] = if let Some(index) = direct.and_then(|value| orders.iter().position(|&candidate| candidate == value)) {
                            left_cubatures[i][index].direct_integral(&right_cubatures[j][index])?
                        } else {
                            tetra_integral_adaptive(*target, source, order, 1e-12)?
                        };
                    }
                    if i % 16 == 0 && start.elapsed().as_secs_f64() > seconds {
                        return Err(error("CHANNEL_MAGNETIC_DEADLINE", "Shared kernel deadline exceeded"));
                    }
                }
                Ok(kernel)
            })?;
            let mut matrix = vec![vec![Sum::default(); right.dofs]; left.dofs];
            for (i, target) in left.cells.iter().enumerate() {
                for (j, source) in right.cells.iter().enumerate() {
                    let integral = kernel[i * right.cells.len() + j];
                    for u in 0..left.dofs {
                        for v in 0..right.dofs {
                            let dot = (0..3).map(|axis| target.currents_per_amp_m2[u][axis] * source.currents_per_amp_m2[v][axis]).sum::<f64>();
                            matrix[u][v] = matrix[u][v].add(Sum::from(dot).mul(integral));
                        }
                    }
                }
            }
            let matrix: Vec<Vec<f64>> = matrix.into_iter().map(|row| row.into_iter().map(|v| 1e-7 * v.value()).collect()).collect();
            if matrix.iter().flatten().any(|v| !v.is_finite()) {
                return Err(error("CHANNEL_MAGNETIC_RANGE", "Nonfinite cross-block inductance"));
            }
            couplings.push(json!({"a":left.id,"b":right.id,"matrixH":matrix,"cellPairs":left.cells.len()*right.cells.len()}));
        }
    }
    if start.elapsed().as_secs_f64() > seconds {
        return Err(error("CHANNEL_MAGNETIC_DEADLINE", "Shared output deadline exceeded"));
    }
    Ok(json!({"status":"complete","couplings":couplings,"selfMatrices":self_matrices,"quadratureOrder":order,"plannedBytes":planned,"selfTerms":"direct finite-volume matrices only for explicitly requested blocks; all other self terms remain with their source producers","mu0Over4Pi":1e-7,"elapsedMs":start.elapsed().as_secs_f64()*1000.}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::local::polyhedron::tetra_self_integral;
    fn tetra(offset: f64) -> [[f64; 3]; 4] {
        [[offset, 0., 0.], [offset + 0.001, 0., 0.], [offset, 0.001, 0.], [offset, 0., 0.001]]
    }
    fn request(sign: f64, offset: f64, order: usize) -> Request {
        serde_json::from_value(json!({"blocks":[
            {"id":"a","dofs":1,"cells":[{"verticesM":tetra(0.),"currentsPerAmpM2":[[0.,0.,1e6]]}]},
            {"id":"b","dofs":1,"cells":[{"verticesM":tetra(offset),"currentsPerAmpM2":[[0.,0.,sign*1e6]]}]}
        ],"order":order,"budget":{"maxSeconds":30.,"maxMemoryMiB":64.}})).unwrap()
    }
    #[test]
    fn separated_currents_are_reciprocal_convergent_and_signed() {
        let a = run(request(1., 0.01, 8)).unwrap();
        let b = run(request(1., 0.01, 12)).unwrap();
        let av = a["couplings"][0]["matrixH"][0][0].as_f64().unwrap();
        let bv = b["couplings"][0]["matrixH"][0][0].as_f64().unwrap();
        assert!(av > 0. && (av - bv).abs() / bv < 1e-10, "{av} vs {bv}");
        let mut reversed = request(1., 0.01, 12);
        reversed.blocks.reverse();
        let rv = run(reversed).unwrap()["couplings"][0]["matrixH"][0][0].as_f64().unwrap();
        assert!((rv - bv).abs() / bv < 1e-12, "directed reciprocity: {rv} vs {bv}");
        let negative = run(request(-1., 0.01, 12)).unwrap()["couplings"][0]["matrixH"][0][0].as_f64().unwrap();
        assert!((negative + bv).abs() / bv < 1e-14);
        let far = run(request(1., 0.1, 12)).unwrap()["couplings"][0]["matrixH"][0][0].as_f64().unwrap();
        assert!(far > 0. && far < bv);
        let self_a = 1e-7 * 1e12 * tetra_self_integral(tetra(0.), 12).unwrap();
        let self_b = 1e-7 * 1e12 * tetra_self_integral(tetra(0.01), 12).unwrap();
        for current in [1., -1.] {
            let energy = 0.5 * (self_a + self_b + 2. * current * bv);
            assert!(energy > 0. && energy.is_finite());
        }
    }
    #[test]
    fn requested_self_uses_the_same_finite_volume_operator() {
        let mut request = request(1., 0.01, 12);
        request.blocks[0].direct_self = true;
        let result = run(request).unwrap();
        let actual = result["selfMatrices"][0]["matrixH"][0][0].as_f64().unwrap();
        let expected = 1e-7 * 1e12 * tetra_self_integral(tetra(0.), 12).unwrap();
        assert!((actual - expected).abs() / expected < 1e-12, "{actual} vs {expected}");
        assert_eq!(result["selfMatrices"].as_array().unwrap().len(), 1);
        assert_eq!(result["couplings"].as_array().unwrap().len(), 1);
    }
    #[test]
    fn duplicate_ids_and_basis_shape_are_refused() {
        let duplicate = serde_json::from_value::<Request>(json!({"blocks":[
            {"id":"same","dofs":1,"cells":[{"verticesM":tetra(0.),"currentsPerAmpM2":[[1.,0.,0.]]}]},
            {"id":"same","dofs":1,"cells":[{"verticesM":tetra(0.01),"currentsPerAmpM2":[[1.,0.,0.]]}]}
        ]})).unwrap();
        assert!(run(duplicate).unwrap_err().contains("CHANNEL_MAGNETIC_INPUT"));
        let bad = serde_json::from_value::<Request>(json!({"blocks":[
            {"id":"a","dofs":2,"cells":[{"verticesM":tetra(0.),"currentsPerAmpM2":[[1.,0.,0.]]}]},
            {"id":"b","dofs":1,"cells":[{"verticesM":tetra(0.01),"currentsPerAmpM2":[[1.,0.,0.]]}]}
        ]})).unwrap();
        assert!(run(bad).unwrap_err().contains("size mismatch"));
    }
}
