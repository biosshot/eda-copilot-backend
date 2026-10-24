//! Passive full R/L/C descriptor. Infinity-referenced C retains every potential.
use super::*;
use faer::{Mat, Side};
use std::collections::{BTreeMap, BTreeSet};
pub mod composition;
#[path = "frequency.rs"]
pub mod frequency_response;
pub mod physical;
pub mod time;

pub type Entry = (usize, usize, f64);
type Matrix = BTreeMap<(usize, usize), f64>;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Model {
    pub format: String,
    pub reference_topology: String,
    pub potential_nodes: Vec<String>,
    pub current_dofs: Vec<String>,
    pub incidence: Vec<Entry>,
    pub port_incidence: Vec<Entry>,
    pub resistance_ohm: Vec<Entry>,
    pub inductance_h: Vec<Entry>,
    pub capacitance_f: Vec<Entry>,
    #[serde(default)]
    pub conserved_potential_modes: Vec<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_terminals: Option<Vec<physical::PortTerminals>>,
}
pub struct System {
    pub(super) linear: network::Network,
    pub(super) nv: usize,
    pub(super) ni: usize,
    pub(super) b: Matrix,
    pub(super) r: Matrix,
    pub(super) candidates: Vec<Vec<f64>>,
    pub(super) memory_plan: usize,
    pub(super) residual: f64,
    pub(super) kcl: f64,
}
fn entries(input: &[Entry], rows: usize, columns: usize) -> Result<Matrix> {
    let mut out = Matrix::new();
    for &(i, j, v) in input {
        if i >= rows || j >= columns || !v.is_finite() || out.insert((i, j), v).is_some() {
            return Err(fail(
                "COUPLED_MATRIX",
                "Invalid index, duplicate or nonfinite coefficient",
            ));
        }
    }
    Ok(out)
}
fn symmetric(input: &[Entry], size: usize, positive: bool) -> Result<Matrix> {
    let out = entries(input, size, size)?;
    let scale = out.values().map(|v| v.abs()).fold(0., f64::max);
    if out
        .iter()
        .any(|(&(i, j), v)| out.get(&(j, i)).is_none_or(|w| *v != *w))
    {
        return Err(fail(
            "COUPLED_SYMMETRY",
            "Full exact symmetric entries required; no averaging or dropped mutual terms",
        ));
    }
    if size > 0 && scale > 0. {
        let dense = Mat::from_fn(size, size, |i, j| {
            out.get(&(i, j)).copied().unwrap_or(0.) / scale
        });
        let eigen = dense
            .self_adjoint_eigenvalues(Side::Lower)
            .map_err(|e| fail("COUPLED_PSD", format!("{e:?}")))?;
        if eigen.iter().any(|v| !v.is_finite() || *v < -1e-12) || positive && eigen[0] <= 1e-14 {
            return Err(fail(
                "COUPLED_PSD",
                "Nonpassive/indefinite or unresolved positive resistance matrix",
            ));
        }
    } else if positive && size > 0 {
        return Err(fail(
            "COUPLED_PSD",
            "Current-basis resistance must be positive definite",
        ));
    }
    Ok(out)
}
pub(super) fn multiply(matrix: &Matrix, x: &[f64], size: usize) -> Vec<f64> {
    let mut y = vec![0.; size];
    for (&(i, j), v) in matrix {
        y[i] += v * x[j];
    }
    y
}
fn independent(mut modes: Vec<Vec<f64>>, nv: usize) -> Result<Vec<Vec<f64>>> {
    if modes.len() > nv
        || modes
            .iter()
            .any(|v| v.len() != nv || v.iter().any(|x| !x.is_finite()))
    {
        return Err(fail(
            "COUPLED_GAUGE",
            "Invalid candidate potential-mode dimensions",
        ));
    }
    // Reorthogonalization preserves the supplied subspace. No physical matrix changes.
    let mut out: Vec<Vec<f64>> = vec![];
    for v in &mut modes {
        let original = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        for _ in 0..2 {
            for q in &out {
                let dot = v.iter().zip(q).map(|(a, b)| a * b).sum::<f64>();
                for (a, b) in v.iter_mut().zip(q) {
                    *a -= dot * b;
                }
            }
        }
        let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if !norm.is_finite() || original == 0. || norm <= 1e-12 * original {
            return Err(fail(
                "COUPLED_GAUGE",
                "Dependent/zero/unresolved candidate modes",
            ));
        }
        for a in v.iter_mut() {
            *a /= norm;
        }
        out.push(v.clone());
    }
    Ok(out)
}
impl System {
    pub fn compile(model: &Model, ports: usize, budget: Option<&Budget>) -> Result<Self> {
        let nv = model.potential_nodes.len();
        let ni = model.current_dofs.len();
        let n = nv + ni;
        let count = model
            .incidence
            .len()
            .saturating_add(model.port_incidence.len())
            .saturating_add(model.resistance_ohm.len())
            .saturating_add(model.inductance_h.len())
            .saturating_add(model.capacitance_f.len());
        let memory = budget.and_then(|b| b.max_memory_mi_b).unwrap_or(256.);
        let planned = n
            .saturating_mul(n)
            .saturating_mul(1280)
            .saturating_add(count.saturating_mul(128));
        if nv == 0
            || nv > 1024
            || ni > 1536
            || n > 2048
            || ports == 0
            || ports > 16
            || count > 4_000_000
            || !memory.is_finite()
            || memory <= 0.
            || memory > 4096.
            || planned as f64 > memory * 1048576.
        {
            return Err(fail(
                "COUPLED_BUDGET",
                format!("Descriptor plan {planned} bytes, {nv}+{ni} states/{count} coefficients"),
            ));
        }
        if model.format != "coupled-rlc"
            || !["grounded", "infinity"].contains(&model.reference_topology.as_str())
            || model
                .potential_nodes
                .iter()
                .chain(&model.current_dofs)
                .any(|s| s.is_empty())
            || model.potential_nodes.iter().collect::<BTreeSet<_>>().len() != nv
            || model.current_dofs.iter().collect::<BTreeSet<_>>().len() != ni
        {
            return Err(fail(
                "COUPLED_INPUT",
                "Invalid descriptor identity/reference topology",
            ));
        }
        physical::terminals(model, ports)?;
        let a = entries(&model.incidence, nv, ni)?;
        let b = entries(&model.port_incidence, nv, ports)?;
        for port in 0..ports {
            if !b
                .iter()
                .any(|(&(row, col), v)| row < nv && col == port && *v != 0.)
            {
                return Err(fail("COUPLED_PORT", "Empty port incidence"));
            }
        }
        if model.reference_topology == "infinity" {
            for (matrix, columns) in [(&a, ni), (&b, ports)] {
                for j in 0..columns {
                    let (mut sum, mut magnitude) = (0., 0.);
                    for (&(_, col), v) in matrix {
                        if col == j {
                            sum += v;
                            magnitude += v.abs();
                        }
                    }
                    if sum.abs() > 1e-12 * magnitude {
                        return Err(fail("COUPLED_CHARGE","Infinity-reference incidence must conserve total charge; no grounded metal inferred"));
                    }
                }
            }
        }
        let r = symmetric(&model.resistance_ohm, ni, true)?;
        let l = symmetric(&model.inductance_h, ni, false)?;
        let c = symmetric(&model.capacitance_f, nv, false)?;
        let raw = if ni == 0 && model.conserved_potential_modes.is_empty() {
            (0..nv)
                .map(|i| (0..nv).map(|j| if i == j { 1. } else { 0. }).collect())
                .collect()
        } else {
            model.conserved_potential_modes.clone()
        };
        let candidates = independent(raw, nv)?;
        for mode in &candidates {
            let mut residual = vec![0.; ni];
            let mut scale = vec![0.; ni];
            for (&(i, j), v) in &a {
                residual[j] += v * mode[i];
                scale[j] += (v * mode[i]).abs();
            }
            if residual
                .iter()
                .zip(scale)
                .any(|(r, s)| r.abs() > 1e-12 * s.max(1e-300))
            {
                return Err(fail(
                    "COUPLED_GAUGE",
                    "Candidate potential mode is not in null(A^T)",
                ));
            }
        }
        let mut g = Matrix::new();
        let mut e = c;
        for (&(i, j), v) in &a {
            g.insert((i, nv + j), *v);
            g.insert((nv + j, i), -*v);
        }
        for (&(i, j), v) in &r {
            g.insert((nv + i, nv + j), *v);
        }
        for (&(i, j), v) in &l {
            e.insert((nv + i, nv + j), *v);
        }
        Ok(Self {
            linear: network::Network::descriptor(&model.potential_nodes, n, g, e),
            nv,
            ni,
            b,
            r,
            candidates,
            memory_plan: planned,
            residual: 0.,
            kcl: 0.,
        })
    }
    /// Positive semidefinite port load / finite Thevenin driver. Call before gauge discovery.
    pub fn add_port_admittance(
        &mut self,
        port: usize,
        conductance: f64,
        capacitance: f64,
    ) -> Result<()> {
        if !conductance.is_finite()
            || conductance < 0.
            || !capacitance.is_finite()
            || capacitance < 0.
        {
            return Err(fail("COUPLED_LOAD", "Nonpassive port load"));
        }
        let weights: Vec<_> = self
            .b
            .iter()
            .filter_map(|(&(i, j), v)| (j == port).then_some((i, *v)))
            .collect();
        if weights.is_empty() {
            return Err(fail("COUPLED_PORT", "Unknown port"));
        }
        for &(i, a) in &weights {
            for &(j, b) in &weights {
                if conductance != 0. {
                    *self.linear.g.entry((i, j)).or_default() += conductance * a * b;
                }
                if capacitance != 0. {
                    *self.linear.e.entry((i, j)).or_default() += capacitance * a * b;
                }
            }
        }
        Ok(())
    }
    pub fn port_rhs(&self, values: &[f64]) -> Vec<f64> {
        multiply(&self.b, values, self.linear.n)
    }
    pub fn port_voltages(&self, x: &[f64], ports: usize) -> Vec<f64> {
        let mut v = vec![0.; ports];
        for (&(i, j), b) in &self.b {
            v[j] += b * x[i];
        }
        v
    }
    pub fn energy(&self, x: &[f64]) -> f64 {
        let y = multiply(&self.linear.e, x, self.linear.n);
        x.iter().zip(y).map(|(a, b)| a * b / 2.).sum()
    }
    pub fn joule_loss(&self, x: &[f64]) -> f64 {
        let y = multiply(&self.r, &x[self.nv..], self.ni);
        x[self.nv..].iter().zip(y).map(|(a, b)| a * b).sum()
    }

    /// Null modes belong to the complete assembled system, including port loads.
    fn null_modes(&self) -> Result<Vec<Vec<f64>>> {
        let k = self.candidates.len();
        if k == 0 {
            return Ok(vec![]);
        }
        let matrix = Mat::from_fn(k, k, |i, j| {
            self.linear
                .g
                .iter()
                .filter(|((a, b), _)| *a < self.nv && *b < self.nv)
                .map(|(&(a, b), g)| self.candidates[i][a] * g * self.candidates[j][b])
                .sum::<f64>()
        });
        let scale = (0..k)
            .flat_map(|i| (0..k).map(move |j| (i, j)))
            .map(|(i, j)| matrix[(i, j)].abs())
            .fold(0., f64::max);
        if scale == 0. {
            return Ok(self.candidates.clone());
        }
        let dense = Mat::from_fn(k, k, |i, j| matrix[(i, j)] / scale);
        let eigen = dense
            .self_adjoint_eigen(Side::Lower)
            .map_err(|e| fail("COUPLED_GAUGE", format!("{e:?}")))?;
        let mut modes = vec![];
        for j in 0..k {
            if eigen.S()[j].abs() <= 1e-12 {
                let mode = (0..self.nv)
                    .map(|r| {
                        (0..k)
                            .map(|i| self.candidates[i][r] * eigen.U()[(i, j)])
                            .sum()
                    })
                    .collect();
                modes.push(mode);
            }
        }
        Ok(modes)
    }
    fn constraints(&self, steady: bool, initial: &[f64]) -> Result<Vec<(Vec<f64>, Vec<f64>, f64)>> {
        let modes = self.null_modes()?;
        let k = modes.len();
        if k == 0 {
            return Ok(vec![]);
        }
        let weights: Vec<Vec<f64>> = modes
            .iter()
            .map(|mode| {
                let mut x = vec![0.; self.linear.n];
                x[..self.nv].copy_from_slice(mode);
                multiply(&self.linear.e, &x, self.linear.n)
            })
            .collect();
        let metric = Mat::from_fn(k, k, |i, j| {
            modes[i]
                .iter()
                .zip(&weights[j])
                .map(|(a, b)| a * b)
                .sum::<f64>()
        });
        let scale = (0..k).map(|i| metric[(i, i)].abs()).fold(0., f64::max);
        let eigen = Mat::from_fn(k, k, |i, j| {
            if scale > 0. {
                metric[(i, j)] / scale
            } else {
                0.
            }
        })
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| fail("COUPLED_GAUGE", format!("{e:?}")))?;
        let mut out = vec![];
        for col in 0..k {
            let eigenvalue = eigen.S()[col];
            if eigenvalue < -1e-12 {
                return Err(fail("COUPLED_CHARGE", "Indefinite charge metric"));
            }
            let vector: Vec<_> = (0..self.nv)
                .map(|r| {
                    (0..k)
                        .map(|i| modes[i][r] * eigen.U()[(i, col)])
                        .sum::<f64>()
                })
                .collect();
            let weight: Vec<_> = (0..self.linear.n)
                .map(|r| {
                    (0..k)
                        .map(|i| weights[i][r] * eigen.U()[(i, col)])
                        .sum::<f64>()
                })
                .collect();
            let zero = weight.iter().all(|v| *v == 0.);
            if !zero && eigenvalue <= 0. {
                return Err(fail(
                    "COUPLED_CHARGE",
                    "Unresolved physical-charge versus mathematical-gauge mode",
                ));
            }
            if steady || zero {
                let row = if zero {
                    let mut x = vec![0.; self.linear.n];
                    x[..self.nv].copy_from_slice(&vector);
                    x
                } else {
                    weight
                };
                let value = if zero {
                    0.
                } else {
                    row.iter().zip(initial).map(|(a, b)| a * b).sum()
                };
                out.push((vector, row, value));
            }
        }
        Ok(out)
    }
    pub fn solve(
        &mut self,
        omega: f64,
        rhs: Vec<f64>,
        initial: &[f64],
        stats: &mut Counters,
    ) -> Result<Vec<f64>> {
        if omega < 0.
            || !omega.is_finite()
            || rhs.len() != self.linear.n * (if omega == 0. { 1 } else { 2 })
            || initial.len() != self.linear.n
        {
            return Err(fail("COUPLED_INPUT", "Invalid solve dimensions/frequency"));
        }
        let n = self.linear.n;
        let complex = omega != 0.;
        let constraints = self.constraints(!complex, initial)?;
        if !complex {
            for mode in self.null_modes()? {
                let residual = mode.iter().zip(&rhs).map(|(a, b)| a * b).sum::<f64>();
                let scale = mode
                    .iter()
                    .zip(&rhs)
                    .map(|(a, b)| (a * b).abs())
                    .sum::<f64>();
                if residual.abs() > 1e-12 * scale.max(1e-30) {
                    return Err(fail(
                        "COUPLED_DC_INCOMPATIBLE",
                        "No steady response: input injects charge into an unanchored mode",
                    ));
                }
            }
        }
        let base = self.linear.matrix(0., complex.then_some(omega));
        let k = constraints.len();
        if k == 0 {
            let (x, res, kcl) = self.linear.solve(base, rhs, stats)?;
            self.residual = self.residual.max(res);
            self.kcl = self.kcl.max(kcl);
            return Ok(x);
        }
        let dimension = n + k;
        let mut matrix = Matrix::new();
        for (&(i, j), v) in &base {
            let row = if i < n { i } else { i - n + dimension };
            let column = if j < n { j } else { j - n + dimension };
            matrix.insert((row, column), *v);
        }
        let mut b = vec![0.; dimension * (if complex { 2 } else { 1 })];
        b[..n].copy_from_slice(&rhs[..n]);
        if complex {
            b[dimension..dimension + n].copy_from_slice(&rhs[n..]);
        }
        for (g, (mode, row, value)) in constraints.iter().enumerate() {
            for part in 0..if complex { 2 } else { 1 } {
                let offset = part * dimension;
                for i in 0..n {
                    if i < self.nv && mode[i] != 0. {
                        matrix.insert((offset + i, offset + n + g), mode[i]);
                    }
                    if row[i] != 0. {
                        matrix.insert((offset + n + g, offset + i), row[i]);
                    }
                }
                b[offset + n + g] = if complex { 0. } else { *value };
            }
        }
        let (solved, res, kcl) = self.linear.solve(matrix, b, stats)?;
        self.residual = self.residual.max(res);
        self.kcl = self.kcl.max(kcl);
        let mut x = solved[..n].to_vec();
        if complex {
            x.extend_from_slice(&solved[dimension..dimension + n]);
        }
        // Gauge multipliers must not become physical injected currents.
        let mut residual = rhs.iter().map(|v| -v).collect::<Vec<_>>();
        let mut magnitude = rhs.iter().map(|v| v.abs()).collect::<Vec<_>>();
        for (&(i, j), v) in &base {
            residual[i] += v * x[j];
            magnitude[i] += (v * x[j]).abs();
        }
        if residual
            .iter()
            .zip(magnitude)
            .any(|(r, s)| r.abs() > 1e-15 + 1e-8 * s)
        {
            return Err(fail(
                "COUPLED_KCL",
                "Gauge failed original physical equations",
            ));
        }
        Ok(x)
    }
}

pub fn impedance(
    model: &Model,
    ports: usize,
    frequencies: &[f64],
    budget: Option<&Budget>,
) -> Result<Value> {
    if ports == 0
        || ports > 16
        || frequencies.is_empty()
        || frequencies.len() > 131072 / ports / ports
        || frequencies
            .iter()
            .enumerate()
            .any(|(i, f)| !f.is_finite() || *f < 0. || i > 0 && *f <= frequencies[i - 1])
    {
        return Err(fail("COUPLED_INPUT", "Invalid or oversized frequency axis"));
    }
    let started = Instant::now();
    let mut system = System::compile(model, ports, budget)?;
    let output = frequencies.len() * ports * ports * 64;
    if (system.memory_plan + output) as f64
        > budget.and_then(|b| b.max_memory_mi_b).unwrap_or(256.) * 1048576.
    {
        return Err(fail(
            "COUPLED_BUDGET",
            "Descriptor output exceeds admitted request memory",
        ));
    }
    let mut stats = Counters::default();
    let mut samples = vec![];
    let n = system.linear.n;
    let mut power_error: f64 = 0.;
    for f in frequencies {
        if started.elapsed().as_secs_f64() > budget.and_then(|b| b.max_seconds).unwrap_or(120.) {
            return Err(fail("NETWORK_DEADLINE", "Coupled frequency deadline"));
        }
        let mut z = vec![vec![json!({"real":0.,"imaginary":0.}); ports]; ports];
        for input in 0..ports {
            let mut excitation = vec![0.; ports];
            excitation[input] = 1.;
            let mut rhs = system.port_rhs(&excitation);
            if *f != 0. {
                rhs.resize(2 * n, 0.);
            }
            let x = system.solve(2. * std::f64::consts::PI * f, rhs, &vec![0.; n], &mut stats)?;
            let re = system.port_voltages(&x, ports);
            let im = if *f == 0. {
                vec![0.; ports]
            } else {
                system.port_voltages(&x[n..], ports)
            };
            let loss = system.joule_loss(&x[..n])
                + if *f == 0. {
                    0.
                } else {
                    system.joule_loss(&x[n..])
                };
            let error = (re[input] - loss).abs();
            power_error = power_error.max(error / (1e-12 + re[input].abs() + loss.abs()));
            if error > 1e-12 + 1e-8 * (re[input].abs() + loss.abs()) {
                return Err(fail(
                    "COUPLED_POWER",
                    "Port real power differs from full R quadratic dissipation",
                ));
            }
            for output in 0..ports {
                z[output][input] = json!({"real":re[output],"imaginary":im[output]});
            }
        }
        samples.push(z);
    }
    Ok(
        json!({"status":"complete","representation":"Z","frequenciesHz":frequencies,"parameters":samples,"counters":stats,"memoryPlanBytes":system.memory_plan+output,"elapsedMs":started.elapsed().as_secs_f64()*1000.,"referenceTopology":model.reference_topology,"quality":{"relativeResidual":system.residual,"kclAbsA":system.kcl,"relativeRealPowerError":power_error},"assumptions":["Full symmetric physical R/L/C; no hidden metal grounding or discarded mutual entries","Infinity common-mode charge differs from a mathematical zero-energy gauge"]}),
    )
}
