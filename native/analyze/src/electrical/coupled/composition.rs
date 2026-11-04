//! Congruence assembly of full conductor models with explicitly extracted mutuals.
use super::*;
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Topology {
    pub instances: Vec<crate::electrical::frequency::Instance>,
    pub connections: Vec<[usize; 2]>,
    pub connection_contacts: Option<Vec<physical::Contacts>>,
    pub ports: Vec<crate::electrical::frequency::OutputPort>,
    #[serde(default)]
    pub instance_inductance_h: Vec<crate::electrical::frequency::InstanceInductance>,
}
pub struct Built {
    pub model: Model,
    pub fit: Option<Value>,
    pub conditional: bool,
    pub ports: Vec<String>,
    pub band: Option<[f64; 2]>,
    pub source_memory: usize,
    pub recovery: physical::Recovery,
    pub assembly_plan: usize,
}
/// Includes original JSON, normalized arrays and retained source/fitted report
/// matrices. This is reserved before fitting, independently of dense LU plans.
pub(crate) fn source_plan(resource: &crate::electrical::frequency::Resource) -> usize {
    fn weight(v: &Value) -> usize {
        match v {
            Value::Array(a) => a.iter().fold(64usize, |n, v| n.saturating_add(weight(v))),
            Value::Object(a) => a.iter().fold(128usize, |n, (k, v)| {
                n.saturating_add(64 + k.len()).saturating_add(weight(v))
            }),
            Value::String(s) => 32usize.saturating_add(s.len()),
            _ => 32,
        }
    }
    let metadata = resource.id.len()
        + resource.kind.len()
        + resource.ports.iter().map(String::len).sum::<usize>()
        + resource.source.as_ref().map(String::len).unwrap_or(0)
        + resource
            .analyses
            .as_ref()
            .map(|a| a.iter().map(String::len).sum::<usize>())
            .unwrap_or(0);
    let raw = weight(&resource.data)
        .saturating_add(resource.validity.as_ref().map(weight).unwrap_or(0))
        .saturating_add(metadata)
        .saturating_mul(4);
    if resource.data["format"] == "touchstone" {
        raw.saturating_add(
            4096usize
                .saturating_mul(resource.ports.len())
                .saturating_mul(resource.ports.len())
                .saturating_mul(1536),
        )
    } else {
        raw
    }
}
pub(crate) fn remaining_budget(budget: Option<&Budget>, reserved: usize) -> Result<Budget> {
    let memory = budget.and_then(|v| v.max_memory_mi_b).unwrap_or(256.);
    if !memory.is_finite() || memory <= 0. || memory > 4096. || reserved as f64 >= memory * 1048576.
    {
        return Err(fail(
            "COUPLED_BUDGET",
            "Retained source/fit reports exceed request memory before fitting/assembly",
        ));
    }
    Ok(Budget {
        max_seconds: budget.and_then(|v| v.max_seconds),
        max_memory_mi_b: Some(memory - reserved as f64 / 1048576.),
    })
}
pub(super) fn authored_band(
    resource: &crate::electrical::frequency::Resource,
) -> Result<Option<[f64; 2]>> {
    let Some(value) = resource
        .validity
        .as_ref()
        .and_then(|v| v["domain"].get("bandHz"))
    else {
        return Ok(None);
    };
    let band: Vec<f64> =
        serde_json::from_value(value.clone()).map_err(|e| fail("TEMPORAL_BAND", e))?;
    if band.len() != 2 || band[0] != 0. || !band[1].is_finite() || band[1] <= 0. {
        return Err(fail(
            "TEMPORAL_BAND",
            "Authored temporal applicability must include DC and a finite positive upper frequency",
        ));
    }
    Ok((band[1] < f64::MAX).then_some([0., band[1]]))
}
fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
fn merge(parent: &mut [usize], a: usize, b: usize) {
    let a = root(parent, a);
    let b = root(parent, b);
    parent[b] = a;
}
pub fn resource(
    resource: &crate::electrical::frequency::Resource,
    budget: Option<&Budget>,
    start: Instant,
) -> Result<Built> {
    resource_for(resource, budget, start, "si")
}
fn resource_for(
    resource: &crate::electrical::frequency::Resource,
    budget: Option<&Budget>,
    start: Instant,
    operation: &str,
) -> Result<Built> {
    super::super::deadline(start, budget, "Temporal source normalization")?;
    let source_memory = source_plan(resource);
    remaining_budget(budget, source_memory)?;
    if resource.kind != "network"
        || resource.ports.is_empty()
        || resource.ports.len() > 16
        || resource.ports.iter().any(|v| v.trim().is_empty())
        || resource.ports.iter().collect::<BTreeSet<_>>().len() != resource.ports.len()
        || resource
            .analyses
            .as_ref()
            .is_some_and(|a| !a.iter().any(|v| v == operation))
    {
        return Err(fail(
            "COUPLED_RESOURCE",
            "Invalid or unauthorized SI network Resource",
        ));
    }
    let conditional = match resource
        .validity
        .as_ref()
        .and_then(|v| v["status"].as_str())
    {
        None | Some("valid") => false,
        Some("conditional") => true,
        _ => return Err(fail("COUPLED_VALIDITY", "Unavailable/invalid SI source")),
    };
    let (model, fit) = if resource.data["format"] == "coupled-rlc" {
        (
            serde_json::from_value::<Model>(resource.data.clone())
                .map_err(|e| fail("COUPLED_INPUT", e))?,
            None,
        )
    } else {
        let fit = if resource.data["temporal"]["method"] == "passive-resonant-y" {
            resonant::fit_started(resource, budget, start)?
        } else {
            relaxation::fit_started(resource, budget, start)?
        };
        (fit.model, Some(fit.report))
    };
    let band = if operation == "si" {
        authored_band(resource)?
    } else {
        None
    };
    super::super::deadline(start, budget, "Temporal source normalization")?;
    let recovery = physical::Recovery::single(&model, &resource.id);
    Ok(Built {
        assembly_plan: 0,
        recovery,
        model,
        conditional: conditional || fit.is_some() || band.is_some(),
        fit,
        ports: resource.ports.clone(),
        band,
        source_memory,
    })
}
pub fn build(topology: &Topology, budget: Option<&Budget>, start: Instant) -> Result<Built> {
    build_for(topology, budget, start, "si")
}
pub(super) fn build_for(
    topology: &Topology,
    budget: Option<&Budget>,
    start: Instant,
    operation: &str,
) -> Result<Built> {
    super::super::deadline(start, budget, "Temporal composition")?;
    let source_memory = topology
        .instances
        .iter()
        .fold(0usize, |n, i| n.saturating_add(source_plan(&i.model)));
    let source_memory = source_memory.saturating_add(
        topology
            .instances
            .iter()
            .map(|i| i.id.len() + i.ports.iter().map(String::len).sum::<usize>())
            .sum::<usize>()
            .saturating_add(topology.ports.iter().map(|p| p.name.len()).sum::<usize>())
            .saturating_mul(16),
    );
    let source_memory = source_memory.saturating_add(
        topology.instance_inductance_h.len().saturating_mul(128),
    );
    let matrix_budget = remaining_budget(budget, source_memory)?;
    let p = topology
        .instances
        .iter()
        .map(|i| i.ports.len())
        .sum::<usize>();
    if topology
        .instances
        .iter()
        .any(|i| i.id.trim().is_empty() || i.id.len() > 1024)
        || topology.instances.is_empty()
        || topology.instances.len() > 32
        || p > 128
        || p == 0
        || topology.ports.is_empty()
        || topology.ports.len() > 16
        || topology
            .instances
            .iter()
            .map(|i| &i.id)
            .collect::<BTreeSet<_>>()
            .len()
            != topology.instances.len()
    {
        return Err(fail(
            "COUPLED_TOPOLOGY",
            "Invalid time composition size/identity",
        ));
    }
    if topology
        .connection_contacts
        .as_ref()
        .is_some_and(|v| v.len() != topology.connections.len())
    {
        return Err(fail(
            "COUPLED_TERMINAL_MAP",
            "connectionContacts must match pairs",
        ));
    }
    let mapped_junctions=topology.instances.iter().all(|i|i.model.data["format"]=="coupled-rlc"&&i.model.data["portTerminals"].is_array());
    if mapped_junctions&&topology.connections.len()>2*p{return Err(fail("COUPLED_TOPOLOGY","Physical junction connection budget exceeded"));}
    let mut assigned = vec![false; p];
    for &[a, b] in &topology.connections {
        if a >= p || b >= p || a == b || !mapped_junctions&&(assigned[a] || assigned[b]) {
            return Err(fail(
                "COUPLED_TOPOLOGY",
                "Ports pair once; no self or junction connection",
            ));
        }
        assigned[a] = true;
        assigned[b] = true;
    }
    let mut external_indices=BTreeSet::new();
    for port in &topology.ports {
        if port.index >= p || !external_indices.insert(port.index) || !mapped_junctions&&assigned[port.index] || port.name.trim().is_empty() {
            return Err(fail(
                "COUPLED_TOPOLOGY",
                "Invalid output alias or externally connected port",
            ));
        }
        assigned[port.index] = true;
    }
    if assigned.iter().any(|v| !*v)
        || topology
            .ports
            .iter()
            .map(|p| &p.name)
            .collect::<BTreeSet<_>>()
            .len()
            != topology.ports.len()
    {
        return Err(fail(
            "COUPLED_TOPOLOGY",
            "Every block port must be external or paired; duplicate output name",
        ));
    }
    let mut owner = vec![];
    for (k, instance) in topology.instances.iter().enumerate() {
        owner.extend(std::iter::repeat_n(k, instance.ports.len()));
    }
    let mut groups: Vec<_> = (0..topology.instances.len()).collect();
    for &[a, b] in &topology.connections {
        merge(&mut groups, owner[a], owner[b]);
    }
    if (0..groups.len()).any(|i| root(&groups, i) != root(&groups, 0)) {
        return Err(fail(
            "COUPLED_TOPOLOGY",
            "Disconnected/hidden blocks have no implicit common-return connection",
        ));
    }
    let mut built = vec![];
    let (mut total_v, mut total_i, mut coefficients) = (
        0usize,
        0usize,
        topology.instance_inductance_h.len().saturating_mul(2),
    );
    for instance in &topology.instances {
        if instance.ports.len() != instance.model.ports.len()
            || instance.ports.iter().any(|v| v.trim().is_empty())
            || instance.ports.iter().collect::<BTreeSet<_>>().len() != instance.ports.len()
        {
            return Err(fail(
                "COUPLED_PORT",
                "Invalid complete instance port mapping",
            ));
        }
        let block = resource_for(&instance.model, budget, start, operation)?;
        let m = &block.model;
        total_v = total_v.saturating_add(m.potential_nodes.len());
        total_i = total_i.saturating_add(m.current_dofs.len());
        coefficients = coefficients.saturating_add(
            m.incidence.len()
                + m.port_incidence.len()
                + m.resistance_ohm.len()
                + m.inductance_h.len()
                + m.capacitance_f.len(),
        );
        if total_v > 1024 || total_i > 1536 || total_v + total_i > 2048 || coefficients > 4_000_000
        {
            return Err(fail(
                "COUPLED_BUDGET",
                "Aggregate descriptor cap exceeded before assembly",
            ));
        }
        System::compile(m, instance.ports.len(), Some(&matrix_budget))?; // Reject duplicates/bad PSD before aggregation can hide them.
        super::super::deadline(start, budget, "Temporal block compile")?;
        built.push(block);
    }
    let preplan = (2 * total_v + total_i)
        .saturating_pow(2)
        .saturating_mul(1280)
        .saturating_add(coefficients.saturating_mul(128));
    if preplan as f64 > matrix_budget.max_memory_mi_b.unwrap() * 1048576. {
        return Err(fail(
            "COUPLED_BUDGET",
            "Pre-weld aggregate matrix/recovery plan exceeds memory",
        ));
    }
    let mapped = built.iter().any(|b| b.model.port_terminals.is_some());
    if (mapped || topology.connection_contacts.is_some())
        && built.iter().any(|b| b.model.port_terminals.is_none())
    {
        return Err(fail(
            "COUPLED_TERMINAL_MAP",
            "Physical connection requires explicit contacts/return on every coupled endpoint",
        ));
    }
    let grounded = built
        .iter()
        .any(|b| b.model.reference_topology == "grounded");
    let ground = total_v;
    let mut parent: Vec<_> = (0..=total_v).collect();
    let mut port_nodes = vec![];
    let mut offset = 0;
    for block in &built {
        let m = &block.model;
        if let Some(pairs) = physical::terminals(m, block.ports.len())? {
            port_nodes.extend(
                pairs
                    .into_iter()
                    .map(|p| p.map(|i| i.map(|i| offset + i).unwrap_or(ground))),
            );
        } else {
            if m.reference_topology != "grounded" {
                return Err(fail(
                    "COUPLED_TERMINAL_MAP",
                    "Infinity composition requires explicit physical contacts",
                ));
            }
            for port in 0..block.ports.len() {
                let weights: Vec<_> = m
                    .port_incidence
                    .iter()
                    .filter(|(_, j, v)| *j == port && *v != 0.)
                    .collect();
                if weights.len() != 1 || weights[0].2 != 1. {
                    return Err(fail(
                        "COUPLED_TERMINAL_MAP",
                        "Legacy composition requires canonical grounded ports",
                    ));
                }
                port_nodes.push([offset + weights[0].0, ground]);
            }
        }
        offset += m.potential_nodes.len();
    }
    for (i, &[a, b]) in topology.connections.iter().enumerate() {
        let contacts = topology
            .connection_contacts
            .as_ref()
            .map(|v| v[i])
            .unwrap_or_default();
        use physical::Contacts::{Both, Negative, NegativePositive, Positive, PositiveNegative};
        let pairs: &[(usize, usize)] = match contacts {
            Both => &[(0, 0), (1, 1)],
            Positive => &[(0, 0)],
            Negative => &[(1, 1)],
            PositiveNegative => &[(0, 1)],
            NegativePositive => &[(1, 0)],
        };
        for &(from, to) in pairs {
            merge(&mut parent, port_nodes[a][from], port_nodes[b][to]);
        }
    }
    let mut nodes = BTreeMap::new();
    let mut potential_nodes = vec![];
    let mut indices = vec![];
    for i in 0..total_v {
        let r = root(&parent, i);
        let index = if r == root(&parent, ground) {
            None
        } else {
            Some(*nodes.entry(r).or_insert_with(|| {
                potential_nodes.push(format!("assembled:{r}"));
                potential_nodes.len() - 1
            }))
        };
        indices.push(index);
    }
    let assembled_pair =
        |p: usize| port_nodes[p].map(|i| if i == ground { None } else { indices[i] });
    let mut port_incidence = vec![];
    let mut port_terminals = vec![];
    for (j, p) in topology.ports.iter().enumerate() {
        let pair = assembled_pair(p.index);
        if pair[0] == pair[1] {
            return Err(fail(
                "COUPLED_TERMINAL_MAP",
                format!("External port {} shorted by physical weld", p.name),
            ));
        }
        for (i, sign) in [(pair[0], 1.), (pair[1], -1.)] {
            if let Some(i) = i {
                port_incidence.push((i, j, sign));
            }
        }
        let terminal = |i: Option<usize>| match i {
            Some(i) => physical::Terminal::Node(physical::Node {
                node: potential_nodes[i].clone(),
            }),
            None => physical::Terminal::Ground(physical::Ground {
                reference: physical::GroundReference::Ground,
            }),
        };
        port_terminals.push(physical::PortTerminals {
            positive: terminal(pair[0]),
            negative: terminal(pair[1]),
        });
    }
    let mut recovery = physical::Recovery {
        mapped,
        ..Default::default()
    };
    let mut vbase = 0;
    let mut ibase = 0;
    for (instance, block) in topology.instances.iter().zip(&built) {
        for (i, node) in block.model.potential_nodes.iter().enumerate() {
            recovery.potentials.push(physical::Potential {
                instance_id: instance.id.clone(),
                node: node.clone(),
                index: indices[vbase + i],
            });
        }
        for (i, dof) in block.model.current_dofs.iter().enumerate() {
            recovery.currents.push(physical::Current {
                instance_id: instance.id.clone(),
                dof: dof.clone(),
                index: potential_nodes.len() + ibase + i,
            });
        }
        vbase += block.model.potential_nodes.len();
        ibase += block.model.current_dofs.len();
    }
    let mut current_offsets = BTreeMap::new();
    let mut offset = 0usize;
    for (index, block) in built.iter().enumerate() {
        current_offsets.insert(index, (offset, block.model.current_dofs.len()));
        offset += block.model.current_dofs.len();
    }
    let instance_indices: BTreeMap<_, _> = topology.instances.iter().enumerate().map(|(i, v)| (v.id.as_str(), i)).collect();
    let mut mutuals = vec![];
    let mut mutual_keys = BTreeSet::new();
    for coupling in &topology.instance_inductance_h {
        let Some(&a) = instance_indices.get(coupling.a.as_str()) else { return Err(fail("COUPLED_MUTUAL_INPUT", "Unknown mutual instance a")); };
        let Some(&b) = instance_indices.get(coupling.b.as_str()) else { return Err(fail("COUPLED_MUTUAL_INPUT", "Unknown mutual instance b")); };
        if a == b || !coupling.value_h.is_finite() {
            return Err(fail("COUPLED_MUTUAL_INPUT", "Only finite mutuals between distinct physical instances are admitted"));
        }
        let (ao, an) = current_offsets[&a];
        let (bo, bn) = current_offsets[&b];
        if coupling.local_a >= an || coupling.local_b >= bn {
            return Err(fail("COUPLED_MUTUAL_INPUT", "Mutual local current index outside source block"));
        }
        let canonical = if (a, coupling.local_a) <= (b, coupling.local_b) {
            (a, coupling.local_a, b, coupling.local_b)
        } else {
            (b, coupling.local_b, a, coupling.local_a)
        };
        if !mutual_keys.insert(canonical) {
            return Err(fail("COUPLED_MUTUAL_INPUT", "Duplicate directed/reciprocal mutual entry"));
        }
        mutuals.push((ao + coupling.local_a, bo + coupling.local_b, coupling.value_h));
    }
    let mut a = Matrix::new();
    let mut r = Matrix::new();
    let mut l = Matrix::new();
    let mut c = Matrix::new();
    let mut current_dofs = vec![];
    let (mut vo, mut io) = (0, 0);
    let mut candidates = vec![];
    let mut structural_candidates = vec![];
    let mut fits = vec![];
    let mut conditional = false;
    let mut common_max = f64::INFINITY;
    let mut band: Option<[f64; 2]> = None;
    for (instance, block) in topology.instances.iter().zip(built) {
        let m = block.model;
        conditional |= block.conditional;
        if let Some(value) = block.band {
            band = Some([
                0.,
                band.map(|v| v[1]).unwrap_or(f64::INFINITY).min(value[1]),
            ]);
        }
        if let Some(fit) = block.fit {
            common_max = common_max.min(
                fit["frequenciesHz"]
                    .as_array()
                    .and_then(|v| v.last())
                    .and_then(Value::as_f64)
                    .ok_or_else(|| fail("TEMPORAL_BAND", "Missing fit band"))?,
            );
            fits.push(fit);
        }
        for mode in independent(m.structural_gauge_modes.clone(), m.potential_nodes.len())? {
            let mut v = vec![0.; total_v];
            v[vo..vo + mode.len()].copy_from_slice(&mode);
            structural_candidates.push(v);
        }
        // Supplied columns are a full candidate nullspace contract. Pure C defaults I.
        let modes = if m.current_dofs.is_empty() && m.conserved_potential_modes.is_empty() {
            (0..m.potential_nodes.len())
                .map(|i| {
                    (0..m.potential_nodes.len())
                        .map(|j| if i == j { 1. } else { 0. })
                        .collect()
                })
                .collect()
        } else {
            m.conserved_potential_modes
        };
        for mode in modes {
            let mut v = vec![0.; total_v];
            v[vo..vo + mode.len()].copy_from_slice(&mode);
            candidates.push(v);
        }
        for (i, j, v) in m.incidence {
            if let Some(i) = indices[vo + i] {
                *a.entry((i, io + j)).or_default() += v;
            }
        }
        for (i, j, v) in m.capacitance_f {
            if let (Some(i), Some(j)) = (indices[vo + i], indices[vo + j]) {
                *c.entry((i, j)).or_default() += v;
            }
        }
        for (i, j, v) in m.resistance_ohm {
            r.insert((io + i, io + j), v);
        }
        for (i, j, v) in m.inductance_h {
            l.insert((io + i, io + j), v);
        }
        current_dofs.extend(
            m.current_dofs
                .iter()
                .map(|v| format!("{}:{v}", instance.id)),
        );
        vo += m.potential_nodes.len();
        io += m.current_dofs.len();
    }
    for (a, b, value) in mutuals {
        if l.insert((a, b), value).is_some() || l.insert((b, a), value).is_some() {
            return Err(fail("COUPLED_MUTUAL_INPUT", "Mutual overlaps an existing/self coefficient; no double counting"));
        }
    }
    // Intersection of original nullspaces with the node-identification constraints.
    // This derives assembly invariants rather than imposing each block's charges.
    let intersect = |candidates: Vec<Vec<f64>>| -> Result<Vec<Vec<f64>>> {
        let candidates = independent(candidates, total_v)?;
        let k = candidates.len();
        let mut modes = vec![];
        if k > 0 {
            let differences: Vec<Vec<f64>> = (0..total_v)
                .filter_map(|i| {
                    let r = root(&parent, i);
                    if r == i {
                        return None;
                    }
                    Some(
                        candidates
                            .iter()
                            .map(|v| v[i] - if r == ground { 0. } else { v[r] })
                            .collect(),
                    )
                })
                .chain(
                    (0..total_v)
                        .filter(|&i| root(&parent, i) == root(&parent, ground))
                        .map(|i| candidates.iter().map(|v| v[i]).collect()),
                )
                .collect();
            // Work on the constraint operator itself. Eigenvalues of D^T D
            // square small singular values and can admit non-null combinations
            // at a 1e-6 operator residual under a 1e-12 eigenvalue cutoff.
            // Exact support components prevent machine-epsilon SVD leakage
            // into an unrelated conductor constraint. No nonzero coefficient
            // is filtered and every original row is still certified below.
            let mut support: Vec<_> = (0..k).collect();
            for row in &differences {
                if let Some(first) = row.iter().position(|v| *v != 0.) {
                    for (j, v) in row.iter().enumerate() {
                        if *v != 0. { merge(&mut support, first, j); }
                    }
                }
            }
            let mut components = BTreeMap::<usize, Vec<usize>>::new();
            for j in 0..k { components.entry(root(&support, j)).or_default().push(j); }
            // Nonsingular row equilibration leaves the exact kernel intact.
            let row_scales: Vec<_> = differences.iter().map(|row| {
                row.iter().map(|v| v.abs()).fold(0., f64::max)
            }).collect();
            for columns in components.values() {
                let width = columns.len();
                // Pad short operators so thin SVD retains all right-null vectors.
                let operator = Mat::from_fn(differences.len().max(width), width, |i, j| {
                    if i < differences.len() && row_scales[i] > 0. { differences[i][columns[j]] / row_scales[i] } else { 0. }
                });
                let svd = operator.thin_svd()
                    .map_err(|e| fail("COUPLED_GAUGE", format!("{e:?}")))?;
                let maximum = (0..width).map(|i| svd.S()[i]).fold(0., f64::max);
                for col in 0..width {
                    if svd.S()[col] > 1e-12 * maximum { continue; }
                    let mut coefficients = vec![0.; k];
                    for (j, &original) in columns.iter().enumerate() { coefficients[original] = svd.V()[(j, col)]; }
                    // Certify original, unsquared identifications and grounding.
                    for row in &differences {
                        let residual = (0..k).map(|j| row[j] * coefficients[j]).sum::<f64>();
                        let bound = (0..k).map(|j| (row[j] * coefficients[j]).abs()).sum::<f64>();
                        if residual.abs() > 1e-12 * bound.max(1e-300) {
                            return Err(fail("COUPLED_GAUGE", "Unresolved assembly constraint nullspace"));
                        }
                    }
                    let full: Vec<_> = (0..total_v)
                        .map(|i| {
                            (0..k)
                                .map(|j| candidates[j][i] * coefficients[j])
                                .sum::<f64>()
                        })
                        .collect();
                    let mut reduced = vec![0.; potential_nodes.len()];
                    let mut seen = vec![false; potential_nodes.len()];
                    for (i, &v) in full.iter().enumerate() {
                        let Some(j) = indices[i] else { continue };
                        if seen[j] && (reduced[j] - v).abs() > 1e-10 {
                            return Err(fail("COUPLED_GAUGE", "Unresolved assembly nullspace"));
                        }
                        reduced[j] = v;
                        seen[j] = true;
                    }
                    modes.push(reduced);
                }
            }
        }
        Ok(modes)
    };
    let modes = intersect(candidates)?;
    let structural_modes = intersect(structural_candidates)?;
    let entries = |m: Matrix| {
        m.into_iter()
            .filter_map(|((i, j), v)| (v != 0.).then_some((i, j, v)))
            .collect()
    };
    let model = Model {
        format: "coupled-rlc".into(),
        reference_topology: if grounded { "grounded" } else { "infinity" }.into(),
        potential_nodes,
        current_dofs,
        incidence: entries(a),
        port_incidence,
        port_terminals: mapped.then_some(port_terminals),
        resistance_ohm: entries(r),
        inductance_h: entries(l),
        capacitance_f: entries(c),
        structural_gauge_modes: structural_modes,
        conserved_potential_modes: modes,
    };
    // Cross-block mutuals participate in the same exact symmetry/PSD and
    // descriptor checks as every source block before any frequency/time solve.
    System::compile(&model, topology.ports.len(), Some(&matrix_budget))?;
    let fit = if fits.is_empty() {
        None
    } else {
        let coefficients: Vec<_> = fits
            .iter()
            .flat_map(|v| v["coefficients"].as_array().into_iter().flatten().cloned())
            .collect();
        Some(
            json!({"status":"complete","validity":{"status":"conditional","reason":"Composition retains finite-sample/high-frequency closure assumptions of every source fit"},"frequenciesHz":[0.,common_max],"coefficients":coefficients,"sourceFits":fits,"sourceLimitations":"Source intersample/extraband behavior remains conditional; composition cannot prove unseen resonances"}),
        )
    };
    super::super::deadline(start, budget, "Temporal composition assembly")?;
    Ok(Built {
        assembly_plan: preplan,
        recovery,
        model,
        fit,
        conditional,
        ports: topology.ports.iter().map(|p| p.name.clone()).collect(),
        band,
        source_memory,
    })
}

#[cfg(test)]
mod mutual_tests {
    use super::*;
    fn topology(instance_inductance_h: Value) -> Topology {
        let model = |id: &str| json!({
            "id":id,"kind":"network","ports":["in","out"],
            "data":{
                "format":"coupled-rlc","referenceTopology":"grounded",
                "potentialNodes":[format!("{id}:a"),format!("{id}:b")],"currentDofs":["i"],
                "incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,1,1.]],
                "resistanceOhm":[[0,0,1.]],"inductanceH":[[0,0,if id=="a" {4e-6} else {9e-6}]],
                "capacitanceF":[[0,0,1e-12],[1,1,1e-12]],
                "conservedPotentialModes":[],"structuralGaugeModes":[]
            }
        });
        serde_json::from_value(json!({
            "instances":[{"id":"a","model":model("a"),"ports":["in","out"]},{"id":"b","model":model("b"),"ports":["in","out"]}],
            "connections":[[1,2]],"connectionContacts":null,
            "ports":[{"name":"in","index":0},{"name":"out","index":3}],
            "instanceInductanceH":instance_inductance_h
        })).unwrap()
    }
    fn mutual(a: &str, b: &str, local_a: usize, local_b: usize, value_h: f64) -> Value {
        json!({"a":a,"b":b,"localA":local_a,"localB":local_b,"valueH":value_h})
    }
    #[test]
    fn cross_block_mutual_is_symmetric_once_and_preserves_positive_energy() {
        let plain = build(&topology(json!([])), None, Instant::now()).unwrap();
        assert_eq!(plain.model.inductance_h, vec![(0,0,4e-6),(1,1,9e-6)]);
        for value in [1e-6, -1e-6] {
            let built = build(&topology(json!([mutual("a","b",0,0,value)])), None, Instant::now()).unwrap();
            assert_eq!(built.model.inductance_h, vec![(0,0,4e-6),(0,1,value),(1,0,value),(1,1,9e-6)]);
            for currents in [[1.,1.],[1.,-2.],[-2.,1.]] {
                let energy = 0.5*(4e-6*currents[0]*currents[0]+9e-6*currents[1]*currents[1]+2.*value*currents[0]*currents[1]);
                assert!(energy > 0. && energy.is_finite());
            }
        }
    }
    #[test]
    fn invalid_duplicate_reciprocal_and_indefinite_mutuals_are_refused() {
        let cases = [
            vec![mutual("a","b",0,0,1e-6),mutual("a","b",0,0,1e-6)],
            vec![mutual("a","b",0,0,1e-6),mutual("b","a",0,0,1e-6)],
            vec![mutual("a","a",0,0,1e-6)],
            vec![mutual("a","missing",0,0,1e-6)],
            vec![mutual("a","b",1,0,1e-6)],
            vec![mutual("a","b",0,0,7e-6)],
        ];
        for mutuals in cases {
            let error = build(&topology(json!(mutuals)), None, Instant::now()).err().unwrap();
            assert!(error.starts_with("COUPLED_MUTUAL_INPUT") || error.starts_with("COUPLED_PSD"), "{error}");
        }
    }
}
