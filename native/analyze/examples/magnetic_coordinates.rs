use pcb_analyze::{import_json,extraction::local::{board_mesh,board_prepare,volume_magnetic::Operator}};
fn main()->Result<(),String>{
 let args=std::env::args().collect::<Vec<_>>();if args.len()!=3{return Err("magnetic_coordinates board.json request.json".into());}
 let board=import_json(&std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?).board.ok_or("board import")?;
 let request:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(&args[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let budget=board_prepare::allocation_budget(8192);let start=std::time::Instant::now();let built=board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(600))?;
 for (profile,air,trace,body) in [("baseline",3,4,6),("compact-v1",2,4,4)]{
  std::env::set_var("P7_MAGNETIC_PROFILE",profile);
  let audit=Operator::audit_patch_coordinates(&built.mesh,budget,air,trace,body,1)?;
  println!("{}",serde_json::json!({"profile":profile,"audit":audit,"elapsedSeconds":start.elapsed().as_secs_f64()}));
 }Ok(())
}
