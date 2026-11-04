//! Local low-frequency volume primitives. No board/DSL admission is exposed yet.
//!
//! SI units throughout. The geometry is finite copper, not an inferred return.
//! Full board extraction remains gated on independent quadrature/field evidence.
pub mod parallel;
pub mod capacitance;
pub mod current_space;
pub mod reduction;
pub mod layered_capacitance;
pub mod far;
pub mod near_far;
pub mod polyhedron;
pub mod mesh;
pub mod volume_current;
pub mod mixed_conductor;
pub mod conservative_current;
pub mod board_mesh;
mod copper_skin_profile;
pub mod board_prepare;
pub mod board_response;
pub mod board_dc;
mod volume_dc;
pub mod board_foil;
pub mod foil_reduced;
pub mod frequency_audit;
pub mod magnetic_actions;
pub mod developer_checkpoint;
pub mod volume_field;
pub mod field_refinement;
pub mod volume_magnetic;
pub mod volume_integral_magnetic;
pub mod magnetic_kernel_cache;
mod bernstein;
pub(crate) mod sparse_potential;
mod field_assembly;
pub mod volume_reduction;
mod spherical;
pub(crate) mod accurate;
mod compensated;
pub mod volume_solve;
pub mod prism;
pub mod sheet;

pub type Result<T> = std::result::Result<T, String>;

pub(crate) fn error(code: &str, detail: &str) -> String {
    format!("LOCAL_{code}: {detail}")
}

/// Explicit per-probe allocation budget. This does not change analyzer defaults.
#[derive(Clone, Copy, Debug)]
pub struct AllocationBudget {
    pub max_bytes: usize,
    pub max_nodes: usize,
    pub max_tetrahedra: usize,
}

impl AllocationBudget {
    pub fn check(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_bytes {
            Err(error(
                "MEMORY_LIMIT",
                "planned working set exceeds explicit budget",
            ))
        } else {
            Ok(())
        }
    }
}

/// One scalar foil kernel is shared by Cartesian x/x and y/y blocks. Axial
/// barrel z/z is separate; x/y, x/z and y/z dot products vanish exactly.
pub fn packed_kernel_bytes(foil_volumes: usize, axial_volumes: usize) -> Result<usize> {
    fn count(n: usize) -> Option<usize> {
        n.checked_add(1)?.checked_mul(n)?.checked_div(2)
    }
    count(foil_volumes)
        .and_then(|n| n.checked_add(count(axial_volumes)?))
        .and_then(|n| n.checked_mul(std::mem::size_of::<f64>()))
        .ok_or_else(|| error("MEMORY_LIMIT", "packed scalar kernel size overflow"))
}
