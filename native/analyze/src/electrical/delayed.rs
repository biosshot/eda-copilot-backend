//! Exact causal constant-scattering delay resources. No Pade/FFT reconstruction.
use super::*;
use faer::{Mat, Side};
use frequency::math::{Matrix, C};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Data {
    format: String,
    reference_topology: String,
    reference_ohm: Value,
    scattering: Vec<Vec<f64>>,
    delay_s: f64,
    nature: String,
}
pub(super) struct Model {
    pub refs: Vec<f64>,
    pub k: Vec<Vec<f64>>,
    pub delay: f64,
}
pub(super) fn model(resource: &frequency::Resource) -> Result<Model> {
    let n = resource.ports.len();
    if n == 0 || n > 16 {
        return Err(fail("DELAY_BUDGET", "1..16 ports required"));
    }
    let data: Data =
        serde_json::from_value(resource.data.clone()).map_err(|e| fail("DELAY_INPUT", e))?;
    if data.format != "delayed-scattering"
        || data.reference_topology != "common"
        || data.nature != "passive"
        || !data.delay_s.is_finite()
        || data.delay_s < 0.
        || data.scattering.len() != n
        || data
            .scattering
            .iter()
            .any(|r| r.len() != n || r.iter().any(|v| !v.is_finite()))
    {
        return Err(fail(
            "DELAY_INPUT",
            "Finite real K, common return, nonnegative delay and passive nature required",
        ));
    }
    let refs = frequency::refs(&data.reference_ohm, &resource.ports)?;
    let gram = Mat::from_fn(n, n, |i, j| {
        (0..n)
            .map(|r| data.scattering[r][i] * data.scattering[r][j])
            .sum::<f64>()
    });
    let eigen = gram
        .self_adjoint_eigenvalues(Side::Lower)
        .map_err(|e| fail("DELAY_PASSIVITY", format!("{e:?}")))?;
    if eigen
        .iter()
        .any(|v| !v.is_finite() || *v > (1_f64 + 1e-9).powi(2))
    {
        return Err(fail(
            "DELAY_PASSIVITY",
            "K is not a contraction; coefficients retained without repair",
        ));
    }
    Ok(Model {
        refs,
        k: data.scattering,
        delay: data.delay_s,
    })
}
pub(super) fn sample(model: &Model, f: f64) -> Result<Matrix> {
    let phase = -2. * std::f64::consts::PI * f * model.delay;
    if !phase.is_finite() || phase.abs() > 1e14 {
        return Err(fail(
            "DELAY_PHASE_RESOLUTION",
            "Delay phase is unresolved in F64",
        ));
    }
    Ok(model
        .k
        .iter()
        .map(|r| {
            r.iter()
                .map(|v| C::new(phase.cos() * v, phase.sin() * v))
                .collect()
        })
        .collect())
}
fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
fn merge(parent: &mut [usize], a: usize, b: usize) {
    let a = root(parent, a);
    let b = root(parent, b);
    parent[b] = a;
}
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
struct History {
    times: Vec<f64>,
    a: Vec<Vec<f64>>,
    integrals: Vec<Vec<f64>>,
    initial: Vec<f64>,
}
impl History {
    fn append(&mut self, t: f64, a: Vec<f64>) -> Result<()> {
        if self
            .times
            .len()
            .saturating_add(1)
            .saturating_mul(2 * a.len() + 1)
            > 2_000_000
        {
            return Err(fail(
                "DELAY_HISTORY_BUDGET",
                "Bounded accepted history exhausted; incomplete result",
            ));
        }
        let dt = t - self.times.last().unwrap();
        if dt <= 0. {
            return Err(fail("DELAY_HISTORY", "Nonmonotone history"));
        }
        let old = self.a.last().unwrap();
        let prev = self.integrals.last().unwrap();
        let integral = a
            .iter()
            .zip(old)
            .zip(prev)
            .map(|((b, a), q)| q + dt * (a * a + a * b + b * b) / 3.)
            .collect();
        self.times.push(t);
        self.a.push(a);
        self.integrals.push(integral);
        Ok(())
    }
    fn at(&self, t: f64) -> Result<Vec<f64>> {
        if t <= 0. {
            return Ok(self.initial.clone());
        }
        if t > *self.times.last().unwrap() + 32. * f64::EPSILON * t.abs() {
            return Err(fail(
                "DELAY_HISTORY",
                format!(
                    "Future history {t:e} exceeds accepted {:e}; step must not exceed delay",
                    self.times.last().unwrap()
                ),
            ));
        }
        let hi = self
            .times
            .partition_point(|v| *v < t)
            .min(self.times.len() - 1);
        if hi == 0 || self.times[hi] == t {
            return Ok(self.a[hi].clone());
        }
        let lo = hi - 1;
        let fraction = ((t - self.times[lo]) / (self.times[hi] - self.times[lo])).min(1.);
        Ok(self.a[lo]
            .iter()
            .zip(&self.a[hi])
            .map(|(a, b)| a + fraction * (b - a))
            .collect())
    }
    fn integral(&self, t: f64) -> Result<Vec<f64>> {
        if t <= 0. {
            return Ok(self.initial.iter().map(|a| t * a * a).collect());
        }
        let hi = self
            .times
            .partition_point(|v| *v < t)
            .min(self.times.len() - 1);
        if self.times[hi] == t {
            return Ok(self.integrals[hi].clone());
        }
        let lo = hi.saturating_sub(1);
        let a = &self.a[lo];
        let b = self.at(t)?;
        let dt = t - self.times[lo];
        Ok(a.iter()
            .zip(b)
            .zip(&self.integrals[lo])
            .map(|((a, b), q)| q + dt * (a * a + a * b + b * b) / 3.)
            .collect())
    }
}
struct Block {
    offset: usize,
    model: Model,
}
fn reflected(blocks: &[Block], history: &History, t: f64, p: usize) -> Result<Vec<f64>> {
    let mut b = vec![0.; p];
    for block in blocks {
        if block.model.delay > 0. {
            let a = history.at(t - block.model.delay)?;
            let n = block.model.refs.len();
            for i in 0..n {
                b[block.offset + i] = dot(&block.model.k[i], &a[block.offset..block.offset + n]);
            }
        }
    }
    Ok(b)
}
fn source(
    drivers: &[coupled::time::Driver],
    output_nodes: &[usize],
    nodes: usize,
    t: f64,
) -> Vec<f64> {
    coupled::time::terminal_source(drivers, output_nodes, nodes, t)
}
fn rhs(
    blocks: &[Block],
    history: &History,
    drivers: &[coupled::time::Driver],
    output_nodes: &[usize],
    port_nodes: &[usize],
    refs: &[f64],
    nodes: usize,
    t: f64,
) -> Result<Vec<f64>> {
    let p = refs.len();
    let reflected = reflected(blocks, history, t, p)?;
    let mut b = source(drivers, output_nodes, nodes, t);
    b.resize(nodes + p, 0.);
    for i in 0..p {
        b[port_nodes[i]] += reflected[i] / refs[i].sqrt();
        b[nodes + i] = -reflected[i];
    }
    Ok(b)
}
fn storage(blocks: &[Block], history: &History, t: f64) -> Result<f64> {
    let mut energy = 0.;
    let now = history.integral(t)?;
    for block in blocks {
        if block.model.delay > 0. {
            let old = history.integral(t - block.model.delay)?;
            for i in block.offset..block.offset + block.model.refs.len() {
                energy += now[i] - old[i];
            }
        }
    }
    Ok(energy)
}
/// Exact integral of the contraction loss of a piecewise-linear history.
/// Splitting at every accepted knot retains cross terms in K^T K.
fn attenuation(block: &Block, history: &History, from: f64, to: f64) -> Result<f64> {
    let mut at = from;
    let mut total = 0.;
    while at < to {
        let next = history.times.partition_point(|t| *t <= at);
        let end = history.times.get(next).copied().unwrap_or(to).min(to);
        let a = history.at(at)?;
        let b = history.at(end)?;
        let lo = block.offset;
        let hi = lo + block.model.refs.len();
        let (a, b) = (&a[lo..hi], &b[lo..hi]);
        let ka: Vec<_> = block.model.k.iter().map(|r| dot(r, a)).collect();
        let kb: Vec<_> = block.model.k.iter().map(|r| dot(r, b)).collect();
        total += (end - at) / 3.
            * (dot(a, a) + dot(a, b) + dot(b, b) - dot(&ka, &ka) - dot(&ka, &kb) - dot(&kb, &kb));
        at = end;
    }
    Ok(total)
}
/// Only mathematical potential gauges of the complete assembled steady network
/// can be fixed. Any nonzero storage projection remains a physical initial-state
/// ambiguity and is rejected. Wave-connected potentials cannot enter this kernel.
fn steady_gauges(
    g: &BTreeMap<(usize, usize), f64>,
    e: &BTreeMap<(usize, usize), f64>,
    nodes: usize,
    n: usize,
) -> Result<Vec<Vec<f64>>> {
    let mut rows = vec![vec![]; n];
    let mut scales = vec![0_f64; n];
    for (&(i, j), value) in g {
        scales[i] = scales[i].max(value.abs());
        if j < nodes && *value != 0. {
            rows[i].push((j, *value));
        }
    }
    let mut gram = Mat::<f64>::zeros(nodes, nodes);
    for (i, row) in rows.iter().enumerate() {
        for &(a, x) in row {
            for &(b, y) in row {
                gram[(a, b)] += (x / scales[i]) * (y / scales[i]);
            }
        }
    }
    let eigen = gram
        .self_adjoint_eigen(Side::Lower)
        .map_err(|error| fail("DELAY_INITIAL", format!("Gauge rank: {error:?}")))?;
    let mut gauges = vec![];
    for col in 0..nodes {
        if eigen.S()[col].abs() > 1e-12 {
            continue;
        }
        let q: Vec<_> = (0..nodes).map(|i| eigen.U()[(i, col)]).collect();
        let mut residual = vec![0.; n];
        let mut magnitude = vec![0.; n];
        for (&(i, j), value) in g {
            if j < nodes {
                residual[i] += value * q[j];
                magnitude[i] += (value * q[j]).abs();
            }
        }
        if residual
            .iter()
            .zip(magnitude)
            .any(|(r, s)| r.abs() > 1e-12 * s)
        {
            return Err(fail(
                "DELAY_INITIAL",
                "Unresolved steady rank; no physical mode removed",
            ));
        }
        let mut charge = vec![0.; n];
        for (&(i, j), value) in e {
            if j < nodes {
                charge[i] += value * q[j];
            }
        }
        if charge.iter().any(|v| *v != 0.) {
            return Err(fail(
                "DELAY_INITIAL",
                "Nonunique physical steady charge; explicit incoming state/history required",
            ));
        }
        gauges.push(q);
    }
    Ok(gauges)
}
fn constrain(
    matrix: &mut BTreeMap<(usize, usize), f64>,
    rhs: &mut Vec<f64>,
    gauges: &[Vec<f64>],
    n: usize,
) {
    rhs.resize(n + gauges.len(), 0.);
    for (k, q) in gauges.iter().enumerate() {
        for (i, value) in q.iter().enumerate() {
            if *value != 0. {
                matrix.insert((i, n + k), *value);
                matrix.insert((n + k, i), *value);
            }
        }
    }
}
fn add_event(events: &mut BTreeSet<u64>, at: f64, stop: f64) -> Result<()> {
    if at > stop {
        return Ok(());
    }
    if !at.is_finite() || at < 0. {
        return Err(fail("DELAY_EVENT", "Nonfinite event"));
    }
    let bits = at.to_bits();
    if events
        .range(bits.saturating_sub(32)..=bits.saturating_add(32))
        .next()
        .is_none()
    {
        events.insert(bits);
    }
    if events.len() > 100000 {
        return Err(fail(
            "DELAY_EVENT_BUDGET",
            "Too many delayed source corners",
        ));
    }
    Ok(())
}
pub fn time(request: coupled::time::Request, start: Instant) -> Result<Value> {
    if request.drivers.is_empty() {
        return Err(fail(
            "DELAY_BINDING",
            "Finite Thevenin driver required for delayed network",
        ));
    }
    super::deadline(start, request.budget.as_ref(), "Delayed model admission")?;
    if request.initial.as_ref().is_some_and(|v| v != "steady") {
        return Err(fail("DELAY_INITIAL","Only explicit constant steady prehistory is supported; arbitrary incoming waves/continuation are unsupported"));
    }
    let owned;
    let topology = match (&request.resource, &request.topology) {
        (Some(resource), None) => {
            owned = coupled::composition::Topology {
                instances: vec![frequency::Instance {
                    id: resource.id.clone(),
                    model: resource.clone(),
                    ports: resource.ports.clone(),
                }],
                connection_contacts: None,
                connections: vec![],
                ports: resource
                    .ports
                    .iter()
                    .enumerate()
                    .map(|(index, name)| frequency::OutputPort {
                        name: name.clone(),
                        index,
                    })
                    .collect(),
            };
            &owned
        }
        (None, Some(topology)) => topology,
        _ => return Err(fail("DELAY_INPUT", "Choose one Resource or topology")),
    };
    if topology.connection_contacts.is_some()
        || topology
            .instances
            .iter()
            .any(|i| i.model.data.get("portTerminals").is_some())
    {
        return Err(fail("COUPLED_TERMINAL_MAP","Physical contact joins with delay need authored conductor return/charge trace; contacts only for mapped coupled endpoints"));
    }
    if request
        .numerics
        .as_ref()
        .is_some_and(|n| n.max_step_s.is_some())
    {
        return Err(fail(
            "DELAY_NUMERICS",
            "maxStepS currently requires coupled descriptor backend",
        ));
    }
    let p = topology
        .instances
        .iter()
        .map(|i| i.ports.len())
        .sum::<usize>();
    let outputs = topology.ports.len();
    if p == 0
        || p > 128
        || outputs == 0
        || outputs > 16
        || topology.instances.len() > 32
        || topology
            .instances
            .iter()
            .map(|i| &i.id)
            .collect::<BTreeSet<_>>()
            .len()
            != topology.instances.len()
    {
        return Err(fail("DELAY_BUDGET", "Bounded complete topology required"));
    }
    let time = &request.time;
    if !time.stop_s.is_finite()
        || time.stop_s <= 0.
        || !time.sample_step_s.is_finite()
        || time.sample_step_s <= 0.
        || time.stop_s / time.sample_step_s > 100000.
    {
        return Err(fail("DELAY_TIME", "Invalid or oversized output axis"));
    }
    let mut blocks = vec![];
    let mut refs: Vec<f64> = vec![];
    let mut owner = vec![];
    let source_memory = topology
        .instances
        .iter()
        .map(|i| coupled::composition::source_plan(&i.model))
        .sum::<usize>()
        + request
            .drivers
            .iter()
            .map(|d| {
                256 + match &d.waveform {
                    Waveform::Pwl { points, .. } => points.capacity().saturating_mul(16),
                    _ => 0,
                }
            })
            .sum::<usize>()
        + request.receivers.len() * 128
        + 100_000 * 96; // Simultaneously live bounded formula-event tree.
    let matrix_budget =
        coupled::composition::remaining_budget(request.budget.as_ref(), source_memory)?;
    let mut lumped = vec![];
    let mut physical_ports = vec![];
    let mut potential_count = 0;
    let mut branch_count = 0;
    let mut fits = vec![];
    let mut conditional = false;
    let mut band_max = f64::MAX;
    for (index, instance) in topology.instances.iter().enumerate() {
        super::deadline(start, request.budget.as_ref(), "Delayed block admission")?;
        let r = &instance.model;
        if r.kind != "network"
            || r.ports.len() != instance.ports.len()
            || r.ports.iter().collect::<BTreeSet<_>>().len() != r.ports.len()
            || instance.ports.iter().any(|p| p.is_empty())
            || instance.ports.iter().collect::<BTreeSet<_>>().len() != instance.ports.len()
            || r.analyses
                .as_ref()
                .is_some_and(|a| !a.iter().any(|v| v == "si"))
        {
            return Err(fail(
                "DELAY_RESOURCE",
                "Invalid/unauthorized complete SI model",
            ));
        }
        match r.validity.as_ref().and_then(|v| v["status"].as_str()) {
            None | Some("valid") => {}
            Some("conditional") => conditional = true,
            _ => return Err(fail("DELAY_VALIDITY", "Unavailable source")),
        };
        let band = frequency::declared_band(r)?;
        if band[0] != 0. || band[1] <= 0. {
            return Err(fail(
                "TEMPORAL_BAND",
                "Delayed temporal applicability must include DC and positive upper frequency",
            ));
        }
        if band[1] < f64::MAX {
            conditional = true;
            band_max = band_max.min(band[1]);
        }
        owner.extend(std::iter::repeat_n(index, r.ports.len()));
        if r.data["format"] == "delayed-scattering" {
            let model = model(r)?;
            let offset = refs.len();
            refs.extend(&model.refs);
            physical_ports.extend(potential_count..potential_count + r.ports.len());
            potential_count += r.ports.len();
            blocks.push(Block { offset, model });
        } else {
            let built = coupled::composition::resource(r, Some(&matrix_budget), start)?;
            let m = &built.model;
            if m.reference_topology != "grounded" {
                return Err(fail("COUPLED_TERMINAL_MAP", "Mixed infinity joins require explicit physical terminal mapping; no return inferred"));
            }
            coupled::System::compile(m, r.ports.len(), Some(&matrix_budget))?;
            for port in 0..r.ports.len() {
                let entries: Vec<_> = m
                    .port_incidence
                    .iter()
                    .filter(|(_, j, v)| *j == port && *v != 0.)
                    .collect();
                if entries.len() != 1 || entries[0].2 != 1. {
                    return Err(fail(
                        "COUPLED_TERMINAL_MAP",
                        "Mixed composition requires canonical unit positive-potential ports",
                    ));
                }
                physical_ports.push(potential_count + entries[0].0);
            }
            conditional |= built.conditional;
            if let Some(fit) = built.fit {
                let upper = fit["frequenciesHz"]
                    .as_array()
                    .and_then(|a| a.last())
                    .and_then(Value::as_f64)
                    .ok_or_else(|| fail("TEMPORAL_BAND", "Missing fitted temporal band"))?;
                band_max = band_max.min(upper);
                fits.push(fit);
            }
            lumped.push((potential_count, branch_count, built.model));
            potential_count += lumped.last().unwrap().2.potential_nodes.len();
            branch_count += lumped.last().unwrap().2.current_dofs.len();
        }
        if potential_count > 1024
            || branch_count > 1536
            || potential_count + branch_count + refs.len() > 2048
        {
            return Err(fail(
                "COUPLED_BUDGET",
                "Aggregate mixed descriptor cap exceeded before assembly",
            ));
        }
    }
    let mut parent: Vec<_> = (0..potential_count).collect();
    let mut assigned = vec![false; p];
    let mut groups: Vec<_> = (0..topology.instances.len()).collect();
    for &[a, b] in &topology.connections {
        if a >= p || b >= p || a == b || assigned[a] || assigned[b] {
            return Err(fail(
                "DELAY_TOPOLOGY",
                "Ports pair once; no self/junction connections",
            ));
        }
        assigned[a] = true;
        assigned[b] = true;
        merge(&mut parent, physical_ports[a], physical_ports[b]);
        merge(&mut groups, owner[a], owner[b]);
    }
    for out in &topology.ports {
        if out.index >= p || assigned[out.index] || out.name.is_empty() {
            return Err(fail("DELAY_TOPOLOGY", "Invalid external alias"));
        }
        assigned[out.index] = true;
    }
    if assigned.iter().any(|v| !*v)
        || topology
            .ports
            .iter()
            .map(|p| &p.name)
            .collect::<BTreeSet<_>>()
            .len()
            != outputs
        || (0..groups.len()).any(|i| root(&groups, i) != root(&groups, 0))
    {
        return Err(fail(
            "DELAY_TOPOLOGY",
            "Every port must be paired or external; no hidden/disconnected return groups",
        ));
    }
    let mut map = BTreeMap::new();
    let mut potential_nodes = vec![];
    for i in 0..potential_count {
        let root = root(&parent, i);
        let next = map.len();
        potential_nodes.push(*map.entry(root).or_insert(next));
    }
    let nodes = map.len();
    let wave_base = nodes + branch_count;
    let n = wave_base + refs.len();
    let output_nodes: Vec<_> = topology
        .ports
        .iter()
        .map(|o| potential_nodes[physical_ports[o.index]])
        .collect();
    let mut port_nodes = vec![];
    let mut po = 0;
    for instance in &topology.instances {
        if instance.model.data["format"] == "delayed-scattering" {
            port_nodes.extend(
                physical_ports[po..po + instance.ports.len()]
                    .iter()
                    .map(|i| potential_nodes[*i]),
            );
        }
        po += instance.ports.len();
    }
    let p = refs.len();
    coupled::time::validate_bindings(&request.drivers, &request.receivers, outputs, "DELAY")?;
    if request
        .drivers
        .iter()
        .any(|d| coupled::time::fastest_front(&d.waveform) < 10. / band_max)
    {
        return Err(fail(
            "TEMPORAL_SOURCE_BAND",
            "Fastest declared front must be at least 10/fMax; applicability stays conditional",
        ));
    }
    let samples = (time.stop_s / time.sample_step_s).ceil() as usize + 1;
    let plan = (n + nodes) * (n + nodes) * 1280
        + 2_000_000 * 64
        + samples * (2 * outputs + request.drivers.len() + 12) * 64
        + source_memory;
    let memory = request
        .budget
        .as_ref()
        .and_then(|b| b.max_memory_mi_b)
        .unwrap_or(256.);
    let seconds = request
        .budget
        .as_ref()
        .and_then(|b| b.max_seconds)
        .unwrap_or(120.);
    if !memory.is_finite()
        || memory <= 0.
        || memory > 4096.
        || plan as f64 > memory * 1048576.
        || !seconds.is_finite()
        || seconds <= 0.
    {
        return Err(fail(
            "DELAY_BUDGET",
            "Matrix/history/output memory or deadline admission failed",
        ));
    }
    let mut g = vec![0.; nodes];
    let mut cap = vec![0.; nodes];
    for (port, conductance, capacitance) in
        coupled::time::terminal_admittances(&request.drivers, &request.receivers)
    {
        g[output_nodes[port]] += conductance;
        cap[output_nodes[port]] += capacitance;
    }
    let matrix = |steady: bool| {
        let mut m = BTreeMap::new();
        for i in 0..nodes {
            if g[i] != 0. {
                m.insert((i, i), g[i]);
            }
        }
        for (vo, io, model) in &lumped {
            for &(i, j, value) in &model.incidence {
                let v = potential_nodes[vo + i];
                let current = nodes + io + j;
                *m.entry((v, current)).or_default() += value;
                *m.entry((current, v)).or_default() -= value;
            }
            for &(i, j, value) in &model.resistance_ohm {
                *m.entry((nodes + io + i, nodes + io + j)).or_default() += value;
            }
        }
        for block in &blocks {
            let size = block.model.refs.len();
            for i in 0..size {
                let port = block.offset + i;
                let node = port_nodes[port];
                m.insert((wave_base + port, node), -1. / refs[port].sqrt());
                for j in 0..size {
                    let other = block.offset + j;
                    let k = if steady || block.model.delay == 0. {
                        block.model.k[i][j]
                    } else {
                        0.
                    };
                    let identity = if i == j { 1. } else { 0. };
                    if identity + k != 0. {
                        m.insert((wave_base + port, wave_base + other), identity + k);
                    }
                    if identity - k != 0. {
                        *m.entry((node, wave_base + other)).or_default() +=
                            (identity - k) / refs[port].sqrt();
                    }
                }
            }
        }
        m
    };
    let mut e: BTreeMap<_, _> = cap
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (*c > 0.).then_some(((i, i), *c)))
        .collect();
    for (vo, io, model) in &lumped {
        for &(i, j, value) in &model.capacitance_f {
            *e.entry((potential_nodes[vo + i], potential_nodes[vo + j]))
                .or_default() += value;
        }
        for &(i, j, value) in &model.inductance_h {
            *e.entry((nodes + io + i, nodes + io + j)).or_default() += value;
        }
    }
    let lumped_energy = |x: &[f64]| {
        e.iter()
            .map(|(&(i, j), v)| x[i] * v * x[j] / 2.)
            .sum::<f64>()
    };
    let lumped_loss = |x: &[f64]| {
        lumped
            .iter()
            .map(|(_, io, model)| {
                model
                    .resistance_ohm
                    .iter()
                    .map(|&(i, j, r)| x[nodes + io + i] * r * x[nodes + io + j])
                    .sum::<f64>()
            })
            .sum::<f64>()
    };
    let names: Vec<_> = (0..nodes).map(|i| format!("terminal:{i}")).collect();
    let mut linear = network::Network::descriptor(&names, n, matrix(false), e.clone());
    let mut stats = Counters::default();
    let mut b = source(&request.drivers, &output_nodes, nodes, 0.);
    b.resize(n, 0.);
    let steady = matrix(true);
    let mut gauges = vec![];
    let initial = match linear.solve(steady.clone(), b.clone(), &mut stats) {
        Ok(result) => result,
        Err(error) => {
            super::deadline(
                start,
                request.budget.as_ref(),
                "Mixed steady constraint recovery",
            )?;
            gauges = steady_gauges(&steady, &e, nodes, n)?;
            if gauges.is_empty() {
                return Err(fail(
                    "DELAY_INITIAL",
                    format!("No unique compatible steady history: {error}"),
                ));
            }
            let mut constrained = steady.clone();
            let mut rhs = b.clone();
            constrain(&mut constrained, &mut rhs, &gauges, n);
            linear.solve(constrained, rhs, &mut stats)?
        }
    };
    let (state, mut residual, mut kcl) = initial;
    let mut x = state[..n].to_vec();
    let mut recovered = vec![0.; n];
    let mut scale = b.iter().map(|v| v.abs()).collect::<Vec<_>>();
    for (&(i, j), g) in &steady {
        recovered[i] += g * x[j];
        scale[i] += (g * x[j]).abs();
    }
    if recovered
        .iter()
        .zip(b)
        .zip(scale)
        .any(|((a, b), scale)| (a - b).abs() > 1e-14 + 1e-8 * scale)
    {
        return Err(fail(
            "DELAY_INITIAL",
            "Recovered steady state requires nonphysical gauge currents",
        ));
    }
    super::deadline(start, request.budget.as_ref(), "Delayed initial solve")?;
    let initial_a = x[wave_base..].to_vec();
    let mut history = History {
        times: vec![0.],
        a: vec![initial_a.clone()],
        integrals: vec![vec![0.; p]],
        initial: initial_a,
    };
    let initial_lumped_energy = lumped_energy(&x);
    let initial_delay_energy = storage(&blocks, &history, 0.)?;
    let initial_energy = initial_lumped_energy + initial_delay_energy;
    if !initial_energy.is_finite() {
        return Err(fail("DELAY_NONFINITE", "Initial physical storage overflow"));
    }
    let rel = request
        .numerics
        .as_ref()
        .and_then(|v| v.relative_tolerance)
        .unwrap_or(1e-5);
    let abs = request
        .numerics
        .as_ref()
        .and_then(|v| v.absolute_tolerance)
        .unwrap_or(1e-8);
    let limit = request
        .numerics
        .as_ref()
        .and_then(|v| v.max_iterations)
        .unwrap_or(100000);
    if !rel.is_finite()
        || rel <= 0.
        || rel > 1e-3
        || !abs.is_finite()
        || abs <= 0.
        || abs > 1e-5
        || limit == 0
        || limit > 1000000
    {
        return Err(fail("DELAY_NUMERICS", "Unsupported tolerances/step limit"));
    }
    let mut delays: Vec<_> = blocks
        .iter()
        .map(|b| b.model.delay)
        .filter(|v| *v > 0.)
        .collect();
    delays.sort_by(f64::total_cmp);
    delays.dedup();
    if delays.iter().any(|d| *d < 32. * f64::EPSILON * time.stop_s) {
        return Err(fail(
            "DELAY_RESOLUTION",
            "Positive delay unresolved at requested absolute time",
        ));
    }
    let max_h = request
        .drivers
        .iter()
        .map(|d| d.waveform.max_step())
        .chain(
            request
                .drivers
                .iter()
                .map(|d| coupled::time::fastest_front(&d.waveform) / 4.),
        )
        .chain(delays.iter().map(|d| d / 4.))
        .fold(time.stop_s / 100., f64::min)
        * 0.1
        * (rel / 1e-5).sqrt().min(1.);
    let mut events = BTreeSet::new();
    add_event(&mut events, 0., time.stop_s)?;
    for d in &request.drivers {
        let mut at = 0.;
        loop {
            let next = d.waveform.next(at);
            if next > time.stop_s {
                break;
            }
            add_event(&mut events, next, time.stop_s)?;
            at = next;
        }
    }
    let mut t = 0.;
    let seed = cap
        .iter()
        .enumerate()
        .filter(|(_, c)| **c > 0.)
        .map(|(node, c)| {
            c / (g[node]
                + port_nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| **n == node)
                    .map(|(i, _)| 1. / refs[i])
                    .sum::<f64>())
                / 10.
        })
        .fold(time.stop_s / 1000., f64::min);
    let mut h = max_h.min(seed);
    let mut delivered = 0usize;
    let mut axis = vec![0.];
    let voltage = |x: &[f64]| output_nodes.iter().map(|i| x[*i]).collect::<Vec<_>>();
    let mut values = vec![voltage(&x)];
    let mut energies = vec![initial_energy];
    let mut lumped_energies = vec![initial_lumped_energy];
    let mut delay_energies = vec![initial_delay_energy];
    let mut work_trace = vec![0.];
    let mut r_loss_trace = vec![0.];
    let mut attenuation_trace = vec![0.];
    let mut numerical_trace = vec![0.];
    let port_current = |state: &[f64], derivative: &[f64], at: f64| {
        let mut current = vec![0.; outputs];
        for d in &request.drivers {
            current[d.port] +=
                (d.waveform.sample(at) - state[output_nodes[d.port]]) / d.resistance_ohm;
        }
        for r in &request.receivers {
            current[r.port] -= r
                .resistance_ohm
                .map(|resistance| state[output_nodes[r.port]] / resistance)
                .unwrap_or(0.)
                + r.capacitance_f * derivative[output_nodes[r.port]];
        }
        current
    };
    let mut port_currents = vec![port_current(&x, &vec![0.; n], 0.)];
    let mut next_sample = 1;
    let (mut work, mut external_loss, mut internal_loss, mut max_error) = (0., 0., 0., 0_f64);
    let (mut rlc_loss, mut numerical, mut wave_work) = (0., 0., 0.);
    let step = |linear: &mut network::Network,
                old: &[f64],
                at: f64,
                h: f64,
                stats: &mut Counters,
                history: &History|
     -> Result<(Vec<f64>, f64, f64)> {
        let mut rhs = rhs(
            &blocks,
            history,
            &request.drivers,
            &output_nodes,
            &port_nodes,
            &refs,
            wave_base,
            at,
        )?;
        let physical_rhs = rhs.clone();
        for (&(i, j), g) in &linear.g {
            rhs[i] -= g * old[j];
        }
        let mut matrix = linear.matrix(1. / h, None);
        constrain(&mut matrix, &mut rhs, &gauges, n);
        let (delta, res, kcl) = linear.solve(matrix, rhs, stats)?;
        let next: Vec<_> = old.iter().zip(&delta[..n]).map(|(a, b)| a + b).collect();
        if next.iter().any(|v| !v.is_finite()) {
            return Err(fail("DELAY_NONFINITE", "Physical state increment overflow"));
        }
        let mut lhs = vec![0.; next.len()];
        let mut scale = physical_rhs.iter().map(|v| v.abs()).collect::<Vec<_>>();
        for (&(i, j), e) in &linear.e {
            lhs[i] += e * delta[j] / h;
            scale[i] += (e * delta[j] / h).abs();
        }
        for (&(i, j), g) in &linear.g {
            lhs[i] += g * next[j];
            scale[i] += (g * next[j]).abs();
        }
        if lhs
            .iter()
            .zip(&physical_rhs)
            .zip(scale)
            .any(|((a, b), s)| (a - b).abs() > 1e-14 + 1e-8 * s)
        {
            return Err(fail(
                "DELAY_RESIDUAL",
                "Original mixed descriptor equations failed after increment recovery",
            ));
        }
        Ok((next, res, kcl))
    };
    while t < time.stop_s {
        super::deadline(start, request.budget.as_ref(), "Delayed-wave integration")?;
        if stats.accepted_steps + stats.rejected_steps >= limit {
            return Err(fail(
                "NETWORK_STEP_BUDGET",
                "Delayed-wave step cap; incomplete result",
            ));
        }
        while events
            .first()
            .is_some_and(|v| f64::from_bits(*v) <= t + 64. * f64::EPSILON * t.abs())
        {
            let at = f64::from_bits(events.pop_first().unwrap());
            delivered += 1;
            if delivered > 100000 {
                return Err(fail(
                    "DELAY_EVENT_BUDGET",
                    "Delayed reflection-corner budget",
                ));
            }
            for delay in &delays {
                add_event(&mut events, at + delay, time.stop_s)?;
            }
        }
        let event = events
            .first()
            .map(|v| f64::from_bits(*v))
            .unwrap_or(time.stop_s);
        h = h.min(max_h).min(event - t).min(time.stop_s - t);
        if h <= 0. || (h <= 32. * f64::EPSILON * t.abs().max(1e-30) && h != time.stop_s - t) {
            return Err(fail("NETWORK_STEP_UNDERFLOW", "Delay/front time underflow"));
        }
        let (full, _, _) = step(&mut linear, &x, t + h, h, &mut stats, &history)?;
        let (half, r1, k1) = step(&mut linear, &x, t + h / 2., h / 2., &mut stats, &history)?;
        let (fine, r2, k2) = step(&mut linear, &half, t + h, h / 2., &mut stats, &history)?;
        let error = full
            .iter()
            .zip(&fine)
            .zip(&x)
            .map(|((a, b), c)| (a - b).abs() / (abs + rel * b.abs().max(c.abs())))
            .fold(0., f64::max);
        if error > 1. {
            stats.rejected_steps += 1;
            h *= (0.9 / error.sqrt()).clamp(0.1, 0.5);
            continue;
        }
        stats.accepted_steps += 1;
        max_error = max_error.max(error);
        residual = residual.max(r1).max(r2);
        kcl = kcl.max(k1).max(k2);
        let initial_ledger = [work, external_loss + rlc_loss, internal_loss, numerical];
        let mut ledgers = vec![initial_ledger];
        // Endpoint BE work and physical R losses. History attenuation is integrated
        // independently and exactly; its signed quadrature defect is never hidden
        // inside the nonnegative storage dissipation of backward Euler.
        for (old, new, at, dt) in [(&x, &half, t, h / 2.), (&half, &fine, t + h / 2., h / 2.)] {
            let end = at + dt;
            let volts = voltage(new);
            let (power, loss) =
                coupled::time::terminal_power(&request.drivers, &request.receivers, &volts, end);
            work += power * dt;
            external_loss += loss * dt;
            rlc_loss += lumped_loss(new) * dt;
            let delta: Vec<_> = new.iter().zip(old).map(|(a, b)| a - b).collect();
            numerical += lumped_energy(&delta);
            for block in &blocks {
                let lo = block.offset;
                let hi = lo + block.model.refs.len();
                let a = &new[wave_base + lo..wave_base + hi];
                let delayed;
                let incident = if block.model.delay == 0. {
                    a
                } else {
                    delayed = history.at(end - block.model.delay)?;
                    &delayed[lo..hi]
                };
                let b: Vec<_> = block.model.k.iter().map(|row| dot(row, incident)).collect();
                wave_work += (dot(a, a) - dot(&b, &b)) * dt;
                if block.model.delay == 0. {
                    let a0 = &old[wave_base + lo..wave_base + hi];
                    let b0: Vec<_> = block.model.k.iter().map(|row| dot(row, a0)).collect();
                    internal_loss += dt / 3.
                        * (dot(a0, a0) + dot(a0, a) + dot(a, a)
                            - dot(&b0, &b0)
                            - dot(&b0, &b)
                            - dot(&b, &b));
                } else {
                    internal_loss += attenuation(
                        block,
                        &history,
                        at - block.model.delay,
                        end - block.model.delay,
                    )?;
                }
            }
            ledgers.push([work, external_loss + rlc_loss, internal_loss, numerical]);
        }
        if [
            work,
            external_loss,
            internal_loss,
            rlc_loss,
            numerical,
            wave_work,
            lumped_energy(&fine),
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err(fail(
                "DELAY_NONFINITE",
                "Physical work/loss/storage ledger overflow",
            ));
        }
        history.append(t + h / 2., half[wave_base..].to_vec())?;
        history.append(t + h, fine[wave_base..].to_vec())?;
        while next_sample < samples {
            let out = (next_sample as f64 * time.sample_step_s).min(time.stop_s);
            if out > t + h {
                break;
            }
            let (a, b, f) = if out <= t + h / 2. {
                (&x, &half, (out - t) / (h / 2.))
            } else {
                (&half, &fine, (out - t - h / 2.) / (h / 2.))
            };
            let state: Vec<_> = a.iter().zip(b).map(|(a, b)| a + f * (b - a)).collect();
            axis.push(out);
            values.push(voltage(&state));
            let derivative: Vec<_> = b.iter().zip(a).map(|(b, a)| (b - a) / (h / 2.)).collect();
            port_currents.push(port_current(&state, &derivative, out));
            let ul = lumped_energy(&state);
            let ud = storage(&blocks, &history, out)?;
            lumped_energies.push(ul);
            delay_energies.push(ud);
            energies.push(ul + ud);
            let idx = if out <= t + h / 2. { 0 } else { 1 };
            let cumulative =
                |k: usize| ledgers[idx][k] + f * (ledgers[idx + 1][k] - ledgers[idx][k]);
            work_trace.push(cumulative(0));
            r_loss_trace.push(cumulative(1));
            attenuation_trace.push(cumulative(2));
            numerical_trace.push(cumulative(3));
            next_sample += 1;
        }
        t += h;
        x = fine;
        h *= if error == 0. {
            2.
        } else {
            (0.9 / error.sqrt()).clamp(0.5, 2.)
        };
    }
    let final_lumped_energy = lumped_energy(&x);
    let final_delay_energy = storage(&blocks, &history, t)?;
    let energy = final_lumped_energy + final_delay_energy;
    let lumped_change = final_lumped_energy - initial_lumped_energy;
    let delay_change = final_delay_energy - initial_delay_energy;
    let raw_balance = work - external_loss - rlc_loss - internal_loss - (energy - initial_energy);
    let balance = raw_balance - numerical;
    let history_defect = wave_work - delay_change - internal_loss;
    let norm = work.abs()
        + external_loss
        + rlc_loss
        + internal_loss
        + lumped_change.abs()
        + delay_change.abs()
        + numerical;
    if ![energy, balance, norm, history_defect]
        .iter()
        .all(|v| v.is_finite())
        || balance.abs() > if norm == 0. { 1e-20 } else { 1e-4 * norm }
    {
        return Err(fail("DELAY_ENERGY", format!("Physical source/R/attenuation/storage/BE balance failed: {balance:e}/{norm:e}; refine time/history")));
    }
    super::deadline(start, request.budget.as_ref(), "Delayed response recovery")?;
    let driver_currents: Vec<_> = axis
        .iter()
        .zip(&values)
        .map(|(t, v)| {
            request
                .drivers
                .iter()
                .map(|d| (d.waveform.sample(*t) - v[d.port]) / d.resistance_ohm)
                .collect::<Vec<_>>()
        })
        .collect();
    Ok(
        json!({"status":"complete","diagnostics":[],"axis":axis,"portVoltages":values,"driverCurrentsA":driver_currents,"portCurrentsA":port_currents,"energyJ":energies,"lumpedEnergyJ":lumped_energies,"delayEnergyJ":delay_energies,"sourceWorkJ":work_trace,"resistiveLossJ":r_loss_trace,"attenuationLossJ":attenuation_trace,"beNumericalDissipationJ":numerical_trace,"fit":fits,"conditional":conditional,"counters":stats,"quality":{"relativeResidual":residual,"kclAbsA":kcl,"maximumLocalErrorRatio":max_error,"workJ":work,"externalDissipatedJ":external_loss,"internalDissipatedJ":internal_loss,"rlcDissipatedJ":rlc_loss,"initialStoredEnergyJ":initial_energy,"finalStoredEnergyJ":energy,"initialLumpedEnergyJ":initial_lumped_energy,"finalLumpedEnergyJ":final_lumped_energy,"initialDelayEnergyJ":initial_delay_energy,"finalDelayEnergyJ":final_delay_energy,"beNumericalDissipationJ":numerical,"rawEnergyBalanceJ":raw_balance,"historyQuadratureDefectJ":history_defect,"energyNormalizationJ":norm,"energyBalanceJ":balance},"history":{"zeroEnergyGauges":gauges.len(),"acceptedKnots":history.times.len(),"numericValues":history.times.len()*(2*p+1),"deliveredFormulaCorners":delivered,"interpolation":"piecewise-linear accepted incident waves; exact source/delayed corner delivery"},"memoryPlanBytes":plan,"elapsedMs":start.elapsed().as_secs_f64()*1000.,"assumptions":["Exact passive real constant K with one causal delay per block; no sampled-table recognition or Pade delay","Finite producer bands remain conditional; front screening does not prove absence of out-of-band source spectrum","Power waves (V+RI)/(2sqrtR), (V-RI)/(2sqrtR); connections join authored positive and common return terminals","Finite drivers and passive RC receivers; constant steady prehistory is solved, not zero-filled","Time/history accuracy requires independent global refinement; local BE doubling does not certify delayed interpolation","Output interpolation can miss extrema; arbitrary incoming-wave initial conditions/continuation unsupported"]}),
    )
}
