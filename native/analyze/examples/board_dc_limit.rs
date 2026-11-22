//! Validation only: existing full conductive FEM versus a restored local P6 DC limit.
//! No AC extraction, magnetic assembly, altered ports or production band extension.
use pcb_analyze::{import_json,extraction::local::{board_mesh,board_response,volume_current,AllocationBudget}};
fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=3{return Err("board_dc_limit <board.json> <cache-envelope.json>".into());}
 let board=import_json(&std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?).board.ok_or("board rejected")?;
 let envelope:serde_json::Value=serde_json::from_slice(&std::fs::read(&args[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let prepared:serde_json::Value=serde_json::from_str(envelope["payload"].as_str().ok_or("cache payload")?).map_err(|e|e.to_string())?;
 let r:board_mesh::Request=serde_json::from_value(prepared["request"].clone()).map_err(|e|e.to_string())?;
 if r.ports.len()!=2{return Err("two original physical ports required".into());}
 let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 let built=board_mesh::build(&board,&r,budget,std::time::Duration::from_secs(30))?;
 let electrodes=r.electrodes.iter().map(|e|built.electrode_nodes(e)).collect::<Result<Vec<_>,_>>()?;
 let space=volume_current::assemble(&built.mesh,&built.conductivity_s_m,&electrodes,budget)?;
 let contacts:Vec<_>=electrodes.iter().map(|nodes|space.node_potential[&nodes[0]]).collect();
 let ports:Vec<_>=r.ports.iter().map(|p|p.iter().map(|name|r.electrodes.iter().position(|e|&e.name==name).unwrap()).collect::<Vec<_>>()).collect();
 let drive=[1.,-1.];let mut injection=vec![0.;space.potential_count()];
 for(p,&a)in ports.iter().zip(&drive){injection[contacts[p[0]]]+=a;injection[contacts[p[1]]]-=a;}
 for component in &space.components{if component.iter().map(|&i|injection[i]).sum::<f64>().abs()>1e-12{return Err("incompatible component DC injection".into());}}
 let voltage=space.dc_factor(&[],budget)?.solve(&space,&vec![0.;injection.len()],&injection)?;
 let mut current=space.gradient(&voltage)?;for(i,cell)in space.cells.iter().enumerate(){for k in 0..3{current[3*i+k]/=cell.resistance_ohm;}}
 let full_losses=space.losses(&current)?;let full_loss:f64=full_losses.iter().sum();
 let full_power:f64=injection.iter().zip(&voltage).map(|(i,v)|i*v).sum();
 let reduced=board_response::solve(board_response::Request{prepared,frequencies_hz:vec![0.],drive:drive.to_vec(),loads_ohm:vec![]})?;
 let sample=&reduced["samples"][0];let reduced_loss=sample["totalLossW"].as_f64().unwrap();
 let scale=current.iter().copied().map(f64::abs).fold(0.,f64::max);
 let current_error=current.iter().enumerate().map(|(i,&v)|(v-sample["currentRealImaginaryA"][i][0].as_f64().unwrap()).abs()/v.abs().max(1e-12*scale)).fold(0.,f64::max);
 let loss_error=(full_loss-reduced_loss).abs()/full_loss.max(1e-18);
 let power_error=(full_power-full_loss).abs()/full_loss.max(1e-18);
 println!("{}",serde_json::json!({"status":if current_error<=0.05&&loss_error<=0.05&&power_error<=1e-8{"pass"}else{"failed"},"drive":drive,"fullLossW":full_loss,"fullSourcePowerW":full_power,"reducedLossW":reduced_loss,"maxCurrentCoordinateError":current_error,"totalLossError":loss_error,"fullPowerError":power_error,"coordinateRelativeFloor":1e-12,"lossFloorW":1e-18,"physicalTolerance":0.05,"numericalPowerGuard":1e-8,"scope":"Existing full conductive FEM; balanced component-compatible DC only. Individual island common potentials require retained charge constraints; no AC band extension."}));
 if current_error>0.05||loss_error>0.05||power_error>1e-8{return Err("DC limit validation failed".into());}Ok(())
}
