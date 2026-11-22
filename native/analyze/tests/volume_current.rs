use pcb_analyze::extraction::local::{mesh::{self,LayeredInput,Material},polyhedron::{PointPotential,positive_tetra_potential},volume_current,AllocationBudget};
fn budget()->AllocationBudget {AllocationBudget{max_bytes:32*1024*1024,max_nodes:10000,max_tetrahedra:10000}}
#[test]
fn thin_tetra_field_preserves_constant_and_independent_affine_energy() {
    use pcb_analyze::extraction::{EPS0,local::capacitance::{Boundary,TraceRow,CapacitanceOperator}};
    // Dyadic coordinates/voltages make the independent affine boundary values
    // exact F64 inputs, including the weak transverse derivative and offset.
    let mesh=mesh::extrude(&LayeredInput{xy_m:vec![[0.,0.],[2_f64.powi(-10),0.],[0.,2_f64.powi(-35)]],triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,2_f64.powi(-12)],materials:vec![vec![Material::Dielectric{label:"thin exact cell".into(),epsilon_r:4.2}]]},budget()).unwrap();
    let operator=CapacitanceOperator::new(&mesh,Boundary{potential_count:mesh.nodes_m.len(),metal_trace:(0..mesh.nodes_m.len()).map(|node|TraceRow{node,weights:vec![(node,1.)]}).collect(),far_zero_nodes:vec![],insulated_external:true},budget()).unwrap();
    let constant=operator.apply(&vec![3.;mesh.nodes_m.len()]).unwrap();
    assert_eq!(constant.energy_j,0.);
    assert!(constant.charges_c.iter().all(|q|*q==0.));
    let e=[32.,2_f64.powi(-5),-16.];
    let volts=mesh.nodes_m.iter().map(|p|3.+(0..3).map(|k|e[k]*p[k]).sum::<f64>()).collect::<Vec<f64>>();
    let field=operator.apply(&volts).unwrap();
    let expected=0.5*EPS0*4.2*mesh.dielectric_volume_m3*e.iter().map(|x|x*x).sum::<f64>();
    assert!((field.energy_j-expected).abs()/expected<1e-8);
    for f in field.electric_fields_v_per_m {for k in 0..3 {assert!((f[k]+e[k]).abs()<1e-8);}}
}
#[test]
fn thin_conductor_keeps_weak_transverse_current_and_adjoint_power() {
    let mesh=mesh::extrude(&LayeredInput{xy_m:vec![[0.,0.],[2_f64.powi(-10),0.],[0.,2_f64.powi(-35)]],triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,2_f64.powi(-12)],materials:vec![vec![Material::Copper{source_ids:vec!["thin physical conductor".into()]}]]},budget()).unwrap();
    let sigma=58e6;let e=[32.,2_f64.powi(-5),-16.];
    let space=volume_current::assemble(&mesh,&vec![sigma;mesh.copper_tetrahedra.len()],&[],budget()).unwrap();
    let volts=space.mesh_nodes.iter().map(|nodes|{let p=mesh.nodes_m[nodes[0]];3.+(0..3).map(|k|e[k]*p[k]).sum::<f64>()}).collect::<Vec<_>>();
    let mut currents=space.gradient(&volts).unwrap();
    for(i,c)in space.cells.iter().enumerate(){for k in 0..3 {currents[3*i+k]/=c.resistance_ohm;}}
    for j in space.density(&currents).unwrap() {for k in 0..3 {assert!((j[k]+sigma*e[k]).abs()/(sigma*e[k]).abs()<1e-8);}}
    let expected=sigma*mesh.copper_volume_m3*e.iter().map(|v|v*v).sum::<f64>();
    let loss=space.losses(&currents).unwrap().iter().sum::<f64>();
    assert!((loss-expected).abs()/expected<1e-9);
    let reaction=space.divergence(&currents).unwrap();
    let work=volts.iter().zip(&reaction).map(|(v,i)|v*i).sum::<f64>();
    assert!((work-loss).abs()/loss<1e-8);
    assert!(reaction.iter().sum::<f64>().abs()/reaction.iter().map(|x|x.abs()).sum::<f64>()<1e-12);
}
#[test]
fn tetra_source_matches_independent_positive_duffy_at_interior_boundary_and_exterior() {
    let tetra=[[0.,0.,0.],[1e-3,0.,0.],[0.,0.8e-3,0.],[0.,0.,0.7e-3]];
    let source=PointPotential::tetrahedron(tetra).unwrap();
    for point in [[0.2e-3,0.2e-3,0.1e-3],[0.,0.,0.],[0.3e-3,0.2e-3,0.],[2e-3,-1e-3,1e-3]] {
        let a=source.at(point).unwrap();
        let b=positive_tetra_potential(tetra,point,48).unwrap();
        assert!((a-b).abs()/b<1e-6,"point={point:?} analytic={a:e} independent={b:e}");
    }
}
#[test]
fn analytic_segment_source_matches_old_duffy_and_independent_prism_partition() {
    use pcb_analyze::extraction::local::{polyhedron::positive_tetra_potential_duffy,prism::Prism};
    let ordinary=[[0.,0.,0.],[1e-3,0.,0.],[0.,0.8e-3,0.],[0.,0.,0.7e-3]];
    for point in [[0.2e-3,0.2e-3,0.1e-3],[0.,0.,0.],[0.3e-3,0.2e-3,0.],[2e-3,-1e-3,1e-3]] {
        let a=positive_tetra_potential(ordinary,point,48).unwrap();let b=positive_tetra_potential_duffy(ordinary,point,48).unwrap();
        assert!((a-b).abs()/b<1e-6,"line={a:e} old Duffy={b:e}");
    }
    for width in [1e-4,1e-7] {
        let length=1e-3;let height=1e-4;
        let prism=Prism::new("independent-prism".into(),[[0.,0.],[length,0.],[0.,width]],[0.,height]).unwrap();
        let source=PointPotential::new(&prism).unwrap();
        let mesh=mesh::extrude(&LayeredInput{xy_m:prism.triangle_m.to_vec(),triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,height],materials:vec![vec![Material::Copper{source_ids:vec!["partition".into()]}]]},budget()).unwrap();
        for point in [[0.2*length,0.2*width,0.4*height],[0.99*length,0.005*width,0.5*height],[0.5*length,-2.*width,0.5*height],[2.*length,-length,height]] {
            let reference=source.at(point).unwrap();
            let estimate=mesh.copper_tetrahedra.iter().map(|t|positive_tetra_potential(t.nodes.map(|n|mesh.nodes_m[n]),point,63).unwrap()).sum::<f64>();
            assert!((estimate-reference).abs()/reference<1e-6,"width={width:e}, point={point:?}, line={estimate:e}, independent prism={reference:e}");
            let actual=mesh.copper_tetrahedra.iter().map(|t|PointPotential::tetrahedron(t.nodes.map(|n|mesh.nodes_m[n])).unwrap().at(point).unwrap()).sum::<f64>();
            assert!((actual-reference).abs()/reference<1e-6,"width={width:e}, point={point:?}, production={actual:e}, independent prism={reference:e}");
        }
    }
}
#[test]
fn board_sliver_self_refinement_is_resolved_without_removing_volume() {
    use pcb_analyze::extraction::local::polyhedron::tetra_self_integral;
    let tetra = [[0.0008729877471923828,0.0012507317066192628,0.],
        [0.0008386001586914063,0.001236487865447998,0.],
        [0.0008639266490936279,0.001246978521347046,0.],
        [0.0008639266490936279,0.001246978521347046,3.5e-5]];
    let values = [8,16,32,64].map(|n|tetra_self_integral(tetra,n).unwrap());
    eprintln!("source573 self8/16/32/64 = {values:?}");
    assert!((values[3]-values[2]).abs()/values[3]<1e-10);
    // Frozen70-digit facet arithmetic, with exact authored binary64 inputs.
    // This checks numerical evaluation; the independent ordinary source/outer
    // integral and prism partition tests establish the separate physical gate.
    let reference=1.65534986777049442644785510683262e-35;
    assert!((values[3]-reference).abs()/reference<1e-10);
    for a in 0..4 {for b in 0..4 {for c in 0..4 {for d in 0..4 {
        if [a,b,c,d].iter().copied().collect::<std::collections::BTreeSet<_>>().len()!=4 {continue;}
        let reordered=[tetra[a],tetra[b],tetra[c],tetra[d]];
        let translated=reordered.map(|p|std::array::from_fn(|k|p[k]-tetra[0][k]));
        for t in [reordered,translated] {
            let low=tetra_self_integral(t,16).unwrap();let high=tetra_self_integral(t,32).unwrap();
            assert!((high-low).abs()/high<1e-10,"permutation={a}/{b}/{c}/{d} low={low:e} high={high:e}");
            assert!((high-reference).abs()/reference<1e-10,"permutation={a}/{b}/{c}/{d} self={high:e}");
        }
    }}}}
}
#[test]
fn reduced_self_integral_matches_analytic_source_outer_duffy_and_scales_exactly() {
    use pcb_analyze::extraction::local::polyhedron::{tetra_self_integral,tetra_integral};
    for tetra in [
        [[0.,0.,0.],[1e-3,0.,0.],[0.,1e-3,0.],[0.,0.,1e-3]],
        [[0.,0.,0.],[1e-3,0.,0.],[1e-3,1e-3,0.],[1e-3,1e-3,1e-3]],
    ] {
        let source=PointPotential::tetrahedron(tetra).unwrap();
        let reference=tetra_integral(tetra,&source,32).unwrap();
        let low=tetra_self_integral(tetra,16).unwrap();let high=tetra_self_integral(tetra,32).unwrap();
        assert!((high-reference).abs()/reference<1e-6,"self={high:e}, outer source={reference:e}");
        assert!((high-low).abs()/high<1e-10);
        let scaled=tetra.map(|p|p.map(|x|2.*x));
        assert!((tetra_self_integral(scaled,32).unwrap()/high-32.).abs()<1e-11);
    }
}
#[test]
fn mixed_volume_elimination_is_independent_3d_p1_and_joule_identity() {
    let mesh=mesh::extrude(&LayeredInput{xy_m:vec![[0.,0.],[0.002,0.],[0.,0.001]],triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,0.0003],materials:vec![vec![Material::Copper{source_ids:vec!["actual-foil-barrel".into()]}]]},budget()).unwrap();
    assert_eq!(mesh.copper_tetrahedra.len(),3);
    let space=volume_current::assemble(&mesh,&vec![58e6;3],&[],budget()).unwrap();
    let e=[23.,-31.,7.];
    let volts:Vec<_>=space.mesh_nodes.iter().map(|nodes| {let p=mesh.nodes_m[nodes[0]];(0..3).map(|k|p[k]*e[k]).sum::<f64>()}).collect();
    let mut currents=space.gradient(&volts).unwrap();
    for (i,c) in space.cells.iter().enumerate() {for axis in 0..3 {currents[3*i+axis]/=c.resistance_ohm;}}
    for density in space.density(&currents).unwrap() {for k in 0..3 {assert!((density[k]+58e6*e[k]).abs()/(58e6*e[k]).abs()<1e-12);}}
    let losses:f64=space.losses(&currents).unwrap().iter().sum();
    let expected=58e6*mesh.copper_volume_m3*e.iter().map(|v|v*v).sum::<f64>();
    assert!((losses-expected).abs()/expected<1e-12);
    let div=space.divergence(&currents).unwrap();
    let work:f64=volts.iter().zip(&div).map(|(v,i)|v*i).sum();
    assert!((work-expected).abs()/expected<1e-12);
    assert!(div.iter().sum::<f64>().abs()/div.iter().map(|v|v.abs()).sum::<f64>()<1e-14);
    assert_eq!(space.components.len(),1);
}
#[test]
fn finite_annular_barrel_dc_matches_area_reference_and_axial_flux() {
    let sectors=32;let radii=[0.15e-3,0.175e-3];let length=1e-3;let sigma=58e6;
    let xy_m:Vec<_>=radii.iter().flat_map(|r|(0..sectors).map(move |i|{let a=2.*std::f64::consts::PI*i as f64/sectors as f64;[r*a.cos(),r*a.sin()]})).collect();
    let mut triangles=vec![];let mut boundary_edges=vec![];
    for i in 0..sectors {let j=(i+1)%sectors;triangles.extend([[i,sectors+i,sectors+j],[i,sectors+j,j]]);boundary_edges.extend([[i,j],[sectors+i,sectors+j]]);}
    let count=triangles.len();
    let input=LayeredInput{xy_m,triangles,boundary_edges,z_m:vec![0.,length/2.,length],materials:vec![vec![Material::Copper{source_ids:vec!["finite-plating".into()]};count];2]};
    let mesh=mesh::extrude(&input,budget()).unwrap();
    let contacts:Vec<_>=[0.,length].iter().map(|&z|mesh.nodes_m.iter().enumerate().filter(|(_,p)|p[2]==z).map(|(i,_)|i).collect::<Vec<_>>()).collect();
    let space=volume_current::assemble(&mesh,&vec![sigma;mesh.copper_tetrahedra.len()],&contacts,budget()).unwrap();
    let port_dofs=contacts.iter().map(|c|space.node_potential[&c[0]]).collect::<Vec<_>>();
    let factor=space.dc_factor(&port_dofs,budget()).unwrap();
    let mut boundary=vec![0.;space.potential_count()];boundary[port_dofs[1]]=1.;
    let volts=factor.solve(&space,&boundary,&vec![0.;space.potential_count()]).unwrap();
    let mut currents=space.gradient(&volts).unwrap();
    for (i,c) in space.cells.iter().enumerate() {for k in 0..3 {currents[3*i+k]/=c.resistance_ohm;}}
    let reaction=space.divergence(&currents).unwrap();
    // Independent exact polygon annulus area, no field/kernel production values.
    let area=sectors as f64/2.*(2.*std::f64::consts::PI/sectors as f64).sin()*(radii[1]*radii[1]-radii[0]*radii[0]);
    let expected=sigma*area/length;
    assert!((reaction[port_dofs[1]]-expected).abs()/expected<1e-9);
    assert!((reaction[port_dofs[0]]+expected).abs()/expected<1e-9);
    let loss:f64=space.losses(&currents).unwrap().iter().sum();
    assert!((loss-expected).abs()/expected<1e-9);
    for d in space.density(&currents).unwrap() {assert!(d[0].abs()/(sigma/length)<1e-9);assert!(d[1].abs()/(sigma/length)<1e-9);assert!((d[2]+sigma/length).abs()/(sigma/length)<1e-9);}
}
