//! Bounded direct-addressing support for the micro-router's existing grid.
//! These tables memoize exact tests; they do not rasterize or enlarge keepouts.
use super::{cell_to_point, Cell, MicroRouteConfig, EPS};
use crate::geometry::{is_finite_box, point_in_polygon, point_to_polygon_distance, Box2, Point, PolygonDistanceCache};
use std::cell::Cell as ValueCell;

// At most 4 MiB for the shared contour mask; oversized domains use exact fallback.
const MAX_OUTLINE_CELLS: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) struct GridShape {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) len: usize,
}

impl GridShape {
    pub(super) fn new(bounds: Box2, grid: f64, max_cells: usize) -> Option<Self> {
        if !is_finite_box(&bounds) || !grid.is_finite() || grid <= 0.0 {
            return None;
        }
        let count = |low: f64, high: f64| -> Option<usize> {
            let steps = ((high - low) / grid).ceil();
            if high < low || !steps.is_finite() || steps < 0.0 || steps >= i32::MAX as f64 {
                return None;
            }
            (steps as usize).checked_add(1)
        };
        // Include the rounded-up boundary cell. The caller's original world-space
        // bounds test remains authoritative for cells outside a non-aligned edge.
        let width = count(bounds.left, bounds.right)?;
        let height = count(bounds.top, bounds.bottom)?;
        let len = width.checked_mul(height)?;
        (len <= max_cells).then_some(Self { width, height, len })
    }

    #[inline(always)]
    pub(super) fn index(&self, x: i32, y: i32) -> Option<usize> {
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        if x >= self.width || y >= self.height { return None; }
        Some(y * self.width + x)
    }
}

pub(super) struct OutlineMask {
    fallback: PolygonDistanceCache,
    bounds: Box2,
    grid: f64,
    edge_clearance: f64,
    dense: Option<(GridShape, Vec<ValueCell<u8>>)>,
    #[cfg(test)]
    hits: ValueCell<u64>,
    #[cfg(test)]
    misses: ValueCell<u64>,
    #[cfg(test)]
    entries: ValueCell<usize>,
}

impl OutlineMask {
    pub(super) fn new(polygon: &[Point], bounds: Box2, config: &MicroRouteConfig, fallback_limit: usize) -> Self {
        Self::with_cell_limit(polygon, bounds, config, fallback_limit, MAX_OUTLINE_CELLS)
    }

    pub(super) fn with_cell_limit(polygon: &[Point], bounds: Box2, config: &MicroRouteConfig,
        fallback_limit: usize, max_cells: usize) -> Self {
        let dense = if polygon.is_empty() { None } else {
            GridShape::new(bounds, config.grid, max_cells).and_then(|shape| {
                let mut cells = Vec::new();
                cells.try_reserve_exact(shape.len).ok()?;
                cells.resize_with(shape.len, || ValueCell::new(0));
                Some((shape, cells))
            })
        };
        Self {
            fallback: PolygonDistanceCache::new(polygon, fallback_limit),
            bounds, grid: config.grid,
            edge_clearance: config.clearance + config.trace_width / 2.0,
            dense,
            #[cfg(test)]
            hits: ValueCell::new(0),
            #[cfg(test)]
            misses: ValueCell::new(0),
            #[cfg(test)]
            entries: ValueCell::new(0),
        }
    }

    pub(super) fn polygon(&self) -> &[Point] { self.fallback.polygon() }

    // Checked at batch/search entry, not on every neighbor lookup. Layer does
    // not enter a contour key: the current estimator uses one outline on all layers.
    pub(super) fn matches(&self, bounds: Box2, config: &MicroRouteConfig) -> bool {
        self.bounds.left.to_bits() == bounds.left.to_bits()
            && self.bounds.right.to_bits() == bounds.right.to_bits()
            && self.bounds.top.to_bits() == bounds.top.to_bits()
            && self.bounds.bottom.to_bits() == bounds.bottom.to_bits()
            && self.grid.to_bits() == config.grid.to_bits()
            && self.edge_clearance.to_bits() == (config.clearance + config.trace_width / 2.0).to_bits()
    }

    /// Only the ordinary contour test. Bounds, endpoint carving, pads, nets and
    /// temporary copper stay in blocked() and must never be memoized here.
    pub(super) fn blocked(&self, cell: Cell) -> bool {
        if let Some((shape, cells)) = &self.dense {
            if let Some(index) = shape.index(cell.x, cell.y) {
                let slot = &cells[index];
                let value = slot.get();
                if value != 0 {
                    #[cfg(test)]
                    self.hits.set(self.hits.get() + 1);
                    return value == 2;
                }
                let point = cell_to_point(cell, self.bounds, self.grid);
                let blocked = !point_in_polygon(&point, self.polygon())
                    || point_to_polygon_distance(&point, self.polygon()) + EPS < self.edge_clearance;
                slot.set(if blocked { 2 } else { 1 });
                #[cfg(test)]
                {
                    self.misses.set(self.misses.get() + 1);
                    self.entries.set(self.entries.get() + 1);
                }
                return blocked;
            }
        }
        // Negative/out-of-table coordinates (including EPS-border cases) and
        // oversized grids retain the old exact behavior, never an implicit keepout.
        let point = cell_to_point(cell, self.bounds, self.grid);
        !self.polygon().is_empty() && (!point_in_polygon(&point, self.polygon())
            || self.fallback.distance(&point) + EPS < self.edge_clearance)
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> (u64, u64, usize) {
        (self.hits.get(), self.misses.get(), self.entries.get())
    }

    #[cfg(test)]
    pub(super) fn is_dense(&self) -> bool { self.dense.is_some() }
}
