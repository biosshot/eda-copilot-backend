use pcb_analyze::extraction::local::polyhedron::{PointPotential, TetraCubature, tetra_integral};
use serde_json::Value;

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 { return Err("d1_local_quadrature <magnetic-request.json> <block-index> <output.json>".into()); }
    let input: Value = serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let block = &input["blocks"][args[2].parse::<usize>().map_err(|_| "invalid block index")?];
    let tetra: Vec<[[f64; 3]; 4]> = block["cells"].as_array().unwrap().iter().map(|c| serde_json::from_value(c["verticesM"].clone()).unwrap()).collect();
    let cubatures: Vec<_> = tetra.iter().copied().map(|cell| TetraCubature::new(cell, 8)).collect::<Result<_, _>>()?;
    let mut pairs: Vec<_> = (0..tetra.len()).flat_map(|i| (0..i).map(move |j| (i, j))).map(|(i, j)| (i, j, cubatures[i].separation_q(&cubatures[j]))).filter(|v| v.2 >= 0.6 && v.2 < 1.).collect();
    pairs.sort_by(|a, b| a.2.total_cmp(&b.2));
    let mut samples = vec![];
    for band in 0..4 {
        let low = 0.6 + band as f64 * 0.1;
        let high = low + 0.1;
        let selected: Vec<_> = pairs.iter().copied().filter(|v| v.2 >= low && v.2 < high).collect();
        if selected.is_empty() { continue; }
        for index in [0, selected.len() / 2, selected.len() - 1] {
            let (i, j, q) = selected[index];
            let source = PointPotential::tetrahedron(tetra[j])?;
            let reference = tetra_integral(tetra[i], &source, 16)?;
            let direct6 = TetraCubature::new(tetra[i], 6)?.direct_integral(&TetraCubature::new(tetra[j], 6)?)?;
            let direct8 = cubatures[i].direct_integral(&cubatures[j])?;
            let direct12 = TetraCubature::new(tetra[i], 12)?.direct_integral(&TetraCubature::new(tetra[j], 12)?)?;
            samples.push(serde_json::json!({"target":i,"source":j,"q":q,"referenceM4":reference,"order6RelativeError":(direct6-reference).abs()/reference,"order8RelativeError":(direct8-reference).abs()/reference,"order12RelativeError":(direct12-reference).abs()/reference}));
            println!("band={low:.1} pair={index}/{} i={i} j={j} q={q:.6}", selected.len() - 1);
        }
    }
    std::fs::write(&args[3], serde_json::to_vec_pretty(&serde_json::json!({"samples":samples})).unwrap()).map_err(|e| e.to_string())?;
    Ok(())
}
