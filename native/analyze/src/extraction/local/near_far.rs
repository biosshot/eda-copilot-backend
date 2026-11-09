//! Bounded magnetic near/far planning and a small physical-space prototype.
//! Far blocks compress the plan, not a fast-multipole action: far application
//! still streams individual degree-four moment pairs. No ROI solver admission.
use super::{
    error,
    far::Moments,
    polyhedron::PointPotential,
    prism::{GaussRule, Prism},
    Result,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_bytes: usize,
    pub max_nodes: usize,
    pub max_blocks: usize,
    pub max_pairs: usize,
    pub max_wall: Duration,
}
fn checked(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| error("MEMORY_LIMIT", "near/far size overflow"))
}
fn product(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| error("MEMORY_LIMIT", "near/far size overflow"))
}
fn packed(n: usize) -> Result<usize> {
    Ok(product(n, checked(n, 1)?)? / 2)
}
fn clock(start: Instant, b: Budget) -> Result<()> {
    if start.elapsed() > b.max_wall {
        Err(error(
            "DEADLINE",
            "original near/far request deadline exceeded",
        ))
    } else {
        Ok(())
    }
}
fn memory(bytes: usize, b: Budget) -> Result<()> {
    if bytes > b.max_bytes {
        Err(error(
            "MEMORY_LIMIT",
            "near/far simultaneous storage plan exceeds explicit request cap",
        ))
    } else {
        Ok(())
    }
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}

#[derive(Clone, Debug)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub children: Option<[usize; 2]>,
    pub center: [f64; 3],
    pub radius: f64,
    prefix_count: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub a: usize,
    pub b: usize,
    pub far: bool,
    pub pairs: usize,
    pub radius_ratio: f64,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub moments: Vec<Moments>,
    pub order: Vec<usize>,
    pub nodes: Vec<Node>,
    pub blocks: Vec<Block>,
    pub near_pairs: usize,
    pub far_pairs: usize,
    pub planned_bytes: usize,
    pub construction_seconds: f64,
    pub promoted_pairs: usize,
    promoted: Vec<u64>,
    budget: Budget,
    started: Instant,
    leaf: usize,
    theta: f64,
    included_prefix: usize,
    block_cap: usize,
}
impl Plan {
    pub fn new(prisms: &[Prism], theta: f64, leaf: usize, budget: Budget) -> Result<Self> {
        Self::new_policy(prisms,theta,leaf,budget,true)
    }
    /// Retained G1 diagnostic baseline on exactly the same geometry.
    pub fn new_coarse(prisms: &[Prism], theta: f64, leaf: usize, budget: Budget) -> Result<Self> {
        Self::new_policy(prisms,theta,leaf,budget,false)
    }
    fn new_policy(prisms: &[Prism], theta: f64, leaf: usize, budget: Budget, individual: bool) -> Result<Self> {
        let started=Instant::now();
        memory(product(prisms.len(),std::mem::size_of::<Moments>())?,budget)?;
        let mut moments=Vec::with_capacity(prisms.len());
        for prism in prisms {clock(started,budget)?;moments.push(Moments::new(prism)?);}
        Self::from_moments_policy(moments,theta,leaf,budget,individual,started,None)
    }
    /// Build the same conservative pair partition for any validated physical
    /// support moments. Geometry ownership and non-overlap remain caller-owned.
    pub fn from_moments(moments: Vec<Moments>, theta:f64, leaf:usize, budget:Budget)->Result<Self> {
        Self::from_moments_policy(moments,theta,leaf,budget,true,Instant::now(),None)
    }
    pub fn from_moments_coarse(moments: Vec<Moments>, theta:f64, leaf:usize, budget:Budget)->Result<Self> {
        Self::from_moments_policy(moments,theta,leaf,budget,false,Instant::now(),None)
    }
    /// Cover all pairs with at least one support in the prefix. The suffix
    /// self-domain is owned by a separate operator and is never visited.
    pub fn from_moments_excluding_suffix_pairs(moments: Vec<Moments>, prefix: usize, theta:f64, leaf:usize, budget:Budget)->Result<Self> {
        if prefix > moments.len() { return Err(error("NEAR_FAR_INPUT", "prefix exceeds support count")); }
        Self::from_moments_policy(moments,theta,leaf,budget,true,Instant::now(),Some(prefix))
    }
    fn from_moments_policy(moments:Vec<Moments>,theta:f64,leaf:usize,budget:Budget,individual:bool,started:Instant,prefix:Option<usize>)->Result<Self> {
        Self::from_moments_full(moments,theta,leaf,budget,individual,started,prefix,false)
    }
    /// Like the prefix-excluding plan, but individual near-pair promotion is
    /// limited to prefix-suffix pairs (prefix-prefix pairs are owned elsewhere).
    pub fn from_moments_cross_only(moments: Vec<Moments>, prefix: usize, theta:f64, leaf:usize, budget:Budget)->Result<Self> {
        if prefix > moments.len() { return Err(error("NEAR_FAR_INPUT", "prefix exceeds support count")); }
        Self::from_moments_full(moments,theta,leaf,budget,true,Instant::now(),Some(prefix),true)
    }
    fn from_moments_full(moments:Vec<Moments>,theta:f64,leaf:usize,budget:Budget,individual:bool,started:Instant,prefix:Option<usize>,cross_only:bool)->Result<Self> {
        clock(started,budget)?;
        if moments.is_empty() || !(theta>0. && theta<1.) || !(1..=64).contains(&leaf) || moments.iter().any(|m|m.center_m.iter().any(|x|!x.is_finite()) || !m.radius_m.is_finite() || m.radius_m<=0. || !m.volume_m3.is_finite() || m.volume_m3<=0.) {
            return Err(error("NEAR_FAR_INPUT","nonempty finite moments, 0<theta<1 and leaf1..64 required"));
        }
        let n = moments.len();
        let included_prefix = prefix.unwrap_or(n);
        let total = packed(n)?.checked_sub(packed(n - included_prefix)?).ok_or_else(||error("MEMORY_LIMIT", "near/far domain overflow"))?;
        let node_cap = product(n, 2)?;
        if node_cap > budget.max_nodes || total > budget.max_pairs {
            return Err(error(
                "MESH_LIMIT",
                "near/far support/pair preflight exceeds explicit cap",
            ));
        }
        let block_cap = total.min(budget.max_blocks);
        let mask_words=if individual {checked(total,63)?/64}else{0};
        let bytes = checked(product(mask_words,8)?,checked(
            product(n, std::mem::size_of::<Moments>() + 8 + 8192)?,
            checked(
                product(node_cap, std::mem::size_of::<Node>())?,
                0,
            )?,
        )?)?;
        memory(bytes, budget)?;
        let mut p = Self {
            moments,
            order: (0..n).collect(),
            nodes: Vec::with_capacity(node_cap),
            blocks: Vec::with_capacity(block_cap.min(1024)),
            block_cap,
            near_pairs: 0,
            far_pairs: 0,
            planned_bytes: bytes,
            construction_seconds: 0.,
            promoted_pairs: 0,
            promoted: vec![0;mask_words],
            budget,
            started,
            leaf,
            theta,
            included_prefix,
        };
        let root = p.tree(0, n)?;
        p.interactions(root, root)?;
        if individual {
            let mut mask=std::mem::take(&mut p.promoted);
            let mut count=0;
            let promote=|i:usize,j:usize|->Result<()> {
                clock(started,budget)?;
                if i!=j {
                    let a=&p.moments[i];let b=&p.moments[j];
                    let q=(a.radius_m+b.radius_m)/distance(a.center_m,b.center_m)*(1.+32.*f64::EPSILON);
                    if q.is_finite() && q<=theta {
                        // The same finite moment/remainder admission used by
                        // the operator. A failed check stays exact-near.
                        if a.pair(b).is_ok() {
                            let k=p.pair_index(i,j);mask[k/64]|=1u64<<(k%64);count+=1;
                        }
                    }
                }
                Ok(())
            };
            if cross_only {p.visit_cross_pairs(false,promote)?} else {p.visit_pairs(false,promote)?}
            p.promoted=mask;p.promoted_pairs=count;p.near_pairs-=count;p.far_pairs+=count;
        }
        if checked(p.near_pairs, p.far_pairs)? != total {
            return Err(error(
                "NEAR_FAR_COVERAGE",
                "pair partition lost or duplicated geometry",
            ));
        }
        p.planned_bytes=checked(p.planned_bytes,product(p.blocks.capacity(),std::mem::size_of::<Block>())?)?;
        memory(p.planned_bytes,budget)?;
        p.construction_seconds = started.elapsed().as_secs_f64();
        Ok(p)
    }
    fn tree(&mut self, start: usize, end: usize) -> Result<usize> {
        clock(self.started, self.budget)?;
        if self.nodes.len() >= self.budget.max_nodes {
            return Err(error("MESH_LIMIT", "tree node cap"));
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for k in start..end {
            let m = &self.moments[self.order[k]];
            for a in 0..3 {
                lo[a] = lo[a].min(m.center_m[a]);
                hi[a] = hi[a].max(m.center_m[a]);
            }
        }
        let center = std::array::from_fn(|a| lo[a] + (hi[a] - lo[a]) / 2.);
        // Covers every complete individual bounding ball, not just centroids.
        let radius = (start..end)
            .map(|k| {
                let m = &self.moments[self.order[k]];
                distance(center, m.center_m) + m.radius_m
            })
            .fold(0., f64::max)
            * (1. + 64. * f64::EPSILON);
        if !radius.is_finite() || radius <= 0. {
            return Err(error(
                "NEAR_FAR_GEOMETRY",
                "invalid full-support bounding ball",
            ));
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            start,
            end,
            children: None,
            center,
            radius,
            prefix_count: (start..end).filter(|&k| self.order[k] < self.included_prefix).count(),
        });
        if end - start > self.leaf {
            let axis = (0..3)
                .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
                .unwrap();
            let middle = start + (end - start) / 2;
            let moments = &self.moments;
            self.order[start..end].select_nth_unstable_by(middle - start, |&i, &j| {
                moments[i].center_m[axis]
                    .total_cmp(&moments[j].center_m[axis])
                    .then(i.cmp(&j))
            });
            let left = self.tree(start, middle)?;
            let right = self.tree(middle, end)?;
            self.nodes[id].children = Some([left, right]);
        }
        Ok(id)
    }
    fn push(&mut self, a: usize, b: usize, far: bool, q: f64) -> Result<()> {
        if self.blocks.len() >= self.block_cap {
            return Err(error(
                "MESH_LIMIT",
                "bounded interaction block capacity exhausted",
            ));
        }
        if self.blocks.len()==self.blocks.capacity() && self.blocks.len()>=1024 {
            // Amortized: growth doubles, so check the real storage on each doubling.
            let real=checked(self.planned_bytes,product(self.blocks.capacity()*2,std::mem::size_of::<Block>())?)?;
            memory(real,self.budget)?;
        }
        let na = self.nodes[a].end - self.nodes[a].start;
        let nb = self.nodes[b].end - self.nodes[b].start;
        let suffix_a = na - self.nodes[a].prefix_count;
        let suffix_b = nb - self.nodes[b].prefix_count;
        let pairs = if a == b {
            packed(na)?.checked_sub(packed(suffix_a)?)
        } else {
            product(na, nb)?.checked_sub(product(suffix_a, suffix_b)?)
        }.ok_or_else(||error("MEMORY_LIMIT", "near/far domain count overflow"))?;
        if pairs == 0 { return Ok(()); }
        if far {
            self.far_pairs = checked(self.far_pairs, pairs)?
        } else {
            self.near_pairs = checked(self.near_pairs, pairs)?
        }
        self.blocks.push(Block {
            a,
            b,
            far,
            pairs,
            radius_ratio: q,
        });
        Ok(())
    }
    fn interactions(&mut self, a: usize, b: usize) -> Result<()> {
        clock(self.started, self.budget)?;
        let aa = self.nodes[a].clone();
        let bb = self.nodes[b].clone();
        if aa.prefix_count == 0 && bb.prefix_count == 0 { return Ok(()); }
        if a == b {
            if let Some([l, r]) = aa.children {
                self.interactions(l, l)?;
                self.interactions(l, r)?;
                self.interactions(r, r)
            } else {
                self.push(a, b, false, f64::INFINITY)
            }
        } else {
            let q = (aa.radius + bb.radius) / distance(aa.center, bb.center)
                * (1. + 64. * f64::EPSILON);
            if q.is_finite() && q <= self.theta {
                return self.push(a, b, true, q);
            }
            if aa.children.is_none() && bb.children.is_none() {
                return self.push(a, b, false, q);
            }
            if aa.children.is_some() && (bb.children.is_none() || aa.radius >= bb.radius) {
                let [l, r] = aa.children.unwrap();
                self.interactions(l, b)?;
                self.interactions(r, b)
            } else {
                let [l, r] = bb.children.unwrap();
                self.interactions(a, l)?;
                self.interactions(a, r)
            }
        }
    }
    pub fn visit_pairs(
        &self,
        far: bool,
        mut f: impl FnMut(usize, usize) -> Result<()>,
    ) -> Result<()> {
        for block in &self.blocks {
            if block.far && !far {continue;}
            if !block.far && far && self.promoted_pairs==0 {continue;}
            let a = &self.nodes[block.a];
            let b = &self.nodes[block.b];
            for i in a.start..a.end {
                let first = if block.a == block.b { i } else { b.start };
                for j in first..b.end {
                    let (ii,jj)=(self.order[i],self.order[j]);
                    if ii >= self.included_prefix && jj >= self.included_prefix { continue; }
                    let k=self.pair_index(ii,jj);
                    let promoted=!self.promoted.is_empty() && (self.promoted[k/64]>>(k%64))&1!=0;
                    if (block.far || promoted)!=far {continue;}
                    f(ii,jj)?;
                }
            }
        }
        Ok(())
    }
    /// Visit only pairs with one prefix and one suffix support.
    pub fn visit_cross_pairs(&self, far: bool, mut f: impl FnMut(usize, usize) -> Result<()>) -> Result<()> {
        let ip=self.included_prefix;
        for block in &self.blocks {
            if block.far && !far {continue;}
            if !block.far && far && self.promoted_pairs==0 {continue;}
            let a = &self.nodes[block.a];
            let b = &self.nodes[block.b];
            let (pa,pb)=(a.prefix_count,b.prefix_count);
            let (sa,sb)=((a.end-a.start)-pa,(b.end-b.start)-pb);
            if (pa==0||sb==0)&&(pb==0||sa==0) {continue;}
            for i in a.start..a.end {
                let first = if block.a == block.b { i } else { b.start };
                for j in first..b.end {
                    let (ii,jj)=(self.order[i],self.order[j]);
                    if (ii<ip)==(jj<ip) { continue; }
                    let k=self.pair_index(ii,jj);
                    let promoted=!self.promoted.is_empty() && (self.promoted[k/64]>>(k%64))&1!=0;
                    if (block.far || promoted)!=far {continue;}
                    f(ii,jj)?;
                }
            }
        }
        Ok(())
    }
    fn pair_index(&self,i:usize,j:usize)->usize {
        let (i,j)=(i.min(j),i.max(j));let n=self.moments.len();
        i*n-i*i.saturating_sub(1)/2+(j-i)
    }
    pub fn near_cache_upper_bytes(&self) -> Result<usize> {
        product(self.near_pairs, std::mem::size_of::<NearPair>())
    }
}

/// Deliberately small exact cache: only coupon baselines/near values, never ROI.
/// All expensive accepted-kernel evaluations are reused across policy controls.
pub struct ExactCache {
    pub prisms: Vec<Prism>,
    sources: Vec<PointPotential>,
    rule: GaussRule,
    values: Vec<f64>,
    pub evaluations: usize,
    pub evaluation_seconds: f64,
    pub planned_bytes: usize,
    started: Instant,
    budget: Budget,
}
impl ExactCache {
    pub fn new(prisms: Vec<Prism>, order: usize, budget: Budget) -> Result<Self> {
        let started = Instant::now();
        clock(started, budget)?;
        let n = prisms.len();
        if n == 0 || n > 64 {
            return Err(error(
                "OPERATOR_SCOPE",
                "exact dense cache limited to64 coupon supports; ROI planning only",
            ));
        }
        let count = product(n, n)?;
        if count > budget.max_pairs {
            return Err(error("MESH_LIMIT", "exact cache pair preflight"));
        }
        let bytes = checked(product(count, 8)?, product(n, 16384)?)?;
        memory(bytes, budget)?;
        let rule = GaussRule::new(order)?;
        let sources = prisms
            .iter()
            .map(PointPotential::new)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            prisms,
            sources,
            rule,
            values: vec![f64::NAN; count],
            evaluations: 0,
            evaluation_seconds: 0.,
            planned_bytes: bytes,
            started,
            budget,
        })
    }
    pub fn get(&mut self, i: usize, j: usize) -> Result<f64> {
        clock(self.started, self.budget)?;
        let n = self.prisms.len();
        if i >= n || j >= n {
            return Err(error("NEAR_FAR_INPUT", "exact cache index"));
        }
        let k = i * n + j;
        if self.values[k].is_nan() {
            let start = Instant::now();
            let v =
                super::polyhedron::volume_integral(&self.prisms[i], &self.sources[j], &self.rule)?;
            clock(self.started, self.budget)?;
            self.values[k] = v;
            self.evaluations += 1;
            self.evaluation_seconds += start.elapsed().as_secs_f64();
        }
        Ok(self.values[k])
    }
    pub fn dense(&mut self) -> Result<Vec<Vec<f64>>> {
        let n = self.prisms.len();
        let mut out = vec![vec![0.; n]; n];
        for i in 0..n {
            for j in 0..n {
                out[i][j] = self.get(i, j)?;
            }
        }
        Ok(out)
    }
}
#[derive(Clone, Copy, Debug)]
struct NearPair {
    i: usize,
    j: usize,
    forward: f64,
    reverse: f64,
}
pub struct Operator {
    pub plan: Plan,
    pub densities: Vec<Vec<[f64; 3]>>,
    near: Vec<NearPair>,
    pub truncation_rows_m5: Vec<f64>,
    pub bound_matrix_h: Vec<Vec<f64>>,
    pub far_pair_preparation_seconds: f64,
    pub planned_bytes: usize,
    started: Instant,
    budget: Budget,
}
pub struct Applied {
    pub response: Vec<f64>,
    pub integrated_potential: Vec<[f64; 3]>,
    pub absolute_far_potential_bounds: Vec<[f64; 3]>,
    pub energy_j: f64,
    pub absolute_truncation_energy_bound_j: f64,
}
impl Operator {
    pub fn new(
        cache: &mut ExactCache,
        densities: Vec<Vec<[f64; 3]>>,
        theta: f64,
        leaf: usize,
    ) -> Result<Self> {
        Self::new_policy(cache,densities,theta,leaf,true)
    }
    pub fn new_coarse(cache:&mut ExactCache,densities:Vec<Vec<[f64;3]>>,theta:f64,leaf:usize)->Result<Self> {
        Self::new_policy(cache,densities,theta,leaf,false)
    }
    fn new_policy(cache:&mut ExactCache,densities:Vec<Vec<[f64;3]>>,theta:f64,leaf:usize,individual:bool)->Result<Self> {
        let n = cache.prisms.len();
        let k = densities.first().map_or(0, Vec::len);
        if densities.len() != n
            || k == 0
            || k > 16
            || densities
                .iter()
                .any(|v| v.len() != k || v.iter().flatten().any(|x| !x.is_finite()))
        {
            return Err(error(
                "NEAR_FAR_INPUT",
                "finite complete physical basis1..16 required",
            ));
        }
        clock(cache.started, cache.budget)?;
        let plan = Plan::new_policy(&cache.prisms, theta, leaf, cache.budget,individual)?;
        let bytes = checked(
            cache.planned_bytes,
            checked(
                plan.planned_bytes,
                checked(
                    plan.near_cache_upper_bytes()?,
                    checked(product(n * k, 24)?, product(k * k, 8)?)?,
                )?,
            )?,
        )?;
        memory(bytes, cache.budget)?;
        let mut near = Vec::with_capacity(plan.near_pairs);
        plan.visit_pairs(false, |i, j| {
            clock(cache.started, cache.budget)?;
            let forward = cache.get(i, j)?;
            let reverse = if i == j { forward } else { cache.get(j, i)? };
            near.push(NearPair {
                i,
                j,
                forward,
                reverse,
            });
            Ok(())
        })?;
        let start = Instant::now();
        let mut rows = vec![0.; n];
        plan.visit_pairs(true, |i, j| {
            clock(cache.started, cache.budget)?;
            let a = &plan.moments[i];
            let b = &plan.moments[j];
            let value = a.pair(b)?;
            let r = value.absolute_remainder_m5;
            // Weighted Young inequality with weights1/volume. The resulting
            // sum of per-support Gram matrices bounds every physical vector.
            rows[i] += r * (a.volume_m3 / b.volume_m3);
            rows[j] += r * (b.volume_m3 / a.volume_m3);
            Ok(())
        })?;
        if rows.iter().any(|x| !x.is_finite() || *x < 0.) {
            return Err(error(
                "BOUND_RANGE",
                "aggregate truncation row outside finite nonnegative range",
            ));
        }
        let seconds = start.elapsed().as_secs_f64();
        let mut bound = vec![vec![0.; k]; k];
        for i in 0..n {
            for a in 0..k {
                for b in 0..k {
                    bound[a][b] += 1e-7 * rows[i] * dot(densities[i][a], densities[i][b]);
                }
            }
        }
        if bound.iter().flatten().any(|x| !x.is_finite()) {
            return Err(error(
                "BOUND_RANGE",
                "physical Gram bound outside finite range",
            ));
        }
        Ok(Self {
            plan,
            densities,
            near,
            truncation_rows_m5: rows,
            bound_matrix_h: bound,
            far_pair_preparation_seconds: seconds,
            planned_bytes: bytes,
            started: cache.started,
            budget: cache.budget,
        })
    }
    pub fn apply(&self, coefficients: &[f64]) -> Result<Applied> {
        clock(self.started, self.budget)?;
        let n = self.densities.len();
        let k = self.densities[0].len();
        if coefficients.len() != k || coefficients.iter().any(|x| !x.is_finite()) {
            return Err(error(
                "NEAR_FAR_INPUT",
                "complete finite physical current vector required",
            ));
        }
        let mut current = vec![[0.; 3]; n];
        for i in 0..n {
            for a in 0..k {
                for d in 0..3 {
                    current[i][d] += self.densities[i][a][d] * coefficients[a];
                }
            }
        }
        let mut potential = vec![[0.; 3]; n];
        let mut potential_bounds = vec![[0.; 3]; n];
        for p in &self.near {
            clock(self.started, self.budget)?;
            for d in 0..3 {
                potential[p.i][d] += 1e-7 * p.forward * current[p.j][d];
                if p.i != p.j {
                    potential[p.j][d] += 1e-7 * p.reverse * current[p.i][d];
                }
            }
        }
        self.plan.visit_pairs(true, |i, j| {
            clock(self.started, self.budget)?;
            let pair = self.plan.moments[i].pair(&self.plan.moments[j])?;
            let v = pair.integral_m5;
            for d in 0..3 {
                potential[i][d] += 1e-7 * v * current[j][d];
                potential[j][d] += 1e-7 * v * current[i][d];
                potential_bounds[i][d] += 1e-7 * pair.absolute_remainder_m5 * current[j][d].abs();
                potential_bounds[j][d] += 1e-7 * pair.absolute_remainder_m5 * current[i][d].abs();
            }
            Ok(())
        })?;
        let mut response = vec![0.; k];
        let mut energy = 0.;
        let mut bound = 0.;
        for i in 0..n {
            energy += 0.5 * dot(current[i], potential[i]);
            bound += 0.5e-7 * self.truncation_rows_m5[i] * dot(current[i], current[i]);
            for a in 0..k {
                response[a] += dot(self.densities[i][a], potential[i]);
            }
        }
        if !energy.is_finite()
            || !bound.is_finite()
            || response
                .iter()
                .chain(potential.iter().flatten())
                .chain(potential_bounds.iter().flatten())
                .any(|x| !x.is_finite())
        {
            return Err(error(
                "BOUND_RANGE",
                "near/far action or absolute bound outside finite range",
            ));
        }
        Ok(Applied {
            response,
            integrated_potential: potential,
            absolute_far_potential_bounds: potential_bounds,
            energy_j: energy,
            absolute_truncation_energy_bound_j: bound,
        })
    }
    pub fn matrix(&self) -> Result<Vec<Vec<f64>>> {
        let k = self.densities[0].len();
        let mut out = vec![vec![0.; k]; k];
        for a in 0..k {
            let mut unit = vec![0.; k];
            unit[a] = 1.;
            let v = self.apply(&unit)?;
            for b in 0..k {
                out[b][a] = v.response[b];
            }
        }
        Ok(out)
    }
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[cfg(test)]
mod mixed_moment_plan_tests {
    use super::*;
    #[test]
    fn excluded_suffix_covers_prefix_pairs_once() {
        let prism = |x: f64| Prism::new(format!("p{x}"), [[x,0.],[x+1.,0.],[x,1.]], [0.,1.]).unwrap();
        let mut moments: Vec<_> = [0., 7.].iter().map(|&x| Moments::new(&prism(x)).unwrap()).collect();
        for x in [2., 4., 9., 12.] {
            moments.push(Moments::tetrahedron([[x,0.,0.],[x+1.,0.,0.],[x,1.,0.],[x,0.,1.]]).unwrap());
        }
        // Two prefix supports: 3 PP and 8 PL pairs, no 10 TT pairs.
        let budget = Budget { max_bytes: 4_000_000, max_nodes: 32, max_blocks: 32, max_pairs: 11, max_wall: Duration::from_secs(10) };
        let full_budget = Budget { max_pairs: 21, ..budget };
        let empty = Plan::from_moments_excluding_suffix_pairs(moments.clone(), 0, 0.5, 3, full_budget).unwrap();
        assert_eq!(empty.near_pairs + empty.far_pairs, 0);
        let mut empty_visits = 0;
        for far in [false, true] { empty.visit_pairs(far, |_,_| { empty_visits += 1; Ok(()) }).unwrap(); }
        assert_eq!(empty_visits, 0);
        let all = Plan::from_moments_excluding_suffix_pairs(moments.clone(), 6, 0.5, 3, full_budget).unwrap();
        let baseline = Plan::from_moments(moments.clone(), 0.5, 3, full_budget).unwrap();
        let collect = |plan: &Plan| {
            let mut pairs = Vec::new();
            for far in [false, true] { plan.visit_pairs(far, |a,b| { pairs.push((a.min(b),a.max(b),far)); Ok(()) }).unwrap(); }
            pairs.sort_unstable(); pairs
        };
        assert_eq!(collect(&all), collect(&baseline));
        assert!(Plan::from_moments_excluding_suffix_pairs(moments.clone(), 7, 0.5, 3, full_budget).is_err());
        let limited = Plan::from_moments_excluding_suffix_pairs(moments.clone(), 2, 0.5, 3, budget).unwrap();
        assert!(limited.planned_bytes < all.planned_bytes);
        assert!(Plan::from_moments(moments.clone(), 0.5, 3, budget).is_err());
        for leaf in [1, 3, 6] {
            let plan = Plan::from_moments_excluding_suffix_pairs(moments.clone(), 2, 0.5, leaf, budget).unwrap();
            let mut got = Vec::new();
            for far in [false, true] { plan.visit_pairs(far, |a,b| { got.push((a.min(b),a.max(b))); Ok(()) }).unwrap(); }
            got.sort_unstable();
            let expected: Vec<_> = (0..2).flat_map(|i| (i..6).map(move |j| (i,j))).collect();
            assert_eq!(got, expected);
            assert_eq!(plan.near_pairs + plan.far_pairs, 11);
        }
    }
    #[test]
    fn prism_tetra_partition_preserves_pairs_and_far_bound() {
        let prism=Prism::new("p".into(),[[0.,0.],[1.,0.],[0.,1.]],[0.,1.]).unwrap();
        let tetra=[[8.,0.,0.],[9.,0.,0.],[8.,1.,0.],[8.,0.,1.]];
        let moments=vec![Moments::new(&prism).unwrap(),Moments::tetrahedron(tetra).unwrap()];
        let budget=Budget{max_bytes:4_000_000,max_nodes:32,max_blocks:32,max_pairs:3,max_wall:Duration::from_secs(10)};
        let plan=Plan::from_moments(moments,0.5,1,budget).unwrap();
        assert_eq!(plan.near_pairs+plan.far_pairs,3);
        let mut pairs=Vec::new();for far in [false,true] {plan.visit_pairs(far,|i,j|{pairs.push((i.min(j),i.max(j),far));Ok(())}).unwrap();}
        pairs.sort_unstable();assert_eq!(pairs.len(),3);assert_eq!(pairs.iter().filter(|p|p.0==0 && p.1==1).count(),1);
        let far=plan.moments[0].pair(&plan.moments[1]).unwrap();
        let q=[1.];let direct=super::super::planar_mixed::cross(&prism,tetra,super::super::planar_mixed::Densities{charge:&q,current:&[]},super::super::planar_mixed::Densities{charge:&q,current:&[]},1./(4.*std::f64::consts::PI),12,super::super::AllocationBudget{max_bytes:4_000_000,max_nodes:32,max_tetrahedra:32},Duration::from_secs(10)).unwrap().potential_per_farad[0][0];
        assert!((far.integral_m5-direct).abs()<=far.absolute_remainder_m5+1e-10*direct.abs());
    }
}
