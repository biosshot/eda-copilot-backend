//! Common-return canonical-potential composition. Differential infinity ports
//! cannot be joined without a physical terminal map and are deliberately refused.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topology {
    pub instances: Vec<crate::electrical::frequency::Instance>,
    pub connections: Vec<[usize; 2]>,
    pub ports: Vec<crate::electrical::frequency::OutputPort>,
}
pub struct Built {
    pub model: Model,
    pub fit: Option<Value>,
    pub conditional: bool,
    pub ports: Vec<String>,
    pub band: Option<[f64; 2]>,
    pub source_memory: usize,
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
    let raw = weight(&resource.data).saturating_mul(4);
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
            .is_some_and(|a| !a.iter().any(|v| v == "si"))
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
        let fit = relaxation::fit_started(resource, budget, start)?;
        (fit.model, Some(fit.report))
    };
    let band = authored_band(resource)?;
    super::super::deadline(start, budget, "Temporal source normalization")?;
    Ok(Built {
        model,
        conditional: conditional || fit.is_some() || band.is_some(),
        fit,
        ports: resource.ports.clone(),
        band,
        source_memory,
    })
}
pub fn build(topology: &Topology, budget: Option<&Budget>, start: Instant) -> Result<Built> {
    super::super::deadline(start, budget, "Temporal composition")?;
    let source_memory = topology
        .instances
        .iter()
        .fold(0usize, |n, i| n.saturating_add(source_plan(&i.model)));
    let matrix_budget = remaining_budget(budget, source_memory)?;
    let p = topology
        .instances
        .iter()
        .map(|i| i.ports.len())
        .sum::<usize>();
    if topology.instances.is_empty()
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
    let mut assigned = vec![false; p];
    for &[a, b] in &topology.connections {
        if a >= p || b >= p || a == b || assigned[a] || assigned[b] {
            return Err(fail(
                "COUPLED_TOPOLOGY",
                "Ports pair once; no self or junction connection",
            ));
        }
        assigned[a] = true;
        assigned[b] = true;
    }
    for port in &topology.ports {
        if port.index >= p || assigned[port.index] || port.name.trim().is_empty() {
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
    let (mut total_v, mut total_i, mut coefficients) = (0usize, 0usize, 0usize);
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
        let block = resource(&instance.model, budget, start)?;
        let m = &block.model;
        if m.reference_topology != "grounded" {
            return Err(fail("COUPLED_TERMINAL_MAP","Infinity/differential descriptor composition needs explicit physical terminal mapping; no return inferred"));
        }
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
    let mut parent: Vec<_> = (0..total_v).collect();
    let mut port_nodes = vec![];
    let mut offset = 0;
    for block in &built {
        let m = &block.model;
        for port in 0..block.ports.len() {
            let weights: Vec<_> = m
                .port_incidence
                .iter()
                .filter(|(_, j, v)| *j == port && *v != 0.)
                .collect();
            if weights.len() != 1 || weights[0].2 != 1. {
                return Err(fail(
                    "COUPLED_TERMINAL_MAP",
                    "Common-return composition requires canonical unit positive-potential ports",
                ));
            }
            port_nodes.push(offset + weights[0].0);
        }
        offset += m.potential_nodes.len();
    }
    for &[a, b] in &topology.connections {
        merge(&mut parent, port_nodes[a], port_nodes[b]);
    }
    let mut nodes = BTreeMap::new();
    let mut potential_nodes = vec![];
    let mut indices = vec![];
    for i in 0..total_v {
        let root = root(&parent, i);
        let index = *nodes.entry(root).or_insert_with(|| {
            potential_nodes.push(format!("assembled:{root}"));
            potential_nodes.len() - 1
        });
        indices.push(index);
    }
    let mut a = Matrix::new();
    let mut r = Matrix::new();
    let mut l = Matrix::new();
    let mut c = Matrix::new();
    let mut current_dofs = vec![];
    let (mut vo, mut io) = (0, 0);
    let mut candidates = vec![];
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
            *a.entry((indices[vo + i], io + j)).or_default() += v;
        }
        for (i, j, v) in m.capacitance_f {
            *c.entry((indices[vo + i], indices[vo + j])).or_default() += v;
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
    // Intersection of original nullspaces with the node-identification constraints.
    // This derives assembly invariants rather than imposing each block's charges.
    let k = candidates.len();
    let mut modes = vec![];
    if k > 0 {
        let differences: Vec<Vec<_>> = topology
            .connections
            .iter()
            .map(|[a, b]| {
                candidates
                    .iter()
                    .map(|v| v[port_nodes[*a]] - v[port_nodes[*b]])
                    .collect()
            })
            .collect();
        let gram = Mat::from_fn(k, k, |i, j| {
            differences.iter().map(|v| v[i] * v[j]).sum::<f64>()
        });
        let scale = (0..k).map(|i| gram[(i, i)]).fold(0., f64::max);
        let eig = Mat::from_fn(
            k,
            k,
            |i, j| if scale > 0. { gram[(i, j)] / scale } else { 0. },
        )
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| fail("COUPLED_GAUGE", format!("{e:?}")))?;
        for col in 0..k {
            if eig.S()[col].abs() <= 1e-12 {
                let full: Vec<_> = (0..total_v)
                    .map(|i| {
                        (0..k)
                            .map(|j| candidates[j][i] * eig.U()[(j, col)])
                            .sum::<f64>()
                    })
                    .collect();
                let mut reduced = vec![0.; potential_nodes.len()];
                let mut seen = vec![false; potential_nodes.len()];
                for (i, &v) in full.iter().enumerate() {
                    let j = indices[i];
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
    let entries = |m: Matrix| {
        m.into_iter()
            .filter_map(|((i, j), v)| (v != 0.).then_some((i, j, v)))
            .collect()
    };
    let model = Model {
        format: "coupled-rlc".into(),
        reference_topology: "grounded".into(),
        potential_nodes,
        current_dofs,
        incidence: entries(a),
        port_incidence: topology
            .ports
            .iter()
            .enumerate()
            .map(|(i, p)| (indices[port_nodes[p.index]], i, 1.))
            .collect(),
        resistance_ohm: entries(r),
        inductance_h: entries(l),
        capacitance_f: entries(c),
        conserved_potential_modes: modes,
    };
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
        model,
        fit,
        conditional,
        ports: topology.ports.iter().map(|p| p.name.clone()).collect(),
        band,
        source_memory,
    })
}
