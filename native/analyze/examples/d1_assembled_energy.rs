//! Replay saved assembled matched states and resolve magnetic energy by block.
use pcb_analyze::electrical::{Budget,Counters,coupled::{Model,System}};
use serde_json::Value;
use std::collections::BTreeMap;
fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=3&&args.len()!=4{return Err("d1_assembled_energy <segmented.json> <output.json> [owner-source-segmented.json]".into());}
 let input:Value=serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let owner_input:Value=if args.len()==4{serde_json::from_slice(&std::fs::read(&args[3]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?}else{input.clone()};
 let result=&input["result"];let model:Model=serde_json::from_value(result["assembledModel"].clone()).map_err(|e|e.to_string())?;
 let recovery=owner_input["result"]["physicalRecovery"]["currents"].as_array().ok_or("current recovery required")?;
 let owners:Vec<String>=model.current_dofs.iter().map(|dof|{let name=dof.split(':').next().unwrap_or("");recovery.iter().find(|r|r["instanceId"].as_str()==Some(name)&&r["dof"].as_str()==dof.split(':').nth(1)).map(|_|name.to_string()).ok_or_else(||format!("unmapped current DOF {dof}"))}).collect::<Result<_,_>>()?;
 let ports=result["ports"].as_array().ok_or("ports required")?.len();let nv=model.potential_nodes.len();let n=nv+model.current_dofs.len();
 let mut system=System::compile(&model,ports,Some(&Budget{max_seconds:Some(30.),max_memory_mi_b:Some(512.)}))?;
 for port in 0..ports{system.add_port_admittance(port,1./50.,0.)?;}
 let mut states=vec![];let mut stats=Counters::default();
 for input_port in 0..ports{let mut drive=vec![0.;ports];drive[input_port]=1.;let mut rhs=system.port_rhs(&drive);rhs.resize(2*n,0.);states.push(system.solve(2.*std::f64::consts::PI*1000.,rhs,&vec![0.;n],&mut stats)?);}
 let mut groups:BTreeMap<String,Vec<Vec<f64>>>=BTreeMap::new();let mut total=vec![vec![0.;ports];ports];
 for &(i,j,l) in &model.inductance_h{let a=&owners[i];let b=&owners[j];let key=if a==b{format!("self:{a}")}else if a<=b{format!("cross:{a}:{b}")}else{format!("cross:{b}:{a}")};let gram=groups.entry(key).or_insert_with(||vec![vec![0.;ports];ports]);
  for p in 0..ports{for q in 0..ports{let value=0.5*l*(states[p][nv+i]*states[q][nv+j]+states[p][n+nv+i]*states[q][n+nv+j]);gram[p][q]+=value;total[p][q]+=value;}}
 }
 let saved:Vec<Vec<f64>>=serde_json::from_value(result["loadedEnergyGram"][0]["magneticJPerA2"].clone()).map_err(|e|e.to_string())?;
 let mut max_error:f64=0.;for p in 0..ports{for q in 0..ports{max_error=max_error.max((total[p][q]-saved[p][q]).abs());}}
 let output=serde_json::json!({"source":args[1],"frequencyHz":1000.,"referenceOhm":50.,"potentialCount":nv,"states":states,"owners":owners,"groups":groups,"total":total,"saved":saved,"maxReplayAbsoluteErrorJ":max_error});
 std::fs::write(&args[2],serde_json::to_vec_pretty(&output).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;println!("{}",serde_json::to_string(&output).map_err(|e|e.to_string())?);Ok(())
}
