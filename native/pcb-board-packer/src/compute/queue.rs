//! Process-wide admission queue. GPU scratch is owned by each admitted operation.
//! Queue depth is separate from CPU worker count and starts at the midpoint.
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Instant;

const MIN_DEPTH: usize = 1;
const MAX_DEPTH: usize = 8;
const INITIAL_DEPTH: usize = 4;

#[derive(Debug)]
struct State {
    next: u64,
    waiting: VecDeque<u64>,
    active: usize,
    depth: usize,
    completed: u64,
    wait_ms: f64,
    service_ms: f64,
    controller: Controller,
    window_class: &'static str,
    window_started: Instant,
    window_jobs: usize,
    window_work: f64,
    window_latency: f64,
    window_saturated: bool,
    decisions: u64,
    last_decision: &'static str,
    headroom:Option<u64>,
    reserved_bytes:usize,
    observed_workspace:HashMap<&'static str,usize>,
    memory_waits:u64,
    cold_samples:u64,
}
impl Default for State {
    fn default() -> Self {Self { next:0, waiting:VecDeque::new(), active:0,
        depth:INITIAL_DEPTH, completed:0, wait_ms:0.0, service_ms:0.0,
        controller:Controller::default(),window_class:"",window_started:Instant::now(),
        window_jobs:0,window_work:0.0,window_latency:0.0,window_saturated:false,
        decisions:0,last_decision:"middle_start",headroom:None,reserved_bytes:0,observed_workspace:HashMap::new(),memory_waits:0,cold_samples:0 }}
}
#[derive(Default)]
struct Queue { state: Mutex<State>, changed: Condvar }
static QUEUE: OnceLock<Queue> = OnceLock::new();

pub(super) struct Permit { started: Instant, class: &'static str, work:f64, saturated:bool, reservation:usize, startup:super::startup::Snapshot }

#[derive(Default, Debug)]
struct Controller { baseline:Option<(f64,f64,usize)>, reduce_trial:bool }
impl Controller {
    fn observe(&mut self,depth:usize,throughput:f64,latency:f64,saturated:bool)->(usize,&'static str) {
        if !saturated || !throughput.is_finite() || throughput<=0.0 {return (depth,"insufficient_demand");}
        if let Some((rate,delay,old_depth))=self.baseline.take() {
            let reducing=depth<old_depth;
            let useful=if reducing {throughput>=rate*0.98 && latency<=delay} else {throughput>=rate*1.03 && latency<=delay*1.25};
            if !useful {
                self.reduce_trial=!reducing;
                return (old_depth.max(MIN_DEPTH),"trial_reverted");
            }
        }
        if self.reduce_trial && depth>MIN_DEPTH {
            self.baseline=Some((throughput,latency,depth));
            return (depth-1,"lower_depth_trial");
        }
        self.reduce_trial=false;
        if depth<MAX_DEPTH {
            self.baseline=Some((throughput,latency,depth));
            (depth+1,"throughput_trial")
        } else {(depth,"at_depth_limit")}
    }
}

pub(super) fn acquire(class: &'static str, work:usize) -> Permit {
    let queue=QUEUE.get_or_init(Queue::default);
    let start=Instant::now();
    let mut s=queue.state.lock().unwrap_or_else(|e|e.into_inner());
    let saturated=s.active>=s.depth;
    let reservation=s.observed_workspace.get(class).copied().unwrap_or(0);
    let fits=|s:&State| memory_fits(s.active,s.headroom,s.reserved_bytes,reservation);
    if !fits(&s) {s.memory_waits+=1;}
    let ticket=s.next;s.next+=1;s.waiting.push_back(ticket);
    while s.waiting.front()!=Some(&ticket) || s.active>=s.depth || !fits(&s) {
        s=queue.changed.wait(s).unwrap_or_else(|e|e.into_inner());
    }
    s.waiting.pop_front();s.active+=1;s.reserved_bytes=s.reserved_bytes.saturating_add(reservation);s.wait_ms+=start.elapsed().as_secs_f64()*1000.0;
    queue.changed.notify_all();
    Permit {started:Instant::now(),class,work:work as f64,saturated,reservation,startup:super::startup::snapshot()}
}
impl Drop for Permit {
    fn drop(&mut self) {
        let queue=QUEUE.get().unwrap();
        let mut s=queue.state.lock().unwrap_or_else(|e|e.into_inner());
        s.active-=1;s.completed+=1;s.reserved_bytes=s.reserved_bytes.saturating_sub(self.reservation);
        let duration=self.started.elapsed().as_secs_f64()*1000.0;s.service_ms+=duration;
        if !super::startup::unchanged(self.startup) {
            s.cold_samples+=1;
            // Discard the whole window: its wall time overlaps preparation,
            // including jobs other than the one that triggered compilation.
            s.window_started=Instant::now();s.window_jobs=0;s.window_work=0.0;
            s.window_latency=0.0;s.window_saturated=false;s.controller=Controller::default();
            s.last_decision="cold_preparation_excluded";
        } else if self.work>0.0 {
            if s.window_class!=self.class {
                s.window_class=self.class;s.window_started=self.started;s.window_jobs=0;
                s.window_work=0.0;s.window_latency=0.0;s.window_saturated=false;
                s.controller=Controller::default();
            }
            s.window_jobs+=1;s.window_work+=self.work;s.window_latency+=duration;
            s.window_saturated|=self.saturated;
            if s.window_jobs>=32 && s.active==0 {
                let seconds=s.window_started.elapsed().as_secs_f64();
                let rate=s.window_work/seconds.max(1e-9);let latency=s.window_latency/s.window_jobs as f64;
                let depth=s.depth;let saturated=s.window_saturated;
                let (next,reason)=s.controller.observe(depth,rate,latency,saturated);
                s.depth=next;s.last_decision=reason;s.decisions+=1;
                s.window_started=Instant::now();s.window_jobs=0;s.window_work=0.0;
                s.window_latency=0.0;s.window_saturated=false;
            }
        }
        queue.changed.notify_all();
    }
}
pub(super) fn observe_memory(headroom:Option<u64>,class:&'static str,workspace:usize) {
    let queue=QUEUE.get_or_init(Queue::default);
    let mut s=queue.state.lock().unwrap_or_else(|e|e.into_inner());
    s.headroom=headroom;
    if workspace>0 {let estimate=s.observed_workspace.entry(class).or_default();*estimate=(*estimate).max(workspace);}
    queue.changed.notify_all();
}

// Reservations are conservative admission estimates, not guaranteed free VRAM.
// One operation must remain admissible to make progress when reusable buffers
// consume the budget; domain allocation guards and runtime recovery still apply.
fn memory_fits(active:usize,headroom:Option<u64>,reserved:usize,requested:usize)->bool {
    active==0 || headroom.is_none_or(|free|
        (reserved as u64).saturating_add(requested as u64)<=free)
}

pub(super) fn statistics()->serde_json::Value {
    let queue=QUEUE.get_or_init(Queue::default);
    let s=queue.state.lock().unwrap_or_else(|e|e.into_inner());
    serde_json::json!({"queued":s.waiting.len(),"active":s.active,"depth":s.depth,
        "initialDepth":INITIAL_DEPTH,"minDepth":MIN_DEPTH,"maxDepth":MAX_DEPTH,
        "completed":s.completed,"coldSamplesExcluded":s.cold_samples,"queueWaitMs":s.wait_ms,"serviceWorkerMs":s.service_ms,
        "estimatedBytesPerFlightByClass":s.observed_workspace,"reservedBytes":s.reserved_bytes,"usableHeadroomBytes":s.headroom,"memoryWaits":s.memory_waits,"adaptive":true,"decisions":s.decisions,"lastDecision":s.last_decision})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_pressure_limits_overlap_without_deadlocking_reuse() {
        assert!(memory_fits(1,Some(1000),400,600));
        assert!(!memory_fits(1,Some(999),400,600));
        assert!(!memory_fits(3,Some(0),0,1));
        assert!(memory_fits(0,Some(0),0,1000));
        assert!(memory_fits(3,None,400,600));
        assert!(!memory_fits(1,Some(1000),usize::MAX,usize::MAX));
    }
    #[test]
    fn controller_starts_midrange_and_reverts_useless_concurrency() {
        assert_eq!(State::default().depth,4);
        let mut c=Controller::default();
        assert_eq!(c.observe(4,100.0,10.0,true).0,5);
        assert_eq!(c.observe(5,100.0,12.0,true),(4,"trial_reverted"));
        assert_eq!(c.observe(4,100.0,10.0,false).0,4);
    }
    #[test]
    fn controller_requires_throughput_gain_without_excess_latency() {
        let mut c=Controller::default();c.observe(4,100.0,10.0,true);
        assert_eq!(c.observe(5,120.0,11.0,true).0,6);
        assert_eq!(c.observe(6,140.0,20.0,true).0,5);
    }
}
