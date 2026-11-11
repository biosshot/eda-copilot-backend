//! Exact geometry adapter from mixed conductor surface coordinates to layered traces.
use super::{error, Result, mixed_conductor::MixedMap, layered_capacitance::{Input, PlaneInput, PotentialTrace, TraceNode, Budget}};
use geo::{Intersects, LineString, Point, Polygon};
use std::{collections::BTreeMap, time::Instant};
use rstar::{RTree, RTreeObject, AABB};
#[derive(Clone)]
struct FaceIndex { index: usize, bounds: AABB<[f64;3]> }
impl RTreeObject for FaceIndex { type Envelope=AABB<[f64;3]>; fn envelope(&self)->Self::Envelope { self.bounds } }


/// Nodes a closed ring at its own crossings and T-touches (vertex on another edge), so that the
/// shared points are identical vertices and `ring_loops` can split the ring there.
fn node_ring(ring: &LineString<f64>) -> LineString<f64> {
    use rstar::primitives::{GeomWithData, Rectangle};
    let pts: Vec<geo::Coord<f64>> = if ring.0.len() > 1 && ring.0.first() == ring.0.last() { ring.0[..ring.0.len() - 1].to_vec() } else { ring.0.clone() };
    let n = pts.len();
    if n < 4 { return ring.clone(); }
    let seg = |i: usize| (pts[i], pts[(i + 1) % n]);
    let tree = RTree::bulk_load((0..n).map(|i| { let (a, b) = seg(i); GeomWithData::new(Rectangle::from_corners([a.x, a.y], [b.x, b.y]), i) }).collect::<Vec<_>>());
    let orient = |p: geo::Coord<f64>, q: geo::Coord<f64>, r: geo::Coord<f64>| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let on = |a: geo::Coord<f64>, b: geo::Coord<f64>, c: geo::Coord<f64>| c.x >= a.x.min(b.x) && c.x <= a.x.max(b.x) && c.y >= a.y.min(b.y) && c.y <= a.y.max(b.y);
    let mut cuts: Vec<Vec<(f64, geo::Coord<f64>)>> = vec![vec![]; n];
    let param = |a: geo::Coord<f64>, b: geo::Coord<f64>, p: geo::Coord<f64>| { let (dx, dy) = (b.x - a.x, b.y - a.y); ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy) };
    for i in 0..n {
        let (a, b) = seg(i);
        for g in tree.locate_in_envelope_intersecting(&rstar::AABB::from_corners([a.x.min(b.x), a.y.min(b.y)], [a.x.max(b.x), a.y.max(b.y)])) {
            let j = g.data;
            if j <= i || j == (i + 1) % n || i == (j + 1) % n { continue; }
            let (c, d) = seg(j);
            let (d1, d2, d3, d4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
            if d1 * d2 < 0. && d3 * d4 < 0. {
                let t = d3 / (d3 - d4);
                let p = geo::Coord { x: a.x + t * (b.x - a.x), y: a.y + t * (b.y - a.y) };
                cuts[i].push((param(a, b, p), p));
                cuts[j].push((param(c, d, p), p));
            } else {
                if d1 == 0. && on(a, b, c) { cuts[i].push((param(a, b, c), c)); }
                if d2 == 0. && on(a, b, d) { cuts[i].push((param(a, b, d), d)); }
                if d3 == 0. && on(c, d, a) { cuts[j].push((param(c, d, a), a)); }
                if d4 == 0. && on(c, d, b) { cuts[j].push((param(c, d, b), b)); }
            }
        }
    }
    let mut out = Vec::with_capacity(n + 8);
    for i in 0..n {
        out.push(pts[i]);
        let c = &mut cuts[i];
        c.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (t, p) in c.iter() { if *t > 0. && *t < 1. && out.last() != Some(p) && *p != pts[(i + 1) % n] { out.push(*p); } }
    }
    out.push(out[0]);
    LineString::new(out)
}
/// Splits a closed ring that visits the same vertex more than once into simple loops.
pub(super) fn ring_loops(ring: &LineString<f64>) -> Vec<Vec<geo::Coord<f64>>> {
    let pts: Vec<_> = if ring.0.len() > 1 && ring.0.first() == ring.0.last() { ring.0[..ring.0.len() - 1].to_vec() } else { ring.0.clone() };
    let k = |c: &geo::Coord<f64>| (c.x.to_bits(), c.y.to_bits());
    let (mut stack, mut at, mut loops) = (Vec::<geo::Coord<f64>>::new(), BTreeMap::<(u64, u64), usize>::new(), Vec::new());
    for c in pts {
        if let Some(&pos) = at.get(&k(&c)) {
            let lp: Vec<_> = stack.drain(pos..).collect();
            for q in &lp[1..] { at.remove(&k(q)); }
            stack.push(c);
            if lp.len() >= 3 { loops.push(lp); }
        } else {
            at.insert(k(&c), stack.len());
            stack.push(c);
        }
    }
    if stack.len() >= 3 { loops.push(stack); }
    loops
}
pub(super) fn signed_area(r: &[geo::Coord<f64>]) -> f64 {
    (0..r.len()).map(|i| { let (a, b) = (r[i], r[(i + 1) % r.len()]); a.x * b.y - b.x * a.y }).sum::<f64>() / 2.
}
/// A pour whose ring touches itself at vertices is the area filled under the winding rule:
/// same-orientation loops are shells, opposite-orientation loops are voids. The loops are returned as
/// simple polygons whose areas add up to the original; anything else is refused with coordinates.
pub(super) fn split_self_touching(poly: &Polygon<f64>) -> std::result::Result<Vec<Polygon<f64>>, String> { split_self_touching_depth(poly, 0) }
fn split_self_touching_depth(poly: &Polygon<f64>, depth: usize) -> std::result::Result<Vec<Polygon<f64>>, String> {
    use geo::{Area, Contains, InteriorPoint, algorithm::Validation};
    if poly.is_valid() { return Ok(vec![poly.clone()]); }
    let b = geo::BoundingRect::bounding_rect(poly).map(|b| format!("bbox [{:.6},{:.6}]..[{:.6},{:.6}] mm", b.min().x, b.min().y, b.max().x, b.max().y)).unwrap_or_default();
    let loops = ring_loops(&node_ring(poly.exterior()));
    let outer_sign = signed_area(&poly.exterior().0[..poly.exterior().0.len().saturating_sub(1)]).signum();
    let (mut shells, mut voids) = (Vec::new(), Vec::new());
    for l in loops {
        let a = signed_area(&l);
        if a == 0. { continue; }
        let ring = LineString::new({ let mut v = l; v.push(v[0]); v });
        if a.signum() == outer_sign { shells.push(Polygon::new(ring, vec![])); } else { voids.push(ring); }
    }
    let mut holes: Vec<LineString<f64>> = voids;
    for r in poly.interiors() { for l in ring_loops(&node_ring(r)) { let mut v = l; v.push(v[0]); holes.push(LineString::new(v)); } }
    let mut owned: Vec<Vec<LineString<f64>>> = vec![vec![]; shells.len()];
    for h in holes {
        let hp = Polygon::new(h.clone(), vec![]);
        let Some(pt) = hp.interior_point() else { continue };
        let owner = (0..shells.len()).filter(|&i| shells[i].contains(&pt)).min_by(|&x, &y| shells[x].unsigned_area().total_cmp(&shells[y].unsigned_area()));
        match owner { Some(i) => owned[i].push(h),
            // A void smaller than 1e-6 of the largest shell that touches or leaves the shell is rounding noise: dropped (the area check below bounds the total change).
            None if hp.unsigned_area() <= 1e-6 * shells.iter().map(|x| x.unsigned_area()).fold(0., f64::max) => { eprintln!("P7_COPPER_VOID_DROP loop area {:e} mm2 outside every shell, {b}", hp.unsigned_area()); }
            None => return Err(format!("ring loop outside every shell (loop area {:e}, {} shells with areas {:?}, exterior signed area {:e}, {} original holes), {b}", hp.unsigned_area(), shells.len(), shells.iter().map(|x| x.unsigned_area()).collect::<Vec<_>>(), signed_area(&poly.exterior().0[..poly.exterior().0.len().saturating_sub(1)]), poly.interiors().len())) }
    }
    let out: Vec<Polygon<f64>> = shells.into_iter().zip(owned).map(|(s, h)| Polygon::new(s.exterior().clone(), h)).collect();
    let area: f64 = out.iter().map(|p| p.unsigned_area()).sum();
    let original = poly.unsigned_area();
    if (area - original).abs() > 1e-6 * original { return Err(format!("winding fill area {area:e} differs from ring area {original:e} by more than 1e-6, {b}")); }
    // A polygon that is still invalid (overlapping holes, pinched or crossing exterior after a ROI crop) is
    // normalised once: the shell is renoded by an i_overlay NonZero simplification, the holes are merged by a
    // union, and the polygon is rebuilt as shell minus holes. The area must stay within 1e-6 relative.
    let mut normalised: Vec<Polygon<f64>> = Vec::with_capacity(out.len());
    for p in out {
        if p.is_valid() { normalised.push(p); continue; }
        use i_overlay::{core::fill_rule::FillRule, float::simplify::SimplifyShape};
        use geo::Orient;
        let shell_ring: Vec<[f64; 2]> = p.exterior().0[..p.exterior().0.len() - 1].iter().map(|c| [c.x, c.y]).collect();
        let shells: Vec<Polygon<f64>> = vec![shell_ring].simplify_shape(FillRule::NonZero).into_iter().filter_map(|shape| {
            let mut contours = shape.into_iter().map(|c| { let mut v: Vec<geo::Coord<f64>> = c.into_iter().map(|q| geo::Coord { x: q[0], y: q[1] }).collect(); if let Some(f) = v.first().copied() { v.push(f); } LineString::new(v) });
            contours.next().map(|ext| Polygon::new(ext, vec![]))
        }).collect();
        let holes = crate::geometry::areas::union(p.interiors().iter().map(|r| Polygon::new(r.clone(), vec![]).orient(geo::orient::Direction::Default)));
        let rebuilt = crate::geometry::areas::difference(&geo::MultiPolygon(shells), &holes);
        let expected = Polygon::new(p.exterior().clone(), vec![]).unsigned_area() - crate::geometry::areas::intersection(&crate::geometry::areas::one(Polygon::new(p.exterior().clone(), vec![])), &holes).unsigned_area();
        let got: f64 = rebuilt.0.iter().map(|q| q.unsigned_area()).sum();
        if (got - expected).abs() > 1e-6 * expected.abs() {
            return Err(format!("normalised area {got:e} differs from {expected:e} by more than 1e-6, {b}"));
        }
        // The overlay merges a hole touching the shell into a pinched shell ring: split those pinches again.
        let mut dropped = 0.;
        for q in rebuilt.0 {
            if q.is_valid() || depth >= 2 { normalised.push(q); continue; }
            // An invalid micro-piece of the overlay output (below 1e-6 of the polygon) is rounding noise: dropped, the total is bounded below.
            let qa = q.unsigned_area();
            if qa <= 1e-6 * expected.abs() { dropped += qa; eprintln!("P7_COPPER_PIECE_DROP area {qa:e} mm2 of {expected:e}, {b}"); continue; }
            normalised.extend(split_self_touching_depth(&q, depth + 1)?);
        }
        if dropped > 1e-6 * expected.abs() { return Err(format!("dropped overlay pieces total {dropped:e} exceed 1e-6 of {expected:e}, {b}")); }
    }
    let out = normalised;
    if let Some(bad) = out.iter().find(|p| !p.is_valid()) { return Err(format!("loop still invalid after splitting: {:?}, {b}", bad.check_validation().err())); }
    Ok(out)
}

/// Canonical copper polygons of one slab, as (millimetre polygon, SI polygon relative to `origin_mm`).
/// The slab union is already canonical (see copper_normalize); `split_self_touching` stays as a guard
/// that returns it unchanged when valid. A polygon the mm to SI scaling makes invalid is normalised
/// in SI and its parts are converted back, so the two lists stay one-to-one and every consumer sees
/// valid input whose hole and part counts are the canonical ones.
fn canonical_slab(union: &geo::MultiPolygon<f64>, origin_mm: [f64; 2], first_index: usize, prefix: &str, noise_extent_mm: f64) -> Result<Vec<(Polygon<f64>, Polygon<f64>)>> {
    use geo::algorithm::Validation;
    let to_si = |r: &LineString<f64>| LineString::new(r.0.iter().map(|p| geo::Coord { x: (p.x - origin_mm[0]) * 1e-3, y: (p.y - origin_mm[1]) * 1e-3 }).collect());
    let to_mm = |r: &LineString<f64>| LineString::new(r.0.iter().map(|p| geo::Coord { x: p.x * 1e3 + origin_mm[0], y: p.y * 1e3 + origin_mm[1] }).collect());
    let mut out = Vec::with_capacity(union.0.len());
    let mut stats = super::copper_normalize::Stats { noise_extent_mm, ..Default::default() };
    for (n, poly) in union.0.iter().enumerate() {
        // The slab union is rebuilt here from the field copper (layer copper plus barrels), so it can
        // create fresh hole contacts: it goes through the same canonical normalisation.
        let parts = super::copper_normalize::normalize_polygon(poly, &format!("{prefix}:{}", first_index + n), &mut stats)?;
        if parts.len() != 1 || !poly.is_valid() { eprintln!("P7_COPPER_RING_SPLIT shape={prefix}:{} loops={}", first_index + out.len(), parts.len()); }
        for part in parts {
            let si = Polygon::new(to_si(part.exterior()), part.interiors().iter().map(&to_si).collect());
            if si.is_valid() { out.push((part, si)); continue; }
            let before = (si.interiors().len(), 1usize);
            let split = match split_self_touching(&si) {
                Ok(v) => v,
                Err(e) => {
                    // A feature that only the metre scaling invalidated and that cannot be split is dropped as noise when the dropped total stays within 1e-6 of the slab area (checked below).
                    use geo::Area;
                    let area_mm2 = part.unsigned_area();
                    eprintln!("P7_COPPER_NOISE_DROP shape={prefix}:{} area {area_mm2:e} mm2 after metre scaling: {e}", first_index + out.len());
                    stats.parts_dropped += 1; stats.dropped_area += area_mm2; stats.shapes_changed += 1;
                    continue;
                }
            };
            eprintln!("P7_COPPER_SI_NORMALIZED shape={prefix}:{} holes {}->{} parts {}->{} reason {:?}", first_index + out.len(), before.0, split.iter().map(|p| p.interiors().len()).sum::<usize>(), before.1, split.len(), si.check_validation().err().map(|e| e.to_string().chars().take(300).collect::<String>()));
            // The SI polygon is the canonical metal. Its millimetre twin is the original polygon clipped to it, so it
            // never grows over a neighbouring grid-width sliver (ownership reads the millimetre polygons).
            for s in split {
                let back = Polygon::new(to_mm(s.exterior()), s.interiors().iter().map(&to_mm).collect());
                let clipped = crate::geometry::areas::intersection(&crate::geometry::areas::one(part.clone()), &crate::geometry::areas::one(back.clone()));
                let mm = if clipped.0.len() == 1 { clipped.0.into_iter().next().unwrap() } else { back };
                out.push((mm, s));
            }
        }
    }
    if stats.dropped_area > 0. {
        use geo::Area;
        let total = union.unsigned_area();
        if stats.dropped_area > super::copper_normalize::AREA_TOLERANCE * total {
            return Err(error("COPPER_RING", &format!("physical copper polygons dropped as noise after metre scaling total {:e} mm2, above {:e} relative of the slab area {:e} mm2", stats.dropped_area, super::copper_normalize::AREA_TOLERANCE, total)));
        }
    }
    if stats.shapes_changed > 0 { eprintln!("P7_COPPER_NORMALIZATION slab {}", stats.evidence()); }
    Ok(out)
}

/// Owner of a point that lies in several same-layer copper polygons: the one polygon whose interior contains it
/// (shared boundary of touching copper), or, when several do, the one that dwarfs the rest by more than 1e6 in
/// area (the others are numerical slivers of the canonical union, noise tier). Anything else stays ambiguous.
fn resolve_shared_owner(polys: &[Polygon<f64>], found: &[usize], p: &Point<f64>) -> Option<usize> {
    use geo::{Area, Contains};
    let inner: Vec<usize> = found.iter().copied().filter(|&i| polys[i].contains(p)).collect();
    match inner.len() {
        1 => Some(inner[0]),
        n if n > 1 => {
            let areas: Vec<f64> = inner.iter().map(|&i| polys[i].unsigned_area()).collect();
            let (k, &big) = areas.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
            let rest: f64 = areas.iter().sum::<f64>() - big;
            if rest <= big * 1e-6 {
                eprintln!("P7_COPPER_OWNER_NOISE point=({:.9},{:.9}) mm owner=physical:{} slivers={:?} sliverAreaMm2={:e} ownerAreaMm2={:e}", p.x(), p.y(), inner[k], inner.iter().filter(|&&i| i != inner[k]).collect::<Vec<_>>(), rest, big);
                Some(inner[k])
            } else { None }
        }
        _ => None,
    }
}

fn key(p: [f64; 3]) -> [u64; 3] { p.map(|v| if v == 0.0 { 0 } else { v.to_bits() }) }

/// Exact field nodes only: no nearest-node interpolation or net-name welding.
/// CDT-generated vertices are not predictable here and therefore refuse.
pub(super) fn trace(input: &Input, map: &MixedMap<'_>, budget: Budget) -> Result<PotentialTrace> {
    let started = Instant::now();
    let mesh = &map.mesh().mesh;
    let polygon_vertices = input.metals.iter().try_fold(0usize, |n,m| m.holes_m.iter().try_fold(n.checked_add(m.polygon_m.len())?, |v,r| v.checked_add(r.len()))).ok_or_else(||error("MEMORY_LIMIT", "polygon size overflow"))?;
    let reserve = mesh.copper_tetrahedra.len().checked_mul(4).and_then(|v|v.checked_mul(192)).and_then(|v|v.checked_add(mesh.copper_interfaces.len().checked_mul(128)?)).and_then(|v|v.checked_add(polygon_vertices.checked_mul(64)?)).and_then(|v|v.checked_add(map.potential_count().checked_mul(128)?)).ok_or_else(||error("MEMORY_LIMIT", "trace plan overflow"))?;
    if reserve > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace plan exceeds budget")); }
    let rows = map.electric_node_trace();
    let count = map.potential_count();
    let mut surface = BTreeMap::new();
    for row in rows {
        let p = *map.mesh().mesh.nodes_m.get(row.node).ok_or_else(|| error("PLANAR_TRACE", "surface node out of range"))?;
        if p.iter().any(|v| !v.is_finite()) { return Err(error("PLANAR_TRACE", "nonfinite surface node")); }
        if let Some(old) = surface.insert(key(p), row.weights.clone()) {
            if old != row.weights { return Err(error("PLANAR_TRACE", "conflicting physical potential coordinates")); }
        }
    }
    let index_bytes = mesh.copper_interfaces.len().checked_mul(128).ok_or_else(|| error("MEMORY_LIMIT", "face index overflow"))?;
    if index_bytes > budget.max_bytes || mesh.copper_interfaces.len() > budget.max_local_terms { return Err(error("MEMORY_LIMIT", "face index exceeds budget")); }
    let index = RTree::bulk_load(mesh.copper_interfaces.iter().enumerate().map(|(index,face)| {
        let points = face.nodes.map(|i| mesh.nodes_m[i]);
        let lower = std::array::from_fn(|d| points.iter().map(|p|p[d]).fold(f64::INFINITY,f64::min)-1e-15);
        let upper = std::array::from_fn(|d| points.iter().map(|p|p[d]).fold(f64::NEG_INFINITY,f64::max)+1e-15);
        FaceIndex { index, bounds: AABB::from_corners(lower,upper) }
    }).collect());
    let polygons: Vec<_> = input.metals.iter().map(|m| Polygon::new(LineString::from(m.polygon_m.clone()), m.holes_m.iter().cloned().map(LineString::from).collect())).collect();
    // Recreate the same slab unions in their original millimetre coordinates.
    // Mesh ownership was classified in this coordinate system; an SI
    // round-trip can move the centroid of a very narrow arrangement face.
    let mut original_polygons = Vec::with_capacity(input.metals.len());
    for slab in input.z_m.windows(2) {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "trace ownership geometry")); }
        let mid = (slab[0] + slab[1]) / 2.;
        let active = map.mesh().field_copper.iter().filter(|(_,span,_)| mid > span[0] && mid < span[1]);
        let vertices = active.clone().flat_map(|(set,_,_)|set.0.iter()).flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(|| error("MEMORY_LIMIT", "trace union overflow"))?;
        let required = reserve.checked_add(vertices.checked_mul(1024).ok_or_else(||error("MEMORY_LIMIT", "trace union overflow"))?).and_then(|v|v.checked_add(polygon_vertices.checked_mul(64)?)).ok_or_else(||error("MEMORY_LIMIT", "trace union overflow"))?;
        if required > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace union exceeds budget")); }
        let union = crate::geometry::areas::union(active.flat_map(|(set,_,_)| set.0.iter().cloned()));
        original_polygons.extend(canonical_slab(&union, map.mesh().origin_mm, original_polygons.len(), "physical", 0.)?.into_iter().map(|(mm, _)| mm));
    }
    if original_polygons.len() != input.metals.len() { return Err(error("PLANAR_TRACE", "slab ownership geometry mismatch")); }
    // Establish ownership from physical copper tetrahedra, never coordinate alone.
    let mut owned = BTreeMap::new();
    for tetra in &mesh.copper_tetrahedra {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "trace ownership")); }
        let points = tetra.nodes.map(|i| mesh.nodes_m[i]);
        // The extrusion owns the entire planar triangle by its centroid.
        // A tetrahedron centroid is biased toward one triangle corner and can
        // land on a rounded polygon boundary for a very thin arrangement face.
        let triangle = map.mesh().planar_triangles.get(tetra.planar_triangle)
            .ok_or_else(|| error("PLANAR_TRACE", "copper planar triangle out of range"))?;
        let center = ownership_probe(triangle.map(|i| mesh.nodes_m[i]), points);
        let slab = input.z_m.get(tetra.slab..=tetra.slab + 1).ok_or_else(|| error("PLANAR_TRACE", "copper slab out of range"))?;
        let xy = triangle.map(|i| map.mesh().planar_nodes_mm[i]);
        let probe_mm = Point::new(xy.iter().map(|p|p[0]).sum::<f64>()/3.,xy.iter().map(|p|p[1]).sum::<f64>()/3.);
        let matches: Vec<_> = input.metals.iter().zip(&original_polygons).enumerate().filter(|(_, (metal,poly))| slab[0] == metal.z0_m && slab[1] == metal.z1_m && poly.intersects(&probe_mm)).map(|(i,_)|i).collect();
        let matches = if matches.len() > 1 { resolve_shared_owner(&original_polygons, &matches, &probe_mm).map_or(matches, |o| vec![o]) } else { matches };
        if matches.len() != 1 {
            let board = map.mesh();
            let xy = triangle.map(|i| board.planar_nodes_mm[i]);
            let probe_mm = Point::new(xy.iter().map(|p|p[0]).sum::<f64>()/3.,xy.iter().map(|p|p[1]).sum::<f64>()/3.);
            let original: Vec<_> = board.field_copper.iter().enumerate().filter(|(_, (poly, span, _))| slab[0] >= span[0] && slab[1] <= span[1] && poly.intersects(&probe_mm)).map(|(i,_)|i).collect();
            return Err(error("PLANAR_TRACE", &format!("ambiguous or missing copper solid ownership: probe={center:?} slab={} triangle={} matches={matches:?} original={original:?}", tetra.slab, tetra.planar_triangle)));
        }
        for point in points {
            let weights = surface.get(&key(point)).ok_or_else(||error("PLANAR_TRACE", "copper node missing mixed potential"))?;
            if let Some(old) = owned.insert((matches[0],key(point)), weights.clone()) {
                if old != *weights { return Err(error("PLANAR_TRACE", "ambiguous owned node")); }
            }
        }
        let bytes=owned.len().checked_mul(192).and_then(|v|v.checked_add(index_bytes)).ok_or_else(||error("MEMORY_LIMIT", "owned nodes overflow"))?;
        if owned.len()>budget.max_nodes || bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "owned nodes exceed budget")); }
    }
    let mut nodes = Vec::new();
    for (plane, &mesh_id) in input.planes.iter().enumerate() {
        let z = *input.z_m.get(plane).ok_or_else(|| error("PLANAR_TRACE", "plane z missing"))?;
        let xy = match input.meshes.get(mesh_id) {
            Some(PlaneInput::Exact { xy_m, .. }) => xy_m,
            _ => return Err(error("PLANAR_TRACE", "exact field meshes required")),
        };
        for &p in xy {
            if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "planar trace")); }
            for (metal_index, (metal, poly)) in input.metals.iter().zip(&polygons).enumerate() {
                if z >= metal.z0_m && z <= metal.z1_m && poly.intersects(&Point::from(p)) {
                    let position_m = [p[0], p[1], z];
                    let weights = if let Some(exact) = owned.get(&(metal_index,key(position_m))) { exact.clone() } else {
                        let mut candidate: Option<Vec<(usize,f64)>> = None;
                        for hit in index.locate_in_envelope_intersecting(&AABB::from_point(position_m)) {
                            if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "planar trace interpolation")); }
                            let face = &mesh.copper_interfaces[hit.index];
                            let coords = face.nodes.map(|i| map.mesh().mesh.nodes_m[i]);
                            if !coords.iter().all(|p| p[2] >= metal.z0_m && p[2] <= metal.z1_m && poly.intersects(&Point::new(p[0],p[1]))) { continue; }
                            if let Some(bary) = barycentric(position_m, coords[0], coords[1], coords[2]) {
                                let mut combined = BTreeMap::<usize,f64>::new();
                                for j in 0..3 {
                                    let row = owned.get(&(metal_index,key(coords[j]))).ok_or_else(|| error("PLANAR_TRACE", "surface triangle missing potential"))?;
                                    for &(i,w) in row { *combined.entry(i).or_default() += bary[j]*w; }
                                }
                                let result: Vec<_> = combined.into_iter().filter(|(_,w)| *w > 1e-14).collect();
                                if let Some(ref previous) = candidate {
                                    if previous.len() != result.len() || previous.iter().zip(&result).any(|(a,b)| a.0 != b.0 || (a.1-b.1).abs() > 1e-10) {
                                        return Err(error("PLANAR_TRACE", "ambiguous surface interpolation across conductors"));
                                    }
                                } else { candidate = Some(result); }
                            }
                        }
                        candidate.ok_or_else(|| error("PLANAR_TRACE", "field node outside physical conductor surface"))?
                    };
                    if weights.iter().any(|&(i,w)| i >= count || !w.is_finite() || w < 0.0) || (weights.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() > 1e-12 { return Err(error("PLANAR_TRACE", "invalid partition of unity")); }
                    nodes.push(TraceNode { metal_source_id: metal.source_id.clone(), position_m, weights });
                    let bytes = nodes.len().checked_mul(256).and_then(|x| x.checked_add(surface.len().checked_mul(128)?)).and_then(|x| x.checked_add(index_bytes)).ok_or_else(|| error("MEMORY_LIMIT", "trace size overflow"))?;
                    if nodes.len() > budget.max_nodes || bytes > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace exceeds budget")); }
                }
            }
        }
    }
    Ok(PotentialTrace { potential_nodes: (0..count).map(|i| format!("mixed:{i}")).collect(), nodes })
}

fn ownership_probe(triangle: [[f64;3];3], tetra: [[f64;3];4]) -> [f64;3] {
    [triangle.iter().map(|p| p[0]).sum::<f64>() / 3.,
     triangle.iter().map(|p| p[1]).sum::<f64>() / 3.,
     tetra.iter().map(|p| p[2]).sum::<f64>() / 4.]
}

// P1 interpolation on an actual surface triangle, with a scale-relative plane check.
fn barycentric(p: [f64;3], a: [f64;3], b: [f64;3], c: [f64;3]) -> Option<[f64;3]> {
    let sub = |x: [f64;3], y: [f64;3]| [x[0]-y[0], x[1]-y[1], x[2]-y[2]];
    let dot = |x: [f64;3], y: [f64;3]| x[0]*y[0]+x[1]*y[1]+x[2]*y[2];
    let u=sub(b,a); let v=sub(c,a); let w=sub(p,a);
    let uu=dot(u,u); let uv=dot(u,v); let vv=dot(v,v); let wu=dot(w,u); let wv=dot(w,v);
    let det=uu*vv-uv*uv;
    if !det.is_finite() || det <= 0.0 { return None; }
    let beta=(wu*vv-wv*uv)/det; let gamma=(wv*uu-wu*uv)/det; let alpha=1.0-beta-gamma;
    let residual=[w[0]-beta*u[0]-gamma*v[0],w[1]-beta*u[1]-gamma*v[1],w[2]-beta*u[2]-gamma*v[2]];
    if dot(residual,residual) > 1e-24*uu.max(vv) || [alpha,beta,gamma].iter().any(|x| *x < -1e-12 || *x > 1.0+1e-12) { return None; }
    let mut weights=[alpha.max(0.0),beta.max(0.0),gamma.max(0.0)]; let sum: f64=weights.iter().sum();
    for x in &mut weights { *x /= sum; } Some(weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ownership_probe_uses_extrusion_triangle_centroid() {
        let tri = [[0.,0.,0.],[3.,0.,0.],[0.,3.,0.]];
        let tet = [[0.,0.,0.],[3.,0.,0.],[0.,0.,1.],[0.,3.,0.]];
        assert_eq!(ownership_probe(tri, tet), [1.,1.,0.25]);
    }
    #[test] fn surface_interpolation_preserves_constant_and_rejects_void() {
        let a=[0.,0.,0.]; let b=[1.,0.,0.]; let c=[0.,1.,0.];
        let w=barycentric([0.25,0.25,0.],a,b,c).unwrap();
        assert!((w.iter().sum::<f64>()-1.).abs()<1e-14);
        assert!(barycentric([0.8,0.8,0.],a,b,c).is_none());
        assert!(barycentric([0.25,0.25,0.1],a,b,c).is_none());
    }
}

/// Build a conforming layered field request from the already prepared native grid.
/// Distinct overlapping copper solids are refused until ownership is partitioned.
pub(super) struct OriginalGeometry { polygons: Vec<Polygon<f64>>, si_polygons: Vec<Polygon<f64>>, bytes: usize }
pub(super) fn input_from_board(board: &super::board_mesh::BoardMesh, request: &super::board_mesh::Request, budget: Budget) -> Result<(Input, Vec<super::layered_capacitance::SpatialDielectric>, OriginalGeometry)> {
    use super::layered_capacitance::{Metal, Dielectric, Exterior, SpatialDielectric};
    use geo::{Area, Relate};
    use geo::algorithm::{coordinate_position::CoordPos, dimensions::Dimensions};
    let started=Instant::now();
    let n=board.planar_nodes_mm.len();
    let planned=n.checked_mul(128).and_then(|x| x.checked_add(board.planar_triangles.len().checked_mul(128)?)).and_then(|x| x.checked_add(board.mesh.nodes_m.len().checked_mul(32)?)).ok_or_else(||error("MEMORY_LIMIT", "field request size overflow"))?;
    if n > budget.max_nodes || planned > budget.max_bytes { return Err(error("MEMORY_LIMIT", "field request exceeds budget")); }
    if n == 0 || board.mesh.nodes_m.len()%n != 0 { return Err(error("PLANAR_TRACE", "invalid product mesh")); }
    let xy: Vec<_>=board.planar_nodes_mm.iter().map(|p|[(p[0]-board.origin_mm[0])*1e-3,(p[1]-board.origin_mm[1])*1e-3]).collect();
    let z: Vec<_>=board.mesh.nodes_m.chunks_exact(n).map(|p|p[0][2]).collect();
    let bounds=[[xy.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min),xy.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)], [xy.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max),xy.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)]];
    let convert = |ring: &geo::LineString<f64>| ring.0.iter().map(|p|[(p.x-board.origin_mm[0])*1e-3,(p.y-board.origin_mm[1])*1e-3]).collect::<Vec<_>>();
    let mut metals=Vec::new(); let mut originals=Vec::new(); let mut si_originals=Vec::new(); let mut original_bytes=0usize;
    let mut output_bytes=planned;
    // Partition at existing z planes, unioning overlapping foil/barrel supports
    // in each slab. This matches the original single-owner copper mesh.
    for slab in z.windows(2) {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "field geometry adapter")); }
        let mid=(slab[0]+slab[1])/2.;
        let active=board.field_copper.iter().filter(|(_,span,_)|mid>span[0] && mid<span[1]);
        let vertices=active.clone().flat_map(|(set,_,_)|set.0.iter()).flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "copper polygon size overflow"))?;
        let needed=output_bytes.checked_add(vertices.checked_mul(1024).ok_or_else(||error("MEMORY_LIMIT", "union size overflow"))?).ok_or_else(||error("MEMORY_LIMIT", "union size overflow"))?;
        if needed>budget.max_bytes { return Err(error("MEMORY_LIMIT", "copper union exceeds budget")); }
        let union=crate::geometry::areas::union(active.flat_map(|(set,_,_)|set.0.iter().cloned()));
        let union_vertices=union.0.iter().flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|v,r|v.checked_add(r.0.capacity())).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        let union_bytes=union_vertices.checked_mul(64).and_then(|v|v.checked_add(union.0.len().checked_mul(256)?)).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        original_bytes=original_bytes.checked_add(union_bytes).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        output_bytes=output_bytes.checked_add(union_bytes).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        if output_bytes>budget.max_bytes {return Err(error("MEMORY_LIMIT","original geometry exceeds budget"));}
        let canonical = canonical_slab(&union, board.origin_mm, metals.len(), "physical", 4. * (request.max_triangle_area_m2 * 1e6).sqrt())?;
        for (_, poly) in &canonical {
            let vertices=std::iter::once(poly.exterior()).chain(poly.interiors()).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?;
            output_bytes=output_bytes.checked_add(vertices.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?).and_then(|v|v.checked_add(256)).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?;
            if output_bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "metal polygons exceed budget")); }
            if metals.len() >= budget.max_local_terms { return Err(error("MESH_LIMIT", "field metal count")); }
            let ring = |r: &geo::LineString<f64>| r.0.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>();
            metals.push(Metal { source_id:format!("physical:{}",metals.len()),polygon_m:ring(poly.exterior()),holes_m:poly.interiors().iter().map(ring).collect(),z0_m:slab[0],z1_m:slab[1] });
        }
        for (mm, si) in canonical { originals.push(mm); si_originals.push(si); }
    }
    let dielectric=vec![Dielectric { source_id:"explicit-air-domain".into(),z0_m:z[0],z1_m:*z.last().unwrap(),epsilon_r:1. }];
    let mut regions=Vec::new(); let mut base=0.;
    for layer in &request.stack {
        let end=base+layer.thickness_m;
        if !layer.copper {
            let mut domain=crate::geometry::areas::difference(&board.field_substrate,&board.field_bores);
            for (span,bore) in &board.field_partial_bores { if span[0]<end && span[1]>base { if span[0]>base || span[1]<end { return Err(error("PLANAR_TRACE", "bore crosses dielectric interval boundary")); } domain=crate::geometry::areas::difference(&domain,bore); } }
            // Dielectric footprints go through the same canonical normalisation as copper (area bound 1e-6, noise tier).
            for (_, poly) in canonical_slab(&domain, board.origin_mm, regions.len(), "dielectric", 4. * (request.max_triangle_area_m2 * 1e6).sqrt())? { let vertices=std::iter::once(poly.exterior()).chain(poly.interiors()).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "dielectric polygon overflow"))?;
                output_bytes=output_bytes.checked_add(vertices.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT", "dielectric polygon overflow"))?).and_then(|v|v.checked_add(256)).ok_or_else(||error("MEMORY_LIMIT", "dielectric polygon overflow"))?;
                if output_bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "dielectric polygons exceed budget")); }
                let ring=|r:&geo::LineString<f64>| r.0.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>();
                regions.push(SpatialDielectric { z0_m:base,z1_m:end,epsilon_r:layer.epsilon_r.ok_or_else(||error("MATERIAL", "missing epsilon"))?,polygon_m:ring(poly.exterior()),holes_m:poly.interiors().iter().map(ring).collect() }); }
        }
        base=end;
    }
    let planes=vec![0;z.len()];
    Ok((Input { meshes:vec![PlaneInput::Exact { source_id:"board-overlay".into(),xy_m:xy,triangles:board.planar_triangles.clone() }], planes,z_m:z,xy_bounds_m:bounds,metals,dielectrics:dielectric,exterior:Exterior::InfinityBox },regions,OriginalGeometry{polygons:originals,si_polygons:si_originals,bytes:original_bytes}))
}

#[cfg(test)]
mod board_adapter_tests {
    use super::*;
    use super::super::{board_mesh::{self, Electrode, Request}, mixed_conductor, AllocationBudget};
    use std::time::Duration;
    #[test]
    fn hybrid_via_masks_limit_volume_support_without_changing_copper() {
        let args:serde_json::Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/reference-swap-native-args.json")).unwrap();
        let board=crate::import_json(args[0].as_str().unwrap()).board.unwrap();
        let request:Request=serde_json::from_value(args[1].clone()).unwrap();
        let allocation=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000}; let wall=Duration::from_secs(20);
        let profile=mixed_conductor::CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        let plain=board_mesh::build_profiled(&board,&request,allocation,wall,None,&[],Some(profile)).unwrap();
        let masked=board_mesh::build_profiled_hybrid(&board,&request,allocation,wall,Some(profile)).unwrap();
        assert_eq!(masked.hybrid_mesh_masks.len(),2);
        assert_eq!(plain.barrel_ids,masked.barrel_ids); assert_eq!(plain.source_mapping,masked.source_mapping);
        assert!((plain.mesh.copper_volume_m3-masked.mesh.copper_volume_m3).abs()<=1e-8*plain.mesh.copper_volume_m3);
        for (name,area) in &plain.electrode_areas_m2 { assert!((masked.electrode_areas_m2[name]-area).abs()<=1e-8*area); }
        let longest=|mesh:&board_mesh::BoardMesh| { let map=mixed_conductor::prepare(mesh,&request.electrodes,1e6,allocation,wall).unwrap(); map.local_current_supports().unwrap().iter().flat_map(|s| {let t=s.tetra;(0..4).flat_map(move |i|(0..i).map(move |j| (t[i][0]-t[j][0]).hypot(t[i][1]-t[j][1]))) }).fold(0_f64,f64::max) };
        let before=longest(&plain); let after=longest(&masked); eprintln!("P7_MASK_SUPPORT before={before:e} after={after:e}"); assert!(after<before);
    }
    #[test]
    fn reference_swap_two_vias_preserves_owned_spatial_trace() {
        // Captured at the native call boundary before executing the solver.
        let args: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/reference-swap-native-args.json")).unwrap();
        let board = crate::import_json(args[0].as_str().unwrap()).board.unwrap();
        let request: Request = serde_json::from_value(args[1].clone()).unwrap();
        let allocation = AllocationBudget { max_bytes:512*1024*1024, max_nodes:100_000, max_tetrahedra:200_000 };
        let wall = Duration::from_secs(20);
        let profile = mixed_conductor::CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        let board_mesh = board_mesh::build_profiled(&board,&request,allocation,wall,None,&[],Some(profile)).unwrap();
        let map = mixed_conductor::prepare(&board_mesh,&request.electrodes,1e6,allocation,wall).unwrap();
        let budget = Budget { max_nodes:100_000, max_local_terms:2_400_000, max_bytes:512*1024*1024, max_wall:wall };
        let (input,regions,original) = input_from_board(&board_mesh,&request,budget).unwrap();
        assert!(certified_trace(&input,&original,&map,Budget{max_bytes:64,..budget}).is_err());
        let mut wrong_mesh=input.clone();
        if let PlaneInput::Exact { xy_m, .. } = &mut wrong_mesh.meshes[0] { xy_m[0][0] += 1e-6; }
        assert!(certified_trace(&wrong_mesh,&original,&map,budget).is_err());
        let (trace,certified) = certified_trace(&input,&original,&map,budget).unwrap();
        assert!(!trace.nodes.is_empty());
        let action = super::super::layered_capacitance::prepare_certified_action(&input,&trace,&regions,&certified,budget).unwrap();
        drop(action);
        let altered_input = input.clone();
        assert!(super::super::layered_capacitance::prepare_certified_action(&altered_input,&trace,&regions,&certified,budget).is_err());
        let mut missing = trace.clone(); missing.nodes.remove(0);
        assert!(super::super::layered_capacitance::prepare_certified_action(&input,&missing,&regions,&certified,budget).is_err());
        let mut unmatched = trace.clone(); let mut extra = unmatched.nodes[0].clone(); extra.position_m[0] += 100.; unmatched.nodes.push(extra);
        assert!(super::super::layered_capacitance::prepare_certified_action(&input,&unmatched,&regions,&certified,budget).is_err());
    }
    #[test]
    fn production_profiled_via_field_trace_covers_mixed_coordinates() {
        let board=crate::import_json(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
        // Exact geometry request captured from automatic-hybrid DSL before native execution.
        let request:Request=serde_json::from_str(r#"{"roiMm":[[-0.54,-1.07],[10.54,0.6900000000000001]],"stack":[{"name":"TOP","copper":true,"thicknessM":1e-05,"conductivitySM":58000000,"epsilonR":null},{"name":"substrate","copper":false,"thicknessM":0.0002,"conductivitySM":null,"epsilonR":4},{"name":"BOTTOM","copper":true,"thicknessM":1e-05,"conductivitySM":58000000,"epsilonR":null}],"finishedHolePlatingM":2.5e-05,"barrelConductivitySM":58000000,"airMarginM":0.002,"maxTriangleAreaM2":1e-06,"maxVerticalStepM":0.002,"curveDeviationMm":0.001,"electrodes":[{"name":"foil-terminal-0","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"foil-terminal-1","sourcePath":"pads[1]","layer":"BOTTOM","upper":true},{"name":"foil-terminal-2","sourcePath":"pads[2]","layer":"BOTTOM","upper":true},{"name":"foil-terminal-3","sourcePath":"pads[3]","layer":"BOTTOM","upper":true}],"ports":[["foil-terminal-0","foil-terminal-2"],["foil-terminal-1","foil-terminal-3"]],"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"minTriangleAngleDeg":0,"airEdgeDivisions":1,"domainPolygonMm":[[-0.54,-1.07],[10.54,-1.07],[10.54,0.6900000000000001],[-0.54,0.6900000000000001]],"cutPlanes":{}}"#).unwrap();
        let allocation=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000};
        let wall=Duration::from_secs(20);
        let profile=mixed_conductor::CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        let board_mesh=board_mesh::build_profiled(&board,&request,allocation,wall,None,&[],Some(profile)).unwrap();
        // Witness from the formerly duplicated pre/post-overlay outer spoke.
        let old=[f64::from_bits(4572911535762694515),f64::from_bits(4562287101040354732)];
        let canonical=[f64::from_bits(4572911535755849934),f64::from_bits(4562287101017622609)];
        let xy=|p:&[f64;3]| [p[0],p[1]];
        assert!(board_mesh.mesh.nodes_m.iter().any(|p|xy(p)==canonical));
        assert!(!board_mesh.mesh.nodes_m.iter().any(|p|xy(p)==old),"pre-overlay spoke endpoint recreated a duplicate boundary");
        eprintln!("P7_CANONICAL_SPOKE copperTetra={} minCopperVolumeM3={:e}",board_mesh.mesh.copper_tetrahedra.len(),board_mesh.mesh.copper_tetrahedra.iter().map(|t|t.volume_m3).fold(f64::INFINITY,f64::min));
        let smallest=board_mesh.mesh.copper_tetrahedra.iter().min_by(|a,b|a.volume_m3.total_cmp(&b.volume_m3)).unwrap();
        eprintln!("P7_CANONICAL_MIN_TET {:?}",smallest.nodes.map(|i|board_mesh.mesh.nodes_m[i]));
        let map=mixed_conductor::prepare(&board_mesh,&request.electrodes,1e6,allocation,wall).unwrap();
        let budget=Budget{max_nodes:100_000,max_local_terms:2_400_000,max_bytes:512*1024*1024,max_wall:wall};
        let (input,regions,_original)=input_from_board(&board_mesh,&request,budget).unwrap();
        let trace=trace(&input,&map,budget).unwrap();
        let referenced:std::collections::BTreeSet<_>=trace.nodes.iter().flat_map(|n|n.weights.iter().map(|&(i,_)|i)).collect();
        assert_eq!(referenced.len(),map.potential_count(),"every mixed mode must be referenced by a matched physical metal node");
        let action=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&trace,&regions,budget).unwrap();
        let dielectric_nodes:std::collections::BTreeSet<_>=board_mesh.mesh.tetrahedra.iter().flat_map(|t|t.nodes).collect();
        let mut interior=vec![true;map.potential_count()];
        for row in map.electric_node_trace() { if dielectric_nodes.contains(&row.node) { for (i,_) in row.weights {interior[i]=false;} } }
        let mode=interior.iter().position(|&v|v).expect("physical interior mode");
        let mut excitation=vec![0.;map.potential_count()];excitation[mode]=1.;
        let charge=action.apply(&excitation).unwrap();
        assert!(charge.iter().all(|&q|q==0.),"interior-only mode must have exactly zero field charge");
        drop(action);
        let mut unused=trace.clone();unused.potential_nodes.push("unreferenced-bogus".into());
        let failure=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&unused,&regions,budget).err().unwrap();
        assert!(failure.contains("FIELD_TRACE") && failure.contains("unusedCount=1"),"{failure}");
        let mut unmatched=trace.clone();let mut extra=unmatched.nodes[0].clone();extra.position_m[0]+=100.;unmatched.nodes.push(extra);
        let failure=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&unmatched,&regions,budget).err().unwrap();
        assert!(failure.contains("FIELD_TRACE") && !failure.contains("unmatchedCount=0"),"{failure}");
        let mut missing=trace.clone();missing.nodes.remove(0);
        let failure=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&missing,&regions,budget).err().unwrap();
        assert!(failure.contains("FIELD_TRACE"),"{failure}");
    }
    #[test]
    fn prepared_fr4_via_board_builds_owned_spatial_trace() {
        let board=crate::import_json(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
        let mut request:Request=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json")).unwrap();
        request.cut_planes.clear(); request.allow_insulated_copper_cuts=false; request.domain_polygon_mm=None;
        request.electrodes=["s0","s1","r0","r1"].into_iter().enumerate().map(|(i,name)|Electrode {name:name.into(),source_path:Some(format!("pads[{i}]")),source_paths:None,layer:if i==0 {"TOP"} else {"BOTTOM"}.into(),upper:i!=0}).collect();
        request.ports=vec![["s0".into(),"r0".into()],["s1".into(),"r1".into()]];
        let allocation=AllocationBudget {max_bytes:1<<30,max_nodes:200_000,max_tetrahedra:400_000};
        let wall=Duration::from_secs(20);
        let board_mesh=board_mesh::build(&board,&request,allocation,wall).unwrap();
        let map=mixed_conductor::prepare(&board_mesh,&request.electrodes,10_000.,allocation,wall).unwrap();
        let budget=Budget {max_nodes:200_000,max_local_terms:4_000_000,max_bytes:1<<30,max_wall:wall};
        let (input,regions,_original)=input_from_board(&board_mesh,&request,budget).unwrap();
        assert!(!regions.is_empty());
        assert!(regions.iter().any(|r|r.epsilon_r>1.));
        let trace=trace(&input,&map,budget).unwrap();
        assert!(!trace.nodes.is_empty());
        assert_eq!(trace.potential_nodes.len(),map.potential_count());
    }
}

/// Certify the exact board product mesh from the already classified copper and dielectric tetrahedra.
pub(super) fn certified_trace<'a>(input: &'a Input, original: &OriginalGeometry, map: &MixedMap<'_>, budget: Budget) -> Result<(PotentialTrace, super::layered_capacitance::CertifiedOverlay<'a>)> {
    use super::layered_capacitance::CertifiedOverlay;
    let start=Instant::now(); let board=map.mesh(); let mesh=&board.mesh;
    let (xy,triangles)=match input.meshes.as_slice() { [PlaneInput::Exact{xy_m,triangles,..}] => (xy_m,triangles), _=>return Err(error("PLANAR_TRACE","certificate requires one exact mesh")) };
    let n=board.planar_nodes_mm.len(); let nt=board.planar_triangles.len(); let slabs=input.z_m.len().checked_sub(1).ok_or_else(||error("PLANAR_TRACE","missing slabs"))?;
    if n==0 || xy.len()!=n || triangles!=&board.planar_triangles || input.planes.iter().any(|&x|x!=0) || input.planes.len()!=input.z_m.len() || mesh.nodes_m.len()!=n*input.z_m.len() { return Err(error("PLANAR_TRACE","certificate product mesh mismatch")); }
    for (k,&z) in input.z_m.iter().enumerate() { for (i,p) in xy.iter().enumerate() { if mesh.nodes_m[k*n+i]!=[p[0],p[1],z] {return Err(error("PLANAR_TRACE","certificate node mismatch"));} } }
    let count=slabs.checked_mul(nt).ok_or_else(||error("MEMORY_LIMIT","certificate size overflow"))?;
    let input_vertices=input.metals.iter().try_fold(0usize,|v,m|m.holes_m.iter().try_fold(v.checked_add(m.polygon_m.len())?,|v,r|v.checked_add(r.len()))).ok_or_else(||error("MEMORY_LIMIT","certificate input overflow"))?;
    let bytes=count.checked_mul(64).and_then(|v|v.checked_add(mesh.nodes_m.len().checked_mul(128)?)).and_then(|v|v.checked_add(mesh.copper_tetrahedra.len().checked_mul(4*64)?)).and_then(|v|v.checked_add(input.metals.len().checked_mul(256)?)).and_then(|v|v.checked_add(input_vertices.checked_mul(64)?)).ok_or_else(||error("MEMORY_LIMIT","certificate size overflow"))?;
    if bytes>budget.max_bytes || mesh.nodes_m.len()>budget.max_nodes || count>budget.max_local_terms {return Err(error("MEMORY_LIMIT","certificate exceeds budget"));}
    let retained_union=original.bytes;
    if bytes.checked_add(retained_union).is_none_or(|v|v>budget.max_bytes) {return Err(error("MEMORY_LIMIT","certificate retained geometry exceeds budget"));}
    if original.polygons.len()!=input.metals.len(){return Err(error("PLANAR_TRACE","certificate metal count mismatch"));}
    if original.si_polygons.len()!=input.metals.len(){return Err(error("PLANAR_TRACE","certificate metal count mismatch"));}
    // The canonical SI polygon is the metal itself (a polygon the mm to SI scaling invalidated was normalised there).
    for (i,(metal,poly)) in input.metals.iter().zip(&original.si_polygons).enumerate() {
        let convert=|r:&LineString<f64>| r.0.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>();
        if metal.source_id!=format!("physical:{i}") || metal.polygon_m!=convert(poly.exterior()) || metal.holes_m!=poly.interiors().iter().map(convert).collect::<Vec<_>>() {return Err(error("PLANAR_TRACE","certificate metal geometry mismatch"));}
    }
    let mut copper=vec![None;count]; let mut epsilon=vec![0.;count]; let mut seen=vec![0u8;count]; let mut node_owners=vec![Vec::<usize>::new();mesh.nodes_m.len()];
    let tetra_bit=|slab:usize,t:usize, nodes:[usize;4]| -> Result<u8> {
        let mut tri=triangles[t]; tri.sort(); let [a,b,c]=tri; let [a0,b0,c0]=[a,b,c].map(|i|slab*n+i); let [a1,b1,c1]=[a,b,c].map(|i|(slab+1)*n+i);
        let mut actual=nodes;actual.sort();
        for (i,mut expected) in [[a0,b0,c0,c1],[a0,b0,b1,c1],[a0,a1,b1,c1]].into_iter().enumerate() {expected.sort();if actual==expected{return Ok(1<<i);}}
        Err(error("PLANAR_TRACE","certificate tetra connectivity mismatch"))
    };
    let mut owner_noise=0usize;let mut owner_noise_max=0_f64;
    for tet in &mesh.copper_tetrahedra {
        if start.elapsed()>=budget.max_wall{return Err(error("DEADLINE","certificate copper"));}
        if tet.slab>=slabs || tet.planar_triangle>=nt {return Err(error("PLANAR_TRACE","certificate copper index"));}
        let idx=tet.slab*nt+tet.planar_triangle; let tri=triangles[tet.planar_triangle].map(|i|board.planar_nodes_mm[i]);
        let p=Point::new(tri.iter().map(|p|p[0]).sum::<f64>()/3.,tri.iter().map(|p|p[1]).sum::<f64>()/3.);
        let mut found:Vec<_>=input.metals.iter().zip(&original.polygons).enumerate().filter(|(_, (m,poly))|m.z0_m==input.z_m[tet.slab]&&m.z1_m==input.z_m[tet.slab+1]&&poly.intersects(&p)).map(|(i,_)|i).collect();
        if found.len() > 1 { if let Some(o) = resolve_shared_owner(&original.polygons, &found, &p) { found = vec![o]; } }
        if found.is_empty() {
            // Union noise (rounding, renoding): as in the board ownership, a centre within 1e-2 mm of copper of this
            // layer belongs to the nearest one if it is clearly nearest (4x closer than the next); otherwise refuse.
            use geo::{Distance,Euclidean};
            let mut near:Vec<(f64,usize)>=input.metals.iter().zip(&original.polygons).enumerate().filter(|(_,(m,_))|m.z0_m==input.z_m[tet.slab]&&m.z1_m==input.z_m[tet.slab+1]).map(|(i,(_,poly))|(Euclidean.distance(poly,&p),i)).filter(|(d,_)|*d<=1e-2).collect();
            near.sort_by(|a,b|a.0.total_cmp(&b.0));
            if near.first().is_some_and(|f|near.get(1).is_none_or(|n|4.*f.0<=n.0)) {found=vec![near[0].1];owner_noise+=1;owner_noise_max=owner_noise_max.max(near[0].0);}
            else {
                // Several copper polygons all within rounding distance (1e-6 mm) of the centre: it sits on a hairline between
                // a large pour and numerical slivers of the canonical union. The pour dwarfing the rest by more than 1e6 in area owns it.
                use geo::Area;
                let close:Vec<usize>=near.iter().filter(|x|x.0<=1e-6).map(|x|x.1).collect();
                if close.len()>1 {
                    let areas:Vec<f64>=close.iter().map(|&i|original.polygons[i].unsigned_area()).collect();
                    let (k,&big)=areas.iter().enumerate().max_by(|a,b|a.1.total_cmp(b.1)).unwrap();
                    if areas.iter().sum::<f64>()-big<=big*1e-6 {
                        eprintln!("P7_COPPER_OWNER_NOISE point=({:.9},{:.9}) mm owner=physical:{} hairlineCandidates={:?} maxDistanceMm={:e}",p.x(),p.y(),close[k],close,near.iter().map(|x|x.0).fold(0.,f64::max));
                        found=vec![close[k]];owner_noise+=1;owner_noise_max=owner_noise_max.max(near.iter().map(|x|x.0).fold(0.,f64::max));
                    }
                }
            }
        }
        if found.len()!=1 || copper[idx].is_some_and(|v|v!=found[0]) {
            use geo::{Distance,Euclidean,BoundingRect};
            let near:Vec<String>=input.metals.iter().zip(&original.polygons).enumerate().filter(|(_,(m,_))|m.z0_m==input.z_m[tet.slab]&&m.z1_m==input.z_m[tet.slab+1]).map(|(i,(_,poly))|(Euclidean.distance(poly,&p),i)).fold(vec![],|mut v:Vec<(f64,usize)>,x|{v.push(x);v.sort_by(|a,b|a.0.total_cmp(&b.0));v.truncate(3);v}).iter().map(|(d,i)|format!("physical:{i} {d:e}mm bbox {:?}",original.polygons[*i].bounding_rect().map(|b|[[b.min().x,b.min().y],[b.max().x,b.max().y]]))).collect();
            return Err(error("PLANAR_TRACE",&format!("certificate ambiguous copper owner: triangle centre ({:.9},{:.9}) mm, slab {}, owners {:?}, previous owner {:?}; nearest same-layer copper {:?}",p.x(),p.y(),tet.slab,found,copper[idx],near)));
        }
        let bit=tetra_bit(tet.slab,tet.planar_triangle,tet.nodes)?; if seen[idx]&bit!=0{return Err(error("PLANAR_TRACE","duplicate copper tetra"));} copper[idx]=Some(found[0]); seen[idx]|=bit;
        for &node in &tet.nodes { let owners=node_owners.get_mut(node).ok_or_else(||error("PLANAR_TRACE","certificate node index"))?; if !owners.contains(&found[0]) {owners.push(found[0]);} }
    }
    if owner_noise>0 {eprintln!("P7_PLANAR_TRACE_OWNER_NOISE tetrahedra={owner_noise} assigned to the clearly nearest copper, max distance {owner_noise_max:e} mm (union rounding; noise tier)");}
    for tet in &mesh.tetrahedra {
        if start.elapsed()>=budget.max_wall{return Err(error("DEADLINE","certificate dielectric"));}
        if tet.slab>=slabs || tet.planar_triangle>=nt || !tet.epsilon_r.is_finite() || tet.epsilon_r<=0. {return Err(error("PLANAR_TRACE","certificate dielectric index or material"));}
        let idx=tet.slab*nt+tet.planar_triangle; if copper[idx].is_some() || epsilon[idx]!=0. && epsilon[idx]!=tet.epsilon_r {return Err(error("PLANAR_TRACE","certificate material ownership conflict"));}
        let bit=tetra_bit(tet.slab,tet.planar_triangle,tet.nodes)?; if seen[idx]&bit!=0{return Err(error("PLANAR_TRACE","duplicate dielectric tetra"));} epsilon[idx]=tet.epsilon_r; seen[idx]|=bit;
    }
    if seen.iter().any(|&x|x!=7) {return Err(error("PLANAR_TRACE","certificate incomplete product ownership"));}
    let finite_contacts=certified_contacts(&copper,slabs,nt);
    let incidence_count=node_owners.iter().try_fold(0usize,|n,v|n.checked_add(v.len())).ok_or_else(||error("MEMORY_LIMIT","certificate node incidence overflow"))?;
    let trace_bytes=node_owners.iter().try_fold(0usize,|sum,owners|owners.iter().try_fold(sum,|v,&owner|v.checked_add(256)?.checked_add(input.metals[owner].source_id.len())?.checked_add(32))).ok_or_else(||error("MEMORY_LIMIT","certificate trace overflow"))?;
    if incidence_count>budget.max_nodes || bytes.checked_add(retained_union).and_then(|v|v.checked_add(trace_bytes)).is_none_or(|v|v>budget.max_bytes) {return Err(error("MEMORY_LIMIT","certificate trace exceeds budget"));}
    let rows=map.electric_node_trace(); let mut weights=vec![None;mesh.nodes_m.len()];
    for row in rows { let slot=weights.get_mut(row.node).ok_or_else(||error("PLANAR_TRACE","certificate trace index"))?; if slot.replace(row.weights).is_some(){return Err(error("PLANAR_TRACE","duplicate trace node"));} }
    let mut nodes=Vec::with_capacity(incidence_count); for (i,owners) in node_owners.iter().enumerate() { for &owner in owners {let row=weights[i].as_ref().ok_or_else(||error("PLANAR_TRACE","certificate trace missing"))?; if row.len()!=1 {return Err(error("PLANAR_TRACE","unexpected certificate node trace width"));} nodes.push(TraceNode{metal_source_id:input.metals[owner].source_id.clone(),position_m:mesh.nodes_m[i],weights:row.clone()});} }
    if nodes.len()>budget.max_nodes || bytes.checked_add(nodes.len().checked_mul(256).ok_or_else(||error("MEMORY_LIMIT","certificate trace overflow"))?).is_none_or(|v|v>budget.max_bytes){return Err(error("MEMORY_LIMIT","certificate trace exceeds budget"));}
    Ok((PotentialTrace{potential_nodes:(0..map.potential_count()).map(|i|format!("mixed:{i}")).collect(),nodes},CertifiedOverlay{input,triangles:nt,copper,epsilon,node_owners,finite_contacts,bytes:bytes.checked_add(retained_union).ok_or_else(||error("MEMORY_LIMIT","certificate retained geometry overflow"))?}))
}

fn certified_contacts(copper:&[Option<usize>], slabs:usize, triangles:usize) -> std::collections::BTreeSet<(usize,usize)> {
    let mut contacts=std::collections::BTreeSet::new();
    for k in 1..slabs { for t in 0..triangles {if let (Some(a),Some(b))=(copper[(k-1)*triangles+t],copper[k*triangles+t]) {if a!=b {contacts.insert((a.min(b),a.max(b)));}}} }
    contacts
}

#[cfg(test)]
mod certified_contact_tests {
    use super::*;
    #[test]
    fn point_or_edge_adjacency_does_not_create_area_contact() {
        let cells=[Some(0),None,None,Some(1)];
        assert!(certified_contacts(&cells,2,2).is_empty());
        let cells=[Some(0),None,Some(1),None];
        assert!(certified_contacts(&cells,2,2).contains(&(0,1)));
    }
}

#[cfg(test)]
mod ring_split_tests {
    use super::*;
    use geo::Area;
    #[test]
    fn pinched_ring_splits_into_valid_loops_with_equal_area() {
        let ring = LineString::from(vec![[0., 0.], [1., 0.], [1., 1.], [2., 1.], [2., 2.], [1., 2.], [1., 1.], [0., 1.], [0., 0.]]);
        let parts = split_self_touching(&Polygon::new(ring, vec![])).unwrap();
        assert_eq!(parts.len(), 2);
        assert!((parts.iter().map(|p| p.unsigned_area()).sum::<f64>() - 2.).abs() < 1e-12);
        let square = Polygon::new(LineString::from(vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.], [0., 0.]]), vec![]);
        assert_eq!(split_self_touching(&square).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod ring_node_tests {
    use super::*;
    use geo::Area;
    #[test]
    fn t_touch_and_crossing_rings_are_noded_then_split() {
        // The notch tip (2,0) touches the interior of the bottom edge: a T-touch, not a shared vertex.
        let t = LineString::from(vec![[0., 0.], [4., 0.], [4., 3.], [3., 3.], [2., 0.], [1., 3.], [0., 3.], [0., 0.]]);
        let parts = split_self_touching(&Polygon::new(t, vec![])).unwrap();
        assert_eq!(parts.len(), 2);
        assert!((parts.iter().map(|p| p.unsigned_area()).sum::<f64>() - 9.).abs() < 1e-9);
    }
}
