//! Exact CPU-generated orientation templates and ordered translation histories.
//! A repair pose must replay each rounding operation, not a summed delta.
use super::{WorkingPrimitive, ComponentGeometry, Primitive};
use std::sync::Arc;
use smallvec::SmallVec;

#[derive(Clone)]
pub(super) struct Template {
    pub id: u32,
    pub primitive: Primitive,
    pub components: Arc<Vec<(usize, ComponentGeometry)>>,
    pub placement_ids: Arc<Vec<u32>>,
    pub point_component_ids: Arc<Vec<Option<u32>>>,
}

#[derive(Clone)]
pub(super) struct Pose {
    pub template: Arc<Template>,
    pub translations: SmallVec<[(f32, f32); 4]>,
    pub deferred: bool,
    pub packing_bounds: crate::geometry::Box2,
}

impl Pose {
    pub fn from_primitive(p: &WorkingPrimitive) -> Self {
        Self { template: Arc::new(Template { id: p.id, primitive: p.primitive.clone(), components: p.components.clone(),
            placement_ids:p.placement_ids.clone(), point_component_ids:p.point_component_ids.clone() }),
            translations: SmallVec::new(), deferred: false, packing_bounds: super::packing_box(p) }
    }
}

// Only accepted/shortlisted proposals need their component and pad geometry.
pub(super) fn materialize(p: &mut WorkingPrimitive) {
    let Some(pose) = p.gpu_pose.take() else { return; };
    if !pose.deferred { p.gpu_pose = Some(pose); return; }
    p.primitive = pose.template.primitive.clone();
    p.components = pose.template.components.clone();
    for &(dx,dy) in &pose.translations { super::translate_primitive(p,dx,dy); }
    p.gpu_pose = Some(Pose { deferred: false, ..pose });
}

pub(super) fn coordinate(pose: &Pose, mut value: f32, axis: usize) -> f32 {
    for &(x,y) in &pose.translations { value = crate::geometry::round_placement(value + if axis==0 {x}else{y}); }
    value
}

impl Template {
    pub fn key(&self) -> Vec<u32> {
        let p=&self.primitive;
        let mut key=vec![self.id as u32,p.collision_boxes.len() as u32,self.components.len() as u32];
        let mut box_key=|b:crate::geometry::Box2| {key.extend([b.left.to_bits(),b.right.to_bits(),b.top.to_bits(),b.bottom.to_bits()]);};
        box_key(p.bbox);for b in p.collision_boxes.iter() {box_key(*b);}
        for (_,c) in self.components.iter() {box_key(c.body_box);for b in c.through_hole_boxes.iter() {box_key(*b);}}
        for q in p.connection_points.iter() {key.extend([q.x.to_bits(),q.y.to_bits()]);}
        for q in p.placements.iter() {key.extend([q.x.to_bits(),q.y.to_bits(),q.rotate as u32]);}
        for q in p.path_ports.iter() {key.extend([q.x.to_bits(),q.y.to_bits(),q.normal.x.to_bits(),q.normal.y.to_bits()]);}
        key
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::{round_placement, translate_box, Box2};

    #[test]
    fn repeated_translation_cannot_be_collapsed() {
        let b = Box2 { left: 0.0, right: 1.0, top: 0.0, bottom: 1.0 };
        let moved = translate_box(&translate_box(&b, 0.0004, 0.0), 0.0004, 0.0);
        assert_eq!(moved.left, 0.0);
        assert_eq!(round_placement(b.left + 0.0008), 0.001);
    }
}
