//! Experimental compact-trace adapter to the existing layered field solve.
//! The dielectric field mesh remains explicit and budgeted; no production admission.
use super::{error, Result, layered_capacitance::{self, Input, PotentialTrace, Budget}};

/// Assemble a capacitance matrix on the supplied potential coordinates.
/// The field solver validates geometry, trace coverage, gauge and energy balance.
/// This does not establish mesh convergence or whole-board applicability.
pub(super) fn capacitance(input: &Input, trace: &PotentialTrace, budget: Budget) -> Result<Vec<Vec<f64>>> {
    let n = trace.potential_nodes.len();
    if n == 0 || n > budget.max_nodes { return Err(error("PLANAR_TRACE", "invalid compact potential coordinates")); }
    let bytes = n.checked_mul(n).and_then(|v| v.checked_mul(8)).ok_or_else(|| error("MEMORY_LIMIT", "trace matrix size overflow"))?;
    if bytes > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace matrix exceeds budget")); }
    let solution = layered_capacitance::extract_traced(input, trace, budget)?;
    let matrix = solution.energy_capacitance_f()?;
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n || row.iter().any(|x| !x.is_finite())) {
        return Err(error("PLANAR_TRACE", "invalid layered capacitance matrix"));
    }
    Ok(matrix)
}
