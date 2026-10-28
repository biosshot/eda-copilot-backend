//! Internal full-cell matrix materialization. This never accepts a port RLC.
use super::{
    accurate::Sum, error, mesh::LayeredMesh, volume_current::Space, volume_magnetic::MagneticApply,
    AllocationBudget, Result,
};
use std::{
    io::{BufRead, BufReader},
    path::Path,
};
#[derive(serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Signature {
    pub kind: String,
    pub degrees: Vec<usize>,
    pub node_bits: Vec<[u64; 3]>,
    pub copper_cells: Vec<[usize; 4]>,
    pub dielectric_cells: Vec<[usize; 4]>,
    pub density_bits: Vec<u64>,
    pub start: usize,
    pub count: usize,
}
impl Signature {
    pub fn new(mesh: &LayeredMesh, space: &Space, degrees: Vec<usize>) -> Self {
        Self {
            kind: "full-scalar-source-columns-v0".into(),
            degrees,
            node_bits: mesh.nodes_m.iter().map(|p| p.map(f64::to_bits)).collect(),
            copper_cells: mesh.copper_tetrahedra.iter().map(|t| t.nodes).collect(),
            dielectric_cells: mesh.tetrahedra.iter().map(|t| t.nodes).collect(),
            density_bits: space
                .cells
                .iter()
                .map(|c| c.density_per_amp_m2.to_bits())
                .collect(),
            start: 0,
            count: space.cells.len(),
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Column {
    column: usize,
    values: Vec<f64>,
    seconds: f64,
}
pub struct Operator {
    values: Vec<f64>,
    n: usize,
    pub max_directed_reciprocity: f64,
    pub planned_bytes: usize,
    densities: Vec<u64>,
}
impl Operator {
    pub fn load(
        mesh: &LayeredMesh,
        space: &Space,
        degrees: Vec<usize>,
        paths: &[impl AsRef<Path>],
        budget: AllocationBudget,
    ) -> Result<Self> {
        let n = space.cells.len();
        let plan = n
            .checked_mul(n)
            .and_then(|v| v.checked_mul(32))
            .and_then(|v| v.checked_add(mesh.retained_bytes().ok()?.checked_mul(8)?))
            .and_then(|v| v.checked_add(n.checked_mul(1024)?))
            .ok_or_else(|| error("MEMORY_LIMIT", "complete scalar matrix plan overflow"))?;
        budget.check(plan)?;
        let expected = Signature::new(mesh, space, degrees);
        let mut values = vec![0.; n * n];
        let mut seen = vec![false; n];
        for path in paths {
            let mut lines = BufReader::new(
                std::fs::File::open(path).map_err(|e| error("MAGNETIC_CACHE", &e.to_string()))?,
            )
            .lines();
            let mut header: Signature = serde_json::from_str(
                &lines
                    .next()
                    .ok_or_else(|| error("MAGNETIC_CACHE", "empty packet"))?
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| error("MAGNETIC_CACHE", &e.to_string()))?;
            let start = header.start;
            let count = header.count;
            header.start = 0;
            header.count = n;
            if header != expected
                || start >= n
                || count == 0
                || start.checked_add(count).is_none_or(|v| v > n)
            {
                return Err(error(
                    "MAGNETIC_CACHE",
                    "source mesh/normalization/method identity mismatch",
                ));
            }
            for line in lines {
                let c: Column = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                    .map_err(|e| error("MAGNETIC_CACHE", &e.to_string()))?;
                if c.column < start
                    || c.column >= start + count
                    || c.values.len() != n
                    || seen[c.column]
                    || c.values.iter().any(|v| !v.is_finite())
                    || !c.seconds.is_finite()
                    || c.seconds < 0.
                {
                    return Err(error(
                        "MAGNETIC_CACHE",
                        "invalid or duplicate source column",
                    ));
                }
                for i in 0..n {
                    values[i * n + c.column] = c.values[i];
                }
                seen[c.column] = true;
            }
        }
        if seen.iter().any(|v| !*v) {
            return Err(error(
                "MAGNETIC_CACHE",
                "incomplete full scalar source matrix",
            ));
        }
        let mut reciprocity: f64 = 0.;
        for i in 0..n {
            if values[i * n + i] <= 0. {
                return Err(error("MAGNETIC_CACHE", "nonpositive source self energy"));
            }
            for j in 0..i {
                let scale = values[i * n + i].sqrt() * values[j * n + j].sqrt();
                reciprocity =
                    reciprocity.max((values[i * n + j] - values[j * n + i]).abs() / scale);
            }
        }
        if !reciprocity.is_finite() || reciprocity > 1e-9 {
            return Err(error(
                "MAGNETIC_RECIPROCITY",
                "complete directed source matrix refuses1e-9",
            ));
        }
        // Standard single-triangle symmetric assembly after directed validation.
        // No averaging, eigenvalue clipping, diagonal shifts or removed source modes.
        for i in 0..n {
            for j in 0..i {
                values[j * n + i] = values[i * n + j];
            }
        }
        let mut lower = vec![0.; n * n];
        for i in 0..n {
            for j in 0..=i {
                let scale = values[i * n + i].sqrt() * values[j * n + j].sqrt();
                let mut v = Sum::from(values[i * n + j] / scale);
                for k in 0..j {
                    v = v.add(Sum::from(lower[i * n + k]).mul(-lower[j * n + k]));
                }
                let v = v.value();
                if i == j {
                    if !v.is_finite() || v <= 0. {
                        return Err(error(
                            "MAGNETIC_MATRIX",
                            "complete source matrix not SPD; no regularization",
                        ));
                    }
                    lower[i * n + j] = v.sqrt();
                } else {
                    lower[i * n + j] = v / lower[j * n + j];
                }
            }
        }
        Ok(Self {
            values,
            n,
            max_directed_reciprocity: reciprocity,
            planned_bytes: plan,
            densities: expected.density_bits,
        })
    }
}
impl MagneticApply for Operator {
    fn apply(&self, space: &Space, current: &[f64]) -> Result<Vec<f64>> {
        if space.cells.len() != self.n
            || space
                .cells
                .iter()
                .zip(&self.densities)
                .any(|(c, &v)| c.density_per_amp_m2.to_bits() != v)
            || current.len() != 3 * self.n
            || current.iter().any(|v| !v.is_finite())
        {
            return Err(error(
                "MAGNETIC_INPUT",
                "complete cache source identity/finite full vector required",
            ));
        }
        let mut out = vec![0.; current.len()];
        for i in 0..self.n {
            for a in 0..3 {
                let mut sum = Sum::default();
                for j in 0..self.n {
                    sum = sum.add(Sum::from(current[3 * j + a]).mul(self.values[i * self.n + j]));
                }
                out[3 * i + a] = sum.value();
            }
        }
        Ok(out)
    }
    fn planned_bytes(&self) -> usize {
        self.planned_bytes
    }
}
