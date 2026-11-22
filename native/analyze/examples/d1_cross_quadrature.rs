use pcb_analyze::extraction::local::polyhedron::{PointPotential,TetraCubature,tetra_integral};
use serde_json::Value;
fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=3{return Err("d1_cross_quadrature <magnetic-request.json> <output.json>".into());}
 let input:Value=serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let blocks=input["blocks"].as_array().ok_or("blocks required")?;let started=std::time::Instant::now();let mut samples=vec![];
 for a in 0..blocks.len(){for b in 0..a{
  let ta:Vec<[[f64;3];4]>=blocks[a]["cells"].as_array().unwrap().iter().map(|c|serde_json::from_value(c["verticesM"].clone()).unwrap()).collect();
  let tb:Vec<[[f64;3];4]>=blocks[b]["cells"].as_array().unwrap().iter().map(|c|serde_json::from_value(c["verticesM"].clone()).unwrap()).collect();
  let ca:Vec<_>=ta.iter().copied().map(|t|TetraCubature::new(t,8)).collect::<Result<_,_>>()?;let cb:Vec<_>=tb.iter().copied().map(|t|TetraCubature::new(t,8)).collect::<Result<_,_>>()?;
  let mut pairs:Vec<_>=(0..ta.len()).flat_map(|i|(0..tb.len()).map(move|j|(i,j))).map(|(i,j)|(i,j,ca[i].separation_q(&cb[j]))).filter(|v|v.2<1.).collect();
  pairs.sort_by(|a,b|a.2.total_cmp(&b.2));if pairs.is_empty(){continue;}
  for index in [0,pairs.len()/2,pairs.len()-1]{let(i,j,q)=pairs[index];let source=PointPotential::tetrahedron(tb[j])?;
   let reference=tetra_integral(ta[i],&source,16)?;let direct8=ca[i].direct_integral(&cb[j])?;
   let direct2=TetraCubature::new(ta[i],2)?.direct_integral(&TetraCubature::new(tb[j],2)?)?;
   let direct4=TetraCubature::new(ta[i],4)?.direct_integral(&TetraCubature::new(tb[j],4)?)?;
   let direct6=TetraCubature::new(ta[i],6)?.direct_integral(&TetraCubature::new(tb[j],6)?)?;
   let direct12=TetraCubature::new(ta[i],12)?.direct_integral(&TetraCubature::new(tb[j],12)?)?;
   samples.push(serde_json::json!({"a":blocks[a]["id"],"b":blocks[b]["id"],"target":i,"source":j,"q":q,"referenceM4":reference,"order2RelativeError":(direct2-reference).abs()/reference,"order4RelativeError":(direct4-reference).abs()/reference,"order6RelativeError":(direct6-reference).abs()/reference,"order8RelativeError":(direct8-reference).abs()/reference,"order12RelativeError":(direct12-reference).abs()/reference}));
   eprintln!("{} {} pair {}/{} q={q:.6} elapsed={:.3}s",blocks[a]["id"],blocks[b]["id"],index,pairs.len(),started.elapsed().as_secs_f64());
  }
 }}
 let output=serde_json::json!({"samples":samples,"wallSeconds":started.elapsed().as_secs_f64()});std::fs::write(&args[2],serde_json::to_vec_pretty(&output).unwrap()).map_err(|e|e.to_string())?;
 println!("samples={} wallSeconds={:.3}",output["samples"].as_array().unwrap().len(),started.elapsed().as_secs_f64());Ok(())
}
