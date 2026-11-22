use pcb_analyze::extraction::local::{board_mesh,board_prepare};
#[test]
fn explicit_verification_holdouts_are_not_replaced_with_the_legacy_axis(){
 let requested=[1.,10.,100.,316.22776601683796,1000.];assert_eq!(board_prepare::verification_axis(&requested).unwrap(),&requested);assert_eq!(board_prepare::verification_axis(&[]).unwrap(),&board_prepare::FREQUENCIES_HZ);for invalid in [vec![0.],vec![100.,10.],vec![1.,1.],vec![f64::NAN]]{assert!(board_prepare::verification_axis(&invalid).is_err());}
}
#[test]
fn accepted_electric_defaults_match_frozen_candidate_and_preserve_physical_inputs() {
    let current:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
    let expected:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/field-refinement-request-v1.json")).unwrap();
    let actual=board_prepare::accepted_electric_request(&current).unwrap();
    assert_eq!(serde_json::to_value(&actual).unwrap(),serde_json::to_value(&expected).unwrap());
    assert_eq!(board_mesh::physical_signature(&current),board_mesh::physical_signature(&actual));
    assert_ne!(current.air_margin_m,actual.air_margin_m);
    assert_eq!(current.max_vertical_step_m,0.002);
}

#[test]
fn electric_refinement_retains_finer_current_vertical_boundaries(){
 let mut current:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();current.max_vertical_step_m=0.00153/3.9;
 let field=board_prepare::accepted_electric_request(&current).unwrap();assert_eq!(field.max_vertical_step_m,current.max_vertical_step_m);assert_eq!(board_mesh::physical_signature(&current),board_mesh::physical_signature(&field));
 current.electric_air_margin_m=Some(0.016);let expanded=board_prepare::accepted_electric_request(&current).unwrap();assert_eq!(expanded.air_margin_m,0.016);assert_eq!(expanded.roi_mm,current.roi_mm);assert_eq!(board_mesh::physical_signature(&current),board_mesh::physical_signature(&expanded));
}
#[test]
fn automatic_electric_near_air_tracks_resolution_and_preserves_current_request() {
    let mut current:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
    current.roi_mm=[[0.,-1.01],[0.8,1.01]];
    for (layer,thickness) in current.stack.iter_mut().zip([1e-5,1e-4,1e-5]) {
        layer.thickness_m=thickness;
    }
    current.max_triangle_area_m2=5e-8;
    current.air_edge_divisions=Some(1);
    current.electric_air_margin_m=Some(0.008);
    let original=serde_json::to_value(&current).unwrap();
    let field=board_prepare::accepted_electric_request(&current).unwrap();
    assert_eq!(serde_json::to_value(&current).unwrap(),original);
    assert_eq!(board_mesh::physical_signature(&field),board_mesh::physical_signature(&current));
    assert_eq!(field.air_first_step_m,Some(2.5e-5));
    assert!((field.air_near_distance_m.unwrap()-4.8e-4).abs()<1e-18);
    assert_eq!(field.air_edge_divisions,Some(14));
    assert_eq!(field.air_far_edge_divisions,Some(1));

    current.electric_air_margin_m=Some(0.032);
    let wider=board_prepare::accepted_electric_request(&current).unwrap();
    assert_eq!(wider.air_margin_m,0.032);
    assert_eq!(wider.air_first_step_m,field.air_first_step_m);
    assert_eq!(wider.air_near_distance_m,field.air_near_distance_m);
    assert_eq!(wider.air_edge_divisions,field.air_edge_divisions);
    assert_eq!(wider.air_far_edge_divisions,field.air_far_edge_divisions);

    current.max_triangle_area_m2*=0.25;
    let refined=board_prepare::accepted_electric_request(&current).unwrap();
    assert_eq!(refined.air_edge_divisions,Some(27));
}

#[test]
fn automatic_electric_grading_preserves_finer_controls_and_checks_count_conversion() {
    let mut current:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
    current.electric_air_margin_m=Some(0.016);
    current.air_first_step_m=Some(5e-6);
    current.air_edge_divisions=Some(24);
    current.air_margin_m=0.01;
    current.air_near_distance_m=Some(0.008);
    current.air_far_edge_divisions=Some(2);
    current.interface_step_m=Some(1e-5);
    current.interface_growth=Some(1.5);
    let field=board_prepare::accepted_electric_request(&current).unwrap();
    assert_eq!(field.air_first_step_m,current.air_first_step_m);
    assert_eq!(field.air_edge_divisions,current.air_edge_divisions);
    assert_eq!(field.air_near_distance_m,current.air_near_distance_m);
    assert_eq!(field.air_far_edge_divisions,current.air_far_edge_divisions);
    assert_eq!(field.interface_step_m,current.interface_step_m);
    assert_eq!(field.interface_growth,current.interface_growth);

    current.roi_mm[1][0]+=32.;
    let wide=board_prepare::accepted_electric_request(&current).unwrap();
    assert_eq!(wide.air_edge_divisions,Some(49));

    current.roi_mm[1][0]=f64::MAX;
    let error=board_prepare::accepted_electric_request(&current).unwrap_err();
    assert!(error.starts_with("LOCAL_ELECTRIC_GRADING:")&&error.contains("not representable"),"{error}");
}

#[test]
fn board_mesh_air_ring_divisions_are_bounded_by_allocation_budget() {
    let board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).board.unwrap();
    let mut request:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
    request.roi_mm=[[4.,-0.3],[6.,0.3]];
    for (layer,thickness) in request.stack.iter_mut().zip([1e-5,1e-4,1e-5]) {
        layer.thickness_m=thickness;
    }
    request.electrodes.clear();
    request.ports.clear();
    request.allow_insulated_copper_cuts=true;
    request.air_margin_m=2e-4;
    request.air_first_step_m=Some(5e-5);
    request.air_near_distance_m=Some(1e-4);
    request.air_edge_divisions=Some(48);
    request.air_far_edge_divisions=Some(40);
    let budget=board_prepare::allocation_budget(512);
    let wall=std::time::Duration::from_secs(5);

    let mesh=board_mesh::build(&board,&request,budget,wall).unwrap();
    assert!(mesh.mesh.copper_volume_m3>0.);

    request.air_edge_divisions=Some(budget.max_nodes);
    let error=board_mesh::build(&board,&request,budget,wall).err().unwrap();
    assert!(error.starts_with("LOCAL_MEMORY_LIMIT:")&&error.contains("graded air ring"),"{error}");

    request.air_edge_divisions=Some(48);
    request.air_far_edge_divisions=Some(usize::MAX);
    let error=board_mesh::build(&board,&request,budget,wall).err().unwrap();
    assert!(error.starts_with("LOCAL_MEMORY_LIMIT:")&&error.contains("graded air ring"),"{error}");
}

#[test]
fn absent_internal_cut_transport_preserves_the_historical_physical_signature(){
 let request:board_mesh::Request=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();let signature=board_mesh::physical_signature(&request);assert!(signature.get("cutPlanes").is_none());assert!(signature.get("electricMaxEdgeM").is_none());
}
#[test]
fn increased_explicit_developer_memory_admits_geometry_by_the_same_byte_plan(){
 let default=pcb_analyze::extraction::local::board_prepare::allocation_budget(2048);assert_eq!(default.max_nodes,100000);assert_eq!(default.max_tetrahedra,262144);
 let large=pcb_analyze::extraction::local::board_prepare::allocation_budget(3100);assert!(large.max_nodes>=139601);assert!(large.max_tetrahedra>=814320);assert!(large.check(814320*2048).is_ok());assert!(large.check(large.max_bytes+1).is_err());
}

#[test]
fn loaded_dc_global_voltage_gauge_is_anchor_invariant_with_unequal_conductances(){
 let conductance=vec![vec![3.9,-2.,-0.7,-1.2],vec![-2.,7.,-5.,0.],vec![-0.7,-5.,6.,-0.3],vec![-1.2,0.,-0.3,1.5]];
 let incidence=vec![vec![1.,0.],vec![-1.,0.],vec![0.,1.],vec![0.,-1.]];let drive=[0.8,-0.35];let loads=[50.,75.];
 for row in &conductance{assert!(row.iter().sum::<f64>().abs()<1e-12);}
 let (first,residual_first,norton_first)=board_prepare::anchored_loaded_dc(&conductance,&incidence,&drive,&loads,0).unwrap();
 let (second,residual_second,norton_second)=board_prepare::anchored_loaded_dc(&conductance,&incidence,&drive,&loads,3).unwrap();
 assert_eq!(first[0],0.);assert_eq!(second[3],0.);assert!(residual_first<1e-12&&residual_second<1e-12,"first={first:?} residual={residual_first:e} second={second:?} residual={residual_second:e}");
 let drive_scale=drive.iter().map(|v|v.abs()).fold(0.,f64::max);for norton in [&norton_first,&norton_second]{assert!(norton.iter().map(|v|v.abs()).fold(0.,f64::max)/drive_scale<1e-12);}
 let ports=|v:&[f64]|vec![v[0]-v[1],v[2]-v[3]];let a=ports(&first);let b=ports(&second);let scale=a.iter().map(|v|v.abs()).fold(0.,f64::max);assert!(a.iter().zip(&b).map(|(a,b)|(a-b).abs()).fold(0.,f64::max)/scale<1e-12);
 let current=|v:&[f64]|(0..4).map(|i|(0..4).map(|j|conductance[i][j]*v[j]).sum::<f64>()).collect::<Vec<_>>();let a=current(&first);let b=current(&second);assert!(a.iter().zip(&b).map(|(a,b)|(a-b).abs()).fold(0.,f64::max)/drive_scale<1e-12);
}
