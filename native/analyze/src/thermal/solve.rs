use super::precondition::{Factor, Pattern, Plan, Planning};
#[cfg(test)]
mod blocked_experiment;
use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Instant,
};
type Result<T> = std::result::Result<T, String>;
#[cfg(test)]
thread_local! {pub(super) static COUPON_SINGLE_SLOT:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
fn bad(code: &str, message: impl std::fmt::Display) -> String {
    format!("THERMAL_{code}: {message}")
}
fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.
}
fn network_identity(r: &Request) -> Result<String> {
    struct CanonicalJson<'a>(&'a Value);
    impl serde::Serialize for CanonicalJson<'_> {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            use serde::ser::{SerializeMap, SerializeSeq};
            match self.0 {
                Value::Object(map) => {
                    let mut fields: Vec<_> = map.iter().collect();
                    fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
                    let mut output = serializer.serialize_map(Some(fields.len()))?;
                    for (k, v) in fields {
                        output.serialize_entry(k, &CanonicalJson(v))?;
                    }
                    output.end()
                }
                Value::Array(array) => {
                    let mut output = serializer.serialize_seq(Some(array.len()))?;
                    for v in array {
                        output.serialize_element(&CanonicalJson(v))?;
                    }
                    output.end()
                }
                v => serde::Serialize::serialize(v, serializer),
            }
        }
    }
    struct Writer(Sha256);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        schema_version: &'static str,
        context_id: &'a str,
        network: &'a Network,
        ambient_c: &'a Signal,
        distributed: Option<&'a Vec<board::DistributedPath>>,
        spatial: Option<CanonicalJson<'a>>,
    }
    let mut writer = Writer(Sha256::new());
    serde_json::to_writer(
        &mut writer,
        &Identity {
            schema_version: "thermal-state-v1",
            context_id: &r.context_id,
            network: &r.network,
            ambient_c: &r.ambient_c,
            distributed: r.spatial.as_ref().map(|s| &s.distributed),
            spatial: r.spatial.as_ref().map(|s| CanonicalJson(&s.report)),
        },
    )
    .map_err(|e| bad("INPUT", e))?;
    Ok(format!("thermal-state-v1:sha256:{:x}", writer.0.finalize()))
}
struct Edge {
    a: usize,
    b: usize,
    law: Cooling,
}
struct DistributedEdge {
    id: String,
    terms: Vec<(usize, f64)>,
    g: f64,
}
impl DistributedEdge {
    fn difference(&self, values: &[f64]) -> f64 {
        let reference = values[self.terms[0].0];
        self.terms
            .iter()
            .map(|(i, w)| w * (values[*i] - reference))
            .sum()
    }
}
struct FactorCache {
    step: Option<u64>,
    conductance: Vec<u64>,
    factor: Option<Arc<Factor>>,
}
struct System<'a> {
    request: &'a Request,
    edges: Vec<Edge>,
    distributed: Vec<DistributedEdge>,
    free: Vec<usize>,
    index: Vec<usize>,
    cap: Vec<f64>,
    ids: BTreeMap<String, usize>,
    start: Instant,
    seconds: f64,
    iterations: usize,
    limit: usize,
    linear_residual: f64,
    original_nodal_residual: f64,
    temperature_origin_c: f64,
    current_time: f64,
    pattern: Option<Arc<Pattern>>,
    preconditioner_extra: Vec<f64>,
    pattern_pair_count: usize,
    memory_budget_bytes: usize,
    physical_plan_bytes: usize,
    transport_reserve_bytes: usize,
    preconditioner_allowance_bytes: usize,
    pattern_peak_bytes: usize,
    factor_planning: Planning,
    linear_roundoff_bound: f64,
    linear_stopping_allowance: f64,
    factor_plan: Option<Option<Arc<Plan>>>,
    factor_plan_bytes: usize,
    factors: VecDeque<FactorCache>,
    linear_iterations: usize,
    factor_builds: usize,
    factor_fallbacks: usize,
    #[cfg(test)] coupon_single_slot: bool,
    #[cfg(test)] coupon_refresh: bool,
    #[cfg(test)] coupon_refreshes: usize,
}
impl Cooling {
    pub(super) fn validate(&self) -> Result<()> {
        match (&self.resistance_k_per_w, &self.table) {
            (Some(r), None) if positive(*r) => {}
            (None, Some(t))
                if t.len() >= 2
                    && t.len() <= 10000
                    && t.iter().all(|p| {
                        p.delta_temperature_k.is_finite() && p.heat_flow_w.is_finite()
                    })
                    && t.iter()
                        .any(|p| p.delta_temperature_k == 0. && p.heat_flow_w == 0.)
                    && t.windows(2).all(|w| {
                        w[1].delta_temperature_k > w[0].delta_temperature_k
                            && w[1].heat_flow_w > w[0].heat_flow_w
                    }) => {}
            _ => {
                return Err(bad(
                    "COOLING",
                    "Choose positive resistance or monotone passive table containing (0,0)",
                ))
            }
        }
        if let Some(s) = &self.scale {
            s.validate()?;
            if s.minimum() < 0. {
                return Err(bad(
                    "COOLING",
                    "Cooling scale must remain nonnegative for its full waveform",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn evaluate(&self, d: f64, t: f64) -> Result<(f64, f64)> {
        let scale = self.scale.as_ref().map_or(1., |s| s.sample(t));
        if !scale.is_finite() || scale < 0. {
            return Err(bad("COOLING", "Cooling scale must remain nonnegative"));
        }
        if scale == 0. {
            return Ok((0., 0.));
        }
        if let Some(r) = self.resistance_k_per_w {
            return Ok((scale * d / r, scale / r));
        }
        let points = self.table.as_ref().unwrap();
        let low = points[0].delta_temperature_k;
        let high = points.last().unwrap().delta_temperature_k;
        if d < low - 1e-10 || d > high + 1e-10 {
            return Err(bad(
                "COOLING_RANGE",
                format!("Delta {d} K outside [{low},{high}]; extrapolation forbidden"),
            ));
        }
        let d = d.clamp(low, high);
        let i = points
            .partition_point(|p| p.delta_temperature_k <= d)
            .saturating_sub(1)
            .min(points.len() - 2);
        let a = &points[i];
        let b = &points[i + 1];
        let g = scale * (b.heat_flow_w - a.heat_flow_w)
            / (b.delta_temperature_k - a.delta_temperature_k);
        Ok((scale * a.heat_flow_w + g * (d - a.delta_temperature_k), g))
    }
}
impl<'a> System<'a> {
    fn check(&mut self) -> Result<()> {
        self.iterations += 1;
        if self.start.elapsed().as_secs_f64() > self.seconds {
            return Err(bad("DEADLINE", "Wall-clock budget exceeded"));
        }
        if self.iterations > self.limit {
            return Err(bad("ITERATIONS", "Iteration budget exceeded"));
        }
        Ok(())
    }
    fn compile(r: &'a Request, start: Instant) -> Result<Self> {
        let n = r.network.nodes.len();
        let m = r.network.links.len();
        let distributed_count = r.spatial.as_ref().map_or(0, |s| s.distributed.len());
        let contact_terms = r.spatial.as_ref().map_or(0, |s| {
            s.distributed
                .iter()
                .map(|p| p.from.len() + p.to.len())
                .sum::<usize>()
        });
        if n == 0
            || n > if r.spatial.is_some() {
                board::spatial_node_hard_limit()
            } else {
                50000
            }
            || m + distributed_count
                > board::spatial_link_limit(r.spatial.as_ref().map_or(0, |s| {
                    s.report["mesh"]["maxNodes"].as_u64().unwrap_or(0) as usize
                }))
            || contact_terms > 2_000_000
            || r.network.sources.len() > 250000
        {
            return Err(bad("BUDGET", "Network size exceeds bounds"));
        }
        let seconds = r
            .budget
            .as_ref()
            .and_then(|b| b.max_seconds)
            .unwrap_or(120.);
        let memory = r
            .budget
            .as_ref()
            .and_then(|b| b.max_memory_mi_b)
            .unwrap_or(1024.);
        if !positive(seconds) || !positive(memory) || memory > 4096. {
            return Err(bad("BUDGET", "Invalid time/memory budget"));
        }
        let memory_budget_bytes = (memory * 1048576.) as usize;
        let physical_plan_bytes = n
            .saturating_mul(640)
            .saturating_add((m + distributed_count).saturating_mul(384))
            .saturating_add(contact_terms.saturating_mul(64))
            .saturating_add(r.network.sources.len().saturating_mul(160))
            .saturating_add(
                r.network
                    .sources
                    .iter()
                    .map(|s| s.power_w.heap_bytes())
                    .sum::<usize>(),
            )
            .saturating_add(
                r.network
                    .links
                    .iter()
                    .map(|l| l.cooling.heap_bytes().saturating_mul(2))
                    .sum::<usize>(),
            )
            .saturating_add(r.ambient_c.heap_bytes())
            .saturating_add(r.retained_bytes);
        let transport_reserve_bytes = memory_budget_bytes / 4;
        if physical_plan_bytes.saturating_add(transport_reserve_bytes) > memory_budget_bytes {
            return Err(bad(
                "BUDGET",
                "Physical-system plus report/transport memory plan exceeds budget",
            ));
        }
        let preconditioner_allowance_bytes =
            memory_budget_bytes - physical_plan_bytes - transport_reserve_bytes;
        if !["steady", "time"].contains(&r.mode.as_str()) || r.context_id.is_empty() {
            return Err(bad("INPUT", "Invalid mode/context"));
        }
        r.ambient_c.validate()?;
        if r.ambient_c.minimum() < -273.15 {
            return Err(bad("INPUT", "Ambient below absolute zero"));
        }
        let mut ids = BTreeMap::new();
        let mut free = vec![];
        let mut index = vec![usize::MAX; n];
        let mut cap = vec![0.; n];
        for (i, node) in r.network.nodes.iter().enumerate() {
            if node.id.is_empty() || ids.insert(node.id.clone(), i).is_some() {
                return Err(bad("INPUT", "Empty/duplicate node"));
            }
            match node.boundary.as_deref() {
                Some("environment") => {
                    if node.heat_capacity_j_per_k.is_some() {
                        return Err(bad("CAPACITY", "Environment must not have capacity"));
                    }
                }
                None => {
                    if node.heat_capacity_j_per_k.is_some_and(|x| !positive(x))
                        || r.mode == "time" && node.heat_capacity_j_per_k.is_none()
                    {
                        return Err(bad(
                            "CAPACITY",
                            format!("{} requires explicit positive capacity in time", node.id),
                        ));
                    }
                    cap[i] = node.heat_capacity_j_per_k.unwrap_or(0.);
                    index[i] = free.len();
                    free.push(i);
                }
                _ => return Err(bad("INPUT", "Unknown boundary")),
            }
        }
        if free.is_empty() {
            return Err(bad("INPUT", "No physical thermal nodes"));
        }
        let mut names = BTreeSet::new();
        let mut edges = vec![];
        for link in &r.network.links {
            if link.id.is_empty() || !names.insert(&link.id) {
                return Err(bad("INPUT", "Empty/duplicate link id"));
            }
            let a = *ids
                .get(&link.from)
                .ok_or_else(|| bad("REFERENCE", "Unknown link node"))?;
            let b = *ids
                .get(&link.to)
                .ok_or_else(|| bad("REFERENCE", "Unknown link node"))?;
            if a == b || index[a] == usize::MAX && index[b] == usize::MAX {
                return Err(bad("INPUT", "Self/environment-only link"));
            }
            let fem = r
                .spatial
                .as_ref()
                .is_some_and(|s| s.fem_links.contains(&link.id));
            if !fem {
                link.cooling.validate()?;
            } else if !link
                .cooling
                .resistance_k_per_w
                .is_some_and(|r| r.is_finite() && r != 0.)
            {
                return Err(bad("FEM", "Invalid assembled stiffness"));
            }
            edges.push(Edge {
                a,
                b,
                law: link.cooling.clone(),
            });
        }
        let mut source_nodes = BTreeSet::new();
        for source in &r.network.sources {
            let i = *ids
                .get(&source.node)
                .ok_or_else(|| bad("REFERENCE", "Unknown source node"))?;
            if index[i] == usize::MAX
                || !source_nodes.insert(i) && r.spatial.is_none()
                || source.weight.is_some_and(|w| !positive(w) || w > 1.)
            {
                return Err(bad(
                    "SOURCE",
                    "Environment/duplicate power assignment or invalid spatial weight",
                ));
            }
            source.power_w.validate()?;
            if source.power_w.minimum() < 0. {
                return Err(bad("SOURCE", "Power waveform must remain nonnegative"));
            }
        }
        let limit = r
            .numerics
            .as_ref()
            .and_then(|n| n.max_iterations)
            .unwrap_or(200000);
        if limit == 0 || limit > 1000000 {
            return Err(bad("ITERATIONS", "maxIterations outside 1..1000000"));
        }
        let mut distributed = vec![];
        if let Some(spatial) = &r.spatial {
            let mut parent: Vec<usize> = (0..n).collect();
            fn root(parent: &mut [usize], mut i: usize) -> usize {
                while parent[i] != i {
                    parent[i] = parent[parent[i]];
                    i = parent[i];
                }
                i
            }
            for (link, edge) in r.network.links.iter().zip(&edges) {
                if spatial.fem_links.contains(&link.id) {
                    let a = root(&mut parent, edge.a);
                    let b = root(&mut parent, edge.b);
                    parent[a] = b;
                }
            }
            for path in &spatial.distributed {
                if !positive(path.conductance) || !names.insert(&path.id) {
                    return Err(bad("BARREL", "Invalid/duplicate finite barrel path"));
                }
                let mut terms = vec![];
                for (weights, sign) in [(&path.from, 1.), (&path.to, -1.)] {
                    if weights.is_empty()
                        || weights.iter().any(|(_, w)| !positive(*w))
                        || (weights.iter().map(|(_, w)| w).sum::<f64>() - 1.).abs() > 1e-12
                    {
                        return Err(bad("BARREL", "Incomplete normalized barrel rim contact"));
                    }
                    let mut physical_component = None;
                    for (id, weight) in weights {
                        let node = *ids
                            .get(id)
                            .ok_or_else(|| bad("BARREL", "Unknown rim contact node"))?;
                        let component = root(&mut parent, node);
                        if physical_component.is_some_and(|c| c != component) {
                            return Err(bad(
                                "BARREL",
                                "Annular contact spans disconnected physical mesh components",
                            ));
                        }
                        physical_component = Some(component);
                        terms.push((node, sign * weight));
                    }
                }
                distributed.push(DistributedEdge {
                    id: path.id.clone(),
                    terms,
                    g: path.conductance,
                });
            }
        }
        // Admit the complete finite-barrel outer products before expanding any
        // contact pairs. These entries belong only to M, never the physical A.
        let reserve = preconditioner_allowance_bytes;
        let extra_pairs = distributed
            .iter()
            .try_fold(0usize, |n, e| {
                n.checked_add(
                    e.terms
                        .len()
                        .saturating_mul(e.terms.len().saturating_sub(1))
                        / 2,
                )
            })
            .unwrap_or(usize::MAX);
        let count = edges.len().saturating_add(extra_pairs);
        let pattern_peak = free
            .len()
            .saturating_mul(256)
            .saturating_add(count.saturating_add(free.len()).saturating_mul(96));
        let admitted = free
            .len()
            .saturating_mul(256)
            .saturating_add(count.saturating_add(free.len()).saturating_mul(96))
            <= reserve;
        let mut preconditioner_extra = vec![];
        let pattern = if admitted {
            let mut pairs: Vec<_> = edges.iter().map(|e| (index[e.a], index[e.b])).collect();
            pairs.reserve(extra_pairs);
            preconditioner_extra.reserve(extra_pairs);
            for e in &distributed {
                for (a, (node, w)) in e.terms.iter().enumerate() {
                    for (other, v) in e.terms.iter().skip(a + 1) {
                        pairs.push((index[*node], index[*other]));
                        preconditioner_extra.push(-e.g * w * v);
                    }
                }
            }
            Some(Arc::new(Pattern::new(free.len(), &pairs)))
        } else {
            None
        };
        Ok(Self {
            pattern,
            preconditioner_extra,
            pattern_pair_count: count,
            memory_budget_bytes,
            physical_plan_bytes,
            transport_reserve_bytes,
            preconditioner_allowance_bytes,
            pattern_peak_bytes: if admitted { pattern_peak } else { 0 },
            factor_planning: Planning {
                bytes: free
                    .len()
                    .saturating_mul(256)
                    .saturating_add(count.saturating_add(free.len()).saturating_mul(96)),
                ..Planning::default()
            },
            linear_roundoff_bound: 0.,
            linear_stopping_allowance: 0.,
            factor_plan: None,
            factor_plan_bytes: 0,
            factors: VecDeque::new(),
            linear_iterations: 0,
            factor_builds: 0,
            factor_fallbacks: 0,
            #[cfg(test)] coupon_single_slot: COUPON_SINGLE_SLOT.with(|v|v.get()),
            #[cfg(test)] coupon_refresh: false,
            #[cfg(test)] coupon_refreshes: 0,
            request: r,
            edges,
            distributed,
            free,
            index,
            cap,
            ids,
            start,
            seconds,
            iterations: 0,
            limit,
            linear_residual: 0.,
            original_nodal_residual: 0.,
            temperature_origin_c: 0.,
            current_time: 0.,
        })
    }
    fn reachable(&self, t: f64) -> Result<()> {
        let mut seen: Vec<bool> = self.index.iter().map(|i| *i == usize::MAX).collect();
        loop {
            let mut changed = false;
            for e in &self.edges {
                let scale = e.law.scale.as_ref().map_or(1., |s| s.sample(t));
                if scale > 0. && seen[e.a] != seen[e.b] {
                    seen[e.a] = true;
                    seen[e.b] = true;
                    changed = true;
                }
            }
            for edge in &self.distributed {
                if edge.terms.iter().any(|(i, _)| seen[*i]) {
                    for (i, _) in &edge.terms {
                        if !seen[*i] {
                            seen[*i] = true;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        if self.free.iter().any(|i| !seen[*i]) {
            return Err(bad("BOUNDARY","Steady component has no reachable heat boundary (including unloaded floating components)"));
        }
        Ok(())
    }
    fn powers(&self, t: f64) -> Result<Vec<f64>> {
        let mut q = vec![0.; self.cap.len()];
        for s in &self.request.network.sources {
            let p = s.power_w.sample(t) * s.weight.unwrap_or(1.);
            if !p.is_finite() || p < 0. {
                return Err(bad("SOURCE", "Power must remain finite and nonnegative"));
            }
            q[self.ids[&s.node]] += p;
        }
        Ok(q)
    }
    fn boundary(&self, x: &mut [f64], t: f64) {
        let a = self.request.ambient_c.sample(t) - self.temperature_origin_c;
        for (i, j) in self.index.iter().enumerate() {
            if *j == usize::MAX {
                x[i] = a;
            }
        }
    }
    fn steady_temperature_origin(&self, t: f64) -> Result<f64> {
        let ambient = self.request.ambient_c.sample(t);
        let mut ambient_g = 0.;
        for e in &self.edges {
            if (self.index[e.a] == usize::MAX) != (self.index[e.b] == usize::MAX) {
                // A nonlinear boundary has no exact global G for this balance.
                // Keep the declared ambient reference instead of guessing one.
                if e.law.table.is_some() {
                    return Ok(ambient);
                }
                ambient_g += e.law.evaluate(0., t)?.1;
            }
        }
        for e in &self.distributed {
            let environment_weight: f64 = e
                .terms
                .iter()
                .filter(|(i, _)| self.index[*i] == usize::MAX)
                .map(|(_, w)| w)
                .sum();
            ambient_g += e.g * environment_weight * environment_weight;
        }
        let origin = if ambient_g > 0. {
            ambient + self.powers(t)?.iter().sum::<f64>() / ambient_g
        } else {
            ambient
        };
        if !origin.is_finite() {
            return Err(bad(
                "NONFINITE",
                "Nonfinite common thermal temperature origin",
            ));
        }
        Ok(origin)
    }
    fn residual(
        &self,
        x: &[f64],
        previous: Option<(&[f64], f64)>,
        t: f64,
    ) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>, f64, f64)> {
        let _phase=profile::timer(profile::Phase::Assembly);
        let q = self.powers(t)?;
        let mut residual: Vec<f64> = self.free.iter().map(|i| -q[*i]).collect();
        let mut diag = vec![0.; self.free.len()];
        if let Some((old, h)) = previous {
            for (j, i) in self.free.iter().enumerate() {
                let g = self.cap[*i] / h;
                residual[j] += g * (x[*i] - old[*i]);
                diag[j] += g;
            }
        }
        let mut conductance = vec![];
        let mut outgoing = 0.;
        for e in &self.edges {
            let (f, g) = e.law.evaluate(x[e.a] - x[e.b], t)?;
            conductance.push(g);
            let a = self.index[e.a];
            let b = self.index[e.b];
            if a != usize::MAX {
                residual[a] += f;
                diag[a] += g;
            } else {
                outgoing -= f;
            }
            if b != usize::MAX {
                residual[b] -= f;
                diag[b] += g;
            } else {
                outgoing += f;
            }
        }
        for e in &self.distributed {
            let flow = e.g * e.difference(x);
            for (node, weight) in &e.terms {
                let i = self.index[*node];
                if i != usize::MAX {
                    residual[i] += weight * flow;
                    diag[i] += e.g * weight * weight;
                } else {
                    outgoing -= weight * flow;
                }
            }
        }
        Ok((residual, diag, conductance, q.iter().sum(), outgoing))
    }
    fn preconditioner(
        &mut self,
        diag: &[f64],
        g: &[f64],
        step: Option<f64>,
    ) -> Result<Option<Arc<Factor>>> {
        // M contains the ordinary tangent and admitted full barrel outer products. A retains all
        // distributed paths. Sparse fill is admitted before its allocation.
        if self.free.len() < 64 {
            return Ok(None);
        }
        let step_bits = step.map(f64::to_bits);
        #[cfg(test)]
        if self.coupon_single_slot && self.coupon_refresh {self.factors.clear();self.coupon_refresh=false;self.coupon_refreshes+=1;}
        if let Some(i) = self.factors.iter().position(|f| {
            let close = match (step, f.step) {
                (None, None) => true,
                (Some(h), Some(old)) => (0.5..=2.).contains(&(h / f64::from_bits(old))),
                _ => false,
            };
            #[cfg(test)]
            let close=close || (self.coupon_single_slot && step.is_some() && f.step.is_some());
            close
                && (f.factor.is_some() || f.step == step_bits)
                && f.conductance.iter().zip(g).all(|(a, b)| *a == b.to_bits())
        }) {
            profile::count(profile::Count::FactorCacheHit);
            if self.factors[i].step==step_bits {profile::count(profile::Count::SameDtHit);}
            #[cfg(test)]
            if self.factors[i].step!=step_bits && self.factors.iter().any(|f|f.step==step_bits&&f.conductance.len()==g.len()&&f.conductance.iter().zip(g).all(|(a,b)|*a==b.to_bits())){profile::count(profile::Count::BypassedExact);}
            let entry = self.factors.remove(i).unwrap();
            let factor = entry.factor.clone();
            self.factors.push_back(entry);
            return Ok(factor);
        }
        if self.factor_plan.is_none() {
            let bytes = self.preconditioner_allowance_bytes;
            let cache_slots=if self.request.mode == "steady" {1}else{3};
            #[cfg(test)] let cache_slots=if self.coupon_single_slot {1}else{cache_slots};
            let plan = if let Some(pattern) = &self.pattern {
                Plan::prepare(
                    pattern.clone(),
                    bytes,
                    cache_slots,
                    &mut self.factor_planning,
                    self.start,
                    self.seconds,
                )?
                .map(Arc::new)
            } else {
                None
            };
            if let Some(p) = &plan {
                self.factor_plan_bytes = p.memory_bytes;
            }
            self.factor_plan = Some(plan);
        }
        let cache_slots = self
            .factor_plan
            .as_ref()
            .and_then(|p| p.as_ref())
            .map_or(1, |p| p.cache_slots);
        if self.factors.len() >= cache_slots {
            self.factors.pop_front();
        }
        self.factor_builds += 1;
        profile::count(profile::Count::NumericBuild);
        let factor = if let Some(plan) = self.factor_plan.as_ref().unwrap() {
            let mut coefficients = Vec::with_capacity(g.len() + self.preconditioner_extra.len());
            coefficients.extend_from_slice(g);
            coefficients.extend_from_slice(&self.preconditioner_extra);
            plan.factor(diag, &coefficients, self.start, self.seconds)?
                .map(Arc::new)
        } else {
            None
        };
        if factor.is_none() {
            self.factor_fallbacks += 1;
        }
        self.factors.push_back(FactorCache {
            step: step_bits,
            conductance: g.iter().map(|x| x.to_bits()).collect(),
            factor: factor.clone(),
        });
        Ok(factor)
    }
    fn linear(
        &mut self,
        rhs: &[f64],
        diag: &[f64],
        g: &[f64],
        step: Option<f64>,
    ) -> Result<Vec<f64>> {
        self.linear_with_shift(rhs, diag, g, step, true)
    }
    fn linear_with_shift(
        &mut self,
        rhs: &[f64],
        diag: &[f64],
        g: &[f64],
        step: Option<f64>,
        use_shift: bool,
    ) -> Result<Vec<f64>> {
        let _phase=profile::timer(profile::Phase::Linear);
        profile::count(profile::Count::Linear);
        if diag.iter().any(|x| !positive(*x)) {
            return Err(bad("SINGULAR", "Nonpositive matrix diagonal"));
        }
        let factor = self.preconditioner(diag, g, step)?;
        let precondition = |r: &[f64]| -> Result<Vec<f64>> {
            let _phase=profile::timer(profile::Phase::PreconditionerApply);
            factor.as_ref().map_or_else(
                || Ok(r.iter().zip(diag).map(|(r, d)| r / d).collect()),
                |f| f.apply(r),
            )
        };
        let mul = |v: &[f64]| {
            let _phase=profile::timer(profile::Phase::OperatorApply);
            let mut out: Vec<f64> = self
                .free
                .iter()
                .enumerate()
                .map(|(j, i)| step.map_or(0., |h| self.cap[*i] / h * v[j]))
                .collect();
            for (e, g) in self.edges.iter().zip(g) {
                let a = self.index[e.a];
                let b = self.index[e.b];
                let difference = if a == usize::MAX {
                    -v[b]
                } else if b == usize::MAX {
                    v[a]
                } else {
                    v[a] - v[b]
                };
                let flux = g * difference;
                if a != usize::MAX {
                    out[a] += flux;
                }
                if b != usize::MAX {
                    out[b] -= flux;
                }
            }
            for e in &self.distributed {
                let ri = self.index[e.terms[0].0];
                let reference = if ri == usize::MAX { 0. } else { v[ri] };
                let delta: f64 = e
                    .terms
                    .iter()
                    .map(|(node, w)| {
                        let i = self.index[*node];
                        w * (if i == usize::MAX { 0. } else { v[i] } - reference)
                    })
                    .sum();
                let flow = e.g * delta;
                for (node, w) in &e.terms {
                    let i = self.index[*node];
                    if i != usize::MAX {
                        out[i] += w * flow;
                    }
                }
            }
            out
        };
        let norm = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        let rhs_norm = norm(rhs);
        if !rhs_norm.is_finite() {
            return Err(bad("LINEAR_RESIDUAL", "Nonfinite RHS norm"));
        }
        // Exactly internal constant modes vanish. Never compute A*1 by
        // cancellation of large FEM entries or by a noisy global multiplication.
        let original_rhs = rhs;
        let mut ambient_action = vec![0.; rhs.len()];
        if use_shift && step.is_none() {
            for (e, g) in self.edges.iter().zip(g) {
                let a = self.index[e.a];
                let b = self.index[e.b];
                if a == usize::MAX && b != usize::MAX {
                    ambient_action[b] += g;
                } else if b == usize::MAX && a != usize::MAX {
                    ambient_action[a] += g;
                }
            }
        }
        let boundary_sum: f64 = ambient_action.iter().sum();
        let origin = if boundary_sum > 0. {
            rhs.iter().sum::<f64>() / boundary_sum
        } else {
            0.
        };
        if !origin.is_finite() {
            return Err(bad("LINEAR_RESIDUAL", "Nonfinite steady affine origin"));
        }
        let shifted_rhs: Vec<_> = rhs
            .iter()
            .zip(&ambient_action)
            .map(|(b, a)| b - origin * a)
            .collect();
        let rhs = shifted_rhs.as_slice();
        let tolerance = 1e-12 + 1e-11 * rhs_norm;
        let physical_limit = 1e-8 * (1. + rhs_norm);
        let roundoff = |v: &[f64]| {
            let _phase=profile::timer(profile::Phase::Roundoff);
            let mut magnitude: Vec<_> = self
                .free
                .iter()
                .enumerate()
                .map(|(j, i)| step.map_or(0., |h| self.cap[*i] / h * v[j].abs()))
                .collect();
            let mut operations = vec![2usize; v.len()];
            for (e, g) in self.edges.iter().zip(g) {
                let a = self.index[e.a];
                let b = self.index[e.b];
                let bound = g.abs()
                    * (if a == usize::MAX { 0. } else { v[a].abs() }
                        + if b == usize::MAX { 0. } else { v[b].abs() });
                for i in [a, b] {
                    if i != usize::MAX {
                        magnitude[i] += bound;
                        operations[i] += 4;
                    }
                }
            }
            for e in &self.distributed {
                let bound = e.g
                    * e.terms
                        .iter()
                        .map(|(node, w)| {
                            let i = self.index[*node];
                            if i == usize::MAX {
                                0.
                            } else {
                                (w * v[i]).abs()
                            }
                        })
                        .sum::<f64>();
                for (node, w) in &e.terms {
                    let i = self.index[*node];
                    if i != usize::MAX {
                        magnitude[i] += w.abs() * bound;
                        operations[i] += e.terms.len() + 4;
                    }
                }
            }
            norm(
                &magnitude
                    .iter()
                    .zip(operations)
                    .map(|(s, n)| f64::EPSILON * n as f64 * s)
                    .collect::<Vec<_>>(),
            )
        };
        let mut x = vec![0.; rhs.len()];
        let mut r = rhs.to_vec();
        let mut z = precondition(&r)?;
        let mut p = z.clone();
        let mut rz: f64 = r.iter().zip(&z).map(|(r, z)| r * z).sum();
        let mut linear_iterations = 0;
        #[cfg(test)] let mut coupon_checkpoint_norm=norm(&r);
        #[cfg(test)] let mut coupon_stagnated=false;
        for _ in 0..(rhs.len() * 4 + 100).min(100000) {
            profile::count(profile::Count::CgIteration);
            linear_iterations += 1;
            if norm(&r) <= tolerance {
                // Recursive CG residual can drift for high-contrast, thin-layer
                // operators. Restart from the independently applied residual;
                // never accept the recursive estimate as convergence evidence.
                let actual = mul(&x);
                for i in 0..r.len() {
                    r[i] = rhs[i] - actual[i];
                }
                if norm(&r) <= linear_limit(tolerance, roundoff(&x), physical_limit) {
                    break;
                }
                z = precondition(&r)?;
                p.clone_from(&z);
                rz = r.iter().zip(&z).map(|(r, z)| r * z).sum();
            }
            // Deadline check is independent of temporal attempted-step budget.
            if self.start.elapsed().as_secs_f64() > self.seconds {
                return Err(bad("DEADLINE",format!("Linear solve deadline at t={} s, dt={step:?}, attempts={}, completed/current CG iterations={}/{linear_iterations}, factor builds/fallbacks={}/{}, preconditioner plan bytes={}, pairs={}, fill={}, estimated bytes={}",self.current_time,self.iterations,self.linear_iterations,self.factor_builds,self.factor_fallbacks,self.factor_plan_bytes,self.pattern_pair_count,self.factor_planning.fill,self.factor_planning.bytes)));
            }
            let ap = mul(&p);
            let pap: f64 = p.iter().zip(&ap).map(|(p, a)| p * a).sum();
            if !positive(pap) {
                return Err(bad("SINGULAR", "Nonpositive tangent matrix"));
            }
            let alpha = rz / pap;
            for i in 0..x.len() {
                x[i] += alpha * p[i];
                r[i] -= alpha * ap[i];
            }
            z = precondition(&r)?;
            let next: f64 = r.iter().zip(&z).map(|(r, z)| r * z).sum();
            let beta = next / rz;
            for i in 0..p.len() {
                p[i] = z[i] + beta * p[i];
            }
            rz = next;
            #[cfg(test)]
            if self.coupon_single_slot && linear_iterations%8==0 {let current=norm(&r);if current>tolerance && current>0.5*coupon_checkpoint_norm {coupon_stagnated=true;}coupon_checkpoint_norm=current;}
        }
        let ax = mul(&x);
        let error = norm(
            &ax.iter()
                .zip(&ambient_action)
                .zip(original_rhs)
                .map(|((a, u), b)| a + origin * u - b)
                .collect::<Vec<_>>(),
        );
        let fp_bound = roundoff(&x)
            + 4. * f64::EPSILON
                * norm(
                    &ambient_action
                        .iter()
                        .map(|a| (origin * a).abs())
                        .collect::<Vec<_>>(),
                );
        if !fp_bound.is_finite() {
            return Err(bad("LINEAR_RESIDUAL", "Nonfinite F64 roundoff bound"));
        }
        let allowance = linear_limit(tolerance * 8., fp_bound, physical_limit);
        if !linear_accepted(error, tolerance * 8., fp_bound, physical_limit) {
            return Err(bad(
                "LINEAR_RESIDUAL",
                format!("CG residual {error}, requested tolerance {tolerance}, F64 bound {fp_bound}, physical limit {physical_limit}"),
            ));
        }
        self.linear_roundoff_bound = self.linear_roundoff_bound.max(fp_bound / (1. + rhs_norm));
        self.linear_stopping_allowance = self
            .linear_stopping_allowance
            .max(allowance / (1. + rhs_norm));
        self.linear_iterations += linear_iterations;
        self.linear_residual = self.linear_residual.max(error / (1. + rhs_norm));
        #[cfg(test)]
        if self.coupon_single_slot {self.coupon_refresh=coupon_stagnated||linear_iterations>32;}
        for value in &mut x {
            *value += origin;
        }
        Ok(x)
    }
    fn step(&mut self, old: &[f64], h: Option<f64>, t: f64) -> Result<(Vec<f64>, f64, f64)> {
        self.current_time = t;
        let mut x = old.to_vec();
        self.boundary(&mut x, t);
        let previous = h.map(|h| (old, h));
        // Normalize the ORIGINAL physical equation in an ambient-relative
        // frame: RHS is source power plus C/h*(old-ambient) in time. Internal
        // paths annihilate this constant exactly. Never normalize by the
        // Newton correction RHS or a cancellation-prone absolute C*T/h.
        let ambient_coordinate = self.request.ambient_c.sample(t) - self.temperature_origin_c;
        let powers = self.powers(t)?;
        let rhs: Vec<_> = self
            .free
            .iter()
            .map(|i| {
                powers[*i]
                    + previous.map_or(0., |(old, h)| {
                        self.cap[*i] / h * (old[*i] - ambient_coordinate)
                    })
            })
            .collect();
        let norm = |v: &[f64]| v.iter().map(|v| v * v).sum::<f64>().sqrt();
        let normalization = 1. + norm(&rhs);
        if !normalization.is_finite() {
            return Err(bad(
                "PHYSICAL_RESIDUAL",
                "Nonfinite original-equation normalization",
            ));
        }
        let physical_limit = 1e-8 * normalization;
        for _ in 0..64 {
            let (r, d, g, input, out) = self.residual(&x, previous, t)?;
            let err = r.iter().map(|x| x.abs()).fold(0., f64::max);
            let roundoff = 64.
                * f64::EPSILON
                * x.iter().map(|x| x.abs()).fold(1., f64::max)
                * d.iter().copied().fold(0., f64::max);
            let original_norm = norm(&r);
            if original_norm.is_finite()
                && original_norm <= physical_limit
                && err <= (1e-10 * (1. + input.abs() + out.abs()) + roundoff).min(physical_limit)
            {
                self.original_nodal_residual = self
                    .original_nodal_residual
                    .max(original_norm / normalization);
                return Ok((x, input, out));
            }
            let dx = self.linear(&r.iter().map(|r| -r).collect::<Vec<_>>(), &d, &g, h)?;
            let mut accepted = false;
            let mut alpha = 1.;
            for _ in 0..48 {
                let mut trial = x.clone();
                for (j, i) in self.free.iter().enumerate() {
                    trial[*i] += alpha * dx[j];
                }
                if let Ok((r, _, _, _, _)) = self.residual(&trial, previous, t) {
                    if r.iter().map(|v| v.abs()).fold(0., f64::max) < err {
                        x = trial;
                        accepted = true;
                        break;
                    }
                }
                alpha *= 0.5;
            }
            if !accepted {
                return Err(bad(
                    "PHYSICAL_RESIDUAL",
                    format!("Newton update cannot reduce original nodal residual: L2={original_norm:e} W, normalization={normalization:e} W, normalized={:e}, required<=1e-8; no stiffness-based floor may weaken this guard",original_norm/normalization),
                ));
            }
        }
        Err(bad("NONLINEAR", "Newton iteration limit"))
    }
    fn next(&self, t: f64) -> f64 {
        self.request
            .network
            .sources
            .iter()
            .map(|s| s.power_w.next(t))
            .chain(std::iter::once(self.request.ambient_c.next(t)))
            .chain(
                self.edges
                    .iter()
                    .filter_map(|e| e.law.scale.as_ref())
                    .map(|s| s.next(t)),
            )
            .fold(f64::INFINITY, f64::min)
    }
    fn max_step(&self) -> f64 {
        self.request
            .network
            .sources
            .iter()
            .map(|s| s.power_w.max_step())
            .chain(std::iter::once(self.request.ambient_c.max_step()))
            .chain(
                self.edges
                    .iter()
                    .filter_map(|e| e.law.scale.as_ref())
                    .map(|s| s.max_step()),
            )
            .fold(f64::INFINITY, f64::min)
    }
}
fn left(t: f64) -> f64 {
    if t > 0. {
        f64::from_bits(t.to_bits() - 1)
    } else {
        t
    }
}
pub fn run(mut r: Request) -> Value {
    let start = Instant::now();
    let result = (|| {
        if r.board.as_ref().is_some_and(|b| b.refinement.is_some()) {
            return super::refinement::run(r, start);
        }
        if let Some(board) = r.board.take() {
            let spatial = board::prepare(
                &board,
                &mut r.network,
                r.mode == "time",
                start,
                r.budget.as_ref(),
            )?;
            r.spatial = Some(spatial);
        }
        execute(&r, start)
    })();
    match result {
        Ok(mut v) => {
            v["elapsedMs"] = json!(start.elapsed().as_secs_f64() * 1000.);
            v
        }
        Err(e) => {
            json!({"status":"rejected","diagnostics":[{"code":e.split(':').next().unwrap_or("THERMAL_ERROR"),"message":e,"severity":"error"}],"elapsedMs":start.elapsed().as_secs_f64()*1000.})
        }
    }
}
pub(super) fn execute(r: &Request, start: Instant) -> Result<Value> {
    let compile_phase=profile::timer(profile::Phase::Compile);
    let mut s = System::compile(r, start)?;
    drop(compile_phase);
    let n = s.cap.len();
    // The independent test-only coverage probe must admit its complete sparse
    // symbolic plan before any numeric factor or iterative solve is attempted.
    #[cfg(test)]
    if r.spatial
        .as_ref()
        .is_some_and(|p| p.report["mesh"]["maxNodes"].as_u64().unwrap_or(0) > 110000)
    {
        let pattern = s
            .pattern
            .as_ref()
            .ok_or_else(|| bad("BUDGET", "Independent probe pattern admission rejected"))?;
        let plan = Plan::prepare(
            pattern.clone(),
            s.preconditioner_allowance_bytes,
            1,
            &mut s.factor_planning,
            s.start,
            s.seconds,
        )?;
        let plan = plan.ok_or_else(|| {
            bad(
                "BUDGET",
                format!(
                    "Independent symbolic admission rejected: {}",
                    serde_json::to_string(&s.factor_planning).unwrap_or_default()
                ),
            )
        })?;
        let peak = s.physical_plan_bytes
            + s.transport_reserve_bytes
            + plan.memory_bytes.max(s.pattern_peak_bytes);
        if peak > s.memory_budget_bytes {
            return Err(bad(
                "BUDGET",
                "Independent complete peak admission rejected",
            ));
        }
        eprintln!("Independent symbolic admission before numeric factor: physical={} report={} sparsePeak={} total={} budget={} planning={}",s.physical_plan_bytes,s.transport_reserve_bytes,plan.memory_bytes,peak,s.memory_budget_bytes,serde_json::to_string(&s.factor_planning).unwrap());
        s.factor_plan_bytes = plan.memory_bytes;
        s.factor_plan = Some(Some(Arc::new(plan)));
    }
    let identity = network_identity(r)?;
    let ambient = r.ambient_c.sample(0.);
    s.temperature_origin_c = if r.mode == "time" {
        if let Some(origin) = r.initial.as_ref().and_then(|i| i.get("temperatureOriginC")) {
            origin
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| bad("CONTINUATION", "Invalid saved coordinate origin"))?
        } else {
            ambient
        }
    } else {
        ambient
    };
    let mut x = vec![ambient - s.temperature_origin_c; n];
    let mut time = 0.;
    let mut axis = vec![];
    let mut coordinate_rows = vec![];
    let mut temperatures: BTreeMap<String, Vec<f64>> =
        s.ids.keys().map(|id| (id.clone(), vec![])).collect();
    let mut flows: BTreeMap<String, Vec<f64>> = r
        .network
        .links
        .iter()
        .map(|l| (l.id.clone(), vec![]))
        .chain(s.distributed.iter().map(|e| (e.id.clone(), vec![])))
        .collect();
    let mut input_power = vec![];
    let mut output_power = vec![];
    let mut energy = vec![];
    let mut input_energy = vec![];
    let mut output_energy = vec![];
    let mut balance = vec![];
    let mut accepted = 0;
    let mut rejected = 0;
    let mut ei = 0.;
    let mut eo = 0.;
    let mut max_error: f64 = 0.;
    let mut record =
        |s: &System, x: &[f64], t: f64, ei: f64, eo: f64, reference: &[f64]| -> Result<()> {
            let _phase=profile::timer(profile::Phase::Output);
            axis.push(t);
            if r.capture_coordinates {
                coordinate_rows.push(x.to_vec());
            }
            for (id, i) in &s.ids {
                let absolute = x[*i] + s.temperature_origin_c;
                if !absolute.is_finite() || absolute < -273.15 - 1e-8 {
                    return Err(bad(
                        "NONFINITE",
                        "Nonfinite or physically impossible recovered temperature",
                    ));
                }
                temperatures.get_mut(id).unwrap().push(absolute);
            }
            for (l, e) in r.network.links.iter().zip(&s.edges) {
                flows
                    .get_mut(&l.id)
                    .unwrap()
                    .push(e.law.evaluate(x[e.a] - x[e.b], t)?.0);
            }
            for edge in &s.distributed {
                flows
                    .get_mut(&edge.id)
                    .unwrap()
                    .push(edge.g * edge.difference(x));
            }
            let (_, _, _, pi, po) = s.residual(x, None, t)?;
            input_power.push(pi);
            output_power.push(po);
            let stored: f64 = s
                .free
                .iter()
                .map(|i| s.cap[*i] * (x[*i] - reference[*i]))
                .sum();
            energy.push(stored);
            input_energy.push(ei);
            output_energy.push(eo);
            balance.push(ei - eo - stored);
            Ok(())
        };
    let mode = r.mode.as_str();
    if mode == "steady" {
        if r.time.is_some() || r.initial.is_some() {
            return Err(bad("INPUT", "Steady does not use time/continuation"));
        }
        if let Some(initial) = &r.initial_c {
            if let Some(v) = initial.as_f64() {
                if !v.is_finite() || v < -273.15 {
                    return Err(bad("INITIAL", "Nonfinite initialC"));
                }
                x.fill(v - s.temperature_origin_c);
            } else if initial != "ambient" {
                return Err(bad("INITIAL", "Invalid initialC"));
            }
        }
        time = r.at_s.unwrap_or(0.);
        if !time.is_finite() || time < 0. {
            return Err(bad("INPUT", "Invalid atS"));
        }
        s.reachable(time)?;
        let origin = s.steady_temperature_origin(time)?;
        for value in &mut x {
            *value += s.temperature_origin_c - origin;
        }
        s.temperature_origin_c = origin;
        x = s.step(&x, None, time)?.0;
        record(&s, &x, time, 0., 0., &x)?;
        energy.clear();
        input_energy.clear();
        output_energy.clear();
        balance = vec![input_power[0] - output_power[0]];
    } else {
        if r.at_s.is_some() {
            return Err(bad("INPUT", "atS applies only to steady"));
        }
        if r.initial.is_some() && r.initial_c.is_some() {
            return Err(bad("INITIAL", "Choose initial or initialC"));
        }
        if let Some(initial) = &r.initial {
            if initial == "steady" {
                s.reachable(0.)?;
                x = s.step(&x, None, 0.)?.0;
            } else {
                if initial["kind"] != "thermal-state"
                    || initial["networkIdentity"] != identity
                    || initial["contextId"] != r.context_id
                {
                    return Err(bad(
                        "CONTINUATION",
                        "State identity does not match network/context",
                    ));
                }
                time = initial["timeS"]
                    .as_f64()
                    .filter(|t| t.is_finite() && *t >= 0.)
                    .ok_or_else(|| bad("CONTINUATION", "Invalid state time"))?;
                let values = initial["temperaturesC"]
                    .as_array()
                    .ok_or_else(|| bad("CONTINUATION", "Missing state temperatures"))?;
                if values.len() != n {
                    return Err(bad("CONTINUATION", "State node count"));
                }
                x = values
                    .iter()
                    .map(|v| {
                        v.as_f64()
                            .filter(|v| v.is_finite() && *v >= -273.15)
                            .map(|v| v - s.temperature_origin_c)
                            .ok_or_else(|| bad("CONTINUATION", "Nonfinite state"))
                    })
                    .collect::<Result<_>>()?;
                match (
                    initial.get("temperatureOriginC"),
                    initial.get("temperatureCoordinatesK"),
                ) {
                    (None, None) => {}
                    (Some(origin), Some(coordinates)) => {
                        let origin = origin
                            .as_f64()
                            .filter(|v| v.is_finite() && *v == s.temperature_origin_c)
                            .ok_or_else(|| {
                                bad(
                                    "CONTINUATION",
                                    "State coordinate origin differs from fixed time origin",
                                )
                            })?;
                        let coordinates = coordinates
                            .as_array()
                            .filter(|v| v.len() == n)
                            .ok_or_else(|| bad("CONTINUATION", "State coordinate count"))?;
                        x = coordinates
                            .iter()
                            .zip(values)
                            .map(|(coordinate, absolute)| {
                                let v = coordinate.as_f64().filter(|v| v.is_finite()).ok_or_else(
                                    || bad("CONTINUATION", "Nonfinite state coordinate"),
                                )?;
                                if Some(v + origin) != absolute.as_f64() {
                                    return Err(bad(
                                        "CONTINUATION",
                                        "State coordinates disagree with public temperatures",
                                    ));
                                }
                                Ok(v)
                            })
                            .collect::<Result<_>>()?;
                    }
                    _ => {
                        return Err(bad(
                            "CONTINUATION",
                            "Incomplete state coordinate representation",
                        ))
                    }
                }
                s.boundary(&mut x, time);
            }
        } else if let Some(initial) = &r.initial_c {
            let v = if initial == "ambient" {
                ambient
            } else {
                initial
                    .as_f64()
                    .filter(|x| x.is_finite() && *x >= -273.15)
                    .ok_or_else(|| bad("INITIAL", "Invalid initialC"))?
            };
            x.fill(v - s.temperature_origin_c);
            s.boundary(&mut x, 0.);
        } else {
            return Err(bad("INITIAL", "Transient needs initialC or initial"));
        }
        let ts = r
            .time
            .as_ref()
            .ok_or_else(|| bad("INPUT", "Missing time"))?;
        let stop = ts.stop_s;
        let dt = ts.sample_step_s;
        if !positive(stop) || !positive(dt) || dt > stop || stop <= time {
            return Err(bad("INPUT", "Invalid stop/sample or continuation stop"));
        }
        let samples = r
            .observation_times
            .as_ref()
            .map_or(((stop - time) / dt).ceil() + 1., |a| a.len() as f64);
        let numeric_limit = if r
            .spatial
            .as_ref()
            .is_some_and(|s| s.report["mesh"]["maxNodes"].as_u64().unwrap_or(0) > 100000)
        {
            4_000_000.
        } else {
            2_000_000.
        };
        if samples
            * (n + r.network.links.len()
                + s.distributed.len()
                + 7
                + if r.capture_coordinates { n } else { 0 }) as f64
            + if r.capture_coordinates {
                2. * n as f64
            } else {
                0.
            }
            > numeric_limit
            || samples > 100000.
        {
            return Err(bad(
                "BUDGET",
                format!("Report exceeds {numeric_limit} numbers/100000 samples"),
            ));
        }
        if samples * (n + r.network.links.len() + s.distributed.len() + 7) as f64 * 24.
            > (r.budget
                .as_ref()
                .and_then(|b| b.max_memory_mi_b)
                .unwrap_or(1024.)
                * 1048576.)
        {
            return Err(bad("BUDGET", "Report memory plan exceeds budget"));
        }
        let rel = r
            .numerics
            .as_ref()
            .and_then(|n| n.relative_tolerance)
            .unwrap_or(1e-6);
        let abs = r
            .numerics
            .as_ref()
            .and_then(|n| n.absolute_tolerance)
            .unwrap_or(1e-7);
        if !positive(rel) || !positive(abs) || rel > 0.01 || abs > 0.1 {
            return Err(bad("TOLERANCE", "Invalid relative/absolute tolerance"));
        }
        let reference = x.clone();
        record(&s, &x, time, ei, eo, &reference)?;
        let origin = time;
        let mut sample_index = 1.;
        let mut adaptive_h = (stop - time).min(dt) * 0.05;
        while time < stop {
            let report_t = r
                .observation_times
                .as_ref()
                .map_or((origin + sample_index * dt).min(stop), |a| {
                    a[sample_index as usize]
                });
            let event = s.next(time);
            let boundary = event.min(report_t);
            let candidate = time + adaptive_h.min(s.max_step());
            let end = if (candidate - boundary).abs() <= 64. * f64::EPSILON * boundary.abs() {
                boundary
            } else {
                candidate.min(boundary)
            };
            let h = end - time;
            if h <= 16. * f64::EPSILON * time.abs().max(1e-30) {
                return Err(bad(
                    "TIME_RESOLUTION",
                    "Required internal step below F64 time resolution",
                ));
            }
            s.check()?;
            let evaluation = if end == event { left(end) } else { end };
            let (full, _, _) = s.step(&x, Some(h), evaluation)?;
            let (half, pi1, po1) = s.step(&x, Some(h / 2.), time + h / 2.)?;
            let (two, pi2, po2) = s.step(&half, Some(h / 2.), evaluation)?;
            let a = s.request.ambient_c.sample(time) - s.temperature_origin_c;
            let error = s
                .free
                .iter()
                .map(|i| {
                    (two[*i] - full[*i]).abs()
                        / (abs + rel * (two[*i] - a).abs().max((x[*i] - a).abs()))
                })
                .fold(0., f64::max);
            if !error.is_finite() {
                return Err(bad("NONFINITE", "Integration error nonfinite"));
            }
            if error <= 1. {
                x = two;
                time = end;
                s.boundary(&mut x, time);
                ei += h / 2. * (pi1 + pi2);
                eo += h / 2. * (po1 + po2);
                accepted += 1;
                profile::count(profile::Count::Accepted);
                max_error = max_error.max(error);
                if time == report_t {
                    record(&s, &x, time, ei, eo, &reference)?;
                    sample_index += 1.;
                }
            } else {
                rejected += 1;
                profile::count(profile::Count::Rejected);
            }
            let factor = if error == 0. {
                2.
            } else {
                (0.85 / error.sqrt()).clamp(0.2, 2.)
            };
            // Keep attempted internal step distinct from a shortened event/report step.
            adaptive_h = h * factor;
        }
    }
    let _output_phase=profile::timer(profile::Phase::Output);
    let normalized = if mode == "steady" {
        balance[0].abs() / (1e-3 + input_power[0].abs() + output_power[0].abs())
    } else {
        balance.iter().map(|b| b.abs()).fold(0., f64::max)
            / (1e-3 + ei.abs() + eo.abs() + energy.iter().map(|e| e.abs()).fold(0., f64::max))
    };
    if !normalized.is_finite() || normalized > 1e-6 {
        return Err(bad(
            "ENERGY_BALANCE",
            format!("Normalized imbalance {normalized}"),
        ));
    }
    let mut report = json!({"status":"complete","method":"thermal RC / layered P1 FEM CPU/F64; matrix-free PCG with bounded sparse Cholesky preconditioning; adaptive backward Euler", "diagnostics":[],"system":{"nodeIds":r.network.nodes.iter().map(|n|&n.id).collect::<Vec<_>>(),"linkIds":r.network.links.iter().map(|l|&l.id).chain(s.distributed.iter().map(|e|&e.id)).collect::<Vec<_>>()},"axis":axis,"temperaturesC":temperatures,"heatFlowW":flows,"inputPowerW":input_power,"outputPowerW":output_power,"storedEnergyJ":energy,"inputEnergyJ":input_energy,"outputEnergyJ":output_energy,"energyBalance":balance,"spatial":r.spatial.as_ref().map(|s|&s.report),"normalization":{"originalNodalResidualNormalization":"L2 original BE/steady residual / (1 + L2 source-plus-storage RHS in the uniform instantaneous-ambient frame); every solved step <=1e-8","energyScaleFloor":0.001,"unit":if mode=="steady"{"W"}else{"J"},"absoluteBalanceAllowance":1e-9,"relativeBalanceAllowance":1e-6},"quality":{"linearResidual":s.linear_residual,"originalNodalResidual":s.original_nodal_residual,"linearRoundoffBound":s.linear_roundoff_bound,"linearStoppingAllowance":s.linear_stopping_allowance,"normalizedEnergyBalance":normalized,"maximumAcceptedLocalError":max_error},"counters":{"acceptedSteps":accepted,"rejectedSteps":rejected,"attempts":s.iterations,"linearIterations":s.linear_iterations,"preconditionerBuilds":s.factor_builds,"preconditionerFallbacks":s.factor_fallbacks,"preconditionerPlannedBytes":s.factor_plan_bytes,"preconditionerInputPairs":s.pattern_pair_count,"preconditionerBarrelPairs":s.preconditioner_extra.len(),"preconditionerPlanning":s.factor_planning},"memoryPlan":{"requestBytes":s.memory_budget_bytes,"physicalSystemBytes":s.physical_plan_bytes,"transportReportReserveBytes":s.transport_reserve_bytes,"preconditionerAllowanceBytes":s.preconditioner_allowance_bytes,"preconditionerPeakBytes":s.factor_plan_bytes.max(s.pattern_peak_bytes),"totalPlannedPeakBytes":s.physical_plan_bytes+s.transport_reserve_bytes+s.factor_plan_bytes.max(s.pattern_peak_bytes),"refinementWorkspaceBytes":r.retained_bytes,"scope":"Conservative allocation plan; not a strict OS RSS cap"},"finalState":{"kind":"thermal-state","contextId":r.context_id,"networkIdentity":identity,"timeS":time,"temperaturesC":x.iter().map(|v|v+s.temperature_origin_c).collect::<Vec<_>>(),"temperatureOriginC":s.temperature_origin_c,"temperatureCoordinatesK":x}});
    if r.capture_coordinates {
        report["coordinateFrame"] = json!({"originC":s.temperature_origin_c,"nodeOrder":"system.nodeIds","temperaturesK":coordinate_rows,"scope":"Authoritative common-origin coordinates; Celsius values are rounded display projections"});
    }
    #[cfg(test)]
    {report["couponPreconditionerExperiment"]=json!({"singleSlot":s.coupon_single_slot,"refreshes":s.coupon_refreshes,"rule":"Test-only: replace before next CG solve after >32 iterations or an8-iteration checkpoint failing2x residual reduction; operator/residual/time guards unchanged"});}
    if start.elapsed().as_secs_f64() > s.seconds {
        return Err(bad(
            "DEADLINE",
            "Report construction exceeded total wall-clock budget",
        ));
    }
    Ok(report)
}

fn linear_limit(requested: f64, roundoff: f64, physical: f64) -> f64 {
    requested.max(roundoff).min(physical)
}
fn linear_accepted(error: f64, requested: f64, roundoff: f64, physical: f64) -> bool {
    error.is_finite() && error <= linear_limit(requested, roundoff, physical)
}
#[cfg(test)]
mod residual_floor_tests {
    use super::*;
    #[test]
    fn roundoff_floor_never_weakens_physical_residual_guard() {
        for floor in [0., 1e-20, 1., f64::INFINITY, f64::NAN] {
            assert!(!linear_accepted(1.0001e-8, 1e-12, floor, 1e-8));
            assert!(!linear_accepted(f64::INFINITY, 1e-12, floor, 1e-8));
            assert!(linear_limit(1e-12, floor, 1e-8) <= 1e-8);
        }
        assert!(linear_accepted(1e-10, 1e-12, 1e-9, 1e-8));
    }
}

#[cfg(test)]
mod state_identity_tests {
    use super::*;
    #[test]
    fn streamed_canonical_sha256_matches_full_small_reference_and_binds_barrels() {
        let mut r:Request=serde_json::from_value(json!({"contextId":"identity-reference","mode":"time","ambientC":25.,"network":{"nodes":[{"id":"j","heatCapacityJPerK":1.},{"id":"a","boundary":"environment"}],"links":[],"sources":[]}})).unwrap();
        r.spatial = Some(board::Spatial {
            fem_links: BTreeSet::new(),
            distributed: vec![board::DistributedPath {
                id: "barrel".into(),
                from: vec![("j".into(), 1.)],
                to: vec![("a".into(), 1.)],
                conductance: 1.,
            }],
            report: serde_json::from_str("{\"z\":0,\"a\":1}").unwrap(),
        });
        let full=format!("{{\"schemaVersion\":\"thermal-state-v1\",\"contextId\":{},\"network\":{},\"ambientC\":{},\"distributed\":{},\"spatial\":{{\"a\":1,\"z\":0}}}}",serde_json::to_string(&r.context_id).unwrap(),serde_json::to_string(&r.network).unwrap(),serde_json::to_string(&r.ambient_c).unwrap(),serde_json::to_string(&r.spatial.as_ref().unwrap().distributed).unwrap());
        let expected = format!(
            "thermal-state-v1:sha256:{:x}",
            Sha256::digest(full.as_bytes())
        );
        assert_eq!(network_identity(&r).unwrap(), expected);
        r.spatial.as_mut().unwrap().report = serde_json::from_str("{\"a\":1,\"z\":0}").unwrap();
        assert_eq!(network_identity(&r).unwrap(), expected);
        r.spatial.as_mut().unwrap().distributed[0].conductance = 2.;
        assert_ne!(network_identity(&r).unwrap(), expected);
        r.spatial.as_mut().unwrap().distributed[0].conductance = 1.;
        r.spatial.as_mut().unwrap().report["boardIdentity"] = json!("changed-board");
        assert_ne!(network_identity(&r).unwrap(), expected);
    }
}

#[cfg(test)]
mod matrix_identity_tests {
    use super::*;
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/thermal_fixture.rs"
        ));
    }
    #[test]
    fn captured_controller_matrixfree_vs_full_sparse_and_barrel_diagonal() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../debugging/thermal-verification/controller-linear-request.json");
        let mut expected = fixture::captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, 4.);
        expected["board"]["geometryDeviationMm"] = serde_json::json!(0.01);
        let bytes = std::fs::read(path).unwrap_or_else(|_| serde_json::to_vec(&expected).unwrap());
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            expected,
            "Saved controller request must match the authored fixture input"
        );
        let mut r: Request = serde_json::from_slice(&bytes).unwrap();
        let start = Instant::now();
        let board = r.board.take().unwrap();
        r.spatial =
            Some(board::prepare(&board, &mut r.network, false, start, r.budget.as_ref()).unwrap());
        let mut system = System::compile(&r, start).unwrap();
        let state = vec![30.; r.network.nodes.len()];
        let shifted = vec![130.; r.network.nodes.len()];
        let low = system.residual(&state, None, 0.).unwrap();
        let high = system.residual(&shifted, None, 0.).unwrap();
        assert_eq!(
            low.0, high.0,
            "Common temperature shift preserves every physical residual"
        );
        assert_eq!(low.2, high.2);
        for path in &system.distributed {
            assert_eq!(path.difference(&state), 0.);
            assert_eq!(path.difference(&shifted), 0.);
        }

        let (_, diag, g, _, _) = system.residual(&state, None, 0.).unwrap();
        let mut coefficients = g.clone();
        coefficients.extend_from_slice(&system.preconditioner_extra);
        let apply = |v: &[f64]| {
            let mut y = vec![0.; v.len()];
            for (e, g) in system.edges.iter().zip(&g) {
                let a = system.index[e.a];
                let b = system.index[e.b];
                let flux = g
                    * (if a == usize::MAX { 0. } else { v[a] }
                        - if b == usize::MAX { 0. } else { v[b] });
                if a != usize::MAX {
                    y[a] += flux
                }
                if b != usize::MAX {
                    y[b] -= flux
                }
            }
            for e in &system.distributed {
                let delta = e
                    .terms
                    .iter()
                    .map(|(node, w)| {
                        let i = system.index[*node];
                        if i == usize::MAX {
                            0.
                        } else {
                            w * v[i]
                        }
                    })
                    .sum::<f64>();
                for (node, w) in &e.terms {
                    let i = system.index[*node];
                    if i != usize::MAX {
                        y[i] += e.g * w * delta
                    }
                }
            }
            y
        };
        let n = system.free.len();
        let vectors = vec![
            vec![1.; n],
            (0..n).map(|i| 0.01 * (i % 97) as f64).collect(),
            (0..n).map(|i| 80. + (i as f64 * 0.17).sin()).collect(),
            (0..n)
                .map(|i| {
                    ((i.wrapping_mul(1664525).wrapping_add(1013904223)) % 1000) as f64 / 1000. - 0.5
                })
                .collect(),
        ];
        let mut known = vec![];
        let norm = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        for (case, v) in vectors.iter().enumerate() {
            let a = apply(v);
            let (m, magnitude) =
                system
                    .pattern
                    .as_ref()
                    .unwrap()
                    .apply_assembled(&diag, &coefficients, v);
            let error: Vec<_> = a.iter().zip(&m).map(|(x, y)| x - y).collect();
            let backward = error
                .iter()
                .zip(&magnitude)
                .map(|(e, m)| e.abs() / m.max(1e-300))
                .fold(0., f64::max);
            eprintln!(
                "controller A/M case={case} normA={} normError={} maxBackward={backward:e}",
                norm(&a),
                norm(&error)
            );
            assert!(backward < 1e-10);
            known.push(a);
        }
        // Independently assemble barrel-only matrices and prove no diagonal was
        // doubled; unlike full FEM magnitudes, this cannot hide an erroneous ground.
        let mut direct = vec![0.; n];
        let mut assembled = vec![0.; n];
        let mut doubled = vec![0.; n];
        for e in &system.distributed {
            let sum = e.terms.iter().map(|(_, w)| w).sum::<f64>();
            for (a, (node, w)) in e.terms.iter().enumerate() {
                let i = system.index[*node];
                if i == usize::MAX {
                    continue;
                }
                direct[i] += e.g * w * sum;
                assembled[i] += e.g * w * w;
                doubled[i] += e.g * w * w;
                for (other, v) in e.terms.iter().skip(a + 1) {
                    let j = system.index[*other];
                    if j != usize::MAX {
                        assembled[i] += e.g * w * v;
                        assembled[j] += e.g * w * v;
                    }
                }
            }
        }
        let mismatch = norm(
            &assembled
                .iter()
                .zip(&direct)
                .map(|(a, b)| a - b)
                .collect::<Vec<_>>(),
        );
        let false_ground = norm(&doubled);
        eprintln!("controller barrel-only residual {mismatch:e}; injected extra diagonal norm {false_ground:e}; pairs {}",system.pattern_pair_count);
        assert!(mismatch < 1e-12);
        assert!(false_ground > 1e-8);
        let factor = system.preconditioner(&diag, &g, None).unwrap().unwrap();
        for (case, (v, a)) in vectors.iter().zip(known).enumerate() {
            let solved = factor.apply(&a).unwrap();
            let (m, magnitude) =
                system
                    .pattern
                    .as_ref()
                    .unwrap()
                    .apply_assembled(&diag, &coefficients, &solved);
            let backward = m
                .iter()
                .zip(&a)
                .zip(magnitude)
                .map(|((x, y), s)| (x - y).abs() / s.max(1e-300))
                .fold(0., f64::max);
            let forward = solved
                .iter()
                .zip(v)
                .map(|(x, y)| (x - y).abs())
                .fold(0., f64::max);
            eprintln!(
                "controller factor case={case} maxForward={forward:e}, maxBackward={backward:e}"
            );
            assert!(backward < 1e-10);
        }
    }
}

#[cfg(test)]
mod affine_reference_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn affine_and_unshifted_cosine_ring_agree_and_temperature_origin_is_invariant() {
        let n = 65;
        let phase = |i: usize| std::f64::consts::TAU * i as f64 / n as f64;
        let amplitude = 0.1 / (0.1 + 0.4 * (std::f64::consts::PI / n as f64).sin().powi(2));
        for ambient in [0., 25., 125.] {
            let mut nodes: Vec<_> = (0..n)
                .map(|i| json!({"id":format!("j{i}"),"heatCapacityJPerK":2.}))
                .collect();
            nodes.push(json!({"id":"ambient","boundary":"environment"}));
            let links:Vec<_>=(0..n).flat_map(|i|[json!({"id":format!("boundary-{i}"),"from":format!("j{i}"),"to":"ambient","cooling":{"resistanceKPerW":10.}}),json!({"id":format!("ring-{i}"),"from":format!("j{i}"),"to":format!("j{}",(i+1)%n),"cooling":{"resistanceKPerW":10.}})]).collect();
            let sources: Vec<_> = (0..n)
                .map(|i| json!({"node":format!("j{i}"),"powerW":1.+0.1*phase(i).cos()}))
                .collect();
            let r:Request=serde_json::from_value(json!({"contextId":"affine-analytic-reference","mode":"steady","ambientC":ambient,"network":{"nodes":nodes,"links":links,"sources":sources}})).unwrap();
            let mut system = System::compile(&r, Instant::now()).unwrap();
            let state = vec![ambient; n + 1];
            let (residual, diag, g, _, _) = system.residual(&state, None, 0.).unwrap();
            let rhs: Vec<_> = residual.iter().map(|r| -r).collect();
            let plain = system
                .linear_with_shift(&rhs, &diag, &g, None, false)
                .unwrap();
            let shifted = system
                .linear_with_shift(&rhs, &diag, &g, None, true)
                .unwrap();
            for i in 0..n {
                let expected = 10. + amplitude * phase(i).cos();
                assert!((plain[i] - expected).abs() < 1e-10);
                assert!((shifted[i] - expected).abs() < 1e-10);
                assert!((plain[i] - shifted[i]).abs() < 1e-10);
            }
            let solved = run(r);
            assert_eq!(solved["status"], "complete", "{solved}");
            for i in 0..n {
                let rise = solved["temperaturesC"][format!("j{i}")][0]
                    .as_f64()
                    .unwrap()
                    - ambient;
                assert!((rise - 10. - amplitude * phase(i).cos()).abs() < 1e-10);
            }
        }
    }
}
