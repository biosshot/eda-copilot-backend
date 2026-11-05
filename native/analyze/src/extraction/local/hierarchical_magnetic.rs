//! Reciprocal Cartesian Newton expansion with outward arithmetic and a
//! Legendre tail bound. Near/self retain the original numerical quadrature.
use super::{check_deadline, error, AllocationBudget, BoundedPairs, Result, Space};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(super) struct Interval {
    pub lo: f64,
    pub hi: f64,
}
fn up(v: f64) -> f64 {
    if v.is_nan() || v == f64::INFINITY {
        v
    } else if v == 0. {
        f64::from_bits(1)
    } else {
        f64::from_bits(if v > 0. {
            v.to_bits() + 1
        } else {
            v.to_bits() - 1
        })
    }
}
fn down(v: f64) -> f64 {
    -up(-v)
}
impl Interval {
    pub fn point(v: f64) -> Self {
        Self { lo: v, hi: v }
    }
    fn zero() -> Self {
        Self::point(0.)
    }
    fn one() -> Self {
        Self::point(1.)
    }
    fn finite(self) -> bool {
        self.lo.is_finite() && self.hi.is_finite() && self.lo <= self.hi
    }
    fn exact_zero(self) -> bool {
        self.lo == 0. && self.hi == 0.
    }
    pub fn add(self, b: Self) -> Self {
        if self.exact_zero() {
            return b;
        }
        if b.exact_zero() {
            return self;
        }
        Self {
            lo: down(self.lo + b.lo),
            hi: up(self.hi + b.hi),
        }
    }
    pub fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }
    pub fn sub(self, b: Self) -> Self {
        self.add(b.neg())
    }
    pub fn mul(self, b: Self) -> Self {
        if self.exact_zero() || b.exact_zero() {
            return Self::zero();
        }
        let values = [
            self.lo * b.lo,
            self.lo * b.hi,
            self.hi * b.lo,
            self.hi * b.hi,
        ];
        if values.iter().any(|v| v.is_nan()) {
            return Self {
                lo: f64::NEG_INFINITY,
                hi: f64::INFINITY,
            };
        }
        Self {
            lo: down(values.into_iter().fold(f64::INFINITY, f64::min)),
            hi: up(values.into_iter().fold(f64::NEG_INFINITY, f64::max)),
        }
    }
    fn div(self, b: Self) -> Self {
        if b.lo <= 0. && b.hi >= 0. {
            return Self {
                lo: f64::NEG_INFINITY,
                hi: f64::INFINITY,
            };
        }
        self.mul(Self {
            lo: down(1. / b.hi),
            hi: up(1. / b.lo),
        })
    }
    fn sqrt(self) -> Self {
        if self.hi < 0. {
            return Self {
                lo: f64::NAN,
                hi: f64::NAN,
            };
        }
        Self {
            lo: if self.lo <= 0. {
                0.
            } else {
                down(self.lo.sqrt())
            },
            hi: up(self.hi.sqrt()),
        }
    }
    fn pow(self, n: usize) -> Self {
        (0..n).fold(Self::one(), |a, _| a.mul(self))
    }
    pub fn mid(self) -> f64 {
        self.lo * 0.5 + self.hi * 0.5
    }
    pub fn radius(self, mid: f64) -> f64 {
        up((mid - self.lo).max(self.hi - mid).max(0.))
    }
}
fn sum(values: impl IntoIterator<Item = Interval>) -> Interval {
    values.into_iter().fold(Interval::zero(), Interval::add)
}
fn distance(a: [f64; 3], b: [f64; 3]) -> Interval {
    sum((0..3).map(|k| Interval::point(a[k]).sub(Interval::point(b[k])).pow(2))).sqrt()
}
fn factorial(n: usize) -> u64 {
    (1..=n as u64).product()
}
fn choose(n: usize, k: usize) -> f64 {
    (factorial(n) / (factorial(k) * factorial(n - k))) as f64
}
#[cfg(test)]
fn choose3(a: [usize; 3], b: [usize; 3]) -> f64 {
    (0..3).map(|k| choose(a[k], b[k])).product()
}
fn degree(a: [usize; 3]) -> usize {
    a.into_iter().sum()
}
fn plus(a: [usize; 3], b: [usize; 3]) -> [usize; 3] {
    std::array::from_fn(|k| a[k] + b[k])
}
#[derive(Clone)]
struct Powers {
    list: Vec<[usize; 3]>,
    index: Vec<usize>,
    order: usize,
}
impl Powers {
    fn new(order: usize) -> Self {
        let mut list = vec![];
        let mut index = vec![usize::MAX; (order + 1).pow(3)];
        for d in 0..=order {
            for x in 0..=d {
                for y in 0..=d - x {
                    let a = [x, y, d - x - y];
                    index[(a[0] * (order + 1) + a[1]) * (order + 1) + a[2]] = list.len();
                    list.push(a);
                }
            }
        }
        Self { list, index, order }
    }
    fn at(&self, a: [usize; 3]) -> usize {
        self.index[(a[0] * (self.order + 1) + a[1]) * (self.order + 1) + a[2]]
    }
}
/// Canonical differential polynomials modulo Laplace: Dz^2=-Dx^2-Dy^2.
/// Each homogeneous degree has2*l+1 real coefficients, (p+1)^2 in total.
struct Harmonic {
    list: Vec<[usize; 3]>,
    index: Vec<usize>,
    order: usize,
}
impl Harmonic {
    fn new(powers: &Powers) -> Self {
        let list: Vec<_> = powers.list.iter().copied().filter(|a| a[2] <= 1).collect();
        let mut index = vec![usize::MAX; (powers.order + 1).pow(3)];
        for (i, &a) in list.iter().enumerate() {
            index[(a[0] * (powers.order + 1) + a[1]) * (powers.order + 1) + a[2]] = i;
        }
        Self {
            list,
            index,
            order: powers.order,
        }
    }
    fn at(&self, a: [usize; 3]) -> usize {
        self.index[(a[0] * (self.order + 1) + a[1]) * (self.order + 1) + a[2]]
    }
}
#[derive(Clone, Copy)]
struct Shift {
    source: usize,
    target: usize,
    power: usize,
    weight: Interval,
}
#[derive(Clone, Copy)]
struct Coupling {
    target: usize,
    source: usize,
    jet: usize,
    source_degree: usize,
}
/// Centers with a common outward error radius per homogeneous shell. Reads
/// reconstruct intervals; all coefficients of an update are frozen before
/// writing any shell radius, avoiding in-place uncertainty amplification.
struct Packed {
    centers: Vec<f64>,
    radii: Vec<f64>,
    rank: usize,
    shells: usize,
}
impl Packed {
    fn new(nodes: usize, basis: &Harmonic) -> Self {
        Self {
            centers: vec![0.; nodes * basis.list.len()],
            radii: vec![0.; nodes * (basis.order + 1)],
            rank: basis.list.len(),
            shells: basis.order + 1,
        }
    }
    fn read(&self, node: usize, basis: &Harmonic) -> Vec<Interval> {
        basis
            .list
            .iter()
            .enumerate()
            .map(|(i, &a)| {
                let c = self.centers[node * self.rank + i];
                let r = self.radii[node * self.shells + degree(a)];
                if r == 0. {
                    Interval::point(c)
                } else {
                    Interval {
                        lo: down(c - r),
                        hi: up(c + r),
                    }
                }
            })
            .collect()
    }
    fn write(&mut self, node: usize, basis: &Harmonic, values: &[Interval]) -> Result<()> {
        let mut radius = vec![0f64; self.shells];
        for (i, (&a, &value)) in basis.list.iter().zip(values).enumerate() {
            if !value.finite() {
                return Err(error(
                    "HIERARCHICAL_MAGNETIC_INTERVAL",
                    "packed harmonic shell overflow",
                ));
            }
            let center = value.mid();
            if !center.is_finite() {
                return Err(error(
                    "HIERARCHICAL_MAGNETIC_INTERVAL",
                    "packed harmonic center overflow",
                ));
            }
            self.centers[node * self.rank + i] = center;
            radius[degree(a)] = radius[degree(a)].max(if value.exact_zero() {
                0.
            } else {
                value.radius(center)
            });
        }
        self.radii[node * self.shells..(node + 1) * self.shells].copy_from_slice(&radius);
        Ok(())
    }
    fn add(&mut self, node: usize, basis: &Harmonic, delta: &[Interval]) -> Result<()> {
        let old = self.read(node, basis);
        let values: Vec<_> = old.into_iter().zip(delta).map(|(v, &d)| v.add(d)).collect();
        self.write(node, basis, &values)
    }
}

#[derive(Clone)]
struct Node {
    center: [f64; 3],
    radius: f64,
    range: [usize; 2],
    children: Option<[usize; 2]>,
    terminal_bounds: [[f64; 3]; 2],
    minimum_terminal_radius: f64,
    depth: usize,
}
#[derive(Clone, Copy)]
struct Pair {
    a: usize,
    b: usize,
}
#[derive(Default)]
struct InteractionCounts {
    visited: usize,
    far: usize,
    near: usize,
    exact: usize,
    far_by_depth: Vec<usize>,
    near_by_depth: Vec<usize>,
    exact_by_depth: Vec<usize>,
}
pub(super) struct Operator {
    nodes: Vec<Node>,
    permutation: Vec<usize>,
    interactions: InteractionCounts,
    traversal_capacity: usize,
    traversal_levels: usize,
    radius_histogram: [usize; 64],
    minimum_radius: f64,
    maximum_radius: f64,
    powers: Powers,
    harmonic: Harmonic,
    shifts: [Vec<Shift>; 3],
    couplings: Vec<Coupling>,
    pub planned_bytes: usize,
    pub exact_pairs: usize,
    signature: String,
    leaf: usize,
    qmax: f64,
    start: Instant,
    wall: Duration,
}
impl Operator {
    pub fn new(
        tetra: &[[[f64; 3]; 4]],
        order: usize,
        leaf: usize,
        qmax: f64,
        base: usize,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<Self> {
        check_deadline(start, wall)?;
        if tetra.is_empty()
            || !(2..=12).contains(&order)
            || leaf == 0
            || !qmax.is_finite()
            || qmax <= 0.
            || qmax >= 0.5
        {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_POLICY",
                "bounded order2..12, positive leaf size and0<qmax<0.5 required",
            ));
        }
        for cell in tetra {
            check_deadline(start, wall)?;
            if cell.iter().flatten().any(|v| !v.is_finite()) {
                return Err(error(
                    "HIERARCHICAL_MAGNETIC_GEOMETRY",
                    "finite original tetra coordinates required",
                ));
            }
        }
        let n = tetra.len();
        let rank = (order + 1) * (order + 2) * (order + 3) / 6;
        let harmonic_rank = (order + 1) * (order + 1);
        let minimum = (leaf / 2).max(1);
        let node_bound = n
            .div_ceil(minimum)
            .checked_mul(2)
            .ok_or_else(|| error("MEMORY_LIMIT", "magnetic node bound overflow"))?;
        let mut shift_counts = [0usize; 3];
        let mut coupling_count = 0usize;
        for degree_g in 0..=order {
            for gx in 0..=degree_g {
                for gy in 0..=degree_g - gx {
                    let g = [gx, gy, degree_g - gx - gy];
                    if g[2] > 1 {
                        continue;
                    }
                    for k in 0..=order - degree_g {
                        shift_counts[0] += 1;
                        shift_counts[1] += 1;
                        shift_counts[2] += (g[2] + k) / 2 + 1;
                    }
                    for degree_h in 0..=order - degree_g {
                        coupling_count += 2 * degree_h + 1;
                    }
                }
            }
        }
        let stencil_bytes = shift_counts
            .into_iter()
            .sum::<usize>()
            .checked_mul(std::mem::size_of::<Shift>())
            .and_then(|v| {
                v.checked_add(coupling_count.checked_mul(std::mem::size_of::<Coupling>())?)
            })
            .ok_or_else(|| error("MEMORY_LIMIT", "harmonic stencil count overflow"))?;
        let depth = (usize::BITS - (n - 1).leading_zeros()) as usize;
        let traversal_levels = depth + 1;
        let traversal_capacity = 4 * depth + 4;
        let traversal_bytes = traversal_capacity * std::mem::size_of::<Pair>()
            + 6 * traversal_levels * std::mem::size_of::<usize>()
            + 64 * std::mem::size_of::<usize>();
        let work = node_bound
            .checked_mul(2 * (harmonic_rank + order + 1) * std::mem::size_of::<f64>())
            .and_then(|v| v.checked_add(node_bound.checked_mul(256)?))
            .and_then(|v| v.checked_add(n.checked_mul(128)?))
            .and_then(|v| v.checked_add(rank.checked_mul(512)?))
            .and_then(|v| v.checked_add(stencil_bytes))
            .and_then(|v| v.checked_add(traversal_bytes))
            .ok_or_else(|| error("MEMORY_LIMIT", "packed harmonic work plan overflow"))?;
        budget.check(
            base.checked_add(work)
                .ok_or_else(|| error("MEMORY_LIMIT", "harmonic base overflow"))?,
        )?;
        let powers = Powers::new(order);
        let harmonic = Harmonic::new(&powers);
        let mut shifts: [Vec<Shift>; 3] =
            std::array::from_fn(|axis| Vec::with_capacity(shift_counts[axis]));
        for (source, &g) in harmonic.list.iter().enumerate() {
            for axis in 0..3 {
                for k in 0..=order - degree(g) {
                    let mut raw = g;
                    raw[axis] += k;
                    let half = raw[2] / 2;
                    for j in 0..=half {
                        let target =
                            harmonic.at([raw[0] + 2 * j, raw[1] + 2 * (half - j), raw[2] % 2]);
                        let sign = if half % 2 == 0 { 1. } else { -1. };
                        let weight = Interval::point(sign * choose(half, j))
                            .div(Interval::point(factorial(k) as f64));
                        shifts[axis].push(Shift {
                            source,
                            target,
                            power: k,
                            weight,
                        });
                    }
                }
            }
        }
        let mut couplings = Vec::with_capacity(coupling_count);
        for (target, &h) in harmonic.list.iter().enumerate() {
            for (source, &g) in harmonic.list.iter().enumerate() {
                if degree(h) + degree(g) <= order {
                    couplings.push(Coupling {
                        target,
                        source,
                        jet: powers.at(plus(h, g)),
                        source_degree: degree(g),
                    });
                }
            }
        }
        let mut result = Self {
            nodes: Vec::with_capacity(node_bound),
            permutation: (0..n).collect(),
            interactions: InteractionCounts::default(),
            traversal_capacity,
            traversal_levels,
            radius_histogram: [0; 64],
            minimum_radius: f64::INFINITY,
            maximum_radius: 0.,
            powers,
            harmonic,
            shifts,
            couplings,
            planned_bytes: work,
            exact_pairs: 0,
            signature: String::new(),
            leaf,
            qmax,
            start,
            wall,
        };
        result.build(tetra, 0, n, 0)?;
        for node in &result.nodes {
            if node.children.is_none() {
                result.minimum_radius = result.minimum_radius.min(node.radius);
                result.maximum_radius = result.maximum_radius.max(node.radius);
                let bin = (node.radius.log2().floor() as i32 + 48).clamp(0, 63) as usize;
                result.radius_histogram[bin] += 1;
            }
        }
        let mut hash = 0xcbf29ce484222325u64;
        let mut stamp = |v: u64| {
            for b in v.to_le_bytes() {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        };
        stamp(3); // immutable streamed interaction representation; harmonic basis remains2
        stamp(order as u64);
        stamp(leaf as u64);
        stamp(qmax.to_bits());
        for t in tetra {
            for p in t {
                for v in p {
                    stamp(v.to_bits());
                }
            }
        }
        for &i in &result.permutation {
            stamp(i as u64);
        }
        for node in &result.nodes {
            for coordinate in node.center {
                stamp(coordinate.to_bits());
            }
            stamp(node.radius.to_bits());
            stamp(node.range[0] as u64);
            stamp(node.range[1] as u64);
        }
        // A rolling stream stamp binds the immutable pair cover without
        // retaining or serializing millions of descriptors.
        let counts = result.walk(|pair, far| {
            stamp(u64::from(far));
            stamp(pair.a as u64);
            stamp(pair.b as u64);
            Ok(())
        })?;
        result.exact_pairs = counts.exact;
        result.interactions = counts;
        result.signature = format!("fnv1a64:{hash:016x}");
        budget.check(
            base.checked_add(result.planned_bytes)
                .ok_or_else(|| error("MEMORY_LIMIT", "hierarchical magnetic total overflow"))?,
        )?;
        Ok(result)
    }
    fn build(
        &mut self,
        tetra: &[[[f64; 3]; 4]],
        begin: usize,
        end: usize,
        depth: usize,
    ) -> Result<usize> {
        check_deadline(self.start, self.wall)?;
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for &i in &self.permutation[begin..end] {
            for vertex in tetra[i] {
                for k in 0..3 {
                    low[k] = low[k].min(vertex[k]);
                    high[k] = high[k].max(vertex[k]);
                }
            }
        }
        let center = std::array::from_fn(|k| low[k] * 0.5 + high[k] * 0.5);
        let mut radius = 0f64;
        for &i in &self.permutation[begin..end] {
            for vertex in tetra[i] {
                radius = radius.max(distance(vertex, center).hi);
            }
        }
        if !radius.is_finite() || radius <= 0. {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_GEOMETRY",
                "finite positive cluster radius required",
            ));
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            center,
            radius,
            range: [begin, end],
            children: None,
            terminal_bounds: [center, center],
            minimum_terminal_radius: radius,
            depth,
        });
        if end - begin > self.leaf {
            let mut axis = 0;
            for k in 1..3 {
                if high[k] - low[k] > high[axis] - low[axis] {
                    axis = k;
                }
            }
            self.permutation[begin..end].sort_unstable_by(|&i, &j| {
                let center = |id: usize| tetra[id].iter().map(|p| p[axis] * 0.25).sum::<f64>();
                center(i).total_cmp(&center(j)).then(i.cmp(&j))
            });
            let middle = begin + (end - begin) / 2;
            let left = self.build(tetra, begin, middle, depth + 1)?;
            let right = self.build(tetra, middle, end, depth + 1)?;
            self.nodes[index].children = Some([left, right]);
            self.nodes[index].terminal_bounds = std::array::from_fn(|side| {
                std::array::from_fn(|k| {
                    let a = self.nodes[left].terminal_bounds[side][k];
                    let b = self.nodes[right].terminal_bounds[side][k];
                    if side == 0 {
                        a.min(b)
                    } else {
                        a.max(b)
                    }
                })
            });
            self.nodes[index].minimum_terminal_radius = self.nodes[left]
                .minimum_terminal_radius
                .min(self.nodes[right].minimum_terminal_radius);
        }
        Ok(index)
    }
    fn separation(&self, a: usize, b: usize) -> (Interval, Interval) {
        let a = &self.nodes[a];
        let b = &self.nodes[b];
        let r = distance(a.center, b.center);
        let q = Interval::point(a.radius)
            .add(Interval::point(b.radius))
            .div(r);
        (r, q)
    }
    fn all_terminal_near(&self, a: usize, b: usize) -> bool {
        let a = &self.nodes[a];
        let b = &self.nodes[b];
        let mut squared = Interval::zero();
        for axis in 0..3 {
            let first = Interval::point(a.terminal_bounds[1][axis])
                .sub(Interval::point(b.terminal_bounds[0][axis]));
            let second = Interval::point(b.terminal_bounds[1][axis])
                .sub(Interval::point(a.terminal_bounds[0][axis]));
            let maximum = first
                .lo
                .abs()
                .max(first.hi.abs())
                .max(second.lo.abs())
                .max(second.hi.abs());
            squared = squared.add(Interval::point(maximum).pow(2));
        }
        let maximum_distance = squared.sqrt();
        let minimum_radii = Interval::point(a.minimum_terminal_radius)
            .add(Interval::point(b.minimum_terminal_radius));
        // This only chooses exact integration. No descendantterminal pair can
        // pass qmax under this conservative separation lower bound; even a
        // missed coarser far opportunity would add cost, never physical error.
        minimum_radii.lo > Interval::point(self.qmax).mul(maximum_distance).hi
    }
    /// Deterministic DFS on immutable geometry. No interaction descriptor is
    /// retained. All originalpairs remain covered exactly once; each action
    /// replays this same geometry-only decision stream for each Cartesianaxis.
    fn walk(&self, mut emit: impl FnMut(Pair, bool) -> Result<()>) -> Result<InteractionCounts> {
        let mut counts = InteractionCounts {
            far_by_depth: vec![0; self.traversal_levels],
            near_by_depth: vec![0; self.traversal_levels],
            exact_by_depth: vec![0; self.traversal_levels],
            ..Default::default()
        };
        let mut pending = Vec::with_capacity(self.traversal_capacity);
        pending.push(Pair { a: 0, b: 0 });
        while let Some(pair) = pending.pop() {
            if let Err(failure) = check_deadline(self.start, self.wall) {
                return Err(format!("{failure}; streamed counts visitedNodePairs={}, farClusterPairs={}, nearTiles={}, exactPairs={}",counts.visited,counts.far,counts.near,counts.exact));
            }
            counts.visited = counts
                .visited
                .checked_add(1)
                .ok_or_else(|| error("MEMORY_LIMIT", "visited interaction count overflow"))?;
            let Pair { a, b } = pair;
            let ac = self.nodes[a].children;
            let bc = self.nodes[b].children;
            let depth = self.nodes[a].depth.max(self.nodes[b].depth);
            let far = if a != b {
                let (_, q) = self.separation(a, b);
                q.finite() && q.hi <= self.qmax
            } else {
                false
            };
            if far {
                counts.far = counts
                    .far
                    .checked_add(1)
                    .ok_or_else(|| error("MEMORY_LIMIT", "far interaction count overflow"))?;
                counts.far_by_depth[depth] += 1;
                emit(pair, true)?;
                continue;
            }
            if ac.is_none() && bc.is_none() || self.all_terminal_near(a, b) {
                let na = self.nodes[a].range[1] - self.nodes[a].range[0];
                let nb = self.nodes[b].range[1] - self.nodes[b].range[0];
                let exact = if a == b {
                    na.checked_mul(na + 1).map(|v| v / 2)
                } else {
                    na.checked_mul(nb)
                }
                .ok_or_else(|| error("MEMORY_LIMIT", "exact pair count overflow"))?;
                counts.near = counts
                    .near
                    .checked_add(1)
                    .ok_or_else(|| error("MEMORY_LIMIT", "near tile count overflow"))?;
                counts.exact = counts
                    .exact
                    .checked_add(exact)
                    .ok_or_else(|| error("MEMORY_LIMIT", "exact pair count overflow"))?;
                counts.near_by_depth[depth] += 1;
                counts.exact_by_depth[depth] = counts.exact_by_depth[depth]
                    .checked_add(exact)
                    .ok_or_else(|| error("MEMORY_LIMIT", "depth exact pair count overflow"))?;
                emit(pair, false)?;
                continue;
            }
            // Push in reverse original DFS order. A diagonal split adds2live
            // frames while advancing both depths; 4D+4 slots bound all paths.
            if a == b {
                let [l, r] = ac.unwrap();
                if pending.len() + 3 > self.traversal_capacity {
                    return Err(error(
                        "MEMORY_LIMIT",
                        "immutable interaction stack capacity exhausted",
                    ));
                }
                pending.extend_from_slice(&[
                    Pair { a: r, b: r },
                    Pair { a: l, b: r },
                    Pair { a: l, b: l },
                ]);
            } else if ac.is_some() && (bc.is_none() || self.nodes[a].radius >= self.nodes[b].radius)
            {
                let [l, r] = ac.unwrap();
                if pending.len() + 2 > self.traversal_capacity {
                    return Err(error(
                        "MEMORY_LIMIT",
                        "immutable interaction stack capacity exhausted",
                    ));
                }
                pending.extend_from_slice(&[Pair { a: r, b }, Pair { a: l, b }]);
            } else {
                let [l, r] = bc.unwrap();
                if pending.len() + 2 > self.traversal_capacity {
                    return Err(error(
                        "MEMORY_LIMIT",
                        "immutable interaction stack capacity exhausted",
                    ));
                }
                pending.extend_from_slice(&[Pair { a, b: r }, Pair { a, b: l }]);
            }
        }
        Ok(counts)
    }
    fn offset(&self, parent: usize, child: usize) -> ([Interval; 3], Interval) {
        let p = &self.nodes[parent];
        let c = &self.nodes[child];
        (
            std::array::from_fn(|k| {
                Interval::point(c.center[k])
                    .sub(Interval::point(p.center[k]))
                    .div(Interval::point(p.radius))
            }),
            Interval::point(c.radius).div(Interval::point(p.radius)),
        )
    }
    fn moment(&self, tetra: [[f64; 3]; 4], node: usize) -> Result<Vec<Interval>> {
        let center = self.nodes[node].center;
        let scale = Interval::point(self.nodes[node].radius);
        let volume = volume(tetra)?;
        let rank = self.powers.list.len();
        let mut h = vec![Interval::zero(); rank];
        h[0] = Interval::one();
        for point in tetra {
            let u: [Interval; 3] = std::array::from_fn(|k| {
                Interval::point(point[k])
                    .sub(Interval::point(center[k]))
                    .div(scale)
            });
            let old = h.clone();
            for (i, &a) in self.powers.list.iter().enumerate() {
                let mut v = old[i];
                for k in 0..3 {
                    if a[k] > 0 {
                        let mut previous = a;
                        previous[k] -= 1;
                        v = v.add(u[k].mul(h[self.powers.at(previous)]));
                    }
                }
                h[i] = v;
            }
        }
        for (value, &a) in h.iter_mut().zip(&self.powers.list) {
            let numerator = 6 * factorial(a[0]) * factorial(a[1]) * factorial(a[2]);
            let denominator = factorial(degree(a) + 3);
            *value = value
                .mul(volume)
                .mul(Interval::point(numerator as f64))
                .div(Interval::point(denominator as f64));
        }
        if h.iter().any(|v| !v.finite()) {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_INTERVAL",
                "tetra moment enclosure overflow",
            ));
        }
        Ok(h)
    }
    fn jet(&self, a: usize, b: usize) -> Result<(Vec<Interval>, Interval, Interval, Interval)> {
        let (r, q) = self.separation(a, b);
        let p = self.powers.order;
        let rank = self.powers.list.len();
        if !q.finite() || q.hi >= 1. || r.lo <= 0. {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_INTERVAL",
                "invalid separated pair",
            ));
        }
        let direction: [Interval; 3] = std::array::from_fn(|k| {
            Interval::point(self.nodes[a].center[k])
                .sub(Interval::point(self.nodes[b].center[k]))
                .div(r)
        });
        let mut result = vec![Interval::zero(); rank];
        result[0] = Interval::one();
        for (i, &alpha) in self.powers.list.iter().enumerate().skip(1) {
            let n = degree(alpha);
            let mut linear = Interval::zero();
            let mut quadratic = Interval::zero();
            for axis in 0..3 {
                if alpha[axis] > 0 {
                    let mut lower = alpha;
                    lower[axis] -= 1;
                    linear = linear.add(direction[axis].mul(result[self.powers.at(lower)]));
                }
                if alpha[axis] > 1 {
                    let mut lower = alpha;
                    lower[axis] -= 2;
                    quadratic = quadratic.add(result[self.powers.at(lower)]);
                }
            }
            result[i] = linear
                .mul(Interval::point((2 * n - 1) as f64))
                .add(quadratic.mul(Interval::point((n - 1) as f64)))
                .neg()
                .div(Interval::point(n as f64));
        }
        let remainder = Interval::one()
            .div(r)
            .mul(q.pow(p + 1))
            .div(Interval::one().sub(q));
        if result.iter().any(|v| !v.finite()) || !remainder.finite() {
            return Err(error(
                "HIERARCHICAL_MAGNETIC_INTERVAL",
                "Taylor enclosure overflow",
            ));
        }
        Ok((
            result,
            remainder,
            Interval::point(self.nodes[a].radius).div(r),
            Interval::point(self.nodes[b].radius).div(r),
        ))
    }
    fn quotient(&self, tetra: [[f64; 3]; 4], node: usize) -> Result<Vec<Interval>> {
        let full = self.moment(tetra, node)?;
        let mut result = vec![Interval::zero(); self.harmonic.list.len()];
        for (&a, value) in self.powers.list.iter().zip(full) {
            let normalized = value.div(Interval::point(
                (factorial(a[0]) * factorial(a[1]) * factorial(a[2])) as f64,
            ));
            let half = a[2] / 2;
            let sign = if half % 2 == 0 { 1. } else { -1. };
            for j in 0..=half {
                let target = self
                    .harmonic
                    .at([a[0] + 2 * j, a[1] + 2 * (half - j), a[2] % 2]);
                result[target] =
                    result[target].add(normalized.mul(Interval::point(sign * choose(half, j))));
            }
        }
        Ok(result)
    }
    fn shifted(
        &self,
        input: &[Interval],
        parent: usize,
        child: usize,
        transpose: bool,
    ) -> Vec<Interval> {
        let (offset, scale) = self.offset(parent, child);
        let mut values = input.to_vec();
        let scales: Vec<_> = (0..=self.powers.order).map(|d| scale.pow(d)).collect();
        if !transpose {
            for (v, &a) in values.iter_mut().zip(&self.harmonic.list) {
                *v = v.mul(scales[degree(a)]);
            }
        }
        let axes = if transpose { [2, 1, 0] } else { [0, 1, 2] };
        for axis in axes {
            let powers: Vec<_> = (0..=self.powers.order)
                .map(|d| offset[axis].pow(d))
                .collect();
            let mut next = vec![Interval::zero(); values.len()];
            for term in &self.shifts[axis] {
                let factor = term.weight.mul(powers[term.power]);
                let (target, source) = if transpose {
                    (term.source, term.target)
                } else {
                    (term.target, term.source)
                };
                next[target] = next[target].add(factor.mul(values[source]));
            }
            values = next;
        }
        if transpose {
            for (v, &a) in values.iter_mut().zip(&self.harmonic.list) {
                *v = v.mul(scales[degree(a)]);
            }
        }
        values
    }
    pub fn evidence(&self) -> serde_json::Value {
        let c = &self.interactions;
        let radii:Vec<_>=self.radius_histogram.iter().enumerate().filter(|(_,n)|**n>0).map(|(i,&n)|serde_json::json!({"log2RadiusMetersFloor":i as i32-48,"terminalNodes":n,"clamped":i==0||i==63})).collect();
        serde_json::json!({"policy":"certified-cartesian-tree-v1","method":"symmetric dual-tree Newton expansion","expansionOrder":self.powers.order,"separationRatioMaximum":self.qmax,"treeNodes":self.nodes.len(),"representationVersion":2,"harmonicBasis":"laplace-quotient-z01","momentCoefficientCount":self.harmonic.list.len(),"m2lCouplings":self.couplings.len(),"shiftStencilTerms":self.shifts.iter().map(Vec::len).sum::<usize>(),"axisSerial":true,"leafTetrahedra":self.leaf,"treeSignature":self.signature,
            "interactionStorage":"immutable-dual-tree-v1","interactionStackSlots":self.traversal_capacity,"interactionLevels":self.traversal_levels,"traversalStorageBytes":self.traversal_capacity*std::mem::size_of::<Pair>()+6*self.traversal_levels*std::mem::size_of::<usize>()+64*std::mem::size_of::<usize>(),"interactionReplaysPerAction":3,
            "visitedNodePairs":c.visited,"exactPairs":self.exact_pairs,"nearTiles":c.near,"farClusterPairs":c.far,
            "m2lScalarContractionsPerAxis":c.far as u128*self.couplings.len()as u128,
            "m2lScalarContractionsPerAction":3*c.far as u128*self.couplings.len()as u128,
            "shiftStencilApplicationsPerAction":6*(self.nodes.len()-1)as u128*self.shifts.iter().map(Vec::len).sum::<usize>()as u128,
            "visitedNodePairsPerAction":3*c.visited as u128,"pairDepthConvention":"maximum of source and target node depth",
            "farBlocksByDepth":c.far_by_depth,"nearTilesByDepth":c.near_by_depth,"exactPairsByDepth":c.exact_by_depth,
            "terminalRadiusMinimumM":self.minimum_radius,"terminalRadiusMaximumM":self.maximum_radius,"terminalRadiusLog2MetersHistogram":radii,
            "plannedBytes":self.planned_bytes,"scope":"far truncation and arithmetic enclosure; existing near/self quadrature, mesh and physical coverage remain unverified; streamed traversal storage does not certify action runtime"})
    }
    pub fn apply(
        &self,
        pair: &BoundedPairs,
        space: &Space,
        current: &[f64],
    ) -> Result<(Vec<f64>, Vec<f64>)> {
        check_deadline(self.start, self.wall)?;
        let action_started = Instant::now();
        let n = pair.tetra.len();
        let rank = self.harmonic.list.len();
        if current.len() != 3 * n || current.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "MAGNETIC_INPUT",
                "finite original three-axis current column required",
            ));
        }
        let mut action = vec![0.; 3 * n];
        let mut action_error = vec![0.; 3 * n];
        let mut near = vec![Interval::zero(); 3 * n];
        // Each Cartesian axis is an independent Newton action. Packed state is
        // released between axes; no3axis*allnodes interval slab is retained.
        for axis in 0..3 {
            let mut moments = Packed::new(self.nodes.len(), &self.harmonic);
            let mut local = Packed::new(self.nodes.len(), &self.harmonic);
            let mut tv = vec![Interval::zero(); self.nodes.len()];
            let mut tails = tv.clone();
            for (node, geometry) in self.nodes.iter().enumerate() {
                if geometry.children.is_some() {
                    continue;
                }
                let mut values = vec![Interval::zero(); rank];
                for &i in &self.permutation[geometry.range[0]..geometry.range[1]] {
                    check_deadline(self.start, self.wall)?;
                    let q = self.quotient(pair.tetra[i], node)?;
                    let density = Interval::point(space.cells[i].density_per_amp_m2);
                    let source = density.mul(Interval::point(current[3 * i + axis]));
                    tv[node] = tv[node].add(
                        density
                            .mul(Interval::point(current[3 * i + axis].abs()))
                            .mul(q[0]),
                    );
                    for k in 0..rank {
                        values[k] = values[k].add(source.mul(q[k]));
                    }
                }
                moments.write(node, &self.harmonic, &values)?;
            }
            for parent in (0..self.nodes.len()).rev() {
                if let Some(children) = self.nodes[parent].children {
                    for child in children {
                        check_deadline(self.start, self.wall)?;
                        tv[parent] = tv[parent].add(tv[child]);
                        let shifted = self.shifted(
                            &moments.read(child, &self.harmonic),
                            parent,
                            child,
                            false,
                        );
                        moments.add(parent, &self.harmonic, &shifted)?;
                    }
                }
            }
            self.walk(|block, far| {
                if !far {
                    if axis == 0 {
                        let a = &self.nodes[block.a];
                        let b = &self.nodes[block.b];
                        for x in a.range[0]..a.range[1] {
                            for y in b.range[0]..b.range[1] {
                                if block.a == block.b && y > x {
                                    continue;
                                }
                                check_deadline(self.start, self.wall)?;
                                let i = self.permutation[x];
                                let j = self.permutation[y];
                                let value = pair.pair(i.max(j), i.min(j))?;
                                let coefficient = Interval::point(1e-7)
                                    .mul(Interval::point(space.cells[i].density_per_amp_m2))
                                    .mul(Interval::point(space.cells[j].density_per_amp_m2))
                                    .mul(Interval::point(value));
                                for axis in 0..3 {
                                    near[3 * i + axis] = near[3 * i + axis].add(
                                        coefficient.mul(Interval::point(current[3 * j + axis])),
                                    );
                                    if i != j {
                                        near[3 * j + axis] = near[3 * j + axis].add(
                                            coefficient.mul(Interval::point(current[3 * i + axis])),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    return Ok(());
                }

                check_deadline(self.start, self.wall)?;
                let (mut derivatives, tail, sa, sb) = self.jet(block.a, block.b)?;
                let (r, _) = self.separation(block.a, block.b);
                for (value, &a) in derivatives.iter_mut().zip(&self.powers.list) {
                    *value = value
                        .mul(Interval::point(
                            (factorial(a[0]) * factorial(a[1]) * factorial(a[2])) as f64,
                        ))
                        .div(r);
                }
                tails[block.a] = tails[block.a].add(tail.mul(tv[block.b]));
                tails[block.b] = tails[block.b].add(tail.mul(tv[block.a]));
                let pa: Vec<_> = (0..=self.powers.order).map(|d| sa.pow(d)).collect();
                let pb: Vec<_> = (0..=self.powers.order).map(|d| sb.pow(d)).collect();
                let mut ma = moments.read(block.a, &self.harmonic);
                let mut mb = moments.read(block.b, &self.harmonic);
                for (i, &a) in self.harmonic.list.iter().enumerate() {
                    ma[i] = ma[i].mul(pa[degree(a)]);
                    mb[i] = mb[i].mul(pb[degree(a)]);
                }
                let mut la = vec![Interval::zero(); rank];
                let mut lb = la.clone();
                for term in &self.couplings {
                    let mut value = derivatives[term.jet];
                    if term.source_degree % 2 == 1 {
                        value = value.neg();
                    }
                    la[term.target] = la[term.target].add(value.mul(mb[term.source]));
                    lb[term.source] = lb[term.source].add(value.mul(ma[term.target]));
                }
                for (i, &a) in self.harmonic.list.iter().enumerate() {
                    la[i] = la[i].mul(pa[degree(a)]);
                    lb[i] = lb[i].mul(pb[degree(a)]);
                }
                local.add(block.a, &self.harmonic, &la)?;
                local.add(block.b, &self.harmonic, &lb)?;

                Ok(())
            })?;
            for parent in 0..self.nodes.len() {
                if let Some(children) = self.nodes[parent].children {
                    for child in children {
                        check_deadline(self.start, self.wall)?;
                        tails[child] = tails[child].add(tails[parent]);
                        let shifted =
                            self.shifted(&local.read(parent, &self.harmonic), parent, child, true);
                        local.add(child, &self.harmonic, &shifted)?;
                    }
                }
            }
            let mut out = vec![Interval::zero(); n];
            let mut truncation = vec![0f64; n];
            for (node, geometry) in self.nodes.iter().enumerate() {
                if geometry.children.is_some() {
                    continue;
                }
                let values = local.read(node, &self.harmonic);
                for &i in &self.permutation[geometry.range[0]..geometry.range[1]] {
                    check_deadline(self.start, self.wall)?;
                    let target = self.quotient(pair.tetra[i], node)?;
                    let density = Interval::point(1e-7)
                        .mul(Interval::point(space.cells[i].density_per_amp_m2));
                    out[i] = sum((0..rank).map(|k| values[k].mul(target[k]))).mul(density);
                    truncation[i] = density.mul(target[0]).mul(tails[node]).hi.max(0.);
                }
            }
            for (i, far_value) in out.into_iter().enumerate() {
                let value = far_value.add(near[3 * i + axis]);
                let mid = value.mid();
                let uncertainty = up(value.radius(mid) + truncation[i]);
                if !value.finite() || !mid.is_finite() || !uncertainty.is_finite() {
                    return Err(error(
                        "HIERARCHICAL_MAGNETIC_INTERVAL",
                        "original harmonic action enclosure overflow",
                    ));
                }
                action[3 * i + axis] = mid;
                action_error[3 * i + axis] = uncertainty;
            }
        }
        eprintln!("P7_HIERARCHICAL_ACTION originalCurrents={} harmonicCoefficients={} m2lCouplings={} farClusterPairs={} exactPairs={} elapsedSeconds={:.6}",3*n,rank,self.couplings.len(),self.interactions.far,self.exact_pairs,action_started.elapsed().as_secs_f64());
        Ok((action, action_error))
    }
}
fn volume(t: [[f64; 3]; 4]) -> Result<Interval> {
    let v: [[Interval; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|k| Interval::point(t[i + 1][k]).sub(Interval::point(t[0][k])))
    });
    let determinant = v[0][0]
        .mul(v[1][1].mul(v[2][2]).sub(v[1][2].mul(v[2][1])))
        .sub(v[0][1].mul(v[1][0].mul(v[2][2]).sub(v[1][2].mul(v[2][0]))))
        .add(v[0][2].mul(v[1][0].mul(v[2][1]).sub(v[1][1].mul(v[2][0]))));
    let determinant = if determinant.hi < 0. {
        determinant.neg()
    } else {
        determinant
    };
    let result = determinant.div(Interval::point(6.));
    if !result.finite() || result.lo <= 0. {
        return Err(error(
            "HIERARCHICAL_MAGNETIC_GEOMETRY",
            "positive tetra volume interval required",
        ));
    }
    Ok(result)
}

/// Numerator UPPER, denominator LOWER and uncertainty UPPER for one original
/// KVL row. The normalization cannot grow because of approximate L error.
fn abs_lower(value: Interval) -> f64 {
    if value.lo <= 0. && value.hi >= 0. {
        0.
    } else {
        value.lo.abs().min(value.hi.abs())
    }
}
fn kvl_intervals(
    resistive: Interval,
    gradient: Interval,
    inductive: f64,
    signed_w: f64,
    bound: f64,
) -> Result<(f64, f64, f64)> {
    if !resistive.finite()
        || !gradient.finite()
        || [inductive, signed_w, bound].iter().any(|v| !v.is_finite())
        || bound < 0.
    {
        return Err(error(
            "HIERARCHICAL_MAGNETIC_INTERVAL",
            "invalid original KVL uncertainty input",
        ));
    }
    let uncertainty = Interval::point(signed_w.abs()).mul(Interval::point(bound));
    let residual = resistive
        .sub(gradient)
        .add(Interval::point(signed_w).mul(Interval::point(inductive)));
    let numerator = Interval::point(residual.lo.abs().max(residual.hi.abs()))
        .add(uncertainty)
        .hi;
    let image_lower = Interval::point(inductive.abs())
        .sub(Interval::point(bound))
        .lo
        .max(0.);
    let denominator = Interval::point(abs_lower(resistive))
        .add(Interval::point(abs_lower(gradient)))
        .add(Interval::point(signed_w.abs()).mul(Interval::point(image_lower)))
        .lo
        .max(0.);
    if !numerator.is_finite() || !denominator.is_finite() || !uncertainty.finite() {
        return Err(error(
            "HIERARCHICAL_MAGNETIC_INTERVAL",
            "original KVL enclosure overflow",
        ));
    }
    Ok((numerator, denominator, uncertainty.hi.max(0.)))
}
pub(crate) fn kvl_cell(
    resistance: f64,
    current: f64,
    terms: &[(usize, f64)],
    voltage: &[f64],
    inductive: f64,
    signed_w: f64,
    bound: f64,
) -> Result<(f64, f64, f64)> {
    let resistive = Interval::point(resistance).mul(Interval::point(current));
    let mut gradient = Interval::zero();
    if let Some((&(anchor, _), remaining)) = terms.split_first() {
        let anchor = *voltage.get(anchor).ok_or_else(|| {
            error(
                "HIERARCHICAL_MAGNETIC_INTERVAL",
                "original gradient anchor missing",
            )
        })?;
        for &(node, coefficient) in remaining {
            let value = *voltage.get(node).ok_or_else(|| {
                error(
                    "HIERARCHICAL_MAGNETIC_INTERVAL",
                    "original gradient coordinate missing",
                )
            })?;
            gradient = gradient.add(
                Interval::point(coefficient)
                    .mul(Interval::point(value).sub(Interval::point(anchor))),
            );
        }
    }
    kvl_intervals(resistive, gradient, inductive, signed_w, bound)
}
#[cfg(test)]
fn kvl_row(
    resistive: f64,
    gradient: f64,
    inductive: f64,
    signed_w: f64,
    bound: f64,
) -> Result<(f64, f64, f64)> {
    kvl_intervals(
        Interval::point(resistive),
        Interval::point(gradient),
        inductive,
        signed_w,
        bound,
    )
}
pub(crate) fn ratio(numerator: f64, denominator: f64) -> Result<f64> {
    if numerator == 0. {
        return Ok(0.);
    }
    if denominator <= 0. {
        return Err(error(
            "HIERARCHICAL_MAGNETIC_UNCERTIFIED",
            "positive original KVL normalization lower bound unavailable",
        ));
    }
    let value = Interval::point(numerator)
        .div(Interval::point(denominator))
        .hi;
    if !value.is_finite() {
        return Err(error(
            "HIERARCHICAL_MAGNETIC_INTERVAL",
            "original KVL ratio overflow",
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::super::{PairQuadrature, Streamed};
    use super::*;
    use crate::extraction::local::{mesh, polyhedron, prism, volume_current};
    use std::collections::BTreeMap;

    fn samples(tetra: [[f64; 3]; 4]) -> Vec<([f64; 3], f64)> {
        let volume = mesh::tetra_coefficients(tetra).unwrap().0;
        let mut result = vec![];
        polyhedron::tetra_integrate(tetra, volume, prism::cached_rule(8).unwrap(), |p, w| {
            result.push((p, w));
            Ok(())
        })
        .unwrap();
        result
    }
    fn discrete_moments(
        tree: &Operator,
        node: usize,
        samples: &[([f64; 3], f64)],
    ) -> Vec<Interval> {
        let geometry = &tree.nodes[node];
        tree.powers
            .list
            .iter()
            .map(|&a| {
                sum(samples.iter().map(|&(p, w)| {
                    (0..3).fold(Interval::point(w), |v, k| {
                        v.mul(
                            Interval::point(p[k])
                                .sub(Interval::point(geometry.center[k]))
                                .div(Interval::point(geometry.radius))
                                .pow(a[k]),
                        )
                    })
                }))
            })
            .collect()
    }
    // Independent positive fixed8 direct double cubature. Its certificate
    // explicitly measures all computed polynomial moment defects; no floating
    // Gauss node or quadrature exactness is assumed. Both true and discrete
    // positive measures receive the Newton tail bound.
    fn reference_pair(
        tree: &Operator,
        a: usize,
        b: usize,
        tetra: &[[[f64; 3]; 4]],
        points: &[Vec<([f64; 3], f64)>],
    ) -> (f64, f64) {
        let i = tree.permutation[tree.nodes[a].range[0]];
        let j = tree.permutation[tree.nodes[b].range[0]];
        let ma = tree.moment(tetra[i], a).unwrap();
        let mb = tree.moment(tetra[j], b).unwrap();
        let da = discrete_moments(tree, a, &points[i]);
        let db = discrete_moments(tree, b, &points[j]);
        let (jet, tail, sa, sb) = tree.jet(a, b).unwrap();
        let (r, _) = tree.separation(a, b);
        let mut defect = Interval::zero();
        for (x, &alpha) in tree.powers.list.iter().enumerate() {
            for (y, &beta) in tree.powers.list.iter().enumerate() {
                let together = plus(alpha, beta);
                if degree(together) > tree.powers.order {
                    continue;
                }
                let mut factor = jet[tree.powers.at(together)]
                    .mul(Interval::point(choose3(together, alpha)))
                    .mul(sa.pow(degree(alpha)))
                    .mul(sb.pow(degree(beta)))
                    .div(r);
                if degree(beta) % 2 == 1 {
                    factor = factor.neg();
                }
                defect = defect.add(factor.mul(ma[x].mul(mb[y]).sub(da[x].mul(db[y]))));
            }
        }
        let mut direct = Interval::zero();
        for &(p, wp) in &points[i] {
            for &(q, wq) in &points[j] {
                direct = direct.add(
                    Interval::point(wp)
                        .mul(Interval::point(wq))
                        .div(distance(p, q)),
                );
            }
        }
        let mid = direct.mid();
        let remainder = tail.mul(ma[0].mul(mb[0]).add(da[0].mul(db[0])));
        let error = Interval::point(defect.lo.abs().max(defect.hi.abs()))
            .add(remainder)
            .add(Interval::point(direct.radius(mid)))
            .hi;
        assert!(error.is_finite() && error >= 0.);
        (mid, error)
    }
    #[test]
    fn hierarchical_signed_original_actions_and_full_kvl_enclosures_match_bounded_reference() {
        let start = Instant::now();
        let wall = Duration::from_secs(1800);
        let tetra = |x: f64| {
            [
                [x, 0., 0.],
                [x + 0.001, 0., 0.],
                [x, 0.001, 0.],
                [x, 0., 0.001],
            ]
        };
        let geometry = vec![tetra(0.), tetra(0.001), tetra(0.1)];
        let cells = geometry
            .iter()
            .enumerate()
            .map(|(i, &t)| {
                let v = mesh::tetra_coefficients(t).unwrap().0;
                let length = v.cbrt();
                volume_current::Cell {
                    mesh_cell: i,
                    potentials: [0, 1, 2, 3],
                    incidence: [[0.; 3]; 4],
                    density_per_amp_m2: 1. / length.powi(2),
                    resistance_ohm: 1. / (58e6 * length),
                }
            })
            .collect();
        let space = Space {
            mesh_nodes: vec![vec![]; 4],
            node_potential: BTreeMap::new(),
            cells,
            components: vec![vec![0, 1, 2, 3]],
            planned_bytes: 0,
        };
        let budget = AllocationBudget {
            max_bytes: 64 * 1024 * 1024,
            max_nodes: 1000,
            max_tetrahedra: 1000,
        };
        let tree = Operator::new(&geometry, 10, 1, 0.2, 0, budget, start, wall).unwrap();
        assert_eq!(tree.harmonic.list.len(), 121);
        assert_eq!(tree.couplings.len(), 2926);
        assert_eq!(
            tree.shifts.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![506, 506, 1096]
        );
        assert!(tree.interactions.far > 0);
        let mut tiny_near = vec![];
        let mut tiny_far = vec![];
        let replay = tree
            .walk(|pair, far| {
                if far {
                    tiny_far.push(pair)
                } else {
                    tiny_near.push(pair)
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(replay.visited, tree.interactions.visited);
        assert!(tree.exact_pairs > 0);
        let tile_pairs: usize = tiny_near
            .iter()
            .map(|pair| {
                let a = tree.nodes[pair.a].range;
                let b = tree.nodes[pair.b].range;
                let na = a[1] - a[0];
                let nb = b[1] - b[0];
                if pair.a == pair.b {
                    na * (na + 1) / 2
                } else {
                    na * nb
                }
            })
            .sum();
        let far_pairs: usize = tiny_far
            .iter()
            .map(|pair| {
                let a = tree.nodes[pair.a].range;
                let b = tree.nodes[pair.b].range;
                (a[1] - a[0]) * (b[1] - b[0])
            })
            .sum();
        assert_eq!(tile_pairs, tree.exact_pairs);
        assert_eq!(
            tile_pairs + far_pairs,
            geometry.len() * (geometry.len() + 1) / 2
        );
        // Geometry-only exacttile cover control, not a physical overlapping
        // mesh or an additional electromagnetic acceptance case.
        let colocated = vec![geometry[0]; 4];
        let tiles = Operator::new(&colocated, 10, 1, 0.2, 0, budget, start, wall).unwrap();
        assert_eq!(tiles.interactions.far, 0);
        assert_eq!(tiles.interactions.near, 1);
        assert_eq!(tiles.exact_pairs, 10);

        let (owned, slots) = BoundedPairs::plan(geometry.len(), 12, 0, budget).unwrap();
        let pair = BoundedPairs::new(geometry.clone(), 12, slots, start, wall).unwrap();
        // Same production dispatch, fineleaf policy and fixed operator for
        // every basis and independently recovered full-state action.
        let streamed = Streamed {
            space: &space,
            pair,
            hierarchical: Some(tree),
            owned_bytes: owned,
            start,
            wall,
        };
        let tree = streamed.hierarchical.as_ref().unwrap();
        let points: Vec<_> = geometry.iter().copied().map(samples).collect();
        let mut reference_tree =
            Operator::new(&geometry, 12, 1, 0.49, 0, budget, start, wall).unwrap();
        // Include actual computed cubature points in the independent reference
        // balls, covering any interpolation rounding beyond the input simplex.
        for node in &mut reference_tree.nodes {
            if node.children.is_none() {
                let cell = reference_tree.permutation[node.range[0]];
                for &(point, _) in &points[cell] {
                    node.radius = node.radius.max(distance(point, node.center).hi);
                }
            }
        }
        let leaf_for = |cell: usize| {
            reference_tree
                .nodes
                .iter()
                .position(|node| {
                    node.children.is_none() && reference_tree.permutation[node.range[0]] == cell
                })
                .unwrap()
        };
        let old = PairQuadrature::new(&geometry, 12, || check_deadline(start, wall)).unwrap();
        let count = space.current_count();
        let mut kernel = vec![vec![(0., 0.); geometry.len()]; geometry.len()];
        for i in 0..geometry.len() {
            for j in 0..=i {
                check_deadline(start, wall).unwrap();
                let a = leaf_for(i);
                let b = leaf_for(j);
                let (_, q) = reference_tree.separation(a, b);
                let value = if i != j && q.finite() && q.hi < 0.49 {
                    reference_pair(&reference_tree, a, b, &geometry, &points)
                } else {
                    (old.pair(i, j).unwrap(), 0.)
                };
                kernel[i][j] = value;
                kernel[j][i] = value;
            }
        }
        let mut currents: Vec<Vec<f64>> = (0..count)
            .map(|i| {
                let mut q = vec![0.; count];
                q[i] = 1.;
                q
            })
            .collect();
        let signed: Vec<_> = (0..count)
            .map(|i| {
                if i % 2 == 0 {
                    1. + 0.125 * i as f64
                } else {
                    -(1. + 0.125 * i as f64)
                }
            })
            .collect();
        currents.push(signed.clone());
        currents.push(signed.iter().map(|v| -v).collect());
        let mut cases = vec![];
        let mut observed = vec![];
        for (case, current) in currents.iter().enumerate() {
            check_deadline(start, wall).unwrap();
            let (mut action, mut enclosure) =
                streamed.actions_certified(&[current.clone()]).unwrap();
            let approximate = action.remove(0);
            let error = enclosure.as_mut().unwrap().remove(0);
            let mut exact = vec![Interval::zero(); count];
            let mut reference_bound = vec![Interval::zero(); count];
            for i in 0..geometry.len() {
                for j in 0..geometry.len() {
                    let (value, bound) = kernel[i][j];
                    let coefficient = Interval::point(1e-7)
                        .mul(Interval::point(space.cells[i].density_per_amp_m2))
                        .mul(Interval::point(space.cells[j].density_per_amp_m2));
                    for axis in 0..3 {
                        exact[3 * i + axis] = exact[3 * i + axis].add(
                            coefficient
                                .mul(Interval::point(value))
                                .mul(Interval::point(current[3 * j + axis])),
                        );
                        reference_bound[3 * i + axis] = reference_bound[3 * i + axis].add(
                            coefficient
                                .mul(Interval::point(bound))
                                .mul(Interval::point(current[3 * j + axis].abs())),
                        );
                    }
                }
            }
            let reference: Vec<_> = exact.iter().map(|v| v.mid()).collect();
            let reference_error: Vec<_> = exact
                .iter()
                .enumerate()
                .map(|(i, v)| up(v.radius(reference[i]) + reference_bound[i].hi))
                .collect();
            for row in 0..count {
                assert!(
                    (reference[row] - approximate[row]).abs() <= error[row] + reference_error[row],
                    "case{case} row{row}"
                );
            }
            if case < count {
                observed.push(approximate.clone());
            }
            let mut packet = serde_json::json!({"current":current,"exactAction":reference,"approximateAction":approximate,"absoluteErrorBound":error,"referenceErrorBound":reference_error,"frequencyHz":1000.});
            if case >= count {
                let w = 2. * std::f64::consts::PI * 1000.;
                let mut approximation_residual = vec![];
                let mut reference_residual = vec![];
                let mut uncertainty = vec![];
                let mut reference_uncertainty = vec![];
                let mut scale = 0f64;
                let mut numerator = 0f64;
                let mut reference_numerator = 0f64;
                for row in 0..count {
                    // Deliberately manufactured original-current KVL rowterms:
                    // gradient cancels the independently computed reference
                    // inductive term. This is not a solved FEMboard response.
                    let gradient = w * reference[row];
                    let residual = Interval::point(w)
                        .mul(Interval::point(approximate[row]))
                        .sub(Interval::point(gradient));
                    let reference_value = Interval::point(w)
                        .mul(Interval::point(reference[row]))
                        .sub(Interval::point(gradient));
                    let (num, den, unc) =
                        kvl_row(0., gradient, approximate[row], w, error[row]).unwrap();
                    let (ref_num, ref_den, ref_unc) =
                        kvl_row(0., gradient, reference[row], w, reference_error[row]).unwrap();
                    numerator = numerator.max(num);
                    reference_numerator = reference_numerator.max(ref_num);
                    scale = scale.max(den.min(ref_den));
                    approximation_residual.push(residual.mid());
                    reference_residual.push(reference_value.mid());
                    uncertainty.push(
                        Interval::point(unc)
                            .add(Interval::point(residual.radius(residual.mid())))
                            .hi,
                    );
                    reference_uncertainty.push(
                        Interval::point(ref_unc)
                            .add(Interval::point(
                                reference_value.radius(reference_value.mid()),
                            ))
                            .hi,
                    );
                }
                // Extra interval row-rounding is explicit in the packet,
                // rather than a hidden comparison floor in the verifier.
                numerator = (0..count)
                    .map(|i| up(approximation_residual[i].abs() + uncertainty[i]))
                    .fold(numerator, f64::max);
                reference_numerator = (0..count)
                    .map(|i| up(reference_residual[i].abs() + reference_uncertainty[i]))
                    .fold(reference_numerator, f64::max);
                let upper = ratio(numerator, scale).unwrap();
                assert!(upper <= 1e-8);
                assert!(ratio(reference_numerator, scale).unwrap() <= 1e-8);
                packet["fullKvl"] = serde_json::json!({"scope":"manufactured-original-current-rows","exactResidual":reference_residual,"approximateResidual":approximation_residual,"uncertaintyBound":uncertainty,"referenceUncertaintyBound":reference_uncertainty,"scale":scale,"upperRelative":upper});
            }
            cases.push(packet);
        }
        for i in 0..count {
            for j in 0..count {
                assert!(
                    (observed[i][j] - observed[j][i]).abs()
                        <= 1e-10 * observed[i][i].abs().max(observed[j][j].abs())
                );
            }
        }
        assert!(Operator::new(
            &geometry,
            10,
            1,
            0.2,
            0,
            AllocationBudget {
                max_bytes: 1,
                ..budget
            },
            start,
            wall
        )
        .is_err());
        assert!(Operator::new(&geometry, 10, 1, 0.2, 0, budget, start, Duration::ZERO).is_err());
        let mut invalid = currents[0].clone();
        invalid[0] = f64::NAN;
        assert!(streamed.actions_certified(&[invalid]).is_err());
        let mut packet = tree.evidence();
        packet["schema"] = serde_json::json!("p7-hierarchical-magnetic-v1");
        packet["originalCellCount"] = serde_json::json!(geometry.len());
        packet["cases"] = serde_json::json!(cases);
        packet["coverage"] = serde_json::json!("unverified");
        println!("P7_HIERARCHICAL_MAGNETIC {packet}");
    }
}
