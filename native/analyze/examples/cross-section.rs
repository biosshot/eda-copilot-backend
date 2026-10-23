//! Small transport-parity tool, calls the production extractor (no second solver).
use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin()
        .take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut input)
        .unwrap();
    assert!(input.len() <= 8 * 1024 * 1024);
    let report = pcb_analyze::extraction::run(serde_json::from_str(&input).unwrap());
    println!(
        "{}",
        serde_json::json!({"status":report["status"],"diagnostics":report["diagnostics"],"parameters":report["parameters"],"quality":report["field"]["quality"],"nodes":report["field"]["nodes"].as_array().map(Vec::len),"triangles":report["field"]["triangles"].as_array().map(Vec::len)})
    );
}
