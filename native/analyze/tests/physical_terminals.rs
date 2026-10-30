use pcb_analyze::electrical::{
    coupled::{composition, time, Model},
    frequency,
};
use serde_json::{json, Value};
use std::time::Instant;

fn cell(second: bool) -> Value {
    let c = if second {
        vec![7e-9, -3e-9, -3e-9, 7e-9]
    } else {
        vec![3e-9, -2e-9, -2e-9, 3e-9]
    };
    json!({"id":if second{"c2"}else{"c1"},"kind":"network","ports":["join","probe"],"analyses":["si","networkParameters"],"source":"Authored independent conductor coupon","data":{
        "format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["p","n"],"currentDofs":["j"],
        "incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.],[0,1,1.],[1,1,-1.]],
        "resistanceOhm":[[0,0,100.]],"inductanceH":[[0,0,1e-6]],"capacitanceF":[[0,0,c[0]],[0,1,c[1]],[1,0,c[2]],[1,1,c[3]]],
        "conservedPotentialModes":[[1.,1.]],"portTerminals":[{"positive":{"node":"p"},"negative":{"node":"n"}},{"positive":{"node":"p"},"negative":{"node":"n"}}]}})
}
fn topology(contacts: &str) -> Value {
    json!({"instances":[{"id":"left","model":cell(false),"ports":["join","probe"]},{"id":"right","model":cell(true),"ports":["join","probe"]}],"connections":[[0,2]],"connectionContacts":[contacts],"ports":[{"name":"left","index":1},{"name":"right","index":3}]})
}
fn request(contacts: &str, common: bool) -> Value {
    json!({"topology":topology(contacts),"drivers":[],"receivers":[],"time":{"stopS":5e-6,"sampleStepS":1e-8},
        "initial":{"kind":"physical-state","steady":false,"potentialsV":[{"instanceId":"left","node":"p","value":2.},{"instanceId":"left","node":"n","value":if common{2.}else{1.}},{"instanceId":"right","node":"p","value":2.},{"instanceId":"right","node":"n","value":if common{2.}else{1.}}],"currentsA":[{"instanceId":"left","dof":"j","value":0.},{"instanceId":"right","dof":"j","value":0.}]},
        "numerics":{"relativeTolerance":1e-6,"absoluteTolerance":1e-10,"maxStepS":1e-9},"budget":{"maxSeconds":60.,"maxMemoryMiB":256.}})
}
fn solve(q: Value) -> Value {
    let result = time::solve(serde_json::from_value(q).unwrap()).unwrap();
    let axis = result["axis"].as_array().unwrap();
    assert!(
        axis.windows(2).all(|w| number(&w[1]) > number(&w[0])),
        "duplicate output endpoint"
    );
    result
}
fn near(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, tol {t}");
}
fn number(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn sum(v: &Value) -> f64 {
    v.as_array().unwrap().iter().map(number).sum()
}

#[test]
fn full_c_weld_matches_one_independently_authored_model_and_analytic_decay() {
    let built = composition::build(
        &serde_json::from_value(topology("both")).unwrap(),
        None,
        Instant::now(),
    )
    .unwrap();
    assert_eq!(built.model.potential_nodes.len(), 2);
    assert_eq!(
        built.model.capacitance_f,
        vec![(0, 0, 1e-8), (0, 1, -5e-9), (1, 0, -5e-9), (1, 1, 1e-8)]
    );
    let mut q = request("both", false);
    let result = solve(q.clone());
    let single = json!({"id":"one-original","kind":"network","ports":["left","right"],"analyses":["si"],"data":{"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["p","n"],"currentDofs":["a","b"],"incidence":[[0,0,1.],[1,0,-1.],[0,1,1.],[1,1,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.],[0,1,1.],[1,1,-1.]],"resistanceOhm":[[0,0,100.],[1,1,100.]],"inductanceH":[[0,0,1e-6],[1,1,1e-6]],"capacitanceF":[[0,0,1e-8],[0,1,-5e-9],[1,0,-5e-9],[1,1,1e-8]],"conservedPotentialModes":[[1.,1.]]}});
    q.as_object_mut().unwrap().remove("topology");
    q["resource"] = single;
    q["initial"] = json!({"kind":"physical-state","steady":false,"potentialsV":[{"instanceId":"one-original","node":"p","value":2.},{"instanceId":"one-original","node":"n","value":1.}],"currentsA":[{"instanceId":"one-original","dof":"a","value":0.},{"instanceId":"one-original","dof":"b","value":0.}]});
    let original = solve(q);
    let mut error = 0_f64;
    let roots = [
        (-1e8 + (1e16_f64 - 4. / (0.5e-6 * 7.5e-9)).sqrt()) / 2.,
        (-1e8 - (1e16_f64 - 4. / (0.5e-6 * 7.5e-9)).sqrt()) / 2.,
    ];
    for (i, t) in result["axis"].as_array().unwrap().iter().enumerate() {
        let t = number(t);
        let delta = (-roots[1] * (roots[0] * t).exp() + roots[0] * (roots[1] * t).exp())
            / (roots[0] - roots[1]);
        error = error.max((number(&result["portVoltages"][i][0]) - delta).abs());
        near(
            number(&result["portVoltages"][i][0]),
            number(&original["portVoltages"][i][0]),
            1e-10,
        );
        near(sum(&result["conductorChargesC"][i]), 15e-9, 1e-18);
    }
    assert!(error <= 0.00025, "analytic error {error}");
    near(number(&result["energyJ"][0]), 15e-9, 1e-20);
    near(
        number(&result["quality"]["normalizedEnergyBalance"]),
        0.,
        1e-4,
    );
    let mut fine = request("both", false);
    fine["numerics"]["relativeTolerance"] = json!(2.5e-7);
    fine["numerics"]["absoluteTolerance"] = json!(2.5e-11);
    fine["numerics"]["maxStepS"] = json!(0.5e-9);
    let fine = solve(fine);
    let mut refined = 0_f64;
    for (i, t) in fine["axis"].as_array().unwrap().iter().enumerate() {
        let t = number(t);
        let delta = (-roots[1] * (roots[0] * t).exp() + roots[0] * (roots[1] * t).exp())
            / (roots[0] - roots[1]);
        refined = refined.max((number(&fine["portVoltages"][i][0]) - delta).abs());
    }
    assert!(
        refined <= 0.000125 && (refined <= error * 0.51 || refined < 1e-6),
        "{error} -> {refined}"
    );
}

// Independent ODE: inverse of the hand-written 3x3 C, not production assembly.
fn derivative(x: [f64; 5]) -> [f64; 5] {
    [
        (-7. * x[3] - 12. * x[4]) * 1e9 / 155.,
        (47. * x[3] - 8. * x[4]) * 1e9 / 155.,
        (-3. * x[3] + 17. * x[4]) * 1e9 / 155.,
        (x[0] - x[1] - 100. * x[3]) * 1e6,
        (x[0] - x[2] - 100. * x[4]) * 1e6,
    ]
}
fn reference_step(x: [f64; 5], dt: f64) -> [f64; 5] {
    let mut out = x;
    let mut term = x;
    for k in 1..=12 {
        let d = derivative(term);
        for j in 0..5 {
            term[j] = d[j] * dt / k as f64;
            out[j] += term[j];
        }
    }
    out
}
#[test]
fn one_contact_has_distinct_returns_without_hidden_ground_and_preserves_charge() {
    let result = solve(request("positive", false));
    assert_eq!(
        result["conductorPotentialsV"][0].as_array().unwrap().len(),
        3
    );
    let mut reference = [2., 1., 1., 0., 0.];
    let mut finer = reference;
    let mut error = 0_f64;
    let mut separation = 0_f64;
    for (i, _) in result["axis"].as_array().unwrap().iter().enumerate() {
        if i > 0 {
            for _ in 0..40 {
                reference = reference_step(reference, 0.25e-9);
            }
            for _ in 0..80 {
                finer = reference_step(finer, 0.125e-9);
            }
        }
        for j in 0..5 {
            near(reference[j], finer[j], 1e-10);
        }
        let v = &result["conductorPotentialsV"][i];
        for j in 0..3 {
            error = error.max((number(&v[j]) - reference[j]).abs());
        }
        separation = separation.max((number(&v[1]) - number(&v[2])).abs());
        near(sum(&result["conductorChargesC"][i]), 15e-9, 1e-18);
    }
    assert!(error < 0.00025, "reference error {error}");
    assert!(separation > 0.01);
    for contacts in ["both", "positive", "negative"] {
        let common = solve(request(contacts, true));
        for q in common["conductorChargesC"].as_array().unwrap() {
            near(sum(q), 20e-9, 1e-18);
        }
        for u in common["energyJ"].as_array().unwrap() {
            near(number(u), 20e-9, 1e-20);
        }
    }
}

#[test]
fn strict_terminal_initial_trace_budget_and_dc_refusals() {
    for k in 0..10 {
        let mut q = request("both", false);
        match k {
            0 => q["topology"]["instances"][0]["model"]["data"]["portIncidence"][0][2] = json!(-1.),
            1 => {
                q["topology"]["instances"][1]["model"]["data"]
                    .as_object_mut()
                    .unwrap()
                    .remove("portTerminals");
            }
            2 => {
                q["topology"]["instances"][0]["model"]["data"]["portTerminals"][0]["negative"] =
                    json!({"reference":"ground"})
            }
            3 => {
                q["topology"]["instances"][0]["model"]["data"]["portTerminals"][0]["positive"] =
                    json!({"node":"unknown"})
            }
            4 => q["initial"]["potentialsV"][2]["value"] = json!(3.),
            5 => {
                q.as_object_mut().unwrap().remove("initial");
            }
            6 => {
                q["initial"]["potentialsV"].as_array_mut().unwrap().pop();
            }
            7 => q["initial"]["steady"] = json!(true),
            8 => q["budget"]["maxMemoryMiB"] = json!(0.01),
            _ => {
                q["topology"]["instances"][0]["model"]["data"]["terminalTrace"] =
                    json!({"unknownReducedBasis":true})
            }
        };
        assert!(
            time::solve(serde_json::from_value(q).unwrap()).is_err(),
            "negative{k}"
        );
    }
    let mut m: Model = serde_json::from_value(cell(false)["data"].clone()).unwrap();
    m.current_dofs.clear();
    m.incidence.clear();
    m.resistance_ohm.clear();
    m.inductance_h.clear();
    m.conserved_potential_modes.clear();
    assert!(
        pcb_analyze::electrical::coupled::impedance(&m, 2, &[0.], None)
            .unwrap_err()
            .starts_with("COUPLED_DC_INCOMPATIBLE")
    );
    let isolated:Model=serde_json::from_value(json!({"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["isolated"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,1e-9]]})).unwrap();
    let mut system = pcb_analyze::electrical::coupled::System::compile(&isolated, 1, None).unwrap();
    assert!(system
        .solve(0., vec![1e-3], &[0.], &mut Default::default())
        .unwrap_err()
        .starts_with("COUPLED_DC_INCOMPATIBLE"));
}

#[test]
fn legacy_grounded_continuation_with_null_physical_seed_is_compatible() {
    let mut q = json!({"resource":{"id":"legacy","kind":"network","ports":["p"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["v"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,1e-9]]}},"drivers":[{"port":0,"resistanceOhm":50.,"waveform":{"kind":"step","from":1.,"to":1.,"atS":0.,"riseS":1e-9}}],"receivers":[],"time":{"stopS":1e-7,"sampleStepS":1e-9}});
    let original = solve(q.clone());
    q["initial"] = original["finalState"].clone();
    q["time"]["stopS"] = json!(1.1e-7);
    let r = solve(q);
    for v in r["portVoltages"].as_array().unwrap() {
        near(number(&v[0]), 1., 1e-12);
    }
}

#[test]
fn weld_cannot_silently_delete_a_shorted_external_measurement() {
    let mut top = topology("both");
    for (k, i) in top["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        i["ports"] = json!(["join", "reverse", "probe"]);
        i["model"]["ports"] = i["ports"].clone();
        let sign = if k == 0 { 1. } else { -1. };
        i["model"]["data"]["portIncidence"] = json!([
            [0, 0, 1.],
            [1, 0, -1.],
            [0, 1, sign],
            [1, 1, -sign],
            [0, 2, 1.],
            [1, 2, -1.]
        ]);
        let p = json!({"positive":{"node":"p"},"negative":{"node":"n"}});
        let reverse = if k == 0 {
            p.clone()
        } else {
            json!({"positive":{"node":"n"},"negative":{"node":"p"}})
        };
        i["model"]["data"]["portTerminals"] = json!([p, reverse, p]);
    }
    top["connections"] = json!([[0, 3], [1, 4]]);
    top["connectionContacts"] = json!(["both", "both"]);
    top["ports"] = json!([{"name":"left","index":2},{"name":"right","index":5}]);
    let error = composition::build(&serde_json::from_value(top).unwrap(), None, Instant::now())
        .err()
        .unwrap();
    assert!(
        error.contains("External port") && error.contains("shorted"),
        "{error}"
    );
}

#[test]
fn physical_frequency_uses_same_assembly_and_retains_domains_permissions() {
    let mut q = topology("both");
    q["frequencies"] = json!([1e3, 1e6, 1e8]);
    q["representation"] = json!("Z");
    let r = frequency::run(serde_json::from_value(q.clone()).unwrap());
    assert_eq!(r["status"], "complete", "{r}");
    for (k, f) in [1e3, 1e6, 1e8].iter().enumerate() {
        let w = 2. * std::f64::consts::PI * f;
        let r2 = 50.;
        let l2 = 0.5e-6;
        let c = 7.5e-9;
        let yr = r2 / (r2 * r2 + (w * l2).powi(2));
        let yi = w * c - w * l2 / (r2 * r2 + (w * l2).powi(2));
        let d = yr * yr + yi * yi;
        near(number(&r["parameters"][k][0][0]["real"]), yr / d, 1e-8);
        near(
            number(&r["parameters"][k][0][0]["imaginary"]),
            -yi / d,
            1e-8,
        );
    }
    let mut banded = q.clone();
    banded["instances"][0]["model"]["validity"] =
        json!({"status":"conditional","reason":"producer domain","domain":{"bandHz":[1e3,1e8]}});
    banded["instances"][1]["model"]["validity"] =
        json!({"status":"valid","domain":{"bandHz":[100.,1e7]}});
    let rejected = frequency::run(serde_json::from_value(banded.clone()).unwrap());
    assert_eq!(rejected["status"], "rejected");
    banded["frequencies"] = json!([1e3, 1e6]);
    let admitted = frequency::run(serde_json::from_value(banded.clone()).unwrap());
    assert_eq!(admitted["status"], "complete");
    assert_eq!(admitted["bandHz"], json!([1e3, 1e7]));
    assert_eq!(admitted["conditional"], true);
    assert_eq!(
        admitted["provenance"][0]["resource"]["validity"],
        banded["instances"][0]["model"]["validity"]
    );
    q["instances"][0]["model"]["analyses"] = json!(["si"]);
    let r = frequency::run(serde_json::from_value(q).unwrap());
    assert_eq!(r["status"], "rejected");
}

#[test]
fn permutation_orientation_continuation_and_true_zero_energy_gauge() {
    let original = solve(request("both", false));
    let mut q = request("both", false);
    for instance in q["topology"]["instances"].as_array_mut().unwrap() {
        let m = &mut instance["model"]["data"];
        m["potentialNodes"] = json!(["renamed-n", "renamed-p"]);
        for key in ["incidence", "portIncidence", "capacitanceF"] {
            for entry in m[key].as_array_mut().unwrap() {
                entry[0] = json!(1 - entry[0].as_u64().unwrap());
                if key == "capacitanceF" {
                    entry[1] = json!(1 - entry[1].as_u64().unwrap());
                }
                if key == "portIncidence" {
                    entry[2] = json!(-number(&entry[2]));
                }
            }
        }
        for p in m["portTerminals"].as_array_mut().unwrap() {
            p["positive"] = json!({"node":"renamed-n"});
            p["negative"] = json!({"node":"renamed-p"});
        }
    }
    for p in q["initial"]["potentialsV"].as_array_mut().unwrap() {
        p["node"] = json!(if p["node"] == "p" {
            "renamed-p"
        } else {
            "renamed-n"
        });
    }
    let changed = solve(q);
    for (a, b) in original["portVoltages"]
        .as_array()
        .unwrap()
        .iter()
        .zip(changed["portVoltages"].as_array().unwrap())
    {
        for j in 0..2 {
            near(number(&a[j]), -number(&b[j]), 1e-10);
        }
    }
    let mut continued = request("both", false);
    continued["initial"] = original["finalState"].clone();
    continued["time"]["stopS"] = json!(5.01e-6);
    // Tiny late exchange on a charged rail must reject if F64 energy balance is unresolved.
    assert!(
        time::solve(serde_json::from_value(continued.clone()).unwrap())
            .unwrap_err()
            .starts_with("COUPLED_ENERGY")
    );
    let mut earlier = request("both", false);
    earlier["time"]["stopS"] = json!(1e-6);
    let earlier = solve(earlier);
    continued["initial"] = earlier["finalState"].clone();
    continued["time"]["stopS"] = json!(1.01e-6);
    time::solve(serde_json::from_value(continued.clone()).unwrap()).unwrap();
    continued["initial"]["physicalInitial"]["potentialsV"][0]["value"] = json!(3.);
    assert!(time::solve(serde_json::from_value(continued).unwrap())
        .unwrap_err()
        .contains("identity"));
    let mut gauge = request("both", true);
    for i in 0..2 {
        gauge["topology"]["instances"][i]["model"]["data"]["capacitanceF"] =
            json!([[0, 0, 1e-9], [0, 1, -1e-9], [1, 0, -1e-9], [1, 1, 1e-9]]);
    }
    let gauge = solve(gauge);
    assert_eq!(gauge["zeroEnergyGauges"], 1);
    for q in gauge["conductorChargesC"].as_array().unwrap() {
        near(sum(q), 0., 1e-20);
    }
}

#[test]
fn only_an_authored_ground_return_can_ground_an_infinity_conductor() {
    let ground = json!({"id":"explicit-return","kind":"network","ports":["join","probe"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["g"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.],[0,1,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,1e-9]],"portTerminals":[{"positive":{"node":"g"},"negative":{"reference":"ground"}},{"positive":{"node":"g"},"negative":{"reference":"ground"}}]}});
    let mut q = request("both", false);
    q["topology"]["instances"][1]["model"] = ground;
    q["initial"] = json!({"kind":"physical-state","steady":false,"potentialsV":[{"instanceId":"left","node":"p","value":1.},{"instanceId":"left","node":"n","value":0.},{"instanceId":"right","node":"g","value":1.}],"currentsA":[{"instanceId":"left","dof":"j","value":0.}]});
    let r = solve(q.clone());
    assert!(r["physicalRecovery"]["potentials"][1]["index"].is_null());
    near(number(&r["energyJ"][0]), 2e-9, 1e-20);
    q["initial"]["potentialsV"][1]["value"] = json!(1.);
    assert!(time::solve(serde_json::from_value(q.clone()).unwrap())
        .unwrap_err()
        .contains("ground"));
    q["topology"]["connectionContacts"] = json!(["positive"]);
    let r = solve(q);
    assert!(r["physicalRecovery"]["potentials"][1]["index"].is_number());
}
#[test]
fn mapped_three_way_junction_preserves_all_parallel_current_coordinates(){
 let mut top=topology("both");let third=top["instances"][0].clone();top["instances"].as_array_mut().unwrap().push(third);top["instances"][2]["id"]=json!("third");top["instances"][2]["model"]["id"]=json!("third-model");
 top["connections"]=json!([[0,2],[0,4]]);top["connectionContacts"]=json!(["both","both"]);top["ports"]=json!([{"name":"a","index":1},{"name":"b","index":3},{"name":"c","index":5}]);
 let built=composition::build(&serde_json::from_value(top).unwrap(),None,Instant::now()).unwrap();assert_eq!(built.model.current_dofs.len(),3);assert_eq!(built.model.potential_nodes.len(),2);assert_eq!(built.ports.len(),3);
}
