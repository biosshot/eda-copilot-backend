//! Per-batch virtual copper with a bounded direct-addressed occupancy index.
//! Cell ranges are lookup bounds, not keepouts; exact segment tests are unchanged.
use super::{Cell, EPS};
use super::grid::GridShape;
use crate::geometry::Box2;
use rustc_hash::FxHashMap;
use std::sync::Arc;

// u32 indices use at most 8 MiB. Occupancy data exists only for reserved cells.
const MAX_DENSE_CELLS: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct TemporaryRoutes {
    dense: Option<DenseIndex>,
    // Also stores out-of-range cells when a dense index is active.
    sparse: FxHashMap<Cell, u32>,
    occupied: Vec<Occupancy>,
    needs_segment_checks: bool,
}

struct DenseIndex {
    shape: GridShape,
    layers: usize,
    indices: Vec<u32>,
    touched: Vec<usize>,
}

impl DenseIndex {
    #[inline(always)]
    fn index(&self, cell: Cell) -> Option<usize> {
        if cell.layer >= self.layers { return None; }
        Some(cell.layer * self.shape.len + self.shape.index(cell.x, cell.y)?)
    }
}

struct Occupancy {
    net: Arc<str>,
    // Only actual reconstructed planar edges, never inferred from nearby cells.
    edges: Vec<Cell>,
}

impl TemporaryRoutes {
    pub(super) fn is_empty(&self) -> bool { self.occupied.is_empty() }

    pub(super) fn new(bounds: Box2, grid: f64, layers: usize) -> Self {
        Self::with_cell_limit(bounds, grid, layers, MAX_DENSE_CELLS)
    }

    fn with_cell_limit(bounds: Box2, grid: f64, layers: usize, max_cells: usize) -> Self {
        let dense = max_cells.checked_div(layers)
            .and_then(|limit| GridShape::new(bounds, grid, limit))
            .and_then(|shape| {
                let len = shape.len.checked_mul(layers)?;
                let mut indices = Vec::new();
                indices.try_reserve_exact(len).ok()?;
                indices.resize(len, 0);
                Some(DenseIndex { shape, layers, indices, touched: Vec::new() })
            });
        Self { dense, ..Self::default() }
    }

    /// Reuse the allocation across candidates, clearing only actually occupied
    /// indices. Geometry and nets from a previous candidate never survive.
    pub(super) fn clear(&mut self) {
        if let Some(dense) = self.dense.as_mut() {
            for index in dense.touched.drain(..) { dense.indices[index] = 0; }
        }
        self.sparse.clear();
        self.occupied.clear();
        self.needs_segment_checks = false;
    }

    #[inline(always)]
    fn get(&self, cell: Cell) -> Option<&Occupancy> {
        if let Some(dense) = &self.dense {
            if let Some(index) = dense.index(cell) {
                let id = dense.indices[index];
                return if id == 0 { None } else { Some(&self.occupied[id as usize - 1]) };
            }
        }
        if self.sparse.is_empty() { return None; }
        self.sparse.get(&cell).map(|&id| &self.occupied[id as usize - 1])
    }

    fn ensure_cell(&mut self, cell: Cell, net: &Arc<str>) -> usize {
        let dense_index = self.dense.as_ref().and_then(|dense| dense.index(cell));
        let existing = match dense_index {
            Some(index) => self.dense.as_ref().unwrap().indices[index],
            None => self.sparse.get(&cell).copied().unwrap_or(0),
        };
        if existing != 0 { return existing as usize - 1; }
        let index = self.occupied.len();
        let id = u32::try_from(index + 1).expect("temporary occupancy index exceeds u32");
        self.occupied.push(Occupancy { net: net.clone(), edges: Vec::new() });
        match dense_index {
            Some(slot) => {
                let dense = self.dense.as_mut().unwrap();
                dense.indices[slot] = id;
                dense.touched.push(slot);
            }
            None => { self.sparse.insert(cell, id); }
        }
        index
    }

    pub(super) fn reserve(&mut self, path: &[Cell], net: &Arc<str>) {
        for &cell in path { self.ensure_cell(cell, net); }
        for pair in path.windows(2) {
            if pair[0].layer == pair[1].layer {
                let a = self.ensure_cell(pair[0], net);
                let b = self.ensure_cell(pair[1], net);
                self.needs_segment_checks |= !unit_step(pair[0], pair[1])
                    || self.occupied[a].net != self.occupied[b].net;
                self.occupied[a].edges.push(pair[1]);
                self.occupied[b].edges.push(pair[0]);
            }
        }
    }

    /// Test actual centerlines and via landings with the original tolerances.
    pub(super) fn conflicts(&self, a: Cell, b: Cell, net: &Arc<str>, grid: f64, spacing: f64) -> bool {
        if self.occupied.is_empty() { return false; }
        if a.layer != b.layer {
            return self.conflicts(a, a, net, grid, spacing)
                || self.conflicts(b, b, net, grid, spacing);
        }
        // Two axis-aligned unit edges on an integer grid cannot cross in their
        // interiors. Their minimum separation is attained at an endpoint of
        // the reserved edge; both endpoints are in occupied. For router paths
        // it is therefore sufficient to test points, without scanning edges or
        // the extra ring needed by the general segment predicate.
        let unit_edges = !self.needs_segment_checks && unit_step(a, b);
        let radius = (spacing / grid).ceil() as i32 + i32::from(!unit_edges);
        let limit_squared = (spacing / grid).powi(2);
        for y in a.y.min(b.y) - radius..=a.y.max(b.y) + radius {
            for x in a.x.min(b.x) - radius..=a.x.max(b.x) + radius {
                let cell = Cell { x, y, layer: a.layer };
                let Some(occupied) = self.get(cell) else { continue };
                if &occupied.net == net { continue; }
                if point_segment_squared(cell, a, b) + EPS < limit_squared { return true; }
                if unit_edges { continue; }
                for &end in &occupied.edges {
                    if segments_squared(a, b, cell, end) + EPS < limit_squared { return true; }
                }
            }
        }
        false
    }
}

fn unit_step(a: Cell, b: Cell) -> bool {
    (i64::from(a.x) - i64::from(b.x)).abs() + (i64::from(a.y) - i64::from(b.y)).abs() <= 1
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

#[cfg(test)]
mod dense_tests {
    use super::*;
#[derive(Default)]
pub(super) struct LegacyTemporaryRoutes {
    cells: FxHashMap<Cell, LegacyOccupancy>,
}

struct LegacyOccupancy {
    net: Arc<str>,
    // Only actual reconstructed planar edges, never inferred from nearby cells.
    edges: Vec<Cell>,
}

impl LegacyTemporaryRoutes {
    pub(super) fn reserve(&mut self, path: &[Cell], net: &Arc<str>) {
        for &cell in path {
            self.cells.entry(cell).or_insert_with(|| LegacyOccupancy { net: net.clone(), edges: Vec::new() });
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
        for y in a.y.min(b.y) - radius..=a.y.max(b.y) + radius {
            for x in a.x.min(b.x) - radius..=a.x.max(b.x) + radius {
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

    fn bounds() -> Box2 { Box2 { left: -3.0, right: 3.0, top: -3.0, bottom: 3.0 } }
    fn c(x: i32, y: i32, layer: usize) -> Cell { Cell { x, y, layer } }

    #[test]
    fn dense_and_sparse_match_legacy_centerlines_for_all_probe_moves() {
        let mut dense = TemporaryRoutes::new(bounds(), 1.0, 2);
        let mut sparse = TemporaryRoutes::default();
        let mut legacy = LegacyTemporaryRoutes::default();
        assert!(dense.dense.is_some());
        for (path, net) in [
            (vec![c(0,0,0), c(1,0,0), c(1,1,0), c(1,1,1), c(2,1,1)], "A"),
            (vec![c(2,3,0), c(5,3,0)], "B"),
            (vec![c(3,2,1), c(5,4,1)], "A"),
            (vec![c(-1,2,0), c(0,2,0)], "B"),
            (vec![c(6,6,0), c(7,6,0)], "B"),
            (vec![c(2,2,3), c(3,2,3)], "A"),
            (vec![c(1,1,0), c(1,1,0), c(2,1,0)], "A"),
            // Preserve the legacy first-owner behavior even for duplicate cells.
            (vec![c(0,0,0)], "B"),
        ] {
            let net = Arc::from(net);
            dense.reserve(&path, &net); sparse.reserve(&path, &net); legacy.reserve(&path, &net);
        }
        for layer in 0..4 {
            for x in -2..=8 {
                for y in -2..=8 {
                    let a = c(x,y,layer);
                    for b in [a, c(x+1,y,layer), c(x-1,y,layer), c(x,y+1,layer),
                        c(x,y-1,layer), c(x+2,y+2,layer), c(x,y,(layer+1)%4)] {
                        for name in ["A", "B", "OTHER"] {
                            let net = Arc::from(name);
                            for spacing in [0.0, 0.1, 0.381, 0.5, 1.000000001] {
                                let expected = legacy.conflicts(a,b,&net,0.25,spacing);
                                assert_eq!(dense.conflicts(a,b,&net,0.25,spacing), expected, "dense {a:?} {b:?} {name} {spacing}");
                                assert_eq!(sparse.conflicts(a,b,&net,0.25,spacing), expected, "sparse {a:?} {b:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dense_clear_reuses_allocation_and_removes_overflow_and_edges() {
        let mut routes = TemporaryRoutes::new(bounds(), 1.0, 2);
        let ptr = routes.dense.as_ref().unwrap().indices.as_ptr();
        routes.reserve(&[c(0,0,0),c(1,0,0),c(1,0,1)], &Arc::from("A"));
        routes.reserve(&[c(-1,0,0),c(0,0,0)], &Arc::from("A"));
        routes.reserve(&[c(8,8,3)], &Arc::from("A"));
        routes.clear();
        assert!(routes.occupied.is_empty() && routes.sparse.is_empty());
        assert!(routes.dense.as_ref().unwrap().touched.is_empty());
        assert!(routes.dense.as_ref().unwrap().indices.iter().all(|&id| id == 0));
        assert_eq!(ptr, routes.dense.as_ref().unwrap().indices.as_ptr());
        routes.reserve(&[c(4,4,0)], &Arc::from("B"));
        assert!(!routes.conflicts(c(0,0,0),c(1,0,0),&Arc::from("OTHER"),1.0,0.1));
        assert!(!routes.conflicts(c(-1,0,0),c(-1,0,0),&Arc::from("OTHER"),1.0,0.1));
        assert!(routes.conflicts(c(4,4,0),c(4,4,0),&Arc::from("OTHER"),1.0,0.1));
    }

    #[test]
    fn unit_edge_fast_path_matches_segments_at_clearance_boundaries() {
        for path in [vec![c(0,0,0),c(1,0,0)], vec![c(0,0,0),c(0,1,0)],
            vec![c(0,0,0),c(1,0,0),c(1,1,0),c(1,1,1)]] {
            let mut routes = TemporaryRoutes::new(bounds(), 0.25, 2);
            let mut legacy = LegacyTemporaryRoutes::default();
            routes.reserve(&path, &Arc::from("A"));
            legacy.reserve(&path, &Arc::from("A"));
            assert!(!routes.needs_segment_checks);
            for x in -4..=4 { for y in -4..=4 { for layer in 0..2 {
                let a = c(x,y,layer);
                for b in [a,c(x+1,y,layer),c(x-1,y,layer),c(x,y+1,layer),c(x,y-1,layer),c(x,y,1-layer),c(x+3,y+2,layer)] {
                    for spacing in [0.0,0.1,0.25-1e-10,0.25,0.25+1e-10,0.381,0.5,0.75] {
                        for net in [Arc::from("A"),Arc::from("B")] {
                            assert_eq!(routes.conflicts(a,b,&net,0.25,spacing), legacy.conflicts(a,b,&net,0.25,spacing),
                                "a={a:?}, b={b:?}, spacing={spacing}");
                        }
                    }
                }
            } } }
        }
    }

    #[test]
    fn overlapping_nets_and_long_segments_keep_general_predicate() {
        let mut routes = TemporaryRoutes::default();
        let mut legacy = LegacyTemporaryRoutes::default();
        for (path, net) in [(vec![c(1,0,0)], "B"), (vec![c(0,0,0),c(1,0,0)], "A")] {
            routes.reserve(&path, &Arc::from(net));
            legacy.reserve(&path, &Arc::from(net));
        }
        assert!(routes.needs_segment_checks);
        assert_eq!(routes.conflicts(c(1,0,0),c(1,1,0),&Arc::from("B"),0.25,0.1),
            legacy.conflicts(c(1,0,0),c(1,1,0),&Arc::from("B"),0.25,0.1));
        routes.clear();
        assert!(!routes.needs_segment_checks);
        routes.reserve(&[c(-2,0,0),c(2,0,0)], &Arc::from("A"));
        assert!(routes.needs_segment_checks);
        assert!(routes.conflicts(c(0,-1,0),c(0,1,0),&Arc::from("B"),1.0,0.1));
    }

    #[test]
    fn oversized_or_invalid_dense_domains_use_sparse_fallback() {
        for mut routes in [TemporaryRoutes::with_cell_limit(bounds(),1.0,2,1),
            TemporaryRoutes::new(bounds(),0.0,2), TemporaryRoutes::new(bounds(),1.0,0),
            TemporaryRoutes::new(bounds(),1e-30,usize::MAX)] {
            assert!(routes.dense.is_none());
            routes.reserve(&[c(-1,0,0),c(0,0,0)], &Arc::from("A"));
            assert!(routes.conflicts(c(0,0,0),c(0,0,0),&Arc::from("B"),1.0,0.1));
            routes.clear();
            assert!(!routes.conflicts(c(0,0,0),c(0,0,0),&Arc::from("B"),1.0,0.1));
        }
    }
}
