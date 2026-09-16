use crate::geometry::Point;
use crate::model::{Primitive, Relation};
use std::collections::{BTreeMap, HashMap};

pub const MARKER_PREFIX: &str = "__ordinary_net__:";
pub const ORDINARY_NET_SCALE: f64 = 0.5;
pub const ORDINARY_PAIR_AFFINITY_CAP: f64 = 4.0;
pub const ORDINARY_DISTANCE_CAP_MM: f64 = 30.0;
pub const ORDINARY_MAX_FANOUT: usize = 8;

#[derive(Clone, Copy, Default)]
struct PairScore {
    affinity: f64,
    min_distance: f64,
}

/// Weak board-level electrical affinity. Marker relations are produced by the
/// TypeScript board adapter after GND/ignored/high-fanout filtering and carry
/// the normal (1.0) or power (0.1) net weight. They intentionally have
/// unresolvable endpoints so the ordinary relation scorer never treats them as
/// explicit placement constraints.
pub fn penalty(primitives: &[&Primitive], relations: &[Relation]) -> f64 {
    let markers: BTreeMap<&str, f64> = relations
        .iter()
        .filter_map(|relation| {
            let net = relation.from.strip_prefix(MARKER_PREFIX)?;
            if relation.to.as_ref() != relation.from.as_ref() {
                return None;
            }
            let weight = relation.weight.unwrap_or(1.0);
            (weight.is_finite() && weight > 0.0).then_some((net, weight))
        })
        .collect();
    if markers.is_empty() {
        return 0.0;
    }

    let mut pairs: HashMap<(usize, usize), PairScore> = HashMap::new();
    for (net, net_weight) in markers {
        let mut by_primitive: Vec<(usize, Vec<Point>)> = Vec::new();
        for (primitive_index, primitive) in primitives.iter().enumerate() {
            let points: Vec<_> = primitive
                .connection_points
                .iter()
                .filter(|point| point.net.as_deref() == Some(net))
                .map(|point| Point { x: point.x, y: point.y })
                .collect();
            if !points.is_empty() {
                by_primitive.push((primitive_index, points));
            }
        }
        let count = by_primitive.len();
        if count < 2 || count > ORDINARY_MAX_FANOUT {
            continue;
        }
        let contribution = net_weight / (count as f64 - 1.0);
        for a in 0..count {
            for b in (a + 1)..count {
                let key = (by_primitive[a].0, by_primitive[b].0);
                let distance = shortest_point_distance(&by_primitive[a].1, &by_primitive[b].1);
                let entry = pairs.entry(key).or_insert(PairScore {
                    affinity: 0.0,
                    min_distance: f64::INFINITY,
                });
                entry.affinity = (entry.affinity + contribution).min(ORDINARY_PAIR_AFFINITY_CAP);
                entry.min_distance = entry.min_distance.min(distance);
            }
        }
    }

    pairs
        .values()
        .filter(|pair| pair.affinity > 0.0 && pair.min_distance.is_finite())
        .map(|pair| pair.min_distance.min(ORDINARY_DISTANCE_CAP_MM) * pair.affinity * ORDINARY_NET_SCALE)
        .sum()
}

fn shortest_point_distance(a: &[Point], b: &[Point]) -> f64 {
    let mut best = f64::INFINITY;
    for left in a {
        for right in b {
            best = best.min((left.x - right.x).hypot(left.y - right.y));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Box2;
    use crate::model::{ConnectionPoint, Placement};
    use std::sync::Arc;

    fn primitive(id: &str, x: f64, nets: &[&str]) -> Primitive {
        Primitive {
            id: Arc::from(id),
            kind: Arc::from("block"),
            label: Arc::from(id),
            source_node_id: Arc::from(id),
            source_node_ids: Arc::new(vec![Arc::from(id)]),
            locked: false,
            can_rotate: false,
            allowed_orientations: Arc::new(vec![0]),
            bbox: Box2 { left: x - 0.5, right: x + 0.5, top: -0.5, bottom: 0.5 },
            collision_boxes: Arc::new(vec![]),
            width: 1.0,
            height: 1.0,
            placements: Arc::new(vec![Placement {
                designator: Arc::from(id), x, y: 0.0, rotate: 0, layer: Arc::from("top"), score: 0.0,
            }]),
            connection_points: Arc::new(nets.iter().enumerate().map(|(index, net)| ConnectionPoint {
                x,
                y: index as f64 * 0.1,
                reference: Arc::from(format!("{id}.{}", index + 1)),
                net: Some(Arc::from(*net)),
            }).collect()),
            path_ports: Arc::new(vec![]),
            edge_place: None,
        }
    }

    fn marker(net: &str, weight: f64) -> Relation {
        Relation {
            id: Arc::from(format!("{MARKER_PREFIX}{net}")),
            kind: Arc::from("net"),
            from: Arc::from(format!("{MARKER_PREFIX}{net}")),
            to: Arc::from(format!("{MARKER_PREFIX}{net}")),
            relation: None,
            priority: None,
            hard: false,
            weight: Some(weight),
            effect: Arc::from("score_only"),
            max_distance: None,
            min_distance: None,
            satellite_anchor: false,
            anchor_offset: None,
            side_preference: None,
            path_id: None,
            path_shape: None,
            prefer_facing_pads: false,
        }
    }

    #[test]
    fn normal_net_prefers_near_over_far() {
        let near_a = primitive("A", 0.0, &["SIG"]);
        let near_b = primitive("B", 2.0, &["SIG"]);
        let far_b = primitive("B", 10.0, &["SIG"]);
        let relation = marker("SIG", 1.0);
        assert!(penalty(&[&near_a, &near_b], &[relation.clone()]) < penalty(&[&near_a, &far_b], &[relation]));
    }

    #[test]
    fn power_is_ten_times_weaker_than_signal() {
        let a = primitive("A", 0.0, &["SIG", "3V3"]);
        let b = primitive("B", 10.0, &["SIG", "3V3"]);
        let normal = penalty(&[&a, &b], &[marker("SIG", 1.0)]);
        let power = penalty(&[&a, &b], &[marker("3V3", 0.1)]);
        assert!((power * 10.0 - normal).abs() < 1e-9);
    }

    #[test]
    fn absent_marker_contributes_nothing() {
        let a = primitive("A", 0.0, &["GND"]);
        let b = primitive("B", 10.0, &["GND"]);
        assert_eq!(penalty(&[&a, &b], &[]), 0.0);
    }

    #[test]
    fn fanout_above_eight_is_ignored() {
        let primitives: Vec<_> = (0..9).map(|index| primitive(&format!("P{index}"), index as f64, &["BUS"])).collect();
        let refs: Vec<_> = primitives.iter().collect();
        assert_eq!(penalty(&refs, &[marker("BUS", 1.0)]), 0.0);
    }

    #[test]
    fn parallel_nets_are_capped_at_four() {
        let nets: Vec<String> = (0..20).map(|index| format!("D{index}" )).collect();
        let net_refs: Vec<_> = nets.iter().map(String::as_str).collect();
        let a = primitive("A", 0.0, &net_refs);
        let b = primitive("B", 10.0, &net_refs);
        let markers: Vec<_> = net_refs.iter().map(|net| marker(net, 1.0)).collect();
        assert!((penalty(&[&a, &b], &markers) - 20.0).abs() < 1e-9);
    }
}
