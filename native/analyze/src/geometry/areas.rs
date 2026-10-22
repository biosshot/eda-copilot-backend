//! Validated F64 polygons. Boolean operations use geo/i_overlay's bounded local
//! integer adapter; predicates use geo's robust F64 DE-9IM implementation.
use crate::model;
use geo::{
    Area, BooleanOps, BoundingRect, Coord, LineString, MultiPolygon, Orient, Polygon, Validation,
};
pub type AreaSet = MultiPolygon<f64>;
pub type Poly = Polygon<f64>;

pub fn ring(points: &[model::Point]) -> Result<LineString<f64>, String> {
    let mut out = Vec::new();
    for p in points {
        if !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > 10_000.0 {
            return Err("Coordinate outside finite +/-10000 mm range".into());
        }
        let c = Coord { x: p.x, y: p.y };
        if out.last() != Some(&c) {
            out.push(c);
        }
    }
    if out.first() == out.last() {
        out.pop();
    }
    if out.len() < 3 {
        return Err("Ring has fewer than three distinct consecutive vertices".into());
    }
    out.push(out[0]);
    Ok(LineString(out))
}
pub fn polygon(p: &model::Polygon) -> Result<Poly, String> {
    let p = Polygon::new(
        ring(&p.outer)?,
        p.holes.iter().map(|r| ring(r)).collect::<Result<_, _>>()?,
    );
    p.check_validation()
        .map_err(|e| format!("Invalid polygon topology: {e}"))?;
    if p.unsigned_area() == 0.0 {
        return Err("Zero-area polygon".into());
    }
    Ok(p)
}
pub fn one(p: Poly) -> AreaSet {
    MultiPolygon(vec![p])
}
pub fn union(items: impl IntoIterator<Item = Poly>) -> AreaSet {
    let items: Vec<_> = items
        .into_iter()
        .map(|p| p.orient(geo::orient::Direction::Default))
        .collect();
    geo::algorithm::bool_ops::unary_union(&items)
}
pub fn subtract(a: &AreaSet, b: &AreaSet) -> AreaSet {
    if b.0.is_empty() {
        a.clone()
    } else {
        a.difference(b)
    }
}
pub fn sorted(mut polys: Vec<Poly>) -> Vec<Poly> {
    polys.sort_by(|a, b| {
        let a = a.bounding_rect().unwrap();
        let b = b.bounding_rect().unwrap();
        a.min()
            .x
            .total_cmp(&b.min().x)
            .then(a.min().y.total_cmp(&b.min().y))
            .then(a.max().x.total_cmp(&b.max().x))
            .then(a.max().y.total_cmp(&b.max().y))
    });
    polys
}
pub fn snapshot(p: &Poly) -> super::PolygonSnapshot {
    let points = |r: &LineString<f64>| r.0[..r.0.len() - 1].iter().map(|p| [p.x, p.y]).collect();
    super::PolygonSnapshot {
        outer: points(p.exterior()),
        holes: p.interiors().iter().map(points).collect(),
    }
}
pub fn perimeter(p: &Poly) -> f64 {
    std::iter::once(p.exterior())
        .chain(p.interiors())
        .flat_map(|r| r.lines())
        .map(|l| (l.end.x - l.start.x).hypot(l.end.y - l.start.y))
        .sum()
}

/// Decompose an overlay's weakly-simple walk at exact vertex/edge touches.
/// Coordinates and covered area are unchanged. Proper crossings, displaced
/// vertices and invalid resulting polygons are rejected, never repaired.
pub fn separate_touches(poly: &Poly) -> Result<Vec<Poly>, String> {
    use geo::{Contains, Intersects, Point};
    use std::collections::BTreeMap;
    let mut cycles = vec![];
    for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
        let vertices = &ring.0[..ring.0.len() - 1];
        let mut walk = vec![];
        for edge in ring.lines() {
            let dx = edge.end.x - edge.start.x;
            let dy = edge.end.y - edge.start.y;
            let mut points: Vec<_> = vertices
                .iter()
                .copied()
                .filter(|p| edge.intersects(&Point(*p)))
                .collect();
            points.sort_by(|a, b| {
                ((a.x - edge.start.x) * dx + (a.y - edge.start.y) * dy)
                    .total_cmp(&((b.x - edge.start.x) * dx + (b.y - edge.start.y) * dy))
            });
            points.dedup();
            walk.extend(points.into_iter().filter(|p| *p != edge.end));
        }
        if walk.is_empty() {
            return Err("Empty boundary walk".into());
        }
        walk.push(walk[0]);
        let key = |p: Coord<f64>| {
            (
                if p.x == 0. { 0 } else { p.x.to_bits() },
                if p.y == 0. { 0 } else { p.y.to_bits() },
            )
        };
        let mut positions = BTreeMap::new();
        let mut stack = vec![];
        for p in walk {
            if let Some(&i) = positions.get(&key(p)) {
                let mut cycle = stack[i..].to_vec();
                cycle.push(p);
                if cycle.len() < 4 {
                    return Err("Degenerate touching-boundary cycle".into());
                }
                cycles.push(LineString(cycle));
                for old in stack.drain(i + 1..) {
                    positions.remove(&key(old));
                }
            } else {
                positions.insert(key(p), stack.len());
                stack.push(p);
            }
        }
        if stack.len() != 1 {
            return Err("Unclosed boundary walk".into());
        }
    }
    let signed = |r: &LineString<f64>| {
        r.lines()
            .map(|l| l.start.x * l.end.y - l.end.x * l.start.y)
            .sum::<f64>()
            / 2.
    };
    let outer_area = signed(poly.exterior());
    if outer_area == 0. || cycles.iter().any(|c| signed(c) == 0.) {
        return Err("Zero-area crossing or cycle".into());
    }
    let outer_sign = outer_area.signum();
    let mut result: Vec<_> = cycles
        .iter()
        .filter(|c| signed(c) * outer_sign > 0.)
        .map(|r| Poly::new(r.clone(), vec![]))
        .collect();
    for hole in cycles.into_iter().filter(|c| signed(c) * outer_sign < 0.) {
        let owner = result
            .iter()
            .position(|p| hole.0.iter().any(|v| p.contains(&Point(*v))))
            .ok_or("Hole has no containing exterior")?;
        result[owner].interiors_push(hole);
    }
    if result.iter().any(|p| !p.is_valid()) {
        return Err("Touch decomposition remains invalid".into());
    }
    if (result.iter().map(|p| p.unsigned_area()).sum::<f64>() - poly.unsigned_area()).abs()
        > 1e-10 * (1. + poly.unsigned_area())
    {
        return Err("Touch decomposition changes area".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_touch_decomposition_preserves_holes_and_point_separation() {
        let p = Poly::new(
            LineString::from(vec![
                (0., 0.),
                (4., 0.),
                (4., 4.),
                (2., 4.),
                (3., 3.),
                (1., 3.),
                (2., 4.),
                (0., 4.),
                (0., 0.),
            ]),
            vec![],
        );
        let out = separate_touches(&p).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].interiors().len(), 1);
        assert!(out[0].is_valid());
        assert!((out[0].unsigned_area() - 15.).abs() < 1e-12);
        let p = Poly::new(
            LineString::from(vec![
                (0., 0.),
                (1., 0.),
                (1., 1.),
                (2., 1.),
                (2., 2.),
                (1., 2.),
                (1., 1.),
                (0., 1.),
                (0., 0.),
            ]),
            vec![],
        );
        let out = separate_touches(&p).unwrap();
        assert_eq!(out.len(), 2);
        assert!((out.iter().map(Area::unsigned_area).sum::<f64>() - 2.).abs() < 1e-12);
        let cross = Poly::new(
            LineString::from(vec![(0., 0.), (2., 2.), (0., 2.), (2., 0.), (0., 0.)]),
            vec![],
        );
        assert!(separate_touches(&cross).is_err());
    }
}
