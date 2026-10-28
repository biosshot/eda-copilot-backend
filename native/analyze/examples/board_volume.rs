//! CLI wrapper around the shared library; never used as a production subprocess.
use pcb_analyze::{import_json,extraction::local::{board_mesh,board_prepare}};
fn main()->Result<(),String> {
    let threads=std::env::var("P7_FEM_THREADS").unwrap_or_else(|_|"1".into()).parse::<usize>().map_err(|_|"invalid P7_FEM_THREADS")?;
    pcb_analyze::extraction::local::parallel::with_threads(threads,execute)?
}
fn execute()->Result<(),String> {
    let args:Vec<_>=std::env::args().collect();
    if !(3..=6).contains(&args.len()) {return Err("board_volume <board.json> <request.json> [magnetic-config] [geometry|field|prepare|verify|full|train] [field-request.json]".into());}
    let config=args.get(3).map(String::as_str).unwrap_or("entity:6:8:16:3");
    let magnetic_config=config.strip_prefix("entity:").ok_or("only direct FEM entity:trace:body:exterior:stars is supported")?
        .split(':').map(|s|s.parse::<usize>().map_err(|_|"invalid direct FEM configuration"))
        .collect::<Result<Vec<_>,_>>()?;
    if magnetic_config.len()!=4 {return Err("entity:trace:body:exterior:stars required".into());}
    if args.get(4).is_some_and(|s|!["geometry","field","full","train","prepare","verify"].contains(&s.as_str())) {
        return Err("only geometry/field/full/train/prepare/verify direct FEM modes are supported".into());
    }
    let text=std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?;
    let board=import_json(&text).board.ok_or("board import rejected")?;
    let r:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(&args[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let cached_field=args.get(5).and_then(|s|s.strip_prefix("field-cache:"));
    let field_request=if let Some(path)=args.get(5).filter(|_|cached_field.is_none()) {
        Some(serde_json::from_str(&std::fs::read_to_string(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?)
    }else if cached_field.is_none(){Some(board_prepare::accepted_electric_request(&r))}else{None};
    let config:[usize;4]=magnetic_config.try_into().map_err(|_|"four magnetic parameters required")?;
    let output=board_prepare::run(&board,&r,board_prepare::Options{mode:args.get(4).map(String::as_str).unwrap_or("prepare"),magnetic_config:config,field_request:field_request.as_ref(),cached_field,frequencies_hz:if args.get(4).is_some_and(|m|m=="verify"||m=="full"){&board_prepare::FREQUENCIES_HZ}else{&[]}})?;
    println!("{output}");Ok(())
}
