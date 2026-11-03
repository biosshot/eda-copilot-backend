//! Independent constant-density Newton-integral audit of saved D1 loaded states.
use pcb_analyze::extraction::local::polyhedron::{PointPotential,tetra_integral,tetra_self_integral};
use serde_json::Value;

fn number(v:&Value)->Result<f64,String>{v.as_f64().ok_or_else(||"numeric field required".into())}
fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=4&&args.len()!=5{return Err("d1_magnetic_integral <input.json> <output.json> <order> [profile-rows]".into());}
 let input:Value=serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let order=args[3].parse::<usize>().map_err(|_|"invalid quadrature order")?;
 let nodes:Vec<[f64;3]>=serde_json::from_value(input["nodesM"].clone()).map_err(|e|e.to_string())?;
 let cells:Vec<[usize;4]>=serde_json::from_value(input["cells"].clone()).map_err(|e|e.to_string())?;
 let tetra:Vec<_>=cells.iter().map(|c|c.map(|i|nodes[i])).collect();
 let sources:Vec<_>=tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<_,_>>()?;
 if let Some(rows)=args.get(4){
  let rows=rows.parse::<usize>().map_err(|_|"invalid profile row count")?;
  if rows==0{return Err("profile row count must be positive".into());}
  let mut samples=vec![];let started=std::time::Instant::now();
  for row in 0..rows.min(cells.len()){let i=(row+1)*cells.len()/rows.min(cells.len())-1;for j in 0..i {
   let mut previous=None;let mut accepted=None;let mut orders=vec![];
   for order in [2,4,8,12,16]{
    let pair_started=std::time::Instant::now();let value=tetra_integral(tetra[i],&sources[j],order)?;
    let change=previous.map(|old:f64|(value-old).abs()/value);
    orders.push(serde_json::json!({"order":order,"valueM4":value,"relativeChange":change,"seconds":pair_started.elapsed().as_secs_f64()}));
    if change.is_some_and(|v|v<=1e-10){accepted=Some(order);break;}previous=Some(value);
   }
   samples.push(serde_json::json!({"target":i,"source":j,"acceptedOrder":accepted,"orders":orders}));
  }
  println!("profile row {i}/{} elapsed {:.3}s",rows-1,started.elapsed().as_secs_f64());}
  let output=serde_json::json!({"source":input["source"],"rows":rows,"pairCount":samples.len(),"samples":samples,"wallSeconds":started.elapsed().as_secs_f64()});
  std::fs::write(&args[2],serde_json::to_vec_pretty(&output).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
  println!("profile pairs={} wallSeconds={:.3}",samples.len(),started.elapsed().as_secs_f64());return Ok(());
 }
 let density:Vec<f64>=serde_json::from_value(input["densityPerAmpM2"].clone()).map_err(|e|e.to_string())?;
 let resistance:Vec<f64>=serde_json::from_value(input["cellResistanceOhm"].clone()).map_err(|e|e.to_string())?;
 let basis:Vec<Vec<f64>>=serde_json::from_value(input["currentBasis"].clone()).map_err(|e|e.to_string())?;
 let n=cells.len();let mut kernel=vec![0.;n*n];let started=std::time::Instant::now();
 for i in 0..n {for j in 0..=i {
  let value=if i==j{tetra_self_integral(tetra[i],order)?}else{tetra_integral(tetra[i],&sources[j],order)?};
  kernel[i*n+j]=value;kernel[j*n+i]=value;
 }
 println!("row {i}/{} elapsed {:.3}s",n-1,started.elapsed().as_secs_f64());}
 let entries=input["model"]["inductanceH"].as_array().unwrap();
 let inductance:Vec<(usize,usize,f64)>=entries.iter().map(|v|Ok((v[0].as_u64().unwrap() as usize,v[1].as_u64().unwrap() as usize,number(&v[2])?))).collect::<Result<_,String>>()?;
 let mut output_states=vec![];
 for state in input["states"].as_array().unwrap(){
  let currents:Vec<[f64;2]>=serde_json::from_value(state["current"].clone()).map_err(|e|e.to_string())?;
  let q:Vec<Vec<f64>>=(0..2).map(|part|basis.iter().map(|row|row.iter().enumerate().map(|(j,&v)|v*currents[j][part]*resistance[j/3]).sum()).collect()).collect();
  let model=inductance.iter().map(|&(i,j,l)|0.5*l*(q[0][i]*q[0][j]+q[1][i]*q[1][j])).sum::<f64>();
  let mut contraction=0.;
  for i in 0..n {for j in 0..n {let dot=(0..3).map(|a|currents[3*i+a][0]*currents[3*j+a][0]+currents[3*i+a][1]*currents[3*j+a][1]).sum::<f64>();contraction+=density[i]*density[j]*dot*kernel[i*n+j];}}
  let integral=1e-7/2.*contraction;
  output_states.push(serde_json::json!({"drive":state["drive"],"modelMeanMagneticJ":model,"integralMeanMagneticJ":integral,"relativeDifference":(integral-model).abs()/model.abs().max(5e-12)}));
 }
 let output=serde_json::json!({"source":input["source"],"order":order,"cells":n,"states":output_states,"wallSeconds":started.elapsed().as_secs_f64(),"formula":"mean W = mu0/(8pi) sum J_i dot J_j * integral(1/|r-r'|); constant vector density per saved tetra; independent of Newton FEM"});
 std::fs::write(&args[2],serde_json::to_vec_pretty(&output).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 println!("{}",serde_json::to_string(&output).map_err(|e|e.to_string())?);Ok(())
}
