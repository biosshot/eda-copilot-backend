//! One bounded pool for the existing faer kernels. Default callers stay serial.
use std::{cell::Cell,sync::OnceLock};
thread_local! {static THREADS:Cell<usize>=const{Cell::new(1)};}
static POOL:OnceLock<std::result::Result<rayon::ThreadPool,String>>=OnceLock::new();
/// Restore the pre-rayon-feature implicit dense policy; sparse calls opt in.
pub fn initialize() {
    static SERIAL_DENSE:std::sync::Once=std::sync::Once::new();
    SERIAL_DENSE.call_once(||faer::set_global_parallelism(faer::Par::Seq));
}
pub fn current()->faer::Par {initialize();let n=THREADS.with(Cell::get);if n==1 {faer::Par::Seq}else{faer::Par::rayon(n)}}
pub fn stack_reservation()->usize {if POOL.get().is_none() {0}else{4*2*1024*1024}}
pub fn with_threads<T:Send>(threads:usize,run:impl FnOnce()->T+Send)->super::Result<T> {
    let available=std::thread::available_parallelism().map_or(1,usize::from).min(4);
    if threads==0||threads>available {return Err(super::error("THREADS","requested threads exceed available bounded1..4 pool"));}
    initialize();
    struct Reset(usize);impl Drop for Reset {fn drop(&mut self){THREADS.with(|n|n.set(self.0));}}
    if threads==1 {let _reset=Reset(THREADS.with(|n|n.replace(1)));return Ok(run());}
    let pool=POOL.get_or_init(||rayon::ThreadPoolBuilder::new().num_threads(available).stack_size(2*1024*1024).build().map_err(|e|e.to_string())).as_ref().map_err(Clone::clone)?;
    Ok(pool.install(|| {
        let _reset=Reset(THREADS.with(|n|n.replace(threads)));run()
    }))
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn bounded_pool_and_nested_serial_policy_restore() {
        let n=std::thread::available_parallelism().map_or(1,usize::from).min(4);
        assert!(with_threads(0,||()).is_err());assert!(with_threads(5,||()).is_err());
        with_threads(n,|| {if n>1 {assert_eq!(rayon::current_num_threads(),n);assert_eq!(stack_reservation(),8*1024*1024);}
            assert_eq!(current(),if n==1 {faer::Par::Seq}else{faer::Par::rayon(n)});
            with_threads(1,||{assert_eq!(current(),faer::Par::Seq);if n>1 {assert_eq!(stack_reservation(),8*1024*1024);}}).unwrap();
            assert_eq!(THREADS.with(Cell::get),n);
        }).unwrap();assert_eq!(current(),faer::Par::Seq);
    }
}
