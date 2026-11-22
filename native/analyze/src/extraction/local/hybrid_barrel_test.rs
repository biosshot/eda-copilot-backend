//! Thin-wall axial barrel prisms: ownership, DC, magnetic and hybrid solve controls.
use super::*;
use geo::Area;

pub(crate) fn production_via_fixture() -> (crate::model::Board, board_mesh::Request) {
    let board=crate::import_json(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
    // Exact geometry captured at the automatic hybrid native boundary.
    let request:board_mesh::Request=serde_json::from_str(r#"{"roiMm":[[-0.54,-1.07],[10.54,0.6900000000000001]],"stack":[{"name":"TOP","copper":true,"thicknessM":1e-05,"conductivitySM":58000000,"epsilonR":null},{"name":"substrate","copper":false,"thicknessM":0.0002,"conductivitySM":null,"epsilonR":4},{"name":"BOTTOM","copper":true,"thicknessM":1e-05,"conductivitySM":58000000,"epsilonR":null}],"finishedHolePlatingM":2.5e-05,"barrelConductivitySM":58000000,"airMarginM":0.002,"maxTriangleAreaM2":1e-06,"maxVerticalStepM":0.002,"curveDeviationMm":0.001,"electrodes":[{"name":"foil-terminal-0","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"foil-terminal-1","sourcePath":"pads[1]","layer":"BOTTOM","upper":true},{"name":"foil-terminal-2","sourcePath":"pads[2]","layer":"BOTTOM","upper":true},{"name":"foil-terminal-3","sourcePath":"pads[3]","layer":"BOTTOM","upper":true}],"ports":[["foil-terminal-0","foil-terminal-2"],["foil-terminal-1","foil-terminal-3"]],"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"minTriangleAngleDeg":0,"airEdgeDivisions":1,"domainPolygonMm":[[-0.54,-1.07],[10.54,-1.07],[10.54,0.6900000000000001],[-0.54,0.6900000000000001]],"cutPlanes":{}}"#).unwrap();
    (board, request)
}

const BUDGET: AllocationBudget = AllocationBudget { max_bytes: 512 * 1024 * 1024, max_nodes: 100_000, max_tetrahedra: 200_000 };

fn hybrid_profile(request: &board_mesh::Request) -> copper_skin_profile::Profile {
    CheckedOperator::HybridPlanarV3
        .skin_profile(1e6, Some(1e6), request.max_vertical_step_m).unwrap().unwrap()
        .with_thickness_screen([58e6]).unwrap()
}

/// Dense DC solve of the mixed network (A R^-1 A^T v = injection) with Dirichlet
/// values; returns potentials and the injection vector.
fn dense_dc(map: &MixedMap<'_>, fixed: &BTreeMap<usize, f64>) -> (Vec<f64>, Vec<f64>) {
    let nv = map.potential_count();
    let mut y = vec![vec![0.; nv]; nv];
    let mut columns = vec![Vec::new(); map.current_count()];
    for &(i, j, a) in &map.incidence { columns[j].push((i, a)); }
    for (j, column) in columns.iter().enumerate() {
        let g = 1. / map.resistance_ohm[j];
        for &(a, x) in column { for &(b, w) in column { y[a][b] += x * w * g; } }
    }
    let mut fixed = fixed.clone();
    for component in &map.potential_components {
        if !component.iter().any(|i| fixed.contains_key(i)) { fixed.insert(component[0], 0.); }
    }
    let free: Vec<usize> = (0..nv).filter(|i| !fixed.contains_key(i)).collect();
    let n = free.len();
    let mut a = vec![vec![0.; n + 1]; n];
    for (r, &i) in free.iter().enumerate() {
        for (c, &j) in free.iter().enumerate() { a[r][c] = y[i][j]; }
        a[r][n] = -fixed.iter().map(|(&j, &v)| y[i][j] * v).sum::<f64>();
    }
    for col in 0..n {
        let pivot = (col..n).max_by(|&p, &q| a[p][col].abs().total_cmp(&a[q][col].abs())).unwrap();
        a.swap(col, pivot);
        let d = a[col][col];
        assert!(d.abs() > 0., "singular DC network at potential {}", free[col]);
        for row in col + 1..n {
            let f = a[row][col] / d;
            if f != 0. { for k in col..=n { let t = a[col][k]; a[row][k] -= f * t; } }
        }
    }
    let mut x = vec![0.; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (a[row][n] - s) / a[row][row];
    }
    let mut v = vec![0.; nv];
    for (&i, &val) in &fixed { v[i] = val; }
    for (r, &i) in free.iter().enumerate() { v[i] = x[r]; }
    let injection = (0..nv).map(|i| (0..nv).map(|j| y[i][j] * v[j]).sum::<f64>()).collect();
    (v, injection)
}

fn mixed_current(map: &MixedMap<'_>, v: &[f64], mode: usize) -> f64 {
    map.incidence.iter().filter(|e| e.1 == mode).map(|e| e.2 * v[e.0]).sum::<f64>() / map.resistance_ohm[mode]
}

fn barrel_span(mesh: &board_mesh::BoardMesh) -> f64 {
    let lo = mesh.layer_spans.values().map(|s| s[0]).fold(f64::INFINITY, f64::min);
    let hi = mesh.layer_spans.values().map(|s| s[1]).fold(f64::NEG_INFINITY, f64::max);
    hi - lo
}

fn assert_thin_ownership(mesh: &board_mesh::BoardMesh, map: &MixedMap<'_>) {
    assert!(!map.barrels.is_empty());
    assert_eq!(map.barrel_fallback_columns, 0);
    let mut owned = vec![0usize; map.owners.len()];
    let mut slivers = 0usize;
    for (i, f) in map.foils.iter().enumerate() {
        for &c in &f.tetrahedra { assert!(matches!(map.owners[c], Owner::Foil(k) if k == i)); owned[c] += 1; }
    }
    for (i, b) in map.barrels.iter().enumerate() {
        for &c in &b.tetrahedra { assert!(matches!(map.owners[c], Owner::Barrel(k) if k == i)); owned[c] += 1; }
    }
    for (i, o) in map.owners.iter().enumerate() {
        owned[i] += usize::from(matches!(o, Owner::Volume | Owner::Dropped));
        assert_eq!(owned[i], 1, "tetrahedron {i} must have exactly one owner");
        let t = &mesh.mesh.copper_tetrahedra[i];
        if t.source_ids.iter().any(|id| mesh.barrel_ids.contains(id)) && !matches!(o, Owner::Barrel(_)) {
            // Only mesh slivers of pad copper that touch the annulus by source id.
            let ids = mesh.planar_triangles[t.planar_triangle];
            let [p, q, r] = ids.map(|n| mesh.mesh.nodes_m[n]);
            let area = 0.5 * ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])).abs();
            assert!(area < 1e-14, "barrel tetrahedron {i} left volumetric (area {area:e})");
            slivers += 1;
        }
    }
    assert_eq!(slivers > 0, map.barrel_sliver_columns > 0);
    let total: f64 = mesh.mesh.copper_tetrahedra.iter().map(|t| t.volume_m3).sum();
    let dropped: f64 = map.owners.iter().enumerate().filter(|(_, o)| matches!(o, Owner::Dropped)).map(|(i, _)| mesh.mesh.copper_tetrahedra[i].volume_m3).sum();
    assert!((dropped - map.dropped_volume_m3).abs() <= 1e-9 * dropped.max(1e-30) && map.dropped_volume_fraction <= 1e-6);
    let owned_volume: f64 = map.foils.iter().map(|f| f.volume_m3).sum::<f64>()
        + map.barrels.iter().map(|b| b.volume_m3).sum::<f64>()
        + dropped
        + map.owners.iter().enumerate().filter(|(_, o)| matches!(o, Owner::Volume)).map(|(i, _)| mesh.mesh.copper_tetrahedra[i].volume_m3).sum::<f64>();
    assert!((total - owned_volume).abs() <= 1e-9 * total, "copper volume changed: {total:e} vs {owned_volume:e}");
}

/// Potentials of every ring group in the first and in the last copper layer.
fn end_layer_potentials(mesh: &board_mesh::BoardMesh, map: &MixedMap<'_>) -> BTreeMap<usize, f64> {
    let top = mesh.layer_spans["TOP"];
    let bottom = mesh.layer_spans["BOTTOM"];
    let mut fixed = BTreeMap::new();
    for prism in &map.barrels {
        for &t in &prism.tetrahedra {
            for &n in &mesh.mesh.copper_tetrahedra[t].nodes {
                let z = mesh.mesh.nodes_m[n][2];
                let id = map.mixed_potential(n).unwrap();
                if z >= top[0] && z <= top[1] { fixed.insert(id, 1.); }
                if z >= bottom[0] && z <= bottom[1] { fixed.insert(id, 0.); }
            }
        }
    }
    fixed
}

#[test]
fn thin_wall_barrel_dc_resistance_matches_shell_formula() {
    let (board, request) = production_via_fixture();
    let wall = Duration::from_secs(60);
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, Some(hybrid_profile(&request))).unwrap();
    let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, BarrelModel::ThinWall).unwrap();
    assert_thin_ownership(&mesh, &map);
    let volume_tets = map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count();
    eprintln!("P7_THIN_BARREL barrels={} prisms={} foils={} remainingTetrahedra={volume_tets} potentials={} currents={}", map.barrel_count, map.barrels.len(), map.foils.len(), map.potential_count(), map.current_count());
    let (v, _) = dense_dc(&map, &end_layer_potentials(&mesh, &map));
    let current: f64 = map.barrels.iter().map(|b| mixed_current(&map, &v, b.mode)).sum();
    let resistance = 1. / current.abs();
    let sigma = request.barrel_conductivity_s_m.unwrap();
    let area: f64 = map.barrels.iter().map(|b| b.area_m2).sum();
    let expected = barrel_span(&mesh) / (sigma * area);
    let polygon_area = mesh.field_copper.iter().filter(|c| c.2).map(|c| c.0.unsigned_area() * 1e-6).sum::<f64>();
    eprintln!("P7_THIN_BARREL_DC R={resistance:e} expected={expected:e} relative={:e} polygonAreaRelative={:e}", (resistance - expected).abs() / expected, (area - polygon_area).abs() / polygon_area);
    assert!((resistance - expected).abs() <= 1e-2 * expected, "R={resistance:e} expected={expected:e}");
    assert!((area - polygon_area).abs() <= 1e-6 * polygon_area);
    for b in &map.barrels {
        let r = map.resistance_ohm[b.mode];
        let e = (b.z_span_m[1] - b.z_span_m[0]) / (sigma * b.area_m2);
        assert!((r - e).abs() <= 1e-9 * e);
    }
}

#[test]
fn thin_wall_barrel_agrees_with_volume_barrel_dc_through_the_whole_path() {
    let (board, request) = production_via_fixture();
    let wall = Duration::from_secs(60);
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, Some(hybrid_profile(&request))).unwrap();
    let mut results = Vec::new();
    for model in [BarrelModel::Volume, BarrelModel::ThinWall] {
        let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, model).unwrap();
        let fixed = BTreeMap::from([(map.electrodes[0], 1.), (map.electrodes[1], 0.)]);
        let (v, injection) = dense_dc(&map, &fixed);
        let r = 1. / injection[map.electrodes[0]];
        eprintln!("P7_BARREL_DC model={} R={r:e} volumeTetrahedra={}", model.name(), map.owners.iter().filter(|o| matches!(o, Owner::Volume)).count());
        assert!(v.iter().all(|x| x.is_finite()));
        results.push(r);
    }
    let change = (results[0] - results[1]).abs() / results[0];
    eprintln!("P7_BARREL_DC thin-vs-volume relative={change:e}");
    assert!(change < 0.05, "thin wall changed the pad-to-pad DC resistance by {change:e}");
}

/// Reference partial inductance of a thin circular shell with uniform axial
/// current, by quadrature of the azimuthally averaged Neumann kernel
/// (complete elliptic integral through the AGM). Independent of the GMD formula.
fn thin_circular_shell_inductance(r: f64, h: f64) -> f64 {
    let kernel = |u: f64| {
        let a = (u * u + 4. * r * r).sqrt();
        let (mut x, mut y) = (1., u / a);
        for _ in 0..40 { let n = 0.5 * (x + y); y = (x * y).sqrt(); x = n; }
        1. / (a * x)
    };
    // u = h t^4 softens the logarithmic singularity at u = 0; composite Simpson.
    let n = 20_000;
    let f = |t: f64| { let u = h * t.powi(4); 2. * (h - u) * kernel(u.max(1e-30)) * 4. * h * t.powi(3) };
    let dt = 1. / n as f64;
    let mut sum = f(0.) + f(1.);
    for i in 1..n { sum += f(i as f64 * dt) * if i % 2 == 1 { 4. } else { 2. }; }
    1e-7 * sum * dt / 3.
}

fn barrel_self_inductance(plating_m: f64, substrate_m: Option<f64>) -> (f64, f64, f64, f64) {
    let (board, mut request) = production_via_fixture();
    request.finished_hole_plating_m = Some(plating_m);
    if let Some(t) = substrate_m { for layer in &mut request.stack { if !layer.copper { layer.thickness_m = t; } } }
    let wall = Duration::from_secs(120);
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, Some(hybrid_profile(&request))).unwrap();
    let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, BarrelModel::ThinWall).unwrap();
    let supports = map.barrel_current_supports().unwrap();
    let total: f64 = map.barrels.iter().map(|b| b.area_m2).sum();
    let started = Instant::now();
    // One ampere distributed uniformly: mode amplitude A_p / A (the mode density is 1 / A_p).
    let j = |s: usize| supports[s].current_density[0].1[2] * map.barrels[s].area_m2 / total;
    let mut l = 0.;
    for a in 0..supports.len() {
        for b in 0..=a {
            let (y, _, _) = super::super::planar_operator::pair_integral_compact(&supports[a].prism, &supports[b].prism, 8, started, wall).unwrap();
            let value = 1e-7 * y * j(a) * j(b);
            l += if a == b { value } else { 2. * value };
        }
    }
    let h = barrel_span(&mesh);
    let barrel = mesh.field_copper.iter().find(|c| c.2).unwrap();
    let ring_area = |ring: &geo::LineString<f64>| geo::Polygon::new(ring.clone(), vec![]).unsigned_area() * 1e-6;
    let polygon = &barrel.0 .0[0];
    let r_out = (ring_area(polygon.exterior()) / std::f64::consts::PI).sqrt();
    let r_in = (ring_area(&polygon.interiors()[0]) / std::f64::consts::PI).sqrt();
    let r = 0.5 * (r_in + r_out);
    let analytic = 2e-7 * h * ((h / r).asinh() - (1. + (r / h).powi(2)).sqrt() + r / h);
    let error = (l - analytic).abs() / analytic;
    let numeric = thin_circular_shell_inductance(r, h);
    eprintln!("P7_THIN_BARREL_L plating={plating_m:e} h={h:e} L={l:e} gmdFormula={analytic:e} formulaRelative={error:e} shellQuadrature={numeric:e} quadratureRelative={:e} prisms={} rIn={r_in:e} rOut={r_out:e} seconds={:.3}", (l - numeric).abs() / numeric, supports.len(), started.elapsed().as_secs_f64());
    (l, analytic, error, numeric)
}

#[test]
fn thin_wall_barrel_self_inductance_matches_shell_partial_inductance() {
    // Board-thickness barrel (h / r = 16): the GMD closed form applies.
    let (l, analytic, error, numeric) = barrel_self_inductance(2.5e-5, Some(1.6e-3));
    assert!(error <= 0.03, "L={l:e} analytic={analytic:e}");
    assert!((l - numeric).abs() <= 0.03 * numeric);
    // Squat via (h / r = 2.2) against the independent thin-shell quadrature; the
    // closed form with GMD = r is not accurate there.
    let (l, _, _, numeric) = barrel_self_inductance(5e-6, None);
    assert!((l - numeric).abs() <= 0.03 * numeric, "L={l:e} quadrature={numeric:e}");
}

fn hybrid_via_solve(drill_mm: Option<f64>) {
    let (mut board, request) = production_via_fixture();
    if let Some(drill) = drill_mm {
        let mut board_json: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).unwrap();
        board_json["copper"]["fixed"]["vias"][0]["drillMm"] = serde_json::json!(drill);
        board = crate::import_json(&board_json.to_string()).board.unwrap();
    }
    let wall = Duration::from_secs(std::env::var("P7_BARREL_TEST_WALL_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(120));
    let operator = CheckedOperator::HybridPlanarV3;
    let profile = Some(hybrid_profile(&request));
    let started = Instant::now();
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, profile).unwrap();
    let mesh_seconds = started.elapsed().as_secs_f64();
    let grid = CheckedGrid { frequencies_hz: &[1e6], reference_ohm: &[50., 50.], max_iterations: 16 };
    let result = solve_checked_mesh_impl_policy(&mesh, &request, grid, profile, &[], None, PartitionPolicy::AutomaticHybridV1, operator, BUDGET, started, wall).unwrap();
    let seconds = started.elapsed().as_secs_f64();
    let compact = &result["provenance"]["compactOperator"];
    eprintln!("P7_THIN_BARREL_HYBRID meshSeconds={mesh_seconds:.3} totalSeconds={seconds:.3} evidence={} skin={}", compact["barrelEvidence"], compact["barrelSkinApproximation"]);
    assert_eq!(compact["barrelModel"], serde_json::json!("thin-wall-axial-prism-v1"));
    assert!(compact["barrelEvidence"]["barrelPrisms"].as_u64().unwrap() > 0);
    assert_eq!(compact["barrelEvidence"]["barrels"].as_u64().unwrap(), 1);
    assert_eq!(compact["barrelSkinApproximation"]["approximation"], serde_json::json!(true));
    let columns = result["samples"][0]["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 2);
    for (input, column) in columns.iter().enumerate() {
        assert!(column["kclRelative"].as_f64().unwrap() <= 1e-8);
        assert!(column["kvlRelative"].as_f64().unwrap() <= 1e-8);
        let copper = column["copperLossW"].as_f64().unwrap();
        assert!(copper > 0.);
        let mut wave_power = 12.5;
        for output in 0..2 {
            let s = &result["parameters"][0][output][input];
            let (r, i) = (s["real"].as_f64().unwrap(), s["imaginary"].as_f64().unwrap());
            wave_power -= 12.5 * (r * r + i * i);
        }
        assert!((wave_power - copper).abs() <= 1e-12 + 1e-8 * (wave_power.abs() + copper.abs()), "power balance");
    }
    assert!(seconds < wall.as_secs_f64());
}

/// Full hybrid assembly of the production via. Ignored by default: the 2 + 2
/// zero-width (27 pm) pad slivers of this exact mesh fail the tetrahedron source
/// quadrature (`LOCAL_POLYHEDRON_SOURCE_QUADRATURE`) after ~2 minutes, and the
/// remaining ~420 pad-fan tetrahedra (not barrel copper) cost minutes.
#[test]
#[ignore = "PLANAR_UNRESOLVED: shared-edge Duffy converges algebraically on adjacent 100:1 pad prisms (4.5 mm x 48/100 um); last change at order 64 is 6.8e-4 > 1e-4 noise bound"]
fn hybrid_via_solves_with_thin_wall_barrel_in_seconds() {
    hybrid_via_solve(None);
}

/// Same via with a drill whose mesh has no zero-width pad slivers (~30 s release).
#[test]
fn hybrid_via_without_mesh_slivers_solves_with_thin_wall_barrel() {
    hybrid_via_solve(Some(0.25));
}

/// Diagnostic only (no convergence assertion): compact adaptive foil solve of the
/// sliver-free via at 100 kHz and 1 MHz; the support count is far below 512, so the
/// magnetic action is exact (no H-matrix / fast local table).
#[test]
#[ignore = "diagnostic: prints per-iteration KVL/KCL/uncertainty at 100 kHz and 1 MHz"]
fn diag_compact_kvl_100k_vs_1m() {
    std::env::set_var("P7_MAGNETIC_PHASE_PROFILE", "1");
    let (_, mut request) = production_via_fixture();
    // DIAG_AREA_SCALE=k divides the triangle area target so the support count can cross the 512 H-matrix gate.
    if let Ok(k) = std::env::var("DIAG_AREA_SCALE") { request.max_triangle_area_m2 /= k.parse::<f64>().unwrap(); }
    // DIAG_THICKNESS_M=t overrides every copper layer thickness (exercises P7_FOIL_SHEET at 18-35 um).
    if let Ok(t) = std::env::var("DIAG_THICKNESS_M") { for l in request.stack.iter_mut().filter(|l| l.copper) { l.thickness_m = t.parse().unwrap(); } }
    let mut board_json: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).unwrap();
    board_json["copper"]["fixed"]["vias"][0]["drillMm"] = serde_json::json!(0.25);
    let board = crate::import_json(&board_json.to_string()).board.unwrap();
    let wall = Duration::from_secs(120);
    for hz in [1e5, 1e6] {
        let profile = Some(CheckedOperator::HybridPlanarV3.skin_profile(hz, Some(hz), request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap());
        let started = Instant::now();
        let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, profile).unwrap();
        let grid = CheckedGrid { frequencies_hz: &[hz], reference_ohm: &[50., 50.], max_iterations: 16 };
        let result = solve_checked_mesh_impl_policy(&mesh, &request, grid, profile, &[], None, PartitionPolicy::AutomaticHybridV1, CheckedOperator::HybridPlanarV3, BUDGET, started, wall);
        match result {
            Ok(r) => {
                let c = &r["provenance"]["compactOperator"];
                eprintln!("DIAG_COMPACT hz={hz:e} OK seconds={:.1} foilSupports={} foilSheet={} localTetra={} currentModes={} quality={} history={}", started.elapsed().as_secs_f64(), c["foilSupports"], c["foilSheet"], c["localTetraSupports"], r["provenance"]["currentModes"], r["quality"], c["enrichment"]);
            }
            Err(e) => eprintln!("DIAG_COMPACT hz={hz:e} REFUSED seconds={:.1} error={e:?}", started.elapsed().as_secs_f64()),
        }
    }
}

#[test]
fn plated_slot_barrel_is_owned_by_thin_wall_prisms() {
    let (_, request) = production_via_fixture();
    let mut board_json: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).unwrap();
    board_json["copper"]["fixed"]["vias"] = serde_json::json!([]);
    board_json["pads"].as_array_mut().unwrap().push(serde_json::json!({"id":"slot","component":"J0","number":"3","net":"SIGNAL","at":{"x":5,"y":0},"rotationDeg":0,"layers":["TOP","BOTTOM"],"shape":{"kind":"rect","widthMm":1.0,"heightMm":0.6},"hole":{"shape":"slot","diameterMm":0.3,"slotLengthMm":0.8,"plated":true}}));
    let board = crate::import_json(&board_json.to_string()).board.unwrap();
    let wall = Duration::from_secs(60);
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, Some(hybrid_profile(&request))).unwrap();
    assert_eq!(mesh.barrel_ids.len(), 1);
    let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, BarrelModel::ThinWall).unwrap();
    assert_thin_ownership(&mesh, &map);
    let (v, _) = dense_dc(&map, &end_layer_potentials(&mesh, &map));
    let current: f64 = map.barrels.iter().map(|b| mixed_current(&map, &v, b.mode)).sum();
    let area: f64 = map.barrels.iter().map(|b| b.area_m2).sum();
    let expected = barrel_span(&mesh) / (request.barrel_conductivity_s_m.unwrap() * area);
    eprintln!("P7_THIN_SLOT prisms={} R={:e} expected={expected:e}", map.barrels.len(), 1. / current.abs());
    assert!(((1. / current.abs()) - expected).abs() <= 1e-2 * expected);
    // Pad-to-pad DC through the slot against the volumetric barrel.
    let mut results = Vec::new();
    for model in [BarrelModel::Volume, BarrelModel::ThinWall] {
        let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, model).unwrap();
        let fixed = BTreeMap::from([(map.electrodes[0], 1.), (map.electrodes[1], 0.)]);
        let (_, injection) = dense_dc(&map, &fixed);
        results.push(1. / injection[map.electrodes[0]]);
    }
    let change = (results[0] - results[1]).abs() / results[0];
    eprintln!("P7_THIN_SLOT_DC volume={:e} thin={:e} relative={change:e}", results[0], results[1]);
    assert!(change < 0.05);
}

#[test]
fn thin_wall_barrel_splits_at_interior_copper_layers() {
    let (_, mut request) = production_via_fixture();
    let mut board_json: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).unwrap();
    board_json["layers"] = serde_json::json!([{"name":"TOP","index":0,"side":"top"},{"name":"INNER1","index":1,"side":"inner"},{"name":"INNER2","index":2,"side":"inner"},{"name":"BOTTOM","index":3,"side":"bottom"}]);
    let board = crate::import_json(&board_json.to_string()).board.unwrap();
    // Two inner copper layers without drawn copper: the barrel crosses them.
    let copper = |name: &str| serde_json::from_value(serde_json::json!({"name":name,"copper":true,"thicknessM":1e-5,"conductivitySM":58000000,"epsilonR":null})).unwrap();
    let dielectric = |name: &str| serde_json::from_value(serde_json::json!({"name":name,"copper":false,"thicknessM":0.0002,"conductivitySM":null,"epsilonR":4})).unwrap();
    let top = request.stack[0].clone();
    let bottom = request.stack[2].clone();
    request.stack = vec![top, dielectric("s1"), copper("INNER1"), dielectric("s2"), copper("INNER2"), dielectric("s3"), bottom];
    let wall = Duration::from_secs(60);
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, Some(hybrid_profile(&request))).unwrap();
    let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, BarrelModel::ThinWall).unwrap();
    assert_thin_ownership(&mesh, &map);
    let triangles: std::collections::BTreeSet<_> = map.barrels.iter().map(|b| b.planar_triangle).collect();
    assert_eq!(map.barrels.len(), 3 * triangles.len(), "three segments per annulus triangle");
    let span = barrel_span(&mesh);
    for t in &triangles {
        let mut z: Vec<_> = map.barrels.iter().filter(|b| b.planar_triangle == *t).map(|b| b.z_span_m).collect();
        z.sort_by(|a, b| a[0].total_cmp(&b[0]));
        assert!(z.windows(2).all(|w| w[0][1] == w[1][0]));
        assert!((z[2][1] - z[0][0] - span).abs() <= 1e-12 * span);
        // Interior segments start at the lower face of an interior copper layer.
        assert_eq!(z[1][0], mesh.layer_spans["INNER1"][0]);
        assert_eq!(z[2][0], mesh.layer_spans["INNER2"][0]);
    }
    let top = mesh.layer_spans["TOP"]; let bottom = mesh.layer_spans["BOTTOM"];
    let mut fixed = BTreeMap::new();
    for prism in &map.barrels { for &t in &prism.tetrahedra { for &n in &mesh.mesh.copper_tetrahedra[t].nodes {
        let z = mesh.mesh.nodes_m[n][2]; let id = map.mixed_potential(n).unwrap();
        if z >= top[0] && z <= top[1] { fixed.insert(id, 1.); }
        if z >= bottom[0] && z <= bottom[1] { fixed.insert(id, 0.); }
    }}}
    let (v, _) = dense_dc(&map, &fixed);
    // Series continuity: the three segment levels carry the same total current.
    let mut level = BTreeMap::<u64, f64>::new();
    for b in &map.barrels { *level.entry(b.z_span_m[0].to_bits()).or_default() += mixed_current(&map, &v, b.mode); }
    assert_eq!(level.len(), 3);
    let total = *level.values().next().unwrap();
    for c in level.values() { assert!((c - total).abs() <= 1e-9 * total.abs(), "{level:?}"); }
    let sigma = request.barrel_conductivity_s_m.unwrap();
    let area: f64 = triangles.iter().map(|t| map.barrels.iter().find(|b| b.planar_triangle == *t).unwrap().area_m2).sum();
    let expected = span / (sigma * area);
    eprintln!("P7_THIN_BARREL_4L prisms={} R={:e} expected={expected:e}", map.barrels.len(), 1. / total.abs());
    assert!(((1. / total.abs()) - expected).abs() <= 1e-2 * expected);
}

/// Merged barrel potentials must still match the unchanged electric field mesh.
#[test]
fn thin_wall_barrel_potentials_match_the_electric_field_trace() {
    use super::super::layered_capacitance::{prepare_certified_action, prepare_traced_spatial_action, Budget};
    let (board, request) = production_via_fixture();
    let wall = Duration::from_secs(60);
    let mesh = board_mesh::build_profiled_hybrid(&board, &request, BUDGET, wall, Some(hybrid_profile(&request))).unwrap();
    let map = prepare_model(&mesh, &request.electrodes, 1e6, BUDGET, wall, BarrelModel::ThinWall).unwrap();
    assert!(!map.barrels.is_empty());
    let budget = Budget { max_nodes: 100_000, max_local_terms: 2_400_000, max_bytes: 512 * 1024 * 1024, max_wall: wall };
    let (input, regions, original) = super::super::planar_trace::input_from_board(&mesh, &request, budget).unwrap();
    let trace = super::super::planar_trace::trace(&input, &map, budget).unwrap();
    let referenced: std::collections::BTreeSet<_> = trace.nodes.iter().flat_map(|n| n.weights.iter().map(|&(i, _)| i)).collect();
    assert_eq!(referenced.len(), map.potential_count(), "every mixed mode must be referenced by a matched physical metal node");
    let action = prepare_traced_spatial_action(&input, &trace, &regions, budget).unwrap();
    let dielectric_nodes: std::collections::BTreeSet<_> = mesh.mesh.tetrahedra.iter().flat_map(|t| t.nodes).collect();
    let mut interior = vec![true; map.potential_count()];
    for row in map.electric_node_trace() { if dielectric_nodes.contains(&row.node) { for (i, _) in row.weights { interior[i] = false; } } }
    if let Some(mode) = interior.iter().position(|&v| v) {
        let mut excitation = vec![0.; map.potential_count()];
        excitation[mode] = 1.;
        assert!(action.apply(&excitation).unwrap().iter().all(|&q| q == 0.), "interior-only mode must have exactly zero field charge");
    }
    drop(action);
    let (certified_trace, certificate) = super::super::planar_trace::certified_trace(&input, &original, &map, budget).unwrap();
    prepare_certified_action(&input, &certified_trace, &regions, &certificate, budget).unwrap();
}
