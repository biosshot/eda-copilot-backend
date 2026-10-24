#![allow(dead_code)]
#[path = "__MODEL__"]
mod model;
mod geometry {
    #[derive(Clone, Copy)]
    pub struct GeometryOptions {
        pub max_deviation_mm: f64,
        pub max_vertices_per_curve: usize,
        pub max_generated_vertices: usize,
    }
    pub struct PolygonSnapshot {
        pub outer: Vec<[f64; 2]>,
        pub holes: Vec<Vec<[f64; 2]>>,
    }
    #[path = "__AREAS__"]
    pub mod areas;
    #[path = "__PRIMITIVES__"]
    pub mod primitives;
}
#[path = "__FAR__"]
mod far;
#[path = "__NEAR_FAR__"]
mod near_far;
#[path = "__POLY_COUNT__"]
mod poly_count;
#[path = "__POLYHEDRON__"]
mod polyhedron;
#[path = "__PRISM__"]
mod prism;
type Result<T> = std::result::Result<T, String>;
fn error(c: &str, t: &str) -> String {
    format!("LOCAL_{c}: {t}")
}
use prism::{GaussRule, Prism};
use serde_json::{json, Value};
use std::{cell::Cell, io::Write, time::Instant};
thread_local! {static COUNTS:Cell<[u64;6]>=const {Cell::new([0;6])};}
fn record_evaluation(order: usize) {
    COUNTS.with(|c| {
        let mut a = c.get();
        a[0] += 1;
        if order > 0 {
            a[1] += 1;
            a[match order {
                15 => 2,
                31 => 3,
                _ => 4,
            }] += 1;
        }
        c.set(a);
    });
}
fn record_failure() {
    COUNTS.with(|c| {
        let mut a = c.get();
        a[5] += 1;
        c.set(a);
    });
}
fn emit(v: Value) {
    println!("{v}");
    std::io::stdout().flush().unwrap();
}
fn read(v: &Value) -> Result<Prism> {
    let mut t: [[f64; 2]; 3] = serde_json::from_value(v["triangleMm"].clone()).unwrap();
    let mut z: [f64; 2] = serde_json::from_value(v["zRangeMm"].clone()).unwrap();
    for p in &mut t {
        for x in p {
            *x *= 1e-3;
        }
    }
    for x in &mut z {
        *x *= 1e-3;
    }
    if (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]) < 0. {
        t.swap(1, 2);
    }
    Prism::new("frozen-census".into(), t, z)
}
fn geometry_replay(trace: &Value) {
    let opts = geometry::GeometryOptions {
        max_deviation_mm: 0.001,
        max_vertices_per_curve: 16384,
        max_generated_vertices: 2000000,
    };
    for track in trace["normalizedTracks"].as_array().unwrap() {
        let mut b = geometry::primitives::Builder {
            options: opts,
            generated: 0,
        };
        let points: Vec<_> = track["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| model::Point {
                x: p["x"].as_f64().unwrap(),
                y: p["y"].as_f64().unwrap(),
            })
            .collect();
        let result = b.stroke(&points, track["widthMm"].as_f64().unwrap());
        match result {
            Ok(g) => {
                let polys: Vec<_> =
                    g.0.iter()
                        .map(|p| {
                            let v = geometry::areas::snapshot(p);
                            json!({"outer":v.outer,"holes":v.holes})
                        })
                        .collect();
                emit(
                    json!({"kind":"geometry","sourceId":track["id"],"polygons":polys,"generatedVertices":b.generated}),
                );
            }
            Err(e) => emit(json!({"kind":"geometry","sourceId":track["id"],"refusal":e})),
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args[1] == "--plans" {
        for path in &args[2..] {
            let data: Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            for family in ["foil", "axialSubprisms"] {
                let prisms: Vec<_> = data[family]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| read(v).unwrap())
                    .collect();
                let budget = near_far::Budget {
                    max_bytes: 512 * 1024 * 1024,
                    max_nodes: 8192,
                    max_blocks: 100000,
                    max_pairs: 6000000,
                    max_wall: std::time::Duration::from_secs(60),
                };
                let plan = near_far::Plan::new(&prisms, 0.4, 8, budget).unwrap();
                let cells: Vec<_> = prisms
                    .iter()
                    .map(|p| json!({"triangleM":p.triangle_m,"zM":p.z_m,"volumeM3":p.volume_m3}))
                    .collect();
                let moments: Vec<_> = plan
                    .moments
                    .iter()
                    .map(|m| json!({"center":m.center_m,"radius":m.radius_m}))
                    .collect();
                let nodes:Vec<_>=plan.nodes.iter().map(|n|json!({"s":n.start,"e":n.end,"c":n.center,"r":n.radius,"children":n.children})).collect();
                let blocks: Vec<_> = plan
                    .blocks
                    .iter()
                    .map(|b| json!([b.a, b.b, b.far]))
                    .collect();
                emit(
                    json!({"kind":"plan","source":path,"family":family,"order":plan.order,"nodes":nodes,"blocks":blocks,"moments":moments,"cells":cells,"nearPairs":plan.near_pairs,"farPairs":plan.far_pairs,"kernelEvaluations":0}),
                );
            }
        }
        return;
    }
    let input: Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let trace: Value = serde_json::from_str(&std::fs::read_to_string(&args[2]).unwrap()).unwrap();
    let start = Instant::now();
    geometry_replay(&trace);
    for (index, s) in input["samples"].as_array().unwrap().iter().enumerate() {
        if start.elapsed().as_secs_f64() > 118. {
            emit(json!({"kind":"refusal","code":"DEADLINE","beforeSample":index}));
            return;
        }
        let parsed = (read(&s["cells"][0]), read(&s["cells"][1]));
        let (a, b) = match parsed {
            (Ok(a), Ok(b)) => (a, b),
            (a, b) => {
                emit(
                    json!({"kind":"case","sample":index,"refusal":format!("{:?} {:?}",a.err(),b.err())}),
                );
                continue;
            }
        };
        let self_pair = s["indices"][0] == s["indices"][1];
        let reuse = s["reusePriorTiming"].as_bool().unwrap_or(false);
        let prep = Instant::now();
        let sources = (
            polyhedron::PointPotential::new(&a),
            polyhedron::PointPotential::new(&b),
            poly_count::PointPotential::new(&a),
            poly_count::PointPotential::new(&b),
        );
        let (pa, pb, ca, cb) = match sources {
            (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
            _ => {
                emit(json!({"kind":"case","sample":index,"refusal":"POINT_POTENTIAL_PREPARATION"}));
                continue;
            }
        };
        let preparation = prep.elapsed().as_secs_f64();
        for (xy, z) in [(12, 8), (24, 16)] {
            let xr = GaussRule::new(xy).unwrap();
            let zr = GaussRule::new(z).unwrap();
            let timer = Instant::now();
            let actual = if reuse {
                None
            } else {
                let f = polyhedron::volume_integral_anisotropic(&a, &pb, &xr, &zr);
                let r = if self_pair {
                    f.clone()
                } else {
                    polyhedron::volume_integral_anisotropic(&b, &pa, &xr, &zr)
                };
                Some((f, r))
            };
            let seconds = if reuse {
                None
            } else {
                Some(timer.elapsed().as_secs_f64())
            };
            COUNTS.with(|c| c.set([0; 6]));
            let f = poly_count::volume_integral_anisotropic(&a, &cb, &xr, &zr);
            let r = if self_pair {
                f.clone()
            } else {
                poly_count::volume_integral_anisotropic(&b, &ca, &xr, &zr)
            };
            let counts = COUNTS.with(Cell::get);
            let parity = actual.as_ref().map(|(af, ar)| match (af, ar, &f, &r) {
                (Ok(a), Ok(b), Ok(c), Ok(d)) => {
                    a.to_bits() == c.to_bits() && b.to_bits() == d.to_bits()
                }
                (Err(a), Err(b), Err(c), Err(d)) => a == c && b == d,
                _ => false,
            });
            emit(
                json!({"kind":"measurement","sample":index,"xyOrder":xy,"zOrder":z,"self":self_pair,"directedCalls":if self_pair{1}else{2},"preparationSecondsBothVersions":preparation,"actualSeconds":seconds,"reusePriorTiming":reuse,"forwardM5":f,"reverseM5":r,"positiveCounters":counts,"instrumentedParity":parity,"elapsedSeconds":start.elapsed().as_secs_f64()}),
            );
        }
    }
    emit(
        json!({"kind":"done","cases":input["samples"].as_array().unwrap().len(),"seconds":start.elapsed().as_secs_f64()}),
    );
}
