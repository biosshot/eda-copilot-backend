//! Bounded DC graph falsification only; never fabricates EM RLGC.
use faer::{prelude::Solve, Mat, Side};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    time::Instant,
};

fn solve(case: &Value) -> Result<Value, String> {
    let start = Instant::now();
    let model = &case["model"];
    let nodes = model["nodes"].as_array().ok_or("nodes required")?;
    let edges = model["edges"].as_array().ok_or("edges required")?;
    let ports = model["ports"].as_array().ok_or("ports required")?;
    let n = nodes.len();
    let p = ports.len();
    if n == 0 || n > 256 || edges.len() > 4096 || p == 0 || p > 16 {
        return Err("bounded capacity: 1..256 nodes, <=4096 edges, 1..16 ports".into());
    }
    let mut ids = BTreeMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let id = node["id"].to_string();
        if id == "null" || ids.insert(id, i).is_some() {
            return Err("missing/duplicate node id".into());
        }
    }
    let index = |v: &Value| {
        ids.get(&v.to_string())
            .copied()
            .ok_or_else(|| "unknown node".to_string())
    };
    let mut adjacency = vec![vec![]; n];
    let mut ee = vec![];
    for e in edges {
        let a = index(&e["a"])?;
        let b = index(&e["b"])?;
        let r = e["resistance_ohm"].as_f64().ok_or("R required")?;
        if a == b || !r.is_finite() || !(1e-12..=1e12).contains(&r) {
            return Err("distinct edge ends and finite R in [1e-12,1e12] required".into());
        }
        adjacency[a].push(b);
        adjacency[b].push(a);
        ee.push((a, b, r));
    }
    let mut components = vec![usize::MAX; n];
    let mut roots = vec![];
    for i in 0..n {
        if components[i] != usize::MAX {
            continue;
        }
        let c = roots.len();
        roots.push(i);
        let mut stack = vec![i];
        components[i] = c;
        while let Some(a) = stack.pop() {
            for &b in &adjacency[a] {
                if components[b] == usize::MAX {
                    components[b] = c;
                    stack.push(b);
                }
            }
        }
    }
    let mut pp = vec![];
    for port in ports {
        let a = index(&port["positive"])?;
        let b = index(&port["negative"])?;
        if a == b || components[a] != components[b] {
            return Err("port has coincident terminals or unclosed physical return".into());
        }
        pp.push((a, b));
    }
    let free = (0..n).filter(|i| !roots.contains(i)).collect::<Vec<_>>();
    let m = free.len();
    let mut map = vec![None; n];
    for (j, &i) in free.iter().enumerate() {
        map[i] = Some(j);
    }
    let mut k = Mat::<f64>::zeros(m, m);
    for &(a, b, r) in &ee {
        let g = 1. / r;
        for (i, j, v) in [(a, a, g), (b, b, g), (a, b, -g), (b, a, -g)] {
            if let (Some(i), Some(j)) = (map[i], map[j]) {
                k[(i, j)] += v;
            }
        }
    }
    let rhs = Mat::from_fn(m, p, |i, j| {
        let (a, b) = pp[j];
        if free[i] == a {
            1.
        } else if free[i] == b {
            -1.
        } else {
            0.
        }
    });
    let assembly_ms = start.elapsed().as_secs_f64() * 1000.;
    let factor_start = Instant::now();
    let factor = k
        .llt(Side::Lower)
        .map_err(|_| "SPD factor refused; no regularization")?;
    let factor_ms = factor_start.elapsed().as_secs_f64() * 1000.;
    let solve_start = Instant::now();
    let x = factor.solve(&rhs);
    let mut v = vec![vec![0.; n]; p];
    for j in 0..p {
        for (i, &node) in free.iter().enumerate() {
            v[j][node] = x[(i, j)];
        }
    }
    let z = (0..p)
        .map(|i| {
            (0..p)
                .map(|j| v[j][pp[i].0] - v[j][pp[i].1])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let currents = (0..p)
        .map(|j| {
            ee.iter()
                .map(|&(a, b, r)| (v[j][a] - v[j][b]) / r)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut residual: f64 = 0.;
    let mut power_error: f64 = 0.;
    let mut reciprocity: f64 = 0.;
    for j in 0..p {
        let mut net = vec![0.; n];
        let mut power = 0.;
        for (e, &(a, b, r)) in ee.iter().enumerate() {
            let i = currents[j][e];
            net[a] += i;
            net[b] -= i;
            power += r * i * i;
        }
        net[pp[j].0] -= 1.;
        net[pp[j].1] += 1.;
        for r in net {
            residual = residual.max(r.abs());
        }
        power_error = power_error.max((power - z[j][j]).abs() / z[j][j].abs().max(1e-12));
        for i in 0..p {
            reciprocity = reciprocity.max((z[i][j] - z[j][i]).abs());
        }
    }
    if residual > 1e-8
        || power_error > 1e-8
        || reciprocity > 1e-8
        || z.iter()
            .enumerate()
            .any(|(i, row)| !row[i].is_finite() || row[i] <= 0.)
    {
        return Err(format!("numerical certification failed: KCL={residual},power={power_error},reciprocity={reciprocity}"));
    }
    let nnz = (0..m)
        .map(|i| (0..m).filter(|&j| k[(i, j)] != 0.).count())
        .sum::<usize>();
    Ok(
        json!({"id":case["id"],"status":"ok","units":"ohm","band_hz":[0,0],"ports":ports,"impedance_ohm":z,"edge_currents_a_per_port_amp":currents,"node_voltages_v_per_port_amp":v,"quality":{"kcl_abs_a":residual,"relative_power_error":power_error,"reciprocity_abs_ohm":reciprocity},"resources":{"full_potential_nodes":n,"gauge_reduced_unknowns":m,"port_reduced_size":p,"nnz_full_gauged":nnz,"stored_factor_entries":m*m,"dense_payload_estimate_bytes":8*(3*m*m+3*m*p+2*n*p+edges.len()*p),"estimate_excludes":"allocator/faer scratch/process runtime; not RSS or commit","assembly_ms":assembly_ms,"factor_ms":factor_ms,"solve_and_certify_ms":solve_start.elapsed().as_secs_f64()*1000.,"total_ms":start.elapsed().as_secs_f64()*1000.},"unsupported_effects":["L/C/G","propagation","skin/proximity","inductive/capacitive crosstalk","arbitrary polygon extraction"]}),
    )
}
fn main() -> Result<(), String> {
    let mut text = String::new();
    io::stdin()
        .take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 8 * 1024 * 1024 {
        return Err("input exceeds 8 MiB".into());
    }
    let input: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if input["schema"] != "p7-shared-return-benchmark-v1" {
        return Err("unsupported manifest schema".into());
    }
    let cases = input["cases"].as_array().ok_or("cases required")?;
    if cases.len() > 64 {
        return Err("maximum 64 cases".into());
    }
    let start = Instant::now();
    let mut out = vec![];
    for case in cases {
        if start.elapsed().as_secs() > 10 {
            return Err("10-second bounded attempt exceeded".into());
        }
        out.push(
            solve(case)
                .unwrap_or_else(|e| json!({"id":case["id"],"status":"unsupported","diagnostic":e})),
        );
    }
    println!(
        "{}",
        json!({"schema":"p7-shared-return-output-v1","physics":"DC finite resistor graph only","integration_decision":"no-go","cases":out})
    );
    Ok(())
}
