//! Projected planar contour arrangement, without moving coefficient geometry.
use geo::{
    algorithm::line_intersection::{line_intersection, LineIntersection},
    Line,
};
use rstar::{RTree, RTreeObject, AABB};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::BTreeSet;
type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;
#[derive(Clone)]
struct Segment {
    a: [f64; 2],
    b: [f64; 2],
    index: usize,
}
impl RTreeObject for Segment {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.a[0].min(self.b[0]), self.a[1].min(self.b[1])],
            [self.a[0].max(self.b[0]), self.a[1].max(self.b[1])],
        )
    }
}
fn order(a: &[f64; 2], b: &[f64; 2]) -> std::cmp::Ordering {
    a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1]))
}
pub struct Arrangement {
    pub cdt: Cdt,
    pub max_deviation_mm: f64,
}
pub fn arrange(
    lines: Vec<([f64; 2], [f64; 2])>,
    edge: f64,
    cap: usize,
    check: impl Fn() -> Result<(), String>,
) -> Result<Arrangement, String> {
    if lines.len() > cap * 8 {
        return Err("THERMAL_BUDGET: Projected contour input budget".into());
    }
    let mut canonical: Vec<_> = lines
        .into_iter()
        .filter(|(a, b)| a != b)
        .map(|(a, b)| {
            if order(&a, &b).is_gt() {
                (b, a)
            } else {
                (a, b)
            }
        })
        .collect();
    canonical.sort_by(|a, b| order(&a.0, &b.0).then(order(&a.1, &b.1)));
    canonical.dedup();
    let segments: Vec<_> = canonical
        .into_iter()
        .enumerate()
        .map(|(index, (a, b))| Segment { a, b, index })
        .collect();
    let tree = RTree::bulk_load(segments.clone());
    let mut splits: Vec<Vec<[f64; 2]>> = segments.iter().map(|s| vec![s.a, s.b]).collect();
    let mut pairs = 0usize;
    let mut split_count = segments.len() * 2;
    for s in &segments {
        check()?;
        for t in tree
            .locate_in_envelope_intersecting(&s.envelope())
            .filter(|t| t.index > s.index)
        {
            pairs += 1;
            if pairs > 2_000_000 {
                return Err("THERMAL_BUDGET: Projected contour intersection budget".into());
            }
            let mut add = |p: geo::Coord<f64>| -> Result<(), String> {
                split_count += 2;
                if split_count > cap * 16 {
                    return Err("THERMAL_BUDGET: Projected contour split-point budget".into());
                }
                let p = [p.x, p.y];
                splits[s.index].push(p);
                splits[t.index].push(p);
                Ok(())
            };
            match line_intersection(Line::new(s.a, s.b), Line::new(t.a, t.b)) {
                Some(LineIntersection::SinglePoint { intersection, .. }) => add(intersection)?,
                Some(LineIntersection::Collinear { intersection }) => {
                    add(intersection.start)?;
                    add(intersection.end)?
                }
                None => {}
            }
        }
    }
    let mut edges = vec![];
    let mut max_deviation_mm = 0.0f64;
    for (s, points) in segments.iter().zip(splits.iter_mut()) {
        check()?;
        let d = [s.b[0] - s.a[0], s.b[1] - s.a[1]];
        let parameter = |p: &[f64; 2]| (p[0] - s.a[0]) * d[0] + (p[1] - s.a[1]) * d[1];
        for p in points.iter() {
            let t = (parameter(p) / (d[0] * d[0] + d[1] * d[1])).clamp(0., 1.);
            max_deviation_mm =
                max_deviation_mm.max((p[0] - s.a[0] - t * d[0]).hypot(p[1] - s.a[1] - t * d[1]));
        }
        if !max_deviation_mm.is_finite() || max_deviation_mm > 1e-9 {
            return Err("THERMAL_BOARD: Ambiguous projected contour intersection accuracy".into());
        }
        points.sort_by(|a, b| parameter(a).total_cmp(&parameter(b)).then(order(a, b)));
        points.dedup();
        for pair in points.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let count = ((b[0] - a[0]).hypot(b[1] - a[1]) / edge).ceil().max(1.) as usize;
            if count > cap || edges.len() + count > cap * 8 {
                return Err("THERMAL_BUDGET: Projected contour subdivision budget".into());
            }
            let mut from = a;
            for i in 1..=count {
                let t = i as f64 / count as f64;
                let to = if i == count {
                    b
                } else {
                    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
                };
                if from != to {
                    edges.push((from, to));
                }
                from = to;
            }
        }
    }
    let mut points: Vec<_> = edges.iter().flat_map(|(a, b)| [*a, *b]).collect();
    points.sort_by(order);
    points.dedup();
    if points.len() > cap {
        return Err("THERMAL_BOARD: Conforming copper mesh exceeds node budget".into());
    }
    let mut cdt = Cdt::new();
    let handles = points
        .iter()
        .map(|p| {
            cdt.insert(Point2::new(p[0], p[1]))
                .map_err(|e| format!("THERMAL_BOARD: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut unique = BTreeSet::new();
    for (a, b) in edges {
        check()?;
        let a = points.binary_search_by(|p| order(p, &a)).unwrap();
        let b = points.binary_search_by(|p| order(p, &b)).unwrap();
        let pair = (a.min(b), a.max(b));
        if !unique.insert(pair) {
            continue;
        }
        if cdt.try_add_constraint(handles[a], handles[b]).is_empty() {
            return Err(format!(
                "THERMAL_BOARD: Ambiguous projected contour crossing {:?} {:?}",
                points[a], points[b]
            ));
        }
    }
    Ok(Arrangement {
        cdt,
        max_deviation_mm,
    })
}
