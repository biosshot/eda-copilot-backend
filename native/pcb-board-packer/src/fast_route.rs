//! Bounded geometric routing proxy, not a connectivity proof. No A* and no vias
//! invented from line intersections. Length already exists in the block objective;
//! this module charges only detour, foreign pads, clearance bands and crossings.
use crate::geometry::{Box2, Point};
use crate::model::{Primitive, RouteObstacle};
use crate::net_class::is_ground;
use std::sync::Arc;

type Segment = (Point, Point);

pub fn candidate_penalty(
    candidate: &Primitive,
    placed: &[&Primitive],
    obstacles: &[RouteObstacle],
    external: &[Arc<str>],
    ignored: &[Arc<str>],
    clearance: f64,
    bounds: Box2,
) -> f64 {
    let eligible = |n: &str| {
        !n.is_empty() && !is_ground(n) && !ignored.iter().any(|s| s.eq_ignore_ascii_case(n))
    };
    let mut nets: Vec<_> = candidate
        .connection_points
        .iter()
        .filter_map(|p| p.net.clone())
        .filter(|n| eligible(n))
        .collect();
    nets.sort();
    nets.dedup();
    // Existing connection skeleton is only a crossing hint. Never charge all-pairs
    // bus length; grow a deterministic nearest-edge spanning tree per net.
    let skeleton = skeleton(placed, &eligible);
    let mut total = 0.0;
    for net in nets {
        let sources: Vec<_> = candidate
            .connection_points
            .iter()
            .filter(|p| p.net.as_ref() == Some(&net))
            .collect();
        let mut pairs = Vec::new();
        for source in &sources {
            for p in placed {
                for target in p
                    .connection_points
                    .iter()
                    .filter(|p| p.net.as_ref() == Some(&net))
                {
                    let a = Point {
                        x: source.x,
                        y: source.y,
                    };
                    let b = Point {
                        x: target.x,
                        y: target.y,
                    };
                    pairs.push((
                        distance(a, b),
                        a,
                        b,
                        common_layer(&source.reference, &target.reference, obstacles),
                    ));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        if let Some(best) = pairs
            .iter()
            .take(4)
            .map(|(_, a, b, layer)| {
                path_cost(
                    *a,
                    *b,
                    &net,
                    obstacles,
                    &skeleton,
                    clearance,
                    layer.as_deref(),
                )
            })
            .reduce(f64::min)
        {
            total += best;
        }
        if external.contains(&net) {
            // Keep an escape to some side of the block; actual neighbour direction
            // becomes known only to board-level portfolio selection.
            let escape = sources
                .iter()
                .flat_map(|p| {
                    let a = Point { x: p.x, y: p.y };
                    [
                        Point {
                            x: bounds.left,
                            y: p.y,
                        },
                        Point {
                            x: bounds.right,
                            y: p.y,
                        },
                        Point {
                            x: p.x,
                            y: bounds.top,
                        },
                        Point {
                            x: p.x,
                            y: bounds.bottom,
                        },
                    ]
                    .map(|b| {
                        path_cost(
                            a,
                            b,
                            &net,
                            obstacles,
                            &skeleton,
                            clearance,
                            pad_layer(&p.reference, obstacles),
                        ) + distance(a, b) * 0.25
                    })
                })
                .reduce(f64::min)
                .unwrap_or(0.0);
            total += escape * 0.5;
        }
    }
    total
}

fn skeleton(
    primitives: &[&Primitive],
    eligible: &impl Fn(&str) -> bool,
) -> Vec<(Arc<str>, Segment)> {
    let mut nets = std::collections::BTreeMap::<Arc<str>, Vec<Point>>::new();
    for p in primitives {
        for cp in p.connection_points.iter() {
            if let Some(n) = &cp.net {
                if eligible(n) {
                    nets.entry(n.clone())
                        .or_default()
                        .push(Point { x: cp.x, y: cp.y });
                }
            }
        }
    }
    let mut result = Vec::new();
    for (n, mut pts) in nets {
        pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        pts.dedup_by(|a, b| distance(*a, *b) < 1e-6);
        if pts.len() < 2 || pts.len() > 32 {
            continue;
        }
        let mut used = vec![false; pts.len()];
        used[0] = true;
        for _ in 1..pts.len() {
            let mut best = (f64::INFINITY, 0, 0);
            for a in 0..pts.len() {
                if !used[a] {
                    continue;
                }
                for b in 0..pts.len() {
                    if used[b] {
                        continue;
                    }
                    let d = distance(pts[a], pts[b]);
                    if d < best.0 {
                        best = (d, a, b);
                    }
                }
            }
            used[best.2] = true;
            result.push((n.clone(), (pts[best.1], pts[best.2])));
        }
    }
    result
}

fn pad_layer<'a>(reference: &str, obstacles: &'a [RouteObstacle]) -> Option<&'a str> {
    obstacles
        .iter()
        .find(|p| p.reference.as_deref() == Some(reference))
        .and_then(|p| p.layer.as_deref())
}

fn common_layer(a: &str, b: &str, obstacles: &[RouteObstacle]) -> Option<Arc<str>> {
    match (pad_layer(a, obstacles), pad_layer(b, obstacles)) {
        (Some(a), Some(b)) if a == b => Some(Arc::from(a)),
        (Some(a), None) | (None, Some(a)) => Some(Arc::from(a)),
        _ => None, // Mixed-side or through-hole pair: conservative projection.
    }
}

fn path_cost(
    a: Point,
    b: Point,
    net: &str,
    obstacles: &[RouteObstacle],
    skeleton: &[(Arc<str>, Segment)],
    clearance: f64,
    layer: Option<&str>,
) -> f64 {
    let middle = Point {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    };
    // Direct, both Ls and four doglegs. Constant-size search, no grid expansion.
    let gap = clearance.max(0.25) * 2.0;
    let mut paths = vec![
        vec![a, b],
        vec![a, Point { x: a.x, y: b.y }, b],
        vec![a, Point { x: b.x, y: a.y }, b],
    ];
    for d in [-gap, gap] {
        paths.push(vec![
            a,
            Point {
                x: middle.x + d,
                y: a.y,
            },
            Point {
                x: middle.x + d,
                y: b.y,
            },
            b,
        ]);
        paths.push(vec![
            a,
            Point {
                x: a.x,
                y: middle.y + d,
            },
            Point {
                x: b.x,
                y: middle.y + d,
            },
            b,
        ]);
    }
    paths
        .iter()
        .map(|path| {
            let mut cost = (path.windows(2).map(|s| distance(s[0], s[1])).sum::<f64>()
                - distance(a, b))
            .max(0.0);
            for obstacle in obstacles {
                if layer.is_some() && obstacle.layer.is_some() && layer != obstacle.layer.as_deref()
                {
                    continue;
                }
                if obstacle.net.as_deref() == Some(net) {
                    continue;
                }
                if path.windows(2).any(|s| hits(s[0], s[1], obstacle.box_)) {
                    cost += 20.0;
                } else {
                    let q = obstacle.box_;
                    let expanded = Box2 {
                        left: q.left - clearance,
                        right: q.right + clearance,
                        top: q.top - clearance,
                        bottom: q.bottom + clearance,
                    };
                    if path.windows(2).any(|s| hits(s[0], s[1], expanded)) {
                        cost += 2.0;
                    }
                }
            }
            for (other, (c, d)) in skeleton {
                if other.as_ref() != net && path.windows(2).any(|s| crosses(s[0], s[1], *c, *d)) {
                    cost += 3.0;
                }
            }
            cost
        })
        .reduce(f64::min)
        .unwrap_or(0.0)
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn crosses(a: Point, b: Point, c: Point, d: Point) -> bool {
    let orient =
        |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    orient(a, b, c) * orient(a, b, d) < -1e-9 && orient(c, d, a) * orient(c, d, b) < -1e-9
}
pub(crate) fn hits(a: Point, b: Point, q: Box2) -> bool {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for (start, delta, min, max) in [
        (a.x, b.x - a.x, q.left, q.right),
        (a.y, b.y - a.y, q.top, q.bottom),
    ] {
        if delta.abs() < 1e-9 {
            if start < min || start > max {
                return false;
            }
        } else {
            let t1 = (min - start) / delta;
            let t2 = (max - start) / delta;
            lo = lo.max(t1.min(t2));
            hi = hi.min(t1.max(t2));
            if lo > hi {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segment_box_handles_vertical_and_inside() {
        let q = Box2 {
            left: 0.,
            right: 1.,
            top: 0.,
            bottom: 1.,
        };
        assert!(hits(Point { x: 0.5, y: -1. }, Point { x: 0.5, y: 2. }, q));
        assert!(!hits(Point { x: 2., y: -1. }, Point { x: 2., y: 2. }, q));
        assert!(hits(Point { x: 0.5, y: 0.5 }, Point { x: 0.6, y: 0.6 }, q));
    }
    #[test]
    fn foreign_pad_cost_but_same_net_is_free() {
        let a = Point { x: 0., y: 0. };
        let b = Point { x: 10., y: 0. };
        let obstacle = RouteObstacle {
            box_: Box2 {
                left: 4.,
                right: 6.,
                top: -2.,
                bottom: 2.,
            },
            layer: None,
            reference: None,
            net: Some(Arc::from("OTHER")),
            primitive_id: None,
        };
        assert!(path_cost(a, b, "SIG", &[obstacle.clone()], &[], 0.25, None) > 0.0);
        assert_eq!(
            path_cost(a, b, "OTHER", &[obstacle.clone()], &[], 0.25, None),
            0.0
        );
        let bottom = RouteObstacle {
            layer: Some(Arc::from("bottom")),
            ..obstacle
        };
        assert_eq!(
            path_cost(a, b, "SIG", &[bottom], &[], 0.25, Some("top")),
            0.0
        );
    }
}
