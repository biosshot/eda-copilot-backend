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
