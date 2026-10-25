//! Bounded conforming foil current/potential assembly. No barrel/PCB admission.
//! Reuses the accepted finite-volume magnetic and product-space electric kernels.
use super::{
    error,
    layered_capacitance::Solution,
    mesh::validate_planar_embedding_xy,
    polyhedron::{self, PointPotential},
    prism::{GaussRule, Prism},
    sheet::SheetCell,
    Result,
};
use crate::electrical::{
    coupled::{
        physical::{Node, PortTerminals, Terminal},
        Model, System,
    },
    Budget as ElectricalBudget,
};
use geo::{LineString, Polygon, Relate};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, serde::Serialize)]
pub struct Foil {
    pub source_id: String,
    pub xy_m: Vec<[f64; 2]>,
    pub triangles: Vec<[usize; 3]>,
    /// Explicit finite electrode quotient. Equal IDs prescribe equipotential
    /// vertices; they are never inferred from source/net names or proximity.
    pub potential_dofs: Vec<usize>,
    pub z_m: [f64; 2],
    pub conductivity_s_per_m: f64,
}
#[derive(Clone, Copy)]
pub struct Budget {
    pub max_bytes: usize,
    pub max_wall: Duration,
}
pub struct CurrentSpace {
    pub model: Model,
    pub cells: Vec<SheetCell>,
    /// Directed finite-volume results retain quadrature asymmetry diagnostics.
    pub directed_kernel_m5: Vec<Vec<f64>>,
    pub max_quadrature_change: f64,
    pub max_reciprocity_error: f64,
    pub planned_bytes: usize,
    pub elapsed_seconds: f64,
    pub band_hz: [f64; 2],
}
fn check(start: Instant, budget: Budget) -> Result<()> {
    if start.elapsed() > budget.max_wall {
        Err(error("DEADLINE", "whole current-space assembly deadline"))
    } else {
        Ok(())
    }
}
fn entries(a: &[Vec<f64>]) -> Vec<(usize, usize, f64)> {
    a.iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.iter()
                .enumerate()
                .filter(|(_, v)| **v != 0.)
                .map(move |(j, &v)| (i, j, v))
        })
        .collect()
}
fn interpolation(foil: &Foil, p: [f64; 2]) -> Result<BTreeMap<usize, f64>> {
    let mut found: Option<BTreeMap<usize, f64>> = None;
    for t in &foil.triangles {
        let [a, b, c] = t.map(|i| foil.xy_m[i]);
        let det = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if !det.is_finite() || det <= 0. {
            return Err(error(
                "CURRENT_INPUT",
                "counterclockwise nondegenerate foil cells required",
            ));
        }
        let u = ((p[0] - a[0]) * (c[1] - a[1]) - (p[1] - a[1]) * (c[0] - a[0])) / det;
        let v = ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / det;
        let w = [1. - u - v, u, v];
        // Roundoff allowance only verifies supplied interpolation; no geometry
        // snapping or weights used in the physical matrix are changed.
        if w.iter().any(|x| *x < -1e-12) {
            continue;
        }
        let mut map = BTreeMap::new();
        for k in 0..3 {
            *map.entry(foil.potential_dofs[t[k]]).or_insert(0.) += w[k];
        }
        if let Some(old) = &found {
            if old
                .keys()
                .chain(map.keys())
                .any(|i| (old.get(i).unwrap_or(&0.) - map.get(i).unwrap_or(&0.)).abs() > 1e-12)
            {
                return Err(error(
                    "CURRENT_TRACE",
                    "discontinuous potential trace across shared edge",
                ));
            }
        } else {
            found = Some(map);
        }
    }
    found.ok_or_else(|| {
        error(
            "CURRENT_TRACE",
            "electric surface lies outside retained current mesh",
        )
    })
}
/// Allocation/scope admission before either field factorization or dense
/// magnetic integration. Count-only PCB meshes can be rejected here; passing
/// counts never establishes mesh conformity, ports, contacts or applicability.
pub fn preflight_counts(
    supports: usize,
    potentials: usize,
    field_plan_bytes: usize,
    budget: Budget,
) -> Result<usize> {
    if supports == 0 || supports > 64 || potentials == 0 || potentials > 64 {
        return Err(error(
            "CURRENT_SCOPE",
            "experimental all-foil limit64 supports/64 potentials; PCB/via space not admitted",
        ));
    }
    let ni = supports
        .checked_mul(2)
        .ok_or_else(|| error("MEMORY_LIMIT", "current count overflow"))?;
    let plan = potentials
        .checked_add(ni)
        .and_then(|n| n.checked_pow(2))
        .and_then(|n| n.checked_mul(1600))
        .and_then(|n| n.checked_add(field_plan_bytes))
        .and_then(|n| n.checked_add(supports.checked_mul(32768)?))
        .ok_or_else(|| error("MEMORY_LIMIT", "current-space allocation overflow"))?;
    if plan > budget.max_bytes {
        return Err(error(
            "MEMORY_LIMIT",
            "simultaneous field/current/P6 plan exceeds request cap",
        ));
    }
    Ok(plan)
}
/// Full spatial C is supplied only by the accepted field solver with exactly
/// the same physical surfaces and potential trace. No guessed capacitor exists.
/// All-foil experimental limit: 64 supports and 64 potential coordinates.
/// This is not an extractor for vias, arbitrary ROI copper, or lossy dielectrics.
pub fn assemble(
    foils: &[Foil],
    electric: &Solution,
    ports: &[[usize; 2]],
    max_frequency_hz: f64,
    budget: Budget,
) -> Result<CurrentSpace> {
    let start = Instant::now();
    check(start, budget)?;
    if !max_frequency_hz.is_finite() || max_frequency_hz <= 0. || max_frequency_hz > 1e5 {
        return Err(error(
            "CURRENT_BAND",
            "explicit thin-volume screening band0..fmax<=100kHz required",
        ));
    }
    let nv = electric.source_ids.len();
    let supports = foils
        .iter()
        .try_fold(0usize, |n, f| n.checked_add(f.triangles.len()))
        .ok_or_else(|| error("MEMORY_LIMIT", "current support count overflow"))?;
    if foils.is_empty()
        || supports == 0
        || supports > 64
        || nv == 0
        || nv > 64
        || ports.is_empty()
        || ports.len() > 16
    {
        return Err(error(
            "CURRENT_SCOPE",
            "bounded conforming all-foil space required; no barrel admission",
        ));
    }
    let plan = preflight_counts(
        supports,
        nv,
        electric.plan.planned_simultaneous_bytes,
        budget,
    )?;
    if foils
        .iter()
        .map(|f| &f.source_id)
        .collect::<BTreeSet<_>>()
        .len()
        != foils.len()
        || electric.metals().len() != foils.len()
    {
        return Err(error(
            "CURRENT_TRACE",
            "each electric metal needs exactly one complete foil mesh",
        ));
    }
    let mut used = vec![false; nv];
    for f in foils {
        check(start, budget)?;
        if f.xy_m.len() != f.potential_dofs.len()
            || f.triangles.iter().flatten().any(|&i| i >= f.xy_m.len())
            || f.potential_dofs.iter().any(|&i| i >= nv)
            || !f.xy_m.iter().flatten().all(|v| v.is_finite())
        {
            return Err(error(
                "CURRENT_INPUT",
                "finite complete indexed foil geometry required",
            ));
        }
        validate_planar_embedding_xy(&f.xy_m, &f.triangles)?;
        let thickness = f.z_m[1] - f.z_m[0];
        let delta = (2.
            / (2.
                * std::f64::consts::PI
                * max_frequency_hz
                * crate::extraction::MU0
                * f.conductivity_s_per_m))
            .sqrt();
        if !thickness.is_finite()
            || thickness <= 0.
            || !f.conductivity_s_per_m.is_finite()
            || f.conductivity_s_per_m <= 0.
            || !delta.is_finite()
            || delta < 3. * thickness
        {
            return Err(error(
                "CURRENT_BAND",
                "finite fixed copper material and delta>=3*thickness required",
            ));
        }
        for t in &f.triangles {
            let [a, b, c] = t.map(|i| f.xy_m[i]);
            let twice = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            let longest = (a[0] - b[0])
                .hypot(a[1] - b[1])
                .max((b[0] - c[0]).hypot(b[1] - c[1]))
                .max((c[0] - a[0]).hypot(c[1] - a[1]));
            if !twice.is_finite() || twice <= 0. || thickness > twice / longest / 4. {
                return Err(error(
                    "CURRENT_BAND",
                    "thin foil requires positive cell altitude>=4*thickness; no sliver removal",
                ));
            }
        }
        let metal = electric
            .metals()
            .iter()
            .find(|m| m.source_id == f.source_id)
            .ok_or_else(|| error("CURRENT_TRACE", "electric/current source mismatch"))?;
        if f.z_m != [metal.z0_m, metal.z1_m] {
            return Err(error(
                "CURRENT_TRACE",
                "finite foil z extent differs from electric metal",
            ));
        }
        // Equal covered area alone is insufficient. The electric trace is also
        // checked below at every surface node against this P1 current mesh.
        let area: f64 = f
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| f.xy_m[i]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.
            })
            .sum();
        let polygon = Polygon::new(LineString::from(metal.polygon_m.clone()), vec![]);
        let mut referenced = vec![false; f.xy_m.len()];
        for t in &f.triangles {
            let triangle = Polygon::new(LineString::from(t.map(|i| f.xy_m[i]).to_vec()), vec![]);
            let relation = triangle.relate(&polygon);
            if !relation.is_within() && !relation.is_equal_topo() {
                return Err(error(
                    "CURRENT_TRACE",
                    "current cell extends outside exact electric metal",
                ));
            }
            for &i in t {
                referenced[i] = true;
            }
        }
        if referenced.iter().any(|x| !*x) {
            return Err(error("CURRENT_INPUT", "unused geometric current node"));
        }
        let mut poly = 0.;
        for i in 0..metal.polygon_m.len() {
            let a = metal.polygon_m[i];
            let b = metal.polygon_m[(i + 1) % metal.polygon_m.len()];
            poly += a[0] * b[1] - a[1] * b[0];
        }
        if area <= 0. || (area - poly.abs() / 2.).abs() > 1e-10 * area {
            return Err(error(
                "CURRENT_TRACE",
                "foil coverage area differs from electric metal",
            ));
        }
        for &i in &f.potential_dofs {
            used[i] = true;
        }
    }
    if used.iter().any(|x| !*x) {
        return Err(error(
            "CURRENT_TRACE",
            "potential coordinate without current mesh",
        ));
    }
    for node in electric.surface_traces() {
        check(start, budget)?;
        let foil = foils
            .iter()
            .find(|f| f.source_id == node.metal_source_id)
            .unwrap();
        let map = interpolation(foil, [node.position_m[0], node.position_m[1]])?;
        let weights: BTreeMap<_, _> = node.weights.into_iter().collect();
        if map
            .keys()
            .chain(weights.keys())
            .any(|i| (map.get(i).unwrap_or(&0.) - weights.get(i).unwrap_or(&0.)).abs() > 1e-12)
        {
            return Err(error(
                "CURRENT_TRACE",
                "field trace and current P1 potential coordinates differ",
            ));
        }
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for f in foils {
        for p in &f.xy_m {
            for z in f.z_m {
                for (i, v) in [p[0], p[1], z].iter().enumerate() {
                    lo[i] = lo[i].min(*v);
                    hi[i] = hi[i].max(*v);
                }
            }
        }
    }
    let d = (hi[0] - lo[0]).hypot(hi[1] - lo[1]).hypot(hi[2] - lo[2]);
    if 2. * std::f64::consts::PI * max_frequency_hz / 299792458.
        * d
        * electric.max_epsilon_r().sqrt()
        > 0.1
    {
        return Err(error(
            "CURRENT_BAND",
            "quasistatic k0*D*sqrt(epsilon)<=0.1 screening failed",
        ));
    }
    let mut model = Model {
        format: "coupled-rlc".into(),
        reference_topology: "infinity".into(),
        potential_nodes: electric.source_ids.clone(),
        current_dofs: vec![],
        incidence: vec![],
        port_incidence: vec![],
        resistance_ohm: vec![],
        inductance_h: vec![],
        capacitance_f: entries(&electric.energy_capacitance_f()?),
        conserved_potential_modes: vec![],
        port_terminals: Some(vec![]),
    };
    let mut cells = vec![];
    let mut prisms = vec![];
    let mut parent: Vec<_> = (0..nv).collect();
    fn root(p: &[usize], mut i: usize) -> usize {
        while p[i] != i {
            i = p[i];
        }
        i
    }
    for f in foils {
        for t in &f.triangles {
            check(start, budget)?;
            let cell = SheetCell::new(
                f.source_id.clone(),
                t.map(|i| f.xy_m[i]),
                f.z_m[1] - f.z_m[0],
                f.conductivity_s_per_m,
            )?;
            let base = 2 * cells.len();
            for k in 0..3 {
                for axis in 0..2 {
                    model.incidence.push((
                        f.potential_dofs[t[k]],
                        base + axis,
                        cell.incidence[k][axis],
                    ));
                }
            }
            for axis in 0..2 {
                model.current_dofs.push(format!(
                    "{}:triangle{}:{}",
                    f.source_id,
                    cells.len(),
                    axis
                ));
                model
                    .resistance_ohm
                    .push((base + axis, base + axis, cell.resistance_ohm));
            }
            for k in 1..3 {
                let a = root(&parent, f.potential_dofs[t[0]]);
                let b = root(&parent, f.potential_dofs[t[k]]);
                parent[b] = a;
            }
            prisms.push(Prism::new(f.source_id.clone(), cell.triangle_m, f.z_m)?);
            cells.push(cell);
        }
    }
    // Electrode quotients can accumulate multiple element entries into a row.
    let mut incidence = BTreeMap::new();
    for (i, j, v) in model.incidence.drain(..) {
        *incidence.entry((i, j)).or_insert(0.) += v;
    }
    model.incidence = incidence
        .into_iter()
        .filter(|(_, v)| *v != 0.)
        .map(|((i, j), v)| (i, j, v))
        .collect();
    let components: BTreeSet<_> = (0..nv).map(|i| root(&parent, i)).collect();
    for c in components {
        model.conserved_potential_modes.push(
            (0..nv)
                .map(|i| if root(&parent, i) == c { 1. } else { 0. })
                .collect(),
        );
    }
    for (p, &[a, b]) in ports.iter().enumerate() {
        if a >= nv || b >= nv || a == b {
            return Err(error(
                "CURRENT_PORT",
                "distinct finite positive/negative electrodes required",
            ));
        }
        model.port_incidence.extend([(a, p, 1.), (b, p, -1.)]);
        model.port_terminals.as_mut().unwrap().push(PortTerminals {
            positive: Terminal::Node(Node {
                node: model.potential_nodes[a].clone(),
            }),
            negative: Terminal::Node(Node {
                node: model.potential_nodes[b].clone(),
            }),
        });
    }
    let sources = prisms
        .iter()
        .map(PointPotential::new)
        .collect::<Result<Vec<_>>>()?;
    let mut matrices = vec![];
    for (xy, z) in [(12, 8), (24, 16)] {
        let xr = GaussRule::new(xy)?;
        let zr = GaussRule::new(z)?;
        let mut m = vec![vec![0.; supports]; supports];
        for i in 0..supports {
            for j in 0..supports {
                check(start, budget)?;
                m[i][j] =
                    polyhedron::volume_integral_anisotropic(&prisms[i], &sources[j], &xr, &zr)?;
            }
        }
        matrices.push(m);
    }
    let final_m = matrices.pop().unwrap();
    let coarse = matrices.pop().unwrap();
    let mut quad: f64 = 0.;
    let mut reciprocity: f64 = 0.;
    let self_scale = (0..supports)
        .map(|i| final_m[i][i] * cells[i].current_density_per_amp_m2.powi(2))
        .fold(0., f64::max);
    for i in 0..supports {
        for j in 0..supports {
            let v = final_m[i][j];
            let weight = cells[i].current_density_per_amp_m2 * cells[j].current_density_per_amp_m2
                / self_scale;
            quad = quad.max((v - coarse[i][j]).abs() * weight);
            reciprocity = reciprocity.max((v - final_m[j][i]).abs() * weight);
        }
    }
    if quad > 0.002 || reciprocity > 1e-9 {
        return Err(error("CURRENT_QUADRATURE", &format!("quadrature={quad:e}, reciprocity={reciprocity:e}; unchanged0.2% refinement/1e-9 raw reciprocity gates")));
    }
    for i in 0..supports {
        for j in i..supports {
            for axis in 0..2 {
                // One canonical directed integral per unordered pair. Raw reverse and
                // refinement values remain diagnostic; no averaging, clipping or gmin.
                let value = 1e-7
                    * final_m[i][j]
                    * cells[i].current_density_per_amp_m2
                    * cells[j].current_density_per_amp_m2;
                model.inductance_h.push((2 * i + axis, 2 * j + axis, value));
                if i != j {
                    model.inductance_h.push((2 * j + axis, 2 * i + axis, value));
                }
            }
        }
    }
    check(start, budget)?;
    let eb = ElectricalBudget {
        max_memory_mi_b: Some(budget.max_bytes as f64 / 1048576.),
        max_seconds: Some(budget.max_wall.as_secs_f64()),
    };
    System::compile(&model, ports.len(), Some(&eb))?;
    check(start, budget)?;
    Ok(CurrentSpace {
        model,
        cells,
        directed_kernel_m5: final_m,
        max_quadrature_change: quad,
        max_reciprocity_error: reciprocity,
        planned_bytes: plan,
        elapsed_seconds: start.elapsed().as_secs_f64(),
        band_hz: [0., max_frequency_hz],
    })
}
impl CurrentSpace {
    pub fn recover_density(&self, currents: &[f64]) -> Result<Vec<[f64; 2]>> {
        if currents.len() != 2 * self.cells.len() {
            return Err(error(
                "CURRENT_RECOVERY",
                "full physical current coordinates required",
            ));
        }
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| c.recover_current_density([currents[2 * i], currents[2 * i + 1]]))
            .collect()
    }
    pub fn cell_losses_w(&self, currents: &[f64]) -> Result<Vec<f64>> {
        self.recover_density(currents)?;
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let p = c.resistance_ohm * (currents[2 * i].powi(2) + currents[2 * i + 1].powi(2));
                if p.is_finite() {
                    Ok(p)
                } else {
                    Err(error("CURRENT_RECOVERY", "nonfinite Joule power"))
                }
            })
            .collect()
    }
}
