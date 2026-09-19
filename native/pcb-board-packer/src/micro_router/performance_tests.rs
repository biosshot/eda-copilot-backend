use super::*;
use super::tests::{bounds, primitive};
use std::time::Instant;

// Run explicitly in release mode; timings are diagnostics, never CI assertions.
#[test]
#[ignore]
fn benchmark_route_search() {
    let config = MicroRouteConfig::post_place();
    let a = primitive("A", -8.0, -3.0, "SIG");
    let b = primitive("B", 8.0, 3.0, "SIG");
    let job = schedule_jobs(&a, &[&b], &[], &config).remove(0);
    for scenario in ["empty", "wall", "dense"] {
        let mut job = job.clone();
        if scenario == "empty" { job.target.point.y = job.source.point.y; }
        let mut obstacles = Vec::new();
        if scenario != "empty" {
            obstacles.push(StaticObstacle { box_: Box2 { left: -0.5, right: 0.5, top: -11.0, bottom: 11.0 },
                layer: Some(0), primitive_id: None, reference: None, net: None });
        }
        if scenario == "dense" {
            for i in 0..200 {
                let x = -9.0 + (i % 20) as f64 * 0.9;
                let y = -9.0 + (i / 20) as f64 * 1.8;
                obstacles.push(StaticObstacle { box_: Box2 { left: x, right: x + 0.2, top: y, bottom: y + 0.2 },
                    layer: Some(0), primitive_id: None, reference: None, net: None });
            }
        }
        let mut temporary = TemporaryRoutes::new(bounds(), config.grid, config.layers.len());
        if scenario == "dense" {
            let path: Vec<_> = (8..60).map(|x| Cell { x, y: 48, layer: 1 }).collect();
            temporary.reserve(&path, &Arc::from("OTHER"));
        }
        let expected = route_job(&job, bounds(), &[], &obstacles, &temporary, &config);
        println!("{scenario} outcome: {expected:?}");
        let mut times = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            for _ in 0..100 {
                std::hint::black_box(route_job(std::hint::black_box(&job), bounds(), &[], &obstacles, &temporary, &config));
            }
            times.push(start.elapsed().as_secs_f64() * 10.0);
        }
        times.sort_by(f64::total_cmp);
        println!("{scenario}: median {:.3} ms/job; samples {times:?}", times[2]);
    }
}
