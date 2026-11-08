use super::*;
use std::time::Duration;

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
