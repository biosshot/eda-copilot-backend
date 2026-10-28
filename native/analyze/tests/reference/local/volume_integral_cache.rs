//! Internal immutable original-tetra pair checkpoints, with exact bit transport.
use super::{error,mesh::LayeredMesh,AllocationBudget,Result};
use serde::{Deserialize,Serialize};
use std::{fs::{File,OpenOptions},io::{BufRead,BufReader,BufWriter,Read,Write},path::Path};
#[derive(Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
struct Header{kind:String,p:usize,near_order:usize,node_bits:Vec<[u64;3]>,cells:Vec<[usize;4]>,diagonal_bits:Vec<u64>}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
struct Pair{i:usize,j:usize,order:usize,bits:[u64;4]}
pub(super) struct Cache{values:Vec<f64>,diagonal:Vec<f64>,output:BufWriter<File>,pub reused:usize,pub written:usize,pub planned_bytes:usize,pub max_reciprocity:f64}
fn index(i:usize,j:usize)->usize{j*(j-1)/2+i}
fn validate(row:&Pair,diagonal:&[f64])->Result<[f64;4]>{
 if row.i>=row.j||row.j>=diagonal.len()||![0,4,8,16,32,64].contains(&row.order){return Err(error("INTEGRAL_CACHE","invalid directed pair indices/order"));}
 let v=row.bits.map(f64::from_bits);let[a,b,pa,pb]=v;
 if v.iter().any(|x|!x.is_finite()||*x<=0.)||(a-pa).abs()/a>0.002||(b-pb).abs()/b>0.002||(a-b).abs()/(diagonal[row.i]*diagonal[row.j]).sqrt()>1e-9{return Err(error("INTEGRAL_CACHE","cached directed/refinement gates fail"));}Ok(v)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]
 fn immutable_pair_checkpoint_preserves_bits_and_rejects_corruption(){
  use super::super::mesh::{self,LayeredInput,Material};
  let budget=AllocationBudget{max_bytes:16*1024*1024,max_nodes:100,max_tetrahedra:100};
  let mesh=mesh::extrude(&LayeredInput{xy_m:vec![[0.,0.],[0.001,0.],[0.,0.001]],triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,0.001],materials:vec![vec![Material::Copper{source_ids:vec!["cache-cell".into()]}]]},budget).unwrap();
  let dir=std::env::temp_dir().join(format!("p7-original-pair-cache-{}",std::process::id()));std::fs::create_dir(&dir).unwrap();let first=dir.join("first.jsonl");let second=dir.join("second.jsonl");let refused=dir.join("refused.jsonl");let diag=vec![1e-24;3];
  let mut c=Cache::new(&mesh,3,64,&diag,budget,1024,&[]as &[String],&first,||Ok(())).unwrap();assert!(c.get(0,1).is_none());let a=0.3e-24;let b=a*(1.+1e-12);let v=c.put(2,0,16,b,a,b*1.0001,a*1.0001).unwrap();assert_eq!(v.to_bits(),a.to_bits());assert!(c.put(0,1,16,a,2.*a,a,2.*a).is_err());drop(c);
  let c=Cache::new(&mesh,3,64,&diag,budget,1024,&[&first],&second,||Ok(())).unwrap();assert_eq!(c.get(2,0).unwrap().to_bits(),a.to_bits());assert_eq!(c.reused,1);drop(c);
  assert!(Cache::new(&mesh,3,32,&diag,budget,1024,&[&first],&refused,||Ok(())).err().unwrap().contains("identity"));
  let mut changed=mesh.clone();changed.nodes_m[0][0]=f64::from_bits(changed.nodes_m[0][0].to_bits()+1);assert!(Cache::new(&changed,3,64,&diag,budget,1024,&[&first],&refused,||Ok(())).err().unwrap().contains("identity"));
  assert!(Cache::new(&mesh,3,64,&diag,budget,1024,&[&first,&first],&refused,||Ok(())).err().unwrap().contains("duplicate"));
  let low=AllocationBudget{max_bytes:1,..budget};assert!(Cache::new(&mesh,3,64,&diag,low,1024,&[]as &[String],&refused,||Ok(())).err().unwrap().contains("MEMORY_LIMIT"));
  std::fs::write(&refused,format!("{}\n{{}}",std::fs::read_to_string(&first).unwrap().lines().next().unwrap())).unwrap();assert!(Cache::new(&mesh,3,64,&diag,budget,1024,&[&refused],dir.join("unused"),||Ok(())).err().unwrap().contains("incomplete"));
  for p in [&first,&second,&refused]{std::fs::remove_file(p).unwrap();}std::fs::remove_dir(&dir).unwrap();
 }
}
fn line(reader:&mut BufReader<File>,limit:usize)->Result<Option<String>>{
 let mut text=String::new();let count=Read::take(&mut *reader,limit as u64+1).read_line(&mut text).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;
 if count==0{return Ok(None);}if count>limit||!text.ends_with('\n'){return Err(error("INTEGRAL_CACHE","oversized or incomplete checkpoint line"));}Ok(Some(text))
}
impl Cache{
 pub fn new(mesh:&LayeredMesh,p:usize,near_order:usize,diagonal:&[f64],budget:AllocationBudget,base:usize,inputs:&[impl AsRef<Path>],output:impl AsRef<Path>,mut check:impl FnMut()->Result<()>)->Result<Self>{
  let n=diagonal.len();let pairs=n.checked_mul(n.saturating_sub(1)).and_then(|v|v.checked_div(2)).ok_or_else(||error("MEMORY_LIMIT","near cache pair count overflow"))?;
  let header_limit=mesh.nodes_m.len().checked_mul(96).and_then(|v|v.checked_add(n.checked_mul(128)?)).and_then(|v|v.checked_add(65536)).ok_or_else(||error("MEMORY_LIMIT","near cache header overflow"))?;
  let plan=base.checked_add(pairs.checked_mul(8).ok_or_else(||error("MEMORY_LIMIT","near cache table overflow"))?).and_then(|v|v.checked_add(mesh.nodes_m.len().checked_mul(256)?)).and_then(|v|v.checked_add(n.checked_mul(256)?)).and_then(|v|v.checked_add(1048576)).ok_or_else(||error("MEMORY_LIMIT","near cache live plan overflow"))?;budget.check(plan)?;
  let expected=Header{kind:"surface-positive-volume-original-pairs-v1".into(),p,near_order,node_bits:mesh.nodes_m.iter().map(|p|p.map(f64::to_bits)).collect(),cells:mesh.copper_tetrahedra.iter().map(|t|t.nodes).collect(),diagonal_bits:diagonal.iter().map(|v|v.to_bits()).collect()};
  let mut values=vec![f64::NAN;pairs];let mut reused=0;let mut max_reciprocity=0_f64;
  for path in inputs{
   check()?;
   if path.as_ref().parent().is_some_and(|p|p.join("CACHE-REJECTED.json").exists()){return Err(error("INTEGRAL_CACHE","checkpoint explicitly rejected; no load"));}
   let mut reader=BufReader::new(File::open(path).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?);
   let text=line(&mut reader,header_limit)?.ok_or_else(||error("INTEGRAL_CACHE","missing exact header"))?;
   let header:Header=serde_json::from_str(&text).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;if header!=expected{return Err(error("INTEGRAL_CACHE","geometry/self/order/method identity mismatch"));}
   while let Some(text)=line(&mut reader,4096)?{
    if reused%1024==0{check()?;}
    let row:Pair=serde_json::from_str(&text).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;let v=validate(&row,diagonal)?;max_reciprocity=max_reciprocity.max((v[0]-v[1]).abs()/(diagonal[row.i]*diagonal[row.j]).sqrt());let at=index(row.i,row.j);if values[at].is_finite(){return Err(error("INTEGRAL_CACHE","duplicate original pair"));}values[at]=v[0];reused+=1;
   }
  }
  let mut writer=BufWriter::new(OpenOptions::new().create_new(true).write(true).open(output).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?);
  serde_json::to_writer(&mut writer,&expected).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;writer.write_all(b"\n").and_then(|_|writer.flush()).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;
  Ok(Self{values,diagonal:diagonal.to_vec(),output:writer,reused,written:0,planned_bytes:plan,max_reciprocity})
 }
 pub fn get(&self,i:usize,j:usize)->Option<f64>{let(a,b)=if i<j{(i,j)}else{(j,i)};let value=self.values[index(a,b)];if value.is_finite(){Some(value)}else{None}}
 pub fn put(&mut self,i:usize,j:usize,order:usize,forward:f64,reverse:f64,previous_forward:f64,previous_reverse:f64)->Result<f64>{
  let(i,j,v)=if i<j{(i,j,[forward,reverse,previous_forward,previous_reverse])}else{(j,i,[reverse,forward,previous_reverse,previous_forward])};let row=Pair{i,j,order,bits:v.map(f64::to_bits)};validate(&row,&self.diagonal)?;
  let at=index(i,j);if self.values[at].is_finite(){return Err(error("INTEGRAL_CACHE","duplicate write"));}
  serde_json::to_writer(&mut self.output,&row).map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;self.output.write_all(b"\n").map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;self.written+=1;self.values[at]=v[0];
  // Flush a whole admitted row before any later pair can fail or time out.
  self.output.flush().map_err(|e|error("INTEGRAL_CACHE",&e.to_string()))?;Ok(v[0])
 }
}
