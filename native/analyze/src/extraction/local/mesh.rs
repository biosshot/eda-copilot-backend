//! Deterministic specialized layer extrusion. Planar arrangement ownership is
//! supplied explicitly; this primitive never reclassifies boundary vertices by
//! floating point point-in-polygon tests or snaps/drops a thin feature.
use super::{error, AllocationBudget, Result};
use geo::algorithm::{coordinate_position::CoordPos, dimensions::Dimensions};
use geo::{LineString, Polygon, Relate};
use rstar::{RTree, RTreeObject, AABB};
use std::collections::{BTreeMap, BTreeSet};

struct PlanarBounds {
    index: usize,
    envelope: AABB<[f64; 2]>,
}
impl RTreeObject for PlanarBounds {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

// Combinatorial edge incidence alone admits overlapping disconnected triangles.
// Require geometric intersection to equal exactly the shared vertex/edge IDs.
pub(crate) fn validate_planar_embedding_xy(
    xy_m: &[[f64; 2]],
    triangles: &[[usize; 3]],
) -> Result<()> {
    let mut coordinates = BTreeSet::new();
    for p in xy_m {
        let key = p.map(|v| if v == 0. { 0 } else { v.to_bits() });
        if !coordinates.insert(key) {
            return Err(error(
                "PLANAR_OWNERSHIP",
                "duplicate coordinates with distinct node IDs",
            ));
        }
    }
    let polygons: Vec<_> = triangles
        .iter()
        .map(|t| {
            Polygon::new(
                LineString::from(vec![xy_m[t[0]], xy_m[t[1]], xy_m[t[2]], xy_m[t[0]]]),
                vec![],
            )
        })
        .collect();
    let bounds: Vec<_> = triangles
        .iter()
        .enumerate()
        .map(|(index, t)| {
            let lo = std::array::from_fn(|k| {
                t.iter().map(|&i| xy_m[i][k]).fold(f64::INFINITY, f64::min)
            });
            let hi = std::array::from_fn(|k| {
                t.iter()
                    .map(|&i| xy_m[i][k])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            PlanarBounds {
                index,
                envelope: AABB::from_corners(lo, hi),
            }
        })
        .collect();
    let tree = RTree::bulk_load(bounds);
    for a in tree.iter() {
        for b in tree
            .locate_in_envelope_intersecting(&a.envelope)
            .filter(|b| b.index > a.index)
        {
            let shared = triangles[a.index]
                .iter()
                .filter(|i| triangles[b.index].contains(i))
                .count();
            let expected = match shared {
                0 => Dimensions::Empty,
                1 => Dimensions::ZeroDimensional,
                2 => Dimensions::OneDimensional,
                _ => return Err(error("PLANAR_OVERLAP", "duplicate triangle support")),
            };
            let relation = polygons[a.index].relate(&polygons[b.index]);
            if relation.get(CoordPos::Inside, CoordPos::Inside) != Dimensions::Empty
                || relation.get(CoordPos::Inside, CoordPos::OnBoundary) != Dimensions::Empty
                || relation.get(CoordPos::OnBoundary, CoordPos::Inside) != Dimensions::Empty
                || relation.get(CoordPos::OnBoundary, CoordPos::OnBoundary) != expected
            {
                return Err(error(
                    "PLANAR_OVERLAP",
                    "geometric intersection differs from shared node/edge ownership",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub enum Material {
    Copper { source_ids: Vec<String> },
    Dielectric { label: String, epsilon_r: f64 },
}

#[derive(Clone, Debug)]
pub struct LayeredInput {
    pub xy_m: Vec<[f64; 2]>,
    pub triangles: Vec<[usize; 3]>,
    /// Exact outer field-domain edges of the already conforming XY arrangement.
    /// Hole/barrel contours are internal constraints, not external domain edges.
    pub boundary_edges: Vec<[usize; 2]>,
    pub z_m: Vec<f64>,
    /// [slab][triangle], inherited from planar face ownership and layer policy.
    pub materials: Vec<Vec<Material>>,
}

#[derive(Clone, Debug)]
pub struct Tetrahedron {
    pub nodes: [usize; 4],
    pub slab: usize,
    pub planar_triangle: usize,
    pub epsilon_r: f64,
    pub material_label: String,
    pub volume_m3: f64,
    pub gradients_per_m: [[f64; 3]; 4],
}

#[derive(Clone, Debug)]
pub struct InterfaceFace {
    pub nodes: [usize; 3],
    pub copper_source_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct LayeredMesh {
    pub nodes_m: Vec<[f64; 3]>,
    pub tetrahedra: Vec<Tetrahedron>,
    pub copper_interfaces: Vec<InterfaceFace>,
    pub external_faces: Vec<[usize; 3]>,
    pub dielectric_volume_m3: f64,
    pub copper_volume_m3: f64,
    pub planned_peak_bytes: usize,
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Positive tetra volume and P1 gradients, invariant under input orientation.
pub fn tetra_coefficients(points: [[f64; 3]; 4]) -> Result<(f64, [[f64; 3]; 4])> {
    if !points.iter().flatten().all(|x| x.is_finite()) {
        return Err(error("TETRA_INPUT", "nonfinite coordinates"));
    }
    let u = sub(points[1], points[0]);
    let v = sub(points[2], points[0]);
    let w = sub(points[3], points[0]);
    let determinant = dot(u, cross(v, w));
    let volume = determinant.abs() / 6.;
    if !volume.is_finite() || volume <= 0. {
        return Err(error(
            "TETRA_DEGENERATE",
            "zero/nonfinite tetra volume; no removal applied",
        ));
    }
    let g1 = cross(v, w).map(|x| x / determinant);
    let g2 = cross(w, u).map(|x| x / determinant);
    let g3 = cross(u, v).map(|x| x / determinant);
    let g0 = [
        -g1[0] - g2[0] - g3[0],
        -g1[1] - g2[1] - g3[1],
        -g1[2] - g2[2] - g3[2],
    ];
    let gradients = [g0, g1, g2, g3];
    if !gradients.iter().flatten().all(|x| x.is_finite()) {
        return Err(error("TETRA_RANGE", "gradient overflow"));
    }
    Ok((volume, gradients))
}

fn faces(nodes: [usize; 4]) -> [[usize; 3]; 4] {
    std::array::from_fn(|skip| {
        let mut face = [0; 3];
        let mut j = 0;
        for (i, node) in nodes.into_iter().enumerate() {
            if i != skip {
                face[j] = node;
                j += 1;
            }
        }
        face.sort();
        face
    })
}

pub fn extrude(input: &LayeredInput, budget: AllocationBudget) -> Result<LayeredMesh> {
    let n = input.xy_m.len();
    let slabs = input.z_m.len().checked_sub(1).unwrap_or(0);
    if n < 3
        || slabs == 0
        || input.triangles.is_empty()
        || input.materials.len() != slabs
        || !input
            .xy_m
            .iter()
            .flatten()
            .chain(input.z_m.iter())
            .all(|x| x.is_finite())
        || input.z_m.windows(2).any(|z| z[1] <= z[0])
        || input
            .materials
            .iter()
            .any(|m| m.len() != input.triangles.len())
    {
        return Err(error(
            "MESH_INPUT",
            "finite shared XY mesh, increasing layers and complete material ownership required",
        ));
    }
    let node_count = n
        .checked_mul(input.z_m.len())
        .ok_or_else(|| error("MESH_LIMIT", "node count overflow"))?;
    let tetra_count = input
        .triangles
        .len()
        .checked_mul(slabs)
        .and_then(|v| v.checked_mul(3))
        .ok_or_else(|| error("MESH_LIMIT", "tetra count overflow"))?;
    if node_count > budget.max_nodes || tetra_count > budget.max_tetrahedra {
        return Err(error(
            "MESH_LIMIT",
            "extrusion exceeds explicit node/tetra cap before allocation",
        ));
    }
    // Includes full face maps and identifiers during topological auditing, not
    // just retained tetra arrays. Field/factor/basis workspace is separate.
    let planned = node_count
        .checked_mul(32)
        .and_then(|v| v.checked_add(tetra_count.checked_mul(1024)?))
        .ok_or_else(|| error("MEMORY_LIMIT", "mesh plan overflow"))?;
    budget.check(planned)?;
    let mut triangle_set = BTreeSet::new();
    let mut planar_edges = BTreeMap::<[usize; 2], usize>::new();
    for &t in &input.triangles {
        if t.iter().any(|&i| i >= n) {
            return Err(error("MESH_INPUT", "triangle index outside XY mesh"));
        }
        let mut key = t;
        key.sort();
        if key[0] == key[1] || key[1] == key[2] || !triangle_set.insert(key) {
            return Err(error("MESH_INPUT", "duplicate/degenerate planar triangle"));
        }
        let [a, b, c] = t.map(|i| input.xy_m[i]);
        let area = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.;
        if !area.is_finite() || area <= 0. {
            return Err(error(
                "MESH_INPUT",
                "zero/nonfinite planar triangle; no dropping applied",
            ));
        }
        for j in 0..3 {
            let mut edge = [t[j], t[(j + 1) % 3]];
            edge.sort();
            *planar_edges.entry(edge).or_default() += 1;
        }
    }
    let mut boundary_edges = BTreeSet::new();
    for &edge in &input.boundary_edges {
        let mut edge = edge;
        edge.sort();
        if edge[0] == edge[1] || edge[1] >= n || !boundary_edges.insert(edge) {
            return Err(error(
                "MESH_INPUT",
                "invalid/duplicate declared field boundary edge",
            ));
        }
    }
    if planar_edges
        .iter()
        .any(|(edge, &count)| count > 2 || (count == 1) != boundary_edges.contains(edge))
        || boundary_edges
            .iter()
            .any(|edge| !planar_edges.contains_key(edge))
    {
        return Err(error(
            "MESH_CRACK",
            "planar edge incidence differs from declared field boundary",
        ));
    }
    for material in input.materials.iter().flatten() {
        match material {
            Material::Copper { source_ids }
                if source_ids.is_empty() || source_ids.iter().any(|s| s.is_empty()) =>
            {
                return Err(error(
                    "MATERIAL_INPUT",
                    "copper requires exact source identifiers",
                ))
            }
            Material::Dielectric { label, epsilon_r }
                if label.is_empty() || !epsilon_r.is_finite() || *epsilon_r <= 0. =>
            {
                return Err(error(
                    "MATERIAL_INPUT",
                    "positive finite epsilon and material label required",
                ))
            }
            _ => {}
        }
    }
    validate_planar_embedding_xy(&input.xy_m, &input.triangles)?;
    let nodes_m: Vec<_> = input
        .z_m
        .iter()
        .flat_map(|&z| input.xy_m.iter().map(move |p| [p[0], p[1], z]))
        .collect();
    let mut tetrahedra = Vec::new();
    let mut all_faces = BTreeMap::<[usize; 3], usize>::new();
    let mut dielectric_faces = BTreeMap::<[usize; 3], usize>::new();
    let mut copper_faces = BTreeMap::<[usize; 3], BTreeSet<String>>::new();
    let (mut dielectric_volume_m3, mut copper_volume_m3) = (0., 0.);
    for slab in 0..slabs {
        for (index, &triangle) in input.triangles.iter().enumerate() {
            let mut t = triangle;
            t.sort();
            let [a, b, c] = t;
            let [a0, b0, c0] = [a, b, c].map(|i| slab * n + i);
            let [a1, b1, c1] = [a, b, c].map(|i| (slab + 1) * n + i);
            // Global vertex ordering makes every shared rectangular face use
            // the same diagonal, independent of triangle orientation/ownership.
            for nodes in [[a0, b0, c0, c1], [a0, b0, b1, c1], [a0, a1, b1, c1]] {
                let (volume, gradients) = tetra_coefficients(nodes.map(|i| nodes_m[i]))?;
                let ff = faces(nodes);
                for face in ff {
                    *all_faces.entry(face).or_default() += 1;
                }
                match &input.materials[slab][index] {
                    Material::Copper { source_ids } => {
                        copper_volume_m3 += volume;
                        for face in ff {
                            copper_faces
                                .entry(face)
                                .or_default()
                                .extend(source_ids.iter().cloned());
                        }
                    }
                    Material::Dielectric { label, epsilon_r } => {
                        dielectric_volume_m3 += volume;
                        for face in ff {
                            *dielectric_faces.entry(face).or_default() += 1;
                        }
                        tetrahedra.push(Tetrahedron {
                            nodes,
                            slab,
                            planar_triangle: index,
                            epsilon_r: *epsilon_r,
                            material_label: label.clone(),
                            volume_m3: volume,
                            gradients_per_m: gradients,
                        });
                    }
                }
            }
        }
    }
    if all_faces.values().any(|&n| n > 2) {
        return Err(error(
            "MESH_NONMANIFOLD",
            "face has more than two incident tetrahedra",
        ));
    }
    let mut copper_interfaces = Vec::new();
    let mut external_faces = Vec::new();
    for (face, count) in dielectric_faces {
        if count != 1 {
            continue;
        }
        if let Some(sources) = copper_faces.get(&face) {
            copper_interfaces.push(InterfaceFace {
                nodes: face,
                copper_source_ids: sources.iter().cloned().collect(),
            });
        } else if all_faces[&face] == 1 {
            external_faces.push(face);
        } else {
            return Err(error(
                "MESH_CRACK",
                "internal dielectric boundary has no copper neighbor",
            ));
        }
    }
    Ok(LayeredMesh {
        nodes_m,
        tetrahedra,
        copper_interfaces,
        external_faces,
        dielectric_volume_m3,
        copper_volume_m3,
        planned_peak_bytes: planned,
    })
}

pub fn stiffness(tet: &Tetrahedron) -> Result<[[f64; 4]; 4]> {
    let scale = super::super::EPS0 * tet.epsilon_r * tet.volume_m3;
    let matrix = std::array::from_fn(|i| {
        std::array::from_fn(|j| scale * dot(tet.gradients_per_m[i], tet.gradients_per_m[j]))
    });
    if matrix.iter().flatten().all(|x| x.is_finite()) {
        Ok(matrix)
    } else {
        Err(error("FIELD_RANGE", "tetra stiffness overflow"))
    }
}
