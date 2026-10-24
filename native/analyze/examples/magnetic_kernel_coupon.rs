//! Offline bounded magnetic primitive/coupon probe. Actual kernel sources only.
#![allow(dead_code)]
#[path = "../src/extraction/local/polyhedron.rs"]
mod polyhedron;
#[path = "../src/extraction/local/prism.rs"]
mod prism;
type Result<T> = std::result::Result<T, String>;
fn error(code: &str, text: &str) -> String {
    format!("LOCAL_{code}: {text}")
}
use polyhedron::PointPotential;
use prism::{GaussRule, Prism};
use serde_json::{json, Value};
use std::time::Instant;
fn read(v: &Value) -> Prism {
    Prism::new(
        "frozen".into(),
        serde_json::from_value(v["triangleM"].clone()).unwrap(),
        serde_json::from_value(v["zM"].clone()).unwrap(),
    )
    .unwrap()
}
fn transform(p: &Prism, s: f64, shift: [f64; 3], rotate: bool) -> Prism {
    let t = p.triangle_m.map(|v| {
        if rotate {
            [-s * v[1] + shift[0], s * v[0] + shift[1]]
        } else {
            [s * v[0] + shift[0], s * v[1] + shift[1]]
        }
    });
    Prism::new(p.source_id.clone(), t, p.z_m.map(|z| s * z + shift[2])).unwrap()
}
fn pair(label: &str, a: &Prism, b: &Prism, xy: usize, z: usize) -> Value {
    let start = Instant::now();
    let xr = GaussRule::new(xy).unwrap();
    let zr = GaussRule::new(z).unwrap();
    let source = PointPotential::new(b).unwrap();
    let reverse = PointPotential::new(a).unwrap();
    let f = polyhedron::volume_integral_anisotropic(a, &source, &xr, &zr);
    let r = polyhedron::volume_integral_anisotropic(b, &reverse, &xr, &zr);
    json!({"label":label,"xyOrder":xy,"zOrder":z,"forwardM5":f,"reverseM5":r,"seconds":start.elapsed().as_secs_f64(),"aVolumeM3":a.volume_m3,"bVolumeM3":b.volume_m3})
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let input: Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let started = Instant::now();
    let a = read(&input["stmPair"][0]);
    let b = read(&input["stmPair"][1]);
    let mut pairs = vec![];
    for (xy, z) in [(12, 8), (24, 16)] {
        pairs.push(pair("stm668-556", &a, &b, xy, z));
    }
    if args.iter().any(|a| a == "--pair-only") {
        println!(
            "{}",
            json!({"pairs":pairs,"seconds":started.elapsed().as_secs_f64()})
        );
        return;
    }
    for (label, p) in [("stm668-self", &a), ("stm556-self", &b)] {
        for (xy, z) in [(12, 8), (24, 16)] {
            pairs.push(pair(label, p, p, xy, z));
        }
    }
    for (label, s, t, rot) in [
        ("stm-scaled", 2., [0.; 3], false),
        ("stm-translated", 1., [0.001, -0.002, 0.0003], false),
        ("stm-rotated", 1., [0.; 3], true),
    ] {
        pairs.push(pair(
            label,
            &transform(&a, s, t, rot),
            &transform(&b, s, t, rot),
            12,
            8,
        ));
    }
    let mut bars = vec![];
    let mut areas = vec![];
    for v in input["bars"].as_array().unwrap() {
        let x = v["x0M"].as_f64().unwrap();
        let y = v["y0M"].as_f64().unwrap();
        let l = v["lengthM"].as_f64().unwrap();
        let w = v["widthM"].as_f64().unwrap();
        let z: [f64; 2] = serde_json::from_value(v["zM"].clone()).unwrap();
        bars.push([
            Prism::new("a".into(), [[x, y], [x + l, y], [x + l, y + w]], z).unwrap(),
            Prism::new("b".into(), [[x, y], [x + l, y + w], [x, y + w]], z).unwrap(),
        ]);
        areas.push(w * (z[1] - z[0]));
    }
    let mut matrices = vec![];
    for (xy, z) in [(12, 8), (24, 16)] {
        let mut matrix = vec![vec![0.; bars.len()]; bars.len()];
        let mut costs = vec![];
        for i in 0..bars.len() {
            for j in 0..bars.len() {
                let before = Instant::now();
                let mut sum = 0.;
                let mut refusal = None;
                for aa in &bars[i] {
                    for bb in &bars[j] {
                        match polyhedron::volume_integral_anisotropic(
                            aa,
                            &PointPotential::new(bb).unwrap(),
                            &GaussRule::new(xy).unwrap(),
                            &GaussRule::new(z).unwrap(),
                        ) {
                            Ok(v) => sum += v,
                            Err(e) => refusal = Some(e),
                        }
                    }
                }
                matrix[i][j] = sum * 1e-7 / (areas[i] * areas[j]);
                costs.push(json!({"i":i,"j":j,"seconds":before.elapsed().as_secs_f64(),"prismPairs":4,"refusal":refusal}));
            }
        }
        matrices.push(json!({"xyOrder":xy,"zOrder":z,"rawLH":matrix,"pairCosts":costs}));
    }
    println!(
        "{}",
        json!({"pairs":pairs,"matrices":matrices,"seconds":started.elapsed().as_secs_f64()})
    );
}
