use super::*;
use std::time::Duration;

#[test]
fn prepared_spatial_action_matches_capacitance_and_preserves_gauge() {
    let (mut input, trace)=coupon();
    input.z_m.insert(2,0.00055);
    input.planes=vec![0;input.z_m.len()];
    let reference=extract_traced_spatial(&input,&trace,&[region()],budget()).unwrap();
    let action=prepare_traced_spatial_action(&input,&trace,&[region()],budget()).unwrap();
    assert!(action.solution.plan.free_nodes>0);
    assert!(action.solution.capacitance_f.is_empty());
    assert!(action.solution.basis_potentials_v.is_empty());
    assert_eq!(action.max_epsilon_r(),4.);
    let scale=reference.capacitance_f.iter().flatten().map(|v|v.abs()).fold(0_f64,f64::max);
    for v in [[1.,0.],[0.,1.],[1.,-2.],[1.,1.]] {
        let q=action.apply(&v).unwrap();
        for i in 0..2 {
            let expected=reference.capacitance_f[i].iter().zip(v).map(|(c,v)|c*v).sum::<f64>();
            assert!((q[i]-expected).abs()<1e-9*scale);
        }
    }
    assert_eq!(action.apply(&[0.,0.]).unwrap(),vec![0.,0.]);
    assert!(action.apply(&[1.]).is_err());
    assert!(action.apply(&[f64::NAN,0.]).is_err());
    assert_eq!(action.action_count(),5);
    assert!(prepare_traced_spatial_action(&input,&trace,&[region()],Budget{max_wall:Duration::ZERO,..budget()}).is_err());
}

fn coupon() -> (Input, PotentialTrace) {
    let xy = vec![[0.,0.], [0.001,0.], [0.001,0.001], [0.,0.001]];
    let z = vec![0., 0.0001, 0.001, 0.0011];
    let metals: Vec<_> = [(z[0],z[1]),(z[2],z[3])].into_iter().enumerate().map(|(i,(a,b))| Metal {
        source_id: format!("plate-{i}"), polygon_m: xy.clone(), holes_m: vec![], z0_m:a, z1_m:b,
    }).collect();
    let nodes = metals.iter().enumerate().flat_map(|(i,m)| [m.z0_m,m.z1_m].into_iter().flat_map({ let xy=xy.clone(); let name=m.source_id.clone(); move |height| { let name=name.clone(); xy.clone().into_iter().map(move |p| TraceNode { metal_source_id:name.clone(), position_m:[p[0],p[1],height], weights:vec![(i,1.)] }) } })).collect();
    let input = Input { meshes:vec![PlaneInput::Exact { source_id:"grid".into(), xy_m:xy, triangles:vec![[0,1,2],[0,2,3]] }], planes:vec![0;4], z_m:z, xy_bounds_m:[[0.,0.],[0.001,0.001]], metals, dielectrics:vec![Dielectric { source_id:"background".into(), z0_m:0.,z1_m:0.0011,epsilon_r:1. }], exterior:Exterior::InsulatedAnalyticCoupon };
    (input, PotentialTrace { potential_nodes:vec!["lower".into(),"upper".into()], nodes })
}
fn budget() -> Budget { Budget { max_nodes:1000, max_local_terms:100_000, max_bytes:64*1024*1024, max_wall:Duration::from_secs(20) } }
fn region() -> SpatialDielectric { SpatialDielectric { z0_m:0., z1_m:0.0011, epsilon_r:4., polygon_m:vec![[0.,0.],[0.001,0.],[0.001,0.001],[0.,0.001]], holes_m:vec![] } }

#[test]
fn uniform_spatial_dielectric_matches_layer_and_parallel_plate_energy() {
    let (mut input, trace) = coupon();
    let spatial = extract_traced_spatial(&input, &trace, &[region()], budget()).unwrap();
    input.dielectrics[0].epsilon_r = 4.;
    let layered = extract_traced(&input, &trace, budget()).unwrap();
    let energy = spatial.energy_capacitance_f().unwrap();
    let expected = EPS0 * 4. * 1e-6 / 0.0009;
    for i in 0..2 { for j in 0..2 {
        let target = if i == j { expected } else { -expected };
        assert!((spatial.capacitance_f[i][j]-target).abs() < expected*1e-9);
        assert!((spatial.capacitance_f[i][j]-layered.capacitance_f[i][j]).abs() < expected*1e-9);
        assert!((energy[i][j]-target).abs() < expected*1e-9);
        assert!((spatial.capacitance_f[i][j]-spatial.capacitance_f[j][i]).abs() < expected*1e-9);
    }}
    assert_eq!(spatial.max_epsilon_r(), 4.);
}

#[test]
fn dielectric_boundary_cutting_a_cell_is_refused() {
    let (input, trace) = coupon();
    let mut partial=region();
    partial.holes_m = vec![vec![[0.0002,0.0002],[0.0004,0.0002],[0.0004,0.0004],[0.0002,0.0004]]];
    assert!(extract_traced_spatial(&input, &trace, &[partial], budget()).is_err());
}

#[test]
fn spatial_route_enforces_count_budget_before_assembly() {
    let (mut input, trace)=coupon();
    input.metals = (0..17).map(|i| { let mut metal=input.metals[0].clone(); metal.source_id=format!("m{i}"); metal }).collect();
    let mut limited=budget(); limited.max_local_terms=16;
    let error=extract_traced_spatial(&input,&trace,&[],limited).err().expect("budget refusal");
    assert!(format!("{error:?}").contains("FIELD_INPUT"));
}

#[test]
fn prepared_cut_flux_matches_parallel_plate_field_and_rejects_invalid_cuts() {
    let (input, trace) = coupon();
    let action = prepare_traced_spatial_action(&input, &trace, &[region()], budget()).unwrap();
    let bounds = [[0.0002, 0.0002, 0.0002], [0.0008, 0.0008, 0.0009]];
    let flux = action
        .cut_displacement_flux(&[0., 1.], &[0., 2.], bounds)
        .unwrap();
    let externally_supervised = Budget {
        max_nodes: usize::MAX,
        max_local_terms: usize::MAX,
        max_bytes: usize::MAX,
        max_wall: Duration::MAX,
    };
    let unbounded = prepare_traced_spatial_action(&input, &trace, &[region()], externally_supervised).unwrap();
    assert_eq!(unbounded.cut_displacement_flux(&[0., 1.], &[0., 2.], bounds).unwrap(), flux);
    let expected = EPS0 * 4. * 0.0006_f64.powi(2) * 5_f64.sqrt() / 0.0009;
    for &f in &flux[..4] {
        assert!(f < expected * 1e-10);
    }
    for &f in &flux[4..] {
        assert!((f / expected - 1.).abs() < 1e-10, "{flux:?}");
    }
    let shifted = action
        .cut_displacement_flux(&[7., 8.], &[-3., -1.], bounds)
        .unwrap();
    for i in 0..6 {
        assert!((shifted[i] - flux[i]).abs() < expected * 1e-10);
    }
    assert_eq!(
        action
            .cut_displacement_flux(&[0., 0.], &[0., 0.], bounds)
            .unwrap(),
        [0.; 6]
    );
    // A box touching or wider than the field plane is measured at the plane limit; one entirely outside it is refused.
    assert!(action
        .cut_displacement_flux(&[0., 1.], &[0., 0.], [[0., 0.0002, 0.0002], bounds[1]])
        .is_ok());
    assert!(action
        .cut_displacement_flux(&[0., 1.], &[0., 0.], [[1., 1., 0.0002], [2., 2., bounds[1][2]]])
        .is_err());
    let copper_bounds = [[0.0002, 0.0002, 0.00005], bounds[1]];
    let copper_flux = action
        .cut_displacement_flux(&[0., 1.], &[0., 2.], copper_bounds)
        .unwrap();
    assert_eq!(copper_flux[4], 0.);
    assert!((copper_flux[5] / expected - 1.).abs() < 1e-10);
    let mut missing = prepare_traced_spatial_action(&input, &trace, &[region()], budget()).unwrap();
    missing.solution.copper_cells.clear();
    assert!(missing
        .cut_displacement_flux(&[0., 1.], &[0., 0.], copper_bounds)
        .is_err());
    // Manufactured linear nodal field: real gradient (2,3,0), imaginary (4,0,0).
    let mut real = Vec::new();
    let mut imag = Vec::new();
    for &plane in &action.solution.planes {
        for p in &action.solution.meshes[plane].xy {
            real.push(2. * p[0] + 3. * p[1]);
            imag.push(4. * p[0]);
        }
    }
    let xy_flux = action.cut_nodal_flux(&real, &imag, bounds).unwrap();
    let vertical_area = 0.0006 * 0.0007;
    let x_expected = EPS0 * 4. * vertical_area * 20_f64.sqrt();
    let y_expected = EPS0 * 4. * vertical_area * 3.;
    for &v in &xy_flux[..2] {
        assert!((v / x_expected - 1.).abs() < 1e-10, "{xy_flux:?}");
    }
    for &v in &xy_flux[2..4] {
        assert!((v / y_expected - 1.).abs() < 1e-10, "{xy_flux:?}");
    }
    for &v in &xy_flux[4..] {
        assert!(v < x_expected * 1e-10);
    }
    let mut limited = prepare_traced_spatial_action(&input, &trace, &[region()], budget()).unwrap();
    limited.solution.budget.max_bytes = limited.solution.plan.planned_simultaneous_bytes;
    assert!(limited
        .cut_displacement_flux(&[0., 1.], &[0., 0.], bounds)
        .is_err());
}

// ---- P7-R stage 4.1.3zh diagnostic (ignored): remaining 3D-vs-2D capacitance excess of the straight microstrip coupon. ----
// `CAP_MODE=3d` (default): the layered field builder on the coupon geometry, trace/return capacitance per metre from a
// length difference (`CAP_LENGTHS_MM`, default "10,5"). Env: CAP_LEVEL (0 = corner seeds only, 1..3 graded XY grid),
// CAP_IFC_UM (interface step, 0 = coarse z), CAP_MARGIN_MM (box margin), CAP_BOX (1 grounded InfinityBox, 0 insulated),
// CAP_FIRST_UM (first air step), CAP_NOMID (no mid-copper planes), CAP_EPS (substrate), CAP_SUB_FULL (1 = substrate over the whole box), CAP_LAYER_EPS (copper-layer fill outside metal).
// `CAP_MODE=2d`: the polygon section of the same cross-section. Env: CAP_AREA_M2, CAP_MARGIN_MM, CAP_BOX (1 = grounded
// frame conductor as reference, 0 = return is the reference, natural outer boundary), CAP_EPS, CAP_PROD (production domain), CAP_LOSS (planner path with a loss input, refused here: SECTION_SKIN_DEPTH; default is the plain electrostatic solve).
// Physical accuracy unverified. Run: cargo test -j 4 --release diag_coupon_field_capacitance -- --ignored --nocapture
fn env_f(name: &str, default: f64) -> f64 { std::env::var(name).ok().map_or(default, |v| v.parse().unwrap()) }

fn graded_lines(features: &[f64], lo: f64, hi: f64, first: f64, max_step: f64) -> Vec<f64> {
    let mut v: Vec<f64> = features.to_vec();
    if first > 0. {
        for &f in features {
            for sign in [-1., 1.] {
                let mut d = first;
                while f + sign * d > lo + 1e-9 && f + sign * d < hi - 1e-9 && d < (hi - lo) { let c = f + sign * d; if !features.iter().any(|g| (g - c).abs() < 1e-7) { v.push(c); } d *= 2.; }
            }
        }
    }
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let mut out = vec![];
    for w in v.windows(2) {
        out.push(w[0]);
        let gap = w[1] - w[0];
        if max_step > 0. && gap > 8. * max_step { let n = (gap / (8. * max_step)).ceil() as usize; for k in 1..n { out.push(w[0] + gap * k as f64 / n as f64); } }
    }
    out.push(*v.last().unwrap());
    out
}

#[test]
#[ignore = "diagnostic: stage 4.1.3zh remaining capacitance of the coupon, box / XY mesh / 2D section"]
fn diag_coupon_field_capacitance() {
    use crate::extraction::polygon_section as ps;
    let mode = std::env::var("CAP_MODE").unwrap_or_else(|_| "3d".into());
    let eps_sub = env_f("CAP_EPS", 4.);
    let margin_mm = env_f("CAP_MARGIN_MM", 2.);
    let grounded = env_f("CAP_BOX", 1.) != 0.;
    let (t, h) = (1e-5, 1e-4);
    let total = 2. * t + h;
    if mode == "2d" {
        let area = env_f("CAP_AREA_M2", 1e-8);
        let prod = std::env::var("CAP_PROD").is_ok();
        let (x_half, air) = if prod { (1.29e-3, 1e-3) } else { ((0.6 + margin_mm) * 1e-3, margin_mm * 1e-3) };
        let rect = |x0: f64, z0: f64, x1: f64, z1: f64| ps::Contour { outer_m: vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]], holes_m: vec![] };
        let mut dielectrics = vec![
            ps::MaterialRegion { id: "upper-air".into(), relative_permittivity: 1., contour: rect(-x_half, -air, x_half, 0.) },
            ps::MaterialRegion { id: "lower-air".into(), relative_permittivity: 1., contour: rect(-x_half, total, x_half, total + air) },
        ];
        for (k, (z0, z1, e)) in [(0., t, 1.), (t, t + h, eps_sub), (t + h, total, 1.)].into_iter().enumerate() {
            for (j, (a, b, inside)) in [(-x_half, -0.6e-3, false), (-0.6e-3, 0.6e-3, true), (0.6e-3, x_half, false)].into_iter().enumerate() {
                dielectrics.push(ps::MaterialRegion { id: format!("m{k}{j}"), relative_permittivity: if inside { e } else { 1. }, contour: rect(a, z0, b, z1) });
            }
        }
        let mut conductors = vec![
            ps::Metal { id: "trace".into(), role: "signal".into(), contour: rect(-0.05e-3, 0., 0.05e-3, t) },
            ps::Metal { id: "return".into(), role: if grounded { "signal" } else { "reference" }.into(), contour: rect(-0.25e-3, t + h, 0.25e-3, total) },
        ];
        if grounded {
            let (g, w) = (30e-6, 30e-6);
            conductors.push(ps::Metal { id: "box".into(), role: "reference".into(), contour: ps::Contour {
                outer_m: vec![[-x_half + g, -air + g], [x_half - g, -air + g], [x_half - g, total + air - g], [-x_half + g, total + air - g]],
                holes_m: vec![vec![[-x_half + g + w, -air + g + w], [x_half - g - w, -air + g + w], [x_half - g - w, total + air - g - w], [-x_half + g + w, total + air - g - w]]] } });
        }
        let loss = if std::env::var("CAP_LOSS").is_err() { None } else {
            let n = conductors.len(); // the planner always supplies the loss input; it also sets the corner grading of the section mesh
            Some(ps::LossInput { frequencies_hz: vec![1e8], conductivity_s_m: vec![58e6; n], minimum_skin_dimensions_m: vec![t; n], dielectric_loss_tangents: vec![0.; dielectrics.len()], band_hz: [1e8, 1e8] })
        };
        let request = ps::Request { loss, domain: rect(-x_half, -air, x_half, total + air), conductors, dielectrics, max_triangle_area_m2: area, max_nodes: 200_000, max_seconds: 300., max_memory_mib: 2048 };
        let out = if request.loss.is_some() {
            // The planner path (levels 0/1, corner grading from the skin dimension); `area` is its level-0 cell area.
            let wrapped = serde_json::json!({"line": {"kind": "polygon-section", "geometry": &request, "lengthMm": 10.}, "frequencies": [1e8]});
            let transport = ps::run_transport(serde_json::from_value(wrapped).unwrap());
            eprintln!("CAP2D_LEVELS {}", transport["convergence"].as_array().map_or(String::new(), |l| l.iter().map(|v| format!("{}:{}tri", v["maxTriangleAreaM2"], v["triangles"])).collect::<Vec<_>>().join(" ")));
            if transport["parameters"].is_null() { eprintln!("CAP2D_REFUSED {transport}"); return; }
            transport
        } else { match ps::solve(&request) { Ok(o) => o, Err(e) => { eprintln!("CAP2D_REFUSED {e:?}"); return; } } };
        eprintln!("CAP2D {}", serde_json::json!({"eps": eps_sub, "marginMm": margin_mm, "grounded": grounded, "area": area, "prod": prod,
            "order": out["conductorOrder"], "loss": request.loss.is_some(), "cPerM": out["parameters"]["capacitanceFPerM"], "vacuumCPerM": out["parameters"]["vacuumCapacitanceFPerM"], "nodes": out["nodesM"].as_array().map(|n| n.len())}));
        return;
    }
    let level = env_f("CAP_LEVEL", 0.) as usize;
    let ifc = env_f("CAP_IFC_UM", 25.) * 1e-6;
    let sub_full = env_f("CAP_SUB_FULL", 0.) != 0.;
    let layer_eps = env_f("CAP_LAYER_EPS", 1.);
    let lengths: Vec<f64> = std::env::var("CAP_LENGTHS_MM").unwrap_or_else(|_| "10,5".into()).split(',').map(|v| v.parse().unwrap()).collect();
    let margin = margin_mm * 1e-3;
    let mut results = vec![];
    for &l_mm in &lengths {
        let m = |v: f64| v * 1e-3;
        let (bx0, bx1, by0, by1) = (-0.3 - margin_mm, l_mm + 0.3 + margin_mm, -0.6 - margin_mm, 0.6 + margin_mm);
        let rects_mm: Vec<[f64; 4]> = vec![[bx0, by0, bx1, by1], [-0.3, -0.6, l_mm + 0.3, 0.6], [-0.1, -0.05, l_mm + 0.1, 0.05], [-0.25, -0.25, l_mm + 0.25, 0.25]];
        let (sx, sy, dx, dy) = match level { 1 => (0.1, 0.05, 1., 0.5), 2 => (0.05, 0.025, 0.5, 0.25), 3 => (0.025, 0.0125, 0.25, 0.125), _ => (0., 0., 0., 0.) };
        let mut seeds: Vec<[f64; 2]> = vec![]; let mut constraints: Vec<[usize; 2]> = vec![];
        if level == 0 {
            for r in &rects_mm { let base = seeds.len(); seeds.extend([[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]); for k in 0..4 { constraints.push([base + k, base + (k + 1) % 4]); } }
        } else {
            let xs = graded_lines(&[bx0, -0.3, -0.25, -0.1, l_mm + 0.1, l_mm + 0.25, l_mm + 0.3, bx1], bx0, bx1, sx, dx);
            let ys = graded_lines(&[by0, -0.6, -0.25, -0.05, 0.05, 0.25, 0.6, by1], by0, by1, sy, dy);
            let index = |x: f64, y: f64| -> usize { let i = xs.iter().position(|v| (v - x).abs() < 1e-9).unwrap(); let j = ys.iter().position(|v| (v - y).abs() < 1e-9).unwrap(); i * ys.len() + j };
            for &x in &xs { for &y in &ys { seeds.push([x, y]); } }
            for r in &rects_mm {
                let along_x: Vec<f64> = xs.iter().copied().filter(|v| *v >= r[0] - 1e-9 && *v <= r[2] + 1e-9).collect();
                let along_y: Vec<f64> = ys.iter().copied().filter(|v| *v >= r[1] - 1e-9 && *v <= r[3] + 1e-9).collect();
                for pair in along_x.windows(2) { for y in [r[1], r[3]] { constraints.push([index(pair[0], y), index(pair[1], y)]); } }
                for pair in along_y.windows(2) { for x in [r[0], r[2]] { constraints.push([index(x, pair[0]), index(x, pair[1])]); } }
            }
        }
        let seeds_m: Vec<[f64; 2]> = seeds.iter().map(|p| [m(p[0]), m(p[1])]).collect();
        let sub = if sub_full { rects_mm[0] } else { rects_mm[1] };
        // z planes (metres): the board builder layout, TOP copper [0,t], substrate, BOTTOM copper [t+h, total].
        let first = if env_f("CAP_FIRST_UM", 0.) > 0. { env_f("CAP_FIRST_UM", 0.) * 1e-6 } else if ifc > 0. { ifc.min(2e-3).min(margin) } else { 2e-3_f64.min(margin) };
        let mut air = vec![]; let mut d = first; loop { air.push(d); if d == margin { break; } d = (2. * d).min(margin); }
        let mut z: Vec<f64> = air.iter().map(|d| -d).collect();
        z.extend([0., t, t + h, total]); if env_f("CAP_NOMID", 0.) == 0. { z.extend([t / 2., t + h + t / 2.]); }
        if ifc > 0. { let mut d = ifc; while d < h / 2. { z.extend([t + d, t + h - d]); d *= 2.; } }
        z.extend(air.iter().map(|d| total + d));
        z.sort_by(f64::total_cmp); z.dedup_by(|a, b| (*a - *b).abs() < 1e-10); // roundoff twins (h/2 vs 2 x 25 um) otherwise make zero-thickness slabs
        let planes = vec![0; z.len()];
        let metal = |id: &str, r: &[f64; 4], z0: f64, z1: f64| Metal { source_id: id.into(), polygon_m: vec![[m(r[0]), m(r[1])], [m(r[2]), m(r[1])], [m(r[2]), m(r[3])], [m(r[0]), m(r[3])]], holes_m: vec![], z0_m: z0, z1_m: z1 };
        let input = Input {
            meshes: vec![PlaneInput::Cdt { source_id: "grid".into(), xy_m: seeds_m, constraints }], planes, z_m: z.clone(),
            xy_bounds_m: [[m(bx0), m(by0)], [m(bx1), m(by1)]],
            metals: vec![metal("trace", &rects_mm[2], 0., t), metal("return", &rects_mm[3], t + h, total)],
            dielectrics: vec![Dielectric { source_id: "air".into(), z0_m: z[0], z1_m: *z.last().unwrap(), epsilon_r: 1. }],
            exterior: if grounded { Exterior::InfinityBox } else { Exterior::InsulatedAnalyticCoupon },
        };
        let poly = |r: &[f64; 4]| vec![[m(r[0]), m(r[1])], [m(r[2]), m(r[1])], [m(r[2]), m(r[3])], [m(r[0]), m(r[3])]];
        let mut spatial = vec![SpatialDielectric { z0_m: t, z1_m: t + h, epsilon_r: eps_sub, polygon_m: poly(&sub), holes_m: vec![] }];
        if layer_eps != 1. { for (a, b) in [(0., t), (t + h, total)] { spatial.push(SpatialDielectric { z0_m: a, z1_m: b, epsilon_r: layer_eps, polygon_m: poly(&sub), holes_m: vec![] }); } }
        let budget = Budget { max_nodes: 3_000_000, max_local_terms: 50_000_000, max_bytes: 3500 * 1024 * 1024, max_wall: Duration::from_secs(700) };
        let started = std::time::Instant::now();
        match extract_impl(&input, None, &spatial, true, false, None, budget) {
            Ok(s) => {
                let c = s.capacitance_f.clone();
                eprintln!("CAP3D_RAW {}", serde_json::json!({"lengthMm": l_mm, "level": level, "ifcUm": ifc * 1e6, "marginMm": margin_mm, "box": grounded, "nodes": s.plan.nodes, "freeNodes": s.plan.free_nodes, "planes": z.len(), "seconds": started.elapsed().as_secs_f64(), "C_F": c, "planBytes": s.plan.planned_simultaneous_bytes}));
                results.push((l_mm, c));
            }
            Err(e) => { eprintln!("CAP3D_REFUSED lengthMm={l_mm} level={level} error={e:?}"); return; }
        }
    }
    if results.len() >= 2 {
        let (la, ca) = &results[0]; let (lb, cb) = &results[1];
        let dl = (la - lb) * 1e-3;
        let per = |i: usize, j: usize| (ca[i][j] - cb[i][j]) / dl * 1e12;
        let (c11, c12, c22) = (per(0, 0), per(0, 1), per(1, 1));
        eprintln!("CAP3D {}", serde_json::json!({"level": level, "ifcUm": ifc * 1e6, "marginMm": margin_mm, "box": grounded, "eps": eps_sub, "subFull": sub_full,
            "C11_pFperM": c11, "C12_pFperM": c12, "C22_pFperM": c22, "traceToBox_pFperM": c11 + c12, "traceToReturn_pFperM": -c12}));
    }
}
