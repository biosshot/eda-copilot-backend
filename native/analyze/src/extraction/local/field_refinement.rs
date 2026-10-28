//! Replay conforming P1 field bisections on an independently rebuilt native seed.
//! No saved coefficient/field solution is consumed; original Cu stays immutable.
use super::{capacitance::TraceRow,compensated::W,error,mesh::{self,LayeredMesh,Tetrahedron,InterfaceFace},volume_current::Space,AllocationBudget,Result};
use std::{collections::{BTreeMap,BTreeSet,HashMap},time::{Duration,Instant},path::Path};
type Edge=[usize;2];type Face=[usize;3];type Weights=BTreeMap<usize,f64>;
const EDGES:[[usize;2];6]=[[0,1],[0,2],[0,3],[1,2],[1,3],[2,3]];
fn edge(mut e:Edge)->Edge{e.sort();e}
fn face(mut f:Face)->Face{f.sort();f}
fn face_edges(f:Face)->[Edge;3]{[edge([f[0],f[1]]),edge([f[0],f[2]]),edge([f[1],f[2]])]}
fn cell_edges(t:&Tetrahedron)->[Edge;6]{EDGES.map(|e|edge([t.nodes[e[0]],t.nodes[e[1]]]))}
fn bits(p:[f64;3])->[u64;3]{p.map(f64::to_bits)}
fn copper_identity(mesh:&LayeredMesh)->Vec<([[u64;3];4],Vec<String>)>{let mut result=mesh.copper_tetrahedra.iter().map(|t|{let mut p=t.nodes.map(|i|bits(mesh.nodes_m[i]));p.sort();let mut ids=t.source_ids.clone();ids.sort();(p,ids)}).collect::<Vec<_>>();result.sort();result}
fn signed(t:[[f64;3];4])->W{let v=t.map(|p|p.map(W::f));let[a,b,c]=[1,2,3].map(|i|std::array::from_fn::<_,3,_>(|k|v[i][k]-v[0][k]));(0..3).fold(W::ZERO,|s,k|s+a[k]*(b[(k+1)%3]*c[(k+2)%3]-b[(k+2)%3]*c[(k+1)%3]))}
fn signed_w(v:[[W;3];4])->W{let[a,b,c]=[1,2,3].map(|i|std::array::from_fn::<_,3,_>(|k|v[i][k]-v[0][k]));(0..3).fold(W::ZERO,|s,k|s+a[k]*(b[(k+1)%3]*c[(k+2)%3]-b[(k+2)%3]*c[(k+1)%3]))}
#[derive(Clone,serde::Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Saved {
 #[serde(default)]pub nodes_m:Vec<[f64;3]>,#[serde(default)]pub node_bits:Option<Vec<[u64;3]>>,pub copper_cells:Vec<[usize;4]>,pub field_cells:Vec<[usize;4]>,#[serde(default)]pub field_epsilon_r:Vec<f64>,#[serde(default)]pub field_epsilon_bits:Option<Vec<u64>>,
 pub external_faces:Vec<Face>,pub field_metal_faces:Vec<Face>,pub field_metal_nodes:Vec<usize>,#[serde(default)]pub field_trace_weights:BTreeMap<String,Vec<(usize,f64)>>,#[serde(default)]pub field_trace_weight_bits:Option<BTreeMap<String,Vec<(usize,u64)>>>,
 pub field_initial_node_count:usize,pub field_node_parent_edges:Vec<Edge>,#[serde(default)]pub field_node_parent_fractions:Vec<f64>,#[serde(default)]pub field_coordinate_model:Option<String>,
}
impl Saved {
 pub fn read(path:impl AsRef<Path>,live:usize,budget:AllocationBudget)->Result<Self>{
  let bytes=usize::try_from(std::fs::metadata(path.as_ref()).map_err(|e|error("FIELD_REFINEMENT",&e.to_string()))?.len()).map_err(|_|error("MEMORY_LIMIT","field geometry length overflow"))?;
  budget.check(bytes.checked_mul(16).and_then(|v|v.checked_add(live)).ok_or_else(||error("MEMORY_LIMIT","field geometry parse plan overflow"))?)?;
  let mut saved:Self=serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path).map_err(|e|error("FIELD_REFINEMENT",&e.to_string()))?)).map_err(|e|error("FIELD_REFINEMENT",&e.to_string()))?;
  if let Some(bits)=saved.node_bits.take(){if !saved.nodes_m.is_empty(){return Err(error("FIELD_REFINEMENT","ambiguous decimal/bit node transport"));}saved.nodes_m=bits.into_iter().map(|p|p.map(f64::from_bits)).collect();}
  if let Some(bits)=saved.field_epsilon_bits.take(){if !saved.field_epsilon_r.is_empty(){return Err(error("FIELD_REFINEMENT","ambiguous decimal/bit epsilon transport"));}saved.field_epsilon_r=bits.into_iter().map(f64::from_bits).collect();}
  if let Some(bits)=saved.field_trace_weight_bits.take(){if !saved.field_trace_weights.is_empty(){return Err(error("FIELD_REFINEMENT","ambiguous decimal/bit trace transport"));}saved.field_trace_weights=bits.into_iter().map(|(node,w)|(node,w.into_iter().map(|(i,v)|(i,f64::from_bits(v))).collect())).collect();}Ok(saved)
 }
}
#[derive(Clone)]enum BoundaryKind{Copper(Vec<String>),Far}
struct Boundary {faces:BTreeMap<Face,BoundaryKind>,edges:BTreeMap<Edge,BTreeSet<Face>>}
impl Boundary {
 fn insert(&mut self,f:Face,kind:BoundaryKind)->Result<()>{let f=face(f);if self.faces.insert(f,kind).is_some(){return Err(error("FIELD_REFINEMENT","duplicate seed/refined boundary face"));}for e in face_edges(f){self.edges.entry(e).or_default().insert(f);}Ok(())}
 fn remove(&mut self,f:Face)->Result<BoundaryKind>{let kind=self.faces.remove(&f).ok_or_else(||error("FIELD_REFINEMENT","missing boundary face"))?;for e in face_edges(f){let empty=if let Some(set)=self.edges.get_mut(&e){set.remove(&f);set.is_empty()}else{false};if empty{self.edges.remove(&e);}}Ok(kind)}
}
fn boundary_split(boundary:&mut Boundary,trace:&mut BTreeMap<usize,Weights>,[a,b]:Edge,node:usize,fraction:f64)->Result<()>{
 let affected=boundary.edges.get(&[a,b]).map(|s|s.iter().copied().collect::<Vec<_>>()).unwrap_or_default();let is_metal=affected.iter().any(|f|matches!(boundary.faces[f],BoundaryKind::Copper(_)));
 if is_metal{let mut weights=Weights::new();for(parent,factor)in[(a,1.-fraction),(b,fraction)]{for(&dof,&value)in trace.get(&parent).ok_or_else(||error("FIELD_REFINEMENT","metal midpoint parent lacks original trace"))?{*weights.entry(dof).or_insert(0.)+=factor*value;}}if weights.values().any(|v|!v.is_finite()||*v<=0.)||(weights.values().sum::<f64>()-1.).abs()>1e-14{return Err(error("FIELD_REFINEMENT","midpoint trace lost positive constant partition"));}trace.insert(node,weights);}
 for f in affected{let kind=boundary.remove(f)?;let other=*f.iter().find(|&&i|i!=a&&i!=b).unwrap();boundary.insert([a,node,other],kind.clone())?;boundary.insert([node,b,other],kind)?;}Ok(())
}
fn batched_exact(seed:&mut LayeredMesh,saved:&Saved,boundary:&mut Boundary,trace:&mut BTreeMap<usize,Weights>,clock:&impl Fn()->Result<()>)->Result<f64>{
 let mut conceptual=seed.nodes_m.iter().map(|p|p.map(W::f)).collect::<Vec<_>>();let mut start=0;let mut maximum=0_f64;
 while start<saved.field_node_parent_edges.len(){
  clock()?;let old_nodes=seed.nodes_m.len();let mut end=start;
  while end<saved.field_node_parent_edges.len()&&end-start<512{let[a,b]=saved.field_node_parent_edges[end];if a>=b{return Err(error("FIELD_REFINEMENT","invalid parent edge ancestry"));}if b>=old_nodes{break;}end+=1;}
  if end==start{return Err(error("FIELD_REFINEMENT","invalid parent edge ancestry"));}
  let mut selected=HashMap::with_capacity(end-start);for index in start..end{if selected.insert(saved.field_node_parent_edges[index],index).is_some(){return Err(error("FIELD_REFINEMENT","duplicate parent edge command"));}}
  let mut counts=vec![0usize;end-start];
  for(index,t)in seed.tetrahedra.iter().enumerate(){if index%1024==0{clock()?;}let mut matches=cell_edges(t).into_iter().filter_map(|e|selected.get(&e).copied()).collect::<Vec<_>>();matches.sort_unstable();for &i in &matches{counts[i-start]+=1;}if matches.len()>1{end=end.min(matches[1]);}}
  selected.retain(|_,index|*index<end);if counts[..end-start].contains(&0){return Err(error("FIELD_REFINEMENT","parent edge is not in the actual native field"));}let touched=counts[..end-start].iter().sum::<usize>();let new_len=seed.tetrahedra.len().checked_add(touched).ok_or_else(||error("MEMORY_LIMIT","batched tetra count overflow"))?;if new_len>saved.field_cells.len(){return Err(error("FIELD_REFINEMENT","commands exceed saved final tetra count"));}
  for index in start..end{let[a,b]=saved.field_node_parent_edges[index];let fraction=saved.field_node_parent_fractions.get(index).copied().unwrap_or(0.5);if ![0.25,0.5,0.75].contains(&fraction){return Err(error("FIELD_REFINEMENT","unsupported dyadic edge parameter"));}let q=std::array::from_fn(|k|conceptual[a][k].scale(1.-fraction)+conceptual[b][k].scale(fraction));let mid=q.map(W::value);if mid==seed.nodes_m[a]||mid==seed.nodes_m[b]||mid.iter().any(|x|!x.is_finite()){return Err(error("FIELD_REFINEMENT","noninterior/unrepresentable field midpoint"));}let node=seed.nodes_m.len();seed.nodes_m.push(mid);conceptual.push(q);boundary_split(boundary,trace,[a,b],node,fraction)?;}
  let previous=std::mem::take(&mut seed.tetrahedra);let mut next=Vec::with_capacity(new_len);
  for(index,parent)in previous.into_iter().enumerate(){if index%1024==0{clock()?;}let chosen=cell_edges(&parent).into_iter().filter_map(|e|selected.get(&e).copied()).collect::<Vec<_>>();if chosen.len()>1{return Err(error("FIELD_REFINEMENT","batched stars overlap a parent cell"));}if chosen.is_empty(){next.push(parent);continue;}let command=chosen[0];let[a,b]=saved.field_node_parent_edges[command];let node=old_nodes+command-start;let determinant=signed_w(parent.nodes.map(|i|conceptual[i]));let mut children=[parent.clone(),parent];children[0].nodes.iter_mut().filter(|i|**i==a).for_each(|i|*i=node);children[1].nodes.iter_mut().filter(|i|**i==b).for_each(|i|*i=node);let mut sum=W::ZERO;
   for child in &mut children{let p=child.nodes.map(|i|conceptual[i]);let det=signed_w(p);if(det*determinant).value()<=0.{return Err(error("FIELD_REFINEMENT","child orientation/positive volume failed"));}sum=sum+det.abs();let(v,g)=mesh::tetra_coefficients_w(p)?;child.volume_m3=v;child.gradients_per_m=g;}
   let partition=((sum-determinant.abs())/determinant.abs()).value().abs();maximum=maximum.max(partition);if partition>1e-10{return Err(error("FIELD_REFINEMENT","unchanged parent volume partition gate failed"));}next.extend(children);
  }
  if next.len()!=new_len{return Err(error("FIELD_REFINEMENT","batched incidence count changed"));}seed.tetrahedra=next;start=end;
 }
 Ok(maximum)
}
pub struct Refined {
 pub mesh:LayeredMesh,pub metal_trace:Vec<TraceRow>,pub original_potential_count:usize,
 pub bisections:usize,pub maximum_parent_volume_error:f64,pub replay_peak_bytes:usize,
}
/// Reconstruct every field cell/material/trace from native seed topology and
/// existing-edge midpoint commands. Saved arrays are verification targets only.
pub fn replay(mut seed:LayeredMesh,current:&LayeredMesh,space:&Space,saved:&Saved,budget:AllocationBudget,limit:Duration)->Result<Refined>{
 let start=Instant::now();let clock=||if start.elapsed()>limit{Err(error("DEADLINE","native conforming field replay"))}else{Ok(())};
 let exact=match saved.field_coordinate_model.as_deref(){None=>false,Some("exact-dyadic-edge-ancestry-v1")=>true,_=>return Err(error("FIELD_REFINEMENT","unsupported field coordinate model"))};
 if saved.nodes_m.len()>budget.max_nodes||saved.field_cells.len().checked_add(saved.copper_cells.len()).is_none_or(|n|n>budget.max_tetrahedra)||saved.field_cells.len()!=saved.field_epsilon_r.len()||saved.field_initial_node_count!=seed.nodes_m.len()||saved.nodes_m.len()!=seed.nodes_m.len()+saved.field_node_parent_edges.len(){return Err(error("FIELD_REFINEMENT","saved node/cell/ancestry counts outside source or budget"));}
 if saved.nodes_m.iter().flatten().any(|v|!v.is_finite())||saved.copper_cells.iter().chain(&saved.field_cells).flatten().any(|&i|i>=saved.nodes_m.len())||saved.field_metal_faces.iter().chain(&saved.external_faces).flatten().any(|&i|i>=saved.nodes_m.len())||saved.field_epsilon_r.iter().any(|v|!v.is_finite()||*v<=0.){return Err(error("FIELD_REFINEMENT","invalid saved vertex/material/index"));}
 let plan=seed.retained_bytes()?.checked_add(current.retained_bytes()?).and_then(|v|v.checked_add(saved.nodes_m.len().checked_mul(1024)?)).and_then(|v|v.checked_add(saved.field_cells.len().checked_mul(if exact{640}else{1536})?)).and_then(|v|v.checked_add(saved.field_node_parent_edges.len().checked_mul(64)?)).ok_or_else(||error("MEMORY_LIMIT","native field replay plan overflow"))?;budget.check(plan)?;
 if copper_identity(&seed)!=copper_identity(current){return Err(error("FIELD_REFINEMENT","seed changed original Cu geometry/source ownership"));}
 let mut source_nodes=BTreeMap::new();for(&node,&dof)in &space.node_potential{let key=bits(current.nodes_m[node]);if source_nodes.insert(key,dof).is_some_and(|old|old!=dof){return Err(error("FIELD_REFINEMENT","ambiguous original current node bits"));}}
 let mut trace=BTreeMap::<usize,Weights>::new();let mut boundary=Boundary{faces:BTreeMap::new(),edges:BTreeMap::new()};
 for node in seed.copper_tetrahedra.iter().flat_map(|t|t.nodes).collect::<BTreeSet<_>>(){let dof=*source_nodes.get(&bits(seed.nodes_m[node])).ok_or_else(||error("FIELD_REFINEMENT","seed metal trace lacks exact original current node"))?;trace.insert(node,BTreeMap::from([(dof,1.)]));}
 for f in &seed.copper_interfaces{boundary.insert(f.nodes,BoundaryKind::Copper(f.copper_source_ids.clone()))?;}
 for f in &seed.external_faces{boundary.insert(*f,BoundaryKind::Far)?;}
 if !saved.field_node_parent_fractions.is_empty()&&saved.field_node_parent_fractions.len()!=saved.field_node_parent_edges.len(){return Err(error("FIELD_REFINEMENT","dyadic parameter count differs from ancestry"));}
 let maximum=if exact{batched_exact(&mut seed,saved,&mut boundary,&mut trace,&clock)?}else{
 let mut arena=std::mem::take(&mut seed.tetrahedra).into_iter().map(Some).collect::<Vec<_>>();let mut incidence=BTreeMap::<Edge,BTreeSet<usize>>::new();
 for(i,t)in arena.iter().enumerate(){for e in cell_edges(t.as_ref().unwrap()){incidence.entry(e).or_default().insert(i);}}
 let mut maximum=0_f64;let mut conceptual=if exact{seed.nodes_m.iter().map(|p|p.map(W::f)).collect::<Vec<_>>()}else{vec![]};
 for(index,&[a,b])in saved.field_node_parent_edges.iter().enumerate(){
  clock()?;if a>=b||b>=seed.nodes_m.len(){return Err(error("FIELD_REFINEMENT","invalid parent edge ancestry"));}let fraction=saved.field_node_parent_fractions.get(index).copied().unwrap_or(0.5);if ![0.25,0.5,0.75].contains(&fraction){return Err(error("FIELD_REFINEMENT","unsupported dyadic edge parameter"));}let e=[a,b];let attached=incidence.get(&e).ok_or_else(||error("FIELD_REFINEMENT","parent edge is not in the actual native field"))?.iter().copied().collect::<Vec<_>>();let node=seed.nodes_m.len();let mid=std::array::from_fn(|k|if fraction==0.5{(seed.nodes_m[a][k]+seed.nodes_m[b][k])*0.5}else{seed.nodes_m[a][k]*(1.-fraction)+seed.nodes_m[b][k]*fraction});
  let mid=if exact{let q=std::array::from_fn(|k|conceptual[a][k].scale(1.-fraction)+conceptual[b][k].scale(fraction));conceptual.push(q);q.map(W::value)}else{mid};
  if mid==seed.nodes_m[a]||mid==seed.nodes_m[b]||mid.iter().any(|x|!x.is_finite()){return Err(error("FIELD_REFINEMENT","noninterior/unrepresentable field midpoint"));}seed.nodes_m.push(mid);
  boundary_split(&mut boundary,&mut trace,e,node,fraction)?;
  for index in attached{
   let parent=arena[index].take().ok_or_else(||error("FIELD_REFINEMENT","retired parent reused"))?;
   for old in cell_edges(&parent){let empty={let set=incidence.get_mut(&old).ok_or_else(||error("FIELD_REFINEMENT","missing native edge incidence"))?;if !set.remove(&index){return Err(error("FIELD_REFINEMENT","lost parent edge incidence"));}set.is_empty()};if empty{incidence.remove(&old);}}
   let mut children=[parent.clone(),parent.clone()];children[0].nodes.iter_mut().filter(|i|**i==a).for_each(|i|*i=node);children[1].nodes.iter_mut().filter(|i|**i==b).for_each(|i|*i=node);
   let determinant=if exact{signed_w(parent.nodes.map(|i|conceptual[i]))}else{signed(parent.nodes.map(|i|seed.nodes_m[i]))};let mut sum=W::ZERO;
   for child in &mut children{let p=child.nodes.map(|i|if exact{conceptual[i]}else{seed.nodes_m[i].map(W::f)});let det=signed_w(p);if (det*determinant).value()<=0.{return Err(error("FIELD_REFINEMENT","child orientation/positive volume failed"));}sum=sum+det.abs();let(v,g)=mesh::tetra_coefficients_w(p)?;child.volume_m3=v;child.gradients_per_m=g;}
   let partition_error=((sum-determinant.abs())/determinant.abs()).value().abs();maximum=maximum.max(partition_error);if partition_error>1e-10{return Err(error("FIELD_REFINEMENT","unchanged parent volume partition gate failed"));}
   for child in children{let index=arena.len();for e in cell_edges(&child){incidence.entry(e).or_default().insert(index);}arena.push(Some(child));}
  }
 }
 seed.tetrahedra=arena.into_iter().flatten().collect();drop(incidence); maximum
 };
 seed.copper_interfaces.clear();seed.external_faces.clear();
 for(f,kind)in boundary.faces{match kind{BoundaryKind::Copper(ids)=>seed.copper_interfaces.push(InterfaceFace{nodes:f,copper_source_ids:ids}),BoundaryKind::Far=>seed.external_faces.push(f)}}
 if seed.nodes_m.iter().map(|p|p.map(f64::to_bits)).ne(saved.nodes_m.iter().map(|p|p.map(f64::to_bits))){return Err(error("FIELD_REFINEMENT","replayed final node bits differ from saved geometry"));}
 let mut cells=seed.tetrahedra.iter().map(|t|{let mut n=t.nodes;n.sort();(n,t.epsilon_r.to_bits())}).collect::<Vec<_>>();let mut expected=saved.field_cells.iter().zip(&saved.field_epsilon_r).map(|(n,e)|{let mut n=*n;n.sort();(n,e.to_bits())}).collect::<Vec<_>>();cells.sort();expected.sort();if cells!=expected{return Err(error("FIELD_REFINEMENT","replayed tetra/material complex differs from saved geometry"));}
 let expected_faces=saved.field_metal_faces.iter().copied().map(face).collect::<BTreeSet<_>>();let actual_faces=seed.copper_interfaces.iter().map(|f|face(f.nodes)).collect::<BTreeSet<_>>();if expected_faces.len()!=saved.field_metal_faces.len()||expected_faces!=actual_faces||saved.external_faces.len()!=seed.external_faces.len()||saved.external_faces.iter().copied().map(face).collect::<BTreeSet<_>>()!=seed.external_faces.iter().copied().map(face).collect(){return Err(error("FIELD_REFINEMENT","replayed complete boundaries differ"));}
 if trace.len()!=saved.field_metal_nodes.len()||trace.len()!=saved.field_trace_weights.len()||trace.keys().copied().collect::<BTreeSet<_>>()!=saved.field_metal_nodes.iter().copied().collect(){return Err(error("FIELD_REFINEMENT","saved metal node coverage differs"));}
 for(&node,weights)in &trace{let expected=saved.field_trace_weights.get(&node.to_string()).ok_or_else(||error("FIELD_REFINEMENT","saved midpoint trace row missing"))?;if weights.iter().map(|(&i,&v)|(i,v)).ne(expected.iter().copied()){return Err(error("FIELD_REFINEMENT","saved trace differs from original P1/contact ancestry"));}}
 let mut cu=saved.copper_cells.iter().map(|t|{let mut p=t.map(|i|bits(saved.nodes_m[i]));p.sort();p}).collect::<Vec<_>>();let mut actual=seed.copper_tetrahedra.iter().map(|t|{let mut p=t.nodes.map(|i|bits(seed.nodes_m[i]));p.sort();p}).collect::<Vec<_>>();cu.sort();actual.sort();if cu!=actual{return Err(error("FIELD_REFINEMENT","saved original Cu differs"));}
 seed.dielectric_volume_m3=seed.tetrahedra.iter().map(|t|t.volume_m3).sum();seed.planned_peak_bytes=plan;clock()?;
 let exposed=seed.copper_interfaces.iter().flat_map(|f|f.nodes).collect::<BTreeSet<_>>();
 Ok(Refined{mesh:seed,metal_trace:trace.into_iter().filter(|(node,_)|exposed.contains(node)).map(|(node,w)|TraceRow{node,weights:w.into_iter().collect()}).collect(),original_potential_count:space.potential_count(),bisections:saved.field_node_parent_edges.len(),maximum_parent_volume_error:maximum,replay_peak_bytes:plan})
}
