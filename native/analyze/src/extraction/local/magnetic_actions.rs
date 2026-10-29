//! Bounded reuse of already validated actions of one immutable operator/space.
//! Exact bits only: no approximation, rounding, factor dump or changed guard.
use super::{error,Result,volume_current::Space,volume_magnetic::MagneticApply};
use std::{cell::{Cell,RefCell},collections::VecDeque};
pub struct Reuse<'a> {operator:&'a dyn MagneticApply,space:&'a Space,entries:RefCell<VecDeque<(Vec<u64>,Vec<f64>)>>,bytes:Cell<usize>,max_bytes:usize,planned:usize,hits:Cell<usize>,misses:Cell<usize>}
impl<'a> Reuse<'a> {
 pub fn reservation(space:&Space,max_bytes:usize)->Result<usize>{space.current_count().checked_mul(24).and_then(|n|n.checked_add(128)).and_then(|n|n.checked_add(max_bytes)).ok_or_else(||error("MEMORY_LIMIT","action scratch reservation overflow"))}
 pub fn new(operator:&'a dyn MagneticApply,space:&'a Space,max_bytes:usize)->Result<Self>{let planned=operator.planned_bytes().checked_add(Self::reservation(space,max_bytes)?).ok_or_else(||error("MEMORY_LIMIT","action reuse plan overflow"))?;Ok(Self{operator,space,entries:RefCell::new(VecDeque::new()),bytes:Cell::new(0),max_bytes,planned,hits:Cell::new(0),misses:Cell::new(0)})}
 pub fn stats(&self)->serde_json::Value {serde_json::json!({"hits":self.hits.get(),"misses":self.misses.get(),"retainedBytes":self.bytes.get(),"maxBytes":self.max_bytes,"reservedBytes":self.planned-self.operator.planned_bytes(),"identity":"same immutable operator and Space; complete current-coordinate bits"})}
}
impl MagneticApply for Reuse<'_> {
 fn planned_bytes(&self)->usize{self.planned}
 fn apply(&self,space:&Space,current:&[f64])->Result<Vec<f64>> {
  if !std::ptr::eq(space,self.space){return Err(error("MAGNETIC_CACHE_SCOPE","immutable Space identity mismatch"));}
  if current.len()!=space.current_count()||current.iter().any(|v|!v.is_finite()){return Err(error("MAGNETIC_INPUT","finite full current vector required"));}
  let key=current.iter().map(|v|v.to_bits()).collect::<Vec<_>>();
  let mut entries=self.entries.borrow_mut();
  if let Some(index)=entries.iter().position(|(k,_)|*k==key){let entry=entries.remove(index).unwrap();let result=entry.1.clone();entries.push_back(entry);self.hits.set(self.hits.get()+1);return Ok(result);}
  drop(entries);self.misses.set(self.misses.get()+1);
  let result=self.operator.apply(space,current)?;
  if result.len()!=current.len()||result.iter().any(|v|!v.is_finite()){return Err(error("MAGNETIC_RANGE","invalid operator action cannot be cached"));}
  let bytes=key.len().checked_mul(16).and_then(|n|n.checked_add(128)).ok_or_else(||error("MEMORY_LIMIT","action entry overflow"))?;
  if bytes<=self.max_bytes {let mut entries=self.entries.borrow_mut();while self.bytes.get()+bytes>self.max_bytes||entries.len()>=512 {if let Some((key,value))=entries.pop_front(){self.bytes.set(self.bytes.get()-(key.len()*8+value.len()*8+128));}else{break;}}
   entries.push_back((key,result.clone()));self.bytes.set(self.bytes.get()+bytes);
  }Ok(result)
 }
}
