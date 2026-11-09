//! Symmetric H-matrix (ACA-compressed) for the planar+barrel prism scalar
//! magnetic kernel K(a,b) [m^5]. Admissible blocks come from the coarse
//! near/far plan; the residual of every low-rank block is tracked so the
//! action can report a rigorous (Cauchy-Schwarz) per-support envelope.
use super::{error, far::Moments, near_far::{Budget, Plan}, planar_operator, prism::Prism, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(super) struct Params { pub theta: f64, pub eps: f64, pub threads: usize, pub leaf: usize }
fn env_f64(name: &str, default: f64) -> f64 { std::env::var(name).ok().and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.).unwrap_or(default) }
impl Params {
    pub(super) fn from_env() -> Self {
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
        let threads = std::env::var("P7_HMATRIX_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|t| *t >= 1).unwrap_or(cpus);
        Self { theta: env_f64("P7_HMATRIX_THETA", 0.6).min(0.99), eps: env_f64("P7_HMATRIX_EPS", 1e-6), threads, leaf: 32 }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Stats {
    pub supports: usize, pub far_blocks: usize, pub near_blocks: usize, pub exact_pairs: usize, pub moment_pairs: usize,
    pub stored_bytes: usize, pub max_rank: usize, pub build_seconds: f64, pub eps: f64,
    pub max_order: usize, pub max_change: f64, pub unconverged: usize, pub aca_dense_fallbacks: usize,
}
impl Stats {
    fn merge(&mut self, o: &Stats) {
        self.far_blocks += o.far_blocks; self.near_blocks += o.near_blocks; self.exact_pairs += o.exact_pairs; self.moment_pairs += o.moment_pairs;
        self.max_rank = self.max_rank.max(o.max_rank); self.max_order = self.max_order.max(o.max_order); self.max_change = self.max_change.max(o.max_change);
        self.unconverged += o.unconverged; self.aca_dense_fallbacks += o.aca_dense_fallbacks;
    }
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({"supports":self.supports,"farBlocks":self.far_blocks,"nearBlocks":self.near_blocks,"exactPairs":self.exact_pairs,"momentPairs":self.moment_pairs,"storedBytes":self.stored_bytes,"maxRank":self.max_rank,"buildSeconds":self.build_seconds,"eps":self.eps,"maxOrder":self.max_order,"maxRelativeChange":self.max_change,"unconvergedPairs":self.unconverged,"acaDenseFallbacks":self.aca_dense_fallbacks})
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
        let (v, order, change) = planar_operator::pair_integral_compact(self.prisms[a], self.prisms[b], 8, self.start, self.wall)?;
        st.exact_pairs += 1; st.max_order = st.max_order.max(order); st.max_change = st.max_change.max(change);
        if change > planar_operator::PAIR_TARGET_RELATIVE_CHANGE { st.unconverged += 1; }
        if !v.is_finite() { return Err(error("HYBRID_RANGE", "non-finite planar kernel entry")); }
        Ok(v)
    }
    fn time(&self) -> Result<()> { if self.start.elapsed() >= self.wall { Err(error("DEADLINE", "planar H-matrix build")) } else { Ok(()) } }
}

enum Store { Dense(Vec<f64>), Low { rank: usize, u: Vec<f64>, v: Vec<f64>, err: f64 } }
struct HBlock { a0: usize, a1: usize, b0: usize, b1: usize, diag: bool, store: Store }

pub(super) struct PlanarHMatrix { n: usize, order: Vec<usize>, blocks: Vec<HBlock>, pub stats: Stats }

fn dot(x: &[f64], y: &[f64]) -> f64 { x.iter().zip(y).map(|(a, b)| a * b).sum() }

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
        let pairs = n.checked_mul(n + 1).map(|v| v / 2).ok_or_else(|| error("MEMORY_LIMIT", "H-matrix pair overflow"))?;
        let plan = Plan::from_moments_coarse(moments, params.theta, params.leaf, Budget { max_bytes: max_bytes / 4, max_nodes: 2 * n, max_blocks: max_bytes / 4 / std::mem::size_of::<super::near_far::Block>(), max_pairs: pairs, max_wall: wall.saturating_sub(start.elapsed()) })?;
        let limit = max_bytes.saturating_sub(plan.planned_bytes);
        let kernel = Kernel { prisms, moments: &plan.moments, start, wall };
        let used = AtomicUsize::new(0);
        let charge = |bytes: usize| -> Result<()> {
            let total = used.fetch_add(bytes, Ordering::Relaxed) + bytes;
            if total > limit { Err(error("MEMORY_LIMIT", &format!("planar H-matrix storage {total} bytes exceeds {limit} bytes (plan {} of {max_bytes})", plan.planned_bytes))) } else { Ok(()) }
        };
        use rayon::prelude::*;
        let pool = rayon::ThreadPoolBuilder::new().num_threads(params.threads.max(1)).stack_size(4 * 1024 * 1024).build().map_err(|_| error("MEMORY_LIMIT", "H-matrix worker pool creation failed"))?;
        let order = &plan.order;
        let results: Result<Vec<(HBlock, Stats)>> = pool.install(|| plan.blocks.par_iter().map(|b| {
            let (na, nb) = (&plan.nodes[b.a], &plan.nodes[b.b]);
            let rows: Vec<usize> = order[na.start..na.end].to_vec();
            let cols: Vec<usize> = order[nb.start..nb.end].to_vec();
            let mut st = Stats::default();
            let diag = b.a == b.b;
            let mut store = None;
            if b.far && !diag {
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
            Ok((HBlock { a0: na.start, a1: na.end, b0: nb.start, b1: nb.end, diag, store }, st))
        }).collect());
        let results = results?;
        let mut stats = Stats { supports: n, eps: params.eps, ..Default::default() };
        let mut blocks = Vec::with_capacity(results.len());
        for (b, st) in results { stats.merge(&st); blocks.push(b); }
        stats.stored_bytes = used.load(Ordering::Relaxed);
        stats.build_seconds = t0.elapsed().as_secs_f64();
        Ok(Self { n, order: plan.order.clone(), blocks, stats })
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
        assert!(hm.stats.stored_bytes * 10 < dense_bytes * 9, "{} vs {}", hm.stats.stored_bytes, dense_bytes);
    }
}
