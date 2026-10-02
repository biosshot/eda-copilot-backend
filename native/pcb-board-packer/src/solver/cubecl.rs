//! Board-owned resident templates, bounded batches and shared runtime access.
use crate::compute::f32_runtime::PcbRuntime as WgpuRuntime;
use super::{compact::Pose, compact::Template, Context, WorkingPrimitive, CompiledEndpoint, Side};
use crate::compute::{gpu, Error, ErrorKind, Requirements, ScratchKey};
use crate::geometry::Box2;
use crate::model::{BoardPackProblem, Rank};
use ::cubecl::{prelude::*, server::Handle};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::sync::atomic::{AtomicU64,Ordering};
use std::collections::BTreeMap;
use super::gpu_kernels as k;

const REQUIREMENTS: Requirements = Requirements { f32: true, u64: false };
const CHUNK: usize = 4096;
#[derive(Default,serde::Serialize)]
struct Timings {
    batches:usize,candidates:usize,encode_ms:f64,upload_ms:f64,submit_ms:f64,read_ms:f64,gpu_operation_ms:f64,
}
#[derive(Clone)]
struct FixedFrame {geometry:Handle,unary:Handle,tags:Handle,pairs:Handle,bytes:usize}
pub(super) struct Engine {
    frames:FxHashMap<Vec<u32>,FixedFrame>,frame_hits:usize,frame_misses:usize,
    sf: Vec<f32>, si: Vec<u32>,
    templates: FxHashMap<Vec<u32>,u32>,
    template_pointers:FxHashMap<usize,(Arc<Template>,u32)>,
    base_sf:usize,base_si:usize,
    handles: Option<(Handle, Handle)>,
    batches: usize,
    candidates: usize,
    markers: BTreeMap<Arc<str>,f32>,
    net_points: BTreeMap<Arc<str>,Vec<Vec<usize>>>,
    ordinary_plans: FxHashMap<Vec<u32>,Arc<Vec<OrdinaryPair>>>,
    stages:BTreeMap<&'static str,Timings>,phase:&'static str,fail_batch:Option<usize>,legality_candidates:usize,rank_candidates:usize,
}
#[derive(Default)]
struct OrdinaryPair {
    a:usize,b:usize,affinity:f32,nets:Vec<(Vec<usize>,Vec<usize>)>,
}
pub(super) struct Evaluated {pub index:usize,pub rank:Rank}
#[derive(Clone,Copy)]
struct Selection<'a> {flags:&'a[bool],ordinary:usize,aligned:usize}
pub(super) struct Control {
    engine: Mutex<Engine>,
    failure: Mutex<Option<Error>>,
    wait_nanos:AtomicU64,
    _call:Option<gpu::CallPermit>,
}
#[derive(Debug)]
pub(super) struct Abort;

fn u(value: usize) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| Error::new(ErrorKind::InvalidInput, "board GPU index overflow"))
}
fn floats(values: &[f32]) -> Result<(), Error> {
    if values.iter().any(|v| !v.is_finite() || v.abs() > 1e8) {
        Err(Error::new(ErrorKind::InvalidInput, "unsafe board GPU number"))
    } else { Ok(()) }
}
fn put_box(v: &mut Vec<f32>, b: Box2) -> u32 {
    let off=v.len() as u32;v.extend([b.left,b.right,b.top,b.bottom]);off
}
fn layer(value: &str) -> u32 { if value=="top" {0} else {1} }

// Workload floors relaxed by approximately 8x from the Telemetry baseline.
// These are admission heuristics; device/input guards and CPU recovery still apply.
pub(super) fn auto_profitable(p:&BoardPackProblem)->bool {
    p.primitives.len()>=4 && p.components.len()>=19 && p.search_width>=4 && p.relations.len()>=66
        && p.primitives.iter().filter(|p|!p.locked).count()>=3
}

pub(super) fn supported(p: &BoardPackProblem) -> Result<(), String> {
    // Reject unsupported/unsafe inputs before search. Acceptance of this guard
    // still requires the saved-board and numerical-boundary validations.
    if p.components.iter().any(|c| !matches!(c.layer.as_ref(), "top"|"bottom")) {
        return Err("unknown component layer".into());
    }
    let ordered=|b:&Box2|b.left<=b.right && b.top<=b.bottom;
    if !ordered(&p.bounds) || !ordered(&p.full_board_bounds) || p.board_outline.len()<3
        || p.obstacles.iter().any(|b|!ordered(b)) || p.constraint_regions.iter().any(|r|!ordered(&r.box_))
        || p.primitives.iter().any(|p|!ordered(&p.bbox) || p.collision_boxes.iter().any(|b|!ordered(b)))
        || p.components.iter().any(|c|!ordered(&c.body_box) || c.through_hole_boxes.iter().any(|b|!ordered(b))) {
        return Err("unsupported board GPU box or outline".into());
    }
    let mut values=vec![p.grid,p.clearance,p.edge_clearance];
    for primitive in &p.primitives {
        values.extend([primitive.width,primitive.height]);
        for b in std::iter::once(&primitive.bbox).chain(primitive.collision_boxes.iter()) {values.extend([b.left,b.right,b.top,b.bottom]);}
        for q in primitive.connection_points.iter() {values.extend([q.x,q.y]);}
        for q in primitive.placements.iter() {
            if !(0..360).contains(&q.rotate) {return Err("unsafe orientation range".into());}values.extend([q.x,q.y,q.score]);
        }
        for q in primitive.path_ports.iter() {values.extend([q.x,q.y,q.normal.x,q.normal.y]);}
        if primitive.edge_place.as_ref().is_some_and(|e|e.edges.is_empty()) {return Err("empty edge alternatives".into());}
        if let Some(e)=&primitive.edge_place {values.extend([e.inset.unwrap_or(0.0),e.x.unwrap_or(0.0),e.y.unwrap_or(0.0),e.offset.unwrap_or(0.0)]);}
    }
    for c in &p.components {
        for b in std::iter::once(&c.body_box).chain(c.through_hole_boxes.iter()) {values.extend([b.left,b.right,b.top,b.bottom]);}
        values.extend([c.edge_clearance,c.board_overflow.left,c.board_overflow.right,c.board_overflow.top,c.board_overflow.bottom]);
    }
    floats(&values).map_err(|e|e.to_string())?;
    let unique=p.primitives.iter().map(|p|p.id.as_ref()).collect::<std::collections::HashSet<_>>();
    if unique.len()!=p.primitives.len() {return Err("duplicate board GPU primitive identities".into());}
    // Counts enter u32 reductions and Vulkan dispatch axes. Bound every geometry
    // collection before resident packing, including the worst possible hard count.
    let geometry=p.primitives.iter().map(|p|p.collision_boxes.len()+p.connection_points.len()+p.placements.len()+p.path_ports.len()).sum::<usize>()
        +p.components.iter().map(|c|1+c.through_hole_boxes.len()).sum::<usize>();
    if geometry>65536 || p.obstacles.len()>4096 || p.constraint_regions.len()>4096 || p.board_outline.len()>65536
        || p.relations.len()>65536 || p.soft_alignment.as_ref().is_some_and(|a|a.pairs.len()>65536) {
        return Err("board GPU geometry capacity guard".into());
    }
    let n=p.components.len();
    if n.checked_mul(n).is_none() || n>2048 || p.primitives.len()>2048 {return Err("board GPU capacity guard".into());}
    // CPU hard-overlap caches use an unordered pose-pair key. Directional
    // matrices therefore depend on prior cache access; keep that case on CPU.
    for i in 0..n {for j in 0..i {
        if (p.component_conflict[i*n+j]!=0)!=(p.component_conflict[j*n+i]!=0)
            || p.component_pair_clearance[i*n+j].to_bits()!=p.component_pair_clearance[j*n+i].to_bits() {
            return Err("asymmetric board GPU component rules".into());
        }
    }}
    Ok(())
}

impl Engine {
    pub fn new(p:&BoardPackProblem)->Result<Self,Error> {
        supported(p).map_err(|s|Error::new(ErrorKind::InvalidInput,s))?;
        let mut sf=Vec::new();let mut si=vec![0;21];
        put_box(&mut sf,p.bounds);put_box(&mut sf,p.full_board_bounds);
        sf.extend([p.clearance,p.grid,p.soft_spacing.as_ref().map_or(1.0,|s|s.compactness_scale),
            p.soft_spacing.as_ref().map_or(0.0,|s|s.gap)]);
        si[0]=u32::from(p.compactness.as_ref()=="high");si[1]=u(p.components.len())?;
        si[2]=u(si.len())?;si.extend(p.component_conflict.iter().map(|v|*v as u32));
        si[3]=u(sf.len())?;sf.extend_from_slice(&p.component_pair_clearance);
        si[4]=u(sf.len())?;si[5]=u(p.board_outline.len())?;
        for point in &p.board_outline {sf.extend([point.x,point.y]);}
        si[6]=u(sf.len())?;si[7]=u(p.obstacles.len())?;
        for b in &p.obstacles {put_box(&mut sf,*b);}
        si[8]=u(si.len())?;si[9]=u(p.constraint_regions.len())?;
        for region in &p.constraint_regions {
            let mask=region.layers.iter().fold(0,|mask,l|mask|if l.as_ref()=="top" {1} else if l.as_ref()=="bottom" {2} else {0});
            si.extend([put_box(&mut sf,region.box_),mask]);
        }
        si[10]=u(si.len())?;
        for region in &p.constraint_regions {for c in &p.components {si.push(u32::from(region.allow_blocks.contains(&c.block_name)));}}
        let ids=super::lexical_ids(p.primitives.iter().map(|p|p.id.clone()));
        let np=p.primitives.len();si[11]=u(si.len())?;si[12]=u(np)?;
        let mut exemptions=vec![0;np.checked_mul(np).ok_or_else(||Error::new(ErrorKind::InvalidInput,"exemption size overflow"))?];
        if let Some(s)=&p.soft_spacing {for pair in &s.exempt_pairs {
            if let (Some(&a),Some(&b))=(ids.get(&pair[0]),ids.get(&pair[1])) {exemptions[a as usize*np+b as usize]=1;exemptions[b as usize*np+a as usize]=1;}
        }} si.extend(exemptions);
        // Primitive membership and matrix rules never change with a pose.
        // Precompute the identical CPU maximum/any reductions once per call.
        let mut members=vec![Vec::new();np];
        for (i,c) in p.components.iter().enumerate() {if let Some(&id)=ids.get(&c.primitive_id) {members[id as usize].push(i);}}
        si[19]=u(sf.len())?;si[20]=u(si.len())?;
        for a in &members {for b in &members {
            let mut clearance=p.clearance;let mut conflict=a.is_empty() || b.is_empty();
            for &i in a {for &j in b {let off=i*p.components.len()+j;
                clearance=clearance.max(p.component_pair_clearance[off]);conflict|=p.component_conflict[off]!=0;
            }}sf.push(clearance);si.push(u32::from(conflict));
        }}
        let designator_ids=super::lexical_ids(p.primitives.iter().flat_map(|p|p.placements.iter().map(|q|q.designator.clone())));
        let relations=super::compile_relations(p,&ids,&designator_ids);
        si[13]=u(si.len())?;si[14]=u(relations.len())?;
        let relation_start=si.len();si.resize(si.len()+relations.len()*7,0);
        fn endpoint(sf:&mut Vec<f32>,si:&mut Vec<u32>,e:CompiledEndpoint)->Result<u32,Error> {
            let off=u(si.len())?;
            let desc=match e {
                CompiledEndpoint::Anchor(point)=>{let f=u(sf.len())?;sf.extend([point.x,point.y]);[0,0,0,f]},
                CompiledEndpoint::Pad{primitive,point}=>[1,primitive,u(point)?,0],
                CompiledEndpoint::Component{primitive,designator}=>[2,primitive,designator,0],
                CompiledEndpoint::Primitive{primitive}=>[3,primitive,0,0],
                CompiledEndpoint::Missing=>[4,0,0,0],
            };si.extend(desc);Ok(off)
        }
        for (i,r) in relations.iter().enumerate() {
            let from=endpoint(&mut sf,&mut si,r.from)?;let to=endpoint(&mut sf,&mut si,r.to)?;
            let f=u(sf.len())?;
            sf.extend([r.weight,r.max_distance.unwrap_or(0.0),r.min_distance.unwrap_or(0.0),
                r.anchor_offset.map_or(0.0,|p|p.x),r.anchor_offset.map_or(0.0,|p|p.y)]);
            let side=match r.side {Some(Side::Left)=>0,Some(Side::Right)=>1,Some(Side::Top)=>2,Some(Side::Bottom)=>3,None=>4};
            si[relation_start+i*7..relation_start+i*7+7].copy_from_slice(&[from,to,
                u32::from(r.skip)|u32::from(r.hard)*2|u32::from(r.satellite_anchor)*4|u32::from(r.anchor_offset.is_some())*8,
                u32::from(r.max_distance.is_some()),u32::from(r.min_distance.is_some()),side,f]);
        }
        si[15]=u(si.len())?;si[16]=u(p.soft_alignment.as_ref().map_or(0,|s|s.pairs.len()))?;
        if let Some(alignment)=&p.soft_alignment {for pair in &alignment.pairs {
            let f=u(sf.len())?;sf.extend([alignment.weight,alignment.tolerance,pair.similarity,alignment.orientation_weight]);
            let mut oa=u32::MAX;let mut ob=u32::MAX;let mut of=0;
            if let Some(o)=&pair.orientation {
                oa=designator_ids[&o.a];ob=designator_ids[&o.b];of=u(sf.len())?;
                // Angles are template constants. Compute libm cosine once per
                // possible normalized rotation difference, never per candidate.
                for difference in -359..=359 {sf.push(1.0-crate::rotation::cos_degrees(difference as f32-o.offset));}
            }
            si.extend([ids[&pair.a],ids[&pair.b],pair.anchor_a.as_ref().map_or(u32::MAX,|a|designator_ids[a]),
                pair.anchor_b.as_ref().map_or(u32::MAX,|b|designator_ids[b]),f,oa,ob,of]);
        }}
        let path_refs=super::lexical_ids(p.primitives.iter().flat_map(|p|p.path_ports.iter().map(|q|q.reference.clone())));
        let mut paths=BTreeMap::<Arc<str>,BTreeMap<i32,Vec<(u32,usize,u32)>>>::new();
        for primitive in &p.primitives {for (index,port) in primitive.path_ports.iter().enumerate() {
            paths.entry(port.path_id.clone()).or_default().entry(port.order).or_default()
                .push((ids[&primitive.id],index,path_refs[&port.reference]));
        }}
        let metadata=crate::signal_path::path_metadata(&p.relations);
        si[17]=u(si.len())?;si[18]=u(paths.len())?;
        let path_start=si.len();si.resize(si.len()+paths.len()*4,0);
        for (i,(path,groups)) in paths.iter().enumerate() {
            let group_start=u(si.len())?;si.resize(si.len()+groups.len()*4,0);
            for (j,(&order,entries)) in groups.iter().enumerate() {
                let offset=u(si.len())?;
                for &(id,index,reference) in entries {si.extend([id,u(index)?,reference]);}
                let target=order.checked_add(1).and_then(|next|groups.keys().position(|&o|o==next)).map_or(u32::MAX,|n|n as u32);
                si[group_start as usize+j*4..group_start as usize+j*4+4].copy_from_slice(&[order as u32,offset,u(entries.len())?,target]);
            }
            let meta=metadata.get(path.as_ref()).copied();
            let straight=meta.is_some_and(|m|m.straight);
            let priority=match meta.map_or(1,|m|m.priority) {3=>1.8,2=>1.35,0=>0.65,_=>1.0};
            let f=u(sf.len())?;sf.extend([if straight {22.0}else{8.0},if straight {18.0}else{5.0},priority,meta.map_or(1.0,|m|m.weight)]);
            si[path_start+i*4..path_start+i*4+4].copy_from_slice(&[group_start,u(groups.len())?,f,u32::from(meta.is_some_and(|m|m.prefer_facing_pads))]);
        }
        floats(&sf)?;
        let markers:BTreeMap<Arc<str>,f32>=p.relations.iter().filter_map(|r| {
            let net=r.from.strip_prefix(crate::ordinary_net::MARKER_PREFIX)?;
            if r.from!=r.to {return None;}let weight=r.weight.unwrap_or(1.0);
            (weight.is_finite() && weight>0.0).then(||(Arc::from(net),weight))
        }).collect();
        let mut net_points=BTreeMap::new();
        for net in markers.keys() {
            let mut members=vec![Vec::new();np];
            for primitive in &p.primitives {members[ids[&primitive.id] as usize]=primitive.connection_points.iter()
                .enumerate().filter_map(|(i,q)|(q.net.as_deref()==Some(net.as_ref())).then_some(i)).collect();}
            net_points.insert(net.clone(),members);
        }
        if sf.len()*4+si.len()*4>64*1024*1024 {return Err(Error::new(ErrorKind::InvalidInput,"board GPU resident capacity guard"));}
        let base_sf=sf.len();let base_si=si.len();
        Ok(Self {frames:FxHashMap::default(),frame_hits:0,frame_misses:0,sf,si,templates:FxHashMap::default(),template_pointers:FxHashMap::default(),base_sf,base_si,handles:None,batches:0,candidates:0,markers,net_points,ordinary_plans:FxHashMap::default(),stages:BTreeMap::new(),phase:"beam",legality_candidates:0,rank_candidates:0,fail_batch:std::env::var("PCB_BOARD_GPU_FAIL_BATCH").ok().and_then(|v|v.parse().ok())})
    }
    fn template(&mut self,t:&Arc<Template>,p:&BoardPackProblem)->Result<u32,Error> {
        let key=Arc::as_ptr(t) as usize;
        if let Some((_,index))=self.template_pointers.get(&key) {return Ok(*index);}
        let geometry_key=t.key();
        if let Some(&index)=self.templates.get(&geometry_key) {
            self.template_pointers.insert(key,(t.clone(),index));return Ok(index);
        }
        // Retain the Arc: an allocator cannot recycle a registered pointer.
        let start=u(self.si.len())?;self.si.resize(self.si.len()+19,0);
        let float_start=u(self.sf.len())?;let mut coordinates=Vec::<(u32,u32)>::new();
        let mark_box=|coordinates:&mut Vec<(u32,u32)>,f:u32| {coordinates.extend([(f,0),(f+1,0),(f+2,1),(f+3,1)]);};
        let v=&t.primitive;
        let bbox=put_box(&mut self.sf,v.bbox);mark_box(&mut coordinates,bbox);let boxes=u(self.sf.len())?;
        for b in v.collision_boxes.iter() {let f=put_box(&mut self.sf,*b);mark_box(&mut coordinates,f);}
        let component_start=u(self.si.len())?;
        self.si.resize(self.si.len()+t.components.len()*8,0);
        for (j,(index,c)) in t.components.iter().enumerate() {
            let body=put_box(&mut self.sf,c.body_box);mark_box(&mut coordinates,body);let holes=u(self.sf.len())?;
            for b in c.through_hole_boxes.iter() {let f=put_box(&mut self.sf,*b);mark_box(&mut coordinates,f);}
            let full=p.full_board_bounds;let o=c.board_overflow;let edge=c.edge_clearance.max(0.0);
            let bounds=put_box(&mut self.sf,Box2 {
                left:if o.left>0.0 {full.left-o.left}else{full.left+edge},
                right:if o.right>0.0 {full.right+o.right}else{full.right-edge},
                top:if o.top>0.0 {full.top-o.top}else{full.top+edge},
                bottom:if o.bottom>0.0 {full.bottom+o.bottom}else{full.bottom-edge},
            });
            let ef=u(self.sf.len())?;self.sf.push(c.edge_clearance);
            let entry=component_start as usize+j*8;
            self.si[entry..entry+8].copy_from_slice(&[u(*index)?,layer(&c.layer),body,holes,u(c.through_hole_boxes.len())?,bounds,ef,
                u32::from(o.left>0.0 || o.right>0.0 || o.top>0.0 || o.bottom>0.0)]);
        }
        let mut intent=0;
        if let Some(e)=&v.edge_place {
            intent=u(self.si.len())?;let f=u(self.sf.len())?;
            self.sf.extend([e.inset.unwrap_or(0.0),e.x.unwrap_or(0.0),e.y.unwrap_or(0.0)]);
            self.si.extend([u(e.edges.len())?,f,u32::from(e.x.is_some()),u32::from(e.y.is_some())]);
            self.si.extend(e.edges.iter().map(|e|match e.as_ref(){"left"=>0,"right"=>1,"top"=>2,_=>3}));
        }
        let edge=u(self.sf.len())?;
        self.sf.push(v.edge_place.as_ref().and_then(|e|e.inset).unwrap_or(p.edge_clearance).max(0.0));
        let cp=u(self.si.len())?;
        for (j,q) in v.connection_points.iter().enumerate() {
            let f=u(self.sf.len())?;self.sf.extend([q.x,q.y]);coordinates.extend([(f,0),(f+1,1)]);
            self.si.extend([f,t.point_component_ids[j].unwrap_or(u32::MAX)]);
        }
        let placements=u(self.si.len())?;
        for (j,q) in v.placements.iter().enumerate() {
            let f=u(self.sf.len())?;self.sf.extend([q.x,q.y]);coordinates.extend([(f,0),(f+1,1)]);
            self.si.extend([f,t.placement_ids[j],q.rotate as u32]);
        }
        let ports=u(self.si.len())?;
        for q in v.path_ports.iter() {
            let f=u(self.sf.len())?;self.sf.extend([q.x,q.y,q.normal.x,q.normal.y]);coordinates.extend([(f,0),(f+1,1)]);self.si.push(f);
        }
        let packing=put_box(&mut self.sf,crate::geometry::union_boxes(&v.collision_boxes));
        if !v.collision_boxes.is_empty() {mark_box(&mut coordinates,packing);}
        self.si[start as usize+18]=packing;
        let float_len=u(self.sf.len())?-float_start;let masks=u(self.si.len())?;
        let mut axes=vec![2;float_len as usize];for (f,axis) in coordinates {axes[(f-float_start) as usize]=axis;}self.si.extend(axes);
        self.si[start as usize+15..start as usize+18].copy_from_slice(&[float_start,float_len,masks]);
        self.si[start as usize+9..start as usize+15].copy_from_slice(&[cp,u(v.connection_points.len())?,placements,u(v.placements.len())?,ports,u(v.path_ports.len())?]);
        self.si[start as usize..start as usize+9].copy_from_slice(&[t.id,u32::from(v.locked)|u32::from(v.kind.as_ref()=="module")*2,
            bbox,boxes,u(v.collision_boxes.len())?,component_start,u(t.components.len())?,intent,edge]);
        floats(&self.sf)?;
        self.templates.insert(geometry_key,start);self.template_pointers.insert(key,(t.clone(),start));self.handles=None;Ok(start)
    }
    fn append_cpu_geometry(&self,item:&WorkingPrimitive,template:u32,expected:&mut Vec<f32>) {
        let t=template as usize;let start=self.si[t+15] as usize;let len=self.si[t+16] as usize;
        let mut values=self.sf[start..start+len].to_vec();
        let mut set=|offset:usize,v:&[f32]|values[offset-start..offset-start+v.len()].copy_from_slice(v);
        let box_values=|b:Box2|[b.left,b.right,b.top,b.bottom];
        set(self.si[t+2] as usize,&box_values(item.primitive.bbox));
        set(self.si[t+18] as usize,&box_values(super::packing_box(item)));
        for (i,b) in item.primitive.collision_boxes.iter().enumerate() {set(self.si[t+3] as usize+i*4,&box_values(*b));}
        for (i,(_,c)) in item.components.iter().enumerate() {
            let entry=self.si[t+5] as usize+i*8;set(self.si[entry+2] as usize,&box_values(c.body_box));
            for (j,b) in c.through_hole_boxes.iter().enumerate() {set(self.si[entry+3] as usize+j*4,&box_values(*b));}
        }
        for (i,q) in item.primitive.connection_points.iter().enumerate() {set(self.si[self.si[t+9] as usize+i*2] as usize,&[q.x,q.y]);}
        for (i,q) in item.primitive.placements.iter().enumerate() {set(self.si[self.si[t+11] as usize+i*3] as usize,&[q.x,q.y]);}
        for (i,q) in item.primitive.path_ports.iter().enumerate() {set(self.si[self.si[t+13] as usize+i] as usize,&[q.x,q.y,q.normal.x,q.normal.y]);}
        expected.extend(values);
    }
    fn ordinary_plan(&mut self,ids:Vec<u32>)->Arc<Vec<OrdinaryPair>> {
        if let Some(plan)=self.ordinary_plans.get(&ids) {return plan.clone();}
        let mut pairs=FxHashMap::<(usize,usize),OrdinaryPair>::default();
        for (net,&weight) in &self.markers {
            let members:Vec<_>=ids.iter().enumerate().filter_map(|(i,&id)| {
                let points=&self.net_points[net][id as usize];(!points.is_empty()).then_some((i,points))
            }).collect();
            let count=members.len();if !(2..=8).contains(&count) {continue;}
            let contribution=weight/(count as f32-1.0);
            for a in 0..count {for b in a+1..count {
                let entry=pairs.entry((members[a].0,members[b].0)).or_insert_with(||OrdinaryPair {
                    a:members[a].0,b:members[b].0,..Default::default()
                });
                entry.affinity=(entry.affinity+contribution).min(4.0);
                entry.nets.push((members[a].1.clone(),members[b].1.clone()));
            }}
        }
        // Same FxHashMap key type, insertion sequence and hasher as the CPU
        // reference: values() summation order is part of the numerical contract.
        let plan:Arc<Vec<OrdinaryPair>>=Arc::new(pairs.into_values().collect());
        if self.ordinary_plans.len()>=512 {self.ordinary_plans.clear();}
        self.ordinary_plans.insert(ids,plan.clone());plan
    }
    pub fn evaluate(&mut self,fixed:&[WorkingPrimitive],states:&[Vec<WorkingPrimitive>],context:&Context,legality_only:bool,selection:Option<Selection<'_>>)->Result<Vec<Evaluated>,Error> {
        if states.is_empty() {return Ok(Vec::new());}
        let verify=std::env::var("PCB_BOARD_GPU_VERIFY").as_deref()==Ok("1");
        let full_ranks=!legality_only || verify;
        let verify_shortlist=std::env::var("PCB_BOARD_GPU_VERIFY_SHORTLIST").as_deref()==Ok("1");
        if selection.is_some_and(|s|s.flags.len()!=states.len() || s.ordinary+s.aligned>96) {
            return Err(Error::new(ErrorKind::InvalidInput,"invalid board GPU selection"));
        }
        let moving=states[0].len();
        if states.iter().any(|s|s.len()!=moving) {return Err(Error::new(ErrorKind::InvalidInput,"mixed board batch widths"));}
        let ids:Vec<_>=fixed.iter().chain(states[0].iter()).map(|p|p.id).collect();
        if states.iter().any(|s|!s.iter().map(|p|p.id).eq(states[0].iter().map(|p|p.id))) {return Err(Error::new(ErrorKind::InvalidInput,"mixed board state identities"));}
        let ordinary=self.ordinary_plan(ids);
        let mut result=Vec::with_capacity(states.len());
        let fixed_pairs=fixed.len().saturating_sub(1)*fixed.len()/2;
        let cross_pairs=fixed.len()*moving+moving.saturating_sub(1)*moving/2;
        let requested_chunk=std::env::var("PCB_BOARD_GPU_CHUNK_SIZE").ok().and_then(|v|v.parse::<usize>().ok()).unwrap_or(CHUNK).clamp(1,CHUNK);
        let geometry_size=|p:&WorkingPrimitive| {
            8+p.primitive.collision_boxes.len()*4+p.components.iter().map(|(_,c)|13+c.through_hole_boxes.len()*4).sum::<usize>()
                +8+p.primitive.connection_points.len()*2+p.primitive.placements.len()*2+p.primitive.path_ports.len()*4
        };
        let pose_bytes=|p:&WorkingPrimitive|geometry_size(p)*4+(context.problem.obstacles.len()+6)*4+4;
        let budget=64*1024*1024usize;
        let fixed_bytes=fixed.iter().map(pose_bytes).sum::<usize>()+fixed_pairs*12;
        let moving_bytes=states.iter().map(|s|s.iter().map(pose_bytes).sum::<usize>()).max().unwrap_or(0)+cross_pairs*24;
        if fixed_bytes>=budget || moving_bytes>budget-fixed_bytes {
            return Err(Error::new(ErrorKind::InvalidInput,"board GPU workspace capacity guard"));
        }
        let chunk_size=requested_chunk.min(((budget-fixed_bytes)/moving_bytes.max(1)).max(1));
        for (chunk_number,chunk) in states.chunks(chunk_size).enumerate() {
            let chunk_start=chunk_number*chunk_size;
            let encode_started=Instant::now();
            if self.templates.len()>1024 || self.template_pointers.len()>4096 || self.sf.len()*4+self.si.len()*4>64*1024*1024 {
                self.frames.clear();self.sf.truncate(self.base_sf);self.si.truncate(self.base_si);self.templates.clear();self.template_pointers.clear();self.handles=None;
            }
            let mut expected_geometry=Vec::new();
            let mut geometry_floats=0usize;let mut max_geometry=0usize;
            let mut pf=Vec::new();let mut pi=Vec::new();let mut frame=vec![u(chunk.len())?,u(fixed.len())?,u(moving)?,0,u(ordinary.len())?,0];
            let mut pose_ids=FxHashMap::<(usize,Vec<(u32,u32)>),u32>::default();
            let mut encode=|item:&WorkingPrimitive|->Result<u32,Error> {
                let fallback;
                let pose=if let Some(pose)=&item.gpu_pose {pose} else {fallback=Pose::from_primitive(item);&fallback};
                if pose.translations.len()>65536 {return Err(Error::new(ErrorKind::InvalidInput,"board pose history capacity"));}
                let key=(Arc::as_ptr(&pose.template) as usize,pose.translations.iter().map(|&(x,y)|(x.to_bits(),y.to_bits())).collect());
                if let Some(&index)=pose_ids.get(&key) {return Ok(index);}
                let template=self.template(&pose.template,&context.problem)?;
                if verify {let mut cpu=item.clone();super::compact::materialize(&mut cpu);self.append_cpu_geometry(&cpu,template,&mut expected_geometry);}
                let off=u(pi.len())?;pi.extend([template,u(pf.len())?,u(pose.translations.len())?,u(geometry_floats)?]);
                let len=self.si[template as usize+16] as usize;geometry_floats+=len;max_geometry=max_geometry.max(len);
                for &(x,y) in &pose.translations {pf.extend([x,y]);}
                pose_ids.insert(key,off);Ok(off)
            };
            for p in fixed {frame.push(encode(p)?);}
            // Offsets/translation bits identify the exact immutable parent.
            
            for state in chunk {for p in state {frame.push(encode(p)?);}}
            drop(encode);
            let fixed_geometry_len=if fixed.is_empty(){0}else{let p=(fixed.len()-1)*4;pi[p+3] as usize+self.si[pi[p] as usize+16] as usize};
            let fixed_pf_len=if fixed.is_empty(){0}else{let p=(fixed.len()-1)*4;pi[p+1] as usize+pi[p+2] as usize*2};
            let frame_key:Vec<u32>=pi[..fixed.len()*4].iter().copied().chain(pf[..fixed_pf_len].iter().map(|v|v.to_bits())).collect();
            frame[5]=u(frame.len())?;
            let mut slots=vec![u32::MAX;context.problem.primitives.len()];
            for (i,p) in fixed.iter().chain(chunk[0].iter()).enumerate() {slots[p.id as usize]=u(i)?;}
            frame.extend(slots);
            frame[3]=u(frame.len())?;
            let pairs_offset=frame.len();frame.resize(frame.len()+ordinary.len()*5,0);
            for (i,pair) in ordinary.iter().enumerate() {
                let affinity=u(pf.len())?;pf.push(pair.affinity);
                let nets=u(frame.len())?;frame.resize(frame.len()+pair.nets.len()*4,0);
                for (j,(a,b)) in pair.nets.iter().enumerate() {
                    let ap=u(frame.len())?;for &point in a {frame.push(u(point)?);}
                    let bp=u(frame.len())?;for &point in b {frame.push(u(point)?);}
                    frame[nets as usize+j*4..nets as usize+j*4+4].copy_from_slice(&[ap,u(a.len())?,bp,u(b.len())?]);
                }
                frame[pairs_offset+i*5..pairs_offset+i*5+5].copy_from_slice(&[u(pair.a)?,u(pair.b)?,affinity,nets,u(pair.nets.len())?]);
            }
            floats(&pf)?;
            if pi.is_empty() {return Ok(states.iter().enumerate().map(|(index,_)|Evaluated{index,rank:Rank {hard_count:0,hard_severity:0.0,score:0.0}}).collect());}if pf.is_empty() {pf.push(0.0);}
            // Vulkan's portable per-axis dispatch limit is 65535. Materialize
            // uses one workgroup per distinct pose; other kernels use 128 lanes.
            if pi.len()/4>65535 || max_geometry.div_ceil(128)>65535
                || (fixed_pairs+chunk.len()*cross_pairs).div_ceil(128)>65535 {
                return Err(Error::new(ErrorKind::InvalidInput,"board GPU dispatch capacity guard"));
            }
            let count=chunk.len();
            let encode_ms=encode_started.elapsed().as_secs_f64()*1000.0;
            let operation_started=Instant::now();
            let (rows,all_rows,materialized,term_values,upload_ms,submit_ms,read_ms)=gpu::with_batch(REQUIREMENTS,"board-score",count.saturating_mul(context.problem.components.len().max(1)),|session| {
                if self.fail_batch==Some(self.batches+1) {panic!("injected board GPU failure at batch {}",self.batches+1);}
                let upload_started=Instant::now();
                if self.handles.is_none() {self.handles=Some((session.client.create_from_slice(f32::as_bytes(&self.sf)),
                    session.client.create_from_slice(u32::as_bytes(&self.si))));}
                let (sf,si)=self.handles.as_ref().unwrap();
                let pfh=session.client.create_from_slice(f32::as_bytes(&pf));let pih=session.client.create_from_slice(u32::as_bytes(&pi));
                let fh=session.client.create_from_slice(u32::as_bytes(&frame));
                let output=session.workspace(ScratchKey::new("board-ranks-f32",0),count*2*4);
                let parent=session.workspace(ScratchKey::new("board-ranks-f32",9),4);
                let diagnostic_tags=session.workspace(ScratchKey::new("board-ranks-f32",10),if verify {count*4}else{4});
                let term_output=session.workspace(ScratchKey::new("board-ranks-f32",8),if verify {count*10*4}else{8});
                let tags=session.workspace(ScratchKey::new("board-ranks-f32",1),count*4);
                let geometry=session.workspace(ScratchKey::new("board-ranks-f32",2),geometry_floats*4);
                let pair_count=fixed_pairs+count*cross_pairs;
                let pair_values=session.workspace(ScratchKey::new("board-ranks-f32",3),pair_count.max(1)*3*4);
                let pose_count=pi.len()/4;let unary_stride=context.problem.obstacles.len()+6;
                let unary_values=session.workspace(ScratchKey::new("board-ranks-f32",4),pose_count*unary_stride*4);
                let unary_tags=session.workspace(ScratchKey::new("board-ranks-f32",5),pose_count*4);
                let limits=selection.map(|s|(s.ordinary,s.aligned));
                let flags=selection.map(|s|s.flags[chunk_start..chunk_start+count].iter().map(|&a|u32::from(a)).collect::<Vec<_>>());
                let flag_handle=flags.as_ref().map(|flags|session.client.create_from_slice(u32::as_bytes(flags)));
                let limit=limits.map_or(0,|(a,b)|a+b);
                let winner_ids=session.workspace(ScratchKey::new("board-ranks-f32",6),limit.max(1)*2*4);
                let winner_scores=session.workspace(ScratchKey::new("board-ranks-f32",7),limit.max(1)*2*4);
                let cached=self.frames.get(&frame_key).cloned();
                let fresh=cached.is_none();
                let parent_frame=cached.unwrap_or_else(||FixedFrame {
                    geometry:session.client.empty((fixed_geometry_len*4).max(8)),
                    unary:session.client.empty((fixed.len()*unary_stride*4).max(8)),
                    tags:session.client.empty((fixed.len()*4).max(8)),
                    pairs:session.client.empty((fixed_pairs*3*4).max(8)),
                    bytes:fixed_geometry_len*4+fixed.len()*(unary_stride*4+4)+fixed_pairs*12,
                });
                let input=|| unsafe {k::InputLaunch::new(ArrayArg::from_raw_parts(sf.clone(),self.sf.len()),ArrayArg::from_raw_parts(si.clone(),self.si.len()),
                    ArrayArg::from_raw_parts(pfh.clone(),pf.len()),ArrayArg::from_raw_parts(pih.clone(),pi.len()),ArrayArg::from_raw_parts(fh.clone(),frame.len()),
                    ArrayArg::from_raw_parts(geometry.clone(),geometry_floats),ArrayArg::from_raw_parts(pair_values.clone(),pair_count.max(1)*3),
                    ArrayArg::from_raw_parts(unary_values.clone(),pose_count*unary_stride),ArrayArg::from_raw_parts(unary_tags.clone(),pose_count),
                    ArrayArg::from_raw_parts(parent_frame.geometry.clone(),fixed_geometry_len.max(1)),
                    ArrayArg::from_raw_parts(parent_frame.pairs.clone(),(fixed_pairs*3).max(1)),
                    ArrayArg::from_raw_parts(parent_frame.unary.clone(),(fixed.len()*unary_stride).max(1)),
                    ArrayArg::from_raw_parts(parent_frame.tags.clone(),fixed.len().max(1))) };
                let upload_ms=upload_started.elapsed().as_secs_f64()*1000.0;
                let submit_started=Instant::now();
                unsafe {
                    if fresh && !fixed.is_empty() {
                        k::materialize::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(fixed.len() as u32,max_geometry.div_ceil(128) as u32,1),CubeDim::new_1d(128),input(),ArrayArg::from_raw_parts(parent_frame.geometry.clone(),fixed_geometry_len),0);
                        k::unary::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(fixed.len().div_ceil(128) as u32,1,1),CubeDim::new_1d(128),input(),ArrayArg::from_raw_parts(parent_frame.unary.clone(),fixed.len()*unary_stride),ArrayArg::from_raw_parts(parent_frame.tags.clone(),fixed.len()),0,fixed.len() as u32,true);
                        if fixed_pairs>0 {k::pair_terms::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(fixed_pairs.div_ceil(128) as u32,1,1),CubeDim::new_1d(128),input(),ArrayArg::from_raw_parts(parent_frame.pairs.clone(),fixed_pairs*3),0,fixed_pairs as u32,true);}
                    }
                    if pose_count>fixed.len() {
                        k::materialize::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static((pose_count-fixed.len()) as u32,max_geometry.div_ceil(128) as u32,1),CubeDim::new_1d(128),input(),ArrayArg::from_raw_parts(geometry.clone(),geometry_floats),fixed.len() as u32);
                        k::unary::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static((pose_count-fixed.len()).div_ceil(128) as u32,1,1),CubeDim::new_1d(128),input(),ArrayArg::from_raw_parts(unary_values.clone(),pose_count*unary_stride),ArrayArg::from_raw_parts(unary_tags.clone(),pose_count),fixed.len() as u32,pose_count as u32,full_ranks);
                    }
                    if pair_count>fixed_pairs {k::pair_terms::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static((pair_count-fixed_pairs).div_ceil(128) as u32,1,1),CubeDim::new_1d(128),input(),ArrayArg::from_raw_parts(pair_values.clone(),pair_count*3),fixed_pairs as u32,pair_count as u32,full_ranks);}
                    if !full_ranks || verify {
                        k::legality_parent::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(1,1,1),CubeDim::new_1d(1),
                            input(),ArrayArg::from_raw_parts(parent.clone(),1));
                        k::legality::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(count.div_ceil(128) as u32,1,1),CubeDim::new_1d(128),
                            input(),ArrayArg::from_raw_parts(parent.clone(),1),ArrayArg::from_raw_parts(if verify {diagnostic_tags.clone()}else{tags.clone()},count));
                    }
                    if full_ranks {
                        k::ranks::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(count as u32,1,1),CubeDim::new_1d(128),
                            input(),ArrayArg::from_raw_parts(output.clone(),count*2),ArrayArg::from_raw_parts(tags.clone(),count),
                            ArrayArg::from_raw_parts(term_output.clone(),if verify {count*10}else{1}),verify);
                    }
                    if let Some((ordinary,aligned))=limits {
                        k::clear_selection::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static((limit*2).div_ceil(128) as u32,1,1),CubeDim::new_1d(128),ArrayArg::from_raw_parts(winner_ids.clone(),limit*2));
                        k::select::launch_unchecked::<WgpuRuntime>(&session.client,CubeCount::Static(count.div_ceil(128) as u32,1,1),CubeDim::new_1d(128),
                            ArrayArg::from_raw_parts(output.clone(),count*2),ArrayArg::from_raw_parts(tags.clone(),count),
                            ArrayArg::from_raw_parts(flag_handle.as_ref().unwrap().clone(),count),
                            ArrayArg::from_raw_parts(winner_ids.clone(),limit*2),ArrayArg::from_raw_parts(winner_scores.clone(),limit*2),ordinary,aligned);
                    }
                }
                let submit_ms=submit_started.elapsed().as_secs_f64()*1000.0;
                let read_started=Instant::now();
                let all_rows=if selection.is_none() || verify || verify_shortlist {
                    let values=if full_ranks {Some(session.client.read_one(output).map_err(|e|format!("board GPU score readback: {e:?}"))?)}else{None};
                    let counts=session.client.read_one(tags).map_err(|e|format!("board GPU count readback: {e:?}"))?;
                    let values=values.as_ref().map(|b|f32::from_bytes(b)).unwrap_or(&[]);let counts=u32::from_bytes(&counts);
                    Some((0..count).map(|i|Rank{hard_count:counts[i] as usize,hard_severity:if full_ranks {values[i*2]}else{0.0},score:if full_ranks {values[i*2+1]}else{0.0}}).collect::<Vec<_>>())
                }else{None};
                if verify {
                    let diagnostic=session.client.read_one(diagnostic_tags).map_err(|e|format!("board GPU diagnostic legality: {e:?}"))?;
                    let diagnostic=u32::from_bytes(&diagnostic);
                    if all_rows.as_ref().unwrap().iter().zip(diagnostic).any(|(r,&h)|r.hard_count!=h as usize) {
                        return Err("board GPU legality differs from full rank".into());
                    }
                }
                let mut rows=if selection.is_some() {
                    let mut read=cubecl::future::block_on(session.client.read_async(vec![winner_ids,winner_scores]))
                        .map_err(|e|format!("board GPU shortlist readback: {e:?}"))?;
                    let values=read.pop().unwrap();let ids=read.pop().unwrap();
                    let ids=u32::from_bytes(&ids);let values=f32::from_bytes(&values);
                    let mut seen=std::collections::HashSet::new();
                    for i in 0..limit {let index=ids[i*2];
                        if index!=u32::MAX && (index as usize>=count || !seen.insert(index)) {
                            return Err("invalid board GPU shortlist ID".into());
                        }
                    }
                    (0..limit).filter_map(|i|(ids[i*2]!=u32::MAX).then(||Evaluated{index:chunk_start+ids[i*2] as usize,
                        rank:Rank{hard_count:ids[i*2+1] as usize,hard_severity:values[i*2],score:values[i*2+1]}})).collect::<Vec<_>>()
                }else {
                    all_rows.as_ref().unwrap().iter().enumerate().map(|(i,rank)|Evaluated{index:chunk_start+i,rank:rank.clone()}).collect()
                };
                rows.sort_by_key(|r|r.index);
                if rows.iter().any(|r|!r.rank.hard_severity.is_finite() || !r.rank.score.is_finite()) {return Err("nonfinite board GPU rank".into());}
                let materialized=if verify {
                    let bytes=session.client.read_one(geometry).map_err(|e|format!("board GPU geometry readback: {e:?}"))?;
                    let mut values=f32::from_bytes(&bytes).to_vec();
                    if fixed_geometry_len>0 {let bytes=session.client.read_one(parent_frame.geometry.clone()).map_err(|e|format!("fixed geometry: {e:?}"))?;values[..fixed_geometry_len].copy_from_slice(&f32::from_bytes(&bytes)[..fixed_geometry_len]);}
                    Some(values)
                }else{None};
                let term_values=if verify {Some(session.client.read_one(term_output).map_err(|e|format!("board GPU term readback: {e:?}"))?)}else{None};
                if fresh {
                    if self.frames.len()>=16 || self.frames.values().map(|f|f.bytes).sum::<usize>()+parent_frame.bytes>64*1024*1024 {self.frames.clear();}
                    self.frames.insert(frame_key.clone(),parent_frame);self.frame_misses+=1;
                }else{self.frame_hits+=1;}
                Ok((rows,all_rows,materialized,term_values,upload_ms,submit_ms,read_started.elapsed().as_secs_f64()*1000.0))
            })?;
            let gpu_operation_ms=operation_started.elapsed().as_secs_f64()*1000.0;
            if verify {
                let actual=materialized.as_ref().unwrap();
                if actual.len()<expected_geometry.len() {return Err(Error::new(ErrorKind::InvalidInput,"short board GPU geometry readback"));}
                if let Some(index)=expected_geometry.iter().zip(actual).position(|(a,b)|a.to_bits()!=b.to_bits()) {
                    eprintln!("[board-gpu-validation] geometry {}",serde_json::json!({"index":index,"cpu":expected_geometry[index],"gpu":actual[index]}));
                    return Err(Error::new(ErrorKind::InvalidInput,"board GPU geometry validation failed"));
                }
                for (index,(state,row)) in chunk.iter().zip(all_rows.as_ref().unwrap()).enumerate() {
                    let mut all=fixed.to_vec();all.extend(state.iter().cloned());for p in &mut all {super::compact::materialize(p);}let cpu=super::state_rank(&all,context);
                    let same=crate::f32_policy::score_close;
                    let boxes=all.iter().flat_map(|p|super::packing_boxes(p).iter().copied()).collect::<Vec<_>>();
                    let b=crate::geometry::union_boxes(&boxes);let w=b.right-b.left;let h=b.bottom-b.top;
                    let primitives=all.iter().map(|p|&p.primitive).collect::<Vec<_>>();
                    let expected=[super::relation_penalty(&all,context),super::envelope_overlap_penalty(&all,context),w*h,w+h,
                        super::soft_spacing_penalty(&all,context),super::soft_alignment_score(&all,context),super::edge_bias_penalty(&all,context),
                        super::edge_place_penalty(&all,context),crate::signal_path::topology_penalty(&primitives,&context.problem.relations),
                        crate::ordinary_net::penalty(&primitives,&context.problem.relations)];
                    let actual=f32::from_bytes(term_values.as_ref().unwrap());
                    for (term,&value) in expected.iter().enumerate() {
                        if !same(value,actual[index*10+term]) {
                            if term==8 && crate::f32_policy::quantized_score_close(value,actual[index*10+term]) {continue;}
                            if term==0 {
                                let enclosure=super::relation_penalty_enclosure(&all,context);
                                if enclosure.lo.is_finite() && enclosure.hi.is_finite()
                                    && enclosure.contains(value) && enclosure.contains(actual[index*10]) {
                                    eprintln!("[board-gpu-validation] bounded relation {}",serde_json::json!({"candidate":index,"cpu":value,"gpu":actual[index*10],"lower":enclosure.lo,"upper":enclosure.hi,"reason":"propagated Vulkan division/sqrt precision; diagnostic only"}));
                                    continue;
                                }
                            }
                            eprintln!("[board-gpu-validation] term {}",serde_json::json!({"candidate":index,"term":term,"cpu":value,"gpu":actual[index*10+term]}));
                            return Err(Error::new(ErrorKind::InvalidInput,"board GPU term validation failed"));
                        }
                    }
                    // Terms have each passed their own diagnostic/error bound.
                    // Recompose with the CPU expression and exact configured
                    // weights; a sqrt difference can be amplified downstream.
                    // This verifies contributions/signs/units without applying
                    // a global tolerance to geometry or search decisions.
                    let t=&actual[index*10..index*10+10];
                    let high=context.problem.compactness.as_ref()=="high";
                    let scale=context.problem.soft_spacing.as_ref().map_or(1.0,|s|s.compactness_scale);
                    let explained_score=t[0]*if high {0.35}else{1.0}
                        +row.hard_severity*1_000_000.0
                        +t[1]*if high {1.2}else{1.0}+t[2]*if high {1.2}else{0.8}*scale
                        +t[3]*if high {8.0}else{1.5}*scale+t[4]+t[5]
                        +t[6]*if high {0.4}else{1.0}+t[7]+t[8]*if high {3.0}else{5.0};
                    if row.hard_count!=cpu.hard_count || !same(cpu.hard_severity,row.hard_severity) || !same(explained_score,row.score) {
                        eprintln!("[board-gpu-validation] {}",serde_json::json!({"batch":self.batches+1,"candidate":index,"cpu":cpu,"gpu":row}));
                        return Err(Error::new(ErrorKind::InvalidInput,"board GPU candidate validation failed"));
                    }
                }
            }
            if verify_shortlist {
                if let Some(selection)=selection {
                    let all=all_rows.as_ref().unwrap();
                    let mut indices=(0..count).collect::<Vec<_>>();
                    indices.sort_by(|&a,&b|super::compare_rank(&all[a],&all[b]).then(a.cmp(&b)));
                    let mut ordinary=0;let mut aligned=0;
                    indices.retain(|&i| {
                        let (seen,limit)=if selection.flags[chunk_start+i] {(&mut aligned,selection.aligned)}else{(&mut ordinary,selection.ordinary)};
                        *seen+=1;*seen<=limit
                    });indices.sort_unstable();
                    let actual=rows.iter().map(|r|r.index-chunk_start).collect::<Vec<_>>();
                    if actual!=indices {return Err(Error::new(ErrorKind::InvalidInput,"GPU shortlist differs from unpruned batch"));}
                    for row in &rows {let r=&all[row.index-chunk_start];
                        if row.rank.hard_count!=r.hard_count || row.rank.hard_severity.to_bits()!=r.hard_severity.to_bits() || row.rank.score.to_bits()!=r.score.to_bits() {
                            return Err(Error::new(ErrorKind::InvalidInput,"GPU shortlist rank/ID mismatch"));
                        }
                    }
                }
            }
            let stage=if self.phase=="beam" && moving==2 {"joint_pair"}else{self.phase};
            let timings=self.stages.entry(stage).or_default();timings.batches+=1;timings.candidates+=count;
            timings.encode_ms+=encode_ms;timings.upload_ms+=upload_ms;timings.submit_ms+=submit_ms;timings.read_ms+=read_ms;timings.gpu_operation_ms+=gpu_operation_ms;
            if legality_only {self.legality_candidates+=count;}if full_ranks {self.rank_candidates+=count;}
            self.batches+=1;self.candidates+=count;result.extend(rows);
        } Ok(result)
    }
}

impl Control {
    pub fn new(p:&BoardPackProblem,explicit:bool)->Result<Arc<Self>,Error> {
        let engine=Engine::new(p)?;
        let call=if p.primitives.iter().any(|p|!p.locked) {Some(gpu::enter(REQUIREMENTS,gpu::Admission::explicit(explicit))?)}else{None};
        Ok(Arc::new(Self {engine:Mutex::new(engine),failure:Mutex::new(None),wait_nanos:AtomicU64::new(0),_call:call}))
    }
    pub fn failure(&self)->Option<Error> {self.failure.lock().unwrap_or_else(|e|e.into_inner()).clone()}
    pub fn legality(&self,fixed:&[WorkingPrimitive],states:&[Vec<WorkingPrimitive>],context:&Context)->Vec<Rank> {
        self.evaluate_mode(fixed,states,context,true,None).into_iter().map(|row|row.rank).collect()
    }
    pub fn shortlist(&self,fixed:&[WorkingPrimitive],states:&[Vec<WorkingPrimitive>],flags:&[bool],ordinary:usize,aligned:usize,context:&Context)->Vec<Evaluated> {
        self.evaluate_mode(fixed,states,context,false,Some(Selection{flags,ordinary,aligned}))
    }
    fn evaluate_mode(&self,fixed:&[WorkingPrimitive],states:&[Vec<WorkingPrimitive>],context:&Context,legality_only:bool,selection:Option<Selection<'_>>)->Vec<Evaluated> {
        if self.failure().is_some() {std::panic::panic_any(Abort);}
        let waiting=Instant::now();
        let mut engine=self.engine.lock().unwrap_or_else(|e|e.into_inner());
        self.wait_nanos.fetch_add(waiting.elapsed().as_nanos() as u64,Ordering::Relaxed);
        if self.failure().is_some() {std::panic::panic_any(Abort);}
        let result=engine.evaluate(fixed,states,context,legality_only,selection);
        match result {
            Ok(rows)=>rows,
            Err(error)=>{*self.failure.lock().unwrap_or_else(|e|e.into_inner())=Some(error);std::panic::panic_any(Abort);}
        }
    }
    pub fn set_phase(&self,phase:&'static str) {self.engine.lock().unwrap_or_else(|e|e.into_inner()).phase=phase;}
    pub fn checkpoint(&self,stage:&str) {
        if std::env::var("PCB_BOARD_GPU_FAIL_AT").as_deref()!=Ok(stage) {return;}
        let failure=gpu::with_session::<()>(REQUIREMENTS,|_|panic!("injected board GPU failure at {stage}"));
        if let Err(error)=failure {*self.failure.lock().unwrap_or_else(|e|e.into_inner())=Some(error);std::panic::panic_any(Abort);}
    }
    pub fn performed_work(&self)->bool {self.engine.lock().unwrap_or_else(|e|e.into_inner()).batches>0}
    pub fn report(&self) {
        let engine=self.engine.lock().unwrap_or_else(|e|e.into_inner());
        eprintln!("[board-gpu-stage] {}",serde_json::json!({"batches":engine.batches,"candidates":engine.candidates,
            "frameHits":engine.frame_hits,"frameMisses":engine.frame_misses,"frameBytes":engine.frames.values().map(|f|f.bytes).sum::<usize>(),"templates":engine.templates.len(),"legalityCandidates":engine.legality_candidates,"rankCandidates":engine.rank_candidates,"residentBytes":engine.sf.len()*4+engine.si.len()*4,"stages":engine.stages,"engineWaitWorkerMs":self.wait_nanos.load(Ordering::Relaxed) as f64/1e6,"runtime":gpu::statistics()}));
    }
}
