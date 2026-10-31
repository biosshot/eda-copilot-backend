//! Equilibrated RT0 face-current recovery on the unchanged conforming copper mesh.
//! This is an observable postprocessor: mixed PDE, R/L/C and magnetic states stay unchanged.
use super::{error, mesh::LayeredMesh, volume_current::Space, AllocationBudget, Result};
use faer::{dyn_stack::{MemBuffer, MemStack}, sparse::{linalg::cholesky, SparseColMat, Triplet}, Conj, Mat, Par, Side};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FaceCell {
    pub cell: usize,
    pub opposite: usize,
    /// +1 when the global face normal is the tetrahedron's outward normal.
    pub sign: f64,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Face {
    pub nodes: [usize; 3],
    pub cells: Vec<FaceCell>,
    pub electrode: bool,
}

struct Projector {
    free_faces: Vec<usize>,
    /// Constraint columns in globally oriented face-flux coordinates.
    columns: Vec<Vec<(usize, f64)>>,
    row_map: Vec<usize>,
    row_count: usize,
    symbolic: cholesky::SymbolicCholesky<usize>,
    values: Vec<f64>,
    planned_bytes: usize,
}

/// Topology is reusable; the two boundary conditions have separate bounded factors.
/// DC insulation fixes every non-electrode boundary flux to zero. AC retains the
/// original charged-surface reactions through boundary flux unknowns.
pub struct ConservativeSpace {
    pub faces: Vec<Face>,
    cell_faces: Vec<Vec<usize>>,
    dc: Projector,
    ac: Projector,
    pub planned_bytes: usize,
}

fn add_entry(map: &mut BTreeMap<usize, f64>, row: usize, value: f64) {
    *map.entry(row).or_insert(0.) += value;
}

impl ConservativeSpace {
    pub fn new(mesh: &LayeredMesh, space: &Space, electrodes: &[Vec<usize>], budget: AllocationBudget) -> Result<Self> {
        let nt = space.cells.len();
        let nv = space.potential_count();
        if nt == 0 || nv == 0 || mesh.copper_tetrahedra.len() != nt {
            return Err(error("CONSERVATIVE_INPUT", "matching nonempty copper mesh and current space required"));
        }
        let electrode_dofs: BTreeSet<usize> = electrodes.iter().map(|nodes| {
            let first = nodes.first().ok_or_else(|| error("CONSERVATIVE_ELECTRODE", "empty electrode"))?;
            space.node_potential.get(first).copied().ok_or_else(|| error("CONSERVATIVE_ELECTRODE", "electrode is outside copper"))
        }).collect::<Result<_>>()?;
        let mut faces = BTreeMap::<[usize; 3], Vec<FaceCell>>::new();
        for (cell, tetra) in mesh.copper_tetrahedra.iter().enumerate() {
            for opposite in 0..4 {
                let mut nodes = [0; 3];
                let mut at = 0;
                for k in 0..4 { if k != opposite { nodes[at] = tetra.nodes[k]; at += 1; } }
                nodes.sort();
                let p = nodes.map(|n| mesh.nodes_m[n]);
                let a = [p[1][0]-p[0][0], p[1][1]-p[0][1], p[1][2]-p[0][2]];
                let b = [p[2][0]-p[0][0], p[2][1]-p[0][1], p[2][2]-p[0][2]];
                let normal = [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]];
                let outward = tetra.gradients_per_m[opposite].map(|x| -x);
                let orientation = (0..3).map(|k| normal[k]*outward[k]).sum::<f64>();
                if !orientation.is_finite() || orientation == 0. {
                    return Err(error("CONSERVATIVE_FACE", "degenerate face orientation"));
                }
                faces.entry(nodes).or_default().push(FaceCell { cell, opposite, sign: if orientation > 0. { 1. } else { -1. } });
            }
        }
        let mut owned_faces = Vec::with_capacity(faces.len());
        let mut cell_faces = vec![vec![]; nt];
        for (nodes, cells) in faces {
            if cells.len() > 2 || cells.iter().any(|c| c.cell >= nt || c.opposite >= 4) {
                return Err(error("CONSERVATIVE_FACE", "nonmanifold copper face"));
            }
            let electrode = cells.len() == 1 && nodes.iter().all(|node| {
                space.node_potential.get(node).is_some_and(|dof| electrode_dofs.contains(dof))
            });
            let index = owned_faces.len();
            for cell in &cells { cell_faces[cell.cell].push(index); }
            owned_faces.push(Face { nodes, cells, electrode });
        }
        if owned_faces.is_empty() || cell_faces.iter().any(|faces| faces.len() != 4) {
            return Err(error("CONSERVATIVE_FACE", "complete four-face tetrahedral ownership required"));
        }
        let mut component_of_node = vec![usize::MAX; nv];
        for (component, nodes) in space.components.iter().enumerate() {
            for &node in nodes { component_of_node[node] = component; }
        }
        let mut gauged_cells = vec![false; nt];
        let mut dc_gauged_cells = vec![false; nt];
        for component in 0..space.components.len() {
            let cell = (0..nt).rev().find(|&cell| component_of_node[space.cells[cell].potentials[0]] == component)
                .ok_or_else(|| error("CONSERVATIVE_COMPONENT", "component has no current cell"))?;
            gauged_cells[cell] = true;
            // With insulated non-electrode boundaries, 3 r_v = sum_{T containing v} div_T.
            // Eliminating those exact star dependencies leaves one component relation
            // 3 sum_electrode r + sum_T (k_T-1) div_T = 0, where k_T counts distinct
            // non-electrode potential DOFs in T. Gauge only a nonzero coefficient.
            let dc_cell = (0..nt).rev().find(|&cell| {
                if component_of_node[space.cells[cell].potentials[0]] != component { return false; }
                let non_electrode = space.cells[cell].potentials.iter().filter(|node| !electrode_dofs.contains(node)).collect::<BTreeSet<_>>().len();
                non_electrode != 1
            });
            if let Some(cell) = dc_cell { dc_gauged_cells[cell] = true; }
        }
        let topology_bytes = owned_faces.len().checked_mul(256)
            .and_then(|n| n.checked_add(nt.checked_mul(128)?))
            .and_then(|n| n.checked_add(space.planned_bytes))
            .ok_or_else(|| error("MEMORY_LIMIT", "conservative topology plan overflow"))?;
        budget.check(topology_bytes)?;
        let dc = Self::projector(space, &owned_faces, &electrode_dofs, &dc_gauged_cells, true, topology_bytes, budget)?;
        let ac = Self::projector(space, &owned_faces, &electrode_dofs, &gauged_cells, false, dc.planned_bytes, budget)?;
        let planned_bytes = dc.planned_bytes.max(ac.planned_bytes);
        budget.check(planned_bytes)?;
        Ok(Self { faces: owned_faces, cell_faces, dc, ac, planned_bytes })
    }

    fn projector(space: &Space, faces: &[Face], electrode_dofs: &BTreeSet<usize>, gauged_cells: &[bool], insulated: bool, base: usize, budget: AllocationBudget) -> Result<Projector> {
        let nv = space.potential_count();
        let nt = space.cells.len();
        let raw_rows = nv + nt;
        let mut row_map = vec![usize::MAX; raw_rows];
        let mut row_count = 0;
        // DC non-electrode reactions follow from cell divergence and insulation;
        // recover_with still audits every omitted raw row against the full residual.
        for node in 0..nv { if !insulated || electrode_dofs.contains(&node) { row_map[node] = row_count; row_count += 1; } }
        for cell in 0..nt { if !gauged_cells[cell] { row_map[nv+cell] = row_count; row_count += 1; } }
        let free_faces: Vec<usize> = faces.iter().enumerate().filter_map(|(index, face)| {
            (face.cells.len() == 2 || face.electrode || !insulated).then_some(index)
        }).collect();
        if row_count == 0 || free_faces.is_empty() {
            return Err(error("CONSERVATIVE_SPACE", "free face/constraint space required"));
        }
        let mut columns = Vec::with_capacity(free_faces.len());
        let mut terms = BTreeMap::<(usize, usize), f64>::new();
        for &face_index in &free_faces {
            let mut column = BTreeMap::new();
            for incidence in &faces[face_index].cells {
                // A tetrahedron's nodal reaction is one third of its physical
                // opposite-face flux; cell divergence uses the complete flux.
                add_entry(&mut column, row_map[space.cells[incidence.cell].potentials[incidence.opposite]], incidence.sign/3.);
                let cell_row = nv + incidence.cell;
                if !gauged_cells[incidence.cell] { add_entry(&mut column, row_map[cell_row], incidence.sign); }
            }
            let column: Vec<(usize, f64)> = column.into_iter().filter(|(row, value)| *row != usize::MAX && *value != 0.).collect();
            for &(a, x) in &column { for &(b, y) in &column {
                *terms.entry((a, b)).or_insert(0.) += x*y;
            }}
            columns.push(column);
        }
        let incidence_count = columns.iter().map(Vec::len).sum::<usize>().checked_mul(32).ok_or_else(|| error("MEMORY_LIMIT", "conservative incidence overflow"))?;
        let plan = base.checked_add(incidence_count)
            .and_then(|n| n.checked_add(terms.len().checked_mul(128)?))
            .and_then(|n| n.checked_add(row_count.checked_mul(1024)?))
            .ok_or_else(|| error("MEMORY_LIMIT", "conservative factor plan overflow"))?;
        budget.check(plan)?;
        let triplets: Vec<_> = terms.into_iter().map(|((i,j),v)| Triplet::new(i,j,v)).collect();
        let matrix = SparseColMat::<usize,f64>::try_new_from_triplets(row_count,row_count,&triplets)
            .map_err(|e| error("CONSERVATIVE_MATRIX", &e.to_string()))?;
        let symbolic = cholesky::factorize_symbolic_cholesky(matrix.symbolic(),Side::Lower,Default::default(),Default::default())
            .map_err(|e| error("CONSERVATIVE_MATRIX", &e.to_string()))?;
        let factor_scratch = symbolic.factorize_numeric_llt_scratch::<f64>(Par::Seq,Default::default());
        let solve_scratch = symbolic.solve_in_place_scratch::<f64>(1,Par::Seq);
        let plan = plan.checked_add(symbolic.len_val().checked_mul(8).ok_or_else(|| error("MEMORY_LIMIT", "conservative fill overflow"))?)
            .and_then(|n| n.checked_add(factor_scratch.unaligned_bytes_required().max(solve_scratch.unaligned_bytes_required())))
            .ok_or_else(|| error("MEMORY_LIMIT", "conservative scratch overflow"))?;
        budget.check(plan)?;
        let mut values = vec![0.; symbolic.len_val()];
        let mut workspace = MemBuffer::try_new(factor_scratch).map_err(|_| error("MEMORY_LIMIT", "conservative factor workspace"))?;
        symbolic.factorize_numeric_llt(&mut values,matrix.as_ref(),Side::Lower,Default::default(),Par::Seq,MemStack::new(&mut workspace),Default::default())
            .map_err(|_| error("CONSERVATIVE_SINGULAR", "independent equilibration constraints are not positive definite; no regularization"))?;
        Ok(Projector { free_faces, columns, row_map, row_count, symbolic, values, planned_bytes: plan })
    }

    fn solve_multiplier(&self, projector: &Projector, rhs: &[f64]) -> Result<Vec<f64>> {
        let mut value = Mat::from_fn(projector.row_count,1,|i,_| rhs[i]);
        let mut workspace = MemBuffer::try_new(projector.symbolic.solve_in_place_scratch::<f64>(1,Par::Seq))
            .map_err(|_| error("MEMORY_LIMIT", "conservative solve workspace"))?;
        cholesky::LltRef::new(&projector.symbolic,&projector.values)
            .solve_in_place_with_conj(Conj::No,value.as_mut(),Par::Seq,MemStack::new(&mut workspace));
        Ok((0..projector.row_count).map(|i| value[(i,0)]).collect())
    }

    fn raw_faces(&self, space: &Space, currents: &[f64]) -> Vec<f64> {
        self.faces.iter().map(|face| {
            face.cells.iter().map(|incidence| {
                let cell = &space.cells[incidence.cell];
                let q = (0..3).map(|axis| currents[3*incidence.cell+axis]*cell.incidence[incidence.opposite][axis]).sum::<f64>();
                3.*incidence.sign*q
            }).sum::<f64>()/face.cells.len() as f64
        }).collect()
    }

    fn recover_with(&self, space: &Space, currents: &[f64], projector: &Projector, insulated: bool) -> Result<Vec<f64>> {
        let nv = space.potential_count();
        let nt = space.cells.len();
        let target = space.divergence(currents)?;
        let raw = self.raw_faces(space,currents);
        let free: BTreeMap<usize,usize> = projector.free_faces.iter().enumerate().map(|(index,&face)|(face,index)).collect();
        let raw_rows: Vec<usize> = (0..projector.row_count).map(|row| {
            projector.row_map.iter().position(|&mapped| mapped == row).unwrap()
        }).collect();
        let mut flux = vec![0.; self.faces.len()];
        for &face in free.keys() { flux[face] = raw[face]; }
        let correction = |flux: &mut [f64]| -> Result<()> {
            let mut residual = vec![0.; nv+nt];
            residual[..nv].copy_from_slice(&target);
            for (&face,&column) in &free {
                for &(row,value) in &projector.columns[column] {
                    residual[raw_rows[row]] -= value*flux[face];
                }
            }
            let reduced: Vec<f64> = raw_rows.iter().map(|&raw_row| residual[raw_row]).collect();
            if reduced.iter().any(|v| !v.is_finite()) { return Err(error("CONSERVATIVE_RANGE", "nonfinite equilibration residual")); }
            let multiplier = self.solve_multiplier(projector,&reduced)?;
            if multiplier.iter().any(|v| !v.is_finite()) { return Err(error("CONSERVATIVE_RANGE", "nonfinite equilibration multiplier")); }
            for (&face,&column) in &free {
                for &(row,value) in &projector.columns[column] { flux[face] += value*multiplier[row]; }
            }
            Ok(())
        };
        // Two deterministic residual corrections use the same bounded factor.
        correction(&mut flux)?;
        correction(&mut flux)?;
        if flux.iter().any(|v| !v.is_finite()) { return Err(error("CONSERVATIVE_RANGE", "nonfinite recovered face flux")); }
        if insulated {
            for (index,face) in self.faces.iter().enumerate() {
                if face.cells.len() == 1 && !face.electrode && flux[index] != 0. {
                    return Err(error("CONSERVATIVE_BOUNDARY", "DC insulation changed a fixed face"));
                }
            }
        }
        let mut reactions = vec![0.; nv];
        let mut reaction_scale = vec![0.; nv];
        let mut divergence = vec![0.; nt];
        let mut divergence_scale = vec![0.; nt];
        for (face_index,face) in self.faces.iter().enumerate() {
            for incidence in &face.cells {
                let value = incidence.sign*flux[face_index];
                let node = space.cells[incidence.cell].potentials[incidence.opposite];
                reactions[node] += value/3.;
                reaction_scale[node] += value.abs()/3.;
                divergence[incidence.cell] += value;
                divergence_scale[incidence.cell] += value.abs();
            }
        }
        // Each row may pass either its local relative test or the absolute
        // component of the same 1e-8 test at this state's current scale.
        // This distinguishes zero-row roundoff from finite incompatibility.
        let target_scale=target.iter().map(|v|v.abs()).fold(0.,f64::max).max(f64::MIN_POSITIVE);
        let reaction_error = (0..nv).map(|node| (reactions[node]-target[node]).abs()/(reaction_scale[node]+target[node].abs()).max(target_scale)).fold(0.,f64::max);
        let divergence_error = (0..nt).map(|cell| divergence[cell].abs()/divergence_scale[cell].max(target_scale)).fold(0.,f64::max);
        if !reaction_error.is_finite() || !divergence_error.is_finite() || reaction_error > 1e-8 || divergence_error > 1e-8 {
            return Err(error("CONSERVATIVE_RESIDUAL", &format!("nodal={reaction_error:e} cell={divergence_error:e}")));
        }
        Ok(flux)
    }

    /// `insulated=true` is the DC/no-charge boundary condition. `false` retains
    /// charged copper-surface reactions in an AC phasor state.
    pub fn recover(&self, space: &Space, currents: &[f64], insulated: bool) -> Result<Vec<f64>> {
        if currents.len() != space.current_count() || currents.iter().any(|v| !v.is_finite()) {
            return Err(error("CONSERVATIVE_INPUT", "finite full cell-current coordinates required"));
        }
        self.recover_with(space,currents,if insulated { &self.dc } else { &self.ac },insulated)
    }

    pub fn project_basis(&self, space: &Space, basis: &[Vec<f64>], insulated: bool) -> Result<Vec<Vec<f64>>> {
        basis.iter().map(|current| self.recover(space,current,insulated)).collect()
    }

    pub fn cell_divergence(&self, space: &Space, flux: &[f64]) -> Result<Vec<f64>> {
        if flux.len() != self.faces.len() || flux.iter().any(|v| !v.is_finite()) {
            return Err(error("CONSERVATIVE_INPUT", "finite full face flux required"));
        }
        Ok(self.cell_faces.iter().enumerate().map(|(cell,faces)| {
            let _ = space.cells[cell];
            faces.iter().map(|&face| self.faces[face].cells.iter().find(|incidence| incidence.cell == cell).unwrap().sign*flux[face]).sum()
        }).collect())
    }
}
