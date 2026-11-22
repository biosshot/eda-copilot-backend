//! Independent Rust replay of a saved DC request, without Node or DSL adaptation.
fn main() {
    let a: Vec<_> = std::env::args().collect();
    let mut request: pcb_analyze::dc::Request =
        serde_json::from_str(&std::fs::read_to_string(&a[2]).unwrap()).unwrap();
    request.board_json = Some(std::fs::read_to_string(&a[1]).unwrap());
    let r = pcb_analyze::dc::Session::default().solve(request);
    std::fs::write(&a[3], serde_json::to_vec(&r).unwrap()).unwrap();
    eprintln!(
        "status={} diagnostics={} elapsed_ms={}",
        r["status"], r["diagnostics"], r["elapsed_ms"]
    );
}
