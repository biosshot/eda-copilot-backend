//! Goal-controlled common-batch Newton cover. Geometry and native current DOFs
//! are immutable; every omitted Taylor tail is allocated a positive source-mass
//! share. Optional near/self omission carries a conservative potential bound.
use super::*;
use std::cell::Ref;

fn profile_enabled(trace: Option<&std::ffi::OsStr>, profile: Option<&std::ffi::OsStr>) -> bool {
    trace == Some(std::ffi::OsStr::new("1")) || profile == Some(std::ffi::OsStr::new("1"))
}

// Opt-in diagnostic only. Never changes certificates, tolerances or traversal.
// stderr is streamed so completed checkpoints survive an external process kill.
struct Profile {
    id: u64,
    clock: Option<(Instant, Instant)>,
    phase: &'static str,
    axis: Option<usize>,
    counts: [usize; 6], // visited, far, near, exact (planned), zero, bounded omissions
    seconds: [f64; 3], // classification, far callback, near callback
    complete: bool,
}
impl Profile {
    fn new(phase: &'static str, axis: Option<usize>) -> Self {
        let trace = std::env::var_os("P7_MAGNETIC_TRACE");
        let profile = std::env::var_os("P7_MAGNETIC_PHASE_PROFILE");
        let enabled = profile_enabled(trace.as_deref(), profile.as_deref());
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let result = Self { id: if enabled {NEXT_ID.fetch_add(1,std::sync::atomic::Ordering::Relaxed)} else {0},
            clock: enabled.then(|| {let now=Instant::now(); (now,now)}), phase, axis,
            counts: [0;6], seconds: [0.;3], complete:false };
        result.emit("start");
        result
    }
    fn timer(&self) -> Option<Instant> { self.clock.map(|_| Instant::now()) }
    fn add_time(&mut self, slot: usize, start: Option<Instant>) {
        if let Some(start)=start { self.seconds[slot]+=start.elapsed().as_secs_f64(); }
    }
    fn emit(&self, status: &str) {
        if let Some((start,_))=self.clock {
            use std::io::Write;
            let record=serde_json::json!({"id":self.id,"thread":format!("{:?}",std::thread::current().id()),"phase":self.phase,"axis":self.axis,"status":status,
                "wallSeconds":start.elapsed().as_secs_f64(),"visited":self.counts[0],
                "far":self.counts[1],"near":self.counts[2],"plannedExactPairs":self.counts[3],"zero":self.counts[4],"boundedOmittedBlocks":self.counts[5],
                "classificationSeconds":self.seconds[0],"farCallbackSeconds":self.seconds[1],"nearCallbackSeconds":self.seconds[2]});
            let _=writeln!(std::io::stderr().lock(),"P7_MAGNETIC_TRACE {record}");
        }
    }
    fn checkpoint(&mut self, counts: &Counts) {
        if self.clock.is_none() {return;}
        self.counts=[counts.visited,counts.far,counts.near,counts.exact,counts.zero,counts.negligible];
        if counts.visited % 4096 == 0 && self.clock.unwrap().1.elapsed().as_secs_f64()>=1. {
            self.emit("progress"); self.clock.as_mut().unwrap().1=Instant::now();
        }
    }
    fn finish(&mut self) {self.complete=true;self.emit("complete");}
}
impl Drop for Profile {
    fn drop(&mut self) {if !self.complete {self.emit("unfinished");}}
}

pub(super) struct Geometry {
    leaf: Packed,
    leaf_index: Vec<usize>,
    internal_index: Vec<usize>,
    internal_count: usize,
}
fn fnv(values: impl IntoIterator<Item = u64>) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for v in values {
        for b in v.to_le_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("fnv1a64:{hash:016x}")
}
fn add_hash(hash: &mut u64, v: u64) {
    for b in v.to_le_bytes() {
        *hash ^= b as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}
fn required(slot: &mut usize, p: usize) {
    if *slot == usize::MAX || *slot < p {
        *slot = p;
    }
}
fn factorial3(a: [usize; 3]) -> f64 {
    (factorial(a[0]) * factorial(a[1]) * factorial(a[2])) as f64
}

// Pure kernels may run concurrently; interval additions retain traversal order.
fn replay_near_window(pair:&BoundedPairs,space:&Space,q:&[f64],near:&mut [Interval],pending:&mut Vec<(usize,usize)>)->Result<()> {
    if pending.is_empty() {return Ok(());}
    let values=pair.pair_window(pending)?;
    for (&(i,j),value) in pending.iter().zip(values) {
        check_deadline(pair.start,pair.wall)?;
        let coefficient=Interval::point(1e-7)
            .mul(Interval::point(space.cells[i].density_per_amp_m2))
            .mul(Interval::point(space.cells[j].density_per_amp_m2))
            .mul(Interval::point(value));
        for k in 0..3 {
            near[3*i+k]=near[3*i+k].add(coefficient.mul(Interval::point(q[3*j+k])));
            if i!=j {near[3*j+k]=near[3*j+k].add(coefficient.mul(Interval::point(q[3*i+k])));}
        }
    }
    pending.clear();Ok(())
}

impl Operator {
    // Direct quotient generating function: reduction commutes with homogeneous
    // degree, so q_alpha=6V*H_alpha/(degree+3)! without Cartesian alpha! factors.
    pub(super) fn fast_quotient(&self, tetra: [[f64; 3]; 4], node: usize) -> Result<Vec<Interval>> {
        let mut h = vec![Interval::zero(); self.harmonic.list.len()];
        h[0] = Interval::one();
        let center = self.nodes[node].center;
        let radius = Interval::point(self.nodes[node].radius);
        for point in tetra {
            let u: [Interval; 3] = std::array::from_fn(|k| {
                Interval::point(point[k])
                    .sub(Interval::point(center[k]))
                    .div(radius)
            });
            let old = h.clone();
            h = old;
            for degree_now in 1..=self.powers.order {
                for (i, &a) in self.harmonic.list.iter().enumerate() {
                    if degree(a) != degree_now - 1 {
                        continue;
                    }
                    for axis in 0..3 {
                        let value = h[i].mul(u[axis]);
                        let mut target = a;
                        target[axis] += 1;
                        if target[2] <= 1 {
                            let j = self.harmonic.at(target);
                            h[j] = h[j].add(value);
                        } else {
                            let x = self.harmonic.at([target[0] + 2, target[1], 0]);
                            let y = self.harmonic.at([target[0], target[1] + 2, 0]);
                            h[x] = h[x].sub(value);
                            h[y] = h[y].sub(value);
                        }
                    }
                }
            }
        }
        let volume = volume(tetra)?;
        for (value, &a) in h.iter_mut().zip(&self.harmonic.list) {
            *value = value
                .mul(volume)
                .mul(Interval::point(6.))
                .div(Interval::point(factorial(degree(a) + 3) as f64));
        }
        if h.iter().any(|v| !v.finite()) {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_INTERVAL",
                "cached quotient geometry overflow",
            ));
        }
        Ok(h)
    }
    fn ensure_geometry(&self, tetra: &[[[f64; 3]; 4]]) -> Result<(Ref<'_, Geometry>, f64)> {
        let started = Instant::now();
        if self.geometry_cache.borrow().is_none() {
            let leaves = self.nodes.iter().filter(|n| n.children.is_none()).count();
            if leaves != tetra.len() {
                return Err(error(
                    "HIERARCHICAL_MAGNETIC_POLICY",
                    "adaptive geometry cache requires one original tetra per terminal",
                ));
            }
            let mut geometry = Geometry {
                leaf: Packed::new(leaves, &self.harmonic),
                leaf_index: vec![usize::MAX; self.nodes.len()],
                internal_index: vec![usize::MAX; self.nodes.len()],
                internal_count: 0,
            };
            let mut leaf = 0;
            for (node, shape) in self.nodes.iter().enumerate() {
                check_deadline(self.start, self.wall)?;
                if shape.children.is_none() {
                    geometry.leaf_index[node] = leaf;
                    let original = self.permutation[shape.range[0]];
                    geometry.leaf.write(
                        leaf,
                        &self.harmonic,
                        &self.fast_quotient(tetra[original], node)?,
                    )?;
                    leaf += 1;
                } else {
                    geometry.internal_index[node] = geometry.internal_count;
                    geometry.internal_count += 1;
                }
            }
            *self.geometry_cache.borrow_mut() = Some(geometry);
        }
        Ok((
            Ref::map(self.geometry_cache.borrow(), |g| g.as_ref().unwrap()),
            started.elapsed().as_secs_f64(),
        ))
    }
    #[cfg(test)]
    pub(super) fn cover_control(
        &self, tetra: &[[[f64; 3]; 4]], space: &Space,
        columns: &[Vec<f64>], goals: &[f64],
    ) -> Result<serde_json::Value> {
        let started = Instant::now();
        let (geometry, _) = self.ensure_geometry(tetra)?;
        let plan = Plan::new(self, &geometry, space, columns, goals)?;
        let mut self_near = 0usize;
        let mut shape_near = 0usize;
        let mut tail_near = 0usize;
        plan.walk(None, |pair, decision| {
            if matches!(decision, Decision::Near) {
                if pair.a == pair.b { self_near += 1; }
                else {
                    let (r, q) = self.separation(pair.a, pair.b);
                    if !q.finite() || q.hi >= 1. || r.lo <= 0. { shape_near += 1; }
                    else { tail_near += 1; }
                }
            }
            Ok(())
        })?;
        let mut evidence = plan.evidence(goals);
        evidence["nearRejections"] = serde_json::json!({"self":self_near,"overlappingBalls":shape_near,"tailOrderExhausted":tail_near});
        evidence["controlSeconds"] = serde_json::json!(started.elapsed().as_secs_f64());
        Ok(evidence)
    }
    // Experimental caller-controlled partitioning. Geometry is reused and each
    // plan is dropped before constructing the next; original goals are sliced
    // unchanged. Default production dispatch continues to call batch directly.
    pub(super) fn batch_chunked(
        &self, pair: &BoundedPairs, space: &Space,
        columns: &[Vec<f64>], goals: &[f64], chunk_columns: usize,
    ) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, serde_json::Value)> {
        if !(1..=64).contains(&chunk_columns) || columns.len()!=goals.len() {
            return Err(error("HIERARCHICAL_MAGNETIC_GOAL", "chunk columns must be 1..64 with one original goal per column"));
        }
        if columns.len()<=chunk_columns { return self.batch_common(pair,space,columns,goals); }
        let started=Instant::now();
        let mut actions=Vec::with_capacity(columns.len());
        let mut bounds=Vec::with_capacity(columns.len());
        let mut certificates=vec![];
        for offset in (0..columns.len()).step_by(chunk_columns) {
            let end=(offset+chunk_columns).min(columns.len());
            let (mut action,mut bound,certificate)=self.batch_common(pair,space,&columns[offset..end],&goals[offset..end])?;
            actions.append(&mut action);
            bounds.append(&mut bound);
            certificates.push(certificate);
        }
        let signature=|key: &str| fnv(std::iter::once(chunk_columns as u64).chain(certificates.iter()
            .flat_map(|v| v[key].as_str().unwrap().bytes().map(u64::from))));
        let mut certificate=serde_json::json!({
            "coverSignature":signature("coverSignature"),
            "envelopeSignature":signature("envelopeSignature"),
            "modelRevision":1,"rhsColumns":columns.len(),"absoluteActionGoals":goals,
            "batchColumns":chunk_columns,"chunkCount":certificates.len(),"coverAggregation":"sum-of-sequential-chunk-covers",
            "maximumAbsoluteErrorBounds":bounds.iter().map(|b| b.iter().copied().fold(0f64,f64::max)).collect::<Vec<_>>(),
            "actionWallSeconds":started.elapsed().as_secs_f64()
        });
        for key in ["visitedNodePairs","farClusterPairs","nearTiles","exactPairs","zeroBlocks",
            "boundedOmittedBlocks",
            "m2lScalarContractionsPerAction","shiftStencilApplicationsPerAction"] {
            certificate[key]=serde_json::json!(certificates.iter().map(|v|v[key].as_u64().unwrap()).sum::<u64>());
        }
        let enabled=certificates[0]["nearBoundEnabled"].clone();
        if certificates.iter().any(|c|c["nearBoundEnabled"]!=enabled) {return Err(error("HIERARCHICAL_MAGNETIC_GOAL","near-bound policy changed between chunks"));}
        certificate["nearBoundEnabled"]=enabled;
        certificate["nearBoundPolicy"]=serde_json::json!("summed-per-tetra-volume-v1");
        certificate["activeEnvelopeTerminals"]=serde_json::json!((0..space.cells.len()).filter(|&i| columns.iter().any(|q| (0..3).any(|axis| q[3*i+axis]!=0.))).count());
        for (target, source) in [("weightedActionExactPairs","exactPairs"),("weightedActionM2l","m2lScalarContractionsPerAction")] {
            certificate[target]=serde_json::json!(certificates.iter().map(|v|v[source].as_u64().unwrap()*v["rhsColumns"].as_u64().unwrap()).sum::<u64>());
        }
        for key in ["chosenOrderHistogram","maximumMomentDegreeHistogram","maximumLocalDegreeHistogram"] {
            let n=certificates[0][key].as_array().unwrap().len();
            certificate[key]=serde_json::json!((0..n).map(|i|certificates.iter().map(|v|v[key][i].as_u64().unwrap()).sum::<u64>()).collect::<Vec<_>>());
        }
        for key in ["planPreparationSeconds","geometryMomentPreparationSeconds"] {
            certificate[key]=serde_json::json!(certificates.iter().map(|v|v[key].as_f64().unwrap()).sum::<f64>());
        }
        // These are peak retained cover storage, never simultaneous chunk sums.
        for key in ["coverCacheEntries","coverCacheBytes"] {
            certificate[key]=serde_json::json!(certificates.iter().map(|v|v[key].as_u64().unwrap()).max().unwrap());
        }
        certificate["coverCacheEnabled"]=serde_json::json!(certificates.iter().all(|v|v["coverCacheEnabled"]==true));
        certificate["chunkCertificates"]=serde_json::json!(certificates);
        Ok((actions,bounds,certificate))
    }
    pub fn batch(
        &self, pair: &BoundedPairs, space: &Space,
        columns: &[Vec<f64>], goals: &[f64],
    ) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, serde_json::Value)> {
        let chunk = match std::env::var("P7_MAGNETIC_BATCH_COLUMNS") {
            Err(std::env::VarError::NotPresent) => 0,
            Ok(raw) => raw.parse::<usize>().ok().filter(|v|*v<=64).ok_or_else(||error("HIERARCHICAL_MAGNETIC_GOAL","P7_MAGNETIC_BATCH_COLUMNS must be an integer 0..64"))?,
            Err(_) => return Err(error("HIERARCHICAL_MAGNETIC_GOAL","P7_MAGNETIC_BATCH_COLUMNS must be an integer 0..64")),
        };
        if chunk==0 {self.batch_common(pair,space,columns,goals)}
        else {self.batch_chunked(pair,space,columns,goals,chunk)}
    }
    fn batch_common(
        &self,
        pair: &BoundedPairs,
        space: &Space,
        columns: &[Vec<f64>],
        goals: &[f64],
    ) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, serde_json::Value)> {
        let started = Instant::now();
        let mut batch_profile = Profile::new("batch", None);
        let mut geometry_profile = Profile::new("geometry", None);
        let (geometry, geometry_seconds) = self.ensure_geometry(&pair.tetra)?;
        geometry_profile.finish();
        let plan_started = Instant::now();
        let mut plan_profile = Profile::new("plan", None);
        let plan = Plan::new(self, &geometry, space, columns, goals)?;
        plan_profile.finish();
        let plan_seconds = plan_started.elapsed().as_secs_f64();
        if plan_profile.clock.is_some() {
            use std::io::Write;
            let record = serde_json::json!({"phase":"plan-summary", "planSeconds":plan_seconds,
                "coverCacheEnabled":plan.cover_cached, "coverCacheEntries":plan.cover.len(),
                "coverCacheCapacity":plan.cover.capacity(), "coverCacheCapBytes":self.adaptive_cover_cache_bytes,
                "coverCacheRetainedBytes":plan.cover.capacity() * std::mem::size_of::<(Pair, Decision)>(),
                "visited":plan.counts.visited, "far":plan.counts.far, "near":plan.counts.near,
                "zero":plan.counts.zero});
            let _ = writeln!(std::io::stderr().lock(), "P7_MAGNETIC_TRACE {record}");
        }
        let mut actions = Vec::with_capacity(columns.len());
        let mut bounds = Vec::with_capacity(columns.len());
        let mut maxima = Vec::with_capacity(columns.len());
        for (column, &goal) in columns.iter().zip(goals) {
            let mut column_profile = Profile::new("rhs-column", None);
            let (action, bound) = plan.apply(pair, space, column, goal)?;
            column_profile.finish();
            maxima.push(bound.iter().copied().fold(0f64, f64::max));
            actions.push(action);
            bounds.push(bound);
        }
        let mut certificate = plan.evidence(goals);
        certificate["maximumAbsoluteErrorBounds"] = serde_json::json!(maxima);
        certificate["actionWallSeconds"] = serde_json::json!(started.elapsed().as_secs_f64());
        certificate["geometryMomentPreparationSeconds"] = serde_json::json!(geometry_seconds);
        certificate["planPreparationSeconds"] = serde_json::json!(plan_seconds);
        batch_profile.finish();
        Ok((actions, bounds, certificate))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Zero,
    Negligible,
    Near,
    Far(usize),
}
#[derive(Default)]
struct Counts {
    visited: usize,
    far: usize,
    near: usize,
    exact: usize,
    zero: usize,
    negligible: usize,
    orders: Vec<usize>,
    m2l: usize,
}
struct Plan<'a> {
    tree: &'a Operator,
    geometry: &'a Geometry,
    mass: Vec<Interval>,
    max_weight: Vec<f64>,
    node_budget: Vec<f64>,
    node_weight: Vec<Interval>,
    envelope: Vec<f64>,
    tv: Vec<Interval>,
    phi: Vec<Interval>,
    near_bound: bool,
    moment_order: Vec<usize>,
    local_order: Vec<usize>,
    counts: Counts,
    cover_signature: String,
    envelope_signature: String,
    active_terminals: usize,
    cover: Vec<(Pair, Decision)>,
    cover_cached: bool,
}
impl<'a> Plan<'a> {
    fn new(
        tree: &'a Operator,
        geometry: &'a Geometry,
        space: &Space,
        columns: &[Vec<f64>],
        goals: &[f64],
    ) -> Result<Self> {
        Self::new_with_policy(tree, geometry, space, columns, goals,
            std::env::var_os("P7_MAGNETIC_NEAR_BOUND").as_deref() == Some(std::ffi::OsStr::new("1")))
    }
    fn new_with_policy(tree: &'a Operator, geometry: &'a Geometry, space: &Space,
        columns: &[Vec<f64>], goals: &[f64], near_bound: bool) -> Result<Self> {
        let n = space.cells.len();
        let nodes = tree.nodes.len();
        if columns.len() != goals.len()
            || goals.iter().any(|v| !v.is_finite() || *v <= 0.)
            || columns
                .iter()
                .any(|q| q.len() != 3 * n || q.iter().any(|v| !v.is_finite()))
        {
            return Err(error("HIERARCHICAL_MAGNETIC_GOAL","complete finite original current batch and positive action goal per column required"));
        }
        let mut result = Self {
            tree,
            geometry,
            mass: vec![Interval::zero(); nodes],
            max_weight: vec![0.; nodes],
            node_budget: vec![0.; nodes],
            node_weight: vec![Interval::zero(); nodes],
            envelope: vec![0.; n],
            tv: vec![Interval::zero(); nodes],
            phi: if near_bound { vec![Interval::zero(); nodes] } else { Vec::new() },
            near_bound,
            moment_order: vec![usize::MAX; nodes],
            local_order: vec![usize::MAX; nodes],
            counts: Counts::default(),
            cover_signature: String::new(),
            envelope_signature: String::new(),
            active_terminals: 0,
            cover: Vec::new(),
            cover_cached: tree.adaptive_cover_cache_bytes > 0,
        };
        for (q, &goal) in columns.iter().zip(goals) {
            for i in 0..n {
                for axis in 0..3 {
                    let value = Interval::point(q[3 * i + axis].abs()).div(Interval::point(goal));
                    if !value.finite() {
                        return Err(error(
                            "HIERARCHICAL_MAGNETIC_GOAL",
                            "normalized RHS envelope overflow",
                        ));
                    }
                    result.envelope[i] = result.envelope[i].max(value.hi);
                }
            }
        }
        result.envelope_signature = fnv(std::iter::once(columns.len() as u64)
            .chain(goals.iter().map(|v| v.to_bits()))
            .chain(result.envelope.iter().map(|v| v.to_bits())));
        for node in (0..nodes).rev() {
            check_deadline(tree.start, tree.wall)?;
            if let Some([left, right]) = tree.nodes[node].children {
                result.mass[node] = result.mass[left].add(result.mass[right]);
                result.max_weight[node] = result.max_weight[left].max(result.max_weight[right]);
                result.tv[node] = result.tv[left].add(result.tv[right]);
                if near_bound { result.phi[node] = result.phi[left].add(result.phi[right]); }
            } else {
                let i = tree.permutation[tree.nodes[node].range[0]];
                let volume = geometry
                    .leaf
                    .read(geometry.leaf_index[node], &tree.harmonic)[0];
                let mass = Interval::point(space.cells[i].density_per_amp_m2).mul(volume);
                if !mass.finite() || mass.lo <= 0. {
                    return Err(error(
                        "HIERARCHICAL_MAGNETIC_INTERVAL",
                        "positive source mass enclosure required",
                    ));
                }
                result.mass[node] = mass;
                result.max_weight[node] = mass.hi;
                result.tv[node] = mass.mul(Interval::point(result.envelope[i]));
                // Layer-cake rearrangement bounds a uniform source of volume V by
                // its equal-volume ball: 2*pi*(3/(4*pi))^(2/3)*sigma*V^(2/3)
                // < 3*sigma*V^(2/3). Summing tetrahedron bounds remains valid
                // for duplicate or overlapping tetrahedra and self interactions.
                if near_bound {
                let mut root = volume.hi.cbrt();
                let mut certified = false;
                for _ in 0..16 {
                    let cube = Interval::point(root).mul(Interval::point(root)).mul(Interval::point(root));
                    if cube.finite() && cube.lo >= volume.hi { certified = true; break; }
                    root = up(root);
                }
                result.phi[node] = if certified && root.is_finite() {
                    Interval::point(3.).mul(Interval::point(space.cells[i].density_per_amp_m2))
                        .mul(Interval::point(result.envelope[i])).mul(Interval::point(root)).mul(Interval::point(root))
                } else { Interval { lo: 0., hi: f64::INFINITY } };
                }
                if result.envelope[i] > 0. {
                    result.active_terminals += 1;
                }
            }
        }
        if result.mass.iter().any(|v| !v.finite() || v.lo <= 0.)
            || result.tv.iter().any(|v| !v.finite())
            || result.max_weight.iter().any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_GOAL",
                "aggregate source mass or RHS envelope overflow",
            ));
        }
        for node in 0..nodes {
            check_deadline(tree.start, tree.wall)?;
            result.node_budget[node] = Interval::point(0.25).mul(Interval::point(result.mass[node].lo)).div(Interval::point(result.mass[0].hi)).lo;
            result.node_weight[node] = Interval::point(1e-7).mul(Interval::point(result.max_weight[node]));
        }
        let mut moment = vec![usize::MAX; nodes];
        let mut local = moment.clone();
        let mut hash = 0xcbf29ce484222325u64;
        if result.near_bound { add_hash(&mut hash, 1); }
        let mut cover = Vec::new();
        let cache_cap = tree.adaptive_cover_cache_bytes;
        let mut cache_enabled = cache_cap > 0;
        let counts = result.walk(None, |pair, decision| {
            if cache_enabled {
                if cover.len() == cover.capacity() {
                    let entry_bytes = std::mem::size_of::<(Pair, Decision)>();
                    let max_entries = cache_cap / entry_bytes;
                    let desired = cover.capacity().saturating_mul(2).max(1024).min(max_entries);
                    let reserve_failed = desired <= cover.capacity()
                        || cover.try_reserve_exact(desired.saturating_sub(cover.len())).is_err();
                    let bytes_over_cap = cover.capacity().checked_mul(entry_bytes).map_or(true, |b| b > cache_cap);
                    if reserve_failed || bytes_over_cap {
                        cover = Vec::new();
                        cache_enabled = false;
                    }
                }
                if cache_enabled { cover.push((pair, decision)); }
            }
            add_hash(&mut hash, pair.a as u64);
            add_hash(&mut hash, pair.b as u64);
            match decision {
                Decision::Zero => add_hash(&mut hash, 0),
                Decision::Negligible => add_hash(&mut hash, u64::MAX),
                Decision::Near => add_hash(&mut hash, 1),
                Decision::Far(p) => {
                    add_hash(&mut hash, (p + 2) as u64);
                    if result.tv[pair.b].hi > 0. {
                        required(&mut moment[pair.b], p);
                        required(&mut local[pair.a], p);
                    }
                    if result.tv[pair.a].hi > 0. {
                        required(&mut moment[pair.a], p);
                        required(&mut local[pair.b], p);
                    }
                }
            }
            Ok(())
        })?;
        for parent in 0..nodes {
            if let Some(children) = tree.nodes[parent].children {
                for child in children {
                    if moment[parent] != usize::MAX && result.tv[child].hi > 0. {
                        let cap = moment[parent];
                        required(&mut moment[child], cap);
                    }
                    if local[parent] != usize::MAX {
                        let cap = local[parent];
                        required(&mut local[child], cap);
                    }
                }
            }
        }
        result.moment_order = moment;
        result.local_order = local;
        result.counts = counts;
        result.cover = cover;
        result.cover_cached = cache_enabled;
        result.cover_signature = format!("fnv1a64:{hash:016x}");
        Ok(result)
    }
    fn decide(&self, pair: Pair) -> Option<Decision> {
        let Pair { a, b } = pair;
        if self.tv[a].hi == 0. && self.tv[b].hi == 0. {
            return Some(Decision::Zero);
        }
        let budgets = || (self.node_budget[a], self.node_budget[b]);
        let near_budgets = if self.near_bound {
            let (budget_a, budget_b) = budgets();
            let ea = self.node_weight[a].mul(self.phi[b]);
            let eb = self.node_weight[b].mul(self.phi[a]);
            if ea.finite() && eb.finite() && ea.hi <= budget_b && eb.hi <= budget_a {
                return Some(Decision::Negligible);
            }
            Some((budget_a, budget_b))
        } else { None };
        if a == b { return None; }
        let (r, q) = self.tree.separation(a, b);
        if !q.finite() || q.hi >= 1. || r.lo <= 0. { return None; }
        let (budget_a, budget_b) = near_budgets.unwrap_or_else(budgets);
        let inverse_r = Interval::one().div(r);
        let one_minus_q = Interval::one().sub(q);
        let weight_a = self.node_weight[a];
        let weight_b = self.node_weight[b];
        let mut q_power = Interval::one();
        for p in 0..=self.tree.powers.order {
            q_power = q_power.mul(q);
            let tail = inverse_r.mul(q_power).div(one_minus_q);
            let a_error = weight_a.mul(tail).mul(self.tv[b]);
            let b_error = weight_b.mul(tail).mul(self.tv[a]);
            if tail.finite()
                && a_error.finite()
                && b_error.finite()
                && a_error.hi <= budget_b
                && b_error.hi <= budget_a
            {
                return Some(Decision::Far(p));
            }
        }
        None
    }
    #[cfg(test)]
    fn decide_reference(&self, pair: Pair) -> Option<Decision> {
        let Pair { a, b } = pair;
        if self.tv[a].hi == 0. && self.tv[b].hi == 0. {
            return Some(Decision::Zero);
        }
        let budgets = || {
            let a = Interval::point(0.25).mul(Interval::point(self.mass[a].lo)).div(Interval::point(self.mass[0].hi)).lo;
            let b = Interval::point(0.25).mul(Interval::point(self.mass[b].lo)).div(Interval::point(self.mass[0].hi)).lo;
            (a, b)
        };
        let near_budgets = if self.near_bound {
            let (budget_a, budget_b) = budgets();
            let ea = Interval::point(1e-7).mul(Interval::point(self.max_weight[a])).mul(self.phi[b]);
            let eb = Interval::point(1e-7).mul(Interval::point(self.max_weight[b])).mul(self.phi[a]);
            if ea.finite() && eb.finite() && ea.hi <= budget_b && eb.hi <= budget_a {
                return Some(Decision::Negligible);
            }
            Some((budget_a, budget_b))
        } else { None };
        if a == b { return None; }
        let (r, q) = self.tree.separation(a, b);
        if !q.finite() || q.hi >= 1. || r.lo <= 0. { return None; }
        let (budget_a, budget_b) = near_budgets.unwrap_or_else(budgets);
        let inverse_r = Interval::one().div(r);
        let one_minus_q = Interval::one().sub(q);
        let weight_a = Interval::point(1e-7).mul(Interval::point(self.max_weight[a]));
        let weight_b = Interval::point(1e-7).mul(Interval::point(self.max_weight[b]));
        let mut q_power = Interval::one();
        for p in 0..=self.tree.powers.order {
            q_power = q_power.mul(q);
            let tail = inverse_r.mul(q_power).div(one_minus_q);
            let a_error = weight_a.mul(tail).mul(self.tv[b]);
            let b_error = weight_b.mul(tail).mul(self.tv[a]);
            if tail.finite()
                && a_error.finite()
                && b_error.finite()
                && a_error.hi <= budget_b
                && b_error.hi <= budget_a
            {
                return Some(Decision::Far(p));
            }
        }
        None
    }
    fn walk(&self, axis: Option<usize>, mut emit: impl FnMut(Pair, Decision) -> Result<()>) -> Result<Counts> {
        let mut profile = Profile::new(if axis.is_some() {"apply-walk"} else {"cover-walk"}, axis);
        let t = self.tree;
        let mut counts = Counts {
            orders: vec![0; t.powers.order + 1],
            ..Default::default()
        };
        let mut pending = Vec::with_capacity(t.traversal_capacity);
        pending.push(Pair { a: 0, b: 0 });
        while let Some(pair) = pending.pop() {
            check_deadline(t.start, t.wall).map_err(|failure| {
                format!(
                    "{failure}; adaptive visited={},far={},near={},exact={},zero={}",
                    counts.visited, counts.far, counts.near, counts.exact, counts.zero
                )
            })?;
            counts.visited = counts
                .visited
                .checked_add(1)
                .ok_or_else(|| error("MEMORY_LIMIT", "adaptive visited count overflow"))?;
            let a = t.nodes[pair.a].children;
            let b = t.nodes[pair.b].children;
            let decision_started = profile.timer();
            let decision = self.decide(pair).or_else(|| {
                if a.is_none() && b.is_none() {
                    Some(Decision::Near)
                } else {
                    None
                }
            });
            profile.add_time(0, decision_started);
            if let Some(decision) = decision {
                match decision {
                    Decision::Zero => counts.zero += 1,
                    Decision::Negligible => counts.negligible += 1,
                    Decision::Far(p) => {
                        counts.far += 1;
                        counts.orders[p] += 1;
                        let terms = t.coupling_prefix[p];
                        counts.m2l = counts.m2l.checked_add(3 * terms).ok_or_else(|| {
                            error("MEMORY_LIMIT", "adaptive contraction count overflow")
                        })?;
                    }
                    Decision::Near => {
                        counts.near += 1;
                        let na = t.nodes[pair.a].range[1] - t.nodes[pair.a].range[0];
                        let nb = t.nodes[pair.b].range[1] - t.nodes[pair.b].range[0];
                        let exact = if pair.a == pair.b {
                            na.checked_mul(na + 1).map(|v| v / 2)
                        } else {
                            na.checked_mul(nb)
                        }
                        .ok_or_else(|| error("MEMORY_LIMIT", "adaptive exact count overflow"))?;
                        counts.exact = counts.exact.checked_add(exact).ok_or_else(|| {
                            error("MEMORY_LIMIT", "adaptive exact count overflow")
                        })?;
                    }
                }
                profile.checkpoint(&counts);
                let callback_started = profile.timer();
                let emitted = emit(pair, decision);
                match decision {
                    Decision::Far(_) => profile.add_time(1, callback_started),
                    Decision::Near => profile.add_time(2, callback_started),
                    Decision::Zero | Decision::Negligible => {}
                }
                emitted?;
                continue;
            }
            if pair.a == pair.b {
                let [l, r] = a.unwrap();
                if pending.len() + 3 > t.traversal_capacity {
                    return Err(error("MEMORY_LIMIT", "adaptive explicit stack exhausted"));
                }
                pending.extend_from_slice(&[
                    Pair { a: r, b: r },
                    Pair { a: l, b: r },
                    Pair { a: l, b: l },
                ]);
            } else if a.is_some()
                && (b.is_none() || t.nodes[pair.a].radius >= t.nodes[pair.b].radius)
            {
                let [l, r] = a.unwrap();
                if pending.len() + 2 > t.traversal_capacity {
                    return Err(error("MEMORY_LIMIT", "adaptive explicit stack exhausted"));
                }
                pending.extend_from_slice(&[Pair { a: r, b: pair.b }, Pair { a: l, b: pair.b }]);
            } else {
                let [l, r] = b.unwrap();
                if pending.len() + 2 > t.traversal_capacity {
                    return Err(error("MEMORY_LIMIT", "adaptive explicit stack exhausted"));
                }
                pending.extend_from_slice(&[Pair { a: pair.a, b: r }, Pair { a: pair.a, b: l }]);
            }
            profile.checkpoint(&counts);
        }
        profile.checkpoint(&counts);
        profile.finish();
        Ok(counts)
    }
    fn moment(
        &self,
        node: usize,
        state: &Packed,
        space: &Space,
        q: &[f64],
        axis: usize,
    ) -> Vec<Interval> {
        let t = self.tree;
        if t.nodes[node].children.is_some() {
            state.read(self.geometry.internal_index[node], &t.harmonic)
        } else {
            let i = t.permutation[t.nodes[node].range[0]];
            let factor = Interval::point(space.cells[i].density_per_amp_m2)
                .mul(Interval::point(q[3 * i + axis]));
            let mut values = self
                .geometry
                .leaf
                .read(self.geometry.leaf_index[node], &t.harmonic);
            for v in &mut values {
                *v = v.mul(factor);
            }
            values
        }
    }
    fn apply(
        &self,
        pair: &BoundedPairs,
        space: &Space,
        q: &[f64],
        goal: f64,
    ) -> Result<(Vec<f64>, Vec<f64>)> {
        self.apply_with_zero_policy(pair, space, q, goal, true)
    }
    fn apply_with_zero_policy(
        &self,
        pair: &BoundedPairs,
        space: &Space,
        q: &[f64],
        goal: f64,
        skip_zero_pairs: bool,
    ) -> Result<(Vec<f64>, Vec<f64>)> {
        let t = self.tree;
        let n = space.cells.len();
        let rank = t.harmonic.list.len();
        if q.iter().all(|v| *v == 0.) {
            return Ok((vec![0.; 3 * n], vec![0.; 3 * n]));
        }
        let mut action = vec![0.; 3 * n];
        let mut bounds = vec![0.; 3 * n];
        let mut near = vec![Interval::zero(); 3 * n];
        let parallel_near=pair.parallel_enabled();
        let mut pending=if parallel_near {Vec::with_capacity(128)}else{Vec::new()};
        for axis in 0..3 {
            let mut axis_profile = Profile::new("axis", Some(axis));
            let mut upward_profile = Profile::new("source-and-upward", Some(axis));
            let mut moments = Packed::new(self.geometry.internal_count, &t.harmonic);
            let mut local = Packed::new(t.nodes.len(), &t.harmonic);
            let mut tv = vec![Interval::zero(); t.nodes.len()];
            let mut tails = tv.clone();
            for node in (0..t.nodes.len()).rev() {
                check_deadline(t.start, t.wall)?;
                if let Some([left, right]) = t.nodes[node].children {
                    tv[node] = tv[left].add(tv[right]);
                } else {
                    let i = t.permutation[t.nodes[node].range[0]];
                    tv[node] = self.mass[node].mul(Interval::point(q[3 * i + axis].abs()));
                }
            }
            for parent in (0..t.nodes.len()).rev() {
                if self.moment_order[parent] == usize::MAX {
                    continue;
                }
                if let Some(children) = t.nodes[parent].children {
                    for child in children {
                        if tv[child].hi == 0. {
                            continue;
                        }
                        let shifted = t.shifted_required(
                            &self.moment(child, &moments, space, q, axis),
                            parent,
                            child,
                            false,
                            self.moment_order[parent],
                        );
                        moments.add(self.geometry.internal_index[parent], &t.harmonic, &shifted)?;
                    }
                }
            }
            upward_profile.finish();
            // Reuse bounded order/rank scratch across far blocks. The block
            // traversal and every interval addition retain their original order.
            let mut pa = Vec::<Interval>::new();
            let mut pb = Vec::<Interval>::new();
            let mut la = vec![Interval::zero(); rank];
            let mut lb = vec![Interval::zero(); rank];
            let mut apply_block = |block: Pair, decision: Decision| {
                match decision {
                    Decision::Zero => {}
                    Decision::Negligible => {
                        tails[block.a] = tails[block.a].add(self.phi[block.b].mul(Interval::point(goal)));
                        if block.a != block.b {
                            tails[block.b] = tails[block.b].add(self.phi[block.a].mul(Interval::point(goal)));
                        }
                    }
                    Decision::Near => {
                        if axis == 0 {
                            let a = &t.nodes[block.a];
                            let b = &t.nodes[block.b];
                            for x in a.range[0]..a.range[1] {
                                for y in b.range[0]..b.range[1] {
                                    if block.a == block.b && y > x {
                                        continue;
                                    }
                                    check_deadline(t.start, t.wall)?;
                                    let i = t.permutation[x];
                                    let j = t.permutation[y];
                                    // Both reciprocal updates are exactly zero for
                                    // this column; retain the full certified cover.
                                    if skip_zero_pairs && q[3*i..3*i+3].iter().all(|v| *v == 0.) && q[3*j..3*j+3].iter().all(|v| *v == 0.) { continue; }
                                    if parallel_near {
                                        pending.push((i,j));
                                        if pending.len()==128 {replay_near_window(pair,space,q,&mut near,&mut pending)?;}
                                        continue;
                                    }
                                    let value = pair.pair(i.max(j), i.min(j))?;
                                    let coefficient = Interval::point(1e-7)
                                        .mul(Interval::point(space.cells[i].density_per_amp_m2))
                                        .mul(Interval::point(space.cells[j].density_per_amp_m2))
                                        .mul(Interval::point(value));
                                    for k in 0..3 {
                                        near[3 * i + k] = near[3 * i + k]
                                            .add(coefficient.mul(Interval::point(q[3 * j + k])));
                                        if i != j {
                                            near[3 * j + k] = near[3 * j + k].add(
                                                coefficient.mul(Interval::point(q[3 * i + k])),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Decision::Far(p) => {
                        let (mut derivatives, tail, sa, sb) =
                            t.jet_required(block.a, block.b, p)?;
                        let (r, _) = t.separation(block.a, block.b);
                        for (v, &a) in derivatives.iter_mut().zip(&t.powers.list) {
                            *v = v.mul(Interval::point(factorial3(a))).div(r);
                        }
                        tails[block.a] = tails[block.a].add(tail.mul(tv[block.b]));
                        tails[block.b] = tails[block.b].add(tail.mul(tv[block.a]));
                        pa.resize(p + 1, Interval::zero());
                        pb.resize(p + 1, Interval::zero());
                        for d in 0..=p {
                            pa[d] = sa.pow(d);
                            pb[d] = sb.pow(d);
                        }
                        let mut ma = self.moment(block.a, &moments, space, q, axis);
                        let mut mb = self.moment(block.b, &moments, space, q, axis);
                        for (i, &a) in t
                            .harmonic
                            .list
                            .iter()
                            .enumerate()
                            .take_while(|(_, a)| degree(**a) <= p)
                        {
                            ma[i] = ma[i].mul(pa[degree(a)]);
                            mb[i] = mb[i].mul(pb[degree(a)]);
                        }
                        la.fill(Interval::zero());
                        lb.fill(Interval::zero());
                        for term in t.couplings.iter().take(t.coupling_prefix[p]) {
                            let mut value = derivatives[term.jet];
                            if term.source_degree % 2 == 1 {
                                value = value.neg();
                            }
                            la[term.target] = la[term.target].add(value.mul(mb[term.source]));
                            lb[term.source] = lb[term.source].add(value.mul(ma[term.target]));
                        }
                        for (i, &a) in t
                            .harmonic
                            .list
                            .iter()
                            .enumerate()
                            .take_while(|(_, a)| degree(**a) <= p)
                        {
                            la[i] = la[i].mul(pa[degree(a)]);
                            lb[i] = lb[i].mul(pb[degree(a)]);
                        }
                        if tv[block.b].hi > 0. {
                            local.add(block.a, &t.harmonic, &la)?;
                        }
                        if tv[block.a].hi > 0. {
                            local.add(block.b, &t.harmonic, &lb)?;
                        }
                    }
                }
                Ok(())
            };
            if self.cover_cached {
                let mut profile = Profile::new("apply-cover-replay", Some(axis));
                let mut counts = Counts { orders: vec![0; t.powers.order + 1], ..Default::default() };
                for &(block, decision) in &self.cover {
                    check_deadline(t.start, t.wall)?;
                    counts.visited += 1;
                    match decision {
                        Decision::Zero => counts.zero += 1,
                    Decision::Negligible => counts.negligible += 1,
                        Decision::Near => {
                            counts.near += 1;
                            let a=&t.nodes[block.a]; let b=&t.nodes[block.b];
                            let na=a.range[1]-a.range[0]; let nb=b.range[1]-b.range[0];
                            let exact=if block.a==block.b {na.checked_mul(na+1).map(|v|v/2)} else {na.checked_mul(nb)}
                                .ok_or_else(||error("MEMORY_LIMIT","adaptive exact count overflow"))?;
                            counts.exact=counts.exact.checked_add(exact).ok_or_else(||error("MEMORY_LIMIT","adaptive exact count overflow"))?;
                        },
                        Decision::Far(p) => { counts.far += 1; counts.orders[p] += 1; }
                    }
                    profile.checkpoint(&counts);
                    let callback_started = profile.timer();
                    let result = apply_block(block, decision);
                    match decision { Decision::Far(_) => profile.add_time(1, callback_started), Decision::Near => profile.add_time(2, callback_started), Decision::Zero | Decision::Negligible => {} }
                    result?;
                }
                profile.checkpoint(&counts);
                profile.finish();
            } else {
                self.walk(Some(axis), apply_block)?;
            }
            replay_near_window(pair,space,q,&mut near,&mut pending)?;
            let mut downward_profile = Profile::new("downward", Some(axis));
            for parent in 0..t.nodes.len() {
                if let Some(children) = t.nodes[parent].children {
                    for child in children {
                        tails[child] = tails[child].add(tails[parent]);
                        if self.local_order[parent] != usize::MAX {
                            let shifted = t.shifted_required(
                                &local.read(parent, &t.harmonic),
                                parent,
                                child,
                                true,
                                self.local_order[parent],
                            );
                            local.add(child, &t.harmonic, &shifted)?;
                        }
                    }
                }
            }
            downward_profile.finish();
            let mut recovery_profile = Profile::new("leaf-recovery", Some(axis));
            for (node, shape) in t.nodes.iter().enumerate() {
                if shape.children.is_some() {
                    continue;
                }
                let i = t.permutation[shape.range[0]];
                let target = self
                    .geometry
                    .leaf
                    .read(self.geometry.leaf_index[node], &t.harmonic);
                let values = local.read(node, &t.harmonic);
                let cap = self.local_order[node];
                let far = if cap == usize::MAX {
                    Interval::zero()
                } else {
                    sum(t
                        .harmonic
                        .list
                        .iter()
                        .enumerate()
                        .take_while(|(_, a)| degree(**a) <= cap)
                        .map(|(k, _)| values[k].mul(target[k])))
                };
                let factor =
                    Interval::point(1e-7).mul(Interval::point(space.cells[i].density_per_amp_m2));
                let value = far.mul(factor).add(near[3 * i + axis]);
                let mid = value.mid();
                let bound = Interval::point(value.radius(mid))
                    .add(factor.mul(target[0]).mul(tails[node]))
                    .hi;
                if !value.finite() || !mid.is_finite() || !bound.is_finite() || bound > goal {
                    return Err(error("HIERARCHICAL_MAGNETIC_ACCURACY","action interval/roundoff enclosure exceeds requested absolute goal; no response admitted"));
                }
                action[3 * i + axis] = mid;
                bounds[3 * i + axis] = bound.max(0.);
            }
            recovery_profile.finish();
            axis_profile.finish();
        }
        Ok((action, bounds))
    }
    fn evidence(&self, goals: &[f64]) -> serde_json::Value {
        let mut mh = vec![0usize; self.tree.powers.order + 1];
        let mut lh = mh.clone();
        let mut shifts = 0usize;
        for (&m, &l) in self.moment_order.iter().zip(&self.local_order) {
            if m != usize::MAX {
                mh[m] += 1;
            }
            if l != usize::MAX {
                lh[l] += 1;
            }
        }
        for parent in 0..self.tree.nodes.len() {
            if let Some(children) = self.tree.nodes[parent].children {
                for child in children {
                    let m = self.moment_order[parent];
                    if m != usize::MAX && self.tv[child].hi > 0. {
                        shifts += 3 * self.tree.shift_prefix.iter().map(|v| v[m]).sum::<usize>();
                    }
                    let l = self.local_order[parent];
                    if l != usize::MAX {
                        shifts += 3 * self.tree.shift_prefix.iter().map(|v| v[l]).sum::<usize>();
                    }
                }
            }
        }
        serde_json::json!({"coverSignature":self.cover_signature,"coverCacheEnabled":self.cover_cached,"coverCacheEntries":self.cover.len(),"coverCacheBytes":self.cover.capacity()*std::mem::size_of::<(Pair,Decision)>(),"envelopeSignature":self.envelope_signature,"modelRevision":1,"rhsColumns":goals.len(),"absoluteActionGoals":goals,"chosenOrderHistogram":self.counts.orders,"visitedNodePairs":self.counts.visited,"farClusterPairs":self.counts.far,"nearTiles":self.counts.near,"exactPairs":self.counts.exact,"zeroBlocks":self.counts.zero,"boundedOmittedBlocks":self.counts.negligible,"nearBoundEnabled":self.near_bound,"nearBoundPolicy":"summed-per-tetra-volume-v1","activeEnvelopeTerminals":self.active_terminals,"maximumMomentDegreeHistogram":mh,"maximumLocalDegreeHistogram":lh,"m2lScalarContractionsPerAction":self.counts.m2l,"shiftStencilApplicationsPerAction":shifts})
    }
}

pub fn resistive_goals(
    space: &Space,
    columns: &[Vec<f64>],
    angular_frequency: f64,
) -> Result<Vec<f64>> {
    if !angular_frequency.is_finite() || angular_frequency <= 0. {
        return Err(error(
            "HIERARCHICAL_MAGNETIC_GOAL",
            "positive finite angular frequency required",
        ));
    }
    columns
        .iter()
        .map(|q| {
            if q.len() != space.current_count() || q.iter().any(|v| !v.is_finite()) {
                return Err(error(
                    "MAGNETIC_INPUT",
                    "complete original current column required",
                ));
            }
            if q.iter().all(|v| *v == 0.) {
                return Ok(1.);
            }
            let scale = q
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    abs_lower(
                        Interval::point(space.cells[i / 3].resistance_ohm).mul(Interval::point(v)),
                    )
                })
                .fold(0f64, f64::max);
            let goal = Interval::point(1e-9)
                .mul(Interval::point(scale))
                .div(Interval::point(angular_frequency))
                .lo;
            if !goal.is_finite() || goal <= 0. {
                return Err(error(
                    "HIERARCHICAL_MAGNETIC_GOAL",
                    "positive resistive normalization lower bound unavailable",
                ));
            }
            Ok(goal)
        })
        .collect()
}

#[cfg(test)]
mod classification_power_tests {
    use super::*;
    use crate::extraction::local::volume_current;

    #[test]
    fn profile_policy_accepts_only_enabled_switches() {
        use std::ffi::OsStr;
        assert!(!profile_enabled(None, None));
        assert!(!profile_enabled(Some(OsStr::new("0")), Some(OsStr::new("other"))));
        assert!(profile_enabled(Some(OsStr::new("1")), None));
        assert!(profile_enabled(None, Some(OsStr::new("1"))));
        assert!(profile_enabled(Some(OsStr::new("1")), Some(OsStr::new("1"))));
    }

    #[test]
    fn zero_pair_skip_matches_full_near_action_and_reduces_calls() {
        use std::collections::BTreeMap;
        let start=Instant::now();let wall=Duration::from_secs(30);
        let tetra=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        let geometry:Vec<_>=(0..3).map(|i|tetra.map(|p|[p[0]+i as f64*0.002,p[1],p[2]])).collect();
        let space=Space {mesh_nodes:vec![vec![];4],node_potential:BTreeMap::new(),cells:(0..3).map(|i|volume_current::Cell {mesh_cell:i,potentials:[0,1,2,3],incidence:[[0.;3];4],density_per_amp_m2:1.,resistance_ohm:1.}).collect(),components:vec![vec![0,1,2,3]],planned_bytes:0};
        let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:100,max_tetrahedra:100};
        let tree=Operator::new(&geometry,4,1,0.2,0,budget,start,wall).unwrap();
        let (shape,_)=tree.ensure_geometry(&geometry).unwrap();
        let columns=vec![vec![1.,0.,0.,0.,0.,0.,0.,0.,0.],vec![1.;9]];
        let plan=Plan::new_with_policy(&tree,&shape,&space,&columns,&[1e-30;2],false).unwrap();
        let mut pair=BoundedPairs::new(geometry,12,4,start,wall).unwrap();
        pair.trace=Some(std::cell::RefCell::new(super::super::super::PairTrace {counts:[0;3],seconds:[0.;3],last:Instant::now(),hits:0,misses:0,evictions:0}));
        let reference=plan.apply_with_zero_policy(&pair,&space,&columns[0],1e-30,false).unwrap();
        let count=|p:&BoundedPairs|p.trace.as_ref().unwrap().borrow().counts.iter().sum::<u64>();
        let before=count(&pair);
        let actual=plan.apply_with_zero_policy(&pair,&space,&columns[0],1e-30,true).unwrap();
        let optimized=count(&pair)-before;
        assert!(optimized < before && optimized > 0, "reference={before} optimized={optimized}");
        assert_eq!(reference,actual);
        // Tiny but nonzero current must retain its interactions.
        let mut tiny=columns[0].clone();tiny[3]=1e-200;
        let before_tiny=count(&pair);
        let tiny_action=plan.apply_with_zero_policy(&pair,&space,&tiny,1e-30,true).unwrap();
        let tiny_count=count(&pair)-before_tiny;
        let tiny_reference=plan.apply_with_zero_policy(&pair,&space,&tiny,1e-30,false).unwrap();
        assert!(tiny_count>optimized && tiny_count<=before);
        assert_eq!(tiny_action,tiny_reference);
    }

    #[test]
    fn parallel_near_replay_preserves_actions_bounds_and_cover_bits() {
        use std::collections::BTreeMap;
        let start=Instant::now();let wall=Duration::from_secs(60);
        let tetra=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        // Overlap forces >128 near pairs; the distant cell also exercises far work.
        let mut geometry=vec![tetra;16];geometry.push(tetra.map(|p|[p[0]+1.,p[1],p[2]]));
        let space=Space {mesh_nodes:vec![vec![];4],node_potential:BTreeMap::new(),cells:(0..geometry.len()).map(|i|volume_current::Cell {mesh_cell:i,potentials:[0,1,2,3],incidence:[[0.;3];4],density_per_amp_m2:1e6+i as f64*1e4,resistance_ohm:1.}).collect(),components:vec![vec![0,1,2,3]],planned_bytes:0};
        let budget=AllocationBudget{max_bytes:128*1024*1024,max_nodes:1000,max_tetrahedra:1000};
        let tree=Operator::new(&geometry,4,1,0.2,0,budget,start,wall).unwrap();
        let columns:Vec<Vec<f64>>=(0..2).map(|s|(0..3*geometry.len()).map(|k|if (k+s)%2==0 {1.+k as f64*0.01}else{-0.3-k as f64*0.005}).collect()).collect();
        let goals=[1e-20;2];
        let (shape, _) = tree.ensure_geometry(&geometry).unwrap();
        let mut saw_far = false;
        let mut saw_none = false;
        for near_bound in [false, true] {
            for goal in [1e-20, 1e20] {
                let plan = Plan::new_with_policy(&tree, &shape, &space, &columns, &[goal; 2], near_bound).unwrap();
                for a in 0..tree.nodes.len() {
                    for b in 0..tree.nodes.len() {
                        let pair = Pair { a, b };
                        let actual = plan.decide(pair);
                        assert_eq!(actual, plan.decide_reference(pair));
                        saw_far |= matches!(actual, Some(Decision::Far(_)));
                        saw_none |= actual.is_none();
                    }
                }
            }
        }
        assert!(saw_far && saw_none);
        let mut pair=BoundedPairs::new(geometry,12,8,start,wall).unwrap();
        let (expected,expected_bounds,mut expected_cert)=tree.batch_common(&pair,&space,&columns,&goals).unwrap();
        assert!(expected_cert["exactPairs"].as_u64().unwrap()>=128);
        for key in ["actionWallSeconds","geometryMomentPreparationSeconds","planPreparationSeconds"] {expected_cert.as_object_mut().unwrap().remove(key);}
        for workers in [2,4] {
            budget.check(BoundedPairs::parallel_bytes(workers).unwrap()).unwrap();pair.configure_parallel(workers).unwrap();
            let (actual,bounds,mut cert)=tree.batch_common(&pair,&space,&columns,&goals).unwrap();
            assert_eq!(expected.iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>(),actual.iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>());
            assert_eq!(expected_bounds.iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>(),bounds.iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>());
            for key in ["actionWallSeconds","geometryMomentPreparationSeconds","planPreparationSeconds"] {cert.as_object_mut().unwrap().remove(key);}
            assert_eq!(expected_cert,cert);
        }
    }

    #[test]
    fn negligible_duplicate_tetra_bounds_enclose_unpruned_actions() {
        use std::collections::BTreeMap;
        let start = Instant::now();
        let wall = Duration::from_secs(120);
        let tetra = [[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        let geometry = vec![tetra, tetra];
        let cell = |i| volume_current::Cell { mesh_cell: i, potentials: [0,1,2,3], incidence: [[0.;3];4], density_per_amp_m2: 1e6, resistance_ohm: 1. };
        let space = Space { mesh_nodes: vec![vec![];4], node_potential: BTreeMap::new(), cells: vec![cell(0),cell(1)], components: vec![vec![0,1,2,3]], planned_bytes: 0 };
        let budget = AllocationBudget { max_bytes: 64*1024*1024, max_nodes: 1000, max_tetrahedra: 1000 };
        let tree = Operator::new(&geometry, 4, 1, 0.2, 0, budget, start, wall).unwrap();
        let pair = BoundedPairs::new_with_scalar(geometry.clone(), 12, 4, 4, start, wall).unwrap();
        let (shape, _) = tree.ensure_geometry(&geometry).unwrap();
        let columns = vec![vec![1.,-0.5,0.25,0.5,0.25,-0.125]];
        let goal = 1.;
        let mut plan = Plan::new_with_policy(&tree, &shape, &space, &columns, &[goal], true).unwrap();
        for a in 0..tree.nodes.len() {
            let budget = Interval::point(0.25).mul(Interval::point(plan.mass[a].lo)).div(Interval::point(plan.mass[0].hi)).lo;
            let weight = Interval::point(1e-7).mul(Interval::point(plan.max_weight[a]));
            assert_eq!(plan.node_budget[a].to_bits(), budget.to_bits());
            assert_eq!(plan.node_weight[a].lo.to_bits(), weight.lo.to_bits());
            assert_eq!(plan.node_weight[a].hi.to_bits(), weight.hi.to_bits());
            for b in a..tree.nodes.len() {
                let pair = Pair { a, b };
                assert_eq!(plan.decide(pair), plan.decide_reference(pair));
            }
        }
        plan.cover_cached = false;
        plan.near_bound = false;
        let (reference, _) = plan.apply(&pair, &space, &columns[0], goal).unwrap();
        assert!(reference.iter().any(|v|v.abs()>0.));
        plan.near_bound = true;
        assert!(matches!(plan.decide(Pair { a: 0, b: 0 }), Some(Decision::Negligible)));
        let (action, bounds) = plan.apply(&pair, &space, &columns[0], goal).unwrap();
        for i in 0..action.len() { assert!((action[i]-reference[i]).abs() <= bounds[i]); }
        let mut cover = Vec::new();
        plan.walk(None, |pair, decision| { cover.push((pair, decision)); Ok(()) }).unwrap();
        plan.cover = cover;
        plan.cover_cached = true;
        let (cached_action, cached_bounds) = plan.apply(&pair, &space, &columns[0], goal).unwrap();
        assert_eq!(cached_action, action);
        assert_eq!(cached_bounds, bounds);
        plan.phi[0] = Interval { lo: 0., hi: f64::INFINITY };
        assert!(!matches!(plan.decide(Pair { a: 0, b: 0 }), Some(Decision::Negligible)));
        let mut strong = Plan::new_with_policy(&tree, &shape, &space, &columns, &[1e-30], true).unwrap();
        strong.near_bound = true;
        assert!(!matches!(strong.decide(Pair { a: 0, b: 0 }), Some(Decision::Negligible)));
    }

    #[test]
    #[ignore = "requires P7_MAGNETIC_NEAR_BOUND=1 in an isolated process"]
    fn optin_chunked_omission_provenance() {
        use std::collections::BTreeMap;
        assert_eq!(std::env::var("P7_MAGNETIC_NEAR_BOUND").unwrap(),"1");
        let tetra=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        let geometry=vec![tetra,tetra];let start=Instant::now();let wall=Duration::from_secs(30);
        let cell=|i|volume_current::Cell{mesh_cell:i,potentials:[0,1,2,3],incidence:[[0.;3];4],density_per_amp_m2:1e6,resistance_ohm:1.};
        let space=Space{mesh_nodes:vec![vec![];4],node_potential:BTreeMap::new(),cells:vec![cell(0),cell(1)],components:vec![vec![0,1,2,3]],planned_bytes:0};
        let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:1000,max_tetrahedra:1000};
        let tree=Operator::new(&geometry,4,1,0.2,0,budget,start,wall).unwrap();
        let pair=BoundedPairs::new_with_scalar(geometry,12,4,0,start,wall).unwrap();
        let columns=vec![vec![1.,0.,0.,0.5,0.,0.],vec![0.1,0.,0.,0.05,0.,0.]];
        let (actions,bounds,certificate)=tree.batch_chunked(&pair,&space,&columns,&[1.,1.],1).unwrap();
        assert!(actions.iter().flatten().all(|v|*v==0.));
        assert!(bounds.iter().flatten().all(|v|*v>0.&&*v<=1.));
        assert_eq!(certificate["boundedOmittedBlocks"],2);
        assert_eq!(certificate["nearBoundEnabled"],true);
        assert_eq!(certificate["nearBoundPolicy"],"summed-per-tetra-volume-v1");
        assert_eq!(certificate["zeroBlocks"],0);
    }

    #[test]
    fn incremental_classification_power_matches_fold_bits() {
        for (lo, hi) in [(0., 0.), (0., 1e-12), (0.01, 0.02), (0.19, 0.21),
            (0.49, 0.51), (0.99, 0.999999999999)] {
            let q = Interval { lo, hi };
            let mut power = Interval::one();
            for degree in 1..=13 {
                power = power.mul(q);
                let reference = q.pow(degree);
                assert_eq!(power.lo.to_bits(), reference.lo.to_bits());
                assert_eq!(power.hi.to_bits(), reference.hi.to_bits());
            }
        }
    }
}
