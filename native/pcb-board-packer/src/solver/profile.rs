//! Opt-in accumulated CPU work. Worker totals are not stage wall time.
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};
use std::time::Instant;

pub(super) const NAMES: &[&str] = &[
    "generation", "candidate_legality", "full_rank", "hard_count", "hard_severity",
    "relations", "envelope", "spacing", "alignment", "edge_bias", "edge_place", "topology", "route", "joint_pair",
];
pub(super) struct Profile {
    enabled: bool,
    calls: Vec<AtomicU64>,
    nanos: Vec<AtomicU64>,
}
pub(super) struct Span<'a> {
    profile: &'a Profile,
    index: usize,
    started: Option<Instant>,
}
impl Profile {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            enabled: std::env::var("PCB_BOARD_PACKER_DETAIL").as_deref() == Ok("1"),
            calls: NAMES.iter().map(|_| AtomicU64::new(0)).collect(),
            nanos: NAMES.iter().map(|_| AtomicU64::new(0)).collect(),
        })
    }
    #[inline]
    pub fn span(&self, index: usize) -> Span<'_> {
        Span { profile: self, index, started: self.enabled.then(Instant::now) }
    }
    pub fn report(&self) {
        if !self.enabled { return; }
        let totals: serde_json::Map<String, serde_json::Value> = NAMES.iter().enumerate().map(|(i, name)| {
            (name.to_string(), serde_json::json!({ "calls": self.calls[i].load(Ordering::Relaxed),
                "workerMs": self.nanos[i].load(Ordering::Relaxed) as f64 / 1e6 }))
        }).collect();
        eprintln!("[board-detail] {}", serde_json::json!({ "totals": totals,
            "note": "nested accumulated worker time; not stage wall time" }));
    }
}
impl Drop for Span<'_> {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            self.profile.calls[self.index].fetch_add(1, Ordering::Relaxed);
            self.profile.nanos[self.index].fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }
}


pub(super) fn checkpoint(stage:&str,primitives:&[super::WorkingPrimitive],context:&super::Context) {
    if std::env::var("PCB_BOARD_GPU_VERIFY_STAGES").as_deref()!=Ok("1") {return;}
    let poses=primitives.iter().map(|p|serde_json::json!({"primitive":p.primitive,"rotation":p.rotation,
        "components":p.components.iter().map(|(index,c)|serde_json::json!({"index":index,"body":c.body_box,"holes":c.through_hole_boxes})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    eprintln!("[board-stage-checkpoint] {}",serde_json::json!({"stage":stage,"rank":super::state_rank(primitives,context),"poses":poses}));
}
