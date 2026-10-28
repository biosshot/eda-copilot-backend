//! Board-owned, shared-overlay-grid material arrangement for volume extraction.
//! Every intersecting source is retained, including floating neighbouring copper.
use super::{error,mesh::{self,LayeredInput,LayeredMesh,Material},AllocationBudget,Result};
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
    pub stack:Vec<Layer>,
    /// Explicit finished-hole convention; unknown bore conventions are refused.
    pub finished_hole_plating_m:Option<f64>,
    pub barrel_conductivity_s_m:Option<f64>,
    pub air_margin_m:f64,
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
    #[serde(default)]
    pub ports:Vec<[String;2]>,
}
/// Geometry-bound coefficient identity excludes only independent field/current
/// discretization controls. It retains every authored physical assumption.
pub fn physical_signature(r:&Request)->serde_json::Value {
    serde_json::json!({"roiMm":r.roi_mm,"stack":r.stack,"finishedHolePlatingM":r.finished_hole_plating_m,"barrelConductivitySM":r.barrel_conductivity_s_m,"curveDeviationMm":r.curve_deviation_mm,"electrodes":r.electrodes,"ports":r.ports,"acceptUnknownFillFreshness":r.accept_unknown_fill_freshness,"allowInsulatedCopperCuts":r.allow_insulated_copper_cuts})
}
#[derive(Clone,Debug,PartialEq,serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Electrode {
    pub name:String,
    pub source_path:String,
    pub layer:String,
    /// Lower or upper face of the physical copper layer, in the stack z axis.
    pub upper:bool,
}
pub struct BoardMesh {
    pub mesh:LayeredMesh,
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
    let start=Instant::now();
    if r.interface_step_m.is_some_and(|s|!s.is_finite()||s<=0.) {return Err(error("BOARD_PROFILE","positive finite interface step required"));}
    if r.interface_growth.is_some_and(|s|!s.is_finite()||!(1.1..=2.).contains(&s))||r.interface_growth.is_some()&&r.interface_step_m.is_none() {return Err(error("BOARD_PROFILE","interface growth1.1..2 requires an interface step"));}
    if r.air_far_edge_divisions.is_some_and(|n|!(1..=32).contains(&n))||r.air_first_step_m.is_some_and(|v|!v.is_finite()||v<=0.||v>r.air_margin_m)||r.air_near_distance_m.is_some_and(|v|!v.is_finite()||v<=0.||v>r.air_margin_m)||r.air_far_edge_divisions.is_some()&&r.air_near_distance_m.is_none()||r.bore_refinement_rings.is_some_and(|n|n>4){return Err(error("BOARD_PROFILE","finite positive air grading, near/far divisions1..32 and bore rings0..4 required"));}
    if r.air_edge_divisions.is_some_and(|n|!(1..=32).contains(&n)){return Err(error("BOARD_PROFILE","air edge divisions must lie in1..32"));}
    if r.min_triangle_angle_deg.is_some_and(|a|!a.is_finite()||!(0.0..=30.0).contains(&a)) {return Err(error("BOARD_PROFILE","minimum triangle angle must lie in0..30 degrees"));}
    if r.roi_mm.iter().flatten().any(|v|!v.is_finite()) || (0..2).any(|k|r.roi_mm[1][k]<=r.roi_mm[0][k]) || r.stack.len()<3 || r.finished_hole_plating_m.is_some_and(|v|!v.is_finite()||v<=0.) || r.barrel_conductivity_s_m.is_some_and(|v|!v.is_finite()||v<=0.) || !r.air_margin_m.is_finite() || r.air_margin_m<=0. || !r.max_triangle_area_m2.is_finite() || r.max_triangle_area_m2<=0. || !r.max_vertical_step_m.is_finite() || r.max_vertical_step_m<=0. {
        return Err(error("BOARD_PROFILE","finite ROI, full stack, finished-hole plating, material, air and mesh profile required"));
    }
    let options=GeometryOptions{max_deviation_mm:r.curve_deviation_mm,..Default::default()}; options.validate()?;
    let context=Context::for_board(board,options)?;
    let _scope=Context::install(Some(context.clone()));
    let geometry=geometry::prepare(board,options);
    check(start,max_wall)?;
    if geometry.outline.is_none() || !geometry.findings.is_empty() || !geometry.excluded.is_empty() || geometry.fills.iter().any(|f|f.status!="available") {
        return Err(error("BOARD_GEOMETRY","complete trustworthy copper, outline and native fills required"));
    }
    if !r.accept_unknown_fill_freshness&&geometry.fills.iter().any(|f|f.freshness.as_deref()!=Some("current")) {return Err(error("BOARD_FILL_POLICY","CAD fill freshness is not confirmed; an explicit captured-snapshot policy is required"));}
    // The clipping contour itself must use the already accepted board overlay
    // grid, just like the clipped result. Mixing the raw ROI coordinates with
    // their overlay image creates artificial sub-nanometre air strips.
    let roi=areas::union([rect(r.roi_mm)]);
    let bounds=roi.bounding_rect().ok_or_else(||error("BOARD_ROI","ROI disappeared on the shared board grid"))?;
    let roi_bounds=[[bounds.min().x,bounds.min().y],[bounds.max().x,bounds.max().y]];
    let margin=r.air_margin_m*1e3;
    let field_domain=areas::one(rect([[roi_bounds[0][0]-margin,roi_bounds[0][1]-margin],[roi_bounds[1][0]+margin,roi_bounds[1][1]+margin]]));
    let outline=areas::one(polygon(geometry.outline.as_ref().unwrap()));
    let mut substrate=areas::intersection(&outline,&roi);
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
        let poly=areas::intersection(&areas::one(polygon(&region.polygon)),&roi);
        if poly.unsigned_area()==0. {continue;}
        if !r.allow_insulated_copper_cuts&&areas::difference(&areas::one(polygon(&region.polygon)),&roi).unsigned_area()>0. {return Err(error("BOARD_CUT_POLICY","ROI cuts copper; explicit insulated-cut boundary policy required"));}
        let layer=r.stack.iter().find(|l|l.copper&&l.name==region.layer).ok_or_else(||error("BOARD_PROFILE","region layer absent from stack"))?;
        source_mapping.push((region.id.clone(),region.source.path.clone(),region.source.net.clone(),region.layer.clone()));
        shapes.push(Shape{poly,id:region.id.clone(),sigma:layer.conductivity_s_m.unwrap(),span:spans[&region.layer]});
    }
    let mut bores=MultiPolygon(vec![]);let mut barrel_ids=vec![];
    // Retain the conservative native bore and use its same angular phase for
    // the nominal outer radius. Independent inward/outward curve errors must
    // not be amplified across the thin plating annulus.
    for hole in &geometry.holes {
        let bore=areas::union(hole.polygons.iter().map(polygon));
        if !bore.intersects(&roi) {continue;}
        if !hole.plated || !hole.span_confirmed {return Err(error("BOARD_PADSTACK","ROI hole requires confirmed ordinary plated through-via"));}
        let via=board.vias.iter().find(|v|v.source.path==hole.source.path).ok_or_else(||error("BOARD_PADSTACK","only explicit ordinary round vias supported; pad drills require their own padstack"))?;
        if via.kind!=crate::model::ViaKind::Through || via.first_layer!=0 || via.last_layer+1!=board.layers.len() {return Err(error("BOARD_PADSTACK","only confirmed ordinary through-via span supported"));}
        let plating=r.finished_hole_plating_m.ok_or_else(||error("BOARD_PADSTACK","finished-hole plating thickness is missing"))?;
        let sigma=r.barrel_conductivity_s_m.ok_or_else(||error("BOARD_PADSTACK","barrel conductivity is missing"))?;
        let outer=barrel_outer(&bore,[via.at.x,via.at.y],via.drill_mm/2.,plating*1e3,options.max_deviation_mm+2.*geometry.boolean_error_bound_mm)?;
        let outer=areas::one(outer);
        if areas::difference(&outer,&roi).unsigned_area()>0. {return Err(error("BOARD_ROI","ROI cuts through via barrel; choose a complete via"));}
        let annulus=areas::difference(&outer,&bore);
        if annulus.unsigned_area()<=0. {return Err(error("BOARD_PADSTACK","plating annulus disappeared"));}
        let span=[spans[&board.layers[via.first_layer].name][0],spans[&board.layers[via.last_layer].name][1]];
        shapes.push(Shape{poly:annulus,id:hole.id.clone(),sigma,span});
        bores=areas::union(bores.0.into_iter().chain(bore.0));barrel_ids.push(hole.id.clone());
    }
    if shapes.is_empty() {return Err(error("BOARD_ROI","no retained copper"));}
    // Source boundaries inside an already continuous equal-material conductor
    // are provenance, not dielectric interfaces. Meshing every overlapping
    // track/pad contour creates artificial near-collinear slivers. Constrain
    // the exact prepared physical union instead; attribute every positive-area
    // source overlap below without duplicating its resistance or metal volume.
    let mut arrangement_sets=geometry.layers.iter().map(|layer|areas::intersection(&areas::union(layer.copper.iter().map(polygon)),&roi)).collect::<Vec<_>>();
    arrangement_sets.extend(shapes.iter().filter(|s|barrel_ids.contains(&s.id)).map(|s|s.poly.clone()));
    let mut physical_shapes=vec![];
    for layer in &geometry.layers {
        let material=r.stack.iter().find(|m|m.copper&&m.name==layer.layer).ok_or_else(||error("BOARD_PROFILE","prepared copper layer absent from profile"))?;
        physical_shapes.push((areas::intersection(&areas::union(layer.copper.iter().map(polygon)),&roi),spans[&layer.layer],material.conductivity_s_m.unwrap(),false));
    }
    physical_shapes.extend(shapes.iter().filter(|s|barrel_ids.contains(&s.id)).map(|s|(s.poly.clone(),s.span,s.sigma,true)));
    let mut electrode_polygons=BTreeMap::new();
    for e in &r.electrodes {
        if e.name.trim().is_empty()||electrode_polygons.contains_key(&e.name)||!spans.contains_key(&e.layer) {return Err(error("BOARD_ELECTRODE","unique named electrodes on known copper layers required"));}
        let poly=areas::intersection(&areas::union(geometry.regions.iter().filter(|v|v.source.path==e.source_path&&v.layer==e.layer).map(|v|polygon(&v.polygon))),&roi);
        if poly.unsigned_area()==0. {return Err(error("BOARD_ELECTRODE","contact source absent from retained layer copper"));}
        // A finite electrode is a material boundary condition even when its
        // outline lies inside continuous same-material copper. Keep that exact
        // contour; partial provenance overlap must never enlarge the contact.
        arrangement_sets.push(poly.clone());electrode_polygons.insert(e.name.clone(),poly);
    }
    // Node every material/contact contour once on the same accepted overlay
    // grid before triangulation. Independent F64 constraint intersections can
    // otherwise create sub-grid cells between equivalent prepared contours.
    let original_contact_areas:BTreeMap<_,_>=electrode_polygons.iter().map(|(name,p)|(name.clone(),p.unsigned_area())).collect();
    let _local_scope=Context::install(Some(context.refined_for_bounds(roi_bounds)?));
    let physical_count=physical_shapes.len();
    if arrangement_sets.len()!=physical_count+r.electrodes.len() {return Err(error("BOARD_ARRANGEMENT","material/contact cut ordering mismatch"));}
    let cuts=std::iter::once(substrate.clone()).chain(std::iter::once(bores.clone())).chain(arrangement_sets).collect::<Vec<_>>();
    let atoms=common_partition(&roi,&cuts,budget,start,max_wall)?;
    let member_set=|index:usize|areas::union(atoms.iter().filter(|a|a.membership[index]).map(|a|a.poly.clone()));
    substrate=member_set(0);bores=member_set(1);
    let mut max_copper_area_change:f64=0.;
    for(i,shape)in physical_shapes.iter_mut().enumerate() {
        let canonical=member_set(i+2);let original_area=shape.0.unsigned_area();let area=canonical.unsigned_area();
        let change=if original_area==0. {if area==0.{0.}else{f64::INFINITY}}else{(area-original_area).abs()/original_area};
        let holes=|p:&MultiPolygon<f64>|p.0.iter().map(|p|p.interiors().len()).sum::<usize>();
        if change>1e-8||canonical.0.len()!=shape.0.0.len()||holes(&canonical)!=holes(&shape.0) {return Err(error("BOARD_ARRANGEMENT",&format!("physical copper set{i} changed area/topology: relative area={change:e}, components={}/{}, holes={}/{}; no filtering permitted",canonical.0.len(),shape.0.0.len(),holes(&canonical),holes(&shape.0))));}
        max_copper_area_change=max_copper_area_change.max(change);shape.0=canonical;
    }
    for(i,e)in r.electrodes.iter().enumerate(){electrode_polygons.insert(e.name.clone(),member_set(2+physical_count+i));}
    let origin_mm=roi_bounds[0];
    let mut cdt=ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    // Every ring, including all overlapping source boundaries, constrains the
    // one XY arrangement. Split intersections rather than centroid ownership on
    // an unconstrained triangulation. No feature/area threshold is applied.
    for poly in atoms.iter().map(|a|&a.poly) {for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {for line in ring.lines() {
        check(start,max_wall)?;
        if cdt.num_vertices()+cdt.num_undirected_edges()+2>budget.max_nodes {return Err(error("MEMORY_LIMIT","board constraint intersection preflight"));}
        budget.check((cdt.num_vertices()+cdt.num_undirected_edges()+2).checked_mul(4096).ok_or_else(||error("MEMORY_LIMIT","arrangement overflow"))?)?;
        let a=cdt.insert(Point2::new(line.start.x,line.start.y)).map_err(|e|error("BOARD_MESH",&e.to_string()))?;
        let b=cdt.insert(Point2::new(line.end.x,line.end.y)).map_err(|e|error("BOARD_MESH",&e.to_string()))?;
        if a!=b {cdt.add_constraint_and_split(a,b,|p|p);}
    }}}
    let max_xy=budget.max_nodes.min(budget.max_bytes/4096);
    let refinement=cdt.refine(RefinementParameters::new().keep_constraint_edges().with_angle_limit(AngleLimit::from_deg(r.min_triangle_angle_deg.unwrap_or(30.))).with_max_allowed_area(r.max_triangle_area_m2*1e6).with_max_additional_vertices(max_xy.saturating_sub(cdt.num_vertices())));
    if !refinement.refinement_complete {return Err(error("MEMORY_LIMIT","board XY mesh refinement incomplete within budget"));}
    check(start,max_wall)?;
    // Insert only air points strictly inside retained native bore polygons.
    // Existing constrained Cu/bore/contact vertices and all source faces stay.
    if let Some(rings)=r.bore_refinement_rings {
        use geo::Centroid;
        for bore in &bores.0 {
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
        if cdt.num_vertices()+4*divisions>max_xy {return Err(error("MEMORY_LIMIT","graded air ring exceeds node budget"));}
        for edge in 0..4 {for i in 0..divisions {let t=i as f64/divisions as f64;let a=corners[edge];let b=corners[(edge+1)%4];cdt.insert(Point2::new(a[0]+t*(b[0]-a[0]),a[1]+t*(b[1]-a[1]))).map_err(|e|error("BOARD_MESH",&e.to_string()))?;}}
        if distance==margin {break;}distance=(2.*distance).min(margin);
    }
    let xy_m:Vec<_>=cdt.vertices().map(|v|[(v.position().x-origin_mm[0])*1e-3,(v.position().y-origin_mm[1])*1e-3]).collect();
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
    let mut z_m:Vec<_>=air_distances.iter().rev().map(|v|-v).collect();z_m.push(0.);
    for span in &stack_spans {let pieces=((span[1]-span[0])/r.max_vertical_step_m).ceil() as usize;for i in 1..=pieces {z_m.push(if i==pieces {span[1]}else{span[0]+(span[1]-span[0])*i as f64/pieces as f64});}}
    if let Some(step)=r.interface_step_m {for (layer,span) in r.stack.iter().zip(&stack_spans) {if !layer.copper {let mut distance=step;while distance<(span[1]-span[0])/2. {let lo=span[0]+distance;let hi=span[1]-distance;if lo==span[0]||hi==span[1] {return Err(error("BOARD_PROFILE","interface step below representable vertical resolution"));}z_m.extend([lo,hi]);distance*=r.interface_growth.unwrap_or(2.);}}}}
    z_m.extend(air_distances.iter().map(|d|z+d));
    z_m.sort_unstable_by(f64::total_cmp);z_m.dedup();
    let count=triangles.len().checked_mul(z_m.len()-1).and_then(|v|v.checked_mul(3)).ok_or_else(||error("MEMORY_LIMIT","board tetra count overflow"))?;
    budget.check(count.checked_mul(2048).ok_or_else(||error("MEMORY_LIMIT","board tetra memory overflow"))?)?;
    if count>budget.max_tetrahedra || xy_m.len().checked_mul(z_m.len()).is_none_or(|n|n>budget.max_nodes) {return Err(error("MEMORY_LIMIT","board extrusion exceeds budget"));}
    let mut materials=vec![];let mut sigma_per_triangle=vec![];
    for slab in z_m.windows(2) {
        let middle=(slab[0]+slab[1])/2.;let layer=r.stack.iter().zip(&stack_spans).find(|(_,s)|middle>s[0]&&middle<s[1]).map(|(l,_)|l);
        let mut row=vec![];let mut sigma=vec![];
        for (ti,p) in centres.iter().enumerate() {
            // Physical ownership is constant inside each constrained material
            // face; provenance can cover only part of that same face.
            let physical:Vec<_>=physical_shapes.iter().filter(|(poly,span,_,_)|middle>span[0]&&middle<span[1]&&poly.contains(p)).collect();
            let copper=!physical.is_empty();
            let owners:Vec<_>=if copper {overlaps[ti].iter().map(|&i|&shapes[i]).filter(|s|middle>s.span[0]&&middle<s.span[1]).collect()}else{vec![]};
            if copper&&owners.is_empty() {return Err(error("BOARD_OWNERSHIP",&format!("canonical copper face{ti} at {p:?}, z={middle:e} has no positive-area source attribution; no copper removed")));}
            if !copper {
                let dielectric=layer.filter(|l|!l.copper&&substrate.contains(p)&&!bores.contains(p));
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
    let mesh=mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m,materials},budget)?;
    let conductivity_s_m=mesh.copper_tetrahedra.iter().map(|t|sigma_per_triangle[t.slab][t.planar_triangle]).collect();
    let mut certified_electrodes=BTreeMap::new();let mut electrode_areas_m2=BTreeMap::new();let mut max_contact_area_change:f64=0.;
    for e in &r.electrodes {
        let poly=&electrode_polygons[&e.name];let z=spans[&e.layer][if e.upper {1}else{0}];
        let mut nodes=std::collections::BTreeSet::new();let mut area=0.;
        for face in &mesh.copper_interfaces {
            if !face.nodes.iter().all(|&i|mesh.nodes_m[i][2]==z) {continue;}
            let xy=face.nodes.map(|i|[mesh.nodes_m[i][0],mesh.nodes_m[i][1]]);
            let centre=Point::new(origin_mm[0]+1e3*xy.iter().map(|p|p[0]).sum::<f64>()/3.,origin_mm[1]+1e3*xy.iter().map(|p|p[1]).sum::<f64>()/3.);
            if poly.contains(&centre) {
                let [[ax,ay],[bx,by],[cx,cy]]=xy;area+=((bx-ax)*(cy-ay)-(by-ay)*(cx-ax)).abs()/2.;nodes.extend(face.nodes);
            }
        }
        let expected=original_contact_areas[&e.name]*1e-6;
        if nodes.len()<3||(area-expected).abs()>1e-8*expected {return Err(error("BOARD_ELECTRODE",&format!("finite exposed contact area mismatch: {} mesh={area:e} source={expected:e}",e.name)));}
        max_contact_area_change=max_contact_area_change.max((area-expected).abs()/expected);
        electrode_areas_m2.insert(e.name.clone(),area);certified_electrodes.insert(e.name.clone(),(e.clone(),nodes.into_iter().collect()));
    }
    check(start,max_wall)?;
    Ok(BoardMesh{mesh,conductivity_s_m,geometry,origin_mm,source_mapping,barrel_ids,elapsed_seconds:start.elapsed().as_secs_f64(),layer_spans:spans,electrode_areas_m2,arrangement_atoms:atoms.len(),max_copper_area_change,max_contact_area_change,certified_electrodes})
}
impl BoardMesh {
    /// End geometry preparation before a separately budgeted field phase.
    pub fn into_mesh(self)->LayeredMesh{self.mesh}
    pub fn electrode_nodes(&self,e:&Electrode)->Result<Vec<usize>> {
        let (descriptor,nodes)=self.certified_electrodes.get(&e.name).ok_or_else(||error("BOARD_ELECTRODE","electrode was not part of the conforming mesh request"))?;
        if descriptor!=e {return Err(error("BOARD_ELECTRODE","electrode descriptor differs from its certified mesh contour"));}Ok(nodes.clone())
    }
}
