#[path = "support/thermal_fixture.rs"]
mod captured_input;
use captured_input::captured_board_request;
use pcb_analyze::thermal::{run, Request};
use serde_json::{json, Value};
fn points(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([{"x":x0,"y":y0},{"x":x1,"y":y0},{"x":x1,"y":y1},{"x":x0,"y":y1}])
}
fn board() -> Value {
    json!({"outline":points(0.,0.,10.,10.),"cutouts":[],"layers":[{"name":"TOP","index":0,"side":"top"},{"name":"BOTTOM","index":1,"side":"bottom"}],"nets":[{"name":"A"}],"components":[],"pads":[],"holes":[],"keepouts":[],"rules":{"default":{"clearanceMm":0.2,"edgeClearanceMm":0.3,"minTrackWidthMm":0.1,"preferredTrackWidthMm":0.2,"via":{"minDiameterMm":0.5,"preferredDiameterMm":0.6,"minDrillMm":0.2,"preferredDrillMm":0.3}},"nets":[]},"copper":{"fixed":{"tracks":[],"vias":[],"zones":[]},"editable":{"tracks":[],"vias":[],"zones":[]}}})
}
fn material(k: f64) -> Value {
    json!({"thermalConductivityWPerMK":k,"densityKgM3":1000.,"specificHeatJPerKgK":1000.,"source":"analytic coupon inputs","state":"constant isotropic"})
}
fn region(layer: &str) -> Value {
    json!({"kind":"region","layers":[layer],"polygon":{"outer":points(0.,0.,10.,10.)}})
}
fn request(edge: f64) -> Value {
    json!({"contextId":"spatial-reference","mode":"steady","ambientC":25.,"network":{"nodes":[{"id":"external","boundary":"environment"}],"links":[],"sources":[]},"board":{"boardJson":board().to_string(),"boardIdentity":"analytic-board","layers":[{"kind":"copper","name":"TOP","thicknessMm":0.1,"material":material(1.),"background":material(1.)},{"kind":"dielectric","name":"core","thicknessMm":1.,"material":material(2.)},{"kind":"copper","name":"BOTTOM","thicknessMm":0.2,"material":material(3.),"background":material(3.)}],"maxEdgeMm":edge,"maxNodes":50000,"snapshot":false,"heat":[{"id":"input","target":region("TOP"),"node":"board","powerW":1.}],"contacts":[{"id":"sink","node":"external","target":region("BOTTOM"),"resistanceKPerW":1.}]}})
}
fn solve(r: Value) -> Value {
    run(serde_json::from_value::<Request>(r).unwrap())
}
fn complete(r: &Value) {
    assert_eq!(r["status"], "complete", "{r}");
}
fn layer_mean(r: &Value, name: &str) -> f64 {
    let fields = r["spatial"]["mesh"]["fieldNodes"].as_array().unwrap();
    let mut a = 0.;
    let mut sum = 0.;
    for f in fields.iter().filter(|f| f["layer"] == name) {
        let w = f["supportAreaMm2"].as_f64().unwrap();
        sum += r["temperaturesC"][f["id"].as_str().unwrap()][0]
            .as_f64()
            .unwrap()
            * w;
        a += w;
    }
    sum / a
}
#[test]
fn homogeneous_slab_and_multilayer_harmonic_series() {
    // Uniform flux: bottom Robin R=1 K/W plus between-slice centers:
    // .0001/(2*1*A)+.001/(2*A)+.0002/(2*3*A), A=.0001 m².
    let expected = 25. + 1. + 0.5 + 5. + 1. / 3.;
    for edge in [5., 2.5, 1.25] {
        let r = solve(request(edge));
        complete(&r);
        assert!(
            (layer_mean(&r, "TOP") - expected).abs() < 1e-7,
            "{} vs {expected}",
            layer_mean(&r, "TOP")
        );
        assert!((layer_mean(&r, "BOTTOM") - 26.).abs() < 1e-8);
        assert!(r["quality"]["normalizedEnergyBalance"].as_f64().unwrap() < 1e-8);
        assert!(r["quality"]["linearResidual"].as_f64().unwrap() < 1e-8);
    }
}
#[test]
fn spatial_source_conservation_and_time_capacity() {
    let mut r = request(2.5);
    r["mode"] = json!("time");
    r["initialC"] = json!("ambient");
    r["time"] = json!({"stopS":1.,"sampleStepS":0.2});
    let r = solve(r);
    complete(&r);
    let sources = r["spatial"]["sources"][0]["weights"].as_array().unwrap();
    let sum: f64 = sources.iter().map(|s| s["weight"].as_f64().unwrap()).sum();
    assert!((sum - 1.).abs() < 1e-12);
    assert!(r["quality"]["normalizedEnergyBalance"].as_f64().unwrap() < 1e-6);
    for p in r["inputPowerW"].as_array().unwrap() {
        assert!((p.as_f64().unwrap() - 1.).abs() < 1e-12);
    }
}
#[test]
fn missing_material_contact_source_and_geometry_reject() {
    let mut r = request(5.);
    r["board"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("background");
    assert_eq!(solve(r)["status"], "rejected");
    let mut r = request(5.);
    r["board"]["contacts"] = json!([]);
    assert_eq!(solve(r)["diagnostics"][0]["code"], "THERMAL_BOUNDARY");
    let mut r = request(5.);
    r["board"]["heat"][0]["target"] = json!({"kind":"padId","id":"foreign"});
    assert_eq!(solve(r)["status"], "rejected");
    let mut r = request(5.);
    let mut b = board();
    b["cutouts"] = json!([points(4., 0., 6., 10.)]);
    r["board"]["boardJson"] = json!(b.to_string());
    assert_eq!(solve(r)["status"], "rejected");
}

#[test]
fn independent_fourier_spreading_and_spatial_refinement() {
    // Independent continuum thin-sheet cosine expansion: insulated sides and
    // Robin H=100 W/m2/K. Uniform 2x2 mm source, same area-average observation.
    let length = 0.01;
    let width = 0.002;
    let sheet = 0.0003;
    let h = 100.;
    let mut reference = 0.;
    for m in 0..250 {
        for n in 0..250 {
            let f = |i: usize| {
                if i == 0 {
                    1.
                } else {
                    let a = i as f64 * std::f64::consts::PI;
                    let s = a * width / (2. * length);
                    2. * (a / 2.).cos().powi(2) * (s.sin() / s).powi(2)
                }
            };
            reference += f(m) * f(n)
                / (length
                    * length
                    * (h + sheet * std::f64::consts::PI.powi(2) * (m * m + n * n) as f64
                        / (length * length)));
        }
    }
    let mut errors = vec![];
    for edge in [1.25, 0.625, 0.3125, 0.15625] {
        let mut r = request(edge);
        r["board"]["contacts"] = json!([]);
        r["board"]["surfaceCooling"] = json!({"resistanceKPerW":100.});
        for i in 0..3 {
            r["board"]["layers"][i]["thicknessMm"] = json!(0.01);
            r["board"]["layers"][i]["material"] = material(10.);
            if i != 1 {
                r["board"]["layers"][i]["background"] = material(10.);
            }
        }
        r["board"]["heat"][0]["target"] =
            json!({"kind":"region","layers":["TOP"],"polygon":{"outer":points(4.,4.,6.,6.)}});
        let result = solve(r);
        complete(&result);
        let observed: f64 = result["spatial"]["sources"][0]["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                w["weight"].as_f64().unwrap()
                    * (result["temperaturesC"][w["node"].as_str().unwrap()][0]
                        .as_f64()
                        .unwrap()
                        - 25.)
            })
            .sum();
        errors.push((observed - reference).abs() / reference);
    }
    assert!(
        errors[3] < 0.02,
        "Fourier reference {reference}, spatial errors {errors:?}"
    );
    assert!(errors.windows(2).all(|w| w[1] < w[0]), "{errors:?}");
}

// Independent cell-centered finite-volume REFERENCE only (not a production
// engine): exact piecewise strip face resistances and a separately assembled
// layered operator. No thermal solver code is reused.
fn fd_spreading(n: usize, center: f64) -> f64 {
    let length = 0.002;
    let dx = length / n as f64;
    let strip = [center * 1e-3 - 0.00004, center * 1e-3 + 0.00004];
    let mut breaks = vec![0., strip[0], strip[1], 0.00085, 0.00115, length];
    breaks.sort_by(f64::total_cmp);
    breaks.dedup();
    let mut y_edges = vec![0.];
    for interval in breaks.windows(2) {
        let pieces = ((interval[1] - interval[0]) / dx).ceil() as usize;
        for j in 1..=pieces {
            y_edges.push(interval[0] + (interval[1] - interval[0]) * j as f64 / pieces as f64);
        }
    }
    let ny = y_edges.len() - 1;
    let cells = n * ny;
    let count = 3 * cells;
    let thick = [0.00002, 0.0001, 0.00002];
    let copper = 400.;
    let resin = 0.3;
    let overlap = |a: f64, b: f64, c: f64, d: f64| (b.min(d) - a.max(c)).max(0.);
    let mut edge: Vec<(usize, usize, f64)> = vec![];
    let mut diagonal = vec![0.; count];
    let mut boundary = vec![0.; count];
    let mut source = vec![0.; count];
    for layer in 0..3 {
        for y in 0..ny {
            for x in 0..n {
                let i = layer * cells + y * n + x;
                let dy = y_edges[y + 1] - y_edges[y];
                let wi = if layer == 0 {
                    overlap(y_edges[y], y_edges[y + 1], strip[0], strip[1])
                } else {
                    0.
                };
                if x + 1 < n {
                    let g = thick[layer] * (wi * copper + (dy - wi) * resin) / dx;
                    edge.push((i, i + 1, g));
                }
                if y + 1 < ny {
                    let a = (y_edges[y] + y_edges[y + 1]) / 2.;
                    let b = (y_edges[y + 1] + y_edges[y + 2]) / 2.;
                    let inside = if layer == 0 {
                        overlap(a, b, strip[0], strip[1])
                    } else {
                        0.
                    };
                    let g = thick[layer] * dx / (inside / copper + (b - a - inside) / resin);
                    edge.push((i, i + n, g));
                }
                if layer + 1 < 3 {
                    let g = dx
                        * (wi / (thick[layer] / (2. * copper) + thick[layer + 1] / (2. * resin))
                            + (dy - wi)
                                / (thick[layer] / (2. * resin) + thick[layer + 1] / (2. * resin)));
                    edge.push((i, i + cells, g));
                }
                if layer == 0 || layer == 2 {
                    boundary[i] = dx * dy / (200. * 2. * length * length);
                }
                if layer == 0 {
                    source[i] = overlap(x as f64 * dx, (x + 1) as f64 * dx, 0.0004, 0.0008)
                        * overlap(y_edges[y], y_edges[y + 1], 0.00085, 0.00115)
                        / (0.0004 * 0.0003);
                }
            }
        }
    }
    for (i, b) in boundary.iter().enumerate() {
        diagonal[i] += b;
    }
    for (a, b, g) in &edge {
        diagonal[*a] += g;
        diagonal[*b] += g;
    }
    let apply = |v: &[f64]| {
        let mut out: Vec<f64> = v.iter().zip(&boundary).map(|(v, b)| v * b).collect();
        for (a, b, g) in &edge {
            let q = g * (v[*a] - v[*b]);
            out[*a] += q;
            out[*b] -= q;
        }
        out
    };
    let mut t = vec![0.; count];
    let mut residual = source.clone();
    let mut z: Vec<f64> = residual.iter().zip(&diagonal).map(|(r, d)| r / d).collect();
    let mut direction = z.clone();
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    let mut rz = dot(&residual, &z);
    for _ in 0..20000 {
        if dot(&residual, &residual).sqrt() < 1e-11 {
            break;
        }
        let ad = apply(&direction);
        let alpha = rz / dot(&direction, &ad);
        for i in 0..count {
            t[i] += alpha * direction[i];
            residual[i] -= alpha * ad[i];
            z[i] = residual[i] / diagonal[i];
        }
        let next = dot(&residual, &z);
        let beta = next / rz;
        for i in 0..count {
            direction[i] = z[i] + beta * direction[i];
        }
        rz = next;
    }
    let actual = apply(&t);
    let residual = actual
        .iter()
        .zip(&source)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(
        residual < 1e-8,
        "Independent FD reference residual {residual}"
    );
    dot(&t, &source)
}
#[test]
fn high_contrast_narrow_copper_spreading_and_mesh_alignment() {
    for center in [1., 1.0125] {
        let reference = fd_spreading(256, center);
        let coarse_reference = fd_spreading(192, center);
        assert!(
            (reference - coarse_reference).abs() / reference < 0.005,
            "Independent reference not refined: {reference}, {coarse_reference}"
        );
        let mut values = vec![];
        for edge in [0.0625, 0.03125, 0.02] {
            let mut r = request(edge);
            let mut b = board();
            b["outline"] = points(0., 0., 2., 2.);
            b["pads"] = json!([{"id":"narrow-copper","number":"1","net":"A","at":{"x":1.,"y":center},"rotationDeg":0.,"layers":["TOP"],"shape":{"kind":"rect","widthMm":2.,"heightMm":0.08}}]);
            r["board"]["boardJson"] = json!(b.to_string());
            r["board"]["contacts"] = json!([]);
            r["board"]["surfaceCooling"] = json!({"resistanceKPerW":200.});
            for i in 0..3 {
                r["board"]["layers"][i]["thicknessMm"] = json!(if i == 1 { 0.1 } else { 0.02 });
                r["board"]["layers"][i]["material"] = material(if i == 1 { 0.3 } else { 400. });
                if i != 1 {
                    r["board"]["layers"][i]["background"] = material(0.3);
                }
            }
            r["board"]["heat"][0]["target"] = json!({"kind":"region","layers":["TOP"],"polygon":{"outer":points(0.4,0.85,0.8,1.15)}});
            let solved = solve(r);
            complete(&solved);
            let observed: f64 = solved["spatial"]["sources"][0]["weights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| {
                    w["weight"].as_f64().unwrap()
                        * (solved["temperaturesC"][w["node"].as_str().unwrap()][0]
                            .as_f64()
                            .unwrap()
                            - 25.)
                })
                .sum();
            values.push(observed);
        }
        let error = (values[2] - reference).abs() / reference;
        let change = (values[2] - values[1]).abs() / values[2];
        eprintln!("high-contrast center={center}, independent FD={coarse_reference}/{reference}, FEM={values:?}, error={error}, change={change}");
        assert!(
            error < 0.02 && change < 0.02,
            "{values:?}, reference {reference}"
        );
    }
}

#[test]
fn finite_via_barrel_conductance_mass_and_loss_transfer() {
    let mut r = request(2.5);
    let mut b = board();
    b["copper"]["fixed"]["vias"] = json!([{"id":"hotvia","net":"A","at":{"x":5.,"y":5.},"diameterMm":0.6,"drillMm":0.3,"fromLayer":"TOP","toLayer":"BOTTOM","type":"through"}]);
    r["board"]["boardJson"] = json!(b.to_string());
    r["board"]["barrel"] =
        json!({"thicknessMm":0.025,"diameterConvention":"finished_hole","material":material(400.)});
    r["board"]["barrelHeat"] = json!([{"id":"loss-map:barrel","barrel":"via:hotvia","fromLayer":"TOP","toLayer":"BOTTOM","powerW":0.4}]);
    let solved = solve(r.clone());
    complete(&solved);
    let barrel = &solved["spatial"]["barrels"][0];
    let area = std::f64::consts::PI * (0.175f64.powi(2) - 0.15f64.powi(2)) * 1e-6;
    let length = 0.00115;
    let g = 400. * area / length;
    assert!((barrel["conductanceWPerK"].as_f64().unwrap() - g).abs() < 1e-12);
    assert!((barrel["lengthM"].as_f64().unwrap() - length).abs() < 1e-15);
    assert_eq!(
        solved["spatial"]["barrelSources"].as_array().unwrap().len(),
        1
    );
    assert!((solved["inputPowerW"][0].as_f64().unwrap() - 1.4).abs() < 1e-12);
    assert!(
        solved["quality"]["normalizedEnergyBalance"]
            .as_f64()
            .unwrap()
            < 1e-6
    );
    let capacity: f64 = solved["spatial"]["mesh"]["fieldNodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["capacityJPerK"].as_f64().unwrap())
        .sum();
    let analytic_capacity = (100. - std::f64::consts::PI * 0.15f64.powi(2)) * 1.3 * 0.001;
    assert!(
        (capacity - analytic_capacity).abs() / analytic_capacity < 1e-5,
        "{capacity} vs {analytic_capacity}"
    );
    // Independent limiting resistance: nearly isothermal outer sheets, almost
    // insulating core. The only appreciable cross-board path is finite kA/L;
    // the bottom finite contact adds its authored 1 K/W in series.
    let mut limiting = r.clone();
    limiting["board"]["barrelHeat"] = json!([]);
    for li in [0usize, 2] {
        limiting["board"]["layers"][li]["material"] = material(1e5);
        limiting["board"]["layers"][li]["background"] = material(1e5);
    }
    limiting["board"]["layers"][1]["material"] = material(1e-6);
    let reference = solve(limiting);
    complete(&reference);
    let expected_rise = 1. + 1. / g;
    let actual_rise = layer_mean(&reference, "TOP") - 25.;
    assert!(
        (actual_rise - expected_rise).abs() / expected_rise < 0.001,
        "finite via limiting rise {actual_rise}, analytic {expected_rise}"
    );
    let mut transient = r.clone();
    transient["mode"] = json!("time");
    transient["initialC"] = json!("ambient");
    transient["time"] = json!({"stopS":0.01,"sampleStepS":0.01});
    let state = solve(transient.clone());
    complete(&state);
    transient.as_object_mut().unwrap().remove("initialC");
    transient["initial"] = state["finalState"].clone();
    transient["time"]["stopS"] = json!(0.02);
    complete(&solve(transient.clone()));
    for change in 0..2 {
        let mut changed = transient.clone();
        if change == 0 {
            changed["board"]["barrel"]["thicknessMm"] = json!(0.03);
        } else {
            changed["board"]["boardIdentity"] = json!("foreign-board");
        }
        assert_eq!(
            solve(changed)["diagnostics"][0]["code"],
            "THERMAL_CONTINUATION"
        );
    }
    r["board"]["barrelHeat"][0]["barrel"] = json!("via:foreign");
    assert_eq!(solve(r)["status"], "rejected");
}

fn captured_tps_request(edge: f64) -> Value {
    captured_board_request("TPS63060DSCR", "U1", 1.5, edge)
}
#[test]
fn captured_tps_steady_real_geometry_regression() {
    let mut observed = vec![];
    for edge in [4., 2.] {
        let solved = solve(captured_tps_request(edge));
        complete(&solved);
        observed.push(solved["temperaturesC"]["U1:junction"][0].as_f64().unwrap());
        assert!(solved["quality"]["linearResidual"].as_f64().unwrap() <= 1e-8);
        assert!(
            solved["quality"]["normalizedEnergyBalance"]
                .as_f64()
                .unwrap()
                <= 1e-6
        );
    }
    let change = (observed[1] - observed[0]).abs() / (observed[1] - 30.);
    eprintln!("TPS junction coarse/fine {observed:?}, relative change {change}");
    assert!(change <= 0.02);
}

fn captured_time_gate(fixture: &str, component: &str, power: f64, deviation: Option<f64>) {
    let mut observed = vec![];
    let mut coarse_temperatures: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    let mut full_field_temporal = 0.0f64;
    for relative in [1e-4, 2.5e-5] {
        let mut r = captured_board_request(fixture, component, power, 2.);
        if let Some(d) = deviation {
            r["board"]["geometryDeviationMm"] = json!(d);
            r["board"]["maxNodes"] = json!(100000);
            r["board"]["maxEdgeMm"] = json!(1.);
            r["budget"]["maxSeconds"] = json!(240.);
        }
        r["mode"] = json!("time");
        r["initialC"] = json!("ambient");
        r["ambientC"] = json!({"kind":"step","from":25.,"to":30.,"atS":20.,"riseS":0.});
        r["network"]["sources"][0]["powerW"] =
            json!({"kind":"step","from":0.,"to":power,"atS":5.,"riseS":0.});
        r["time"] = json!({"stopS":60.,"sampleStepS":15.});
        r["numerics"] = json!({"relativeTolerance":relative,"absoluteTolerance":relative/10.});
        let solved = solve(r);
        complete(&solved);
        let temperatures = solved["temperaturesC"].as_object().unwrap();
        if coarse_temperatures.is_empty() {
            coarse_temperatures = temperatures
                .iter()
                .map(|(id, series)| {
                    (
                        id.clone(),
                        series
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_f64().unwrap())
                            .collect(),
                    )
                })
                .collect();
        } else {
            assert_eq!(coarse_temperatures.len(), temperatures.len());
            for (id, series) in temperatures {
                let old = &coarse_temperatures[id];
                assert_eq!(old.len(), series.as_array().unwrap().len());
                for (a, b) in old.iter().zip(series.as_array().unwrap()) {
                    let b = b.as_f64().unwrap();
                    full_field_temporal =
                        full_field_temporal.max((a - b).abs() / (b - 25.).abs().max(1.));
                }
            }
        }
        eprintln!(
            "{fixture} transient rel={relative} temperatures {} counters {} elapsed {} ms",
            solved["temperaturesC"][format!("{component}:junction")],
            solved["counters"],
            solved["elapsedMs"]
        );
        observed.push(
            solved["temperaturesC"][format!("{component}:junction")]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect::<Vec<_>>(),
        );
        assert!(solved["quality"]["linearResidual"].as_f64().unwrap() <= 1e-8);
        assert!(
            solved["quality"]["normalizedEnergyBalance"]
                .as_f64()
                .unwrap()
                <= 1e-6
        );
    }
    let change = observed[0]
        .iter()
        .zip(&observed[1])
        .map(|(a, b)| (a - b).abs() / (b - 25.).abs().max(1.))
        .fold(0., f64::max);
    eprintln!("{fixture} transient refinement relative change {change}");
    assert!(change <= 0.001);
    eprintln!("{fixture} full-field transient refinement relative change {full_field_temporal}");
    assert!(full_field_temporal <= 0.001);
}

#[test]
fn captured_tps_time_real_geometry_regression() {
    captured_time_gate("TPS63060DSCR", "U1", 1.5, None);
}
#[test]
#[ignore = "Large-board transient benchmark; the captured STM32 can exceed the CI wall-time budget"]
fn captured_controller_time_real_geometry_regression() {
    captured_time_gate("STM32G0_CAN_RS485", "U3", 0.8, Some(0.005));
}

#[test]
#[ignore = "Known full-field refinement failure on the captured STM32 controller; see p9-merge-status-2026-11-15.md"]
fn captured_controller_steady_geometry_budget_regression() {
    let mut temperatures = vec![];
    let mut coarse_field: std::collections::BTreeMap<String, (f64, f64)> =
        std::collections::BTreeMap::new();
    for (case, (edge, deviation)) in [(4., 0.01), (4., 0.005), (2., 0.005), (1., 0.005)]
        .into_iter()
        .enumerate()
    {
        let mut r = captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, edge);
        r["board"]["geometryDeviationMm"] = json!(deviation);
        r["board"]["maxNodes"] = json!(100000);
        let solved = solve(r);
        complete(&solved);
        let temperature = solved["temperaturesC"]["U3:junction"][0].as_f64().unwrap();
        temperatures.push(temperature);
        eprintln!("Controller edge={edge} deviation={deviation} junction={temperature} elapsed={} ms counters={}", solved["elapsedMs"],solved["counters"]);
        eprintln!(
            "Controller nodes={} planar={} triangles={} memoryPlan={} contactRefinement={}",
            solved["system"]["nodeIds"].as_array().unwrap().len(),
            solved["spatial"]["mesh"]["pointsMm"]
                .as_array()
                .unwrap()
                .len(),
            solved["spatial"]["mesh"]["triangles"]
                .as_array()
                .unwrap()
                .len(),
            solved["memoryPlan"],
            solved["spatial"]["mesh"]["localRefinement"]
        );
        let mut difference = 0.0f64;
        let mut rise = 0.0f64;
        let mut shared = std::collections::BTreeMap::new();
        let mut current_field = std::collections::BTreeMap::new();
        let mut maximum_error = Value::Null;
        for field in solved["spatial"]["mesh"]["fieldNodes"].as_array().unwrap() {
            let layer = field["layer"].as_str().unwrap();
            let key = format!(
                "{}:{}:{}",
                layer,
                field["pointMm"][0].as_f64().unwrap().to_bits(),
                field["pointMm"][1].as_f64().unwrap().to_bits()
            );
            let value = solved["temperaturesC"][field["id"].as_str().unwrap()][0]
                .as_f64()
                .unwrap();
            let area = field["supportAreaMm2"].as_f64().unwrap();
            if case >= 1 {
                current_field.insert(key.clone(), (value, area));
            }
            if case >= 2 {
                rise = rise.max((value - 30.).abs());
                if let Some((old, old_area)) = coarse_field.get(&key) {
                    let error = (old - value).abs();
                    if error > difference {
                        difference = error;
                        maximum_error = json!({"layer":layer,"pointMm":field["pointMm"],"coarseTemperatureC":old,"fineTemperatureC":value,"coarseSupportAreaMm2":old_area,"fineSupportAreaMm2":area,"differenceC":error});
                    }
                    *shared.entry(layer.to_string()).or_insert(0usize) += 1;
                }
            }
        }
        if case >= 2 {
            let relative = difference / rise;
            eprintln!("Controller refinement to edge={edge} full-field spatial relative change={relative}, shared counts={shared:?}, maximum error={maximum_error}");
            assert_eq!(shared.len(), 3);
            if case == 3 {
                assert!(relative <= 0.02);
            }
        }
        if case >= 1 {
            coarse_field = current_field;
        }
        assert!(solved["quality"]["linearResidual"].as_f64().unwrap() <= 1e-8);
        assert!(
            solved["quality"]["normalizedEnergyBalance"]
                .as_f64()
                .unwrap()
                <= 1e-6
        );
    }
    let curve_change = (temperatures[1] - temperatures[0]).abs() / (temperatures[1] - 30.);
    let start_change = (temperatures[2] - temperatures[1]).abs() / (temperatures[2] - 30.);
    let mesh_change = (temperatures[3] - temperatures[2]).abs() / (temperatures[3] - 30.);
    eprintln!("Controller curve .01/.005 relative change={curve_change}; starting mesh4/2 relative change={start_change}; final mesh2/1 relative change={mesh_change}");
    assert!(curve_change <= 0.02);
    assert!(mesh_change <= 0.02);
}

#[test]
#[ignore = "Development resource probe; original and corrected evidence saved separately"]
fn captured_controller_fine_resource_admission_probe() {
    let edge = std::env::var("THERMAL_PROBE_EDGE").map_or(1., |v| v.parse::<f64>().unwrap());
    assert!([0.5, 1., 2.].contains(&edge));
    let mut r = captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, edge);
    r["board"]["geometryDeviationMm"] = json!(0.005);
    r["board"]["maxNodes"] = json!(if edge == 0.5 { 110000 } else { 100000 });
    let memory_mib = if edge == 0.5 { 768 } else { 384 };
    r["budget"]["maxMemoryMiB"] = json!(memory_mib);
    r["budget"]["maxSeconds"] = json!(240.);
    let solved = solve(r.clone());
    #[derive(serde::Serialize)]
    struct Evidence<'a> {
        request: &'a Value,
        result: &'a Value,
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../debugging/thermal-verification/controller-mesh{edge}-resource-result.json"
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    serde_json::to_writer(
        std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
        &Evidence {
            request: &r,
            result: &solved,
        },
    )
    .unwrap();
    eprintln!(
        "Saved exact request/result bytes={}",
        std::fs::metadata(&path).unwrap().len()
    );
    complete(&solved);
    eprintln!("Controller explicit resource probe mesh{edge} .005: nodes={}, planar={}, triangles={}, links={}, elapsedMs={}, memoryPlan={}, counters={}, Tj={}",
        solved["system"]["nodeIds"].as_array().unwrap().len(),
        solved["spatial"]["mesh"]["pointsMm"].as_array().unwrap().len(),
        solved["spatial"]["mesh"]["triangles"].as_array().unwrap().len(),
        solved["system"]["linkIds"].as_array().unwrap().len(),
        solved["elapsedMs"],solved["memoryPlan"],solved["counters"],solved["temperaturesC"]["U3:junction"]);
    assert!(
        solved["memoryPlan"]["totalPlannedPeakBytes"]
            .as_u64()
            .unwrap()
            <= memory_mib * 1048576
    );
    assert!(solved["quality"]["linearResidual"].as_f64().unwrap() <= 1e-8);
}

fn adaptive_coupon() -> Value {
    let mut r = request(0.5);
    let mut b = board();
    b["outline"] = points(0., 0., 1., 1.);
    r["board"]["boardJson"] = json!(b.to_string());
    r["board"]["maxNodes"] = json!(5000);
    r["board"]["heat"][0]["target"]["polygon"]["outer"] = points(0., 0., 1., 1.);
    r["board"]["contacts"][0]["target"]["polygon"]["outer"] = points(0., 0., 1., 1.);
    r["board"]["heat"][0]["powerW"] = json!(0.001);
    r["board"]["refinement"] =
        json!({"maxPasses":2,"relativeTolerance":0.02,"validationMaxEdgeMm":0.25});
    r["budget"] = json!({"maxSeconds":90.,"maxMemoryMiB":384.});
    r
}

#[test]
fn adaptive_refinement_uniform_analytic_and_independent_final_field() {
    let solved = solve(adaptive_coupon());
    complete(&solved);
    assert_eq!(
        solved["refinement"]["converged"], true,
        "{}",
        solved["refinement"]
    );
    assert_eq!(solved["refinement"]["finalRole"], "independent-control");
    assert_eq!(solved["spatial"]["mesh"]["maxEdgeMm"], 0.25);
    // Uniform exact flux, 1 mm²: bottom1 K/W, then harmonic slice centers.
    let expected = 25. + 0.001 * (1. + 50. + 500. + 100. / 3.);
    assert!((layer_mean(&solved, "TOP") - expected).abs() < 1e-7);
    let history = solved["refinement"]["history"].as_array().unwrap();
    assert!(history.len() >= 3 && history.len() <= 4);
    for level in history {
        assert!(
            level["summary"]["quality"]["originalNodalResidual"]
                .as_f64()
                .unwrap()
                <= 1e-8
        );
        assert!(level.get("temperaturesC").is_none());
        assert!(level.get("spatial").is_none());
    }
    let proof = &solved["spatial"]["mesh"]["nested"]["nestedProof"];
    assert_eq!(
        proof["checkedFineCells"].as_u64().unwrap() as usize,
        solved["spatial"]["mesh"]["triangles"]
            .as_array()
            .unwrap()
            .len()
    );
    assert!(proof["maximumFineToParentDiameterRatio"].as_f64().unwrap() <= 1. + 1e-8);
    assert!(
        solved["memoryPlan"]["refinementWorkspaceBytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    eprintln!(
        "Small adaptive analytical gate: elapsed={} refinement={} memory={}",
        solved["elapsedMs"], solved["refinement"], solved["memoryPlan"]
    );
}

#[test]
fn adaptive_refinement_time_events_zero_signal_and_absolute_translation() {
    let mut request = adaptive_coupon();
    request["mode"] = json!("time");
    request["initialC"] = json!("ambient");
    request["time"] = json!({"stopS":0.2,"sampleStepS":0.1});
    request["ambientC"] = json!({"kind":"step","from":25.,"to":26.,"atS":0.11,"riseS":0.});
    request["board"]["heat"][0]["powerW"] =
        json!({"kind":"step","from":0.,"to":0.001,"atS":0.07,"riseS":0.});
    request["board"]["surfaceCooling"] = json!({"resistanceKPerW":100.,"scale":{"kind":"step","from":0.,"to":1.,"atS":0.13,"riseS":0.}});
    request["numerics"] = json!({"relativeTolerance":0.0001,"absoluteTolerance":1e-7});
    let solved = solve(request.clone());
    complete(&solved);
    assert_eq!(
        solved["refinement"]["converged"], true,
        "{}",
        solved["refinement"]
    );
    let expected = json!([0., 0.07, 0.1, 0.11, 0.13, 0.2]);
    assert_eq!(solved["axis"], expected);
    for pass in solved["refinement"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
    {
        assert_eq!(
            pass["comparison"]["timeSamples"],
            expected.as_array().unwrap().len()
        );
        assert_eq!(pass["comparison"]["observationAxisReference"], "axis");
        assert_eq!(pass["comparison"]["missingPhysicalOrBoundarySamples"], 0);
    }
    assert!(
        solved["quality"]["normalizedEnergyBalance"]
            .as_f64()
            .unwrap()
            < 1e-6
    );
    let mut zero = adaptive_coupon();
    zero["board"]["heat"][0]["powerW"] = json!(0.);
    zero["ambientC"] = json!(1_000_025.);
    let zero = solve(zero);
    complete(&zero);
    assert_eq!(zero["refinement"]["converged"], true);
    assert!(
        zero["refinement"]["history"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["comparison"]["maximumDifferenceK"]
            .as_f64()
            .unwrap()
            <= 1e-9
    );
}

#[test]
fn adaptive_refinement_shared_budget_continuation_and_invalid_policy_reject() {
    let mut r = adaptive_coupon();
    r["budget"]["maxSeconds"] = json!(1e-12);
    let rejected = solve(r);
    assert_eq!(rejected["status"], "rejected");
    assert!(rejected.get("temperaturesC").is_none());
    let mut r = adaptive_coupon();
    r["budget"]["maxMemoryMiB"] = json!(1.);
    assert_eq!(solve(r)["status"], "rejected");
    let mut r = adaptive_coupon();
    r["board"].as_object_mut().unwrap().remove("refinement");
    r["board"]["maxNodes"] = json!(140000);
    assert!(solve(r)["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("require explicit opt-in refinement"));
    let mut r = adaptive_coupon();
    r["initial"] = json!({"kind":"thermal-state","contextId":"foreign"});
    assert!(solve(r)["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("Continuation"));
    for value in [
        json!({"maxPasses":7,"relativeTolerance":0.02,"validationMaxEdgeMm":0.25}),
        json!({"maxPasses":1,"relativeTolerance":0.03,"validationMaxEdgeMm":0.25}),
        json!({"maxPasses":1,"relativeTolerance":0.02,"validationMaxEdgeMm":0.5}),
    ] {
        let mut r = adaptive_coupon();
        r["board"]["refinement"] = value;
        assert_eq!(solve(r)["status"], "rejected");
    }
}

#[test]
#[ignore = "Parent-authorized single production policy measurement; full evidence retained"]
fn captured_controller_production_policy_steady_probe() {
    let mut request = captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, 1.);
    request["board"]["geometryDeviationMm"] = json!(0.005);
    request["board"]["maxNodes"] = json!(140000);
    request["board"]["refinement"] =
        json!({"maxPasses":5,"relativeTolerance":0.02,"validationMaxEdgeMm":0.5});
    request["budget"] = json!({"maxSeconds":900.,"maxMemoryMiB":1024.});
    let result = solve(request.clone());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../debugging/thermal-verification/controller-production-policy-steady-result.json",
    );
    #[derive(serde::Serialize)]
    struct Evidence<'a> {
        request: &'a Value,
        result: &'a Value,
    }
    serde_json::to_writer(
        std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
        &Evidence {
            request: &request,
            result: &result,
        },
    )
    .unwrap();
    eprintln!("Production opt-in controller: savedBytes={} status={} refinement={} quality={} memory={} Tj={}",
        std::fs::metadata(path).unwrap().len(),result["status"],result["refinement"],result["quality"],result["memoryPlan"],result["temperaturesC"]["U3:junction"]);
    complete(&result);
    assert_eq!(result["refinement"]["converged"], true);
    assert_eq!(result["refinement"]["finalRole"], "independent-control");
    assert!(result["quality"]["originalNodalResidual"].as_f64().unwrap() <= 1e-8);
    assert!(
        result["quality"]["normalizedEnergyBalance"]
            .as_f64()
            .unwrap()
            <= 1e-6
    );
    assert!(
        result["refinement"]["maximumPlannedPeakBytes"]
            .as_u64()
            .unwrap()
            <= 1073741824
    );
}

#[test]
fn adaptive_refinement_exhausted_target_is_explicitly_unconverged() {
    let mut r = adaptive_coupon();
    r["board"]["heat"][0]["target"]["polygon"]["outer"] = points(0.25, 0.25, 0.75, 0.75);
    r["board"]["refinement"]["maxPasses"] = json!(1);
    r["board"]["refinement"]["relativeTolerance"] = json!(0.0001);
    let solved = solve(r);
    complete(&solved);
    assert_eq!(
        solved["refinement"]["converged"], false,
        "{}",
        solved["refinement"]
    );
    assert_eq!(solved["refinement"]["finalRole"], "unconverged-candidate");
    assert_eq!(solved["refinement"]["history"].as_array().unwrap().len(), 2);
    assert!(solved["quality"]["originalNodalResidual"].as_f64().unwrap() <= 1e-8);
}

#[test]
fn adaptive_refinement_and_ordinary_board_admit_signal_fanout_before_cloning() {
    let mut r = adaptive_coupon();
    r["board"].as_object_mut().unwrap().remove("refinement");
    r["budget"]["maxMemoryMiB"] = json!(1.);
    complete(&solve(r.clone()));
    let points: Vec<_> = (0..10000)
        .map(|i| json!([i as f64 * 0.0001, 0.001]))
        .collect();
    let mut pulse = r.clone();
    pulse["board"]["heat"][0]["powerW"] =
        json!({"kind":"pwl","points":points,"interpolation":"linear"});
    let rejected = solve(pulse);
    assert_eq!(rejected["status"], "rejected");
    assert!(rejected["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("fanout memory plan"));
    let table: Vec<_> = (-4999..=4999)
        .map(|i| json!({"deltaTemperatureK":i as f64,"heatFlowW":i as f64/100.}))
        .collect();
    r["board"]["surfaceCooling"] = json!({"table":table});
    let rejected = solve(r);
    assert_eq!(rejected["status"], "rejected");
    assert!(rejected["diagnostics"][0]["message"]
        .as_str()
        .unwrap()
        .contains("fanout memory plan"));
}

#[test]
#[ignore = "Explicit development evidence export; other mathematical tests are self-contained"]
fn save_controller_request_for_linear_assembly_audit() {
    let mut r = captured_board_request("STM32G0_CAN_RS485", "U3", 0.8, 4.);
    r["board"]["geometryDeviationMm"] = json!(0.01);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../debugging/thermal-verification/controller-linear-request.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&r).unwrap()).unwrap();
}
