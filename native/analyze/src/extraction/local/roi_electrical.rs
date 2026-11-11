//! Electrical-size approximation tier. The quasistatic screen of `mixed_conductor::checked_reserve_policy`
//! measures the diagonal of the requested ROI, which for an automatic channel is the whole board even when the
//! channel is a few millimetres long. When the screen fails on the full ROI, the domain shrinks to the channel
//! neighbourhood: the bounding box of the port electrodes and the copper of the non-reference channel nets
//! (the support of `roi_fallback::channel_support`; the memory-driven shrink there composes with this one: it runs later on the already smaller request) plus the largest margin from
//! a ladder for which the screen passes. Copper is cut at the new boundary: far return paths and far coupling are
//! not modelled. Approximation tier with a warning; the screen itself stays a real refusal when no margin fits.
//! `P7_ROI_ELECTRICAL=0` disables the shrink.
use super::board_mesh::{polygon,Request};
use super::{error,Result};
use crate::{geometry::{self,areas,context::Context,GeometryOptions},model::Board};
use geo::{Area,BoundingRect};

const MARGIN_LADDER_MM:[f64;8]=[20.,10.,5.,3.,2.,1.,0.5,0.2];

pub(super) struct Shrunk {
    /// Request for the mesh: shrunk ROI, copper cut at its boundary allowed.
    pub mesh:Request,
    /// Request for the solver admission: shrunk ROI, original closed-copper policy.
    pub solve:Request,
    pub evidence:serde_json::Value,
}

fn in_rect(p:[f64;2],b:&[[f64;2];2])->bool {p[0]>=b[0][0]&&p[0]<=b[1][0]&&p[1]>=b[0][1]&&p[1]<=b[1][1]}

/// `fits` decides whether the screen passes for a candidate request (an `Err` from it means it does not).
pub(super) fn shrink(board:&Board,r:&Request,original_error:&str,fits:impl Fn(&Request)->bool)->Result<Option<Shrunk>> {
    if std::env::var("P7_ROI_ELECTRICAL").as_deref()==Ok("0")||!r.cut_planes.is_empty()||r.allow_insulated_copper_cuts {return Ok(None);}
    let roi0=r.roi_mm;
    if let Some(p)=&r.domain_polygon_mm {
        let axis=p.len()==4&&p.iter().all(|q|(q[0]==roi0[0][0]||q[0]==roi0[1][0])&&(q[1]==roi0[0][1]||q[1]==roi0[1][1]));
        if !axis {return Ok(None);}
    }
    let options=GeometryOptions{max_deviation_mm:r.curve_deviation_mm,..Default::default()};
    options.validate()?;
    let context=Context::for_board(board,options)?;
    let _scope=Context::install(Some(context));
    let g=geometry::prepare(board,options);
    let Some(super::roi_fallback::Support{electrode_regions,channel_nets,reference,bbox,..})=super::roi_fallback::channel_support(r,&g) else {return Ok(None);};
    let mut chosen=None;
    for m in MARGIN_LADDER_MM {
        let mut roi=[[(bbox[0][0]-m).max(roi0[0][0]),(bbox[0][1]-m).max(roi0[0][1])],[(bbox[1][0]+m).min(roi0[1][0]),(bbox[1][1]+m).min(roi0[1][1])]];
        // A via barrel must lie wholly inside the ROI: grow the box around any barrel it would cut.
        for _ in 0..8 {
            let mut grown=false;
            for h in &g.holes {
                let pts:Vec<[f64;2]>=h.polygons.iter().flat_map(|p|p.outer.iter().copied()).collect();
                if pts.is_empty() {continue;}
                let lo=[pts.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min)-0.2,pts.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)-0.2];
                let hi=[pts.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max)+0.2,pts.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)+0.2];
                let touches=lo[0]<=roi[1][0]&&hi[0]>=roi[0][0]&&lo[1]<=roi[1][1]&&hi[1]>=roi[0][1];
                if touches&&!(in_rect(lo,&roi)&&in_rect(hi,&roi)) {for k in 0..2 {roi[0][k]=roi[0][k].min(lo[k]).max(roi0[0][k]);roi[1][k]=roi[1][k].max(hi[k]).min(roi0[1][k]);}grown=true;}
            }
            if !grown {break;}
        }
        let mut solve=r.clone();
        solve.roi_mm=roi;
        if solve.domain_polygon_mm.is_some() {solve.domain_polygon_mm=Some(vec![roi[0],[roi[1][0],roi[0][1]],roi[1],[roi[0][0],roi[1][1]]]);}
        // Keep the guard box of the return-current audit strictly inside the smaller domain (a box on the domain edge is not an internal cut).
        if let Some(b)=&mut solve.return_boundary_mm {for k in 0..2 {b[0][k]=b[0][k].max(roi[0][k]+GUARD_INSET_MM);b[1][k]=b[1][k].min(roi[1][k]-GUARD_INSET_MM);}}
        if fits(&solve) {chosen=Some((m,roi,solve));break;}
    }
    let Some((margin,roi,solve))=chosen else {return Ok(None);};
    for &i in &electrode_regions {
        if let Some(b)=polygon(&g.regions[i].polygon).bounding_rect() {
            if !(in_rect([b.min().x,b.min().y],&roi)&&in_rect([b.max().x,b.max().y],&roi)) {
                return Err(error("BOARD_FOIL_BAND",&format!("channel electrode {} does not fit the electrical-size neighbourhood; {original_error}",g.regions[i].id)));
            }
        }
    }
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
    let mut mesh=solve.clone();
    mesh.allow_insulated_copper_cuts=true;
    let evidence=serde_json::json!({"tier":"approximation","reason":"electrical-size screen failed on the full ROI","screenError":original_error,
        "marginMm":margin,"roiMmBefore":roi0,"roiMmAfter":roi,"copperCutArea":cut_all,"channelNetCopperCut":cut_channel,"areaUnit":"mm2",
        "referenceNet":reference,"channelNets":channel_nets,
        "warning":"The domain is the channel neighbourhood only: copper beyond the ROI is cut and its channel-net pieces have open boundaries, so far return paths and far coupling are not modelled. Unverified approximation."});
    eprintln!("P7_ROI_ELECTRICAL {evidence}");
    Ok(Some(Shrunk{mesh,solve,evidence}))
}

/// 1 nm inset of the clamped guard box from the ROI/field-plane edge (the field cut requires a strictly internal box).
const GUARD_INSET_MM: f64 = 1e-6;
