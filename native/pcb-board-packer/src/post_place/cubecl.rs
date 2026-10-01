//! One resident evaluator shared by local geometric and final route-aware refinement.
use super::{Candidate,RefineProblem,World,Target};
use crate::compute::{gpu,Error,ErrorKind,Requirements,ScratchKey};
use cubecl::{prelude::*,server::Handle,wgpu::WgpuRuntime};
use std::time::{Instant};
use rustc_hash::FxHashMap;
#[path="gpu_kernels.rs"] mod k;
const REQUIREMENTS:Requirements=Requirements{f64:true,u64:true};
const MAX_BYTES:usize=64*1024*1024;
fn invalid(s:impl Into<String>)->Error {Error::new(ErrorKind::InvalidInput,s)}
fn layer(s:&str)->Result<u32,Error>{match s{"top"=>Ok(1),"bottom"=>Ok(2),_=>Err(invalid("unsupported post-place layer"))}}
struct Data {si:Vec<u32>,sf:Vec<f64>}
impl Data {
 fn floats(&mut self,v:&[f64])->u32 {let o=self.sf.len() as u32;self.sf.extend(v);o}
 fn ints(&mut self,v:&[u32])->u32 {let o=self.si.len() as u32;self.si.extend(v);o}
 fn target(&mut self,t:&Target)->Option<u32>{
  let record=match t.kind.as_str(){
   "component"=>[1,t.component? as u32,0],"pin"=>[2,t.component? as u32,t.pad? as u32],
   "point"=>{let p=t.point?;[3,self.floats(&[p.x,p.y]),0]},
   "group"=>{let v=t.members.as_ref()?;if v.is_empty(){return None;}[4,self.ints(&v.iter().map(|&i|i as u32).collect::<Vec<_>>()),v.len() as u32]},_=>return None,
  };Some(self.ints(&record))
 }
}
pub(super) struct Engine {
 data:Data,orientations:Vec<Vec<u32>>,handles:Option<(Handle,Handle)>,
 pub failure:Option<Error>,pub batches:usize,pub candidates:usize,pub milliseconds:f64,pub encode_ms:f64,
}
impl Engine {
 pub fn new(p:&RefineProblem)->Result<Self,Error>{
  let mut d=Data{si:vec![0;18],sf:vec![]};let n=p.components.len();
  if p.nets.iter().map(|n|n.name.as_ref()).collect::<std::collections::HashSet<_>>().len()!=p.nets.len() {return Err(invalid("duplicate post-place net names"));}
  if n==0 || n>4096 {return Err(invalid("post-place component capacity"));}
  let mut components=Vec::new();let mut orientations=Vec::new();let mut geometry=0;
  for c in &p.components {
   let pads=d.ints(&c.pads.iter().flat_map(|pad|[0,0,0,if pad.through{1}else if pad.opposite{2}else{0}]).collect::<Vec<_>>());
   components.extend([geometry as u32,c.pads.len() as u32,pads,u32::from(c.internal_pad_owner)]);geometry+=7+c.pads.len()*7;
   let mut offsets=Vec::new();for o in &c.orientations {layer(&o.layer)?;let mut f=vec![o.box_.left,o.box_.right,o.box_.top,o.box_.bottom];
    for (point,b) in o.points.iter().zip(&o.pad_boxes){f.extend([point.x,point.y,b.left,b.right,b.top,b.bottom]);}offsets.push(d.floats(&f));
   }orientations.push(offsets);
  }
  d.si[0]=n as u32;d.si[1]=d.ints(&components);d.si[11]=geometry as u32;
  let mut nets=Vec::new();let mut points=Vec::new();let mut segment_count=0;
  for net in &p.nets {
   if net.points.len()>512 || net.weight<0.0 {return Err(invalid("post-place net capacity/negative MST crossing weight"));}
   nets.extend([(points.len()/2) as u32,net.points.len() as u32,segment_count as u32,d.floats(&[net.weight])]);
   points.extend(net.points.iter().flat_map(|q|[q.component as u32,q.pad as u32]));segment_count+=net.points.len().saturating_sub(1);
  }
  d.si[2]=p.nets.len() as u32;d.si[3]=d.ints(&nets);d.si[6]=(points.len()/2) as u32;d.si[4]=d.ints(&points);d.si[5]=segment_count as u32;
  let mut groups:Vec<Vec<u32>>=Vec::new();let mut group_ids=FxHashMap::default();
  for (i,c) in p.components.iter().enumerate(){for (j,pad) in c.pads.iter().enumerate(){
   // Every refiner obstacle is named. Keep first-occurrence physical-pad order.
   let next=groups.len();let id=*group_ids.entry(pad.reference.clone()).or_insert_with(||{groups.push(vec![]);next});
   // CPU owner exemptions use the reference prefix, which need not equal the
   // containing component on malformed/duplicated imported geometry.
   if pad.reference.rsplit_once('.').is_some_and(|(owner,_)|owner!=c.designator.as_ref()) {return Err(invalid("post-place obstacle owner mismatch"));}
   let net=pad.net.as_ref().and_then(|name|p.nets.iter().position(|n|&n.name==name)).map_or(u32::MAX,|i|i as u32);
   groups[id].extend([i as u32,j as u32,net]);
  }}
  let mut table=Vec::new();for g in &groups {table.extend([d.ints(g),(g.len()/3) as u32]);}
  d.si[15]=groups.len() as u32;d.si[16]=d.ints(&table);d.si[12]=d.floats(&[p.pad_crossing_weight]);
  d.si[14]=d.floats(&[p.board.left,p.board.right,p.board.top,p.board.bottom]);
  let mut buckets:[Vec<u32>;4]=Default::default();
  for h in &p.hints {match h.kind.as_str(){
   "distance" if !h.all=>{if let(Some(a),Some(b))=(d.target(&h.source),d.target(&h.target)){let f=d.floats(&[h.weight,h.min.unwrap_or(0.0),h.max.unwrap_or(0.0)]);buckets[0].extend([0,a,b,f,u32::from(h.max.is_some())|u32::from(h.min.is_some())*2]);}},
   "clearance"=>{if let(Some(a),Some(minimum))=(d.target(&h.source),h.min){let f=d.floats(&[h.weight,minimum]);if h.all {
    for i in 0..n {if !(matches!(h.source.kind.as_str(),"component"|"pin")&&h.source.component==Some(i)){let b=d.ints(&[1,i as u32,0]);buckets[1].extend([1,a,b,f,0]);}}
   }else if let Some(b)=d.target(&h.target){buckets[1].extend([1,a,b,f,0]);}}},
   "same_side" if h.source.kind=="component"&&h.target.kind=="component"=>{let f=d.floats(&[h.weight*20.0]);buckets[2].extend([2,h.source.component.unwrap() as u32,h.target.component.unwrap() as u32,f,0]);},
   "prefer_layer" if h.source.kind=="component"=>{if let Some(l)=&h.layer{let f=d.floats(&[h.weight*10.0]);buckets[2].extend([2,h.source.component.unwrap() as u32,u32::MAX,f,layer(l)?]);}},
   "edge"=>{if let(Some(a),Some(edge))=(d.target(&h.source),&h.edge){let f=d.floats(&[h.weight]);buckets[3].extend([3,a,0,f,match edge.as_str(){"left"=>0,"right"=>1,"top"=>2,_=>3}]);}},_=>{}
  }}
  let terms:Vec<_>=buckets.into_iter().flatten().collect();d.si[7]=(terms.len()/5) as u32;d.si[8]=d.ints(&terms);
  let mut paths=Vec::new();for path in &p.paths {
   let mut ports:Vec<_>=path.ports.iter().filter(|port|port.target.component.is_some()).filter_map(|port|d.target(&port.target).map(|t|(port,t))).collect();ports.sort_by(|(a,_),(b,_)|a.order.cmp(&b.order).then_with(||a.reference.cmp(&b.reference)));ports.dedup_by_key(|(q,_)|q.order);
   let mut raw=Vec::new();for (port,t) in ports {raw.extend([t,port.order as u32,port.target.component.unwrap() as u32]);}
   let priority=match path.priority.as_ref(){"critical"=>1.8,"high"=>1.35,"low"=>0.65,_=>1.0};
   paths.extend([d.ints(&raw),(raw.len()/3) as u32,u32::from(path.shape.as_ref()=="straight"),d.floats(&[priority]),u32::from(path.prefer_facing_pads)]);
  }d.si[9]=p.paths.len() as u32;d.si[10]=d.ints(&paths);
  if d.sf.iter().any(|x|!x.is_finite()||x.abs()>1e7) || !p.pad_crossing_weight.is_finite() || p.pad_crossing_weight<0.0 {return Err(invalid("unsafe post-place GPU number"));}
  if geometry*8+(points.len()/2)*4+segment_count*(36+segment_count.div_ceil(32)*4)+((terms.len()/5)+(paths.len()/5))*24>MAX_BYTES || d.si.len()*4+d.sf.len()*8>MAX_BYTES || segment_count>4096 {return Err(invalid("post-place resident capacity"));}
  Ok(Self{data:d,orientations,handles:None,failure:None,batches:0,candidates:0,milliseconds:0.0,encode_ms:0.0})
 }
 pub fn fail(&mut self,error:Error)->String{let text=error.to_string();self.failure=Some(error);text}
 pub fn injected(&mut self,stage:&str)->Result<(),String>{if std::env::var("PCB_POST_PLACE_GPU_FAIL").ok().as_deref()==Some(stage){let error=gpu::with_session::<()>(REQUIREMENTS,|_|Err(Error::new(ErrorKind::RuntimeFailure,format!("injected post-place GPU failure: {stage}")))).unwrap_err();return Err(self.fail(error));}Ok(())}
 fn pose(&self,p:&RefineProblem,component:usize,pose:&crate::model::Placement,f:&mut Vec<f64>)->Result<[u32;3],Error>{
  if !pose.x.is_finite()||!pose.y.is_finite()||pose.x.abs()>1e6||pose.y.abs()>1e6{return Err(invalid("unsafe post-place pose"));}
  let orientation=p.components[component].orientations.iter().position(|o|o.rotate==crate::geometry::normalize_rotation(pose.rotate)&&o.layer==pose.layer).ok_or_else(||invalid("post-place orientation unavailable"))?;
  let off=f.len() as u32;f.extend([pose.x,pose.y]);Ok([self.orientations[component][orientation],off,layer(&pose.layer)?])
 }
 pub fn chunk_size(&self)->usize{let d=&self.data.si;let ns=d[5] as usize;let bytes=d[11] as usize*8+d[6] as usize*4+ns*(12+8+16+ns.div_ceil(32)*4)+(d[7]+d[9]) as usize*24+8;
  let dispatch=(65535*128)/(d[0].max(d[2]).max(d[5]).max(d[7]+d[9]).max(1) as usize);
  std::env::var("PCB_POST_PLACE_GPU_CHUNK_SIZE").ok().and_then(|s|s.parse::<usize>().ok()).unwrap_or(128).clamp(1,256).min((MAX_BYTES/bytes.max(1)).max(1)).min(dispatch.max(1))
 }
 pub fn scores(&mut self,p:&RefineProblem,current:&World,candidates:&[&Candidate],budget:Option<&super::CpuBudget>)->Result<Vec<f64>,String>{
  self.injected("batch")?;let permit=budget.map(|b|b.acquire());let started=Instant::now();let mut bi=vec![candidates.len() as u32];let mut bf=Vec::new();
  for i in 0..p.components.len(){let pose=self.pose(p,i,p.pose(current,i),&mut bf).map_err(|e|self.fail(e))?;bi.extend(pose);}
  let frames=bi.len();bi.resize(frames+candidates.len()*2,0);
  for (i,c) in candidates.iter().enumerate(){bi[frames+i*2]=bi.len() as u32;bi[frames+i*2+1]=c.changes.len() as u32;for (id,placement) in &c.changes{bi.push(*id as u32);bi.extend(self.pose(p,*id,placement,&mut bf).map_err(|e|self.fail(e))?);}}
  self.encode_ms+=started.elapsed().as_secs_f64()*1000.0;
  let started=Instant::now();let count=candidates.len();let d=&self.data;let ns=d.si[5] as usize;let nt=(d.si[7]+d.si[9]) as usize;
  let verify=std::env::var("PCB_POST_PLACE_GPU_VERIFY").ok().as_deref()==Some("1");
  let result=gpu::with_session(REQUIREMENTS,|session|{
   if self.handles.is_none(){self.handles=Some((session.client.create_from_slice(u32::as_bytes(&d.si)),session.client.create_from_slice(f64::as_bytes(&d.sf))));}
   let (si,sf)=self.handles.as_ref().unwrap();let bih=session.client.create_from_slice(u32::as_bytes(&bi));let bfh=session.client.create_from_slice(f64::as_bytes(&bf));
   let geometry_len=count*d.si[11] as usize;let edge_len=(count*ns*3).max(1);let length_len=(count*ns).max(1);let conn_len=(count*d.si[6] as usize).max(1);let term_len=(count*nt*3).max(1);let seg_len=(count*ns*2).max(1);let mask_len=(count*ns*ns.div_ceil(32)).max(1);
   let geo=session.workspace(ScratchKey::new("post-place-f64",0),geometry_len*8);let edges=session.workspace(ScratchKey::new("post-place-f64",1),edge_len*4);let lengths=session.workspace(ScratchKey::new("post-place-f64",2),length_len*8);let connected=session.workspace(ScratchKey::new("post-place-f64",3),conn_len*4);
   let terms=session.workspace(ScratchKey::new("post-place-f64",4),term_len*8);let segments=session.workspace(ScratchKey::new("post-place-f64",5),seg_len*8);let masks=session.workspace(ScratchKey::new("post-place-f64",6),mask_len*4);let output=session.workspace(ScratchKey::new("post-place-f64",7),count*8);
   let input=||unsafe{k::InputLaunch::new(ArrayArg::from_raw_parts(si.clone(),d.si.len()),ArrayArg::from_raw_parts(sf.clone(),d.sf.len()),ArrayArg::from_raw_parts(bih.clone(),bi.len()),ArrayArg::from_raw_parts(bfh.clone(),bf.len()),ArrayArg::from_raw_parts(geo.clone(),geometry_len),ArrayArg::from_raw_parts(edges.clone(),edge_len),ArrayArg::from_raw_parts(lengths.clone(),length_len))};
   let grid=|n:usize|CubeCount::Static(n.div_ceil(128) as u32,1,1);let dim=CubeDim::new_1d(128);
   unsafe{
    k::materialize::launch_unchecked::<WgpuRuntime>(&session.client,grid(count*d.si[0] as usize),dim,input(),ArrayArg::from_raw_parts(geo.clone(),geometry_len));
    if d.si[2]>0{k::mst::launch_unchecked::<WgpuRuntime>(&session.client,grid(count*d.si[2] as usize),dim,input(),ArrayArg::from_raw_parts(connected,conn_len),ArrayArg::from_raw_parts(edges.clone(),edge_len),ArrayArg::from_raw_parts(lengths.clone(),length_len));}
    if ns>0{k::segment_costs::launch_unchecked::<WgpuRuntime>(&session.client,grid(count*ns),dim,input(),ArrayArg::from_raw_parts(segments.clone(),seg_len),ArrayArg::from_raw_parts(masks.clone(),mask_len));}
    if nt>0{k::terms::launch_unchecked::<WgpuRuntime>(&session.client,grid(count*nt),dim,input(),ArrayArg::from_raw_parts(terms.clone(),term_len));}
    k::reduce::launch_unchecked::<WgpuRuntime>(&session.client,grid(count),dim,input(),ArrayArg::from_raw_parts(segments,seg_len),ArrayArg::from_raw_parts(masks,mask_len),ArrayArg::from_raw_parts(terms.clone(),term_len),ArrayArg::from_raw_parts(output.clone(),count));
   }
   drop(permit);
   let bytes=session.client.read_one(output).map_err(|e|Error::new(ErrorKind::RuntimeFailure,e.to_string()))?;let diagnostic=if verify {
    let e=session.client.read_one(edges).map_err(|e|Error::new(ErrorKind::RuntimeFailure,e.to_string()))?;
    let l=session.client.read_one(lengths).map_err(|e|Error::new(ErrorKind::RuntimeFailure,e.to_string()))?;
    let g=session.client.read_one(geo).map_err(|e|Error::new(ErrorKind::RuntimeFailure,e.to_string()))?;
    let t=session.client.read_one(terms).map_err(|e|Error::new(ErrorKind::RuntimeFailure,e.to_string()))?;
    Some((u32::from_bytes(&e).to_vec(),f64::from_bytes(&l).to_vec(),f64::from_bytes(&g).to_vec(),f64::from_bytes(&t).to_vec()))
   }else{None};Ok((f64::from_bytes(&bytes)[..count].to_vec(),diagnostic))
  });
  self.milliseconds+=started.elapsed().as_secs_f64()*1000.0;let (scores,diagnostic)=result.map_err(|e|self.fail(e))?;
  if let Some((edges,lengths,geometry,terms))=diagnostic {
   let mut world=current.clone();
   for (i,c) in candidates.iter().enumerate(){
    for (component,pose) in &c.changes{p.apply(&mut world,*component,pose)?;}
    let score_problem=p.score_problem(&world);
    let expected_terms=crate::post_place::term_reference(&score_problem)?;
    let mut actual=Vec::new();let mut fixed=0.0;let mut added=false;
    for j in 0..nt {let kind=if j<self.data.si[7] as usize{self.data.si[self.data.si[8] as usize+j*5]}else{4};let off=(i*nt+j)*3;
     if kind==2 {fixed+=terms[off];}else{if kind>=3 && !added{actual.push(fixed);added=true;}actual.push((terms[off]+terms[off+1])+terms[off+2]);}
    }if !added {actual.push(fixed);}
    if actual.len()!=expected_terms.len() || actual.iter().zip(&expected_terms).any(|(a,b)|(a-b).abs()>1e-8+b.abs()*1e-12) {return Err(self.fail(Error::new(ErrorKind::RuntimeFailure,format!("post-place GPU term mismatch candidate {i}: GPU={actual:?} CPU={expected_terms:?}"))));}
    let expected=crate::post_place::graph_reference(&score_problem);
    for (j,(from,to,net,len)) in expected.into_iter().enumerate(){let e=i*ns+j;
     if edges[e*3]!=from as u32 || edges[e*3+1]!=to as u32 || edges[e*3+2]!=net as u32 || (lengths[e]-len).abs()>1e-10+len.abs()*1e-13 {return Err(self.fail(Error::new(ErrorKind::RuntimeFailure,format!("post-place GPU MST mismatch candidate {i} edge {j}"))));}
    }
    for (component,def) in p.components.iter().enumerate(){let base=i*self.data.si[11] as usize+self.data.si[self.data.si[1] as usize+component*4] as usize;let b=world.boxes[component];let pose=p.pose(&world,component);
     let mut cpu=vec![pose.x,pose.y,layer(&pose.layer).unwrap() as f64,b.left,b.right,b.top,b.bottom];
     for (pad,point) in world.points[component].iter().enumerate(){let obs=&world.obstacles[def.obstacle_offset+pad];let b=obs.box_;cpu.extend([point.x,point.y,b.left,b.right,b.top,b.bottom,obs.layer.as_ref().map_or(0,|s|layer(s).unwrap()) as f64]);}
     if !cpu.iter().zip(&geometry[base..base+cpu.len()]).all(|(a,b)|a==b){return Err(self.fail(Error::new(ErrorKind::RuntimeFailure,format!("post-place GPU geometry mismatch candidate {i} component {component}"))));}
    }
    for (component,_) in &c.changes {p.apply(&mut world,*component,p.pose(current,*component))?;}
   }
  }
  if scores.iter().any(|s|!s.is_finite()){return Err(self.fail(Error::new(ErrorKind::RuntimeFailure,"non-finite post-place GPU score")));}
  self.batches+=1;self.candidates+=count;Ok(scores)
 }
 pub fn report(&self){eprintln!("[post-place-gpu] {}",serde_json::json!({"batches":self.batches,"candidates":self.candidates,"gpuOperationMs":self.milliseconds,"encodeMs":self.encode_ms,"residentBytes":self.data.si.len()*4+self.data.sf.len()*8,"runtime":gpu::statistics()}));}
}

// Cold single-worker Telemetry is measured; multi-worker admission additionally
// requires an already initialized shared GPU so startup cannot erase the gain.
pub(super) fn profitable(p:&RefineProblem)->bool {
 let workers=p.threads.max(1).min((std::thread::available_parallelism().map_or(1,|n|n.get())/2).clamp(1,8));
 p.components.len()>=154 && p.components.iter().map(|c|c.pads.len()).sum::<usize>()>=660
  && p.nets.iter().map(|n|n.points.len().saturating_sub(1)).sum::<usize>()>=235
  && p.iterations>0 && p.timeout_ms>=2000 && (workers==1 || (workers<=4 && gpu::ready()))
}
