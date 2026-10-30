//! Bounded developer low-frequency holdouts of the existing accepted direct-FEM
//! operator. Does not extend the production local band or bypass journal identity.
use pcb_analyze::{import_json,extraction::local::{board_mesh,board_prepare,developer_checkpoint::{Journal,identity}}};
fn main()->Result<(),String>{
 pcb_analyze::extraction::local::parallel::initialize();
 let args=std::env::args().collect::<Vec<_>>();if args.len()!=4{return Err("board_low_band <board.json> <request.json> <journal-directory>".into());}
 let text=std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?;
 let board=import_json(&text).board.ok_or("board import rejected")?;
 let request:board_mesh::Request=serde_json::from_slice(&std::fs::read(&args[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let field=board_prepare::accepted_electric_request(&request);let config=[6,8,16,3];
 let frequencies=[1.,10.,100.,316.22776601683796,1000.];
 let journal=Journal::new(std::path::Path::new(&args[3]),identity(&text,&serde_json::json!({"request":request,"fieldRequest":field,"config":config,"lowBandHoldoutsHz":frequencies}))?)?;
 let output=board_prepare::run(&board,&request,board_prepare::Options{mode:"verify",developer_checkpoint:Some(&journal),magnetic_config:config,magnetic_air_degree:3,field_request:Some(&field),cached_field:None,frequencies_hz:&frequencies})?;
 println!("{output}");if output["candidate"]["verificationStatus"]!="complete"{return Err("low-frequency verification incomplete; resume the same journal under the external300s/2GiB supervisor".into());}Ok(())
}
