//! Fixed coupled Gram projection and directed-reciprocity control for streaming reduction.
use pcb_analyze::extraction::local::{volume_current::{Space,Cell},volume_reduction::Basis,volume_field::ElectricApply,volume_magnetic::MagneticApply,AllocationBudget};
struct Field;
impl ElectricApply for Field{
 fn apply(&self,v:&[f64])->Result<Vec<f64>,String>{Ok(vec![v[0]-v[1],v[1]-v[0]])}
 fn energy_gram(&self,_:&[Vec<f64>])->Result<Vec<(usize,usize,f64)>,String>{Ok(vec![(0,0,1.),(0,1,-1.),(1,0,-1.),(1,1,1.)])}
 fn planned_bytes(&self)->usize{0}
}
struct Magnetic{skew:f64}
impl MagneticApply for Magnetic{
 fn apply(&self,_:&Space,q:&[f64])->Result<Vec<f64>,String>{Ok(vec![4.*q[0]+(1.+self.skew)*q[1]+0.5*q[2],q[0]+3.*q[1]+0.25*q[2],0.5*q[0]+0.25*q[1]+2.*q[2]])}
 fn planned_bytes(&self)->usize{0}
}
#[test]
fn streaming_projection_preserves_coupled_energy_and_directed_reciprocity_refusal(){
 let space=Space{mesh_nodes:vec![vec![0],vec![1]],node_potential:Default::default(),cells:vec![Cell{mesh_cell:0,potentials:[0,1,0,1],incidence:[[1.,0.,0.],[-1.,0.,0.],[0.;3],[0.;3]],density_per_amp_m2:1.,resistance_ohm:1.}],components:vec![vec![0,1]],planned_bytes:0};
 let basis:Basis=serde_json::from_value(serde_json::json!({"voltage":[[1.,0.],[0.,1.]],"current":[[1.,2.,0.],[-1.,0.,3.]],"names":["p","n"],"component_columns":[[0,1]],"harmonic":[],"electrode_dofs":[],"bubbles":[]})).unwrap();
 let budget=AllocationBudget{max_bytes:1024*1024,max_nodes:100,max_tetrahedra:100};
 let model=basis.model(&space,&Field,&Magnetic{skew:0.},&[[0,1]],budget).unwrap();
 assert_eq!(model.inductance_h,vec![(0,0,20.),(0,1,-3.),(1,0,-3.),(1,1,19.)]);
 assert!(basis.model(&space,&Field,&Magnetic{skew:1e-6},&[[0,1]],budget).err().unwrap().contains("MAGNETIC_RECIPROCITY"));
}

#[test]
fn trained_bubbles_change_voltage_coordinates_but_not_cardinal_harmonic_electrodes(){
 use pcb_analyze::extraction::local::{mesh::{self,LayeredInput,Material},volume_current,volume_reduction,volume_solve};
 let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:10000,max_tetrahedra:10000};
 let mesh=mesh::extrude(&LayeredInput{
  xy_m:vec![[0.,0.],[1e-3,0.],[1e-3,0.5e-3],[0.,0.5e-3],[0.5e-3,0.25e-3]],
  triangles:vec![[0,1,4],[1,2,4],[2,3,4],[3,0,4]],boundary_edges:vec![[0,1],[1,2],[2,3],[3,0]],z_m:vec![0.,0.2e-3],
  materials:vec![vec![Material::Copper{source_ids:vec!["harmonic-source-regression".into()]};4]],
 },budget).unwrap();
 let electrodes:Vec<_>=[0.,1e-3].map(|x|mesh.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[0]==x).then_some(i)).collect::<Vec<_>>()).into();
 let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&electrodes,budget).unwrap();
 let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,nodes)|(format!("electrode-{i}"),space.node_potential[&nodes[0]])).collect();
 let mut basis=volume_reduction::harmonic(&space,&contacts,budget).unwrap();let original=basis.harmonic_electrodes().unwrap().to_vec();assert_eq!(original.len(),2);
 let electrode_dofs:std::collections::BTreeSet<_>=contacts.iter().map(|(_,dof)|*dof).collect();let interior:Vec<_>=(0..space.potential_count()).filter(|dof|!electrode_dofs.contains(dof)).collect();assert_eq!(interior.len(),2);
 let mut shape=vec![0.;space.potential_count()];shape[interior[0]]=2.3;shape[interior[1]]=-1.1;
 let solution=volume_solve::Solution{real_shape_v:shape,imaginary_shape_v:vec![0.;space.potential_count()],real_shape_correction_v:vec![0.;space.potential_count()],imaginary_shape_correction_v:vec![0.;space.potential_count()],real_constants_v:vec![0.;space.components.len()],imaginary_constants_v:vec![0.;space.components.len()],real_currents_a:vec![0.;space.current_count()],imaginary_currents_a:vec![0.;space.current_count()],audit:volume_solve::Audit{iterations:0,scaled_relative_residual:0.,normalized_kcl:0.,normalized_kvl:0.,relative_complex_power_error:0.,relative_joule_power_error:0.}};
 let added=basis.include_solution(&space,&solution,budget).unwrap();assert!(added.0>0);assert_ne!(basis.voltage[0],original[0]);assert_eq!(basis.harmonic_electrodes().unwrap(),original);
 for (e,_) in contacts.iter().enumerate(){for (other,(_,dof)) in contacts.iter().enumerate(){assert_eq!(basis.harmonic_electrodes().unwrap()[e][*dof],if e==other{1.}else{0.});}}
}
