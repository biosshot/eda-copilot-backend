//! Channel-neighbourhood fallback (approximation tier). When the estimated cost of meshing the
//! whole requested ROI exceeds the memory budget, the ROI is shrunk to the bounding box of the
//! channel support (port electrodes and the copper of the non-reference channel nets) plus a margin.
//! Copper is cut at the new boundary; far return paths and far coupling are lost.
//! `P7_ROI_FALLBACK=0` disables it.
use super::board_mesh::{polygon,Request};
use super::{error,AllocationBudget,Result};
use crate::geometry::{areas,PreparationReport};
use geo::{Area,BoundingRect};
use std::collections::{BTreeMap,BTreeSet};

/// Calibrated on TPS 24k nodes/0.7 GiB, BandAmpV5 60k/1.1 GiB, STM32G0 154k/3.9 GiB.
const BASE_BYTES:f64=0.2*1073741824.;
const BYTES_PER_NODE:f64=24.*1024.;
const MARGIN_LADDER_MM:[f64;4]=[20.,10.,5.,3.];

pub(super) struct Plan {pub request:Request,pub evidence:serde_json::Value}

fn z_planes(r:&Request)->usize {
    let air=r.air_margin_m;let mut d=r.interface_step_m.unwrap_or(r.max_vertical_step_m).min(r.max_vertical_step_m).min(air);
    let mut air_n=1;while d<air {d=(2.*d).min(air);air_n+=1;}
    let mut z=2*air_n+1;
    for l in &r.stack {
        z+=(l.thickness_m/r.max_vertical_step_m).ceil().max(1.) as usize;
        if let (false,Some(step)) = (l.copper,r.interface_step_m) {let mut dd=step;while dd<l.thickness_m/2. {z+=2;dd*=r.interface_growth.unwrap_or(2.);}}
    }
    z
}
fn in_rect(p:[f64;2],b:&[[f64;2];2])->bool {p[0]>=b[0][0]&&p[0]<=b[1][0]&&p[1]>=b[0][1]&&p[1]<=b[1][1]}
/// (planar vertices, nodes, bytes) for meshing `roi`.
fn estimate(g:&PreparationReport,r:&Request,roi:&[[f64;2];2])->(usize,usize,f64) {
    let mut v=0usize;
    for l in &g.layers {for p in &l.copper {for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {v+=ring.iter().filter(|q|in_rect(**q,roi)).count();}}}
    for h in &g.holes {for p in &h.polygons {for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {v+=2*ring.iter().filter(|q|in_rect(**q,roi)).count();}}}
    let area=(roi[1][0]-roi[0][0])*(roi[1][1]-roi[0][1]);
    let planar=(1.3*v as f64+area/(2.*r.max_triangle_area_m2*1e6)) as usize;
    let nodes=planar.saturating_mul(z_planes(r));
    (planar,nodes,BASE_BYTES+BYTES_PER_NODE*nodes as f64)
}

pub(super) fn plan(r:&Request,g:&PreparationReport,budget:AllocationBudget)->Result<Option<Plan>> {
    if std::env::var("P7_ROI_FALLBACK").as_deref()==Ok("0")||!r.cut_planes.is_empty() {return Ok(None);}
    let roi0=r.roi_mm;
    if let Some(p)=&r.domain_polygon_mm {
        let axis=p.len()==4&&p.iter().all(|q|(q[0]==roi0[0][0]||q[0]==roi0[1][0])&&(q[1]==roi0[0][1]||q[1]==roi0[1][1]));
        if !axis {return Ok(None);}
    }
    let budget_bytes=std::env::var("P7_ROI_BUDGET_MIB").ok().and_then(|v|v.parse::<f64>().ok()).map(|m|m*1048576.)
        .unwrap_or(if budget.max_bytes<usize::MAX/2 {budget.max_bytes as f64} else {6.*1073741824.});
    let (planar0,nodes0,bytes0)=estimate(g,r,&roi0);
    // Small meshes never need the fallback (the fixed overhead term must not refuse unit-scale fixtures).
    if bytes0<=budget_bytes||nodes0<100_000 {return Ok(None);}
    // Support: electrode source regions and the copper of the non-reference channel nets.
    let keys:BTreeSet<(String,&str)>=r.electrodes.iter().flat_map(|e|e.canonical_paths().unwrap_or_default().into_iter().map(move|p|(p,e.layer.as_str()))).collect();
    let electrode_regions:Vec<usize>=g.regions.iter().enumerate().filter(|(_,x)|keys.contains(&(x.source.path.clone(),x.layer.as_str()))).map(|(i,_)|i).collect();
    let channel_nets:BTreeSet<&str>=electrode_regions.iter().filter_map(|&i|g.regions[i].source.net.as_deref()).collect();
    let mut net_area=BTreeMap::<&str,f64>::new();
    for x in &g.regions {if let Some(n)=x.source.net.as_deref().filter(|n|channel_nets.contains(n)) {*net_area.entry(n).or_default()+=polygon(&x.polygon).unsigned_area();}}
    let reference=if channel_nets.len()>1 {net_area.iter().max_by(|a,b|a.1.total_cmp(b.1)).map(|(n,_)|*n)} else {None};
    let support:Vec<usize>=g.regions.iter().enumerate().filter(|(i,x)|electrode_regions.contains(i)||x.source.net.as_deref().is_some_and(|n|channel_nets.contains(n)&&Some(n)!=reference)).map(|(i,_)|i).collect();
    let mut bbox=[[f64::INFINITY;2],[f64::NEG_INFINITY;2]];
    for &i in &support {if let Some(b)=polygon(&g.regions[i].polygon).bounding_rect() {for k in 0..2 {bbox[0][k]=bbox[0][k].min(if k==0{b.min().x}else{b.min().y});bbox[1][k]=bbox[1][k].max(if k==0{b.max().x}else{b.max().y});}}}
    if electrode_regions.is_empty()||bbox[0][0]>bbox[1][0] {return Ok(None);}
    let stack_mm=r.stack.iter().map(|l|l.thickness_m).sum::<f64>()*1e3;
    let scale=(stack_mm/1.6).max(1.);
    let mut chosen=None;let mut last=(0usize,0usize,0.);
    for m in MARGIN_LADDER_MM {
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
        let e=estimate(g,r,&roi);last=(e.0,e.1,e.2);
        if e.2<=budget_bytes {chosen=Some((m,roi,e));break;}
    }
    let gib=|b:f64|b/1073741824.;
    let Some((margin,roi,(planar,nodes,bytes)))=chosen else {
        return Err(error("MEMORY_LIMIT",&format!("channel-neighbourhood fallback does not fit: even the smallest margin ({} mm) needs about {:.1} GiB ({} nodes), budget {:.1} GiB; the full ROI needs {:.1} GiB",MARGIN_LADDER_MM[3]*scale,gib(last.2),last.1,gib(budget_bytes),gib(bytes0))));
    };
    for &i in &electrode_regions {
        if let Some(b)=polygon(&g.regions[i].polygon).bounding_rect() {
            if !(in_rect([b.min().x,b.min().y],&roi)&&in_rect([b.max().x,b.max().y],&roi)) {
                return Err(error("MEMORY_LIMIT",&format!("channel electrode {} does not fit the fallback ROI; the full ROI needs about {:.1} GiB, budget {:.1} GiB",g.regions[i].id,gib(bytes0),gib(budget_bytes))));
            }
        }
    }
    // Copper removed by the cut: all copper, and the copper of every channel net (reference included).
    let window=areas::one(geo::Polygon::new(geo::LineString::from(vec![roi[0],[roi[1][0],roi[0][1]],roi[1],[roi[0][0],roi[1][1]]]),vec![]));
    let (mut cut_all,mut cut_channel)=(0.,0.);
    for x in &g.regions {
        let p=polygon(&x.polygon);let a=p.unsigned_area();
        let Some(b)=p.bounding_rect() else {continue};
        let outside=b.max().x<roi[0][0]||b.min().x>roi[1][0]||b.max().y<roi[0][1]||b.min().y>roi[1][1];
        let inside=in_rect([b.min().x,b.min().y],&roi)&&in_rect([b.max().x,b.max().y],&roi);
        let removed=if outside {a} else if inside {0.} else {(a-areas::intersection(&areas::one(p),&window).unsigned_area()).max(0.)};
        cut_all+=removed;
        if x.source.net.as_deref().is_some_and(|n|channel_nets.contains(n)) {cut_channel+=removed;}
    }
    let mut nr=r.clone();
    nr.roi_mm=roi;nr.allow_insulated_copper_cuts=true;
    if nr.domain_polygon_mm.is_some() {nr.domain_polygon_mm=Some(vec![roi[0],[roi[1][0],roi[0][1]],roi[1],[roi[0][0],roi[1][1]]]);}
    let evidence=serde_json::json!({"tier":"approximation","marginM":margin*1e-3,"marginMm":margin,"estimateBytes":bytes,"budgetBytes":budget_bytes,
        "copperCutArea":cut_all,"channelNetCopperCut":cut_channel,"areaUnit":"mm2","roiMmBefore":roi0,"roiMmAfter":roi,
        "estimatedNodesBefore":nodes0,"estimatedPlanarVerticesBefore":planar0,"estimatedNodes":nodes,"estimatedPlanarVertices":planar,
        "referenceNet":reference,"channelNets":channel_nets,
        "warning":"The mesh covers only the channel neighbourhood: copper beyond the ROI is cut and its channel-net pieces have open boundaries, so far return paths and far coupling are not modelled. Unverified approximation."});
    eprintln!("P7_ROI_FALLBACK {evidence}");
    Ok(Some(Plan{request:nr,evidence}))
}
