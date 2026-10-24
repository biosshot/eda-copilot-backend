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
                "Future history requested; step must not exceed delay",
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
    let mut b = vec![0.; nodes];
    for d in drivers {
        b[output_nodes[d.port]] += d.waveform.sample(t) / d.resistance_ohm;
    }
    b
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
    let mut conditional = false;
    let mut band_max = f64::MAX;
    for (index, instance) in topology.instances.iter().enumerate() {
        super::deadline(start, request.budget.as_ref(), "Delayed block admission")?;
        let r = &instance.model;
        if r.kind != "network"
            || r.ports.len() != instance.ports.len()
            || r.ports.iter().collect::<BTreeSet<_>>().len() != r.ports.len()
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
        let model = model(r)?;
        owner.extend(std::iter::repeat_n(index, model.refs.len()));
        let offset = refs.len();
        refs.extend(&model.refs);
        blocks.push(Block { offset, model });
    }
    let mut parent: Vec<_> = (0..p).collect();
    let mut assigned = vec![false; p];
    let mut groups: Vec<_> = (0..blocks.len()).collect();
    for &[a, b] in &topology.connections {
        if a >= p || b >= p || a == b || assigned[a] || assigned[b] {
            return Err(fail(
                "DELAY_TOPOLOGY",
                "Ports pair once; no self/junction connections",
            ));
        }
        assigned[a] = true;
        assigned[b] = true;
        merge(&mut parent, a, b);
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
    let mut port_nodes = vec![];
    for i in 0..p {
        let root = root(&parent, i);
        let next = map.len();
        port_nodes.push(*map.entry(root).or_insert(next));
    }
    let nodes = map.len();
    let n = nodes + p;
    let output_nodes: Vec<_> = topology.ports.iter().map(|o| port_nodes[o.index]).collect();
    if request.drivers.is_empty()
        || request.drivers.len() > 16
        || request.receivers.len() > 32
        || request
            .drivers
            .iter()
            .any(|d| d.port >= outputs || !d.resistance_ohm.is_finite() || d.resistance_ohm <= 0.)
        || request.receivers.iter().any(|r| {
            r.port >= outputs
                || !r.capacitance_f.is_finite()
                || r.capacitance_f < 0.
                || r.resistance_ohm.is_some_and(|v| !v.is_finite() || v <= 0.)
        })
    {
        return Err(fail(
            "DELAY_BINDING",
            "Finite Thevenin drivers and passive RC receivers required",
        ));
    }
    for d in &request.drivers {
        d.waveform.validate()?;
        if d.waveform.discontinuous() {
            return Err(fail(
                "DELAY_FINITE_FRONT",
                "Continuous finite fronts required",
            ));
        }
    }
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
    let mut g = vec![0.; nodes];
    let mut cap = vec![0.; nodes];
    for d in &request.drivers {
        g[output_nodes[d.port]] += 1. / d.resistance_ohm;
    }
    for r in &request.receivers {
        g[output_nodes[r.port]] += r.resistance_ohm.map(|v| 1. / v).unwrap_or(0.);
        cap[output_nodes[r.port]] += r.capacitance_f;
    }
    let matrix = |steady: bool| {
        let mut m = BTreeMap::new();
        for i in 0..nodes {
            if g[i] != 0. {
                m.insert((i, i), g[i]);
            }
        }
        for block in &blocks {
            let size = block.model.refs.len();
            for i in 0..size {
                let port = block.offset + i;
                let node = port_nodes[port];
                m.insert((nodes + port, node), -1. / refs[port].sqrt());
                for j in 0..size {
                    let other = block.offset + j;
                    let k = if steady || block.model.delay == 0. {
                        block.model.k[i][j]
                    } else {
                        0.
                    };
                    let identity = if i == j { 1. } else { 0. };
                    if identity + k != 0. {
                        m.insert((nodes + port, nodes + other), identity + k);
                    }
                    if identity - k != 0. {
                        *m.entry((node, nodes + other)).or_default() +=
                            (identity - k) / refs[port].sqrt();
                    }
                }
            }
        }
        m
    };
    let e: BTreeMap<_, _> = cap
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (*c > 0.).then_some(((i, i), *c)))
        .collect();
    let names: Vec<_> = (0..nodes).map(|i| format!("terminal:{i}")).collect();
    let mut linear = network::Network::descriptor(&names, n, matrix(false), e);
    let samples = (time.stop_s / time.sample_step_s).ceil() as usize + 1;
    let plan = n * n * 1280 + 2_000_000 * 64 + samples * (outputs + 4) * 64;
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
    let mut stats = Counters::default();
    let mut b = source(&request.drivers, &output_nodes, nodes, 0.);
    b.resize(n, 0.);
    let (mut x, mut residual, mut kcl) =
        linear.solve(matrix(true), b, &mut stats).map_err(|e| {
            fail(
                "DELAY_INITIAL",
                format!("No unique compatible steady history: {e}"),
            )
        })?;
    super::deadline(start, request.budget.as_ref(), "Delayed initial solve")?;
    let initial_a = x[nodes..].to_vec();
    let mut history = History {
        times: vec![0.],
        a: vec![initial_a.clone()],
        integrals: vec![vec![0.; p]],
        initial: initial_a,
    };
    let initial_energy = cap.iter().zip(&x).map(|(c, v)| c * v * v / 2.).sum::<f64>()
        + storage(&blocks, &history, 0.)?;
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
        .chain(delays.iter().map(|d| d / 4.))
        .fold(time.stop_s / 100., f64::min);
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
    let mut next_sample = 1;
    let (mut work, mut external_loss, mut internal_loss, mut max_error) = (0., 0., 0., 0_f64);
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
            nodes,
            at,
        )?;
        for (&(i, j), g) in &linear.g {
            rhs[i] -= g * old[j];
        }
        let (delta, res, kcl) = linear.solve(linear.matrix(1. / h, None), rhs, stats)?;
        Ok((
            old.iter().zip(delta).map(|(a, b)| a + b).collect(),
            res,
            kcl,
        ))
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
            .is_some_and(|v| f64::from_bits(*v) <= t + 16. * f64::EPSILON * t.abs())
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
        if h <= 32. * f64::EPSILON * t.abs().max(1e-30) {
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
        // Simpson integration on each accepted continuous half interval. Delay storage
        // uses exact integral of the same piecewise-linear accepted wave history.
        for (old, new, at, dt) in [(&x, &half, t, h / 2.), (&half, &fine, t + h / 2., h / 2.)] {
            for (fraction, weight) in [(0., 1.), (0.5, 4.), (1., 1.)] {
                let at = at + fraction * dt;
                let state: Vec<_> = old
                    .iter()
                    .zip(new)
                    .map(|(a, b)| a + fraction * (b - a))
                    .collect();
                let mut power = 0.;
                let mut loss = 0.;
                for d in &request.drivers {
                    let v = state[output_nodes[d.port]];
                    let current = (d.waveform.sample(at) - v) / d.resistance_ohm;
                    power += d.waveform.sample(at) * current;
                    loss += d.resistance_ohm * current * current;
                }
                for r in &request.receivers {
                    if let Some(resistance) = r.resistance_ohm {
                        let v = state[output_nodes[r.port]];
                        loss += v * v / resistance;
                    }
                }
                work += power * dt * weight / 6.;
                external_loss += loss * dt * weight / 6.;
                for block in &blocks {
                    let size = block.model.refs.len();
                    let a = if block.model.delay == 0. {
                        state[nodes + block.offset..nodes + block.offset + size].to_vec()
                    } else {
                        history.at(at - block.model.delay)?[block.offset..block.offset + size]
                            .to_vec()
                    };
                    let b: Vec<_> = block.model.k.iter().map(|row| dot(row, &a)).collect();
                    internal_loss += (dot(&a, &a) - dot(&b, &b)) * dt * weight / 6.;
                }
            }
        }
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
            next_sample += 1;
        }
        history.append(t + h / 2., half[nodes..].to_vec())?;
        history.append(t + h, fine[nodes..].to_vec())?;
        t += h;
        x = fine;
        h *= if error == 0. {
            2.
        } else {
            (0.9 / error.sqrt()).clamp(0.5, 2.)
        };
    }
    let energy = cap.iter().zip(&x).map(|(c, v)| c * v * v / 2.).sum::<f64>()
        + storage(&blocks, &history, t)?;
    let balance = work - external_loss - internal_loss - (energy - initial_energy);
    if balance.abs()
        > 1e-12
            + 1e-3
                * (work.abs()
                    + external_loss.abs()
                    + internal_loss.abs()
                    + (energy - initial_energy).abs())
    {
        return Err(fail(
            "DELAY_ENERGY",
            "Source/external losses/K contraction/delay storage balance failed; refine time",
        ));
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
        json!({"status":"complete","diagnostics":[],"axis":axis,"portVoltages":values,"driverCurrentsA":driver_currents,"conditional":conditional,"counters":stats,"quality":{"relativeResidual":residual,"kclAbsA":kcl,"maximumLocalErrorRatio":max_error,"workJ":work,"externalDissipatedJ":external_loss,"internalDissipatedJ":internal_loss,"initialStoredEnergyJ":initial_energy,"finalStoredEnergyJ":energy,"energyBalanceJ":balance},"history":{"acceptedKnots":history.times.len(),"numericValues":history.times.len()*(2*p+1),"deliveredFormulaCorners":delivered,"interpolation":"piecewise-linear accepted incident waves; exact source/delayed corner delivery"},"memoryPlanBytes":plan,"elapsedMs":start.elapsed().as_secs_f64()*1000.,"assumptions":["Exact passive real constant K with one causal delay per block; no sampled-table recognition or Pade delay","Finite producer bands remain conditional; front screening does not prove absence of out-of-band source spectrum","Power waves (V+RI)/(2sqrtR), (V-RI)/(2sqrtR); connections join authored positive and common return terminals","Finite drivers and passive RC receivers; constant steady prehistory is solved, not zero-filled","Time/history accuracy requires independent global refinement; local BE doubling does not certify delayed interpolation","Output interpolation can miss extrema; arbitrary incoming-wave initial conditions/continuation unsupported"]}),
    )
}
