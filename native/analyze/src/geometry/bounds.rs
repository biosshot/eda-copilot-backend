//! Two-sided copper envelopes, constructed BEFORE clipping and drilling.
//! Offsetting the final snapshot would lose features already erased by a
//! boolean operation (and could not provide an upper bound).
use super::*;
use geo::{
    algorithm::buffer::{BufferStyle, LineJoin},
    Buffer, Relate,
};

pub(super) fn offset(
    shape: &AreaSet,
    distance: f64,
    builder: &mut primitives::Builder,
) -> Result<AreaSet, String> {
    if distance == 0. || shape.0.is_empty() {
        return Ok(shape.clone());
    }
    let vertices: usize = shape
        .0
        .iter()
        .map(|p| {
            std::iter::once(p.exterior())
                .chain(p.interiors())
                .map(|r| r.0.len())
                .sum::<usize>()
        })
        .sum();
    // A round join has at most 8 segments per full turn. Reserve workspace
    // before offsetting; also count the actual result before retaining it.
    let workspace = vertices
        .checked_mul(12)
        .ok_or("Envelope vertex budget overflow")?;
    if workspace
        > builder
            .options
            .max_generated_vertices
            .saturating_sub(builder.generated)
    {
        return Err("Envelope vertex budget exceeded; reliable topology not asserted".into());
    }
    // Round joins are chords. The pinned implementation floors the segment
    // count, so an actual step can approach pi/2 at a pi/4 setting.
    // Doubling still encloses the requested disk: 2*cos(pi/4) > 1, on either
    // side of convex/concave boundaries. The caller includes the boolean guard.
    let result = shape.buffer_with_style(
        BufferStyle::new(2. * distance).line_join(LineJoin::Round(std::f64::consts::FRAC_PI_4)),
    );
    let count = result
        .0
        .iter()
        .map(|p| {
            std::iter::once(p.exterior())
                .chain(p.interiors())
                .map(|r| r.0.len())
                .sum::<usize>()
        })
        .sum();
    builder.reserve(count)?;
    for p in &result.0 {
        p.check_validation()
            .map_err(|e| format!("Invalid envelope: {e}"))?;
    }
    Ok(result)
}

// At most six locally wider guards are tried. Widening bounds never changes
// the nominal tolerance, invents a reliable path, or repairs captured geometry.
#[allow(clippy::too_many_arguments)]
pub(super) fn derive(
    x: &Raw,
    material: &AreaSet,
    bores: &[Bore],
    nearby: &[usize],
    cached_bores: &[Result<(AreaSet, AreaSet), String>],
    cached_material: &Result<(AreaSet, AreaSet), String>,
    base_guard: f64,
    guard: f64,
    builder: &mut primitives::Builder,
) -> Result<(AreaSet, AreaSet), String> {
    if nearby.is_empty() && x.error == 0. && material.relate(&x.shape).is_covers() {
        return Ok((x.shape.clone(), x.shape.clone()));
    }
    let lower_source = offset(&x.shape, -guard, builder)?;
    let upper_source = offset(&x.shape, x.error + guard, builder)?;
    let fresh_material;
    let (lower_material, upper_material) = if guard == base_guard {
        cached_material.as_ref().map_err(Clone::clone)?
    } else {
        fresh_material = (
            offset(material, -guard, builder)?,
            offset(material, guard, builder)?,
        );
        &fresh_material
    };
    let mut larger_drills = Vec::new();
    let mut smaller_drills = Vec::new();
    for &i in nearby {
        let fresh_bore;
        let (larger, smaller) = if guard == base_guard {
            cached_bores[i].as_ref().map_err(Clone::clone)?
        } else {
            fresh_bore = (
                offset(
                    &bores[i].shape,
                    builder.options.max_deviation_mm + guard,
                    builder,
                )?,
                offset(&bores[i].shape, -guard, builder)?,
            );
            &fresh_bore
        };
        larger_drills.extend(larger.0.clone());
        smaller_drills.extend(smaller.0.clone());
    }
    let lower = areas::subtract(
        &lower_source.intersection(lower_material),
        &areas::union(larger_drills),
    );
    let upper = areas::subtract(
        &upper_source.intersection(upper_material),
        &areas::union(smaller_drills),
    );
    for p in lower.0.iter().chain(&upper.0) {
        p.check_validation().map_err(|e| {
            format!("Degenerate boolean envelope: {e}; no reliable topology asserted")
        })?;
    }
    Ok((lower, upper))
}

pub(super) fn snapshots(board: &Board, copper: &[Copper]) -> Vec<Region> {
    copper
        .iter()
        .map(|c| Region {
            id: c.id.clone(),
            layer: board.layers[c.layer].name.clone(),
            source: c.provenance.clone(),
            polygon: areas::snapshot(&c.poly),
            area_mm2: c.poly.unsigned_area(),
            approximation_mm: c.error,
        })
        .collect()
}

pub(super) fn identify(copper: &mut [Copper], prefix: &str) {
    copper.sort_by(|a, b| {
        key(&a.provenance)
            .cmp(&key(&b.provenance))
            .then(a.provenance.path.cmp(&b.provenance.path))
            .then(a.layer.cmp(&b.layer))
            .then_with(|| {
                a.poly
                    .bounding_rect()
                    .unwrap()
                    .min()
                    .x
                    .total_cmp(&b.poly.bounding_rect().unwrap().min().x)
            })
            .then_with(|| {
                a.poly
                    .bounding_rect()
                    .unwrap()
                    .min()
                    .y
                    .total_cmp(&b.poly.bounding_rect().unwrap().min().y)
            })
            .then_with(|| a.poly.unsigned_area().total_cmp(&b.poly.unsigned_area()))
    });
    for (i, c) in copper.iter_mut().enumerate() {
        c.id = format!("{prefix}-{i:06}");
    }
}

// Compare whole components, including topology INSIDE one input source.
// No net label participates in construction of either graph.
pub(super) fn compare(
    board: &Board,
    lower: &[Copper],
    upper: &[Copper],
    r: &mut PreparationReport,
) {
    use geo::{Contains, InteriorPoint};
    let mut by_source = BTreeMap::<_, Vec<_>>::new();
    for (i, c) in upper.iter().enumerate() {
        by_source
            .entry((c.layer, c.provenance.path.as_str()))
            .or_default()
            .push(i);
    }
    let lower_island: BTreeMap<_, _> = r
        .islands
        .iter()
        .flat_map(|i| {
            i.region_ids
                .iter()
                .map(move |id| (id.as_str(), i.id.as_str()))
        })
        .collect();
    let upper_island: BTreeMap<_, _> = r
        .possible_islands
        .iter()
        .enumerate()
        .flat_map(|(i, island)| island.region_ids.iter().map(move |id| (id.as_str(), i)))
        .collect();
    let mut roots = vec![BTreeSet::new(); r.possible_islands.len()];
    let mut orphaned = Vec::new();
    for c in lower {
        // By construction C- is contained in C+ of the SAME source. Usually
        // there is one upper fragment; do not run whole-polygon DE-9IM against
        // all overlapping board fills merely to recover this parent relation.
        let candidates = by_source.get(&(c.layer, c.provenance.path.as_str()));
        let parent = candidates.and_then(|ids| {
            if ids.len() == 1 {
                return Some(ids[0]);
            }
            let witness = c.poly.interior_point()?;
            ids.iter()
                .copied()
                .find(|&i| upper[i].poly.contains(&witness))
        });
        if let Some(i) = parent {
            roots[upper_island[upper[i].id.as_str()]]
                .insert(lower_island[c.id.as_str()].to_string());
        } else {
            orphaned.push(c.provenance.clone());
        }
    }
    for p in orphaned {
        diagnostic(
            r,
            &p,
            "envelope_inconsistent",
            "Reliable component has no possible parent; bounds cannot certify topology",
            "invalid",
        );
    }
    r.statistics.uncertain_components = roots.iter().filter(|roots| roots.len() != 1).count();
    let by_id: BTreeMap<_, _> = upper.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut affected = BTreeMap::new();
    for (i, island) in r.possible_islands.iter().enumerate() {
        if roots[i].len() != 1 {
            for id in &island.region_ids {
                let c = by_id[id.as_str()];
                affected.insert((c.layer, c.provenance.path.clone()), c.provenance.clone());
            }
        }
    }
    for ((layer, _), p) in affected {
        diagnostic(r, &p, "topology_uncertain", format!("{}: possible copper component has zero or multiple reliable components; internal neck, contact or disappearing feature lies within the two-sided boundary envelope. Separate reliable islands are not a confirmed open", board.layers[layer].name), "limitation");
    }
}
