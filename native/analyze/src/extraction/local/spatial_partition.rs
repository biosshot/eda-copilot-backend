//! Owner-local copper coordinates welded to one original physical field space.
use super::{
    accurate::Sum, error, mesh::LayeredMesh, volume_current::Space, AllocationBudget, Result,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const LIMIT: f64 = 1e-8;

struct Owner {
    global: Vec<usize>,
    trace: Vec<Option<usize>>,
}
struct OwnedCell {
    owner: usize,
    local: [usize; 4],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Face {
    nodes: [usize; 3],
    outward_positive_owner: usize,
    outward_negative_owner: usize,
}

pub(super) struct Partition {
    owners: Vec<Owner>,
    cells: Vec<OwnedCell>,
    traces: Vec<usize>,
    trace_owner_masks: Vec<u16>,
    faces: Vec<Face>,
    pub planned_bytes: usize,
    pub compact: bool,
}

fn deadline(start: Instant, wall: Duration) -> Result<()> {
    if start.elapsed() >= wall {
        return Err(error(
            "SHARED_RETURN_DEADLINE",
            "shared spatial interface deadline",
        ));
    }
    Ok(())
}

fn face_orientation(mesh: &LayeredMesh, nodes: [usize; 3], opposite: usize) -> Result<bool> {
    let [a, b, c] = nodes.map(|i| mesh.nodes_m[i]);
    let diff = |p: [f64; 3]| std::array::from_fn::<_, 3, _>(|k| p[k] - a[k]);
    let [u, v, w] = [diff(b), diff(c), diff(mesh.nodes_m[opposite])];
    let normal = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let inward = (0..3).map(|k| normal[k] * w[k]).sum::<f64>();
    if !inward.is_finite() || inward == 0. {
        return Err(error(
            "SHARED_RETURN_FACE",
            "degenerate oriented copper face",
        ));
    }
    Ok(inward < 0.)
}

impl Partition {
    pub fn new(
        mesh: &LayeredMesh,
        space: &Space,
        owner_by_tet: &[usize],
        columns: usize,
        max_output_bytes: usize,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<Self> {
        Self::new_impl(
            mesh,
            space,
            owner_by_tet,
            columns,
            max_output_bytes,
            budget,
            start,
            wall,
            false,
        )
    }

    fn new_impl(
        mesh: &LayeredMesh,
        space: &Space,
        owner_by_tet: &[usize],
        columns: usize,
        max_output_bytes: usize,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
        compact: bool,
    ) -> Result<Self> {
        let cells_count = mesh.copper_tetrahedra.len();
        let potentials = space.potential_count();
        let owners_count = owner_by_tet
            .iter()
            .copied()
            .max()
            .and_then(|owner| owner.checked_add(1))
            .unwrap_or(0);
        if owner_by_tet.len() != cells_count
            || space.cells.len() != cells_count
            || !(2..=16).contains(&owners_count)
            || columns == 0
        {
            return Err(error(
                "SHARED_RETURN_OWNERSHIP",
                "exact original Cu tetrahedron ownership and 2..16 owners required",
            ));
        }
        // Bound maps for four original faces and four local nodes per cell
        // before building them. Shared observer scratch is charged separately.
        let topology_bytes = cells_count
            .checked_mul(2048)
            .and_then(|n| n.checked_add(potentials.checked_mul(128)?))
            .ok_or_else(|| error("MEMORY_LIMIT", "shared return topology plan overflow"))?;
        budget.check(topology_bytes)?;
        let mut local_maps = vec![BTreeMap::<usize, usize>::new(); owners_count];
        let mut cells = Vec::with_capacity(cells_count);
        let mut occurrences = BTreeMap::<[usize; 3], (usize, bool, bool)>::new();
        let mut faces = Vec::new();
        for (i, (tet, cell)) in mesh.copper_tetrahedra.iter().zip(&space.cells).enumerate() {
            deadline(start, wall)?;
            if cell.mesh_cell != i {
                return Err(error(
                    "SHARED_RETURN_OWNERSHIP",
                    "native cell order differs from original Cu tetrahedra",
                ));
            }
            let owner = owner_by_tet[i];
            let local_map = &mut local_maps[owner];
            let local = cell.potentials.map(|global| {
                let next = local_map.len();
                *local_map.entry(global).or_insert(next)
            });
            cells.push(OwnedCell { owner, local });
            for opposite in 0..4 {
                let mut nodes = [0; 3];
                for (k, (_, &node)) in tet
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != opposite)
                    .enumerate()
                {
                    nodes[k] = node;
                }
                nodes.sort_unstable();
                let orientation = face_orientation(mesh, nodes, tet.nodes[opposite])?;
                if let Some((first, positive, paired)) = occurrences.get_mut(&nodes) {
                    if *paired || *positive == orientation {
                        return Err(error(
                            "SHARED_RETURN_FACE",
                            "nonmanifold or inconsistent copper face orientation",
                        ));
                    }
                    *paired = true;
                    if *first != owner {
                        let (outward_positive_owner, outward_negative_owner) = if *positive {
                            (*first, owner)
                        } else {
                            (owner, *first)
                        };
                        faces.push(Face {
                            nodes,
                            outward_positive_owner,
                            outward_negative_owner,
                        });
                    }
                } else {
                    occurrences.insert(nodes, (owner, orientation, false));
                }
            }
        }
        drop(occurrences);
        if !compact && (faces.is_empty() || local_maps.iter().any(BTreeMap::is_empty)) {
            return Err(error(
                "SHARED_RETURN_OWNERSHIP",
                "nonempty contiguous owners and conforming shared copper faces required",
            ));
        }
        let mut touches = vec![0; potentials];
        for local in &local_maps {
            for &global in local.keys() {
                touches[global] += 1;
            }
        }
        let traces: Vec<_> = touches
            .iter()
            .enumerate()
            .filter_map(|(global, &n)| (n > 1).then_some(global))
            .collect();
        let memberships = traces.iter().map(|&global| touches[global]).sum::<usize>();
        let trace_owner_masks = traces
            .iter()
            .map(|global| {
                local_maps
                    .iter()
                    .enumerate()
                    .fold(0u16, |mask, (owner, map)| {
                        mask | if map.contains_key(global) {
                            1 << owner
                        } else {
                            0
                        }
                    })
            })
            .collect();

        let output_bytes = if compact {
            4096
        } else {
            traces
                .len()
                .checked_mul(512)
                .and_then(|n| n.checked_add(memberships.checked_mul(192)?))
                .and_then(|n| n.checked_mul(columns))
                .and_then(|n| n.checked_add(faces.len().checked_mul(384)?))
                .and_then(|n| n.checked_add(4096))
                .filter(|&n| n <= max_output_bytes)
                .ok_or_else(|| {
                    error(
                        "SHARED_RETURN_OUTPUT",
                        "distributed trace output exceeds budget",
                    )
                })?
        };
        let planned_bytes = topology_bytes
            .checked_add(output_bytes)
            .and_then(|n| n.checked_add(traces.len().checked_mul(owners_count)?.checked_mul(128)?))
            .and_then(|n| {
                n.checked_add(
                    space
                        .current_count()
                        .checked_add(potentials)?
                        .checked_mul(128)?,
                )
            })
            .ok_or_else(|| error("MEMORY_LIMIT", "shared return observer plan overflow"))?;
        budget.check(planned_bytes)?;
        let owners = local_maps
            .into_iter()
            .map(|map| {
                let mut global = vec![0; map.len()];
                let mut trace = vec![None; map.len()];
                for (g, l) in map {
                    global[l] = g;
                    trace[l] = traces.binary_search(&g).ok();
                }
                Owner { global, trace }
            })
            .collect();
        Ok(Self {
            owners,
            cells,
            traces,
            trace_owner_masks,
            faces,
            planned_bytes,
            compact,
        })
    }

    /// Geometry-only binary candidate; eligibility uses the solver's unchanged
    /// retained physical electrodes and floating component coordinates.
    pub fn automatic(
        mesh: &LayeredMesh,
        space: &Space,
        retained: &std::collections::BTreeSet<usize>,
        budget: AllocationBudget,
        start: Instant,
        wall: Duration,
    ) -> Result<(Option<Self>, Value)> {
        let count = mesh.copper_tetrahedra.len();
        let bytes = count
            .checked_mul(2048)
            .and_then(|v| v.checked_add(space.potential_count().checked_mul(128)?))
            .ok_or_else(|| error("MEMORY_LIMIT", "automatic partition topology overflow"))?;
        budget.check(bytes)?;
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for tet in &mesh.copper_tetrahedra {
            deadline(start, wall)?;
            for &node in &tet.nodes {
                for k in 0..3 {
                    let v = mesh.nodes_m[node][k];
                    if !v.is_finite() {
                        return Err(error(
                            "SHARED_RETURN_OWNERSHIP",
                            "finite original coordinates required",
                        ));
                    }
                    low[k] = low[k].min(v);
                    high[k] = high[k].max(v);
                }
            }
        }
        let mut axis = 0;
        for k in 1..3 {
            if high[k] - low[k] > high[axis] - low[axis] {
                axis = k;
            }
        }
        let middle = low[axis] * 0.5 + high[axis] * 0.5;
        let mut ids = Vec::with_capacity(count);
        for tet in &mesh.copper_tetrahedra {
            deadline(start, wall)?;
            let center = tet
                .nodes
                .iter()
                .map(|&i| mesh.nodes_m[i][axis] * 0.25)
                .sum::<f64>();
            ids.push(usize::from(center > middle));
        }
        // Noncryptographic topology stamp. Cache producer and geometry hashes
        // remain authoritative; include the actual finite electrode quotient.
        let mut hash = 0xcbf29ce484222325u64;
        let mut stamp = |v: u64| {
            for byte in v.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        };
        stamp(mesh.nodes_m.len() as u64);
        stamp(count as u64);
        for xyz in &mesh.nodes_m {
            deadline(start, wall)?;
            for v in xyz {
                stamp(v.to_bits());
            }
        }
        for (i, tet) in mesh.copper_tetrahedra.iter().enumerate() {
            deadline(start, wall)?;
            for &node in &tet.nodes {
                stamp(node as u64);
            }
            stamp(ids[i] as u64);
            for &potential in &space.cells[i].potentials {
                stamp(potential as u64);
            }
        }
        stamp(retained.len() as u64);
        for &i in retained {
            stamp(i as u64);
        }
        let mut evidence = json!({"policy":"automatic-conductive-schur-v1", "status":"not-applicable",
            "algorithm":"longest-extent-midpoint-v1", "meshOwnershipSignature":format!("fnv1a64:{hash:016x}"),
            "owners":if count == 0 {0} else {2}, "originalCopperTetrahedra":count,
            "crossOwnerFaces":0, "freeSharedTraceDofs":0, "ownerInteriorDofs":if count == 0 {vec![]} else {vec![0usize;2]},
            "reason":"binary spatial candidate has no finite interface and free interiors on both owners"});
        if !ids.contains(&0) || !ids.contains(&1) {
            return Ok((None, evidence));
        }
        let partition =
            Self::new_impl(mesh, space, &ids, 1, usize::MAX, budget, start, wall, true)?;
        let free_trace = partition
            .traces
            .iter()
            .filter(|i| !retained.contains(i))
            .count();
        let interiors: Vec<_> = partition
            .owners
            .iter()
            .map(|owner| {
                owner
                    .global
                    .iter()
                    .filter(|i| !retained.contains(i) && partition.traces.binary_search(i).is_err())
                    .count()
            })
            .collect();
        evidence["crossOwnerFaces"] = json!(partition.faces.len());
        evidence["freeSharedTraceDofs"] = json!(free_trace);
        evidence["ownerInteriorDofs"] = json!(interiors);
        if partition.faces.is_empty() || free_trace == 0 || interiors.iter().any(|&n| n == 0) {
            return Ok((None, evidence));
        }
        evidence["status"] = json!("selected");
        evidence["reason"] = json!("finite original shared faces, free shared trace and free interiors on both spatial owners");
        Ok((Some(partition), evidence))
    }

    pub fn owner_count(&self) -> usize {
        self.owners.len()
    }

    pub fn current_owner(&self, original_current: usize) -> Option<usize> {
        self.cells.get(original_current / 3).map(|cell| cell.owner)
    }

    pub fn trace_dofs(&self) -> &[usize] {
        &self.traces
    }

    pub fn weld(&self, cell: usize) -> [usize; 4] {
        let c = &self.cells[cell];
        c.local.map(|index| self.owners[c.owner].global[index])
    }

    pub fn topology(&self) -> Value {
        json!({
            "owners":self.owners.len(),"originalCopperTetrahedra":self.cells.len(),
            "tracePotentialDofs":self.traces,"orientedCrossOwnerFaces":self.faces,
            "fieldOwnership":"single common electric and magnetic actions on the original mesh",
            "reactionConvention":"owner weak nodal conduction dual in A; not pointwise face-normal flux",
            "plannedBytes":self.planned_bytes,
        })
    }

    pub fn observe(
        &self,
        space: &Space,
        v: [&[f64]; 2],
        q: [&[f64]; 2],
        c: [&[f64]; 2],
        injected: [&[f64]; 2],
        omega: f64,
        copper_loss_w: f64,
        start: Instant,
        wall: Duration,
    ) -> Result<Value> {
        if !omega.is_finite()
            || v.into_iter().chain(c).chain(injected).any(|row| {
                row.len() != space.potential_count() || row.iter().any(|value| !value.is_finite())
            })
        {
            return Err(error(
                "SHARED_RETURN_BALANCE",
                "finite complete original potential, charge and injection rows required",
            ));
        }
        let original = [space.divergence(q[0])?, space.divergence(q[1])?];
        let mut reactions = vec![vec![[Sum::default(); 2]; self.traces.len()]; self.owners.len()];
        let mut owner_loss = vec![Sum::default(); self.owners.len()];
        for (i, cell) in self.cells.iter().enumerate() {
            if i % 1024 == 0 {
                deadline(start, wall)?;
            }
            let full = &space.cells[i];
            for phase in 0..2 {
                for axis in 0..3 {
                    let current = q[phase][3 * i + axis];
                    owner_loss[cell.owner] = owner_loss[cell.owner]
                        .add(Sum::from(current).mul(current).mul(full.resistance_ohm));
                }
                let mut nodal = [Sum::default(); 4];
                for k in 1..4 {
                    nodal[k] = (0..3).fold(Sum::default(), |s, axis| {
                        s.add(Sum::from(q[phase][3 * i + axis]).mul(full.incidence[k][axis]))
                    });
                }
                nodal[0] = nodal[1].add(nodal[2]).add(nodal[3]).neg();
                for k in 0..4 {
                    if let Some(t) = self.owners[cell.owner].trace[cell.local[k]] {
                        reactions[cell.owner][t][phase] =
                            reactions[cell.owner][t][phase].add(nodal[k]);
                    }
                }
            }
        }
        let displacement = |i: usize| [-omega * c[1][i], omega * c[0][i]];
        let scale = (0..space.potential_count())
            .flat_map(|i| (0..2).map(move |phase| (i, phase)))
            .map(|(i, phase)| {
                original[phase][i].abs() + displacement(i)[phase].abs() + injected[phase][i].abs()
            })
            .fold(0., f64::max)
            .max(f64::MIN_POSITIVE);
        if !scale.is_finite() {
            return Err(error(
                "SHARED_RETURN_BALANCE",
                "nonfinite original KCL scale",
            ));
        }
        let mut rows = Vec::with_capacity(self.traces.len());
        let mut max_weld: f64 = 0.;
        let mut max_kcl: f64 = 0.;
        for (t, &i) in self.traces.iter().enumerate() {
            deadline(start, wall)?;
            let mut total = [Sum::default(); 2];
            let mut owner_duals = Vec::new();
            for owner in 0..self.owners.len() {
                if self.trace_owner_masks[t] & (1 << owner) == 0 {
                    continue;
                }
                let r = reactions[owner][t];
                total = [total[0].add(r[0]), total[1].add(r[1])];
                owner_duals.push(json!({"owner":owner,"weakConductionA":r.map(Sum::value)}));
            }
            let total = total.map(Sum::value);
            let disp = displacement(i);
            let external = [injected[0][i], injected[1][i]];
            for phase in 0..2 {
                let weld = (total[phase] - original[phase][i]).abs() / scale;
                let kcl = (total[phase] + disp[phase] - external[phase]).abs() / scale;
                if !weld.is_finite() || !kcl.is_finite() || weld > LIMIT || kcl > LIMIT {
                    return Err(error(
                        "SHARED_RETURN_BALANCE",
                        &format!("trace dof={i}, weld={weld:e}, KCL={kcl:e}"),
                    ));
                }
                max_weld = max_weld.max(weld);
                max_kcl = max_kcl.max(kcl);
            }
            rows.push(
                json!({"potentialDof":i,"voltageV":[v[0][i],v[1][i]],"ownerDuals":owner_duals,
                "commonDisplacementA":disp,"externalInjectionA":external}),
            );
        }
        let total_loss = owner_loss
            .iter()
            .fold(Sum::default(), |sum, &loss| sum.add(loss))
            .value();
        let owner_loss = owner_loss.into_iter().map(Sum::value).collect::<Vec<_>>();
        if !total_loss.is_finite()
            || !copper_loss_w.is_finite()
            || owner_loss
                .iter()
                .any(|loss| !loss.is_finite() || *loss < 0.)
            || (total_loss - copper_loss_w).abs() > LIMIT * total_loss.max(copper_loss_w).max(1e-18)
        {
            return Err(error(
                "SHARED_RETURN_BALANCE",
                "exclusive owner Joule loss differs from the original solved loss",
            ));
        }
        Ok(json!({"maxWeldRelative":max_weld,"maxKclRelative":max_kcl,
            "ownerJouleLossW":owner_loss,"traces":rows}))
    }
}

#[cfg(test)]
mod automatic_tests {
    use super::*;
    #[test]
    fn automatic_partition_is_geometry_based_frozen_and_refuses_unusable_free_sets() {
        use super::super::mesh::{extrude, LayeredInput, Material};
        let budget = AllocationBudget {
            max_bytes: 8 * 1024 * 1024,
            max_nodes: 1000,
            max_tetrahedra: 1000,
        };
        let xy_m = (0..=4)
            .flat_map(|i| [[i as f64 * 0.5 - 1., 0.], [i as f64 * 0.5 - 1., 0.25]])
            .collect();
        let triangles: Vec<_> = (0..4)
            .flat_map(|i| {
                [
                    [2 * i, 2 * i + 2, 2 * i + 1],
                    [2 * i + 2, 2 * i + 3, 2 * i + 1],
                ]
            })
            .collect();
        let boundary_edges = vec![
            [0, 2],
            [2, 4],
            [4, 6],
            [6, 8],
            [8, 9],
            [9, 7],
            [7, 5],
            [5, 3],
            [3, 1],
            [1, 0],
        ];
        let mesh = extrude(
            &LayeredInput {
                xy_m,
                materials: vec![vec![
                    Material::Copper {
                        source_ids: vec!["authored".into()]
                    };
                    triangles.len()
                ]],
                triangles,
                boundary_edges,
                z_m: vec![0., 0.1],
            },
            budget,
        )
        .unwrap();
        let space = super::super::volume_current::assemble(
            &mesh,
            &vec![58e6; mesh.copper_tetrahedra.len()],
            &[],
            budget,
        )
        .unwrap();
        let retained = std::collections::BTreeSet::from([space.components[0][0]]);
        let start = Instant::now();
        let wall = Duration::from_secs(5);
        let (chosen, evidence) =
            Partition::automatic(&mesh, &space, &retained, budget, start, wall).unwrap();
        let partition = chosen.unwrap();
        assert!(partition.compact);
        assert_eq!(evidence["status"], "selected");
        assert_eq!(evidence["owners"], 2);
        assert!(evidence["crossOwnerFaces"].as_u64().unwrap() > 0);
        assert!(evidence["ownerInteriorDofs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.as_u64().unwrap() > 0));
        assert_eq!(
            space
                .cells
                .iter()
                .enumerate()
                .map(|(i, c)| (partition.weld(i), c.potentials))
                .filter(|(a, b)| a != b)
                .count(),
            0
        );
        let (_, repeat) =
            Partition::automatic(&mesh, &space, &retained, budget, start, wall).unwrap();
        assert_eq!(repeat, evidence);
        let all = (0..space.potential_count()).collect();
        let (chosen, unavailable) =
            Partition::automatic(&mesh, &space, &all, budget, start, wall).unwrap();
        assert!(chosen.is_none());
        assert_eq!(unavailable["status"], "not-applicable");
        assert_ne!(
            unavailable["meshOwnershipSignature"],
            evidence["meshOwnershipSignature"]
        );
        assert!(Partition::automatic(
            &mesh,
            &space,
            &retained,
            AllocationBudget {
                max_bytes: 1,
                ..budget
            },
            start,
            wall
        )
        .is_err());
        assert!(
            Partition::automatic(&mesh, &space, &retained, budget, start, Duration::ZERO).is_err()
        );
    }
}
