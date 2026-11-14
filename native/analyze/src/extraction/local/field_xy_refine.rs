//! Field-only in-plane (XY) refinement of the hybrid electric field mesh (`P7_FIELD_REFINE=1`, default off).
//!
//! The current-mesh potentials stay on the original product-mesh nodes. Only the electric field gets extra XY nodes:
//! every original edge is kept (constrained Delaunay), so each refined triangle lies in one original triangle and
//! inherits its copper owner and permittivity. A refined node on copper takes its potential trace by barycentric
//! interpolation of the parent nodes of the same z plane (partition of unity, positive weights).
//! Physical accuracy is unverified (approximation).
use super::{error, Result, mixed_conductor::MixedMap, layered_capacitance::{Input, PlaneInput, PotentialTrace, TraceNode, Budget, CertifiedOverlay}};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::{BTreeMap, HashMap, VecDeque};

pub(super) const RULE: &str = "triangles that have a copper sidewall edge (copper owner differs across an edge in some slab) are refined to depth L, one depth less per ring of edge neighbours; a triangle of depth d gets a 2^d barycentric lattice, edges shared with a deeper neighbour follow the deeper count; original edges are constrained, z planes unchanged";
const MAX_DEPTH: usize = 4;
pub(super) fn enabled() -> bool { std::env::var("P7_FIELD_REFINE").as_deref() == Ok("1") }
fn levels() -> usize { std::env::var("P7_FIELD_REFINE_LEVELS").ok().and_then(|v| v.parse::<usize>().ok()).map_or(3, |v| v.clamp(1, MAX_DEPTH)) }
/// `P7_FIELD_REFINE_COUNT_ONLY=1`: report the refined field node count and refuse before the solve.
fn count_only() -> bool { std::env::var("P7_FIELD_REFINE_COUNT_ONLY").as_deref() == Ok("1") }
/// Optional cap on the refined XY node count (`P7_FIELD_REFINE_MAX_XY`); the largest triangles are refined first.
fn max_xy() -> Option<usize> { std::env::var("P7_FIELD_REFINE_MAX_XY").ok().and_then(|v| v.parse::<usize>().ok()) }

pub(super) struct Refined {
    pub input: Input,
    pub trace: PotentialTrace,
    pub triangles: usize,
    pub copper: Vec<Option<usize>>,
    pub epsilon: Vec<f64>,
    pub node_owners: Vec<Vec<usize>>,
    pub finite_contacts: std::collections::BTreeSet<(usize, usize)>,
    pub rowless_modes: std::collections::BTreeSet<usize>,
    pub bytes: usize,
    pub record: serde_json::Value,
}

pub(super) fn refine(input: &Input, trace: PotentialTrace, cert: CertifiedOverlay<'_>, map: &MixedMap<'_>, budget: Budget) -> Result<Refined> {
    drop(trace);
    let started = std::time::Instant::now();
    let fail = |m: &str| error("FIELD_REFINE", m);
    let (xy, tri) = match input.meshes.as_slice() { [PlaneInput::Exact { xy_m, triangles, .. }] => (xy_m, triangles), _ => return Err(fail("one exact coarse mesh required")) };
    let (n, nt, nz) = (xy.len(), tri.len(), input.z_m.len());
    let slabs = nz.checked_sub(1).ok_or_else(|| fail("missing slabs"))?;
    if !std::ptr::eq(input, cert.input) || cert.triangles != nt || cert.copper.len() != slabs * nt || cert.epsilon.len() != slabs * nt || cert.node_owners.len() != n * nz { return Err(fail("coarse certificate mismatch")); }
    let level = levels();
    let mut edge_tris: BTreeMap<[usize; 2], Vec<usize>> = BTreeMap::new();
    for (t, v) in tri.iter().enumerate() { for k in 0..3 { let mut e = [v[k], v[(k + 1) % 3]]; e.sort(); edge_tris.entry(e).or_default().push(t); } }
    let mut nbr = vec![Vec::<usize>::new(); nt];
    for ts in edge_tris.values() { match ts[..] { [a, b] => { nbr[a].push(b); nbr[b].push(a); } [_] => {} _ => return Err(fail("nonmanifold coarse edge")) } }
    let differs = |a: usize, b: usize| (0..slabs).any(|s| cert.copper[s * nt + a] != cert.copper[s * nt + b]);
    let mut depth = vec![0usize; nt];
    let mut queue = VecDeque::new();
    let mut sidewall = 0;
    for t in 0..nt { if nbr[t].iter().any(|&u| differs(t, u)) { sidewall += 1; depth[t] = level; queue.push_back(t); } }
    while let Some(t) = queue.pop_front() { if depth[t] <= 1 { continue; } for &u in &nbr[t] { if depth[u] + 1 < depth[t] { depth[u] = depth[t] - 1; queue.push_back(u); } } }
    let mut capped = false;
    if let Some(cap) = max_xy() {
        let area = |t: usize| { let v = tri[t]; ((xy[v[1]][0] - xy[v[0]][0]) * (xy[v[2]][1] - xy[v[0]][1]) - (xy[v[1]][1] - xy[v[0]][1]) * (xy[v[2]][0] - xy[v[0]][0])).abs() };
        let mut order: Vec<usize> = (0..nt).filter(|&t| depth[t] > 0).collect();
        order.sort_by(|&a, &b| depth[b].cmp(&depth[a]).then(area(b).total_cmp(&area(a))).then(a.cmp(&b)));
        let mut spent = 0.;
        for t in order {
            let m = (1usize << depth[t]) as f64;
            let cost = (m - 1.) * (m - 2.) / 2. + 1.5 * (m - 1.);
            if (n as f64) + spent + cost > cap as f64 { depth[t] = 0; capped = true; } else { spent += cost; }
        }
    }
    let refined_triangles = depth.iter().filter(|&&d| d > 0).count();
    let mut depth_counts = vec![0usize; level + 1];
    for &d in &depth { depth_counts[d] += 1; }
    {
        let edge_depth = |e: &[usize; 2]| edge_tris[e].iter().map(|&t| depth[t]).max().unwrap_or(0);
        let mut xy_count = n;
        for e in edge_tris.keys() { xy_count += (1usize << edge_depth(e)) - 1; }
        for &d in &depth { let m = 1usize << d; xy_count += (m - 1) * m.saturating_sub(2) / 2; }
        eprintln!("P7_FIELD_REFINE_STATS levels={level} depthCounts={depth_counts:?} xyNodes={xy_count} fieldNodes={}", xy_count * nz);
        if count_only() { return Err(error("FIELD_REFINE_COUNT_ONLY", &format!("count only: xyNodes {n}->{xy_count}, fieldNodes {}->{} (planes {nz}), sidewall={sidewall} refined={refined_triangles} depthCounts={depth_counts:?}", n * nz, xy_count * nz))); }
    }
    let mut pts: Vec<[f64; 2]> = xy.clone();
    let mut support: Vec<[(usize, f64); 3]> = (0..n).map(|i| [(i, 1.), (i, 0.), (i, 0.)]).collect();
    let mut chains: BTreeMap<[usize; 2], Vec<usize>> = BTreeMap::new();
    for (e, ts) in &edge_tris {
        let d = ts.iter().map(|&t| depth[t]).max().unwrap_or(0);
        let div = 1usize << d;
        let [a, b] = *e;
        let mut chain = Vec::new();
        for k in 1..div {
            let f = k as f64 / div as f64;
            chain.push(pts.len());
            pts.push([xy[a][0] + (xy[b][0] - xy[a][0]) * f, xy[a][1] + (xy[b][1] - xy[a][1]) * f]);
            support.push([(a, 1. - f), (b, f), (a, 0.)]);
        }
        chains.insert(*e, chain);
    }
    for t in 0..nt {
        let d = depth[t];
        if d == 0 { continue; }
        let m = 1usize << d;
        let v = tri[t];
        for i in 1..m { for j in 1..m - i {
            let k = m - i - j;
            let (wa, wb, wc) = (i as f64 / m as f64, j as f64 / m as f64, k as f64 / m as f64);
            pts.push([wa * xy[v[0]][0] + wb * xy[v[1]][0] + wc * xy[v[2]][0], wa * xy[v[0]][1] + wb * xy[v[1]][1] + wc * xy[v[2]][1]]);
            support.push([(v[0], wa), (v[1], wb), (v[2], wc)]);
        } }
        if pts.len() > budget.max_nodes { return Err(error("MESH_LIMIT", "refined field XY nodes exceed explicit request cap")); }
    }
    let nf = pts.len();
    if nf.checked_mul(nz).is_none_or(|v| v > budget.max_nodes) { return Err(error("MESH_LIMIT", "refined field nodes exceed explicit request cap")); }
    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    let mut ids = Vec::with_capacity(nf);
    for p in &pts { ids.push(cdt.insert(Point2::new(p[0], p[1])).map_err(|_| fail("refined node rejected"))?); }
    if cdt.num_vertices() != nf || ids.iter().enumerate().any(|(i, h)| h.index() != i) { return Err(fail("duplicate or reordered refined node")); }
    for e in edge_tris.keys() {
        let mut seq = vec![e[0]];
        seq.extend(chains[e].iter().copied());
        seq.push(e[1]);
        for w in seq.windows(2) { cdt.add_constraint_and_split(ids[w[0]], ids[w[1]], |p| p); }
    }
    if cdt.num_vertices() != nf { return Err(fail("constraint insertion split a vertex")); }
    let fine: Vec<[usize; 3]> = cdt.inner_faces().map(|f| f.vertices().map(|v| v.fix().index())).collect();
    let ntf = fine.len();
    let by_corners: HashMap<[usize; 3], usize> = tri.iter().enumerate().map(|(t, v)| { let mut s = *v; s.sort(); (s, t) }).collect();
    let mut parent = Vec::with_capacity(ntf);
    let mut seen_parent = vec![false; nt];
    for f in &fine {
        let mut set = [0usize; 9];
        let mut cnt = 0;
        for &v in f { for &(c, w) in &support[v] { if w > 0. && !set[..cnt].contains(&c) { set[cnt] = c; cnt += 1; } } }
        if cnt != 3 { return Err(fail("refined triangle is not inside one coarse triangle")); }
        let mut key = [set[0], set[1], set[2]];
        key.sort();
        let p = *by_corners.get(&key).ok_or_else(|| fail("refined triangle has no coarse parent"))?;
        seen_parent[p] = true;
        parent.push(p);
    }
    if seen_parent.iter().any(|&s| !s) { return Err(fail("coarse triangle without refined children")); }
    let (count, coarse_count) = (slabs.checked_mul(ntf).ok_or_else(|| error("MEMORY_LIMIT", "refined certificate overflow"))?, slabs * nt);
    if count > budget.max_local_terms { return Err(error("MESH_LIMIT", "refined local term count exceeds request cap")); }
    let bytes = cert.bytes.checked_add((count - coarse_count.min(count)) * 64).and_then(|v| v.checked_add((nf * nz - n * nz) * 128)).ok_or_else(|| error("MEMORY_LIMIT", "refined certificate overflow"))?;
    if bytes > budget.max_bytes { return Err(error("MEMORY_LIMIT", "refined certificate exceeds budget")); }
    let mut copper = vec![None; count];
    let mut epsilon = vec![0.; count];
    for (tf, &p) in parent.iter().enumerate() { for s in 0..slabs { copper[s * ntf + tf] = cert.copper[s * nt + p]; epsilon[s * ntf + tf] = cert.epsilon[s * nt + p]; } }
    let mut owners = vec![Vec::<usize>::new(); nf * nz];
    for s in 0..slabs { for tf in 0..ntf { if let Some(m) = copper[s * ntf + tf] { for &v in &fine[tf] { for k in [s, s + 1] { let o = &mut owners[k * nf + v]; if !o.contains(&m) { o.push(m); } } } } } }
    for k in 0..nz { for v in 0..n {
        let (mut a, mut b) = (owners[k * nf + v].clone(), cert.node_owners[k * n + v].clone());
        a.sort(); b.sort();
        if a != b { return Err(fail("owners of an original node changed")); }
    } }
    let mut weights: Vec<Option<Vec<(usize, f64)>>> = vec![None; n * nz];
    for row in map.electric_node_trace() { let slot = weights.get_mut(row.node).ok_or_else(|| fail("trace node index"))?; if slot.replace(row.weights).is_some() { return Err(fail("duplicate trace node")); } }
    let mut nodes = Vec::new();
    for k in 0..nz { for v in 0..nf {
        let own = &owners[k * nf + v];
        if own.is_empty() { continue; }
        let row: Vec<(usize, f64)> = if v < n { weights[k * n + v].clone().ok_or_else(|| fail("original node trace missing"))? } else {
            let mut acc = BTreeMap::<usize, f64>::new();
            for &(c, w) in &support[v] {
                if w == 0. { continue; }
                if own.iter().any(|&m| !cert.node_owners[k * n + c].contains(&m)) { return Err(fail("parent node not owned by the metal of a refined node")); }
                for &(i, x) in weights[k * n + c].as_ref().ok_or_else(|| fail("parent node trace missing"))? { *acc.entry(i).or_default() += w * x; }
            }
            let row: Vec<_> = acc.into_iter().filter(|(_, x)| *x > 1e-14).collect();
            if row.is_empty() || row.iter().any(|&(i, x)| i >= map.potential_count() || !x.is_finite() || x < 0.) || (row.iter().map(|x| x.1).sum::<f64>() - 1.).abs() > 1e-12 { return Err(fail("invalid interpolated partition of unity")); }
            row
        };
        for &m in own { nodes.push(TraceNode { metal_source_id: input.metals[m].source_id.clone(), position_m: [pts[v][0], pts[v][1], input.z_m[k]], weights: row.clone() }); }
    } }
    if nodes.len() > budget.max_nodes.saturating_mul(4) || bytes.checked_add(nodes.len().saturating_mul(256)).is_none_or(|v| v > budget.max_bytes) { return Err(error("MEMORY_LIMIT", "refined trace exceeds budget")); }
    let mut fine_input = input.clone();
    fine_input.meshes = vec![PlaneInput::Exact { source_id: "board-overlay-xy-refined".into(), xy_m: pts, triangles: fine }];
    let record = serde_json::json!({"levels": level, "depthCounts": depth_counts, "fieldNodes": nf * nz, "coarseFieldNodes": n * nz, "xyNodes": nf, "coarseXyNodes": n, "planes": nz, "triangles": ntf, "coarseTriangles": nt,
        "sidewallTriangles": sidewall, "refinedTriangles": refined_triangles, "capped": capped, "rule": RULE, "status": "approximation",
        "statusNote": "field mesh refined in XY only near copper sidewalls; potentials stay on the original current-mesh nodes and refined copper nodes interpolate from them; field accuracy is not verified against a reference"});
    eprintln!("P7_FIELD_REFINE levels={level} xyNodes {n}->{nf} fieldNodes {}->{} triangles {nt}->{ntf} sidewall={sidewall} refined={refined_triangles} capped={capped} elapsed={:.3}s", n * nz, nf * nz, started.elapsed().as_secs_f64());
    Ok(Refined { input: fine_input, trace: PotentialTrace { potential_nodes: (0..map.potential_count()).map(|i| format!("mixed:{i}")).collect(), nodes }, triangles: ntf, copper, epsilon, node_owners: owners, finite_contacts: cert.finite_contacts, rowless_modes: cert.rowless_modes, bytes, record })
}
