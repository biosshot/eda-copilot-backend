//! Sparse, per-batch virtual copper. Cell ranges are lookup bounds, not keepouts.
use super::{Cell, EPS};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct TemporaryRoutes {
    cells: HashMap<Cell, Occupancy>,
}

struct Occupancy {
    net: Arc<str>,
    // Only actual reconstructed planar edges, never inferred from nearby cells.
    edges: Vec<Cell>,
}

impl TemporaryRoutes {
    pub(super) fn reserve(&mut self, path: &[Cell], net: &Arc<str>) {
        for &cell in path {
            self.cells.entry(cell).or_insert_with(|| Occupancy { net: net.clone(), edges: Vec::new() });
        }
        for pair in path.windows(2) {
            if pair[0].layer == pair[1].layer {
                self.cells.get_mut(&pair[0]).unwrap().edges.push(pair[1]);
                self.cells.get_mut(&pair[1]).unwrap().edges.push(pair[0]);
            }
        }
    }

    /// Test the entire candidate segment against reserved centerlines. A via
    /// transition tests its landing point on both layers (same diameter model
    /// as the existing estimator; this is not a manufacturing via DRC).
    pub(super) fn conflicts(&self, a: Cell, b: Cell, net: &Arc<str>, grid: f64, spacing: f64) -> bool {
        if self.cells.is_empty() { return false; }
        if a.layer != b.layer {
            return self.conflicts(a, a, net, grid, spacing)
                || self.conflicts(b, b, net, grid, spacing);
        }
        let radius = (spacing / grid).ceil() as i32 + 1;
        let limit_squared = (spacing / grid).powi(2);
        for x in a.x.min(b.x) - radius..=a.x.max(b.x) + radius {
            for y in a.y.min(b.y) - radius..=a.y.max(b.y) + radius {
                let cell = Cell { x, y, layer: a.layer };
                let Some(occupied) = self.cells.get(&cell) else { continue };
                if &occupied.net == net { continue; }
                if point_segment_squared(cell, a, b) + EPS < limit_squared { return true; }
                for &end in &occupied.edges {
                    if segments_squared(a, b, cell, end) + EPS < limit_squared { return true; }
                }
            }
        }
        false
    }
}

fn point_segment_squared(p: Cell, a: Cell, b: Cell) -> f64 {
    let dx = (b.x - a.x) as f64;
    let dy = (b.y - a.y) as f64;
    let px = (p.x - a.x) as f64;
    let py = (p.y - a.y) as f64;
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared <= EPS { 0.0 } else { ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0) };
    (px - t * dx).powi(2) + (py - t * dy).powi(2)
}

fn segments_squared(a: Cell, b: Cell, c: Cell, d: Cell) -> f64 {
    let cross = |p: Cell, q: Cell, r: Cell| -> f64 {
        (q.x - p.x) as f64 * (r.y - p.y) as f64 - (q.y - p.y) as f64 * (r.x - p.x) as f64
    };
    if a.x.min(b.x) <= c.x.max(d.x) && c.x.min(d.x) <= a.x.max(b.x)
        && a.y.min(b.y) <= c.y.max(d.y) && c.y.min(d.y) <= a.y.max(b.y)
        && cross(a, b, c) * cross(a, b, d) <= 0.0
        && cross(c, d, a) * cross(c, d, b) <= 0.0
    { return 0.0; }
    point_segment_squared(a, c, d).min(point_segment_squared(b, c, d))
        .min(point_segment_squared(c, a, b)).min(point_segment_squared(d, a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cell(x: i32, y: i32) -> Cell { Cell { x, y, layer: 0 } }
    #[test]
    fn half_mm_is_not_a_square_half_mm_keepout() {
        let mut routes = TemporaryRoutes::default();
        routes.reserve(&[cell(0, 0), cell(1, 0)], &Arc::from("N"));
        assert!(!routes.conflicts(cell(0, 2), cell(1, 2), &Arc::from("P"), 0.25, 0.381));
        assert!(routes.conflicts(cell(0, 1), cell(1, 1), &Arc::from("P"), 0.25, 0.381));
        assert!(!routes.conflicts(cell(0, 0), cell(1, 0), &Arc::from("N"), 0.25, 0.381));
    }
    #[test]
    fn edges_and_via_landings_are_obstacles_on_their_layers() {
        let mut routes = TemporaryRoutes::default();
        let bottom = Cell { layer: 1, ..cell(0, 0) };
        routes.reserve(&[cell(0, 0), bottom], &Arc::from("N"));
        assert!(routes.conflicts(bottom, bottom, &Arc::from("P"), 0.25, 0.381));
        let other_layer = Cell { layer: 2, ..bottom };
        assert!(!routes.conflicts(other_layer, other_layer, &Arc::from("P"), 0.25, 0.381));
        routes.reserve(&[cell(-1, 0), cell(1, 0)], &Arc::from("N"));
        assert!(routes.conflicts(cell(0, -1), cell(0, 1), &Arc::from("P"), 1.0, 0.1));
    }
}
