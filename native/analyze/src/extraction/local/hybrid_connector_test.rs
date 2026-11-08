//! Finite pad-to-narrow-track joint hybrid control; no physical accuracy claim.
use super::*;
#[test]
fn finite_wide_pad_to_narrow_track_two_drive_hybrid() {
    let cases:serde_json::Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/orientation-aligned-pads.json")).unwrap();
    let mut board_json:serde_json::Value=serde_json::from_str(cases[0][0].as_str().unwrap()).unwrap();
    for track in board_json["copper"]["fixed"]["tracks"].as_array_mut().unwrap() {track["widthMm"]=serde_json::json!(0.4);}
    assert!(board_json["pads"].as_array().unwrap().iter().all(|p|p["shape"]["widthMm"]==0.8));
    let board=crate::import_json(&board_json.to_string()).board.unwrap();
    let request:board_mesh::Request=serde_json::from_value(cases[0][1].clone()).unwrap();
    let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000};
    let started=Instant::now();let wall=Duration::from_secs(25);
    let operator=CheckedOperator::HybridPlanarV3;
    // Mirror board_foil's AutomaticHybridV1 physical-thickness screen.
    let profile=operator.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap()
        .map(|p|p.with_thickness_screen(request.stack.iter().filter(|l|l.copper).filter_map(|l|l.conductivity_s_m)).unwrap());
    let mesh=board_mesh::build_profiled(&board,&request,budget,wall,None,&[],profile).unwrap();
    for electrode in &request.electrodes {assert!(mesh.electrode_areas_m2[&electrode.name]>0.);}
    let grid=CheckedGrid{frequencies_hz:&[1e6],reference_ohm:&[50.,50.],max_iterations:16};
    let result=solve_checked_mesh_impl_policy(&mesh,&request,grid,profile,&[],None,PartitionPolicy::AutomaticHybridV1,operator,budget,started,wall).unwrap();
    assert_eq!(result["coverage"],"unverified");
    assert_eq!(result["ports"],serde_json::json!(request.ports));
    let columns=result["samples"][0]["columns"].as_array().unwrap();assert_eq!(columns.len(),2);
    for (input,column) in columns.iter().enumerate() {
        assert!(column["kclRelative"].as_f64().unwrap()<=1e-8);
        assert!(column["kvlRelative"].as_f64().unwrap()<=1e-8);
        let copper=column["copperLossW"].as_f64().unwrap();assert!(copper>0.);
        let mut wave_power=12.5;
        for output in 0..2 {let s=&result["parameters"][0][output][input];let r=s["real"].as_f64().unwrap();let i=s["imaginary"].as_f64().unwrap();wave_power-=12.5*(r*r+i*i);}
        let error=(wave_power-copper).abs();
        assert!(error<=1e-12+1e-8*(wave_power.abs()+copper.abs()),"power balance error={error:e}");
    }
    let transfer=&result["parameters"][0][1][0];
    assert!(transfer["real"].as_f64().unwrap().hypot(transfer["imaginary"].as_f64().unwrap())>1e-12);
    let mut invalid=request.clone();invalid.electrodes[0].source_path=Some("pads[missing]".into());
    assert!(board_mesh::build(&board,&invalid,budget,wall.saturating_sub(started.elapsed())).is_err());
    assert!(started.elapsed()<wall);
}
