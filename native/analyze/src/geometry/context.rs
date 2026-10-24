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
    pub fn metadata(&self) -> ([f64;2],f64,f64) {(self.adapter.offset,self.adapter.inv_scale,self.error_bound())}
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
