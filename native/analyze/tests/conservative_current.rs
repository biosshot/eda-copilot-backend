use pcb_analyze::extraction::local::{conservative_current::ConservativeSpace, mesh::{self,LayeredInput,Material}, volume_current, AllocationBudget};

fn budget()->AllocationBudget {AllocationBudget{max_bytes:64*1024*1024,max_nodes:10000,max_tetrahedra:10000}}
fn fixture()->(mesh::LayeredMesh,Vec<Vec<usize>>) {
    let mesh=mesh::extrude(&LayeredInput{
        xy_m:vec![[0.,0.],[1e-3,0.],[1e-3,0.5e-3],[0.,0.5e-3]],
        triangles:vec![[0,1,2],[0,2,3]],
        boundary_edges:vec![[0,1],[1,2],[2,3],[3,0]],
        z_m:vec![0.,0.2e-3],
        materials:vec![vec![Material::Copper{source_ids:vec!["unchanged-copper".into()]};2]],
    },budget()).unwrap();
    assert_eq!(mesh.copper_tetrahedra.len(),6);
    let electrodes=[0.,1e-3].map(|x|mesh.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[0]==x).then_some(i)).collect::<Vec<_>>());
    (mesh,electrodes.into_iter().collect())
}
fn currents(space:&volume_current::Space,mesh:&mesh::LayeredMesh,electric_field:[f64;3],sigma:f64)->Vec<f64> {
    let volts:Vec<_>=space.mesh_nodes.iter().map(|nodes|{let p=mesh.nodes_m[nodes[0]];-(0..3).map(|k|electric_field[k]*p[k]).sum::<f64>()}).collect();
    let mut current=space.gradient(&volts).unwrap();
    for (i,c) in space.cells.iter().enumerate(){for axis in 0..3{current[3*i+axis]/=c.resistance_ohm;}}
    assert_eq!(space.divergence(&current).unwrap().iter().sum::<f64>().abs(),0.);
    assert!(sigma>0.);
    current
}
fn density_at(mesh:&mesh::LayeredMesh,space:&volume_current::Space,conservative:&ConservativeSpace,flux:&[f64],cell:usize,point:[f64;3])->[f64;3] {
    let tetra=&mesh.copper_tetrahedra[cell];let mut out=[0.;3];
    for (face_index,face) in conservative.faces.iter().enumerate(){for incidence in face.cells.iter().filter(|c|c.cell==cell){
        let opposite=space.cells[cell].potentials[incidence.opposite];
        let mesh_node=mesh.copper_tetrahedra[cell].nodes[incidence.opposite];let p=mesh.nodes_m[mesh_node];
        let _=opposite;
        for axis in 0..3{out[axis]+=incidence.sign*flux[face_index]*(point[axis]-p[axis])/(3.*tetra.volume_m3);}
    }}
    out
}
fn close(a:f64,b:f64){assert!((a-b).abs()<=1e-12+1e-9*b.abs(),"actual={a:e} expected={b:e}");}

#[test]
fn constant_current_preserves_reactions_electrodes_insulation_and_known_section_flux(){
    let (mesh,electrodes)=fixture();let sigma=58e6;let space=volume_current::assemble(&mesh,&vec![sigma;6],&electrodes,budget()).unwrap();
    let conservative=ConservativeSpace::new(&mesh,&space,&electrodes,budget()).unwrap();
    let current=currents(&space,&mesh,[1e-6,0.,0.],sigma);let flux=conservative.recover(&space,&current,true).unwrap();
    let reactions=space.divergence(&current).unwrap();
    let expected=sigma*1e-6*0.5e-3*0.2e-3;
    let left=space.node_potential[&electrodes[0][0]];let right=space.node_potential[&electrodes[1][0]];
    close(reactions[left],expected);close(reactions[right],-expected);
    for (index,face) in conservative.faces.iter().enumerate(){if face.cells.len()==1&&!face.electrode{assert_eq!(flux[index],0.);}}
    for cell in 0..6{let point=[0.371e-3,0.173e-3,0.09e-3];let density=density_at(&mesh,&space,&conservative,&flux,cell,point);for axis in 0..3{close(density[axis],sigma*[1e-6,0.,0.][axis]);}}
    assert!(conservative.cell_divergence(&space,&flux).unwrap().iter().all(|v|v.abs()<1e-15));
}

#[test]
fn affine_rt0_basis_has_exact_oriented_face_and_section_integrals(){
    let (mesh,electrodes)=fixture();let space=volume_current::assemble(&mesh,&vec![58e6;6],&electrodes,budget()).unwrap();let conservative=ConservativeSpace::new(&mesh,&space,&electrodes,budget()).unwrap();
    let a=[2.3,-1.7,0.4];let c=3100.;let field=|p:[f64;3]|->[f64;3]{std::array::from_fn(|k|a[k]+c*p[k])};
    let flux:Vec<f64>=conservative.faces.iter().map(|face|{
        let p=face.nodes.map(|n|mesh.nodes_m[n]);let u:[f64;3]=std::array::from_fn(|k|p[1][k]-p[0][k]);let v:[f64;3]=std::array::from_fn(|k|p[2][k]-p[0][k]);
        let normal=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];let area=(0..3).map(|k|normal[k]*normal[k]).sum::<f64>().sqrt()/2.;let center:[f64;3]=std::array::from_fn(|k|(p[0][k]+p[1][k]+p[2][k])/3.);let j:[f64;3]=field(center);area*(0..3).map(|k|normal[k]*j[k]).sum::<f64>()/(2.*area)
    }).collect();
    for cell in 0..6{for point in [[0.11e-3,0.07e-3,0.04e-3],[0.83e-3,0.31e-3,0.16e-3]]{let actual=density_at(&mesh,&space,&conservative,&flux,cell,point);let expected=field(point);for axis in 0..3{close(actual[axis],expected[axis]);}}}
}

#[test]
fn shared_faces_complex_states_and_determinism_preserve_original_nodal_reactions(){
    let (mesh,electrodes)=fixture();let space=volume_current::assemble(&mesh,&vec![58e6;6],&electrodes,budget()).unwrap();let conservative=ConservativeSpace::new(&mesh,&space,&electrodes,budget()).unwrap();
    let real=currents(&space,&mesh,[1e-6,2e-7,-1e-7],58e6);let imaginary=currents(&space,&mesh,[-3e-7,1e-7,2e-7],58e6);
    for current in [&real,&imaginary]{let target=space.divergence(current).unwrap();let flux=conservative.recover(&space,current,false).unwrap();let again=conservative.recover(&space,current,false).unwrap();assert_eq!(flux.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),again.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
        let mut actual=vec![0.;space.potential_count()];for(face_index,face)in conservative.faces.iter().enumerate(){for incidence in &face.cells{actual[space.cells[incidence.cell].potentials[incidence.opposite]]+=incidence.sign*flux[face_index]/3.;if face.cells.len()==2{let other=face.cells.iter().find(|c|c.cell!=incidence.cell).unwrap();close(incidence.sign*flux[face_index],-other.sign*flux[face_index]);}}}
        for node in 0..actual.len(){close(actual[node],target[node]);}assert!(conservative.cell_divergence(&space,&flux).unwrap().iter().all(|v|v.abs()<1e-10));
    }
}

#[test]
fn dc_omits_only_cell_star_implied_non_electrode_reactions(){
    let mesh=mesh::extrude(&LayeredInput{
        xy_m:vec![[0.,0.],[1e-3,0.],[1e-3,0.5e-3],[0.,0.5e-3],[0.5e-3,0.25e-3]],
        triangles:vec![[0,1,4],[1,2,4],[2,3,4],[3,0,4]],
        boundary_edges:vec![[0,1],[1,2],[2,3],[3,0]],
        z_m:vec![0.,0.2e-3],
        materials:vec![vec![Material::Copper{source_ids:vec!["cell-star-regression".into()]};4]],
    },budget()).unwrap();
    let electrodes=[0.,1e-3].map(|x|mesh.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[0]==x).then_some(i)).collect::<Vec<_>>());
    let electrodes:Vec<_>=electrodes.into_iter().collect();
    let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&electrodes,budget()).unwrap();
    let electrode_dofs:std::collections::BTreeSet<_>=electrodes.iter().map(|nodes|space.node_potential[&nodes[0]]).collect();
    let omitted:Vec<_>=(0..space.potential_count()).filter(|dof|!electrode_dofs.contains(dof)).collect();
    assert_eq!(omitted.len(),2);
    let conservative=ConservativeSpace::new(&mesh,&space,&electrodes,budget()).unwrap();
    let current=currents(&space,&mesh,[1e-6,0.,0.],58e6);let target=space.divergence(&current).unwrap();
    let flux=conservative.recover(&space,&current,true).unwrap();
    let mut actual=vec![0.;space.potential_count()];
    for(face_index,face)in conservative.faces.iter().enumerate(){for incidence in &face.cells{actual[space.cells[incidence.cell].potentials[incidence.opposite]]+=incidence.sign*flux[face_index]/3.;}}
    for dof in omitted{close(actual[dof],target[dof]);}
    assert!(conservative.cell_divergence(&space,&flux).unwrap().iter().all(|value|value.abs()<1e-12));
}

#[test]
fn recovery_refuses_nonfinite_range_and_never_changes_mesh_identity(){
    let (mesh,electrodes)=fixture();let nodes:Vec<_>=mesh.nodes_m.iter().map(|p|p.map(f64::to_bits)).collect();let cells:Vec<_>=mesh.copper_tetrahedra.iter().map(|c|c.nodes).collect();let space=volume_current::assemble(&mesh,&vec![58e6;6],&electrodes,budget()).unwrap();let conservative=ConservativeSpace::new(&mesh,&space,&electrodes,budget()).unwrap();
    assert!(conservative.recover(&space,&vec![f64::NAN;space.current_count()],true).unwrap_err().contains("CONSERVATIVE_INPUT"));
    assert!(conservative.recover(&space,&vec![1e308;space.current_count()],true).is_err());
    assert!(conservative.recover(&space,&vec![0.;space.current_count()-1],true).is_err());
    assert_eq!(nodes,mesh.nodes_m.iter().map(|p|p.map(f64::to_bits)).collect::<Vec<_>>());assert_eq!(cells,mesh.copper_tetrahedra.iter().map(|c|c.nodes).collect::<Vec<_>>());
}
