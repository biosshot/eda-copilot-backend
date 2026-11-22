//! Deterministic bounding-box sweep, followed by narrow phase in topology.rs.
use geo::{BoundingRect, Polygon, Rect};
use rstar::{
    primitives::{GeomWithData, Rectangle},
    RTree, AABB,
};
pub struct Index(RTree<GeomWithData<Rectangle<[f64; 2]>, usize>>);
impl Index {
    pub fn new(bounds: impl IntoIterator<Item = (usize, Rect<f64>)>) -> Self {
        Self(RTree::bulk_load(
            bounds
                .into_iter()
                .map(|(id, b)| {
                    GeomWithData::new(
                        Rectangle::from_corners([b.min().x, b.min().y], [b.max().x, b.max().y]),
                        id,
                    )
                })
                .collect(),
        ))
    }
    pub fn query(&self, b: Rect<f64>, padding: f64) -> Vec<usize> {
        let box_ = AABB::from_corners(
            [b.min().x - padding, b.min().y - padding],
            [b.max().x + padding, b.max().y + padding],
        );
        let mut ids: Vec<_> = self
            .0
            .locate_in_envelope_intersecting(&box_)
            .map(|v| v.data)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}
pub fn candidates(polys: &[Polygon<f64>], padding: f64) -> Vec<(usize, usize)> {
    let bounds: Vec<Rect<f64>> = polys.iter().map(|p| p.bounding_rect().unwrap()).collect();
    let mut order: Vec<_> = (0..polys.len()).collect();
    order.sort_by(|&a, &b| {
        bounds[a]
            .min()
            .x
            .total_cmp(&bounds[b].min().x)
            .then(a.cmp(&b))
    });
    let mut active: Vec<usize> = Vec::new();
    let mut pairs = Vec::new();
    for i in order {
        active.retain(|&j| bounds[j].max().x + padding >= bounds[i].min().x);
        for &j in &active {
            if bounds[j].min().y <= bounds[i].max().y + padding
                && bounds[j].max().y + padding >= bounds[i].min().y
            {
                pairs.push((j.min(i), j.max(i)));
            }
        }
        active.push(i);
    }
    pairs.sort_unstable();
    pairs
}
