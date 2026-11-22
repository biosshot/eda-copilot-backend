//! Small joint discrete-model control; no physical-accuracy acceptance claim.
use super::*;
use super::super::{mesh::{self,LayeredInput,Material},volume_current,volume_field,volume_reduction};
use crate::electrical::{coupled,Counters};

#[test]
fn split_top_bottom_references_carry_joint_ac_displacement() {
    let start=Instant::now(); let wall=Duration::from_secs(25);
    let budget=AllocationBudget{max_bytes:256*1024*1024,max_nodes:1000,max_tetrahedra:1000};
    let xy:Vec<_>=(0..4).flat_map(|y|(0..2).map(move |x|[x as f64*0.001,y as f64*0.001])).collect();
    let triangles:Vec<_>=(0..3).flat_map(|y| {let a=2*y; [[a,a+1,a+3],[a,a+3,a+2]]}).collect();
    let edges=vec![[0,1],[6,7],[0,2],[2,4],[4,6],[1,3],[3,5],[5,7]];
    let dielectric=||Material::Dielectric{label:"air".into(),epsilon_r:1.};
    let mut layers=vec![vec![dielectric();triangles.len()];7];
    for (slab,tri,name) in [(1,0,"bottom-left"),(1,4,"bottom-right"),(3,2,"signal"),(5,0,"top-left"),(5,4,"top-right")] {
        layers[slab][tri]=Material::Copper{source_ids:vec![name.into()]};
    }
    let zs=vec![0.,0.001,0.0011,0.002,0.0021,0.003,0.0031,0.004];
    let mesh=mesh::extrude(&LayeredInput{xy_m:xy.clone(),triangles:triangles.clone(),boundary_edges:edges,z_m:zs,materials:layers},budget).unwrap();
    let face=|triangle:usize,z:f64| -> Vec<usize> {
        let nodes=triangles[triangle].map(|i|xy[i]);
        mesh.nodes_m.iter().enumerate().filter_map(|(i,p)|
            (p[2]==z && nodes.iter().any(|v|v[0]==p[0]&&v[1]==p[1])).then_some(i)).collect()
    };
    let electrodes=vec![face(2,0.002),face(2,0.0021),face(0,0.001),face(4,0.001),face(0,0.003),face(4,0.003)];
    assert!(electrodes.iter().all(|v|v.len()==3));
    let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&electrodes,budget).unwrap();
    assert_eq!(space.components.len(),5);
    let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,n)|(format!("e{i}"),space.node_potential[&n[0]])).collect();
    let component=|i:usize| space.components.iter().position(|c|c.contains(&contacts[i].1)).unwrap();
    assert_eq!(component(0),component(1));
    let distinct:std::collections::BTreeSet<_>=[0,2,3,4,5].into_iter().map(component).collect();
    assert_eq!(distinct.len(),5);
    let basis=volume_reduction::harmonic(&space,&contacts,budget).unwrap();
    let field=volume_field::Field::new(&mesh,&space,budget).unwrap();
    let signal_cells:Vec<_>=space.cells.iter().enumerate().filter_map(|(i,c)|
        (mesh.copper_tetrahedra[c.mesh_cell].source_ids==["signal"]).then_some(i)).collect();
    assert_eq!(signal_cells.len(),3);
    let group=Group{prism:Prism::new("signal".into(),triangles[2].map(|i|xy[i]),[0.002,0.0021]).unwrap(),cells:signal_cells};
    let ports=[[0,2],[1,4]];
    let model=model(&mesh,&space,&basis,&[group],&field,&ports,12,budget,wall).unwrap();
    let n=model.potential_nodes.len()+model.current_dofs.len();
    let mut unloaded=coupled::System::compile(&model,2,None).unwrap();
    let rhs=unloaded.port_rhs(&[1.,0.]);
    assert!(unloaded.solve(0.,rhs,&vec![0.;n],&mut Counters::default()).unwrap_err().starts_with("COUPLED_DC_INCOMPATIBLE"));
    let mut system=coupled::System::compile(&model,2,None).unwrap();
    for port in 0..2 {system.add_port_admittance(port,1./50.,0.).unwrap();}
    let mut low_frequency_current=[0.;2];
    for hz in [1000.,10000.] {for driven in 0..2 {
        let w=std::f64::consts::TAU*hz;let mut drive=[0.,0.];drive[driven]=1.;
        let mut rhs=system.port_rhs(&drive);rhs.resize(2*n,0.);
        let state=system.solve_charge_coordinates(w,&rhs,&mut Counters::default()).unwrap();
        let re=system.port_voltages(&state[..n],2);let im=system.port_voltages(&state[n..],2);
        let mut net_power=50./4.;
        for k in 0..2 {let sr=2.*re[k]/50.-if k==driven {1.}else{0.};let si=2.*im[k]/50.;net_power-=(sr*sr+si*si)*50./4.;}
        let loss=system.joule_loss(&state[..n])+system.joule_loss(&state[n..]);
        assert!(loss.is_finite() && loss>=0. && (net_power-loss).abs()<=1e-12+1e-8*(net_power.abs()+loss.abs()));
        let nr=[drive[0]-re[0]/50.,drive[1]-re[1]/50.];let ni=[-im[0]/50.,-im[1]/50.];
        let conduction=nr[driven].hypot(ni[driven]);
        assert!(conduction>1e-12,"driven displacement path vanished");
        assert!(re[1-driven].hypot(im[1-driven])>1e-14,"cross-port response vanished");
        if hz==1000. {low_frequency_current[driven]=conduction;} else {
            // Millimetre air gaps and 50 ohm terminations make omega*R*C
            // small here; this checks the nonzero capacitive response trend.
            let ratio=conduction/low_frequency_current[driven];
            assert!((5. ..15.).contains(&ratio),"displacement frequency ratio={ratio:e}");
        }
        for (island,cols) in basis.component_columns.iter().enumerate() {
            let mut qr=0.;let mut qi=0.;
            for &(i,j,c) in &model.capacitance_f {if cols.contains(&i) {qr+=c*state[j];qi+=c*state[n+j];}}
            let mut ir=0.;let mut ii=0.;
            for k in 0..2 {let sign=if island==component(ports[k][0]) {1.}else if island==component(ports[k][1]) {-1.}else{0.};ir+=sign*nr[k];ii+=sign*ni[k];}
            let defect=(-w*qi-ir).hypot(w*qr-ii);
            assert!(defect<=1e-12+1e-8*(ir.hypot(ii)+w*qr.hypot(qi)),"island={island} hz={hz} drive={driven} defect={defect:e}");
        }
    }}
    assert!(start.elapsed()<wall);
}
