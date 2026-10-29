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
impl Journal {
 pub fn new(directory:&Path,identity:String)->Result<Self>{std::fs::create_dir_all(directory).map_err(|e|e.to_string())?;Ok(Self{directory:directory.to_owned(),identity})}
 pub fn load<T:DeserializeOwned>(&self,name:&str)->Result<Option<T>> {
  let path=self.directory.join(name);if !path.exists(){return Ok(None);}
  let size=std::fs::metadata(&path).map_err(|e|e.to_string())?.len();if size>64*1024*1024{return Err("developer checkpoint exceeds size cap".into());}
  let envelope:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
  if envelope["schema"]!=1||envelope["identity"]!=self.identity{return Err("developer checkpoint identity/schema mismatch".into());}
  let payload=envelope["payload"].as_str().ok_or("developer checkpoint payload")?;
  if envelope["checksum"]!=checksum(payload.as_bytes()){return Err("developer checkpoint checksum mismatch".into());}
  Ok(Some(serde_json::from_str(payload).map_err(|e|e.to_string())?))
 }
 pub fn save<T:Serialize>(&self,name:&str,value:&T)->Result<()> {
  let payload=serde_json::to_string(value).map_err(|e|e.to_string())?;
  if payload.len()>64*1024*1024{return Err("developer checkpoint exceeds size cap".into());}
  let envelope=serde_json::json!({"schema":1,"identity":self.identity,"checksum":checksum(payload.as_bytes()),"payload":payload});
  let path=self.directory.join(name);let temp=path.with_extension(format!("{}.tmp",std::process::id()));
  std::fs::write(&temp,serde_json::to_vec(&envelope).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
  std::fs::rename(temp,path).map_err(|e|e.to_string())
 }
}
