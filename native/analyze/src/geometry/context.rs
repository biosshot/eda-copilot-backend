//! One board-scoped dyadic overlay grid. No feature/area filtering.
use super::{areas, GeometryOptions};
use crate::model::{Board, CadFillState, Drill, DrillShape, PadShape, Point};
use geo::Coord;
use i_overlay::{core::{fill_rule::FillRule, overlay::ShapeType, overlay_rule::OverlayRule},
    float::overlay::{FloatOverlay, OverlayOptions},
    i_float::{adapter::FloatPointAdapter, float::rect::FloatRect}};
use std::cell::RefCell;

#[derive(Clone)]
pub(crate) struct Context {
    adapter: FloatPointAdapter<[f64; 2], f64>,
    frame: Option<(Point, f64)>,
}
thread_local! {static ACTIVE: RefCell<Option<Context>> = const {RefCell::new(None)};}
pub(crate) struct Scope(Option<Context>);
impl Drop for Scope {fn drop(&mut self) { ACTIVE.with(|a| *a.borrow_mut()=self.0.take()); }}
impl Context {
    pub fn install(value: Option<Self>) -> Scope {
        Scope(ACTIVE.with(|a| a.replace(value)))
    }
    pub fn frame(at: Point, rotation: f64) -> Scope {
        let value=ACTIVE.with(|a| a.borrow().clone()).map(|mut c| {c.frame=Some((at,rotation));c});
        Self::install(value)
    }
    pub fn error_bound(&self) -> f64 {16. * 2_f64.sqrt() * self.adapter.inv_scale}
    /// Arrangement grid step in mm (the adapter's `inv_scale` is 1/scale).
    pub fn grid_step_mm(&self) -> f64 {self.adapter.inv_scale}
    pub fn metadata(&self) -> ([f64;2],f64,f64) {(self.adapter.offset,self.adapter.inv_scale,self.error_bound())}
    /// Apply exactly the same lattice mapping as this context's overlay input.
    pub(crate) fn canonical_point(&self, point: [f64;2]) -> [f64;2] {
        self.adapter.int_to_float(&self.adapter.float_to_int(&point))
    }
    /// Nested local grid for already clipped polygons. Prepared board vertices
    /// remain exactly representable; only newly noded intersections gain bits.
    pub fn refined_for_bounds(&self,bounds:[[f64;2];2])->Result<Self,String> {
        self.refined_grid(bounds,28)
    }
    /// Final arrangements contain bounded, already clipped inputs. Use the
    /// pinned kernel range (abs coordinate <2^30; cross_solver.rs), preserving
    /// inherited dyadic vertices exactly. This improves intersection precision
    /// without changing copper/contact conservation tolerances.
    pub fn refined_for_arrangement(&self,bounds:[[f64;2];2])->Result<Self,String> {
        self.refined_grid(bounds,30)
    }
    fn refined_grid(&self,bounds:[[f64;2];2],bits:u32)->Result<Self,String> {
        let span=(bounds[1][0]-bounds[0][0]).max(bounds[1][1]-bounds[0][1]);
        if !span.is_finite()||span<=0. {return Err("Invalid local overlay extent".into());}
        let inherited=1./self.adapter.inv_scale;
        let padded=2_f64.powf(((1u64<<bits.min(29))as f64/(span*0.625)).log2().floor()).max(inherited);
        // Keep already fine accepted local lattices stable. For broad final
        // arrangements request a 2^-25 mm step, bounded by the pinned kernel's
        // actual range; this is one grid selection, never a retry after failure.
        // Two integer units reserve origin/end-point rounding at tight bounds.
        let safe=2_f64.powf((((1u64<<30)-2)as f64/(span*0.5)).log2().floor());
        let scale=if bits==30 {padded.max(2_f64.powi(25).min(safe))}else{padded};
        let origin=std::array::from_fn::<_,2,_>(|k|((bounds[0][k]+(bounds[1][k]-bounds[0][k])/2.)*scale).round()/scale);
        if !scale.is_finite() || (0..2).any(|k|
            (bounds[0][k]-origin[k]).abs().max((bounds[1][k]-origin[k]).abs())*scale+1. >= (1u64<<30)as f64) {
            return Err("Local overlay extent exceeds the pinned kernel integer range".into());
        }
        let radius=(1u64<<30)as f64/scale;
        let rect=FloatRect{min_x:origin[0]-radius,max_x:origin[0]+radius,min_y:origin[1]-radius,max_y:origin[1]+radius};
        Ok(Self{adapter:FloatPointAdapter::with_scale(rect,scale),frame:None})
    }
    pub fn for_board(b: &Board, options: GeometryOptions) -> Result<Self,String> {
        let mut lo=[f64::INFINITY;2];let mut hi=[f64::NEG_INFINITY;2];
        let mut add=|p:Point,r:f64| {
            for (k,v) in [p.x,p.y].iter().enumerate(){lo[k]=lo[k].min(v-r);hi[k]=hi[k].max(v+r);}
        };
        for p in b.outline.iter().chain(b.cutouts.iter().flatten()){add(*p,0.);}
        for t in &b.tracks {for p in &t.points {add(*p,t.width_mm/2.);}}
        for v in &b.vias {add(v.at,v.diameter_mm.max(v.drill_mm));}
        fn shape(s:&PadShape)->f64 {match s {
            PadShape::Circle{diameter_mm}=>diameter_mm/2.,
            PadShape::Rect{width_mm,height_mm}|PadShape::Oval{width_mm,height_mm}|PadShape::RoundRect{width_mm,height_mm,..}=>width_mm.hypot(*height_mm)/2.,
            PadShape::Polygon(p)=>p.outer.iter().chain(p.holes.iter().flatten()).map(|p|p.x.hypot(p.y)).fold(0.,f64::max),
        }}
        fn drill(d:&Drill)->f64 {d.offset.map_or(0.,|p|p.x.hypot(p.y)) + match d.shape {DrillShape::Round=>d.diameter_mm,DrillShape::Slot{length_mm}=>length_mm+d.diameter_mm}}
        for p in &b.pads {
            let mut r=shape(&p.common_shape);
            if let Some(s)=&p.layer_shapes {for s in s.values(){r=r.max(shape(s));}}
            if let Some(d)=&p.drill {r=r.max(drill(d));} add(p.at,r);
        }
        if let Some(ds)=&b.drills {for d in ds {add(d.at,drill(&d.drill));}}
        for z in &b.zones {for p in z.boundary.outer.iter().chain(z.boundary.holes.iter().flatten()){add(*p,0.);}
            for f in &z.fills {if let CadFillState::Available{polygons,..}=&f.state {for p in polygons {for p in p.outer.iter().chain(p.holes.iter().flatten()){add(*p,0.);}}}}
        }
        let span=(hi[0]-lo[0]).max(hi[1]-lo[1]);
        if !span.is_finite() || span<=0. {return Err("Invalid common overlay extent".into());}
        // 1/8 extent margin covers enclosing drills and overlay guards. Keep
        // all coordinates below2^29, within the library's integer range.
        let half=span*0.625+8.*options.max_deviation_mm;
        let scale=2_f64.powf(((1u64<<28) as f64/half).log2().floor());
        let origin=std::array::from_fn::<_,2,_>(|k|((lo[k]+(hi[k]-lo[k])/2.)*scale).round()/scale);
        let radius=(1u64<<29) as f64/scale;
        let rect=FloatRect{min_x:origin[0]-radius,max_x:origin[0]+radius,min_y:origin[1]-radius,max_y:origin[1]+radius};
        let c=Self{adapter:FloatPointAdapter::with_scale(rect,scale),frame:None};
        if !scale.is_finite() {
            return Err("Common overlay extent cannot fit the existing geometry approximation budget".into());
        }
        Ok(c)
    }
    fn global(&self,p:[f64;2])->[f64;2] {if let Some((at,r))=self.frame {let p=super::primitives::transform_point(Point{x:p[0],y:p[1]},at,r);[p.x,p.y]}else{p}}
    fn local(&self,p:[f64;2])->[f64;2] {if let Some((at,r))=self.frame {let (s,c)=r.to_radians().sin_cos();let x=p[0]-at.x;let y=p[1]-at.y;[x*c+y*s,-x*s+y*c]}else{p}}
}
pub(crate) fn error_bound()->f64 {ACTIVE.with(|a|a.borrow().as_ref().map_or(0.,Context::error_bound))}
pub(crate) fn overlay(a:&areas::AreaSet,b:&areas::AreaSet,rule:OverlayRule)->Option<areas::AreaSet> {
    ACTIVE.with(|active| {let borrow=active.borrow();let c=borrow.as_ref()?;
        let paths=|g:&areas::AreaSet| {use geo::Orient;g.0.iter().map(|p|p.orient(geo::orient::Direction::Default)).map(|p|{
            std::iter::once(p.exterior()).chain(p.interiors()).map(|r|r.0[..r.0.len()-1].iter().map(|p|c.global([p.x,p.y])).collect::<Vec<_>>()).collect::<Vec<_>>()
        }).collect::<Vec<_>>()};
        let aa=paths(a);let bb=paths(b);let cap=aa.iter().chain(&bb).flatten().map(Vec::len).sum();
        let options=OverlayOptions{preserve_input_collinear:false,preserve_output_collinear:false,clean_result:false,min_output_area:0.,..Default::default()};
        let mut overlay=FloatOverlay::new_custom(c.adapter.clone(),options,Default::default(),cap)
            .unsafe_add_source(&aa,ShapeType::Subject).unsafe_add_source(&bb,ShapeType::Clip);
        let shapes=overlay.overlay(rule,FillRule::NonZero);
        Some(geo::MultiPolygon(shapes.into_iter().map(|rings|{
            let mut rings=rings.into_iter().map(|r| {let mut p:Vec<_>=r.into_iter().map(|p|{let p=c.local(p);Coord{x:p[0],y:p[1]}}).collect();if let Some(v)=p.first().copied(){p.push(v);}geo::LineString(p)});
            geo::Polygon::new(rings.next().unwrap(),rings.collect())
        }).collect()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{Area, LineString, Polygon};
    fn context() -> Context {
        let rect=FloatRect{min_x:-128.,max_x:128.,min_y:-128.,max_y:128.};
        Context{adapter:FloatPointAdapter::with_scale(rect,2_f64.powi(20)),frame:None}
    }
    fn box_at(x:f64,y:f64,w:f64,h:f64)->areas::AreaSet {
        areas::one(Polygon::new(LineString::from(vec![[x,y],[x+w,y],[x+w,y+h],[x,y+h],[x,y]]),vec![]))
    }
    #[test]
    fn arrangement_uses_checked_kernel_range_and_preserves_inherited_vertices() {
        for shift in [0.,10000.,-10000.] {
            let c=context().refined_for_arrangement([[shift,shift],[shift+60.1,shift+40.1]]).unwrap();
            assert_eq!(c.adapter.dir_scale,2_f64.powi(25));
            for p in [[shift,shift],[shift+60.1,shift+40.1]] {
                let q=c.adapter.float_to_int(&p);
                assert!(i64::from(q.x).abs()<1<<30 && i64::from(q.y).abs()<1<<30);
            }
            let p=[shift+12.25,shift+7.5];
            assert_eq!(c.adapter.int_to_float(&c.adapter.float_to_int(&p)),p);
        }
    }
    #[test]
    fn wide_arrangement_retains_thin_ring_real_gap_bridge_and_source_subdivision() {
        for (shift,angle) in [(0.,0_f64),(1000.,37.),(-1000.,90.)] {
            let c=context().refined_for_arrangement([[shift-40.,shift-40.],[shift+40.,shift+40.]]).unwrap();
            let _scope=Context::install(Some(c));
            let transform=|set:areas::AreaSet| {use geo::MapCoords;set.map_coords(|p| {
                let(s,c)=angle.to_radians().sin_cos();Coord{x:shift+p.x*c-p.y*s,y:shift+p.x*s+p.y*c}
            })};
            let outer=transform(box_at(-0.3,-0.3,0.6,0.6));
            let inner=transform(box_at(-0.299,-0.299,0.598,0.598));
            let ring=areas::difference(&outer,&inner);
            assert_eq!(ring.0.len(),1);assert_eq!(ring.0[0].interiors().len(),1);
            assert!((ring.unsigned_area()/(0.36-0.598_f64.powi(2))-1.).abs()<1e-4);
            let left=transform(box_at(1.,0.,1.,1.));
            let right=transform(box_at(2.0002,0.,1.,1.));
            assert_eq!(areas::union(left.0.iter().chain(&right.0).cloned()).0.len(),2);
            let bridge=transform(box_at(1.9,0.4,0.2,0.0002));
            assert_eq!(areas::union(left.0.iter().chain(&right.0).chain(&bridge.0).cloned()).0.len(),1);
            let whole=transform(box_at(4.,0.,2.,1.));
            let a=transform(box_at(4.,0.,1.,1.));let b=transform(box_at(5.,0.,1.,1.));
            let joined=areas::union(a.0.into_iter().chain(b.0));
            assert!((joined.unsigned_area()-whole.unsigned_area()).abs()<1e-7);
        }
    }
}
