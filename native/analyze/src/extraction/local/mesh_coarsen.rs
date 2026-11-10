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
    fn distance(&self,x:f64,y:f64)->f64 {
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
