//! Internal test-scoped scalar diagnostics. No DSL switch, fields or cache.
#[derive(Clone, Copy)]
pub(super) enum Phase {
    Mesh,
    Compile,
    Assembly,
    Symbolic,
    NumericFactor,
    PreconditionerApply,
    OperatorApply,
    Roundoff,
    Linear,
    Output,
    Comparison,
    ApplyWorkspace,
    TriangularSolve,
}
#[derive(Clone, Copy)]
pub(super) enum Count {
    Linear,
    CgIteration,
    Accepted,
    Rejected,
    FactorCacheHit,
    SameDtHit,
    BypassedExact,
    NumericBuild,
}
#[cfg(test)]
mod enabled {
    use super::*;
    use serde_json::{json, Value};
    use std::{cell::RefCell, time::Instant};
    #[derive(Clone, Default)]
    struct Stage {
        ns: [u64; 13],
        calls: [u64; 13],
        counts: [u64; 8],
    }
    struct Profile {
        stages: [Stage; 16],
        current: usize,
        overflow: usize,
    }
    thread_local! {static ACTIVE:RefCell<Option<Profile>>=const{RefCell::new(None)};}
    pub(crate) fn start() {
        ACTIVE.with(|p| {
            assert!(p.borrow().is_none());
            *p.borrow_mut() = Some(Profile {
                stages: std::array::from_fn(|_| Stage::default()),
                current: 0,
                overflow: 0,
            });
        });
    }
    pub(crate) fn stage(i: usize) {
        ACTIVE.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                p.current = i.min(15);
                if i > 15 {
                    p.overflow += 1;
                }
            }
        });
    }
    pub(crate) fn count(c: Count) {
        ACTIVE.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                let i = p.current;
                p.stages[i].counts[c as usize] += 1;
            }
        });
    }
    pub(crate) struct Timer(Option<(Instant, usize, usize)>);
    pub(crate) fn timer(phase: Phase) -> Timer {
        Timer(ACTIVE.with(|p| {
            p.borrow()
                .as_ref()
                .map(|p| (Instant::now(), p.current, phase as usize))
        }))
    }
    impl Drop for Timer {
        fn drop(&mut self) {
            if let Some((start, i, k)) = self.0 {
                let ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                ACTIVE.with(|p| {
                    if let Some(p) = p.borrow_mut().as_mut() {
                        p.stages[i].ns[k] = p.stages[i].ns[k].saturating_add(ns);
                        p.stages[i].calls[k] += 1;
                    }
                });
            }
        }
    }
    pub(crate) fn finish() -> Value {
        ACTIVE.with(|p|{let p=p.borrow_mut().take().expect("profile started");let names=["mesh","compile","assembly","symbolic","numericFactor","preconditionerApply","operatorApply","roundoff","linear","output","comparison","applyWorkspace","triangularSolve"];let counts=["linearSolves","cgLoopIterations","acceptedSteps","rejectedSteps","factorCacheHits","sameDtAndTangentHits","bypassedExactDtMatches","numericBuilds"];json!({"scope":"Internal bounded test diagnostics only; nested inclusive phase timers are NOT additive. No previous/intermediate fields. At most16 stage slots; later overflow aggregates in last slot.","timeUnit":"ms","counterUnit":"calls/iterations/steps as named","overflowStages":p.overflow,"stages":p.stages.iter().enumerate().filter(|(_,s)|s.calls.iter().any(|n|*n>0)||s.counts.iter().any(|n|*n>0)).map(|(i,s)|json!({"stage":i,"timings":names.iter().enumerate().map(|(k,name)|((*name).to_owned(),json!({"ms":s.ns[k]as f64/1e6,"calls":s.calls[k]}))).collect::<serde_json::Map<_,_>>(),"counters":counts.iter().enumerate().map(|(k,name)|((*name).to_owned(),json!(s.counts[k]))).collect::<serde_json::Map<_,_>>() })).collect::<Vec<_>>()})})
    }
}
#[cfg(test)]
pub(super) use enabled::{count, stage, timer};
#[cfg(test)]
pub(super) use enabled::{finish, start};
#[cfg(not(test))]
pub(super) struct Timer;
#[cfg(not(test))]
#[inline(always)]
pub(super) fn timer(_: Phase) -> Timer {
    Timer
}
#[cfg(not(test))]
#[inline(always)]
pub(super) fn count(_: Count) {}
#[cfg(not(test))]
#[inline(always)]
pub(super) fn stage(_: usize) {}
