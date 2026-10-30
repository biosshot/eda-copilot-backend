//! Restorable P6 model execution and physical current/Joule recovery. No FEM.
use super::Result;
use crate::electrical::{coupled,Counters,Budget};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Request {pub prepared:serde_json::Value,pub frequencies_hz:Vec<f64>,#[serde(default)]pub drive:Vec<f64>,#[serde(default)]pub loads_ohm:Vec<Option<f64>>}
#[derive(Default)]
pub struct Session {compiled:Option<(String,coupled::System)>}
impl Session {
pub fn clear(&mut self){self.compiled=None;}
pub fn solve(&mut self,r:Request)->Result<serde_json::Value> {
 self.solve_with_budget(r,30.)
}
pub fn solve_with_budget(&mut self,r:Request,seconds:f64)->Result<serde_json::Value> {self.solve_with_options(r,seconds,false)}
pub fn solve_with_options(&mut self,r:Request,seconds:f64,observables_only:bool)->Result<serde_json::Value> {
 if !seconds.is_finite()||seconds<=0.||seconds>super::board_prepare::verification_limit("P7_VERIFY_SECONDS",300,1,3600)? as f64{return Err("LOCAL_DEADLINE: invalid remaining response budget".into());}
 let seconds=seconds.min(30.);let started=std::time::Instant::now();
 let get=|v:&serde_json::Value|serde_json::from_value::<Vec<Vec<f64>>>(v.clone()).map_err(|e|e.to_string());
 let model:coupled::Model=serde_json::from_value(r.prepared["candidate"]["model"].clone()).map_err(|e|e.to_string())?;
 let basis=get(&r.prepared["candidate"]["currentBasis"])?;
 let resistances:Vec<f64>=serde_json::from_value(r.prepared["cellResistanceOhm"].clone()).map_err(|e|e.to_string())?;
 let ports=r.prepared["request"]["ports"].as_array().ok_or("missing physical ports")?.len();
 let ni=model.current_dofs.len();let nv=model.potential_nodes.len();
 if ports==0||ports>16||basis.len()!=ni||resistances.is_empty()||resistances.len()>262144||resistances.iter().any(|v|!v.is_finite()||*v<=0.)||basis.iter().any(|v|v.len()!=3*resistances.len()||v.iter().any(|x|!x.is_finite())) {return Err("LOCAL_RECOVERY: invalid physical current recovery".into());}
 if r.frequencies_hz.len()>8192||r.frequencies_hz.iter().any(|f|!f.is_finite()||(*f!=0.&&*f<1000.)||*f>100000.) {return Err("LOCAL_BAND: supported AC extraction band is 1kHz..100kHz; DC requires charge-compatible excitation; rebuild/validation required outside band".into());}
 let output_plan=if observables_only{resistances.len().checked_mul(64).and_then(|n|n.checked_add(r.frequencies_hz.len().checked_mul(ports)?.checked_mul(64)?))}else{resistances.len().checked_mul(448).and_then(|n|n.checked_mul(r.frequencies_hz.len()))}.ok_or("LOCAL_OUTPUT_BUDGET: recovery output plan overflow")?;
 if output_plan>64*1024*1024{return Err("LOCAL_OUTPUT_BUDGET: recovered samples exceed bounded output plan; request fewer frequencies".into());}
 let drive=if r.drive.is_empty(){let mut d=vec![0.;ports];d[0]=1.;d}else{r.drive};
 if drive.len()!=ports||drive.iter().any(|v|!v.is_finite())||(!r.loads_ohm.is_empty()&&r.loads_ohm.len()!=ports){return Err("LOCAL_DRIVE: complete finite port drive/load mapping required".into());}
 for load in r.loads_ohm.iter().flatten(){if !load.is_finite()||*load<=0.{return Err("LOCAL_LOAD: positive resistance required".into());}}
 let budget=Budget{max_memory_mi_b:Some(512.),max_seconds:Some(seconds)};
 let key=serde_json::to_string(&(ports,&model,&r.loads_ohm)).map_err(|e|e.to_string())?;
 let compiled=self.compiled.as_ref().is_none_or(|entry|entry.0!=key);
 if compiled{let mut system=coupled::System::compile(&model,ports,Some(&budget))?;for (port,load)in r.loads_ohm.iter().enumerate(){if let Some(load)=load{system.add_port_admittance(port,1./load,0.)?;}}self.compiled=Some((key,system));}
 let system=&mut self.compiled.as_mut().unwrap().1;
 let n=nv+model.current_dofs.len();let mut stats=Counters::default();let mut samples=vec![];
 for &frequency in &r.frequencies_hz {
  if started.elapsed().as_secs_f64()>seconds{return Err("LOCAL_DEADLINE: reduced response deadline".into());}
  let mut rhs=system.port_rhs(&drive);rhs.resize(if frequency==0.{n}else{2*n},0.);
  let x=system.solve(2.*std::f64::consts::PI*frequency,rhs,&vec![0.;n],&mut stats)?;
  let currents=(0..3*resistances.len()).map(|j|[(0..ni).map(|k|basis[k][j]*x[nv+k]).sum::<f64>(),if frequency==0.{0.}else{(0..ni).map(|k|basis[k][j]*x[n+nv+k]).sum::<f64>()}]).collect::<Vec<_>>();
  let losses=resistances.iter().enumerate().map(|(i,r)|r*(0..3).map(|k|currents[3*i+k][0].powi(2)+currents[3*i+k][1].powi(2)).sum::<f64>()).collect::<Vec<_>>();
  let total_loss=losses.iter().sum::<f64>();let mut sample=serde_json::json!({"frequencyHz":frequency,"portRealV":system.port_voltages(&x,ports),"portImaginaryV":if frequency==0.{vec![0.;ports]}else{system.port_voltages(&x[n..],ports)},"totalLossW":total_loss});
  if !observables_only{sample["currentRealImaginaryA"]=serde_json::json!(currents);sample["cellLossW"]=serde_json::json!(losses);}samples.push(sample);
 }
 if started.elapsed().as_secs_f64()>seconds{return Err("LOCAL_DEADLINE: reduced response exhausted remaining shared budget".into());}
 Ok(serde_json::json!({"status":"complete","phasorConvention":"RMS; sum R*(real(I)^2+imag(I)^2) is mean Joule W","dcScope":"Only charge-compatible DC sources; original physical charge constraints remain active","bandHz":[1000.,100000.],"samples":samples,"model":model,"loadsOhm":r.loads_ohm,"modelScope":"Intrinsic extracted model; external loads belong to request state","stageCounts":{"assembly":0,"training":0,"p6Solves":r.frequencies_hz.len(),"p6Compiled":compiled,"numericalFactorizations":stats.numerical_factorizations,"reusedFactors":stats.reused_factors},"validity":{"status":"conditional","reason":"Accepted electric v1; finite local ROI with insulated cuts. Complete full/reduced and broader board physical validation remain separate."}}))
}

}
pub fn solve(r:Request)->Result<serde_json::Value>{Session::default().solve(r)}
