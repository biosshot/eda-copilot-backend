use pcb_analyze::extraction::local::polyhedron::{PointPotential, TetraCubature, tetra_integral_adaptive, tetra_self_integral};
use serde_json::Value;
use std::time::Instant;

fn separated_order(q: f64) -> Option<usize> {
    if q.is_finite() && q < 0.12 { Some(4) } else if q < 0.6 { Some(6) } else if q < 1. { Some(8) } else { None }
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 { return Err("d1_local_profile <magnetic-request.json> <block-index>".into()); }
    let input: Value = serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let block = &input["blocks"][args[2].parse::<usize>().map_err(|_| "invalid block index")?];
    let tetra: Vec<[[f64; 3]; 4]> = block["cells"].as_array().unwrap().iter().map(|c| serde_json::from_value(c["verticesM"].clone()).unwrap()).collect();
    let sources: Vec<_> = tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<_, _>>()?;
    let orders = [4, 6, 8, 12];
    let cubatures: Vec<Vec<_>> = tetra.iter().copied().map(|cell| orders.iter().map(|&order| TetraCubature::new(cell, order)).collect::<Result<Vec<_>, _>>()).collect::<Result<_, _>>()?;
    let started = Instant::now();
    let mut direct = 0usize;
    let mut near = 0usize;
    let mut self_count = 0usize;
    let mut direct_seconds = 0.;
    let mut near_seconds = 0.;
    let mut self_seconds = 0.;
    for i in 0..tetra.len() {
        for j in 0..=i {
            let pair = Instant::now();
            if i == j {
                self_count += 1;
                tetra_self_integral(tetra[i], 12)?;
                self_seconds += pair.elapsed().as_secs_f64();
            } else {
                let q = cubatures[i][0].separation_q(&cubatures[j][0]);
                if let Some(index) = separated_order(q).and_then(|value| orders.iter().position(|&candidate| candidate == value)) {
                    direct += 1;
                    cubatures[i][index].direct_integral(&cubatures[j][index])?;
                    direct_seconds += pair.elapsed().as_secs_f64();
                } else {
                    near += 1;
                    tetra_integral_adaptive(tetra[i], &sources[j], 12, 1e-10)?;
                    near_seconds += pair.elapsed().as_secs_f64();
                }
            }
        }
        println!("row={i}/{} direct={direct} near={near} self={self_count} directSeconds={direct_seconds:.3} nearSeconds={near_seconds:.3} selfSeconds={self_seconds:.3} wallSeconds={:.3}", tetra.len() - 1, started.elapsed().as_secs_f64());
        if started.elapsed().as_secs_f64() > 90. { return Err("diagnostic 90 s deadline".into()); }
    }
    Ok(())
}
