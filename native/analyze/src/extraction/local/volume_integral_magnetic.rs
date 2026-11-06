//! Direct constant-vector finite-volume magnetostatic action on the unchanged
//! copper cells. Bounded local blocks may use an explicitly budgeted dense
//! kernel; the mixed operator streams the same pair quadrature without retaining
//! quadratic kernel storage.
use super::{
    accurate::Sum,
    error, magnetic_kernel_cache,
    mesh::LayeredMesh,
    polyhedron::{
        separated_order, tetra_integral_adaptive, tetra_self_integral, PointPotential,
        TetraCubature,
    },
    volume_current::Space,
    volume_magnetic::MagneticApply,
    AllocationBudget, Result,
};

use std::time::{Duration, Instant};

#[path = "hierarchical_magnetic.rs"]
pub(super) mod hierarchical_magnetic;

const CUBATURE_ORDERS: [usize; 5] = [4, 6, 8, 12, 16];

struct PairQuadrature {
    tetra: Vec<[[f64; 3]; 4]>,
    sources: Vec<PointPotential>,
    cubatures: Vec<Vec<TetraCubature>>,
    order: usize,
}

impl PairQuadrature {
    fn extra_bytes(n: usize, order: usize) -> Result<usize> {
        if !(8..=16).contains(&order) {
            return Err(error(
                "MAGNETIC_QUADRATURE",
                "direct volume integral orders8..16 required",
            ));
        }
        let per_cell = CUBATURE_ORDERS
            .into_iter()
            .filter(|&o| o <= order)
            .map(|o| o.pow(3) * 32)
            .sum::<usize>();
        n.checked_mul(per_cell + 128 * 1024)
            .ok_or_else(|| error("MEMORY_LIMIT", "direct magnetic cubature overflow"))
    }

    fn new(
        tetra: &[[[f64; 3]; 4]],
        order: usize,
        deadline: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        let mut sources = Vec::with_capacity(tetra.len());
        let mut cubatures = Vec::with_capacity(tetra.len());
        for &cell in tetra {
            deadline()?;
            sources.push(PointPotential::tetrahedron(cell)?);
            cubatures.push(
                CUBATURE_ORDERS
                    .into_iter()
                    .filter(|&o| o <= order)
                    .map(|o| TetraCubature::new(cell, o))
                    .collect::<Result<Vec<_>>>()?,
            );
        }
        Ok(Self {
            tetra: tetra.to_vec(),
            sources,
            cubatures,
            order,
        })
    }

    fn pair(&self, i: usize, j: usize) -> Result<f64> {
        if i == j {
            return tetra_self_integral(self.tetra[i], self.order);
        }
        let q = self.cubatures[i][0].separation_q(&self.cubatures[j][0]);
        if let Some(k) = separated_order(q, self.order)
            .and_then(|o| CUBATURE_ORDERS.iter().position(|&candidate| candidate == o))
        {
            return self.cubatures[i][k].direct_integral(&self.cubatures[j][k]);
        }
        tetra_integral_adaptive(self.tetra[i], &self.sources[j], self.order, 1e-10)
    }
}

// Fixed exact working set, not a spatial approximation or retained all-pairs
// kernel. One slot includes 25,344 B separated cubature at orders4/6/8,
// 32,000 B lazy far-source samples, tetra faces/edges/headers, and allocator
// capacity padding. Two additional slots cover construction and near/self
// graded integration scratch; no cache-owned reference escapes pair().
const STREAMED_PAIR_SLOT_BYTES: usize = 192 * 1024;
const STREAMED_PAIR_MAX_SLOTS: usize = 64;
// Both original orders 6/8 can retain n, n+2 and a lazy n+4 line rule.
const AXIAL_CELL_EXTRA_BYTES: usize = 40 * 1024;

struct PairCell {
    index: usize,
    touched: u64,
    source: PointPotential,
    cubatures: Vec<TetraCubature>,
    axial_prepared: [bool; 3],
}
// Diagnostic counters exist only when explicitly requested.
struct PairTrace {
    counts: [u64; 3],
    seconds: [f64; 3],
    last: Instant,
    hits:u64, misses:u64, evictions:u64,
}
impl PairTrace {
    fn emit(&self, final_record: bool) {
        use std::io::Write;
        let record = serde_json::json!({"phase":"near-kernels","final":final_record,"countsIncludeFailed":true,
            "selfCalls":self.counts[0],"separatedCalls":self.counts[1],"adaptiveCalls":self.counts[2],
            "pairCacheHits":self.hits,"pairCacheMisses":self.misses,"pairCacheEvictions":self.evictions,
            "selfSeconds":self.seconds[0],"separatedSeconds":self.seconds[1],"adaptiveSeconds":self.seconds[2]});
        let _ = writeln!(std::io::stderr().lock(), "P7_MAGNETIC_TRACE {record}");
    }
}
impl Drop for PairTrace {
    fn drop(&mut self) { self.emit(true); }
}
#[derive(Clone,Copy)]
struct ScalarSlot { i:usize, j:usize, value:f64 }
fn scalar_plan(value:Option<&str>,n:usize,order:usize,base:usize,budget:AllocationBudget)->Result<(usize,usize,usize,usize)> {
    let text=value.unwrap_or("0");
    if text.is_empty()||!text.bytes().all(|b|b.is_ascii_digit()) {return Err(error("MAGNETIC_CONFIG","P7_MAGNETIC_PAIR_CACHE_MIB must be an integer from 0 to 4096"));}
    let mib=text.parse::<usize>().map_err(|_|error("MAGNETIC_CONFIG","invalid scalar cache size"))?;
    if mib>4096 {return Err(error("MAGNETIC_CONFIG","P7_MAGNETIC_PAIR_CACHE_MIB exceeds 4096"));}
    let requested=mib.checked_mul(1024*1024).ok_or_else(||error("MEMORY_LIMIT","scalar pair cache overflow"))?;
    let count=requested/std::mem::size_of::<Option<ScalarSlot>>();let bytes=count.checked_mul(std::mem::size_of::<Option<ScalarSlot>>()).ok_or_else(||error("MEMORY_LIMIT","scalar pair cache overflow"))?;
    let reserved=base.checked_add(bytes).ok_or_else(||error("MEMORY_LIMIT","scalar pair cache plan overflow"))?;
    let (pair,slots)=BoundedPairs::plan(n,order,reserved,budget)?;
    Ok((count,bytes,pair.checked_add(bytes).ok_or_else(||error("MEMORY_LIMIT","scalar pair cache plan overflow"))?,slots))
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureRecord { i: usize, j: usize, target_bits: [[u64; 3]; 4], source_bits: [[u64; 3]; 4], value_bits: u64, order: usize, tolerance: f64, seconds: f64, geometry_hash: u64 }
struct SlowCapture { writer: std::io::BufWriter<std::fs::File>, count: usize, geometry_hash: u64 }
impl SlowCapture {
    fn open(path: &std::ffi::OsStr, tetra: &[[[f64;3];4]]) -> std::io::Result<Self> {
        let mut hash=14695981039346656037u64;
        for t in tetra { for p in t { for x in p { for b in x.to_bits().to_le_bytes() { hash=(hash ^ u64::from(b)).wrapping_mul(1099511628211); } } } }
        let file=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Self { writer: std::io::BufWriter::with_capacity(8192,file), count:0, geometry_hash:hash })
    }
    fn record(&mut self, row: &CaptureRecord) -> std::io::Result<()> {
        use std::io::Write;
        if self.writer.get_ref().metadata()?.len() > 65536 - 4096 { return Err(std::io::Error::new(std::io::ErrorKind::Other, "capture file limit reached")); }
        serde_json::to_writer(&mut self.writer,row)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        self.count+=1;
        Ok(())
    }
}
#[derive(Clone,Copy,serde::Serialize)]
#[serde(rename_all="camelCase")]
struct AxialFallbackSample { i:usize, j:usize, order:usize, reason:usize, target_bits:[[u64;3];4], source_bits:[[u64;3];4], coarse:Option<f64>, fine:Option<f64>, change:Option<f64>, limit:Option<f64> }
#[derive(Default)]
struct ParallelCounts { counts: [u64; 3], seconds: [f64; 3], geometry_hits:u64, geometry_builds:u64, lazy_direct_rules:u64, lazy_direct_points:u64, axial_accepted:u64, axial_fallback:u64, axial_outer_retries:u64, axial_outer_retry_accepted:u64, axial_max_relative:f64, orders:[u64;3], axial_reasons:[u64;4], samples:[Option<AxialFallbackSample>;4] }
struct ParallelProgress { windows: u64, pairs: u64, wall: f64, last: Instant, failed:bool, seen:u8 }
struct ParallelState {
    pool: rayon::ThreadPool,
    workers: Vec<std::sync::Mutex<BoundedPairs>>,
    progress: std::cell::RefCell<ParallelProgress>,
}
impl ParallelState {
    fn report(&self, final_record: bool, failed: bool) {
        use std::io::Write;
        let mut p = self.progress.borrow_mut();
        let mut counts = [0u64; 3]; let mut seconds = [0f64; 3]; let mut accepted=0; let mut fallback=0; let mut retries=0; let mut retry_accepted=0; let mut max_relative=0f64; let mut orders=[0u64;3];let mut geometry_hits=0;let mut geometry_builds=0;let mut lazy_rules=0;let mut lazy_points=0; let mut reasons=[0u64;4];
        for worker in &self.workers {
            let worker = worker.lock().unwrap_or_else(|e| e.into_inner());
            let c = worker.parallel_counts.as_ref().unwrap().borrow();
            for k in 0..3 { counts[k] += c.counts[k]; seconds[k] += c.seconds[k]; orders[k]+=c.orders[k]; } accepted+=c.axial_accepted; fallback+=c.axial_fallback; max_relative=max_relative.max(c.axial_max_relative);
            geometry_hits+=c.geometry_hits;geometry_builds+=c.geometry_builds;
            lazy_rules+=c.lazy_direct_rules;lazy_points+=c.lazy_direct_points;
            retries+=c.axial_outer_retries;retry_accepted+=c.axial_outer_retry_accepted;
            for k in 0..4 { reasons[k]+=c.axial_reasons[k]; if p.seen & (1<<k)==0 { if let Some(sample)=c.samples[k] { let _=writeln!(std::io::stderr().lock(),"P7_MAGNETIC_AXIAL_FALLBACK {}",serde_json::json!({"category":(["unsupported","inner_q_cap","inner_range_allowance","outer_convergence"][k]),"sample":sample})); p.seen|=1<<k; } } }
        }
        let _=writeln!(std::io::stderr().lock(),"P7_MAGNETIC_PARALLEL_PROGRESS {}", serde_json::json!({
            "completedWindows":p.windows,"completedPairs":p.pairs,"kernelWallSeconds":p.wall,
            "selfCalls":counts[0],"separatedCalls":counts[1],"adaptiveCalls":counts[2],
            "selfSeconds":seconds[0],"separatedSeconds":seconds[1],"adaptiveSeconds":seconds[2],
            "geometryCacheHits":geometry_hits,"geometryCacheBuilds":geometry_builds,
            "lazyDirectRuleBuilds":lazy_rules,"lazyDirectPointsBuilt":lazy_points,
            "axialOuterRetries":retries,"axialOuterRetryAccepted":retry_accepted,
            "axialAccepted":accepted,"axialFallbacks":fallback,"axialFallbackUnsupported":reasons[0],"axialFallbackInnerQCap":reasons[1],"axialFallbackRangeAllowance":reasons[2],"axialFallbackOuterConvergence":reasons[3],"axialMaxSeriesRoundoffEstimate":max_relative,"selectedOrder4":orders[0],"selectedOrder6":orders[1],"selectedOrder8":orders[2],
            "countsIncludeFailed":true,"partial":true,"final":final_record,"failed":p.failed || failed
        }));
    }
}
impl Drop for ParallelState { fn drop(&mut self) { self.report(true, false); } }
struct BoundedPairs {
    scalar:std::cell::RefCell<Vec<Option<ScalarSlot>>>,
    trace: Option<std::cell::RefCell<PairTrace>>,
    capture: Option<std::cell::RefCell<Option<SlowCapture>>>,
    tetra: std::sync::Arc<Vec<[[f64; 3]; 4]>>,
    pool: Option<ParallelState>,
    parallel_counts: Option<std::cell::RefCell<ParallelCounts>>,
    axial: bool,
    cells: std::cell::RefCell<Vec<PairCell>>,
    slots: usize,
    tick: std::cell::Cell<u64>,
    order: usize,
    start: Instant,
    wall: Duration,
}
impl BoundedPairs {
    fn plan(
        n: usize,
        order: usize,
        base: usize,
        budget: AllocationBudget,
    ) -> Result<(usize, usize)> {
        PairQuadrature::extra_bytes(0, order)?; // unchanged supported order contract
        let fixed = n
            .checked_mul(std::mem::size_of::<[[f64; 3]; 4]>())
            .and_then(|v| v.checked_add(2 * STREAMED_PAIR_SLOT_BYTES))
            .and_then(|v| v.checked_add(if std::env::var_os("P7_MAGNETIC_SLOW_PAIR_PATH").is_some() { 16384 } else { 0 }))
            .ok_or_else(|| error("MEMORY_LIMIT", "bounded magnetic geometry plan overflow"))?;
        let available = budget
            .max_bytes
            .checked_sub(base)
            .and_then(|v| v.checked_sub(fixed))
            .ok_or_else(|| {
                error(
                    "MEMORY_LIMIT",
                    "bounded exact magnetic working set exceeds budget",
                )
            })?;
        let slots = (available / STREAMED_PAIR_SLOT_BYTES).min(STREAMED_PAIR_MAX_SLOTS);
        if slots < 2 {
            return Err(error(
                "MEMORY_LIMIT",
                "two exact magnetic cache slots and scratch required",
            ));
        }
        let owned = fixed
            .checked_add(slots * STREAMED_PAIR_SLOT_BYTES)
            .ok_or_else(|| error("MEMORY_LIMIT", "bounded magnetic cache overflow"))?;
        budget.check(
            base.checked_add(owned)
                .ok_or_else(|| error("MEMORY_LIMIT", "bounded magnetic total overflow"))?,
        )?;
        Ok((owned, slots))
    }
    fn new(
        tetra: Vec<[[f64; 3]; 4]>,
        order: usize,
        slots: usize,
        start: Instant,
        wall: Duration,
    ) -> Result<Self> {
        Self::new_with_scalar(tetra,order,slots,0,start,wall)
    }
    #[cfg(test)]
    fn new_with_axial(tetra:Vec<[[f64;3];4]>,order:usize,slots:usize,start:Instant,wall:Duration,axial:bool)->Result<Self> {
        let mut pair=Self::new(tetra,order,slots,start,wall)?; pair.axial=axial; Ok(pair)
    }
    fn new_with_scalar(tetra:Vec<[[f64;3];4]>,order:usize,slots:usize,scalar_slots:usize,start:Instant,wall:Duration)->Result<Self> {
        check_deadline(start, wall)?;
        let mut scalar=Vec::new();scalar.try_reserve_exact(scalar_slots).map_err(|_|error("MEMORY_LIMIT","scalar pair cache allocation failed"))?;
        scalar.resize(scalar_slots,None);
        let capture=std::env::var_os("P7_MAGNETIC_SLOW_PAIR_PATH").and_then(|path| match SlowCapture::open(&path,&tetra) {
            Ok(c) => Some(std::cell::RefCell::new(Some(c))),
            Err(e) => { eprintln!("P7_MAGNETIC_SLOW_PAIR capture disabled: {e}"); None }
        });
        Ok(Self {
            capture,
            scalar:std::cell::RefCell::new(scalar),
            trace: (std::env::var_os("P7_MAGNETIC_TRACE").as_deref() == Some(std::ffi::OsStr::new("1")))
                .then(|| std::cell::RefCell::new(PairTrace { counts: [0; 3], seconds: [0.; 3], last: Instant::now(), hits:0, misses:0, evictions:0 })),
            tetra: std::sync::Arc::new(tetra),
            pool: None,
            parallel_counts: None,
            axial: false,
            cells: std::cell::RefCell::new(Vec::with_capacity(slots)),
            slots,
            tick: std::cell::Cell::new(0),
            order,
            start,
            wall,
        })
    }
    fn parallel_bytes(workers: usize) -> Result<usize> { Self::parallel_bytes_for(workers, false) }
    fn parallel_bytes_for(workers: usize, axial: bool) -> Result<usize> {
        if !(2..=32).contains(&workers) { return Err(error("MAGNETIC_CONFIG", "parallel worker count must be 2..32")); }
        workers.checked_mul(66 * STREAMED_PAIR_SLOT_BYTES + 2 * 1024 * 1024 + 8192 + if axial { 64 * AXIAL_CELL_EXTRA_BYTES + 64 * 1024 } else { 0 })
            .and_then(|v| v.checked_add(8192))
            .ok_or_else(|| error("MEMORY_LIMIT", "parallel magnetic window plan overflow"))
    }
    fn configure_parallel(&mut self, workers: usize) -> Result<()> {
        Self::parallel_bytes_for(workers, self.axial)?;
        let pool = rayon::ThreadPoolBuilder::new().num_threads(workers)
            .stack_size(2 * 1024 * 1024).build()
            .map_err(|_| error("MEMORY_LIMIT", "parallel magnetic pool creation failed"))?;
        let mut caches = Vec::new();
        caches.try_reserve_exact(workers).map_err(|_| error("MEMORY_LIMIT", "parallel cache allocation failed"))?;
        for _ in 0..workers {
            let mut cells = Vec::new();
            cells.try_reserve_exact(64).map_err(|_| error("MEMORY_LIMIT", "parallel cache allocation failed"))?;
            caches.push(std::sync::Mutex::new(BoundedPairs {
                tetra:self.tetra.clone(), pool:None, axial:self.axial, parallel_counts:Some(std::cell::RefCell::new(ParallelCounts::default())),
                scalar:std::cell::RefCell::new(Vec::new()), trace:None, capture:None,
                cells:std::cell::RefCell::new(cells), slots:64, tick:std::cell::Cell::new(0),
                order:self.order, start:self.start, wall:self.wall,
            }));
        }
        self.pool = Some(ParallelState { pool, workers:caches, progress:std::cell::RefCell::new(ParallelProgress { windows:0, pairs:0, wall:0., last:Instant::now(),failed:false,seen:0 }) });
        Ok(())
    }
    fn parallel_enabled(&self) -> bool { self.pool.is_some() }
    fn pair_window(&self, indices: &[(usize, usize)]) -> Result<Vec<f64>> {
        use rayon::prelude::*;
        if indices.len() > 128 { return Err(error("MEMORY_LIMIT", "parallel magnetic window exceeds 128 pairs")); }
        if let Err(failure)=check_deadline(self.start,self.wall) {
            if let Some(state)=&self.pool {state.progress.borrow_mut().failed=true;state.report(false,true);}
            return Err(failure);
        }
        let Some(state) = &self.pool else {
            return indices.iter().map(|&(i,j)| self.pair(i.max(j),i.min(j))).collect();
        };
        if indices.is_empty() { return Ok(Vec::new()); }
        let started = Instant::now();
        let caches = &state.workers;
        let results: Vec<Vec<Result<f64>>> = state.pool.install(|| indices.par_chunks(8).map(|part| {
            let Some(index) = rayon::current_thread_index() else { return vec![Err(error("MAGNETIC_CONFIG", "parallel worker index unavailable"))]; };
            let local = match caches[index].lock() { Ok(v) => v, Err(_) => return vec![Err(error("MAGNETIC_CONFIG", "parallel worker cache poisoned"))] };
            part.iter().map(|&(i,j)| local.pair(i.max(j),i.min(j))).collect()
        }).collect());
        let result: Result<Vec<f64>> = results.into_iter().flatten().collect();
        let report = {
            let mut p = state.progress.borrow_mut();
            p.wall += started.elapsed().as_secs_f64();
            p.failed |= result.is_err();
            if result.is_ok() { p.windows += 1; p.pairs += indices.len() as u64; }
            let report = result.is_err() || p.last.elapsed() >= Duration::from_secs(5);
            if report { p.last = Instant::now(); }
            report
        };
        if report { state.report(false, result.is_err()); }
        result
    }
    fn ensure(&self, cells: &mut Vec<PairCell>, index: usize, pin: Option<usize>) -> Result<()> {
        check_deadline(self.start, self.wall)?;
        let tick = self.tick.get().saturating_add(1);
        self.tick.set(tick);
        if let Some(cell) = cells.iter_mut().find(|cell| cell.index == index) {
            if let Some(counts)=&self.parallel_counts {counts.borrow_mut().geometry_hits+=1;}
            cell.touched = tick;
            return Ok(());
        }
        // Evict before constructing the replacement. The current target stays
        // pinned for every source of its original upper-triangle row.
        if cells.len() == self.slots {
            let remove = cells
                .iter()
                .enumerate()
                .filter(|(_, cell)| Some(cell.index) != pin)
                .min_by_key(|(_, cell)| cell.touched)
                .map(|(i, _)| i)
                .ok_or_else(|| {
                    error(
                        "MEMORY_LIMIT",
                        "bounded magnetic cache has no evictable slot",
                    )
                })?;
            cells.remove(remove);
        }
        let tetra = self.tetra[index];
        if let Some(counts)=&self.parallel_counts {counts.borrow_mut().geometry_builds+=1;}
        let source = PointPotential::tetrahedron(tetra)?;
        let mut cubatures = Vec::with_capacity(3);
        // separated_order selects only4,6,8; near/self keep maximum=self.order.
        for order in [4, 6, 8] {
            check_deadline(self.start, self.wall)?;
            if order <= self.order {
                cubatures.push(if self.axial {TetraCubature::geometry_only(tetra)?} else {TetraCubature::new(tetra, order)?});
            }
        }
        cells.push(PairCell {
            index,
            touched: tick,
            source,
            cubatures,
            axial_prepared: [false;3],
        });
        Ok(())
    }
    fn pair(&self, i: usize, j: usize) -> Result<f64> {
        check_deadline(self.start,self.wall)?;
        let slot={let cache=self.scalar.borrow();if cache.is_empty(){None}else{Some((i.wrapping_mul(0x9e3779b1).wrapping_add(j.wrapping_mul(0x85ebca6b)))%cache.len())}};
        if let Some(k)=slot {if let Some(entry)=self.scalar.borrow()[k] {if entry.i==i && entry.j==j {if let Some(t)=&self.trace {t.borrow_mut().hits+=1;}return Ok(entry.value);}} if let Some(t)=&self.trace {t.borrow_mut().misses+=1;}}
        let started = (self.parallel_counts.is_some() || self.trace.is_some() || self.capture.as_ref().is_some_and(|c| c.borrow().is_some())).then(Instant::now);
        let mut kind = if i == j { 0 } else { 2 };
        let result = self.pair_inner(i, j, &mut kind);
        if let (Some(k),Ok(value))=(slot,&result) {let mut cache=self.scalar.borrow_mut();if cache[k].is_some() {if let Some(t)=&self.trace {t.borrow_mut().evictions+=1;}}cache[k]=Some(ScalarSlot{i,j,value:*value});}
        let elapsed=started.map(|t| t.elapsed());
        if let (Some(counts), Some(elapsed)) = (&self.parallel_counts, elapsed) {
            let mut counts = counts.borrow_mut(); counts.counts[kind] += 1; counts.seconds[kind] += elapsed.as_secs_f64();
        }
        if let (Some(capture), Some(elapsed), Ok(value)) = (&self.capture, elapsed, &result) {
            if kind == 2 && self.order == 12 && elapsed >= Duration::from_millis(10) {
                let mut guard=capture.borrow_mut();
                if let Some(c)=guard.as_mut() {
                    let row=CaptureRecord { i,j,target_bits:self.tetra[i].map(|p|p.map(f64::to_bits)),source_bits:self.tetra[j].map(|p|p.map(f64::to_bits)),value_bits:value.to_bits(),order:self.order,tolerance:1e-10,seconds:elapsed.as_secs_f64(),geometry_hash:c.geometry_hash };
                    if let Err(e)=c.record(&row) { eprintln!("P7_MAGNETIC_SLOW_PAIR capture disabled: {e}"); *guard=None; }
                    else if c.count >= 16 { *guard=None; }
                }
            }
        }
        if let (Some(trace), Some(elapsed)) = (&self.trace, elapsed) {
            let mut trace = trace.borrow_mut();
            trace.counts[kind] += 1;
            trace.seconds[kind] += elapsed.as_secs_f64();
            if trace.counts.iter().sum::<u64>() % 64 == 0 && trace.last.elapsed() >= Duration::from_secs(1) {
                trace.emit(false);
                trace.last = Instant::now();
            }
        }
        result
    }
    fn pair_inner(&self, i: usize, j: usize, kind: &mut usize) -> Result<f64> {
        check_deadline(self.start, self.wall)?;
        if i == j {
            return tetra_self_integral(self.tetra[i], self.order);
        }
        *kind = 2;
        let mut cells = self.cells.borrow_mut();
        self.ensure(&mut cells, i, None)?;
        self.ensure(&mut cells, j, Some(i))?;
        let ti = cells.iter().position(|cell| cell.index == i).unwrap();
        let si = cells.iter().position(|cell| cell.index == j).unwrap();
        let q = cells[ti].cubatures[0].separation_q(&cells[si].cubatures[0]);
        if let Some(k) = separated_order(q, self.order)
            .and_then(|o| [4, 6, 8].iter().position(|&candidate| candidate == o))
        {
            *kind = 1;
            if let Some(c) = &self.parallel_counts { c.borrow_mut().orders[k] += 1; }
            if self.axial && k > 0 {
                for (cell_index, tetra_index) in [(ti,i),(si,j)] {
                    if !cells[cell_index].axial_prepared[k] {
                        cells[cell_index].cubatures[k].prepare_axial(self.tetra[tetra_index], [4,6,8][k])?;
                        cells[cell_index].axial_prepared[k]=true;
                    }
                }
                let mut detail = cells[ti].cubatures[k].try_axial_detailed(&cells[si].cubatures[k])?;
                let retried=detail.reason==Some(3);
                if retried {
                    check_deadline(self.start,self.wall)?;
                    if let Some(c)=&self.parallel_counts {c.borrow_mut().axial_outer_retries+=1;}
                    for (cell_index,tetra_index) in [(ti,i),(si,j)] {
                        cells[cell_index].cubatures[k].prepare_axial_retry(self.tetra[tetra_index],[4,6,8][k])?;
                    }
                    detail=cells[ti].cubatures[k].try_axial_retry(&cells[si].cubatures[k],&detail)?;
                }
                if let Some((value, error_estimate)) = detail.value {
                    if let Some(c) = &self.parallel_counts { let mut c=c.borrow_mut(); c.axial_accepted+=1; c.axial_max_relative=c.axial_max_relative.max(error_estimate/value.abs()); }
                    if retried {if let Some(c)=&self.parallel_counts {c.borrow_mut().axial_outer_retry_accepted+=1;}}
                    return Ok(value);
                }
                if let Some(c) = &self.parallel_counts {
                    let mut c=c.borrow_mut(); c.axial_fallback+=1;
                    let reason=detail.reason.expect("fallback reason"); c.axial_reasons[reason]+=1;
                    if c.samples[reason].is_none() { c.samples[reason]=Some(AxialFallbackSample { i,j,order:[4,6,8][k]+if retried {2}else{0},reason,
                        target_bits:self.tetra[i].map(|p|p.map(f64::to_bits)), source_bits:self.tetra[j].map(|p|p.map(f64::to_bits)),
                        coarse:detail.coarse,fine:detail.fine,change:detail.coarse.zip(detail.fine).map(|(a,b)|(a-b).abs()),limit:detail.fine.map(|v|1e-10*v) }); }
                }
            }
            if self.axial {
                for (cell_index,tetra_index) in [(ti,i),(si,j)] {
                    check_deadline(self.start,self.wall)?;
                    if cells[cell_index].cubatures[k].prepare_direct(self.tetra[tetra_index],[4,6,8][k])? {
                        if let Some(c)=&self.parallel_counts {let mut c=c.borrow_mut();c.lazy_direct_rules+=1;c.lazy_direct_points+=[4u64,6,8][k].pow(3);}
                    }
                }
            }
            return cells[ti].cubatures[k].direct_integral(&cells[si].cubatures[k]);
        }
        let source = &cells[si];
        tetra_integral_adaptive(self.tetra[i], &source.source, self.order, 1e-10)
    }
}

fn geometry(mesh: &LayeredMesh) -> Vec<[[f64; 3]; 4]> {
    mesh.copper_tetrahedra
        .iter()
        .map(|cell| cell.nodes.map(|i| mesh.nodes_m[i]))
        .collect()
}

/// Applies the unchanged direct tetrahedral Newton operator to bounded batches.
/// Pair quadrature is reused across columns; no all-pairs array is retained.
pub(super) struct Streamed<'a> {
    space: &'a Space,
    pair: BoundedPairs,
    hierarchical: Option<hierarchical_magnetic::Operator>,
    pub owned_bytes: usize,
    start: Instant,
    wall: Duration,
}

impl<'a> Streamed<'a> {
    pub fn new(
        mesh: &LayeredMesh,
        space: &'a Space,
        order: usize,
        base_bytes: usize,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<Self> {
        let n = mesh.copper_tetrahedra.len();
        if n == 0
            || n != space.cells.len()
            || space
                .cells
                .iter()
                .enumerate()
                .any(|(i, cell)| cell.mesh_cell != i)
        {
            return Err(error(
                "MAGNETIC_MESH",
                "unchanged native copper cell order required",
            ));
        }
        let config=match std::env::var("P7_MAGNETIC_PAIR_CACHE_MIB") {Ok(v)=>Some(v),Err(std::env::VarError::NotPresent)=>None,Err(_)=>return Err(error("MAGNETIC_CONFIG","P7_MAGNETIC_PAIR_CACHE_MIB must be valid UTF-8"))};
        let axial = match std::env::var("P7_MAGNETIC_AXIAL_TETS") { Ok(v) if v=="1" => true, Ok(v) if v=="0" => false, Err(std::env::VarError::NotPresent) => false, _ => return Err(error("MAGNETIC_CONFIG", "P7_MAGNETIC_AXIAL_TETS must be 0 or 1")) };
        let requested = match std::env::var("P7_MAGNETIC_NEAR_THREADS") {
            Ok(v) => v.parse::<usize>().ok().filter(|&n| (1..=32).contains(&n))
                .ok_or_else(|| error("MAGNETIC_CONFIG", "P7_MAGNETIC_NEAR_THREADS must be an integer 1..32"))?,
            Err(std::env::VarError::NotPresent) => 1,
            Err(_) => return Err(error("MAGNETIC_CONFIG", "P7_MAGNETIC_NEAR_THREADS must be valid UTF-8")),
        };
        let diagnostics = std::env::var_os("P7_MAGNETIC_TRACE").as_deref() == Some(std::ffi::OsStr::new("1"))
            || std::env::var_os("P7_MAGNETIC_SLOW_PAIR_PATH").is_some()
            || config.as_deref().is_some_and(|v| v != "0");
        let workers = if diagnostics { if requested > 1 { eprintln!("P7_MAGNETIC_NEAR_THREADS serial fallback: diagnostics or scalar cache enabled"); } 1 } else { requested };
        let parallel_bytes = if workers > 1 { BoundedPairs::parallel_bytes_for(workers, axial)? } else { 0 };
        eprintln!("P7_MAGNETIC_PARALLEL_PLAN nearWorkers={workers} requestedNearWorkers={requested} nearParallelBytes={parallel_bytes} nearWindowPairs=128");
        let axial_bytes = if axial { 64 * AXIAL_CELL_EXTRA_BYTES + 64 * 1024 } else { 0 };
        eprintln!("P7_MAGNETIC_AXIAL_PLAN enabled={axial} serialExtraBytes={axial_bytes}");
        let planned_base = base_bytes.checked_add(axial_bytes).and_then(|v| v.checked_add(parallel_bytes)).ok_or_else(|| error("MEMORY_LIMIT", "parallel magnetic base overflow"))?;
        let (scalar_slots,scalar_bytes,mut owned_bytes,slots)=scalar_plan(config.as_deref(),n,order,planned_base,budget)?;
        owned_bytes = owned_bytes.checked_add(axial_bytes).and_then(|v| v.checked_add(parallel_bytes)).ok_or_else(|| error("MEMORY_LIMIT", "parallel magnetic owned overflow"))?;
        eprintln!("P7_STREAMED_MEMORY_PLAN tetrahedra={n} geometryBytes={} exactCacheSlots={slots} slotBytes={STREAMED_PAIR_SLOT_BYTES} constructionScratchSlots=2 scalarCacheBytes={scalar_bytes} scalarCacheSlots={scalar_slots} ownedBytes={owned_bytes} baseBytes={base_bytes} budgetBytes={} operator=unchanged-direct-pairs complexity=quadratic", n*std::mem::size_of::<[[f64;3];4]>(),budget.max_bytes);
        // Move the original geometry into the cache; do not duplicate its Vec.
        let mut pair = BoundedPairs::new_with_scalar(geometry(mesh), order, slots, scalar_slots, start, wall)?;
        pair.axial = axial;
        if workers > 1 { pair.configure_parallel(workers)?; }
        let hierarchical = {
            let tree = hierarchical_magnetic::Operator::new(
                &pair.tetra,
                12,
                1,
                0.2,
                base_bytes
                    .checked_add(owned_bytes)
                    .ok_or_else(|| error("MEMORY_LIMIT", "magnetic hierarchy base overflow"))?,
                budget,
                start,
                wall,
            ).map_err(|failure| format!("{failure}; stage=adaptive-magnetic-hierarchy-admission originalTetrahedra={n} callerBaseBytes={base_bytes} exactCacheBytes={owned_bytes} availableBudgetBytes={}", budget.max_bytes))?;
            owned_bytes = owned_bytes
                .checked_add(tree.planned_bytes)
                .ok_or_else(|| error("MEMORY_LIMIT", "magnetic hierarchy plan overflow"))?;
            Some(tree)
        };
        Ok(Self {
            space,
            pair,
            hierarchical,
            owned_bytes,
            start,
            wall,
        })
    }

    pub fn evidence(&self) -> serde_json::Value {
        self.hierarchical.as_ref().map_or_else(||serde_json::json!({"policy":"exact-streamed-v1",
            "method":"unchanged direct tetrahedral pairs","scope":"existing near/far quadrature, mesh and physical coverage remain unverified"}),|tree|tree.evidence())
    }
    pub fn resistive_goals(
        &self,
        columns: &[Vec<f64>],
        angular_frequency: f64,
    ) -> Result<Vec<f64>> {
        hierarchical_magnetic::adaptive::resistive_goals(self.space, columns, angular_frequency)
    }
    pub fn actions_certified(
        &self,
        columns: &[Vec<f64>],
        absolute_goals: &[f64],
    ) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, serde_json::Value)> {
        let (actions, bounds, mut certificate) = self.hierarchical
            .as_ref()
            .ok_or_else(|| {
                error(
                    "HIERARCHICAL_MAGNETIC_POLICY",
                    "adaptive common-batch tree required",
                )
            })?
            .batch(&self.pair, self.space, columns, absolute_goals)?;
        if self.pair.axial {
            fn mark(value: &mut serde_json::Value) {
                value["nearQuadraturePolicy"] = serde_json::json!("axial-tetra-estimated-v1");
                value["nearQuadratureErrorCertified"] = serde_json::json!(false);
                if let Some(chunks)=value.get_mut("chunkCertificates").and_then(|v|v.as_array_mut()) { for chunk in chunks { mark(chunk); } }
            }
            mark(&mut certificate);
        }
        Ok((actions,bounds,certificate))
    }

    /// Upper triangle visited once per batch. Both action rows receive the
    /// same scalar pair and exact vector-axis dot product.
    pub fn actions(&self, columns: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
        let n = self.space.current_count();
        if columns
            .iter()
            .any(|col| col.len() != n || col.iter().any(|v| !v.is_finite()))
        {
            return Err(error(
                "MAGNETIC_INPUT",
                "finite native current columns required",
            ));
        }
        if columns.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = vec![vec![Sum::default(); n]; columns.len()];
        for (i, target) in self.space.cells.iter().enumerate() {
            for (j, source) in self.space.cells[..=i].iter().enumerate() {
                check_deadline(self.start, self.wall)?;
                let coefficient = 1e-7
                    * target.density_per_amp_m2
                    * source.density_per_amp_m2
                    * self.pair.pair(i, j)?;
                for (column, action) in columns.iter().zip(&mut out) {
                    for k in 0..3 {
                        action[3 * i + k] =
                            action[3 * i + k].add(Sum::from(column[3 * j + k]).mul(coefficient));
                        if i != j {
                            action[3 * j + k] = action[3 * j + k]
                                .add(Sum::from(column[3 * i + k]).mul(coefficient));
                        }
                    }
                }
            }
        }
        out.into_iter()
            .map(|col| {
                let values: Vec<_> = col.into_iter().map(Sum::value).collect();
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(error(
                        "MAGNETIC_RANGE",
                        "nonfinite streamed volume magnetic action",
                    ));
                }
                Ok(values)
            })
            .collect()
    }
}

fn check_deadline(start: Instant, wall: Duration) -> Result<()> {
    if start.elapsed() >= wall {
        return Err(error("DEADLINE", "streamed tetra magnetic action"));
    }
    Ok(())
}
pub struct Operator<'a> {
    mesh: &'a LayeredMesh,
    kernel: Vec<f64>,
    planned_bytes: usize,
}

impl<'a> Operator<'a> {
    pub fn new(mesh: &'a LayeredMesh, budget: AllocationBudget, order: usize) -> Result<Self> {
        let n = mesh.copper_tetrahedra.len();
        if n == 0 {
            return Err(error(
                "MAGNETIC_MESH",
                "direct volume integral requires copper cells",
            ));
        }
        let kernel_bytes = n
            .checked_mul(n)
            .and_then(|v| v.checked_mul(8))
            .ok_or_else(|| error("MEMORY_LIMIT", "direct magnetic kernel overflow"))?;
        let cubature_bytes = PairQuadrature::extra_bytes(n, order)?;
        let plan = mesh
            .retained_bytes()?
            .checked_add(kernel_bytes)
            .and_then(|v| v.checked_add(cubature_bytes))
            .ok_or_else(|| error("MEMORY_LIMIT", "direct magnetic plan overflow"))?;
        budget.check(plan)?;
        let tetra = geometry(mesh);
        let kernel = magnetic_kernel_cache::square(&tetra, order, 1e-10, |tetra| {
            let pair = PairQuadrature::new(tetra, order, || Ok(()))?;
            let mut kernel = vec![0.; n * n];
            for i in 0..n {
                for j in 0..=i {
                    let value = pair.pair(i, j)?;
                    kernel[i * n + j] = value;
                    kernel[j * n + i] = value;
                }
            }
            Ok(kernel)
        })?;
        Ok(Self {
            mesh,
            kernel,
            planned_bytes: plan,
        })
    }
    fn apply_current(&self, space: &Space, current: &[f64]) -> Result<Vec<f64>> {
        if current.len() != space.current_count() || current.iter().any(|x| !x.is_finite()) {
            return Err(error(
                "MAGNETIC_INPUT",
                "finite full current vector required",
            ));
        }
        let n = self.mesh.copper_tetrahedra.len();
        let mut out = vec![Sum::default(); current.len()];
        for (i, target) in space.cells.iter().enumerate() {
            for (j, source) in space.cells.iter().enumerate() {
                let coefficient = 1e-7
                    * target.density_per_amp_m2
                    * source.density_per_amp_m2
                    * self.kernel[target.mesh_cell * n + source.mesh_cell];
                for k in 0..3 {
                    out[3 * i + k] =
                        out[3 * i + k].add(Sum::from(current[3 * j + k]).mul(coefficient));
                }
            }
        }
        let out: Vec<_> = out.into_iter().map(|v| v.value()).collect();
        if out.iter().any(|x| !x.is_finite()) {
            return Err(error(
                "MAGNETIC_RANGE",
                "nonfinite direct volume magnetic action",
            ));
        }
        Ok(out)
    }
}

impl MagneticApply for Operator<'_> {
    fn apply(&self, space: &Space, current: &[f64]) -> Result<Vec<f64>> {
        self.apply_current(space, current)
    }
    fn planned_bytes(&self) -> usize {
        self.planned_bytes
    }
    fn scratch_bytes(&self) -> usize {
        self.planned_bytes
    }
}

#[cfg(test)]
mod bounded_pair_tests {
    use super::*;
    #[test]
    #[ignore = "bounded captured adaptive-orientation performance diagnostic"]
    fn captured_adaptive_orientation_benchmark() {
        use std::io::Read;
        let start=Instant::now(); let limit=Duration::from_secs(30); let mut bytes=Vec::new();
        std::fs::File::open(std::env::var("P7_PAIR_OUTPUT").unwrap()).unwrap().take(65537).read_to_end(&mut bytes).unwrap();
        assert!(bytes.len()<=65536);
        let rows:Vec<serde_json::Value>=if let Ok(report)=serde_json::from_slice::<serde_json::Value>(&bytes) {
            report["pairs"].as_array().cloned().unwrap_or_else(||vec![report])
        } else {bytes.split(|&b|b==b'\n').filter(|line|!line.is_empty()).take(16).map(|line|serde_json::from_slice(line).unwrap()).collect()};
        assert!(!rows.is_empty()); let mut worst:Option<(f64,[[f64;3];4],[[f64;3];4],f64,f64)>=None; let mut done=0;
        for row in rows.iter().take(16) {
            if start.elapsed()>=limit {println!("P7_ORIENTATION deadline before pair {done}"); break}
            let a:[[u64;3];4]=serde_json::from_value(row["targetBits"].clone()).unwrap();
            let b:[[u64;3];4]=serde_json::from_value(row["sourceBits"].clone()).unwrap();
            let a=a.map(|p|p.map(f64::from_bits)); let b=b.map(|p|p.map(f64::from_bits));
            let source_a=PointPotential::tetrahedron(a).unwrap(); let source_b=PointPotential::tetrahedron(b).unwrap();
            let mut times=[Duration::ZERO;2]; let mut values=[0.;2]; let mut repetitions=0;
            for _ in 0..2 {
                if start.elapsed()>=limit {break}
                let now=Instant::now(); let ab=tetra_integral_adaptive(a,&source_b,12,1e-10).unwrap(); times[0]+=now.elapsed();
                if start.elapsed()>=limit {break}
                let now=Instant::now(); let ba=tetra_integral_adaptive(b,&source_a,12,1e-10).unwrap(); times[1]+=now.elapsed();
                assert!(ab.is_finite()&&ab>0.&&ba.is_finite()&&ba>0.); values=[ab,ba]; repetitions+=1;
            }
            if repetitions==0 {println!("P7_ORIENTATION deadline during pair {done}"); break}
            let mismatch=(values[0]-values[1]).abs()/values[0].abs().max(values[1].abs());
            println!("P7_ORIENTATION i={} j={} repetitions={repetitions} forwardSeconds={} reverseSeconds={} ratio={} relativeMismatch={mismatch} status={}",row["i"],row["j"],times[0].as_secs_f64(),times[1].as_secs_f64(),times[0].as_secs_f64()/times[1].as_secs_f64(),if mismatch>1e-8 {"refused"} else {"comparison-only"});
            if worst.as_ref().map_or(true,|w|mismatch>w.0) {worst=Some((mismatch,a,b,values[0],values[1]));} done+=1;
        }
        assert!(done>0,"no complete pair before deadline");
        if start.elapsed()<Duration::from_secs(20) {if let Some((mismatch,a,b,ab,ba))=worst {
            let source=PointPotential::tetrahedron(b).unwrap(); let now=Instant::now();
            let reference=super::super::polyhedron::tetra_integral(a,&source,16).unwrap();
            println!("P7_ORIENTATION_REFERENCE mismatch={mismatch} seconds={} forwardRelative={} reverseRelative={}",now.elapsed().as_secs_f64(),(ab-reference).abs()/reference,(ba-reference).abs()/reference);
        }} else {println!("P7_ORIENTATION_REFERENCE skipped: remaining deadline");}
        assert!(start.elapsed()<limit,"diagnostic exceeded deadline; use external supervisor for hard interruption");
    }
    #[test]
    fn axial_lazy_direct_preparation_selected_only() {
        let start=Instant::now();let wall=Duration::from_secs(30);
        let (a,b)=super::super::polyhedron::axial_retry_fixture();
        let pairs=BoundedPairs::new_with_axial(vec![a,b],12,8,start,wall,true).unwrap();
        let value=pairs.pair(0,1).unwrap();
        assert!(pairs.cells.borrow().iter().flat_map(|c|&c.cubatures).all(|q|q.direct_point_count()==0));
        let eager=BoundedPairs::new_with_axial(vec![a,b],12,8,start,wall,true).unwrap();
        {let mut cells=eager.cells.borrow_mut();for i in 0..2 {eager.ensure(&mut cells,i,None).unwrap();}
         for cell in &mut *cells {for (k,q) in cell.cubatures.iter_mut().enumerate() {q.prepare_direct(eager.tetra[cell.index],[4,6,8][k]).unwrap();}}}
        assert_eq!(value.to_bits(),eager.pair(0,1).unwrap().to_bits());
        let unsupported=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.0002,0.0003,0.001]];
        for distance in [0.002,0.02] {
            let b=unsupported.map(|p|[p[0]+distance,p[1],p[2]]);
            let lazy=BoundedPairs::new_with_axial(vec![unsupported,b],12,8,start,wall,true).unwrap();
            let plain=BoundedPairs::new_with_axial(vec![unsupported,b],12,8,start,wall,false).unwrap();
            assert_eq!(lazy.pair(0,1).unwrap().to_bits(),plain.pair(0,1).unwrap().to_bits());
            let cells=lazy.cells.borrow();
            let q=cells[0].cubatures[0].separation_q(&cells[1].cubatures[0]);
            let order=separated_order(q,12).expect("separated fallback");
            for cell in &*cells {for (k,rule) in cell.cubatures.iter().enumerate() {assert_eq!(rule.direct_point_count(),if [4usize,6,8][k]==order {order.pow(3)}else{0});}}
        }
    }
    #[test]
    #[ignore = "bounded captured geometry preparation diagnostic"]
    fn axial_lazy_preparation_captured_benchmark() {
        use std::io::Read;
        let mut text=String::new();std::fs::File::open(std::env::var("P7_PAIR_OUTPUT").unwrap()).unwrap().take(65537).read_to_string(&mut text).unwrap();
        assert!(text.len()<=65536);let rows:Vec<CaptureRecord>=text.lines().map(|s|serde_json::from_str(s).unwrap()).collect();assert_eq!(rows.len(),16);
        let geometry:Vec<_>=rows.iter().flat_map(|r|[r.source_bits.map(|p|p.map(f64::from_bits)),r.target_bits.map(|p|p.map(f64::from_bits))]).collect();
        let start=Instant::now();let wall=Duration::from_secs(30);let mut elapsed=[Duration::ZERO;2];
        for pass in 0..4 {for axial in if pass%2==0 {[false,true]}else{[true,false]} {
            let now=Instant::now();
            for _ in 0..32 {let pairs=BoundedPairs::new_with_axial(geometry.clone(),12,64,start,wall,axial).unwrap();let mut cells=pairs.cells.borrow_mut();for i in 0..geometry.len(){pairs.ensure(&mut cells,i,None).unwrap();}
                let points:usize=cells.iter().flat_map(|c|&c.cubatures).map(TetraCubature::direct_point_count).sum();assert_eq!(points,if axial {0}else{geometry.len()*792});}
            elapsed[usize::from(axial)]+=now.elapsed();
        }}
        eprintln!("P7_LAZY_PREPARATION cells={} eager_seconds={} lazy_seconds={} ratio={}",geometry.len()*32*4,elapsed[0].as_secs_f64(),elapsed[1].as_secs_f64(),elapsed[0].as_secs_f64()/elapsed[1].as_secs_f64());
    }
    #[test]
    fn axial_outer_retry_routes_and_reuses_geometry() {
        let (a,b)=super::super::polyhedron::axial_retry_fixture();
        let start=Instant::now();let wall=Duration::from_secs(30);
        let plain=BoundedPairs::new_with_axial(vec![a,b],12,8,start,wall,false).unwrap();
        let axial=BoundedPairs::new_with_axial(vec![a,b],12,8,start,wall,true).unwrap();
        let reference=plain.pair(0,1).unwrap();let value=axial.pair(0,1).unwrap();
        assert!((value-reference).abs()<1e-8*reference);
        assert_eq!(value.to_bits(),axial.pair(0,1).unwrap().to_bits());
        let mut parallel=BoundedPairs::new_with_axial(vec![a,b],12,8,start,wall,true).unwrap();
        parallel.configure_parallel(2).unwrap();
        assert_eq!(value.to_bits(),parallel.pair_window(&[(0,1)]).unwrap()[0].to_bits());
        let state=parallel.pool.as_ref().unwrap();let mut retries=0;let mut accepted=0;let mut fallbacks=0;
        for worker in &state.workers {let worker=worker.lock().unwrap();let c=worker.parallel_counts.as_ref().unwrap().borrow();retries+=c.axial_outer_retries;accepted+=c.axial_outer_retry_accepted;fallbacks+=c.axial_fallback;}
        assert_eq!((retries,accepted,fallbacks),(1,1,0));
    }
    #[test]
    fn axial_fallback_sample_is_bounded_and_emitted_once() {
        let t=[[0.,0.,0.],[0.1,0.,0.],[0.,0.1,0.],[0.,0.,2.]];
        let b=t.map(|p|[p[0]+4.,p[1],p[2]]);
        let mut pairs=BoundedPairs::new_with_axial(vec![t,b],12,8,Instant::now(),Duration::from_secs(30),true).unwrap();
        pairs.configure_parallel(2).unwrap();pairs.pair_window(&[(1,0)]).unwrap();
        let state=pairs.pool.as_ref().unwrap();
        let reasons:u64=state.workers.iter().map(|w|w.lock().unwrap().parallel_counts.as_ref().unwrap().borrow().axial_reasons.iter().sum::<u64>()).sum();
        assert_eq!(reasons,1);state.report(false,false);let seen=state.progress.borrow().seen;
        assert_eq!(seen.count_ones(),1);state.report(false,false);assert_eq!(state.progress.borrow().seen,seen);
    }
    #[test]
    #[ignore = "explicit bounded captured-pair performance diagnostic"]
    fn captured_pair_window_parallel_benchmark() {
        use std::io::Read;
        assert!(std::env::var_os("P7_MAGNETIC_TRACE").is_none());
        assert!(std::env::var_os("P7_MAGNETIC_SLOW_PAIR_PATH").is_none());
        assert!(std::env::var_os("P7_MAGNETIC_PAIR_CACHE_MIB").is_none());
        let path = std::env::var("P7_PAIR_OUTPUT").unwrap();
        let mut text = String::new();
        std::fs::File::open(path).unwrap().take(65537).read_to_string(&mut text).unwrap();
        assert!(text.len() <= 65536);
        let rows: Vec<CaptureRecord> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(rows.len(), 16);
        let mut geometry = Vec::with_capacity(32);
        let mut indices = Vec::with_capacity(128);
        for row in &rows {
            geometry.push(row.source_bits.map(|p| p.map(f64::from_bits)));
            geometry.push(row.target_bits.map(|p| p.map(f64::from_bits)));
        }
        for _ in 0..8 { for i in 0..16 { indices.push((2*i+1, 2*i)); } }
        let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:100,max_tetrahedra:100};
        let mut expected = Vec::new();
        for workers in [1,2,4] {
            let mut pairs=BoundedPairs::new(geometry.clone(),12,64,Instant::now(),Duration::from_secs(115)).unwrap();
            if workers > 1 {
                budget.check(BoundedPairs::parallel_bytes(workers).unwrap()).unwrap();
                pairs.configure_parallel(workers).unwrap();
            }
            for window in 0..2 {
                let started=Instant::now();
                let values=pairs.pair_window(&indices).unwrap();
                let bits: Vec<u64> = values.iter().map(|v|v.to_bits()).collect();
                assert_eq!(bits.len(),128);
                if workers == 1 && window == 0 { expected=bits; } else { assert_eq!(expected,bits); }
                eprintln!("P7_CAPTURED_PAIR_BENCH workers={workers} window={window} valid={} seconds={:.6}",values.len(),started.elapsed().as_secs_f64());
            }
        }
    }
    #[test]
    fn axial_opt_in_preserves_full_action_and_parallel_bits() {
        use std::collections::BTreeMap;
        use super::super::volume_current;
        let start=Instant::now(); let wall=Duration::from_secs(60);
        let a=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.001]];
        let geometry:Vec<_>=(0..5).map(|i| a.map(|p|[p[0]+i as f64*0.004,p[1],p[2]])).collect();
        let space=Space {mesh_nodes:vec![vec![];4],node_potential:BTreeMap::new(),cells:(0..geometry.len()).map(|i|volume_current::Cell {mesh_cell:i,potentials:[0,1,2,3],incidence:[[0.;3];4],density_per_amp_m2:1e6,resistance_ohm:1.}).collect(),components:vec![vec![0,1,2,3]],planned_bytes:0};
        let budget=AllocationBudget{max_bytes:128*1024*1024,max_nodes:1000,max_tetrahedra:1000};
        let tree=hierarchical_magnetic::Operator::new(&geometry,4,1,0.2,0,budget,start,wall).unwrap();
        let columns=vec![(0..15).map(|i|if i%2==0 {1.} else {-0.5}).collect::<Vec<_>>()];
        let original=BoundedPairs::new_with_axial(geometry.clone(),12,64,start,wall,false).unwrap();
        let mut axial=BoundedPairs::new_with_axial(geometry,12,64,start,wall,true).unwrap();
        let reference=tree.batch(&original,&space,&columns,&[1e-20]).unwrap().0;
        let serial=tree.batch(&axial,&space,&columns,&[1e-20]).unwrap().0;
        for (a,b) in reference.iter().flatten().zip(serial.iter().flatten()) { assert!((a-b).abs() <= 1e-8 * a.abs().max(1e-30)); }
        axial.configure_parallel(2).unwrap();
        let parallel=tree.batch(&axial,&space,&columns,&[1e-20]).unwrap().0;
        assert_eq!(serial.iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>(),parallel.iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>());
        assert!(BoundedPairs::parallel_bytes_for(2,true).unwrap()>BoundedPairs::parallel_bytes(2).unwrap());
        let unsupported=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.0002,0.0003,0.001]];
        let other=unsupported.map(|p|[p[0]+0.02,p[1],p[2]]);
        let plain=BoundedPairs::new_with_axial(vec![unsupported,other],12,4,start,wall,false).unwrap();
        let fallback=BoundedPairs::new_with_axial(vec![unsupported,other],12,4,start,wall,true).unwrap();
        assert_eq!(plain.pair(1,0).unwrap().to_bits(),fallback.pair(1,0).unwrap().to_bits());
    }
    #[test]
    fn axial_accepted_near_pair_and_action() {
        use std::collections::BTreeMap;
        use super::super::volume_current;
        let start=Instant::now(); let wall=Duration::from_secs(60);
        let a=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.00001]];
        let b=a.map(|p|[p[0]+0.002,p[1],p[2]]); let geometry=vec![a,b];
        let space=Space {mesh_nodes:vec![vec![];4],node_potential:BTreeMap::new(),cells:(0..2).map(|i|volume_current::Cell {mesh_cell:i,potentials:[0,1,2,3],incidence:[[0.;3];4],density_per_amp_m2:1e6,resistance_ohm:1.}).collect(),components:vec![vec![0,1,2,3]],planned_bytes:0};
        let budget=AllocationBudget{max_bytes:128*1024*1024,max_nodes:1000,max_tetrahedra:1000};
        let tree=hierarchical_magnetic::Operator::new(&geometry,4,1,0.2,0,budget,start,wall).unwrap();
        let original=BoundedPairs::new_with_axial(geometry.clone(),12,64,start,wall,false).unwrap();
        let mut axial=BoundedPairs::new_with_axial(geometry,12,64,start,wall,true).unwrap();
        let reference=original.pair(1,0).unwrap(); axial.configure_parallel(2).unwrap();
        let actual=axial.pair_window(&[(1,0)]).unwrap()[0];
        assert!((reference-actual).abs() <= 1e-8*reference.abs());
        let state=axial.pool.as_ref().unwrap();
        let accepted:u64=state.workers.iter().map(|w|w.lock().unwrap().parallel_counts.as_ref().unwrap().borrow().axial_accepted).sum();
        assert!(accepted>0,"axial acceptance path must run");
        let columns=vec![vec![1.,-0.5,0.25,-0.4,0.2,0.1]];
        let (reference,_,cert)=tree.batch(&original,&space,&columns,&[1e-25]).unwrap();
        assert!(cert["exactPairs"].as_u64().unwrap()>=3);
        let (actual,_,_)=tree.batch(&axial,&space,&columns,&[1e-25]).unwrap();
        for (a,b) in reference.iter().flatten().zip(actual.iter().flatten()) { assert!((a-b).abs() <= 1e-8*a.abs().max(1e-30)); }
    }
    #[test]
    #[ignore = "requires isolated P7_MAGNETIC_AXIAL_TETS=1 and P7_MAGNETIC_BATCH_COLUMNS=1"]
    fn axial_streamed_certificate_metadata() {
        use std::collections::BTreeMap;
        use super::super::{mesh::{LayeredMesh,CopperTetrahedron},volume_current};
        assert_eq!(std::env::var("P7_MAGNETIC_AXIAL_TETS").unwrap(),"1");
        assert_eq!(std::env::var("P7_MAGNETIC_BATCH_COLUMNS").unwrap(),"1");
        let a=[[0.,0.,0.],[0.001,0.,0.],[0.,0.001,0.],[0.,0.,0.00001]];
        let b=a.map(|p|[p[0]+0.002,p[1],p[2]]);
        let mesh=LayeredMesh { nodes_m:a.into_iter().chain(b).collect(), tetrahedra:vec![], copper_tetrahedra:(0..2).map(|i|CopperTetrahedron {nodes:[4*i,4*i+1,4*i+2,4*i+3],slab:0,planar_triangle:i,source_ids:vec![],volume_m3:1e-11/6.,gradients_per_m:[[0.;3];4]}).collect(), copper_interfaces:vec![],external_faces:vec![],dielectric_volume_m3:0.,copper_volume_m3:0.,planned_peak_bytes:0 };
        let space=Space {mesh_nodes:vec![vec![];4],node_potential:BTreeMap::new(),cells:(0..2).map(|i|volume_current::Cell {mesh_cell:i,potentials:[0,1,2,3],incidence:[[0.;3];4],density_per_amp_m2:1e6,resistance_ohm:1.}).collect(),components:vec![vec![0,1,2,3]],planned_bytes:0};
        let budget=AllocationBudget{max_bytes:128*1024*1024,max_nodes:1000,max_tetrahedra:1000};
        let op=Streamed::new(&mesh,&space,12,0,budget,Instant::now(),Duration::from_secs(60)).unwrap();
        if std::env::var_os("P7_MAGNETIC_PROFILE").as_deref()==Some(std::ffi::OsStr::new("1")) {
            assert!(std::env::var_os("P7_MAGNETIC_TRACE").is_none());
            assert_eq!(op.pair.pool.as_ref().expect("profile must preserve parallel workers").workers.len(),4);
        }
        let columns=vec![vec![1.;6],vec![-0.5;6]];
        let (actions,bounds,cert)=op.actions_certified(&columns,&[1e-25;2]).unwrap();
        eprintln!("P7_PROFILE_CONTROL {}",serde_json::json!({"actions":actions.iter().map(|v|v.iter().map(|x|x.to_bits()).collect::<Vec<_>>()).collect::<Vec<_>>(),"bounds":bounds.iter().map(|v|v.iter().map(|x|x.to_bits()).collect::<Vec<_>>()).collect::<Vec<_>>(),"certificate":cert}));
        for c in std::iter::once(&cert).chain(cert["chunkCertificates"].as_array().unwrap().iter()) {
            assert_eq!(c["nearQuadraturePolicy"],"axial-tetra-estimated-v1"); assert_eq!(c["nearQuadratureErrorCertified"],false);
        }
    }
    #[test]
    fn parallel_pair_window_preserves_bits_and_checks_resources() {
        let tetra=|x:f64| [[x,0.,0.],[x+0.001,0.,0.],[x,0.001,0.],[x,0.,0.001]];
        let mut pairs=BoundedPairs::new(vec![tetra(0.),tetra(0.001),tetra(0.1)],12,2,Instant::now(),Duration::from_secs(60)).unwrap();
        let indices=[(0,0),(1,0),(2,0),(2,1),(1,1),(2,2)];
        let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:100,max_tetrahedra:100};
        assert!(std::mem::size_of::<BoundedPairs>() + std::mem::size_of::<ParallelCounts>() + std::mem::size_of::<std::sync::Mutex<BoundedPairs>>() < 8192);
        for bad in [0,33] { assert!(BoundedPairs::parallel_bytes(bad).is_err()); assert!(pairs.configure_parallel(bad).is_err()); }
        let expected=pairs.pair_window(&indices).unwrap();
        for workers in [2,4] {
            budget.check(BoundedPairs::parallel_bytes(workers).unwrap()).unwrap();
            pairs.configure_parallel(workers).unwrap();
            let actual=pairs.pair_window(&indices).unwrap();
            assert_eq!(expected.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),actual.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
            let state=pairs.pool.as_ref().unwrap();
            assert_eq!((state.progress.borrow().windows,state.progress.borrow().pairs),(1,6));
            let calls:u64=state.workers.iter().map(|w|w.lock().unwrap().parallel_counts.as_ref().unwrap().borrow().counts.iter().sum::<u64>()).sum();
            assert_eq!(calls,6);
            let actual=pairs.pair_window(&indices).unwrap();
            assert_eq!(expected,actual);
            assert_eq!((state.progress.borrow().windows,state.progress.borrow().pairs),(2,12));
        }
        assert!(AllocationBudget{max_bytes:1,..budget}.check(BoundedPairs::parallel_bytes(2).unwrap()).is_err());
        pairs.wall=Duration::ZERO;
        assert!(pairs.pair_window(&indices).is_err());
        assert!(pairs.pool.as_ref().unwrap().progress.borrow().failed);
    }
    #[test]
    fn scalar_cache_preserves_order_eviction_and_deadlines() {
        let tetra=|x:f64|[[x,0.,0.],[x+0.001,0.,0.],[x,0.001,0.],[x,0.,0.001]];
        let cells=vec![tetra(0.),tetra(0.001),tetra(0.1)];
        let plain=BoundedPairs::new_with_scalar(cells.clone(),12,2,0,Instant::now(),Duration::from_secs(30)).unwrap();
        let mut cache=BoundedPairs::new_with_scalar(cells,12,2,1,Instant::now(),Duration::from_secs(30)).unwrap();
        cache.trace=Some(std::cell::RefCell::new(PairTrace{counts:[0;3],seconds:[0.;3],last:Instant::now(),hits:0,misses:0,evictions:0}));
        for (i,j) in [(0,0),(0,1),(1,0),(2,0),(2,0),(0,1)] {let a=plain.pair(i,j).unwrap();let b=cache.pair(i,j).unwrap();assert_eq!(a.to_bits(),b.to_bits());}
        assert_eq!((cache.scalar.borrow()[0].unwrap().i,cache.scalar.borrow()[0].unwrap().j),(0,1));
        {let t=cache.trace.as_ref().unwrap().borrow();assert_eq!((t.hits,t.misses,t.evictions),(1,5,4));}
        let budget=AllocationBudget{max_bytes:4*1024*1024,max_nodes:100,max_tetrahedra:100};
        let (count,bytes,owned,_)=scalar_plan(Some("1"),3,12,0,budget).unwrap();assert_eq!(bytes,count*std::mem::size_of::<Option<ScalarSlot>>());assert!(owned>=bytes);
        assert!(scalar_plan(Some("4"),3,12,0,budget).is_err());assert_eq!(scalar_plan(None,3,12,0,budget).unwrap().0,0);
        assert!(scalar_plan(Some("4096"),3,12,0,AllocationBudget{max_bytes:5usize*1024*1024*1024,..budget}).is_ok());
        for bad in ["", "-1", "1.5", "NaN", "4097"] {assert!(scalar_plan(Some(bad),3,12,0,budget).is_err());}
        let start=Instant::now();for _ in 0..1000 {cache.pair(0,1).unwrap();}let elapsed=start.elapsed();
        eprintln!("scalar cache repeated hits=1000 elapsed={elapsed:?}");
        let mut expired=BoundedPairs::new_with_scalar(vec![tetra(0.)],12,2,1,Instant::now(),Duration::from_secs(1)).unwrap();expired.pair(0,0).unwrap();
        expired.wall=Duration::ZERO;assert!(expired.pair(0,0).is_err());assert!(expired.scalar.borrow()[0].is_some());
        let budget=AllocationBudget{max_bytes:1024*1024,max_nodes:100,max_tetrahedra:100};
        assert!(BoundedPairs::plan(3,12,1024*1024,budget).is_err());
    }
    #[test]
    fn bounded_pair_cache_eviction_preserves_exact_coefficients_and_preflights_memory() {
        let tetra = |x: f64| {
            [
                [x, 0., 0.],
                [x + 0.001, 0., 0.],
                [x, 0.001, 0.],
                [x, 0., 0.001],
            ]
        };
        let cells = vec![tetra(0.), tetra(0.001), tetra(0.1)];
        let reference = PairQuadrature::new(&cells, 12, || Ok(())).unwrap();
        let bounded =
            BoundedPairs::new(cells, 12, 2, Instant::now(), Duration::from_secs(30)).unwrap();
        for (i, j) in [(2, 0), (1, 0), (2, 1), (1, 1), (2, 0)] {
            let expected = reference.pair(i, j).unwrap();
            let actual = bounded.pair(i, j).unwrap();
            assert_eq!(actual.to_bits(), expected.to_bits(), "pair({i},{j})");
            assert!(bounded.cells.borrow().len() <= 2);
        }
        let budget = AllocationBudget {
            max_bytes: 32 * 1024 * 1024,
            max_nodes: 1000000,
            max_tetrahedra: 1000000,
        };
        let (owned, slots) = BoundedPairs::plan(154899, 12, 0, budget).unwrap();
        assert_eq!(slots, 64);
        // Original geometry plus 64 enlarged per-order caches and two scratch slots.
        assert!(owned < 28 * 1024 * 1024);
        assert!(BoundedPairs::plan(
            154899,
            12,
            0,
            AllocationBudget {
                max_bytes: 1,
                ..budget
            }
        )
        .is_err());
        assert!(BoundedPairs::new(vec![tetra(0.)], 12, 2, Instant::now(), Duration::ZERO).is_err());
    }
}

#[cfg(test)]
mod actual_tree_planning {
    use super::*;
    #[test]
    fn capture_file_bound_and_io_failure() {
        let path=std::env::temp_dir().join(format!("p7-capture-{}",std::process::id()));
        let mut c=SlowCapture::open(path.as_os_str(), &[]).unwrap();
        assert!(SlowCapture::open(path.as_os_str(), &[]).is_err());
        c.writer.get_ref().set_len(65536-4096+1).unwrap();
        let row=CaptureRecord{i:0,j:1,target_bits:[[0;3];4],source_bits:[[0;3];4],value_bits:0,order:12,tolerance:1e-10,seconds:0.01,geometry_hash:0};
        assert!(c.record(&row).is_err());
        assert_eq!(c.writer.get_ref().metadata().unwrap().len(),65536-4096+1);
        drop(c); std::fs::remove_file(&path).unwrap();
        assert!(SlowCapture::open(path.join("missing").as_os_str(), &[]).is_err());
    }

    #[test]
    #[ignore = "explicit saved board diagnostic"]
    fn capture_actual_neighbor_pairs() {
        use std::{env,fs};
        let raw=fs::read_to_string(env::var("P7_SAVED_PROBLEM").unwrap()).unwrap();let problem:serde_json::Value=serde_json::from_str(&raw).unwrap();
        let board_text=problem["boardJson"].as_str().unwrap();let board=crate::import_json(board_text).board.unwrap();
        let geometry_value=problem["geometry"].clone();let request:super::super::board_mesh::Request=serde_json::from_value(geometry_value.clone()).unwrap();
        let hash=|bytes:&[u8]| {let mut h=14695981039346656037u64;for &b in bytes {h=(h^b as u64).wrapping_mul(1099511628211);}format!("fnv1a64:{h:016x}")};
        let start=Instant::now();let wall=Duration::from_secs(120);let budget=super::super::board_prepare::allocation_budget(4096);
        let mesh=super::super::board_mesh::build(&board,&request,budget,wall).unwrap();
        let base=mesh.mesh.planned_peak_bytes.checked_add(raw.len()*8).unwrap();let (owned,slots)=BoundedPairs::plan(mesh.mesh.copper_tetrahedra.len(),12,base,budget).unwrap();budget.check(base+owned).unwrap();
        let pairs=BoundedPairs::new_with_scalar(geometry(&mesh.mesh),12,slots,0,start,wall).unwrap();let mut records=vec![];let mut attempted=0;let mut failed=0;let mut skipped=0;let pair_start=Instant::now();
        for i in 0..128.min(pairs.tetra.len().saturating_sub(1)) {if pair_start.elapsed()>=Duration::from_secs(30)||start.elapsed()>=wall {break;}let j=i+1;attempted+=1;let mut kind=2;let t=Instant::now();let value=pairs.pair_inner(i,j,&mut kind);let seconds=t.elapsed().as_secs_f64();
            if kind==2 {if let Ok(value)=value {records.push(serde_json::json!({"i":i,"j":j,"target":pairs.tetra[i],"source":pairs.tetra[j],"order":12,"tolerance":1e-10,"value":value,"valueBits":value.to_bits(),"targetBits":pairs.tetra[i].map(|p|p.map(f64::to_bits)),"sourceBits":pairs.tetra[j].map(|p|p.map(f64::to_bits)),"seconds":seconds}));records.sort_by(|a,b|b["seconds"].as_f64().unwrap().total_cmp(&a["seconds"].as_f64().unwrap()));records.truncate(16);}else{failed+=1;}}else{skipped+=1;}
        }
        let report=serde_json::json!({"boardDigest":hash(board_text.as_bytes()),"geometryDigest":hash(serde_json::to_string(&geometry_value).unwrap().as_bytes()),"status":if records.is_empty(){"refused"}else{"pass"},"scope":"diagnostic pair capture only, no electrical solve; first 128 adjacent-index pairs, bounded 30 seconds", "attempted":attempted,"failed":failed,"skippedNonAdaptive":skipped,"stopReason":if attempted<128 {"time or mesh limit"}else{"sample limit"},"meshTetrahedra":pairs.tetra.len(),"elapsedSeconds":start.elapsed().as_secs_f64(),"pairs":records});
        let output=env::var("P7_PAIR_OUTPUT").unwrap();fs::write(output,serde_json::to_vec_pretty(&report).unwrap()).unwrap();println!("P7_ACTUAL_PAIRS count={} elapsed={}",report["pairs"].as_array().unwrap().len(),report["elapsedSeconds"]);assert!(!report["pairs"].as_array().unwrap().is_empty());
    }
    #[test]
    #[ignore = "replay captured actual pairs"]
    fn replay_actual_neighbor_pairs() {
        use std::{env,fs,io::Read};let start=Instant::now();let mut bytes=Vec::new();fs::File::open(env::var("P7_PAIR_OUTPUT").unwrap()).unwrap().take(65537).read_to_end(&mut bytes).unwrap();assert!(bytes.len()<=65536);
        let rows:Vec<serde_json::Value>=if let Ok(report)=serde_json::from_slice::<serde_json::Value>(&bytes) { report["pairs"].as_array().cloned().unwrap_or_else(||vec![report]) } else { bytes.split(|&b|b==b'\n').filter(|line|!line.is_empty()).take(16).map(|line|serde_json::from_slice(line).unwrap()).collect() };
        for row in rows.iter().take(16) {assert!(start.elapsed()<Duration::from_secs(30));let target:[[u64;3];4]=serde_json::from_value(row["targetBits"].clone()).unwrap();let source:[[u64;3];4]=serde_json::from_value(row["sourceBits"].clone()).unwrap();let target=target.map(|p|p.map(f64::from_bits));let source=source.map(|p|p.map(f64::from_bits));let src=PointPotential::tetrahedron(source).unwrap();let t=Instant::now();let value=tetra_integral_adaptive(target,&src,12,1e-10).unwrap();let actual_seconds=t.elapsed().as_secs_f64();assert_eq!(value.to_bits(),row["valueBits"].as_u64().unwrap());
            let volume=super::super::mesh::tetra_coefficients(target).unwrap().0;let mut counts=std::collections::BTreeMap::new();let profile_start=Instant::now();
            for order in [2,4,8,12] {let rule=super::super::prism::cached_rule(order).unwrap();super::super::polyhedron::tetra_integrate(target,volume,rule,|point,_|{if start.elapsed()>=Duration::from_secs(30) {return Err(error("DEADLINE","pair replay"));}let d=src.at_diagnostic(point)?;*counts.entry(d.source_order).or_insert(0usize)+=1;Ok(())}).unwrap();}
            println!("P7_PAIR_REPLAY i={} j={} productionSeconds={actual_seconds} fixedAllOrderDiagnosticSeconds={} sourceOrders={counts:?}",row["i"],row["j"],profile_start.elapsed().as_secs_f64());}

    }
    #[test]
    #[ignore = "requires an explicitly supplied saved actual-board request and supervisor"]
    fn saved_stm32_geometry_and_hierarchy_plan_only() {
        use std::{env, fs, path::PathBuf};
        let request_path = env::var("P7_SAVED_PROBLEM").expect("explicit saved problem path");
        let output = PathBuf::from(env::var("P7_TEST_EVIDENCE_DIR").expect("evidence directory"));
        let raw = fs::read_to_string(request_path).unwrap();
        let problem: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let board = crate::import_json(problem["boardJson"].as_str().unwrap())
            .board
            .unwrap();
        let request: super::super::board_mesh::Request =
            serde_json::from_value(problem["geometry"].clone()).unwrap();
        let started = Instant::now();
        let wall = Duration::from_secs(1800);
        let mut budget = super::super::board_prepare::allocation_budget(3072);
        let ports = request.ports.len();
        let samples = problem["request"]["frequenciesHz"]
            .as_array()
            .unwrap()
            .len();
        budget.max_bytes -= samples * ports * (ports * 256 + 512) + 256 * 1024;
        let mesh = super::super::board_mesh::build(&board, &request, budget, wall).unwrap();
        let mesh_retained = mesh.mesh.retained_bytes().unwrap();
        // Preserve the larger geometry preparation bound and reserve decoded
        // saved-input storage, rather than allocating a second whole budget.
        let geometry_base = mesh
            .mesh
            .planned_peak_bytes
            .max(mesh_retained)
            .checked_add(raw.len().checked_mul(8).unwrap())
            .unwrap();
        let tetra_bytes = mesh
            .mesh
            .copper_tetrahedra
            .len()
            .checked_mul(std::mem::size_of::<[[f64; 3]; 4]>())
            .unwrap();
        let base = geometry_base.checked_add(tetra_bytes).unwrap();
        budget.check(base).unwrap();
        let tetra = geometry(&mesh.mesh);
        let tree =
            hierarchical_magnetic::Operator::new(&tetra, 12, 1, 0.2, base, budget, started, wall);
        let report = match tree {
            Ok(tree) => serde_json::json!({
                "status":"pass", "phase":"geometry and hierarchy plan only",
                "seconds":started.elapsed().as_secs_f64(), "operator":tree.evidence(),
                "meshRetainedBytes":mesh_retained, "meshPreparationBoundBytes":mesh.mesh.planned_peak_bytes,
                "geometryAndInputBaseBytes":geometry_base, "tetraGeometryBytes":tetra_bytes,
                "totalPlannedBytes":base.checked_add(tree.planned_bytes).unwrap(),
                "admittedBudgetBytes":budget.max_bytes, "fieldSolves":0, "magneticActions":0,
                "originalCopperTetrahedra":tetra.len(), "sourceMappings":mesh.source_mapping.len(),
                "barrels":mesh.barrel_ids.len(), "physicalCoverage":"unverified"
            }),
            Err(error) => serde_json::json!({
                "status":"refused", "phase":"hierarchy planning", "error":error,
                "seconds":started.elapsed().as_secs_f64(), "geometryAndInputBaseBytes":geometry_base,
                "tetraGeometryBytes":tetra_bytes, "admittedBudgetBytes":budget.max_bytes,
                "fieldSolves":0, "magneticActions":0
            }),
        };
        fs::create_dir_all(&output).unwrap();
        fs::write(
            output.join("comparison.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("P7_ACTUAL_TREE_PREFLIGHT {report}");
        assert_eq!(report["status"], "pass", "{report}");
    }
}
