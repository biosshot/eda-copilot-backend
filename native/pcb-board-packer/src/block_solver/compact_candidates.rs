use super::*;
#[derive(Clone, Copy)]
pub(super) struct Compact {
    pub pose: Pose,
    pub bbox: Box2,
}
fn translate_primitive(p: &WorkingPrimitive, dx: f32, dy: f32) -> Compact {
    Compact {
        pose: Pose {
            template_index: (p.source_index * 4 + normalize_rotation(p.rotation) as usize / 90)
                as u32,
            ordinal: 0,
            dx,
            dy,
        },
        bbox: translate_box(&p.primitive.bbox, dx, dy),
    }
}
fn move_primitive_center_exact(p: &WorkingPrimitive, center: Point) -> Compact {
    let origin = box_center(&p.primitive.bbox);
    translate_primitive(
        p,
        round_placement(center.x - origin.x),
        round_placement(center.y - origin.y),
    )
}
fn move_primitive_center(p: &WorkingPrimitive, center: Point, grid: f32) -> Compact {
    let origin = box_center(&p.primitive.bbox);
    translate_primitive(
        p,
        snap(center.x - origin.x, grid),
        snap(center.y - origin.y, grid),
    )
}
fn move_box_center_exact(p: &WorkingPrimitive, b: &Box2, center: Point) -> Compact {
    let origin = box_center(b);
    translate_primitive(
        p,
        round_placement(center.x - origin.x),
        round_placement(center.y - origin.y),
    )
}
fn move_box_center(p: &WorkingPrimitive, b: &Box2, center: Point, grid: f32) -> Compact {
    let origin = box_center(b);
    translate_primitive(
        p,
        snap(center.x - origin.x, grid),
        snap(center.y - origin.y, grid),
    )
}
fn fit_to_bounds(p: Compact, c: &Context) -> Compact {
    assert!(c.problem.bounds.is_none());
    p
}
fn dedupe_primitives(v: Vec<Compact>) -> Vec<Compact> {
    let mut seen = FxHashSet::default();
    v.into_iter()
        .filter(|p| {
            seen.insert((
                p.pose.template_index,
                number_key(p.bbox.left),
                number_key(p.bbox.top),
                number_key(p.bbox.right),
                number_key(p.bbox.bottom),
            ))
        })
        .collect()
}
pub(super) fn generate(
    p: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    c: &Context,
) -> Vec<Pose> {
    if p.primitive.locked {return vec![Pose{template_index:(p.source_index*4+normalize_rotation(p.rotation) as usize/90) as u32,ordinal:0,dx:0.0,dy:0.0}];}
    if c.problem.bounds.is_some() {
        // Reuse the reference bounded generator, including its board fallback.
        // The compact path remains allocation-light for ordinary unbounded blocks.
        return super::super::block_candidates(p,placed,c).into_iter().enumerate().map(|(ordinal,q)| {
            let rotated=rotate_primitive(p,normalize_rotation(q.rotation-p.rotation));
            let a=box_center(&rotated.primitive.bbox);let b=box_center(&q.primitive.bbox);
            Pose{template_index:(q.source_index*4+normalize_rotation(q.rotation) as usize/90) as u32,ordinal:ordinal as u32,dx:round_placement(b.x-a.x),dy:round_placement(b.y-a.y)}
        }).collect();
    }
    let variants = orientation_variants(p);
    let candidates = if placed.is_empty() {
        variants
            .iter()
            .map(|v| move_primitive_center(v, Point { x: 0., y: 0. }, c.problem.grid))
            .collect()
    } else {
        variants
            .iter()
            .flat_map(|v| block_candidates_for_orientation(v, placed, c))
            .collect()
    };
    dedupe_primitives(candidates)
        .into_iter()
        .enumerate()
        .map(|(i, p)| Pose {
            ordinal: i as u32,
            ..p.pose
        })
        .collect()
}
fn block_candidates_for_orientation(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Compact> {
    let mut anchors: Vec<(Box2, f32)> = placed
        .iter()
        .flat_map(|item| {
            primitive_candidate_boxes(item, context)
                .into_iter()
                .map(move |b| (b, candidate_clearance(primitive, item, context)))
        })
        .collect();
    let union_clearance = anchors.iter().map(|(_, c)| *c).fold(0.0, f32::max);
    anchors.push((
        union_boxes(
            &placed
                .iter()
                .map(|item| item.primitive.bbox)
                .collect::<Vec<_>>(),
        ),
        union_clearance,
    ));
    let mut candidates = Vec::new();
    for (anchor, clearance) in anchors {
        let centers = adjacent_centers(
            primitive.primitive.width,
            primitive.primitive.height,
            &anchor,
            clearance,
        );
        for center in centers {
            candidates.push(fit_to_bounds(
                move_primitive_center_exact(primitive, center),
                context,
            ));
            candidates.push(fit_to_bounds(
                move_primitive_center(primitive, center, context.problem.grid),
                context,
            ));
        }
        if context.problem.candidate_box_mode.as_ref() == "collision" {
            for moving_box in primitive_candidate_boxes(primitive, context) {
                candidates.extend(box_anchored_candidates(
                    primitive,
                    &moving_box,
                    &anchor,
                    clearance,
                    context,
                ));
            }
        }
    }
    context.detail.count("candidate_body_raw", candidates.len());
    let relations = relation_anchored_candidates(primitive, placed, context);
    context
        .detail
        .count("candidate_relation_raw", relations.len());
    candidates.extend(relations);
    if context.problem.experiments.net_candidates {
        let nets = net_anchored_candidates(primitive, placed, context);
        context.detail.count("candidate_net_raw", nets.len());
        candidates.extend(nets);
    }
    let before_bridges = candidates.len();
    let placed_primitives: Vec<_> = placed.iter().map(|item| &item.primitive).collect();
    for delta in signal_path::bridge_deltas(&primitive.primitive, &placed_primitives) {
        candidates.push(fit_to_bounds(
            translate_primitive(primitive, delta.x, delta.y),
            context,
        ));
        candidates.push(fit_to_bounds(
            translate_primitive(
                primitive,
                snap(delta.x, context.problem.grid),
                snap(delta.y, context.problem.grid),
            ),
            context,
        ));
    }
    context
        .detail
        .count("candidate_bridge_raw", candidates.len() - before_bridges);
    context
        .detail
        .count("candidate_total_raw", candidates.len());
    let unique = dedupe_primitives(candidates);
    context
        .detail
        .count("candidate_per_orientation_unique", unique.len());
    unique
}

fn net_anchored_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Compact> {
    let mut candidates = Vec::new();
    // Fixed ordering and bounded anchors per pad keep high-fanout supply nets cheap.
    for moving in primitive.primitive.connection_points.iter() {
        let Some(net) = &moving.net else { continue };
        if net.is_empty()
            || is_ground(net)
            || context
                .problem
                .experiments
                .ignored_nets
                .iter()
                .any(|n| n.eq_ignore_ascii_case(net))
        {
            continue;
        }
        let mut anchors = Vec::new();
        for item in placed {
            for target in item.primitive.connection_points.iter() {
                if target.net.as_ref() == Some(net) {
                    anchors.push((item, target));
                }
            }
        }
        anchors.sort_by(|(a, ap), (b, bp)| a.id.cmp(&b.id).then(ap.reference.cmp(&bp.reference)));
        let anchor_limit = if context.problem.experiments.pad_owner_candidates {
            usize::MAX
        } else {
            4
        };
        for (item, target) in anchors.into_iter().take(anchor_limit) {
            context.detail.count("candidate_net_pad_pairs", 1);
            let center = box_center(&primitive.primitive.bbox);
            let dx = moving.x - center.x;
            let dy = moving.y - center.y;
            // A compound's empty envelope must not hide space next to its IC.
            // Hard legality still checks every actual component in the island.
            let owner = target.reference.split_once('.').map(|(name, _)| name);
            let b = if context.problem.experiments.pad_owner_candidates
                && context.problem.hard_collision_mode.as_ref() == "components"
            {
                item.components
                    .iter()
                    .find(|(_, c)| Some(c.designator.as_ref()) == owner)
                    .map(|(_, c)| c.body_box)
                    .unwrap_or(item.primitive.bbox)
            } else {
                item.primitive.bbox
            };
            let clearance = candidate_clearance(primitive, item, context);
            let slide = context.problem.grid.max(0.25);
            let expanded = context.problem.experiments.candidate_rings;
            let slides = if expanded {
                vec![0.0, -slide, slide, -2.0 * slide, 2.0 * slide]
            } else {
                vec![0.0, -slide, slide]
            };
            for ring in 0..(if expanded { 3 } else { 1 }) {
                let c = clearance + ring as f32 * slide;
                for &s in &slides {
                    for p in [
                        Point {
                            x: b.left - c - primitive.primitive.width / 2.0,
                            y: target.y - dy + s,
                        },
                        Point {
                            x: b.right + c + primitive.primitive.width / 2.0,
                            y: target.y - dy + s,
                        },
                        Point {
                            x: target.x - dx + s,
                            y: b.top - c - primitive.primitive.height / 2.0,
                        },
                        Point {
                            x: target.x - dx + s,
                            y: b.bottom + c + primitive.primitive.height / 2.0,
                        },
                    ] {
                        candidates.push(fit_to_bounds(
                            move_primitive_center_exact(primitive, p),
                            context,
                        ));
                    }
                }
            }
        }
    }
    candidates
}

fn box_anchored_candidates(
    primitive: &WorkingPrimitive,
    moving_box: &Box2,
    anchor: &Box2,
    clearance: f32,
    context: &Context,
) -> Vec<Compact> {
    adjacent_centers(
        moving_box.right - moving_box.left,
        moving_box.bottom - moving_box.top,
        anchor,
        clearance,
    )
    .into_iter()
    .flat_map(|center| {
        [
            fit_to_bounds(
                move_box_center_exact(primitive, moving_box, center),
                context,
            ),
            fit_to_bounds(
                move_box_center(primitive, moving_box, center, context.problem.grid),
                context,
            ),
        ]
    })
    .collect()
}

fn relation_anchored_candidates(
    primitive: &WorkingPrimitive,
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<Compact> {
    let mut candidates = Vec::new();
    for compiled in &context.relations {
        let relation = &compiled.relation;
        if relation.effect.as_ref() == "lock"
            || relation.kind.as_ref() == "net"
            || relation.effect.as_ref() == "score_only"
        {
            continue;
        }
        let moving_from = endpoint_point(std::slice::from_ref(primitive), &compiled.from);
        let placed_to = endpoint_point(placed, &compiled.to)
            .map(|point| relation_target_point(point, relation));
        if let (Some(moving), Some(target)) = (moving_from, placed_to) {
            let anchor = placed
                .iter()
                .find(|item| Some(item.id) == target.primitive_id)
                .map(|item| item.primitive.bbox)
                .unwrap_or(point_box(target.point));
            candidates.extend(endpoint_anchored_candidates(
                primitive,
                moving.point,
                &anchor,
                target.point,
                relation,
                context,
            ));
        }
        let moving_to = endpoint_point(std::slice::from_ref(primitive), &compiled.to);
        let placed_from = endpoint_point(placed, &compiled.from)
            .map(|point| relation_target_point(point, relation));
        if let (Some(moving), Some(target)) = (moving_to, placed_from) {
            let anchor = placed
                .iter()
                .find(|item| Some(item.id) == target.primitive_id)
                .map(|item| item.primitive.bbox)
                .unwrap_or(point_box(target.point));
            candidates.extend(endpoint_anchored_candidates(
                primitive,
                moving.point,
                &anchor,
                target.point,
                relation,
                context,
            ));
        }
    }
    candidates
}

fn endpoint_anchored_candidates(
    primitive: &WorkingPrimitive,
    moving: Point,
    anchor: &Box2,
    target: Point,
    relation: &Relation,
    context: &Context,
) -> Vec<Compact> {
    let center = box_center(&primitive.primitive.bbox);
    let offset = Point {
        x: moving.x - center.x,
        y: moving.y - center.y,
    };
    let near_distance = context
        .problem
        .clearance
        .max(relation.min_distance.unwrap_or(0.0));
    let size = primitive.primitive.width.max(primitive.primitive.height);
    let slide_step = context
        .problem
        .grid
        .max((size * 0.25).min(relation.max_distance.unwrap_or(size)));
    let slides = dedupe_numbers(vec![
        0.0,
        -slide_step,
        slide_step,
        -slide_step * 2.0,
        slide_step * 2.0,
    ]);
    let c = context.problem.clearance;
    let mut centers = vec![
        Point {
            x: target.x - offset.x - c,
            y: target.y - offset.y,
        },
        Point {
            x: target.x - offset.x + c,
            y: target.y - offset.y,
        },
        Point {
            x: target.x - offset.x,
            y: target.y - offset.y - c,
        },
        Point {
            x: target.x - offset.x,
            y: target.y - offset.y + c,
        },
        Point {
            x: anchor.left - c - primitive.primitive.width / 2.0,
            y: target.y - offset.y,
        },
        Point {
            x: anchor.right + c + primitive.primitive.width / 2.0,
            y: target.y - offset.y,
        },
        Point {
            x: target.x - offset.x,
            y: anchor.top - c - primitive.primitive.height / 2.0,
        },
        Point {
            x: target.x - offset.x,
            y: anchor.bottom + c + primitive.primitive.height / 2.0,
        },
    ];
    for slide in slides {
        centers.extend([
            Point {
                x: anchor.left - c - primitive.primitive.width / 2.0,
                y: target.y - offset.y + slide,
            },
            Point {
                x: anchor.right + c + primitive.primitive.width / 2.0,
                y: target.y - offset.y + slide,
            },
            Point {
                x: target.x - offset.x + slide,
                y: anchor.top - c - primitive.primitive.height / 2.0,
            },
            Point {
                x: target.x - offset.x + slide,
                y: anchor.bottom + c + primitive.primitive.height / 2.0,
            },
        ]);
    }
    if let Some(max_distance) = relation.max_distance {
        if max_distance > near_distance {
            let distances = dedupe_numbers(
                vec![
                    near_distance,
                    max_distance * 0.35,
                    max_distance * 0.6,
                    max_distance * 0.9,
                ]
                .into_iter()
                .filter(|value| *value <= max_distance)
                .collect(),
            );
            let directions = [
                Point { x: 1.0, y: 0.0 },
                Point { x: -1.0, y: 0.0 },
                Point { x: 0.0, y: 1.0 },
                Point { x: 0.0, y: -1.0 },
                Point { x: 1.0, y: 1.0 },
                Point { x: 1.0, y: -1.0 },
                Point { x: -1.0, y: 1.0 },
                Point { x: -1.0, y: -1.0 },
            ]
            .map(normalize);
            for distance in distances {
                for direction in directions {
                    centers.push(Point {
                        x: target.x + direction.x * distance - offset.x,
                        y: target.y + direction.y * distance - offset.y,
                    });
                }
            }
        }
    }
    centers
        .into_iter()
        .flat_map(|center| {
            [
                fit_to_bounds(move_primitive_center_exact(primitive, center), context),
                fit_to_bounds(
                    move_primitive_center(primitive, center, context.problem.grid),
                    context,
                ),
            ]
        })
        .collect()
}

// Same candidates used by CPU frontier_scarcity, without materializing geometry.
pub(super) fn scarcity(p: &WorkingPrimitive, cores: &[WorkingPrimitive], c: &Context) -> Vec<Pose> {
    let candidates = orientation_variants(p)
        .iter()
        .flat_map(|v| net_anchored_candidates(v, cores, c))
        .collect();
    dedupe_primitives(candidates)
        .into_iter()
        .enumerate()
        .map(|(i, p)| Pose {
            ordinal: i as u32,
            ..p.pose
        })
        .collect()
}
