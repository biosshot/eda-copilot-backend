//! Immutable reference-only positive-cubature work; never an accepted pair.
use super::*;
use serde::{Deserialize,Serialize};
#[derive(Serialize,Deserialize)]
enum Node { Leaf { low:[u64;2],high:[u64;2],order:usize }, Branch { a:usize,b:usize,left:usize,right:usize } }
#[derive(Serialize,Deserialize)]
struct State { kind:String,executable:String,source: [[[u64;2];3];4],target:[[[u64;2];3];4],tolerance:u64,nodes:Vec<Node> }
struct Leaf { node:usize,vertices:[V;4],volume:W,value:W,error:f64,order:usize }
fn bits(x:W)->[u64;2]{[x.0.hi.to_bits(),x.0.lo.to_bits()]}
fn vertices(v:[V;4])->[[[u64;2];3];4]{v.map(|p|p.map(bits))}
fn number(b:[u64;2])->Result<W>{let hi=f64::from_bits(b[0]);let lo=f64::from_bits(b[1]);if !hi.is_finite()||!lo.is_finite()||!(hi>0.)||lo.abs()>f64::EPSILON*hi.abs(){return Err(error("STATE","finite positive normalized leaf sum required"));}Ok(W(accurate::Sum{hi,lo}))}
fn longest(v:[V;4])->(usize,usize){let edge=|i:usize,j:usize|dot(sub(v[i],v[j]),sub(v[i],v[j])).value();(0..4).flat_map(|a|(a+1..4).map(move|b|(a,b))).max_by(|&(a,b),&(c,d)|edge(a,b).total_cmp(&edge(c,d))).unwrap()}
fn children(v:[V;4],a:usize,b:usize)->([V;4],[V;4]){let m=std::array::from_fn(|k|(v[a][k]+v[b][k]).scale(0.5));let(mut l,mut r)=(v,v);l[a]=m;r[b]=m;(l,r)}
fn restore(state:&State,index:usize,v:[V;4],volume:W,seen:&mut[bool],leaves:&mut Vec<Leaf>,depth:usize)->Result<()>{
 if depth>128||index>=seen.len()||seen[index]{return Err(error("STATE","invalid tree depth, child or repeated parent"));}seen[index]=true;
 let exact=dot(sub(v[1],v[0]),cross(sub(v[2],v[0]),sub(v[3],v[0]))).abs()/W::f(6.);if ((exact-volume)/volume).value().abs()>1e-12{return Err(error("STATE","checkpoint geometric partition fails"));}
 match state.nodes[index]{Node::Leaf{low,high,order}=>{if ![16,32,64].contains(&order){return Err(error("STATE","leaf order16/32/64 required"));}let low=number(low)?;let value=number(high)?;leaves.push(Leaf{node:index,vertices:v,volume,value,error:(value-low).value().abs(),order});},Node::Branch{a,b,left,right}=>{if (a,b)!=longest(v)||left<=index||right<=index||left==right{return Err(error("STATE","noncanonical midpoint split"));}let(l,r)=children(v,a,b);restore(state,left,l,volume.scale(0.5),seen,leaves,depth+1)?;restore(state,right,r,volume.scale(0.5),seen,leaves,depth+1)?;}}
 Ok(())
}
fn save(path:&str,state:&State)->Result<()>{use std::io::Write;let data=serde_json::to_vec(state).map_err(|e|e.to_string())?;let temporary=format!("{path}.tmp");let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e|e.to_string())?;f.write_all(&data).map_err(|e|e.to_string())?;f.sync_all().map_err(|e|e.to_string())?;if std::path::Path::new(path).exists(){return Err(error("STATE","immutable output already exists"));}std::fs::rename(temporary,path).map_err(|e|e.to_string())}
impl Source {
 pub fn adaptive_checkpoint(&self,target:&Self,tolerance:f64,direction:&str,mut check:impl FnMut()->Result<()>)->Result<(f64,usize,f64)>{
  let output=std::env::var("P7_REFERENCE_STATE_OUTPUT").map_err(|_|error("STATE","output required"))?;let output=format!("{output}-{direction}.json");
  let executable=std::env::var("P7_REFERENCE_EXECUTABLE_SHA256").map_err(|_|error("STATE","executable hash required"))?;
  let mut state=State{kind:"positive-original-tetra-tree-v0".into(),executable,source:vertices(self.vertices),target:vertices(target.vertices),tolerance:tolerance.to_bits(),nodes:vec![]};
  if let Ok(prefix)=std::env::var("P7_REFERENCE_STATE_INPUT"){let path=format!("{prefix}-{direction}.json");if std::path::Path::new(&path).exists(){let old:State=serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;if old.kind!=state.kind||old.executable!=state.executable||old.source!=state.source||old.target!=state.target||old.tolerance!=state.tolerance||old.nodes.is_empty()||old.nodes.len()>8191{return Err(error("STATE","checkpoint identity or size mismatch"));}state=old;}}
  let mut leaves=vec![];
  if !state.nodes.is_empty(){let mut seen=vec![false;state.nodes.len()];restore(&state,0,self.contact_order(target),target.volume,&mut seen,&mut leaves,0)?;if seen.iter().any(|x|!*x)||leaves.len()>4096{return Err(error("STATE","unreachable node or leaf cap"));}let volume=leaves.iter().fold(W::ZERO,|s,l|s+l.volume);if ((volume-target.volume)/target.volume).value().abs()>1e-28{return Err(error("STATE","full original target coverage required"));}eprintln!("reference restored {direction}: {} leaves",leaves.len());}
  leaves.sort_by_key(|l|l.node);
  let evaluate=|v:[V;4],volume:W,node:usize,check:&mut dyn FnMut()->Result<()>|->Result<(Leaf,Node)>{check()?;let exact=dot(sub(v[1],v[0]),cross(sub(v[2],v[0]),sub(v[3],v[0]))).abs()/W::f(6.);if ((exact-volume)/volume).value().abs()>1e-12{return Err(error("PARTITION","adaptive original volume required"));}let low=self.integrate_target(v,volume,8,false,&mut *check)?;let high=self.integrate_target(v,volume,16,false,&mut *check)?;Ok((Leaf{node,vertices:v,volume,value:high,error:(high-low).value().abs(),order:16},Node::Leaf{low:bits(low),high:bits(high),order:16}))};
  if leaves.is_empty(){let(l,n)=evaluate(self.contact_order(target),target.volume,0,&mut check)?;leaves.push(l);state.nodes.push(n);}
  let result=(||loop{let value=leaves.iter().fold(W::ZERO,|s,l|s+l.value).value();let estimate=leaves.iter().map(|l|l.error).sum::<f64>();if estimate<=tolerance{return Ok((value,leaves.len(),estimate));}if leaves.len().is_power_of_two(){eprintln!("reference checkpoint adaptive {direction} leaves{}: estimated/tolerance={:e}",leaves.len(),estimate/tolerance);}if leaves.len()>=4096{
    let Some(at)=(0..leaves.len()).filter(|&i|leaves[i].order<64).max_by(|&a,&b|leaves[a].error.total_cmp(&leaves[b].error))else{return Err(error("QUADRATURE",&format!("adaptive4096-leaf/order64 estimate exhausted: error={estimate:e}, tolerance={tolerance:e}")));};
    let old=&leaves[at];let order=old.order*2;let high=self.integrate_target(old.vertices,old.volume,order,false,&mut check)?;let error=(high-old.value).value().abs();
    state.nodes[old.node]=Node::Leaf{low:bits(old.value),high:bits(high),order};leaves[at].value=high;leaves[at].error=error;leaves[at].order=order;continue;
   }
   let at=(0..leaves.len()).max_by(|&a,&b|leaves[a].error.total_cmp(&leaves[b].error)).unwrap();let old=&leaves[at];let(a,b)=longest(old.vertices);let(l,r)=children(old.vertices,a,b);let next=state.nodes.len();let(left,ln)=evaluate(l,old.volume.scale(0.5),next,&mut check)?;let(right,rn)=evaluate(r,old.volume.scale(0.5),next+1,&mut check)?;
   // Both children must succeed before replacing the fully covering parent.
   state.nodes[old.node]=Node::Branch{a,b,left:next,right:next+1};state.nodes.push(ln);state.nodes.push(rn);leaves.swap_remove(at);leaves.push(left);leaves.push(right);leaves.sort_by_key(|l|l.node);
  })();save(&output,&state)?;eprintln!("reference checkpoint saved {direction}: {} leaves, {}",leaves.len(),output);result
 }
}
pub fn controls(prefix:&str)->Result<serde_json::Value>{
 let t=[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];let source=Source::new(t)?;let target=Source::new(t.map(|p|[p[0]+0.1,p[1]+0.2,p[2]+0.3]))?;
 // Force interruption after the root but during a child: the retained tree
 // must still cover the complete original target.
 let tolerance=1e-8;std::env::set_var("P7_REFERENCE_STATE_OUTPUT",format!("{prefix}-partial"));std::env::remove_var("P7_REFERENCE_STATE_INPUT");let mut calls=0;
 let refused=source.adaptive_checkpoint(&target,tolerance,"control",||{calls+=1;if calls>160{Err(error("DEADLINE","forced reference checkpoint control"))}else{Ok(())}});
 if refused.is_ok(){return Err(error("CONTROL","forced interruption was not exercised"));}
 let path=format!("{prefix}-partial-control.json");let state:State=serde_json::from_slice(&std::fs::read(&path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let mut seen=vec![false;state.nodes.len()];let mut leaves=vec![];restore(&state,0,source.contact_order(&target),target.volume,&mut seen,&mut leaves,0)?;
 if seen.iter().any(|x|!*x)||leaves.len()!=1{return Err(error("CONTROL","interruption did not retain fully covering parent"));}
 std::env::set_var("P7_REFERENCE_STATE_INPUT",format!("{prefix}-partial"));std::env::set_var("P7_REFERENCE_STATE_OUTPUT",format!("{prefix}-resumed"));let resumed=source.adaptive_checkpoint(&target,tolerance,"control",||Ok(()))?;
 std::env::remove_var("P7_REFERENCE_STATE_INPUT");std::env::set_var("P7_REFERENCE_STATE_OUTPUT",format!("{prefix}-cold"));let cold=source.adaptive_checkpoint(&target,tolerance,"control",||Ok(()))?;
 if resumed.0.to_bits()!=cold.0.to_bits()||resumed.1!=cold.1||resumed.2.to_bits()!=cold.2.to_bits(){return Err(error("CONTROL","cold/resumed cubature differs"));}
 // Reject malformed trees before using any saved quadrature value.
 let bad=State{nodes:vec![Node::Branch{a:0,b:1,left:1,right:1},Node::Leaf{low:bits(W::ONE),high:bits(W::ONE),order:16}],..state};let mut seen=vec![false;2];let mut leaves=vec![];
 if restore(&bad,0,source.contact_order(&target),target.volume,&mut seen,&mut leaves,0).is_ok(){return Err(error("CONTROL","duplicate child accepted"));}
 std::env::set_var("P7_REFERENCE_STATE_INPUT",format!("{prefix}-partial"));std::env::set_var("P7_REFERENCE_STATE_OUTPUT",format!("{prefix}-wrong-tolerance"));
 if source.adaptive_checkpoint(&target,tolerance*2.,"control",||Ok(())).is_ok(){return Err(error("CONTROL","wrong tolerance identity accepted"));}
 Ok(serde_json::json!({"method":"immutable original-support reference cubature checkpoints","forcedPartialLeaves":1,"coldResumedBitwise":true,"rejectedDuplicateChild":true,"rejectedWrongTolerance":true,"leaves":cold.1,"estimatedAbsoluteError":cold.2,"physicalAdmission":false}))
}
