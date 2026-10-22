//! Independent Rust entry; shares no Node/runtime code.
fn main() {
    let a: Vec<_> = std::env::args().collect();
    let json = std::fs::read_to_string(&a[1]).unwrap();
    let options = if a.len() > 3 {
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap()
    } else {
        Default::default()
    };
    let r = pcb_analyze::conduction::prepare_json(&json, options);
    std::fs::write(&a[2], serde_json::to_vec(&r).unwrap()).unwrap();
    eprintln!("{} {:?}", r.status, r.counts);
}
