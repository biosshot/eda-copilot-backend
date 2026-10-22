use super::*;
use geo::{
    coordinate_position::CoordPos, dimensions::Dimensions, Contains, Distance, Euclidean, Point,
    PreparedGeometry, Relate,
};

struct Sets(Vec<usize>);
impl Sets {
    fn root(&mut self, i: usize) -> usize {
        if self.0[i] != i {
            self.0[i] = self.root(self.0[i]);
        }
        self.0[i]
    }
    fn join(&mut self, a: usize, b: usize) {
        let a = self.root(a);
        let b = self.root(b);
        self.0[a.max(b)] = a.min(b);
    }
}
pub(super) fn build(
    board: &Board,
    copper: &[Copper],
    bores: &[Bore],
    raw: &[Raw],
    material: &AreaSet,
    possible: bool,
    r: &mut PreparationReport,
) {
    let mut sets = Sets((0..copper.len()).collect());
    let copper_index = spatial::Index::new(
        copper
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.poly.bounding_rect().map(|b| (i, b))),
    );
    let raw_by_source: BTreeMap<_, _> = raw
        .iter()
        .map(|s| ((s.layer, s.source.path.as_str()), s))
        .collect();
    let prepared: Vec<_> = copper
        .iter()
        .map(|c| PreparedGeometry::from(&c.poly))
        .collect();
    let raw_prepared: BTreeMap<_, _> = raw
        .iter()
        .map(|s| {
            (
                (s.layer, s.source.path.as_str()),
                PreparedGeometry::from(&s.shape),
            )
        })
        .collect();
    let mut uncertain = vec![];
    for layer in 0..board.layers.len() {
        let indices: Vec<_> = copper
            .iter()
            .enumerate()
            .filter(|(_, c)| c.layer == layer)
            .map(|(i, _)| i)
            .collect();
        let polys: Vec<_> = indices.iter().map(|&i| copper[i].poly.clone()).collect();
        let pairs = spatial::candidates(
            &polys,
            2. * (r.options.max_deviation_mm + r.boolean_error_bound_mm),
        );
        r.statistics.candidate_pairs += pairs.len();
        for (a, b) in pairs {
            let a = indices[a];
            let b = indices[b];
            let ca = &copper[a];
            let cb = &copper[b];
            r.statistics.narrow_phase_pairs += 1;
            let matrix = prepared[a].relate(&prepared[b]);
            let error = ca.error + cb.error + 2. * r.boolean_error_bound_mm;
            let mut overlap = 0.;
            let (kind, reliable) =
                if matrix.get(CoordPos::Inside, CoordPos::Inside) == Dimensions::TwoDimensional {
                    let intersection = ca.poly.intersection(&cb.poly);
                    overlap = intersection.unsigned_area();
                    let narrow =
                        overlap <= intersection.0.iter().map(areas::perimeter).sum::<f64>() * error;
                    (if narrow { "uncertain" } else { "area" }, !narrow)
                } else if matrix.get(CoordPos::OnBoundary, CoordPos::OnBoundary)
                    == Dimensions::OneDimensional
                {
                    // Straight authored boundaries have no curve approximation.
                    // Merely touching approximated curves cannot establish a path.
                    if ca.error + cb.error == 0. {
                        ("boundary", true)
                    } else {
                        ("uncertain", false)
                    }
                } else if matrix.is_intersects() {
                    ("point", false)
                } else if Euclidean.distance(&ca.poly, &cb.poly) <= error {
                    ("uncertain", false)
                } else {
                    continue;
                };
            // A boolean representation must never close an originally positive
            // gap. Consult the uncut input representation before accepting a
            // derived boundary or overlap as a conductor.
            let (kind, reliable) = if possible {
                (kind, true)
            } else if reliable {
                let original = raw_prepared[&(ca.layer, ca.provenance.path.as_str())]
                    .relate(&raw_prepared[&(cb.layer, cb.provenance.path.as_str())]);
                if original.is_disjoint()
                    || (kind == "boundary"
                        && original.get(CoordPos::OnBoundary, CoordPos::OnBoundary)
                            != Dimensions::OneDimensional)
                {
                    ("uncertain", false)
                } else {
                    (kind, true)
                }
            } else {
                (kind, false)
            };
            if reliable {
                sets.join(a, b);
            } else {
                uncertain.push((a, b, kind));
            }
            r.contacts.push(Contact {
                a: ca.id.clone(),
                b: cb.id.clone(),
                kind: kind.into(),
                reliable: reliable && !possible,
                overlap_mm2: overlap,
                drill_id: None,
            });
        }
    }
    // Plating is a surface on the hole boundary, not a disk. An actual boundary
    // contact of nonzero length is required. Rounding-only near misses remain
    // explicitly uncertain, never an epsilon-created vertical conductor.
    for bore in bores.iter().filter(|b| b.plated && b.span_confirmed) {
        let mut hits = Vec::new();
        let nearby = copper_index.query(
            bore.shape.bounding_rect().unwrap(),
            4. * (r.options.max_deviation_mm + 2. * r.boolean_error_bound_mm),
        );
        for i in nearby {
            let c = &copper[i];
            if !bore.layers.contains(&c.layer) {
                continue;
            }
            let mut confirmed = false;
            let mut near = false;
            for hole in &bore.shape.0 {
                let m = prepared[i].relate(hole.exterior());
                near |= m.get(CoordPos::OnBoundary, CoordPos::Inside) == Dimensions::OneDimensional;
                // A fragment wholly inside the drilled empty centre is excluded
                // by the global drill difference before this point.
                near |= Euclidean.distance(&c.poly, hole.exterior())
                    <= 4. * (r.options.max_deviation_mm + 2. * r.boolean_error_bound_mm);
            }
            if near {
                // Prove a nonzero barrel patch in the *uncut* source, away
                // from its boundary and other holes. Boolean rounding can move
                // the derived hole boundary; proximity here only associates
                // that already-proven patch with its surviving source fragment.
                // It cannot create a contact between distinct copper sources.
                'proof: {
                    if possible {
                        confirmed = true;
                        break 'proof;
                    }
                    let source = raw_by_source[&(c.layer, c.provenance.path.as_str())];
                    for hole in &bore.shape.0 {
                        for line in hole.exterior().lines() {
                            let witness = Point::new(
                                (line.start.x + line.end.x) / 2.,
                                (line.start.y + line.end.y) / 2.,
                            );
                            let margin = source.error
                                + 4. * (r.options.max_deviation_mm + 2. * r.boolean_error_bound_mm);
                            let contained = source.shape.0.iter().any(|p| {
                                p.contains(&witness)
                                    && std::iter::once(p.exterior())
                                        .chain(p.interiors())
                                        .all(|ring| Euclidean.distance(&witness, ring) > margin)
                            });
                            let on_board = material.0.iter().any(|p| {
                                p.contains(&witness)
                                    && std::iter::once(p.exterior())
                                        .chain(p.interiors())
                                        .all(|ring| Euclidean.distance(&witness, ring) > margin)
                            });
                            let blocked = bores
                                .iter()
                                .filter(|b| b.id != bore.id && b.layers.contains(&c.layer))
                                .any(|b| {
                                    b.shape.0.iter().any(|p| {
                                        p.contains(&witness)
                                            || Euclidean.distance(&witness, p) <= margin
                                    })
                                });
                            if contained
                                && on_board
                                && !blocked
                                && Euclidean.distance(&witness, &c.poly)
                                    <= 4.
                                        * (r.options.max_deviation_mm
                                            + 2. * r.boolean_error_bound_mm)
                            {
                                confirmed = true;
                                break 'proof;
                            }
                        }
                    }
                }
            }
            if confirmed {
                hits.push(i);
            } else if near {
                diagnostic(r,&c.provenance,"barrel_contact_uncertain",format!("{}: boundary match below boolean representation accuracy; no vertical edge asserted",bore.id),"limitation");
            }
        }
        // A barrel can connect multiple copper fragments on the same layer too.
        if let Some(&first) = hits.first() {
            for &other in &hits[1..] {
                sets.join(first, other);
                r.contacts.push(Contact {
                    a: copper[first].id.clone(),
                    b: copper[other].id.clone(),
                    kind: "barrel".into(),
                    reliable: !possible,
                    overlap_mm2: 0.,
                    drill_id: Some(bore.id.clone()),
                });
            }
        }
    }
    for (a, b, kind) in uncertain {
        diagnostic(
            r,
            &copper[a].provenance,
            if kind == "point" {
                "point_contact"
            } else {
                "contact_uncertain"
            },
            format!(
                "{} and {}: {kind}; not a reliable conduction path",
                copper[a].id, copper[b].id
            ),
            "limitation",
        );
    }
    let mut groups = BTreeMap::<usize, Vec<usize>>::new();
    for i in 0..copper.len() {
        groups.entry(sets.root(i)).or_default().push(i);
    }
    for (i, group) in groups.values().enumerate() {
        let nets: BTreeSet<_> = group
            .iter()
            .filter_map(|&i| copper[i].provenance.net.clone())
            .collect();
        let ids: Vec<_> = group.iter().map(|&i| copper[i].id.clone()).collect();
        let id = format!("island-{i:06}");
        for n in &nets {
            r.net_islands.entry(n.clone()).or_default().push(id.clone());
        }
        if nets.len() > 1 {
            r.findings.push(Finding{code:"physical_short".into(),region_ids:ids.clone(),source_ids:group.iter().filter_map(|&i|copper[i].provenance.source_id.clone()).collect::<BTreeSet<_>>().into_iter().collect(),nets:nets.iter().cloned().collect(),message:"Confirmed physical copper path joins distinct source net labels; no winning net assigned".into()});
        }
        r.islands.push(Island {
            id,
            region_ids: ids,
            nets: nets.into_iter().collect(),
            has_unknown_net: group.iter().any(|&i| copper[i].provenance.net.is_none()),
        });
    }
    for (net, islands) in &r.net_islands {
        if islands.len() > 1 {
            r.findings.push(Finding{code:"net_islands".into(),nets:vec![net.clone()],region_ids:r.islands.iter().filter(|i|islands.contains(&i.id)).flat_map(|i|i.region_ids.clone()).collect(),source_ids:vec![],message:format!("Net label occurs in {} separate reliable components; possible_islands must be consulted, this is not a confirmed open",islands.len())});
        }
    }
    for track in &board.tracks {
        for &pad in track.connected_pads.iter().flatten() {
            let track_roots: BTreeSet<_> = copper
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.provenance.kind == "track" && c.provenance.path == track.source.path
                })
                .map(|(i, _)| sets.root(i))
                .collect();
            let pad_roots: BTreeSet<_> = copper
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.provenance.kind == "pad" && c.provenance.path == board.pads[pad].source.path
                })
                .map(|(i, _)| sets.root(i))
                .collect();
            if track_roots.is_disjoint(&pad_roots) {
                let p = provenance(
                    board,
                    &track.source,
                    "track",
                    track.net,
                    Some(track.ownership),
                );
                diagnostic(r,&p,"cad_pad_contact_mismatch",format!("CAD connectedPadIds claims {}, but no reliable physical path was established",board.pads[pad].source.id.as_deref().unwrap_or(&board.pads[pad].source.path)),"limitation");
            }
        }
    }
}
