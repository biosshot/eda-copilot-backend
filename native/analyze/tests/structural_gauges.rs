use pcb_analyze::electrical::{
    coupled::{self, composition, time, Model, System},
    Counters,
};
use serde_json::{json, Value};
use std::time::Instant;
fn cell(exterior: f64) -> Model {
    serde_json::from_value(json!({"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["p","n"],"currentDofs":["j"],"incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.]],"resistanceOhm":[[0,0,100.]],"inductanceH":[],"capacitanceF":[[0,0,1e-6+exterior],[0,1,-1e-6],[1,0,-1e-6],[1,1,1e-6+exterior]],"conservedPotentialModes":[[1.,1.]],"structuralGaugeModes":if exterior==0.{json!([[1.,1.]])}else{json!([])},"portTerminals":[{"positive":{"node":"p"},"negative":{"node":"n"}}]})).unwrap()
}
fn resource(m: &Model) -> Value {
    json!({"id":"cell","kind":"network","ports":["p"],"analyses":["si","networkParameters"],"data":m})
}
fn z(m: &Model) -> Value {
    coupled::impedance(m, 1, &[0., 1e3, 1e4, 1e5], None).unwrap()
}
fn composition_resource(m: &Model) -> Value {
    let mut model = m.clone();
    let extra: Vec<_> = model
        .port_incidence
        .iter()
        .map(|&(i, _, v)| (i, 1, v))
        .collect();
    model.port_incidence.extend(extra);
    if let Some(t) = &mut model.port_terminals {
        t.push(t[0].clone());
    }
    json!({"id":"cell","kind":"network","ports":["join","probe"],"analyses":["si","networkParameters"],"data":model})
}
#[test]
fn certificate_scaling_permutation_and_reference_preserve_observables() {
    let m = cell(0.);
    let expected = z(&m);
    for factor in [1e-100, -3., 1e100] {
        let mut other = m.clone();
        other.structural_gauge_modes = vec![vec![factor; 2]];
        other.potential_nodes.reverse();
        for entries in [
            &mut other.incidence,
            &mut other.port_incidence,
            &mut other.capacitance_f,
        ] {
            for (i, _, _) in entries {
                *i = 1 - *i;
            }
        }
        for (_, j, _) in &mut other.capacitance_f {
            *j = 1 - *j;
        }
        let actual = z(&other);
        for k in 0..4 {
            for field in ["real", "imaginary"] {
                assert!(
                    (actual["parameters"][k][0][0][field].as_f64().unwrap()
                        - expected["parameters"][k][0][0][field].as_f64().unwrap())
                    .abs()
                        < 1e-10
                );
            }
        }
    }
    let mut s = System::compile(&m, 1, None).unwrap();
    let rhs = s.port_rhs(&[0.01]);
    let x = s
        .solve(0., rhs, &[17., 17., 0.], &mut Counters::default())
        .unwrap();
    assert!((x[0] - x[1] - 1.).abs() < 1e-12);
    assert!((x[0] + x[1]).abs() < 1e-12);
}
#[test]
fn invalid_certificates_and_impossible_sources_have_specific_failures() {
    for q in [
        vec![vec![1.]],
        vec![vec![f64::NAN, 1.]],
        vec![vec![0., 0.]],
        vec![vec![1., 1.], vec![2., 2.]],
    ] {
        let mut m = cell(0.);
        m.structural_gauge_modes = q;
        assert!(System::compile(&m, 1, None)
            .err()
            .unwrap()
            .contains("COUPLED_GAUGE"));
    }
    let mut m = cell(1e-18);
    m.structural_gauge_modes = vec![vec![1., 1.]];
    assert!(System::compile(&m, 1, None)
        .err()
        .unwrap()
        .contains("incompatible with C"));
    let mut m = cell(0.);
    m.capacitance_f.clear();
    m.structural_gauge_modes = vec![vec![1., 0.]];
    assert!(System::compile(&m, 1, None).err().unwrap().contains("A^T"));
    let mut m = cell(0.);
    m.reference_topology = "grounded".into();
    m.port_terminals = None;
    m.port_incidence = vec![(0, 0, 1.)];
    let mut incompatible = System::compile(&m, 1, None).unwrap();
    let rhs = incompatible.port_rhs(&[1.]);
    assert!(incompatible
        .solve(
            1e3,
            {
                let mut r = rhs;
                r.resize(6, 0.);
                r
            },
            &[0.; 3],
            &mut Counters::default()
        )
        .unwrap_err()
        .contains("COUPLED_GAUGE_INCOMPATIBLE"));

    let mut s = System::compile(&cell(0.), 1, None).unwrap();
    for omega in [0., 1e3] {
        let mut rhs = vec![1., 0., 0.];
        if omega != 0. {
            rhs.resize(6, 0.);
        }
        assert!(s
            .solve(omega, rhs, &[0.; 3], &mut Counters::default())
            .unwrap_err()
            .contains("COUPLED_GAUGE_INCOMPATIBLE"));
    }
}
#[test]
fn physical_exterior_capacitance_and_common_charge_are_never_inferred_as_gauge() {
    for ext in [1e-6, 1e-12, 1e-18] {
        let m = cell(ext);
        assert!(m.structural_gauge_modes.is_empty());
        let mut s = System::compile(&m, 1, None).unwrap();
        let x = s
            .solve(0., vec![0.; 3], &[3., 3., 0.], &mut Counters::default())
            .unwrap();
        assert!((x[0] - 3.).abs() < 1e-8 && (x[1] - 3.).abs() < 1e-8);
        let e = s.energy(&x);
        assert!((e - 9. * ext).abs() < 1e-20 + 1e-4 * 9. * ext);
        let actual = z(&m);
        let omega = 2. * std::f64::consts::PI * 1e5;
        let c = 1e-6 + ext / 2.;
        let den = 0.01f64.powi(2) + (omega * c).powi(2);
        assert!(
            (actual["parameters"][3][0][0]["imaginary"].as_f64().unwrap() + omega * c / den).abs()
                < 1e-9
        );
    }
}
#[test]
fn composition_intersects_gauges_with_welds_ground_and_exterior_storage() {
    for (connection, ext, count) in [(true, 0., 1), (true, 1e-6, 0)] {
        let top = json!({"instances":[{"id":"a","model":composition_resource(&cell(0.)),"ports":["join","probe"]},{"id":"b","model":composition_resource(&cell(ext)),"ports":["join","probe"]}],"connections":if connection{json!([[0,2]])}else{json!([])},"ports":if connection{json!([{"name":"a","index":1},{"name":"b","index":3}])}else{json!([{"name":"aJoin","index":0},{"name":"aProbe","index":1},{"name":"bJoin","index":2},{"name":"bProbe","index":3}])}});
        let built = composition::build(&serde_json::from_value(top).unwrap(), None, Instant::now())
            .unwrap();
        assert_eq!(built.model.structural_gauge_modes.len(), count);
        coupled::impedance(&built.model, built.ports.len(), &[1e3], None).unwrap();
    }
}
#[test]
fn transient_preserves_differential_charge_and_independent_rc_decay() {
    let mut m = cell(0.);
    m.current_dofs.clear();
    m.incidence.clear();
    m.resistance_ohm.clear();
    m.conserved_potential_modes.clear();
    let q = json!({"resource":resource(&m),"drivers":[],"receivers":[{"port":0,"resistanceOhm":100.,"capacitanceF":0.}],"time":{"stopS":1e-4,"sampleStepS":1e-6},"initial":{"kind":"physical-state","steady":false,"potentialsV":[{"instanceId":"cell","node":"p","value":3.},{"instanceId":"cell","node":"n","value":1.}],"currentsA":[]},"numerics":{"relativeTolerance":1e-6,"absoluteTolerance":1e-10,"maxStepS":1e-7},"budget":{"maxSeconds":120.,"maxMemoryMiB":512.}});
    let result = time::solve(serde_json::from_value(q).unwrap()).unwrap();
    for (k, t) in result["axis"].as_array().unwrap().iter().enumerate() {
        let at = t.as_f64().unwrap();
        let v = result["portVoltages"][k][0].as_f64().unwrap();
        assert!((v - 2. * (-at / 1e-4).exp()).abs() < 5e-4);
        let charge = result["conductorChargesC"][k].as_array().unwrap();
        assert!((charge[0].as_f64().unwrap() + charge[1].as_f64().unwrap()).abs() < 1e-18);
    }
    assert!((result["energyJ"][0].as_f64().unwrap() - 2e-6).abs() < 1e-18);
}
#[test]
fn independent_floating_components_and_rotated_certificate_basis() {
    let mut m = cell(0.);
    m.potential_nodes = vec!["p".into(), "n".into(), "p2".into(), "n2".into()];
    m.current_dofs.push("j2".into());
    let old = m.capacitance_f.clone();
    m.capacitance_f
        .extend(old.iter().map(|&(i, j, v)| (i + 2, j + 2, v)));
    m.incidence.extend([(2, 1, 1.), (3, 1, -1.)]);
    m.resistance_ohm.push((1, 1, 50.));
    m.port_incidence.extend([(2, 1, 1.), (3, 1, -1.)]);
    m.port_terminals = None;
    m.conserved_potential_modes = vec![vec![1., 1., 0., 0.], vec![0., 0., 1., 1.]];
    m.structural_gauge_modes = m.conserved_potential_modes.clone();
    let first = coupled::impedance(&m, 2, &[0., 1e3, 1e5], None).unwrap();
    m.structural_gauge_modes = vec![vec![1., 1., 1., 1.], vec![1e-200, 1e-200, -1e-200, -1e-200]];
    let rotated = coupled::impedance(&m, 2, &[0., 1e3, 1e5], None).unwrap();
    for f in 0..3 {
        for i in 0..2 {
            for j in 0..2 {
                for field in ["real", "imaginary"] {
                    assert!(
                        (first["parameters"][f][i][j][field].as_f64().unwrap()
                            - rotated["parameters"][f][i][j][field].as_f64().unwrap())
                        .abs()
                            < 1e-9
                    );
                }
            }
        }
    }
}
#[test]
fn physical_ground_weld_removes_submodel_gauge() {
    let ground = json!({"id":"ground","kind":"network","ports":["join","probe"],"data":{"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["g"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.],[0,1,1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,1e-6]],"portTerminals":[{"positive":{"node":"g"},"negative":{"reference":"ground"}},{"positive":{"node":"g"},"negative":{"reference":"ground"}}]}});
    let top = json!({"instances":[{"id":"a","model":composition_resource(&cell(0.)),"ports":["join","probe"]},{"id":"b","model":ground,"ports":["join","probe"]}],"connections":[[0,2]],"ports":[{"name":"a","index":1},{"name":"b","index":3}]});
    let built =
        composition::build(&serde_json::from_value(top).unwrap(), None, Instant::now()).unwrap();
    assert!(built.model.structural_gauge_modes.is_empty());
    assert_eq!(built.model.reference_topology, "grounded");
    coupled::impedance(&built.model, 2, &[0., 1e3], None).unwrap();
}
#[test]
fn structural_gauge_keeps_nonzero_initial_inductor_current_and_rlc_energy() {
    let mut m = cell(0.);
    m.inductance_h = vec![(0, 0, 1e-3)];
    let q = json!({"resource":resource(&m),"drivers":[],"receivers":[],"time":{"stopS":1e-4,"sampleStepS":1e-6},"initial":{"kind":"physical-state","steady":false,"potentialsV":[{"instanceId":"cell","node":"p","value":4.},{"instanceId":"cell","node":"n","value":2.}],"currentsA":[{"instanceId":"cell","dof":"j","value":0.01}]},"numerics":{"relativeTolerance":1e-6,"absoluteTolerance":1e-10,"maxStepS":1e-7},"budget":{"maxSeconds":120.,"maxMemoryMiB":512.}});
    let r = time::solve(serde_json::from_value(q).unwrap()).unwrap();
    let r1 = (-1e5 + (1e10_f64 - 4e9).sqrt()) / 2.;
    let r2 = (-1e5 - (1e10_f64 - 4e9).sqrt()) / 2.;
    let a = (-1e4 - 2. * r2) / (r1 - r2);
    let b = 2. - a;
    for (k, t) in r["axis"].as_array().unwrap().iter().enumerate() {
        let t = t.as_f64().unwrap();
        let v = a * (r1 * t).exp() + b * (r2 * t).exp();
        let current = -1e-6 * (a * r1 * (r1 * t).exp() + b * r2 * (r2 * t).exp());
        assert!((r["portVoltages"][k][0].as_f64().unwrap() - v).abs() < 5e-4);
        let energy = 0.5e-6 * v * v + 0.5e-3 * current * current;
        assert!((r["energyJ"][k].as_f64().unwrap() - energy).abs() < 1e-9);
    }
    assert!((r["energyJ"][0].as_f64().unwrap() - 2.05e-6).abs() < 1e-18);
    let at = 1e-4;
    let expected = -1e-6 * (a * r1 * (r1 * at).exp() + b * r2 * (r2 * at).exp());
    assert!((r["finalState"]["x"][2].as_f64().unwrap() - expected).abs() < 5e-6);
}
#[test]
fn physical_port_capacitance_and_resistor_update_structural_subspace() {
    let mut m = cell(0.);
    m.potential_nodes.extend(["p2".into(), "n2".into()]);
    m.current_dofs.push("j2".into());
    let c = m.capacitance_f.clone();
    m.capacitance_f
        .extend(c.iter().map(|&(i, j, v)| (i + 2, j + 2, v)));
    m.incidence.extend([(2, 1, 1.), (3, 1, -1.)]);
    m.resistance_ohm.push((1, 1, 100.));
    m.port_incidence = vec![(0, 0, 1.), (2, 0, -1.)];
    m.port_terminals = None;
    m.conserved_potential_modes = vec![vec![1., 1., 0., 0.], vec![0., 0., 1., 1.]];
    m.structural_gauge_modes = m.conserved_potential_modes.clone();
    let mut bare = System::compile(&m, 1, None).unwrap();
    let mut rhs = bare.port_rhs(&[1.]);
    rhs.resize(12, 0.);
    assert!(bare
        .solve(1e3, rhs, &[0.; 6], &mut Counters::default())
        .unwrap_err()
        .contains("COUPLED_GAUGE_INCOMPATIBLE"));
    let mut tiny_incidence = m.clone();
    for (_, _, b) in &mut tiny_incidence.port_incidence {
        *b *= 1e-200;
    }
    let mut underflow = System::compile(&tiny_incidence, 1, None).unwrap();
    assert!(underflow
        .add_port_admittance(0, 0., 1e-6)
        .unwrap_err()
        .contains("COUPLED_LOAD_RANGE"));
    let mut unresolved = System::compile(&m, 1, None).unwrap();
    assert!(unresolved
        .add_port_admittance(0, 0., 1e-24)
        .unwrap_err()
        .contains("COUPLED_LOAD_RANGE"));
    for cap in [1e-6, 1e-18] {
        let mut s = System::compile(&m, 1, None).unwrap();
        s.add_port_admittance(0, 0., cap).unwrap();
        let state = s
            .solve(
                0.,
                vec![0.; 6],
                &[3., 3., -3., -3., 0., 0.],
                &mut Counters::default(),
            )
            .unwrap();
        for i in 0..4 {
            assert!((state[i] - if i < 2 { 3. } else { -3. }).abs() < 1e-3);
        }
        assert!((s.energy(&state) - 18. * cap).abs() < 1e-20 + 1e-3 * 18. * cap);
    }
    let mut s = System::compile(&m, 1, None).unwrap();
    s.add_port_admittance(0, 0.01, 1e-6).unwrap();
    let mut rhs = s.port_rhs(&[1.]);
    rhs.resize(12, 0.);
    let x = s
        .solve(1e3, rhs, &[0.; 6], &mut Counters::default())
        .unwrap();
    let re = s.port_voltages(&x, 1)[0];
    let im = s.port_voltages(&x[6..], 1)[0];
    let denom = 0.01f64.powi(2) + 0.001f64.powi(2);
    assert!((re - 0.01 / denom).abs() < 1e-9);
    assert!((im + 0.001 / denom).abs() < 1e-9);
}
