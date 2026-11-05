//! Goal-controlled common-batch Newton cover. Geometry and native current DOFs
//! are immutable; every omitted Taylor tail is allocated a positive source-mass
//! share. Numerical near/self kernels are never weak-pruned.
use super::*;
use std::cell::Ref;

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
    pub fn batch(
        &self,
        pair: &BoundedPairs,
        space: &Space,
        columns: &[Vec<f64>],
        goals: &[f64],
    ) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, serde_json::Value)> {
        let started = Instant::now();
        let (geometry, geometry_seconds) = self.ensure_geometry(&pair.tetra)?;
        let plan_started = Instant::now();
        let plan = Plan::new(self, &geometry, space, columns, goals)?;
        let plan_seconds = plan_started.elapsed().as_secs_f64();
        let mut actions = Vec::with_capacity(columns.len());
        let mut bounds = Vec::with_capacity(columns.len());
        let mut maxima = Vec::with_capacity(columns.len());
        for (column, &goal) in columns.iter().zip(goals) {
            let (action, bound) = plan.apply(pair, space, column, goal)?;
            maxima.push(bound.iter().copied().fold(0f64, f64::max));
            actions.push(action);
            bounds.push(bound);
        }
        let mut certificate = plan.evidence(goals);
        certificate["maximumAbsoluteErrorBounds"] = serde_json::json!(maxima);
        certificate["actionWallSeconds"] = serde_json::json!(started.elapsed().as_secs_f64());
        certificate["geometryMomentPreparationSeconds"] = serde_json::json!(geometry_seconds);
        certificate["planPreparationSeconds"] = serde_json::json!(plan_seconds);
        Ok((actions, bounds, certificate))
    }
}

#[derive(Clone, Copy)]
enum Decision {
    Zero,
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
    orders: Vec<usize>,
    m2l: usize,
}
struct Plan<'a> {
    tree: &'a Operator,
    geometry: &'a Geometry,
    mass: Vec<Interval>,
    max_weight: Vec<f64>,
    envelope: Vec<f64>,
    tv: Vec<Interval>,
    moment_order: Vec<usize>,
    local_order: Vec<usize>,
    counts: Counts,
    cover_signature: String,
    envelope_signature: String,
    active_terminals: usize,
}
impl<'a> Plan<'a> {
    fn new(
        tree: &'a Operator,
        geometry: &'a Geometry,
        space: &Space,
        columns: &[Vec<f64>],
        goals: &[f64],
    ) -> Result<Self> {
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
            envelope: vec![0.; n],
            tv: vec![Interval::zero(); nodes],
            moment_order: vec![usize::MAX; nodes],
            local_order: vec![usize::MAX; nodes],
            counts: Counts::default(),
            cover_signature: String::new(),
            envelope_signature: String::new(),
            active_terminals: 0,
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
        let mut moment = vec![usize::MAX; nodes];
        let mut local = moment.clone();
        let mut hash = 0xcbf29ce484222325u64;
        let counts = result.walk(|pair, decision| {
            add_hash(&mut hash, pair.a as u64);
            add_hash(&mut hash, pair.b as u64);
            match decision {
                Decision::Zero => add_hash(&mut hash, 0),
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
        result.cover_signature = format!("fnv1a64:{hash:016x}");
        Ok(result)
    }
    fn decide(&self, pair: Pair) -> Option<Decision> {
        let Pair { a, b } = pair;
        if self.tv[a].hi == 0. && self.tv[b].hi == 0. {
            return Some(Decision::Zero);
        }
        if a == b {
            return None;
        }
        let (r, q) = self.tree.separation(a, b);
        if !q.finite() || q.hi >= 1. || r.lo <= 0. {
            return None;
        }
        let budget_a = Interval::point(0.25)
            .mul(Interval::point(self.mass[a].lo))
            .div(Interval::point(self.mass[0].hi))
            .lo;
        let budget_b = Interval::point(0.25)
            .mul(Interval::point(self.mass[b].lo))
            .div(Interval::point(self.mass[0].hi))
            .lo;
        for p in 0..=self.tree.powers.order {
            let tail = Interval::one()
                .div(r)
                .mul(q.pow(p + 1))
                .div(Interval::one().sub(q));
            let a_error = Interval::point(1e-7)
                .mul(Interval::point(self.max_weight[a]))
                .mul(tail)
                .mul(self.tv[b]);
            let b_error = Interval::point(1e-7)
                .mul(Interval::point(self.max_weight[b]))
                .mul(tail)
                .mul(self.tv[a]);
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
    fn walk(&self, mut emit: impl FnMut(Pair, Decision) -> Result<()>) -> Result<Counts> {
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
            let decision = self.decide(pair).or_else(|| {
                if a.is_none() && b.is_none() {
                    Some(Decision::Near)
                } else {
                    None
                }
            });
            if let Some(decision) = decision {
                match decision {
                    Decision::Zero => counts.zero += 1,
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
                emit(pair, decision)?;
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
        }
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
        let t = self.tree;
        let n = space.cells.len();
        let rank = t.harmonic.list.len();
        if q.iter().all(|v| *v == 0.) {
            return Ok((vec![0.; 3 * n], vec![0.; 3 * n]));
        }
        let mut action = vec![0.; 3 * n];
        let mut bounds = vec![0.; 3 * n];
        let mut near = vec![Interval::zero(); 3 * n];
        for axis in 0..3 {
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
            self.walk(|block, decision| {
                match decision {
                    Decision::Zero => {}
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
                        let pa: Vec<_> = (0..=p).map(|d| sa.pow(d)).collect();
                        let pb: Vec<_> = (0..=p).map(|d| sb.pow(d)).collect();
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
                        let mut la = vec![Interval::zero(); rank];
                        let mut lb = la.clone();
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
            })?;
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
        serde_json::json!({"coverSignature":self.cover_signature,"envelopeSignature":self.envelope_signature,"modelRevision":1,"rhsColumns":goals.len(),"absoluteActionGoals":goals,"chosenOrderHistogram":self.counts.orders,"visitedNodePairs":self.counts.visited,"farClusterPairs":self.counts.far,"nearTiles":self.counts.near,"exactPairs":self.counts.exact,"zeroBlocks":self.counts.zero,"activeEnvelopeTerminals":self.active_terminals,"maximumMomentDegreeHistogram":mh,"maximumLocalDegreeHistogram":lh,"m2lScalarContractionsPerAction":self.counts.m2l,"shiftStencilApplicationsPerAction":shifts})
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
