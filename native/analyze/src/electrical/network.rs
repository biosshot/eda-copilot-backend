use super::*;
use faer::{
    prelude::Solve,
    sparse::{
        linalg::solvers::{Lu, SymbolicLu},
        SparseColMat, Triplet,
    },
    Mat,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Circuit {
    pub nodes: Vec<String>,
    pub reference: String,
    pub ports: Vec<Port>,
    pub elements: Vec<Element>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub id: String,
    pub positive: String,
    pub negative: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Element {
    Resistor {
        id: String,
        nodes: [String; 2],
        #[serde(rename = "resistanceOhm")]
        r: f64,
    },
    Capacitor {
        id: String,
        nodes: [String; 2],
        #[serde(rename = "capacitanceF")]
        c: f64,
        #[serde(rename = "esrOhm")]
        r: f64,
        #[serde(rename = "eslH")]
        l: f64,
    },
    Inductor {
        id: String,
        nodes: [String; 2],
        #[serde(rename = "inductanceH")]
        l: f64,
        #[serde(rename = "seriesResistanceOhm")]
        r: f64,
    },
    VoltageSource {
        id: String,
        nodes: [String; 2],
        #[serde(rename = "voltageV")]
        value: f64,
        waveform: Option<Waveform>,
    },
    CurrentSource {
        id: String,
        nodes: [String; 2],
        #[serde(rename = "currentA")]
        value: f64,
        waveform: Option<Waveform>,
    },
}
impl Element {
    pub fn id(&self) -> &str {
        match self {
            Self::Resistor { id, .. }
            | Self::Capacitor { id, .. }
            | Self::Inductor { id, .. }
            | Self::VoltageSource { id, .. }
            | Self::CurrentSource { id, .. } => id,
        }
    }
    pub fn nodes(&self) -> &[String; 2] {
        match self {
            Self::Resistor { nodes, .. }
            | Self::Capacitor { nodes, .. }
            | Self::Inductor { nodes, .. }
            | Self::VoltageSource { nodes, .. }
            | Self::CurrentSource { nodes, .. } => nodes,
        }
    }
    pub fn source(&self, t: Option<f64>) -> f64 {
        match self {
            Self::VoltageSource {
                value, waveform, ..
            }
            | Self::CurrentSource {
                value, waveform, ..
            } => t
                .and_then(|t| waveform.as_ref().map(|w| w.sample(t)))
                .unwrap_or(*value),
            _ => 0.,
        }
    }
    pub fn waveform(&self) -> Option<&Waveform> {
        match self {
            Self::VoltageSource { waveform, .. } | Self::CurrentSource { waveform, .. } => {
                waveform.as_ref()
            }
            _ => None,
        }
    }
}
pub struct Network {
    pub identity: String,
    pub matrix_identity: String,
    pub final_state: Option<Value>,
    pub circuit: Circuit,
    pub nodes: BTreeMap<String, Option<usize>>,
    pub n: usize,
    pub nv: usize,
    pub branch: Vec<Option<usize>>,
    pub cap: Vec<Option<usize>>,
    pub g: BTreeMap<(usize, usize), f64>,
    pub e: BTreeMap<(usize, usize), f64>,
    symbolic: Option<(usize, Vec<(usize, usize)>, SymbolicLu<usize>)>,
    factor: Option<(Vec<(usize, usize, u64)>, Lu<usize, f64>)>,
}
fn stamp(m: &mut BTreeMap<(usize, usize), f64>, i: Option<usize>, j: Option<usize>, v: f64) {
    if let (Some(i), Some(j)) = (i, j) {
        *m.entry((i, j)).or_default() += v;
    }
}
impl Network {
    pub fn compile(c: &Circuit, identity: String, r: &Request) -> Result<Self> {
        if c.nodes.len() > 1024 || c.elements.len() > 2048 || c.ports.len() > 128 {
            return Err(fail(
                "NETWORK_BUDGET",
                "Network size limit (1024 nodes, 2048 elements, 128 ports)",
            ));
        }
        if c.nodes.is_empty() || c.elements.is_empty() || !c.nodes.contains(&c.reference) {
            return Err(fail(
                "NETWORK_INPUT",
                "Explicit reference and elements required",
            ));
        }
        let unique = |v: Vec<&str>| {
            v.iter().all(|s| !s.is_empty()) && v.iter().collect::<BTreeSet<_>>().len() == v.len()
        };
        if !unique(c.nodes.iter().map(String::as_str).collect())
            || !unique(c.elements.iter().map(Element::id).collect())
            || !unique(c.ports.iter().map(|p| p.id.as_str()).collect())
        {
            return Err(fail("NETWORK_INPUT", "Duplicate/empty identifiers"));
        }
        let mut nodes = BTreeMap::new();
        let mut n = 0;
        for id in &c.nodes {
            nodes.insert(
                id.clone(),
                if id == &c.reference {
                    None
                } else {
                    let i = n;
                    n += 1;
                    Some(i)
                },
            );
        }
        for pair in c
            .elements
            .iter()
            .map(Element::nodes)
            .map(|p| [&p[0], &p[1]])
            .chain(c.ports.iter().map(|p| [&p.positive, &p.negative]))
        {
            if pair.iter().any(|s| !nodes.contains_key(*s)) || pair[0] == pair[1] {
                return Err(fail(
                    "NETWORK_INPUT",
                    "Unknown node or identical terminal pair",
                ));
            }
        }
        let nv = n;
        let mut branch = vec![None; c.elements.len()];
        let mut cap = branch.clone();
        for (i, x) in c.elements.iter().enumerate() {
            let positive = |v: f64| v.is_finite() && v > 0.;
            let nonnegative = |v: f64| v.is_finite() && v >= 0.;
            let valid = match x {
                Element::Resistor { r, .. } => positive(*r),
                Element::Capacitor { c, r, l, .. } => {
                    positive(*c) && nonnegative(*r) && nonnegative(*l)
                }
                Element::Inductor { l, r, .. } => positive(*l) && nonnegative(*r),
                Element::VoltageSource { value, .. } | Element::CurrentSource { value, .. } => {
                    value.is_finite()
                }
            };
            if !valid {
                return Err(fail(
                    "NETWORK_INPUT",
                    format!("Invalid coefficients: {}", x.id()),
                ));
            }
            if let Some(w) = x.waveform() {
                w.validate()?;
            }
            if !matches!(x, Element::Resistor { .. } | Element::CurrentSource { .. }) {
                branch[i] = Some(n);
                n += 1;
            }
            if matches!(x, Element::Capacitor { .. }) {
                cap[i] = Some(n);
                n += 1;
            }
        }
        let mut out = Self {
            identity,
            matrix_identity: String::new(),
            final_state: None,
            circuit: c.clone(),
            nodes,
            n,
            nv,
            branch,
            cap,
            g: BTreeMap::new(),
            e: BTreeMap::new(),
            symbolic: None,
            factor: None,
        };
        out.admit(r)?;
        for (i, x) in c.elements.iter().enumerate() {
            let pair = x.nodes();
            let [a, b] = [out.nodes[&pair[0]], out.nodes[&pair[1]]];
            let k = out.branch[i];
            if let Element::Resistor { r, .. } = x {
                for (u, v, s) in [(a, a, 1.), (b, b, 1.), (a, b, -1.), (b, a, -1.)] {
                    stamp(&mut out.g, u, v, s / r);
                }
            }
            if k.is_some() {
                for (u, s) in [(a, 1.), (b, -1.)] {
                    stamp(&mut out.g, u, k, s);
                    stamp(&mut out.g, k, u, s);
                }
            }
            match x {
                Element::Inductor { r, l, .. } => {
                    stamp(&mut out.g, k, k, -r);
                    stamp(&mut out.e, k, k, -l);
                }
                Element::Capacitor { c, r, l, .. } => {
                    stamp(&mut out.g, k, k, -r);
                    stamp(&mut out.e, k, k, -l);
                    stamp(&mut out.g, k, out.cap[i], -1.);
                    stamp(&mut out.g, out.cap[i], k, 1.);
                    stamp(&mut out.e, out.cap[i], out.cap[i], -c);
                }
                _ => {}
            }
        }
        Ok(out)
    }
    pub fn admit(&self, r: &Request) -> Result<()> {
        let memory = r
            .budget
            .as_ref()
            .and_then(|b| b.max_memory_mi_b)
            .unwrap_or(256.);
        // Conservative upper bound for sparse fill, real-block AC, solver workspace and output.
        let points = if r.mode == "frequency" {
            r.frequencies
                .len()
                .saturating_mul(self.circuit.ports.len().max(1))
        } else if r.mode == "time" {
            let start = solve::initial_time(self, r)?;
            solve::output_samples(
                r.time
                    .as_ref()
                    .ok_or_else(|| fail("NETWORK_INPUT", "time required"))?,
                start,
            )?
        } else {
            1
        };
        let bytes = (self.n as f64 * 2.).powi(2) * 64.
            + points as f64 * (self.n + self.circuit.elements.len() + 8) as f64 * 32.;
        if !memory.is_finite()
            || memory <= 0.
            || memory > 4096.
            || !bytes.is_finite()
            || bytes > memory * 1048576.
            || points > 100_000
        {
            return Err(fail(
                "NETWORK_BUDGET",
                format!("Planned {bytes} bytes/{points} samples exceeds limit"),
            ));
        }
        Ok(())
    }
    pub fn v(&self, x: &[f64], node: &str) -> f64 {
        self.nodes[node].map(|i| x[i]).unwrap_or(0.)
    }
    pub fn rhs(&self, t: Option<f64>) -> Vec<f64> {
        let mut b = vec![0.; self.n];
        for (i, e) in self.circuit.elements.iter().enumerate() {
            let v = e.source(t);
            match e {
                Element::VoltageSource { .. } => b[self.branch[i].unwrap()] = v,
                Element::CurrentSource { nodes, .. } => {
                    if let Some(a) = self.nodes[&nodes[0]] {
                        b[a] -= v;
                    }
                    if let Some(a) = self.nodes[&nodes[1]] {
                        b[a] += v;
                    }
                }
                _ => {}
            }
        }
        b
    }
    pub fn currents(&self, x: &[f64], t: Option<f64>) -> Vec<f64> {
        self.circuit
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if let Some(k) = self.branch[i] {
                    x[k]
                } else {
                    match e {
                        Element::Resistor { nodes, r, .. } => {
                            (self.v(x, &nodes[0]) - self.v(x, &nodes[1])) / r
                        }
                        _ => e.source(t),
                    }
                }
            })
            .collect()
    }
    pub fn energy_loss(&self, x: &[f64], t: Option<f64>) -> (f64, f64, f64) {
        let currents = self.currents(x, t);
        let (mut energy, mut loss, mut power) = (0., 0., 0.);
        for (i, e) in self.circuit.elements.iter().enumerate() {
            let v = self.v(x, &e.nodes()[0]) - self.v(x, &e.nodes()[1]);
            let current = currents[i];
            match e {
                Element::Resistor { r, .. } => loss += r * current * current,
                Element::Inductor { r, l, .. } => {
                    loss += r * current * current;
                    energy += l * current * current / 2.;
                }
                Element::Capacitor { c, r, l, .. } => {
                    loss += r * current * current;
                    energy += c * x[self.cap[i].unwrap()].powi(2) / 2. + l * current * current / 2.;
                }
                _ => power -= v * current,
            }
        }
        (energy, loss, power)
    }
    pub fn matrix(&self, alpha: f64, omega: Option<f64>) -> BTreeMap<(usize, usize), f64> {
        let mut m = self.g.clone();
        for (&ij, &v) in &self.e {
            *m.entry(ij).or_default() += alpha * v;
        }
        if let Some(w) = omega {
            let base = m.clone();
            for (&(i, j), &v) in &base {
                m.insert((i + self.n, j + self.n), v);
            }
            for (&(i, j), &v) in &self.e {
                *m.entry((i, j + self.n)).or_default() -= w * v;
                *m.entry((i + self.n, j)).or_default() += w * v;
            }
        }
        m
    }
    pub fn solve(
        &mut self,
        m: BTreeMap<(usize, usize), f64>,
        b: Vec<f64>,
        stats: &mut Counters,
    ) -> Result<(Vec<f64>, f64, f64)> {
        if b.is_empty() || b.iter().chain(m.values()).any(|x| !x.is_finite()) {
            return Err(fail("NETWORK_NONFINITE", "Matrix/RHS"));
        }
        let key = m
            .iter()
            .map(|(&(i, j), &v)| (i, j, v.to_bits()))
            .collect::<Vec<_>>();
        if self.factor.as_ref().is_none_or(|f| f.0 != key) {
            let a = SparseColMat::try_new_from_triplets(
                b.len(),
                b.len(),
                &m.iter()
                    .map(|(&(i, j), &v)| Triplet::new(i, j, v))
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| fail("NETWORK_MATRIX", e))?;
            let pattern = m.keys().copied().collect::<Vec<_>>();
            if self
                .symbolic
                .as_ref()
                .is_none_or(|s| s.0 != b.len() || s.1 != pattern)
            {
                self.symbolic = Some((
                    b.len(),
                    pattern,
                    SymbolicLu::try_new(a.symbolic()).map_err(|e| fail("NETWORK_SINGULAR", e))?,
                ));
                stats.symbolic_factorizations += 1;
            }
            self.factor = Some((
                key,
                Lu::try_new_with_symbolic(self.symbolic.as_ref().unwrap().2.clone(), a.as_ref())
                    .map_err(|e| fail("NETWORK_SINGULAR", e))?,
            ));
            stats.numerical_factorizations += 1;
        } else {
            stats.reused_factors += 1;
        }
        let rhs = Mat::from_fn(b.len(), 1, |i, _| b[i]);
        let f = &self.factor.as_ref().unwrap().1;
        let mut x = f.solve(&rhs);
        stats.solves += 1;
        for _ in 0..2 {
            let mut residual = b.clone();
            for (&(i, j), &v) in &m {
                residual[i] -= v * x[(j, 0)];
            }
            let delta = f.solve(Mat::from_fn(b.len(), 1, |i, _| residual[i]));
            for i in 0..b.len() {
                x[(i, 0)] += delta[(i, 0)];
            }
        }
        let x = (0..b.len()).map(|i| x[(i, 0)]).collect::<Vec<_>>();
        if x.iter().any(|v| !v.is_finite()) {
            return Err(fail(
                "NETWORK_SINGULAR",
                "Nonunique/inconsistent ideal constraints or floating nodes",
            ));
        }
        let mut residual = b.iter().map(|v| -v).collect::<Vec<_>>();
        let mut scale = b.iter().map(|v| v.abs()).collect::<Vec<_>>();
        for (&(i, j), &v) in &m {
            residual[i] += v * x[j];
            scale[i] += (v * x[j]).abs();
        }
        let normalized = residual
            .iter()
            .zip(&scale)
            .map(|(v, s)| v.abs() / s.max(1e-15))
            .fold(0., f64::max);
        let kcl = residual[..self.nv]
            .iter()
            .chain(if b.len() == 2 * self.n {
                &residual[self.n..self.n + self.nv]
            } else {
                &[]
            })
            .map(|v| v.abs())
            .fold(0., f64::max);
        if normalized > 1e-8 {
            return Err(fail("NETWORK_RESIDUAL", normalized));
        }
        Ok((x, normalized, kcl))
    }
}
