//! Direct Newton-integral energies of the saved full and reduced loaded states.
use pcb_analyze::extraction::local::polyhedron::{PointPotential,TetraCubature,tetra_integral_adaptive,tetra_self_integral};
use serde_json::Value;

fn separated_order(q:f64,maximum:usize)->Option<usize>{
 let order=if q.is_finite()&&q<0.12{4}else if q<0.6{6}else if q<1.{8}else{return None};(order<=maximum).then_some(order)
}
fn model_energy(model:&Value,state:&Value)->Result<f64,String>{
 let nv=model["potentialNodes"].as_array().ok_or("potentialNodes required")?.len();
 let ni=model["currentDofs"].as_array().ok_or("currentDofs required")?.len();
 let reduced:Vec<f64>=serde_json::from_value(state["reducedState"].clone()).map_err(|e|e.to_string())?;
 if reduced.len()!=nv+ni&&reduced.len()!=2*(nv+ni){return Err("loaded reducedState size mismatch".into());}
 let real=&reduced[nv..nv+ni];let imaginary=if reduced.len()==nv+ni{&reduced[0..0]}else{&reduced[nv+ni+nv..]};
 let mut energy=0.;
 for entry in model["inductanceH"].as_array().ok_or("inductanceH required")?{
  let i=entry[0].as_u64().ok_or("inductance row required")? as usize;let j=entry[1].as_u64().ok_or("inductance column required")? as usize;
  if i>=ni||j>=ni{return Err("inductance index outside current model".into());}
  let l=entry[2].as_f64().ok_or("inductance value required")?;energy+=0.5*l*(real[i]*real[j]+if imaginary.is_empty(){0.}else{imaginary[i]*imaginary[j]});
 }
 Ok(energy)
}
fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=4{return Err("d1_loaded_energy <full-prepared.json> <output.json> <order>".into());}
 let input:Value=serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let order=args[3].parse::<usize>().map_err(|_|"invalid order")?;if !(8..=16).contains(&order){return Err("order 8..16 required".into());}
 let nodes:Vec<[f64;3]>=serde_json::from_value(input["nodesM"].clone()).map_err(|e|e.to_string())?;
 let cells:Vec<[usize;4]>=serde_json::from_value(input["copperCells"].clone()).map_err(|e|e.to_string())?;
 let density:Vec<f64>=serde_json::from_value(input["cellDensityPerAmpM2"].clone()).map_err(|e|e.to_string())?;
 let tetra:Vec<[[f64;3];4]>=cells.iter().map(|c|c.map(|i|nodes[i])).collect();
 let sources:Vec<_>=tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<_,_>>()?;
 let orders:Vec<_>=[4,6,8,12,16].into_iter().filter(|&value|value<=order).collect();
 let cubatures:Vec<Vec<_>>=tetra.iter().copied().map(|cell|orders.iter().map(|&value|TetraCubature::new(cell,value)).collect::<Result<Vec<_>,_>>()).collect::<Result<_,_>>()?;
 let n=tetra.len();if density.len()!=n{return Err("cell density size mismatch".into());}
 let started=std::time::Instant::now();let mut kernel=vec![0.;n*n];
 for i in 0..n{for j in 0..=i{
  let value=if i==j{tetra_self_integral(tetra[i],order)?}else{
   let q=cubatures[i][0].separation_q(&cubatures[j][0]);
   if let Some(index)=separated_order(q,order).and_then(|value|orders.iter().position(|&candidate|candidate==value)){cubatures[i][index].direct_integral(&cubatures[j][index])?}else{tetra_integral_adaptive(tetra[i],&sources[j],order,1e-12)?}
  };kernel[i*n+j]=value;kernel[j*n+i]=value;
 }eprintln!("energy kernel row {i}/{} elapsed={:.3}s",n-1,started.elapsed().as_secs_f64());}
 let direct=|ar:&[f64],ai:&[f64],br:&[f64],bi:&[f64]|->Result<f64,String>{
  if ar.len()!=3*n||ai.len()!=3*n||br.len()!=3*n||bi.len()!=3*n{return Err("loaded current size mismatch".into());}let mut contraction=0.;
  for i in 0..n{for j in 0..n{let dot=(0..3).map(|axis|ar[3*i+axis]*br[3*j+axis]+ai[3*i+axis]*bi[3*j+axis]).sum::<f64>();contraction+=density[i]*density[j]*dot*kernel[i*n+j];}}
  Ok(1e-7/2.*contraction)
 };
 let currents=|state:&Value,prefix:&str|->Result<(Vec<f64>,Vec<f64>),String>{Ok((serde_json::from_value(state[format!("{prefix}CurrentReal")].clone()).map_err(|e|e.to_string())?,serde_json::from_value(state[format!("{prefix}CurrentImaginary")].clone()).map_err(|e|e.to_string())?))};
 let model=&input["candidate"]["model"];let mut output=vec![];
 if let Some(states)=input["candidate"]["conservativeCurrent"]["states"].as_array(){for state in states{
  let (fr,fi)=currents(state,"full")?;let (rr,ri)=currents(state,"reduced")?;
  let full=direct(&fr,&fi,&fr,&fi)?;let reduced=direct(&rr,&ri,&rr,&ri)?;let model=model_energy(model,state)?;
  output.push(serde_json::json!({"frequencyHz":state["frequencyHz"],"driveA":state["driveA"],"loadsOhm":state["loadsOhm"],"fullDirectMeanMagneticJ":full,"reducedDirectMeanMagneticJ":reduced,"reducedModelMeanMagneticJ":model,"reducedModelVsFullRelative":(model-full).abs()/full.abs().max(5e-12),"reducedDirectVsFullRelative":(reduced-full).abs()/full.abs().max(5e-12)}));
 }}
 let checks=input["candidate"]["fullChecks"].as_array().ok_or("full unit checks required")?.iter().filter(|s|s["frequencyHz"].as_f64()==Some(1000.)).collect::<Vec<_>>();
 let mut full_gram=vec![vec![0.;checks.len()];checks.len()];let mut reduced_gram=full_gram.clone();let mut model_gram=full_gram.clone();
 for i in 0..checks.len(){for j in 0..=i{
  let (ar,ai)=currents(checks[i],"full")?;let (br,bi)=currents(checks[j],"full")?;let full=direct(&ar,&ai,&br,&bi)?;
  let (ar,ai)=currents(checks[i],"reduced")?;let (br,bi)=currents(checks[j],"reduced")?;let reduced=direct(&ar,&ai,&br,&bi)?;
  let model=if i==j{model_energy(model,checks[i])?}else{
   let a=checks[i]["reducedState"].as_array().ok_or("reducedState required")?;let b=checks[j]["reducedState"].as_array().unwrap();if a.len()!=b.len(){return Err("unit reducedState size mismatch".into());}
   let sum=serde_json::json!({"reducedState":a.iter().zip(b).map(|(a,b)|a.as_f64().unwrap()+b.as_f64().unwrap()).collect::<Vec<_>>()});
   0.5*(model_energy(model,&sum)?-model_energy(model,checks[i])?-model_energy(model,checks[j])?)
  };
  full_gram[i][j]=full;full_gram[j][i]=full;reduced_gram[i][j]=reduced;reduced_gram[j][i]=reduced;model_gram[i][j]=model;model_gram[j][i]=model;
 }}
 let result=serde_json::json!({"source":args[1],"order":order,"cells":n,"states":output,"unitDrives":checks.iter().map(|s|s["drive"].clone()).collect::<Vec<_>>(),"unitFullDirectEnergyGram":full_gram,"unitReducedDirectEnergyGram":reduced_gram,"unitReducedModelEnergyGram":model_gram,"wallSeconds":started.elapsed().as_secs_f64(),"formula":"mean W = mu0/(8pi) sum J_i dot J_j * integral(1/|r-r'|); full and reconstructed currents contracted independently with one Newton kernel"});
 std::fs::write(&args[2],serde_json::to_vec_pretty(&result).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;println!("{}",serde_json::to_string(&result).map_err(|e|e.to_string())?);Ok(())
}
