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
    pub stored_bytes: usize, pub dense_bytes: usize, pub low_rank_bytes: usize, pub cache: &'static str, pub max_rank: usize, pub build_seconds: f64, pub eps: f64,
    pub threads: usize, pub max_order: usize, pub max_change: f64, pub unconverged: usize, pub approximated: usize, pub aca_dense_fallbacks: usize, pub fast_pairs: usize, pub slow_ns: u64, pub cpu_ns: u64,
    /** Diagnostic only (`P7_HMATRIX_PAIR_PROBE=1`): exact pair entries by contact class and aspect ratio. */
    pub probe: Option<Box<PairProbe>>,
}
impl Stats {
    fn merge(&mut self, o: &Stats) {
        self.far_blocks += o.far_blocks; self.near_blocks += o.near_blocks; self.exact_pairs += o.exact_pairs; self.fast_pairs += o.fast_pairs;self.moment_pairs += o.moment_pairs;
        self.max_rank = self.max_rank.max(o.max_rank); self.max_order = self.max_order.max(o.max_order); self.max_change = self.max_change.max(o.max_change);
        self.slow_ns += o.slow_ns; self.cpu_ns += o.cpu_ns; self.unconverged += o.unconverged; self.approximated += o.approximated; self.aca_dense_fallbacks += o.aca_dense_fallbacks;
        if let Some(p) = &o.probe { self.probe.get_or_insert_with(Default::default).merge(p); }
    }
    pub(super) fn json(&self) -> serde_json::Value {
        let mut j = serde_json::json!({"supports":self.supports,"farBlocks":self.far_blocks,"nearBlocks":self.near_blocks,"exactPairs":self.exact_pairs,"fastPairs":self.fast_pairs,"momentPairs":self.moment_pairs,"storedBytes":self.stored_bytes,"denseBytes":self.dense_bytes,"lowRankBytes":self.low_rank_bytes,"hmatrixCache":self.cache,"threads":self.threads,"maxRank":self.max_rank,"buildSeconds":self.build_seconds,"eps":self.eps,"maxOrder":self.max_order,"maxRelativeChange":self.max_change,"unconvergedPairs":self.unconverged,"approximatedPairs":self.approximated,"acaDenseFallbacks":self.aca_dense_fallbacks,"cpuSeconds":self.cpu_ns as f64*1e-9});
        if let Some(p) = &self.probe { j["pairProbe"] = p.json(); }
        j
    }
}

/// `P7_HMATRIX_PAIR_PROBE=1`: classify every exact pair entry (diagnostic only, values unchanged; bypasses the disk cache).
fn probe_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("P7_HMATRIX_PAIR_PROBE").as_deref() == Ok("1"))
}
const PROBE_CLASSES: [&str; 7] = ["same-footprint", "shared-edge", "shared-vertex", "touching-other", "gap<0.5d", "gap<2d", "gap>=2d"];
const PROBE_ASPECTS: [&str; 4] = ["<10", "10-100", "100-1000", ">=1000"];
#[derive(Clone, Copy, Debug, Default)]
struct ProbeCell { n: u64, n_unc: u64, n_noise: u64, sum: f64, sum_unc: f64, err: f64, err_unc: f64, max_change: f64 }
#[derive(Clone, Debug, Default)]
pub(super) struct PairProbe { cells: [[ProbeCell; 4]; 7] }
impl PairProbe {
    fn add(&mut self, class: usize, aspect: usize, v: f64, change: f64) {
        let c = &mut self.cells[class][aspect];
        let (m, e) = (v.abs(), v.abs() * change);
        c.n += 1; c.sum += m; c.err += e; c.max_change = c.max_change.max(change);
        if change > planar_operator::PAIR_NOISE_ACCEPT_CHANGE { c.n_noise += 1; }
        if change > planar_operator::PAIR_TARGET_RELATIVE_CHANGE { c.n_unc += 1; c.sum_unc += m; c.err_unc += e; }
    }
    fn merge(&mut self, o: &PairProbe) {
        for (x, y) in self.cells.iter_mut().flatten().zip(o.cells.iter().flatten()) {
            x.n += y.n; x.n_unc += y.n_unc; x.n_noise += y.n_noise; x.sum += y.sum; x.sum_unc += y.sum_unc; x.err += y.err; x.err_unc += y.err_unc; x.max_change = x.max_change.max(y.max_change);
        }
    }
    fn json(&self) -> serde_json::Value {
        let mut rows = Vec::new();
        for (ci, row) in self.cells.iter().enumerate() { for (ai, c) in row.iter().enumerate() {
            if c.n == 0 { continue; }
            rows.push(serde_json::json!({"class":PROBE_CLASSES[ci],"aspect":PROBE_ASPECTS[ai],"n":c.n,"unconverged":c.n_unc,"aboveNoise":c.n_noise,"sumAbs":c.sum,"sumAbsUnconverged":c.sum_unc,"errAbs":c.err,"errAbsUnconverged":c.err_unc,"maxChange":c.max_change}));
        } }
        serde_json::json!({"note":"counts exact() entries (ACA samples included, a pair may repeat); sums are |pair integral| in m^5 and |integral|*relative change","rows":rows})
    }
}
/// Contact class (3-D: shared xy vertices only count when the z intervals touch) and aspect bucket of a pair.
fn probe_class(a: &Prism, b: &Prism) -> (usize, usize) {
    const TOL: f64 = 1e-9;
    let (ba, bb) = (Bbox::of(a), Bbox::of(b));
    let z_gap = (a.z_m[0].min(a.z_m[1]).max(b.z_m[0].min(b.z_m[1])) - a.z_m[0].max(a.z_m[1]).min(b.z_m[0].max(b.z_m[1]))).max(0.);
    let shared = a.triangle_m.iter().filter(|p| b.triangle_m.iter().any(|q| (p[0] - q[0]).abs() <= TOL && (p[1] - q[1]).abs() <= TOL)).count();
    let gap = ba.gap(bb);
    let dmin = ba.diam().min(bb.diam()).max(1e-30);
    let class = if z_gap <= TOL && shared >= 3 { 0 } else if z_gap <= TOL && shared == 2 { 1 } else if z_gap <= TOL && shared == 1 { 2 }
        else if gap <= TOL { 3 }
        else if gap < 0.5 * dmin { 4 } else if gap < 2. * dmin { 5 } else { 6 };
    let aspect = |p: &Prism| {
        let t = &p.triangle_m;
        let e = [(t[1][0] - t[0][0]).hypot(t[1][1] - t[0][1]), (t[2][0] - t[1][0]).hypot(t[2][1] - t[1][1]), (t[0][0] - t[2][0]).hypot(t[0][1] - t[2][1])];
        let lmax = e[0].max(e[1]).max(e[2]);
        let area = 0.5 * ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1])).abs();
        let alt = if lmax > 0. { 2. * area / lmax } else { 0. };
        let h = (p.z_m[1] - p.z_m[0]).abs();
        lmax.max(h) / alt.min(h).max(1e-30)
    };
    let r = aspect(a).max(aspect(b));
    (class, if r < 10. { 0 } else if r < 100. { 1 } else if r < 1000. { 2 } else { 3 })
}

pub(super) struct Kernel<'a> { pub prisms: &'a [&'a Prism], pub moments: &'a [Moments], pub start: Instant, pub wall: Duration }
impl Kernel<'_> {
    pub(super) fn entry(&self, a: usize, b: usize, st: &mut Stats) -> Result<f64> {
        let (ma, mb) = (&self.moments[a], &self.moments[b]);
        let dist = (ma.center_m[0] - mb.center_m[0]).hypot(ma.center_m[1] - mb.center_m[1]).hypot(ma.center_m[2] - mb.center_m[2]);
        if a != b && (ma.radius_m + mb.radius_m) / dist <= 0.2 {
            let _g = planar_operator::ProfGuard::new(0);
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
        if probe_on() { let (class, aspect) = probe_class(self.prisms[a], self.prisms[b]); st.probe.get_or_insert_with(Default::default).add(class, aspect, v, change); }
        if !v.is_finite() { return Err(error("HYBRID_RANGE", "non-finite planar kernel entry")); }
        Ok(v)
    }
    fn time(&self) -> Result<()> { if self.start.elapsed() >= self.wall { Err(error("DEADLINE", "planar H-matrix build")) } else { Ok(()) } }
}

enum Store { Dense(Vec<f64>), Low { rank: usize, u: Vec<f64>, v: Vec<f64>, err: f64 } }
struct HBlock { a0: usize, a1: usize, b0: usize, b1: usize, diag: bool, store: Store }

pub(super) struct PlanarHMatrix { n: usize, order: Vec<usize>, blocks: Vec<HBlock>, pub stats: Stats }

/// Workers worth starting for `entries` table entries per pass: a fork-join costs a scheduler
/// round trip per thread (tens of milliseconds on a loaded machine), so small tables stay inline.
pub(super) fn workers_for(threads: usize, entries: usize, entries_per_worker: usize) -> usize { threads.max(1).min(entries / entries_per_worker.max(1)).max(1) }
/// Runs `f(worker, workers)` on `workers` scoped threads and collects the results in worker order; a single
/// worker runs inline. Callers stripe their work by worker index (not a shared counter), so each worker's
/// accumulation order, and hence the summed result, is the same on every run.
pub(super) fn run_workers<T: Send>(workers: usize, f: impl Fn(usize, usize) -> T + Sync) -> Vec<T> {
    if workers <= 1 { return vec![f(0, 1)]; }
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = (0..workers).map(|w| scope.spawn(move || f(w, workers))).collect();
        handles.into_iter().map(|h| h.join().expect("worker thread")).collect()
    })
}

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
    /// Builds the H-matrix, or loads it from the disk cache (default on, `P7_HMATRIX_CACHE=0` disables).
    pub(super) fn build(prisms: &[&Prism], max_bytes: usize, params: Params, start: Instant, wall: Duration) -> Result<Self> {
        // The probe needs a real build (a cache hit evaluates no pairs).
        let dir = if probe_on() { None } else { disk_cache::directory() };
        let Some(dir) = dir else { return Self::build_uncached(prisms, max_bytes, params, start, wall); };
        let t0 = Instant::now();
        let key = disk_cache::key(prisms, &params);
        let path = dir.join(format!("hm-{key:032x}.bin"));
        if let Some(mut hm) = disk_cache::load(&path, prisms.len()) {
            if hm.stats.stored_bytes <= max_bytes {
                hm.stats.cache = "hit"; hm.stats.threads = params.threads; hm.stats.build_seconds = t0.elapsed().as_secs_f64();
                eprintln!("P7_COMPACT_HMATRIX_CACHE hit path={} loadSeconds={:.2}", path.display(), hm.stats.build_seconds);
                return Ok(hm);
            }
        }
        let mut hm = Self::build_uncached(prisms, max_bytes, params, start, wall)?;
        hm.stats.cache = "miss";
        match disk_cache::save(&dir, &path, &hm) {
            Ok(bytes) => eprintln!("P7_COMPACT_HMATRIX_CACHE saved path={} bytes={}", path.display(), bytes),
            Err(e) => eprintln!("P7_COMPACT_HMATRIX_CACHE save failed: {e}"),
        }
        Ok(hm)
    }
    fn build_uncached(prisms: &[&Prism], max_bytes: usize, params: Params, start: Instant, wall: Duration) -> Result<Self> {
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
        // Expensive blocks first, one block per scheduling unit: a few slow near blocks
        // otherwise end up in one chunk and run single-threaded at the tail. The block order
        // of the result (and hence the summation order of `apply`) is unchanged.
        let cost = |b: &Pairing| -> f64 {
            let (na, nb) = (&nodes[b.a], &nodes[b.b]);
            let (m, k) = ((na.end - na.start) as f64, (nb.end - nb.start) as f64);
            let c = |bb: &Bbox| [0, 1, 2].map(|i| 0.5 * (bb.lo[i] + bb.hi[i]));
            let (ca, cb) = (c(&na.bbox), c(&nb.bbox));
            let dist = ((ca[0] - cb[0]).powi(2) + (ca[1] - cb[1]).powi(2) + (ca[2] - cb[2]).powi(2)).sqrt().max(1e-30);
            // Entries are cheap moment pairs when the bounding spheres are well separated, exact otherwise.
            let per_entry = if b.a != b.b && 0.5 * (na.bbox.diam() + nb.bbox.diam()) / dist <= 0.2 { 1. } else { 20. };
            let size = if b.a == b.b { m * (m + 1.) / 2. } else { m * k };
            if b.far && b.a != b.b { per_entry * size.min(128. * (m + k)) + 0.1 * size } else { per_entry * size }
        };
        let mut schedule: Vec<usize> = (0..pairings.len()).collect();
        let costs: Vec<f64> = pairings.iter().map(cost).collect();
        schedule.sort_by(|&x, &y| costs[y].total_cmp(&costs[x]).then(x.cmp(&y)));
        let scheduled: Result<Vec<(usize, HBlock, Stats)>> = pool.install(|| schedule.par_iter().with_max_len(1).map(|&index| {
            let b = &pairings[index];
            let (na, nb) = (&nodes[b.a], &nodes[b.b]);
            let rows: Vec<usize> = order[na.start..na.end].to_vec();
            let cols: Vec<usize> = order[nb.start..nb.end].to_vec();
            let mut st = Stats::default();
            let block_started = Instant::now();
            let diag = b.a == b.b;
            let mut store = None;
            // Small blocks rarely compress at this tolerance; a failed ACA would be thrown away.
            if b.far && !diag && rows.len() * cols.len() > 1024 {
                let aca_started = Instant::now();
                let aca_result = aca(&kernel, &rows, &cols, params.eps, &mut st)?;
                if planar_operator::prof_on() { planar_operator::prof_add(9, aca_started); if aca_result.is_none() { planar_operator::prof_add(10, aca_started); } }
                if let Some((rank, u, v, err)) = aca_result {
                    charge(8 * (u.len() + v.len()))?; st.far_blocks = 1; st.max_rank = rank;
                    store = Some(Store::Low { rank, u, v, err });
                } else { st.aca_dense_fallbacks = 1; }
            }
            let dense_guard = if store.is_none() { Some(planar_operator::ProfGuard::new(11)) } else { None };
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
            drop(dense_guard);
            st.cpu_ns = block_started.elapsed().as_nanos() as u64;
            let exact_total = exact_done.fetch_add(st.exact_pairs, Ordering::Relaxed) + st.exact_pairs;
            fast_done.fetch_add(st.fast_pairs, Ordering::Relaxed); slow_ns_done.fetch_add(st.slow_ns, Ordering::Relaxed); near_done.fetch_add(st.near_blocks, Ordering::Relaxed); fallback_done.fetch_add(st.aca_dense_fallbacks, Ordering::Relaxed);
            let done = blocks_done.fetch_add(1, Ordering::Relaxed) + 1;
            if done % progress_step == 0 || done == total_blocks {
                eprintln!("P7_COMPACT_HMATRIX_PROGRESS done={} of={} elapsed={:.1} exact={} fast={} slowCpuS={:.1} nearBlocks={} acaFallbacks={}", done, total_blocks, t0.elapsed().as_secs_f64(), exact_total, fast_done.load(Ordering::Relaxed), slow_ns_done.load(Ordering::Relaxed) as f64 * 1e-9, near_done.load(Ordering::Relaxed), fallback_done.load(Ordering::Relaxed));
            }
            Ok((index, HBlock { a0: na.start, a1: na.end, b0: nb.start, b1: nb.end, diag, store }, st))
        }).collect());
        let mut results: Vec<Option<(HBlock, Stats)>> = (0..pairings.len()).map(|_| None).collect();
        for (index, block, st) in scheduled? { results[index] = Some((block, st)); }
        let results: Vec<(HBlock, Stats)> = results.into_iter().map(|r| r.expect("every scheduled block is built")).collect();
        let mut stats = Stats { supports: n, eps: params.eps, ..Default::default() };
        let mut blocks = Vec::with_capacity(results.len());
        for (b, st) in results { stats.merge(&st); blocks.push(b); }
        stats.stored_bytes = used.load(Ordering::Relaxed);
        stats.threads = params.threads;
        for b in &blocks { match &b.store { Store::Dense(d) => stats.dense_bytes += 8 * d.len(), Store::Low { u, v, .. } => stats.low_rank_bytes += 8 * (u.len() + v.len()) } }
        stats.build_seconds = t0.elapsed().as_secs_f64();
        Ok(Self { n, order: tree_order, blocks, stats })
    }

    /// Returns (potential, envelope); the operator is exactly symmetric and the
    /// envelope per support/component is sum of block residual estimates times
    /// the 2-norm of the other block's input (Cauchy-Schwarz).
    /// All columns in one pass over the stored blocks, parallel over blocks with
    /// per-thread accumulators (sums are reordered, so results agree with `apply`
    /// to rounding). Columns are processed in groups to bound the accumulator size.
    pub(super) fn apply_many(&self, rho: &[&[[f64; 3]]], threads: usize) -> Vec<(Vec<[f64; 3]>, Vec<[f64; 3]>)> {
        self.apply_many_workers(rho, workers_for(threads, self.stats.stored_bytes / 8, 2_000_000))
    }
    pub(super) fn apply_many_workers(&self, rho: &[&[[f64; 3]]], workers: usize) -> Vec<(Vec<[f64; 3]>, Vec<[f64; 3]>)> {
        const GROUP: usize = 8;
        const CHUNK: usize = 8;
        let n = self.n;
        let mut out = Vec::with_capacity(rho.len());
        for group in rho.chunks(GROUP) {
            let nc = group.len();
            let xs: Vec<Vec<[f64; 3]>> = group.iter().map(|r| { assert_eq!(r.len(), n); self.order.iter().map(|&s| r[s]).collect() }).collect();
            let workers = workers.min(self.blocks.len().div_ceil(CHUNK)).max(1);
            let parts: Vec<(Vec<[f64; 3]>, Vec<[f64; 3]>)> = run_workers(workers, |w, workers| {
                let (mut y, mut e) = (vec![[0f64; 3]; n * nc], vec![[0f64; 3]; n * nc]);
                for k in (w * CHUNK..self.blocks.len()).step_by(workers * CHUNK) {
                    for b in &self.blocks[k..(k + CHUNK).min(self.blocks.len())] { self.block_apply(b, &xs, n, &mut y, &mut e); }
                }
                (y, e)
            });
            let mut parts = parts.into_iter();
            let (mut y, mut e) = parts.next().expect("at least one worker");
            for (yb, eb) in parts {
                for (p, q) in y.iter_mut().zip(&yb) { for k in 0..3 { p[k] += q[k]; } }
                for (p, q) in e.iter_mut().zip(&eb) { for k in 0..3 { p[k] += q[k]; } }
            }
            for c in 0..nc {
                let (mut py, mut pe) = (vec![[0f64; 3]; n], vec![[0f64; 3]; n]);
                for (p, &s) in self.order.iter().enumerate() { py[s] = y[c * n + p]; pe[s] = e[c * n + p]; }
                out.push((py, pe));
            }
        }
        out
    }
    fn block_apply(&self, b: &HBlock, xs: &[Vec<[f64; 3]>], n: usize, y: &mut [[f64; 3]], e: &mut [[f64; 3]]) {
        let (m, nn) = (b.a1 - b.a0, b.b1 - b.b0);
        for (c, x) in xs.iter().enumerate() {
            let (y, e) = (&mut y[c * n..(c + 1) * n], &mut e[c * n..(c + 1) * n]);
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
                        let norm = |r: std::ops::Range<usize>| { let mut s = [0f64; 3]; for p in r { for k in 0..3 { s[k] += x[p][k] * x[p][k]; } } s.map(f64::sqrt) };
                        let (xa, xb) = (norm(b.a0..b.a1), norm(b.b0..b.b1));
                        for i in b.a0..b.a1 { for k in 0..3 { e[i][k] += err * xb[k]; } }
                        for j in b.b0..b.b1 { for k in 0..3 { e[j][k] += err * xa[k]; } }
                    }
                }
            }
        }
    }
    /// Read-only view of the dense diagonal leaf blocks: (support ids of the leaf in tree order, packed upper triangle
    /// `K(i,j)`, `j >= i`, row by row). Used by the block preconditioner of the compact enrichment.
    pub(super) fn diagonal_blocks(&self) -> Vec<(&[usize], &[f64])> {
        self.blocks.iter().filter(|b| b.diag).filter_map(|b| match &b.store { Store::Dense(d) => Some((&self.order[b.a0..b.a1], d.as_slice())), _ => None }).collect()
    }
    #[cfg_attr(not(test), allow(dead_code))]
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
        // Batched parallel action agrees with the serial single-column action to rounding.
        let (z, w) = (rho(2.73), rho(0.58));
        let batch = hm.apply_many_workers(&[&x, &y, &z, &w], 3);
        for (column, input) in [&x, &y, &z, &w].into_iter().enumerate() {
            let (single, single_env) = hm.apply(input);
            let scale = single.iter().flatten().fold(0f64, |m, v| m.max(v.abs()));
            let env_scale = single_env.iter().flatten().fold(0f64, |m, v| m.max(v.abs())).max(1e-300);
            for i in 0..n { for k in 0..3 {
                assert!((batch[column].0[i][k] - single[i][k]).abs() <= 1e-12 * scale, "column {column} support {i}");
                assert!((batch[column].1[i][k] - single_env[i][k]).abs() <= 1e-12 * env_scale, "envelope column {column} support {i}");
            } }
        }
        let dense_bytes = n * (n + 1) / 2 * 8;
        assert!(hm.stats.stored_bytes <= dense_bytes * 2, "{} vs {}", hm.stats.stored_bytes, dense_bytes);
        assert_eq!(hm.stats.dense_bytes + hm.stats.low_rank_bytes, hm.stats.stored_bytes, "{:?}", hm.stats);
        assert!(hm.stats.low_rank_bytes > 0 && hm.stats.dense_bytes > 0);
        // Disk cache round trip: identical bytes of every action, truncated or foreign files are rejected.
        let dir = std::env::temp_dir().join(format!("p7-hm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hm.bin");
        let params = Params { theta: 0.6, eps: 1e-6, threads: 4, leaf: 32 };
        disk_cache::save(&dir, &path, &hm).unwrap();
        let loaded = disk_cache::load(&path, n).expect("round trip");
        assert_eq!(loaded.apply(&x).0, hm.apply(&x).0);
        assert_eq!(loaded.apply(&x).1, hm.apply(&x).1);
        assert_eq!((loaded.stats.dense_bytes, loaded.stats.low_rank_bytes, loaded.stats.stored_bytes), (hm.stats.dense_bytes, hm.stats.low_rank_bytes, hm.stats.stored_bytes));
        assert!(disk_cache::load(&path, n + 1).is_none());
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 5]).unwrap();
        assert!(disk_cache::load(&path, n).is_none());
        let mut moved: Vec<Prism> = grid(); moved[0] = Prism::new("m".into(), [[0., 0.], [0.6e-3, 0.], [0., 0.5e-3]], [0., 35e-6]).unwrap();
        let moved_refs: Vec<&Prism> = moved.iter().collect();
        assert_ne!(disk_cache::key(&refs, &params), disk_cache::key(&moved_refs, &params));
        assert_eq!(disk_cache::key(&refs, &params), disk_cache::key(&refs, &params));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Opt-in persistent copy of the built H-matrix (on by default since 2026-11-14 when a cache directory is known, `P7_HMATRIX_CACHE=0` disables; directory `P7_HMATRIX_CACHE_DIR`
/// or `$PCB_ANALYSIS_CACHE_DIR/hmatrix`). The key hashes every prism coordinate, the build parameters and the
/// kernel-relevant `P7_*` environment; `KERNEL_VERSION` must be bumped when the pair kernel changes.
mod disk_cache {
    use super::*;
    use std::path::{Path, PathBuf};
    const KERNEL_VERSION: &str = "hm-cache-2";
    const MAGIC: &[u8; 8] = b"P7HMAT01";
    /// Variables that never change kernel values.
    const NEUTRAL: &[&str] = &["P7_ENRICH", "P7_OUTPUT", "P7_TREE", "P7_SMOKE", "P7_SUPERVISED", "P7_HMATRIX_THREADS", "P7_HMATRIX_CACHE",
        "P7_MAGNETIC_PHASE", "P7_COMPACT_PL_THREADS", "P7_MAGNETIC_NEAR_THREADS", "P7_PAIR_PROFILE", "P7_DUMP", "P7_FOIL_SHEET"];

    pub(super) fn directory() -> Option<PathBuf> {
        if !crate::extraction::p7_default_on("P7_HMATRIX_CACHE") { return None; }
        let dir = std::env::var_os("P7_HMATRIX_CACHE_DIR").map(PathBuf::from)
            .or_else(|| std::env::var_os("PCB_ANALYSIS_CACHE_DIR").map(|d| PathBuf::from(d).join("hmatrix")))?;
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }
    /// Two independent FNV-1a lanes (128 bits); not cryptographic, the cache is local and trusted.
    struct Hash([u64; 2]);
    impl Hash {
        fn new() -> Self { Hash([0xcbf29ce484222325, 0x84222325cbf29ce4]) }
        fn bytes(&mut self, b: &[u8]) { for &x in b { self.0[0] = (self.0[0] ^ x as u64).wrapping_mul(0x100000001b3); self.0[1] = (self.0[1] ^ (x as u64 ^ 0x5a)).wrapping_mul(0x100000001b3).rotate_left(7); } }
        fn f64s(&mut self, v: &[f64]) { for x in v { self.bytes(&x.to_bits().to_le_bytes()); } }
    }
    pub(super) fn key(prisms: &[&Prism], params: &Params) -> u128 {
        let mut h = Hash::new();
        h.bytes(KERNEL_VERSION.as_bytes());
        h.bytes(&(prisms.len() as u64).to_le_bytes());
        for p in prisms { for t in &p.triangle_m { h.f64s(t); } h.f64s(&p.z_m); }
        h.bytes(&[super::planar_operator::escalate_on() as u8]); // effective value: P7_PAIR_ESCALATE is on by default and changes pair values
        h.f64s(&[params.theta, params.eps]); h.bytes(&(params.leaf as u64).to_le_bytes());
        let mut vars: Vec<(String, String)> = std::env::vars().filter(|(k, _)| k.starts_with("P7_") && !NEUTRAL.iter().any(|n| k.starts_with(n))).collect();
        vars.sort();
        for (k, v) in vars { h.bytes(k.as_bytes()); h.bytes(&[0]); h.bytes(v.as_bytes()); h.bytes(&[1]); }
        ((h.0[0] as u128) << 64) | h.0[1] as u128
    }
    fn put_u64(o: &mut Vec<u8>, v: usize) { o.extend_from_slice(&(v as u64).to_le_bytes()); }
    fn put_f64s(o: &mut Vec<u8>, v: &[f64]) { put_u64(o, v.len()); for x in v { o.extend_from_slice(&x.to_le_bytes()); } }
    pub(super) fn save(dir: &Path, path: &Path, hm: &PlanarHMatrix) -> std::io::Result<usize> {
        let mut o = Vec::with_capacity(hm.stats.stored_bytes + (1 << 20));
        o.extend_from_slice(MAGIC); put_u64(&mut o, hm.n);
        put_u64(&mut o, hm.order.len()); for &x in &hm.order { put_u64(&mut o, x); }
        let s = &hm.stats;
        for v in [s.supports, s.far_blocks, s.near_blocks, s.exact_pairs, s.moment_pairs, s.stored_bytes, s.dense_bytes, s.low_rank_bytes, s.max_rank, s.max_order, s.unconverged, s.approximated, s.aca_dense_fallbacks, s.fast_pairs] { put_u64(&mut o, v); }
        o.extend_from_slice(&s.slow_ns.to_le_bytes()); o.extend_from_slice(&s.cpu_ns.to_le_bytes());
        for v in [s.build_seconds, s.eps, s.max_change] { o.extend_from_slice(&v.to_le_bytes()); }
        put_u64(&mut o, hm.blocks.len());
        for b in &hm.blocks {
            for v in [b.a0, b.a1, b.b0, b.b1, usize::from(b.diag)] { put_u64(&mut o, v); }
            match &b.store {
                Store::Dense(d) => { o.push(0); put_f64s(&mut o, d); }
                Store::Low { rank, u, v, err } => { o.push(1); put_u64(&mut o, *rank); o.extend_from_slice(&err.to_le_bytes()); put_f64s(&mut o, u); put_f64s(&mut o, v); }
            }
        }
        let tmp = dir.join(format!("{}.{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("hm"), std::process::id()));
        std::fs::write(&tmp, &o)?; std::fs::rename(&tmp, path)?;
        Ok(o.len())
    }
    struct Cur<'a>(&'a [u8], usize);
    impl Cur<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> { let e = self.1.checked_add(n)?; let r = self.0.get(self.1..e)?; self.1 = e; Some(r) }
        fn u(&mut self) -> Option<usize> { Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?) as usize) }
        fn f(&mut self) -> Option<f64> { Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?)) }
        fn f64s(&mut self) -> Option<Vec<f64>> { let n = self.u()?; let b = self.take(n.checked_mul(8)?)?; Some(b.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()) }
    }
    /// `None` on any mismatch or corruption: the caller rebuilds.
    pub(super) fn load(path: &Path, n: usize) -> Option<PlanarHMatrix> {
        let data = std::fs::read(path).ok()?;
        let mut c = Cur(&data, 0);
        if c.take(8)? != MAGIC || c.u()? != n { return None; }
        let m = c.u()?; if m != n { return None; }
        let mut order = Vec::with_capacity(n); let mut seen = vec![false; n];
        for _ in 0..n { let x = c.u()?; if x >= n || std::mem::replace(&mut seen[x], true) { return None; } order.push(x); }
        let mut f = [0usize; 14]; for v in f.iter_mut() { *v = c.u()?; }
        let (slow_ns, cpu_ns) = (c.u()? as u64, c.u()? as u64);
        let (build_seconds, eps, max_change) = (c.f()?, c.f()?, c.f()?);
        let stats = Stats { supports: f[0], far_blocks: f[1], near_blocks: f[2], exact_pairs: f[3], moment_pairs: f[4], stored_bytes: f[5], dense_bytes: f[6], low_rank_bytes: f[7], max_rank: f[8], max_order: f[9], unconverged: f[10], approximated: f[11], aca_dense_fallbacks: f[12], fast_pairs: f[13], slow_ns, cpu_ns, build_seconds, eps, max_change, ..Default::default() };
        let count = c.u()?; let mut blocks = Vec::with_capacity(count.min(1 << 20));
        for _ in 0..count {
            let (a0, a1, b0, b1, diag) = (c.u()?, c.u()?, c.u()?, c.u()?, c.u()? != 0);
            if a0 > a1 || b0 > b1 || a1 > n || b1 > n { return None; }
            let (rows, cols) = (a1 - a0, b1 - b0);
            let store = match c.take(1)?[0] {
                0 => { let d = c.f64s()?; if d.len() != if diag { rows * (rows + 1) / 2 } else { rows * cols } { return None; } Store::Dense(d) }
                1 => { let rank = c.u()?; let err = c.f()?; let (u, v) = (c.f64s()?, c.f64s()?); if u.len() != rank * rows || v.len() != rank * cols { return None; } Store::Low { rank, u, v, err } }
                _ => return None,
            };
            blocks.push(HBlock { a0, a1, b0, b1, diag, store });
        }
        if c.1 != data.len() { return None; }
        Some(PlanarHMatrix { n, order, blocks, stats })
    }
}
