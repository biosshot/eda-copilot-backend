use pcb_analyze::extraction::local::polyhedron::{PointPotential, TetraCubature, tetra_integral};
use serde_json::{json, Value};
use std::time::Instant;

fn separated_order(q: f64) -> Option<usize> {
    if q.is_finite() && q < 0.12 { Some(4) } else if q < 0.6 { Some(6) } else if q < 1. { Some(8) } else { None }
}

fn relative_error(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(f64::MIN_POSITIVE)
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 { return Err("d1_analytic_far_profile <magnetic-request.json> <block-index>".into()); }
    let input: Value = serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let block = &input["blocks"][args[2].parse::<usize>().map_err(|_| "invalid block index")?];
    let tetra: Vec<[[f64; 3]; 4]> = block["cells"].as_array().unwrap().iter().map(|c| serde_json::from_value(c["verticesM"].clone()).unwrap()).collect();
    let sources: Vec<_> = tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<_, _>>()?;
    let orders = [4, 6, 8, 16];
    let cubatures: Vec<Vec<_>> = tetra.iter().copied().map(|cell| orders.iter().map(|&order| TetraCubature::new(cell, order)).collect::<Result<Vec<_>, _>>()).collect::<Result<_, _>>()?;
    let started = Instant::now();
    let mut direct_seconds = 0.;
    let mut analytic_seconds = 0.;
    let mut reference_seconds = 0.;
    let mut pairs = 0usize;
    let mut references = 0usize;
    let mut max_direct_error = 0f64;
    let mut max_reference_error = 0f64;
    let mut max_reciprocity_error = 0f64;
    let mut worst_direct = Value::Null;
    let mut worst_reference = Value::Null;
    let mut worst_reciprocity = Value::Null;
    for i in 0..tetra.len() {
        for j in 0..i {
            let q = cubatures[i][0].separation_q(&cubatures[j][0]);
            let Some(order) = separated_order(q) else { continue; };
            let index = orders.iter().position(|&candidate| candidate == order).unwrap();
            let direct_started = Instant::now();
            let direct = cubatures[i][index].direct_integral(&cubatures[j][index])?;
            direct_seconds += direct_started.elapsed().as_secs_f64();
            let analytic_started = Instant::now();
            let analytic = tetra_integral(tetra[i], &sources[j], order)?;
            analytic_seconds += analytic_started.elapsed().as_secs_f64();
            let error = relative_error(analytic, direct);
            let pair = json!({"i":i,"j":j,"q":q,"order":order,"direct":direct,"analytic":analytic,"relativeError":error});
            if error > max_direct_error { max_direct_error = error; worst_direct = pair.clone(); }
            if pairs % 251 == 0 {
                let reference_started = Instant::now();
                let reference = tetra_integral(tetra[i], &sources[j], 16)?;
                let reverse = tetra_integral(tetra[j], &sources[i], order)?;
                reference_seconds += reference_started.elapsed().as_secs_f64();
                references += 1;
                let reference_error = relative_error(analytic, reference);
                let reciprocity_error = relative_error(analytic, reverse);
                if reference_error > max_reference_error {
                    max_reference_error = reference_error;
                    worst_reference = json!({"pair":pair,"reference":reference,"relativeError":reference_error});
                }
                if reciprocity_error > max_reciprocity_error {
                    max_reciprocity_error = reciprocity_error;
                    worst_reciprocity = json!({"pair":pair,"reverse":reverse,"relativeError":reciprocity_error});
                }
            }
            pairs += 1;
        }
        if i % 32 == 0 {
            println!("row={i}/{} pairs={pairs} directSeconds={direct_seconds:.3} analyticSeconds={analytic_seconds:.3} referenceSeconds={reference_seconds:.3}", tetra.len() - 1);
        }
        if started.elapsed().as_secs_f64() > 280. { return Err("diagnostic 280 s deadline".into()); }
    }
    let result = json!({
        "status": if max_reference_error < 1e-10 && max_reciprocity_error < 1e-10 { "pass" } else { "fail" },
        "cells": tetra.len(), "pairs": pairs, "referencePairs": references,
        "directSeconds": direct_seconds, "analyticSeconds": analytic_seconds,
        "referenceSeconds": reference_seconds, "wallSeconds": started.elapsed().as_secs_f64(),
        "maxDirectRelativeError": max_direct_error, "maxReferenceRelativeError": max_reference_error,
        "maxReciprocityRelativeError": max_reciprocity_error,
        "worstDirect": worst_direct, "worstReference": worst_reference, "worstReciprocity": worst_reciprocity,
    });
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    if result["status"] == "pass" { Ok(()) } else { Err("analytic separated-pair accuracy gate failed".into()) }
}
