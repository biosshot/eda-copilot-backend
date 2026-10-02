//! Host-side cold kernel preparation, separate from GPU execution/readback.
use std::sync::atomic::{AtomicU64,AtomicUsize,Ordering};
use std::time::Instant;

#[derive(Default)]
struct Counters {generation:AtomicU64,active:AtomicUsize,launches:AtomicU64,launch_ns:AtomicU64,
    compilations:AtomicU64,compile_ns:AtomicU64}
static COUNTERS:Counters=Counters{generation:AtomicU64::new(0),active:AtomicUsize::new(0),
    launches:AtomicU64::new(0),launch_ns:AtomicU64::new(0),compilations:AtomicU64::new(0),compile_ns:AtomicU64::new(0)};

#[derive(Clone,Copy)]
pub(super) struct Snapshot {generation:u64,active:usize}
impl Counters {
    fn snapshot(&self)->Snapshot {Snapshot{generation:self.generation.load(Ordering::SeqCst),active:self.active.load(Ordering::SeqCst)}}
    fn unchanged(&self,before:Snapshot)->bool {
        let after=self.snapshot();before.active==0 && after.active==0 && before.generation==after.generation
    }
}
pub(super) fn snapshot()->Snapshot {COUNTERS.snapshot()}
pub(super) fn unchanged(before:Snapshot)->bool {COUNTERS.unchanged(before)}

pub(super) struct ColdLaunch {started:Instant,name:&'static str}
impl ColdLaunch {
    pub fn new(name:&'static str)->Self {
        COUNTERS.active.fetch_add(1,Ordering::SeqCst);
        COUNTERS.generation.fetch_add(1,Ordering::SeqCst);
        Self{started:Instant::now(),name}
    }
}
impl Drop for ColdLaunch {
    fn drop(&mut self) {
        let ns=self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        COUNTERS.launch_ns.fetch_add(ns,Ordering::Relaxed);COUNTERS.launches.fetch_add(1,Ordering::Relaxed);
        COUNTERS.generation.fetch_add(1,Ordering::SeqCst);COUNTERS.active.fetch_sub(1,Ordering::SeqCst);
        if std::env::var_os("PCB_GPU_PROFILE_KERNELS").is_some() {
            eprintln!("[gpu-kernel-prepare] {}",serde_json::json!({"kernel":self.name,"firstLaunchHostMs":ns as f64/1e6}));
        }
    }
}
pub(super) struct Compilation(Instant);
impl Compilation {pub fn new()->Self {Self(Instant::now())}}
impl Drop for Compilation {
    fn drop(&mut self) {
        COUNTERS.compile_ns.fetch_add(self.0.elapsed().as_nanos().min(u64::MAX as u128) as u64,Ordering::Relaxed);
        COUNTERS.compilations.fetch_add(1,Ordering::Relaxed);
    }
}
pub(super) fn statistics()->serde_json::Value {
    serde_json::json!({"firstLaunches":COUNTERS.launches.load(Ordering::Relaxed),
        "firstLaunchHostMs":COUNTERS.launch_ns.load(Ordering::Relaxed) as f64/1e6,
        "sourceCompilations":COUNTERS.compilations.load(Ordering::Relaxed),
        "sourceCompilationMs":COUNTERS.compile_ns.load(Ordering::Relaxed) as f64/1e6,
        "preparing":COUNTERS.active.load(Ordering::SeqCst)})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_startup_invalidates_a_sample_even_if_it_finishes_before_readback() {
        let counters=Counters::default();let before=counters.snapshot();assert!(counters.unchanged(before));
        counters.active.store(1,Ordering::SeqCst);counters.generation.fetch_add(1,Ordering::SeqCst);
        let during=counters.snapshot();assert!(!counters.unchanged(before));assert!(!counters.unchanged(during));
        counters.generation.fetch_add(1,Ordering::SeqCst);counters.active.store(0,Ordering::SeqCst);
        assert!(!counters.unchanged(before));assert!(!counters.unchanged(during));
        assert!(counters.unchanged(counters.snapshot()));
    }
}
