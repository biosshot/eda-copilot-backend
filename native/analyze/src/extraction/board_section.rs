//! Physical board slicer for the general section operator. Source provenance and
//! disconnected conductors survive slicing; net labels never create a reference.
use super::local::board_mesh::Layer;
use super::{
    error,
    polygon_section::{self, Contour, MaterialRegion, Metal},
    Result,
};
use crate::geometry::{PolygonSnapshot, PreparationReport};
use geo::{Contains, LineString, Point, Polygon};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Slice {
    pub at_mm: [f64; 2],
    pub transverse_unit: [f64; 2],
    pub half_width_mm: f64,
    pub air_margin_mm: f64,
    pub reference_layer: String,
    pub reference_source_path: String,
    pub reference_x_mm: f64,
}
fn polygon(p: &PolygonSnapshot) -> Polygon<f64> {
    Polygon::new(
        LineString::from(p.outer.clone()),
        p.holes.iter().cloned().map(LineString::from).collect(),
    )
}
fn intervals(p: &PolygonSnapshot, s: &Slice, radius: f64) -> Result<Vec<[f64; 2]>> {
    let mut crossings = vec![];
    let [nx, ny] = s.transverse_unit;
    for ring in std::iter::once(&p.outer).chain(&p.holes) {
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            let a = [a[0] - s.at_mm[0], a[1] - s.at_mm[1]];
            let b = [b[0] - s.at_mm[0], b[1] - s.at_mm[1]];
            let va = a[0] * ny - a[1] * nx;
            let vb = b[0] * ny - b[1] * nx;
            if va == 0. && vb == 0. {
                return Err(error(
                    "SECTION_SLICE_TANGENCY",
                    "slice follows a captured polygon edge; an interior slice is required",
                ));
            }
            if (va <= 0. && vb > 0.) || (vb <= 0. && va > 0.) {
                let t = va / (va - vb);
                let x = if va == 0. {
                    a[0] * nx + a[1] * ny
                } else if vb == 0. {
                    b[0] * nx + b[1] * ny
                } else {
                    (a[0] + t * (b[0] - a[0])) * nx + (a[1] + t * (b[1] - a[1])) * ny
                };
                if x > -radius && x < radius {
                    crossings.push(x);
                    if crossings.len() > 100000 {
                        return Err(error("SECTION_SLICE_BUDGET", "transverse crossing cap"));
                    }
                }
            }
        }
    }
    crossings.sort_by(f64::total_cmp);
    let mut groups: Vec<(f64, usize)> = vec![];
    for x in crossings {
        if groups.last().is_some_and(|v| v.0 == x) {
            groups.last_mut().unwrap().1 += 1;
        } else {
            groups.push((x, 1));
        }
    }
    let mut cuts = vec![-radius];
    cuts.extend(groups.iter().map(|v| v.0));
    cuts.push(radius);
    let shape = polygon(p);
    let first = cuts[0] + (cuts[1] - cuts[0]) / 2.;
    let mut inside = shape.contains(&Point::new(
        s.at_mm[0] + first * nx,
        s.at_mm[1] + first * ny,
    ));
    let mut out = vec![];
    for (index, pair) in cuts.windows(2).enumerate() {
        if index > 0 && groups[index - 1].1 % 2 == 1 {
            inside = !inside;
        }
        let x = pair[0] + (pair[1] - pair[0]) / 2.;
        if x == pair[0] || x == pair[1] {
            return Err(error(
                "SECTION_SLICE_PRECISION",
                "unresolved transverse interval; no feature removed",
            ));
        }
        if inside {
            out.push([pair[0], pair[1]]);
        }
    }
    Ok(out)
}
fn rectangle(x0: f64, z0: f64, x1: f64, z1: f64) -> Contour {
    Contour {
        outer_m: vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]],
        holes_m: vec![],
    }
}

/// Returns geometry for the existing polygon FEM and source mapping separately.
/// Cropped transverse copper ends require a separate environment sensitivity
/// check. This slicer makes no whole-channel/transition or 5% accuracy claim.
pub fn slice(g: &PreparationReport, stack: &[Layer], s: &Slice) -> Result<serde_json::Value> {
    if g.status == "invalid"
        || g.outline.is_none()
        || stack.len() < 3
        || stack.len() > 256
        || g.regions.len() > 100000
        || !s.at_mm.iter().all(|v| v.is_finite())
        || !s.transverse_unit.iter().all(|v| v.is_finite())
        || ((s.transverse_unit[0].hypot(s.transverse_unit[1])) - 1.).abs() > 1e-10
        || !s.half_width_mm.is_finite()
        || s.half_width_mm <= 0.
        || !s.air_margin_mm.is_finite()
        || s.air_margin_mm <= 0.
        || !s.reference_x_mm.is_finite()
    {
        return Err(error(
            "SECTION_SLICE_INPUT",
            "finite point/unit direction, positive finite domain and complete stack required",
        ));
    }
    let total = stack.iter().map(|l| l.thickness_m).sum::<f64>();
    if !total.is_finite()
        || stack
            .iter()
            .any(|l| !l.thickness_m.is_finite() || l.thickness_m <= 0.)
    {
        return Err(error(
            "SECTION_SLICE_STACK",
            "positive explicit layer thicknesses required",
        ));
    }
    let radius = (s.half_width_mm + s.air_margin_mm) * 1e-3;
    let air = s.air_margin_mm * 1e-3;
    let domain = rectangle(-radius, -air, radius, total + air);
    let mut metals = vec![];
    let mut mapping = vec![];
    let mut dielectrics = vec![
        MaterialRegion {
            id: "upper-air".into(),
            relative_permittivity: 1.,
            contour: rectangle(-radius, -air, radius, 0.),
        },
        MaterialRegion {
            id: "lower-air".into(),
            relative_permittivity: 1.,
            contour: rectangle(-radius, total, radius, total + air),
        },
    ];
    let mut z = 0.;
    let mut reference_count = 0;
    let mut region_index = 0;
    for (layer_index, l) in stack.iter().enumerate() {
        let hi = z + l.thickness_m;
        let epsilon = if l.copper {
            if layer_index == 0 || layer_index == stack.len() - 1 {
                1.
            } else {
                return Err(error("SECTION_SLICE_MATERIAL","internal copper-gap dielectric needs an explicit physical material; no nearest-layer substitution"));
            }
        } else {
            l.epsilon_r
                .filter(|e| e.is_finite() && *e > 0.)
                .ok_or_else(|| {
                    error(
                        "SECTION_SLICE_MATERIAL",
                        "explicit dielectric epsilon required",
                    )
                })?
        };
        let outline = g.outline.as_ref().unwrap();
        let substrate = intervals(outline, s, s.half_width_mm + s.air_margin_mm)?;
        let mut cuts = vec![-radius, radius];
        for interval in &substrate {
            cuts.extend(interval.iter().map(|x| x * 1e-3));
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        for pair in cuts.windows(2) {
            let centre = (pair[0] + pair[1]) / 2.;
            let inside = substrate
                .iter()
                .any(|v| centre > v[0] * 1e-3 && centre < v[1] * 1e-3);
            dielectrics.push(MaterialRegion {
                id: format!("material-{region_index}"),
                relative_permittivity: if inside { epsilon } else { 1. },
                contour: rectangle(pair[0], z, pair[1], hi),
            });
            region_index += 1;
        }
        if l.copper {
            let layer = g
                .layers
                .iter()
                .find(|v| v.layer == l.name)
                .ok_or_else(|| error("SECTION_SLICE_LAYER", "captured copper layer absent"))?;
            for p in &layer.copper {
                for interval in intervals(p, s, s.half_width_mm)? {
                    let x = (interval[0] + interval[1]) / 2.;
                    let point = Point::new(
                        s.at_mm[0] + x * s.transverse_unit[0],
                        s.at_mm[1] + x * s.transverse_unit[1],
                    );
                    let owners = g
                        .regions
                        .iter()
                        .filter(|r| r.layer == l.name && polygon(&r.polygon).contains(&point))
                        .collect::<Vec<_>>();
                    if owners.is_empty() {
                        return Err(error(
                            "SECTION_SLICE_MAPPING",
                            "physical copper interval has no positive-area source attribution",
                        ));
                    }
                    let reference = l.name == s.reference_layer
                        && s.reference_x_mm > interval[0]
                        && s.reference_x_mm < interval[1]
                        && owners
                            .iter()
                            .any(|r| r.source.path == s.reference_source_path);
                    if reference {
                        reference_count += 1;
                    }
                    let id = format!("conductor-{}", metals.len());
                    let role = if reference { "reference" } else { "signal" };
                    mapping.push(serde_json::json!({"conductorId":id,"role":role,"layer":l.name,"intervalMm":interval,"sourcePaths":owners.iter().map(|r|&r.source.path).collect::<Vec<_>>(),"sourceRegionIds":owners.iter().map(|r|&r.id).collect::<Vec<_>>(),"condition":"independent section conductor; external connections are not idealized by net name"}));
                    metals.push(Metal {
                        id,
                        role: role.into(),
                        contour: rectangle(interval[0] * 1e-3, z, interval[1] * 1e-3, hi),
                    });
                }
            }
        }
        z = hi;
    }
    if reference_count != 1 || metals.len() > 17 {
        return Err(error("SECTION_SLICE_REFERENCE","one physical reference interval and at most16 independent other conductors required; slots/multiple contacts need a local transition model"));
    }
    let geometry = polygon_section::Request {
        loss: None,
        domain,
        conductors: metals,
        dielectrics,
        max_triangle_area_m2: 1e-8,
        max_nodes: 50000,
        max_seconds: 300.,
        max_memory_mib: 1024,
    };
    Ok(
        serde_json::json!({"geometry":geometry,"sourceMapping":mapping,"slice":s,"assumptions":["Captured copper and substrate contours, finite thickness and explicit materials; normalized transverse coordinates in metres","Transverse crop is an internal numerical domain requiring expansion checks; no end-pad/via/discontinuity model is supplied by one section","Every other conductor is independent until physical external connections are assembled; no ground-name equipotential joins"],"geometryDiagnostics":g.diagnostics,"geometryOptions":g.options}),
    )
}
