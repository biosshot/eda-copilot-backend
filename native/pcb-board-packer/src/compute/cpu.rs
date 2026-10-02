//! Bounded blocking compensation for independent native jobs. A GPU waiter
//! keeps its stack, but lends its CPU permit to another job in the same batch.
use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::{Arc,Condvar,Mutex}, time::Instant};

#[derive(Default)]
struct State { active:usize, peak:usize, suspensions:u64, suspended_ms:f64 }
pub(crate) struct Budget { limit:usize, state:Mutex<State>, wake:Condvar }
struct Local { budget:Arc<Budget>, held:bool }
thread_local! {static LOCAL:RefCell<Option<Local>>=const {RefCell::new(None)};}
pub(crate) struct Worker { _local:PhantomData<Rc<()>> }
struct Suspended { budget:Option<Arc<Budget>>, started:Instant, _local:PhantomData<Rc<()>> }

impl Budget {
    pub fn new(limit:usize)->Arc<Self> {
        Arc::new(Self{limit:limit.max(1),state:Mutex::new(State::default()),wake:Condvar::new()})
    }
    fn acquire(&self) {
        let mut state=self.state.lock().unwrap_or_else(|e|e.into_inner());
        while state.active==self.limit {state=self.wake.wait(state).unwrap_or_else(|e|e.into_inner());}
        state.active+=1;state.peak=state.peak.max(state.active);
    }
    fn release(&self) {
        let mut state=self.state.lock().unwrap_or_else(|e|e.into_inner());
        state.active-=1;self.wake.notify_one();
    }
    pub fn enter(self:&Arc<Self>)->Worker {
        LOCAL.with(|local|assert!(local.borrow().is_none(),"nested CPU worker registration"));
        self.acquire();
        LOCAL.with(|local|*local.borrow_mut()=Some(Local{budget:self.clone(),held:true}));
        Worker{_local:PhantomData}
    }
    pub fn report(&self)->serde_json::Value {
        let state=self.state.lock().unwrap_or_else(|e|e.into_inner());
        serde_json::json!({"activeLimit":self.limit,"active":state.active,"peakActive":state.peak,
            "suspensions":state.suspensions,"suspendedWorkerMs":state.suspended_ms})
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let local=LOCAL.with(|local|local.borrow_mut().take()).expect("registered worker");
        debug_assert!(local.held);local.budget.release();
    }
}

/// Only wrap blocking waits, never CPU encoding/compilation. Nested waits and
/// callers outside a registered independent-job batch are no-ops.
pub(crate) fn waiting<T>(wait:impl FnOnce()->T)->T {
    let budget=LOCAL.with(|local| {
        let mut local=local.borrow_mut();let worker=local.as_mut()?;
        if !worker.held {return None;}worker.held=false;Some(worker.budget.clone())
    });
    if let Some(budget)=&budget {budget.release();}
    let _suspended=Suspended{budget,started:Instant::now(),_local:PhantomData};
    wait()
}

/// Never retain a mutex guard while reacquiring a CPU slot: the current slot
/// owner may need this same mutex before it can finish or suspend.
pub(crate) fn lock<T>(mutex:&Mutex<T>)->std::sync::MutexGuard<'_,T> {
    loop {
        match mutex.try_lock() {
            Ok(guard)=>return guard,
            Err(std::sync::TryLockError::Poisoned(error))=>return error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock)=>waiting(|| {
                drop(mutex.lock().unwrap_or_else(|e|e.into_inner()));
            }),
        }
    }
}
impl Drop for Suspended {
    fn drop(&mut self) {
        if let Some(budget)=&self.budget {
            budget.acquire();
            LOCAL.with(|local|local.borrow_mut().as_mut().expect("registered worker").held=true);
            let mut state=budget.state.lock().unwrap_or_else(|e|e.into_inner());
            state.suspensions+=1;state.suspended_ms+=self.started.elapsed().as_secs_f64()*1000.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waiting_job_lends_the_only_cpu_slot_and_resumes_after_nested_wait() {
        let budget=Budget::new(1);
        std::thread::scope(|scope| {
            let (ready,started)=std::sync::mpsc::channel();
            let (finish,finished)=std::sync::mpsc::channel();
            let b=&budget;
            let first=scope.spawn(move || {
                let _worker=b.enter();ready.send(()).unwrap();
                waiting(||waiting(||finished.recv_timeout(std::time::Duration::from_secs(2)).unwrap()));
            });
            started.recv().unwrap();
            let b=&budget;
            scope.spawn(move ||{let _worker=b.enter();finish.send(()).unwrap();});
            first.join().unwrap();
        });
        let state=budget.state.lock().unwrap();
        assert_eq!(state.active,0);assert_eq!(state.peak,1);assert_eq!(state.suspensions,1);
    }
    #[test]
    fn panic_restores_the_cpu_permit_before_recovery() {
        let budget=Budget::new(1);let worker=budget.enter();
        assert!(std::panic::catch_unwind(||waiting(||panic!("injected wait failure"))).is_err());
        assert_eq!(budget.state.lock().unwrap().active,1);
        drop(worker);let _next=budget.enter();
        assert_eq!(budget.state.lock().unwrap().peak,1);
    }
    #[test]
    fn mutex_wait_does_not_keep_a_guard_while_reacquiring_the_cpu_slot() {
        let budget=Budget::new(1);let mutex=Arc::new(Mutex::new(()));
        let held=mutex.lock().unwrap();
        let (started,ready)=std::sync::mpsc::channel();
        let (done,completed)=std::sync::mpsc::channel();
        let b=budget.clone();let m=mutex.clone();let done_a=done.clone();
        let a=std::thread::spawn(move || {
            let _worker=b.enter();started.send(()).unwrap();
            let _held=lock(&m);done_a.send(()).unwrap();
        });
        ready.recv().unwrap();
        let (started,ready)=std::sync::mpsc::channel();
        let b=budget.clone();let m=mutex.clone();
        let other=std::thread::spawn(move || {
            let _worker=b.enter();started.send(()).unwrap();
            // A caller not using cooperative locking must still be able to
            // complete while it owns the only CPU slot.
            let _held=m.lock().unwrap();done.send(()).unwrap();
        });
        ready.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        drop(held);
        for _ in 0..2 {completed.recv_timeout(std::time::Duration::from_secs(2)).unwrap();}
        a.join().unwrap();other.join().unwrap();
        assert_eq!(budget.state.lock().unwrap().active,0);
    }
}
