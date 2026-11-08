//! Cross-regime consistency of one discrete coupled model; no physical accuracy claim.
use super::{error, AllocationBudget, Result};
use crate::electrical::{Budget, Counters, Numerics, Time, Waveform};
use crate::electrical::coupled::{Model, System, time::{self, Driver, Receiver, Request}};
use crate::electrical::frequency::Resource;
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::{Duration, Instant}};

fn remaining(start: Instant, wall: Duration, budget: AllocationBudget) -> Result<Budget> {
    let seconds = wall.checked_sub(start.elapsed()).ok_or_else(|| error("DEADLINE", "regime audit deadline exceeded"))?.as_secs_f64();
    if seconds <= 0. { return Err(error("DEADLINE", "regime audit deadline exceeded")); }
    Ok(Budget { max_seconds: Some(seconds), max_memory_mi_b: Some(budget.max_bytes as f64 / 1048576.) })
}

pub(super) fn audit(model: &Model, port_names: &[String], references: &[f64], probe_hz: f64, upper_hz: f64, budget: AllocationBudget, start: Instant, wall: Duration) -> Result<Value> {
    let ports = port_names.len();
    if ports == 0 || ports > 16 || references.len() != ports || port_names.iter().any(|s| s.trim().is_empty()) || port_names.iter().collect::<BTreeSet<_>>().len() != ports || references.iter().any(|r| !r.is_finite() || *r <= 0.) || !probe_hz.is_finite() || probe_hz <= 0. || !upper_hz.is_finite() || probe_hz > upper_hz / 20. {
        return Err(error("REGIME_BINDING", "invalid matched ports, references, or temporal probe band"));
    }
    let n = model.potential_nodes.len().checked_add(model.current_dofs.len()).ok_or_else(|| error("MEMORY_LIMIT", "state size overflow"))?;
    // Reserve the retained bounded response and physical-state vectors before allocating them.
    let reserve = ports.checked_mul(ports).and_then(|v| v.checked_mul(256)).and_then(|v| n.checked_mul(256).and_then(|x| v.checked_add(x))).ok_or_else(|| error("MEMORY_LIMIT", "audit output plan overflow"))?;
    budget.check(reserve)?;
    let available = AllocationBudget { max_bytes: budget.max_bytes - reserve, ..budget };
    let mut dc = Vec::with_capacity(ports);
    let mut ac = Vec::with_capacity(ports);
    let mut diagnostics = Vec::with_capacity(ports);
    let omega = std::f64::consts::TAU * probe_hz;
    let period = 1. / probe_hz;
    let mut passed = true;
    for drive in 0..ports {
        remaining(start, wall, available)?;
        let mut system = System::compile(model, ports, Some(&remaining(start, wall, available)?))?;
        for (i, r) in references.iter().enumerate() { system.add_port_admittance(i, 1. / r, 0.)?; }
        let mut excitation = vec![0.; ports]; excitation[drive] = 1.;
        let rhs = system.port_rhs(&excitation);
        let mut stats = Counters::default();
        eprintln!("P7_REGIME_STAGE drive={drive} phase=dc-start elapsed={:.6}",start.elapsed().as_secs_f64());
        let d = system.solve(0., rhs.clone(), &vec![0.; n], &mut stats).map_err(|e| format!("REGIME_dc: {e}"))?;
        eprintln!("P7_REGIME_STAGE drive={drive} phase=dc-end elapsed={:.6}",start.elapsed().as_secs_f64());
        dc.push(system.port_voltages(&d, ports));
        let mut complex_rhs = rhs; complex_rhs.resize(2 * n, 0.);
        eprintln!("P7_REGIME_STAGE drive={drive} phase=ac-start elapsed={:.6}",start.elapsed().as_secs_f64());
        let state = system.solve(omega, complex_rhs, &vec![0.; n], &mut stats).map_err(|e| format!("REGIME_ac: {e}"))?;
        eprintln!("P7_REGIME_STAGE drive={drive} phase=ac-end elapsed={:.6}",start.elapsed().as_secs_f64());
        let re = system.port_voltages(&state[..n], ports);
        let im = system.port_voltages(&state[n..], ports);
        drop(system);
        ac.push(json!({"real":re,"imaginary":im}));
        let initial = json!({"kind":"physical-state","steady":false,
            "potentialsV":model.potential_nodes.iter().enumerate().map(|(i,node)| json!({"instanceId":"regime-model","node":node,"value":std::f64::consts::SQRT_2*state[i]})).collect::<Vec<_>>(),
            "currentsA":model.current_dofs.iter().enumerate().map(|(i,dof)| json!({"instanceId":"regime-model","dof":dof,"value":std::f64::consts::SQRT_2*state[model.potential_nodes.len()+i]})).collect::<Vec<_>>()});
        let mut errors = Vec::new();
        let mut qualities = Vec::new();
        for steps in [128., 256.] {
            let resource = Resource { id:"regime-model".into(), kind:"network".into(), ports:port_names.to_vec(), data:serde_json::to_value(model).map_err(|e| error("REGIME_MODEL", &e.to_string()))?, source:None, validity:Some(json!({"status":"conditional","reason":"Discrete model consistency control only; no physical band acceptance","domain":{"bandHz":[0.,upper_hz]}})), analyses:None };
            let request = Request { resource:Some(resource), topology:None,
                drivers:vec![Driver {port:drive, resistance_ohm:references[drive], waveform:Waveform::Sine {offset:0., amplitude:std::f64::consts::SQRT_2*references[drive], period, phase:90.}}],
                receivers:(0..ports).filter(|&i| i != drive).map(|i| Receiver {port:i, resistance_ohm:Some(references[i]), capacitance_f:0.}).collect(),
                time:Time {stop_s:period, sample_step_s:period/32.}, initial:Some(initial.clone()),
                numerics:Some(Numerics {relative_tolerance:Some(1e-5), absolute_tolerance:Some(1e-8), max_iterations:None, max_step_s:Some(period/steps)}), budget:Some(remaining(start, wall, available)?) };
            eprintln!("P7_REGIME_STAGE drive={drive} steps={steps} phase=time-start elapsed={:.6}",start.elapsed().as_secs_f64());
            let result = time::solve(request)?;
            eprintln!("P7_REGIME_STAGE drive={drive} steps={steps} phase=time-end elapsed={:.6}",start.elapsed().as_secs_f64());
            remaining(start, wall, available)?;
            let axis = result["axis"].as_array().ok_or_else(|| error("REGIME_TIME", "missing time axis"))?;
            let voltages = result["portVoltages"].as_array().ok_or_else(|| error("REGIME_TIME", "missing time voltages"))?;
            if axis.len() != voltages.len() { return Err(error("REGIME_TIME", "inconsistent time output")); }
            let scale = re.iter().zip(&im).map(|(a,b)| a.hypot(*b)*std::f64::consts::SQRT_2).fold(0., f64::max);
            let mut max_error: f64 = 0.;
            for (t, row) in axis.iter().zip(voltages) {
                let t = t.as_f64().ok_or_else(|| error("REGIME_TIME", "invalid time"))?;
                let row = row.as_array().filter(|v| v.len()==ports).ok_or_else(|| error("REGIME_TIME", "invalid port row"))?;
                for i in 0..ports {
                    let actual = row[i].as_f64().ok_or_else(|| error("REGIME_TIME", "invalid voltage"))?;
                    let expected = std::f64::consts::SQRT_2*(re[i]*(omega*t).cos()-im[i]*(omega*t).sin());
                    max_error = max_error.max((actual-expected).abs());
                }
            }
            errors.push(max_error / scale.max(1e-12));
            qualities.push(result["quality"].clone());
        }
        passed &= errors[1] <= 0.02 && errors[1] <= errors[0] * 1.25 + 1e-5;
        diagnostics.push(json!({"relativePeakErrors":errors,"timeQuality":qualities}));
    }
    Ok(json!({"status":if passed {"pass"} else {"fail"},"scope":"discrete-model-only","physicalCoverage":"unverified","probeHz":probe_hz,"dcPortVoltagesV":dc,"acPortVoltagesV":ac,"timeDiagnostics":diagnostics}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Model {
        serde_json::from_value(json!({"format":"coupled-rlc","referenceTopology":"grounded","potentialNodes":["a","b"],"currentDofs":["i1","i2"],"incidence":[[0,0,1.],[1,1,1.]],"portIncidence":[[0,0,1.],[1,1,1.]],"resistanceOhm":[[0,0,0.02],[1,1,0.03]],"inductanceH":[[0,0,8e-9],[0,1,2e-9],[1,0,2e-9],[1,1,12e-9]],"capacitanceF":[[0,0,200e-12],[0,1,-40e-12],[1,0,-40e-12],[1,1,160e-12]]})).unwrap()
    }
    fn budget(bytes:usize) -> AllocationBudget { AllocationBudget {max_bytes:bytes,max_nodes:100,max_tetrahedra:100} }
    #[test]
    fn passive_rlc_matched_periodic_overlap() {
        let v = audit(&model(), &["a".into(),"b".into()], &[50.,50.], 1e5, 2e6, budget  (64*1024*1024), Instant::now(), Duration::from_secs(25)).unwrap();
        assert_eq!(v["status"], "pass");
        assert_eq!(v["physicalCoverage"], "unverified");
        let dc = v["dcPortVoltagesV"][0][0].as_f64().unwrap();
        assert!((dc - 50.*0.02/(50.+0.02)).abs() < 1e-8);
        assert_eq!(v["timeDiagnostics"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn rejects_invalid_bindings_and_budget() {
        let names = vec!["a".into(), "b".into()];
        assert!(audit(&model(), &names, &[50.,0.], 1e5, 2e6, budget(1<<20), Instant::now(), Duration::from_secs(1)).is_err());
        assert!(audit(&model(), &names, &[50.,50.], 1e5, 1e6, budget(1<<20), Instant::now(), Duration::from_secs(1)).is_err());
        assert!(audit(&model(), &names, &[50.,50.], 1e5, 2e6, budget(1), Instant::now(), Duration::from_secs(1)).is_err());
    }
    #[test]
    fn welded_floating_resistor_preserves_common_charge() {
        let copper = json!({"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["a","b"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.],[1,0,-1.],[1,1,1.],[0,1,-1.],[0,2,1.],[1,2,-1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,4.071261140951303e-14],[0,1,-1.6101666716681695e-15],[1,0,-1.6101666716681695e-15],[1,1,5.883475744607333e-14]],"portTerminals":[{"positive":{"node":"a"},"negative":{"node":"b"}},{"positive":{"node":"b"},"negative":{"node":"a"}},{"positive":{"node":"a"},"negative":{"node":"b"}}]});
        let package = json!({"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["p","n"],"currentDofs":["series"],"incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.]],"resistanceOhm":[[0,0,25.]],"inductanceH":[],"capacitanceF":[],"structuralGaugeModes":[[1.,1.]],"portTerminals":[{"positive":{"node":"p"},"negative":{"node":"n"}}]});
        let topology: crate::electrical::coupled::composition::Topology = serde_json::from_value(json!({"instances":[{"id":"copper","ports":["a","b","private"],"model":{"id":"copper","kind":"network","ports":["a","b","private"],"data":copper}},{"id":"bridge","ports":["terminals"],"model":{"id":"bridge","kind":"network","ports":["terminals"],"data":package}}],"connections":[[2,3]],"connectionContacts":["both"],"ports":[{"name":"a","index":0},{"name":"b","index":1}]})).unwrap();
        let m = crate::electrical::coupled::composition::build(&topology, None, Instant::now()).unwrap().model;
        assert_eq!(m.conserved_potential_modes.len(), 1);
        assert!(m.structural_gauge_modes.is_empty());
        let mut system = System::compile(&m, 2, None).unwrap();
        for p in 0..2 { system.add_port_admittance(p, 0.02, 0.).unwrap(); }
        let rhs = system.port_rhs(&[1., 0.]);
        let zero = system.solve(0., rhs.clone(), &[0.,0.,0.], &mut Counters::default()).unwrap();
        let shifted = system.solve(0., rhs, &[1.,1.,0.], &mut Counters::default()).unwrap();
        let a = system.port_voltages(&zero, 2);
        let b = system.port_voltages(&shifted, 2);
        assert!((a[0]-12.5).abs() < 1e-9);
        assert!((a[0]-b[0]).abs() < 1e-9);
        assert!((shifted[0]-zero[0]-1.).abs() < 1e-9);
        assert!((shifted[1]-zero[1]-1.).abs() < 1e-9);
        let report = audit(&m, &["a".into(),"b".into()], &[50.,50.], 50., 1000., budget(64*1024*1024), Instant::now(), Duration::from_secs(25)).unwrap();
        assert_eq!(report["status"], "pass");
    }
}


