//! Canonical copper normalisation (noise tier), applied once to the cropped copper set before the
//! board arrangement. Every shape (shell plus holes) is renoded by an i_overlay NonZero
//! simplification and pinched shells are split into valid polygons (`planar_trace::split_self_touching`).
//! Downstream stages (arrangement, ownership, LOCAL_COPPER_INPUT) read the canonical set and compare
//! against its hole and part counts, never against the raw polygons. A shape whose area changes by more
//! than 1e-6 relative is refused with its id and bounding box.
use super::{error, planar_trace::split_self_touching, Result};
use geo::{algorithm::Validation, Area, BoundingRect, MultiPolygon, Polygon};

pub(super) const AREA_TOLERANCE: f64 = 1e-6;

#[derive(Default, Clone, Debug)]
pub(super) struct Stats {
    pub shapes: usize,
    pub shapes_changed: usize,
    pub max_area_relative: f64,
    pub holes_before: usize,
    pub holes_after: usize,
    pub parts_before: usize,
    pub parts_after: usize,
    pub holes_merged: usize,
    pub parts_split: usize,
    /// Shapes whose bbox extent is at most this many mm (4h) and that cannot be normalised are dropped as noise; 0 disables.
    pub noise_extent_mm: f64,
    pub parts_dropped: usize,
    pub dropped_area: f64,
}
impl Stats {
    pub fn merge(&mut self, o: &Stats) {
        self.shapes += o.shapes; self.shapes_changed += o.shapes_changed; self.max_area_relative = self.max_area_relative.max(o.max_area_relative);
        self.holes_before += o.holes_before; self.holes_after += o.holes_after; self.parts_before += o.parts_before; self.parts_after += o.parts_after;
        self.holes_merged += o.holes_merged; self.parts_split += o.parts_split; self.parts_dropped += o.parts_dropped; self.dropped_area += o.dropped_area;
    }
    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "tier": "noise", "areaUnit": "relative", "areaToleranceRelative": AREA_TOLERANCE,
            "shapes": self.shapes, "shapesChanged": self.shapes_changed, "maxAreaRelative": self.max_area_relative,
            "holesMerged": self.holes_merged, "partsSplit": self.parts_split,
            "holesBefore": self.holes_before, "holesAfter": self.holes_after,
            "partsBefore": self.parts_before, "partsAfter": self.parts_after,
            "partsDropped": self.parts_dropped, "droppedArea": self.dropped_area, "noiseExtentMm": self.noise_extent_mm,
        })
    }
}
fn bbox(p: &Polygon<f64>) -> String {
    p.bounding_rect().map(|b| format!("bbox [{:.6},{:.6}]..[{:.6},{:.6}]", b.min().x, b.min().y, b.max().x, b.max().y)).unwrap_or_default()
}
/// Normalises one polygon. Valid polygons are canonical already and returned unchanged; an invalid one is renoded
/// (i_overlay NonZero shell, merged holes) and split at pinches by `split_self_touching`.
pub(super) fn normalize_polygon(poly: &Polygon<f64>, label: &str, stats: &mut Stats) -> Result<Vec<Polygon<f64>>> {
    let holes = poly.interiors().len();
    stats.shapes += 1;
    stats.holes_before += holes;
    stats.parts_before += 1;
    let unchanged = |stats: &mut Stats| -> Result<Vec<Polygon<f64>>> { stats.holes_after += holes; stats.parts_after += 1; Ok(vec![poly.clone()]) };
    let before = poly.unsigned_area();
    let parts = if poly.is_valid() { return unchanged(stats); } else {
        match split_self_touching(poly) {
            Ok(p) => p,
            Err(e) => {
                // A feature no larger than 4h cannot be resolved by the mesh: drop it as noise (bounded per set in `normalize_set`).
                let extent = poly.bounding_rect().map(|b| (b.max().x - b.min().x).max(b.max().y - b.min().y)).unwrap_or(f64::INFINITY);
                if stats.noise_extent_mm > 0. && extent <= stats.noise_extent_mm {
                    eprintln!("P7_COPPER_NOISE_DROP shape={label} {} extent {extent:e} mm <= {:e} mm area {before:e}: {e}", bbox(poly), stats.noise_extent_mm);
                    stats.shapes_changed += 1;
                    stats.parts_dropped += 1;
                    stats.dropped_area += before;
                    stats.holes_merged += holes;
                    return Ok(vec![]);
                }
                return Err(error("COPPER_NORMALIZATION", &format!("shape {label} {}: {e}", bbox(poly))));
            }
        }
    };
    let after: f64 = parts.iter().map(|p| p.unsigned_area()).sum();
    let relative = if before > 0. { (after - before).abs() / before } else if after == 0. { 0. } else { f64::INFINITY };
    if !(relative <= AREA_TOLERANCE) {
        return Err(error("COPPER_NORMALIZATION", &format!("shape {label} {}: area change {relative:e} relative exceeds {AREA_TOLERANCE:e} ({before:e} -> {after:e})", bbox(poly))));
    }
    let holes_after: usize = parts.iter().map(|p| p.interiors().len()).sum();
    stats.shapes_changed += 1;
    stats.max_area_relative = stats.max_area_relative.max(relative);
    stats.holes_after += holes_after;
    stats.parts_after += parts.len();
    stats.holes_merged += holes.saturating_sub(holes_after);
    stats.parts_split += parts.len().saturating_sub(1);
    Ok(parts)
}
pub(super) fn normalize_set(set: &MultiPolygon<f64>, label: &str, stats: &mut Stats) -> Result<MultiPolygon<f64>> {
    let mut out = Vec::with_capacity(set.0.len());
    let dropped_before = stats.dropped_area;
    for (i, p) in set.0.iter().enumerate() {
        out.extend(normalize_polygon(p, &format!("{label}.{i}"), stats)?);
    }
    let dropped = stats.dropped_area - dropped_before;
    if dropped > 0. {
        let total = set.unsigned_area();
        if dropped > AREA_TOLERANCE * total {
            return Err(error("COPPER_NORMALIZATION", &format!("set {label}: dropped noise features total {dropped:e} exceed {AREA_TOLERANCE:e} relative of {total:e}")));
        }
    }
    Ok(MultiPolygon(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::LineString;
    fn sq(x: f64, y: f64, s: f64) -> LineString<f64> { LineString::from(vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s], [x, y]]) }
    #[test]
    fn valid_shapes_pass_unchanged() {
        let m = MultiPolygon(vec![Polygon::new(sq(0., 0., 10.), vec![sq(2., 2., 2.)])]);
        let mut st = Stats::default();
        let n = normalize_set(&m, "t", &mut st).unwrap();
        assert_eq!(n, m);
        assert_eq!((st.shapes, st.shapes_changed, st.holes_merged, st.parts_split), (1, 0, 0, 0));
    }
    #[test]
    fn touching_holes_merge_and_area_is_kept() {
        // Two holes share an edge portion: a hole-in-hole contact makes the polygon invalid.
        let m = MultiPolygon(vec![Polygon::new(sq(0., 0., 10.), vec![sq(2., 2., 2.), sq(4., 2., 2.)])]);
        let mut st = Stats::default();
        let n = normalize_set(&m, "t", &mut st).unwrap();
        assert!(n.0.iter().all(|p| p.is_valid()));
        assert!((n.unsigned_area() - (100. - 8.)).abs() < 1e-9, "{}", n.unsigned_area());
        assert_eq!(st.shapes_changed, 1);
        assert_eq!(st.holes_merged, 1);
        assert!(st.max_area_relative <= AREA_TOLERANCE);
    }
    #[test]
    fn pinched_shell_splits_into_valid_parts() {
        let m = MultiPolygon(vec![Polygon::new(LineString::from(vec![[0., 0.], [2., 0.], [1., 1.], [2., 2.], [0., 2.], [1., 1.], [0., 0.]]), vec![])]);
        let mut st = Stats::default();
        let n = normalize_set(&m, "t", &mut st).unwrap();
        assert!(n.0.iter().all(|p| p.is_valid()));
        assert!(n.0.len() >= 2);
        assert_eq!(st.parts_split, n.0.len() - 1);
    }
}
