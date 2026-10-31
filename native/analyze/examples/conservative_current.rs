//! Cold bounded four-port full-state capture for the conservative-current diagnostic.
use pcb_analyze::{import_json,extraction::local::{board_mesh,board_prepare}};
fn main()->Result<(),String> {
    let threads=std::env::var("P7_FEM_THREADS").unwrap_or_else(|_|"1".into()).parse::<usize>().map_err(|_|"invalid P7_FEM_THREADS")?;
    pcb_analyze::extraction::local::parallel::with_threads(threads,execute)?
}
fn execute()->Result<(),String> {
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=4{return Err("conservative_current <board.json> <request.json> <output.json>".into());}
    let board_text=std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?;
    let board=import_json(&board_text).board.ok_or("board import rejected")?;
    let request:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(&args[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let output=board_prepare::run_with_budget(&board,&request,board_prepare::Options{
        mode:"full",developer_checkpoint:None,magnetic_config:[6,8,16,3],magnetic_air_degree:3,
        field_request:None,cached_field:None,frequencies_hz:&[1000.],
    },300.)?;
    if output["candidate"]["verificationStatus"]!="complete"||output["candidate"]["fullChecks"].as_array().map_or(true,|v|v.len()!=4){
        return Err("four exact full unit-port states were not completed".into());
    }
    std::fs::write(&args[3],serde_json::to_vec(&output).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    println!("conservative-current full capture complete: {} faces, {} unit states",output["candidate"]["conservativeCurrent"]["faces"].as_array().map_or(0,Vec::len),output["candidate"]["fullChecks"].as_array().map_or(0,Vec::len));
    Ok(())
}
