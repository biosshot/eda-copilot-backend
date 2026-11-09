//! Symmetric H-matrix (ACA-compressed) for the planar+barrel prism scalar
//! magnetic kernel K(a,b) [m^5]. Admissible blocks come from the coarse
//! near/far plan; the residual of every low-rank block is tracked so the
//! action can report a rigorous (Cauchy-Schwarz) per-support envelope.
use super::{error, far::Moments, planar_operator, prism::Prism, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(super) struct Params { /** Admissibility eta: min(diam A, diam B) <= eta * box gap. */ pub theta: f64, pub eps: f64, pub threads: usize, pub leaf: usize }
fn env_f64(name: &str, default: f64) -> f64 { std::env::var(name).ok().and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.).unwrap_or(default) }
impl Params {
    pub(super) fn from_env() -> Self {
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_sub(1).clamp(1, 11);
        let threads = std::env::var("P7_HMATRIX_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|t| *t >= 1).unwrap_or(cpus);
        Self { theta: env_f64("P7_HMATRIX_ETA", 1.0), eps: env_f64("P7_HMATRIX_EPS", 1e-6), threads, leaf: 16 }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Stats {
    pub supports: usize, pub far_blocks: usize, pub near_blocks: usize, pub exact_pairs: usize, pub moment_pairs: usize,
    pub stored_bytes: usize, pub max_rank: usize, pub build_seconds: f64, pub eps: f64,
    pub max_order: usize, pub max_change: f64, pub unconverged: usize, pub approximated: usize, pub aca_dense_fallbacks: usize, pub fast_pairs: usize, pub slow_ns: u64,
}
impl Stats {
    fn merge(&mut self, o: &Stats) {
        self.far_blocks += o.far_blocks; self.near_blocks += o.near_blocks; self.exact_pairs += o.exact_pairs; self.fast_pairs += o.fast_pairs;self.moment_pairs += o.moment_pairs;
        self.max_rank = self.max_rank.max(o.max_rank); self.max_order = self.max_order.max(o.max_order); self.max_change = self.max_change.max(o.max_change);
        self.slow_ns += o.slow_ns; self.unconverged += o.unconverged; self.approximated += o.approximated; self.aca_dense_fallbacks += o.aca_dense_fallbacks;
    }
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({"supports":self.supports,"farBlocks":self.far_blocks,"nearBlocks":self.near_blocks,"exactPairs":self.exact_pairs,"fastPairs":self.fast_pairs,"momentPairs":self.moment_pairs,"storedBytes":self.stored_bytes,"maxRank":self.max_rank,"buildSeconds":self.build_seconds,"eps":self.eps,"maxOrder":self.max_order,"maxRelativeChange":self.max_change,"unconvergedPairs":self.unconverged,"approximatedPairs":self.approximated,"acaDenseFallbacks":self.aca_dense_fallbacks})
    }
}

pub(super) struct Kernel<'a> { pub prisms: &'a [&'a Prism], pub moments: &'a [Moments], pub start: Instant, pub wall: Duration }
impl Kernel<'_> {
    pub(super) fn entry(&self, a: usize, b: usize, st: &mut Stats) -> Result<f64> {
        let (ma, mb) = (&self.moments[a], &self.moments[b]);
        let dist = (ma.center_m[0] - mb.center_m[0]).hypot(ma.center_m[1] - mb.center_m[1]).hypot(ma.center_m[2] - mb.center_m[2]);
        if a != b && (ma.radius_m + mb.radius_m) / dist <= 0.2 {
            if let Ok(p) = ma.pair(mb) { st.moment_pairs += 1; return Ok(p.integral_m5); }
        }
        self.exact(a, b, st)
    }
    fn exact(&self, a: usize, b: usize, st: &mut Stats) -> Result<f64> {
        let begun = Instant::now();
        let (v, order, change) = planar_operator::pair_integral_compact(self.prisms[a], self.prisms[b], 8, self.start, self.wall)?;
        st.exact_pairs += 1; if order <= 5 { st.fast_pairs += 1; } else { st.slow_ns += begun.elapsed().as_nanos() as u64; } st.max_order = st.max_order.max(order); st.max_change = st.max_change.max(change);
        if change > planar_operator::PAIR_TARGET_RELATIVE_CHANGE { st.unconverged += 1; }
        if change > planar_operator::PAIR_NOISE_ACCEPT_CHANGE { st.approximated += 1; }
        if !v.is_finite() { return Err(error("HYBRID_RANGE", "non-finite planar kernel entry")); }
        Ok(v)
    }
    fn time(&self) -> Result<()> { if self.start.elapsed() >= self.wall { Err(error("DEADLINE", "planar H-matrix build")) } else { Ok(()) } }
}

enum Store { Dense(Vec<f64>), Low { rank: usize, u: Vec<f64>, v: Vec<f64>, err: f64 } }
struct HBlock { a0: usize, a1: usize, b0: usize, b1: usize, diag: bool, store: Store }

pub(super) struct PlanarHMatrix { n: usize, order: Vec<usize>, blocks: Vec<HBlock>, pub stats: Stats }

fn dot(x: &[f64], y: &[f64]) -> f64 { x.iter().zip(y).map(|(a, b)| a * b).sum() }

// Cluster boxes use true prism extents: bounding balls of needle triangles and
// tall barrel prisms inflate clusters so that almost nothing becomes admissible.
#[derive(Clone, Copy)]
struct Bbox { lo: [f64; 3], hi: [f64; 3] }
impl Bbox {
    fn of(p: &Prism) -> Self {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in p.triangle_m { for k in 0..2 { lo[k] = lo[k].min(v[k]); hi[k] = hi[k].max(v[k]); } }
        lo[2] = p.z_m[0].min(p.z_m[1]); hi[2] = p.z_m[0].max(p.z_m[1]);
        Self { lo, hi }
    }
    fn join(self, o: Self) -> Self { Self { lo: std::array::from_fn(|k| self.lo[k].min(o.lo[k])), hi: std::array::from_fn(|k| self.hi[k].max(o.hi[k])) } }
    fn diam(self) -> f64 { (self.hi[0] - self.lo[0]).hypot(self.hi[1] - self.lo[1]).hypot(self.hi[2] - self.lo[2]) }
    fn gap(self, o: Self) -> f64 { (0..3).map(|k| { let d = (self.lo[k] - o.hi[k]).max(o.lo[k] - self.hi[k]).max(0.); d * d }).sum::<f64>().sqrt() }
}
struct Cluster { start: usize, end: usize, bbox: Bbox, children: Option<[usize; 2]> }
#[derive(Clone, Copy)]
struct Pairing { a: usize, b: usize, far: bool }

fn split(nodes: &mut Vec<Cluster>, order: &mut [usize], boxes: &[Bbox], start: usize, end: usize, leaf: usize) -> usize {
    let bbox = order[start..end].iter().map(|&i| boxes[i]).reduce(Bbox::join).expect("nonempty cluster");
    let id = nodes.len();
    nodes.push(Cluster { start, end, bbox, children: None });
    if end - start > leaf {
        let axis = (0..3).max_by(|&a, &b| (bbox.hi[a] - bbox.lo[a]).total_cmp(&(bbox.hi[b] - bbox.lo[b]))).unwrap();
        let middle = (end - start) / 2;
        let center = |i: usize| boxes[i].lo[axis] + boxes[i].hi[axis];
        order[start..end].select_nth_unstable_by(middle, |&i, &j| center(i).total_cmp(&center(j)).then(i.cmp(&j)));
        let left = split(nodes, order, boxes, start, start + middle, leaf);
        let right = split(nodes, order, boxes, start + middle, end, leaf);
        nodes[id].children = Some([left, right]);
    }
    id
}
fn pair_up(nodes: &[Cluster], a: usize, b: usize, eta: f64, out: &mut Vec<Pairing>) {
    let (ca, cb) = (&nodes[a], &nodes[b]);
    if a == b {
        match ca.children {
            Some([l, r]) => { pair_up(nodes, l, l, eta, out); pair_up(nodes, l, r, eta, out); pair_up(nodes, r, r, eta, out); }
            None => out.push(Pairing { a, b, far: false }),
        }
        return;
    }
    let (da, db, gap) = (ca.bbox.diam(), cb.bbox.diam(), ca.bbox.gap(cb.bbox));
    if gap > 0. && da.min(db) <= eta * gap { out.push(Pairing { a, b, far: true }); return; }
    match (ca.children, cb.children) {
        (None, None) => out.push(Pairing { a, b, far: false }),
        (Some([l, r]), None) => { pair_up(nodes, l, b, eta, out); pair_up(nodes, r, b, eta, out); }
        (None, Some([l, r])) => { pair_up(nodes, a, l, eta, out); pair_up(nodes, a, r, eta, out); }
        (Some([l, r]), Some(_)) if da >= db => { pair_up(nodes, l, b, eta, out); pair_up(nodes, r, b, eta, out); }
        (Some(_), Some([l, r])) => { pair_up(nodes, a, l, eta, out); pair_up(nodes, a, r, eta, out); }
    }
}

/// ACA with partial pivoting. None means "not converged or not worthwhile: use dense".
fn aca(k: &Kernel, rows: &[usize], cols: &[usize], eps: f64, st: &mut Stats) -> Result<Option<(usize, Vec<f64>, Vec<f64>, f64)>> {
    let (m, n) = (rows.len(), cols.len());
    let cap = m.min(n).min(64).min(m * n / (m + n));
    if cap < 1 { return Ok(None); }
    let (mut u, mut v): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let (mut ur, mut uc) = (vec![false; m], vec![false; n]);
    let (mut rank, mut i, mut norm2, mut small, mut zeros, mut err) = (0usize, 0usize, 0f64, 0, 0, 0f64);
    loop {
        if rank >= cap { return Ok(None); }
        k.time()?;
        let mut row = Vec::with_capacity(n);
        for j in 0..n { row.push(k.entry(rows[i], cols[j], st)?); }
        for l in 0..rank { let c = u[l * m + i]; if c != 0. { for j in 0..n { row[j] -= c * v[l * n + j]; } } }
        ur[i] = true;
        let (mut pj, mut pv) = (usize::MAX, 0f64);
        for j in 0..n { if !uc[j] && row[j].abs() > pv { pv = row[j].abs(); pj = j; } }
        if pj == usize::MAX {
            zeros += 1;
            match (0..m).find(|&r| !ur[r]) { Some(r) if zeros < 3 => { i = r; continue; } _ => { err = 0.; break; } }
        }
        zeros = 0;
        let pivot = row[pj];
        for x in row.iter_mut() { *x /= pivot; }
        let mut col = Vec::with_capacity(m);
        for ii in 0..m { col.push(k.entry(rows[ii], cols[pj], st)?); }
        for l in 0..rank { let c = v[l * n + pj]; if c != 0. { for ii in 0..m { col[ii] -= c * u[l * m + ii]; } } }
        let (nu, nv) = (dot(&col, &col).sqrt(), dot(&row, &row).sqrt());
        let cross: f64 = (0..rank).map(|l| dot(&u[l * m..(l + 1) * m], &col) * dot(&v[l * n..(l + 1) * n], &row)).sum();
        norm2 = (norm2 + nu * nu * nv * nv + 2. * cross).max(0.);
        if !norm2.is_finite() { return Err(error("HYBRID_RANGE", "ACA norm overflow")); }
        err = nu * nv;
        uc[pj] = true; rank += 1;
        let next = (0..m).filter(|&r| !ur[r]).max_by(|&a, &b| col[a].abs().total_cmp(&col[b].abs()));
        u.extend_from_slice(&col); v.extend_from_slice(&row);
        if err <= eps * norm2.sqrt() { small += 1; } else { small = 0; }
        if small >= 2 { break; }
        match next { Some(r) => i = r, None => { err = 0.; break; } }
    }
    Ok(Some((rank, u, v, err)))
}

impl PlanarHMatrix {
    pub(super) fn build(prisms: &[&Prism], max_bytes: usize, params: Params, start: Instant, wall: Duration) -> Result<Self> {
        let t0 = Instant::now();
        let n = prisms.len();
        let moments = prisms.iter().map(|p| Moments::new(p)).collect::<Result<Vec<_>>>()?;
        if n == 0 { return Err(error("HYBRID_INPUT", "H-matrix needs supports")); }
        let boxes: Vec<Bbox> = prisms.iter().map(|p| Bbox::of(p)).collect();
        let mut tree_order: Vec<usize> = (0..n).collect();
        let mut nodes = Vec::with_capacity(2 * n);
        let root = split(&mut nodes, &mut tree_order, &boxes, 0, n, params.leaf);
        let mut pairings = Vec::new();
        pair_up(&nodes, root, root, params.theta, &mut pairings);
        let tree_bytes = nodes.len() * std::mem::size_of::<Cluster>() + pairings.len() * std::mem::size_of::<Pairing>() + n * std::mem::size_of::<Bbox>();
        if tree_bytes > max_bytes { return Err(error("MEMORY_LIMIT", &format!("planar H-matrix tree {tree_bytes} bytes exceeds {max_bytes} bytes"))); }
        let limit = max_bytes - tree_bytes;
        let kernel = Kernel { prisms, moments: &moments, start, wall };
        let used = AtomicUsize::new(0);
        let charge = |bytes: usize| -> Result<()> {
            let total = used.fetch_add(bytes, Ordering::Relaxed) + bytes;
            if total > limit { Err(error("MEMORY_LIMIT", &format!("planar H-matrix storage {total} bytes exceeds {limit} bytes (tree {tree_bytes} of {max_bytes})"))) } else { Ok(()) }
        };
        use rayon::prelude::*;
        let pool = rayon::ThreadPoolBuilder::new().num_threads(params.threads.max(1)).stack_size(4 * 1024 * 1024).build().map_err(|_| error("MEMORY_LIMIT", "H-matrix worker pool creation failed"))?;
        let order = &tree_order;
        let total_blocks = pairings.len();
        let progress_step = (total_blocks / 10).max(1);
        let blocks_done = AtomicUsize::new(0);
        let exact_done = AtomicUsize::new(0);
        let (fast_done, slow_ns_done, near_done, fallback_done) = (AtomicUsize::new(0), std::sync::atomic::AtomicU64::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
        let results: Result<Vec<(HBlock, Stats)>> = pool.install(|| pairings.par_iter().map(|b| {
            let (na, nb) = (&nodes[b.a], &nodes[b.b]);
            let rows: Vec<usize> = order[na.start..na.end].to_vec();
            let cols: Vec<usize> = order[nb.start..nb.end].to_vec();
            let mut st = Stats::default();
            let diag = b.a == b.b;
            let mut store = None;
            // Small blocks rarely compress at this tolerance; a failed ACA would be thrown away.
            if b.far && !diag && rows.len() * cols.len() > 1024 {
                if let Some((rank, u, v, err)) = aca(&kernel, &rows, &cols, params.eps, &mut st)? {
                    charge(8 * (u.len() + v.len()))?; st.far_blocks = 1; st.max_rank = rank;
                    store = Some(Store::Low { rank, u, v, err });
                } else { st.aca_dense_fallbacks = 1; }
            }
            let store = match store { Some(s) => s, None => {
                let m = rows.len();
                let mut d = Vec::new();
                if diag {
                    charge(8 * m * (m + 1) / 2)?; d.reserve(m * (m + 1) / 2);
                    for i in 0..m { kernel.time()?; for j in i..m { d.push(kernel.entry(rows[i], rows[j], &mut st)?); } }
                } else {
                    charge(8 * m * cols.len())?; d.reserve(m * cols.len());
                    for &r in &rows { kernel.time()?; for &c in &cols { d.push(kernel.entry(r, c, &mut st)?); } }
                }
                st.near_blocks = 1; Store::Dense(d)
            } };
            let exact_total = exact_done.fetch_add(st.exact_pairs, Ordering::Relaxed) + st.exact_pairs;
            fast_done.fetch_add(st.fast_pairs, Ordering::Relaxed); slow_ns_done.fetch_add(st.slow_ns, Ordering::Relaxed); near_done.fetch_add(st.near_blocks, Ordering::Relaxed); fallback_done.fetch_add(st.aca_dense_fallbacks, Ordering::Relaxed);
            let done = blocks_done.fetch_add(1, Ordering::Relaxed) + 1;
            if done % progress_step == 0 || done == total_blocks {
                eprintln!("P7_COMPACT_HMATRIX_PROGRESS done={} of={} elapsed={:.1} exact={} fast={} slowCpuS={:.1} nearBlocks={} acaFallbacks={}", done, total_blocks, t0.elapsed().as_secs_f64(), exact_total, fast_done.load(Ordering::Relaxed), slow_ns_done.load(Ordering::Relaxed) as f64 * 1e-9, near_done.load(Ordering::Relaxed), fallback_done.load(Ordering::Relaxed));
            }
            Ok((HBlock { a0: na.start, a1: na.end, b0: nb.start, b1: nb.end, diag, store }, st))
        }).collect());
        let results = results?;
        let mut stats = Stats { supports: n, eps: params.eps, ..Default::default() };
        let mut blocks = Vec::with_capacity(results.len());
        for (b, st) in results { stats.merge(&st); blocks.push(b); }
        stats.stored_bytes = used.load(Ordering::Relaxed);
        stats.build_seconds = t0.elapsed().as_secs_f64();
        Ok(Self { n, order: tree_order, blocks, stats })
    }

    /// Returns (potential, envelope); the operator is exactly symmetric and the
    /// envelope per support/component is sum of block residual estimates times
    /// the 2-norm of the other block's input (Cauchy-Schwarz).
    pub(super) fn apply(&self, rho: &[[f64; 3]]) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
        assert_eq!(rho.len(), self.n);
        let x: Vec<[f64; 3]> = self.order.iter().map(|&s| rho[s]).collect();
        let mut y = vec![[0f64; 3]; self.n];
        let mut e = vec![[0f64; 3]; self.n];
        let norm = |r: std::ops::Range<usize>| { let mut s = [0f64; 3]; for p in r { for k in 0..3 { s[k] += x[p][k] * x[p][k]; } } s.map(f64::sqrt) };
        for b in &self.blocks {
            let (m, nn) = (b.a1 - b.a0, b.b1 - b.b0);
            match &b.store {
                Store::Dense(d) if b.diag => {
                    let mut at = 0;
                    for i in 0..m { for j in i..m {
                        let a = d[at]; at += 1;
                        for k in 0..3 { y[b.a0 + i][k] += a * x[b.a0 + j][k]; if j != i { y[b.a0 + j][k] += a * x[b.a0 + i][k]; } }
                    } }
                }
                Store::Dense(d) => for i in 0..m { for j in 0..nn {
                    let a = d[i * nn + j];
                    for k in 0..3 { y[b.a0 + i][k] += a * x[b.b0 + j][k]; y[b.b0 + j][k] += a * x[b.a0 + i][k]; }
                } },
                Store::Low { rank, u, v, err } => {
                    for l in 0..*rank {
                        let (ul, vl) = (&u[l * m..(l + 1) * m], &v[l * nn..(l + 1) * nn]);
                        let (mut t, mut s) = ([0f64; 3], [0f64; 3]);
                        for j in 0..nn { for k in 0..3 { t[k] += vl[j] * x[b.b0 + j][k]; } }
                        for i in 0..m { for k in 0..3 { s[k] += ul[i] * x[b.a0 + i][k]; } }
                        for i in 0..m { for k in 0..3 { y[b.a0 + i][k] += ul[i] * t[k]; } }
                        for j in 0..nn { for k in 0..3 { y[b.b0 + j][k] += vl[j] * s[k]; } }
                    }
                    if *err > 0. {
                        let (xa, xb) = (norm(b.a0..b.a1), norm(b.b0..b.b1));
                        for i in b.a0..b.a1 { for k in 0..3 { e[i][k] += err * xb[k]; } }
                        for j in b.b0..b.b1 { for k in 0..3 { e[j][k] += err * xa[k]; } }
                    }
                }
            }
        }
        let (mut py, mut pe) = (vec![[0f64; 3]; self.n], vec![[0f64; 3]; self.n]);
        for (p, &s) in self.order.iter().enumerate() { py[s] = y[p]; pe[s] = e[p]; }
        (py, pe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn grid() -> Vec<Prism> {
        let (h, pitch) = (0.5e-3, 1.5e-3); let mut out = Vec::new();
        for gx in 0..25 { for gy in 0..12 {
            let (x, y) = (gx as f64 * pitch, gy as f64 * pitch);
            out.push(Prism::new(format!("a{gx}_{gy}"), [[x, y], [x + h, y], [x, y + h]], [0., 35e-6]).unwrap());
            out.push(Prism::new(format!("b{gx}_{gy}"), [[x + h, y], [x + h, y + h], [x, y + h]], [0., 35e-6]).unwrap());
        } }
        out
    }
    #[test]
    fn planar_hmatrix_matches_dense_symmetric_and_compact() {
        let prisms = grid(); let refs: Vec<&Prism> = prisms.iter().collect(); let n = refs.len();
        assert_eq!(n, 600);
        let (start, wall) = (Instant::now(), Duration::from_secs(600));
        let hm = PlanarHMatrix::build(&refs, 1 << 30, Params { theta: 0.6, eps: 1e-6, threads: 4, leaf: 32 }, start, wall).unwrap();
        assert!(hm.stats.far_blocks > 0, "{:?}", hm.stats);
        let rho = |seed: f64| -> Vec<[f64; 3]> { (0..n).map(|i| std::array::from_fn(|k| ((i as f64 + 1.) * (k as f64 + 1.7) * seed).sin())).collect() };
        let (x, y) = (rho(0.37), rho(1.91));
        let (ax, _env) = hm.apply(&x); let (ay, _) = hm.apply(&y);
        let moments: Vec<Moments> = refs.iter().map(|p| Moments::new(p).unwrap()).collect();
        let kernel = Kernel { prisms: &refs, moments: &moments, start, wall };
        let mut st = Stats::default();
        let mut dense = vec![[0f64; 3]; n];
        for a in 0..n { for b in a..n {
            let v = kernel.entry(a, b, &mut st).unwrap();
            for k in 0..3 { dense[a][k] += v * x[b][k]; if a != b { dense[b][k] += v * x[a][k]; } }
        } }
        let num: f64 = (0..n).flat_map(|i| (0..3).map(move |k| (i, k))).map(|(i, k)| (ax[i][k] - dense[i][k]).powi(2)).sum();
        let den: f64 = dense.iter().flatten().map(|v| v * v).sum();
        assert!((num / den).sqrt() < 1e-4, "rel err {}", (num / den).sqrt());
        let xay: f64 = (0..n).map(|i| (0..3).map(|k| x[i][k] * ay[i][k]).sum::<f64>()).sum();
        let yax: f64 = (0..n).map(|i| (0..3).map(|k| y[i][k] * ax[i][k]).sum::<f64>()).sum();
        assert!((xay - yax).abs() <= 1e-12 * xay.abs().max(yax.abs()), "{xay} {yax}");
        let dense_bytes = n * (n + 1) / 2 * 8;
        assert!(hm.stats.stored_bytes <= dense_bytes * 2, "{} vs {}", hm.stats.stored_bytes, dense_bytes);
    }
}
