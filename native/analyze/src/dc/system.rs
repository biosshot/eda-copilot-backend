use super::*;
use crate::conduction::Domain;
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    sparse::{
        linalg::qr::{self, QrRef, SymbolicQr},
        SparseColMat, Triplet,
    },
    Conj, Mat, Par,
};
use std::collections::{BTreeSet, VecDeque};

pub(super) struct System {
    key: String,
    matrix: SparseColMat<usize, f64>,
    factor: Factor,
    dofs: Vec<Vec<usize>>,
    terminal_nodes: BTreeMap<String, usize>,
    gauges: BTreeSet<usize>,
    reference: BTreeMap<usize, usize>,
    components: Vec<usize>,
    voltage_elements: Vec<usize>,
    included: Vec<usize>,
    barrels: Vec<(String, String, String, usize, usize, f64)>,
    /// Original FEM terms retained for physical KCL/reactions and energy.
    terms: Vec<(usize, usize, f64)>,
    node_count: usize,
    memory_plan: usize,
}
// faer owns symbolic/numerical QR. Explicit factor/workspace lengths permit
// admission from actual sparse storage instead of a dense N-squared guess.
struct Factor {
    symbolic: SymbolicQr<usize>,
    indices: Vec<usize>,
    values: Vec<f64>,
}
impl Factor {
    fn solve(&self, rhs: &Mat<f64>) -> Result<Mat<f64>> {
        let mut x = rhs.clone();
        let mut scratch =
            MemBuffer::try_new(self.symbolic.solve_in_place_scratch::<f64>(1, Par::Seq))
                .map_err(|e| error("DC_MEMORY_LIMIT", format!("Solve workspace: {e:?}")))?;
        // SAFETY: these arrays were initialized by factorize_numeric_qr below
        // and retained with the exact symbolic structure that produced them.
        unsafe { QrRef::new_unchecked(&self.symbolic, &self.indices, &self.values) }
            .solve_in_place_with_conj(Conj::No, x.as_mut(), Par::Seq, MemStack::new(&mut scratch));
        Ok(x)
    }
}
fn area_gradient(d: &Domain, t: [usize; 3]) -> Result<(f64, [[f64; 2]; 3])> {
    let [a, b, c] = t.map(|i| d.nodes[i]);
    let det = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if !det.is_finite() || det.abs() < 1e-20 {
        return Err(error("DC_DEGENERATE_TRIANGLE", &d.id));
    }
    Ok((
        det.abs() / 2.,
        [
            [(b[1] - c[1]) / det, (c[0] - b[0]) / det],
            [(c[1] - a[1]) / det, (a[0] - c[0]) / det],
            [(a[1] - b[1]) / det, (b[0] - a[0]) / det],
        ],
    ))
}
fn join_all(u: &mut Union, nodes: &[usize]) -> Result<usize> {
    let first = *nodes
        .first()
        .ok_or_else(|| error("DC_EMPTY_CONTACT", "Contact has no mesh nodes"))?;
    for &n in nodes {
        u.join(first, n);
    }
    Ok(first)
}
fn snapshot_allowed(d: &Domain, m: &ConductionReport, r: &Request) -> bool {
    r.snapshot
        && d.readiness
            .reasons
            .iter()
            .all(|s| s == "fill_freshness_unconfirmed")
        && m.fills.iter().filter(|f| f.layer == d.layer).all(|f| {
            f.status == "available" && matches!(f.freshness.as_deref(), Some("current" | "unknown"))
        })
}
fn assemble(m: &ConductionReport, r: &Request, key: String) -> Result<System> {
    if !unique_ids(r.terminals.iter().map(|t| t.id.as_str()))
        || !unique_ids(r.elements.iter().map(|e| e.id.as_str()))
    {
        return Err(error("DC_DUPLICATE_ID", "Duplicate terminal or element ID"));
    }
    // Pick the external surface in declared physical stack order. Never collapse
    // terminals across layers: that would turn a plated hole into an ideal short.
    let mut selected = BTreeSet::new();
    let mut contacts = BTreeMap::new();
    for t in &r.terminals {
        let path = m
            .sources
            .iter()
            .find(|s| s.kind == "pad" && s.source_id.as_deref() == Some(&t.pad_id))
            .map(|s| &s.path);
        let surface = m
            .profile
            .layers
            .iter()
            .find(|l| {
                m.domains
                    .iter()
                    .any(|d| d.layer == l.layer && path.is_some_and(|p| d.source_paths.contains(p)))
            })
            .map(|l| &l.layer);
        let mut matches: Vec<_> = m
            .domains
            .iter()
            .enumerate()
            .flat_map(|(di, d)| {
                d.terminals
                    .iter()
                    .filter(|p| p.source.source_id.as_deref() == Some(&t.pad_id))
                    .map(move |p| (di, p))
            })
            .collect();
        if let Some(layer) = surface {
            matches.retain(|(di, _)| &m.domains[*di].layer == layer);
        }
        matches.sort_by_key(|(i, _)| {
            m.profile
                .layers
                .iter()
                .position(|l| l.layer == m.domains[*i].layer)
                .unwrap_or(usize::MAX)
        });
        let (di, p) = *matches.first().ok_or_else(|| {
            error(
                "DC_TERMINAL_MISSING",
                format!("Pad {} has no finite-area mesh terminal", t.pad_id),
            )
        })?;
        let layer = &m.domains[di].layer;
        if matches
            .iter()
            .filter(|(i, _)| &m.domains[*i].layer == layer)
            .count()
            != 1
        {
            return Err(error(
                "DC_AMBIGUOUS_CONTACT",
                format!("Pad {} split into multiple surface domains", t.pad_id),
            ));
        }
        contacts.insert(t.id.clone(), (di, p.triangles.clone()));
        selected.insert(di);
    }
    if selected.is_empty() {
        return Err(error("DC_TERMINAL_MISSING", "No selected board contacts"));
    }
    // Closure uses physical barrel contacts, including expected/unbuilt contacts.
    // An unready barrel cannot disappear from an otherwise selected domain.
    loop {
        let before = selected.len();
        for b in &m.barrels {
            let domains: BTreeSet<_> = b
                .contacts
                .iter()
                .map(|c| &c.domain)
                .chain(b.expected_contacts.iter().flat_map(|c| c.domain_ids.iter()))
                .collect();
            if selected.iter().any(|i| domains.contains(&m.domains[*i].id)) {
                if !b.ready {
                    return Err(error(
                        "DC_BARREL_NOT_READY",
                        format!("{}: {}", b.hole_id, b.reasons.join(", ")),
                    ));
                }
                for (i, d) in m.domains.iter().enumerate() {
                    if domains.contains(&d.id) {
                        selected.insert(i);
                    }
                }
            }
        }
        if before == selected.len() {
            break;
        }
    }
    for &i in &selected {
        let d = &m.domains[i];
        if !d.readiness.dc_ready && !snapshot_allowed(d, m, r) {
            return Err(error(
                "DC_DOMAIN_NOT_READY",
                format!(
                    "{} on {}: {}",
                    d.id,
                    d.layer,
                    d.readiness.reasons.join(", ")
                ),
            ));
        }
    }
    // Global exclusions/unknown drills already block copper readiness. Reject
    // known shorts on participating source regions, without relabelling nets.
    if m.findings.iter().any(|f| {
        f.code.contains("short")
            && f.region_ids.iter().any(|id| {
                selected
                    .iter()
                    .any(|i| m.domains[*i].region_ids.contains(id))
            })
    }) {
        return Err(error(
            "DC_COPPER_SHORT",
            "Participating geometry contains a confirmed net short",
        ));
    }
    let mut offsets = vec![0; m.domains.len()];
    let mut n = 0usize;
    for &i in &selected {
        offsets[i] = n;
        n += m.domains[i].nodes.len();
    }
    let triangles: usize = selected.iter().map(|i| m.domains[*i].triangles.len()).sum();
    let linear_plan = n.saturating_mul(480) + triangles.saturating_mul(480);
    if linear_plan as f64 > r.limits.max_memory_mib * 1048576. {
        return Err(error(
            "DC_MEMORY_LIMIT",
            "Selected mesh/assembly plan exceeds memory budget",
        ));
    }
    let mut u = Union::new(n);
    let mut contact_nodes = BTreeMap::new();
    for (id, (di, triangles)) in &contacts {
        let d = &m.domains[*di];
        let nodes: Vec<_> = triangles
            .iter()
            .flat_map(|i| d.triangles[*i])
            .map(|n| n + offsets[*di])
            .collect();
        let node = join_all(&mut u, &nodes)?;
        contact_nodes.insert(id.clone(), node);
    }
    let mut barrel_nodes = BTreeMap::new();
    for b in &m.barrels {
        for c in &b.contacts {
            let Some((di, d)) = m
                .domains
                .iter()
                .enumerate()
                .find(|(i, d)| selected.contains(i) && d.id == c.domain)
            else {
                continue;
            };
            let nodes: Vec<_> = c
                .boundary_edges
                .iter()
                .flat_map(|e| d.boundaries[*e].nodes)
                .map(|n| n + offsets[di])
                .collect();
            let node = join_all(&mut u, &nodes)?;
            // Multiple fragments of the same circumference have one ring contact.
            let k = (b.hole_id.clone(), c.layer.clone());
            if let Some(&old) = barrel_nodes.get(&k) {
                u.join(old, node);
            } else {
                barrel_nodes.insert(k, node);
            }
        }
    }
    let mut roots = BTreeMap::new();
    for i in 0..n {
        let root = u.root(i);
        let next = roots.len();
        roots.entry(root).or_insert(next);
    }
    let node_count = roots.len();
    if node_count > r.limits.max_unknowns {
        return Err(error(
            "DC_SYSTEM_LIMIT",
            format!("{node_count} electrode-reduced mesh unknowns exceed cap"),
        ));
    }
    let idx: Vec<_> = (0..n).map(|i| roots[&u.root(i)]).collect();
    let mut dofs = vec![vec![]; m.domains.len()];
    for &di in &selected {
        dofs[di] = (0..m.domains[di].nodes.len())
            .map(|i| idx[offsets[di] + i])
            .collect();
    }
    let terminal_nodes: BTreeMap<_, _> = contact_nodes
        .into_iter()
        .map(|(id, n)| (id, idx[n]))
        .collect();
    if r.elements.iter().any(|e| {
        !terminal_nodes.contains_key(&e.positive) || !terminal_nodes.contains_key(&e.negative)
    }) {
        return Err(error(
            "DC_ELEMENT_CONTACT_MISSING",
            "Element references an unknown terminal",
        ));
    }
    let mut connectivity = Union::new(node_count);
    let mut terms = Vec::with_capacity(triangles * 9);
    for &di in &selected {
        let d = &m.domains[di];
        let sheet = d
            .sheet_conductance_s
            .ok_or_else(|| error("DC_MISSING_PHYSICS", &d.id))?;
        for t in &d.triangles {
            let (area, g) = area_gradient(d, *t)?;
            let nodes = t.map(|i| dofs[di][i]);
            connectivity.join(nodes[0], nodes[1]);
            connectivity.join(nodes[1], nodes[2]);
            // Pairwise Laplacian assembly preserves the constant-potential
            // nullspace even when floating-point gradient sums cancel poorly.
            for i in 0..3 {
                for j in i + 1..3 {
                    if nodes[i] != nodes[j] {
                        let weight = -sheet * area * (g[i][0] * g[j][0] + g[i][1] * g[j][1]);
                        add_resistor(&mut terms, nodes[i], nodes[j], weight);
                    }
                }
            }
        }
    }
    let mut barrels = vec![];
    for b in &m.barrels {
        for s in &b.segments {
            let a = barrel_nodes.get(&(b.hole_id.clone(), s.from_layer.clone()));
            let z = barrel_nodes.get(&(b.hole_id.clone(), s.to_layer.clone()));
            if let (Some(&a), Some(&z)) = (a, z) {
                let a = idx[a];
                let z = idx[z];
                let g = s
                    .conductance_s
                    .ok_or_else(|| error("DC_MISSING_BARREL_PHYSICS", &b.hole_id))?;
                add_resistor(&mut terms, a, z, g);
                connectivity.join(a, z);
                barrels.push((
                    b.hole_id.clone(),
                    s.from_layer.clone(),
                    s.to_layer.clone(),
                    a,
                    z,
                    g,
                ));
            }
        }
    }
    let voltage_elements: Vec<_> = r
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, ElementKind::Voltage))
        .map(|(i, _)| i)
        .collect();
    validate_voltage_constraints(r, &terminal_nodes)?;
    for e in &r.elements {
        let a = terminal_nodes[&e.positive];
        let b = terminal_nodes[&e.negative];
        match e.kind {
            ElementKind::Resistance => {
                add_resistor(&mut terms, a, b, 1. / e.value);
                connectivity.join(a, b);
            }
            ElementKind::Voltage => connectivity.join(a, b),
            ElementKind::Current => {}
        }
    }
    let components: Vec<_> = (0..node_count).map(|i| connectivity.root(i)).collect();
    let mut reference = BTreeMap::new();
    for e in r
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Voltage))
    {
        let n = terminal_nodes[&e.negative];
        reference.entry(components[n]).or_insert(n);
    }
    for (i, &c) in components.iter().enumerate() {
        reference.entry(c).or_insert(i);
    }
    let gauges: BTreeSet<_> = reference.values().copied().collect();
    let size = node_count + voltage_elements.len();
    let retained_plan = m.counts.nodes.saturating_mul(320)
        + m.counts.triangles.saturating_mul(192)
        + r.board_json
            .as_ref()
            .map_or(0, |s| s.len().saturating_mul(16));
    let mut triplets = vec![];
    for &(i, j, v) in &terms {
        if !gauges.contains(&i) && !gauges.contains(&j) {
            triplets.push(Triplet::new(i, j, v));
        }
    }
    for &i in &gauges {
        triplets.push(Triplet::new(i, i, 1.));
    }
    for (k, &ei) in voltage_elements.iter().enumerate() {
        let e = &r.elements[ei];
        for (n, sign) in [
            (terminal_nodes[&e.positive], 1.),
            (terminal_nodes[&e.negative], -1.),
        ] {
            if !gauges.contains(&n) {
                triplets.push(Triplet::new(n, node_count + k, sign));
                triplets.push(Triplet::new(node_count + k, n, sign));
            }
        }
    }
    let matrix = SparseColMat::try_new_from_triplets(size, size, &triplets)
        .map_err(|e| error("DC_ASSEMBLY_FAILED", format!("{e:?}")))?;
    // Bound symbolic input/work arrays before asking faer for factor sizes.
    let symbolic_plan = retained_plan
        .saturating_add(linear_plan)
        .saturating_add(matrix.val().len().saturating_mul(128))
        .saturating_add(size.saturating_mul(512));
    if symbolic_plan as f64 > r.limits.max_memory_mib * 1048576. {
        return Err(error(
            "DC_MEMORY_LIMIT",
            format!("Sparse symbolic/mesh plan {symbolic_plan} bytes exceeds budget"),
        ));
    }
    let symbolic = qr::factorize_symbolic_qr(matrix.symbolic(), Default::default())
        .map_err(|e| error("DC_ASSEMBLY_FAILED", format!("Sparse symbolic QR: {e:?}")))?;
    let scratch_req = symbolic.factorize_numeric_qr_scratch::<f64>(Par::Seq, Default::default());
    let memory_plan = symbolic_plan
        .saturating_add(
            symbolic
                .len_idx()
                .saturating_mul(std::mem::size_of::<usize>()),
        )
        .saturating_add(symbolic.len_val().saturating_mul(8))
        .saturating_add(
            scratch_req.unaligned_bytes_required().max(
                symbolic
                    .solve_in_place_scratch::<f64>(1, Par::Seq)
                    .unaligned_bytes_required(),
            ),
        );
    if memory_plan as f64 > r.limits.max_memory_mib * 1048576. {
        return Err(error(
            "DC_MEMORY_LIMIT",
            format!("Sparse factors/workspace/mesh plan {memory_plan} bytes exceeds budget"),
        ));
    }
    let mut factor = Factor {
        indices: vec![0; symbolic.len_idx()],
        values: vec![0.; symbolic.len_val()],
        symbolic,
    };
    let mut scratch = MemBuffer::try_new(scratch_req)
        .map_err(|e| error("DC_MEMORY_LIMIT", format!("Factor workspace: {e:?}")))?;
    factor.symbolic.factorize_numeric_qr(
        &mut factor.indices,
        &mut factor.values,
        matrix.as_ref(),
        Par::Seq,
        MemStack::new(&mut scratch),
        Default::default(),
    );
    drop(scratch);
    Ok(System {
        key,
        matrix,
        factor,
        dofs,
        terminal_nodes,
        gauges,
        reference,
        components,
        voltage_elements,
        included: selected.into_iter().collect(),
        barrels,
        terms,
        node_count,
        memory_plan,
    })
}
fn add_resistor(terms: &mut Vec<(usize, usize, f64)>, a: usize, b: usize, g: f64) {
    for (i, j, v) in [(a, a, g), (b, b, g), (a, b, -g), (b, a, -g)] {
        terms.push((i, j, v));
    }
}
fn validate_voltage_constraints(r: &Request, nodes: &BTreeMap<String, usize>) -> Result<()> {
    let mut graph: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
    for e in r
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Voltage))
    {
        let a = nodes[&e.positive];
        let b = nodes[&e.negative];
        let mut q = VecDeque::from([(a, 0.)]);
        let mut seen = BTreeSet::new();
        let mut found = None;
        while let Some((n, v)) = q.pop_front() {
            if !seen.insert(n) {
                continue;
            }
            if n == b {
                found = Some(v);
                break;
            }
            for &(j, d) in graph.get(&n).into_iter().flatten() {
                q.push_back((j, v + d));
            }
        }
        if let Some(v) = found {
            return Err(error(
                if (v + e.value).abs() <= 1e-10 {
                    "DC_INDETERMINATE_SOURCE_CURRENT"
                } else {
                    "DC_CONFLICTING_VOLTAGE_SOURCES"
                },
                format!("Dependent ideal voltage constraint {}", e.id),
            ));
        }
        graph.entry(a).or_default().push((b, -e.value));
        graph.entry(b).or_default().push((a, e.value));
    }
    Ok(())
}

// Evaluate the physical operator by voltage differences. Multiplying a rounded
// assembled diagonal by a large common-mode voltage loses the Laplacian zero
// row sum and can hide KCL error behind a tiny normwise residual.
// Retain sub-ULP refinement corrections: adding them directly to a rail's
// absolute voltage can erase the small differences needed for physical KCL.
fn difference(x: &Mat<f64>, low: &Mat<f64>, a: usize, b: usize) -> f64 {
    (x[(a, 0)] - x[(b, 0)]) + (low[(a, 0)] - low[(b, 0)])
}
fn add_correction(high: &mut f64, low: &mut f64, correction: f64) {
    let sum = *high + correction;
    let part = sum - *high;
    let error = (*high - (sum - part)) + (correction - part);
    let remainder = *low + error;
    let next = sum + remainder;
    *low = remainder - (next - sum);
    *high = next;
}
fn apply_physical(s: &System, r: &Request, x: &Mat<f64>, low: &Mat<f64>, out: &mut [f64]) {
    out.fill(0.);
    for &(i, j, g) in &s.terms {
        if i != j {
            out[i] += g * difference(x, low, j, i);
        }
    }
    for (k, &ei) in s.voltage_elements.iter().enumerate() {
        let e = &r.elements[ei];
        let a = s.terminal_nodes[&e.positive];
        let b = s.terminal_nodes[&e.negative];
        let row = s.node_count + k;
        out[a] += x[(row, 0)] + low[(row, 0)];
        out[b] -= x[(row, 0)] + low[(row, 0)];
        out[row] = difference(x, low, a, b);
    }
    for &i in &s.gauges {
        out[i] = x[(i, 0)] + low[(i, 0)];
    }
}
pub(super) fn solve(
    m: &ConductionReport,
    r: &Request,
    old: Option<System>,
    start: Instant,
) -> Result<(Value, System)> {
    let mut signature = r.clone();
    for e in &mut signature.elements {
        if !matches!(e.kind, ElementKind::Resistance) {
            e.value = 0.;
        }
    }
    let key = serde_json::to_string(&signature).unwrap();
    let reused = old.as_ref().is_some_and(|s| s.key == key);
    let assembly_start = Instant::now();
    let s = if reused {
        old.unwrap()
    } else {
        assemble(m, r, key)?
    };
    let assembly_ms = assembly_start.elapsed().as_secs_f64() * 1000.;
    if start.elapsed().as_secs_f64() > r.limits.max_seconds {
        return Err(error("DC_DEADLINE", "Assembly exceeded budget"));
    }
    let mut rhs = Mat::zeros(s.matrix.nrows(), 1);
    let mut injection = vec![0.; s.node_count];
    for e in &r.elements {
        if matches!(e.kind, ElementKind::Current) {
            injection[s.terminal_nodes[&e.positive]] += e.value;
            injection[s.terminal_nodes[&e.negative]] -= e.value;
        }
    }
    let mut balance = BTreeMap::<usize, f64>::new();
    for (i, &v) in injection.iter().enumerate() {
        *balance.entry(s.components[i]).or_default() += v;
    }
    let current_scale = r
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Current))
        .map(|e| e.value.abs())
        .sum::<f64>()
        .max(1e-12);
    if balance
        .values()
        .any(|v| v.abs() > 1e-12 + current_scale * 1e-10)
    {
        return Err(error(
            "DC_OPEN_OR_FLOATING_PATH",
            "Current has no conductive/voltage-source return path; no hidden ground added",
        ));
    }
    for i in 0..s.node_count {
        if !s.gauges.contains(&i) {
            rhs[(i, 0)] = injection[i];
        }
    }
    for (k, &i) in s.voltage_elements.iter().enumerate() {
        rhs[(s.node_count + k, 0)] = r.elements[i].value;
    }
    let solve_start = Instant::now();
    let mut x = s.factor.solve(&rhs)?;
    let mut low = Mat::zeros(x.nrows(), 1);
    // Iterative refinement uses the same factorization, no approximate zero RHS.
    let mut residual = vec![0.; s.matrix.nrows()];
    for _ in 0..3 {
        apply_physical(&s, r, &x, &low, &mut residual);
        for i in 0..residual.len() {
            residual[i] = rhs[(i, 0)] - residual[i];
        }
        let correction = s
            .factor
            .solve(&Mat::from_fn(residual.len(), 1, |i, _| residual[i]))?;
        for i in 0..residual.len() {
            add_correction(&mut x[(i, 0)], &mut low[(i, 0)], correction[(i, 0)]);
        }
    }
    if !(0..x.nrows()).all(|i| x[(i, 0)].is_finite() && low[(i, 0)].is_finite()) {
        return Err(error("DC_NONFINITE", "Nonfinite sparse solution"));
    }
    apply_physical(&s, r, &x, &low, &mut residual);
    let mut row_norm = vec![0.; residual.len()];
    for j in 0..s.matrix.ncols() {
        for (i, &a) in s.matrix.row_idx_of_col(j).zip(s.matrix.val_of_col(j)) {
            row_norm[i] += a.abs();
        }
    }
    let infinity = |v: &[f64]| v.iter().map(|v| v.abs()).fold(0., f64::max);
    for i in 0..residual.len() {
        residual[i] -= rhs[(i, 0)];
    }
    let relative_residual = infinity(&residual)
        / (infinity(&row_norm) * (0..x.nrows()).map(|i| x[(i, 0)].abs()).fold(0., f64::max)
            + (0..rhs.nrows())
                .map(|i| rhs[(i, 0)].abs())
                .fold(0., f64::max))
        .max(1e-30);
    let solve_ms = solve_start.elapsed().as_secs_f64() * 1000.;
    let mut domains = vec![];
    let mut copper_loss = 0.;
    for &di in &s.included {
        let d = &m.domains[di];
        let sheet = d.sheet_conductance_s.unwrap();
        let thickness = m
            .profile
            .layers
            .iter()
            .find(|l| l.layer == d.layer)
            .and_then(|l| l.thickness_mm)
            .ok_or_else(|| error("DC_MISSING_PHYSICS", &d.layer))?;
        let voltage: Vec<_> = s.dofs[di].iter().map(|&i| x[(i, 0)] + low[(i, 0)]).collect();
        let mut losses = vec![];
        let mut density = vec![];
        let mut vector = vec![];
        for t in &d.triangles {
            let (area, g) = area_gradient(d, *t)?;
            let grad = [
                (1..3)
                    .map(|i| difference(&x, &low, s.dofs[di][t[i]], s.dofs[di][t[0]]) * g[i][0])
                    .sum::<f64>(),
                (1..3)
                    .map(|i| difference(&x, &low, s.dofs[di][t[i]], s.dofs[di][t[0]]) * g[i][1])
                    .sum::<f64>(),
            ];
            // sheet(S)*gradient(V/mm)/thickness(mm) = A/mm².
            let j = grad.map(|v| -sheet * v / thickness);
            let loss = sheet * area * (grad[0] * grad[0] + grad[1] * grad[1]);
            if !j.iter().all(|v| v.is_finite()) || !loss.is_finite() {
                return Err(error("DC_NONFINITE", "Field post-processing overflow"));
            }
            losses.push(loss);
            density.push(j[0].hypot(j[1]));
            vector.push(j);
            copper_loss += loss;
        }
        domains.push(json!({"domain_id":d.id,"layer":d.layer,"voltage_v":voltage,"current_density_a_mm2":density,"current_density_vector_a_mm2":vector,"loss_w":losses,"reference_node":s.reference[&s.components[s.dofs[di][0]]]}));
    }
    let mut barrel_loss = 0.;
    let barrels:Vec<_>=s.barrels.iter().map(|(id,from,to,a,b,g)|{
        let drop=difference(&x,&low,*a,*b);let current=drop*g;let loss=drop*current;barrel_loss+=loss;
        json!({"hole_id":id,"from_layer":from,"to_layer":to,"current_a":current,"drop_v":drop,"loss_w":loss})}).collect();
    let mut source_power = 0.;
    let mut load_power = 0.;
    let mut element_results = vec![];
    let mut solved_current_scale = current_scale;
    let mut kcl = vec![0.; s.node_count];
    for &(i, j, g) in &s.terms {
        if i != j {
            kcl[i] += g * difference(&x, &low, j, i);
        }
    }
    for (i, e) in r.elements.iter().enumerate() {
        let a = s.terminal_nodes[&e.positive];
        let b = s.terminal_nodes[&e.negative];
        let voltage = difference(&x, &low, a, b);
        let delivered = match e.kind {
            ElementKind::Voltage => {
                -x[(
                    s.node_count + s.voltage_elements.iter().position(|j| *j == i).unwrap(),
                    0,
                )]
            }
            ElementKind::Current => e.value,
            ElementKind::Resistance => -voltage / e.value,
        };
        solved_current_scale = solved_current_scale.max(delivered.abs());
        if matches!(e.kind, ElementKind::Voltage) {
            kcl[a] -= delivered;
            kcl[b] += delivered;
        }
        if e.role == "source" {
            source_power += voltage * delivered;
        } else {
            load_power -= voltage * delivered;
        }
        element_results.push(json!({"id":e.id,"kind":e.kind,"role":e.role,"positive":e.positive,"negative":e.negative,"voltage_v":voltage,"delivered_current_a":delivered,"absorbed_power_w":-voltage*delivered}));
    }
    for i in 0..kcl.len() {
        kcl[i] -= injection[i];
    }
    let current_balance = infinity(&kcl);
    let power_balance = source_power - load_power - copper_loss - barrel_loss;
    let normalized_current = current_balance / solved_current_scale;
    let normalized_power =
        power_balance.abs() / source_power.abs().max(load_power.abs()).max(1e-15);
    let voltage_constraint_error = r
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Voltage))
        .map(|e| {
            (difference(&x, &low, s.terminal_nodes[&e.positive], s.terminal_nodes[&e.negative]) - e.value)
                .abs()
        })
        .fold(0., f64::max);
    let voltage_scale = r
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Voltage))
        .map(|e| e.value.abs())
        .fold(1e-12, f64::max);
    if ![
        copper_loss,
        barrel_loss,
        source_power,
        load_power,
        current_balance,
        power_balance,
        normalized_current,
        normalized_power,
        relative_residual,
        voltage_constraint_error,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err(error(
            "DC_NONFINITE",
            "Nonfinite power, current or quality metric",
        ));
    }
    if relative_residual > 1e-8
        || current_balance > 1e-12 && normalized_current > 1e-8
        || power_balance.abs() > 1e-15 && normalized_power > 1e-8
        || voltage_constraint_error > 1e-12 && voltage_constraint_error / voltage_scale > 1e-8
    {
        return Err(error("DC_BALANCE_FAILED",format!("residual={relative_residual:e}, KCL={current_balance:e} A, power={power_balance:e} W")));
    }
    if start.elapsed().as_secs_f64() > r.limits.max_seconds {
        return Err(error("DC_DEADLINE", "Post-processing exceeded budget"));
    }
    let terminals: BTreeMap<_, _> = s
        .terminal_nodes
        .iter()
        .map(|(id, &i)| {
            (
                id,
                json!({"voltage_v":x[(i,0)]+low[(i,0)],"reference_node":s.reference[&s.components[i]],"electrical_node":i}),
            )
        })
        .collect();
    let conditional = s.included.iter().any(|&i| {
        !m.domains[i].readiness.copper_complete
            || m.diagnostics.iter().any(|warning| {
                warning.code.ends_with("_redundant")
                    && (m.domains[i].source_paths.contains(&warning.path)
                        || m.domains[i].region_ids.contains(&warning.path))
            })
    });
    Ok((
        json!({"status":"complete","diagnostics":[],"conditional":conditional,
        "included_domains":s.included.iter().map(|i|&m.domains[*i].id).collect::<Vec<_>>(),
        "excluded_domains":m.domains.iter().enumerate().filter(|(i,_)|!s.included.contains(i)).map(|(_,d)|&d.id).collect::<Vec<_>>(),
        "domains":domains,"barrels":barrels,"terminals":terminals,"elements":element_results,
        "copper_loss_w":copper_loss,"barrel_loss_w":barrel_loss,"load_power_w":load_power,"source_power_w":source_power,
        "quality":{"relative_residual":relative_residual,"current_balance_a":current_balance,"power_balance_w":power_balance,"normalized_current_balance":normalized_current,"normalized_power_balance":normalized_power,"voltage_constraint_error_v":voltage_constraint_error},
        "system":{"unknowns":s.matrix.nrows(),"nonzeros":s.matrix.val().len(),"memory_plan_bytes":s.memory_plan,"memory_metric":"symbolic sparse factor/workspace sizes plus retained-data plan, not measured peak RSS","gauges":s.gauges,"node_count":s.node_count,"solver":"faer-0.24.4 sparse QR, sequential CPU/F64","refinement_steps":3,"refinement_accumulation":"compensated F64"},
        "timings_ms":{"assembly_factorization":assembly_ms,"rhs_solve_refinement":solve_ms},"reused_factorization":reused}),
        s,
    ))
}
