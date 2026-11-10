//! Distance-graded planar mesh size field (approximation tier).
//!
//! Far from the analysed channel the planar refinement of the board mesh is relaxed. No copper
//! outline, bore or contact constraint is moved, removed or simplified: only unconstrained
//! interior vertices are left out, so a feature narrower than the local size keeps its own
//! resolution through its constraint edges. Physics beyond the near field is unverified; the
//! parameters are recorded in the quality evidence. `P7_MESH_COARSEN=0` disables it.
use super::board_mesh::{point_in_segments,Seg,SegTree};
use geo::{Area,BoundingRect,MultiPolygon};
use rayon::prelude::*;
use rstar::PointDistance;
use spade::{ConstrainedDelaunayTriangulation,Point2,Triangulation};

/// Largest growth of the element size over one local element size.
pub(super) const GROWTH_PER_STEP:f64=1.5;
pub(super) const NEAR_STEPS:f64=10.;
pub(super) const MAX_STEPS:f64=32.;
/// Board diagonal divided by this bounds the coarsest element size.
pub(super) const DIAGONAL_DIVISOR:f64=50.;

pub(super) fn disabled()->bool {std::env::var("P7_MESH_COARSEN").as_deref()==Ok("0")}
/// Experiment overrides (`P7_MESH_COARSEN_MAX_STEPS`, `P7_MESH_COARSEN_DIAGONAL`); defaults are the agreed values.
fn knob(name:&str,default:f64)->f64 {std::env::var(name).ok().and_then(|v|v.parse::<f64>().ok()).filter(|v|v.is_finite()&&*v>0.).unwrap_or(default)}

pub(super) struct SizeField {
    shapes:Vec<(geo::Rect<f64>,SegTree)>,
    boundary:SegTree,
    pub h0:f64,
    pub h_max:f64,
    pub d0:f64,
}
impl SizeField {
    pub fn h_max_for(h0:f64,diagonal:f64)->f64 {(knob("P7_MESH_COARSEN_MAX_STEPS",MAX_STEPS)*h0).min(diagonal/knob("P7_MESH_COARSEN_DIAGONAL",DIAGONAL_DIVISOR))}
    /// None when nothing could be coarsened (no support, or a board not much larger than h0).
    pub fn new(support:&[MultiPolygon<f64>],h0:f64,diagonal:f64)->Option<Self> {
        let h_max=Self::h_max_for(h0,diagonal);
        if !(h0.is_finite()&&h0>0.&&h_max>=GROWTH_PER_STEP*h0) {return None;}
        let mut shapes=vec![];let mut segs=vec![];
        for m in support {
            let Some(b)=m.bounding_rect() else {continue};
            if m.unsigned_area()<=0. {continue;}
            let tree=super::board_mesh::build_seg_tree(m);
            segs.extend(tree.iter().map(|s|Seg{a:s.a,b:s.b}));
            shapes.push((b,tree));
        }
        if shapes.is_empty() {return None;}
        Some(SizeField{shapes,boundary:rstar::RTree::bulk_load(segs),h0,h_max,d0:NEAR_STEPS*h0})
    }
    /// Outline simplification tolerance at distance `d` from the support: zero within d0, then
    /// growing with slope 1/8 up to h_max/4.
    pub fn tolerance(&self,d:f64)->f64 {if d<=self.d0 {0.} else {(0.125*(d-self.d0)).min(self.h_max/4.)}}
    /// Smallest distance to the support over a ring's vertices and sampled edge points.
    pub fn ring_distance(&self,ring:&geo::LineString<f64>)->f64 {
        let step=2.*self.h0;let mut d=f64::INFINITY;
        for l in ring.lines() {
            d=d.min(self.distance(l.start.x,l.start.y));
            let len=(l.end.x-l.start.x).hypot(l.end.y-l.start.y);
            let n=((len/step).ceil() as usize).min(64);
            for k in 1..n {let t=k as f64/n as f64;d=d.min(self.distance(l.start.x+t*(l.end.x-l.start.x),l.start.y+t*(l.end.y-l.start.y)));}
            if d==0. {break;}
        }
        d
    }
    pub fn distance(&self,x:f64,y:f64)->f64 {
        let inside=self.shapes.iter().any(|(b,t)|x>=b.min().x&&x<=b.max().x&&y>=b.min().y&&y<=b.max().y&&point_in_segments(t,x,y));
        if inside {return 0.;}
        self.boundary.nearest_neighbor(&[x,y]).map_or(f64::INFINITY,|s|s.distance_2(&[x,y]).sqrt())
    }
    /// Local target element size: h0 within d0 of the support, then growing by at most
    /// GROWTH_PER_STEP per element size (slope GROWTH_PER_STEP-1), up to h_max.
    pub fn size(&self,x:f64,y:f64)->f64 {
        let d=self.distance(x,y);
        if d<=self.d0 {self.h0} else {(self.h0+(GROWTH_PER_STEP-1.)*(d-self.d0)).min(self.h_max)}
    }
    /// Area limit equivalent to the uniform limit `a0` at size h0.
    pub fn area_limit(&self,a0:f64,h:f64)->f64 {a0*(h/self.h0).powi(2)}
}

/// Insert interior vertices into every face larger than the size field allows there.
/// Circumcentres (centroids when the circumcentre leaves the face) never lie on a constraint edge.
/// Returns the number of inserted vertices.
pub(super) fn grade<F:FnMut()->super::Result<()>>(cdt:&mut ConstrainedDelaunayTriangulation<Point2<f64>>,field:&SizeField,a0:f64,max_vertices:usize,mut tick:F)->super::Result<usize> {
    let mut cached:Vec<f64>=vec![];let mut inserted=0usize;
    for _pass in 0..64 {
        tick()?;
        let positions:Vec<[f64;2]>=cdt.vertices().map(|v|[v.position().x,v.position().y]).collect();
        let fresh:Vec<f64>=positions[cached.len()..].par_iter().map(|p|field.size(p[0],p[1])).collect();
        cached.extend(fresh);
        let mut points:Vec<[f64;2]>=vec![];
        for f in cdt.inner_faces() {
            let v=f.vertices();let idx=v.map(|v|v.fix().index());
            let p=idx.map(|i|positions[i]);
            let area=0.5*((p[1][0]-p[0][0])*(p[2][1]-p[0][1])-(p[1][1]-p[0][1])*(p[2][0]-p[0][0])).abs();
            let h=idx.iter().map(|&i|cached[i]).fold(f64::INFINITY,f64::min);
            if area<=field.area_limit(a0,h) {continue;}
            points.push(split_point(&p));
        }
        if points.is_empty() {break;}
        if cdt.num_vertices()+points.len()>max_vertices {return Err(super::error("MEMORY_LIMIT","graded planar refinement exceeds node budget"));}
        for (k,q) in points.iter().enumerate() {
            if k%4096==0 {tick()?;}
            cdt.insert(Point2::new(q[0],q[1])).map_err(|e|super::error("BOARD_MESH",&e.to_string()))?;
        }
        inserted+=points.len();
    }
    Ok(inserted)
}
fn split_point(p:&[[f64;2];3])->[f64;2] {
    let centroid=[(p[0][0]+p[1][0]+p[2][0])/3.,(p[0][1]+p[1][1]+p[2][1])/3.];
    let (ax,ay)=(p[1][0]-p[0][0],p[1][1]-p[0][1]);let (bx,by)=(p[2][0]-p[0][0],p[2][1]-p[0][1]);
    let d=2.*(ax*by-ay*bx);
    if d==0. {return centroid;}
    let (a2,b2)=(ax*ax+ay*ay,bx*bx+by*by);
    let (ux,uy)=((by*a2-ay*b2)/d,(ax*b2-bx*a2)/d);
    // Barycentric coordinates of the circumcentre must leave margin to every edge.
    let det=ax*by-ay*bx;
    let l1=(ux*by-uy*bx)/det;let l2=(ax*uy-ay*ux)/det;let l0=1.-l1-l2;
    if l0>0.1&&l1>0.1&&l2>0.1 {[p[0][0]+ux,p[0][1]+uy]} else {centroid}
}


/// Boundaries a simplified copper ring must not newly touch (barrel annuli and bores).
pub(super) struct Keepouts {
    polys:Vec<geo::Polygon<f64>>,
    tree:rstar::RTree<rstar::primitives::GeomWithData<rstar::primitives::Rectangle<[f64;2]>,usize>>,
}
impl Keepouts {
    pub fn new<'a>(sets:impl IntoIterator<Item=&'a MultiPolygon<f64>>)->Self {
        let polys:Vec<geo::Polygon<f64>>=sets.into_iter().flat_map(|m|m.0.iter().cloned()).collect();
        let tree=rstar::RTree::bulk_load(polys.iter().enumerate().filter_map(|(i,p)|p.bounding_rect().map(|b|rstar::primitives::GeomWithData::new(rstar::primitives::Rectangle::from_corners([b.min().x,b.min().y],[b.max().x,b.max().y]),i))).collect());
        Keepouts{polys,tree}
    }
}
#[derive(Default,Clone,Debug,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(super) struct SimplifyStats {
    pub vertices_before:usize,
    pub vertices_after:usize,
    pub rings_simplified:usize,
    pub polygons_reverted:usize,
    pub copper_area_mm2:f64,
    pub abs_area_change_mm2:f64,
    pub total_relative_area_change:f64,
    pub max_polygon_relative_area_change:f64,
    pub max_tolerance_mm:f64,
    pub rings_total:usize,
    pub vertices_in_near_rings:usize,
    pub vertices_in_small_rings:usize,
    pub vertices_in_reverted_polygons:usize,
}
fn vertex_count(m:&MultiPolygon<f64>)->usize {m.0.iter().map(|p|std::iter::once(p.exterior()).chain(p.interiors()).map(|r|r.0.len()).sum::<usize>()).sum()}
fn ring_area(r:&geo::LineString<f64>)->f64 {geo::Polygon::new(r.clone(),vec![]).unsigned_area()}

/// Topology-preserving far-outline simplification of one layer's copper (approximation tier).
/// Rings within d0 of the support are untouched. Every accepted polygon is valid, does not newly
/// touch another polygon, barrel or bore, and respects the polygon (1e-3) and shared (`budget`) area bounds.
pub(super) fn simplify_set(set:&MultiPolygon<f64>,field:&SizeField,keep:&Keepouts,budget:&mut f64,st:&mut SimplifyStats)->MultiPolygon<f64> {
    use geo::{Contains,Intersects,SimplifyVwPreserve,Validation};
    use rstar::primitives::{GeomWithData,Rectangle};
    let n_before=vertex_count(set);st.vertices_before+=n_before;
    let pad=field.h_max/4.+1e-6;
    let mut cur=set.0.clone();
    let tree=rstar::RTree::bulk_load(cur.iter().enumerate().filter_map(|(i,p)|p.bounding_rect().map(|b|GeomWithData::new(Rectangle::from_corners([b.min().x-pad,b.min().y-pad],[b.max().x+pad,b.max().y+pad]),i))).collect::<Vec<_>>());
    let ring_d:Vec<Vec<f64>>=cur.par_iter().map(|p|std::iter::once(p.exterior()).chain(p.interiors()).map(|r|field.ring_distance(r)).collect()).collect();
    for (i,p) in cur.iter().enumerate() {for (k,r) in std::iter::once(p.exterior()).chain(p.interiors()).enumerate() {
        st.rings_total+=1;
        if field.tolerance(ring_d[i][k])<=0. {st.vertices_in_near_rings+=r.0.len();}
        else if r.0.len()<=5||(field.tolerance(ring_d[i][k]).powi(2)).min(2e-3*ring_area(r))<=0. {st.vertices_in_small_rings+=r.0.len();}
    }}
    let mut order:Vec<usize>=(0..cur.len()).collect();
    let far=|i:usize|ring_d[i].iter().copied().filter(|d|d.is_finite()).fold(0.,f64::max);
    order.sort_by(|&a,&b|far(b).total_cmp(&far(a)));
    for idx in order {
        if *budget<=0. {break;}
        if ring_d[idx].iter().all(|&d|field.tolerance(d)<=0.) {continue;}
        let p=cur[idx].clone();
        let old_rings:Vec<&geo::LineString<f64>>=std::iter::once(p.exterior()).chain(p.interiors()).collect();
        let (mut new_rings,mut changed,mut delta,mut tol)=(vec![],vec![],0.,0_f64);
        for (k,ring) in old_rings.iter().enumerate() {
            let t=field.tolerance(ring_d[idx][k]);
            let ra=ring_area(ring);let eps=(t*t).min(2e-3*ra);
            if eps>0.&&ring.0.len()>5 {
                let s=ring.simplify_vw_preserve(eps);
                if s.0.len()>=4&&s.0.len()<ring.0.len() {delta+=(ring_area(&s)-ra).abs();tol=tol.max(t);changed.push(k);new_rings.push(s);continue;}
            }
            new_rings.push((*ring).clone());
        }
        if changed.is_empty() {continue;}
        let pa=p.unsigned_area();
        if delta>*budget||delta>1e-3*pa {st.polygons_reverted+=1;st.vertices_in_reverted_polygons+=vertex_count(&MultiPolygon(vec![p.clone()]));continue;}
        let mut it=new_rings.into_iter();let ext=it.next().unwrap();
        let q=geo::Polygon::new(ext,it.collect());
        if !q.is_valid() {st.polygons_reverted+=1;st.vertices_in_reverted_polygons+=vertex_count(&MultiPolygon(vec![p.clone()]));continue;}
        let mut ok=true;
        'rings: for &k in &changed {
            let (old,new)=(old_rings[k],if k==0 {q.exterior()} else {&q.interiors()[k-1]});
            let Some(b)=geo::Polygon::new(new.clone(),vec![]).bounding_rect() else {continue};
            let env=rstar::AABB::from_corners([b.min().x-1e-6,b.min().y-1e-6],[b.max().x+1e-6,b.max().y+1e-6]);
            for g in tree.locate_in_envelope_intersecting(&env) {
                if g.data==idx {continue;}
                let o=&cur[g.data];
                if new.intersects(o)&&!old.intersects(o) {ok=false;break 'rings;}
                if k==0 {
                    let (np,op)=(geo::Polygon::new(new.clone(),vec![]),geo::Polygon::new(old.clone(),vec![]));
                    let pt=geo::Point::from(o.exterior().0[0]);
                    if np.contains(&pt)&&!op.contains(&pt) {ok=false;break 'rings;}
                }
            }
            for g in keep.tree.locate_in_envelope_intersecting(&env) {
                let o=&keep.polys[g.data];
                if new.intersects(o)&&!old.intersects(o) {ok=false;break 'rings;}
            }
        }
        if !ok {st.polygons_reverted+=1;st.vertices_in_reverted_polygons+=vertex_count(&MultiPolygon(vec![p.clone()]));continue;}
        *budget-=delta;st.abs_area_change_mm2+=delta;st.rings_simplified+=changed.len();
        st.max_polygon_relative_area_change=st.max_polygon_relative_area_change.max(delta/pa);st.max_tolerance_mm=st.max_tolerance_mm.max(tol);
        cur[idx]=q;
    }
    let out=MultiPolygon(cur);
    st.vertices_after+=vertex_count(&out);st.copper_area_mm2+=set.unsigned_area();
    st.total_relative_area_change=if st.copper_area_mm2>0. {st.abs_area_change_mm2/st.copper_area_mm2} else {0.};
    out
}

/// (maximum aspect = longest edge / shortest altitude, minimum angle in degrees) over all inner faces.
pub(super) fn quality(cdt:&ConstrainedDelaunayTriangulation<Point2<f64>>,min_area:f64)->(f64,f64,[[f64;2];3],f64) {
    let (mut aspect,mut min_angle)=(0_f64,180_f64);let mut worst=[[0.;2];3];let mut fixable=0_f64;
    for f in cdt.inner_faces() {
        let p=f.vertices().map(|v|[v.position().x,v.position().y]);
        let l2=[0,1,2].map(|k|{let (a,b)=(p[k],p[(k+1)%3]);(b[0]-a[0]).powi(2)+(b[1]-a[1]).powi(2)});
        let area2=((p[1][0]-p[0][0])*(p[2][1]-p[0][1])-(p[1][1]-p[0][1])*(p[2][0]-p[0][0])).abs();
        let longest=l2.iter().copied().fold(0.,f64::max);
        let a=if area2>0. {longest/area2} else {f64::INFINITY};
        if a>aspect {aspect=a;worst=p;}
        if area2*0.5>=min_area {fixable=fixable.max(a);}
        // The smallest angle lies between the two longer edges: sin = 2A / (l1 l2).
        let mut sorted=l2;sorted.sort_by(f64::total_cmp);
        let sine=area2/(sorted[1]*sorted[2]).sqrt();
        if sine.is_finite() {min_angle=min_angle.min(sine.min(1.).asin().to_degrees());}
    }
    (aspect,min_angle,worst,fixable)
}
/// Chew/Ruppert-style minimum-angle refinement. Constraint edges may be split (collinear
/// Steiner points; the outline is unchanged). Returns true when the angle target was reached
/// within the insertion cap.
pub(super) fn improve(cdt:&mut ConstrainedDelaunayTriangulation<Point2<f64>>,angle_deg:f64,min_area:f64,cap:usize)->bool {
    // Faces below `min_area` are grid-noise slivers between coincident boundaries; Steiner points cannot fix them.
    cdt.refine(spade::RefinementParameters::new().with_angle_limit(spade::AngleLimit::from_deg(angle_deg)).with_min_required_area(min_area).with_max_additional_vertices(cap)).refinement_complete
}
pub(super) fn quality_angle()->f64 {knob("P7_MESH_QUALITY_ANGLE",20.)}
pub(super) fn quality_cap(n0:usize)->usize {(knob("P7_MESH_QUALITY_CAP",(n0/5).clamp(1000,8000) as f64)) as usize}
pub(super) fn quality_min_area()->f64 {knob("P7_MESH_QUALITY_MIN_AREA",1e-4)}
pub(super) fn quality_disabled()->bool {std::env::var("P7_MESH_QUALITY_ANGLE").as_deref()==Ok("0")}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{LineString,Polygon};
    use spade::{AngleLimit,RefinementParameters};
    fn rect(x0:f64,y0:f64,x1:f64,y1:f64)->Polygon<f64> {Polygon::new(LineString::from(vec![[x0,y0],[x1,y0],[x1,y1],[x0,y1],[x0,y0]]),vec![])}
    #[test]
    fn grading_keeps_constraints_and_coarsens_only_far_from_support() {
        let support=[MultiPolygon(vec![rect(5.,5.,9.,7.)])];
        let (a0,h0)=(1.,1.);
        let field=SizeField::new(&support,h0,200_f64.hypot(100.)).unwrap();
        assert!((field.h_max-200_f64.hypot(100.)/50.).abs()<1e-12&&field.size(7.,6.)==h0&&field.size(190.,90.)==field.h_max);
        // Growth never exceeds 1.5 per element size: h(d+h)<=1.5h(d).
        let (x0,mut prev)=(9.+field.d0,field.size(9.+field.d0,6.));
        for k in 1..40 {let x=x0+0.25*k as f64;let h=field.size(x,6.);assert!(h>=prev&&h<=prev+0.5*0.25+1e-9);prev=h;}
        let build=||{
            let mut cdt=ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
            let ring=[[0.,0.],[200.,0.],[200.,100.],[0.,100.]];
            let ids:Vec<_>=ring.iter().map(|p|cdt.insert(Point2::new(p[0],p[1])).unwrap()).collect();
            for k in 0..4 {cdt.add_constraint(ids[k],ids[(k+1)%4]);}
            for p in [[63.7,41.3],[140.2,77.9],[20.1,70.4]] {cdt.insert(Point2::new(p[0],p[1])).unwrap();}
            cdt
        };
        let params=|area:f64|RefinementParameters::new().keep_constraint_edges().with_angle_limit(AngleLimit::from_deg(0.)).with_max_allowed_area(area).with_max_additional_vertices(10_000_000);
        let mut graded=build();
        graded.refine(params(field.area_limit(a0,field.h_max)));
        grade(&mut graded,&field,a0,usize::MAX,||Ok(())).unwrap();
        // A uniform mesh at area <= 1 mm2 over 20000 mm2 needs at least 10000 vertices.
        assert!(graded.num_vertices()*5<10_000,"{}",graded.num_vertices());
        assert_eq!(graded.num_constraints(),4);
        for f in graded.inner_faces() {
            let p=f.vertices().map(|v|[v.position().x,v.position().y]);
            let area=0.5*((p[1][0]-p[0][0])*(p[2][1]-p[0][1])-(p[1][1]-p[0][1])*(p[2][0]-p[0][0])).abs();
            let h=p.iter().map(|q|field.size(q[0],q[1])).fold(f64::INFINITY,f64::min);
            assert!(area<=field.area_limit(a0,h)*(1.+1e-9),"face area {area} h {h}");
        }
    }
}
