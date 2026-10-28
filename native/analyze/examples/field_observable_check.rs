//! Bounded h/domain check using the saved real-board reduction, not full C.
use pcb_analyze::{import_json,electrical::coupled,extraction::local::{board_mesh,volume_current,volume_field,AllocationBudget}};
use pcb_analyze::extraction::local::volume_field::ElectricApply;
use std::{fs,time::{Duration,Instant}};
#[path="../../../tests/fixtures/analysis-local-extraction/current-space/fixture.rs"]
#[allow(dead_code)]mod fixture;
fn main()->Result<(),String>{
    pcb_analyze::extraction::local::parallel::with_threads(1,execute)?
}
fn execute()->Result<(),String>{
    let args=std::env::args().collect::<Vec<_>>();if args.len()!=5{return Err("field_observable_check board request prepared-output field-request".into());}
    let started=Instant::now();let remaining=||Duration::from_secs(300).checked_sub(started.elapsed()).ok_or("shared field-check deadline");
    let board=import_json(&fs::read_to_string(&args[1]).map_err(|e|e.to_string())?).board.ok_or("board import")?;
    let read=|p:&str|fs::read_to_string(p).map_err(|e|e.to_string());
    let r:board_mesh::Request=serde_json::from_str(&read(&args[2])?).map_err(|e|e.to_string())?;
    let refined:board_mesh::Request=serde_json::from_str(&read(&args[4])?).map_err(|e|e.to_string())?;
    if board_mesh::physical_signature(&r)!=board_mesh::physical_signature(&refined){return Err("field check changed physical inputs".into());}
    let budget=AllocationBudget{max_bytes:2*1024*1024*1024,max_nodes:100000,max_tetrahedra:262144};
    let b=board_mesh::build(&board,&r,budget,remaining()?)?;
    let field_geometry=board_mesh::build(&board,&refined,budget,remaining()?)?;
    if (b.mesh.copper_volume_m3-field_geometry.mesh.copper_volume_m3).abs()>1e-8*b.mesh.copper_volume_m3{return Err("refinement changed copper volume".into());}
    let electrodes=r.electrodes.iter().map(|e|b.electrode_nodes(e)).collect::<Result<Vec<_>,_>>()?;
    let space=volume_current::assemble(&b.mesh,&b.conductivity_s_m,&electrodes,budget)?;
    let field=volume_field::Field::new_vertical_refined(&field_geometry.mesh,&b.mesh,&space,budget)?;
    let saved:serde_json::Value=serde_json::from_str(&read(&args[3])?).map_err(|e|e.to_string())?;
    let basis:Vec<Vec<f64>>=serde_json::from_value(saved["candidate"]["voltageBasis"].clone()).map_err(|e|e.to_string())?;
    let base:coupled::Model=serde_json::from_value(saved["candidate"]["model"].clone()).map_err(|e|e.to_string())?;
    let mut candidate=base.clone();candidate.capacitance_f=field.energy_gram(&basis)?;
    // Reserve both meshes, field/current storage and conservative JSON/model
    // copies before handing the remaining shared budget to P6.
    let saved_bytes=fs::metadata(&args[3]).map_err(|e|e.to_string())?.len() as usize;
    let reserved=b.mesh.retained_bytes()?.checked_add(field_geometry.mesh.retained_bytes()?)
        .and_then(|n|n.checked_add(space.planned_bytes)).and_then(|n|n.checked_add(field.planned_bytes()))
        .and_then(|n|n.checked_add(saved_bytes.checked_mul(32)?)).ok_or("field check live budget overflow")?;
    let p6_bytes=budget.max_bytes.checked_sub(reserved).filter(|&n|n>0).ok_or("field check shared budget exhausted")?;
    let limit=pcb_analyze::electrical::Budget{max_memory_mi_b:Some(p6_bytes as f64/1048576.),max_seconds:Some(remaining()?.as_secs_f64())};
    let coarse=coupled::impedance(&base,r.ports.len(),&fixture::FREQUENCIES_HZ,Some(&limit))?;
    let fine=coupled::impedance(&candidate,r.ports.len(),&fixture::FREQUENCIES_HZ,Some(&limit))?;
    println!("{}",serde_json::json!({"kind":"unaccepted-real-field-observable-refinement-v1","physicalAdmission":false,"elapsedSeconds":started.elapsed().as_secs_f64(),"fieldNodes":field_geometry.mesh.nodes_m.len(),"fieldTetrahedra":field_geometry.mesh.tetrahedra.len(),"copperVolumeM3":b.mesh.copper_volume_m3,"refinedCopperVolumeM3":field_geometry.mesh.copper_volume_m3,"base":coarse,"refined":fine,"refinedCapacitanceF":candidate.capacitance_f,"limitations":"same direct FEM, fixed existing reduction subspace; h/domain sensitivity check, not independent continuum oracle"}));Ok(())
}
