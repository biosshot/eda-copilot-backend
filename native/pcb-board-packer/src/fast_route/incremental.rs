//! Exact incremental form of the post-placement route plan (`nets_plan` with
//! no external or ignored nets). A candidate that moves a few pads changes only
//! the obstacle terms of those pads and the links of their nets, so every path
//! keeps its nonzero terms in the original summation order and recomputes only
//! the affected ones. Terms that are exactly zero do not change an F32 sum, and
//! bounding-box tests skip only terms that cannot be nonzero. Totals are
//! therefore bit-identical to `nets_plan(..).cpu()`; any structural change
//! falls back to the full plan.
use super::arithmetic::{self, Path, P};
use super::layout::{link_layer, route_links, route_nets, sorted_explicit, sorted_obstacles, spanning_segments};
use super::plan::{alternatives, excess};
use super::*;
use crate::model::{ExplicitConnection, PostPlaceNet, PostPlaceScoreProblem};
use rustc_hash::FxHashMap;

/// Beyond slab epsilons and F32 rounding of coordinates within the local frame.
const MARGIN: f32 = 0.01;

#[derive(Clone)]
struct Alternative { path: Path, bounds: Box2, excess: f32, obstacles: Vec<(u32, f32)>, links: Vec<(u32, f32)> }

#[derive(Clone)]
struct Link { net: Arc<str>, a: Point, b: Point, layer: Option<Arc<str>>, class: u32, bounds: Box2 }

type LinkKey = (Arc<str>, [u32; 4], Option<Arc<str>>, u32);
fn key(l: &Link) -> LinkKey { (l.net.clone(), [l.a.x.to_bits(), l.a.y.to_bits(), l.b.x.to_bits(), l.b.y.to_bits()], l.layer.clone(), l.class) }

fn segment_bounds(a: Point, b: Point) -> Box2 { Box2 { left: a.x.min(b.x), right: a.x.max(b.x), top: a.y.min(b.y), bottom: a.y.max(b.y) } }
fn apart(a: &Box2, b: &Box2, margin: f32) -> bool {
    a.right + margin < b.left || b.right + margin < a.left || a.bottom + margin < b.top || b.bottom + margin < a.top
}
fn expand(b: &Box2, d: f32) -> Box2 { Box2 { left: b.left - d, right: b.right + d, top: b.top - d, bottom: b.bottom + d } }
fn union(a: Option<Box2>, b: Box2) -> Option<Box2> {
    Some(match a { None => b, Some(a) => Box2 { left: a.left.min(b.left), right: a.right.max(b.right), top: a.top.min(b.top), bottom: a.bottom.max(b.bottom) } })
}

/// The base plan of one layout with every nonzero term recorded.
pub(crate) struct RouteCache {
    clearance: f32,
    exact: bool,
    obstacles: Vec<RouteObstacle>,
    nets: Vec<PostPlaceNet>,
    /// Sorted obstacle position of each input obstacle, and the sorted input ids.
    position: Vec<u32>,
    sorted: Vec<usize>,
    /// Input obstacles that share net, layer and reference with another one.
    grouped: Vec<bool>,
    /// Input index of each routed net, in name order, and its spanning links.
    order: Vec<usize>,
    segments: Vec<Vec<Segment>>,
    links: Vec<Link>,
    alternatives: Vec<Vec<Alternative>>,
    index: FxHashMap<LinkKey, u32>,
    total: f32,
}

struct View<'a> {
    obstacles: &'a [RouteObstacle],
    sorted: &'a [usize],
    links: &'a [Link],
    clearance: f32,
}
impl View<'_> {
    fn obstacle_term(&self, alt: &Alternative, link: &Link, position: usize) -> f32 {
        let o = &self.obstacles[self.sorted[position]];
        if link.layer.is_some() && o.layer.is_some() && link.layer != o.layer { return 0.0; }
        if o.net.as_deref() == Some(link.net.as_ref()) { return 0.0; }
        if apart(&alt.bounds, &o.box_, self.clearance + crate::geometry_policy::epsilon() + MARGIN) { return 0.0; }
        arithmetic::obstacle(alt.path, o.box_.left, o.box_.right, o.box_.top, o.box_.bottom, self.clearance, crate::relation_score::pad_factor(link.class))
    }
    fn link_term(&self, alt: &Alternative, link: &Link, other: usize) -> f32 {
        let l = &self.links[other];
        if l.net == link.net || !(link.layer.is_none() || l.layer.is_none() || l.layer == link.layer) { return 0.0; }
        if apart(&alt.bounds, &l.bounds, MARGIN) { return 0.0; }
        arithmetic::crossing(alt.path, P { x: l.a.x, y: l.a.y }, P { x: l.b.x, y: l.b.y }, crate::relation_score::crossing_factor(link.class, l.class))
    }
    fn full(&self, link: &Link) -> Vec<Alternative> {
        alternatives(link.a, link.b, self.clearance).into_iter().map(|points| {
            let excess = excess(&points, link.a, link.b);
            let at = |i: usize| { let p = points[i.min(points.len() - 1)]; P { x: p.x, y: p.y } };
            let bounds = points.iter().fold(None, |b, p| union(b, segment_bounds(*p, *p))).unwrap();
            let mut alt = Alternative { path: Path { a: at(0), b: at(1), c: at(2), d: at(3), count: points.len() as u32 }, bounds, excess, obstacles: vec![], links: vec![] };
            alt.obstacles = (0..self.sorted.len()).filter_map(|s| { let t = self.obstacle_term(&alt, link, s); (t != 0.0).then_some((s as u32, t)) }).collect();
            alt.links = (0..self.links.len()).filter_map(|j| { let t = self.link_term(&alt, link, j); (t != 0.0).then_some((j as u32, t)) }).collect();
            alt
        }).collect()
    }
}
fn cost(alt: &Alternative) -> f32 {
    let mut cost = alt.excess;
    for (_, t) in &alt.obstacles { cost += t; }
    for (_, t) in &alt.links { cost += t; }
    cost
}
fn value(alternatives: &[Alternative]) -> f32 { alternatives.iter().map(cost).reduce(f32::min).unwrap() }

fn links_of(nets: &[&PostPlaceNet], segments: &[Vec<Segment>], explicit: &[ExplicitConnection]) -> Vec<Link> {
    route_links(nets, segments.iter().cloned(), explicit).into_iter().map(|(net, (a, b))| Link {
        layer: link_layer(nets, explicit, &net, a, b), class: explicit_class(&net, a, b, explicit), bounds: segment_bounds(a, b), net, a, b,
    }).collect()
}

impl RouteCache {
    pub(crate) fn new(problem: &PostPlaceScoreProblem) -> Self {
        let clearance = problem.route_clearance;
        let obstacles = problem.routing_obstacles.clone();
        let sorted = sorted_obstacles(&obstacles);
        let mut position = vec![u32::MAX; obstacles.len()];
        for (s, &i) in sorted.iter().enumerate() { position[i] = s as u32; }
        let mut groups: FxHashMap<(Option<Arc<str>>, Option<Arc<str>>, Option<Arc<str>>), usize> = FxHashMap::default();
        for o in &obstacles { *groups.entry((o.net.clone(), o.layer.clone(), o.reference.clone())).or_default() += 1; }
        let grouped: Vec<bool> = obstacles.iter().map(|o| groups[&(o.net.clone(), o.layer.clone(), o.reference.clone())] > 1).collect();
        // A dropped duplicate shares its retained twin's box, hence its position.
        for (i, o) in obstacles.iter().enumerate() {
            if position[i] == u32::MAX {
                position[i] = sorted.iter().position(|&j| obstacles[j].reference == o.reference && obstacles[j].net == o.net && obstacles[j].layer == o.layer
                    && obstacles[j].box_ == o.box_).map_or(u32::MAX, |s| s as u32);
            }
        }
        let routed = route_nets(&problem.nets, &[]);
        let order: Vec<usize> = routed.iter().map(|n| problem.nets.iter().position(|m| std::ptr::eq(m, *n)).unwrap()).collect();
        let segments: Vec<_> = routed.iter().map(|n| spanning_segments(n.points.clone())).collect();
        let explicit = sorted_explicit(&problem.explicit_connections);
        let links = links_of(&routed, &segments, &explicit);
        let view = View { obstacles: &obstacles, sorted: &sorted, links: &links, clearance };
        let alternatives: Vec<_> = links.iter().map(|l| view.full(l)).collect();
        let total = alternatives.iter().map(|a| value(a)).fold(0.0, |a, b| a + b);
        let mut index = FxHashMap::default();
        let mut unique = true;
        for (i, l) in links.iter().enumerate() { unique &= index.insert(key(l), i as u32).is_none(); }
        // The recorded terms must reproduce the full plan exactly.
        let exact = unique && !position.contains(&u32::MAX) && total.to_bits() == crate::post_place::route_plan(problem).cpu().to_bits();
        Self { clearance, exact, obstacles, nets: problem.nets.clone(), position, sorted, grouped, order, segments, links, alternatives, index, total }
    }
    pub(crate) fn exact(&self) -> bool { self.exact }

    /// `route_plan(problem).cpu()` for a problem of the same structure.
    pub(crate) fn evaluate(&self, problem: &PostPlaceScoreProblem) -> f32 {
        self.incremental(problem).unwrap_or_else(|| crate::post_place::route_plan(problem).cpu())
    }

    fn incremental(&self, problem: &PostPlaceScoreProblem) -> Option<f32> {
        if !self.exact || problem.route_clearance.to_bits() != self.clearance.to_bits()
            || problem.routing_obstacles.len() != self.obstacles.len() || problem.nets.len() != self.nets.len() { return None; }
        // Obstacles whose boxes changed; their sorted positions must be stable.
        let mut changed = Vec::new();
        for (i, (o, base)) in problem.routing_obstacles.iter().zip(&self.obstacles).enumerate() {
            if o.net != base.net || o.layer != base.layer || o.reference != base.reference { return None; }
            if o.box_ != base.box_ { changed.push(i); }
        }
        if changed.iter().any(|&i| self.grouped[i]) && sorted_obstacles(&problem.routing_obstacles) != self.sorted { return None; }
        let mut positions: Vec<u32> = changed.iter().map(|&i| self.position[i]).collect();
        positions.sort_unstable();
        positions.dedup();
        // Nets whose points or sides changed get new spanning links.
        let mut segments = self.segments.clone();
        for (k, &i) in self.order.iter().enumerate() {
            let (net, base) = (&problem.nets[i], &self.nets[i]);
            if net.name != base.name { return None; }
            if net.points.len() != base.points.len() || net.layers != base.layers
                || net.points.iter().zip(&base.points).any(|(p, q)| p.x.to_bits() != q.x.to_bits() || p.y.to_bits() != q.y.to_bits()) {
                segments[k] = spanning_segments(net.points.clone());
            }
        }
        let routed: Vec<&PostPlaceNet> = self.order.iter().map(|&i| &problem.nets[i]).collect();
        if routed.iter().zip(route_nets(&problem.nets, &[])).any(|(a, b)| !std::ptr::eq(*a, b)) { return None; }
        let explicit = sorted_explicit(&problem.explicit_connections);
        let links = links_of(&routed, &segments, &explicit);
        let view = View { obstacles: &problem.routing_obstacles, sorted: &self.sorted, links: &links, clearance: self.clearance };
        // Base link for each new link, and the new position of each base link.
        let matched: Vec<Option<u32>> = links.iter().map(|l| self.index.get(&key(l)).copied()).collect();
        let mut moved_to = vec![u32::MAX; self.links.len()];
        for (j, m) in matched.iter().enumerate() {
            if let Some(i) = m { if moved_to[*i as usize] != u32::MAX { return None; } moved_to[*i as usize] = j as u32; }
        }
        let fresh: Vec<usize> = (0..links.len()).filter(|&j| matched[j].is_none()).collect();
        let changed_area = positions.iter().fold(None, |b, &s| union(b, problem.routing_obstacles[self.sorted[s as usize]].box_))
            .map(|b| expand(&b, self.clearance + crate::geometry_policy::epsilon() + MARGIN));
        let fresh_area = fresh.iter().fold(None, |b, &j| union(b, links[j].bounds)).map(|b| expand(&b, MARGIN));
        let mut total = 0.0;
        for (j, link) in links.iter().enumerate() {
            let Some(base) = matched[j] else { total += value(&view.full(link)); continue };
            let alternatives: Vec<Alternative> = self.alternatives[base as usize].iter().map(|alt| {
                let mut next = Alternative { obstacles: Vec::new(), links: Vec::new(), ..alt.clone() };
                if changed_area.is_some_and(|area| !apart(&alt.bounds, &area, 0.0)) {
                    let mut terms: Vec<(u32, f32)> = alt.obstacles.iter().filter(|(s, _)| positions.binary_search(s).is_err()).copied().collect();
                    terms.extend(positions.iter().filter_map(|&s| { let t = view.obstacle_term(alt, link, s as usize); (t != 0.0).then_some((s, t)) }));
                    terms.sort_unstable_by_key(|(s, _)| *s);
                    next.obstacles = terms;
                } else { next.obstacles = alt.obstacles.iter().filter(|(s, _)| positions.binary_search(s).is_err()).copied().collect(); }
                let mut terms: Vec<(u32, f32)> = alt.links.iter().filter_map(|&(i, t)| (moved_to[i as usize] != u32::MAX).then_some((moved_to[i as usize], t))).collect();
                if fresh_area.is_some_and(|area| !apart(&alt.bounds, &area, 0.0)) {
                    terms.extend(fresh.iter().filter_map(|&f| { let t = view.link_term(alt, link, f); (t != 0.0).then_some((f as u32, t)) }));
                }
                terms.sort_unstable_by_key(|(i, _)| *i);
                next.links = terms;
                next
            }).collect();
            total += value(&alternatives);
        }
        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn problem() -> PostPlaceScoreProblem {
        let net = |name: &str, points: &[(f32, f32)]| PostPlaceNet { name: name.into(), points: points.iter().map(|&(x, y)| Point { x, y }).collect(),
            layers: vec![Some("top".into()); points.len()], internal_owners: vec![], local_scopes: vec![], weight: 1.0 };
        let pad = |r: &str, net: &str, x: f32, y: f32| RouteObstacle { box_: Box2 { left: x - 0.3, right: x + 0.3, top: y - 0.3, bottom: y + 0.3 },
            layer: Some("top".into()), reference: Some(r.into()), net: Some(net.into()), primitive_id: None };
        PostPlaceScoreProblem { route_clearance: 0.2, access_components: vec![], explicit_connections: vec![], version: 1,
            routing_obstacles: vec![pad("A.1", "S1", 0.0, 0.0), pad("B.1", "S1", 9.0, 0.0), pad("C.1", "S2", 4.5, -3.0), pad("D.1", "S2", 4.5, 3.0),
                pad("E.1", "S3", 2.0, 1.0), pad("E.2", "S3", 7.0, 1.0)],
            nets: vec![net("S1", &[(0.0, 0.0), (9.0, 0.0)]), net("S2", &[(4.5, -3.0), (4.5, 3.0)]), net("S3", &[(2.0, 1.0), (7.0, 1.0)])],
            distances: vec![], clearances: vec![], fixed_penalties: vec![], edges: vec![], paths: vec![] }
    }
    fn translate(p: &mut PostPlaceScoreProblem, reference: &str, dx: f32, dy: f32) {
        for o in p.routing_obstacles.iter_mut().filter(|o| o.reference.as_deref().is_some_and(|r| r.starts_with(reference))) {
            let (x, y) = ((o.box_.left + o.box_.right) / 2.0, (o.box_.top + o.box_.bottom) / 2.0);
            for net in &mut p.nets { for q in &mut net.points { if q.x == x && q.y == y { q.x += dx; q.y += dy; } } }
            o.box_ = Box2 { left: o.box_.left + dx, right: o.box_.right + dx, top: o.box_.top + dy, bottom: o.box_.bottom + dy };
        }
    }
    #[test]
    fn candidates_match_the_full_plan_bit_for_bit() {
        let _env = crate::float_env::Guard::enter();
        let base = problem();
        let cache = RouteCache::new(&base);
        assert!(cache.exact());
        assert!(cache.total > 0.0);
        for (reference, dx, dy) in [("E", 0.0, -2.0), ("E", 0.5, 3.0), ("C", 1.0, 0.0), ("A", 0.0, 0.25), ("E", 40.0, 40.0)] {
            let mut candidate = base.clone();
            translate(&mut candidate, reference, dx, dy);
            let full = crate::post_place::route_plan(&candidate).cpu();
            assert_eq!(cache.incremental(&candidate).map(f32::to_bits), Some(full.to_bits()), "{reference} {dx} {dy}");
        }
    }
}
