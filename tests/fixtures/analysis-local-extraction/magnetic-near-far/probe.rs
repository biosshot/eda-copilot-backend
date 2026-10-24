#![allow(dead_code)]
#[path = "__FAR__"]
mod far;
#[path = "__NEAR_FAR__"]
mod near_far;
#[path = "__POLYHEDRON__"]
mod polyhedron;
#[path = "__PRISM__"]
mod prism;
type Result<T> = std::result::Result<T, String>;
fn error(c: &str, t: &str) -> String {
    format!("LOCAL_{c}: {t}")
}
use near_far::{Budget, ExactCache, Operator, Plan};
use prism::Prism;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
fn budget() -> Budget {
    Budget {
        max_bytes: 512 * 1024 * 1024,
        max_nodes: 8192,
        max_blocks: 100000,
        max_pairs: 6000000,
        max_wall: Duration::from_secs(60),
    }
}
fn read(v: &Value, mm: bool) -> Prism {
    let mut t: [[f64; 2]; 3] =
        serde_json::from_value(v[if mm { "triangleMm" } else { "triangleM" }].clone()).unwrap();
    let mut z: [f64; 2] =
        serde_json::from_value(v[if mm { "zRangeMm" } else { "zM" }].clone()).unwrap();
    if mm {
        for row in &mut t {
            for x in row {
                *x *= 1e-3;
            }
        }
        for x in &mut z {
            *x *= 1e-3;
        }
    }
    if (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]) < 0. {
        t.swap(1, 2);
    }
    Prism::new("frozen".into(), t, z).unwrap()
}
fn stats(p: &Plan) -> Value {
    json!({"supports":p.moments.len(),"nearPairs":p.near_pairs,"farPairs":p.far_pairs,"blocks":p.blocks.len(),"nodes":p.nodes.len(),"constructionSeconds":p.construction_seconds,"plannedBytes":p.planned_bytes,"nearCacheUpperBytes":p.near_cache_upper_bytes().unwrap(),"maxFarRatio":p.blocks.iter().filter(|b|b.far).map(|b|b.radius_ratio).fold(0.,f64::max)})
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn apply_dense(kernel: &[Vec<f64>], density: &[Vec<[f64; 3]>], q: &[f64]) -> Value {
    let n = density.len();
    let k = q.len();
    let mut j = vec![[0.; 3]; n];
    for i in 0..n {
        for a in 0..k {
            for d in 0..3 {
                j[i][d] += density[i][a][d] * q[a];
            }
        }
    }
    let mut a = vec![[0.; 3]; n];
    for i in 0..n {
        for t in 0..n {
            for d in 0..3 {
                a[i][d] += 1e-7 * kernel[i][t] * j[t][d];
            }
        }
    }
    let mut response = vec![0.; k];
    let mut energy = 0.;
    for i in 0..n {
        energy += 0.5 * dot(j[i], a[i]);
        for b in 0..k {
            response[b] += dot(density[i][b], a[i]);
        }
    }
    json!({"response":response,"energyJ":energy,"integratedPotential":a})
}
fn action(op: &Operator, kernel: &[Vec<f64>], q: &[f64]) -> Value {
    let x = op.apply(q).unwrap();
    json!({"coefficients":q,"response":x.response,"energyJ":x.energy_j,"absoluteTruncationEnergyBoundJ":x.absolute_truncation_energy_bound_j,"integratedPotential":x.integrated_potential,"absoluteFarPotentialBounds":x.absolute_far_potential_bounds,"dense":apply_dense(kernel,&op.densities,q)})
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let started = Instant::now();
    if args[1] == "plan" {
        let mut result = vec![];
        for path in &args[2..] {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let mut groups = vec![];
            for field in ["foil", "axialSubprisms"] {
                let ps: Vec<_> = v[field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| read(c, true))
                    .collect();
                for theta in [0.7, 0.4, 0.2] {
                    let p = match Plan::new(&ps, theta, 8, budget()) {
                        Ok(p) => p,
                        Err(e) => {
                            groups.push(json!({"kind":field,"theta":theta,"refusal":e}));
                            continue;
                        }
                    };
                    groups.push(json!({"kind":field,"theta":theta,"plan":stats(&p)}));
                }
            }
            result.push(json!({"source":path,"groups":groups}));
        }
        println!(
            "{}",
            json!({"rois":result,"seconds":started.elapsed().as_secs_f64(),"kernelEvaluations":0})
        );
        return;
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&args[2]).unwrap()).unwrap();
    let cells = v["cells"].as_array().unwrap();
    let ps = cells.iter().map(|c| read(c, false)).collect();
    let ds: Vec<Vec<[f64; 3]>> = cells
        .iter()
        .map(|c| serde_json::from_value(c["densities"].clone()).unwrap())
        .collect();
    let mut cache = ExactCache::new(ps, 24, budget()).unwrap();
    let kernel = cache.dense().unwrap();
    let mut dense = vec![vec![0.; 4]; 4];
    for a in 0..4 {
        let mut q = vec![0.; 4];
        q[a] = 1.;
        let r = apply_dense(&kernel, &ds, &q);
        for b in 0..4 {
            dense[b][a] = r["response"][b].as_f64().unwrap();
        }
    }
    let mut policies = vec![];
    for theta in [0.7, 0.4, 0.2] {
        let op = Operator::new(&mut cache, ds.clone(), theta, 1).unwrap();
        let vectors = vec![
            vec![1., -0.5, 0.2, 0.8],
            vec![1., 1., 1., 1.],
            vec![1., -1., 1., -1.],
            vec![0.; 4],
        ];
        let actions: Vec<_> = vectors.iter().map(|q| action(&op, &kernel, q)).collect();
        policies.push(json!({"theta":theta,"plan":stats(&op.plan),"matrixH":op.matrix().unwrap(),"boundMatrixH":op.bound_matrix_h,"rowsM5":op.truncation_rows_m5,"actions":actions,"farPreparationSeconds":op.far_pair_preparation_seconds,"plannedSimultaneousBytes":op.planned_bytes}));
    }
    let mut weak = vec![];
    for epsilon in [0., 1e-12] {
        let basis: Vec<Vec<[f64; 3]>> = ds
            .iter()
            .map(|d| vec![d[0], std::array::from_fn(|i| d[0][i] + epsilon * d[1][i])])
            .collect();
        let op = Operator::new(&mut cache, basis, 0.7, 1).unwrap();
        weak.push(json!({"epsilon":epsilon,"action":action(&op,&kernel,&[1.,-1.]),"relativeCertificateAvailable":false}));
    }
    println!(
        "{}",
        json!({"kernelM5":kernel,"denseMatrixH":dense,"policies":policies,"weakModes":weak,"kernelEvaluations":cache.evaluations,"kernelSeconds":cache.evaluation_seconds,"seconds":started.elapsed().as_secs_f64()})
    );
}
