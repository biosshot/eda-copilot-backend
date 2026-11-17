//! Channel-neighbourhood fallback (approximation tier). When the estimated cost of meshing the
//! whole requested ROI exceeds the memory budget, the ROI is shrunk to the bounding box of the
//! channel support (port electrodes and the copper of the non-reference channel nets) plus a margin.
//! Copper is cut at the new boundary; far return paths and far coupling are lost.
//! When even the smallest bbox margin does not fit (the signal net spans the board), the ROI becomes a
//! corridor: a union of capsules of radius m around the shortest copper path between the port
//! electrodes (coarse raster Dijkstra over the net's copper, vias as links) plus margin boxes around the
//! electrodes. The corridor polygon is the mesh domain (`domainPolygonMm`), so copper outside it is cut.
//! `P7_ROI_FALLBACK=0` disables the fallback, `P7_ROI_CORRIDOR=0` only the corridor step,
//! `P7_ROI_FORCE_CORRIDOR=1` skips the bbox ladder (diagnostics).
use super::board_mesh::{polygon,Request};
use super::{copper_skin_profile,error,AllocationBudget,Result};
use crate::geometry::{areas,PreparationReport};
use geo::{Area,BoundingRect,Contains,Intersects};
use std::cmp::Reverse;
use std::collections::{BTreeMap,BTreeSet,BinaryHeap,HashMap,HashSet};

/// Calibrated on TPS 24k nodes/0.7 GiB, BandAmpV5 60k/1.1 GiB, STM32G0 154k/3.9 GiB.
const BASE_BYTES:f64=0.2*1073741824.;
const BYTES_PER_NODE:f64=24.*1024.;
const MARGIN_LADDER_MM:[f64;4]=[20.,10.,5.,3.];
/// The corridor ladder continues below the bbox ladder: its area is already small, so tighter radii are still meaningful.
const CORRIDOR_LADDER_MM:[f64;6]=[20.,10.,5.,3.,2.,1.5];

pub(super) struct Plan {pub request:Request,pub evidence:serde_json::Value}

/// With a skin profile every copper slab is split to a third of the skin depth (and the profile's current/field steps), so the plane count grows with frequency.
fn z_planes(r:&Request,skin:Option<copper_skin_profile::Profile>)->usize {
    let air=r.air_margin_m;let mut d=r.interface_step_m.unwrap_or(r.max_vertical_step_m).min(r.max_vertical_step_m).min(air);
    let mut air_n=1;while d<air {d=(2.*d).min(air);air_n+=1;}
    let mut z=2*air_n+1;
    for l in &r.stack {
        let skin_pieces=skin.and_then(|p|p.pieces(l.thickness_m,l.conductivity_s_m.filter(|_|l.copper),r.max_vertical_step_m));
        z+=skin_pieces.unwrap_or_else(||(l.thickness_m/r.max_vertical_step_m).ceil().max(1.) as usize);
        if let (false,Some(step)) = (l.copper,r.interface_step_m) {let mut dd=step;while dd<l.thickness_m/2. {z+=2;dd*=r.interface_growth.unwrap_or(2.);}}
    }
    z
}
fn in_rect(p:[f64;2],b:&[[f64;2];2])->bool {p[0]>=b[0][0]&&p[0]<=b[1][0]&&p[1]>=b[0][1]&&p[1]<=b[1][1]}
/// (planar vertices, nodes, bytes) for meshing `roi`.
fn estimate(g:&PreparationReport,r:&Request,roi:&[[f64;2];2],skin:Option<copper_skin_profile::Profile>)->(usize,usize,f64) {estimate_in(g,r,roi,None,skin)}
/// Same, for a corridor polygon whose bounding box is `roi`.
fn estimate_in(g:&PreparationReport,r:&Request,roi:&[[f64;2];2],poly:Option<&geo::Polygon<f64>>,skin:Option<copper_skin_profile::Profile>)->(usize,usize,f64) {
    let inside=|q:[f64;2]|in_rect(q,roi)&&poly.is_none_or(|p|p.contains(&geo::Point::new(q[0],q[1])));
    let mut v=0usize;
    for l in &g.layers {for p in &l.copper {for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {v+=ring.iter().filter(|q|inside(**q)).count();}}}
    for h in &g.holes {for p in &h.polygons {for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {v+=2*ring.iter().filter(|q|inside(**q)).count();}}}
    let area=poly.map_or((roi[1][0]-roi[0][0])*(roi[1][1]-roi[0][1]),|p|p.unsigned_area());
    let planar=(1.3*v as f64+area/(2.*r.max_triangle_area_m2*1e6)) as usize;
    let nodes=planar.saturating_mul(z_planes(r,skin));
    (planar,nodes,BASE_BYTES+BYTES_PER_NODE*nodes as f64)
}


// ---- Corridor ---------------------------------------------------------------------------------------
const ARC:usize=10;
const VIA_LINK_MM:f64=0.05;

/// Coarse raster over a mm rectangle; one boolean plane per copper layer.
struct Grid {x0:f64,y0:f64,h:f64,nx:usize,ny:usize}
impl Grid {
    fn cell(&self,p:[f64;2])->Option<(usize,usize)> {
        let (i,j)=(((p[0]-self.x0)/self.h).floor(),((p[1]-self.y0)/self.h).floor());
        (i>=0.&&j>=0.&&(i as usize)<self.nx&&(j as usize)<self.ny).then(||(i as usize,j as usize))
    }
    fn centre(&self,n:usize)->[f64;2] {let c=n%(self.nx*self.ny);[self.x0+((c%self.nx) as f64+0.5)*self.h,self.y0+((c/self.nx) as f64+0.5)*self.h]}
    fn plane(&self)->usize {self.nx*self.ny}
}
/// Marks the cells touched by the polygon boundary and its interior (even-odd scanline).
fn raster_poly(grid:&Grid,p:&geo::Polygon<f64>,mark:&mut impl FnMut(usize)) {
    let rings:Vec<&geo::LineString<f64>>=std::iter::once(p.exterior()).chain(p.interiors()).collect();
    for ring in &rings {for l in ring.lines() {
        let len=(l.end.x-l.start.x).hypot(l.end.y-l.start.y);let n=(len/(grid.h*0.5)).ceil().max(1.) as usize;
        for s in 0..=n {let t=s as f64/n as f64;if let Some((i,j))=grid.cell([l.start.x+(l.end.x-l.start.x)*t,l.start.y+(l.end.y-l.start.y)*t]) {mark(j*grid.nx+i);}}
    }}
    let Some(b)=p.bounding_rect() else {return};
    let j0=((b.min().y-grid.y0)/grid.h).floor().max(0.) as usize;let j1=(((b.max().y-grid.y0)/grid.h).ceil().max(0.) as usize).min(grid.ny.saturating_sub(1));
    let mut xs:Vec<f64>=vec![];
    for j in j0..=j1 {
        let yc=grid.y0+(j as f64+0.5)*grid.h;xs.clear();
        for ring in &rings {for l in ring.lines() {if (l.start.y<=yc)!=(l.end.y<=yc) {xs.push(l.start.x+(yc-l.start.y)/(l.end.y-l.start.y)*(l.end.x-l.start.x));}}}
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            let a=((pair[0]-grid.x0)/grid.h-0.5).ceil().max(0.) as usize;let bq=((pair[1]-grid.x0)/grid.h-0.5).floor();
            if bq<0. {continue;}
            for i in a..=(bq as usize).min(grid.nx-1) {mark(j*grid.nx+i);}
        }
    }
}
/// One copper polygon of the channel net on layer index `layer`.
pub(super) struct Piece {pub layer:usize,pub poly:geo::Polygon<f64>}
/// A port electrode: its net, layer and polygons.
pub(super) struct Port {pub net:String,pub layer:usize,pub polys:Vec<geo::Polygon<f64>>}
/// A via barrel: centre and the layer indices it joins.
pub(super) struct Via {pub at:[f64;2],pub layers:Vec<usize>}
pub(super) struct Paths {pub lines:Vec<Vec<[f64;2]>>,pub length_mm:f64,pub unconnected:usize}

fn douglas_peucker(pts:&[[f64;2]],tol:f64)->Vec<[f64;2]> {
    if pts.len()<3 {return pts.to_vec();}
    let (a,b)=(pts[0],pts[pts.len()-1]);let (dx,dy)=(b[0]-a[0],b[1]-a[1]);let len=dx.hypot(dy);
    let (mut worst,mut wi)=(0.,0);
    for (i,p) in pts.iter().enumerate().take(pts.len()-1).skip(1) {
        let d=if len==0. {(p[0]-a[0]).hypot(p[1]-a[1])} else {((p[0]-a[0])*dy-(p[1]-a[1])*dx).abs()/len};
        if d>worst {worst=d;wi=i;}
    }
    if worst<=tol {return vec![a,b];}
    let mut l=douglas_peucker(&pts[..=wi],tol);let r=douglas_peucker(&pts[wi..],tol);l.pop();l.extend(r);l
}

/// Shortest conductive paths between every pair of same-net ports over `pieces` (one net per call).
/// Raster Dijkstra, 8-connected per layer, vias as near-zero-cost links. Port pairs the raster does not
/// connect are counted in `unconnected` and left to the margin boxes.
pub(super) fn net_paths(pieces:&[Piece],ports:&[&Port],vias:&[Via],n_layers:usize,h_hint:f64)->Paths {
    let mut out=Paths{lines:vec![],length_mm:0.,unconnected:0};
    if ports.len()<2||pieces.is_empty() {return out;}
    let mut bb=[[f64::INFINITY;2],[f64::NEG_INFINITY;2]];
    for p in pieces {if let Some(b)=p.poly.bounding_rect() {bb[0][0]=bb[0][0].min(b.min().x);bb[0][1]=bb[0][1].min(b.min().y);bb[1][0]=bb[1][0].max(b.max().x);bb[1][1]=bb[1][1].max(b.max().y);}}
    let ext=(bb[1][0]-bb[0][0]).max(bb[1][1]-bb[0][1]);
    let h=h_hint.max(ext/900.);
    let grid=Grid{x0:bb[0][0]-2.*h,y0:bb[0][1]-2.*h,h,nx:((bb[1][0]-bb[0][0])/h).ceil() as usize+5,ny:((bb[1][1]-bb[0][1])/h).ceil() as usize+5};
    let plane=grid.plane();let total=plane*n_layers;
    let mut occ=vec![false;total];
    for p in pieces {raster_poly(&grid,&p.poly,&mut |c|occ[p.layer*plane+c]=true);}
    let mut links:HashMap<usize,Vec<usize>>=HashMap::new();
    for v in vias {
        let Some((ci,cj))=grid.cell(v.at) else {continue};
        let mut nodes=vec![];
        for &l in &v.layers {'f:for d in 0..=1i64 {for dj in -d..=d {for di in -d..=d {
            let (i,j)=(ci as i64+di,cj as i64+dj);
            if i<0||j<0||i as usize>=grid.nx||j as usize>=grid.ny {continue;}
            let n=l*plane+j as usize*grid.nx+i as usize;if occ[n] {nodes.push(n);break 'f;}
        }}}}
        for &a in &nodes {for &b in &nodes {if a!=b {links.entry(a).or_default().push(b);}}}
    }
    let port_nodes:Vec<Vec<usize>>=ports.iter().map(|p|{
        let mut set=BTreeSet::new();
        for poly in &p.polys {raster_poly(&grid,poly,&mut |c|{if occ[p.layer*plane+c] {set.insert(p.layer*plane+c);}});}
        set.into_iter().collect()
    }).collect();
    let mut edges:HashSet<(usize,usize)>=HashSet::new();
    let diag=h*std::f64::consts::SQRT_2;
    for a in 0..ports.len().min(16) {
        if port_nodes[a].is_empty() {continue;}
        let mut dist=vec![f64::INFINITY;total];let mut parent=vec![u32::MAX;total];
        let mut heap=BinaryHeap::new();
        for &n in &port_nodes[a] {dist[n]=0.;heap.push(Reverse((0f64.to_bits(),n as u32)));}
        let mut pending:HashSet<usize>=(a+1..ports.len()).flat_map(|b|port_nodes[b].iter().copied()).collect();
        while let Some(Reverse((db,n)))=heap.pop() {
            let n=n as usize;let d=f64::from_bits(db);if d>dist[n] {continue;}
            pending.remove(&n);if pending.is_empty() {break;}
            let (l,c)=(n/plane,n%plane);let (i,j)=((c%grid.nx) as i64,(c/grid.nx) as i64);
            let mut relax=|m:usize,w:f64,heap:&mut BinaryHeap<Reverse<(u64,u32)>>|{if d+w<dist[m] {dist[m]=d+w;parent[m]=n as u32;heap.push(Reverse(((d+w).to_bits(),m as u32)));}};
            for dj in -1..=1i64 {for di in -1..=1i64 {
                if di==0&&dj==0 {continue;}
                let (ni,nj)=(i+di,j+dj);
                if ni<0||nj<0||ni as usize>=grid.nx||nj as usize>=grid.ny {continue;}
                let m=l*plane+nj as usize*grid.nx+ni as usize;
                if occ[m] {relax(m,if di!=0&&dj!=0 {diag} else {h},&mut heap);}
            }}
            if let Some(ls)=links.get(&n) {for &m in ls {relax(m,VIA_LINK_MM,&mut heap);}}
        }
        for b in a+1..ports.len() {
            let Some(&t)=port_nodes[b].iter().filter(|&&n|dist[n].is_finite()).min_by(|x,y|dist[**x].total_cmp(&dist[**y])) else {out.unconnected+=1;continue};
            let mut line=vec![grid.centre(t)];let mut cur=t;
            while parent[cur]!=u32::MAX {
                let p=parent[cur] as usize;edges.insert((cur.min(p),cur.max(p)));
                cur=p;line.push(grid.centre(cur));
            }
            out.lines.push(line);
        }
    }
    out.length_mm=edges.iter().map(|&(a,b)|{let (pa,pb)=(grid.centre(a),grid.centre(b));(pa[0]-pb[0]).hypot(pa[1]-pb[1])}).sum();
    out
}

fn capsule(a:[f64;2],b:[f64;2],m:f64)->geo::Polygon<f64> {
    use std::f64::consts::{PI,FRAC_PI_2};
    let mut pts=vec![];
    if a==b {
        for k in 0..2*ARC {let t=2.*PI*k as f64/(2*ARC) as f64;pts.push([a[0]+m*t.cos(),a[1]+m*t.sin()]);}
    } else {
        let th=(b[1]-a[1]).atan2(b[0]-a[0]);
        for k in 0..=ARC {let t=th-FRAC_PI_2+PI*k as f64/ARC as f64;pts.push([b[0]+m*t.cos(),b[1]+m*t.sin()]);}
        for k in 0..=ARC {let t=th+FRAC_PI_2+PI*k as f64/ARC as f64;pts.push([a[0]+m*t.cos(),a[1]+m*t.sin()]);}
    }
    geo::Polygon::new(geo::LineString::from(pts),vec![])
}
fn rect_poly(b:[[f64;2];2])->geo::Polygon<f64> {geo::Polygon::new(geo::LineString::from(vec![b[0],[b[1][0],b[0][1]],b[1],[b[0][0],b[1][1]]]),vec![])}
fn grow(b:[[f64;2];2],m:f64)->[[f64;2];2] {[[b[0][0]-m,b[0][1]-m],[b[1][0]+m,b[1][1]+m]]}

/// Corridor domain: capsules of radius `m` around `lines`, margin boxes around `boxes` (grown by `m`) and the
/// raw `keep` boxes, as one simple hole-free polygon (holes filled, separate pieces bridged).
pub(super) fn corridor_polygon(lines:&[Vec<[f64;2]>],boxes:&[[[f64;2];2]],keep:&[[[f64;2];2]],m:f64)->Option<geo::Polygon<f64>> {
    let mut items:Vec<geo::Polygon<f64>>=vec![];
    for l in lines {
        let s=douglas_peucker(l,m*0.25);
        if s.len()==1 {items.push(capsule(s[0],s[0],m));}
        for w in s.windows(2) {items.push(capsule(w[0],w[1],m));}
    }
    items.extend(boxes.iter().map(|b|rect_poly(grow(*b,m))));
    items.extend(keep.iter().map(|b|rect_poly(*b)));
    if items.is_empty() {return None;}
    let mut set=areas::union(items.clone());
    for _ in 0..16 {
        if set.0.len()<=1 {break;}
        let a:Vec<[f64;2]>=set.0[0].exterior().coords().map(|c|[c.x,c.y]).collect();
        let mut best=(f64::INFINITY,[0.;2],[0.;2]);
        for q in &set.0[1..] {for c in q.exterior().coords() {for p in &a {let d=(p[0]-c.x).hypot(p[1]-c.y);if d<best.0 {best=(d,*p,[c.x,c.y]);}}}}
        items.push(capsule(best.1,best.2,m*0.5));
        set=areas::union(items.clone());
    }
    if set.0.len()!=1 {return None;}
    Some(geo::Polygon::new(set.0[0].exterior().clone(),vec![]))
}

/// `forced` = (mode, margin mm) rebuilds with exactly that rung, skipping the budget test (recalibration).
/// Channel support: electrode regions, the copper of the non-reference channel nets and its bounding box (shared with roi_electrical).
pub(super) struct Support<'a> {pub electrode_regions:Vec<usize>,pub channel_nets:BTreeSet<&'a str>,pub net_area:BTreeMap<&'a str,f64>,pub reference:Option<&'a str>,pub support:Vec<usize>,pub bbox:[[f64;2];2]}
pub(super) fn channel_support<'a>(r:&Request,g:&'a PreparationReport)->Option<Support<'a>> {
    let keys:BTreeSet<(String,&str)>=r.electrodes.iter().flat_map(|e|e.canonical_paths().unwrap_or_default().into_iter().map(move|p|(p,e.layer.as_str()))).collect();
    let electrode_regions:Vec<usize>=g.regions.iter().enumerate().filter(|(_,x)|keys.contains(&(x.source.path.clone(),x.layer.as_str()))).map(|(i,_)|i).collect();
    let channel_nets:BTreeSet<&str>=electrode_regions.iter().filter_map(|&i|g.regions[i].source.net.as_deref()).collect();
    let mut net_area=BTreeMap::<&str,f64>::new();
    for x in &g.regions {if let Some(n)=x.source.net.as_deref().filter(|n|channel_nets.contains(n)) {*net_area.entry(n).or_default()+=polygon(&x.polygon).unsigned_area();}}
    let reference=if channel_nets.len()>1 {net_area.iter().max_by(|a,b|a.1.total_cmp(b.1)).map(|(n,_)|*n)} else {None};
    let support:Vec<usize>=g.regions.iter().enumerate().filter(|(i,x)|electrode_regions.contains(i)||x.source.net.as_deref().is_some_and(|n|channel_nets.contains(n)&&Some(n)!=reference)).map(|(i,_)|i).collect();
    let mut bbox=[[f64::INFINITY;2],[f64::NEG_INFINITY;2]];
    for &i in &support {if let Some(b)=polygon(&g.regions[i].polygon).bounding_rect() {for k in 0..2 {bbox[0][k]=bbox[0][k].min(if k==0{b.min().x}else{b.min().y});bbox[1][k]=bbox[1][k].max(if k==0{b.max().x}else{b.max().y});}}}
    if electrode_regions.is_empty()||bbox[0][0]>bbox[1][0] {return None;}
    Some(Support{electrode_regions,channel_nets,net_area,reference,support,bbox})
}

thread_local!{static REQUEST_MEMORY:std::cell::Cell<Option<f64>>=const{std::cell::Cell::new(None)};}
fn request_memory_bytes()->Option<f64> { REQUEST_MEMORY.with(|c|c.get()) }
/// The request memory limit (`budget.maxMemoryMiB`) for the ROI memory fit of the calling thread, restored on drop.
/// The checked endpoint keeps its unbounded allocation budget (an external supervisor owns hard limits); only the
/// ROI size decision reads this limit. `P7_ROI_BUDGET_MIB` still overrides it.
pub(super) struct RequestMemory(Option<f64>);
impl RequestMemory {
    /// Any finite positive request budget moves the ROI fit (the DSL always sends its resolved budget: the request's
    /// `.budget()` or the machine-derived cap), including values at or above 2048 MiB.
    pub(super) fn set(mib:Option<f64>)->Self {
        let bytes=mib.filter(|m|m.is_finite()&&*m>0.).map(|m|m*1048576.);
        Self(REQUEST_MEMORY.with(|c|c.replace(bytes)))
    }
}
/// The request memory limit in whole MiB, if the calling thread carries one.
pub(super) fn request_memory_mib()->Option<f64> { request_memory_bytes().map(|b|(b/1048576.).floor()) }
impl Drop for RequestMemory { fn drop(&mut self) { REQUEST_MEMORY.with(|c|c.set(self.0)); } }
pub(super) fn plan(r:&Request,g:&PreparationReport,budget:AllocationBudget,forced:Option<(&str,f64)>,skin:Option<copper_skin_profile::Profile>)->Result<Option<Plan>> {
    if std::env::var("P7_ROI_FALLBACK").as_deref()==Ok("0")||!r.cut_planes.is_empty() {return Ok(None);}
    let roi0=r.roi_mm;
    if let Some(p)=&r.domain_polygon_mm {
        let axis=p.len()==4&&p.iter().all(|q|(q[0]==roi0[0][0]||q[0]==roi0[1][0])&&(q[1]==roi0[0][1]||q[1]==roi0[1][1]));
        if !axis {return Ok(None);}
    }
    let budget_bytes=std::env::var("P7_ROI_BUDGET_MIB").ok().and_then(|v|v.parse::<f64>().ok()).map(|m|m*1048576.)
        .or_else(request_memory_bytes)
        .unwrap_or(if budget.max_bytes<usize::MAX/2 {budget.max_bytes as f64} else {6.*1073741824.});
    let fit_bytes=if forced.is_some() {f64::INFINITY} else {budget_bytes};
    let (planar0,nodes0,bytes0)=estimate(g,r,&roi0,skin);
    if skin.is_some() {eprintln!("P7_ROI_ESTIMATE planar={planar0} nodes={nodes0} bytes={bytes0:.0} budgetBytes={budget_bytes:.0} z={}",z_planes(r,skin));}
    // Small meshes never need the fallback (the fixed overhead term must not refuse unit-scale fixtures).
    if bytes0<=budget_bytes||nodes0<100_000 {return Ok(None);}
    if r.refuse_roi_fallback {return Err(error("MEMORY_LIMIT",&format!("the whole-copper domain needs about {:.1} GiB ({nodes0} nodes), budget {:.1} GiB; copper may not be cut",bytes0/1073741824.,budget_bytes/1073741824.)));}
    // Support: electrode source regions and the copper of the non-reference channel nets.
    let Some(Support{electrode_regions,channel_nets,net_area,reference,support,bbox})=channel_support(r,g) else {return Ok(None);};
    let stack_mm=r.stack.iter().map(|l|l.thickness_m).sum::<f64>()*1e3;
    let scale=(stack_mm/1.6).max(1.);
    {   // What makes the support large: per channel net and the biggest support regions.
        let bb=|i:usize|polygon(&g.regions[i].polygon).bounding_rect().map(|b|[[b.min().x,b.min().y],[b.max().x,b.max().y]]);
        let nets:Vec<_>=channel_nets.iter().map(|n|{let ids:Vec<usize>=g.regions.iter().enumerate().filter(|(_,x)|x.source.net.as_deref()==Some(*n)).map(|(i,_)|i).collect();
            let mut b=[[f64::INFINITY;2],[f64::NEG_INFINITY;2]];for &i in &ids {if let Some(q)=bb(i) {for k in 0..2 {b[0][k]=b[0][k].min(q[0][k]);b[1][k]=b[1][k].max(q[1][k]);}}}
            serde_json::json!({"net":n,"regions":ids.len(),"areaMm2":net_area.get(n),"bboxMm":b,"reference":Some(*n)==reference})}).collect();
        let mut big:Vec<usize>=support.clone();big.sort_by(|a,b|polygon(&g.regions[*b].polygon).unsigned_area().total_cmp(&polygon(&g.regions[*a].polygon).unsigned_area()));
        let big:Vec<_>=big.iter().take(8).map(|&i|serde_json::json!({"id":g.regions[i].id,"net":g.regions[i].source.net,"layer":g.regions[i].layer,"areaMm2":polygon(&g.regions[i].polygon).unsigned_area(),"bboxMm":bb(i)})).collect();
        let ports:Vec<_>=r.electrodes.iter().map(|e|serde_json::json!({"name":e.name,"layer":e.layer,"paths":e.canonical_paths().ok(),"upper":e.upper})).collect();
        eprintln!("P7_ROI_SUPPORT {}",serde_json::json!({"supportRegions":support.len(),"supportBboxMm":bbox,"roiMm":roi0,"channelNets":nets,"biggestSupportRegions":big,"ports":ports}));
    }
    let force=std::env::var("P7_ROI_FORCE_CORRIDOR").as_deref()==Ok("1");
    let mut tried:Vec<[f64;2]>=vec![];
    let mut chosen:Option<(f64,[[f64;2];2],(usize,usize,f64),Option<Corridor>)>=None;let mut last=(0usize,0usize,0.);
    for m in MARGIN_LADDER_MM.iter().copied().filter(|m|!force&&forced.is_none_or(|(md,v)|md=="bbox"&&(m*scale-v).abs()<1e-6)) {
        let m=m*scale;
        let mut roi=[[(bbox[0][0]-m).max(roi0[0][0]),(bbox[0][1]-m).max(roi0[0][1])],[(bbox[1][0]+m).min(roi0[1][0]),(bbox[1][1]+m).min(roi0[1][1])]];
        // A via barrel must lie wholly inside the ROI: grow the box around any barrel it would cut.
        for _ in 0..8 {
            let mut grown=false;
            for h in &g.holes {
                let pts:Vec<[f64;2]>=h.polygons.iter().flat_map(|p|p.outer.iter().copied()).collect();
                if pts.is_empty() {continue;}
                let hb=[[pts.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min)-0.2,pts.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)-0.2],[pts.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max)+0.2,pts.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)+0.2]];
                let touches=hb[0][0]<=roi[1][0]&&hb[1][0]>=roi[0][0]&&hb[0][1]<=roi[1][1]&&hb[1][1]>=roi[0][1];
                let inside=in_rect(hb[0],&roi)&&in_rect(hb[1],&roi);
                if touches&&!inside {for k in 0..2 {roi[0][k]=roi[0][k].min(hb[0][k]).max(roi0[0][k]);roi[1][k]=roi[1][k].max(hb[1][k]).min(roi0[1][k]);}grown=true;}
            }
            if !grown {break;}
        }
        let e=estimate(g,r,&roi,skin);last=(e.0,e.1,e.2);
        if e.2<=fit_bytes {chosen=Some((m,roi,e,None));break;}
        tried.push([m,e.2]);
    }
    let gib=|b:f64|b/1073741824.;
    let bbox_last=last;
    let mut corridor_last=None;
    if chosen.is_none()&&std::env::var("P7_ROI_CORRIDOR").as_deref()!=Ok("0") {
        tried.clear();
        let (found,l,t)=corridor_plan(g,r,&electrode_regions,&roi0,scale,fit_bytes,forced.filter(|f|f.0=="corridor").map(|f|f.1),skin);
        tried=t;
        corridor_last=l;
        if let Some(c)=found {chosen=Some((c.margin_mm,c.bbox,c.estimate,Some(c)));}
    }
    let Some((margin,roi,(planar,nodes,bytes),corridor))=chosen else {
        let c=corridor_last.map_or(String::new(),|(m,n,b)|format!("; the corridor at the smallest margin ({m} mm) needs about {:.1} GiB ({n} nodes)",gib(b)));
        if force&&bbox_last.1==0 {return Err(error("MEMORY_LIMIT",&format!("corridor fallback does not fit (budget {:.1} GiB){c}",gib(budget_bytes))));}
        return Err(error("MEMORY_LIMIT",&format!("channel-neighbourhood fallback does not fit: even the smallest margin ({} mm) needs about {:.1} GiB ({} nodes), budget {:.1} GiB{c}; the full ROI needs {:.1} GiB",MARGIN_LADDER_MM[3]*scale,gib(last.2),last.1,gib(budget_bytes),gib(bytes0))));
    };
    for &i in &electrode_regions {
        if let Some(c)=&corridor {
            let p=polygon(&g.regions[i].polygon);
            if areas::difference(&areas::one(p),&areas::one(c.polygon.clone())).unsigned_area()>1e-6 {
                return Err(error("MEMORY_LIMIT",&format!("channel electrode {} does not fit the corridor ROI; the full ROI needs about {:.1} GiB, budget {:.1} GiB",g.regions[i].id,gib(bytes0),gib(budget_bytes))));
            }
        } else if let Some(b)=polygon(&g.regions[i].polygon).bounding_rect() {
            if !(in_rect([b.min().x,b.min().y],&roi)&&in_rect([b.max().x,b.max().y],&roi)) {
                return Err(error("MEMORY_LIMIT",&format!("channel electrode {} does not fit the fallback ROI; the full ROI needs about {:.1} GiB, budget {:.1} GiB",g.regions[i].id,gib(bytes0),gib(budget_bytes))));
            }
        }
    }
    // Copper removed by the cut: all copper, the copper of every channel net (reference included) and of the signal nets.
    let window=areas::one(corridor.as_ref().map_or_else(||geo::Polygon::new(geo::LineString::from(vec![roi[0],[roi[1][0],roi[0][1]],roi[1],[roi[0][0],roi[1][1]]]),vec![]),|c|c.polygon.clone()));
    let (mut cut_all,mut cut_channel,mut cut_signal)=(0.,0.,0.);
    for x in &g.regions {
        let p=polygon(&x.polygon);let a=p.unsigned_area();
        let Some(b)=p.bounding_rect() else {continue};
        let outside=b.max().x<roi[0][0]||b.min().x>roi[1][0]||b.max().y<roi[0][1]||b.min().y>roi[1][1];
        let inside=corridor.is_none()&&in_rect([b.min().x,b.min().y],&roi)&&in_rect([b.max().x,b.max().y],&roi);
        let removed=if outside {a} else if inside {0.} else {(a-areas::intersection(&areas::one(p),&window).unsigned_area()).max(0.)};
        cut_all+=removed;
        if let Some(n)=x.source.net.as_deref().filter(|n|channel_nets.contains(n)) {cut_channel+=removed;if Some(n)!=reference {cut_signal+=removed;}}
    }
    let mut nr=r.clone();
    nr.roi_mm=roi;nr.allow_insulated_copper_cuts=true;
    if let Some(c)=&corridor {
        let mut pts:Vec<[f64;2]>=c.polygon.exterior().coords().map(|q|[q.x,q.y]).collect();
        if pts.first()==pts.last() {pts.pop();}
        // The guard box of the return-current audit must stay strictly inside the shrunken field domain.
        if let Some(b)=&mut nr.return_boundary_mm {
            let (lo,hi)=(pts.iter().fold([f64::INFINITY;2],|a,q|[a[0].min(q[0]),a[1].min(q[1])]),pts.iter().fold([f64::NEG_INFINITY;2],|a,q|[a[0].max(q[0]),a[1].max(q[1])]));
            for k in 0..2 {b[0][k]=b[0][k].max(lo[k].max(roi[0][k])+1e-6);b[1][k]=b[1][k].min(hi[k].min(roi[1][k])-1e-6);}
        }
        nr.domain_polygon_mm=Some(pts);
    } else {
        if let Some(b)=&mut nr.return_boundary_mm {for k in 0..2 {b[0][k]=b[0][k].max(roi[0][k]+1e-6);b[1][k]=b[1][k].min(roi[1][k]-1e-6);}}
        if nr.domain_polygon_mm.is_some() {nr.domain_polygon_mm=Some(vec![roi[0],[roi[1][0],roi[0][1]],roi[1],[roi[0][0],roi[1][1]]]);}
    }
    let warning=if corridor.is_some() {"The mesh covers only a corridor along the current copper path between the port electrodes: all copper outside it is cut, including far parts of the signal net itself and of the reference plane, and the cut pieces have open boundaries. Lost physics: far return paths, far coupling, and stubs/branches of the signal net. Unverified approximation."}
        else {"The mesh covers only the channel neighbourhood: copper beyond the ROI is cut and its channel-net pieces have open boundaries, so far return paths and far coupling are not modelled. Unverified approximation."};
    let evidence=serde_json::json!({"tier":"approximation","mode":if corridor.is_some() {"corridor"} else {"bbox"},"marginM":margin*1e-3,"marginMm":margin,"estimateBytes":bytes,"budgetBytes":budget_bytes,
        "copperCutArea":cut_all,"channelNetCopperCut":cut_channel,"signalNetCopperCut":cut_signal,"areaUnit":"mm2","roiMmBefore":roi0,"roiMmAfter":roi,
        "pathLengthM":corridor.as_ref().map(|c|c.path_length_mm*1e-3),"corridorAreaM2":corridor.as_ref().map(|c|c.polygon.unsigned_area()*1e-6),
        "corridorUnconnectedPortPairs":corridor.as_ref().map(|c|c.unconnected),
        "bboxLadderLastEstimateBytes":bbox_last.2,"largerRungsBytes":tried,
        "estimatedNodesBefore":nodes0,"estimatedPlanarVerticesBefore":planar0,"estimatedNodes":nodes,"estimatedPlanarVertices":planar,
        "referenceNet":reference,"channelNets":channel_nets,"warning":warning});
    eprintln!("P7_ROI_FALLBACK {evidence}");
    Ok(Some(Plan{request:nr,evidence}))
}

struct Corridor {margin_mm:f64,bbox:[[f64;2];2],polygon:geo::Polygon<f64>,estimate:(usize,usize,f64),path_length_mm:f64,unconnected:usize}

/// Corridor ladder. Returns the first margin that fits, and the (margin mm, nodes, bytes) of the last attempt.
fn corridor_plan(g:&PreparationReport,r:&Request,electrode_regions:&[usize],roi0:&[[f64;2];2],scale:f64,budget_bytes:f64,forced:Option<f64>,skin:Option<copper_skin_profile::Profile>)->(Option<Corridor>,Option<(f64,usize,f64)>,Vec<[f64;2]>) {
    let layer_ix:BTreeMap<&str,usize>=g.layers.iter().enumerate().map(|(i,l)|(l.layer.as_str(),i)).collect();
    let mut ports:Vec<Port>=vec![];
    for e in &r.electrodes {
        let paths=e.canonical_paths().unwrap_or_default();
        let ids:Vec<usize>=electrode_regions.iter().copied().filter(|&i|g.regions[i].layer==e.layer&&paths.contains(&g.regions[i].source.path)).collect();
        let Some(net)=ids.iter().find_map(|&i|g.regions[i].source.net.clone()) else {continue};
        let Some(&layer)=layer_ix.get(e.layer.as_str()) else {continue};
        ports.push(Port{net,layer,polys:ids.iter().map(|&i|polygon(&g.regions[i].polygon)).collect()});
    }
    let mut lines=vec![];let mut length=0.;let mut unconnected=0;
    for net in ports.iter().map(|p|p.net.clone()).collect::<BTreeSet<_>>() {
        let np:Vec<&Port>=ports.iter().filter(|p|p.net==net).collect();
        let pieces:Vec<Piece>=g.regions.iter().filter(|x|x.source.net.as_deref()==Some(net.as_str())).filter_map(|x|{
            let p=polygon(&x.polygon);let b=p.bounding_rect()?;
            (b.max().x>=roi0[0][0]&&b.min().x<=roi0[1][0]&&b.max().y>=roi0[0][1]&&b.min().y<=roi0[1][1]).then_some(())?;
            Some(Piece{layer:*layer_ix.get(x.layer.as_str())?,poly:p})}).collect();
        let vias:Vec<Via>=g.holes.iter().filter(|h|h.plated&&h.source.net.as_deref().is_none_or(|n|n==net)).filter_map(|h|{
            let pts:Vec<[f64;2]>=h.polygons.iter().flat_map(|p|p.outer.iter().copied()).collect();
            if pts.is_empty() {return None;}
            let c=[(pts.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min)+pts.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max))/2.,(pts.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)+pts.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max))/2.];
            Some(Via{at:c,layers:h.layers.iter().filter_map(|l|layer_ix.get(l.as_str()).copied()).collect()})}).collect();
        let pr=net_paths(&pieces,&np,&vias,g.layers.len(),0.1);
        length+=pr.length_mm;unconnected+=pr.unconnected;lines.extend(pr.lines);
    }
    let boxes:Vec<[[f64;2];2]>=ports.iter().flat_map(|p|p.polys.iter()).filter_map(|p|p.bounding_rect()).map(|b|[[b.min().x,b.min().y],[b.max().x,b.max().y]]).collect();
    let hole_boxes:Vec<[[f64;2];2]>=g.holes.iter().filter_map(|h|{
        let pts:Vec<[f64;2]>=h.polygons.iter().flat_map(|p|p.outer.iter().copied()).collect();
        if pts.is_empty() {return None;}
        Some([[pts.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min)-0.2,pts.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)-0.2],[pts.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max)+0.2,pts.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)+0.2]])}).collect();
    let mut last=None;
    let mut tried=vec![];
    for m in CORRIDOR_LADDER_MM {
        let m=m*scale;
        if forced.is_some_and(|f|(m-f).abs()>1e-6) {continue;}
        // A via barrel must lie wholly inside the corridor: add its box for every barrel the corridor would cut.
        let mut keep:Vec<[[f64;2];2]>=vec![];
        let mut poly=None;
        for _ in 0..8 {
            let Some(p)=corridor_polygon(&lines,&boxes,&keep,m) else {break};
            let mut grown=false;
            for hb in &hole_boxes {
                if p.intersects(&rect_poly(*hb))&&areas::difference(&areas::one(rect_poly(*hb)),&areas::one(p.clone())).unsigned_area()>1e-9 {keep.push(*hb);grown=true;}
            }
            poly=Some(p);if !grown {break;}
        }
        let Some(mut p)=poly else {continue};
        // Stay inside the requested ROI.
        if let Some(b)=p.bounding_rect() {
            if b.min().x<roi0[0][0]||b.min().y<roi0[0][1]||b.max().x>roi0[1][0]||b.max().y>roi0[1][1] {
                let c=areas::intersection(&areas::one(p.clone()),&areas::one(rect_poly(*roi0)));
                if let Some(big)=c.0.into_iter().max_by(|a,b|a.unsigned_area().total_cmp(&b.unsigned_area())) {p=geo::Polygon::new(big.exterior().clone(),vec![]);}
            }
        }
        let Some(b)=p.bounding_rect() else {continue};
        let bbox=[[b.min().x,b.min().y],[b.max().x,b.max().y]];
        let e=estimate_in(g,r,&bbox,Some(&p),skin);last=Some((m,e.1,e.2));
        eprintln!("P7_ROI_CORRIDOR_TRY margin_mm={m} area_mm2={:.1} bbox={bbox:?} planar={} nodes={} z={} bytes={:.0}",p.unsigned_area(),e.0,e.1,z_planes(r,skin),e.2);
        if e.2<=budget_bytes {return (Some(Corridor{margin_mm:m,bbox,polygon:p,estimate:e,path_length_mm:length,unconnected}),last,tried);}
        tried.push([m,e.2]);
    }
    (None,last,tried)
}

const SAFETY:f64=1.1;
/// After the first build: (forced rung, estimateCorrection evidence) when a larger rung fits the corrected estimate.
pub(super) fn recalibrate(ev:&serde_json::Value,built_nodes:usize,build_seconds:f64,budget:AllocationBudget)->Option<((String,f64),serde_json::Value)> {
    if std::env::var("P7_ROI_RECALIBRATE").as_deref()==Ok("0")||build_seconds>120. {return None;}
    let est=ev["estimatedNodes"].as_f64()?;let first=ev["marginMm"].as_f64()?;let mode=ev["mode"].as_str()?;
    let ratio=built_nodes as f64/est;
    let budget_bytes=ev["budgetBytes"].as_f64().unwrap_or(budget.max_bytes as f64);
    // Rungs tried before the chosen one are larger; the first that fits the corrected estimate wins.
    let pick=ev["largerRungsBytes"].as_array()?.iter().filter_map(|v|Some((v[0].as_f64()?,v[1].as_f64()?))).find(|(_,b)|BASE_BYTES+(b-BASE_BYTES)*ratio*SAFETY<=budget_bytes)?;
    Some(((mode.to_string(),pick.0),serde_json::json!({"ratio":ratio,"firstMargin":first,"finalMargin":pick.0,"rebuilt":true,"builtNodesFirst":built_nodes,"estimatedNodesFirst":est})))
}
pub(super) fn not_rebuilt(ev:&serde_json::Value,built_nodes:usize)->serde_json::Value {
    let est=ev["estimatedNodes"].as_f64().unwrap_or(0.);
    serde_json::json!({"ratio":if est>0. {built_nodes as f64/est} else {f64::NAN},"firstMargin":ev["marginMm"],"finalMargin":ev["marginMm"],"rebuilt":false})
}


// ---- Honest post-build estimate ---------------------------------------------------------------------
/// Peak private memory of a whole run versus the counts of the built mesh (GiB). Calibration points, from
/// debugging/p7-v1-stage2 (peaks are lower bounds when the run was stopped early):
/// NanoScope 198540 nodes / 317205 tets / 6081 prisms -> >= 8.78 (resource-failure in the magnetic stage),
/// BandAmpV5 60492 / 113787 / 7730 -> >= 10.24 (resource-failure), STM32G0_CAN_RS485 154092 / 198780 / 6966 -> >= 3.77 (timeout),
/// TPS63060DSCR 54336 / 77412 / 2499 -> 0.6 to 1.0 (diagnostic-complete or timeout). The model is the larger of a tet
/// term and a barrel-term (barrel copper tetrahedra, about 7 per thin-wall prism: NanoScope 42843 tets for 6081 prisms, ThunderF722 23199 for 3485),
/// plus a fixed base; it stays at or above the observed peak for NanoScope (10.4 vs >= 8.78) and ThunderF722 (5.8 vs 5.18 at the charge stage).
pub(super) const HONEST_BASE_GIB:f64=0.5;
pub(super) const HONEST_GIB_PER_TET:f64=2.8e-5;
pub(super) const HONEST_GIB_PER_BARREL_TET:f64=2.3e-4;
pub(super) fn honest_estimate(nodes:usize,tets:usize,planar_triangles:usize,barrel_tets:usize)->(f64,serde_json::Value) {
    let tet_term=HONEST_GIB_PER_TET*tets as f64;
    let barrel_term=HONEST_GIB_PER_BARREL_TET*barrel_tets as f64;
    let gib=HONEST_BASE_GIB+tet_term.max(barrel_term);
    (gib*1073741824.,serde_json::json!({"model":"base + max(gibPerTet*tets, gibPerBarrelTet*barrelTets)","baseGiB":HONEST_BASE_GIB,"gibPerTet":HONEST_GIB_PER_TET,"gibPerBarrelTet":HONEST_GIB_PER_BARREL_TET,
        "nodes":nodes,"copperTetrahedra":tets,"planarTriangles":planar_triangles,"barrelTetrahedra":barrel_tets,"tetTermGiB":tet_term,"barrelTermGiB":barrel_term,"estimateGiB":gib,
        "calibration":[{"board":"NanoScope","nodes":198540,"tets":317205,"prisms":6081,"observedPeakGiB":8.78,"note":"lower bound, stopped by the memory cap"},
            {"board":"BandAmpV5","nodes":60492,"tets":113787,"prisms":7730,"observedPeakGiB":10.24,"note":"lower bound, stopped by the memory cap"},
            {"board":"STM32G0_CAN_RS485","nodes":154092,"tets":198780,"prisms":6966,"observedPeakGiB":3.77,"note":"timeout, lower bound"},
            {"board":"TPS63060DSCR","nodes":54336,"tets":77412,"prisms":2499,"observedPeakGiB":1.04,"note":"timeout, lower bound"}]}))
}
/// The next smaller rung below the current ROI (bbox margins, then corridor radii; margins in mm as in the evidence).
pub(super) fn next_smaller(r:&Request,mode:&str,margin_mm:f64)->Option<(String,f64)> {
    let stack_mm=r.stack.iter().map(|l|l.thickness_m).sum::<f64>()*1e3;
    let scale=(stack_mm/1.6).max(1.);
    let seq:Vec<(&str,f64)>=MARGIN_LADDER_MM.iter().map(|m|("bbox",m*scale)).chain(CORRIDOR_LADDER_MM.iter().map(|m|("corridor",m*scale))).collect();
    let pos=seq.iter().position(|(md,m)|*md==mode&&(m-margin_mm).abs()<1e-6)?;
    // From the smallest bbox rung the corridor starts at the 3 mm radius (larger radii cover more than the bbox).
    let next=if mode=="bbox"&&pos+1==MARGIN_LADDER_MM.len() {seq.iter().position(|(md,m)|*md=="corridor"&&(m-3.*scale).abs()<1e-6)?} else {pos+1};
    seq.get(next).map(|(md,m)|(md.to_string(),*m))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_budget_of_6144_mib_reaches_the_roi_fit() {
        let _g=RequestMemory::set(Some(6144.));
        assert_eq!(request_memory_mib(),Some(6144.));
        assert_eq!(request_memory_bytes(),Some(6144.*1048576.));
        drop(_g);
        assert_eq!(request_memory_bytes(),None);
    }
    fn rc(x0:f64,y0:f64,x1:f64,y1:f64)->geo::Polygon<f64> {rect_poly([[x0,y0],[x1,y1]])}
    #[test]
    fn corridor_follows_the_l_trace_and_drops_a_stub_pour() {
        // L trace (0.2 mm wide): 20 mm along x at y=0, then 20 mm up at x=20. Pads at both ends.
        // A 30 x 30 mm pour of the same net hangs below the horizontal arm on a 1 mm neck.
        let pour=rc(-5.,-32.,25.,-2.);
        let pieces=vec![
            Piece{layer:0,poly:rc(0.,-0.1,20.1,0.1)},Piece{layer:0,poly:rc(19.9,0.,20.1,20.)},
            Piece{layer:0,poly:rc(8.,-2.,9.,-0.1)},Piece{layer:0,poly:pour.clone()},
        ];
        let a=Port{net:"S".into(),layer:0,polys:vec![rc(0.,-0.5,1.,0.5)]};
        let b=Port{net:"S".into(),layer:0,polys:vec![rc(19.5,19.,20.5,20.)]};
        let p=net_paths(&pieces,&[&a,&b],&[],1,0.1);
        assert_eq!(p.unconnected,0);
        assert!((p.length_mm-39.).abs()<2.,"path length {}",p.length_mm);
        let boxes=[[[0.,-0.5],[1.,0.5]],[[19.5,19.],[20.5,20.]]];
        let c=corridor_polygon(&p.lines,&boxes,&[],2.).expect("corridor");
        let kept=areas::intersection(&areas::one(c.clone()),&areas::one(pour.clone())).unsigned_area();
        assert!(kept<0.05*pour.unsigned_area(),"corridor keeps {kept} of the stub pour");
        assert!(c.unsigned_area()<0.25*(30.*52.),"corridor area {}",c.unsigned_area());
        for q in [[0.5,0.],[10.,0.],[20.,10.],[20.,19.5]] {assert!(c.contains(&geo::Point::new(q[0],q[1])),"{q:?}");}
    }
}
