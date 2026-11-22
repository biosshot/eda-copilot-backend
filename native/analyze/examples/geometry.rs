//! Independent Rust entry for checking Node-worker report parity.
use std::{env, fs, time::Instant};
fn main() {
    let args: Vec<_> = env::args().collect();
    let start = Instant::now();
    let json = fs::read_to_string(&args[1]).expect("board JSON");
    let report = pcb_analyze::geometry::prepare_json(&json, Default::default());
    fs::write(&args[2], serde_json::to_vec(&report).unwrap()).expect("report output");
    eprintln!(
        "status={} regions={} elapsed_ms={}",
        report.status,
        report.regions.len(),
        start.elapsed().as_millis()
    );
}
