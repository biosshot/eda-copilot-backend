//! Developer-only atomic progress journal. Not production cache or factor dump.
use super::Result;
use serde::{Serialize,de::DeserializeOwned};
use std::{path::{Path,PathBuf},io::Read};
pub fn checksum(bytes:&[u8])->String {let mut h=0xcbf29ce484222325u64;for &b in bytes{h^=b as u64;h=h.wrapping_mul(0x100000001b3);}format!("{h:016x}")}
pub fn identity(board:&str,request:&serde_json::Value)->Result<String>{
 let mut file=std::fs::File::open(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;let mut bytes=[0u8;65536];let mut h=0xcbf29ce484222325u64;
 loop{let n=file.read(&mut bytes).map_err(|e|e.to_string())?;if n==0{break;}for &b in &bytes[..n]{h^=b as u64;h=h.wrapping_mul(0x100000001b3);}}
 Ok(checksum(format!("v1:{h:016x}:{}:{}",checksum(board.as_bytes()),request).as_bytes()))
}
pub struct Journal {pub directory:PathBuf,pub identity:String}
// serde_json's default decimal reader is not an exact f64 round trip. Keep
// checkpoint numbers as explicit IEEE bits, without changing production parsing.
fn encode(value:&mut serde_json::Value){match value{
 serde_json::Value::Number(n) if n.is_f64()=>{*value=serde_json::json!({"@p7-f64":format!("{:016x}",n.as_f64().unwrap().to_bits())});},
 serde_json::Value::Array(a)=>for v in a{encode(v)},serde_json::Value::Object(o)=>for v in o.values_mut(){encode(v)},_=>{}
}}
fn decode(value:&mut serde_json::Value)->Result<()>{
 if let Some(o)=value.as_object(){if o.len()==1&&o.contains_key("@p7-f64"){
  let text=o["@p7-f64"].as_str().filter(|s|s.len()==16).ok_or("developer float bits")?;
  let number=f64::from_bits(u64::from_str_radix(text,16).map_err(|_|"developer float bits")?);
  *value=serde_json::Value::Number(serde_json::Number::from_f64(number).ok_or("developer nonfinite float")?);return Ok(());
 }}
 match value{serde_json::Value::Array(a)=>for v in a{decode(v)?},serde_json::Value::Object(o)=>for v in o.values_mut(){decode(v)?},_=>{}}Ok(())
}
impl Journal {
 pub fn new(directory:&Path,identity:String)->Result<Self>{std::fs::create_dir_all(directory).map_err(|e|e.to_string())?;Ok(Self{directory:directory.to_owned(),identity})}
 pub fn load<T:DeserializeOwned>(&self,name:&str)->Result<Option<T>> {
  let path=self.directory.join(name);if !path.exists(){return Ok(None);}
  let size=std::fs::metadata(&path).map_err(|e|e.to_string())?.len();if size>64*1024*1024{return Err("developer checkpoint exceeds size cap".into());}
  let envelope:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
  if envelope["schema"]!=2||envelope["identity"]!=self.identity{return Err("developer checkpoint identity/schema mismatch".into());}
  let payload=envelope["payload"].as_str().ok_or("developer checkpoint payload")?;
  if envelope["checksum"]!=checksum(payload.as_bytes()){return Err("developer checkpoint checksum mismatch".into());}
  let mut value=serde_json::from_str(payload).map_err(|e|e.to_string())?;decode(&mut value)?;
  Ok(Some(serde_json::from_value(value).map_err(|e|e.to_string())?))
 }
 pub fn save<T:Serialize>(&self,name:&str,value:&T)->Result<()> {
  let mut value=serde_json::to_value(value).map_err(|e|e.to_string())?;encode(&mut value);
  let payload=serde_json::to_string(&value).map_err(|e|e.to_string())?;
  if payload.len()>64*1024*1024{return Err("developer checkpoint exceeds size cap".into());}
  let envelope=serde_json::json!({"schema":2,"identity":self.identity,"checksum":checksum(payload.as_bytes()),"payload":payload});
  let path=self.directory.join(name);let temp=path.with_extension(format!("{}.tmp",std::process::id()));
  std::fs::write(&temp,serde_json::to_vec(&envelope).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
  std::fs::rename(temp,path).map_err(|e|e.to_string())
 }
}
