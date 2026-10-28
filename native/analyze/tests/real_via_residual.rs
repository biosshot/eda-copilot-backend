//! The saved actual-board opposite-drive refusal, not a substitute circuit.
use pcb_analyze::{import_json,extraction::local::{board_mesh,volume_current,volume_field,volume_magnetic,volume_solve,AllocationBudget}};
use std::time::{Duration,Instant};

#[test]
#[ignore = "actual ROI sparse factor; run explicitly in release under 2 GiB/300 s"]
fn stm32_opposite_drive_1khz_preserves_the_original_kvl_guard() {
    pcb_analyze::extraction::local::parallel::with_threads(1,actual_case).unwrap();
}
fn actual_case() {
    let started=Instant::now();
    let remaining=||Duration::from_secs(300).checked_sub(started.elapsed()).expect("shared test deadline");
    let board=import_json(include_str!("../../../tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json")).board.unwrap();
    let request:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
    let budget=AllocationBudget{max_bytes:2*1024*1024*1024,max_nodes:100000,max_tetrahedra:262144};
    let geometry=board_mesh::build(&board,&request,budget,remaining()).unwrap();
    let electrodes=request.electrodes.iter().map(|e|geometry.electrode_nodes(e).unwrap()).collect::<Vec<_>>();
    let space=volume_current::assemble(&geometry.mesh,&geometry.conductivity_s_m,&electrodes,budget).unwrap();
    let field=volume_field::Field::new(&geometry.mesh,&space,budget).unwrap();
    let magnetic=volume_magnetic::Operator::new_enriched_patch(&geometry.mesh,budget,16,6,8,3).unwrap();
    let solver=volume_solve::Solver::new(&space,&field,&magnetic,budget).unwrap();
    let mut injection=vec![0.;space.potential_count()];
    for (port,drive) in request.ports.iter().zip([1.,-1.]) {
        for (name,sign) in port.iter().zip([1.,-1.]) {
            let index=request.electrodes.iter().position(|e|e.name==*name).unwrap();
            injection[space.node_potential[&electrodes[index][0]]]+=drive*sign;
        }
    }
    let full=solver.solve(1000.,&injection,&vec![0.;injection.len()],remaining()).unwrap();
    assert!(full.audit.normalized_kvl<=1e-8);
    assert!(full.audit.normalized_kcl<=1e-8);
    assert!(full.audit.relative_complex_power_error<=1e-8);
    eprintln!("actual opposite-drive regression {:.3}s {:?}",started.elapsed().as_secs_f64(),full.audit);
}
