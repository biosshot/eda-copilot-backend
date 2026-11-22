//! Developer-only cold full-volume capture on a hash-bound matched port axis.
use std::io::{BufWriter, Write};

use pcb_analyze::{
    extraction::local::{board_mesh, board_prepare, parallel},
    import_json,
};

fn main() -> Result<(), String> {
    if !matches!(std::env::var("P7_VERIFY_SECONDS").as_deref(), Ok("1800"))
        || !matches!(std::env::var("P7_VERIFY_MEMORY_MIB").as_deref(), Ok("4096"))
        || !matches!(std::env::var("PCB_ANALYSIS_TRAINING_JOBS").as_deref(), Ok("1"))
    {
        return Err("matched D1 requires its supervised 1800 s / 4096 MiB / 1 job environment".into());
    }
    parallel::with_threads(1, execute)?
}

fn execute() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 5 {
        return Err("d1_matched_full <board.json> <request.json> <axis.json> <output.json>".into());
    }
    let read = |i: usize| std::fs::read_to_string(&args[i]).map_err(|e| e.to_string());
    let board = import_json(&read(1)?).board.ok_or("board import rejected")?;
    let request: board_mesh::Request =
        serde_json::from_str(&read(2)?).map_err(|e| e.to_string())?;
    let axis: Vec<f64> = serde_json::from_str(&read(3)?).map_err(|e| e.to_string())?;
    if request.ports.len() != 4 || axis.len() != 5 || axis.first() != Some(&1000.) {
        return Err("matched D1 requires four ordered physical ports and five AC points starting at 1 kHz".into());
    }
    board_prepare::verification_axis(&axis)?;
    let output = board_prepare::run_with_budget(
        &board, &request,
        board_prepare::Options {
            mode: "full", developer_checkpoint: None, magnetic_config: [6, 8, 16, 3],
            magnetic_air_degree: 3, field_request: None, cached_field: None, frequencies_hz: &axis,
        },
        1800.,
    )?;
    // Preserve any completed full unit states when verification is incomplete.
    // The comparator refuses incomplete coverage.
    let out = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(&args[4]).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(out);
    serde_json::to_writer(&mut writer, &output).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    if output["candidate"]["verificationStatus"] != "complete"
        || output["candidate"]["fullChecks"].as_array().map_or(true, |rows| rows.len() != 4 * axis.len())
        || output["candidate"]["conservativeCurrent"]["states"].as_array().map_or(true, |rows| rows.len() != 2)
    {
        return Err("matched D1 full unit states or loaded RT0 state coverage incomplete".into());
    }
    Ok(())
}
