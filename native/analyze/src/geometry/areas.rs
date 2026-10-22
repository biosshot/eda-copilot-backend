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
