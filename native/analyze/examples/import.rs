//! Diagnostic reader, not an analysis runner or a Node addon.
use pcb_analyze::import_json;
use serde_json::json;
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let paths: Vec<_> = env::args_os().skip(1).collect();
    if paths.is_empty() {
        eprintln!("Usage: cargo run --example import -- <RoutingBoardV2.json> ...");
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    for path in paths {
        match fs::read_to_string(&path) {
            Ok(text) => {
                let report = import_json(&text);
                failed |= report.has_errors();
                println!(
                    "{}",
                    json!({"file":path.to_string_lossy(),"accepted":report.accepted,"imported":report.board.is_some(),"diagnostics":report.diagnostics})
                );
            }
            Err(e) => {
                failed = true;
                eprintln!("{}: {e}", path.to_string_lossy());
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
