use super::*;
use std::time::Duration;

#[test]
fn prepared_spatial_action_matches_capacitance_and_preserves_gauge() {
    let (mut input, trace)=coupon();
    input.z_m.insert(2,0.00055);
    input.planes=vec![0;input.z_m.len()];
    let reference=extract_traced_spatial(&input,&trace,&[region()],budget()).unwrap();
    let action=prepare_traced_spatial_action(&input,&trace,&[region()],budget()).unwrap();
    assert!(action.solution.plan.free_nodes>0);
    assert!(action.solution.capacitance_f.is_empty());
    assert!(action.solution.basis_potentials_v.is_empty());
    assert_eq!(action.max_epsilon_r(),4.);
    let scale=reference.capacitance_f.iter().flatten().map(|v|v.abs()).fold(0_f64,f64::max);
    for v in [[1.,0.],[0.,1.],[1.,-2.],[1.,1.]] {
        let q=action.apply(&v).unwrap();
        for i in 0..2 {
            let expected=reference.capacitance_f[i].iter().zip(v).map(|(c,v)|c*v).sum::<f64>();
            assert!((q[i]-expected).abs()<1e-9*scale);
        }
    }
    assert_eq!(action.apply(&[0.,0.]).unwrap(),vec![0.,0.]);
    assert!(action.apply(&[1.]).is_err());
    assert!(action.apply(&[f64::NAN,0.]).is_err());
    assert_eq!(action.action_count(),5);
    assert!(prepare_traced_spatial_action(&input,&trace,&[region()],Budget{max_wall:Duration::ZERO,..budget()}).is_err());
}

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

#[test]
fn prepared_cut_flux_matches_parallel_plate_field_and_rejects_invalid_cuts() {
    let (input, trace) = coupon();
    let action = prepare_traced_spatial_action(&input, &trace, &[region()], budget()).unwrap();
    let bounds = [[0.0002, 0.0002, 0.0002], [0.0008, 0.0008, 0.0009]];
    let flux = action
        .cut_displacement_flux(&[0., 1.], &[0., 2.], bounds)
        .unwrap();
    let externally_supervised = Budget {
        max_nodes: usize::MAX,
        max_local_terms: usize::MAX,
        max_bytes: usize::MAX,
        max_wall: Duration::MAX,
    };
    let unbounded = prepare_traced_spatial_action(&input, &trace, &[region()], externally_supervised).unwrap();
    assert_eq!(unbounded.cut_displacement_flux(&[0., 1.], &[0., 2.], bounds).unwrap(), flux);
    let expected = EPS0 * 4. * 0.0006_f64.powi(2) * 5_f64.sqrt() / 0.0009;
    for &f in &flux[..4] {
        assert!(f < expected * 1e-10);
    }
    for &f in &flux[4..] {
        assert!((f / expected - 1.).abs() < 1e-10, "{flux:?}");
    }
    let shifted = action
        .cut_displacement_flux(&[7., 8.], &[-3., -1.], bounds)
        .unwrap();
    for i in 0..6 {
        assert!((shifted[i] - flux[i]).abs() < expected * 1e-10);
    }
    assert_eq!(
        action
            .cut_displacement_flux(&[0., 0.], &[0., 0.], bounds)
            .unwrap(),
        [0.; 6]
    );
    assert!(action
        .cut_displacement_flux(&[0., 1.], &[0., 0.], [[0., 0.0002, 0.0002], bounds[1]])
        .is_err());
    let copper_bounds = [[0.0002, 0.0002, 0.00005], bounds[1]];
    let copper_flux = action
        .cut_displacement_flux(&[0., 1.], &[0., 2.], copper_bounds)
        .unwrap();
    assert_eq!(copper_flux[4], 0.);
    assert!((copper_flux[5] / expected - 1.).abs() < 1e-10);
    let mut missing = prepare_traced_spatial_action(&input, &trace, &[region()], budget()).unwrap();
    missing.solution.copper_cells.clear();
    assert!(missing
        .cut_displacement_flux(&[0., 1.], &[0., 0.], copper_bounds)
        .is_err());
    // Manufactured linear nodal field: real gradient (2,3,0), imaginary (4,0,0).
    let mut real = Vec::new();
    let mut imag = Vec::new();
    for &plane in &action.solution.planes {
        for p in &action.solution.meshes[plane].xy {
            real.push(2. * p[0] + 3. * p[1]);
            imag.push(4. * p[0]);
        }
    }
    let xy_flux = action.cut_nodal_flux(&real, &imag, bounds).unwrap();
    let vertical_area = 0.0006 * 0.0007;
    let x_expected = EPS0 * 4. * vertical_area * 20_f64.sqrt();
    let y_expected = EPS0 * 4. * vertical_area * 3.;
    for &v in &xy_flux[..2] {
        assert!((v / x_expected - 1.).abs() < 1e-10, "{xy_flux:?}");
    }
    for &v in &xy_flux[2..4] {
        assert!((v / y_expected - 1.).abs() < 1e-10, "{xy_flux:?}");
    }
    for &v in &xy_flux[4..] {
        assert!(v < x_expected * 1e-10);
    }
    let mut limited = prepare_traced_spatial_action(&input, &trace, &[region()], budget()).unwrap();
    limited.solution.budget.max_bytes = limited.solution.plan.planned_simultaneous_bytes;
    assert!(limited
        .cut_displacement_flux(&[0., 1.], &[0., 0.], bounds)
        .is_err());
}
