//! Bounded diagnostic: direct constant-current self matrices of saved section blocks.
use pcb_analyze::extraction::local::polyhedron::{PointPotential,TetraCubature,tetra_integral_adaptive,tetra_self_integral};
use serde_json::Value;

fn separated_order(q:f64,maximum:usize)->Option<usize>{
 let order=if q.is_finite()&&q<0.12{4}else if q<0.6{6}else if q<1.{8}else{return None};(order<=maximum).then_some(order)
}

fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=4{return Err("d1_block_self <magnetic-request.json> <output.json> <order>".into());}
 let input:Value=serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let order=args[3].parse::<usize>().map_err(|_|"invalid order")?;if !(8..=16).contains(&order){return Err("order 8..16 required".into());}
 let orders:Vec<_>=[4,6,8,12,16].into_iter().filter(|&v|v<=order).collect();let started=std::time::Instant::now();let mut output=vec![];
 for block in input["blocks"].as_array().ok_or("blocks required")?.iter().filter(|b|b["id"].as_str().is_some_and(|id|id.starts_with("run-"))){
  let cells=block["cells"].as_array().ok_or("cells required")?;let dofs=block["dofs"].as_u64().ok_or("dofs required")? as usize;
  let tetra:Vec<[[f64;3];4]>=cells.iter().map(|c|serde_json::from_value(c["verticesM"].clone()).map_err(|e|e.to_string())).collect::<Result<_,_>>()?;
  let sources:Vec<_>=tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<_,_>>()?;
  let cubatures:Vec<Vec<_>>=tetra.iter().copied().map(|t|orders.iter().map(|&v|TetraCubature::new(t,v)).collect::<Result<Vec<_>,_>>()).collect::<Result<_,_>>()?;
  let n=tetra.len();let mut kernel=vec![0.;n*n];
  for i in 0..n{for j in 0..=i{
   let value=if i==j{tetra_self_integral(tetra[i],order)?}else{let q=cubatures[i][0].separation_q(&cubatures[j][0]);if let Some(k)=separated_order(q,order).and_then(|v|orders.iter().position(|&candidate|candidate==v)){cubatures[i][k].direct_integral(&cubatures[j][k])?}else{tetra_integral_adaptive(tetra[i],&sources[j],order,1e-12)?}};
   kernel[i*n+j]=value;kernel[j*n+i]=value;
  }}
  let mut matrix=vec![vec![0.;dofs];dofs];
  for(i,target)in cells.iter().enumerate(){for(j,source)in cells.iter().enumerate(){for u in 0..dofs{for v in 0..dofs{
   let a:Vec<f64>=serde_json::from_value(target["currentsPerAmpM2"][u].clone()).map_err(|e|e.to_string())?;let b:Vec<f64>=serde_json::from_value(source["currentsPerAmpM2"][v].clone()).map_err(|e|e.to_string())?;
   let dot=(0..3).map(|k|a[k]*b[k]).sum::<f64>();matrix[u][v]+=dot*kernel[i*n+j];
  }}}}
  output.push(serde_json::json!({"id":block["id"],"matrixH":matrix.into_iter().map(|row|row.into_iter().map(|v|1e-7*v).collect::<Vec<_>>()).collect::<Vec<_>>(),"cells":n,"dofs":dofs}));
  eprintln!("{} self complete: {:.3}s",block["id"],started.elapsed().as_secs_f64());
 }
 let result=serde_json::json!({"blocks":output,"order":order,"wallSeconds":started.elapsed().as_secs_f64()});std::fs::write(&args[2],serde_json::to_vec_pretty(&result).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;println!("{}",serde_json::to_string(&result).map_err(|e|e.to_string())?);Ok(())
}
