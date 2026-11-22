//! Full-port/internal response packet; no board/via/extraction admission claim.
use pcb_analyze::{
    electrical::{
        coupled::{self, System},
        Counters,
    },
    extraction::local::{
        current_space, layered_capacitance,
        reduction::{automatic_currents, AutomaticOptions},
    },
};
use serde_json::json;
use std::time::{Duration, Instant};
#[path = "../../../tests/fixtures/analysis-local-extraction/current-space/fixture.rs"]
mod fixture;
fn main() {
    let mut runs = vec![];
    for name in ["cold", "same-process-repeat"] {
        let start = Instant::now();
        let (input, trace, foils) = fixture::quotient();
        let electric =
            layered_capacitance::extract_traced(&input, &trace, fixture::budget()).unwrap();
        let space = current_space::assemble(
            &foils,
            &electric,
            &[[0, 1], [2, 3]],
            1e5,
            current_space::Budget {
                max_bytes: 512 * 1024 * 1024,
                max_wall: Duration::from_secs(120),
            },
        )
        .unwrap();
        let automatic = automatic_currents(
            &space.model,
            2,
            AutomaticOptions {
                training_hz: fixture::FREQUENCIES_HZ.iter().step_by(2).copied().collect(),
                relative_tolerance: 1e-6,
                max_current_order: space.model.current_dofs.len(),
                max_bytes: 512 * 1024 * 1024,
                max_wall: Duration::from_secs(120),
            },
        );
        let mut rows = vec![];
        let (reduced_model, basis, audit, diagnostic) = match &automatic {
            Ok(a) => (
                json!(a.projection.model),
                json!(a.projection.basis),
                json!(a.audit),
                None,
            ),
            Err(e) => (json!(null), json!(null), json!(null), Some(e.clone())),
        };
        let mut full = System::compile(&space.model, 2, None).unwrap();
        let mut reduced = automatic
            .as_ref()
            .ok()
            .map(|a| System::compile(&a.projection.model, 2, None).unwrap());
        for &f in &fixture::FREQUENCIES_HZ {
            for (drive_name, drive) in [
                ("port0", [1., 0.]),
                ("port1", [0., 1.]),
                ("opposing", [1., -1.]),
            ] {
                let mut rhs = full.port_rhs(&drive);
                rhs.resize(24, 0.);
                let x = full
                    .solve(
                        2. * std::f64::consts::PI * f,
                        rhs,
                        &[0.; 12],
                        &mut Counters::default(),
                    )
                    .unwrap();
                let (back, rs) = if let (Some(s), Ok(a)) = (&mut reduced, &automatic) {
                    let nr = a.projection.basis.len();
                    let n = 4 + nr;
                    let mut rhs = s.port_rhs(&drive);
                    rhs.resize(2 * n, 0.);
                    let r = s
                        .solve(
                            2. * std::f64::consts::PI * f,
                            rhs,
                            &vec![0.; n],
                            &mut Counters::default(),
                        )
                        .unwrap();
                    let mut b = r[..4].to_vec();
                    b.extend(a.projection.recover_currents(&r[4..n]).unwrap());
                    b.extend_from_slice(&r[n..n + 4]);
                    b.extend(a.projection.recover_currents(&r[n + 4..]).unwrap());
                    (json!(b), json!(r))
                } else {
                    (json!(null), json!(null))
                };
                rows.push(json!({"frequencyHz":f,"drive":drive_name,"fullState":x,"recoveredState":back,"reducedState":rs}));
            }
        }
        runs.push(json!({"name":name,"fullPathSeconds":start.elapsed().as_secs_f64(),"input":input,"trace":trace,"foils":foils,
            "model":space.model,"reducedModel":reduced_model,"basis":basis,"audit":audit,"diagnostic":diagnostic,
            "fieldPlan":electric.plan,"currentPlanBytes":space.planned_bytes,"rows":rows,
            "fullZ":coupled::impedance(&space.model,2,&fixture::FREQUENCIES_HZ,None),
            "fullLoopZ":coupled::impedance(&fixture::loop_model(&space.model),1,&fixture::FREQUENCIES_HZ,None)}));
    }
    println!(
        "{}",
        json!({"status":"bounded-automatic-current-probe","p7Complete":false,"pcbExtractionAdmitted":false,
        "viaAdmitted":false,"runs":runs})
    );
}
