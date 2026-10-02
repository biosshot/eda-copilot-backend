//! Scoped cross-process device ownership. Idle runtimes do not own the lock.
use super::{Error,ErrorKind};
use std::{fs::File,sync::{Arc,Mutex,Condvar},time::Instant};
#[derive(Clone,Copy,PartialEq,Eq)]
pub(crate) enum Admission {Wait,Try}
impl Admission {pub fn explicit(explicit:bool)->Self {if explicit {Self::Wait}else{Self::Try}}}
#[derive(Default)]
struct State {holders:usize,acquiring:bool,wait_ms:f64,busy:u64,acquisitions:u64}
pub(crate) struct Lease {file:File,state:Mutex<State>,changed:Condvar}
pub(crate) struct Permit {lease:Arc<Lease>}
impl Lease {
    pub fn new(file:File)->Arc<Self>{Arc::new(Self{file,state:Mutex::new(State::default()),changed:Condvar::new()})}
    pub fn acquire(self:&Arc<Self>,policy:Admission)->Result<Permit,Error> {
        let started=Instant::now();let mut state=self.state.lock().unwrap_or_else(|e|e.into_inner());
        while state.acquiring {
            if policy==Admission::Try {state.busy+=1;return Err(Error::new(ErrorKind::Busy,"GPU admission pending in this process"));}
            state=self.changed.wait(state).unwrap_or_else(|e|e.into_inner());
        }
        if state.holders>0 {state.holders+=1;return Ok(Permit{lease:self.clone()});}
        state.acquiring=true;drop(state);
        let result=match self.file.try_lock() {
            Ok(())=>Ok(()),
            Err(std::fs::TryLockError::WouldBlock) if policy==Admission::Wait=>self.file.lock().map_err(|e|Error::new(ErrorKind::Lease,format!("GPU lease wait: {e}"))),
            Err(std::fs::TryLockError::WouldBlock)=>Err(Error::new(ErrorKind::Busy,"GPU owned by another process")),
            Err(std::fs::TryLockError::Error(e))=>Err(Error::new(ErrorKind::Lease,format!("GPU lease: {e}"))),
        };
        let mut state=self.state.lock().unwrap_or_else(|e|e.into_inner());state.acquiring=false;
        state.wait_ms+=started.elapsed().as_secs_f64()*1000.0;
        if result.is_ok(){state.holders=1;state.acquisitions+=1;}else if result.as_ref().is_err_and(|e|e.kind==ErrorKind::Busy){state.busy+=1;}
        self.changed.notify_all();result.map(|()|Permit{lease:self.clone()})
    }
    pub fn statistics(&self)->serde_json::Value {
        let s=self.state.lock().unwrap_or_else(|e|e.into_inner());
        serde_json::json!({"holders":s.holders,"acquiring":s.acquiring,"waitMs":s.wait_ms,"busy":s.busy,"acquisitions":s.acquisitions})
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut s=self.lease.state.lock().unwrap_or_else(|e|e.into_inner());s.holders-=1;
        if s.holders==0 {let _=self.lease.file.unlock();self.lease.changed.notify_all();}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_owner_releases_and_waiting_contender_recovers() {
        let path=std::env::temp_dir().join(format!("pcb-scoped-lease-{}.lock",std::process::id()));
        let open=||std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).unwrap();
        let owner=Lease::new(open());let contender=Lease::new(open());
        let one=owner.acquire(Admission::Wait).unwrap();let two=owner.acquire(Admission::Wait).unwrap();
        assert!(matches!(contender.acquire(Admission::Try),Err(Error{kind:ErrorKind::Busy,..})));
        drop(one);assert!(contender.acquire(Admission::Try).is_err());
        std::thread::scope(|scope| {
            let (tx,rx)=std::sync::mpsc::channel();
            let contender=&contender;
            let pending=scope.spawn(move || {let permit=contender.acquire(Admission::Wait).unwrap();tx.send(()).unwrap();permit});
            assert!(rx.recv_timeout(std::time::Duration::from_millis(20)).is_err());
            drop(two);rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();drop(pending.join().unwrap());
        });
        assert_eq!(owner.statistics()["holders"],0);assert!(owner.acquire(Admission::Try).is_ok());
        drop(owner);drop(contender);std::fs::remove_file(path).unwrap();
    }
}
