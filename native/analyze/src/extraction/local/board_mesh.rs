//! Board-owned, shared-overlay-grid material arrangement for volume extraction.
//! Every intersecting source is retained, including floating neighbouring copper.
use super::{copper_skin_profile,error,mesh::{self,LayeredInput,LayeredMesh,Material},AllocationBudget,Result};
use crate::{geometry::{self,areas,context::Context,GeometryOptions,PreparationReport},model::Board};
use geo::{algorithm::{coordinate_position::CoordPos,dimensions::Dimensions},Area,BoundingRect,Contains,Intersects,LineString,MultiPolygon,Point,Polygon,Relate};
use spade::{AngleLimit,ConstrainedDelaunayTriangulation,Point2,RefinementParameters,Triangulation};
use std::collections::BTreeMap;
use std::time::{Duration,Instant};

#[derive(Clone,Debug,PartialEq,serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Layer {
    pub name:String,
    pub thickness_m:f64,
    /// Copper layers have a board layer name. Dielectric layers require epsilon.
    pub copper:bool,
    pub epsilon_r:Option<f64>,
    pub conductivity_s_m:Option<f64>,
}
#[derive(Clone,Debug,serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Request {
    pub roi_mm:[[f64;2];2],
    /// Numerical local-coordinate datum, independent of domain expansion.
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub coordinate_origin_mm:Option<[f64;2]>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub domain_polygon_mm:Option<Vec<[f64;2]>>,
    pub stack:Vec<Layer>,
    /// Explicit finished-hole convention; unknown bore conventions are refused.
    pub finished_hole_plating_m:Option<f64>,
    pub barrel_conductivity_s_m:Option<f64>,
    pub air_margin_m:f64,
    /// Optional exterior boundary for the independently admitted electric field.
    /// The current/magnetic ROI and its air mesh remain unchanged.
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub electric_air_margin_m:Option<f64>,
    #[serde(default)]
    pub air_edge_divisions:Option<usize>,
    /// Field-only air refinement. Far rings can remain coarse independently.
    #[serde(default)]
    pub air_far_edge_divisions:Option<usize>,
    #[serde(default)]
    pub air_first_step_m:Option<f64>,
    #[serde(default)]
    pub air_near_distance_m:Option<f64>,
    /// None preserves the old grid; Some(0) adds the bore centre, then rings.
    #[serde(default)]
    pub bore_refinement_rings:Option<usize>,
    pub max_triangle_area_m2:f64,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub electric_max_edge_m:Option<f64>,
    pub max_vertical_step_m:f64,
    /// Optional geometric z grading into dielectric/air from every metal face.
    #[serde(default)]
    pub interface_step_m:Option<f64>,
    /// Dielectric-only growth; external air retains its independent dyadic grid.
    #[serde(default)]
    pub interface_growth:Option<f64>,
    #[serde(default)]
    pub min_triangle_angle_deg:Option<f64>,
    pub curve_deviation_mm:f64,
    #[serde(default)]
    pub accept_unknown_fill_freshness:bool,
    #[serde(default)]
    pub allow_insulated_copper_cuts:bool,
    #[serde(default)]
    pub electrodes:Vec<Electrode>,
    #[serde(default,skip_serializing_if="BTreeMap::is_empty")]
    pub cut_planes:BTreeMap<String,CutPlane>,
    #[serde(default)]
    pub ports:Vec<[String;2]>,
}
/// Geometry-bound coefficient identity excludes only independent field/current
/// discretization controls. It retains every authored physical assumption.
pub fn physical_signature(r:&Request)->serde_json::Value {
    let mut value=serde_json::json!({"roiMm":r.roi_mm,"stack":r.stack,"finishedHolePlatingM":r.finished_hole_plating_m,"barrelConductivitySM":r.barrel_conductivity_s_m,"curveDeviationMm":r.curve_deviation_mm,"electrodes":r.electrodes,"ports":r.ports,"acceptUnknownFillFreshness":r.accept_unknown_fill_freshness,"allowInsulatedCopperCuts":r.allow_insulated_copper_cuts});
    value["electrodes"]=serde_json::Value::Array(r.electrodes.iter().map(|e|serde_json::json!({"name":e.name,"sourcePaths":e.canonical_paths().unwrap_or_default(),"layer":e.layer,"upper":e.upper})).collect());
    if let Some(p)=&r.domain_polygon_mm {value["domainPolygonMm"]=serde_json::json!(p);}
    if !r.cut_planes.is_empty(){value["cutPlanes"]=serde_json::json!(r.cut_planes);}
    value
}
/// Internal physical section/local interface. Tangent is (-ny,nx).
#[derive(Clone,Debug,PartialEq,serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
#[serde(untagged,rename_all_fields="camelCase")]
pub enum CutPlane {
    Axis {axis:usize,at_mm:f64,#[serde(default,skip_serializing_if="Option::is_none")]span_mm:Option<[f64;2]>},
    Oriented {origin_mm:[f64;2],#[serde(rename="normalXY")]normal_xy:[f64;2],#[serde(default,skip_serializing_if="Option::is_none")]span_mm:Option<[f64;2]>},
}
impl CutPlane {
    fn frame(&self)->Result<([f64;2],[f64;2],Option<[f64;2]>)> {
        let (o,n,span)=match *self {
            Self::Axis{axis,at_mm,span_mm}=>{if axis>1{return Err(error("BOARD_CUT","axis must be0 or1"));}let mut o=[0.;2];o[axis]=at_mm;let mut n=[0.;2];n[axis]=1.;
                // Legacy transverse coordinate increases with the other axis.
                if axis==1 {n[axis]=-1.;} (o,n,span_mm)},
            Self::Oriented{origin_mm,normal_xy,span_mm}=>(origin_mm,normal_xy,span_mm),
        };
        let norm=n[0].hypot(n[1]);
        if o.iter().chain(n.iter()).any(|v|!v.is_finite())||!norm.is_finite()||(norm-1.).abs()>1e-8||span.is_some_and(|s|s.iter().any(|v|!v.is_finite())||s[0]>=s[1]) {return Err(error("BOARD_CUT","finite origin, unit normal and increasing finite span required"));}
        Ok((o,[n[0]/norm,n[1]/norm],span))
    }
}
fn signed(p:[f64;2],o:[f64;2],n:[f64;2])->f64 {(p[0]-o[0])*n[0]+(p[1]-o[1])*n[1]}
/// Match only the requested physical domain edge; tolerance is the existing
/// board overlay error bound, never a connectivity/welding distance.
fn cut_edges(c:&CutPlane,roi:&MultiPolygon<f64>,tolerance:f64)->Result<Vec<[[f64;2];2]>> {
    let(o,n,_)=c.frame()?;let mut edges=vec![];
    for p in &roi.0 {for e in p.exterior().lines(){let a=[e.start.x,e.start.y];let b=[e.end.x,e.end.y];if signed(a,o,n).abs()<=tolerance&&signed(b,o,n).abs()<=tolerance {edges.push([a,b]);}}}
    if edges.is_empty(){return Err(error("BOARD_CUT","interface must coincide with a physical polygon domain boundary"));}Ok(edges)
}
fn on_edges(p:[f64;2],edges:&[[[f64;2];2]],tol:f64)->bool {
    edges.iter().any(|[a,b]|{let d=[b[0]-a[0],b[1]-a[1]];let length=d[0].hypot(d[1]);let t=((p[0]-a[0])*d[0]+(p[1]-a[1])*d[1])/length;let cross=((p[0]-a[0])*d[1]-(p[1]-a[1])*d[0])/length;cross.abs()<=tol&&t>=-tol&&t<=length+tol})
}

#[derive(Clone,Debug,PartialEq,serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Electrode {
    pub name:String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub source_path:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub source_paths:Option<Vec<String>>,
    pub layer:String,
    /// Lower or upper face of the physical copper layer, in the stack z axis.
    pub upper:bool,
}
impl Electrode {
    pub fn canonical_paths(&self)->Result<Vec<String>> {
        let mut paths=match (&self.source_path,&self.source_paths) {
            (Some(p),None)=>vec![p.clone()],(None,Some(p))=>p.clone(),
            _=>return Err(error("BOARD_ELECTRODE","exactly one sourcePath or sourcePaths required")),
        };
        if paths.is_empty()||paths.iter().any(|p|p.trim().is_empty()){return Err(error("BOARD_ELECTRODE","nonempty source identities required"));}
        paths.sort();paths.dedup();Ok(paths)
    }
    fn same_contact(&self,other:&Self)->bool {self.name==other.name&&self.layer==other.layer&&self.upper==other.upper&&self.canonical_paths().ok()==other.canonical_paths().ok()}
}
fn face_intervals(poly:&MultiPolygon<f64>,edges:&[[[f64;2];2]],origin:[f64;2],normal:[f64;2],tol:f64)->Vec<[f64;2]> {
    let tangent=[-normal[1],normal[0]];
    let mut spans=poly.0.iter().flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).flat_map(|r|r.lines()).filter_map(|e|{
        let a=[e.start.x,e.start.y];let b=[e.end.x,e.end.y];
        if !on_edges(a,edges,tol)||!on_edges(b,edges,tol){return None;}
        let a=signed(a,origin,tangent);let b=signed(b,origin,tangent);(a!=b).then_some([a.min(b),a.max(b)])
    }).collect::<Vec<_>>();
    spans.sort_by(|a,b|a[0].total_cmp(&b[0]));let mut union:Vec<[f64;2]>=vec![];
    for span in spans {if let Some(last)=union.last_mut(){if span[0]<=last[1]{last[1]=last[1].max(span[1]);continue;}}union.push(span);}union
}
pub struct BoardMesh {
    pub(super) copper_skin_layers:Vec<CopperSkinLayer>,
    pub mesh:LayeredMesh,
    pub planar_nodes_mm:Vec<[f64;2]>,
    pub planar_triangles:Vec<[usize;3]>,
    pub conductivity_s_m:Vec<f64>,
    pub geometry:PreparationReport,
    pub origin_mm:[f64;2],
    pub source_mapping:Vec<(String,String,Option<String>,String)>,
    pub barrel_ids:Vec<String>,
    pub elapsed_seconds:f64,
    pub layer_spans:BTreeMap<String,[f64;2]>,
    pub electrode_areas_m2:BTreeMap<String,f64>,
    pub arrangement_atoms:usize,
    pub max_copper_area_change:f64,
    pub max_contact_area_change:f64,
    pub source_copper_restorations:Vec<serde_json::Value>,
    certified_electrodes:BTreeMap<String,(Electrode,Vec<usize>)>,
}
struct Shape {poly:MultiPolygon<f64>,id:String,sigma:f64,span:[f64;2]}
struct Atom {poly:Polygon<f64>,membership:Vec<bool>}
fn common_partition(roi:&MultiPolygon<f64>,cuts:&[MultiPolygon<f64>],budget:AllocationBudget,start:Instant,limit:Duration)->Result<Vec<Atom>> {
    let mut atoms=roi.0.iter().cloned().map(|poly|Atom{poly,membership:vec![]}).collect::<Vec<_>>();
    for cut in cuts {
        let mut next=vec![];
        for atom in &atoms {
            check(start,limit)?;
            let vertices=std::iter::once(&atom.poly).chain(cut.0.iter()).map(|p|std::iter::once(p.exterior()).chain(p.interiors()).map(|r|r.0.len()).sum::<usize>()).sum::<usize>();
            budget.check(vertices.checked_mul(4096).and_then(|v|v.checked_add((atoms.len()+next.len()).checked_mul(4096)?)).ok_or_else(||error("MEMORY_LIMIT","common arrangement plan overflow"))?)?;
            let cell=areas::one(atom.poly.clone());
            for (set,inside) in [(areas::intersection(&cell,cut),true),(areas::difference(&cell,cut),false)] {
                for poly in set.0 {
                    // Native overlay has no positive-area cutoff. Every output
                    // polygon is retained, including disconnected/floating Cu.
                    if poly.unsigned_area()<=0. {return Err(error("BOARD_ARRANGEMENT","nonpositive native partition polygon"));}
                    let mut membership=atom.membership.clone();membership.push(inside);next.push(Atom{poly,membership});
                    if next.len()>budget.max_nodes {return Err(error("MEMORY_LIMIT","common arrangement polygon budget"));}
                }
            }
        }atoms=next;
    }Ok(atoms)
}
fn polygon(p:&geometry::PolygonSnapshot)->Polygon<f64> {
    Polygon::new(LineString::from(p.outer.clone()),p.holes.iter().map(|r|LineString::from(r.clone())).collect())
}
fn rect(b:[[f64;2];2])->Polygon<f64> {Polygon::new(LineString::from(vec![b[0],[b[1][0],b[0][1]],b[1],[b[0][0],b[1][1]]]),vec![])}
fn check(start:Instant,limit:Duration)->Result<()> {if start.elapsed()>limit {Err(error("DEADLINE","board material arrangement deadline"))}else{Ok(())}}
fn barrel_outer(bore:&MultiPolygon<f64>,center:[f64;2],radius:f64,plating:f64,tolerance:f64)->Result<Polygon<f64>> {
    if bore.0.len()!=1||!bore.0[0].interiors().is_empty(){return Err(error("BOARD_PADSTACK","ordinary round bore must have one hole-free contour"));}
    let mut points=Vec::new();let ring=&bore.0[0].exterior().0;
    for point in &ring[..ring.len()-1] {let dx=point.x-center[0];let dy=point.y-center[1];let r=dx.hypot(dy);if !r.is_finite()||r<=0.||(r-radius).abs()>tolerance{return Err(error("BOARD_PADSTACK","prepared bore exceeds ordinary round curve bound"));}points.push([point.x+plating*dx/r,point.y+plating*dy/r]);}
    let outer=Polygon::new(LineString::from(points),vec![]);let expected=radius+plating;
    for edge in outer.exterior().lines(){let a=[edge.start.x-center[0],edge.start.y-center[1]];let b=[edge.end.x-center[0],edge.end.y-center[1]];let d=[b[0]-a[0],b[1]-a[1]];let square=d[0]*d[0]+d[1]*d[1];if square==0.{return Err(error("BOARD_PADSTACK","zero barrel contour edge"));}let t=(-(a[0]*d[0]+a[1]*d[1])/square).clamp(0.,1.);let closest=(a[0]+t*d[0]).hypot(a[1]+t*d[1]);if (a[0].hypot(a[1])-expected).abs()>tolerance||(closest-expected).abs()>tolerance{return Err(error("BOARD_PADSTACK","coherent outer barrel exceeds the unchanged curve bound; refine geometry"));}}
    Ok(outer)
}
pub fn build(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration)->Result<BoardMesh> {
    build_supported(board,r,budget,max_wall,None)
}
pub(super) fn build_supported(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration,support:Option<(&[[f64;2]],&[[usize;3]])>)->Result<BoardMesh> {
    build_conforming(board,r,budget,max_wall,support,&[])
}
/// Internal trace-line constraints in absolute millimetres. These subdivide
/// the common arrangement only; they add no contact, material or cut.
#[doc(hidden)]
pub fn build_conforming(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration,support:Option<(&[[f64;2]],&[[usize;3]])>,trace_lines:&[[[f64;2];2]])->Result<BoardMesh> {
    build_profiled(board,r,budget,max_wall,support,trace_lines,None)
}

#[derive(Debug,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(super) struct CopperSkinLayer {
    layer:String,
    skin_depth_m:f64,
    maximum_step_m:f64,
    slabs:usize,
}

pub(super) fn build_profiled(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration,
    support:Option<(&[[f64;2]],&[[usize;3]])>,trace_lines:&[[[f64;2];2]],skin:Option<copper_skin_profile::Profile>)->Result<BoardMesh> {
    let start=Instant::now();
    if r.electric_max_edge_m.is_some_and(|v|!v.is_finite()||v<=0.||v>0.1){return Err(error("BOARD_PROFILE","positive finite electric refinement spacing required"));}
    if r.electric_air_margin_m.is_some_and(|v|!v.is_finite()||v<=0.){return Err(error("BOARD_PROFILE","positive finite electric air margin required"));}
    if r.interface_step_m.is_some_and(|s|!s.is_finite()||s<=0.) {return Err(error("BOARD_PROFILE","positive finite interface step required"));}
    if r.interface_growth.is_some_and(|s|!s.is_finite()||!(1.1..=2.).contains(&s))||r.interface_growth.is_some()&&r.interface_step_m.is_none() {return Err(error("BOARD_PROFILE","interface growth1.1..2 requires an interface step"));}
    if r.air_far_edge_divisions==Some(0)||r.air_first_step_m.is_some_and(|v|!v.is_finite()||v<=0.||v>r.air_margin_m)||r.air_near_distance_m.is_some_and(|v|!v.is_finite()||v<=0.||v>r.air_margin_m)||r.air_far_edge_divisions.is_some()&&r.air_near_distance_m.is_none()||r.bore_refinement_rings.is_some_and(|n|n>4){return Err(error("BOARD_PROFILE","finite positive air grading, positive far divisions and bore rings0..4 required"));}
    if r.air_edge_divisions==Some(0){return Err(error("BOARD_PROFILE","positive air edge divisions required"));}
    if r.min_triangle_angle_deg.is_some_and(|a|!a.is_finite()||!(0.0..=30.0).contains(&a)) {return Err(error("BOARD_PROFILE","minimum triangle angle must lie in0..30 degrees"));}
    if r.coordinate_origin_mm.is_some_and(|p|p.iter().any(|v|!v.is_finite())) || r.roi_mm.iter().flatten().any(|v|!v.is_finite()) || (0..2).any(|k|r.roi_mm[1][k]<=r.roi_mm[0][k]) || r.stack.len()<3 || r.finished_hole_plating_m.is_some_and(|v|!v.is_finite()||v<=0.) || r.barrel_conductivity_s_m.is_some_and(|v|!v.is_finite()||v<=0.) || !r.air_margin_m.is_finite() || r.air_margin_m<=0. || !r.max_triangle_area_m2.is_finite() || r.max_triangle_area_m2<=0. || !r.max_vertical_step_m.is_finite() || r.max_vertical_step_m<=0. {
        return Err(error("BOARD_PROFILE","finite ROI, full stack, finished-hole plating, material, air and mesh profile required"));
    }
    if r.cut_planes.keys().any(|name|!r.electrodes.iter().any(|e|&e.name==name)){return Err(error("BOARD_CUT","cut plane has no physical electrode"));}
    let options=GeometryOptions{max_deviation_mm:r.curve_deviation_mm,..Default::default()}; options.validate()?;
    let context=Context::for_board(board,options)?;
    let _scope=Context::install(Some(context.clone()));
    let geometry=geometry::prepare(board,options);
    check(start,max_wall)?;
    if geometry.outline.is_none() || geometry.findings.iter().any(|f|f.code!="net_islands") || !geometry.excluded.is_empty() || geometry.fills.iter().any(|f|f.status!="available") {
        return Err(error("BOARD_GEOMETRY","complete trustworthy copper, outline and native fills required"));
    }
    if !r.accept_unknown_fill_freshness&&geometry.fills.iter().any(|f|f.freshness.as_deref()!=Some("current")) {return Err(error("BOARD_FILL_POLICY","CAD fill freshness is not confirmed; an explicit captured-snapshot policy is required"));}
    // The clipping contour itself must use the already accepted board overlay
    // grid, just like the clipped result. Mixing the raw ROI coordinates with
    // their overlay image creates artificial sub-nanometre air strips.
    let domain=if let Some(points)=&r.domain_polygon_mm {
        if points.len()<3||points.iter().flatten().any(|v|!v.is_finite()) {return Err(error("BOARD_ROI","finite simple polygon required"));}
        let p=Polygon::new(LineString::from(points.clone()),vec![]);
        use geo::Validation;
        if !p.is_valid()||p.unsigned_area()<=0. {return Err(error("BOARD_ROI","valid positive-area simple polygon required"));}p
    }else{rect(r.roi_mm)};
    let roi=areas::union([domain]);
    let cut_tolerance=context.error_bound();
    let contact_boundary_edges=r.cut_planes.iter().map(|(name,c)|Ok((name.clone(),cut_edges(c,&roi,cut_tolerance)?))).collect::<Result<BTreeMap<_,_>>>()?;
    let bounds=roi.bounding_rect().ok_or_else(||error("BOARD_ROI","ROI disappeared on the shared board grid"))?;
    let roi_bounds=[[bounds.min().x,bounds.min().y],[bounds.max().x,bounds.max().y]];
    let margin=r.air_margin_m*1e3;
    let field_domain=areas::one(rect([[roi_bounds[0][0]-margin,roi_bounds[0][1]-margin],[roi_bounds[1][0]+margin,roi_bounds[1][1]+margin]]));
    let local_context=context.refined_for_bounds(roi_bounds)?;
    let clip_domain=|set:&MultiPolygon<f64>|->MultiPolygon<f64>{
        if r.domain_polygon_mm.is_none(){return areas::intersection(set,&roi);}
        // Crop on the board grid before entering the finer bounded adapter.
        // This crop lies strictly outside the physical domain; only the second
        // intersection defines the copper boundary, shared by every source.
        let pad=(roi_bounds[1][0]-roi_bounds[0][0]).max(roi_bounds[1][1]-roi_bounds[0][1])*0.1;
        let crop=areas::one(rect([[roi_bounds[0][0]-pad,roi_bounds[0][1]-pad],[roi_bounds[1][0]+pad,roi_bounds[1][1]+pad]]));
        let cropped=areas::intersection(set,&crop);
        let _scope=Context::install(Some(local_context.clone()));
        areas::intersection(&cropped,&roi)
    };
    let outline=areas::one(polygon(geometry.outline.as_ref().unwrap()));
    let mut substrate=clip_domain(&outline);
    if substrate.unsigned_area()<=0. {return Err(error("BOARD_ROI","ROI misses board"));}
    let mut z=0.;let mut spans=BTreeMap::new();let mut stack_spans=vec![];let mut copper_names=vec![];
    for layer in &r.stack {
        if !layer.thickness_m.is_finite() || layer.thickness_m<=0. || layer.thickness_m>0.1 {return Err(error("BOARD_PROFILE","positive finite stack thickness required"));}
        let next=z+layer.thickness_m;
        if layer.copper {
            if !board.layers.iter().any(|l|l.name==layer.name) || spans.insert(layer.name.clone(),[z,next]).is_some() || layer.conductivity_s_m.is_none_or(|v|!v.is_finite()||v<=0.) {return Err(error("BOARD_PROFILE","unique known copper layers and conductivity required"));}
            copper_names.push(layer.name.clone());
        } else if layer.epsilon_r.is_none_or(|v|!v.is_finite()||v<1.) {return Err(error("BOARD_PROFILE","dielectric epsilon required"));}
        stack_spans.push([z,next]);z=next;
    }
    if copper_names!=board.layers.iter().map(|l|l.name.clone()).collect::<Vec<_>>() || !r.stack[0].copper || !r.stack.last().unwrap().copper || r.stack.windows(2).any(|a|a[0].copper&&a[1].copper) {return Err(error("BOARD_PROFILE","complete ordered alternating copper stack required"));}
    let mut shapes=vec![];let mut source_mapping=vec![];
    for region in &geometry.regions {
        let poly=clip_domain(&areas::one(polygon(&region.polygon)));
        if poly.unsigned_area()==0. {continue;}
        if !r.allow_insulated_copper_cuts&&areas::difference(&areas::one(polygon(&region.polygon)),&roi).unsigned_area()>0. {return Err(error("BOARD_CUT_POLICY","ROI cuts copper; explicit insulated-cut boundary policy required"));}
        let layer=r.stack.iter().find(|l|l.copper&&l.name==region.layer).ok_or_else(||error("BOARD_PROFILE","region layer absent from stack"))?;
        source_mapping.push((region.id.clone(),region.source.path.clone(),region.source.net.clone(),region.layer.clone()));
        shapes.push(Shape{poly,id:region.id.clone(),sigma:layer.conductivity_s_m.unwrap(),span:spans[&region.layer]});
    }
    let mut bores=MultiPolygon(vec![]);let mut barrel_ids=vec![];
    // Keep one planar cut per partial bore span. XY ownership is shared by
    // the whole extrusion, but a drill only displaces material in its span.
    let mut partial_bores=BTreeMap::<(usize,usize),MultiPolygon<f64>>::new();
    // Retain the conservative native bore and use its same angular phase for
    // the nominal outer radius. Independent inward/outward curve errors must
    // not be amplified across the thin plating annulus.
    for hole in &geometry.holes {
        let bore=areas::union(hole.polygons.iter().map(polygon));
        if !bore.intersects(&roi) {continue;}
        if !hole.span_confirmed {return Err(error("BOARD_PADSTACK","ROI hole requires a confirmed physical span"));}
        if !hole.plated{bores=areas::union(bores.0.into_iter().chain(bore.0));continue;}
        let (center,radius,first,last)=if let Some(via)=board.vias.iter().find(|v|v.source.path==hole.source.path){
            ([via.at.x,via.at.y],via.drill_mm/2.,via.first_layer,via.last_layer)
        }else if let Some(pad)=board.pads.iter().find(|p|p.source.path==hole.source.path){
            let drill=pad.drill.as_ref().ok_or_else(||error("BOARD_PADSTACK","prepared pad bore has no authored drill"))?;
            if drill.shape!=crate::model::DrillShape::Round{return Err(error("BOARD_PADSTACK","plated slot requires its explicit capsule-wall support; no round substitution"));}
            if !drill.plated||pad.layers.len()!=board.layers.len()||(0..board.layers.len()).any(|l|!pad.layers.contains(&l)){return Err(error("BOARD_PADSTACK","confirmed round through-pad stack required"));}
            let center=geometry::primitives::transform_point(drill.offset.unwrap_or(crate::model::Point{x:0.,y:0.}),pad.at,pad.rotation_deg);
            ([center.x,center.y],drill.diameter_mm/2.,0,board.layers.len()-1)
        }else{return Err(error("BOARD_PADSTACK","plated bore has no explicit supported source padstack"));};
        let plating=r.finished_hole_plating_m.ok_or_else(||error("BOARD_PADSTACK","finished-hole plating thickness is missing"))?;
        let sigma=r.barrel_conductivity_s_m.ok_or_else(||error("BOARD_PADSTACK","barrel conductivity is missing"))?;
        let outer=barrel_outer(&bore,center,radius,plating*1e3,options.max_deviation_mm+2.*geometry.boolean_error_bound_mm)?;
        let outer=areas::one(outer);
        if areas::difference(&outer,&roi).unsigned_area()>0. {return Err(error("BOARD_ROI","ROI cuts through via barrel; choose a complete via"));}
        let annulus=if r.domain_polygon_mm.is_some() {
            // The complete barrel is bounded by the physical ROI. Form its
            // annulus on the final arrangement grid, not the coarser global
            // board grid; the inherited bore/source vertices stay exact.
            let _barrel_scope=Context::install(Some(context.refined_for_arrangement(roi_bounds)?));
            areas::difference(&outer,&bore)
        }else{areas::difference(&outer,&bore)};
        if annulus.unsigned_area()<=0. {return Err(error("BOARD_PADSTACK","plating annulus disappeared"));}
        let span=[spans[&board.layers[first].name][0],spans[&board.layers[last].name][1]];
        shapes.push(Shape{poly:annulus,id:hole.id.clone(),sigma,span});
        if first==0&&last+1==board.layers.len() {
            bores=areas::union(bores.0.into_iter().chain(bore.0));
        }else{
            let set=partial_bores.entry((first,last)).or_insert_with(||MultiPolygon(vec![]));
            *set=areas::union(set.0.drain(..).chain(bore.0));
        }
        barrel_ids.push(hole.id.clone());
    }
    let mut partial_bores=partial_bores.into_iter().map(|((first,last),poly)| {
        let span=[spans[&board.layers[first].name][0],spans[&board.layers[last].name][1]];
        (span,poly)
    }).collect::<Vec<_>>();
    if shapes.is_empty() {return Err(error("BOARD_ROI","no retained copper"));}
    // Source boundaries inside an already continuous equal-material conductor
    // are provenance, not dielectric interfaces. Meshing every overlapping
    // track/pad contour creates artificial near-collinear slivers. Constrain
    // the exact prepared physical union instead; attribute every positive-area
    // source overlap below without duplicating its resistance or metal volume.
    let mut arrangement_sets=geometry.layers.iter().map(|layer|clip_domain(&areas::union(layer.copper.iter().map(polygon)))).collect::<Vec<_>>();
    arrangement_sets.extend(shapes.iter().filter(|s|barrel_ids.contains(&s.id)).map(|s|s.poly.clone()));
    let mut physical_shapes=vec![];
    for layer in &geometry.layers {
        let material=r.stack.iter().find(|m|m.copper&&m.name==layer.layer).ok_or_else(||error("BOARD_PROFILE","prepared copper layer absent from profile"))?;
        physical_shapes.push((clip_domain(&areas::union(layer.copper.iter().map(polygon))),spans[&layer.layer],material.conductivity_s_m.unwrap(),false));
    }
    physical_shapes.extend(shapes.iter().filter(|s|barrel_ids.contains(&s.id)).map(|s|(s.poly.clone(),s.span,s.sigma,true)));
    let mut electrode_polygons=BTreeMap::new();
    for e in &r.electrodes {
        if e.name.trim().is_empty()||electrode_polygons.contains_key(&e.name)||!spans.contains_key(&e.layer) {return Err(error("BOARD_ELECTRODE","unique named electrodes on known copper layers required"));}
        let paths=e.canonical_paths()?;
        for path in &paths {if !geometry.regions.iter().any(|v|&v.source.path==path&&v.layer==e.layer){return Err(error("BOARD_ELECTRODE",&format!("source {path} absent on requested layer {}",e.layer)));}}
        let mut poly=clip_domain(&areas::union(geometry.regions.iter().filter(|v|paths.contains(&v.source.path)&&v.layer==e.layer).map(|v|polygon(&v.polygon))));
        if paths.len()>1 {
            let fragments=shapes.iter().filter(|shape|source_mapping.iter().any(|m|m.0==shape.id&&m.3==e.layer&&paths.contains(&m.1))).flat_map(|shape|shape.poly.0.iter().cloned()).collect::<Vec<_>>();
            let _union_scope=Context::install(Some(local_context.clone()));poly=areas::union(fragments);
        }
        if let Some(cut)=r.cut_planes.get(&e.name){
            let (origin,n,span)=cut.frame()?;
            if let Some(span)=span {
                let tangent=[-n[1],n[0]];
                let corners=[roi_bounds[0],[roi_bounds[1][0],roi_bounds[0][1]],roi_bounds[1],[roi_bounds[0][0],roi_bounds[1][1]]];
                let low=corners.iter().map(|p|signed(*p,origin,n)).fold(f64::INFINITY,f64::min)-cut_tolerance;
                let high=corners.iter().map(|p|signed(*p,origin,n)).fold(f64::NEG_INFINITY,f64::max)+cut_tolerance;
                let t_low=corners.iter().map(|p|signed(*p,origin,tangent)).fold(f64::INFINITY,f64::min).max(span[0]);
                let t_high=corners.iter().map(|p|signed(*p,origin,tangent)).fold(f64::NEG_INFINITY,f64::max).min(span[1]);
                if t_low>=t_high{return Err(error("BOARD_ELECTRODE","finite contact span misses domain"));}
                let points=[(t_low,low),(t_high,low),(t_high,high),(t_low,high)].map(|(t,d)|[origin[0]+t*tangent[0]+d*n[0],origin[1]+t*tangent[1]+d*n[1]]);
                let _span_scope=Context::install(Some(local_context.clone()));
                poly=areas::intersection(&poly,&areas::one(Polygon::new(LineString::from(points.to_vec()),vec![])));
            }
        }
        // An interior foil face can be occupied by a barrel continuing into the
        // adjacent dielectric. Only its actually exposed source-copper annulus
        // is a finite terminal; never cut the barrel or charge a buried face.
        let face=spans[&e.layer][usize::from(e.upper)];
        let adjacent_copper=areas::union(physical_shapes.iter().filter(|s|if e.upper {s.1[0]<=face&&s.1[1]>face}else{s.1[0]<face&&s.1[1]>=face}).flat_map(|s|s.0.0.iter().cloned()));
        if !r.cut_planes.contains_key(&e.name)&&adjacent_copper.unsigned_area()>0. {poly=areas::difference(&poly,&adjacent_copper);}
        if poly.unsigned_area()==0. {return Err(error("BOARD_ELECTRODE","contact source absent from retained layer copper"));}
        if paths.len()>1 {
            if let Some(cut)=r.cut_planes.get(&e.name) {
                let(o,n,_)=cut.frame()?;let edges=&contact_boundary_edges[&e.name];
                if face_intervals(&poly,edges,o,n,cut_tolerance).len()!=1 {return Err(error("BOARD_ELECTRODE","multi-source face must be one connected interval; split disconnected contacts"));}
                for path in &paths {
                    let part=clip_domain(&areas::union(geometry.regions.iter().filter(|v|&v.source.path==path&&v.layer==e.layer).map(|v|polygon(&v.polygon))));
                    let _part_scope=Context::install(Some(local_context.clone()));
                    let part=areas::intersection(&part,&poly);
                    if face_intervals(&part,edges,o,n,cut_tolerance).is_empty(){return Err(error("BOARD_ELECTRODE",&format!("source {path} does not contribute to finite contact face")));}
                }
            }else {
                if poly.0.len()!=1 {return Err(error("BOARD_ELECTRODE","multi-source exposed face disconnected; split contacts"));}
                for path in &paths {
                    let part=clip_domain(&areas::union(geometry.regions.iter().filter(|v|&v.source.path==path&&v.layer==e.layer).map(|v|polygon(&v.polygon))));
                    if areas::intersection(&part,&poly).unsigned_area()==0. {return Err(error("BOARD_ELECTRODE","source does not contribute exposed contact area"));}
                }
            }
        }
        // A finite electrode is a material boundary condition even when its
        // outline lies inside continuous same-material copper. Keep that exact
        // contour; partial provenance overlap must never enlarge the contact.
        arrangement_sets.push(poly.clone());electrode_polygons.insert(e.name.clone(),poly);
    }
    // Node every material/contact contour once on the same accepted overlay
    // grid before triangulation. Independent F64 constraint intersections can
    // otherwise create sub-grid cells between equivalent prepared contours.
    let original_contact_areas:BTreeMap<_,_>=electrode_polygons.iter().map(|(name,p)|(name.clone(),p.unsigned_area())).collect();
    let arrangement_context=if r.domain_polygon_mm.is_some() {
        // Bounds include every Boolean input (not only Cu), so tightening the
        // integer-grid padding cannot put a real bore outside its adapter.
        let mut bounds=roi_bounds;
        for set in std::iter::once(&bores).chain(arrangement_sets.iter()).chain(partial_bores.iter().map(|(_,poly)|poly)) {
            if let Some(b)=set.bounding_rect(){for k in 0..2 {bounds[0][k]=bounds[0][k].min(if k==0{b.min().x}else{b.min().y});bounds[1][k]=bounds[1][k].max(if k==0{b.max().x}else{b.max().y});}}
        }
        context.refined_for_arrangement(bounds)?
    }else{context.refined_for_bounds(roi_bounds)?};
    let _local_scope=Context::install(Some(arrangement_context));
    // A prepared source polygon is physical copper. The coarser derived layer
    // union can lose a sub-grid boundary sliver when sources are overlaid. Only
    // reconcile a contact that would otherwise fail the unchanged coverage gate;
    // preserve every actual bore, component and hole and the original1e-8 metal
    // area bound. This is source consistency, never a net-name/topology repair.
    if r.domain_polygon_mm.is_some() {
        // Source polygons are the copper authority. Union their clipped local
        // images on this same grid, avoiding a second coarse global union
        // whose rounding can produce unattributed material at rotated edges.
        for (i,layer) in geometry.layers.iter().enumerate() {
            let ids=source_mapping.iter().filter(|m|m.3==layer.layer).map(|m|&m.0).collect::<Vec<_>>();
            let copper=areas::union(shapes.iter().filter(|s|ids.contains(&&s.id)).flat_map(|s|s.poly.0.iter().cloned()));
            physical_shapes[i].0=copper.clone();arrangement_sets[i]=copper;
        }
    }
    let original_physical_stats=physical_shapes.iter().map(|s|(s.0.unsigned_area(),s.0.0.len(),s.0.0.iter().map(|p|p.interiors().len()).sum::<usize>())).collect::<Vec<_>>();
    let mut source_copper_restorations=vec![];
    for e in &r.electrodes {
        let layer=geometry.layers.iter().position(|l|l.layer==e.layer).ok_or("contact layer absent")?;
        let contact=&electrode_polygons[&e.name];let expected=original_contact_areas[&e.name];
        let covered=areas::intersection(contact,&physical_shapes[layer].0).unsigned_area();
        if (expected-covered).max(0.)<=1e-8*expected {continue;}
        for piece in &contact.0 {
            let owners=physical_shapes[layer].0.0.iter().filter(|parent|areas::intersection(&areas::one(piece.clone()),&areas::one((*parent).clone())).unsigned_area()>0.).count();
            if owners!=1 {return Err(error("BOARD_SOURCE_UNION",&format!("{} source piece overlaps {owners} material components; no new island or connection inferred",e.name)));}
        }
        let missing=areas::difference(contact,&physical_shapes[layer].0);
        let [start,end]=spans[&e.layer];
        let bore_overlap=std::iter::once(&bores)
            .chain(partial_bores.iter().filter(|(span,_)|span[0]<=start&&end<=span[1]).map(|(_,poly)|poly))
            .map(|poly|areas::intersection(&missing,poly).unsigned_area())
            .fold(0.,f64::max);
        if bore_overlap>0. {return Err(error("BOARD_SOURCE_UNION",&format!("{} missing source copper intersects a real bore by {bore_overlap:e}mm2; no hole filled",e.name)));}
        let restored=areas::union(physical_shapes[layer].0.0.iter().chain(contact.0.iter()).cloned());
        let (area,components,holes)=original_physical_stats[layer];let change=(restored.unsigned_area()-area).abs()/area;
        let restored_holes=restored.0.iter().map(|p|p.interiors().len()).sum::<usize>();
        if change>1e-8||restored.0.len()!=components||restored_holes!=holes {return Err(error("BOARD_SOURCE_UNION",&format!("{} source reconciliation would change material area/topology: relative={change:e}, components={}/{components}, holes={restored_holes}/{holes}; no copper repaired",e.name,restored.0.len())));}
        source_copper_restorations.push(serde_json::json!({"contact":e.name,"sourcePaths":e.canonical_paths()?,"layer":e.layer,"decision":"resolved","reason":"retain exact prepared source copper lost by the derived layer union; component/hole counts and original1e-8 copper-area/contact guards preserved","missingAreaMm2":missing.unsigned_area(),"relativeContactGap":(expected-covered)/expected,"relativeMetalAreaChange":change,"boreOverlapAreaMm2":bore_overlap}));
        arrangement_sets[layer]=restored.clone();physical_shapes[layer].0=restored;
    }
    let physical_count=physical_shapes.len();
    if arrangement_sets.len()!=physical_count+r.electrodes.len() {return Err(error("BOARD_ARRANGEMENT","material/contact cut ordering mismatch"));}
    let cuts=std::iter::once(substrate.clone()).chain(std::iter::once(bores.clone()))
        .chain(arrangement_sets).chain(partial_bores.iter_mut().map(|(_,poly)|std::mem::replace(poly,MultiPolygon(vec![]))))
        .collect::<Vec<_>>();
    let atoms=common_partition(&roi,&cuts,budget,start,max_wall)?;
    let member_set=|index:usize|areas::union(atoms.iter().filter(|a|a.membership[index]).map(|a|a.poly.clone()));
    substrate=member_set(0);bores=member_set(1);
    for (i,(_,poly)) in partial_bores.iter_mut().enumerate() {
        *poly=member_set(2+physical_count+r.electrodes.len()+i);
    }
    let mut max_copper_area_change:f64=0.;
    for(i,shape)in physical_shapes.iter_mut().enumerate() {
        let canonical=member_set(i+2);let (original_area,original_components,original_holes)=original_physical_stats[i];let area=canonical.unsigned_area();
        let change=if original_area==0. {if area==0.{0.}else{f64::INFINITY}}else{(area-original_area).abs()/original_area};
        let holes=|p:&MultiPolygon<f64>|p.0.iter().map(|p|p.interiors().len()).sum::<usize>();
        if change>1e-8||canonical.0.len()!=original_components||holes(&canonical)!=original_holes {return Err(error("BOARD_ARRANGEMENT",&format!("physical copper set{i} changed area/topology: relative area={change:e}, components={}/{}, holes={}/{}; no filtering permitted",canonical.0.len(),original_components,holes(&canonical),original_holes)));}
        max_copper_area_change=max_copper_area_change.max(change);shape.0=canonical;
    }
    for(i,e)in r.electrodes.iter().enumerate(){electrode_polygons.insert(e.name.clone(),member_set(2+physical_count+i));}
    // Keep an explicit stable numerical datum. Domain expansion changes the
    // physical air/ROI request but must not retranslate identical copper.
    let origin_mm=r.coordinate_origin_mm.unwrap_or(r.roi_mm[0]);
    let mut cdt=ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    // Every ring, including all overlapping source boundaries, constrains the
    // one XY arrangement. Split intersections rather than centroid ownership on
    // an unconstrained triangulation. No feature/area threshold is applied.
    let field_spacing=if support.is_some(){r.electric_max_edge_m.map(|v|v*1e3)}else{None};
    let near=r.stack.iter().map(|l|l.thickness_m).sum::<f64>()*4e3;
    let near_box=[[roi_bounds[0][0]-near,roi_bounds[0][1]-near],[roi_bounds[1][0]+near,roi_bounds[1][1]+near]];
    let mut segment=|a:[f64;2],b:[f64;2]|->Result<()>{
        let refine=field_spacing.filter(|_|(0..2).all(|k|a[k].min(b[k])<=near_box[1][k]&&a[k].max(b[k])>=near_box[0][k]));
        let mut divisions=1usize;if let Some(spacing)=refine{let length=(b[0]-a[0]).hypot(b[1]-a[1]);while length/divisions as f64>spacing{divisions=divisions.checked_mul(2).ok_or("field edge subdivision overflow")?;if divisions>budget.max_nodes{return Err(error("MEMORY_LIMIT","field edge subdivision node cap"));}}}
        let mut previous=None;for i in 0..=divisions{check(start,max_wall)?;if cdt.num_vertices()+cdt.num_undirected_edges()+2>budget.max_nodes{return Err(error("MEMORY_LIMIT","board constraint subdivision preflight"));}budget.check((cdt.num_vertices()+cdt.num_undirected_edges()+2).checked_mul(4096).ok_or("arrangement overflow")?)?;
            let fraction=i as f64/divisions as f64;let p=if i==0{a}else if i==divisions{b}else{std::array::from_fn(|k|a[k]*(1.-fraction)+b[k]*fraction)};
            let node=cdt.insert(Point2::new(p[0],p[1])).map_err(|e|error("BOARD_MESH",&e.to_string()))?;if let Some(old)=previous{if old!=node{cdt.add_constraint_and_split(old,node,|p|p);}}previous=Some(node);
        }Ok(())
    };
    for poly in atoms.iter().map(|a|&a.poly){for ring in std::iter::once(poly.exterior()).chain(poly.interiors()){for line in ring.lines(){segment([line.start.x,line.start.y],[line.end.x,line.end.y])?;}}}
    if let Some((nodes,triangles))=support{let mut edges=std::collections::BTreeSet::new();for t in triangles{for k in 0..3{let mut e=[t[k],t[(k+1)%3]];e.sort();edges.insert(e);}}for [a,b]in edges{segment(nodes[a],nodes[b])?;}}
    for [a,b] in trace_lines {segment(*a,*b)?;}
    drop(segment);
    let max_xy=budget.max_nodes.min(budget.max_bytes/4096);
    let refinement=cdt.refine(RefinementParameters::new().keep_constraint_edges().with_angle_limit(AngleLimit::from_deg(r.min_triangle_angle_deg.unwrap_or(30.))).with_max_allowed_area(r.max_triangle_area_m2*1e6).with_max_additional_vertices(max_xy.saturating_sub(cdt.num_vertices())));
    if !refinement.refinement_complete {return Err(error("MEMORY_LIMIT","board XY mesh refinement incomplete within budget"));}
    check(start,max_wall)?;
    // Refine XY only inside retained native bore contours. The material there
    // still depends on z. Existing Cu/bore/contact constraints stay fixed.
    if let Some(rings)=r.bore_refinement_rings {
        use geo::Centroid;
        for bore in bores.0.iter().chain(partial_bores.iter().flat_map(|(_,poly)|&poly.0)) {
            let centre=bore.centroid().ok_or_else(||error("BOARD_PROFILE","bore centroid unavailable"))?;
            let mut points=vec![[centre.x(),centre.y()]];
            for ring in 1..=rings {let fraction=ring as f64/(rings+1)as f64;for p in &bore.exterior().0[..bore.exterior().0.len()-1]{points.push([centre.x()+fraction*(p.x-centre.x()),centre.y()+fraction*(p.y-centre.y())]);}}
            for point in points {let p=Point::new(point[0],point[1]);if bore.contains(&p)&&roi.contains(&p){if cdt.num_vertices()+1>max_xy{return Err(error("MEMORY_LIMIT","bore air refinement exceeds node budget"));}cdt.insert(Point2::new(point[0],point[1])).map_err(|e|error("BOARD_MESH",&e.to_string()))?;}}
        }
    }
    // Refine the physical ROI first. Graded air rings keep a larger far domain
    // from demanding a uniform copper-scale mesh over its entire area.
    let mut distance=r.air_first_step_m.map(|v|v*1e3).unwrap_or_else(||(roi_bounds[1][0]-roi_bounds[0][0]).min(roi_bounds[1][1]-roi_bounds[0][1])/4.).min(margin);
    loop {
        let corners=[[roi_bounds[0][0]-distance,roi_bounds[0][1]-distance],[roi_bounds[1][0]+distance,roi_bounds[0][1]-distance],[roi_bounds[1][0]+distance,roi_bounds[1][1]+distance],[roi_bounds[0][0]-distance,roi_bounds[1][1]+distance]];
        let divisions=if r.air_near_distance_m.is_some_and(|near|distance>near*1e3*(1.+1e-12)){r.air_far_edge_divisions.unwrap_or(r.air_edge_divisions.unwrap_or(4))}else{r.air_edge_divisions.unwrap_or(4)};
        check(start,max_wall)?;
        let planned_vertices=divisions.checked_mul(4).and_then(|count|cdt.num_vertices().checked_add(count));
        if planned_vertices.is_none_or(|count|count>max_xy) {
            return Err(error("MEMORY_LIMIT","graded air ring exceeds node budget"));
        }
        for edge in 0..4 {
            let a=corners[edge];let b=corners[(edge+1)%4];
            for i in 0..divisions {
                if i%256==0 {check(start,max_wall)?;}
                let t=i as f64/divisions as f64;
                cdt.insert(Point2::new(a[0]+t*(b[0]-a[0]),a[1]+t*(b[1]-a[1]))).map_err(|e|error("BOARD_MESH",&e.to_string()))?;
            }
        }
        if distance==margin {break;}distance=(2.*distance).min(margin);
    }
    let planar_nodes_mm:Vec<_>=cdt.vertices().map(|v|[v.position().x,v.position().y]).collect();
    let xy_m:Vec<_>=planar_nodes_mm.iter().map(|p|[(p[0]-origin_mm[0])*1e-3,(p[1]-origin_mm[1])*1e-3]).collect();
    let mut triangles=vec![];let mut centres=vec![];let mut overlaps=vec![];
    for face in cdt.inner_faces() {
        let v=face.vertices();let centre=Point::new(v.iter().map(|p|p.position().x).sum::<f64>()/3.,v.iter().map(|p|p.position().y).sum::<f64>()/3.);
        if field_domain.contains(&centre) {
            let triangle=areas::one(Polygon::new(LineString::from(v.map(|v|[v.position().x,v.position().y]).to_vec()),vec![]));
            // Provenance is a predicate on the actual F64 arrangement. Running
            // another quantizing Boolean on a tiny mesh face can erase its
            // positive overlap; robust DE-9IM keeps that source association.
            let sources=shapes.iter().enumerate().filter(|(_,s)|s.poly.relate(&triangle).get(CoordPos::Inside,CoordPos::Inside)==Dimensions::TwoDimensional).map(|(i,_)|i).collect::<Vec<_>>();
            triangles.push(v.map(|v|v.fix().index()));centres.push(centre);overlaps.push(sources);
        }
    }
    let mut edges=BTreeMap::new();for t in &triangles {for k in 0..3 {let mut edge=[t[k],t[(k+1)%3]];edge.sort();*edges.entry(edge).or_insert(0usize)+=1;}}
    let boundary_edges=edges.into_iter().filter(|(_,n)|*n==1).map(|(e,_)|e).collect();
    let mut air_distances=vec![];let mut distance=r.interface_step_m.unwrap_or(r.max_vertical_step_m).min(r.max_vertical_step_m).min(r.air_margin_m);
    loop {air_distances.push(distance);if distance==r.air_margin_m {break;}distance=(2.*distance).min(r.air_margin_m);}
    let mut copper_skin_layers=Vec::new();
    let max_planes=if skin.is_some() {
        copper_skin_profile::max_extrusion_planes(xy_m.len(),triangles.len(),budget.max_nodes,budget.max_tetrahedra,budget.max_bytes)
            .ok_or_else(||error("MEMORY_LIMIT","skin-profile extrusion plane cap overflow"))?
    }else{usize::MAX};
    if skin.is_some() {copper_skin_profile::require_plane_room(air_distances.len(),1,air_distances.len(),max_planes)?;}
    let mut z_m:Vec<_>=air_distances.iter().rev().map(|v|-v).collect();z_m.push(0.);
    for (layer,span) in r.stack.iter().zip(&stack_spans) {
        let thickness=span[1]-span[0];
        if let Some(profile)=skin {
            let sigma=layer.conductivity_s_m.filter(|_|layer.copper);
            let pieces=profile.pieces(thickness,sigma,r.max_vertical_step_m)
                .ok_or_else(||error("BOARD_PROFILE","skin-profile slab count is not representable"))?;
            let max_step=copper_skin_profile::append_layer_planes(&mut z_m,*span,pieces,air_distances.len(),max_planes,||check(start,max_wall))?;
            if let Some(sigma)=sigma {
                copper_skin_layers.push(CopperSkinLayer{layer:layer.name.clone(),skin_depth_m:profile.skin_depth_m(sigma),maximum_step_m:max_step,slabs:pieces});
            }
        }else{
            let pieces=(thickness/r.max_vertical_step_m).ceil() as usize;
            for i in 1..=pieces {z_m.push(if i==pieces {span[1]}else{span[0]+thickness*i as f64/pieces as f64});}
        }
    }
    if let Some(step)=r.interface_step_m {
        for (layer,span) in r.stack.iter().zip(&stack_spans) {
            if !layer.copper {
                let mut distance=step;
                while distance<(span[1]-span[0])/2. {
                    let lo=span[0]+distance;let hi=span[1]-distance;
                    if lo==span[0]||hi==span[1] {return Err(error("BOARD_PROFILE","interface step below representable vertical resolution"));}
                    if skin.is_some() {copper_skin_profile::require_plane_room(z_m.len(),2,air_distances.len(),max_planes)?;}
                    z_m.extend([lo,hi]);distance*=r.interface_growth.unwrap_or(2.);
                }
            }
        }
    }
    z_m.extend(air_distances.iter().map(|d|z+d));
    z_m.sort_unstable_by(f64::total_cmp);z_m.dedup();
    let count=triangles.len().checked_mul(z_m.len()-1).and_then(|v|v.checked_mul(3)).ok_or_else(||error("MEMORY_LIMIT","board tetra count overflow"))?;
    budget.check(count.checked_mul(2048).ok_or_else(||error("MEMORY_LIMIT","board tetra memory overflow"))?)?;
    if count>budget.max_tetrahedra || xy_m.len().checked_mul(z_m.len()).is_none_or(|n|n>budget.max_nodes) {return Err(error("MEMORY_LIMIT",&format!("board extrusion nodes={} (cap{}), tetrahedra={count} (cap{}) exceed explicit count budget",xy_m.len().saturating_mul(z_m.len()),budget.max_nodes,budget.max_tetrahedra)));}
    let mut materials=vec![];let mut sigma_per_triangle=vec![];
    for slab in z_m.windows(2) {
        let middle=(slab[0]+slab[1])/2.;let layer=r.stack.iter().zip(&stack_spans).find(|(_,s)|middle>s[0]&&middle<s[1]).map(|(l,_)|l);
        let active_bores=partial_bores.iter().filter(|(span,_)|span[0]<middle&&middle<span[1])
            .map(|(_,poly)|poly).collect::<Vec<_>>();
        let mut row=vec![];let mut sigma=vec![];
        for (ti,p) in centres.iter().enumerate() {
            // Physical ownership is constant inside each constrained material
            // face; provenance can cover only part of that same face.
            let physical:Vec<_>=physical_shapes.iter().filter(|(poly,span,_,_)|middle>span[0]&&middle<span[1]&&poly.contains(p)).collect();
            let copper=!physical.is_empty();
            let owners:Vec<_>=if copper {overlaps[ti].iter().map(|&i|&shapes[i]).filter(|s|middle>s.span[0]&&middle<s.span[1]).collect()}else{vec![]};
            if copper&&owners.is_empty() {return Err(error("BOARD_OWNERSHIP",&format!("canonical copper face{ti} at {p:?}, z={middle:e} has no positive-area source attribution; no copper removed")));}
            if !copper {
                let dielectric=layer.filter(|l|!l.copper&&substrate.contains(p)&&!bores.contains(p)
                    &&!active_bores.iter().any(|poly|poly.contains(p)));
                row.push(Material::Dielectric{label:dielectric.map_or("air",|l|l.name.as_str()).into(),epsilon_r:dielectric.and_then(|l|l.epsilon_r).unwrap_or(1.)});sigma.push(0.);
            }else{
                // Finished-hole wall material owns its actual annulus also
                // within a foil slab. The native pad includes that final metal
                // outline; never count an overlapping second foil resistance.
                let chosen=physical.iter().find(|s|s.3).unwrap_or(&physical[0]);let s=chosen.2;
                if physical.iter().filter(|o|o.3==chosen.3).any(|o|o.2!=s) {return Err(error("BOARD_MATERIAL","overlapping physical copper materials disagree"));}
                row.push(Material::Copper{source_ids:owners.iter().map(|o|o.id.clone()).collect()});sigma.push(s);
            }
        }
        materials.push(row);sigma_per_triangle.push(sigma);
    }
    let planar_triangles=triangles.clone();
    let mesh=mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m,materials},budget)?;
    let conductivity_s_m=mesh.copper_tetrahedra.iter().map(|t|sigma_per_triangle[t.slab][t.planar_triangle]).collect();
    let mut certified_electrodes=BTreeMap::new();let mut electrode_areas_m2=BTreeMap::new();let mut max_contact_area_change:f64=0.;
    for e in &r.electrodes {
        let poly=&electrode_polygons[&e.name];let z=spans[&e.layer][if e.upper {1}else{0}];
        let cut=r.cut_planes.get(&e.name);
        let edges=contact_boundary_edges.get(&e.name);
        let mut nodes=std::collections::BTreeSet::new();let mut area=0.;
        for face in &mesh.copper_interfaces {
            if let Some(cut)=cut {
                let _=cut;
                if !face.nodes.iter().all(|&i|on_edges([origin_mm[0]+mesh.nodes_m[i][0]*1e3,origin_mm[1]+mesh.nodes_m[i][1]*1e3],edges.unwrap(),cut_tolerance)&&mesh.nodes_m[i][2]>=spans[&e.layer][0]&&mesh.nodes_m[i][2]<=spans[&e.layer][1]){continue;}
            }else if !face.nodes.iter().all(|&i|mesh.nodes_m[i][2]==z) {continue;}
            let xy=face.nodes.map(|i|[mesh.nodes_m[i][0],mesh.nodes_m[i][1]]);
            let centre=Point::new(origin_mm[0]+1e3*xy.iter().map(|p|p[0]).sum::<f64>()/3.,origin_mm[1]+1e3*xy.iter().map(|p|p[1]).sum::<f64>()/3.);

            let owns_contact=if cut.is_some(){
                // A mean of three rotated boundary nodes need not lie exactly
                // on a polygon edge. Test its physical one-dimensional contact
                // intervals, retaining the original source identity.
                let contact_edges=poly.0.iter().flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).flat_map(|r|r.lines()).map(|e|[[e.start.x,e.start.y],[e.end.x,e.end.y]]).filter(|[a,b]|on_edges(*a,edges.unwrap(),cut_tolerance)&&on_edges(*b,edges.unwrap(),cut_tolerance)).collect::<Vec<_>>();
                let source=source_mapping.iter().filter(|m|e.canonical_paths().is_ok_and(|paths|paths.contains(&m.1))&&m.3==e.layer).any(|m|face.copper_source_ids.contains(&m.0));
                source&&on_edges([centre.x(),centre.y()],&contact_edges,cut_tolerance)
            }else{poly.contains(&centre)};
            if owns_contact {
                let [a,b,c]=face.nodes.map(|i|mesh.nodes_m[i]);let u=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let v=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];
                area+=((u[1]*v[2]-u[2]*v[1]).powi(2)+(u[2]*v[0]-u[0]*v[2]).powi(2)+(u[0]*v[1]-u[1]*v[0]).powi(2)).sqrt()/2.;nodes.extend(face.nodes);
            }
        }
        let expected=if let Some(cut)=cut {
            let _=cut;
            let mut length=0.;for p in &poly.0 {for ring in std::iter::once(p.exterior()).chain(p.interiors()) {for edge in ring.0.windows(2){let a=[edge[0].x,edge[0].y];let b=[edge[1].x,edge[1].y];if on_edges(a,edges.unwrap(),cut_tolerance)&&on_edges(b,edges.unwrap(),cut_tolerance){length+=(a[0]-b[0]).hypot(a[1]-b[1]);}}}}
            length*1e-3*(spans[&e.layer][1]-spans[&e.layer][0])
        }else{original_contact_areas[&e.name]*1e-6};
        if nodes.len()<3||(area-expected).abs()>1e-8*expected {return Err(error("BOARD_ELECTRODE",&format!("finite exposed contact area mismatch: {} mesh={area:e} source={expected:e} canonical={:e}",e.name,poly.unsigned_area()*1e-6)));}
        max_contact_area_change=max_contact_area_change.max((area-expected).abs()/expected);
        electrode_areas_m2.insert(e.name.clone(),area);certified_electrodes.insert(e.name.clone(),(e.clone(),nodes.into_iter().collect()));
    }
    check(start,max_wall)?;
    Ok(BoardMesh{mesh,copper_skin_layers,planar_nodes_mm,planar_triangles,conductivity_s_m,geometry,origin_mm,source_mapping,barrel_ids,elapsed_seconds:start.elapsed().as_secs_f64(),layer_spans:spans,electrode_areas_m2,arrangement_atoms:atoms.len(),max_copper_area_change,max_contact_area_change,source_copper_restorations,certified_electrodes})
}
impl BoardMesh {
    /// End geometry preparation before a separately budgeted field phase.
    pub fn into_mesh(self)->LayeredMesh{self.mesh}
    pub fn electrode_nodes(&self,e:&Electrode)->Result<Vec<usize>> {
        let (descriptor,nodes)=self.certified_electrodes.get(&e.name).ok_or_else(||error("BOARD_ELECTRODE","electrode was not part of the conforming mesh request"))?;
        if !descriptor.same_contact(e) {return Err(error("BOARD_ELECTRODE","electrode descriptor differs from its certified mesh contour"));}Ok(nodes.clone())
    }
}

