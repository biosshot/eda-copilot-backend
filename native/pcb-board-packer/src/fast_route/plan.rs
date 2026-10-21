//! Host connection planning, shared by both execution backends. Only independent
//! path tests are batched; expression reduction preserves the original F32 order.
use super::*;
use arithmetic::{P, Path};
use rustc_hash::FxHashMap;

pub(super) enum Cost {
    Constant(f32), Job(usize), Add(Box<Cost>,Box<Cost>), Min(Box<Cost>,Box<Cost>),
    Scale(Box<Cost>,f32),
}
impl Cost {
    fn offset(&mut self, offset: usize) {
        match self {
            Self::Job(i) => *i += offset,
            Self::Add(a,b) | Self::Min(a,b) => { a.offset(offset); b.offset(offset); },
            Self::Scale(a,_) => a.offset(offset), Self::Constant(_) => {},
        }
    }
    pub fn min(self, other: Self) -> Self { Self::Min(Box::new(self),Box::new(other)) }
    fn value(&self, jobs: &[f32]) -> f32 { match self {
        Self::Constant(v)=>*v, Self::Job(i)=>jobs[*i],
        Self::Add(a,b)=>a.value(jobs)+b.value(jobs), Self::Min(a,b)=>a.value(jobs).min(b.value(jobs)),
        Self::Scale(a,b)=>a.value(jobs)*b,
    } }
}
impl std::ops::Add for Cost { type Output=Self; fn add(self,b:Self)->Self { Self::Add(Box::new(self),Box::new(b)) } }
impl std::ops::Add<f32> for Cost { type Output=Self; fn add(self,b:f32)->Self { self+Self::Constant(b) } }
impl std::ops::Mul<f32> for Cost { type Output=Self; fn mul(self,b:f32)->Self { Self::Scale(Box::new(self),b) } }
impl std::iter::Sum for Cost { fn sum<I:Iterator<Item=Self>>(i:I)->Self { i.fold(Self::Constant(0.0),|a,b|a+b) } }

#[derive(Default)]
pub(crate) struct Plan {
    pub(super) floats: Vec<f32>, pub(super) ints: Vec<u32>,
    pub(super) obstacles: Vec<f32>, pub(super) links: Vec<f32>,
    obstacle_sets: FxHashMap<Vec<u32>,(u32,u32)>, link_sets: FxHashMap<Vec<u32>,(u32,u32)>,
    roots: Vec<Cost>,
}
fn intern(data: Vec<f32>, storage: &mut Vec<f32>, sets: &mut FxHashMap<Vec<u32>,(u32,u32)>, stride:usize)->(u32,u32) {
    let key:Vec<_>=data.iter().map(|v|v.to_bits()).collect();
    *sets.entry(key).or_insert_with(|| {
        let range=((storage.len()/stride).try_into().expect("route offset"),(data.len()/stride).try_into().expect("route length"));
        storage.extend(data); range
    })
}
/// The seven candidate routes of one link.
pub(super) fn alternatives(a:Point,b:Point,clearance:f32)->Vec<Vec<Point>> {
    let middle=Point{x:(a.x+b.x)*0.5,y:(a.y+b.y)*0.5}; let gap=clearance.max(0.25)*2.0;
    let mut paths=vec![vec![a,b],vec![a,Point{x:a.x,y:b.y},b],vec![a,Point{x:b.x,y:a.y},b]];
    for d in [-gap,gap] {
        paths.push(vec![a,Point{x:middle.x+d,y:a.y},Point{x:middle.x+d,y:b.y},b]);
        paths.push(vec![a,Point{x:a.x,y:middle.y+d},Point{x:b.x,y:middle.y+d},b]);
    }
    paths
}
/// Route length beyond the direct segment.
pub(super) fn excess(path:&[Point],a:Point,b:Point)->f32 {
    (path.windows(2).map(|s|distance(s[0],s[1])).sum::<f32>()-distance(a,b)).max(0.0)
}
impl Plan {
    /// One root per layout, one dispatch for the whole candidate batch.
    pub(crate) fn append(&mut self, mut other: Self) {
        let jobs=self.ints.len()/35;
        for root in &mut other.roots { root.offset(jobs); }
        let obstacles=(self.obstacles.len()/4) as u32;
        let links=(self.links.len()/5) as u32;
        for n in other.ints.chunks_exact_mut(5) { n[1]+=obstacles; n[3]+=links; }
        self.floats.extend(other.floats); self.ints.extend(other.ints);
        self.obstacles.extend(other.obstacles); self.links.extend(other.links); self.roots.extend(other.roots);
    }
    pub(super) fn reduce_many(&self,paths:&[f32])->Vec<f32> {
        let jobs:Vec<_>=paths.chunks_exact(7).map(|p|p.iter().copied().reduce(f32::min).unwrap()).collect();
        self.roots.iter().map(|c|c.value(&jobs)).collect()
    }
    // Approximate segment/obstacle work, including the clearance-band test.
    // Used only for dispatch admission, never for scores or candidate pruning.
    pub(super) fn work(&self)->usize {
        self.ints.chunks_exact(5).map(|n| (n[0] as usize-1)*(2*n[2] as usize+n[4] as usize)).sum()
    }
    pub(super) fn root(&mut self,cost:Cost) { self.roots.push(cost); }
    pub(super) fn path(&mut self,a:Point,b:Point,net:&str,obstacles:&[RouteObstacle],skeleton:&[(Arc<str>,Segment)],clearance:f32,layer:Option<&str>,explicit:&[crate::model::ExplicitConnection])->Cost {
        let class=explicit_class(net,a,b,explicit);
        let boxes=obstacles.iter().filter(|o| !(layer.is_some() && o.layer.is_some() && layer!=o.layer.as_deref()) && o.net.as_deref()!=Some(net))
            .flat_map(|o|[o.box_.left,o.box_.right,o.box_.top,o.box_.bottom]).collect();
        let edges=skeleton.iter().filter(|(n,_)|n.as_ref()!=net).flat_map(|(n,(c,d))|
            [c.x,c.y,d.x,d.y,crate::relation_score::crossing_factor(class,explicit_class(n,*c,*d,explicit))]).collect();
        let (oi,on)=intern(boxes,&mut self.obstacles,&mut self.obstacle_sets,4);
        let (li,ln)=intern(edges,&mut self.links,&mut self.link_sets,5);
        let job=self.ints.len()/35;
        for path in alternatives(a,b,clearance) {
            let excess=excess(&path,a,b);
            for i in 0..4 { let p=path[i.min(path.len()-1)]; self.floats.extend([p.x,p.y]); }
            self.floats.extend([excess,clearance,crate::relation_score::pad_factor(class)]);
            self.ints.extend([path.len() as u32,oi,on,li,ln]);
        }
        Cost::Job(job)
    }
    pub(super) fn reduce(&self,paths:&[f32])->f32 {
        let jobs:Vec<_>=paths.chunks_exact(7).map(|p|p.iter().copied().reduce(f32::min).unwrap()).collect();
        // Scalar callers sum roots; batched callers keep one result per layout.
        self.roots.iter().map(|c|c.value(&jobs)).reduce(|a,b|a+b).unwrap_or(0.0)
    }
    pub(crate) fn cpu(&self)->f32 { self.reduce(&self.cpu_paths()) }
    pub(super) fn cpu_paths(&self)->Vec<f32> {
        self.ints.chunks_exact(5).enumerate().map(|(i,n)| {
            let f=&self.floats[i*11..];
            let p=Path{a:P{x:f[0],y:f[1]},b:P{x:f[2],y:f[3]},c:P{x:f[4],y:f[5]},d:P{x:f[6],y:f[7]},count:n[0]};
            let mut cost=f[8];
            for j in n[1]..n[1]+n[2] { let o=&self.obstacles[j as usize*4..]; cost+=arithmetic::obstacle(p,o[0],o[1],o[2],o[3],f[9],f[10]); }
            for j in n[3]..n[3]+n[4] { let l=&self.links[j as usize*5..]; cost+=arithmetic::crossing(p,P{x:l[0],y:l[1]},P{x:l[2],y:l[3]},l[4]); }
            cost
        }).collect()
    }
}
