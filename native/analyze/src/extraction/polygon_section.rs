//! Internal polygon-section transport. Uses the existing P1 electric operator;
//! finite physical reference, natural exterior boundary, no template substitution.
use super::{error, field, Limits, Result};
use geo::{Area, Contains, Distance, Euclidean, Intersects, LineString, Point, Polygon, Validation};
use serde::{Deserialize, Serialize};
use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};
use std::{collections::BTreeSet, time::Instant};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Contour {
    pub outer_m: Vec<[f64; 2]>,
    #[serde(default)]
    pub holes_m: Vec<Vec<[f64; 2]>>,
}
impl Contour {
    pub(super) fn polygon(&self) -> Result<Polygon<f64>> {
        if self.outer_m.len() < 3
            || self
                .outer_m
                .iter()
                .chain(self.holes_m.iter().flatten())
                .flatten()
                .any(|x| !x.is_finite() || x.abs() > 10.)
            || self.holes_m.iter().any(|p| p.len() < 3)
        {
            return Err(error(
                "SECTION_POLYGON",
                "finite metre coordinates and rings with at least three vertices required",
            ));
        }
        let p = Polygon::new(
            LineString::from(self.outer_m.clone()),
            self.holes_m.iter().cloned().map(LineString::from).collect(),
        );
        if !p.is_valid() || p.unsigned_area() <= 0. {
            return Err(error("SECTION_POLYGON", "invalid/nonpositive polygon"));
        }
        Ok(p)
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Metal {
    pub id: String,
    pub role: String,
    pub contour: Contour,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MaterialRegion {
    pub id: String,
    pub relative_permittivity: f64,
    pub contour: Contour,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LossInput {
    pub frequencies_hz: Vec<f64>,
    pub conductivity_s_m: Vec<f64>,
    pub minimum_skin_dimensions_m: Vec<f64>,
    pub dielectric_loss_tangents: Vec<f64>,
    pub band_hz: [f64; 2],
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    #[serde(default)]
    pub loss: Option<LossInput>,
    pub domain: Contour,
    pub conductors: Vec<Metal>,
    pub dielectrics: Vec<MaterialRegion>,
    pub max_triangle_area_m2: f64,
    pub max_nodes: usize,
    pub max_seconds: f64,
    pub max_memory_mib: usize,
}
fn insert(cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>, p: &Contour) -> Result<()> {
    for ring in std::iter::once(&p.outer_m).chain(&p.holes_m) {
        let handles = ring
            .iter()
            .map(|p| {
                cdt.insert(Point2::new(p[0], p[1]))
                    .map_err(|e| error("SECTION_MESH", e))
            })
            .collect::<Result<Vec<_>>>()?;
        for i in 0..handles.len() {
            let a = handles[i];
            let b = handles[(i + 1) % handles.len()];
            if a != b {
                cdt.add_constraint_and_split(a, b, |p| p);
            }
        }
    }
    Ok(())
}
/// Geometric corner grading splits existing straight edges without changing
/// physical copper. Step halves with sqrt(cell area), so loss convergence also
/// refines the current singularity instead of only the distant dielectric.
fn corner_graded(p:&Contour,step:f64,cap:usize)->Result<Contour>{
 let mut total=0usize;let mut rings=vec![];
 for ring in std::iter::once(&p.outer_m).chain(&p.holes_m){let sharp=(0..ring.len()).map(|i|{let a=ring[(i+ring.len()-1)%ring.len()];let b=ring[i];let c=ring[(i+1)%ring.len()];let u=[b[0]-a[0],b[1]-a[1]];let v=[c[0]-b[0],c[1]-b[1]];(u[0]*v[0]+u[1]*v[1])<30_f64.to_radians().cos()*u[0].hypot(u[1])*v[0].hypot(v[1])}).collect::<Vec<_>>();let mut out=vec![];for i in 0..ring.len(){let a=ring[i];let b=ring[(i+1)%ring.len()];let length=(b[0]-a[0]).hypot(b[1]-a[1]);let mut cuts=vec![0.];let mut d=step;while d<length/2.{if sharp[i]{cuts.push(d/length);}if sharp[(i+1)%ring.len()]{cuts.push(1.-d/length);}d*=2.;if cuts.len()>256{return Err(error("SECTION_MESH_LIMIT","corner grading depth cap"));}}cuts.sort_by(f64::total_cmp);cuts.dedup();for t in cuts{total+=1;if total>cap{return Err(error("SECTION_MESH_LIMIT","corner vertex budget"));}out.push(if t==0.{a}else{[a[0]+t*(b[0]-a[0]),a[1]+t*(b[1]-a[1])]});}}rings.push(out);}
 Ok(Contour{outer_m:rings.remove(0),holes_m:rings})
}
/// The transport role "reference" fixes one conductor's voltage gauge. Every
/// other conductor has its own unit-potential coordinate under role "signal",
/// including conductors used as physical returns by the assembled network.
pub fn solve(r: &Request) -> Result<serde_json::Value> {solve_with_corner_scale(r,1.)}
struct SectionGeometry {
    limits: Limits,
    domain: Polygon<f64>,
    metal: Vec<Polygon<f64>>,
    materials: Vec<Polygon<f64>>,
    signals: Vec<usize>,
    cdt: ConstrainedDelaunayTriangulation<Point2<f64>>,
}

fn section_geometry(r: &Request, corner_scale: f64, start: Instant) -> Result<SectionGeometry> {
    if !(16..=200000).contains(&r.max_nodes)
        || !r.max_triangle_area_m2.is_finite()
        || r.max_triangle_area_m2 <= 0.
        || !r.max_seconds.is_finite()
        || r.max_seconds <= 0.
        || r.max_seconds > 300.
        || !(1..=2048).contains(&r.max_memory_mib)
        || !(2..=17).contains(&r.conductors.len())
    {
        return Err(error("SECTION_BUDGET", "invalid polygon section limits"));
    }
    let vertices = std::iter::once(&r.domain)
        .chain(r.conductors.iter().map(|c| &c.contour))
        .chain(r.dielectrics.iter().map(|d| &d.contour))
        .map(|c| c.outer_m.len() + c.holes_m.iter().map(Vec::len).sum::<usize>())
        .sum::<usize>();
    if vertices > r.max_nodes || r.dielectrics.len() > 256 {
        return Err(error("SECTION_BUDGET", "contour/material admission cap"));
    }
    let limits = Limits {
        nodes: r.max_nodes,
        triangles: 2 * r.max_nodes,
        memory: r.max_memory_mib * 1024 * 1024,
        seconds: r.max_seconds,
        start,
    };
    limits.check(r.max_nodes * 2048)?;
    let domain = r.domain.polygon()?;
    let metal = r
        .conductors
        .iter()
        .map(|c| c.contour.polygon())
        .collect::<Result<Vec<_>>>()?;
    let materials = r
        .dielectrics
        .iter()
        .map(|d| d.contour.polygon())
        .collect::<Result<Vec<_>>>()?;
    let mut ids = BTreeSet::new();
    let mut signals = vec![];
    let mut references = 0;
    for (i, c) in r.conductors.iter().enumerate() {
        if c.id.trim().is_empty() || !ids.insert(c.id.clone()) || !domain.contains(&metal[i]) {
            return Err(error(
                "SECTION_GEOMETRY",
                "unique conductors strictly inside domain required",
            ));
        }
        match c.role.as_str() {
            "signal" => signals.push(i),
            "reference" => references += 1,
            _ => return Err(error(
                "SECTION_CONDITION",
                "explicit signal/reference role required; floating condition not yet implemented",
            )),
        }
        for j in 0..i {
            if metal[i].intersects(&metal[j]) {
                return Err(error(
                    "SECTION_GEOMETRY",
                    "distinct conductor polygons touch or overlap",
                ));
            }
        }
    }
    if references != 1 || signals.is_empty() {
        return Err(error(
            "SECTION_CONDITION",
            "one physically connected reference polygon and 1..16 signals required",
        ));
    }
    for (i, d) in r.dielectrics.iter().enumerate() {
        if d.id.trim().is_empty()
            || !d.relative_permittivity.is_finite()
            || !(1. ..=100.).contains(&d.relative_permittivity)
        {
            return Err(error(
                "SECTION_MATERIAL",
                "explicit finite epsilon in 1..100 required",
            ));
        }
        for j in 0..i {
            if materials[i].intersection(&materials[j]).unsigned_area() > 1e-25 {
                return Err(error("SECTION_MATERIAL", "dielectric regions overlap"));
            }
        }
    }
    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    insert(&mut cdt, &r.domain)?;
    for c in &r.conductors {
        if let Some(loss)=&r.loss{
            let ordered=r.conductors.iter().filter(|c|c.role=="signal").chain(r.conductors.iter().filter(|c|c.role=="reference")).collect::<Vec<_>>();
            let index=ordered.iter().position(|m|m.id==c.id).unwrap();
            let dim=*loss.minimum_skin_dimensions_m.get(index).ok_or_else(||error("SECTION_LOSS_INPUT","complete physical skin dimensions required"))?;
            let bounds=geo::BoundingRect::bounding_rect(&domain).unwrap();let scale=bounds.width().max(bounds.height());let step=corner_scale*dim*r.max_triangle_area_m2.sqrt()/scale;
            if !step.is_finite()||step<=64.*f64::EPSILON*scale{return Err(error("SECTION_MESH_PRECISION","unresolved physical corner step"));}
            let graded=corner_graded(&c.contour,step,r.max_nodes.saturating_sub(cdt.num_vertices()))?;insert(&mut cdt,&graded)?;
        }else{insert(&mut cdt,&c.contour)?;}
    }
    for d in &r.dielectrics {
        insert(&mut cdt, &d.contour)?;
    }
    if cdt.num_vertices() > r.max_nodes {
        return Err(error("SECTION_MESH_LIMIT", "contour vertex cap"));
    }
    let refinement = cdt.refine(
        RefinementParameters::new()
            .with_angle_limit(AngleLimit::from_deg(30.))
            .with_max_allowed_area(r.max_triangle_area_m2)
            .with_max_additional_vertices(r.max_nodes - cdt.num_vertices()),
    );
    if !refinement.refinement_complete {
        return Err(error("SECTION_MESH_LIMIT", "polygon refinement cap"));
    }
    limits.check(cdt.num_vertices() * 2048)?;
    Ok(SectionGeometry { limits, domain, metal, materials, signals, cdt })
}

fn solve_with_corner_scale(r:&Request,corner_scale:f64)->Result<serde_json::Value>{
    let start = Instant::now();
    let SectionGeometry { limits, domain, metal, materials, signals, cdt } =
        section_geometry(r, corner_scale, start)?;
    let all_nodes = cdt
        .vertices()
        .map(|v| [v.position().x, v.position().y])
        .collect::<Vec<_>>();
    let mut triangles = vec![];
    let mut epsilon = vec![];
    let mut material_indices = vec![];
    for face in cdt.inner_faces() {
        let vertices = face.vertices();
        let p = Point::new(
            vertices.iter().map(|v| v.position().x).sum::<f64>() / 3.,
            vertices.iter().map(|v| v.position().y).sum::<f64>() / 3.,
        );
        if !domain.contains(&p) || metal.iter().any(|m| m.contains(&p)) {
            continue;
        }
        let regions = materials
            .iter()
            .enumerate()
            .filter(|(_, m)| m.contains(&p))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if regions.len() != 1 {
            return Err(error(
                "SECTION_MATERIAL",
                "every dielectric triangle needs exactly one material",
            ));
        }
        triangles.push(vertices.map(|v| v.fix().index()));
        epsilon.push(r.dielectrics[regions[0]].relative_permittivity);
        material_indices.push(regions[0]);
    }
    let mut used = vec![false; all_nodes.len()];
    for t in &triangles {
        for &i in t {
            used[i] = true;
        }
    }
    let mut nodes = vec![];
    let mut boundary = vec![];
    let mut remap = vec![usize::MAX; all_nodes.len()];
    // CDT splits constrained segments with floating coordinates. An exact point
    // predicate can then miss a boundary node by a few ulps; resolve only that
    // arithmetic uncertainty against the original contour, never a mesh-sized gap.
    let coordinate_scale = r.domain.outer_m.iter().flatten().map(|x| x.abs()).fold(0., f64::max);
    let boundary_roundoff = 64. * f64::EPSILON * coordinate_scale;
    for (i, &p) in all_nodes.iter().enumerate() {
        if !used[i] {
            continue;
        }
        remap[i] = nodes.len();
        nodes.push(p);
        let p = Point::from(p);
        let mut tag = 0;
        for (k, m) in metal.iter().enumerate() {
            if m.intersects(&p) || std::iter::once(m.exterior()).chain(m.interiors()).any(|ring| Euclidean.distance(&p, ring) <= boundary_roundoff) {
                if tag != 0 {
                    return Err(error("SECTION_BOUNDARY_AMBIGUOUS", "two physical conductor contours lie within floating boundary classification uncertainty"));
                }
                tag = signals
                    .iter()
                    .position(|&s| s == k)
                    .map(|v| v as i32 + 1)
                    .unwrap_or(-1);
            }
        }
        boundary.push(tag);
    }
    for t in &mut triangles {
        for i in t.iter_mut() {
            *i = remap[*i];
        }
    }
    for (k, _) in signals.iter().enumerate() {
        if !boundary.contains(&(k as i32 + 1)) {
            return Err(error("SECTION_MESH", "signal has no boundary nodes"));
        }
    }
    if !boundary.contains(&-1) {
        return Err(error("SECTION_MESH", "reference has no boundary nodes"));
    }
    let actual = field::solve_basis(
        &nodes,
        &triangles,
        &epsilon,
        &boundary,
        signals.len(),
        &limits,
    )?;
    let vacuum = field::solve_basis(
        &nodes,
        &triangles,
        &vec![1.; triangles.len()],
        &boundary,
        signals.len(),
        &limits,
    )?;
    let mut parameters = parameters(&actual.c, &vacuum.c)?;
    let loss = r
        .loss
        .as_ref()
        .map(|input| {
            loss_coefficients(
                r,
                input,
                &nodes,
                &triangles,
                &material_indices,
                &boundary,
                &actual,
                &vacuum,
                &parameters,
            )
        })
        .transpose()?;
    if loss.is_some() {
        parameters
            .as_object_mut()
            .unwrap()
            .remove("resistanceOhmPerM");
        parameters
            .as_object_mut()
            .unwrap()
            .remove("conductanceSPerM");
    }
    Ok(
        serde_json::json!({"conductorOrder":signals.iter().map(|&i|&r.conductors[i].id).collect::<Vec<_>>(),"reference":r.conductors.iter().find(|c|c.role=="reference").unwrap().id,"parameters":parameters,"frequencyDependent":loss,"capacitanceFPerM":actual.c,"vacuumCapacitanceFPerM":vacuum.c,"nodesM":nodes,"triangles":triangles,"epsilonR":epsilon,"potentialsV":actual.phi,"electricFieldsVPerM":actual.e,"quality":actual.quality,"vacuumQuality":vacuum.quality,"elapsedSeconds":start.elapsed().as_secs_f64(),"assumptions":[if loss.is_some(){"Weak-loss smooth surface impedance and regional dielectric energy; explicit conductivity, dimensions and material band"}else{"PEC electrostatics, explicit finite physical reference and polygon materials; natural outer boundary"},"One mesh is not spatial/domain convergence or a 5% accuracy certificate","No roughness, radiation or DC skin-transition admission; transient requires a separately verified causal model"]}),
    )
}
/// The magnetic handoff owns a distinct full-domain mesh and borrows original
/// conductor names. Its budget excludes mesh storage already retained here.
pub(super) struct TransitionMesh<'a> {
    nodes_m: Vec<[f64; 2]>,
    triangles: Vec<super::section_transition::Triangle>,
    conductors: Vec<super::section_transition::Conductor<'a>>,
    pub budget: super::section_transition::Budget,
    pub retained_bytes: usize,
    pub meshing_plan_bytes: usize,
}

impl TransitionMesh<'_> {
    pub(super) fn as_mesh(&self) -> super::section_transition::Mesh<'_> {
        super::section_transition::Mesh {
            nodes_m: &self.nodes_m,
            triangles: &self.triangles,
            conductors: &self.conductors,
            reference: self.conductors.len() - 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FullOwner {
    Outside,
    Dielectric,
    Metal(usize),
}

fn transition_ownership(detail: &str) -> String {
    error("SECTION_TRANSITION_OWNERSHIP", detail)
}

/// Materials use the existing signal order followed by the physical reference.
/// Conductivity is explicit and is independent of the weak-loss transport input.
pub(super) fn transition_mesh<'a>(
    request: &'a Request,
    conductivities: &[(&str, f64)],
    start: Instant,
) -> Result<TransitionMesh<'a>> {
    use super::{
        local::accurate::Sum,
        section_transition::{Budget, Conductor, Triangle},
    };
    use std::{mem::size_of, time::Duration};

    if !(2..=17).contains(&request.conductors.len())
        || conductivities.len() != request.conductors.len()
    {
        return Err(error(
            "SECTION_TRANSITION_MATERIAL",
            "one explicit conductivity for each of 2..17 original conductors required",
        ));
    }
    let order: Vec<_> = ["signal", "reference"]
        .into_iter()
        .flat_map(|role| {
            request
                .conductors
                .iter()
                .enumerate()
                .filter(move |(_, conductor)| conductor.role == role)
        })
        .collect();
    if order.len() != request.conductors.len()
        || order
            .iter()
            .zip(conductivities)
            .any(|((_, conductor), &(id, sigma))| {
                conductor.id != id || !sigma.is_finite() || sigma <= 0.
            })
    {
        return Err(error("SECTION_TRANSITION_MATERIAL",
            "explicit positive finite conductivity for each original conductor in signal/reference order required"));
    }

    let SectionGeometry {
        cdt,
        limits,
        domain,
        metal,
        ..
    } = section_geometry(request, 1., start)?;
    let count = cdt.num_vertices();
    let faces = cdt.num_all_faces();
    let meshing_plan_bytes = count
        .checked_mul(
            2048 + 2 * size_of::<[f64; 2]>()
                + size_of::<usize>()
                + size_of::<Option<usize>>()
                + size_of::<bool>(),
        )
        .and_then(|n| {
            n.checked_add(faces.checked_mul(size_of::<Triangle>() + size_of::<FullOwner>())?)
        })
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| {
            error(
                "SECTION_TRANSITION_MEMORY",
                "full CDT handoff byte plan overflow",
            )
        })?
        .max(request.max_nodes * 2048);
    limits.check(meshing_plan_bytes)?;

    let coordinate_scale = request
        .domain
        .outer_m
        .iter()
        .flatten()
        .map(|x| x.abs())
        .fold(0., f64::max);
    let roundoff = 64. * f64::EPSILON * coordinate_scale;
    for (i, polygon) in metal.iter().enumerate() {
        if std::iter::once(domain.exterior())
            .chain(domain.interiors())
            .any(|ring| Euclidean.distance(ring, polygon) <= roundoff)
            || metal[..i]
                .iter()
                .any(|other| Euclidean.distance(other, polygon) <= roundoff)
        {
            return Err(transition_ownership(
                "conductor clearance is unresolved at a metal or domain boundary",
            ));
        }
    }

    let all_nodes: Vec<_> = cdt
        .vertices()
        .map(|v| [v.position().x, v.position().y])
        .collect();
    let mut canonical = vec![0; metal.len()];
    for (index, &(original, _)) in order.iter().enumerate() {
        canonical[original] = index;
    }
    let mut used = vec![false; count];
    let mut vertex_metal = vec![None; count];
    let mut owners = vec![FullOwner::Outside; faces];
    let mut triangles = Vec::with_capacity(cdt.num_inner_faces());
    let mut domain_area = Sum::default();
    let mut conductor_areas = vec![Sum::default(); metal.len()];
    for face in cdt.inner_faces() {
        limits.check(meshing_plan_bytes)?;
        let nodes = face.vertices().map(|v| v.fix().index());
        let [a, b, c] = nodes.map(|i| all_nodes[i]);
        let centre = Point::new((a[0] + b[0] + c[0]) / 3., (a[1] + b[1] + c[1]) / 3.);
        if !domain.contains(&centre) {
            continue;
        }
        let mut conductor = None;
        for (original, polygon) in metal.iter().enumerate() {
            if polygon.contains(&centre) && conductor.replace(canonical[original]).is_some() {
                return Err(transition_ownership(
                    "triangle belongs to multiple physical conductors",
                ));
            }
        }
        let (area, _) = field::element(&all_nodes, nodes)?;
        domain_area = domain_area.add(Sum::from(area));
        for node in nodes {
            used[node] = true;
        }
        let owner = if let Some(i) = conductor {
            conductor_areas[i] = conductor_areas[i].add(Sum::from(area));
            for node in nodes {
                if vertex_metal[node].replace(i).is_some_and(|old| old != i) {
                    return Err(transition_ownership(
                        "distinct conductors share a triangulated vertex",
                    ));
                }
            }
            FullOwner::Metal(i)
        } else {
            FullOwner::Dielectric
        };
        owners[face.fix().index()] = owner;
        triangles.push(Triangle { nodes, conductor });
    }

    // Verify that every change of ownership is on an original constrained
    // polygon edge. The domain and every conductor can contain holes.
    for edge in cdt.undirected_edges() {
        limits.check(meshing_plan_bytes)?;
        let directed = edge.as_directed();
        let a = owners[directed.face().fix().index()];
        let b = owners[directed.rev().face().fix().index()];
        if a == b {
            continue;
        }
        let contour = match (a, b) {
            (FullOwner::Metal(i), FullOwner::Dielectric)
            | (FullOwner::Dielectric, FullOwner::Metal(i)) => &metal[order[i].0],
            (FullOwner::Outside, FullOwner::Dielectric)
            | (FullOwner::Dielectric, FullOwner::Outside) => &domain,
            _ => {
                return Err(transition_ownership(
                    "metal touches another conductor or the exterior boundary",
                ))
            }
        };
        let [a, b] = edge.vertices().map(|v| [v.position().x, v.position().y]);
        let checks = [
            Point::from(a),
            Point::from(b),
            Point::new((a[0] + b[0]) / 2., (a[1] + b[1]) / 2.),
        ];
        if !edge.is_constraint_edge()
            || !std::iter::once(contour.exterior())
                .chain(contour.interiors())
                .any(|ring| {
                    checks
                        .iter()
                        .all(|p| Euclidean.distance(p, ring) <= roundoff)
                })
        {
            return Err(transition_ownership(
                "mesh ownership interface is outside the original polygon constraints",
            ));
        }
    }

    let area_matches = |actual: Sum, expected: f64| {
        let actual = actual.value();
        actual.is_finite()
            && expected.is_finite()
            && actual > 0.
            && (actual - expected).abs() <= 1e-8 * expected
    };
    if !area_matches(domain_area, domain.unsigned_area())
        || order.iter().enumerate().any(|(i, &(original, _))| {
            !area_matches(conductor_areas[i], metal[original].unsigned_area())
        })
    {
        return Err(transition_ownership(
            "full-domain or conductor mesh area disagrees with the original polygon",
        ));
    }

    let mut remap = vec![usize::MAX; count];
    let mut nodes_m = Vec::with_capacity(used.iter().filter(|&&active| active).count());
    for (i, node) in all_nodes.into_iter().enumerate() {
        if used[i] {
            remap[i] = nodes_m.len();
            nodes_m.push(node);
        }
    }
    for triangle in &mut triangles {
        triangle.nodes = triangle.nodes.map(|i| remap[i]);
    }
    let conductors: Vec<_> = order
        .into_iter()
        .zip(conductivities)
        .map(|((_, conductor), &(_, conductivity_s_m))| Conductor {
            id: &conductor.id,
            conductivity_s_m,
        })
        .collect();
    let retained_bytes = nodes_m.capacity() * size_of::<[f64; 2]>()
        + triangles.capacity() * size_of::<Triangle>()
        + conductors.capacity() * size_of::<Conductor<'_>>();
    let max_bytes = limits.memory.checked_sub(retained_bytes).ok_or_else(|| {
        error(
            "SECTION_TRANSITION_MEMORY",
            "retained full-domain mesh exceeds budget",
        )
    })?;
    limits.check(meshing_plan_bytes)?;
    Ok(TransitionMesh {
        nodes_m,
        triangles,
        conductors,
        budget: Budget {
            max_bytes,
            deadline: Some(start + Duration::from_secs_f64(limits.seconds)),
        },
        retained_bytes,
        meshing_plan_bytes,
    })
}

use geo::BooleanOps;

fn loss_coefficients(
    r: &Request,
    loss: &LossInput,
    nodes: &[[f64; 2]],
    triangles: &[[usize; 3]],
    material_indices: &[usize],
    boundary: &[i32],
    actual: &field::Solution,
    vacuum: &field::Solution,
    p: &serde_json::Value,
) -> Result<serde_json::Value> {
    use std::collections::BTreeMap;
    let n = actual.c.len();
    if loss.conductivity_s_m.len() != n + 1
        || loss.minimum_skin_dimensions_m.len() != n + 1
        || loss.dielectric_loss_tangents.len() != r.dielectrics.len()
        || loss
            .conductivity_s_m
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.)
        || loss
            .minimum_skin_dimensions_m
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.)
        || loss
            .dielectric_loss_tangents
            .iter()
            .any(|v| !v.is_finite() || *v < 0. || *v > 0.05)
        || !loss.band_hz.iter().all(|v| v.is_finite())
        || loss.band_hz[0] <= 0.
        || loss.band_hz[1] < loss.band_hz[0]
        || loss.frequencies_hz.is_empty()
        || loss.frequencies_hz.len() > 8192
        || loss.frequencies_hz.len() * n * n * 6 > 131072
    {
        return Err(error("SECTION_LOSS_INPUT","explicit complete signal-then-reference conductivities/thicknesses, regional tangents and positive declared material band required"));
    }
    let inverse = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| p["inductanceHPerM"][i][j].as_f64().unwrap() / (super::MU0 * super::EPS0))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for t in triangles {
        for [a, b] in [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]] {
            *counts.entry((a.min(b), a.max(b))).or_insert(0usize) += 1;
        }
    }
    let mut edges = vec![vec![]; n + 1];
    for ((a, b), count) in counts {
        if count == 1 && boundary[a] != 0 && boundary[a] == boundary[b] {
            let group = if boundary[a] > 0 {
                boundary[a] as usize - 1
            } else {
                n
            };
            edges[group].push([a, b]);
        }
    }
    let ids = r
        .conductors
        .iter()
        .filter(|c| c.role == "signal")
        .chain(r.conductors.iter().filter(|c| c.role == "reference"))
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    let (surfaces, current_error) =
        super::losses::surface_grams_projected(nodes, &vacuum.reaction, &inverse, edges, &ids,Some(super::local::AllocationBudget{max_bytes:r.max_memory_mib.checked_mul(1024*1024).and_then(|v|v.checked_sub(r.max_nodes*2048)).ok_or_else(||error("SECTION_MEMORY_LIMIT","surface projection remaining budget"))?,max_nodes:r.max_nodes,max_tetrahedra:2*r.max_nodes}))?;
    let mut regional = vec![vec![vec![0.; n]; n]; r.dielectrics.len()];
    for (k, t) in triangles.iter().enumerate() {
        let [a, b, c] = t.map(|i| nodes[i]);
        let area = ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.;
        let region = material_indices[k];
        for i in 0..n {
            for j in 0..n {
                let ei = actual.e[i][k];
                let ej = actual.e[j][k];
                regional[region][i][j] += super::EPS0
                    * r.dielectrics[region].relative_permittivity
                    * area
                    * (ei[0] * ej[0] + ei[1] * ej[1]);
            }
        }
    }
    let mut regional_error: f64 = 0.;
    for i in 0..n {
        for j in 0..n {
            let sum = regional.iter().map(|a| a[i][j]).sum::<f64>();
            regional_error = regional_error.max((sum - actual.c[i][j]).abs() / actual.c[i][i]);
        }
    }
    if regional_error > 1e-8 {
        return Err(error(
            "SECTION_LOSS_NORMALIZATION",
            format!("regional charge energy {regional_error:e}"),
        ));
    }
    let samples=loss_samples(loss,p,&surfaces,&regional,&ids)?;
    Ok(
        serde_json::json!({"samples":samples,"surfaceIntegrals":surfaces,"regionalCapacitanceFPerM":regional,"currentNormalizationError":current_error,"regionalEnergyError":regional_error,"model":"vacuum unit-current surface impedance/regional dielectric energy perturbation; full coupled complex telegrapher modes","validBandHz":loss.band_hz}),
    )
}

/// Reuse immutable frequency-independent surface/region energies. No field solve.
pub(super) fn loss_samples(loss:&LossInput,p:&serde_json::Value,surfaces:&[super::losses::Surface],regional:&[Vec<Vec<f64>>],ids:&[String])->Result<Vec<serde_json::Value>> {
 let capacitance:Vec<Vec<f64>>=serde_json::from_value(p["capacitanceFPerM"].clone()).map_err(|e|error("SECTION_CACHE",e))?;let n=capacitance.len();
 if loss.conductivity_s_m.len()!=n+1||loss.minimum_skin_dimensions_m.len()!=n+1||surfaces.len()!=n+1||ids.len()!=n+1||loss.dielectric_loss_tangents.len()!=regional.len()||loss.conductivity_s_m.iter().chain(&loss.minimum_skin_dimensions_m).any(|v|!v.is_finite()||*v<=0.)||loss.dielectric_loss_tangents.iter().any(|v|!v.is_finite()||*v<0.||*v>0.05)||loss.band_hz.iter().any(|v|!v.is_finite())||loss.band_hz[0]<=0.||loss.band_hz[1]<loss.band_hz[0]||loss.frequencies_hz.is_empty()||loss.frequencies_hz.len()>8192||loss.frequencies_hz.len()*n*n*6>131072{return Err(error("SECTION_LOSS_INPUT","invalid restored loss/material dimensions or band"));}
    let mut samples = vec![];
    for (index, &f) in loss.frequencies_hz.iter().enumerate() {
        if !f.is_finite()
            || f < loss.band_hz[0]
            || f > loss.band_hz[1]
            || index > 0 && f <= loss.frequencies_hz[index - 1]
        {
            return Err(error(
                "SECTION_LOSS_BAND",
                "ordered frequencies must lie inside explicit material band",
            ));
        }
        let w = std::f64::consts::TAU * f;
        let mut skin = vec![];
        for (i, &sigma) in loss.conductivity_s_m.iter().enumerate() {
            let delta = (2. / (w * super::MU0 * sigma)).sqrt();
            if !delta.is_finite() || delta <= 0. || delta > loss.minimum_skin_dimensions_m[i] / 10.
            {
                return Err(error(
                    "SECTION_SKIN_DEPTH",
                    format!(
                        "surface {} delta={delta:e}m does not fit thickness/10",
                        ids[i]
                    ),
                ));
            }
            skin.push(delta);
        }
        let resistance = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| {
                        surfaces
                            .iter()
                            .enumerate()
                            .map(|(k, s)| {
                                s.gram_per_m[i][j]
                                    * (w * super::MU0 / (2. * loss.conductivity_s_m[k])).sqrt()
                            })
                            .sum::<f64>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let conductance = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| {
                        regional
                            .iter()
                            .enumerate()
                            .map(|(k, a)| w * loss.dielectric_loss_tangents[k] * a[i][j])
                            .sum::<f64>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let internal = (0..n)
            .map(|i| (0..n).map(|j| resistance[i][j] / w).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let total = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| p["inductanceHPerM"][i][j].as_f64().unwrap() + internal[i][j])
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let external = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| p["inductanceHPerM"][i][j].as_f64().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let conductor_bound = energy_bound(&resistance, &external)? / w;
        let dielectric_bound = energy_bound(&conductance, &capacitance)? / w;
        if conductor_bound > 0.05 || dielectric_bound > 0.05 {
            return Err(error("SECTION_WEAK_LOSS",format!("full matrix perturbation bounds {conductor_bound:e}/{dielectric_bound:e} exceed0.05")));
        }
        let modes = lossy_modes(&resistance, &total, &conductance, &capacitance, w)?;
        samples.push(serde_json::json!({"frequencyHz":f,"modes":modes,"conductorPerturbationBound":conductor_bound,"dielectricPerturbationBound":dielectric_bound,"capacitanceFPerM":capacitance,"resistanceOhmPerM":resistance,"internalInductanceHPerM":internal,"totalInductanceHPerM":total,"conductanceSPerM":conductance,"skinDepthM":skin}));
    }
 Ok(samples)
}

/// Exact uniform multiconductor telegrapher modes for the assembled complex
/// coefficients. Eigenvectors are not projected onto symmetric even/odd modes.
fn lossy_modes(
    r: &[Vec<f64>],
    l: &[Vec<f64>],
    g: &[Vec<f64>],
    c: &[Vec<f64>],
    omega: f64,
) -> Result<Vec<serde_json::Value>> {
    use super::propagation::Z;
    use faer::{c64, Mat};
    let n = r.len();
    let z = Mat::from_fn(n, n, |i, j| c64::new(r[i][j], omega * l[i][j]));
    let y = Mat::from_fn(n, n, |i, j| c64::new(g[i][j], omega * c[i][j]));
    let a = Mat::from_fn(n, n, |i, j| {
        (0..n).map(|k| z[(i, k)] * y[(k, j)]).sum::<c64>()
    });
    let scale = (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .map(|(i, j)| a[(i, j)].norm())
        .fold(0., f64::max);
    if !scale.is_finite() || scale <= 0. {
        return Err(error(
            "SECTION_PROPAGATION",
            "invalid complex telegrapher operator",
        ));
    }
    let eigen = Mat::from_fn(n, n, |i, j| a[(i, j)] / scale)
        .eigen()
        .map_err(|e| error("SECTION_PROPAGATION", format!("complex eigenproblem {e:?}")))?;
    let mut out = vec![];
    for k in 0..n {
        let lambda = eigen.S()[k] * scale;
        let gamma = Z {
            r: lambda.re,
            i: lambda.im,
        }
        .sqrt();
        if !gamma.r.is_finite() || !gamma.i.is_finite() || gamma.r < 0. || gamma.i <= 0. {
            return Err(error(
                "SECTION_PROPAGATION",
                "nonpassive modal square-root branch",
            ));
        }
        let norm = (0..n)
            .map(|i| eigen.U()[(i, k)].norm_sqr())
            .sum::<f64>()
            .sqrt();
        let v = (0..n).map(|i| eigen.U()[(i, k)] / norm).collect::<Vec<_>>();
        let current = (0..n)
            .map(|i| (0..n).map(|j| y[(i, j)] * v[j]).sum::<c64>() / c64::new(gamma.r, gamma.i))
            .collect::<Vec<_>>();
        let residual = (0..n)
            .map(|i| ((0..n).map(|j| a[(i, j)] * v[j]).sum::<c64>() - lambda * v[i]).norm_sqr())
            .sum::<f64>()
            .sqrt()
            / scale;
        if !residual.is_finite() || residual > 1e-8 {
            return Err(error(
                "SECTION_PROPAGATION",
                format!("complex modal residual {residual:e}"),
            ));
        }
        let values = |v: &[c64]| {
            v.iter()
                .map(|z| serde_json::json!({"real":z.re,"imaginary":z.im}))
                .collect::<Vec<_>>()
        };
        out.push(serde_json::json!({"name":format!("mode-{}",k+1),"propagationConstantPerM":gamma.value(),"voltageVector":values(&v),"currentVectorAPerV":values(&current),"normalizedEigenResidual":residual}));
    }
    Ok(out)
}

fn energy_bound(a:&[Vec<f64>],b:&[Vec<f64>])->Result<f64>{energy_metric_bound(a,b,false)}
fn energy_metric_bound(a: &[Vec<f64>], b: &[Vec<f64>],signed:bool) -> Result<f64> {
    use faer::{Mat, Side};
    let n = a.len();
    let scale = b.iter().flatten().map(|x| x.abs()).fold(0., f64::max);
    let eigen = Mat::from_fn(n, n, |i, j| b[i][j] / scale)
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| error("SECTION_ENERGY", format!("{e:?}")))?;
    if (0..n).any(|k| eigen.S()[k] <= 1e-12) {
        return Err(error(
            "SECTION_ENERGY",
            "loss reference energy is not resolved SPD",
        ));
    }
    let inverse = Mat::from_fn(n, n, |i, j| {
        (0..n)
            .map(|k| eigen.U()[(i, k)] * eigen.U()[(j, k)] / (scale * eigen.S()[k]).sqrt())
            .sum::<f64>()
    });
    let relative = Mat::from_fn(n, n, |i, j| {
        (0..n)
            .flat_map(|k| (0..n).map(move |m| (k, m)))
            .map(|(k, m)| inverse[(i, k)] * a[k][m] * inverse[(m, j)])
            .sum::<f64>()
    });
    let values = relative
        .self_adjoint_eigenvalues(Side::Lower)
        .map_err(|e| error("SECTION_ENERGY", format!("{e:?}")))?;
    let bound = values.iter().map(|v|if signed{v.abs()}else{*v}).fold(0., f64::max);
    if !bound.is_finite() || !signed&&values.iter().any(|v| *v < -1e-10 * bound.max(1.)) {
        return Err(error(
            "SECTION_ENERGY",
            "nonpassive loss coefficient matrix",
        ));
    }
    Ok(bound)
}

fn loss_change_raw(a: &serde_json::Value, b: &serde_json::Value) -> f64 {
    let compare = |x: &serde_json::Value, y: &serde_json::Value| -> f64 {
        let Some(rows) = x.as_array() else {
            return f64::INFINITY;
        };
        let n = rows.len();
        if y.as_array().is_none_or(|v| v.len() != n) {
            return f64::INFINITY;
        }
        let mut bound: f64 = 0.;
        for i in 0..n {
            let mut row = 0.;
            for j in 0..n {
                let scale = x[i][i].as_f64().unwrap().max(y[i][i].as_f64().unwrap())
                    * x[j][j].as_f64().unwrap().max(y[j][j].as_f64().unwrap());
                let difference = (x[i][j].as_f64().unwrap() - y[i][j].as_f64().unwrap()).abs();
                row += if scale > 0. {
                    difference / scale.sqrt()
                } else if difference == 0. {
                    0.
                } else {
                    f64::INFINITY
                };
            }
            bound = bound.max(row);
        }
        bound
    };
    let mut bound: f64 = 0.;
    for (key, matrix_key) in [
        ("surfaceIntegrals", "gramPerM"),
        ("regionalCapacitanceFPerM", ""),
    ] {
        let Some(x) = a[key].as_array() else {
            return f64::INFINITY;
        };
        let Some(y) = b[key].as_array() else {
            return f64::INFINITY;
        };
        if x.len() != y.len() {
            return f64::INFINITY;
        }
        for (x, y) in x.iter().zip(y) {
            bound = bound.max(if matrix_key.is_empty() {
                compare(x, y)
            } else {
                compare(&x[matrix_key], &y[matrix_key])
            });
        }
    }
    bound
}

fn loss_error_metric(y:&[Vec<f64>],floor:f64)->Result<Vec<Vec<f64>>>{
 use faer::{Mat,Side};let n=y.len();let scale=(0..n).map(|i|y[i][i].abs()).fold(0.,f64::max);
 if scale==0.{return Ok((0..n).map(|i|(0..n).map(|j|if i==j{floor}else{0.}).collect()).collect());}
 let e=Mat::from_fn(n,n,|i,j|y[i][j]/scale).self_adjoint_eigen(Side::Lower).map_err(|e|error("SECTION_ENERGY",format!("{e:?}")))?;
 Ok((0..n).map(|i|(0..n).map(|j|(0..n).map(|k|e.U()[(i,k)]*e.U()[(j,k)]*(scale*e.S()[k]).max(floor)).sum()).collect()).collect())
}
/// Physical total/component loss energies, with an explicit weak-energy floor.
/// The floor belongs only to error normalization, never to physical R or G.
fn loss_change(a:&serde_json::Value,b:&serde_json::Value,loss:&LossInput)->f64{
 let read=|v:&serde_json::Value|serde_json::from_value::<Vec<Vec<f64>>>(v.clone()).ok();
 let Some(ar)=read(&a["samples"][0]["resistanceOhmPerM"])else{return f64::INFINITY};let Some(br)=read(&b["samples"][0]["resistanceOhmPerM"])else{return f64::INFINITY};let Some(ag)=read(&a["samples"][0]["conductanceSPerM"])else{return f64::INFINITY};let Some(bg)=read(&b["samples"][0]["conductanceSPerM"])else{return f64::INFINITY};let n=ar.len();let frequency=a["samples"][0]["frequencyHz"].as_f64().unwrap();let omega=std::f64::consts::TAU*frequency;
 let compare=|x:&[Vec<f64>],y:&[Vec<f64>],scale:f64|{let difference=(0..n).map(|i|(0..n).map(|j|x[i][j]-y[i][j]).collect::<Vec<_>>()).collect::<Vec<_>>();if scale==0.{return if difference.iter().flatten().all(|v|*v==0.){0.}else{f64::INFINITY};}let Ok(metric)=loss_error_metric(y,1e-3*scale)else{return f64::INFINITY};energy_metric_bound(&difference,&metric,true).unwrap_or(f64::INFINITY)};
 let rs=(0..n).map(|i|ar[i][i].max(br[i][i])).fold(0.,f64::max);let gs=(0..n).map(|i|ag[i][i].max(bg[i][i])).fold(0.,f64::max);let mut bound=compare(&ar,&br,rs).max(compare(&ag,&bg,gs));
 for (key,weights,scale) in [("surfaceIntegrals",loss.conductivity_s_m.iter().map(|s|(omega*super::MU0/(2.*s)).sqrt()).collect::<Vec<_>>(),rs),("regionalCapacitanceFPerM",loss.dielectric_loss_tangents.iter().map(|t|omega*t).collect::<Vec<_>>(),gs)]{
  for(k,weight)in weights.into_iter().enumerate(){let (x,y)=if key=="surfaceIntegrals"{(&a[key][k]["gramPerM"],&b[key][k]["gramPerM"])}else{(&a[key][k],&b[key][k])};let Some(mut x)=read(x)else{return f64::INFINITY};let Some(mut y)=read(y)else{return f64::INFINITY};for row in &mut x{for v in row{*v*=weight;}}for row in &mut y{for v in row{*v*=weight;}}bound=bound.max(compare(&x,&y,scale));}
 }
 bound
}

fn component(mode: &serde_json::Value, key: &str, index: usize) -> Result<super::propagation::Z> {
    use super::propagation::Z;
    let value = &mode[key][index];
    if let Some(r) = value.as_f64() {
        return Ok(Z { r, i: 0. });
    }
    let r = value["real"]
        .as_f64()
        .ok_or_else(|| error("SECTION_INPUT", "missing complex mode coefficient"))?;
    let i = value["imaginary"]
        .as_f64()
        .ok_or_else(|| error("SECTION_INPUT", "missing complex mode coefficient"))?;
    if !r.is_finite() || !i.is_finite() {
        return Err(error("SECTION_NONFINITE", "nonfinite mode coefficient"));
    }
    Ok(Z { r, i })
}
fn wave_factor(mode: &serde_json::Value, f: f64, length: f64) -> Result<super::propagation::Z> {
    use super::propagation::Z;
    let gamma = &mode["propagationConstantPerM"];
    let (alpha, beta) = if gamma.is_object() {
        (
            gamma["real"]
                .as_f64()
                .ok_or_else(|| error("SECTION_INPUT", "missing attenuation"))?,
            gamma["imaginary"]
                .as_f64()
                .ok_or_else(|| error("SECTION_INPUT", "missing phase constant"))?,
        )
    } else {
        (
            0.,
            std::f64::consts::TAU
                * f
                * mode["delaySPerM"]
                    .as_f64()
                    .ok_or_else(|| error("SECTION_INPUT", "missing modal delay"))?,
        )
    };
    let phase = beta * length;
    if !phase.is_finite() || phase.abs() > 1e9 || !alpha.is_finite() || alpha < 0. || beta < 0. {
        return Err(error(
            "SECTION_APPLICABILITY",
            "invalid passive propagation range",
        ));
    }
    let attenuation = (-alpha * length).exp();
    Ok(Z {
        r: attenuation * phase.cos(),
        i: -attenuation * phase.sin(),
    })
}

/// Internal transport on the existing section dispatch; no new DSL builder.
/// Polygon input is never replaced by the nearest microstrip/stripline template.
pub(super) fn run_transport(input: super::Request) -> serde_json::Value {
    let started = Instant::now();
    let mut convergence = vec![];
    let result = (|| -> Result<serde_json::Value> {
        if input.line.as_object().is_none_or(|v| {
            v.keys()
                .any(|k| !["kind", "geometry", "lengthMm"].contains(&k.as_str()))
        }) {
            return Err(error("SECTION_INPUT", "unknown polygon transport field"));
        }
        let mut r: Request = serde_json::from_value(input.line["geometry"].clone())
            .map_err(|e| error("SECTION_INPUT", e))?;
        if let Some(loss) = &mut r.loss {
            loss.frequencies_hz = input.frequencies.clone();
        }
        let seconds = input
            .budget
            .as_ref()
            .and_then(|b| b.max_seconds)
            .unwrap_or(r.max_seconds)
            .min(r.max_seconds);
        let memory = input
            .budget
            .as_ref()
            .and_then(|b| b.max_memory_mi_b)
            .unwrap_or(r.max_memory_mib as f64);
        if !memory.is_finite() || memory < 1. || !seconds.is_finite() || seconds <= 0. {
            return Err(error("SECTION_BUDGET", "invalid transport budget"));
        }
        r.max_memory_mib = r.max_memory_mib.min(memory as usize);
        if let Some(nodes) = input.mesh.as_ref().and_then(|m| m.max_nodes) {
            r.max_nodes = r.max_nodes.min(nodes);
        }
        if let Some(mesh) = &input.mesh {
            if mesh.max_edge_mm.is_some() || mesh.domain_scale.is_some() {
                return Err(error("SECTION_MESH_INPUT", "polygon geometry has an explicit exterior domain and target cell area; maxEdgeMm/domainScale are unsupported"));
            }
            if let Some(triangles) = mesh.max_triangles {
                // A planar triangulation has fewer than twice as many faces as vertices.
                r.max_nodes = r.max_nodes.min(triangles / 2);
            }
        }
        let max_refinements = input
            .mesh
            .as_ref()
            .and_then(|m| m.max_refinements)
            .unwrap_or(4);
        if max_refinements > 5 {
            return Err(error("SECTION_MESH_INPUT", "at most five bounded polygon mesh refinements are supported"));
        }
        let domain = r.domain.polygon()?;
        let bounds = geo::BoundingRect::bounding_rect(&domain).unwrap();
        let dimension = bounds.width().max(bounds.height());
        let epsilon = r
            .dielectrics
            .iter()
            .map(|d| d.relative_permittivity)
            .fold(1., f64::max);
        let max_frequency = 299792458. / (20. * dimension * epsilon.sqrt());
        if input.frequencies.is_empty() || input.frequencies.len() > 8192
            || input.frequencies.iter().enumerate().any(|(i, f)| {
                !f.is_finite()
                    || *f < 0.
                    || *f > max_frequency
                    || i > 0 && *f <= input.frequencies[i - 1]
            })
        {
            return Err(error(
                "SECTION_APPLICABILITY",
                "finite ordered frequencies must fit transverse wavelength/20 screen",
            ));
        }
        let length = input
            .line
            .get("lengthMm")
            .map(|v| {
                v.as_f64()
                    .filter(|x| x.is_finite() && *x > 0.)
                    .map(|x| x * 1e-3)
                    .ok_or_else(|| error("SECTION_INPUT", "positive finite lengthMm required"))
            })
            .transpose()?;
        let mut last: Option<serde_json::Value> = None;
        let mut last_loss: Option<serde_json::Value> = None;
        let mut accepted = None;let mut corner_scale=1.;
        for level in 0..=max_refinements {
            r.max_seconds = seconds - started.elapsed().as_secs_f64();
            let mut report = solve_with_corner_scale(&r,corner_scale)?;
            let p = report["parameters"].clone();
            let mut delta: f64 = 0.;
            if let Some(old) = &last {
                for key in [
                    "capacitanceFPerM",
                    "vacuumCapacitanceFPerM",
                    "inductanceHPerM",
                ] {
                    let n = p[key].as_array().unwrap().len();
                    let floor = (0..n)
                        .map(|i| p[key][i][i].as_f64().unwrap().abs())
                        .fold(0., f64::max)
                        * 1e-3;
                    for i in 0..n {
                        for j in 0..n {
                            let x = p[key][i][j].as_f64().unwrap();
                            let y = old[key][i][j].as_f64().unwrap();
                            delta = delta.max((x - y).abs() / x.abs().max(floor));
                        }
                    }
                }
            }
            let loss_delta = last_loss
                .as_ref()
                .map(|old| loss_change(&report["frequencyDependent"], old,r.loss.as_ref().unwrap()));
            convergence.push(serde_json::json!({"cornerStepScale":corner_scale,"lossEnergyAbsoluteFloorRelativeToLargestTotalSelf":0.001,"unweightedIntegralChangeDiagnostic":last_loss.as_ref().map(|old|loss_change_raw(&report["frequencyDependent"],old)),"lossPowerRelativeChangeBound":loss_delta,"level":level,"maxTriangleAreaM2":r.max_triangle_area_m2,"nodes":report["nodesM"].as_array().unwrap().len(),"triangles":report["triangles"].as_array().unwrap().len(),"maxMatrixRelativeChange":if last.is_some(){Some(delta)}else{None}}));
            if last.is_some()
                && delta <= 0.01
                && (r.loss.is_none() || loss_delta.is_some_and(|v| v <= 0.02))
            {
                if let Some(length) = length {
                    if r.loss.is_some() {
                        let samples = report["frequencyDependent"]["samples"].as_array().unwrap();
                        let mut all = vec![];
                        let mut ports = serde_json::Value::Null;
                        let mut excess: f64 = 0.;
                        let mut reciprocity: f64 = 0.;
                        for sample in samples {
                            let waves = network(
                                sample,
                                &[sample["frequencyHz"].as_f64().unwrap()],
                                length,
                                50.,
                            )?;
                            all.push(waves["parameters"][0].clone());
                            ports = waves["ports"].clone();
                            excess = excess
                                .max(waves["quality"]["maxPassivityExcess"].as_f64().unwrap());
                            reciprocity = reciprocity
                                .max(waves["quality"]["reciprocityResidual"].as_f64().unwrap());
                        }
                        report["network"] = serde_json::json!({"representation":"S","referenceOhm":50.,"frequenciesHz":input.frequencies,"ports":ports,"parameters":all,"quality":{"maxPassivityExcess":excess,"reciprocityResidual":reciprocity},"phaseConvention":"exp(+j omega t); passive complex coupled modes"});
                    } else {
                        report["network"] = network(&p, &input.frequencies, length, 50.)?;
                    }
                }
                accepted = Some(report);
                break;
            }
            last = Some(p);
            if r.loss.is_some() {
                last_loss = Some(report["frequencyDependent"].clone());
            }
            if last.is_some()&&delta<=0.01&&r.loss.is_some()&&loss_delta.is_some_and(|v|v>0.02){
                // C/L bulk already converged: continue refining it, while
                // focusing the next loss refinement on Cu corners.
                r.max_triangle_area_m2*=0.5;corner_scale*=0.5;
            }else{r.max_triangle_area_m2*=0.25;}
        }
        let mut report = accepted.ok_or_else(|| {
            error(
                "SECTION_CONVERGENCE",
                "polygon matrix mesh convergence failed within shared caps",
            )
        })?;
        report["status"] = serde_json::json!("complete");
        report["diagnostics"] = serde_json::json!([]);
        report["fieldRequestMesh"]=serde_json::json!(input.mesh);
        report["input"] = input.line;
        report["convergence"] = serde_json::json!(convergence);
        report["frequenciesHz"] = serde_json::json!(input.frequencies);
        report["applicability"] = serde_json::json!({"maxFrequencyHz":max_frequency,"domainConvergence":"unverified explicit finite natural boundary","fullWaveValidated":false,"lossModel":if r.loss.is_some(){"conditional smooth surface impedance/regional dielectric energy with coupled complex modes"}else{"PEC/lossless only"}});
        Ok(report)
    })();
    match result {
        Ok(mut report) => {
            report["elapsedMs"] = serde_json::json!(started.elapsed().as_secs_f64() * 1000.);
            report
        }
        Err(e) => {
            serde_json::json!({"status":"rejected","diagnostics":[{"code":e.split(':').next().unwrap_or("SECTION_REJECTED"),"message":e,"severity":"error"}],"convergence":convergence,"elapsedMs":started.elapsed().as_secs_f64()*1000.})
        }
    }
}

/// General multiconductor lossless modes. Voltage vectors have Euclidean norm
/// one; corresponding current vectors are C*v/delay. This explicitly defined
/// modal normalization avoids symmetric-pair formulas for unequal conductors.
pub fn parameters(c: &[Vec<f64>], c0: &[Vec<f64>]) -> Result<serde_json::Value> {
    use faer::{Mat, Side};
    let n = c.len();
    if n == 0
        || n > 16
        || c0.len() != n
        || c.iter()
            .chain(c0)
            .any(|r| r.len() != n || r.iter().any(|x| !x.is_finite()))
    {
        return Err(error(
            "SECTION_MATRIX",
            "finite square C/C0 of order 1..16 required",
        ));
    }
    let scale = c0.iter().flatten().map(|x| x.abs()).fold(0., f64::max);
    if scale <= 0. {
        return Err(error("SECTION_ENERGY", "nonpositive vacuum energy"));
    }
    for a in [c, c0] {
        for i in 0..n {
            for j in 0..n {
                if (a[i][j] - a[j][i]).abs() > 1e-8 * a[i][i].abs().max(a[j][j].abs()) {
                    return Err(error("SECTION_MATRIX", "asymmetric energy matrix"));
                }
            }
        }
    }
    let eigen = Mat::from_fn(n, n, |i, j| c0[i][j] / scale)
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| error("SECTION_MATRIX", format!("{e:?}")))?;
    if (0..n).any(|i| eigen.S()[i] <= 1e-12) {
        return Err(error(
            "SECTION_ENERGY",
            "vacuum C not positive definite or ill-conditioned",
        ));
    }
    let l = Mat::from_fn(n, n, |i, j| {
        (0..n)
            .map(|k| eigen.U()[(i, k)] * eigen.U()[(j, k)] / eigen.S()[k])
            .sum::<f64>()
            * super::MU0
            * super::EPS0
            / scale
    });
    let le = l
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| error("SECTION_MATRIX", format!("{e:?}")))?;
    let root = Mat::from_fn(n, n, |i, j| {
        (0..n)
            .map(|k| le.U()[(i, k)] * le.U()[(j, k)] * le.S()[k].sqrt())
            .sum::<f64>()
    });
    let b = Mat::from_fn(n, n, |i, j| {
        (0..n)
            .flat_map(|k| (0..n).map(move |m| (k, m)))
            .map(|(k, m)| root[(i, k)] * c[k][m] * root[(m, j)])
            .sum::<f64>()
    });
    let be = b
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| error("SECTION_MATRIX", format!("{e:?}")))?;
    let mut modes = vec![];
    for k in 0..n {
        let delay = be.S()[k].sqrt();
        if !delay.is_finite() || delay <= 0. {
            return Err(error("SECTION_ENERGY", "nonpositive modal energy"));
        }
        let mut v = (0..n)
            .map(|i| (0..n).map(|j| root[(i, j)] * be.U()[(j, k)]).sum::<f64>())
            .collect::<Vec<_>>();
        let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        for x in &mut v {
            *x /= norm;
        }
        let current = (0..n)
            .map(|i| (0..n).map(|j| c[i][j] * v[j]).sum::<f64>() / delay)
            .collect::<Vec<_>>();
        let z = 1. / current.iter().map(|x| x * x).sum::<f64>().sqrt();
        modes.push(serde_json::json!({"name":format!("mode-{}",k+1),"delaySPerM":delay,"impedanceOhm":z,"voltageVector":v,"currentVectorAPerV":current}));
    }
    let l_rows = (0..n)
        .map(|i| (0..n).map(|j| l[(i, j)]).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    Ok(
        serde_json::json!({"capacitanceFPerM":c,"vacuumCapacitanceFPerM":c0,"inductanceHPerM":l_rows,"modes":modes,"modalConvention":"voltage Euclidean norm=1; i=C*v/delay; impedance=norm(v)/norm(i)","resistanceOhmPerM":{"state":"PEC assumption","matrix":vec![vec![0.;n];n]},"conductanceSPerM":{"state":"lossless assumption","matrix":vec![vec![0.;n];n]}}),
    )
}

/// Stable forward/backward waves for the general voltage/current modal basis.
/// Ports remain physical conductors; the basis need not be orthogonal in the
/// Euclidean metric. No positive exponential or transmission-line segmentation.
pub fn network(
    p: &serde_json::Value,
    frequencies: &[f64],
    length: f64,
    reference: f64,
) -> Result<serde_json::Value> {
    use super::propagation::Z;
    let modes = p["modes"]
        .as_array()
        .ok_or_else(|| error("SECTION_INPUT", "missing modes"))?;
    let n = modes.len();
    let m = 2 * n;
    if n == 0
        || n > 16
        || !length.is_finite()
        || length <= 0.
        || !reference.is_finite()
        || reference <= 0.
        || frequencies.is_empty()
        || frequencies.len() * m * m > 131072
        || frequencies
            .iter()
            .enumerate()
            .any(|(i, f)| !f.is_finite() || *f < 0. || i > 0 && *f <= frequencies[i - 1])
    {
        return Err(error(
            "SECTION_BUDGET",
            "invalid multiconductor network request",
        ));
    }
    let lossy = modes
        .iter()
        .any(|m| m["propagationConstantPerM"].is_object());
    let mut max_passivity_excess: f64 = 0.;
    let mut parameters = vec![];
    let mut unitary: f64 = 0.;
    let mut reciprocity: f64 = 0.;
    for &f in frequencies {
        let mut a = vec![vec![Z::default(); m]; m];
        let mut rhs = vec![vec![Z::default(); m]; m];
        for i in 0..m {
            rhs[i][i] = Z {
                r: 2. * reference.sqrt(),
                i: 0.,
            };
        }
        for k in 0..n {
            let q = wave_factor(&modes[k], f, length)?;
            for i in 0..n {
                let v = component(&modes[k], "voltageVector", i)?;
                let current = component(&modes[k], "currentVectorAPerV", i)?;
                let direct = v.add(current.scale(reference));
                let reverse = q.mul(v.sub(current.scale(reference)));
                a[i][k] = direct;
                a[i][k + n] = reverse;
                a[i + n][k] = reverse;
                a[i + n][k + n] = direct;
            }
        }
        // Bounded dense coefficient solve (<=32), pivoted complex elimination. All
        // independent incident-port RHS share one factorization per frequency.
        let scale = a.iter().flatten().map(|z| z.abs()).fold(0., f64::max);
        for k in 0..m {
            let pivot = (k..m)
                .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
                .unwrap();
            if a[pivot][k].abs() <= scale * 1e-12 {
                return Err(error("SECTION_MATRIX", "singular modal wave matching"));
            }
            a.swap(k, pivot);
            rhs.swap(k, pivot);
            let diagonal = a[k][k];
            for j in k..m {
                a[k][j] = a[k][j].div(diagonal);
            }
            for j in 0..m {
                rhs[k][j] = rhs[k][j].div(diagonal);
            }
            for i in 0..m {
                if i == k {
                    continue;
                }
                let factor = a[i][k];
                for j in k..m {
                    a[i][j] = a[i][j].sub(factor.mul(a[k][j]));
                }
                for j in 0..m {
                    rhs[i][j] = rhs[i][j].sub(factor.mul(rhs[k][j]));
                }
            }
        }
        let mut s = vec![vec![Z::default(); m]; m];
        for i in 0..n {
            for j in 0..m {
                for k in 0..n {
                    let q = wave_factor(&modes[k], f, length)?;
                    let v = component(&modes[k], "voltageVector", i)?.scale(1. / reference.sqrt());
                    s[i][j] = s[i][j].add(rhs[k][j].add(q.mul(rhs[k + n][j])).mul(v));
                    s[i + n][j] = s[i + n][j].add(q.mul(rhs[k][j]).add(rhs[k + n][j]).mul(v));
                }
            }
        }
        for i in 0..m {
            s[i][i].r -= 1.;
        }
        for i in 0..m {
            for j in 0..m {
                let mut dot = Z::default();
                for row in &s {
                    dot = dot.add(
                        Z {
                            r: row[i].r,
                            i: -row[i].i,
                        }
                        .mul(row[j]),
                    );
                }
                dot.r -= if i == j { 1. } else { 0. };
                unitary = unitary.max(dot.abs());
                reciprocity = reciprocity.max(s[i][j].sub(s[j][i]).abs());
            }
        }
        if lossy {
            let gram = faer::Mat::from_fn(m, m, |i, j| {
                let mut z = Z::default();
                for row in &s {
                    z = z.add(
                        Z {
                            r: row[i].r,
                            i: -row[i].i,
                        }
                        .mul(row[j]),
                    );
                }
                faer::c64::new(z.r, z.i)
            });
            let eigen = gram
                .self_adjoint_eigenvalues(faer::Side::Lower)
                .map_err(|e| error("SECTION_POWER_BALANCE", format!("{e:?}")))?;
            max_passivity_excess =
                max_passivity_excess.max(eigen.iter().copied().fold(0., f64::max) - 1.);
        }
        if max_passivity_excess > 1e-8
            || !unitary.is_finite()
            || !reciprocity.is_finite()
            || !lossy && unitary > 1e-8
            || reciprocity > 1e-8
        {
            return Err(error(
                "SECTION_POWER_BALANCE",
                format!("unitary {unitary:e}; reciprocity {reciprocity:e}"),
            ));
        }
        parameters.push(
            s.iter()
                .map(|row| row.iter().map(|z| z.value()).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        );
    }
    Ok(
        serde_json::json!({"representation":"S","referenceOhm":reference,"frequenciesHz":frequencies,"ports":(0..m).map(|i|format!("{}-{}",if i<n{"input"}else{"output"},i%n)).collect::<Vec<_>>(),"parameters":parameters,"quality":{"unitarityResidual":if lossy{None}else{Some(unitary)},"maxPassivityExcess":max_passivity_excess,"reciprocityResidual":reciprocity},"phaseConvention":"exp(+j omega t), physical currents enter both ends"}),
    )
}

#[cfg(test)]mod loss_energy_floor_tests{
 use super::*;use serde_json::json;
 #[test]fn signed_energy_changes_are_bounded_in_both_directions(){let m=vec![vec![1.,0.],vec![0.,0.001]];let d=vec![vec![-0.01,0.],vec![0.,0.00001]];assert!((energy_metric_bound(&d,&m,true).unwrap()-0.01).abs()<1e-12);let opposite=d.iter().map(|r|r.iter().map(|x|-x).collect()).collect::<Vec<_>>();assert!((energy_metric_bound(&opposite,&m,true).unwrap()-0.01).abs()<1e-12);assert!(energy_bound(&d,&m).is_err());}
 #[test]fn weak_loss_floor_does_not_hide_a_strong_total_loss_change(){
  let f=1e9;let loss=LossInput{frequencies_hz:vec![f],conductivity_s_m:vec![std::f64::consts::TAU*f*super::super::MU0/2.;2],minimum_skin_dimensions_m:vec![1.;2],dielectric_loss_tangents:vec![0.],band_hz:[f,f]};
  let response=|strong:f64,weak:f64|json!({"samples":[{"frequencyHz":f,"resistanceOhmPerM":[[strong+weak]],"conductanceSPerM":[[0.]]}],"surfaceIntegrals":[{"gramPerM":[[strong]]},{"gramPerM":[[weak]]}],"regionalCapacitanceFPerM":[[[1.]]]});
  let original=response(1.,0.);let weak=response(1.,1e-6);assert!(loss_change_raw(&weak,&original)>0.02);assert!(loss_change(&weak,&original,&loss)<0.002);assert!(loss_change(&response(1.03,0.),&original,&loss)>0.02);assert!(loss_change(&response(1.02001,0.),&original,&loss)>0.02);assert!(loss_change(&response(0.97,0.),&original,&loss)>0.02);
 }
}

#[cfg(test)]
mod transition_handoff_tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[test]
    fn full_transition_mesh_keeps_original_copper_and_holes() {
        let domain = Contour {
            outer_m: vec![[0., 0.], [4., 0.], [4., 2.], [2., 2.], [2., 4.], [0., 4.]],
            holes_m: vec![],
        };
        let mut request = Request {
            loss: None,
            domain: domain.clone(),
            conductors: vec![
                Metal {
                    id: "return".into(),
                    role: "reference".into(),
                    contour: Contour {
                        outer_m: rect(0.25, 0.25, 1.25, 1.25),
                        holes_m: vec![rect(0.5, 0.5, 1., 1.)],
                    },
                },
                Metal {
                    id: "signal".into(),
                    role: "signal".into(),
                    contour: Contour {
                        outer_m: rect(2.5, 0.25, 3.5, 1.25),
                        holes_m: vec![],
                    },
                },
            ],
            dielectrics: vec![MaterialRegion {
                id: "air".into(),
                relative_permittivity: 1.,
                contour: domain,
            }],
            max_triangle_area_m2: 0.2,
            max_nodes: 2048,
            max_seconds: 10.,
            max_memory_mib: 16,
        };
        let conductivities = [("signal", 58e6), ("return", 49e6)];
        let start = Instant::now();
        assert!(transition_mesh(&request, &conductivities[..1], start)
            .err()
            .unwrap()
            .contains("SECTION_TRANSITION_MATERIAL"));
        {
            let prepared = transition_mesh(&request, &conductivities, start).unwrap();
            let mesh = prepared.as_mesh();
            assert_eq!(mesh.reference, 1);
            assert_eq!(
                mesh.conductors
                    .iter()
                    .map(|c| (c.id, c.conductivity_s_m))
                    .collect::<Vec<_>>(),
                conductivities
            );
            let mut areas = [0.; 3];
            let mut used = BTreeSet::new();
            let mut hole_faces = 0;
            for triangle in mesh.triangles {
                let (area, _) = field::element(mesh.nodes_m, triangle.nodes).unwrap();
                areas[triangle.conductor.unwrap_or(2)] += area;
                used.extend(triangle.nodes);
                let [a, b, c] = triangle.nodes.map(|i| mesh.nodes_m[i]);
                let x = (a[0] + b[0] + c[0]) / 3.;
                let y = (a[1] + b[1] + c[1]) / 3.;
                assert!(x < 2. || y < 2.);
                if x > 0.5 && x < 1. && y > 0.5 && y < 1. {
                    assert_eq!(triangle.conductor, None);
                    hole_faces += 1;
                }
            }
            assert!(hole_faces > 0);
            assert_eq!(used.len(), mesh.nodes_m.len());
            for (actual, expected) in areas.into_iter().zip([1., 0.75, 10.25]) {
                assert!((actual - expected).abs() < 1e-10);
            }
            assert_eq!(
                prepared.budget.max_bytes + prepared.retained_bytes,
                16 << 20
            );
            assert!(prepared.meshing_plan_bytes >= prepared.retained_bytes);
            assert_eq!(
                prepared.budget.deadline,
                Some(start + std::time::Duration::from_secs(10))
            );
        }
        request.conductors[1].contour.outer_m = rect(1.25, 0.25, 2.25, 1.25);
        assert!(transition_mesh(&request, &conductivities, Instant::now())
            .err()
            .unwrap()
            .contains("SECTION_GEOMETRY"));
    }
}
