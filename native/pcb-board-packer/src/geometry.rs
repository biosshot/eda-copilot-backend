use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::cell::RefCell;

const GEOMETRY_EPSILON: f64 = 1e-6;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Box2 {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// Exact distance memoization for one immutable outline, owned by a solve context.
/// No coordinate rounding, global state, or per-query polygon hashing.
pub(crate) struct PolygonDistanceCache {
    polygon: Box<[Point]>,
    max_entries: usize,
    finite_polygon: bool,
    data: RefCell<PolygonDistanceCacheData>,
}

#[derive(Default)]
struct PolygonDistanceCacheData {
    values: FxHashMap<(u64, u64), f64>,
    #[cfg(test)]
    hits: u64,
    #[cfg(test)]
    misses: u64,
}

impl PolygonDistanceCache {
    pub(crate) fn new(polygon: &[Point], max_entries: usize) -> Self {
        Self {
            polygon: polygon.to_vec().into_boxed_slice(),
            max_entries,
            finite_polygon: polygon.iter().all(is_finite_point),
            data: RefCell::new(PolygonDistanceCacheData::default()),
        }
    }

    pub(crate) fn polygon(&self) -> &[Point] {
        &self.polygon
    }

    pub(crate) fn distance(&self, point: &Point) -> f64 {
        let cacheable = self.finite_polygon && is_finite_point(point);
        let key = (point.x.to_bits(), point.y.to_bits());
        let mut data = self.data.borrow_mut();
        if cacheable {
            if let Some(value) = data.values.get(&key).copied() {
                #[cfg(test)]
                { data.hits += 1; }
                return value;
            }
        }
        #[cfg(test)]
        { data.misses += 1; }

        // Keep the existing numerical and degenerate-edge behavior unchanged.
        let value = point_to_polygon_distance(point, &self.polygon);
        if cacheable && value.is_finite() && data.values.len() < self.max_entries {
            data.values.insert(key, value);
        }
        value
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> (u64, u64, usize) {
        let data = self.data.borrow();
        (data.hits, data.misses, data.values.len())
    }
}

#[inline(always)]
pub fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

#[inline(always)]
pub fn round_placement(value: f64) -> f64 {
    js_round(value * 1000.0) / 1000.0
}

#[inline(always)]
pub fn normalize_rotation(value: i32) -> i32 {
    ((value % 360) + 360) % 360
}

#[inline(always)]
pub fn box_center(box_: &Box2) -> Point {
    Point {
        x: round_placement((box_.left + box_.right) / 2.0),
        y: round_placement((box_.top + box_.bottom) / 2.0),
    }
}

#[inline(always)]
pub fn translate_box(box_: &Box2, dx: f64, dy: f64) -> Box2 {
    Box2 {
        left: round_placement(box_.left + dx),
        right: round_placement(box_.right + dx),
        top: round_placement(box_.top + dy),
        bottom: round_placement(box_.bottom + dy),
    }
}

#[inline(always)]
pub fn rotate_point(point: &Point, origin: &Point, angle: i32) -> Point {
    let radians = f64::from(angle).to_radians();
    let (sin, cos) = radians.sin_cos();
    let dx = point.x - origin.x;
    let dy = point.y - origin.y;
    Point {
        x: round_placement(origin.x + dx * cos - dy * sin),
        y: round_placement(origin.y + dx * sin + dy * cos),
    }
}

#[inline(always)]
pub fn rotate_box(box_: &Box2, origin: &Point, angle: i32) -> Box2 {
    let corners = box_corners(box_);
    let first = rotate_point(&corners[0], origin, angle);
    let mut result = Box2 {
        left: first.x,
        right: first.x,
        top: first.y,
        bottom: first.y,
    };
    for corner in &corners[1..] {
        let point = rotate_point(corner, origin, angle);
        result.left = result.left.min(point.x);
        result.right = result.right.max(point.x);
        result.top = result.top.min(point.y);
        result.bottom = result.bottom.max(point.y);
    }
    result
}

#[inline(always)]
pub fn union_boxes(boxes: &[Box2]) -> Box2 {
    let Some(first) = boxes.first() else {
        return Box2 {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        };
    };
    let mut result = *first;
    for box_ in &boxes[1..] {
        result.left = result.left.min(box_.left);
        result.right = result.right.max(box_.right);
        result.top = result.top.min(box_.top);
        result.bottom = result.bottom.max(box_.bottom);
    }
    result
}

#[inline(always)]
pub fn overlap_depth(a: &Box2, b: &Box2, clearance: f64) -> f64 {
    let x1 = a.right + clearance - b.left;
    let x2 = b.right + clearance - a.left;
    if x1 <= 0.0 || x2 <= 0.0 {
        return 0.0;
    }
    let y1 = a.bottom + clearance - b.top;
    let y2 = b.bottom + clearance - a.top;
    if y1 <= 0.0 || y2 <= 0.0 {
        return 0.0;
    }
    x1.min(x2).min(y1.min(y2))
}

#[inline(always)]
pub fn boxes_overlap_depth(a_boxes: &[Box2], b_boxes: &[Box2], clearance: f64) -> f64 {
    let mut max_overlap: f64 = 0.0;
    for a in a_boxes {
        for b in b_boxes {
            max_overlap = max_overlap.max(overlap_depth(a, b, clearance));
        }
    }
    max_overlap
}

#[inline(always)]
pub fn box_outside_bounds_severity(box_: &Box2, bounds: &Box2) -> f64 {
    (bounds.left - box_.left).max(0.0)
        + (box_.right - bounds.right).max(0.0)
        + (bounds.top - box_.top).max(0.0)
        + (box_.bottom - bounds.bottom).max(0.0)
}

#[inline(always)]
pub fn point_in_polygon(point: &Point, polygon: &[Point]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[j];
        if point_on_segment(point, &a, &b) {
            return true;
        }
        let intersects = ((a.y > point.y) != (b.y > point.y))
            && point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y) + a.x;
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[inline(always)]
pub fn point_to_polygon_distance(point: &Point, polygon: &[Point]) -> f64 {
    if polygon.is_empty() {
        return f64::INFINITY;
    }
    let mut min = f64::INFINITY;
    for index in 0..polygon.len() {
        min = min.min(point_to_segment_distance(
            point,
            &polygon[index],
            &polygon[(index + 1) % polygon.len()],
        ));
    }
    min
}

#[inline(always)]
pub fn box_inside_polygon_board(
    box_: &Box2,
    bounds: &Box2,
    polygon: &[Point],
    edge_clearance: f64,
) -> bool {
    let corners = box_corners(box_);
    for corner in &corners {
        if corner.x < bounds.left + edge_clearance - GEOMETRY_EPSILON
            || corner.x > bounds.right - edge_clearance + GEOMETRY_EPSILON
            || corner.y < bounds.top + edge_clearance - GEOMETRY_EPSILON
            || corner.y > bounds.bottom - edge_clearance + GEOMETRY_EPSILON
            || !point_in_polygon(corner, polygon)
            || (edge_clearance > 0.0
                && point_to_polygon_distance(corner, polygon) + GEOMETRY_EPSILON < edge_clearance)
        {
            return false;
        }
    }
    for index in 0..corners.len() {
        let a = corners[index];
        let b = corners[(index + 1) % corners.len()];
        for outline_index in 0..polygon.len() {
            if segment_intersection(
                &a,
                &b,
                &polygon[outline_index],
                &polygon[(outline_index + 1) % polygon.len()],
            )
            .is_some()
            {
                return false;
            }
        }
    }
    true
}

#[inline(always)]
pub fn is_finite_point(point: &Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

#[inline(always)]
pub fn is_finite_box(box_: &Box2) -> bool {
    box_.left.is_finite()
        && box_.right.is_finite()
        && box_.top.is_finite()
        && box_.bottom.is_finite()
}

#[inline(always)]
pub fn box_corners(box_: &Box2) -> [Point; 4] {
    [
        Point {
            x: box_.left,
            y: box_.top,
        },
        Point {
            x: box_.right,
            y: box_.top,
        },
        Point {
            x: box_.right,
            y: box_.bottom,
        },
        Point {
            x: box_.left,
            y: box_.bottom,
        },
    ]
}

#[inline(always)]
fn point_on_segment(point: &Point, a: &Point, b: &Point) -> bool {
    ((b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x)).abs() < GEOMETRY_EPSILON
        && point.x <= a.x.max(b.x) + GEOMETRY_EPSILON
        && point.x >= a.x.min(b.x) - GEOMETRY_EPSILON
        && point.y <= a.y.max(b.y) + GEOMETRY_EPSILON
        && point.y >= a.y.min(b.y) - GEOMETRY_EPSILON
}

#[inline(always)]
fn point_to_segment_distance(point: &Point, a: &Point, b: &Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared < GEOMETRY_EPSILON {
        return ((point.x - a.x).powi(2) + (point.y - a.y).powi(2)).sqrt();
    }
    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    let projection_x = a.x + t * dx;
    let projection_y = a.y + t * dy;
    ((point.x - projection_x).powi(2) + (point.y - projection_y).powi(2)).sqrt()
}

#[inline(always)]
fn segment_intersection(a: &Point, b: &Point, c: &Point, d: &Point) -> Option<Point> {
    let denominator = (a.x - b.x) * (c.y - d.y) - (a.y - b.y) * (c.x - d.x);
    if denominator.abs() < GEOMETRY_EPSILON {
        return None;
    }
    let t = ((a.x - c.x) * (c.y - d.y) - (a.y - c.y) * (c.x - d.x)) / denominator;
    let u = -((a.x - b.x) * (a.y - c.y) - (a.y - b.y) * (a.x - c.x)) / denominator;
    if !(-GEOMETRY_EPSILON..=1.0 + GEOMETRY_EPSILON).contains(&t)
        || !(-GEOMETRY_EPSILON..=1.0 + GEOMETRY_EPSILON).contains(&u)
    {
        return None;
    }
    Some(Point {
        x: a.x + t * (b.x - a.x),
        y: a.y + t * (b.y - a.y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_javascript_round_for_negative_halves() {
        assert_eq!(js_round(-1.5), -1.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(1.5), 2.0);
    }

    #[test]
    fn rotates_boxes_around_arbitrary_center() {
        let box_ = Box2 {
            left: 1.0,
            right: 5.0,
            top: 2.0,
            bottom: 4.0,
        };
        let rotated = rotate_box(&box_, &Point { x: 3.0, y: 3.0 }, 90);
        assert_eq!(
            rotated,
            Box2 {
                left: 2.0,
                right: 4.0,
                top: 1.0,
                bottom: 5.0
            }
        );
    }

    #[test]
    fn computes_clearance_overlap_depth() {
        let a = Box2 {
            left: 0.0,
            right: 2.0,
            top: 0.0,
            bottom: 2.0,
        };
        let b = Box2 {
            left: 2.5,
            right: 4.0,
            top: 0.0,
            bottom: 2.0,
        };
        assert_eq!(overlap_depth(&a, &b, 0.5), 0.0);
        assert_eq!(overlap_depth(&a, &b, 0.75), 0.25);
    }

    #[test]
    fn rejects_box_in_polygon_notch() {
        let polygon = vec![
            Point { x: -5.0, y: -5.0 },
            Point { x: 5.0, y: -5.0 },
            Point { x: 5.0, y: 5.0 },
            Point { x: 0.0, y: 5.0 },
            Point { x: 0.0, y: 0.0 },
            Point { x: -5.0, y: 0.0 },
        ];
        let bounds = Box2 {
            left: -5.0,
            right: 5.0,
            top: -5.0,
            bottom: 5.0,
        };
        let box_ = Box2 {
            left: -2.0,
            right: -1.0,
            top: 1.0,
            bottom: 2.0,
        };
        assert!(!box_inside_polygon_board(&box_, &bounds, &polygon, 0.0));
    }
}

#[cfg(test)]
mod polygon_distance_cache_tests {
    use super::*;

    fn square(size: f64) -> Vec<Point> {
        vec![Point { x: 0.0, y: 0.0 }, Point { x: size, y: 0.0 },
             Point { x: size, y: size }, Point { x: 0.0, y: size }]
    }

    fn assert_same(cache: &PolygonDistanceCache, point: Point) {
        let expected = point_to_polygon_distance(&point, cache.polygon());
        let actual = cache.distance(&point);
        if expected.is_nan() { assert!(actual.is_nan()); }
        else { assert_eq!(actual.to_bits(), expected.to_bits()); }
    }

    #[test]
    fn reuses_exact_points_without_rounding() {
        let cache = PolygonDistanceCache::new(&square(10.0), 100);
        let point = Point { x: 2.0, y: 3.0 };
        assert_same(&cache, point);
        assert_same(&cache, point);
        assert_eq!(cache.stats(), (1, 1, 1));
        assert_same(&cache, Point { x: f64::from_bits(point.x.to_bits() + 1), y: point.y });
        assert_same(&cache, Point { x: 0.0, y: 3.0 });
        assert_same(&cache, Point { x: -0.0, y: 3.0 });
        assert_eq!(cache.stats(), (1, 4, 4));
    }

    #[test]
    fn capacity_and_zero_capacity_keep_exact_results() {
        for limit in [0, 1] {
            let cache = PolygonDistanceCache::new(&square(10.0), limit);
            for point in [Point { x: 2.0, y: 3.0 }, Point { x: 4.0, y: 4.0 }, Point { x: 2.0, y: 3.0 }] {
                assert_same(&cache, point);
            }
            assert_eq!(cache.stats(), if limit == 0 { (0, 3, 0) } else { (1, 2, 1) });
        }
    }

    #[test]
    fn snapshots_and_distinct_outlines_do_not_share_answers() {
        let mut polygon = square(10.0);
        let original = polygon.clone();
        let first = PolygonDistanceCache::new(&polygon, 100);
        let point = Point { x: 4.0, y: 4.0 };
        assert_eq!(first.distance(&point), 4.0);
        polygon = square(5.0);
        let second = PolygonDistanceCache::new(&polygon, 100);
        assert_eq!(second.distance(&point), 1.0);
        assert_eq!(first.distance(&point), 4.0);
        assert_eq!(first.polygon(), original.as_slice());
    }

    #[test]
    fn matches_raw_distance_for_concave_and_degenerate_outlines() {
        let outlines = [square(10.0), vec![
            Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 }, Point { x: 5.0, y: 10.0 },
            Point { x: 5.0, y: 5.0 }, Point { x: 0.0, y: 5.0 },
        ], vec![Point { x: 1.0, y: 1.0 }], vec![
            Point { x: 1.0, y: 1.0 }, Point { x: 1.000_01, y: 1.0 },
        ], vec![]];
        for outline in &outlines {
            let cache = PolygonDistanceCache::new(outline, 10_000);
            for _ in 0..2 {
                for x in -4..=44 {
                    for y in -4..=44 {
                        assert_same(&cache, Point { x: x as f64 * 0.25, y: y as f64 * 0.25 });
                    }
                }
                for delta in [-1e-9, 0.0, 1e-9, -1e-6, 1e-6] {
                    assert_same(&cache, Point { x: 5.0 + delta, y: 5.0 });
                }
            }
        }
    }

    #[test]
    fn nonfinite_inputs_and_results_are_not_memoized() {
        let cache = PolygonDistanceCache::new(&square(10.0), 100);
        for point in [Point { x: f64::NAN, y: 0.0 }, Point { x: f64::INFINITY, y: 0.0 }] {
            assert_same(&cache, point);
            assert_same(&cache, point);
        }
        assert_eq!(cache.stats(), (0, 4, 0));
        let empty = PolygonDistanceCache::new(&[], 100);
        assert_same(&empty, Point { x: 0.0, y: 0.0 });
        assert_eq!(empty.stats(), (0, 1, 0));
        let invalid = PolygonDistanceCache::new(&[Point { x: f64::NAN, y: 0.0 }], 100);
        assert_same(&invalid, Point { x: 0.0, y: 0.0 });
        assert_eq!(invalid.stats(), (0, 1, 0));
    }
}
