//! Board-owned, shared-overlay-grid material arrangement for volume extraction.
//! Every intersecting source is retained, including floating neighbouring copper.
use super::{copper_skin_profile,error,mesh_coarsen,mesh::{self,LayeredInput,LayeredMesh,Material},AllocationBudget,Result};
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
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub return_boundary_mm:Option<[[f64;3];2]>,
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
    pub(super) return_boundary_mm:Option<[[f64;3];2]>,
    pub(super) copper_skin_layers:Vec<CopperSkinLayer>,
    pub(super) copper_skin_barrels:Vec<CopperSkinBarrel>,
    pub mesh:LayeredMesh,
    /// Prepared physical regions retained for independent field adapters.
    pub(super) field_substrate: MultiPolygon<f64>,
    pub(super) field_bores: MultiPolygon<f64>,
    pub(super) field_partial_bores: Vec<([f64;2], MultiPolygon<f64>)>,
    pub(super) field_copper: Vec<(MultiPolygon<f64>, [f64;2], bool)>,
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
    pub hybrid_mesh_masks:Vec<serde_json::Value>,
    /// Far-field mesh coarsening parameters (approximation tier); None when off or not applicable.
    pub mesh_coarsening:Option<serde_json::Value>,
    /// Channel-neighbourhood ROI fallback evidence (approximation tier); None when not triggered.
    pub roi_fallback:Option<serde_json::Value>,
    certified_electrodes:BTreeMap<String,(Electrode,Vec<usize>)>,
}
/// Sutherland-Hodgman crop of a closed ring to an axis-aligned box (exact on the box:
/// the cropped region equals the ring region intersected with the box).
/// Boundary segment oriented so the area interior lies on its left.
pub(super) struct Seg {pub(super) a:[f64;2],pub(super) b:[f64;2]}
impl rstar::RTreeObject for Seg {
    type Envelope=rstar::AABB<[f64;2]>;
    fn envelope(&self)->Self::Envelope {rstar::AABB::from_corners(self.a,self.b)}
}
impl rstar::PointDistance for Seg {
    fn distance_2(&self,p:&[f64;2])->f64 {let (_,d2)=seg_project(self.a,self.b,*p);d2}
}
/// Parameter of the closest point on a-b and the squared distance to it.
fn seg_project(a:[f64;2],b:[f64;2],p:[f64;2])->(f64,f64) {
    let (dx,dy)=(b[0]-a[0],b[1]-a[1]);let l2=dx*dx+dy*dy;
    let t=if l2>0. {(((p[0]-a[0])*dx+(p[1]-a[1])*dy)/l2).clamp(0.,1.)} else {0.};
    let (cx,cy)=(a[0]+t*dx,a[1]+t*dy);
    (t,(p[0]-cx).powi(2)+(p[1]-cy).powi(2))
}
pub(super) type SegTree=rstar::RTree<Seg>;
pub(super) fn build_seg_tree(m:&MultiPolygon<f64>)->SegTree {
    use super::planar_trace::{ring_loops,signed_area};
    let mut segs=vec![];
    for p in &m.0 {
        for (k,ring) in std::iter::once(p.exterior()).chain(p.interiors()).enumerate() {
            let pts=&ring.0[..ring.0.len().saturating_sub(1)];
            let net=signed_area(pts).signum();
            // A pinched (self-touching) ring is several simple loops; orient each by its own sign:
            // loops of the ring's net orientation are shells (holes for k>0), the others are the reverse.
            for lp in ring_loops(ring) {
                let sigma=signed_area(&lp).signum();
                if sigma==0. {continue;}
                let shell_like=if k==0 {sigma==net} else {sigma!=net};
                let flip=(sigma>0.)!=shell_like;
                for i in 0..lp.len() {
                    let (a,b)=(lp[i],lp[(i+1)%lp.len()]);
                    let (a,b)=([a.x,a.y],[b.x,b.y]);
                    segs.push(if flip {Seg{a:b,b:a}} else {Seg{a,b}});
                }
            }
        }
    }
    rstar::RTree::bulk_load(segs)
}
/// Point-in-area from the nearest boundary segment (O(log n)); a nearest point at a segment end is
/// ambiguous, so those fall back to an even-odd ray count.
pub(super) fn point_in_segments(tree:&SegTree,x:f64,y:f64)->bool {
    let Some(g)=tree.nearest_neighbor(&[x,y]) else {return false};
    let (t,_)=seg_project(g.a,g.b,[x,y]);
    if t>1e-9&&t<1.-1e-9 {
        return (g.b[0]-g.a[0])*(y-g.a[1])-(g.b[1]-g.a[1])*(x-g.a[0])>0.;
    }
    let mut inside=false;
    for g in tree.locate_in_envelope_intersecting(&rstar::AABB::from_corners([x,y],[f64::MAX,y])) {
        let (a,b)=(g.a,g.b);
        if (a[1]>y)!=(b[1]>y) {
            let xi=a[0]+(y-a[1])*(b[0]-a[0])/(b[1]-a[1]);
            if xi>x {inside=!inside;}
        }
    }
    inside
}
/// Closed segment/triangle intersection (endpoint inside, or crossing/touching any triangle edge).
fn segment_meets_triangle(a:[f64;2],b:[f64;2],t:&[[f64;2];3])->bool {
    let orient=|p:[f64;2],q:[f64;2],r:[f64;2]|(q[0]-p[0])*(r[1]-p[1])-(q[1]-p[1])*(r[0]-p[0]);
    let in_tri=|p:[f64;2]|{let d=[orient(t[0],t[1],p),orient(t[1],t[2],p),orient(t[2],t[0],p)];d.iter().all(|&v|v>=0.)||d.iter().all(|&v|v<=0.)};
    if in_tri(a)||in_tri(b) {return true;}
    let on=|p:[f64;2],q:[f64;2],r:[f64;2]|r[0]>=p[0].min(q[0])&&r[0]<=p[0].max(q[0])&&r[1]>=p[1].min(q[1])&&r[1]<=p[1].max(q[1]);
    (0..3).any(|k|{
        let (c,d)=(t[k],t[(k+1)%3]);
        let (o1,o2,o3,o4)=(orient(a,b,c),orient(a,b,d),orient(c,d,a),orient(c,d,b));
        (o1*o2<0.&&o3*o4<0.)||(o1==0.&&on(a,b,c))||(o2==0.&&on(a,b,d))||(o3==0.&&on(c,d,a))||(o4==0.&&on(c,d,b))
    })
}
fn crop_ring(ring:&[geo::Coord<f64>],lo:[f64;2],hi:[f64;2])->Vec<geo::Coord<f64>> {
    let mut pts:Vec<geo::Coord<f64>>=if ring.len()>1&&ring.first()==ring.last(){ring[..ring.len()-1].to_vec()}else{ring.to_vec()};
    for side in 0..4 {
        if pts.is_empty(){break;}
        let (axis,bound,keep_ge)=match side{0=>(0,lo[0],true),1=>(0,hi[0],false),2=>(1,lo[1],true),_=>(1,hi[1],false)};
        let val=|c:&geo::Coord<f64>|if axis==0{c.x}else{c.y};
        let inside=|c:&geo::Coord<f64>|if keep_ge{val(c)>=bound}else{val(c)<=bound};
        let mut out=Vec::with_capacity(pts.len()+4);
        for i in 0..pts.len() {
            let (a,b)=(pts[i],pts[(i+1)%pts.len()]);
            let (ia,ib)=(inside(&a),inside(&b));
            if ia{out.push(a);}
            if ia!=ib {
                let t=(bound-val(&a))/(val(&b)-val(&a));
                let mut c=geo::Coord{x:a.x+t*(b.x-a.x),y:a.y+t*(b.y-a.y)};
                if axis==0{c.x=bound}else{c.y=bound}
                out.push(c);
            }
        }
        pts=out;
    }
    if pts.len()<3{return vec![];}
    pts.push(pts[0]);pts
}
fn crop_cut(local:&[&Polygon<f64>],a:&geo::Rect<f64>,pad:f64)->MultiPolygon<f64> {
    let (lo,hi)=([a.min().x-pad,a.min().y-pad],[a.max().x+pad,a.max().y+pad]);
    MultiPolygon(local.iter().flat_map(|&p|{
        let Some(b)=p.bounding_rect() else {return vec![]};
        if b.min().x>=lo[0]&&b.min().y>=lo[1]&&b.max().x<=hi[0]&&b.max().y<=hi[1]{return vec![p.clone()];}
        let ext=crop_ring(&p.exterior().0,lo,hi);
        if ext.is_empty(){return vec![];}
        // Clipped rings carry zero-width bridges along the box edge, and a clipped hole can share
        // edges with the clipped shell. Such a Polygon(shell,holes) is invalid and the overlay may read
        // it as solid (STM32G0 set1 gained ~1.3 mm2). Resolve shell minus holes by overlay on the
        // small clipped rings only.
        let shell=areas::one(Polygon::new(LineString::new(ext),vec![]));
        let holes=p.interiors().iter().map(|r|crop_ring(&r.0,lo,hi)).filter(|r|!r.is_empty()).map(|r|Polygon::new(LineString::new(r),vec![])).collect::<Vec<_>>();
        if holes.is_empty() {return shell.0;}
        areas::difference(&shell,&areas::union(holes)).0
    }).collect())
}
struct Shape {poly:MultiPolygon<f64>,id:String,sigma:f64,span:[f64;2]}
/// One arrangement cell. `inside` lists, in increasing order, the cut indices that contain it
/// (sparse, so cuts that do not touch a cell cost it nothing).
struct Atom {poly:std::sync::Arc<Polygon<f64>>,inside:Vec<usize>,bbox:Option<geo::Rect<f64>>,verts:usize}
impl Atom {
    fn new(poly:Polygon<f64>,inside:Vec<usize>)->Self {let bbox=poly.bounding_rect();let verts=std::iter::once(poly.exterior()).chain(poly.interiors()).map(|r|r.0.len()).sum();Atom{poly:std::sync::Arc::new(poly),inside,bbox,verts}}
    fn member(&self,cut:usize)->bool {self.inside.binary_search(&cut).is_ok()}
}
fn rects_disjoint(a:&geo::Rect<f64>,b:&geo::Rect<f64>)->bool {a.max().x<b.min().x||b.max().x<a.min().x||a.max().y<b.min().y||b.max().y<a.min().y}
fn common_partition(roi:&MultiPolygon<f64>,cuts:&[MultiPolygon<f64>],budget:AllocationBudget,start:Instant,limit:Duration)->Result<Vec<Atom>> {
    let mut atoms=roi.0.iter().cloned().map(|poly|Atom::new(poly,vec![])).collect::<Vec<_>>();
    for (cut_index,cut) in cuts.iter().enumerate() {
        let cut_bboxes:Vec<Option<geo::Rect<f64>>>=cut.0.iter().map(|p|p.bounding_rect()).collect();
        let Some(cut_box)=cut_bboxes.iter().flatten().copied().reduce(|a,b|geo::Rect::new(
            geo::Coord{x:a.min().x.min(b.min().x),y:a.min().y.min(b.min().y)},geo::Coord{x:a.max().x.max(b.max().x),y:a.max().y.max(b.max().y)})) else {continue};
        let mut next=Vec::with_capacity(atoms.len());
        // Boundary-segment index: an atom whose bbox meets no cut segment is
        // wholly inside or outside the cut, so no overlay or relate is needed.
        let seg_index=rstar::RTree::bulk_load(cut.0.iter().flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors()))
            .flat_map(|r|r.0.windows(2).map(|w|rstar::primitives::Rectangle::from_corners([w[0].x,w[0].y],[w[1].x,w[1].y])).collect::<Vec<_>>()).collect::<Vec<_>>());
        let (mut fast,mut slow)=(0usize,0usize);
        let cut_vertices=cut.0.iter().map(|p|std::iter::once(p.exterior()).chain(p.interiors()).map(|r|r.0.len()).sum::<usize>()).sum::<usize>();
        let total=atoms.len();
        for atom in std::mem::take(&mut atoms) {
            // Disjoint from every cut polygon: exactly outside, membership unchanged.
            if atom.bbox.is_none_or(|a|rects_disjoint(&a,&cut_box)) {next.push(atom);continue;}
            check(start,limit)?;
            budget.check((atom.verts+cut_vertices).checked_mul(4096).and_then(|v|v.checked_add((total+next.len()).checked_mul(4096)?)).ok_or_else(||error("MEMORY_LIMIT","common arrangement plan overflow"))?)?;
            let a=atom.bbox.unwrap();
            let env=rstar::AABB::from_corners([a.min().x-1e-6,a.min().y-1e-6],[a.max().x+1e-6,a.max().y+1e-6]);
            if seg_index.locate_in_envelope_intersecting(&env).next().is_none() {
                // Every ring vertex lies off the cut boundary, so one classifies the whole atom.
                let v=atom.poly.exterior().0[0];
                let inside=cut.0.iter().zip(&cut_bboxes).any(|(p,b)|b.is_some_and(|b|v.x>=b.min().x&&v.x<=b.max().x&&v.y>=b.min().y&&v.y<=b.max().y)&&p.contains(&Point::new(v.x,v.y)));
                let mut atom=atom;if inside{atom.inside.push(cut_index);}
                next.push(atom);
                if next.len()>budget.max_nodes {return Err(error("MEMORY_LIMIT","common arrangement polygon budget"));}
                fast+=1;continue;
            }
            slow+=1;
            let cell=areas::one((*atom.poly).clone());
            // Exact geometric no-ops must not repeatedly renode an accepted
            // boundary. Only cut polygons whose bbox overlaps (or touches) the
            // atom bbox can interact with it; the rest are exactly disjoint.
            // Large cut contours are cropped to the atom bbox first (exact within it).
            let near:Vec<&Polygon<f64>>=cut.0.iter().zip(&cut_bboxes).filter(|(_,b)|b.is_some_and(|b|!rects_disjoint(&a,&b))).map(|(p,_)|p).collect();
            let local=if near.iter().map(|p|p.exterior().0.len()).sum::<usize>()>256 {crop_cut(&near,&a,1e-6)} else {MultiPolygon(near.into_iter().cloned().collect())};
            // geo's `relate` misreported whole cells (NanoScope set3 lost 0.104 mm2: a fully covered
            // cell classed as outside), so decide the no-op cases from the exact overlay areas instead.
            let inter=if local.0.is_empty() {MultiPolygon(vec![])} else {areas::intersection(&cell,&local)};
            let (cell_area,inter_area)=(atom.poly.unsigned_area(),inter.unsigned_area());
            let unchanged=if inter_area<=1e-12*cell_area {Some(false)} else if cell_area-inter_area<=1e-12*cell_area {Some(true)} else {None};
            if let Some(inside)=unchanged {
                let mut atom=atom;if inside{atom.inside.push(cut_index);}
                next.push(atom);
                if next.len()>budget.max_nodes {return Err(error("MEMORY_LIMIT","common arrangement polygon budget"));}
                continue;
            }
            for (set,inside) in [(inter,true),(areas::difference(&cell,&local),false)] {
                for poly in set.0 {
                    // Native overlay has no positive-area cutoff. Every output
                    // polygon is retained, including disconnected/floating Cu.
                    if poly.unsigned_area()<=0. {return Err(error("BOARD_ARRANGEMENT","nonpositive native partition polygon"));}
                    let mut inside_cuts=atom.inside.clone();if inside{inside_cuts.push(cut_index);}
                    next.push(Atom::new(poly,inside_cuts));
                    if next.len()>budget.max_nodes {return Err(error("MEMORY_LIMIT","common arrangement polygon budget"));}
                }
            }
        }atoms=next;
        if std::env::var_os("P7_PHASE_TRACE").is_some() && (cut_index<8||cut_index+1==cuts.len()) {
            super::board_foil::phase_trace("partition-cut-done",start,serde_json::json!({"cut":cut_index,"of":cuts.len(),"atoms":atoms.len(),"fast":fast,"slow":slow}));
        }
    }Ok(atoms)
}
pub(super) fn polygon(p:&geometry::PolygonSnapshot)->Polygon<f64> {
    Polygon::new(LineString::from(p.outer.clone()),p.holes.iter().map(|r|LineString::from(r.clone())).collect())
}
fn rect(b:[[f64;2];2])->Polygon<f64> {Polygon::new(LineString::from(vec![b[0],[b[1][0],b[0][1]],b[1],[b[0][0],b[1][1]]]),vec![])}
fn check(start:Instant,limit:Duration)->Result<()> {if start.elapsed()>limit {Err(error("DEADLINE","board material arrangement deadline"))}else{Ok(())}}
/// Nearest point on the slot axis segment `center +- half_axis` (the centre for a round hole).
fn axis_point(p:[f64;2],center:[f64;2],half_axis:[f64;2])->[f64;2] {
    let square=half_axis[0]*half_axis[0]+half_axis[1]*half_axis[1];
    if square==0. {return center;}
    let t=(((p[0]-center[0])*half_axis[0]+(p[1]-center[1])*half_axis[1])/square).clamp(-1.,1.);
    [center[0]+t*half_axis[0],center[1]+t*half_axis[1]]
}
/// Half axis vector of a slot (same convention as geometry::primitives: total length along the
/// slot's local x, rotated by pad rotation + drill rotation); zero for a round drill.
fn slot_half_axis(drill:&crate::model::Drill,pad_rotation_deg:f64)->[f64;2] {
    let crate::model::DrillShape::Slot{length_mm}=drill.shape else {return [0.;2]};
    let half=((length_mm-drill.diameter_mm)/2.).max(0.);
    let (s,c)=(pad_rotation_deg+drill.rotation_deg.unwrap_or(0.)).to_radians().sin_cos();
    [half*c,half*s]
}
fn perimeter_mm(shape:&MultiPolygon<f64>)->f64 {
    shape.0.iter().flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).flat_map(|r|r.lines()).map(|e|(e.end.x-e.start.x).hypot(e.end.y-e.start.y)).sum()
}
fn ring_perimeter_mm(ring:&geo::LineString<f64>)->f64 {
    ring.lines().map(|e|(e.end.x-e.start.x).hypot(e.end.y-e.start.y)).sum()
}
/// A ring is grid rounding noise when its area is within k*h*P (P its own perimeter,
/// k=16 as in `repair_metal_polygon`) or it is a crack of width 2A/P <= 4h.
fn ring_is_noise(ring:&geo::LineString<f64>,grid_step_mm:f64)->bool {
    let p=ring_perimeter_mm(ring);let a=Polygon::new(ring.clone(),vec![]).unsigned_area();
    a<=16.*grid_step_mm*p||2.*a<=4.*grid_step_mm*p
}
/// (components, holes) after removing grid-noise components and holes. Pure counting:
/// the geometry itself is never modified. Returns (raw, filtered).
fn noise_filtered_topology(shape:&MultiPolygon<f64>,grid_step_mm:f64)->((usize,usize),(usize,usize),usize) {
    let raw=(shape.0.len(),shape.0.iter().map(|p|p.interiors().len()).sum::<usize>());
    let (mut components,mut holes,mut dropped_holes)=(0usize,0usize,0usize);
    for p in &shape.0 {
        if ring_is_noise(p.exterior(),grid_step_mm) {dropped_holes+=p.interiors().len();continue;}
        components+=1;
        for h in p.interiors() {if ring_is_noise(h,grid_step_mm){dropped_holes+=1}else{holes+=1}}
    }
    (raw,(components,holes),dropped_holes)
}
/// Real (non-noise) components and holes as (area, perimeter), each sorted by descending area.
fn noise_pieces(shape:&MultiPolygon<f64>,grid_step_mm:f64)->(Vec<(f64,f64)>,Vec<(f64,f64)>) {
    let (mut comps,mut holes)=(vec![],vec![]);
    let piece=|r:&geo::LineString<f64>|(Polygon::new(r.clone(),vec![]).unsigned_area(),ring_perimeter_mm(r));
    for p in &shape.0 {
        if ring_is_noise(p.exterior(),grid_step_mm) {continue;}
        comps.push((p.unsigned_area(),piece(p.exterior()).1));
        for h in p.interiors() {if !ring_is_noise(h,grid_step_mm){holes.push(piece(h));}}
    }
    for v in [&mut comps,&mut holes] {v.sort_by(|x,y|y.0.total_cmp(&x.0));}
    (comps,holes)
}
/// Sorted-area matching: the N largest pieces must agree pairwise within 16h(Pa+Pb)+1e-9A, and the
/// surplus on the longer side must total <= 1e-6 of the set area. Ok((extra count, surplus area)).
fn match_sorted_pieces(now:&[(f64,f64)],src:&[(f64,f64)],grid_step_mm:f64,set_area:f64)->std::result::Result<(usize,f64),String> {
    let n=now.len().min(src.len());
    for k in 0..n {
        let (a,pa)=now[k];let (b,pb)=src[k];
        if (a-b).abs()>16.*grid_step_mm*(pa+pb)+1e-9*a.max(b) {let w=|v:&[(f64,f64)]|v[k.saturating_sub(3)..(k+4).min(v.len())].iter().map(|(a,p)|format!("{a:.6e}/{p:.3e}")).collect::<Vec<_>>().join(" ");
            return Err(format!("piece {k} area {a:e}/{b:e} (perimeter {pa:e}/{pb:e}) differs: split or merged; rebuilt window [{}] source window [{}]; count {}/{}",w(now),w(src),now.len(),src.len()));}
    }
    let longer=if now.len()>=src.len(){now}else{src};
    let surplus=&longer[n..];let total:f64=surplus.iter().map(|p|p.0).sum();
    if total>1e-6*set_area {
        let top=surplus.iter().take(5).map(|(a,p)|format!("(area={a:e},perimeter={p:e},width={:e})",2.*a/p)).collect::<Vec<_>>().join(",");
        return Err(format!("surplus {} pieces total {total:e} > 1e-6*{set_area:e}; largest {top}",surplus.len()));
    }
    Ok((surplus.len(),total))
}
/// Spatial component match: every rebuilt real component is assigned to the real source component
/// that contains one of its interior points. Per source component the pieces must conserve area
/// within 16h(P_src+P_dom)+1e-9A with one dominant piece; all non-dominant (or unassigned) pieces,
/// i.e. cracks that detach a sliver, total <= 1e-6 of the set area. A source component without a
/// rebuilt piece (merge) or a split into large parts is refused. Ok((extra pieces, surplus area)).
fn real_polys(m:&MultiPolygon<f64>,h:f64)->Vec<&Polygon<f64>> {m.0.iter().filter(|p|!ring_is_noise(p.exterior(),h)).collect()}
fn match_components_spatial(now:&MultiPolygon<f64>,src:&MultiPolygon<f64>,grid_step_mm:f64,set_area:f64)->std::result::Result<(usize,f64,usize),String> {
    use geo::{InteriorPoint,Distance,Euclidean};
    let (rn,rs)=(real_polys(now,grid_step_mm),real_polys(src,grid_step_mm));
    let boxes=rs.iter().map(|p|p.bounding_rect()).collect::<Vec<_>>();
    let mut groups:Vec<Vec<&Polygon<f64>>>=vec![vec![];rs.len()];let mut merged=0usize;let mut unassigned:Vec<(f64,f64)>=vec![];
    for p in &rn {
        let piece=(p.unsigned_area(),ring_perimeter_mm(p.exterior()));
        let hit=p.interior_point().and_then(|pt|(0..rs.len()).find(|&j|boxes[j].is_some_and(|b|pt.x()>=b.min().x&&pt.x()<=b.max().x&&pt.y()>=b.min().y&&pt.y()<=b.max().y)&&rs[j].contains(&pt)));
        match hit {Some(j)=>groups[j].push(*p),None=>unassigned.push(piece)}
    }
    let mut surplus:Vec<(f64,f64)>=unassigned;
    for (j,g) in groups.iter_mut().enumerate() {
        let (a_src,p_src)=(rs[j].unsigned_area(),ring_perimeter_mm(rs[j].exterior()));
        if g.is_empty() {return Err(format!("source component {j} (area {a_src:e}) has no rebuilt piece: merged or lost"));}
        // Pieces closer than 4h (touching at a point or separated by a sub-grid crack) are one
        // conductor of the source; cluster them and keep only truly separate clusters as surplus.
        let n=g.len();let mut parent:Vec<usize>=(0..n).collect();
        fn find(p:&mut Vec<usize>,x:usize)->usize{let mut r=x;while p[r]!=r{r=p[r];}let mut c=x;while p[c]!=r{let nx=p[c];p[c]=r;c=nx;}r}
        for x in 0..n {for y in x+1..n {
            let (bx,by)=(g[x].bounding_rect(),g[y].bounding_rect());let d=4.*grid_step_mm;
            if let (Some(bx),Some(by))=(bx,by) {if bx.max().x+d<by.min().x||by.max().x+d<bx.min().x||bx.max().y+d<by.min().y||by.max().y+d<bx.min().y {continue;}}
            if Euclidean.distance(g[x].exterior(),g[y].exterior())<=d {let (rx,ry)=(find(&mut parent,x),find(&mut parent,y));if rx!=ry{parent[rx]=ry;merged+=1;}}
        }}
        let mut clusters:BTreeMap<usize,(f64,f64)>=BTreeMap::new();
        for x in 0..n {let r=find(&mut parent,x);let e=clusters.entry(r).or_insert((0.,0.));e.0+=g[x].unsigned_area();e.1+=ring_perimeter_mm(g[x].exterior());}
        let mut areas_sorted:Vec<(f64,f64)>=clusters.into_values().collect();areas_sorted.sort_by(|x,y|y.0.total_cmp(&x.0));
        let total:f64=areas_sorted.iter().map(|c|c.0).sum();
        if (total-a_src).abs()>16.*grid_step_mm*(2.*p_src)+1e-9*a_src {return Err(format!("source component {j} area {a_src:e} vs rebuilt pieces {total:e}"));}
        surplus.extend(areas_sorted.into_iter().skip(1));
    }
    surplus.sort_by(|x,y|y.0.total_cmp(&x.0));
    let total:f64=surplus.iter().map(|p|p.0).sum();
    if total>1e-6*set_area {
        let top=surplus.iter().take(5).map(|(a,p)|format!("(area={a:e},ref_perimeter={p:e})")).collect::<Vec<_>>().join(",");
        return Err(format!("surplus {} pieces total {total:e} > 1e-6*{set_area:e}; largest {top}",surplus.len()));
    }
    Ok((surplus.len(),total,merged))
}
/// Relative 1e-8 bound, or the arrangement grid's rounding noise: |dA| <= step * perimeter.
fn area_change_ok(abs_change:f64,original:f64,grid_step_mm:f64,perimeter_mm:f64)->bool {
    abs_change<=1e-8*original||abs_change<=grid_step_mm*perimeter_mm
}
fn barrel_outer(bore:&MultiPolygon<f64>,center:[f64;2],half_axis:[f64;2],radius:f64,plating:f64,tolerance:f64)->Result<Polygon<f64>> {
    if bore.0.len()!=1||!bore.0[0].interiors().is_empty(){return Err(error("BOARD_PADSTACK","ordinary round bore must have one hole-free contour"));}
    let mut points=Vec::new();let ring=&bore.0[0].exterior().0;
    for point in &ring[..ring.len()-1] {let q=axis_point([point.x,point.y],center,half_axis);let dx=point.x-q[0];let dy=point.y-q[1];let r=dx.hypot(dy);if !r.is_finite()||r<=0.||(r-radius).abs()>tolerance{return Err(error("BOARD_PADSTACK","prepared bore exceeds ordinary round curve bound"));}points.push([point.x+plating*dx/r,point.y+plating*dy/r]);}
    let outer=Polygon::new(LineString::from(points),vec![]);let expected=radius+plating;
    for edge in outer.exterior().lines(){let q=axis_point([(edge.start.x+edge.end.x)/2.,(edge.start.y+edge.end.y)/2.],center,half_axis);let a=[edge.start.x-q[0],edge.start.y-q[1]];let b=[edge.end.x-q[0],edge.end.y-q[1]];let d=[b[0]-a[0],b[1]-a[1]];let square=d[0]*d[0]+d[1]*d[1];if square==0.{return Err(error("BOARD_PADSTACK","zero barrel contour edge"));}let t=(-(a[0]*d[0]+a[1]*d[1])/square).clamp(0.,1.);let closest=(a[0]+t*d[0]).hypot(a[1]+t*d[1]);let own=axis_point([edge.start.x,edge.start.y],center,half_axis);if ((edge.start.x-own[0]).hypot(edge.start.y-own[1])-expected).abs()>tolerance||(closest-expected).abs()>tolerance{return Err(error("BOARD_PADSTACK","coherent outer barrel exceeds the unchanged curve bound; refine geometry"));}}
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

#[derive(Debug,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(super) struct CopperSkinBarrel {
    id:String,
    skin_depth_m:f64,
    maximum_radial_step_m:f64,
    shells:usize,
}

pub(super) fn build_profiled(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration,
    support:Option<(&[[f64;2]],&[[usize;3]])>,trace_lines:&[[[f64;2];2]],skin:Option<copper_skin_profile::Profile>)->Result<BoardMesh> {
    build_profiled_impl(board,r,budget,max_wall,support,trace_lines,skin,false)
}
pub(super) fn build_profiled_hybrid(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration,skin:Option<copper_skin_profile::Profile>)->Result<BoardMesh> {
    build_profiled_impl(board,r,budget,max_wall,None,&[],skin,true)
}
fn build_profiled_impl(board:&Board,r:&Request,budget:AllocationBudget,max_wall:Duration,
    support:Option<(&[[f64;2]],&[[usize;3]])>,trace_lines:&[[[f64;2];2]],skin:Option<copper_skin_profile::Profile>,hybrid_masks:bool)->Result<BoardMesh> {
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
    super::board_foil::phase_trace("mesh-context",start,serde_json::json!({}));
    let _scope=Context::install(Some(context.clone()));
    let geometry=geometry::prepare(board,options);
    super::board_foil::phase_trace("mesh-prepared",start,serde_json::json!({}));
    check(start,max_wall)?;
    if geometry.outline.is_none() || geometry.findings.iter().any(|f|f.code!="net_islands") || !geometry.excluded.is_empty() || geometry.fills.iter().any(|f|f.status!="available") {
        return Err(error("BOARD_GEOMETRY","complete trustworthy copper, outline and native fills required"));
    }
    if !r.accept_unknown_fill_freshness&&geometry.fills.iter().any(|f|f.freshness.as_deref()!=Some("current")) {return Err(error("BOARD_FILL_POLICY","CAD fill freshness is not confirmed; an explicit captured-snapshot policy is required"));}
    // Approximation tier: when the whole ROI would exceed the memory budget, mesh only the channel neighbourhood.
    let fallback_plan=super::roi_fallback::plan(r,&geometry,budget)?;
    let (r,roi_fallback)=match &fallback_plan {Some(p)=>(&p.request,Some(p.evidence.clone())),None=>(r,None)};
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
    super::board_foil::phase_trace("mesh-substrate",start,serde_json::json!({}));
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
        // A region whose bbox misses the ROI bbox cannot intersect the domain.
        let rb=polygon(&region.polygon).bounding_rect();
        if rb.is_none_or(|b|b.max().x<roi_bounds[0][0]||b.min().x>roi_bounds[1][0]||b.max().y<roi_bounds[0][1]||b.min().y>roi_bounds[1][1]) {continue;}
        let poly=clip_domain(&areas::one(polygon(&region.polygon)));
        if poly.unsigned_area()==0. {continue;}
        if !r.allow_insulated_copper_cuts&&areas::difference(&areas::one(polygon(&region.polygon)),&roi).unsigned_area()>0. {return Err(error("BOARD_CUT_POLICY","ROI cuts copper; explicit insulated-cut boundary policy required"));}
        let layer=r.stack.iter().find(|l|l.copper&&l.name==region.layer).ok_or_else(||error("BOARD_PROFILE","region layer absent from stack"))?;
        source_mapping.push((region.id.clone(),region.source.path.clone(),region.source.net.clone(),region.layer.clone()));
        shapes.push(Shape{poly,id:region.id.clone(),sigma:layer.conductivity_s_m.unwrap(),span:spans[&region.layer]});
    }
    super::board_foil::phase_trace("mesh-shapes",start,serde_json::json!({"shapes":shapes.len()}));
    let mut bores=MultiPolygon(vec![]);let mut barrel_ids=vec![];
    let mut copper_skin_barrels=Vec::new();
    let mut barrel_shell_constraints=Vec::new();
    let mut barrel_constraint_work=0usize;
    // Keep one planar cut per partial bore span. XY ownership is shared by
    // the whole extrusion, but a drill only displaces material in its span.
    let mut partial_bores=BTreeMap::<(usize,usize),Vec<Polygon<f64>>>::new();
    let mut bore_polys:Vec<Polygon<f64>>=vec![];
    // Retain the conservative native bore and use its same angular phase for
    // the nominal outer radius. Independent inward/outward curve errors must
    // not be amplified across the thin plating annulus.
    for hole in &geometry.holes {
        let bore=areas::union(hole.polygons.iter().map(polygon));
        if !bore.intersects(&roi) {continue;}
        if !hole.span_confirmed {return Err(error("BOARD_PADSTACK","ROI hole requires a confirmed physical span"));}
        if !hole.plated{bore_polys.extend(bore.0);continue;}
        let (center,half_axis,radius,first,last)=if let Some(via)=board.vias.iter().find(|v|v.source.path==hole.source.path){
            ([via.at.x,via.at.y],[0.;2],via.drill_mm/2.,via.first_layer,via.last_layer)
        }else if let Some(pad)=board.pads.iter().find(|p|p.source.path==hole.source.path){
            let drill=pad.drill.as_ref().ok_or_else(||error("BOARD_PADSTACK","prepared pad bore has no authored drill"))?;
            if !drill.plated||pad.layers.len()!=board.layers.len()||(0..board.layers.len()).any(|l|!pad.layers.contains(&l)){return Err(error("BOARD_PADSTACK","confirmed round or slot through-pad stack required"));}
            let center=geometry::primitives::transform_point(drill.offset.unwrap_or(crate::model::Point{x:0.,y:0.}),pad.at,pad.rotation_deg);
            ([center.x,center.y],slot_half_axis(drill,pad.rotation_deg),drill.diameter_mm/2.,0,board.layers.len()-1)
        }else{return Err(error("BOARD_PADSTACK","plated bore has no explicit supported source padstack"));};
        let plating=r.finished_hole_plating_m.ok_or_else(||error("BOARD_PADSTACK","finished-hole plating thickness is missing"))?;
        let sigma=r.barrel_conductivity_s_m.ok_or_else(||error("BOARD_PADSTACK","barrel conductivity is missing"))?;
        let outer=barrel_outer(&bore,center,half_axis,radius,plating*1e3,options.max_deviation_mm+2.*geometry.boolean_error_bound_mm)?;
        if let Some(profile)=skin {
            let inner_ring=&bore.0[0].exterior().0;
            let outer_ring=&outer.exterior().0;
            if inner_ring.len()!=outer_ring.len()||inner_ring.len()<4 {
                return Err(error("BOARD_PROFILE","matching round barrel contours required for skin refinement"));
            }
            let vertices=inner_ring.len()-1;
            let thickness_m=inner_ring[..vertices].iter().zip(&outer_ring[..vertices])
                .map(|(a,b)|(a.x-b.x).hypot(a.y-b.y)*1e-3).fold(0.,f64::max);
            let plan=copper_skin_profile::radial_plan(profile,sigma,thickness_m,vertices)
                .ok_or_else(||error("BOARD_PROFILE","radial copper mesh spacing or constraint count is not representable"))?;
            barrel_constraint_work=copper_skin_profile::radial_constraint_room(
                barrel_constraint_work,plan.constraint_work,budget.max_nodes,budget.max_bytes)?;
            copper_skin_barrels.push(CopperSkinBarrel{id:hole.id.clone(),skin_depth_m:profile.skin_depth_m(sigma),
                maximum_radial_step_m:plan.maximum_step_m,shells:plan.pieces});
            if plan.pieces>1 {
                let vertices=inner_ring[..vertices].iter().zip(&outer_ring[..vertices])
                    .map(|(a,b)|([a.x,a.y],[b.x,b.y])).collect::<Vec<_>>();
                barrel_shell_constraints.push((hole.id.clone(),vertices,plan.pieces));
            }
        }
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
            bore_polys.extend(bore.0);
        }else{
            partial_bores.entry((first,last)).or_default().extend(bore.0);
        }
        barrel_ids.push(hole.id.clone());
    }
    bores=areas::union(bores.0.into_iter().chain(bore_polys));
    let mut partial_bores=partial_bores.into_iter().map(|((first,last),polys)| {
        let poly=areas::union(polys);
        let span=[spans[&board.layers[first].name][0],spans[&board.layers[last].name][1]];
        (span,poly)
    }).collect::<Vec<_>>();
    super::board_foil::phase_trace("mesh-bores",start,serde_json::json!({"holes":geometry.holes.len()}));
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
    super::board_foil::phase_trace("mesh-layer-unions",start,serde_json::json!({}));
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
    super::board_foil::phase_trace("mesh-electrodes",start,serde_json::json!({}));
    let original_contact_areas:BTreeMap<_,_>=electrode_polygons.iter().map(|(name,p)|(name.clone(),p.unsigned_area())).collect();
    // Approximation tier: relax the interior refinement far from the channel (see mesh_coarsen).
    let a0_mm2=r.max_triangle_area_m2*1e6;
    let mut mesh_coarsening=None;
    let coarse_field=if mesh_coarsen::disabled() {None} else {
        let electrode_keys:std::collections::HashSet<(&str,&str)>=r.electrodes.iter().flat_map(|e|e.source_path.iter().chain(e.source_paths.iter().flatten()).map(move|p|(p.as_str(),e.layer.as_str()))).collect();
        let net_of:std::collections::HashMap<&str,&str>=source_mapping.iter().filter_map(|m|m.2.as_deref().map(|n|(m.0.as_str(),n))).collect();
        let channel_nets:std::collections::BTreeSet<&str>=source_mapping.iter().filter(|m|electrode_keys.contains(&(m.1.as_str(),m.3.as_str()))).filter_map(|m|m.2.as_deref()).collect();
        let mut net_area=BTreeMap::<&str,f64>::new();
        for sh in &shapes {if let Some(n)=net_of.get(sh.id.as_str()).filter(|n|channel_nets.contains(*n)) {*net_area.entry(*n).or_default()+=sh.poly.unsigned_area();}}
        // With several channel nets the largest copper (the reference plane) is not support:
        // its return current spreads under the signal copper, which is support.
        let plane=if channel_nets.len()>1 {net_area.iter().max_by(|a,b|a.1.total_cmp(b.1)).map(|(n,_)|*n)} else {None};
        let wanted=|n:Option<&str>|n.is_some_and(|n|channel_nets.contains(n)&&Some(n)!=plane);
        let mut support:Vec<MultiPolygon<f64>>=electrode_polygons.values().cloned().collect();
        support.extend(shapes.iter().filter(|sh|wanted(net_of.get(sh.id.as_str()).copied())).map(|sh|sh.poly.clone()));
        for hole in &geometry.holes {
            let Some(sh)=shapes.iter().find(|sh|sh.id==hole.id&&barrel_ids.contains(&sh.id)) else {continue};
            let net=board.vias.iter().find(|v|v.source.path==hole.source.path).and_then(|v|v.net)
                .or_else(||board.pads.iter().find(|p|p.source.path==hole.source.path).and_then(|p|p.net))
                .and_then(|i|board.nets.get(i));
            if wanted(net.map(|n|n.as_str())) {support.push(sh.poly.clone());}
        }
        let diagonal=(roi_bounds[1][0]-roi_bounds[0][0]).hypot(roi_bounds[1][1]-roi_bounds[0][1]);
        let field=mesh_coarsen::SizeField::new(&support,a0_mm2.sqrt(),diagonal);
        if field.is_none() {
            let h0=a0_mm2.sqrt();
            mesh_coarsening=Some(serde_json::json!({"kind":"distance-graded-planar-mesh","tier":"approximation","active":false,"h0Mm":h0,"hMaxMm":mesh_coarsen::SizeField::h_max_for(h0,diagonal),"d0Mm":null,
                "reason":"coarsest size below 1.5 h0 (board diagonal / 50 or 32 h0) or no channel support; mesh unchanged"}));
        }
        field
    };
    let arrangement_context={
        // The final shared arrangement uses the same checked kernel range for
        // rectangular and polygonal domains. Neither inherits the coarser
        // clipping-only grid merely because its contour is rectangular.
        // Bounds include every Boolean input (not only Cu), so tightening the
        // integer-grid padding cannot put a real bore outside its adapter.
        let mut bounds=roi_bounds;
        for set in std::iter::once(&bores).chain(arrangement_sets.iter()).chain(partial_bores.iter().map(|(_,poly)|poly)) {
            if let Some(b)=set.bounding_rect(){for k in 0..2 {bounds[0][k]=bounds[0][k].min(if k==0{b.min().x}else{b.min().y});bounds[1][k]=bounds[1][k].max(if k==0{b.max().x}else{b.max().y});}}
        }
        context.refined_for_arrangement(bounds)?
    };
    let _local_scope=Context::install(Some(arrangement_context.clone()));
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
            let ids=source_mapping.iter().filter(|m|m.3==layer.layer).map(|m|&m.0).collect::<std::collections::HashSet<_>>();
            let copper=areas::union(shapes.iter().filter(|s|ids.contains(&&s.id)).flat_map(|s|s.poly.0.iter().cloned()));
            physical_shapes[i].0=copper.clone();arrangement_sets[i]=copper;
        }
    }
    // Approximation tier: simplify far copper rings (never within d0 of the channel support).
    let mut outline_simplification=None;
    if let Some(field)=&coarse_field {
        if std::env::var("P7_MESH_SIMPLIFY").as_deref()!=Ok("0") {
            let layers_n=geometry.layers.len();
            let keep=mesh_coarsen::Keepouts::new(std::iter::once(&bores).chain(partial_bores.iter().map(|(_,p)|p)).chain(physical_shapes[layers_n..].iter().map(|s|&s.0)));
            let total_area:f64=physical_shapes[..layers_n].iter().map(|s|s.0.unsigned_area()).sum();
            let mut budget=1e-4*total_area;let mut st=mesh_coarsen::SimplifyStats::default();
            for i in 0..layers_n {
                check(start,max_wall)?;
                let simplified=mesh_coarsen::simplify_set(&physical_shapes[i].0,field,&keep,&mut budget,&mut st);
                physical_shapes[i].0=simplified.clone();arrangement_sets[i]=simplified;
            }
            super::board_foil::phase_trace("mesh-simplified",start,serde_json::json!({"verticesBefore":st.vertices_before,"verticesAfter":st.vertices_after}));
            outline_simplification=Some(st);
        }
    }
    let owner_slack=if outline_simplification.as_ref().is_some_and(|st|st.rings_simplified>0) {coarse_field.as_ref().map_or(1e-2,|f|(f.h_max/4.*1.5).max(1e-2))} else {1e-2};
    let unowned_limit=if owner_slack>1e-2 {1e-4} else {1e-6};
    let original_physical_stats=physical_shapes.iter().map(|s|(s.0.unsigned_area(),s.0.0.len(),s.0.0.iter().map(|p|p.interiors().len()).sum::<usize>(),perimeter_mm(&s.0))).collect::<Vec<_>>();
    let grid_step_mm=arrangement_context.grid_step_mm();
    let original_filtered=physical_shapes.iter().map(|s|noise_pieces(&s.0,grid_step_mm)).collect::<Vec<_>>();
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
        let (area,components,holes,perimeter)=original_physical_stats[layer];let change=(restored.unsigned_area()-area).abs()/area;
        let restored_holes=restored.0.iter().map(|p|p.interiors().len()).sum::<usize>();
        if !area_change_ok(change*area,area,grid_step_mm,perimeter)||restored.0.len()!=components||restored_holes!=holes {return Err(error("BOARD_SOURCE_UNION",&format!("{} source reconciliation would change material area/topology: relative={change:e}, components={}/{components}, holes={restored_holes}/{holes}; no copper repaired",e.name,restored.0.len())));}
        source_copper_restorations.push(serde_json::json!({"contact":e.name,"sourcePaths":e.canonical_paths()?,"layer":e.layer,"decision":"resolved","reason":"retain exact prepared source copper lost by the derived layer union; component/hole counts and original1e-8 copper-area/contact guards preserved","missingAreaMm2":missing.unsigned_area(),"relativeContactGap":(expected-covered)/expected,"relativeMetalAreaChange":change,"boreOverlapAreaMm2":bore_overlap}));
        arrangement_sets[layer]=restored.clone();physical_shapes[layer].0=restored;
    }
    let physical_count=physical_shapes.len();
    if arrangement_sets.len()!=physical_count+r.electrodes.len() {return Err(error("BOARD_ARRANGEMENT","material/contact cut ordering mismatch"));}
    let mut masks=Vec::new(); let mut hybrid_mesh_masks=Vec::new();
    if hybrid_masks {
        for (i,id) in barrel_ids.iter().enumerate() {
            check(start,max_wall)?;
            let shape=&physical_shapes[geometry.layers.len()+i].0;
            let b=shape.bounding_rect().ok_or_else(||error("BOARD_PROFILE","empty barrel mask"))?;
            let width=b.width().max(b.height());
            let margin=width.max(r.stack.iter().map(|l|l.thickness_m).sum::<f64>()*1e3);
            let bounds=[[(b.min().x-margin).max(roi_bounds[0][0]),(b.min().y-margin).max(roi_bounds[0][1])],[(b.max().x+margin).min(roi_bounds[1][0]),(b.max().y+margin).min(roi_bounds[1][1])]];
            if bounds.iter().flatten().any(|v|!v.is_finite()) {return Err(error("BOARD_PROFILE","nonfinite barrel mask"));}
            budget.check(masks.len().checked_add(1).and_then(|v|v.checked_mul(4096)).ok_or_else(||error("MEMORY_LIMIT","barrel mask count overflow"))?)?;
            masks.push(areas::one(rect(bounds))); hybrid_mesh_masks.push(serde_json::json!({"barrelId":id,"boundsMm":bounds,"scope":"mesh-only"}));
        }
    }
    let cuts=std::iter::once(substrate.clone()).chain(std::iter::once(bores.clone()))
        .chain(arrangement_sets).chain(partial_bores.iter_mut().map(|(_,poly)|std::mem::replace(poly,MultiPolygon(vec![]))))
        .chain(masks)
        .collect::<Vec<_>>();
    let atoms=common_partition(&roi,&cuts,budget,start,max_wall)?;
    super::board_foil::phase_trace("arrangement-done",start,serde_json::json!({"atoms":atoms.len()}));
    let member_set=|index:usize|areas::union(atoms.iter().filter(|a|a.member(index)).map(|a|(*a.poly).clone()));
    substrate=member_set(0);bores=member_set(1);
    for (i,(_,poly)) in partial_bores.iter_mut().enumerate() {
        *poly=member_set(2+physical_count+r.electrodes.len()+i);
    }
    let mut max_copper_area_change:f64=0.;
    for(i,shape)in physical_shapes.iter_mut().enumerate() {
        let canonical=member_set(i+2);let (original_area,original_components,original_holes,original_perimeter)=original_physical_stats[i];let area=canonical.unsigned_area();
        let change=if original_area==0. {if area==0.{0.}else{f64::INFINITY}}else{(area-original_area).abs()/original_area};
        let holes=|p:&MultiPolygon<f64>|p.0.iter().map(|p|p.interiors().len()).sum::<usize>();
        super::board_foil::phase_trace(&format!("t-set{i}-union"),start,serde_json::json!({}));
        let (now_c,now_h)=noise_pieces(&canonical,grid_step_mm);
        let (src_c,src_h)=&original_filtered[i];
        let matched=match_components_spatial(&canonical,&shape.0,grid_step_mm,original_area).and_then(|c|match_sorted_pieces(&now_h,src_h,grid_step_mm,original_area).or_else(|e|{
            // A pinch point of the source ring that rounding opened by <= 4h joins a hole to the
            // outside. Close the rebuilt union by +-2h (check only) and match the holes again.
            // Only the filled outer shells need closing: a sub-4h crack or pinch between shells turns a
            // hole into part of the outside, and closing the union of the filled shells turns it back
            // into a hole. The holes of each component are kept as they are, so the huge multi-hole
            // union is never buffered.
            use geo::Buffer;let delta=2.*grid_step_mm;
            let mut closed_h:Vec<(f64,f64)>=vec![];
            let real:Vec<&Polygon<f64>>=canonical.0.iter().filter(|p|!ring_is_noise(p.exterior(),grid_step_mm)).collect();
            let filled=areas::union(real.iter().map(|p|Polygon::new(p.exterior().clone(),vec![])));
            let closed=filled.buffer(delta).buffer(-delta);
            for q in &closed.0 {for ring in q.interiors() {if !ring_is_noise(ring,grid_step_mm) {closed_h.push((Polygon::new(ring.clone(),vec![]).unsigned_area(),ring_perimeter_mm(ring)));}}}
            for poly in &real {for ring in poly.interiors() {if !ring_is_noise(ring,grid_step_mm) {closed_h.push((Polygon::new(ring.clone(),vec![]).unsigned_area(),ring_perimeter_mm(ring)));}}}
            closed_h.sort_by(|x,y|y.0.total_cmp(&x.0));
            match_sorted_pieces(&closed_h,src_h,grid_step_mm,original_area).map(|(n,a)|{eprintln!("P7_ARRANGEMENT_NOISE set={i} holes reopened by sub-4h cracks, closed by +-{delta:e}: {e}");(n,a)}).map_err(|e2|format!("{e}; after closing +-{delta:e}: {e2}"))
        }).map(|h|(c,h)));
        super::board_foil::phase_trace(&format!("t-set{i}-matched"),start,serde_json::json!({}));
        let raw=format!("components raw={}/{original_components} filtered={}/{}, holes raw={}/{original_holes} filtered={}/{}",canonical.0.len(),now_c.len(),src_c.len(),holes(&canonical),now_h.len(),src_h.len());
        if !area_change_ok((area-original_area).abs(),original_area,grid_step_mm,original_perimeter) {
            let describe=|m:&MultiPolygon<f64>|{let mut v=m.0.iter().map(|p|(p.unsigned_area(),p.bounding_rect())).collect::<Vec<_>>();v.sort_by(|a,b|b.0.total_cmp(&a.0));v.iter().take(5).map(|(a,b)|format!("area={a:e} bbox={:?}",b.map(|b|[[b.min().x,b.min().y],[b.max().x,b.max().y]]))).collect::<Vec<_>>().join("; ")};
            eprintln!("P7_ARRANGEMENT_DIFF set={i} lost_from_source=[{}] gained_vs_source=[{}]",describe(&areas::difference(&shape.0,&canonical)),describe(&areas::difference(&canonical,&shape.0)));
            return Err(error("BOARD_ARRANGEMENT",&format!("physical copper set{i} changed area: relative area={change:e}, {raw}")));}
        let ((xc,sc_area,merged),(xh,sh_area))=matched.map_err(|e|error("BOARD_ARRANGEMENT",&format!("physical copper set{i} changed topology: relative area={change:e}, {raw}; {e}")))?;
        if xc>0||xh>0||canonical.0.len()!=original_components||holes(&canonical)!=original_holes {eprintln!("P7_ARRANGEMENT_NOISE set={i} touchingMerged={merged} extraComponents={xc} extraHoles={xh} surplusAreaRelative={:e} ({raw})",(sc_area+sh_area)/original_area);}
        max_copper_area_change=max_copper_area_change.max(change);shape.0=canonical;
    }
    for(i,e)in r.electrodes.iter().enumerate(){electrode_polygons.insert(e.name.clone(),member_set(2+physical_count+i));}
    super::board_foil::phase_trace("t-member-sets",start,serde_json::json!({}));
    // Keep an explicit stable numerical datum. Domain expansion changes the
    // physical air/ROI request but must not retranslate identical copper.
    if let Some(b)=r.return_boundary_mm { if b.iter().flatten().any(|v|!v.is_finite()) || (0..3).any(|k|b[0][k]>=b[1][k]) {return Err(error("RETURN_BOUNDARY","invalid guard bounds"));} }
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
    // These are mesh constraints only; canonical material and contact atoms
    // still come from the original authored inner and outer barrel contours.
    for (id,vertices,pieces) in barrel_shell_constraints {
        let barrel=barrel_ids.iter().position(|x|x==&id).and_then(|i|physical_shapes.get(geometry.layers.len()+i)).ok_or_else(||error("BOARD_PROFILE","canonical barrel source missing"))?;
        let on_boundary=|p:[f64;2]| barrel.0.0.iter().any(|poly|std::iter::once(poly.exterior()).chain(poly.interiors()).any(|ring|ring.lines().any(|line|line.intersects(&Point::from(p)))));
        let mut canonical=Vec::with_capacity(vertices.len());
        for (inner,outer) in vertices {
            let a=arrangement_context.canonical_point(inner); let b=arrangement_context.canonical_point(outer);
            if !on_boundary(a)||!on_boundary(b) {return Err(error("BOARD_PROFILE","canonical barrel spoke endpoint absent from its source boundary"));}
            canonical.push((a,b));
        }
        copper_skin_profile::visit_radial_constraints(&canonical,pieces,&mut segment)?;
    }
    // Atoms from different overlays are not mutually noded: a vertex of one atom can lie within
    // rounding distance of a long edge of its neighbour (T-junction). spade's constraint insertion
    // asserts on such near-collinear overlaps (NanoScope, ThunderF722), so split every edge at the
    // atom vertices lying within 2h of it and constrain the pieces between existing vertices.
    let tol=2.*grid_step_mm;
    let mut unique=std::collections::HashSet::new();
    // Vertices closer than 4h are arrangement rounding noise of one point (two atoms that should
    // share a node). Snap each cluster to its first vertex; otherwise the CDT has sliver faces of
    // width ~1e-7 mm and aspect up to 1e8. The displacement is below the overlay grid error bound.
    let snap_radius=std::env::var("P7_MESH_SNAP_MM").ok().and_then(|v|v.parse::<f64>().ok()).filter(|v|v.is_finite()&&*v>=0.).unwrap_or(2.*tol);
    let mut canon=std::collections::HashMap::<(u64,u64),[f64;2]>::new();
    let mut rep_tree=rstar::RTree::<[f64;2]>::new();
    let mut snapped=0usize;
    for c in atoms.iter().flat_map(|a|std::iter::once(a.poly.exterior()).chain(a.poly.interiors())).flat_map(|r|r.0.iter()) {
        if !unique.insert((c.x.to_bits(),c.y.to_bits())) {continue;}
        let p=[c.x,c.y];
        let hit=rep_tree.locate_within_distance(p,snap_radius*snap_radius).min_by(|a,b|((a[0]-p[0]).powi(2)+(a[1]-p[1]).powi(2)).total_cmp(&((b[0]-p[0]).powi(2)+(b[1]-p[1]).powi(2)))).copied();
        match hit {Some(q)=>{canon.insert((c.x.to_bits(),c.y.to_bits()),q);snapped+=1;},None=>{rep_tree.insert(p);canon.insert((c.x.to_bits(),c.y.to_bits()),p);}}
    }
    let snap=|p:[f64;2]|canon.get(&(p[0].to_bits(),p[1].to_bits())).copied().unwrap_or(p);
    let vertex_tree=rep_tree;
    super::board_foil::phase_trace("t-vertex-snap",start,serde_json::json!({"snapped":snapped,"radiusMm":snap_radius}));
    for poly in atoms.iter().map(|a|&a.poly){for ring in std::iter::once(poly.exterior()).chain(poly.interiors()){for line in ring.lines(){
        let (a,c)=(snap([line.start.x,line.start.y]),snap([line.end.x,line.end.y]));
        if a==c {continue;}
        let (dx,dy)=(c[0]-a[0],c[1]-a[1]);let len2=dx*dx+dy*dy;
        let mut cuts:Vec<(f64,[f64;2])>=vec![];
        if len2>tol*tol {
            let env=rstar::AABB::from_corners([a[0].min(c[0])-tol,a[1].min(c[1])-tol],[a[0].max(c[0])+tol,a[1].max(c[1])+tol]);
            for q in vertex_tree.locate_in_envelope(&env) {
                if *q==a||*q==c {continue;}
                let t=((q[0]-a[0])*dx+(q[1]-a[1])*dy)/len2;
                if t<=0.||t>=1. {continue;}
                let d=((q[0]-a[0])*dy-(q[1]-a[1])*dx).abs()/len2.sqrt();
                if d<=tol {cuts.push((t,*q));}
            }
            cuts.sort_by(|x,y|x.0.total_cmp(&y.0));
        }
        let mut from=a;for (_,q) in cuts {segment(from,q)?;from=q;}
        segment(from,c)?;
    }}}
    if let Some((nodes,triangles))=support{let mut edges=std::collections::BTreeSet::new();for t in triangles{for k in 0..3{let mut e=[t[k],t[(k+1)%3]];e.sort();edges.insert(e);}}for [a,b]in edges{segment(nodes[a],nodes[b])?;}}
    for [a,b] in trace_lines {segment(*a,*b)?;}
    drop(segment);
    super::board_foil::phase_trace("t-constraints-inserted",start,serde_json::json!({}));
    let max_xy=budget.max_nodes.min(budget.max_bytes/4096);
    let refine_area=|area:f64,vertices:usize|RefinementParameters::new().keep_constraint_edges().with_angle_limit(AngleLimit::from_deg(r.min_triangle_angle_deg.unwrap_or(30.))).with_max_allowed_area(area).with_max_additional_vertices(max_xy.saturating_sub(vertices));
    if let Some(field)=&coarse_field {
        let baseline=std::env::var_os("P7_MESH_COARSEN_BASELINE").map(|_|{let mut c=cdt.clone();c.refine(refine_area(a0_mm2,c.num_vertices()));c.num_vertices()});
        let area_far=field.area_limit(a0_mm2,field.h_max);
        let refinement=cdt.refine(refine_area(area_far,cdt.num_vertices()));
        if !refinement.refinement_complete {return Err(error("MEMORY_LIMIT","board XY mesh refinement incomplete within budget"));}
        let graded=mesh_coarsen::grade(&mut cdt,field,a0_mm2,max_xy,||check(start,max_wall))?;
        if r.min_triangle_angle_deg.is_none_or(|a|a>0.) {
            // Repair any angle violation left by the graded insertion without refining below the far size.
            let repair=cdt.refine(refine_area(area_far,cdt.num_vertices()));
            if !repair.refinement_complete {return Err(error("MEMORY_LIMIT","board XY mesh refinement incomplete within budget"));}
        }
        mesh_coarsening=Some(serde_json::json!({"kind":"distance-graded-planar-mesh","tier":"approximation","active":true,"h0Mm":field.h0,"hMaxMm":field.h_max,"d0Mm":field.d0,"growthPerStep":mesh_coarsen::GROWTH_PER_STEP,
            "support":"port electrode contacts, copper of the channel nets (largest net excluded when several), their barrels","geometry":"copper rings farther than d0 from the support are simplified (topology-preserving Visvalingam-Whyatt, tolerance up to hMax/4); channel copper within d0, contacts and bores unchanged",
            "unverified":"coarsened far-field mesh accuracy is not independently verified; air/dielectric fill shares the extrusion planes and is coarsened only in XY",
            "gradedVertices":graded,"outline":outline_simplification,"planarVertices":cdt.num_vertices(),"planarVerticesUniform":baseline}));
    }else{
        let refinement=cdt.refine(refine_area(a0_mm2,cdt.num_vertices()));
        if !refinement.refinement_complete {return Err(error("MEMORY_LIMIT","board XY mesh refinement incomplete within budget"));}
    }
    check(start,max_wall)?;
    super::board_foil::phase_trace("t-cdt-refined",start,serde_json::json!({}));
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
    super::board_foil::phase_trace("t-air-rings",start,serde_json::json!({}));
    // Minimum-angle refinement (Chew/Ruppert): long constrained edges are split by collinear Steiner points.
    {
        let min_area=mesh_coarsen::quality_min_area();
        let (aspect_before,angle_before,worst_before,fixable_before)=mesh_coarsen::quality(&cdt,min_area);
        let n0=cdt.num_vertices();let mut complete=true;
        if !mesh_coarsen::quality_disabled() {
            let cap=mesh_coarsen::quality_cap(n0).min(max_xy.saturating_sub(n0));
            complete=mesh_coarsen::improve(&mut cdt,mesh_coarsen::quality_angle(),min_area,cap);
        }
        let (aspect_after,angle_after,worst_after,fixable_after)=mesh_coarsen::quality(&cdt,min_area);
        let q=serde_json::json!({"targetMinAngleDeg":if mesh_coarsen::quality_disabled(){0.}else{mesh_coarsen::quality_angle()},"minFaceAreaMm2":min_area,"maxAspectBefore":aspect_before,"maxAspectAboveMinAreaBefore":fixable_before,"maxAspectAboveMinAreaAfter":fixable_after,"maxAspectAfter":aspect_after,"minAngleBeforeDeg":angle_before,"minAngleAfterDeg":angle_after,"steinerVertices":cdt.num_vertices()-n0,"worstFaceBeforeMm":worst_before,"worstFaceAfterMm":worst_after,"complete":complete,"note":"collinear Steiner points on constrained edges; outlines unchanged"});
        eprintln!("P7_MESH_QUALITY {q}");
        mesh_coarsening.get_or_insert_with(||serde_json::json!({"kind":"distance-graded-planar-mesh","tier":"approximation","active":false}))["quality"]=q;
    }
    let planar_nodes_mm:Vec<_>=cdt.vertices().map(|v|[v.position().x,v.position().y]).collect();
    let xy_m:Vec<_>=planar_nodes_mm.iter().map(|p|[(p[0]-origin_mm[0])*1e-3,(p[1]-origin_mm[1])*1e-3]).collect();
    let mut triangles=vec![];let mut centres=vec![];let mut overlaps=vec![];
    let overlaps_bbox=|a:geo::Rect<f64>,b:Option<geo::Rect<f64>>|b.is_none_or(|b|!(a.max().x<b.min().x||b.max().x<a.min().x||a.max().y<b.min().y||b.max().y<a.min().y));
    let shape_bboxes:Vec<_>=shapes.iter().map(|s|s.poly.bounding_rect()).collect();
    let bbox_index=|boxes:&[Option<geo::Rect<f64>>]|(rstar::RTree::bulk_load(boxes.iter().enumerate().filter_map(|(i,b)|b.map(|b|rstar::primitives::GeomWithData::new(rstar::primitives::Rectangle::from_corners([b.min().x,b.min().y],[b.max().x,b.max().y]),i))).collect::<Vec<_>>()),boxes.iter().enumerate().filter(|(_,b)|b.is_none()).map(|(i,_)|i).collect::<Vec<_>>());
    let candidates=|index:&(rstar::RTree<rstar::primitives::GeomWithData<rstar::primitives::Rectangle<[f64;2]>,usize>>,Vec<usize>),at:geo::Rect<f64>|{let mut v:Vec<usize>=index.0.locate_in_envelope_intersecting(&rstar::AABB::from_corners([at.min().x,at.min().y],[at.max().x,at.max().y])).map(|g|g.data).chain(index.1.iter().copied()).collect();v.sort_unstable();v};
    let shape_index=bbox_index(&shape_bboxes);
    // Per-shape boundary segment index: a face whose centre is outside a shape and whose bbox meets
    // no boundary segment of it cannot overlap it, and a centre inside it overlaps by definition;
    // DE-9IM runs only for the ambiguous faces next to a boundary.
    use rayon::prelude::*;
    let segment_trees:Vec<SegTree>=shapes.par_iter().map(|sh|build_seg_tree(&sh.poly)).collect();
    super::board_foil::phase_trace("t-segment-trees",start,serde_json::json!({}));
    let faces:Vec<([usize;3],[[f64;2];3])>=cdt.inner_faces().map(|f|{let v=f.vertices();(v.map(|v|v.fix().index()),v.map(|v|[v.position().x,v.position().y]))}).collect();
    let face_results:Vec<Option<(Point<f64>,Vec<usize>)>>=faces.par_iter().map(|(_,pos)|{
        let centre=Point::new((pos[0][0]+pos[1][0]+pos[2][0])/3.,(pos[0][1]+pos[1][1]+pos[2][1])/3.);
        if !field_domain.contains(&centre) {return None;}
        let triangle=areas::one(Polygon::new(LineString::from(pos.to_vec()),vec![]));
        let tri_bbox=triangle.bounding_rect();
        let cand:Vec<usize>=match tri_bbox{Some(t)=>candidates(&shape_index,t),None=>(0..shapes.len()).collect()};
        let env=tri_bbox.map(|t|rstar::AABB::from_corners([t.min().x,t.min().y],[t.max().x,t.max().y]));
        let sources=cand.into_iter().filter(|&i|{
            if point_in_segments(&segment_trees[i],centre.x(),centre.y()) {return true;}
            // Ambiguous only when a boundary segment of this shape touches the face itself;
            // otherwise the face is wholly outside (its centre is not inside).
            let touched=env.is_none_or(|e|segment_trees[i].locate_in_envelope_intersecting(&e).any(|g|segment_meets_triangle(g.a,g.b,pos)));
            if !touched {return false;}
            // DE-9IM against a many-vertex polygon costs O(vertices) per face and dominated the
            // stage on boards with large pours; for those, interior sample points decide (a
            // sub-1/6-face sliver of a large pour is not attributed, its owner is the pour at the centre).
            if segment_trees[i].size()>200 {
                return (1..6usize).any(|u|(1..6-u).any(|v|{let w=6-u-v;let (fu,fv,fw)=(u as f64/6.,v as f64/6.,w as f64/6.);
                    point_in_segments(&segment_trees[i],fu*pos[0][0]+fv*pos[1][0]+fw*pos[2][0],fu*pos[0][1]+fv*pos[1][1]+fw*pos[2][1])}));
            }
            shapes[i].poly.relate(&triangle).get(CoordPos::Inside,CoordPos::Inside)==Dimensions::TwoDimensional
        }).collect::<Vec<_>>();
        Some((centre,sources))
    }).collect();
    for ((idx,_),res) in faces.into_iter().zip(face_results) {
        if let Some((centre,sources))=res {triangles.push(idx);centres.push(centre);overlaps.push(sources);}
    }
    let mut edges=BTreeMap::new();for t in &triangles {for k in 0..3 {let mut edge=[t[k],t[(k+1)%3]];edge.sort();*edges.entry(edge).or_insert(0usize)+=1;}}
    let boundary_edges=edges.into_iter().filter(|(_,n)|*n==1).map(|(e,_)|e).collect();
    super::board_foil::phase_trace("t-tri-sources",start,serde_json::json!({}));
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
    if let Some(c)=&mut mesh_coarsening {
        let nodes=xy_m.len()*z_m.len();c["nodes"]=serde_json::json!(nodes);
        eprintln!("P7_MESH_COARSEN {}",serde_json::json!({"h0":c["h0Mm"],"hMax":c["hMaxMm"],"d0":c["d0Mm"],"nodesBefore":c["planarVerticesUniform"].as_u64().map(|v|v as usize*z_m.len()),"nodes":nodes,"planarVertices":xy_m.len()}));
    }
    let mut materials=vec![];let mut sigma_per_triangle=vec![];
    let physical_bboxes:Vec<_>=physical_shapes.iter().map(|s|s.0.bounding_rect()).collect();
    // Per-triangle (z-independent) ownership candidates, in physical_shapes order.
    let physical_index=bbox_index(&physical_bboxes);
    // O(log n) point-in-area through boundary segment trees; geo's `contains` is O(vertices) per
    // triangle and dominated this stage on boards with large multi-hole pours.
    let physical_trees:Vec<SegTree>=physical_shapes.par_iter().map(|s|build_seg_tree(&s.0)).collect();
    let (substrate_tree,bores_tree)=(build_seg_tree(&substrate),build_seg_tree(&bores));
    let partial_trees:Vec<SegTree>=partial_bores.iter().map(|(_,poly)|build_seg_tree(poly)).collect();
    let tri_own:Vec<(Vec<usize>,bool,bool,Vec<bool>)>=centres.par_iter().map(|p|{
        let at=geo::Rect::new(p.0,p.0);
        let phys=candidates(&physical_index,at).into_iter().filter(|&i|physical_bboxes[i].is_some()).filter(|&i|point_in_segments(&physical_trees[i],p.x(),p.y())).collect();
        (phys,point_in_segments(&substrate_tree,p.x(),p.y()),point_in_segments(&bores_tree,p.x(),p.y()),partial_trees.iter().map(|t|point_in_segments(t,p.x(),p.y())).collect())
    }).collect();
    let mut unowned_faces=std::collections::BTreeSet::<usize>::new();
    for slab in z_m.windows(2) {
    super::board_foil::phase_trace("t-tri-own",start,serde_json::json!({}));
        let middle=(slab[0]+slab[1])/2.;let layer=r.stack.iter().zip(&stack_spans).find(|(_,s)|middle>s[0]&&middle<s[1]).map(|(l,_)|l);
        let active_bores=partial_bores.iter().enumerate().filter(|(_,(span,_))|span[0]<middle&&middle<span[1]).map(|(i,_)|i).collect::<Vec<_>>();
        let mut row=vec![];let mut sigma=vec![];
        for (ti,p) in centres.iter().enumerate() {let _=p;
            // Physical ownership is constant inside each constrained material
            // face; provenance can cover only part of that same face.
            let own=&tri_own[ti];let physical:Vec<_>=own.0.iter().map(|&i|&physical_shapes[i]).filter(|(_,span,_,_)|middle>span[0]&&middle<span[1]).collect();
            let copper=!physical.is_empty();
            let owners:Vec<_>=if copper {overlaps[ti].iter().map(|&i|&shapes[i]).filter(|s|middle>s.span[0]&&middle<s.span[1]).collect()}else{vec![]};
            // A face centre in a rounding sliver of the canonical union (outside every source polygon of
            // this layer by far less than a feature) is attributed to the nearest source polygon. The
            // arrangement check bounded the excess copper at |dA| <= h*P; the attributed face area is
            // re-bounded below at 1e-6 of the copper area, so this cannot hide a real gain.
            let near_owner:Vec<_>=if copper&&owners.is_empty() {
                use rstar::PointDistance;let d=owner_slack;
                let at=geo::Rect::new(geo::Coord{x:p.x()-d,y:p.y()-d},geo::Coord{x:p.x()+d,y:p.y()+d});
                let best=candidates(&shape_index,at).into_iter().filter(|&i|middle>shapes[i].span[0]&&middle<shapes[i].span[1])
                    .filter_map(|i|segment_trees[i].nearest_neighbor(&[p.x(),p.y()]).map(|g|(g.distance_2(&[p.x(),p.y()]),i)))
                    .filter(|(d2,_)|*d2<=d*d).min_by(|a,b|a.0.total_cmp(&b.0));
                if let Some((_,i))=best {unowned_faces.insert(ti);vec![&shapes[i]]} else {vec![]}
            } else {vec![]};
            let owners=if owners.is_empty() {near_owner} else {owners};
            if copper&&owners.is_empty() {
                use rstar::PointDistance;
                let near:Vec<String>=(0..shapes.len()).filter(|&k|middle>shapes[k].span[0]&&middle<shapes[k].span[1]).filter_map(|k|segment_trees[k].nearest_neighbor(&[p.x(),p.y()]).map(|g|(g.distance_2(&[p.x(),p.y()]).sqrt(),k))).fold(vec![],|mut v,(d,k)|{v.push((d,k));v.sort_by(|a,b|a.0.total_cmp(&b.0));v.truncate(2);v}).iter().map(|(d,k)|format!("{}:{d:e}mm inside={}",shapes[*k].id,point_in_segments(&segment_trees[*k],p.x(),p.y()))).collect();
                return Err(error("BOARD_OWNERSHIP",&format!("canonical copper face{ti} at {p:?}, z={middle:e} has no positive-area source attribution (nearest same-span sources {near:?}, grid step {grid_step_mm:e}; physical owners {:?}; layer source copper contains point: {:?}; regions of that layer containing point: {}); no copper removed",own.0.iter().map(|&k|(k,physical_shapes[k].1,physical_shapes[k].3)).collect::<Vec<_>>(),own.0.iter().map(|&k|geometry.layers.get(k).map(|l|l.copper.iter().any(|c|polygon(c).contains(p)))).collect::<Vec<_>>(),geometry.regions.iter().filter(|g|spans.get(&g.layer).is_some_and(|sp|middle>sp[0]&&middle<sp[1])&&polygon(&g.polygon).contains(p)).count())));
            }
            if !copper {
                let dielectric=layer.filter(|l|!l.copper&&own.1&&!own.2
                    &&!active_bores.iter().any(|&i|own.3[i]));
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
    if !unowned_faces.is_empty() {
        let tri_area=|t:&[usize;3]|{let [a,b,c]=t.map(|i|planar_nodes_mm[i]);0.5*((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])).abs()};
        let unowned:f64=unowned_faces.iter().map(|&ti|tri_area(&triangles[ti])).sum();
        let copper_area:f64=physical_shapes.iter().map(|s|s.0.unsigned_area()).sum();
        eprintln!("P7_ARRANGEMENT_NOISE unowned copper faces={} attributed to nearest source, area {unowned:e} mm2 ({:e} of copper)",unowned_faces.len(),unowned/copper_area);
        if !(unowned<=unowned_limit*copper_area) {return Err(error("BOARD_OWNERSHIP",&format!("{} canonical copper faces (area {unowned:e} mm2) lie outside every source polygon: more than 1e-6 of copper area {copper_area:e} mm2",unowned_faces.len())));}
    }
    let mesh=mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m,materials},budget)?;
    let conductivity_s_m=mesh.copper_tetrahedra.iter().map(|t|sigma_per_triangle[t.slab][t.planar_triangle]).collect();
    let mut certified_electrodes:BTreeMap<String,(Electrode,Vec<usize>)>=BTreeMap::new();let mut electrode_areas_m2=BTreeMap::new();let mut max_contact_area_change:f64=0.;
    for e in &r.electrodes {
        let poly=&electrode_polygons[&e.name];let z=spans[&e.layer][if e.upper {1}else{0}];
        let cut=r.cut_planes.get(&e.name);
        let edges=contact_boundary_edges.get(&e.name);
        let mut nodes=std::collections::BTreeSet::new();let mut area=0.;
        let contact_source_ids:std::collections::HashSet<&String>=match e.canonical_paths() {
            Ok(paths)=>source_mapping.iter().filter(|m|paths.contains(&m.1)&&m.3==e.layer).map(|m|&m.0).collect(),
            Err(_)=>Default::default(),
        };
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
                let source=face.copper_source_ids.iter().any(|id|contact_source_ids.contains(id));
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
        let contact_rounding_m2=if cut.is_some(){0.}else{grid_step_mm*perimeter_mm(&poly)*1e-6};
        if nodes.len()<3||((area-expected).abs()>1e-8*expected&&(area-expected).abs()>contact_rounding_m2) {return Err(error("BOARD_ELECTRODE",&format!("finite exposed contact area mismatch: {} mesh={area:e} source={expected:e} canonical={:e}",e.name,poly.unsigned_area()*1e-6)));}
        max_contact_area_change=max_contact_area_change.max((area-expected).abs()/expected);
        electrode_areas_m2.insert(e.name.clone(),area);certified_electrodes.insert(e.name.clone(),(e.clone(),nodes.into_iter().collect()));
    }
    check(start,max_wall)?;
    let geometry_vertices = std::iter::once(&substrate).chain(std::iter::once(&bores))
        .chain(partial_bores.iter().map(|(_,p)|p))
        .chain(physical_shapes.iter().map(|(p,_,_,_)|p))
        .flat_map(|p|p.0.iter()).flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors()))
        .try_fold(0usize, |n,r| n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT","field geometry overflow"))?;
    let retained_bytes = geometry_vertices.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT","field geometry overflow"))?;
    if mesh.planned_peak_bytes.checked_add(retained_bytes).is_none_or(|n|n>budget.max_bytes) { return Err(error("MEMORY_LIMIT","retained field geometry exceeds budget")); }
    let field_copper=physical_shapes.into_iter().map(|(p,span,_,barrel)|(p,span,barrel)).collect();
    if let Some(b)=r.return_boundary_mm {
        for (_,nodes) in certified_electrodes.values() { for &n in nodes { let p=mesh.nodes_m[n]; let world=[origin_mm[0]+1000.*p[0],origin_mm[1]+1000.*p[1],1000.*p[2]]; if (0..3).any(|k|world[k]<=b[0][k] || world[k]>=b[1][k]) {return Err(error("RETURN_BOUNDARY","electrode support must lie strictly inside guard"));} } }
    }
    Ok(BoardMesh{return_boundary_mm:r.return_boundary_mm,field_substrate:substrate,field_bores:bores,field_partial_bores:partial_bores,field_copper,mesh,copper_skin_layers,copper_skin_barrels,planar_nodes_mm,planar_triangles,conductivity_s_m,geometry,origin_mm,source_mapping,barrel_ids,elapsed_seconds:start.elapsed().as_secs_f64(),layer_spans:spans,electrode_areas_m2,arrangement_atoms:atoms.len(),max_copper_area_change,max_contact_area_change,source_copper_restorations,hybrid_mesh_masks,mesh_coarsening,roi_fallback,certified_electrodes})
}
impl BoardMesh {
    /// End geometry preparation before a separately budgeted field phase.
    pub fn into_mesh(self)->LayeredMesh{self.mesh}
    pub fn electrode_nodes(&self,e:&Electrode)->Result<Vec<usize>> {
        let (descriptor,nodes)=self.certified_electrodes.get(&e.name).ok_or_else(||error("BOARD_ELECTRODE","electrode was not part of the conforming mesh request"))?;
        if !descriptor.same_contact(e) {return Err(error("BOARD_ELECTRODE","electrode descriptor differs from its certified mesh contour"));}Ok(nodes.clone())
    }
}


#[cfg(test)]
mod annulus_arrangement_tests {
    use super::*;
    #[test]
    fn nearest_segment_point_test_matches_polygon_contains() {
        let sq=|x:f64,y:f64,s:f64|LineString::from(vec![[x,y],[x+s,y],[x+s,y+s],[x,y+s],[x,y]]);
        let m=MultiPolygon(vec![Polygon::new(sq(0.,0.,10.),vec![sq(4.,4.,2.)]),Polygon::new(sq(20.,0.,1.),vec![])]);
        let tree=build_seg_tree(&m);
        for (x,y) in [(1.,1.),(5.,5.),(9.5,0.5),(15.,5.),(20.5,0.5),(-1.,3.),(3.9,5.),(4.1,5.),(10.5,5.)] {
            assert_eq!(point_in_segments(&tree,x,y),m.contains(&Point::new(x,y)),"({x},{y})");
        }
    }
    #[test]
    fn nearest_segment_point_test_handles_pinched_rings() {
        // Two squares joined at the single vertex (1,1) by one ring, each loop with its own orientation.
        let ring=LineString::from(vec![[0.,0.],[1.,0.],[1.,1.],[2.,1.],[2.,2.],[1.,2.],[1.,1.],[0.,1.],[0.,0.]]);
        let tree=build_seg_tree(&MultiPolygon(vec![Polygon::new(ring,vec![])]));
        for (x,y,expect) in [(0.5,0.5,true),(1.5,1.5,true),(0.5,1.5,false),(1.5,0.5,false),(3.,3.,false)] {
            assert_eq!(point_in_segments(&tree,x,y),expect,"({x},{y})");
        }
    }
    #[test]
    fn grid_noise_rings_are_ignored_but_real_holes_change_topology() {
        let h=1e-6;
        let sq=|x:f64,y:f64,s:f64|LineString::from(vec![[x,y],[x+s,y],[x+s,y+s],[x,y+s],[x,y]]);
        let plain=MultiPolygon(vec![Polygon::new(sq(0.,0.,1.),vec![])]);
        let noisy=MultiPolygon(vec![Polygon::new(sq(0.,0.,1.),vec![sq(0.5,0.5,1e-5)]),Polygon::new(sq(2.,2.,1e-5),vec![])]);
        assert_eq!(noise_filtered_topology(&noisy,h).1,noise_filtered_topology(&plain,h).1);
        assert_eq!(noise_filtered_topology(&noisy,h).0,(2,1));
        let (nc,nh)=noise_pieces(&noisy,h);let (pc,ph)=noise_pieces(&plain,h);
        assert!(match_components_spatial(&noisy,&plain,h,1.).is_ok()&&match_sorted_pieces(&nh,&ph,h,1.).is_ok());let _=(nc,pc.clone());
        let rect=|x0:f64,x1:f64|Polygon::new(LineString::from(vec![[x0,0.],[x1,0.],[x1,1.],[x0,1.],[x0,0.]]),vec![]);
        let touching=MultiPolygon(vec![rect(0.,0.5),rect(0.5,1.)]);
        assert!(match_components_spatial(&touching,&plain,h,1.).is_ok());
        let split=MultiPolygon(vec![rect(0.,0.45),rect(0.55,1.)]);
        assert!(match_components_spatial(&split,&plain,h,1.).is_err());
        let merged=MultiPolygon(vec![Polygon::new(sq(0.,0.,1.),vec![])]);
        let two=MultiPolygon(vec![rect(0.,0.5),rect(0.6,1.1)]);
        assert!(match_components_spatial(&merged,&two,h,1.).is_err());
        let big_extra=MultiPolygon(vec![Polygon::new(sq(0.,0.,1.),vec![]),Polygon::new(sq(2.,2.,0.01),vec![])]);
        assert!(match_components_spatial(&big_extra,&plain,h,1.).is_err());
        let neck=MultiPolygon(vec![Polygon::new(sq(0.,0.,1.),vec![]),Polygon::new(sq(2.,2.,1e-5),vec![])]);
        assert!(match_components_spatial(&neck,&plain,h,1.).is_ok());
        let real=MultiPolygon(vec![Polygon::new(sq(0.,0.,1.),vec![sq(0.4,0.4,0.1)])]);
        assert_ne!(noise_filtered_topology(&real,h).1,noise_filtered_topology(&plain,h).1);
    }
    #[test]
    fn broad_arrangement_retains_actual_offset_annulus_through_exact_noop_cuts() {
        let mut board=crate::import_json(include_str!(
            "../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json"
        )).board.unwrap();
        board.outline=[[-32.89589996337891,-22.895899963378906],
            [32.89589996337891,-22.895899963378906],[32.89589996337891,22.895899963378906],
            [-32.89589996337891,22.895899963378906]].map(|[x,y]|crate::model::Point{x,y}).to_vec();
        let context=Context::for_board(&board,GeometryOptions{max_deviation_mm:0.001,..Default::default()}).unwrap();
        let bounds=[[-32.89589996337891,-22.895899963378906],[32.89589996337891,22.895899963378906]];
        let _scope=Context::install(Some(context.refined_for_arrangement(bounds).unwrap()));
        // Source witness: same physical center, conservative 28-gon bore and
        // 25 um authored wall as the failed full-board via. No input rounding.
        let center=[19.8617,6.2463];
        let radius=0.14985/(std::f64::consts::PI/28.).cos();
        let bore=areas::union([Polygon::new(LineString::from((0..28).map(|i| {
            let a=2.*std::f64::consts::PI*i as f64/28.;
            [center[0]+radius*a.cos(),center[1]+radius*a.sin()]
        }).collect::<Vec<_>>()),vec![])]);
        let outer=barrel_outer(&bore,center,[0.;2],0.14985,0.025,0.001).unwrap();
        let annulus=areas::difference(&areas::one(outer),&bore);
        let cover=areas::one(rect([[19.,5.],[21.,7.]]));
        let away=areas::one(rect([[-20.,-10.],[-19.,-9.]]));
        let budget=AllocationBudget{max_bytes:32*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let atoms=common_partition(&annulus,&[away,cover,bore.clone()],budget,Instant::now(),Duration::from_secs(10)).unwrap();
        assert_eq!(atoms.len(),1);
        assert_eq!((0..3).map(|i|atoms[0].member(i)).collect::<Vec<_>>(),vec![false,true,false]);
        assert_eq!(*atoms[0].poly,annulus.0[0]);
        let recovered=areas::union(atoms.into_iter().map(|a|(*a.poly).clone()));
        assert_eq!(recovered.0.len(),1);assert_eq!(recovered.0[0].interiors().len(),1);
        assert!((recovered.unsigned_area()-annulus.unsigned_area()).abs()<=1e-8*annulus.unsigned_area());
        // A cut's hole must not be mistaken for containing copper.
        let atoms=common_partition(&bore,&[annulus],budget,Instant::now(),Duration::from_secs(10)).unwrap();
        assert!(atoms.iter().all(|a|!a.member(0)));
    }
    #[test]
    fn bbox_local_partition_matches_brute_force_membership() {
        use geo::InteriorPoint;
        let budget=AllocationBudget{max_bytes:32*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let roi=areas::union([rect([[0.,0.],[10.,10.]])]);
        let cut_a=MultiPolygon(vec![rect([[1.,1.],[3.,3.]]),rect([[6.,2.],[8.,9.]]),rect([[20.,20.],[21.,21.]]),rect([[10.,0.],[12.,4.]])]);
        let cut_b=MultiPolygon(vec![rect([[2.,2.],[7.,4.]])]);
        let atoms=common_partition(&roi,&[cut_a.clone(),cut_b.clone()],budget,Instant::now(),Duration::from_secs(10)).unwrap();
        assert!(atoms.len()>3);
        for atom in &atoms {
            let p=atom.poly.interior_point().unwrap();
            assert_eq!((0..2).map(|i|atom.member(i)).collect::<Vec<_>>(),vec![cut_a.contains(&p),cut_b.contains(&p)]);
        }
        let total:f64=atoms.iter().map(|a|a.poly.unsigned_area()).sum();
        assert!((total-100.).abs()<1e-9);
    }
    #[test]
    fn rounding_aware_area_gate_accepts_only_grid_noise() {
        let step=2_f64.powi(-25);
        // 25 um annulus ring of 0.3 mm radius: perimeter ~ 1.9 mm, area ~ 0.047 mm2.
        let perimeter=2.*std::f64::consts::PI*0.3;let area=0.025*perimeter;
        assert!(area_change_ok(1.5e-8*area,area,step,perimeter));
        assert!(!area_change_ok(1e-4*area,area,step,perimeter));
        assert!(area_change_ok(0.5e-8*area,area,0.,perimeter));
    }
    #[test]
    fn capsule_barrel_outer_keeps_vertices_and_stadium_area() {
        let (r,t,tol)=(0.3,0.025,1e-3);
        let build=|h:[f64;2],n:usize,c:[f64;2]|{
            let ring=(0..n).map(|i|{let a=2.*std::f64::consts::PI*(i as f64+0.5)/n as f64;
                let side=if a.cos()*h[0]+a.sin()*h[1]>=0.{1.}else{-1.};[c[0]+side*h[0]+r*a.cos(),c[1]+side*h[1]+r*a.sin()]}).collect::<Vec<_>>();
            MultiPolygon(vec![Polygon::new(LineString::from(ring),vec![])])
        };
        let (c,h)=([5.,-2.],[0.,-0.4]); // axis rotated -90 degrees, 0.4 mm half segment
        let bore=build(h,128,c);
        let outer=barrel_outer(&bore,c,h,r,t,tol).unwrap();
        assert_eq!(outer.exterior().0.len(),bore.0[0].exterior().0.len());
        let annulus=outer.unsigned_area()-bore.unsigned_area();
        let analytic=t*(4.*0.4+2.*std::f64::consts::PI*(r+t/2.));
        assert!((annulus-analytic).abs()<=0.01*analytic,"{annulus} vs {analytic}");
        let round=build([0.;2],128,c);
        let outer=barrel_outer(&round,c,[0.;2],r,t,tol).unwrap();
        let analytic=std::f64::consts::PI*((r+t).powi(2)-r*r);
        assert!((outer.unsigned_area()-round.unsigned_area()-analytic).abs()<=0.01*analytic);
    }
}
