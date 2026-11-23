//! Repropagation of immutable, identity-bound section coefficients. No FEM.
use super::{error,geometry,losses,polygon_section,propagation,Request,Result};
use serde_json::{Value,json};
use std::time::Instant;
fn physical(mut line:Value)->Value{if let Some(v)=line.as_object_mut(){v.remove("lengthMm");}if let Some(v)=line.pointer_mut("/geometry/loss").and_then(Value::as_object_mut){v.remove("frequenciesHz");}line}
fn matrix(v:&Value)->Result<Vec<Vec<f64>>>{serde_json::from_value(v.clone()).map_err(|e|error("SECTION_CACHE",e))}
fn gram(m:&[Vec<f64>],n:usize,positive:bool)->Result<()>{
 if n==0||n>16||m.len()!=n||m.iter().any(|r|r.len()!=n||r.iter().any(|x|!x.is_finite())){return Err(error("SECTION_CACHE","finite complete energy matrix required"));}
 let scale=m.iter().flatten().map(|x|x.abs()).fold(0.,f64::max);if scale==0.{return if positive{Err(error("SECTION_CACHE","positive energy required"))}else{Ok(())};}
 for i in 0..n{for j in 0..n{if(m[i][j]-m[j][i]).abs()>1e-9*scale{return Err(error("SECTION_CACHE","asymmetric stored energy"));}}}
 let eig=faer::Mat::from_fn(n,n,|i,j|m[i][j]/scale).self_adjoint_eigen(faer::Side::Lower).map_err(|e|error("SECTION_CACHE",format!("{e:?}")))?;
 if(0..n).any(|i|if positive{eig.S()[i]<=1e-12}else{eig.S()[i]< -1e-10}){return Err(error("SECTION_CACHE","stored energy is not positive (semi)definite"));}Ok(())
}
fn audit(v:&Value,key:&str)->Result<()>{if v[key].as_f64().is_none_or(|x|!x.is_finite()||x<0.||x>1e-8){return Err(error("SECTION_CACHE",format!("invalid {key} audit")));}Ok(())}
fn surfaces(v:&Value,n:usize,ids:&[String])->Result<Vec<losses::Surface>>{
 let rows:Vec<losses::Surface>=serde_json::from_value(v.clone()).map_err(|e|error("SECTION_CACHE",e))?;
 if rows.len()!=ids.len(){return Err(error("SECTION_CACHE","surface ownership count changed"));}
 for(s,id)in rows.iter().zip(ids){if &s.id!=id||s.integrated_current_a_per_a.len()!=n||s.integrated_current_a_per_a.iter().any(|v|!v.is_finite())||s.current_a_per_m_per_a.len()!=n||s.current_a_per_m_per_a.iter().any(|v|v.len()!=s.node_indices.len()||v.iter().any(|x|!x.is_finite())){return Err(error("SECTION_CACHE","surface identity/current mapping changed"));}gram(&s.gram_per_m,n,false)?;}
 for k in 0..n{for(g,s)in rows.iter().take(n).enumerate(){if(s.integrated_current_a_per_a[k]-if g==k{1.}else{0.}).abs()>1e-8{return Err(error("SECTION_CACHE","signal current normalization changed"));}}if rows.iter().map(|s|s.integrated_current_a_per_a[k]).sum::<f64>().abs()>1e-8{return Err(error("SECTION_CACHE","physical reference return normalization changed"));}}
 Ok(rows)
}
fn regional(rows:&[Vec<Vec<f64>>],c:&[Vec<f64>])->Result<()>{let n=c.len();for a in rows{gram(a,n,false)?;}for i in 0..n{for j in 0..n{if(rows.iter().map(|a|a[i][j]).sum::<f64>()-c[i][j]).abs()>1e-8*c[i][i]{return Err(error("SECTION_CACHE","regional energy no longer sums to C"));}}}Ok(())}
/// Strict physical/material identity is supplemented by native energy/passivity
/// validation. Cache checksums/versioning belong to the existing host cache.
pub fn replay(input:Request,mut stored:Value)->Value{
 let start=Instant::now();let result=(||->Result<Value>{
  let stored_mesh = serde_json::from_value::<Option<super::MeshOptions>>(stored["fieldRequestMesh"].clone())
   .map_err(|e|error("SECTION_CACHE",format!("invalid stored mesh options: {e}")))?;
  if stored["status"]!="complete"||physical(stored["input"].clone())!=physical(input.line.clone())||stored_mesh!=input.mesh||!stored["convergence"].is_array(){return Err(error("SECTION_CACHE","complete identical physical section required"));}
  let seconds=input.budget.as_ref().and_then(|b|b.max_seconds).unwrap_or(120.);let memory=input.budget.as_ref().and_then(|b|b.max_memory_mi_b).unwrap_or(768.);
  if !seconds.is_finite()||seconds<=0.||!memory.is_finite()||memory<1.{return Err(error("SECTION_BUDGET","positive finite replay budget required"));}
  let polygon=input.line["kind"]=="polygon-section";
  let c=matrix(&stored["parameters"]["capacitanceFPerM"])?;let c0=matrix(&stored["parameters"]["vacuumCapacitanceFPerM"])?;let n=c.len();gram(&c,n,true)?;gram(&c0,n,true)?;
  let mut p=if polygon{polygon_section::parameters(&c,&c0)?}else{propagation::parameters(&c,&c0)?};
  let stored_l=matrix(&stored["parameters"]["inductanceHPerM"])?;gram(&stored_l,n,true)?;let derived_l=matrix(&p["inductanceHPerM"])?;for i in 0..n{for j in 0..n{if(stored_l[i][j]-derived_l[i][j]).abs()>1e-9*derived_l[i][i].abs().max(derived_l[j][j].abs()){return Err(error("SECTION_CACHE","stored L is inconsistent with vacuum C"));}}}
  let quality=if polygon{vec![&stored["quality"],&stored["vacuumQuality"]]}else{vec![&stored["field"]["quality"]["actual"],&stored["field"]["quality"]["vacuum"]]};for q in quality{for key in ["normalizedResidual","normalizedSymmetry","normalizedChargeBalance","normalizedEnergyReactionError"]{audit(q,key)?;}}
  let length=input.line.get("lengthMm").map(|v|v.as_f64().filter(|x|x.is_finite()&&*x>0.).map(|x|x*1e-3).ok_or_else(||error("SECTION_INPUT","positive finite length required"))).transpose()?;
  let max_frequency=if polygon{use geo::BoundingRect;let r:polygon_section::Request=serde_json::from_value(input.line["geometry"].clone()).map_err(|e|error("SECTION_INPUT",e))?;let domain=r.domain.polygon()?;let bounds=domain.bounding_rect().unwrap();let epsilon=r.dielectrics.iter().map(|d|d.relative_permittivity).fold(1.,f64::max);299792458./(20.*bounds.width().max(bounds.height())*epsilon.sqrt())}else{let g=geometry::parse(&input.line)?;299792458./(20.*g.scale()*g.max_epsilon().sqrt())};

  if input.frequencies.is_empty()||input.frequencies.len()>8192||input.frequencies.len()*(2*n).pow(2)>131072||input.frequencies.iter().enumerate().any(|(i,f)|!f.is_finite()||*f<0.||*f>max_frequency||i>0&&*f<=input.frequencies[i-1]){return Err(error("SECTION_APPLICABILITY","invalid axis or geometric frequency screen"));}
  let original_axis=stored.pointer("/fieldReuse/originalAxisHz").unwrap_or(&stored["frequenciesHz"]).clone();let preparation_ms=stored.pointer("/fieldReuse/fieldPreparationElapsedMs").unwrap_or(&stored["elapsedMs"]).clone();let mut network=Value::Null;
  if polygon{
   let mut r:polygon_section::Request=serde_json::from_value(input.line["geometry"].clone()).map_err(|e|error("SECTION_INPUT",e))?;
   if let Some(loss)=&mut r.loss{
    loss.frequencies_hz=input.frequencies.clone();let f=&stored["frequencyDependent"];audit(f,"currentNormalizationError")?;audit(f,"regionalEnergyError")?;
    let ids=r.conductors.iter().filter(|c|c.role=="signal").chain(r.conductors.iter().filter(|c|c.role=="reference")).map(|c|c.id.clone()).collect::<Vec<_>>();
    let ss=surfaces(&f["surfaceIntegrals"],n,&ids)?;let rr:Vec<Vec<Vec<f64>>>=serde_json::from_value(f["regionalCapacitanceFPerM"].clone()).map_err(|e|error("SECTION_CACHE",e))?;if rr.len()!=r.dielectrics.len(){return Err(error("SECTION_CACHE","regional material ownership changed"));}regional(&rr,&c)?;
    let samples=polygon_section::loss_samples(loss,&p,&ss,&rr,&ids)?;
    if let Some(l)=length{let mut params=vec![];let mut ports=Value::Null;let mut excess=0_f64;let mut reciprocity=0_f64;for sample in &samples{let waves=polygon_section::network(sample,&[sample["frequencyHz"].as_f64().unwrap()],l,50.)?;params.push(waves["parameters"][0].clone());ports=waves["ports"].clone();excess=excess.max(waves["quality"]["maxPassivityExcess"].as_f64().unwrap());reciprocity=reciprocity.max(waves["quality"]["reciprocityResidual"].as_f64().unwrap());}network=json!({"representation":"S","referenceOhm":50.,"frequenciesHz":input.frequencies,"ports":ports,"parameters":params,"quality":{"maxPassivityExcess":excess,"reciprocityResidual":reciprocity},"phaseConvention":"exp(+j omega t); passive complex coupled modes"});}
    stored["frequencyDependent"]["samples"]=json!(samples);p.as_object_mut().unwrap().remove("resistanceOhmPerM");p.as_object_mut().unwrap().remove("conductanceSPerM");
   }else if let Some(l)=length{network=polygon_section::network(&p,&input.frequencies,l,50.)?;}
  }else{
   let g=geometry::parse(&input.line)?;let config=losses::Configuration::parse(&input.line,&g)?;
   if let Some(config)=config{
    config.screen(&input.frequencies)?;if input.frequencies.len()*((2*n).pow(2)+n*n*(6+n+g.reference_y_m.len()+2*g.dielectrics.len())+n*32)>131072{return Err(error("SECTION_BUDGET","loss/output coefficient cap131072"));}let v=&stored["lossIntegrals"];audit(v,"maxCurrentNormalizationError")?;audit(v,"maxRegionalCapacitanceError")?;
    let ids=g.conductors.iter().map(|c|c.id.clone()).chain((0..g.reference_y_m.len()).map(|i|if i==0{"bottom"}else{"top"}.into())).collect::<Vec<_>>();
    // Producer surface IDs are retained in order; signal IDs are physical geometry IDs.
    let expected=v["surfaces"].as_array().ok_or_else(||error("SECTION_CACHE","missing surface ownership"))?.iter().map(|s|s["id"].as_str().unwrap_or("").to_string()).collect::<Vec<_>>();if expected!=ids{return Err(error("SECTION_CACHE","conductor mapping changed"));}
    let ss=surfaces(&v["surfaces"],n,&expected)?;let regions:Vec<losses::Region>=serde_json::from_value(v["regions"].clone()).map_err(|e|error("SECTION_CACHE",e))?;
    if regions.len()!=g.dielectrics.len()||regions.iter().zip(&g.dielectrics).any(|(a,b)|a.id!=b.id){return Err(error("SECTION_CACHE","material region mapping changed"));}regional(&regions.iter().map(|r|r.capacitance_f_per_m.clone()).collect::<Vec<_>>(),&c)?;
    let integrals=losses::Integrals{surfaces:ss,regions,normalization:"restored validated immutable integrals",max_current_normalization_error:v["maxCurrentNormalizationError"].as_f64().unwrap(),max_regional_capacitance_error:v["maxRegionalCapacitanceError"].as_f64().unwrap()};
    let samples=losses::samples(&p,&integrals,&config,&input.frequencies,length)?;if let Some(l)=length{network=propagation::lossy_network(&samples,l,50.)?;}let mut dependency=stored["parameters"]["frequencyDependent"].clone();dependency["frequenciesHz"]=json!(input.frequencies);dependency["samples"]=json!(samples);p["frequencyDependent"]=dependency;p.as_object_mut().unwrap().remove("resistanceOhmPerM");p.as_object_mut().unwrap().remove("conductanceSPerM");
   }else if let Some(l)=length{network=propagation::network(&p,&input.frequencies,l,50.)?;}
  }
  stored["applicability"]["maxFrequencyHz"]=json!(max_frequency);stored["input"]=input.line;stored["frequenciesHz"]=json!(input.frequencies);stored["parameters"]=p;stored["network"]=network;
  for pointer in ["/geometry","/field/geometry"]{if let Some(v)=stored.pointer_mut(pointer).and_then(Value::as_object_mut){v.insert("length".into(),json!(length));}}
  stored["fieldReuse"]=json!({"assembly":0,"factorization":0,"originalAxisHz":original_axis,"fieldPreparationElapsedMs":preparation_ms,"validation":"identical physical geometry/materials/mesh; native energy, loss and geometric applicability rechecked; no wider physical band is inferred"});
  if start.elapsed().as_secs_f64()>seconds{return Err(error("SECTION_TIMEOUT","repropagation deadline"));}stored["elapsedMs"]=json!(start.elapsed().as_secs_f64()*1000.);Ok(stored)
 })();match result{Ok(v)=>v,Err(e)=>json!({"status":"rejected","diagnostics":[{"severity":"error","code":e.split(':').next().unwrap_or("SECTION_CACHE"),"message":e}],"elapsedMs":start.elapsed().as_secs_f64()*1000.})}
}
